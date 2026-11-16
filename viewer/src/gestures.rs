//! Trackpad (macOS): pinch = smooth zoom about the fingers inside the original's in-game zoom range, two-finger scroll
//! = pan (Cam::limit keeps the original's camera limits every frame), two-finger rotate = the Q / E map turn. Mouse
//! wheels (line deltas, and precise deltas outside a touch gesture: `pans`) keep the original's stepped zoom in
//! main.rs. Nothing here reaches the sim.

use crate::{zoom_pos_clamp, Cam, ZOOM_LEVELS};
use winit::event::TouchPhase;

/// Does a precise-delta scroll event pan? macOS reports precise deltas for the trackpad, the Magic Mouse and mice
/// whose driver smooths the wheel (Logitech Options+ and many Bluetooth mice). Only touch surfaces send a gesture
/// phase (Started … Ended, momentum included): `touch` tracks one, so a smoothed mouse wheel falls through to the
/// original's stepped zoom. `option` = View > Trackpad Scrolling Pans; off, touch scrolls zoom like the original's.
pub fn pans(phase: TouchPhase, option: bool, touch: &mut bool) -> bool {
    match phase { TouchPhase::Started => *touch = true, TouchPhase::Ended | TouchPhase::Cancelled => *touch = false, TouchPhase::Moved => {} }
    option && (*touch || phase == TouchPhase::Ended)
}

#[test]
fn touch_scrolls_pan_mouse_wheels_zoom() {
    use TouchPhase::*;
    let mut t = false;
    // a trackpad gesture: pans from the first event to the last, including the momentum tail
    assert_eq!([Started, Moved, Moved, Ended].map(|p| pans(p, true, &mut t)), [true; 4]);
    assert!(!t);
    // a smoothed mouse wheel: precise deltas, never a phase: zoom
    assert_eq!([Moved, Moved].map(|p| pans(p, true, &mut t)), [false; 2]);
    // the option off: everything zooms, the gesture is still tracked
    assert_eq!([Started, Moved, Ended].map(|p| pans(p, false, &mut t)), [false; 3]);
    assert!(!t);
}

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

/// Touch screen (iOS) -> the game's mouse, camera and keys. No tool in the hand: tap = click; one finger swiped = the
/// map follows it (and glides on); tap, then press again at once and drag = the selection box. A building in
/// the hand (`Tool::Place`): it shows under the finger, follows it, and is placed where the finger lifts. A wall-type
/// tool (`Tool::Drag`, walls, stairs, moat, ditch): the first touch aims the start like a building and pins it where
/// it lifts (a ring marks it, the finger never hides it); the second touch draws the line from the pin to the finger,
/// lift = build. A two-finger tap drops the pin. With a tool in the hand the map moves with two fingers. The app
/// passes `Tool::None` for a touch that starts on a HUD button: those always click. Two fingers: pan, pinch zoom, twist = the Q / E map turn; a quick two-finger tap = right click (cancel,
/// deselect). Three fingers: Escape (the pause menu: no key for it on a phone).
#[derive(Default)]
pub struct Touches { fingers: Vec<(u64, [f32; 2])>, mode: Mode, glide: [f32; 2], last_tick: f32, turn: Turn, pin: Option<[f32; 2]>, tap: Option<([f32; 2], f32)> }

