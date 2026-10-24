//! Extras (ours, the main menu's sixth shield): this build (version, platform, folders), the update check on request
//! (update.rs, which also installs the new version) and the custom lords: the ones installed and the published ones
//! (Get / Update), and importing a lord's .zip (lord_share.rs; a .zip dropped on the window lands here too). Imported lords simply join the skirmish's Add-AI faces. Load Game's dark panel and
//! text buttons. A switch over the right column shows the published maps instead (map_share.rs; a dropped .map lands
//! in the player folder's Maps). A child module of menu.rs.
use super::*;

/// The screen's own clicks.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum A { Open, Folder, Check, Notes, Import, Replace(bool), Lord(u8), Maps(bool), Map(u8), MapsFolder }

/// Lords listed (more: "+n more").
const SHOWN: usize = 6;
const OK: [f32; 4] = [0.45, 0.85, 0.35, 1.0];
const BAD: [f32; 4] = [1.0, 0.4, 0.3, 1.0];
const DIM: [f32; 4] = [0.75, 0.72, 0.6, 1.0];

/// A path with the home folder as "~".
fn short(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    match std::env::var("HOME") { Ok(h) if !h.is_empty() && s.starts_with(&h) => format!("~{}", &s[h.len()..]), _ => s }
}

/// The lords column: the installed lords, then the published ones not installed here.
fn lord_rows() -> Vec<(String, Option<crate::custom_lords::Custom>)> {
    let mut v: Vec<_> = (0..crate::custom_lords::count()).filter_map(crate::custom_lords::get).map(|c| (c.name.clone(), Some(c))).collect();
    for o in crate::lord_share::online() { if !v.iter().any(|r| r.0 == o.name) { v.push((o.name, None)); } }
    v
}

impl Menu {
    pub(super) fn extras_click(&mut self, a: A) -> Option<Go> {
        match a {
            A::Open => {
                crate::custom_lords::rescan();
                crate::lord_share::fetch_online();
                crate::map_share::fetch_online();
                *crate::lord_share::MESSAGE.lock().unwrap() = None;
                *self = Menu { screen: Screen::Extras, ..Self::main() };
            }
            A::Folder => {
                let dir = skirmish::profile_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("open").arg(&dir).spawn();
            }
            A::Check => crate::update::check_now(),
            A::Notes => { self.open_options(); if let Screen::Options(op) = &mut self.screen { op.page = 5; } }
            A::Import => if let Some(f) = crate::lord_share::pick_zip() { crate::lord_share::import_ui(&f, false) },
            A::Lord(k) => if let Some((name, _)) = lord_rows().get(k as usize) { crate::lord_share::get(name) },
            A::Maps(on) => crate::map_share::SHOW_MAPS.store(on, std::sync::atomic::Ordering::Relaxed),
            A::Map(k) => if let Some(o) = crate::map_share::online().get(k as usize) { crate::map_share::get(&o.name) },
            A::MapsFolder => {
                let dir = skirmish::user_map_dirs()[0].clone();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("open").arg(&dir).spawn();
            }
            A::Replace(yes) => {
                let p = crate::lord_share::PENDING.lock().unwrap().take();
                if let (true, Some((f, _))) = (yes, p) { crate::lord_share::import_ui(&f, true); }
            }
        }
        None
    }

    pub(super) fn extras_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        self.panel(ui, out, 300.0, 110.0, 1320.0, 880.0);
        self.text(ui, out, "Extras", 960.0, 185.0, 56.0, 0.5, GOLD);

        // ---- this build
        let x = 380.0;
        self.text(ui, out, "This build", x, 270.0, 34.0, 0.0, GOLD);
        self.text(ui, out, &format!("Strong Hold {} (native port)", crate::update::VERSION), x, 320.0, 24.0, 0.0, CREAM);
        self.text(ui, out, &format!("{} \u{b7} {}", match std::env::consts::OS { "macos" => "macOS", "windows" => "Windows", o => o }, std::env::consts::ARCH), x, 354.0, 20.0, 0.0, DIM);
        let mut y = 380.0;
        for (label, p) in [("Game files", crate::paths::game()), ("Player folder", skirmish::profile_dir())] {
            self.label(ui, out, label, x, y + 12.0, 18.0, 0.0, DIM);
            y = self.para(ui, out, &short(&p), x + 140.0, y, 400.0, 18.0, CREAM) + 6.0;
        }
        self.button(ui, out, "Open player folder", x, y + 8.0, 300.0, 48.0, Act::Extras(A::Folder), cursor);

