//! New skirmish without the game's front end: the lobby choices become the calls the front end makes
//! (DLL_PreInitMap_Multiplayer, DLL_RegisterMultiplayerUser / RegisterSkirmishUser, DLL_ImportAIV,
//! DLL_ApplyMultiplayerSetupData), then the sim's native `setup::new_skirmish` (DLL_LoadMapToPlay + DLL_SetMPRandSeed).
//! Values the lobby doesn't offer are the ones every recorded skirmish sends (formats/examples/setup_blobs.rs).
//! Maps are read in place from the game install; AIV castle layouts from aiv.tsv (AIVLoader.cs, the arrays the
//! game's C# hands to DLL_ImportAIV, carved from the user's Assembly-CSharp.dll by the first-run setup).
use std::path::{Path, PathBuf};


/// The game's maps; SHC_MAPS overrides (the .app's own copy).
pub fn maps_dir() -> PathBuf {
    std::env::var_os("SHC_MAPS").map(PathBuf::from).unwrap_or_else(|| crate::paths::streaming_assets().join("Maps"))
}
/// Where Save Game writes and Load Game looks first (the .app sets SHC_SAVES to its own folder).
pub fn saves_dir() -> PathBuf { std::env::var_os("SHC_SAVES").map(PathBuf::from).unwrap_or_else(crossover_saves) }
/// The real game's save folder (macOS: CrossOver bottle "Steam"; Linux: the Proton prefix; paths.rs).
pub fn crossover_saves() -> PathBuf { crate::paths::game_user().join("Saves") }

/// Files in `dir` ending in `ext`, sorted by name (empty if the folder is missing).
pub fn list(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == ext)).collect();
    v.sort();
    v
}

/// The header blocks of a .map / .sav (before the section directory), read without unpacking the sections.
fn header_blocks(map: &Path) -> Option<Vec<Vec<u8>>> {
    use std::io::Read;
    fn int(f: &mut impl Read) -> Option<i32> { let mut b = [0u8; 4]; f.read_exact(&mut b).ok().map(|_| i32::from_le_bytes(b)) }
    let mut f = std::io::BufReader::new(std::fs::File::open(map).ok()?);
    if int(&mut f)? >= 0 { return None; }
    let mut blocks = vec![];
    for _ in 0..6 {
        let len = int(&mut f)?;
        if len <= 0 { break; }
        let mut b = vec![0; len as usize];
        f.read_exact(&mut b).ok()?;
        blocks.push(b);
    }
    Some(blocks)
}

/// What the lobby shows about a map (MapFileManager.GetFileInfoFromFileName): header block 3 = mapType,
/// mapKeeps[5], maxPlayers; block 5 = isKingOfTheHill (one position less), skirmishMap, xPlayAutoSave, balanced (0 =
/// yes), keep_locations[8] (x, y on the 200 px radar), world_size, chimps / flies limits, extreme powers, hasOutposts.
#[derive(Clone, Debug, PartialEq)]
pub struct MapInfo { pub players: i32, pub size: i32, pub balanced: bool, pub outposts: bool, pub keeps: [(i32, i32); 8] }

/// None = not a skirmish / multiplayer map (mapType != 1; negative = a trail map, also 1).
pub fn map_info(map: &Path) -> Option<MapInfo> {
    let b = header_blocks(map)?;
    let int = |k: usize, o: usize| b.get(k).and_then(|b| b.get(o..o + 4)).map(|v| i32::from_le_bytes(v.try_into().unwrap()));
    let ty = int(3, 0)?;
    if ty != 1 && ty >= 0 { return None; }
    let keeps = std::array::from_fn(|i| (int(5, 16 + i * 8).unwrap_or(-1), int(5, 20 + i * 8).unwrap_or(-1)));
    Some(MapInfo { players: int(3, 24)? - (int(5, 0)? > 0) as i32, size: int(5, 80).unwrap_or(0), balanced: int(5, 12) == Some(0),
        outposts: int(5, 96).unwrap_or(0) != 0, keeps })
}

/// A single-player map for FRONT_StandaloneMission: `size` = world_size (header block 5 +80), `animals` = hostileAnimals
/// (block 5 +100, when present), `user` = from a user Maps folder.
#[derive(Clone, Copy, Debug)]
pub struct SoloInfo { pub size: i32, pub animals: i32, pub user: bool }

/// MapFileManager's single-player lists: maps whose header scnMissionSiegeOrInvasion (block 4 +4) is `kind`
/// (3 = Free Build, 1 = Invasion / Custom Scenarios), not multiplayer or trail (mapType 1 / < 0), not a campaign mission
/// or the tutorial (missionLockType 2 or the name), 35 kB..9 MB. User maps (the app's and the real game's Maps
/// folders) first, then the built-in ones, each by file name. ponytail: no Workshop maps.
pub fn solo_maps(kind: i32, user: bool, builtin: bool) -> Vec<(PathBuf, SoloInfo)> {
    let dirs = user_map_dirs().into_iter().filter(|_| user).map(|d| (d, true)).chain(builtin.then(|| (maps_dir(), false)));
    dirs.flat_map(|(d, u)| list(&d, "map").into_iter().map(move |p| (p, u))).filter_map(|(p, user)| {
        let stem = p.file_stem()?.to_string_lossy().to_lowercase();
        if stem.starts_with("crusaders_mission") || stem.starts_with("crusader_tutorial") { return None; }
        if !(35_000..=9_000_000).contains(&std::fs::metadata(&p).ok()?.len()) { return None; }
        let b = header_blocks(&p)?;
        let int = |k: usize, o: usize| b.get(k).and_then(|b| b.get(o..o + 4)).map(|v| i32::from_le_bytes(v.try_into().unwrap()));
        let ty = int(3, 0)?;
        if ty == 1 || ty < 0 || int(4, 4)? != kind || int(4, 8) == Some(2) { return None; }
        Some((p, SoloInfo { size: int(5, 80).unwrap_or(-1), animals: int(5, 100).unwrap_or(0), user }))
    }).collect()
}

/// The user Maps folders (MapFileManager's user maps): the app's and the real game's.
pub fn user_map_dirs() -> [PathBuf; 2] { [profile_dir().join("Maps"), crossover_saves().with_file_name("Maps")] }

