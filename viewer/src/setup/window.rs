//! The macOS setup window (setup/mod.rs): checks, then the game from this Mac or from Steam (QR code with the
//! Steam phone app, or user name + password + Steam Guard), the download, the export. Plain AppKit in our own event
//! loop (winit isn't running yet); buttons queue their tag like macos.rs' menu. Look: dark leather, gold Copperplate,
//! Baskerville, crimson buttons; once a copy of the game is on the Mac, its own loading-screen art fills the back.
use super::{steam, Dirs};
use objc2::{declare_class, msg_send_id, mutability, rc::Retained, runtime::AnyObject, sel, ClassType, DeclaredClass};
use objc2_app_kit::*;
use objc2_foundation::*;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

static CLICKS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

declare_class!(
    struct Target;
    unsafe impl ClassType for Target {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "ShcSetupTarget";
    }
    impl DeclaredClass for Target {}
    unsafe impl Target {
        /// Buttons and the Quit menu item (NSControl / NSMenuItem both answer `tag`).
        #[method(shcSetup:)]
        fn clicked(&self, sender: &AnyObject) { CLICKS.lock().unwrap().push(unsafe { objc2::msg_send![sender, tag] }); }
    }
);

// buttons (tags)
const USE_COPY: usize = 0;
const DOWNLOAD: usize = 1;
const CHOOSE: usize = 2;
const RECHECK: usize = 3;
const STORE: usize = 4;
const PASSWORD_MODE: usize = 5;
const QR_MODE: usize = 6;
const SIGN_IN: usize = 7;
const SEND_CODE: usize = 8;
const CANCEL: usize = 9;
const RETRY: usize = 10;
const LOG: usize = 11;
const QUIT: usize = 12;
const TITLES: [&str; 13] = ["Use This Copy", "Download from Steam", "Choose Game Folder…", "Check Again", "Store Page",
    "Sign In with Password Instead", "Use the QR Code Instead", "Sign In", "Continue", "Cancel", "Try Again", "Show Log", "Quit"];
/// Primary (crimson, Return) buttons.
const PRIMARY: [usize; 6] = [USE_COPY, DOWNLOAD, SIGN_IN, SEND_CODE, RETRY, RECHECK];

const W: f64 = 680.0;
const H: f64 = 560.0;

fn rgb(r: f64, g: f64, b: f64) -> Retained<NSColor> { unsafe { NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0) } }
fn ns(s: &str) -> Retained<NSString> { NSString::from_str(s) }
/// Frame from the top-left corner (AppKit counts from the bottom).
fn at(x: f64, y: f64, w: f64, h: f64) -> NSRect { NSRect::new(NSPoint::new(x, H - y - h), NSSize::new(w, h)) }
fn font(name: &str, size: f64) -> Retained<NSFont> {
    unsafe { NSFont::fontWithName_size(&ns(name), size).unwrap_or_else(|| NSFont::systemFontOfSize(size)) }
}

#[derive(PartialEq, Clone, Copy)]
enum SignIn { Starting, Qr, Password, Code, Approve }

#[derive(PartialEq)]
enum Stage { Check, SignIn(SignIn), Download, Extract, Failed }

struct Ui {
    win: Retained<NSWindow>,
    art: Retained<NSImageView>,
    body: Retained<NSTextField>,
    checks: Vec<Retained<NSTextField>>,
    qr: Retained<NSImageView>,
    labels: [Retained<NSTextField>; 3],
    fields: [Retained<NSTextField>; 3], // user, password (secure), code
    bar: Retained<NSProgressIndicator>,
    status: Retained<NSTextField>,
    note: Retained<NSTextField>,
    buttons: Vec<Retained<NSButton>>,
}

fn label(mtm: MainThreadMarker, view: &NSView, text: &str, f: Retained<NSFont>, color: &NSColor, frame: NSRect, wrap: bool) -> Retained<NSTextField> {
    let l = unsafe { if wrap { NSTextField::wrappingLabelWithString(&ns(text), mtm) } else { NSTextField::labelWithString(&ns(text), mtm) } };
    unsafe {
        l.setFont(Some(&f));
        l.setTextColor(Some(color));
        l.setFrame(frame);
    }
    unsafe { view.addSubview(&l); }
    l
}

