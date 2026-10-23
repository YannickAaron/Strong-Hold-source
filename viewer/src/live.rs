//! `--live save.sav`: run the Rust sim (../sim) instead of a recording. Each tick yields the same 21×i16 draw records
//! a recording carries, so the viewer's replay path renders it unchanged; the HUD gets a `HudState` from sim memory.
//!
//! Each tick is DLL_RunTick (`shc_sim::frontend::run_tick`): mouse hover and clicks, chores, the sim step, the draw
//! pass (records) and PlayStateReturnData (HudState). `SHC_TILES_ONLY=1`: tile records straight from the tile
//! graphic layer instead (no units / trees / anims).
use crate::hud::HudState;
use crate::replay::Rec;

/// Minimap texture size in pixels: GameMap.getMaxRadarSize of an 800 map (tilemapSize − 4), the game's "zoomed out"
/// radar = whole map. Smaller maps render at their own size (`Live::radar_n`) and are scaled up to this.
pub const RADAR_N: i32 = 796;

/// Placement footprint side of an eMappers id: FUN_180069400 (jump table at 0x1800694c0; same as sim setup::f69400,
/// which is private). -1 = no footprint (not placeable as a building).
#[cfg_attr(not(feature = "live"), allow(dead_code))]
pub fn footprint(mapper: i32) -> i32 {
    const IDX: [u8; 412] = *include_bytes!("../../formats/data/aiv_size_jumptable.bin");
    const SIDE: [i32; 15] = [2, 4, 3, 5, 2, 6, 1, 7, 11, 9, 10, 13, 0, 8, -1];
    let k = mapper.wrapping_sub(0x2c) as u32;
    if k >= 0x19c { -1 } else { SIDE[IDX[k as usize] as usize] }
}

/// The sim's verdict for the build tool at a cursor tile: footprint top-left x, y and side, ok, and `why`
/// (placement reason at world +0x204e704, or one of the negative codes below). Drag tools (walls, moat, pitch):
/// `tiles` = the sim's preview layer (x, y, buildable) instead of a footprint.
#[derive(Clone, Debug)]
pub struct Verdict { pub tool: i32, pub x: i32, pub y: i32, pub size: i32, pub ok: bool, pub why: i32, pub tiles: Vec<(i32, i32, bool)> }
pub const NOT_PORTED: i32 = -3; // mapaction has no path for this tool
pub const BROKE: i32 = -2; // valid spot but no chore queued: FUN_1800cb630 said the player can't afford it
pub const NO_STONE: i32 = -4; // wall tools: FUN_1800ce850 = 0 pieces affordable (the release queues nothing)
pub const PAUSED: i32 = -5; // pause latch set (GameAction 1048): mapaction refuses tools and clicks
pub const NEED: i32 = -10; // NEED - n: FUN_1800cb630 would refuse, "… needed" = TEXT_FEEDBACK_{n+1} (`Live::afford`)

impl Verdict {
    /// A tool the sim can't run: red ghost of the tool's footprint at the cursor.
    pub fn unported(tool: i32, x: i32, y: i32) -> Self {
        let size = footprint(tool).max(1);
        Verdict { tool, x: x - size / 2, y: y - size / 2, size, ok: false, why: NOT_PORTED, tiles: vec![] }
    }

    /// Text key (or plain text) for `why`. Reasons are TEXT_PANEL_FEEDBACK indices (lookUpText(77, n) = `_{n+1}`):
    /// 3 stockpile, 0x11 enemy, 0x12 keep distance, 0x15 signpost, 0x18 high ground, ...
    pub fn text(&self) -> Option<String> {
        match self.why {
            _ if self.ok => None,
            NOT_PORTED => Some("Not ported in the sim yet".into()),
            BROKE => Some("Not enough resources".into()),
            NO_STONE => Some("TEXT_FEEDBACK_018".into()), // "Not enough stone m'lord" (f93370's message 6)
            PAUSED => Some("TEXT_FEEDBACK_023".into()), // "Game Paused"
            0 => Some("Can't build here".into()), // ponytail: no reason code (taken / blocked tile); the game shows none
            0x3e8 => Some("Too many buildings".into()),
            w @ 1..=24 => Some(format!("TEXT_PANEL_FEEDBACK_{:03}", w + 1)),
            w @ -16..=-11 => Some(format!("TEXT_FEEDBACK_{:03}", NEED - w + 1)),
            _ => None,
        }
    }
}

#[cfg(feature = "live")]
pub use sim::Live;

/// Tools driven by press / drag / release (walls, stairs, moat dig / undig, pitch ditch): `mapaction::is_drag`.
#[cfg(feature = "live")]
pub fn is_drag(tool: i32) -> bool { shc_sim::mapaction::is_drag(tool) }
#[cfg(not(feature = "live"))]
pub fn is_drag(_: i32) -> bool { false }

#[cfg(feature = "live")]
mod sim {
    use super::*;
    use crate::replay::ROTATION;
    use shc_sim::{commands::Command, core, game as g, mapaction as ma, mem::Mem, placement, terrain as t};
    const PAUSE: u64 = 0x183668088;

    pub struct Live {
        m: Mem,
        last: Vec<Rec>, // last tile record sent per tile index (dirty-only output, like the engine's hash cache)
        tiles: Vec<(i16, i16)>, // (x, y) of every valid map tile, by tile index (x = -1: none)
        t0: std::time::Instant,  // engine clock (timeGetTime) for the draw pass's animation clocks
        sent: bool,              // first draw frame done
        preview: Vec<usize>,     // tile indices of the drag tool preview the sim drew last (mapaction::PREVIEW)
        pub view: Option<[i32; 4]>, // draw-pass camera (left, right, top, bottom); None = whole map
        pub mouse: [i32; 2],        // map tile under the cursor (-1 = off the map), DLL_RunTick mouse_x / mouse_y
        pub centre: [i32; 4],       // DLL_RunTick centre tile x, y and tiles wide / high (radar camera)
        pub pos: Option<[i32; 2]>,  // DLL_RunTick screenCentrePosX / Y (CameraControls2D.getScreenCentreTileXY): the radar rectangle's centre
        pub flattened: bool,        // DLL_RunTick flattenedLandscape (EngineInterface.flattenedLandscape: right-click menu down)
        ret: Vec<u8>,               // the last PlayStateReturnData run_tick filled
        sel: Vec<u8>,               // the last selection, (id, type) i32 pairs
        held: Option<(i32, i32, i32, i32)>, // build tool input to repeat before each tick: (tool, mouse state 0 / 2, x, y)
        pub troop: crate::military::Troop,  // DLL_TroopSelection input, handed over before each DLL_RunTick
        idx: Vec<i32>,                      // the last radar's tile index per pixel pair (GameAction 0x404 clicks)
        radar_n: i32,                       // this map's radar size (GameMap.getMaxRadarSize)
        pub me: i32,                        // the local player: 1, or this peer's in a LAN game (mp::register_user)
        pub net: Option<crate::lan::Lockstep>, // LAN game: the lockstep around each RunTick
        ev: crate::hud::Events,             // the engine's UI events so far (message bar, objectives), for the HUD
        pub tap: Option<crate::game_replay::Tap>, // around every RunTick: records the game, or plays a replay (game_replay.rs)
        editor: Option<i32>,                      // the map editor's brush size while its tool input repeats (editor.rs)
        pub hill: Option<crate::koth::Hill>,      // King of the Hill (koth.rs); a replay's is its Player's
    }