/// Free Build options (FRONT_StandaloneMission → EngineInterface.loadJustBuildMap), fresh-open defaults.
/// `goods` gold / food / resources / weapons −1 (map default)..5; `events`, `invasions` 0 (off)..8; `invasion_difficulty`
/// 0..8; `peacetime` 0..6 (4 = 20 min); `opponents` bits 1 Crusader, 2 Arab, 4 Bedouin (never 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeBuild {
    pub advanced: bool, pub goods: [i32; 4], pub events: i32, pub invasions: i32, pub invasion_difficulty: i32, pub peacetime: i32,
    pub opponents: i32, pub remove_animals: bool, pub extreme_troops: bool, pub extreme_powers: bool, pub defeat_on_death: bool,
}
impl Default for FreeBuild {
    fn default() -> Self {
        FreeBuild { advanced: false, goods: [-1; 4], events: 0, invasions: 0, invasion_difficulty: 0, peacetime: 4, opponents: 7,
            remove_animals: false, extreme_troops: false, extreme_powers: false, defeat_on_death: false }
    }
}

/// A built-in map's display name and description (Translate.translateMapNames, re/data/map_names.tsv) as string
/// keys; None = a user map (shown by file name).
pub fn map_text_keys(map: &Path) -> Option<(String, String)> {
    let stem = map.file_stem()?.to_string_lossy().to_lowercase();
    let row = include_str!("../../re/data/map_names.tsv").lines().filter(|l| !l.starts_with('#')).map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|r| r.len() == 3 && r[0].to_lowercase() == stem)?;
    let (id, desc): (i32, i32) = (row[1].parse().ok()?, row[2].parse().ok()?);
    // lookUpText is 0-based: key suffix = id + 1
    Some(if id < 0 { (format!("TEXT_TRAIL_NAMES_CRU_{:03}", 1 - id), if desc < 0 { String::new() } else { format!("TEXT_MAP_NAMES_{:03}", desc + 1) }) }
        else { (format!("TEXT_MAP_NAMES_{:03}", id + 1), format!("TEXT_MAP_NAMES_{:03}", id + 2)) })
}

/// Start positions (`map_info` players); None = not a skirmish map.
pub fn start_positions(map: &Path) -> Option<i32> { map_info(map).map(|i| i.players) }

/// The 200×200 radar preview (header block 0) as RGBA, top row first.
pub fn map_preview(map: &Path) -> Option<Vec<u8>> {
    shc_formats::map::MapFile { version: -2, header_blocks: header_blocks(map)?, sections: vec![], broken: vec![] }.preview_rgba()
}

/// The briefing (header block 1, packed mission text: ANSI, or UTF-16 when every other byte is 0).
pub fn map_briefing(map: &Path) -> Option<String> {
    let raw = shc_formats::map::unpack(header_blocks(map)?.get(1)?.get(8..)?, -1).ok()?; // after i32 pair
    let wide = raw.len() > 1 && raw[1] == 0;
    let s = if wide { String::from_utf16_lossy(&raw.chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect::<Vec<_>>()) }
        else { raw.iter().map(|&c| c as char).collect() };
    Some(s.trim_end_matches('\0').trim().to_string()).filter(|s| !s.is_empty())
}

// ---- campaigns (Historical Campaigns): progress as the original keeps it, and the mission start

/// The app's own player folder (campaign progress): SHC_PROFILE, else ~/Library/Application Support/Strong Hold.
pub fn profile_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(p) = TEST_PROFILE.with_borrow(|p| p.clone()) { return p; }
    std::env::var_os("SHC_PROFILE").map(PathBuf::from).unwrap_or_else(crate::paths::app_data)
}

/// Tests: this thread's player folder. SHC_PROFILE is process-wide, so parallel tests setting it raced on each
/// other's progress / settings files.
#[cfg(test)]
pub fn set_test_profile(p: impl Into<PathBuf>) { TEST_PROFILE.set(Some(p.into())); crate::menu::reset_cfg() }
#[cfg(test)]
thread_local! { static TEST_PROFILE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) }; }

/// Campaign progress like ConfigSettings / missions.cfg: `unlocked[c]` = highest unlocked mission of campaign c + 1
/// (settings.cfg `Campaign1`..`Campaign7`, 1..5, default 1); `done` = mission id -> best difficulty won (missions.cfg
/// `mission<id>:<d>`). The original keeps "campaign complete" as a Steam achievement; here = mission 5 of it won.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress { pub unlocked: [i32; 7], pub done: std::collections::BTreeMap<i32, i32> }

impl Default for Progress {
    fn default() -> Self { Progress { unlocked: [1; 7], done: Default::default() } }
}

/// settings.cfg like ConfigSettings.SaveSettings / loadSettingsFromString: the `||SETTINGS||` block's `Key:Value`
/// lines in file order (keys compared ignoring case), then the `||KEYS||` hotkey block kept verbatim. Writers merge
/// (set the keys they own, keep the rest), so campaign progress and the Options screen share one file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings { pub lines: Vec<(String, String)>, pub keys: String }

impl Settings {
    pub fn parse(text: &str) -> Self {
        let mut parts = text.split("||SETTINGS||\n");
        let (_, body, rest) = (parts.next(), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let lines = body.lines().filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
        Settings { lines, keys: rest.to_string() }
    }
    /// `dir`/settings.cfg; None = no file there.
    pub fn read(dir: &Path) -> Option<Self> { std::fs::read_to_string(dir.join("settings.cfg")).ok().map(|t| Self::parse(&t)) }
    /// The app's settings; on the first run the real game's (CrossOver LocalLow folder), else empty (= defaults).
    pub fn load() -> Self {
        Self::read(&profile_dir()).or_else(|| crossover_saves().parent().and_then(Self::read)).unwrap_or_default()
    }
    pub fn get(&self, key: &str) -> Option<&str> { self.lines.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str()) }
    pub fn set(&mut self, key: &str, value: impl ToString) {
        match self.lines.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some(l) => l.1 = value.to_string(),
            None => self.lines.push((key.to_string(), value.to_string())),
        }
    }
    /// A bool ("True" / "False"), `default` when missing or unreadable.
    pub fn flag(&self, key: &str, default: bool) -> bool { self.get(key).map_or(default, |v| v.eq_ignore_ascii_case("true")) }
    pub fn num(&self, key: &str, default: f32) -> f32 { self.get(key).and_then(|v| v.parse().ok()).unwrap_or(default) }
    pub fn text(&self) -> String {
        let body: String = self.lines.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
        format!("||SETTINGS||\n{body}||SETTINGS||\n{}", self.keys)
    }
    /// Into the app's folder.
    pub fn save(&self) -> std::io::Result<()> {
        let dir = profile_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("settings.cfg"), self.text())
    }
    /// ConfigSettings.Settings_GameSpeed: ticks per second, `/5*5` clamped to 10..90, default 40.
    pub fn game_speed(&self) -> f32 { ((self.num("GameSpeed", 40.0) as i32 / 5 * 5).clamp(10, 90)) as f32 }
    /// The volume sliders (0..1; master 0.8, the rest 1) and ReduceMusicVolumeForSpeech, for `audio::set_volumes`.
    pub fn volumes(&self) -> (f32, f32, f32, f32, f32, bool) {
        let v = |k: &str, d: f32| self.num(k, d).clamp(0.0, 1.0);
        (v("MasterVolume", 0.8), v("MusicVolume", 1.0), v("SpeechVolume", 1.0), v("UnitSpeechVolume", 1.0), v("SFXVolume", 1.0), self.flag("ReduceMusicVolumeForSpeech", true))
    }
}