impl Ui {
    fn new(mtm: MainThreadMarker, target: &'static Target, update: bool) -> Ui {
        let (title, sub) = if update { ("Strong Hold: New Game Files", "Definitive Edition, on your Mac. New game files.") }
            else { ("Strong Hold: First-Time Setup", "Definitive Edition, on your Mac. First-time setup.") };
        let gold = rgb(0.86, 0.71, 0.38);
        let parchment = rgb(0.93, 0.88, 0.76);
        let dim = rgb(0.72, 0.65, 0.52);
        let win = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(mtm.alloc(), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W, H)),
                NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable, NSBackingStoreType::NSBackingStoreBuffered, false)
        };
        unsafe {
            win.setReleasedWhenClosed(false);
            win.setTitle(&ns(title));
            win.setBackgroundColor(Some(&rgb(0.12, 0.085, 0.055)));
            if let Some(a) = NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua) { win.setAppearance(Some(&a)); }
        }
        let view = win.contentView().unwrap();
        let art = unsafe { NSImageView::initWithFrame(mtm.alloc(), at(0.0, 0.0, W, H)) };
        unsafe {
            art.setImageScaling(NSImageScaling::NSImageScaleAxesIndependently);
        }
        unsafe { view.addSubview(&art); }
        label(mtm, &view, "STRONGHOLD CRUSADER", font("Copperplate-Bold", 34.0), &gold, at(40.0, 28.0, 600.0, 44.0), false);
        label(mtm, &view, sub, font("Baskerville-Italic", 17.0), &parchment, at(42.0, 74.0, 600.0, 24.0), false);
        let body = label(mtm, &view, "", font("Baskerville", 16.0), &parchment, at(42.0, 112.0, 596.0, 84.0), true);
        let checks = (0..4).map(|i| label(mtm, &view, "", font("Baskerville", 15.0), &parchment, at(58.0, 204.0 + i as f64 * 34.0, 580.0, 30.0), true)).collect();
        let qr = unsafe { NSImageView::initWithFrame(mtm.alloc(), at(220.0, 200.0, 240.0, 240.0)) };
        unsafe { qr.setImageScaling(NSImageScaling::NSImageScaleProportionallyUpOrDown); }
        unsafe { view.addSubview(&qr); }
        let labels = ["Steam user name", "Password", "Steam Guard code"].map(|t| label(mtm, &view, t, font("Baskerville", 15.0), &gold, at(80.0, 0.0, 170.0, 24.0), false));
        let fields = [0, 1, 2].map(|i| {
            let f: Retained<NSTextField> = unsafe {
                if i == 1 { Retained::into_super(NSSecureTextField::initWithFrame(mtm.alloc(), at(260.0, 0.0, 300.0, 26.0))) }
                else { NSTextField::initWithFrame(mtm.alloc(), at(260.0, 0.0, 300.0, 26.0)) }
            };
            unsafe { f.setFont(Some(&NSFont::systemFontOfSize(14.0))); }
            unsafe { view.addSubview(&f); }
            f
        });
        let bar = unsafe { NSProgressIndicator::initWithFrame(mtm.alloc(), at(42.0, 230.0, 596.0, 20.0)) };
        unsafe {
            bar.setStyle(NSProgressIndicatorStyle::Bar);
            bar.setIndeterminate(false);
            bar.setMinValue(0.0);
            bar.setMaxValue(1.0);
        }
        unsafe { view.addSubview(&bar); }
        let status = label(mtm, &view, "", font("Baskerville", 15.0), &dim, at(42.0, 260.0, 596.0, 44.0), true);
        let note = label(mtm, &view, "", font("Baskerville-Italic", 13.0), &dim, at(42.0, 456.0, 596.0, 36.0), true);
        let buttons = TITLES.iter().enumerate().map(|(i, t)| {
            let b = unsafe { NSButton::buttonWithTitle_target_action(&ns(t), Some(target as &AnyObject), Some(sel!(shcSetup:)), mtm) };
            unsafe {
                b.setTag(i as isize);
                b.setFont(Some(&font("Baskerville-SemiBold", 15.0)));
            }
            unsafe { view.addSubview(&b); }
            b
        }).collect();
        win.center();
        win.makeKeyAndOrderFront(None);
        Ui { win, art, body, checks, qr, labels, fields, bar, status, note, buttons }
    }

    /// Show these buttons right-aligned on the bottom row (first = rightmost, the default for Return).
    fn buttons(&self, show: &[usize]) {
        let mut x = W - 40.0;
        for (i, b) in self.buttons.iter().enumerate() {
            let pos = show.iter().position(|&s| s == i);
            unsafe {
                b.setHidden(pos.is_none());
                let default = pos == Some(0) && PRIMARY.contains(&i); // crimson, Return
                b.setKeyEquivalent(&ns(if default { "\r" } else { "" }));
                b.setBezelColor(if default { Some(rgb(0.55, 0.12, 0.09)) } else { None }.as_deref());
            }
        }
        for &i in show {
            let b = &self.buttons[i];
            let w = unsafe { b.fittingSize() }.width.max(110.0) + 16.0;
            x -= w;
            unsafe { b.setFrame(at(x, 506.0, w, 34.0)); }
            x -= 10.0;
        }
    }

    fn text(l: &NSTextField, s: &str) { unsafe { l.setStringValue(&ns(s)); } }

    /// Which parts are visible: checklist, QR, fields (bit 0 user, 1 password, 2 code), progress.
    fn show(&self, checks: bool, qr: bool, fields: u8, bar: bool) {
        unsafe {
            for c in &self.checks { c.setHidden(!checks); }
            self.qr.setHidden(!qr);
            let mut y = 214.0;
            for i in 0..3 {
                let on = fields & (1 << i) != 0;
                self.labels[i].setHidden(!on);
                self.fields[i].setHidden(!on);
                if on {
                    self.labels[i].setFrame(at(80.0, y + 2.0, 170.0, 24.0));
                    self.fields[i].setFrame(at(260.0, y, 300.0, 26.0));
                    y += 44.0;
                }
            }
            self.bar.setHidden(!bar);
            self.status.setHidden(!bar);
        }
    }

    fn image(v: &NSImageView, bmp: &[u8]) {
        let data = NSData::with_bytes(bmp);
        let img = NSImage::initWithData(NSImage::alloc(), &data);
        unsafe { v.setImage(img.as_deref()); }
    }
}

