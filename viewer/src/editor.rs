//! The map editor's front end, a child module of menu.rs (it draws with that file's helpers): FRONT_EditorSetup
//! (new map size and type, or a map to load) and, over the running editor, HUD_Main's map-editor panel (MEMenuGrid:
//! the six tool menus, brush size, delete brush) with a name box, Save Map and Main Menu of ours. The sim side is
//! shc_sim::editor (DLL_PreInitMap_Editor, the editor load / map save) and mapaction's inGame 0 path; main.rs runs the
//! editor as a live game (live.rs `Cmd::Editor` / `Cmd::SaveMap`).
#![cfg_attr(not(feature = "live"), allow(dead_code))] // without the sim only the setup screen builds
use super::*;

/// FRONT_EditorSetup: the map size (160..800) and type picked (C# mode 1 invasion, 3 free build, 4 multiplayer);
/// `load` = the map list is open instead.
#[derive(Clone, Copy, PartialEq)]
pub struct T { size: i32, mode: u8, load: bool }
impl T {
    /// The map list is open (Escape goes back to the setup).
    pub fn loading(&self) -> bool { self.load }
}

/// Editor setup clicks (menu.rs `Act::Ed`).
#[derive(Clone, Copy, PartialEq)]
pub enum A { Open, Size(i32), Mode(u8), Start, Load }

/// What main.rs starts (`Go::Editor`): EditorDirector.createNewMap(size, mode) or loadMapIntoEditor(file).
#[derive(Clone, Debug, PartialEq)]
pub enum Start { New { size: i32, mode: u8 }, Load(PathBuf) }

impl Start {
    /// DLL_PreInitMap_Editor's (mapType, multiplayerMap) for a new map: createNewMap passes GameModes INVASION (3) for
    /// mode 1, else BUILD (0); mode 4 is the multiplayer map.
    pub fn map_type(mode: u8) -> (i32, bool) { (if mode == 1 { 3 } else { 0 }, mode == 4) }

    /// The name a new map is saved under first.
    pub fn name(&self) -> String {
        match self {
            Start::New { .. } => "New Map".into(),
            Start::Load(p) => p.file_stem().map_or("New Map".into(), |s| s.to_string_lossy().into_owned()),
        }
    }
}

/// Where the editor saves maps: the player's Maps folder, which the Sandbox lists (skirmish::solo_maps).
pub fn maps_dir() -> PathBuf { skirmish::profile_dir().join("Maps") }

/// Maps the editor can load: own maps first, then the game's.
fn loadable() -> Vec<PathBuf> {
    let mut v = skirmish::list(&maps_dir(), "map");
    v.extend(skirmish::list(&skirmish::maps_dir(), "map"));
    v
}

const SIZES: [(i32, &str); 8] = [(160, "TEXT_NEW_CTEXT_275"), (200, "TEXT_NEW_CTEXT_276"), (300, "TEXT_NEW_CTEXT_277"), (400, "TEXT_NEW_CTEXT_278"),
    (500, "TEXT_NEW_CTEXT_279"), (600, "TEXT_NEW_CTEXT_280"), (700, "TEXT_NEW_CTEXT_281"), (800, "TEXT_NEW_CTEXT_282")];
/// Map types: mode, label, help (FRONT_EditorSetup.MouseEnterTypeHandler: TEXT_NEW_TEXT 109, TEXT_NEWMAP_TYPES_HELP 3 / 7).
const MODES: [(u8, &str, &str); 3] = [(4, "TEXT_NEW_CTEXT_270", "TEXT_NEW_TEXT_110"), (1, "TEXT_MAINOPTIONS_031", "TEXT_NEWMAP_TYPES_HELP_004"),
    (3, "TEXT_BUBBLE_HELP_TEXT_265", "TEXT_NEWMAP_TYPES_HELP_008")];

impl Menu {
    /// FRONT_EditorSetup.Open: 160, Multiplayer.
    fn ed_open(&mut self) { *self = Menu { screen: Screen::Editor(T { size: 160, mode: 4, load: false }), ..Self::main() }; }

    /// Clicks of the editor screens; None = not ours.
    pub(super) fn ed_click_first(&mut self, a: Act) -> Option<Option<Go>> {
        if let Act::Ed(A::Open) = a {
            self.ed_open();
            return Some(None);
        }
        let Screen::Editor(t) = &mut self.screen else { return None };
        match a {
            Act::Ed(A::Size(s)) => t.size = s,
            Act::Ed(A::Mode(m)) => t.mode = m,
            Act::Ed(A::Start) => return Some(Some(Go::Editor(Start::New { size: t.size, mode: t.mode }))),
            Act::Ed(A::Load) => {
                t.load = true;
                let files = loadable();
                (self.files, self.scroll, self.error) = (files, 0, None);
            }
            Act::Pick(i) if t.load => return Some(self.files.get(i).cloned().map(|f| Go::Editor(Start::Load(f)))),
            Act::Back if t.load => t.load = false,
            _ => return None,
        }
        Some(None)
    }

    /// Escape on the editor screens: the map list back to the setup, the setup to the main menu.
    pub(super) fn ed_back(&mut self) -> bool {
        match &mut self.screen {
            Screen::Editor(t) if t.load => { t.load = false; true }
            _ => false,
        }
    }

    /// FRONT_EditorSetup (1366×768 grid ×1.4 in the 1920 space): title, "Select New Map Size" (8 sizes, 2 columns)
    /// with Load Map File below, "Select Game Type" with its help and Start; Back. Or the map list (Load).
    pub(super) fn ed_draw(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, t: T, cursor: [f32; 2]) {
        let title = ui.text_of("TEXT_MAPEDIT_001");
        self.heading(ui, out, 602.5, &title);
        let back = ui.text_of("TEXT_MAPEDIT_008");
        self.button(ui, out, &back, 60.0, 980.0, 200.0, 56.0, Act::Back, cursor);
        if t.load {
            self.panel(ui, out, 460.0, 200.0, 1000.0, 740.0);
            let head = ui.text_of("TEXT_SCENARIO_046");
            self.text(ui, out, &head, 960.0, 255.0, 34.0, 0.5, GOLD);
            if self.files.is_empty() { self.text(ui, out, "No maps found", 960.0, 400.0, 28.0, 0.5, DIM); }
            self.list(ui, out, 520.0, 280.0, 880.0, cursor);
            return;
        }
        let black = [0.0, 0.0, 0.0, 1.0];
        // left: map size
        self.frame(out, 240.0, 250.0, 700.0, 520.0, true);
        let h = ui.text_of("TEXT_MAPEDIT_021");
        self.label(ui, out, &h, 590.0, 290.0, 30.0, 0.5, black);
        for (k, (size, key)) in SIZES.into_iter().enumerate() {
            let (x, y) = (290.0 + (k / 4) as f32 * 320.0, 350.0 + (k % 4) as f32 * 60.0);
            let label = ui.text_of(key);
            self.glow(ui, out, &label, x, y, 300.0, Some(if t.size == size { "UI-Buttons S100" } else { "UI-Buttons S043" }), Some(Act::Ed(A::Size(size))), cursor);
        }
        let load = ui.text_of("TEXT_SCENARIO_046");
        self.glow(ui, out, &load, 440.0, 680.0, 300.0, None, Some(Act::Ed(A::Load)), cursor);
        // right: game type, its help, Start
        self.frame(out, 980.0, 250.0, 700.0, 520.0, true);
        let h = ui.text_of("TEXT_MAPEDIT_020");
        self.label(ui, out, &h, 1330.0, 290.0, 30.0, 0.5, black);
        let mut help = None;
        for (k, (mode, key, hk)) in MODES.into_iter().enumerate() {
            let y = 350.0 + k as f32 * 60.0;
            let label = ui.text_of(key);
            if Self::inside(self.r(1155.0, y - 1.5, 350.0, 31.0), cursor) || (help.is_none() && t.mode == mode) { help = Some(hk); }
            self.glow(ui, out, &label, 1155.0, y, 350.0, Some(if t.mode == mode { "UI-Buttons S100" } else { "UI-Buttons S043" }), Some(Act::Ed(A::Mode(mode))), cursor);
        }
        if let Some(hk) = help {
            let s = ui.text_of(hk);
            self.para(ui, out, &s, 1040.0, 560.0, 580.0, 20.0, black);
        }
        let start = ui.text_of("TEXT_MAPEDIT_002");
        self.button(ui, out, &start, 1380.0, 690.0, 240.0, 56.0, Act::Ed(A::Start), cursor);
    }
}

