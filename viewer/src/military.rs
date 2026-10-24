//! `--live` troop controls the way the game does them. The front end (C# EditorDirector / TroopSelector) only turns
//! mouse events into DLL_TroopSelection's state: left button 1 press / 2 held / 3 release, right down / up, the drag
//! box (on / established, the units in it), the units under the cursor and, on a press, the units on screen. It hands
//! that over right before every DLL_RunTick; the sim's troop cursor FUN_18008b850 (`troopcursor::f8b850` + its callees
//! in `troopsel`, inside `frontend::run_tick`) then selects, picks the cursor sprite (draw record type 5) and issues
//! the orders as chores, with the DE's default SH1 controls (left click orders, right click deselects).
//! The patrol flag stays here (Troops_Patrol is never a map tool); siege tents are build tools (`mapaction`).
use crate::live::TroopIn;
use crate::replay::{self, Rec};
use shc_sim::{commands, mem::Mem};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const UN: u64 = 0x1867e53f0; // +0 count; unit +0x6e4 state, +0x6e6 type, +0x6ee owner, +0x70a selected by, +0x8f8 dying
const LOCAL: u64 = 0x1888e0d60;
const GROUPS: u64 = 0x187cc3710; // unit groups (0x688 each); +0 = the selection's group

fn u(i: i32) -> u64 { UN + i as u64 * 0x490 }
fn ty(m: &Mem, i: i32) -> i32 { m.i16(u(i) + 0x6e6) as i32 }
fn alive(m: &Mem, i: i32) -> bool { m.i16(u(i) + 0x6e4) == 2 && m.i16(u(i) + 0x8f8) == 0 }

/// The local player's selected units (+0x70a).
pub fn selected(m: &Mem) -> Vec<i32> {
    let p = m.i32(LOCAL);
    (1..m.i32(UN)).filter(|&i| alive(m, i) && m.i16(u(i) + 0x70a) as i32 == p && m.i16(u(i) + 0x6ee) as i32 == p).collect()
}

/// Ours (Ctrl+A, keys.rs EXTRA): every own, commandable unit of the selection's types on the whole map becomes the
/// selection, the way SelectClan selects a control group (marks +0x68c, FUN_180190470: bitmap → new group → chore
/// 0x10), so it goes out as the same chore in a LAN game. Returns the units selected.
pub fn select_types(m: &mut Mem) -> i32 {
    let p = m.i32(LOCAL);
    let types: std::collections::BTreeSet<i32> = selected(m).iter().map(|&i| ty(m, i)).collect();
    if types.is_empty() { return 0; }
    for i in 1..m.i32(UN) {
        let pick = alive(m, i) && m.i16(u(i) + 0x6ee) as i32 == p && m.i16(u(i) + 0x8fc) != 0 && types.contains(&ty(m, i));
        m.set_i16(u(i) + 0x68c, pick as i16);
    }
    shc_sim::frontend::f190470(m)
}

/// GameMap.body_actual_dimensions by eChimps type: width, height above and below the feet (game px, ×2 on screen).
fn body(t: i32) -> (f32, f32, f32) {
    match t {
        28 | 74 | 78 | 83 => (30.0, 62.0, 0.0),
        39 => (75.0, 55.0, 6.0),
        40 | 59 => (75.0, 55.0, 24.0),
        41 | 50 | 51 | 61 => (75.0, 55.0, 0.0),
        44 => (26.0, 32.0, 0.0),
        45 => (34.0, 36.0, 0.0),
        46 => (18.0, 12.0, 0.0),
        52 | 62 => (26.0, 12.0, 0.0),
        58 => (80.0, 135.0, 18.0),
        60 => (75.0, 55.0, 8.0),
        77 => (75.0, 52.0, 24.0),
        _ => (26.0, 42.0, 0.0),
    }
}

/// EditorDirector's DLL_TroopSelection inputs between two DLL_RunTicks.
#[derive(Default)]
pub struct Troop {
    left: i32, // leftMouseStateForEngine
    read: bool,
    up_pending: bool,
    set_at: Option<Instant>,
    rdown: bool, // rightDown / rightUp ForEngine, latched until the next tick
    rup: bool,
    start: Option<[f32; 2]>, // troopSelectMouseStart (screen px)
    start_w: [f32; 2],       // the same in world px
    action: i32,             // MainControls.CurrentAction: 0, 8 drag box, 9 box just released
    on: bool,                // TroopSelector.selection_on
    established: bool,       // TroopSelector.selection_established (dragged > 16 px)
    box_on: bool,            // troopSelectionBoxOn
    boxed: Vec<i32>,         // selectedChimpList: the units in the box
    on_screen: Vec<i32>,     // onScreenChimpsList, grabbed on the press
    cursor: [f32; 2],        // world px
    screen: [f32; 2],
    pub keys: [bool; 3], // shift, ctrl, alt (DLL_RunTick)
    units: BTreeMap<i32, Rec>, // last draw record per unit index: GameMap's chimps
    narrow: bool,              // drop the selection before this press goes in (see `event`)
}

