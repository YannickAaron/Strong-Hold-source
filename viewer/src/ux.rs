//! A modern Mac game around the original (nothing here touches the sim): trackpad gestures (gestures.rs), the macOS
//! menu bar (macos.rs), frame pacing (every display refresh up front, ProMotion up to 120 Hz; a few fps when the window
//! is hidden or the app is in the background), pause while the app is in the background, a user UI scale, the F1
//! keyboard-shortcut sheet and short notices ("Game saved"). Hooked into main.rs by one-line calls (ux_event,
//! ux_about_to_wait, ux_resumed, overlay, continuous, hud_scale).

use std::{sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed}, time::{Duration, Instant}};
use winit::{event::{ElementState, MouseScrollDelta, TouchPhase, WindowEvent}, event_loop::{ActiveEventLoop, ControlFlow}, keyboard::{Key, NamedKey}, window::Window};
use crate::{gestures, hud::{HudTri, Ui}, keys::Keys, App};

/// App commands: the macOS menu bar (macos.rs) and the Ctrl shortcuts elsewhere ask for these.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)] // the quit sheet's answers and some items exist on macOS only
pub enum Cmd { Quit, Save, Load, Options, Shortcuts, UiBigger, UiSmaller, UiReset, PauseInBackground, QuitYes, QuitNo, FullScreen }

#[derive(Default)]
pub struct Ux {
    unfocused: bool, // the app is in the background (another app is active, or a sheet has the keys)
    hidden: bool,    // the window can't be seen: minimized, on another Space, covered
    auto_paused: bool, // the game was paused because the app went to the background: resume when it's back
    asking: Option<bool>, // the quit sheet is up; Some(true) = resume the game if it's cancelled
    turn: gestures::Turn,
    pub help: bool, // the F1 sheet is shown
    toast: Option<(String, Instant)>,
}

// Scripted runs (crate::scripted: SHC_SHOT / SHC_INPUT) render every frame, never pause themselves and quit without
// asking: their frame counts and shots must not depend on whether the window happens to be in front.
use crate::scripted;

impl Ux {
    /// Time between frames: None = every display refresh (vsync). 30 fps while the app is in the background but the
    /// window visible, 5 fps while it's hidden (the sim thread paces itself, so the game runs on at its speed).
    fn interval(&self) -> Option<Duration> {
        if scripted() { None }
        else if self.hidden { Some(Duration::from_millis(200)) }
        else if self.unfocused { Some(Duration::from_millis(33)) }
        else { None }
    }
    /// main.rs: ask for the next frame right away (else about_to_wait wakes it).
    pub fn continuous(&self) -> bool { self.interval().is_none() }
    pub(crate) fn note(&mut self, s: impl Into<String>) { self.toast = Some((s.into(), Instant::now())); }

    /// Focus lost / back (`on`) while the live game is `playing` (None = no live game): Some(x) = set playing to x.
    /// Leaving pauses it (option `pause_bg`, default on; not under a menu or the quit sheet); coming back resumes it
    /// only if that pause was ours. Scripted runs never pause themselves.
    fn focus_change(&mut self, on: bool, playing: Option<bool>, menu: bool, pause_bg: bool) -> Option<bool> {
        self.unfocused = !on;
        if scripted() { return None; }
        if !on {
            if pause_bg && !menu && self.asking.is_none() && playing == Some(true) { self.auto_paused = true; return Some(false); }
        } else if std::mem::take(&mut self.auto_paused) && !menu && playing == Some(false) {
            return Some(true);
        }
        None
    }
}

// ---- settings: UI scale (75..150 %), pause in background; settings.cfg keys of our own so the game's never clash
static UI_PCT: AtomicU32 = AtomicU32::new(100);
static DPR: AtomicU32 = AtomicU32::new(0x4000_0000); // 2.0f32: Retina until the window says otherwise
static PAUSE_BG: AtomicBool = AtomicBool::new(true);
const UI_STEPS: [u32; 7] = [75, 85, 100, 110, 125, 140, 150];