impl Progress {
    /// From `dir`'s settings.cfg + missions.cfg (None = no settings.cfg there).
    pub fn read(dir: &Path) -> Option<Self> {
        let cfg = Settings::read(dir)?;
        let mut p = Progress::default();
        for (k, v) in &cfg.lines {
            let c = k.to_lowercase().strip_prefix("campaign").and_then(|c| c.parse::<usize>().ok());
            if let Some(c) = c.filter(|c| (1..=7).contains(c)) { p.unlocked[c - 1] = v.parse::<i32>().unwrap_or(1).clamp(1, 5); }
        }
        for (k, v) in std::fs::read_to_string(dir.join("missions.cfg")).unwrap_or_default().lines().filter_map(|l| l.split_once(':')) {
            if let (Some(id), Ok(d)) = (k.trim().strip_prefix("mission").and_then(|i| i.parse().ok()), v.trim().parse()) { p.done.insert(id, d); }
        }
        Some(p)
    }
    /// The app's progress; on the first run the real game's (CrossOver LocalLow folder), else a fresh install.
    pub fn load() -> Self {
        Self::read(&profile_dir()).or_else(|| crossover_saves().parent().and_then(Self::read)).unwrap_or_default()
    }
    /// Into the app's folder, in the original's formats: the Campaign keys merged into settings.cfg, missions.cfg.
    pub fn save(&self) -> std::io::Result<()> {
        let mut cfg = Settings::load();
        for (c, v) in self.unlocked.iter().enumerate() { cfg.set(&format!("Campaign{}", c + 1), v); }
        cfg.save()?;
        std::fs::write(profile_dir().join("missions.cfg"), self.done.iter().map(|(id, d)| format!("mission{id}:{d}\n")).collect::<String>())
    }
    /// GameData's victory rule: winning the highest unlocked mission of a campaign unlocks the next (1..4); the best
    /// difficulty per mission is kept.
    pub fn won(&mut self, id: i32, difficulty: i32) {
        let (c, m) = (((id - 1) / 5) as usize, (id - 1) % 5 + 1);
        if m < 5 && self.unlocked[c] == m { self.unlocked[c] += 1; }
        let d = self.done.entry(id).or_insert(difficulty);
        *d = (*d).max(difficulty);
    }
    /// Campaign c (0..7) finished: its fifth mission won.
    pub fn complete(&self, c: usize) -> bool { self.done.contains_key(&(5 * c as i32 + 5)) }
}

/// EngineInterface's map loads end with DLL_GetColourMapping(.., PlayerColour + 1): the map's colour table and your
/// colour id, which BuildPlayerColourMapping swaps with player 1 outside skirmish / MP (`replay::colours`, the radar).
#[cfg(feature = "live")]
fn your_colour(m: &mut shc_sim::mem::Mem) { shc_sim::setup::colour_mapping(m, (Settings::load().num("PlayerColour", 0.0) as i32).clamp(0, 7) + 1); }

/// Campaign mission `id` (1..35, (campaign - 1) * 5 + n) at `difficulty` (0 easy .. 3; the front end loads at 1 and
/// reloads at the briefing's choice): EditorDirector.LoadCampaignMap = DLL_PreInitMap_Campaign + DLL_LoadMapToPlay
/// with the mission's map from the game's Maps folder (`campaign::start_mission`).
#[cfg(feature = "live")]
pub fn start_mission(id: i32, difficulty: i32) -> Result<shc_sim::mem::Mem, String> {
    let mut m = shc_sim::mem::Mem::default();
    shc_sim::terrain::init_static(&mut m);
    let name = shc_sim::campaign::map_name(&m, id).ok_or(format!("no campaign mission {id}"))?;
    let file = std::fs::read(maps_dir().join(&name)).map_err(|e| format!("{name}: {e}"))?;
    if !shc_sim::campaign::start_mission(&mut m, id, difficulty, &file) { return Err(format!("{name} did not load")); }
    your_colour(&mut m);
    Ok(m)
}

/// Sandbox Play (FRONT_StandaloneMission.StartMap): Free Build with `fb` (`campaign::start_free_build` =
/// DLL_PreInitMap_JustBuild + DLL_LoadMapToPlay(-1, path)) or, with `scenario` = Some(difficulty 0..3), a Custom
/// Scenario (loadInvasionMap). A sim panic in an unported path comes back as an error instead of taking the app down.
#[cfg(feature = "live")]
pub fn start_solo(map: &Path, fb: &FreeBuild, scenario: Option<i32>) -> Result<shc_sim::mem::Mem, String> {
    use shc_sim::campaign as c;
    let file = std::fs::read(map).map_err(|e| format!("{}: {e}", map.display()))?;
    let path = map.to_string_lossy();
    let o = c::FreeBuild { advanced: fb.advanced, gold: fb.goods[0], food: fb.goods[1], resources: fb.goods[2], weapons: fb.goods[3],
        random_events: fb.events, invasions: fb.invasions, invasion_difficulty: fb.invasion_difficulty, peacetime: fb.peacetime,
        opponents: fb.opponents, remove_hostile_animals: fb.remove_animals, extreme_troops: fb.extreme_troops,
        extreme_powers: fb.extreme_powers, defeat_on_death: fb.defeat_on_death };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut m = shc_sim::mem::Mem::default();
        shc_sim::terrain::init_static(&mut m);
        let ok = match scenario { Some(d) => c::start_scenario(&mut m, d, &path, &file), None => c::start_free_build(&mut m, &o, &path, &file) };
        ok.then(|| { your_colour(&mut m); m })
    })).map_err(|e| format!("not ported yet: {}", e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()))?
        .ok_or_else(|| format!("{} did not load", map.display()))
}

