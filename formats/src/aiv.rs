//! AIV castle layouts as fed to `DLL_ImportAIV(lord, id, i16* data, len, custom)`.
//! Format: re/AIV.md. Native reader: `FUN_180054590` (re/decomp/aiv/).
//!
//! ```text
//! [0]            pause_delay           (ticks between build steps; engine uses 100 if <= 0)
//! [1]            n_pause  (<= 50)
//! [2..2+n]       pause step indices    (step 0 always present; rest are 1-based step numbers)
//! [..]           n_steps
//! n_steps ×      kind, pos                      (single tile)
//!            or  -kind, count, pos×count        (multi-tile: walls, moat, stairs)
//! [..]           n_troops
//! n_troops ×     slot, pos, index               (troop/engine standing position `index` of `slot`)
//! ```
//! `pos = y*100 + x` on a 100×100 castle grid; `kind` is an `eMappers` id (Enums.cs:723).

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub kind: i16,
    pub tiles: Vec<(u8, u8)>, // (x, y)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Troop {
    pub slot: i16,  // 0..29, see slot_name
    pub index: i16, // 0..9 within the slot
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Aiv {
    pub pause_delay: i16,
    pub pause_steps: Vec<i16>,
    pub steps: Vec<Step>,
    pub troops: Vec<Troop>,
}

pub const GRID: usize = 100;

fn xy(pos: i16) -> Result<(u8, u8), String> {
    if !(0..10000).contains(&pos) {
        return Err(format!("tile pos {pos} out of 0..10000"));
    }
    Ok(((pos % 100) as u8, (pos / 100) as u8))
}

pub fn parse(d: &[i16]) -> Result<Aiv, String> {
    let mut i = 0;
    let mut next = |what: &str| -> Result<i16, String> {
        let v = *d.get(i).ok_or_else(|| format!("truncated at {what} (offset {i})"))?;
        i += 1;
        Ok(v)
    };
    let pause_delay = next("pause_delay")?;
    let n = next("n_pause")?;
    let pause_steps = (0..n.max(0)).map(|_| next("pause step")).collect::<Result<_, _>>()?;
    let n = next("n_steps")?;
    let mut steps = Vec::with_capacity(n.max(0) as usize);
    for _ in 0..n.max(0) {
        let k = next("step kind")?;
        let (kind, count) = if k < 0 { (-k, next("step count")?) } else { (k, 1) };
        let tiles = (0..count.max(0)).map(|_| xy(next("step pos")?)).collect::<Result<_, _>>()?;
        steps.push(Step { kind, tiles });
    }
    let n = next("n_troops")?;
    let mut troops = Vec::with_capacity(n.max(0) as usize);
    for _ in 0..n.max(0) {
        let slot = next("troop slot")?;
        let (x, y) = xy(next("troop pos")?)?;
        let index = next("troop index")?;
        troops.push(Troop { slot, index, x, y });
    }
    if i != d.len() {
        return Err(format!("{} trailing shorts after troops", d.len() - i));
    }
    Ok(Aiv { pause_delay, pause_steps, steps, troops })
}

/// Little-endian i16 blob (what the recorder stores for the `data` arg) → shorts.
pub fn from_le_bytes(b: &[u8]) -> Vec<i16> {
    b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
}

/// Footprint side length the engine uses when stamping `kind` onto the grid (`FUN_180069400`),
/// or -1 if the table has no entry (then the engine stamps 1 tile: walls, stairs, moat...).
pub fn footprint(kind: i16) -> i32 {
    // Jump-table bytes at 0x1800694c0 for kinds 44..455, then case → side length.
    const IDX: [u8; 412] = *include_bytes!("../data/aiv_size_jumptable.bin");
    const SIDE: [i32; 14] = [2, 4, 3, 5, 2, 6, 1, 7, 11, 9, 10, 13, 0, 8];
    match (kind - 44).try_into().ok().and_then(|i: usize| IDX.get(i)) {
        Some(&c) if (c as usize) < SIDE.len() => SIDE[c as usize],
        _ => -1,
    }
}

