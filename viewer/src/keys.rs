//! Hotkeys like the original's KeyManager: the binding table (two key slots per KeyFunctions entry, kept in
//! settings.cfg's `||KEYS||` block, KeyManager.SaveToString / LoadFromString), the Modern (DE) and Classic (SH1)
//! presets (re/data/hotkeys_de.txt / hotkeys_sh1.txt, generated from KeyManager.SetDefaultFunctions*), and the
//! pressed key as a Unity KeyCode name. A key is "Code[,shift][,ctrl][,alt][,mp]"; "" = not bound.

const MODERN: &str = include_str!("../../re/data/hotkeys_de.txt");
const CLASSIC: &str = include_str!("../../re/data/hotkeys_sh1.txt");
/// The port's own additions on top of the original's table (the preset files stay the original's): function, slot,
/// key. A new function gets a row; a key is only added while that slot is empty and nothing else holds the key.
/// ToggleMuteSounds is the original's (unbound there, and nothing handles it): the port gives it Ctrl+M.
const EXTRA: [(&str, usize, &str); 4] = [("QuickSave", 1, "F5"), ("QuickLoad", 0, "F9"), ("ToggleStats", 0, "F3"), ("ToggleMuteSounds", 0, "M,ctrl")];

#[derive(Clone, Debug, PartialEq)]
pub struct Keys { pub rows: Vec<(String, [String; 2])> } // KeyFunctions order

impl Keys {
    /// A preset as written (SetDefaultFunctionsNew / SetDefaultFunctionsSH1).
    fn preset(block: &str) -> Self {
        let rows = lines(block).map(|(f, a, b)| (f.to_string(), [a.to_string(), b.to_string()])).collect();
        let mut k = Keys { rows };
        k.extras();
        k
    }
    fn extras(&mut self) {
        for (f, slot, key) in EXTRA {
            if !self.rows.iter().any(|r| r.0 == f) { self.rows.push((f.to_string(), Default::default())); }
            if self.get(f)[slot].is_empty() && self.owner(key).is_none() { self.set(f, slot, key); }
        }
    }
    pub fn modern() -> Self { Self::preset(MODERN) }
    pub fn classic() -> Self { Self::preset(CLASSIC) }

    /// LoadFromString: the Modern defaults, then every saved line in order: its keys come off all functions first,
    /// then it takes them (a line without a first key unbinds the function).
    pub fn from_block(block: &str) -> Self {
        let mut k = Self::modern();
        if block.split("||KEYS||\n").count() != 3 { return k; }
        for (f, a, b) in lines(block) {
            let Some(i) = k.rows.iter().position(|r| r.0 == f) else { continue };
            let (a, b) = if a.is_empty() { ("", "") } else { (a, b) };
            for key in [a, b].into_iter().filter(|s| !s.is_empty()) { k.take(key); }
            k.rows[i].1 = [a.to_string(), b.to_string()];
        }
        k.extras();
        k
    }
    /// The app's bindings (settings.cfg; the Modern defaults without a KEYS block).
    pub fn load() -> Self { Self::from_block(&crate::skirmish::Settings::load().keys) }

    /// KeyManager.SaveToString: `Name:` + each bound key + `:` per function, `||KEYS||` around.
    pub fn text(&self) -> String {
        let body: String = self.rows.iter().map(|(f, [a, b])| {
            let keys: String = [a, b].into_iter().filter(|s| !s.is_empty()).map(|s| format!("{s}:")).collect();
            format!("{f}:{keys}\n")
        }).collect();
        format!("||KEYS||\n{body}||KEYS||\n")
    }

    pub fn get(&self, f: &str) -> [&str; 2] {
        self.rows.iter().find(|r| r.0 == f).map_or(["", ""], |r| [r.1[0].as_str(), r.1[1].as_str()])
    }
    /// The function `key` is bound to (either slot), if any.
    pub fn owner(&self, key: &str) -> Option<&str> {
        self.rows.iter().find(|r| r.1.iter().any(|k| !k.is_empty() && k == key)).map(|r| r.0.as_str())
    }
    /// KeyManager.SetNewKey: `key` comes off every function, then goes into `f`'s slot (`""` clears the slot).
    pub fn set(&mut self, f: &str, slot: usize, key: &str) {
        if !key.is_empty() { self.take(key); }
        if let Some(r) = self.rows.iter_mut().find(|r| r.0 == f) { r.1[slot] = key.to_string(); }
    }
    fn take(&mut self, key: &str) {
        for r in &mut self.rows { for k in &mut r.1 { if k == key { k.clear(); } } }
    }