// ---- Options applied outside the menu (menu::opt): VSync, Lock Cursor, the screen mode, Mute when not Focused
static FOCUSED: AtomicBool = AtomicBool::new(true);
static SCREEN_W: AtomicU32 = AtomicU32::new(1920);
static SCREEN_H: AtomicU32 = AtomicU32::new(1080);
/// The app is in front (Application.isFocused): UI sounds only then (SFXManager.playUISound).
pub fn focused() -> bool { FOCUSED.load(Relaxed) }
/// The screen's size in logical px (Options' window sizes must fit it).
pub fn screen_size() -> [f32; 2] { [SCREEN_W.load(Relaxed) as f32, SCREEN_H.load(Relaxed) as f32] }

/// Options > Screen Mode: 0 Fullscreen (macOS: its own Space), 1 Borderless (macOS: the "simple" full screen over
/// the menu bar, no Space; elsewhere borderless full screen), 2 Windowed at the Resolution picked (menu::window_size).
pub fn set_mode(window: &Window, mode: u8) {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowExtMacOS;
        window.set_simple_fullscreen(false);
        if mode == 1 { window.set_fullscreen(None); window.set_simple_fullscreen(true); return; }
    }
    crate::set_windowed(window, mode == 2);
}

/// Options > Video > VSync (ours, "NativeVSync", on by default); SHC_NOVSYNC=1 wins (uncapped, for timing).
pub fn present_mode() -> wgpu::PresentMode {
    if std::env::var_os("SHC_NOVSYNC").is_some() || !crate::menu::opt("NativeVSync", true) { wgpu::PresentMode::AutoNoVsync } else { wgpu::PresentMode::AutoVsync }
}

pub fn ui_scale_pct() -> u32 { UI_PCT.load(Relaxed) }
pub fn ui_scale() -> f32 { ui_scale_pct() as f32 / 100.0 }
/// The in-game UI's logical px per design px for a `w`×`h` logical-px window: MainViewModel.ScaleIngameUI at its
/// default Settings_UIScale 1 makes the UI a W/k × H/k grid (W, H in device px), k = min(W/1360, H/768), or 1 on
/// screens up to 1366×768; that's k / dpr logical px. Times the player's UI size (⌘= / ⌘-), but never wider than the
/// window: the HUD's two corner blocks span about 1200 design px.
pub fn hud_scale(w: f32, h: f32) -> f32 {
    let d = dpr();
    let (wd, hd) = (w * d, h * d);
    let k = if wd <= 1366.0 || hd <= 768.0 { 1.0 } else { (wd / 1360.0).min(hd / 768.0) };
    (k / d * ui_scale()).min(w / 1200.0)
}
/// Device px per logical px (2 on Retina): glyphs are rasterised at this density.
pub fn dpr() -> f32 { f32::from_bits(DPR.load(Relaxed)) }
pub fn pause_in_background() -> bool { PAUSE_BG.load(Relaxed) }

fn save(key: &str, v: impl ToString) {
    let mut cfg = crate::skirmish::Settings::load();
    cfg.set(key, v);
    if let Err(e) = cfg.save() { eprintln!("settings.cfg: {e}"); }
}
pub fn set_ui_scale_pct(p: u32) {
    UI_PCT.store(p.clamp(75, 150), Relaxed);
    save("NativeUIScale", ui_scale_pct());
}
pub fn set_pause_in_background(on: bool) {
    PAUSE_BG.store(on, Relaxed);
    save("NativePauseInBackground", on);
    #[cfg(target_os = "macos")]
    crate::macos::set_pause_check(on);
}
/// The next UI step up (`dir` 1) or down (-1) from `p`.
fn ui_step(p: u32, dir: i32) -> u32 {
    if dir > 0 { UI_STEPS.into_iter().find(|&s| s > p).unwrap_or(150) } else { UI_STEPS.into_iter().rev().find(|&s| s < p).unwrap_or(75) }
}

