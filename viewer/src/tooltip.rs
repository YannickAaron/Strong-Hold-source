//! HUD_Main's rollover (UpdateRollover / CreateBuildingTooltip, MainHUD.xaml "Game Hover Text"): hovering a build-menu
//! button or tab shows its name and, for a building, up to two costs with the amount you have, at a fixed place
//! above the build menu. With Options > Show Extended Building Tooltips the building's line sits in a box with its
//! description and what it consumes / produces. A child module of hud.rs: it draws with Ui's helpers.
use super::*;

/// What the cursor is over: the HoverString (name), the costs (needed, have, icon), the eStructs name (build.tsv).
#[derive(Clone, Debug, Default)]
pub(super) struct Rollover { pub name: String, pub costs: Vec<(i32, i32, &'static str)>, pub structure: String }

/// One piece of a TextBlock line: text, or an inline image (sprite, box width; 18 high, Margin 0,4,0,-4).
enum Seg<'a> { T(String), I(&'a str, Option<f32>) }

const DDD: [f32; 4] = [0xdd as f32 / 255.0, 0xdd as f32 / 255.0, 0xdd as f32 / 255.0, 1.0];

impl Rollover {
    /// UpdateRollover's cost columns for a building: wood, stone, iron and pitch (when > 0), gold; the first two shown.
    pub(super) fn of(name: String, structure: &str, cost: &[i32], res: &[i32; 25]) -> Self {
        // GameSprites 442 / 444 / 445 / 446 / 453 and lastGameState.resources 2 / 4 / 6 / 7 / 15
        const C: [(usize, &str, bool); 5] = [(2, "UISprites E001", false), (4, "UISprites E005", false), (6, "UISprites E007", true),
            (7, "UISprites E009", true), (15, "UISprites E023", false)];
        let costs = C.iter().enumerate().filter(|&(k, &(_, _, pos))| cost.get(k).is_some_and(|&c| if pos { c > 0 } else { c != 0 }))
            .map(|(k, &(g, icon, _))| (cost[k], res[g], icon)).take(2).collect();
        Rollover { name, costs, structure: structure.into() }
    }
}

impl Ui {
    /// Draws (or with `out` None measures) a run of segments from window x, baseline y at `size`; returns its width.
    fn segs(&mut self, mut out: Option<&mut Vec<HudTri>>, segs: &[Seg], x: f32, y: f32, size: f32, color: [f32; 4]) -> f32 {
        let s = self.s;
        let mut cx = x;
        for g in segs {
            match g {
                Seg::T(t) => cx += match out.as_deref_mut() { Some(o) => self.text_sh(o, t, cx, y, size, 0.0, color), None => self.tw(t, size) * s },
                Seg::I(key, bw) => {
                    let r = self.rects.get(*key).copied().unwrap_or([0.0, 0.0, 1.0, 1.0]);
                    let w = (18.0 * r[2] / r[3]).min(bw.unwrap_or(f32::MAX));
                    let bw = bw.unwrap_or(w);
                    if let Some(o) = out.as_deref_mut() {
                        let x0 = cx + (bw - w) / 2.0 * s;
                        self.img(o, key, [x0, y - 14.0 * s, x0 + w * s, y + 4.0 * s], [1.0; 4]); // bottom 4 under the baseline
                    }
                    cx += bw * s;
                }
            }
        }
        cx - x
    }

