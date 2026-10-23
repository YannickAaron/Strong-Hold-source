//! The skirmish lobby's map list (FRONT_Multiplayer.populateMapList: the search box, the column sorts, the user /
//! built-in include boxes) and its options (the extreme toggles, the Advanced Skirmish Options popup =
//! Show_MP_SkirmishAdvanced). A child module of menu.rs: it draws with that file's helpers.
use super::*;
use crate::skirmish::{MapInfo, ADV};

/// The lobby's own clicks.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum A { Extreme(u8), AdvOpen, AdvClose, AdvEnable, Adv(usize), Remember, Sort(u8), Include(u8), Search, ClearSearch, Mode }

/// The map list: every skirmish map found (path, info, user map), the search text (`typing`: the box has the
/// keyboard), the sort (FileListHeaderClickedHandler: 0 name, 1 date, 2 players, 3 size, 5 balanced; ascending?),
/// the include boxes (user, built-in), `adv` = the Advanced popup is open. `dirty`: re-sort by the shown (translated)
/// names at the next draw.
pub struct List { all: Vec<(PathBuf, MapInfo, bool)>, pub filter: String, sort: u8, asc: bool, include: [bool; 2], pub typing: bool, dirty: bool, pub adv: bool }

impl Default for List {
    fn default() -> Self { List { all: vec![], filter: String::new(), sort: 0, asc: true, include: [true; 2], typing: false, dirty: false, adv: false } }
}
impl List {
    /// A map's info, also while the search hides it (the original keeps selectedMPHeader).
    pub fn info(&self, map: &std::path::Path) -> Option<&MapInfo> { self.all.iter().find(|e| e.0 == map).map(|e| &e.1) }
    /// From a user Maps folder (the row's icon).
    pub fn is_user(&self, map: &std::path::Path) -> bool { self.all.iter().any(|e| e.0 == map && e.2) }
}

/// MapFileManager.GetMultiplayerMaps: multiplayer maps (mapType 1) with room for you and an AI, the user Maps folders'
/// and the game's. ponytail: no Workshop maps.
fn scan(include: [bool; 2]) -> Vec<(PathBuf, MapInfo, bool)> {
    let dirs = skirmish::user_map_dirs().into_iter().filter(|_| include[0]).map(|d| (d, true)).chain(include[1].then(|| (skirmish::maps_dir(), false)));
    dirs.flat_map(|(d, u)| skirmish::list(&d, "map").into_iter().map(move |p| (p, u)))
        .filter_map(|(p, u)| skirmish::map_info(&p).filter(|i| i.players >= 2).map(|i| (p, i, u))).collect()
}