// ---- key names for people (the F1 sheet)
/// A bound key as people write it: "Alpha1,ctrl" -> "Ctrl+1", "KeypadPlus" -> "Num +", "F1,shift" -> "Shift+F1".
pub fn key_name(k: &str) -> String {
    let mut p = k.split(',');
    let code = p.next().unwrap_or("");
    let mut out: String = p.filter_map(|m| Some(match m { "shift" => "Shift+", "ctrl" => "Ctrl+", "alt" => "Option+", _ => return None })).collect();
    let name = match code {
        c if c.starts_with("Alpha") => c[5..].to_string(),
        c if c.starts_with("Keypad") => format!("Num {}", match &c[6..] { "Plus" => "+", "Minus" => "-", "Multiply" => "*", "Divide" => "/", "Period" => ".", "Equals" => "=", "Enter" => "Enter", d => d }),
        "Equals" => "=".into(), "Minus" => "-".into(), "Plus" => "+".into(), "BackQuote" => "`".into(), "Backslash" => "\\".into(),
        "LeftBracket" => "[".into(), "RightBracket" => "]".into(), "Semicolon" => ";".into(), "Quote" => "'".into(), "Comma" => ",".into(),
        "Period" => ".".into(), "Slash" => "/".into(), "Escape" => "Esc".into(), "Return" => "Return".into(),
        "UpArrow" => "Up Arrow".into(), "DownArrow" => "Down Arrow".into(), "LeftArrow" => "Left Arrow".into(), "RightArrow" => "Right Arrow".into(),
        "PageUp" => "Page Up".into(), "PageDown" => "Page Down".into(),
        c => c.to_string(),
    };
    out.push_str(&name);
    out
}

/// The keys bound to KeyFunctions `f` right now ("P", "Num + / ="), None if unbound.
fn label_of(keys: &Keys, f: &str) -> Option<String> {
    let names: Vec<String> = keys.get(f).iter().filter(|k| !k.is_empty() && !k.ends_with(",mp")).map(|k| key_name(k)).collect();
    (!names.is_empty()).then(|| names.join(" / "))
}

/// The in-game functions the port runs (main.rs hotkey / key_event): the sheet lists these, and F1 opens the sheet
/// unless the table puts one of them on F1.
const PORTED: [&str; 12] = ["Pause", "IncreaseEngineSpeed", "DecreaseEngineSpeed", "ZoomIn", "ZoomOut", "MapRotateLeft", "MapRotateRight",
    "Up", "Down", "Left", "Right", "OptionsMenu"];

impl App {
    /// After the window and GPU are up: settings, Retina factor, the menu bar.
    pub(crate) fn ux_resumed(&mut self) {
        let cfg = crate::skirmish::Settings::load();
        UI_PCT.store((cfg.num("NativeUIScale", 100.0) as u32).clamp(75, 150), Relaxed);
        PAUSE_BG.store(cfg.flag("NativePauseInBackground", true), Relaxed);
        // shots: SHC_UI_SCALE=150 (percent, not saved), SHC_HELP=1 opens the F1 sheet
        if let Some(p) = std::env::var("SHC_UI_SCALE").ok().and_then(|v| v.parse::<u32>().ok()) { UI_PCT.store(p.clamp(75, 150), Relaxed); }
        self.ux.help = std::env::var_os("SHC_HELP").is_some();
        if let Some(g) = &self.gfx {
            DPR.store((g.window.scale_factor() as f32).to_bits(), Relaxed);
            if let Some(m) = g.window.current_monitor() {
                let s = m.size().to_logical::<f32>(m.scale_factor());
                (SCREEN_W.store(s.width as u32, Relaxed), SCREEN_H.store(s.height as u32, Relaxed));
            }
            // Borderless is a full-screen window that main.rs opens as fullscreen: switch it over
            if cfg.num("FullscreenType", 1.0) as i32 == 1 && !scripted() && std::env::var_os("SHC_WINDOW").is_none() && !self.sheet { set_mode(&g.window, 1); }
        }
        self.apply_options();
        #[cfg(target_os = "macos")]
        crate::macos::install_menu(pause_in_background());
    }

