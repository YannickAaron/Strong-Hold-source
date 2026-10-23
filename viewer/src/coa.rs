//! The coat of arms (Avatars.cs, HUD_CoatOfArms): the player's shield made of an ordinary (background pattern, two
//! colours) and a charge (emblem, two colours), saved in settings.cfg as the original saves it (AvatarOrdinary,
//! AvatarOrdinaryColour1/2, AvatarCharge, AvatarChargeColour1/2) and sent as AvatarDesign.ToString's 13 digits.
//! The layers are the game's own 260×260 PNGs in Assets/GUI/Avatars, read from the user's copy at run time; the
//! composed shields go into HUD texture `TEX` (main.rs uploads `take_uploads`). Options > Coat of Arms is drawn
//! here with menu.rs's helpers (a child module of it).
use super::{quad, Act, Menu, FACE_BACKGROUNDS};
use crate::hud::{HudTri, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// HUD texture index of the coat-of-arms sheet (main.rs creates it W×H, Rgba8UnormSrgb).
pub const TEX: u32 = 7;
pub const W: u32 = 2048;
pub const H: u32 = 1024;
const N: usize = 260;
const FACES: usize = 16; // composed faces kept (lobby, trail, partners), 130² each right of the preview
const ICON: usize = 65; // the editor's grid icons: 260 / 4

/// Avatars.AvatarDesign: AvatarItems values (ordinary 1001..1091, charge 2001..2089, colours 1..25, the charge's
/// second colour 101..125 = the darker shade).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Design { pub ordinary: i32, pub ord: [i32; 2], pub charge: i32, pub chg: [i32; 2] }

/// settings.cfg keys, and ConfigSettings.loadSettingsFromString's valid range and default for each.
const KEYS: [(&str, i32, i32, i32); 6] = [("AvatarOrdinary", 1000, 1091, 1002), ("AvatarOrdinaryColour1", 1, 25, 22), ("AvatarOrdinaryColour2", 1, 25, 11),
    ("AvatarCharge", 2000, 2089, 2019), ("AvatarChargeColour1", 1, 25, 10), ("AvatarChargeColour2", 101, 125, 110)];

impl Default for Design { fn default() -> Self { Self::from_vals(KEYS.map(|k| k.3)) } }

impl Design {
    fn vals(&self) -> [i32; 6] { [self.ordinary, self.ord[0], self.ord[1], self.charge, self.chg[0], self.chg[1]] }
    fn from_vals(v: [i32; 6]) -> Self {
        let v: Vec<i32> = v.iter().zip(KEYS).map(|(&v, (_, lo, hi, d))| if (lo..=hi).contains(&v) { v } else { d }).collect();
        Design { ordinary: v[0], ord: [v[1], v[2]], charge: v[3], chg: [v[4], v[5]] }
    }
    /// From settings (`get` = a settings.cfg value); out of range = that field's default, as the game loads it.
    pub fn load(get: impl Fn(&str) -> Option<String>) -> Self {
        Self::from_vals(KEYS.map(|(k, _, _, d)| get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(d)))
    }
    pub fn store(&self, cfg: &mut crate::skirmish::Settings) {
        for ((k, ..), v) in KEYS.iter().zip(self.vals()) { cfg.set(k, v); }
    }
    /// AvatarDesign.ToString: "OOccCCIIIiiJJ" (ordinary − 1000, colours, charge − 2000, colour, colour − 100).
    pub fn code(&self) -> String {
        format!("{:02}{:02}{:02}{:03}{:02}{:02}", self.ordinary - 1000, self.ord[0], self.ord[1], self.charge - 2000, self.chg[0], self.chg[1] - 100)
    }
    /// AvatarDesign.FromString (13 digits); None = not a coat of arms.
    pub fn parse(s: &str) -> Option<Self> {
        if s.len() != 13 || !s.bytes().all(|b| b.is_ascii_digit()) { return None; }
        let n = |a: usize, b: usize| s[a..b].parse::<i32>().unwrap();
        Some(Self::from_vals([n(0, 2) + 1000, n(2, 4), n(4, 6), n(6, 9) + 2000, n(9, 11), n(11, 13) + 100]))
    }
}

/// The player's coat of arms as saved (Options).
pub fn mine() -> Design { Design::load(super::opt_text) }

/// Settings_UserName (Options > Player Settings, "Name" in settings.cfg); empty = the Mac login, as before.
pub fn player_name() -> String {
    super::opt_text("Name").map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
        .or_else(|| std::env::var("USER").ok()).unwrap_or_else(|| "Player".into())
}

/// Colours 1..25 (Avatars.colourLoader): "<NAME>_Base" / the darker "<NAME>_Base_Darker" for 101..125.
const COLOURS: [&str; 25] = ["BLACK", "BLUE", "BROWN", "DARKBLUE", "DARKBROWN", "DARKGREEN", "DARKPURPLE", "DARKRED", "FLESH", "GOLD",
    "GREEN", "GREY", "LIGHTBLUE", "LIGHTGREEN", "LIGHTORANGE", "LIGHTPINK", "LIGHTPURPLE", "LIGHTYELLOW", "ORANGE", "PINK", "PURPLE", "RED",
    "TEAL", "TURQUOISE", "WHITE"];
/// HUD_CoatOfArms.ColourSwabs (by colour 1..25): the Main / Alt swatches.
const SWABS: [[u8; 3]; 25] = [[19, 19, 19], [31, 62, 171], [109, 54, 5], [22, 22, 91], [51, 28, 11], [0, 100, 0], [43, 20, 60], [75, 9, 9],
    [192, 97, 97], [231, 180, 72], [100, 141, 13], [93, 93, 93], [72, 150, 215], [143, 195, 72], [255, 141, 0], [212, 155, 209], [114, 89, 164],
    [239, 189, 73], [178, 87, 24], [229, 74, 160], [130, 22, 130], [195, 43, 43], [0, 96, 96], [37, 177, 140], [181, 181, 181]];
/// ColourDisplayOrder (the 5×5 seal grid) and its Seal_<name> sprites.
const COLOUR_ORDER: [(i32, &str); 25] = [(16, "LightPink"), (18, "LightYellow"), (10, "Gold"), (14, "LightGreen"), (25, "White"), (17, "LightPurple"),
    (9, "Flesh"), (15, "LightOrange"), (11, "Green"), (12, "Grey"), (21, "Purple"), (20, "Pink"), (19, "Orange"), (6, "DarkGreen"), (13, "LightBlue"),
    (7, "DarkPurple"), (22, "Red"), (3, "Brown"), (24, "Turquoise"), (2, "Blue"), (1, "Black"), (8, "DarkRed"), (5, "DarkBrown"), (23, "Teal"), (4, "DarkBlue")];
/// OrdinariesDisplayOrder (BackMask_0NN − 1000); 91 = none (the charge alone).
const ORDINARY_ORDER: [i32; 91] = [1, 11, 7, 9, 8, 10, 14, 2, 3, 4, 12, 13, 15, 53, 54, 29, 23, 24, 30, 31, 18, 34, 21, 74, 76, 75, 86, 84, 66, 82, 55,
    56, 25, 45, 46, 32, 37, 38, 48, 49, 50, 47, 51, 39, 40, 81, 78, 52, 16, 17, 5, 6, 58, 89, 60, 61, 62, 63, 59, 77, 83, 88, 26, 90, 67, 72, 57, 65,
    64, 85, 70, 69, 68, 87, 36, 35, 22, 80, 27, 28, 79, 73, 33, 71, 44, 43, 42, 41, 20, 19, 91];
/// Charges (Enums.AvatarItems Item_*): id, file name part, has a second-colour mask (Avatars.itemLoader).
const ITEMS: [(i32, &str, bool); 86] = [(2001, "AF", false), (2002, "AM", false), (2004, "BESSY", false), (2005, "BF", false), (2006, "BM", false),
    (2007, "COTL", true), (2008, "EF", false), (2009, "EM", true), (2010, "GOING", true), (2011, "GUNGEON", true), (2012, "INK", true),
    (2013, "INSCRYPTION", true), (2014, "KZERO", true), (2015, "NEVA", true), (2016, "NORTH", true), (2018, "REIGNS", true), (2019, "SCRIBE", false),
    (2020, "STYX", true), (2021, "TALOS", true), (2022, "THRONE", true), (2023, "VOLVY", true), (2024, "KTC", true), (2025, "BLANK", false),
    (2026, "SABERS", false), (2027, "SARROW", true), (2028, "SCHECKER", true), (2029, "SCROSS", true), (2030, "SDRAGON", true), (2031, "SFDL", true),
    (2032, "SKULL", false), (2033, "SKULL2", false), (2034, "SLION", true), (2035, "SMOON", true), (2036, "SPHX", true), (2037, "SSTRIPE", true),
    (2038, "SSUN", true), (2039, "SUNICORN", true), (2040, "SWORDS", false), (2042, "ABBOT", true), (2043, "CALIPH", true), (2044, "CANARY", true),
    (2045, "CROCODILE", true), (2046, "EMIR", true), (2047, "FREDERICK", true), (2048, "JEWEL", true), (2049, "KAHINAH", true), (2050, "MARSHAL", true),
    (2051, "NIZAR", true), (2052, "NOMAD", true), (2053, "PHILIP", true), (2054, "PIG", true), (2055, "RAT", true), (2056, "RICHARD", true),
    (2057, "SALADIN", true), (2058, "SHERIFF", true), (2059, "SNAKE", true), (2060, "SULTAN", true), (2061, "TRADER", true), (2062, "WAZIR", true),
    (2063, "WOLF", true), (2064, "SENTINEL", true), (2065, "GOAT", true), (2066, "JACKAL", true), (2067, "LAMB", true), (2068, "LEOPARD", true),
    (2069, "FALCON", true), (2070, "LIONESS", true), (2071, "SERGEANT", true), (2072, "COBRA", true), (2073, "FISH", true), (2074, "CROWN", true),
    (2075, "ROSE", true), (2076, "FLOWERS", true), (2077, "CAT", true), (2078, "PAW", true), (2079, "SUN", true), (2080, "ANKH", true),
    (2081, "CROSSA", true), (2082, "CROSSB", true), (2083, "CROSSC", true), (2084, "CROSSD", true), (2085, "CIRCLE", true), (2086, "CIRCLEB", true),
    (2087, "CIRCLEC", true), (2088, "BALDWIN", true), (2089, "BULLSEYE", true)];
/// ChargesDisplayOrder, by name.
const CHARGE_ORDER: [&str; 86] = ["BLANK", "RAT", "SNAKE", "PIG", "WOLF", "SALADIN", "CALIPH", "SULTAN", "RICHARD", "FREDERICK", "PHILIP", "WAZIR",
    "EMIR", "NIZAR", "SHERIFF", "MARSHAL", "ABBOT", "JEWEL", "SENTINEL", "NOMAD", "KAHINAH", "CROCODILE", "CANARY", "TRADER", "LIONESS", "SERGEANT",
    "BALDWIN", "BULLSEYE", "LAMB", "GOAT", "FALCON", "LEOPARD", "JACKAL", "COBRA", "FISH", "CROWN", "ROSE", "FLOWERS", "CAT", "PAW", "SUN", "ANKH",
    "CROSSA", "CROSSB", "CROSSC", "CROSSD", "CIRCLE", "CIRCLEB", "CIRCLEC", "COTL", "GUNGEON", "INSCRYPTION", "KZERO", "REIGNS", "TALOS", "VOLVY",
    "NEVA", "KTC", "NORTH", "INK", "STYX", "GOING", "THRONE", "EM", "EF", "AF", "AM", "BM", "BF", "SCRIBE", "BESSY", "SSTRIPE", "SARROW", "SCHECKER",
    "SCROSS", "SDRAGON", "SFDL", "SLION", "SMOON", "SPHX", "SSUN", "SUNICORN", "SWORDS", "SKULL", "SKULL2", "SABERS"];

fn charge_id(name: &str) -> i32 { ITEMS.iter().find(|i| i.1 == name).map_or(2025, |i| i.0) }
/// chargeRolloverIDs: the collaboration charges name their game.
fn rollover(charge: i32) -> &'static str {
    match charge {
        2007 => "Cult of the Lamb", 2010 => "Going Medieval", 2011 => "Enter the Gungeon", 2012 => "Inkulinati", 2013 => "Inscryption",
        2014 => "Katana ZERO", 2015 => "Neva", 2016 => "Northgard", 2018 => "Reigns", 2020 => "Songs of Syx", 2021 => "The Talos Principle 2",
        2022 => "Thronefall", 2023 => "Volvy - Devolver Digital", 2024 => "Kingdom Two Crowns", _ => "",
    }
}