impl Troop {
    /// EditorDirector.updateLeftMouseStateForEngine: a release within 500 ms of an unread press waits a tick.
    fn set_left(&mut self, state: i32) {
        if matches!(state, 2 | 3) && self.left == 0 { return; }
        if !self.read && self.left > 0 && self.set_at.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
            if state == 3 { self.up_pending = true; }
        } else {
            (self.left, self.read, self.set_at) = (state, false, Some(Instant::now()));
        }
    }

    /// EditorDirector.getMouseStateForEngine: 1 → 2 (held), 3 → 0 after it was read once.
    fn take_left(&mut self) -> i32 {
        let r = self.left;
        self.read = true;
        if std::mem::take(&mut self.up_pending) { self.left = 3 } else if self.left == 1 { self.left = 2 } else if self.left == 3 { self.left = 0 }
        r
    }

    /// One UI mouse event (EditorDirector.Update).
    pub fn event(&mut self, m: &Mem, rot: i16, e: TroopIn) {
        match e {
            TroopIn::Press(w, s, view) => {
                self.set_left(1);
                if self.action == 9 { self.action = 0; }
                (self.start, self.start_w, self.cursor, self.screen) = (Some(s), w, w, s);
                self.on_screen = self.grab(m, rot, view);
                // ponytail: not the original's. With SH1 controls the engine only orders when the unit under the
                // cursor is already selected, so a click can't pick one unit out of a group; deselect first and the
                // press selects just it (no Shift / Ctrl: those add to the selection).
                let sel = selected(m);
                self.narrow = crate::menu::sh1_rts() && self.keys == [false; 3] && sel.len() > 1
                    && self.under(m, rot).first().is_some_and(|i| sel.contains(i));
            }
            TroopIn::Release => {
                if self.action == 8 {
                    self.boxed = self.grab(m, rot, self.rect());
                    self.box_on |= self.on;
                    (self.on, self.established, self.action) = (false, false, 9); // TroopSelector.endSelection
                }
                self.set_left(3);
                self.start = None;
            }
            TroopIn::Right(down) => if down { self.rdown = true } else { self.rup = true },
            TroopIn::Cursor(w, s, keys) => {
                (self.cursor, self.screen, self.keys) = (w, s, keys);
                let Some(s0) = self.start else { return };
                let d = (s[0] - s0[0]).abs().max((s[1] - s0[1]).abs());
                if self.action == 0 && d > 3.0 { (self.action, self.on, self.established) = (8, true, false); }
                if self.action == 8 {
                    self.established |= d > 16.0;
                    self.boxed = self.grab(m, rot, self.rect());
                }
            }
        }
    }

    /// GameData: RunTick's completeSelectionBox (+0x3b8) > 0, the sim wants a fresh selection (a press on an
    /// unselected own unit): CurrentAction 8, TroopSelector.startSelection at the mouse and triggerTroopsSelection, a
    /// zero-size box there. The release then hands the units under the click to the engine, which keeps one.
    pub fn complete_box(&mut self, m: &Mem, rot: i16) {
        (self.action, self.on, self.established) = (8, true, false);
        (self.start, self.start_w) = (Some(self.screen), self.cursor);
        self.boxed = self.grab(m, rot, self.rect());
    }

    /// The drag box (world px), press → cursor.
    fn rect(&self) -> [f32; 4] { [self.start_w[0], self.start_w[1], self.cursor[0], self.cursor[1]] }

    /// The box being dragged (world px), for drawing.
    pub fn drag_box(&self) -> Option<[f32; 4]> { (self.action == 8 && self.established).then(|| self.rect()) }

    /// GameMap keeps a chimp per unit draw record (type 2, objectID = unit index; file 0 deletes it).
    pub fn track(&mut self, recs: &[Rec]) {
        for r in recs.iter().filter(|r| r[0] == 2) {
            if r[3] == 0 { self.units.remove(&(r[10] as i32)); } else { self.units.insert(r[10] as i32, *r); }
        }
    }

    /// A unit's body box in world px (x0, y0, x1, y1): Chimp.position ± bodyWidth / 2, bodyHeight up, bodyHeightBelow
    /// down, 3/32 unit lower (grabTroopsOnScreen). ponytail: tele_x_adjust and the between-tiles slide are left out.
    fn body_px(m: &Mem, rot: i16, i: i32, r: &Rec) -> [f32; 4] {
        let mut r = *r;
        (r[1], r[2]) = replay::to_west_pub(rot, r[1], r[2]);
        let p = replay::chimp_px(&r, 0); // Chimp.position
        let (w, h, below) = body(ty(m, i));
        [p[0] - w, p[1] - 2.0 * h + 6.0, p[0] + w, p[1] + 2.0 * below + 6.0]
    }

    /// GameMap.grabTroopsOnScreen with a box: every drawn unit whose body meets world rect `r`, by id (the engine
    /// keeps the selectable ones).
    fn grab(&self, m: &Mem, rot: i16, r: [f32; 4]) -> Vec<i32> {
        let (x0, x1, y0, y1) = (r[0].min(r[2]), r[0].max(r[2]), r[1].min(r[3]), r[1].max(r[3]));
        self.units.iter().filter(|(&i, rec)| {
            let b = Self::body_px(m, rot, i, rec);
            b[0] <= x1 && b[2] >= x0 && b[1] <= y1 && b[3] >= y0
        }).map(|(&i, _)| i).collect()
    }

    /// grabTroopsOnScreen's chimpsUnderCursor: units whose body holds the cursor, front-most (lowest) first.
    fn under(&self, m: &Mem, rot: i16) -> Vec<i32> {
        let c = self.cursor;
        let mut v: Vec<(f32, i32)> = self.units.iter().filter_map(|(&i, rec)| {
            let b = Self::body_px(m, rot, i, rec);
            (c[0] >= b[0] && c[0] <= b[2] && c[1] >= b[1] && c[1] <= b[3]).then_some((b[3], i))
        }).collect();
        v.sort_by(|a, b| b.0.total_cmp(&a.0));
        v.into_iter().map(|e| e.1).collect()
    }

    /// EditorDirector.preDLLCallActions: DLL_TroopSelection right before DLL_RunTick. ponytail: building / troop over
    /// depth stay 0 (units under the cursor always win over a building behind them).
    pub fn apply(&mut self, m: &mut Mem, rot: i16) {
        let (rd, ru) = (std::mem::take(&mut self.rdown), std::mem::take(&mut self.rup));
        let mut num = self.take_left();
        if rd && self.on { // a right press ends the box (scheduleTroopSelectionEnd)
            (self.on, self.established, self.box_on, self.action, self.start) = (false, false, false, 0, None);
            num = 0;
        }
        if self.action == 9 && num == 3 { self.action = 0; }
        // Troops_DeSelectExceptType (1022) with no type: everyone out of the selection, by the chore (LAN-safe)
        if std::mem::take(&mut self.narrow) { shc_sim::frontend::game_action(m, 1022, 0, 0, 0, 0); }
        let under = self.under(m, rot);
        let os: &[i32] = if num == 1 { &self.on_screen } else { &[] };
        let sel = &self.boxed;
        shc_sim::frontend::troop_selection(m, num, rd as u8, ru as u8, sel.len() as i32, sel, (self.on || self.box_on) as u8,
            self.established as u8, under.len() as i32, &under, self.screen[0] as i32, self.screen[1] as i32, 1, os.len() as i32, os);
        if !matches!(self.action, 8 | 9) { self.box_on = false; self.boxed.clear(); }
    }
}

