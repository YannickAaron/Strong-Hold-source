//! In-game panels outside the bottom-right slot: the allies panel (HUD_AlliesPanel, skirmish / multiplayer), the
//! standings (HUD_MeritPanel), the
//! extreme powers bar (HUD_ExtremePowers), the Free Build events / invasions menu (HUD_FreebuildMenu) and the control
//! groups list over the troops panel (HUD_ControlGroups). Layout from their XAML in design px, the orders are the
//! C# ButtonClicked GameActions. A child module of hud.rs like reports.rs: it draws with Ui's helpers and state.
use super::*;

/// A panel's frame: screen top-left `o`, design px inside at Viewbox scale `s`.
#[derive(Clone, Copy)]
pub(super) struct Pane { o: [f32; 2], s: f32 }
impl Pane {
    fn r(&self, x: f32, y: f32, w: f32, h: f32) -> [f32; 4] { [self.o[0] + x * self.s, self.o[1] + y * self.s, self.o[0] + (x + w) * self.s, self.o[1] + (y + h) * self.s] }
    fn at(&self, x: f32, y: f32) -> [f32; 2] { [self.o[0] + x * self.s, self.o[1] + y * self.s] }
    fn sub(&self, x: f32, y: f32) -> Pane { Pane { o: self.at(x, y), s: self.s } }
}

/// HUD_AlliesPanel's state: view 0 main, 1 orders, 2 send goods, 3 request goods; the ally shown (curr_ally), the goods
/// picked and their amount (-1 = none yet), and whether Send opened on the ally's own request (req_goods_asked_for).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Allies { view: u8, cur: usize, goods: i32, amount: i32, asked: bool }

/// HUD_MeritPanel's state: sortBy (0 Greatest Lord, 1 gold, 2 troops) and sortReversed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Merit { sort: u8, rev: bool }

/// HUD_MeritPanel.populateList: the players top to bottom for DLL_GetMeritData rows `d`. Sort 0 by rank key then gold,
/// 1 by gold then troops, 2 by troops then gold, all descending, a full tie in player order; `rev` turns it round.
/// The original also ranks the empty slots (player -1) and draws them as blank rows between; they are left out here.
pub(super) fn ranking(d: &[[i32; 5]; 9], sort: u8, rev: bool) -> Vec<usize> {
    let (a, b) = match sort { 1 => (2, 3), 2 => (3, 2), _ => (1, 2) };
    let mut v: Vec<usize> = (1..9).filter(|&p| d[p][0] > 0).collect();
    v.sort_by_key(|&p| std::cmp::Reverse((d[p][a], d[p][b]))); // stable: ties stay in player order
    if rev { v.reverse(); }
    v
}

/// HUD_FreebuildMenu's state: the event picked and its slider value, the invading side, the troop kind the invasion
/// slider sets and every kind's count, the Invade Now lock-out (2 s after an invasion).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FreeBuild { event: usize, value: i32, enemy: usize, kind: usize, sizes: [i32; 32], lock: Option<std::time::Instant> }

/// GameData.freeBuildEventsOrder: the event ids of the ten event buttons (their labels are TEXT_SCENARIO_(id + 1)).
const FB_EVENTS: [i32; 10] = [133, 139, 143, 144, 145, 146, 148, 149, 150, 179];
/// The event buttons' bottom margins (left 80, 200 wide).
const FB_EVENT_Y: [f32; 10] = [472.0, 436.0, 400.0, 364.0, 328.0, 292.0, 256.0, 220.0, 184.0, 148.0];
/// GameData.scn_max_invasion_sizes by invasion slot.
const INV_MAX: [i32; 32] = [200, 100, 200, 100, 100, 100, 50, 100, 50, 10, 10, 10, 10, 10, 50, 10, 200, 200, 200, 50, 100, 100, 50, 10, 100, 20, 20,
    50, 100, 50, 200, 100];
/// GameData.getInvasionSizeTroopTypeFromIndex: the eChimps kind of each invasion slot.
const INV_TYPE: [i32; 32] = [22, 23, 24, 25, 26, 27, 28, 29, 30, 39, 40, 59, 58, 60, 37, 5, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85];
/// The invasion troop radio buttons (CommandParameter 100 + slot): slot, sprite, left, top.
const INV_BUTTONS: [(usize, &str, f32, f32); 32] = [(0, "UISprites Z001", 312.0, 83.0), (1, "UISprites Z002", 422.0, 83.0), (2, "UISprites Z003", 532.0, 83.0),
    (3, "UISprites Z004", 642.0, 83.0), (4, "UISprites Z005", 752.0, 83.0), (5, "UISprites Z006", 862.0, 83.0), (6, "UISprites Z007", 312.0, 133.0),
    (14, "UISprites Z011", 422.0, 133.0), (8, "UISprites Z009", 532.0, 133.0), (7, "UISprites Z008", 642.0, 133.0), (15, "UISprites Z010", 752.0, 133.0),
    (22, "UI-Buttons O049", 862.0, 133.0), (16, "UI-Buttons O031", 312.0, 183.0), (17, "UI-Buttons O034", 422.0, 183.0), (18, "UI-Buttons O037", 532.0, 183.0),
    (19, "UI-Buttons O040", 642.0, 183.0), (20, "UI-Buttons O043", 752.0, 183.0), (21, "UI-Buttons O046", 862.0, 183.0), (23, "UI-Buttons O052", 312.0, 233.0),
    (9, "UISprites Z012", 422.0, 233.0), (10, "UISprites Z013", 532.0, 233.0), (12, "UISprites Z014", 642.0, 233.0), (11, "UISprites Z015", 752.0, 233.0),
    (13, "UISprites Z016", 862.0, 233.0), (24, "UI-Buttons O058", 312.0, 283.0), (25, "UI-Buttons O061", 422.0, 283.0), (26, "UI-Buttons O064", 532.0, 283.0),
    (27, "UI-Buttons O067", 642.0, 283.0), (28, "UI-Buttons O070", 752.0, 283.0), (29, "UI-Buttons O073", 862.0, 283.0), (30, "UI-Buttons O076", 312.0, 333.0),
    (31, "UI-Buttons O079", 422.0, 333.0)];
/// HUD_FreebuildMenu.ButtonClicked 200..205: the preset invasions, by side and preset, as (slot, count).
const PRESETS: [[&[(usize, i32)]; 6]; 3] = [
    [&[(0, 10)], &[(0, 15), (2, 20)], &[(0, 25), (2, 20), (4, 10), (1, 5), (8, 5), (7, 10), (13, 5)],
     &[(0, 25), (2, 30), (4, 15), (1, 8), (3, 8), (5, 10), (14, 5), (8, 10), (7, 15), (9, 3), (12, 1), (11, 1), (13, 5)],
     &[(0, 25), (2, 30), (4, 20), (1, 12), (3, 15), (5, 12), (6, 5), (14, 5), (8, 15), (7, 20), (15, 3), (9, 3), (12, 2), (10, 1), (11, 1), (13, 10)],
     &[(0, 150), (2, 120), (4, 70), (1, 60), (3, 50), (5, 50), (6, 30), (14, 50), (8, 50), (7, 100), (15, 10), (9, 10), (12, 5), (10, 5), (11, 5), (13, 10)]],
    [&[(18, 10)], &[(18, 15), (17, 20)], &[(18, 25), (17, 20), (21, 6), (16, 5), (20, 5), (8, 5), (7, 10), (13, 5)],
     &[(18, 25), (17, 30), (21, 12), (16, 8), (20, 8), (22, 6), (23, 2), (8, 10), (7, 15), (9, 3), (12, 1), (11, 1), (13, 5)],
     &[(18, 25), (17, 30), (16, 12), (20, 12), (22, 12), (21, 16), (23, 3), (8, 15), (7, 20), (15, 3), (9, 3), (12, 2), (10, 1), (11, 1), (13, 10)],
     &[(18, 150), (17, 120), (21, 70), (16, 60), (20, 60), (22, 60), (23, 15), (8, 50), (7, 100), (15, 10), (9, 10), (12, 5), (10, 5), (11, 5), (13, 10)]],
    [&[(30, 5), (28, 5)], &[(30, 15), (28, 20)], &[(30, 25), (28, 20), (24, 10), (29, 5), (8, 5), (7, 10), (13, 5)],
     &[(30, 25), (28, 30), (24, 15), (29, 12), (31, 15), (8, 10), (7, 15), (9, 3), (12, 1), (11, 1), (13, 5)],
     &[(30, 25), (28, 30), (24, 20), (29, 12), (31, 20), (26, 5), (8, 15), (7, 20), (15, 3), (9, 3), (12, 2), (10, 1), (11, 1), (13, 10)],
     &[(30, 150), (28, 120), (24, 70), (29, 60), (31, 70), (26, 30), (8, 50), (7, 100), (15, 10), (9, 10), (12, 5), (10, 5), (11, 5), (13, 10)]],
];
/// The preset buttons: right / bottom margin (120 wide).
const PRESET_AT: [(f32, f32); 6] = [(406.0, 126.0), (281.0, 126.0), (156.0, 126.0), (31.0, 126.0), (281.0, 93.0), (156.0, 93.0)];
/// HUD_AlliesPanel's goods grid: good, left, top (Grid margin 10, -10 added when drawn).
const ALLY_GOODS: [(i32, f32, f32); 21] = [(2, 30.0, 60.0), (3, 94.0, 60.0), (4, 158.0, 60.0), (6, 222.0, 60.0), (7, 286.0, 60.0), (9, 350.0, 60.0),
    (10, 414.0, 60.0), (11, 478.0, 60.0), (12, 542.0, 60.0), (13, 606.0, 60.0), (14, 30.0, 120.0), (16, 94.0, 120.0), (17, 158.0, 120.0),
    (18, 222.0, 120.0), (19, 286.0, 120.0), (20, 350.0, 120.0), (21, 414.0, 120.0), (22, 478.0, 120.0), (23, 542.0, 120.0), (24, 606.0, 120.0),
    (15, 30.0, 170.0)];

