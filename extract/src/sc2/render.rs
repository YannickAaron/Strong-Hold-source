//! CPU renderer for posed SC2 meshes → one DE-style sprite frame (straight-alpha RGBA + team mask), the Rust port
//! of the spike's render.py (re/SC2_MODELS.md "Render recipe"). Orthographic, supersampled, nearest-texel.
use super::read::{Bone, Clip, Model};

/// The look knobs, defaults = the spike's "DE look" (re/SC2_MODELS.md). Per unit if another SC2 unit needs others.
#[derive(Clone, Copy)]
pub struct Look {
    /// Output px per model unit (SC2 ≈ cm) and camera elevation in degrees (30 = DE's 2:1 tiles).
    pub scale: f32, pub elev: f32,
    /// Supersampling per axis.
    pub ss: usize,
    /// Drop-shadow light (projects the mesh onto z = 0), shadow alpha, blur sigma in output px.
    pub light: [f32; 3], pub shadow: f32, pub shadow_blur: f32,
    /// Shading key light (camera frame: x right, y away from the camera, z up), ambient, diffuse, sky/ground ambient.
    pub key: [f32; 3], pub amb: f32, pub dif: f32, pub hemi: f32,
    /// Blinn-Phong gloss strength / exponent, output gamma.
    pub spec: f32, pub spec_exp: f32, pub gamma: f32,
    /// Share of the texture's fine detail kept (blur sigma = texture height / 256).
    pub detail: f32,
}

pub const DE_LOOK: Look = Look {
    scale: 0.5, elev: 30.0, ss: 4, light: [-1.0, 0.35, 5.0], shadow: 0.5, shadow_blur: 1.5,
    key: [-0.9, -0.7, 1.0], amb: 0.55, dif: 0.9, hemi: 0.9, spec: 0.55, spec_exp: 16.0, gamma: 0.8, detail: 0.3,
};

/// Rigid 3×4 transform, rows.
pub type Aff = [[f32; 4]; 3];

/// Quaternion x,y,z,w as SC2 stores it (conjugated: the matrix is built from (−x, −y, −z, w)) + position.
pub fn aff(p: [f32; 3], q: [f32; 4]) -> Aff {
    let n = (q.iter().map(|v| v * v).sum::<f32>()).sqrt().max(1e-12);
    let (x, y, z, w) = (-q[0] / n, -q[1] / n, -q[2] / n, q[3] / n);
    [[1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w), p[0]],
     [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w), p[1]],
     [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y), p[2]]]
}
pub fn mul(a: &Aff, b: &Aff) -> Aff {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum::<f32>() + if j == 3 { a[i][3] } else { 0.0 }))
}
pub fn inv(a: &Aff) -> Aff {
    std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { a[j][i] } else { -(0..3).map(|k| a[k][i] * a[k][3]).sum::<f32>() }))
}
pub fn translate(t: [f32; 3]) -> Aff { [[1.0, 0.0, 0.0, t[0]], [0.0, 1.0, 0.0, t[1]], [0.0, 0.0, 1.0, t[2]]] }
pub fn pt(a: &Aff, p: [f32; 3]) -> [f32; 3] { std::array::from_fn(|i| a[i][0] * p[0] + a[i][1] * p[1] + a[i][2] * p[2] + a[i][3]) }
fn dir(a: &Aff, p: [f32; 3]) -> [f32; 3] { std::array::from_fn(|i| a[i][0] * p[0] + a[i][1] * p[1] + a[i][2] * p[2]) }
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn norm(a: [f32; 3]) -> [f32; 3] { let l = dot(a, a).sqrt().max(1e-9); a.map(|v| v / l) }

/// Keys around `t`: (i, j, blend). Looping clips wrap, one-shots hold their last key.
fn keys(times: &[f32], t: f32, looped: bool) -> (usize, usize, f32) {
    let end = *times.last().unwrap();
    let t = if end <= 0.0 { 0.0 } else if looped { t.rem_euclid(end) } else { t.clamp(0.0, end) };
    let i = times.partition_point(|&k| k <= t).saturating_sub(1).min(times.len() - 1);
    let j = (i + 1).min(times.len() - 1);
    (i, j, if j == i { 0.0 } else { (t - times[i]) / (times[j] - times[i]) })
}