// ---- the editor HUD (HUD_Main in map-editor mode) ----

/// One MEMenuGrid tool button: menu (1 terrain .. 6 rocks), eMappers id, margin x / y and size in the 382×118 build
/// menu grid, the sprite (rollover = next number, checked = the one after), its help text (HUD_Main.xaml).
struct Tool { menu: u8, mapper: i32, x: f32, y: f32, w: f32, h: f32, sprite: &'static str, help: &'static str }
const fn tool(menu: u8, mapper: i32, x: f32, y: f32, w: f32, h: f32, sprite: &'static str, help: &'static str) -> Tool { Tool { menu, mapper, x, y, w, h, sprite, help } }
const TOOLS: &[Tool] = &[
    tool(1, 0x25, 13.0, 0.0, 70.0, 50.0, "G109", "TEXT_BUBBLE_HELP_TEXT_159"), tool(1, 0x24, 95.0, 0.0, 70.0, 54.0, "G112", "TEXT_BUBBLE_HELP_TEXT_158"),
    tool(1, 0x15, 170.0, -2.0, 63.0, 59.0, "G115", "TEXT_BUBBLE_HELP_TEXT_155"), tool(1, 0x16, 228.0, 0.0, 76.0, 60.0, "G118", "TEXT_BUBBLE_HELP_TEXT_156"),
    tool(1, 0x10, 254.0, 0.0, 109.0, 56.0, "G058", "TEXT_BUBBLE_HELP_TEXT_167"), tool(1, 0x17, 304.0, 58.0, 74.0, 65.0, "G121", "TEXT_BUBBLE_HELP_TEXT_157"),
    tool(1, 2, 13.0, 59.0, 71.0, 48.0, "G124", "TEXT_BUBBLE_HELP_TEXT_153"), tool(1, 3, 88.0, 65.0, 73.0, 48.0, "G127", "TEXT_BUBBLE_HELP_TEXT_154"),
    tool(1, 0x2f, 164.0, 66.0, 74.0, 47.0, "G130", "TEXT_BUBBLE_HELP_TEXT_160"), tool(1, 0x30, 242.0, 70.0, 64.0, 43.0, "G133", "TEXT_BUBBLE_HELP_TEXT_161"),
    tool(2, 0xe0, 10.0, 2.0, 51.0, 59.0, "G019", "TEXT_BUBBLE_HELP_TEXT_180"), tool(2, 0xde, 60.0, 4.0, 79.0, 55.0, "G022", "TEXT_BUBBLE_HELP_TEXT_177"),
    tool(2, 0xe1, 132.0, 2.0, 94.0, 46.0, "G025", "TEXT_BUBBLE_HELP_TEXT_179"), tool(2, 0xe4, 220.0, 2.0, 94.0, 46.0, "G025 a", "TEXT_NEW_CTEXT_255"),
    tool(2, 0xdc, 5.0, 45.0, 78.0, 72.0, "G028", "TEXT_BUBBLE_HELP_TEXT_175"), tool(2, 0xe2, 85.0, 45.0, 78.0, 72.0, "G028 a", "TEXT_NEW_CTEXT_252"),
    tool(2, 0xdf, 165.0, 48.0, 62.0, 64.0, "G031", "TEXT_BUBBLE_HELP_TEXT_178"), tool(2, 0xdd, 230.0, 57.0, 73.0, 59.0, "G034", "TEXT_BUBBLE_HELP_TEXT_176"),
    tool(2, 0xe3, 310.0, 52.0, 73.0, 59.0, "G034 a", "TEXT_NEW_CTEXT_253"), tool(2, 0xe5, 310.0, 0.0, 73.0, 59.0, "G034 b", "TEXT_NEW_CTEXT_254"),
    tool(3, 0x12, 10.0, 20.0, 80.0, 45.0, "G037", "TEXT_BUBBLE_HELP_TEXT_165"), tool(3, 0x154, 104.0, 20.0, 84.0, 48.0, "G040", "TEXT_BUBBLE_HELP_TEXT_169"),
    tool(3, 10, 194.0, 21.0, 72.0, 44.0, "G043", "TEXT_BUBBLE_HELP_TEXT_164"), tool(3, 0xc, 271.0, 0.0, 108.0, 57.0, "G046", "TEXT_BUBBLE_HELP_TEXT_166"),
    tool(3, 5, 19.0, 66.0, 78.0, 43.0, "G049", "TEXT_BUBBLE_HELP_TEXT_168"), tool(3, 7, 110.0, 69.0, 78.0, 39.0, "G052", "TEXT_BUBBLE_HELP_TEXT_163"),
    tool(3, 0x13, 295.0, 60.0, 64.0, 46.0, "G055", "TEXT_BUBBLE_HELP_TEXT_162"), tool(3, 0x155, 202.0, 60.0, 76.0, 60.0, "G118", "TEXT_BUBBLE_HELP_TEXT_156"),
    tool(4, 4, 2.0, 5.0, 67.0, 47.0, "G079", "TEXT_BUBBLE_HELP_TEXT_181"), tool(4, 8, 70.0, 6.0, 62.0, 47.0, "G085", "TEXT_BUBBLE_HELP_TEXT_183"),
    tool(4, 0xe, -2.0, 64.0, 75.0, 48.0, "G094", "TEXT_BUBBLE_HELP_TEXT_186"), tool(4, 0xf, 72.0, 66.0, 68.0, 49.0, "G097", "TEXT_BUBBLE_HELP_TEXT_187"),
    tool(4, 0x11, 162.0, 62.0, 58.0, 52.0, "G103", "TEXT_BUBBLE_HELP_TEXT_184"), tool(4, 0x31, 224.0, 58.0, 90.0, 47.0, "G106", "TEXT_BUBBLE_HELP_TEXT_185"),
    tool(5, 0x2a, 87.0, 0.0, 42.0, 63.0, "G064", "TEXT_BUBBLE_HELP_TEXT_172"), tool(5, 0x29, 10.0, 0.0, 60.0, 63.0, "G061", "TEXT_BUBBLE_HELP_TEXT_171"),
    tool(5, 0x28, 156.0, 0.0, 55.0, 63.0, "G067", "TEXT_BUBBLE_HELP_TEXT_170"), tool(5, 0x2b, 222.0, 0.0, 67.0, 63.0, "G070", "TEXT_BUBBLE_HELP_TEXT_173"),
    tool(5, 0x87, 306.0, 0.0, 40.0, 63.0, "G076", "TEXT_BUBBLE_HELP_TEXT_174"), tool(5, 0x82, 18.0, 65.0, 63.0, 52.0, "G139", "TEXT_BUBBLE_HELP_TEXT_174"),
    tool(5, 0x99, 90.0, 65.0, 65.0, 52.0, "G136", "TEXT_BUBBLE_HELP_TEXT_174"), tool(5, 0x9a, 158.0, 65.0, 66.0, 52.0, "G142", "TEXT_BUBBLE_HELP_TEXT_174"),
    tool(5, 0x9b, 228.0, 65.0, 67.0, 52.0, "G145", "TEXT_BUBBLE_HELP_TEXT_174"), tool(5, 0x9c, 300.0, 65.0, 64.0, 52.0, "G148", "TEXT_BUBBLE_HELP_TEXT_174"),
    tool(6, 0xcd, 62.0, 4.0, 63.0, 89.0, "G004", "TEXT_BUBBLE_HELP_TEXT_193"), tool(6, 0xce, 126.0, 4.0, 66.0, 89.0, "G007", "TEXT_BUBBLE_HELP_TEXT_192"),
    tool(6, 0xcf, 193.0, 4.0, 66.0, 89.0, "G010", "TEXT_BUBBLE_HELP_TEXT_191"), tool(6, 0xd0, 261.0, 6.0, 58.0, 89.0, "G013", "TEXT_BUBBLE_HELP_TEXT_190"),
    tool(6, 0xd1, 317.0, 4.0, 64.0, 89.0, "G016", "TEXT_BUBBLE_HELP_TEXT_194"),
    // the water menu's Flow button (ButtonMEWaterFlow: MAPPER_RIPPLE, drag from a river tile the way it should flow)
    tool(4, 0x97, 104.0, 45.0, 75.0, 30.0, "G157", "TEXT_BUBBLE_HELP_TEXT_189"),
    // the delete brush, ME 'Key' (ButtonME: MAPPER_DELETE_EDITOR, 60×125 at 12, 21 of MEMenuGrid; menu 0 = always)
    tool(0, 0x15d, 12.0, 21.0, 60.0, 125.0, "E001", "TEXT_BUBBLE_HELP_TEXT_146"),
];
/// ME menu tabs (WrapPanel 100×137 at 134, 12 of MEMenuGrid, two per row): sprite and help.
const TABS: [(&str, &str, f32, f32); 6] = [("E004", "TEXT_BUBBLE_HELP_TEXT_147", 46.0, 47.0), ("E007", "TEXT_BUBBLE_HELP_TEXT_150", 47.0, 47.0),
    ("E010", "TEXT_BUBBLE_HELP_TEXT_148", 46.0, 44.0), ("E013", "TEXT_BUBBLE_HELP_TEXT_151", 47.0, 44.0), ("E016", "TEXT_BUBBLE_HELP_TEXT_149", 46.0, 47.0),
    ("E019", "TEXT_BUBBLE_HELP_TEXT_152", 47.0, 47.0)];