// ---- the layers: Assets/GUI/Avatars/<name>.png in the user's game (the names' case varies: looked up case-blind)

type Layer = Option<Arc<Vec<u8>>>;
static CACHE: Mutex<Option<HashMap<String, Layer>>> = Mutex::new(None);

fn files() -> &'static HashMap<String, PathBuf> {
    static F: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    F.get_or_init(|| {
        let dir = crate::paths::game().join("Assets/GUI/Avatars");
        std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| (e.file_name().to_string_lossy().to_lowercase(), e.path())).collect()
    })
}
/// `name`.png as 260×260 RGBA, uncached.
fn load(name: &str) -> Layer {
    let p = files().get(&format!("{}.png", name.to_lowercase()))?;
    let img = image::load_from_memory_with_format(&std::fs::read(p).ok()?, image::ImageFormat::Png).ok()?.to_rgba8();
    (img.dimensions() == (N as u32, N as u32)).then(|| Arc::new(img.into_raw()))
}
/// `load`, kept.
fn layer(name: &str) -> Layer {
    if let Some(l) = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(name) { return l.clone(); }
    let l = load(name);
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(name.to_string(), l.clone());
    l
}
fn colour_name(id: i32) -> Option<String> {
    match id { 1..=25 => Some(format!("{}_Base", COLOURS[id as usize - 1])), 101..=125 => Some(format!("{}_Base_Darker", COLOURS[id as usize - 101])), _ => None }
}