/// Bone world matrices of `bones` (a mesh skeleton) posed by `clip` at `t` s; bones match the clip's skeleton by
/// name (others keep their bind). The root's position keys are deltas against the anim skeleton's own bind root.
pub fn pose(bones: &[Bone], anim_skel: &[Bone], clip: &Clip, t: f32, looped: bool) -> Vec<Aff> {
    let (pi, pj, pf) = keys(&clip.pos.0, t, looped);
    let (ri, rj, rf) = keys(&clip.rot.0, t, looped);
    let mut world: Vec<Aff> = Vec::with_capacity(bones.len());
    for b in bones {
        let local = match anim_skel.iter().position(|a| a.name == b.name) {
            None => aff(b.local.0, b.local.1),
            Some(k) => {
                let (a, c) = (clip.pos.1[pi][k], clip.pos.1[pj][k]);
                let mut p: [f32; 3] = std::array::from_fn(|i| a[i] + (c[i] - a[i]) * pf);
                if b.parent < 0 { p = std::array::from_fn(|i| b.local.0[i] + p[i] - anim_skel[k].local.0[i]); }
                let (qa, mut qb) = (clip.rot.1[ri][k], clip.rot.1[rj][k]);
                if (0..4).map(|i| qa[i] * qb[i]).sum::<f32>() < 0.0 { qb = qb.map(|v| -v); }
                aff(p, std::array::from_fn(|i| qa[i] + (qb[i] - qa[i]) * rf))
            }
        };
        world.push(if b.parent < 0 { local } else { mul(&world[b.parent as usize], &local) });
    }
    world
}

/// A mesh plus its colour texture, softened once (`Look::detail`): RGBA 0..1, alpha = 1 − team colour.
pub struct Mesh { pub m: Model, tw: usize, th: usize, tex: Vec<[f32; 4]> }

impl Mesh {
    pub fn new(m: Model, (tw, th, px): (usize, usize, Vec<u8>), detail: f32) -> Mesh {
        let tex: Vec<[f32; 4]> = px.chunks(4).map(|p| std::array::from_fn(|i| p[i] as f32 / 255.0)).collect();
        let mut soft: Vec<f32> = tex.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        blur(&mut soft, tw, th, 3, th as f32 / 256.0);
        let tex = tex.iter().zip(soft.chunks(3)).map(|(p, b)| {
            let s = |i: usize| (b[i] + detail * (p[i] - b[i])).clamp(0.0, 1.0);
            [s(0), s(1), s(2), p[3]] // the team mask (alpha) stays crisp
        }).collect();
        Mesh { m, tw, th, tex }
    }
    /// World positions and normals for bone matrices `w` (linear blend skinning); rigid meshes take `w[0]`.
    pub fn skin(&self, w: &[Aff]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
        if self.m.weights.is_empty() { return (self.m.pos.iter().map(|&p| pt(&w[0], p)).collect(), self.m.nrm.iter().map(|&n| norm(dir(&w[0], n))).collect()); }
        let s: Vec<Aff> = self.m.bones.iter().zip(w).map(|(b, w)| mul(w, &aff(b.inv.0, b.inv.1))).collect();
        self.m.weights.iter().zip(self.m.pos.iter().zip(&self.m.nrm)).map(|(wt, (&p, &n))| {
            let mut m = [[0f32; 4]; 3];
            for &(bi, f) in wt.iter().filter(|e| e.1 > 0.0) { for r in 0..3 { for c in 0..4 { m[r][c] += s[bi as usize][r][c] * f; } } }
            (pt(&m, p), norm(dir(&m, n)))
        }).unzip()
    }
}

/// Separable Gaussian on `ch`-channel f32 rows, edges clamped.
fn blur(px: &mut [f32], w: usize, h: usize, ch: usize, sigma: f32) {
    if sigma <= 0.0 { return; }
    let r = (sigma * 3.0).ceil() as isize;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = k.iter().sum();
    let k: Vec<f32> = k.iter().map(|v| v / sum).collect();
    let mut tmp = vec![0f32; px.len()];
    for (len, step, lines, lstep) in [(w, ch, h, w * ch), (h, w * ch, w, ch)] {
        for l in 0..lines {
            for i in 0..len {
                for c in 0..ch {
                    tmp[l * lstep + i * step + c] = k.iter().enumerate().map(|(j, kv)| {
                        let s = (i as isize + j as isize - r).clamp(0, len as isize - 1) as usize;
                        kv * px[l * lstep + s * step + c]
                    }).sum();
                }
            }
        }
        px.copy_from_slice(&tmp);
    }
}

