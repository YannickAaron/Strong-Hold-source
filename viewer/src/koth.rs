//! King of the Hill (ours; the skirmish lobby's other game mode next to conquest), on the engine's own King of the
//! Hill. The DLL has that mode's skeleton but nothing that counts (side-session-notes king-of-the-hill.md): the map
//! flag 0x183666168, a reserved neutral "hill player" 0x1837ed8c0 (no lord, no keep people, no goods), a score per
//! player (PlayerRecord +0x229c, the HUD's koth_scores) and game::end_check's win: a present player at 0 wins for its
//! team. On top of that, here:
//! - The hill: a round area in the middle of the map's main land with a square tower in its centre. At the start
//!   the tower belongs to the hill player (a free slot), with archers on it.
//! - Every game second (`SECOND` ticks) a side, a player with its allies (the sim's team table), scores a point when
//!   it has military units on the hill and nobody else has (the hill's guards count as a side that never scores);
//!   contested = nobody scores. Peasants and workers don't count. The engine's score counts down from the target.
//! - The fort: a side alone on the hill for `TAKE` seconds takes it: the old tower (or its ruins) goes, the side's
//!   strongest player there gets a new one on the spot. Its archers on the hill climb onto it (no stairs needed).
//! - The first side to the target wins through end_check. A dead lord still knocks a player out as usual.
//! - The AI lords play it too (`lords`): every `WAVE` ticks they send half their idle defenders to the hill, which
//!   shoot its guards, attack a fort that isn't theirs and hold the hill (the sim's AI doesn't know the mode).
//! The rules read the sim between ticks and act through the engine's own functions (placement, demolition, spawn,
//! teleport, group orders), the same on every LAN peer; the sim's code is untouched. What lives outside the sim
//! memory (the hill, the counts, the capture clock) travels explicitly: a sidecar next to a save (`<save>.hill`), the
//! Go of a LAN hold (lan.rs), a replay's info block and seek snapshots (game_replay.rs; a replay replays the writes,
//! it only counts). ponytail: the hill is always the map's middle.
#![cfg_attr(not(feature = "live"), allow(dead_code))] // without the sim only the drawing builds
use crate::hud::{HudTri, Ui};
use crate::Vert;

/// Ticks per point: a game second at the default speed (40 ticks/s).
pub const SECOND: i32 = 40;
/// The hill's radius in tiles (the tower takes the middle 6×6).
pub const RADIUS: i32 = 8;
/// Game seconds a side must hold the hill alone to take its fort.
pub const TAKE: i32 = 20;
/// The lobby's targets: Short, Normal, Long.
pub const TARGETS: [i32; 3] = [180, 300, 600];
/// The lobby's names for them.
pub const LENGTHS: [&str; 3] = ["Short", "Normal", "Long"];
/// `Hill::holder` while only the hill's guards are on it.
pub const GUARDS: i32 = 9;
/// Ticks between the AI lords' orders to their hill forces, and between the waves they send (`lords`).
pub const ORDERS: i32 = 5 * SECOND;
pub const WAVE: i32 = 30 * SECOND;

/// The mode's state outside the sim. `score` and `holder` are by team id (the sim's team table, 1..8).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hill {
    pub target: i32,
    pub x: i32, // the centre tile (game coordinates); the tower's top-left is (x - 2, y - 2)
    pub y: i32,
    pub score: [i32; 9],
    pub holder: i32, // at the last count: a team, GUARDS, 0 = nobody, -1 = contested
    pub winner: i32, // the team that reached the target, 0 = none yet
    pub last: i32,   // the tick of the last count (a paused redraw or a reload never counts twice)
    pub keeper: i32, // the engine's hill player (0x1837ed8c0): the free slot the neutral fort started with, 0 = none
    pub taking: i32, // the team alone on the hill on its way to the fort, 0 = nobody
    pub since: i32,  // the tick it began
}

impl Hill {
    /// As text: `koth2,target,x,y,holder,winner,last,keeper,taking,since,score 1..8` (the sidecar, a replay's info
    /// line, LAN bytes).
    pub fn encode(&self) -> String {
        let mut v = vec![self.target, self.x, self.y, self.holder, self.winner, self.last, self.keeper, self.taking, self.since];
        v.extend(&self.score[1..]);
        format!("koth2,{}", v.iter().map(i32::to_string).collect::<Vec<_>>().join(","))
    }
    pub fn decode(s: &str) -> Option<Hill> {
        let v: Vec<i32> = s.trim().strip_prefix("koth2,")?.split(',').map(|t| t.parse().ok()).collect::<Option<_>>()?;
        if v.len() != 17 || v[0] <= 0 { return None; }
        let mut score = [0; 9];
        score[1..].copy_from_slice(&v[9..]);
        Some(Hill { target: v[0], x: v[1], y: v[2], holder: v[3], winner: v[4], last: v[5], keeper: v[6], taking: v[7], since: v[8], score })
    }

    /// Optional state as bytes (empty = conquest), for LAN messages and replay files.
    pub fn bytes(h: &Option<Hill>) -> Vec<u8> { h.as_ref().map_or(vec![], |h| h.encode().into_bytes()) }
    pub fn from_bytes(b: &[u8]) -> Option<Hill> { std::str::from_utf8(b).ok().and_then(Hill::decode) }

    /// The sidecar of save `path` (`Crater Lake.sav` -> `Crater Lake.sav.hill`).
    pub fn sidecar(path: &std::path::Path) -> std::path::PathBuf {
        let mut s = path.as_os_str().to_owned();
        s.push(".hill");
        s.into()
    }
    /// Next to a save: written with it; a conquest save removes an old one (a reused name).
    pub fn write_beside(h: &Option<Hill>, path: &std::path::Path) -> std::io::Result<()> {
        match h {
            Some(h) => crate::paths::write_atomic(&Self::sidecar(path), h.encode()),
            None => match std::fs::remove_file(Self::sidecar(path)) { Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e), _ => Ok(()) },
        }
    }
    pub fn read_beside(path: &std::path::Path) -> Option<Hill> { std::fs::read_to_string(Self::sidecar(path)).ok().and_then(|s| Hill::decode(&s)) }

    /// The hill's tiles: those within RADIUS of the centre.
    pub fn on(&self, x: i32, y: i32) -> bool { (x - self.x).pow(2) + (y - self.y).pow(2) <= RADIUS * RADIUS }
    /// The tower's footprint (6×6 from (x − 2, y − 2)).
    pub fn in_fort(&self, x: i32, y: i32) -> bool { (self.x - 2..self.x + 4).contains(&x) && (self.y - 2..self.y + 4).contains(&y) }
}

#[cfg(feature = "live")]
pub use sim::*;

#[cfg(feature = "live")]
mod sim {
    use super::*;
    use shc_sim::{buildings as b, core::TICK_COUNTER, game::player, mem::Mem, units};