/// The tutorial (HUD_Tutorial.StartTutorial -> EngineInterface.loadTutorial = DLL_PreInitTutorial + DLL_LoadMapToPlay(-1)
/// with StreamingAssets/Maps/crusader_tutorial.map; `campaign::start_tutorial`).
#[cfg(feature = "live")]
pub fn start_tutorial() -> Result<shc_sim::mem::Mem, String> {
    let map = maps_dir().join("crusader_tutorial.map");
    let file = std::fs::read(&map).map_err(|e| format!("{}: {e}", map.display()))?;
    let mut m = shc_sim::mem::Mem::default();
    shc_sim::terrain::init_static(&mut m);
    if !shc_sim::campaign::start_tutorial(&mut m, &file) { return Err("the tutorial map did not load".into()); }
    your_colour(&mut m); // loadMap's DLL_GetColourMapping(PlayerColour + 1), postLoading's BuildPlayerColourMapping
    Ok(m)
}

/// AI lords in DLL_RegisterSkirmishUser order (= AIVLoader.getAIVData `lord`, checked against recordings).
pub const LORDS: [&str; 27] = ["Rat", "Snake", "Pig", "Wolf", "Saladin", "Caliph", "Sultan", "Richard", "Frederick", "Philip",
    "Wazir", "Emir", "Nizar", "Sheriff", "Marshal", "Abbot", "Jewel", "Sentinel", "Nomad", "Kahinah", "Canary", "Trader",
    "Sergeant", "Lioness", "Crocodile", "Baldwin", "Bullseye"];
/// AIVLoader.cs array per lord (same order).
const AIV_ARRAYS: [&str; 27] = ["rat", "snake", "pig", "wolf", "saladin", "caliph", "sultan", "richard", "frederick", "phillip",
    "wazir", "emir", "nizar", "sheriff", "marshal", "abbot", "jewel", "sentinel", "nomad", "kahinah", "canary", "trader",
    "sergeant", "lioness", "croc", "baldwin", "bullseye"];

/// The lobby. `fairness` = MultiplayerSetupTransferData.fairness 1..5: your / the AI's starting goods
/// 150/50, 125/75, 100/100, 75/125, 50/150 % (tables 0x1802d2f10 / 0x1802d2f28). `goods` = starting_goods_level 1..3
/// (Low / Med / High: the start goods row). `teams`: 0 = none, 1..4.
#[derive(Clone, Debug)]
pub struct Lobby {
    pub map: PathBuf,
    pub lord_kind: i32, // your lord's look (DLL_RegisterMultiplayerUser colour arg -> lord kind): -1 = the map's
    pub team: i32,
    pub ais: Vec<(usize, i32)>, // (lord index into LORDS, team), players 2.. in order
    pub fairness: i32,
    pub goods: i32,
    pub outposts: bool, // allow_outposts (only shown for maps with outposts)
    pub seed: i32,
    /// your colour 0..7 (Options > Player Colour = ConfigSettings.Settings_PlayerColour; colour id + 1)
    pub colour: i32,
    /// extreme_troops, extreme_powers, extreme_powers_around_lord (the lobby's toggles)
    pub extreme: [bool; 3],
    /// advanced_skirmish_options: the Advanced Settings popup's "enable" box (the values below count only with it)
    pub advanced: bool,
    /// the popup's values in `ADV` order (ToStringCustomSkirmish's): 0 / 1, enemy HP 0..3 (66 / 100 / 125 / 150 %)
    pub adv: [i32; 14],
    /// MP_BuildingsAvailable0..12 (barracks, mercenary post, Bedouin stockade, ...: Customize's trail switches)
    pub buildings: [i32; 13],
    /// start_keep_location_order: start position k -> member (0 = you, AI j = j + 1), -10 empty; None = in order
    pub keeps: Option<[i32; 8]>,
    /// per member (player − 1): the trail mission's AIV code (aiv_type: lord AIV list index % 100, rotation / 100),
    /// -1 = all 8 default castles (AIVLoader.UploadDefaultAIV)
    pub aivs: [i32; 8],
    /// Customize on the Extreme trail (DLL_PreInitMap_Multiplayer customisedExtremeTrail: triple start gold)
    pub extreme_trail: bool,
    /// The game mode (ours): 0 = conquest, else King of the Hill to this many points (koth.rs; the sim never sees it)
    pub hill: i32,
}

/// The Advanced Settings rows (FRONT_Multiplayer.xaml, left column then right): MultiplayerSetupTransferData word,
/// label key (None = the enemy HP cycle, TEXT_NEW_CTEXT_413 + percent), `Lobby::adv` index (ToStringCustomSkirmish order).
pub const ADV: [(usize, Option<&str>, usize); 14] = [
    (0x20, None, 8), (0x21, Some("TEXT_NEW_CTEXT_427"), 9), (0x73, Some("TEXT_NEW_CTEXT_548"), 13), (0x18, Some("TEXT_NEW_CTEXT_398"), 0),
    (0x19, Some("TEXT_NEW_CTEXT_400"), 1), (0x1c, Some("TEXT_NEW_CTEXT_402"), 4), (0x1a, Some("TEXT_NEW_CTEXT_404"), 2),
    (0x1b, Some("TEXT_NEW_CTEXT_406"), 3), (0x1d, Some("TEXT_NEW_CTEXT_408"), 5), (0x1e, Some("TEXT_NEW_CTEXT_410"), 6),
    (0x1f, Some("TEXT_NEW_CTEXT_412"), 7), (0x22, Some("TEXT_NEW_CTEXT_435"), 10), (0x23, Some("TEXT_NEW_CTEXT_450"), 11),
    (0x24, Some("TEXT_NEW_CTEXT_452"), 12),
];
/// The defaults (initMultiplayerGame): everything off, enemy HP 100 %.
pub const ADV_DEFAULT: [i32; 14] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0];