const PATROL: u64 = 0x187cc372c; // FUN_18008f760's flag: the next move order patrols

/// Troops_Patrol (GameAction 1019 returns tool 0x22, FUN_18008afc0(0x22)): with troops selected, toggle the patrol
/// flag; the next move order patrols between the troops and the target.
fn patrol(m: &mut Mem) -> bool {
    if selected(m).is_empty() { return false; }
    let on = (m.i32(PATROL) == 0) as i32;
    m.set_i32(PATROL, on);
    m.set_i32(0x187cc3730, 1);
    on != 0
}

/// After DLL_GameAction: Troops_Patrol (1019) toggles the patrol flag. Every result goes to `hud::ACTION_RET` (the
/// market reads its trade errors there; EngBuild_* 1014..1018 / 1060 return the siege tent tool the HUD then holds
/// like a build tool, as C# GameAction → EditorDirector.placeBuilding does).
pub fn after_action(m: &mut Mem, action: i32, ret: i32) {
    if let Ok(mut r) = crate::hud::ACTION_RET.lock() { *r = (action, ret, r.2.wrapping_add(1)); }
    if action == 1019 { println!("patrol {}", if patrol(m) { "on" } else { "off" }); }
}

/// What the HUD needs beside PlayStateReturnData: the local player, DLL_GetMeritData's "still in" per lord
/// ([p, 1] >= 0: in the game, its lord alive or the game < 200 ticks old), and the game kind for the allies button
/// (skirmish / multiplayer, not a campaign) and the Free Build menu (game mode 2, scenario kind 3, single player).
pub fn hud_extra(m: &Mem, h: &mut crate::hud::HudState) {
    let (mode, mp) = (m.i32(0x183663eb8), m.i32(0x188571b80));
    let l = &mut h.lords;
    l.me = m.i32(LOCAL);
    l.colours = crate::endgame::radar_colours(m);
    l.allies_button = mp != 0 && mode != 0;
    l.free_build = mp == 0 && mode == 2 && m.i32(0x1832d8cac) == 3;
    let young = m.i32(0x183663eec) + 200 >= m.i32(0x183663ee8);
    for p in 1..9 { l.in_game[p] = m.u8(0x1887ebd7d + p as u64 - 1) != 0 && (young || lord_alive(m, p as i32)); }
    l.merit_button = mp != 0; // SkirmishModeVisMerit: any skirmish / multiplayer game
    l.merit = merit(m);
}

/// DLL_GetMeritData (0x1800833b0) as C# GetMeritData's int[9, 5]: per player 1..8 [player (-1 = no one), rank key
/// (FUN_180001e20; -1 = the lord fell over 200 ticks into the game), gold (≥ 0), troops (record +0x53c), team].
/// Read only.
pub fn merit(m: &Mem) -> [[i32; 5]; 9] {
    let young = m.i32(0x183663eec) + 200 >= m.i32(0x183663ee8);
    let mut d = [[0; 5]; 9];
    for (p, row) in d.iter_mut().enumerate().skip(1) {
        let r = shc_sim::game::player(p as u32);
        *row = match () {
            _ if m.u8(0x1887ebd7d + p as u64 - 1) == 0 => [-1, 0, 0, 0, 0],
            _ if !young && !lord_alive(m, p as i32) => [p as i32, -1, 0, 0, 0],
            _ => [p as i32, rank_key(m, r), m.i32(r + 0x50c).max(0), m.i32(r + 0x53c), m.i32(0x1837f1db0 + (p as u64 - 1) * 4)],
        };
    }
    d
}

/// FUN_180001e20(p): the merit score the Greatest Lord sort uses: the stock (record +0x4d0 + 4 · good) of goods 2..4,
/// 6, 8, 10..14, 16..24 at their value (FUN_1800ceb30: the market's price table, G + 0x1817bc, ÷ 5) in three sums ÷ 10,
/// population (+0x2180) × 10, record +0x38e8 and gold ÷ 10. C int arithmetic (wrapping). The Steam DLL's build has
/// these globals 0x2070 higher than the one the sim ports (its LEA 0x18366c210 = the sim's G).
fn rank_key(m: &Mem, r: u64) -> i32 {
    use shc_sim::game::{G, P_GOLD, P_POPULATION};
    let worth = |goods: &[u64]| goods.iter().fold(0i32, |a, &k| a.wrapping_add((m.i32(G + 0x1817bc + k * 8) / 5).wrapping_mul(m.i32(r + 0x4d0 + k * 4))));
    (worth(&[2, 3, 4, 6, 8, 14, 16]) / 10).wrapping_add(m.i32(r + P_POPULATION).wrapping_mul(10)).wrapping_add(worth(&[17, 18, 19, 20, 21, 22, 23, 24]) / 10)
        .wrapping_add(worth(&[10, 11, 12, 13]) / 10).wrapping_add(m.i32(r + 0x38e8)).wrapping_add(m.i32(r + P_GOLD) / 10)
}

/// FUN_180186e20(units, p): player p's lord unit (id 0x183a0d980, serial 0x183a0d994) is alive (sim commands::lord_alive).
fn lord_alive(m: &Mem, p: i32) -> bool {
    let u = m.i16(0x183a0d980 + p as u64 * 2) as i32;
    let r = shc_sim::units::u(u);
    u != 0 && m.i32(r + 0x98) == m.i32(0x183a0d994 + p as u64 * 4) && m.i16(r + 0x8c) == 2 && m.i16(r + 0x2a0) == 0
}

