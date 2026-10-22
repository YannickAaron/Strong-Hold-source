//! Game statistics over time, like a modern RTS's end-of-game graphs (the original has none): gold, population,
//! food, troops and buildings per player, sampled read-only from sim memory on the sim thread (`live::Thread`,
//! every `EVERY` ticks). Drawn as an in-game overlay (F3) and under the Victory / Defeat box. Nothing here writes
//! engine memory: `sample` only gets `&Mem`.
use crate::hud::{HudTri, Ui};

/// Ticks per sample: 16 per game month (800 ticks), the engine's FLAG_50 rhythm.
pub const EVERY: i32 = 50;
pub const NAMES: [&str; 5] = ["Gold", "Population", "Food", "Troops", "Buildings"];

/// The samples so far: (tick, per player 1..=8 the five values in `NAMES` order); player names as text keys; the
/// players' colours (`replay::colours`, 0 = the default).
#[derive(Clone, Default, Debug)]
pub struct History { pub at: Vec<(i32, [[i32; 5]; 8])>, pub names: [String; 8], pub colours: [u8; 9] }

impl History {
    /// Players (0-based) with anything to show: population, troops or buildings above 0 at some point.
    pub fn players(&self) -> Vec<usize> { (0..8).filter(|&p| self.at.iter().any(|(_, v)| v[p][1] + v[p][3] + v[p][4] > 0)).collect() }
}

/// FUN_180193a90(U, type): the military unit types (same table as sim scenario::military, which is private).
#[cfg(feature = "live")]
fn military(ty: i16) -> bool { matches!(ty, 5 | 0x16..=0x1e | 0x25 | 0x27..=0x29 | 0x3a..=0x3d | 0x46..=0x55) }

/// One sample when `EVERY` ticks passed since the last (a load / restart that goes back in time starts over).
/// Gold = player record +0x50c, population +0x2180, food = the four food stocks +0x209c; troops = alive units
/// (+0x8c state 2) of a military type (+0x8e) by owner (+0x96); buildings = used building slots by owner.
#[cfg(feature = "live")]
pub fn sample(m: &shc_sim::mem::Mem, h: &mut History) {
    use shc_sim::{buildings as b, game as g, units as u};
    let tick = m.i32(shc_sim::core::TICK_COUNTER);
    if h.at.last().is_some_and(|&(t, _)| t > tick) { h.at.clear(); }
    if h.at.last().is_some_and(|&(t, _)| tick - t < EVERY) { return; }
    let mut v = [[0i32; 5]; 8];
    for (p, row) in v.iter_mut().enumerate() {
        let r = g::player(p as u32 + 1);
        (row[0], row[1], row[2]) = (m.i32(r + g::P_GOLD), m.i32(r + g::P_POPULATION), m.i32(r + g::P_FOOD_TOTAL));
    }
    for i in 1..m.i32(u::OBJ).clamp(0, 10000) {
        let r = u::u(i);
        let o = m.i16(r + 0x96) as usize;
        if m.i16(r + 0x8c) == 2 && military(m.i16(r + 0x8e)) && (1..=8).contains(&o) { v[o - 1][3] += 1; }
    }
    for i in 1..b::SLOTS {
        let o = b::owner(m, i) as usize;
        if b::state(m, i) != 0 && (1..=8).contains(&o) { v[o - 1][4] += 1; }
    }
    for p in 0..8u64 { h.names[p as usize] = crate::endgame::name(m.i32(g::MP_LORD_ID + (p + 1) * 4), m.i16(0x188571c58 + (p + 1) * 4) as i32); }
    h.colours = crate::replay::colours(m);
    h.at.push((tick, v));
}

/// The graph panel: a tab per value (click switches), one line per player in team colour, the value axis from 0,
/// the time axis in game months, a legend with each player's latest value. `on`: F3 (SHC_STATS=1: on from the start,
/// for scripted screenshots); `end_seen`: the game end turned it on once.
#[derive(Default)]
pub struct Panel { pub on: bool, pub kind: usize, hits: Vec<([f32; 4], usize)>, area: [f32; 4], end_seen: bool, env: bool }

/// RedrawRequested, after the end screen: the panel in a live game while F3 has it on; a game end turns it on once
/// (under the Victory / Defeat box; F3 hides it again).
pub fn overlay(p: &mut Panel, ui: Option<&mut Ui>, play: &Option<crate::Play>, menu_open: bool, g: &crate::Gfx, cursor: [f32; 2], hud: Vec<HudTri>) -> Vec<HudTri> {
    let (Some(ui), Some(pl)) = (ui, play.as_ref()) else { p.hits.clear(); p.area = [0.0; 4]; return hud };
    let Some(live) = pl.live.as_ref() else { return hud };
    if !p.env { (p.env, p.on) = (true, p.on || std::env::var_os("SHC_STATS").is_some()); }
    let end = pl.huds.get(pl.at.saturating_sub(1)).is_some_and(|h| h.end.is_some());
    if end && !p.end_seen { p.on = true; }
    p.end_seen = end;
    if !p.on || menu_open { p.hits.clear(); p.area = [0.0; 4]; return hud; }
    let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
    let h = live.stats.lock().unwrap();
    [hud, p.draw(&h, ui, sz.width, sz.height, cursor, end)].concat()
}

