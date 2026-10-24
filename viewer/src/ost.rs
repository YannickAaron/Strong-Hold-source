//! OnScreenText (OnScreenText.cs updateOST + the OST_* TextBlocks of IngameUIScreens.xaml, MainHUD.xaml and
//! OST_StartingGoods.xaml): game paused, game speed, keep message, feedback line, who owns a building, the left stack
//! (win / defeat / peace timer bar, starting goods being carted into the stockpile) and the editor's item counts.
//! The engine posts entries with draw records 14 (add) / 15 (remove), `Events::scan` keeps them per slot; timed ones
//! run on the wall clock like the original's timedEnd. A child module of hud.rs: it draws with Ui's helpers.
use super::*;
use std::time::{Duration, Instant};

/// One eOnScreenText slot as the engine's records left it: data1..5 of the last add, serials of the last add / remove.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ost { pub data: [i32; 5], pub added: u32, pub removed: u32 }

/// Slot ids (Enums.eOnScreenText).
const FEEDBACK_1: usize = 4;
const STARTING_GOODS: usize = 12;
const KEEP_MESSAGE: usize = 20;
const WHO_OWNS: usize = 21;
const WIN_TIMER: usize = 26;
const TIMETODEFEAT: usize = 27;
const PEOPLE_LEFT: usize = 29;
const PEACETIMER: usize = 31;

/// The UI side of the slots: serials taken, timedEnd, when last added, data1 shown (wasTurnedOnOrChanged), and the
/// starting goods' timers (startingGoodsExpiry / StaticTime, the cart's start, the stock seen, speech lines played).
#[derive(Default)]
pub(super) struct OstUi {
    seen: [(u32, u32); 32],
    end: [Option<Instant>; 32],
    last: [Option<Instant>; 32],
    shown: [Option<i32>; 32],
    goods: Option<Goods>,
    speed: Option<(f32, Instant)>, // OST_GAME_SPEED: the speed shown, until when
    first_speed: Option<f32>,
    owner: Option<(String, String, [i32; 3])>, // OST_WhoOwns_Text, OST_WhoOwns_Building, (player, lord kind, team)
}

#[derive(Clone)]
struct Goods { expiry: Option<Instant>, calm: Instant, cart: Instant, stock: [i32; 25], said: [bool; 3], line: String }

const DDD: [f32; 4] = [0xdd as f32 / 255.0, 0xdd as f32 / 255.0, 0xdd as f32 / 255.0, 1.0];
const BG: [f32; 4] = [0.0, 0.0, 0.0, 0x55 as f32 / 255.0];
const GOLDENROD: [f32; 4] = [218.0 / 255.0, 165.0 / 255.0, 32.0 / 255.0, 1.0];

/// How long an add keeps a slot on (addOSTEntry's timedEnd); None = only while the engine keeps adding it.
fn timed(id: usize) -> Option<Duration> {
    match id {
        FEEDBACK_1 | 5 | PEACETIMER => Some(Duration::from_secs(5)),
        KEEP_MESSAGE | WHO_OWNS | 16 | 17 | 22 | 25 | WIN_TIMER | TIMETODEFEAT => Some(Duration::from_secs(1 << 30)),
        _ => None,
    }
}

impl Events {
    /// GameMap case 14: OST `r[3]`, data1 = r11 + r12·10⁴, data2 = r13 + r14·10⁴, data3 = r15 + r16·10⁴, data4 =
    /// r17 + r18·10⁴, data5 = r8 + r9·10⁴; who-owns with an owner outside 0..9 removes instead. Case 15 removes.
    pub(super) fn ost_record(&mut self, r: &[i16; 21], next: u32) {
        let d = |a: usize, b: usize| r[a] as i32 + r[b] as i32 * 10000;
        let id = r[3] as usize;
        let Some(o) = self.ost.get_mut(id) else { return };
        let data = [d(11, 12), d(13, 14), d(15, 16), d(17, 18), d(8, 9)];
        if r[0] == 15 || id == WHO_OWNS && !(0..10).contains(&data[0]) { o.removed = next } else { (o.data, o.added) = (data, next) }
    }
}

