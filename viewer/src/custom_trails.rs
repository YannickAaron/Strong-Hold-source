//! Custom trails (Trail Maker): CustomTrails/<name>/Trail_Mission_<n>.trail (MapFileManager: each folder with .trail
//! files is a trail), HUD_CustomTrailsSelect's list, the trail map of 90 / 91 / 92 (the Crusader / Warchest / Extreme
//! map by mission count), progress in custom_trails.cfg (`name/` + 50 `v:`: 0 not done, 1 won, −1 chicken), and a
//! mission's start (EditorDirector.LoadCustomTrailMap → FRONT_Multiplayer.StartCustomTrailMission: the .trail's
//! restart info as the lobby, shc_sim::trail::custom_setup; the .trail is the map). Difficulty is always 1, like
//! the original's (its toggle is never saved for custom trails). Child of trail_menu.

use super::*;

/// One custom trail: its folder name and its missions in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Custom { pub name: String, pub missions: Vec<PathBuf> }

/// The folders searched: the app's own player folder first, then the real game's (read only).
fn roots() -> Vec<PathBuf> { vec![skirmish::profile_dir().join("CustomTrails"), crate::paths::game_user().join("CustomTrails")] }

/// MapFileManager.GetCustomTrailFolders: every folder holding .trail files (missions by their number), by name.
pub fn list() -> Vec<Custom> {
    let mut v: Vec<Custom> = vec![];
    for root in roots() {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for d in rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
            let name = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if v.iter().any(|c| c.name == name) { continue; }
            let mut missions: Vec<PathBuf> = std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "trail")).collect();
            let num = |p: &PathBuf| p.file_stem().and_then(|s| s.to_string_lossy().rsplit('_').next().and_then(|n| n.parse::<i32>().ok())).unwrap_or(i32::MAX);
            missions.sort_by_key(|p| (num(p), p.clone()));
            missions.truncate(50);
            if !missions.is_empty() { v.push(Custom { name, missions }); }
        }
    }
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

static CURRENT: std::sync::Mutex<Option<Custom>> = std::sync::Mutex::new(None);

/// The custom trail being played (the trail map 90..92 shows it).
pub fn current() -> Option<Custom> { CURRENT.lock().unwrap().clone() }
pub fn select(c: Custom) { *CURRENT.lock().unwrap() = Some(c); }

/// OpenCustomTrail: the map layout by mission count (more than 30: the Crusader trail's, 21..30 the Warchest's, else
/// the Extreme's).
pub fn map_id(missions: usize) -> i32 { if missions > 30 { 90 } else if missions > 20 { 91 } else { 92 } }

fn progress_file() -> PathBuf { skirmish::profile_dir().join("custom_trails.cfg") }

/// ConfigSettings.LoadCustomTrailProgress: `name`'s 50 statuses (0 when unknown).
pub fn statuses(name: &str) -> [i32; 50] {
    let text = std::fs::read_to_string(progress_file()).or_else(|_| std::fs::read_to_string(crate::paths::game_user().join("custom_trails.cfg"))).unwrap_or_default();
    let mut s = [0; 50];
    if let Some(rest) = text.lines().find_map(|l| l.strip_prefix(&format!("{name}/"))) {
        for (k, v) in rest.split(':').filter_map(|v| v.trim().parse::<i32>().ok()).take(50).enumerate() { s[k] = v; }
    }
    s
}

/// SaveCustomTrailProgress: `name`'s line replaced (the other trails' lines kept).
pub fn save_statuses(name: &str, s: &[i32; 50]) -> std::io::Result<()> {
    let text = std::fs::read_to_string(progress_file()).or_else(|_| std::fs::read_to_string(crate::paths::game_user().join("custom_trails.cfg"))).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with(&format!("{name}/")) && !l.trim().is_empty()).map(str::to_string).collect();
    lines.push(format!("{name}/{}", s.iter().map(|v| format!("{v}:")).collect::<String>()));
    std::fs::create_dir_all(skirmish::profile_dir())?;
    crate::paths::write_atomic(&progress_file(), lines.join("\n") + "\n")
}