    /// Options that act on the window / audio, after a change (main.rs Go::Settings) and at start: VSync, Lock Cursor,
    /// Mute when not Focused.
    pub(crate) fn apply_options(&mut self) {
        if let Some(g) = self.gfx.as_mut() {
            let pm = present_mode();
            // ponytail: on macOS winit paces redraws to the display either way; VSync off = Immediate presents (less latency)
            if g.config.present_mode != pm { g.config.present_mode = pm; g.surface.configure(&g.device, &g.config); println!("present mode {pm:?}"); }
        }
        self.lock_cursor();
        self.mute_in_background();
    }
    /// Lock Cursor to Window (FatControler: CursorLockMode.Confined) while the app is in front. macOS has no confined
    /// cursor (winit: not supported): the cursor is put back inside when it leaves (`ux_event` CursorLeft).
    fn lock_cursor(&self) {
        let Some(g) = &self.gfx else { return };
        let _ = g.window.set_cursor_grab(if self.cursor_locked() { winit::window::CursorGrabMode::Confined } else { winit::window::CursorGrabMode::None });
    }
    fn cursor_locked(&self) -> bool { crate::menu::opt("LockCursor", false) && !self.ux.unfocused && !scripted() }
    /// MyAudioManager.GetMasterVolume: 0 while the app is in the background and "Mute when not Focused" is on
    /// (settings.cfg BackgroundAudio False).
    fn mute_in_background(&self) {
        let (m, mu, sp, us, sfx, red) = crate::skirmish::Settings::load().volumes();
        let mute = self.ux.unfocused && !crate::menu::opt("BackgroundAudio", true);
        crate::audio::set_volumes(if mute { 0.0 } else { m }, mu, sp, us, sfx, red);
    }

    fn live(&self) -> bool { self.play.as_ref().is_some_and(|p| p.live.is_some()) }

    /// Play / pause the live game (the sim thread hears it at once); returns whether it was playing.
    fn set_playing(&mut self, on: bool) -> bool {
        let Some(p) = self.play.as_mut().filter(|p| p.live.is_some()) else { return false };
        let was = p.playing;
        if was != on { p.playing = on; self.tick_replay(0.0, 0); }
        was
    }

    /// Window events this module handles; true = taken (main.rs's own match is skipped). Everything else falls through.
    pub(crate) fn ux_event(&mut self, el: &ActiveEventLoop, ev: &WindowEvent) -> bool {
        match ev {
            WindowEvent::CloseRequested => { self.quit(el); true }
            WindowEvent::Focused(on) => { self.focus(*on); false }
            WindowEvent::Occluded(o) => { self.ux.hidden = *o; self.wake(); false }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => { DPR.store((*scale_factor as f32).to_bits(), Relaxed); false }
            // Lock Cursor on macOS: the cursor left the window, put it back just inside, where it went out
            WindowEvent::CursorLeft { .. } if self.cursor_locked() => {
                if let Some(g) = &self.gfx {
                    let sz = g.window.inner_size().to_logical::<f64>(g.window.scale_factor());
                    let [x, y] = self.cam.cursor.map(f64::from);
                    let _ = g.window.set_cursor_position(winit::dpi::LogicalPosition::new(x.clamp(2.0, sz.width - 3.0), y.clamp(2.0, sz.height - 3.0)));
                }
                true
            }
            _ if scripted() => false, // shots: real gestures / keys stay out (main.rs ignores the real mouse too)
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat
                && !self.menu.as_ref().is_some_and(|m| m.capturing()) => {
                match &event.logical_key {
                    Key::Named(NamedKey::F1) if self.f1_is_help(event) => { self.ux.help = !self.ux.help; true }
                    Key::Named(NamedKey::Escape) if self.ux.help => { self.ux.help = false; true }
                    #[cfg(not(target_os = "macos"))]
                    _ => match self.ctrl_cmd(event) { Some(c) => { self.cmd(c, el); true } None => false },
                    #[cfg(target_os = "macos")]
                    _ => false, // the menu bar's key equivalents (macos.rs) never get here
                }
            }
            WindowEvent::PinchGesture { delta, phase, .. } => { self.pinch(*delta as f32, *phase); true }
            WindowEvent::RotationGesture { delta, phase, .. } if self.live() && self.menu.is_none() => {
                if *phase == TouchPhase::Started { self.ux.turn.reset(); }
                if let Some(left) = self.ux.turn.feed(*delta) { self.hotkey(if left { "MapRotateLeft" } else { "MapRotateRight" }); }
                true
            }
            // a trackpad's two-finger scroll (precise deltas, momentum included) pans; a mouse wheel's lines keep zooming
            WindowEvent::MouseWheel { delta: MouseScrollDelta::PixelDelta(d), .. } if self.live() && self.menu.is_none() => {
                let sf = self.gfx.as_ref().map_or(1.0, |g| g.window.scale_factor()) as f32;
                self.cam.pan([d.x as f32 / sf, d.y as f32 / sf]);
                true
            }
            _ => false,
        }
    }

