//! A custom lord's viewer-side reactions (our extension; re/LORD_LINDSCHIED.md §7b): on top of the lines the sim's AI
//! says (custom_lords, its "*<p>*SK_<KEY>" speech), he reacts to what this player sees happen: your tower or gate
//! falls, your farms burn, your lord takes a hit, he loses a tower, his lord takes a hit, his siege sets out for you,
//! as your ally you ignore his request for goods, you click his face in the allies panel or send your allies at him.
//! A reaction = a short clip over the minimap (`react_<clip>.webm` in his CustomMedia, else his fallback line's clip,
//! `custom_lords::video_stem`) + his line (`REACT_<KEY>.wav` on speech channel 3, the text from CustomMedia's
//! `react.txt`, `KEY ="..."`, in the message bar). A lord without the react.txt line never reacts to it, so DE's
//! lords and older custom lords stay as they were. One at a time, `GAP` between any two, `AGAIN` before the same one.
//! Nothing here writes to the sim: `watch` only reads memory (sim thread), `frame` plays on the UI thread, the
//! engine's own videos and lines go first. LAN safe: each peer sees its own.
use std::{sync::Mutex, time::{Duration, Instant}};

/// (reaction, the line whose clip it plays: `REACT_<x>` = its own `react_<x>.webm`, its mood's when missing).
const KEYS: [(&str, &str); 9] = [
    ("BREACH", "REACT_BREACH"), ("FARMS", "REACT_FARMS"), ("LORD_HIT", "REACT_LORD_HIT"), ("TOWER_LOST", "REACT_TOWER_LOST"),
    ("HIT", "REACT_HIT"), ("SIEGE", "ABOUT2SIEGE"), ("REFUSED", "NOT_SENDING_GOODS"), ("PORTRAIT", "REACT_PORTRAIT"), ("TARGET", "REACT_PORTRAIT"),
];
const GAP: Duration = Duration::from_secs(20);
const AGAIN: Duration = Duration::from_secs(60);
const WAIT: Duration = Duration::from_secs(5); // a reaction not started by then is dropped
const SHOW: Duration = Duration::from_secs(6); // at least this long on screen (the clips are 5–6 s), or as long as the line
const ASK: Duration = Duration::from_secs(60); // REQUEST_GOODS unanswered this long = REFUSED

