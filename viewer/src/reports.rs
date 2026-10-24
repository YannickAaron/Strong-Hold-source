//! Reports book (the scribe's book): MainHUD.xaml's "Reports Book" button and HUD_Buildings.xaml's report pages,
//! drawn over the building frame while the engine is in app mode 16 with a report sub-mode (0x42..=0x4f). Spec:
//! side-session notes reports-spec.md (HUD_Buildings.xaml, MainViewModel.ButtonReports, FatControler
//! .NoesisGUIUpdateChecksInGame). A child module of hud.rs: it draws with Ui's helpers. Read-only: the values are the
//! PlayStateReturnData fields the engine fills every tick (`Data`); a page change is DLL_SetAppMode(16, sub) through
//! `Ui::sub_mode`, as directSetAppSubMode does. No close button, as in the game: right-click / Escape leave mode 16.
use super::*;

/// The PlayStateReturnData fields the pages show (EngineInterface FieldOffset; sim hudstate::fill writes them).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    total: i32, food: i32, tax: i32, crowd: i32, fear_pop: i32, religion: i32, church: i8, ale: i16,
    events: [i32; 7], // fair, marriage, jester, plague, wolves, bandits, fire (popularity)
    good: i32, bad: i32, fear: i32, next: i32, efficiency: i32,
    graph: Vec<i16>, months: i16, // population graph (short[300]) and its month count
    not_eatable: [i16; 4],        // bread, cheese, meat, apples
    troops: Vec<i16>,             // troop_counts[34] by Enums.Troops order (1-based in the C#)
    priests: i16, blessed: i16, blessed_next: i16, blessed_pop: i16,
    desc: [u8; 2], // TEXT_PLAYER_DESC indices of the motto
}

impl Data {
    pub fn decode(b: &[u8]) -> Self {
        let i = |o: usize| b.get(o..o + 4).map_or(0, |v| i32::from_le_bytes(v.try_into().unwrap()));
        let h = |o: usize| b.get(o..o + 2).map_or(0, |v| i16::from_le_bytes(v.try_into().unwrap()));
        let u = |o: usize| b.get(o).copied().unwrap_or(0);
        Data {
            total: i(0x7c), food: i(0x88), tax: i(0x8c), crowd: i(0x90), fear_pop: i(0x94), religion: i(0x98), church: u(0x4fa) as i8,
            ale: h(0x3fc), events: [0x9c, 0xb0, 0xb4, 0xa0, 0xa4, 0xa8, 0xac].map(i),
            good: i(0xb8), bad: i(0xbc), fear: i(0xc0), next: i(0xc4), efficiency: i(0xc8),
            graph: (0..300).map(|k| h(0xcc + 2 * k)).collect(), months: h(0x59a),
            not_eatable: [0, 1, 2, 3].map(|k| h(0x324 + 2 * k)), troops: (0..34).map(|k| h(0x32c + 2 * k)).collect(),
            priests: h(0x370), blessed: h(0x372), blessed_next: h(0x378), blessed_pop: h(0x4f8), desc: [u(0x415), u(0x416)],
        }
    }
}

const WHITE: [f32; 4] = [1.0; 4];

