//! The Crusader Trails front end, a child module of menu.rs (its screen state, hit list and drawing helpers):
//! FrontendMenus.xaml Show_Frontend_Skirmish (the Skirmish menu: Classic Trails, Co-op, Sands of Time, Custom Trails,
//! Custom skirmish), Show_Frontend_Skirmish_Trails (the three classic trails, Classic Trails options), the Sands of
//! Time menu and the trail maps (FRONT_Trail*.xaml / FRONT_SandsTrail*.xaml, FrontendMenus.cs GenerateSwords /
//! UpdateTrailCampaignSelectedButton / TrailMapClicked); progress, best times and difficulty in settings.cfg like
//! ConfigSettings; the mission start (EditorDirector.LoadSkirmishMap → shc_sim::trail::start) and the result
//! (GameData.ManageGameOver). Spec: side-session notes trail.md. Art: re/scripts/export_trail.py (maps) and
//! export_frontend.py (sprites).
use super::*;
use std::sync::atomic::{AtomicI32, Ordering::Relaxed};

#[path = "masters.rs"]
pub mod masters;
#[path = "custom_trails.rs"]
pub mod custom_trails;

/// Trail screens. `Map(trail, sel)`: the trail map of front-end trail id 0..2 / 11..17 / 21..23 (co-op) with mission
/// `sel` (1-based) selected; `options` = the Classic Trails options popup over `Trails`; `Coop` = the co-op trails;
/// `Brief(trail, id, difficulty)`: the briefing over the loaded, paused mission `id` (0-based); `Board(trail, sel)`: the
/// Sands leaderboard of mission `sel` (1-based); `Masters`: Skirmish Masters (masters.rs) opened from trail `tr`'s map;
/// `Custom`: the custom trails (custom_trails.rs), whose maps are trail ids 90..92.
#[derive(Clone, Copy, PartialEq)]
pub enum T { Skirmish, Trails { options: bool }, Sands, Coop, Map(i32, i32), Brief(i32, i32, i32), Board(i32, i32), Masters { tr: i32, sort: u8, flags: u8, del: i64 }, Custom }

/// Trail clicks (menu.rs `Act::Trail`).
#[derive(Clone, Copy, PartialEq)]
pub enum A { Open(T), Pick(i32), Start, Difficulty, Chicken, Bedouin, Host, AllyOpen, Ally(i32), Swap, Type, Quit, Board, Customize, Masters, MSort(u8), MFlag(u8), MDel(i32), MDelYes, MDelNo, CustomPick(i32), CustomPage(i32) }

/// The classic trails' and Sands of Time trails' ids in menu order.
const CLASSIC: [i32; 3] = [0, 1, 2];
const SANDS: [i32; 7] = [11, 12, 13, 14, 15, 16, 17];
const COOPS: [i32; 3] = [21, 22, 23];

/// Missions of a front-end trail id (FrontendMenus.cs; the engine's tables shc_sim::trail::TRAILS / COOP, checked
/// by the `trail_tables` test; here without the sim so the viewer builds without the `live` feature).
fn count(trail: i32) -> i32 {
    match trail { 0 => 50, 1 => 30, 2 => 20, 11 => 5, 12 => 7, 13 => 9, 14 => 11, 15..=17 => 9, 21..=23 => 10, 90..=92 => custom_trails::current().map_or(0, |c| c.missions.len() as i32), _ => 0 }
}

/// Mission positions on the 3840×2160 map (FrontendMenus.cs trailLocations ..: x, y per mission).
fn coords(trail: i32) -> &'static [i32] {
    const T1: [i32; 100] = [550, 546, 772, 543, 986, 555, 1279, 535, 1397, 434, 1523, 350, 1781, 277, 2026, 237, 2265, 264, 2286, 360,
        2104, 460, 1790, 693, 1551, 732, 1309, 668, 1079, 710, 834, 719, 613, 793, 516, 936, 593, 1056, 789, 1150,
        1084, 1054, 1300, 899, 1579, 879, 1904, 900, 2108, 801, 2191, 698, 2295, 591, 2419, 497, 2584, 520, 2498, 645,
        2388, 731, 2560, 804, 2811, 796, 2834, 938, 3040, 1171, 2905, 1331, 2560, 1376, 2400, 1250, 2305, 1057, 2055, 994,
        1974, 1204, 2019, 1469, 1814, 1600, 1514, 1644, 1327, 1533, 1349, 1374, 1082, 1360, 813, 1481, 737, 1631, 835, 1834];
    const T2: [i32; 60] = [329, 529, 529, 580, 657, 520, 868, 468, 1125, 416, 1363, 383, 1613, 333, 1901, 351, 2116, 245, 2386, 298,
        2316, 378, 2394, 436, 2540, 550, 2688, 733, 2910, 1084, 2539, 1179, 2197, 877, 2112, 544, 1915, 480, 1625, 500,
        1382, 597, 1589, 752, 1775, 997, 2015, 1162, 1492, 1582, 1412, 1354, 902, 1334, 1322, 1054, 1022, 897, 570, 757];
    const T3: [i32; 40] = [529, 540, 444, 679, 448, 818, 507, 983, 611, 1139, 770, 1296, 1020, 1424, 1322, 1520, 1680, 1557, 2078, 1485,
        2289, 1291, 2576, 1168, 2793, 1059, 2665, 909, 2358, 850, 1839, 648, 1644, 748, 1318, 739, 1381, 492, 1539, 366];
    const S1: [i32; 10] = [1900, 524, 2152, 668, 2587, 668, 2450, 844, 2072, 1131];
    const S2: [i32; 14] = [2265, 420, 2363, 555, 2217, 661, 2385, 770, 2330, 920, 2469, 1083, 2819, 1128];
    const S3: [i32; 18] = [2374, 333, 2135, 411, 2054, 546, 2013, 733, 2393, 737, 2726, 755, 2817, 939, 2476, 1085, 2282, 1261];
    const S4: [i32; 22] = [1698, 529, 1937, 392, 2280, 359, 2567, 431, 2674, 581, 2398, 679, 2137, 770, 2548, 835, 2637, 1031, 2839, 1172, 3097, 1096];
    const S5: [i32; 18] = [1824, 316, 2174, 357, 2467, 522, 2793, 642, 2850, 876, 2445, 998, 1902, 918, 2100, 1211, 1739, 1391];
    const S6: [i32; 18] = [1948, 1163, 2139, 892, 1922, 705, 1835, 542, 2122, 405, 2491, 463, 2730, 609, 2847, 807, 2915, 1094];
    const S7: [i32; 18] = [2065, 1139, 2126, 857, 1720, 794, 1843, 581, 2200, 450, 2587, 505, 2900, 648, 2978, 879, 2624, 1120];
    const C1: [i32; 20] = [2280, 235, 1904, 319, 1863, 444, 2180, 518, 2813, 653, 2602, 844, 3208, 1024, 2878, 1287, 2354, 1133, 2211, 942];
    const C2: [i32; 20] = [2364, 802, 2148, 983, 2971, 926, 3180, 826, 2845, 720, 2652, 653, 2391, 644, 2113, 653, 1904, 487, 2450, 179];
    const C3: [i32; 20] = [2661, 1100, 2630, 898, 2504, 779, 2378, 672, 2172, 592, 1946, 555, 1850, 481, 2048, 422, 1948, 309, 1729, 309];
    match trail { 0 | 90 => &T1, 1 | 91 => &T2, 2 | 92 => &T3, 11 => &S1, 12 => &S2, 13 => &S3, 14 => &S4, 15 => &S5, 16 => &S6, 17 => &S7, 21 => &C1, 22 => &C2, 23 => &C3, _ => &[] }
}

/// GetRawTrailCoord: mission n (1-based) on the 3840-wide map, None past the trail.
fn raw(trail: i32, n: i32) -> Option<(f32, f32)> {
    let c = coords(trail);
    let k = (n - 1) as usize * 2;
    (n >= 1 && k + 1 < c.len()).then(|| (c[k] as f32, c[k + 1] as f32))
}

/// FrontendMenus.cs sword_ang: the sword sprite per done mission (+4 from mission 19 and 35 on trail 0).
const SWORD_ANG: [i32; 50] = [3, 0, 2, 0, 3, 2, 0, 2, 3, 1, 3, 0, 1, 2, 0, 1, 2, 3, 2, 1, 0, 1, 2, 0, 3, 2, 0, 1, 2, 1,
    3, 0, 1, 2, 0, 1, 2, 3, 2, 1, 0, 1, 2, 0, 3, 2, 0, 2, 1, 3];
/// trailChickens as UI-TrailAtlas icon numbers: the marker of a skipped (chicken) mission.
const CHICKENS: [i32; 50] = [212, 196, 214, 193, 214, 195, 197, 217, 215, 200, 218, 202, 213, 192, 226, 196, 190, 227, 199, 228,
    229, 193, 230, 231, 191, 201, 232, 219, 220, 209, 194, 221, 208, 223, 190, 203, 222, 210, 230, 204, 223, 205, 224, 225, 207,
    217, 203, 211, 212, 206];

/// The trail's text section key of mission n (1-based): TEXT_TRAIL_NAMES_CRU 1 / 51 / 81 + n, TEXT_SANDS_OF_TIME base + n.
fn mission_name(ui: &Ui, trail: i32, n: i32) -> String {
    let key = match trail {
        0 => format!("TEXT_TRAIL_NAMES_CRU_{:03}", n + 1),
        1 => format!("TEXT_TRAIL_NAMES_CRU_{:03}", n + 51),
        2 => format!("TEXT_TRAIL_NAMES_CRU_{:03}", n + 81),
        11..=17 => format!("TEXT_SANDS_OF_TIME_{:03}", [7, 12, 19, 28, 39, 49, 82][(trail - 11) as usize] + n),
        // CoopMissionChanged: TEXT_COOP m / m + 10 / m + 25, already numbered
        21..=23 => return ui.text_of(&format!("TEXT_COOP_{:03}", [1, 11, 26][(trail - 21) as usize] + n)),
        // a custom trail's missions are numbered with its name (HUD_LoadSaveRequester.ClearSavedName)
        90..=92 => return format!("{n}. {}", custom_trails::current().map(|c| c.name).unwrap_or_default()),
        _ => return String::new(),
    };
    // trail 2 numbers its missions 1.. like the others (UpdateTrailCampaignSelectedButton)
    format!("{n}. {}", ui.text_of(&key))
}

/// settings.cfg keys of a trail (ConfigSettings.SaveSettings): progress, best times, difficulty.
fn keys(trail: i32) -> (String, String, String) {
    match trail {
        0 => ("CampaignTrail".into(), "Trail1Times".into(), "Trail1Difficulty".into()),
        1 | 2 => (format!("CampaignTrail{}", trail + 1), format!("Trail{}Times", trail + 1), format!("Trail{}Difficulty", trail + 1)),
        _ => (format!("Sands{}Trail", trail - 10), format!("TrailS{}Times", trail - 10), format!("SandsTrail{}Difficulty", trail - 10)),
    }
}

/// One trail's progress like ConfigSettings: `next` = the first mission not won (1-based; n + 1 = trail done),
/// `times` = best time per mission (classic: game months, Sands: ticks; -1 not done, -1200 skipped by the chicken),
/// `difficulty` 0..4.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress { pub trail: i32, pub next: i32, pub times: Vec<i32>, pub difficulty: i32 }

impl Progress {
    pub fn read(cfg: &skirmish::Settings, trail: i32) -> Self {
        if COOPS.contains(&trail) {
            return coop_progress(trail, coop_ui().partner.as_deref());
        }
        if (90..=92).contains(&trail) {
            // custom_trails.cfg: 1 won, -1 chicken, 0 not done → the times' done / chicken / not done; the frontier is
            // the first mission still 0; difficulty always 1
            let n = count(trail).max(0) as usize;
            let st = custom_trails::current().map_or([0; 50], |c| custom_trails::statuses(&c.name));
            let times: Vec<i32> = st[..n.min(50)].iter().map(|&v| match v { 1 => 0, -1 => -1200, _ => -1 }).collect();
            let next = st[..n.min(50)].iter().position(|&v| v == 0).map_or(n as i32 + 1, |k| k as i32 + 1);
            return Progress { trail, next, times, difficulty: 1 };
        }
        let n = count(trail).max(0) as usize;
        let (kp, kt, kd) = keys(trail);
        let mut times = vec![-1; n];
        for (k, v) in cfg.get(&kt).unwrap_or("").split(':').filter_map(|v| v.trim().parse::<i32>().ok()).take(n).enumerate() {
            times[k] = v;
        }
        let next = cfg.get(&kp).and_then(|v| v.parse().ok()).unwrap_or(1);
        let difficulty = cfg.get(&kd).and_then(|v| v.parse().ok()).unwrap_or(1);
        Progress { trail, next, times, difficulty }
    }
    pub fn load(trail: i32) -> Self { Self::read(&skirmish::Settings::load(), trail) }
    /// Merged into the app's settings.cfg (the other keys stay). Co-op progress is kept per partner (`coop_won`).
    pub fn save(&self) -> std::io::Result<()> {
        if COOPS.contains(&self.trail) { return Ok(()); }
        if (90..=92).contains(&self.trail) {
            let Some(c) = custom_trails::current() else { return Ok(()) };
            let mut st = [0; 50];
            for (k, &t) in self.times.iter().take(50).enumerate() { st[k] = if t == -1200 { -1 } else if t >= 0 { 1 } else { 0 }; }
            return custom_trails::save_statuses(&c.name, &st);
        }
        let mut cfg = skirmish::Settings::load();
        let (kp, kt, kd) = keys(self.trail);
        cfg.set(&kp, self.next);
        cfg.set(&kt, self.times.iter().map(|t| format!("{t}:")).collect::<String>());
        cfg.set(&kd, self.difficulty);
        cfg.save()
    }
    pub fn len(&self) -> i32 { self.times.len() as i32 }
    /// countTrailMissionsCompleted: missions with a time >= 0.
    pub fn completed(&self) -> i32 { self.times.iter().filter(|&&t| t >= 0).count() as i32 }
    /// GameData.ManageGameOver's trail branch: mission `id` (0-based) won in `time`: the best time kept, the next
    /// mission unlocked when this was the frontier (all done after the last).
    pub fn won(&mut self, id: i32, time: i32) {
        let t = &mut self.times[id as usize];
        if *t <= 0 || *t > time { *t = time; }
        if id == self.len() - 1 { self.next = self.len() + 1 } else if id + 1 == self.next { self.next += 1 }
    }
    /// The chicken (ButtonTrailCampaignClicked -12): the frontier mission skipped. ponytail: like the original it marks
    /// times[next] (the mission after the frontier, index as written), bounded to the trail.
    pub fn chicken(&mut self) {
        if self.next > self.len() { return; }
        if let Some(t) = self.times.get_mut(self.next as usize) { *t = -1200; }
        self.next += 1;
    }
    /// The engine's start date of mission `id` (FUN_180006220 after DLL_ImportTrailTimes): the first date plus the
    /// earlier missions' times that are set (>= 0).
    pub fn engine_start_date(&self, id: i32) -> i32 {
        let base = if self.trail == 0 { 13200 } else { 13560 };
        base + self.times.iter().take(id.max(0) as usize).filter(|&&t| t >= 0).sum::<i32>()
    }
    /// ConfigSettings.getTrailStartDate (classic trails, months): mission `id`'s start date = Jan 1100 (Warchest and
    /// Extreme 1130) + the earlier missions' times (a chicken adds 100 years); `id` past the done ones = the frontier's.
    pub fn start_date(&self, id: i32) -> i32 {
        let mut total = if self.trail == 0 { 13200 } else { 13560 };
        let mut dates = vec![0; self.times.len() + 1];
        let mut i = 0;
        while i < self.times.len() {
            let t = self.times[i];
            dates[i] = total;
            if t == -1200 { total += 1200 } else if t < 0 { break } else { total += t }
            i += 1;
        }
        dates[i] = total;
        let mut k = id.clamp(0, self.times.len() as i32) as usize;
        loop {
            if dates[k] > 0 || k == 0 { return if dates[k] > 0 { dates[k] } else { total } }
            k -= 1;
        }
    }
}

/// The front end's start of trail mission `id` (0-based) at `difficulty` (EditorDirector.LoadSkirmishMap): the
/// profile's classic best times into the engine (DLL_ImportTrailTimes at boot), the lords' default AIVs, the
/// Bedouin-stockade option, the mission's map from the game's Maps folder. A sim panic comes back as an error.
#[cfg(feature = "live")]
pub fn start(trail: i32, id: i32, difficulty: i32) -> Result<shc_sim::mem::Mem, String> {
    use shc_sim::trail as tr;
    if COOPS.contains(&trail) {
        return start_coop_ally(trail, id);
    }
    if (90..=92).contains(&trail) {
        return custom_trails::start(id);
    }
    let cfg = skirmish::Settings::load();
    let mut m = shc_sim::mem::Mem::default();
    shc_sim::terrain::init_static(&mut m);
    let ms = tr::Mission::read(&m, trail, id).ok_or(format!("no mission {} in trail {trail}", id + 1))?;
    let path = skirmish::maps_dir().join(format!("{}.map", ms.map));
    let file = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let aivs = tr::default_aivs(shc_sim::data::aivs(), &ms);
    let bedouin = cfg.flag("Allow_Classic_Bedouin_Stockade", false);
    let colour = cfg.num("PlayerColour", 0.0) as i32;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        for c in CLASSIC {
            tr::import_times(&mut m, c, &Progress::read(&cfg, c).times);
        }
        tr::start(&mut m, trail, id, difficulty, bedouin, &aivs, &file).map(|_| { skirmish::trail_colours(&mut m, colour); m })
    })).map_err(|e| format!("not ported yet: {}", e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()))?
        .ok_or_else(|| format!("{} did not load", ms.map))
}