impl Panel {
    /// Triangles for this frame: centred over the map (`end` = false), or under the end screen's box.
    pub fn draw(&mut self, h: &History, ui: &mut Ui, w: f32, hh: f32, cursor: [f32; 2], end: bool) -> Vec<HudTri> {
        self.hits.clear();
        let s = crate::ux::hud_scale(w, hh); // the HUD's scale: another ui.s makes Ui::draw drop its glyphs every frame
        (ui.s, ui.w, ui.h) = (s, w, hh);
        let (pw, ph) = (1000.0 * s, if end { (hh / 2.0 - 140.0 * s).clamp(260.0 * s, 420.0 * s) } else { 440.0 * s });
        let x0 = (w - pw) / 2.0;
        let y0 = if end { hh - ph - 24.0 * s } else { (hh - ph) / 2.0 - 60.0 * s };
        let mut out = vec![];
        self.area = [x0, y0, x0 + pw, y0 + ph];
        Ui::fill(&mut out, [x0 - 3.0 * s, y0 - 3.0 * s, x0 + pw + 3.0 * s, y0 + ph + 3.0 * s], [0.313, 0.248, 0.152, 0.95]);
        ui.img(&mut out, "Panel_BG", [x0, y0, x0 + pw, y0 + ph], [1.0, 1.0, 1.0, 0.95]);
        // tabs
        let tw = pw / NAMES.len() as f32;
        for (k, name) in NAMES.iter().enumerate() {
            let r = [x0 + k as f32 * tw + 4.0 * s, y0 + 8.0 * s, x0 + (k + 1) as f32 * tw - 4.0 * s, y0 + 40.0 * s];
            let hot = inside(r, cursor);
            let bg = if k == self.kind { [0.767, 0.680, 0.461, 0.95] } else if hot { [0.634, 0.556, 0.410, 0.95] } else { [0.461, 0.396, 0.313, 0.9] };
            Ui::fill(&mut out, r, bg);
            ui.text_sh(&mut out, name, (r[0] + r[2]) / 2.0, r[3] - 9.0 * s, 20.0, 0.5, [0xef as f32 / 255.0, 0xf3 as f32 / 255.0, 0xc6 as f32 / 255.0, 1.0]);
            self.hits.push((r, k));
        }
        // plot area: left value labels, right legend
        let (gx0, gy0, gx1, gy1) = (x0 + 70.0 * s, y0 + 60.0 * s, x0 + pw - 230.0 * s, y0 + ph - 36.0 * s);
        Ui::fill(&mut out, [gx0, gy0, gx1, gy1], [0.0, 0.0, 0.0, 0.25]);
        let dark = [0.381, 0.313, 0.221, 1.0];
        let players = h.players();
        let k = self.kind;
        let top = h.at.iter().flat_map(|(_, v)| players.iter().map(move |&p| v[p][k])).max().unwrap_or(0).max(1);
        let top = nice(top);
        for i in 0..=4 {
            let y = gy1 - (gy1 - gy0) * i as f32 / 4.0;
            Ui::fill(&mut out, [gx0, y - 0.5 * s, gx1, y + 0.5 * s], [0.0, 0.0, 0.0, 0.3]);
            ui.text(&mut out, &(top * i / 4).to_string(), gx0 - 8.0 * s, y + 6.0 * s, 16.0, 1.0, 0.0, 1.0, dark);
        }
        let (t0, t1) = match (h.at.first(), h.at.last()) { (Some(a), Some(b)) => (a.0, b.0.max(a.0 + 1)), _ => (0, 1) };
        let months = (t1 - t0) as f32 / 800.0;
        let step = [1.0, 2.0, 3.0, 6.0, 12.0, 24.0, 60.0].into_iter().find(|&m| months / m <= 8.0).unwrap_or(120.0);
        let mut m = 0.0;
        while m <= months {
            let x = gx0 + (gx1 - gx0) * m / months.max(1e-3);
            ui.text(&mut out, &format!("{}", m as i32), x, gy1 + 20.0 * s, 16.0, 1.0, 0.0, 0.5, dark);
            m += step;
        }
        ui.text(&mut out, "months", gx1, gy1 + 20.0 * s, 14.0, 1.0, 0.0, 1.0, dark);
        // lines, at most one point per 2 px
        let n = h.at.len();
        let every = (n as f32 / ((gx1 - gx0) / 2.0)).ceil().max(1.0) as usize;
        let at = |i: usize, p: usize| {
            let (t, v) = &h.at[i];
            [gx0 + (gx1 - gx0) * (t - t0) as f32 / (t1 - t0) as f32, gy1 - (gy1 - gy0) * v[p][k] as f32 / top as f32]
        };
        for (row, &p) in players.iter().enumerate() {
            let c = crate::endgame::rgb(crate::endgame::colour(&h.colours, p as u8 + 1));
            let idx: Vec<usize> = (0..n).step_by(every).chain((n > 0 && (n - 1) % every != 0).then_some(n - 1)).collect();
            for pair in idx.windows(2) { seg(&mut out, at(pair[0], p), at(pair[1], p), 2.5 * s, c); }
            let ly = gy0 + 18.0 * s + row as f32 * 28.0 * s;
            Ui::fill(&mut out, [gx1 + 16.0 * s, ly - 10.0 * s, gx1 + 30.0 * s, ly + 2.0 * s], c);
            let name = ui.text_of(&h.names[p]);
            let name: String = name.chars().take(18).collect();
            ui.text(&mut out, &name, gx1 + 38.0 * s, ly, 17.0, 1.0, 0.0, 0.0, dark);
            let last = h.at.last().map_or(0, |(_, v)| v[p][k]);
            ui.text(&mut out, &last.to_string(), x0 + pw - 12.0 * s, ly, 17.0, 1.0, 0.0, 1.0, dark);
        }
        if n < 2 { ui.text(&mut out, "Collecting data...", (gx0 + gx1) / 2.0, (gy0 + gy1) / 2.0, 20.0, 1.0, 0.0, 0.5, dark); }
        out
    }