    /// The function a key press triggers in single player (",mp" bindings only count in multiplayer).
    pub fn action(&self, key: &str) -> Option<&str> {
        self.rows.iter().find(|r| r.1.iter().any(|k| !k.is_empty() && !k.ends_with(",mp") && k == key)).map(|r| r.0.as_str())
    }
    /// Whether `f` has a binding on the key `code` with any modifiers (a held key's release).
    pub fn on_code(&self, f: &str, code: &str) -> bool {
        self.get(f).iter().any(|k| k.split(',').next() == Some(code))
    }
}

/// KeyFunctions value of `f`: the preset lists the enum in order, row 0 = "0" (Left = 1 .. MPPing = 202).
pub fn code(f: &str) -> Option<i32> { lines(MODERN).position(|l| l.0 == f).map(|i| i as i32) }

/// Whether a key on `f` does something in the port (main.rs `hotkey`); the F1 sheet leaves F1 / Shift+F1 to these.
pub fn runs(f: &str) -> bool {
    const RUN: [&str; 23] = ["Pause", "IncreaseEngineSpeed", "DecreaseEngineSpeed", "ZoomIn", "ZoomOut", "MapRotateLeft", "MapRotateRight",
        "QuickSave", "QuickLoad", "ToggleStats", "Save", "Load", "FlattenLandscape", "ToggleUI", "ToggleFrameRate", "ToggleMuteSounds",
        "RotateBuilding", "Up", "Down", "Left", "Right", "OptionsMenu", "Cathedral"];
    RUN.contains(&f) || PLACE.iter().any(|p| p.0 == f) || game_action(f, &Ctx { mode: 14, sub: 61, sh1_centre: true, attack_here: true, centre: None }).is_some()
}

/// KeyManager.buildingPlaceHotKeys: function -> eMappers id of the build tool it picks (all unbound but PlaceWalls).
pub const PLACE: [(&str, i32); 78] = [("PlaceWalls", 25), ("PlaceStairs", 27), ("PlaceLowWalls", 46), ("PlaceCrenal", 26), ("PlaceBarracks", 87),
    ("PlaceMercPost", 86), ("PlaceBedouinStockade", 79), ("PlaceArmoury", 81), ("PlaceTower1", 110), ("PlaceTower2", 111), ("PlaceTower3", 112),
    ("PlaceTower4", 113), ("PlaceTower5", 114), ("PlaceEngineersGuild", 88), ("PlaceTunnelGuild", 89), ("PlaceBallista", 211), ("PlaceMangonel", 210),
    ("PlaceStables", 65), ("PlaceSmelter", 180), ("PlaceSmallGatehouse", 144), ("PlaceLargeGatehouse", 146), ("PlaceDrawbridge", 105),
    ("PlaceDogCage", 312), ("PlacePitchDitch", 99), ("PlaceKillingPit", 98), ("PlaceDigMoat", 106), ("PlaceClearMoat", 107), ("PlaceBrazier", 148),
    ("PlaceStockpile", 52), ("PlaceWoodcutter", 51), ("PlaceQuarry", 56), ("PlaceOxen", 55), ("PlaceIronMine", 90), ("PlacePitchRig", 91),
    ("PlaceMarket", 77), ("PlaceHunter", 78), ("PlaceDairyFarm", 73), ("PlaceAppleFarm", 72), ("PlaceWheatFarm", 70), ("PlaceHopsFarm", 71),
    ("PlaceHouse", 54), ("PlaceChurchMosque1", 95), ("PlaceChurchMosque2", 96), ("PlaceChurchMosque3", 97), ("PlaceApothecary", 93),
    ("PlaceWell", 330), ("PlaceWaterpot", 342), ("PlaceFletcher", 50), ("PlacePoleturner", 82), ("PlaceBlacksmith", 83), ("PlaceTanner", 85),
    ("PlaceArmourer", 84), ("PlaceGranary", 80), ("PlaceBaker", 75), ("PlaceMill", 74), ("PlaceBrewer", 76), ("PlaceInn", 92), ("PlaceMaypole", 175),
    ("PlaceDancingBear", 324), ("PlaceGardens1", 160), ("PlaceGardens2", 166), ("PlaceGardens3", 169), ("PlaceStatue", 313), ("PlaceShrine", 318),
    ("PlaceFlag1", 120), ("PlaceFlag2", 121), ("PlaceFlag3", 122), ("PlaceFlag4", 123), ("PlaceGallows", 176), ("PlaceCesspit", 301),
    ("PlaceStocks", 177), ("PlaceHeads", 129), ("PlaceBurningStake", 305), ("PlaceDungeon", 307), ("PlaceRack", 308), ("PlaceGibbett", 306),
    ("PlaceChoppingBlock", 310), ("PlaceDunkingStool", 311)];