    impl Live {
        /// Load a .sav (DLL_LoadSaveGame, `world::load_save`).
        pub fn load(path: &str) -> Self {
            let mut m = Mem::default();
            shc_sim::world::load_save(&mut m, &std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"))).expect("parse save");
            Self::from_mem(m)
        }

        /// Write a .sav the original can load (DLL_SaveSaveGame, `save::save_game`); play time grows by the time
        /// since this Live was made.
        pub fn save(&mut self, path: &std::path::Path) -> std::io::Result<()> {
            // on a copy, written in the background (save_game takes 1-2 s; saves.rs)
            // a LAN game's saves are .msv, like the original's multiplayer saves (the LAN lobby's Load lists those)
            let path = if self.net.is_some() { path.with_extension("msv") } else { path.to_path_buf() };
            crate::koth::Hill::write_beside(&self.hill, &path)?; // the score is not in the sim memory (koth.rs)
            crate::saves::save_async(&self.m, path, self.t0.elapsed().as_millis() as u32, |_| {});
            Ok(())
        }

        /// A Live around a sim state that is ready to tick (a loaded save or a new skirmish, `crate::skirmish`).
        pub fn from_mem(mut m: Mem) -> Self {
            let mut tiles = vec![(-1, -1); 320_800];
            for y in 0..800i32 {
                for x in 0..800i32 {
                    if m.u8(t::VALID + (y * 800 + x) as u64) != 0 { tiles[(t::row_off(&m, y) + x) as usize] = (x as i16, y as i16); }
                }
            }
            // GameMap.getMaxRadarSize: tilemapSize (0x1860ab524) − 4, at most 800; then GameAction 0x406
            // (ResizeRadarMapBuffer). Mission maps are 200 / 300 / ...: their radar is that many pixels.
            let size = m.i32(0x1860ab524);
            let radar_n = if size <= 4 { RADAR_N } else if size > 800 { 800 } else { size - 4 };
            shc_sim::hudstate::set_radar_size(&mut m, radar_n);
            let me = m.i32(ma::LOCAL).max(1);
            Self { radar_n, m, last: vec![[0; 21]; 320_800], tiles, t0: std::time::Instant::now(), sent: false, view: None, preview: vec![],
                mouse: [-1, -1], centre: [400, 400, 30, 30], pos: None, flattened: false, ret: vec![0; shc_sim::hudstate::RET_LEN], sel: vec![], held: None,
                troop: Default::default(), idx: vec![], ev: Default::default(), me, net: None, tap: Some(crate::game_replay::Tap::record()), editor: None, hill: None }
        }

        /// A new game in King of the Hill to `target` points (0 = conquest): its hill, fort and guards, koth.rs.
        pub fn with_hill(mut self, target: i32) -> Self {
            self.hill = (target > 0).then(|| crate::koth::Hill::begin(&mut self.m, target));
            self
        }

        pub fn tick(&self) -> i32 { self.m.i32(core::TICK_COUNTER) }
        pub fn mem(&mut self) -> &mut Mem { &mut self.m }
        #[cfg(test)]
        pub fn mem_ref(&self) -> &Mem { &self.m }

        /// A UI mouse event for DLL_TroopSelection (`military::Troop`).
        pub fn troop_event(&mut self, e: TroopIn) {
            let rot = self.rotation();
            self.troop.event(&self.m, rot, e);
        }

        /// Centre tile of player 1's keep (kinds 0x28..0x2c, record +0xee/+0xf0 = top-left, +0xf8 = size): where the
        /// game puts the camera after loading. State 1 = placed this tick: a new skirmish's keeps until the first tick.
        pub fn keep(&self) -> Option<(i16, i16)> {
            use shc_sim::buildings as b;
            let i = (1..b::SLOTS).find(|&i| matches!(b::state(&self.m, i), 1 | 2) && b::owner(&self.m, i) == self.me && (0x28..=0x2c).contains(&b::kind(&self.m, i)))?;
            let (r, n) = (b::rec(i), self.m.i32(b::rec(i) + 0xf8) as i16);
            Some((self.m.i16(r + 0xee) + n / 2, self.m.i16(r + 0xf0) + n / 2))
        }
        pub fn rotation(&self) -> i16 { self.m.i32(t::ROT) as i16 }
        /// The ROTATION record that leads every batch: the rotation and the players' colours (`replay::colours`).
        fn header(&self) -> Rec {
            let mut r = [0i16; 21];
            (r[0], r[1]) = (ROTATION, self.rotation());
            for (k, c) in crate::replay::colours(&self.m).into_iter().enumerate() { r[2 + k] = c as i16; }
            r
        }

        /// Records for the current state: a rotation record, then every tile that changed since the last call
        /// (everything on the first call).
        pub fn records(&mut self) -> Vec<Rec> {
            let mut out = vec![[0i16; 21]];
            if std::env::var_os("SHC_TILES_ONLY").is_none() {
                let recs = self.run_tick(false);
                out[0] = self.header(); // after the tick: a rotating tick draws in the new one
                out.extend(recs);
                return out;
            }
            out[0] = self.header();
            {
                // SHC_TILES_ONLY=1: tile records straight from the tile graphic layer (no units / trees / anims)
                let n = self.tiles.len();
                // bulk slices: per-address Mem reads cost ~12 ms per scan, slices ~1 ms
                let (gfx, hgt, light) = (self.m.slice(t::GFX, n * 4), self.m.slice(t::HEIGHT, n), self.m.slice(0x184ec6240, n)); // light = draw::tile's byte
                let htab: Vec<i32> = (0..256).map(|h| self.m.i32(t::HTAB + h * 4)).collect();
                for (i, &(x, y)) in self.tiles.iter().enumerate() {
                    if x < 0 { continue; }
                    let g = u32::from_le_bytes(gfx[i * 4..i * 4 + 4].try_into().unwrap());
                    if (g as i32) < 1 { continue; }
                    let mut r = [0i16; 21];
                    // draw::tile (FUN_1801a1d40) field layout: x, y, file, image + 1, height, light
                    (r[1], r[2], r[3], r[4], r[5], r[6]) = (x, y, (g >> 16) as i16, (g & 0xffff) as i16 + 1, htab[hgt[i] as usize] as i16, light[i] as i16);
                    if self.last[i] != r { self.last[i] = r; out.push(r); }
                }
            }
            out
        }

        /// DLL_RunTick (`frontend::run_tick`): mouse hover / clicks (building panels), chores, the sim step (`step`;
        /// else mpFrameSkip: input and drawing only), the draw pass over the camera (whole map on the first call) and
        /// PlayStateReturnData. Returns the draw records.
        fn run_tick(&mut self, step: bool) -> Vec<Rec> {
            self.repeat_input();
            let rot = self.rotation();
            self.troop.apply(&mut self.m, rot); // EditorDirector.preDLLCallActions: DLL_TroopSelection
            let first = !std::mem::replace(&mut self.sent, true);
            let [l, r, t, b] = match self.view {
                // `camera` gives the 800-tile frame; GameMap's render bounds are in the map's own (rows 0..size,
                // columns 0..size / 2) and the pass adds (800 - size) / 2 to rows, half that to columns (draw::pass)
                Some([l, r, t, b]) if !first => {
                    let s = self.m.i32(0x1860ab524); // tilemapSize; ≤ 4 = unset (from_mem's radar rule)
                    let o = if (5..=800).contains(&s) { (800 - s) / 2 } else { 0 };
                    [(l - o / 2).max(0), (r - o / 2).max(0), (t - o).max(0), (b - o).max(0)]
                }
                _ => [0, 400, 0, 800],
            };
            let mut inp = shc_sim::frontend::TickInput {
                mouse_x: self.mouse[0], mouse_y: self.mouse[1], no_step: !step,
                shift: self.troop.keys[0], ctrl: self.troop.keys[1], alt: self.troop.keys[2],
                sh1_rts: crate::menu::sh1_rts(), // Options > Controls (ConfigSettings.Settings_SH1RTSControls, default on)
                scr_left: l, scr_right: r, scr_top: t, scr_bottom: b, screen_zoom: 1, now_ms: self.t0.elapsed().as_millis() as u32,
                centre_tile_x: self.centre[0], centre_tile_y: self.centre[1], tiles_wide: self.centre[2], tiles_high: self.centre[3],
                centre_pos_x: self.pos.map_or(0, |p| p[0]), centre_pos_y: self.pos.map_or(0, |p| p[1]), flattened: self.flattened,
                radar_w: self.radar_n, radar_h: self.radar_n,
                // MyAudioManager's music state: the sim's music manager picks the next piece when one ends
                music_playing: crate::audio::music_state().0, music_about_to_loop: crate::audio::music_state().1,
                speech1: crate::audio::speech_state().0, speech2: crate::audio::speech_state().1, // C# isSpeechPlaying(1 / 2)
                bink_playing: self.ev.video.playing(), // SFXManager.isBinkPlaying (video.rs)
                ..Default::default()
            };
            if let Some(crate::game_replay::Tap::Rec(r)) = &mut self.tap { r.hill.get_or_insert_with(|| self.hill.clone()); } // a replay's start
            if let Some(tap) = &mut self.tap { if !tap.before(&mut self.m, &mut inp) { return vec![]; } } // replay over: no tick
            if let Some(n) = &mut self.net { n.before(&mut self.m); } // LAN: peers' chores in, exec-tick timing
            let out = shc_sim::frontend::run_tick(&mut self.m, &inp, &mut self.ret, None);
            if let Some(n) = &mut self.net { n.after(&mut self.m); } // LAN: own chores and the reached tick out
            if let Some(tap) = &mut self.tap { tap.after(&mut self.m, &inp); }
            // King of the Hill counts between ticks, after the tap: its end-of-game writes are recorded front-end
            // writes, which a replay plays back (it counts in its Player, koth.rs)
            crate::koth::after_tick(&mut self.hill, &mut self.m, true);
            if let Some(crate::game_replay::Tap::Rec(r)) = &mut self.tap {
                if let Some(p) = r.autosaved.take() { let _ = crate::koth::Hill::write_beside(&self.hill, &p); } // after this tick's count
            }
            self.sel = out.selected.iter().flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes())).collect();
            self.troop.track(&out.draw);
            // completeSelectionBox; not in a replay, which plays the recorded selection writes
            let play = matches!(self.tap, Some(crate::game_replay::Tap::Play(_)));
            if !play && i32::from_le_bytes(self.ret[0x3b8..0x3bc].try_into().unwrap()) > 0 { self.troop.complete_box(&self.m, rot); }
            self.ev.scan(&out.draw);
            self.ev.video.scan(&out.draw, &self.ret); // the engine's video channel (video.rs)
            // FatControler: skirmishInsultFrom (+0xafe) / skirmishInsult (+0xaff, 1..20), an AI lord's answer to an
            // insult: SFXManager.playInsult unless Options mutes insults or their speech. ponytail: no chat line (the
            // port has no skirmish chat panel, and nothing sends the player's insult (1069) yet)
            if self.ret[0xafe] > 0 && !crate::menu::opt("MuteInsults", false) && !crate::menu::opt("MuteInsultSpeech", false) {
                if let n @ 1..=20 = self.ret[0xaff] { shc_sim::sound::push(shc_sim::sound::Sound::Named(format!("Insult{n}.wav"))); }
            }
            out.draw
        }

        /// One sim tick (DLL_RunTick with the sim step); returns the records it produced.
        pub fn step(&mut self) -> Vec<Rec> {
            let recs = self.run_tick(true);
            // the rotation after the tick: run_tick resets the draw cache, the sim step turns the map (terrain::f795e0)
            // and the draw pass then sends the new rotation's records
            let mut out = vec![[0i16; 21]];
            out[0] = self.header();
            out.extend(recs);
            out
        }

        /// DLL_SetMapRotation (`setup::set_map_rotation`): rotation 0 / 2 / 4 / 6 around game tile (x, y); the next
        /// stepping tick turns the map.
        pub fn rotate(&mut self, rot: i32, x: i32, y: i32) { shc_sim::setup::set_map_rotation(&mut self.m, rot, x, y) }

        /// DLL_SetAppMode (`game::set_app_mode`): a panel the front end opens itself (EditorDirector.directSetAppSubMode).
        pub fn set_app_mode(&mut self, mode: i32, sub: i32) { shc_sim::game::set_app_mode(&mut self.m, mode, sub) }

        /// DLL_TutorialAction(id, value) (`campaign::tutorial_action`): the tutorial's front-end events, between ticks.
        pub fn tutorial(&mut self, id: i32, value: i32) { shc_sim::campaign::tutorial_action(&mut self.m, id, value) }

