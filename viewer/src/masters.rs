//! Skirmish Masters (FRONT_SkirmishMasters, ConfigSettings.AddSkirmishMastersGame): every finished skirmish-mode game
//! (skirmish, Crusader Trail, Sands of Time, single-player co-op) as a row of skirmishmasters.cfg, the original's
//! Unity JsonUtility file {"skirmishMastersData":[{MPScoreData}, ...]} (no cap, `index` = list position, version 2),
//! and the screen listing them: sort by score / name / time / date, filters (trails, others, best score only),
//! delete a row. The row's statistics are DLL_GetMPScoreData (shc_sim::mpscore), its score
//! HUD_MissionOver.calc_greatest_lord_points. Child of trail_menu (the trail maps' -11 button opens it).

use super::*;

/// A JSON value as JsonUtility writes this file: integers, strings, arrays, objects (keys in declaration order).
#[derive(Clone, Debug, PartialEq)]
pub enum J { Num(i64), Str(String), Arr(Vec<J>), Obj(Vec<(String, J)>) }

impl J {
    fn get(&self, k: &str) -> Option<&J> { if let J::Obj(v) = self { v.iter().find(|e| e.0 == k).map(|e| &e.1) } else { None } }
    pub fn int(&self, k: &str) -> i64 { if let Some(J::Num(n)) = self.get(k) { *n } else { 0 } }
    fn str(&self, k: &str) -> String { if let Some(J::Str(s)) = self.get(k) { s.clone() } else { String::new() } }
    pub fn ints(&self, k: &str) -> Vec<i64> { if let Some(J::Arr(a)) = self.get(k) { a.iter().map(|v| if let J::Num(n) = v { *n } else { 0 }).collect() } else { vec![] } }
    fn strs(&self, k: &str) -> Vec<String> { if let Some(J::Arr(a)) = self.get(k) { a.iter().map(|v| if let J::Str(s) = v { s.clone() } else { String::new() }).collect() } else { vec![] } }
    fn set(&mut self, k: &str, v: J) { if let J::Obj(o) = self { if let Some(e) = o.iter_mut().find(|e| e.0 == k) { e.1 = v } else { o.push((k.into(), v)) } } }

    /// Compact, like JsonUtility.ToJson (`"` and `\` escaped).
    pub fn write(&self, out: &mut String) {
        match self {
            J::Num(n) => out.push_str(&n.to_string()),
            J::Str(s) => { out.push('"'); for c in s.chars() { if matches!(c, '"' | '\\') { out.push('\\'); } out.push(c); } out.push('"'); }
            J::Arr(a) => { out.push('['); for (i, v) in a.iter().enumerate() { if i > 0 { out.push(','); } v.write(out); } out.push(']'); }
            J::Obj(o) => {
                out.push('{');
                for (i, (k, v)) in o.iter().enumerate() { if i > 0 { out.push(','); } J::Str(k.clone()).write(out); out.push(':'); v.write(out); }
                out.push('}');
            }
        }
    }

    /// Parse JSON (what this file holds: no floats, booleans or nulls; those read as 0).
    pub fn parse(s: &str) -> Option<J> {
        fn ws(b: &[u8], i: &mut usize) { while *i < b.len() && b[*i].is_ascii_whitespace() { *i += 1; } }
        fn val(s: &str, i: &mut usize) -> Option<J> {
            let b = s.as_bytes();
            ws(b, i);
            match *b.get(*i)? {
                b'{' => {
                    *i += 1;
                    let mut o = vec![];
                    loop {
                        ws(b, i);
                        if b.get(*i) == Some(&b'}') { *i += 1; return Some(J::Obj(o)); }
                        let J::Str(k) = val(s, i)? else { return None };
                        ws(b, i);
                        if b.get(*i) != Some(&b':') { return None; }
                        *i += 1;
                        o.push((k, val(s, i)?));
                        ws(b, i);
                        if b.get(*i) == Some(&b',') { *i += 1; }
                    }
                }
                b'[' => {
                    *i += 1;
                    let mut a = vec![];
                    loop {
                        ws(b, i);
                        if b.get(*i) == Some(&b']') { *i += 1; return Some(J::Arr(a)); }
                        a.push(val(s, i)?);
                        ws(b, i);
                        if b.get(*i) == Some(&b',') { *i += 1; }
                    }
                }
                b'"' => {
                    *i += 1;
                    let mut t = String::new();
                    let mut chars = s[*i..].char_indices();
                    while let Some((k, c)) = chars.next() {
                        match c {
                            '"' => { *i += k + 1; return Some(J::Str(t)); }
                            '\\' => match chars.next()?.1 {
                                'n' => t.push('\n'),
                                't' => t.push('\t'),
                                'u' => { let h: String = (0..4).filter_map(|_| chars.next().map(|c| c.1)).collect(); t.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?).unwrap_or('?')) }
                                c => t.push(c),
                            },
                            c => t.push(c),
                        }
                    }
                    None
                }
                _ => {
                    let st = *i;
                    while *i < b.len() && !matches!(b[*i], b',' | b']' | b'}') && !b[*i].is_ascii_whitespace() { *i += 1; }
                    Some(J::Num(s[st..*i].parse::<f64>().map_or(0, |f| f as i64)))
                }
            }
        }
        val(s, &mut 0)
    }
}

/// skirmishmasters.cfg in the player folder; the first time the real game's (read only), like settings.cfg.
fn file() -> PathBuf { skirmish::profile_dir().join("skirmishmasters.cfg") }

/// The saved games, in file order (ConfigSettings.LoadSkirmishMasters: only version 2 kept, re-indexed).
pub fn load() -> Vec<J> {
    let text = std::fs::read_to_string(file()).or_else(|_| std::fs::read_to_string(crate::paths::game_user().join("skirmishmasters.cfg"))).unwrap_or_default();
    let Some(J::Arr(v)) = J::parse(text.trim_start_matches('\u{feff}')).and_then(|j| j.get("skirmishMastersData").cloned()) else { return vec![] };
    let mut v: Vec<J> = v.into_iter().filter(|e| e.int("version") == 2).collect();
    for (k, e) in v.iter_mut().enumerate() { e.set("index", J::Num(k as i64)); }
    v
}

/// SaveSkirmishMasters (re-indexed first).
fn save(mut v: Vec<J>) -> std::io::Result<()> {
    for (k, e) in v.iter_mut().enumerate() { e.set("index", J::Num(k as i64)); }
    let mut out = String::from("{\"skirmishMastersData\":");
    J::Arr(v).write(&mut out);
    out.push('}');
    std::fs::create_dir_all(skirmish::profile_dir())?;
    std::fs::write(file(), out)
}