/// What KeyManager.Update reads before a key's DLL_GameAction: the HUD's app mode / sub mode, Options > Controls
/// "SH1 centre controls" (ConfigSettings.Settings_SH1CentreControls), troops_show_attack_here_and_type, and the
/// tile under the window centre (SetBookmark; None = off the map).
pub struct Ctx { pub mode: i32, pub sub: i32, pub sh1_centre: bool, pub attack_here: bool, pub centre: Option<[i32; 2]> }

/// KeyManager.Update's DLL_GameAction(action, a, b) for function `f` (directSentFunctions, the stance / stop /
/// attack-here / patrol keys), None = nothing to send. Camera keys answer y * 1000 + x (live.rs centres on it).
pub fn game_action(f: &str, c: &Ctx) -> Option<[i32; 3]> {
    let k = code(f)?;
    let troops = c.mode == 14 && matches!(c.sub, 61 | 62);
    let panel = |subs: &[i32]| c.mode == 16 && subs.contains(&c.sub);
    // SH1 centre controls: the camera jumps to the building; DE's: its panel opens (SelectBuildingType 1047), the
    // jump comes with a second press while that panel is up
    let jump = |kind: i32, subs: &[i32]| Some(if c.sh1_centre || panel(subs) { [k, -1, 0] } else { [1047, kind, -1] });
    match f {
        "HomeKeep" => jump(40, &[2]),
        "Market" => jump(26, &[25, 53, 54, 55, 56, 57]),
        "Barracks" => jump(9, &[1]),
        "MercPost" => jump(8, &[44]),
        "BedouinStockade" => jump(108, &[63]),
        "Granary" => jump(19, &[4]),
        "EngineersGuild" => jump(24, &[23]),
        "TunnelersGuild" => jump(25, &[24]),
        "Armoury" => jump(11, &[12]),
        "Cathedral" => (!c.sh1_centre && !panel(&[96])).then_some([1047, 38, -1]),
        "Signpost" | "Lord" | "CycleLord" | "AllyToggle" | "MPPing" => Some([k, -1, 0]),
        // a barracks / merc post / stockade / guild / cathedral panel: the number keys pick its assembly-point
        // tools there. ponytail: those tools aren't ported, so the key does nothing in those panels
        _ if f.starts_with("SelectClan") => (!(c.mode == 16 && matches!(c.sub, 1 | 23 | 24 | 44 | 63 | 96))).then_some([k, -1, 0]),
        _ if f.starts_with("GroupTroops") || f.starts_with("GotoBookmark") => Some([k, -1, 0]),
        _ if f.starts_with("SetBookmark") => Some(c.centre.map_or([k, -1, -1], |[x, y]| [k, x, y])),
        _ if f.starts_with("ExtremePower") => Some([1068, k - 100, k - 100]),
        // Troops_ChangeStance 287 stand / 288 defensive / 289 aggressive, Troops_Stop, Troops_AttackHere: troops panel only
        "StanceStand" | "StanceDefensive" | "StanceAggressive" => troops.then_some([1020, -1, 287 + k - 16]),
        "Stop" => troops.then_some([1010, 0, 0]),
        "AttackHere" => (troops && c.attack_here).then_some([1012, 0, 0]),
        // EditorDirector.placeBuilding(MAPPER_PATROL): what the troops panel's Patrol button does (Troops_Patrol, tool 0x22)
        "Patrol" => Some([1019, 0, 0]),
        _ => None,
    }
}

/// The `||KEYS||` block's lines: (function, key 1, key 2).
fn lines(block: &str) -> impl Iterator<Item = (&str, &str, &str)> {
    block.split("||KEYS||\n").nth(1).unwrap_or("").lines().filter_map(|l| {
        let mut p = l.split(':');
        Some((p.next()?, p.next().unwrap_or(""), p.next().unwrap_or("")))
    })
}