impl Lobby {
    pub fn new(map: PathBuf) -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() as i32);
        Lobby { map, lord_kind: -1, team: 0, ais: vec![(0, 0)], fairness: 3, goods: 1, outposts: true, seed, colour: 0, extreme: [false; 3],
            advanced: false, adv: ADV_DEFAULT, buildings: [1; 13], keeps: None, aivs: [-1; 8], extreme_trail: false, hill: 0 }
    }
    /// FRONT_Multiplayer.doOpen(skirmishSetup): your colour from Options, the Advanced Settings remembered
    /// (Settings_SkirmishPresets, the popup's Remember).
    pub fn from_settings(map: PathBuf) -> Self {
        let cfg = Settings::load();
        let mut l = Lobby { colour: cfg.num("PlayerColour", 0.0) as i32, ..Self::new(map) };
        if let Some(p) = cfg.get("SkirmishPresets") { l.set_presets(p); }
        l
    }
    /// MultiplayerSetupData.ToStringCustomSkirmish (version -4).
    pub fn presets(&self) -> String {
        format!("-4,{},{}", self.advanced as i32, self.adv.iter().map(|v| format!("{v},")).collect::<String>())
    }
    /// FromStringCustomSkirmish: older versions (-1..-3) lack the later fields (0 then).
    pub fn set_presets(&mut self, s: &str) {
        let v: Vec<i32> = s.split(',').map(|t| t.trim().parse().unwrap_or(0)).collect();
        let (ver, at) = (v.first().copied().unwrap_or(0), |k: usize| v.get(k).copied().unwrap_or(0));
        self.advanced = at(1) > 0;
        let n = match ver { ..=-4 => 14, -3 => 13, -2 => 11, _ => 10 };
        for k in 0..14 { self.adv[k] = if k < n { at(k + 2) } else { 0 }; }
    }
    /// The members' colour ids 1..8 (you first): yours from Options, each AI the lowest one free
    /// (Platform_Multiplayer.AddSkirmishPlayerLocal).
    pub fn colour_ids(&self) -> Vec<i32> {
        let mut ids = vec![self.colour.clamp(0, 7) + 1];
        for _ in &self.ais {
            let c = (1..9).find(|c| !ids.contains(c)).unwrap_or(1);
            ids.push(c);
        }
        ids
    }
    /// MultiplayerSetupData.advancedSkirmishOptionsEnabled: the "advanced settings on" line under the button.
    pub fn advanced_on(&self) -> bool { self.advanced && self.adv != ADV_DEFAULT }
}

#[test]
fn presets_round_trip() {
    let mut l = Lobby::new(PathBuf::new());
    (l.advanced, l.adv) = (true, [1, 0, 1, 0, 0, 0, 0, 1, 3, 1, 0, 1, 1, 1]);
    let s = l.presets();
    assert_eq!(s, "-4,1,1,0,1,0,0,0,0,1,3,1,0,1,1,1,");
    let mut m = Lobby::new(PathBuf::new());
    m.set_presets(&s);
    assert_eq!((m.advanced, m.adv), (l.advanced, l.adv));
    m.set_presets("-1,1,1,1,1,1,1,1,1,1,2,1,");
    assert_eq!(m.adv, [1, 1, 1, 1, 1, 1, 1, 1, 2, 1, 0, 0, 0, 0], "version -1: no healers, eunuchs, no gold, sieging X");
    // colour 3 (yellow, id 3) for you: the AIs take 1, 2, 4
    let l = Lobby { colour: 2, ais: vec![(0, 0); 3], ..Lobby::new(PathBuf::new()) };
    assert_eq!(l.colour_ids(), [3, 1, 2, 4]);
}

/// The front end's lobby calls as a `Setup` (what `Setup::from_calls` reads from a recording).
#[cfg(feature = "live")]
pub fn setup(l: &Lobby) -> shc_sim::setup::Setup { setup_at(l, 2) }