/// Single-player co-op (the co-op lobby with an AI ally, FRONT_Multiplayer.StartSkirmishGame): co-op mission `id` of
/// `trail` (21..23) as a skirmish with the lobby's ally lord and keep swap (`coop_ui`), seeded by the clock like the
/// original's DLL_StartMultiplayerGame.
#[cfg(feature = "live")]
fn start_coop_ally(trail: i32, id: i32) -> Result<shc_sim::mem::Mem, String> {
    use shc_sim::trail as tr;
    let (ally, swapped) = { let u = coop_ui(); (u.ally.ok_or("no AI ally chosen")?, u.swapped) };
    let c = tr::COOP.get((trail - 21) as usize).and_then(|t| t.get(id as usize)).ok_or(format!("no co-op mission {}", id + 1))?;
    let path = skirmish::maps_dir().join(format!("{}.map", c.map));
    let file = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() as i32);
    let s = tr::coop_setup_ally(shc_sim::data::aivs(), trail - 20, id, -1, ally, swapped, seed).ok_or("co-op setup")?;
    let colour = skirmish::Settings::load().num("PlayerColour", 0.0) as i32;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let mut m = shc_sim::mem::Mem::default();
        shc_sim::terrain::init_static(&mut m);
        // FRONT_Multiplayer.StartSkirmishGame: your Options colour, BuildMultiPlayerColourMapping for the members
        shc_sim::setup::new_skirmish(&mut m, &s, &file, colour.clamp(0, 7) + 1, Some(seed));
        skirmish::member_colours(&mut m, colour);
        m
    })).map_err(|e| format!("not ported yet: {}", e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()))
}

/// Trail mission `id` won with the HUD's date at the end: its time (classic: the game months since its start, as the
/// engine's FUN_180087730 takes it; Sands: ponytail: no tick count reaches the viewer, so the relaxed mode's
/// int.MaxValue = done without a rank) and the progress saved (GameData.ManageGameOver).
pub fn record_win(trail: i32, id: i32, month: i32, year: i32, ticks: i32) {
    if COOPS.contains(&trail) {
        let (partner, coa) = { let u = coop_ui(); (u.partner.clone().unwrap_or_default(), u.partner_coa.clone()) };
        if let Err(e) = coop_won(&partner, &coa, trail, id) { eprintln!("co-op progress: {e}"); }
        return;
    }
    let mut p = Progress::load(trail);
    if (90..=92).contains(&trail) {
        p.won(id, 0);
        if let Err(e) = p.save() { eprintln!("custom trail progress: {e}"); }
        return;
    }
    // classic: game months; Sands: the game time in ticks (lastGameState.game_time), relaxed mode int.MaxValue = done untimed
    let time = if trail <= 2 { year * 12 + month - p.engine_start_date(id) } else if relaxed() { i32::MAX } else { ticks.max(0) };
    p.won(id, time);
    if trail >= 11 && time != i32::MAX { board_add(trail, id, &player_name(), time / 40); } // UploadSandsScore (seconds)
    if let Err(e) = p.save() { eprintln!("trail progress: {e}"); }
}

/// HUD_MissionOver's continue for a trail mission: won → the next mission at once (the trail map after the last),
/// lost → the same mission again.
pub fn after(trail: i32, id: i32, difficulty: i32, won: bool) -> Go {
    if COOPS.contains(&trail) {
        return Go::ToMenu; // a co-op mission goes back to the co-op trail (the original: its lobby)
    }
    match (won, id + 1 < count(trail)) {
        (false, _) => Go::Trail(trail, id, difficulty),
        (true, true) => Go::Trail(trail, id + 1, difficulty),
        (true, false) => Go::ToMenu,
    }
}

/// The full-screen image of a trail screen (main.rs uploads it into `SCREEN`; re/scripts/export_trail.py).
pub fn screen_image(t: &T) -> Option<String> {
    Some(match t {
        T::Map(trail, _) => format!("trail-map-{}", if *trail >= 90 { *trail - 90 } else { *trail }),
        T::Brief(..) => return None, // over the game (brief_screen fills it)
        T::Board(trail, _) => format!("trail-map-{trail}"),
        T::Masters { .. } => "masters-bg".into(),
        T::Sands => "trail-sands-bg".into(),
        T::Coop => "trail-bg".into(),
        _ => "trail-bg".into(),
    })
}

/// FrontendMenus.ButtonClicked("Trail" ..): the map of `trail`, its frontier selected (CurrentSelectedTrail<n>Mission).
pub fn map(trail: i32) -> T {
    let p = Progress::load(trail);
    T::Map(trail, p.next.min(p.len()).max(1))
}

const WHITE: [f32; 4] = [1.0; 4];
const INK: [f32; 4] = [0.0, 0.0, 0.0, 1.0]; // text on the parchment panel
const GOLD: [f32; 4] = [0.957, 0.871, 0.667, 1.0]; // #F4DEAA, the light text on the dark plates

/// Animation clock (ms since the first trail screen) and when a mission was last picked (the knight's walk).
fn clock() -> i32 {
    static T0: OnceLock<Instant> = OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_millis() as i32
}
static PICKED: AtomicI32 = AtomicI32::new(i32::MIN / 2);

/// The co-op lobby of mission `n` (1-based) of co-op trail `tr` (21..23) with the session's humans (team 1), their
/// keeps `swapped` or not:
/// `lan::game` builds it with shc_sim::trail::coop_setup (fixed map, AI lords, keeps). `map` = the mission's map file
/// (what the LAN screen lists).
fn coop_lobby(net: &crate::lan::Net, tr: i32, n: i32, swapped: bool) -> crate::lan::Lobby {
    #[cfg(feature = "live")]
    let map = format!("{}.map", shc_sim::trail::COOP[(tr - 21) as usize][(n - 1) as usize].map);
    #[cfg(not(feature = "live"))]
    let map = String::new();
    let humans = net.lobby.humans.iter().map(|h| crate::lan::Human { team: 1, ..h.clone() }).collect();
    crate::lan::Lobby { map, humans, ais: vec![], coop: Some((tr - 20, n - 1, swapped)), ..net.lobby.clone() }
}

/// Options > Player Settings ("Name"), else the Mac user (as the LAN screen names you).
fn player_name() -> String { super::coa::player_name() }

/// The co-op lobby's own choices (FRONT_CoopTrail; one menu at a time): the AI ally (LORDS index) when you play
/// without a human (singlePlayerCoop), its picker open, the humans' keeps swapped (coopOrderSwapped), the chat line
/// being typed, and the partner the progress belongs to (the other human's name, or "AI <lord>") and their coat of arms.
struct CoopUi { ally: Option<i32>, picking: bool, swapped: bool, typing: bool, line: String, partner: Option<String>, partner_coa: String }
static COOP_UI: std::sync::Mutex<CoopUi> = std::sync::Mutex::new(CoopUi { ally: None, picking: false, swapped: false, typing: false, line: String::new(), partner: None, partner_coa: String::new() });

/// The LAN partner (the human who isn't `me`): their name and coat of arms into COOP_UI.
fn set_partner(humans: &[crate::lan::Human], me: i32) {
    let other = humans.iter().enumerate().find(|(k, _)| *k as i32 + 1 != me).map(|(_, h)| h);
    let mut u = coop_ui();
    (u.partner, u.partner_coa) = (other.map(|h| h.name.clone()), other.map_or(String::new(), |h| h.coa.clone()));
}

fn coop_ui() -> std::sync::MutexGuard<'static, CoopUi> {
    COOP_UI.lock().unwrap_or_else(|e| e.into_inner())
}

/// coop.cfg like ConfigSettings.SaveCoop: one line per partner, `name:t1:t2:t3:coa` with 10 statuses each (1 = won
/// together) and their coat of arms (AvatarDesign string, empty for an AI ally). ponytail: partners by name (LAN), not
/// Steam id; no hidden flag.
fn coop_file() -> PathBuf { skirmish::profile_dir().join("coop.cfg") }

fn coop_lines() -> Vec<(String, [[i32; 10]; 3], String)> {
    std::fs::read_to_string(coop_file()).unwrap_or_default().lines().filter_map(|l| {
        let mut f = l.split(':');
        let name = f.next()?.to_string();
        let mut st = [[0; 10]; 3];
        for t in &mut st {
            for (k, v) in f.next()?.split(',').take(10).enumerate() { t[k] = v.trim().parse().unwrap_or(0); }
        }
        Some((name, st, f.next().unwrap_or("").to_string()))
    }).collect()
}