impl Aiv {
    /// Grid of `kind` per tile after stamping every step with its footprint, like
    /// `FUN_180054590` does (later steps overwrite earlier). 0 = empty, -1 = reserved
    /// (keep forecourt, barracks drill ground, guild/smelter yard).
    ///
    /// A footprint anchored at (x, y) covers x..x+n-1 and **y-n+1..y**: the engine's grid row
    /// is `99 - y`, and it stamps `n` rows forward from there.
    pub fn grid(&self) -> Vec<[i16; GRID]> {
        let mut g = vec![[0i16; GRID]; GRID];
        let mut put = |x: i32, y: i32, v: i16| {
            if (0..GRID as i32).contains(&x) && (0..GRID as i32).contains(&y) {
                g[y as usize][x as usize] = v;
            }
        };
        for s in &self.steps {
            let n = match footprint(s.kind) {
                n if n > 0 && s.kind != 106 => n, // MOAT is stamped 1×1
                _ => 1,
            };
            for &(x, y) in &s.tiles {
                let (x, y) = (x as i32, y as i32);
                // (dx range, dy range) blocks reserved around the building, dy counted downward in y
                let extra: &[(i32, i32, i32, i32)] = match s.kind {
                    60..=64 => &[(7, 11, 2, 6), (2, 4, 7, 7), (0, 6, 8, 14)], // keep: side yard, door, forecourt
                    79 | 86 | 87 => &[(n, 2 * n - 1, 0, n - 1), (0, 2 * n - 1, n, n)], // barracks drill ground
                    88 | 89 => &[(0, n - 1, n, 2 * n - 1)],                            // guild yard
                    180 => &[(0, n - 1, 4, 4 + n - 1)],                                // oil smelter yard
                    _ => &[],
                };
                for &(x0, x1, y0, y1) in extra {
                    for dy in y0..=y1 {
                        for dx in x0..=x1 {
                            put(x + dx, y - dy, -1);
                        }
                    }
                }
                for dy in 0..n {
                    for dx in 0..n {
                        put(x + dx, y - dy, s.kind);
                    }
                }
            }
        }
        g
    }

    /// ASCII picture of the used part of the grid, one char per tile.
    pub fn render(&self) -> String {
        let g = self.grid();
        let mut tr = vec![[0i16; GRID]; GRID];
        for t in &self.troops {
            tr[t.y as usize][t.x as usize] = t.slot + 1; // +1 so slot 0 shows
        }
        let used = |y: usize, x: usize| g[y][x] != 0 || tr[y][x] != 0;
        let ys: Vec<usize> = (0..GRID).filter(|&y| (0..GRID).any(|x| used(y, x))).collect();
        let xs: Vec<usize> = (0..GRID).filter(|&x| (0..GRID).any(|y| used(y, x))).collect();
        let (Some(&y0), Some(&y1), Some(&x0), Some(&x1)) = (ys.first(), ys.last(), xs.first(), xs.last()) else {
            return String::new();
        };
        let mut out = format!("      x={x0}..{x1}  y={y1}..{y0} top-down (engine row = 99-y); letters = kinds, digits = troop slot (base 36)\n");
        for y in (y0..=y1).rev() {
            out.push_str(&format!("{y:3} "));
            for x in x0..=x1 {
                out.push(if tr[y][x] != 0 { char::from_digit((tr[y][x] - 1) as u32 % 36, 36).unwrap() } else { glyph(g[y][x]) });
            }
            out.push('\n');
        }
        out
    }
}

/// One char per mapper kind for `render`.
pub fn glyph(kind: i16) -> char {
    match kind {
        0 => '.',
        -1 => ':',       // reserved
        25 => '#',       // stone wall
        26 | 35 => '=',  // crenellated wall
        46 => '+',       // wood wall
        106 => '~',      // moat
        60..=64 => 'K',  // keeps
        110..=114 => 'T',
        100..=105 | 140..=147 => 'G', // gatehouses
        181..=186 => '/', // stairs
        27 => '/',
        54 => 'h',       // hovel
        52 => 'S',       // stockpile
        80 => 'g',       // granary
        81 => 'a',       // armoury
        86 | 87 => 'B',  // barracks
        88 => 'E',       // engineers guild
        92 => 'I',       // inn
        77 => 'M',       // market (tradepost)
        70..=73 => 'F',  // farms
        56 | 90 | 91 | 51 | 78 => 'r', // raw-material: quarry, iron, pitch, woodcutter, hunter
        74..=76 | 82..=85 | 50 => 'w', // workshops
        95..=97 => 'C',  // churches
        98 | 99 => 'x',  // killing pit, pitch ditch
        93 => '+',       // apothecary
        330 => 'o',      // well
        313..=324 | 160..=171 | 175..=177 | 301..=312 => '*', // good/bad things
        _ => '?',
    }
}