/// The game's own loading-screen art (the crusader before the walls) behind the window, cropped to the window from
/// the left, darkened so the text stays readable. From the user's files: nothing ships with the app.
fn art_of(game: &Path) -> Option<Vec<u8>> {
    let t = shc_extract::tex::Textures::open(&game.join(shc_extract::GAME_DATA), &["sharedassets1.assets"]).ok()?;
    let im = t.get("loading background").ok()?;
    let cw = ((im.h as f64 * W / H) as usize).min(im.w);
    let im = im.crop(0, 0, cw, im.h).resize((W * 2.0) as usize, (H * 2.0) as usize);
    Some(super::bmp(im.w, im.h, |x, y| {
        let p = &im.px[(y * im.w + x) * 4..];
        let k = 0.30 - 0.10 * (1.0 - (y as f64 / im.h as f64 * 2.0 - 1.0).abs()); // darker band behind the text
        [p[0], p[1], p[2]].map(|c| (c as f64 * k) as u8)
    }))
}

/// Where a game folder is, in words.
fn place(g: &Path) -> String {
    let s = g.to_string_lossy();
    if g == crate::paths::downloaded() { "downloaded by this app earlier".into() }
    else if s.contains("/CrossOver/") { "installed with Steam in CrossOver".into() }
    else { s.replace(&crate::paths::home().to_string_lossy().into_owned(), "~") }
}