/// "UI-Buttons G109" + k: the sprite after `s` by k numbers ("G025 a" keeps its suffix).
fn sprite(s: &str, k: u32) -> String {
    let (head, tail) = s.split_once(' ').map_or((s, ""), |(h, t)| (h, t));
    let n: u32 = head[1..].parse().unwrap_or(0);
    let t = if tail.is_empty() { String::new() } else { format!(" {tail}") };
    format!("UI-Buttons {}{:03}{t}", &head[..1], n + k)
}

#[derive(Clone, Copy, PartialEq)]
enum H { Tab(u8), Tool(i32), Brush, Name, Save, Exit, Radar, Mode(bool), Screen(usize), Build(usize), Shield(i32), Scenario, Panel, Good(usize, i32), Preset(usize) }

/// What a click on the editor HUD asks main.rs to do: save, leave, DLL_StartMapAction (a building / troop tool, 0 =
/// none), DLL_SetEditorPlayer, a DLL_GameAction (the scenario setters).
pub enum Out { Save(PathBuf), Exit, Tool(i32), Player(i32), Action([i32; 3]) }

/// The map as the sim has it (live.rs `editor_info`, after each frame): the scenario's starting goods (0x1832c5748,
/// 25 goods), which players have a keep (their start position), multiplayer map, map type (GameModes), editor player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Info { pub goods: [i32; 25], pub keeps: [bool; 8], pub mp: bool, pub map_type: i32, pub player: i32 }

impl Info {
    /// GameData.getNumShieldsToDisplayInEditor: 8 on a multiplayer map, 5 on an invasion, else 1.
    pub fn shields(&self) -> i32 { if self.mp { 8 } else if self.map_type == 3 { 5 } else { 1 } }
}

/// ME troop / siege / ruins tabs right of the build tabs (HUD_Main BottomTabs1: left margin, sprite, build screen, help).
const ME_TABS: [(f32, &str, usize, &str); 5] = [(42.0, "A025", 15, "TEXT_BUBBLE_HELP_TEXT_234"), (6.0, "A029", 16, "TEXT_BUBBLE_HELP_TEXT_233"),
    (6.0, "A063", 19, "TEXT_NEW_CTEXT_219"), (6.0, "A049", 12, "TEXT_BUBBLE_HELP_TEXT_315"), (6.0, "A059", 20, "TEXT_NEW_CTEXT_220")];
/// MESheildPanel's player buttons 1..8 (Sprite1; rollover +1, checked +2).
const SHIELDS: [&str; 8] = ["H016", "H010", "H013", "H007", "H019", "H022", "H025", "H028"];
/// The ruins pages (ButtonBuildRuinsPageToggle1..4 go round them).
const RUINS: [usize; 4] = [15, 17, 18, 21];
/// GameData.startingGoodsLimits: the goods the scenario editor's Starting Goods view offers (limit > 0) and their
/// slider's curve (MainViewModel.GoodsCurve200 / 1000 / 5000, gold 5000 × 10).
const LIMITS: [i32; 25] = [0, 0, 5000, 200, 1000, 0, 200, 0, 200, 200, 1000, 1000, 1000, 1000, 200, 50000, 200, 200, 200, 200, 200, 200, 200, 200, 200];
const CURVE200: [i32; 50] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50,
    55, 60, 65, 70, 75, 80, 85, 90, 95, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200];
const CURVE1000: [i32; 50] = [0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 35, 40, 45, 50, 60, 70, 80, 100, 120, 140,
    160, 180, 200, 220, 240, 260, 280, 300, 350, 400, 450, 500, 550, 600, 650, 700, 750, 800, 850, 900, 1000];
const CURVE5000: [i32; 50] = [0, 1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 25, 30, 35, 40, 50, 60, 70, 80, 90, 100, 120, 140, 160, 180, 200,
    250, 300, 350, 400, 450, 500, 550, 600, 700, 800, 900, 1000, 1200, 1400, 1600, 1800, 2000, 2500, 3000, 3500, 4000, 4500, 5000];