    /// The units object: +0 = slots in use + 1; record i at UN + i * 0x490: +0x6e4 state (2 alive), +0x6e6 type,
    /// +0x6ee owner, +0x71c / +0x71e tile x / y, +0x8f8 dead flag (UNITS.md, offsets from the object).
    const UN: u64 = 0x1867e53f0;
    /// eChimps CHIMP_TYPE_LORD.
    const LORD: i32 = 55;
    /// What climbs onto the fort: archers, crossbowmen, Arab archers, slingers.
    const LIFT: [i32; 4] = [22, 23, 70, 72];
    /// The fort's guards at the start (archers) and the most units its top takes.
    const GUARD_COUNT: i32 = 6;
    const SEATS: usize = 8;
    /// MAPPER_TOWER4, the square tower (eStructs 0x4d, 6×6), and its ruins' kind (MAPPER_TOWER4_DESTROYED 0x76).
    const TOWER: i32 = 0x71;
    const TOWER_KIND: i32 = 0x4d;
    const RUINS: i32 = 0x59;
    /// The engine's King of the Hill: the map flag, the hill player, game::end_check's latch (the game is over).
    const KOTH: u64 = 0x183666168;
    const HILL_PLAYER: u64 = 0x1837ed8c0;
    const OVER: u64 = 0x1837ebe84;
    /// Tiles units can't stand on: NAVIGATION.md's blocked mask, buildings and wall tops.
    const BLOCKED: u32 = 0x4a50_14b1 | 0x1000_0100;

    /// game::end_check's `present`: a network human or an AI lord.
    fn present(m: &Mem, p: u64) -> bool { m.i32(0x188571bbc + p * 4) != -1 || m.i32(0x188571c34 + p * 4) != 0 }
    /// Present and not out yet.
    fn playing(m: &Mem, p: u64) -> bool { present(m, p) && m.i32(player(p as u32) + 0x2210) == 0 }
    fn team(m: &Mem, p: u64) -> i32 { m.i32(0x1837ebecc + p * 4) }

    /// Each player's team (the sim's table 0x1837ebecc), 0 = not playing.
    pub fn sides(m: &Mem) -> [i32; 9] {
        std::array::from_fn(|p| if p > 0 && playing(m, p as u64) { team(m, p as u64).clamp(0, 8) } else { 0 })
    }

    /// The building on the hill's centre, if it is the tower or its ruins: (id, owner, kind).
    pub fn fort(m: &Mem, h: &Hill) -> Option<(i32, i32, i32)> {
        let id = units::bld(m, units::tile(m, h.x, h.y)) as i32;
        (id > 0 && matches!(b::state(m, id), 1 | 2) && matches!(b::kind(m, id), TOWER_KIND | RUINS)).then(|| (id, b::owner(m, id), b::kind(m, id)))
    }

    /// One thing a count decides (`Hill::update`) and `apply` does.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Act { Take(i32), Lift(i32, i32, i32) }