fn qr_bmp(rows: &[Vec<bool>]) -> Vec<u8> {
    let (n, s) = (rows.len(), 8); // 8 px per module: sharp after the view's scaling
    super::bmp(n * s, n * s, |x, y| if rows.get(y / s).and_then(|r| r.get(x / s)).copied().unwrap_or(false) { [0; 3] } else { [255; 3] })
}

/// The Quit item (⌘Q, through `target`: a running download is stopped first) and the Edit menu (⌘V into the fields).
fn menu(mtm: MainThreadMarker, app: &NSApplication, target: &'static Target) {
    let bar = NSMenu::new(mtm);
    let add = |title: &str, items: &[(&str, objc2::runtime::Sel, &str)]| {
        let m = unsafe { NSMenu::initWithTitle(mtm.alloc(), &ns(title)) };
        for (t, a, k) in items { m.addItem(&*unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &ns(t), Some(*a), &ns(k)) }); }
        let top = NSMenuItem::new(mtm);
        top.setSubmenu(Some(&m));
        bar.addItem(&top);
    };
    add("", &[]);
    let quit = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &ns("Quit Strong Hold"), Some(sel!(shcSetup:)), &ns("q")) };
    unsafe { quit.setTarget(Some(target as &AnyObject)); quit.setTag(QUIT as isize); }
    unsafe { bar.itemAtIndex(0).and_then(|i| i.submenu()) }.unwrap().addItem(&quit);
    add("Edit", &[("Cut", sel!(cut:), "x"), ("Copy", sel!(copy:), "c"), ("Paste", sel!(paste:), "v"), ("Select All", sel!(selectAll:), "a")]);
    app.setMainMenu(Some(&bar));
}

struct Checks { free: Option<u64>, online: Option<bool> }