impl OstUi {
    /// OST_Keep_Message is up (the message bar moves up over it).
    pub(super) fn keep_message(&self) -> bool { self.shown[KEEP_MESSAGE].is_some() }

    /// updateOST's expiry pass: which slots are on now, and which turned on or changed data1 this frame.
    fn update(&mut self, ev: &Events) -> ([bool; 32], [bool; 32]) {
        let now = Instant::now();
        let (mut on, mut fresh) = ([false; 32], [false; 32]);
        for id in 0..32 {
            let o = ev.ost[id];
            if o.added != self.seen[id].0 {
                self.seen[id].0 = o.added;
                self.last[id] = Some(now);
                if let Some(t) = timed(id) { self.end[id] = Some(now + t); }
            }
            if o.removed != self.seen[id].1 {
                self.seen[id].1 = o.removed;
                if timed(id).is_some_and(|t| t.as_secs() > 5) { self.end[id] = None; } // removeOSTEntry: the open-ended ones
            }
            // ponytail: an untimed slot counts as on for 1 s after its last add (the engine re-adds it every tick)
            on[id] = self.end[id].is_some_and(|e| e > now) || timed(id).is_none() && self.last[id].is_some_and(|t| now - t < Duration::from_secs(1)) && o.added != 0;
            if !on[id] { self.shown[id] = None; continue; }
            fresh[id] = self.shown[id] != Some(o.data[0]);
            self.shown[id] = Some(o.data[0]);
        }
        (on, fresh)
    }
}

impl Ui {
    /// Design point (x, y) of the 1920×1080 grid anchored at fraction (ax, ay) of the window (0 left / top, 1 right /
    /// bottom, 0.5 centre).
    fn anchor(&self, x: f32, y: f32, ax: f32, ay: f32) -> [f32; 2] {
        [ax * self.w + (x - ax * 1920.0) * self.s, ay * self.h + (y - ay * 1080.0) * self.s]
    }

    /// A TextBlock with a background: `s` at design top-left (x, top) anchored like `anchor`, the box as wide as the text
    /// (at least `min_w`) and one line high, cream / grey text with the 2 px drop shadow.
    fn block(&mut self, out: &mut Vec<HudTri>, s: &str, x: f32, top: f32, a: [f32; 2], size: f32, min_w: f32, color: [f32; 4], bg: Option<[f32; 4]>) -> f32 {
        let m = self.font.horizontal_line_metrics(size);
        let (lh, asc) = m.map_or((size * 1.25, size), |m| (m.new_line_size, m.ascent));
        let tw = self.tw(s, size).max(min_w);
        let [x0, y0] = self.anchor(x, top, a[0], a[1]);
        if let Some(bg) = bg { Self::fill(out, [x0, y0, x0 + tw * self.s, y0 + lh * self.s], bg); }
        self.text_sh(out, s, x0, y0 + asc * self.s, size, 0.0, color);
        tw
    }