/// DeleteSkirmishMastersGame: the entry with this `index`.
pub fn delete(index: i64) -> std::io::Result<()> {
    let mut v = load();
    if let Some(k) = v.iter().position(|e| e.int("index") == index) { v.remove(k); }
    save(v)
}

/// The game a row is for (HUD_MissionOver.ShowMPScore's trailLevel): a trail mission (level as stored), or a skirmish
/// on the map with this name (trailLevel −1), a custom trail's mission (customTrailLevel, its trail's name).
pub enum Kind { Trail(i32), Skirmish(String), Custom(i32, String) }

/// main.rs's game as a Kind: a trail / co-op mission (`trail` = front-end trail id, `id` 0-based), else the skirmish
/// lobby's map.
pub fn kind(trail: Option<(i32, i32)>, lobby_map: Option<&std::path::Path>) -> Option<Kind> {
    match (trail, lobby_map) {
        // trail type 0 / 1 / 2: level, + 50, + 80; Sands type × 1000 + level; co-op 20000 / 21000 / 22000 + mission
        (Some((90..=92, id)), _) => Some(Kind::Custom(id + 1, custom_trails::current()?.name)),
        (Some((t, id)), _) => Some(Kind::Trail(match t { 0 => id, 1 => id + 50, 2 => id + 80, 21..=23 => 20000 + (t - 21) * 1000 + id, _ => t * 1000 + id })),
        (None, Some(p)) => Some(Kind::Skirmish(p.file_stem()?.to_string_lossy().into_owned())),
        _ => None,
    }
}

/// The game-over statistics of the game that just ended (sim thread, Live::hud once the game is over) and its play
/// time in ms and the players' colours (endgame::radar_colours); taken by `record`.
pub(crate) static CAPTURED: std::sync::Mutex<Option<(Vec<u8>, u32, [u8; 9])>> = std::sync::Mutex::new(None);

#[cfg(feature = "live")]
pub fn capture(m: &shc_sim::mem::Mem, real_ms: u32) { *CAPTURED.lock().unwrap() = Some((shc_sim::mpscore::get(m), real_ms, crate::endgame::radar_colours(m))); }

/// HUD_MissionOver.calc_greatest_lord_points (1113-1128) for player `p` of an entry.
pub fn score(e: &J, p: usize) -> i64 {
    let at = |k: &str| e.ints(k).get(p).copied().unwrap_or(0);
    let a = at("troop_points_killed") + at("gold_acquired") / 10 + at("enemy_buildings_razed_points") * 100;
    let b = a + a * at("lords_killed") / 4;
    let d = e.ints("blank2"); // the game's calendar: start year, month, end year, month
    let months = ((d.get(3).copied().unwrap_or(0) + d.get(2).copied().unwrap_or(0) * 12) - (d.get(1).copied().unwrap_or(0) + d.first().copied().unwrap_or(0) * 12)).max(1);
    b * 200 / (200 + months)
}

/// MPScoreData's fields in declaration order (= the JSON key order) with their DLL_GetMPScoreData offset and length
/// (EngineInterface 3351-3570, convertMPStats; teams sits last in the export but before computer_register here).
const FIELDS: [(&str, usize, usize); 31] = [
    ("valid", 0, 9), ("gold_acquired", 36, 9), ("max_population", 72, 9), ("fearfactor", 108, 9), ("time_deceased", 144, 9),
    ("who_killed_who", 180, 81), ("enemy_buildings_destroyed", 504, 9), ("food_produced", 540, 9), ("iron_produced", 576, 9),
    ("stone_produced", 612, 9), ("wood_produced", 648, 9), ("pitch_produced", 684, 9), ("minfearfactor", 720, 9), ("winners", 756, 9),
    ("troop_points_killed", 792, 9), ("enemy_buildings_razed_points", 828, 9), ("troops_produced", 864, 9), ("goods_received", 900, 9),
    ("goods_sent", 936, 9), ("notable_victories", 972, 9), ("notable_defeats", 1008, 9), ("time_lord_killed", 1044, 9),
    ("blank2", 1080, 9), ("blank3", 1116, 9), ("blank4", 1152, 9), ("weapons_produced", 1188, 9), ("buildings_lost", 1224, 9),
    ("lords_killed", 1260, 9), ("team_shield", 1296, 9), ("teams", 1392, 9), ("computer_register", 1332, 9),
];

/// The row ConfigSettings.AddSkirmishMastersGame appends for statistics `d` (DLL_GetMPScoreData bytes) of a game that
/// took `real_ms`, finished `now` (unix seconds, UTC): convertMPStats + ShowMPScore's score / numPlayers / trailLevel;
/// colourMap2 = the players' colours (SpriteMapping.remapColours; `colours` as captured, 0 = the default).
pub fn entry(d: &[u8], real_ms: u32, now: u64, kind: &Kind, me: usize, names: &dyn Fn(usize, i64) -> String, colours: &[u8; 9]) -> J {
    let i32_at = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as i64;
    let mut o: Vec<(String, J)> = FIELDS.iter().map(|&(k, at, n)| (k.to_string(), J::Arr((0..n).map(|i| J::Num(i32_at(at + 4 * i))).collect()))).collect();
    let real = if i32_at(1368) > 0 { i32_at(1368) } else { real_ms as i64 }; // the port keeps the engine's clock at 0
    let unique = match i64::from_le_bytes(d[1384..1392].try_into().unwrap()) { 0 => now as i64 * 1000 + (real_ms % 1000) as i64, u => u };
    for (k, v) in [("real_time", real), ("game_time", i32_at(1372)), ("ranged_made", i32_at(1376)), ("melee_made", i32_at(1380)), ("unique", unique)] {
        o.push((k.into(), J::Num(v)));
    }
    let (y, mo, da) = civil(now as i64 / 86400);
    let sec = now as i64 % 86400;
    for (k, v) in [("Year", y), ("Month", mo), ("Day", da), ("Hour", sec / 3600), ("Minute", sec / 60 % 60), ("Second", sec % 60)] {
        o.push((format!("completedDate_{k}"), J::Num(v)));
    }
    let mut e = J::Obj(o);
    let valid = e.ints("valid");
    let cr = e.ints("computer_register");
    let (level, map, trail) = match kind { Kind::Trail(l) => (*l as i64, String::new(), String::new()), Kind::Skirmish(m) => (-1, m.clone(), String::new()), Kind::Custom(l, n) => (*l as i64, String::new(), n.clone()) };
    for (k, v) in [("score", J::Num(score(&e, me))), ("lord_type", J::Num(skirmish::Settings::load().get("LordType").and_then(|v| v.parse().ok()).unwrap_or(0))),
        ("numPlayers", J::Num(valid.iter().skip(1).filter(|&&v| v > 0).count() as i64)), ("trailLevel", J::Num(level)), ("mapName", J::Str(map)),
        ("playerName", J::Arr((0..9).map(|l| J::Str(if valid.get(l).is_some_and(|&v| v > 0) { names(l, cr.get(l).copied().unwrap_or(0)) } else { String::new() })).collect())),
        ("colourMap1", J::Arr(vec![J::Num(-1); 9])), ("colourMap2", J::Arr((0..9u8).map(|p| J::Num(crate::endgame::colour(colours, p) as i64)).collect())),
        ("version", J::Num(2)), ("index", J::Num(0)), ("trailName", J::Str(trail))] {
        e.set(k, v);
    }
    e
}