/// Mission `id` (0-based) of the current custom trail: the .trail's restart info with the custom trail fields set
/// (LoadCustomTrailMap: custom trail, level = mission number, its name, difficulty 1), the lobby it describes
/// (shc_sim::trail::custom_setup), the .trail loaded as the map; the colour mapping by the player colour setting.
#[cfg(feature = "live")]
pub fn start(id: i32) -> Result<shc_sim::mem::Mem, String> {
    use shc_sim::trail as tr;
    let c = current().ok_or("no custom trail chosen")?;
    let path = c.missions.get(id as usize).ok_or(format!("{} has no mission {}", c.name, id + 1))?;
    let file = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut tmp = shc_sim::mem::Mem::default();
    shc_sim::world::load_save(&mut tmp, &file).map_err(|e| format!("{}: {e:?}", path.display()))?;
    if tmp.u64(0x1808e9fb0) != shc_sim::save::BLOB_AT { return Err(format!("{}: no restart info", path.display())); }
    let mut r = tr::Restart::decode(&tmp.slice(shc_sim::save::BLOB_AT, tmp.i32(0x1808e9fb8) as usize)).ok_or("bad restart info")?;
    (r.custom_trail, r.level, r.name, r.difficulty) = (true, id + 1, c.name.clone(), 1);
    let blob = r.encode();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() as i32);
    let s = tr::custom_setup(shc_sim::data::aivs(), &r, blob.len() as i32, seed).ok_or("this mission's setup is not supported (custom lord or old format)")?;
    let colour = skirmish::Settings::load().get("PlayerColour").and_then(|v| v.parse::<i32>().ok()).unwrap_or(0) + 1;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let mut m = shc_sim::mem::Mem::default();
        shc_sim::terrain::init_static(&mut m);
        shc_sim::setup::new_skirmish(&mut m, &s, &file, colour, Some(seed));
        skirmish::member_colours(&mut m, colour - 1);
        m
    })).map_err(crate::skirmish::panic_text)
}

