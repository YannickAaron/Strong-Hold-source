//! Stronghold Crusader 2's Sassanid Knight as a DE sprite sheet, rendered from the player's own SC2 install
//! (re/SC2_MODELS.md). Fills every frame the sim picks for a DE knight (gm 44 `body_knight`) under the sprite name
//! `sc2_sassanid_knight-<frame>`, in one BC3 atlas + BC1 mask appended to the DE export's atlases/masks/sprites.tsv.
//! Nothing of SC2's is shipped: this runs on the player's machine and writes only to the export cache.
pub mod read;
pub mod render;

use crate::{bc, fmt_g, Result};
use read::{Bone, Clip, Zip};
use render::{Frame, Look, Mesh, DE_LOOK};
use std::{collections::HashMap, fmt::Write as _, fs, path::Path};

/// Bump when the output changes: the step runs again once.
pub const VERSION: &str = "1";
/// SC2's Steam folder name (steamapps/common/…).
pub const GAME: &str = "Stronghold Crusader 2";
/// Atlas name and sprite name prefix (gm.tsv row 1044).
pub const NAME: &str = "sc2_sassanid_knight";
const DONE: &str = "sc2.done";

/// The SC2 install folder (holds assets/assets.v).
pub fn is_game(dir: &Path) -> bool { dir.join("assets/assets.v").is_file() }
pub fn is_complete(cache: &Path) -> bool { fs::read_to_string(cache.join(DONE)).is_ok_and(|v| v.trim() == VERSION) }
/// The DE sprite export rewrites the .tsv files without our rows (atlases::export calls this).
pub fn invalidate(cache: &Path) { let _ = fs::remove_file(cache.join(DONE)); }

/// DE body_knight frame blocks (frame = start + facing + 8 × counter, re/UNIT_BEHAVIOUR.md §28 and
/// sim/src/units/mod.rs `image`) → the SC2 clip resampled into them. Odd unit slots (DE's second horse livery) take
/// the `_02` idle / attack for variety. `looped`: counter k samples k/n of the clip, else k/(n−1) (ends on the pose).
pub const BLOCKS: [(u32, u32, &str, bool); 10] = [
    (0, 16, "walk_01", true),       // walk (base 1)
    (128, 16, "canter_01", true),   // gallop (base 0x81)
    (384, 24, "idle_01", true),     // idle 0x179 (celebrate 0x17a shares it)
    (576, 18, "attack_01", true),   // fight 0x239
    (720, 16, "walk_01", true),     // odd slots: walk + 0x2d0
    (848, 16, "canter_01", true),   // odd slots: gallop + 0x2d0
    (1104, 24, "idle_02", true),    // odd slots: idle 0x449
    (1296, 18, "attack_02", true),  // odd slots: fight 0x509
    (1440, 24, "death_01", false),  // death 0x599 (action 0x6f)
    (1632, 24, "death_01", false),  // substitute: death 0x659 (0x70-0x74); SC2 has one death
];

/// SC2's model of the unit: rider + horse + spear + shield and their master animations.
pub struct Knight { horse: Mesh, rider: Mesh, spear: Mesh, shield: Mesh, hsk: Vec<Bone>, hclips: HashMap<String, Clip>, rsk: Vec<Bone>, rclips: HashMap<String, Clip> }

/// Where the rider sits: on the horse's `Bip01 Spine` + this (the real value is in SC2's exe; fitted by eye).
const SEAT: [f32; 3] = [0.0, 0.0, 95.0];