        /// HudState from the PlayStateReturnData the last run_tick filled (FUN_18019c910), decoded
        /// exactly like a recording's blob 1 + 2.
        pub fn hud(&mut self) -> HudState {
            let mut h = HudState::decode(&self.ret, &self.sel).unwrap_or_default();
            h.game_paused = self.m.u8(PAUSE); // = ret +2200, read now: a paused '.' step clears the latch for its tick
            h.available = (0..0x1b8).map(|id| shc_sim::hudstate::mapper_available(&self.m, id)).collect();
            h.costs = crate::hud::Table((0..0x1b8).map(|id| self.cost(id)).collect());
            h.coords = crate::hud::Table((0..0x1b8).map(|id| ma::mapper_coord(&self.m, id)).collect());
            h.end = crate::endgame::read(&self.m);
            // the game is over: its statistics for Skirmish Masters (DLL_GetMPScoreData; masters.rs records them)
            if h.end.is_some() { crate::menu::trail_menu::masters::capture(&self.m, self.t0.elapsed().as_millis() as u32); }
            h.ev = crate::hud::Events { campaign: self.m.i32(0x183663eb8) == 0, ..self.ev.clone() }; // game mode 0 = campaign
            crate::military::hud_extra(&self.m, &mut h); // local player, lords still in, game kind (allies / Free Build)
            let hill = self.hill.as_ref().or(match &self.tap { Some(crate::game_replay::Tap::Play(p)) => p.hill.as_ref(), _ => None });
            if let Some(hill) = hill {
                let names = self.net.as_ref().map_or(vec![], |n| n.status().lock().unwrap().players.clone());
                h.hill = Some(crate::koth::board(hill, &self.m, self.me, &names));
            }
            h
        }

        /// SHC_OPEN=mapper (scripted screenshots): place `mapper` near player 1's keep (`campaign::bot::place`, the
        /// original validator) and open its panel with a map click on its centre; the next tick draws the whole map.
        pub fn open_new(&mut self, mapper: i32) {
            use shc_sim::buildings as b;
            let Some((kx, ky)) = self.keep() else { return };
            let before: Vec<i32> = (1..b::SLOTS).filter(|&i| b::state(&self.m, i) != 0).collect();
            if !shc_sim::campaign::bot::place(&mut self.m, mapper, kx as i32, ky as i32) { return println!("SHC_OPEN {mapper}: not placed"); }
            self.step();
            let Some(id) = (1..b::SLOTS).find(|&i| b::state(&self.m, i) != 0 && !before.contains(&i)) else { return };
            let (r, n) = (b::rec(id), self.m.i32(b::rec(id) + 0xf8));
            self.mouse = [self.m.i16(r + 0xee) as i32 + n / 2, self.m.i16(r + 0xf0) as i32 + n / 2];
            crate::military::input(self, TroopIn::Press([0.0; 2], [0.0; 2], [0.0; 4]));
            self.step();
            crate::military::input(self, TroopIn::Release);
            self.step();
            self.sent = false;
            let h = self.hud();
            println!("SHC_OPEN {mapper}: building {id} at {:?}, panel {} / {}", self.mouse, h.app_mode, h.app_sub_mode);
        }

        /// SHC_SAY=id (scripted screenshots): the first AI lord's speech `id` 1..33 (FUN_180102bc0), as the AI posts it.
        pub fn say(&mut self, id: i32) {
            let lord = (0..0x26u64).find(|&k| self.m.i32(0x18404a8e4 + k * 0x5e4) > 0).unwrap_or(0); // 0x1e..: custom lords
            shc_sim::hudmsg::f102bc0(&mut self.m, 0x181b60e70, 2, lord as i32, id);
        }

        /// DLL_StartMapAction(mapper) for player 1 (0 = drop the tool). False when mapaction has no path for the tool
        /// or the player can't act now.
        pub fn set_tool(&mut self, mapper: i32) -> bool {
            self.m.set_i32(ma::LOCAL, self.me);
            self.clear_preview();
            if mapper == 0 || !ma::supported(mapper) { ma::start(&mut self.m, 0); return false; }
            ma::start(&mut self.m, mapper) >= 0
        }

        /// DLL_MapAction hover (mouse state 0): the original validator FUN_1800770d0 on the footprint around (x, y).
        /// The ghost (FUN_18005cfe0, inside mapaction's hover) goes to `mapaction::PREVIEW`: the building's tile
        /// graphics on a valid spot, red / green tile marks otherwise. The draw pass shows and clears it every frame,
        /// so like the C# side (DLL_MapAction hover every frame) the caller repeats this before each tick.
        pub fn hover(&mut self, tool: i32, x: i32, y: i32) -> Verdict {
            self.free_slots();
            self.clear_preview();
            ma::hover(&mut self.m, x, y, self.me);
            self.collect_preview();
            self.held = Some((tool, 0, x, y));
            self.verdict(tool, x, y)
        }

        /// Zero the preview tiles the sim drew last (mapaction never clears them) and stop repeating the input.
        pub fn clear_preview(&mut self) {
            for &i in &self.preview { self.m.set_u32(ma::PREVIEW + i as u64 * 4, 0); }
            self.preview.clear();
            self.held = None;
        }

        /// The draw pass shows and clears the preview layer every frame; the game calls DLL_MapAction every frame
        /// (hover 0 with the button up, drag 2 while held), so the last tool input is repeated before each tick.
        fn repeat_input(&mut self) {
            let Some((tool, state, x, y)) = self.held else { return };
            if self.m.i32(ma::TOOL) != tool && !(ma::is_wall(tool) && ma::is_wall(self.m.i32(ma::TOOL))) { self.held = None; return; }
            for &i in &self.preview { self.m.set_u32(ma::PREVIEW + i as u64 * 4, 0); }
            self.free_slots();
            match self.editor {
                // the editor: its brush marks / the building ghost again (hover; a wall line held: drag; the strokes
                // come from editor.rs)
                Some(b) => { ma::map_action(&mut self.m, tool, x, y, b, self.me, false, true, state); }
                None if state == 0 => { ma::hover(&mut self.m, x, y, self.me); }
                None => { ma::drag(&mut self.m, x, y, self.me); }
            }
            self.collect_preview();
        }

        /// The map editor's DLL_MapAction(tool, x, y, brush, editor player, inGame, constructing, state)
        /// (EditorDirector.preDLLCallActions): the brush marks / building ghost (repeated before each tick like a
        /// hover; a wall line held repeats its drag), on press / drag / release the tool's chore.
        pub fn editor_input(&mut self, e: EdIn) {
            for &i in &self.preview { self.m.set_u32(ma::PREVIEW + i as u64 * 4, 0); }
            self.free_slots();
            ma::map_action(&mut self.m, e.tool, e.x, e.y, e.brush, self.me, e.game, e.cons, e.state);
            self.collect_preview();
            let rep = if ma::is_wall(e.tool) && matches!(e.state, 1 | 2) { 2 } else { 0 };
            (self.held, self.editor) = (Some((e.tool, rep, e.x, e.y)), Some(e.brush));
            if e.game || e.state == 3 { self.held = None; } // delete / release: nothing to repeat
        }

        /// DLL_SetEditorPlayer(p) (EditorDirector.SetEditorPlayerID): the editor places for player p from now on.
        pub fn editor_player(&mut self, p: i32) {
            shc_sim::editor::set_editor_player(&mut self.m, p);
            self.me = p;
        }

        /// What the editor HUD shows of the map (editor.rs `Info`); None when not editing.
        pub fn editor_info(&self) -> Option<crate::menu::editor::Info> {
            let m = &self.m;
            (m.i32(shc_sim::terrain::GAME_MODE) == 1).then(|| crate::menu::editor::Info {
                goods: std::array::from_fn(|g| m.i32(0x1832c5748 + g as u64 * 4)),
                keeps: std::array::from_fn(|p| m.i32(0x18379e630 + p as u64 * 0x583c) != 0),
                mp: m.i32(0x183666170) != 0,
                map_type: m.i32(0x1808e9fbc),
                player: self.me,
            })
        }

        /// DLL_SaveSaveGame with mapSave (shc_sim::editor::save): the edited map as a .map file (its folder made).
        pub fn save_map(&mut self, path: &std::path::Path) -> std::io::Result<()> {
            let c = [self.centre[0], self.centre[1], -1, -1];
            let bytes = shc_sim::editor::save(&mut self.m, c, false, false, self.t0.elapsed().as_millis() as u32);
            if let Some(d) = path.parent() { std::fs::create_dir_all(d)?; }
            std::fs::write(path, bytes)
        }

        /// Remember which tiles carry a preview now.
        fn collect_preview(&mut self) {
            let layer = self.m.slice(ma::PREVIEW, self.tiles.len() * 4);
            self.preview = (0..self.tiles.len()).filter(|&i| layer[i * 4..i * 4 + 4] != [0; 4] && self.tiles[i].0 >= 0).collect();
        }

        /// DLL_MapAction press (mouse state 1): queues the place chore (runs next tick) only if the spot is valid and
        /// affordable. Returns the verdict and the chores queued.
        pub fn click(&mut self, tool: i32, x: i32, y: i32) -> (Verdict, usize) {
            self.free_slots();
            let n = ma::click(&mut self.m, x, y, self.me).len();
            let v = self.verdict(tool, x, y);
            (if v.ok && n == 0 { Verdict { ok: false, why: BROKE, ..v } } else { v }, n)
        }

        /// FUN_18008ff30's building limit reads DAT_1864c9bf4 (free building slots, buildings OBJ+0x54), which
        /// `buildings::tick` sets; a save is 0 there until the first tick, so a tool used while paused right after
        /// loading would read "too many buildings". ponytail: filled like the tick does (4000 − live) only while 0.
        fn free_slots(&mut self) {
            use shc_sim::buildings as b;
            if self.m.i32(0x1864c9bf4) == 0 {
                let free = (1..b::SLOTS).filter(|&i| b::state(&self.m, i) == 0).count() as i32;
                self.m.set_i32(0x1864c9bf4, free);
            }
        }