/// Avatars.GetAvatarTexture: the 260×260 shield, straight-alpha RGBA (the original premultiplies for Noesis; our HUD
/// blends straight). `feather`: the 15 px faded edge the faces get.
pub fn compose(d: &Design, feather: bool) -> Option<Vec<u8>> { compose_with(d, feather, &layer) }

fn compose_with(d: &Design, feather: bool, get: &dyn Fn(&str) -> Layer) -> Option<Vec<u8>> {
    let solid = |id: i32| colour_name(id).and_then(|n| get(&n));
    let none = d.ordinary == 1091; // BackMask_091: no ordinary, the charge on transparency
    let mut px = vec![[0f32; 4]; N * N];
    let at = |l: &[u8], k: usize, c: usize| l[k * 4 + c] as f32 / 255.0;
    // over: colour `l` at k with coverage a (a == 1 replaces, alpha 1)
    let over = |p: &mut [f32; 4], l: &[u8], k: usize, a: f32| {
        if a == 1.0 { *p = [at(l, k, 0), at(l, k, 1), at(l, k, 2), 1.0]; return; }
        for c in 0..3 { p[c] = p[c] * (1.0 - a) + at(l, k, c) * a; }
    };
    match solid(d.ord[0]) {
        Some(bg) => {
            for (k, p) in px.iter_mut().enumerate() { *p = if none { [1.0, 1.0, 1.0, 0.0] } else { [at(&bg, k, 0), at(&bg, k, 1), at(&bg, k, 2), 1.0] }; }
            if d.charge == 0 && none { px.fill([0.0, 0.0, 0.0, 1.0]); }
            if let (false, Some(m), Some(bg2)) = (none, get(&format!("Base_Mask_{}", d.ordinary - 1000)), solid(d.ord[1])) {
                for (k, p) in px.iter_mut().enumerate() {
                    let a = m[k * 4] as f32 / 255.0;
                    if a != 0.0 { over(p, &bg2, k, a); }
                }
            }
        }
        None => px.fill([0.0, 0.0, 0.0, 0.5]),
    }
    if let Some(&(_, name, two)) = ITEMS.iter().find(|i| i.0 == d.charge && i.1 != "BLANK") {
        if let Some(ov) = get(&format!("Emblem_Outline_{name}")) {
            if let (Some(m), Some(c1)) = (get(&format!("Emblem_Mask_{name}")), solid(d.chg[0])) {
                for (k, p) in px.iter_mut().enumerate() {
                    let a = m[k * 4] as f32 / 255.0;
                    if a == 0.0 { continue; }
                    if none && a != 1.0 { *p = [at(&c1, k, 0), at(&c1, k, 1), at(&c1, k, 2), a]; } else { over(p, &c1, k, a); }
                }
            }
            if let (true, Some(m), Some(c2)) = (two, get(&format!("Emblem_Colour2_Mask_{name}")), solid(d.chg[1])) {
                for (k, p) in px.iter_mut().enumerate() {
                    let a = m[k * 4] as f32 / 255.0;
                    if a == 0.0 { continue; }
                    over(p, &c2, k, a);
                    p[3] = p[3].max(a);
                }
            }
            for (k, p) in px.iter_mut().enumerate() {
                let a = ov[k * 4 + 3] as f32 / 255.0;
                if a == 0.0 { continue; }
                over(p, &ov, k, a);
                p[3] = p[3].max(a);
            }
        }
    }
    if feather && !none {
        for y in 0..N {
            for x in 0..N {
                let f = |v: usize| if v > 130 { N - v - 1 } else { v };
                let (fx, fy) = (f(x), f(y));
                if fx < 30 || fy < 30 { px[y * N + x][3] = fx.saturating_sub(15).min(fy.saturating_sub(15)) as f32 / 15.0; }
            }
        }
    }
    Some(px.iter().flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect())
}