impl Knight {
    pub fn load(sc2: &Path, look: &Look) -> Result<Knight> {
        let assets = Zip::open(&sc2.join("assets/assets.v"), 0x55)?;
        let paks: Vec<Zip> = ["ultrahigh", "high", "med", "low"].iter().map(|q| sc2.join(format!("assets/textures.{q}.pak")))
            .filter(|p| p.is_file()).map(|p| Zip::open(&p, 0)).collect::<Result<_>>()?;
        let tex = |name: &str| -> Result<(usize, usize, Vec<u8>)> {
            let path = format!("textures/characters/{name}/{name}.colour.dds");
            let b = paks.iter().find_map(|z| z.read(&path).ok()).ok_or_else(|| format!("SC2: no {path}"))?;
            read::dds(&b)
        };
        let mesh = |m: &str, t: &(usize, usize, Vec<u8>)| -> Result<Mesh> { Ok(Mesh::new(read::model(&assets.read(m)?)?, t.clone(), look.detail)) };
        let (k, h) = (tex("sassanid_knight")?, tex("sassanid_knight_horse")?);
        let (hsk, hclips) = read::anim(&assets.read("animations/characters/animal.horse_master.anim")?)?;
        let (rsk, rclips) = read::anim(&assets.read("animations/characters/soldier_master_02.anim")?)?;
        Ok(Knight {
            horse: mesh("meshes/characters/sassanid_knight_horse/sassanid_knight_horse.high.model", &h)?,
            rider: mesh("meshes/characters/sassanid_knight/sassanid_knight.high.model", &k)?,
            spear: mesh("meshes/props/weapons/sassanid_knight_spear.high.model", &k)?,
            shield: mesh("meshes/props/weapons/sassanid_knight_shield.high.model", &k)?,
            hsk, hclips, rsk, rclips,
        })
    }

    /// Rider and horse clips of one name (same duration in SC2's data).
    fn clips(&self, name: &str) -> Result<(&Clip, &Clip)> {
        let r = format!("soldier.sassanid_knight.{name}");
        let h = format!("animal.sassanid_knight_horse.{name}");
        Ok((self.rclips.get(&r).ok_or_else(|| format!("SC2: no clip {r}"))?, self.hclips.get(&h).ok_or_else(|| format!("SC2: no clip {h}"))?))
    }

    /// Length of clip `name` in seconds.
    pub fn duration(&self, name: &str) -> Result<f32> { Ok(self.clips(name)?.0.duration()) }

    /// The four meshes posed at `t` s of clip `name`, rendered facing DE direction `d` (0 NE, 1 E, … 7 N).
    pub fn frame(&self, name: &str, t: f32, looped: bool, d: u32, look: &Look) -> Result<Frame> {
        let (rc, hc) = self.clips(name)?;
        let wh = render::pose(&self.horse.m.bones, &self.hsk, hc, t, looped);
        let mut wr = render::pose(&self.rider.m.bones, &self.rsk, rc, t, looped);
        let bone = |bs: &[Bone], n: &str| bs.iter().position(|b| b.name == n).ok_or_else(|| format!("SC2: no bone {n}"));
        let s = bone(&self.horse.m.bones, "Bip01 Spine")?;
        let bind = render::inv(&render::aff(self.horse.m.bones[s].inv.0, self.horse.m.bones[s].inv.1));
        let off = render::translate(std::array::from_fn(|i| SEAT[i] + wh[s][i][3] - bind[i][3]));
        for m in &mut wr { *m = render::mul(&off, m); }
        let (p1, p2) = (bone(&self.rider.m.bones, "Bip01 Prop1")?, bone(&self.rider.m.bones, "Bip01 Prop2")?);
        let (hp, hn) = self.horse.skin(&wh);
        let (rp, rn) = self.rider.skin(&wr);
        let (sp, sn) = self.spear.skin(&[wr[p1]]);
        let (dp, dn) = self.shield.skin(&[wr[p2]]);
        let parts = [(hp, hn, &self.horse), (rp, rn, &self.rider), (sp, sn, &self.spear), (dp, dn, &self.shield)];
        Ok(render::render(&parts, 90.0 - 45.0 * (d as f32 - 1.0), look))
    }

