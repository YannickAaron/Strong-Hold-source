//! `--replay rec.shcrec`: play back the per-tick draw lists of a recorder log (format: recorder/README.md).
//! Draw list semantics: re/CSHARP_NOTES.md §2. The list is incremental: type 0 tiles are dirty-only,
//! objects (1 org, 2 chimp, 4 building anim) are keyed by objectID and `file == 0` deletes.
use std::collections::{BTreeMap, HashMap};
use std::io::Read;

pub type Rec = [i16; 21];
pub const N: usize = 800; // world is 800×800 game tiles
/// Synthetic record type the loader inserts for `DLL_SetMapRotation(rot, ..)`; `r[1]` = engine rotation.
/// Live also sends `r[2..11]` = the players' colours (`colours`; all 0 = unknown, e.g. a recording: kept as they were).
pub const ROTATION: i16 = -100;

/// SpriteMapping.remapColours: sprite colour (player) 0..8 -> colour index into the GM palettes (1 red, 2 blue, ...).
/// Skirmish / MP / trails (game mode 3): what BuildMultiPlayerColourMapping (or a trail's BuildPlayerColourMapping)
/// hands DLL_SetMPRadarColours, the engine's radar colours (skirmish::RADAR_COLOURS, in saves too); else the map's
/// colour table (SetRemapColours of DLL_GetColourMapping, 0x1802c1ce0) with BuildPlayerColourMapping's swap of
/// player 1 and your colour id (DLL_GetColourMapping's `remapped`, 0x1834941f8). From engine memory, so loaded saves
/// and LAN games get theirs.
#[cfg(feature = "live")]
pub fn colours(m: &shc_sim::mem::Mem) -> [u8; 9] {
    if m.i32(0x183663eb8) == 3 { return crate::endgame::radar_colours(m); }
    let mut c: [u8; 9] = std::array::from_fn(|k| m.i32(0x1802c1ce0 + k as u64 * 4).clamp(0, 8) as u8);
    if c[1] == 0 { return IDENTITY; } // no table (nothing loaded)
    let you = m.i32(0x1834941f8);
    if (2..=8).contains(&you) { c.swap(1, you as usize); }
    c
}
pub const IDENTITY: [u8; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 8];

/// Engine rotation (C# `Enums.Dircs` − 1: 0 N, 2 E, 4 S, 6 W) -> game coords in the West frame this viewer's
/// projection was tuned on. = W⁻¹ ∘ R, with R from GameMap.mapGameTileToTilemapCoord (±1 offsets ignored).
fn to_west(rot: i16, x: i16, y: i16) -> (i16, i16) {
    const S: i16 = N as i16 - 1;
    match rot { 0 => (y, S - x), 2 => (S - x, S - y), 4 => (S - y, x), _ => (x, y) }
}

pub fn to_west_pub(rot: i16, x: i16, y: i16) -> (i16, i16) { to_west(rot, x, y) }

pub fn from_west(rot: i16, x: i16, y: i16) -> (i16, i16) {
    const S: i16 = N as i16 - 1;
    match rot { 0 => (S - y, x), 2 => (S - x, S - y), 4 => (y, S - x), _ => (x, y) }
}