    /// The rollover: "  name   12 [wood](30)   5 [stone](0)" in 20 px #DDDDDD. Plain: GameHoverText on #55000000, left
    /// at x 1140 (550-wide grid at margin -20 right, TranslateTransform −250), bottom 950 (RollOverText_Margin 130).
    /// Extended (a building with a CreateBuildingTooltip entry): the 500-wide grid at margin 0,0,280,125 holds a
    /// #88000000 box with that line, the description (14 px cream, wrapped at 450, margin 10,1,10,5) and the
    /// Consumes / Produces row (17 px, icons 25×18).
    pub(super) fn draw_rollover(&mut self, out: &mut Vec<HudTri>, r: &Rollover, lord_type: u8) {
        let s = self.s;
        let mut line = vec![Seg::T(format!("  {}", r.name))];
        for (k, &(need, have, icon)) in r.costs.iter().enumerate() {
            line.push(Seg::T(format!("   {need} ")));
            line.push(Seg::I(icon, None));
            line.push(Seg::T(if k == 1 { format!("({have})  ") } else { format!("({have})") }));
        }
        let lh = |ui: &Self, size: f32| ui.font.horizontal_line_metrics(size).map_or((size * 1.25, size), |m| (m.new_line_size, m.ascent));
        let (lh20, asc20) = lh(self, 20.0);
        let ext = crate::menu::building_tooltips().then(|| TOOLTIPS.iter().find(|t| t.0 == r.structure)).flatten();
        let Some(&(_, body, arab, consumes, produces)) = ext else {
            let w = self.segs(None, &line, 0.0, 0.0, 20.0, DDD);
            let [x, y] = self.br(1140.0, 950.0 - lh20);
            Self::fill(out, [x, y, x + w, y + lh20 * s], [0.0, 0.0, 0.0, 0x55 as f32 / 255.0]);
            self.segs(Some(out), &line, x, y + asc20 * s, 20.0, DDD);
            return;
        };
        let arab_lord = matches!(lord_type, 1 | 2 | 6 | 7);
        let body = self.text_of(arab.filter(|_| arab_lord).unwrap_or(body));
        let lines = self.wrap(&body, 14.0, 450.0);
        let (lh14, asc14) = lh(self, 14.0);
        let (lh17, asc17) = lh(self, 17.0);
        let icons = |v: &'static [&'static str]| v.iter().map(|k| Seg::I(k, Some(25.0))).collect::<Vec<_>>();
        let mut row: Vec<Vec<Seg>> = vec![];
        for (key, v) in [("TEXT_NEW_TEXT2_079", consumes), ("TEXT_NEW_TEXT2_080", produces)] {
            if v.is_empty() { continue; }
            let mut segs = vec![Seg::T(self.text_of(key)), Seg::T("  ".into())];
            segs.extend(icons(v));
            row.push(segs);
        }
        // sizes in design px
        let w1 = self.segs(None, &line, 0.0, 0.0, 20.0, DDD) / s;
        let wb = lines.iter().map(|l| self.tw(l, 14.0)).fold(0.0, f32::max) + 20.0;
        let wr: f32 = row.iter().map(|g| self.segs(None, g, 0.0, 0.0, 17.0, DDD) / s + 20.0).sum();
        let bw = w1.max(wb).max(wr);
        let bh = lh20 + 1.0 + lh14 * lines.len() as f32 + 5.0 + if row.is_empty() { 0.0 } else { lh17 + 2.0 };
        let (x0, y0) = (1140.0, 1080.0 - 125.0 - bh);
        let [px, py] = self.br(x0, y0);
        Self::fill(out, [px, py, px + bw * s, py + bh * s], [0.0, 0.0, 0.0, 0x88 as f32 / 255.0]);
        self.segs(Some(out), &line, px, py + asc20 * s, 20.0, DDD);
        let mut y = y0 + lh20 + 1.0;
        for l in &lines {
            let [x, yy] = self.br(x0 + 10.0, y + asc14);
            self.text_sh(out, l, x, yy, 14.0, 0.0, CREAM);
            y += lh14;
        }
        y += 5.0;
        let mut x = x0;
        for g in &row {
            let [px, py] = self.br(x + 10.0, y + asc17);
            x += self.segs(Some(out), g, px, py, 17.0, DDD) / s + 20.0;
        }
    }
}

#[test]
fn rollover_costs() {
    let mut res = [0; 25];
    (res[2], res[4], res[15]) = (30, 7, 500);
    // wood 12, stone 5, iron 0, pitch -1 (only > 0 counts), gold 10: the first two columns
    let r = Rollover::of("Keep".into(), "STRUCT_KEEP_ONE", &[12, 5, 0, -1, 10], &res);
    assert_eq!(r.costs, vec![(12, 30, "UISprites E001"), (5, 7, "UISprites E005")]);
    let r = Rollover::of("Church".into(), "STRUCT_CHURCH1", &[0, 0, 0, 0, 250], &res);
    assert_eq!(r.costs, vec![(250, 500, "UISprites E023")]);
    assert!(TOOLTIPS.iter().any(|t| t.0 == "STRUCT_WOODCUTTERS_HUT" && t.4 == ["UISprites E001"]));
}

