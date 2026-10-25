//! Lord David von Lindschied (re/LORD_LINDSCHIED.md, re/lindschied/): his CustomLords folder loads, and the AI plays
//! his config and castles in a Crater Lake skirmish against Frederick. Headless, through the lobby path
//! (custom_lords::rescan → skirmish::start), as `custom_lord_skirmish` does.
//!   cargo test --release lindschied -- --ignored --nocapture        (needs the game install for the map)
//!   LINDSCHIED_TICKS=30000 LINDSCHIED_CASTLES=1,2 ...                 (default: castle 1 30 000 ticks, 2-8 10 000)
use crate::custom_lords::{self, BASE};
use shc_sim::{ai, buildings as b, mem::Mem};
use std::path::{Path, PathBuf};

const NAME: &str = "Lord David von Lindschied";
const SRC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../re/lindschied");

/// The probes read game state only: the draw pass gets a 2-tile view instead of the whole map (it dominated their
/// time). PROBE_FULL_VIEW=1 keeps the whole map (to check a result does not depend on it).
fn headless(l: &mut crate::live::Live) {
    if std::env::var_os("PROBE_FULL_VIEW").is_none() { l.view = Some([0, 2, 0, 2]); }
}

/// A player folder with CustomLords/<NAME>: the lordjson, the avatar and the castles `ks` (1..8).
fn folder(tag: &str, ks: &[usize]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("shc-lindschied-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lord = dir.join("CustomLords").join(NAME);
    std::fs::create_dir_all(&lord).unwrap();
    let src = Path::new(SRC);
    // LINDSCHIED_LORDJSON: try another config (a tuning variant) instead of the committed one
    let cfg = std::env::var_os("LINDSCHIED_LORDJSON").map(PathBuf::from).unwrap_or_else(|| src.join(format!("{NAME}.lordjson")));
    std::fs::copy(cfg, lord.join(format!("{NAME}.lordjson"))).unwrap();
    std::fs::copy(src.join("avatar.png"), lord.join("avatar.png")).unwrap();
    std::fs::copy(src.join("face.png"), lord.join("face.png")).unwrap();
    for k in ks {
        let f = format!("lindschied_{k}.aivjson");
        let dir = std::env::var_os("LINDSCHIED_CASTLES_DIR").map(PathBuf::from).unwrap_or_else(|| src.join("castles"));
        std::fs::copy(dir.join(&f), lord.join(&f)).unwrap();
    }
    dir
}

/// The shipped and the generated castles: parse, KEEP2 at step 0, no two steps on one tile (with the keep / barracks /
/// guild yards), only kinds and troop slots the 344 shipped castles use.
#[test]
fn lindschied_castles_valid() {
    use shc_formats::aiv;
    let cs = shc_sim::data::aivs();
    let sets: std::collections::BTreeSet<&str> = cs.lines().filter_map(|l| l.split('\t').next()).collect();
    let shipped: Vec<aiv::Aiv> = sets.iter().flat_map(|n| shc_sim::trail::aiv_layouts(cs, n)).map(|d| aiv::parse(&d).unwrap()).collect();
    assert!(shipped.len() >= 344, "{} shipped castles", shipped.len());
    let kinds: std::collections::BTreeSet<i16> = shipped.iter().flat_map(|a| a.steps.iter().map(|s| s.kind)).collect();
    for k in 1..=8 {
        let f = Path::new(SRC).join(format!("castles/lindschied_{k}.aivjson"));
        let d = custom_lords::aivjson(&std::fs::read_to_string(&f).unwrap()).unwrap_or_else(|| panic!("{}", f.display()));
        let a = aiv::parse(&d).unwrap();
        assert_eq!(a.steps[0].kind, 61, "castle {k}: KEEP2 first");
        let odd: Vec<i16> = a.steps.iter().map(|s| s.kind).filter(|k| !kinds.contains(k)).collect();
        assert!(odd.is_empty(), "castle {k}: kinds no shipped castle has: {odd:?}");
        assert!(a.troops.iter().all(|t| [2, 3, 6, 7, 11].contains(&t.slot)), "castle {k}: troop slots"); // 6: the economy towers' archers
        let mut owner = vec![[usize::MAX; aiv::GRID]; aiv::GRID];
        for (i, s) in a.steps.iter().enumerate() {
            let one = aiv::Aiv { steps: vec![s.clone()], ..a.clone() };
            for (y, row) in one.grid().iter().enumerate() {
                for (x, &v) in row.iter().enumerate() {
                    if v == 0 { continue; }
                    assert_eq!(owner[y][x], usize::MAX, "castle {k}: step {i} ({}) on ({x},{y}) of step {}", aiv::kind_name(s.kind), owner[y][x]);
                    owner[y][x] = i;
                }
            }
        }
    }
}

/// The folder as the lobby scans it: one lord, all 8 castles, the avatar, Frederick as the parent. Also the installed
/// one (~/Library/Application Support/Strong Hold/CustomLords/<NAME>) when it is there.
#[test]
fn lindschied_folder_loads() {
    let dir = folder("scan", &[1, 2, 3, 4, 5, 6, 7, 8]);
    for (profile, installed) in [(dir.clone(), false), (crate::paths::app_data(), true)] {
        if installed && !profile.join("CustomLords").join(NAME).is_dir() { continue; }
        crate::skirmish::set_test_profile(profile.clone());
        custom_lords::rescan();
        let k = custom_lords::names().iter().position(|n| n == NAME).expect("found");
        let c = custom_lords::get(k).unwrap();
        assert_eq!((c.aivs.len(), c.avatar.as_ref().map(|a| a.len()), custom_lords::parent(&c)), (8, Some(130 * 130 * 4), 8), "{}", profile.display());
        assert_eq!((c.config[0], c.config[52], c.config[88], c.config[275]), (9, 18, 0, 200), "Frederick's type, crossbows, no moat, hps");
        println!("{}: {} castles, avatar, parent Frederick", profile.display(), c.aivs.len());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// One lord's castle progress and army: (wall ring, wall towers, battlements and economy towers placed / total,
/// archers on the economy towers, AIV fit, step reached, entries placed / total, failed entries' kinds, buildings,
/// crossbowmen, swordsmen, other troops, engines, gold).
#[derive(Debug)]
struct Stats { ring: (usize, usize), towers: (usize, usize), crenels: (usize, usize), eco: (usize, usize), on_eco: usize, fit: i32, reached: i32, placed: usize, total: usize, failed: Vec<i16>, buildings: usize, xbow: usize, sword: usize, other: usize, engines: usize, gold: i32 }

fn stats(m: &Mem, p: i32) -> Stats {
    let r = (1..=8u64).map(|k| ai::D + k * 0x6d98).find(|&r| m.i32(r + 4) == p).expect("AI record");
    let n = m.i32(r + 0x24).max(0) as u64;
    // a stairs entry the AI re-checks fails (state 5) once its tile holds the stairs (ai::build::wall_ok refuses a
    // raised tile): stairs on the tile (flag 0x800) count as placed
    let stair_on = |e: u64| m.u32(shc_sim::terrain::W + 0x898400 + m.i32(e + 8) as u64 * 4) & 0x800 != 0;
    let entries: Vec<(u8, i16)> = (1..=n).map(|e| { let e = r + 0x38 + e * 0xc; (m.u8(e), m.i16(e + 2), e) })
        .filter(|&(_, k, _)| k != 0).map(|(s, k, e)| (if s == 5 && (181..=186).contains(&k) && stair_on(e) { 3 } else { s }, k)).collect();
    let (mut xbow, mut sword, mut other, mut engines, mut on_eco) = (0, 0, 0, 0, 0);
    // the economy towers (TOWER1 entries placed) by building id, for the archers standing on them
    const BLD_AT: u64 = 0x184b689e0;
    let eco: Vec<i16> = (1..=n).map(|e| r + 0x38 + e * 0xc).filter(|&e| m.i16(e + 2) == 110 && m.u8(e) == 3)
        .map(|e| m.i16(BLD_AT + m.i32(e + 8) as u64 * 2)).filter(|&b| b > 0).collect();
    const U: u64 = 0x1867e5a48;
    for i in 1..m.i32(0x1867e53f0).max(1) as u64 {
        let u = U + i * 0x490;
        if m.i16(u + 0x8c) != 2 || m.i16(u + 0x96) as i32 != p { continue; }
        match m.i16(u + 0x8e) {
            23 => xbow += 1,
            27 => sword += 1,
            22 if eco.contains(&m.i16(BLD_AT + m.i32(u + 0xd4) as u64 * 2)) => { on_eco += 1; other += 1 }
            22..=28 => other += 1,
            39..=41 | 50..=60 => engines += 1,
            _ => {}
        }
    }
    Stats {
        ring: (entries.iter().filter(|e| e.0 == 3 && [25, 26, 113, 144, 145].contains(&e.1)).count(), entries.iter().filter(|e| [25, 26, 113, 144, 145].contains(&e.1)).count()),
        towers: (entries.iter().filter(|e| e.0 == 3 && (111..=114).contains(&e.1)).count(), entries.iter().filter(|e| (111..=114).contains(&e.1)).count()),
        eco: (entries.iter().filter(|e| e.0 == 3 && e.1 == 110).count(), entries.iter().filter(|e| e.1 == 110).count()), on_eco,
        crenels: (entries.iter().filter(|e| e.0 == 3 && e.1 == 26).count(), entries.iter().filter(|e| e.1 == 26).count()),
        fit: m.i32(r + 0x14), reached: m.i32(r + 0x18), placed: entries.iter().filter(|e| e.0 == 3).count(), total: entries.len(),
        failed: entries.iter().filter(|e| e.0 == 5).map(|e| e.1).collect(),
        buildings: (1..b::SLOTS).filter(|&i| b::state(m, i) == 2 && b::owner(m, i) == p).count(),
        xbow, sword, other, engines, gold: m.i32(0x183798d60 + p as u64 * 0x583c + 0x50c),
    }
}

/// Crater Lake: you idle as player 1, Lord David with castle k as player 2, Frederick as player 3; `ticks` ticks.
fn play(k: usize, ticks: i32) -> Option<(Stats, Stats)> {
    let map = crate::skirmish::maps_dir().join("Crater Lake.map");
    if !map.exists() { return None; } // needs the game install
    let dir = folder(&format!("game{k}"), &[k]);
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    let lord = BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap();
    let lobby = crate::skirmish::Lobby { ais: vec![(lord, 0), (8, 0)], seed: 1234, ..crate::skirmish::Lobby::new(map) };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    headless(&mut l);
    for t in 0..ticks {
        l.step();
        if t % 5000 == 4999 {
            let s = stats(l.mem(), 2);
            let f = stats(l.mem(), 3);
            eprintln!("  castle {k} tick {}: steps {}/{} ring {}/{} reached {} buildings {} troops {}+{}+{} gold {} | Frederick steps {}/{} troops {} gold {}", t + 1,
                s.placed, s.total, s.ring.0, s.ring.1, s.reached, s.buildings, s.xbow, s.sword, s.other, s.gold, f.placed, f.total, f.xbow + f.sword + f.other, f.gold);
        }
    }
    if std::env::var_os("LINDSCHIED_ENTRIES").is_some() {
        let m = l.mem();
        let r = (1..=8u64).map(|k| ai::D + k * 0x6d98).find(|&r| m.i32(r + 4) == 2).unwrap();
        let es: Vec<String> = (1..=m.i32(r + 0x24).max(0) as u64).map(|e| { let e = r + 0x38 + e * 0xc; format!("{}:{}", m.i16(e + 2), m.u8(e)) }).collect();
        eprintln!("  castle {k} entries kind:state {}", es.join(" "));
        // failed entries: the tile's terrain flags / height / base height / building / unit (ai::build::wall_ok's tests)
        let w = shc_sim::terrain::W;
        for e in 1..=m.i32(r + 0x24).max(0) as u64 {
            let e = r + 0x38 + e * 0xc;
            if m.u8(e) != 5 { continue; }
            let t = m.i32(e + 8) as u64;
            let y = m.i16(0x183aac234 + t * 2) as i64;
            let x = t as i64 - m.i32((0x18402debc + y * 12) as u64) as i64;
            eprintln!("    failed {} at ({x},{y}): flags {:#x} h {} base {} fa {} bld {} unit {}", m.i16(e + 2), m.u32(w + 0x898400 + t * 4),
                m.u8(w + 0xd7e5a0 + t), m.u8(w + 0xdccac0 + t), m.i8(w + 0xfa2980 + t), m.i16(w + 0xb0bca0 + t * 2), m.i16(w + 0xbf6c00 + t * 2));
        }
    }
    custom_lords::set_in_game(2, None);
    let _ = std::fs::remove_dir_all(&dir);
    Some((stats(l.mem(), 2), stats(l.mem(), 3)))
}

#[test]
#[ignore] // minutes: run explicitly
fn lindschied_plays() {
    let env = |k: &str| std::env::var(k).ok();
    let ticks: i32 = env("LINDSCHIED_TICKS").and_then(|v| v.parse().ok()).unwrap_or(30_000);
    let ks: Vec<usize> = env("LINDSCHIED_CASTLES").map(|v| v.split(',').filter_map(|k| k.parse().ok()).collect()).unwrap_or((1..=8).collect());
    println!("castle | ticks | fit | steps placed/total | ring | reached | failed kinds | buildings | xbow sword other engines | gold || Frederick: steps, buildings, troops, gold");
    for k in ks {
        let t = if k == 1 { ticks } else { ticks.min(env("LINDSCHIED_SHORT").and_then(|v| v.parse().ok()).unwrap_or(10_000)) };
        let Some((d, f)) = play(k, t) else { println!("no Crater Lake.map: skipped"); return };
        println!("{k} | {t} | {} | {}/{} | {}/{} | {} | {:?} | {} | {} {} {} {} | {} || {}/{}, {}, {}, {}",
            d.fit, d.placed, d.total, d.ring.0, d.ring.1, d.reached, d.failed, d.buildings, d.xbow, d.sword, d.other, d.engines, d.gold,
            f.placed, f.total, f.buildings, f.xbow + f.sword + f.other, f.gold);
        // the shipped Frederick on the same map builds ~40 of his 107 steps in 30 000 ticks: the bar is his pace, no
        // failed step, most of the wall ring and an army past the 20 he starts with
        if k == 1 {
            // one barracks, inside the walls (make_castles.py): no step may fail
            assert!(d.failed.is_empty() && d.placed >= f.placed && d.ring.0 * 4 >= d.ring.1 * 3, "castle 1 stalled: {d:?} (Frederick {f:?})");
            assert!(d.xbow + d.sword > 20, "no recruits: {d:?}");
        }
    }
}

/// Debug: you + Lord David alone on Crater Lake (the `SHC_AI=1000 --skirmish` game), all 8 castles; player 2's
/// keep (kind 40..44) every 100 ticks. LINDSCHIED_LORD=8: Frederick instead, LINDSCHIED_TICKS (default 2000).
#[test]
#[ignore]
fn lindschied_keep() {
    let map = crate::skirmish::maps_dir().join("Crater Lake.map");
    if !map.exists() { return; }
    let ks: Vec<usize> = std::env::var("LINDSCHIED_CASTLES").map(|v| v.split(',').filter_map(|k| k.parse().ok()).collect()).unwrap_or((1..=8).collect());
    let dir = folder(&format!("keep{}", ks.iter().map(|k| k.to_string()).collect::<String>()), &ks);
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    let lord = std::env::var("LINDSCHIED_LORD").ok().and_then(|v| v.parse().ok())
        .unwrap_or_else(|| BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap());
    let seed = std::env::var("LINDSCHIED_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1234);
    // LINDSCHIED_SETTINGS=1: the lobby as `--skirmish` makes it (Lobby::from_settings: your settings.cfg colour and
    // advanced presets)
    let base = if std::env::var_os("LINDSCHIED_SETTINGS").is_some() {
        crate::skirmish::set_test_profile(crate::paths::app_data());
        let l = crate::skirmish::Lobby::from_settings(map);
        crate::skirmish::set_test_profile(dir.clone());
        l
    } else { crate::skirmish::Lobby::new(map) };
    eprintln!("lobby adv {:?} advanced {}", base.adv, base.advanced);
    let lobby = crate::skirmish::Lobby { ais: vec![(lord, 0)], seed, ..base };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    headless(&mut l);
    let ticks: i32 = std::env::var("LINDSCHIED_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000);
    let keeps = |m: &Mem| -> Vec<(i32, i32, i32, i32)> {
        (1..b::SLOTS).filter(|&i| b::owner(m, i) == 2 && (40..=44).contains(&b::kind(m, i)) && b::state(m, i) != 0)
            .map(|i| (i, b::kind(m, i), b::state(m, i), (0..320_800u64).filter(|&t| m.i16(0x184b689e0 + t * 2) as i32 == i).count() as i32)).collect()
    };
    let mut last = vec![];
    for t in 0..=ticks {
        if t > 0 { l.step(); }
        let k = keeps(l.mem());
        if t % 100 == 0 || k != last {
            let tick = l.tick();
            let m = l.mem();
            let r = (1..=8u64).map(|k| ai::D + k * 0x6d98).find(|&r| m.i32(r + 4) == 2);
            let aiv = r.map(|r| (m.i32(r + 0x10), m.i32(r + 0x14), m.i32(r + 0xc), m.i32(r + 0x18)));
            eprintln!("tick {tick} keeps (id, kind, state, tiles in BLD) {k:?} aiv (id, fit, side, reached) {aiv:?} P+0x1e0 {}", m.i32(0x183798d60 + 2 * 0x583c + 0x1e0));
            last = k;
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// What an AI lord does to you in a 1v1 (you idle as player 1, `lord` = lobby index as player 2).
#[derive(Debug, Default)]
struct Attack {
    /// first tick its siege state (P+0x2ba4) leaves 0, and the first tick a troop of its comes within 40 tiles of your keep
    launch: Option<i32>,
    near: Option<i32>,
    /// per launch: (tick, siege troops P+0x30f0, defenders P+0x30e8, military 22..28 owned, its army by type)
    launches: Vec<(i32, i32, i32, usize, std::collections::BTreeMap<i16, usize>)>,
    /// most of its knights (28) / engines (catapult 39, trebuchet 40) / troops 22..28 at once within 40 tiles of your keep
    knights_near: usize,
    engines_near: usize,
    troops_near: usize,
}

/// One 1v1 on PROBE_MAP (default Crater Lake) / PROBE_SEED / PROBE_GOODS for PROBE_TICKS (default 15 000) ticks. Every
/// 2500 ticks: its troop counts by role (P+0x30ec harass, 0x30e8 defend, 0x30f0 siege), troop mode, gold, army by type,
/// and what of its army stood near your keep since the last line.
fn first_attack(tag: &str, lord_of: impl Fn() -> usize) -> Option<Attack> {
    use shc_sim::game::player;
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<i32>().ok());
    let map = crate::skirmish::maps_dir().join(std::env::var("PROBE_MAP").unwrap_or("Crater Lake.map".into()));
    if !map.exists() { return None; } // needs the game install
    let dir = folder(&format!("attack{tag}"), &[1, 2, 3, 4, 5, 6, 7, 8]);
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    // PROBE_EXT=1: an Extended game (his smart raids / recruiting)
    let lobby = crate::skirmish::Lobby { ais: vec![(lord_of(), 0)], seed: env("PROBE_SEED").unwrap_or(1234), goods: env("PROBE_GOODS").unwrap_or(1), extended: env("PROBE_EXT") == Some(1), ..crate::skirmish::Lobby::new(map) };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    headless(&mut l);
    let mut r = Attack::default();
    const UB: u64 = 0x1867e53f0;
    let (mut was, mut peak) = (0, std::collections::BTreeMap::<i16, usize>::new());
    for t in 0..env("PROBE_TICKS").unwrap_or(15_000) {
        l.step();
        let m = l.mem();
        let (p1, p2) = (player(1), player(2));
        let xy = |t: i32| { let y = m.i16(0x183aac234 + t as u64 * 2) as i32; (t - m.i32(0x18402debc + y as u64 * 12), y) };
        let (kx, ky) = xy(m.i32(p1 + 0x1e0));
        let (mut army, mut close) = (std::collections::BTreeMap::<i16, usize>::new(), std::collections::BTreeMap::<i16, usize>::new());
        for i in 1..m.i32(UB).max(1) as u64 {
            let u = UB + i * 0x490;
            if m.i16(u + 0x6e4) != 2 || m.i16(u + 0x6ee) != 2 { continue; }
            let k = m.i16(u + 0x6e6);
            if !matches!(k, 5 | 22..=30 | 39..=41) { continue; }
            *army.entry(k).or_default() += 1;
            let (x, y) = xy(m.i32(u + 0x72c));
            if (x - kx).abs() + (y - ky).abs() < 40 { *close.entry(k).or_default() += 1; }
        }
        let mil = |a: &std::collections::BTreeMap<i16, usize>| a.iter().filter(|(k, _)| (22..=28).contains(*k)).map(|(_, v)| v).sum::<usize>();
        if !close.is_empty() && r.near.is_none() { r.near = Some(t); }
        r.knights_near = r.knights_near.max(close.get(&28).copied().unwrap_or(0));
        r.engines_near = r.engines_near.max(close.get(&39).copied().unwrap_or(0) + close.get(&40).copied().unwrap_or(0));
        r.troops_near = r.troops_near.max(mil(&close));
        for (k, v) in &close { let e = peak.entry(*k).or_default(); *e = (*e).max(*v); }
        let s = m.i32(p2 + 0x2ba4);
        if was == 0 && s != 0 {
            r.launch.get_or_insert(t);
            eprintln!("  {tag} t{:6}: ATTACK launch: siege {} defend {} harass {} military {} army {army:?}", t + 1, m.i32(p2 + 0x30f0), m.i32(p2 + 0x30e8), m.i32(p2 + 0x30ec), mil(&army));
            r.launches.push((t, m.i32(p2 + 0x30f0), m.i32(p2 + 0x30e8), mil(&army), army.clone()));
        }
        was = s;
        if t % 2500 == 2499 {
            let brain = (1..=8u64).map(|k| ai::D + k * 0x6d98).find(|&r| m.i32(r + 4) == 2).unwrap_or(ai::D);
            let st = stats(m, 2);
            eprintln!("  {tag} t{:6}: harass {} defend {} siege {} | mode {} attack {} | gold {} | military {} army {army:?} | near your keep (most) {peak:?} | castle {} steps {}/{} ring {}/{} towers {}/{} crenels {}/{} eco {}/{} archers {} failed {:?}", t + 1, m.i32(p2 + 0x30ec), m.i32(p2 + 0x30e8), m.i32(p2 + 0x30f0), m.i32(p2 + 0x30f8), m.i32(p2 + 0x2ba4), m.i32(p2 + 0x50c), mil(&army), m.i32(brain + 0x10) + 1, st.placed, st.total, st.ring.0, st.ring.1, st.towers.0, st.towers.1, st.crenels.0, st.crenels.1, st.eco.0, st.eco.1, st.on_eco, st.failed);
            peak.clear();
        }
    }
    custom_lords::set_in_game(2, None);
    let _ = std::fs::remove_dir_all(&dir);
    Some(r)
}
/// He comes for you, as his sheet says (re/LORD_LINDSCHIED.md section 2: few attacks, each one big; knights raid your
/// farms): 1v1, you idle, PROBE_TICKS (default 80 000: on Crater Lake he builds up until ~74 000) on PROBE_MAP /
/// PROBE_SEED (LINDSCHIED_LORDJSON for a variant).
/// Within the game he launches a siege, and only with an army built up: at least 30 military (types 22..28) and 12
/// siege troops (his siege_trigger_level) at the first launch, no early rush; a defence stays home. When his troops
/// first come within 40 tiles of your keep is printed (on Crater Lake a wave launched at t52,401 is still on the
/// march at 60 000). Frederick on the same map and seed is printed for comparison (PROBE_WHO=lind skips him,
/// PROBE_WHO=fred runs only him).
#[test]
#[ignore] // minutes: run explicitly
fn lindschied_attack_timing() {
    if std::env::var_os("PROBE_TICKS").is_none() { std::env::set_var("PROBE_TICKS", "80000"); }
    // PROBE_LORD=n: only the built-in lord of lobby index n (7 Richard, 8 Frederick, ...), printed as Frederick's is
    if let Some(n) = std::env::var("PROBE_LORD").ok().and_then(|v| v.parse::<usize>().ok()) {
        let Some(f) = first_attack("lord", || n) else { return };
        println!("lord {n}: first attack {:?}, first troop near your keep {:?}; launches {:?}; most near: knights {}, engines {}, troops {}", f.launch, f.near, f.launches, f.knights_near, f.engines_near, f.troops_near);
        return;
    }
    if std::env::var("PROBE_WHO").as_deref() == Ok("fred") {
        let Some(f) = first_attack("fred", || 8) else { return };
        println!("Frederick: first attack {:?}, first troop near your keep {:?}; launches {:?}; most near: knights {}, engines {}, troops {}", f.launch, f.near, f.launches, f.knights_near, f.engines_near, f.troops_near);
        return;
    }
    let Some(d) = first_attack("lind", || BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap()) else { return };
    println!("Lord David: first attack {:?}, first troop near your keep {:?} (ticks, 25 per s); at each launch (tick, siege, defend, military, army): {:?}; most near your keep at once: knights {}, engines {}, troops {}",
        d.launch, d.near, d.launches, d.knights_near, d.engines_near, d.troops_near);
    if std::env::var("PROBE_WHO").as_deref() != Ok("lind") {
        let f = first_attack("fred", || 8).unwrap();
        println!("Frederick: first attack {:?}, first troop near your keep {:?}; launches {:?}; most near: knights {}, engines {}, troops {}", f.launch, f.near, f.launches, f.knights_near, f.engines_near, f.troops_near);
    }
    let (t, siege, defend, mil, _) = d.launches.first().cloned().expect("he never attacks");
    assert!(mil >= 30 && siege >= 12, "first attack at t{t} is a rush: military {mil}, siege troops {siege}");
    assert!(defend > 0, "nobody left at home at t{t}");
}

/// His reaction when you take a castle building of his (natives::castle_lost_msg, what the sim calls when his gatehouse
/// or a tower falls to you), through the viewer's path: the greeting, then "*2*SK_ANGRY_CASTLE_DAMAGED" (his
/// CustomMedia wav), the message bar line (kind −2 → his text.txt) and his sad reaction video over the minimap.
#[cfg(feature = "live")]
#[test]
fn lindschied_reacts_to_castle_loss() {
    let map = crate::skirmish::maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    let dir = folder("react", &[1]);
    let media = dir.join("CustomMedia").join(NAME);
    std::fs::create_dir_all(&media).unwrap();
    let src = Path::new(SRC);
    for f in std::fs::read_dir(src.join("voice")).unwrap().flatten() { std::fs::copy(f.path(), media.join(f.file_name())).unwrap(); }
    for mood in ["angry", "happy", "neutral", "sad"] { std::fs::copy(src.join(format!("video/{mood}.webm")), media.join(format!("{mood}.webm"))).unwrap(); }
    std::fs::copy(src.join("text.txt"), media.join("text.txt")).unwrap();
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    let lord = BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap();
    let lobby = crate::skirmish::Lobby { ais: vec![(lord, 0)], seed: 1234, ..crate::skirmish::Lobby::new(map) };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    // full view: the video / message events it checks come out of the draw records (Live::run_tick → ev.scan)
    // the lords walk in after the start; until then every lord line is gated on a live lord unit (built-in or custom)
    for _ in 0..600 { l.step(); }
    assert_ne!(ai::build::lord_unit(l.mem(), 2), 0, "his lord unit is on the map");
    let _ = shc_sim::sound::drain();
    shc_sim::units::natives::castle_lost_msg(l.mem(), 2, 1);
    let (mut named, mut videos, mut bars) = (vec![], vec![], vec![]);
    for _ in 0..300 {
        l.step();
        named.extend(shc_sim::sound::drain().into_iter().filter_map(|s| if let shc_sim::sound::Sound::Named(n) = s { Some(n) } else { None }));
        let ev = l.hud().ev;
        if let Some(v) = ev.video.now { if !videos.contains(&v.0) { videos.push(v.0) } }
        if let Some(m) = ev.message { if !bars.contains(&(m.0, m.1)) { bars.push((m.0, m.1)) } }
    }
    println!("named {named:?}, videos {videos:?}, bars {bars:?}");
    let line = named.iter().find_map(|n| custom_lords::split(n)).expect("his line");
    assert!(named.iter().any(|n| n.contains("general_message1")), "the greeting: {named:?}");
    assert_eq!(line, (2, "ANGRY_CASTLE_DAMAGED".into()));
    assert!(custom_lords::wav(2, &line.1).is_some_and(|p| p.ends_with("ANGRY_CASTLE_DAMAGED.wav")), "his wav");
    assert!(bars.contains(&(-2, 6)), "message bar kind -2, line 6: {bars:?}");
    assert_eq!(custom_lords::text(2, &line.1).as_deref(), Some("Stone can be mended. The insult cannot. Wir sehen uns."));
    // his sad clip once exported (set_in_game starts the export; until it is there the generic custom-lord clip)
    let want = custom_lords::video_stem(2, &line.1).unwrap_or_else(|| "bad_soldier_taunt".into());
    assert!(videos.contains(&want), "video {videos:?}, want {want}");
    custom_lords::set_in_game(2, None);
    let _ = std::fs::remove_dir_all(&dir);
}


/// REVIEW PROBE (re/LORD_LINDSCHIED_REVIEW.md sections 6..8): REVIEW_SUBJECT (lobby index, 1000 = Lord David) as
/// player 2, REVIEW_OPP as player 3, you idle as player 1, Crater Lake seed 1234, REVIEW_TICKS (default 60 000).
/// Every 2500 ticks: troops by type, engines, gold, food, weapons, iron, popularity, population, castle steps, troop
/// roles / mode / stance, tax, gold in / out, the recruit gate P+0x2b6c and the spend flag P+0x3974; then the economy:
/// buildings by kind as built / unmanned, food stocks, housing, and gold spent since the start by what took it (a
/// drop with a new recruit, a stock that grew = market buy, a new building of his). Each attack launch (P+0x2ba4
/// leaving 0): army at launch and the most of his troops later within 40 tiles of each keep.
#[test]
#[ignore]
fn review_probe() {
    use shc_sim::game::player;
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok());
    let map = crate::skirmish::maps_dir().join("Crater Lake.map");
    let dir = folder(&format!("rev{}", env("REVIEW_SUBJECT").unwrap_or(1000)), &[1, 2, 3, 4, 5, 6, 7, 8]);
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    let lind = BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap();
    let pick = |v: usize| if v == 1000 { lind } else { v };
    let (subj, opp) = (pick(env("REVIEW_SUBJECT").unwrap_or(1000)), pick(env("REVIEW_OPP").unwrap_or(8)));
    let lobby = crate::skirmish::Lobby { ais: vec![(subj, 0), (opp, 0)], seed: 1234, ..crate::skirmish::Lobby::new(map) };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    headless(&mut l);
    let ticks = env("REVIEW_TICKS").unwrap_or(60_000) as i32;
    const UB: u64 = 0x1867e53f0;
    let army = |m: &Mem, p: i16| -> std::collections::BTreeMap<i16, usize> {
        let mut c = std::collections::BTreeMap::new();
        for i in 1..m.i32(UB).max(1) as u64 {
            let u = UB + i * 0x490;
            if m.i16(u + 0x6e4) == 2 && m.i16(u + 0x6ee) == p { *c.entry(m.i16(u + 0x6e6)).or_insert(0) += 1; }
        }
        c
    };
    let xy = |m: &Mem, t: i32| { let y = m.i16(0x183aac234 + t as u64 * 2) as i32; (t - m.i32(0x18402debc + y as u64 * 12), y) };
    let near = |m: &Mem, p: i16, k: u32| -> usize {
        let (kx, ky) = xy(m, m.i32(player(k) + 0x1e0));
        (1..m.i32(UB).max(1) as u64).filter(|&i| {
            let u = UB + i * 0x490;
            let (x, y) = xy(m, m.i32(u + 0x72c));
            m.i16(u + 0x6e4) == 2 && m.i16(u + 0x6ee) == p && matches!(m.i16(u + 0x6e6), 22..=30 | 39..=41) && (x - kx).abs() + (y - ky).abs() < 40
        }).count()
    };
    let g = |m: &Mem, p: u32, k: u64| m.i32(player(p) + 0x4d0 + k * 4);
    let mut was = [0i32; 4];
    let mut peak = [[0usize; 4]; 4]; // [p][keep k] since last sample
    let mut starve = [0; 4];
    let (mut gold_was, mut gold_in, mut gold_out) = ([0i32; 4], [0i32; 4], [0i32; 4]); // gold up / down per window
    let (mut stock_was, mut recruits_was) = (vec![vec![]; 4], [0i32; 4]);
    let mut bld_was = vec![std::collections::BTreeSet::<i32>::new(); 4];
    let mut spent = vec![std::collections::BTreeMap::<String, i32>::new(); 4]; // gold spent since the start by category
    for t in 0..ticks {
        l.step();
        let m = l.mem();
        for p in 2..=3u32 {
            let s = m.i32(player(p) + 0x2ba4);
            if was[p as usize] == 0 && s != 0 {
                let a = army(m, p as i16);
                eprintln!("  t{} p{p} ATTACK launch state {s} siege {} harass {} army {:?}", t + 1, m.i32(player(p) + 0x30f0), m.i32(player(p) + 0x30ec), a);
            }
            was[p as usize] = s;
            for k in 1..=3u32 { if k != p { let n = near(m, p as i16, k); peak[p as usize][k as usize] = peak[p as usize][k as usize].max(n); } }
            if t % 250 == 0 && (10..=13).map(|k| g(m, p, k)).sum::<i32>() == 0 { starve[p as usize] += 1; }
            let (gn, pu) = (m.i32(player(p) + 0x50c), p as usize);
            if gn > gold_was[pu] { gold_in[pu] += gn - gold_was[pu] } else { gold_out[pu] += gold_was[pu] - gn }
            // where gold goes: a drop with a new recruit, a stock that grew (market buy) or a new building of p
            let stock: Vec<i32> = (1..25).map(|k| g(m, p, k)).collect();
            let rec_n = m.i32(0x1887ec0ac + p as u64 * 4);
            if gn < gold_was[pu] && t > 0 {
                let d = gold_was[pu] - gn;
                let mine: std::collections::BTreeSet<i32> = (1..b::SLOTS).filter(|&i| b::state(m, i) != 0 && b::owner(m, i) == p as i32).collect();
                let cat = if rec_n > recruits_was[pu] { "recruit".to_string() }
                    else if let Some(k) = (0..24).find(|&k| stock[k] > stock_was[pu].get(k).copied().unwrap_or(i32::MAX)) { format!("buy{}", k + 1) }
                    else if let Some(&i) = mine.difference(&bld_was[pu]).next() { format!("bld{}", b::kind(m, i)) }
                    else { "other".to_string() };
                *spent[pu].entry(cat).or_default() += d;
                bld_was[pu] = mine;
            }
            (stock_was[pu], recruits_was[pu]) = (stock, rec_n);
            gold_was[pu] = gn;
        }
        if t % 2500 == 2499 {
            for p in 2..=3u32 {
                let a = army(m, p as i16);
                let troops: usize = a.iter().filter(|(k, _)| (22..=30).contains(*k) || (60..=80).contains(*k)).map(|(_, v)| v).sum();
                let eng: usize = a.iter().filter(|(k, _)| (39..=41).contains(*k)).map(|(_, v)| v).sum();
                let s = stats(m, p as i32);
                eprintln!("  t{} p{p} troops {troops} eng {eng} {:?} | gold {} food {} wpn {} (xb {} sw {}) iron {} stone {} wood {} | pop {} people {} | steps {}/{} ring {}/{} failed {} bld {} | near p1 {} p2/3 {} | nofood {} | roles d{} h{} s{} mode {} stance {} leather {} metal {} swords {} | tax {} gold +{} -{} market net {} sold {} recruits {} | may recruit (P+0x2b6c) {} spend {}",
                    t + 1, a.iter().filter(|(k, _)| (22..=41).contains(*k)).collect::<Vec<_>>(), m.i32(player(p) + 0x50c), (10..=13).map(|k| g(m, p, k)).sum::<i32>(),
                    (17..=24).map(|k| g(m, p, k)).sum::<i32>(), g(m, p, 18), g(m, p, 22), g(m, p, 6), g(m, p, 4), g(m, p, 2),
                    m.i32(player(p) + 0x60) / 100, m.i32(player(p) + 0x2180), s.placed, s.total, s.ring.0, s.ring.1, s.failed.len(), s.buildings,
                    peak[p as usize][1], peak[p as usize][if p == 2 { 3 } else { 2 }], starve[p as usize], m.i32(player(p) + 0x30e8), m.i32(player(p) + 0x30ec), m.i32(player(p) + 0x30f0), m.i32(player(p) + 0x30f8), m.i32(player(p) + 0x30f4), g(m, p, 23), g(m, p, 24), g(m, p, 22),
                    m.i32(player(p) + 0x2188), gold_in[p as usize], gold_out[p as usize], m.i32(player(p) + 0x54), m.i32(0x1887ebd88 + p as u64 * 4), m.i32(0x1887ec0ac + p as u64 * 4), m.i32(player(p) + 0x2b6c), m.i32(player(p) + 0x3974));
                // the economy: finished buildings by kind (eStructs) as built / unmanned (worker slots +0x198, vacant +0x19c)
                let mut eco: std::collections::BTreeMap<i32, (usize, usize)> = Default::default();
                for i in 1..b::SLOTS {
                    if b::state(m, i) != 2 || b::owner(m, i) != p as i32 { continue; }
                    let e = eco.entry(b::kind(m, i)).or_default();
                    e.0 += 1;
                    if m.i16(b::rec(i) + 0x198) > 0 && m.i16(b::rec(i) + 0x19c) > 0 { e.1 += 1; }
                }
                let show = [1, 3, 5, 12, 13, 15, 16, 17, 18, 20, 22, 30, 31, 32, 33, 34, 35];
                eprintln!("  t{} p{p} eco (kind: built/unmanned) {} | wheat {} flour {} bread {} cheese {} apples {} ale {} hops {} | housing {} P+0x2b08 {} | spent {:?}", t + 1,
                    show.iter().filter_map(|k| eco.get(k).map(|(n, v)| format!("{k}:{n}/{v}"))).collect::<Vec<_>>().join(" "),
                    g(m, p, 9), g(m, p, 16), g(m, p, 10), g(m, p, 11), g(m, p, 13), g(m, p, 14), g(m, p, 3), m.i32(player(p) + 0x74), m.i32(player(p) + 0x2b08), spent[p as usize]);
                peak[p as usize] = [0; 4];
                (gold_in[p as usize], gold_out[p as usize]) = (0, 0);
            }
        }
    }
    let m = l.mem();
    let r = (1..=8u64).map(|k| ai::D + k * 0x6d98).find(|&r| m.i32(r + 4) == 2).unwrap();
    let mut by: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for e in 1..=m.i32(r + 0x24).max(0) as u64 {
        let e = r + 0x38 + e * 0xc;
        let k = m.i16(e + 2);
        if k == 0 { continue; }
        let x = by.entry(shc_formats::aiv::kind_name(k)).or_default();
        x.1 += 1;
        if m.u8(e) == 3 { x.0 += 1; }
    }
    eprintln!("  END p2 castle (placed/total by kind) {by:?} fit {} reached {}", m.i32(r + 0x14), m.i32(r + 0x18));
    custom_lords::set_in_game(2, None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// REVIEW (section 7..8): which of player 2's own finished buildings vanish in the first REVIEW_TICKS (default 15000)
/// ticks, with the tick and the building kind (eStructs). REVIEW_SUBJECT as in review_probe.
#[test]
#[ignore]
fn review_demolish() {
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok());
    let map = crate::skirmish::maps_dir().join("Crater Lake.map");
    let dir = folder(&format!("dem{}", env("REVIEW_SUBJECT").unwrap_or(1000)), &[1, 2, 3, 4, 5, 6, 7, 8]);
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    let lind = BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap();
    let pick = |v: usize| if v == 1000 { lind } else { v };
    let lobby = crate::skirmish::Lobby { ais: vec![(pick(env("REVIEW_SUBJECT").unwrap_or(1000)), 0), (pick(env("REVIEW_OPP").unwrap_or(8)), 0)], seed: 1234, ..crate::skirmish::Lobby::new(map) };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    headless(&mut l);
    let mut had: Vec<(i32, i32)> = vec![(0, 0); b::SLOTS as usize]; // (owner 2 and finished: kind, 0)
    let mut gone: std::collections::BTreeMap<i32, usize> = Default::default();
    for t in 0..env("REVIEW_TICKS").unwrap_or(15_000) as i32 {
        l.step();
        let m = l.mem();
        for i in 1..b::SLOTS {
            let now = if b::state(m, i) == 2 && b::owner(m, i) == 2 { b::kind(m, i) } else { 0 };
            let was = had[i as usize].0;
            if was != 0 && now != was {
                eprintln!("  t{} p2 lost building kind {was} (slot {i}, state now {}, owner now {})", t + 1, b::state(m, i), b::owner(m, i));
                *gone.entry(was).or_insert(0) += 1;
            }
            had[i as usize].0 = now;
        }
    }
    eprintln!("  LOST by kind {gone:?}");
    custom_lords::set_in_game(2, None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Smart raids (Extended rules, `shc_sim::ext::SMART_RAIDS`): 1v1, you idle as player 1 with wheat farms on every
/// spot round your keep (6..60 tiles) where the validator lets one stand, four archers beside every third far one.
/// Over PROBE_TICKS (default 60 000; LINDSCHIED_LORDJSON for a variant, PROBE_MAP / PROBE_SEED): every farm his raid
/// squads set out for is unguarded at that moment (no guard at it or on the straight way), raids cost at most a tenth
/// of the men sent, and a squad whose farm is gone rides home.
#[test]
#[ignore] // minutes: run explicitly
fn lindschied_smart_raids() {
    use shc_sim::{ai::build, ext, game::player, units};
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<i32>().ok());
    // Armenia: farmland round your keep (Crater Lake has room for one farm there)
    let map = crate::skirmish::maps_dir().join(std::env::var("PROBE_MAP").unwrap_or("Armenia.map".into()));
    if !map.exists() { return; } // needs the game install
    let dir = folder("raids", &[1, 2, 3, 4, 5, 6, 7, 8]);
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    let lind = BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap();
    let lobby = crate::skirmish::Lobby { ais: vec![(lind, 0)], seed: env("PROBE_SEED").unwrap_or(1234), extended: true, ..crate::skirmish::Lobby::new(map) };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    headless(&mut l);
    for _ in 0..100 { l.step(); } // the keeps settle
    let m = l.mem();
    assert!(ext::on(m), "an Extended game");
    let at = ai::F + (2 + 0x1d) * ai::CFG_STRIDE as u64 + ext::SMART_RAIDS as u64;
    assert_ne!(m.i32(at), 0, "his lordjson asks for smart raids");
    let xy = |m: &Mem, t: i32| { let y = m.i16(0x183aac234 + t as u64 * 2) as i32; (t - m.i32(0x18402debc + y as u64 * 12), y) };
    let farms = |m: &Mem| (1..b::SLOTS).filter(|&i| b::owner(m, i) == 1 && b::kind(m, i) == 0x1e && !matches!(b::state(m, i), 0 | 3)).collect::<Vec<_>>();
    let size = shc_sim::mapaction::footprint(0x46);
    let (kx, ky) = l.keep_of(1).map(|(x, y)| (x as i32, y as i32)).expect("your keep");
    let m = l.mem();
    let mut placed: Vec<i32> = vec![];
    for r in (6..=60).step_by(4) {
        for k in 0..(r * 2).max(12) {
            let a = k as f32 * std::f32::consts::TAU / (r * 2).max(12) as f32;
            let (x, y) = (kx + (a.cos() * r as f32) as i32, ky + (a.sin() * r as f32) as i32);
            if placed.iter().any(|&f| { let (fx, fy) = ext::building_xy(m, f); (fx - x).abs().max((fy - y).abs()) < 6 }) { continue; }
            let before = farms(m);
            build::place(m, 1, x, y, 0x46, size, 0xf, true);
            if let Some(&f) = farms(m).iter().find(|f| !before.contains(f)) { placed.push(f); }
            if placed.len() >= 14 { break; }
        }
    }
    let far: Vec<i32> = placed.iter().copied().filter(|&f| { let (x, y) = ext::building_xy(m, f); (x - kx).abs().max((y - ky).abs()) > ext::RAID_TOWER + 4 }).collect();
    for &f in far.iter().step_by(3) {
        let (x, y) = ext::building_xy(m, f);
        for d in 0..4 {
            let t = units::tile(m, x + d % 2, y + 3 + d / 2);
            units::helpers::spawn(m, 1, 1, ((x + d % 2) * 8) as i16, ((y + 3 + d / 2) * 8) as i16, units::height(m, t) as i16, 22);
        }
    }
    l.step();
    let m = l.mem();
    let open: Vec<i32> = placed.iter().copied().filter(|&f| !ext::guarded(m, 1, f)).collect();
    println!("your keep {:?}: {} farms, {} far, unguarded {open:?}", (kx, ky), placed.len(), far.len());
    assert!(placed.len() > open.len() && !open.is_empty(), "need guarded and unguarded farms: {} placed, {} open", placed.len(), open.len());
    const GROUPS: u64 = 0x187cc3710; // 0x688 per squad: +0x5a leader, +0x64c target building
    let r2 = player(2);
    let mut target = [0i32; 6];
    let mut out = [0i32; 6]; // men the squad set out with
    let (mut sent, mut back) = (0, 0);
    let (mut raids, mut bad, mut homes, mut away) = (vec![], vec![], 0, 0);
    let mut home_due: Vec<(usize, i32)> = vec![]; // (squad slot, tick its farm was done): on its way home
    let mut was_guarded: Vec<(i32, bool)> = placed.iter().map(|&f| (f, true)).collect();
    let mut burned: Vec<(i32, i32, bool)> = vec![]; // (tick, farm, guarded the tick before)
    let mut first_squad = None; // the tick his first raid squad stood
    for t in 0..env("PROBE_TICKS").unwrap_or(60_000) {
        l.step();
        let m = l.mem();
        if first_squad.is_none() && (0..6u64).any(|k| m.i16(r2 + 0x4a08 + k * 2) != 0) { first_squad = Some(t); }
        for k in 0..6usize {
            let s = m.i16(r2 + 0x4a08 + k as u64 * 2) as i32;
            let now = if s == 0 { 0 } else { m.i16(GROUPS + s as u64 * 0x688 + 0x64c) as i32 };
            if now != target[k] {
                let men = if s == 0 { 0 } else { m.i16(GROUPS + s as u64 * 0x688 + 0x5c) as i32 };
                if target[k] != 0 { sent += out[k]; back += men.min(out[k]); }
                out[k] = men;
                if now != 0 && home_due.iter().any(|h| h.0 == k) {
                    homes += 1; // set out again: smart raids only leave from home
                    home_due.retain(|h| h.0 != k);
                }
                if now != 0 {
                    let g = ext::guarded(m, b::owner(m, now), now);
                    raids.push((t, k, now, b::kind(m, now), g));
                    if g || !matches!(b::kind(m, now), 0x1e..=0x21) { bad.push((t, k, now)); }
                } else {
                    home_due.push((k, t));
                }
                target[k] = now;
            }
        }
        for (f, g) in was_guarded.iter_mut() {
            if *f == 0 { continue; }
            if matches!(b::state(m, *f), 0 | 3) || b::kind(m, *f) != 0x1e { burned.push((t, *f, *g)); *f = 0; continue; }
            if t % 25 == 0 { *g = ext::guarded(m, 1, *f); }
        }
        if t % 10_000 == 9_999 {
            let sq: Vec<(i32, i32)> = (0..6u64).filter_map(|k| {
                let s = m.i16(r2 + 0x4a08 + k * 2) as i32;
                (s != 0).then(|| (m.i16(GROUPS + s as u64 * 0x688 + 0x5c) as i32, m.i16(GROUPS + s as u64 * 0x688 + 0x64c) as i32))
            }).collect();
            let from = ext::raid_home(m, 2).unwrap_or((0, 0));
            println!("  t{}: his raid squads (men, target) {sq:?}, harass role {}, your farms with a clear way from his home {:?}", t + 1, m.i32(r2 + 0x30ec), ext::unguarded_farms(m, 1, from));
        }
        if std::env::var_os("RAID_TRACE").is_some() && t % 250 == 0 {
            for &(k, _) in &home_due {
                let s = m.i16(r2 + 0x4a08 + k as u64 * 2) as i32;
                let lead = if s == 0 { 0 } else { m.i16(GROUPS + s as u64 * 0x688 + 0x5a) as i32 };
                if lead == 0 { continue; }
                let ur = units::u(lead);
                println!("    t{t} squad {k}: leader {:?} order/state {} {} target unit {} building {}", xy(m, m.i32(ur + 0xd4)),
                    m.i16(ur + 0x2c0), m.i16(ur + 0x2a4), m.i16(ur + 0x346), m.i16(ur + 0x344));
            }
        }
        home_due.retain(|&(k, done)| {
            if t % 250 != 0 { return true; }
            let s = m.i16(r2 + 0x4a08 + k as u64 * 2) as i32;
            let lead = if s == 0 { 0 } else { m.i16(GROUPS + s as u64 * 0x688 + 0x5a) as i32 };
            if lead == 0 { return false; } // the squad is gone
            let (x, y) = xy(m, m.i32(units::u(lead) + 0xd4));
            let (hx, hy) = ext::raid_home(m, 2).unwrap_or((x, y));
            if (x - hx).abs().max((y - hy).abs()) <= ext::RAID_HOME + 6 {
                println!("  t{}: squad {k} home, {} ticks after its farm", t + 1, t - done);
                homes += 1;
                return false;
            }
            if t - done > 10_000 { // swordsmen walk ~6 tiles per 250 ticks
                println!("  t{}: squad {k} still out at {:?}, home {:?}", t + 1, (x, y), (hx, hy));
                away += 1;
                return false;
            }
            true
        });
    }
    println!("raids (tick, squad, building, kind, guarded): {raids:?}");
    println!("your farms burned (tick, farm, guarded just before): {burned:?}; squads home after a raid: {homes}, still away: {away}; raiders out {sent}, back {back}");
    custom_lords::set_in_game(2, None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(bad.is_empty(), "raids on guarded farms or non-farms: {bad:?}");
    // he must raid unless none of your farms was left by the time his first raid squad stood (his siege burned them)
    let left = |t: i32| placed.len() > burned.iter().filter(|b| b.0 < t).count();
    assert!(!raids.is_empty() || first_squad.is_some_and(|t| !left(t)), "he never raided (first raid squad at {first_squad:?})");
    // (farms that burn under his siege are no raid's: only the raid targets are checked, in `bad`)
    assert!(homes >= away, "squads stay out after their raid: home {homes}, away {away}");
    assert!(sent - back <= (sent / 10).max(1), "raids cost {} of {sent} men", sent - back);
}

/// Head to head: Lord David against a built-in lord (PROBE_LORD, lobby index, default 8 = Frederick), you idle and on
/// his team so the two fight each other; PROBE_MAP / PROBE_SEED, PROBE_TICKS (default 120 000), PROBE_SWAP=1 gives
/// the other lord the first AI slot. Every 10 000 ticks both armies; at the end whose lord still lives.
#[test]
#[ignore] // minutes: run explicitly
fn lindschied_war() {
    use shc_sim::game::player;
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<i32>().ok());
    let map = crate::skirmish::maps_dir().join(std::env::var("PROBE_MAP").unwrap_or("Crater Lake.map".into()));
    if !map.exists() { return; } // needs the game install
    let dir = folder("war", &[1, 2, 3, 4, 5, 6, 7, 8]);
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    let lind = BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap();
    let foe = env("PROBE_LORD").unwrap_or(8) as usize;
    let swap = env("PROBE_SWAP").unwrap_or(0) != 0;
    let ais = if swap { vec![(foe, 2), (lind, 1)] } else { vec![(lind, 1), (foe, 2)] };
    let (pd, pf) = if swap { (3, 2) } else { (2, 3) };
    let lobby = crate::skirmish::Lobby { ais, team: 1, seed: env("PROBE_SEED").unwrap_or(1234), extended: env("PROBE_EXT") == Some(1), ..crate::skirmish::Lobby::new(map.clone()) };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    headless(&mut l);
    let mil = |m: &Mem, p: i32| (1..m.i32(shc_sim::units::OBJ).max(1)).filter(|&i| {
        let r = shc_sim::units::u(i);
        m.i16(r + 0x8c) == 2 && m.i16(r + 0x96) as i32 == p && matches!(m.i16(r + 0x8e), 22..=28 | 70..=76 | 78..=85)
    }).count();
    let alive = |m: &Mem, p: i32| ai::build::lord_unit(m, p) != 0;
    let mut was = [0i32; 4];
    for t in 0..env("PROBE_TICKS").unwrap_or(120_000) {
        l.step();
        let m = l.mem();
        for p in [pd, pf] {
            let s = m.i32(player(p as u32) + 0x2ba4);
            if was[p as usize] == 0 && s != 0 { eprintln!("  t{} {} attacks (target {})", t + 1, if p == pd { "David" } else { "foe" }, m.i32(player(p as u32) + 0x2bd8)); }
            was[p as usize] = s;
        }
        if t % 10_000 == 9_999 {
            eprintln!("  t{}: David {} soldiers{}, foe {} soldiers{}", t + 1, mil(m, pd), if alive(m, pd) { "" } else { " (lord dead)" }, mil(m, pf), if alive(m, pf) { "" } else { " (lord dead)" });
            for (who, p) in [("David", pd), ("foe", pf)] {
                let r = player(p as u32);
                let g = |k: u64| m.i32(r + 0x4d0 + k * 4); // goods
                eprintln!("    {who}: people {} housing {} idle {} | gold {} tax {} pop {} | food {} wood {} stone {} iron {} | weapons {}",
                    m.i32(r + 0x2180), m.i32(r + 0x74), m.i32(r + 0x2b08), m.i32(r + 0x50c), m.i32(r + 0x2188), m.i32(r + 0x60) / 100,
                    (10..=13).map(g).sum::<i32>(), g(2), g(4), g(6), (17..=24).map(g).sum::<i32>());
            }
        }
        if t > 3000 && (!alive(m, pd) || !alive(m, pf)) { break; } // the lords walk in after the start
    }
    let m = l.mem();
    // what an army is worth: recruit gold + its arms at market price (TEXT trade_buy for 5, /5): archer 12 + bow 31,
    // crossbow 20 + 58 + leather 25, spear 8 + 20, pike 20 + 36 + metal 58, mace 20 + 58 + leather 25, sword 40 + 58 +
    // 58, knight 40 + 58 + 58 + horse ~25; Arab and Bedouin troops at their mercenary price ~40..80 (60)
    let worth = |m: &Mem, p: i32| (1..m.i32(shc_sim::units::OBJ).max(1)).map(|i| {
        let r = shc_sim::units::u(i);
        if m.i16(r + 0x8c) != 2 || m.i16(r + 0x96) as i32 != p { return 0; }
        match m.i16(r + 0x8e) { 22 => 43, 23 => 103, 24 => 28, 25 => 114, 26 => 103, 27 => 156, 28 => 181, 70..=76 | 78..=85 => 60, _ => 0 }
    }).sum::<i32>();
    println!("war on {} (seed {}, swap {swap}): David {} ({} soldiers, worth {}), lord {foe} {} ({} soldiers, worth {})", map.display(), lobby.seed,
        if alive(m, pd) { "alive" } else { "DEAD" }, mil(m, pd), worth(m, pd), if alive(m, pf) { "alive" } else { "DEAD" }, mil(m, pf), worth(m, pf));
    custom_lords::set_in_game(pd as usize, None);
    let _ = std::fs::remove_dir_all(&dir);
}