/// MainViewModel low / medium / high starting goods presets: goods 2, 4, 6, 8, 13, 12, 11, 10, 15.
const PRESET_GOODS: [usize; 9] = [2, 4, 6, 8, 13, 12, 11, 10, 15];
const PRESETS: [[i32; 9]; 3] = [[40, 0, 0, 0, 10, 10, 0, 0, 50], [120, 20, 5, 5, 50, 50, 20, 20, 200], [500, 100, 25, 25, 200, 200, 50, 50, 1000]];

/// SliderScenarioStartingGoods: good `g`'s amount one slider stop up / down from `v` (initSliderScenarioStartingGoods
/// finds the stop: the first curve value ≥ v).
pub fn goods_step(g: usize, v: i32, up: bool) -> i32 {
    let (curve, k) = match LIMITS[g] { 200 => (&CURVE200, 1), 1000 => (&CURVE1000, 1), 5000 => (&CURVE5000, 1), _ => (&CURVE5000, 10) };
    let at = curve.iter().position(|&c| v / k <= c).unwrap_or(49) as i32;
    curve[(at + if up { 1 } else { -1 }).clamp(0, 49) as usize] * k
}

/// Where our top-left controls start (design px): right of OnScreenText's item counts (OST_PeopleLeft, 170 wide).
const LEFT: f32 = 200.0;

/// A troop tool (EditorDirector.placeEditorTroop: placed on release).
fn is_troop(t: i32) -> bool { matches!(t, 270..=286 | 350..=357 | 400..=407) }
/// preDLLCallActions' tools that get DLL_MapAction every frame with constructingOnly (walls, stairs, moats, pitch).
fn is_line(t: i32) -> bool { matches!(t, 25 | 26 | 27 | 44 | 45 | 46 | 99 | 106 | 107) }

/// The running editor's HUD: terrain mode (MEMode 0: the open menu, the brush tool and size, C# brush 1..9; 7 = 169
/// tiles, InitEditorMode) or building mode (MEMode 1: the build screen, the building / troop tool, the editor player),
/// the map's name and whether keys go into it, the scenario panel, the last note (saved / error), the held mouse
/// (press tile, when the stroke last repeated on it), the map info from the sim.
pub struct Hud {
    pub menu: u8,
    pub tool: i32,
    pub brush: i32,
    pub build: bool,
    pub screen: usize,
    pub place: i32,
    pub name: String,
    pub typing: bool,
    pub scenario: bool,
    pub down: bool, // the left button is held on the map (a stroke)
    pub exit: bool, // Main Menu was pressed
    pub info: Info,
    note: Option<(String, Instant)>,
    held: Option<([i32; 2], Instant, bool)>,
    hover_at: [i32; 2],
    hits: Vec<([f32; 4], H)>,
    ours: Option<PathBuf>, // the map file this editing session loaded or saved: Save replaces it without asking
    armed: Option<PathBuf>, // another map of that name: the next Save replaces it
}

impl Hud {
    pub fn new(start: &Start) -> Self {
        Hud { menu: 1, tool: 2, brush: 7, build: false, screen: 1, place: 0, name: start.name(), typing: false, scenario: false, down: false, exit: false,
            info: Info { player: 1, ..Default::default() }, note: None, held: None, hover_at: [i32::MIN; 2], hits: vec![],
            ours: match start { Start::Load(p) => Some(p.clone()), _ => None }, armed: None }
    }

    pub fn over(&self, c: [f32; 2]) -> bool { self.hits.iter().any(|(p, _)| Menu::inside(*p, c)) }

    /// The file Save Map writes: the name in the player's Maps folder.
    pub fn path(&self) -> PathBuf {
        let clean: String = self.name.chars().filter(|c| !"/\\:*?\"<>|".contains(*c)).collect();
        maps_dir().join(format!("{}.map", if clean.trim().is_empty() { "New Map" } else { clean.trim() }))
    }

    pub fn note(&mut self, s: String) { self.note = Some((s, Instant::now())); }

