//! The tutorial panel (HUD_Tutorial, game type 4) over the game: the engine's render records type 23 (GameMap case
//! 23 -> HUD_Tutorial.setTutorialText / showTutorialButton / clearTutorialButton) set its heading, body and two
//! buttons; drawn 720×200 at the top centre (IngameUIScreens: HorizontalAlignment Center, Margin 0,100). A child
//! module of menu.rs: it draws with the menu's helpers. The step's arrow (HUD_Main.ShowTutorialArrow /
//! monitorTutorialArrows) points at the HUD button the text talks about.
use super::*;

/// What the engine asked for, as (TEXT section number, line) pairs: heading, body, Continue (button 1) / Exit
/// (button 2) captions. `visible` = Show_HUD_Tutorial (Continue with Exit showing closes it); `mode` = the last app
/// mode / sub mode seen (EditorDirector.tutorialAppModes); `arrow` = HUD_Main.currentTutorialArrow (-1 none), set when
/// a new body text comes without the warning flag (`last_body`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tutorial { pub heading: Option<(i32, i32)>, pub body: Option<(i32, i32)>, pub buttons: [Option<(i32, i32)>; 2], pub visible: bool, pub mode: (i32, i32),
    pub arrow: i32, last_body: Option<(i32, i32)> }

impl Default for Tutorial {
    /// HUD_Tutorial.OpenTutorial: shown, no text yet, both buttons hidden.
    fn default() -> Self { Tutorial { heading: None, body: None, buttons: [None; 2], visible: true, mode: (0, 0), arrow: -1, last_body: None } }
}

/// A HUD_Tutorial button: what main.rs does with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Button { Continue, Exit }

impl Tutorial {
    /// Render record type 23: r[4] 0 = text (heading r[3] / r[11], body r[13] / r[14]; r[12] = a warning, which only
    /// keeps the HUD arrow), 1 = button r[13] with caption r[3] / r[11] (r[11] < 0 hides both).
    pub fn record(&mut self, r: &[i16; 21]) {
        let (section, line) = (r[3] as i32, r[11] as i32);
        match r[4] {
            0 => {
                let body = (r[13] as i32, r[14] as i32);
                (self.heading, self.body) = (Some((section, line)), Some(body));
                if r[12] <= 0 && self.last_body != Some(body) { // setTutorialText: ShowTutorialArrow per body line
                    self.last_body = Some(body);
                    self.arrow = match body {
                        (95, 9) => 1, (95, 22) => 2, (95, 33) => 3, (95, 42) => 4, (95, 46) => 5, (95, 49) => 6, (95, 63 | 64) => 7,
                        (95, 66) => 8, (95, 120) => 10, (95, 87) => 12, (95, 133) => 13, (95, 136) => 14, (95, 139) => 15,
                        (95, 142) => 16, (95, 146) => 17, (95, 149) => 18, _ => -1,
                    };
                }
            }
            1 if line < 0 => self.buttons = [None; 2],
            1 => self.buttons[if r[13] == 1 { 0 } else { 1 }] = Some((section, line)),
            _ => {}
        }
    }

    /// The panel's layout in the 1920 grid for a `w`×`h` window: scale, offset (horizontally centred, from the top).
    fn layout(w: f32, h: f32) -> Menu {
        let mut m = Menu::main();
        m.s = (w / 1920.0).min(h / 1080.0);
        m.o = [(w - 1920.0 * m.s) / 2.0, 0.0];
        m
    }
    /// The visible buttons' design rects (BTN_SH_GlowS 183 wide in a centred StackPanel, the second 200 to the right,
    /// 26 above the panel's bottom).
    fn button_rects(&self) -> [Option<(f32, f32)>; 2] {
        let [a, b] = [self.buttons[0].is_some(), self.buttons[1].is_some()];
        let width = a as i32 as f32 * 183.0 + b as i32 as f32 * 383.0;
        let x0 = 600.0 + (720.0 - width) / 2.0;
        [a.then_some((x0, 246.0)), b.then_some((x0 + if a { 383.0 } else { 200.0 }, 246.0))]
    }
    /// The button under window point `c`.
    pub fn hit(&self, w: f32, h: f32, c: [f32; 2]) -> Option<Button> {
        if !self.visible { return None; }
        let m = Self::layout(w, h);
        let rects = self.button_rects();
        [Button::Continue, Button::Exit].into_iter().zip(rects).find(|(_, r)| matches!(r, Some((x, y)) if Menu::inside(m.r(*x, *y - 1.5, 183.0, 31.0), c))).map(|(b, _)| b)
    }
    pub fn over(&self, w: f32, h: f32, c: [f32; 2]) -> bool {
        self.visible && Menu::inside(Self::layout(w, h).r(600.0, 100.0, 720.0, 200.0), c)
    }

