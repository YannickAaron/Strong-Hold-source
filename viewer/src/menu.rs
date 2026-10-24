//! Front end: main menu, the save list, the skirmish lobby and the in-game Escape menu. Immediate-mode on the HUD's
//! UI-MasterAtlas + font (`hud::Ui`) and the main-menu art sheet (`ART`), laid out in the HUD's 1920×1080 design
//! space (centred, uniform scale). The main menu copies FrontendMenus.xaml (Show_Frontend_MainMenu) and
//! BTN_NewFrontend.xaml; save list and lobby are plain panels for now (ponytail), over the same backdrop.
use crate::hud::{HudTri, Ui};
use crate::skirmish::{self, Lobby, LORDS};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

// the Crusader Trail screens (Skirmish menu, trails, maps): a child module, it draws with this file's helpers
#[path = "trail_menu.rs"]
pub mod trail_menu;
#[path = "chat_panel.rs"]
pub mod chat_panel; // the LAN game's chat panel (lan.rs keeps its state)
// the tutorial's panel over the game (HUD_Tutorial): a child module, it draws with this file's helpers
#[path = "tutorial.rs"]
pub mod tutorial;
#[path = "mp_menu.rs"]
pub mod mp_menu; // LAN multiplayer: the LAN screen + the lobby session (mp_menu.rs)
#[path = "help.rs"]
pub mod help; // in-game Help: the game's HTML help pages, drawn natively (help.rs)
#[path = "credits.rs"]
mod credits; // FRONT_Credits (credits.rs)
#[path = "lobby.rs"]
mod lobby; // the skirmish lobby's map list search / sort and its options popup (lobby.rs)
#[path = "editor.rs"]
pub mod editor; // the map editor: FRONT_EditorSetup and its in-game panel (editor.rs)
#[path = "lord_info.rs"]
mod lord_info; // a custom lord's backstory page, opened from the lobby (lord_info.rs)
#[path = "extras.rs"]
mod extras; // the main menu's Extras: this build, updates, importing custom lords (extras.rs)
#[path = "coa.rs"]
pub mod coa; // the coat of arms: compositing, the faces, Options > Coat of Arms (coa.rs)

// vertex colours are linear (the surface is sRGB): #EFF3C6, #FFD673, #99998C
const CREAM: [f32; 4] = [0.863, 0.896, 0.565, 1.0];
const GOLD: [f32; 4] = [1.0, 0.672, 0.171, 1.0];
const DIM: [f32; 4] = [0.319, 0.319, 0.262, 1.0];
const ROWS: usize = 18; // list rows shown
const LIST_ROWS: usize = 6; // lobby map list rows (260 px ListView: 29 px header, 34 px rows; the 7th is cut off)
const SOLO_ROWS: usize = 13; // Sandbox map list rows (486 px ListView)

/// HUD texture index of the main-menu art sheet (re/scripts/export_frontend.py), uploaded by main.rs.
pub const ART: u32 = 3;

/// frontend.tsv: sheet size and piece rects (key -> x, y, w, h in sheet px, origin top-left). Empty if not exported.
fn art() -> &'static (f32, f32, HashMap<String, [f32; 4]>) {
    static A: OnceLock<(f32, f32, HashMap<String, [f32; 4]>)> = OnceLock::new();
    A.get_or_init(|| {
        let tsv = std::fs::read_to_string(crate::cache_dir().join("ui/frontend.tsv")).unwrap_or_default();
        let rows: Vec<Vec<&str>> = tsv.lines().map(|l| l.split('\t').collect()).filter(|r: &Vec<&str>| r.len() >= 3).collect();
        let n = |s: &str| s.parse::<f32>().unwrap_or(1.0);
        let (w, h) = rows.first().map_or((1.0, 1.0), |r| (n(r[1]), n(r[2])));
        (w, h, rows.iter().skip(1).filter(|r| r.len() == 5).map(|r| (r[0].to_string(), [n(r[1]), n(r[2]), n(r[3]), n(r[4])])).collect())
    })
}

/// The art sheet's pixels for the GPU: (w, h, RGBA rows top-down). None = not exported.
pub fn art_pixels() -> Option<(u32, u32, Vec<u8>)> {
    let &(w, h, _) = art();
    let px = std::fs::read(crate::cache_dir().join("ui/frontend.rgba")).ok().filter(|p| p.len() == (w * h * 4.0) as usize)?;
    Some((w as u32, h as u32, px))
}

/// Textured quad on HUD texture `tex`: screen rect `p` (x0, y0, x1, y1), `uv` = u0, v0 (top), u1, v1 (bottom).
fn quad(out: &mut Vec<HudTri>, tex: u32, p: [f32; 4], uv: [f32; 4], color: [f32; 4]) {
    let v = |x: f32, y: f32, u: f32, t: f32| crate::Vert { pos: [x, y], uv: [u, t], color, team: [0.0; 4] };
    let (tl, tr, bl, br) = (v(p[0], p[1], uv[0], uv[1]), v(p[2], p[1], uv[2], uv[1]), v(p[0], p[3], uv[0], uv[3]), v(p[2], p[3], uv[2], uv[3]));
    out.push((tex, [tl, tr, bl]));
    out.push((tex, [tr, br, bl]));
}

/// Art piece `key` stretched to screen rect `p`, alpha `a`.
pub(crate) fn pic(out: &mut Vec<HudTri>, key: &str, p: [f32; 4], a: f32) {
    let (sw, sh, rects) = art();
    let Some(&[x, y, w, h]) = rects.get(key) else { return };
    // half a texel in: the pieces are packed edge to edge
    quad(out, ART, p, [(x + 0.5) / sw, (y + 0.5) / sh, (x + w - 0.5) / sw, (y + h - 0.5) / sh], [1.0, 1.0, 1.0, a]);
}

/// UI-MasterAtlas rects (rects.tsv, key -> x, y, w, h, top-left origin): `Ui::img` only draws whole sprites.
fn atlas_rects() -> &'static HashMap<String, [f32; 4]> {
    static R: OnceLock<HashMap<String, [f32; 4]>> = OnceLock::new();
    R.get_or_init(|| std::fs::read_to_string(crate::cache_dir().join("ui/rects.tsv")).unwrap_or_default().lines().filter_map(|l| {
        let c: Vec<&str> = l.split('\t').collect();
        let n = |i: usize| c.get(i)?.parse::<f32>().ok();
        Some((c.first()?.to_string(), [n(1)?, n(2)?, n(3)?, n(4)?]))
    }).collect())
}

const FULL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
/// HUD texture `SCREEN` is 1920 × SCREEN_H: the screen's 1920×1080 image on top; a story page's drawing (694×409) and
/// map (561×535) side by side below it (they sit in the 1920 grid, the background fills the window).
pub const SCREEN_H: u32 = 1620;
const SCREEN_UV: [f32; 4] = [0.0, 0.0, 1.0, 1080.0 / SCREEN_H as f32];
const STORY_ART: [(f32, f32, f32, f32, f32, f32); 2] = [(0.0, 694.0, 409.0, 236.0, 215.0, 694.0), (694.0, 561.0, 535.0, 1273.0, 204.5, 561.0)]; // texture x, w, h; grid x, y, w

/// Part `f` (fractions x0, y0, x1, y1) of atlas sprite `key` into screen rect `p` (the atlas is stored bottom row first).
fn atlas_part(out: &mut Vec<HudTri>, key: &str, f: [f32; 4], p: [f32; 4], color: [f32; 4]) {
    let Some(&[x, y, w, h]) = atlas_rects().get(key) else { return };
    const A: f32 = 8192.0;
    quad(out, 0, p, [(x + f[0] * w) / A, (A - y - f[1] * h) / A, (x + f[2] * w) / A, (A - y - f[3] * h) / A], color);
}

/// Part `f` of art-sheet piece `key` (`ART`, rows top-down) into screen rect `p`.
fn art_part(out: &mut Vec<HudTri>, key: &str, f: [f32; 4], p: [f32; 4], color: [f32; 4]) {
    let (sw, sh, rects) = art();
    let Some(&[x, y, w, h]) = rects.get(key) else { return };
    quad(out, ART, p, [(x + f[0] * w) / sw, (y + f[1] * h) / sh, (x + f[2] * w) / sw, (y + f[3] * h) / sh], color);
}

/// HUD texture index of the current screen's full-screen image (ui/screens/<name>.rgba, 1920×1080, `Menu::screen_image`),
/// uploaded by main.rs.
pub const SCREEN: u32 = 5;

/// ImageBrush TileMode=Tile: sprite `key` of texture `tex` (0 = UI-MasterAtlas, `ART`) in `tw`×`th` screen-px tiles
/// from the top-left of `p`, cut at the far edges.
fn tiled(out: &mut Vec<HudTri>, tex: u32, key: &str, p: [f32; 4], tw: f32, th: f32, color: [f32; 4]) {
    let mut y = p[1];
    while y < p[3] - 0.01 {
        let y1 = (y + th).min(p[3]);
        let mut x = p[0];
        while x < p[2] - 0.01 {
            let x1 = (x + tw).min(p[2]);
            let f = [0.0, 0.0, (x1 - x) / tw, (y1 - y) / th];
            if tex == ART { art_part(out, key, f, [x, y, x1, y1], color) } else { atlas_part(out, key, f, [x, y, x1, y1], color) }
            x = x1;
        }
        y = y1;
    }
}

/// The main-menu video's first frame over the whole window (Image Stretch=UniformToFill, centred).
fn backdrop(out: &mut Vec<HudTri>, w: f32, h: f32) {
    let Some(&[_, _, bw, bh]) = art().2.get("backdrop") else { return Ui::fill(out, [0.0, 0.0, w, h], [0.05, 0.04, 0.03, 1.0]) };
    let k = (w / bw).max(h / bh);
    let p = [(w - bw * k) / 2.0, (h - bh * k) / 2.0, (w + bw * k) / 2.0, (h + bh * k) / 2.0];
    if !crate::video::backdrop(out, p) { pic(out, "backdrop", p, 1.0); } // the looping video (video.rs) once exported
}

/// Main-menu column (FrontendMenus.xaml, Grid Margin 1082,40,220,0, one button every 150 px): label key, shield
/// sprite, help text key (Show_FrontEndCombat_*_Help), action (None = mode not in the native port yet).
/// The original's sixth, Roadmap (Firefly's DLC store page), is left out: nothing to sell here.
const MAIN: [(&str, &str, &str, Option<Act>); 6] = [
    ("TEXT_NEW_CTEXT_198", "shield_campaign", "TEXT_NEW_CTEXT_290", Some(Act::Trail(trail_menu::A::Open(trail_menu::T::Skirmish)))),
    ("TEXT_SKIRMISH_CHOOSE2_002", "shield_castle", "TEXT_NEW_CTEXT_291", Some(Act::Campaigns)),
    ("TEXT_NEW_CTEXT_199", "shield_economic_gmaes", "TEXT_NEW_CTEXT_292", Some(Act::Sandbox)),
    ("TEXT_PREVIEW_014", "shield_combat_games", "TEXT_NEW_CTEXT_293", Some(Act::Mp(mp_menu::A::Open))),
    ("TEXT_MAPEDIT_001", "shield_mapeditor", "TEXT_NEW_CTEXT_294", Some(Act::Ed(editor::A::Open))),
    // ours (extras.rs): the original's sixth slot was the Workshop; not a text key, Ui::text_of keeps it as it is
    ("Extras", "shield_crusader", "Check for updates, see this build's details and import custom lords.", Some(Act::Extras(extras::A::Open))),
];
const NOT_YET: &str = "Not in the native port yet.";

/// What the caller has to do after a click.
/// `Mission(id)`: load campaign mission id (1..35) for its briefing; `Brief(d)`: the briefing's Start at difficulty d;
/// `QuitMission(c)`: leave it for campaign c's mission list.
/// `FreeBuild(map, options)` / `Scenario(map, difficulty)`: the Sandbox screen's Play (FRONT_StandaloneMission.StartMap).
/// `Settings(mode)`: Options changed settings.cfg (game speed, VSync, cursor lock, ...); Some = Apply's screen mode
/// (0 fullscreen, 1 borderless, 2 windowed at the chosen size). `Save(path)`: the Save dialog's file.
pub enum Go { Load(PathBuf), Replay(PathBuf), Start(Lobby), Quit, Resume, ToMenu, Save(PathBuf), Restart, Mission(i32), Brief(i32), QuitMission(usize), FreeBuild(PathBuf, skirmish::FreeBuild), Scenario(PathBuf, i32), Settings(Option<u8>), Tutorial, Trail(i32, i32, i32), Lan(Box<mp_menu::Started>), Editor(editor::Start) }

#[derive(Clone, Copy, PartialEq)]
enum Act { Skirmish, LoadList, Quit, Back, Pick(usize), Fair(i32), Goods(i32), Outposts, AddAiOpen, AddAiClose, AddLord(usize), DelAi(usize), LordInfo(usize), LordInfoClose, LordTrailer, TeamsOpen, TeamsClose, TeamFace(usize), TeamCancel, Restart, Campaigns, Campaign(usize), ToCampaigns, Mission(i32), PlayMission, Difficulty, BriefStart, BriefQuit, BriefTab(u8), Hint(usize), HelpBack, StoryAdvance, Sandbox, SoloOpen(bool), SoloNext, SoloPlay, SoloDifficulty, Advanced, Cycle(u8), Faction(i32), Flag(u8), Include(u8), Options, OptPage(u8), OptBack, OptFolder, ScreenMode, ApplyMode, Check(u8), Slider(u8), Group(u8, bool), Reset(u8), Colour(i32), HkRow(u16), HkSlot(u8), HkClose, HkApply, HkClear, HkCancel, HkPreset(bool), Nop, Tutorial, Start, Resume, ToMenu, Save, Trail(trail_menu::A), Mp(mp_menu::A), Help(help::A), Lobby(lobby::A), Update, GameFiles, Extras(extras::A),
    WinSize, SaveGo, Delete(usize), Yes, No, Sort(u8), Credits, Ed(editor::A), Coa(coa::A), NameBox }

/// `Missions(c, m)`: campaign c (0..7) with mission m selected.
/// `Load`: the save browser; `Save`: the Save Game dialog (the same list, own saves only, and a name box).
enum Screen { Main, Load, Save, Lobby(Lobby), Pause, Campaigns, Missions(usize, i32), Briefing(Brief), Story(Story), Sandbox, Solo(Solo), Options(Opt), Trail(trail_menu::T), Mp(mp_menu::T), Credits(credits::Credits), Help(Box<help::Help>), Editor(editor::T), Lord(Box<lord_info::Page>), Extras }

/// The save browser's popup: overwrite this file / delete this file? `Leave`: the pause menu's Quit Mission (false) /
/// Exit Crusader (true), HUD_IngameMenu cases 6 / 7.
#[derive(Clone, PartialEq)]
enum Confirm { Overwrite(PathBuf), Delete(PathBuf), Leave(bool) }

/// HUD_Options (its 800×600 grid, options-ui-spec.md): `page` 0 Video, 1 Sound, 2 Controls, 3 Hotkeys, 4 Player;
/// `in_game` = from the pause menu (back to it, `can_restart` kept for it); `mode` the screen mode picked (0
/// fullscreen, 1 borderless, 2 windowed), `applied` the one in use; `size` the window size picked for windowed mode
/// (logical px, None = unchanged); `scroll` the page's, design px. Page 5 = View Patch Notes (ours: this app's notes).
/// `hk`: the Hotkeys popup on row r of `hotkeys()`, slot -1 = choosing a slot, 0 / 1 = waiting for a key (`hk_new`).
/// Page 6 = Coat of Arms (`coa` its tabs, coa.rs); `typing` = the Player page's name box has the keyboard.
#[derive(Clone, Copy)]
struct Opt { page: u8, in_game: bool, can_restart: bool, mode: u8, applied: u8, size: Option<[u32; 2]>, scroll: f32, hk: Option<(u16, i8)>, coa: coa::Ed, typing: bool }

/// Windowed-mode sizes (logical px) the Resolution row cycles through: those that fit the screen (ux::screen_size).
const WINDOW_SIZES: [[u32; 2]; 7] = [[1280, 720], [1366, 768], [1440, 900], [1600, 900], [1920, 1080], [2304, 1296], [2560, 1440]];
/// The saved windowed size ("NativeWindowSize" = "WxH"), None = 80 % of the screen.
pub fn window_size() -> Option<[u32; 2]> {
    let v = opt_text("NativeWindowSize")?;
    let (w, h) = v.split_once('x')?;
    Some([w.trim().parse().ok()?, h.trim().parse().ok()?])
}

/// Options checkboxes: page, settings.cfg key ("!" = stored inverted), label, default, x, y (800×600 grid, before the
/// page's scroll). Applied: the sound ones (audio.rs, ux.rs), Lock Cursor and VSync (ux.rs apply_options), the clocks
/// (`clock_overlay`), SH1 RTS controls (live.rs), Confirm Disband / Building Tooltips (`opt`, for the HUD), Compass
/// (ost.rs), Sands Timer (trail_menu.rs). VSync is ours ("NativeVSync", on by default; the game's "VSync" key defaults
/// off). ponytail: Radar zoomed out, Steam overlay, Extra Zoom, Moat, Custom Intros are saved only.
const CHECKS: [(u8, &str, &str, bool, f32, f32); 19] = [
    (0, "LockCursor", "TEXT_NEW_TEXT_084", false, 373.5, 288.0), (0, "Compass", "TEXT_NEW_TEXT2_125", false, 373.5, 316.0),
    (0, "RadarDefaultZoomedOut", "TEXT_NEW_TEXT2_127", false, 373.5, 344.0), (0, "ShowBuildingTooltips", "TEXT_NEW_TEXT2_081", true, 373.5, 372.0),
    (0, "UseSteamOverlayForHelp", "TEXT_NEW_TEXT2_166", false, 373.5, 400.0), (0, "ExtraZoom", "TEXT_NEW_TEXT2_171", true, 373.5, 428.0),
    (0, "LocalTime", "TEXT_NEW_CTEXT_215", false, 373.5, 456.0), (0, "GameTime", "TEXT_NEW_CTEXT_214", false, 373.5, 484.0),
    (0, "SandsTimer", "TEXT_NEW_CTEXT_332", true, 373.5, 512.0),
    (1, "PlayUISFX", "TEXT_NEW_TEXT2_155", true, 373.5, 543.0), (1, "GenieSpeech", "TEXT_SKIRMISH_MISC_013", true, 373.5, 573.0),
    (1, "ReduceMusicVolumeForSpeech", "TEXT_NEW_TEXT2_165", true, 373.5, 603.0), (1, "!BackgroundAudio", "TEXT_NEW_CTEXT_242", false, 373.5, 633.0),
    (1, "MuteInsults", "TEXT_NEW_CTEXT_267", false, 373.5, 663.0), (1, "MuteInsultSpeech", "TEXT_NEW_CTEXT_268", false, 373.5, 693.0),
    (2, "Moat", "TEXT_NEW_CTEXT_204", false, 375.5, 623.48), (2, "Confirm_Disband_Troops", "TEXT_NEW_CTEXT_393", true, 375.5, 658.48),
    (4, "CustomIntros", "TEXT_NEW_TEXT2_143", true, 373.5, 163.8),
    (0, "NativeVSync", "TEXT_NEW_TEXT_085", true, 373.5, 221.0),
];

/// settings.cfg as Options last saved it (read once): what the rest of the app applies.
/// The cached settings.cfg. Tests: one per thread, as their profile folder (skirmish::set_test_profile) is, so
/// parallel tests don't read each other's options.
fn cfg() -> std::sync::MutexGuard<'static, Option<skirmish::Settings>> {
    type M = std::sync::Mutex<Option<skirmish::Settings>>;
    #[cfg(test)]
    thread_local! { static C: &'static M = Box::leak(Box::default()); }
    #[cfg(test)]
    return C.with(|c| *c).lock().unwrap();
    #[cfg(not(test))]
    { static C: M = std::sync::Mutex::new(None); C.lock().unwrap() }
}
/// An Options flag as saved (settings.cfg key, the game's default when missing).
pub fn opt(key: &str, default: bool) -> bool { cfg().get_or_insert_with(skirmish::Settings::load).flag(key, default) }
/// Sets a True / False option outside the options screen (the chat panel's mute boxes) and saves settings.cfg.
pub fn set_opt(key: &str, on: bool) -> std::io::Result<()> {
    let mut c = cfg();
    let cfg = c.get_or_insert_with(skirmish::Settings::load);
    cfg.set(key, if on { "True" } else { "False" });
    cfg.save()
}
/// An Options value as saved ("NativeWindowSize" = "1600x900", ...).
pub fn opt_text(key: &str) -> Option<String> { cfg().get_or_insert_with(skirmish::Settings::load).get(key).map(str::to_string) }
/// Tests: a new test profile's settings.cfg is read afresh.
#[cfg(test)]
pub fn reset_cfg() { *cfg() = None; }
/// Options > Controls > Confirm Disband Troops? (ConfigSettings.Settings_Confirm_Disband_Troops, default on): the HUD's
/// Disband asks first (MainViewModel:36733).
#[allow(dead_code)] // for the HUD's troop panel
pub fn confirm_disband() -> bool { opt("Confirm_Disband_Troops", true) }
/// Options > Video > Show Extended Building Tooltips (default on; HUD_Main.CreateBuildingTooltip).
pub fn building_tooltips() -> bool { opt("ShowBuildingTooltips", true) }
/// Options > Controls: SH1 RTS controls (left click orders, right click deselects; default on), DLL_RunTick's flag.
#[allow(dead_code)] // live.rs (feature "live")
pub fn sh1_rts() -> bool { opt("SH1RTSControls", true) }
/// Options sliders: page, key, label, slider min, max, tick, box x, y (350×60; the slider at +137, +29, 208 wide).
const SLIDERS: [(u8, &str, &str, i32, i32, i32, f32, f32); 7] = [
    (1, "MasterVolume", "TEXT_NEW_TEXT_078", 0, 100, 10, 373.5, 133.0), (1, "MusicVolume", "TEXT_NEW_TEXT_080", 0, 100, 10, 373.5, 223.0),
    (1, "SpeechVolume", "TEXT_NEW_TEXT_081", 0, 100, 10, 373.5, 303.0), (1, "UnitSpeechVolume", "TEXT_NEW_TEXT2_123", 0, 100, 10, 373.5, 383.0),
    (1, "SFXVolume", "TEXT_NEW_TEXT_079", 0, 100, 10, 373.5, 463.0), (2, "GameSpeed", "TEXT_FEEDBACK_024", 2, 18, 1, 377.0, 553.48),
    (0, "ScrollSpeed", "TEXT_NEW_TEXT_083", 0, 15, 1, 374.0, 638.0),
];
/// Controls groups: key, title, first / second button (first = true), default, box y.
const GROUPS: [(&str, &str, &str, &str, bool, f32); 4] = [
    ("PushMapScrolling", "TEXT_NEW_TEXT_068", "TEXT_NEW_TEXT_069", "TEXT_NEW_TEXT_070", true, 113.48),
    ("SH1RTSControls", "TEXT_NEW_TEXT_071", "TEXT_NEW_TEXT_072", "TEXT_NEW_TEXT_073", true, 223.48),
    ("SH1MouseWheel", "TEXT_NEW_TEXT_074", "TEXT_NEW_TEXT_075", "TEXT_NEW_TEXT_076", false, 333.48),
    ("SH1CentreControls", "TEXT_NEW_TEXT2_076", "TEXT_NEW_TEXT2_077", "TEXT_NEW_TEXT2_078", true, 443.48),
];
/// Page content heights (scrolled in a 504 high viewport from y 73): Video, Sound, Controls.
const PAGE_H: [f32; 3] = [781.0, 688.0, 693.48];

/// FRONT_StandaloneMission: Custom Scenarios (`invasion`, at `difficulty` 0..3) or Free Build (options `fb`; `advanced`
/// shows them); `sel` = the map in `files` / `solo`; `user` / `builtin` = the include boxes.
#[derive(Clone, Copy)]
struct Solo { invasion: bool, sel: Option<usize>, fb: skirmish::FreeBuild, difficulty: i32, user: bool, builtin: bool }

/// HUD_Briefing over the paused mission `id` at difficulty `d`: `tab` 0 Objectives, 1 Hints (`hints` revealed,
/// GameData.numHintsUnlockedForMission), 2 Help (scrolled `scroll` design px).
#[derive(Clone, Copy)]
/// `mid`: opened from the HUD's briefing button in a running mission: Start / Esc resume it, difficulty fixed.
struct Brief { id: i32, d: i32, tab: u8, hints: usize, scroll: f32, mid: bool }

/// FRONT_Story, one page for mission `id`: `campaign` = the campaign's intro page (before its first mission) or,
/// with `aftermath`, its closing page. The body types in from `t0`, 50 ms a letter (`all`: clicked, shown whole;
/// `len` = its letters, set by the draw).
#[derive(Clone, Copy)]
struct Story { id: i32, campaign: bool, aftermath: bool, t0: Instant, all: bool, len: usize }

/// SFXManager.music_filenames[47 + id - 1] (id <= 20) / [116 + id - 21]: the story page music of mission id.
const STORY_MUSIC: [&str; 35] = ["crusader", "crusader", "sand wedgie", "flt_04", "crusader", "thelastdrop", "caravan_ambient",
    "thelastdrop", "caravan_ambient", "thelastdrop", "crusader", "caravan_ambient", "caravan_ambient", "suspense1b", "caravan_ambient",
    "crusader", "caravan", "null", "sand wedgie", "suspense2b", "campaign 6_01", "campaign 6_02", "campaign 6_03", "campaign 6_04",
    "campaign 6_05", "campaign 7_01", "campaign 7_02", "campaign 7_03", "campaign 7_04", "campaign 7_05", "campaign 8_01",
    "campaign 8_02", "campaign 8_03", "campaign 8_04", "campaign 8_05"];
/// GameData.nhints: hint lines per mission.
const NHINTS: [usize; 35] = [2, 3, 3, 3, 2, 2, 3, 3, 1, 3, 3, 4, 3, 4, 2, 2, 1, 1, 3, 1, 4, 4, 3, 3, 3, 4, 3, 3, 4, 4, 4, 3, 3, 4, 3];