#[derive(Default, Clone, Copy, PartialEq, Debug)]
enum Mode {
    #[default] Idle,
    /// one finger, no tool, not yet a swipe: (where it went down, a second press right after a tap: box on drag)
    Pending([f32; 2], bool),
    /// one finger moving the map: (last point, last time)
    Pan([f32; 2], f32),
    /// a tool in the hand, the finger aiming it: (where it went down, wall-type tool)
    Aim([f32; 2], bool),
    /// left button held: selection box / wall line
    Press,
    /// two fingers: (when the second went down, moved enough to not be a tap)
    Two(f32, bool),
    /// done with this touch: nothing until every finger is up
    Done,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool { None, Place, Drag }

#[derive(PartialEq, Debug)]
pub enum Touch { Move([f32; 2]), Down, Up, Right, Pan([f32; 2]), Pinch(f32), Turn(bool), Esc }

/// Logical px a finger may wander and still be "still"; the two-finger tap's
/// longest time; glide: how much speed is left after a second, and the slowest glide (px/s).
pub const SLOP: f32 = 10.0;
/// Seconds between a tap and the press that makes a selection box; logical px apart they may be.
pub const DOUBLE: f32 = 0.35;
const SLOP_BOX: f32 = 20.0;
pub const TAP2: f32 = 0.3;
const GLIDE_KEEP: f32 = 0.02;
const GLIDE_MIN: f32 = 20.0;

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 { (a[0] - b[0]).hypot(a[1] - b[1]) }

impl Touches {
    /// Two fingers: (centre, spread, angle in degrees).
    fn two(&self) -> Option<([f32; 2], f32, f32)> {
        let [(_, a), (_, b)] = self.fingers[..] else { return None };
        Some(([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], dist(a, b).max(1.0), (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees()))
    }

    /// A wall's pinned start, while no finger is down.
    pub fn pin(&self) -> Option<[f32; 2]> { self.pin.filter(|_| self.fingers.is_empty()) }
    /// A tool other than a wall-type one in the hand: no pin.
    pub fn tool(&mut self, tool: Tool) { if tool == Tool::Place { self.pin = None } }

    /// A touch event: finger `id` at `p` (logical px), `now` in seconds, `tool` in the hand.
    pub fn feed(&mut self, id: u64, phase: TouchPhase, p: [f32; 2], now: f32, tool: Tool) -> Vec<Touch> {
        use Touch::*;
        let known = self.fingers.iter().position(|f| f.0 == id);
        match phase {
            TouchPhase::Started => {
                if known.is_some() { return vec![]; }
                self.glide = [0.0; 2];
                self.fingers.push((id, p));
                match (self.fingers.len(), self.mode) {
                    (1, _) => match (tool, self.pin.take()) {
                        (Tool::None, pin) => { // a HUD click keeps the pin
                            self.pin = pin;
                            let again = self.tap.take().is_some_and(|(q, t)| now - t < DOUBLE && dist(p, q) < 2.0 * SLOP_BOX);
                            self.mode = Mode::Pending(p, again);
                            vec![]
                        }
                        (Tool::Drag, Some(s)) => { self.mode = Mode::Press; vec![Move(s), Down, Move(p)] }
                        (t, _) => { self.mode = Mode::Aim(p, t == Tool::Drag); vec![Move(p)] }
                    },
                    (2, Mode::Pending(..) | Mode::Pan(..) | Mode::Aim(..)) => { self.mode = Mode::Two(now, false); self.turn.reset(); vec![] }
                    (3, Mode::Two(..)) => { self.mode = Mode::Done; self.pin = None; vec![Esc] }
                    _ => vec![],
                }
            }
            TouchPhase::Moved => {
                let Some(i) = known else { return vec![] };
                let before = self.two();
                self.fingers[i].1 = p;
                match self.mode {
                    Mode::Pending(s, true) if dist(p, s) > SLOP => { self.mode = Mode::Press; vec![Move(s), Down, Move(p)] }
                    Mode::Pending(s, false) if dist(p, s) > SLOP => { self.mode = Mode::Pan(p, now); vec![Pan([p[0] - s[0], p[1] - s[1]])] }
                    Mode::Pan(l, t) => {
                        let (d, dt) = ([p[0] - l[0], p[1] - l[1]], (now - t).max(1e-3));
                        for k in 0..2 { self.glide[k] = 0.5 * self.glide[k] + 0.5 * d[k] / dt; } // smoothed px/s
                        self.mode = Mode::Pan(p, now);
                        vec![Pan(d)]
                    }
                    Mode::Aim(..) => vec![Move(p)],
                    Mode::Press => vec![Move(p)],
                    Mode::Two(t0, moved) => {
                        let (Some((c0, d0, a0)), Some((c1, d1, a1))) = (before, self.two()) else { return vec![] };
                        let mut out = vec![Pan([c1[0] - c0[0], c1[1] - c0[1]]), Move(c1), Pinch(d1 / d0 - 1.0)];
                        let da = (a1 - a0 + 540.0).rem_euclid(360.0) - 180.0;
                        if let Some(left) = self.turn.feed(-da) { out.push(Turn(left)); } // screen y is down: flip
                        self.mode = Mode::Two(t0, moved || dist(c0, c1) > 0.0 && now - t0 > TAP2 || (d1 / d0 - 1.0).abs() > 0.05);
                        out
                    }
                    _ => vec![],
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(i) = known else { return vec![] };
                let c = self.two();
                self.fingers.remove(i);
                let out = match self.mode {
                    Mode::Pending(s, again) => { if !again { self.tap = Some((s, now)); } vec![Move(s), Down, Up] }
                    Mode::Pan(_, t) => { if now - t > 0.1 { self.glide = [0.0; 2]; } vec![] } // stopped before lifting: no glide
                    Mode::Press => { self.mode = Mode::Done; vec![Move(p), Up] }
                    Mode::Aim(_, true) => { self.pin = Some(p); vec![Move(p)] } // the wall's start, pinned
                    Mode::Aim(_, false) => vec![Move(p), Down, Up], // placed where the finger lifts
                    Mode::Two(t0, moved) if !moved && now - t0 < TAP2 => { self.mode = Mode::Done; self.pin = None; vec![Move(c.unwrap().0), Right] }
                    Mode::Two(..) => { self.mode = Mode::Done; vec![] }
                    _ => vec![],
                };
                if self.fingers.is_empty() { self.mode = Mode::Idle; }
                out
            }
        }
    }

    /// Every frame: a released swipe glides on.
    pub fn tick(&mut self, now: f32) -> Vec<Touch> {
        use Touch::*;
        let dt = (now - self.last_tick).clamp(0.0, 0.1);
        self.last_tick = now;
        match self.mode {
            Mode::Idle if self.glide[0].hypot(self.glide[1]) > GLIDE_MIN => {
                let d = [self.glide[0] * dt, self.glide[1] * dt];
                let k = GLIDE_KEEP.powf(dt);
                self.glide = [self.glide[0] * k, self.glide[1] * k];
                vec![Pan(d)]
            }
            _ => vec![],
        }
    }
}

/// A ring at `at` (screen px), `f` full (a wall's pinned start: 1): a dark track and a light arc from 12 o'clock.
pub fn ring(out: &mut Vec<crate::hud::HudTri>, at: [f32; 2], f: f32) {
    const N: usize = 32;
    let (r0, r1) = (26.0, 33.0);
    let pt = |a: f32, r: f32| [at[0] + r * a.sin(), at[1] - r * a.cos()];
    for (n, colour) in [(N, [0.0, 0.0, 0.0, 0.45]), (((N as f32) * f).ceil() as usize, [1.0, 0.9, 0.55, 0.95])] {
        for k in 0..n {
            let a0 = k as f32 / N as f32 * std::f32::consts::TAU;
            let a1 = ((k + 1) as f32 / N as f32).min(if colour[3] > 0.9 { f } else { 1.0 }) * std::f32::consts::TAU;
            crate::hud::Ui::fill_convex(out, &[pt(a0, r0), pt(a0, r1), pt(a1, r1), pt(a1, r0)], colour);
        }
    }
}

#[test]
fn touch_controls() {
    use Touch::*;
    use TouchPhase::*;
    use Tool::{None as No, Place, Drag};
    let mut t = Touches::default();
    // tap: a click where it went down, on the lift
    assert_eq!(t.feed(1, Started, [100.0, 50.0], 0.0, No), vec![]);
    assert_eq!(t.feed(1, Moved, [103.0, 52.0], 0.05, No), vec![]); // a wobble is no swipe
    assert_eq!(t.feed(1, Ended, [103.0, 52.0], 0.1, No), vec![Move([100.0, 50.0]), Down, Up]);
    // swipe: the map follows the finger, then glides on and slows down
    t.feed(1, Started, [0.0, 0.0], 1.0, No);
    assert_eq!(t.feed(1, Moved, [20.0, 0.0], 1.02, No), vec![Pan([20.0, 0.0])]);
    assert_eq!(t.feed(1, Moved, [40.0, 0.0], 1.04, No), vec![Pan([20.0, 0.0])]);
    assert_eq!(t.feed(1, Ended, [40.0, 0.0], 1.05, No), vec![]);
    t.tick(1.05);
    let Pan(a) = t.tick(1.07).remove(0) else { panic!() };
    let Pan(b) = t.tick(1.09).remove(0) else { panic!() };
    assert!(a[0] > b[0] && b[0] > 0.0, "{a:?} {b:?}");
    for k in 0..200 { t.tick(1.1 + k as f32 * 0.02); }
    assert_eq!(t.tick(6.0), vec![]); // stops
    // a finger on the gliding map stops it
    t.feed(1, Started, [0.0, 0.0], 7.0, No); t.feed(1, Moved, [30.0, 0.0], 7.01, No); t.feed(1, Ended, [30.0, 0.0], 7.02, No);
    t.feed(2, Started, [0.0, 0.0], 7.05, No);
    assert_eq!(t.tick(7.06), vec![]);
    t.feed(2, Ended, [0.0, 0.0], 7.07, No); // (a tap)
    // tap, then press again at once and drag: the selection box from the second press
    assert_eq!(t.feed(1, Started, [200.0, 200.0], 7.5, No), vec![]);
    assert_eq!(t.feed(1, Ended, [200.0, 200.0], 7.55, No), vec![Move([200.0, 200.0]), Down, Up]);
    assert_eq!(t.feed(1, Started, [205.0, 202.0], 7.7, No), vec![]);
    assert_eq!(t.feed(1, Moved, [260.0, 250.0], 7.75, No), vec![Move([205.0, 202.0]), Down, Move([260.0, 250.0])]);
    assert_eq!(t.feed(1, Ended, [260.0, 250.0], 7.8, No), vec![Move([260.0, 250.0]), Up]);
    // too late for a box: a swipe moves the map
    t.feed(1, Started, [0.0, 0.0], 7.85, No); t.feed(1, Ended, [0.0, 0.0], 7.86, No);
    t.feed(1, Started, [0.0, 0.0], 8.3, No);
    assert_eq!(t.feed(1, Moved, [30.0, 0.0], 8.35, No), vec![Pan([30.0, 0.0])]);
    t.feed(1, Ended, [30.0, 0.0], 8.5, No); t.tick(9.0); for k in 0..300 { t.tick(9.0 + k as f32 * 0.02); }
    // holding still does nothing; the lift is a tap
    t.feed(1, Started, [10.0, 10.0], 18.0, No);
    assert_eq!(t.tick(19.0), vec![]);
    assert_eq!(t.feed(1, Ended, [10.0, 10.0], 19.1, No), vec![Move([10.0, 10.0]), Down, Up]);
    // a building: shown under the finger, follows it, placed where it lifts; no waiting
    assert_eq!(t.feed(1, Started, [0.0, 0.0], 9.0, Place), vec![Move([0.0, 0.0])]);
    assert_eq!(t.feed(1, Moved, [50.0, 0.0], 9.1, Place), vec![Move([50.0, 0.0])]);
    assert_eq!(t.tick(11.0), vec![]); // holding does nothing
    assert_eq!(t.feed(1, Ended, [50.0, 0.0], 11.1, Place), vec![Move([50.0, 0.0]), Down, Up]);
    // a wall: the first touch aims and pins the start (nothing built), the second draws from the pin, lift builds
    assert_eq!(t.feed(1, Started, [0.0, 0.0], 12.0, Drag), vec![Move([0.0, 0.0])]);
    assert_eq!(t.feed(1, Moved, [30.0, 0.0], 12.1, Drag), vec![Move([30.0, 0.0])]);
    assert_eq!(t.feed(1, Ended, [30.0, 0.0], 12.2, Drag), vec![Move([30.0, 0.0])]);
    assert_eq!(t.pin(), Some([30.0, 0.0]));
    assert_eq!(t.feed(1, Started, [90.0, 0.0], 12.5, Drag), vec![Move([30.0, 0.0]), Down, Move([90.0, 0.0])]);
    assert_eq!(t.pin(), None);
    assert_eq!(t.feed(1, Moved, [120.0, 40.0], 12.6, Drag), vec![Move([120.0, 40.0])]);
    assert_eq!(t.feed(1, Ended, [120.0, 40.0], 12.7, Drag), vec![Move([120.0, 40.0]), Up]);
    // a HUD button tapped with a pin: it clicks, the pin stays; a two-finger tap drops it
    t.feed(1, Started, [5.0, 5.0], 13.0, Drag); t.feed(1, Ended, [5.0, 5.0], 13.1, Drag);
    assert_eq!(t.feed(1, Started, [500.0, 300.0], 13.2, No), vec![]);
    assert_eq!(t.feed(1, Ended, [500.0, 300.0], 13.25, No), vec![Move([500.0, 300.0]), Down, Up]);
    assert_eq!(t.pin(), Some([5.0, 5.0]));
    t.feed(3, Started, [0.0, 0.0], 13.4, Drag); t.feed(4, Started, [100.0, 0.0], 13.42, Drag);
    t.feed(3, Ended, [0.0, 0.0], 13.5, Drag); t.feed(4, Ended, [100.0, 0.0], 13.51, Drag);
    assert_eq!(t.pin(), None);
    // a building tool drops a pin
    t.feed(1, Started, [5.0, 5.0], 13.6, Drag); t.feed(1, Ended, [5.0, 5.0], 13.7, Drag);
    t.tool(Place);
    assert_eq!(t.pin(), None);
    // two-finger tap: a right click between the fingers
    t.feed(3, Started, [0.0, 0.0], 13.8, No); t.feed(4, Started, [100.0, 0.0], 13.82, No);
    assert_eq!(t.feed(3, Ended, [0.0, 0.0], 13.95, No), vec![Move([50.0, 0.0]), Right]);
    assert_eq!(t.feed(4, Ended, [100.0, 0.0], 13.96, No), vec![]);
    // two fingers moving: pan + pinch, no click; a twist turns the map once per TURN_DEG
    t.feed(3, Started, [0.0, 0.0], 14.0, No); t.feed(4, Started, [100.0, 0.0], 14.0, No);
    assert_eq!(t.feed(4, Moved, [200.0, 0.0], 14.1, No), vec![Pan([50.0, 0.0]), Move([100.0, 0.0]), Pinch(1.0)]);
    let out = t.feed(4, Moved, [0.0, 200.0], 14.2, No); // 90° clockwise on screen
    assert!(out.contains(&Turn(false)), "{out:?}");
    assert_eq!(t.feed(3, Ended, [0.0, 0.0], 14.5, No), vec![]);
    assert_eq!(t.feed(4, Moved, [9.0, 9.0], 14.6, No), vec![]);
    t.feed(4, Ended, [9.0, 9.0], 14.7, No);
    // three fingers: Escape once
    t.feed(5, Started, [0.0, 0.0], 15.0, No); t.feed(6, Started, [10.0, 0.0], 15.0, No);
    assert_eq!(t.feed(7, Started, [20.0, 0.0], 15.0, No), vec![Esc]);
    for id in [5, 6, 7] { assert_eq!(t.feed(id, Ended, [0.0, 0.0], 15.2, No), vec![]); }
}