fn modified(p: &std::path::Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

/// MultiplayerSetupData's enemy HP steps (advopt_enemy_hps 0..3).
const HPS: [&str; 4] = ["66%", "100%", "125%", "150%"];

impl Menu {
    /// FRONT_Multiplayer.Open(skirmishSetup): the map list with `l.map` selected (an empty path: Crater Lake, the map
    /// the setup port is verified on, else the first; the original starts with none).
    pub(super) fn open_lobby(&mut self, mut l: Lobby) {
        let all = scan([true; 2]);
        if !all.iter().any(|e| e.0 == l.map) {
            if !l.map.as_os_str().is_empty() {
                self.error = Some(format!("{} is not a skirmish map", l.map.file_stem().unwrap_or_default().to_string_lossy()));
                return;
            }
            match all.iter().find(|e| e.0.ends_with("Crater Lake.map")).or(all.first()) {
                Some(e) => l.map = e.0.clone(),
                None => { self.error = Some(format!("no maps in {}", skirmish::maps_dir().display())); return }
            }
        }
        let map = l.map.clone();
        self.open(Screen::Lobby(l), vec![]);
        self.lst = List { all, dirty: true, ..List::default() };
        self.lobby_rebuild(None, &map);
        if let Some(i) = self.files.iter().position(|f| *f == map) { self.select(i); }
    }

    /// Before the lobby draws: a changed search / sort / include rebuilds the list by the shown names.
    pub(super) fn lobby_refresh(&mut self, ui: &Ui, map: &std::path::Path) {
        if std::mem::take(&mut self.lst.dirty) { self.lobby_rebuild(Some(ui), map); }
    }

    /// populateMapList: `files` / `infos` = the maps whose shown name contains the search text (any case), sorted
    /// (MapFileManager.SortList by the lower-case name, the file date, players, size; balanced = by name, then
    /// balanced first); the selected `map` scrolled into view. `ui` None: file names (before the first draw).
    fn lobby_rebuild(&mut self, ui: Option<&Ui>, map: &std::path::Path) {
        if self.lst.all.is_empty() { return; } // a LAN lobby: its own list (mp_menu.rs)
        let name = |p: &PathBuf| ui.map_or_else(|| p.file_stem().unwrap_or_default().to_string_lossy().into_owned(), |u| map_name(u, p)).to_lowercase();
        let f = self.lst.filter.to_lowercase();
        let mut v: Vec<(String, &(PathBuf, MapInfo, bool))> = self.lst.all.iter().map(|e| (name(&e.0), e)).filter(|(n, _)| n.contains(&f)).collect();
        let asc = self.lst.asc;
        let dir = |o: std::cmp::Ordering| if asc { o } else { o.reverse() };
        v.sort_by(|a, b| dir(a.0.cmp(&b.0)));
        match self.lst.sort {
            1 => v.sort_by(|a, b| dir(modified(&a.1 .0).cmp(&modified(&b.1 .0)))),
            2 => v.sort_by(|a, b| dir(a.1 .1.players.cmp(&b.1 .1.players))),
            3 => v.sort_by(|a, b| dir(a.1 .1.size.cmp(&b.1 .1.size))),
            5 => v.sort_by(|a, b| dir(a.1 .1.balanced.cmp(&b.1 .1.balanced))),
            _ => {}
        }
        (self.files, self.infos) = v.into_iter().map(|(_, e)| (e.0.clone(), e.1.clone())).unzip();
        let sel = self.files.iter().position(|f| f == map);
        let max = self.files.len().saturating_sub(LIST_ROWS);
        self.scroll = match sel {
            Some(i) if i < self.scroll || i >= self.scroll + LIST_ROWS => i.saturating_sub(LIST_ROWS / 2).min(max),
            _ => self.scroll.min(max),
        };
    }

    pub(super) fn lobby_typing(&self) -> bool { self.lst.typing && matches!(self.screen, Screen::Lobby(_)) }

    /// The search box's keys (TextBox MaxLength 32); Return / Escape leave it.
    pub(super) fn lobby_key(&mut self, k: &str) {
        match k.split(',').next().unwrap_or("") {
            "Backspace" | "Delete" => { self.lst.filter.pop(); }
            "Return" | "KeypadEnter" | "Escape" => { self.lst.typing = false; return }
            _ => match key_char(k) { Some(c) if self.lst.filter.chars().count() < 32 => self.lst.filter.push(c), _ => return },
        }
        self.lst.dirty = true;
    }

    /// Esc in the lobby: out of the search box, then the popup closes; false = not ours.
    pub(super) fn lobby_back(&mut self) -> bool {
        if self.lst.typing { self.lst.typing = false; return true; }
        if self.lst.adv { self.lst.adv = false; return true; }
        false
    }

    pub(super) fn lobby_click(&mut self, a: A) -> Option<Go> {
        let Screen::Lobby(l) = &mut self.screen else { return None };
        match a {
            // Settings_ExtremeTroops / Powers / PowerLord (skirmish: straight into MPsetupData); around the lord only
            // with the powers on, the powers off clear it
            A::Extreme(k) => {
                let k = k as usize;
                if k != 2 || l.extreme[1] { l.extreme[k] = !l.extreme[k]; }
                if !l.extreme[1] { l.extreme[2] = false; }
            }
            A::AdvOpen => self.lst.adv = true,
            A::AdvClose => self.lst.adv = false,
            A::AdvEnable => l.advanced = !l.advanced,
            // the toggles; enemy HP steps 66 → 100 → 125 → 150 % → 66
            A::Adv(i) => l.adv[i] = if i == 8 { (l.adv[i] + 1) % 4 } else { (l.adv[i] == 0) as i32 },
            A::Remember => {
                let mut cfg = skirmish::Settings::load();
                cfg.set("SkirmishPresets", l.presets());
                if let Err(e) = cfg.save() { self.error = Some(format!("settings.cfg: {e}")); }
            }
            A::Sort(c) => {
                // the same column again flips; name starts A-Z, the others high first
                (self.lst.sort, self.lst.asc) = if self.lst.sort == c { (c, !self.lst.asc) } else { (c, c == 0) };
                self.lst.dirty = true;
            }
            A::Include(k) => {
                self.lst.include[k as usize] = !self.lst.include[k as usize];
                self.lst.all = scan(self.lst.include);
                self.lst.dirty = true;
            }
            // the game mode (ours): Conquest -> King of the Hill Short / Normal / Long -> Conquest
            A::Mode => l.hill = crate::koth::TARGETS.iter().copied().find(|&t| t > l.hill).unwrap_or(0),
            A::Search => self.lst.typing = true,
            A::ClearSearch => { self.lst.filter.clear(); self.lst.dirty = true; }
        }
        None
    }

    /// Under the map list: the include boxes (user maps, built-in maps; Workshop drawn, nothing to include), the
    /// search box (TEXT_NEW_TEXT2_126 while empty) and its clear button.
    pub(super) fn lobby_list_bar(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        for (k, (x, icon)) in [(50.0, "user"), (110.0, "ff"), (170.0, "steam")].into_iter().enumerate() {
            let on = self.lst.include.get(k).copied().unwrap_or(true);
            self.checkbox(out, x, 666.0, on, (k < 2).then_some(Act::Lobby(A::Include(k as u8))));
            ui.img_fit(out, icon, self.r(x + 22.0, 661.0, 30.0, 30.0), [1.0; 4]);
        }
        let p = self.r(260.0, 660.0, 255.0, 34.0);
        widget(out, |o| Ui::text_field(o, p, self.lst.typing || Self::inside(p, cursor), self.s));
        self.hits.push((p, Act::Lobby(A::Search)));
        let caret = if self.lst.typing && std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis()) % 1000 < 500 { "|" } else { "" };
        if self.lst.filter.is_empty() && !self.lst.typing {
            self.label(ui, out, &ui.text_of("TEXT_NEW_TEXT2_126"), 270.0, 677.0, 20.0, 0.0, CREAM);
        } else {
            let t = format!("{}{caret}", self.lst.filter);
            self.label(ui, out, &t, 270.0, 677.0, 20.0, 0.0, CREAM);
        }
        if !self.lst.filter.is_empty() {
            self.img_button(ui, out, ["UI-Buttons L009", "UI-Buttons L010", "UI-Buttons L009"], 520.0, 660.0, 34.0, 34.0, Some(Act::Lobby(A::ClearSearch)), cursor);
        }
    }

    /// The options panel's toggles: Extreme Troops, Extreme Powers, Extreme Powers near the Lord (50 % without the
    /// powers), Allow Outposts (maps with outposts), the Advanced Skirmish Options button and, with them in use,
    /// "Advanced Options Enabled" above it. A LAN lobby: shown, not changeable (the LAN lobby does not carry them).
    pub(super) fn lobby_toggles(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, l: &Lobby, outposts: bool, cursor: [f32; 2]) {
        let lan = self.mp.net.is_some();
        let tick = |on: bool| if on { "UI-Buttons S100" } else { "UI-Buttons S043" };
        for (k, key) in ["TEXT_SKIRMISH_MISC_018", "TEXT_SKIRMISH_MISC_017", "TEXT_SKIRMISH_MISC_015"].into_iter().enumerate() {
            let alpha = if k == 2 && !l.extreme[1] { 0.5 } else { 1.0 };
            let a = (!lan).then_some(Act::Lobby(A::Extreme(k as u8)));
            self.glow_at(ui, out, &ui.text_of(key), 1016.0, 419.0 + 40.0 * k as f32, 280.0, Some((tick(l.extreme[k]), 250.0)), a, alpha, cursor);
        }
        if outposts {
            self.glow(ui, out, &ui.text_of("TEXT_SKIRMISH_MISC_019"), 1016.0, 539.0, 280.0, Some(tick(l.outposts)), Some(Act::Outposts), cursor);
        }
        if l.advanced_on() { self.label(ui, out, &ui.text_of("TEXT_NEW_CTEXT_432"), 1156.0, 580.0, 14.0, 0.5, [0.0, 0.0, 0.0, 1.0]); }
        // the game mode (ours, koth.rs): the host's to change in a LAN lobby (it travels with it)
        let mode = match crate::koth::TARGETS.iter().position(|&t| t == l.hill) {
            Some(k) => format!("King of the Hill: {} ({} points)", crate::koth::LENGTHS[k], l.hill),
            None => "Game Mode: Conquest".to_string(),
        };
        self.glow(ui, out, &mode, 657.0, 613.0, 345.0, None, self.mp_can_start().then_some(Act::Lobby(A::Mode)), cursor);
        self.glow(ui, out, &ui.text_of("TEXT_NEW_CTEXT_396"), 1016.0, 589.0, 280.0, None, (!lan).then_some(Act::Lobby(A::AdvOpen)), cursor);
    }

    /// Show_MP_SkirmishAdvanced: dimmed screen, a 1000×465 BDR_SH1DEa panel (centred), the heading, "Enable Advanced
    /// Options", the 14 options in two columns of 380 px rows (50 % while not enabled; enemy HP cycles), Remember
    /// These Options (Settings_SkirmishPresets) and Close; the hovered option's description in the bar at the bottom
    /// (MPGame_Type_Description). ponytail: no Spectator Mode button (no spectating in a skirmish here).
    pub(super) fn lobby_advanced(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, l: &Lobby, cursor: [f32; 2]) {
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        Ui::fill(out, self.r(0.0, 0.0, 1366.0, 768.0), [0.0, 0.0, 0.0, 0.667]);
        self.hits.push((self.r(0.0, 0.0, 1366.0, 768.0), Act::Lobby(A::AdvOpen))); // the dim swallows clicks
        let (px, py) = (183.0, 151.5);
        self.frame_with(out, px, py, 1000.0, 465.0, true, "Panel_Corner_");
        self.small_caps(ui, out, &ui.text_of("TEXT_NEW_CTEXT_396"), px + 500.0, py + 42.0, 30.0, BLACK);
        ui.img(out, "Panel_Divider", self.r(px + 200.0, py + 50.0, 600.0, 12.0), [1.0; 4]);
        // CheckBox_Template + its 16 px text, centred
        let text = ui.text_of("TEXT_NEW_CTEXT_397");
        let tw = ui.text(&mut vec![], &text, 0.0, 0.0, 16.0, 1.0, 0.0, 0.0, BLACK) / self.s;
        let x = px + 500.0 - (26.0 + tw) / 2.0;
        self.checkbox(out, x, py + 76.5, l.advanced, Some(Act::Lobby(A::AdvEnable)));
        self.label(ui, out, &text, x + 26.0, py + 86.5, 16.0, 0.0, BLACK);
        self.hits.push((self.r(x, py + 75.0, 26.0 + tw, 23.0), Act::Lobby(A::AdvEnable)));
        let alpha = if l.advanced { 1.0 } else { 0.5 }; // MPSettings_AdvSkirmish_Opacity
        let mut help = None;
        for (k, &(_, key, i)) in ADV.iter().enumerate() {
            let (x, y) = (px + if k < 7 { 60.0 } else { 560.0 }, py + 110.0 + 35.0 * (k % 7) as f32);
            let v = l.adv[i];
            let (label, tick) = match key {
                Some(key) => (ui.text_of(key), Some((if v > 0 { "UI-Buttons S100" } else { "UI-Buttons S043" }, 350.0))),
                None => (format!("{} {}", ui.text_of("TEXT_NEW_CTEXT_414"), HPS[v.clamp(0, 3) as usize]), None),
            };
            self.glow_at(ui, out, &label, x, y, 380.0, tick, Some(Act::Lobby(A::Adv(i))), alpha, cursor);
            // the description: the label's next text (lookUpText(n) = key n + 1), enemy HP's 415
            if Self::inside(self.r(x, y - 1.5, 380.0, 31.0), cursor) {
                help = Some(key.and_then(|k| k.rsplit('_').next()?.parse::<u32>().ok()).map_or("TEXT_NEW_CTEXT_415".to_string(), |n| format!("TEXT_NEW_CTEXT_{:03}", n + 1)));
            }
        }
        self.glow(ui, out, &ui.text_of("TEXT_NEW_CTEXT_430"), px + 20.0, py + 403.5, 300.0, None, Some(Act::Lobby(A::Remember)), cursor);
        self.glow(ui, out, &ui.text_of("TEXT_NEW_TEXT_091"), px + 680.0, py + 403.5, 300.0, None, Some(Act::Lobby(A::AdvClose)), cursor);
        if let Some(h) = help {
            self.frame(out, 83.0, 698.0, 1200.0, 55.0, true);
            self.para(ui, out, &ui.text_of(&h), 108.0, 714.0, 1150.0, 18.0, BLACK);
        }
    }
}