/// Days since 1970-01-01 → (year, month, day) (proleptic Gregorian, H. Hinnant's civil_from_days).
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

/// HUD_MissionOver.ShowMPScore → AddSkirmishMastersGame: the game that just ended as a row (main.rs, once, when its
/// end box is clicked). Nothing when no statistics were captured (not a game that ended).
pub fn record(kind: Kind) {
    let Some((d, real_ms, colours)) = CAPTURED.lock().unwrap().take() else { return };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |t| t.as_secs());
    let e = entry(&d, real_ms, now, &kind, 1, &names(), &colours);
    let mut v = load();
    v.push(e);
    if let Err(e) = save(v) { eprintln!("skirmish masters: {e}"); }
}

/// MPScoreData.playerName: you (slot 1, or a human) by the profile's name, an AI lord by its name.
fn names() -> impl Fn(usize, i64) -> String {
    let me = player_name();
    move |l, cr| if cr == 0 || l == 1 { me.clone() } else { skirmish::LORDS.get((cr - 1).max(0) as usize).copied().unwrap_or("").to_string() }
}

/// The game that just ended as the score screen shows it (the captured statistics, left for `record`).
pub fn peek() -> Option<J> {
    let (d, ms, colours) = CAPTURED.lock().unwrap().clone()?;
    Some(entry(&d, ms, 0, &Kind::Skirmish(String::new()), 1, &names(), &colours))
}

/// The score screen's buttons: a page tab, a column's sort, the original's Exit (= continue), and the port's own
/// Play Again / Main Menu / Save Replay / Graphs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Btn { Page(u8), Sort(u8), Help(u8), Exit, Again, Menu, Replay, Graphs }

/// mp_help_text[help] as strings.tsv keys (lookUpText is 0-based): 0 the rank, 1..15 the columns (= mp_sortType),
/// 16 / 17 the Troops / Eco tabs, 18 fear, 19 exit.
const HELP: [&str; 20] = ["TEXT_MP_RANK_002", "TEXT_DEMOSCORE_019", "TEXT_DEMOSCORE_026", "TEXT_DEMOSCORE_020", "TEXT_DEMOSCORE_021",
    "TEXT_DEMOSCORE_027", "TEXT_DEMOSCORE_028", "TEXT_DEMOSCORE_029", "TEXT_DEMOSCORE_030", "TEXT_DEMOSCORE_031", "TEXT_DEMOSCORE_022",
    "TEXT_SKIRMISH_CHOOSE2_020", "TEXT_SKIRMISH_CHOOSE2_021", "TEXT_SKIRMISH_MISC_001", "TEXT_SKIRMISH_MISC_002", "TEXT_SKIRMISH_MISC_003",
    "TEXT_SCENARIO_026", "TEXT_NEW_TEXT_060", "TEXT_DEMOSCORE_023", "TEXT_HELP_003"];
/// The pages' header columns (HUD_MissionOver.xaml): the sort / help index and its UI-Buttons S sprite (normal, over,
/// checked follow). Page 1 = 580 + 6 × 150 px, page 2 = 600 + 10 × 110 px (the last, fear, only explains).
const PAGES: [&[(u8, u32)]; 2] = [&[(0, 45), (1, 55), (3, 58), (2, 85), (12, 88), (13, 91), (4, 48)],
    &[(0, 45), (5, 61), (6, 64), (7, 67), (8, 70), (9, 73), (11, 76), (10, 79), (14, 94), (15, 97), (18, 82)]];
/// MPScoreData field of the columns 2, 4..15 (1 and 3 sum who_killed_who).
const COLS: [&str; 16] = ["", "", "enemy_buildings_destroyed", "", "gold_acquired", "food_produced", "wood_produced", "stone_produced",
    "iron_produced", "pitch_produced", "max_population", "weapons_produced", "buildings_lost", "troops_produced", "goods_received", "goods_sent"];

/// Column `sort` (mp_sortType 1..15) of player p: enemy troops killed = who_killed_who[p * 9 + i], troops lost =
/// [i * 9 + p] (prepMPScores), the rest straight from the statistics.
fn stat(e: &J, sort: u8, p: usize) -> i64 {
    let w = e.ints("who_killed_who");
    let k = |a: usize, b: usize| w.get(a * 9 + b).copied().unwrap_or(0);
    match sort {
        1 => (1..9).filter(|&i| i != p).map(|i| k(p, i)).sum(),
        3 => (0..9).filter(|&i| i != p).map(|i| k(i, p)).sum(),
        s => e.ints(COLS[s as usize & 15]).get(p).copied().unwrap_or(0),
    }
}

/// prepMPScores' rankings of players 1..8 (its bubble sorts swap on strict order: stable sorts): 0 = the greatest lord
/// (score, then the later death, then the winner), else column `sort` best first (fewest lost for 3 and 12).
fn order(e: &J, sort: u8) -> Vec<usize> {
    let mut v: Vec<usize> = (1..9).collect();
    let at = |k: &str, p: usize| e.ints(k).get(p).copied().unwrap_or(0);
    match sort {
        0 => v.sort_by_key(|&p| std::cmp::Reverse((score(e, p), at("time_deceased", p), at("winners", p)))),
        3 | 12 => v.sort_by_key(|&p| stat(e, sort, p)),
        _ => v.sort_by_key(|&p| std::cmp::Reverse(stat(e, sort, p))),
    }
    v
}