pub struct Menu {
    screen: Screen,
    files: Vec<PathBuf>, // saves (Load) or maps (Lobby)
    infos: Vec<skirmish::MapInfo>, // Lobby: per map in `files`
    scroll: usize,
    hits: Vec<([f32; 4], Act)>,
    pub error: Option<String>,
    in_game: bool,             // opened from a running game (Escape): Back / Load return to the pause menu
    progress: skirmish::Progress, // campaign screens: unlocked / won missions
    can_restart: bool,         // pause menu: the game came from the lobby (Restart Mission replays it)
    add_ai: bool,              // Lobby: the Add-AI face grid is open
    teams_open: bool,          // Lobby: the round-table Teams popup is open
    team_pick: Option<usize>,  // Teams: the picked-up member (0 = you, 1.. = AI k - 1)
    brief: Option<String>,     // Lobby: the selected map's own briefing (user maps)
    help: Vec<(u8, String)>,   // Briefing Help tab: the mission's help page as (0 title / 1 heading / 2 text) blocks
    solo: Vec<skirmish::SoloInfo>, // Sandbox screen: per map in `files`
    cfg: skirmish::Settings,   // Options: settings.cfg as loaded / edited
    hk_new: Option<String>,    // Options > Hotkeys: the key pressed for the slot being changed
    hover: Option<(i32, Instant)>, // mission list: the completed mission under the cursor, since when
    brief_map: Option<PathBuf>, // a Custom Scenario's briefing (Brief.id 0): its map (the title)
    /// the running game's objective lines, GameData.game_type == 0 and the objective timer (now, length, text), handed
    /// in by main.rs while a briefing shows
    pub goals: (Vec<crate::hud::Goal>, bool, Option<[i32; 3]>),
    /// Lobby: the selected map's 200×200 RGBA radar preview, for main.rs to upload into HUD texture 2 (taken once)
    pub preview: Option<Vec<u8>>,
    // layout of the current frame: scale, top-left of the design box (1920×1080; the lobby 1366×768)
    s: f32, o: [f32; 2],
    mp: mp_menu::Mp, // LAN: the games found / the lobby session (mp_menu.rs)
    name: String,              // Save dialog: the file name typed
    confirm: Option<Confirm>,  // Save / Load: the Yes / No popup
    lst: lobby::List,          // Lobby: the map list's search / sort / include, the Advanced popup (lobby.rs)
    mouse: ([f32; 2], bool),   // this frame's cursor and held left button (`draw`): the checkboxes' hover / press
}

/// Save / Load list order (HUD_LoadSaveRequester sortByColumn / sortByAscending): 0 name, 1 date; ascending. Kept
/// for the session, date newest first to start with.
static SORT: std::sync::Mutex<(u8, bool)> = std::sync::Mutex::new((1, false));
/// The last name saved under or loaded (HUD_LoadSaveRequester.rememberedSaveName): the Save dialog starts with it.
static LAST_SAVE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

impl Menu {
    pub fn main() -> Self { Menu { screen: Screen::Main, files: vec![], infos: vec![], scroll: 0, hits: vec![], error: None, in_game: false, progress: Default::default(), can_restart: false, add_ai: false, teams_open: false, team_pick: None, brief: None, help: vec![], solo: vec![], cfg: Default::default(), hk_new: None, hover: None, brief_map: None, goals: (vec![], false, None), preview: None, s: 1.0, o: [0.0; 2], mp: Default::default(), name: String::new(), confirm: None, lst: Default::default(), mouse: ([0.0; 2], false) } }
    pub fn pause(can_restart: bool) -> Self { Menu { screen: Screen::Pause, in_game: true, can_restart, ..Self::main() } }
    /// Back from a sub-screen: the pause menu in game, else the main menu.
    fn home(&self) -> Self { if self.in_game { Self::pause(self.can_restart) } else { Self::main() } }
    pub fn is_pause(&self) -> bool { matches!(self.screen, Screen::Pause) }
    /// Screen B for campaign c (0..7), its highest unlocked mission selected (CurrentSelectedHistorical<c>Mission).
    pub fn missions(c: usize) -> Self {
        let progress = skirmish::Progress::load();
        Menu { screen: Screen::Missions(c, progress.unlocked[c].min(5)), progress, ..Self::main() }
    }
    /// HUD_Briefing for campaign mission `id` over the paused game (loaded at difficulty 1, like the original);
    /// `from_story`: its narration plays (MainViewModel.ButtonGotoBriefing("FromStory")).
    pub fn briefing(id: i32, from_story: bool) -> Self {
        if from_story { crate::audio::story(None, Some(&format!("Campaign\\M{id}_Brief.wav"))); }
        Menu { screen: Screen::Briefing(Brief { id, d: 1, tab: 0, hints: 0, scroll: 0.0, mid: false }), in_game: true, ..Self::main() }
    }
    /// The HUD's briefing button (SideTabs ButtonGotoBriefing) in a running campaign mission at difficulty `d`.
    pub fn briefing_mid(id: i32, d: i32) -> Self {
        Menu { screen: Screen::Briefing(Brief { id, d, tab: 0, hints: 0, scroll: 0.0, mid: true }), in_game: true, ..Self::main() }
    }
    /// HUD_Briefing for a Custom Scenario (FRONT_StandaloneMission.StartMap -> PostStartMapMission ->
    /// ButtonGotoBriefing): the map's name, its objectives, the difficulty it was started at (shown, not changeable),
    /// no Hints / Help tabs, no narration. `mid`: reopened from the HUD's side tab (Free Build too, `d` 0: no
    /// difficulty row); Start resumes.
    pub fn scenario_briefing(map: PathBuf, d: i32, mid: bool) -> Self {
        Menu { screen: Screen::Briefing(Brief { id: 0, d, tab: 0, hints: 0, scroll: 0.0, mid }), in_game: true, brief_map: Some(map), ..Self::main() }
    }
    /// FRONT_Story.OpenStory(id, aftermath): the campaign's intro page first when `id` opens a campaign, then the
    /// mission's page; `aftermath`: the campaign's closing page (after its 5th mission), then the campaign list.
    pub fn story(id: i32, aftermath: bool) -> Self {
        let s = Story { id, campaign: aftermath || (id - 1) % 5 == 0, aftermath, t0: Instant::now(), all: false, len: 0 };
        let c = (id - 1) / 5;
        let music = if aftermath { if c == 0 { "solovln_01" } else { "flt_narr1" } } else { STORY_MUSIC[id as usize - 1] };
        crate::audio::story(Some(&format!("fx\\music\\{music}.raw")), Some(&s.speech()));
        Menu { screen: Screen::Story(s), ..Self::main() }
    }
    pub fn is_story(&self) -> bool { matches!(self.screen, Screen::Story(_)) }
    /// HUD_MissionOver's Exit after a skirmish: its setup again with the game's map and lords (FRONT_Multiplayer.Open
    /// with restartSkirmishMapInfo): the Skirmish button's list, the game's lobby swapped in.
    pub fn skirmish_again(l: Lobby) -> Self {
        let mut m = Self::main();
        m.open_lobby(l);
        m
    }
    /// The full-screen image this screen shows (main.rs uploads it into `SCREEN`).
    pub fn screen_image(&self) -> Option<String> {
        match self.screen {
            Screen::Campaigns => Some("HistoricalCampaignMenu-Background".into()),
            Screen::Missions(c, _) => Some(format!("HistoricalCampaign-Campaign0{}", c + 1)),
            Screen::Briefing(..) => Some("BriefingBackground".into()),
            Screen::Story(s) => Some(format!("story:{}", s.art())),
            Screen::Trail(t) => trail_menu::screen_image(&t),
            Screen::Help(ref h) => Some(h.image()),
            Screen::Credits(c) => c.image(),
            Screen::Lord(ref p) => Some(format!("lord:{}", p.name)), // lord_info.rs
            _ => None,
        }
    }

    /// Escape: back one screen. None = close the menu (in-game) / nothing (main menu).
    pub fn back(&mut self) -> Option<Go> {
        if matches!(self.screen, Screen::Lobby(_) | Screen::Solo(_)) && self.lobby_back() { return None; } // the search box, the Advanced popup, the mode picker
        match self.screen {
            Screen::Pause if self.confirm.is_some() => { self.confirm = None; None } // Esc = No: back to Game Options
            Screen::Pause => Some(Go::Resume),
            Screen::Main => None,
            Screen::Lobby(_) if self.add_ai || self.teams_open => { (self.add_ai, self.teams_open, self.team_pick) = (false, false, None); None }
            Screen::Missions(..) => { self.screen = Screen::Campaigns; None }
            Screen::Briefing(b) if b.mid => Some(Go::Resume),
            Screen::Briefing(b) => { crate::audio::story(None, None); Some(if b.id > 0 { Go::QuitMission(((b.id - 1) / 5) as usize) } else { Go::ToMenu }) }
            Screen::Story(_) => self.story_end(), // FRONT_Story.EscapePressed
            Screen::Solo(_) => { *self = Menu { screen: Screen::Sandbox, ..Self::main() }; None } // ButtonClicked("Combat")
            Screen::Trail(t) => self.trail_back(t),
            Screen::Mp(_) if self.mp_typing() => { self.mp.typing = false; None } // Esc closes the address field (mp_menu.rs)
            Screen::Options(op) if op.hk.is_some() => { if let Screen::Options(op) = &mut self.screen { op.hk = None; } None } // Esc closes the popup
            Screen::Options(op) if op.typing => { if let Screen::Options(op) = &mut self.screen { op.typing = false; } None } // and the name box
            Screen::Options(_) => self.close_options(),
            Screen::Help(_) => self.help_close(),
            Screen::Lord(_) => { self.lord_close(); None }
            Screen::Editor(t) if t.loading() => { self.ed_back(); None } // the map list back to the setup (editor.rs)
            Screen::Load | Screen::Save if self.confirm.is_some() => { self.confirm = None; None } // Esc = No
            _ => { *self = self.home(); None }
        }
    }

    /// The Load Game list: own saves, then the real game's (read-only) when they are a different folder.
    pub fn open_load(&mut self) {
        self.open(Screen::Load, crate::saves::files()); // saves, quick / auto saves, replays (saves.rs)
        self.resort();
    }
    /// HUD_LoadSaveRequester in save mode (the pause menu's Save, ⌘S): our saves, the name box with the name last
    /// saved / loaded.
    pub fn open_save(&mut self) {
        self.open(Screen::Save, crate::saves::own_saves());
        self.resort();
        (self.name, self.confirm) = (LAST_SAVE.lock().unwrap().clone(), None);
    }
    fn resort(&mut self) {
        let (by, asc) = *SORT.lock().unwrap();
        crate::saves::sort(&mut self.files, by, asc);
    }
    /// Save dialog: Save pressed (or Return). A name that exists asks first ("Overwrite File?").
    fn save_go(&mut self) -> Option<Go> {
        let Some(path) = crate::saves::path_for(&self.name) else { self.error = Some("Type a name for the save".into()); return None };
        if path.exists() && self.confirm.is_none() { self.confirm = Some(Confirm::Overwrite(path)); return None; }
        *LAST_SAVE.lock().unwrap() = self.name.trim().to_string();
        crate::audio::speech(1, "General_Saving.wav");
        *self = self.home();
        self.error = Some(format!("Saved {}", path.file_stem().unwrap_or_default().to_string_lossy()));
        Some(Go::Save(path))
    }
    /// Save dialog name box: a typed key (keys.rs Unity name with modifiers). Letters (Shift = capitals), digits,
    /// space, - and _; Backspace; Return saves.
    fn save_key(&mut self, k: &str) -> Option<Go> {
        let c = match k.split(',').next().unwrap_or("") {
            "Backspace" | "Delete" => { self.name.pop(); return None }
            "Return" | "KeypadEnter" => return self.save_go(),
            _ => lobby::key_char(k)?,
        };
        if self.name.chars().count() < 40 { self.name.push(c); }
        None
    }
    /// HUD_Options.OpenOptions: Video first; the screen mode from settings.cfg (FullscreenType 3 = windowed). From the
    /// pause menu (`in_game`) Back returns to it.
    pub fn open_options(&mut self) {
        let cfg = skirmish::Settings::load();
        let mode = match cfg.num("FullscreenType", 1.0) as i32 { 0 => 0, 3 => 2, _ => 1 };
        let op = Opt { page: 0, in_game: self.in_game, can_restart: self.can_restart, mode, applied: mode, size: None, scroll: 0.0, hk: None, coa: Default::default(), typing: false };
        *self = Menu { screen: Screen::Options(op), in_game: self.in_game, can_restart: self.can_restart, cfg, ..Self::main() };
    }

    /// Options > Hotkeys is waiting for a key (KeyManager.HotKeySelectorMode).
    pub fn capturing(&self) -> bool {
        matches!(self.screen, Screen::Options(Opt { hk: Some((_, 0..=1)), .. })) || self.mp_typing() || self.trail_typing() || self.save_typing() || self.lobby_typing()
            || self.name_typing()
    }
    fn name_typing(&self) -> bool { matches!(self.screen, Screen::Options(Opt { typing: true, .. })) }
    /// Options > Player Settings name box (HUD_Options TextBoxChangeName, MaxLength 39): typed keys, Backspace;
    /// Return leaves the box. Saved as it changes ("Name", Settings_UserName).
    fn name_key(&mut self, k: &str) {
        let mut name = coa::player_name();
        match k.split(',').next().unwrap_or("") {
            "Backspace" | "Delete" => { name.pop(); }
            "Return" | "KeypadEnter" | "Tab" => { if let Screen::Options(op) = &mut self.screen { op.typing = false; } return; }
            _ => match lobby::key_char(k) { Some(c) if name.chars().count() < 39 => name.push(c), _ => return },
        }
        self.cfg.set("Name", name);
        self.save_cfg();
    }
    fn save_typing(&self) -> bool { matches!(self.screen, Screen::Save) && self.confirm.is_none() }
    /// The key pressed while `capturing` (a KeyCode name with modifiers, keys.rs): it shows as the New Key until Apply.
    /// Some = the Save dialog's Return saved.
    pub fn key(&mut self, k: &str) -> Option<Go> {
        if self.mp_typing() { let _ = self.mp_key(k); return None; } // the LAN screen's address field (mp_menu.rs)
        if self.trail_typing() { self.trail_key(k); return None; } // the co-op lobby's chat line (trail_menu.rs)
        if self.save_typing() { return self.save_key(k); } // the Save dialog's name box
        if self.lobby_typing() { self.lobby_key(k); return None; } // the lobby's map search (lobby.rs)
        if self.name_typing() { self.name_key(k); return None; } // Options > Player Settings: the name box
        if self.capturing() { self.hk_new = Some(k.to_string()); }
        None
    }

    /// Right click (MouseRightButtonUp): a Free Build option steps back.
    pub fn right_click(&mut self, c: [f32; 2]) {
        let Some(&(_, Act::Cycle(f))) = self.hits.iter().rev().find(|(p, _)| Self::inside(*p, c)) else { return };
        if let Screen::Solo(so) = &mut self.screen { cycle(&mut so.fb, f, true); }
    }

    /// FRONT_StandaloneMission.doOpen: the mode's map list (`fresh` = fromNew: search, sort and Show Completed reset;
    /// the carousel and the include boxes keep them; Free Build hides Show Completed), the selected map kept, else the
    /// first (Update auto-selects row 0). lobby.rs filters and sorts it.
    fn open_solo(&mut self, so: Solo, fresh: bool) {
        if fresh { self.lst = Default::default(); }
        self.lst.completed &= so.invasion;
        self.lst.solo = skirmish::solo_maps(if so.invasion { 1 } else { 3 }, so.user, so.builtin);
        self.progress = skirmish::Progress::load(); // the won maps, for Show Completed
        (self.screen, self.scroll, self.error, self.preview, self.brief) = (Screen::Solo(so), 0, None, None, None);
        self.lst.dirty = true; // again by the shown names at the draw
        self.solo_rebuild(None);
    }
    fn select_solo(&mut self, i: usize) {
        self.preview = skirmish::map_preview(&self.files[i]);
        self.brief = skirmish::map_briefing(&self.files[i]);
    }

    pub fn wheel(&mut self, lines: f32) {
        if let Screen::Help(h) = &mut self.screen { return h.wheel(lines); }
        if let Screen::Briefing(b) = &mut self.screen { b.scroll = (b.scroll - lines * 40.0).max(0.0); return; } // Help page
        if let Screen::Options(op) = &mut self.screen {
            let max = match op.page { 0..=2 => PAGE_H[op.page as usize] - 504.0, 3 => hotkeys().len() as f32 * 27.0 - 340.0, 5 => f32::MAX,
                6 => coa::grid_h(&op.coa) - 135.0, _ => 0.0 }; // 5: the draw clamps

            op.scroll = (op.scroll - lines * 40.0).clamp(0.0, max.max(0.0));
            return;
        }
        let max = self.files.len().saturating_sub(self.rows());
        self.scroll = (self.scroll as f32 - lines).clamp(0.0, max as f32) as usize;
    }

    fn open(&mut self, screen: Screen, files: Vec<PathBuf>) { (self.screen, self.files, self.scroll, self.error) = (screen, files, 0, None); }

    /// Lobby: map `i` picked: its radar preview (for main.rs) and own briefing.
    fn select(&mut self, i: usize) {
        self.preview = skirmish::map_preview(&self.files[i]);
        self.brief = skirmish::map_briefing(&self.files[i]);
    }