    /// Save Map's file, None = a map of that name that isn't this one is there: asked first, the next Save replaces it.
    fn save_to(&mut self) -> Option<PathBuf> {
        let p = self.path();
        if p.exists() && self.ours.as_ref() != Some(&p) && self.armed.as_ref() != Some(&p) {
            self.note(format!("{} exists: Save again to replace it", p.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned())));
            self.armed = Some(p);
            return None;
        }
        (self.ours, self.armed) = (Some(p.clone()), None);
        Some(p)
    }

    /// The tool held now (MainControls.CurrentAction != 0): the building / troop tool, else the brush.
    pub fn holding(&self) -> bool { if self.build { self.place != 0 } else { self.tool != 0 } }

    /// MainControls.StopAllPlacement (a right press on the map): the tool goes; Out::Tool(0) = DLL_StartMapAction(0).
    pub fn drop_tool(&mut self) -> Out {
        (self.tool, self.place, self.held) = (if self.build { self.tool } else { 0 }, 0, None);
        Out::Tool(0)
    }

    /// A left press on the HUD: the action, Some(None) = taken without one, None = not on the HUD.
    pub fn click(&mut self, c: [f32; 2], ui: &mut Ui) -> Option<Option<Out>> {
        let &(p, h) = self.hits.iter().rev().find(|(p, _)| Menu::inside(*p, c))?;
        self.typing = false;
        match h {
            H::Tab(m) => self.menu = m,
            H::Tool(t) => self.tool = t,
            H::Brush => self.brush = self.brush % 9 + 1, // RightClickSizeCommand: the next size, round
            H::Name => self.typing = true,
            H::Save => return Some(self.save_to().map(Out::Save)),
            H::Exit => return Some(Some(Out::Exit)),
            H::Radar => {
                let n = crate::live::RADAR_N as f32;
                ui.radar_click = Some([((c[0] - p[0]) / (p[2] - p[0]) * n) as i32, ((c[1] - p[1]) / (p[3] - p[1]) * n) as i32]);
            }
            // EditorDirector.changeEditorMode: StopAllPlacement, then the other mode's panel
            H::Mode(b) if b != self.build => {
                (self.build, self.place, self.held) = (b, 0, None);
                return Some(Some(Out::Tool(0)));
            }
            H::Mode(_) | H::Panel => {}
            H::Screen(s) => (self.screen, self.place) = (s, 0),
            H::Build(i) => {
                let b = &ui.build[i];
                if let Some(s) = crate::hud::submenu(&b.name) {
                    self.screen = s;
                } else if b.name.contains("RuinsPageToggle") {
                    let k = RUINS.iter().position(|&r| r == self.screen).unwrap_or(0);
                    self.screen = RUINS[(k + 1) % RUINS.len()];
                } else if b.mapper > 0 {
                    // EditorDirector.placeBuildingInteraction: DLL_StartMapAction (the delete tool too)
                    self.place = b.mapper;
                    return Some(Some(Out::Tool(b.mapper)));
                }
            }
            H::Shield(p) => return Some(Some(Out::Player(p))), // MainViewModel.ButtonME(MAPPER_REPORTn): SetEditorPlayerID
            H::Scenario => self.scenario = !self.scenario,
            // Scenario_Set_StartingGoods (GameAction 2004) one slider stop up / down
            H::Good(g, d) => return Some(Some(Out::Action([2004, g as i32, goods_step(g, self.info.goods[g], d > 0)]))),
            H::Preset(k) => {
                // ButtonScenarioStartingGoodSelect(-1 / -2 / -3): all goods 0, then the preset's nine
                for g in 0..25 { self.info.goods[g] = 0; }
                for (n, &g) in PRESET_GOODS.iter().enumerate() { self.info.goods[g] = PRESETS[k][n]; }
                return Some(Some(Out::Action([-1, k as i32, 0]))); // main.rs sends the 25 setters (`preset_actions`)
            }
        }
        Some(None)
    }

    /// The 25 Scenario_Set_StartingGoods actions of preset k (low / medium / high).
    pub fn preset_actions(k: usize) -> Vec<[i32; 3]> {
        (0..25).map(|g| [2004, g as i32, PRESET_GOODS.iter().position(|&p| p == g).map_or(0, |n| PRESETS[k][n])]).collect()
    }

    /// A key while typing the name: text, Backspace, Return / Escape end it.
    pub fn key(&mut self, k: &winit::keyboard::Key) {
        use winit::keyboard::{Key, NamedKey};
        match k {
            Key::Named(NamedKey::Backspace) => { self.name.pop(); }
            Key::Named(NamedKey::Enter | NamedKey::Escape) => self.typing = false,
            Key::Named(NamedKey::Space) if self.name.len() < 40 => self.name.push(' '),
            Key::Character(s) if self.name.len() < 40 => self.name.push_str(s),
            _ => {}
        }
    }

    /// The stroke at map tile `t` (None = off the map / over the HUD) with the button `down`: the DLL_MapAction calls
    /// to send now (EditorDirector.preDLLCallActions). Terrain brushes: hover (0) when the tile changes, press (1),
    /// drag (2) on each new tile and, for the height / ground brushes, again on the same tile after 1 s and then every
    /// 50 ms (performEditorConstantAction; plants, rocks, animals and ripple only once per tile), release (3).
    /// Building mode: buildings hover (constructing) and press; troops hover and are placed on release, `n` at once
    /// (Shift 5, Ctrl 20, Alt 50); walls, stairs, moats and pitch get every state constructing; the delete tool (39)
    /// press and drag as in game (EngineInterface.DeleteBuilding, inGame 1).
    pub fn input(&mut self, t: Option<[i32; 2]>, down: bool, n: usize) -> Vec<crate::live::EdIn> {
        let tool = if self.build { self.place } else { self.tool };
        if tool == 0 { self.held = None; return vec![]; }
        let mut v = vec![];
        let now = Instant::now();
        let once = self.build || matches!(tool, 0x97 | 0x28..=0x2b | 0x82..=0x9c | 0xcd..=0xd1 | 0xdc..=0xe5);
        match (t, down, self.held) {
            (Some(t), true, None) => { self.held = Some((t, now, false)); v.push((1, t)); }
            (Some(t), true, Some((at, since, rep))) => {
                let wait = if rep { 50 } else { 1000 };
                if t != at {
                    self.held = Some((t, now, false));
                    v.push((2, t));
                } else if !once && now.duration_since(since).as_millis() >= wait {
                    self.held = Some((t, now, true));
                    v.push((2, t));
                }
            }
            (t, false, Some((at, ..))) => { self.held = None; v.push((3, t.unwrap_or(at))); }
            (Some(t), false, None) if t != self.hover_at => v.push((0, t)),
            _ => {}
        }
        if let Some(t) = t { self.hover_at = t; }
        let e = |state: i32, [x, y]: [i32; 2], cons: bool, game: bool| crate::live::EdIn { tool, brush: if self.build { 0 } else { self.brush }, state, x, y, cons, game };
        v.into_iter().flat_map(|(s, t)| match () {
            _ if !self.build => vec![e(s, t, s == 0, false)],
            _ if tool == 39 => if matches!(s, 1 | 2) { vec![e(s, t, false, true)] } else { vec![] },
            _ if is_line(tool) => vec![e(s, t, true, false)],
            _ if is_troop(tool) => match s { 0 => vec![e(0, t, true, false)], 3 => vec![e(3, t, false, false); n], _ => vec![] },
            _ => match s { 0 => vec![e(0, t, true, false)], 1 => vec![e(1, t, false, false)], _ => vec![] },
        }).collect()
    }

    /// The HUD over the game (HUD_Main's 914×166 box bottom-right): terrain mode the UI-HUD 004 frame with MEMenuGrid,
    /// building mode the build menu frame (UI-HUD 000, the build screens, the build and ME troop tabs, the delete
    /// tool) and MESheildPanel's player shields; MEModeToggle above it; the minimap; our name box / Save Map / Main
    /// Menu / Scenario top left (right of the editor's item counts, OnScreenText), the scenario panel under them, the
    /// hovered button's help, the right-click menu.
    pub fn draw(&mut self, ui: &mut Ui, st: &crate::hud::HudState, w: f32, h: f32, cursor: [f32; 2], radar: bool) -> Vec<HudTri> {
        let s = crate::ux::hud_scale(w, h);
        (ui.s, ui.w, ui.h) = (s, w, h);
        let mut out = vec![];
        self.hits.clear();
        if ui.hidden { // the right-click menu's "UI off": only that menu
            ui.rclick_draw(&mut out);
            return out;
        }
        let br = |x: f32, y: f32, bw: f32, bh: f32| [w - (1920.0 - x) * s, h - (1080.0 - y) * s, w - (1920.0 - x - bw) * s, h - (1080.0 - y - bh) * s];
        let (ox, oy) = (1920.0 - 914.0, 1080.0 - 166.0); // HUD_Main's box
        let white = [1.0; 4];
        let mut tip: Option<String> = None;
        let button = |out: &mut Vec<HudTri>, hits: &mut Vec<([f32; 4], H)>, key: &str, p: [f32; 4], on: bool, hit: H, help: &str, tip: &mut Option<String>| {
            let hot = Menu::inside(p, cursor);
            ui.img_fit(out, &sprite(key, if on { 2 } else { hot as u32 }), p, white);
            if hot { *tip = Some(ui.text_of(help)); }
            hits.push((p, hit));
        };
        if !self.build {
            ui.img(&mut out, "UI-HUD 004", br(ox + 114.0, oy, 800.0, 166.0), white);
            let (gx, gy) = (ox + 114.0, oy + 10.0); // MEMenuGrid
            // the six menu tabs
            let (mut tx, mut ty, mut row_h) = (gx + 134.0, gy + 12.0, 0.0f32);
            for (k, (spr, help, tw, th)) in TABS.into_iter().enumerate() {
                if k % 2 == 0 && k > 0 { (tx, ty) = (gx + 134.0, ty + row_h); row_h = 0.0; }
                button(&mut out, &mut self.hits, spr, br(tx, ty, tw, th), self.menu == k as u8 + 1, H::Tab(k as u8 + 1), help, &mut tip);
                tx += tw;
                row_h = row_h.max(th);
            }
            // the open menu's tools (build menu grid 382×118 at 248, 22) and the delete brush
            for t in TOOLS.iter().filter(|t| t.menu == self.menu || t.menu == 0) {
                let p = if t.menu == 0 { br(gx + t.x, gy + t.y, t.w, t.h) } else { br(gx + 248.0 + t.x, gy + 22.0 + t.y, t.w, t.h) };
                button(&mut out, &mut self.hits, t.sprite, p, self.tool == t.mapper, H::Tool(t.mapper), t.help, &mut tip);
            }
            // brush size (Drawing tools grid 40×150 at 80, 6: ButtonBrushSize1..9 at 3, 15; sprites F001 + 3k)
            let p = br(gx + 83.0, gy + 21.0, 36.0, 35.0);
            let hot = Menu::inside(p, cursor);
            let n = 1 + 3 * (self.brush.min(7) - 1) + hot as i32; // F001 / F002 rollover .. F019; 8, 9 = F019x2 / x4
            let key = match self.brush { 8 => format!("F{n:03}x2"), 9 => format!("F{n:03}x4"), _ => format!("F{n:03}") };
            ui.img_fit(&mut out, &format!("UI-Buttons {key}"), p, white);
            if hot { tip = Some(ui.text_of("TEXT_BUBBLE_HELP_TEXT_144")); }
            self.hits.push((p, H::Brush));
        } else {
            // BuildMenuGrid's scroll, the screen's buttons, the frame (as hud.rs draws HUD_Main)
            ui.img(&mut out, "UI-HUD 007", br(ox + 126.0, oy + 38.5, 876.0, 103.0), white);
            for i in 0..ui.build.len() {
                let b = &ui.build[i];
                if b.screen != self.screen || b.name.ends_with('b') && b.name.contains("RuinsPageToggle") { continue; }
                let (s1, s2) = (b.s1.clone(), b.s2.clone());
                let r = ui.rects.get(&s1).copied().unwrap_or([0.0, 0.0, 1.0, 1.0]);
                let (bw, bh) = match (b.w, b.h) { (0.0, h) => (h * r[2] / r[3], h), (w, 0.0) => (w, w * r[3] / r[2]), wh => wh };
                let p = br(ox + 150.0 + b.x, oy + 55.0 + b.y, bw, bh);
                let (hot, on) = (Menu::inside(p, cursor), b.mapper > 0 && self.place == b.mapper);
                if on { ui.img(&mut out, "UI Player Border", [p[0] - 3.0, p[1] - 3.0, p[2] + 3.0, p[3] + 3.0], white); } // as hud.rs's build menu
                ui.img_fit(&mut out, if hot || on { &s2 } else { &s1 }, p, white);
                if hot { tip = Some(ui.text_of(&b.help)); }
                self.hits.push((p, H::Build(i)));
            }
            ui.img(&mut out, "UI-HUD 000", br(ox + 114.0, oy + 22.0, 800.0, 144.0), white);
            // BottomTabs1: the six build tabs, then Ruins / Troops / Siege / Arab troops / Bedouin
            let tab_of = |s: usize| match s { 0 | 7..=9 | 13 | 14 => 1, 10 | 11 => 4, 17 | 18 | 21 => 15, s => s };
            let mut x = 118.0;
            let tabs: Vec<(f32, String, usize, String)> = ui.tabs.iter().enumerate().map(|(t, (k, m))| (*m, k[0].clone(), t + 1, ["TEXT_BUBBLE_HELP_TEXT_002", "TEXT_BUBBLE_HELP_TEXT_003",
                "TEXT_BUBBLE_HELP_TEXT_128", "TEXT_BUBBLE_HELP_TEXT_004", "TEXT_BUBBLE_HELP_TEXT_215", "TEXT_BUBBLE_HELP_TEXT_214"][t.min(5)].to_string()))
                .chain(ME_TABS.iter().map(|&(m, k, s, h)| (m, format!("UI-Buttons {k}"), s, h.to_string()))).collect();
            for (m, key, scr, help) in tabs {
                x += m;
                let key = key.trim_start_matches("UI-Buttons ").to_string();
                button(&mut out, &mut self.hits, &key, br(ox + x, oy + 166.0 - 35.0, 30.0, 35.0), tab_of(self.screen) == scr, H::Screen(scr), &help, &mut tip);
                x += 30.0;
            }
            // SideTabs' delete tool (GameDeleteButton: STRUCT_NEW_DELETE)
            if let Some(i) = ui.build.iter().position(|b| b.screen == 0 && b.mapper == 0x27) {
                let p = br(ox + 632.0, oy + 109.0, 20.0, 21.0);
                let k = if self.place == 0x27 { "A043" } else if Menu::inside(p, cursor) { "A042" } else { "A041" };
                ui.img_fit(&mut out, &format!("UI-Buttons {k}"), p, white);
                if Menu::inside(p, cursor) { tip = Some(ui.text_of("TEXT_BUBBLE_HELP_TEXT_146")); }
                self.hits.push((p, H::Build(i)));
            }
            // MESheildPanel (95×144 bottom-right): the editor players, two per row; a keep placed = a start position
            ui.img(&mut out, "UI-HUD 002", br(ox + 819.0, oy + 22.0, 95.0, 144.0), white);
            for k in 0..self.info.shields() as usize {
                let r = ui.rects.get(&format!("UI-Buttons {}", SHIELDS[k])).copied().unwrap_or([0.0, 0.0, 31.0, 31.0]);
                let sw = 31.0 * r[2] / r[3];
                let p = br(ox + 819.0 + 15.0 + (k % 2) as f32 * (sw + 6.0), oy + 22.0 + 11.0 + (k / 2) as f32 * 31.0, sw, 31.0);
                let help = format!("{}{}", ui.text_of(&format!("TEXT_BUBBLE_HELP_TEXT_{}", 206 + k)), if self.info.keeps[k] { " (keep placed)" } else { "" });
                let hot = Menu::inside(p, cursor);
                ui.img_fit(&mut out, &sprite(SHIELDS[k], if self.info.player == k as i32 + 1 { 2 } else { hot as u32 }), p, white);
                if self.info.keeps[k] { Ui::fill(&mut out, [p[2] - 7.0 * s, p[3] - 7.0 * s, p[2] - 2.0 * s, p[3] - 2.0 * s], [0.2, 0.9, 0.2, 1.0]); }
                if hot { tip = Some(help); }
                self.hits.push((p, H::Shield(k as i32 + 1)));
            }
        }
        // MEModeToggle (top right of HUD_Main, 24 px above it): buildings H001, terrain H004
        for (k, (spr, bw, help)) in [("H001", 48.0, "TEXT_BUBBLE_HELP_TEXT_002"), ("H004", 47.0, "TEXT_NEW_TEXT_016")].into_iter().enumerate() {
            let x = ox + 914.0 - 47.0 - if k == 0 { 48.0 } else { 0.0 };
            button(&mut out, &mut self.hits, spr, br(x, oy - 24.0, bw, 46.0), self.build == (k == 0), H::Mode(k == 0), help, &mut tip);
        }
        // minimap: in UI-HUD 004's box right of the tools (the game's RadarMapGrid spot would cover the last tools);
        // in building mode left of the shields (RadarMargin 0,0,103,8)
        if radar {
            let p = if self.build { br(1920.0 - 103.0 - 124.0, 945.0, 124.0, 124.0) } else { br(1786.0, 945.0, 124.0, 124.0) };
            Ui::fill(&mut out, [p[0] - 2.0 * s, p[1] - 2.0 * s, p[2] + 2.0 * s, p[3] + 2.0 * s], [0.0, 0.0, 0.0, 0.8]);
            quad(&mut out, 2, p, [0.0, 1.0, 1.0, 0.0], white);
            self.hits.push((p, H::Radar));
        }
        // ours, top left: the map's name, Save Map, Main Menu, Scenario
        let tl = |x: f32, y: f32, bw: f32, bh: f32| [x * s, y * s, (x + bw) * s, (y + bh) * s];
        let blink = self.typing && std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() / 500) % 2 == 0;
        let name = format!("{}{}", self.name, if blink { "|" } else { "" });
        let save = ui.text_of("TEXT_MAPEDIT_004");
        let exit = ui.text_of("TEXT_MAPEDIT_006");
        let scen = ui.text_of("TEXT_SCENARIO_003");
        let bar = [(name, H::Name, 360.0), (save, H::Save, 180.0), (exit, H::Exit, 180.0), (scen, H::Scenario, 220.0)];
        let mut x = LEFT;
        for (label, hit, bw) in bar {
            let p = tl(x, 16.0, bw, 44.0);
            x += bw + 12.0;
            let hot = Menu::inside(p, cursor);
            if hit == H::Name { // the name box
                widget(&mut out, |o| Ui::text_field(o, p, hot || self.typing, s));
                ui.text_sh(&mut out, &label, p[0] + 10.0 * s, p[1] + 31.0 * s, 24.0, 0.0, if self.typing { GOLD } else { CREAM });
            } else {
                widget(&mut out, |o| ui.text_button(o, &label, p, hot, hit == H::Scenario && self.scenario, true));
            }
            self.hits.push((p, hit));
        }
        let path = self.path();
        let file = path.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned());
        let starts: Vec<String> = (0..8).filter(|&p| self.info.keeps[p]).map(|p| (p + 1).to_string()).collect();
        let hint = if self.info.mp {
            format!("Saves as Maps/{file} | start positions (keeps): {} - two or more and Skirmish lists it", if starts.is_empty() { "none".into() } else { starts.join(", ") })
        } else {
            format!("Saves as Maps/{file} (the Sandbox lists it)")
        };
        ui.text_sh(&mut out, &hint, LEFT * s, 84.0 * s, 16.0, 0.0, DIM);
        if let Some((n, t)) = &self.note {
            if t.elapsed().as_secs() < 6 { ui.text_sh(&mut out, n, LEFT * s, 108.0 * s, 20.0, 0.0, GOLD); }
        }
        if self.scenario { self.scenario_panel(ui, &mut out, cursor, s); }
        if let Some(t) = tip {
            let tw = ui.text(&mut vec![], &t, 0.0, 0.0, 18.0, 1.0, 0.0, 0.0, white);
            let (tx, ty) = (cursor[0].min(w - tw - 8.0), cursor[1] - 14.0);
            Ui::fill(&mut out, [tx - 4.0, ty - 22.0 * s, tx + tw + 4.0, ty + 6.0 * s], [0.0, 0.0, 0.0, 0.6]);
            ui.text_sh(&mut out, &t, tx, ty, 18.0, 0.0, CREAM);
        }
        encode(&mut out); // the menu's colour constants are linear (menu.rs `draw`)
        ui.on_screen_text(st, &mut out); // OnScreenText: the editor's item counts top left (ost.rs)
        ui.rclick_draw(&mut out); // HUD_RightClick while the right button is held
        out
    }

    /// HUD_Scenario, the part a Skirmish / Free Build map needs: its Starting Goods view (every good with a limit,
    /// one slider stop per − / +, the low / medium / high presets). A multiplayer map's goods come from the lobby: the
    /// panel says so (the original shows only the mission text there).
    fn scenario_panel(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, cursor: [f32; 2], s: f32) {
        let tl = |x: f32, y: f32, bw: f32, bh: f32| [x * s, y * s, (x + bw) * s, (y + bh) * s];
        let goods: Vec<usize> = (0..25).filter(|&g| LIMITS[g] > 0).collect();
        let rows = goods.len().div_ceil(2);
        let p = tl(LEFT, 124.0, 640.0, 110.0 + rows as f32 * 34.0);
        widget(out, |o| ui.panel(o, p, s)); // Menu::panel's look
        self.hits.push((p, H::Panel)); // the panel takes its clicks (under its buttons)
        let head = ui.text_of("TEXT_STARTUP_001");
        ui.text_sh(out, &head, p[0] + 16.0 * s, p[1] + 34.0 * s, 24.0, 0.0, GOLD);
        if self.info.mp {
            ui.text_sh(out, "A multiplayer map: the lobby sets the starting goods.", p[0] + 16.0 * s, p[1] + 70.0 * s, 18.0, 0.0, CREAM);
            return;
        }
        for (k, &g) in goods.iter().enumerate() {
            let (cx, cy) = (LEFT + 16.0 + (k / rows) as f32 * 310.0, 124.0 + 56.0 + (k % rows) as f32 * 34.0);
            let name = ui.text_of(&format!("TEXT_GOODS_{:03}", g + 1));
            ui.text_sh(out, &name, cx * s, (cy + 22.0) * s, 18.0, 0.0, CREAM);
            let v = self.info.goods[g].to_string();
            ui.text_sh(out, &v, (cx + 225.0) * s, (cy + 22.0) * s, 18.0, 0.5, GOLD);
            for (d, dx) in [(-1, 175.0), (1, 255.0)] {
                let b = tl(cx + dx, cy, 26.0, 28.0);
                widget(out, |o| ui.text_button(o, if d < 0 { "-" } else { "+" }, b, Menu::inside(b, cursor), false, true));
                self.hits.push((b, H::Good(g, d)));
            }
        }
        let y = 124.0 + 64.0 + rows as f32 * 34.0;
        for k in 0..3 {
            let label = ui.text_of(&format!("TEXT_NEW_TEXT2_{:03}", 68 + k));
            let b = tl(LEFT + 16.0 + k as f32 * 200.0, y, 180.0, 36.0);
            widget(out, |o| ui.text_button(o, &label, b, Menu::inside(b, cursor), false, true));
            self.hits.push((b, H::Preset(k)));
        }
    }
}