    impl Hill {
        /// A new game's hill: the middle of the map's valid tiles, moved to the nearest spot whose 6×6 tower footprint
        /// is open ground of the map's main land (the nav region with the most open tiles: no island or walled-in
        /// pocket). Ties: the lower row, then column, so every peer picks the same.
        pub fn new(m: &Mem, target: i32) -> Hill {
            let (mut lo, mut hi) = ((800, 800), (0, 0));
            let mut regions = std::collections::BTreeMap::new();
            let open = |x: i32, y: i32| units::valid_xy(m, x, y) && units::flags(m, units::tile(m, x, y)) & BLOCKED == 0;
            for y in 0..800 {
                for x in 0..800 {
                    if !units::valid_xy(m, x, y) { continue; }
                    (lo, hi) = ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y)));
                    if open(x, y) { *regions.entry(units::region(m, units::tile(m, x, y))).or_insert(0) += 1; }
                }
            }
            let land = regions.iter().max_by_key(|&(r, n)| (*n, -(*r as i32))).map_or(0, |(&r, _)| r);
            let (cx, cy) = if lo.0 > hi.0 { (400, 400) } else { ((lo.0 + hi.0) / 2, (lo.1 + hi.1) / 2) };
            let ok = |x: i32, y: i32| open(x, y) && units::region(m, units::tile(m, x, y)) == land;
            // ponytail: within 100 tiles of the middle, else the middle itself
            let mut spots: Vec<(i32, i32, i32)> = (cy - 100..=cy + 100).flat_map(|y| (cx - 100..=cx + 100).map(move |x| ((x - cx).pow(2) + (y - cy).pow(2), y, x)))
                .filter(|&(_, y, x)| ok(x, y)).collect();
            spots.sort_unstable();
            let best = spots.iter().find(|&&(_, y, x)| (y - 2..y + 4).all(|ty| (x - 2..x + 4).all(|tx| ok(tx, ty)))).map_or((cx, cy), |&(_, y, x)| (x, y));
            Hill { target, x: best.0, y: best.1, ..Default::default() }
        }

        /// A new game (every LAN peer the same): the hill, the engine's King of the Hill on (the flag, every player's
        /// score at the target), a free slot as the hill player (a team nobody playing has) with the tower and its
        /// guards. No free slot (8 players): no neutral fort, the first side to take the hill gets the tower.
        pub fn begin(m: &mut Mem, target: i32) -> Hill {
            let mut h = Hill::new(m, target);
            h.keeper = (2..9u64).rev().find(|&p| !present(m, p)).map_or(0, |p| p as i32);
            m.set_i32(KOTH, 1);
            m.set_i32(HILL_PLAYER, h.keeper);
            for p in 1..9u64 { if present(m, p) { m.set_i32(player(p as u32) + 0x229c, target); } }
            if h.keeper > 0 {
                let k = h.keeper as u64;
                let used: Vec<i32> = (1..9u64).filter(|&p| present(m, p)).map(|p| team(m, p)).collect();
                if used.contains(&team(m, k)) {
                    if let Some(t) = (1..=8).find(|t| !used.contains(t)) { m.set_i32(0x1837ebecc + k * 4, t); }
                }
                b::place(m, h.keeper, h.x - 2, h.y - 2, TOWER, 6, 0, true);
                for (x, y) in seats(&h).take(GUARD_COUNT as usize) {
                    let z = units::height(m, units::tile(m, x, y)) as i16;
                    units::helpers::spawn(m, h.keeper, h.keeper as i16, (x * 8) as i16, (y * 8) as i16, z, 22);
                }
            }
            h
        }

        /// After a stepping tick: on a game second, count who is on the hill, score, and decide the fort (None = no
        /// count this tick). Reads only: `apply` writes.
        pub fn update(&mut self, m: &Mem) -> Option<Vec<Act>> {
            let tick = m.i32(TICK_COUNTER);
            if self.winner != 0 || tick == self.last || tick % SECOND != 0 || m.i32(OVER) != 0 { return None; }
            self.last = tick;
            let team = sides(m);
            let side = |o: usize| if o > 0 && o as i32 == self.keeper { GUARDS } else { team[o.min(8)] };
            let fort = fort(m, self).filter(|f| f.2 == TOWER_KIND);
            let lifts = fort.is_some_and(|f| team[f.1.clamp(0, 8) as usize] > 0); // a playing player's tower
            let (mut on, mut men, mut seated, mut climb) = ([false; 10], [0; 9], 0, vec![]);
            for i in 1..m.i32(UN).clamp(0, 10_000) {
                let r = UN + i as u64 * 0x490;
                if m.i16(r + 0x6e4) != 2 || m.i16(r + 0x8f8) != 0 { continue; }
                let ty = m.i16(r + 0x6e6) as i32;
                let (o, x, y) = ((m.i16(r + 0x6ee) as usize).min(8), m.i16(r + 0x71c) as i32, m.i16(r + 0x71e) as i32);
                let s = side(o) as usize;
                if !(shc_sim::scenario::military(ty) || ty == LORD) || s == 0 || !self.on(x, y) { continue; }
                (on[s], men[o]) = (true, men[o] + 1);
                if lifts && fort.is_some_and(|f| f.1 == o as i32) {
                    if self.in_fort(x, y) { seated += 1 } else if LIFT.contains(&ty) { climb.push(i) }
                }
            }
            self.holder = match on.iter().filter(|&&b| b).count() { 0 => 0, 1 => on.iter().position(|&b| b).unwrap() as i32, _ => -1 };
            let mut acts = vec![];
            if (1..=8).contains(&self.holder) {
                let s = &mut self.score[self.holder as usize];
                *s += 1;
                if *s >= self.target { self.winner = self.holder; }
            }
            // the fort: a side alone on the hill for TAKE seconds takes it (unless it is its own); its strongest player there
            let theirs = fort.is_some_and(|f| side(f.1.clamp(0, 8) as usize) == self.holder);
            if (1..=8).contains(&self.holder) && !theirs {
                if self.taking != self.holder { (self.taking, self.since) = (self.holder, tick); }
                else if tick - self.since >= TAKE * SECOND {
                    let p = (1..9).filter(|&p| team[p] == self.holder).max_by_key(|&p| (men[p], -(p as i32))).unwrap_or(1);
                    acts.push(Act::Take(p as i32));
                    self.taking = 0;
                }
            } else {
                self.taking = 0;
            }
            // the lift: the tower's owner's archers on the hill climb onto it, while it has room
            let free = seats(self).filter(|&(x, y)| m.i16(units::TILE_HEAD + units::tile(m, x, y) as u64 * 2) == 0);
            for (u, (x, y)) in climb.into_iter().zip(free).take(SEATS.saturating_sub(seated)) { acts.push(Act::Lift(u, x, y)); }
            Some(acts)
        }
    }

    /// The tower's top, inner 4×4, row by row: where guards stand and climbers land.
    fn seats(h: &Hill) -> impl Iterator<Item = (i32, i32)> {
        let (x0, y0) = (h.x - 1, h.y - 1);
        (0..16).map(move |k| (x0 + k % 4, y0 + k / 4))
    }

    /// A count's writes: the engine's score (+0x229c = what each playing player's team still needs; 0 = the team
    /// wins, game::end_check), the fort changing hands (the old tower or its ruins demolished the engine's way, the
    /// new one placed if the footprint is clear), the climbers onto the tower.
    pub fn apply(m: &mut Mem, h: &Hill, acts: &[Act]) {
        for p in 1..9u64 {
            if playing(m, p) { m.set_i32(player(p as u32) + 0x229c, (h.target - h.score[team(m, p).clamp(0, 8) as usize]).max(0)); }
        }
        for &a in acts {
            match a {
                Act::Take(p) => {
                    if let Some((id, _, _)) = fort(m, h) {
                        shc_sim::setup::fc34a0(m, id);
                        b::remove(m, id);
                    }
                    let clear = (h.y - 2..h.y + 4).all(|y| (h.x - 2..h.x + 4).all(|x| units::bld(m, units::tile(m, x, y)) == 0));
                    if clear { b::place(m, p, h.x - 2, h.y - 2, TOWER, 6, 0, true); }
                }
                Act::Lift(u, x, y) => {
                    let z = units::height(m, units::tile(m, x, y)) as i32;
                    units::motion::teleport(m, u, x, y, z);
                }
            }
        }
    }

    /// Live's hook after every RunTick: count and act, and the AI lords' orders (`write`: not while a replay plays,
    /// whose recorded writes do).
    pub fn after_tick(h: &mut Option<Hill>, m: &mut Mem, write: bool) {
        if let Some(h) = h {
            if let Some(acts) = h.update(m) {
                if write {
                    apply(m, h, &acts);
                    if h.winner == 0 && m.i32(TICK_COUNTER) % ORDERS == 0 { lords(m, h); }
                }
            }
        }
    }

    /// Unit groups (ai::groups): +0x34 serial, +0x40 state (2 in use), +0x5a leader, +0x5c size, +0x64c / +0x650
    /// the building an attack order is at (id, serial).
    const GROUPS: u64 = shc_sim::ai::A;
    pub(super) fn grp(g: i32) -> u64 { GROUPS + g as u64 * 0x688 }
    /// Unit i's group while it is one (in use, the serial it joined: +0x930 / +0x93c), 0 = none.
    pub(super) fn group(m: &Mem, i: i32) -> i32 {
        let r = UN + i as u64 * 0x490;
        let g = m.i16(r + 0x930) as i32;
        if g > 0 && m.i16(grp(g) + 0x40) == 2 && m.i32(grp(g) + 0x34) == m.i32(r + 0x93c) { g } else { 0 }
    }
    /// How close (tiles from the hill's centre) a force's leader gets before it fights.
    const FIGHT: i32 = RADIUS + 6;
    /// The troops a lord sends: no lord, engineers, laddermen, tunnelers, siege engines, healers or sappers.
    pub(super) fn fighter(ty: i32) -> bool { matches!(ty, 0x16..=0x1c | 0x25 | 0x46..=0x4c | 0x4e | 0x50..=0x53 | 0x55) }

    /// The AI lords on the hill (the sim's AI doesn't know the mode): every WAVE ticks a lord that isn't under attack
    /// (P+0x38f0) sends half its idle defenders (AI role +0xa82 = 1 in its own squads), the nearest that can walk
    /// there, as two groups (the shooters, LIFT, and the rest; a player's batch: a group's move order leaves members
    /// far from its leader standing, so waves don't merge). They leave their squads for groups outside the lord's
    /// squad slots (P+0x48a0 / +0x4af8), so its defence turns don't call them back, and that is how they are found
    /// again: the force lives in the sim memory (saves, LAN, replays). Every ORDERS ticks each group gets the troop
    /// cursor's orders (FUN_18011da80, chore 0x24's): on its way until its leader is close (an attack from afar
    /// leaves shooters standing); then the shooters at the nearest enemy on the hill (guards, another side's men),
    /// the rest at the fort while it isn't their side's, else at the nearest enemy on the ground there; with nobody
    /// left to fight, onto the hill to hold it. Those on their side's tower stay up there, in a group of their own.
    /// ponytail: a fixed half; an AIC-driven share if lords should differ
    pub fn lords(m: &mut Mem, h: &Hill) {
        let tick = m.i32(TICK_COUNTER);
        let team = sides(m);
        let side = |o: usize| if o > 0 && o as i32 == h.keeper { GUARDS } else { team[o.min(8)] };
        let n = m.i32(UN).clamp(0, 10_000);
        let rec = |i: i32| UN + i as u64 * 0x490;
        let pos = |m: &Mem, i: i32| (m.i16(rec(i) + 0x71c) as i32, m.i16(rec(i) + 0x71e) as i32);
        let live = |m: &Mem, i: i32| m.i16(rec(i) + 0x6e4) == 2 && m.i16(rec(i) + 0x8f8) == 0;
        let shooter = |m: &Mem, i: i32| LIFT.contains(&(m.i16(rec(i) + 0x6e6) as i32));
        let tower = fort(m, h).filter(|f| f.2 == TOWER_KIND);
        // who is on the hill: (unit, side, on the tower)
        let men: Vec<(i32, i32, bool)> = (1..n).filter(|&i| live(m, i)).filter_map(|i| {
            let (ty, (x, y)) = (m.i16(rec(i) + 0x6e6) as i32, pos(m, i));
            ((shc_sim::scenario::military(ty) || ty == LORD) && h.on(x, y)).then(|| (i, side(m.i16(rec(i) + 0x6ee) as usize), h.in_fort(x, y)))
        }).collect();
        // where a force walks to: open ground beside the tower, on each side
        let open = |m: &Mem, x: i32, y: i32| units::valid_xy(m, x, y) && units::flags(m, units::tile(m, x, y)) & BLOCKED == 0;
        let spots: Vec<(i32, i32)> = [(0, 6), (6, 0), (0, -5), (-5, 0)].iter().map(|&(dx, dy)| (h.x + dx, h.y + dy)).filter(|&(x, y)| open(m, x, y)).collect();
        let Some(&(sx, sy)) = spots.first() else { return };
        for p in 1..9usize {
            let pu = p as u64;
            if !playing(m, pu) || m.i32(0x188571c34 + pu * 4) == 0 || m.i32(0x188571bbc + pu * 4) != -1 { continue; } // AI lords
            let (me, pr) = (team[p], player(p as u32));
            let squads: Vec<i32> = (0..300u64).map(|k| (m.i16(pr + 0x48a0 + k * 2) as i32, m.i32(pr + 0x4af8 + k * 4)))
                .filter(|&(g, s)| g > 0 && m.i32(grp(g) + 0x34) == s).map(|(g, _)| g).collect();
            let (mut force, mut home): (Vec<(i32, Vec<i32>)>, Vec<i32>) = (vec![], vec![]); // the force by group
            for i in 1..n {
                let r = rec(i);
                // free: not in a tower or engine (+0xa6b), not frozen (+0x8f6)
                if !live(m, i) || m.i16(r + 0x6ee) as usize != p || !fighter(m.i16(r + 0x6e6) as i32) || m.u8(r + 0xa6b) != 0 || m.i16(r + 0x8f6) == 1 { continue; }
                match group(m, i) {
                    0 => {}
                    _ if m.i16(r + 0xa82) != 1 => {} // a defender (a fresh recruit's group is no force)
                    g if !squads.contains(&g) => match force.iter_mut().find(|f| f.0 == g) { Some(f) => f.1.push(i), None => force.push((g, vec![i])) },
                    _ => home.push(i),
                }
            }
            if tick % WAVE == 0 && m.i32(pr + 0x38f0) < 1 {
                let to = units::region(m, units::tile(m, sx, sy)) as i32;
                let mut reach: Vec<(i32, bool)> = vec![]; // per region
                home.retain(|&i| {
                    let from = units::region(m, m.i32(rec(i) + 0x72c)) as i32;
                    if let Some(&(_, ok)) = reach.iter().find(|r| r.0 == from) { return ok; }
                    let ok = shc_sim::nav::route::route(m, p as i32, from, to, 0) != 0;
                    reach.push((from, ok));
                    ok
                });
                home.sort_by_key(|&i| { let (x, y) = pos(m, i); ((x - h.x).pow(2) + (y - h.y).pow(2), i) });
                if home.len() >= 4 {
                    let (shoot, rest): (Vec<i32>, Vec<i32>) = home[..home.len() / 2].iter().partition(|&&i| shooter(m, i));
                    for us in [shoot, rest] {
                        let g = band(m, p as i32, &us);
                        if g > 0 { force.push((g, us)); }
                    }
                }
            }
            let foe = |s: i32| s != 0 && s != me;
            for (g, us) in force {
                let (up, us): (Vec<i32>, Vec<i32>) = us.iter().partition(|&&i| { let (x, y) = pos(m, i); h.in_fort(x, y) });
                if us.is_empty() { continue; }
                band(m, p as i32, &up); // up on their tower: they stay
                let shooters = shooter(m, us[0]);
                let (lx, ly) = pos(m, m.i16(grp(g) + 0x5a) as i32);
                if (lx - h.x).pow(2) + (ly - h.y).pow(2) <= FIGHT * FIGHT {
                    if let Some((id, _, _)) = tower.filter(|f| !shooters && foe(side(f.1.clamp(0, 8) as usize))) {
                        let serial = m.i32(b::rec(id) + 0xd8);
                        if m.i16(grp(g) + 0x64c) as i32 != id || m.i32(grp(g) + 0x650) != serial { order(m, g, 9, id, serial, 0); }
                        continue;
                    }
                    // the nearest enemy there (the rest: none up on a tower)
                    let near = men.iter().filter(|&&(_, s, top)| foe(s) && (shooters || !top || tower.is_none()))
                        .min_by_key(|&&(i, _, _)| { let (x, y) = pos(m, i); ((x - lx).pow(2) + (y - ly).pow(2), i) });
                    if let Some(&(t, _, _)) = near {
                        let on_it = us.iter().any(|&i| m.i16(rec(i) + 0x9f4) == 4 && m.i16(rec(i) + 0x9f6) as i32 == t); // +0x39c / +0x39e
                        if !on_it { order(m, g, 4, t, m.i32(rec(t) + 0x6f0), 0); }
                        continue;
                    }
                }
                // onto the hill, unless all are there or on their way (walking, +0x74e = 2, to a goal +0x744 on it)
                let away = us.iter().any(|&i| {
                    let (x, y) = pos(m, i);
                    !h.on(x, y) && !(m.i16(rec(i) + 0x74e) == 2 && h.on(m.i16(rec(i) + 0x744) as i32, m.i16(rec(i) + 0x746) as i32))
                });
                if away {
                    let &(x, y) = spots.iter().min_by_key(|&&(x, y)| ((x - lx).pow(2) + (y - ly).pow(2), y, x)).unwrap();
                    order(m, g, 7, x, y, 1000);
                }
            }
        }
    }

    /// Group g's order (chore 0x24's FUN_18011da80), its members' arrived flags (+0x42c) dropped first
    /// (FUN_180125140, as the lord's turns do before sending a squad: a move skips arrived members).
    fn order(m: &mut Mem, g: i32, what: i32, a: i32, b: i32, c: i32) {
        shc_sim::ai::groups::clear_hold(m, g);
        shc_sim::orders::f11da80(m, g, what, a, b, c);
    }

    /// `us` (p's) leave their groups for a new one (FUN_180118e80, FUN_180122fc0, FUN_18011c490: as the lord's
    /// defence turns regroup). 0 = none (no one, or no free group).
    fn band(m: &mut Mem, p: i32, us: &[i32]) -> i32 {
        if us.is_empty() { return 0; }
        let g = shc_sim::ai::groups::new_group_top(m, p);
        if g <= 0 { return 0; }
        for &i in us {
            let cur = m.i16(UN + i as u64 * 0x490 + 0x930) as i32;
            if cur > 0 { shc_sim::units::helpers::group_leave(m, GROUPS, i, cur); }
            shc_sim::ai::groups::join(m, i, g);
        }
        g
    }

    /// The HUD's view: the hill, each player's team (0 = not playing), colours (endgame::radar_colours), names (text
    /// keys of AI lords, else plain names: you, the LAN game's humans by `names`, else "Player n"), the fort's owner
    /// (0 = in ruins / none) and guards left, the tick.
    pub fn board(h: &Hill, m: &Mem, me: i32, names: &[(i32, String)]) -> Board {
        let team = sides(m);
        let names = std::array::from_fn(|p| {
            let kind = m.i32(0x188571c34 + p as u64 * 4);
            match names.iter().find(|n| n.0 == p as i32) {
                _ if p == 0 => String::new(),
                _ if kind != 0 => crate::endgame::name(kind, m.i16(0x188571c58 + p as u64 * 4) as i32),
                Some(n) => n.1.clone(),
                None if p as i32 == me => crate::menu::coa::player_name(),
                None => format!("Player {p}"),
            }
        });
        let guards = (1..m.i32(UN).clamp(0, 10_000)).filter(|&i| {
            let r = UN + i as u64 * 0x490;
            m.i16(r + 0x6e4) == 2 && m.i16(r + 0x8f8) == 0 && h.keeper > 0 && m.i16(r + 0x6ee) as i32 == h.keeper
        }).count() as i32;
        let fort = fort(m, h).filter(|f| f.2 == TOWER_KIND).map_or(0, |f| f.1);
        let t = units::tile(m, h.x, h.y);
        let height = units::helpers::tile_height(m, t) - units::height(m, t) as i32; // the tower's top above the ground
        Board { hill: h.clone(), team, colours: crate::endgame::radar_colours(m), names, fort, guards, height, tick: m.i32(TICK_COUNTER) }
    }
}