    pub fn click(&mut self, c: [f32; 2]) -> Option<Go> {
        let &(hit, a) = self.hits.iter().rev().find(|(p, _)| c[0] >= p[0] && c[0] < p[2] && c[1] >= p[1] && c[1] < p[3])?;
        if let Some(r) = self.mp_click_first(a) { return r; } // LAN screen, LAN lobby Play / a client's edits (mp_menu.rs)
        if a != Act::Lobby(lobby::A::Search) { self.lst.typing = false; } // a click elsewhere leaves the search box
        if let Screen::Options(op) = &mut self.screen { op.typing = a == Act::NameBox; } // and the name box
        if let Act::Lobby(a) = a { return self.lobby_click(a); }
        if let Some(r) = self.ed_click_first(a) { return r; } // the map editor's setup (editor.rs)
        match (&mut self.screen, a) {
            // HUD_IngameMenu cases 6 / 7: Quit Mission / Exit Crusader ask first; the advisor's line comes with the popup
            (Screen::Pause, Act::ToMenu | Act::Quit) => {
                crate::audio::speech(1, "General_Quitgame.wav");
                self.confirm = Some(Confirm::Leave(a == Act::Quit));
            }
            (_, Act::Quit) => return Some(Go::Quit),
            (_, Act::Resume) => return Some(Go::Resume),
            (_, Act::Save) => self.open_save(),
            (_, Act::SaveGo) => return self.save_go(),
            (Screen::Save, Act::Pick(i)) => self.name = self.files[i].file_stem().unwrap_or_default().to_string_lossy().into_owned(),
            (_, Act::Delete(i)) => self.confirm = Some(Confirm::Delete(self.files[i].clone())),
            (_, Act::No) => self.confirm = None,
            (_, Act::Yes) => match self.confirm.clone() {
                Some(Confirm::Overwrite(_)) => return self.save_go(),
                Some(Confirm::Leave(exit)) => { self.confirm = None; return Some(if exit { Go::Quit } else { Go::ToMenu }) }
                Some(Confirm::Delete(p)) => {
                    self.confirm = None;
                    let _ = std::fs::remove_file(crate::koth::Hill::sidecar(&p)); // a King of the Hill save's score
                    match std::fs::remove_file(&p) {
                        Ok(()) => { self.files.retain(|f| *f != p); self.scroll = self.scroll.min(self.files.len().saturating_sub(ROWS)); }
                        Err(e) => self.error = Some(format!("{}: {e}", p.display())),
                    }
                }
                None => {}
            },
            // the column headers: the same column again flips the order; a new one starts A-Z / newest first
            (_, Act::Sort(c)) => {
                let mut s = SORT.lock().unwrap();
                *s = if s.0 == c { (c, !s.1) } else { (c, c == 0) };
                drop(s);
                self.resort();
            }
            (_, Act::Credits) => *self = Menu { screen: Screen::Credits(credits::Credits::new()), ..Self::main() },
            (_, Act::Back) => *self = self.home(),
            (_, Act::Restart) => return Some(Go::Restart),
            (_, Act::LoadList) => self.open_load(),
            (_, Act::Trail(a)) => return self.trail_click(a),
            (_, Act::Help(a)) => return self.help_click(a),
            (_, Act::Skirmish) => self.open_lobby(Lobby::from_settings(PathBuf::new())), // lobby.rs
            (Screen::Load, Act::Pick(i)) => {
                if !crate::saves::is_replay(&self.files[i]) {
                    crate::audio::speech(1, "General_Loading.wav");
                    *LAST_SAVE.lock().unwrap() = self.files[i].file_stem().unwrap_or_default().to_string_lossy().into_owned();
                }
                return Some(if crate::saves::is_replay(&self.files[i]) { Go::Replay(self.files[i].clone()) } else { Go::Load(self.files[i].clone()) })
            }
            (_, Act::Campaigns) => *self = Menu { screen: Screen::Campaigns, progress: skirmish::Progress::load(), ..Self::main() },
            (_, Act::Campaign(c)) => (self.screen, self.error) = (Screen::Missions(c, self.progress.unlocked[c].min(5)), None),
            (_, Act::ToCampaigns) => self.screen = Screen::Campaigns,
            (Screen::Missions(_, sel), Act::Mission(m)) => *sel = m,
            (Screen::Missions(c, sel), Act::PlayMission) => { let id = 5 * *c as i32 + *sel; *self = Menu::story(id, false) }
            (Screen::Briefing(b), Act::Difficulty) => b.d = (b.d + 1) % 4, // Easy, Normal, Hard, Very Hard
            (Screen::Briefing(b), Act::BriefStart) if b.mid => return Some(Go::Resume),
            (Screen::Briefing(b), Act::BriefStart) => { crate::audio::story(None, None); return Some(Go::Brief(b.d)) }
            (Screen::Briefing(b), Act::BriefQuit) => { crate::audio::story(None, None); return Some(if b.id > 0 { Go::QuitMission(((b.id - 1) / 5) as usize) } else { Go::ToMenu }) }
            (Screen::Briefing(b), Act::BriefTab(t)) => {
                (b.tab, b.scroll) = (t, 0.0);
                if t == 2 { self.help = help_blocks(b.id); }
            }
            // MainViewModel.ButtonBriefingHint: only the next locked hint opens
            (Screen::Briefing(b), Act::Hint(k)) => if k == b.hints + 1 { b.hints = k },
            (Screen::Briefing(b), Act::HelpBack) => b.scroll = 0.0, // ponytail: links aren't followed, so Back = top
            (_, Act::Sandbox) => *self = Menu { screen: Screen::Sandbox, ..Self::main() },
            (_, Act::Tutorial) => return Some(Go::Tutorial),
            // HUD_Options.OpenOptions: Video first; the screen mode from settings.cfg (FullscreenType 3 = windowed)
            (_, Act::Options) => self.open_options(),
            (Screen::Options(op), Act::OptPage(p)) => (op.page, op.scroll, op.coa) = (p, 0.0, coa::Ed::open()),
            (Screen::Options(op), Act::Coa(x)) => {
                let mut o = *op;
                self.coa_click(&mut o.coa, &mut o.scroll, x);
                self.screen = Screen::Options(o);
            }
            // Hotkeys (HUD_Options.HotKeyList / OptionsHotKeyPanel, KeyManager.SetNewKey)
            (Screen::Options(op), Act::HkRow(r)) => op.hk = Some((r, -1)),
            (Screen::Options(op), Act::HkSlot(k)) => { op.hk = op.hk.map(|(r, _)| (r, k as i8)); self.hk_new = None }
            (Screen::Options(op), Act::HkClose) => op.hk = None,
            (Screen::Options(op), Act::HkCancel) => op.hk = op.hk.map(|(r, _)| (r, -1)),
            (Screen::Options(op), Act::HkApply | Act::HkClear) => if let Some((r, slot @ 0..=1)) = op.hk {
                op.hk = Some((r, -1));
                let key = if a == Act::HkApply { self.hk_new.take().unwrap_or_default() } else { String::new() };
                let mut keys = crate::keys::Keys::from_block(&self.cfg.keys);
                keys.set(&hotkeys()[r as usize].0, slot as usize, &key);
                self.cfg.keys = keys.text();
                self.save_cfg();
            },
            (Screen::Options(_), Act::HkPreset(modern)) => {
                self.cfg.keys = if modern { crate::keys::Keys::modern() } else { crate::keys::Keys::classic() }.text();
                self.save_cfg();
            }
            (_, Act::Update) => if crate::update::click() { return Some(Go::Quit) },
            (_, Act::Extras(a)) => return self.extras_click(a),
            (_, Act::GameFiles) => crate::setup::start_update(),
            (Screen::Options(_), Act::OptBack) => return self.close_options(),
            (Screen::Options(_), Act::OptFolder) => {
                let dir = skirmish::profile_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("open").arg(&dir).spawn(); // Application.OpenURL("file://" + persistentDataPath)
            }
            (Screen::Options(op), Act::ScreenMode) => op.mode = (op.mode + 1) % 3,
            // Resolution (windowed): the next size that fits the screen
            (Screen::Options(op), Act::WinSize) => {
                let [sw, sh] = crate::ux::screen_size();
                let fits: Vec<[u32; 2]> = WINDOW_SIZES.into_iter().filter(|s| s[0] as f32 <= sw && s[1] as f32 <= sh).collect();
                let cur = op.size.or(window_size());
                let next = fits.iter().position(|s| Some(*s) == cur).map_or(0, |i| (i + 1) % fits.len().max(1));
                op.size = fits.get(next).copied();
            }
            (Screen::Options(op), Act::ApplyMode) => {
                (op.applied, op.size) = (op.mode, op.size.filter(|_| op.mode == 2));
                let (mode, size) = (op.mode, op.size);
                op.size = None;
                self.cfg.set("FullscreenType", [0, 1, 3][mode as usize]);
                if let Some([w, h]) = size { self.cfg.set("NativeWindowSize", format!("{w}x{h}")); }
                self.save_cfg();
                return Some(Go::Settings(Some(mode)));
            }
            (Screen::Options(_), Act::Check(i)) => {
                let (_, key, _, default, ..) = CHECKS[i as usize];
                let (key, inv) = key.strip_prefix('!').map_or((key, false), |k| (k, true));
                let on = self.cfg.flag(key, default ^ inv) ^ inv;
                self.cfg.set(key, if !on ^ inv { "True" } else { "False" });
                self.save_cfg();
                return Some(Go::Settings(None)); // applied at once (VSync, cursor lock, sounds, ...)
            }
            (Screen::Options(_), Act::Slider(i)) => {
                let (_, key, _, lo, hi, ..) = SLIDERS[i as usize];
                let f = ((c[0] - hit[0]) / (hit[2] - hit[0]) * 208.0 - 10.0) / 188.0; // thumb centre over the 188 px travel
                self.set_slider(key, lo + ((hi - lo) as f32 * f.clamp(0.0, 1.0)).round() as i32);
            }
            (Screen::Options(_), Act::Group(g, v)) => { self.cfg.set(GROUPS[g as usize].0, if v { "True" } else { "False" }); self.save_cfg(); return Some(Go::Settings(None)) }
            (Screen::Options(_), Act::Colour(k)) => { self.cfg.set("PlayerColour", k); self.save_cfg(); }
            // Reset To Default: only that page's sliders
            (Screen::Options(_), Act::Reset(p)) => match p {
                0 => self.set_slider("ScrollSpeed", 5),
                1 => for (k, v) in [("MasterVolume", 80), ("MusicVolume", 100), ("SpeechVolume", 100), ("UnitSpeechVolume", 100), ("SFXVolume", 100)] { self.set_slider(k, v) },
                _ => self.set_slider("GameSpeed", 8),
            },
            (_, Act::SoloOpen(invasion)) => self.open_solo(Solo { invasion, sel: None, fb: Default::default(), difficulty: 1, user: true, builtin: true }, true),
            // Next: the other mode, keeping the options (its Advanced flag too: the original's quirk) and the include boxes
            (Screen::Solo(so), Act::SoloNext) => { let so = Solo { invasion: !so.invasion, difficulty: 1, ..*so }; self.open_solo(so, false) }
            (Screen::Solo(so), Act::Pick(i)) => { so.sel = Some(i); self.select_solo(i) }
            (Screen::Solo(so), Act::SoloDifficulty) => so.difficulty = (so.difficulty + 1) % 4,
            (Screen::Solo(so), Act::Advanced) => so.fb.advanced = !so.fb.advanced,
            (Screen::Solo(so), Act::Cycle(f)) => cycle(&mut so.fb, f, false),
            (Screen::Solo(so), Act::Faction(b)) => so.fb.opponents = faction(so.fb.opponents, b),
            (Screen::Solo(so), Act::Flag(f)) => {
                let fb = &mut so.fb;
                let v = match f { 0 => &mut fb.remove_animals, 1 => &mut fb.extreme_troops, 2 => &mut fb.extreme_powers, _ => &mut fb.defeat_on_death };
                *v = !*v;
            }
            (Screen::Solo(so), Act::Include(k)) => { if k == 0 { so.user = !so.user } else { so.builtin = !so.builtin }; let so = Solo { sel: None, ..*so }; self.open_solo(so, false) }
            (Screen::Solo(so), Act::SoloPlay) => if let Some(i) = so.sel {
                let map = self.files[i].clone();
                // the Remove Hostile Animals box only counts while it shows (a Free Build map with hostile animals)
                let fb = skirmish::FreeBuild { remove_animals: so.fb.remove_animals && self.solo[i].animals > 0, ..so.fb };
                return Some(if so.invasion { Go::Scenario(map, so.difficulty) } else { Go::FreeBuild(map, fb) });
            },
            // FRONT_Story.ClickStoryAdvance: the whole text, then the next page / the mission
            (Screen::Story(st), Act::StoryAdvance) => {
                if !st.all && st.typed() < st.len { st.all = true }
                else if st.campaign && !st.aftermath {
                    *st = Story { campaign: false, t0: Instant::now(), all: false, len: 0, ..*st };
                    crate::audio::story(None, Some(&st.speech()));
                } else { return self.story_end() }
            }
            (Screen::Lord(p), Act::StoryAdvance) => p.all = true, // the whole story at once
            (Screen::Lord(_), Act::LordInfoClose) => self.lord_close(),
            (Screen::Lord(p), Act::LordTrailer) => p.watch = !p.watch, // "Watch his story" / a click on the playing trailer
            (Screen::Lobby(l), a) => match a {
                Act::Pick(i) => { l.map = self.files[i].clone(); self.select(i) }
                Act::Fair(v) => { l.fairness = v; crate::audio::ui(crate::audio::ui_id::GOLD[(v.clamp(1, 5) - 1) as usize], 1.0) }
                Act::Goods(v) => l.goods = v,
                Act::Outposts => l.outposts = !l.outposts,
                Act::AddAiOpen => { self.add_ai = true; crate::custom_lords::rescan(); crate::audio::speech(2, "Genie_03.wav") }
                Act::AddAiClose => self.add_ai = false,
                // our backstory page of custom lord `n` (lobby lord index), its neutral video exported for it
                Act::LordInfo(n) => if let Some(c) = crate::custom_lords::of_lord(n) { let l = l.clone(); self.lord_open(c, l) },
                // FRONT_Multiplayer AddSkirmishPlayer: refused once the lobby holds the map's start positions (its
                // maxPlayers, at most 8), as the Add AI button already is
                Act::AddLord(n) if self.mp.net.as_ref().map_or(1, |n| n.lobby.humans.len()) + l.ais.len()
                    < self.files.iter().position(|f| *f == l.map).and_then(|i| self.infos.get(i)).or_else(|| self.lst.info(&l.map)).map_or(8, |i| i.players.clamp(0, 8) as usize) => {
                    if n == 99 { crate::audio::ui(crate::audio::ui_id::DICE, 1.0) }
                    // 99 = Random: any lord with a castle layout
                    let n = if n == 99 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize) % LORDS.len() } else { n };
                    l.ais.push((n, 0));
                    // add_player_to_keep_locations: a set start-position order gives the newcomer the first free keep
                    if let Some(free) = l.keeps.as_mut().and_then(|k| k.iter_mut().find(|v| **v < 0)) { *free = l.ais.len() as i32; }
                    crate::audio::genie_add(n); // the lord's "added" line (quiet while the advisor talks)
                }
                Act::DelAi(k) => {
                    crate::audio::genie_kick(l.ais[k].0);
                    let mut t = teams(l);
                    t.remove(k + 1);
                    l.ais.remove(k);
                    normalize(&mut t);
                    set_teams(l, &t);
                    // remove_player_from_keep_locations; the members after it move up (their keeps and castles too)
                    let m = k as i32 + 1;
                    for v in l.keeps.iter_mut().flatten() { if *v == m { *v = -10 } else if *v > m { *v -= 1 } }
                    l.aivs.copy_within(k + 2.., k + 1);
                    l.aivs[7] = -1;
                }
                // the dim overlay re-sends TeamsOpen on every click: the line only on the first
                Act::TeamsOpen => { if !self.teams_open { crate::audio::speech(3, "Genie_01.wav") } (self.teams_open, self.team_pick) = (true, None) }
                Act::TeamsClose => { if self.teams_open && !l.ais.is_empty() { crate::audio::speech(3, "Genie_02.wav") } (self.teams_open, self.team_pick) = (false, None) }
                // pick a face, then another: the first joins the second's team (FRONT_Multiplayer "TeamFace", ReSortTeamInfo)
                Act::TeamFace(m) => { crate::audio::ui(crate::audio::ui_id::TABLE, 1.0); match self.team_pick.take() {
                    None => self.team_pick = Some(m),
                    Some(a) if a == m => {}
                    Some(a) => {
                        let mut t = teams(l);
                        if t[m] == 0 { t[m] = t.iter().max().unwrap_or(&0) + 1; }
                        t[a] = t[m];
                        normalize(&mut t);
                        set_teams(l, &t);
                        if a > 0 { crate::audio::genie_team(l.ais[a - 1].0, t[a] == t[0]) } // the lord's team line
                    }
                } },
                // the centre button: the picked member leaves its team
                Act::TeamCancel => if let Some(a) = self.team_pick.take() {
                    crate::audio::ui(crate::audio::ui_id::TABLE, 1.0);
                    let mut t = teams(l);
                    t[a] = 0;
                    normalize(&mut t);
                    set_teams(l, &t);
                },
                Act::Start => return Some(Go::Start(l.clone())),
                _ => {}
            },
            _ => {}
        }
        None
    }

    // ---- drawing helpers (design px -> screen)
    fn r(&self, x: f32, y: f32, w: f32, h: f32) -> [f32; 4] {
        [self.o[0] + x * self.s, self.o[1] + y * self.s, self.o[0] + (x + w) * self.s, self.o[1] + (y + h) * self.s]
    }
    fn text(&self, ui: &mut Ui, out: &mut Vec<HudTri>, s: &str, x: f32, y: f32, size: f32, align: f32, color: [f32; 4]) {
        ui.text_sh(out, s, self.o[0] + x * self.s, self.o[1] + y * self.s, size, align, color);
    }
    /// Text button (Ui::text_button: the game's red GlowS bar at this size).
    fn button(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, label: &str, x: f32, y: f32, w: f32, h: f32, a: Act, cursor: [f32; 2]) {
        let p = self.r(x, y, w, h);
        widget(out, |o| ui.text_button(o, label, p, Self::inside(p, cursor), false, true));
        self.hits.push((p, a));
    }
    /// Darkened parchment (light text reads on it) in the carved BDR_SH1DEc frame.
    fn panel(&self, ui: &Ui, out: &mut Vec<HudTri>, x: f32, y: f32, w: f32, h: f32) {
        let p = self.r(x, y, w, h);
        widget(out, |o| ui.panel(o, p, self.s));
    }

    /// List rows on this screen (the Save dialog's name box takes the bottom ones).
    fn rows(&self) -> usize { match self.screen { Screen::Lobby(_) => LIST_ROWS, Screen::Solo(_) => SOLO_ROWS, Screen::Save => ROWS - 3, _ => ROWS } }

    /// Load Game as a save browser (ours): Name / Date column headers sort (HUD_LoadSaveRequester), the kind (quick /
    /// auto save, replay) and date per row, Delete on the hovered row (asks first); the hovered file's (else the
    /// first's) preview (save header block 0), map, date and play time on the right. `save`: the Save Game dialog,
    /// our saves only, a click puts the name into the name box; Save (or Return) writes, "Overwrite File?" first when
    /// the name exists.
    fn load_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, save: bool, cursor: [f32; 2]) {
        use crate::saves;
        let rows = self.rows();
        self.panel(ui, out, 300.0, 110.0, 1320.0, 880.0);
        self.text(ui, out, if save { "Save Game" } else { "Load Game" }, 960.0, 185.0, 56.0, 0.5, GOLD);
        // column headers: click = sort by it, again = the other way
        let (by, asc) = *SORT.lock().unwrap();
        for (c, key, x, align, hx, hw) in [(0u8, "TEXT_GAME_OPTIONS_028", 352.0, 0.0, 340.0, 560.0), (1, "TEXT_GAME_OPTIONS_029", 1040.0, 1.0, 900.0, 200.0)] {
            let p = self.r(hx, 212.0, hw, 30.0);
            let hot = Self::inside(p, cursor);
            let arrow = match (by == c, c, asc) { (false, ..) => "", (_, 0, true) => "  (A-Z)", (_, 0, false) => "  (Z-A)", (_, _, true) => "  (oldest first)", _ => "  (newest first)" };
            self.text(ui, out, &format!("{}{arrow}", ui.text_of(key)), x, 235.0, 18.0, align, if hot { GOLD } else { [0.75, 0.72, 0.6, 1.0] });
            self.hits.push((p, Act::Sort(c)));
        }
        if self.files.is_empty() { self.text(ui, out, "No saves found", 720.0, 400.0, 30.0, 0.5, DIM); }
        let mut hover = None;
        for (k, i) in (self.scroll..self.files.len()).take(rows).enumerate() {
            let (y, f) = (250.0 + k as f32 * 34.0, self.files[i].clone());
            let p = self.r(340.0, y, 760.0, 32.0);
            let hot = Self::inside(p, cursor) && self.confirm.is_none();
            let name = f.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
            let chosen = save && name == self.name.trim();
            if hot || chosen { hover = Some(i); row(out, p, hot, chosen); }
            self.text(ui, out, &name, 352.0, y + 24.0, 22.0, 0.0, if hot || chosen { GOLD } else { CREAM });
            let writing = saves::pending(&f); // just saved, still being written: shown, not loadable yet
            let kind = if writing { "Saving\u{2026}" } else { saves::Entry::kind(&f) };
            if !kind.is_empty() { self.text(ui, out, kind, 880.0, y + 23.0, 16.0, 1.0, if kind == "Replay" { [0.62, 0.8, 1.0, 1.0] } else { [0.75, 0.72, 0.6, 1.0] }); }
            if !writing { self.text(ui, out, &saves::entry(&f).date_text(), 1040.0, y + 23.0, 16.0, 1.0, CREAM); }
            if writing { continue; }
            self.hits.push((p, Act::Pick(i)));
            if hot && saves::deletable(&f) { // Delete, on top of the row
                let d = self.r(1046.0, y + 4.0, 50.0, 24.0);
                let t = ui.text_of("TEXT_SCENARIO_020");
                widget(out, |o| ui.text_button(o, &t, d, Self::inside(d, cursor), false, true));
                self.hits.push((d, Act::Delete(i)));
            }
        }
        if self.files.len() > rows {
            let s = format!("{}-{} of {} (scroll)", self.scroll + 1, (self.scroll + rows).min(self.files.len()), self.files.len());
            self.text(ui, out, &s, 1100.0, 250.0 + rows as f32 * 34.0 + 24.0, 18.0, 1.0, DIM);
        }
        if save { // the name box (HUD_LoadSaveRequester FileName), a blinking caret
            self.text(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_028"), 352.0, 835.0, 22.0, 0.0, [0.75, 0.72, 0.6, 1.0]);
            widget(out, |o| Ui::text_field(o, self.r(440.0, 806.0, 660.0, 42.0), true, self.s));
            let caret = if std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis()) % 1000 < 500 { "|" } else { "" };
            let name = self.name.clone();
            self.text(ui, out, &format!("{name}{caret}"), 452.0, 836.0, 24.0, 0.0, CREAM);
        }
        if let Some(i) = hover.or((!self.files.is_empty() && !save).then_some(self.scroll)) {
            let (f, e) = (&self.files[i], saves::entry(&self.files[i]));
            if saves::shown(f) { self.preview = e.preview.clone(); } // main.rs uploads it into texture 2, once per change
            self.frame(out, 1160.0, 230.0, 400.0, 400.0, true);
            let n = crate::live::RADAR_N as f32;
            if e.preview.is_some() { quad(out, 2, self.r(1160.0, 230.0, 400.0, 400.0), [0.0, 0.0, 200.0 / n, 200.0 / n], [1.0; 4]); }
            else { self.text(ui, out, "No preview", 1360.0, 440.0, 22.0, 0.5, DIM); }
            let name = f.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
            self.text(ui, out, &name, 1360.0, 680.0, 28.0, 0.5, GOLD);
            let mut rows = vec![("Map", e.map.clone()), ("Saved", e.date_text())];
            if saves::is_replay(f) { rows.push(("Length", saves::hms(e.ticks as u64 * 25))); } // ticks at 40/s
            rows.push(("Play time", e.play_text()));
            for (k, (l, v)) in rows.into_iter().enumerate() {
                self.text(ui, out, l, 1350.0, 730.0 + k as f32 * 36.0, 22.0, 1.0, [0.75, 0.72, 0.6, 1.0]);
                self.text(ui, out, &v, 1370.0, 730.0 + k as f32 * 36.0, 22.0, 0.0, CREAM);
            }
        }
        if save {
            self.button(ui, out, "Back", 700.0, 910.0, 240.0, 56.0, Act::Back, cursor);
            self.button(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_004"), 980.0, 910.0, 240.0, 56.0, Act::SaveGo, cursor);
        } else {
            self.button(ui, out, "Back", 840.0, 910.0, 240.0, 56.0, Act::Back, cursor);
        }
        if let Some(c) = self.confirm.clone() { self.confirm_popup(ui, out, &c, cursor); }
    }

    /// "Overwrite File?" / "Delete <name>?" over the browser (it takes every click): Yes / No.
    fn confirm_popup(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, c: &Confirm, cursor: [f32; 2]) {
        let all = [0.0, 0.0, 2.0 * self.o[0] + 1920.0 * self.s, 2.0 * self.o[1] + 1080.0 * self.s]; // the whole window
        Ui::fill(out, all, [0.0, 0.0, 0.0, 0.5]);
        self.hits.push((all, Act::Nop));
        self.panel(ui, out, 610.0, 400.0, 700.0, 240.0);
        let (q, p) = match c { Confirm::Overwrite(p) => (ui.text_of("TEXT_GAME_OPTIONS_031"), p), Confirm::Delete(p) => (format!("{}?", ui.text_of("TEXT_SCENARIO_020")), p), Confirm::Leave(_) => return }; // Leave: leave_popup
        self.text(ui, out, &q, 960.0, 470.0, 36.0, 0.5, GOLD);
        self.text(ui, out, &p.file_stem().unwrap_or_default().to_string_lossy(), 960.0, 520.0, 26.0, 0.5, CREAM);
        self.button(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_023"), 700.0, 560.0, 220.0, 52.0, Act::Yes, cursor);
        self.button(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_024"), 1000.0, 560.0, 220.0, 52.0, Act::No, cursor);
    }

    /// The menu's triangles for a `w`×`h` (logical px) window.
    pub fn draw(&mut self, ui: &mut Ui, w: f32, h: f32, cursor: [f32; 2]) -> Vec<HudTri> {
        self.mouse = (cursor, ui.down);
        // a lord's zip dropped on the window (lord_share.rs): Extras shows the result; in the front end only, not in a
        // LAN lobby (it would stop polling)
        if crate::lord_share::SHOW.swap(false, std::sync::atomic::Ordering::Relaxed) && !self.in_game && self.mp.net.is_none() && !matches!(self.screen, Screen::Extras) {
            *self = Menu { screen: Screen::Extras, ..Self::main() };
        }
        // FRONT_Multiplayer is a 1366×768 grid in its own Viewbox; everything else 1920×1080
        let (dw, dh) = match self.screen { Screen::Lobby(_) | Screen::Solo(_) => (1366.0, 768.0), Screen::Briefing(..) => (1365.0, 768.0), _ => (1920.0, 1080.0) };
        self.s = (w / dw).min(h / dh);
        self.o = [(w - dw * self.s) / 2.0, (h - dh * self.s) / 2.0];
        // HUD_Options: its 800×600 grid at (320, 60) ×1.6 over the front end (Viewbox 836k, k = 1.6 at UIScale 1), at
        // (560, 240) ×1 over a game
        if let Screen::Options(op) = &self.screen {
            let (k, px, py) = if op.in_game { (1.0, 560.0, 240.0) } else { (1.6, 320.0, 60.0) };
            self.o = [self.o[0] + px * self.s, self.o[1] + py * self.s];
            self.s *= k;
        }
        // the in-game HUD (hud.rs Ui::draw) drops its glyph cache when ui.s changes: put its scale back afterwards,
        // else a non-1920 screen over a game (briefing) loses its glyphs every frame before they're uploaded
        let hud_scale = (ui.s, ui.w, ui.h);
        (ui.s, ui.w, ui.h) = (self.s, w, h);
        self.hits.clear();
        // front-end music + greeting; the pause menu and the story pages leave their music alone
        crate::audio::menu(!self.in_game && !matches!(self.screen, Screen::Story(_) | Screen::Credits(_)));
        let mut out = vec![];
        let out = &mut out;
        match self.screen {
            Screen::Main | Screen::Load | Screen::Save | Screen::Lobby(_) | Screen::Solo(_) | Screen::Sandbox | Screen::Mp(_) | Screen::Editor(_) | Screen::Extras => backdrop(out, w, h),
            Screen::Campaigns | Screen::Missions(..) | Screen::Story(_) | Screen::Lord(_) => {
                // the screen's 16:9 image, UniformToFill
                let k = (w / 1920.0).max(h / 1080.0);
                quad(out, SCREEN, [(w - 1920.0 * k) / 2.0, (h - 1080.0 * k) / 2.0, (w + 1920.0 * k) / 2.0, (h + 1080.0 * k) / 2.0], SCREEN_UV, [1.0; 4]);
            }
            _ => {}
        }
        match std::mem::replace(&mut self.screen, Screen::Main) {
            Screen::Main => {
                self.screen = Screen::Main;
                self.main_menu(ui, out, cursor);
            }
            Screen::Pause => {
                self.screen = Screen::Pause;
                // the original hides Game Options while its popup asks (No shows it again)
                match self.confirm { Some(Confirm::Leave(exit)) => self.leave_popup(ui, out, exit, cursor), _ => self.game_options(ui, out, cursor) }
            }
            Screen::Campaigns => {
                self.screen = Screen::Campaigns;
                self.campaigns(ui, out, cursor);
            }
            Screen::Missions(c, sel) => {
                self.screen = Screen::Missions(c, sel);
                self.missions_screen(ui, out, c, sel, cursor);
            }
            Screen::Briefing(mut b) => {
                self.briefing_screen(ui, out, &mut b, w, h, cursor);
                self.screen = Screen::Briefing(b);
            }
            Screen::Story(mut st) => {
                self.story_screen(ui, out, &mut st, w, h);
                self.screen = Screen::Story(st);
            }
            Screen::Sandbox => {
                self.screen = Screen::Sandbox;
                self.sandbox(ui, out, cursor);
            }
            Screen::Solo(so) => {
                self.screen = Screen::Solo(so);
                if std::mem::take(&mut self.lst.dirty) { self.solo_rebuild(Some(&*ui)); } // the search / sort changed (lobby.rs)
                let Screen::Solo(so) = self.screen else { unreachable!() };
                self.solo_screen(ui, out, &so, cursor);
            }
            Screen::Trail(t) => {
                self.screen = Screen::Trail(t);
                self.trail_draw(ui, out, t, w, h, cursor);
            }
            Screen::Mp(t) => {
                self.screen = Screen::Mp(t);
                self.mp_draw(ui, out, t, cursor);
            }
            Screen::Options(op) => {
                self.screen = Screen::Options(op);
                self.options_screen(ui, out, &op, w, h, cursor);
            }
            Screen::Load => {
                self.screen = Screen::Load;
                self.load_screen(ui, out, false, cursor);
            }
            Screen::Save => {
                self.screen = Screen::Save;
                self.load_screen(ui, out, true, cursor);
            }
            Screen::Credits(c) => {
                self.screen = Screen::Credits(c);
                self.credits_screen(ui, out, &c, w, h, cursor);
            }
            Screen::Lobby(l) => {
                self.lobby(ui, out, &l, cursor);
                self.screen = Screen::Lobby(l);
            }
            Screen::Help(mut hp) => {
                self.help_draw(ui, out, &mut hp, cursor);
                self.screen = Screen::Help(hp);
            }
            Screen::Editor(t) => {
                self.screen = Screen::Editor(t);
                self.ed_draw(ui, out, t, cursor);
            }
            Screen::Lord(mut p) => {
                self.lord_draw(ui, out, &mut p, w, h, cursor);
                self.screen = Screen::Lord(p);
            }
            Screen::Extras => {
                self.screen = Screen::Extras;
                self.extras_screen(ui, out, cursor);
            }
        }
        if let Some(e) = &self.error { let e = e.clone(); self.text(ui, out, &e, dw / 2.0, dh * 0.972, dh / 41.5, 0.5, [1.0, 0.4, 0.3, 1.0]); }
        (ui.s, ui.w, ui.h) = hud_scale;
        // the colour constants here are linear (converted from the XAML hex); sprite.wgsl now takes sRGB vertex colours
        // and decodes them, so encode once on the way out. ponytail: one pass over the menu's triangles, exact round trip
        encode(out);
        std::mem::take(out)
    }

    /// HUD_IngameMenu (skirmish): 500×438 BDR_SH1DEa panel in the middle, WGT_Heading "Game Options" (32 px black,
    /// Celtic knots, divider), then the BTN_SH_GlowSDisable column (300 wide, every 38 px from y + 82). Options and
    /// Help open their screens; the game stays visible (no dim), paused.
    fn game_options(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let (x, y) = (710.0, 321.0);
        self.frame_with(out, x, y, 500.0, 438.0, true, "Panel_Corner_");
        for kx in [x + 25.0, x + 455.0] { ui.img(out, "Panel_CelticKnot", self.r(kx, y + 11.0, 20.0, 40.0), [1.0; 4]); }
        self.label(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_002"), x + 250.0, y + 34.0, 32.0, 0.5, [0.0, 0.0, 0.0, 1.0]);
        tiled(out, 0, "Panel_Divider", self.r(x + 3.0, y + 56.0, 494.0, 8.0), 250.0 * self.s, 7.5 * self.s, [1.0; 4]);
        let buttons = [("TEXT_GAME_OPTIONS_004", Some(Act::Save)), ("TEXT_GAME_OPTIONS_003", Some(Act::LoadList)), ("TEXT_GAME_OPTIONS_025", Some(Act::Options)),
            ("TEXT_GAME_OPTIONS_027", Some(Act::Help(help::A::Open))), ("TEXT_GAME_OPTIONS_045", self.can_restart.then_some(Act::Restart)), ("TEXT_GAME_OPTIONS_008", Some(Act::ToMenu)),
            ("TEXT_GAME_OPTIONS_010", Some(Act::Quit)), ("TEXT_GAME_OPTIONS_011", Some(Act::Resume))];
        for (k, (key, a)) in buttons.into_iter().enumerate() {
            self.glow(ui, out, &ui.text_of(key), x + 100.0, y + 82.0 + 38.0 * k as f32, 300.0, None, a, cursor);
        }
    }

    /// HUD_ConfirmationPopup.ShowConfirmation: a 450×170 BDR_SH1DEa panel in the middle, WGT_Heading with the title
    /// (TEXT_GAME_OPTIONS_008 Quit Mission / _010 Exit Crusader), Yes / No (BTN_SH_GlowS 100 wide, bottom 42).
    fn leave_popup(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, exit: bool, cursor: [f32; 2]) {
        let (x, y, w, h) = (735.0, 455.0, 450.0, 170.0);
        self.frame_with(out, x, y, w, h, true, "Panel_Corner_");
        for kx in [x + 25.0, x + w - 45.0] { ui.img(out, "Panel_CelticKnot", self.r(kx, y + 11.0, 20.0, 40.0), [1.0; 4]); }
        tiled(out, 0, "Panel_Divider", self.r(x + 3.0, y + 56.0, w - 6.0, 8.0), 250.0 * self.s, 7.5 * self.s, [1.0; 4]);
        let title = ui.text_of(if exit { "TEXT_GAME_OPTIONS_010" } else { "TEXT_GAME_OPTIONS_008" });
        self.label(ui, out, &title, x + w / 2.0, y + 34.0, 30.0, 0.5, [0.0, 0.0, 0.0, 1.0]);
        self.glow(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_023"), x + 80.0, y + h - 70.0, 100.0, None, Some(Act::Yes), cursor);
        self.glow(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_024"), x + w - 180.0, y + h - 70.0, 100.0, None, Some(Act::No), cursor);
    }

    // ---- Historical Campaigns (FrontendMenus.xaml Show_Frontend_Historical, FRONT_Extra<c>Campaign.xaml, HUD_Briefing;
    // spec in the side-session notes, campaign-ui-spec.md)

    /// WGT_Heading2 at design x: Title Banner 715 wide, Celtic knots, the title in 40 px black small caps.
    fn heading(&self, ui: &mut Ui, out: &mut Vec<HudTri>, x: f32, text: &str) {
        ui.img(out, "Title Banner", self.r(x, 5.0, 715.0, 152.7), [1.0; 4]);
        for kx in [x + 45.0, x + 650.0] { ui.img(out, "Panel_CelticKnot", self.r(kx, 45.0, 20.0, 40.0), [1.0; 4]); }
        self.small_caps(ui, out, text, x + 357.5, 83.0, 40.0, [0.0, 0.0, 0.0, 1.0]);
    }

    /// Screen A: the seven campaigns (BTN_NewFrontend 580 wide, overlapping 20 px; yellow = campaign won), help on hover.
    fn campaigns(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let title = ui.text_of("TEXT_SKIRMISH_CHOOSE2_002");
        self.heading(ui, out, 55.0, &title);
        let mut help = None;
        for (c, &(shield, t, d)) in CAMPAIGNS.iter().enumerate() {
            let y = 145.42 + 119.36 * c as f32;
            let name = ui.text_of(&format!("TEXT_CAMPAIGN_INFO_{:03}", 6 * c + 2));
            let done = self.progress.complete(c);
            if self.shield_button(ui, out, &name, shield, 120.0, y, 580.0, 139.36, Some(Act::Campaign(c)), 38.0, done, cursor) { help = Some((t, d, y + 22.36)); }
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::Back), cursor);
        self.img_button(ui, out, ["Load Shield Up", "Load Shield Over", "Load Shield Up"], 1767.0, 957.0, 103.0, 103.0, Some(Act::LoadList), cursor);
        if let Some((t, d, y)) = help {
            let (t, d) = (ui.text_of(t), ui.text_of(d));
            self.help_panel(ui, out, &t, &d, false, 725.0, y, 800.0);
        }
    }

    /// BDR_Wood around design box x, y, w, h: 9-slice at 0.4 (edge strips centred on the box edges, Wood_BL in all
    /// four corners, as the XAML has it).
    fn wood(&self, out: &mut Vec<HudTri>, x: f32, y: f32, w: f32, h: f32) {
        let (s, c) = (self.s, [1.0; 4]);
        tiled(out, ART, "Wood_C", self.r(x, y, w, h), 336.8 * s, 336.8 * s, c);
        tiled(out, ART, "Wood_T", self.r(x + 20.8, y - 30.4, w - 41.6, 60.0), 336.8 * s, 61.0 * s, c);
        tiled(out, ART, "Wood_B", self.r(x + 20.8, y + h - 30.4, w - 41.6, 60.0), 336.8 * s, 61.0 * s, c);
        tiled(out, ART, "Wood_L", self.r(x - 30.4, y + 20.8, 60.0, h - 41.6), 61.0 * s, 336.8 * s, c);
        tiled(out, ART, "Wood_R", self.r(x + w - 30.4, y + 20.8, 60.0, h - 41.6), 61.0 * s, 336.8 * s, c);
        for (cx, cy) in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)] { art_part(out, "Wood_BL", FULL, self.r(cx - 30.4, cy - 30.4, 60.8, 60.8), c); }
    }

    /// BTN_SH_Campaign (×4/3): Button_03 bar, Button_04 when hovered or selected; mission number at +16 and the
    /// name centred, 21.33 px black.
    fn campaign_button(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, num: &str, name: &str, x: f32, y: f32, w: f32, on: bool, a: Act, cursor: [f32; 2]) {
        let p = self.r(x, y, w, 37.33);
        let hot = Self::inside(p, cursor);
        if hot { crate::audio::hover(p, crate::audio::ui_id::RED_BUTTON, 1.0); }
        let n = if hot || on { ["Button_04_Left_", "Button_04_Center", "Button_04_Right"] } else { ["Button_03_Left", "Button_03_Center", "Button_03_Right"] };
        let (bx, by, bw, bh, cap) = (x - 2.0, y - 1.33, w + 4.0, 40.0, 39.33);
        atlas_part(out, n[0], FULL, self.r(bx, by, cap, bh), [1.0; 4]);
        tiled(out, 0, n[1], self.r(bx + cap, by, bw - 2.0 * cap, bh), cap * self.s, bh * self.s, [1.0; 4]);
        atlas_part(out, n[2], FULL, self.r(bx + bw - cap, by, cap, bh), [1.0; 4]);
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        self.label(ui, out, num, x + 16.0, y + 18.7, 21.33, 0.0, BLACK);
        self.label(ui, out, name, x + w / 2.0, y + 18.7, 21.33, 0.5, BLACK);
        self.hits.push((p, a));
    }

    /// Screen B: campaign c's missions on the wood panel; missions past the unlocked one are hidden (not clickable);
    /// a click only selects, Next plays the selection.
    fn missions_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, c: usize, sel: i32, cursor: [f32; 2]) {
        let title = ui.text_of(&format!("TEXT_CAMPAIGN_INFO_{:03}", 6 * c + 2));
        self.heading(ui, out, 150.0, &title);
        self.wood(out, 227.0, 250.0, 560.0, 340.0);
        let mut hovered = false;
        for m in 1..=self.progress.unlocked[c].clamp(1, 5) {
            let id = 5 * c as i32 + m;
            let y = 300.0 + 50.67 * (m - 1) as f32;
            let hot = Self::inside(self.r(273.67, y, 466.67, 37.33), cursor);
            hovered |= hot && self.progress.done.contains_key(&id);
            let name = self.mission_label(ui, id, hot);
            self.campaign_button(ui, out, &m.to_string(), &name, 273.67, y, 466.67, m == sel, Act::Mission(m), cursor);
        }
        if !hovered { self.hover = None; }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::ToCampaigns), cursor);
        self.img_button(ui, out, ["Load Shield Up", "Load Shield Over", "Load Shield Up"], 1567.0, 957.0, 103.0, 103.0, Some(Act::LoadList), cursor);
        self.img_button(ui, out, ["UI-Buttons B001", "UI-Buttons B002", "UI-Buttons B001"], 1709.0, 984.0, 151.0, 66.0, Some(Act::PlayMission), cursor);
    }

    /// A mission list label: the name; for a completed mission under the cursor (FRONT_Extra<c>Campaign rollover)
    /// "Completed : <best difficulty>" from 1 s after the cursor came onto it, then back and forth every second.
    fn mission_label(&mut self, ui: &Ui, id: i32, hot: bool) -> String {
        let name = ui.text_of(&format!("TEXT_MISSION{id}_STORY_002"));
        let Some(&d) = self.progress.done.get(&id).filter(|_| hot) else { return name };
        let since = match self.hover { Some((h, t)) if h == id => t, _ => self.hover.insert((id, Instant::now())).1 };
        if since.elapsed().as_secs() % 2 == 0 { return name; }
        format!("{} : {}", ui.text_of("TEXT_NEW_TEXT2_190"), ui.text_of(&format!("TEXT_MAINOPTIONS_{:03}", 20 + d.clamp(0, 3))))
    }

    /// HUD_Briefing at mission start (1365×768): background, the mission's name, the Objectives page's briefing
    /// text, difficulty (hidden for missions 1, 2, 31-33), Quit / Start and their rollover line.
    /// ponytail: the Help tab is text only.
    fn briefing_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, b: &mut Brief, w: f32, h: f32, cursor: [f32; 2]) {
        let (id, mid) = (b.id, b.mid);
        Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, 1.0]);
        quad(out, SCREEN, self.r(0.0, 0.0, 1365.0, 768.0), SCREEN_UV, [1.0; 4]);
        // a campaign mission's name, or (Custom Scenario) GameData.cachedMissionName = the map's name
        let title = match &self.brief_map { Some(map) if id == 0 => map_name(ui, map), _ => ui.text_of(&format!("TEXT_MISSION{id}_STORY_002")) };
        self.label(ui, out, &title, 682.5, 39.0, 30.0, 0.5, CREAM);
        match b.tab {
            0 => {
                // Objectives heading: the first letter on the P064 stamp, the rest after it
                self.stamped(ui, out, &ui.text_of("TEXT_MISSION_BUTTONS_008"), 319.0, 55.0);
                // FatControler.BriefingUIUpdate: the objective rows (two columns of up to 10 from y 89), the briefing
                // text below them (a Custom Scenario has none: GetMissionBriefing(null) is "" for game type 2)
                let n = self.objective_rows(ui, out);
                let rows = if n <= 9 { n } else if n <= 18 { 9 } else { 10 };
                if id > 0 {
                    let brief = ui.text_of(&format!("TEXT_MISSION{id}_BRIEFING_002"));
                    self.para(ui, out, &brief, 282.5, 134.0 + 25.0 * rows as f32, 560.0, 16.0, CREAM); // BriefingTextWidth 560
                }
                // difficulty: campaign missions but 1, 2, 31-33 (changeable); a scenario shows the one it started at
                if (id == 0 && b.d > 0) || (id > 0 && ![1, 2, 31, 32, 33].contains(&id)) {
                    self.glow(ui, out, "", 382.5, 519.0, 300.0, None, (id > 0 && !mid).then_some(Act::Difficulty), cursor);
                    self.label(ui, out, &ui.text_of("TEXT_MAINOPTIONS_025"), 394.5, 534.0, 16.0, 0.0, [0.905, 0.73, 0.402, 1.0]);
                    self.label(ui, out, &ui.text_of(&format!("TEXT_MAINOPTIONS_{:03}", 20 + b.d)), 670.5, 534.0, 16.0, 1.0, [1.0, 0.68, 0.0, 1.0]); // #FFD700
                }
            }
            // BriefingHintsPanel (1050×492 at 157.5, 68): Strategy, then the hints, each behind a P065 button
            1 => {
                self.stamped(ui, out, &ui.text_of("TEXT_HINTS_002"), 294.0, 55.0);
                self.para(ui, out, &ui.text_of(&format!("TEXT_MISSION{id}_HINTS_002")), 357.5, 99.0, 520.0, 16.0, CREAM);
                let n = NHINTS[id as usize - 1].min(5);
                if n > 0 { self.stamped(ui, out, &ui.text_of("TEXT_HINTS_003"), 294.0, 200.0); }
                for k in 0..n {
                    let y = [244.0, 314.0, 384.0, 454.0, 484.0][k];
                    self.img_button(ui, out, ["UI-Buttons P065", "UI-Buttons P066", "UI-Buttons P065"], 317.5, y + 0.5, 19.0, 19.0, Some(Act::Hint(k + 1)), cursor);
                    let text = if k < b.hints { format!("TEXT_MISSION{id}_HINTS_{:03}", 3 + k) } else { "TEXT_HINTS_005".into() };
                    self.para(ui, out, &ui.text_of(&text), 357.5, y, 530.0, 16.0, CREAM);
                    if k >= b.hints { break; } // the first locked one says "click to reveal"
                }
            }
            // BriefingHelpPanel: the mission's help page (StreamingAssets/Help/mission<id>.html; the original renders it
            // in a web view) in an 800×450 box at 282.5, 44, wheel-scrolled. ponytail: text only, no pictures or links
            _ => {
                self.frame(out, 272.5, 34.0, 820.0, 470.0, false);
                Ui::fill(out, self.r(282.5, 44.0, 800.0, 450.0), [0.0, 0.0, 0.0, 0.5]);
                let (top, bottom) = (44.0, 494.0);
                let mut y = top + 10.0 - b.scroll;
                for (kind, text) in &self.help {
                    let (size, color) = match kind { 0 => (24.0, [1.0, 0.955, 0.604, 1.0]), 1 => (24.0, [0.701, 0.376, 0.017, 1.0]), _ => (16.0, [1.0, 0.955, 0.604, 1.0]) }; // #FFFACC, #DAA520
                    if *kind < 2 {
                        if y >= top && y + size * 1.25 <= bottom { self.label(ui, out, text, 682.5, y + size * 0.62, size, 0.5, color); }
                        y += size * 1.25 + 8.0;
                    } else {
                        y = self.para_n(ui, out, text, 292.5, y, 780.0, size, color, usize::MAX, [top, bottom]) + 8.0;
                    }
                }
                b.scroll = b.scroll.min((y + b.scroll - top - 450.0).max(0.0)); // not past the end
                self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 309.0, 510.5, 107.0, 47.0, Some(Act::HelpBack), cursor);
            }
        }
        // BTN_RADIO_Image tabs: Sprite1 off, Sprite2 on
        let tabs = [("UI-Buttons R011", "UI-Buttons R012", [499.5, 557.5, 88.0, 115.0]), ("UI-Buttons R013", "UI-Buttons R014", [616.5, 557.5, 88.0, 115.0]),
            ("UI-Buttons R015", "UI-Buttons R016", [717.0, 559.5, 153.0, 117.0])];
        for (t, (off, on, [x, y, tw, th])) in tabs.into_iter().enumerate().filter(|_| id > 0) { // no tabs for a scenario
            let sprite = if b.tab == t as u8 { on } else { off };
            self.img_button(ui, out, [sprite, on, sprite], x, y, tw, th, Some(Act::BriefTab(t as u8)), cursor);
        }
        let quit = self.r(309.0, 595.5, 107.0, 47.0);
        let start = self.r(949.0, 595.5, 107.0, 47.0);
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 309.0, 595.5, 107.0, 47.0, Some(Act::BriefQuit), cursor);
        self.img_button(ui, out, ["UI-Buttons B001", "UI-Buttons B002", "UI-Buttons B001"], 949.0, 595.5, 107.0, 47.0, Some(Act::BriefStart), cursor);
        let tab_at = |k: usize| self.r(tabs[k].2[0], tabs[k].2[1], tabs[k].2[2], tabs[k].2[3]);
        let tip = if Self::inside(start, cursor) { "TEXT_BUBBLE_HELP_TEXT_303" } else if Self::inside(quit, cursor) { "TEXT_GAME_OPTIONS_008" }
            else if id > 0 && Self::inside(tab_at(0), cursor) { "TEXT_BUBBLE_HELP_TEXT_306" } else if id > 0 && Self::inside(tab_at(1), cursor) { "TEXT_BUBBLE_HELP_TEXT_305" }
            else if Self::inside(tab_at(2), cursor) { "TEXT_GAME_OPTIONS_027" } else { "" };
        if !tip.is_empty() { self.label(ui, out, &ui.text_of(tip), 682.5, 704.0, 22.0, 0.5, CREAM); }
    }

    /// FatControler.UpdateObjectiveRows into HUD_Briefing's WGT_Objective rows (380×48, 18 px, the HUD's wording,
    /// hud.rs goal_row): x 322.5 for the first 10, 722.5 after, a row every 25 from y 89. Returns how many.
    /// With an objective timer (ObjectiveTimer: its text 18 px, a 204×16 frame #EBE6B5 at x 660.5 with the 200 wide
    /// green fill, now / length) it takes the first row.
    fn objective_rows(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>) -> usize {
        let (goals, campaign, timer) = std::mem::take(&mut self.goals);
        let timer_row = timer.filter(|t| t[1] > 0 && t[2] > 0);
        if let Some([now, len, text]) = timer_row {
            self.label(ui, out, &ui.text_of(&format!("TEXT_OBJECTIVES_{:03}", text + 1)), 322.5, 103.0, 18.0, 0.0, CREAM);
            let frame = [0.83, 0.79, 0.46, 1.0]; // #EBE6B5
            for (fx, fy, fw, fh) in [(660.5, 95.0, 204.0, 2.0), (660.5, 109.0, 204.0, 2.0), (660.5, 95.0, 2.0, 16.0), (862.5, 95.0, 2.0, 16.0)] {
                Ui::fill(out, self.r(fx, fy, fw, fh), frame);
            }
            Ui::fill(out, self.r(662.5, 97.0, 200.0 * now.min(len) as f32 / len as f32, 12.0), [0.019, 0.287, 0.028, 1.0]); // #26922f
        }
        let skip = timer_row.is_some() as usize;
        for (k, g) in goals.iter().enumerate().take(20 - skip) {
            let k = k + skip;
            let (x, top) = (if k < 10 { 322.5 } else { 722.5 }, 89.0 + 25.0 * (k % 10) as f32);
            let (left, right, done) = ui.goal_row(g, campaign);
            self.label(ui, out, &left, x, top + 14.0, 18.0, 0.0, CREAM);
            self.label(ui, out, &right, x + 380.0 - 54.0, top + 14.0, 18.0, 1.0, CREAM);
            let (key, w, h, mr) = if done { ("UI-Buttons R002", 42.0, 43.0, 2.0) } else { ("UI-Buttons R001", 17.0, 17.0, 26.0) };
            ui.img(out, key, self.r(x + 380.0 - mr - w, top - 10.0 + 48.0 - 2.0 - h, w, h), [1.0; 4]);
        }
        let n = goals.len() + skip;
        self.goals = (goals, campaign, timer);
        n
    }

    /// A briefing heading: its first letter on the UI-Buttons P064 stamp (23×28 at x, y), the rest after it, 24 px.
    fn stamped(&self, ui: &mut Ui, out: &mut Vec<HudTri>, text: &str, x: f32, y: f32) {
        ui.img_fit(out, "UI-Buttons P064", self.r(x, y, 23.0, 28.0), [1.0; 4]);
        let mut chars = text.chars();
        if let Some(first) = chars.next() {
            self.label(ui, out, &first.to_string(), x + 11.5, y + 14.0, 24.0, 0.5, CREAM);
            self.label(ui, out, chars.as_str(), x + 25.5, y + 14.0, 24.0, 0.0, CREAM);
        }
    }

    /// FRONT_Story page (1920 grid; the art is the SCREEN image): campaign title 45 px, or the mission's number 32 px
    /// and title 35 px #531414, all centred over x 245..915; the body (StoryFontSize 30, 670 wide at 245, 645) typing in.
    /// MaskGrid: from black over 0.8 s (Intro, cubic ease-out). One full-window click target.
    fn story_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, st: &mut Story, w: f32, h: f32) {
        let c = (st.id - 1) / 5;
        let ink = |a: f32| [0.0, 0.0, 0.0, a];
        let th = SCREEN_H as f32;
        for (tx, tw, h, x, y, w) in STORY_ART { quad(out, SCREEN, self.r(x, y, w, h), [tx / 1920.0, 1080.0 / th, (tx + tw) / 1920.0, (1080.0 + h) / th], [1.0, 1.0, 1.0, 0.95]); }
        let body = if st.campaign {
            let k = c * 6 + if st.aftermath { 5 } else { 2 };
            self.label(ui, out, &ui.text_of(&format!("TEXT_CAMPAIGN_INFO_{k:03}")), 580.0, 120.0 + 45.0 * 0.62, 45.0, 0.5, ink(0.85));
            ui.text_of(&format!("TEXT_CAMPAIGN_INFO_{:03}", k + 1))
        } else {
            let id = st.id;
            self.label(ui, out, &ui.text_of(&format!("TEXT_MISSION{id}_STORY_001")), 580.0, 97.0 + 32.0 * 0.62, 32.0, 0.5, ink(0.85));
            self.label(ui, out, &ui.text_of(&format!("TEXT_MISSION{id}_STORY_002")), 580.0, 150.0 + 35.0 * 0.62, 35.0, 0.5, [0.0865, 0.007, 0.007, 0.85]);
            ui.text_of(&format!("TEXT_MISSION{id}_STORY_003"))
        };
        st.len = body.chars().count();
        let shown = if st.all { usize::MAX } else { st.typed() };
        self.para_n(ui, out, &body, 245.0, 645.0, 670.0, 30.0, ink(if st.campaign { 0.85 } else { 0.75 }), shown, [f32::MIN, f32::MAX]);
        let p = (st.t0.elapsed().as_secs_f32() / 0.8).min(1.0);
        if p < 1.0 { Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, (1.0 - p).powi(3)]); }
        self.hits.push(([0.0, 0.0, w, h], Act::StoryAdvance));
    }

    /// The story is over (clicked through or Escape): its speech stops; the mission loads, or after a campaign's
    /// closing page the campaign list (FrontEndMenu.ButtonClicked("Historical")).
    fn story_end(&mut self) -> Option<Go> {
        crate::audio::story(None, None);
        let Screen::Story(st) = self.screen else { return None };
        if !st.aftermath { return Some(Go::Mission(st.id)); }
        *self = Menu { screen: Screen::Campaigns, progress: skirmish::Progress::load(), ..Self::main() };
        None
    }

    /// Show_Frontend_OtherModes ("Sandbox", 1920 grid): Custom Scenarios and Free Build shields, help on hover, Back / Load.
    fn sandbox(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let title = ui.text_of("TEXT_NEW_CTEXT_199");
        self.heading(ui, out, 602.5, &title);
        let mut help = None;
        for (k, (label, shield, text, x, hx)) in [("TEXT_MAINOPTIONS_031", "shield_custom", "TEXT_NEW_CTEXT_330", 272.0, 282.5),
            ("TEXT_MAINOPTIONS_029", "siege_freebuild", "TEXT_NEW_CTEXT_331", 1010.0, 1025.0)].into_iter().enumerate() {
            let t = ui.text_of(label);
            if self.shield_button(ui, out, &t, shield, x, 464.26, 618.0, 148.49, Some(Act::SoloOpen(k == 0)), 44.0, false, cursor) { help = Some((label, text, hx)); }
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::Back), cursor);
        self.img_button(ui, out, ["Load Shield Up", "Load Shield Over", "Load Shield Up"], 1767.0, 957.0, 103.0, 103.0, Some(Act::LoadList), cursor);
        if let Some((t, d, x)) = help {
            let (t, d) = (ui.text_of(t), ui.text_of(d));
            self.help_panel(ui, out, &t, &d, false, x, 590.0, 600.0);
        }
    }

    /// FRONT_StandaloneMission (1366×768 grid, sandbox-ui-spec.md): title + Next carousel, the map list, map info and
    /// radar, Difficulty (Custom Scenarios) or Advanced / Remove Hostile Animals and the options panel (Free Build),
    /// Back / Play; the list's search box, sortable Size / Name / Date headers and (Custom Scenarios) Show Completed.
    /// ponytail: no Workshop maps.
    fn solo_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, so: &Solo, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        const ACCENT7: [f32; 4] = [0.905, 0.73, 0.402, 1.0]; // #F4DEAA
        let tx = |ui: &Ui, k: &str| ui.text_of(k);
        Ui::fill(out, self.r(0.0, 0.0, 1366.0, 768.0), BLACK);
        pic(out, "lobby", self.r(0.0, 0.0, 1366.0, 768.0), 1.0);
        let (title, other) = if so.invasion { ("TEXT_MAINOPTIONS_031", "TEXT_BUBBLE_HELP_TEXT_265") } else { ("TEXT_BUBBLE_HELP_TEXT_265", "TEXT_MAINOPTIONS_031") };
        self.frame(out, 425.5, 25.0, 515.0, 60.0, true);
        for x in [450.5, 895.5] { ui.img(out, "Panel_CelticKnot", self.r(x, 35.0, 20.0, 40.0), [1.0; 4]); }
        self.small_caps(ui, out, &tx(ui, title), 683.0, 68.0, 40.0, BLACK);
        // the "Next" carousel: a small header with the other mode's name, 60 % (90 % hovered), and the arrows
        let next = self.r(983.0, 36.56, 300.0, 34.95);
        let a = if Self::inside(next, cursor) { 0.9 } else { 0.6 };
        self.frame(out, 983.0, 36.56, 300.0, 34.95, true);
        Ui::fill(out, next, [0.0, 0.0, 0.0, 1.0 - a]);
        self.small_caps(ui, out, &tx(ui, other), 1133.0, 62.0, 23.3, [0.0, 0.0, 0.0, a]);
        self.hits.push((next, Act::SoloNext));
        self.img_button(ui, out, ["Button_Arrow_Left", "Button_Arrow_Left_Over", "Button_Arrow_Left"], 10.0, 23.0, 60.0, 60.0, None, cursor); // Prev: no handler
        self.img_button(ui, out, ["Button_Arrow", "Button_Arrow_Over", "Button_Arrow"], 1306.0, 23.0, 60.0, 60.0, Some(Act::SoloNext), cursor);

        // map list: type, size, name, date; 34 px rows under a 29 px header
        self.frame(out, 47.0, 120.0, 473.0, 496.0, true);
        let (x0, y0) = (52.0, 125.0);
        Ui::fill(out, self.r(x0, y0, 463.0, 29.0), [0.125, 0.006, 0.0, 1.0]); // #631200
        self.label(ui, out, &tx(ui, "TEXT_GAME_OPTIONS_028"), x0 + 80.0 + 152.0, y0 + 14.5, 15.0, 0.5, ACCENT7);
        self.label(ui, out, &tx(ui, "TEXT_GAME_OPTIONS_029"), x0 + 384.0 + 40.0, y0 + 14.5, 15.0, 0.5, ACCENT7);
        // FileListHeaderClickedHandler: Size, Name, Date (the Type column sorts nothing)
        for (x, w, c) in [(40.0, 40.0, 3), (80.0, 304.0, 0), (384.0, 80.0, 1)] { self.hits.push((self.r(x0 + x, y0, w, 29.0), Act::Lobby(lobby::A::Sort(c)))); }
        let completed = so.invasion && self.lst.completed;
        for (k, j) in (self.scroll..self.files.len()).take(SOLO_ROWS).enumerate() {
            let y = y0 + 29.0 + 34.0 * k as f32;
            let p = self.r(x0, y, 463.0, 34.0);
            if so.sel == Some(j) || Self::inside(p, cursor) {
                Ui::fill(out, p, if so.sel == Some(j) { [0.356, 0.014, 0.002, 1.0] } else { [0.462, 0.027, 0.004, 1.0] }); // #A11F08 / #B52E0C
            }
            let i = self.solo[j];
            // the type, or with Show Completed won / not won (GameSprites 368 / 369)
            let icon = match (completed, i.user) { (true, _) if self.progress.map_done(&self.files[j]) => "Slider_Thumb", (true, _) => "notcompleted", (_, true) => "user", _ => "ff" };
            ui.img_fit(out, icon, self.r(x0 + 3.0, y + 2.0, 30.0, 30.0), [1.0; 4]);
            if i.size >= 0 { self.label(ui, out, &i.size.to_string(), x0 + 40.0, y + 17.0, 16.0, 0.0, BLACK); }
            self.label(ui, out, &map_name(ui, &self.files[j]), x0 + 80.0, y + 17.0, 16.0, 0.0, BLACK);
            if let Some(d) = std::fs::metadata(&self.files[j]).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
                self.label(ui, out, &date(d.as_secs() as i64), x0 + 384.0 + 40.0, y + 17.0, 11.0, 0.5, BLACK);
            }
            self.hits.push((p, Act::Pick(j)));
        }
        // include boxes: user maps, built-in maps (Workshop drawn, nothing to include)
        for (k, (x, icon, on)) in [(46.0, "user", so.user), (104.0, "ff", so.builtin), (164.0, "steam", true)].into_iter().enumerate() {
            self.checkbox(out, x, 630.0, on, (k < 2).then_some(Act::Include(k as u8)));
            ui.img_fit(out, icon, self.r(x + 22.0, 625.0, 30.0, 30.0), [1.0; 4]);
        }
        self.search_box(ui, out, 230.0, 624.0, true, cursor);
        if so.invasion {
            // Show Completed: a 450×23 CheckBox_Template with its 20 px text (TEXT_NEW_TEXT2_181)
            self.checkbox(out, 46.0, 671.5, self.lst.completed, Some(Act::Lobby(lobby::A::Completed)));
            self.label(ui, out, &tx(ui, "TEXT_NEW_TEXT2_181"), 72.0, 682.0, 20.0, 0.0, BLACK);
            self.hits.push((self.r(46.0, 670.0, 450.0, 23.0), Act::Lobby(lobby::A::Completed)));
        }

        // map info + radar
        let info = so.sel.map(|i| self.solo[i]);
        self.frame(out, 561.0, 120.0, 515.0, 200.0, true);
        if let Some(i) = so.sel {
            let name = map_name(ui, &self.files[i]);
            self.label(ui, out, &name, 818.5, 141.0, 18.0, 0.5, BLACK);
            let info = self.solo[i];
            let size = if (160..=800).contains(&info.size) { info.size.to_string() } else { "?".into() };
            let animals = info.animals > 0 && !so.invasion;
            let x = if animals { 721.0 } else { 781.0 };
            ui.img_fit(out, "Map", self.r(x, 157.0, 26.0, 26.0), [1.0; 4]);
            self.label(ui, out, &size, x + 33.0, 170.0, 16.0, 0.0, BLACK);
            if animals {
                ui.img_fit(out, "Animals", self.r(891.0, 157.0, 26.0, 26.0), [1.0; 4]);
                self.label(ui, out, &info.animals.to_string(), 924.0, 170.0, 16.0, 0.0, BLACK);
            }
            let brief = skirmish::map_text_keys(&self.files[i]).filter(|k| !k.1.is_empty()).map(|k| tx(ui, &k.1)).or(self.brief.clone()).unwrap_or_default();
            self.para_n(ui, out, &brief, 601.0, 195.0, 435.0, 16.0, BLACK, usize::MAX, [195.0, 315.0]);
        }
        self.frame(out, 1116.0, 120.0, 200.0, 200.0, true);
        if so.sel.is_some() {
            let n = crate::live::RADAR_N as f32;
            quad(out, 2, self.r(1116.0, 120.0, 200.0, 200.0), [0.0, 0.0, 200.0 / n, 200.0 / n], [1.0; 4]);
            if let Some(size @ (160 | 300 | 500 | 700)) = info.map(|i| i.size) { ui.img(out, &format!("Border {size}"), self.r(1116.0, 120.0, 200.0, 200.0), [1.0; 4]); }
        }
        self.frame(out, 1116.0, 120.0, 200.0, 200.0, false);

        if so.invasion {
            self.glow(ui, out, "", 1023.0, 340.0, 300.0, None, Some(Act::SoloDifficulty), cursor);
            self.label(ui, out, &tx(ui, "TEXT_MAINOPTIONS_025"), 1035.0, 355.0, 16.0, 0.0, ACCENT7);
            self.label(ui, out, &tx(ui, &format!("TEXT_MAINOPTIONS_{:03}", 20 + so.difficulty)), 1311.0, 355.0, 16.0, 1.0, [1.0, 0.68, 0.0, 1.0]);
        } else if let Some(i) = info {
            self.glow(ui, out, &tx(ui, "TEXT_XPLAY_WAITING_ROOM_075"), 556.0, 340.0, 200.0, None, Some(Act::Advanced), cursor);
            if i.animals > 0 {
                self.checkbox(out, 776.0, 343.5, so.fb.remove_animals, Some(Act::Flag(0)));
                self.label(ui, out, &tx(ui, "TEXT_NEW_CTEXT_262"), 802.0, 355.0, 18.0, 0.0, BLACK);
            }
            if so.fb.advanced { self.free_build_options(ui, out, &so.fb, cursor); }
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 43.0, 700.0, 106.0, 47.0, Some(Act::Back), cursor);
        self.img_button(ui, out, ["UI-Buttons B001", "UI-Buttons B002", "UI-Buttons B004"], 1216.0, 700.0, 107.0, 47.0, so.sel.map(|_| Act::SoloPlay), cursor);
    }

    /// The Free Build options panel (561, 381, 755×305): starting goods left, events / invasions / opponents / peace right,
    /// the extreme toggles, Defeat on Lord's Death. Left click = next value, right click = previous (wrapping).
    fn free_build_options(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, fb: &skirmish::FreeBuild, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        let tx = |ui: &Ui, k: &str| ui.text_of(k);
        self.frame(out, 561.0, 381.0, 755.0, 305.0, true);
        self.label(ui, out, &tx(ui, "TEXT_XPLAY_WAITING_ROOM_017"), 751.0, 403.0, 20.0, 0.5, BLACK);
        self.label(ui, out, &tx(ui, "TEXT_NEW_TEXT2_098"), 1126.0, 403.0, 20.0, 0.5, BLACK);
        let goods = |v: i32| if v < 0 { "TEXT_NEW_CTEXT_283".to_string() } else { format!("TEXT_NEW_TEXT2_{:03}", [108, 119, 109, 110, 111, 112][v as usize]) };
        let interval = |v: i32| format!("TEXT_NEW_TEXT2_{:03}", [102, 101, 120, 121, 103, 104, 105, 106, 122][v as usize]);
        let rows = [
            (581.0, 416.0, "TEXT_GOODS_016".to_string(), goods(fb.goods[0])),
            (581.0, 455.0, "TEXT_XPLAY_WAITING_ROOM_012".into(), goods(fb.goods[1])),
            (581.0, 494.0, "TEXT_XPLAY_WAITING_ROOM_014".into(), goods(fb.goods[2])),
            (581.0, 533.0, "TEXT_XPLAY_WAITING_ROOM_011".into(), goods(fb.goods[3])),
            (956.0, 416.0, "TEXT_SCENARIO_175".into(), interval(fb.events)),
            (956.0, 455.0, "TEXT_NEW_TEXT2_097".into(), interval(fb.invasions)),
            (956.0, 494.0, format!("TEXT_NEW_TEXT2_{}", if fb.invasion_difficulty < 6 { "099" } else { "186" }),
                format!("TEXT_NEW_TEXT2_{:03}", [113, 114, 115, 116, 117, 118, 187, 188, 189][fb.invasion_difficulty as usize])),
            (956.0, 572.0, "TEXT_NEW_TEXT2_100".into(), format!("TEXT_NEW_TEXT2_{:03}", [102, 103, 104, 105, 106, 122, 129][fb.peacetime as usize])),
        ];
        for (f, (x, y, label, value)) in rows.into_iter().enumerate() {
            self.glow(ui, out, "", x, y, 340.0, None, Some(Act::Cycle(f as u8)), cursor);
            self.label(ui, out, &tx(ui, &label), x + 12.0, y + 15.0, 16.0, 0.0, [0.905, 0.73, 0.402, 1.0]); // #F4DEAA
            self.label(ui, out, &tx(ui, &value), x + 328.0, y + 15.0, 16.0, 1.0, [1.0, 0.68, 0.0, 1.0]); // #FFD700
        }
        // opponent factions: ToggleBTN_SH_GlowS, checked = the glow bar
        for (x, bit, key) in [(956.0, 1, "TEXT_SCENARIO_224"), (1072.0, 2, "TEXT_SCENARIO_225"), (1189.0, 4, "TEXT_NEW_CTEXT_221")] {
            let on = fb.opponents & bit != 0;
            let cursor = if on { [self.o[0] + (x + 50.0) * self.s, self.o[1] + 545.0 * self.s] } else { cursor }; // lit when on
            self.glow(ui, out, &tx(ui, key), x, 533.0, 107.0, None, Some(Act::Faction(bit)), cursor);
        }
        for (x, key, on, f) in [(611.0, "TEXT_SKIRMISH_MISC_018", fb.extreme_troops, 1), (986.0, "TEXT_SKIRMISH_MISC_017", fb.extreme_powers, 2)] {
            self.glow(ui, out, &tx(ui, key), x, 611.0, 280.0, Some(if on { "UI-Buttons S100" } else { "UI-Buttons S043" }), Some(Act::Flag(f)), cursor);
        }
        let defeat = tx(ui, "TEXT_NEW_CTEXT_445");
        let tw = ui.text(&mut vec![], &defeat, 0.0, 0.0, 18.0, 1.0, 0.0, 0.0, BLACK) / self.s;
        let x = 561.0 + (755.0 - (26.0 + tw)) / 2.0;
        self.checkbox(out, x, 649.5, fb.defeat_on_death, Some(Act::Flag(3)));
        self.label(ui, out, &defeat, x + 26.0, 660.0, 18.0, 0.0, BLACK);
    }

    /// CheckBox_Template (Ui::checkbox), 20×20 at design x, y; hovered / held while the cursor is on the box.
    fn checkbox(&mut self, out: &mut Vec<HudTri>, x: f32, y: f32, on: bool, a: Option<Act>) {
        let p = self.r(x, y, 20.0, 20.0);
        let hot = a.is_some() && Self::inside(p, self.mouse.0);
        widget(out, |o| Ui::checkbox(o, p, on, hot, hot && self.mouse.1));
        if let Some(a) = a { self.hits.push((p, a)); }
    }

    /// Options closed (Back / Esc): settings.cfg is already saved; back to the pause menu or the main menu.
    fn close_options(&mut self) -> Option<Go> {
        *self = if self.in_game { Self::pause(self.can_restart) } else { Self::main() };
        Some(Go::Settings(None))
    }
    fn save_cfg(&mut self) {
        if let Err(e) = self.cfg.save() { self.error = Some(format!("settings.cfg: {e}")); }
        *cfg() = Some(self.cfg.clone());
    }
    /// An Options slider's position from settings.cfg (volumes ×100, game speed /5).
    fn slider_value(&self, key: &str) -> i32 {
        match key {
            "GameSpeed" => self.cfg.game_speed() as i32 / 5,
            "ScrollSpeed" => (self.cfg.num(key, 5.0) as i32).clamp(0, 15),
            _ => (self.cfg.num(key, if key == "MasterVolume" { 0.8 } else { 1.0 }).clamp(0.0, 1.0) * 100.0).round() as i32,
        }
    }
    /// Slider moved: settings.cfg (volume = v / 100, GameSpeed = v × 5), saved; volumes apply at once.
    fn set_slider(&mut self, key: &str, v: i32) {
        match key {
            "GameSpeed" => self.cfg.set(key, v * 5),
            "ScrollSpeed" => self.cfg.set(key, v),
            _ => self.cfg.set(key, v as f32 / 100.0),
        }
        self.save_cfg();
        let (m, mu, sp, us, sfx, red) = self.cfg.volumes();
        crate::audio::set_volumes(m, mu, sp, us, sfx, red);
    }
    fn check_on(&self, i: usize) -> bool {
        let (_, key, _, default, ..) = CHECKS[i];
        let (key, inv) = key.strip_prefix('!').map_or((key, false), |k| (k, true));
        self.cfg.flag(key, default ^ inv) ^ inv
    }

    /// OptionsHotKeyPanel over the whole options panel (#44444444, it blocks the clicks under it): BDR_SH1DEb 100, 175,
    /// 600×250, the row's name as its heading. `slot` -1: Hotkey 1 / Hotkey 2 buttons and Close; 0 / 1: Press Key, the
    /// current and the new key, who loses the new key, Apply / Clear / Cancel.
    fn hotkey_popup(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, keys: &crate::keys::Keys, row: usize, slot: i8, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        let tx = |ui: &Ui, k: &str| ui.text_of(k);
        let all = self.r(0.0, 0.0, 800.0, 600.0);
        Ui::fill(out, all, [0.058, 0.058, 0.058, 0x44 as f32 / 255.0]);
        self.hits.push((all, Act::Nop));
        self.frame(out, 100.0, 175.0, 600.0, 250.0, true);
        for kx in [125.0, 655.0] { ui.img(out, "Panel_CelticKnot", self.r(kx, 186.0, 20.0, 40.0), [1.0; 4]); }
        self.label(ui, out, &hotkey_label(ui, row), 400.0, 209.0, 32.0, 0.5, BLACK);
        tiled(out, 0, "Panel_Divider", self.r(103.0, 231.0, 594.0, 8.0), 250.0 * self.s, 7.5 * self.s, [1.0; 4]);
        let func = &hotkeys()[row].0;
        let cur = keys.get(func);
        let name = |ui: &Ui, k: &str| if k.is_empty() { tx(ui, "TEXT_NEW_TEXT_094") } else { key_name(ui, k) };
        if slot < 0 {
            for (k, x) in [164.0, 436.0].into_iter().enumerate() {
                self.label(ui, out, &tx(ui, ["TEXT_NEW_TEXT_092", "TEXT_NEW_TEXT_093"][k]), x + 100.0, 286.0, 24.0, 0.5, BLACK);
                let t = name(ui, cur[k]);
                self.glow(ui, out, &t, x, 319.0, 200.0, None, Some(Act::HkSlot(k as u8)), cursor);
            }
            self.glow(ui, out, &tx(ui, "TEXT_NEW_TEXT_091"), 475.0, 375.0, 200.0, None, Some(Act::HkClose), cursor);
        } else {
            self.label(ui, out, &tx(ui, "TEXT_NEW_TEXT_095"), 400.0, 259.0, 24.0, 0.5, BLACK);
            self.label(ui, out, &tx(ui, "TEXT_NEW_TEXT_096"), 264.0, 286.0, 24.0, 0.5, BLACK);
            self.label(ui, out, &tx(ui, "TEXT_NEW_TEXT_097"), 536.0, 286.0, 24.0, 0.5, BLACK);
            self.label(ui, out, &name(ui, cur[slot as usize]), 264.0, 326.0, 24.0, 0.5, BLACK);
            let new = self.hk_new.clone();
            if let Some(k) = &new {
                self.label(ui, out, &key_name(ui, k), 536.0, 326.0, 24.0, 0.5, BLACK);
                // SetNewKey takes the key off its other function: say which
                if let Some(other) = keys.owner(k).filter(|f| f != func) {
                    let who = hotkeys().iter().position(|h| h.0 == other).map_or(other.to_string(), |i| hotkey_label(ui, i));
                    self.label(ui, out, &format!("{} {who}", tx(ui, "TEXT_NEW_TEXT_098")), 400.0, 356.0, 18.0, 0.5, BLACK);
                }
            }
            self.glow(ui, out, &tx(ui, "TEXT_GAME_OPTIONS_019"), 150.0, 375.0, 150.0, None, new.is_some().then_some(Act::HkApply), cursor);
            self.glow(ui, out, &tx(ui, "TEXT_NEW_TEXT_099"), 325.0, 375.0, 150.0, None, Some(Act::HkClear), cursor);
            self.glow(ui, out, &tx(ui, "TEXT_SCENARIO_037"), 500.0, 375.0, 150.0, None, Some(Act::HkCancel), cursor);
        }
    }

    /// BTN_SH_GlowSB: GlowS that can be selected (the lit bar, #FFD700 text).
    fn glow_sel(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, text: &str, x: f32, y: f32, w: f32, selected: bool, a: Option<Act>, cursor: [f32; 2]) {
        let lit = if selected { [self.o[0] + (x + w / 2.0) * self.s, self.o[1] + (y + 14.0) * self.s] } else { cursor };
        self.glow(ui, out, "", x, y, w, None, a, lit);
        let color = if selected { [1.0, 0.68, 0.0, 1.0] } else { [0.905, 0.73, 0.402, if a.is_some() { 1.0 } else { 0.7 }] };
        self.label(ui, out, text, x + w / 2.0, y + 15.0, 16.0, 0.5, color);
    }

    /// HUD_Options (800×600 grid): BDR_SH1DEa frame, "Game Options", the section column, Back, and the page (350, 70,
    /// 397×510; Video / Sound / Controls scroll in 73..577, Hotkeys lists 179 rows). ponytail: no resolution list, UI
    /// scale slider, cursor icons, lords, Cheats.
    fn options_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, op: &Opt, w: f32, h: f32, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        let tx = |ui: &Ui, k: &str| ui.text_of(k);
        if !op.in_game {
            backdrop(out, w, h);
            Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0xAA as f32 / 255.0]); // Show_HUD_FrontEndBlackout
        }
        self.frame_with(out, 0.0, 0.0, 800.0, 600.0, true, "Panel_Corner_");
        for kx in [25.0, 755.0] { ui.img(out, "Panel_CelticKnot", self.r(kx, 11.0, 20.0, 40.0), [1.0; 4]); }
        self.label(ui, out, &tx(ui, "TEXT_GAME_OPTIONS_002"), 400.0, 34.0, 32.0, 0.5, BLACK);
        tiled(out, 0, "Panel_Divider", self.r(3.0, 56.0, 794.0, 8.0), 250.0 * self.s, 7.5 * self.s, [1.0; 4]);
        // section column (GlowSB), chicken (Cheats, not ported), Back
        let sections = [("TEXT_GAME_OPTIONS_005", Some(0)), ("TEXT_GAME_OPTIONS_006", Some(1)), ("TEXT_NEW_TEXT_058", Some(2)), ("TEXT_NEW_TEXT_077", Some(3)),
            ("TEXT_NEW_TEXT2_141", Some(4)), ("TEXT_NEW_CTEXT_373", Some(6))];
        for (k, (key, page)) in sections.into_iter().enumerate() {
            self.glow_sel(ui, out, &tx(ui, key), 45.0, 100.0 + 38.0 * k as f32, 260.0, page == Some(op.page), page.map(Act::OptPage), cursor);
        }
        // View Patch Notes: the original opens Steam's news page; ours shows this app's release notes (update.rs)
        self.glow_sel(ui, out, &tx(ui, "TEXT_NEW_CTEXT_429"), 45.0, 328.0, 260.0, op.page == 5, Some(Act::OptPage(5)), cursor);
        self.glow(ui, out, &tx(ui, "TEXT_NEW_TEXT_082"), 45.0, 366.0, 260.0, None, Some(Act::OptFolder), cursor);
        self.img_button(ui, out, ["chicken1", "chicken2", "chicken1"], 115.0, 405.0, 122.0, 118.0, None, cursor);
        self.glow(ui, out, &tx(ui, "TEXT_GAME_OPTIONS_018"), 75.0, 541.0, 200.0, None, Some(Act::OptBack), cursor);
        // the page: #44444444 fill, 2 px black edge
        let pg = self.r(350.0, 70.0, 397.0, 510.0);
        Ui::fill(out, pg, [0.058, 0.058, 0.058, 0x44 as f32 / 255.0]);
        let e = 2.0 * self.s;
        for b in [[pg[0], pg[1], pg[2], pg[1] + e], [pg[0], pg[3] - e, pg[2], pg[3]], [pg[0], pg[1], pg[0] + e, pg[3]], [pg[2] - e, pg[1], pg[2], pg[3]]] { Ui::fill(out, b, BLACK); }
        let sc = if op.page < 3 { op.scroll } else { 0.0 };
        let vis = |y: f32, hh: f32| y - sc >= 73.0 && y - sc + hh <= 577.0; // inside the ScrollViewer
        let title = ["TEXT_GAME_OPTIONS_005", "TEXT_GAME_OPTIONS_006", "TEXT_NEW_TEXT_058", "TEXT_NEW_TEXT_077", "TEXT_GAME_OPTIONS_046", "TEXT_NEW_CTEXT_429", ""][op.page as usize];
        let ty = if op.page == 3 { 80.0 } else if op.page == 4 { 78.0 } else { 83.0 };
        if (vis(ty, 30.0) || op.page >= 3) && op.page != 6 { self.label(ui, out, &tx(ui, title), 548.5, ty + 15.0 - sc, 24.0, 0.5, BLACK); }
        for (i, &(page, _, label, _, x, y)) in CHECKS.iter().enumerate() {
            if page != op.page || !vis(y, 23.0) { continue; }
            let (on, row, p) = (self.check_on(i), self.r(x, y - sc, 350.0, 23.0), self.r(x, y + 1.5 - sc, 20.0, 20.0));
            let hot = Self::inside(row, cursor); // the label is part of the box (CheckBox content)
            widget(out, |o| Ui::checkbox(o, p, on, hot, hot && ui.down));
            self.hits.push((row, Act::Check(i as u8)));
            self.label(ui, out, &tx(ui, label), x + 26.0, y + 13.5 - sc, 20.0, 0.0, BLACK);
        }
        for (i, &(page, key, label, lo, hi, tick, bx, by)) in SLIDERS.iter().enumerate() {
            if page != op.page || !vis(by, 60.0) { continue; }
            let by = by - sc;
            Ui::fill(out, self.r(bx, by, 350.0, 60.0), [0.102, 0.006, 0.003, 0x22 as f32 / 255.0]); // #225A130A
            let v = self.slider_value(key);
            self.label(ui, out, &tx(ui, label), bx + 5.0, by + 17.0, 20.0, 0.0, BLACK);
            let shown = if key == "GameSpeed" { v * 5 } else { v };
            self.label(ui, out, &shown.to_string(), bx + 341.0, by + 17.0, 20.0, 1.0, BLACK);
            let (sx, sy) = (bx + 137.0, by + 29.0);
            Ui::fill(out, self.r(sx, sy + 9.0, 208.0, 2.0), BLACK);
            let mut t = lo;
            while t <= hi { Ui::fill(out, self.r(sx + 10.0 + (t - lo) as f32 / (hi - lo) as f32 * 188.0 - 0.5, sy + 20.0, 1.0, 6.0), BLACK); t += tick; }
            let tx0 = sx + (v - lo) as f32 / (hi - lo) as f32 * 188.0;
            ui.img_fit(out, "Slider_Thumb", self.r(tx0, sy, 20.0, 20.0), [1.0; 4]);
            self.hits.push((self.r(sx, sy, 208.0, 26.0), Act::Slider(i as u8)));
        }
        match op.page {
            // Video: screen mode (cycles; Apply switches), the window size, Apply, checks; Scroll Speed + Reset below
            0 => {
                let modes = ["TEXT_NEW_TEXT_089", "TEXT_NEW_TEXT_101", "TEXT_NEW_TEXT_102"];
                // Resolution: the window's size; windowed mode picks one of WINDOW_SIZES (full screen = the screen's)
                let size = match (op.mode, op.size) { (2, Some([sw, sh])) => format!("{sw} x {sh}"), _ => format!("{} x {}", w.round(), h.round()) };
                for (y, label, value, a) in [(132.0, "TEXT_NEW_TEXT_103", tx(ui, modes[op.mode as usize]), Some(Act::ScreenMode)),
                    (177.0, "TEXT_NEW_TEXT_104", size, (op.mode == 2).then_some(Act::WinSize))] {
                    if !vis(y, 32.0) { continue; }
                    self.label(ui, out, &tx(ui, label), 374.0, y + 16.0 - sc, 16.0, 0.0, BLACK);
                    let p = self.r(540.0, y - sc, 182.0, 32.0);
                    Ui::fill(out, p, [0.25, 0.27, 0.29, 1.0]); // #6D7378 edge
                    Ui::fill(out, [p[0] + self.s, p[1] + self.s, p[2] - self.s, p[3] - self.s], [0.125, 0.006, 0.0, 1.0]); // #631200
                    self.label(ui, out, &value, 546.0, y + 16.0 - sc, 16.0, 0.0, [0.905, 0.73, 0.402, if a.is_some() { 1.0 } else { 0.6 }]);
                    if let Some(a) = a { self.hits.push((p, a)); }
                }
                if (op.mode != op.applied || op.size.is_some()) && vis(248.0, 28.0) { self.glow(ui, out, &tx(ui, "TEXT_GAME_OPTIONS_019"), 520.0, 248.0 - sc, 200.0, None, Some(Act::ApplyMode), cursor); }
                if vis(703.0, 28.0) { self.glow(ui, out, &tx(ui, "TEXT_NEW_TEXT_088"), 470.0, 703.0 - sc, 250.0, None, Some(Act::Reset(0)), cursor); }
            }
            1 => if vis(723.0, 28.0) { self.glow(ui, out, &tx(ui, "TEXT_NEW_TEXT_088"), 446.5, 723.0 - sc, 250.0, None, Some(Act::Reset(1)), cursor) },
            // Controls: the four GlowSB pairs, game speed, checks, Reset
            2 => {
                for (g, &(key, title, a, b, default, y)) in GROUPS.iter().enumerate() {
                    let on = self.cfg.flag(key, default);
                    if vis(y, 105.0) {
                        Ui::fill(out, self.r(376.0, y - sc, 345.0, 105.0), [0.102, 0.006, 0.003, 0x22 as f32 / 255.0]);
                        self.label(ui, out, &tx(ui, title), 548.5, y + 12.0 - sc, 19.0, 0.5, BLACK);
                        self.glow_sel(ui, out, &tx(ui, a), 378.5, y + 24.13 - sc, 340.0, on, Some(Act::Group(g as u8, true)), cursor);
                        self.glow_sel(ui, out, &tx(ui, b), 378.5, y + 62.13 - sc, 340.0, !on, Some(Act::Group(g as u8, false)), cursor);
                    }
                }
                if vis(728.48, 28.0) { self.glow(ui, out, &tx(ui, "TEXT_NEW_TEXT_088"), 467.0, 728.48 - sc, 250.0, None, Some(Act::Reset(2)), cursor); }
            }
            // Hotkeys: the list (name, "K1 / K2"), 27 px rows, wheel-scrolled; the presets (rebinding not ported)
            3 => {
                let keys = crate::keys::Keys::from_block(&self.cfg.keys);
                let first = (op.scroll / 27.0) as usize;
                for (k, (func, ..)) in hotkeys().iter().enumerate().skip(first).take(12) {
                    let y = 120.0 + 27.0 * (k - first) as f32;
                    let row = self.r(354.0, y, 389.0, 27.0);
                    let sel = op.hk.is_some_and(|(r, _)| r as usize == k);
                    if sel || Self::inside(row, cursor) { Ui::fill(out, row, if sel { [0.356, 0.014, 0.002, 1.0] } else { [0.462, 0.027, 0.004, 1.0] }); }
                    self.label(ui, out, &hotkey_label(ui, k), 359.0, y + 13.5, 14.0, 0.0, BLACK);
                    self.label(ui, out, &bound_text(ui, keys.get(func)), 574.0, y + 13.5, 14.0, 0.0, BLACK);
                    self.hits.push((row, Act::HkRow(k as u16)));
                }
                self.glow(ui, out, &tx(ui, "TEXT_NEW_TEXT_066"), 373.0, 482.0, 350.0, None, Some(Act::HkPreset(false)), cursor);
                self.glow(ui, out, &tx(ui, "TEXT_NEW_TEXT_067"), 373.0, 522.0, 350.0, None, Some(Act::HkPreset(true)), cursor);
                if let Some((r, slot)) = op.hk { self.hotkey_popup(ui, out, &keys, r as usize, slot, cursor); }
            }
            // View Patch Notes: the latest release's notes (GitHub, fetched with the update check), wheel-scrolled
            5 => {
                let (top, bottom) = (118.0, 572.0);
                let mut y = top - op.scroll;
                match crate::update::notes() {
                    Some((tag, body)) => {
                        let ours = if tag.trim_start_matches('v') == crate::update::VERSION { " (this version)".to_string() } else { format!(" (you have {})", crate::update::VERSION) };
                        if y >= top { self.label(ui, out, &format!("{tag}{ours}"), 548.5, y + 10.0, 18.0, 0.5, [0.25, 0.0, 0.0, 1.0]); }
                        y += 30.0;
                        for (kind, line) in notes_lines(&body) {
                            let (x, size, color) = match kind { 0 => (362.0, 17.0, [0.25, 0.0, 0.0, 1.0]), 1 => (376.0, 14.0, BLACK), _ => (362.0, 14.0, BLACK) };
                            if kind == 1 && y >= top && y + 18.0 <= bottom { self.label(ui, out, "\u{2022}", 366.0, y + 9.0, 14.0, 0.0, BLACK); }
                            y = self.para_n(ui, out, &line, x, y, 735.0 - x, size, color, usize::MAX, [top, bottom]) + if kind == 0 { 6.0 } else { 2.0 };
                        }
                    }
                    None => { self.para(ui, out, "No release notes yet: they come with the update check (online, once a day).", 362.0, y, 373.0, 16.0, BLACK); }
                }
                // not past the end
                let end = (y + op.scroll - top - (bottom - top)).max(0.0);
                if let Screen::Options(o) = &mut self.screen { o.scroll = o.scroll.min(end); }
            }
            6 => self.coa_page(ui, out, op.coa, op.scroll, cursor),
            // Player: the name box (click to type), Custom Intros, the single-player colour shields (front end only)
            _ => {
                let name = coa::player_name();
                let p = self.r(398.5, 108.5, 300.0, 35.0);
                widget(out, |o| Ui::text_field(o, p, op.typing, self.s));
                let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                let caret = if op.typing && ms / 500 % 2 == 0 { "|" } else { "" };
                self.label(ui, out, &format!("{name}{caret}"), 548.5, 126.0, 18.0, 0.5, [0.905, 0.73, 0.402, 1.0]); // TextAlignment Center
                self.hits.push((p, Act::NameBox));
                if !op.in_game {
                    self.label(ui, out, &tx(ui, "TEXT_NEW_TEXT2_142"), 548.5, 212.0, 20.0, 0.5, BLACK);
                    let col = self.cfg.num("PlayerColour", 0.0) as i32;
                    let col = if (0..8).contains(&col) { col } else { 0 };
                    for (i, n) in [16, 10, 13, 7, 19, 22, 25, 28].into_iter().enumerate() {
                        let sp = |d: i32| format!("UI-Buttons H{:03}", n + d);
                        let (a, b, c) = (sp(0), sp(1), sp(2));
                        let normal = if i as i32 == col { c.as_str() } else { a.as_str() };
                        self.img_button(ui, out, [normal, b.as_str(), normal], 408.0 + 37.0 * i as f32, 235.8, 32.0, 33.0, Some(Act::Colour(i as i32)), cursor);
                    }
                }
            }
        }
    }

    /// Show_Frontend_MainMenu: logo, the six mode buttons, Load / Options, tutorial book, credits, exit door.
    fn main_menu(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        // LogoMain 1050×202 at 10,10, Stretch=Uniform (centred)
        let Some(&[_, _, lw, lh]) = art().2.get("logo") else { return };
        let k = (1050.0 / lw).min(202.0 / lh);
        pic(out, "logo", self.r(10.0 + (1050.0 - lw * k) / 2.0, 10.0, lw * k, lh * k), 1.0);
        // version TextBlock: bottom left, 14 px, white at 40 %
        self.text(ui, out, &format!("native port {}", crate::update::VERSION), 4.0, 1074.0, 14.0, 0.0, [1.0, 1.0, 1.0, 0.4]);
        // ours: a newer release on GitHub (update.rs), left of the door column, clear of the mode help panels (x 570)
        if let Some(tag) = crate::update::available() {
            self.button(ui, out, &crate::update::button(&tag), 40.0, 890.0, 500.0, 48.0, Act::Update, cursor);
        }
        // ours: a newer version of an installed custom lord in the `lords` release (lord_share.rs)
        if let n @ 1.. = crate::lord_share::updates() {
            let what = if n == 1 { "A custom lord has an update".to_string() } else { format!("{n} custom lords have updates") };
            self.button(ui, out, &format!("{what} \u{2014} Extras"), 40.0, 778.0, 500.0, 48.0, Act::Extras(extras::A::Open), cursor);
        }
        // ours: steam_files.txt grew since the setup's Steam download (setup/mod.rs update_todo)
        if let Some(u) = crate::setup::pending() {
            let size = if u.bytes > 0 { format!(" ({} MB)", u.bytes.div_ceil(1_000_000)) } else { String::new() };
            self.button(ui, out, &format!("New game files available{size} \u{2014} Download"), 40.0, 834.0, 500.0, 48.0, Act::GameFiles, cursor);
        }
        let mut help = None;
        for (i, &(label, shield, text, a)) in MAIN.iter().enumerate() {
            let y = 40.0 + 150.0 * i as f32;
            let t = ui.text_of(label);
            if self.shield_button(ui, out, &t, shield, 1082.0, y, 618.0, 148.5, a, 44.0, false, cursor) {
                help = Some((label, text, a.is_none(), 570.0, y + 20.0));
            }
        }
        // bottom row: Load Game / Game Options 380×103, 455 px in from the sides, bottom aligned
        let (load, options) = (ui.text_of("TEXT_MAINOPTIONS_005"), ui.text_of("TEXT_GAME_OPTIONS_002"));
        self.shield_button(ui, out, &load, "shield_load", 455.0, 977.0, 380.0, 103.0, Some(Act::LoadList), 44.0, false, cursor);
        self.shield_button(ui, out, &options, "shield_options", 1085.0, 977.0, 380.0, 103.0, Some(Act::Options), 44.0, false, cursor);
        // BTN_Image: sprite, rollover sprite on hover
        let atlas = |m: &mut Self, ui: &Ui, out: &mut Vec<HudTri>, s: [&str; 2], p: [f32; 4]| {
            let p = m.r(p[0], p[1], p[2], p[3]);
            let hot = Self::inside(p, cursor);
            ui.img_fit(out, s[hot as usize], p, [1.0; 4]);
            hot
        };
        if atlas(self, ui, out, ["btn_book", "btn_book_rollover"], [1730.0, 983.0, 130.0, 77.0]) { help = Some(("", "", false, 1360.0, 920.0)); }
        self.hits.push((self.r(1730.0, 983.0, 130.0, 77.0), Act::Tutorial)); // MainMenu_TutorialButton -> InitNewScene(Tutorial)
        // the Firefly logo: ButtonMainMenuCommand("Credits") -> FRONT_Credits
        atlas(self, ui, out, ["Firefly_1", "Firefly_2"], [883.5, 968.0, 153.0, 102.0]);
        self.hits.push((self.r(883.5, 968.0, 153.0, 102.0), Act::Credits));
        let door = self.r(60.0, 970.0, 63.0, 80.0);
        if Self::inside(door, cursor) { crate::audio::hover(door, crate::audio::ui_id::EXIT, 1.0); }
        pic(out, if Self::inside(door, cursor) { "btn_exit_rollover_solo" } else { "btn_exit_solo" }, door, 1.0);
        self.hits.push((door, Act::Quit));
        if let Some((title, text, soon, x, y)) = help {
            let title = if title.is_empty() { String::new() } else { ui.text_of(title) };
            let text = if text.is_empty() { String::new() } else { ui.text_of(text) };
            self.help_panel(ui, out, &title, &text, soon, x, y, 500.0);
        }
    }

    /// BTN_NewFrontend (720×173 art in a Viewbox) fitted into the design box x, y, w, h: shield bar, the mode's
    /// shield on its boss, the label (44 px small caps, white at 70 %). Hover: rollover bar, ×1.02, text 90 %.
    /// `a` None = not ported: drawn the same, no click. Returns whether the cursor is on it.
    /// `font` = FrontEndButtonFontSize in the 720 grid (44 main menu, 38 campaigns); `yellow` = the completed-campaign bar.
    fn shield_button(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, label: &str, shield: &str, x: f32, y: f32, w: f32, h: f32, a: Option<Act>, font: f32, yellow: bool, cursor: [f32; 2]) -> bool {
        let k = (w / 720.0).min(h / 173.0);
        let (cx, cy) = (x + (w - 720.0 * k) / 2.0, y + (h - 173.0 * k) / 2.0);
        let hot = Self::inside(self.r(cx, cy, 720.0 * k, 173.0 * k), cursor);
        if hot { crate::audio::hover(self.r(cx, cy, 720.0 * k, 173.0 * k), crate::audio::ui_id::SHIELD, if matches!(shield, "shield_load" | "shield_options") { 0.5 } else { 1.0 }); }
        let z = if hot { 1.02 } else { 1.0 };
        let mid = [cx + 360.0 * k, cy + 86.5 * k];
        let at = |gx: f32, gy: f32| [mid[0] + (cx + gx * k - mid[0]) * z, mid[1] + (cy + gy * k - mid[1]) * z];
        let g = |m: &Self, gx: f32, gy: f32, gw: f32, gh: f32| { let p = at(gx, gy); m.r(p[0], p[1], gw * k * z, gh * k * z) };
        let bar = match (hot, yellow) { (false, false) => "btn_shield_bar_01", (true, false) => "btn_shield_bar_01_rollover", (false, true) => "btn_shield_bar_01_yellow", (true, true) => "btn_shield_bar_01_rollover_yellow" };
        pic(out, bar, g(self, 0.0, 0.0, 720.0, 173.0), 1.0);
        ui.img_fit(out, shield, g(self, 25.0, 16.5, 93.0, 102.0), [1.0; 4]);
        // text grid 550×80, Margin 132,0,0,20, centred: caps centre ~ grid centre
        let p = at(407.0, 76.5 + font * 0.33);
        self.small_caps(ui, out, label, p[0], p[1], font * k * z, [1.0, 1.0, 1.0, if hot { 0.9 } else { 0.7 }]);
        if let Some(a) = a { self.hits.push((self.r(cx, cy, 720.0 * k, 173.0 * k), a)); }
        hot
    }

    /// Typography.Capitals="SmallCaps": capitals at `size`, lower case as capitals at 78 % (fontdue has no smcp).
    fn small_caps(&self, ui: &mut Ui, out: &mut Vec<HudTri>, s: &str, cx: f32, y: f32, size: f32, color: [f32; 4]) {
        let runs: Vec<(String, f32)> = s.chars().map(|c| if c.is_lowercase() { (c.to_uppercase().collect(), size * 0.78) } else { (c.to_string(), size) }).collect();
        let width: f32 = runs.iter().map(|(t, z)| ui.text(&mut vec![], t, 0.0, 0.0, *z, 1.0, 0.0, 0.0, color)).sum();
        let mut x = self.o[0] + cx * self.s - width / 2.0;
        for (t, z) in &runs { x += ui.text(out, t, x, self.o[1] + y * self.s, *z, 1.0, 0.0, 0.0, color); }
    }

    /// Mode help (DockPanel left of the column): #cc000000 box, #816D18 border, title 28 px, text 24 px #EFF3C6,
    /// 500 wide (main column: x 570); `soon` adds the not-ported note.
    /// `w` wide (main menu 500, centred lines; campaigns 800, left-aligned).
    fn help_panel(&self, ui: &mut Ui, out: &mut Vec<HudTri>, title: &str, text: &str, soon: bool, x: f32, y: f32, w: f32) {
        let mut lines: Vec<(String, f32, [f32; 4])> = vec![];
        if !title.is_empty() { lines.push((title.to_string(), 28.0, [1.0; 4])); }
        for (para, size, color) in [(text, 24.0, CREAM), (if soon { NOT_YET } else { "" }, 24.0, GOLD)] {
            let mut line = String::new();
            for word in para.split_whitespace() {
                let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if ui.text(&mut vec![], &next, 0.0, 0.0, size, 1.0, 0.0, 0.0, color) > (w - 20.0) * self.s && !line.is_empty() {
                    lines.push((std::mem::replace(&mut line, word.to_string()), size, color));
                } else {
                    line = next;
                }
            }
            if !line.is_empty() { lines.push((line, size, color)); }
        }
        let h = 5.0 + lines.iter().map(|l| l.1 * 1.25).sum::<f32>();
        let p = self.r(x, y, w, h);
        let e = 2.0 * self.s;
        Ui::fill(out, [p[0] + e, p[1] + e, p[2] - e, p[3] - e], [0.0, 0.0, 0.0, 0.8]);
        let gold = [0.22, 0.153, 0.009, 1.0]; // #816D18
        for b in [[p[0], p[1], p[2], p[1] + e], [p[0], p[3] - e, p[2], p[3]], [p[0], p[1], p[0] + e, p[3]], [p[2] - e, p[1], p[2], p[3]]] { Ui::fill(out, b, gold); }
        let mut ty = y + 3.0;
        for (s, size, color) in lines {
            ty += size * 1.25;
            if w > 700.0 { self.text(ui, out, &s, x + 10.0, ty - size * 0.3, size, 0.0, color) } else { self.text(ui, out, &s, x + w / 2.0, ty - size * 0.3, size, 0.5, color) }
        }
    }

    fn inside(p: [f32; 4], c: [f32; 2]) -> bool { c[0] >= p[0] && c[0] < p[2] && c[1] >= p[1] && c[1] < p[3] }

    /// File list (`files`, from `scroll`), one clickable row each; highlights `sel`.
    fn list_sel(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, x: f32, y: f32, w: f32, sel: Option<&PathBuf>, cursor: [f32; 2]) {
        for (k, i) in (self.scroll..self.files.len()).take(ROWS).enumerate() {
            let name = self.files[i].file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
            let p = self.r(x, y + k as f32 * 34.0, w, 32.0);
            let hot = cursor[0] >= p[0] && cursor[0] < p[2] && cursor[1] >= p[1] && cursor[1] < p[3];
            let on = sel == Some(&self.files[i]);
            row(out, p, hot, on);
            self.text(ui, out, &name, x + 12.0, y + k as f32 * 34.0 + 24.0, 24.0, 0.0, if on { GOLD } else { CREAM });
            self.hits.push((p, Act::Pick(i)));
        }
        if self.files.len() > ROWS {
            let s = format!("{}-{} of {} (scroll)", self.scroll + 1, (self.scroll + ROWS).min(self.files.len()), self.files.len());
            self.text(ui, out, &s, x + w, y + ROWS as f32 * 34.0 + 26.0, 20.0, 1.0, DIM);
        }
    }
    fn list(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, x: f32, y: f32, w: f32, cursor: [f32; 2]) { self.list_sel(ui, out, x, y, w, None, cursor) }

    // ---- FRONT_Multiplayer (skirmish), 1366×768 design px; layout from FRONT_Multiplayer.xaml (spec in the
    // side-session notes, lobby-layout.md)

    /// BDR_SH1DEb around design rect x, y, w, h: parchment (Panel_BG tiled 256², 89 %) inside, 30 px tiled border
    /// strips (18 px out, 12 px in), 30 px corners. `centre` false = BDR_SH1DEc (frame only).
    fn frame(&self, out: &mut Vec<HudTri>, x: f32, y: f32, w: f32, h: f32, centre: bool) { self.frame_with(out, x, y, w, h, centre, "Panel_Corner_2_") }
    /// `corner` = sprite prefix: "Panel_Corner_2_" (BDR_SH1DEb / c) or "Panel_Corner_" (BDR_SH1DEa).
    fn frame_with(&self, out: &mut Vec<HudTri>, x: f32, y: f32, w: f32, h: f32, centre: bool, corner: &str) {
        let (s, c) = (self.s, [1.0; 4]);
        if centre { tiled(out, 0, "Panel_BG", self.r(x + 2.0, y + 2.0, w - 4.0, h - 4.0), 256.0 * s, 256.0 * s, [1.0, 1.0, 1.0, 0.89]); }
        tiled(out, 0, "Panel_Border_Top", self.r(x - 1.0, y - 18.0, w + 2.0, 30.0), 250.0 * s, 30.0 * s, c);
        tiled(out, 0, "Panel_Border_Bottom", self.r(x - 1.0, y + h - 12.0, w + 2.0, 30.0), 250.0 * s, 30.0 * s, c);
        tiled(out, 0, "Panel_Border_Left", self.r(x - 18.0, y - 1.0, 30.0, h + 2.0), 30.0 * s, 250.0 * s, c);
        tiled(out, 0, "Panel_Border_Right", self.r(x + w - 12.0, y - 1.0, 30.0, h + 2.0), 30.0 * s, 250.0 * s, c);
        for (k, cx, cy) in [("Top_Left", x - 18.0, y - 18.0), ("Top_Right", x + w - 12.0, y - 18.0), ("Bottom_Left", x - 18.0, y + h - 12.0), ("Bottom_Right", x + w - 12.0, y + h - 12.0)] {
            atlas_part(out, &format!("{corner}{k}"), FULL, self.r(cx, cy, 30.0, 30.0), c);
        }
    }

    /// TextBlock without effect, vertically centred on `mid` (caps centre; Junicode).
    fn label(&self, ui: &mut Ui, out: &mut Vec<HudTri>, s: &str, x: f32, mid: f32, size: f32, align: f32, color: [f32; 4]) -> f32 {
        ui.text(out, s, self.o[0] + x * self.s, self.o[1] + (mid + size * 0.33) * self.s, size, 1.0, 0.0, align, color)
    }

    /// Word-wrapped `label` lines from `top`, `width` wide, line pitch 1.25 × size. Returns the bottom.
    fn para(&self, ui: &mut Ui, out: &mut Vec<HudTri>, s: &str, x: f32, top: f32, width: f32, size: f32, color: [f32; 4]) -> f32 {
        self.para_n(ui, out, s, x, top, width, size, color, usize::MAX, [f32::MIN, f32::MAX])
    }
    /// `para` showing only its first `shown` letters (typing text; the layout is the whole text's) and only the lines
    /// inside design rows `clip` (a scrolled box).
    fn para_n(&self, ui: &mut Ui, out: &mut Vec<HudTri>, s: &str, x: f32, top: f32, width: f32, size: f32, color: [f32; 4], mut shown: usize, clip: [f32; 2]) -> f32 {
        let (mut y, mut line) = (top, String::new());
        let fits = |ui: &mut Ui, t: &str| ui.text(&mut vec![], t, 0.0, 0.0, size, 1.0, 0.0, 0.0, color) <= width * self.s;
        let mut emit = |ui: &mut Ui, out: &mut Vec<HudTri>, line: &str, y: f32| {
            let part: String = line.chars().take(shown).collect();
            if !part.is_empty() && y >= clip[0] && y + size * 1.25 <= clip[1] { self.label(ui, out, &part, x, y + size * 0.62, size, 0.0, color); }
            shown = shown.saturating_sub(line.chars().count() + 1);
        };
        for word in s.split_whitespace() {
            let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && !fits(ui, &next) {
                emit(ui, out, &line, y);
                (y, line) = (y + size * 1.25, word.to_string());
            } else {
                line = next;
            }
        }
        if !line.is_empty() { emit(ui, out, &line, y); y += size * 1.25; }
        y
    }

    /// BTN_Image: sprite fitted (Uniform, centred) into the design rect; hover = `s[1]`. `a` None = disabled / not
    /// ported: `s[2]`, no click.
    fn img_button(&mut self, ui: &Ui, out: &mut Vec<HudTri>, s: [&str; 3], x: f32, y: f32, w: f32, h: f32, a: Option<Act>, cursor: [f32; 2]) {
        let p = self.r(x, y, w, h);
        if a.is_some() && Self::inside(p, cursor) { crate::audio::hover(p, crate::audio::ui_id::RED_BUTTON, 1.0); }
        let key = match a { None => s[2], Some(_) if Self::inside(p, cursor) => s[1], Some(_) => s[0] };
        ui.img_fit(out, key, p, [1.0; 4]);
        if let Some(a) = a { self.hits.push((p, a)); }
    }

    /// BTN_SH_GlowS(Image): 3-slice bar (caps 29.5 wide, 31 high from y − 1.5), text 16 px #F4DEAA at +12 or
    /// centred, GlowSImage's sprite 29×29 at +250. `a` None = disabled (text 60 %).
    fn glow(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, text: &str, x: f32, y: f32, w: f32, tick: Option<&str>, a: Option<Act>, cursor: [f32; 2]) {
        self.glow_at(ui, out, text, x, y, w, tick.map(|t| (t, 250.0)), a, 1.0, cursor)
    }
    /// `glow` with the sprite at `tick.1` (Sprite1Margin) and the whole button at `alpha` (Opacity).
    fn glow_at(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, text: &str, x: f32, y: f32, w: f32, tick: Option<(&str, f32)>, a: Option<Act>, alpha: f32, cursor: [f32; 2]) {
        let p = self.r(x, y - 1.5, w, 31.0);
        let hot = a.is_some() && Self::inside(p, cursor);
        if hot { crate::audio::hover(p, crate::audio::ui_id::RED_BUTTON, 1.0); }
        let held = hot && ui.down;
        let c = [1.0, 1.0, 1.0, alpha];
        ui.glow_bar(out, p, hot, true, if held { [0.6, 0.6, 0.6, alpha] } else { c }); // linear: Ui::text_button's 0.8
        let color = [0.905, 0.73, 0.402, alpha * if a.is_none() { 0.6 } else if held { 0.9 } else { 1.0 }]; // #F4DEAA
        let y = y + held as u8 as f32;
        match tick {
            Some((t, tx)) => { self.label(ui, out, text, x + 12.0, y + 15.0, 16.0, 0.0, color); ui.img_fit(out, t, self.r(x + tx, y - 0.5, 29.0, 29.0), c); }
            None => { self.label(ui, out, text, x + w / 2.0, y + 15.0, 16.0, 0.5, color); }
        }
        if let Some(a) = a { self.hits.push((p, a)); }
    }

    /// The custom-skirmish setup screen.
    fn lobby(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, l: &Lobby, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        let tx = |ui: &Ui, k: &str| ui.text_of(k);
        self.lobby_refresh(ui, &l.map); // the search / sort changed: the list again (lobby.rs)
        let sel = self.files.iter().position(|f| *f == l.map);
        let info = sel.and_then(|i| self.infos.get(i)).or_else(|| self.lst.info(&l.map)).cloned();
        let positions = info.as_ref().map_or(0, |i| i.players.min(8)) as usize;
        // Basemap (victory_military_small, UniformToFill) on the grid's black
        Ui::fill(out, self.r(0.0, 0.0, 1366.0, 768.0), BLACK);
        pic(out, "lobby", self.r(0.0, 0.0, 1366.0, 768.0), 1.0);
        // header: "Setup", 40 px black small caps, Celtic knots
        self.frame(out, 410.5, 25.0, 545.0, 60.0, true);
        for x in [435.5, 910.5] { ui.img(out, "Panel_CelticKnot", self.r(x, 35.0, 20.0, 40.0), [1.0; 4]); }
        self.small_caps(ui, out, &tx(ui, "TEXT_NEW_TEXT2_057"), 683.0, 68.0, 40.0, BLACK);

        // players: 8 rows of 28 px, the AI column on the right
        self.frame(out, 47.0, 120.0, 563.0, 231.0, true);
        Ui::fill(out, self.r(515.0, 125.0, 90.0, 221.0), [0.604, 0.604, 0.604, 0.533]); // #88CCCCCC
        let nh = self.mp_humans(); // 1, or the humans of a LAN game
        let room = nh + l.ais.len() < positions.max(2);
        let add = (room && !self.add_ai).then_some(Act::AddAiOpen);
        self.img_button(ui, out, ["SkirmishSetup 015", "SkirmishSetup 016", "SkirmishSetup 015"], 519.0, 135.0, 82.0, 59.0, add, cursor);
        self.img_button(ui, out, ["SkirmishSetup 038", "SkirmishSetup 039", "SkirmishSetup 038"], 528.0, 205.0, 65.0, 51.0, None, cursor);
        self.img_button(ui, out, ["SkirmishSetup 017", "SkirmishSetup 018", "SkirmishSetup 017"], 521.0, 270.0, 78.0, 47.0, (!self.add_ai).then_some(Act::TeamsOpen), cursor);
        let human = self.mp_rows(ui).unwrap_or_else(|| vec![(tx(ui, "TEXT_XPLAY_WAITING_ROOM_403"), coa::player_name())]);
        let rows: Vec<(String, String)> = human.into_iter().chain(l.ais.iter().map(|&(n, _)| { let (a, b) = lord_keys(n); (tx(ui, &a), tx(ui, &b)) })).collect();
        // member i's colour id: your Options colour, the AIs the free ones (Lobby::colour_ids); a LAN lobby in row order
        let cids = l.colour_ids();
        let colour = |i: usize| if nh > 1 { i } else { cids.get(i).map_or(i, |&c| (c - 1) as usize) }.min(7);
        for (i, (kind, name)) in rows.iter().enumerate().take(8) {
            let y = 125.0 + 28.0 * i as f32;
            Ui::fill(out, self.r(52.0, y, 463.0, 28.0), if i % 2 == 0 { [0.604, 0.604, 0.604, 0.533] } else { [0.402, 0.402, 0.402, 0.533] }); // #88CCCCCC / #88AAAAAA
            ui.img_fit(out, COLOUR_SHIELDS[colour(i)], self.r(53.0, y + 1.0, 26.0, 26.0), [1.0; 4]);
            self.label(ui, out, kind, 92.0, y + 14.0, 15.0, 0.0, BLACK);
            self.label(ui, out, name, 202.0, y + 14.0, 15.0, 0.0, BLACK);
            if i >= nh { self.img_button(ui, out, ["UI-Buttons L009", "UI-Buttons L010", "UI-Buttons L009"], 488.0, y + 1.0, 26.0, 26.0, Some(Act::DelAi(i - nh)), cursor); }
            // a custom lord's backstory (lord_info.rs)
            if let Some(&(n, _)) = l.ais.get(i.wrapping_sub(nh)).filter(|a| crate::custom_lords::lord_of(a.0).is_some()) {
                self.info_badge(ui, out, 458.0, y + 4.0, 20.0, Act::LordInfo(n), cursor);
            }
        }

        // map info + radar
        self.frame(out, 638.0, 120.0, 418.0, 232.0, true);
        let keys = skirmish::map_text_keys(&l.map);
        let title = keys.as_ref().map_or_else(|| l.map.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned()), |k| tx(ui, &k.0));
        self.label(ui, out, &title, 847.0, 141.0, 18.0, 0.5, BLACK);
        if let Some(i) = &info {
            ui.img_fit(out, "Map", self.r(698.0, 157.0, 26.0, 26.0), [1.0; 4]);
            self.label(ui, out, &i.size.to_string(), 731.0, 170.0, 16.0, 0.0, BLACK);
            ui.img_fit(out, "player count", self.r(868.0, 157.0, 26.0, 26.0), [1.0; 4]);
            self.label(ui, out, &i.players.to_string(), 901.0, 170.0, 16.0, 0.0, BLACK);
            ui.img_fit(out, if i.balanced { "balanced" } else { "Unbalanced" }, self.r(698.0, 190.0, 26.0, 26.0), [1.0; 4]);
            self.label(ui, out, &tx(ui, if i.balanced { "TEXT_XPLAY_WAITING_ROOM_412" } else { "TEXT_XPLAY_WAITING_ROOM_413" }), 731.0, 204.0, 16.0, 0.0, BLACK);
            if i.outposts {
                ui.img_fit(out, "Outpost", self.r(868.0, 190.0, 26.0, 26.0), [1.0; 4]);
                self.label(ui, out, &tx(ui, "TEXT_SKIRMISH_MISC_016"), 901.0, 204.0, 16.0, 0.0, BLACK);
            }
        }
        let brief = keys.as_ref().filter(|k| !k.1.is_empty()).map(|k| tx(ui, &k.1)).or(self.brief.clone()).unwrap_or_default();
        self.para(ui, out, &brief, 668.0, 232.0, 358.0, 16.0, BLACK);
        self.frame(out, 1084.0, 120.0, 232.0, 232.0, true);
        let n = crate::live::RADAR_N as f32;
        quad(out, 2, self.r(1084.0, 120.0, 232.0, 232.0), [0.0, 0.0, 200.0 / n, 200.0 / n], [1.0; 4]);
        // the play-area border of the 160 / 300 / 500 / 700 sizes (Show_Radar*Border; none for 400 / 600 / 800)
        if let Some(size @ (160 | 300 | 500 | 700)) = info.as_ref().map(|i| i.size) {
            ui.img(out, &format!("Border {size}"), self.r(1084.0, 120.0, 232.0, 232.0), [1.0; 4]);
        }
        self.frame(out, 1084.0, 120.0, 232.0, 232.0, false);
        // keep shields: start position k -> member order[k] (skirmish::setup; Customize sets the mission's)
        let order = l.keeps.unwrap_or(std::array::from_fn(|k| k as i32));
        for (k, &(kx, ky)) in info.iter().flat_map(|i| i.keeps.iter()).enumerate().take(positions) {
            if kx < 0 { continue; }
            let at = |v: i32, d: i32| ((v - d) as f32 * 232.0 / 200.0).clamp(4.0, 212.0);
            let m = usize::try_from(order[k]).ok().filter(|&m| m < rows.len());
            let s = m.map_or("UI-Buttons H039", |m| KEEP_SHIELDS[colour(m)]);
            let (sx, sy) = (1084.0 + at(kx, 4), 120.0 + at(ky, 6));
            ui.img_fit(out, s, self.r(sx, sy, 16.0, 16.0), [1.0; 4]);
            if let Some(&t) = m.and_then(|m| teams(l).get(m).copied()).as_ref().filter(|&&t| t > 0) {
                ui.img_fit(out, &format!("Team Allies Shield {t} Large"), self.r(sx + 14.0, sy + 8.0, 12.0, 12.0), [1.0; 4]);
            }
        }

        if !self.add_ai {
            self.map_list(ui, out, sel, cursor);
            self.options(ui, out, l, info.as_ref().is_some_and(|i| i.outposts), cursor);
        }

        // bottom: Back / Play hands, Skirmish Masters, Load, Options
        let back = if self.add_ai { Act::AddAiClose } else if self.teams_open { Act::TeamsClose } else { Act::Back };
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 43.0, 700.0, 106.0, 47.0, Some(back), cursor);
        let ok = (!l.ais.is_empty() || nh > 1) && nh + l.ais.len() <= positions && self.mp_can_start();
        if let Some(why) = self.mp_missing_lords() { self.para(ui, out, &why, 700.0, 655.0, 500.0, 15.0, [0.6, 0.1, 0.05, 1.0]); } // LAN: Start waits for everyone's custom lords
        self.img_button(ui, out, ["UI-Buttons B001", "UI-Buttons B002", "UI-Buttons B004"], 1216.0, 700.0, 107.0, 47.0, ok.then_some(Act::Start), cursor);
        self.img_button(ui, out, ["SkirmishMasters", "SkirmishMastersOver", "SkirmishMasters"], 870.0, 670.0, 96.0, 88.0, None, cursor);
        self.img_button(ui, out, ["MPLoad", "MPLoad_over", "MPLoad"], 1026.0, 684.0, 60.0, 60.0, Some(Act::LoadList), cursor);
        self.img_button(ui, out, ["Settings Up", "Settings Over", "Settings Up"], 1126.0, 684.0, 60.0, 60.0, None, cursor);
        if self.add_ai { self.add_ai_popup(ui, out, cursor); } // over the bottom row (z-order), it covers Back
        if self.teams_open { self.teams_popup(ui, out, l, cursor); }
        if self.lst.adv { self.lobby_advanced(ui, out, l, cursor); } // lobby.rs
        if self.lst.mode { self.lobby_modes(ui, out, l, cursor); } // lobby.rs
    }

    /// The map ListView: header (#631200, 15 px #F4DEAA), 34 px rows (selected #A11F08, hover #B52E0C); columns
    /// type 40, players 30, size 40, balanced 30, name 340, date 80 (clipped at the 552 px viewport).
    fn map_list(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, sel: Option<usize>, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        const ACCENT7: [f32; 4] = [0.905, 0.73, 0.402, 1.0]; // #F4DEAA
        self.frame(out, 47.0, 384.0, 562.0, 270.0, true);
        let (x0, y0) = (52.0, 389.0);
        Ui::fill(out, self.r(x0, y0, 552.0, 29.0), [0.125, 0.006, 0.0, 1.0]); // #631200
        let col = |c: usize| x0 + [0.0, 40.0, 70.0, 110.0, 140.0, 480.0][c];
        self.label(ui, out, "#", col(1) + 15.0, y0 + 14.5, 15.0, 0.5, ACCENT7);
        ui.img_fit(out, "Unbalanced", self.r(167.0, 394.0, 18.0, 18.0), [1.0; 4]);
        self.label(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_028"), col(4) + 170.0, y0 + 14.5, 15.0, 0.5, ACCENT7);
        self.label(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_029"), col(5) + 36.0, y0 + 14.5, 15.0, 0.5, ACCENT7);
        // the column headers sort (FileListHeaderClickedHandler: Players, Size, Balanced, Name, Date); not in a LAN lobby
        if self.mp.net.is_none() {
            for (c, sort, w) in [(1, 2, 30.0), (2, 3, 40.0), (3, 5, 30.0), (4, 0, 340.0), (5, 1, 72.0)] {
                self.hits.push((self.r(col(c), y0, w, 29.0), Act::Lobby(lobby::A::Sort(sort))));
            }
        }
        for (k, j) in (self.scroll..self.files.len()).take(LIST_ROWS).enumerate() {
            let y = y0 + 29.0 + 34.0 * k as f32;
            let p = self.r(x0, y, 552.0, 34.0);
            if sel == Some(j) || Self::inside(p, cursor) {
                Ui::fill(out, p, if sel == Some(j) { [0.356, 0.014, 0.002, 1.0] } else { [0.462, 0.027, 0.004, 1.0] }); // #A11F08 / #B52E0C
            }
            ui.img_fit(out, if self.lst.is_user(&self.files[j]) { "user" } else { "ff" }, self.r(x0 - 2.0 + 5.0, y + 2.0, 30.0, 30.0), [1.0; 4]);
            let i = &self.infos[j];
            self.label(ui, out, &i.players.to_string(), col(1) + 15.0, y + 17.0, 16.0, 0.5, BLACK);
            self.label(ui, out, &i.size.to_string(), col(2) + 20.0, y + 17.0, 16.0, 0.5, BLACK);
            if !i.balanced { ui.img_fit(out, "Unbalanced", self.r(col(3) + 6.0, y + 10.0, 18.0, 18.0), [1.0; 4]); }
            let name = skirmish::map_text_keys(&self.files[j]).map(|k| ui.text_of(&k.0))
                .unwrap_or_else(|| self.files[j].file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned()));
            self.label(ui, out, &name, col(4) + 6.0, y + 17.0, 16.0, 0.0, BLACK);
            if let Some(d) = std::fs::metadata(&self.files[j]).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
                self.label(ui, out, &date(d.as_secs() as i64), col(5) + 36.0, y + 17.0, 11.0, 0.5, BLACK);
            }
            self.hits.push((p, Act::Pick(j)));
        }
        if self.mp.net.is_none() { self.lobby_list_bar(ui, out, cursor); } // include boxes, search (lobby.rs)
    }

    /// The skirmish options panel: fairness gems, starting goods (game type) and the gold each side gets, toggles.
    fn options(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, l: &Lobby, outposts: bool, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        self.frame(out, 637.0, 384.0, 679.0, 270.0, true);
        ui.img(out, "SkirmishSetup 019", self.r(657.0, 429.0, 345.0, 27.0), [1.0; 4]);
        for v in 1..=5 {
            let p = self.r(637.0 + 40.0 + 70.0 * (v - 1) as f32, 429.0, 30.0, 27.0);
            let on = l.fairness == v || Self::inside(p, cursor);
            ui.img_fit(out, &if on { format!("SkirmishSetup {:03}", 19 + v) } else { "SkirmishSetup 043".into() }, p, [1.0; 4]);
            self.hits.push((p, Act::Fair(v)));
        }
        for v in 1..=3 {
            let p = self.r(637.0 + 80.0 + 90.0 * (v - 1) as f32, 467.0, 50.0, 50.0);
            let state = if l.goods == v { 2 } else { Self::inside(p, cursor) as i32 };
            ui.img_fit(out, &format!("SkirmishSetup {:03}", 29 + 3 * (v - 1) + state), p, [1.0; 4]);
            self.hits.push((p, Act::Goods(v)));
        }
        self.label(ui, out, &ui.text_of(&format!("TEXT_GAME_TYPE_{:03}", l.goods)), 829.5, 538.0, 30.0, 0.5, BLACK);
        let adv = match l.fairness { ..=2 => "TEXT_GAME_TYPE_004", 4.. => "TEXT_GAME_TYPE_005", _ => "" };
        if !adv.is_empty() { self.label(ui, out, &ui.text_of(adv), 829.5, 563.0, 18.0, 0.5, BLACK); }
        // updateSkirmishStartingGoldLevels: the Extreme trail's ×3, none for you with No Starting Gold
        let (you, ai) = START_GOLD[(l.goods - 1).clamp(0, 2) as usize][(l.fairness - 1).clamp(0, 4) as usize];
        let k = if l.extreme_trail { 3 } else { 1 };
        let (you, ai) = (if l.advanced && l.adv[12] > 0 || l.competitive { 0 } else { you * k }, ai * k); // competitive: wood and bread, no gold
        self.label(ui, out, &you.to_string(), 797.0, 590.0, 22.0, 1.0, BLACK);
        ui.img_fit(out, "SkirmishSetup 040", self.r(807.0, 572.0, 44.0, 33.0), [1.0; 4]);
        self.label(ui, out, &ai.to_string(), 862.0, 590.0, 22.0, 0.0, BLACK);
        // toggles (BTN_SH_GlowSImage, tick S100 on / S043 off) and the Advanced Skirmish Options button (lobby.rs)
        self.lobby_toggles(ui, out, l, outposts, cursor);
    }

    /// Show_SkirmishTeamsPanel: dimmed screen, the round table (768×411 panel), one seat per member in team order
    /// (PopulateTeamsPanel): seat highlights in the team's colour (red, yellow, blue, green) for real teams, faces
    /// with team shield and name; the centre button sends the picked member back to no team.
    fn teams_popup(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, l: &Lobby, cursor: [f32; 2]) {
        Ui::fill(out, self.r(0.0, 0.0, 1366.0, 768.0), [0.0, 0.0, 0.0, 0.533]);
        self.hits.push((self.r(0.0, 0.0, 1366.0, 768.0), Act::TeamsOpen)); // the dim swallows clicks
        let (px, py) = (299.0, 178.5);
        self.frame(out, px - 3.0, py - 3.0, 774.0, 417.0, true);
        pic(out, "table", self.r(px - 4.0, py - 2.0, 776.0, 415.0), 1.0);
        let t = teams(l);
        let order = seats(&t);
        for (k, &m) in order.iter().enumerate() {
            if t[m] > 0 {
                let (x, y, w, h) = SLICES[k];
                pic(out, &format!("tslice{}{}", k + 1, ["", "b", "c", "d"][(t[m] as usize - 1).min(3)]), self.r(px + x, py + y, w, h), 0.5);
            }
        }
        let can = self.team_pick.is_some_and(|a| t[a] > 0);
        self.img_button(ui, out, ["SkirmishSetup 041", "SkirmishSetup 042", "SkirmishSetup 041"], px + 356.0, py + 134.0, 60.0, 64.0, can.then_some(Act::TeamCancel), cursor);
        for (k, &m) in order.iter().enumerate() {
            let (fx, fy) = FACE_SEATS[k];
            let p = self.r(px + fx, py + fy, 72.0, 72.0);
            self.member_face(ui, out, l, m, p, if self.team_pick == Some(m) { 0.4 } else { 1.0 });
            if t[m] > 0 { ui.img(out, &format!("Team Allies Shield {} Large", t[m]), self.r(px + fx - 12.0, py + fy - 14.0, 24.0, 29.0), [1.0; 4]); }
            let name = if m == 0 { coa::player_name() } else { ui.text_of(&computer_name_key(l.ais[m - 1].0)) };
            self.text(ui, out, &name, px + fx + 36.0, py + fy + 86.0, 14.0, 0.5, NAME_COLOURS[m.min(7)]);
            self.hits.push((p, Act::TeamFace(m)));
        }
        self.glow(ui, out, &ui.text_of("TEXT_NEW_TEXT_091"), px + 548.0, py + 363.0, 200.0, None, Some(Act::TeamsClose), cursor);
        // the picked-up face follows the mouse
        if let Some(m) = self.team_pick {
            let h = 36.0 * self.s;
            self.member_face(ui, out, l, m, [cursor[0] - h, cursor[1] - h, cursor[0] + h, cursor[1] + h], 1.0);
        }
    }

    /// A member's face: colour background + lord face (AIs) or your coat of arms + Face Border (GetLocalAvatar; the
    /// crusader shield without the game's Avatars pictures).
    fn member_face(&self, ui: &Ui, out: &mut Vec<HudTri>, l: &Lobby, m: usize, p: [f32; 4], a: f32) {
        let c = [1.0, 1.0, 1.0, a];
        ui.img(out, FACE_BACKGROUNDS[m.min(7)], p, c);
        if m == 0 {
            let k = (p[2] - p[0]) * 0.15;
            if !coa::draw_mine(out, p, c) { ui.img_fit(out, "shield_crusader", [p[0] + k, p[1] + k, p[2] - k, p[3] - k], c); }
            ui.img(out, "Face Border", p, c);
        } else if let Some(cl) = crate::custom_lords::of_lord(l.ais[m - 1].0) {
            if !crate::custom_lords::draw_face(out, &cl, p, c) { if let Some(&(face, _)) = FACES.iter().find(|f| f.1 == crate::custom_lords::parent(&cl)) { ui.img(out, face, p, c); } }
            ui.img(out, "Face Border", p, c);
        } else if let Some(&(face, _)) = FACES.iter().find(|f| f.1 == l.ais[m - 1].0) {
            ui.img(out, face, p, c);
        }
    }

    /// Show_AddAIPanel: the lord faces (72 px every 80 px), then the custom lords' faces (CustomLords folders, a (!)
    /// for the backstory page; imported on the Extras screen), Random, and the rollover panel for the face under the
    /// cursor. More than 4 rows shrink the grid to fit the panel.
    fn add_ai_popup(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        self.frame(out, 50.0, 380.0, 700.0, 360.0, true);
        self.frame(out, 780.0, 380.0, 534.0, 360.0, true);
        // the panels swallow clicks (what they cover stays inert)
        for p in [self.r(32.0, 362.0, 736.0, 396.0), self.r(762.0, 362.0, 570.0, 396.0)] { self.hits.push((p, Act::AddAiOpen)); }
        let customs = crate::custom_lords::count();
        // the built-in lords, the custom lords, Random
        let tiles: Vec<usize> = FACES.iter().map(|f| f.1).filter(|&n| n != 99).chain((0..customs).map(|k| crate::custom_lords::BASE + k)).chain([99]).collect();
        let k = (4.0 / tiles.len().div_ceil(8) as f32).min(1.0);
        let mut hover = None;
        for (c, &n) in tiles.iter().enumerate() {
            let p = self.r(70.0 + 80.0 * k * (c % 8) as f32, 400.0 + 80.0 * k * (c / 8) as f32, 72.0 * k, 72.0 * k);
            let custom = crate::custom_lords::of_lord(n);
            if let Some(cl) = &custom {
                ui.img(out, "Face Background Generic", p, [1.0; 4]);
                if !crate::custom_lords::draw_face(out, cl, p, [1.0; 4]) { ui.img(out, FACES.iter().find(|f| f.1 == crate::custom_lords::parent(cl)).map_or("", |f| f.0), p, [1.0; 4]); }
                ui.img(out, "Face Border", p, [1.0; 4]);
                self.hits.push((p, Act::AddLord(n)));
            } else {
                let face = FACES.iter().find(|f| f.1 == n).map_or("", |f| f.0);
                let avail = n == 99 || n < LORDS.len();
                if face.starts_with("Face") {
                    ui.img(out, "Face Background Generic", p, [1.0; 4]);
                    ui.img(out, face, p, [1.0; 4]);
                    if !avail { ui.img(out, &format!("{face}_Locked"), p, [1.0; 4]); }
                } else {
                    ui.img_fit(out, face, p, [1.0; 4]);
                    ui.img(out, "Face Border", p, [1.0; 4]);
                }
                if avail { self.hits.push((p, Act::AddLord(n))); }
            }
            if Self::inside(p, cursor) { hover = Some(n); }
            // the (!) corner badge: the custom lord's backstory page (lord_info.rs); pushed last, so it wins the click
            if custom.is_some() { self.info_badge(ui, out, 70.0 + 80.0 * k * (c % 8) as f32 + 50.0 * k, 400.0 + 80.0 * k * (c / 8) as f32 + 2.0 * k, 20.0 * k, Act::LordInfo(n), cursor); }
        }
        self.img_button(ui, out, ["UI-Buttons S043", "UI-Buttons S044", "UI-Buttons S043"], 711.0, 390.0, 29.0, 29.0, Some(Act::AddAiClose), cursor);
        match hover {
            Some(n) if crate::custom_lords::lord_of(n).is_some() => {
                let Some(c) = crate::custom_lords::of_lord(n) else { return };
                self.label(ui, out, &c.name, 1047.0, 402.0, 22.0, 0.5, BLACK);
                let p = self.r(790.0, 420.0, 100.0, 100.0);
                ui.img(out, "Face Background Generic", p, [1.0; 4]);
                if !crate::custom_lords::draw_face(out, &c, p, [1.0; 4]) { ui.img(out, FACES.iter().find(|f| f.1 == crate::custom_lords::parent(&c)).map_or("", |f| f.0), p, [1.0; 4]); }
                ui.img(out, "Face Border", p, [1.0; 4]);
                self.label(ui, out, "(!) his story", 905.0, 470.0, 16.0, 0.0, BLACK);
                let mut y = 530.0;
                for k in 0..5 {
                    let Some(v) = crate::custom_lords::help(&c, k) else { continue };
                    y = self.para(ui, out, &format!("{}: {v}", ui.text_of(&format!("TEXT_AI_LORD_HELP_{:03}", k + 1))), 790.0, y + 3.0, 500.0, 16.0, BLACK);
                }
            }
            Some(n) if n < 29 => {
                self.label(ui, out, &ui.text_of(&lord_keys(n).0), 1047.0, 402.0, 22.0, 0.5, BLACK);
                let p = self.r(790.0, 420.0, 100.0, 100.0);
                ui.img(out, "Face Background Generic", p, [1.0; 4]);
                ui.img(out, FACES.iter().find(|f| f.1 == n).map_or("", |f| f.0), p, [1.0; 4]);
                let mut y = 530.0;
                for k in 0..5 {
                    let line = format!("{}: {}", ui.text_of(&format!("TEXT_AI_LORD_HELP_{:03}", k + 1)), ui.text_of(&format!("TEXT_AI_LORD_HELP_{:03}", (n + 1) * 5 + 1 + k)));
                    y = self.para(ui, out, &line, 790.0, y + 3.0, 500.0, 16.0, BLACK);
                }
            }
            _ => { self.label(ui, out, &ui.text_of("TEXT_XPLAY_WAITING_ROOM_402"), 1047.0, 402.0, 22.0, 0.5, BLACK); }
        }
    }

    /// A round "(!)" badge, `d` design px across at x, y (top left): more about a custom lord.
    fn info_badge(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, x: f32, y: f32, d: f32, a: Act, cursor: [f32; 2]) {
        let p = self.r(x, y, d, d);
        let hot = Self::inside(p, cursor);
        let ring: Vec<[f32; 2]> = (0..20).map(|i| { let t = i as f32 / 20.0 * std::f32::consts::TAU; [(p[0] + p[2]) / 2.0 + t.cos() * (p[2] - p[0]) / 2.0, (p[1] + p[3]) / 2.0 + t.sin() * (p[3] - p[1]) / 2.0] }).collect();
        Ui::fill_convex(out, &ring, if hot { [0.55, 0.06, 0.03, 1.0] } else { [0.32, 0.03, 0.02, 0.95] });
        self.label(ui, out, "!", x + d / 2.0, y + d / 2.0, d * 0.8, 0.5, [0.905, 0.73, 0.402, 1.0]);
        if hot { crate::audio::hover(p, crate::audio::ui_id::RED_BUTTON, 1.0); }
        self.hits.push((p, a));
    }
}