/// `px` (n×n RGBA) shrunk by `k` (alpha-weighted box filter: no light fringes from transparent pixels).
fn shrink(px: &[u8], n: usize, k: usize) -> Vec<u8> {
    let m = n / k;
    let mut out = vec![0u8; m * m * 4];
    for y in 0..m {
        for x in 0..m {
            let mut s = [0f32; 4];
            for dy in 0..k {
                for dx in 0..k {
                    let o = ((y * k + dy) * n + x * k + dx) * 4;
                    let a = px[o + 3] as f32;
                    for c in 0..3 { s[c] += px[o + c] as f32 * a; }
                    s[3] += a;
                }
            }
            let o = (y * m + x) * 4;
            for c in 0..3 { out[o + c] = if s[3] > 0.0 { (s[c] / s[3]).round() as u8 } else { 0 }; }
            out[o + 3] = (s[3] / (k * k) as f32).round() as u8;
        }
    }
    out
}

// ---- the sheet (HUD texture TEX): the editor's preview 260² at 0,0, FACES faces 130² right of it, then the icons

#[derive(Default)]
struct Sheet { preview: Option<String>, faces: Vec<(String, bool)>, next: usize, icons: Option<bool>, up: Vec<([u32; 4], Vec<u8>)> }
static SHEET: Mutex<Option<Sheet>> = Mutex::new(None);