/// The editor's "Saved X" / "Can't save X: why" line for a `Cmd::SaveMap` result.
fn save_note(r: Result<std::path::PathBuf, String>) -> String {
    let name = |p: &std::path::Path| p.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned());
    match r { Ok(p) => format!("Saved {}", name(&p)), Err(e) => format!("Can't save: {e}") }
}

/// A map file into `m` for editing. On the UI thread: a sim panic on a bad map is an error, not the app's end.
#[cfg(feature = "live")]
pub(crate) fn load_map(m: &mut shc_sim::mem::Mem, f: &std::path::Path) -> Result<(), String> {
    let b = std::fs::read(f).map_err(|e| e.to_string())?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| shc_sim::editor::load(m, &b).map_err(|e| format!("{e:?}"))))
        .unwrap_or_else(|e| Err(crate::skirmish::panic_text(e)))
}


// ---- main.rs's side: start the editor, its clicks and strokes ----

impl crate::App {
    /// EditorDirector.createNewMap / loadMapIntoEditor: the sim set up for editing (shc_sim::editor), run like a live
    /// game (no replay recording), the camera on the map's centre (or its first keep), the editor HUD up.
    #[cfg(feature = "live")]
    pub(crate) fn start_editor(&mut self, s: Start) {
        let mut m = shc_sim::mem::Mem::default();
        let r = match &s {
            Start::New { size, mode } => {
                let (ty, mp) = Start::map_type(*mode);
                shc_sim::editor::new_map(&mut m, *size, ty, mp);
                Ok(())
            }
            Start::Load(f) => load_map(&mut m, f),
        };
        if let Err(e) = r {
            if let Some(mm) = &mut self.menu { mm.error = Some(format!("Can't open the map: {e}")); }
            return;
        }
        (self.menu, self.last_lobby, self.campaign) = (None, None, None);
        let mut l = crate::live::Live::from_mem(m);
        l.tap = None;
        let cam = Some(crate::replay::to_west_pub(l.rotation(), 400, 400));
        self.start_live(l, cam);
        self.editor = Some(Hud::new(&s));
    }