/// Screen A's campaigns in order: shield sprite, hover help title and text keys (campaign 7's extra "new" line skipped).
const CAMPAIGNS: [(&str, &str, &str); 7] = [
    ("shield_gauntlet", "TEXT_BUBBLE_HELP_SUBTEXT_002", "TEXT_SKIRMISH_CHOOSE2_010"),
    ("shield_helmet", "TEXT_BUBBLE_HELP_SUBTEXT_003", "TEXT_SKIRMISH_CHOOSE2_011"),
    ("shield_crown", "TEXT_BUBBLE_HELP_SUBTEXT_004", "TEXT_SKIRMISH_CHOOSE2_012"),
    ("shield_crusader", "TEXT_BUBBLE_HELP_SUBTEXT_005", "TEXT_SKIRMISH_CHOOSE2_013"),
    ("shield_scroll", "TEXT_NEW_CTEXT_336", "TEXT_NEW_CTEXT_337"),
    ("shield_pyramid", "TEXT_NEW_CTEXT_338", "TEXT_NEW_CTEXT_339"),
    ("shield_crocodile", "TEXT_NEW_CTEXT_340", "TEXT_NEW_CTEXT_341"),
];

/// Player colours 1..8 (red, orange, yellow, blue, grey, purple, light blue, green): row shields and radar keeps.
const COLOUR_SHIELDS: [&str; 8] = ["UI-Buttons H016", "UI-Buttons H010", "UI-Buttons H013", "UI-Buttons H007", "UI-Buttons H019", "UI-Buttons H022", "UI-Buttons H025", "UI-Buttons H028"];
const KEEP_SHIELDS: [&str; 8] = ["UI-Buttons H034", "UI-Buttons H032", "UI-Buttons H033", "UI-Buttons H031", "UI-Buttons H035", "UI-Buttons H036", "UI-Buttons H037", "UI-Buttons H038"];