/// What the HUD draws (HudState::hill). `fort` = the tower's owner (the hill player while neutral), 0 = none;
/// `height` = its top above the ground (height units, ×2 = world px).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Board { pub hill: Hill, pub team: [i32; 9], pub colours: [u8; 9], pub names: [String; 9], pub fort: i32, pub guards: i32, pub height: i32, pub tick: i32 }

impl Board {
    /// The sides still playing, by their first player: (team, players).
    pub fn sides(&self) -> Vec<(i32, Vec<u8>)> {
        let mut v: Vec<(i32, Vec<u8>)> = vec![];
        for p in 1..9u8 {
            let t = self.team[p as usize];
            if t <= 0 { continue; }
            match v.iter_mut().find(|s| s.0 == t) { Some(s) => s.1.push(p), None => v.push((t, vec![p])) }
        }
        v
    }
    /// A team's colour index: its first player's (endgame::colour), 0 = none.
    pub fn colour(&self, team: i32) -> u8 {
        (1..9u8).find(|&p| self.team[p as usize] == team).map_or(0, |p| crate::endgame::colour(&self.colours, p))
    }
    /// The colour index the flag flies: the tower's owner's, neutral (0) for the hill player or no tower.
    pub fn flag(&self) -> u8 { if self.fort > 0 && self.fort != self.hill.keeper { crate::endgame::colour(&self.colours, self.fort as u8) } else { 0 } }
}