/// MainViewModel.imageFileNames (Enums.eImages): Assets/GUI/Images files, the sketches from 12 on.
const IMAGES: [&str; 108] = [
    "CrusaderDE_Logo.png", "CrusaderSH_Logo.png", "credits1.png", "credits2.png", "credits3.png", "credits4.png", "credits5.png",
    "credits6.png", "", "", "", "", "house_sketch.png", "woodcutter_sketch.png", "ox_tether_sketch.png", "iron_sketch.png", "pitch_sketch.png",
    "hunter_hut_sketch.png", "fletcher_sketch.png", "bsmith_sketch.png", "pole_sketch.png", "armourer_sketch.png", "tanner_sketch.png",
    "bakery_sketch.png", "brewer_sketch.png", "quarry_sketch.png", "inn_sketch.png", "healers_sketch.png", "well_sketch.png",
    "oil_smelter_sketch.png", "wheat_sketch.png", "hop_sketch.png", "fruit_sketch.png", "dairy_sketch.png", "mill_sketch.png",
    "stables_sketch.png", "church_sketch.png", "keep_sketch.png", "campfire_sketch.png", "tower_sketch.png", "gallows_sketch.png",
    "stocks_sketch.png", "maypole_sketch.png", "gardens_sketch.png", "killing_pits_sketch.png", "cess_pit_sketch.png", "stake_sketch.png",
    "gibbet_sketch.png", "dungeon_sketch.png", "stretching_rack_sketch.png", "chopping_block_sketch.png", "ducking_stool_sketch.png",
    "dog_cage_sketch.png", "statue_sketch.png", "dancing_bear_sketch.png", "ponds_sketch.png", "army_sketch.png", "baker_sketch.png",
    "blacksmith_sketch.png", "chicken_sketch.png", "child_sketch.png", "crossbowman_sketch.png", "drunk_sketch.png", "farmer_sketch.png",
    "fearfactor_sketch.png", "firewatch_sketch.png", "food_sketch.png", "ghost_sketch.png", "heads_on_spikes_sketch.png", "healer_sketch.png",
    "hunter_sketch.png", "innkeeper_sketch.png", "iron_miner_sketch.png", "jester_sketch.png", "mother_sketch.png", "null_sketch.png",
    "pitchworker_sketch.png", "poleturner_sketch.png", "popularity_sketch.png", "population_sketch.png", "priest_sketch.png",
    "religion_sketch.png", "siege_engineer_sketch.png", "stockpile_sketch.png", "stone quarry.png", "stonemason_sketch.png",
    "trader_sketch.png", "tunnelor_sketch.png", "weapons_sketch.png", "wedding_sketch.png", "lady_sketch.png", "blacksmith_char_sketch.png",
    "tanner_char_sketch.png", "woodcutter_char_sketch.png", "brewer_char_sketch.png", "fletcher_char_sketch.png", "waterpot_sketch.png",
    "tunnelors_guild_sketch.png", "mosque_sketch.png", "imam_sketch.png", "engineers_guild_sketch.png", "hunters_post_dog_sketch.png",
    "tunnel_sketch.png", "signpost_sketch.png", "tent_sketch.png", "travelling_fair_sketch.png", "travelling_fair_2_sketch.png",
    "arabic_statue_sketch.png"
];
/// HUD_Buildings.sketchList: the sketch image per building type (eStructs).
const SKETCH_LIST: [u8; 110] = [
    0, 12, 0, 13, 14, 15, 16, 17, 0, 0, 0, 0, 18, 19, 20, 21, 22, 23, 24, 0, 25, 25, 26, 27, 100, 97, 0, 28, 29, 0,
    30, 31, 32, 33, 34, 35, 36, 36, 36, 0, 37, 37, 37, 37, 37, 39, 39, 39, 39, 39, 102, 0, 103, 0, 104, 38, 0, 0, 0, 0,
    0, 39, 40, 41, 0, 42, 43, 44, 0, 0, 96, 37, 37, 37, 39, 39, 39, 39, 39, 39, 104, 104, 104, 104, 104, 0, 39, 39, 39, 39,
    0, 45, 46, 47, 48, 49, 0, 50, 51, 52, 53, 81, 0, 54, 55, 0, 0, 0, 0, 0,
];