        /// A valid spot the player can't pay for says why (the click would queue nothing).
        fn verdict(&self, tool: i32, x: i32, y: i32) -> Verdict {
            let size = ma::footprint(self.m.i32(ma::TOOL)).max(1);
            let r = self.m.i32(placement::RESULT);
            let need = if r == 0 { self.afford(self.m.i32(ma::TOOL)) } else { None };
            let why = match (r, need) { (0, Some(n)) => NEED - n, (0, None) => -1, _ => self.m.i32(placement::REASON) };
            Verdict { tool, x: x - size / 2, y: y - size / 2, size, ok: r == 0 && need.is_none(), why, tiles: vec![] }
        }

        /// Wood, stone, iron, pitch, gold the local player pays for `mapper` (the table FUN_1800cb630 reads,
        /// 0x1867e2b7c + kind × 0x14 = GameData.getStructureCosts' game_data_txt); 0 when it is free (first
        /// woodcutter / granary, FUN_1800b8920) or not a building.
        pub fn cost(&self, mapper: i32) -> [i32; 5] {
            let kind = shc_sim::buildings::mapper_kind(mapper);
            if kind <= 0 || self.free(kind) { return [0; 5]; }
            let c = 0x1867e2b7c + kind as u64 * 0x14;
            [0, 4, 8, 0xc, 0x10].map(|o| self.m.i32(c + o))
        }

        /// FUN_1800b8920 (private `buildings::fb8920`): free placement for the local player.
        fn free(&self, kind: i32) -> bool {
            use shc_sim::buildings::{MODE, MP, PSTRIDE};
            let (m, me) = (&self.m, 1u64);
            let mode = m.i32(MODE);
            if mode == 6 { return false; }
            if (m.i32(MP) != 0 || (mode == 2 && m.i32(0x183666170) == 0)) && kind == 0x1a { return true; }
            let yards = m.i32(0x183799238 + me * PSTRIDE);
            match kind {
                3 => m.u8(0x1867e53d8 + me) == 0 && yards < m.i32(0x1867e2bb8),
                0x13 => m.u8(0x1867e53e1 + me) == 0 && m.u8(0x1867e53d8 + me) == 0 && yards < m.i32(0x1867e2cf8),
                _ => false,
            }
        }

        /// FUN_1800cb630 for the local human in single player, read-only: the original's UI calls pass sound = true,
        /// which posts the "… needed" message and steps RNG a (`commands::fcb630` is pub(crate)), and its AI want
        /// bookkeeping never applies to a human. None = affordable, Some(n) = TEXT_FEEDBACK_{n+1}: 1 iron, 2 pitch,
        /// 3 gold, 4 wood and stone, 5 wood (fcb630's messages); 6 stone (fcb630 refuses silently there).
        pub fn afford(&self, mapper: i32) -> Option<i32> {
            let m = &self.m;
            if m.i32(0x183663f58) != 0 || m.i32(shc_sim::buildings::MODE) == 1 || (0x3c..=0x3e).contains(&mapper) { return None; }
            let kind = shc_sim::buildings::mapper_kind(mapper);
            let [wood, stone, iron, pitch, gold] = self.cost(mapper);
            let have = |o: u64| m.i32(0x18366a1a0 + shc_sim::buildings::PSTRIDE + o);
            if have(0x12f0a8) < iron { return Some(1); }
            if have(0x12f0ac) < pitch { return if kind != 0x44 || m.i16(0x18366a1a0 + shc_sim::buildings::PSTRIDE + 0x130e3c) == 0 { Some(2) } else { None }; }
            if have(0x12f0cc) < gold { return Some(3); }
            if have(0x12f098) < wood { return Some(if stone <= have(0x12f0a0) { 5 } else { 4 }); }
            (stone > have(0x12f0a0)).then_some(6)
        }

        /// Drag tool input: DLL_MapAction mouse state 0 hover / 1 press / 2 drag / 3 release at (x, y). The sim writes
        /// its preview into `mapaction::PREVIEW` but never clears it: the tiles shown last time are zeroed first.
        /// Returns the verdict with the preview tiles and the chores queued (walls: on release; moat / pitch: per tile).
        pub fn drag_input(&mut self, tool: i32, state: i32, x: i32, y: i32) -> (Verdict, usize) {
            self.free_slots();
            self.clear_preview();
            // after a press the line is held at its end tile; after a release the tool hovers again
            self.held = Some((tool, if state == 1 || state == 2 { 2 } else { 0 }, x, y));
            let n = match state {
                0 => { ma::hover(&mut self.m, x, y, self.me); 0 }
                1 => ma::press(&mut self.m, x, y, self.me).len(),
                2 => ma::drag(&mut self.m, x, y, self.me).len(),
                _ => ma::release(&mut self.m, x, y, self.me).len(),
            };
            self.collect_preview();
            let layer = self.m.slice(ma::PREVIEW, self.tiles.len() * 4);
            // red: walls 0x60013..0x60015, moat brush 0x3e (blocked)
            let tiles: Vec<(i32, i32, bool)> = self.preview.iter().map(|&i| {
                let v = u32::from_le_bytes(layer[i * 4..i * 4 + 4].try_into().unwrap());
                (self.tiles[i].0 as i32, self.tiles[i].1 as i32, !matches!(v, 0x60013..=0x60015 | 0x3e))
            }).collect();
            // walls: the release quits with "not enough stone" when FUN_1800ce850 (2 pieces per stone) is 0 although
            // the line itself is fine. ponytail: player 1's stone (GAME + P + 0x12f0a0) read directly, as fce850 is
            // pub(crate); free-build modes (always 100 pieces) not special-cased
            let broke = ma::is_wall(tool) && self.m.i32(0x18366a1a0 + shc_sim::buildings::PSTRIDE + 0x12f0a0) == 0;
            let bad = broke || if ma::is_wall(tool) { self.m.i32(ma::LINE_BAD) != 0 } else { tiles.is_empty() || tiles.iter().any(|t| !t.2) };
            let why = if broke { NO_STONE } else if bad { self.m.i32(placement::REASON) } else { -1 };
            (Verdict { tool, x, y, size: 1, ok: !bad, why, tiles }, n)
        }

        /// Minimap (FUN_1800f4bb0, `hudstate::radar`) at this map's size, scaled (nearest) to RADAR_N² ARGB, bottom
        /// row first. `view` = game tile at the window centre + tiles wide / high (DLL_RunTick's screen_centre_pos /
        /// screen_tiles); it only moves the white camera rectangle at this zoom. Needs this tick's `draw::run_tick`.
        pub fn radar(&mut self, view: [i32; 4]) -> Vec<u32> {
            let n = self.radar_n as usize;
            let (mut out, mut idx) = (vec![0u32; n * n + 2 * n], vec![0i32; n * n + 2 * n]);
            let [x, y] = self.pos.unwrap_or([view[0], view[1]]); // no screen position sent (tests): the centre tile
            shc_sim::hudstate::radar(&self.m, &mut out, &mut idx, [x, y, view[2], view[3], self.radar_n, self.radar_n]);
            self.idx = idx;
            let big = RADAR_N as usize;
            if n == big { out.truncate(n * n); return out; }
            (0..big * big).map(|k| out[(k / big) * n / big * n + (k % big) * n / big]).collect()
        }

        /// GameAction RadarClicked (1028 → FUN_1800f6090, not ported: it reads the front end's radar index map):
        /// the game tile under radar pixel (x, y) (top-down, as displayed) of the last `radar` call.
        pub fn radar_tile(&self, x: i32, y: i32) -> Option<(i32, i32)> {
            if !(0..RADAR_N).contains(&x) || !(0..RADAR_N).contains(&y) { return None; }
            let n = self.radar_n;
            let (x, y) = (x * n / RADAR_N, y * n / RADAR_N); // this map's radar pixel
            let t = *self.idx.get((y * n + x / 2) as usize)?;
            let &(tx, ty) = self.tiles.get(usize::try_from(t).ok()?)?;
            (t > 0 && tx >= 0).then_some((tx as i32, ty as i32))
        }

        /// DLL_GameAction(action, s, v, 0, 0) as player 1 (`frontend::game_action`, byte-identical to the DLL).
        pub fn game_action(&mut self, action: i32, s: i32, v: i32) -> i32 { self.game_action4(action, s, v, 0) }
        /// The same with a third argument (Ally_Orders' enemy, Ally_Send / RequestGoods' amount).
        pub fn game_action4(&mut self, action: i32, s: i32, v: i32, c: i32) -> i32 {
            self.m.set_i32(ma::LOCAL, self.me);
            shc_sim::frontend::game_action(&mut self.m, action, s, v, c, 0)
        }

        /// The panel's Delete button: the delete tool (mapaction::DELETE, FUN_18008fa90) pressed and released on
        /// the building's centre tile, with the cursor-pick globals saying "no wall / moat mark / unit here".
        /// Returns the chores queued (0x1d on success; it runs at the next tick).
        pub fn delete_building(&mut self, id: i32) -> usize {
            use shc_sim::buildings as b;
            if id <= 0 || b::state(&self.m, id) == 0 { return 0; }
            let (r, n) = (b::rec(id), self.m.i32(b::rec(id) + 0xf8));
            let (x, y) = (self.m.i16(r + 0xee) as i32 + n / 2, self.m.i16(r + 0xf0) as i32 + n / 2);
            self.free_slots();
            self.m.set_i32(ma::LOCAL, self.me);
            if ma::start(&mut self.m, ma::DELETE) < 0 { return 0; }
            for (a, v) in [(0x183a0fdac, 0), (0x183a0fd94, 0), (0x183a0fd9c, 0), (0x183a0fd80, 0)] { self.m.set_i32(a, v); }
            let n = ma::press(&mut self.m, x, y, self.me).len() + ma::release(&mut self.m, x, y, self.me).len();
            ma::start(&mut self.m, 0);
            n
        }

        /// GameAction Game_Paused (1048, `frontend::game_action` 0x418): the latch FUN_1800cd070 checks. Set, a tick
        /// skips the scheduler and everything after the terrain step (no units, buildings, AI); mapaction refuses
        /// tools, clicks and building panels. KeyManager's P sends (1, 1) / (0, 0).
        pub fn set_paused(&mut self, on: bool) { self.game_action(1048, on as i32, on as i32); }