/// `setup` with the AIs as players `first`.. (a LAN game: after its humans).
#[cfg(feature = "live")]
pub fn setup_at(l: &Lobby, first: i32) -> shc_sim::setup::Setup {
    let cs = shc_sim::data::aivs(); // aiv.tsv, from the first-run setup
    let mut s = shc_sim::setup::Setup { preinit: [1, 0, 0, 0, 0, 0, l.extreme_trail as i32], positions: [-10; 8], aiv_choice: [-1; 8], seed: l.seed, ..Default::default() };
    s.humans.push((1, l.team, true, l.lord_kind));
    let mut rotation = [0; 8];
    for (k, &(lord, team)) in l.ais.iter().enumerate().take(9 - first.clamp(2, 8) as usize) {
        let p = k as i32 + first;
        // a custom lord (FRONT_Multiplayer.StartSkirmishGame): lord type 29 + slot, its .aivjson castles as custom
        // AIVs (none: the parent lord's defaults), then its config (DLL_SetExtendedLordConfig)
        if let Some(c) = crate::custom_lords::of_lord(lord) {
            s.ais.push((p, p + 28, 0, team));
            let layouts = if c.aivs.is_empty() { shc_sim::trail::aiv_layouts(cs, AIV_ARRAYS[crate::custom_lords::parent(&c)]) } else { c.aivs.clone() };
            for (id, a) in layouts.into_iter().enumerate() { s.aivs.push((p - 1, id as i32, (!c.aivs.is_empty()) as i32, a.iter().flat_map(|v| v.to_le_bytes()).collect())); }
            s.lord_configs.push((p, c.config.clone()));
            crate::custom_lords::set_in_game(p as usize, Some(c));
            continue;
        }
        crate::custom_lords::set_in_game(p as usize, None);
        if lord >= LORDS.len() { eprintln!("skirmish: no custom lord {lord} any more, slot {p} left empty"); continue; }
        s.ais.push((p, lord as i32, 0, team));
        let code = l.aivs[p as usize - 1];
        match picked_aiv(cs, lord, code) {
            // a trail mission's castle: that one AIV as a custom upload (EngineInterface.ImportAIV(slot, 0, data, 1))
            Some(a) => { s.aivs.push((p - 1, 0, 1, a.iter().flat_map(|v| v.to_le_bytes()).collect())); rotation[p as usize - 1] = code / 100; }
            None => for (id, a) in shc_sim::trail::aiv_layouts(cs, AIV_ARRAYS[lord]).into_iter().enumerate() {
                s.aivs.push((p - 1, id as i32, 0, a.iter().flat_map(|v| v.to_le_bytes()).collect()));
            },
        }
    }
    // MultiplayerSetupTransferData (464 B, i32 words): fairness, starting_gamespeed, starting_goods_level, win_condition,
    // allow_autotrading, no_knockdown_walls, autosave, peacetime, no_cows, no_dogs, start_keep_location_order0..7,
    // extreme troops / powers / powers around the lord, allow_outposts (0x15), advanced_options,
    // advanced_skirmish_options (0x17), the advopt_* (0x18..=0x24, 0x73), the 70 option switches 0x25.. (buildings
    // 0x25..=0x31, goods, troops) and preferredAIVs0..7 (0x6b..); the rest as recorded
    let mut w = [0i32; 116];
    (w[0], w[1], w[2], w[4], w[6], w[0x15]) = (l.fairness, 40, l.goods, 1, 10, l.outposts as i32);
    for (k, &on) in l.extreme.iter().enumerate() { w[0x12 + k] = on as i32; }
    w[0x14] &= w[0x13]; // around the lord only with the powers on
    w[0x17] = l.advanced as i32;
    for &(word, _, i) in &ADV { w[word] = l.adv[i]; }
    let order = l.keeps.unwrap_or_else(|| std::array::from_fn(|k| if k <= l.ais.len().min(7) { k as i32 } else { -10 }));
    w[10..18].copy_from_slice(&order); // start position k -> member (player − 1)
    for v in &mut w[0x25..0x6b] { *v = 1; }
    w[0x25..0x32].copy_from_slice(&l.buildings);
    for (k, v) in w[0x6b..0x73].iter_mut().enumerate() { *v = -1 - rotation[k]; } // StartSkirmishGame: -1 - AIVs[i].rotation
    s.mp_setup = w.iter().flat_map(|v| v.to_le_bytes()).collect();
    for k in 0..8 { s.positions[k] = w[10 + k] as i8; }
    s
}

/// A trail mission's AIV code (aiv_type) as the castle it names: CustomisationFileManager.getLordAIVList(lord)
/// [code % 100] = the lord's 8 castles, then (lords below 16) Evrey's 8 skirmish ones, then Evrey's historical one.
/// None = -1 (all 8 defaults). ponytail: the historical castle is not in aiv.tsv: the first default instead.
#[cfg(feature = "live")]
fn picked_aiv(cs: &str, lord: usize, code: i32) -> Option<Vec<i16>> {
    if code < 0 { return None; }
    let i = (code % 100) as usize;
    let (set, k) = if (8..16).contains(&i) && lord < 16 { (format!("Evrey_{}", AIV_ARRAYS[lord]), i - 8) } else { (AIV_ARRAYS[lord].to_string(), if i < 8 { i } else { 0 }) };
    shc_sim::trail::aiv_layouts(cs, &set).into_iter().nth(k)
}

/// The engine's radar colour per player (DLL_SetMPRadarColours' array, 0 = no such player): what the front end's
/// BuildMultiPlayerColourMapping makes of the members' colour ids (`Lobby::colour_ids`, through the map's colour
/// table DLL_GetColourMapping hands out). The HUD's score screen and end box read it back (endgame.rs).
pub const RADAR_COLOURS: u64 = 0x1834941fc;

/// A classic trail's colours (EditorDirector.LoadSkirmishTrailMap): DLL_GetColourMapping(PlayerColour + 1) ->
/// remapColours, BuildPlayerColourMapping(PlayerColour + 1, force) swaps player 1 with your colour id, then
/// DLL_SetMPRadarColours of it. `colour` = Options colour 0..7.
#[cfg(feature = "live")]
pub fn trail_colours(m: &mut shc_sim::mem::Mem, colour: i32) {
    let you = colour.clamp(0, 7) as usize + 1;
    let mut c = shc_sim::setup::colour_mapping(m, you as i32);
    c.swap(1, you);
    for (p, v) in c.into_iter().enumerate() { m.set_i32(RADAR_COLOURS + p as u64 * 4, v); }
}

/// BuildMultiPlayerColourMapping without a lobby (a custom trail, FRONT_Multiplayer): colour id PlayerColour + 1 for
/// you (player 1), the lowest free one for each other player present (AddSkirmishPlayerLocal), through the table.
#[cfg(feature = "live")]
pub fn member_colours(m: &mut shc_sim::mem::Mem, colour: i32) {
    let mut ids = vec![colour.clamp(0, 7) + 1];
    for p in 1..9u64 {
        if p > 1 && m.i32(RADAR_COLOURS + p * 4) == 0 { continue; }
        let id = if p == 1 { ids[0] } else { let c = (1..9).find(|c| !ids.contains(c)).unwrap_or(1); ids.push(c); c };
        m.set_i32(RADAR_COLOURS + p * 4, m.i32(0x1802c1ce0 + id as u64 * 4));
    }
}

/// A fresh sim with the skirmish set up, ready for `Live::from_mem`.
#[cfg(feature = "live")]
pub fn start(l: &Lobby) -> Result<shc_sim::mem::Mem, String> {
    // the sim places every player on a map keep; without one it places at (-1, -1) (the game's lobby never offers that)
    let name = l.map.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    match start_positions(&l.map) {
        None => return Err(format!("{name} is not a skirmish map")),
        Some(n) if n < 1 + l.ais.len() as i32 => return Err(format!("{name} has {n} start positions: remove an AI")),
        _ => {}
    }
    let file = std::fs::read(&l.map).map_err(|e| format!("{}: {e}", l.map.display()))?;
    shc_formats::map::parse(&file).map_err(|e| format!("{}: {e:?}", l.map.display()))?;
    let mut m = shc_sim::mem::Mem::default();
    shc_sim::terrain::init_static(&mut m);
    // remap = your colour id (EngineInterface.loadMultiplayerMap: DLL_GetColourMapping(.., PlayerColour + 1); 1 in
    // every recording) and the seed (DLL_SetMPRandSeed)
    shc_sim::setup::new_skirmish(&mut m, &setup(l), &file, l.colour.clamp(0, 7) + 1, Some(l.seed));
    // BuildMultiPlayerColourMapping: player p gets the table's colour of its member's colour id -> SetMPRadarColours
    let table: Vec<i32> = (0..9u64).map(|c| m.i32(0x1802c1ce0 + c * 4)).collect();
    for (k, &c) in l.colour_ids().iter().enumerate().take(8) {
        m.set_i32(RADAR_COLOURS + (k as u64 + 1) * 4, table[c as usize]);
    }
    Ok(m)
}