/// The goods icon sets' index of good `g` (getAlliesGoodsIcon / getSmallGoodsIcon: 1 and 2 share one, 7 and 8 too).
fn goods_slot(g: i32) -> Option<u32> {
    Some(match g { 1 | 2 => 0, 3 => 1, 4 => 2, 6 => 3, 7 | 8 => 4, 9..=24 => g as u32 - 4, _ => return None })
}
/// getAlliesGoodsIcon(g): the request icon (GameSprites 419.. = UI-Buttons S003..).
fn ally_goods_icon(g: i32) -> Option<String> { goods_slot(g).map(|k| format!("UI-Buttons S{:03}", 3 + k)) }
/// getAIFace(kind): Face 01..27 (GameSprites 372..387, 577..585, 730, 731); the extended lords 30.. (custom AIV
/// lords) use the plain lord picture (738).
fn ai_face(kind: i16) -> &'static str {
    const F: [&str; 27] = ["Face 01", "Face 02", "Face 03", "Face 04", "Face 05", "Face 06", "Face 07", "Face 08", "Face 09", "Face 10", "Face 11",
        "Face 12", "Face 13", "Face 14", "Face 15", "Face 16", "Face 17", "Face 18", "Face 19", "Face 20", "Face 21", "Face 22", "Face 23", "Face 24",
        "Face 25", "Face 26", "Face 27"];
    F.get((kind - 1).max(0) as usize).filter(|_| kind >= 1).copied().unwrap_or("IMG_Lord_Crusader_Silver")
}
/// getAIFaceBackground(p): the lord colour's face background (GameSprites 388..395 by colour 1, 3, 4, 2, 6, 5, 7, 8).
/// ponytail: colour = shcRemapColours[p] as endgame::team_colour (no lobby colour picks).
fn face_bg(p: usize) -> &'static str {
    const BY_COLOUR: [&str; 9] = ["Face Background Generic", "Face Background 1", "Face Background 4", "Face Background 2", "Face Background 3",
        "Face Background 6", "Face Background 5", "Face Background 7", "Face Background 8"];
    const SHC: [usize; 9] = [0, 1, 3, 4, 2, 6, 5, 7, 8];
    BY_COLOUR[SHC[p.min(8)]]
}
/// HUD_ControlGroups.GetTroopSprite: a group's troop kind (control_groups_type) -> its UI-Buildings picture.
fn group_sprite(t: u8) -> Option<&'static str> {
    const S: [&str; 34] = ["O001", "O009", "O003", "O005", "O007", "O011", "O013", "O015", "O017", "O021", "O023", "O025", "M003", "O029", "O027",
        "O031", "M005", "O035", "O037", "O039", "O041", "O043", "O045", "O047", "O049", "O051", "O053", "O055", "O057", "O059", "O061", "O063", "O065", "O033"];
    S.get(t as usize).copied()
}

impl Ui {
    /// TextBlock with its box top at screen `at` (logical px), optionally with the HUD's drop shadow.
    fn top_text(&mut self, out: &mut Vec<HudTri>, s: &str, at: [f32; 2], size: f32, align: f32, color: [f32; 4], shadow: bool) {
        let asc = self.font.horizontal_line_metrics(size).map_or(size * 0.8, |m| m.ascent) * self.s;
        if shadow { self.text_sh(out, s, at[0], at[1] + asc, size, align, color); } else { self.text(out, s, at[0], at[1] + asc, size, 1.0, 0.0, align, color); }
    }

    /// BDR_SH1DE60 (`a` = 0xAA) / BDR_SH1DEe (0x88): a black fill at 0.89 inset 2, Panel_Border_* edges tiled (250×30)
    /// and Panel_Corner_2_* corners, 18 px outside the box.
    fn frame2(&self, out: &mut Vec<HudTri>, p: Pane, w: f32, h: f32, a: u8) {
        let s = p.s;
        Self::fill(out, p.r(2.0, 2.0, w - 4.0, h - 4.0), [0.0, 0.0, 0.0, a as f32 / 255.0 * 0.89]);
        self.img_tiled(out, "Panel_Border_Top", p.r(11.0, -18.0, w - 22.0, 30.0), 250.0 * s, 30.0 * s);
        self.img_tiled(out, "Panel_Border_Bottom", p.r(11.0, h - 12.0, w - 22.0, 30.0), 250.0 * s, 30.0 * s);
        self.img_tiled(out, "Panel_Border_Left", p.r(-18.0, 11.0, 30.0, h - 22.0), 30.0 * s, 250.0 * s);
        self.img_tiled(out, "Panel_Border_Right", p.r(w - 12.0, 11.0, 30.0, h - 22.0), 30.0 * s, 250.0 * s);
        for (k, x, y) in [("Top_Left", -18.0, -18.0), ("Top_Right", w - 12.0, -18.0), ("Bottom_Left", -18.0, h - 12.0), ("Bottom_Right", w - 12.0, h - 12.0)] {
            self.img(out, &format!("Panel_Corner_2_{k}"), p.r(x, y, 30.0, 30.0), [1.0; 4]);
        }
    }

    /// SDR_SH_Plain in a pane: the 2 px track across the 22 px box at (x, y), the thumb at `frac`; click / drag sets it.
    fn pane_slider(&mut self, out: &mut Vec<HudTri>, p: Pane, x: f32, y: f32, w: f32, frac: f32, hit: Hit) {
        Self::fill(out, p.r(x, y + 10.0, w, 2.0), BLACK);
        self.img(out, "Slider_Thumb_top", p.r(x + frac.clamp(0.0, 1.0) * (w - 20.0), y - 1.5, 20.0, 15.0), [1.0; 4]);
        self.hits.push((p.r(x, y - 4.0, w, 26.0), hit));
    }

