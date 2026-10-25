//! A custom lord's viewer-side reactions (our extension; re/LORD_LINDSCHIED.md §7b): on top of the lines the sim's AI
//! says (custom_lords, its "*<p>*SK_<KEY>" speech), he reacts to what this player sees happen: your tower or gate
//! falls, your farms burn, your lord takes a hit, he loses a tower, his lord takes a hit, his siege sets out for you,
//! as your ally you ignore his request for goods, you click his face in the allies panel or send your allies at him,
//! your army reaches his walls, a fight costs him (or you) ten men.
//! A reaction = a short clip over the minimap (`react_<clip>.webm` in his CustomMedia, else his fallback line's clip,
//! `custom_lords::video_stem`) + his line (`REACT_<KEY>.wav` on speech channel 3, the text from CustomMedia's
//! `react.txt`, `KEY ="..."`, in the message bar). A lord without the react.txt line never reacts to it, so DE's
//! lords and older custom lords stay as they were. One at a time, `GAP` between any two, `AGAIN` before the same one.
//! Nothing here writes to the sim: `watch` only reads memory (sim thread), `frame` plays on the UI thread, the
//! engine's own videos and lines go first. LAN safe: each peer sees its own.
use std::{sync::Mutex, time::{Duration, Instant}};

/// (reaction, the line whose clip it plays: `REACT_<x>` = its own `react_<x>.webm`, its mood's when missing).
const KEYS: [(&str, &str); 12] = [
    ("BREACH", "REACT_BREACH"), ("FARMS", "REACT_FARMS"), ("LORD_HIT", "REACT_LORD_HIT"), ("TOWER_LOST", "REACT_TOWER_LOST"),
    ("HIT", "REACT_HIT"), ("SIEGE", "ABOUT2SIEGE"), ("REFUSED", "NOT_SENDING_GOODS"), ("PORTRAIT", "REACT_PORTRAIT"), ("TARGET", "REACT_PORTRAIT"),
    ("APPROACH", "REACT_APPROACH"), ("LOSSES", "ANGRY_SIEGE_LOST"), ("WIN", "BOAST_OF_KILL"),
];
const AT_WALLS: i32 = 6; // your soldiers within `look::NEAR` of his keep = your army at his walls
const BLOODY: i32 = 10; // soldiers lost within `WINDOW` looks = a fight worth a word
const WINDOW: usize = 10; // looks (50 ticks each: 20 s at normal speed)
const GAP: Duration = Duration::from_secs(20);
const AGAIN: Duration = Duration::from_secs(60);
const WAIT: Duration = Duration::from_secs(5); // a small reaction not started by then is dropped
const WAIT_BIG: Duration = Duration::from_secs(30); // a big one waits out a battle's lines and clips
const SHOW: Duration = Duration::from_secs(6); // at least this long on screen (the clips are 5–6 s), or as long as the line
const ASK: Duration = Duration::from_secs(60); // REQUEST_GOODS unanswered this long = REFUSED
/// (reaction, the engine line that already answers the same event): his own "castle damaged" line when you take his
/// tower, his "about to siege" one when his siege sets out. Said by the engine in the last `WAIT_BIG` = not again.
const SAID: [(&str, &str); 2] = [("TOWER_LOST", "ANGRY_CASTLE_DAMAGED"), ("SIEGE", "ABOUT2SIEGE")];