    /// Every on-screen text of this frame (main HUD visible or not: they sit in IngameUIScreens / MainHUD).
    pub(crate) fn on_screen_text(&mut self, st: &HudState, out: &mut Vec<HudTri>) {
        let (on, fresh) = self.ost.update(&st.ev);
        let t = |ui: &Self, sec: &str, i: i32| ui.text_of(&format!("{sec}_{:03}", i + 1)); // lookUpText is 0-based

        // OST_Game_Paused: 48 px, centred in the window (the score screen pauses without it)
        if st.game_paused != 0 && st.end.is_none() {
            let p = t(self, "TEXT_FEEDBACK", 22);
            let tw = self.tw(&p, 48.0);
            self.block(out, &p, 960.0 - tw / 2.0, 540.0 - 30.0, [0.5, 0.5], 48.0, 0.0, CREAM, None);
        }

        // OST_GameSpeed: " Game Speed: 45 " for 5 s after a change, 22 px, top right at margin 0,60,10 (90 with the
        // compass, Settings_Compass); goldenrod at the normal speed 40. Our fast-forward (infinite) shows nothing.
        if st.speed.is_finite() && self.ost.first_speed != Some(st.speed) {
            if self.ost.first_speed.is_some_and(|v| v != 0.0) { self.ost.speed = Some((st.speed, Instant::now() + Duration::from_secs(5))); }
            self.ost.first_speed = Some(st.speed);
        }
        if let Some((v, _)) = self.ost.speed.filter(|s| s.1 > Instant::now()) {
            let n = if v >= crate::speed::MAX { "Max".to_string() } else { (v as i32).to_string() }; // speeds past 90: speed.rs
            let txt = format!(" {} {n} ", t(self, "TEXT_FEEDBACK", 23));
            let right = if crate::skirmish::Settings::load().flag("Compass", false) { 90.0 } else { 10.0 };
            let tw = self.tw(&txt, 22.0);
            self.block(out, &txt, 1920.0 - right - tw, 60.0, [1.0, 0.0], 22.0, 0.0, if v as i32 == 40 { GOLDENROD } else { CREAM }, Some(BG));
        } else { self.ost.speed = None; }

        // OST_Keep_Message: 18 px, bottom right at margin 0,0,100,160; its speech on turning on / changing
        if on[KEEP_MESSAGE] {
            let m = st.ev.ost[KEEP_MESSAGE].data[0];
            if fresh[KEEP_MESSAGE] { crate::audio::speech(1, if m == 1 { "other_warning1.wav" } else { "other_warning2.wav" }); }
            let txt = format!("  {}  ", t(self, "TEXT_FEEDBACK", 10 + m - 1));
            let (tw, lh) = (self.tw(&txt, 18.0), self.font.horizontal_line_metrics(18.0).map_or(22.0, |m| m.new_line_size));
            self.block(out, &txt, 1920.0 - 100.0 - tw, 1080.0 - 160.0 - lh, [1.0, 1.0], 18.0, 0.0, CREAM, Some(BG));
        }
        // OST_Feedback_1: 22 px, bottom left at margin 10,0,0,160 (99 = TEXT_NEW_CTEXT 440)
        if on[FEEDBACK_1] {
            let m = st.ev.ost[FEEDBACK_1].data[0];
            let txt = format!("  {}  ", if m == 99 { t(self, "TEXT_NEW_CTEXT", 440) } else { t(self, "TEXT_FEEDBACK", m) });
            let lh = self.font.horizontal_line_metrics(22.0).map_or(27.0, |m| m.new_line_size);
            self.block(out, &txt, 10.0, 1080.0 - 160.0 - lh, [0.0, 1.0], 22.0, 0.0, CREAM, Some(BG));
        }
        self.who_owns(st, on[WHO_OWNS]);
        self.left_stack(st, out, &on, &fresh);
        if let Some((name, building, [p, kind, team])) = self.ost.owner.clone() {
            // MainHUD who-owns grid (bottom 180, two halves, 72 high): the face 72² right in the left half (margin 10:
            // OST_WhoOwns_FaceBackground, _Face, Face Border), the team's shield 23×29 at the right half's top-left
            // (-8, -6; OST_WhoOwns_Allies); name 24 px left in the right half, bottom 25 up; the building's name 14 px
            // #CCCCCC under it
            let [fx, fy] = self.anchor(960.0 - 10.0 - 72.0, 1080.0 - 180.0 - 72.0, 0.5, 1.0);
            let face = [fx, fy, fx + 72.0 * self.s, fy + 72.0 * self.s];
            self.lord_face(out, face, p.clamp(0, 8) as usize, if kind < 1 { -1 } else { kind }, true, 1.0);
            self.img(out, "Face Border", face, [1.0; 4]);
            if (1..=4).contains(&team) {
                let [sx, sy] = self.anchor(960.0 - 8.0, 1080.0 - 180.0 - 72.0 - 6.0, 0.5, 1.0);
                self.img(out, &format!("Team Allies Shield {team} Large"), [sx, sy, sx + 23.0 * self.s, sy + 29.0 * self.s], [1.0; 4]);
            }
            let lh = self.font.horizontal_line_metrics(24.0).map_or(30.0, |m| m.new_line_size);
            self.block(out, &name, 965.0, 1080.0 - 180.0 - 25.0 - lh, [0.5, 1.0], 24.0, 0.0, CREAM, Some(BG));
            if !building.is_empty() {
                let lh = self.font.horizontal_line_metrics(14.0).map_or(17.0, |m| m.new_line_size);
                self.block(out, &building, 965.0, 1080.0 - 185.0 - lh, [0.5, 1.0], 14.0, 0.0, [0.8, 0.8, 0.8, 1.0], Some([0.0, 0.0, 0.0, 0.2]));
            }
        }
    }