/// A key event as the original's KeyCode name with ",shift" / ",ctrl" / ",alt" (KeyManager folds the held modifiers
/// in). Letters and signs by what the key types without Shift (the layout's labels), the numeric keypad by location.
/// None for keys the original ignores (the modifiers themselves, Escape is the menu key).
pub fn unity_key(ev: &winit::event::KeyEvent, mods: winit::keyboard::ModifiersState) -> Option<String> {
    use winit::keyboard::{Key, KeyLocation, NamedKey as N};
    #[cfg(target_os = "macos")]
    let key = { use winit::platform::modifier_supplement::KeyEventExtModifierSupplement; ev.key_without_modifiers() };
    #[cfg(not(target_os = "macos"))]
    let key = ev.logical_key.clone();
    let pad = ev.location == KeyLocation::Numpad;
    let code = match &key {
        Key::Character(c) => char_code(c, pad)?,
        Key::Named(n) => match n {
            N::Space => "Space".into(), N::Enter => if pad { "KeypadEnter".into() } else { "Return".into() }, N::Tab => "Tab".into(),
            N::Backspace => "Backspace".into(), N::Delete => "Delete".into(), N::Insert => "Insert".into(), N::Home => "Home".into(),
            N::End => "End".into(), N::PageUp => "PageUp".into(), N::PageDown => "PageDown".into(), N::ArrowUp => "UpArrow".into(),
            N::ArrowDown => "DownArrow".into(), N::ArrowLeft => "LeftArrow".into(), N::ArrowRight => "RightArrow".into(),
            N::CapsLock => "CapsLock".into(), N::NumLock => "Numlock".into(),
            N::F1 => "F1".into(), N::F2 => "F2".into(), N::F3 => "F3".into(), N::F4 => "F4".into(), N::F5 => "F5".into(), N::F6 => "F6".into(),
            N::F7 => "F7".into(), N::F8 => "F8".into(), N::F9 => "F9".into(), N::F10 => "F10".into(), N::F11 => "F11".into(), N::F12 => "F12".into(),
            _ => return None,
        },
        _ => return None,
    };
    Some(with_mods(&code, mods.shift_key(), mods.control_key(), mods.alt_key()))
}

/// A typed character's KeyCode name ("a" -> "A", "3" -> "Alpha3", "=" -> "Equals"; keypad: "Keypad3", "KeypadPlus").
pub fn char_code(c: &str, pad: bool) -> Option<String> {
    let c = c.to_lowercase();
    let ch = c.chars().next()?;
    if pad {
        return Some(match ch { '0'..='9' => format!("Keypad{ch}"), '+' => "KeypadPlus".into(), '-' => "KeypadMinus".into(), '*' => "KeypadMultiply".into(),
            '/' => "KeypadDivide".into(), '.' | ',' => "KeypadPeriod".into(), '=' => "KeypadEquals".into(), _ => return None });
    }
    Some(match ch {
        'a'..='z' => ch.to_ascii_uppercase().to_string(),
        '0'..='9' => format!("Alpha{ch}"),
        '-' => "Minus".into(), '=' => "Equals".into(), '+' => "Plus".into(), '`' => "BackQuote".into(), '\\' => "Backslash".into(),
        '[' => "LeftBracket".into(), ']' => "RightBracket".into(), ';' => "Semicolon".into(), '\'' => "Quote".into(), ',' => "Comma".into(),
        '.' => "Period".into(), '/' => "Slash".into(),
        _ => return None,
    })
}

pub fn with_mods(code: &str, shift: bool, ctrl: bool, alt: bool) -> String {
    format!("{code}{}{}{}", if shift { ",shift" } else { "" }, if ctrl { ",ctrl" } else { "" }, if alt { ",alt" } else { "" })
}

#[test]
fn keys_load_save_rebind() {
    // the Modern preset is the live game's KEYS block (+ the port's extras) and survives a save / load
    let m = Keys::modern();
    assert_eq!(m.text(), MODERN.replace("QuickSave:Home:", "QuickSave:Home:F5:").replace("ToggleMuteSounds:\n", "ToggleMuteSounds:M,ctrl:\n")
        .replace("MPPing:P,alt:\n", "MPPing:P,alt:\nQuickLoad:F9:\nToggleStats:F3:\n"));
    assert_eq!((m.action("F5"), m.action("F9"), Keys::from_block("||KEYS||\nQuickSave:Home:\n||KEYS||\n").get("QuickSave")), (Some("QuickSave"), Some("QuickLoad"), ["Home", "F5"]));
    assert_eq!(Keys::from_block(&m.text()), m);
    assert_eq!((m.action("P"), m.action("Alpha1,ctrl"), m.action("Return")), (Some("Pause"), Some("GroupTroops1"), None)); // Return is ",mp"
    // SetNewKey: L leaves Lord for Pause; P is free afterwards
    let mut k = m.clone();
    k.set("Pause", 0, "L");
    assert_eq!((k.action("L"), k.action("P"), k.get("Lord")), (Some("Pause"), None, ["", ""]));
    let back = Keys::from_block(&k.text());
    assert_eq!(back, k);
    // a saved block only overrides what it names; Classic moves rotation to X / C
    let c = Keys::classic();
    assert_eq!((c.action("X"), c.action("C"), c.get("ZoomIn")), (Some("MapRotateLeft"), Some("MapRotateRight"), ["", ""]));
    assert_eq!(Keys::from_block("||KEYS||\nPause:K:\n||KEYS||\n").action("K"), Some("Pause"));
}