#[derive(Default)]
struct St {
    pending: Vec<(usize, &'static str, Instant)>,
    last: Vec<(&'static str, Instant)>, // per reaction key; any = the latest of them
    now: Option<(String, i32, Instant, Duration)>, // clip (.vid file), its serial, start, how long it stays
    serial: i32,
    asked: [Option<Instant>; 9], // his REQUEST_GOODS, not answered yet
}
static ST: Mutex<Option<St>> = Mutex::new(None);
fn st<R>(f: impl FnOnce(&mut St) -> R) -> R { f(ST.lock().unwrap().get_or_insert_with(St::default)) }

/// Player p's lord reacts with `key` (any thread), if he has its line.
pub fn fire(p: usize, key: &str) {
    let Some(&(k, _)) = KEYS.iter().find(|k| k.0 == key) else { return };
    if crate::custom_lords::react_text(p, k).is_some() { st(|s| s.pending.push((p, k, Instant::now()))) }
}

/// May `key` start at `t`: none in the last `GAP`, not the same in the last `AGAIN`.
fn ready(last: &[(&str, Instant)], key: &str, t: Instant) -> bool {
    last.iter().all(|(k, at)| t.duration_since(*at) >= if *k == key { AGAIN } else { GAP })
}

/// hud.rs, every frame (UI thread): the reaction clip to show over the minimap, (.vid file, serial), and the line's text
/// when one starts this frame (for the message bar). `engine` = the engine's video shows (it wins: no clip), `talking` =
/// a line of the engine's is playing (none start meanwhile).
pub fn frame(engine: bool, talking: bool) -> Option<(String, i32, Option<String>)> {
    st(|s| {
        let t = Instant::now();
        if s.now.as_ref().is_some_and(|n| t.duration_since(n.2) > n.3) { s.now = None; }
        s.pending.retain(|p| t.duration_since(p.2) < WAIT);
        let mut text = None;
        if s.now.is_none() && !engine && !talking {
            if let Some(i) = s.pending.iter().position(|p| ready(&s.last, p.1, t)) {
                let (p, k, _) = s.pending.remove(i);
                let clip = KEYS.iter().find(|x| x.0 == k).map_or(k, |x| x.1);
                if let (Some(c), Some(stem)) = (crate::custom_lords::in_game(p), crate::custom_lords::video_stem(p, clip)) {
                    let line = crate::custom_lords::lobby_wav(&c, &format!("REACT_{k}"));
                    crate::audio::custom_line(&c, &format!("REACT_{k}"));
                    // the clip's last frame stays until the line is over: its length from the wav's size (DE's spec,
                    // 44.1 kHz 16-bit mono)
                    let said = line.and_then(|f| std::fs::metadata(f).ok()).map_or(Duration::ZERO, |m| Duration::from_secs_f32(m.len().saturating_sub(44) as f32 / 88200.0 + 0.3));
                    s.last.retain(|l| l.0 != k);
                    s.last.push((k, t));
                    s.serial += 1;
                    s.now = Some((format!("{stem}.vid"), s.serial, t, said.clamp(SHOW, Duration::from_secs(12))));
                    text = crate::custom_lords::react_text(p, k);
                    println!("lord reaction {k} (player {p}): {stem}");
                }
            }
        }
        s.now.as_ref().filter(|_| !engine).map(|n| (n.0.clone(), n.1, text))
    })
}

/// live.rs, before the sim's sounds go to the UI thread: an AI's REQUEST_GOODS starts its REFUSED clock.
#[cfg(feature = "live")]
pub fn heard(v: &[shc_sim::sound::Sound]) {
    for s in v {
        if let shc_sim::sound::Sound::Named(n) = s {
            if let Some((p, _)) = crate::custom_lords::split(n).filter(|x| x.1 == "REQUEST_GOODS" && (1..9).contains(&x.0)) { st(|s| s.asked[p] = Some(Instant::now())) }
        }
    }
}
/// live.rs: the local player sent goods to `p` (Ally_SendGoods, game action 1063).
pub fn gave(p: i32) { if (1..9).contains(&p) { st(|s| s.asked[p as usize] = None) } }

/// What `watch` compares between two looks, for lord `d` against the local player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Seen {
    ally: bool,
    farms: i32, defs: i32, lost: i32,      // the local player's farms, towers + gatehouses, buildings lost
    his_defs: i32, his_lost: i32,          // the lord's towers + gatehouses, buildings lost
    hp: Option<(i32, i32)>, his_hp: Option<(i32, i32)>, // the lords' hit points (now, max), alive
    attack: i32, target: i32,              // his attack state (0 idle) and siege target
}

/// The reactions between look `o` and look `s` (the local player = `me`).
fn changes(o: &Seen, s: &Seen, me: i32) -> Vec<&'static str> {
    let mut v = vec![];
    let hit = |a: Option<(i32, i32)>, b: Option<(i32, i32)>| matches!((a, b), (Some((h0, m)), Some((h1, _))) if h0 - h1 >= (m / 20).max(1));
    if s.ally { return v; }
    if s.lost > o.lost && s.farms < o.farms { v.push("FARMS"); }
    if s.lost > o.lost && s.defs < o.defs { v.push("BREACH"); }
    if s.his_lost > o.his_lost && s.his_defs < o.his_defs { v.push("TOWER_LOST"); }
    if hit(o.hp, s.hp) { v.push("LORD_HIT"); }
    if hit(o.his_hp, s.his_hp) { v.push("HIT"); }
    if o.attack == 0 && s.attack != 0 && s.target == me { v.push("SIEGE"); }
    v
}