/// prepMPScores' stars: 3 points for the greatest lord, more for a column won by over twice the runner-up (and over a
/// floor), a star per 3 points, at most 5 (4 when slot 1 is empty: its num_players loop counts valid[1] 8 times).
/// Kept as it is: the highest population's point goes to the enemy-troops leader.
fn stars(e: &J) -> [i64; 9] {
    let mut pts = [0i64; 9];
    pts[order(e, 0)[0]] += 3;
    // (column, floor, points, the column whose leader gets them)
    for (sort, min, add, to) in [(1, 10, 3, 1), (2, 2, 2, 2), (5, 50, 2, 5), (4, 100, 3, 4), (10, 24, 1, 1), (6, 100, 1, 6), (7, 50, 1, 7), (8, 10, 1, 8), (9, 20, 1, 9), (11, 20, 1, 11)] {
        let o = order(e, sort);
        let (a, b) = (stat(e, sort, o[0]), stat(e, sort, o[1]));
        if a > min && a > b * 2 { pts[order(e, to)[0]] += add; }
    }
    let full = e.ints("valid").get(1).is_some_and(|&v| v > 0);
    pts.map(|x| match (x / 3).min(5) { 5 if !full => 4, s => s })
}

/// OnScreenText's month + year of a month count (TEXT_MONTHS is 0-based).
fn month_year(ui: &Ui, months: i64, year: i64) -> String { format!("{} {year}", ui.text_of(&format!("TEXT_MONTHS_{:03}", months.rem_euclid(12) + 1))) }

/// HUD_MissionOver's MP score (ShowMPVictory / ShowMPDefeat → ShowMPScore): the Victory / Defeat heading, the help line
/// (the hovered button's text, else the sort's; under the rank the game's first and last month), the Troops / Eco tabs,
/// a header of sort buttons per page and a 70 px row per player (colour shield, a crown per star and for the winner,
/// a star per lord killed, the month the own lord fell, the page's values; page 2 ends with the fear factor pie), the
/// rows ranked by the sort (the same button again reverses it), then Exit. The port adds Play Again, Main Menu, Save
/// Replay and its graphs (stats.rs) along the bottom. 1920×1080 design space, centred.
pub struct Score { e: J, stars: [i64; 9], page: u8, sort: u8, rev: bool, hits: Vec<([f32; 4], Btn)>, lay: Menu }

impl Score {
    pub fn new(e: J) -> Self { Score { stars: stars(&e), e, page: 0, sort: 0, rev: false, hits: vec![], lay: Menu::main() } }

    /// The rows top to bottom (MO_MP_PlayersVisible: valid slots only).
    pub fn rows(&self) -> Vec<usize> {
        let mut o = order(&self.e, self.sort);
        if self.rev { o.reverse(); }
        let valid = self.e.ints("valid");
        o.into_iter().filter(|&p| valid.get(p).is_some_and(|&v| v > 0)).collect()
    }