#[test]
fn keys_game_actions() {
    // KeyFunctions values (Enums.cs) and the sim's GameAction ranges (frontend::game_action)
    assert_eq!([code("Left"), code("HomeKeep"), code("GroupTroops0"), code("SelectClan0"), code("SetBookmark0"), code("GotoBookmark0"), code("Lord"), code("MercPost"), code("ExtremePower1"), code("MPPing")],
        [Some(1), Some(6), Some(0x13), Some(0x1d), Some(0x27), Some(0x31), Some(0x5c), Some(0x61), Some(100), Some(202)]);
    let m = Keys::modern();
    let field = Ctx { mode: 0, sub: 0, sh1_centre: true, attack_here: false, centre: Some([120, 250]) };
    let troops = Ctx { mode: 14, sub: 61, ..field };
    let go = |k: &str, c: &Ctx| m.action(k).and_then(|f| game_action(f, c));
    // jump keys (SH1 centre controls, the default): H keep, M market, B barracks, G granary, O signpost, L lord
    assert_eq!([go("H", &field), go("M", &field), go("B", &field), go("G", &field), go("O", &field), go("L", &field)],
        [Some([6, -1, 0]), Some([7, -1, 0]), Some([9, -1, 0]), Some([10, -1, 0]), Some([8, -1, 0]), Some([0x5c, -1, 0])]);
    // DE centre controls: the panel first, the jump from inside it
    let de = Ctx { sh1_centre: false, ..field };
    assert_eq!((go("M", &de), go("M", &Ctx { mode: 16, sub: 25, ..de }), go("H", &de)), (Some([1047, 26, -1]), Some([7, -1, 0]), Some([1047, 40, -1])));
    // groups: Ctrl+n assigns, n selects (a second press centres: the sim's select_clan), Ctrl+Alt+n / Alt+n bookmarks
    assert_eq!((go("Alpha3,ctrl", &field), go("Alpha3", &field), go("Alpha0", &field)), (Some([0x16, -1, 0]), Some([0x20, -1, 0]), Some([0x1d, -1, 0])));
    assert_eq!((go("Alpha2,ctrl,alt", &field), go("Alpha2,alt", &field)), (Some([0x29, 120, 250]), Some([0x33, -1, 0])));
    assert_eq!(go("Alpha1", &Ctx { mode: 16, sub: 1, ..field }), None); // barracks panel: assembly points (not ported)
    // stances R / T / Y, Stop (Backspace), Patrol (Shift+F7): troops panel only (patrol: always, like the original)
    assert_eq!([go("R", &troops), go("T", &troops), go("Y", &troops), go("Backspace", &troops), go("F7,shift", &field)],
        [Some([1020, -1, 287]), Some([1020, -1, 288]), Some([1020, -1, 289]), Some([1010, 0, 0]), Some([1019, 0, 0])]);
    assert_eq!((go("R", &field), go("Backspace", &field)), (None, None));
    assert_eq!(go("Alpha4,shift", &field), Some([1068, 3, 3])); // ExtremePower4
    // the rest of the ported keys, by name; the build hotkey C picks the wall tool
    assert_eq!([m.action("F1,shift"), m.action("F2,shift"), m.action("Space"), m.action("Z,alt"), m.action("F"), m.action("M,ctrl"), m.action("Tab")],
        [Some("Save"), Some("Load"), Some("FlattenLandscape"), Some("ToggleUI"), Some("ToggleFrameRate"), Some("ToggleMuteSounds"), Some("RotateBuilding")]);
    assert_eq!(m.action("C").and_then(|f| PLACE.iter().find(|p| p.0 == f)).map(|p| p.1), Some(25));
    assert!(runs("Save") && runs("SelectClan4") && runs("PlaceHouse") && runs("StanceStand") && !runs("FreeBuildEvents") && !runs("ToggleGoods"));
    // rebinding moves the function: SelectClan1 on Shift+1 takes it from ExtremePower1, plain 1 is free afterwards
    let mut k = m.clone();
    k.set("SelectClan1", 0, "Alpha1,shift");
    assert_eq!((k.action("Alpha1,shift"), k.action("Alpha1"), k.get("ExtremePower1")), (Some("SelectClan1"), None, ["", ""]));
    assert_eq!(k.action("Alpha1,shift").and_then(|f| game_action(f, &field)), Some([0x1e, -1, 0]));
}