/// ConfigSettings.CoopCompleted: co-op mission `id` of `trail` (21..23) won with `partner` (coat of arms `coa`, kept
/// when empty).
fn coop_won(partner: &str, coa: &str, trail: i32, id: i32) -> std::io::Result<()> {
    let name = partner.replace([':', '\n'], "_");
    let mut lines = coop_lines();
    let i = lines.iter().position(|l| l.0 == name).unwrap_or_else(|| { lines.push((name.clone(), [[0; 10]; 3], String::new())); lines.len() - 1 });
    lines[i].1[(trail - 21) as usize][id as usize] = 1;
    if super::coa::Design::parse(coa).is_some() { lines[i].2 = coa.to_string(); }
    std::fs::create_dir_all(skirmish::profile_dir())?;
    let text: String = lines.iter().map(|(n, st, coa)| {
        let t: Vec<String> = st.iter().map(|t| t.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")).collect();
        format!("{n}:{}:{coa}\n", t.join(":"))
    }).collect();
    std::fs::write(coop_file(), text)
}

/// ConfigSettings.CalcCoopProgress: the unlocked frontier = the highest mission won with anyone + 1; the swords show
/// the missions won with `partner` (None: with anyone), the others before the frontier a chicken.
fn coop_progress(trail: i32, partner: Option<&str>) -> Progress {
    let lines = coop_lines();
    let t = (trail - 21) as usize;
    let any: Vec<bool> = (0..10).map(|k| lines.iter().any(|l| l.1[t][k] > 0)).collect();
    let with: Vec<bool> = (0..10).map(|k| match partner { None => any[k], Some(p) => lines.iter().any(|l| l.0 == p && l.1[t][k] > 0) }).collect();
    let next = any.iter().rposition(|&d| d).map_or(1, |k| k as i32 + 2);
    Progress { trail, next, times: with.iter().map(|&d| if d { 0 } else { -1 }).collect(), difficulty: 1 }
}

impl Menu {
    /// The trail map of `trail` (front end back from a trail mission).
    pub fn trail_map(trail: i32) -> Self { Menu { screen: Screen::Trail(map(trail)), ..Self::main() } }

    /// ButtonGotoBriefing("FromStory") after a trail start: the briefing over the paused mission, its map's radar
    /// preview for main.rs to upload. None for the co-op trails (a multiplayer start).
    #[cfg(feature = "live")]
    pub fn trail_brief(trail: i32, id: i32, difficulty: i32) -> Option<Self> {
        if COOPS.contains(&trail) { return None; }
        let b = brief(trail, id)?;
        Some(Menu { screen: Screen::Trail(T::Brief(trail, id, difficulty)), in_game: true, preview: skirmish::map_preview(&b.path), ..Self::main() })
    }

    /// Back (Escape / the Back button): map → its menu, the menus → the Skirmish menu → the main menu.
    pub(super) fn trail_back(&mut self, t: T) -> Option<Go> {
        // Escape first closes the chat line / ally picker (main.rs sends it here, not to trail_key)
        {
            let mut u = coop_ui();
            if u.typing || u.picking {
                (u.typing, u.picking) = (false, false);
                u.line.clear();
                return None;
            }
        }
        let up = match t {
            T::Skirmish => { *self = Self::main(); return None }
            T::Brief(..) => return None, // Escape doesn't leave the briefing (Quit / Start do)
            T::Board(tr, n) => T::Map(tr, n),
            T::Masters { tr, .. } => map(tr),
            T::Trails { options: true } => T::Trails { options: false },
            T::Map(tr, _) if tr >= 90 => T::Custom,
            T::Custom => T::Skirmish,
            T::Map(tr, _) if COOPS.contains(&tr) => {
                // leaving the co-op lobby ends the session and its choices
                self.mp = Default::default();
                *coop_ui() = CoopUi { ally: None, picking: false, swapped: false, typing: false, line: String::new(), partner: None, partner_coa: String::new() };
                T::Coop
            }
            T::Map(tr, _) if tr >= 11 => T::Sands,
            T::Map(..) => T::Trails { options: false },
            _ => T::Skirmish,
        };
        self.screen = Screen::Trail(up);
        None
    }

    pub(super) fn trail_click(&mut self, a: A) -> Option<Go> {
        match a {
            A::Open(T::Map(tr, _)) => { self.screen = Screen::Trail(map(tr)); return None }
            A::Open(to) => { self.screen = Screen::Trail(to); return None }
            _ => {}
        }
        let Screen::Trail(t) = self.screen else { return None };
        match (t, a) {
            // ButtonBriefingResume: main.rs's Go::Brief unpauses, HideObjectiveProgress(0, 0), ResetTime
            (T::Brief(_, _, d), A::Start) => return Some(Go::Brief(d)),
            // Quit Mission: back to the trail map (main.rs Go::ToMenu with the trail set)
            (T::Brief(..), A::Quit) => return Some(Go::ToMenu),
            (T::Custom, A::CustomPick(k)) => {
                if let Some(c) = custom_trails::list().into_iter().nth(k as usize) {
                    let tr = custom_trails::map_id(c.missions.len());
                    custom_trails::select(c);
                    self.screen = Screen::Trail(map(tr));
                }
            }
            (T::Custom, A::CustomPage(d)) => self.scroll = (self.scroll as i32 + 10 * d).max(0) as usize,
            (T::Map(tr, n), A::Board) => self.screen = Screen::Trail(T::Board(tr, n)), // -13: HUD_Leaderboard
            #[cfg(feature = "live")]
            (T::Map(tr, n), A::Customize) if tr <= 2 => self.customize(tr, n - 1),
            (T::Board(tr, n), A::Quit) => self.screen = Screen::Trail(T::Map(tr, n)),
            // Skirmish Masters (-11): sorted by score, trail and other games shown (FRONT_SkirmishMasters.Open)
            (T::Map(tr, _), A::Masters) => { self.screen = Screen::Trail(T::Masters { tr, sort: 0, flags: 3, del: -1 }); self.scroll = 0; }
            (T::Masters { tr, flags, del, .. }, A::MSort(sort)) => self.screen = Screen::Trail(T::Masters { tr, sort, flags, del }),
            (T::Masters { tr, sort, flags, del }, A::MFlag(b)) => {
                // turning off the one of trails / others that is still on turns the other back on
                let mut f = flags ^ b;
                if f & 3 == 0 { f |= 3 & !b; }
                self.screen = Screen::Trail(T::Masters { tr, sort, flags: f, del });
                self.scroll = 0;
            }
            (T::Masters { tr, sort, flags, .. }, A::MDel(i)) => self.screen = Screen::Trail(T::Masters { tr, sort, flags, del: i as i64 }),
            (T::Masters { tr, sort, flags, del }, A::MDelYes | A::MDelNo) => {
                if a == A::MDelYes { if let Err(e) = masters::delete(del) { self.error = Some(e.to_string()) } }
                self.screen = Screen::Trail(T::Masters { tr, sort, flags, del: -1 });
            }
            (T::Map(tr, _), A::Pick(_)) if COOPS.contains(&tr) && self.mp.net.as_ref().is_some_and(|n| !n.is_host()) => {} // the co-op host picks
            (T::Map(tr, _), A::Pick(n)) => { self.screen = Screen::Trail(T::Map(tr, n)); PICKED.store(clock(), Relaxed); }
            // the co-op lobby (FRONT_CoopTrail): you host here, your partner joins from Multiplayer → LAN
            (T::Map(tr, n), A::Host) if COOPS.contains(&tr) && self.mp.net.is_none() => match crate::lan::Net::host(&player_name(), 0) {
                Ok(net) => {
                    self.mp = Default::default();
                    let l = coop_lobby(&net, tr, n, coop_ui().swapped);
                    self.mp.net = Some(net);
                    self.mp.net.as_mut().unwrap().set_lobby(l);
                }
                Err(e) => self.error = Some(format!("Can't host: {e}")),
            },
            (T::Map(tr, _), A::AllyOpen) if COOPS.contains(&tr) && self.mp.net.is_none() => coop_ui().picking = true,
            (T::Map(tr, _), A::Ally(l)) if COOPS.contains(&tr) => {
                let mut u = coop_ui();
                (u.ally, u.picking, u.partner, u.partner_coa) = (Some(l), false, Some(format!("AI {}", skirmish::LORDS[l as usize])), String::new());
            }
            (T::Map(tr, _), A::Swap) if COOPS.contains(&tr) => { let mut u = coop_ui(); u.swapped = !u.swapped; }
            (T::Map(tr, _), A::Type) if COOPS.contains(&tr) => coop_ui().typing = true,
            // single-player co-op: the AI ally plays slot 2 (the third value of Go::Trail is unused for co-op)
            (T::Map(tr, n), A::Start) if COOPS.contains(&tr) && self.mp.net.is_none() && coop_ui().ally.is_some() => return Some(Go::Trail(tr, n - 1, 1)),
            (T::Map(tr, n), A::Start) if COOPS.contains(&tr) => {
                let Some(net) = self.mp.net.as_mut().filter(|n| n.is_host() && n.lobby.humans.len() >= 2) else { return None };
                set_partner(&net.lobby.humans, net.me);
                let l = coop_lobby(net, tr, n, coop_ui().swapped);
                net.set_lobby(l);
                let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() as i32);
                let (lobby, seed, delay) = net.start(seed, crate::lan::DELAY);
                let net = self.mp.net.take().unwrap();
                return Some(Go::Lan(Box::new(super::mp_menu::Started { net, lobby, seed, delay })));
            }
            (T::Map(tr, n), A::Start) => {
                let p = Progress::load(tr);
                // StartTrailMission: the trail's difficulty; the Sands trails are played at 1 unless relaxed (no timing)
                let d = if tr >= 11 && (tr >= 90 || !skirmish::Settings::load().flag("HideSoTTiming", false)) { 1 } else { p.difficulty };
                return Some(Go::Trail(tr, n - 1, d));
            }
            (T::Map(tr, _), A::Difficulty) => {
                let mut p = Progress::load(tr);
                p.difficulty = if p.difficulty >= 4 { 0 } else { p.difficulty + 1 };
                if let Err(e) = p.save() { self.error = Some(e.to_string()) }
            }
            // a custom trail (ConfigSettings.CustomTrailCheat): the frontier itself, its first mission still 0, is skipped
            (T::Map(tr, _), A::Chicken) if tr >= 90 => {
                if let Some(c) = custom_trails::current() {
                    let mut st = custom_trails::statuses(&c.name);
                    if let Some(k) = st.iter().take(c.missions.len()).position(|&v| v == 0) {
                        st[k] = -1;
                        if let Err(e) = custom_trails::save_statuses(&c.name, &st) { self.error = Some(e.to_string()) }
                        crate::audio::ui(258, 1.0);
                    }
                }
                self.screen = Screen::Trail(map(tr));
            }
            (T::Map(tr, _), A::Chicken) => {
                let mut p = Progress::load(tr);
                if p.next <= p.len() {
                    p.chicken();
                    if let Err(e) = p.save() { self.error = Some(e.to_string()) }
                    crate::audio::ui(258, 1.0); // the chicken's cluck (SFXManager.playUISound(258))
                }
                self.screen = Screen::Trail(map(tr));
            }
            (T::Trails { .. }, A::Bedouin) => {
                let mut cfg = skirmish::Settings::load();
                let on = cfg.flag("Allow_Classic_Bedouin_Stockade", false);
                cfg.set("Allow_Classic_Bedouin_Stockade", if on { "False" } else { "True" });
                if let Err(e) = cfg.save() { self.error = Some(e.to_string()) }
            }
            _ => {}
        }
        None
    }

    /// The co-op lobby's chat line takes the keys (menu.rs `capturing` / `key`).
    pub(super) fn trail_typing(&self) -> bool {
        matches!(self.screen, Screen::Trail(T::Map(21..=23, _))) && self.mp.net.is_some() && coop_ui().typing
    }

    /// A key (keys.rs Unity name, ",shift" etc. appended) into the chat line; Return sends it, Escape stops typing.
    pub(super) fn trail_key(&mut self, k: &str) {
        let (code, shift) = (k.split(',').next().unwrap_or(""), k.contains("shift"));
        let mut u = coop_ui();
        let c = match code {
            "Backspace" | "Delete" => { u.line.pop(); return }
            "Escape" => { u.typing = false; return }
            "Return" | "KeypadEnter" => {
                let line = std::mem::take(&mut u.line);
                u.typing = false;
                if let (Some(net), false) = (self.mp.net.as_mut(), line.trim().is_empty()) { net.say(line.trim()); }
                return;
            }
            "Space" => ' ',
            "Period" => if shift { ':' } else { '.' },
            "Comma" => if shift { ';' } else { ',' },
            "Minus" => '-',
            "Quote" => '\'',
            "Slash" => if shift { '?' } else { '/' },
            _ => match code.strip_prefix("Alpha").and_then(|d| d.parse::<u8>().ok()) {
                Some(1) if shift => '!',
                Some(d) if d < 10 => (b'0' + d) as char,
                _ if code.len() == 1 && code.as_bytes()[0].is_ascii_alphabetic() => {
                    let c = code.chars().next().unwrap();
                    if shift { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }
                }
                _ => return,
            },
        };
        if u.line.chars().count() < 80 { u.line.push(c); }
    }

    /// Every frame (mp_menu's poll): the co-op lobby's session. Host: the lobby follows the selected mission, a lost
    /// connection ends it; client: the host's mission, its Start (→ `Go::Lan`, the game `lan::game` builds).
    pub(super) fn trail_poll(&mut self) -> Option<Go> {
        let Screen::Trail(T::Map(tr, n)) = self.screen else { return None };
        let net = self.mp.net.as_mut()?;
        net.poll();
        if let Some(e) = net.error.clone() {
            self.mp = Default::default();
            self.error = Some(e);
            return None;
        }
        if net.is_host() {
            let l = coop_lobby(net, tr, n, coop_ui().swapped);
            if net.lobby != l { net.set_lobby(l); }
            return None;
        }
        if let Some((lobby, seed, delay)) = net.started.clone() {
            set_partner(&lobby.humans, net.me);
            let net = self.mp.net.take().unwrap();
            return Some(Go::Lan(Box::new(super::mp_menu::Started { net, lobby, seed, delay })));
        }
        if let Some((t, k, sw)) = net.lobby.coop {
            coop_ui().swapped = sw;
            self.screen = Screen::Trail(T::Map(20 + t, k + 1));
        }
        None
    }

    /// Draw trail screen `t` (1920×1080 design grid): the screen image, then the screen.
    pub(super) fn trail_draw(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, t: T, w: f32, h: f32, cursor: [f32; 2]) {
        if let T::Brief(tr, id, _) = t { return self.brief_screen(ui, out, tr, id, w, h, cursor); }
        let k = (w / 1920.0).max(h / 1080.0);
        Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, 1.0]);
        // Image Stretch=UniformToFill (the maps: the mapgrid's 1920×1080 in the Viewbox, so the map scales with the grid)
        let full = if matches!(t, T::Map(..) | T::Board(..)) { self.r(0.0, 0.0, 1920.0, 1080.0) } else { [(w - 1920.0 * k) / 2.0, (h - 1080.0 * k) / 2.0, (w + 1920.0 * k) / 2.0, (h + 1080.0 * k) / 2.0] };
        quad(out, SCREEN, full, SCREEN_UV, [1.0; 4]);
        match t {
            T::Skirmish => self.skirmish_menu(ui, out, cursor),
            T::Trails { options } => self.trails_menu(ui, out, options, cursor),
            T::Sands => self.sands_menu(ui, out, cursor),
            T::Coop => self.coop_menu(ui, out, cursor),
            T::Map(tr, sel) => self.trail_map_screen(ui, out, tr, sel, cursor),
            T::Brief(..) => {}
            T::Board(tr, sel) => self.board_screen(ui, out, tr, sel, cursor),
            T::Masters { tr, sort, flags, del } => self.masters_screen(ui, out, tr, sort, flags, del, w, h, cursor),
            T::Custom => self.custom_list(ui, out, cursor),
        }
    }

    /// BTN_NewFrontend_LargeImage (720×440 template in a Viewbox `w` wide at design x, y): the mode picture (500×338)
    /// top centre, the text bar (btn_text_bar_01, hover _02; yellow = completed) at the bottom, the label 38 px small
    /// caps. `a` None = not ported (no click). Returns the hover.
    fn large_button(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, label: &str, picture: &str, x: f32, y: f32, w: f32, a: Option<Act>, yellow: bool, cursor: [f32; 2]) -> bool {
        let k = w / 720.0;
        let p = self.r(x, y, w, 440.0 * k);
        let hot = Self::inside(p, cursor);
        if hot && a.is_some() { crate::audio::hover(p, crate::audio::ui_id::SHIELD, 1.0); }
        let at = self.r(x + 110.0 * k, y, 500.0 * k, 338.0 * k);
        ui.img(out, picture, at, [1.0; 4]); // UI-MasterAtlas, or (Trail_Coop3 / 4) UI-TrailAtlas in the art sheet
        pic(out, picture, at, 1.0);
        let bar = match (hot, yellow) { (false, false) => "btn_text_bar_01", (true, false) => "btn_text_bar_02", (false, true) => "btn_text_bar_01_yellow", (true, true) => "btn_text_bar_02_yellow" };
        pic(out, bar, self.r(x, y + (440.0 - 125.5) * k, w, 125.5 * k), 1.0);
        self.small_caps(ui, out, label, x + w / 2.0, y + (440.0 - 18.0 - 40.0 + 38.0 * 0.33) * k, 38.0 * k, [1.0, 1.0, 1.0, if hot { 0.9 } else { 0.7 }]);
        if let Some(a) = a { self.hits.push((p, a)); }
        hot
    }

    /// Show_Frontend_Skirmish: Classic Trails, Co-op, Sands of Time over Custom Trails and Custom (the skirmish lobby).
    fn skirmish_menu(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let title = ui.text_of("TEXT_NEW_CTEXT_198");
        self.heading(ui, out, 602.5, &title);
        let tiles: [(&str, &str, Option<Act>, f32, f32); 5] = [
            ("TEXT_NEW_CTEXT_202", "Trail_Classic", Some(Act::Trail(A::Open(T::Trails { options: false }))), 33.0, 162.3),
            ("TEXT_COOP_022", "Trail_Coop", Some(Act::Trail(A::Open(T::Coop))), 651.0, 162.3),
            ("TEXT_SANDS_OF_TIME_071", "Trail_Sands Of Time", Some(Act::Trail(A::Open(T::Sands))), 1269.0, 162.3),
            ("TEXT_CUSTOMISATION_053", "Trail_Original", Some(Act::Trail(A::Open(T::Custom))), 267.0, 537.0), // made with the original's Trail Maker
            ("TEXT_NEW_CTEXT_203", "Trail_Custom", Some(Act::Skirmish), 1035.0, 537.0),
        ];
        let mut help = None;
        for (label, picture, a, x, y) in tiles {
            let t = ui.text_of(label);
            if self.large_button(ui, out, &t, picture, x, y, 618.0, a, false, cursor) && a.is_none() { help = Some((x, y)); }
        }
        if let Some((x, y)) = help { self.help_panel(ui, out, "", "", true, x + 59.0, y + 120.0, 500.0); }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::Back), cursor);
        self.img_button(ui, out, ["Load Shield Up", "Load Shield Over", "Load Shield Up"], 1767.0, 957.0, 103.0, 103.0, Some(Act::LoadList), cursor);
    }

    /// Show_Frontend_Skirmish_Trails: Original / Warchest / Extreme with "n / 50 completed" (yellow bar when done);
    /// the Classic Trails options popup (allow the Bedouin stockade).
    fn trails_menu(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, options: bool, cursor: [f32; 2]) {
        let title = ui.text_of("TEXT_NEW_CTEXT_202");
        self.heading(ui, out, 602.5, &title);
        let cfg = skirmish::Settings::load();
        let done_text = ui.text_of("TEXT_NEW_TEXT2_190");
        for (i, (label, picture)) in [("TEXT_SKIRMISH_CHOOSE2_007", "Trail_Original"), ("TEXT_SKIRMISH_CHOOSE2_022", "Trail_Warchest"), ("TEXT_SKIRMISH_CHOOSE2_024", "Trail_Extreme")].into_iter().enumerate() {
            let p = Progress::read(&cfg, CLASSIC[i]);
            let x = 13.0 + 10.0 + 618.0 * i as f32 + 10.0 * (i as f32 + 1.0);
            let t = ui.text_of(label);
            let a = (!options).then_some(Act::Trail(A::Open(T::Map(CLASSIC[i], 1))));
            self.large_button(ui, out, &t, picture, x, 322.0, 618.0, a, p.completed() == p.len(), cursor);
            // completed box: #88000000, 400×50, 380 below the button grid's top
            Ui::fill(out, self.r(x + 109.0, 705.0, 400.0, 50.0), [0.0, 0.0, 0.0, 0.53]);
            self.label(ui, out, &format!("{} / {} {done_text}", p.completed(), p.len()), x + 309.0, 730.0, 24.0, 0.5, WHITE);
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::Trail(A::Open(T::Skirmish))), cursor);
        self.img_button(ui, out, ["Settings Up", "Settings Over", "Settings Up"], 1617.0, 957.0, 103.0, 103.0, Some(Act::Trail(A::Open(T::Trails { options: true }))), cursor);
        self.img_button(ui, out, ["Load Shield Up", "Load Shield Over", "Load Shield Up"], 1767.0, 957.0, 103.0, 103.0, Some(Act::LoadList), cursor);
        if options {
            // the options popup (750×300 centred): heading, the Bedouin toggle (BTN_SH_GlowS + tick), Close
            let (x, y) = (585.0, 390.0);
            self.frame(out, x, y, 750.0, 300.0, true);
            self.label(ui, out, &ui.text_of("TEXT_XPLAY_WAITING_ROOM_075"), x + 375.0, y + 50.0, 28.0, 0.5, CREAM);
            let on = cfg.flag("Allow_Classic_Bedouin_Stockade", false);
            let tick = if on { "UI-Buttons S100" } else { "UI-Buttons S043" };
            let label = ui.text_of("TEXT_NEW_CTEXT_371");
            self.glow(ui, out, &label, x + 175.0, y + 130.0, 400.0, Some(tick), Some(Act::Trail(A::Bedouin)), cursor);
            let close = ui.text_of("TEXT_NEW_TEXT_091");
            self.glow(ui, out, &close, x + 580.0, y + 250.0, 150.0, None, Some(Act::Trail(A::Open(T::Trails { options: false }))), cursor);
        }
    }

    /// Show_Frontend_Skirmish_Sands: the seven Sands of Time trails (5-7 are DLC in the original; here all open).
    /// ponytail: the original's Viewbox rows (3 + 4 tiles) at 460 px wide; ranks and the leaderboard not shown.
    fn sands_menu(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let title = ui.text_of("TEXT_SANDS_OF_TIME_001");
        self.heading(ui, out, 602.5, &title);
        let cfg = skirmish::Settings::load();
        let done_text = ui.text_of("TEXT_NEW_TEXT2_190");
        for (i, &tr) in SANDS.iter().enumerate() {
            let (row, col, cols) = if i < 3 { (0, i, 3) } else { (1, i - 3, 4) };
            let w = 440.0;
            let x = (1920.0 - cols as f32 * 460.0) / 2.0 + col as f32 * 460.0 + 10.0;
            let y = 190.0 + row as f32 * 360.0;
            let p = Progress::read(&cfg, tr);
            // the trail's title (TrailTitle: TEXT_SANDS_OF_TIME 1..6, the 7th 102)
            let label = ui.text_of(&format!("TEXT_SANDS_OF_TIME_{:03}", if i == 6 { 103 } else { 2 + i }));
            self.large_button(ui, out, &label, &format!("Trail_Sands{}", i + 1), x, y, w, Some(Act::Trail(A::Open(T::Map(tr, 1)))), p.completed() == p.len(), cursor);
            Ui::fill(out, self.r(x + w / 2.0 - 140.0, y + 274.0, 280.0, 36.0), [0.0, 0.0, 0.0, 0.53]);
            let done = p.completed() == p.len();
            let line = if done { ui.text_of("TEXT_SANDS_OF_TIME_061") } else { format!("{} / {} {done_text}", p.completed(), p.len()) };
            self.label(ui, out, &line, x + w / 2.0, y + 292.0, 20.0, 0.5, WHITE);
            // the trail's rank (types 11..16 only, like the original's loop): total best vs total target, rank_6 when a
            // mission was skipped or untimed
            if done && i < 6 && !relaxed() {
                let r = sands_total(&p).map_or("rank_6".to_string(), |(t, target)| format!("rank_{}", sands_rank(t, target) + 1));
                ui.img_fit(out, &r, self.r(x + w - 110.0, y + 10.0, 100.0, 90.0), WHITE);
            }
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::Trail(A::Open(T::Skirmish))), cursor);
    }

    /// FRONT_CoopTrail's lobby pane over the co-op map (no difficulty, no chicken): without a partner "Host co-op game"
    /// (your partner joins from Multiplayer → LAN) or "Play with an AI ally" (the lord picker); then the two partners,
    /// Swap keeps, the chat (LAN), Start for the host once both are there.
    fn coop_pane(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, back: Act, cursor: [f32; 2]) {
        let (ally, picking, swapped, typing, line) = { let u = coop_ui(); (u.ally, u.picking, u.swapped, u.typing, u.line.clone()) };
        self.frame(out, 1480.0, 90.0, 420.0, 470.0, true);
        // (name, face): a coat of arms (AvatarDesign string) or an AI ally's lord
        let mut rows: Vec<(String, Result<String, i32>)> = vec![];
        let (mut can_start, mut host) = (false, true);
        let status = match (&self.mp.net, ally) {
            (Some(net), _) => {
                rows = net.lobby.humans.iter().enumerate().take(2).map(|(k, h)| (format!("{}{}", h.name, if k as i32 + 1 == net.me { " (you)" } else { "" }), Ok(h.coa.clone()))).collect();
                host = net.is_host();
                can_start = host && net.lobby.humans.len() >= 2;
                match (host, can_start) { (true, false) => "Waiting for your partner (Multiplayer → join)", (true, true) => "Ready: Start the mission", _ => "Waiting for the host to start" }
            }
            (None, Some(l)) => {
                rows = vec![(format!("{} (you)", player_name()), Ok(super::coa::mine().code())), (format!("{} (AI ally)", skirmish::LORDS[l as usize]), Err(l))];
                can_start = true;
                "Ready: Start the mission"
            }
            (None, None) => "Play with a partner or an AI ally",
        };
        // the humans trade keeps when swapped (keep order slots 1 ↔ 2)
        for (k, (r, face)) in rows.iter().enumerate() {
            let slot = if swapped { k ^ 1 } else { k };
            let y = 140.0 + 40.0 * slot as f32;
            self.label(ui, out, &format!("{}.", slot + 1), 1500.0, y, 24.0, 0.0, INK);
            let p = self.r(1528.0, y - 18.0, 36.0, 36.0);
            match face {
                Ok(coa) => { if !super::coa::draw_face(out, coa, p, WHITE) { ui.img_fit(out, "shield_crusader", p, WHITE); } ui.img(out, "Face Border", p, WHITE); }
                Err(l) => if let Some(&(f, _)) = FACES.iter().find(|f| f.1 as i32 == *l) { ui.img(out, "Face Background Generic", p, WHITE); ui.img(out, f, p, WHITE); },
            }
            self.label(ui, out, r, 1572.0, y, 24.0, 0.0, INK);
        }
        self.label(ui, out, status, 1690.0, 230.0, 18.0, 0.5, INK);
        if self.mp.net.is_none() && ally.is_none() {
            self.glow(ui, out, "Host co-op game", 1540.0, 270.0, 300.0, None, Some(Act::Trail(A::Host)), cursor);
            self.glow(ui, out, "Play with an AI ally", 1540.0, 315.0, 300.0, None, Some(Act::Trail(A::AllyOpen)), cursor);
        }
        if rows.len() == 2 && host {
            self.glow(ui, out, "Swap keeps", 1540.0, 270.0, 300.0, None, Some(Act::Trail(A::Swap)), cursor);
        }
        if let Some(net) = &self.mp.net {
            // chat: the last lines, then the line being typed (click to type, Return sends)
            let names: Vec<(i32, String)> = net.lobby.humans.iter().enumerate().map(|(k, h)| (k as i32 + 1, h.name.clone())).collect();
            let lines: Vec<String> = net.chat.iter().rev().take(4).rev().map(|(p, t)| {
                format!("{}: {t}", names.iter().find(|n| n.0 == *p).map_or("?", |n| n.1.as_str()))
            }).collect();
            for (k, l) in lines.iter().enumerate() { self.label(ui, out, l, 1500.0, 330.0 + 26.0 * k as f32, 16.0, 0.0, INK); }
            let blink = clock() / 500 % 2 == 0;
            let field = if typing { format!("> {line}{}", if blink { "|" } else { "" }) } else { "> click to chat".to_string() };
            let p = self.r(1495.0, 455.0, 390.0, 30.0);
            Ui::fill(out, p, [0.0, 0.0, 0.0, 0.15]);
            self.label(ui, out, &field, 1500.0, 470.0, 16.0, 0.0, INK);
            self.hits.push((p, Act::Trail(A::Type)));
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(back), cursor);
        let start = can_start.then_some(Act::Trail(A::Start));
        self.img_button(ui, out, ["UI-Buttons B001", "UI-Buttons B002", "UI-Buttons B004"], 1709.0, 984.0, 151.0, 66.0, start, cursor);
        if picking {
            // the AI ally picker (FRONT_Multiplayer's AI face grid for the co-op ally): every lord, three columns
            self.frame(out, 460.0, 140.0, 1000.0, 800.0, true);
            self.label(ui, out, "Choose your AI ally", 960.0, 185.0, 32.0, 0.5, INK);
            for (i, name) in skirmish::LORDS.iter().enumerate() {
                let (x, y) = (500.0 + 310.0 * (i % 3) as f32, 230.0 + 76.0 * (i / 3) as f32);
                self.glow(ui, out, name, x, y, 290.0, None, Some(Act::Trail(A::Ally(i as i32))), cursor);
            }
        }
    }

    /// Show_Frontend_Coop: the co-op trails in two rows (the fourth is an unreleased shell in the original: drawn,
    /// no click).
    fn coop_menu(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let title = ui.text_of("TEXT_COOP_022");
        self.heading(ui, out, 602.5, &title);
        let cfg = skirmish::Settings::load();
        let done_text = ui.text_of("TEXT_NEW_TEXT2_190");
        for k in 0..4 {
            let (x, y) = ([267.0, 1035.0][k % 2], [162.3, 537.0][k / 2]);
            let label = ui.text_of(&format!("TEXT_COOP_{:03}", 23 + k));
            let a = (k < 3).then(|| Act::Trail(A::Open(T::Map(COOPS[k], 1))));
            let done = k < 3 && { let p = Progress::read(&cfg, COOPS[k]); p.completed() == p.len() };
            self.large_button(ui, out, &label, &format!("Trail_Coop{}", k + 1), x, y, 618.0, a, done, cursor);
            if k < 3 {
                let p = Progress::read(&cfg, COOPS[k]);
                Ui::fill(out, self.r(x + 109.0, y + 380.0, 400.0, 40.0), [0.0, 0.0, 0.0, 0.53]);
                self.label(ui, out, &format!("{} / {} {done_text}", p.completed(), p.len()), x + 309.0, y + 400.0, 22.0, 0.5, WHITE);
            }
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::Trail(A::Open(T::Skirmish))), cursor);
    }

    /// FRONT_Trail.xaml: the map, a sword per won mission (chicken marker when skipped), the flag on the frontier,
    /// the knight on the selection, mission / trail texts, difficulty, the chicken, Back / Load / Start.
    fn trail_map_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, tr: i32, sel: i32, cursor: [f32; 2]) {
        let p = Progress::load(tr);
        let n = p.len();
        let front = p.next.min(n);
        let ms = clock();
        // the swords and the knight, back to front by their y (Panel.ZIndex = y)
        let mut sprites: Vec<(f32, String, [f32; 4])> = vec![];
        let mut num4 = 0;
        for i in 0..(p.next - 1).min(n) {
            let Some((rx, ry)) = raw(tr, i + 1) else { continue };
            if tr == 0 && (i == 18 || i == 34) { num4 += 4; }
            if p.times[i as usize] < 0 {
                sprites.push((ry / 2.0 + 65.0, format!("skirmish_trail_icons-{}", CHICKENS[i as usize]), [rx / 2.0 - 55.0, ry / 2.0 - 67.0, 100.0, 100.0]));
            } else {
                sprites.push((ry / 2.0 + 50.0, format!("skirmish_trail_icons-{}", 44 + SWORD_ANG[i as usize] + num4), [rx / 2.0 - 105.0, ry / 2.0 - 82.0, 160.0, 100.0]));
            }
        }
        if let Some((rx, ry)) = raw(tr, sel) {
            // AnimateKnightAndChickenButton: idle frames 0..18 every 50 ms, after a pick the walk 19..49 once
            let f = (ms - PICKED.load(Relaxed)) / 50;
            let frame = if (0..=30).contains(&f) { 19 + f } else { (ms / 50) % 19 };
            let dx = if sel == front { -241.0 } else { -211.0 };
            // the co-op trails show the two partners (GameSprites 718), still
            let key = if COOPS.contains(&tr) { "skirmish_trail_coop_character".to_string() } else { format!("skirmish_trail_icons-{}", 93 + frame) };
            sprites.push((ry / 2.0 + 30.0, key, [rx / 2.0 + dx, ry / 2.0 - 200.0, 340.0, 210.0]));
        }
        sprites.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, key, [x, y, sw, sh]) in &sprites { pic(out, key, self.r(*x, *y, *sw, *sh), 1.0); }
        // the flag (ZIndex 20000): 16 frames of 60 ms
        if let Some((rx, ry)) = raw(tr, front) {
            pic(out, &format!("skirmish_trail_icons-{}", 16 + (ms / 60) % 16), self.r(rx / 2.0 - 110.0, ry / 2.0 - 112.0, 147.0, 120.0), 1.0);
        }
        // the mission hit boxes (TrailMapClicked: ±60 × ±32 around the raw spot, missions up to the frontier)
        for m in 1..=front {
            if let Some((rx, ry)) = raw(tr, m) { self.hits.push((self.r(rx / 2.0 - 30.0, ry / 2.0 - 16.0, 60.0, 32.0), Act::Trail(A::Pick(m)))); }
        }
        // texts: mission title 30 px at 20,20; its date 22 px at 30,60; the trail's date 30 px right at 30 in
        // (the Sands: the trail's title at 20,20, the mission's at 30,60, then its target and best times)
        if (11..=17).contains(&tr) {
            self.label(ui, out, &ui.text_of(&format!("TEXT_SANDS_OF_TIME_{:03}", if tr == 17 { 103 } else { tr - 9 })), 20.0, 40.0, 30.0, 0.0, WHITE);
            self.label(ui, out, &mission_name(ui, tr, sel), 30.0, 80.0, 30.0, 0.0, WHITE);
            self.sands_map(ui, out, tr, sel, &p, cursor);
        } else {
            self.label(ui, out, &mission_name(ui, tr, sel), 20.0, 40.0, 30.0, 0.0, WHITE);
        }
        let ad = ui.text_of("TEXT_NEW_CTEXT_365");
        if tr <= 2 {
            let d = p.start_date(sel - 1);
            self.label(ui, out, &format!("{} {ad}", d / 12), 30.0, 74.0, 22.0, 0.0, WHITE);
            let now = p.start_date(p.next - 1);
            self.label(ui, out, &format!("{} {} {ad}", ui.text_of(&format!("TEXT_MONTHS_{:03}", now % 12 + 1)), now / 12), 1890.0, 40.0, 30.0, 1.0, WHITE);
        }
        let back = Act::Trail(A::Open(if tr >= 90 { T::Custom } else if COOPS.contains(&tr) { T::Coop } else if tr >= 11 { T::Sands } else { T::Trails { options: false } }));
        if COOPS.contains(&tr) {
            self.coop_pane(ui, out, back, cursor);
            return;
        }
        // difficulty (TEXT_MAINOPTIONS_025 over a BTN_SH_GlowL: TEXT_MAINOPTIONS 19 + d, 4 = TEXT_NEW_TEXT2_215); the timed
        // Sands are played at Normal (Show_LeaderboardOptOut: only relaxed)
        if tr <= 2 || ((11..=17).contains(&tr) && relaxed()) {
            self.label(ui, out, &ui.text_of("TEXT_MAINOPTIONS_025"), 1760.0, 117.0, 26.0, 0.5, WHITE);
            let d = if p.difficulty == 4 { ui.text_of("TEXT_NEW_TEXT2_216") } else { ui.text_of(&format!("TEXT_MAINOPTIONS_{:03}", 20 + p.difficulty)) };
            self.glow(ui, out, &d, 1610.0, 146.0, 300.0, None, Some(Act::Trail(A::Difficulty)), cursor);
        }
        // Show_TrailCustomisationButtons (classic trails): "Mission" / Customize = the skirmish lobby set up as this mission
        if tr <= 2 {
            self.label(ui, out, &ui.text_of("TEXT_CUSTOMISATION_070"), 1760.0, 197.0, 26.0, 0.5, WHITE);
            self.glow(ui, out, &ui.text_of("TEXT_CUSTOMISATION_071"), 1610.0, 226.0, 300.0, None, Some(Act::Trail(A::Customize)), cursor);
            // Skirmish Masters (-11), 192×176 at right 100, bottom 160
            self.img_button(ui, out, ["SkirmishMasters", "SkirmishMastersOver", "SkirmishMasters"], 1628.0, 744.0, 192.0, 176.0, Some(Act::Trail(A::Masters)), cursor);
        }
        // the chicken: skip the frontier mission (rollover picture + "cheat" line)
        let chick = self.r(903.5, 700.0, 506.0, 374.0);
        let hot = Self::inside(chick, cursor) && p.next <= n;
        pic(out, if hot { "skirmish_trail_icons-175" } else { "skirmish_trail_icons-143" }, chick, 1.0);
        if p.next <= n { self.hits.push((chick, Act::Trail(A::Chicken))); }
        if hot { self.label(ui, out, &ui.text_of("TEXT_CHEATS_001"), 960.0, 1048.0, 24.0, 0.5, WHITE); }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(back), cursor);
        self.img_button(ui, out, ["Load Shield Up", "Load Shield Over", "Load Shield Up"], 1567.0, 957.0, 103.0, 103.0, Some(Act::LoadList), cursor);
        self.img_button(ui, out, ["UI-Buttons B001", "UI-Buttons B002", "UI-Buttons B001"], 1709.0, 984.0, 151.0, 66.0, Some(Act::Trail(A::Start)), cursor);
    }
}