        // ---- updates
        self.text(ui, out, "Updates", x, 610.0, 34.0, 0.0, GOLD);
        let (line, color) = match crate::update::status() {
            crate::update::Status::Idle => ("Not checked yet.".to_string(), DIM),
            crate::update::Status::Checking => ("Checking\u{2026}".to_string(), DIM),
            crate::update::Status::Latest(t) => (format!("Up to date (latest release: {}).", t.trim_start_matches('v')), OK),
            crate::update::Status::Newer(t) => (format!("Version {} is available.", t.trim_start_matches('v')), GOLD),
            crate::update::Status::Offline => ("Couldn't reach GitHub. Offline?".to_string(), BAD),
        };
        self.para(ui, out, &line, x, 632.0, 540.0, 22.0, color);
        self.button(ui, out, "Check for updates", x, 680.0, 260.0, 48.0, Act::Extras(A::Check), cursor);
        self.button(ui, out, "Patch notes", x + 280.0, 680.0, 260.0, 48.0, Act::Extras(A::Notes), cursor);
        if let Some(tag) = crate::update::available() {
            self.button(ui, out, &crate::update::button(&tag), x, 740.0, 540.0, 48.0, Act::Update, cursor);
            if let crate::update::Install::Failed(e) = crate::update::install_state() { self.para(ui, out, &e, x, 800.0, 540.0, 18.0, BAD); }
        }

        // ---- custom lords / maps: the column's two titles switch it
        let x = 1000.0;
        let maps = crate::map_share::SHOW_MAPS.load(std::sync::atomic::Ordering::Relaxed);
        self.text(ui, out, "Custom lords", x, 270.0, 34.0, 0.0, if maps { DIM } else { GOLD });
        self.text(ui, out, "Maps", x + 300.0, 270.0, 34.0, 0.0, if maps { GOLD } else { DIM });
        let (ux, uw) = if maps { (x + 296.0, 90.0) } else { (x - 4.0, 196.0) }; // the open one underlined
        ui.img(out, "Panel_Divider", self.r(ux, 280.0, uw, 8.0), [1.0; 4]);
        self.hits.push((self.r(x, 236.0, 280.0, 44.0), Act::Extras(A::Maps(false))));
        self.hits.push((self.r(x + 300.0, 236.0, 120.0, 44.0), Act::Extras(A::Maps(true))));
        if maps { return self.extras_maps(ui, out, x, cursor); }
        let rows = lord_rows();
        if rows.is_empty() { self.para(ui, out, "None installed yet.", x, 296.0, 540.0, 20.0, DIM); }
        let getting = crate::lord_share::GETTING.lock().unwrap().clone();
        for (k, (name, c)) in rows.iter().enumerate().take(SHOWN) {
            let (ry, p) = (296.0 + 66.0 * k as f32, self.r(x, 296.0 + 66.0 * k as f32, 60.0, 60.0));
            ui.img(out, "Face Background Generic", p, [1.0; 4]);
            if let Some(c) = c {
                if !crate::custom_lords::draw_face(out, c, p, [1.0; 4]) { ui.img(out, FACES.iter().find(|f| f.1 == crate::custom_lords::parent(c)).map_or("", |f| f.0), p, [1.0; 4]); }
            }
            ui.img(out, "Face Border", p, [1.0; 4]);
            self.label(ui, out, name, x + 76.0, ry + 20.0, 22.0, 0.0, CREAM);
            let title = c.as_ref().and_then(|c| crate::custom_lords::help_text(c, "TITLE"));
            let sub = title.or_else(|| c.is_none().then(|| "Not installed".into()));
            if let Some(t) = sub { self.label(ui, out, &t, x + 76.0, ry + 44.0, 17.0, 0.0, DIM); }
            // published (lord_share.rs): download / update, or how far the download is
            let (bx, by) = (x + 410.0, ry + 10.0);
            match (getting.as_ref().filter(|g| &g.0 == name), crate::lord_share::state(name)) {
                (Some((_, f)), _) => { self.label(ui, out, &format!("{:.0} %", f * 100.0), bx + 65.0, by + 20.0, 20.0, 0.5, GOLD); }
                (None, Some(crate::lord_share::State::Get)) => self.button(ui, out, "Get", bx, by, 130.0, 40.0, Act::Extras(A::Lord(k as u8)), cursor),
                (None, Some(crate::lord_share::State::Update)) => self.button(ui, out, "Update", bx, by, 130.0, 40.0, Act::Extras(A::Lord(k as u8)), cursor),
                (None, Some(crate::lord_share::State::Current)) => { self.label(ui, out, "Up to date", bx + 65.0, by + 20.0, 17.0, 0.5, OK); }
                (None, None) => {}
            }
        }
        if rows.len() > SHOWN { self.label(ui, out, &format!("+{} more", rows.len() - SHOWN), x, 296.0 + 66.0 * SHOWN as f32 + 10.0, 18.0, 0.0, DIM); }
        let y = 720.0;
        let pending = crate::lord_share::PENDING.lock().unwrap().clone();
        if let Some((_, names)) = pending {
            let b = self.para(ui, out, &format!("{} already installed. Replace?", names.join(", ")), x, y, 540.0, 20.0, GOLD);
            self.button(ui, out, "Yes, replace", x, b + 10.0, 260.0, 48.0, Act::Extras(A::Replace(true)), cursor);
            self.button(ui, out, "No", x + 280.0, b + 10.0, 260.0, 48.0, Act::Extras(A::Replace(false)), cursor);
        } else {
            self.button(ui, out, "Import lord (.zip)", x, y, 540.0, 48.0, Act::Extras(A::Import), cursor);
            match crate::lord_share::MESSAGE.lock().unwrap().clone() {
                Some((m, ok)) => { self.para(ui, out, &m, x, y + 60.0, 540.0, 18.0, if ok { OK } else { BAD }); }
                None => { self.para(ui, out, "Or drop a lord's .zip on the window. Imported lords join the skirmish's lords.", x, y + 60.0, 540.0, 18.0, DIM); }
            }
        }