fn sheet<R>(f: impl FnOnce(&mut Sheet) -> R) -> R { f(SHEET.lock().unwrap().get_or_insert_with(Sheet::default)) }
fn uv(x: usize, y: usize, s: usize) -> [f32; 4] { [x as f32 / W as f32, y as f32 / H as f32, (x + s) as f32 / W as f32, (y + s) as f32 / H as f32] }

/// The sheet's new parts for main.rs: (x, y, w, h) and RGBA rows.
pub fn take_uploads() -> Vec<([u32; 4], Vec<u8>)> { sheet(|s| std::mem::take(&mut s.up)) }

/// The editor's preview (no feathered edge) of `d`; None = the game's Avatars layers aren't there.
fn preview(d: &Design) -> Option<[f32; 4]> {
    let code = d.code();
    if sheet(|s| s.preview.as_ref() != Some(&code)) {
        let px = compose(d, false)?;
        sheet(|s| { s.up.push(([0, 0, N as u32, N as u32], px)); s.preview = Some(code); });
    }
    Some(uv(0, 0, N))
}

/// Face slot of coat of arms `code` (feathered, 130²), composed the first time it's asked for.
fn face(code: &str) -> Option<[f32; 4]> {
    face_with(code, || Design::parse(code).and_then(|d| compose(&d, true)).map(|px| shrink(&px, N, 2)))
}
/// Face slot of `code`, its 130² RGBA from `make` the first time (a coat of arms, or a custom lord's avatar.png).
fn face_with(code: &str, make: impl FnOnce() -> Option<Vec<u8>>) -> Option<[f32; 4]> {
    let slot = |i: usize| (N + 130 * (i % 8), 130 * (i / 8));
    if let Some(i) = sheet(|s| s.faces.iter().position(|f| f.0 == code)) {
        let (x, y) = slot(i);
        return sheet(|s| s.faces[i].1).then(|| uv(x, y, 130));
    }
    let px = make();
    sheet(|s| {
        let i = if s.faces.len() < FACES { s.faces.push((String::new(), false)); s.faces.len() - 1 } else { s.next = (s.next + 1) % FACES; s.next };
        s.faces[i] = (code.to_string(), px.is_some());
        let (x, y) = slot(i);
        px.map(|px| { s.up.push(([x as u32, y as u32, 130, 130], px)); uv(x, y, 130) })
    })
}

/// Grid icon i: 0..91 the ordinaries (white on black, ORDINARY_ORDER), 91.. the charges' UI_MASK pictures. All built
/// the first time (≈180 small PNGs).
fn icon(i: usize) -> Option<[f32; 4]> {
    let at = |i: usize| (ICON * (i % 31), 2 * 130 + ICON * (i / 31));
    if sheet(|s| s.icons.is_none()) {
        let t0 = std::time::Instant::now();
        let masks = |n: &str| if n.starts_with("Base_Mask") { load(n) } else { layer(n) }; // the 91 masks aren't kept
        let mut up = vec![];
        for (k, &o) in ORDINARY_ORDER.iter().enumerate() {
            let d = Design { ordinary: 1000 + o, ord: [25, 1], charge: 0, chg: [1, 101] };
            if let Some(px) = compose_with(&d, false, &masks) { let (x, y) = at(k); up.push(([x as u32, y as u32, ICON as u32, ICON as u32], shrink(&px, N, 4))); }
        }
        for (k, name) in CHARGE_ORDER.iter().enumerate() {
            if let Some(px) = load(&format!("UI_MASK_{name}")) { let (x, y) = at(91 + k); up.push(([x as u32, y as u32, ICON as u32, ICON as u32], shrink(&px, N, 4))); }
        }
        eprintln!("coat of arms: {} icons in {:.2}s", up.len(), t0.elapsed().as_secs_f32());
        sheet(|s| { s.icons = Some(!up.is_empty()); s.up.extend(up); });
    }
    let (x, y) = at(i);
    sheet(|s| s.icons == Some(true)).then(|| uv(x, y, ICON))
}

