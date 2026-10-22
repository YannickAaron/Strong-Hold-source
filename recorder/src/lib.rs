//! Proxy for CrusaderDE.dll: forwards all 77 DLL_* exports to CrusaderDE_orig.dll
//! (same folder) and logs every call. Log format: README.md.
//!
//! ABI notes: Win64 has one calling convention, so `extern "C"` == the game's
//! `[DllImport]` default. C# `bool` has no `[MarshalAs]` in EngineInterface.cs, so Mono
//! passes a 4-byte Win32 BOOL; the native side only reads the low byte. We type every
//! bool (args and returns) as i32 so the full register value is forwarded untouched.
#![cfg(windows)]
#![allow(non_snake_case)]

mod log;

use log::{CallInfo, Rec, Section};
use std::ffi::c_void;
use std::slice;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;

unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(m: *mut c_void, name: *const u8) -> *mut c_void;
    fn GetModuleHandleExW(flags: u32, addr: *const c_void, out: *mut *mut c_void) -> i32;
    fn GetModuleFileNameW(m: *mut c_void, buf: *mut u16, len: u32) -> u32;
}

/// Bytes of `PlayStateReturnData` (EngineInterface.cs:2036, explicit layout, last field
/// `byte customisedExtremeTrail` @2835, align 4) = what `Marshal.SizeOf` allocates.
const PLAYSTATE_SIZE: usize = 2836;
/// `LoadMapReturnData` (EngineInterface.cs:11): 21 int + 16 short + 40 byte, sequential.
const LOADMAP_RET_SIZE: usize = 156;
/// `MultiplayerSetupTransferData` = 116 ints, `AILordConfigTransferData` = 377 ints.
const MP_SETUP_SIZE: usize = 464;
const LORD_CONFIG_SIZE: usize = 1508;
/// `short[14_700_000]` draw buffer = 700 000 records of 21 i16.
const MAX_RECORDS: usize = 700_000;

static TICK: AtomicU32 = AtomicU32::new(0);

struct State {
    procs: Vec<usize>,
    tx: Sender<Vec<u8>>,
    /// Save-section table read from the loaded original (README "State hashes").
    secs: Vec<Section>,
    /// Index into `secs` of the engine tick counter (section 1046), if present.
    tick_sec: Option<usize>,
    full: bool,
}

/// Sections hashed every tick: the classic-size / object arrays (type id < 3000) up to 3 MB,
/// ~20 MB. The rest (3xxx/4xxx large tile layers, 10 MB+ arrays, ~60 MB) every `BIG_PERIOD`
/// ticks, or every tick with SHC_REC_FULL=1.
const BIG_PERIOD: u32 = 10;
fn every_tick(s: &Section) -> bool {
    s.type_id < 3000 && s.size <= 3_000_000
}

/// Locate the section descriptor table in the loaded original module: 24-byte entries
/// `{u64 addr, i32, i32 size, i16, i16 type_id}`, first three type ids 1003, 1037, 1043
/// (Ghidra 0x180334ab0; the Steam DLL of 2026-10-17 has it at +0x2070). Pointers are
/// relocated by the loader, so the addresses are the live ones. Entries pointing outside
/// the image are dropped.
unsafe fn find_sections(m: *const u8) -> Vec<Section> {
    let base = m as u64;
    let pe = unsafe { (m.add(0x3c) as *const u32).read_unaligned() } as usize;
    let size_of_image = unsafe { (m.add(pe + 24 + 56) as *const u32).read_unaligned() } as u64;
    let img = unsafe { slice::from_raw_parts(m, size_of_image as usize) };
    let tid = |o: usize| i16::from_le_bytes([img[o + 0x12], img[o + 0x13]]);
    let Some(t0) = (0..img.len() - 72).step_by(8).find(|&o| tid(o) == 1003 && tid(o + 24) == 1037 && tid(o + 48) == 1043) else {
        return Vec::new();
    };
    let mut v = Vec::new();
    for k in 0..256 {
        let o = t0 + k * 24;
        let addr = u64::from_le_bytes(img[o..o + 8].try_into().unwrap());
        if addr == 0 {
            break;
        }
        let size = u32::from_le_bytes(img[o + 0xc..o + 0x10].try_into().unwrap());
        if addr >= base && addr + size as u64 <= base + size_of_image {
            v.push(Section { type_id: tid(o), addr, size });
        }
    }
    v
}