    /// OST_WHO_OWNS (the cursor over someone's building): data1 player, data2 lord kind (< 1 a human), data4 the
    /// name variant, data5 the building type; shown at once, gone when the engine removes it.
    fn who_owns(&mut self, st: &HudState, on: bool) {
        if !on { self.ost.owner = None; return; }
        let [p, kind, team, variant, building] = st.ev.ost[WHO_OWNS].data;
        let name = if kind < 1 {
            crate::skirmish::Settings::load().get("UserName").filter(|n| !n.is_empty()).map_or_else(|| format!("Player {p}"), str::to_string)
        } else { self.text_of(&crate::endgame::name(kind, variant)) };
        let b = self.titles.get(building.max(0) as usize).map(|k| self.text_of(k)).filter(|s| !s.is_empty()).map_or(String::new(), |s| format!("  {s}  "));
        self.ost.owner = Some((format!("  {name}  "), b, [p, kind, team]));
    }

    /// The top-left stack (OST_StartingGoods.xaml, 18 px rows 25 high): the cart (8 s across the screen), the timer
    /// box (win timer / time to defeat / peace time: 180×50 #DDDDDD centred text, a 160×12 black bar with the green
    /// rest, 158 px full), the starting-goods line and one row per good still in the keep's store (icon + amount). In
    /// the map editor the item counts take the corner instead (OST_PeopleLeft, 170 wide rows).
    fn left_stack(&mut self, st: &HudState, out: &mut Vec<HudTri>, on: &[bool; 32], fresh: &[bool; 32]) {
        let t = |ui: &Self, sec: &str, i: i32| ui.text_of(&format!("{sec}_{:03}", i + 1));
        let now = Instant::now();
        let s = self.s;
        // OST_PEOPLE_LEFT: people / buildings / trees / rocks / tribes, count/limit right-aligned
        if on[PEOPLE_LEFT] {
            let mut y = 0.0;
            for k in 0..5 {
                let label = t(self, "TEXT_NEW_TEXT", 16 + k as i32);
                self.block(out, &label, 0.0, y, [0.0, 0.0], 18.0, 170.0, CREAM, Some(BG));
                let v = format!("{}/{}", st.left[2 * k], st.left[2 * k + 1]);
                let tw = self.tw(&v, 18.0);
                self.block(out, &v, 170.0 - tw, y, [0.0, 0.0], 18.0, 0.0, CREAM, None);
                y += self.font.horizontal_line_metrics(18.0).map_or(22.0, |m| m.new_line_size);
            }
            return;
        }
        // starting goods: addOSTEntry(OST_STARTING_GOODS, message) each tick while the keep's store feeds out
        if !on[STARTING_GOODS] { self.ost.goods = None; }
        if on[STARTING_GOODS] {
            let msg = st.ev.ost[STARTING_GOODS].data[0];
            let startup = format!("  {}  ", t(self, "TEXT_STARTUP", msg));
            let hints: [String; 3] = std::array::from_fn(|i| format!("  {}  ", t(self, "TEXT_NEW_TEXT2", 132 + i as i32)));
            let g = self.ost.goods.get_or_insert_with(|| Goods { expiry: None, calm: now, cart: now, stock: [-1; 25], said: [false; 3], line: String::new() });
            if fresh[STARTING_GOODS] {
                (g.expiry, g.calm) = (Some(now + Duration::from_secs(20)), now + Duration::from_secs(10));
                g.line = startup;
            }
            if msg == 0 {
                if g.expiry.is_some_and(|e| e < now) { (g.expiry, g.line) = (None, String::new()); }
                if g.stock != st.keep_storage { g.calm = now + Duration::from_secs(10); }
                g.stock = st.keep_storage;
                if st.app_mode == 14 && matches!(st.app_sub_mode, 49 | 13 | 48) || st.game_paused != 0 { g.calm = now + Duration::from_secs(10); }
                if g.calm < now {
                    // the goods stuck in the keep for 10 s: where they need to go (stockpile / granary / armoury)
                    let k = &g.stock;
                    let any = |r: &[usize]| r.iter().any(|&i| k[i] > 0);
                    let flags = [any(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 16]), any(&[10, 11, 12, 13]), any(&[17, 18, 19, 20, 21, 22, 23, 24])];
                    let pick = flags.iter().position(|&f| f);
                    let speech = match pick {
                        Some(0) => "Placement_Warning11.wav",
                        Some(1) => if st.got_market & 4 > 0 { "Placement_Warning10.wav" } else { "Space_Warning1.wav" },
                        _ => if st.got_market & 8 > 0 { "Placement_Warning12.wav" } else { "Space_Warning3.wav" },
                    };
                    if let Some(i) = pick { if !g.said[i] { crate::audio::speech(1, speech); } }
                    for i in 0..3 { if !flags[i] { g.said[i] = false; } }
                    if let Some(i) = pick { g.said[i] = true; }
                    g.line = pick.map_or(String::new(), |i| hints[i].clone());
                } else { g.said = [false; 3]; }
            }
        }
        let timer = [TIMETODEFEAT, WIN_TIMER, PEACETIMER].into_iter().find(|&i| on[i]);
        if !on[STARTING_GOODS] && timer.is_none() { return; } // OST_LeftStack_Vis
        let mut y = 0.0;
        // the cart: UISprites B002..B007 every 200 ms, 93×39 (margin 0,2), from x −100 to the screen's width in 8 s
        if let Some(g) = &self.ost.goods {
            let f = (now - g.cart).as_secs_f32();
            if f < 8.0 {
                let x = -100.0 + 2020.0 * f / 8.0; // CartDistance = the UI's design width
                let frame = format!("UISprites B{:03}", 2 + (f / 0.2) as usize % 6);
                self.img_fit(out, &frame, [x * s, 2.0 * s, (x + 93.0) * s, 41.0 * s], [1.0; 4]);
            }
            y += 43.0;
        }
        if let Some(i) = timer {
            let [cur, max, ..] = st.ev.ost[i].data;
            let txt = match i { TIMETODEFEAT => t(self, "TEXT_OBJECTIVES", 17), WIN_TIMER => t(self, "TEXT_OBJECTIVES", 25), _ => t(self, "TEXT_NEW_TEXT2", 99) };
            Self::fill(out, [0.0, y * s, 180.0 * s, (y + 50.0) * s], BG);
            let tw = self.tw(&txt, 18.0);
            let asc = self.font.horizontal_line_metrics(18.0).map_or(18.0, |m| m.ascent);
            self.text_sh(out, &txt, (90.0 - tw / 2.0) * s, (y + asc) * s, 18.0, 0.0, DDD);
            let wbar = if max > 0 { ((max - cur) * 158 / max).min(158) as f32 } else { 0.0 };
            Self::fill(out, [10.0 * s, (y + 25.0) * s, 170.0 * s, (y + 37.0) * s], [0.0, 0.0, 0.0, 1.0]);
            Self::fill(out, [11.0 * s, (y + 26.0) * s, (11.0 + wbar.max(0.0)) * s, (y + 36.0) * s], [0x54 as f32 / 255.0, 0xb7 as f32 / 255.0, 0x27 as f32 / 255.0, 1.0]);
            y += 50.0;
        }
        let Some(g) = self.ost.goods.clone() else { return };
        if !g.line.is_empty() {
            self.block(out, &g.line, 0.0, y + 1.0, [0.0, 0.0], 18.0, 80.0, DDD, Some(BG));
            y += 25.0;
        }
        for k in 1..25 {
            let n = st.keep_storage[k];
            if n <= 0 { continue; }
            self.block(out, &format!("         {n}"), 0.0, y + 1.0, [0.0, 0.0], 18.0, 80.0, DDD, Some(BG));
            self.img_h(out, GOODS_ICONS[k - 1], 6.0 * s, y * s + 3.0 * s, 18.0 * s);
            y += 25.0;
        }
    }

    /// An atlas sprite `h` px high at its aspect, top-left (x, y) (Image Height=18, Stretch=Uniform).
    fn img_h(&self, out: &mut Vec<HudTri>, key: &str, x: f32, y: f32, h: f32) {
        let r = self.rects.get(key).copied().unwrap_or([0.0, 0.0, 1.0, 1.0]);
        self.img(out, key, [x, y, x + h * r[2] / r[3], y + h], [1.0; 4]);
    }
}

