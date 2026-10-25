//! The macOS menu bar (replaces winit's default app menu) and the quit question as a window sheet. Items AppKit does
//! itself (Hide, Minimize, Full Screen, ...) go to the responder chain; the game's items queue a `Cmd` that
//! ux.rs takes in about_to_wait, so nothing runs inside winit's event handler (no re-entrancy).

use std::{cell::RefCell, sync::Mutex};
use objc2::{declare_class, msg_send_id, mutability, rc::Retained, runtime::{AnyObject, Sel}, sel, ClassType, DeclaredClass};
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSMenu, NSMenuItem, NSModalResponse, NSView, NSWindow};
use objc2_foundation::{MainThreadMarker, NSObject, NSString};

use crate::ux::Cmd;
const CMDS: [Cmd; 13] = [Cmd::Quit, Cmd::Save, Cmd::Load, Cmd::Options, Cmd::Shortcuts, Cmd::UiBigger, Cmd::UiSmaller, Cmd::UiReset,
    Cmd::PauseInBackground, Cmd::QuitYes, Cmd::QuitNo, Cmd::FullScreen, Cmd::TrackpadPan];

static PENDING: Mutex<Vec<Cmd>> = Mutex::new(Vec::new());
/// The check-mark items: Game > Pause in Background, View > Trackpad Scrolling Pans.
#[derive(Clone, Copy)]
pub enum Check { PauseBg, TrackpadPan }
thread_local! { static CHECK_ITEMS: RefCell<[Option<Retained<NSMenuItem>>; 2]> = const { RefCell::new([None, None]) }; }

fn push(c: Cmd) { PENDING.lock().unwrap().push(c); }
/// The commands queued since the last call (main thread, about_to_wait).
pub fn take() -> Vec<Cmd> { std::mem::take(&mut *PENDING.lock().unwrap()) }

declare_class!(
    struct Target;
    unsafe impl ClassType for Target {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "ShcMenuTarget";
    }
    impl DeclaredClass for Target {}
    unsafe impl Target {
        #[method(shcMenu:)]
        fn shc_menu(&self, sender: &NSMenuItem) {
            if let Some(&c) = CMDS.get(unsafe { sender.tag() } as usize) { push(c) }
        }
    }
);

const CMD: NSEventModifierFlags = NSEventModifierFlags::NSEventModifierFlagCommand;