        pub fn command(&mut self, c: Command) { c.apply(&mut self.m, self.me); }
        pub fn tax(&self) -> i32 { self.m.i32(g::player(self.me as u32) + g::P_TAX_LEVEL) }
        pub fn rations(&self) -> i32 { self.m.i32(g::player(self.me as u32) + g::P_RATIONS) }
        // LAN game: through the chores like the original (SetTaxRate 1004 / SetRationing 1005), not applied at once
        pub fn set_tax(&mut self, v: i32) { if self.net.is_some() { self.game_action(1004, 0, v.clamp(0, 11)); } else { self.command(Command::Tax(v.clamp(0, 11) as u8)) } }
        pub fn set_rations(&mut self, v: i32) { if self.net.is_some() { self.game_action(1005, 0, v.clamp(0, 4)); } else { self.command(Command::Rations(v.clamp(0, 4) as u8)) } }
        /// LAN game: every peer far enough for the next RunTick (lan::Lockstep::ready); always in single player.
        pub fn net_ready(&mut self) -> bool {
            let Some(n) = &mut self.net else { return true };
            n.rules = crate::koth::Hill::bytes(&self.hill); // the host's, for a hold's Go
            let ok = n.ready(&mut self.m);
            if let Some(b) = n.rules_in.take() { self.hill = crate::koth::Hill::from_bytes(&b); } // with the host's state
            ok
        }
        pub fn in_mp(&self) -> bool { self.net.is_some() }
    }

    /// Build tool through mapaction: hover validates (corner of the map refused), a click on a valid spot queues one
    /// place chore that runs at the next DLL_RunTick, and the spot is then refused.
    #[test]
    fn places_via_mapaction() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.records();
        let n = |l: &Live| (1..shc_sim::buildings::SLOTS).filter(|&i| shc_sim::buildings::state(&l.m, i) != 0).count();
        let before = n(&l);
        assert!(l.set_tool(54)); // hovel
        let (kx, ky) = l.keep().map(|(x, y)| (x as i32, y as i32)).unwrap();
        let far = l.hover(54, 5, 5);
        assert!(!far.ok, "hovel at the map corner: {far:?}");
        let (x, y) = (-30..=30).flat_map(|d| (-30..=30).map(move |e| (kx + d, ky + e))).find(|&(x, y)| l.hover(54, x, y).ok).expect("no valid hovel spot near the keep");
        println!("first valid hovel spot {x},{y} (keep {kx},{ky})");
        let (v, chores) = l.click(54, x, y);
        assert!(v.ok && chores == 1, "{v:?} {chores}");
        let t0 = l.tick();
        let mut k = 0;
        while n(&l) == before && k < 20 { l.step(); k += 1; } // the chore runs at its exec tick
        println!("placed after {k} ticks (from tick {t0})");
        assert_eq!(n(&l), before + 1, "a building slot was taken");
        let j = (t::row_off(&l.m, y) + x) as u64;
        assert_ne!(l.m.i16(t::BLD + j * 2), 0, "no building on the clicked tile");
        let v = l.hover(54, x, y);
        assert!(!v.ok, "same spot again: {v:?}");
        println!("occupied: {v:?} {:?}", v.text());
    }

    /// Campaign mission 1 is a 200-tile map: the radar is GameMap.getMaxRadarSize = 196 px (scaled up for the HUD) and
    /// filled, not a 200-px corner of an 800-px picture; a click on its far corner still finds a tile; the castle tab
    /// has locked (unavailable) buttons the HUD keeps on screen.
    #[test]
    fn mission_radar_size() {
        let Ok(m) = crate::skirmish::start_mission(1, 1) else { return }; // needs the game's Maps folder
        let mut l = Live::from_mem(m);
        l.records();
        l.step();
        assert_eq!(l.radar_n, 196);
        let r = l.radar([100, 100, 30, 30]);
        let lit = r.iter().filter(|&&c| c != 0).count();
        println!("radar {} px, {lit} of {} lit; corner click -> {:?}", l.radar_n, r.len(), l.radar_tile(RADAR_N - 4, RADAR_N - 4));
        assert_eq!(r.len(), (RADAR_N * RADAR_N) as usize);
        assert!(lit * 10 > r.len() * 9, "radar mostly black");
        assert!(l.radar_tile(RADAR_N / 2, 6).is_some(), "no tile under the radar's top");
        let h = l.hud();
        let locked = (0..h.available.len()).filter(|&i| !h.available[i]).count();
        println!("mission 1: {locked} mappers locked");
        assert!(locked > 0);
    }

    /// Hover with a building the player can't pay for: the verdict says what is missing ("Wood needed") before any
    /// click, without the side effects of FUN_1800cb630's message path (RNG a and the message queue unchanged).
    #[test]
    fn hover_cost_check() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.step();
        assert!(l.set_tool(54)); // hovel: wood
        let (kx, ky) = l.keep().map(|(x, y)| (x as i32, y as i32)).unwrap();
        let (x, y) = (-30..=30).flat_map(|d| (-30..=30).map(move |e| (kx + d, ky + e))).find(|&(x, y)| l.hover(54, x, y).ok).expect("no valid hovel spot");
        println!("hovel costs {:?}", l.cost(54));
        assert!(l.cost(54)[0] > 0, "hovel costs no wood");
        let wood = 0x18366a1a0 + shc_sim::buildings::PSTRIDE + 0x12f098;
        l.m.set_i32(wood, 0);
        let (rng, msg) = (l.m.i32(0x1885676c0), l.m.slice(0x181b617d0, 0x100).to_vec());
        let v = l.hover(54, x, y);
        println!("no wood: {v:?} {:?}", v.text());
        assert_eq!((v.ok, v.text().as_deref()), (false, Some("TEXT_FEEDBACK_006")), "Wood needed");
        assert_eq!((l.m.i32(0x1885676c0), l.m.slice(0x181b617d0, 0x100).to_vec()), (rng, msg), "hover had side effects");
        assert_eq!(l.hud().costs.0[54], l.cost(54));
    }

    /// Wall drag through mapaction: press, drag along 6 tiles, release queues one wall chore (0x19); the preview
    /// shows the line while dragging.
    #[test]
    fn wall_drag_via_mapaction() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.step();
        assert!(l.set_tool(25), "stone wall tool refused (no stone?)");
        let (x, y) = (379, 94);
        let (h, _) = l.drag_input(25, 0, x, y);
        println!("hover {h:?} {:?}", h.text());
        l.drag_input(25, 1, x, y);
        let mut v = None;
        for k in 1..=6 { v = Some(l.drag_input(25, 2, x, y + k).0); }
        let v = v.unwrap();
        println!("drag: ok {} why {} {} preview tiles", v.ok, v.why, v.tiles.len());
        assert!(v.tiles.len() >= 6, "preview line too short: {v:?}");
        let (r, n) = l.drag_input(25, 3, x, y + 6);
        println!("release: ok {} why {}, {n} chore(s)", r.ok, r.why);
        assert_eq!(n, 1, "release queued no wall chore");
    }

    /// Wall drag as --live runs it: DLL_RunTick ticks between hover, press, the drag steps and the release; the
    /// release must still queue the wall chore (0x19).
    #[test]
    fn wall_drag_between_ticks() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.records();
        l.step();
        assert!(l.set_tool(25));
        let (x, y) = (379, 94);
        l.mouse = [x, y];
        l.drag_input(25, 0, x, y);
        l.step();
        l.drag_input(25, 1, x, y);
        l.step();
        for k in 1..=6 { l.mouse = [x, y + k]; l.drag_input(25, 2, x, y + k); l.step(); }
        let (r, n) = l.drag_input(25, 3, x, y + 6);
        println!("release after ticks: ok {} why {}, {n} chore(s)", r.ok, r.why);
        assert!(n > 0, "wall drag released between ticks queued no chore");
    }

    /// The wall line preview survives sim ticks while the button is held: the draw pass clears the preview layer
    /// every frame, so the drag is re-applied before each tick (as DLL_MapAction runs every frame in the game) and
    /// later ticks send no tile records for the line (no flicker back to bare ground).
    #[test]
    fn wall_preview_survives_ticks() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.records();
        l.step();
        assert!(l.set_tool(25));
        let (x, y) = (379, 94);
        l.drag_input(25, 1, x, y);
        let v = l.drag_input(25, 2, x, y + 5).0;
        assert!(v.tiles.len() >= 6, "{v:?}");
        let line: Vec<(i16, i16)> = v.tiles.iter().map(|&(x, y, _)| (x as i16, y as i16)).collect(); // records: game coords
        let on_line = |r: &Vec<Rec>| r.iter().filter(|r| r[0] == 0 && line.contains(&(r[1], r[2]))).count();
        let first = l.step();
        println!("tick 1: {} line tile records of {}", on_line(&first), first.len());
        assert!(on_line(&first) > 0, "the preview was never drawn");
        for k in 2..=4 {
            let n = on_line(&l.step());
            println!("tick {k}: {n} line tile records");
            assert_eq!(n, 0, "tick {k}: the line preview changed (flicker)");
        }
        let (_, chores) = l.drag_input(25, 3, x, y + 5);
        assert_eq!(chores, 1);
    }

    /// No stone: the wall release queues nothing and the verdict says why (the --live report "ok true, 0 chores").
    #[test]
    fn wall_drag_without_stone() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.step();
        assert!(l.set_tool(25));
        l.m.set_i32(0x18366a1a0 + shc_sim::buildings::PSTRIDE + 0x12f0a0, 0); // player 1's stone
        let (x, y) = (379, 94);
        l.drag_input(25, 1, x, y);
        let d = l.drag_input(25, 2, x, y + 4).0;
        let (r, n) = l.drag_input(25, 3, x, y + 4);
        println!("no stone: drag ok {} why {}, release ok {} why {} {:?}, {n} chore(s)", d.ok, d.why, r.ok, r.why, r.text());
        assert_eq!(n, 0);
        assert_eq!((d.ok, d.why, r.ok, r.why), (false, NO_STONE, false, NO_STONE));
    }

    /// The panel's Delete: the delete tool on a stockpile queues chore 0x1d; the building is gone after a tick.
    #[test]
    fn delete_via_mapaction() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        use shc_sim::buildings as b;
        let id = (1..b::SLOTS).find(|&i| b::state(&l.m, i) == 2 && b::owner(&l.m, i) == 1 && b::kind(&l.m, i) == 0xa).unwrap();
        assert_eq!(l.delete_building(id), 1, "no delete chore");
        l.step();
        assert_ne!(b::state(&l.m, id), 2, "stockpile {id} still standing");
    }

    #[test]
    fn live_runs_crater_lake() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; } // needs the user's save
        let mut l = Live::load(&p);
        let first = l.records();
        let h0 = l.hud(); // = recording rec_1792268794 tick 1064 HUD (the save's moment)
        assert_eq!((h0.gold, h0.population, h0.housing_cap), (1588, 10, 10));
        assert!(first.len() > 100_000, "{} tile records", first.len());
        let (mut ts, mut tr, mut th) = (std::time::Duration::ZERO, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let mut sizes = vec![];
        for _ in 0..10 {
            let t = std::time::Instant::now();
            core::tick(&mut l.m, true);
            ts += t.elapsed();
            let t = std::time::Instant::now();
            sizes.push(l.records().len());
            tr += t.elapsed();
            let t = std::time::Instant::now();
            l.hud();
            th += t.elapsed();
        }
        println!("records per tick {sizes:?}; per tick: sim {:?}, draw {:?}, hud {:?}", ts / 10, tr / 10, th / 10);
        // window-sized camera (1684×784 px at 1:1 around the keep, as --live starts)
        l.view = Some(super::camera([-30000.0, -2000.0, -28316.0, -1216.0]));
        let t = std::time::Instant::now();
        let sizes: Vec<usize> = (0..10).map(|_| l.step().len()).collect();
        println!("window camera {:?}: records {sizes:?}, {:?}/tick", l.view, t.elapsed() / 10);
        // pan away and back: objects deleted while off screen must be resent
        let objs = |r: &Vec<Rec>| r.iter().filter(|r| matches!(r[0], 1 | 2 | 4) && r[3] != 0).count();
        let home = l.view;
        l.view = Some([300, 330, 600, 680]);
        let away = l.step();
        l.view = home;
        let back = l.step();
        println!("pan: away {} objects / {} records, back {} objects", objs(&away), away.len(), objs(&back));
        assert!(objs(&back) > 10, "objects not resent after panning back");
        l.view = None;
        let t = std::time::Instant::now();
        let radar = l.radar([400, 400, 30, 30]);
        let lit = radar.iter().filter(|&&c| c != 0).count();
        println!("radar {:?}, {lit} of {} px set", t.elapsed(), radar.len());
        assert!(lit > radar.len() / 4, "radar mostly empty");
        println!("after 10 ticks: population {} (recording: 10)", l.hud().population);
        for _ in 0..800 { l.step(); }
        let h1 = l.hud();
        assert_ne!((h0.month, h0.year), (h1.month, h1.year), "800 ticks = one month");
    }

    /// DLL_SetMapRotation: the next stepping tick turns the map, and that tick's records are labelled with the new
    /// rotation (its draw pass already ran in it).
    #[test]
    fn rotate_labels_new_rotation() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.step();
        let r0 = l.rotation();
        let (x, y) = l.keep().map(|(x, y)| (x as i32, y as i32)).unwrap();
        l.rotate((r0 as i32 + 2) % 8, x, y);
        let recs = l.step();
        assert_ne!(l.rotation(), r0, "the stepping tick did not rotate");
        assert_eq!((recs[0][0], recs[0][1]), (ROTATION, l.rotation()));
    }

    /// P = GameAction Game_Paused (1048, 1, 1): a latched tick advances nothing, PlayState.game_paused says so,
    /// build tools are refused; (1048, 0, 0) resumes.
    #[test]
    fn pause_latch() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.step();
        let t = l.tick();
        l.set_paused(true);
        l.step();
        assert_eq!((l.tick(), l.ret[2200], l.hud().game_paused), (t, 1, 1));
        assert!(!l.set_tool(54), "hovel tool while paused");
        l.set_paused(false);
        l.step();
        assert_eq!((l.tick(), l.ret[2200]), (t + 1, 0));
        assert!(l.set_tool(54));
    }

    /// Fast-forward ceiling: the sim thread's per-tick work (sim step + draw::run_tick over the window camera,
    /// HUD, radar every 8th tick) back to back.
    #[test]
    fn max_speed_ticks_per_s() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = Live::load(&p);
        l.records();
        l.view = Some(super::camera([-30000.0, -2000.0, -28316.0, -1216.0]));
        let z = std::time::Duration::ZERO;
        let (mut ts, mut tt, mut th, mut tr) = (z, z, z, z);
        let t = std::time::Instant::now();
        for k in 0..800 {
            let a = std::time::Instant::now();
            core::tick(&mut l.m, true); // the sim step alone, for the split
            ts += a.elapsed();
            let a = std::time::Instant::now();
            l.step();
            tt += a.elapsed();
            let a = std::time::Instant::now();
            l.hud();
            th += a.elapsed();
            let a = std::time::Instant::now();
            if k % 8 == 0 { l.radar([400, 400, 30, 30]); }
            tr += a.elapsed();
        }
        let e = t.elapsed();
        let td = tt + th + tr;
        println!("max speed: {:.0} ticks/s (per tick: sim {:?}, run_tick {:?}, hud {:?}, radar {:?}; 1600 sim steps in {e:?})",
            800.0 / td.as_secs_f64(), ts / 800, tt / 800, th / 800, tr / 800);
        // ponytail: no >= 90 assert: wall time, so parallel builds (load avg 50+) slow everything 5-10x and fail it
    }
}