/// ConfigSettings.Settings_HideSoTTiming: the relaxed Sands of Time (no timing, ranks, leaderboard; free difficulty).
fn relaxed() -> bool { skirmish::Settings::load().flag("HideSoTTiming", false) }

/// GameData's Sands rank (2929): time `t` against target `target` (seconds): 4 Prince of the Sands (< T/2), 3 Champion
/// (< 3T/4), 2 Warrior (< T), 1 Tribesman (≤ 2T), 0 Peasant. Sprite rank_{r + 1}, text TEXT_SANDS_OF_TIME 64 + r.
pub fn sands_rank(t: i32, target: i32) -> i32 {
    if t < target / 2 { 4 } else if t < target * 3 / 4 { 3 } else if t < target { 2 } else if t <= 2 * target { 1 } else { 0 }
}

/// A Sands best-time slot as seconds: Some for a timed win (not -1 / chicken -1200 / relaxed int.MaxValue).
fn timed(t: i32) -> Option<i32> { (0..i32::MAX).contains(&t).then_some(t / 40) }

/// A Sands trail's best times summed against its targets (seconds); None while a mission is not timed.
fn sands_total(p: &Progress) -> Option<(i32, i32)> {
    p.times.iter().enumerate().try_fold((0, 0), |(t, target), (k, &v)| Some((t + timed(v)?, target + sands_target(p.trail, k as i32)?)))
}

/// The local stand-in for the Steam leaderboards "Sands{n}.{m}" (Platform_Leaderboards, score = seconds, KeepBest):
/// sands_leaderboard.cfg in the player folder, one `Sands1.1:seconds:name` line per player and mission.
fn board_file() -> PathBuf { skirmish::profile_dir().join("sands_leaderboard.cfg") }

/// Mission `id`'s board, best first: (name, seconds).
pub fn board(trail: i32, id: i32) -> Vec<(String, i32)> {
    let key = format!("Sands{}.{}:", trail - 10, id + 1);
    let text = std::fs::read_to_string(board_file()).unwrap_or_default();
    let mut v: Vec<(String, i32)> = text.lines().filter_map(|l| {
        let (secs, name) = l.strip_prefix(&key)?.split_once(':')?;
        Some((name.to_string(), secs.parse().ok()?))
    }).collect();
    v.sort_by_key(|e| e.1);
    v
}

/// UploadSandsScore: `name`'s time on mission `id`, kept when it beats their best.
fn board_add(trail: i32, id: i32, name: &str, secs: i32) {
    let key = format!("Sands{}.{}:", trail - 10, id + 1);
    let name = name.replace(['\n', '\r'], " ");
    let text = std::fs::read_to_string(board_file()).unwrap_or_default();
    let mut best = secs;
    let mut lines: Vec<String> = text.lines().filter(|l| {
        let Some((t, n)) = l.strip_prefix(&key).and_then(|r| r.split_once(':')) else { return true };
        if n != name { return true; }
        best = best.min(t.parse().unwrap_or(secs));
        false
    }).map(str::to_string).collect();
    lines.push(format!("{key}{best}:{name}"));
    if let Err(e) = std::fs::create_dir_all(skirmish::profile_dir()).and_then(|_| std::fs::write(board_file(), lines.join("\n") + "\n")) { eprintln!("sands leaderboard: {e}"); }
}