    /// Every distinct frame of BLOCKS: (clip, counters, looped) → frames indexed [counter * 8 + facing].
    pub fn render_all(&self, look: &Look) -> Result<HashMap<(&'static str, u32, bool), Vec<Frame>>> {
        let mut jobs: Vec<(&'static str, u32, bool)> = BLOCKS.iter().map(|b| (b.2, b.1, b.3)).collect();
        jobs.sort();
        jobs.dedup();
        let work: Vec<((&'static str, u32, bool), u32)> = jobs.iter().flat_map(|&j| (0..j.1 * 8).map(move |i| (j, i))).collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let done: Vec<Vec<(usize, Result<Frame>)>> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..threads).map(|_| s.spawn(|| {
                let mut out = vec![];
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&((name, n, looped), f)) = work.get(i) else { break };
                    let r = self.clips(name).and_then(|(rc, _)| {
                        let (k, dur) = (f / 8, rc.duration());
                        let t = if looped { dur * k as f32 / n as f32 } else { dur * k as f32 / (n - 1).max(1) as f32 };
                        self.frame(name, t, looped, f % 8, look)
                    });
                    out.push((i, r));
                }
                out
            })).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut flat: Vec<Option<Result<Frame>>> = (0..work.len()).map(|_| None).collect();
        for (i, r) in done.into_iter().flatten() { flat[i] = Some(r); }
        let mut out: HashMap<_, Vec<Frame>> = HashMap::new();
        for (w, r) in work.iter().zip(flat) { out.entry(w.0).or_default().push(r.unwrap()?); }
        Ok(out)
    }
}

/// Transparent pixels take their opaque neighbours' colour (a few px out): no dark fringes from linear filtering or
/// from BC blocks that straddle the outline.
fn bleed(px: &mut [u8], w: usize, h: usize) {
    let mut have: Vec<bool> = px.chunks(4).map(|p| p[3] > 0).collect();
    for _ in 0..3 {
        let prev = have.clone();
        for y in 0..h { for x in 0..w {
            let i = y * w + x;
            if prev[i] { continue; }
            let (mut s, mut n) = ([0u32; 3], 0);
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 { continue; }
                let j = ny as usize * w + nx as usize;
                if prev[j] { for c in 0..3 { s[c] += px[j * 4 + c] as u32; } n += 1; }
            }
            if n > 0 { for c in 0..3 { px[i * 4 + c] = (s[c] / n) as u8; } have[i] = true; }
        } }
    }
}

/// Shelf-packs frames (tallest first) into rows of `W` px, bottom-up like Unity's atlases. → (x, y, height used).
fn pack(sizes: &[(usize, usize)]) -> Result<(Vec<(usize, usize)>, usize)> {
    const W: usize = 8192;
    const GAP: usize = 2;
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i].1));
    let (mut at, mut x, mut y, mut row) = (vec![(0, 0); sizes.len()], 0, 0, 0);
    for i in order {
        let (w, h) = sizes[i];
        if x + w > W { (x, y, row) = (0, y + row + GAP, 0); }
        at[i] = (x, y);
        x += w + GAP;
        row = row.max(h);
    }
    let used = (y + row).next_multiple_of(4);
    if used > 8192 { return Err(format!("SC2 atlas: {used} px tall, over 8192").into()); }
    Ok((at, used))
}

/// `name`'s .tsv with our earlier rows dropped (`keep` false), our new ones appended; written atomically.
fn rewrite(cache: &Path, name: &str, keep: impl Fn(&str) -> bool, add: &str) -> Result<()> {
    let old = fs::read_to_string(cache.join(name)).unwrap_or_default();
    let mut s: String = old.lines().filter(|l| keep(l)).flat_map(|l| [l, "\n"]).collect();
    s.push_str(add);
    let tmp = cache.join(format!("{name}.tmp"));
    fs::write(&tmp, s)?;
    fs::rename(tmp, cache.join(name))?;
    Ok(())
}