/// Without the sim (`--no-default-features`): `--live` is refused at startup.
#[cfg(not(feature = "live"))]
pub struct Live { pub view: Option<[i32; 4]>, pub mouse: [i32; 2], pub centre: [i32; 4], pub pos: Option<[i32; 2]>, pub flattened: bool }
#[cfg(not(feature = "live"))]
impl Live {
    pub fn load(_: &str) -> Self { panic!("built without the `live` feature (shc-sim)") }
    pub fn tick(&self) -> i32 { 0 }
    pub fn rotation(&self) -> i16 { 6 }
    pub fn records(&mut self) -> Vec<Rec> { vec![] }
    pub fn step(&mut self) -> Vec<Rec> { vec![] }
    pub fn hud(&mut self) -> HudState { HudState::default() }
    pub fn set_tool(&mut self, _: i32) -> bool { false }
    pub fn hover(&mut self, t: i32, x: i32, y: i32) -> Verdict { Verdict::unported(t, x, y) }
    pub fn clear_preview(&mut self) {}
    pub fn click(&mut self, t: i32, x: i32, y: i32) -> (Verdict, usize) { (Verdict::unported(t, x, y), 0) }
    pub fn drag_input(&mut self, t: i32, _: i32, x: i32, y: i32) -> (Verdict, usize) { (Verdict::unported(t, x, y), 0) }
    pub fn game_action(&mut self, _: i32, _: i32, _: i32) -> i32 { 0 }
    pub fn game_action4(&mut self, _: i32, _: i32, _: i32, _: i32) -> i32 { 0 }
    pub fn set_paused(&mut self, _: bool) {}
    pub fn rotate(&mut self, _: i32, _: i32, _: i32) {}
    pub fn tutorial(&mut self, _: i32, _: i32) {}
    pub fn set_app_mode(&mut self, _: i32, _: i32) {}
    pub fn delete_building(&mut self, _: i32) -> usize { 0 }
    pub fn tax(&self) -> i32 { 0 }
    pub fn rations(&self) -> i32 { 0 }
    pub fn set_tax(&mut self, _: i32) {}
    pub fn set_rations(&mut self, _: i32) {}
    pub fn radar(&mut self, _: [i32; 4]) -> Vec<u32> { vec![] }
    pub fn keep(&self) -> Option<(i16, i16)> { None }
    pub fn save(&mut self, _: &std::path::Path) -> std::io::Result<()> { Ok(()) }
    pub fn editor_input(&mut self, _: EdIn) {}
    pub fn editor_player(&mut self, _: i32) {}
    pub fn editor_info(&self) -> Option<crate::menu::editor::Info> { None }
    pub fn save_map(&mut self, _: &std::path::Path) -> std::io::Result<()> { Ok(()) }
    pub fn radar_tile(&self, _: i32, _: i32) -> Option<(i32, i32)> { None }
    pub fn net_ready(&mut self) -> bool { true }
    pub fn in_mp(&self) -> bool { false }
}

#[test]
fn footprints() {
    assert_eq!(footprint(61), 7); // keep (sim setup test)
    assert_eq!(footprint(25), -1); // wall
}

/// Viewer world-px rect (x0, y0, x1, y1) -> draw-pass camera bounds (screen-row columns 0..400, rows 0..800).
/// Fitted on Crater Lake by resetting the draw cache and running bounded passes: column = (X + 38784) / 64,
/// row = Y / 16 + 400 (64 × 16 px per column / row). 30 rows extra below for tall buildings standing lower.
pub fn camera(r: [f32; 4]) -> [i32; 4] {
    let col = |x: f32| ((x + 38784.0) / 64.0).floor().clamp(0.0, 400.0) as i32;
    let row = |y: f32| (y / 16.0 + 400.0).floor().clamp(0.0, 800.0) as i32;
    [col(r[0]), col(r[2]) + 1, row(r[1]), (row(r[3]) + 30).min(800)]
}