/// HUD_Buildings.GetBuildingSketchImage(type, panel): the sketch's image file, if any. `arab`: an Arab lord's church /
/// statue sketches. ponytail: the people panel (70: by in_chimp_type) isn't drawn by the port, so not here either.
pub(super) fn sketch_file(ty: i32, panel: i32, arab: bool) -> Option<&'static str> {
    let k = match panel {
        71 => 0,
        72 | 69 => 78,
        73 => 64,
        74 => 79,
        75 => 66,
        76 | 66..=68 => 61,
        77 => 83,
        78 => 88,
        79 => if arab { 98 } else { 81 },
        93 => 55,
        _ if (118..=120).contains(&ty) => 43,
        _ => match SKETCH_LIST.get(usize::try_from(ty).ok()?).copied()? as usize {
            36 if arab => 98,
            53 if arab => 107,
            k => k,
        },
    };
    Some(IMAGES[k]).filter(|f| k > 0 && !f.is_empty())
}
/// Army pages (76, 68, 67, 66): troop index i (troop_counts[i − 1]), sprite, width (0 = from the height), height,
/// image right / bottom margin, count right margin (HorizontalAlignment Center inside the 800 px panel).
type Troop = (usize, &'static str, f32, f32, f32, f32, f32);
const ARMY: [&[Troop]; 4] = [
    &[(1, "O001", 38.0, 74.0, 500.0, 36.0, 500.0), (2, "O003", 0.0, 64.0, 430.0, 38.0, 420.0), (3, "O007", 0.0, 70.0, 340.0, 36.0, 340.0),
      (4, "O009", 0.0, 60.0, 250.0, 36.0, 250.0), (5, "O005", 32.0, 75.0, 160.0, 36.0, 160.0), (6, "O011", 0.0, 67.0, 70.0, 36.0, 70.0),
      (7, "O013", 42.0, 74.0, -20.0, 34.0, -20.0), (8, "O017", 0.0, 62.0, -110.0, 36.0, -110.0), (9, "O021", 0.0, 65.0, -200.0, 36.0, -200.0)],
    &[(10, "O015", 35.0, 69.0, 500.0, 36.0, 500.0), (18, "O033", 35.0, 60.0, 420.0, 36.0, 420.0), (12, "O027", 46.0, 56.0, 338.0, 36.0, 340.0),
      (13, "O029", 46.0, 57.0, 250.0, 38.0, 250.0), (14, "O023", 0.0, 40.0, 160.0, 38.0, 160.0), (15, "O025", 46.0, 50.0, 70.0, 38.0, 70.0),
      (16, "M003", 49.0, 41.0, -20.0, 38.0, -20.0), (17, "M005", 40.0, 44.0, -110.0, 38.0, -110.0), (11, "O031", 0.0, 56.0, -190.0, 38.0, -200.0)],
    &[(19, "O035", 0.0, 69.0, 500.0, 36.0, 500.0), (20, "O037", 0.0, 69.0, 428.0, 38.0, 420.0), (21, "O039", 0.0, 69.0, 338.0, 36.0, 340.0),
      (22, "O041", 0.0, 69.0, 250.0, 38.0, 250.0), (23, "O043", 0.0, 69.0, 160.0, 38.0, 160.0), (24, "O045", 0.0, 69.0, 70.0, 38.0, 70.0),
      (25, "O047", 0.0, 69.0, -20.0, 38.0, -20.0), (26, "O049", 0.0, 55.0, -120.0, 38.0, -110.0)],
    &[(27, "O051", 0.0, 69.0, 500.0, 36.0, 500.0), (28, "O053", 0.0, 69.0, 409.0, 38.0, 420.0), (29, "O055", 0.0, 69.0, 338.0, 36.0, 340.0),
      (30, "O057", 0.0, 69.0, 250.0, 38.0, 250.0), (31, "O059", 0.0, 69.0, 160.0, 38.0, 160.0), (32, "O061", 0.0, 69.0, 70.0, 38.0, 70.0),
      (33, "O063", 0.0, 69.0, -20.0, 38.0, -20.0), (34, "O065", 0.0, 69.0, -110.0, 38.0, -110.0)],
];
/// Stores (77) and weapons (78): sprite and goods index per slot, right margins 500, 400, .. -200.
const STORES: [(&str, usize); 8] = [("I001", 2), ("I003", 3), ("I005", 4), ("I007", 6), ("I009", 7), ("I011", 9), ("I021", 14), ("I025", 16)];
const WEAPONS: [(&str, usize); 8] = [("I027", 17), ("I031", 19), ("I035", 21), ("I029", 18), ("I033", 20), ("I037", 22), ("I039", 23), ("I041", 24)];

impl Ui {
    /// MainHUD ReportsControl's book button: 89×82, right / bottom margin 12 / 23, no sprite until hovered (B045);
    /// ButtonReports(0) opens the overview.
    pub(super) fn book(&mut self, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let [x0, y0] = self.br(1920.0 - 12.0 - 89.0, 1080.0 - 23.0 - 82.0);
        let p = [x0, y0, x0 + 89.0 * self.s, y0 + 82.0 * self.s];
        if Self::inside(p, cursor) { self.img_fit(out, "UI-Scribe B045", p, WHITE); }
        self.hits.push((p, Hit::Mode(71)));
    }

    /// A report button or tab: DLL_SetAppMode(16, sub); the book first closes the troops panel (GameAction
    /// CloseTroopsPanel 1040), as ButtonReports does.
    pub(super) fn mode_click(&mut self, sub: i32) {
        if sub == 71 { self.actions.push([1040, 0, 0]); }
        self.sub_mode = Some(sub);
    }

    /// HUD_Buildings.xaml BuildingSketch of the 800×170 frame at `o`: building type `ty`'s (or the report page's) sketch,
    /// 123×109 at margin 14 left / 2 bottom (SketchWidth / SketchHeight); straight from the game folder (video::sketch).
    pub(super) fn sketch(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], ty: i32) {
        if let Some(f) = sketch_file(ty, st.app_sub_mode, matches!(st.lord_type, 1 | 2 | 6 | 7)) {
            crate::video::sketch(out, f, self.pr(o, 14.0, 170.0 - 2.0 - 109.0, 123.0, 109.0));
        }
    }

    /// The report page of `st.app_sub_mode` (0x42..=0x4f) in the building panel's place: frame, title, page, back.
    pub(super) fn reports(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let o = [1120.0, 1080.0 - 170.0];
        let bg = self.pr(o, 0.0, 0.0, 800.0, 170.0);
        self.img(out, "UI-HUD 003", bg, WHITE);
        self.hits.push((bg, Hit::Panel));
        let (d, sub) = (&st.report, st.app_sub_mode);
        let title = match sub { 72 | 69 => 2, 73 => 3, 74 => 4, 75 => 5, 76 | 66..=68 => 6, 77 => 7, 78 => 8, 79 => 9, _ => 0 };
        if title > 0 { let t = self.text_of(&format!("TEXT_REPORT_BUTTONS_{title:03}")); self.label(out, &t, o, 20.0, 24.0, 32.0, 0.0, BLACK); }
        self.sketch(st, out, o, 0); // the page's sketch (the overview's shield is its own)
        match sub {
            71 => self.overview(st, out, o, cursor),
            72 | 69 => self.popularity(d, out, o, sub == 69, cursor),
            73 => self.fear(d, out, o),
            74 => self.population(d, out, o),
            75 => self.food(st, out, o, cursor),
            76 | 68 | 67 | 66 => self.army(st, out, o, cursor),
            77 | 78 => for (k, &(img, g)) in (if sub == 77 { &STORES } else { &WEAPONS }).iter().enumerate() {
                let cx = (800.0 - (500.0 - 100.0 * k as f32)) / 2.0;
                self.bottom_img(out, o, &format!("UISprites {img}"), cx, 104.0, 50.0, 0.0);
                self.label(out, &st.resources[g].to_string(), o, cx, 140.0 - 24.0, 20.0, 0.5, BLACK);
            },
            79 => self.religion(st, out, o),
            _ => {}
        }
        if sub != 71 { self.back(out, o, 140.0, Hit::Mode(71), cursor); }
    }

    /// A 26×26 G020 / G021 button at x 518 (right margin 256), top `y`.
    fn back(&mut self, out: &mut Vec<HudTri>, o: [f32; 2], y: f32, hit: Hit, cursor: [f32; 2]) {
        let p = self.pr(o, 518.0, y, 26.0, 26.0);
        self.button(out, p, "UISprites G020", "UISprites G021", false, cursor, Some(hit));
    }

    /// WGT_Popularity placed 80×40 at (x, y): "+" for gains, F007 / F008 / F009 head (inner 60×40 grid centred).
    fn wgt(&mut self, out: &mut Vec<HudTri>, o: [f32; 2], x: f32, y: f32, v: i32) {
        let s = if v > 0 { format!("+{v}") } else { v.to_string() };
        self.label(out, &s, o, x + 46.0, y + 11.0, 20.0, 1.0, BLACK);
        let face = match v { 1.. => "UISprites F007", 0 => "UISprites F008", _ => "UISprites F009" };
        self.img_fit(out, face, self.pr(o, x + 52.0, y + 10.5, 20.0, 23.0), WHITE);
    }

    /// Image with HorizontalAlignment Center at `cx` and its bottom at `bottom`; a 0 size follows the sprite's aspect.
    fn bottom_img(&mut self, out: &mut Vec<HudTri>, o: [f32; 2], key: &str, cx: f32, bottom: f32, w: f32, h: f32) {
        let Some((_, [_, _, rw, rh], _)) = self.sprite(key) else { return };
        let (w, h) = match (w, h) { (0.0, h) => (h * rw / rh, h), (w, 0.0) => (w, w * rh / rw), wh => wh };
        self.img(out, key, self.pr(o, cx - w / 2.0, bottom - h, w, h), WHITE);
    }

    /// 71: the shield sketch, player name and motto on the scroll, eight report buttons (BTN_SH_Plain Q001 / Q002).
    fn overview(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], cursor: [f32; 2]) {
        let shield = match st.lord_type { 1 | 6 => "ReportsShield2", 2 | 7 => "ReportsShield3", _ => "ReportsShield" };
        self.img(out, shield, self.pr(o, 14.0, 168.0 - 141.0, 247.0, 141.0), WHITE);
        let name = crate::endgame::name(0, 0); // Settings_UserName (Options > Player Settings)
        self.label(out, &name, o, 140.0, 31.0, 16.0, 0.5, BLACK);
        let d = &st.report;
        let motto = format!("{} {} {}", self.text_of(&format!("TEXT_PLAYER_DESC_{:03}", d.desc[0] as i32 + 1)), self.text_of("TEXT_PLAYER_DESC_041"),
            self.text_of(&format!("TEXT_PLAYER_DESC_{:03}", d.desc[1] as i32 + 1)));
        self.label(out, &motto, o, 140.0, 143.0, 16.0, 0.5, BLACK);
        for (k, (x, y)) in [(282.0, 36.0), (282.0, 70.0), (282.0, 104.0), (282.0, 138.0), (412.0, 36.0), (412.0, 70.0), (412.0, 104.0), (412.0, 138.0)].into_iter().enumerate() {
            let p = self.pr(o, x, y, 125.0, 30.0);
            self.button(out, p, "UI-Buttons Q001", "UI-Buttons Q002", false, cursor, Some(Hit::Mode(72 + k as i32)));
            let t = self.text_of(&format!("TEXT_REPORT_BUTTONS_{:03}", k + 2));
            self.label(out, &t, o, x + 62.5, y + 5.0, 16.0, 0.5, BLACK);
        }
    }

    /// 72 popularity / 69 events: the month's popularity parts (/25) and their total; the arrow button swaps pages
    /// (on 72 only while an event counts).
    fn popularity(&mut self, d: &Data, out: &mut Vec<HudTri>, o: [f32; 2], events: bool, cursor: [f32; 2]) {
        let ev: i32 = d.events.iter().sum();
        let rows: [(i32, &str); 7] = if events {
            [(d.events[0], "TEXT_SCENARIO_134"), (d.events[1], "TEXT_SCENARIO_150"), (d.events[2], "TEXT_SCENARIO_151"), (d.events[3], "TEXT_SCENARIO_140"),
             (d.events[4], "TEXT_SCENARIO_146"), (d.events[5], "TEXT_SCENARIO_147"), (d.events[6], "TEXT_SCENARIO_180")]
        } else {
            [(d.food, "TEXT_REPORTS_003"), (d.tax, "TEXT_REPORTS_007"), (d.crowd, "TEXT_REPORTS_008"), (d.fear_pop, "TEXT_REPORTS_013"),
             (d.religion + d.church as i32, "TEXT_REPORTS_011"), (d.ale as i32, "TEXT_REPORTS_012"), (ev, "TEXT_SCENARIO_024")]
        };
        for (k, (v, key)) in rows.into_iter().enumerate() {
            let (x, y, lx) = if k < 4 { (160.0, 28.0 + 28.0 * k as f32, 240.0) } else { (332.0, 28.0 + 28.0 * (k - 4) as f32, 410.0) };
            self.wgt(out, o, x, y, v / 25);
            let t = self.text_of(key);
            self.label(out, &t, o, lx, y + 10.0, 16.0, 0.0, BLACK);
        }
        self.wgt(out, o, 440.0, 132.0, d.total / 25);
        let t = self.text_of("TEXT_REPORTS_005");
        self.label(out, &t, o, 448.0, 143.0, 16.0, 1.0, BLACK);
        if events || ev != 0 { self.back(out, o, 91.0, Hit::Mode(if events { 72 } else { 69 }), cursor); }
    }

    /// 73 fear factor: good / bad things, popularity effect, next level, efficiency, commentary.
    fn fear(&mut self, d: &Data, out: &mut Vec<HudTri>, o: [f32; 2]) {
        self.img_fit(out, "UISprites J010", self.pr(o, 150.0, 82.0, 49.0, 54.0), WHITE);
        self.label(out, &d.good.to_string(), o, 175.0, 140.0, 20.0, 0.5, BLACK);
        self.img_fit(out, "UISprites J011", self.pr(o, 216.0, 72.0, 26.0, 64.0), WHITE);
        self.label(out, &d.bad.to_string(), o, 229.0, 140.0, 20.0, 0.5, BLACK);
        self.wgt(out, o, 470.0, 28.0, d.fear_pop / 25);
        for (y, key) in [(40.0, "TEXT_REPORT_BUTTONS_019"), (84.0, "TEXT_REPORT_BUTTONS_020")] {
            let t = self.text_of(key);
            self.label(out, &t, o, 260.0, y, 16.0, 0.0, BLACK);
        }
        let b = |n: i32| format!("TEXT_REPORT_BUTTONS_{n:03}");
        let (next, amount) = match d.fear {
            5.. => (self.text_of(&b(26)), String::new()),
            ..=-5 => (self.text_of(&b(25)), String::new()),
            _ if d.good > d.bad => (self.text_of(&b(28)), d.next.to_string()),
            _ if d.bad > d.good => (self.text_of(&b(27)), d.next.to_string()),
            _ => (String::new(), String::new()),
        };
        self.label(out, &next, o, 260.0, 62.0, 16.0, 0.0, BLACK);
        self.label(out, &amount, o, 520.0, 62.0, 16.0, 1.0, BLACK);
        self.label(out, &d.efficiency.to_string(), o, 520.0, 84.0, 16.0, 1.0, BLACK);
        let comment = self.text_of(&b(match d.fear { 0 => 11, 1.. => 12, _ => 13 }));
        for (k, line) in self.wrap(&comment, 20.0, 236.0).into_iter().take(2).enumerate() {
            self.label(out, &line, o, 260.0, 110.0 + 24.0 * k as f32, 20.0, 0.0, BLACK);
        }
    }

    /// 74 population: the monthly graph (300 × 100 at 210, 40), a green 4 px line under a white 1 px one, black axes,
    /// the scale's maximum / half on the left and years below (every 5).
    fn population(&mut self, d: &Data, out: &mut Vec<HudTri>, o: [f32; 2]) {
        let n = (d.months.max(0) as usize).min(300);
        let yr0 = ((d.months as i32 - 300) / 12).max(0);
        let max = d.graph[..n].iter().copied().max().unwrap_or(0) as i32;
        let (top, mul, div) = [(8, 12, 1), (16, 6, 1), (24, 4, 1), (32, 3, 1), (50, 2, 1), (100, 1, 1), (200, 1, 2), (300, 1, 3), (400, 1, 4),
            (500, 1, 5), (600, 1, 6), (800, 1, 8), (1000, 1, 10)].into_iter().find(|&(t, _, _)| max <= t).unwrap_or((2000, 1, 20));
        let pt = |l: usize| { let [x, y] = self.br(o[0] + 210.0 + l as f32 + 1.0, o[1] + 40.0 + 100.0 - (d.graph[l] as i32 * mul / div) as f32); [x, y] };
        let pts: Vec<[f32; 2]> = (0..n).map(pt).collect();
        for (w, c) in [(4.0, [0.0, 0x88 as f32 / 255.0, 0.0, 1.0]), (1.0, WHITE)] {
            for p in pts.windows(2) { crate::stats::seg(out, p[0], p[1], w * self.s, c); }
        }
        Self::fill(out, self.pr(o, 209.0, 40.0, 2.0, 100.0), BLACK);
        Self::fill(out, self.pr(o, 210.0, 138.0, 300.0, 2.0), BLACK);
        self.label(out, &top.to_string(), o, 197.0, 40.0, 12.0, 0.5, BLACK);
        self.label(out, &(top / 2).to_string(), o, 197.0, 89.0, 12.0, 0.5, BLACK);
        for (k, x) in [210.0, 270.0, 330.0, 390.0, 450.0, 508.0].into_iter().enumerate() {
            self.label(out, &(yr0 + 5 * k as i32).to_string(), o, x, 143.0, 12.0, 0.5, BLACK);
        }
        let t = self.text_of("TEXT_REPORT_BUTTONS_014");
        self.label(out, &t, o, 360.0, 150.0, 16.0, 0.5, BLACK);
    }

    /// 75 food: one button per food (click: GameAction SetFoodEaten 1006) with its stock, a stop sign when it isn't
    /// eaten, and the help line. ponytail: the stop signs follow the engine's state every frame (the C# keeps a local
    /// copy toggled on click), and Back always returns to the overview (not to the granary it may have come from).
    fn food(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], cursor: [f32; 2]) {
        let help = self.text_of("TEXT_REPORT_BUTTONS_024");
        let lines = self.wrap(&help, 18.0, 300.0);
        for (k, line) in lines.iter().enumerate() {
            self.label(out, line, o, 345.0, 170.0 - 20.0 * (lines.len() - k) as f32, 18.0, 0.5, BLACK);
        }
        for (x, s1, s2, k, good, r) in [(203.0, "I017", "I018", 2, 12, 356.0), (283.0, "I015", "I016", 1, 11, 204.0), (363.0, "I013", "I014", 0, 10, 34.0),
            (443.0, "I019", "I020", 3, 13, -121.0)] {
            let p = self.pr(o, x - 8.0, 56.0, 50.0, 50.0);
            self.button(out, p, &format!("UISprites {s1}"), &format!("UISprites {s2}"), false, cursor, Some(Hit::Act(1006, st.in_structure, k)));
            if st.report.not_eatable[k as usize] != 0 { self.img_fit(out, "UISprites H003", self.pr(o, x, 23.0, 33.0, 41.0), WHITE); }
            self.label(out, &st.resources[good].to_string(), o, (800.0 - r) / 2.0, 128.0 - 26.0, 22.0, 0.5, BLACK);
        }
    }

    /// 76 / 68 / 67 / 66 army: header (fear-factor combat bonus on page 1, total troops on the others), each troop
    /// type's picture and count, the page toggle (76 → 68 → 67 → 66 → 76).
    fn army(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], cursor: [f32; 2]) {
        let d = &st.report;
        let page = match st.app_sub_mode { 76 => 0, 68 => 1, 67 => 2, _ => 3 };
        let b = |n: i32| format!("TEXT_REPORT_BUTTONS_{n:03}");
        let head = match page {
            0 if d.fear != 0 => format!("{} {}%", self.text_of(&b(32)), d.fear * 5),
            0 => self.text_of(&b(31)),
            _ => format!("{} {}", self.text_of(&b(17)), d.troops.iter().map(|&n| n as i32).sum::<i32>()),
        };
        self.label(out, &head, o, 140.0, if page == 0 { 42.0 } else { 40.0 }, 18.0, 0.0, BLACK);
        let arab = matches!(st.lord_type, 1 | 2 | 6 | 7);
        for &(i, img, w, h, r, bm, rt) in ARMY[page] {
            let key = format!("UI-Buildings {img}{}", if img == "O021" && arab { "a" } else { "" });
            self.bottom_img(out, o, &key, (800.0 - r) / 2.0, 170.0 - bm, w, h);
            self.label(out, &d.troops[i - 1].to_string(), o, (800.0 - rt) / 2.0, 138.0, 20.0, 0.5, BLACK);
        }
        let (s1, s2) = if page == 3 { ("UI-Buttons L003", "UI-Buttons L004") } else { ("UI-Buttons L001", "UI-Buttons L002") };
        let p = self.pr(o, 509.0, 40.0, 36.0, 35.0);
        self.button(out, p, s1, s2, false, cursor, Some(Hit::Mode([68, 67, 66, 76][page])));
    }

    /// 79 religion: priests, blessed %, popularity effect, next blessing level, church / cathedral bonus.
    fn religion(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2]) {
        let (d, arab) = (&st.report, matches!(st.lord_type, 1 | 2 | 6 | 7));
        let b = |n: i32| format!("TEXT_REPORT_BUTTONS_{n:03}");
        let priests = format!("{} : {}", self.text_of(if arab { "TEXT_ISLAMIC_009" } else { "TEXT_REPORT_BUTTONS_023" }), d.priests);
        let blessed = if arab { format!("{} % : {}", self.text_of("TEXT_ISLAMIC_011"), d.blessed) } else { format!("{} {}", self.text_of(&b(18)), d.blessed) };
        let effect = self.text_of(&b(19));
        let next = if d.blessed_next != 0 { format!("{} {}%", self.text_of(&b(29)), d.blessed_next) } else { self.text_of(&b(30)) };
        let bonus = match d.church { 25 | 50 | 75 if arab => self.text_of("TEXT_ISLAMIC_010"), 25 => self.text_of(&b(22)), 50 => self.text_of(&b(21)), 75 => self.text_of(&b(33)), _ => String::new() };
        for (y, t) in [(40.0, &priests), (62.0, &blessed), (84.0, &effect), (106.0, &next), (138.0, &bonus)] {
            self.label(out, t, o, 180.0, y, 18.0, 0.0, BLACK);
        }
        let we = self.tw(&effect, 18.0);
        self.wgt(out, o, 180.0 - 30.0 + we + 4.0, 72.0, d.blessed_pop as i32 / 25);
        if matches!(d.church, 25 | 50 | 75) {
            let wb = self.tw(&bonus, 18.0);
            self.wgt(out, o, 180.0 - 25.0 + wb, 126.0, d.church as i32 / 25);
        }
    }
}