/// Renders the knight from the SC2 install `sc2` into the DE export `cache` (needs its atlases.tsv / sprites.tsv).
pub fn export(sc2: &Path, cache: &Path) -> Result<()> {
    let t0 = std::time::Instant::now();
    let look = DE_LOOK;
    let knight = Knight::load(sc2, &look)?;
    let frames = knight.render_all(&look)?;
    let t1 = t0.elapsed().as_secs_f32();
    // one atlas slot per distinct frame; DE frame names point at them
    let keys: Vec<(&'static str, u32, bool)> = { let mut k: Vec<_> = frames.keys().copied().collect(); k.sort(); k };
    let all: Vec<&Frame> = keys.iter().flat_map(|k| &frames[k]).collect();
    let (at, h) = pack(&all.iter().map(|f| (f.w, f.h)).collect::<Vec<_>>())?;
    const W: usize = 8192;
    let (mut img, mut mask) = (vec![0u8; W * h * 4], vec![0u8; W * h * 4]);
    for (f, &(x, y)) in all.iter().zip(&at) {
        let mut rgba = f.rgba.clone();
        bleed(&mut rgba, f.w, f.h);
        for r in 0..f.h {
            let d = ((y + f.h - 1 - r) * W + x) * 4; // frame rows top-down → atlas rows bottom-up
            img[d..d + f.w * 4].copy_from_slice(&rgba[r * f.w * 4..][..f.w * 4]);
            mask[d..d + f.w * 4].copy_from_slice(&f.mask[r * f.w * 4..][..f.w * 4]);
        }
    }
    fs::write(cache.join(format!("{NAME}.bc")), bc::encode(&img, W, h, true))?;
    fs::write(cache.join(format!("{NAME}_m.bc")), bc::encode(&mask, W, h, false))?;
    // atlases.tsv: index, name, w, h, unity format (12 = DXT5); masks.tsv: atlas index, w, h, format (10 = DXT1)
    let atlases = fs::read_to_string(cache.join("atlases.tsv")).unwrap_or_default();
    let ours = |l: &str| l.split('\t').nth(1) == Some(NAME);
    let old = atlases.lines().find(|l| ours(l)).and_then(|l| l.split('\t').next()).map(str::to_string);
    let index = atlases.lines().filter(|l| !ours(l)).count();
    rewrite(cache, "atlases.tsv", |l| !ours(l), &format!("{index}\t{NAME}\t{W}\t{h}\t12\n"))?;
    rewrite(cache, "masks.tsv", |l| old.as_deref() != l.split('\t').next(), &format!("{index}\t{W}\t{h}\t10\n"))?;
    let mut rows = String::new();
    let first: HashMap<_, usize> = { let mut o = 0; keys.iter().map(|k| { let s = o; o += frames[k].len(); (*k, s) }).collect() };
    for &(start, n, clip, looped) in &BLOCKS {
        for i in 0..n * 8 {
            let slot = first[&(clip, n, looped)] + i as usize;
            let (f, (x, y)) = (all[slot], at[slot]);
            let (l, r, b, t) = (-f.anchor[0], f.w as f32 - f.anchor[0], f.anchor[1] - f.h as f32, f.anchor[1]);
            let mesh: Vec<String> = [(l, b), (r, b), (r, t), (l, b), (r, t), (l, t)].iter().map(|&(x, y)| format!("{},{}", fmt_g(x as f64), fmt_g(y as f64))).collect();
            writeln!(rows, "{NAME}-{}\t{index}\t{x}\t{y}\t{}\t{}\t{:.6}\t{:.6}\t{}", start + i, f.w, f.h,
                f.anchor[0] as f64 / f.w as f64, 1.0 - f.anchor[1] as f64 / f.h as f64, mesh.join(";"))?;
        }
    }
    let pre = format!("{NAME}-");
    rewrite(cache, "sprites.tsv", |l| !l.starts_with(&pre), &rows)?;
    fs::write(cache.join(DONE), VERSION)?;
    eprintln!("sc2: {} frames rendered in {t1:.1} s, atlas 8192×{h}, {:.1} s total", all.len(), t0.elapsed().as_secs_f32());
    Ok(())
}

#[test]
fn pack_shelves_bottom_up() {
    let (at, h) = pack(&[(10, 5), (8190, 7), (4, 3)]).unwrap();
    assert_eq!(at[1], (0, 0)); // tallest first, alone on the bottom shelf
    assert_eq!(at[0], (0, 9));
    assert_eq!(at[2], (12, 9));
    assert_eq!(h, 16);
}

#[test]
fn rigid_inverse() {
    let a = render::aff([1.0, 2.0, 3.0], [0.1, 0.7, -0.2, 0.68]);
    let i = render::mul(&a, &render::inv(&a));
    for r in 0..3 { for c in 0..4 { assert!((i[r][c] - if r == c { 1.0 } else { 0.0 }).abs() < 1e-5, "{i:?}"); } }
}