/// Draws coat of arms `code` (AvatarDesign string) as a face into `p`; false = none (no game layers / not a
/// code): the caller draws its stand-in.
pub fn draw_face(out: &mut Vec<HudTri>, code: &str, p: [f32; 4], color: [f32; 4]) -> bool {
    face(code).map(|uv| quad(out, TEX, p, uv, color)).is_some()
}
/// A picture face (`px` = 130² RGBA, e.g. a custom lord's avatar.png) under the sheet name `code`; false = no picture.
pub fn draw_picture(out: &mut Vec<HudTri>, code: &str, px: impl FnOnce() -> Option<Vec<u8>>, p: [f32; 4], color: [f32; 4]) -> bool {
    face_with(code, px).map(|uv| quad(out, TEX, p, uv, color)).is_some()
}
/// The player's own face (`mine`).
pub fn draw_mine(out: &mut Vec<HudTri>, p: [f32; 4], color: [f32; 4]) -> bool { draw_face(out, &mine().code(), p, color) }

/// `uv` drawn into `p`, cut to rows `clip` (screen px): the scrolled grid.
fn quad_clip(out: &mut Vec<HudTri>, mut p: [f32; 4], mut uv: [f32; 4], clip: [f32; 2]) {
    let (h, vh) = (p[3] - p[1], uv[3] - uv[1]);
    if p[1] < clip[0] { uv[1] += (clip[0] - p[1]) / h * vh; p[1] = clip[0]; }
    if p[3] > clip[1] { uv[3] -= (p[3] - clip[1]) / h * vh; p[3] = clip[1]; }
    if p[3] > p[1] { quad(out, TEX, p, uv, [1.0; 4]); }
}