#[cfg(feature = "live")]
mod look {
    use shc_sim::{buildings as b, mem::Mem};
    const LOST: u64 = 0x1887ec040; // i32 per player: buildings lost (buildings/fire.rs)
    const TEAM: u64 = 0x1837ebecc;
    fn p(q: i32) -> u64 { shc_sim::game::player(q as u32) }
    /// Player q's lord's (hp, max), alive (military::lord_alive).
    fn lord_hp(m: &Mem, q: i32) -> Option<(i32, i32)> {
        let u = m.i16(0x183a0d980 + q as u64 * 2) as i32;
        let r = shc_sim::units::u(u);
        (u != 0 && m.i32(r + 0x98) == m.i32(0x183a0d994 + q as u64 * 4) && m.i16(r + 0x8c) == 2 && m.i16(r + 0x2a0) == 0).then(|| (m.i32(r + 0x3c8), m.i32(r + 0x3cc)))
    }
    /// Per player: (farms, towers + gatehouses), standing buildings only.
    pub fn counts(m: &Mem) -> [[i32; 2]; 9] {
        let mut c = [[0; 2]; 9];
        for i in 1..b::SLOTS {
            let (o, k, s) = (b::owner(m, i), b::kind(m, i), b::state(m, i));
            if !(1..9).contains(&o) || s == 0 || s == 3 { continue; }
            match k { 30..=33 => c[o as usize][0] += 1, 45..=47 | 74..=78 => c[o as usize][1] += 1, _ => {} }
        }
        c
    }
    pub fn seen(m: &Mem, c: &[[i32; 2]; 9], d: i32, me: i32) -> super::Seen {
        super::Seen {
            ally: m.i32(TEAM + d as u64 * 4) == m.i32(TEAM + me as u64 * 4),
            farms: c[me as usize][0], defs: c[me as usize][1], lost: m.i32(LOST + me as u64 * 4),
            his_defs: c[d as usize][1], his_lost: m.i32(LOST + d as u64 * 4),
            hp: lord_hp(m, me), his_hp: lord_hp(m, d),
            attack: m.i32(p(d) + 0x2ba4), target: m.i32(p(d) + 0x2bd8),
        }
    }
}

/// live.rs, after every sim tick (sim thread, read only): every 50 ticks each custom lord's look against the last
/// one; a new game (tick back to 0) starts over.
#[cfg(feature = "live")]
pub fn watch(m: &shc_sim::mem::Mem, me: i32, tick: i32) {
    static W: Mutex<(i32, [Option<Seen>; 9])> = Mutex::new((0, [None; 9]));
    let mut w = W.lock().unwrap();
    if tick < w.0 { w.1 = [None; 9]; }
    if tick < w.0 + 50 && w.0 != 0 { return; }
    w.0 = tick;
    if !(1..9).contains(&me) { return; }
    let lords: Vec<i32> = (1..9).filter(|&d| d != me && crate::custom_lords::in_game(d as usize).is_some()).collect();
    if lords.is_empty() { return; }
    // SHC_REACT=KEY (screenshots): player 2's lord shows that reaction once, ~10 s in (his clips exported by then)
    static DEV: std::sync::Once = std::sync::Once::new();
    if tick > 500 { if let Ok(k) = std::env::var("SHC_REACT") { DEV.call_once(|| fire(2, &k)) } }
    let c = look::counts(m);
    for d in lords {
        let s = look::seen(m, &c, d, me);
        if let Some(o) = w.1[d as usize] { for k in changes(&o, &s, me) { fire(d as usize, k) } }
        w.1[d as usize] = Some(s);
        let late = st(|x| x.asked[d as usize].is_some_and(|a| a.elapsed() > ASK).then(|| x.asked[d as usize] = None)).is_some();
        if late && s.ally { fire(d as usize, "REFUSED") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_he_reacts_to() {
        let o = Seen { farms: 6, defs: 4, lost: 10, his_defs: 8, his_lost: 3, hp: Some((900, 1000)), his_hp: Some((1800, 1800)), ..Default::default() };
        assert!(changes(&o, &o, 1).is_empty());
        // your farm and a tower fall to him; he loses a tower; both lords hit; his siege sets out for you (player 1)
        let s = Seen { farms: 5, defs: 3, lost: 12, his_defs: 7, his_lost: 4, hp: Some((800, 1000)), his_hp: Some((1700, 1800)), attack: 1, target: 1, ..o };
        assert_eq!(changes(&o, &s, 1), ["FARMS", "BREACH", "TOWER_LOST", "LORD_HIT", "HIT", "SIEGE"]);
        // you tear down your own farm (nothing lost), a scratch under 5 %, his siege goes for someone else, dead lords
        let s = Seen { farms: 5, hp: Some((860, 1000)), attack: 1, target: 3, his_hp: None, ..o };
        assert!(changes(&o, &s, 1).is_empty());
        assert!(changes(&o, &Seen { ally: true, farms: 0, lost: 99, ..o }, 1).is_empty(), "an ally does not gloat");
    }

    #[test]
    fn one_at_a_time() {
        let t = Instant::now();
        let ago = |s| t - Duration::from_secs(s);
        assert!(ready(&[], "FARMS", t));
        assert!(!ready(&[("BREACH", ago(5))], "FARMS", t), "within GAP of any");
        assert!(ready(&[("BREACH", ago(25))], "FARMS", t));
        assert!(!ready(&[("FARMS", ago(25))], "FARMS", t), "the same one waits AGAIN");
        assert!(ready(&[("FARMS", ago(61))], "FARMS", t));
    }
}