/// All `DLL_RunTick` draw lists of a log, in tick order, plus the HUD state the engine returned with each
/// (blob 1 = `PlayStateReturnData`, blob 2 = selectedChimps; default if missing). Reader mirrors recorder/src/bin/dump.rs.
pub fn load(path: &str) -> (Vec<Vec<Rec>>, Vec<crate::hud::HudState>) {
    let f = std::fs::File::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut r = lz4_flex::frame::FrameDecoder::new(std::io::BufReader::new(f));
    let u32le = |b: &[u8]| u32::from_le_bytes(b[..4].try_into().unwrap());
    let mut h = [0u8; 12];
    r.read_exact(&mut h).unwrap();
    assert!(&h[..8] == b"SHCREC01" || &h[..8] == b"SHCREC02", "not a recorder log"); // v2 adds per-tick state hashes (blob 3)
    let mut calls = vec![];
    for _ in 0..u32le(&h[8..]) {
        let mut n = [0u8; 1];
        r.read_exact(&mut n).unwrap();
        let mut name = vec![0; n[0] as usize + 2];
        r.read_exact(&mut name).unwrap();
        let (ns, ret) = (name[n[0] as usize], name[n[0] as usize + 1]);
        name.truncate(n[0] as usize);
        calls.push((String::from_utf8_lossy(&name).into_owned(), ns as usize + ret as usize));
    }
    if &h[..8] == b"SHCREC02" {
        // skip_v2_sections: u32 n + n × {i16 type_id, u64 addr, u32 size} (recorder/README.md)
        let mut n = [0u8; 4];
        r.read_exact(&mut n).unwrap();
        let mut skip = vec![0u8; u32le(&n) as usize * 14];
        r.read_exact(&mut skip).unwrap();
    }
    let (mut ticks, mut huds) = (vec![], vec![]);
    let mut pending: Vec<Rec> = vec![];
    let mut h = [0u8; 9];
    // a truncated tail (game killed mid-flush) just ends the log
    while r.read_exact(&mut h).is_ok() {
        let mut p = vec![0; u32le(&h[5..]) as usize];
        if r.read_exact(&mut p).is_err() { break; }
        let (name, ints) = &calls[h[0] as usize];
        if name == "DLL_SetMapRotation" {
            let mut m = [0i16; 21];
            (m[0], m[1]) = (ROTATION, u32le(&p) as i16);
            pending.push(m);
            continue;
        }
        if name != "DLL_RunTick" { continue; }
        let at = ints * 4; // blob 0 = draw list: u32 len + n × 21 i16
        let end = at + 4 + u32le(&p[at..]) as usize;
        let draw = &p[at + 4..end];
        // blobs 1 (PlayStateReturnData) and 2 (selectedChimps) follow; missing = empty
        let blob = |at: usize| -> (&[u8], usize) {
            let Some(n) = p.get(at..at + 4) else { return (&[], at) };
            let n = u32le(n) as usize;
            (p.get(at + 4..at + 4 + n).unwrap_or(&[]), at + 4 + n)
        };
        let (state, next) = blob(end);
        huds.push(crate::hud::HudState::decode(state, blob(next).0).unwrap_or_default());
        let mut tick = std::mem::take(&mut pending);
        tick.extend(draw.chunks_exact(42).map(|c| std::array::from_fn::<i16, 21, _>(|k| i16::from_le_bytes([c[2 * k], c[2 * k + 1]]))));
        ticks.push(tick);
    }
    (ticks, huds)
}

/// Object alpha from transparency k11: (32-t)/32; orgs store negative values as -1-t (addUpdateOrg). None = fully
/// transparent (t >= 32, e.g. a unit's spawn tick before it fades in): not drawn, since alpha 0 is sprite.wgsl's
/// "flat colour, no texture" marker and would show as a solid white quad for a frame.
pub fn obj_alpha(r: &Rec) -> Option<f32> {
    let t = if r[0] == 1 && r[11] < 0 { -1 - r[11] } else { r[11].max(0) } as i32;
    (t < 32).then(|| (32 - t) as f32 / 32.0)
}