    /// `again`: the game can be restarted (Play Again shown); `saved`: its replay was saved.
    pub fn draw(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, won: bool, again: bool, saved: bool, w: f32, h: f32, cursor: [f32; 2]) {
        let rows = self.rows();
        let m = &mut self.lay;
        m.s = (w / 1920.0).min(h / 1080.0);
        m.o = [(w - 1920.0 * m.s) / 2.0, (h - 1080.0 * m.s) / 2.0];
        (ui.s, ui.w, ui.h) = (m.s, w, h);
        let n = rows.len() as f32;
        // the dim backdrop (the original's: the faction's win / lose video), BDR_SH1DE60 under the rows' stack
        Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.85]);
        Ui::fill(out, m.r(62.0, 202.0, 1796.0, 261.0 + 70.0 * n), [0.0, 0.0, 0.0, 0.6]);
        m.frame(out, 60.0, 200.0, 1800.0, 265.0 + 70.0 * n, false);
        m.o[1] += 50.0 * m.s; // WGT_Heading2 in the grid's 150 px first row (50 px down)
        let title = ui.text_of(if won { "TEXT_MAINOPTIONS_017" } else { "TEXT_MAINOPTIONS_018" });
        m.heading(ui, out, 602.5, &title);
        m.o[1] -= 50.0 * m.s;
        let mut hits: Vec<([f32; 4], Btn)> = vec![];
        let hot = |p: [f32; 4]| Menu::inside(p, cursor);
        // a radio button: Sprite1 normal, 2 over, 3 checked; `fit` = its Height, the width follows the sprite
        let radio = |ui: &Ui, out: &mut Vec<HudTri>, p: [f32; 4], s: u32, on: bool, b: Btn, hits: &mut Vec<([f32; 4], Btn)>| {
            let k = if on { s + 2 } else if hot(p) { s + 1 } else { s };
            ui.img_fit(out, &format!("UI-Buttons S{k:03}"), p, WHITE);
            hits.push((p, b));
        };
        // the tabs (right of the help line)
        for (k, (s, right)) in [(51, 160.0), (61, 60.0)].into_iter().enumerate() {
            radio(ui, out, m.r(1920.0 - right - 71.0, 188.0, 71.0, 110.0), s, self.page == k as u8, Btn::Page(k as u8), &mut hits);
        }
        // the header: the rank (S045, 347 × 100 at the left), then the page's columns centred in theirs
        let (c0, cw) = if self.page == 0 { (580.0, 150.0) } else { (600.0, 110.0) };
        for (k, &(sort, s)) in PAGES[self.page as usize].iter().enumerate() {
            let p = if k == 0 { m.r(110.0, 275.0, 347.0, 100.0) } else { m.r(110.0 + c0 + cw * (k - 1) as f32 + cw / 2.0 - 35.5, 270.0, 71.0, 110.0) };
            radio(ui, out, p, s, sort < 16 && sort == self.sort, if sort < 16 { Btn::Sort(sort) } else { Btn::Help(sort) }, &mut hits);
        }
        // the rows: shield, crowns + stars + the lord's fall, name; the values right-aligned in their columns
        let e = &self.e;
        let names = e.strs("playerName");
        let (winners, lk, tlk) = (e.ints("winners"), e.ints("lords_killed"), e.ints("time_lord_killed"));
        let at = |v: &Vec<i64>, p: usize| v.get(p).copied().unwrap_or(0);
        for (i, &p) in rows.iter().enumerate() {
            let y = 375.0 + 70.0 * i as f32;
            let colours: Vec<u8> = e.ints("colourMap2").iter().map(|&c| c.clamp(0, 8) as u8).collect();
            ui.img_fit(out, crate::endgame::shield(crate::endgame::colour(&colours, p as u8)), m.r(119.0, y + 18.5, 32.0, 33.0), WHITE);
            let mut x = 160.0;
            for _ in 0..((at(&winners, p) > 0) as i64 + self.stars[p]).min(7) { ui.img(out, "koth", m.r(x, y + 10.0, 30.0, 25.0), WHITE); x += 30.0; }
            for _ in 0..at(&lk, p).min(7) { ui.img(out, "MP_Star", m.r(x, y + 10.0, 26.0, 37.0), WHITE); x += 26.0; }
            if at(&tlk, p) > 0 {
                x += m.label(ui, out, "  (", x, y + 20.0, 20.0, 0.0, CREAM) / m.s;
                ui.img_fit(out, "UI-Buttons S054", m.r(x, y + 10.0, 14.0, 20.0), WHITE);
                x += 14.0;
                let t = month_year(ui, at(&tlk, p), at(&tlk, p) / 12);
                x += m.label(ui, out, &t, x, y + 20.0, 20.0, 0.0, CREAM) / m.s;
                m.label(ui, out, ")", x, y + 20.0, 20.0, 0.0, CREAM);
            }
            m.label(ui, out, names.get(p).map_or("", |s| s.as_str()), 160.0, y + 52.0, 24.0, 0.0, CREAM);
            let (v0, vw) = if self.page == 0 { (630.0, 150.0) } else { (670.0, 110.0) };
            for (k, &(sort, _)) in PAGES[self.page as usize].iter().enumerate().skip(1) {
                let cx = v0 + vw * (k - 1) as f32;
                if sort < 16 { m.label(ui, out, &stat(e, sort, p).to_string(), cx + vw, y + 38.0, 24.0, 1.0, CREAM); }
                else { // GetPlayerFear: GameSprites[115 + fear * 6 − min fear] = Pie 001..
                    let pie = at(&e.ints("fearfactor"), p) * 6 - at(&e.ints("minfearfactor"), p);
                    ui.img_fit(out, &format!("Pie {:03}", (pie + 1).clamp(1, 36)), m.r(1736.0, y + 21.0, 28.0, 28.0), WHITE);
                }
            }
        }
        for (x, y, w_, h_) in [(110.0, 375.0, 1700.0, 2.0), (110.0, 373.0 + 70.0 * n, 1700.0, 2.0), (110.0, 375.0, 2.0, 70.0 * n), (1808.0, 375.0, 2.0, 70.0 * n)] {
            Ui::fill(out, m.r(x, y, w_, h_), [0.627, 0.0, 0.0, 1.0]); // #FFA00000, 2 px
        }
        // help / exit row: the original's Exit (B001, 60 high, 50 px from the right); the port's buttons on the left
        let by = 375.0 + 70.0 * n;
        let p = m.r(1746.0, by + 15.0, 124.0, 60.0);
        ui.img_fit(out, if hot(p) { "UI-Buttons B002" } else { "UI-Buttons B001" }, p, WHITE);
        hits.push((p, Btn::Exit));
        let buttons = [("Play Again", Btn::Again), ("Main Menu", Btn::Menu), (if saved { "Replay saved" } else { "Save Replay" }, Btn::Replay), ("Graphs", Btn::Graphs)];
        for (k, (label, b)) in buttons.into_iter().filter(|b| again || b.1 != Btn::Again).enumerate() {
            let p = m.r(110.0 + 250.0 * k as f32, by + 21.0, 230.0, 48.0);
            ui.text_button(out, label, p, hot(p), false, b != Btn::Replay || !saved);
            hits.push((p, b));
        }
        // the help line: the hovered button's text, else the sort's (UpdateHelpText); the dates under the rank's
        let help = hits.iter().rev().find(|(p, _)| hot(*p)).map_or(self.sort, |&(_, b)| match b {
            Btn::Sort(s) | Btn::Help(s) => s, Btn::Page(k) => 16 + k, Btn::Exit => 19, _ => self.sort,
        });
        let t = ui.text_of(HELP[help as usize]);
        m.label(ui, out, &t, 960.0, 228.0, 32.0, 0.5, CREAM);
        let d = e.ints("blank2");
        if help == 0 && d.first().is_some_and(|&y| y != 0) {
            let at = |k: usize| d.get(k).copied().unwrap_or(0);
            let s = format!("{} - {}", month_year(ui, at(1), at(0)), month_year(ui, at(3), at(2)));
            m.label(ui, out, &s, 960.0, 260.0, 20.0, 0.5, CREAM);
        }
        self.hits = hits;
    }

    /// A left press: page tabs and sorts act here (ButtonClicked: the same sort again reverses it); the others are
    /// returned for the end screen.
    pub fn click(&mut self, c: [f32; 2]) -> Option<Btn> {
        let b = self.hits.iter().rev().find(|(p, _)| Menu::inside(*p, c))?.1;
        match b {
            Btn::Page(k) => (self.page, self.sort, self.rev) = (k, 0, false),
            Btn::Sort(s) => (self.sort, self.rev) = (s, s == self.sort && !self.rev),
            _ => return Some(b),
        }
        crate::audio::ui(crate::audio::ui_id::RED_BUTTON, 1.0);
        None
    }
}

/// GetTrailMapName: a row's name (and the trail it belongs to, empty for a skirmish).
fn row_name(ui: &Ui, e: &J) -> (String, String) {
    let l = e.int("trailLevel") as i32;
    if l < 0 { return (e.str("mapName"), String::new()); }
    let trail = e.str("trailName");
    if !trail.is_empty() { return (format!("{l}. {trail}"), format!("{} {trail}", ui.text_of("TEXT_CUSTOMISATION_056"))); }
    let (t, n) = match l { 0..=49 => (0, l), 50..=79 => (1, l - 50), 80..=99 => (2, l - 80), 20000..=20999 => (100, l % 1000), 21000.. => (101, l % 1000), _ => (l / 1000, l % 1000) };
    let tx = |k: String| ui.text_of(&k);
    match t {
        0 => (format!("{}. {}", n + 1, tx(format!("TEXT_TRAIL_NAMES_CRU_{:03}", n + 2))), tx("TEXT_SKIRMISH_CHOOSE2_007".into())),
        1 => (format!("{}. {}", n + 51, tx(format!("TEXT_TRAIL_NAMES_CRU_{:03}", n + 52))), tx("TEXT_SKIRMISH_CHOOSE2_022".into())),
        2 => (format!("{}. {}", n + 1, tx(format!("TEXT_TRAIL_NAMES_CRU_{:03}", n + 82))), tx("TEXT_SKIRMISH_CHOOSE2_024".into())),
        11..=17 => {
            let base = [7, 12, 19, 28, 39, 49, 82][(t - 11) as usize];
            (format!("{}. {}", n + 1, tx(format!("TEXT_SANDS_OF_TIME_{:03}", base + n + 1))), tx(format!("TEXT_SANDS_OF_TIME_{:03}", if t == 17 { 81 } else { t - 9 })))
        }
        100 => (tx(format!("TEXT_COOP_{:03}", n + 2)), tx("TEXT_COOP_023".into())),
        _ => (tx(format!("TEXT_COOP_{:03}", n + 12)), tx("TEXT_COOP_024".into())),
    }
}