    /// Linux / Windows: the menu bar's shortcuts with Ctrl (F11 = full screen) on keys the game's table leaves free. By
    /// key position: with Ctrl held some platforms report a control character as the logical key.
    #[cfg(not(target_os = "macos"))]
    fn ctrl_cmd(&self, ev: &winit::event::KeyEvent) -> Option<Cmd> {
        use winit::keyboard::{KeyCode as K, PhysicalKey};
        if self.live() && self.menu.is_none() && crate::keys::unity_key(ev, self.mods).is_some_and(|k| self.keys.action(&k).is_some()) { return None; }
        let PhysicalKey::Code(code) = ev.physical_key else { return None };
        let m = self.mods;
        if code == K::F11 && m.is_empty() { return Some(Cmd::FullScreen); }
        if !m.control_key() || m.alt_key() || m.super_key() || m.shift_key() { return None; }
        Some(match code {
            K::KeyS => Cmd::Save, K::KeyO => Cmd::Load, K::Comma => Cmd::Options, K::Slash => Cmd::Shortcuts,
            K::Equal | K::NumpadAdd => Cmd::UiBigger, K::Minus | K::NumpadSubtract => Cmd::UiSmaller,
            _ => return None,
        })
    }

    /// F1 = the sheet, unless the key table gives F1 to a function the port runs (Options > Hotkeys can).
    fn f1_is_help(&self, ev: &winit::event::KeyEvent) -> bool {
        let k = crate::keys::unity_key(ev, self.mods).unwrap_or_default();
        !(self.live() && self.menu.is_none() && self.keys.action(&k).is_some_and(crate::keys::runs)) // e.g. Shift+F1 = Save
    }

    /// Pinch: in game between the original's zoom levels (wheel / X / Z steps go on from the nearest one); the replay /
    /// map viewers zoom freely like their wheel.
    fn pinch(&mut self, mag: f32, _phase: TouchPhase) {
        let Some(g) = &self.gfx else { return };
        if self.menu.is_some() || self.intro.is_some() { return; }
        let (w, h, sf) = (g.config.width as f32, g.config.height as f32, g.window.scale_factor() as f32);
        let at = self.cam.cursor;
        if self.live() {
            let size = self.cam.map.map_or(800.0, |m| m.size);
            self.cam.pinch(mag, at, gestures::zoom_range(w, h, sf, size));
            self.cam.zoom_pos = gestures::nearest_pos(self.cam.zoom * sf, crate::zoom_pos_clamp(0.0, w, h, size));
        } else {
            self.cam.pinch(mag, at, (0.01, 8.0));
        }
    }

    /// The app went to the background / came back: pause / resume the live game (Ux::focus_change).
    fn focus(&mut self, on: bool) {
        let playing = self.play.as_ref().filter(|p| p.live.is_some()).map(|p| p.playing);
        if let Some(play) = self.ux.focus_change(on, playing, self.menu.is_some(), pause_in_background()) {
            self.set_playing(play);
            println!("app {}: game {}", if on { "in front" } else { "in the background" }, if play { "resumed" } else { "paused" });
        }
        FOCUSED.store(on, Relaxed);
        self.lock_cursor();
        self.mute_in_background();
        self.wake();
    }

    fn wake(&self) { if let Some(g) = &self.gfx { g.window.request_redraw(); } }