/// The hill on the ground, under the HUD: a ring in the holder's colour (cream while free or only the guards are on
/// it, blinking red and white while contested) and a soft disc while no tower stands; the ring's back is left out
/// behind the tower. `at(x, y)` = the screen point of a game tile position (fractional, on the ground).
pub fn ring(out: &mut Vec<HudTri>, b: &Board, at: impl Fn(f32, f32) -> [f32; 2], t: f32) {
    let h = &b.hill;
    let c = match h.holder {
        -1 => if (t * 3.0) as i32 % 2 == 0 { [1.0, 0.25, 0.2, 1.0] } else { [1.0, 1.0, 1.0, 1.0] },
        0 | GUARDS => [0.937, 0.953, 0.776, 1.0],
        k => crate::endgame::rgb(b.colour(k)),
    };
    let uv = [1.0 / crate::hud::FONT_PX as f32; 2]; // Ui::fill's white texels
    let v = |p: [f32; 2], a: f32| Vert { pos: p, uv, color: [c[0], c[1], c[2], a], team: [0.0; 4] };
    const N: usize = 96;
    let r = RADIUS as f32 + 0.5;
    let pt = |k: usize, r: f32| { let a = k as f32 / N as f32 * std::f32::consts::TAU; at(h.x as f32 + 0.5 + r * a.cos(), h.y as f32 + 0.5 + r * a.sin()) };
    let mid = at(h.x as f32 + 0.5, h.y as f32 + 0.5);
    // the tower's screen span (its footprint's corners): the ring's back half there is behind it
    let corners = [(-2.5, -2.5), (3.5, -2.5), (3.5, 3.5), (-2.5, 3.5)].map(|(dx, dy)| at(h.x as f32 + dx, h.y as f32 + dy)[0]);
    let (lo, hi) = (corners.iter().copied().fold(f32::MAX, f32::min), corners.iter().copied().fold(f32::MIN, f32::max));
    let tower = b.fort > 0;
    for k in 0..N {
        let (a, b2) = (pt(k, r), pt(k + 1, r));
        if !tower { out.push((1, [v(mid, 0.12), v(a, 0.12), v(b2, 0.12)])); }
        if tower && a[1] < mid[1] && (lo..hi).contains(&((a[0] + b2[0]) / 2.0)) { continue; }
        let (ia, ib) = (pt(k, r - 0.35), pt(k + 1, r - 0.35));
        out.push((1, [v(a, 0.8), v(b2, 0.8), v(ib, 0.8)]));
        out.push((1, [v(a, 0.8), v(ib, 0.8), v(ia, 0.8)]));
    }
}

/// The score bar at the top centre (the HUD's scale; the end box's parchment look, endgame.rs): the title and the
/// target, a row per side (its players' shields, its first player's name, the score as a bar in its colour), who
/// holds the hill, and the fort (its owner or guards, and a side's clock to take it).
pub fn bar(ui: &mut Ui, out: &mut Vec<HudTri>, b: &Board, w: f32, h: f32) {
    let s = crate::ux::hud_scale(w, h);
    (ui.s, ui.w, ui.h) = (s, w, h);
    const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    let sides = b.sides();
    let (bw, bh) = (440.0, 88.0 + 26.0 * sides.len() as f32);
    let (x0, y0) = ((w - bw * s) / 2.0, 8.0 * s);
    let r = |x: f32, y: f32, ww: f32, hh: f32| [x0 + x * s, y0 + y * s, x0 + (x + ww) * s, y0 + (y + hh) * s];
    Ui::fill(out, [x0 - 3.0 * s, y0 - 3.0 * s, x0 + (bw + 3.0) * s, y0 + (bh + 3.0) * s], [0.313, 0.248, 0.152, 0.95]);
    ui.img(out, "Panel_BG", r(0.0, 0.0, bw, bh), [1.0, 1.0, 1.0, 0.95]);
    let hill = &b.hill;
    ui.text(out, "King of the Hill", x0 + 14.0 * s, y0 + 24.0 * s, 20.0, 1.0, 0.0, 0.0, BLACK);
    ui.text(out, &format!("first to {}", hill.target), x0 + (bw - 14.0) * s, y0 + 24.0 * s, 16.0, 1.0, 0.0, 1.0, BLACK);
    let names: Vec<String> = b.names.iter().map(|n| ui.text_of(n)).collect(); // AI lords' are text keys
    let name = |p: u8| names[p as usize].clone();
    let side_name = |t: i32| sides.iter().find(|x| x.0 == t).map_or(String::new(), |x| name(x.1[0]));
    for (k, (t, ps)) in sides.iter().enumerate() {
        let y = 34.0 + 26.0 * k as f32;
        for (j, &p) in ps.iter().enumerate().take(4) {
            ui.img_fit(out, crate::endgame::shield(crate::endgame::colour(&b.colours, p)), r(12.0 + 18.0 * j as f32, y, 22.0, 22.0), [1.0; 4]);
        }
        let label = if ps.len() > 1 { format!("{} +{}", name(ps[0]), ps.len() - 1) } else { name(ps[0]) };
        ui.text(out, &label, x0 + (44.0 + 18.0 * (ps.len().min(4) - 1) as f32) * s, y0 + (y + 17.0) * s, 15.0, 1.0, 0.0, 0.0, BLACK);
        let c = crate::endgame::rgb(b.colour(*t));
        let (bx, bwid) = (250.0, 130.0);
        Ui::fill(out, r(bx, y + 5.0, bwid, 13.0), [0.15, 0.1, 0.05, 0.85]);
        let f = (hill.score[*t as usize] as f32 / hill.target.max(1) as f32).clamp(0.0, 1.0);
        Ui::fill(out, r(bx + 1.0, y + 6.0, (bwid - 2.0) * f, 11.0), c);
        if hill.holder == *t { Ui::fill(out, r(bx - 8.0, y + 8.0, 5.0, 7.0), c); } // holds it now
        ui.text(out, &hill.score[*t as usize].to_string(), x0 + (bw - 14.0) * s, y0 + (y + 17.0) * s, 15.0, 1.0, 0.0, 1.0, BLACK);
    }
    let status = match hill.holder {
        -1 => "Contested!".to_string(),
        0 => "Nobody holds the hill".to_string(),
        GUARDS => "The hill's guards hold it".to_string(),
        t => format!("Held by {}", side_name(t)),
    };
    let y = bh - 34.0;
    if hill.holder != 0 {
        let c = match hill.holder { -1 => [0.75, 0.1, 0.05, 1.0], GUARDS => crate::endgame::rgb(0), t => crate::endgame::rgb(b.colour(t)) };
        Ui::fill(out, r(14.0, y - 10.0, 10.0, 10.0), c);
    }
    ui.text(out, &status, x0 + 32.0 * s, y0 + y * s, 15.0, 1.0, 0.0, 0.0, BLACK);
    let fort = match b.fort {
        0 => "Fort: in ruins".to_string(),
        k if k == hill.keeper => format!("Fort: neutral, {} guard{} left", b.guards, if b.guards == 1 { "" } else { "s" }),
        k => format!("Fort: {}'s", name(k as u8)),
    };
    let taking = (hill.taking > 0).then(|| format!("  -  {} takes it in {} s", side_name(hill.taking), (TAKE - (b.tick - hill.since) / SECOND).max(0)));
    ui.text(out, &format!("{fort}{}", taking.unwrap_or_default()), x0 + 32.0 * s, y0 + (bh - 12.0) * s, 15.0, 1.0, 0.0, 0.0, BLACK);
    ui.img_fit(out, crate::endgame::shield(b.flag()), r(12.0, bh - 28.0, 16.0, 16.0), [1.0; 4]);
}