#[test]
fn spawn_tick_not_drawn_as_flat_quad() {
    // first record of a peasant spawned at the campfire (sim draw pass, Crater Lake), then its fade-in steps
    let mut r: Rec = [2, 389, 358, 17, 969, -80, 0, 6, 14, 8, 280, 32, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    assert_eq!(obj_alpha(&r), None);
    r[11] = 31;
    assert_eq!(obj_alpha(&r), Some(1.0 / 32.0));
    r[11] = 0;
    assert_eq!(obj_alpha(&r), Some(1.0));
    let org: Rec = [1, 0, 0, 5, 1, 0, 0, 0, 0, 0, 7, -33, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // org: -1-t, t = 32
    assert_eq!(obj_alpha(&org), None);
}

pub struct World {
    pub tiles: Vec<Rec>, // per game tile (y*800+x, West frame): last type-0 record; file (k3) 0 = nothing drawn
    pub objs: BTreeMap<(i16, i32), Rec>, // (type, id) -> last record, k1/k2 in West frame
    pub rot: i16,                        // engine rotation of the incoming records
    /// chimp id -> its continuation record 0x33 (GameMap: selection / health bar `hpsFrame` k10 of GM 16 at the sprite
    /// position + (k8, -k9)/32 units; siege flag k4; oil k11; target k17), sent right after the chimp record
    pub marks: BTreeMap<i32, Rec>,
    /// chimp id -> its continuation 0x34 (k1 / k2 next tile, West frame like k1 / k2 of objs; k8 / k9 next pixel
    /// offset, k5 next −height ×32, k7 frames to get there, k17 next layer delay) and `applies` when it came: the
    /// chimp slides there (GameMap addUpdateChimp `interp`)
    pub next: BTreeMap<i32, (Rec, u64)>,
    /// chimp id -> its continuation 0x35 (k17 tele_x_adjust half pixels, k18 / k19 assassin rope x / y, k16 fade)
    pub tele: BTreeMap<i32, Rec>,
    pub applies: u64, // `apply` calls so far (one per tick / frame of records)
    pub applied_at: Option<std::time::Instant>, // when the last `apply` came (interpolation clock)
    pub colours: [u8; 9], // the players' colours (`colours`, from the ROTATION record; identity until one comes)
}

impl World {
    pub fn new() -> Self {
        Self { tiles: vec![[0; 21]; N * N], objs: BTreeMap::new(), rot: 6, marks: BTreeMap::new(), next: BTreeMap::new(), tele: BTreeMap::new(), applies: 0, applied_at: None, colours: IDENTITY }
    }

    /// Rotation changed: move everything already stored into the new frame (all records live in West frame
    /// coordinates of the rotation they arrived under).
    fn rotate(&mut self, rot: i16, dirty: &mut Vec<usize>) {
        let remap = |old: i16, x: i16, y: i16| { let (gx, gy) = from_west(old, x, y); to_west(rot, gx, gy) };
        let mut tiles = vec![[0; 21]; N * N];
        for (i, t) in self.tiles.iter().enumerate() {
            if t[3] == 0 { continue; }
            let (x, y) = remap(self.rot, (i % N) as i16, (i / N) as i16);
            let mut t = *t;
            (t[1], t[2]) = (x, y);
            tiles[y as usize * N + x as usize] = t;
        }
        self.tiles = tiles;
        for r in self.objs.values_mut().chain(self.next.values_mut().map(|n| &mut n.0)) { (r[1], r[2]) = remap(self.rot, r[1], r[2]); }
        self.rot = rot;
        dirty.extend(0..N * N);
    }

    /// Applies one tick; pushes the index of every changed tile to `dirty`.
    pub fn apply(&mut self, tick: &[Rec], dirty: &mut Vec<usize>) {
        let mut chimp = None; // the last chimp record: continuations 0x35, 0x33, 0x34 follow it
        self.applies += 1;
        self.applied_at = Some(std::time::Instant::now());
        for r in tick {
            if r[0] == ROTATION {
                if r[1] != self.rot { self.rotate(r[1], dirty); }
                if r[2..11].iter().any(|&c| c != 0) { self.colours = std::array::from_fn(|k| r[2 + k].clamp(0, 8) as u8); }
                continue;
            }
            match (r[0], chimp) {
                (0x33, Some(id)) => { self.marks.insert(id, *r); continue } // k1 / k2 are offsets here, not tiles
                (0x35, Some(id)) => { self.tele.insert(id, *r); continue }
                (0x34, Some(id)) => {
                    let mut n = *r;
                    (n[1], n[2]) = to_west(self.rot, n[1], n[2]);
                    self.next.insert(id, (n, self.applies));
                    continue;
                }
                (0x33..=0x35, None) => continue,
                _ => {}
            }
            if r[0] == 2 {
                let id = r[10] as i32;
                chimp = Some(id);
                for m in [&mut self.marks, &mut self.tele] { m.remove(&id); }
                self.next.remove(&id);
            }
            let mut r = *r;
            if (0..N as i16).contains(&r[1]) && (0..N as i16).contains(&r[2]) {
                (r[1], r[2]) = to_west(self.rot, r[1], r[2]);
            }
            let r = &r;
            let (x, y) = (r[1] as usize, r[2] as usize);
            match r[0] {
                0 if x < N && y < N => {
                    self.tiles[y * N + x] = *r;
                    // the front neighbours' cliff sides depend on this tile's height (see Gm::chevron)
                    dirty.extend([y * N + x, y * N + x.saturating_sub(1), (y + 1).min(N - 1) * N + x]);
                }
                // orgs, chimps, flies / projectiles, building anims, pixies (8 on walls, 9 on the ground: order markers)
                1 | 2 | 3 | 4 | 8 | 9 => {
                    let id = if r[0] == 4 { r[10] as i32 + r[19] as i32 * 10000 } else { r[10] as i32 };
                    if r[3] == 0 { self.objs.remove(&(r[0], id)); } else { self.objs.insert((r[0], id), *r); }
                }
                // whitecaps (k4 < 0 deletes) and wall fill-ins (k3 == 0 deletes): one per tile
                6 | 7 if x < N && y < N => {
                    let (id, gone) = ((y * N + x) as i32, if r[0] == 6 { r[4] < 0 } else { r[3] == 0 });
                    if gone { self.objs.remove(&(r[0], id)); } else { self.objs.insert((r[0], id), *r); }
                }
                _ => {} // ponytail: continuations 52 / 53 (interpolation, assassin rope), events not drawn yet
            }
        }
    }
}

/// GM file id -> sprite index lookup + team palette, from gm.tsv (re/scripts/gen_gm_table.py).
pub struct Gm { files: HashMap<i16, Vec<(String, bool, i32)>>, names: HashMap<String, usize>, palettes: HashMap<i16, Vec<[f32; 3]>> }

impl Gm {
    pub fn new(names: &[String]) -> Self {
        let (mut files, mut palettes): (HashMap<i16, Vec<(String, bool, i32)>>, HashMap<_, _>) = Default::default();
        for l in include_str!("gm.tsv").lines() {
            let c: Vec<&str> = l.split('\t').collect();
            let id = c[0].parse().unwrap();
            files.entry(id).or_default().push((c[1].into(), c[2] == "1", c[3].parse().unwrap()));
            if !c[4].is_empty() { // empty = plainShader, no team colour
                palettes.entry(id).or_insert_with(|| c[4].split(';').map(|rgb| {
                    let v: Vec<f32> = rgb.split(',').map(|x| x.parse().unwrap()).collect();
                    [v[0], v[1], v[2]]
                }).collect());
            }
        }
        Self { files, names: names.iter().enumerate().map(|(i, n)| (n.clone(), i)).collect(), palettes }
    }

    /// `gmSprites[file][image]`: the entry with the largest ID_Offset <= image wins (sea_new vs sea_shore).
    fn get(&self, file: i16, image: i32) -> Option<usize> {
        let (name, dash, off) = self.files.get(&file)?.iter().filter(|e| e.2 <= image).max_by_key(|e| e.2)?;
        let key = if *dash { format!("{name}-{}", image - off) } else { format!("{name} {:03}", image - off) };
        self.names.get(&key).copied()
    }

    /// SpriteMapping.setGenericBuildingTileGraphic -> (sprite, tile.light). `light` = record k6, which C# only uses
    /// for buildings1 / goods; everything else gets light 0 except the sea folds (CalcLightValueForSea -> 50..55).
    pub fn tile(&self, file: i16, image: i16, light: i16) -> (Option<usize>, i32) {
        let (i, k6) = (image as i32, light as i32);
        let lit = (k6 - 2).max(0) + 10;
        let sea = |v: i32| if v <= 2 { 50 } else { v - 3 + 51 };
        // old sea (GM 5): animation/light variants folded into `image` as blocks of `n` frames
        let fold = |base: i32, n: i32| ((i - base) % n + base, sea((i - base) / n));
        let (i, l) = match (file, i) {
            (5, 1..=32) => fold(1, 4),
            (5, 33..=63) => fold(5, 4),
            (5, 65..=104) => (9 + (i - 65) / 8, sea((i - 65) % 8)),
            (5, 64 | 105..=149) => (1, 0),
            (5, 205..=332) => fold(205 + (i - 205) / 32 * 32, 4),
            (5, 333..=340) => (333, sea(i - 333)),
            (5, 533..=660) => fold(533 + (i - 533) / 32 * 32, 4),
            (5, 661..=668) => (661, sea(i - 661)),
            (5, 669..=732) => fold(669, 8),
            (5, 733..=764) => fold(733, 4),
            (5, 1805..) => fold(1805, 60),
            (166, 1..=1476) => ((i - 1) % 246 + 1, (i - 1) / 246),
            (166, 2168..=7063) => ((i - 2168) % 816 + 2168, (i - 2168) / 816),
            (6, 3..=4) => (1, 0),
            (6, 7..=8) => (5, 0),
            (6, 24..=197 | 297..=305) | (15 | 55, 5..=8) => (i, lit),
            _ => (i, 0),
        };
        (self.get(file, i), l)
    }

    /// Team colour for `colour` (record k7, low 14 bits): SpriteMapping.SetBodySprite -> GetGMMaterial palette, colours
    /// 0..8 through `remap` (remapColours = `World::colours`) except the trees' (files 29-31, 70-72, 97).
    /// White = plain material (no palette). The caller sets `.3` to the _SpriteCutoff of the material variant.
    pub fn team(&self, file: i16, colour: i16, remap: &[u8; 9]) -> [f32; 4] {
        let mut c = (colour & 0x3fff) as usize;
        if !matches!(file, 29..=31 | 70..=72 | 97) && c <= 8 { c = remap[c] as usize; }
        match self.palettes.get(&file).and_then(|p| p.get(c)) { Some(&[r, g, b]) => [r, g, b, 0.0], None => [1.0, 1.0, 1.0, 0.0] }
    }
}

/// gameTile.setTileColour: tile.light / floatColour (k7) -> tint (TilemapManager colours).
pub fn shade(light: i32, float_colour: i16) -> [f32; 3] {
    let g = |v: f32| [v; 3];
    match float_colour {
        55 => [0.976, 0.302, 0.004], 56 => [0.231, 0.647, 0.067], 57 => [0.341, 0.329, 0.878], 58 => [0.898, 0.855, 0.498],
        59 | 62 => g(0.502), 60 => [0.659, 0.271, 0.133], 61 => g(0.251), 63 => [0.824, 0.824, 1.0],
        _ => match light {
            1..=7 => g(1.0 - light as f32 * 0.1),
            11..=17 => g(1.0 - (light - 10) as f32 * 0.06),
            51..=55 => g([0.95, 0.9, 0.85, 0.8, 0.85][(light - 51) as usize]),
            _ => g(1.0),
        },
    }
}

impl Gm {

    /// Cliff side under a raised tile: SpriteMapping.getChevronImage (files 3/9/10, image direct), shown by
    /// gameTile.GetTileData only if a front neighbour, (x+1,y) or (x,y-1), is lower. ponytail: wall case (k13/14) skipped.
    pub fn chevron(&self, w: &World, i: usize) -> Option<usize> {
        let r = &w.tiles[i];
        let h = |j: Option<usize>| j.map_or(0, |j| w.tiles[j][5]);
        let (x, y) = (i % N, i / N);
        let hidden = h((x + 1 < N).then(|| y * N + x + 1)) >= r[5] && h(y.checked_sub(1).map(|y| y * N + x)) >= r[5];
        if !matches!(r[11], 3 | 9 | 10) || hidden { return None; }
        self.get(r[11], r[12] as i32)
    }

    /// SpriteMapping.getBodyImage: frame is 1-based except for a few files (CSHARP_NOTES §2).
    pub fn body(&self, file: i16, image: i16) -> Option<usize> {
        let direct = matches!(file, 16 | 22 | 28 | 54 | 105 | 116 | 136 | 158 | 198);
        if file == 85 && image > 101 {
            // GM_FLOAT_POP_CIRC overflow frames live in GM_FLOAT_POP_CIRC_2 (218): SpriteMapping.getBodyImage
            return self.get(218, image as i32 - 202 - 1);
        }
        self.get(file, image as i32 - if direct { 0 } else { 1 })
    }
}

/// Iso row of game tile index `i` (West frame): screen rows are constant x−y, growing toward the front (down).
/// C# `GameMapTile.row` is the same ordering (sortingOrder −20000 + row*49 grows toward the front).
pub fn row(i: usize) -> i32 { (i % N) as i32 - (i / N) as i32 }

/// Screen px (y down) of a game tile's centre: 64×32 diamonds, screen rows = constant x−y, x+y grows to the left.
/// Picked by rendering the 7×7 keep (tile_castle 714..762) in all 4 axis variants: only this one assembles; the tall
/// parts sit on the BACK edges. Centring offset ignored.
pub fn cell_px(x: i16, y: i16) -> [f32; 2] {
    let (x, y) = (x as f32, y as f32);
    [-(x + y) * 32.0, (x - y) * 16.0]
}

/// GameMap.getPivotOffset by gameObjectType (chimp k20 low byte) in screen px (y down).
pub fn pivot(t: i32) -> [f32; 2] {
    let (x, y) = match t {
        85 => (0.0, 8.0), 84 | 83 | 82 | 76 | 22 | 27 | 30 | 5 | 31 | 32 | 51 | 33 | 55 | 64 => (0.0, 4.0),
        81 | 80 | 79 | 72 | 16 | 65 => (0.0, 5.0), 78 | 18 | 21 | 52 => (0.0, 6.0), 70 | 71 | 17 | 56 => (0.0, 2.0),
        23 => (3.0, -4.0), 25 => (0.0, -2.0), 29 | 19 | 10 | 42 | 66 => (0.0, 3.0), 37 => (2.0, 0.0), 1 => (-3.0, 3.0),
        3 | 45 | 2 | 67 => (0.0, -3.0), 7 => (-3.0, -4.0), 11..=14 => (-2.0, 0.0), 43 => (0.0, 16.0), 6 => (0.0, -1.0),
        44 => (0.0, 12.0), 46 => (5.0, -3.0), 47 => (0.0, 13.0), 57 => (2.0, 6.0), 20 => (2.0, -2.0), 60 => (0.0, 10.0),
        62 => (3.0, 0.0), 41 => (9.0, 10.0), 88 => (2.0, 9.0), 74 => (-10.0, 15.0), _ => (0.0, 0.0),
    };
    [x, -y]
}

/// A chimp's spritePosVector (+ heightAboveGround and the 0x35 tele_x_adjust half pixels `tele`): where its siege
/// flag / health bar / oil / target sprites hang (GameMap addUpdateChimp).
pub fn chimp_base(r: &Rec, tele: i16) -> [f32; 2] {
    let p = sprite_px(r, r[8], r[9], -r[5]);
    [p[0] + tele as f32 * 2.0, p[1]]
}

/// Chimp.position (the body sprite, also what grabTroopsOnScreen picks by): base + getPivotOffset + yOffset1 (−k17).
pub fn chimp_px(r: &Rec, tele: i16) -> [f32; 2] {
    let (b, v) = (chimp_base(r, tele), pivot((r[20] & 0xff) as i32));
    [b[0] + v[0], b[1] + v[1] + r[17] as f32 * 2.0]
}

/// GameMap.getSpritePosVector + heightAboveGround (`up` in Unity units ×32, as in the record).
pub fn sprite_px(r: &Rec, tx: i16, ty: i16, up: i16) -> [f32; 2] {
    let s = |v: i16| if v > 128 { v as f32 - 256.0 } else { v as f32 };
    let [cx, cy] = cell_px(r[1], r[2]);
    [cx - 32.0 + (s(tx) + 1.0) * 2.0, cy - 16.0 + (s(ty) + 1.0) * 2.0 - up as f32 * 2.0]
}

#[test]
fn apply_incremental() {
    let mut w = World::new();
    let mut d = vec![];
    let mut tile = [0i16; 21];
    tile[1] = 3; tile[2] = 4; tile[3] = 55; tile[4] = 7;
    let mut unit = [0i16; 21];
    unit[0] = 2; unit[3] = 89; unit[10] = 42;
    w.apply(&[tile, unit], &mut d);
    assert_eq!((w.tiles[4 * N + 3][3..5].to_vec(), d[0], w.objs.len()), (vec![55, 7], 4 * N + 3, 1));
    unit[3] = 0; // file 0 = delete
    w.apply(&[unit], &mut d);
    assert!(w.objs.is_empty());
}

/// Options colour 3 (blue: red, orange, yellow, blue, ...) in a skirmish vs the Rat: player 1's sprites take colour 2 (blue), the Rat another; a save
/// of that game loads with the same colours; the ROTATION record carries them to the renderer.
#[cfg(feature = "live")]
#[test]
fn player_colours_follow_the_engine() {
    use crate::skirmish::{maps_dir, start, Lobby};
    let map = maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    let mut m = start(&Lobby { colour: 3, ..Lobby::new(map) }).unwrap();
    let c = colours(&m);
    println!("colours {c:?}, mode {}", m.i32(0x183663eb8));
    assert_eq!(c[1], 2, "you: blue");
    assert!(c[2] != 0 && c[2] != 2, "the Rat: another colour");
    let mut loaded = shc_sim::mem::Mem::default();
    shc_sim::world::load_save(&mut loaded, &shc_sim::save::save_game(&mut m, 0)).unwrap();
    assert_eq!(colours(&loaded), c, "a loaded save keeps them");
    let mut r = [0i16; 21];
    (r[0], r[1]) = (ROTATION, 6);
    for k in 0..9 { r[2 + k] = c[k] as i16; }
    let mut w = World::new();
    w.apply(&[r], &mut vec![]);
    assert_eq!(w.colours, c);
    let mut rec = [0i16; 21]; // a recording's DLL_SetMapRotation: no colours, keeps them
    (rec[0], rec[1]) = (ROTATION, 6);
    w.apply(&[rec], &mut vec![]);
    assert_eq!(w.colours, c);
}