/// OST_StartingGoods.xaml's row icons for goods 1..24 (as the XAML has them).
const GOODS_ICONS: [&str; 24] = ["UISprites A001", "UISprites A001", "UISprites A002", "UISprites A003", "UISprites A001", "UISprites A004",
    "UISprites A005", "UISprites A005", "UISprites A006", "UISprites A007", "UISprites A008", "UISprites A009", "UISprites A010",
    "UISprites A011", "UISprites A012", "UISprites A013", "UISprites A014", "UISprites A015", "UISprites A016", "UISprites A017",
    "UISprites A018", "UISprites A019", "UISprites A020", "UISprites A021"];

/// Records 14 / 15 drive the slots: a timed entry stays after its last add, a removal ends an open-ended one, a
/// who-owns add with owner 10 is a removal.
#[test]
fn ost_slots() {
    let mut ev = Events::default();
    let rec = |k: [(usize, i16); 4]| { let mut r = [0i16; 21]; for (i, v) in k { r[i] = v } r };
    ev.ost_record(&rec([(0, 14), (3, 20), (11, 2), (12, 0)]), 1);
    ev.ost_record(&rec([(0, 14), (3, 4), (11, 7), (12, 1)]), 2);
    assert_eq!((ev.ost[20].data[0], ev.ost[4].data[0]), (2, 10007));
    let mut u = OstUi::default();
    let (on, fresh) = u.update(&ev);
    assert!(on[20] && fresh[20] && on[4] && !on[12]);
    assert!(!u.update(&ev).1[20], "unchanged: not fresh again");
    ev.ost_record(&rec([(0, 15), (3, 20), (0, 15), (0, 15)]), 3);
    ev.ost_record(&rec([(0, 14), (3, 21), (11, 10), (0, 14)]), 4);
    let (on, _) = u.update(&ev);
    assert!(!on[20] && on[4] && !on[21]);
}

/// The editor's item counts (OST_PEOPLE_LEFT) and a keep message draw while the engine posts them.
#[test]
fn ost_texts_drawn() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let mut st = HudState::default();
    let base = u.draw(&st, 1920.0, 1080.0, [0.0; 2], 0, None).len();
    st.ev.ost[PEOPLE_LEFT].added = 1;
    st.left = [12, 3000, 4, 4000, 0, 3400, 0, 4000, 0, 4500];
    let people = u.draw(&st, 1920.0, 1080.0, [0.0; 2], 0, None).len();
    assert!(people > base, "no item counts");
    st.ev.ost[KEEP_MESSAGE] = Ost { data: [1, 0, 0, 0, 0], added: 2, removed: 0 };
    assert!(u.draw(&st, 1920.0, 1080.0, [0.0; 2], 0, None).len() > people, "no keep message");
    assert!(u.ost.keep_message());
}