    /// The cursor is on the panel (last frame's layout): clicks there don't reach the map.
    pub fn over(&self, c: [f32; 2]) -> bool { inside(self.area, c) }
    /// Left press on a tab: show that value.
    pub fn click(&mut self, c: [f32; 2]) -> bool {
        let hit = self.hits.iter().find(|(r, _)| inside(*r, c)).map(|&(_, k)| k);
        if let Some(k) = hit { self.kind = k; }
        hit.is_some()
    }
}

fn inside(r: [f32; 4], c: [f32; 2]) -> bool { c[0] >= r[0] && c[0] < r[2] && c[1] >= r[1] && c[1] < r[3] }

/// Axis top: the first 1, 2, 3, 4, 5, 6 or 8 × 10^n at or above `v`.
fn nice(v: i32) -> i32 {
    (0..9).flat_map(|e| [1, 2, 3, 4, 5, 6, 8].map(|m| m * 10i32.pow(e))).find(|&t| t >= v).unwrap_or(i32::MAX)
}

/// A `w` px wide line from `a` to `b` (two triangles on the font atlas's white block, like `Ui::fill`).
pub(crate) fn seg(out: &mut Vec<HudTri>, a: [f32; 2], b: [f32; 2], w: f32, color: [f32; 4]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (nx, ny) = (-dy / l * w / 2.0, dx / l * w / 2.0);
    let t = 1.0 / crate::hud::FONT_PX as f32;
    let v = |x: f32, y: f32| crate::Vert { pos: [x, y], uv: [t, t], color, team: [0.0; 4] };
    let (p0, p1, p2, p3) = (v(a[0] + nx, a[1] + ny), v(b[0] + nx, b[1] + ny), v(a[0] - nx, a[1] - ny), v(b[0] - nx, b[1] - ny));
    out.push((1, [p0, p1, p2]));
    out.push((1, [p1, p3, p2]));
}

#[test]
fn nice_axis() {
    assert_eq!([nice(1), nice(3), nice(7), nice(24), nice(240), nice(1588), nice(2050)], [1, 3, 8, 30, 300, 2000, 3000]);
}

/// A fresh skirmish (you vs the Rat) for 400 ticks: a sample every 50 ticks, both players have buildings and gold,
/// and sampling leaves the sim state alone (same RNG and tick as a run without it).
#[cfg(feature = "live")]
#[test]
fn samples_skirmish() {
    use crate::skirmish::{maps_dir, start, Lobby};
    use shc_sim::frontend;
    let map = maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    let lobby = Lobby { ais: vec![(0, 0)], seed: 99, ..Lobby::new(map) };
    let (mut a, mut b) = (start(&lobby).unwrap(), start(&lobby).unwrap());
    let mut ret = vec![0u8; shc_sim::hudstate::RET_LEN];
    let mut h = History::default();
    for _ in 0..400 {
        frontend::run_tick(&mut a, &frontend::TickInput::default(), &mut ret, None);
        frontend::run_tick(&mut b, &frontend::TickInput::default(), &mut ret, None);
        sample(&a, &mut h);
    }
    println!("{} samples, last {:?}, names {:?}", h.at.len(), h.at.last(), &h.names[..2]);
    assert_eq!(h.at.len(), 8);
    assert_eq!(h.players(), [0, 1]);
    let last = h.at.last().unwrap().1;
    assert!(last[0][4] > 0 && last[1][4] > 0 && last[0][0] > 0, "{last:?}");
    assert_eq!((a.i32(shc_sim::units::RNG), a.i32(shc_sim::core::TICK_COUNTER)), (b.i32(shc_sim::units::RNG), b.i32(shc_sim::core::TICK_COUNTER)));
    assert!(a.slice(0x18366a1a0, 0x400000) == b.slice(0x18366a1a0, 0x400000), "game state differs after sampling");
}