/// One rendered frame: straight-alpha RGBA8 and mask RGBA8 (R team colour, G foot-cut height), rows top-down,
/// `anchor` = the world origin (unit position) in px from the top-left.
pub struct Frame { pub w: usize, pub h: usize, pub rgba: Vec<u8>, pub mask: Vec<u8>, pub anchor: [f32; 2] }

/// `parts`: posed (world positions, normals, mesh). `yaw` turns the model about z (degrees).
pub fn render(parts: &[(Vec<[f32; 3]>, Vec<[f32; 3]>, &Mesh)], yaw: f32, l: &Look) -> Frame {
    let (e, y) = (l.elev.to_radians(), yaw.to_radians());
    let rz = |p: [f32; 3]| [p[0] * y.cos() - p[1] * y.sin(), p[0] * y.sin() + p[1] * y.cos(), p[2]];
    let (up, fw) = ([0.0, e.sin(), e.cos()], [0.0, e.cos(), -e.sin()]);
    let (light, key) = (norm(l.light), norm(l.key));
    let shadow_of = |p: [f32; 3]| [p[0] - p[2] / light[2] * light[0], p[1] - p[2] / light[2] * light[1], 0.0];
    let pw: Vec<Vec<[f32; 3]>> = parts.iter().map(|p| p.0.iter().map(|&v| rz(v)).collect()).collect();
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for v in pw.iter().flatten().flat_map(|&v| [v, shadow_of(v)]) {
        let sy = dot(v, up);
        (x0, x1, y0, y1) = (x0.min(v[0]), x1.max(v[0]), y0.min(sy), y1.max(sy));
    }
    let pad = 4.0 / l.scale;
    (x0, x1, y0, y1) = (x0 - pad, x1 + pad, y0 - pad, y1 + pad);
    let (wd, hd) = (((x1 - x0) * l.scale).ceil() as usize, ((y1 - y0) * l.scale).ceil() as usize);
    let (ws, hs, k) = (wd * l.ss, hd * l.ss, l.scale * l.ss as f32);
    let scr = |v: [f32; 3]| [(v[0] - x0) * k, (y1 - dot(v, up)) * k, dot(v, fw)];

    // visibility: nearest (part, triangle, barycentrics) per sample; shadow: coverage
    let (mut depth, mut hit) = (vec![f32::INFINITY; ws * hs], vec![(u32::MAX, 0u32, 0f32, 0f32); ws * hs]);
    let mut shadow = vec![false; ws * hs];
    for (pi, (part, w)) in parts.iter().zip(&pw).enumerate() {
        let s: Vec<[f32; 3]> = w.iter().map(|&v| scr(v)).collect();
        let sh: Vec<[f32; 3]> = w.iter().map(|&v| scr(shadow_of(v))).collect();
        for (ti, t) in part.2.m.tris.iter().enumerate() {
            raster(&s, t, ws, hs, |o, l1, l2, z| if z < depth[o] { depth[o] = z; hit[o] = (pi as u32, ti as u32, l1, l2); });
            raster(&sh, t, ws, hs, |o, _, _, _| shadow[o] = true);
        }
    }
    // shading per covered sample
    let nw: Vec<Vec<[f32; 3]>> = parts.iter().map(|p| p.1.iter().map(|&n| rz(n)).collect()).collect();
    let half = norm([key[0] - fw[0], key[1] - fw[1], key[2] - fw[2]]);
    let zmax = pw.iter().flatten().map(|v| v[2]).fold(1.0f32, f32::max);
    let mut col = vec![[0f32; 6]; ws * hs]; // rgb, team, height gradient, covered
    for (o, &(pi, ti, l1, l2)) in hit.iter().enumerate() {
        if pi == u32::MAX { continue; }
        let (pi, mesh) = (pi as usize, parts[pi as usize].2);
        let t = mesh.m.tris[ti as usize].map(|i| i as usize);
        let bw = [l1, l2, 1.0 - l1 - l2];
        let lerp3 = |a: &[[f32; 3]]| -> [f32; 3] { std::array::from_fn(|c| (0..3).map(|j| a[t[j]][c] * bw[j]).sum()) };
        let uv: [f32; 2] = std::array::from_fn(|c| (0..3).map(|j| mesh.m.uv[t[j]][c] * bw[j]).sum());
        let mut n = norm(lerp3(&nw[pi]));
        if dot(n, fw) > 0.0 { n = n.map(|v| -v); } // double-sided
        let z = lerp3(&pw[pi])[2];
        let (u, v) = ((uv[0].rem_euclid(1.0) * mesh.tw as f32) as usize, (uv[1].rem_euclid(1.0) * mesh.th as f32) as usize);
        let c = mesh.tex[v.min(mesh.th - 1) * mesh.tw + u.min(mesh.tw - 1)];
        let shade = l.amb * (1.0 - l.hemi * (0.5 - 0.5 * n[2])) + l.dif * dot(n, key).clamp(0.0, 1.0);
        let gloss = l.spec * dot(n, half).clamp(0.0, 1.0).powf(l.spec_exp) * (0.4 + 0.6 * (c[0] + c[1] + c[2]) / 3.0);
        let rgb = [0, 1, 2].map(|i| (c[i] * shade + gloss).clamp(0.0, 1.0).powf(l.gamma));
        col[o] = [rgb[0], rgb[1], rgb[2], 1.0 - c[3], 0.92 * (1.0 - (z / zmax).clamp(0.0, 1.0)), 1.0];
    }
    // box-filter down to output px; the shadow (outside the body) blurs at output size
    let n = (l.ss * l.ss) as f32;
    let mut acc = vec![[0f32; 6]; wd * hd];
    let mut sh = vec![0f32; wd * hd];
    for ys in 0..hs { for xs in 0..ws {
        let (o, d) = (ys * ws + xs, (ys / l.ss) * wd + xs / l.ss);
        let c = col[o];
        for i in 0..5 { acc[d][i] += c[i] * c[5] / n; }
        acc[d][5] += c[5] / n;
        if shadow[o] && c[5] == 0.0 { sh[d] += 1.0 / n; }
    } }
    blur(&mut sh, wd, hd, 1, l.shadow_blur);
    let (mut rgba, mut mask) = (vec![0u8; wd * hd * 4], vec![0u8; wd * hd * 4]);
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    for (i, a) in acc.iter().enumerate() {
        let s = sh[i] * l.shadow * (1.0 - a[5]);
        let alpha = a[5] + s;
        if alpha <= 0.0 { continue; }
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[b(a[0] / alpha), b(a[1] / alpha), b(a[2] / alpha), b(alpha)]);
        mask[i * 4..i * 4 + 4].copy_from_slice(&[b(a[3] / alpha), b((a[4] + 0.75 * s) / alpha), 0, b(alpha)]);
    }
    Frame { w: wd, h: hd, rgba, mask, anchor: [-x0 * l.scale, y1 * l.scale] }
}