    /// A left press / release while editing: the editor HUD's buttons (`Out`), else the stroke on the map
    /// (editor_frame sends it). False = not editing.
    pub(crate) fn editor_button(&mut self, down: bool) -> bool {
        let Some(e) = self.editor.as_mut() else { return false };
        if down {
            let hit = match self.ui.as_mut() { Some(u) => e.click(self.cam.cursor, u), None => None };
            let l = self.play.as_ref().and_then(|p| p.live.as_ref());
            match hit {
                Some(Some(Out::Save(path))) => {
                    if let Some(l) = l { l.send(crate::live::Cmd::SaveMap(path)); } // the note comes with the result (editor_frame)
                }
                Some(Some(Out::Exit)) => e.exit = true,
                Some(Some(Out::Tool(t))) => if let Some(l) = l { l.send(crate::live::Cmd::Tool(t)) },
                Some(Some(Out::Player(p))) => if let Some(l) = l { l.send(crate::live::Cmd::EditorPlayer(p)) },
                Some(Some(Out::Action([-1, k, _]))) => if let Some(l) = l { for a in Hud::preset_actions(k as usize) { l.send(crate::live::Cmd::Action(a)) } },
                Some(Some(Out::Action(a))) => if let Some(l) = l { l.send(crate::live::Cmd::Action(a)) },
                Some(None) => {}
                None => { e.down = true; return true; }
            }
            return true;
        }
        e.down = false;
        true
    }