impl Menu {
    /// The Sands of Time trail map's own parts (FRONT_SandsTrail*.xaml, FrontendMenus 2515-2620): target and best time of
    /// the selected mission, the mission list (BTN_SH_Campaign_Image: name, best rank), the trail's rank and times top
    /// right, the leaderboard button (-13). The timed parts not in relaxed mode.
    fn sands_map(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, tr: i32, sel: i32, p: &Progress, cursor: [f32; 2]) {
        const CREAM: [f32; 4] = [0.937, 0.953, 0.776, 1.0]; // #EFF3C6
        let timing = !relaxed();
        let target = sands_target(tr, sel - 1).unwrap_or(0);
        if timing {
            self.label(ui, out, &format!("{} : {}", ui.text_of("TEXT_SANDS_OF_TIME_060"), sands_time(target)), 40.0, 115.0, 22.0, 0.0, CREAM);
            let best = timed(p.times[(sel - 1) as usize]).map_or("-- : --".to_string(), sands_time);
            self.label(ui, out, &format!("{} : {best}", ui.text_of("TEXT_SANDS_OF_TIME_063")), 40.0, 147.0, 22.0, 0.0, CREAM);
        }
        // the mission list (680×410 at 107,220; rows 370 wide from +60,+30), up to the frontier clickable
        self.frame(out, 132.0, 220.0, 640.0, 60.0 + 47.33 * p.len() as f32, true);
        for m in 1..=p.len() {
            let y = 250.0 + 47.33 * (m - 1) as f32;
            let name = mission_name(ui, tr, m);
            let name = name.split_once(". ").map_or(name.clone(), |(_, n)| n.to_string());
            let a = if m <= p.next { Act::Trail(A::Pick(m)) } else { Act::Trail(A::Pick(sel)) };
            self.campaign_button(ui, out, &m.to_string(), &name, 192.0, y, 370.0, m == sel, a, cursor);
            if let (true, Some(t), Some(target)) = (timing, timed(p.times[(m - 1) as usize]), sands_target(tr, m - 1)) {
                ui.img_fit(out, &format!("rank_{}_S", sands_rank(t, target) + 1), self.r(192.0 + 308.0 + 70.0, y - 11.0, 100.0, 50.0), WHITE);
                self.label(ui, out, &sands_time(t), 192.0 + 380.0 + 100.0, y + 18.7, 20.0, 0.0, WHITE);
            } else if timing && p.times[(m - 1) as usize] >= 0 {
                ui.img_fit(out, "rank_1_S", self.r(192.0 + 378.0, y - 11.0, 100.0, 50.0), WHITE); // untimed: Peasant (592)
            }
        }
        if !timing { return; }
        // top right: "Trail Completed" + its rank, or "Trail Not Yet Complete"; Time in Trail; Target Time
        let total: i32 = p.times.iter().filter_map(|&t| timed(t)).sum();
        let targets: i32 = (0..p.len()).filter_map(|k| sands_target(tr, k)).sum();
        let done = p.completed() == p.len();
        let head = ui.text_of(if done { "TEXT_SANDS_OF_TIME_061" } else { "TEXT_SANDS_OF_TIME_070" });
        self.label(ui, out, &head, 1600.0, 250.0, 26.0, 1.0, WHITE);
        if done {
            let r = sands_total(p).map_or("rank_6".to_string(), |(t, target)| format!("rank_{}", sands_rank(t, target) + 1));
            ui.img_fit(out, &r, self.r(1620.0, 205.0, 100.0, 90.0), WHITE);
        }
        self.label(ui, out, &format!("{} : {}", ui.text_of("TEXT_SANDS_OF_TIME_062"), sands_time(total)), 1890.0, 320.0, 26.0, 1.0, WHITE);
        self.label(ui, out, &format!("{} : {}", ui.text_of("TEXT_SANDS_OF_TIME_060"), sands_time(targets)), 1890.0, 360.0, 26.0, 1.0, WHITE);
        // the leaderboard (-13), 192×176 at right 100, bottom 160
        self.img_button(ui, out, ["Leaderboard", "LeaderboardOver", "Leaderboard"], 1628.0, 744.0, 192.0, 176.0, Some(Act::Trail(A::Board)), cursor);
    }

    /// HUD_Leaderboard (local): mission `sel`'s best times, 10 rows (#, name, time), over the trail map.
    fn board_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, tr: i32, sel: i32, cursor: [f32; 2]) {
        Ui::fill(out, self.r(0.0, 0.0, 1920.0, 1080.0), [0.0, 0.0, 0.0, 0.5]);
        self.frame(out, 560.0, 160.0, 800.0, 700.0, true);
        self.label(ui, out, &mission_name(ui, tr, sel), 960.0, 215.0, 32.0, 0.5, INK);
        self.label(ui, out, &format!("{} : {}", ui.text_of("TEXT_SANDS_OF_TIME_060"), sands_time(sands_target(tr, sel - 1).unwrap_or(0))), 960.0, 260.0, 22.0, 0.5, INK);
        let rows = board(tr, sel - 1);
        if rows.is_empty() { self.label(ui, out, &ui.text_of("TEXT_SANDS_OF_TIME_069"), 960.0, 420.0, 24.0, 0.5, INK); }
        for (k, (name, secs)) in rows.iter().take(10).enumerate() {
            let y = 320.0 + 48.0 * k as f32;
            self.label(ui, out, &format!("{}", k + 1), 640.0, y, 24.0, 0.0, INK);
            self.label(ui, out, name, 720.0, y, 24.0, 0.0, INK);
            self.label(ui, out, &sands_time(*secs), 1280.0, y, 24.0, 1.0, INK);
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::Trail(A::Quit)), cursor);
    }
}

/// The Sands of Time timer in a game (OnScreenText.UpdateSandsOfTimer): `trail` = main.rs's (trail, id, difficulty,
/// recorded) while no menu is open, `tick` = the game time, `over` = the game ended (the end screens take over: the
/// results are `Outro`). Timed mode only and with the SandsTimer setting: top centre the elapsed time, the rank it
/// would earn now and a bar draining within that rank's band.
pub fn sands_overlay(trail: Option<(i32, i32, i32, bool)>, tick: Option<i32>, over: bool, size: [f32; 2], ui: Option<&mut Ui>, mut hud: Vec<HudTri>) -> Vec<HudTri> {
    let (Some((tr, id, _, _)), Some(tick), Some(ui), false) = (trail, tick, ui, over) else { return hud };
    let Some(target) = sands_target(tr, id) else { return hud };
    let cfg = skirmish::Settings::load();
    if cfg.flag("HideSoTTiming", false) || !cfg.flag("SandsTimer", true) { return hud; }
    let [w, h] = size;
    let s = crate::ux::hud_scale(w, h);
    (ui.s, ui.w, ui.h) = (s, w, h);
    let t = tick.max(0) / 40;
    let r = sands_rank(t, target);
    // the band [lo, hi) of the current rank: the bar shows what is left of it
    let bands = [target / 2, target * 3 / 4, target, 2 * target];
    let (lo, hi) = match r { 4 => (0, bands[0]), 3 => (bands[0], bands[1]), 2 => (bands[1], bands[2]), 1 => (bands[2], bands[3] + 1), _ => (0, 0) };
    let left = if hi > lo { (hi - t) as f32 / (hi - lo) as f32 } else { 0.0 };
    let (x0, y0) = (w / 2.0 - 260.0 * s, 8.0 * s);
    Ui::fill(&mut hud, [x0, y0, x0 + 520.0 * s, y0 + 64.0 * s], [0.0, 0.0, 0.0, 0.55]);
    ui.img_fit(&mut hud, &format!("rank_{}_S", r + 1), [x0 + 6.0 * s, y0 + 4.0 * s, x0 + 86.0 * s, y0 + 44.0 * s], WHITE);
    ui.text_sh(&mut hud, &sands_time(t), x0 + 260.0 * s, y0 + 36.0 * s, 28.0, 0.5, WHITE);
    ui.text_sh(&mut hud, &ui.text_of(&format!("TEXT_SANDS_OF_TIME_{:03}", 64 + r)), x0 + 512.0 * s, y0 + 30.0 * s, 18.0, 1.0, GOLD);
    let bar = [x0 + 13.0 * s, y0 + 48.0 * s, x0 + 507.0 * s, y0 + 58.0 * s];
    Ui::fill(&mut hud, bar, [0.15, 0.12, 0.08, 1.0]);
    Ui::fill(&mut hud, [bar[0], bar[1], bar[0] + (bar[2] - bar[0]) * left.clamp(0.0, 1.0), bar[3]], [0.86, 0.68, 0.25, 1.0]);
    hud
}

/// A full-screen still the end screens want that the first-run export lacks, made once from the game's own files on
/// a background thread into ui/screens/<name>.rgba (1920×1080, what `screen_pixels` reads): "sands-score-bg" =
/// SandsofTimeScoreBackground (IngameUIScreens.xaml, behind the Sands results), "weasel" = the last frame of
/// WeaselTurd_low.webm (the classic trails' finale, too big to export as a video). True once the file is there.
pub fn still(name: &'static str) -> bool {
    let path = crate::cache_dir().join("ui/screens").join(format!("{name}.rgba"));
    if path.exists() { return true; }
    static STARTED: std::sync::Mutex<Vec<&str>> = std::sync::Mutex::new(vec![]);
    let mut started = STARTED.lock().unwrap();
    if started.contains(&name) { return false; }
    started.push(name);
    std::thread::spawn(move || {
        let make = || -> shc_extract::Result<shc_extract::tex::Rgba> {
            let data = crate::setup::find_game().ok_or("no game folder")?.join(shc_extract::GAME_DATA);
            match name {
                "weasel" => shc_extract::video::frames(&data.join("Assets/GUI/Video/WeaselTurd_low.webm"))?.0.last_rgba(),
                _ => shc_extract::tex::Textures::open(&data, &["sharedassets1.assets"])?.get("SandsofTimeScoreBackground"),
            }
        };
        let part = path.with_extension("part"); // the file lands whole
        let r = make().and_then(|im| Ok(std::fs::write(&part, im.resize(1920, 1080).px).and_then(|_| std::fs::rename(&part, &path))?));
        if let Err(e) = r { eprintln!("end screen still {name}: {e}"); }
    });
    false
}

/// The still `name` (`still`; main.rs uploads it into SCREEN, see endgame::Screen::screen_image) over the whole window
/// (Stretch UniformToFill), black until it is there.
pub fn backdrop(out: &mut Vec<HudTri>, name: &'static str, w: f32, h: f32) {
    Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, 1.0]);
    if !still(name) { return; }
    let k = (w / 1920.0).max(h / 1080.0);
    quad(out, SCREEN, [(w - 1920.0 * k) / 2.0, (h - 1080.0 * k) / 2.0, (w + 1920.0 * k) / 2.0, (h + 1080.0 * k) / 2.0], SCREEN_UV, WHITE);
}

/// What HUD_MissionOver shows on Exit after the last mission of a trail was won (ButtonClicked "Exit", ShownWeaselTurd):
/// `Weasel(extreme)` = the classic trails' WeaselTurd video with its lines (storyboard WeaselTurd1, the Extreme
/// trail's WeaselTurd2); `Desert(trail)` = a Sands of Time or co-op trail's looped "Desert Background" with the
/// CoopCompletion box and music 131. A click anywhere goes on (InvisibleButton).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Finale { Weasel(bool), Desert(i32) }

impl Finale {
    /// After winning mission `id` (0-based) of `trail`: its finale when that was the trail's last (classic level ≥ 49 /
    /// 29 / 19, Sands its last, co-op mission ≥ 9); the custom trails have none.
    pub fn after(trail: i32, id: i32) -> Option<Self> {
        if id + 1 < count(trail) { return None; }
        match trail { 0 | 1 => Some(Finale::Weasel(false)), 2 => Some(Finale::Weasel(true)), 11..=17 | 21..=23 => Some(Finale::Desert(trail)), _ => None }
    }

    /// Its start: SFXManager.playMusic(131) = DeTrailer for the desert. ponytail: the WeaselTurd video's own sound
    /// isn't played (the still has none).
    pub fn start(&self) { if let Finale::Desert(_) = self { crate::audio::music("fx\\music\\DeTrailer.raw", 1.0) } }

    /// `t` = seconds since it started. Weasel: the video's last frame as a still (`still`), the lines fading in and out
    /// on WeaselTurd1's / 2's timeline (36 px cream, centred in 400 × 250, wrapped). Desert: the video (black while it
    /// is not exported) and the 700 × 250 box: WGT_Heading2 "Congratulations!", the English branch's two lines (30 px
    /// black at 140 / 190): "You have (both) bested" and the trail's name.
    pub fn draw(&self, ui: &mut Ui, out: &mut Vec<HudTri>, t: f32, w: f32, h: f32) {
        let mut m = Menu::main();
        m.s = (w / 1920.0).min(h / 1080.0);
        m.o = [(w - 1920.0 * m.s) / 2.0, (h - 1080.0 * m.s) / 2.0];
        (ui.s, ui.w, ui.h) = (m.s, w, h);
        match *self {
            Finale::Weasel(extreme) => {
                backdrop(out, "weasel", w, h);
                let last = if extreme { "TEXT_SKTRAIL_WIN_005" } else { "TEXT_SKTRAIL_WIN_004" };
                for (key, a, b) in [("TEXT_SKTRAIL_WIN_001", 0.5, 39.5), ("TEXT_SKTRAIL_WIN_002", 40.5, 43.0), ("TEXT_SKTRAIL_WIN_003", 44.0, 46.5), (last, 47.5, 72.0)] {
                    let alpha = ((t - a) / 0.5).clamp(0.0, 1.0).min(((b + 0.5 - t) / 0.5).clamp(0.0, 1.0));
                    if alpha <= 0.0 { continue; }
                    let text = ui.text_of(key);
                    let lines = wrap(ui, &text, 400.0 * m.s, 36.0);
                    let top = 540.0 - 36.0 * 1.25 * lines.len() as f32 / 2.0;
                    for (k, l) in lines.iter().enumerate() {
                        let mid = top + 36.0 * 1.25 * (k as f32 + 0.5);
                        m.label(ui, out, l, 962.0, mid + 2.0, 36.0, 0.5, [0.0, 0.0, 0.0, 0.8 * alpha]); // DropShadowEffect #CC000000, 2 px
                        m.label(ui, out, l, 960.0, mid, 36.0, 0.5, [0.937, 0.953, 0.776, alpha]);
                    }
                }
            }
            Finale::Desert(trail) => {
                if crate::video::mission_over(out, "desert_background", true, w, h).is_none() { Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, 1.0]); }
                let (title, name) = if trail >= 21 {
                    ("TEXT_NEW_CTEXT_528".to_string(), format!("TEXT_COOP_{:03}", 22 + trail - 20))
                } else {
                    ("TEXT_NEW_CTEXT_534".to_string(), format!("TEXT_SANDS_OF_TIME_{:03}", if trail == 17 { 103 } else { trail - 9 }))
                };
                m.frame(out, 610.0, 415.0, 700.0, 250.0, true);
                m.o[1] += 410.0 * m.s; // WGT_Heading2 at the box's top (`heading` draws 5 px down)
                let congrats = ui.text_of("TEXT_SKTRAIL_WIN_001");
                m.heading(ui, out, 602.5, &congrats);
                m.o[1] -= 410.0 * m.s;
                for (key, top) in [(title, 140.0), (name, 190.0)] {
                    let text = ui.text_of(&key);
                    m.label(ui, out, &text, 960.0, 415.0 + top + 30.0 * 0.62, 30.0, 0.5, INK);
                }
            }
        }
    }
}