/// The Add-AI grid in XAML order: face sprite, lord index (`LORDS`; 27/28 Surgeon / Baibars have no castle layouts
/// here; 99 random). The custom lords go before Random in the grid (add_ai_popup).
const FACES: [(&str, usize); 30] = [("Face 01", 0), ("Face 02", 1), ("Face 03", 2), ("Face 04", 3), ("Face 05", 4), ("Face 06", 5), ("Face 07", 6), ("Face 08", 7),
    ("Face 09", 8), ("Face 10", 9), ("Face 11", 10), ("Face 12", 11), ("Face 13", 12), ("Face 14", 13), ("Face 15", 14), ("Face 16", 15),
    ("Face 17", 16), ("Face 18", 17), ("Face 19", 18), ("Face 20", 19), ("Face 25", 24), ("Face 21", 20), ("Face 22", 21), ("Face 23", 22),
    ("Face 24", 23), ("Face 26", 25), ("Face 27", 26), ("Face 28", 27), ("Face 29", 28), ("shield_options", 99)];

/// GameData.starting_gold_table: (you, computer) by game type (Normal, Crusader, Deathmatch) and fairness 1..5.
const START_GOLD: [[(i32, i32); 5]; 3] = [
    [(8000, 2000), (4000, 2000), (2000, 2000), (2000, 4000), (2000, 8000)],
    [(8000, 2000), (4000, 2000), (2000, 2000), (2000, 4000), (2000, 8000)],
    [(40000, 3000), (20000, 7000), (10000, 10000), (7000, 20000), (3000, 40000)],
];

