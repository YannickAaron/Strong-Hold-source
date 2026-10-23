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
        assert!(a.troops.iter().all(|t| [2, 3, 7, 11].contains(&t.slot)), "castle {k}: troop slots");
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

/// One lord's castle progress and army: (AIV fit, step reached, entries placed / total, failed entries' kinds,
/// buildings, crossbowmen, swordsmen, other troops, engines, gold).
#[derive(Debug)]
struct Stats { ring: (usize, usize), fit: i32, reached: i32, placed: usize, total: usize, failed: Vec<i16>, buildings: usize, xbow: usize, sword: usize, other: usize, engines: usize, gold: i32 }

fn stats(m: &Mem, p: i32) -> Stats {
    let r = (1..=8u64).map(|k| ai::D + k * 0x6d98).find(|&r| m.i32(r + 4) == p).expect("AI record");
    let n = m.i32(r + 0x24).max(0) as u64;
    // a stairs entry the AI re-checks fails (state 5) once its tile holds the stairs (ai::build::wall_ok refuses a
    // raised tile): stairs on the tile (flag 0x800) count as placed
    let stair_on = |e: u64| m.u32(shc_sim::terrain::W + 0x898400 + m.i32(e + 8) as u64 * 4) & 0x800 != 0;
    let entries: Vec<(u8, i16)> = (1..=n).map(|e| { let e = r + 0x38 + e * 0xc; (m.u8(e), m.i16(e + 2), e) })
        .filter(|&(_, k, _)| k != 0).map(|(s, k, e)| (if s == 5 && (181..=186).contains(&k) && stair_on(e) { 3 } else { s }, k)).collect();
    let (mut xbow, mut sword, mut other, mut engines) = (0, 0, 0, 0);
    const U: u64 = 0x1867e5a48;
    for i in 1..m.i32(0x1867e53f0).max(1) as u64 {
        let u = U + i * 0x490;
        if m.i16(u + 0x8c) != 2 || m.i16(u + 0x96) as i32 != p { continue; }
        match m.i16(u + 0x8e) {
            23 => xbow += 1,
            27 => sword += 1,
            22..=28 => other += 1,
            39..=41 | 50..=60 => engines += 1,
            _ => {}
        }
    }
    Stats {
        ring: (entries.iter().filter(|e| e.0 == 3 && [25, 26, 113, 144, 145].contains(&e.1)).count(), entries.iter().filter(|e| [25, 26, 113, 144, 145].contains(&e.1)).count()),
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
            // a barracks refused by the map is fine while the second one stands (make_castles.py places two)
            let refused_ok = d.failed.iter().all(|&k| k == 87) && d.failed.len() < 2;
            assert!(refused_ok && d.placed >= f.placed && d.ring.0 * 4 >= d.ring.1 * 3, "castle 1 stalled: {d:?} (Frederick {f:?})");
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

/// When does an AI lord first come at you? 1v1, you idle as player 1, `lord` (lobby index) as player 2: the tick its
/// attack state (P+0x2ba4) leaves 0 and the tick its first troop comes within 40 tiles of your keep, None = never.
/// Every 2500 ticks: its troop counts by role (P+0x30ec harass, 0x30e8 defend, 0x30f0 siege), troop mode, gold.
fn first_attack(tag: &str, lord_of: impl Fn() -> usize) -> Option<(Option<i32>, Option<i32>)> {
    use shc_sim::game::player;
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<i32>().ok());
    let map = crate::skirmish::maps_dir().join(std::env::var("PROBE_MAP").unwrap_or("Crater Lake.map".into()));
    if !map.exists() { return None; } // needs the game install
    let dir = folder(&format!("attack{tag}"), &[1, 2, 3, 4, 5, 6, 7, 8]);
    crate::skirmish::set_test_profile(dir.clone());
    custom_lords::rescan();
    let lobby = crate::skirmish::Lobby { ais: vec![(lord_of(), 0)], seed: env("PROBE_SEED").unwrap_or(1234), goods: env("PROBE_GOODS").unwrap_or(1), ..crate::skirmish::Lobby::new(map) };
    let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
    let (mut attack, mut near) = (None, None);
    for t in 0..env("PROBE_TICKS").unwrap_or(15_000) {
        l.step();
        let m = l.mem();
        let (p1, p2) = (player(1), player(2));
        if attack.is_none() && m.i32(p2 + 0x2ba4) != 0 { attack = Some(t); }
        let xy = |t: i32| { let y = m.i16(0x183aac234 + t as u64 * 2) as i32; (t - m.i32(0x18402debc + y as u64 * 12), y) };
        let (kx, ky) = xy(m.i32(p1 + 0x1e0));
        let close = (1..m.i32(0x1867e53f0).max(1)).any(|i| {
            let u = 0x1867e53f0u64 + i as u64 * 0x490;
            let (x, y) = xy(m.i32(u + 0x72c));
            m.i16(u + 0x6e4) == 2 && m.i16(u + 0x6ee) == 2 && matches!(m.i16(u + 0x6e6), 5 | 22..=30 | 39..=41) && (x - kx).abs() + (y - ky).abs() < 40
        });
        if near.is_none() && close { near = Some(t); }
        if t % 2500 == 2499 {
            eprintln!("  {tag} t{:6}: harass {} defend {} siege {} | mode {} attack {} | gold {}", t + 1, m.i32(p2 + 0x30ec), m.i32(p2 + 0x30e8), m.i32(p2 + 0x30f0), m.i32(p2 + 0x30f8), m.i32(p2 + 0x2ba4), m.i32(p2 + 0x50c));
        }
    }
    custom_lords::set_in_game(2, None);
    let _ = std::fs::remove_dir_all(&dir);
    Some((attack, near))
}
/// Lord David vs the shipped Frederick, same map and seed (env PROBE_MAP / PROBE_SEED / PROBE_GOODS / PROBE_TICKS,
/// LINDSCHIED_LORDJSON): he must not come at you sooner than Frederick does.
#[test]
#[ignore] // minutes: run explicitly
fn lindschied_attack_timing() {
    let Some(d) = first_attack("lind", || BASE + custom_lords::names().iter().position(|n| n == NAME).unwrap()) else { return };
    let f = first_attack("fred", || 8).unwrap();
    println!("first attack / first troop near your keep (ticks, 25 per s): Lord David {d:?}, Frederick {f:?}");
    let soon = |v: Option<i32>| v.unwrap_or(i32::MAX);
    assert!(soon(d.0) >= soon(f.0) && soon(d.1) >= soon(f.1), "Lord David comes at you sooner: {d:?} vs Frederick {f:?}");
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