impl Menu {
    /// HUD_CustomTrailsSelect: the custom trails, 10 a page (name, mission count); a click opens its map. Nothing to
    /// make them with here: the Trail Maker is the original's.
    pub(super) fn custom_list(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let title = ui.text_of("TEXT_CUSTOMISATION_053");
        self.heading(ui, out, 602.5, &title);
        let all = list();
        self.frame(out, 560.0, 220.0, 800.0, 620.0, true);
        if all.is_empty() { self.para(ui, out, &format!("No custom trails. Trails made with the original's Trail Maker go in {}", roots()[0].display()), 600.0, 300.0, 720.0, 22.0, INK); }
        let page = self.scroll.min(all.len().saturating_sub(1)) / 10;
        for (k, c) in all.iter().enumerate().skip(page * 10).take(10) {
            let y = 250.0 + 56.0 * (k - page * 10) as f32;
            self.glow(ui, out, &format!("{}   ({})", c.name, c.missions.len()), 600.0, y, 720.0, None, Some(Act::Trail(A::CustomPick(k as i32))), cursor);
        }
        if all.len() > 10 {
            let pages = all.len().div_ceil(10);
            if page > 0 { self.img_button(ui, out, ["LeaderboardUp", "LeaderboardUpOver", "LeaderboardUp"], 1380.0, 240.0, 60.0, 60.0, Some(Act::Trail(A::CustomPage(-1))), cursor); }
            if page + 1 < pages { self.img_button(ui, out, ["LeaderboardDown", "LeaderboardDownOver", "LeaderboardDown"], 1380.0, 760.0, 60.0, 60.0, Some(Act::Trail(A::CustomPage(1))), cursor); }
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::Trail(A::Open(T::Skirmish))), cursor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A custom trail of two missions (each a .trail made here like the Trail Maker's: the map saved with a restart
    /// blob, you vs the Sultan) in the player folder: the Skirmish menu's Custom Trails lists it, picking it opens the
    /// Extreme map (id 92), Start starts mission 1 as a custom trail skirmish (0x183666fd0 = 2) that ticks, a win is
    /// kept in custom_trails.cfg (the frontier moves), the chicken skips, Skirmish Masters files it under its name.
    #[test]
    #[cfg(feature = "live")]
    fn custom_trail_flow() {
        let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
        let Ok(map) = std::fs::read(skirmish::maps_dir().join("A New Land.map")) else { return };
        let dir = std::env::temp_dir().join(format!("shc-custom-trails-{}", std::process::id()));
        let tdir = dir.join("CustomTrails/My Trail");
        std::fs::create_dir_all(&tdir).unwrap();
        std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nName:Lord Test\n||SETTINGS||\n").unwrap();
        crate::skirmish::set_test_profile(&dir);
        // the .trail: the map loaded, a v60 restart blob in block 6, the save writer
        let mut w = [0i32; 116];
        (w[0], w[1], w[2], w[4], w[6], w[0x15]) = (3, 40, 1, 1, 10, 1);
        w[10..18].copy_from_slice(&[0, 1, -10, -10, -10, -10, -10, -10]);
        w[0x25..0x6b].fill(1);
        w[107..115].fill(-1);
        let mut slots = vec![(-1, 1, 1), (6 * 8 + 1, 2, 2)];
        slots.resize(8, (-9999, 0, 0));
        let r = shc_sim::trail::Restart { version: 60, slots, source: 1, map: "A New Land".into(), setup: shc_sim::trail::setup_string(&w), outposts: true,
            aivs: (0..8).map(|_| shc_sim::trail::RestartAiv { builtin: true, builtin_lord: true, ..Default::default() }).collect(), custom_trail: true, level: 1, name: "My Trail".into(), difficulty: 1, ..Default::default() };
        let blob = r.encode();
        let mut m = shc_sim::mem::Mem::default();
        shc_sim::world::load_save(&mut m, &map).unwrap();
        m.slice_mut(shc_sim::save::BLOB_AT, blob.len()).copy_from_slice(&blob);
        (m.set_u64(0x1808e9fb0, shc_sim::save::BLOB_AT), m.set_i32(0x1808e9fb8, blob.len() as i32));
        let trail = shc_sim::save::save_game(&mut m, 0);
        for n in [1, 2] { std::fs::write(tdir.join(format!("Trail_Mission_{n}.trail")), &trail).unwrap(); }
        let all = list();
        assert_eq!(all.iter().find(|c| c.name == "My Trail").map(|c| c.missions.len()), Some(2));
        let k = all.iter().position(|c| c.name == "My Trail").unwrap();
        let press = |m: &mut Menu, u: &mut Ui, a: Act| {
            m.draw(u, 1920.0, 1080.0, [0.0; 2]);
            let p = m.hits.iter().find(|h| h.1 == a).unwrap_or_else(|| panic!("no button")).0;
            m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0])
        };
        let mut menu = Menu { screen: Screen::Trail(T::Skirmish), ..Menu::main() };
        press(&mut menu, &mut u, Act::Trail(A::Open(T::Custom)));
        press(&mut menu, &mut u, Act::Trail(A::CustomPick(k as i32)));
        assert!(matches!(menu.screen, Screen::Trail(T::Map(92, 1))), "two missions: the Extreme map, mission 1");
        assert_eq!(menu.screen_image().as_deref(), Some("trail-map-2"));
        let Some(Go::Trail(92, 0, 1)) = press(&mut menu, &mut u, Act::Trail(A::Start)) else { panic!("Start") };
        assert!(Menu::trail_brief(92, 0, 1).is_none(), "no briefing data for custom trails");
        let mut l = crate::live::Live::from_mem(super::super::start(92, 0, 1).expect("custom mission starts"));
        assert_eq!((l.mem().i32(0x188571b80), l.mem().i32(0x183666fd0), l.mem().i32(0x188571c34 + 8)), (99, 2, 7), "skirmish, custom trail, the Sultan in slot 2");
        let t0 = l.tick();
        for _ in 0..100 { l.step(); }
        assert_eq!(l.tick() - t0, 100);
        record_win(92, 0, 0, 0, 4000);
        assert_eq!(&statuses("My Trail")[..3], &[1, 0, 0]);
        assert_eq!((Progress::load(92).next, Progress::load(92).completed()), (2, 1));
        assert!(matches!(after(92, 0, 1, true), Go::Trail(92, 1, 1)) && matches!(after(92, 1, 1, true), Go::ToMenu));
        press(&mut menu, &mut u, Act::Trail(A::Chicken));
        assert_eq!(&statuses("My Trail")[..3], &[1, -1, 0], "the chicken skips mission 2");
        assert!(matches!(masters::kind(Some((92, 0)), None), Some(masters::Kind::Custom(1, n)) if n == "My Trail"));
        assert!(matches!(menu.back(), None) && matches!(menu.screen, Screen::Trail(T::Custom)));
        std::fs::remove_dir_all(&dir).ok();
    }
}