#[test]
fn aiv_arrays_parse() {
    let cs = shc_sim::data::aivs();
    for name in AIV_ARRAYS { assert_eq!(shc_sim::trail::aiv_layouts(cs, name).len(), 8, "{name}"); }
    assert_eq!(shc_sim::trail::aiv_layouts(cs, "pig")[0].len(), 507); // rec_1792249990: lord 2, DLL_ImportAIV [1, 0, 507, 0]
}

/// A new skirmish (Crater Lake, the map the recordings verify the setup on) starts natively and ticks 100 ticks with its AI lords present.
#[cfg(feature = "live")]
#[test]
fn new_skirmish_ticks() {
    let map = maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    let lobby = Lobby { ais: vec![(0, 0), (2, 0)], seed: 1234, ..Lobby::new(map) };
    let mut l = crate::live::Live::from_mem(start(&lobby).unwrap());
    assert!(l.keep().is_some(), "own keep before the first tick (camera focus)"); // new buildings are state 1 until a tick
    let recs = l.records();
    assert!(recs.len() > 10_000, "{} records", recs.len());
    let t0 = l.tick();
    for _ in 0..100 { l.step(); }
    assert_eq!(l.tick() - t0, 100, "sim did not tick");
    let m = l.mem();
    let lords: Vec<i32> = (0..9).map(|p| m.i32(0x188571c34 + p * 4)).collect(); // section 1100: AI lord + 1
    println!("lords {lords:?}, keep {:?}, tick {}", l.keep(), l.tick());
    assert_eq!(&lords[2..4], &[1, 3], "AI lords registered");
    use shc_sim::buildings as b;
    let m = l.mem();
    let keeps: Vec<i32> = (1..b::SLOTS).filter(|&i| b::state(m, i) == 2 && (0x28..=0x2c).contains(&b::kind(m, i))).map(|i| b::owner(m, i)).collect();
    println!("keeps by owner {keeps:?}");
    for p in 1..=3 { assert!(keeps.contains(&p), "player {p} has no keep: {keeps:?}"); }
    assert!(l.hud().gold > 0);
}


/// Only maps with start positions start (the game's skirmish list); a map with a junk view centre still does.
#[cfg(feature = "live")]
#[test]
fn skirmish_maps_only() {
    let dir = maps_dir();
    if !dir.join("ABridgeApart.map").exists() { return; } // needs the game install
    assert!(start_positions(&dir.join("Crater Lake.map")).is_some_and(|n| n >= 2));
    assert_eq!(start_positions(&dir.join("A Mighty Oasis.map")), None); // no keeps: the sim would place at (-1, -1)
    assert!(start(&Lobby::new(dir.join("A Mighty Oasis.map"))).is_err());
    assert!(start(&Lobby { ais: vec![(0, 0); 2], ..Lobby::new(dir.join("Armenia.map")) }).is_err()); // 2 positions
    // view centre 0xaaaaaa00 in the file: world_post_load reads it back after the clamp
    assert!(start(&Lobby { seed: 1, ..Lobby::new(dir.join("ABridgeApart.map")) }).is_ok());
}


/// Lobby teams reach the sim's team table (0x1837ebed0, per player; players without a team get the next free ids).
#[cfg(feature = "live")]
#[test]
fn teams_reach_sim() {
    let map = maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    let lobby = Lobby { team: 1, ais: vec![(0, 1), (4, 0)], seed: 7, ..Lobby::new(map) };
    let m = start(&lobby).unwrap();
    let t: Vec<i32> = (0..3u64).map(|k| m.i32(0x1837ebed0 + k * 4)).collect();
    assert_eq!(t, [1, 1, 2], "you + Rat allied, Saladin alone");
}

/// Map rotation (Q / E, DLL_SetMapRotation): the next tick turns the map, the step's records carry the new rotation,
/// the view stays on the same centre tile, and four turns come back to the start.
#[cfg(feature = "live")]
#[test]
fn rotate_four_times() {
    let map = maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    let mut l = crate::live::Live::from_mem(start(&Lobby { seed: 3, ..Lobby::new(map) }).unwrap());
    l.step();
    const CENTRE: u64 = 0x183a0fd50 + 0x14; // view centre tile (FUN_1800452e0 remembers it for the turn)
    let (r0, c0) = (l.rotation(), l.mem().i32(CENTRE));
    let (kx, ky) = l.keep().unwrap();
    for k in 1..=4 {
        let r = (r0 as i32 + 2 * k) % 8;
        l.rotate(r, kx as i32, ky as i32);
        let recs = l.step();
        assert_eq!((recs[0][0], recs[0][1]), (crate::replay::ROTATION, r as i16), "turn {k}: records under the new rotation");
        assert_eq!(l.rotation(), r as i16);
        assert_eq!(l.mem().i32(CENTRE), c0, "turn {k}: same centre tile");
    }
    assert_eq!(l.rotation(), r0);
}