/// Blob 3 of a RunTick record: `u32 engine_tick, u32 hash_micros, n × {i16 type_id, u32 crc}`.
fn hash_state(s: &State, tick: u32, r: &mut Rec) {
    let t0 = std::time::Instant::now();
    let eng = s.tick_sec.map_or(0, |i| unsafe { (s.secs[i].addr as *const u32).read_unaligned() });
    let all = s.full || tick % BIG_PERIOD == 0;
    let mut b = Vec::with_capacity(8 + s.secs.len() * 6);
    b.extend_from_slice(&eng.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    for sec in s.secs.iter().filter(|x| all || every_tick(x)) {
        let crc = log::engine_crc(unsafe { slice::from_raw_parts(sec.addr as *const u8, sec.size as usize) });
        b.extend_from_slice(&sec.type_id.to_le_bytes());
        b.extend_from_slice(&crc.to_le_bytes());
    }
    b[4..8].copy_from_slice(&(t0.elapsed().as_micros() as u32).to_le_bytes());
    r.blob(&b);
}
static STATE: OnceLock<State> = OnceLock::new();

trait Arg {
    const SCALAR: bool;
    fn put(&self, _r: &mut Rec) {}
}
impl Arg for i32 {
    const SCALAR: bool = true;
    fn put(&self, r: &mut Rec) {
        r.i32(*self)
    }
}
impl<T> Arg for *mut T {
    const SCALAR: bool = false;
}
impl Arg for () {
    const SCALAR: bool = false;
}

/// `n` bytes at `p`, empty if null or n <= 0.
unsafe fn bytes<'a, T>(p: *const T, n: i64) -> &'a [u8] {
    if p.is_null() || n <= 0 { &[] } else { unsafe { slice::from_raw_parts(p as *const u8, n as usize) } }
}

fn wide(s: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.as_os_str().encode_wide().chain([0]).collect()
}

fn own_dir() -> std::path::PathBuf {
    let mut h = std::ptr::null_mut();
    let mut buf = [0u16; 1024];
    // FROM_ADDRESS | UNCHANGED_REFCOUNT
    unsafe {
        GetModuleHandleExW(0x4 | 0x2, own_dir as *const c_void, &mut h);
        let n = GetModuleFileNameW(h, buf.as_mut_ptr(), buf.len() as u32);
        let p = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n as usize]));
        p.parent().unwrap().to_path_buf()
    }
}

fn state() -> &'static State {
    STATE.get_or_init(|| {
        let orig = own_dir().join("CrusaderDE_orig.dll");
        let m = unsafe { LoadLibraryW(wide(&orig).as_ptr()) };
        assert!(!m.is_null(), "shc-recorder: cannot load {}", orig.display());
        let procs = CALLS
            .iter()
            .map(|c| {
                let name = format!("{}\0", c.name);
                let p = unsafe { GetProcAddress(m, name.as_ptr()) };
                assert!(!p.is_null(), "shc-recorder: {} missing in original", c.name);
                p as usize
            })
            .collect();
        let secs = unsafe { find_sections(m as *const u8) };
        let tick_sec = secs.iter().position(|s| s.type_id == 1046 && s.size == 4);

        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let file = format!("rec_{now}.shcrec");
        let home = std::env::var_os("USERPROFILE").map(|h| std::path::PathBuf::from(h).join("shc_recordings"));
        // SHC_REC_DIR, else %USERPROFILE%\shc_recordings, else next to this DLL.
        let open = std::env::var_os("SHC_REC_DIR").map(Into::into).into_iter().chain(home).chain([own_dir()]).find_map(|d| {
            let _ = std::fs::create_dir_all(&d);
            std::fs::File::create(d.join(&file)).ok()
        });
        let (tx, rx) = std::sync::mpsc::channel();
        // ponytail: if no log can be opened we still forward; records go nowhere.
        if let Some(f) = open {
            let secs = secs.clone();
            std::thread::spawn(move || log::writer_loop(f, CALLS, secs, rx));
        }
        State { procs, tx, secs, tick_sec, full: std::env::var_os("SHC_REC_FULL").is_some_and(|v| v == "1") }
    })
}