/// Runs the window until the data is ready (true) or the user quits (false). `update`: the main menu's "New game
/// files": sign in, fetch only those into the earlier download, back to the game (true) also on Cancel / close; the
/// export of the new videos runs in the game (mod.rs more()).
pub fn run(d: &Dirs, update: Option<&super::Update>) -> bool {
    let mtm = MainThreadMarker::new().expect("setup runs on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let target: Retained<Target> = unsafe { msg_send_id![Target::class(), new] };
    let target: &'static Target = Box::leak(Box::new(target)); // buttons and menu items hold their target weakly
    menu(mtm, &app, target);
    unsafe { app.finishLaunching(); }
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    let ui = Ui::new(mtm, target, update.is_some());
    let gold = rgb(0.86, 0.71, 0.38);
    let red = rgb(0.90, 0.38, 0.30);
    let dl_dir = crate::paths::downloaded();
    let _ = std::fs::create_dir_all(crate::paths::app_data());
    let mut filelist = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.parent()?.join("Resources/steam_files.txt"))).filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/steam_files.txt")));
    // update: only the missing files; a new pinned build refreshes all (DepotDownloader skips what's unchanged)
    if let Some(u) = update.filter(|u| !u.new_build) {
        let f = crate::paths::app_data().join("steam_update_files.txt");
        if std::fs::write(&f, super::filelist(&u.files)).is_ok() { filelist = f; }
    }
    let mb = update.map_or(0, |u| u.bytes.div_ceil(1_000_000));

    let mut stage = Stage::Check;
    let mut found = super::find_game();
    let mut art_for: Option<PathBuf> = None;
    let (check_tx, check_rx) = mpsc::channel::<bool>();
    let mut checks = Checks { free: super::free_bytes(&crate::paths::app_data()), online: None };
    let recheck = |tx: &mpsc::Sender<bool>| { let tx = tx.clone(); std::thread::spawn(move || { let _ = tx.send(super::online()); }); };
    recheck(&check_tx);
    let mut dl: Option<steam::Download> = None;
    let mut pinned = true;
    let mut password: Option<String> = None;
    let mut extract: Option<Arc<Mutex<(String, f32, Option<Result<(), String>>)>>> = None;
    let mut error = String::new();
    let mut depot = (1, 0.0f32);
    let mut dirty = true;
    let mut shots = 0;
    // dev/test: SHC_SETUP_CLICK="Use This Copy,…" presses those buttons in turn, each once it is visible
    let mut auto: Vec<usize> = std::env::var("SHC_SETUP_CLICK").unwrap_or_default().split(',')
        .filter_map(|t| TITLES.iter().position(|x| x.eq_ignore_ascii_case(t.trim()))).rev().collect();

    loop {
        // AppKit events
        loop {
            let ev = unsafe { app.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, Some(&NSDate::dateWithTimeIntervalSinceNow(0.03)), NSDefaultRunLoopMode, true) };
            match ev { Some(e) => unsafe { app.sendEvent(&e) }, None => break }
        }
        if !ui.win.isVisible() { return update.is_some(); } // closed
        if let Ok(o) = check_rx.try_recv() { checks.online = Some(o); dirty = true; }
        if art_for != found.clone() {
            art_for = found.clone();
            if let Some(b) = found.as_deref().and_then(art_of) { Ui::image(&ui.art, &b); }
        }
        let start_extract = |game: PathBuf| {
            let st = Arc::new(Mutex::new(("Starting".to_string(), 0.0f32, None)));
            let (s2, cache, audio) = (st.clone(), d.cache.clone(), d.audio.clone());
            std::thread::spawn(move || {
                let r = shc_extract::run(&game, &cache, &audio, &mut |l, f| { let mut g = s2.lock().unwrap(); g.0 = l.to_string(); g.1 = f; });
                if let Err(e) = &r { eprintln!("setup: export failed: {e}"); }
                s2.lock().unwrap().2 = Some(r.map_err(|e| e.to_string()));
            });
            st
        };
        let start_dl = |login: steam::Login, pinned: bool| -> Result<steam::Download, String> {
            steam::Download::start(&dl_dir, &filelist, login, pinned).map_err(|e| format!("Couldn't start Valve's DepotDownloader ({}): {e}", steam::tool().display()))
        };

        if let Some(&b) = auto.last() { if !dirty && unsafe { !ui.buttons[b].isHidden() } { auto.pop(); CLICKS.lock().unwrap().push(b as isize); } }
        // clicks
        for c in std::mem::take(&mut *CLICKS.lock().unwrap()) {
            dirty = true;
            match c as usize {
                USE_COPY => if let Some(g) = found.clone() { extract = Some(start_extract(g)); stage = Stage::Extract; },
                DOWNLOAD | QR_MODE => match start_dl(steam::Login::Qr, pinned) {
                    Ok(x) => { dl = Some(x); stage = Stage::SignIn(SignIn::Starting); }
                    Err(e) => { error = e; stage = Stage::Failed; }
                },
                PASSWORD_MODE => { dl = None; stage = Stage::SignIn(SignIn::Password); ui.win.makeFirstResponder(Some(&ui.fields[0])); }
                SIGN_IN => {
                    let user = unsafe { ui.fields[0].stringValue() }.to_string();
                    let pass = unsafe { ui.fields[1].stringValue() }.to_string();
                    unsafe { ui.fields[1].setStringValue(&ns("")); }
                    if user.trim().is_empty() || pass.is_empty() { Ui::text(&ui.status, "Enter your Steam user name and password."); continue; }
                    match start_dl(steam::Login::Password(user.trim().to_string()), pinned) {
                        Ok(x) => { dl = Some(x); password = Some(pass); stage = Stage::SignIn(SignIn::Starting); }
                        Err(e) => { error = e; stage = Stage::Failed; }
                    }
                }
                SEND_CODE => {
                    let code = unsafe { ui.fields[2].stringValue() }.to_string();
                    unsafe { ui.fields[2].setStringValue(&ns("")); }
                    if let Some(x) = &mut dl { x.answer(code.trim()); }
                    stage = Stage::SignIn(SignIn::Starting);
                }
                CHOOSE => {
                    let p = unsafe { NSOpenPanel::openPanel(mtm) };
                    unsafe {
                        p.setCanChooseDirectories(true);
                        p.setCanChooseFiles(false);
                        p.setMessage(Some(&ns("Choose the \"Stronghold Crusader Definitive Edition\" folder (copied from a Windows PC or another Mac)")));
                    }
                    if unsafe { p.runModal() } == NSModalResponseOK {
                        let path = unsafe { p.URL() }.and_then(|u| unsafe { u.path() }).map(|s| PathBuf::from(s.to_string()));
                        match path.as_deref().and_then(super::game_in) {
                            Some(g) => { let _ = std::fs::write(crate::paths::chosen_file(), g.to_string_lossy().as_bytes()); found = Some(g.clone()); extract = Some(start_extract(g)); stage = Stage::Extract; }
                            None => Ui::text(&ui.status, "That folder doesn't hold the game. Choose the one named \"Stronghold Crusader Definitive Edition\"."),
                        }
                    }
                }
                RECHECK | RETRY => { dl = None; found = super::find_game(); checks.free = super::free_bytes(&crate::paths::app_data()); checks.online = None; recheck(&check_tx); stage = Stage::Check; }
                STORE => if let Some(u) = unsafe { NSURL::URLWithString(&ns(steam::STORE)) } { unsafe { NSWorkspace::sharedWorkspace().openURL(&u); } },
                CANCEL if update.is_some() && stage == Stage::Check => return true,
                CANCEL => { dl = None; stage = Stage::Check; }
                LOG => {
                    let log = crate::paths::home().join("Library/Logs/Strong Hold.log");
                    unsafe { NSWorkspace::sharedWorkspace().openURL(&NSURL::fileURLWithPath(&ns(&log.to_string_lossy()))); }
                }
                QUIT => return false,
                _ => {}
            }
        }

        // the download
        let mut evs = vec![];
        if let Some(x) = &dl { while let Ok(e) = x.rx.try_recv() { evs.push(e); } }
        for e in evs {
            dirty = true;
            match e {
                steam::Ev::Qr(rows) => { Ui::image(&ui.qr, &qr_bmp(&rows)); stage = Stage::SignIn(SignIn::Qr); }
                steam::Ev::Password => if let (Some(x), Some(p)) = (&mut dl, password.take()) { x.answer(&p); },
                steam::Ev::TwoFactor | steam::Ev::EmailCode => { stage = Stage::SignIn(SignIn::Code); ui.win.makeFirstResponder(Some(&ui.fields[2])); }
                steam::Ev::WrongCode => { Ui::text(&ui.status, "That code didn't work. Enter the current one."); stage = Stage::SignIn(SignIn::Code); }
                steam::Ev::ApproveOnPhone => stage = Stage::SignIn(SignIn::Approve),
                steam::Ev::LoggedIn => { stage = Stage::Download; depot = (1, 0.0); }
                steam::Ev::Progress(n, f) => { stage = Stage::Download; depot = (n, f); }
                steam::Ev::Exit(r) => {
                    let ok = dl.as_mut().is_some_and(|x| x.wait_ok());
                    dl = None;
                    match r {
                        Ok(()) if ok && update.is_some() => { super::mark_build(pinned); ui.win.orderOut(None); return true; }
                        Ok(()) if ok && super::is_game(&dl_dir) => { super::mark_build(pinned); found = Some(dl_dir.clone()); extract = Some(start_extract(dl_dir.clone())); stage = Stage::Extract; }
                        Err(steam::Fail::ManifestGone) if pinned => {
                            // the supported build is gone from Steam: try the current one (the export checks the DLL)
                            pinned = false;
                            error = "Steam needs you to sign in once more.".into();
                            stage = Stage::SignIn(SignIn::Password);
                            if let Ok(x) = start_dl(steam::Login::Qr, false) { dl = Some(x); stage = Stage::SignIn(SignIn::Starting); }
                        }
                        Ok(()) => { error = "The download finished, but the game's files are incomplete. Try again.".into(); stage = Stage::Failed; }
                        Err(f) => { error = f.message(); stage = Stage::Failed; }
                    }
                }
            }
        }
        // the export
        if let Some(st) = &extract {
            let (label, f, done) = st.lock().unwrap().clone();
            unsafe { ui.bar.setDoubleValue(f as f64); }
            Ui::text(&ui.status, &format!("{label}…  {:.0} %", f * 100.0));
            match done {
                Some(Ok(())) => { ui.win.orderOut(None); return true; }
                Some(Err(e)) => {
                    extract = None;
                    error = if e.contains("game version") {
                        "This copy of the game is a different version than the one this app was made for. A newer version of this app will support it.".into()
                    } else { format!("Couldn't read the game's files: {e}") };
                    stage = Stage::Failed;
                    dirty = true;
                }
                None => {}
            }
        }
        if stage == Stage::Download {
            unsafe { ui.bar.setDoubleValue(depot.1 as f64); }
            let what = if update.is_some() { "the new game files" } else { "your game" };
            Ui::text(&ui.status, &format!("Downloading {what} from Steam (part {} of 2)…  {:.0} %", depot.0.min(2), depot.1 * 100.0));
        }

        if !dirty { continue; }
        dirty = false;
        let mark = |l: &NSTextField, ok: Option<bool>, text: &str| {
            Ui::text(l, &format!("{}  {text}", match ok { Some(true) => "✓", Some(false) => "✗", None => "•" }));
            unsafe { l.setTextColor(Some(if ok == Some(false) { &red } else { &gold })); }
        };
        match stage {
            Stage::Check if update.is_some() => {
                let need = update.map_or(0, |u| u.bytes) + 1_000_000_000; // + the new videos' export
                let space_ok = checks.free.map(|f| f >= need);
                Ui::text(&ui.body, &if update.is_some_and(|u| u.new_build) {
                    "This version of Strong Hold uses a newer version of the game. Setup updates your downloaded copy from Steam with your account; only what changed is downloaded.".to_string()
                } else {
                    format!("This version of Strong Hold uses more of the game's files: videos and pictures, {mb} MB. Setup downloads only those from Steam with your account, then the game goes on.")
                });
                mark(&ui.checks[0], Some(true), "Your downloaded copy of the game");
                mark(&ui.checks[1], space_ok, &format!("Free disk space: {:.1} GB (needs about {:.1} GB)", checks.free.unwrap_or(0) as f64 / 1e9, need as f64 / 1e9));
                mark(&ui.checks[2], checks.online, match checks.online { None => "Internet connection: checking…", Some(true) => "Internet connection", Some(false) => "No connection to Steam: check your internet" });
                Ui::text(&ui.checks[3], "");
                let ok = space_ok != Some(false) && checks.online == Some(true);
                ui.buttons(if ok { &[DOWNLOAD, CANCEL] } else { &[RECHECK, CANCEL] });
                Ui::text(&ui.note, "Cancel plays on without them: the game works as before, and the main menu offers them again.");
                ui.show(true, false, 0, false);
            }
            Stage::Check => {
                let space_ok = checks.free.map(|f| f >= super::needed(found.is_none()));
                let gb = |b: u64| b as f64 / 1e9;
                if let Some(g) = &found {
                    Ui::text(&ui.body, &format!("Your copy of Stronghold Crusader: Definitive Edition is already on this Mac ({}). Setup reads its graphics, sounds and maps once, in about 3 minutes.", place(g)));
                    mark(&ui.checks[0], Some(true), "Game files found");
                    mark(&ui.checks[1], space_ok, &format!("Free disk space: {:.1} GB (needs about 3.4 GB)", gb(checks.free.unwrap_or(0))));
                    Ui::text(&ui.checks[2], ""); Ui::text(&ui.checks[3], "");
                    ui.buttons(if space_ok == Some(false) { &[RECHECK, QUIT] } else { &[USE_COPY, DOWNLOAD, QUIT] });
                } else {
                    Ui::text(&ui.body, "This app plays with the graphics, sounds and maps of your own copy of Stronghold Crusader: Definitive Edition. Setup downloads it from Steam with your account, once.");
                    mark(&ui.checks[0], None, "A Steam account that owns Stronghold Crusader: Definitive Edition");
                    mark(&ui.checks[1], space_ok, &format!("Free disk space: {:.1} GB (needs about 9.5 GB)", gb(checks.free.unwrap_or(0))));
                    mark(&ui.checks[2], checks.online, match checks.online { None => "Internet connection: checking…", Some(true) => "Internet connection", Some(false) => "No connection to Steam: check your internet" });
                    mark(&ui.checks[3], None, "Optional: the Steam app on your phone, to sign in by scanning a code");
                    let ok = space_ok != Some(false) && checks.online == Some(true);
                    ui.buttons(if ok { &[DOWNLOAD, CHOOSE, STORE] } else { &[RECHECK, CHOOSE, STORE] }); // the close button quits
                }
                Ui::text(&ui.note, "Nothing from the original game comes with this app: it uses your own copy. The Steam app doesn't need to be installed.");
                ui.show(true, false, 0, false);
            }
            Stage::SignIn(m) => {
                let (body, fields, buttons): (&str, u8, &[usize]) = match m {
                    SignIn::Starting => ("Connecting to Steam…", 0, &[CANCEL]),
                    SignIn::Qr => ("Open the Steam app on your phone, tap the shield (Steam Guard), then scan this code. It refreshes by itself.", 0, &[PASSWORD_MODE, CANCEL]),
                    SignIn::Password => ("Sign in with your Steam account.", 0b011, &[SIGN_IN, QR_MODE, CANCEL]),
                    SignIn::Code => ("Steam Guard: enter the code from your Steam app or the email Steam sent you.", 0b100, &[SEND_CODE, CANCEL]),
                    SignIn::Approve => ("Steam Guard: open the Steam app on your phone and approve this sign-in.", 0, &[CANCEL]),
                };
                Ui::text(&ui.body, &if error.is_empty() { body.to_string() } else { format!("{error} {body}") });
                error.clear();
                ui.show(false, m == SignIn::Qr, fields, false);
                ui.buttons(buttons);
                Ui::text(&ui.note, "Your sign-in goes to Steam through Valve's DepotDownloader. The password is handed over once and never saved.");
            }
            Stage::Download => {
                Ui::text(&ui.body, &match update {
                    Some(u) if !u.new_build => format!("Signed in. Downloading the new game files ({mb} MB). If this stops, Try Again continues where it left off."),
                    Some(_) => "Signed in. Updating your copy of the game. If this stops, Try Again continues where it left off.".into(),
                    None => "Signed in. Downloading the game's files (about 5.7 GB). You can keep using your Mac; if this stops, Try Again continues where it left off.".into(),
                });
                ui.show(false, false, 0, true);
                ui.buttons(&[CANCEL]);
                Ui::text(&ui.note, "");
            }
            Stage::Extract => {
                Ui::text(&ui.body, "Preparing the graphics, sounds, maps and videos from your copy of the game. This takes a few minutes, once.");
                ui.show(false, false, 0, true);
                ui.buttons(&[QUIT]);
                Ui::text(&ui.note, "");
            }
            Stage::Failed => {
                Ui::text(&ui.body, &error);
                ui.show(false, false, 0, false);
                ui.buttons(if update.is_some() { &[RETRY, LOG, CANCEL] } else { &[RETRY, LOG, QUIT] });
                Ui::text(&ui.note, "");
            }
        }
        // dev/test: SHC_SETUP_SHOT=dir saves the window after each change as dir/setup-N.tiff (no screen recording needed)
        if let Some(dir) = std::env::var_os("SHC_SETUP_SHOT") {
            shots += 1;
            let v = ui.win.contentView().unwrap();
            unsafe {
                if let Some(rep) = v.bitmapImageRepForCachingDisplayInRect(v.bounds()) {
                    v.cacheDisplayInRect_toBitmapImageRep(v.bounds(), &rep);
                    if let Some(t) = rep.TIFFRepresentation() { let _ = std::fs::write(Path::new(&dir).join(format!("setup-{shots:02}.tiff")), t.bytes()); }
                }
            }
        }
    }
}