/// Exported "Coat Of Arms.png" (HUD_CoatOfArms Export): into our data folder, which then opens.
fn export(d: &Design) -> Result<(), String> {
    let px = compose(d, false).ok_or("the game's coat of arms pictures are missing")?;
    let dir = crate::skirmish::profile_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    image::save_buffer(dir.join("Coat Of Arms.png"), &px, N as u32, N as u32, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    let _ = std::process::Command::new("open").arg(&dir).spawn();
    Ok(())
}

// ---- Options > Coat of Arms (HUD_CoatOfArms.xaml, 397×510 in the options page at 350, 70)

/// The editor's state (HUD_CoatOfArms statics): Charges tab (else Ordinaries), Alt colour picked (else Main), the
/// charge's colours locked together.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Ed { pub charges: bool, pub alt: bool, pub locked: bool }
impl Ed {
    /// HUD_CoatOfArms.Init: Ordinaries, Main; locked when the charge's two colours are the same.
    pub fn open() -> Self { let d = mine(); Ed { charges: false, alt: false, locked: d.chg[0] == d.chg[1] - 100 } }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum A { Tab(bool), Main(bool), Lock, Colour(u8), Pick(u8), Export }

/// The grid's rows (6 icons of 56 px) scrolled by `scroll` px: the content's height.
pub fn grid_h(ed: &Ed) -> f32 { (if ed.charges { 86.0f32 } else { 91.0 } / 6.0).ceil() * 56.0 }

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
/// sRGB byte -> the HUD's linear vertex colour.
fn lin(c: u8) -> f32 { let c = c as f32 / 255.0; if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } }
fn rgb(c: [u8; 3]) -> [f32; 4] { [lin(c[0]), lin(c[1]), lin(c[2]), 1.0] }

impl Menu {
    /// A click on the editor (HUD_CoatOfArms.ButtonClicked): every change is saved at once.
    pub(super) fn coa_click(&mut self, ed: &mut Ed, scroll: &mut f32, a: A) {
        let mut d = Design::load(|k| self.cfg.get(k).map(str::to_string));
        match a {
            A::Tab(charges) => { (ed.charges, *scroll) = (charges, 0.0); return; }
            A::Main(alt) => { ed.alt = alt; return; }
            A::Lock => { ed.locked = !ed.locked; return; }
            A::Export => { if let Err(e) = export(&d) { self.error = Some(format!("Coat Of Arms.png: {e}")); } return; }
            A::Pick(i) if ed.charges => d.charge = charge_id(CHARGE_ORDER[i as usize]),
            A::Pick(i) => d.ordinary = 1000 + ORDINARY_ORDER[i as usize],
            A::Colour(k) => {
                let c = COLOUR_ORDER[k as usize].0;
                match (ed.charges, ed.alt) {
                    (false, false) => d.ord[0] = c,
                    (false, true) => d.ord[1] = c,
                    (true, alt) => {
                        if !alt || ed.locked { d.chg[0] = c; }
                        if alt || ed.locked { d.chg[1] = c + 100; }
                    }
                }
            }
        }
        d.store(&mut self.cfg);
        self.save_cfg();
    }

    /// The page: title, preview over the player-colour face, the seal grid, Main / Alt (and the lock on Charges), the
    /// Ordinary / Charge tabs and their scrolled grid, the charge's game under the cursor, the note. ponytail: no
    /// "Display Steam Profile Picture" box (no Steam here).
    pub(super) fn coa_page(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, ed: Ed, scroll: f32, cursor: [f32; 2]) {
        let (ox, oy) = (350.0, 70.0);
        let r = |m: &Self, x: f32, y: f32, w: f32, h: f32| m.r(ox + x, oy + y, w, h);
        let d = Design::load(|k| self.cfg.get(k).map(str::to_string));
        self.frame(out, ox + 58.5, oy + 20.0, 280.0, 40.0, true);
        self.small_caps(ui, out, &ui.text_of("TEXT_NEW_CTEXT_373"), ox + 198.5, oy + 47.0, 22.0, BLACK);
        ui.img_fit(out, "Charge Icon", r(self, 6.0, 20.0, 40.0, 40.0), [1.0; 4]);
        self.img_button(ui, out, ["Charge_Export_Icon_Up", "Charge_Export_Icon_Over", "Charge_Export_Icon_Up"], ox + 351.0, oy + 20.0, 40.0, 40.0, Some(Act::Coa(A::Export)), cursor);
        // the shield: UI Player Border, the player colour's face background, the coat of arms
        ui.img(out, "UI Player Border", r(self, 37.0, 77.0, 138.0, 138.0), [1.0; 4]);
        let col = (self.cfg.num("PlayerColour", 0.0) as usize).min(7);
        ui.img(out, FACE_BACKGROUNDS[col], r(self, 40.0, 80.0, 132.0, 132.0), [1.0; 4]);
        match preview(&d) {
            Some(uv) => quad(out, TEX, r(self, 40.0, 80.0, 132.0, 132.0), uv, [1.0; 4]),
            None => { self.para(ui, out, "The game's coat of arms pictures (Assets/GUI/Avatars) are missing.", ox + 44.0, oy + 110.0, 124.0, 14.0, BLACK); }
        }
        // the seals: 37² every 35 px, 5 a row
        let (c1, c2) = if ed.charges { (d.chg[0], d.chg[1] - 100) } else { (d.ord[0], d.ord[1]) };
        let editing = if ed.alt { c2 } else { c1 };
        for (k, &(c, name)) in COLOUR_ORDER.iter().enumerate() {
            let (x, y) = (ox + 193.0 + 35.0 * (k % 5) as f32, oy + 72.0 + 35.0 * (k / 5) as f32);
            self.img_button(ui, out, [&format!("Seal_{name}_Up"), &format!("Seal_{name}_Over"), &format!("Seal_{name}_Up")], x, y, 37.0, 37.0, Some(Act::Coa(A::Colour(k as u8))), cursor);
            if c == editing { ui.img_fit(out, &format!("Seal_{name}_Selected"), self.r(x, y, 37.0, 37.0), [1.0; 4]); }
        }
        // Main / Alt swatches, the picked one in a #d9a405 frame; the lock (Charges)
        let gold = rgb([0xd9, 0xa4, 0x05]);
        let frame = |m: &Self, out: &mut Vec<HudTri>, x: f32, y: f32, w: f32, h: f32| {
            Ui::fill(out, r(m, x, y, w, h), BLACK);
            Ui::fill(out, r(m, x + 1.0, y + 1.0, w - 2.0, h - 2.0), gold);
        };
        if ed.alt { frame(self, out, 110.0, 217.0, 64.0, 34.0) } else { frame(self, out, 41.0, 218.0, 62.0, 32.0) }
        for (x, c, alt) in [(42.0, c1, false), (112.0, c2, true)] {
            let p = r(self, x, 219.0, 60.0, 30.0);
            Ui::fill(out, p, rgb(SWABS[(c - 1).clamp(0, 24) as usize]));
            self.hits.push((p, Act::Coa(A::Main(alt))));
        }
        if ed.charges {
            let s = if ed.locked { ["Locked", "Locked_Over", "Locked"] } else { ["UnLocked", "UnLocked_Over", "UnLocked"] };
            self.img_button(ui, out, s, ox + 166.0, oy + 215.0, 20.0, 20.0, Some(Act::Coa(A::Lock)), cursor);
        }
        // tabs
        for (x, charges, key) in [(30.0, false, "TEXT_NEW_CTEXT_376"), (200.0, true, "TEXT_NEW_CTEXT_377")] {
            let p = r(self, x, 255.0, 170.0, 30.0);
            Ui::fill(out, p, if ed.charges == charges { rgb([181, 46, 12]) } else { rgb([99, 18, 0]) });
            self.label(ui, out, &ui.text_of(key), ox + x + 85.0, oy + 270.0, 16.0, 0.5, super::CREAM);
            self.hits.push((p, Act::Coa(A::Tab(charges))));
        }
        Ui::fill(out, r(self, 30.0, 285.0, 340.0, 140.0), [0.0, 0.0, 0.0, 0x44 as f32 / 255.0]);
        let e = 2.0 * self.s;
        let b = r(self, 30.0, 255.0, 340.0, 170.0);
        for q in [[b[0], b[1], b[2], b[1] + e], [b[0], b[3] - e, b[2], b[3]], [b[0], b[1], b[0] + e, b[3]], [b[2] - e, b[1], b[2], b[3]]] { Ui::fill(out, q, BLACK); }
        // the grid: 50² buttons every 56 px, 6 a row, in the 336×135 viewer at 30.5, 285
        let view = r(self, 30.5, 285.0, 336.0, 135.0);
        let n = if ed.charges { 86 } else { 91 };
        let mut hover = None;
        for i in 0..n {
            let (x, y) = (ox + 30.5 + 3.0 + 56.0 * (i % 6) as f32, oy + 285.0 + 3.0 + 56.0 * (i / 6) as f32 - scroll);
            let p = self.r(x, y, 50.0, 50.0);
            if p[3] <= view[1] || p[1] >= view[3] { continue; }
            if let Some(uv) = icon(i + if ed.charges { 91 } else { 0 }) { quad_clip(out, p, uv, [view[1], view[3]]); }
            let hit = [p[0], p[1].max(view[1]), p[2], p[3].min(view[3])];
            if Self::inside(hit, cursor) { hover = Some(i); }
            self.hits.push((hit, Act::Coa(A::Pick(i as u8))));
        }
        // CoARollover (the collaboration charges' games), the note
        if let Some(i) = hover.filter(|_| ed.charges) { self.label(ui, out, rollover(charge_id(CHARGE_ORDER[i])), ox + 198.5, oy + 437.0, 16.0, 0.5, BLACK); }
        let note = ui.text_of("TEXT_NEW_CTEXT_375");
        let mut y = oy + 448.0;
        for line in wrap(ui, &note, 300.0 * self.s, 14.0) { self.label(ui, out, &line, ox + 198.5, y + 9.0, 14.0, 0.5, BLACK); y += 17.5; }
    }
}

/// `s` broken into lines at most `w` screen px wide (centred text).
fn wrap(ui: &mut Ui, s: &str, w: f32, size: f32) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in s.split_whitespace() {
        let cur = lines.last_mut().unwrap();
        let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if !cur.is_empty() && ui.text(&mut vec![], &next, 0.0, 0.0, size, 1.0, 0.0, 0.0, BLACK) > w { lines.push(word.to_string()); } else { *cur = next; }
    }
    lines
}