/// The lobby's teams, one per member (you first): 0 = no team, real teams 1..k.
fn teams(l: &Lobby) -> Vec<i32> { std::iter::once(l.team).chain(l.ais.iter().map(|a| a.1)).collect() }
fn set_teams(l: &mut Lobby, t: &[i32]) {
    l.team = t[0];
    for (a, &v) in l.ais.iter_mut().zip(&t[1..]) { a.1 = v; }
}

/// ReSortTeamInfo: a team of one is no team, everyone in one team is nobody allied, ids renumbered 1..k in order.
/// (The game then sends team 0 for anyone alone: Platform_Multiplayer, RegisterSkirmishUser.)
fn normalize(t: &mut [i32]) {
    for id in 1..=8 {
        if t.iter().filter(|&&v| v == id).count() == 1 { t.iter_mut().filter(|v| **v == id).for_each(|v| *v = 0); }
    }
    if t.len() > 1 && t[0] > 0 && t.iter().all(|&v| v == t[0]) { t.fill(0); }
    let mut ids: Vec<i32> = t.iter().copied().filter(|&v| v > 0).collect();
    ids.sort();
    ids.dedup();
    for v in t.iter_mut().filter(|v| **v > 0) { *v = ids.iter().position(|&i| i == *v).unwrap() as i32 + 1; }
}