    /// Quit (⌘Q, the window's close button): asks first while a live game is on (paused meanwhile).
    pub(crate) fn quit(&mut self, el: &ActiveEventLoop) {
        let at_stake = self.play.as_ref().is_some_and(|p| p.live.is_some() && p.huds.last().is_none_or(|h| h.end.is_none()));
        if at_stake && !scripted() {
            if self.ux.asking.is_some() { return; }
            #[cfg(target_os = "macos")]
            {
                let was = self.set_playing(false);
                if self.gfx.as_ref().is_some_and(|g| crate::macos::ask_quit(&g.window)) { self.ux.asking = Some(was || std::mem::take(&mut self.ux.auto_paused)); return; }
            }
        }
        el.exit();
    }

    /// Once per event-loop pass: menu-bar commands, then the next frame's timing.
    pub(crate) fn ux_about_to_wait(&mut self, el: &ActiveEventLoop) {
        #[cfg(target_os = "macos")]
        for c in crate::macos::take() { self.cmd(c, el); }
        match self.ux.interval() {
            None => el.set_control_flow(ControlFlow::Wait), // RedrawRequested asks for the next frame itself (vsync paces it)
            Some(iv) => {
                let due = self.last + iv;
                if Instant::now() >= due { self.wake(); } else { el.set_control_flow(ControlFlow::WaitUntil(due)); }
            }
        }
    }

    /// Open a menu screen over whatever runs: in game the pause menu first (like Escape), then `f` on it.
    fn open_menu(&mut self, f: impl FnOnce(&mut crate::menu::Menu)) {
        self.intro = None;
        if self.menu.is_none() {
            if !self.live() { return; }
            self.set_playing(false);
            self.ux.auto_paused = false;
            self.menu = Some(crate::menu::Menu::pause(self.last_lobby.is_some() || self.campaign.is_some()));
        }
        f(self.menu.as_mut().unwrap());
    }

    /// Save Game from anywhere in a live game (⌘S, the pause menu, a hotkey): the game pauses under the pause menu and
    /// the Save dialog opens (menu.rs: name, overwrite check; Go::Save writes into skirmish::saves_dir, which Load lists).
    pub(crate) fn save_dialog(&mut self) { if self.live() { self.open_menu(|m| m.open_save()) } }