/// Campaign progress: GameData's unlock rule and the original's file formats round-trip.
#[test]
fn campaign_progress() {
    let dir = std::env::temp_dir().join(format!("shc-progress-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nPlayerName:x\ncampaign2:3\nCampaign7:9\n||SETTINGS||\nkeys\n").unwrap();
    let mut p = Progress::read(&dir).unwrap();
    assert_eq!(p.unlocked, [1, 3, 1, 1, 1, 1, 5]); // case-insensitive keys, clamped to 5
    p.won(6, 2); // campaign 2 mission 1: not the highest unlocked -> nothing unlocked
    p.won(8, 1); // campaign 2 mission 3 = highest -> 4
    p.won(8, 0); // best difficulty kept
    p.won(35, 3); // campaign 7 mission 5: complete, no unlock past 5
    assert_eq!((p.unlocked[1], p.unlocked[6], p.done[&8], p.complete(6), p.complete(1)), (4, 5, 1, true, false));
    set_test_profile(&dir);
    p.save().unwrap();
    assert_eq!(Progress::read(&dir).unwrap(), p);
    // settings.cfg is merged, not rewritten: other keys and the KEYS block stay
    let cfg = Settings::read(&dir).unwrap();
    assert_eq!((cfg.get("playername"), cfg.get("CAMPAIGN2"), cfg.keys.as_str()), (Some("x"), Some("4"), "keys\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Campaign mission 1 (Nicaea) starts natively and ticks.
#[cfg(feature = "live")]
#[test]
fn campaign_mission_1_ticks() {
    if !maps_dir().join("crusaders_mission1a.map").exists() { return; } // needs the game install
    let mut l = crate::live::Live::from_mem(start_mission(1, 1).unwrap());
    assert!(l.records().len() > 10_000);
    let t0 = l.tick();
    for _ in 0..100 { l.step(); }
    assert_eq!(l.tick() - t0, 100, "sim did not tick");
}

/// Sandbox: the first built-in Free Build map and Custom Scenario start natively and tick.
#[cfg(feature = "live")]
#[test]
fn sandbox_starts_tick() {
    for (kind, scenario) in [(3, None), (1, Some(1))] {
        let Some((map, _)) = solo_maps(kind, false, true).into_iter().next() else { return }; // needs the game install
        let m = start_solo(&map, &FreeBuild::default(), scenario).unwrap_or_else(|e| panic!("{}: {e}", map.display()));
        let mut l = crate::live::Live::from_mem(m);
        assert!(l.records().len() > 10_000);
        let t0 = l.tick();
        for _ in 0..100 { l.step(); }
        assert_eq!(l.tick() - t0, 100, "{} did not tick", map.display());
    }
}

/// The tutorial starts natively: its panel shows the welcome page and speaks; Continue (TutorialAction 20) turns to
/// the next page (the camera task), and scrolling (TutorialAction 1) is taken.
#[cfg(feature = "live")]
#[test]
fn tutorial_starts_advances_and_speaks() {
    if !maps_dir().join("crusader_tutorial.map").exists() { return; } // needs the game install
    let mut l = crate::live::Live::from_mem(start_tutorial().unwrap());
    let mut t = crate::menu::tutorial::Tutorial::default();
    let _ = shc_sim::sound::drain();
    let mut frames = |l: &mut crate::live::Live, t: &mut crate::menu::tutorial::Tutorial, n: usize| {
        let mut speech = vec![];
        for _ in 0..n {
            for r in l.step().iter().filter(|r| r[0] == 23) { t.record(r) }
            speech.extend(shc_sim::sound::drain().into_iter().filter_map(|s| match s { shc_sim::sound::Sound::Named(n) => Some(n), _ => None }));
        }
        speech
    };
    let first = frames(&mut l, &mut t, 60);
    let welcome = t.body;
    assert!(t.heading.is_some() && welcome.is_some() && t.buttons[0].is_some(), "panel {t:?}");
    assert!(!first.is_empty(), "no voice line on the welcome page");
    l.tutorial(20, -1); // Continue
    let second = frames(&mut l, &mut t, 60);
    assert_ne!(t.body, welcome, "Continue did not turn the page");
    let looking = t.body;
    for _ in 0..30 { l.tutorial(1, -1); frames(&mut l, &mut t, 2); } // the camera scrolls
    let after_scroll = (t.body, t.buttons);
    l.tutorial(20, -1); // Continue
    let third = frames(&mut l, &mut t, 60);
    println!("speech {first:?} then {second:?} then {third:?}; pages {welcome:?} -> {looking:?} -> scrolled {after_scroll:?} -> {:?}", t.body);
    assert_ne!(t.body, looking, "the camera task did not lead on");
}

/// The lobby's options reach the engine where DLL_ApplyMultiplayerSetupData (FUN_180026340, setup::apply_mp_setup)
/// puts them, and stay through the start: the extreme switches, Advanced Settings, the building switches, start
/// positions; your colour picks the radar colours (default: the map's table 1, 3, 4, 2 ...).
#[cfg(feature = "live")]
#[test]
fn lobby_options_reach_engine() {
    let map = maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    const O: u64 = 0x188571310;
    let radar = |m: &shc_sim::mem::Mem| (1..5u64).map(|p| m.i32(RADAR_COLOURS + p * 4)).collect::<Vec<_>>();
    let base = Lobby { ais: vec![(0, 0), (2, 0), (4, 0)], seed: 1, ..Lobby::new(map.clone()) };
    let m = start(&base).unwrap();
    assert_eq!(radar(&m), [1, 3, 4, 2], "default colours");
    assert_eq!((0..70u64).map(|k| m.i32(O + 0x27a008 + k * 4)).filter(|&v| v == 1).count(), 70, "advanced off: everything allowed");
    let mut l = Lobby { colour: 3, extreme: [true, true, true], advanced: true, keeps: Some([2, 0, 3, 1, -10, -10, -10, -10]), ..base };
    l.adv[0] = 1; // pre-built castles
    l.adv[8] = 3; // enemy HP 150 %
    l.adv[13] = 1; // improved sieging X
    l.buildings[1] = 0; // no mercenary post
    let m = start(&l).unwrap();
    assert_eq!(radar(&m), [2, 1, 3, 4], "blue for you, the AIs from red up");
    assert_eq!((m.i32(O + 0x279fc4), m.i32(O + 0x279fc8), m.i32(O + 0x279fd4)), (1, 1, 1), "extreme powers, around the lord, advanced");
    assert_eq!(m.i32(0x183666dc4), 10000, "extreme troops");
    assert_eq!((m.i32(O + 0x279fd8), m.i32(O + 0x27a004), m.i32(0x183663f4c)), (1, 3, 1), "pre-build, enemy HP, sieging X");
    assert_eq!((m.i32(O + 0x27a008), m.i32(O + 0x27a00c), m.i32(O + 0x27a010)), (1, 0, 1), "barracks, merc post, stockade");
    assert_eq!((0..4u64).map(|k| m.u8(O + 0x27b7dc + k) as i8).collect::<Vec<_>>(), [2, 0, 3, 1], "start positions");
}