/// The rows the screen shows (FRONT_SkirmishMasters 166-296): the filters (bit 0 trails, bit 1 the others, bit 2 best
/// score only: per trailLevel, else per game, ties to the quicker), then the sort (0 score, 1 name, 2 time, 3 date).
pub fn rows(ui: &Ui, all: &[J], sort: u8, flags: u8) -> Vec<J> {
    let mut v: Vec<J> = all.iter().filter(|e| if e.int("trailLevel") >= 0 { flags & 1 != 0 } else { flags & 2 != 0 }).cloned().collect();
    if flags & 4 != 0 {
        let key = |e: &J| if e.int("trailLevel") >= 0 { (0, e.int("trailLevel")) } else { (1, e.int("unique")) };
        let mut best: Vec<J> = vec![];
        for e in v {
            if e.int("trailLevel") < 0 && e.int("unique") == 0 { best.push(e); continue; }
            match best.iter_mut().find(|b| key(b) == key(&e)) {
                Some(b) if (e.int("score"), -e.int("real_time")) > (b.int("score"), -b.int("real_time")) => *b = e,
                Some(_) => {}
                None => best.push(e),
            }
        }
        v = best;
    }
    let date = |e: &J| ["Year", "Month", "Day", "Hour", "Minute", "Second"].map(|k| e.int(&format!("completedDate_{k}")));
    match sort {
        1 => v.sort_by_cached_key(|e| row_name(ui, e).0),
        2 => v.sort_by_key(|e| (e.int("real_time"), -e.int("score"))),
        3 => v.sort_by_key(|e| std::cmp::Reverse(date(e))),
        _ => v.sort_by_key(|e| (-e.int("score"), e.int("real_time"))),
    }
    v
}