        self.button(ui, out, "Back", 840.0, 910.0, 240.0, 56.0, Act::Back, cursor);
    }

    /// The right column's maps: the published ones with Get / Update (map_share.rs), the maps folder, the last result.
    fn extras_maps(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, x: f32, cursor: [f32; 2]) {
        let rows = crate::map_share::online();
        if rows.is_empty() { self.para(ui, out, "No maps published yet (or offline).", x, 296.0, 540.0, 20.0, DIM); }
        let getting = crate::map_share::GETTING.lock().unwrap().clone();
        for (k, o) in rows.iter().enumerate().take(SHOWN + 1) {
            let ry = 300.0 + 58.0 * k as f32;
            self.label(ui, out, &o.name, x, ry + 20.0, 22.0, 0.0, CREAM);
            self.label(ui, out, &format!("{:.1} MB", o.file.size as f32 / 1048576.0), x, ry + 42.0, 15.0, 0.0, DIM);
            let (bx, by) = (x + 410.0, ry + 4.0);
            match (getting.as_ref().filter(|g| g.0 == o.name), crate::map_share::state(o)) {
                (Some((_, f)), _) => { self.label(ui, out, &format!("{:.0} %", f * 100.0), bx + 65.0, by + 20.0, 20.0, 0.5, GOLD); }
                (None, crate::map_share::State::Get) => self.button(ui, out, "Get", bx, by, 130.0, 40.0, Act::Extras(A::Map(k as u8)), cursor),
                (None, crate::map_share::State::Update) => self.button(ui, out, "Update", bx, by, 130.0, 40.0, Act::Extras(A::Map(k as u8)), cursor),
                (None, crate::map_share::State::Current) => { self.label(ui, out, "Up to date", bx + 65.0, by + 20.0, 17.0, 0.5, OK); }
            }
        }
        if rows.len() > SHOWN + 1 { self.label(ui, out, &format!("+{} more", rows.len() - SHOWN - 1), x, 300.0 + 58.0 * (SHOWN + 1) as f32 + 10.0, 18.0, 0.0, DIM); }
        let y = 720.0;
        self.button(ui, out, "Open maps folder", x, y, 540.0, 48.0, Act::Extras(A::MapsFolder), cursor);
        match crate::map_share::MESSAGE.lock().unwrap().clone() {
            Some((m, ok)) => { self.para(ui, out, &m, x, y + 60.0, 540.0, 18.0, if ok { OK } else { BAD }); }
            None => { self.para(ui, out, "Maps there, or a .map dropped on the window, show in the skirmish's map list. In a LAN game the host sends its map to players who lack it.", x, y + 60.0, 540.0, 18.0, DIM); }
        }
        self.button(ui, out, "Back", 840.0, 910.0, 240.0, 56.0, Act::Back, cursor);
    }
}

/// The main menu's Extras shield opens the screen (its buttons on it), Esc goes back; a .zip dropped on the window
/// elsewhere in the front end opens it.
#[test]
fn extras_screen_opens() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return }; // needs the exported UI
    skirmish::set_test_profile(std::env::temp_dir().join(format!("shc-extras-{}", std::process::id())));
    let mut m = Menu::main();
    m.draw(&mut u, 1920.0, 1080.0, [0.0; 2]);
    let p = m.hits.iter().find(|h| h.1 == Act::Extras(A::Open)).expect("the Extras shield").0;
    assert!(m.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]).is_none() && matches!(m.screen, Screen::Extras));
    m.draw(&mut u, 1920.0, 1080.0, [0.0; 2]);
    for a in [A::Check, A::Import, A::Folder, A::Notes] { assert!(m.hits.iter().any(|h| h.1 == Act::Extras(a)), "{a:?}"); }
    assert!(m.back().is_none() && matches!(m.screen, Screen::Main));
    crate::lord_share::SHOW.store(true, std::sync::atomic::Ordering::Relaxed);
    m.draw(&mut u, 1920.0, 1080.0, [0.0; 2]);
    assert!(matches!(m.screen, Screen::Extras), "a dropped zip shows its result there");
}