fn send(r: Rec) {
    let _ = state().tx.send(r.finish());
}

macro_rules! exports {
    ($( $id:literal fn $name:ident ( $($a:ident : $t:ty),* ) -> $r:ty $(=> |$rec:ident, $ret:ident| $extra:block)? ;)*) => {
        static CALLS: &[CallInfo] = &[$(CallInfo {
            name: stringify!($name),
            scalars: 0 $(+ <$t as Arg>::SCALAR as u8)*,
            has_ret: <$r as Arg>::SCALAR,
        }),*];

        $(
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name($($a: $t),*) -> $r {
            let s = state();
            let f: unsafe extern "C" fn($($t),*) -> $r = unsafe { std::mem::transmute(s.procs[$id]) };
            let tick = TICK.load(Ordering::Relaxed);
            let ret = unsafe { f($($a),*) };
            let mut rec = Rec::new($id, tick);
            $( Arg::put(&$a, &mut rec); )*
            Arg::put(&ret, &mut rec);
            $( { let $rec = &mut rec; let $ret = ret; #[allow(unused_unsafe)] unsafe { $extra } } )?
            send(rec);
            ret
        }
        )*
    };
}

// Ids = line order of re/managed/dll_signatures.txt. Extra blocks append blobs (u32 len + bytes)
// after the scalars; README.md lists them per call.
exports! {
    0 fn DLL_PreInitMap_Editor(map_size: i32, map_type: i32, siege_that: i32, multiplayer_map: i32, ret_data: *mut u8) -> ()
        => |r, _ret| { r.blob(bytes(ret_data, LOADMAP_RET_SIZE as i64)) };
    1 fn DLL_PreInitMap_Campaign(difficulty: i32) -> ();
    2 fn DLL_PreInitMap_EcoCampaign() -> ();
    3 fn DLL_EcoCampaign_ChangeDifficulty(difficulty: i32) -> ();
    4 fn DLL_EcoCampaign_ChangeDifficulty_briefing(difficulty: i32, ret_data: *mut i32) -> i32;
    5 fn DLL_PreInitMap_SiegeThat(difficulty: i32, player_id: i32, troop0: i32, troop1: i32, troop2: i32, troop3: i32, troop4: i32, troop5: i32, troop6: i32, troop7: i32, troop8: i32, troop9: i32, troop10: i32, advanced_mode: i32) -> ();
    6 fn DLL_PreInitMap_Invasion(difficulty: i32, restart_info_length: i32, restart_info: *mut u8) -> ()
        => |r, _ret| { r.blob(bytes(restart_info, restart_info_length as i64)) };
    7 fn DLL_PreInitMap_EcoMap(difficulty: i32) -> ();
    8 fn DLL_PreInitMap_JustBuild(advanced_freebuild: i32, freebuild_gold_level: i32, freebuild_food_level: i32, freebuild_resources_level: i32, freebuild_weapons_level: i32, freebuild_random_events: i32, freebuild_invasions: i32, freebuild_invasion_difficulty: i32, freebuild_peacetime: i32, freebuild_opponents: i32, restart_info_length: i32, remove_hostile_animals: i32, freebuild_extreme_troops: i32, freebuild_extreme_powers: i32, freebuild_defeat_on_death: i32, restart_info: *mut u8) -> ()
        => |r, _ret| { r.blob(bytes(restart_info, restart_info_length as i64)) };
    9 fn DLL_PreInitMap_Multiplayer(ret_data: *mut u8, skirmish_game: i32, restart_info_length: i32, restart_info: *mut u8, coop_trail_id: i32, coop_mission_id: i32, trail_maker_test_mode: i32, custom_trail: i32, customised_extreme_trail: i32) -> ()
        => |r, _ret| { r.blob(bytes(restart_info, restart_info_length as i64)); r.blob(bytes(ret_data, MP_SETUP_SIZE as i64)) };
    10 fn DLL_PreInitTutorial() -> ();
    11 fn DLL_ApplyMultiplayerSetupData(ret_data: *mut u8) -> ()
        => |r, _ret| { r.blob(bytes(ret_data, MP_SETUP_SIZE as i64)) };
    12 fn DLL_GetMultiplayerSetupData(ret_data: *mut u8) -> ()
        => |r, _ret| { r.blob(bytes(ret_data, MP_SETUP_SIZE as i64)) };
    13 fn DLL_RegisterMultiplayerUser(player_id: i32, name: *mut u8, name_length: i32, team: i32, local_player: i32, lord_type: i32) -> ()
        => |r, _ret| { r.blob(bytes(name, name_length as i64)) };
    14 fn DLL_RegisterSkirmishUser(player_id: i32, ailord: i32, sub_type: i32, team: i32) -> ();
    15 fn DLL_SetExtendedLordConfig(ret_data: *mut u8, player_id: i32) -> ()
        => |r, _ret| { r.blob(bytes(ret_data, LORD_CONFIG_SIZE as i64)) };
    16 fn DLL_StartMultiplayerGame(from_save: i32) -> i32;
    17 fn DLL_SetMPRandSeed(seed: i32) -> ();
    18 fn DLL_StartMultiplayerGameSynced() -> i32;
    19 fn DLL_LoadSaveGame(data: *mut u8, length: i32, ret_data: *mut u8, loading_editor_map: i32) -> i32
        => |r, _ret| { r.blob(bytes(data, length as i64)); r.blob(bytes(ret_data, LOADMAP_RET_SIZE as i64)) };
    20 fn DLL_ReceiveChore(player_id: i32, data: *mut u8, length: i32) -> i32
        => |r, _ret| { r.blob(bytes(data, length as i64)) };
    21 fn DLL_GetMultiplayerChatInfo(players: *mut i32, teams: *mut i32) -> ();
    22 fn DLL_KickMPPlayer(kick_player_id: i32, immediate: i32) -> ();
    23 fn DLL_PromoteMPHost(host_id: i32) -> ();
    24 fn DLL_TriggerMPSave(data: *mut u8, length: i32) -> i32
        => |r, _ret| { r.blob(bytes(data, length as i64)) };
    25 fn DLL_TriggerMPLoad(data: *mut u8, length: i32) -> i32
        => |r, _ret| { r.blob(bytes(data, length as i64)) };
    26 fn DLL_RemapPlayers(new_mappings: *mut i32, new_local_player: i32) -> i32;
    27 fn DLL_ConnectionPause(pause: i32) -> i32;
    28 fn DLL_SaveSaveGame(data: *mut u8, length: i32, screen_centre_x: i32, screen_centre_y: i32, real_screen_centre_x: i32, real_screen_centre_y: i32, lock_map: i32, temp_lock_only: i32, map_save: i32) -> i32
        => |r, _ret| { r.blob(bytes(data, length as i64)) };
    29 fn DLL_SetMPRadarColours(new_mappings: *mut i32) -> i32;
    30 fn DLL_LoadMapToPlay(campaign_map_id: i32, file_name: *mut u8, length: i32, ret_data: *mut u8, dummy: i32, map_name: *mut u8, maplength: i32, multiplayer_save: i32, trail_type: i32, trail_id: i32, allow_classic_bedouins: i32) -> i32
        => |r, _ret| { r.blob(bytes(file_name, length as i64)); r.blob(bytes(map_name, maplength as i64)); r.blob(bytes(ret_data, LOADMAP_RET_SIZE as i64)) };
    31 fn DLL_CreateTrailMission(file_name: *mut u8, length: i32, restart_info: *mut u8, restart_info_length: i32) -> i32
        => |r, _ret| { r.blob(bytes(file_name, length as i64)); r.blob(bytes(restart_info, restart_info_length as i64)) };
    32 fn DLL_CampaignLevel(path: *mut u8, length: i32) -> i32
        => |r, _ret| { r.blob(bytes(path, length as i64)) };
    33 fn DLL_GetColourMapping(ret_data: *mut i32, remapped_player: i32) -> ();
    34 fn DLL_GetTrailMissionLords(ret_data: *mut i32, trail: i32, trail_id: i32) -> ();
    35 fn DLL_GetTrailMissionInfo(ret_data: *mut i32, map_name: *mut u8, trail: i32, trail_id: i32) -> ();
    36 fn DLL_RunTick(data: *mut i16, radar_map: *mut u8, flattened_landscape: i32, mouse_over_x: i32, mouse_over_y: i32, shift_pressed: i32, ctrl_pressed: i32, alt_pressed: i32, ret_data: *mut u8, paused: i32, ambient_sound_channel1: i32, ambient_sound_channel2: i32, speech_sound_channel1: i32, speech_sound_channel2: i32, music_playing: i32, music_about_to_loop: i32, bink_playing: i32, screen_centre_pos_x: i32, screen_centre_pos_y: i32, screen_tiles_wide: i32, screen_tiles_high: i32, radar_map_width: i32, radar_map_height: i32, radar_zoom: i32, screen_zoom: i32, sh1_rts_controls: i32, troop_mode_mode: i32, screen_centre_tile_x: i32, screen_centre_tile_y: i32, chore_buffer: *mut u8, selected_chimps_buffer: *mut i32, mp_frame_skip: i32, building_over_depth: i32, troop_overdepth: i32, scr_bounds_left: i32, scr_bounds_right: i32, scr_bounds_top: i32, scr_bounds_bottom: i32) -> i32
        => |r, ret| {
            TICK.fetch_add(1, Ordering::Relaxed);
            // count < 0: list ends at the first record with rec[0] < 0 (GameMap.cs:893).
            let n = if ret >= 0 || data.is_null() { ret.max(0) as usize } else {
                (0..MAX_RECORDS).find(|&j| *data.add(j * 21) < 0).unwrap_or(MAX_RECORDS)
            };
            r.blob(bytes(data, n as i64 * 42));
            r.blob(bytes(ret_data, PLAYSTATE_SIZE as i64));
            // numSelectedChimps @100; buffer holds (id, type) int pairs.
            let sel = if ret_data.is_null() { 0 } else { (ret_data.add(100) as *const i32).read_unaligned().clamp(0, 10_000) };
            r.blob(bytes(selected_chimps_buffer, sel as i64 * 8));
            hash_state(state(), TICK.load(Ordering::Relaxed) - 1, r);
        };
    37 fn DLL_ImportAIV(ailord: i32, id: i32, data: *mut i16, length: i32, custom: i32) -> ()
        => |r, _ret| { r.blob(bytes(data, length as i64 * 2)) };
    38 fn DLL_SetPath(data: *mut u8, length: i32, auto_data: *mut u8, auto_length: i32, save_folder_data: *mut u8, save_folder_length: i32) -> i32
        => |r, _ret| { r.blob(bytes(data, length as i64)); r.blob(bytes(auto_data, auto_length as i64)); r.blob(bytes(save_folder_data, save_folder_length as i64)) };
    39 fn DLL_Unpack(source: *mut u8, dest: *mut u8, dest_buffer_length: i32) -> i32;
    40 fn DLL_Pack(source: *mut u8, dest: *mut u8, source_length: i32) -> i32;
    41 fn DLL_GetunpackSize(source: *mut u8) -> i32;
    42 fn DLL_UnpackRadarToARGB(source: *mut u8, dest: *mut u8) -> i32;
    43 fn DLL_CRC(source: *mut u8, size: i32) -> i32;
    44 fn DLL_CRCS(source: *mut i16, size: i32) -> i32;
    45 fn DLL_GetSaveRadar(dest: *mut u8, keeps: *mut i32) -> i32;
    46 fn DLL_SetMapRotation(rotation: i32, centre_x: i32, centre_y: i32) -> i32;
    47 fn DLL_StartMapAction(action: i32) -> i32;
    48 fn DLL_MapAction(action: i32, map_x: i32, map_y: i32, brush_size: i32, player_id: i32, in_game_not_map_editor: i32, constructing_only: i32, mouse_state: i32) -> i32;
    49 fn DLL_GameAction(action: i32, structure_id: i32, value: i32, value2: i32) -> i32;
    50 fn DLL_TroopSelection(mouse_state: i32, right_down: i32, right_up: i32, count: i32, selected_chimps: *mut i32, selection_on: i32, selection_established: i32, under_cursor_count: i32, under_cursor_chimps: *mut i32, mouse_pos_x: i32, mouse_pos_y: i32, over_top_half: i32, on_screen_count: i32, on_screen_chimps: *mut i32) -> i32
        => |r, _ret| {
            r.blob(bytes(selected_chimps, count as i64 * 4));
            r.blob(bytes(under_cursor_chimps, under_cursor_count as i64 * 4));
            r.blob(bytes(on_screen_chimps, on_screen_count as i64 * 4));
        };
    51 fn DLL_TroopSelectionChanged(count: i32, selected_chimps: *mut i32) -> i32
        => |r, _ret| { r.blob(bytes(selected_chimps, count as i64 * 4)) };
    52 fn DLL_GetMapperSize(action: i32) -> i32;
    53 fn DLL_IsMapperAvailable(mapper: i32) -> i32;
    54 fn DLL_GetMapperCoord(mapper: i32) -> i32;
    55 fn DLL_SetAchValues(food: i32, wood: i32, weapons: i32) -> ();
    56 fn DLL_ImportTrailTimes(trail_type: i32, times: *mut i32, handle_exceptions: i32) -> i32;
    57 fn DLL_SetEditorPlayer(player_id: i32) -> ();
    58 fn DLL_SetUTF8MissionText(text: *mut u8, length: i32) -> ()
        => |r, _ret| { r.blob(bytes(text, length as i64)) };
    59 fn DLL_SetUTF8MapName(text: *mut u8, length: i32, text2: *mut u8, length2: i32) -> ()
        => |r, _ret| { r.blob(bytes(text, length as i64)); r.blob(bytes(text2, length2 as i64)) };
    60 fn DLL_GetScenarioOverview(ret_data: *mut u8) -> ();
    61 fn DLL_CreateScenarioAction(action: i32) -> i32;
    62 fn DLL_GetScenarioEvent(event_id: i32, ret_data: *mut u8) -> i32;
    63 fn DLL_GetScenarioInvasion(event_id: i32, ret_data: *mut u8) -> i32;
    64 fn DLL_ApplyScenarioEvent(event_id: i32, ret_data: *mut u8) -> i32;
    65 fn DLL_ApplyScenarioInvasion(event_id: i32, ret_data: *mut u8) -> i32;
    66 fn DLL_DeleteScenarioAction(event_id: i32) -> ();
    67 fn DLL_UpdateScenarioActionDate(entry_id: i32, year: i32, month: i32) -> ();
    68 fn DLL_SetMapEditorParam(spmpmode: i32, game_type: i32, koth: i32, map_size: i32) -> i32;
    69 fn DLL_SetAppMode(app_mode: i32, app_sub_mode: i32) -> ();
    70 fn DLL_TutorialAction(id: i32, value: i32) -> ();
    71 fn DLL_GetMeritData(ret_data: *mut i32) -> ();
    72 fn DLL_GetScoreData(ret_data: *mut u8) -> ();
    73 fn DLL_GetMPScoreData(ret_data: *mut u8) -> ();
    74 fn DLL_SetDebugMode(action: i32) -> i32;
    75 fn DLL_GetLayerDebug(x: i32, y: i32, ret_data: *mut u8) -> i32;
    76 fn DLL_GetLayerData(layed_id: i32, ret_data: *mut u8) -> i32;
}
