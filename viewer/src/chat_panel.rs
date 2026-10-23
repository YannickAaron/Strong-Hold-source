//! The in-game chat panel (HUD_MPChatPanel.xaml: 1240×350, top centre at y 100 in the 1920×1080 grid): the players
//! to message with their mute buttons, Select All / Your Team / Enemies, the 20 insult buttons, the message line,
//! Send / Exit and the three mute checkboxes. lan.rs keeps the state and acts on the clicks; this child module of
//! menu.rs only draws with the menu's helpers and returns what was hit.
use super::*;

/// What lan.rs wants shown: per listed player (number, name, colour, selected, muted); Your Team shown; the line (or
/// the hovered insult's text instead); Send enabled; the checkboxes MuteInsults, MuteInsultSpeech, MPChatMuted.
pub struct View { pub players: Vec<(i32, String, [f32; 4], bool, bool)>, pub team: bool, pub line: String, pub insult: Option<String>, pub send: bool, pub checks: [bool; 3] }

/// Click targets (Act::Pick codes): player row k, its mute, All / Team / Enemies, insult n (1..20), Send, Exit, checkbox k.
pub const PLAYER: usize = 0;
pub const MUTE: usize = 10;
pub const ALL: usize = 20;
pub const TEAM: usize = 21;
pub const ENEMIES: usize = 22;
pub const INSULT: usize = 30; // + 1..=20
pub const SEND: usize = 60;
pub const EXIT: usize = 61;
pub const CHECK: usize = 70; // + 0..3

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

fn layout(w: f32, h: f32) -> Menu {
    let mut m = Menu::main();
    m.s = crate::ux::hud_scale(w, h);
    m.o = [w / 2.0 - 620.0 * m.s, 100.0 * m.s];
    m
}

/// The panel's rect in window px (its clicks don't reach the map).
pub fn rect(w: f32, h: f32) -> [f32; 4] { layout(w, h).r(0.0, 0.0, 1240.0, 350.0) }