    /// A right press while editing (EditorDirector: HUD_RightClick.Open unless a tool is held, then
    /// StopAllPlacement). False = not editing.
    pub(crate) fn editor_right(&mut self, pressed: bool) -> bool {
        let Some(e) = self.editor.as_mut() else { return false };
        if !pressed {
            if let Some(u) = &mut self.ui { u.rclick = None; }
            return true;
        }
        if e.over(self.cam.cursor) { return true; }
        let held = e.holding();
        let out = e.drop_tool();
        let l = self.play.as_ref().and_then(|p| p.live.as_ref());
        if let (Some(l), Out::Tool(t)) = (l, out) { l.send(crate::live::Cmd::Tool(t)); }
        if let (false, Some(u), Some(l)) = (held, self.ui.as_mut(), l) { u.rclick_open(self.cam.cursor, l.rot); }
        true
    }

    /// Every frame while editing: the map info from the sim, the stroke's DLL_MapAction calls to the sim thread
    /// (Hud::input), Main Menu.
    pub(crate) fn editor_frame(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        if self.editor.as_ref().is_some_and(|e| e.exit) { return self.menu_go(Go::ToMenu, el); }
        let (Some(e), Some(l)) = (self.editor.as_mut(), self.play.as_mut().and_then(|p| p.live.as_mut())) else { return };
        if let Some(i) = *l.ed.lock().unwrap() { e.info = i; }
        if let Some(r) = l.saved.lock().unwrap().take() { e.note(save_note(r)); }
        let t = (!e.over(self.cam.cursor) && self.menu.is_none()).then(|| self.cam.tile_at(l.rot));
        let n = if self.mods.shift_key() { 5 } else if self.mods.control_key() { 20 } else if self.mods.alt_key() { 50 } else { 1 };
        let down = e.down;
        for i in e.input(t, down, n) {
            l.send(crate::live::Cmd::Editor(i));
        }
    }
}

#[test]
fn stroke_states() {
    let mut h = Hud::new(&Start::New { size: 160, mode: 4 });
    let st = |v: Vec<crate::live::EdIn>| v.into_iter().map(|e| (e.state, [e.x, e.y], e.cons)).collect::<Vec<_>>();
    assert_eq!(st(h.input(Some([5, 5]), false, 1)), vec![(0, [5, 5], true)]); // hover
    assert_eq!(st(h.input(Some([5, 5]), false, 1)), vec![]); // same tile: nothing new
    assert_eq!(st(h.input(Some([5, 5]), true, 1)), vec![(1, [5, 5], false)]); // press
    assert_eq!(st(h.input(Some([5, 5]), true, 1)), vec![]); // held, < 1 s
    assert_eq!(st(h.input(Some([6, 5]), true, 1)), vec![(2, [6, 5], false)]); // new tile: drag
    assert_eq!(st(h.input(None, false, 1)), vec![(3, [6, 5], false)]); // release off the map: at the last tile
    // building mode: a hovel hovers and is placed on press only; archers on release, 5 with Shift
    (h.build, h.place) = (true, 0x36);
    assert_eq!(st(h.input(Some([7, 7]), false, 1)), vec![(0, [7, 7], true)]);
    assert_eq!(st(h.input(Some([7, 7]), true, 1)), vec![(1, [7, 7], false)]);
    assert_eq!(st(h.input(Some([8, 7]), true, 1)), vec![]);
    assert_eq!(st(h.input(Some([8, 7]), false, 1)), vec![]);
    h.place = 270;
    assert_eq!(st(h.input(Some([9, 7]), true, 5)), vec![]);
    assert_eq!(st(h.input(Some([9, 7]), false, 5)).len(), 5);
    assert_eq!(goods_step(15, 0, true), 10); // gold: GoodsCurve5000 × 10
    assert_eq!(goods_step(2, 40, true), 50);
    assert_eq!(goods_step(2, 40, false), 35);
    assert_eq!(sprite("G025 a", 1), "UI-Buttons G026 a");
    assert_eq!(Start::map_type(1), (3, false));
    assert_eq!(Start::map_type(4), (0, true));
    // Save over another map of that name asks first; this session's own file is replaced at once
    let dir = std::env::temp_dir().join(format!("shc-editor-save-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Maps")).unwrap();
    crate::skirmish::set_test_profile(dir.clone());
    std::fs::write(dir.join("Maps/New Map.map"), b"x").unwrap();
    let mut h = Hud::new(&Start::New { size: 160, mode: 4 });
    assert_eq!(h.save_to(), None);
    assert_eq!(h.save_to(), Some(dir.join("Maps/New Map.map")));
    assert_eq!(h.save_to(), Some(dir.join("Maps/New Map.map")));
    let mut h = Hud::new(&Start::Load(dir.join("Maps/New Map.map")));
    assert_eq!(h.save_to(), Some(dir.join("Maps/New Map.map")));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(save_note(Ok(dir.join("Maps/A.map"))), "Saved A.map");
    assert_eq!(save_note(Err("disk full".into())), "Can't save: disk full");
}

/// A bad map file (garbage, a truncated one) is an error line, never a panic on the UI thread.
#[cfg(feature = "live")]
#[test]
fn bad_map_is_an_error() {
    let dir = std::env::temp_dir().join(format!("shc-editor-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for (name, bytes) in [("garbage.map", vec![0x5au8; 4096]), ("short.map", vec![0u8; 12]), ("empty.map", vec![])] {
        let f = dir.join(name);
        std::fs::write(&f, bytes).unwrap();
        let mut m = shc_sim::mem::Mem::default();
        assert!(load_map(&mut m, &f).is_err(), "{name}");
    }
    std::panic::set_hook(hook);
    assert!(load_map(&mut shc_sim::mem::Mem::default(), &dir.join("missing.map")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