/// Our menu bar: app menu (About, Settings… ⌘,, Services, Hide, Quit ⌘Q), Game (Save ⌘S, Load… ⌘O, Pause in
/// Background), View (Toggle Full Screen ⌃⌘F, UI size ⌘= / ⌘- / ⌘0, Keyboard Shortcuts ⌘/), Window (Minimize ⌘M, Zoom,
/// Bring All to Front). `pause_bg` / `trackpad_pan` = the check marks' start state.
pub fn install_menu(pause_bg: bool, trackpad_pan: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    NSWindow::setAllowsAutomaticWindowTabbing(false, mtm); // one game window: no Show Tab Bar items in View
    let target: Retained<Target> = unsafe { msg_send_id![Target::class(), new] };
    let target: &'static Target = Box::leak(Box::new(target)); // menu items hold their target weakly
    let name = "Strong Hold"; // = bundle.sh's CFBundleName (the process itself is shc-viewer)
    let item = |menu: &NSMenu, title: &str, action: Option<Sel>, key: &str, mods: NSEventModifierFlags, cmd: Option<Cmd>| {
        let it = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &NSString::from_str(title), action, &NSString::from_str(key)) };
        it.setKeyEquivalentModifierMask(mods);
        if let Some(c) = cmd {
            unsafe {
                it.setAction(Some(sel!(shcMenu:)));
                it.setTarget(Some(target as &AnyObject));
                it.setTag(CMDS.iter().position(|&x| x == c).unwrap() as isize);
            }
        }
        menu.addItem(&it);
        it
    };
    let sep = |menu: &NSMenu| menu.addItem(&NSMenuItem::separatorItem(mtm));
    let submenu = |bar: &NSMenu, title: &str| {
        let m = unsafe { NSMenu::initWithTitle(mtm.alloc(), &NSString::from_str(title)) };
        let top = NSMenuItem::new(mtm);
        top.setSubmenu(Some(&m));
        bar.addItem(&top);
        m
    };
    let bar = NSMenu::new(mtm);
    let none = NSEventModifierFlags::empty();

    let a = submenu(&bar, "");
    item(&a, &format!("About {name}"), Some(sel!(orderFrontStandardAboutPanel:)), "", CMD, None);
    sep(&a);
    item(&a, "Settings…", None, ",", CMD, Some(Cmd::Options));
    sep(&a);
    let services = NSMenu::new(mtm);
    item(&a, "Services", None, "", none, None).setSubmenu(Some(&services));
    sep(&a);
    item(&a, &format!("Hide {name}"), Some(sel!(hide:)), "h", CMD, None);
    item(&a, "Hide Others", Some(sel!(hideOtherApplications:)), "h", CMD | NSEventModifierFlags::NSEventModifierFlagOption, None);
    item(&a, "Show All", Some(sel!(unhideAllApplications:)), "", CMD, None);
    sep(&a);
    item(&a, &format!("Quit {name}"), None, "q", CMD, Some(Cmd::Quit));

    let g = submenu(&bar, "Game");
    item(&g, "Save Game", None, "s", CMD, Some(Cmd::Save));
    item(&g, "Load Game…", None, "o", CMD, Some(Cmd::Load));
    sep(&g);
    let p = item(&g, "Pause in Background", None, "", CMD, Some(Cmd::PauseInBackground));
    unsafe { p.setState(if pause_bg { NSControlStateValueOn } else { NSControlStateValueOff }) };
    CHECK_ITEMS.with(|c| c.borrow_mut()[Check::PauseBg as usize] = Some(p));

    let v = submenu(&bar, "View");
    // main.rs set_windowed: native full screen (its own Space), back to an 80 % window
    item(&v, "Toggle Full Screen", None, "f", CMD | NSEventModifierFlags::NSEventModifierFlagControl, Some(Cmd::FullScreen));
    sep(&v);
    item(&v, "Bigger UI", None, "+", CMD, Some(Cmd::UiBigger));
    let eq = item(&v, "Bigger UI", None, "=", CMD, Some(Cmd::UiBigger)); // ⌘= without Shift on US layouts
    unsafe { eq.setHidden(true); eq.setAllowsKeyEquivalentWhenHidden(true); }
    item(&v, "Smaller UI", None, "-", CMD, Some(Cmd::UiSmaller));
    item(&v, "Actual Size UI", None, "0", CMD, Some(Cmd::UiReset));
    sep(&v);
    // off: a two-finger scroll (Magic Mouse too) steps the zoom like the original's wheel (gestures::pans)
    let t = item(&v, "Trackpad Scrolling Pans", None, "", CMD, Some(Cmd::TrackpadPan));
    unsafe { t.setState(if trackpad_pan { NSControlStateValueOn } else { NSControlStateValueOff }) };
    CHECK_ITEMS.with(|c| c.borrow_mut()[Check::TrackpadPan as usize] = Some(t));
    sep(&v);
    item(&v, "Keyboard Shortcuts", None, "/", CMD, Some(Cmd::Shortcuts));

    let w = submenu(&bar, "Window");
    item(&w, "Minimize", Some(sel!(performMiniaturize:)), "m", CMD, None);
    item(&w, "Zoom", Some(sel!(performZoom:)), "", CMD, None);
    sep(&w);
    item(&w, "Bring All to Front", Some(sel!(arrangeInFront:)), "", CMD, None);

    let h = submenu(&bar, "Help");
    item(&h, "Keyboard Shortcuts (F1)", None, "", CMD, Some(Cmd::Shortcuts));

    app.setMainMenu(Some(&bar));
    unsafe {
        app.setServicesMenu(Some(&services));
        app.setWindowsMenu(Some(&w));
        app.setHelpMenu(Some(&h));
    }
}

/// A check-mark item's state.
pub fn set_check(which: Check, on: bool) {
    CHECK_ITEMS.with(|c| if let Some(it) = &c.borrow()[which as usize] { unsafe { it.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff }) } });
}

/// "Quit while a game is running?" as a sheet on the game window; the answer arrives later as Cmd::QuitYes / QuitNo.
/// false = no window to attach it to (the caller quits).
pub fn ask_quit(window: &winit::window::Window) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Some(mtm) = MainThreadMarker::new() else { return false };
    // the game window itself: keyWindow / mainWindow are nil while the app is in the background (⌘Q from the menu bar)
    let Ok(RawWindowHandle::AppKit(h)) = window.window_handle().map(|h| h.as_raw()) else { return false };
    let view: &NSView = unsafe { h.ns_view.cast().as_ref() };
    let Some(win) = view.window() else { return false };
    unsafe {
        if win.isMiniaturized() { win.deminiaturize(None); }
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str("Quit the game?"));
        alert.setInformativeText(&NSString::from_str("The running game is lost unless you save it first (⌘S)."));
        alert.addButtonWithTitle(&NSString::from_str("Quit"));
        alert.addButtonWithTitle(&NSString::from_str("Cancel"));
        let done = block2::RcBlock::new(|r: NSModalResponse| push(if r == NSAlertFirstButtonReturn { Cmd::QuitYes } else { Cmd::QuitNo }));
        alert.beginSheetModalForWindow_completionHandler(&win, Some(&done));
    }
    true
}