/// Seat order round the table (team_order): team 1, team 2, ..., then the unallied, each by player number.
fn seats(t: &[i32]) -> Vec<usize> {
    let mut s: Vec<usize> = (0..t.len()).collect();
    s.sort_by_key(|&i| (if t[i] == 0 { 99 } else { t[i] }, i));
    s
}

/// Seat k's highlight (tslice{k+1}) and face, in the 768×411 panel.
const SLICES: [(f32, f32, f32, f32); 8] = [(0.0, 0.0, 382.0, 179.0), (204.0, 0.0, 360.0, 180.0), (386.0, 0.0, 382.0, 179.0), (385.0, 91.0, 383.0, 178.0),
    (386.0, 181.0, 382.0, 179.0), (204.0, 180.0, 360.0, 180.0), (0.0, 181.0, 382.0, 179.0), (0.0, 91.0, 383.0, 178.0)];
const FACE_SEATS: [(f32, f32); 8] = [(124.0, 20.0), (347.0, 10.0), (572.0, 20.0), (606.0, 132.0), (572.0, 243.0), (357.0, 253.0), (124.0, 243.0), (89.0, 132.0)];
/// Face backgrounds by player colour (the sprites are in our colour order: red, orange, yellow, blue, grey, ...).
const FACE_BACKGROUNDS: [&str; 8] = ["Face Background 1", "Face Background 2", "Face Background 3", "Face Background 4", "Face Background 5", "Face Background 6", "Face Background 7", "Face Background 8"];
/// Team-name text by player colour (approximate MPTeamColours, linear).
const NAME_COLOURS: [[f32; 4]; 8] = [[0.716, 0.021, 0.012, 1.0], [0.871, 0.262, 0.007, 1.0], [0.871, 0.716, 0.021, 1.0], [0.021, 0.102, 0.791, 1.0],
    [0.352, 0.352, 0.352, 1.0], [0.402, 0.045, 0.578, 1.0], [0.045, 0.515, 0.791, 1.0], [0.045, 0.515, 0.045, 1.0]];

/// OnScreenText.getComputerName(lord + 1, 0): "Rat, Duc de Puce" etc.
fn computer_name_key(n: usize) -> String {
    match n { 0..=15 => format!("TEXT_XPLAY_WAITING_ROOM_{:03}", 112 + 8 * n), 16..=24 => format!("TEXT_NEW_CTEXT_{:03}", 17 + 8 * (n - 16)), _ => format!("TEXT_NEW_CTEXT_{:03}", 455 + 17 * (n - 25)) }
}