/// `eMappers` name (Enums.cs:723) for the kinds that occur in AIV data.
pub fn kind_name(kind: i16) -> &'static str {
    match kind {
        25 => "WALL", 26 => "CRENAL", 27 => "STAIR", 35 => "CRENAL2", 44 => "UNDUGMOAT", 45 => "DUGMOAT", 46 => "WOODWALL",
        49 => "OIL", 50 => "FLETCHER", 51 => "WOODSMAN", 52 => "STORES", 53 => "OUTPOST_BEDOUIN",
        54 => "HOVEL", 55 => "OXENBASE", 56 => "QUARRY", 57 => "TUNNEL", 58 => "CAMP_FIRE", 59 => "SIGNPOST",
        60 => "KEEP1", 61 => "KEEP2", 62 => "KEEP3", 63 => "KEEP4", 64 => "KEEP5", 65 => "STABLES",
        70 => "WHEATFARM", 71 => "HOPSFARM", 72 => "APPLEFARM", 73 => "CATTLEFARM", 74 => "MILL", 75 => "BAKER",
        76 => "BREWER", 77 => "TRADEPOST", 78 => "HUNTER", 79 => "BEDOUIN_STOCKADE", 80 => "GRANARY",
        81 => "ARMOURY", 82 => "POLETURNER", 83 => "BLACKSMITH", 84 => "ARMOURER", 85 => "TANNER",
        86 => "BARRACKS_WOOD", 87 => "BARRACKS_STONE", 88 => "ENGINEERS_GUILD", 89 => "TUNNELERS_GUILD",
        90 => "IRON_MINE", 91 => "PITCH_WORKINGS", 92 => "INN", 93 => "HEALER", 94 => "SIEGE_TOWER_BASE",
        95 => "CHURCH1", 96 => "CHURCH2", 97 => "CHURCH3", 98 => "KILLING_PIT", 99 => "PITCH_DITCH",
        100 => "GATEHOUSE", 101 => "GATE_MAIN", 102 => "GATE_INNER", 103 => "GATE_WOOD", 104 => "GATE_POSTERN",
        105 => "DRAWBRIDGE", 106 => "MOAT", 107 => "ANTIMOAT", 109 => "QUARRYPILE",
        110 => "TOWER1", 111 => "TOWER2", 112 => "TOWER3", 113 => "TOWER4", 114 => "TOWER5",
        140 => "GATE_WOOD1A", 141 => "GATE_WOOD1B", 142 => "GATE_WOOD1C", 143 => "GATE_WOOD1D",
        144 => "GATE_STONE1A", 145 => "GATE_STONE1B", 146 => "GATE_STONE2A", 147 => "GATE_STONE2B", 148 => "BRAZIER",
        160..=171 => "GARDEN", 175 => "MAYPOLE", 176 => "GALLOWS", 177 => "STOCKS", 178 => "OUTPOST",
        179 => "OUTPOST_ARAB", 180 => "OIL_SMELTER", 181 => "STAIR1", 182 => "STAIR2", 183 => "STAIR3",
        184 => "STAIR4", 185 => "STAIR5", 186 => "STAIR6", 210 => "MANGONEL", 211 => "BALLISTA",
        301..=304 => "CESS_PIT", 305 => "BURNING_STAKE", 306 => "GIBBET", 307 => "DUNGEON", 308 => "RACK_STRETCHING",
        309 => "RACK_FLOGGING", 310 => "CHOPPING_BLOCK", 311 => "DUNKING_STOOL", 312 => "DOG_CAGE",
        313..=317 => "STATUE", 318..=322 => "SHRINE", 323 => "BEE_HIVE", 324 => "DANCING_BEAR",
        325..=328 => "POND", 329 => "BEAR_CAVE", 330 => "WELL", 342 => "WATERPOT", 358 => "ARAB_BALLISTA",
        _ => "?",
    }
}