/// Draws the panel; returns it and the click targets (window rect, code).
pub fn draw(ui: &mut Ui, v: &View, w: f32, h: f32, cursor: [f32; 2]) -> (Vec<HudTri>, Vec<([f32; 4], usize)>) {
    let mut m = layout(w, h);
    let saved = (ui.s, ui.w, ui.h);
    (ui.s, ui.w, ui.h) = (m.s, w, h);
    let mut out = vec![];
    let tx = |ui: &Ui, k: &str| ui.text_of(k);
    m.frame(&mut out, 0.0, 0.0, 1240.0, 350.0, true); // BDR_SH1DEb
    m.label(ui, &mut out, &tx(ui, "TEXT_MULTIPLAYER_CONNECTION_031"), 620.0, 23.0, 22.0, 0.5, BLACK);
    // players: 4 a row, 242 apart, rows 36 apart; the name in the player's colour, a gold edge when selected
    m.label(ui, &mut out, &tx(ui, "TEXT_NEW_CTEXT_264"), 262.0, 53.0, 18.0, 0.0, BLACK);
    for (k, (_, name, colour, on, muted)) in v.players.iter().enumerate().take(8) {
        let (x, y) = (262.0 + 242.0 * (k % 4) as f32, 64.0 + 36.0 * (k / 4) as f32);
        m.glow(ui, &mut out, "", x, y, 232.0, None, Some(Act::Pick(PLAYER + k)), cursor);
        if *on {
            let p = m.r(x + 2.0, y - 1.5, 228.0, 31.0);
            let e = 2.0 * m.s;
            for b in [[p[0], p[1], p[2], p[1] + e], [p[0], p[3] - e, p[2], p[3]], [p[0], p[1], p[0] + e, p[3]], [p[2] - e, p[1], p[2], p[3]]] { Ui::fill(&mut out, b, [1.0, 0.68, 0.0, 1.0]); }
        }
        m.label(ui, &mut out, name, x + 116.0, y + 15.0, 16.0, 0.5, *colour);
        // the mute button: GameSprites 693 / 694 (Un Mute) or 695 / 696 (Mute) when muted
        let s = if *muted { ["Mute Up", "Mute Over", "Mute Over"] } else { ["Un Mute Up", "Un Mute Over", "Un Mute Over"] };
        m.img_button(ui, &mut out, s, x - 6.0, y + 2.0, 26.0, 26.0, Some(Act::Pick(MUTE + k)), cursor);
    }
    // Select: All / Your Team (2+ humans in it) / Enemies
    m.label(ui, &mut out, &tx(ui, "TEXT_NEW_CTEXT_269"), 138.0, 35.0, 18.0, 0.5, BLACK);
    m.glow(ui, &mut out, &tx(ui, "TEXT_MULTIPLAYER_CONNECTION_032"), 22.0, 46.0, 232.0, None, Some(Act::Pick(ALL)), cursor);
    if v.team { m.glow(ui, &mut out, &tx(ui, "TEXT_MULTIPLAYER_CONNECTION_036"), 22.0, 82.0, 232.0, None, Some(Act::Pick(TEAM)), cursor); }
    m.glow(ui, &mut out, &tx(ui, "TEXT_NEW_CTEXT_263"), 22.0, 118.0, 232.0, None, Some(Act::Pick(ENEMIES)), cursor);
    // insults 1..20, 56 wide every 60
    m.label(ui, &mut out, &tx(ui, "TEXT_NEW_CTEXT_265"), 20.0, 165.0, 18.0, 0.0, BLACK);
    for n in 1..=20 {
        m.glow(ui, &mut out, &n.to_string(), 22.0 + 60.0 * (n - 1) as f32, 176.0, 56.0, None, Some(Act::Pick(INSULT + n)), cursor);
    }
    // the message line (TextBox 1200×33, MaxLength 100) or the hovered insult (22 px black)
    m.label(ui, &mut out, &tx(ui, "TEXT_NEW_CTEXT_266"), 20.0, 222.0, 18.0, 0.0, BLACK);
    match &v.insult {
        Some(t) => { m.label(ui, &mut out, t, 20.0, 249.0, 22.0, 0.0, BLACK); }
        None => {
            super::widget(&mut out, |o| Ui::text_field(o, m.r(20.0, 233.0, 1200.0, 33.0), true, m.s));
            let caret = if std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() / 500 % 2) == 0 { "_" } else { "" };
            m.label(ui, &mut out, &format!("{}{caret}", v.line), 26.0, 249.5, 20.0, 0.0, [0.94, 0.95, 0.78, 1.0]);
        }
    }
    m.glow(ui, &mut out, &tx(ui, "TEXT_MULTIPLAYER_CONNECTION_034"), 746.0, 299.0, 232.0, None, v.send.then_some(Act::Pick(SEND)), cursor);
    m.glow(ui, &mut out, &tx(ui, "TEXT_MULTIPLAYER_CONNECTION_035"), 988.0, 299.0, 232.0, None, Some(Act::Pick(EXIT)), cursor);
    // MuteInsultSpeech (bottom 12), MuteInsults (35), MPChatMuted (58): CheckBox_Template + 20 px black text
    for (k, (key, y)) in [("TEXT_NEW_CTEXT_267", 292.0), ("TEXT_NEW_CTEXT_268", 315.0), ("TEXT_NEW_CTEXT_416", 269.0)].into_iter().enumerate() {
        m.checkbox(&mut out, 20.0, y + 1.5, v.checks[k], Some(Act::Pick(CHECK + k)));
        m.label(ui, &mut out, &tx(ui, key), 46.0, y + 11.5, 20.0, 0.0, BLACK);
    }
    encode(&mut out);
    (ui.s, ui.w, ui.h) = saved;
    let hits = m.hits.iter().filter_map(|(p, a)| if let Act::Pick(c) = a { Some((*p, *c)) } else { None }).collect();
    (out, hits)
}