    /// The panel: #a0000000 fill, BDR_SH1DEc frame, heading 22 px and body 18 px (line height 22, 668 wide, wrapped,
    /// centred) in #DDDDDD, the buttons.
    pub fn draw(&self, ui: &mut Ui, st: &crate::hud::HudState, w: f32, h: f32, cursor: [f32; 2]) -> Vec<HudTri> {
        let mut out = vec![];
        self.draw_arrow(ui, st, &mut out); // in the HUD, under the panel
        if !self.visible { return out; }
        let mut m = Self::layout(w, h);
        let saved = (ui.s, ui.w, ui.h);
        (ui.s, ui.w, ui.h) = (m.s, w, h);
        const GREY: [f32; 4] = [0.723, 0.723, 0.723, 1.0]; // #DDDDDD (linear, like menu.rs)
        Ui::fill(&mut out, m.r(600.0, 100.0, 720.0, 200.0), [0.0, 0.0, 0.0, 0xa0 as f32 / 255.0]);
        m.frame(&mut out, 600.0, 100.0, 720.0, 200.0, false);
        if let Some((s, l)) = self.heading { m.label(ui, &mut out, &text(ui, s, l), 960.0, 131.0, 22.0, 0.5, GREY); }
        if let Some((s, l)) = self.body {
            let body = body_text(ui, s, l);
            for (k, line) in wrap(ui, &body, 668.0 * m.s, 18.0).iter().enumerate().take(4) {
                m.label(ui, &mut out, line, 960.0, 150.0 + 11.0 + 22.0 * k as f32, 18.0, 0.5, GREY);
            }
        }
        for (r, cap) in self.button_rects().into_iter().zip(self.buttons) {
            if let (Some((x, y)), Some((s, l))) = (r, cap) { m.glow(ui, &mut out, &text(ui, s, l), x, y, 183.0, None, Some(Act::Nop), cursor); }
        }
        encode(&mut out);
        (ui.s, ui.w, ui.h) = saved;
        out
    }
}

impl Tutorial {
    /// monitorTutorialArrows: which TutorialArrowN shows for the current arrow, as (parent, margin left, margin top):
    /// parent 0 = HUD_Main's LayoutRoot, 1 = HUD_Buildings' InBuildingLayoutRoot, 2 = MainHUD's ReportsControl. A
    /// build-menu arrow moves from the tab to the button once that tab is open (`screen` = Ui::screen; the original
    /// tests app_sub_mode, which its tab clicks set: castle 10, industry 20, farm 40, town 30, weapons 28).
    fn arrow_at(&self, st: &crate::hud::HudState, screen: usize) -> Option<(u8, f32, f32)> {
        let panel = st.app_mode == 16;
        let tab = |k: usize, on: (f32, f32), off: (f32, f32)| Some(if screen == k { (0, on.0, on.1) } else { (0, off.0, off.1) });
        match self.arrow {
            1 if screen == 0 => Some((0, 222.0, 58.0)),   // buildButtons[81] (keep) visible
            2 if screen == 6 => Some((0, 169.0, 26.0)),   // buildButtons[4] (granary)
            3 if panel && st.app_sub_mode == 4 => Some((1, 381.0, 54.0)),   // rations
            4 if panel && st.app_sub_mode == 2 => Some((1, 494.0, -33.0)),  // taxes
            5 => Some((2, 146.0, -42.0)),                 // popularity on the book
            6 => Some((1, 330.0, -113.0)),
            7 => Some((0, 222.0, 112.0)),
            8 => tab(2, (165.0, 56.0), (167.0, 160.0)),   // industry tab -> woodcutter
            10 => tab(3, (310.0, 33.0), (203.0, 160.0)),  // farm tab -> hunter
            12 => tab(4, (143.0, 31.0), (238.0, 160.0)),  // town tab -> hovel
            13 => tab(5, (163.0, 15.0), (277.0, 160.0)),  // weapons tab -> fletcher
            14 => tab(1, (497.0, 10.0), (132.0, 160.0)),  // castle tab -> armoury
            15 => tab(1, (377.0, 30.0), (132.0, 160.0)),  // castle tab -> barracks
            16 if panel && st.app_sub_mode == 1 => Some((1, 31.0, -150.0)),  // archer in the barracks
            17 => tab(1, (436.0, 15.0), (132.0, 160.0)),  // castle tab -> mercenary post
            18 if panel && st.app_sub_mode == 44 => Some((1, 23.0, -150.0)), // arab bow in the mercenary post
            _ => None,
        }
    }