/// `text` broken at spaces into lines at most `width` screen px wide at `size`.
fn wrap(ui: &mut Ui, text: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(l) if ui.text(&mut vec![], &format!("{l} {word}"), 0.0, 0.0, size, 1.0, 0.0, 0.0, WHITE) <= width => { l.push(' '); l.push_str(word) }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// The Sands of Time results after a won mission in timed mode (HUD_MissionOver.InitSands, MO_SandsOutro1 over
/// SandsofTimeScoreBackground, no video): the trail's name; this mission's box (name, time taken, rank and its large
/// picture, a bar running out at 4 × the target with the rank bands' pictures and times); the trail's mission times
/// (4 per column, each a bar to its peasant time); the trail's ranking and time against 4 × the targets; then Stats
/// (the score table) and Exit. The SandsOutro storyboard: the box's bar runs (1-5 s), the box slides down while the
/// mission times fade in (5-6 s), they slide down while the trail line fades in with the buttons (10-11 s), its bar
/// runs (11-15 s). 1920 × 1080 design space, centred.
pub struct Outro { tr: i32, id: i32, secs: i32, p: Progress, t0: Instant, lay: Menu, hits: Vec<([f32; 4], bool)> }

/// QuarticEase over [a, b]: EaseOut, or EaseInOut.
fn quartic(t: f32, a: f32, b: f32, in_out: bool) -> f32 {
    let x = ((t - a) / (b - a)).clamp(0.0, 1.0);
    if !in_out { 1.0 - (1.0 - x).powi(4) } else if x < 0.5 { 8.0 * x.powi(4) } else { 1.0 - 8.0 * (1.0 - x).powi(4) }
}

impl Outro {
    /// The results for a win of Sands mission `id` (0-based) of `tr` that took `ticks`: timed Sands only.
    pub fn new(tr: i32, id: i32, ticks: i32) -> Option<Self> {
        (SANDS.contains(&tr) && !relaxed()).then(|| Outro { tr, id, secs: ticks.max(0) / 40, p: Progress::load(tr), t0: Instant::now(), lay: Menu::main(), hits: vec![] })
    }

    /// One frame (`t0` = when the storyboard began). ponytail: the box shows at once (the storyboard fades it in over
    /// its first second; BDR_SH1DEb has no opacity here).
    pub fn draw(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, w: f32, h: f32, cursor: [f32; 2]) {
        const CREAM: [f32; 4] = [0.937, 0.953, 0.776, 1.0]; // #EFF3C6
        const RED: [f32; 4] = [0.635, 0.212, 0.173, 1.0]; // #A2362C
        let t = self.t0.elapsed().as_secs_f32();
        let m = &mut self.lay;
        m.s = (w / 1920.0).min(h / 1080.0);
        m.o = [(w - 1920.0 * m.s) / 2.0, (h - 1080.0 * m.s) / 2.0];
        (ui.s, ui.w, ui.h) = (m.s, w, h);
        backdrop(out, "sands-score-bg", w, h);
        let rank_text = |ui: &Ui, r: i32| ui.text_of(&format!("TEXT_SANDS_OF_TIME_{:03}", 64 + r));
        let fade = |c: [f32; 4], a: f32| [c[0], c[1], c[2], c[3] * a];
        // black text with the #CCC3B85D drop shadow (2 px), `top` = the TextBlock's top
        let ink = |m: &Menu, ui: &mut Ui, out: &mut Vec<HudTri>, s: &str, x: f32, top: f32, size: f32, align: f32| {
            m.label(ui, out, s, x + 2.0, top + size * 0.62 + 2.0, size, align, [0.765, 0.722, 0.365, 0.8]);
            m.label(ui, out, s, x, top + size * 0.62, size, align, INK);
        };
        // the five rank pictures under a bar (Prince first): band `on` full, the faster ones greyed at half, the slower greyed
        let ranks = |ui: &Ui, out: &mut Vec<HudTri>, m: &Menu, x0: f32, y0: f32, xs: [f32; 5], on: usize, a: f32| {
            for (k, x) in xs.into_iter().enumerate() {
                let key = format!("rank_{}{}", 5 - k, if k == on { "" } else { "_g" });
                ui.img_fit(out, &key, m.r(x0 + x, y0 - 20.0, 105.0, 110.0), [1.0, 1.0, 1.0, a * if k < on { 0.5 } else { 1.0 }]);
            }
        };
        // Update(): the band the bar's run-out part `gone` (of `full`) is in
        let band = |gone: f32, full: f32| [0.125, 0.1875, 0.25, 0.5].iter().position(|&f| gone < f * full).unwrap_or(4);
        // the heading (WGT_Heading2 at the grid's top: 50 px margin, -20)
        let name = ui.text_of(&format!("TEXT_SANDS_OF_TIME_{:03}", if self.tr == 17 { 103 } else { self.tr - 9 }));
        m.o[1] += 25.0 * m.s;
        m.heading(ui, out, 602.5, &name);
        m.o[1] -= 25.0 * m.s;
        // TrailMission: the box (BDR_SH1DEb 800 × 150 in a 1600 wide Viewbox) at 110, 170; at 720 after its drop (5-6 s)
        let target = sands_target(self.tr, self.id).unwrap_or(60);
        let (x0, y0) = (110.0, 170.0 + 550.0 * quartic(t, 5.0, 6.0, true));
        m.frame(out, x0, y0 + 5.0, 1600.0, 300.0, true);
        let label = format!("{} :", ui.text_of("TEXT_SANDS_OF_TIME_078"));
        ink(m, ui, out, &label, x0 + 30.0, y0 + 20.0, 24.0, 0.0);
        let mname = mission_name(ui, self.tr, self.id + 1);
        ink(m, ui, out, mname.split_once(". ").map_or(&mname[..], |(_, n)| n), x0 + 30.0, y0 + 55.0, 26.0, 0.0);
        let r = sands_rank(self.secs, target);
        ink(m, ui, out, &format!("{} : {}", ui.text_of("TEXT_SANDS_OF_TIME_073"), sands_time(self.secs)), x0 + 30.0, y0 + 110.0, 26.0, 0.0);
        ink(m, ui, out, &format!("{} : {}", ui.text_of("TEXT_SANDS_OF_TIME_074"), rank_text(ui, r)), x0 + 30.0, y0 + 150.0, 26.0, 0.0);
        ui.img_fit(out, &format!("rank_{} L", r + 1), m.r(x0 + 1320.0, y0 - 50.0, 300.0, 300.0), WHITE);
        // its bar: 1500 wide = 4 × the target, the red part what was left of it (MO_Sands_CurProgressBar, 1-5 s)
        let peasant = 4 * target;
        let fill = 1500.0 - 1500.0 * self.secs.min(peasant) as f32 / peasant as f32;
        let fill = 1500.0 + (fill - 1500.0) * quartic(t, 1.0, 5.0, false);
        for (x, y, bw, bh) in [(44.0, 199.0, 1508.0, 2.0), (44.0, 227.0, 1508.0, 2.0), (44.0, 199.0, 2.0, 30.0), (1550.0, 199.0, 2.0, 30.0)] {
            Ui::fill(out, m.r(x0 + x, y0 + y, bw, bh), INK);
        }
        Ui::fill(out, m.r(x0 + 48.0 + 1500.0 - fill, y0 + 203.0, fill, 22.0), RED);
        for x in [0.0, 188.0, 281.0, 375.0, 750.0, 1499.0] { Ui::fill(out, m.r(x0 + 47.0 + x, y0 + 228.0, 2.0, 6.0), RED); }
        ranks(ui, out, m, x0 + 48.0, y0 + 228.0, [37.0, 181.0, 275.0, 509.0, 1072.0], band(1500.0 - fill, 1500.0), 1.0);
        for (x, secs) in [(138.0, target / 2), (231.0, target * 3 / 4), (325.0, target), (700.0, target * 2)] {
            ink(m, ui, out, &sands_time(secs), x0 + 48.0 + x + 50.0, y0 + 233.0, 14.0, 0.5);
        }
        ink(m, ui, out, "0", x0 + 48.0, y0 + 233.0, 14.0, 0.0);
        ink(m, ui, out, "∞", x0 + 48.0 + 1500.0, y0 + 223.0, 30.0, 1.0);
        // TrailMissions: fade in at 5-6 s at the top, slide down 260 px at 10-11 s
        let a = (t - 5.0).clamp(0.0, 1.0);
        let (x1, y1) = (62.5, 170.0 + 260.0 * quartic(t, 10.0, 11.0, true));
        if a > 0.0 {
            m.label(ui, out, &format!("{} :", ui.text_of("TEXT_SANDS_OF_TIME_077")), x1 + 30.0, y1 + 10.0 + 30.0 * 0.62, 30.0, 0.0, fade(CREAM, a));
            for k in 0..self.p.len() {
                let (x, y) = (x1 + 50.0 + 470.0 * (k / 4) as f32, y1 + 60.0 + 40.0 * (k % 4) as f32);
                let peasant = 4 * sands_target(self.tr, k).unwrap_or(60);
                let v = timed(self.p.times[k as usize]).filter(|&v| v > 0);
                m.label(ui, out, &format!("{}.", k + 1), x, y + 26.0 * 0.62, 26.0, 0.0, fade(CREAM, a));
                let b = m.r(x + 40.0, y + 10.0, 402.0, 10.0);
                let e = 2.0 * m.s;
                for r in [[b[0], b[1], b[2], b[1] + e], [b[0], b[3] - e, b[2], b[3]], [b[0], b[1], b[0] + e, b[3]], [b[2] - e, b[1], b[2], b[3]]] {
                    Ui::fill(out, r, [1.0, 1.0, 1.0, 0.533 * a]); // #88FFFFFF
                }
                let fill = v.map_or(0.0, |v| 396.0 - 396.0 * v.min(peasant) as f32 / peasant as f32);
                Ui::fill(out, m.r(x + 43.0 + 396.0 - fill, y + 13.0, fill, 4.0), fade(RED, a));
                m.label(ui, out, &v.map_or("--:--".to_string(), sands_time), x + 440.0, y - 5.0 + 14.0 * 0.62, 14.0, 1.0, fade(CREAM, a));
            }
        }
        // TrailInfo: fade in at 10-11 s; its bar 1700 wide = 4 × the targets of the missions timed so far (11-15 s)
        let a = (t - 10.0).clamp(0.0, 1.0);
        if a > 0.0 {
            let x2 = 62.5;
            let done: Vec<(i32, i32)> = (0..self.p.len()).filter_map(|k| Some((timed(self.p.times[k as usize]).filter(|&v| v > 0)?, sands_target(self.tr, k)?))).collect();
            let (total, targets) = done.iter().fold((0, 0), |(a, b), &(v, t)| (a + v, b + t));
            let all = done.len() as i32 == self.p.len();
            // ponytail: InitSands ranks the trail against 4 × the targets (num *= 4 before GetSandsOfTimeRankImage); the
            // port ranks it against the targets, like the trail map's ranking
            let rank = if targets > 0 { rank_text(ui, sands_rank(total, targets)) } else { ui.text_of("TEXT_SANDS_OF_TIME_069") };
            let (rk, tk) = if all { ("TEXT_SANDS_OF_TIME_076", "TEXT_SANDS_OF_TIME_079") } else { ("TEXT_SANDS_OF_TIME_075", "TEXT_SANDS_OF_TIME_080") };
            m.label(ui, out, &format!("{} : {rank}", ui.text_of(rk)), x2 + 30.0, 170.0 + 10.0 + 30.0 * 0.62, 30.0, 0.0, fade(CREAM, a));
            m.label(ui, out, &format!("{} : {}", ui.text_of(tk), sands_time(total)), x2 + 30.0, 170.0 + 50.0 + 30.0 * 0.62, 30.0, 0.0, fade(CREAM, a));
            let full = 4 * targets.max(1);
            let fill = 1700.0 - 1700.0 * total.min(full) as f32 / full as f32;
            let fill = 1700.0 + (fill - 1700.0) * quartic(t, 11.0, 15.0, false);
            let (bx, by) = (x2 + 44.0, 170.0 + 99.0);
            for (x, y, bw, bh) in [(0.0, 0.0, 1708.0, 2.0), (0.0, 48.0, 1708.0, 2.0), (0.0, 0.0, 2.0, 50.0), (1706.0, 0.0, 2.0, 50.0)] {
                Ui::fill(out, m.r(bx + x, by + y, bw, bh), fade(WHITE, a));
            }
            Ui::fill(out, m.r(bx + 4.0 + 1700.0 - fill, by + 4.0, fill, 42.0), fade(RED, a));
            for x in [0.0, 213.0, 318.0, 425.0, 850.0, 1699.0] { Ui::fill(out, m.r(x2 + 47.0 + x, 170.0 + 148.0, 2.0, 10.0), fade(WHITE, a)); }
            ranks(ui, out, m, x2 + 48.0, 170.0 + 148.0, [52.0, 215.0, 322.0, 587.0, 1225.0], band(1700.0 - fill, 1700.0), a);
            m.label(ui, out, "0", x2 + 45.0, 170.0 + 161.0 + 16.0 * 0.62, 16.0, 0.0, fade(CREAM, a));
            m.label(ui, out, "∞", x2 + 48.0 + 1708.0, 170.0 + 151.0 + 30.0 * 0.62, 30.0, 1.0, fade(CREAM, a));
        }
        // Stats (S055 / S056, 140 high) and Exit (B001 / B002, 60 high) bottom right, fading in with the trail line
        self.hits.clear();
        for (p, s, stats) in [(m.r(1784.7, 790.0, 90.3, 140.0), ["UI-Buttons S055", "UI-Buttons S056"], true), (m.r(1776.0, 960.0, 124.0, 60.0), ["UI-Buttons B001", "UI-Buttons B002"], false)] {
            ui.img_fit(out, s[Menu::inside(p, cursor) as usize], p, [1.0, 1.0, 1.0, a]);
            self.hits.push((p, stats));
        }
    }

    /// A left press: Some(true) = Stats, Some(false) = Exit (they answer while still transparent, like Noesis').
    pub fn click(&self, c: [f32; 2]) -> Option<bool> {
        let b = self.hits.iter().find(|(p, _)| Menu::inside(*p, c))?.1;
        crate::audio::ui(crate::audio::ui_id::RED_BUTTON, 1.0);
        Some(b)
    }
}


#[cfg(feature = "live")]
impl Menu {
    /// "Customize" (FrontendMenus 906-960 → FRONT_Multiplayer.Open(skirmishSetup, trail, mission)): the skirmish lobby
    /// prefilled from DLL_GetTrailMissionInfo (FRONT_Multiplayer.doOpen's customise branch): the mission's map, its AI
    /// lords and teams (slot order), fairness and starting goods, the start positions (locations), each lord's castle
    /// (its AIV code: that one AIV, its rotation), the barracks / mercenary post / Bedouin stockade switches (the
    /// stockade on with the Classic Trails option; advanced_skirmish_options = their OR); the Extreme trail adds
    /// extreme troops and powers and triple start gold. Outposts kept when the mission keeps them (barracks 200). A
    /// normal skirmish then: no trail progress.
    fn customize(&mut self, tr: i32, id: i32) {
        let Some(b) = brief(tr, id) else { return };
        let m = &b.m;
        let mut lobby = skirmish::Lobby { team: m.teams[0], fairness: m.fairness, goods: m.starting_level, outposts: m.barracks == 200, ..skirmish::Lobby::from_settings(b.path.clone()) };
        (lobby.advanced, lobby.adv) = (false, skirmish::ADV_DEFAULT); // MPsetupData from the defaults, not the presets
        let slots: Vec<usize> = (1..m.players.clamp(1, 8) as usize).filter(|&k| m.lords[k] > 0).collect();
        lobby.ais = slots.iter().map(|&k| ((m.lords[k] - 1) as usize, m.teams[k])).collect();
        let mut keeps = [-10; 8];
        for (member, &k) in std::iter::once(&0).chain(&slots).enumerate() {
            if let Some(v) = keeps.get_mut((m.locations[k] - 1) as usize) { *v = member as i32; }
            if member > 0 { lobby.aivs[member] = m.aiv[k]; }
        }
        lobby.keeps = Some(keeps);
        let stockade = if skirmish::Settings::load().flag("Allow_Classic_Bedouin_Stockade", false) { 255 } else { m.stockade };
        lobby.buildings[..3].copy_from_slice(&[m.barracks, m.merc_post, stockade]);
        lobby.advanced = (m.barracks | m.merc_post | stockade) != 0;
        if tr == 2 { (lobby.extreme[0], lobby.extreme[1], lobby.extreme_trail) = (true, true, true); }
        self.open_lobby(lobby);
    }
}

/// What a trail briefing shows of mission `id`: the engine's record (DLL_GetTrailMissionInfo) and its map file.
#[cfg(feature = "live")]
#[derive(Clone)]
struct Brief { m: shc_sim::trail::Mission, path: PathBuf, info: Option<skirmish::MapInfo> }

/// The briefing data of mission `id` of `trail` (cached: the record reads the DLL image).
#[cfg(feature = "live")]
fn brief(trail: i32, id: i32) -> Option<Brief> {
    static LAST: std::sync::Mutex<Option<((i32, i32), Brief)>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    if let Some((k, b)) = last.as_ref() { if *k == (trail, id) { return Some(b.clone()); } }
    let m = shc_sim::trail::Mission::read(&shc_sim::mem::Mem::default(), trail, id)?;
    let path = skirmish::maps_dir().join(format!("{}.map", m.map));
    let b = Brief { info: skirmish::map_info(&path), m, path };
    *last = Some(((trail, id), b.clone()));
    Some(b)
}

/// The GameActions a trail start sends (StartSkirmishTrailMission / LoadSkirmishMap): HideObjectiveProgress(1, 1), and
/// for the Sands of Time in relaxed mode (HideSoTTiming) SandsCheatAllowed(1, 1).
pub fn start_actions(trail: i32) -> Vec<[i32; 3]> {
    let mut v = vec![[1045, 1, 1]];
    if (11..=18).contains(&trail) && skirmish::Settings::load().flag("HideSoTTiming", false) { v.push([1072, 1, 1]); }
    v
}

/// GameData.GetSandsOfTimeTargetTime: the target of Sands mission `id` (0-based) in seconds (sands<n>_TargetTimes, minutes).
pub fn sands_target(trail: i32, id: i32) -> Option<i32> {
    const T: [&[i32]; 7] = [&[50, 60, 75, 80, 100], &[80, 100, 70, 60, 110, 70, 60], &[80, 80, 70, 120, 90, 110, 120, 70, 80],
        &[110, 60, 80, 90, 120, 80, 100, 80, 70, 60, 140], &[50, 50, 70, 110, 120, 180, 90, 50, 190], &[45, 105, 100, 100, 80, 80, 50, 90, 120],
        &[100, 60, 100, 110, 120, 90, 150, 80, 100]];
    T.get((trail - 11) as usize)?.get(id as usize).map(|m| m * 60)
}

/// GameData's time text (2731): below an hour "mm:ss" (minutes padded to 2 with a space), else "hh:mm:ss".
pub fn sands_time(secs: i32) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h == 0 { format!("{:>2}:{s:02}", secs / 60) } else { format!("{h:>2}:{m:02}:{s:02}") }
}