    /// Everything here, drawn over the HUD (after the main panels, before the message bar and tooltip).
    pub(super) fn overlays(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2], troops: bool) {
        if troops && self.cg { self.control_groups(st, out, cursor); }
        // MainHUD's action point button (19×19, margin 0,0,220,130 bottom-right) while camera spots wait (FatControler
        // Show_ActionPoint): an ally's ping (Alt+P, MPPing), an event's place; a click jumps to the oldest
        // (ButtonActionPoint: GameAction ActionPointClicked 1035); the engine drops each after 20 s
        if st.action_points > 0 {
            let [x, y] = self.br(1920.0 - 220.0 - 19.0, 1080.0 - 130.0 - 19.0);
            self.button(out, [x, y, x + 19.0 * self.s, y + 19.0 * self.s], "UI-Buttons R003", "UI-Buttons R004", false, cursor, Some(Hit::Act(1035, 0, 0)));
        }
        self.extreme_bar(st, out, cursor);
        if st.lords.free_build {
            let [x0, y0] = self.bl(0.0, 1080.0 - 68.0); // the objectives button's place (IngameUIScreens extras, margin 0,0,0,34)
            self.button(out, [x0, y0, x0 + 36.0 * self.s, y0 + 34.0 * self.s], "Events_Normal", "Events_Highlight", self.fb.is_some(), cursor, Some(Hit::Fb(-100)));
        } else {
            self.fb = None;
        }
        if !st.lords.allies_button { self.allies = None; }
        if self.allies.is_some() { self.allies_panel(st, out, cursor); }
        if !st.lords.merit_button { self.merit = None; }
        if self.merit.is_some() { self.merit_panel(st, out, cursor); }
        if self.fb.is_some() { self.free_build(st, out, cursor); }
    }

    // ---- extreme powers

    /// HUD_ExtremePowers (IngameUIScreens: a Viewbox 400 high, top-right, margin 0,120 around the 240 px grid): the
    /// charge bar (ExtremePoints = min(236, extremeCount · 242 / 7000)), a tick and a button per power, power k usable
    /// from 636 · (k + 1) (GameData.cs:2254); a click is ExtremePower (1068, k). Only while extremeEnabled.
    /// ponytail: the 25 s "Click on an Icon to use Tactical Powers" hint (Settings_ShowExtremeHelp) is not shown.
    fn extreme_bar(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let (on, count) = (st.extreme.0, st.extreme.1 as i32);
        if on == 0 { return; }
        let k = self.s * 400.0 / 240.0;
        let (w, top) = (self.w, 120.0 * self.s);
        let r = |right: f32, y: f32, bw: f32, bh: f32| [w - (right + bw) * k, top + y * k, w - right * k, top + (y + bh) * k];
        let white = [1.0; 4];
        Self::fill(out, r(2.0, 0.0, 8.0, 240.0), white);
        Self::fill(out, r(3.0, 1.0, 6.0, 238.0), BLACK);
        let pts = (count * 242 / 7000).clamp(0, 236) as f32;
        Self::fill(out, r(4.0, 238.0 - pts, 4.0, pts), white);
        for i in 0..8 {
            Self::fill(out, r(10.0, 240.0 - 22.0 - 22.0 * i as f32 - 1.0, 5.0, 1.0), white);
            let p = r(15.0, 240.0 - 12.0 - 22.0 * i as f32 - 20.0, 20.0, 20.0);
            let n = i + 1;
            if count >= 636 * n as i32 {
                self.button(out, p, &format!("extreme power {n}a"), &format!("extreme power {n}c"), false, cursor, Some(Hit::Act(1068, i, 0)));
            } else {
                self.img(out, &format!("extreme power {n}b"), p, white);
                self.hits.push((p, Hit::Panel));
            }
            if Self::inside(p, cursor) { self.panel_tip = Some(self.text_of(&format!("TEXT_EXTREME_POWERS_{n:03}"))); }
        }
    }

    // ---- control groups

    /// HUD_ControlGroups (760×165, in HUD_Troops at margin -1000,-1000,80,150): groups 1..5 left, 6..9 and 0 right, a
    /// row each with Select (SelectClan), the number, up to four troop pictures with counts and "+N" for the rest,
    /// Create (GroupTroops), Add (GroupTroops 10) and Delete (GroupTroops 20). Select / Delete only for a used group.
    fn control_groups(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        if st.cg_total[0] < 0 { return; }
        let o = [1920.0 - 800.0, 1080.0 - 155.0];
        let [x0, y0] = self.br(o[0] - 40.0, o[1] - 160.0);
        let p = Pane { o: [x0, y0], s: self.s };
        let bg = p.r(0.0, 0.0, 760.0, 165.0);
        Self::fill(out, bg, [0.0, 0.0, 0.0, 0x77 as f32 / 255.0]);
        self.hits.push((bg, Hit::Panel));
        for row in 0..10 {
            let g = (row + 1) % 10; // rows 1, 2 .. 9, 0
            let r = p.sub(if row < 5 { 10.0 } else { 385.0 }, 7.5 + 30.0 * (row % 5) as f32);
            Self::fill(out, r.r(0.0, 0.0, 365.0, 30.0), [0.0, 0.0, 0.0, if row % 2 == 0 { 0x88 } else { 0xaa } as f32 / 255.0]);
            let total = st.cg_total[g];
            let used = total > 0;
            if used { self.button(out, r.r(0.0, 2.0, 26.0, 26.0), "UI-Buttons N017", "UI-Buttons N018", false, cursor, Some(Hit::Cg(0, g as i32))); }
            self.top_text(out, &g.to_string(), r.at(10.0, 5.0), 16.0, 0.0, if used { BLACK } else { CREAM }, false);
            if used {
                let mut shown = 0;
                for (j, &(t, n)) in st.cg[g].iter().enumerate().filter(|(_, e)| e.1 > 0) {
                    if let Some(s) = group_sprite(t) { self.img_fit(out, &format!("UI-Buildings {s}"), r.r(35.0 + 50.0 * j as f32, 1.0, 23.0, 26.0), [1.0; 4]); }
                    self.top_text(out, &n.to_string(), r.at(55.0 + 50.0 * j as f32, 5.0), 16.0, 0.0, CREAM, false);
                    shown += n as i32;
                }
                if shown != total as i32 { self.top_text(out, &format!("+{}", total as i32 - shown), r.at(240.0, 5.0), 16.0, 0.0, CREAM, false); }
            }
            self.button(out, r.r(278.0, 2.0, 26.0, 26.0), "UI-Buttons L007", "UI-Buttons L008", false, cursor, Some(Hit::Cg(1, g as i32)));
            self.button(out, r.r(308.0, 2.0, 26.0, 26.0), "UI-Buttons L026", "UI-Buttons L027", false, cursor, Some(Hit::Cg(2, g as i32)));
            if used { self.button(out, r.r(338.0, 2.0, 26.0, 26.0), "UI-Buttons L009", "UI-Buttons L010", false, cursor, Some(Hit::Cg(3, g as i32))); }
        }
    }

    /// Player `p`'s face (allies panel, the who-owns line): the lord colour's background when `bg`, then getAIFace(kind)
    /// (a custom lord: its own picture); `kind` -1 = a human: the generic face. ponytail: no Steam avatars.
    pub(super) fn lord_face(&mut self, out: &mut Vec<HudTri>, r: [f32; 4], p: usize, kind: i32, bg: bool, alpha: f32) {
        let c = [1.0, 1.0, 1.0, alpha];
        if bg { self.img(out, face_bg(p), r, c); }
        if let Some(cl) = crate::custom_lords::by_kind(kind) { if !crate::custom_lords::draw_face(out, &cl, r, c) { self.img_fit(out, ai_face(crate::custom_lords::parent(&cl) as i16 + 1), r, c); } }
        else if kind != -1 { self.img_fit(out, ai_face(kind as i16), r, c); }
        else { self.img(out, "Face Background Generic", r, c); self.img(out, "Face Border", r, c); }
    }

    /// HUD_ControlGroups.ButtonClicked: Select = SelectClanN (29 + N), Create / Add / Delete = GroupTroopsN (19 + N) with
    /// -1 / 10 / 20; 9 = the troops panel's toggle (ButtonTroopPanelToggleCG). The C# sends SelectClan8 for "9" and
    /// SelectClan9 for "0" (and drops Delete 0); here every row works its own group.
    pub(super) fn cg_click(&mut self, op: u8, g: i32) {
        match op {
            0 => self.actions.push([29 + g, -1, 0]),
            1..=3 => self.actions.push([19 + g, [-1, 10, 20][op as usize - 1], 0]),
            _ => self.cg = !self.cg,
        }
    }

    // ---- allies

    /// HUD_Main's ButtonAllies (553, bottom, 30×35; skirmish / multiplayer) with the Notification pip while an ally asks
    /// for goods or help or has an order (ShowAlliesPip); a click opens / closes the panel. `at` = its top-left.
    /// Hovered: its Tag ("Talk to allies") for the hover-text place (MouseEnterBuildingIconHandler, as the build tabs).
    pub(super) fn allies_tab(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2], at: [f32; 2]) -> Option<&'static str> {
        if !st.lords.allies_button { return None; }
        let p = [at[0], at[1], at[0] + 30.0 * self.s, at[1] + 35.0 * self.s];
        self.button(out, p, "UI-Buttons A053", "UI-Buttons A054", false, cursor, Some(Hit::Ally(15, 0)));
        let l = &st.lords;
        if l.side(true).iter().any(|&a| l.req[a][0] > 0 || l.help[a] != 0 || matches!(l.order[a], 1 | 2)) {
            let h = self.rects.get("Notification").map_or(15.0, |r| 15.0 * r[3] / r[2]);
            self.img(out, "Notification", [p[0], p[3] - h * self.s, p[0] + 15.0 * self.s, p[3]], [1.0; 4]);
        }
        Self::inside(p, cursor).then_some("TEXT_BUBBLE_HELP_TEXT_342")
    }

    /// HUD_AlliesPanel.ButtonClicked (`k` = which button, `v` its index / amount / good).
    pub(super) fn ally_click(&mut self, k: u8, v: i32) {
        let l = self.lords.clone(); // the last frame's (click has no HudState)
        let list = l.side(true);
        let Some(mut a) = self.allies else {
            if k == 15 { // HUD_AlliesPanel.Open: the main view (or "no allies"), Genie_11
                crate::audio::speech(3, "Genie_11.wav");
                self.merit = None;
                self.allies = Some(Allies { goods: -1, amount: -1, ..Default::default() });
            }
            return;
        };
        let ally = list.get(a.cur).copied().unwrap_or(0) as i32;
        let back = |a: &mut Allies| a.view = 0;
        match k {
            0 => if (v as usize) < list.len() { a.cur = v as usize; crate::lord_react::fire(list[a.cur], "PORTRAIT") },
            1 | 15 => { self.allies = None; return; } // Exit / the HUD button again
            2 => a.view = 1,
            3 | 4 => { // Send (on the ally's own request: that good and amount) / Request
                let [t, n] = l.req[ally as usize];
                let asked = k == 3 && t > 0;
                a = Allies { view: k - 1, goods: if asked { t as i32 } else { -1 }, amount: if asked { n as i32 } else { -1 }, asked, ..a };
            }
            5 => back(&mut a),
            6 | 7 => { self.actions4.push([1061, if k == 6 { 10 } else { 11 }, ally, 0]); back(&mut a); } // Clear orders / Defend me
            8 => self.actions.push([1065, ally, 0]), // Ally_ConfirmOrders
            9 => self.actions.push([1066, ally, 0]), // Ally_CancelOrders
            10 => if let Some(&e) = l.side(false).get(v as usize) { self.actions4.push([1061, v + 1, ally, e as i32]); back(&mut a); crate::lord_react::fire(e, "TARGET") }, // attack
            11 => a.amount = a.amount.max(0) + v,
            12 => a.goods = v,
            13 => { // ConfirmGoods: Ally_SendGoods / Ally_RequestGoods (ally, good, amount), then Exit
                self.actions4.push([if a.view == 2 { 1063 } else { 1064 }, ally, a.goods, a.amount]);
                self.allies = None;
                return;
            }
            14 => { // CancelGoods: on a request being answered, Ally_CancelGoodsReq and back
                if a.asked && a.view == 2 { self.actions.push([1062, ally, ally]); back(&mut a); }
                (a.goods, a.amount) = (-1, -1);
            }
            _ => {}
        }
        self.allies = Some(a);
    }

    /// HUD_AlliesPanel (700×320, top centre, margin 0,100): the allies' faces with their request / help / order icons,
    /// the chosen ally's box (name, goods asked for, orders, help, Confirm / Cancel orders) and Orders / Send / Request;
    /// the orders view (Clear, Defend me, attack an enemy) and the goods view (a good, an amount, Confirm / Cancel).
    fn allies_panel(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let Some(mut a) = self.allies else { return };
        let l = st.lords.clone();
        let (list, foes) = (l.side(true), l.side(false));
        if list.is_empty() { a.view = 0; }
        a.cur = a.cur.min(list.len().saturating_sub(1));
        self.allies = Some(a);
        let p = Pane { o: [self.w / 2.0 - 350.0 * self.s, 100.0 * self.s], s: self.s };
        let h = if a.view == 1 { 160.0 } else { 320.0 };
        self.hits.push((p.r(0.0, 0.0, 700.0, h), Hit::Panel));
        self.frame2(out, p, 700.0, h, 0xaa);
        let t = |ui: &Self, n: i32| ui.text_of(&format!("TEXT_ALLIES_{n:03}"));
        let title = t(self, match a.view { 1 => 6, 2 => 7, 3 => 8, _ => 1 });
        self.top_text(out, &title, p.at(20.0, 15.0), 22.0, 0.0, CREAM, true);
        let (x1, x2) = if a.view == 0 { ("UI-Buttons S043", "UI-Buttons S044") } else { ("UI-Buttons L005", "UI-Buttons L006") };
        self.button(out, p.r(661.0, 10.0, 29.0, 29.0), x1, x2, false, cursor, Some(Hit::Ally(if a.view == 0 { 1 } else { 5 }, 0)));
        let face = |ui: &mut Self, out: &mut Vec<HudTri>, r: [f32; 4], q: usize, bg: bool, alpha: f32| ui.lord_face(out, r, q, l.ai[q] as i32, bg, alpha);
        let name = |ui: &Self, q: usize| if let Some(c) = crate::custom_lords::by_kind(l.ai[q] as i32) { c.name } else if l.ai[q] != -1 { ui.text_of(&crate::endgame::name(l.ai[q] as i32, l.names[q] as i32)) } else { crate::endgame::name(0, 0) };
        let mut roll = String::new();
        match a.view {
            0 if list.is_empty() => { let s = t(self, 20); self.top_text(out, &s, p.at(20.0, 45.0), 22.0, 0.0, CREAM, true); }
            0 => {
                for (i, &q) in list.iter().enumerate() {
                    let x = 60.0 + 100.0 * i as f32;
                    let r = p.r(x, 60.0, 72.0, 72.0);
                    let cur = i == a.cur;
                    let alpha = if cur { 1.0 } else { 0.5 };
                    face(self, out, r, q, cur, alpha);
                    self.hits.push((r, Hit::Ally(0, i as i32)));
                    if let Some(k) = ally_goods_icon(l.req[q][0] as i32).filter(|_| l.req[q][0] > 0) { self.img_fit(out, &k, p.r(x + 60.0, 48.0, 24.0, 25.0), [1.0, 1.0, 1.0, alpha]); }
                    if l.help[q] != 0 { self.img_fit(out, "UI-Buttons S002", p.r(x + 60.0, 120.0, 24.0, 25.0), [1.0, 1.0, 1.0, alpha]); }
                    if let Some(k) = match l.order[q] { 1 => Some("UI-Buttons S024"), 2 => Some("UI-Buttons S025"), _ => None } {
                        self.img_fit(out, k, p.r(x - 8.0, 120.0, 24.0, 25.0), [1.0, 1.0, 1.0, alpha]);
                    }
                }
                // the chosen ally's box (660×120 at 20, 180, BDR_SH1DEe)
                let b = p.sub(20.0, 180.0);
                self.frame2(out, b, 660.0, 120.0, 0x88);
                let q = list[a.cur];
                let c = crate::endgame::rgb(l.colours[q]).map(|v| v + (1.0 - v) * 0.3); // GameMap.Lighten(colour, 0.3)
                let n = name(self, q);
                self.top_text(out, &n, b.at(15.0, 10.0), 20.0, 0.0, [c[0], c[1], c[2], 1.0], true);
                let [rt, rn] = l.req[q];
                let goods = if rt <= 0 { t(self, 2) } else {
                    if let Some(k) = ally_goods_icon(rt as i32) { self.img_fit(out, &k, b.r(15.0, 37.0, 24.0, 25.0), [1.0; 4]); }
                    format!("       {} {rn} {}", t(self, 13), self.text_of(&format!("TEXT_GOODS_{:03}", rt + 1)))
                };
                self.top_text(out, &goods, b.at(15.0, 40.0), 18.0, 0.0, CREAM, true);
                let order = match l.order[q] {
                    0 => t(self, 3),
                    2 => format!("{} {}", t(self, 17), t(self, 18)),
                    _ => {
                        let on = l.order_player[q].clamp(0, 8) as usize;
                        face(self, out, b.r(15.0, 62.0, 24.0, 24.0), on, true, 1.0);
                        format!("       {} {}", t(self, 17), t(self, 11))
                    }
                };
                self.top_text(out, &order, b.at(15.0, 65.0), 18.0, 0.0, CREAM, true);
                let me = l.me.clamp(0, 8) as usize;
                let asked_me = l.order[me] == 1 && l.order_from[me] as usize == q;
                let help = if asked_me {
                    face(self, out, b.r(15.0, 87.0, 24.0, 24.0), l.order_player[me].clamp(0, 8) as usize, true, 1.0);
                    format!("       {}", t(self, 11))
                } else if l.help[q] != 0 { t(self, 5) } else { String::new() };
                self.top_text(out, &help, b.at(15.0, 90.0), 18.0, 0.0, CREAM, true);
                if asked_me || l.help[q] > 0 { // Allies_OrdersConfirmVis
                    self.button(out, b.r(240.0, 80.0, 35.0, 35.0), "UI-Buttons S037", "UI-Buttons S039", false, cursor, Some(Hit::Ally(8, 0)));
                    self.button(out, b.r(280.0, 80.0, 35.0, 35.0), "UI-Buttons S038", "UI-Buttons S040", false, cursor, Some(Hit::Ally(9, 0)));
                }
                for (k, s1, s2, x, w, bh, tip) in [(2, "S024", "S025", 330.0, 49.0, 69.0, 6), (3, "S026", "S027", 420.0, 79.0, 56.0, 7), (4, "S029", "S030", 530.0, 79.0, 58.0, 8)] {
                    let r = b.r(x, 20.0 + (100.0 - bh) / 2.0, w, bh);
                    self.button(out, r, &format!("UI-Buttons {s1}"), &format!("UI-Buttons {s2}"), false, cursor, Some(Hit::Ally(k, 0)));
                    if Self::inside(r, cursor) { roll = t(self, tip); }
                }
                self.top_text(out, &roll, b.at(455.0, 10.0), 20.0, 0.5, CREAM, true);
            }
            1 => {
                for (k, s1, s2, x, w, bh, tip) in [(6, "S031", "S032", 30.0, 50.0, 69.0, 9), (7, "S033", "S034", 100.0, 50.0, 61.0, 10)] {
                    let r = p.r(x, 10.0 + (150.0 - bh) / 2.0, w, bh);
                    self.button(out, r, &format!("UI-Buttons {s1}"), &format!("UI-Buttons {s2}"), false, cursor, Some(Hit::Ally(k, 0)));
                    if Self::inside(r, cursor) { roll = t(self, tip); }
                }
                for (i, &e) in foes.iter().enumerate() {
                    let r = p.r(170.0 + 80.0 * i as f32, 49.0, 72.0, 72.0);
                    face(self, out, r, e, true, 1.0);
                    self.hits.push((r, Hit::Ally(10, i as i32)));
                    if Self::inside(r, cursor) { roll = format!("{} {}", t(self, 11), name(self, e)); }
                }
                let asc = self.font.horizontal_line_metrics(20.0).map_or(16.0, |m| m.new_line_size);
                self.top_text(out, &roll, p.at(20.0, 160.0 - 10.0 - asc), 20.0, 0.0, CREAM, true);
            }
            _ => {
                for &(g, x, y) in &ALLY_GOODS {
                    let k = goods_slot(g).unwrap_or(0);
                    let r = p.r(x + 10.0, y - 10.0, 35.0, 35.0);
                    let lit = a.goods == g;
                    self.button(out, r, &format!("UISprites E{:03}", 2 * k + 1), &format!("UISprites E{:03}", 2 * k + 2), lit, cursor, Some(Hit::Ally(12, g)));
                    self.top_text(out, &st.resources[g as usize].to_string(), p.at(x + 10.0 + 17.5, y - 10.0 + 35.0), 12.0, 0.5, CREAM, true);
                }
                for (i, n) in [5, 10, 25, 100, 500].into_iter().enumerate() {
                    let r = p.r(130.0 + 100.0 * i as f32, 170.0, 35.0, 35.0);
                    self.button(out, r, "UI-Buttons S035", "UI-Buttons S036", false, cursor, Some(Hit::Ally(11, n)));
                    let size = if n < 100 { 20.0 } else { 19.0 };
                    self.top_text(out, &n.to_string(), p.at(130.0 + 100.0 * i as f32 + 17.5, 176.0 + (20.0 - size)), size, 0.5, [1.0; 4], false);
                }
                // the info box (660×80 at 20, 220): the good, "amount good" / what to choose, Confirm / Cancel
                let b = p.sub(20.0, 220.0);
                self.frame2(out, b, 660.0, 80.0, 0x88);
                let info = if a.goods < 0 { t(self, 14) } else {
                    if let Some(k) = goods_slot(a.goods) { self.img_fit(out, &format!("UISprites E{:03}", 2 * k + 1), b.r(30.0, 22.5, 35.0, 35.0), [1.0; 4]); }
                    if a.amount < 0 { t(self, 15) } else { format!("{} {}", a.amount, self.text_of(&format!("TEXT_GOODS_{:03}", a.goods + 1))) }
                };
                let lh = self.font.horizontal_line_metrics(22.0).map_or(27.0, |m| m.new_line_size);
                self.top_text(out, &info, b.at(75.0, (80.0 - lh) / 2.0), 22.0, 0.0, CREAM, true);
                let ok = a.goods >= 0 && a.amount > 0 && (a.view == 3 || st.resources.get(a.goods as usize).is_some_and(|&n| n >= a.amount));
                if ok { self.button(out, b.r(535.0, 22.5, 35.0, 35.0), "UI-Buttons S037", "UI-Buttons S039", false, cursor, Some(Hit::Ally(13, 0))); }
                self.button(out, b.r(585.0, 22.5, 35.0, 35.0), "UI-Buttons S038", "UI-Buttons S040", false, cursor, Some(Hit::Ally(14, 0)));
            }
        }
    }

    // ---- standings

    /// HUD_Main's ButtonMerit (588, bottom, 30×35; skirmish / multiplayer, "Order of Merit"): opens / closes the panel.
    pub(super) fn merit_tab(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2], at: [f32; 2]) -> Option<&'static str> {
        if !st.lords.merit_button { return None; }
        let p = [at[0], at[1], at[0] + 30.0 * self.s, at[1] + 35.0 * self.s];
        self.button(out, p, "UI-Buttons A056", "UI-Buttons A057", self.merit.is_some(), cursor, Some(Hit::Merit(9, 0)));
        Self::inside(p, cursor).then_some("TEXT_BUBBLE_HELP_TEXT_343") // its Tag, hover-text place (as allies_tab)
    }

    /// HUD_MeritPanel.ButtonClicked: `k` 0..2 = the Lord / Gold / Troops header (the same one again reverses), 3 = a
    /// row's keep (HomeKeep for that player), 8 = Exit, 9 = the HUD button (Open: sort 0, closes the allies panel,
    /// GenieDE121). ponytail: the rank comment 10 s after opening (GenieDE129..144) is not played.
    pub(super) fn merit_click(&mut self, k: u8, v: i32) {
        match (k, self.merit) {
            (9, None) => {
                crate::audio::speech(3, "GenieDE121.wav");
                self.allies = None;
                self.merit = Some(Merit::default());
            }
            (8 | 9, _) => self.merit = None,
            (3, _) => self.actions.push([6, v, 0]), // EngineInterface.GameAction(KeyFunctions.HomeKeep, player)
            (0..=2, Some(m)) => self.merit = Some(Merit { sort: k, rev: m.sort == k && !m.rev }),
            _ => {}
        }
    }

    /// HUD_MeritPanel (640×370, top centre, margin 0,100; BDR_SH1DE60): the sort's title ("Greatest Lord", "Sorted by
    /// gold / troops"), Exit, the Lord / Gold / Troops headers over a BDR_SH1DEe box, and a 20 px row per player:
    /// team shield (a fallen lord's S054), name in the lord's colour lightened 0.3, gold and troops right-aligned, and
    /// the colour keep button that centres on that keep while the lord stands.
    fn merit_panel(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let Some(m) = self.merit else { return };
        let l = &st.lords;
        let p = Pane { o: [self.w / 2.0 - 320.0 * self.s, 100.0 * self.s], s: self.s };
        self.hits.push((p.r(0.0, 0.0, 640.0, 370.0), Hit::Panel));
        self.frame2(out, p, 640.0, 370.0, 0xaa);
        self.frame2(out, p.sub(25.0, 75.0), 590.0, 270.0, 0x88);
        let title = self.text_of(&format!("TEXT_MP_RANK_{:03}", m.sort + 2)); // lookUpText(TEXT_MP_RANK, 1 + sort)
        self.top_text(out, &title, p.at(40.0, 20.0), 28.0, 0.0, CREAM, true);
        self.button(out, p.r(591.0, 20.0, 29.0, 29.0), "UI-Buttons S043", "UI-Buttons S044", false, cursor, Some(Hit::Merit(8, 0)));
        // the headers: BTN_RADIO_Image (Sprite1 normal, 2 over, 3 checked)
        for (k, s0, x, w) in [(0u8, 45, 40.0, 243.0), (1, 48, 370.0, 45.0), (2, 51, 500.0, 45.0)] {
            let r = p.r(x, 85.0, w, 70.0);
            let s = if m.sort == k { s0 + 2 } else if Self::inside(r, cursor) { s0 + 1 } else { s0 };
            self.img_fit(out, &format!("UI-Buttons S{s:03}"), r, [1.0; 4]);
            self.hits.push((r, Hit::Merit(k, 0)));
        }
        for (i, q) in ranking(&l.merit, m.sort, m.rev).into_iter().enumerate() {
            let (row, y) = (l.merit[q], 165.0 + 20.0 * i as f32);
            let c = crate::endgame::colour(&l.colours, q as u8);
            let shield = if row[1] < 0 { Some("UI-Buttons S054".to_string()) } else { (1..=4).contains(&row[4]).then(|| format!("Team Allies Shield {} Small", row[4])) };
            if let Some(k) = shield { self.img_fit(out, &k, p.r(40.0, y, 20.0, 20.0), [1.0; 4]); }
            let lit = crate::endgame::rgb(c).map(|v| v + (1.0 - v) * 0.3); // GameMap.Lighten(colour, 0.3)
            let name = self.text_of(&l.who[q]);
            self.top_text(out, &name, p.at(60.0, y), 18.0, 0.0, lit, true);
            self.top_text(out, &row[2].to_string(), p.at(410.0, y), 18.0, 1.0, CREAM, true);
            self.top_text(out, &row[3].to_string(), p.at(540.0, y), 18.0, 1.0, CREAM, true);
            if row[1] >= 0 { // MeritKeepVis: getTeamKeepImage / getTeamKeepOverImage by the player's colour
                const KEEP: [&str; 9] = ["Red", "Red", "Blue", "Orange", "Yellow", "Purple", "Black", "Light Blue", "Green"];
                let k = KEEP[c.min(8) as usize];
                self.button(out, p.r(578.0, y, 22.0, 22.0), &format!("Keep Up {k}"), &format!("Keep Over {k}"), false, cursor, Some(Hit::Merit(3, q as i32)));
            }
        }
    }

    /// KeyManager's Escape: an open allies or standings panel closes first (true), before the in-game menu.
    pub(crate) fn escape_panel(&mut self) -> bool { self.allies.take().is_some() | self.merit.take().is_some() }

    // ---- Free Build

    /// HUD_FreebuildMenu.ButtonClicked(param), -100 = the extras button (ToggleMenu, which opens it on Init's defaults).
    pub(super) fn fb_click(&mut self, v: i32) {
        if v == -100 {
            self.fb = if self.fb.is_some() { None } else { Some(FreeBuild { event: 0, value: 1, enemy: 0, kind: 0, sizes: [0; 32], lock: None }) };
            return;
        }
        let Some(f) = self.fb.as_mut() else { return };
        match v {
            -10 => self.fb = None,
            -1 => { self.actions.push([1031, FB_EVENTS[f.event], f.value]); self.fb = None; } // FreeBuild_Event (id, size)
            -2 => { // FreeBuild_InvasionCharSet (side), _InvasionCount (slot, n) each, _InvasionStart; Invade Now locked 2 s
                if !fb_can_invade(f, &self.lords) { return; }
                self.actions.push([1032, 0, f.enemy as i32]);
                for (k, &n) in f.sizes.iter().enumerate().filter(|e| *e.1 > 0) { self.actions.push([1033, k as i32, n]); }
                self.actions.push([1034, 0, 0]);
                f.lock = Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
            }
            -3 => { self.actions.push([1044, 4, 0]); crate::audio::ui(326, 1.0); } // SH1Cheats 4: +500 gold
            1..=10 => if f.event != v as usize - 1 { (f.event, f.value) = (v as usize - 1, 1) },
            20..=22 => f.enemy = v as usize - 20,
            100..=131 => f.kind = v as usize - 100,
            200..=205 => {
                f.sizes = [0; 32];
                for &(k, n) in PRESETS[f.enemy][v as usize - 200] { f.sizes[k] = n; }
            }
            _ => {}
        }
    }

    /// The Free Build sliders: 4 = the event's size (1..FreebuildSizeMax), 5 = the picked troop kind's invasion count.
    pub(super) fn fb_slider(&mut self, k: u8, frac: f32) {
        let Some(f) = self.fb.as_mut() else { return };
        if k == 4 { f.value = 1 + (frac * (fb_event_max(f.event) - 1) as f32).round() as i32; } else { f.sizes[f.kind] = (frac * INV_MAX[f.kind] as f32).round() as i32; }
    }

    /// HUD_FreebuildMenu (1000×600, BDR_SH1DEa, centred over the map rows): the ten events (with a size slider for the
    /// ones that have one) and Start; the invading side, the troop kinds with their counts, the count slider, the
    /// presets and Invade Now; +500 gold, Exit, and the missing-signpost warning.
    fn free_build(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let Some(f) = self.fb.clone() else { return };
        let s = self.s;
        let (w, h) = (1000.0, 600.0);
        let p = Pane { o: [self.w / 2.0 - w / 2.0 * s, (self.h * 19.0 / 46.0 - h / 2.0 * s).max(0.0)], s }; // rows 0..2 of IngameUIScreens (19 of 23)
        self.hits.push((p.r(0.0, 0.0, w, h), Hit::Panel));
        self.frame_a(out, p, w, h, "TEXT_SCENARIO_024");
        let ping = st.lords.ping;
        let label = |ui: &mut Self, out: &mut Vec<HudTri>, key: &str, x: f32, y: f32, bw: f32, size: f32| {
            let t = ui.text_of(key);
            ui.top_text(out, &t, p.at(x + bw / 2.0, y), size, 0.5, BLACK, false);
        };
        label(self, out, "TEXT_MAPEDIT_030", 762.0, 67.0, 170.0, 22.0);
        for (k, key) in ["TEXT_SCENARIO_224", "TEXT_SCENARIO_225", "TEXT_NEW_CTEXT_221"].iter().enumerate() {
            let t = self.text_of(key);
            self.glow(out, p.at(312.0 + 140.0 * k as f32, 70.0), 130.0, &t, f.enemy == k, cursor, Some(Hit::Fb(20 + k as i32)));
        }
        for &(k, spr, x, y) in &INV_BUTTONS {
            let n = if f.sizes[k] > 0 { f.sizes[k].to_string() } else { String::new() };
            // RBTN_SH_GlowSImage: the 55 px picture sets the button's height, the 31 px glow bar is centred in it
            self.glow(out, p.at(x, y + 10.0 + 13.5), 100.0, "", f.kind == k, cursor, Some(Hit::Fb(100 + k as i32)));
            self.img_fit(out, spr, p.r(x - 5.0, y + 10.0, 55.0, 55.0), [1.0; 4]);
            self.top_text(out, &n, p.at(x + 88.0, y + 10.0 + 18.0), 16.0, 1.0, [0xf4 as f32 / 255.0, 0xde as f32 / 255.0, 0xaa as f32 / 255.0, 1.0], false);
        }
        // the invasion count slider (StackPanel 408 wide centred right of 498, bottom 195): "N Troop" above it
        let max = INV_MAX[f.kind];
        let it = format!("{} {}", f.sizes[f.kind], self.text_of(&format!("TEXT_CHIMP_NAMES_{:03}", INV_TYPE[f.kind] + 1)));
        self.top_text(out, &it, p.at(749.0, 350.0), 18.0, 0.5, BLACK, false);
        self.pane_slider(out, p, 545.0, 383.0, 408.0, f.sizes[f.kind] as f32 / max as f32, Hit::Slider(5, 0));
        label(self, out, "TEXT_NEW_TEXT2_130", 492.0, 415.0, 460.0, 20.0);
        for (i, &(r, b)) in PRESET_AT.iter().enumerate() {
            let t = self.text_of(&format!("TEXT_NEW_TEXT2_{:03}", 113 + i));
            self.glow(out, p.at(w - r - 120.0, h - b - 28.0), 120.0, &t, false, cursor, Some(Hit::Fb(200 + i as i32)));
        }
        let t = self.text_of("TEXT_SCENARIO_213");
        let inv = fb_can_invade(&f, &st.lords);
        self.glow(out, p.at(720.0, h - 40.0 - 28.0), 200.0, &t, false, cursor, inv.then_some(Hit::Fb(-2)));
        // the events: rabbits (3) need keep spots, wolves (4) a marker on land, bandits / archers (5, 6) siege camps
        for (e, &b) in FB_EVENT_Y.iter().enumerate() {
            let on = match e { 3 => ping[2] > 0, 4 => ping[1] > 0, 5 | 6 => ping[0] > 0, _ => true };
            let t = self.text_of(&format!("TEXT_SCENARIO_{:03}", FB_EVENTS[e] + 1));
            self.glow(out, p.at(80.0, h - b - 28.0), 200.0, &t, f.event == e, cursor, on.then_some(Hit::Fb(e as i32 + 1)));
        }
        if fb_event_max(f.event) > 1 { // EventSliderAdj (left 80, bottom 87)
            self.top_text(out, &f.value.to_string(), p.at(180.0, 463.0), 18.0, 0.5, BLACK, false);
            self.pane_slider(out, p, 80.0, 491.0, 200.0, (f.value - 1) as f32 / (fb_event_max(f.event) - 1) as f32, Hit::Slider(4, 0));
        }
        let t = self.text_of("TEXT_NEW_TEXT_057");
        self.glow(out, p.at(80.0, h - 40.0 - 28.0), 200.0, &t, false, cursor, Some(Hit::Fb(-1)));
        self.glow(out, p.at(310.0, 490.0), 150.0, "+500             ", false, cursor, Some(Hit::Fb(-3)));
        self.img_fit(out, "UISprites A012", p.r(380.0, 490.0, 50.0, 34.0), [1.0; 4]);
        let t = self.text_of("TEXT_HELP_003");
        self.glow(out, p.at(400.0, h - 37.0 - 28.0), 200.0, &t, false, cursor, Some(Hit::Fb(-10)));
        if ping[0] == 0 {
            let t = self.text_of("TEXT_NEW_TEXT2_082");
            self.top_text(out, &t, p.at(500.0, h - 10.0 - 24.0), 19.0, 0.5, BLACK, false);
        }
    }

    /// BDR_SH1DEa with a WGT_Heading (HUD_ConfirmationPopup, HUD_FreebuildMenu): Panel_BG tiled 256, the borders and
    /// corners, the Celtic knots, the 30 px title centred over Panel_Divider.
    pub(super) fn frame_a(&mut self, out: &mut Vec<HudTri>, p: Pane, w: f32, h: f32, title: &str) {
        let s = p.s;
        self.img_tiled(out, "Panel_BG", p.r(2.0, 2.0, w - 4.0, h - 4.0), 256.0 * s, 256.0 * s);
        self.img_tiled(out, "Panel_Border_Top", p.r(-1.0, -18.0, w + 2.0, 30.0), 250.0 * s, 30.0 * s);
        self.img_tiled(out, "Panel_Border_Bottom", p.r(-1.0, h - 12.0, w + 2.0, 30.0), 250.0 * s, 30.0 * s);
        self.img_tiled(out, "Panel_Border_Left", p.r(-18.0, -1.0, 30.0, h + 2.0), 30.0 * s, 250.0 * s);
        self.img_tiled(out, "Panel_Border_Right", p.r(w - 12.0, -1.0, 30.0, h + 2.0), 30.0 * s, 250.0 * s);
        for (k, x, y) in [("Top_Left", -18.0, -18.0), ("Top_Right", w - 12.0, -18.0), ("Bottom_Left", -18.0, h - 12.0), ("Bottom_Right", w - 12.0, h - 12.0)] {
            self.img(out, &format!("Panel_Corner_{k}"), p.r(x, y, 30.0, 30.0), [1.0; 4]);
        }
        for kx in [25.0, w - 45.0] { self.img(out, "Panel_CelticKnot", p.r(kx, 11.0, 20.0, 40.0), [1.0; 4]); }
        self.img_tiled(out, "Panel_Divider", p.r(3.0, 56.0, w - 6.0, 8.0), 250.0 * s, 7.5 * s);
        let t = self.text_of(title);
        let [tx, ty] = p.at(w / 2.0, 34.0 + 30.0 * 0.33);
        self.text(out, &t, tx, ty, 30.0, 1.0, 0.0, 0.5, BLACK);
    }
}