    /// The arrow: floats_new-260..269 (GameSprites 73..82, a frame per 60 ms), 40×82, left-aligned at its margin and
    /// vertically centred in what the margin leaves of its parent (Image with a Height, VerticalAlignment Stretch).
    /// Only while its parent shows: HUD_Main outside panels, HUD_Buildings in one, the book always.
    fn draw_arrow(&self, ui: &mut Ui, st: &crate::hud::HudState, out: &mut Vec<HudTri>) {
        if ui.hidden { return; }
        let Some((parent, ml, mt)) = self.arrow_at(st, ui.screen) else { return };
        let troops = st.app_mode == 14 && matches!(st.app_sub_mode, 61 | 62);
        let shown = match parent { 0 => !troops && st.app_mode != 16, 1 => !troops && st.app_mode == 16, _ => true };
        if !shown { return; }
        let (x0, y0, ph) = [(1006.0, 774.0, 306.0), (1120.0, 885.0, 195.0), (1694.0, 883.0, 197.0)][parent as usize];
        let (x, y) = (x0 + ml, y0 + mt + (ph - mt - 82.0) / 2.0);
        let frame = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() / 60 % 10);
        let p = [ui.w - (1920.0 - x) * ui.s, ui.h - (1080.0 - y) * ui.s];
        ui.img_fit(out, &format!("floats_new-{}", 260 + frame), [p[0], p[1], p[0] + 40.0 * ui.s, p[1] + 82.0 * ui.s], [1.0; 4]);
    }
}

/// Translate.lookUpText(section, line): the TEXT_* key by section number (re/data/text_sections.txt = Enums.eTextSections).
fn text(ui: &Ui, section: i32, line: i32) -> String {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    let names = NAMES.get_or_init(|| include_str!("../../re/data/text_sections.txt").lines().collect());
    names.get(section as usize).filter(|n| !n.is_empty()).map_or(String::new(), |n| ui.text_of(&format!("{n}_{:03}", line + 1)))
}

/// HUD_Tutorial.setTutorialText's swaps: lines that depend on the control settings or show a key binding.
fn body_text(ui: &Ui, section: i32, line: i32) -> String {
    let cfg = skirmish::Settings::load();
    let (s, l, key) = match (section, line) {
        (95, 6) if !cfg.flag("PushMapScrolling", true) => (233, 1, Some(["Up", "Left", "Down", "Right"].as_slice())),
        (95, 62) => (233, 2, None),
        (95, 99) if !cfg.flag("SH1MouseWheel", false) => (233, 10, None),
        (95, 106) => (233, 11, Some(["FlattenLandscape"].as_slice())),
        _ => (section, line, None),
    };
    let t = text(ui, s, l);
    let Some(funcs) = key else { return t };
    let keys = crate::keys::Keys::load();
    let names: Vec<String> = funcs.iter().map(|f| key_name(ui, keys.get(f)[0])).collect();
    t.replace("[KEY]", &format!("[ {} ]", names.join(", ")))
}

/// Word wrap to `width` window px at `size`.
fn wrap(ui: &mut Ui, s: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in s.split_whitespace() {
        let last = lines.last_mut().unwrap();
        let next = if last.is_empty() { word.to_string() } else { format!("{last} {word}") };
        if !last.is_empty() && ui.text(&mut vec![], &next, 0.0, 0.0, size, 1.0, 0.0, 0.0, [1.0; 4]) > width { lines.push(word.to_string()) } else { *last = next }
    }
    lines
}

#[test]
fn tutorial_records() {
    let mut t = Tutorial::default();
    fn rec<const N: usize>(k: [(usize, i16); N]) -> [i16; 21] { let mut r = [0i16; 21]; r[0] = 23; for (i, v) in k { r[i] = v } r }
    t.record(&rec([(4, 0), (3, 94), (11, 1), (13, 95), (14, 2)]));
    t.record(&rec([(4, 1), (3, 94), (11, 3), (13, 1), (0, 23)]));
    assert_eq!((t.heading, t.body, t.buttons), (Some((94, 1)), Some((95, 2)), [Some((94, 3)), None]));
    assert_eq!(t.button_rects()[0], Some((868.5, 246.0))); // one button: centred
    t.record(&rec([(4, 1), (3, 94), (11, 4), (13, 2), (0, 23)]));
    assert_eq!(t.button_rects(), [Some((677.0, 246.0)), Some((1060.0, 246.0))]);
    t.record(&rec([(4, 1), (11, -1), (0, 23), (1, 0), (2, 0)]));
    assert_eq!(t.buttons, [None; 2]);
    // ShowTutorialArrow: body 95/66 -> arrow 8 (industry tab, the woodcutter once that tab is open); a warning keeps it
    t.record(&rec([(4, 0), (3, 94), (11, 1), (13, 95), (14, 66)]));
    let st = crate::hud::HudState::default();
    assert_eq!((t.arrow, t.arrow_at(&st, 1), t.arrow_at(&st, 2)), (8, Some((0, 167.0, 160.0)), Some((0, 165.0, 56.0))));
    t.record(&rec([(4, 0), (3, 94), (11, 1), (13, 95), (14, 70), (12, 1)]));
    assert_eq!(t.arrow, 8);
    t.record(&rec([(4, 0), (3, 94), (11, 1), (13, 95), (14, 70)]));
    assert_eq!((t.arrow, t.arrow_at(&st, 2)), (-1, None));
}