/// A typed key (keys.rs Unity name with modifiers) as a name / search character: letters (Shift = capitals),
/// digits, space, - and _.
pub(super) fn key_char(k: &str) -> Option<char> {
    let (code, shift) = (k.split(',').next().unwrap_or(""), k.contains("shift"));
    Some(match code {
        "Space" => ' ',
        "Minus" | "KeypadMinus" => if shift { '_' } else { '-' },
        _ => match code.strip_prefix("Alpha").or(code.strip_prefix("Keypad")).and_then(|d| d.parse::<u8>().ok()) {
            Some(d) if d < 10 => (b'0' + d) as char,
            _ if code.len() == 1 && code.as_bytes()[0].is_ascii_alphabetic() => {
                let c = code.chars().next()?;
                if shift { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }
            }
            _ => return None,
        },
    })
}

/// The lobby: the search narrows the list (any case, by the shown name), the Name header flips the order, the
/// toggles and the Advanced popup edit the lobby (Remember stores them), Esc leaves the box, then the popup.
#[test]
fn lobby_list_and_options() {
    let dir = std::env::temp_dir().join(format!("shc-lobby-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nPlayerColour:3\n||SETTINGS||\n").unwrap();
    skirmish::set_test_profile(&dir);
    let act = |m: &mut Menu, a: Act| { m.hits = vec![([0.0, 0.0, 1.0, 1.0], a)]; m.click([0.5, 0.5]) };
    let mut m = Menu::main();
    act(&mut m, Act::Skirmish);
    let Screen::Lobby(l) = &m.screen else { return }; // needs the game's maps
    assert_eq!(l.colour, 3, "your colour from Options");
    let n = m.files.len();
    if n < 3 { return; }
    act(&mut m, Act::Lobby(A::Search));
    assert!(m.capturing(), "the search box takes the keys");
    for k in ["c", "R", "a", "t"] { m.key(k); }
    m.lobby_rebuild(None, &PathBuf::new());
    assert!(!m.files.is_empty() && m.files.len() < n, "{} of {n}", m.files.len());
    assert!(m.files.iter().all(|f| f.file_stem().unwrap().to_string_lossy().to_lowercase().contains("crat")));
    act(&mut m, Act::Lobby(A::ClearSearch));
    m.lobby_rebuild(None, &PathBuf::new());
    let first = m.files[0].clone();
    act(&mut m, Act::Lobby(A::Sort(0)));
    m.lobby_rebuild(None, &PathBuf::new());
    assert_eq!(m.files.last(), Some(&first), "Name again: Z-A");
    assert!(!m.capturing(), "a click elsewhere leaves the box");
    act(&mut m, Act::Lobby(A::Search));
    assert!(m.back().is_none() && !m.capturing() && matches!(m.screen, Screen::Lobby(_)), "Esc leaves the box, not the lobby");
    act(&mut m, Act::Lobby(A::Extreme(2)));
    act(&mut m, Act::Lobby(A::Extreme(1)));
    act(&mut m, Act::Lobby(A::Extreme(2)));
    act(&mut m, Act::Lobby(A::AdvOpen));
    act(&mut m, Act::Lobby(A::AdvEnable));
    act(&mut m, Act::Lobby(A::Adv(8)));
    act(&mut m, Act::Lobby(A::Adv(12)));
    act(&mut m, Act::Lobby(A::Remember));
    let Screen::Lobby(l) = &m.screen else { panic!() };
    assert_eq!((l.extreme, l.advanced, l.adv[8], l.adv[12], l.advanced_on()), ([false, true, true], true, 2, 1, true), "around the lord needs the powers");
    assert_eq!(Lobby::from_settings(PathBuf::new()).adv, l.adv, "remembered");
    assert!(m.lst.adv && m.back().is_none() && !m.lst.adv, "Esc closes the popup");
    let _ = std::fs::remove_dir_all(&dir);
}