/// Calls `f(sample index, l1, l2, depth)` for every sample centre inside triangle `t` of screen points `s`.
fn raster(s: &[[f32; 3]], t: &[u32; 3], ws: usize, hs: usize, mut f: impl FnMut(usize, f32, f32, f32)) {
    let [a, b, c] = t.map(|i| s[i as usize]);
    let den = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if den.abs() < 1e-9 { return; }
    let (x0, x1) = (a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize, (a[0].max(b[0]).max(c[0]).ceil() as isize).min(ws as isize - 1));
    let (y0, y1) = (a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize, (a[1].max(b[1]).max(c[1]).ceil() as isize).min(hs as isize - 1));
    if x1 < x0 as isize || y1 < y0 as isize { return; }
    for y in y0..=y1 as usize {
        let gy = y as f32 + 0.5;
        for x in x0..=x1 as usize {
            let gx = x as f32 + 0.5;
            let l1 = ((b[1] - c[1]) * (gx - c[0]) + (c[0] - b[0]) * (gy - c[1])) / den;
            let l2 = ((c[1] - a[1]) * (gx - c[0]) + (a[0] - c[0]) * (gy - c[1])) / den;
            if l1 < 0.0 || l2 < 0.0 || l1 + l2 > 1.0 { continue; }
            f(y * ws + x, l1, l2, l1 * a[2] + l2 * b[2] + (1.0 - l1 - l2) * c[2]);
        }
    }
}