    pub(crate) fn cmd(&mut self, c: Cmd, el: &ActiveEventLoop) {
        match c {
            Cmd::Quit => self.quit(el),
            Cmd::QuitYes => el.exit(),
            // Cancel: the game runs on if it ran before (once the app is in front again; the sheet may still hold the keys)
            Cmd::QuitNo => if self.ux.asking.take() == Some(true) && self.menu.is_none() {
                if self.ux.unfocused { self.ux.auto_paused = true } else { self.set_playing(true); }
            },
            // Save Game: the pause menu's Save dialog (name, overwrite check; the sim thread writes the .sav)
            #[cfg(feature = "live")]
            Cmd::Save => self.save_dialog(),
            Cmd::Load => self.open_menu(|m| m.open_load()),
            Cmd::Options => self.open_menu(|m| m.open_options()),
            Cmd::Shortcuts => self.ux.help = !self.ux.help,
            Cmd::FullScreen => if let Some(g) = &self.gfx { crate::set_windowed(&g.window, g.window.fullscreen().is_some()) },
            Cmd::UiBigger | Cmd::UiSmaller | Cmd::UiReset => {
                set_ui_scale_pct(match c { Cmd::UiBigger => ui_step(ui_scale_pct(), 1), Cmd::UiSmaller => ui_step(ui_scale_pct(), -1), _ => 100 });
                self.ux.note(format!("UI size {} %", ui_scale_pct()));
            }
            Cmd::PauseInBackground => {
                set_pause_in_background(!pause_in_background());
                self.ux.note(if pause_in_background() { "Pause in background: on" } else { "Pause in background: off" });
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
}

const CREAM: [f32; 4] = [0xef as f32 / 255.0, 0xf3 as f32 / 255.0, 0xc6 as f32 / 255.0, 1.0];
const GOLD: [f32; 4] = [0.95, 0.78, 0.35, 1.0];
const DIM: [f32; 4] = [0.75, 0.75, 0.70, 1.0];

/// The Options > Hotkeys label key of each KeyFunctions name (re/data/hotkeys.tsv).
fn hotkey_label(ui: &Ui, f: &str) -> String {
    include_str!("../../re/data/hotkeys.tsv").lines().find_map(|l| { let mut c = l.split('\t'); (c.next() == Some(f)).then(|| c.next()).flatten() })
        .map_or_else(|| if f == "OptionsMenu" { "Game menu".into() } else { f.to_string() }, |k| ui.text_of(k).trim_end_matches([' ', ':']).to_string())
}

/// main.rs, last thing before drawing: the F1 sheet and the current notice on top of everything.
pub(crate) fn overlay(ux: &mut Ux, ui: Option<&mut Ui>, keys: &Keys, window: &Window, mut hud: Vec<HudTri>) -> Vec<HudTri> {
    let Some(ui) = ui else { return hud };
    if ux.toast.as_ref().is_some_and(|t| t.1.elapsed() > Duration::from_millis(2500)) { ux.toast = None; }
    if !ux.help && ux.toast.is_none() { return hud; }
    let sz = window.inner_size().to_logical::<f32>(window.scale_factor());
    let (w, h, s) = (sz.width, sz.height, hud_scale(sz.width, sz.height));
    let keep = (ui.s, ui.w, ui.h); // the HUD drops its glyph cache when ui.s changes: put it back afterwards
    (ui.s, ui.w, ui.h) = (s, w, h);
    let out = &mut hud;
    if let Some((t, _)) = &ux.toast {
        let tw = ui.text(&mut vec![], t, 0.0, 0.0, 26.0, 1.0, 0.0, 0.0, CREAM);
        let (x, y) = (w / 2.0, 90.0 * s);
        ui.panel(out, [x - tw / 2.0 - 24.0 * s, y - 40.0 * s, x + tw / 2.0 + 24.0 * s, y + 18.0 * s], s);
        ui.text(out, t, x, y, 26.0, 1.0, 0.0, 0.5, CREAM);
    }
    if ux.help { sheet(ui, out, keys, w, h, s); }
    (ui.s, ui.w, ui.h) = keep;
    hud
}

/// The keyboard / trackpad sheet: the game's ported keys as bound now (with the game's own labels), the Mac shortcuts,
/// the trackpad and mouse.
fn sheet(ui: &mut Ui, out: &mut Vec<HudTri>, keys: &Keys, w: f32, h: f32, s: f32) {
    let game: Vec<(String, String)> = PORTED.iter().map(|f| (hotkey_label(ui, f), label_of(keys, f).unwrap_or_else(|| "-".into()))).collect();
    #[cfg(target_os = "macos")]
    let (os, mac) = ("Mac", [("Save game", "Cmd+S"), ("Load game", "Cmd+O"), ("Options", "Cmd+,"), ("Full screen", "Ctrl+Cmd+F"),
        ("Minimize", "Cmd+M"), ("Hide", "Cmd+H"), ("Bigger / smaller UI", "Cmd+= / Cmd+-"), ("Normal UI size", "Cmd+0"),
        ("This sheet", "F1 / Cmd+/"), ("Quit", "Cmd+Q")]);
    #[cfg(not(target_os = "macos"))]
    let (os, mac) = ("Shortcuts", [("Save game", "Ctrl+S"), ("Load game", "Ctrl+O"), ("Options", "Ctrl+,"), ("Full screen", "F11 / Alt+Enter"),
        ("Bigger / smaller UI", "Ctrl+= / Ctrl+-"), ("This sheet", "F1 / Ctrl+/")]);
    let mac: Vec<(String, String)> = mac.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let pad: Vec<(String, String)> = [("Zoom", "Pinch / wheel"), ("Pan", "Two-finger scroll"), ("Pan", "Middle-drag / edges"),
        ("Turn the map", "Rotate two fingers"), ("Cancel tool", "Right click")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    // (title, rows, label width, key width) in design px
    let cols = [("Game", game, 250.0, 190.0), (os, mac, 220.0, 180.0), ("Trackpad & mouse", pad, 160.0, 200.0)];
    let gap = 60.0;
    let pw = cols.iter().map(|c| c.2 + c.3).sum::<f32>() + gap * 4.0;
    let s = s.min(w * 0.96 / pw).min(h * 0.96 / 640.0); // the whole sheet on screen, whatever the UI size
    ui.s = s;
    let (pw, ph, row) = (pw * s, 640.0 * s, 36.0 * s);
    let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
    ui.panel(out, [x0, y0, x0 + pw, y0 + ph], s);
    ui.text(out, "Keyboard & Trackpad", w / 2.0, y0 + 56.0 * s, 34.0, 1.0, 0.0, 0.5, CREAM);
    let mut x = x0 + gap * s;
    for (title, rows, lw, kw) in cols {
        ui.text(out, title, x, y0 + 112.0 * s, 26.0, 1.0, 0.0, 0.0, GOLD);
        for (i, (label, key)) in rows.iter().enumerate() {
            let y = y0 + 112.0 * s + row * (i as f32 + 1.2);
            ui.text(out, label, x, y, 21.0, 1.0, 0.0, 0.0, CREAM);
            ui.text(out, key, x + lw * s, y, 21.0, 1.0, 0.0, 0.0, GOLD);
        }
        x += (lw + kw + gap) * s;
    }
    ui.text(out, "F1 or Esc closes. Keys: Options > Hotkeys.", w / 2.0, y0 + ph - 24.0 * s, 19.0, 1.0, 0.0, 0.5, DIM);
}

#[test]
fn key_names_and_ui_steps() {
    assert_eq!((key_name("Alpha1,ctrl"), key_name("KeypadPlus"), key_name("F1,shift"), key_name("Equals")), ("Ctrl+1".into(), "Num +".into(), "Shift+F1".into(), "=".into()));
    assert_eq!(label_of(&Keys::modern(), "IncreaseEngineSpeed").as_deref(), Some("Num + / ="));
    assert_eq!(label_of(&Keys::modern(), "OpenChat"), Some("Num Enter".into())); // the ",mp" slot is left out
    assert_eq!((ui_step(100, 1), ui_step(100, -1), ui_step(150, 1), ui_step(75, -1), ui_step(90, 1)), (110, 85, 150, 75, 100));
    // ScaleIngameUI (dpr 2 in tests): MacBook full screen 1728×1080 -> k = 3456 / 1360; 1366×768 device px and less -> 1 px
    assert!((hud_scale(1728.0, 1080.0) - 3456.0 / 1360.0 / 2.0).abs() < 1e-6);
    assert_eq!(hud_scale(683.0, 384.0), 0.5);
}

/// Pacing: every refresh up front; 30 fps in the background, 5 fps hidden; scripted runs always every frame.
#[test]
fn frame_pacing() {
    if scripted() { return; }
    let mut u = Ux::default();
    assert!(u.continuous());
    u.unfocused = true;
    assert_eq!(u.interval(), Some(Duration::from_millis(33)));
    u.hidden = true;
    assert_eq!(u.interval(), Some(Duration::from_millis(200)));
    u.unfocused = false;
    assert!(!u.continuous()); // minimized but still "key": still throttled
}

/// Pause in background: leaving pauses a running game, coming back resumes only our own pause.
#[test]
fn focus_pause() {
    if scripted() { return; }
    let mut u = Ux::default();
    assert_eq!(u.focus_change(false, Some(true), false, true), Some(false)); // running -> paused
    assert_eq!(u.focus_change(true, Some(false), false, true), Some(true)); // back -> resumed
    assert_eq!(u.focus_change(false, Some(false), false, true), None); // paused by the player: stays paused
    assert_eq!(u.focus_change(true, Some(false), false, true), None); // ... and isn't resumed
    assert_eq!(u.focus_change(false, Some(true), false, false), None); // option off
    assert_eq!(u.focus_change(false, Some(true), true, true), None); // under a menu (already paused there)
    assert_eq!(u.focus_change(false, None, false, true), None); // no live game
    u.asking = Some(true);
    assert_eq!(u.focus_change(false, Some(false), false, true), None); // the quit sheet takes the keys: not ours
    u.asking = None;
    assert_eq!(u.focus_change(false, Some(true), false, true), Some(false));
    assert_eq!(u.focus_change(true, Some(false), true, true), None); // came back into a menu: stays paused
    assert!(!u.auto_paused);
}