/// Troop slot → unit the AI parks there (`DAT_1802ccda0[slot]` = eChimps id, used by
/// `FUN_18002d720`; slots 2-5 are tower engines built by engineers, `DAT_1802ccea0`).
/// Slot 13 doubles as the "ranged troops" slot when the lord config enables it.
pub fn slot_name(slot: i16) -> &'static str {
    match slot {
        1 => "engineer", 2 => "mangonel(tower)", 3 => "ballista(tower)", 4 => "trebuchet(tower)",
        5 => "fire_ballista(tower)", 6 => "archer", 7 => "crossbowman", 8 => "spearman", 9 => "pikeman",
        10 => "maceman", 11 => "swordsman", 12 => "knight", 13 => "arab_slave/ranged", 14 => "slinger",
        15 => "assassin", 16 => "arab_bow", 17 => "horse_archer", 18 => "arab_swordsman", 19 => "fire_thrower",
        22 => "camel_lancer", 23 => "bedouin_healer", 24 => "eunuch", 25 => "ambusher", 26 => "skirmisher",
        27 => "heavy_camel", 28 => "sapper", 29 => "demolisher",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rat1_from_recording() {
        // rec_1792249990.shcrec, `DLL_ImportAIV[1, 0, 507, 0]` = pig[0] (507 shorts); its head and
        // tail as stored in Assembly-CSharp AIVLoader.pig[0] (re/csharp/AIVLoader.cs:1197).
        let head = [100i16, 1, 0, 103, 61, 5643, 87, 4850, 25, 4958, 25, 4959, -25, 4, 4960, 4860, 4760, 4660];
        let a = parse(&[&head[..], &[0, 0][..]].concat()).unwrap_err();
        assert!(a.contains("truncated"), "{a}");
        // A complete tiny layout: keep + a 3-tile wall + 1 troop.
        let d = [100, 2, 0, 2, 2, 61, 5643, -25, 9, 5741, 5742, 5743, 5744, 5745, 5746, 5747, 5748, 5749, 1, 6, 5543, 0];
        let a = parse(&d).unwrap();
        assert_eq!(a.pause_delay, 100);
        assert_eq!(a.pause_steps, vec![0, 2]);
        assert_eq!(a.steps.len(), 2);
        assert_eq!(a.steps[0], Step { kind: 61, tiles: vec![(43, 56)] });
        assert_eq!(a.steps[1].tiles[..3], [(41, 57), (42, 57), (43, 57)]);
        assert_eq!(a.troops, vec![Troop { slot: 6, index: 0, x: 43, y: 55 }]);
        let g = a.grid();
        assert_eq!(footprint(61), 7);
        assert_eq!(g[56][43], 61);
        assert_eq!(g[50][49], 61, "7x7 keep extends toward lower y");
        assert_eq!(g[57][49], 25, "wall row just above the keep anchor");
        assert_eq!(g[49][46], -1, "keep door row reserved");
        assert_eq!(g[52][50], -1, "keep side yard reserved");
        assert_eq!(g[42][43], -1, "keep forecourt reserved");
    }

    #[test]
    fn recorded_blob_round_trip() {
        let p = concat!(env!("HOME"), "/Library/Application Support/CrossOver/Bottles/Steam/drive_c/users/crossover/shc_recordings/rec_1792249990.shcrec");
        let Ok(calls) = crate::shcrec::read(p) else { return }; // recording not on this machine
        let mut n = 0;
        for c in calls.iter().filter(|c| c.name == "DLL_ImportAIV" && c.ints[0] >= 0) {
            let d = from_le_bytes(&c.blobs[0]);
            assert_eq!(d.len(), c.ints[2] as usize);
            let a = parse(&d).unwrap_or_else(|e| panic!("{:?}: {e}", c.ints));
            assert_eq!(a.steps[0].kind, 61, "first step is always the keep");
            assert!(a.steps.iter().all(|s| s.kind > 0 && !s.tiles.is_empty()));
            assert!(a.troops.iter().all(|t| (0..30).contains(&t.slot) && (0..10).contains(&t.index)));
            n += 1;
        }
        assert_eq!(n, 24);
    }
}