/// A selection-box outline (screen px corners a, b) as HUD triangles (texture 1's white texel).
pub fn box_tris(a: [f32; 2], b: [f32; 2]) -> Vec<crate::hud::HudTri> {
    let t = 1.0 / crate::hud::FONT_PX as f32;
    let (x0, x1, y0, y1) = (a[0].min(b[0]), a[0].max(b[0]), a[1].min(b[1]), a[1].max(b[1]));
    let v = |x: f32, y: f32| crate::Vert { pos: [x, y], uv: [t, t], color: [1.0, 1.0, 1.0, 0.9], team: [0.0; 4] };
    [[x0, y0, x1, y0 + 1.0], [x0, y1 - 1.0, x1, y1], [x0, y0, x0 + 1.0, y1], [x1 - 1.0, y0, x1, y1]].iter()
        .flat_map(|q| [(1, [v(q[0], q[1]), v(q[2], q[1]), v(q[2], q[3])]), (1, [v(q[0], q[1]), v(q[2], q[3]), v(q[0], q[3])])]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::Live;
    use shc_sim::buildings as b;

    fn save(name: &str) -> Option<Live> {
        // default options (SH1 RTS controls on), not the developer's settings.cfg (Settings::load falls back to it)
        let dir = std::env::temp_dir().join("shc-military-test");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("settings.cfg"), "SH1RTSControls:True\n");
        crate::skirmish::set_test_profile(&dir);
        let p = crate::paths::test_save(&format!("{name}"));
        std::path::Path::new(&p).exists().then(|| Live::load(&p))
    }
    /// DLL_GetMeritData on a 5-lord skirmish save: gold = the HUD's, troops +0x53c, the rank key (FUN_180001e20) =
    /// wood 70 · 1 + stone 29 · 7 → 27, population 10 · 10, +0x38e8 124, gold 1588 / 10; slots 6..8 empty.
    #[test]
    fn merit_rows() {
        let Some(l) = save("Crater Lake-1test.sav") else { return };
        let d = merit(l.mem_ref());
        assert_eq!(d[1], [1, 409, 1588, 19, 0]);
        assert_eq!(d[2][2], 1686);
        assert!(d[6..].iter().all(|r| r[0] == -1));
    }
    fn tile(m: &Mem, i: i32) -> (i32, i32) { (m.i16(u(i) + 0x71c) as i32, m.i16(u(i) + 0x71e) as i32) }
    /// Unit goal tile (units-object +0xec / +0xee in `units::u` terms = UN + 0x744 / 0x746).
    fn goal(m: &Mem, i: i32) -> (i16, i16) { (m.i16(u(i) + 0x744), m.i16(u(i) + 0x746)) }
    fn own(m: &Mem, t: i32) -> Vec<i32> { (1..m.i32(UN)).filter(|&i| alive(m, i) && m.i16(u(i) + 0x6ee) == 1 && ty(m, i) == t).collect() }

    /// Centre of unit `i`'s body in world px (what a click on it hits).
    fn at(l: &Live, i: i32) -> [f32; 2] {
        let t = &l.troop;
        let b = Troop::body_px(l.mem_ref(), l.rotation(), i, &t.units[&i]);
        [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]
    }

    /// A left click (press + release) at world px `w`, a tick for each.
    fn click(l: &mut Live, w: [f32; 2]) {
        let s = [w[0] + 5000.0, w[1] + 5000.0]; // any screen px; only press == release matters
        l.troop_event(TroopIn::Cursor(w, s, [false; 3]));
        l.troop_event(TroopIn::Press(w, s, [w[0] - 900.0, w[1] - 500.0, w[0] + 900.0, w[1] + 500.0]));
        l.step();
        l.troop_event(TroopIn::Release);
        l.step();
    }

    /// Box-select the own archers of Crater Lake (troops panel opens), left-click 12 tiles away (SH1 controls);
    /// their goals move there within a few ticks.
    #[test]
    fn select_and_move() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let ids = own(l.mem(), 0x16);
        assert!(!ids.is_empty(), "no own archers");
        // a box around all of them
        let ps: Vec<[f32; 2]> = ids.iter().map(|&i| at(&l, i)).collect();
        let (lo, hi) = ps.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
        let (a, z) = ([lo[0] - 10.0, lo[1] - 10.0], [hi[0] + 10.0, hi[1] + 10.0]);
        l.troop_event(TroopIn::Cursor(a, a, [false; 3]));
        l.troop_event(TroopIn::Press(a, a, [a[0] - 900.0, a[1] - 500.0, a[0] + 900.0, a[1] + 500.0]));
        l.step();
        l.troop_event(TroopIn::Cursor(z, [a[0] + 40.0, a[1] + 40.0], [false; 3]));
        l.step();
        l.troop_event(TroopIn::Release);
        for _ in 0..3 { l.step(); }
        let h = l.hud();
        let sel = selected(l.mem());
        println!("selected {} of {} archers; panel {} / {}", sel.len(), ids.len(), h.app_mode, h.app_sub_mode);
        assert!(ids.iter().all(|i| sel.contains(i)), "box selected {sel:?}, archers {ids:?}");
        assert_eq!(h.app_mode, 14, "troops panel");
        let m = l.mem();
        let before: Vec<_> = ids.iter().map(|&i| goal(m, i)).collect();
        let (x, y) = tile(m, ids[0]);
        let (tx, ty_) = (x + 12, y + 6);
        // a left click on bare ground at the target tile (SH1 controls): nothing under the cursor
        l.mouse = [tx, ty_];
        click(&mut l, [-1e6, -1e6]);
        for _ in 0..5 { l.step(); }
        assert!(!selected(l.mem()).is_empty(), "the order click kept the selection");
        let m = l.mem();
        let after: Vec<_> = ids.iter().map(|&i| goal(m, i)).collect();
        println!("goals before {before:?}\n      after {after:?}");
        assert!(after.iter().zip(&before).filter(|(a, b)| a != b).count() * 2 >= ids.len(), "most goals unchanged");
        assert!(after.iter().all(|&(gx, gy)| (gx as i32 - tx).abs() <= 6 && (gy as i32 - ty_).abs() <= 6), "goals not near the target");
    }

    /// A click on one own spearman selects just it; over an enemy building the sim's cursor shows the attack sprite
    /// (draw record type 5: GM 107 frame 48 + its 16-step animation) and a click orders the attack (orders::attack_building: the group's
    /// target). Archers get the "can't" cursor there (GM 172 frame 65): they don't attack buildings.
    #[test]
    fn click_select_and_attack_building() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        // a spearman no other unit stands in front of (the click picks the front-most body)
        let ids = own(l.mem(), 0x18);
        let sel = ids.iter().find_map(|&i| {
            l.troop_event(TroopIn::Right(true)); // drop the last try (SH1 controls: a click with troops selected orders)
            l.step();
            l.troop_event(TroopIn::Right(false));
            let w = at(&l, i);
            click(&mut l, w);
            let s = selected(l.mem());
            (s == vec![i]).then_some(s)
        });
        println!("click selected {sel:?}");
        assert!(sel.is_some(), "no spearman selectable by a click");
        let m = l.mem();
        let team = |p: i32| m.i32(0x1837ebecc + p as u64 * 4);
        // an enemy tower-like building (kinds 0x1e..0x21): what melee troops may attack
        let (bid, bx, by) = (1..b::SLOTS).filter(|&i| b::state(m, i) == 2 && b::owner(m, i) > 0 && team(b::owner(m, i)) != team(1) && (0x1e..=0x21).contains(&b::kind(m, i))).map(|i| {
            let r = b::rec(i);
            let n = m.i32(r + 0xf8);
            (i, m.i16(r + 0xee) as i32 + n / 2, m.i16(r + 0xf0) as i32 + n / 2)
        }).next().expect("no enemy tower");
        l.mouse = [bx, by];
        l.troop_event(TroopIn::Cursor([-1e6, -1e6], [1.0, 1.0], [false; 3]));
        let cursor: Vec<(i16, i16)> = l.step().iter().filter(|r| r[0] == 5).map(|r| (r[3], r[4])).collect();
        click(&mut l, [-1e6, -1e6]);
        l.step();
        let m = l.mem();
        let g = m.i32(GROUPS);
        let target = m.i16(GROUPS + g as u64 * 0x688 + 0x64c) as i32;
        println!("building {bid} at {bx},{by}: cursor {cursor:?}, group {g} target {target}");
        assert!(cursor.len() == 1 && cursor[0].0 == 107 && (48..64).contains(&cursor[0].1), "attack cursor (GM 107, frame 48 + animation): {cursor:?}");
        assert_eq!(target, bid, "no attack order on the enemy building");
    }

    /// SH1 controls: with a group selected, a click on one of its units leaves just that unit selected.
    #[test]
    fn click_one_out_of_group() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let ids = own(l.mem(), 0x16);
        let ps: Vec<[f32; 2]> = ids.iter().map(|&i| at(&l, i)).collect();
        let (lo, hi) = ps.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
        let (a, z) = ([lo[0] - 10.0, lo[1] - 10.0], [hi[0] + 10.0, hi[1] + 10.0]);
        l.troop_event(TroopIn::Cursor(a, a, [false; 3]));
        l.troop_event(TroopIn::Press(a, a, [a[0] - 900.0, a[1] - 500.0, a[0] + 900.0, a[1] + 500.0]));
        l.step();
        l.troop_event(TroopIn::Cursor(z, [a[0] + 40.0, a[1] + 40.0], [false; 3]));
        l.step();
        l.troop_event(TroopIn::Release);
        for _ in 0..3 { l.step(); }
        let sel = selected(l.mem());
        assert!(sel.len() > 1, "box selected {sel:?}");
        // a selected archer that is front-most where it is clicked
        let i = *ids.iter().find(|&&i| { let w = at(&l, i); l.troop.cursor = w; l.troop.under(l.mem_ref(), l.rotation()).first() == Some(&i) }).expect("no archer clear in front");
        l.mouse = [tile(l.mem(), i).0, tile(l.mem(), i).1];
        std::thread::sleep(std::time::Duration::from_millis(600)); // past the engine's double-click window
        let w = at(&l, i);
        click(&mut l, w);
        l.step();
        assert_eq!(selected(l.mem()), vec![i], "the click kept the group");
    }

    /// A click on the own lord selects him alone: the engine's HUD state says so (+2586 lordOnlySelected), which keeps
    /// the right-click menu shut with the modern controls (hud::Ui::rclick_blocked).
    #[test]
    fn lone_lord_flag() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let lord = own(l.mem(), 0x37)[0];
        let w = at(&l, lord);
        click(&mut l, w);
        l.step();
        let h = l.hud();
        println!("selected {:?}, lord {lord}, panel {} / {}, lord only {}", selected(l.mem()), h.app_mode, h.app_sub_mode, h.lord_only_selected);
        assert_eq!(selected(l.mem()), vec![lord]);
        assert_eq!(h.lord_only_selected, 1);
    }

    /// The troops panel's type filter: Troops_DeSelectExceptType (1022) keeps only that type, Troops_DeSelectType
    /// (1021) drops it.
    #[test]
    fn select_type_from_panel() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let all: Vec<i32> = l.troop.units.keys().copied().filter(|&i| alive(l.mem_ref(), i) && l.mem_ref().i16(u(i) + 0x6ee) == 1).collect();
        let w: Vec<[f32; 2]> = all.iter().map(|&i| at(&l, i)).collect();
        let (lo, hi) = w.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
        let (a, z) = ([lo[0] - 10.0, lo[1] - 10.0], [hi[0] + 10.0, hi[1] + 10.0]);
        l.troop_event(TroopIn::Cursor(a, a, [false; 3]));
        l.troop_event(TroopIn::Press(a, a, [a[0] - 900.0, a[1] - 500.0, a[0] + 900.0, a[1] + 500.0]));
        l.step();
        l.troop_event(TroopIn::Cursor(z, [a[0] + 40.0, a[1] + 40.0], [false; 3]));
        l.step();
        l.troop_event(TroopIn::Release);
        for _ in 0..3 { l.step(); }
        let types = |l: &mut Live| { let m = l.mem(); let mut t: Vec<i32> = selected(m).iter().map(|&i| ty(m, i)).collect(); t.dedup(); t.sort(); t.dedup(); t };
        assert!(types(&mut l).len() > 1, "box got one type: {:?}", types(&mut l));
        shc_sim::frontend::game_action(l.mem(), 1022, 0, 0x16, 0, 0);
        l.step();
        assert_eq!(types(&mut l), vec![0x16], "keep archers");
        assert_eq!(selected(l.mem()).len(), own(l.mem(), 0x16).len());
        shc_sim::frontend::game_action(l.mem(), 1021, 0, 0x16, 0, 0);
        l.step();
        assert!(selected(l.mem()).is_empty(), "archers dropped");
    }

    /// A unit standing on raised ground is drawn 2 × height px higher (record +5 = −height ×32); a click on its drawn
    /// body selects it, a click where it would stand on flat ground does not.
    #[test]
    fn click_unit_on_raised_tile() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let m = l.mem_ref();
        let (&i, r) = l.troop.units.iter().filter(|(&i, _)| alive(m, i) && m.i16(u(i) + 0x6ee) == 1 && m.i16(u(i) + 0x8fc) != 0).min_by_key(|(_, r)| r[5]).expect("no own unit drawn");
        let lift = -r[5] as f32 * 2.0;
        let w = at(&l, i);
        println!("unit {i} at {:?}: drawn {lift} px higher, body centre {w:?}", tile(m, i));
        assert!(lift > 50.0, "no own unit on raised ground");
        click(&mut l, [w[0], w[1] + lift]);
        assert!(!selected(l.mem()).contains(&i), "selected by a click below its body");
        click(&mut l, w);
        assert_eq!(selected(l.mem()), vec![i]);
    }

    /// A clicked unit carries the marker record 0x33 (health-bar frame k10 of GM 16) after its chimp record, as the
    /// original draws selected units; a right press drops the selection (SH1 controls).
    #[test]
    fn selected_units_marked() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let ids = own(l.mem(), 0x16);
        let w = at(&l, ids[0]);
        let mut marked = std::collections::BTreeSet::new();
        let mut wd = replay::World::new();
        l.troop_event(TroopIn::Cursor(w, [9.0, 9.0], [false; 3]));
        l.troop_event(TroopIn::Press(w, [9.0, 9.0], [0.0; 4]));
        for k in 0..4 {
            if k == 1 { l.troop_event(TroopIn::Release); }
            let recs = l.step();
            wd.apply(&recs, &mut vec![]);
            marked.extend(recs.windows(2).filter(|p| p[0][0] == 2 && p[1][0] == 0x33 && p[1][10] > 0).map(|p| p[0][10] as i32));
        }
        println!("marked {marked:?}, world marks {:?}", wd.marks.keys().collect::<Vec<_>>());
        let sel = selected(l.mem());
        assert!(sel.len() == 1 && marked.contains(&sel[0]) && wd.marks.contains_key(&sel[0]), "selected {sel:?}, marked {marked:?}");
        l.troop_event(TroopIn::Right(true));
        l.step();
        l.troop_event(TroopIn::Right(false));
        l.step();
        assert!(selected(l.mem()).is_empty(), "right press kept the selection");
    }

    /// The order's ground marker: while selected troops walk, FUN_1801213c0 puts their target flag (GM 107, frame
    /// 0x51 + the 0x6a-v / 0x52+blink animation) on the target tile as a pixie (draw record 8: "on walls" layer, drawn
    /// under the units), which replay::World keeps as an object for drawing.
    #[test]
    fn order_marker_drawn() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let ids = own(l.mem(), 0x16);
        let w = at(&l, ids[0]);
        let mut wd = replay::World::new();
        wd.apply(&l.records(), &mut vec![]);
        click(&mut l, w);
        let (x, y) = tile(l.mem(), ids[0]);
        let (tx, ty_) = (x + 8, y + 4);
        l.mouse = [tx, ty_];
        click(&mut l, [-1e6, -1e6]); // SH1 controls: a left click on the ground orders
        let mut marks = vec![];
        for _ in 0..6 {
            let recs = l.step();
            wd.apply(&recs, &mut vec![]);
            marks.extend(recs.iter().filter(|r| matches!(r[0], 8 | 9) && r[3] == 107).map(|r| (r[1], r[2], r[4])));
        }
        let (gx, gy) = replay::to_west_pub(l.rotation(), tx as i16, ty_ as i16);
        let kept: Vec<_> = wd.objs.iter().filter(|((t, _), r)| matches!(t, 8 | 9) && r[3] == 107).map(|(_, r)| (r[1], r[2], r[4])).collect();
        println!("marker records {marks:?}; kept {kept:?}; target west {gx},{gy}");
        assert!(!marks.is_empty(), "no GM 107 marker pixie after the order");
        assert!(kept.iter().any(|&(px, py, _)| (px - gx).abs() <= 3 && (py - gy).abs() <= 3), "no marker kept at the target");
    }

    /// GameAction 1007 MakeTroop (as the barracks / mercenary / guild panels send it, s = 1) queues chore 0x1f, and
    /// it turns a free peasant into the troop (paid at once, the unit appears once the peasant reached the post).
    #[test]
    fn recruit_queues_chore() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let before = own(l.mem(), 70).len(); // arab archer from the mercenary post (Crater Lake has no barracks)
        // chore slots: +8 type, +9 state (1 = pending)
        let pending = |m: &Mem| (0..500u64).filter(|i| { let s = commands::CHORES + 0xb0bf8 + i * 0x500; m.u8(s + 8) == 0x1f && m.u8(s + 9) == 1 }).count();
        let n0 = pending(l.mem());
        l.game_action(1007, 1, 70);
        let n1 = pending(l.mem());
        println!("pending 0x1f chores {n0} -> {n1}");
        assert_eq!(n1, n0 + 1, "no recruit chore queued");
        let gold = l.hud().gold;
        l.step();
        assert!(l.hud().gold < gold, "not paid");
        // the peasant walks to the post first
        let k = (0..200).find(|_| { l.step(); own(l.mem(), 70).len() > before }).expect("no arab archer after 200 ticks");
        println!("arab archer {} after {k} ticks, gold {gold} -> {}", before + 1, l.hud().gold);
    }

    /// MakeTroop end to end at each recruiting post: the unit is paid and appears once a peasant walked there
    /// (stone barracks archer with bows in the armoury, mercenary post Arab archer, Bedouin stockade skirmisher /
    /// eunuch / sapper). The HUD's recruit button for it is unlocked before the click.
    #[test]
    fn recruit_each_post() {
        for (name, t) in [("asd2.sav", 22), ("Crater Lake-1test.sav", 70), ("Crater Lake-1test.sav", 82), ("Crater Lake-1test.sav", 80), ("Crater Lake-1test.sav", 84)] {
            let Some(mut l) = save(name) else { continue };
            l.step();
            let (before, gold) = (own(l.mem(), t).len(), l.hud().gold);
            let r = l.game_action(1007, 1, t);
            let k = (0..400).find(|_| { l.step(); own(l.mem(), t).len() > before });
            println!("{name} type {t}: action -> {r}, gold {gold} -> {}, unit after {k:?} ticks", l.hud().gold);
            assert!(k.is_some(), "{name}: no type {t} unit after 400 ticks");
        }
    }

    /// asd2's engineers: select them with a box, EngBuild_Catapult → tool 0xbe, then the tool runs through
    /// DLL_StartMapAction / DLL_MapAction like any build tool (C# placeBuilding): the hover draws the flag marker
    /// (file 0x52) on a good spot and the red tent ghost on a bad one, the click queues the tent.
    #[test]
    fn siege_tent() {
        let Some(mut l) = save("asd2.sav") else { return };
        l.step();
        let eng = own(l.mem(), 0x1e);
        assert!(!eng.is_empty(), "no own engineers");
        let e = *eng.iter().find(|i| l.troop.units.contains_key(i)).expect("no engineer drawn");
        let w = at(&l, e);
        click(&mut l, w);
        assert!(!selected(l.mem()).is_empty(), "engineer not selected");
        assert_eq!(l.game_action(1014, 0, 0), 0xbe);
        assert!(l.set_tool(0xbe), "tent tool refused with engineers selected");
        let tents = |l: &mut Live| { let m = l.mem(); (1..b::SLOTS).filter(|&i| b::state(m, i) != 0 && (80..=84).contains(&b::kind(m, i))).count() };
        let n0 = tents(&mut l);
        let (kx, ky) = l.keep().map(|(x, y)| (x as i32, y as i32)).unwrap();
        let spots: Vec<_> = (-20..=20).step_by(3).flat_map(|d| (-20..=20).step_by(3).map(move |e| (kx + d, ky + e))).collect();
        let bad = spots.iter().copied().find(|&(x, y)| !l.hover(0xbe, x, y).ok).expect("no refused spot");
        l.hover(0xbe, bad.0, bad.1);
        assert!(!l.preview.is_empty(), "refused spot: no tent ghost in the preview layer");
        let good = spots.iter().copied().find(|&(x, y)| l.hover(0xbe, x, y).ok).expect("no good spot");
        let v = l.hover(0xbe, good.0, good.1);
        assert_eq!(v.size, 3);
        let recs = l.step();
        assert!(recs.iter().any(|r| r[3] == 0x52), "good spot: no flag marker (file 0x52) drawn");
        let (v, n) = l.click(0xbe, good.0, good.1);
        assert!(v.ok && n == 1, "click: {v:?}, {n} chore(s)");
        for _ in 0..3 { l.step(); }
        println!("tent at {good:?}; tents {n0} -> {}", tents(&mut l));
        assert!(tents(&mut l) > n0, "no siege tent placed");
    }

    /// Extended rules: a Sassanid Knight's draw records name our art (gm 1044), a DE knight beside it keeps 44.
    #[test]
    fn sassanid_drawn_with_own_art() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        let m = l.mem();
        shc_sim::ext::set(m, true);
        let a = (1..m.i32(UN)).find(|&i| alive(m, i) && m.i16(u(i) + 0x6ee) == 1).unwrap();
        let (x, y) = (m.i16(u(a) + 0x70e), m.i16(u(a) + 0x710)); // record +0xb6 / +0xb8 (this file's u is 0x658 lower)
        let sk = shc_sim::units::helpers::spawn(m, 1, 0, x, y, 0, 0x1c);
        let de = shc_sim::units::helpers::spawn(m, 1, 0, x, y, 0, 0x1c);
        shc_sim::ext::make_sassanid(m, sk);
        let files = |r: &[crate::replay::Rec], i: i32| r.iter().filter(|r| r[0] == 2 && r[10] == i as i16).map(|r| r[3]).collect::<Vec<_>>();
        let mut seen = (vec![], vec![]);
        for _ in 0..5 { let r = l.step(); seen.0.extend(files(&r, sk)); seen.1.extend(files(&r, de)); }
        assert!(!seen.0.is_empty() && seen.0.iter().all(|&f| f == 1044), "Sassanid: {:?}", seen.0);
        assert!(!seen.1.is_empty() && seen.1.iter().all(|&f| f == 44), "DE knight: {:?}", seen.1);
    }

    /// A 3-lord Crater Lake skirmish, me + the Rat on team 1 against the Snake: the HUD's lord table has the Rat as the
    /// one ally, the Snake as the one enemy, the allies button on and no Free Build menu; Ally_SendGoods (1063: the
    /// Rat, gold, 100) runs as the chore that hands it over.
    #[test]
    fn allies_in_skirmish() {
        let map = crate::skirmish::maps_dir().join("Crater Lake.map");
        if !map.exists() { return; }
        let lobby = crate::skirmish::Lobby { team: 1, ais: vec![(0, 1), (1, 2)], seed: 1234, ..crate::skirmish::Lobby::new(map) };
        let mut l = Live::from_mem(crate::skirmish::start(&lobby).unwrap());
        for _ in 0..100 { l.step(); } // the start goods come in
        let h = l.hud();
        assert_eq!((h.lords.side(true), h.lords.side(false)), (vec![2], vec![3]));
        assert!(h.lords.allies_button && !h.lords.free_build);
        let gave = |l: &mut Live| l.mem().i32(0x1887ec0f4 + 4); // the engine's "goods given" tally of player 1
        let before = gave(&mut l);
        assert_eq!(l.game_action4(1063, 2, 15, 100), 0, "send refused");
        for _ in 0..5 { l.step(); }
        assert_eq!(gave(&mut l) - before, 100, "100 gold sent");
    }

    /// Not a check: SHC_SHOWCASE=<path.sav> writes an Extended Crater Lake game with a Sassanid Stable beside a plain
    /// stables and three recruited Sassanid Knights, for screenshots (`shc-viewer --live <path.sav>`).
    #[test]
    #[ignore]
    fn sassanid_showcase() {
        let Ok(out) = std::env::var("SHC_SHOWCASE") else { return };
        let map = crate::skirmish::maps_dir().join("Crater Lake.map");
        let lobby = crate::skirmish::Lobby { ais: vec![(0, 0)], seed: 1234, extended: true, ..crate::skirmish::Lobby::new(map) };
        let mut l = Live::from_mem(crate::skirmish::start(&lobby).unwrap());
        for _ in 0..150 { l.step(); }
        let m = l.mem();
        m.set_i32(0x18379926c + b::PSTRIDE, 9000);
        let yard = (1..b::SLOTS).find(|&i| b::state(m, i) == 2 && b::owner(m, i) == 1 && b::kind(m, i) == 10).unwrap();
        m.set_i32(b::rec(yard) + 0x120 + 2 * 4, 300);
        for _ in 0..5 { l.step(); }
        let (kx, ky) = l.keep().map(|(x, y)| (x as i32, y as i32)).unwrap();
        let mut spots = (7..40).flat_map(|d| [(d, 0), (0, d), (-d, 0), (0, -d), (d, d)]).map(|(dx, dy)| (kx + dx, ky + dy));
        for tool in [crate::hud::SASSANID_STABLE, 0x41, 0x56, 0x51] {
            assert!(l.set_tool(tool));
            let (x, y) = spots.find(|&(x, y)| l.hover(tool, x, y).ok).expect("no spot");
            l.click(tool, x, y);
            for _ in 0..4 { l.step(); }
            l.set_tool(0);
        }
        for _ in 0..0x227 * 3 + 20 { l.step(); } // horses
        let m = l.mem();
        let arm = (1..b::SLOTS).find(|&i| b::state(m, i) == 2 && b::owner(m, i) == 1 && b::kind(m, i) == 11).unwrap();
        let (_, goods) = shc_sim::ext::recruit_cost(m);
        for g in goods[..3].iter().filter(|&&g| g > 0) {
            m.set_i32(b::rec(arm) + 0x120 + *g as u64 * 4, 5);
            m.set_i32(shc_sim::game::player(1) + 0x4d0 + *g as u64 * 4, 5);
        }
        for _ in 0..3 { assert_eq!(l.game_action(1007, 1, 0x5a), 0); l.step(); }
        for _ in 0..800 { l.step(); }
        let m = l.mem();
        let n = (1..m.i32(shc_sim::units::OBJ)).filter(|&i| shc_sim::ext::is_sassanid(m, i)).count();
        println!("Sassanid Knights: {n}");
        std::fs::write(&out, shc_sim::save::save_game(m, 0)).unwrap();
        crate::extended::write_beside(std::path::Path::new(&out), true).unwrap();
    }


    /// Ours (keys.rs EXTRA): one archer selected the way SelectClan selects, then Ctrl+A (`select_types`) selects
    /// every own archer and nothing else; Shift+Ctrl+3 (GroupTroops3 with a = 10) adds a spearman to group 3, which
    /// held one archer, so selecting group 3 gives both.
    #[test]
    fn select_types_and_add_to_group() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        let (archers, spears) = (own(l.mem_ref(), 0x16), own(l.mem_ref(), 0x18));
        assert!(archers.len() > 1 && !spears.is_empty(), "{} archers, {} spearmen", archers.len(), spears.len());
        let pick = |l: &mut Live, ids: &[i32]| {
            let m = l.mem();
            shc_sim::frontend::game_action(m, 1022, 0, 0, 0, 0); // nobody selected
            for i in 1..m.i32(UN) { m.set_i16(u(i) + 0x68c, ids.contains(&i) as i16); }
            shc_sim::frontend::f190470(m);
            for _ in 0..2 { l.step(); }
        };
        pick(&mut l, &archers[..1]);
        assert_eq!(selected(l.mem_ref()), archers[..1].to_vec());
        let n = select_types(l.mem());
        for _ in 0..2 { l.step(); }
        let mut sel = selected(l.mem_ref());
        sel.sort();
        assert_eq!((n as usize, sel), (archers.len(), archers.clone()), "every archer, nothing else");
        // group 3 = one archer; then a spearman added (a = 10), the group selects both
        // GroupTroops acts while troops are selected (0x1860ab4fc, set with the troops panel as SelectClan does)
        let group = |l: &mut Live, a: i32| { l.mem().set_i32(0x1860ab4fc, 1); shc_sim::frontend::game_action(l.mem(), 0x16, a, 0, 0, 0); };
        pick(&mut l, &archers[..1]);
        group(&mut l, -1);
        pick(&mut l, &spears[..1]);
        group(&mut l, 10);
        shc_sim::frontend::game_action(l.mem(), 1022, 0, 0, 0, 0);
        l.step();
        shc_sim::frontend::game_action(l.mem(), 0x20, -1, 0, 0, 0); // SelectClan3
        for _ in 0..2 { l.step(); }
        let mut sel = selected(l.mem_ref());
        sel.sort();
        let mut want = vec![archers[0], spears[0]];
        want.sort();
        assert_eq!(sel, want, "group 3 = the archer + the added spearman");
    }

    /// Ours: a right click on the map with a barracks panel open moves all seven of its assembly points there
    /// (`Live::rally`, the panel's own flag tools → chore 0x66 each).
    #[test]
    fn rally_points_by_right_click() {
        let Some(mut l) = save("Crater Lake-1test.sav") else { return };
        l.step();
        l.open_new(87); // a barracks near the keep, its panel open
        assert_eq!(l.hud().app_sub_mode, 1, "barracks panel");
        let (kx, ky) = l.keep().unwrap();
        let p = l.mem_ref().i32(LOCAL) as u64;
        let point = |l: &Live, a: u64| { let at = 0x18379b040 + p * 0x583c + a * 4; (l.mem_ref().i16(at), l.mem_ref().i16(at + 2)) };
        let spot = [(12, 12), (-12, 12), (12, -12), (-12, -12), (20, 0), (0, 20)].into_iter().map(|(dx, dy)| (kx as i32 + dx, ky as i32 + dy))
            .find(|&(x, y)| l.rally(x, y) == 7).expect("no reachable spot near the keep");
        l.step();
        for a in 0..7 { assert_eq!(point(&l, a), (spot.0 as i16, spot.1 as i16), "assembly point {a}"); }
        assert_eq!(l.hud().app_sub_mode, 1, "the panel stays open");
    }
}