#[test]
fn encode_round_trip() {
    let h = Hill { target: 300, x: 402, y: 397, score: [0, 12, 0, 7, 0, 0, 0, 0, 1], holder: -1, winner: 0, last: 4000, keeper: 8, taking: 3, since: 3960 };
    assert_eq!(Hill::decode(&h.encode()), Some(h.clone()));
    assert_eq!(Hill::from_bytes(&Hill::bytes(&Some(h.clone()))), Some(h));
    assert_eq!((Hill::from_bytes(&[]), Hill::decode("koth2,0,1,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0"), Hill::decode("koth1,1")), (None, None, None));
    assert_eq!(Hill::sidecar(std::path::Path::new("/s/Crater Lake.sav")), std::path::PathBuf::from("/s/Crater Lake.sav.hill"));
    let h = Hill { x: 400, y: 400, ..Default::default() };
    assert!(h.in_fort(398, 398) && h.in_fort(403, 403) && !h.in_fort(397, 400) && !h.in_fort(400, 404));
}

#[cfg(all(test, feature = "live"))]
pub mod tests {
    use super::*;
    use crate::live::Live;
    use shc_sim::{mem::Mem, units};

    /// Crater Lake vs the Rat (player 2), King of the Hill to `target` (the hill player = slot 8 with the fort), a
    /// small camera (fast ticks), first draw done; None = no game install.
    pub fn game(target: i32) -> Option<Live> {
        let map = crate::skirmish::maps_dir().join("Crater Lake.map");
        if !map.exists() { return None; }
        let lobby = crate::skirmish::Lobby { ais: vec![(0, 0)], seed: 1234, hill: target, ..crate::skirmish::Lobby::new(map) };
        let mut l = Live::from_mem(crate::skirmish::start(&lobby).unwrap()).with_hill(lobby.hill);
        l.view = Some([0, 40, 0, 40]);
        l.records();
        Some(l)
    }

    /// `n` units of type `ty` for `owner` around (h.x, h.y) (the engine's spawn, FUN_18017ef30).
    pub fn spawn(m: &mut Mem, h: &Hill, owner: i32, ty: i32, n: usize) -> Vec<i32> {
        (0..n as i32).map(|k| {
            let (x, y) = (h.x + k % 3 - 1, h.y + k / 3 - 1);
            let z = units::height(m, units::tile(m, x, y)) as i16;
            units::helpers::spawn(m, owner, owner as i16, (x * 8) as i16, (y * 8) as i16, z, ty)
        }).collect()
    }

    /// The hill player's units alive.
    pub fn guards(m: &Mem, h: &Hill) -> Vec<i32> {
        (1..m.i32(0x1867e53f0)).filter(|&i| { let r = units::u(i); m.i16(r + 0x8c) == 2 && m.i16(r + 0x2a0) == 0 && m.i16(r + 0x96) as i32 == h.keeper }).collect()
    }

    /// Your units on the tower's footprint (its top).
    fn on_tower(m: &Mem, h: &Hill, us: &[i32]) -> usize {
        us.iter().filter(|&&i| { let r = units::u(i); m.i16(r + 0x8c) == 2 && h.in_fort(m.i16(r + 0xc4) as i32, m.i16(r + 0xc6) as i32) }).count()
    }

    /// Step until the next count (a game second) has run.
    pub fn second(l: &mut Live) { l.step(); while l.tick() % SECOND != 0 { l.step(); } }

    fn hill(l: &Live) -> Hill { l.hill.clone().unwrap() }