/// GameData.starting_gold_table[goods − 1, fairness − 1, human ? 0 : 1].
fn start_gold(goods: i32, fairness: i32, human: bool) -> i32 {
    const G: [[[i32; 2]; 5]; 4] = [
        [[8000, 2000], [4000, 2000], [2000, 2000], [2000, 4000], [2000, 8000]],
        [[8000, 2000], [4000, 2000], [2000, 2000], [2000, 4000], [2000, 8000]],
        [[40000, 3000], [20000, 7000], [10000, 10000], [7000, 20000], [3000, 40000]],
        [[4000, 500], [2000, 500], [500, 500], [500, 2000], [500, 4000]],
    ];
    G[(goods - 1).clamp(0, 3) as usize][(fairness - 1).clamp(0, 4) as usize][!human as usize]
}

#[cfg(not(feature = "live"))]
impl Menu {
    fn brief_screen(&mut self, _: &mut Ui, _: &mut Vec<HudTri>, _: i32, _: i32, _: f32, _: f32, _: [f32; 2]) {}
}

#[cfg(feature = "live")]
impl Menu {
    /// HUD_Briefing's skirmish overlay (BriefingSkirmishVis, a 1365×768 grid over the paused game; xaml 805-1850):
    /// victory_military_small, the mission title, the lords in their cases (face, starting gold, name; crossed swords
    /// between enemies, your team first), the radar with keep shields, map size and starting goods, the Sands
    /// "Times to Beat" (not when relaxed), Quit Mission / Start Game.
    fn brief_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, tr: i32, id: i32, w: f32, h: f32, cursor: [f32; 2]) {
        // its own 1365×768 Viewbox (menu.rs lays the trail screens out on 1920×1080)
        self.s = (w / 1365.0).min(h / 768.0);
        self.o = [(w - 1365.0 * self.s) / 2.0, (h - 768.0 * self.s) / 2.0];
        ui.s = self.s;
        Ui::fill(out, [0.0, 0.0, w, h], INK);
        pic(out, "lobby", self.r(0.0, 0.0, 1365.0, 768.0), 1.0);
        let Some(b) = brief(tr, id) else { return };
        let m = &b.m;
        // SkirmishBriefingMapName (WGT_Heading2, 32 px): the Warchest trail numbers from 51
        let title = if tr == 1 { format!("{}. {}", id + 51, ui.text_of(&format!("TEXT_TRAIL_NAMES_CRU_{:03}", id + 51))) } else { mission_name(ui, tr, id + 1) };
        self.frame(out, 325.0, 25.0, 715.0, 60.0, true);
        self.label(ui, out, &title, 682.5, 55.0, 32.0, 0.5, INK);
        // the lords: player slots in order, your team first (SkirmishBriefingAlly / AllyFight)
        let n = (m.players.clamp(1, 8)) as usize;
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&k| (m.teams[k] == 0 || m.teams[k] != m.teams[0], m.teams[k], k));
        order.retain(|&k| k == 0 || m.lords[k] > 0);
        let mult = if tr == 2 { 3 } else { 1 };
        let cases = order.len() as f32;
        let row_w = cases * 144.0 + 16.0 + (cases - 1.0) * 16.0;
        let mut x = 682.5 - row_w / 2.0;
        for (j, &k) in order.iter().enumerate() {
            if j > 0 {
                let (a, b) = (order[j - 1], k);
                if m.teams[a] == 0 || m.teams[a] != m.teams[b] { ui.img_fit(out, "UI-Buttons S041", self.r(x - 12.0, 230.0, 40.0, 40.0), WHITE); } // centred in the row
                x += 16.0;
            }
            ui.img_fit(out, "Lord Case", self.r(x, 150.0, 160.0, 200.0), WHITE);
            let face = self.r(x + 32.0, 201.0, 96.0, 96.0);
            ui.img(out, FACE_BACKGROUNDS[k], face, WHITE);
            if k == 0 {
                let e = 96.0 * 0.15 * self.s;
                if !super::coa::draw_mine(out, face, WHITE) { ui.img_fit(out, "shield_crusader", [face[0] + e, face[1] + e, face[2] - e, face[3] - e], WHITE); }
                ui.img(out, "UI Player Border", face, WHITE);
            } else if let Some(&(f, _)) = FACES.iter().find(|f| f.1 as i32 == m.lords[k] - 1) {
                ui.img(out, f, face, WHITE);
            }
            if m.aiv[k] >= 1000 { ui.img_fit(out, "UI-Buttons L028", self.r(x + 94.0, 183.0, 16.0, 16.0), WHITE); } // castle pre-built
            ui.img_fit(out, "UISprites I024", self.r(x + 63.5, 163.0, 33.0, 22.0), WHITE);
            self.label(ui, out, &(start_gold(m.starting_level, m.fairness, k == 0) * mult).to_string(), x + 80.0, 193.0, 13.0, 0.5, GOLD);
            let name = if k == 0 { player_name() } else { skirmish::LORDS.get((m.lords[k] - 1) as usize).copied().unwrap_or("").to_string() };
            self.label(ui, out, &name, x + 80.0, 306.0, 13.0, 0.5, INK); // 10 px in the original: too small to read
            x += 144.0;
        }
        // radar block (260 wide) + Sands times (260, 100 apart), centred at y 405
        let sands = tr >= 11 && !skirmish::Settings::load().flag("HideSoTTiming", false);
        let x0 = 682.5 - if sands { 620.0 } else { 260.0 } / 2.0;
        let n = crate::live::RADAR_N as f32;
        let rad = [x0 + 30.0, 410.0];
        quad(out, 2, self.r(rad[0], rad[1], 200.0, 200.0), [0.0, 0.0, 200.0 / n, 200.0 / n], WHITE);
        if let Some(size @ (160 | 300 | 500 | 700)) = b.info.as_ref().map(|i| i.size) {
            ui.img(out, &format!("Border {size}"), self.r(rad[0], rad[1], 200.0, 200.0), WHITE);
        }
        // keep shields: player k at start position locations[k] (RadarShield k + 1 = UI-Buttons H007 + 3k), margin (x − 4, y − 6)
        for k in 0..n.min(8.0) as usize {
            let Some(&(kx, ky)) = b.info.as_ref().and_then(|i| i.keeps.get((m.locations[k] - 1).max(0) as usize)) else { continue };
            if m.locations[k] <= 0 || kx < 0 || (k > 0 && m.lords[k] <= 0) { continue; }
            let (sx, sy) = (rad[0] + (kx - 4).max(1) as f32, rad[1] + (ky - 6).max(1) as f32);
            ui.img_fit(out, &format!("UI-Buttons H{:03}", 7 + 3 * k), self.r(sx, sy, 16.0, 16.0), WHITE);
            if m.teams[k] > 0 { ui.img_fit(out, &format!("Team Allies Shield {} Large", m.teams[k]), self.r(sx + 14.0, sy + 8.0, 12.0, 12.0), WHITE); }
        }
        ui.img_fit(out, "Map Banner", self.r(x0 + 8.0, 615.0, 260.0, 40.0), WHITE);
        let size = b.info.as_ref().map_or(0, |i| i.size);
        self.label(ui, out, &format!("{} {size}", ui.text_of("TEXT_NEW_CTEXT_251")), x0 + 135.0, 635.0, 14.0, 0.5, INK);
        // starting goods level 1..3: Normal / Crusader / Deathmatch (SkirmishSetup 029 / 032 / 035)
        let g = m.starting_level.clamp(1, 3);
        ui.img_fit(out, &format!("SkirmishSetup {:03}", 26 + 3 * g), self.r(x0 + 50.0, 655.0, 40.0, 40.0), WHITE);
        self.label(ui, out, &ui.text_of(&format!("TEXT_GAME_TYPE_{g:03}")), x0 + 100.0, 675.0, 18.0, 0.0, WHITE);
        if sands {
            if let Some(t) = sands_target(tr, id) {
                let (sx, sy) = (x0 + 360.0, 390.0);
                self.frame(out, sx + 25.0, sy, 210.0, 260.0, true);
                self.label(ui, out, &ui.text_of("TEXT_SANDS_OF_TIME_072"), sx + 130.0, sy + 20.0, 14.0, 0.5, INK);
                let times = [sands_time(t / 2), sands_time(t * 3 / 4), sands_time(t), sands_time(2 * t), "-- : --".into()];
                for (r, txt) in times.iter().enumerate() {
                    ui.img_fit(out, &format!("rank_{}", 5 - r), self.r(sx + 40.0, sy + 20.0 + 40.0 * r as f32, 70.0, 40.0), WHITE);
                    self.label(ui, out, txt, sx + 200.0, sy + 53.0 + 40.0 * r as f32, 14.0, 1.0, INK);
                }
            }
        }
        // Quit Mission (B005) / Start Game (B001), 107×47, 70 above the bottom, ∓470 from the centre
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 159.0, 651.0, 107.0, 47.0, Some(Act::Trail(A::Quit)), cursor);
        self.img_button(ui, out, ["UI-Buttons B001", "UI-Buttons B002", "UI-Buttons B001"], 1099.0, 651.0, 107.0, 47.0, Some(Act::Trail(A::Start)), cursor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// settings.cfg round trip, GameData's win rule and the chicken, getTrailStartDate.
    #[test]
    fn trail_progress_rules() {
        let cfg = skirmish::Settings::parse("||SETTINGS||\nCampaignTrail:3\nTrail1Times:12:30:-1:\nTrail1Difficulty:4\n||SETTINGS||\n");
        let mut p = Progress::read(&cfg, 0);
        assert_eq!((p.next, p.difficulty, p.len(), &p.times[..4]), (3, 4, 50, &[12, 30, -1, -1][..]));
        assert_eq!(p.completed(), 2);
        assert_eq!((p.start_date(0), p.start_date(2), p.start_date(10)), (13200, 13242, 13242)); // Jan 1100, Jul 1103
        p.won(2, 20);
        assert_eq!((p.next, p.times[2]), (4, 20));
        p.won(2, 25); // slower: best time kept, no unlock
        assert_eq!((p.next, p.times[2]), (4, 20));
        p.won(0, 5); // replay of an old one: better time, frontier unchanged
        assert_eq!((p.next, p.times[0]), (4, 5));
        p.chicken();
        assert_eq!((p.next, p.times[4]), (5, -1200));
        let mut last = Progress { trail: 2, next: 20, times: vec![1; 20], difficulty: 1 };
        last.won(19, 3);
        assert_eq!(last.next, 21);
        assert_eq!(Progress::read(&skirmish::Settings::default(), 13), Progress { trail: 13, next: 1, times: vec![-1; 9], difficulty: 1 });
        // co-op: 10 missions, a mission over (won or lost) goes back to the co-op lobby, not straight on
        assert_eq!(Progress::read(&skirmish::Settings::default(), 22).len(), 10);
        assert!(matches!(after(22, 3, 1, true), Go::ToMenu) && matches!(after(22, 3, 1, false), Go::ToMenu));
    }

    /// Main menu → Skirmish → Classic Trails → the First Edition map (its frontier selected) → Start: mission 1 loads
    /// natively and ticks; its win lands in settings.cfg (best time, mission 2 unlocked) and the continue starts 2.
    #[cfg(feature = "live")]
    #[test]
    fn trail_menu_path_ticks() {
        let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
        if !skirmish::maps_dir().join("A New Land.map").exists() { return; } // needs the game install
        let dir = std::env::temp_dir().join(format!("shc-trail-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\n||SETTINGS||\n").unwrap(); // a fresh player, not the real game's
        crate::skirmish::set_test_profile(&dir);
        let press = |m: &mut Menu, u: &mut Ui, a: Act| {
            m.draw(u, 1920.0, 1080.0, [0.0; 2]);
            let p = m.hits.iter().find(|h| h.1 == a).expect("button on screen").0;
            m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0])
        };
        let mut m = Menu::main();
        assert!(press(&mut m, &mut u, Act::Trail(A::Open(T::Skirmish))).is_none());
        assert!(matches!(m.screen, Screen::Trail(T::Skirmish)));
        assert!(press(&mut m, &mut u, Act::Trail(A::Open(T::Trails { options: false }))).is_none());
        assert!(press(&mut m, &mut u, Act::Trail(A::Open(T::Map(0, 1)))).is_none());
        assert!(matches!(m.screen, Screen::Trail(T::Map(0, 1))));
        assert_eq!(m.screen_image().as_deref(), Some("trail-map-0"));
        let Some(Go::Trail(0, 0, 1)) = press(&mut m, &mut u, Act::Trail(A::Start)) else { panic!("Start did not start mission 1") };
        let mut l = crate::live::Live::from_mem(start(0, 0, 1).unwrap());
        let t0 = l.tick();
        for _ in 0..50 { l.step(); }
        assert_eq!(l.tick() - t0, 50);
        record_win(0, 0, 5, 1102, 0); // won in June 1102: 29 months after January 1100
        let p = Progress::load(0);
        assert_eq!((p.next, p.times[0]), (2, 29));
        assert!(matches!(after(0, 0, 1, true), Go::Trail(0, 1, 1)) && matches!(after(0, 0, 1, false), Go::Trail(0, 0, 1)));
        assert!(matches!(Menu::trail_map(0).screen, Screen::Trail(T::Map(0, 2))));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Co-op trail 1 mission 1 over LAN on this Mac: the host's co-op lobby, the partner joins, Start: both peers build
    /// the same co-op game (lan::game → shc_sim::trail::coop_setup: networked MP, coop ids 1 / 0) and run 600 ticks in
    /// lockstep with the original's sync check (FUN_180021ff0) compared every tick.
    #[cfg(feature = "live")]
    #[test]
    fn coop_mission_two_peers() {
        use crate::lan::{self, Lockstep, Net};
        use std::time::{Duration, Instant};
        if !skirmish::maps_dir().join("TippedScales.map").exists() { return; }
        let mut host = Net::test_host();
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], host.port()));
        let join = std::thread::spawn(move || Net::join(addr, "Partner").expect("join"));
        let t0 = Instant::now();
        while host.lobby.humans.len() < 2 && t0.elapsed() < Duration::from_secs(5) { host.poll(); std::thread::sleep(Duration::from_millis(2)); }
        let mut client = join.join().unwrap();
        let l = coop_lobby(&host, 21, 1, false);
        host.set_lobby(l);
        let (lobby, seed, delay) = host.start(1792400000, lan::DELAY);
        while client.started.is_none() && t0.elapsed() < Duration::from_secs(10) { client.poll(); std::thread::sleep(Duration::from_millis(2)); }
        let (cl, cs, cd) = client.started.clone().expect("the partner got Start");
        assert_eq!((&cl, cs, cd), (&lobby, seed, delay));
        assert_eq!((lobby.coop, lobby.map.as_str(), lobby.humans.len()), (Some((1, 0, false)), "TippedScales.map", 2));
        let done = std::sync::Arc::new(std::sync::Barrier::new(2));
        let peer = |net: Net, lobby: lan::Lobby, done: std::sync::Arc<std::sync::Barrier>| move || {
            let me = net.me;
            let mut l = crate::live::Live::from_mem(lan::game(&lobby, me, seed).expect("co-op game"));
            assert_eq!((l.mem().i32(0x188571b80), l.mem().i32(0x183668020), l.mem().i32(0x183668024)), (1, 1, 0), "networked MP, co-op 1 / mission 0");
            let t0 = l.tick();
            let mut ls = Lockstep::new(net, delay, t0);
            ls.hash_every = 1;
            l.net = Some(ls);
            let start = Instant::now();
            while l.tick() < t0 + 600 && start.elapsed() < Duration::from_secs(120) {
                if let Some(e) = l.net.as_ref().and_then(|n| n.error.clone()) { panic!("player {me}: {e}"); }
                // through the lockstep gate like the app (ungated, a fast peer outruns the 64 kept sums)
                if l.net_ready() { l.step(); } else { std::thread::sleep(Duration::from_micros(300)); }
            }
            let t1 = Instant::now();
            while t1.elapsed() < Duration::from_secs(5) && l.net.as_ref().unwrap().hashes_checked < 598 {
                l.net_ready();
                std::thread::sleep(Duration::from_millis(5));
            }
            let n = l.net.as_ref().unwrap();
            assert!(n.error.is_none(), "player {me}: {:?}", n.error);
            let (checked, tick) = (n.hashes_checked, l.tick());
            let sums = shc_sim::mp::sync_check(l.mem()).unwrap();
            done.wait();
            (me, tick, checked, sums)
        };
        let a = std::thread::spawn(peer(host, lobby, done.clone()));
        let b = std::thread::spawn(peer(client, cl, done));
        let (a, b) = (a.join().expect("host"), b.join().expect("partner"));
        assert_eq!((a.0, b.0), (1, 2));
        assert!(a.1 >= 600 && b.1 >= 600 && a.2 >= 598 && b.2 >= 598, "ticks / sync checks: {:?} {:?}", (a.1, a.2), (b.1, b.2));
        assert_eq!(a.3, b.3, "final sync sums differ");
    }

    /// coop.cfg: wins per partner; the frontier follows the best partner, the swords the current one (CalcCoopProgress).
    #[test]
    fn coop_progress_per_partner() {
        let dir = std::env::temp_dir().join(format!("shc-coop-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::skirmish::set_test_profile(&dir);
        std::fs::remove_file(dir.join("coop.cfg")).ok();
        coop_won("Ann", "0222110191010", 21, 0).unwrap();
        coop_won("Bob", "", 21, 2).unwrap();
        coop_won("Ann", "", 23, 9).unwrap();
        assert_eq!(coop_lines()[0].2, "0222110191010"); // Ann's coat of arms, kept
        let any = coop_progress(21, None);
        assert_eq!((any.next, &any.times[..4]), (4, &[0, -1, 0, -1][..]));
        let ann = coop_progress(21, Some("Ann"));
        assert_eq!((ann.next, &ann.times[..4], ann.completed()), (4, &[0, -1, -1, -1][..], 1));
        assert_eq!(coop_progress(23, Some("Bob")).next, 11);
        assert_eq!(std::fs::read_to_string(dir.join("coop.cfg")).unwrap().lines().count(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The co-op lobby without a human partner: the AI ally picker, Swap keeps, Start → a single-player co-op game
    /// (skirmish, coop ids, the ally lord as player 2 on your team) that ticks; with a LAN session the chat line
    /// sends what you type.
    #[cfg(feature = "live")]
    #[test]
    fn coop_lobby_ally_swap_chat() {
        let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
        if !skirmish::maps_dir().join("TippedScales.map").exists() { return; }
        let dir = std::env::temp_dir().join(format!("shc-coop-lobby-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::skirmish::set_test_profile(&dir);
        let press = |m: &mut Menu, u: &mut Ui, a: Act| {
            m.draw(u, 1920.0, 1080.0, [0.0; 2]);
            let p = m.hits.iter().rev().find(|h| h.1 == a).expect("button on screen").0;
            m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0])
        };
        let mut m = Menu { screen: Screen::Trail(T::Map(21, 1)), ..Menu::main() };
        assert!(press(&mut m, &mut u, Act::Trail(A::AllyOpen)).is_none());
        assert!(press(&mut m, &mut u, Act::Trail(A::Ally(6))).is_none()); // the Sultan
        assert!(press(&mut m, &mut u, Act::Trail(A::Swap)).is_none());
        let c = coop_ui();
        assert_eq!((c.ally, c.swapped, c.partner.clone()), (Some(6), true, Some("AI Sultan".into())));
        drop(c);
        let Some(Go::Trail(21, 0, _)) = press(&mut m, &mut u, Act::Trail(A::Start)) else { panic!("Start did not start co-op mission 1") };
        let mem = start(21, 0, 1).expect("co-op with an AI ally");
        let mut l = crate::live::Live::from_mem(mem);
        assert_eq!((l.mem().i32(0x188571b80), l.mem().i32(0x183668020), l.mem().i32(0x188571c34 + 8), l.mem().i32(0x188571bbc + 8)), (99, 1, 7, -1), "skirmish, co-op 1, ally = AI lord 6 + 1 in slot 2");
        let t0 = l.tick();
        for _ in 0..50 { l.step(); }
        assert_eq!(l.tick() - t0, 50);
        record_win(21, 0, 0, 0, 0);
        assert_eq!(coop_progress(21, Some("AI Sultan")).times[0], 0);
        // Back resets the lobby; hosting opens the chat line
        assert!(m.back().is_none());
        assert_eq!(coop_ui().ally, None);
        let mut m = Menu { screen: Screen::Trail(T::Map(21, 1)), ..Menu::main() };
        m.mp.net = Some(crate::lan::Net::test_host());
        assert!(press(&mut m, &mut u, Act::Trail(A::Type)).is_none());
        assert!(m.trail_typing());
        for k in ["H,shift", "I", "Space", "Alpha1,shift", "Return"] { m.trail_key(k); }
        assert_eq!(m.mp.net.as_ref().unwrap().chat.last().map(|c| c.1.as_str()), Some("Hi !"));
        assert!(!m.trail_typing());
        // Escape (Menu::back) on an open chat line closes only the line, the lobby stays
        assert!(press(&mut m, &mut u, Act::Trail(A::Type)).is_none());
        assert!(m.back().is_none());
        assert!(!m.trail_typing() && m.mp.net.is_some() && matches!(m.screen, Screen::Trail(T::Map(21, _))));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The user's case: a non-fresh profile (missions 1-2 won, progress 3) through the menu path starts mission 3 (a
    /// 400-tile map). With the camera a 1684×784 window on the keep sends (main.rs Cmd::View), 600 ticks run, the
    /// units around the keep are drawn (the lord among them) and a placed hovel appears. Before the map-size offset
    /// in Live::run_tick that 800-tile camera culled every unit and new building on maps smaller than 800.
    #[test]
    #[cfg(feature = "live")]
    fn trail_start_with_progress() {
        let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
        let dir = std::env::temp_dir().join(format!("shc-trail-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let times: String = [44, 60].into_iter().chain([-1; 48]).map(|t| format!("{t}:")).collect();
        std::fs::write(dir.join("settings.cfg"), format!("||SETTINGS||\nCampaignTrail:3\nTrail1Times:{times}\nTrail1Difficulty:1\n||SETTINGS||\n")).unwrap();
        crate::skirmish::set_test_profile(&dir);
        let press = |m: &mut Menu, u: &mut Ui, a: Act| {
            m.draw(u, 1920.0, 1080.0, [0.0; 2]);
            let p = m.hits.iter().find(|h| h.1 == a).expect("button on screen").0;
            m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0])
        };
        let mut m = Menu::main();
        assert!(press(&mut m, &mut u, Act::Trail(A::Open(T::Skirmish))).is_none());
        assert!(press(&mut m, &mut u, Act::Trail(A::Open(T::Trails { options: false }))).is_none());
        assert!(press(&mut m, &mut u, Act::Trail(A::Open(T::Map(0, 1)))).is_none());
        assert!(matches!(m.screen, Screen::Trail(T::Map(0, 3))), "the map opens at the frontier");
        let Some(Go::Trail(0, 2, 1)) = press(&mut m, &mut u, Act::Trail(A::Start)) else { panic!("Start did not start mission 3") };
        let Ok(mem) = start(0, 2, 1) else { return }; // needs the game install
        assert_eq!(mem.i32(0x1860ab524), 400);
        let mut l = crate::live::Live::from_mem(mem);
        let (kx, ky) = l.keep().map(|(x, y)| (x as i32, y as i32)).expect("own keep");
        let (wx, wy) = crate::replay::to_west_pub(l.rotation(), kx as i16, ky as i16);
        let [px, py] = crate::replay::cell_px(wx, wy);
        (l.view, l.centre) = (Some(crate::live::camera([px - 842.0, py - 392.0, px + 842.0, py + 392.0])), [kx, ky, 26, 24]);
        let (mut w, mut dirty) = (crate::replay::World::new(), vec![]);
        w.apply(&l.records(), &mut dirty);
        let t0 = l.tick();
        let mut hovel = None;
        for k in 0..600 {
            if k == 100 {
                l.set_tool(54);
                let mut near: Vec<(i32, i32)> = (-12..=12).flat_map(|d| (-12..=12).map(move |e| (d, e))).collect();
                near.sort_by_key(|&(d, e)| d.abs() + e.abs()); // nearest first: inside the window's view
                let spot = near.into_iter().map(|(d, e)| (kx + d, ky + e)).find(|&(x, y)| l.hover(54, x, y).ok).expect("a free spot");
                assert!(l.click(54, spot.0, spot.1).1 > 0, "the hovel's place chore");
                l.set_tool(0);
                // its footprint's tile records (West frame) before the place chore runs
                let near: Vec<usize> = (-2..=2).flat_map(|d| (-2..=2).map(move |e| (spot.0 + d, spot.1 + e))).map(|(x, y)| {
                    let (x, y) = crate::replay::to_west_pub(l.rotation(), x as i16, y as i16);
                    y as usize * crate::replay::N + x as usize
                }).collect();
                hovel = Some(near.iter().map(|&i| (i, w.tiles[i])).collect::<Vec<_>>());
            }
            w.apply(&l.step(), &mut dirty);
        }
        assert_eq!(l.tick() - t0, 600);
        let m = l.mem();
        let lord = shc_sim::campaign::bot::army_of(m, 1).into_iter().map(|u| u.0).find(|&i| m.i16(0x1867e53f0 + i as u64 * 0x490 + 0x6e6) == 0x37).expect("your lord");
        let drawn = w.objs.keys().filter(|k| k.0 == 2).count();
        assert!(drawn >= 10 && w.objs.contains_key(&(2, lord)), "units drawn: {drawn}, lord {lord} drawn: {}", w.objs.contains_key(&(2, lord)));
        assert!(hovel.unwrap().iter().any(|&(i, before)| w.tiles[i] != before), "the placed hovel is drawn");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The trail briefing (HUD_Briefing's skirmish overlay): opens over the mission paused (in_game) with its map's radar
    /// preview; Start = Go::Brief (main.rs: unpause, HideObjectiveProgress(0, 0), ResetTime), Quit Mission = back to
    /// the trail map, Escape stays; none for the co-op trails. Sands: targets, rank times, relaxed SandsCheatAllowed.
    #[test]
    #[cfg(feature = "live")]
    fn trail_briefing() {
        let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
        if !skirmish::maps_dir().join("A New Land.map").exists() { return; }
        let dir = std::env::temp_dir().join(format!("shc-trail-brief-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\n||SETTINGS||\n").unwrap();
        crate::skirmish::set_test_profile(&dir);
        assert!(Menu::trail_brief(21, 0, 1).is_none(), "co-op: no briefing");
        let press = |m: &mut Menu, u: &mut Ui, a: Act| {
            m.draw(u, 1920.0, 1080.0, [0.0; 2]);
            let p = m.hits.iter().find(|h| h.1 == a).expect("button on screen").0;
            m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0])
        };
        let mut m = Menu::trail_brief(0, 0, 2).expect("briefing");
        assert!(m.in_game && m.preview.as_ref().is_some_and(|p| p.len() == 200 * 200 * 4));
        assert!(matches!(m.back(), None) && matches!(m.screen, Screen::Trail(T::Brief(0, 0, 2))), "Escape stays");
        assert!(matches!(press(&mut m, &mut u, Act::Trail(A::Start)), Some(Go::Brief(2))));
        let mut m = Menu::trail_brief(0, 0, 2).unwrap();
        assert!(matches!(press(&mut m, &mut u, Act::Trail(A::Quit)), Some(Go::ToMenu)));
        // Sands 1 #1: target 50 min; the rank times T/2, 3T/4, T, 2T as GameData formats them
        assert_eq!(sands_target(11, 0), Some(3000));
        assert_eq!([sands_time(1500), sands_time(2250), sands_time(3000), sands_time(6000), sands_time(59)], ["25:00", "37:30", "50:00", " 1:40:00", " 0:59"]);
        assert_eq!(start_actions(11), vec![[1045, 1, 1]]);
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nHideSoTTiming:True\n||SETTINGS||\n").unwrap();
        assert_eq!(start_actions(11), vec![[1045, 1, 1], [1072, 1, 1]]);
        assert_eq!(start_actions(0), vec![[1045, 1, 1]]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The Sands of Time: GameData's ranks, a timed win keeps the best ticks and posts its seconds to the local
    /// leaderboard (best per player, sorted), a relaxed win stores int.MaxValue (done, untimed, no board), the trail's
    /// total; the in-game timer and the results draw.
    #[test]
    fn sands_timing_ranks_board() {
        let dir = std::env::temp_dir().join(format!("shc-sands-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nName:Lord Test\n||SETTINGS||\n").unwrap();
        crate::skirmish::set_test_profile(&dir);
        let t = 3000; // Sands 1 #1: 50 min
        assert_eq!([0, 1499, 1500, 2249, 2250, 2999, 3000, 6000, 6001].map(|x| sands_rank(x, t)), [4, 4, 3, 3, 2, 2, 1, 1, 0]);
        record_win(11, 0, 0, 0, 40 * 2000); // 33:20
        record_win(11, 0, 0, 0, 40 * 2600); // slower: the best stays
        let p = Progress::load(11);
        assert_eq!((p.times[0], p.next), (80000, 2));
        board_add(11, 0, "Emir", 1900);
        board_add(11, 0, "Emir", 2500);
        assert_eq!(board(11, 0), vec![("Emir".to_string(), 1900), ("Lord Test".to_string(), 2000)]);
        assert!(board(11, 1).is_empty());
        assert_eq!(sands_total(&p), None, "not all missions timed");
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nName:Lord Test\nHideSoTTiming:True\n||SETTINGS||\n").unwrap();
        record_win(11, 1, 0, 0, 40 * 100);
        assert_eq!(Progress::load(11).times[1], i32::MAX);
        assert!(board(11, 1).is_empty(), "relaxed: nothing posted");
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nName:Lord Test\n||SETTINGS||\n").unwrap();
        let mut p = Progress::load(11);
        p.times = vec![40 * 1000, 40 * 1200, 40 * 1500, 40 * 1600, 40 * 2000];
        assert_eq!(sands_total(&p), Some((7300, 21900)));
        assert_eq!(sands_rank(7300, 21900), 4);
        if let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) {
            let running = sands_overlay(Some((11, 0, 1, false)), Some(40 * 600), false, [1684.0, 784.0], Some(&mut u), vec![]);
            let over = sands_overlay(Some((11, 0, 1, true)), Some(40 * 2000), true, [1684.0, 784.0], Some(&mut u), vec![]);
            assert!(!running.is_empty() && over.is_empty(), "the game over: the end screens show the results (Outro)");
            assert!(sands_overlay(Some((0, 0, 1, false)), Some(400), false, [1684.0, 784.0], Some(&mut u), vec![]).is_empty(), "classic: no timer");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Customize on the classic trail map: the skirmish lobby set up as the selected mission (its map selected, the AI
    /// lords and teams, fairness, goods); not on the Sands maps.
    #[test]
    #[cfg(feature = "live")]
    fn trail_customize() {
        let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
        if !skirmish::maps_dir().join("A New Land.map").exists() { return; }
        let dir = std::env::temp_dir().join(format!("shc-trail-custom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\n||SETTINGS||\n").unwrap();
        crate::skirmish::set_test_profile(&dir);
        let mut m = Menu { screen: Screen::Trail(T::Map(0, 1)), ..Menu::main() };
        m.draw(&mut u, 1920.0, 1080.0, [0.0; 2]);
        let p = m.hits.iter().find(|h| h.1 == Act::Trail(A::Customize)).expect("Customize button").0;
        assert!(m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]).is_none());
        let Screen::Lobby(l) = &m.screen else { panic!("not the lobby") };
        let ms = shc_sim::trail::Mission::read(&shc_sim::mem::Mem::default(), 0, 0).unwrap();
        assert!(l.map.ends_with("A New Land.map"));
        assert_eq!((l.ais.clone(), l.fairness, l.goods), (vec![(6, ms.teams[1])], ms.fairness, ms.starting_level), "you vs the Sultan");
        assert!(m.preview.is_some(), "the map selected (its radar preview)");
        // the mission's start positions, the Sultan's castle, the barracks / mercenary post / stockade switches
        let mut keeps = [-10; 8];
        (keeps[(ms.locations[0] - 1) as usize], keeps[(ms.locations[1] - 1) as usize]) = (0, 1);
        assert_eq!((l.keeps, l.aivs[1], l.buildings[..3].to_vec()), (Some(keeps), ms.aiv[1], vec![ms.barracks, ms.merc_post, ms.stockade]));
        assert_eq!(l.advanced, (ms.barracks | ms.merc_post | ms.stockade) != 0);
        println!("mission 1: locations {:?} aiv {:?} barracks {} merc {} stockade {}", ms.locations, ms.aiv, ms.barracks, ms.merc_post, ms.stockade);
        let mut s = Menu { screen: Screen::Trail(T::Map(11, 1)), ..Menu::main() };
        s.draw(&mut u, 1920.0, 1080.0, [0.0; 2]);
        assert!(!s.hits.iter().any(|h| h.1 == Act::Trail(A::Customize)), "not on the Sands maps");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Every trail has a map position per mission and a mission name.
    #[test]
    fn trail_tables() {
        for tr in CLASSIC.into_iter().chain(SANDS).chain(COOPS) {
            let n = count(tr);
            #[cfg(feature = "live")]
            assert_eq!(n, if COOPS.contains(&tr) { shc_sim::trail::COOP[(tr - 21) as usize].len() as i32 } else { shc_sim::trail::missions(tr) }, "trail {tr}");
            assert_eq!(coords(tr).len() as i32, 2 * n, "trail {tr}");
            assert!(raw(tr, n).is_some() && raw(tr, n + 1).is_none());
        }
    }
}

/// Options colour 3 (blue): the tutorial (loadMap's colour mapping) and a single-player co-op mission with the Rat as
/// ally (BuildMultiPlayerColourMapping) give player 1 colour 2 like a skirmish, not the default red.
#[cfg(feature = "live")]
#[test]
fn tutorial_and_coop_take_your_colour() {
    let dir = std::env::temp_dir().join(format!("shc-colour-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nPlayerColour:3\n||SETTINGS||\n").unwrap();
    skirmish::set_test_profile(&dir);
    let Ok(m) = skirmish::start_tutorial() else { return }; // needs the game's maps
    assert_eq!(crate::replay::colours(&m)[1], 2, "tutorial: you blue");
    { let mut u = coop_ui(); (u.ally, u.swapped) = (Some(0), false); }
    let m = start(21, 0, 1).expect("co-op mission 1");
    let c = crate::replay::colours(&m);
    assert_eq!(c[1], 2, "co-op: you blue {c:?}");
    assert!(c[2] != 0 && c[2] != 2, "the ally another colour {c:?}");
    coop_ui().ally = None;
}