/// UI → sim thread.
/// `View`: radar camera (centre tile, tiles wide / high) + the window's world-px rect (x0, y0, x1, y1) for the draw pass.
/// `Tool`: DLL_StartMapAction (0 = none); `Hover` / `Click`: DLL_MapAction mouse state 0 / 1 at a game tile;
/// `Drag(state, x, y)`: drag tools' press 1 / drag 2 / release 3; `Action`: DLL_GameAction(action, s, v) (panel
/// buttons); `Troops`: mouse input for DLL_TroopSelection; `Radar(x, y)`: minimap click at radar px (top-down), the
/// camera goes to that tile (`Frame::goto`); `Save`: write a .sav there (pause menu); `Rotate(rot, x, y)`:
/// DLL_SetMapRotation, the camera centres on game tile (x, y) once the map has turned.
pub enum Cmd { Run { playing: bool, rate: f32 }, View([i32; 4], [f32; 4]), Step, Tool(i32), Hover(i32, i32), Click(i32, i32), Drag(i32, i32, i32), Action([i32; 3]), Action4([i32; 4]), Mouse(i32, i32), Delete(i32), Tax(i32), Rations, Troops(TroopIn), Radar(i32, i32), Save(std::path::PathBuf), SaveReplay, Rotate(i32, i32, i32), Tutorial(i32, i32), ViewPos([i32; 2]), Flatten(bool), AppMode(i32, i32),
    /// The map editor (editor.rs): DLL_MapAction in mouse state 0..3 (`EdIn`), DLL_SetEditorPlayer, the map saved there.
    Editor(EdIn), EditorPlayer(i32), SaveMap(std::path::PathBuf) }

/// One map-editor DLL_MapAction: tool (eMappers), brush 1..9 (0 = a building / troop tool), mouse state 0..3, tile,
/// constructingOnly, inGameNotMapEditor (the editor's DeleteBuilding passes true).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdIn { pub tool: i32, pub brush: i32, pub state: i32, pub x: i32, pub y: i32, pub cons: bool, pub game: bool }

/// Mouse input for DLL_TroopSelection (`military::Troop`, the C# EditorDirector's part). World px = the map frame
/// the viewer draws in, screen px = window. `Press(world, screen, window world rect)`: left press on the map with no
/// build tool (the rect gives the on-screen units); `Release`: left release after such a press; `Right(down)`;
/// `Cursor(world, screen, [shift, ctrl, alt])`: every cursor move / modifier change.
pub enum TroopIn { Press([f32; 2], [f32; 2], [f32; 4]), Release, Right(bool), Cursor([f32; 2], [f32; 2], [bool; 3]) }

/// Game speed = sim ticks per second (Director.SetEngineFrameRate: engineFrameTime = 1 / speed), 10..90 in steps
/// of 5, default 40 (ConfigSettings.Default_GameSpeed). At ≤ 20 the C# timer runs 2-4× faster and only every
/// 2nd-4th DLL_RunTick steps the sim (lowFPSCompensationLevel, mpFrameSkip): still `speed` steps/s, so not mirrored.
pub const SPEED_DEFAULT: f32 = 40.0;

/// Keypad + / - (and = / -): Director.IncreaseFrameRate / DecreaseFrameRate, 10..90 by 5 like the original; single
/// player goes on past 90: 120, 160, 250, Max (speed.rs; networked MP stays at 90 there).
pub fn speed_step(s: f32, up: bool) -> f32 {
    const FAST: [f32; 5] = [90.0, 120.0, 160.0, 250.0, crate::speed::MAX];
    match FAST.iter().position(|&f| f >= s) {
        Some(i) if up && s >= 90.0 => FAST[(i + (FAST[i] == s) as usize).min(FAST.len() - 1)],
        Some(i) if !up && s > 90.0 => FAST[i - 1],
        _ if up => (s + 5.0).min(90.0),
        _ => (s - 5.0).max(10.0),
    }
}

/// Director.fixedTimer: one tick every 1 / `rate` s. After a stall fall at most 0.1 s behind instead of bursting
/// (the original queues every missed tick). `rate` = INFINITY (fast-forward): always due.
pub struct Pace(pub std::time::Instant);

impl Pace {
    pub fn due(&mut self, now: std::time::Instant, rate: f32) -> bool {
        if now < self.0 { return false; }
        self.0 = (self.0 + std::time::Duration::from_secs_f32(1.0 / rate)).max(now - std::time::Duration::from_millis(100));
        true
    }
}

#[test]
fn speed_paces_ticks() {
    use std::time::Duration;
    let t0 = std::time::Instant::now() + Duration::from_secs(1);
    let mut s = SPEED_DEFAULT;
    for up in [false, false, false, false, false, false, true] { s = speed_step(s, up); }
    assert_eq!(s, 15.0); // 40 → 10 (clamped) → 15
    let up: Vec<f32> = std::iter::successors(Some(85.0), |&s| Some(speed_step(s, true))).take(7).collect();
    assert_eq!(up, [85.0, 90.0, 120.0, 160.0, 250.0, crate::speed::MAX, crate::speed::MAX]); // single player past 90
    assert_eq!((speed_step(crate::speed::MAX, false), speed_step(120.0, false), speed_step(90.0, false)), (250.0, 90.0, 85.0));
    for (speed, want) in [(10.0, 10), (40.0, 40), (90.0, 90), (f32::INFINITY, 1000)] {
        let mut p = Pace(t0);
        // one second of 1 ms polls on a mock clock
        let n = (0..1000).filter(|&ms| p.due(t0 + Duration::from_millis(ms), speed)).count();
        assert_eq!(n, want, "speed {speed}");
    }
}

/// Sim thread → UI: one sim tick (above 90 ticks/s: every tick since the last Frame, speed.rs).
/// `radar`: RADAR_N² ARGB, bottom row first; only every 8th tick (~25 ms at 796 px). `goto`: game tile a minimap
/// click asked the camera to centre on (C# GameAction RadarClicked → cameraMovedRecalcBounds).
pub struct Frame { pub recs: Vec<Rec>, pub hud: HudState, pub tick: i32, pub rot: i16, pub radar: Option<Vec<u32>>, pub goto: Option<(i32, i32)> }

/// The sim on its own thread: a tick (sim 0.1 ms + draw pass ~24 ms over the whole map) must not block frames.
/// Commands are applied between ticks, as the engine applies chores.
/// `verdict`: the last hover / click verdict (also while paused, when no frames come).
pub struct Thread {
    tx: std::sync::mpsc::Sender<Cmd>, rx: std::sync::mpsc::Receiver<Frame>, pub tick: i32, pub rot: i16,
    verdict: std::sync::Arc<std::sync::Mutex<Option<Verdict>>>,
    pub tool: Option<i32>, pub hover_at: [i32; 2], pub mouse_at: [i32; 2], // what the UI last sent
    pub press_w: Option<[f32; 2]>, // --live troop mode: world px where the left button went down (selection box)
    pub keys: [bool; 3],           // shift, ctrl, alt held (DLL_RunTick)
    rect: std::cell::Cell<[f32; 4]>, // the window's world-px rect last sent with Cmd::View (on-screen units)
    sent: ([f32; 2], [f32; 2], [bool; 3]), // the last TroopIn::Cursor
    pub cursor: Option<Rec>,       // the sim's cursor sprite of the last frame (draw record type 5), if any
    pub replay: bool,              // playing a replay: only Run / View / Step / Radar reach the sim (game_replay.rs)
    pub stats: std::sync::Arc<std::sync::Mutex<crate::stats::History>>, // per-player values over time (stats.rs), sampled on the sim thread
    pub ed: std::sync::Arc<std::sync::Mutex<Option<crate::menu::editor::Info>>>, // the map editor's map info, after each frame
}