/// HUD_Main.CreateBuildingTooltip (generated from the decompiled switch): eStructs -> body text key, the Arab
/// lord's body (churches), what it consumes, what it produces (sprites; empty = that line hidden).
const TOOLTIPS: &[(&str, &str, Option<&str>, &[&str], &[&str])] = &[
    ("STRUCT_APPLEFARM", "TEXT_BUILDING_DESCRIPTIONS_061", None, &[], &["UISprites A010"]),
    ("STRUCT_ARMOURERS_WORKSHOP", "TEXT_BUILDING_DESCRIPTIONS_046", None, &["UISprites E007"], &["UISprites A021"]),
    ("STRUCT_ARMOURY", "TEXT_BUILDING_DESCRIPTIONS_008", None, &[], &[]),
    ("STRUCT_BAKERS_WORKSHOP", "TEXT_BUILDING_DESCRIPTIONS_048", None, &["UISprites A013"], &["UISprites A007"]),
    ("STRUCT_BALLISTA", "TEXT_BUILDING_DESCRIPTIONS_052", None, &[], &[]),
    ("STRUCT_BARRACKS_STONE", "TEXT_BUILDING_DESCRIPTIONS_007", None, &[], &[]),
    ("STRUCT_BARRACKS_WOOD", "TEXT_BUILDING_DESCRIPTIONS_058", None, &[], &[]),
    ("STRUCT_BEDOUIN_STOCKADE", "TEXT_BUILDING_DESCRIPTIONS_059", None, &[], &[]),
    ("STRUCT_BLACKSMITHS_WORKSHOP", "TEXT_BUILDING_DESCRIPTIONS_044", None, &["UISprites E007"], &["UISprites A018", "UISprites A019"]),
    ("STRUCT_BRAZIER", "TEXT_BUILDING_DESCRIPTIONS_018", None, &[], &[]),
    ("STRUCT_BREWERS_WORKSHOP", "TEXT_BUILDING_DESCRIPTIONS_050", None, &["UISprites A002"], &["UISprites A011"]),
    ("STRUCT_BURNING_STAKE", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_CATTLEFARM", "TEXT_BUILDING_DESCRIPTIONS_064", None, &[], &["UISprites A008"]),
    ("STRUCT_CESS_PIT", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_CHOPPING_BLOCK", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_CHURCH1", "TEXT_BUILDING_DESCRIPTIONS_035", Some("TEXT_ISLAMIC_004"), &[], &[]),
    ("STRUCT_CHURCH2", "TEXT_BUILDING_DESCRIPTIONS_035", Some("TEXT_ISLAMIC_005"), &[], &[]),
    ("STRUCT_CHURCH3", "TEXT_BUILDING_DESCRIPTIONS_065", Some("TEXT_ISLAMIC_006"), &[], &[]),
    ("STRUCT_CRENAL_WALL", "TEXT_BUILDING_DESCRIPTIONS_006", None, &[], &[]),
    ("STRUCT_DANCING_BEAR", "TEXT_BUILDING_DESCRIPTIONS_040", None, &[], &[]),
    ("STRUCT_DOG_CAGE", "TEXT_BUILDING_DESCRIPTIONS_015", None, &[], &[]),
    ("STRUCT_DRAWBRIDGE", "TEXT_BUILDING_DESCRIPTIONS_014", None, &[], &[]),
    ("STRUCT_DUNGEON", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_DUNKING_STOOL", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_ENGINEERS_GUILD", "TEXT_BUILDING_DESCRIPTIONS_019", None, &[], &[]),
    ("STRUCT_FLAG1", "TEXT_BUILDING_DESCRIPTIONS_041", None, &[], &[]),
    ("STRUCT_FLAG2", "TEXT_BUILDING_DESCRIPTIONS_041", None, &[], &[]),
    ("STRUCT_FLAG3", "TEXT_BUILDING_DESCRIPTIONS_041", None, &[], &[]),
    ("STRUCT_FLAG4", "TEXT_BUILDING_DESCRIPTIONS_041", None, &[], &[]),
    ("STRUCT_FLETCHERS_WORKSHOP", "TEXT_BUILDING_DESCRIPTIONS_042", None, &["UISprites E001"], &["UISprites A014", "UISprites A015"]),
    ("STRUCT_GALLOWS", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_GARDEN_LARGE", "TEXT_BUILDING_DESCRIPTIONS_040", None, &[], &[]),
    ("STRUCT_GARDEN_MED", "TEXT_BUILDING_DESCRIPTIONS_040", None, &[], &[]),
    ("STRUCT_GARDEN_SMALL", "TEXT_BUILDING_DESCRIPTIONS_040", None, &[], &[]),
    ("STRUCT_GATE_STONE1A", "TEXT_BUILDING_DESCRIPTIONS_013", None, &[], &[]),
    ("STRUCT_GATE_STONE1B", "TEXT_BUILDING_DESCRIPTIONS_013", None, &[], &[]),
    ("STRUCT_GATE_STONE2A", "TEXT_BUILDING_DESCRIPTIONS_013", None, &[], &[]),
    ("STRUCT_GATE_STONE2B", "TEXT_BUILDING_DESCRIPTIONS_013", None, &[], &[]),
    ("STRUCT_GIBBET", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_GOODS_YARD", "TEXT_BUILDING_DESCRIPTIONS_003", None, &[], &[]),
    ("STRUCT_GRANARY", "TEXT_BUILDING_DESCRIPTIONS_047", None, &[], &[]),
    ("STRUCT_HEAD_ON_SPIKE", "TEXT_BUILDING_DESCRIPTIONS_039", None, &[], &[]),
    ("STRUCT_HEALER", "TEXT_BUILDING_DESCRIPTIONS_036", None, &[], &[]),
    ("STRUCT_HOPSFARM", "TEXT_BUILDING_DESCRIPTIONS_063", None, &[], &["UISprites A002"]),
    ("STRUCT_HOVEL", "TEXT_BUILDING_DESCRIPTIONS_034", None, &[], &[]),
    ("STRUCT_HUNTERS_HUT", "TEXT_BUILDING_DESCRIPTIONS_029", None, &[], &["UISprites A009"]),
    ("STRUCT_INN", "TEXT_BUILDING_DESCRIPTIONS_051", None, &["UISprites A011"], &[]),
    ("STRUCT_IRON_MINE", "TEXT_BUILDING_DESCRIPTIONS_026", None, &[], &["UISprites E007"]),
    ("STRUCT_KEEP_ONE", "TEXT_BUILDING_DESCRIPTIONS_002", None, &[], &[]),
    ("STRUCT_KEEP_THREE", "TEXT_BUILDING_DESCRIPTIONS_002", None, &[], &[]),
    ("STRUCT_KEEP_TWO", "TEXT_BUILDING_DESCRIPTIONS_002", None, &[], &[]),
    ("STRUCT_KILLING_PIT", "TEXT_BUILDING_DESCRIPTIONS_017", None, &[], &[]),
    ("STRUCT_MANGONEL", "TEXT_BUILDING_DESCRIPTIONS_053", None, &[], &[]),
    ("STRUCT_MAYPOLE", "TEXT_BUILDING_DESCRIPTIONS_040", None, &[], &[]),
    ("STRUCT_MILL", "TEXT_BUILDING_DESCRIPTIONS_049", None, &["UISprites A006"], &["UISprites A013"]),
    ("STRUCT_NEW_DIG_MOAT", "TEXT_BUILDING_DESCRIPTIONS_055", None, &[], &[]),
    ("STRUCT_NEW_FILL_MOAT", "TEXT_BUILDING_DESCRIPTIONS_056", None, &[], &[]),
    ("STRUCT_OIL_SMELTER", "TEXT_BUILDING_DESCRIPTIONS_022", None, &[], &[]),
    ("STRUCT_OUTPOST", "TEXT_BUILDING_DESCRIPTIONS_060", None, &[], &[]),
    ("STRUCT_OUTPOST_ARAB", "TEXT_BUILDING_DESCRIPTIONS_060", None, &[], &[]),
    ("STRUCT_OUTPOST_BEDOUIN", "TEXT_BUILDING_DESCRIPTIONS_060", None, &[], &[]),
    ("STRUCT_OXEN_BASE", "TEXT_BUILDING_DESCRIPTIONS_025", None, &[], &[]),
    ("STRUCT_PEOPLE_ARAB_ASSASIN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ARAB_BALLISTA", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ARAB_BOW", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ARAB_GRENADIER", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ARAB_HORSEMAN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ARAB_SLAVE", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ARAB_SLINGER", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ARAB_SWORDSMAN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ARCHERS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BATTERING_RAMS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BEDOUIN_AMBUSHER", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BEDOUIN_CAMEL_LANCER", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BEDOUIN_DEMOLISHER", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BEDOUIN_EUNUCH", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BEDOUIN_HEALER", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BEDOUIN_HEAVY_CAMEL", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BEDOUIN_SAPPER", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_BEDOUIN_SKIRMISHER", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_CATAPULTS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ENGINEERS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_ENGINEERS_POTS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_KNIGHTS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_LADDERMEN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_MACEMEN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_MONKS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_PIKEMEN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_PORTABLE_SHIELDS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_SIEGE_TOWERS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_SPEARMEN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_SWORDSMEN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_TREBUCHETS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_TUNNELERS", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PEOPLE_XBOWMEN", "TEXT_NEW_CTEXT_333", None, &[], &[]),
    ("STRUCT_PITCH_DIGGER", "TEXT_BUILDING_DESCRIPTIONS_027", None, &[], &["UISprites E009"]),
    ("STRUCT_PITCH_DITCH", "TEXT_BUILDING_DESCRIPTIONS_016", None, &[], &[]),
    ("STRUCT_POLETURNERS_WORKSHOP", "TEXT_BUILDING_DESCRIPTIONS_043", None, &["UISprites E001"], &["UISprites A017", "UISprites A016"]),
    ("STRUCT_QUARRY", "TEXT_BUILDING_DESCRIPTIONS_024", None, &[], &["UISprites E005"]),
    ("STRUCT_RACK_STRETCHING", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_SHRINE", "TEXT_BUILDING_DESCRIPTIONS_040", None, &[], &[]),
    ("STRUCT_STABLES", "TEXT_BUILDING_DESCRIPTIONS_020", None, &[], &[]),
    ("STRUCT_STAIRS", "TEXT_BUILDING_DESCRIPTIONS_054", None, &[], &[]),
    ("STRUCT_STATUE", "TEXT_BUILDING_DESCRIPTIONS_040", None, &[], &[]),
    ("STRUCT_STOCKS", "TEXT_BUILDING_DESCRIPTIONS_038", None, &[], &[]),
    ("STRUCT_STONE_WALL", "TEXT_BUILDING_DESCRIPTIONS_005", None, &[], &[]),
    ("STRUCT_SUB_MENU_GATEHOUSES_STONELARGE", "TEXT_BUILDING_DESCRIPTIONS_013", None, &[], &[]),
    ("STRUCT_SUB_MENU_GATEHOUSES_STONESMALL", "TEXT_BUILDING_DESCRIPTIONS_013", None, &[], &[]),
    ("STRUCT_TANNERS_WORKSHOP", "TEXT_BUILDING_DESCRIPTIONS_045", None, &["UI-Buttons J007"], &["UISprites A020"]),
    ("STRUCT_TOWER1", "TEXT_BUILDING_DESCRIPTIONS_009", None, &[], &[]),
    ("STRUCT_TOWER2", "TEXT_BUILDING_DESCRIPTIONS_010", None, &[], &[]),
    ("STRUCT_TOWER3", "TEXT_BUILDING_DESCRIPTIONS_011", None, &[], &[]),
    ("STRUCT_TOWER4", "TEXT_BUILDING_DESCRIPTIONS_012", None, &[], &[]),
    ("STRUCT_TOWER5", "TEXT_BUILDING_DESCRIPTIONS_012", None, &[], &[]),
    ("STRUCT_TRADEPOST", "TEXT_BUILDING_DESCRIPTIONS_028", None, &[], &[]),
    ("STRUCT_TUNNELLERS_GUILD", "TEXT_BUILDING_DESCRIPTIONS_021", None, &[], &[]),
    ("STRUCT_WATERPOT", "TEXT_BUILDING_DESCRIPTIONS_057", None, &[], &[]),
    ("STRUCT_WELL", "TEXT_BUILDING_DESCRIPTIONS_037", None, &[], &[]),
    ("STRUCT_WHEATFARM", "TEXT_BUILDING_DESCRIPTIONS_062", None, &[], &["UISprites A006"]),
    ("STRUCT_WOODCUTTERS_HUT", "TEXT_BUILDING_DESCRIPTIONS_023", None, &[], &["UISprites E001"]),
    ("STRUCT_WOOD_WALL", "TEXT_BUILDING_DESCRIPTIONS_004", None, &[], &[]),
];