#[derive(Default)]
struct St {
    pending: Vec<(usize, &'static str, Instant)>,
    last: Vec<(&'static str, Instant)>, // per reaction key; any = the latest of them
    now: Option<(String, i32, Instant, Duration)>, // clip (.vid file), its serial, start, how long it stays
    serial: i32,
    asked: [Option<Instant>; 9], // his REQUEST_GOODS, not answered yet
    said: Vec<(usize, String, Instant)>, // the engine's lines of custom lords (player, KEY), the last `WAIT_BIG`
}
static ST: Mutex<Option<St>> = Mutex::new(None);
fn st<R>(f: impl FnOnce(&mut St) -> R) -> R { f(ST.lock().unwrap().get_or_insert_with(St::default)) }

/// Player p's lord reacts with `key` (any thread), if he has its line.
pub fn fire(p: usize, key: &str) {
    let Some(&(k, _)) = KEYS.iter().find(|k| k.0 == key) else { return };
    if crate::custom_lords::react_text(p, k).is_some() { st(|s| s.pending.push((p, k, Instant::now()))) }
}

/// A big reaction (something fell, burned or sets out; you spoke to him): waits up to `WAIT_BIG` for the minimap and
/// goes before the small ones (a lord's scratch), which keep `GAP` and `WAIT`.
fn big(key: &str) -> bool { !matches!(key, "HIT" | "LORD_HIT") }

/// May `key` start at `t`: not the same in the last `AGAIN`; a small one also none in the last `GAP`.
fn ready(last: &[(&str, Instant)], key: &str, t: Instant) -> bool {
    last.iter().all(|(k, at)| t.duration_since(*at) >= if *k == key { AGAIN } else if big(key) { Duration::ZERO } else { GAP })
}

/// Player p's engine line already answered reaction `key` (`SAID`) in `said`.
fn answered(said: &[(usize, String, Instant)], p: usize, key: &str, t: Instant) -> bool {
    SAID.iter().any(|&(k, line)| k == key && said.iter().any(|(q, l, at)| *q == p && l == line && t.duration_since(*at) < WAIT_BIG))
}

/// hud.rs, every frame (UI thread): the reaction clip to show over the minimap, (.vid file, serial), and the line's text
/// when one starts this frame (for the message bar). `engine` = the engine's video shows (it wins: no clip), `talking` =
/// a line of the engine's is playing (none start meanwhile).
pub fn frame(engine: bool, talking: bool) -> Option<(String, i32, Option<String>)> {
    st(|s| {
        let t = Instant::now();
        if s.now.as_ref().is_some_and(|n| t.duration_since(n.2) > n.3) { s.now = None; }
        let said = std::mem::take(&mut s.said);
        s.pending.retain(|p| t.duration_since(p.2) < if big(p.1) { WAIT_BIG } else { WAIT } && !answered(&said, p.0, p.1, t));
        s.said = said;
        let mut text = None;
        if s.now.is_none() && !engine && !talking {
            let go = |p: &(usize, &str, Instant), b: bool| big(p.1) == b && ready(&s.last, p.1, t);
            if let Some(i) = s.pending.iter().position(|p| go(p, true)).or_else(|| s.pending.iter().position(|p| go(p, false))) {
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

/// live.rs, before the sim's sounds go to the UI thread: an AI's REQUEST_GOODS starts its REFUSED clock; every custom
/// lord line is kept a while (`SAID`).
#[cfg(feature = "live")]
pub fn heard(v: &[shc_sim::sound::Sound]) {
    for s in v {
        if let shc_sim::sound::Sound::Named(n) = s {
            let Some((p, key)) = crate::custom_lords::split(n).filter(|x| (1..9).contains(&x.0)) else { continue };
            let t = Instant::now();
            st(|s| {
                if key == "REQUEST_GOODS" { s.asked[p] = Some(t) }
                s.said.retain(|x| t.duration_since(x.2) < WAIT_BIG);
                s.said.push((p, key, t));
            })
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
    near: i32,                             // your soldiers near his keep
    army: i32, my_army: i32,               // his soldiers, yours
    peak: i32, my_peak: i32,               // the most of each in the last `WINDOW` looks
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
    if o.near < AT_WALLS && s.near >= AT_WALLS { v.push("APPROACH"); }
    // a bloody fight: the side that lost more sets his word (his men: a promise; yours: scorn)
    let (lost, mine) = (s.peak - s.army, s.my_peak - s.my_army);
    let crossed = |now: i32, before: i32| now >= BLOODY && before < BLOODY;
    if lost >= mine && crossed(lost, o.peak - o.army) { v.push("LOSSES"); }
    else if mine > lost && crossed(mine, o.my_peak - o.my_army) { v.push("WIN"); }
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
    pub const NEAR: i32 = 30; // tiles (|dx| + |dy|) from a keep
    fn xy(m: &Mem, t: i32) -> (i32, i32) { let y = m.i16(shc_sim::terrain::TILE_Y + t as u64 * 2) as i32; (t - shc_sim::terrain::row_off(m, y), y) }
    /// Per player: (its soldiers: archers .. knights, Arab and Bedouin troops; the local player's soldiers within
    /// `NEAR` of its keep).
    pub fn armies(m: &Mem, me: i32) -> [[i32; 2]; 9] {
        use shc_sim::units;
        let keep = |q: usize| { let t = m.i32(p(q as i32) + 0x1e0); (t > 0).then(|| xy(m, t)) };
        let keeps: Vec<Option<(i32, i32)>> = (0..9).map(|q| if q == 0 { None } else { keep(q) }).collect();
        let mut a = [[0; 2]; 9];
        for i in 1..m.i32(units::OBJ).max(1) {
            let r = units::u(i);
            let (o, k) = (m.i16(r + 0x96) as i32, m.i16(r + 0x8e));
            if m.i16(r + 0x8c) != 2 || !(1..9).contains(&o) || !matches!(k, 22..=28 | 70..=76 | 78..=85) { continue; }
            a[o as usize][0] += 1;
            if o != me { continue; }
            let (x, y) = xy(m, m.i32(r + 0xd4));
            for (q, at) in keeps.iter().enumerate() {
                if let Some((kx, ky)) = at { if (x - kx).abs() + (y - ky).abs() < NEAR { a[q][1] += 1; } }
            }
        }
        a
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
    pub fn seen(m: &Mem, c: &[[i32; 2]; 9], a: &[[i32; 2]; 9], d: i32, me: i32) -> super::Seen {
        super::Seen {
            ally: m.i32(TEAM + d as u64 * 4) == m.i32(TEAM + me as u64 * 4),
            farms: c[me as usize][0], defs: c[me as usize][1], lost: m.i32(LOST + me as u64 * 4),
            his_defs: c[d as usize][1], his_lost: m.i32(LOST + d as u64 * 4),
            hp: lord_hp(m, me), his_hp: lord_hp(m, d),
            attack: m.i32(p(d) + 0x2ba4), target: m.i32(p(d) + 0x2bd8),
            near: a[d as usize][1], army: a[d as usize][0], my_army: a[me as usize][0],
            peak: 0, my_peak: 0, // `watch` fills them from its history
        }
    }
}

/// live.rs, after every sim tick (sim thread, read only): every 50 ticks each custom lord's look against the last
/// one; a new game (tick back to 0) starts over.
#[cfg(feature = "live")]
pub fn watch(m: &shc_sim::mem::Mem, me: i32, tick: i32) {
    // last tick looked at, the last look and the armies of the last `WINDOW` looks per lord
    static W: Mutex<(i32, [Option<Seen>; 9], [Vec<(i32, i32)>; 9])> = Mutex::new((0, [None; 9], [const { Vec::new() }; 9]));
    let mut w = W.lock().unwrap();
    if tick < w.0 { w.1 = [None; 9]; w.2 = Default::default(); }
    if tick < w.0 + 50 && w.0 != 0 { return; }
    w.0 = tick;
    if !(1..9).contains(&me) { return; }
    let lords: Vec<i32> = (1..9).filter(|&d| d != me && crate::custom_lords::in_game(d as usize).is_some()).collect();
    if lords.is_empty() { return; }
    // SHC_REACT=KEY (screenshots): player 2's lord shows that reaction once, ~10 s in (his clips exported by then)
    static DEV: std::sync::Once = std::sync::Once::new();
    if tick > 500 { if let Ok(k) = std::env::var("SHC_REACT") { DEV.call_once(|| fire(2, &k)) } }
    let (c, a) = (look::counts(m), look::armies(m, me));
    for d in lords {
        let mut s = look::seen(m, &c, &a, d, me);
        let h = &mut w.2[d as usize];
        h.push((s.army, s.my_army));
        if h.len() > WINDOW { h.remove(0); }
        (s.peak, s.my_peak) = h.iter().fold((0, 0), |(x, y), &(u, v)| (x.max(u), y.max(v)));
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
        // your army reaches his walls; a fight costs him ten men (more than you); one costs you ten
        let calm = Seen { army: 40, my_army: 40, peak: 40, my_peak: 40, ..o };
        assert_eq!(changes(&calm, &Seen { near: 8, ..calm }, 1), ["APPROACH"]);
        assert_eq!(changes(&calm, &Seen { army: 29, my_army: 35, ..calm }, 1), ["LOSSES"]);
        assert_eq!(changes(&calm, &Seen { army: 38, my_army: 28, ..calm }, 1), ["WIN"]);
        assert!(changes(&Seen { army: 29, ..calm }, &Seen { army: 27, ..calm }, 1).is_empty(), "once per fight, not every look");
        assert!(changes(&calm, &Seen { army: 33, my_army: 34, ..calm }, 1).is_empty(), "a skirmish: under ten each");
    }

    #[test]
    fn one_at_a_time() {
        let t = Instant::now();
        let ago = |s| t - Duration::from_secs(s);
        assert!(ready(&[], "FARMS", t));
        assert!(!ready(&[("BREACH", ago(5))], "HIT", t), "a small one: within GAP of any");
        assert!(ready(&[("BREACH", ago(25))], "HIT", t));
        assert!(ready(&[("HIT", ago(5))], "TOWER_LOST", t), "a big one does not wait out GAP");
        assert!(!ready(&[("FARMS", ago(25))], "FARMS", t), "the same one waits AGAIN");
        assert!(ready(&[("FARMS", ago(61))], "FARMS", t));
    }

    /// `armies` on a real game: Frederick's start troops count, and at the start all of yours stand at home.
    #[cfg(feature = "live")]
    #[test]
    fn armies_in_a_game() {
        let map = crate::skirmish::maps_dir().join("Crater Lake.map");
        if !map.exists() { return; } // needs the game install
        let lobby = crate::skirmish::Lobby { ais: vec![(8, 0)], seed: 1234, ..crate::skirmish::Lobby::new(map) };
        let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
        for _ in 0..600 { l.step(); }
        let a = look::armies(l.mem(), 1);
        assert!(a[2][0] > 0, "Frederick's start troops: {a:?}");
        assert!(a[1][1] == a[1][0] && a[2][1] == 0, "yours at your keep, none at his: {a:?}");
    }

    #[test]
    fn engine_line_answers_it() {
        let t = Instant::now();
        let said = [(2, "ANGRY_CASTLE_DAMAGED".to_string(), t - Duration::from_secs(10))];
        assert!(answered(&said, 2, "TOWER_LOST", t), "his castle-damaged line already said it");
        assert!(!answered(&said, 3, "TOWER_LOST", t), "another lord's line");
        assert!(!answered(&said, 2, "BREACH", t));
        assert!(!answered(&said, 2, "TOWER_LOST", t + WAIT_BIG), "too long ago");
    }
}