    /// A scripted game: the engine's King of the Hill is on (the flag, slot 8 as the hill player, every score at the
    /// target) with the neutral tower and its 6 guards in the middle of the main land; the guards hold the hill (no
    /// score); your archers there contest it (the Rat's peasants don't count); with the guards dead you score and,
    /// alone for TAKE seconds, take the fort: the tower is rebuilt as yours and your archers climb onto it; a Rat
    /// swordsman contests it; then you reach the target and the engine's own King of the Hill check ends the game
    /// (Victory, you win). Nothing counts after the end.
    #[test]
    fn fort_guards_capture_and_win() {
        let Some(mut l) = game(30) else { return };
        let h = hill(&l);
        let m = l.mem_ref();
        assert!(units::valid_xy(m, h.x, h.y) && (h.x - 400).abs() < 80 && (h.y - 400).abs() < 80, "hill at {},{}", h.x, h.y);
        assert_eq!((m.i32(0x183666168), m.i32(0x1837ed8c0), h.keeper), (1, 8, 8), "engine KotH on, slot 8 the hill player");
        assert_eq!(fort(m, &h).map(|f| (f.1, f.2)), Some((8, 0x4d)), "the neutral square tower");
        let (me, rat) = (sides(m)[1], sides(m)[2]);
        let engine = |l: &Live| [1u32, 2].map(|p| l.mem_ref().i32(shc_sim::game::player(p) + 0x229c));
        assert!(me > 0 && rat > 0 && me != rat && engine(&l) == [30, 30], "teams {:?}, engine scores {:?}", sides(m), engine(&l));
        for _ in 0..2 { second(&mut l); }
        let b = board(&hill(&l), l.mem_ref(), 1, &[]);
        assert_eq!((b.hill.holder, b.hill.score, b.fort, b.guards), (GUARDS, [0; 9], 8, 6), "the guards hold it");
        spawn(l.mem(), &Hill { x: h.x - 5, ..h.clone() }, 2, 1, 3); // the Rat's peasants
        let mine = spawn(l.mem(), &Hill { x: h.x - 5, ..h.clone() }, 1, 22, 6); // your archers below the tower
        second(&mut l);
        assert_eq!(hill(&l).holder, -1, "your archers contest the guards");
        for g in guards(l.mem_ref(), &h) { units::helpers::apply_hit(l.mem(), mine[0], g, 10_000_000); }
        second(&mut l);
        let hl = hill(&l);
        assert_eq!((hl.holder, hl.score[me as usize], hl.taking, engine(&l)), (me, 1, me, [29, 30]), "yours: the engine's score counts down");
        let mut n = 0;
        while fort(l.mem_ref(), &h).map(|f| f.1) != Some(1) && n < TAKE + 2 { second(&mut l); n += 1; }
        assert_eq!(fort(l.mem_ref(), &h).map(|f| (f.1, f.2)), Some((1, 0x4d)), "after {n} s alone: your tower");
        assert_eq!(hill(&l).score[me as usize], TAKE + 1);
        second(&mut l);
        assert!(on_tower(l.mem_ref(), &h, &mine) >= 4, "your archers climbed onto it: {}", on_tower(l.mem_ref(), &h, &mine));
        assert_eq!(board(&hill(&l), l.mem_ref(), 1, &[]).flag(), crate::endgame::colour(&crate::endgame::radar_colours(l.mem_ref()), 1), "your flag");
        // a Rat swordsman arrives right before a count: contested, nobody scores
        let before = hill(&l).score[me as usize];
        while (l.tick() + 1) % SECOND != 0 { l.step(); }
        let theirs = spawn(l.mem(), &Hill { x: h.x + 6, ..h.clone() }, 2, 27, 1);
        l.step();
        assert_eq!((hill(&l).holder, hill(&l).score[me as usize], hill(&l).score[rat as usize]), (-1, before, 0), "contested");
        units::helpers::apply_hit(l.mem(), mine[0], theirs[0], 10_000_000);
        let mut n = 0;
        while hill(&l).winner == 0 && n < 40 { second(&mut l); n += 1; }
        let hl = hill(&l);
        assert_eq!((hl.winner, hl.score[me as usize], engine(&l)[0]), (me, 30, 0), "you reached the target");
        for _ in 0..3 { l.step(); }
        let end = crate::endgame::read(l.mem_ref()).expect("the engine's King of the Hill ended the game");
        assert_eq!((end.result, end.winners.iter().map(|w| w.0).collect::<Vec<_>>()), (1, vec![1]), "Victory: you win");
        assert_eq!(l.hud().hill.map(|b| b.hill.winner), Some(me));
        let frozen = l.hill.clone();
        for _ in 0..2 { second(&mut l); }
        assert_eq!(l.hill, frozen, "no counting after the end");
    }

    /// p's defenders (AI role 1) outside its squads (its hill force, `lords`) and in them (at home).
    pub fn force(m: &Mem, p: i32) -> (Vec<i32>, Vec<i32>) {
        let pr = shc_sim::game::player(p as u32);
        let squad = |g: i32| (0..300u64).any(|k| m.i16(pr + 0x48a0 + k * 2) as i32 == g && m.i32(pr + 0x4af8 + k * 4) == m.i32(grp(g) + 0x34));
        (1..m.i32(0x1867e53f0)).filter(|&i| {
            let r = units::u(i);
            m.i16(r + 0x8c) == 2 && m.i16(r + 0x2a0) == 0 && m.i16(r + 0x96) as i32 == p && fighter(m.i16(r + 0x8e) as i32) && m.i16(r + 0x42a) == 1 && group(m, i) > 0
        }).partition(|&i| !squad(group(m, i)))
    }

    /// 9 crossbowmen and 9 macemen for the Rat, on the way from the hill to its keep (P+0x1d8 / +0x1dc).
    fn rat_troops(l: &mut Live, h: &Hill) {
        let pr = shc_sim::game::player(2);
        let (kx, ky) = (l.mem_ref().i32(pr + 0x1d8), l.mem_ref().i32(pr + 0x1dc));
        let at = |f: i32| Hill { x: h.x + (kx - h.x) * f / 10, y: h.y + (ky - h.y) * f / 10, ..h.clone() };
        spawn(l.mem(), &at(6), 2, 23, 9);
        spawn(l.mem(), &at(7), 2, 26, 9);
    }

    /// The AI lords play the hill (`lords`): the Rat's crossbowmen and macemen (spawned towards its keep, put in its
    /// defender squads by its own military turn) are left alone until the first wave, which sends half of them (the
    /// other half stays in its squads) and the next waves the rest of the idle ones, in groups of their own; they walk
    /// to the hill, the crossbowmen shoot the neutral guards off their tower (the macemen attack it), the Rat holds the
    /// hill alone and takes the fort: its tower, its crossbowmen climb onto it, it scores.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "13,000 ticks: cargo test --release")]
    fn ai_takes_the_hill() {
        let Some(mut l) = game(1000) else { return };
        let h = hill(&l);
        rat_troops(&mut l, &h);
        let rat = sides(l.mem_ref())[2];
        while l.tick() < WAVE - SECOND { second(&mut l); }
        let (sent, home) = force(l.mem_ref(), 2);
        assert!(sent.is_empty() && home.len() >= 18 && hill(&l).holder == GUARDS, "before the first wave: {} sent, {} home", sent.len(), home.len());
        second(&mut l);
        let (sent, home) = force(l.mem_ref(), 2);
        assert!(sent.len() >= 9 && home.len() >= sent.len(), "the first wave: {} sent, {} home", sent.len(), home.len());
        while fort(l.mem_ref(), &h).map(|f| f.1) != Some(2) && l.tick() < 16_000 { second(&mut l); }
        let hl = hill(&l);
        assert_eq!(fort(l.mem_ref(), &h).map(|f| (f.1, f.2)), Some((2, 0x4d)), "the Rat's tower at {}: {hl:?}", l.tick());
        assert!(guards(l.mem_ref(), &h).is_empty() && hl.holder == rat && hl.score[rat as usize] > TAKE, "{hl:?}");
        for _ in 0..2 { second(&mut l); }
        let (sent, _) = force(l.mem_ref(), 2);
        assert!(on_tower(l.mem_ref(), &h, &sent) >= 4, "its crossbowmen on its tower: {}", on_tower(l.mem_ref(), &h, &sent));
    }

