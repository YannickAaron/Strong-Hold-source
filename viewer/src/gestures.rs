//! Trackpad (macOS): pinch = smooth zoom about the fingers inside the original's in-game zoom range, two-finger scroll
//! = pan (Cam::limit keeps the original's camera limits every frame), two-finger rotate = the Q / E map turn. Mouse
//! wheels (line deltas) keep the original's stepped zoom in main.rs. Nothing here reaches the sim.

use crate::{zoom_pos_clamp, Cam, ZOOM_LEVELS};

/// Degrees of two-finger rotation per 90° map turn.
pub const TURN_DEG: f32 = 45.0;

impl Cam {
    /// Pinch by `mag` (NSEvent.magnification: +0.1 = 10 % bigger) about `at` (logical px), zoom kept in `range`.
    pub(crate) fn pinch(&mut self, mag: f32, at: [f32; 2], range: (f32, f32)) {
        let nz = (self.zoom * (1.0 + mag)).clamp(range.0, range.1);
        for k in 0..2 { self.off[k] = at[k] - (at[k] - self.off[k]) * nz / self.zoom; }
        self.zoom = nz;
        self.zoom_anim = None; // a wheel / key glide in flight would fight the fingers
    }

    /// Two-finger scroll: the map follows the fingers by `d` logical px.
    pub(crate) fn pan(&mut self, d: [f32; 2]) {
        self.off[0] += d[0];
        self.off[1] += d[1];
        self.zoom_anim = None;
    }
}

/// In-game cam.zoom range for a `w`×`h` device-px window (`sf` device px per logical px) on a map of `size`: the
/// original's zoomPos range (adjustZoom, CanUserExtraZoom) as scales, ZOOM_LEVELS[pos × 2] / sf. Cam::limit's capZoom
/// still applies on top.
pub fn zoom_range(w: f32, h: f32, sf: f32, size: f32) -> (f32, f32) {
    let lo = zoom_pos_clamp(0.0, w, h, size);
    (ZOOM_LEVELS[(lo * 2.0) as usize] / sf, ZOOM_LEVELS[6] / sf)
}

/// The zoomPos whose level is nearest (in ratio) to device scale `scale`, within `lo..=3`: wheel / key steps after a
/// pinch go on from there.
pub fn nearest_pos(scale: f32, lo: f32) -> f32 {
    let (i, _) = ZOOM_LEVELS.iter().enumerate().take(7).skip((lo * 2.0) as usize)
        .map(|(i, z)| (i, (z / scale).ln().abs())).min_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    i as f32 / 2.0
}

/// Two-finger rotation, accumulated: one map turn per TURN_DEG.
#[derive(Default)]
pub struct Turn(f32);

impl Turn {
    /// A RotationGesture delta in degrees (+ = counter-clockwise). Some(true) = turn left (MapRotateLeft, Q: the map
    /// turns counter-clockwise on screen, rot + 2), Some(false) = right (E).
    pub fn feed(&mut self, deg: f32) -> Option<bool> {
        self.0 += deg;
        if self.0.abs() < TURN_DEG { return None; }
        let left = self.0 > 0.0;
        self.0 -= TURN_DEG.copysign(self.0);
        Some(left)
    }
    pub fn reset(&mut self) { self.0 = 0.0; }
}

#[test]
fn pinch_pan_rotate() {
    // Retina 3456×2234 on an 800 map: zoomPos 2..3 (no ExtraZoom over 2560×1440) = scales 0.5..1.0
    let r = zoom_range(3456.0, 2234.0, 2.0, 800.0);
    assert_eq!(r, (0.25, 0.5));
    assert_eq!(zoom_range(1920.0, 1080.0, 1.0, 800.0), (0.25, 1.0)); // ExtraZoom: down to zoomPos 1
    let mut c = Cam { zoom: 0.5, off: [100.0, 50.0], ..Default::default() };
    let at = [800.0, 400.0];
    let world = |c: &Cam| [(at[0] - c.off[0]) / c.zoom, (at[1] - c.off[1]) / c.zoom];
    let before = world(&c);
    c.pinch(-0.2, at, r); // fingers together: zoom out about the fingers
    assert!((c.zoom - 0.4).abs() < 1e-6);
    let after = world(&c);
    assert!((before[0] - after[0]).abs() < 1e-3 && (before[1] - after[1]).abs() < 1e-3, "point under the fingers moved");
    for _ in 0..50 { c.pinch(-0.2, at, r); }
    assert_eq!(c.zoom, 0.25); // stops at the original's widest level
    for _ in 0..50 { c.pinch(0.3, at, r); }
    assert_eq!(c.zoom, 0.5); // and at its closest (1 game px per device px)
    // after a pinch, wheel steps go on from the nearest level
    assert_eq!((nearest_pos(0.9, 2.0), nearest_pos(0.83, 2.0)), (3.0, 2.5)); // by ratio: 0.83 is nearer 0.7 than 1.0
    assert_eq!(nearest_pos(0.6, 2.0), 2.5);
    assert_eq!(nearest_pos(0.3, 2.0), 2.0); // never below the window's lowest
    assert_eq!(nearest_pos(0.3, 1.0), 1.5);
    let off = c.off;
    c.pan([10.0, -4.0]);
    assert_eq!(c.off, [off[0] + 10.0, off[1] - 4.0]);
    // rotation: one turn per 45°, direction by sign, leftover carried
    let mut t = Turn::default();
    assert_eq!((t.feed(30.0), t.feed(20.0), t.feed(-60.0), t.feed(-30.0), t.feed(-10.0)), (None, Some(true), Some(false), None, Some(false)));
}
