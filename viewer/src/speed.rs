//! Single-player game speeds above the original's 90 ticks/s (live::speed_step: 120, 160, 250, Max), and how the sim
//! thread (live::Thread) keeps the UI fed at them. Every tick still runs the whole DLL_RunTick, draw pass included:
//! the pass writes sim memory (unit / building draw fields, tile edge bits, the first-enemy-sighting latch), so
//! skipping it would change the game. Only the hand-over is decoupled: above 90 the ticks' records go to the UI as one
//! Frame per display frame (`FRAME`), HudState and radar built once per Frame instead of once per tick.
use crate::replay::Rec;
use std::time::{Duration, Instant};

/// "Max": no pacing, ticks back to back (the sim thread's own speed).
pub const MAX: f32 = 10000.0;
/// One Frame per this much wall time above 90 ticks/s (a 120 Hz display).
const FRAME: Duration = Duration::from_millis(8);

/// Networked multiplayer (MP state 0x188571b80 = 1) keeps the original's range: at most 90.
pub fn clamp(l: &mut crate::live::Live, rate: f32) -> f32 {
    #[cfg(feature = "live")]
    if l.mem().i32(shc_sim::game::MP_STATE) == 1 { return rate.min(90.0); }
    let _ = l;
    rate
}

/// The ticks held for the next Frame.
#[derive(Default)]
pub struct Merge { recs: Vec<Rec>, sent: Option<Instant> }

impl Merge {
    /// This tick's records (after the held ones) for a Frame now, or None = held: above 90 ticks/s within `FRAME` of
    /// the last Frame, unless `urgent` (a single step, a camera jump).
    pub fn take(&mut self, recs: Vec<Rec>, rate: f32, urgent: bool) -> Option<Vec<Rec>> {
        if self.recs.is_empty() { self.recs = recs } else { self.recs.extend(recs) }
        let now = Instant::now();
        if rate > 90.0 && !urgent && self.sent.is_some_and(|t| now - t < FRAME) { return None; }
        self.sent = Some(now);
        Some(std::mem::take(&mut self.recs))
    }
}

#[test]
fn merges_only_above_90() {
    let r = |k: i16| vec![[k; 21]];
    let mut m = Merge::default();
    assert_eq!(m.take(r(1), 90.0, false).map(|v| v.len()), Some(1));
    assert_eq!(m.take(r(2), 90.0, false).map(|v| v.len()), Some(1)); // <= 90: every tick
    assert_eq!(m.take(r(3), MAX, false).map(|v| v.len()), None); // within 8 ms: held
    assert_eq!(m.take(r(4), MAX, false).map(|v| v.len()), None);
    assert_eq!(m.take(r(5), MAX, true).map(|v| v.iter().map(|r| r[0]).collect::<Vec<_>>()), Some(vec![3, 4, 5])); // in order
    std::thread::sleep(FRAME);
    assert_eq!(m.take(r(6), MAX, false).map(|v| v.len()), Some(1));
}