    /// A save mid-game keeps the mode: the engine's side (flag, hill player, scores, the tower, its guards) is in the
    /// save, the rest in its sidecar; the loaded game counts and takes the fort like the running one.
    #[test]
    fn save_and_load_keep_the_score() {
        let Some(mut l) = game(1000) else { return };
        let h = hill(&l);
        let mine = spawn(l.mem(), &Hill { x: h.x - 5, ..h.clone() }, 1, 22, 6);
        second(&mut l); // the guards (spawned before the first tick) are live now
        for g in guards(l.mem_ref(), &h) { units::helpers::apply_hit(l.mem(), mine[0], g, 10_000_000); }
        for _ in 0..4 { second(&mut l); }
        for _ in 0..7 { l.step(); } // between two counts, on the way to the fort
        let dir = std::env::temp_dir().join(format!("shc-koth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("koth.sav");
        l.save(&path).unwrap();
        let t0 = std::time::Instant::now();
        while (crate::saves::pending(&path) || !path.exists()) && t0.elapsed() < std::time::Duration::from_secs(30) { std::thread::sleep(std::time::Duration::from_millis(20)); }
        let saved = l.hill.clone();
        let me = sides(l.mem_ref())[1] as usize;
        assert!(saved.as_ref().is_some_and(|s| s.score[me] == 4 && s.taking == me as i32), "{saved:?}");
        let mut back = Live::load(path.to_str().unwrap());
        back.hill = Hill::read_beside(&path);
        back.view = Some([0, 40, 0, 40]);
        back.records();
        assert_eq!((&back.hill, back.tick()), (&saved, l.tick()), "loaded with the score");
        let m = back.mem_ref();
        assert_eq!((m.i32(0x183666168), m.i32(0x1837ed8c0), m.i32(shc_sim::game::player(1) + 0x229c)), (1, 8, 996), "the engine's side in the save");
        for _ in 0..TAKE { second(&mut l); second(&mut back); }
        assert_eq!(back.hill.as_ref().unwrap().score[me], 4 + TAKE);
        assert_eq!(back.hill, l.hill, "counts on like the running game");
        assert_eq!(fort(back.mem_ref(), &h).map(|f| f.1), Some(1), "and takes the fort");
        back.hill = None;
        back.save(&path).unwrap(); // a conquest save over it drops the sidecar
        assert!(!Hill::sidecar(&path).exists());
        let t0 = std::time::Instant::now();
        while crate::saves::pending(&path) && t0.elapsed() < std::time::Duration::from_secs(30) { std::thread::sleep(std::time::Duration::from_millis(20)); }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A replay of a King of the Hill game (through its file) counts the same in playback, its recorded writes take
    /// the fort and end the game the same (no desync), and a seek (from a snapshot past the start) has the state of
    /// that frame.
    #[test]
    #[cfg_attr(debug_assertions, ignore = "2,400 ticks: cargo test --release")]
    fn replay_counts_the_same() {
        use crate::game_replay::{restore, Player, Replay, Tap, SNAP_EVERY};
        let Some(mut l) = game(55) else { return };
        let h = hill(&l);
        let mine = spawn(l.mem(), &Hill { x: h.x - 5, ..h.clone() }, 1, 22, 6);
        let mut hills = vec![]; // the state after every frame
        while l.tick() % SECOND != 0 || hills.is_empty() { l.step(); hills.push(l.hill.clone()); } // the guards live
        for g in guards(l.mem_ref(), &h) { units::helpers::apply_hit(l.mem(), mine[0], g, 10_000_000); }
        while hill(&l).winner == 0 && l.tick() < 5000 { l.step(); hills.push(l.hill.clone()); }
        for _ in 0..3 { l.step(); hills.push(l.hill.clone()); }
        assert!(crate::endgame::read(l.mem_ref()).is_some_and(|e| e.result == 1), "won");
        assert_eq!(fort(l.mem_ref(), &h).map(|f| f.1), Some(1), "the fort taken");
        let m = l.mem_ref().clone();
        let Some(Tap::Rec(r)) = &mut l.tap else { panic!() };
        let rep = r.replay(&m).unwrap();
        assert!(rep.frames.len() > SNAP_EVERY + 100, "{} frames", rep.frames.len());
        let path = std::env::temp_dir().join(format!("shc-koth-{}.shcreplay", std::process::id()));
        rep.write(&path).unwrap();
        let rep = Replay::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(rep.hill, Some(h.clone()), "the start's hill in the file");
        let mut p = Player::new(rep);
        let mut mm = restore(&p.replay.start);
        let mut ret = vec![0; shc_sim::hudstate::RET_LEN];
        let first = 1; // frame 0 = the first draw (no tick)
        while p.run(&mut mm, &mut ret) {
            if p.at > first { assert_eq!(p.hill, hills[p.at - first - 1], "frame {}", p.at); }
        }
        assert_eq!((p.desync, &p.hill), (None, &l.hill));
        assert_eq!(fort(&mm, &h).map(|f| f.1), Some(1), "the replay takes the fort");
        assert!(crate::endgame::read(&mm).is_some_and(|e| e.result == 1), "the replay ends the same");
        let to = SNAP_EVERY + 50;
        let (_, q) = p.seek(to, Default::default());
        assert_eq!(q.hill, hills[to - first - 1], "seek from the snapshot at {SNAP_EVERY}");
    }

    /// SHC_SAVES=<dir> (screenshots): `Hill.sav` (the neutral fort and its guards against your archers, contested),
    /// `HillTaken.sav` (your tower with your archers on it, the Rat's men close by), `HillWon.sav` (to 25: just won)
    /// and `HillAI.sav` (the Rat's waves fighting the guards), with their sidecars; prints SHC_FOCUS for the hill.
    #[test]
    #[ignore]
    fn write_hill_saves() {
        if std::env::var_os("SHC_SAVES").is_none() { return; }
        let wait = |p: &std::path::Path| while crate::saves::pending(p) || !p.exists() { std::thread::sleep(std::time::Duration::from_millis(20)); };
        for (target, name) in [(300, "Hill.sav"), (300, "HillTaken.sav"), (25, "HillWon.sav")] {
            let Some(mut l) = game(target) else { return };
            let h = hill(&l);
            let mine = spawn(l.mem(), &Hill { x: h.x - 5, ..h.clone() }, 1, 22, 6);
            spawn(l.mem(), &Hill { x: h.x - 5, y: h.y + 4, ..h.clone() }, 1, 24, 4);
            second(&mut l);
            if name != "Hill.sav" {
                for g in guards(l.mem_ref(), &h) { units::helpers::apply_hit(l.mem(), mine[0], g, 10_000_000); }
                while fort(l.mem_ref(), &h).map(|f| f.1) != Some(1) { second(&mut l); }
                for _ in 0..2 { second(&mut l); }
                spawn(l.mem(), &Hill { x: h.x + 12, ..h.clone() }, 2, 26, 3);
                while name == "HillWon.sav" && hill(&l).winner == 0 { second(&mut l); }
            }
            for _ in 0..3 { l.step(); }
            let path = crate::skirmish::saves_dir().join(name);
            l.save(&path).unwrap();
            wait(&path);
            let (wx, wy) = crate::replay::to_west_pub(l.rotation(), h.x as i16, h.y as i16);
            println!("{name}: {:?}, SHC_FOCUS={wx},{wy}", l.hill);
        }
        // HillAI.sav: the Rat's waves at the hill (`lords`), shooting the guards off the tower, two of them down
        let Some(mut l) = game(300) else { return };
        let h = hill(&l);
        rat_troops(&mut l, &h);
        second(&mut l); // the guards live
        while guards(l.mem_ref(), &h).len() > 4 && l.tick() < 16_000 { second(&mut l); }
        let path = crate::skirmish::saves_dir().join("HillAI.sav");
        l.save(&path).unwrap();
        wait(&path);
        let (wx, wy) = crate::replay::to_west_pub(l.rotation(), h.x as i16, (h.y + 4) as i16);
        println!("HillAI.sav: tick {} {:?}, SHC_FOCUS={wx},{wy}", l.tick(), l.hill);
    }
}
