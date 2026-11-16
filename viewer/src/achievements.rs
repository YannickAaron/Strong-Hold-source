//! Local achievements: DE's Steam achievements (Enums.Achievements, AchievementsCommon, Platform_Achievements) kept
//! in the player folder (achievements.cfg) instead of Steam, a toast in game when one unlocks (`toast`) and the list
//! on Extras (extras.rs). The conditions are DE's where the port sees them; titles and texts are ours, plain (Steam
//! keeps DE's names and descriptions, not the game files). Not observable here, listed but never unlocked: Map_Uploaded_To_Workshop
//! (no Workshop upload) and Scribe_Unlock (the newsletter sign-up on Firefly's server).
//! The per-game store counters (food / wood / weapons) travel in the save like DE's: StoreValuesInEngine's
//! DLL_SetAchValues puts them at 0x183663fe0 / fe4 / fe2 (i16), DLL_SaveSaveGame writes those 6 bytes into header
//! block 4 and DLL_LoadSaveGame reads them back (`world::load_save`); `save_loaded` takes them from there. The engine
//! never counts them itself (only the Stored messages' amounts), so the count stays ours and `to_save` writes it into
//! the save's memory copy.
use crate::hud::{HudState, HudTri, Ui};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// One achievement: DE's Enums.Achievements id, its Steam API name, our title and condition text, the target of a
/// counted one (AchievementStatus.GetMaxAchievementValue; -1 = unlocked by an event).
pub struct Def { pub id: u8, pub key: &'static str, pub title: &'static str, pub text: &'static str, pub max: i32 }

const fn d(id: u8, key: &'static str, title: &'static str, text: &'static str, max: i32) -> Def { Def { id, key, title, text, max } }

pub const LIST: [Def; 40] = [
    d(1, "Tut_Complete", "Tutorial Complete", "Complete the tutorial.", -1),
    d(11, "Campaign_1", "Campaign 1 Complete", "Complete the first historical campaign.", -1),
    d(12, "Campaign_2", "Campaign 2 Complete", "Complete the second historical campaign.", -1),
    d(13, "Campaign_3", "Campaign 3 Complete", "Complete the third historical campaign.", -1),
    d(14, "Campaign_4", "Campaign 4 Complete", "Complete the fourth historical campaign.", -1),
    d(15, "Campaign_5", "Campaign 5 Complete", "Complete the fifth historical campaign.", -1),
    d(16, "Campaign_6", "Campaign 6 Complete", "Complete the sixth historical campaign.", -1),
    d(17, "Campaign_7", "Campaign 7 Complete", "Complete the seventh historical campaign.", -1),
    d(21, "Trail_1", "Crusader Trail Complete", "Win all 50 missions of the Crusader Trail.", -1),
    d(22, "Trail_2", "Warchest Trail Complete", "Win all 30 missions of the Warchest Trail.", -1),
    d(23, "Trail_3", "Extreme Trail Complete", "Win all 20 missions of the Extreme Trail.", -1),
    d(31, "Sands_Trail_1", "Sands of Time Trail 1", "Finish Sands of Time trail 1 at Warrior rank or better, no chicken.", -1),
    d(32, "Sands_Trail_2", "Sands of Time Trail 2", "Finish Sands of Time trail 2 at Warrior rank or better, no chicken.", -1),
    d(33, "Sands_Trail_3", "Sands of Time Trail 3", "Finish Sands of Time trail 3 at Warrior rank or better, no chicken.", -1),
    d(34, "Sands_Trail_4", "Sands of Time Trail 4", "Finish Sands of Time trail 4 at Warrior rank or better, no chicken.", -1),
    d(35, "Sands_Trail_5", "Sands of Time Trail 5", "Finish Sands of Time trail 5 at Warrior rank or better, no chicken.", -1),
    d(36, "Sands_Trail_6", "Sands of Time Trail 6", "Finish Sands of Time trail 6 at Warrior rank or better, no chicken.", -1),
    d(37, "Sands_Trail_7", "Sands of Time Trail 7", "Finish Sands of Time trail 7 at Warrior rank or better, no chicken.", -1),
    d(41, "Kill_10k_Units", "10,000 Kills", "Kill 10,000 units.", 10_000),
    d(42, "Kill_100k_Units", "100,000 Kills", "Kill 100,000 units.", 100_000),
    d(43, "Kill_1M_Units", "1,000,000 Kills", "Kill 1,000,000 units.", 1_000_000),
    d(51, "Sands_Warrior", "Sands Warrior", "Win a Sands of Time mission at Warrior rank.", -1),
    d(52, "Sands_Champion", "Sands Champion", "Win a Sands of Time mission at Champion rank.", -1),
    d(53, "Sands_Prince", "Sands Prince", "Win a Sands of Time mission at Prince rank.", -1),
    d(61, "Win_Skirmish_Game", "Skirmish Victory", "Win a skirmish game.", -1),
    d(62, "Win_Skirmish_Game_vs_7", "Victory vs 7", "Win a skirmish against 7 AI lords.", -1),
    d(63, "Win_Skirmish_Game_vs_Team_of_7", "Victory vs a Team of 7", "Win a skirmish against a team of 7 AI lords.", -1),
    d(64, "Win_Skirmish_Game_vs_New_Lords", "Victory vs the New Lords", "Win a skirmish against a team of four or more holding all four new lords.", -1),
    d(65, "Skirmish_Beating_All_Lords", "Beat Every Lord", "Beat each of the 20 AI lords in a won skirmish.", -1),
    d(66, "Win_Skirmish_No_Ranged", "Victory Without Ranged", "Win a skirmish with melee troops only.", -1),
    d(67, "Win_Skirmish_All_Ranged", "Victory With Ranged Only", "Win a skirmish with ranged troops only.", -1),
    d(91, "Upload_Map", "Workshop Upload", "Upload a map to the Steam Workshop (not available in this port).", -1),
    d(92, "Scribe_Unlock", "Scribe Unlocked", "Sign up to the newsletter (not available in this port).", -1),
    d(113, "Kill_1000_Lions", "1,000 Lions", "Kill 1,000 lions.", 1000),
    d(114, "Store_Food_1000", "Store 1,000 Food", "Store 1,000 food in one game.", 1000),
    d(115, "Store_Weapons_1000", "Store 1,000 Weapons", "Store 1,000 weapons in one game.", 1000),
    d(116, "Store_Wood_10000", "Store 10,000 Wood", "Store 10,000 wood in one game.", 10_000),
    d(117, "Amass_Gold_10000", "Amass Gold", "Hold 50,000 gold at once.", 50_000),
    d(118, "Population_300", "Population 300", "Reach a population of 300.", 300),
    d(119, "Dairy_Farms", "30 Dairy Farms", "Place 30 dairy farms.", 30),
];

/// The local stand-in for Steam: per achievement its value and whether it is done; `lords` = Platform_Achievements'
/// STAT_Beat_Lord_* as bits 1..20.
#[derive(Clone, Debug, PartialEq)]
struct State { value: [i32; 40], done: [bool; 40], lords: u32, dirty: bool, saved: Option<Instant> }

static S: Mutex<Option<State>> = Mutex::new(None);
/// Unlocks to show in game (`toast`): LIST index, shown since (None = queued).
static TOASTS: Mutex<Vec<(usize, Option<Instant>)>> = Mutex::new(Vec::new());
/// The running game's win was counted (GameData.ManageGameOver runs once).
static WON: AtomicBool = AtomicBool::new(false);

fn file() -> std::path::PathBuf { crate::skirmish::profile_dir().join("achievements.cfg") }
fn idx(id: u8) -> Option<usize> { LIST.iter().position(|a| a.id == id) }

impl State {
    /// achievements.cfg: `Key:value:done` per line (done 1 / 0) and `Lords:bits`.
    fn parse(text: &str) -> Self {
        let mut s = State { value: [0; 40], done: [false; 40], lords: 0, dirty: false, saved: None };
        for l in text.lines() {
            let mut f = l.split(':');
            match (f.next(), f.next(), f.next()) {
                (Some("Lords"), Some(v), _) => s.lords = v.trim().parse().unwrap_or(0),
                (Some(k), Some(v), done) => if let Some(i) = LIST.iter().position(|a| a.key == k) {
                    s.value[i] = v.trim().parse().unwrap_or(0);
                    s.done[i] = done.is_some_and(|d| d.trim() == "1");
                },
                _ => {}
            }
        }
        s
    }
    fn text(&self) -> String {
        let mut t: String = LIST.iter().enumerate().map(|(i, a)| format!("{}:{}:{}\n", a.key, self.value[i], self.done[i] as u8)).collect();
        t.push_str(&format!("Lords:{}\n", self.lords));
        t
    }
    /// AchievementStatus.SetComplete.
    fn complete(&mut self, i: usize) -> bool {
        if self.done[i] || LIST[i].id == 91 || LIST[i].id == 92 { return false; }
        (self.done[i], self.dirty) = (true, true);
        true
    }
    /// AchievementStatus.AddValue / SetValue (`add` false: only a higher value counts): done at the target.
    fn count(&mut self, i: usize, v: i32, add: bool) -> bool {
        if self.done[i] { return false; }
        let n = if add { self.value[i].saturating_add(v) } else if v > self.value[i] { v } else { return false };
        (self.value[i], self.dirty) = (n, true);
        LIST[i].max > 0 && n >= LIST[i].max && self.complete(i)
    }
}

/// Runs `f` on the loaded state; a new unlock is toasted and saved at once.
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = S.lock().unwrap();
    let s = g.get_or_insert_with(|| State::parse(&std::fs::read_to_string(file()).unwrap_or_default()));
    let before = s.done;
    let r = f(s);
    let new: Vec<usize> = (0..LIST.len()).filter(|&i| s.done[i] && !before[i]).collect();
    if !new.is_empty() {
        save(s);
        TOASTS.lock().unwrap().extend(new.into_iter().map(|i| (i, None)));
    }
    r
}

fn save(s: &mut State) {
    let path = file();
    if let Some(d) = path.parent() { let _ = std::fs::create_dir_all(d); }
    match crate::paths::write_atomic(&path, s.text()) {
        Ok(()) => (s.dirty, s.saved) = (false, Some(Instant::now())),
        Err(e) => eprintln!("achievements.cfg: {e}"),
    }
}

/// AchievementsCommon.CompleteAchievement.
pub fn complete(id: u8) { if let Some(i) = idx(id) { with(|s| s.complete(i)); } }

#[cfg(test)]
/// Done yet.
pub fn done(id: u8) -> bool { idx(id).is_some_and(|i| with(|s| s.done[i])) }

/// Each achievement's (done, value), in `LIST` order (the Extras page).
pub fn snapshot() -> Vec<(bool, i32)> { with(|s| s.done.iter().copied().zip(s.value).collect()) }

/// AchievementsCommon.UpdateValue: an engine message (render record 24: `msg` at +17, `v` at +18) or GameData's
/// per-frame gold level (4) and population (10).
pub fn message(msg: i32, v: i32) {
    let i = |id| idx(id).unwrap();
    with(|s| match msg {
        1 => { s.count(i(116), v, true); }
        2 => { s.count(i(114), v, true); }
        3 => { s.count(i(115), v, true); }
        4 => { s.count(i(117), v, false); }
        5 => { s.count(i(113), v, true); }
        7 => for id in [41, 42, 43] { s.count(i(id), v, true); },
        8 => { s.count(i(119), v, true); }
        10 => { s.count(i(118), v, false); }
        _ => {}
    });
}

/// AchievementsCommon.ResetOnMissionStart (a game starts): the per-game counters back to 0; the new game's win not
/// counted yet. Counters gathered since the last save are written.
pub fn mission_start() {
    WON.store(false, Ordering::Relaxed);
    with(|s| {
        for id in [116, 114, 115, 117, 118] { let i = idx(id).unwrap(); if !s.done[i] { s.value[i] = 0; } }
        if s.dirty { save(s) }
    });
}

/// The store counters as DLL_SetAchValues leaves them in the engine (food 0x183663fe0, weapons +2, wood +4):
/// into a save's memory copy (saves.rs) - StoreValuesInEngine without touching the running sim.
#[cfg(feature = "live")]
pub fn to_save(m: &mut shc_sim::mem::Mem) {
    with(|s| for (id, a) in [(114, 0x183663fe0), (115, 0x183663fe2), (116, 0x183663fe4)] { m.set_i16(a, s.value[idx(id).unwrap()] as i16) });
}

/// A loaded save's store counters (header block 4, at 0x183663fe0 in the loaded memory): food, weapons, wood; None on
/// a multiplayer map (0x183666170, the LoadMapReturnData's multiplayerMap). Read before the game starts (`save_loaded`).
#[cfg(feature = "live")]
pub fn saved_counters(m: &shc_sim::mem::Mem) -> Option<[i32; 3]> {
    (m.i32(0x183666170) == 0).then(|| [0, 2, 4].map(|o| m.i16(0x183663fe0 + o) as i32))
}

/// UpdateAfterLoadingASave (EditorDirector's load, after the game start's reset): the save's food / weapons / wood
/// counters back (`saved_counters`); a multiplayer map's, gold and population stay at 0. ponytail: DE's SetValue keeps
/// a counter above the save's (left over from the game before); ours is the save's.
#[cfg(feature = "live")]
pub fn save_loaded(c: Option<[i32; 3]>) {
    mission_start();
    if let Some(c) = c { with(|s| for (id, v) in [114, 115, 116].into_iter().zip(c) { s.count(idx(id).unwrap(), v, false); }); }
}

/// A live game's frame (main.rs): the engine's achievement messages, the gold and population (GameData.UpdateGameState),
/// a won skirmish-mode game (`skirmish`: a lobby or trail game, not co-op / LAN) once its statistics are captured.
/// Counters are written at most every 10 s.
pub fn frame(recs: &[crate::replay::Rec], st: &HudState, skirmish: bool) {
    for r in recs.iter().filter(|r| r[0] == 24) { message(r[17] as i32, r[18] as i32) }
    message(4, st.gold);
    message(10, st.population);
    #[cfg(feature = "live")]
    if skirmish && st.end.as_ref().is_some_and(|e| e.result == 1) && !WON.load(Ordering::Relaxed) {
        if let Some(e) = crate::menu::trail_menu::masters::peek() {
            WON.store(true, Ordering::Relaxed);
            let i = |k: &str| e.ints(k).into_iter().map(|v| v as i32).collect::<Vec<_>>();
            skirmish_won(&i("valid"), &i("computer_register"), &i("teams"), e.int("ranged_made") as i32, e.int("melee_made") as i32);
        }
    }
    #[cfg(not(feature = "live"))]
    let _ = skirmish;
    with(|s| if s.dirty && s.saved.map_or(true, |t| t.elapsed().as_secs() >= 10) { save(s) });
}

/// GameData.ManageGameOver's skirmish branch (coopTrailID 0) from MPScoreData: Win_Skirmish_Game, the all-ranged /
/// no-ranged wins, the AI lords beaten (setLordKilledStat, all 20 → Skirmish_Beating_All_Lords), 7 enemies (one team),
/// a team of 4+ with the four new lords (17..20).
pub fn skirmish_won(valid: &[i32], cr: &[i32], teams: &[i32], ranged: i32, melee: i32) {
    let at = |v: &[i32], k: usize| v.get(k).copied().unwrap_or(0);
    complete(61);
    if ranged == 1 && melee == 0 { complete(67); }
    if ranged == 0 && melee == 1 { complete(66); }
    let me = (1..9).find(|&k| at(valid, k) > 0 && at(cr, k) <= 0).map_or(-1, |k| at(teams, k));
    let enemies: Vec<usize> = (1..9).filter(|&k| at(valid, k) > 0 && at(cr, k) > 0 && at(teams, k) != me).collect();
    let first = enemies.first().map(|&k| at(teams, k));
    let same = enemies.iter().filter(|&&k| Some(at(teams, k)) == first).count();
    let new_lords = |ks: &mut dyn Iterator<Item = usize>| { let mut m = 0u8; for k in ks { if let 17..=20 = at(cr, k) { m |= 1 << (at(cr, k) - 17) } } m == 15 };
    with(|s| {
        for &k in enemies.iter().filter(|&&k| k != 1) { if let l @ 1..=20 = at(cr, k) { s.lords |= 1 << l; } }
        if s.lords & 0x1f_fffe == 0x1f_fffe { s.complete(idx(65).unwrap()); }
    });
    if same >= 4 && new_lords(&mut enemies.iter().copied()) && new_lords(&mut enemies.iter().copied().filter(|&k| Some(at(teams, k)) == first)) { complete(64); }
    if enemies.len() == 7 {
        complete(62);
        if same == 7 { complete(63); }
    }
}

/// A campaign mission won (ManageGameOver): the last of a campaign (5, 10 .. 35) completes it.
pub fn campaign_won(mission: i32) { if mission % 5 == 0 && (5..=35).contains(&mission) { complete(10 + (mission / 5) as u8) } }

/// A trail mission won (GameData's trail branch, trail_menu::record_win): a classic trail with every mission done;
/// a timed Sands mission's rank (`rank`, TestSandsRankAchievement: 4 Prince .. 2 Warrior) and its trail's total rank
/// (`trail_rank`, all missions timed, no chicken) at Warrior or better.
pub fn trail_won(trail: i32, all_done: bool, rank: Option<i32>, trail_rank: Option<i32>) {
    if all_done && (0..=2).contains(&trail) { complete(21 + trail as u8); }
    for (r, id) in [(4, 53), (3, 52), (2, 51)] { if rank.is_some_and(|k| k >= r) { complete(id) } }
    if (11..=17).contains(&trail) && trail_rank.is_some_and(|r| (2..=4).contains(&r)) { complete(20 + trail as u8); }
}

/// The unlock toast (ours, in the HUD's top centre): "Achievement unlocked" and its title for 5 s, one after another.
pub fn toast(ui: &mut Ui, out: &mut Vec<HudTri>) {
    let mut q = TOASTS.lock().unwrap();
    if q.first().and_then(|t| t.1).is_some_and(|t| t.elapsed().as_secs_f32() >= 5.0) { q.remove(0); }
    let Some(t) = q.first_mut() else { return };
    let since = t.1.get_or_insert_with(Instant::now).elapsed().as_secs_f32();
    let def = &LIST[t.0];
    drop(q);
    let a = (since * 4.0).min(1.0).min((5.0 - since) * 4.0).max(0.0); // 0.25 s fade in / out
    let (s, cx) = (ui.s, ui.w / 2.0);
    let (w, h, y) = (440.0 * s, 70.0 * s, 70.0 * s);
    let p = [cx - w / 2.0, y, cx + w / 2.0, y + h];
    Ui::fill(out, [p[0] - 2.0 * s, p[1] - 2.0 * s, p[2] + 2.0 * s, p[3] + 2.0 * s], [0.957, 0.871, 0.667, 0.8 * a]); // #F4DEAA edge
    Ui::fill(out, p, [0.0, 0.0, 0.0, 0.85 * a]);
    ui.text_sh(out, "Achievement unlocked", cx, y + 26.0 * s, 16.0, 0.5, [0.867, 0.867, 0.867, a]);
    ui.text_sh(out, def.title, cx, y + 56.0 * s, 24.0, 0.5, [0.957, 0.871, 0.667, a]);
}

#[test]
fn achievements_count_save_and_unlock() {
    crate::skirmish::set_test_profile(std::env::temp_dir().join(format!("shc-ach-{}", std::process::id())));
    *S.lock().unwrap() = None;
    let _ = std::fs::remove_file(file());
    TOASTS.lock().unwrap().clear();
    message(7, 9_999);
    assert!(!done(41));
    message(7, 1); // Unit_Killed_By_Player: 10k reached
    assert!(done(41) && !done(42));
    message(10, 299);
    message(10, 120); // SetValue: only a higher value counts
    assert!(!done(118));
    message(10, 300);
    assert!(done(118));
    campaign_won(9);
    campaign_won(10);
    assert!(done(12) && !done(11));
    trail_won(12, true, Some(3), Some(1));
    assert!(done(52) && done(51) && !done(53) && !done(32), "Champion implies Warrior; the trail's rank below Warrior");
    complete(91);
    assert!(!done(91), "Workshop upload: never in this port");
    assert_eq!(TOASTS.lock().unwrap().len(), 5);
    // kept on disk (atomic write), read back fresh
    *S.lock().unwrap() = None;
    assert!(done(41) && done(12));
    assert_eq!(with(|s| s.value[idx(42).unwrap()]), 10_000);
    // a 7-AI skirmish win, one team, the four new lords among them
    let valid = [0, 1, 1, 1, 1, 1, 1, 1, 1];
    let cr = [0, 0, 17, 18, 19, 20, 1, 2, 3];
    let teams = [0, 1, 2, 2, 2, 2, 2, 2, 2];
    skirmish_won(&valid, &cr, &teams, 0, 1);
    for id in [61, 62, 63, 64, 66] { assert!(done(id), "{id}"); }
    assert!(!done(67) && !done(65));
    mission_start();
    assert_eq!(with(|s| s.value[idx(118).unwrap()]), 300, "done ones keep their value");
    // the store counters survive a save / load (DE: DLL_SetAchValues -> save header block 4 -> UpdateAfterLoadingASave)
    #[cfg(feature = "live")] {
    let v = |id| with(|s| s.value[idx(id).unwrap()]);
    message(2, 400); // food
    message(1, 7_000); // wood
    message(3, 250); // weapons
    let mut m = shc_sim::mem::Mem::default();
    to_save(&mut m);
    mission_start();
    assert_eq!((v(114), v(116), v(115)), (0, 0, 0));
    save_loaded(saved_counters(&m));
    assert_eq!((v(114), v(116), v(115)), (400, 7_000, 250));
    message(2, 600);
    assert!(done(114), "1,000 food over the load");
    m.set_i32(0x183666170, 1); // a multiplayer map: not restored
    save_loaded(saved_counters(&m));
    assert_eq!((v(116), v(115)), (0, 0));
    }
}