/// HUD_FreebuildMenu.UpdateEventButtons: FreebuildSizeMax of event slot `e` (1 = no slider).
fn fb_event_max(e: usize) -> i32 { match e { 1 | 4 | 9 => 10, 5 | 6 => 50, _ => 1 } }

/// UpdateInvadeButton: troops picked, siege camps on the map, not just invaded.
fn fb_can_invade(f: &FreeBuild, l: &Lords) -> bool {
    f.sizes.iter().sum::<i32>() > 0 && l.ping[0] > 0 && f.lock.is_none_or(|t| std::time::Instant::now() >= t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui() -> Option<Ui> { Ui::load(&crate::cache_dir().join("ui")) }
    /// Draw `st`, click the centre of the topmost `want` hit, return what the click queued.
    fn press(u: &mut Ui, st: &HudState, want: Hit) -> (Vec<[i32; 3]>, Vec<[i32; 4]>) {
        u.draw(st, 1684.0, 784.0, [0.0; 2], 0, None);
        u.actions.clear();
        u.actions4.clear();
        let p = u.hits.iter().rev().find(|(_, h)| *h == want).map(|e| e.0).unwrap_or_else(|| panic!("{want:?} not drawn"));
        assert!(u.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]));
        u.drag = None; // released
        (std::mem::take(&mut u.actions), std::mem::take(&mut u.actions4))
    }
    /// A 2-vs-1 skirmish: me (1) and AI 3 (kind 5) on team 1, AI 2 on team 2.
    fn skirmish() -> HudState {
        let mut l = Lords { me: 1, allies_button: true, ai: [0, -1, 2, 5, -1, -1, -1, -1, -1], human: [0, 0, -1, -1, -1, -1, -1, -1, -1],
            team: [0, 1, 2, 1, -1, -1, -1, -1, -1], ..Default::default() };
        l.in_game[1..4].fill(true);
        l.req[3] = [2, 50]; // wood
        HudState { lords: l, ..Default::default() }
    }

    #[test]
    fn ally_lists() {
        let st = skirmish();
        assert_eq!((st.lords.side(true), st.lords.side(false)), (vec![3], vec![2]));
        let mut l = st.lords.clone();
        l.in_game[1] = false;
        assert!(l.side(true).is_empty(), "my lord gone: no allies");
    }

    /// The allies panel: open, attack the enemy (Ally_Orders 1, ally 3, enemy 2), send the wood the ally asked for.
    #[test]
    fn allies_orders_and_goods() {
        let Some(mut u) = ui() else { return };
        let mut st = skirmish();
        press(&mut u, &st, Hit::Ally(15, 0));
        assert!(u.allies.is_some());
        press(&mut u, &st, Hit::Ally(2, 0));
        assert_eq!(press(&mut u, &st, Hit::Ally(10, 0)).1, vec![[1061, 1, 3, 2]]);
        assert_eq!(u.allies.unwrap().view, 0, "back to the main view");
        press(&mut u, &st, Hit::Ally(3, 0)); // Send: the request preselected
        assert_eq!((u.allies.unwrap().goods, u.allies.unwrap().amount), (2, 50));
        st.resources[2] = 40;
        u.draw(&st, 1684.0, 784.0, [0.0; 2], 0, None);
        assert!(!u.hits.iter().any(|(_, h)| *h == Hit::Ally(13, 0)), "40 wood < 50: no Confirm");
        st.resources[2] = 80;
        assert_eq!(press(&mut u, &st, Hit::Ally(13, 0)).1, vec![[1063, 3, 2, 50]]);
        assert!(u.allies.is_none(), "Confirm closes the panel");
        press(&mut u, &st, Hit::Ally(15, 0));
        press(&mut u, &st, Hit::Ally(4, 0)); // Request 25 + 5 stone
        press(&mut u, &st, Hit::Ally(12, 4));
        press(&mut u, &st, Hit::Ally(11, 25));
        press(&mut u, &st, Hit::Ally(11, 5));
        assert_eq!(press(&mut u, &st, Hit::Ally(13, 0)).1, vec![[1064, 3, 4, 30]]);
    }

    /// HUD_MeritPanel.populateList: Greatest Lord by rank key (ties: gold), gold (ties: troops), troops (ties: gold);
    /// a fallen lord (key -1) last on the first; the same header again reverses; empty slots are left out.
    #[test]
    fn merit_ranking() {
        let mut d = [[0; 5]; 9];
        d[1] = [1, 500, 100, 10, 1];
        d[2] = [2, 900, 50, 30, 2];
        d[3] = [3, 500, 300, 30, 2];
        d[4] = [-1, 0, 0, 0, 0]; // no one
        d[5] = [5, -1, 0, 0, 0]; // fallen
        assert_eq!(ranking(&d, 0, false), [2, 3, 1, 5]);
        assert_eq!(ranking(&d, 0, true), [5, 1, 3, 2]);
        assert_eq!(ranking(&d, 1, false), [3, 1, 2, 5]);
        assert_eq!(ranking(&d, 2, false), [3, 2, 1, 5], "30 troops each: more gold first");
        d[3][2] = 50;
        assert_eq!(ranking(&d, 2, false), [2, 3, 1, 5], "a full tie keeps player order");
    }

    /// The standings: the HUD button opens it (and closes the allies panel), a header sorts and reverses, a row's keep
    /// sends HomeKeep for that player, a fallen lord has no keep button, Exit closes.
    #[test]
    fn merit_panel_clicks() {
        let Some(mut u) = ui() else { return };
        let mut st = skirmish();
        st.lords.merit_button = true;
        st.lords.merit[1] = [1, 100, 10, 0, 1];
        st.lords.merit[2] = [2, -1, 0, 0, 0];
        st.lords.merit[3] = [3, 300, 5, 4, 1];
        press(&mut u, &st, Hit::Ally(15, 0));
        press(&mut u, &st, Hit::Merit(9, 0));
        assert_eq!((u.merit, u.allies.is_none()), (Some(Merit::default()), true));
        assert_eq!(press(&mut u, &st, Hit::Merit(3, 3)).0, vec![[6, 3, 0]]);
        assert!(!u.hits.iter().any(|(_, h)| *h == Hit::Merit(3, 2)), "fallen: no keep");
        press(&mut u, &st, Hit::Merit(1, 0));
        press(&mut u, &st, Hit::Merit(1, 0));
        assert_eq!(u.merit, Some(Merit { sort: 1, rev: true }));
        press(&mut u, &st, Hit::Merit(8, 0));
        assert!(u.merit.is_none());
    }

    /// Extreme powers: hidden unless enabled; power k only from 636 · (k + 1) charge; a click is ExtremePower (1068, k).
    #[test]
    fn extreme_buttons() {
        let Some(mut u) = ui() else { return };
        let mut st = HudState::default();
        u.draw(&st, 1684.0, 784.0, [0.0; 2], 0, None);
        assert!(!u.hits.iter().any(|(_, h)| matches!(h, Hit::Act(1068, ..))));
        st.extreme = (1, 1300);
        assert_eq!(press(&mut u, &st, Hit::Act(1068, 1, 0)).0, vec![[1068, 1, 0]]);
        assert!(!u.hits.iter().any(|(_, h)| *h == Hit::Act(1068, 2, 0)), "1300 < 1908: power 3 locked");
    }

    /// Free Build: the extras button opens the menu; an invasion of 3 + 5 crusader troops sends CharSet, the counts and
    /// Start; the menu's event Start sends FreeBuild_Event (id, size) and closes it.
    #[test]
    fn free_build_menu() {
        let Some(mut u) = ui() else { return };
        let mut st = HudState::default();
        st.lords.free_build = true;
        st.lords.ping = [1, 1, 1, 0, 0, 0, 0, 0];
        press(&mut u, &st, Hit::Fb(-100));
        assert!(u.fb.is_some());
        press(&mut u, &st, Hit::Fb(200)); // Very Easy: 10 archers
        press(&mut u, &st, Hit::Fb(102)); // spearmen
        u.fb_slider(5, 0.5);
        assert_eq!(press(&mut u, &st, Hit::Fb(-2)).0, vec![[1032, 0, 0], [1033, 0, 10], [1033, 2, 100], [1034, 0, 0]]);
        u.draw(&st, 1684.0, 784.0, [0.0; 2], 0, None);
        assert!(!u.hits.iter().any(|(_, h)| *h == Hit::Fb(-2)), "Invade Now locked for 2 s");
        press(&mut u, &st, Hit::Fb(2)); // the second event (id 139) has a 1..10 slider
        press(&mut u, &st, Hit::Slider(4, 0));
        let a = press(&mut u, &st, Hit::Fb(-1)).0;
        assert!(matches!(a[..], [[1031, 139, 5 | 6]]), "{a:?}");
        assert!(u.fb.is_none());
    }

    /// Control groups: the troops panel's toggle shows the list; row "0" is group 0, Create / Select send GroupTroops0 /
    /// SelectClan0 (19 / 29).
    #[test]
    fn control_group_rows() {
        let Some(mut u) = ui() else { return };
        let mut st = HudState { app_mode: 14, app_sub_mode: 61, troops: vec![(22, 3)], ..Default::default() };
        press(&mut u, &st, Hit::Cg(9, 0));
        assert!(u.cg);
        assert_eq!(press(&mut u, &st, Hit::Cg(1, 0)).0, vec![[19, -1, 0]]);
        u.draw(&st, 1684.0, 784.0, [0.0; 2], 0, None);
        assert!(!u.hits.iter().any(|(_, h)| *h == Hit::Cg(0, 0)), "empty group: no Select");
        st.cg_total[0] = 5;
        st.cg[0][0] = (0, 3);
        assert_eq!(press(&mut u, &st, Hit::Cg(0, 0)).0, vec![[29, -1, 0]]);
        assert_eq!(press(&mut u, &st, Hit::Cg(3, 0)).0, vec![[19, 20, 0]]);
    }
}