impl Menu {
    /// FRONT_SkirmishMasters (1366×768 grid over SkirmishMasters.png): banner, the 1100×550 panel with the sort buttons,
    /// 10 rows of 44 px (#, name / date / time, the players' faces by team, score), the filters, Back; `del` = the row
    /// whose delete waits for its confirmation (TEXT_SKMASTERS 13).
    pub(super) fn masters_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, tr: i32, sort: u8, flags: u8, del: i64, w: f32, h: f32, cursor: [f32; 2]) {
        self.s = (w / 1366.0).min(h / 768.0);
        self.o = [(w - 1366.0 * self.s) / 2.0, (h - 768.0 * self.s) / 2.0];
        ui.s = self.s;
        let all = load();
        let rows = rows(ui, &all, sort, flags);
        // the generic wheel (menu.rs) scrolls `files` by 18 rows: padded so it stops at the last 10
        if self.files.len() != rows.len() + 8 { self.files = vec![PathBuf::new(); rows.len() + 8]; self.scroll = self.scroll.min(rows.len().saturating_sub(10)); }
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        let t = |m: &Self, ui: &Ui, k: &str| { let _ = m; ui.text_of(k) };
        self.frame(out, 325.5, 25.0, 715.0, 60.0, true);
        for x in [345.5, 1000.5] { ui.img(out, "Panel_CelticKnot", self.r(x, 35.0, 20.0, 40.0), WHITE); }
        self.small_caps(ui, out, &t(self, ui, "TEXT_SKIRMISH_MASTERS_001"), 683.0, 68.0, 40.0, BLACK);
        let (px, py) = (133.0, 125.0);
        self.frame(out, px, py, 1100.0, 550.0, true);
        let sorts = ["Score", "Name", "Time", "Data"];
        for (k, n) in sorts.iter().enumerate() {
            let p = self.r(px + 20.0 + 40.0 * k as f32, py + 10.0, 33.0, 33.0);
            let key = if k as u8 == sort { format!("SkirmishMasters_{n}Selected") } else if Self::inside(p, cursor) { format!("SkirmishMasters_{n}Over") } else { format!("SkirmishMasters_{n}") };
            ui.img_fit(out, &key, p, WHITE);
            self.hits.push((p, Act::Trail(A::MSort(k as u8))));
        }
        self.label(ui, out, &t(self, ui, &format!("TEXT_SKMASTERS_{:03}", [19, 20, 22, 21][sort as usize & 3])), px + 350.0, py + 27.0, 20.0, 0.0, BLACK);
        for y in [49.0, 501.0] { ui.img(out, "Panel_Divider", self.r(px + 4.0, py + y, 1092.0, 4.0), WHITE); }
        for (j, e) in rows.iter().enumerate().skip(self.scroll).take(10) {
            let (x, y) = (px + 4.0, py + 54.0 + 44.0 * (j - self.scroll) as f32);
            ui.img(out, if j % 2 == 0 { "SkirmishMastersBar1" } else { "SkirmishMastersBar2" }, self.r(x, y, 1092.0, 44.0), WHITE);
            self.label(ui, out, &(j + 1).to_string(), x + 25.0, y + 22.0, 24.0, 0.5, BLACK);
            let (name, sub) = row_name(ui, e);
            self.label(ui, out, &name, x + 74.0, y + 13.0, 20.0, 0.0, BLACK);
            let date = format!("{} {} {}", e.int("completedDate_Day"), ui.text_of(&format!("TEXT_MONTHS_{:03}", e.int("completedDate_Month").clamp(1, 12))), e.int("completedDate_Year"));
            self.label(ui, out, &date, x + 74.0, y + 33.0, 16.0, 0.0, BLACK);
            self.label(ui, out, &sands_time((e.int("real_time") / 1000) as i32), x + 320.0, y + 33.0, 16.0, 1.0, BLACK);
            // the players by team, crossed swords between the teams; a fallen lord greyed
            let (valid, teams, cr, dead) = (e.ints("valid"), e.ints("teams"), e.ints("computer_register"), e.ints("time_lord_killed"));
            let mut ps: Vec<usize> = (1..9).filter(|&l| valid.get(l).is_some_and(|&v| v > 0)).collect();
            ps.sort_by_key(|&l| (teams.get(l).copied().unwrap_or(0) == 0, teams.get(l).copied().unwrap_or(0), l));
            let mut fx = x + 340.0;
            for (i, &l) in ps.iter().enumerate() {
                if i > 0 && (teams[l] == 0 || teams[l] != teams[ps[i - 1]]) { ui.img_fit(out, "UI-Buttons S041", self.r(fx, y + 6.0, 32.0, 32.0), WHITE); fx += 34.0; }
                let face = self.r(fx, y + 4.0, 36.0, 36.0);
                let c = if dead.get(l).is_some_and(|&d| d != 0) { [0.45, 0.45, 0.45, 1.0] } else { WHITE };
                if cr.get(l).copied().unwrap_or(0) == 0 { // you: your coat of arms (GetLocalAvatar)
                    if !crate::menu::coa::draw_mine(out, face, c) { ui.img_fit(out, "shield_crusader", face, c); }
                    ui.img(out, "Face Border", face, c);
                }
                else if let Some(&(f, _)) = FACES.iter().find(|f| f.1 as i64 == cr[l] - 1) { ui.img(out, f, face, c); }
                fx += 41.0;
            }
            if !sub.is_empty() { self.label(ui, out, &sub, x + 950.0, y + 36.0, 12.0, 1.0, BLACK); }
            ui.img_fit(out, "koth", self.r(x + 966.0, y + 2.0, 40.0, 40.0), WHITE);
            self.label(ui, out, &e.int("score").to_string(), x + 1016.0, y + 22.0, 24.0, 0.0, BLACK);
            // delete (the original: right-click): a small cross at the row's end, then Yes / No
            let p = self.r(x + 50.0, y + 12.0, 20.0, 20.0); // in the number cell (the score fills the last one)
            ui.img_fit(out, "UI-Buttons S054", p, if Self::inside(p, cursor) { WHITE } else { [1.0, 1.0, 1.0, 0.6] });
            self.hits.push((p, Act::Trail(A::MDel(e.int("index") as i32))));
        }
        if rows.is_empty() { self.label(ui, out, &t(self, ui, "TEXT_SANDS_OF_TIME_069"), 683.0, 280.0, 24.0, 0.5, BLACK); }
        // the filters along the bottom (checked: the tick)
        for (k, (x, key)) in [(px + 20.0, "TEXT_SKMASTERS_023"), (px + 330.0, "TEXT_SKMASTERS_024"), (px + 730.0, "TEXT_SKMASTERS_025")].into_iter().enumerate() {
            let p = self.r(x, py + 512.0, 300.0, 30.0);
            let (b, hot) = (self.r(x, py + 517.0, 20.0, 20.0), Self::inside(p, cursor));
            crate::menu::widget(out, |o| Ui::checkbox(o, b, flags & (1 << k) != 0, hot, hot && ui.down));
            self.label(ui, out, &t(self, ui, key), x + 25.0, py + 527.0, 20.0, 0.0, BLACK);
            self.hits.push((p, Act::Trail(A::MFlag(1 << k))));
        }
        // ponytail: no footer (TEXT_SKIRMISH_MASTERS_006 offers the score screen and a right-click delete, neither here)
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 43.0, 700.0, 106.0, 47.0, Some(Act::Trail(A::Open(T::Map(tr, 1)))), cursor);
        if del >= 0 {
            // the confirmation (MessageBox with TEXT_SKMASTERS 13): over everything, it takes the clicks
            self.hits.clear();
            Ui::fill(out, self.r(0.0, 0.0, 1366.0, 768.0), [0.0, 0.0, 0.0, 0.5]);
            self.frame(out, 433.0, 300.0, 500.0, 170.0, true);
            self.para(ui, out, &t(self, ui, "TEXT_SKMASTERS_014"), 463.0, 320.0, 440.0, 20.0, BLACK);
            self.glow(ui, out, &t(self, ui, "TEXT_GAME_OPTIONS_023"), 463.0, 420.0, 200.0, None, Some(Act::Trail(A::MDelYes)), cursor);
            self.glow(ui, out, &t(self, ui, "TEXT_GAME_OPTIONS_024"), 703.0, 420.0, 200.0, None, Some(Act::Trail(A::MDelNo)), cursor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The score screen's Exit after a skirmish: the skirmish setup with the game's map and lords.
    #[test]
    fn skirmish_again_keeps_the_lobby() {
        let map = skirmish::maps_dir().join("Crater Lake.map");
        if !map.exists() { return; } // needs the game install
        let m = Menu::skirmish_again(Lobby { ais: vec![(0, 0), (2, 0)], seed: 7, ..Lobby::new(map.clone()) });
        assert!(matches!(&m.screen, Screen::Lobby(l) if l.map == map && l.ais.len() == 2 && l.seed == 7));
        assert_eq!(m.files.get(m.files.iter().position(|f| *f == map).unwrap()), Some(&map));
    }

    /// prepMPScores on a 3-player game: kills from who_killed_who, the greatest-lord ranking (score, then the later
    /// death), the stars (the population point lands on the enemy-troops leader), sorts and their reversal, invalid
    /// slots hidden.
    #[test]
    fn score_ranks_and_stars() {
        let mut wkw = vec![0i64; 81];
        (wkw[9 + 2], wkw[2 * 9 + 1], wkw[3 * 9 + 1]) = (40, 5, 3); // 1 killed 40 of 2's; 2 killed 5 of 1's, 3 killed 3
        let arr = |v: &[i64]| J::Arr(v.iter().map(|&n| J::Num(n)).collect());
        let e = J::Obj(vec![
            ("valid".into(), arr(&[0, 1, 1, 1, 0, 0, 0, 0, 0])), ("who_killed_who".into(), arr(&wkw)),
            ("gold_acquired".into(), arr(&[0, 50, 3000, 10, 0, 0, 0, 0, 0])), ("max_population".into(), arr(&[0, 30, 10, 5, 0, 0, 0, 0, 0])),
            ("troop_points_killed".into(), arr(&[0, 400, 0, 0, 0, 0, 0, 0, 0])), ("time_deceased".into(), arr(&[0, 0, 9, 4, 0, 0, 0, 0, 0])),
            ("blank2".into(), arr(&[1180, 4, 1181, 2, 0, 0, 0, 0, 0])),
        ]);
        assert_eq!((stat(&e, 1, 1), stat(&e, 3, 1), stat(&e, 1, 2), stat(&e, 3, 2)), (40, 8, 5, 40));
        // scores: 1 = 400 + 5 = 405, 2 = 300, 3 = 1; × 200 / (200 + 10 months)
        assert_eq!(&order(&e, 0)[..3], [1, 2, 3]);
        let st = stars(&e);
        // 1: rank 3 + enemies 3 + population 1 = 7 → 2 stars; 2: gold 3 → 1
        assert_eq!((st[1], st[2], st[3]), (2, 1, 0));
        let mut s = Score::new(e);
        assert_eq!(s.rows(), [1, 2, 3]);
        (s.sort, s.rev) = (3, false); // troops lost: fewest first
        assert_eq!(s.rows(), [3, 1, 2]);
        s.hits = vec![([0.0, 0.0, 10.0, 10.0], Btn::Sort(3)), ([20.0, 0.0, 30.0, 10.0], Btn::Exit)];
        assert_eq!(s.click([5.0, 5.0]), None);
        assert_eq!(s.rows(), [2, 1, 3], "the same sort again: reversed");
        assert_eq!(s.click([25.0, 5.0]), Some(Btn::Exit));
    }

    /// The real skirmishmasters.cfg (if this machine has the game's): JsonUtility's compact text round-trips byte for
    /// byte, and calc_greatest_lord_points gives every stored score (the local player = the human slot).
    #[test]
    fn real_file_round_trip_and_scores() {
        let Ok(text) = std::fs::read_to_string(crate::paths::game_user().join("skirmishmasters.cfg")) else { return };
        let j = J::parse(&text).expect("parse");
        let mut out = String::new();
        j.write(&mut out);
        assert_eq!(out, text, "round trip");
        let Some(J::Arr(v)) = j.get("skirmishMastersData") else { panic!("no entries") };
        assert!(!v.is_empty());
        for e in v {
            let me = (1..9).find(|&l| e.ints("valid")[l] > 0 && e.ints("computer_register")[l] == 0).unwrap();
            assert_eq!(score(e, me), e.int("score"), "entry {}", e.int("index"));
        }
    }

    /// A row from the native game's statistics (DLL_GetMPScoreData, byte-checked by trailcheck SCORE): the fields in the
    /// original's order, the UTC date, trailLevel per game type; recorded once, listed, filtered, sorted, deleted
    /// through the screen.
    #[test]
    #[cfg(feature = "live")]
    fn record_list_delete() {
        let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
        let dir = std::env::temp_dir().join(format!("shc-masters-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nName:Lord Test\n||SETTINGS||\n").unwrap();
        std::fs::write(dir.join("skirmishmasters.cfg"), "{\"skirmishMastersData\":[]}").unwrap();
        crate::skirmish::set_test_profile(&dir);
        let Ok(mem) = start(0, 0, 1) else { return };
        let mut l = crate::live::Live::from_mem(mem);
        for _ in 0..200 { l.step(); }
        let d = shc_sim::mpscore::get(l.mem());
        // 2026-10-21 07:30:05 UTC
        let e = entry(&d, 61_000, 1792567805, &Kind::Trail(52), 1, &|l, _| format!("p{l}"), &[0, 4, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(e.ints("colourMap2"), [0, 4, 1, 4, 2, 6, 5, 7, 8], "the game's colours, the rest default");
        let J::Obj(o) = &e else { panic!() };
        let keys: Vec<&str> = o.iter().map(|k| k.0.as_str()).collect();
        assert_eq!(&keys[..3], ["valid", "gold_acquired", "max_population"]);
        assert_eq!(&keys[28..31], ["team_shield", "teams", "computer_register"]);
        assert_eq!(keys.last(), Some(&"trailName"));
        assert_eq!(keys.len(), 31 + 5 + 6 + 11);
        assert_eq!([e.int("completedDate_Year"), e.int("completedDate_Month"), e.int("completedDate_Day"), e.int("completedDate_Hour"), e.int("completedDate_Minute")], [2026, 10, 21, 7, 30]);
        assert_eq!((e.int("trailLevel"), e.int("numPlayers"), e.int("version"), e.int("real_time")), (52, 2, 2, 61_000));
        assert_ne!(e.int("unique"), 0);
        assert_eq!(kind(Some((1, 2)), None).map(|k| matches!(k, Kind::Trail(52))), Some(true));
        assert!(matches!(kind(Some((12, 3)), None), Some(Kind::Trail(12003))) && matches!(kind(Some((22, 4)), None), Some(Kind::Trail(21004))));
        assert!(matches!(kind(None, Some(std::path::Path::new("/m/Crater Lake.map"))), Some(Kind::Skirmish(m)) if m == "Crater Lake"));
        assert!(kind(None, None).is_none(), "campaign: no row");
        // recorded once per capture
        capture(l.mem(), 5000);
        record(Kind::Trail(0));
        record(Kind::Trail(0));
        capture(l.mem(), 9000);
        record(Kind::Skirmish("Crater Lake".into()));
        let all = load();
        assert_eq!(all.len(), 2);
        assert_eq!((all[0].int("index"), all[1].int("index"), all[1].int("trailLevel"), all[1].str("mapName")), (0, 1, -1, "Crater Lake".to_string()));
        assert_eq!((rows(&u, &all, 0, 1).len(), rows(&u, &all, 0, 2).len(), rows(&u, &all, 2, 3)[0].int("real_time")), (1, 1, 5000));
        // the screen from the trail map: sort, filter, delete with its confirmation
        let mut m = Menu { screen: Screen::Trail(T::Map(0, 1)), ..Menu::main() };
        let press = |m: &mut Menu, u: &mut Ui, a: Act| {
            m.draw(u, 1920.0, 1080.0, [0.0; 2]);
            let p = m.hits.iter().find(|h| h.1 == a).unwrap_or_else(|| panic!("no button")).0;
            m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0])
        };
        press(&mut m, &mut u, Act::Trail(A::Masters));
        assert!(matches!(m.screen, Screen::Trail(T::Masters { tr: 0, sort: 0, flags: 3, del: -1 })));
        press(&mut m, &mut u, Act::Trail(A::MSort(3)));
        press(&mut m, &mut u, Act::Trail(A::MFlag(1)));
        press(&mut m, &mut u, Act::Trail(A::MFlag(2))); // both off → trails back on
        assert!(matches!(m.screen, Screen::Trail(T::Masters { sort: 3, flags: 1, .. })));
        press(&mut m, &mut u, Act::Trail(A::MDel(0)));
        press(&mut m, &mut u, Act::Trail(A::MDelYes));
        assert_eq!(load().iter().map(|e| e.int("trailLevel")).collect::<Vec<_>>(), vec![-1]);
        assert!(matches!(m.back(), None) && matches!(m.screen, Screen::Trail(T::Map(0, _))));
        std::fs::remove_dir_all(&dir).ok();
    }
}