impl Thread {
    pub fn spawn(mut l: Live, mut playing: bool, mut rate: f32) -> Self {
        use std::{sync::mpsc, time::{Duration, Instant}};
        let (tx, crx) = mpsc::channel::<Cmd>();
        let (ftx, rx) = mpsc::channel::<Frame>();
        let (tick, rot) = (l.tick(), l.rotation());
        #[cfg(feature = "live")]
        let replay = l.tap.as_ref().is_some_and(|t| t.playing());
        #[cfg(not(feature = "live"))]
        let replay = false;
        let verdict = std::sync::Arc::new(std::sync::Mutex::new(None));
        let out = verdict.clone();
        let stats = std::sync::Arc::new(std::sync::Mutex::new(crate::stats::History::default()));
        let hist = stats.clone();
        let ed = std::sync::Arc::new(std::sync::Mutex::new(None));
        let ed_out = ed.clone();
        std::thread::spawn(move || {
            let mut tool = (0, false); // (mapper, mapaction runs it)
            rate = crate::speed::clamp(&mut l, rate);
            let mut merge = crate::speed::Merge::default(); // above 90 ticks/s: one Frame per display frame (speed.rs)
            let mut pace = Pace(Instant::now());
            let (mut view, mut rect, mut n) = ([400, 400, 30, 30], None, 0u32);
            if !playing { l.set_paused(true); }
            #[cfg(feature = "live")]
            if let Some(id) = std::env::var("SHC_SAY").ok().and_then(|v| v.parse().ok()) { l.say(id); }
            #[cfg(feature = "live")]
            if let Some(m) = std::env::var("SHC_OPEN").ok().and_then(|v| v.parse().ok()) { l.open_new(m); }
            // Rotate: the tile to centre on once the map has turned (GameData.SetCameraFromGameState after a rotate)
            let (mut rot_goto, mut last_rot) = (None, l.rotation());
            loop {
                #[cfg(feature = "live")]
                crate::audio::forward(shc_sim::sound::drain()); // the sim's sound queue is per thread: the UI thread plays it
                let mut step = false;
                let mut hover = None;
                let mut input = false; // tool / panel input this round: redraw even without a tick
                let mut goto = None;
                let set = |v: Verdict, paused: bool| *out.lock().unwrap() = Some(if paused { Verdict { ok: false, why: PAUSED, ..v } } else { v });
                loop {
                    match crx.try_recv() {
                        Ok(Cmd::Run { playing: p, rate: r }) => {
                            let r = crate::speed::clamp(&mut l, r); // networked MP: the original's 10..90
                            let p = p || l.in_mp(); // LAN: no pause (Director.SetPausedState ignores it in MP); the gate keeps peers together
                            if (p, r) != (playing, rate) { input = true; } // paused: one frame, so the HUD shows it
                            if p != playing {
                                l.set_paused(!p); // pause (P / Space / pause menu) = KeyManager's P: GameAction 1048
                                if p && tool.0 != 0 && !tool.1 { tool.1 = l.set_tool(tool.0); } // picked while paused: refused then
                            }
                            (playing, rate) = (p, r);
                        }
                        Ok(Cmd::Step) => step = true,
                        Ok(Cmd::View(v, r)) => { (view, rect) = (v, Some(r)); l.centre = v; }
                        Ok(Cmd::ViewPos(p)) => l.pos = Some(p),
                        Ok(Cmd::Flatten(f)) => { input = true; l.flattened = f }
                        Ok(Cmd::AppMode(a, s)) => { input = true; l.set_app_mode(a, s) }
                        Ok(Cmd::Tool(t)) => { tool = (t, l.set_tool(t)); *out.lock().unwrap() = None; }
                        Ok(Cmd::Hover(x, y)) => { hover = Some((x, y)); input = true; } // only the latest one counts
                        Ok(Cmd::Drag(state, x, y)) => {
                            hover = None;
                            input = true;
                            let (v, n) = l.drag_input(tool.0, state, x, y);
                            if state == 3 || n > 0 { println!("drag {state} mapper {} at tile {x},{y}: ok {} why {}, {n} chore(s)", tool.0, v.ok, v.why); }
                            set(v, !playing);
                        }
                        Ok(cmd @ (Cmd::Action(_) | Cmd::Action4(_))) => {
                            input = true;
                            let [a, s, v, c] = match cmd { Cmd::Action([a, s, v]) => [a, s, v, 0], Cmd::Action4(x) => x, _ => unreachable!() };
                            let r = l.game_action4(a, s, v, c);
                            println!("game action {a} ({s}, {v}) -> {r}");
                            // EngineInterface.GameAction: a camera key (keep, groups, bookmarks, lord, ...; every
                            // KeyFunctions code that answers > 0), CycleBookmarks / CentreMarker: y * 1000 + x to centre on
                            if r > 0 && (a < 1000 || a == 1026 || a == 1043) { goto = Some((r % 1000, r / 1000)); }
                            #[cfg(feature = "live")]
                            crate::military::after_action(l.mem(), a, r);
                        }
                        #[cfg(feature = "live")]
                        Ok(Cmd::Troops(t)) => { input = true; crate::military::input(&mut l, t) }
                        #[cfg(not(feature = "live"))]
                        Ok(Cmd::Troops(_)) => {}
                        Ok(Cmd::Radar(x, y)) => { input = true; goto = l.radar_tile(x, y); }
                        Ok(Cmd::Mouse(x, y)) => {
                            input = true;
                            l.mouse = [x, y];
                            if x < 0 { l.clear_preview(); } // over the HUD: no ghost
                        }
                        Ok(Cmd::Delete(id)) => {
                            input = true;
                            let n = l.delete_building(id);
                            println!("delete building {id}: {n} chore(s)");
                            if n > 0 { l.set_app_mode(14, 10); } // the panel's building is going: back to the build menu (FUN_18008a9e0's 0xe / 10)
                            if tool.1 { l.set_tool(tool.0); } // the delete tool replaced the build tool
                        }
                        Ok(Cmd::Click(x, y)) => {
                            hover = None;
                            let (v, n) = if tool.1 { l.click(tool.0, x, y) } else { (Verdict::unported(tool.0, x, y), 0) };
                            println!("click mapper {} at tile {x},{y}: {v:?}, {n} chore(s)", tool.0);
                            set(v, !playing);
                        }
                        Ok(Cmd::Editor(e)) => { input = true; l.editor_input(e) }
                        Ok(Cmd::EditorPlayer(p)) => { input = true; l.editor_player(p) }
                        Ok(Cmd::SaveMap(p)) => match l.save_map(&p) {
                            Ok(()) => println!("saved map {}", p.display()),
                            Err(e) => eprintln!("save map {}: {e}", p.display()),
                        },
                        Ok(Cmd::Tax(d)) => { l.set_tax(l.tax() + d); println!("tax level {}", l.tax()) }
                        Ok(Cmd::Rations) => { l.set_rations((l.rations() + 1) % 5); println!("rations {}", l.rations()) }
                        Ok(Cmd::Rotate(r, x, y)) => { l.rotate(r, x, y); rot_goto = Some((x, y)); }
                        // EngineInterface.TutorialAction: main.rs sends it while the tutorial runs (game type 4)
                        Ok(Cmd::Tutorial(id, v)) => { input = true; l.tutorial(id, v) }
                        Ok(Cmd::Save(p)) => match l.save(&p) {
                            Ok(()) => println!("saved {}", p.display()),
                            Err(e) => eprintln!("save {}: {e}", p.display()),
                        },
                        #[cfg(feature = "live")]
                        Ok(Cmd::SaveReplay) => crate::game_replay::save_replay(&mut l), // Saves/Replays (game_replay.rs)
                        #[cfg(not(feature = "live"))]
                        Ok(Cmd::SaveReplay) => {}
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                if let Some((x, y)) = hover {
                    set(match tool { (t, true) if is_drag(t) => l.drag_input(t, 0, x, y).0, (t, true) => l.hover(t, x, y), (t, false) => Verdict::unported(t, x, y) }, !playing);
                }
                if step || (playing && l.net_ready() && pace.due(Instant::now(), rate)) {
                    l.view = rect.map(camera);
                    if !playing { l.set_paused(false); } // '.' while paused: one real tick
                    let recs = l.step();
                    if !playing { l.set_paused(true); }
                    #[cfg(feature = "live")]
                    if let Some(c) = l.tap.as_mut().and_then(|t| t.follow()) { goto = Some(c); } // a replay: the recorded camera
                    #[cfg(feature = "live")]
                    crate::stats::sample(l.mem(), &mut hist.lock().unwrap());
                    let Some(recs) = merge.take(recs, rate, step || goto.is_some() || l.rotation() != last_rot) else { continue };
                    let radar = (n % 8 == 0).then(|| l.radar(view));
                    n += 1;
                    if l.rotation() != last_rot { (last_rot, goto) = (l.rotation(), goto.or(rot_goto.take())); }
                    *ed_out.lock().unwrap() = l.editor_info();
                    if ftx.send(Frame { recs, hud: HudState { speed: rate, ..l.hud() }, tick: l.tick(), rot: l.rotation(), radar, goto }).is_err() { return; }
                } else if input {
                    // paused: the draw pass (preview layer, panel state) without a sim step
                    l.view = rect.map(camera);
                    let recs = merge.take(l.records(), 0.0, true).unwrap_or_default(); // after any held ticks
                    if l.rotation() != last_rot { (last_rot, goto) = (l.rotation(), goto.or(rot_goto.take())); }
                    *ed_out.lock().unwrap() = l.editor_info();
                    if ftx.send(Frame { recs, hud: HudState { speed: rate, ..l.hud() }, tick: l.tick(), rot: l.rotation(), radar: None, goto }).is_err() { return; }
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        });
        Self { tx, rx, tick, rot, verdict, tool: None, hover_at: [i32::MIN; 2], mouse_at: [i32::MIN; 2], press_w: None,
            keys: [false; 3], rect: Default::default(), sent: ([f32::NAN; 2], [0.0; 2], [false; 3]), cursor: None, stats, ed, replay }
    }

    /// Left press on the map with no build tool at world px `w` / window px `s` (DLL_TroopSelection state 1).
    pub fn press(&mut self, w: [f32; 2], s: [f32; 2]) {
        self.press_w = Some(w);
        self.send(Cmd::Troops(TroopIn::Press(w, s, self.rect.get())));
    }

    /// Left release after `press` (state 3).
    pub fn release(&mut self) { if self.press_w.take().is_some() { self.send(Cmd::Troops(TroopIn::Release)); } }

    /// The cursor at world px `w` / window px `s` (+ the held modifiers): sent when anything changed.
    pub fn cursor_at(&mut self, w: [f32; 2], s: [f32; 2]) {
        let now = (w, s, self.keys);
        if now != self.sent { self.sent = now; self.send(Cmd::Troops(TroopIn::Cursor(w, s, self.keys))); }
    }

    pub fn verdict(&self) -> Option<Verdict> { self.verdict.lock().unwrap().clone() }

    pub fn send(&self, c: Cmd) {
        if self.replay && !matches!(c, Cmd::Run { .. } | Cmd::View(..) | Cmd::ViewPos(..) | Cmd::Flatten(..) | Cmd::Step | Cmd::Radar(..)) { return; } // a replay plays what was recorded
        if let Cmd::View(_, r) = c { self.rect.set(r); }
        let _ = self.tx.send(c);
    }

    /// Ticks finished since the last call, oldest first.
    pub fn frames(&mut self) -> Vec<Frame> {
        let v: Vec<Frame> = self.rx.try_iter().collect();
        if let Some(f) = v.last() {
            (self.tick, self.rot) = (f.tick, f.rot);
            self.cursor = f.recs.iter().rev().find(|r| r[0] == 5).copied(); // GameMap: no record = no cursor sprite
        }
        v
    }
}

#[cfg(feature = "live")]
#[test]
fn camera_probe() {
    let p = crate::paths::test_save("Crater Lake-1test.sav");
    if !std::path::Path::new(&p).exists() { return; }
    let mut l = Live::load(&p);
    l.records();
    let rot = l.rotation();
    for cam in [[100, 150, 300, 400], [200, 260, 100, 160], [0, 40, 700, 790]] {
        shc_sim::draw::reset(l.mem());
        let c = shc_sim::draw::Camera { left: cam[0], right: cam[1], top: cam[2], bottom: cam[3], zoom: 1, now_ms: 0, full: false };
        let recs = shc_sim::draw::run_tick(l.mem(), &c);
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        let mut n = 0;
        for r in recs.iter().filter(|r| r[0] == 0) {
            let (x, y) = crate::replay::to_west_pub(rot, r[1], r[2]);
            let q = crate::replay::cell_px(x, y);
            for k in 0..2 { lo[k] = lo[k].min(q[k]); hi[k] = hi[k].max(q[k]); }
            n += 1;
        }
        println!("rot {rot} cam {cam:?}: {n} tiles, world px x {:.0}..{:.0} y {:.0}..{:.0}", lo[0], hi[0], lo[1], hi[1]);
    }
}