/// Lord n's short name and title string keys (Translate: lookUpText is 0-based, so keys are + 1).
fn lord_keys(n: usize) -> (String, String) {
    if n >= crate::custom_lords::BASE { return (crate::lord_share::name_of(n), String::new()); }
    let (s, k) = match n { 0..=15 => ("TEXT_XPLAY_WAITING_ROOM", 240 + 9 * n), 16..=24 => ("TEXT_NEW_CTEXT", 89 + 9 * (n - 16)), _ => ("TEXT_NEW_CTEXT", 454 + 17 * (n - 25)) };
    (format!("{s}_{k:03}"), format!("{s}_{:03}", k + 1))
}

/// When the running game started (main.rs start_live): Show Play Time counts from it.
static GAME_T0: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
pub fn game_started() { *GAME_T0.lock().unwrap() = Some(Instant::now()); }

/// Options > Show Local Time / Show Play Time (IngameUIScreens.xaml: top right under the compass, 24 px #EFF3C6 on
/// #33000000, drop shadow): the clock (DateTime.Now short time) and GameData.GetTimeString(elapsedTime / 1000).
/// main.rs draws it over a running game. ponytail: 24 h clock, play time = wall clock since the game started (the
/// engine's elapsedTime is timeGetTime too).
pub fn clock_overlay(ui: &mut Ui, w: f32, h: f32) -> Vec<HudTri> {
    let (local, play) = (opt("LocalTime", false), opt("GameTime", false));
    if !local && !play { return vec![]; }
    let s = crate::ux::hud_scale(w, h);
    let keep = (ui.s, ui.w, ui.h); // the HUD drops its glyph cache when ui.s changes: put it back afterwards
    (ui.s, ui.w, ui.h) = (s, w, h);
    let mut out = vec![];
    let mut y = 56.0 * s; // under the tick label
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) + crate::saves::tz();
    let lines = [local.then(|| format!("{:02}:{:02}", secs.rem_euclid(86400) / 3600, secs.rem_euclid(3600) / 60)),
        play.then(|| GAME_T0.lock().unwrap().map(|t| t.elapsed().as_secs())).flatten().map(|t| if t < 3600 { format!("{}:{:02}", t / 60, t % 60) } else { format!("{}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60) })];
    for l in lines.into_iter().flatten() {
        let tw = ui.text(&mut vec![], &l, 0.0, 0.0, 24.0, 1.0, 0.0, 0.0, CREAM);
        let x = w - 10.0 * s;
        Ui::fill(&mut out, [x - tw - 4.0 * s, y - 24.0 * s, x + 2.0 * s, y + 6.0 * s], [0.0, 0.0, 0.0, 0.2]);
        ui.text_sh(&mut out, &l, x, y, 24.0, 1.0, CREAM);
        y += 32.0 * s;
    }
    (ui.s, ui.w, ui.h) = keep;
    encode(&mut out);
    out
}

/// A release body (GitHub Markdown) as (0 heading / 1 bullet / 2 text, plain text) lines: `#`, `-` / `*` markers,
/// `**`, backticks and link targets dropped. ponytail: no nested lists, tables or images.
fn notes_lines(md: &str) -> Vec<(u8, String)> {
    md.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| {
        let (kind, t) = if l.starts_with('#') { (0, l.trim_start_matches('#')) }
            else if let Some(t) = l.strip_prefix("- ").or(l.strip_prefix("* ")) { (1, t) } else { (2, l) };
        let mut s = t.trim().replace(['*', '`'], "");
        while let (Some(a), Some(b)) = (s.find("]("), s[s.find("](").unwrap_or(0)..].find(')')) { // [text](url) -> text
            let b = a + b;
            s.replace_range(a..=b, "");
            if let Some(o) = s[..a].rfind('[') { s.remove(o); }
        }
        (kind, s)
    }).collect()
}

/// Game Options › Quit Mission / Exit Crusader ask first (HUD_IngameMenu cases 6 / 7): No and Escape go back to the
/// menu, Yes leaves.
#[test]
fn leave_asks_first() {
    let mut m = Menu::pause(false);
    let mut press = |m: &mut Menu, a: Act| { m.hits = vec![([0.0, 0.0, 1.0, 1.0], a)]; m.click([0.5, 0.5]) };
    assert!(press(&mut m, Act::Quit).is_none() && m.confirm == Some(Confirm::Leave(true)), "Exit asks");
    assert!(press(&mut m, Act::No).is_none() && m.confirm.is_none() && m.is_pause(), "No");
    assert!(press(&mut m, Act::ToMenu).is_none() && m.confirm == Some(Confirm::Leave(false)), "Quit Mission asks");
    assert!(m.back().is_none() && m.confirm.is_none() && m.is_pause(), "Esc = No");
    press(&mut m, Act::ToMenu);
    assert!(matches!(press(&mut m, Act::Yes), Some(Go::ToMenu)) && m.confirm.is_none());
    press(&mut m, Act::Quit);
    assert!(matches!(press(&mut m, Act::Yes), Some(Go::Quit)));
}

#[test]
fn release_notes_markdown() {
    let l = notes_lines("## What's new\n\n- **Save** dialog with `names`\n* see [the page](https://x.y/z) now\nPlain line\n");
    assert_eq!(l, vec![(0, "What's new".into()), (1, "Save dialog with names".into()), (1, "see the page now".into()), (2, "Plain line".into())]);
}

/// dd/mm/yyyy of a Unix time.
fn date(secs: i64) -> String {
    let (d, m, y) = civil(secs);
    format!("{d:02}/{m:02}/{y}")
}
/// Day, month, year of a Unix time (civil-from-days, H. Hinnant).
pub(crate) fn civil(secs: i64) -> (i64, i64, i64) {
    let z = secs.div_euclid(86400) + 719468;
    let (era, doe) = (z.div_euclid(146097), z.rem_euclid(146097));
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    (d, m, yoe + era * 400 + (m <= 2) as i64)
}


#[test]
fn team_rules() {
    let mut t = vec![1, 1, 0, 2];
    normalize(&mut t); // team 2 has one member
    assert_eq!(t, [1, 1, 0, 0]);
    let mut t = vec![3, 3, 3];
    normalize(&mut t); // everyone in one team
    assert_eq!(t, [0, 0, 0]);
    let mut t = vec![0, 4, 4, 2, 2];
    normalize(&mut t);
    assert_eq!(t, [0, 2, 2, 1, 1]);
    assert_eq!(seats(&t), [3, 4, 1, 2, 0]);
}

/// FRONT_StandaloneMission's option buttons, field `f`: 0..3 starting goods (−1..5), 4 random events, 5 invasions
/// (0..8), 6 invasion difficulty (0..8), 7 peace time (0..6); wrapping, `back` = right click.
fn cycle(fb: &mut skirmish::FreeBuild, f: u8, back: bool) {
    let (v, lo, hi) = match f {
        0..=3 => (&mut fb.goods[f as usize], -1, 5),
        4 => (&mut fb.events, 0, 8),
        5 => (&mut fb.invasions, 0, 8),
        6 => (&mut fb.invasion_difficulty, 0, 8),
        _ => (&mut fb.peacetime, 0, 6),
    };
    *v = if back { if *v <= lo { hi } else { *v - 1 } } else if *v >= hi { lo } else { *v + 1 };
}

/// A faction toggle (bit 1 Crusader, 2 Arab, 4 Bedouin): the last one on can't go off, the next one comes on instead.
fn faction(opponents: i32, bit: i32) -> i32 {
    if opponents & bit == 0 { return opponents | bit; }
    let rest = opponents & !bit;
    if rest != 0 { rest } else { [0, 2, 4, 0, 1][bit as usize] }
}

/// The Options > Hotkeys rows (re/data/hotkeys.tsv, HUD_Options.hotKeyList order): KeyFunctions name, label key,
/// a "Place :" row.
fn hotkeys() -> &'static Vec<(String, String, bool)> {
    static H: OnceLock<Vec<(String, String, bool)>> = OnceLock::new();
    H.get_or_init(|| include_str!("../../re/data/hotkeys.tsv").lines().filter(|l| !l.starts_with('#')).filter_map(|l| {
        let mut f = l.split('\t');
        Some((f.next()?.to_string(), f.next()?.to_string(), f.next() == Some("place")))
    }).collect())
}

/// A Hotkeys row's name: TEXT_HOT_KEYS_..., or "Place :" + the building.
fn hotkey_label(ui: &Ui, row: usize) -> String {
    let (_, label, place) = &hotkeys()[row];
    if let Some(text) = label.strip_prefix('=') { return text.to_string(); } // a port-only row (no game text)
    if *place { format!("{} {}", ui.text_of("TEXT_NEW_CTEXT_372"), ui.text_of(label)) } else { ui.text_of(label) }
}

/// A function's keys as the list shows them: "K1", "K1 / K2" or "Not Assigned".
fn bound_text(ui: &Ui, keys: [&str; 2]) -> String {
    let k: Vec<String> = keys.into_iter().filter(|s| !s.is_empty()).map(|s| key_name(ui, s)).collect();
    if k.is_empty() { ui.text_of("TEXT_NEW_TEXT_094") } else { k.join(" / ") }
}

/// HUD_Options.GetKeyCodeString for a Unity KeyCode name with ",shift" / ",ctrl" / ",alt": "Shift Ctrl Alt " prefixes,
/// digits / signs as themselves, Numpad / Space / the named keys from TEXT_HOT_KEYS, else the KeyCode name.
fn key_name(ui: &Ui, k: &str) -> String {
    let mut parts = k.split(',');
    let code = parts.next().unwrap_or("");
    let mods: Vec<&str> = parts.collect();
    let mut s: String = [("shift", "TEXT_HOT_KEYS_079"), ("ctrl", "TEXT_HOT_KEYS_080"), ("alt", "TEXT_HOT_KEYS_081")].iter()
        .filter(|(m, _)| mods.contains(m)).map(|(_, t)| ui.text_of(t) + " ").collect();
    let numpad = || ui.text_of("TEXT_HOT_KEYS_082");
    let named = [("BackQuote", "`"), ("Backslash", "\\"), ("Minus", "-"), ("Equals", "="), ("LeftBracket", "["), ("RightBracket", "]"), ("Semicolon", ";"),
        ("Quote", "'"), ("Comma", ","), ("Period", "."), ("Slash", "/")];
    let text = [("Tab", 84), ("CapsLock", 85), ("Return", 86), ("Backspace", 87), ("UpArrow", 88), ("DownArrow", 89), ("RightArrow", 90), ("LeftArrow", 91),
        ("Insert", 92), ("Home", 93), ("End", 94), ("PageUp", 95), ("PageDown", 96), ("Delete", 97), ("Numlock", 98), ("Space", 110)];
    if let Some(d) = code.strip_prefix("Alpha") { s += d; }
    else if let Some(&(_, c)) = named.iter().find(|(n, _)| *n == code) { s += c; }
    else if let Some(&(_, t)) = text.iter().find(|(n, _)| *n == code) { s += &ui.text_of(&format!("TEXT_HOT_KEYS_{t:03}")); }
    else if let Some(d) = code.strip_prefix("Keypad") {
        let d = match d { "Period" => ".", "Plus" => "+", "Minus" => "-", "Divide" => "/", "Multiply" => "*", "Equals" => "=", "Enter" => "Enter", d => d };
        s += &format!("{} {}", numpad(), if d == "Enter" { ui.text_of("TEXT_HOT_KEYS_083") } else { d.to_string() });
    } else { s += code; }
    s
}

/// The menu's colours are linear: encode a finished triangle list for sprite.wgsl (which takes sRGB).
fn encode(out: &mut [HudTri]) { for (_, vs) in out.iter_mut() { for v in vs.iter_mut() { for c in &mut v.color[..3] { *c = srgb(*c) } } } }

/// A list row's background (Template.ListViewItem): Brush.Item.Over #B52E0C, Selected #A11F08, SelectedOver #C2240C.
fn row(out: &mut Vec<HudTri>, p: [f32; 4], hot: bool, sel: bool) {
    let c = match (hot, sel) { (true, true) => [0.539, 0.018, 0.004, 1.0], (false, true) => [0.356, 0.014, 0.002, 1.0], (true, false) => [0.462, 0.027, 0.004, 1.0], _ => return };
    Ui::fill(out, p, c);
}

/// Ui's widgets (`Ui::text_button`, `Ui::text_field`) colour in sRGB like the HUD: drawn into a menu list, back to
/// linear for `encode`.
pub(crate) fn widget(out: &mut Vec<HudTri>, draw: impl FnOnce(&mut Vec<HudTri>)) {
    let n = out.len();
    draw(out);
    let lin = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    for (_, vs) in &mut out[n..] { for v in vs.iter_mut() { for c in &mut v.color[..3] { *c = lin(*c) } } }
}

/// Linear -> sRGB (the inverse of sprite.wgsl's decode).
fn srgb(c: f32) -> f32 { if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 } }

/// A map's list name: a built-in map's translated name, else the file name.
fn map_name(ui: &Ui, map: &std::path::Path) -> String {
    skirmish::map_text_keys(map).map(|k| ui.text_of(&k.0)).unwrap_or_else(|| map.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned()))
}

impl Story {
    /// Letters typed so far (one per 50 ms).
    fn typed(&self) -> usize { (self.t0.elapsed().as_millis() / 50) as usize }
    /// HistoricalCampaign-Drawing / -Map number: 7 per campaign (intro, its 5 missions, aftermath).
    fn art(&self) -> i32 {
        let (c, m) = ((self.id - 1) / 5, (self.id - 1) % 5);
        if self.aftermath { 7 * c + 7 } else if self.campaign { 7 * c + 1 } else { 7 * c + 2 + m }
    }
    /// The page's narration (FRONT_Story.ConfigureScreen).
    fn speech(&self) -> String {
        let c = (self.id - 1) / 5 + 1;
        if !self.campaign { format!("Campaign\\M{}_Hist.wav", self.id) } else if self.aftermath { format!("Campaign\\After_0{c}.wav") } else { format!("Campaign\\Intro_0{c}.wav") }
    }
}

/// StreamingAssets/Help/mission<id>.html as (kind, text) blocks: 0 the <h4> title, 1 a <font size=7> heading, 2 a
/// paragraph. Tags dropped, a few entities decoded. Empty if the file is missing.
fn help_blocks(id: i32) -> Vec<(u8, String)> {
    let Some(html) = skirmish::maps_dir().parent().and_then(|d| std::fs::read_to_string(d.join(format!("Help/mission{id}.html"))).ok()) else { return vec![] };
    let (mut out, mut kind, mut cur) = (vec![], 2u8, String::new());
    let mut flush = |kind: u8, cur: &mut String| {
        let t = cur.replace("&nbsp;", " ").replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'");
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if !t.is_empty() { out.push((kind, t)); }
        cur.clear();
    };
    for (i, part) in html.split('<').enumerate() {
        let (tag, text) = if i == 0 { ("", part) } else { part.split_once('>').unwrap_or((part, "")) };
        let tag = tag.to_ascii_lowercase();
        let open = if tag.starts_with("h4") { Some(0) } else if tag.starts_with("font size=7") { Some(1) } else if tag == "p" || tag.starts_with("p ") { Some(2) } else { None };
        let close = tag.starts_with("/h4") || tag.starts_with("/p") || (tag.starts_with("/font") && kind == 1);
        if open.is_some() || close { flush(kind, &mut cur); }
        if let Some(k) = open { kind = k; } else if close { kind = 2; }
        if tag.starts_with("br") || tag.starts_with("/center") { cur.push(' '); }
        cur.push_str(text);
    }
    flush(kind, &mut cur);
    out
}

/// The RGBA rows (1920 wide) for `screen_image()`'s name: ui/screens/<name>.rgba (1920×1080), or for "story:<k>" the
/// story background with HistoricalCampaign-Drawing-<k> and -Map-<k> packed below it (SCREEN_H rows, see STORY_ART).
pub fn screen_pixels(name: &str) -> Option<Vec<u8>> {
    if let Some(n) = name.strip_prefix("credits:") { return credits::pixels(n.parse().ok()?); }
    if let Some(n) = name.strip_prefix("lord:") { return lord_info::pixels(n); }
    let dir = crate::cache_dir().join("ui/screens");
    let read = |n: &str| std::fs::read(dir.join(format!("{n}.rgba"))).ok();
    let full = |p: &Vec<u8>| p.len() == 1920 * 1080 * 4;
    if let Some(page) = name.strip_prefix("help:") { return help::pixels(page); } // the help page's pictures (help.rs)
    let Some(k) = name.strip_prefix("story:") else { return read(name).filter(full) };
    let mut px = read("HistoricalCampaign-Background").filter(full)?;
    px.resize(1920 * SCREEN_H as usize * 4, 0);
    for (kind, (tx, tw, ..)) in ["drawing", "map"].into_iter().zip(STORY_ART) {
        let Some(src) = read(&format!("story-{kind}-{k}")) else { continue };
        for (row, line) in src.chunks_exact(tw as usize * 4).enumerate().take(SCREEN_H as usize - 1080) {
            let o = ((1080 + row) * 1920 + tx as usize) * 4;
            px[o..o + line.len()].copy_from_slice(line);
        }
    }
    Some(px)
}

/// Main menu → Historical Campaigns → campaign 1 → Next → story pages → mission 1 loads → briefing (Hints, Help) →
/// Start → it ticks.
#[cfg(feature = "live")]
#[test]
fn campaign_menu_path_ticks() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
    if !skirmish::maps_dir().join("crusaders_mission1a.map").exists() { return; } // needs the game install
    crate::skirmish::set_test_profile(std::env::temp_dir().join("shc-menu-path-test")); // fresh progress: mission 1 only
    let press = |m: &mut Menu, u: &mut Ui, a: Act| {
        m.draw(u, 1920.0, 1080.0, [0.0; 2]);
        let p = m.hits.iter().find(|h| h.1 == a).expect("button on screen").0;
        m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0])
    };
    let mut m = Menu::main();
    assert!(press(&mut m, &mut u, Act::Campaigns).is_none());
    assert!(press(&mut m, &mut u, Act::Campaign(0)).is_none());
    assert!(matches!(m.screen, Screen::Missions(0, 1)));
    assert!(press(&mut m, &mut u, Act::PlayMission).is_none());
    // FRONT_Story: campaign 1's intro page, then mission 1's; each click first shows the whole text
    assert!(matches!(m.screen, Screen::Story(Story { id: 1, campaign: true, .. })));
    assert!(press(&mut m, &mut u, Act::StoryAdvance).is_none());
    assert!(press(&mut m, &mut u, Act::StoryAdvance).is_none());
    assert!(matches!(m.screen, Screen::Story(Story { id: 1, campaign: false, .. })));
    assert_eq!(m.screen_image().as_deref(), Some("story:2"));
    assert!(press(&mut m, &mut u, Act::StoryAdvance).is_none());
    let Some(Go::Mission(id)) = press(&mut m, &mut u, Act::StoryAdvance) else { panic!("the story did not start the mission") };
    assert_eq!(id, 1);
    let mut l = crate::live::Live::from_mem(skirmish::start_mission(id, 1).unwrap());
    let mut b = Menu::briefing(id, true);
    // Hints tab: only the first locked hint has a button; opening it shows the next
    press(&mut b, &mut u, Act::BriefTab(1));
    b.draw(&mut u, 1920.0, 1080.0, [0.0; 2]);
    assert!(matches!(b.screen, Screen::Briefing(Brief { tab: 1, hints: 0, .. })) && !b.hits.iter().any(|h| h.1 == Act::Hint(2)));
    press(&mut b, &mut u, Act::Hint(1));
    press(&mut b, &mut u, Act::Hint(2));
    assert!(matches!(b.screen, Screen::Briefing(Brief { hints: 2, .. })));
    press(&mut b, &mut u, Act::BriefTab(2));
    assert!(b.help.iter().any(|(k, t)| *k == 1 && t == "Quarry"), "help page: {:?}", b.help.first());
    assert!(matches!(press(&mut b, &mut u, Act::BriefStart), Some(Go::Brief(1))));
    let t0 = l.tick();
    for _ in 0..50 { l.step(); }
    assert_eq!(l.tick() - t0, 50);
}

/// Options: sliders and checkboxes write settings.cfg (volume = v / 100, game speed = v × 5) and keep the rest.
#[test]
fn options_save_settings() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
    let dir = std::env::temp_dir().join(format!("shc-options-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nName:Lord X\nGameSpeed:40\n||SETTINGS||\n||KEYS||\nPause:P:\n||KEYS||\n").unwrap();
    crate::skirmish::set_test_profile(&dir);
    let press_at = |m: &mut Menu, u: &mut Ui, a: Act, f: f32| {
        m.draw(u, 1920.0, 1080.0, [0.0; 2]);
        let p = m.hits.iter().find(|h| h.1 == a).expect("control on screen").0;
        m.click([p[0] + (p[2] - p[0]) * f, (p[1] + p[3]) / 2.0])
    };
    let mut m = Menu::main();
    press_at(&mut m, &mut u, Act::Options, 0.5);
    press_at(&mut m, &mut u, Act::OptPage(1), 0.5);
    press_at(&mut m, &mut u, Act::Slider(0), (10.0 + 94.0) / 208.0); // the middle of the thumb's travel
    m.wheel(-2.0); // Reduce Music Volume For Speech is below the fold: default on -> off
    press_at(&mut m, &mut u, Act::Check(11), 0.5);
    press_at(&mut m, &mut u, Act::OptPage(2), 0.5);
    m.wheel(-2.0); // the speed row is below the fold
    press_at(&mut m, &mut u, Act::Slider(5), 0.99); // game speed to the end: 18 × 5
    assert!(matches!(press_at(&mut m, &mut u, Act::OptBack, 0.5), Some(Go::Settings(None))));
    let cfg = skirmish::Settings::read(&dir).unwrap();
    assert_eq!((cfg.get("MasterVolume"), cfg.get("ReduceMusicVolumeForSpeech"), cfg.game_speed(), cfg.get("Name")), (Some("0.5"), Some("False"), 90.0, Some("Lord X")));
    assert_eq!(cfg.keys, "||KEYS||\nPause:P:\n||KEYS||\n");
    assert!(matches!(m.screen, Screen::Main));
    assert_eq!((key_name(&u, "Alpha3,shift"), key_name(&u, "Keypad5"), key_name(&u, "LeftArrow")), ("Shift 3".into(), "Numpad 5".into(), "LeftArrow".into()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Options > Hotkeys: Pause row -> Hotkey 1 -> press L -> Apply. L leaves Lord, settings.cfg has it, a fresh load
/// (the next start) runs Pause on L.
#[test]
fn hotkey_rebind_pause() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
    let dir = std::env::temp_dir().join(format!("shc-hotkeys-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nName:Lord X\n||SETTINGS||\n").unwrap();
    crate::skirmish::set_test_profile(&dir);
    let press = |m: &mut Menu, u: &mut Ui, a: Act| {
        m.draw(u, 1920.0, 1080.0, [0.0; 2]);
        let p = m.hits.iter().rev().find(|h| h.1 == a).expect("control on screen").0;
        m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0])
    };
    let mut m = Menu::main();
    press(&mut m, &mut u, Act::Options);
    press(&mut m, &mut u, Act::OptPage(3));
    let pause = hotkeys().iter().position(|h| h.0 == "Pause").unwrap() as u16;
    press(&mut m, &mut u, Act::HkRow(pause));
    press(&mut m, &mut u, Act::HkSlot(0));
    assert!(m.capturing());
    m.key("L");
    press(&mut m, &mut u, Act::HkApply);
    assert!(!m.capturing());
    let k = crate::keys::Keys::load();
    assert_eq!((k.action("L"), k.action("P"), k.get("Lord")), (Some("Pause"), None, ["", ""]));
    assert_eq!(skirmish::Settings::read(&dir).unwrap().get("Name"), Some("Lord X"));
    // Classic preset: rotation on X / C
    press(&mut m, &mut u, Act::HkClose);
    press(&mut m, &mut u, Act::HkPreset(false));
    assert_eq!(crate::keys::Keys::load().action("C"), Some("MapRotateRight"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Lobby: on a map with 2 start positions the Add AI face grid stops at one AI (AddSkirmishPlayer's cap).
#[test]
fn lobby_ai_cap() {
    let act = |m: &mut Menu, a: Act| { m.hits = vec![([0.0, 0.0, 1.0, 1.0], a)]; m.click([0.5, 0.5]) };
    let mut m = Menu::main();
    act(&mut m, Act::Skirmish);
    let Some(two) = m.infos.iter().position(|i| i.players == 2) else { return }; // needs the game's maps
    act(&mut m, Act::Pick(two));
    act(&mut m, Act::AddAiOpen);
    for n in [0, 1, 2] { act(&mut m, Act::AddLord(n)); }
    let Screen::Lobby(l) = &m.screen else { panic!("not in the lobby") };
    assert_eq!(l.ais.len(), 1, "{}", l.map.display());
}

/// Mission list: a mission won on Hard reads "Completed : Hard" from 1 s after the cursor came onto it.
#[test]
fn completed_mission_label() {
    let Some(u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
    let mut m = Menu::main();
    m.progress.done.insert(3, 2);
    let name = m.mission_label(&u, 3, true);
    assert_eq!(name, u.text_of("TEXT_MISSION3_STORY_002"));
    m.hover = Some((3, Instant::now() - std::time::Duration::from_millis(1100)));
    assert_eq!(m.mission_label(&u, 3, true), format!("{} : {}", u.text_of("TEXT_NEW_TEXT2_190"), u.text_of("TEXT_MAINOPTIONS_022")));
    assert_eq!(m.mission_label(&u, 4, true), u.text_of("TEXT_MISSION4_STORY_002")); // not completed
    assert_eq!(m.mission_label(&u, 3, false), name);
}

/// A Custom Scenario's briefing: no Hints / Help tabs, the difficulty shown but not changeable, Start runs it at the
/// difficulty it was loaded with, Quit goes back to the front end.
#[test]
fn scenario_briefing_controls() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
    let mut m = Menu::scenario_briefing(PathBuf::from("Central Defense.map"), 2, false);
    m.draw(&mut u, 1920.0, 1080.0, [0.0; 2]);
    let has = |m: &Menu, f: fn(&Act) -> bool| m.hits.iter().any(|h| f(&h.1));
    assert!(!has(&m, |a| matches!(a, Act::BriefTab(_))) && !has(&m, |a| *a == Act::Difficulty));
    let press = |m: &mut Menu, a: Act| { let p = m.hits.iter().find(|h| h.1 == a).unwrap().0; m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]) };
    assert!(matches!(press(&mut m, Act::BriefStart), Some(Go::Brief(2))));
    assert!(matches!(press(&mut m, Act::BriefQuit), Some(Go::ToMenu)));
}

/// The HUD's briefing button mid-mission: Start and Esc resume the game (no reload, no clock reset), the difficulty
/// can't be changed.
#[test]
fn mid_mission_briefing_resumes() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
    let mut m = Menu::briefing_mid(3, 2);
    m.draw(&mut u, 1920.0, 1080.0, [0.0; 2]);
    assert!(!m.hits.iter().any(|h| h.1 == Act::Difficulty));
    let p = m.hits.iter().find(|h| h.1 == Act::BriefStart).unwrap().0;
    assert!(matches!(m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]), Some(Go::Resume)));
    assert!(matches!(Menu::briefing_mid(3, 2).back(), Some(Go::Resume)));
}