/// The overview → popularity → events → army page path as the HUD clicks it: each button asks for DLL_SetAppMode(16,
/// sub); the decoded values match the return data the sim filled.
#[cfg(feature = "live")]
#[test]
fn report_pages() {
    let p = format!("{}/Library/Application Support/CrossOver/Bottles/Steam/drive_c/users/crossover/AppData/LocalLow/Firefly Studios/Stronghold Crusader Definitive Edition/Saves/Crater Lake-1test.sav", std::env::var("HOME").unwrap());
    let (true, Some(mut u)) = (std::path::Path::new(&p).exists(), Ui::load(&crate::cache_dir().join("ui"))) else { return };
    let mut l = crate::live::Live::load(&p);
    l.step();
    let (w, h) = (1920.0, 1080.0);
    // the book: a click on it asks for the overview
    let mut st = l.hud();
    u.draw(&st, w, h, [0.0; 2], 0, None);
    // centre of a panel-local rect (the panel's top-left at 1120, 910 in the 1920×1080 design space, Ui::pr)
    let at = |u: &Ui, x: f32, y: f32, bw: f32, bh: f32| { let p = u.pr([1120.0, 910.0], x, y, bw, bh); [(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0] };
    let book = { let [x, y] = u.br(1920.0 - 12.0 - 89.0, 1080.0 - 23.0 - 82.0); [x + 44.0 * u.s, y + 41.0 * u.s] };
    // with the troops panel up (remembered sub 61), as main.rs sends them: actions (CloseTroopsPanel 1040), then the
    // sub-mode. The other order leaves the game in mode 14: close_troops requests it after SetAppMode's 16.
    l.set_app_mode(14, 61);
    l.step();
    assert!(u.click(book), "book not clickable");
    assert_eq!(u.actions, [[1040, 0, 0]], "CloseTroopsPanel");
    for [a, s, v] in u.actions.drain(..) { l.game_action(a, s, v); }
    l.set_app_mode(16, u.sub_mode.take().expect("book: no sub-mode"));
    l.step();
    st = l.hud();
    assert_eq!((st.app_mode, st.app_sub_mode), (16, 71), "book with the troops panel up");
    let goto = |l: &mut crate::live::Live, u: &mut Ui, mode: (i32, i32)| {
        l.set_app_mode(mode.0, mode.1);
        l.step();
        let st = l.hud();
        assert_eq!((st.app_mode, st.app_sub_mode), mode);
        u.draw(&st, w, h, [0.0; 2], 0, None);
        st
    };
    goto(&mut l, &mut u, (16, 71));
    // overview: the popularity tab (282, 36 in the panel at 1120, 910) asks for 72
    assert!(u.click(at(&u, 282.0, 36.0, 125.0, 30.0)));
    assert_eq!(u.sub_mode.take(), Some(72));
    st = goto(&mut l, &mut u, (16, 72));
    let d = &st.report;
    println!("popularity: total {} food {} tax {} crowd {} fear {} religion {} ale {} events {:?}", d.total, d.food, d.tax, d.crowd, d.fear_pop, d.religion, d.ale, d.events);
    assert_eq!(d.total, st.upcoming_total_popularity, "+0x7c");
    assert_eq!(d.tax, st.tax_popularity, "+0x8c");
    assert_eq!(d.crowd, st.overcrowding_popularity, "+0x90");
    // back (518, 140) returns to the overview
    assert!(u.click(at(&u, 518.0, 140.0, 26.0, 26.0)));
    assert_eq!(u.sub_mode.take(), Some(71));
    // army: total troops = the sum of troop_counts; the toggle walks 76 → 68
    st = goto(&mut l, &mut u, (16, 76));
    println!("army counts {:?}", st.report.troops);
    assert!(u.click(at(&u, 509.0, 40.0, 36.0, 35.0)));
    assert_eq!(u.sub_mode.take(), Some(68));
    for sub in [68, 67, 66, 69, 73, 74, 75, 77, 78, 79] { goto(&mut l, &mut u, (16, sub)); } // every page draws
    st = goto(&mut l, &mut u, (14, 0));
    assert_eq!(st.app_mode, 14, "closed");
}

/// GetBuildingSketchImage picks: by building type (fletcher, Arab lords' church / statue), by report page; every
/// sketch the tables name is in the game's Assets/GUI/Images (when the game is here).
#[test]
fn sketches() {
    assert_eq!(sketch_file(12, 13, false), Some("fletcher_sketch.png"));
    assert_eq!((sketch_file(36, 13, false), sketch_file(36, 13, true)), (Some("church_sketch.png"), Some("mosque_sketch.png")));
    assert_eq!(sketch_file(100, 13, true), Some("arabic_statue_sketch.png"));
    assert_eq!((sketch_file(0, 72, false), sketch_file(0, 79, true), sketch_file(0, 71, false)), (Some("popularity_sketch.png"), Some("mosque_sketch.png"), None));
    assert_eq!((sketch_file(119, 13, false), sketch_file(2, 13, false)), (Some("gardens_sketch.png"), None));
    let dir = crate::paths::game().join("Assets/GUI/Images");
    if !dir.is_dir() { return; }
    for f in IMAGES.iter().skip(12).filter(|f| !f.is_empty()) { assert!(dir.join(f).is_file(), "{f}"); }
}