#[test]
fn design_round_trips() {
    let d = Design::default();
    assert_eq!(d.code(), "0222110191010"); // AvatarDesign.ToString of the game's defaults
    assert_eq!(Design::parse(&d.code()), Some(d));
    let d2 = Design { ordinary: 1091, ord: [25, 1], charge: 2089, chg: [3, 124] };
    assert_eq!(Design::parse(&d2.code()), Some(d2));
    assert_eq!(Design::parse("12"), None);
    assert_eq!(Design::load(|k| (k == "AvatarCharge").then(|| "9999".into())).charge, 2019); // out of range: default
    let mut cfg = crate::skirmish::Settings::default();
    d2.store(&mut cfg);
    assert_eq!(Design::load(|k| cfg.get(k).map(str::to_string)), d2);
}

/// With the game's pictures: the default shield is opaque in the middle, feathered at the edge; no ordinary leaves the
/// background transparent; every charge and ordinary has its layers.
#[test]
fn composes_from_the_game_layers() {
    let Some(px) = compose(&Design::default(), true) else { return eprintln!("no Assets/GUI/Avatars: skipped") };
    let a = |px: &[u8], x: usize, y: usize| px[(y * N + x) * 4 + 3];
    assert_eq!((a(&px, 130, 130), a(&px, 0, 0), a(&px, 20, 130)), (255, 0, 85));
    let bare = compose(&Design { ordinary: 1091, ..Design::default() }, false).unwrap();
    assert_eq!(a(&bare, 2, 2), 0);
    for &(_, name, two) in &ITEMS {
        if name == "BLANK" { continue; }
        for f in [format!("Emblem_Mask_{name}"), format!("Emblem_Outline_{name}"), format!("UI_MASK_{name}")] { assert!(load(&f).is_some(), "{f}"); }
        if two { assert!(load(&format!("Emblem_Colour2_Mask_{name}")).is_some(), "{name} colour 2"); }
    }
    for o in 1..=91 { assert!(load(&format!("Base_Mask_{o}")).is_some(), "mask {o}"); }
    for c in (1..=25).chain(101..=125) { assert!(layer(&colour_name(c).unwrap()).is_some(), "colour {c}"); }
}
