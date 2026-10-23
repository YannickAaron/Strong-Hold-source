//! In-game HUD (re/UI.md §5 items 1-4): book, goods bar, build menu, drawn immediate-mode from UI-MasterAtlas.
//! Layout = the game's XAML (MainHUD.xaml, HUD_Main.xaml, HUD_Goods.xaml) in its 1920×1080 design space, scaled like
//! the Noesis Viewbox (uniform, min of both axes) and anchored to the window corners the XAML anchors to.
//! The only input is `HudState`, decoded from the engine's `PlayStateReturnData` (today: replay; later: the Rust sim).
use crate::Vert;
#[path = "reports.rs"] mod reports; // the Reports book (scribe), uses Ui's helpers
#[path = "overlays.rs"] mod overlays; // allies, extreme powers, Free Build menu, control groups
#[path = "tooltip.rs"] mod tooltip; // the build menu's rollover text / extended building tooltips
#[path = "ost.rs"] mod ost; // OnScreenText: paused, speed, keep message, timers, starting goods, editor counts
use std::collections::HashMap;

/// The fields of `PlayStateReturnData` (re/csharp/EngineInterface.cs:2036, 2824 bytes) the HUD shows, plus the
/// selected-troop list the engine fills next to it.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct HudState {
    pub resources: [i32; 25], // index = TEXT_GOODS_(i+1); 15 = gold as a good
    pub popularity: i32,
    pub population: i32,
    pub gold: i32,
    pub housing_cap: i32,
    pub upcoming_total_popularity: i32,
    pub overcrowding_popularity: i32,
    pub month: i16, // 0-based, TEXT_MONTHS_(month+1)
    pub year: i16,
    // panel switch (GameData.cs:2285): 14 + sub 61/62 = troops panel, 16 = building panel `app_sub_mode`
    pub app_mode: i32,
    pub app_sub_mode: i32,
    pub in_structure_type: i32, // building panel title (HUD_Buildings.BuildingTitles)
    pub troops: Vec<(i32, i32)>, // (eChimps type, count) of the selection, by type; from `selectedChimps`
    pub troops_show_disband: u8,
    // keep (sub 2)
    pub tax_rate: i32, // 0..11, 3 = no taxes
    pub tax_amount: i16,
    pub tax_popularity: i32,
    // granary (sub 4)
    pub rationing: i32,
    pub rationing_popularity: i32,
    pub food_clock: i32, // 0..15000
    pub total_food: i32,
    pub months_of_food: i32,
    pub food_types_eaten: i32,
    pub foods_eaten_popularity: i32,
    // workers line (FatControler.addChimpActions)
    pub building_type_sleeping: i16,
    pub have_building_stats: i16,
    pub workers_have: i16,
    pub job_vacancies: i16,
    pub got_keep_access: i16,
    pub turned_off: i16,
    pub working: i16,
    pub inchimp_n_text: i16,
    pub lord_type: u8, // +2816: the local player's lord kind (1, 2, 6, 7 = Arab, FUN_18001ce50)
    pub available: Vec<bool>, // by eMappers id: DLL_IsMapperAvailable (--live only; empty = all)
    pub in_structure: i32, // selected building id (GameAction `s` of the panel buttons)
    pub repairs_allowed: i16,
    pub sleep_allowed: i16,
    pub trade_buy: [i16; 25], // market prices by good (resource index): trade_buy_costs +1056
    pub trade_sell: [i16; 25], // trade_sell_costs +1106
    pub trade_buy_n: [i16; 25],  // trade_buy_amounts +1156: goods one Buy brings (0 = can't buy)
    pub trade_sell_n: [i16; 25], // trade_sell_amounts +1206
    pub trading: [i16; 3],       // trading_current / next / prev_goods +1422
    // recruit panels (FatControler: Show_* = available, IsEnabled = peasants, gold, weapons)
    pub peasants: i16,        // +892 peasants_available_for_troops
    pub horses: i16,          // +1278 total_horses_available
    pub troop_avail: [u8; 8], // +2188 barracks: archer, crossbow, spear, pike, mace, swords, knight
    pub merc_avail: [u8; 8],  // +2600 mercenary post: eChimps 70..76
    pub bed_avail: [u8; 8],   // +2796 Bedouin stockade: eChimps 78..85
    pub guild_avail: [u8; 3], // +2817 monk, engineer, ladderman
    pub ladderman_cost: u8,   // +2832
    pub eunuch_cost: u8,      // +2833
    // troops panel: build menu, catapult, trebuchet, siege tower, ram, shield (+1025..1030), Arab ballista (+2609)
    pub siege: [u8; 7],
    pub patrol_mode: u8,      // +1037 troops_patrol_mode
    pub get_ammo: u8,         // +1031 troops_show_get_ammo (AmmoRecharge button)
    pub cows: u8,             // +1032 troops_show_launch_cow_and_num_cows (1..249: the cow button + its count)
    pub attack_here: u8,      // +1033 troops_show_attack_here_and_type: 1 pour oil, 2 tunnel, else attack here
    pub rocks: u8,            // +1034 troops_show_attack_here_number_rocks (type 3; 255 = 250+)
    pub weapon_avail: [u8; 9], // +1047 weapon_types_available: bows, xbows, spears, pikes, maces, swords, leather, armour, horses
    pub costs: Table<[i32; 5]>, // by eMappers id: wood, stone, iron, pitch, gold (--live; empty = unknown)
    pub coords: Table<i32>,      // by eMappers id: DLL_GetMapperCoord, x << 16 | y or 0x55555555 (--live; empty = XAML)
    pub game_paused: u8, // +2200: the pause latch GameAction Game_Paused (1048) sets (OST_GAME_PAUSED)
    pub keep_storage: [i32; 25], // +1436: starting goods still in the keep (OST_STARTING_GOODS rows)
    pub left: [i16; 10],         // +1984: editor counts / limits: people, buildings, vegetation, rocks, tribes (OST_PEOPLE_LEFT)
    pub got_market: u8,          // +2587: bits 4 granary, 8 armoury placed (the starting goods' speech)
    pub report: reports::Data, // reports book values (reports.rs)
    pub mouse_selector_state: u8, // +2812: ≥ 4 blocks the right-click menu (HUD_RightClick.Open)
    pub speed: f32,      // --live: game speed (sim ticks/s) the sim thread runs at; 0 = unknown (replay)
    pub end: Option<crate::endgame::End>, // --live: the game's end for the local player (endgame::read); None = playing
    pub ev: Events, // --live: the engine's UI events (Live scans every tick's records)
    // building panels (EngineInterface.PlayStateReturnData offsets)
    pub gate_state: i32,  // +976 gatehouse_state: 10 = open / lowered (gate and drawbridge buttons)
    pub dog_cage: i16,    // +968 dog_cage_state: the owner of the dogs let out
    pub hp: [i16; 3],     // +982 can_do_repairs, +984 building_hps_for_repair, +986 building_maxhps_for_repair
    pub inn: [i32; 7],    // +1008 pints_of_ale, +1012 barrels_of_ale, working / total inns, coverage %, its popularity, next level
    pub weapon: [u8; 3],  // +1038 weapon_being_made_now, +1043 _next, +1044 production_no_resources
    pub can_make: [u8; 6], // by goods 17..22 (bows, xbows, spears, pikes, maces, swords): +2597, +1040, +2599, +1042, +2598, +1041
    pub marry: [i16; 8],  // +1256 marry_status, male / female type, text, m_name1, m_name2, f_name1, f_name2 (outpost: state, size, delay)
    pub ai_clock: i16,    // +2202 (siege tents: build progress)
    pub trade_sell_fixed: [i16; 25], // +1372 trade_sell_costs_fixed (price list)
    pub autotrade: [[i16; 25]; 2],   // autotrade_buy_amount +2356, autotrade_sell_amount +2304
    pub autotrade_on: [u8; 25],      // +2408 autotrade_onoff
    pub extreme: (u8, i16),          // +2809 extremeEnabled, +2810 extremeCount (HUD_ExtremePowers)
    pub cg_total: [i16; 10],         // +2446 control_groups_total (< 0 in [0]: none sent)
    pub cg: [[(u8, i16); 4]; 10],    // +2466 control_groups_type / +2506 _count: the first four kinds per group
    pub lords: Lords,                // the allies panel's lord table
    pub hill: Option<crate::koth::Board>, // --live: King of the Hill (koth.rs); None = conquest
}

/// The lords' part of PlayStateReturnData (+2612.., written in skirmish / multiplayer; C# CopyPlayStateStruct moves the
/// per-player ones to index = player, the requests stay as sent) and what `military::hud_extra` reads from the sim.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Lords {
    pub ai: [i16; 9],        // computer_register: AI lord kind, -1 = no AI
    pub names: [i16; 9],     // computer_names: its name variant
    pub human: [i16; 9],     // player_register: -1 = no human
    pub team: [i16; 9],
    pub help: [i16; 9],      // skirmish_needs_help
    pub req: [[i16; 2]; 10], // skirmish_player_requesting_type / _amount by player
    pub order: [i16; 9],     // skirmish_order: 1 attack, 2 defend
    pub order_player: [i16; 9],
    pub order_from: [i16; 9],
    pub ping: [i16; 8],      // pingtimes; Free Build: 0 siege camps, 1 a marker on land, 2 keep spots (its event buttons)
    // --live (military::hud_extra)
    pub me: i32,             // GameData.playerID
    pub in_game: [bool; 9],  // DLL_GetMeritData()[p, 1] >= 0
    pub colours: [u8; 9],    // player -> MP colour index (endgame::radar_colours)
    pub allies_button: bool, // SkirmishModeVisAlly (HUD_Main.SetupModeDependantUI)
    pub free_build: bool,    // Show_HUD_Extras_Button_Freebuild
}

impl Lords {
    /// HUD_AlliesPanel.GetAllyList / GetEnemyList: up to six other lords still in, on the local player's team or not.
    pub fn side(&self, allies: bool) -> Vec<usize> {
        let me = self.me as usize;
        if !(1..9).contains(&me) || allies && !self.in_game[me] { return vec![]; }
        (1..9).filter(|&p| p != me && (self.human[p] != -1 || self.ai[p] != -1) && self.in_game[p] && (self.team[p] == self.team[me]) == allies)
            .take(6).collect()
    }
}

/// The last DLL_GameAction the sim thread ran from a HUD button: (action, result, serial). `military::after_action`
/// sets it (the C# side reads the result straight from the call, e.g. the market's trade errors).
pub static ACTION_RET: std::sync::Mutex<(i32, i32, u32)> = std::sync::Mutex::new((0, 0, 0));

/// The engine's UI events (render records `frontend::run_tick` appends to the draw list) as the C# GameMap keeps
/// them. `Live` scans each tick's records into one of these and hands a copy over with every HudState, so drawing
/// the same state twice is harmless (one-shot events carry a serial).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Events {
    pub goals: Vec<Goal>, // the last tick's 0x0c lines ≥ 0 (GameMap resets GameData.scenario every frame)
    pub timer: Option<[i32; 3]>, // the last tick's line −2 (setEndGameTimer): now, length, TEXT_OBJECTIVES index
    pub message: Option<(i32, i32, u32)>, // the last 0x14 with a kind (OST_MESSAGE_BAR): text section, id, serial
    pub pick: Option<(i32, u32)>,         // the last 0x19 (ctrl + click on a building): eStructs kind, serial
    pub campaign: bool,                   // GameData.game_type == 0 (objective texts differ)
    pub video: crate::video::Engine,      // the engine's video channel (render event 0x15; live.rs scans it)
    pub ost: [ost::Ost; 32],              // OnScreenText slots (records 14 / 15)
}

/// One objective line (GameData.ScenarioEvent).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Goal { pub id: i32, pub complete: i32, pub kind: i32, pub target: i32, pub current: i32 }

static SERIAL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0); // across games: a new Live restarts nothing

impl Events {
    /// GameMap's draw-list switch for one tick's records: 0x0c (r[5] line: ≥ 0 goal, −2 timer; −1 = game end, in
    /// endgame.rs), 0x14 (r[11] kind, r[12] id; kind 0 = sound only), 0x19 (r[17] kind), 0x0e / 0x0f OnScreenText add /
    /// remove (ost.rs).
    pub fn scan(&mut self, recs: &[[i16; 21]]) {
        let next = || SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let (mut goals, mut timer) = (vec![], None);
        for r in recs {
            let r32 = |k: usize| r[k] as i32;
            match r[0] {
                0x0c => {
                    let (target, current) = (r32(18) * 1000 + r32(15), r32(19) * 1000 + r32(16));
                    match r[5] {
                        0.. => goals.push(Goal { id: r32(5), complete: r32(9), kind: r32(17), target, current }),
                        -2 => timer = Some([r32(8) * 10000 + r32(9), r32(17) * 10000 + target, current]),
                        _ => {}
                    }
                }
                0x14 if r[11] != 0 => self.message = Some((r32(11), r32(12), next())),
                0x19 => self.pick = Some((r32(17), next())),
                14 | 15 => self.ost_record(r, next()), // OnScreenText add / remove (ost.rs)
                _ => {}
            }
        }
        (self.goals, self.timer) = (goals, timer);
    }

    /// MainViewModel.InitObjectiveGoodsPanel: the Objectives button shows with goals or a timer with a text.
    pub fn has_goals(&self) -> bool { !self.goals.is_empty() || self.timer.is_some_and(|t| t[2] > 0) }
}

impl HudState {
    /// `b` = PlayStateReturnData, `sel` = numSelectedChimps × (id, type) i32 pairs.
    pub fn decode(b: &[u8], sel: &[u8]) -> Option<Self> {
        if b.len() < 1434 { return None; }
        let i = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let s = |o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
        let u = |o: usize| b.get(o).copied().unwrap_or(0); // fields past the old 1434-byte minimum
        let s2 = |o: usize| i16::from_le_bytes([u(o), u(o + 1)]);
        let mut troops: Vec<(i32, i32)> = vec![];
        for t in sel.chunks_exact(8).map(|c| i32::from_le_bytes(c[4..8].try_into().unwrap())) {
            match troops.iter_mut().find(|(k, _)| *k == t) { Some(e) => e.1 += 1, None => troops.push((t, 1)) }
        }
        troops.sort();
        Some(Self {
            resources: std::array::from_fn(|k| i(k * 4)),
            popularity: i(108), population: i(112), gold: i(116), housing_cap: i(120),
            upcoming_total_popularity: i(124), overcrowding_popularity: i(144),
            month: s(1430), year: s(1432),
            app_mode: i(928), app_sub_mode: i(932), in_structure_type: i(948), troops, troops_show_disband: b[1024],
            tax_rate: i(884), tax_amount: s(890), tax_popularity: i(140),
            rationing: i(904), rationing_popularity: i(128), food_clock: i(908), total_food: i(912),
            months_of_food: i(916), food_types_eaten: i(920), foods_eaten_popularity: i(132),
            building_type_sleeping: s(990), have_building_stats: s(992), workers_have: s(994), job_vacancies: s(996),
            got_keep_access: s(1000), turned_off: s(1002), working: s(1004), inchimp_n_text: s(970),
            lord_type: b.get(2816).copied().unwrap_or(0), available: vec![],
            in_structure: i(944), repairs_allowed: s(980), sleep_allowed: s(988),
            trade_buy: std::array::from_fn(|k| s(1056 + k * 2)), trade_sell: std::array::from_fn(|k| s(1106 + k * 2)),
            trade_buy_n: std::array::from_fn(|k| s(1156 + k * 2)), trade_sell_n: std::array::from_fn(|k| s(1206 + k * 2)),
            trading: std::array::from_fn(|k| s(1422 + k * 2)),
            peasants: s(892), horses: s(1278), troop_avail: std::array::from_fn(|k| u(2188 + k)), merc_avail: std::array::from_fn(|k| u(2600 + k)),
            bed_avail: std::array::from_fn(|k| u(2796 + k)), guild_avail: std::array::from_fn(|k| u(2817 + k)), ladderman_cost: u(2832),
            eunuch_cost: u(2833), siege: std::array::from_fn(|k| if k < 6 { u(1025 + k) } else { u(2609) }), patrol_mode: u(1037), get_ammo: u(1031), cows: u(1032), attack_here: u(1033), rocks: u(1034), weapon_avail: std::array::from_fn(|k| u(1047 + k)), costs: Table::default(), coords: Table::default(),
            keep_storage: std::array::from_fn(|k| i32::from_le_bytes(std::array::from_fn(|j| u(1436 + 4 * k + j)))),
            left: std::array::from_fn(|k| s2(1984 + 2 * k)), got_market: u(2587),
            game_paused: b.get(2200).copied().unwrap_or(0), report: reports::Data::decode(b), mouse_selector_state: u(2812), speed: 0.0, end: None, ev: Events::default(),
            gate_state: i(976), dog_cage: s(968), hp: [s(982), s(984), s(986)],
            inn: [i(1008), s(1012) as i32, s(1014) as i32, s(1016) as i32, s(1018) as i32, s(1020) as i32, s(1022) as i32],
            weapon: [b[1038], b[1043], b[1044]], can_make: [u(2597), b[1040], u(2599), b[1042], u(2598), b[1041]],
            marry: std::array::from_fn(|k| s(1256 + k * 2)), ai_clock: s2(2202),
            trade_sell_fixed: std::array::from_fn(|k| s(1372 + k * 2)),
            autotrade: [std::array::from_fn(|k| s2(2356 + k * 2)), std::array::from_fn(|k| s2(2304 + k * 2))],
            autotrade_on: std::array::from_fn(|k| u(2408 + k)),
            extreme: (u(2809), s2(2810)),
            cg_total: std::array::from_fn(|k| s2(2446 + 2 * k)),
            cg: std::array::from_fn(|g| std::array::from_fn(|j| (u(2466 + 4 * g + j), s2(2506 + 2 * (4 * g + j))))),
            lords: {
                let sh = |o: usize| -> [i16; 9] { std::array::from_fn(|p| if p == 0 { 0 } else { s2(o + 2 * (p - 1)) }) };
                Lords { ai: sh(2612), team: sh(2628), human: sh(2644), names: sh(2660), help: sh(2676),
                    req: std::array::from_fn(|k| [s2(2692 + 2 * k), s2(2712 + 2 * k)]), order: sh(2732), order_player: sh(2748),
                    order_from: sh(2764), ping: std::array::from_fn(|q| s2(1968 + 2 * q)), ..Default::default() }
            },
            hill: None,
        })
    }
}

/// A per-mapper table (costs, button coordinates); Debug prints just the length.
#[derive(Clone, Default, PartialEq)]
pub struct Table<T>(pub Vec<T>);
impl<T> std::fmt::Debug for Table<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "Table({})", self.0.len()) }
}

/// HUD_Main.getMargin: SetupNewBuildScreen places every build button of the screen from DLL_GetMapperCoord (x, y)
/// as Margin (x - 35, y - 488), a few with their own nudge; None = no coordinate (the XAML margin stays).
fn margin(mapper: i32, st: &HudState) -> Option<(f32, f32)> {
    let c = *st.coords.0.get(usize::try_from(mapper).ok()?)?;
    if c == 0x55555555 { return None; }
    let (x, y) = (((c >> 16) & 0xffff) as f32, (c & 0xffff) as i16 as f32);
    let (dx, dy) = match mapper {
        50 => (-55.0, -502.0), 82 => (-59.0, -501.0), 83 => (-65.0, -496.0), 85 => (-61.0, -501.0), 84 => (-55.0, -501.0),
        92 | 93 => (-35.0, -494.0), 279 => (-38.0, -488.0), 278 => (-41.0, -488.0), 276 => (-29.0, -488.0), 285 => (-26.0, -488.0),
        274 => (-39.0, -488.0), 283 => (-43.0, -492.0), _ => (-35.0, -488.0),
    };
    Some((x + dx, y + dy))
}

/// Build-tool ghost: tile diamonds (4 corners, logical px) each with its verdict, reason (text key or plain text).
pub struct Ghost { pub tiles: Vec<([[f32; 2]; 4], bool)>, pub why: Option<String> }

/// Menu buttons (HUD_Main.xaml Click handlers): the BuildIconLists row they open. NewBuildScreen*: Keeps 0, Towers 7,
/// Gates 8, Military 9, Bad 10, Good 11, small / large gatehouse 13 / 14; SubTowersRtn -> castle 1, SubTownRtn -> town
/// 4, SubGatesRtn -> gates 8.
pub(crate) fn submenu(name: &str) -> Option<usize> {
    Some(match name.strip_prefix("ButtonBuild")? {
        "SubKeeps" => 0,
        "SubTowers" => 7,
        "SubGates" => 8,
        "SubMilitaryBuildings" => 9,
        "SubBadStuff" => 10,
        "SubGoodStuff" => 11,
        "SmallGate" => 13,
        "LargeGate" => 14,
        "SubKeepsReturn" | "SubTowersReturn" | "SubGatesReturn" | "SubMilitaryBuildingsReturn" => 1,
        "SubBadStuffReturn" | "SubGoodStuffReturn" => 4,
        "SubSubSmallGatesReturn" | "SubSubLargeGatesReturn" => 8,
        _ => return None,
    })
}

/// Category tab that stays lit while a sub-menu row is shown.
fn tab_of(screen: usize) -> usize {
    match screen { 0 | 7..=9 | 13 | 14 => 1, 10 | 11 => 4, s => s }
}

/// Buttons sharing an XAML slot (well 330 / waterpot 342, mercenary post 86 / Bedouin stockade 79) are moved apart by
/// `margin`; without coordinates (replays) one of each pair by lord kind (Arab = 1, 2, 6, 7) shows.
fn variant_ok(mapper: i32, st: &HudState) -> bool {
    let arab = matches!(st.lord_type, 1 | 2 | 6 | 7);
    st.coords.0.is_empty().then(|| match mapper { 342 => arab, 330 => !arab, 86 => false, _ => true }).unwrap_or(true)
}

/// SetupNewBuildScreen's IsEnabled = CanPlaceMapper (`DLL_IsMapperAvailable`): a locked button stays on its screen,
/// drawn as BTN_Building's SpriteLocked (Sprite1 at BuildingButtonDisabledOpacity 0.5), without hover or click.
fn mapper_on(mapper: i32, st: &HudState) -> bool {
    mapper <= 0 || st.available.get(mapper as usize).copied().unwrap_or(true)
}

/// Opaque white 4×2 texels at (0, 0) of the font atlas for solid fills: `Ui::fill` samples uv (1, 1) / FONT_PX, the
/// corner of texels 0 and 1 in both axes, so bilinear filtering reads rows 0 and 1 (row 0 alone halved every fill's
/// alpha). Glyphs start at y 4.
fn white_block(rgba: &mut [u8]) {
    for r in 0..2 { let o = (r * FONT_PX * 4) as usize; rgba[o..o + 16].fill(255); }
}

/// (texture: 0 = UI atlas, 1 = font atlas, triangle) in logical window px.
pub type HudTri = (u32, [Vert; 3]);
pub const FONT_PX: u32 = 1024;
const ATLAS: f32 = 8192.0;
const CREAM: [f32; 4] = [0xef as f32 / 255.0, 0xf3 as f32 / 255.0, 0xc6 as f32 / 255.0, 1.0]; // #EFF3C6
const GREEN: [f32; 4] = [0.0, 136.0 / 255.0, 0.0, 1.0]; // BookColour_Green
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];             // BookColour_Red
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];           // HUD_Buildings TextBlock style
const ACCENT7: [f32; 3] = [0xf4 as f32 / 255.0, 0xde as f32 / 255.0, 0xaa as f32 / 255.0]; // Brush.Accent7, a GlowS button's text
const PRESSED: [f32; 4] = [0.8, 0.8, 0.8, 1.0]; // a held button's bar (ours: IsPressed only dims the text)

pub(crate) struct Button { pub(crate) screen: usize, pub(crate) name: String, pub(crate) structure: String, pub(crate) s1: String, pub(crate) s2: String, pub(crate) x: f32, pub(crate) y: f32, pub(crate) w: f32, pub(crate) h: f32, pub(crate) help: String, pub(crate) mapper: i32 }
struct Glyph { uv: [f32; 4], m: fontdue::Metrics }
#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit { Tab(usize), Build(usize), Stance(usize), Order(usize), Panel, Act(i32, i32, i32), Delete(i32), TradeType(i32), TradeGood(i32, i32), Trade(i32, i32), Radar, Engi(bool), Recruit(i32, Option<&'static str>), Goals, Options, Mode(i32), Help(i32),
    Workshop(i32, i32), Slider(u8, i32), Auto(u8, i32, i32), Confirm(u8), Briefing,
    Ally(u8, i32), Fb(i32), Cg(u8, i32) } // overlays.rs: allies panel, Free Build menu, control groups

/// The trading panel's auto-trade view (HUD_Buildings TradePost_Trade_Auto): on / off, the buy and sell slider
/// positions (indices into SliderCurve1000) and the per-good settings it was opened from.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AutoTrade { on: bool, buy: usize, sell: usize }

/// HUD_Scenario.SliderCurve1000: the auto-trade sliders' 101 stops.
const CURVE: [i32; 101] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 24, 26, 28, 30, 32, 34, 36,
    38, 40, 42, 44, 46, 48, 50, 54, 58, 62, 66, 70, 74, 78, 82, 86, 90, 94, 98, 100, 105, 110, 120, 130, 140, 150, 160, 170, 180, 190,
    200, 220, 240, 260, 280, 300, 320, 340, 360, 380, 400, 420, 440, 460, 480, 500, 520, 540, 560, 580, 600, 620, 640, 660, 680, 700,
    720, 740, 760, 780, 800, 820, 840, 860, 880, 900, 920, 940, 960, 980, 1000];

/// HUD_Buildings.IsWorkerBuilding (the ZZZ button) by in_structure_type.
const SLEEPABLE: [u8; 110] = [0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 0, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// The report pages (and the in-chimp panel 70) show none of the building buttons (GetBuildingSleepable /
/// ShowRepair / ShowWorkers return false for them).
fn report_panel(sub: i32) -> bool { matches!(sub, 66..=79) }

/// HUD_Buildings.GetPartnerImage: a wedding partner's chimp type -> GameSprites index (151.. = MarriagePartners 000..).
fn partner(t: i16) -> String {
    let n = match t { 3 => 151, 4 => 152, 5 => 153, 6 => 157, 7 | 8 => 158, 10 => 159, 13 | 14 => 160, 12 => 174, 11 => 173, 15 => 161,
        16 => 162, 18 => 163, 19 => 164, 20 => 165, 31 | 32 => 166, 33 => 167, 34 => 168, 35 => 169, 36 => 170, 42 => 171, 57 => 172,
        17 => 154, 21 => 155, 63 => 156, _ => 174 };
    format!("MarriagePartners {:03}", n - 151)
}

/// HUD_Buildings.xaml recruit ToggleButtons: (eChimps type, left, bottom margin, height, width (0 = sprite aspect),
/// sprite `UI-Buttons O{n}`: n normal, n + 1 hover, n + 2 locked (drawn at ImageButtonDisabledOpacity 0.5)).
type Recruit = (i32, f32, f32, f32, f32, u32);
const BARRACKS: [Recruit; 7] = [(22, 10.0, 47.0, 137.0, 0.0, 1), (24, 96.0, 47.0, 110.0, 0.0, 4), (26, 176.0, 47.0, 120.0, 0.0, 7),
    (23, 249.0, 46.0, 104.0, 0.0, 10), (25, 340.0, 47.0, 125.0, 0.0, 13), (27, 402.0, 47.0, 107.0, 72.0, 16), (28, 477.0, 47.0, 125.0, 0.0, 19)];
const MERCS: [Recruit; 7] = [(70, 18.0, 39.0, 125.0, 0.0, 31), (71, 86.0, 37.0, 120.0, 72.0, 34), (72, 171.0, 37.0, 115.0, 72.0, 37),
    (73, 246.0, 37.0, 120.0, 72.0, 40), (74, 324.0, 37.0, 120.0, 72.0, 43), (75, 402.0, 37.0, 120.0, 72.0, 46), (76, 483.0, 37.0, 110.0, 72.0, 49)];
const BEDOUIN: [Recruit; 8] = [(78, 146.0, 47.0, 108.0, 0.0, 58), (79, 429.0, 47.0, 105.0, 0.0, 61), (80, 351.0, 47.0, 115.0, 0.0, 64),
    (81, 503.0, 44.0, 105.0, 0.0, 67), (82, 8.0, 47.0, 110.0, 0.0, 70), (83, 216.0, 41.0, 120.0, 0.0, 73), (84, 81.0, 47.0, 102.0, 0.0, 76),
    (85, 288.0, 47.0, 110.0, 0.0, 79)];
const GUILD: [Recruit; 2] = [(30, 300.0, 20.0, 94.0, 39.0, 22), (29, 362.0, 18.0, 107.0, 53.0, 25)];
const TUNNELLER: [Recruit; 1] = [(5, 300.0, 20.0, 103.0, 47.0, 28)];
const MONK: [Recruit; 1] = [(37, 310.0, 20.0, 113.0, 0.0, 55)];

/// FatControler's most of troop `t` one recruit click can make (lastTroopsAmountToMakeMax): free peasants (the heavy
/// camel takes two), the gold, then the weapon / armour / horse stock it needs.
fn recruit_max(t: i32, st: &HudState) -> i32 {
    let mut n = 1000.min(if t == 83 { st.peasants as i32 / 2 } else { st.peasants as i32 });
    let c = gold_cost(t, st);
    if c > 0 { n = n.min(st.gold / c); }
    if n > 1 {
        let r = |k: usize| st.resources[k];
        let need = match t { 22 => vec![r(17)], 24 => vec![r(19)], 26 => vec![r(21), r(23)], 23 => vec![r(18), r(23)], 25 => vec![r(20), r(24)],
            27 => vec![r(22), r(24)], 28 => vec![r(22), r(24), st.horses as i32], _ => vec![] };
        n = need.into_iter().fold(n, i32::min);
    }
    n
}

/// GameData.getChimpGoldCost (single player).
fn gold_cost(t: i32, st: &HudState) -> i32 {
    match t {
        5 | 30 => 30, 29 => st.ladderman_cost as i32, 37 => 10, 22 | 72 => 12, 23 | 25 | 26 => 20, 24 => 8, 27 | 28 | 78 => 40,
        70 => 75, 71 => 5, 73 => 60, 74 | 75 | 85 => 80, 76 | 79 | 83 => 100, 77 | 84 => 50, 80 => st.eunuch_cost as i32,
        81 => 130, 82 => 25, _ => 0,
    }
}

/// FatControler's recruit button: None = hidden (Show_* = the panel's availability byte), else IsEnabled: a free
/// peasant (two for the heavy camel), the gold, the weapons / armour / horse in the armoury.
#[cfg(test)]
fn recruit_state(t: i32, st: &HudState) -> Option<bool> { recruit_why(t, st).map(|w| w.is_none()) }

/// Why a shown recruit button is locked (None inside = it isn't), as the text the viewer shows on a click:
/// "Peasants needed, sire", "Gold needed", "You have no weapons to equip your troops".
fn recruit_why(t: i32, st: &HudState) -> Option<Option<&'static str>> {
    let shown = match t {
        22..=28 => st.troop_avail[(t - 22) as usize] > 0,
        70..=76 => st.merc_avail[(t - 70) as usize] > 0,
        78..=85 => st.bed_avail[(t - 78) as usize] > 0,
        30 => st.guild_avail[1] > 0,
        29 => st.guild_avail[2] > 0,
        37 => st.guild_avail[0] > 0, // monk_available
        _ => true,
    };
    let r = &st.resources;
    let kit = match t {
        22 => r[17] > 0, 24 => r[19] > 0, 26 => r[21] > 0 && r[23] > 0, 23 => r[18] > 0 && r[23] > 0, 25 => r[20] > 0 && r[24] > 0,
        27 => r[22] > 0 && r[24] > 0, 28 => r[22] > 0 && r[24] > 0 && st.horses > 0, _ => true,
    };
    let why = if st.peasants <= (t == 83) as i16 { Some("TEXT_FEEDBACK_020") } else if gold_cost(t, st) > r[15] { Some("TEXT_FEEDBACK_004") }
        else if !kit { Some("TEXT_IN_BARRACKS_002") } else { None };
    shown.then_some(why)
}

/// eStructs kind → eMappers (MainViewModel.Setup_eStructToMapperDictionary) for the kinds GameMap's case 25 picks:
/// GameMap's own exclusions (29, 39-44, 50, 52, 54, 55, 71-73, 79-89) and remapped gates (45 / 46 / 47) left out.
const PICK: [(i32, i32); 78] = [(1, 54), (2, 53), (3, 51), (4, 55), (5, 90), (6, 91), (7, 78), (8, 86), (9, 87), (10, 52), (11, 81),
    (12, 50), (13, 83), (14, 82), (15, 84), (16, 85), (17, 75), (18, 76), (19, 80), (20, 56), (22, 92), (23, 93), (24, 88), (25, 89),
    (26, 77), (27, 330), (28, 180), (30, 70), (31, 71), (32, 72), (33, 73), (34, 74), (35, 65), (36, 95), (37, 96), (38, 97),
    (48, 104), (49, 105), (58, 311), (59, 311), (60, 100), (61, 28), (62, 176), (63, 177), (64, 311), (65, 175), (66, 160),
    (67, 98), (68, 99), (69, 192), (70, 342), (74, 110), (75, 111), (76, 112), (77, 113), (78, 114), (91, 301), (92, 305),
    (93, 306), (94, 307), (95, 308), (96, 309), (97, 310), (98, 311), (99, 312), (100, 313), (101, 318), (102, 323), (103, 324),
    (104, 325), (105, 329), (106, 178), (107, 179), (108, 79), (109, 195), (127, 140), (131, 144), (133, 146)];

const GREY: [f32; 4] = [0xdd as f32 / 255.0, 0xdd as f32 / 255.0, 0xdd as f32 / 255.0, 1.0]; // #DDDDDD

pub struct Ui {
    pub(crate) rects: HashMap<String, [f32; 4]>, // x, y, w, h (TexturePacker, top-left origin)
    strings: HashMap<String, String>,
    goods: Vec<(usize, f32, f32, String)>, // resource index, left, bottom, icon
    pub(crate) build: Vec<Button>,
    pub(crate) tabs: Vec<([String; 4], f32)>,
    troops: HashMap<i32, (String, String)>, // eChimps type -> picture button sprites (HUD_Troops.xaml)
    titles: Vec<String>,                     // in_structure_type -> building panel title key
    font: fontdue::Font,
    glyphs: HashMap<(char, u32), Glyph>,
    pub font_rgba: Vec<u8>, // FONT_PX² RGBA; texel (0,0) is opaque white for solid quads
    pen: (u32, u32, u32),
    pub font_dirty: bool,
    hits: Vec<([f32; 4], Hit)>, // last frame's clickable rects (logical px)
    pub screen: usize,         // BuildIconLists row: 1 castle .. 6 food
    pub selected: Option<usize>,
    pub radar: bool, // a minimap texture (HUD texture 2) has been uploaded: draw it
    pub actions: Vec<[i32; 3]>, // DLL_GameAction(action, s, v) from panel buttons, drained by the caller
    pub actions4: Vec<[i32; 4]>, // the same with a third argument (the allies panel's orders and goods)
    pub drag: Option<([f32; 4], u8, i32, f32)>, // a slider held down: its box, Hit::Slider (k, id), the cursor x last applied
    allies: Option<overlays::Allies>, // the allies panel is open (AlliesPanelVisible)
    fb: Option<overlays::FreeBuild>,  // the Free Build menu is open (Show_HUD_FreebuildMenu)
    cg: bool,                         // the control groups list is up (Show_HUD_ControlGroups)
    lords: Lords,                     // the last frame's lord table (for the allies / Free Build clicks)
    pub delete: Option<i32>, // building id the panel's Delete button asked to remove, drained by the caller
    pub radar_click: Option<[i32; 2]>, // minimap click at radar texture px (top-down), drained by the caller
    radar_at: [f32; 4],                 // where the minimap was drawn last (screen px)
    pub message: Option<String>, // one-shot feedback shown at the cursor (e.g. a button the sim can't run yet)
    pub sub_mode: Option<i32>,   // EditorDirector.directSetAppSubMode request (main.rs sends DLL_SetAppMode(16, it))
    pub shift: bool,             // Shift held (main.rs): market Buy / Sell trade 5 lots
    pub open_menu: bool,         // SideTabs' options button (HUD_Main.InGameOptions): main.rs opens the in-game menu
    pub help: Option<String>,    // a building panel's help button: the page main.rs opens (menu/help.rs)
    pub ctrl: bool,              // Ctrl held (main.rs): recruit as many as possible
    stance: Option<usize>, // troops panel: last clicked stance / order button (display only, no chore sent)
    engi: bool,            // troops panel: the engineers' siege build buttons are up (HUD_Troops.SelectedEngiBuild)
    order: Option<usize>,
    ost: ost::OstUi, // OnScreenText's state (ost.rs)
    bar: Option<(String, std::time::Instant)>, // OST_MESSAGE_BAR: text, when posted (shown 10 s)
    seen: (u32, u32),                          // serials of the last message / pick event taken from `Events`
    goals: bool,                               // the objectives popup is open (Show_HUD_Objectives)
    pub edge_mask: f32, // MapLowerEdgeMaskHeight (design px, 0 = hidden): main.rs's camera limit sets it
    pub rclick: Option<RightClick>, // the right-click menu while the right button is held (HUD_RightClick)
    pub hidden: bool,               // the in-game UI is hidden (MainControls.IsUIVisible false; right-click menu left)
    pub flat: bool,                 // flattened landscape (EngineInterface.flattenedLandscape; right-click menu down)
    pub open_briefing: bool,        // SideTabs' briefing button (ButtonGotoBriefing): main.rs opens the mission's briefing
    confirm: Option<bool>,          // the Disband confirmation popup is up; its "Do not ask again" box ticked
    auto: Option<AutoTrade>,        // the trading panel shows its auto-trade view
    auto_goods: ([[i16; 25]; 2], [u8; 25]), // autotrade buy / sell amounts and on / off per good, from the last frame
    prev_trade: i32,                // HUD.previousTradePanel: the category the trading panel's back arrow returns to
    trade_err: Option<String>,      // TradeErrorText after a refused Buy / Sell (None = the Shift hint)
    seen_ret: u32,                  // serial of the last ACTION_RET taken
    last_panel: (i32, i32),         // (app_mode, app_sub_mode) drawn last: a panel change resets its view state
    panel_tip: Option<String>,      // a building panel button's hover text, shown as the tooltip
    pub down: bool,                 // the left mouse button is held (main.rs): the button under it looks pressed
    pub(crate) s: f32, pub(crate) w: f32, pub(crate) h: f32,    // Viewbox scale, window logical size
}

impl Ui {
    /// None when the export (re/scripts/export_ui_atlas.py) is missing: the viewer then runs without HUD.
    pub fn load(dir: &std::path::Path) -> Option<Self> {
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
        let tsv = |f: &str| -> Option<Vec<Vec<String>>> { Some(read(f)?.lines().map(|l| l.split('\t').map(String::from).collect()).collect()) };
        let num = |s: &str| s.parse::<f32>().unwrap();
        let rects = tsv("rects.tsv")?.into_iter().map(|c| (c[0].clone(), [num(&c[1]), num(&c[2]), num(&c[3]), num(&c[4])])).collect();
        let strings = tsv("strings.tsv")?.into_iter().filter(|c| c.len() > 1).map(|c| (c[0].clone(), c[1].clone())).collect();
        let goods = tsv("goods.tsv")?.into_iter().map(|c| (c[0].parse().unwrap(), num(&c[1]), num(&c[2]), c[3].clone())).collect();
        let build = tsv("build.tsv")?.into_iter().map(|c| Button {
            screen: c[0].parse().unwrap(), name: c[2].clone(), structure: c[3].clone(), s1: c[4].clone(), s2: c[5].clone(),
            x: num(&c[6]), y: num(&c[7]), w: num(&c[8]), h: num(&c[9]), help: c[10].clone(),
            mapper: c.get(11).and_then(|v| v.parse().ok()).unwrap_or(0), // old exports: no column -> no placement
        }).chain(std::iter::once(Button { // SideTabs' GameDeleteButton: ButtonBuildCommand STRUCT_NEW_DELETE = the delete tool
            screen: 0, name: "GameDeleteButton".into(), structure: String::new(), s1: "UI-Buttons A041".into(), s2: "UI-Buttons A042".into(),
            x: 0.0, y: 0.0, w: 20.0, h: 21.0, help: "TEXT_BUBBLE_HELP_TEXT_146".into(), mapper: 0x27,
        })).collect();
        let tabs = tsv("tabs.tsv")?.into_iter().map(|c| ([c[1].clone(), c[2].clone(), c[3].clone(), c[4].clone()], num(&c[5]))).collect();
        let troops = tsv("troops.tsv")?.into_iter().map(|c| (c[0].parse().unwrap(), (c[3].clone(), c[4].clone()))).collect();
        let titles = tsv("titles.tsv")?.into_iter().map(|c| c[1].clone()).collect();
        let font = fontdue::Font::from_bytes(std::fs::read(dir.join("font.ttf")).ok()?, Default::default()).ok()?;
        let mut font_rgba = vec![0u8; (FONT_PX * FONT_PX * 4) as usize];
        white_block(&mut font_rgba);
        Some(Self { rects, strings, goods, build, tabs, troops, titles, font, glyphs: HashMap::new(), font_rgba, pen: (4, 0, 0), font_dirty: true,
            hits: vec![], screen: 1, selected: None, radar: false, actions: vec![], actions4: vec![], drag: None, allies: None, fb: None, cg: false, lords: Lords::default(), delete: None, radar_click: None, radar_at: [0.0; 4], message: None, sub_mode: None, shift: false, open_menu: false, help: None, ctrl: false, stance: None, engi: false, order: None, ost: Default::default(), bar: None, seen: (0, 0), goals: false, edge_mask: 0.0, rclick: None, hidden: false, flat: false,
            open_briefing: false, confirm: None, auto: None, auto_goods: ([[0; 25]; 2], [0; 25]), prev_trade: 0, trade_err: None, seen_ret: 0, last_panel: (0, 0), panel_tip: None, down: false, s: 1.0, w: 0.0, h: 0.0 })
    }

    pub(crate) fn text_of(&self, key: &str) -> String {
        let s = self.strings.get(key).map_or(key, |s| s.as_str());
        // strings.tsv keeps CSV quoting: """No taxes""" is "No taxes"
        match s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) { Some(q) => q.replace("\"\"", "\""), None => s.to_string() }
    }

    // ---- layout anchors: design px (1920×1080, Viewbox scale) -> logical window px
    fn br(&self, x: f32, y: f32) -> [f32; 2] { [self.w - (1920.0 - x) * self.s, self.h - (1080.0 - y) * self.s] }
    fn bl(&self, x: f32, y: f32) -> [f32; 2] { [x * self.s, self.h - (1080.0 - y) * self.s] }

    fn quad(out: &mut Vec<HudTri>, tex: u32, p: [f32; 4], uv: [f32; 4], color: [f32; 4], skew: f32) {
        // p = x0, y0, x1, y1 (screen); uv = u0, v0 (top), u1, v1 (bottom); skew shifts the top edge right
        let v = |x: f32, y: f32, u: f32, w: f32| Vert { pos: [x + (p[3] - y) * skew, y], uv: [u, w], color, team: [0.0; 4] };
        let (a, b, c, d) = (v(p[0], p[1], uv[0], uv[1]), v(p[2], p[1], uv[2], uv[1]), v(p[0], p[3], uv[0], uv[3]), v(p[2], p[3], uv[2], uv[3]));
        out.push((tex, [a, b, c]));
        out.push((tex, [b, d, c]));
    }

    /// Atlas sprite `key` stretched to screen rect `p` (x0, y0, x1, y1). The texture is stored bottom row first.
    pub(crate) fn img(&self, out: &mut Vec<HudTri>, key: &str, p: [f32; 4], color: [f32; 4]) {
        let Some(&[x, y, w, h]) = self.rects.get(key) else { return };
        Self::quad(out, 0, p, [x / ATLAS, (ATLAS - y) / ATLAS, (x + w) / ATLAS, (ATLAS - y - h) / ATLAS], color, 0.0);
    }

    /// WPF `Image` (Stretch=Uniform) inside box `p`: fit, keep aspect, centre.
    pub(crate) fn img_fit(&self, out: &mut Vec<HudTri>, key: &str, p: [f32; 4], color: [f32; 4]) {
        let Some(&[_, _, w, h]) = self.rects.get(key) else { return };
        let k = ((p[2] - p[0]) / w).min((p[3] - p[1]) / h);
        let (cx, cy) = ((p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0);
        self.img(out, key, [cx - w * k / 2.0, cy - h * k / 2.0, cx + w * k / 2.0, cy + h * k / 2.0], color);
    }

    pub(crate) fn fill(out: &mut Vec<HudTri>, p: [f32; 4], color: [f32; 4]) {
        let t = 1.0 / FONT_PX as f32; // centre of the white texel block
        Self::quad(out, 1, p, [t, t, t, t], color, 0.0);
    }

    fn glyph(&mut self, c: char, px: u32) -> &Glyph {
        if !self.glyphs.contains_key(&(c, px)) {
            let (m, cov) = self.font.rasterize(c, px as f32);
            let (gw, gh) = (m.width as u32, m.height as u32);
            if self.pen.0 + gw + 2 > FONT_PX { self.pen = (0, self.pen.1 + self.pen.2 + 2, 0); }
            let (x0, y0) = (self.pen.0, self.pen.1 + 4);
            if y0 + gh < FONT_PX { // ponytail: atlas full = glyph not drawn; 1024² holds thousands at HUD sizes
                for (i, a) in cov.iter().enumerate() {
                    let o = (((y0 + i as u32 / gw.max(1)) * FONT_PX + x0 + i as u32 % gw.max(1)) * 4) as usize;
                    self.font_rgba[o..o + 4].copy_from_slice(&[255, 255, 255, *a]);
                }
                self.font_dirty = true;
            }
            let f = FONT_PX as f32;
            let uv = [x0 as f32 / f, y0 as f32 / f, (x0 + gw) as f32 / f, (y0 + gh) as f32 / f];
            self.pen = (self.pen.0 + gw + 2, self.pen.1, self.pen.2.max(gh));
            self.glyphs.insert((c, px), Glyph { uv, m });
        }
        &self.glyphs[&(c, px)]
    }

    /// Text with its baseline at `y`. `size` in design px, `sx` horizontal scale, `skew` = tan(-SkewTransform.AngleX).
    /// `align`: 0 left of x, 0.5 centred, 1 right of x. Returns the width.
    pub(crate) fn text(&mut self, out: &mut Vec<HudTri>, s: &str, x: f32, y: f32, size: f32, sx: f32, skew: f32, align: f32, color: [f32; 4]) -> f32 {
        let px = (size * self.s * crate::ux::dpr()).round().max(4.0) as u32; // rasterised at the screen's device px (2 on Retina)
        let k = size * self.s / px as f32;
        let width: f32 = s.chars().map(|c| self.glyph(c, px).m.advance_width * k * sx).sum();
        let mut pen = x - width * align;
        for c in s.chars() {
            let g = self.glyph(c, px);
            let (m, uv) = (g.m, g.uv);
            let x0 = pen + m.xmin as f32 * k * sx;
            let y1 = y - m.ymin as f32 * k;
            let p = [x0, y1 - m.height as f32 * k, x0 + m.width as f32 * k * sx, y1];
            // shear around the baseline: shift the whole glyph by its bottom's height above the baseline
            let base = (y - y1) * skew;
            if m.width > 0 { Self::quad(out, 1, [p[0] + base, p[1], p[2] + base, p[3]], uv, color, skew); }
            pen += m.advance_width * k * sx;
        }
        width
    }

    /// Text with the HUD's 2 px drop shadow (DropShadowEffect ShadowDepth=2, Direction=315, #CC000000).
    pub(crate) fn text_sh(&mut self, out: &mut Vec<HudTri>, s: &str, x: f32, y: f32, size: f32, align: f32, color: [f32; 4]) -> f32 {
        let d = 2.0 * self.s * 0.7071;
        self.text(out, s, x + d, y + d, size, 1.0, 0.0, align, [0.0, 0.0, 0.0, 0.8]);
        self.text(out, s, x, y, size, 1.0, 0.0, align, color)
    }

    /// eMappers id of the highlighted build button (0 / None = nothing to place).
    pub fn selected_mapper(&self) -> Option<i32> { self.selected.map(|i| self.build[i].mapper).filter(|&m| m > 0) }

    /// A build hotkey (keys.rs PLACE, EditorDirector.placeBuildingInteraction): mapper `m`'s button picked, its tab shown.
    pub fn pick_mapper(&mut self, m: i32) {
        if let Some(i) = self.build.iter().position(|b| b.mapper == m) { (self.screen, self.selected) = (self.build[i].screen, Some(i)); }
    }

    /// The cursor is over a HUD element (last frame's layout).
    pub fn over(&self, c: [f32; 2]) -> bool { self.hits.iter().any(|(p, _)| Self::inside(*p, c)) }

    fn inside(p: [f32; 4], c: [f32; 2]) -> bool { c[0] >= p[0] && c[0] < p[2] && c[1] >= p[1] && c[1] < p[3] }

    /// Right press at `c`: a troop type in the troops panel leaves the selection (RightClickSelectedTroopCommand:
    /// Troops_DeSelectType 1021). True if the HUD took it.
    pub fn rclick(&mut self, c: [f32; 2]) -> bool {
        let Some(&(_, Hit::Act(1022, _, t))) = self.hits.iter().rev().find(|(p, _)| Self::inside(*p, c)) else { return false };
        self.actions.push([1021, 0, t]);
        true
    }

    /// Left click at `c` (logical px): true if the HUD took it (tabs switch category, build buttons highlight).
    pub fn click(&mut self, c: [f32; 2]) -> bool {
        self.message = None;
        let Some(&(p, hit)) = self.hits.iter().rev().find(|(p, _)| Self::inside(*p, c)) else { return false };
        let frac = ((c[0] - p[0]) / (p[2] - p[0]).max(1.0)).clamp(0.0, 1.0); // where a slider was clicked
        match hit {
            // MainViewModel.ButtonUnitDisband: with Options > Confirm Disband Troops the confirmation popup first
            Hit::Order(0) if crate::skirmish::Settings::load().flag("Confirm_Disband_Troops", true) => { self.order = Some(0); self.confirm = Some(false); }
            Hit::Confirm(k) => match k {
                2 => { // ConfirmCheck: "Do not ask again" -> Settings_Confirm_Disband_Troops = !checked, saved now
                    let on = !self.confirm.unwrap_or(false);
                    self.confirm = Some(on);
                    let mut cfg = crate::skirmish::Settings::load();
                    cfg.set("Confirm_Disband_Troops", if on { "False" } else { "True" });
                    if let Err(e) = cfg.save() { self.message = Some(format!("settings.cfg: {e}")); }
                }
                k => { if k == 1 { self.actions.push([1011, 0, 0]); } self.confirm = None; } // Yes: Troops_Disband
            },
            Hit::Briefing => self.open_briefing = true,
            // ButtonChangeWorkshopOutput: SetNextWeaponMade (1023) for this workshop, every one of its kind with Shift
            Hit::Workshop(id, g) => self.actions.push([1023, if self.shift { -1 } else { id }, g]),
            Hit::Slider(k, id) => { self.drag = Some((p, k, id, c[0])); self.slider(k, id, frac) }
            Hit::Ally(k, v) => self.ally_click(k, v),
            Hit::Fb(v) => self.fb_click(v),
            Hit::Cg(op, g) => self.cg_click(op, g),
            Hit::Auto(k, id, g) => self.auto_click(k, id, g),
            Hit::Tab(t) => {
                if self.screen != t + 1 { crate::audio::ui(crate::audio::ui_id::TAB_CLICK[t], 1.0); crate::audio::ui(crate::audio::ui_id::SWISH, 1.0); }
                self.screen = t + 1;
                self.selected = None;
            }
            Hit::Build(b) => match submenu(&self.build[b].name) {
                Some(s) => { if self.screen != s { crate::audio::ui(crate::audio::ui_id::SWISH, 1.0); } self.screen = s; self.selected = None; }
                None => self.selected = Some(b),
            },
            // Troops_ChangeStance (1020, v = 287..289); Troops_Disband 1011 / Stop 1010 / Patrol 1019
            Hit::Stance(k) => { self.stance = Some(k); self.actions.push([1020, -1, 287 + k as i32]); }
            Hit::Order(k) => { self.order = Some(k); self.actions.push([[1011, 1010, 1019][k], 0, 0]); }
            Hit::Panel => {}
            Hit::Act(a, s, v) => self.actions.push([a, s, v]),
            // MainViewModel.ButtonNewTradeType: 0 = back to the post (25) from a category / the price list, 1..3 =
            // food / resources / weapons (54..56), 4 = the price list (53)
            Hit::TradeType(n) => {
                if n > 0 { self.prev_trade = n; }
                self.sub_mode = Some(match n { 0 => 25, 4 => 53, n => 53 + n });
            }
            // ButtonNewTradeGoodsType: the trading panel (57) for that good (SetCurrentTradedGood 1027)
            Hit::TradeGood(id, g) => { self.sub_mode = Some(57); self.actions.push([1027, id, g]); }
            // ButtonBuySell 1 / 2: BuyGoods 1024 / SellGoods 1025, one lot (Shift: five), v = trading_current_goods
            Hit::Trade(a, g) => self.actions.push([a, self.shift as i32, g]),
            Hit::Delete(id) => self.delete = Some(id),
            // RadarClicked (1028): the radar pixel; the sim thread turns it into the camera tile
            Hit::Engi(on) => self.engi = on,
            Hit::Goals => self.goals = !self.goals, // ButtonExtendedFeaturesFunction "Objectives"
            Hit::Options => self.open_menu = true,
            Hit::Help(t) => self.help = crate::menu::help::building_page(t), // HUD_Help.OpenHelpForCurrentBuildingOrChimmp
            Hit::Mode(s) => self.mode_click(s), // reports book: DLL_SetAppMode(16, s) (reports.rs)

            // MakeTroop (1007, s = 1): sent even when locked (the engine refuses it, as the game's overlay Button does);
            // a locked one says why at the cursor
            Hit::Recruit(t, why) => {
                self.actions.push([1007, if self.shift { 5 } else if self.ctrl { 1000 } else { 1 }, t]); // MainViewModel: Shift 5, Ctrl 1000
                self.message = why.map(|k| self.text_of(k));
            }
            Hit::Radar => {
                let r = self.radar_at;
                let n = crate::live::RADAR_N as f32;
                self.radar_click = Some([((c[0] - r[0]) / (r[2] - r[0]) * n) as i32, ((c[1] - r[1]) / (r[3] - r[1]) * n) as i32]);
            }
        }
        true
    }

    // ---- panel helpers: `o` = panel top-left in design px (panels are bottom-right anchored), x/y/w/h panel-local
    fn pr(&self, o: [f32; 2], x: f32, y: f32, w: f32, h: f32) -> [f32; 4] {
        let [x0, y0] = self.br(o[0] + x, o[1] + y);
        [x0, y0, x0 + w * self.s, y0 + h * self.s]
    }

    /// Text width in design px.
    pub(crate) fn tw(&mut self, s: &str, size: f32) -> f32 { self.text(&mut vec![], s, 0.0, 0.0, size, 1.0, 0.0, 0.0, [1.0; 4]) / self.s }

    /// TextBlock with RenderTransform = SkewTransform `skew_deg` (AngleX) then ScaleTransform `sc`, both about `o`
    /// (RenderTransformOrigin, design px): the untransformed text starts at x `left`, its line box top at `top`.
    fn text_xf(&mut self, out: &mut Vec<HudTri>, s: &str, left: f32, top: f32, size: f32, o: [f32; 2], skew_deg: f32, sc: [f32; 2], color: [f32; 4]) {
        let asc = self.font.horizontal_line_metrics(size).map_or(size * 0.8, |m| m.ascent);
        let k = skew_deg.to_radians().tan();
        let base = top + asc - o[1];
        let [x, y] = self.br(o[0] + sc[0] * (left - o[0] + k * base), o[1] + sc[1] * base);
        self.text(out, s, x, y, size * sc[1], sc[0] / sc[1], -k * sc[0] / sc[1], 0.0, color);
    }

    /// TextBlock whose box top is `top` (panel-local): baseline = top + font ascent. `align` as in `text`.
    fn label(&mut self, out: &mut Vec<HudTri>, s: &str, o: [f32; 2], x: f32, top: f32, size: f32, align: f32, color: [f32; 4]) {
        let asc = self.font.horizontal_line_metrics(size).map_or(size * 0.8, |m| m.ascent);
        let [x, y] = self.br(o[0] + x, o[1] + top + asc);
        self.text(out, s, x, y, size, 1.0, 0.0, align, color);
    }

    /// BTN_Image: sprite 1, sprite 2 when hovered or `on`.
    fn button(&mut self, out: &mut Vec<HudTri>, p: [f32; 4], s1: &str, s2: &str, on: bool, cursor: [f32; 2], hit: Option<Hit>) {
        let hl = on || hit.is_some() && Self::inside(p, cursor);
        self.img_fit(out, if hl { s2 } else { s1 }, p, [1.0; 4]);
        if let Some(h) = hit { self.hits.push((p, h)); }
    }

    /// The panel's help button where HUD_Buildings.GetBuildingShowHelp has one (not on panels 53, 66-79; the game's
    /// page for the kind exists); not on the dog cage (89) once its dogs are out (dog_cage_state != 0).
    fn help_button(&mut self, st: &HudState, out: &mut Vec<HudTri>, p: [f32; 4], s: [&str; 2], cursor: [f32; 2]) {
        if matches!(st.app_sub_mode, 53 | 66..=79) || st.app_sub_mode == 89 && st.dog_cage != 0 || crate::menu::help::building_page(st.in_structure_type).is_none() { return; }
        self.button(out, p, s[0], s[1], false, cursor, Some(Hit::Help(st.in_structure_type)));
    }

    /// WGT_Popularity (60×40 at x, y): value right-aligned at x+36, face F007 (+) / F008 (0) / F009 (−) at the right.
    fn pop_widget(&mut self, out: &mut Vec<HudTri>, o: [f32; 2], x: f32, y: f32, value: i32) {
        self.label(out, &value.to_string(), o, x + 36.0, y + 23.0 - 12.0, 20.0, 1.0, BLACK);
        let face = match value { 1.. => "UISprites F007", 0 => "UISprites F008", _ => "UISprites F009" };
        self.img_fit(out, face, self.pr(o, x + 42.0, y + 10.5, 20.0, 23.0), [1.0; 4]);
    }

    /// HUD_Troops.xaml (800×155, bottom-right): frame, stance tabs, unit orders, up to 8 picture buttons with counts
    /// (HUD_Troops.SetupSelectedTroops: types ascending, 55 skipped, slot X = -181 + 52·slot, Y -13; page 1 only).
    fn troops_panel(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let o = [1920.0 - 800.0, 1080.0 - 155.0];
        let white = [1.0; 4];
        let pad = self.pr(o, -240.0, 155.0 - 136.0, 240.0, 136.0); // Leftpadding, TranslateTransform X -240
        self.img(out, "UI-HUD 008", pad, white);
        let bg = self.pr(o, 0.0, 0.0, 800.0, 155.0);
        self.img(out, "UI-HUD 006", bg, white);
        self.hits.extend([(pad, Hit::Panel), (bg, Hit::Panel)]);
        // ToggleControlGroups (519, 11, 35×35): HUD_ControlGroups.ToggleMenu
        self.button(out, self.pr(o, 519.0, 11.0, 35.0, 35.0), "Control_Group", "Control_Group_Highlight", self.cg, cursor, Some(Hit::Cg(9, 0)));
        // StanceTabs: StackPanel at (0, 5) of 46×65, 40×46 (+18), 43×50 (+14)
        for (k, (x, y, w, h, s1, s2)) in [(0.0, 5.0, 46.0, 65.0, "I001", "I002"), (46.0, 23.0, 40.0, 46.0, "I004", "I005"),
            (86.0, 19.0, 43.0, 50.0, "I007", "I008")].into_iter().enumerate() {
            let p = self.pr(o, x, y, w, h);
            self.button(out, p, &format!("UI-Buttons {s1}"), &format!("UI-Buttons {s2}"), self.stance == Some(k), cursor, Some(Hit::Stance(k)));
        }
        // UnitControls (130×88 grid, bottom-left): two rows of three slots (buttons centred in their margins) and a
        // Back arrow. Normal: Disband (troops_show_disband), Stop, Patrol / PatrolActive, Build siege kit
        // (troops_show_build_menu); after it (SelectedEngiBuild): Catapult, Trebuchet, Siege tower / Ram, Mantlet,
        // Arab ballista as troops_show_make_* allow, EngBuild_* 1014..1018 / 1060 (the next map click places the tent)
        const SLOT: [(f32, f32, f32); 6] = [(6.0, 4.0, 37.0), (47.0, 4.0, 37.0), (87.0, 4.0, 36.0), (6.0, 44.0, 37.0), (46.5, 44.0, 36.0), (87.0, 44.0, 37.0)];
        let gy = 155.0 - 88.0;
        let slot = |ui: &mut Self, out: &mut Vec<HudTri>, k: usize, j: (u32, u32), hit: Hit| {
            let (x, y, w) = SLOT[k];
            let p = ui.pr(o, x, gy + y, w, 37.0);
            ui.button(out, p, &format!("UI-Buttons J{:03}", j.0), &format!("UI-Buttons J{:03}", j.1), false, cursor, Some(hit));
        };
        if self.engi {
            for (k, j, a) in [(0, 31, 1014), (1, 34, 1015), (2, 37, 1016), (3, 40, 1018), (4, 43, 1017), (5, 46, 1060)] {
                if st.siege[[1, 2, 3, 4, 5, 6][k]] > 0 { slot(self, out, k, (j, j + 1), Hit::Act(a, 0, 0)); }
            }
            let p = self.pr(o, 91.0 + (89.0 - 15.0) / 2.0, gy + (85.0 - 17.0) / 2.0, 15.0, 17.0);
            self.button(out, p, "UI-Buttons J028", "UI-Buttons J029", false, cursor, Some(Hit::Engi(false)));
        } else {
            if st.troops_show_disband != 0 { slot(self, out, 0, (4, 5), Hit::Order(0)); }
            slot(self, out, 1, (10, 11), Hit::Order(1));
            slot(self, out, 2, if st.patrol_mode != 0 { (3, 2) } else { (1, 2) }, Hit::Order(2));
            if st.siege[0] > 0 { slot(self, out, 4, (13, 14), Hit::Engi(true)); }
            // ShownAttackHereOrder: slot 3 is pour oil (1), tunnel here (2) or attack here (the rocks left for 3), all
            // Troops_AttackHere 1012; ShowAmmoOrders: reload (AmmoRecharge 1041) in slot 4, launch cow (Troops_Cow 1013,
            // 1..249 cows, their count right-aligned) in slot 5
            let tl = |k: usize| (SLOT[k].0, gy + SLOT[k].1);
            match st.attack_here {
                0 => {}
                1 => slot(self, out, 3, (19, 20), Hit::Act(1012, 0, 0)),
                2 => slot(self, out, 3, (22, 23), Hit::Act(1012, 0, 0)),
                _ => {
                    slot(self, out, 3, (16, 17), Hit::Act(1012, 0, 0));
                    if st.attack_here == 3 {
                        let t = if st.rocks == 255 { "250+".to_string() } else { st.rocks.to_string() };
                        let (x, y) = tl(3);
                        self.label(out, &t, o, x + 4.0, y, 16.0, 0.0, BLACK);
                    }
                }
            }
            if st.get_ammo > 0 { slot(self, out, 4, (25, 26), Hit::Act(1041, 0, 0)); }
            if (1..250).contains(&st.cows) {
                slot(self, out, 5, (7, 8), Hit::Act(1013, 0, 0));
                let (x, y) = tl(5);
                self.label(out, &st.cows.to_string(), o, x + 35.5, y, 16.0, 1.0, BLACK);
            }
        }
        let shown = st.troops.iter().filter(|(t, _)| *t != 55 && *t < 89).take(8);
        for (slot, &(t, n)) in shown.enumerate() {
            let x = 52.0 * slot as f32;
            if let Some((s1, s2)) = self.troops.get(&t).cloned() {
                // TroopSelectionControls 408×120 at (142, bottom 2): 45×86 button centred, + slot transform
                // LeftClickSelectedTroopCommand: Troops_DeSelectExceptType (1022) keeps only this type; a right click
                // drops it (`rclick`)
                let p = self.pr(o, 142.0 + (408.0 - 45.0) / 2.0 - 181.0 + x, 33.0 + 17.0 - 13.0, 45.0, 86.0);
                self.button(out, p, &s1, &s2, false, cursor, Some(Hit::Act(1022, 0, t)));
            }
            // TroopSelectionNumbers 820×30 at (132, bottom 8): 64 px centred TextBlocks, 22 px #DDDDDD, drop shadow
            let asc = self.font.horizontal_line_metrics(22.0).map_or(17.6, |m| m.ascent);
            let [cx, cy] = self.br(o[0] + 132.0 + x + 32.0, o[1] + 155.0 - 8.0 - 30.0 + asc);
            self.text_sh(out, &n.to_string(), cx, cy, 22.0, 0.5, [0xdd as f32 / 255.0, 0xdd as f32 / 255.0, 0xdd as f32 / 255.0, 1.0]);
        }
    }

    /// HUD_Buildings.xaml: MainViewModel.setUpInbuilding picks the sub-panel by `app_sub_mode`. Barracks (1), merc post
    /// (44) and bedouin stockade (63) get their 800×195 frames; everything else the 800×170 `UI-HUD 003` frame with title
    /// and the workers line, plus the Keep (2) and Granary (4) contents.
    fn building_panel(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        if (0x42..=0x4f).contains(&st.app_sub_mode) { return self.reports(st, out, cursor); } // report pages (reports.rs)
        let white = [1.0; 4];
        let troop_frame = match st.app_sub_mode { 1 => Some(("UI-HUD 005", &BARRACKS[..])), 44 => Some(("UI-HUD 009", &MERCS[..])),
            63 => Some(("UI-HUD 010", &BEDOUIN[..])), _ => None };
        if let Some((f, table)) = troop_frame {
            let o = [1120.0, 1080.0 - 195.0];
            let p = self.pr(o, 0.0, 0.0, 800.0, 195.0);
            self.img(out, f, p, white);
            self.hits.push((p, Hit::Panel));
            let hover = self.recruit(st, out, o, 195.0, table, cursor);
            if st.app_sub_mode == 1 { self.weapon_row(st, out, o, hover); }
            self.help_button(st, out, self.pr(o, 546.0, 172.0, 19.0, 19.0), ["UI-Buttons L016", "UI-Buttons L017"], cursor); // Building*HelpButton
            return;
        }
        let o = [1120.0, 1080.0 - 170.0];
        let bg = self.pr(o, 0.0, 0.0, 800.0, 170.0);
        self.img(out, "UI-HUD 003", bg, white);
        self.hits.push((bg, Hit::Panel));
        let title = self.text_of(&self.title_key(st.app_sub_mode, st.in_structure_type));
        self.label(out, &title, o, 20.0, 24.0, 32.0, 0.0, BLACK);
        let (sub, id) = (st.app_sub_mode, st.in_structure);
        let ty = if sub == 11 { 10 } else { st.in_structure_type }; // setUpInbuilding: the stockpile panel's type
        self.sketch(st, out, o, ty); // the building's sketch, bottom left (reports.rs)
        match sub {
            2 => self.keep(st, out, o),
            4 => self.granary(st, out, o),
            23 => { self.recruit(st, out, o, 170.0, &GUILD, cursor); } // engineers' guild
            24 => self.one_recruit(st, out, o, &TUNNELLER, "TEXT_IN_TUNNELLERS_GUILD_002", cursor),
            96 => { // cathedral: monks (Arab lords: temple guards, their own no-gold line)
                let arab = matches!(st.lord_type, 1 | 2 | 6 | 7);
                self.one_recruit(st, out, o, &MONK, if arab { "TEXT_NEW_CTEXT_259" } else { "TEXT_IN_CATHEDRAL_002" }, cursor);
            }
            11 => self.goods_row(st, out, o, &[2, 3, 4, 6, 7, 9, 16, 14], 150.0),
            12 => self.goods_row(st, out, o, &[17, 18, 19, 20, 21, 22, 23, 24], 150.0),
            25 | 53..=57 => self.trade(st, out, o, cursor),
            3 => self.inn(st, out, o),
            13..=15 => self.workshop(st, out, o, cursor),
            35 => self.church(st, out, o),
            // ShowGate / ShowDrawbridge: open (CommandParameter 1 -> 10) and close (2 -> 11); the lit pair follows
            // gatehouse_state (HUD_Buildings.UpdateButtonState: 10 = open)
            36 | 37 => {
                let a = if sub == 36 { 1001 } else { 1008 }; // GateHouseState / DrawbridgeState
                let (open, close) = if st.gate_state == 10 { ("N014", "N015") } else { ("N013", "N016") };
                for (x, k, v) in [(203.0, open, 10), (263.0, close, 11)] {
                    let key = format!("UI-Buttons {k}");
                    self.button(out, self.pr(o, x, 100.0, 57.0, 57.0), &key, &key, false, cursor, Some(Hit::Act(a, id, v)));
                }
            }
            45 => self.outpost(st, out, o, cursor),
            _ => {}
        }
        // ShowInfoPanel: BuildingLine3Text, 18 px wrapped in 380×60 at (140, bottom 40)
        if let Some(t) = self.info_text(st, sub, ty) {
            for (k, l) in self.wrap(&t, 18.0, 380.0).iter().enumerate() { self.label(out, l, o, 140.0, 70.0 + 22.0 * k as f32, 18.0, 0.0, BLACK); }
        }
        let plain = !report_panel(sub) && sub != 70;
        // WorkerPanel (GetBuildingSleepable = IsWorkerBuilding): the ZZZ toggle, ToggleSleep (1003); lit while asleep
        if plain && (1..110).contains(&ty) && SLEEPABLE[ty as usize] == 1 {
            let (a, b) = if st.building_type_sleeping != 0 { ("UI-Buttons L015", "UI-Buttons L014") } else { ("UI-Buttons L014", "UI-Buttons L015") };
            self.button(out, self.pr(o, 508.0, 38.0, 36.0, 35.0), a, b, false, cursor, Some(Hit::Act(1003, id, 0)));
        }
        // ShowRepairPanel (GetBuildingShowRepair: towers, gatehouses): Repair (RepairBuilding 1002) + the HP bar
        if plain && sub != 53 && matches!(ty, 45..=48 | 74..=78) { self.repair(st, out, o, cursor); }
        // Delete (ours: the original panels have none, only the SideTabs demolish tool): that tool's sprite in the
        // ZZZ / help column. Not for the keep or the trading post's pages.
        if plain && !matches!(sub, 2 | 25 | 53..=57) {
            let p = self.pr(o, 511.0, 88.0, 30.0, 32.0);
            if Self::inside(p, cursor) { self.panel_tip = Some(self.text_of("TEXT_BUBBLE_HELP_TEXT_146")); }
            self.button(out, p, "UI-Buttons A041", "UI-Buttons A042", false, cursor, Some(Hit::Delete(id)));
        }
        self.help_button(st, out, self.pr(o, 508.0, 131.0, 36.0, 35.0), ["UI-Buttons L011", "UI-Buttons L013"], cursor); // BuildingHelpButton
        // ShowWorkersPanel: two 16 px lines at (160, bottom 22 / 2)
        let (l1, l2) = self.workers_text(st);
        self.label(out, &l1, o, 160.0, 170.0 - 22.0 - 28.0, 16.0, 0.0, BLACK);
        self.label(out, &l2, o, 160.0, 170.0 - 2.0 - 28.0, 16.0, 0.0, BLACK);
    }

    /// Recruit buttons (MakeTroop, GameAction 1007, s = 1 as C# sends it, v = eChimps) of a panel `ph` high, and the
    /// strip above it: while a button is hovered "Cost N [gold]" (TroopCostsText) and the troop's name
    /// (TroopNameText), always "Available Peasants N" (FreePeasantText). A locked button still sends: the engine
    /// refuses what the player can't pay, as with the game's overlay Button.
    fn recruit(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], ph: f32, table: &[Recruit], cursor: [f32; 2]) -> Option<i32> {
        let mut hover = None;
        for &(t, x, b, h, w, n) in table {
            let Some(why) = recruit_why(t, st) else { continue };
            let ok = why.is_none();
            let key = |k: u32| format!("UI-Buttons O{k:03}");
            let w = if w > 0.0 { w } else { self.rects.get(&key(n)).map_or(h / 2.0, |r| h * r[2] / r[3]) };
            let p = self.pr(o, x, ph - b - h, w, h);
            let over = Self::inside(p, cursor);
            if over { hover = Some(t); }
            match (ok, over) {
                (true, _) => self.img_fit(out, &key(n + over as u32), p, [1.0; 4]),
                (false, _) => self.img_fit(out, &key(n + 2), p, [1.0, 1.0, 1.0, 0.5]),
            }
            self.hits.push((p, Hit::Recruit(t, why)));
        }
        // FatControler's lastTroopsAmountToMake (1, Shift 5, Ctrl all) -> setCreateTroopText: the cost of that many,
        // "Name xN" and the hotkey line "[SHIFT] x5   [CTRL] xMax"
        // the guilds and the cathedral (EngineersCostsText & co.): "Name xN  Cost N [gold]" in one strip at (20, -20)
        // and the hotkey amounts as two black lines at x 130 (TroopShiftCostText bottom 24, TroopCtrlCostText bottom 0)
        let guild = matches!(st.app_sub_mode, 23 | 24 | 96);
        if let Some(t) = hover {
            let max = recruit_max(t, st);
            let n = if self.shift { max.min(5) } else if self.ctrl { max } else { 1 };
            let cost = format!("{} {}", self.text_of("TEXT_IN_BARRACKS_003"), gold_cost(t, st) * n);
            let arab_monk = t == 37 && matches!(st.lord_type, 1 | 2 | 6 | 7);
            let mut name = self.text_of(&if arab_monk { "TEXT_NEW_CTEXT_256".to_string() } else { format!("TEXT_CHIMP_NAMES_{:03}", t + 1) });
            if n > 1 { name += &format!(" x{n}"); }
            let (shift, ctrl) = (format!("[{}] x{}", self.text_of("TEXT_HOT_KEYS_079").to_uppercase(), max.min(5)), format!("[{}] x{max}", self.text_of("TEXT_HOT_KEYS_080").to_uppercase()));
            if guild {
                self.strip_at(out, o, 20.0, -20.0, &format!("{name}  {cost}"), 0.0, Some("UISprites F001"), None);
                self.label(out, &shift, o, 130.0, ph - 24.0 - 28.0, 16.0, 0.0, BLACK);
                self.label(out, &ctrl, o, 130.0, ph - 28.0, 16.0, 0.0, BLACK);
            } else {
                self.strip(out, o, 10.0, &cost, 0.0, Some("UISprites F001"), None);
                self.strip(out, o, 335.0, &name, 0.5, None, Some(&format!("{shift}   {ctrl}")));
            }
        }
        let free = format!("{} {}", self.text_of("TEXT_IN_BARRACKS_004"), st.peasants);
        self.strip_at(out, o, 540.0, if guild { -20.0 } else { -18.0 }, &free, 1.0, None, None);
        hover
    }

    /// The barracks' weapon row (HUD_Buildings BarracksPanel, Left / Bottom 50×52 at x 16 + 60 k): each weapon the
    /// map allows (weapon_types_available), at half opacity without stock, its stock below in white; the hovered
    /// recruit button lights the ones that troop takes (Show_Barracks*2).
    fn weapon_row(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], hover: Option<i32>) {
        // weapon_types_available index -> (x, normal / lit sprite M…, stock resource; horses: total_horses_available)
        const ROW: [(f32, u32, usize); 9] = [(16.0, 1, 17), (196.0, 10, 18), (76.0, 4, 19), (256.0, 13, 20), (136.0, 16, 21),
            (316.0, 7, 22), (376.0, 19, 23), (436.0, 22, 24), (496.0, 25, 0)];
        let lit: &[usize] = match hover { Some(22) => &[0], Some(24) => &[2], Some(26) => &[4, 6], Some(23) => &[1, 6], Some(25) => &[3, 7],
            Some(27) => &[5, 7], Some(28) => &[5, 7, 8], _ => &[] };
        for (k, &(x, m, res)) in ROW.iter().enumerate().filter(|(k, _)| st.weapon_avail[*k] > 0) {
            let stock = if k == 8 { st.horses as i32 } else { st.resources[res] };
            let p = self.pr(o, x, 195.0 - 52.0 + if k == 8 { 2.0 } else { 0.0 }, 50.0, 52.0);
            let key = format!("UI-Buttons M{:03}", if lit.contains(&k) { m + 1 } else { m });
            self.img_fit(out, &key, p, [1.0, 1.0, 1.0, if stock > 0 { 1.0 } else { 0.5 }]);
            if stock > 0 {
                let asc = self.font.horizontal_line_metrics(16.0).map_or(12.8, |m| m.ascent);
                let [tx, ty] = self.br(o[0] + x + 25.0, o[1] + 195.0 + 5.0 - 22.0 + asc);
                self.text_sh(out, &stock.to_string(), tx, ty, 16.0, 0.5, [1.0; 4]);
            }
        }
    }

    /// A 16 px #DDDDDD TextBlock on #55000000 with the HUD drop shadow, Padding 10,2,2,2, its box 18 px above the
    /// panel (Margin top -18), anchored at panel-local x by `align`; `icon`: an 18 px high image after the text.
    /// `line2`: a second, 12 px line under it (LineBreak + Run FontSize 12), centred in the box.
    fn strip(&mut self, out: &mut Vec<HudTri>, o: [f32; 2], x: f32, text: &str, align: f32, icon: Option<&str>, line2: Option<&str>) {
        self.strip_at(out, o, x, -18.0, text, align, icon, line2)
    }

    /// `strip` with its box top `top` px from the panel's top (the guild panels' strips sit at -20).
    fn strip_at(&mut self, out: &mut Vec<HudTri>, o: [f32; 2], x: f32, top: f32, text: &str, align: f32, icon: Option<&str>, line2: Option<&str>) {
        let tw = self.tw(text, 16.0).max(line2.map_or(0.0, |l| self.tw(l, 12.0)));
        let iw = icon.and_then(|k| self.rects.get(k)).map_or(0.0, |r| 5.0 + 18.0 * r[2] / r[3]);
        let bw = 10.0 + tw + iw + 2.0 + if icon.is_some() { 4.0 } else { 10.0 };
        let x0 = x - bw * align;
        let lh2 = line2.map_or(0.0, |_| self.font.horizontal_line_metrics(12.0).map_or(15.0, |m| m.new_line_size));
        Self::fill(out, self.pr(o, x0, top, bw, 24.0 + lh2), [0.0, 0.0, 0.0, 0x55 as f32 / 255.0]);
        let asc = self.font.horizontal_line_metrics(16.0).map_or(12.8, |m| m.ascent);
        let (cx, a) = if line2.is_some() { (x0 + 10.0 + tw / 2.0, 0.5) } else { (x0 + 10.0, 0.0) }; // TextAlignment Center
        let [tx, ty] = self.br(o[0] + cx, o[1] + top + 2.0 + asc);
        self.text_sh(out, text, tx, ty, 16.0, a, GREY);
        if let Some(l) = line2 {
            let [tx, ty] = self.br(o[0] + cx, o[1] + top + 2.0 + asc + lh2);
            self.text_sh(out, l, tx, ty, 12.0, 0.5, GREY);
        }
        if let Some(k) = icon { self.img_fit(out, k, self.pr(o, x0 + 10.0 + tw + 5.0, top + 3.0, iw - 5.0, 18.0), [1.0; 4]); }
    }

    /// Part `f` (fractions x0, y0, x1, y1) of atlas sprite `key` into screen rect `p`.
    fn img_part(&self, out: &mut Vec<HudTri>, key: &str, f: [f32; 4], p: [f32; 4], color: [f32; 4]) {
        let Some(&[x, y, w, h]) = self.rects.get(key) else { return };
        Self::quad(out, 0, p, [(x + f[0] * w) / ATLAS, (ATLAS - y - f[1] * h) / ATLAS, (x + f[2] * w) / ATLAS, (ATLAS - y - f[3] * h) / ATLAS], color, 0.0);
    }

    /// ImageBrush TileMode=Tile: `key` in `tw`×`th` screen-px tiles from the top-left of `p`, cut at the far edges.
    fn img_tiled(&self, out: &mut Vec<HudTri>, key: &str, p: [f32; 4], tw: f32, th: f32) { self.img_tiled_c(out, key, p, tw, th, [1.0; 4]) }
    fn img_tiled_c(&self, out: &mut Vec<HudTri>, key: &str, p: [f32; 4], tw: f32, th: f32, color: [f32; 4]) {
        let mut y = p[1];
        while y < p[3] - 0.01 {
            let y1 = (y + th).min(p[3]);
            let mut x = p[0];
            while x < p[2] - 0.01 {
                let x1 = (x + tw).min(p[2]);
                self.img_part(out, key, [0.0, 0.0, (x1 - x) / tw, (y1 - y) / th], [x, y, x1, y1], color);
                x = x1;
            }
            y = y1;
        }
    }

    /// BTN_SH_GlowS(Disable) `w` wide at screen rect top-left `at`: 3-slice bar (caps 29.5 wide, 31 high from y − 1.5),
    /// Button_02_* while hovered or `lit`, 16 px #F4DEAA text centred; `hit` None = disabled (text dimmed, no click).
    fn glow(&mut self, out: &mut Vec<HudTri>, at: [f32; 2], w: f32, text: &str, lit: bool, cursor: [f32; 2], hit: Option<Hit>) {
        let s = self.s;
        let p = [at[0], at[1] - 1.5 * s, at[0] + w * s, at[1] + 29.5 * s];
        let hot = lit || hit.is_some() && Self::inside(p, cursor);
        if hot && hit.is_some() { crate::audio::hover(p, crate::audio::ui_id::RED_BUTTON, 1.0); }
        let held = hit.is_some() && Self::inside(p, cursor) && self.down;
        self.glow_bar(out, p, hot, true, if held { PRESSED } else { [1.0; 4] });
        let color = [ACCENT7[0], ACCENT7[1], ACCENT7[2], if hit.is_none() { 0.5 } else if held { 0.9 } else { 1.0 }];
        self.text(out, text, (p[0] + p[2]) / 2.0, at[1] + (15.0 + 16.0 * 0.33 + held as u8 as f32) * s, 16.0, 1.0, 0.0, 0.5, color); // centred on y + 15
        if let Some(h) = hit { self.hits.push((p, h)); }
    }

    /// BTN_SH_GlowS's bar filling screen rect `p` at any size: Button_01_* (Button_02_*, the glow, while `glow`), the
    /// caps 29.5 / 31 of the height wide, the centre tiled at that pitch. `star` false (ours: the label would reach
    /// it) or a button narrower than two caps (half of each shows): the left cap is the right one mirrored.
    pub(crate) fn glow_bar(&self, out: &mut Vec<HudTri>, p: [f32; 4], glow: bool, star: bool, color: [f32; 4]) {
        let k = (p[3] - p[1]) / 31.0; // screen px per px of the 31 px bar
        let cap = (29.5 * k).min((p[2] - p[0]) / 2.0);
        let f = cap / (29.5 * k); // the part of a cap that shows
        let n = if glow { ["Button_02_Left_", "Button_02_Center", "Button_02_Right"] } else { ["Button_01_Left", "Button_01_Center", "Button_01_Right"] };
        let (left, part) = if f < 1.0 || !star { (n[2], [1.0, 0.0, 1.0 - f, 1.0]) } else { (n[0], [0.0, 0.0, 1.0, 1.0]) };
        self.img_part(out, left, part, [p[0], p[1], p[0] + cap, p[3]], color);
        self.img_tiled_c(out, n[1], [p[0] + cap, p[1], p[2] - cap, p[3]], 29.5 * k, 31.0 * k, color);
        self.img_part(out, n[2], [1.0 - f, 0.0, 1.0, 1.0], [p[2] - cap, p[1], p[2], p[3]], color);
    }

    /// The port's own text buttons in the game's look: BTN_SH_GlowS (`glow_bar`) filling screen rect `p`, `label`
    /// centred in #F4DEAA at 16 / 31 of its height. `hot` (hovered): the glow bar and the red button's hover sound;
    /// `on` (GlowSB's selected, a toggle that is on): the glow bar, #FFD700 text; held down: the bar darker, the text
    /// 90 % (IsPressed) and a px lower; `enabled` false: no glow, text 60 % (IsEnabled False).
    pub(crate) fn text_button(&mut self, out: &mut Vec<HudTri>, label: &str, p: [f32; 4], hot: bool, on: bool, enabled: bool) {
        let hot = hot && enabled;
        let held = hot && self.down;
        if hot { crate::audio::hover(p, crate::audio::ui_id::RED_BUTTON, 1.0); }
        let (k, c) = ((p[3] - p[1]) / 31.0, if on { [1.0, 0.843, 0.0] } else { ACCENT7 });
        let size = 16.0 * k / self.s;
        let tw = self.text(&mut vec![], label, 0.0, 0.0, size, 1.0, 0.0, 0.0, [1.0; 4]);
        self.glow_bar(out, p, hot || on, p[2] - p[0] - tw >= 40.0 * k, if held { PRESSED } else { [1.0; 4] }); // the star: the cap's left 20 px
        let a = if !enabled { 0.6 } else if held { 0.9 } else { 1.0 };
        self.text(out, label, (p[0] + p[2]) / 2.0, p[1] + (16.5 + 16.0 * 0.33 + held as u8 as f32) * k, size, 1.0, 0.0, 0.5, [c[0], c[1], c[2], a]);
    }

    /// The default TextBox's look (Options > Player Settings' name): #631200, a 1 px #6D7378 edge while `active` (typing
    /// or hovered); the caller writes the text.
    pub(crate) fn text_field(out: &mut Vec<HudTri>, p: [f32; 4], active: bool, s: f32) {
        const FILL: [f32; 4] = [0.388, 0.071, 0.0, 1.0]; // #631200
        Self::fill(out, p, if active { [0.427, 0.451, 0.471, 1.0] } else { [0.0, 0.0, 0.0, 1.0] }); // #6D7378
        Self::fill(out, [p[0] + s, p[1] + s, p[2] - s, p[3] - s], FILL);
    }

    /// The port's dark panel (light text reads on it): Panel_BG darkened, in the carved frame (`frame_c`).
    pub(crate) fn panel(&self, out: &mut Vec<HudTri>, p: [f32; 4], s: f32) {
        self.img(out, "Panel_BG", p, [0.768, 0.735, 0.702, 0.97]);
        Self::fill(out, p, [0.315, 0.271, 0.221, 0.35]);
        self.frame_c(out, p, s);
    }

    /// A progress bar in screen rect `r`: a dark groove (#1E1C10 in black, the HUD's own bars) filled to `frac` with
    /// the game's Loading Bar texture.
    pub(crate) fn progress(&self, out: &mut Vec<HudTri>, r: [f32; 4], frac: f32) {
        Self::fill(out, r, BLACK);
        let e = ((r[3] - r[1]) / 12.0).max(1.0);
        let r = [r[0] + e, r[1] + e, r[2] - e, r[3] - e];
        Self::fill(out, r, [0x1e as f32 / 255.0, 0x1c as f32 / 255.0, 0x10 as f32 / 255.0, 1.0]);
        let f = frac.clamp(0.0, 1.0);
        self.img_part(out, "Loading Bar", [0.0, 0.0, f, 1.0], [r[0], r[1], r[0] + (r[2] - r[0]) * f, r[3]], [1.0; 4]);
    }

    /// A convex polygon `pts` (screen px) on the white texel block, like `fill`.
    fn fill_convex(out: &mut Vec<HudTri>, pts: &[[f32; 2]], color: [f32; 4]) {
        let t = 1.0 / FONT_PX as f32;
        let v = |p: [f32; 2]| Vert { pos: p, uv: [t, t], color, team: [0.0; 4] };
        for k in 1..pts.len() - 1 { out.push((1, [v(pts[0]), v(pts[k]), v(pts[k + 1])])); }
    }

    /// CheckBox_Template in screen rect `p` (20×20 design px): #631200; hovered a 2 px #9FA3A7 edge, held #6D7378;
    /// checked #D59C20 in a #EB452F edge (ours: 1 px; hovered 2) with Geometry.CheckBox.Check, the 14 px tick, in #631200.
    pub(crate) fn checkbox(out: &mut Vec<HudTri>, p: [f32; 4], on: bool, hot: bool, held: bool) {
        let k = (p[2] - p[0]) / 20.0;
        let c = |v: u32| [(v >> 16) as f32 / 255.0, (v >> 8 & 0xff) as f32 / 255.0, (v & 0xff) as f32 / 255.0, 1.0];
        let e = if hot { 2.0 * k } else if on { k } else { 0.0 };
        Self::fill(out, p, if on { c(0xeb452f) } else { c(0x9fa3a7) });
        Self::fill(out, [p[0] + e, p[1] + e, p[2] - e, p[3] - e], if on { c(0xd59c20) } else if held { c(0x6d7378) } else { c(0x631200) });
        if on { // M0,7.5 L5.5,12.5 14,2.75 11.75,1 5.5,9 1.5,5.5 z in the 14 px box 3 px in: two convex arms
            let at = |x: f32, y: f32| [p[0] + (3.0 + x) * k, p[1] + (3.0 + y) * k];
            Self::fill_convex(out, &[at(0.0, 7.5), at(5.5, 12.5), at(5.5, 9.0), at(1.5, 5.5)], c(0x631200));
            Self::fill_convex(out, &[at(5.5, 12.5), at(14.0, 2.75), at(11.75, 1.0), at(5.5, 9.0)], c(0x631200));
        }
    }

    /// BDR_SH1DEc around screen rect `p`, the carved frame only (the inside is the caller's): Panel_Border_* strips 30
    /// px (18 out, 12 in) tiled 250 long, Panel_Corner_2_* corners; `s` = the layout's scale.
    pub(crate) fn frame_c(&self, out: &mut Vec<HudTri>, p: [f32; 4], s: f32) {
        let ([x0, y0, x1, y1], e) = (p, |v: f32| v * s);
        self.img_tiled(out, "Panel_Border_Top", [x0 - e(1.0), y0 - e(18.0), x1 + e(1.0), y0 + e(12.0)], e(250.0), e(30.0));
        self.img_tiled(out, "Panel_Border_Bottom", [x0 - e(1.0), y1 - e(12.0), x1 + e(1.0), y1 + e(18.0)], e(250.0), e(30.0));
        self.img_tiled(out, "Panel_Border_Left", [x0 - e(18.0), y0 - e(1.0), x0 + e(12.0), y1 + e(1.0)], e(30.0), e(250.0));
        self.img_tiled(out, "Panel_Border_Right", [x1 - e(12.0), y0 - e(1.0), x1 + e(18.0), y1 + e(1.0)], e(30.0), e(250.0));
        for (k, x, y) in [("Top_Left", x0 - e(18.0), y0 - e(18.0)), ("Top_Right", x1 - e(12.0), y0 - e(18.0)), ("Bottom_Left", x0 - e(18.0), y1 - e(12.0)), ("Bottom_Right", x1 - e(12.0), y1 - e(12.0))] {
            self.img(out, &format!("Panel_Corner_2_{k}"), [x, y, x + e(30.0), y + e(30.0)], [1.0; 4]);
        }
    }

    /// SDR_SH_Plain_Tickless (Top / Bottom): a 2 px black track across the 22 px high box at panel-local (x, y), the
    /// 20×15 thumb (Slider_Thumb_top 5 px up / _bottom 5 px down) at `frac`; a click on the box sets the value.
    fn slider_box(&mut self, out: &mut Vec<HudTri>, o: [f32; 2], x: f32, y: f32, w: f32, frac: f32, top: bool, hit: Option<Hit>) {
        Self::fill(out, self.pr(o, x, y + 10.0, w, 2.0), BLACK);
        let (key, dy) = if top { ("Slider_Thumb_top", -5.0) } else { ("Slider_Thumb_bottom", 5.0) };
        self.img(out, key, self.pr(o, x + frac.clamp(0.0, 1.0) * (w - 20.0), y + 3.5 + dy, 20.0, 15.0), [1.0; 4]);
        if let Some(h) = hit { self.hits.push((self.pr(o, x, y + if top { -4.0 } else { 11.0 }, w, 15.0), h)); }
    }

    /// A slider clicked at `frac` of its width: 0 auto-sell (1..100), 1 auto-buy (0..99; HUD_Buildings keeps buy below
    /// sell, and moving one turns auto trading on), 2 outpost size (0..5, SetOutpostSize 1056), 3 outpost start
    /// delay (0..24000, SetOutpostDelay 1057).
    fn slider(&mut self, k: u8, id: i32, frac: f32) {
        match k {
            0 | 1 => {
                let Some(mut a) = self.auto else { return };
                if k == 0 {
                    let i = (1.0 + frac * 99.0).round() as usize;
                    if i <= a.buy { a.buy = i - 1; self.actions.push([1052, 0, CURVE[a.buy]]); }
                    a.sell = i;
                    self.actions.push([1053, 0, CURVE[i]]); // Autotrade_SetSell
                } else {
                    let i = (frac * 99.0).round() as usize;
                    if i >= a.sell { a.sell = i + 1; self.actions.push([1053, 0, CURVE[a.sell]]); }
                    a.buy = i;
                    self.actions.push([1052, 0, CURVE[i]]); // Autotrade_SetBuy
                }
                if !a.on { a.on = true; self.actions.push([1051, -1, 1]); } // toggleAutoTrade
                self.auto = Some(a);
            }
            2 => self.actions.push([1056, id, (frac * 5.0).round() as i32]),
            3 => self.actions.push([1057, id, (frac * 24000.0).round() as i32]),
            _ => self.fb_slider(k, frac),
        }
    }

    /// HUD_Buildings.initAutoTrade(good): the good's auto-trade settings into the engine's staging (OnOff, SetSell,
    /// SetBuy with s = -1 snapshot them for Autotrade_Apply) and the sliders.
    fn init_auto(&mut self, g: i32) {
        let k = g.clamp(0, 24) as usize;
        let ([buy, sell], on) = (self.auto_goods.0.map(|a| a[k] as i32), self.auto_goods.1[k] as i32);
        self.actions.extend([[1051, g, on], [1053, -1, sell], [1052, -1, buy]]);
        let idx = |v: i32| CURVE.iter().position(|&c| v <= c).unwrap_or(100);
        self.auto = Some(AutoTrade { on: on > 0, buy: idx(buy).min(99), sell: idx(sell).max(1) });
    }

    /// The trading panel's auto-trade buttons (ButtonBuySell 100 / 101, ButtonNewTradeType 0, ButtonCycleTradeGoodsType):
    /// 0 open the auto view (trading paused while it shows), 1 auto trading on / off, 2 the back arrow (leaves the auto
    /// view, applying it, else back to the list it came from), 3 prev / next good `g`.
    fn auto_click(&mut self, k: u8, id: i32, g: i32) {
        match k {
            0 => { self.init_auto(g); self.actions.push([1050, 1, 1]); }
            1 => if let Some(a) = &mut self.auto { a.on = !a.on; self.actions.push([1051, -1, a.on as i32]); },
            2 if self.auto.is_some() => { self.auto = None; self.actions.extend([[1054, 0, 0], [1050, 0, 0]]); }
            2 => self.sub_mode = Some(match self.prev_trade { 1..=3 => 53 + self.prev_trade, 4 => 53, _ => 25 }),
            _ => {
                self.actions.push([1027, id, g]);
                if self.auto.is_some() { self.actions.push([1054, 0, 0]); self.init_auto(g); }
            }
        }
    }

    /// Tunnellers' guild / cathedral: the one recruit button, hidden with a no-gold line when the gold is short
    /// (FatControler: Visible only if getChimpGoldCost <= gold); nothing when the troop isn't offered (monk_available).
    fn one_recruit(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], table: &[Recruit], no_gold: &str, cursor: [f32; 2]) {
        let t = table[0].0;
        if recruit_why(t, st).is_none() { return; }
        if gold_cost(t, st) > st.resources[15] {
            let msg = self.text_of(no_gold);
            for (k, l) in self.wrap(&msg, 18.0, 380.0).iter().enumerate() { self.label(out, l, o, 140.0, 70.0 + 22.0 * k as f32, 18.0, 0.0, BLACK); }
            self.recruit(st, out, o, 170.0, &[], cursor); // the free-peasants strip stays
        } else {
            self.recruit(st, out, o, 170.0, table, cursor);
        }
    }

    /// InnPanel (FatControler sub 3): barrels + ale icon, flagons, working inns (all), the coverage face, coverage % and
    /// the next level (or "Maximum bonus achieved").
    fn inn(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2]) {
        let t = |n: i32| format!("TEXT_IN_INN_{:03}", n + 1);
        let [pints, barrels, working, total, pct, pop, next] = st.inn;
        self.label(out, &barrels.to_string(), o, 134.0 + 46.0, 36.0 + 6.0, 22.0, 1.0, BLACK);
        let h = self.rects.get("UISprites I021").map_or(39.0, |r| 50.0 * r[3] / r[2]);
        self.img(out, "UISprites I021", self.pr(o, 179.0, 36.0, 50.0, h), [1.0; 4]);
        let lines = [format!("{} {pints}", self.text_of(&t(3))), format!("{} {working} ({total})", self.text_of(&t(7)))];
        for (k, l) in lines.iter().enumerate() { self.label(out, l, o, 224.0, 30.0 + 20.0 * k as f32, 18.0, 0.0, BLACK); }
        self.pop_widget(out, o, 144.0, 70.0, pop / 25);
        let cover = format!("{} {pct}%", self.text_of(&t(5)));
        self.label(out, &cover, o, 224.0, 70.0, 18.0, 0.0, BLACK);
        let nl = if next > 0 { format!("{} {next}%", self.text_of(&t(6))) } else { self.text_of(&t(8)) };
        self.label(out, &nl, o, 224.0, 90.0, 18.0, 0.0, BLACK);
    }

    /// Fletcher (13) / poleturner (15) / blacksmith (14): Producing / Next lines (FatControler) and the two output
    /// radio buttons (57×57 from x 380, bottom 28; Sprite3 when it is the next weapon, shown as can_make_* allow).
    fn workshop(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], cursor: [f32; 2]) {
        let w = |n: i32| self.text_of(&format!("TEXT_IN_BLACKSMITHS_WORKSHOP_{:03}", n + 1));
        // (buttons: good + sprite; the good the C# text tests for, the other; no-resources line; Shift's "all"; hint)
        let (buttons, test, other, nores, all, hint) = match st.app_sub_mode {
            13 => ([(17, 5), (18, 7)], 17, 18, 11, 14, 437),
            15 => ([(19, 9), (20, 11)], 19, 20, 11, 16, 438),
            _ => ([(22, 1), (21, 3)], 21, 22, 12, 15, 439),
        };
        let name = |g: i32| match g { 17 => 7, 18 => 8, 19 => 9, 20 => 10, 21 => 6, _ => 5 };
        let pick = |v: u8| if v as i32 == test { test } else { other };
        let now = if st.weapon[2] > 0 { w(nores) } else { format!("{} {}", w(3), w(name(pick(st.weapon[0])))) };
        let next = format!("{} {}", w(4), w(name(pick(st.weapon[1]))));
        // ButtonMakeWeaponMouseEnter: "Next: X" (Shift: ": All ..."), then the Shift hint
        let tips = buttons.map(|(g, _)| {
            let t = format!("{} {}{}", w(4), w(name(g)), if self.shift { format!(" : {}", w(all)) } else { String::new() });
            format!("{t}   {}", self.text_of(&format!("TEXT_NEW_CTEXT_{:03}", hint + 1)))
        });
        self.label(out, &now, o, 160.0, 66.0, 16.0, 0.0, BLACK);
        self.label(out, &next, o, 160.0, 88.0, 16.0, 0.0, BLACK);
        let checked = if st.weapon[1] as i32 == buttons[0].0 { buttons[0].0 } else { buttons[1].0 }; // OpenFletchers & co.
        for (k, ((g, n), tip)) in buttons.into_iter().zip(tips).enumerate() {
            if st.can_make[(g - 17) as usize] == 0 { continue; }
            let p = self.pr(o, 380.0 + 57.0 * k as f32, 170.0 - 28.0 - 57.0, 57.0, 57.0);
            let key = format!("UI-Buttons N{:03}", n + (g == checked) as u32);
            self.button(out, p, &key, &key, false, cursor, Some(Hit::Workshop(st.in_structure, g)));
            if Self::inside(p, cursor) { self.panel_tip = Some(tip); }
        }
    }

    /// ChurchPanel (FatControler sub 35, European lords): this month's wedding, the rings, partners and their names,
    /// the gossip line.
    fn church(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2]) {
        if matches!(st.lord_type, 1 | 2 | 6 | 7) { return; }
        let [status, male, female, text, m1, m2, f1, f2] = st.marry;
        let gossip = if text == 34 && status <= 0 && st.workers_have == 0 { self.text_of("TEXT_NEW_TEXT2_048") }
            else { self.text_of(&format!("TEXT_MARRIAGE_{:03}", text + 1)) };
        if status > 0 {
            self.img(out, "UISprites J001", self.pr(o, 300.0, 64.0, 56.0, 26.0), [1.0; 4]);
            let name = |ui: &Self, a: i16, b: i16| {
                if a <= 0 { return String::new(); }
                let mut n = ui.text_of(&format!("TEXT_PEASANT_NAMES_{:03}", a + 1));
                if b > 0 { n += &format!(" {}", ui.text_of(&format!("TEXT_PEASANT_SURNAMES_{:03}", b + 1))); }
                n
            };
            let (groom, bride) = (name(self, m1, m2), name(self, f1, f2));
            self.img_fit(out, &partner(male), self.pr(o, 160.0, 0.0, 100.0, 100.0), [1.0; 4]);
            self.label(out, &groom, o, 260.0, 40.0, 16.0, 0.0, BLACK);
            self.img_fit(out, &partner(female), self.pr(o, 200.0, 50.0, 100.0, 100.0), [1.0; 4]);
            self.label(out, &bride, o, 300.0, 96.0, 16.0, 0.0, BLACK);
        }
        for (k, l) in self.wrap(&gossip, 16.0, 300.0).iter().enumerate() { self.label(out, l, o, 305.0, 130.0 + 19.0 * k as f32, 16.0, 0.5, BLACK); }
    }

    /// OutpostPanel / OutpostArab / OutpostBedouin (in_structure_type 106 / 107 / 2): size and start-delay sliders,
    /// eight troop toggles (BTN_SH_GlowSBOutpost, lit while bit k of the state is set; SetOutpostState 1055).
    fn outpost(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], cursor: [f32; 2]) {
        let id = st.in_structure;
        let op = |n: i32| format!("TEXT_IN_OUTPOST_{n:03}");
        let names: [String; 8] = match st.in_structure_type {
            107 => std::array::from_fn(|k| op(11 + k as i32)),
            2 => ["TEXT_CHIMP_NAMES_083", "TEXT_CHIMP_NAMES_085", "TEXT_CHIMP_NAMES_079", "TEXT_CHIMP_NAMES_084", "TEXT_CHIMP_NAMES_086",
                "TEXT_CHIMP_NAMES_081", "TEXT_IN_OUTPOST_018", "TEXT_IN_OUTPOST_008"].map(String::from),
            _ => std::array::from_fn(|k| op(2 + k as i32)),
        };
        for (k, key) in names.iter().enumerate() {
            let t = self.text_of(key);
            let at = self.br(o[0] + 250.0 + 150.0 * (k / 4) as f32, o[1] + 35.0 + 32.0 * (k % 4) as f32);
            self.glow(out, at, 140.0, &t, st.marry[4] & (1 << k) != 0, cursor, Some(Hit::Act(1055, id, k as i32)));
        }
        for (key, x, y, size, align) in [(19, 30.0, 55.0, 18.0, 0.0), (20, 30.0, 90.0, 16.0, 0.0), (21, 230.0, 90.0, 16.0, 1.0),
            (22, 30.0, 110.0, 18.0, 0.0), (23, 30.0, 145.0, 16.0, 0.0), (24, 230.0, 145.0, 16.0, 1.0)] {
            let t = self.text_of(&op(key));
            self.label(out, &t, o, x, y, size, align, BLACK);
        }
        self.slider_box(out, o, 30.0, 80.0, 200.0, st.marry[5] as f32 / 5.0, true, Some(Hit::Slider(2, id)));
        self.slider_box(out, o, 30.0, 135.0, 200.0, st.marry[6] as f32 / 24000.0, true, Some(Hit::Slider(3, id)));
    }

    /// ShowInfoPanel's BuildingLine3Text (HUD_Buildings.GetBuildingInfo; siege tents: FatControler's build progress).
    fn info_text(&self, st: &HudState, sub: i32, ty: i32) -> Option<String> {
        if let Some(n) = match sub { 58 | 95 => Some(320), 59 | 61 => Some(640), 60 => Some(1280), 62 => Some(120), _ => None } {
            return Some(format!("{}{}", st.ai_clock as i32 * 100 / n, self.text_of("TEXT_IN_SIEGE_TENT_007")));
        }
        if sub == 64 { return Some(self.text_of("TEXT_NEW_CTEXT_385")); }
        if report_panel(sub) { return None; }
        let key = match ty {
            1 => "TEXT_IN_HOUSE_002".to_string(), 21 => "TEXT_IN_QUARRYPILE_002".into(), 4 => "TEXT_IN_OXEN_BASE_004".into(),
            28 => "TEXT_IN_OIL_SMELTER_002".into(), 74..=78 => format!("TEXT_IN_TOWER_{:03}", ty - 72),
            62 | 63 | 91..=95 | 97 | 98 => "TEXT_IN_GALLOWS_002".into(), 65 | 101 => "TEXT_IN_MAYPOLE_002".into(),
            103 => "TEXT_IN_DANCING_BEAR_002".into(), 66 | 118..=120 => "TEXT_IN_GARDEN_002".into(), _ => return None,
        };
        Some(self.text_of(&key))
    }

    /// ShowRepairPanel: Repair (BTN_SH_GlowSDisable 100 wide at right margin 340, top 36; enabled while damaged and
    /// can_do_repairs), the 62×12 HP bar (fill 60 · hp / max, #54B727) and "hp/max" under it.
    fn repair(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], cursor: [f32; 2]) {
        let [can, hp, max] = st.hp.map(i32::from);
        let on = max > 0 && hp != max && can > 0;
        let t = self.text_of("TEXT_BUBBLE_HELP_TEXT_258");
        self.glow(out, self.br(o[0] + 360.0, o[1] + 36.0), 100.0, &t, false, cursor, on.then_some(Hit::Act(1002, st.in_structure, 0)));
        if max <= 0 { return; }
        Self::fill(out, self.pr(o, 474.0, 38.0, 62.0, 12.0), BLACK);
        Self::fill(out, self.pr(o, 475.0, 39.0, (60 * hp / max) as f32, 10.0), [0x54 as f32 / 255.0, 0xb7 as f32 / 255.0, 0x27 as f32 / 255.0, 1.0]);
        self.label(out, &format!("{hp}/{max}"), o, 474.0, 51.0, 14.0, 0.0, BLACK);
    }

    /// HUD_ConfirmationPopup.ShowConfirmationCheck for Disband: a 450×170 BDR_SH1DEa panel in the middle (Panel_BG,
    /// tiled borders, corners), WGT_Heading with the question, Yes / No (BTN_SH_GlowS 100 wide, bottom 42) and the
    /// "Do not ask again" box (bottom 10). Over everything; it takes every click until answered.
    fn confirm_popup(&mut self, out: &mut Vec<HudTri>, checked: bool, cursor: [f32; 2]) {
        let s = self.s;
        let (w, h) = (450.0, 170.0);
        let c = |ui: &Self, x: f32, y: f32| [ui.w / 2.0 + (x - w / 2.0) * s, ui.h / 2.0 + (y - h / 2.0) * s]; // panel px -> screen
        let r = |ui: &Self, x: f32, y: f32, rw: f32, rh: f32| { let a = c(ui, x, y); [a[0], a[1], a[0] + rw * s, a[1] + rh * s] };
        self.hits.push(([0.0, 0.0, self.w, self.h], Hit::Panel));
        self.img_tiled(out, "Panel_BG", r(self, 2.0, 2.0, w - 4.0, h - 4.0), 256.0 * s, 256.0 * s);
        self.img_tiled(out, "Panel_Border_Top", r(self, -1.0, -18.0, w + 2.0, 30.0), 250.0 * s, 30.0 * s);
        self.img_tiled(out, "Panel_Border_Bottom", r(self, -1.0, h - 12.0, w + 2.0, 30.0), 250.0 * s, 30.0 * s);
        self.img_tiled(out, "Panel_Border_Left", r(self, -18.0, -1.0, 30.0, h + 2.0), 30.0 * s, 250.0 * s);
        self.img_tiled(out, "Panel_Border_Right", r(self, w - 12.0, -1.0, 30.0, h + 2.0), 30.0 * s, 250.0 * s);
        for (k, x, y) in [("Top_Left", -18.0, -18.0), ("Top_Right", w - 12.0, -18.0), ("Bottom_Left", -18.0, h - 12.0), ("Bottom_Right", w - 12.0, h - 12.0)] {
            self.img(out, &format!("Panel_Corner_{k}"), r(self, x, y, 30.0, 30.0), [1.0; 4]);
        }
        for kx in [25.0, w - 45.0] { self.img(out, "Panel_CelticKnot", r(self, kx, 11.0, 20.0, 40.0), [1.0; 4]); }
        self.img_tiled(out, "Panel_Divider", r(self, 3.0, 56.0, w - 6.0, 8.0), 250.0 * s, 7.5 * s);
        let title = self.text_of("TEXT_NEW_CTEXT_394");
        let [tx, ty] = c(self, w / 2.0, 34.0 + 30.0 * 0.33);
        self.text(out, &title, tx, ty, 30.0, 1.0, 0.0, 0.5, BLACK);
        for (k, x, key) in [(1, 80.0, "TEXT_GAME_OPTIONS_023"), (0, w - 80.0 - 100.0, "TEXT_GAME_OPTIONS_024")] {
            let t = self.text_of(key);
            self.glow(out, c(self, x, h - 42.0 - 28.0), 100.0, &t, false, cursor, Some(Hit::Confirm(k)));
        }
        // CheckBox_Template: 20×20, #631200 off; on #EB452F edge, #D59C20, a #631200 mark; the label 20 px after it
        let label = self.text_of("TEXT_NEW_CTEXT_395");
        let lw = self.tw(&label, 20.0);
        let x0 = (w - 26.0 - lw) / 2.0;
        let b = r(self, x0, h - 10.0 - 21.5, 20.0, 20.0);
        let hot = Self::inside(b, cursor);
        Self::checkbox(out, b, checked, hot, hot && self.down);
        let [lx, ly] = c(self, x0 + 26.0, h - 10.0 - 21.5 + 10.0 + 20.0 * 0.33);
        self.text(out, &label, lx, ly, 20.0, 1.0, 0.0, 0.0, BLACK);
        self.hits.push(([b[0], b[1], lx + lw * s, b[3]], Hit::Confirm(2)));
    }

    /// Stockpile / armoury / granary contents (read-only): goods-bar icons with their counts from panel-local x.
    fn goods_row(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], goods: &[usize], x0: f32) {
        for (k, &g) in goods.iter().enumerate() {
            let Some(icon) = self.goods.iter().find(|e| e.0 == g).map(|e| e.3.clone()) else { continue };
            let x = x0 + 50.0 * k as f32;
            self.img_fit(out, &icon, self.pr(o, x, 60.0, 40.0, 40.0), [1.0; 4]);
            self.label(out, &st.resources[g].to_string(), o, x + 20.0, 102.0, 18.0, 0.5, BLACK);
        }
    }

    /// The trading post (HUD_Buildings.xaml, 800×170): 25 = TradepostPanel (food / resources / weapons / prices), 54..56 =
    /// that category's goods with the stock under each, 53 = TradingPricesPanel (every good the map trades, "buy / 5 /
    /// sell" under it), 57 = TradingTradePanel for trading_current_goods: prev / next good (SetCurrentTradedGood 1027),
    /// Buy / Sell (one lot, Shift five) labelled with the amount and the price, the error / Shift line, and the
    /// auto-trade view (TradePost_Trade_Auto: Auto-Sell above / Auto-Buy below their sliders, on / off).
    fn trade(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2], cursor: [f32; 2]) {
        // good -> its UISprites icon (the category panels' buttons; + 1 = hover)
        const ICON: [(i32, u32); 20] = [(12, 17), (11, 15), (13, 19), (3, 3), (14, 21), (9, 11), (16, 25), (10, 13),
            (2, 1), (4, 5), (6, 7), (7, 9), (17, 27), (19, 31), (21, 35), (18, 29), (20, 33), (22, 37), (23, 39), (24, 41)];
        // per category: (good, left, bottom margin, the stock line's right margin)
        const FOOD: [(i32, f32, f32, f32); 8] = [(12, 75.0, 52.0, 590.0), (11, 129.0, 52.0, 488.0), (13, 182.0, 52.0, 386.0), (3, 233.0, 52.0, 284.0),
            (14, 284.0, 52.0, 182.0), (9, 333.0, 52.0, 80.0), (16, 384.0, 52.0, -22.0), (10, 437.0, 48.0, -124.0)];
        const RES: [(i32, f32, f32, f32); 4] = [(2, 152.0, 52.0, 450.0), (4, 217.0, 52.0, 316.0), (6, 281.0, 52.0, 182.0), (7, 356.0, 52.0, 48.0)];
        const WEAP: [(i32, f32, f32, f32); 8] = [(17, 75.0, 52.0, 590.0), (19, 127.0, 52.0, 488.0), (21, 182.0, 52.0, 386.0), (18, 232.0, 50.0, 284.0),
            (20, 283.0, 52.0, 182.0), (22, 334.0, 52.0, 80.0), (23, 388.0, 52.0, -22.0), (24, 438.0, 52.0, -124.0)];
        // TradingPricesPanel: (good, left, button bottom margin, the price line's right margin; its bottom = button's - 28)
        const PRICES: [(i32, f32, f32, f32); 20] = [(12, 26.0, 102.0, 706.0), (11, 76.0, 102.0, 612.0), (13, 130.0, 102.0, 504.0), (10, 186.0, 102.0, 392.0),
            (3, 30.0, 48.0, 706.0), (14, 78.0, 48.0, 610.0), (9, 134.0, 48.0, 502.0), (16, 186.0, 48.0, 392.0), (2, 240.0, 102.0, 284.0), (4, 291.0, 102.0, 180.0),
            (6, 240.0, 48.0, 284.0), (7, 292.0, 48.0, 180.0), (19, 346.0, 102.0, 70.0), (17, 398.0, 102.0, -34.0), (21, 450.0, 102.0, -140.0), (23, 498.0, 102.0, -230.0),
            (18, 346.0, 48.0, 70.0), (20, 398.0, 48.0, -34.0), (22, 450.0, 48.0, -140.0), (24, 498.0, 48.0, -230.0)];
        let icon = |g: i32, hover: bool| ICON.iter().find(|e| e.0 == g).map(|e| format!("UISprites I{:03}", e.1 + hover as u32));
        let white = [1.0; 4];
        let id = st.in_structure;
        // a Left / Bottom aligned element of the 170 px high panel
        let lb = |ui: &Self, x: f32, b: f32, w: f32, h: f32| ui.pr(o, x, 170.0 - b - h, w, h);
        let back = |ui: &mut Self, out: &mut Vec<HudTri>, b: f32, hit: Hit| { // ButtonBackToTradepost: G018 / G019
            let p = lb(ui, 24.0, b, 26.0, 26.0);
            ui.button(out, p, "UISprites G018", "UISprites G019", false, cursor, Some(hit));
        };
        match st.app_sub_mode {
            25 => {
                for (k, (n, x, b, w, h)) in [(2, 100.0, 44.0, 64.0, 47.0), (4, 180.0, 41.0, 70.0, 53.0), (6, 264.0, 40.0, 73.0, 61.0), (8, 380.0, 45.0, 71.0, 56.0)].into_iter().enumerate() {
                    let p = lb(self, x, b, w, h);
                    self.button(out, p, &format!("UISprites J{n:03}"), &format!("UISprites J{:03}", n + 1), false, cursor, Some(Hit::TradeType(k as i32 + 1)));
                }
                let t = self.text_of("TEXT_IN_TRADEPOST_020");
                self.label(out, &t, o, 256.0, 140.0, 16.0, 0.5, BLACK);
            }
            53 => {
                for &(g, x, b, r) in &PRICES {
                    let gi = g as usize;
                    if st.trade_buy_n[gi] < 0 { continue; } // AllStoredGoodsVisible: the map doesn't trade it
                    let (Some(k1), Some(k2)) = (icon(g, false), icon(g, true)) else { continue };
                    self.button(out, lb(self, x, b, 34.0, 34.0), &k1, &k2, false, cursor, Some(Hit::TradeGood(id, g)));
                    let price = format!("{}/{}", st.trade_buy[gi] / 5, st.trade_sell_fixed[gi]); // GoodsPrices
                    self.label(out, &price, o, (800.0 - r) / 2.0, 170.0 - (b - 28.0) - 28.0, 16.0, 0.5, BLACK);
                }
                back(self, out, 2.0, Hit::TradeType(0));
                let t = self.text_of("TEXT_IN_TRADEPOST_002");
                self.label(out, &t, o, (800.0 - 242.0) / 2.0, 148.0, 16.0, 0.5, BLACK);
            }
            54..=56 => {
                let list: &[(i32, f32, f32, f32)] = match st.app_sub_mode { 54 => &FOOD, 55 => &RES, _ => &WEAP };
                for &(g, x, b, r) in list {
                    let (Some(k1), Some(k2)) = (icon(g, false), icon(g, true)) else { continue };
                    let p = lb(self, x, b, 50.0, 50.0);
                    self.button(out, p, &k1, &k2, false, cursor, Some(Hit::TradeGood(id, g)));
                    self.label(out, &st.resources[g as usize].to_string(), o, (800.0 - r) / 2.0, 170.0 - 20.0 - 28.0, 20.0, 0.5, BLACK);
                }
                back(self, out, 4.0, Hit::TradeType(0));
                for (k, (n, x, b, w, h)) in [(2, 420.0, 110.0, 32.0, 24.0), (4, 460.0, 110.0, 35.0, 26.0), (6, 500.0, 108.0, 36.0, 30.0)].into_iter().enumerate() {
                    let p = lb(self, x, b, w, h);
                    self.button(out, p, &format!("UISprites J{n:03}"), &format!("UISprites J{:03}", n + 1), false, cursor, Some(Hit::TradeType(k as i32 + 1)));
                }
            }
            _ => { // 57
                self.auto_goods = (st.autotrade, st.autotrade_on);
                let [g, next, prev] = st.trading.map(|v| v as i32);
                for (good, x) in [(prev, 26.0), (next, 158.0)] {
                    if let Some(key) = icon(good, false) { self.img_fit(out, &key, self.pr(o, x, 40.0, 50.0, 50.0), white); }
                }
                for (k1, k2, x, to) in [("UI-Buttons R005", "UI-Buttons R006", 65.0, prev), ("UI-Buttons R007", "UI-Buttons R008", 109.0, next)] {
                    self.button(out, self.pr(o, x, 46.0, 42.0, 18.0), k1, k2, false, cursor, Some(Hit::Auto(3, id, to)));
                }
                // ponytail: SetSpriteWidth1(.., 100) sizes the large goods sprite; the icon at 75 px fits the panel
                if let Some(key) = icon(g, false) { self.img_fit(out, &key, self.pr(o, 80.0, 82.0, 75.0, 75.0), white); }
                let gi = (g.clamp(0, 24)) as usize;
                self.label(out, &st.resources[gi].to_string(), o, 50.0, 170.0 - 44.0 - 28.0, 24.0, 0.5, BLACK);
                self.img_fit(out, "UISprites A012", self.pr(o, 448.0, 76.0, 50.0, 50.0), white); // TradeGoldImage = GameSprites[14]
                self.label(out, &st.resources[15].to_string(), o, 486.0, 170.0 - 100.0 - 28.0, 24.0, 0.5, BLACK);
                back(self, out, 4.0, Hit::Auto(2, id, g));
                match self.auto {
                    None => { // TradePost_Trade_Normal
                        for (k, (a, y)) in [(1024, 44.0), (1025, 92.0)].into_iter().enumerate() {
                            let (n, cost) = if k == 0 { (st.trade_buy_n[gi], st.trade_buy[gi]) } else { (st.trade_sell_n[gi], st.trade_sell[gi]) };
                            let p = self.pr(o, 210.0, y, 208.0, 42.0);
                            let key = if n > 0 && Self::inside(p, cursor) { "UI-Buttons Q006" } else { "UI-Buttons Q005" };
                            self.img(out, key, p, if n > 0 { white } else { [1.0, 1.0, 1.0, 0.5] });
                            let t = format!("{}  {n}", self.text_of(if k == 0 { "TEXT_IN_TRADEPOST_006" } else { "TEXT_IN_TRADEPOST_007" }));
                            self.label(out, &t, o, 226.0, y + 9.0, 18.0, 0.0, CREAM);
                            self.label(out, &cost.to_string(), o, 402.0, y + 9.0, 18.0, 1.0, CREAM);
                            if n > 0 { self.hits.push((p, Hit::Trade(a, g))); } // IsEnabled false: nothing to buy / sell
                        }
                        // TradeErrorMessage: centred in 0..612, bottom-aligned; the refusal (ButtonBuySell's code) or
                        // the Shift hint
                        let t = self.trade_err.clone().unwrap_or_else(|| self.text_of("TEXT_NEW_TEXT2_131"));
                        self.label(out, &t, o, 306.0, 170.0 - 28.0, 18.0, 0.5, BLACK);
                        // Trade_GoTo_Auto (UpdateAutoTradeSelectButton: L022 / L023 while the good trades automatically)
                        let (k1, k2) = if st.autotrade_on[gi] > 0 { ("UI-Buttons L022", "UI-Buttons L023") } else { ("UI-Buttons L024", "UI-Buttons L025") };
                        self.button(out, lb(self, 470.0, 4.0, 36.0, 35.0), k1, k2, false, cursor, Some(Hit::Auto(0, id, g)));
                    }
                    Some(a) => {
                        let sell = format!("{} > {}", self.text_of("TEXT_NEW_TEXT2_145"), CURVE[a.sell]);
                        let buy = format!("{} < {}", self.text_of("TEXT_NEW_TEXT2_144"), CURVE[a.buy]);
                        self.label(out, &sell, o, 210.0, 32.0, 20.0, 0.0, BLACK);
                        self.slider_box(out, o, 211.0, 60.0, 208.0, (a.sell - 1) as f32 / 99.0, true, Some(Hit::Slider(0, id)));
                        self.slider_box(out, o, 210.0, 64.0, 208.0, a.buy as f32 / 99.0, false, Some(Hit::Slider(1, id)));
                        self.label(out, &buy, o, 210.0, 82.0, 20.0, 0.0, BLACK);
                        let (k1, k2) = if a.on { ("UI-Buttons L007", "UI-Buttons L008") } else { ("UI-Buttons L009", "UI-Buttons L010") };
                        self.button(out, lb(self, 470.0, 4.0, 36.0, 35.0), k1, k2, false, cursor, Some(Hit::Auto(1, id, g)));
                        // " Auto-trading Paused: Exit panel to resume. ", 18 px #DDDDDD on #88000000 above the panel
                        let t = format!(" {} ", self.text_of("TEXT_NEW_TEXT2_146"));
                        let tw = self.tw(&t, 18.0);
                        Self::fill(out, self.pr(o, 20.0, -10.0, tw, 24.0), [0.0, 0.0, 0.0, 0x88 as f32 / 255.0]);
                        let asc = self.font.horizontal_line_metrics(18.0).map_or(14.4, |m| m.ascent);
                        let [tx, ty] = self.br(o[0] + 20.0, o[1] - 10.0 + asc);
                        self.text_sh(out, &t, tx, ty, 18.0, 0.0, GREY);
                    }
                }
            }
        }
    }

    /// HUD_Buildings.GetBuildingTitle.
    fn title_key(&self, panel: i32, ty: i32) -> String {
        match panel {
            4 => "TEXT_IN_GRANARY_001".into(),
            54..=56 => format!("TEXT_IN_TRADEPOST_{:03}", panel - 51),
            69 | 72 => "TEXT_REPORT_BUTTONS_002".into(),
            73..=75 => format!("TEXT_REPORT_BUTTONS_{:03}", panel - 70),
            66..=68 | 76 => "TEXT_REPORT_BUTTONS_006".into(),
            77..=79 => format!("TEXT_REPORT_BUTTONS_{:03}", panel - 70),
            93 => "TEXT_IN_POND_001".into(),
            53 | 57 | 64 | 70 | 71 => String::new(),
            _ if (118..=120).contains(&ty) => "TEXT_IN_GARDEN_001".into(),
            _ => self.titles.get(ty.max(0) as usize).cloned().unwrap_or_default(), // ponytail: Arab lord gate titles skipped
        }
    }

    /// FatControler.addChimpActions: BuildingLine1Text / BuildingLine2Text. Keys are `lookUpText(section, n)` = `_{n+1}`.
    fn workers_text(&self, st: &HudState) -> (String, String) {
        let g = |n: i32| self.text_of(&format!("TEXT_IN_GENERAL_BUILDINGS_{:03}", n + 1));
        if st.building_type_sleeping != 0 { return (String::new(), self.text_of("TEXT_NEW_TEXT2_132")); }
        // the dog cage: once let out, "Here lie the poor bones of" (ponytail: the owner's name line, getSkirmishName, skipped)
        if st.in_structure_type == 100 { return (if st.dog_cage != 0 { self.text_of("TEXT_MISC2_002") } else { String::new() }, String::new()); }
        if st.have_building_stats <= 0 { return Default::default(); }
        if st.got_keep_access == 0 { (String::new(), g(9)) }
        else if st.turned_off > 0 { (String::new(), g(7)) }
        else if st.job_vacancies > 0 { (String::new(), g(if st.workers_have == 0 { 3 } else { 4 + st.job_vacancies as i32 })) }
        else if st.working > 0 {
            // ponytail: goods / mill / Arab variants of the action text skipped
            let act = if st.inchimp_n_text > 0 { self.text_of(&format!("TEXT_UNIT_ACTIONS_{:03}", st.inchimp_n_text + 1)) } else { String::new() };
            (g(10), act)
        } else { (g(4), String::new()) }
    }

    /// KeepPanel: population + income line, 12-step tax bar with handle at `tax_rate * 25` (InKeepSliderPos), tax text + face.
    fn keep(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2]) {
        // top line: StackPanel h 40 centred in (0..700, 0..120) -> y 40..80; 28 px TextBlocks centred -> top 46
        let (pop, inc) = (st.population.to_string(), st.tax_amount.to_string());
        let (wp, wi) = (self.tw(&pop, 20.0), self.tw(&inc, 20.0));
        let mut x = (700.0 - (wp + 27.0 + 20.0 + wi + 55.0)) / 2.0;
        self.label(out, &pop, o, x, 46.0, 20.0, 0.0, BLACK);
        x += wp;
        self.img_fit(out, "UISprites F006", self.pr(o, x + 5.0, 41.5, 22.0, 31.0), [1.0; 4]);
        x += 27.0 + 20.0;
        self.label(out, &inc, o, x, 46.0, 20.0, 0.0, BLACK);
        x += wi;
        self.img_fit(out, "UISprites I023", self.pr(o, x + 5.0, 40.0, 50.0, 39.0), [1.0; 4]);
        // tax bar: 14 buttons of 26 px from x 175, bottom 55 (down arrow, 12 notches, up arrow)
        for k in 0..14 {
            let s = match k { 0 => "UISprites G018", 13 => "UISprites G020", _ => "UISprites G023" };
            let p = self.pr(o, 175.0 + 25.0 * k as f32, 170.0 - 55.0 - 26.0, 26.0, 26.0);
            self.img_fit(out, s, p, [1.0; 4]);
            // SetTaxRate (1004): the arrows step the rate, a notch sets it
            let rate = match k { 0 => st.tax_rate - 1, 13 => st.tax_rate + 1, k => k - 1 };
            if (0..=11).contains(&rate) { self.hits.push((p, Hit::Act(1004, 0, rate))); }
        }
        self.img_fit(out, "UISprites G022", self.pr(o, 205.0 + st.tax_rate as f32 * 25.0, 170.0 - 50.0 - 36.0, 25.0, 36.0), [1.0; 4]);
        // bottom line: StackPanel centred in (-20..720, 120..170): tax text (bottom-aligned, 28 px + 4) + WGT_Popularity
        let rate = self.text_of(&format!("TEXT_IN_KEEP_{:03}", st.tax_rate + 8 + (st.tax_rate >= 10) as i32));
        let wr = self.tw(&rate, 20.0);
        let x = -20.0 + (740.0 - wr - 60.0) / 2.0;
        self.label(out, &rate, o, x, 133.0, 20.0, 0.0, BLACK);
        self.pop_widget(out, o, x + wr, 125.0, st.tax_popularity / 25);
    }

    /// GranaryPanel: food clock bar, units / months text, food-types face, ration dial (5 buttons + hand), ration text + face.
    fn granary(&mut self, st: &HudState, out: &mut Vec<HudTri>, o: [f32; 2]) {
        let white = [1.0; 4];
        let t = |n: i32| format!("TEXT_IN_GRANARY_{:03}", n + 1);
        self.img(out, "UISprites H001", self.pr(o, 17.0, 65.0, 176.0, 35.0), white);
        let bar = 160.0 * st.food_clock.clamp(0, 15000) as f32 / 15000.0;
        Self::fill(out, self.pr(o, 26.0, 71.0, bar, 14.0), [0x18 as f32 / 255.0, 0x59 as f32 / 255.0, 0x10 as f32 / 255.0, 0xaa as f32 / 255.0]);
        self.img(out, "UISprites H002", self.pr(o, 17.0, 65.0, 176.0, 35.0), white);
        let units = format!("{} {}", st.total_food, self.text_of(&t(if st.total_food != 1 { 1 } else { 2 })));
        self.label(out, &units, o, 24.0, 170.0 - 48.0 - 28.0, 16.0, 0.0, BLACK);
        let months = if st.months_of_food <= 0 { self.text_of(&t(11)) }
            else { format!("{} {}", st.months_of_food, self.text_of(&t(if st.months_of_food != 1 { 9 } else { 10 }))) };
        self.label(out, &months, o, 24.0, 170.0 - 24.0 - 28.0, 16.0, 0.0, BLACK);
        if st.food_types_eaten > 1 {
            self.pop_widget(out, o, 6.0, 130.0, st.foods_eaten_popularity / 25);
            let types = format!("{} {}", st.food_types_eaten, self.text_of(&t(if st.food_types_eaten != 1 { 7 } else { 8 })));
            self.label(out, &types, o, 76.0, 170.0 - 28.0, 16.0, 0.0, BLACK);
        }
        self.pop_widget(out, o, 470.0, 46.0, st.rationing_popularity / 25);
        let level = match st.rationing { 0 => 3, 1 => 4, 2 => 5, 3 => 12, 4 => 6, _ => -1 };
        if level >= 0 { let s = self.text_of(&t(level)); self.label(out, &s, o, 800.0 - 258.0, 30.0, 20.0, 1.0, BLACK); }
        // ration buttons, HorizontalAlignment=Center inside their margins (none, half, full, extra, double)
        // SetRationing (1005) values: none 0, half 1, full 2, extra 4, double 3 (InGranarySetRationHand)
        for (x, y, w, h, s, v) in [(309.5, 135.0, 55.0, 34.0, "G001", 0), (278.0, 111.5, 54.0, 32.0, "G003", 1), (335.5, 96.0, 55.0, 32.0, "G005", 2),
            (394.0, 110.0, 54.0, 34.0, "G007", 4), (372.0, 132.0, 56.0, 37.0, "G009", 3)] {
            let p = self.pr(o, x, y, w, h);
            self.img_fit(out, &format!("UISprites {s}"), p, white);
            self.hits.push((p, Hit::Act(1005, 0, v)));
        }
        // the hand image, all centred on (363, 112); InGranarySetRationHand maps 3 -> Double hand, 4 -> Extra hand
        let hand = match st.rationing { 0 => Some(("G016", 37.0, 38.0)), 1 => Some(("G017", 44.0, 27.0)), 2 => Some(("G013", 23.0, 45.0)),
            3 => Some(("G014", 45.0, 25.0)), 4 => Some(("G015", 30.0, 41.0)), _ => None };
        if let Some((s, w, h)) = hand {
            self.img_fit(out, &format!("UISprites {s}"), self.pr(o, 363.0 - w / 2.0, 112.0 - h / 2.0, w, h), white);
        }
        self.img_fit(out, "UISprites G011", self.pr(o, 800.0 - 290.0 - 56.0, 170.0 - 4.0 - 37.0, 56.0, 37.0), white); // to types-eaten
    }

    /// The engine's one-shot UI events not taken yet: a message bar (OnScreenText OST_MESSAGE_BAR, 10 s), and a ctrl +
    /// click on a building (GameMap case 25: its build tool, unless the troops panel is up and if available).
    fn take_events(&mut self, st: &HudState) {
        if let Some((kind, id, n)) = st.ev.message.filter(|m| m.2 != self.seen.0) {
            self.seen.0 = n;
            self.bar = Some((self.bar_text(kind, id), std::time::Instant::now()));
        }
        if let Some((kind, n)) = st.ev.pick.filter(|p| p.1 != self.seen.1) {
            self.seen.1 = n;
            let troops = st.app_mode == 14 && matches!(st.app_sub_mode, 61 | 62);
            let k = match kind { 46 => 131, 45 => 133, 47 => 127, k => k };
            let m = PICK.iter().find(|p| p.0 == k).map(|p| p.1).filter(|&m| st.available.get(m as usize).copied().unwrap_or(true));
            if let (false, Some(i)) = (troops, m.and_then(|m| self.build.iter().position(|b| b.mapper == m))) { self.selected = Some(i); }
        }
    }

    /// OST_MESSAGE_BAR text: kind > 0 = text section (the ones the engine posts), id = entry; kind −p = player p's
    /// lord (custom lords' own texts aside): TEXT_SKIRMISH_SPEECH id + 986.
    fn bar_text(&self, kind: i32, id: i32) -> String {
        let (sec, id) = match kind {
            201 => ("TEXT_ACTION", id),
            221 => ("TEXT_SKIRMISH_SPEECH", id),
            226 => ("TEXT_NEW_PRE_ATTACK", id),
            228 => ("TEXT_NEW_CTEXT", id),
            // a custom lord (hudmsg::f102bc0 posts kind −player): its text.txt line, else the shipped skirmish line
            ..=-1 => match crate::custom_lords::key(id).and_then(|k| crate::custom_lords::text(-kind as usize, k)) { Some(t) => return t, None => ("TEXT_SKIRMISH_SPEECH", id + 986) },
            k => return format!("text section {k}, {id}"), // ponytail: only the sections the sim posts are named
        };
        self.text_of(&format!("{sec}_{:03}", id + 1))
    }

    /// TextWrapping=Wrap at `max` design px.
    fn wrap(&mut self, s: &str, size: f32, max: f32) -> Vec<String> {
        let mut lines: Vec<String> = vec![];
        for w in s.split_whitespace() {
            let joined = lines.last().map(|l| format!("{l} {w}"));
            match joined {
                Some(j) if self.tw(&j, size) <= max => *lines.last_mut().unwrap() = j,
                _ => lines.push(w.to_string()),
            }
        }
        lines
    }

    /// MainHUD OST_Message_Bar: 17 px cream text wrapped at 350, padding 10,3,10,2 on #88000000, bottom-right at
    /// margin 0,0,70,160 (210 over the barracks' panel, 200 above a keep message); gone 10 s after it was posted.
    fn message_bar(&mut self, st: &HudState, out: &mut Vec<HudTri>) {
        let Some((t, at)) = self.bar.clone() else { return };
        if at.elapsed().as_secs_f32() >= 10.0 { self.bar = None; return; }
        let lines = self.wrap(&t, 17.0, 350.0);
        let m = self.font.horizontal_line_metrics(17.0);
        let (lh, asc) = m.map_or((20.0, 14.0), |m| (m.new_line_size, m.ascent));
        let tw = lines.iter().map(|l| self.tw(l, 17.0)).fold(0.0, f32::max);
        let keep_msg = self.ost.keep_message();
        let (r, b) = (1920.0 - 70.0, 1080.0 - if st.app_mode == 16 && st.app_sub_mode == 1 { 210.0 } else if keep_msg { 200.0 } else { 160.0 });
        let (x0, y0) = (r - tw - 20.0, b - lh * lines.len() as f32 - 5.0);
        let ([px0, py0], [px1, py1]) = (self.br(x0, y0), self.br(r, b));
        Self::fill(out, [px0, py0, px1, py1], [0.0, 0.0, 0.0, 0x88 as f32 / 255.0]);
        for (k, l) in lines.iter().enumerate() {
            let [x, y] = self.br(x0 + 10.0, y0 + 3.0 + asc + lh * k as f32);
            self.text_sh(out, l, x, y, 17.0, 0.0, CREAM);
        }
    }

    /// FatControler.UpdateObjectiveRows: one goal's text, amount text and tick.
    pub(crate) fn goal_row(&self, g: &Goal, campaign: bool) -> (String, String, bool) {
        let t = |sec: &str, i: i32| self.text_of(&format!("{sec}_{:03}", i + 1));
        let n = match g.id + (g.id >= 20) as i32 { 5 | 6 | 17 => 7, 15 | 32 => 8, n => n };
        let left = match n {
            25 | 27 | 28 | 29 | 31 => t("TEXT_NEW_TEXT", if g.kind == 31 { 166 } else { 132 + g.kind }),
            26 => t("TEXT_NEW_TEXT", 89),
            33 => t("TEXT_NEW_TEXT2", 147),
            34 => t("TEXT_NEW_TEXT2", 148),
            30 => t("TEXT_NEW_TEXT2", 6),
            3 if g.kind == 5 => t("TEXT_SCENARIO", 214), // TEXT_SCN_KILL_ALL_ENEMY_LORDS
            3 if g.kind > 0 => match [156, 9, 10, 11, 12, 213].get(g.kind as usize) { // GameData.lord_killed_list
                Some(&l) => format!("{} - {}", t("TEXT_OBJECTIVES", 3), t("TEXT_SCENARIO", l)),
                None => t("TEXT_OBJECTIVES", 3),
            },
            11..=14 if !campaign => t("TEXT_SCENARIO", n + 106),
            n => t("TEXT_OBJECTIVES", n),
        };
        let (goods, amount) = match g.id {
            5 | 6 | 7 | 17 => (g.kind > 0, true),
            1 | 4 | 20..=23 | 32 | 33 => (false, true),
            _ => (false, false),
        };
        let mut right = if goods { t("TEXT_GOODS", g.kind) + " " } else { String::new() };
        if amount { right += &if g.current != -9999 { format!("{} ({})", g.target, g.current) } else { g.target.to_string() }; }
        (left, right, g.complete > 0)
    }

    /// IngameUIScreens HUD_ObjectivesPanel (bottom-left): the Objectives button (36×34, 34 up; shown with goals or a
    /// timer) toggles HUD_Objectives in the goods bar's place: x 36, 400 wide, 25 px per row (SetSizeFromRows, the
    /// timer counts 2), 7 rows at most.
    fn objectives(&mut self, st: &HudState, out: &mut Vec<HudTri>, cursor: [f32; 2]) {
        let ev = &st.ev;
        if !ev.has_goals() { return; }
        // ponytail: GameMap drops campaign mission 8's timer (> 3000000); the mission number isn't in HudState
        let timer = ev.timer.filter(|t| t[2] > 0);
        let [x0, y0] = self.bl(0.0, 1080.0 - 68.0);
        self.button(out, [x0, y0, x0 + 36.0 * self.s, y0 + 34.0 * self.s], "Objectives_Normal", "Objectives_Highlight", false, cursor, Some(Hit::Goals));
        if !self.goals { return; }
        let rows = ev.goals.len().min(7);
        let h = 25.0 * (rows + 2 * timer.is_some() as usize) as f32 + 25.0;
        let top = 1080.0 - h;
        let (a, b) = (self.bl(-44.0, top), self.bl(436.0, 1110.0)); // Border 480 wide, margin -80,0,0,-30
        Self::fill(out, [a[0], a[1], b[0], b[1]], [0.0, 0.0, 0.0, 0x77 as f32 / 255.0]);
        let x = 56.0; // StackPanel margin 20
        let m = self.font.horizontal_line_metrics(18.0);
        let (asc, desc) = m.map_or((14.0, -4.0), |m| (m.ascent, m.descent));
        let mut y = top;
        if let Some([now, len, text]) = timer {
            let [tx, ty] = self.bl(x, y - 5.0 + 15.0 + asc);
            let label = self.text_of(&format!("TEXT_OBJECTIVES_{:03}", text + 1));
            self.text(out, &label, tx, ty, 18.0, 1.0, 0.0, 0.0, CREAM);
            let r = |ui: &Self, rx: f32, ry: f32, w: f32, h: f32| { let p = ui.bl(x + rx, y - 5.0 + ry); [p[0], p[1], p[0] + w * ui.s, p[1] + h * ui.s] };
            Self::fill(out, r(self, 148.0, 39.0, 206.0, 18.0), [0x1e as f32 / 255.0, 0x1c as f32 / 255.0, 0x10 as f32 / 255.0, 1.0]);
            let w = if len > 0 { 200.0 * now.min(len) as f32 / len as f32 } else { 0.0 }; // ObjectiveTimerWidth
            // TimerFill: vertical gradient #26922f -> #0B290F, as two bands
            Self::fill(out, r(self, 151.0, 42.0, w, 6.0), [0x26 as f32 / 255.0, 0x92 as f32 / 255.0, 0x2f as f32 / 255.0, 1.0]);
            Self::fill(out, r(self, 151.0, 48.0, w, 6.0), [0x18 as f32 / 255.0, 0x5e as f32 / 255.0, 0x1f as f32 / 255.0, 1.0]);
            let frame = [0xeb as f32 / 255.0, 0xe6 as f32 / 255.0, 0xb5 as f32 / 255.0, 1.0]; // Timer: 204×16, stroke 2
            for (fx, fy, fw, fh) in [(149.0, 40.0, 204.0, 2.0), (149.0, 54.0, 204.0, 2.0), (149.0, 40.0, 2.0, 16.0), (351.0, 40.0, 2.0, 16.0)] {
                Self::fill(out, r(self, fx, fy, fw, fh), frame);
            }
            y += 49.0;
        }
        for g in &ev.goals[..rows] {
            let (left, right, done) = self.goal_row(g, ev.campaign);
            let bottom = y - 10.0 + 48.0; // WGT_Objective 380×48, margin 0,-10,0,-13
            let [lx, ly] = self.bl(x, bottom + desc);
            self.text(out, &left, lx, ly, 18.0, 1.0, 0.0, 0.0, CREAM);
            let [rx, _] = self.bl(x + 380.0 - 54.0, 0.0);
            self.text(out, &right, rx, ly, 18.0, 1.0, 0.0, 1.0, CREAM);
            let (key, w, hh, mr) = if done { ("UI-Buttons R002", 42.0, 43.0, 2.0) } else { ("UI-Buttons R001", 17.0, 17.0, 26.0) };
            let [cx, cy] = self.bl(x + 380.0 - mr - w, bottom - 2.0 - hh);
            self.img(out, key, [cx, cy, cx + w * self.s, cy + hh * self.s], [1.0; 4]);
            y += 25.0;
        }
    }

    /// One frame of HUD triangles for window size `w`×`h` (logical px), mouse at `cursor`.
    pub fn draw(&mut self, st: &HudState, w: f32, h: f32, cursor: [f32; 2], tick: usize, ghost: Option<&Ghost>) -> Vec<HudTri> {
        let s = crate::ux::hud_scale(w, h); // Viewbox fit × the player's UI scale (ux.rs, ⌘= / ⌘-)
        if s != self.s { // glyphs are rasterised per scale: drop the old ones so resizing can't fill the atlas
            self.glyphs.clear();
            self.font_rgba.fill(0);
            white_block(&mut self.font_rgba);
            self.pen = (4, 0, 0);
            self.font_dirty = true;
        }
        (self.w, self.h, self.s) = (w, h, s);
        let mut out = vec![];
        self.hits.clear();
        if self.hidden { // MainViewModel.SetVisibleState(false): nothing but the right-click menu that brings it back
            self.rclick_draw(&mut out);
            return out;
        }
        self.take_events(st);
        self.lords = st.lords.clone();
        // a held slider follows the cursor (SDR_SH_* thumbs drag)
        if let Some((p, k, id, _)) = self.drag.filter(|d| d.3 != cursor[0]) {
            self.drag = Some((p, k, id, cursor[0]));
            self.slider(k, id, ((cursor[0] - p[0]) / (p[2] - p[0]).max(1.0)).clamp(0.0, 1.0));
        }
        // setUpInbuilding on a panel change: the auto-trade view closes (applied, trading resumed), the trading panel's
        // error line resets; ButtonBuySell's refusal codes 9, 10, 13..18 -> TEXT_IN_TRADEPOST_010.. (else the Shift hint)
        let panel = (st.app_mode, st.app_sub_mode);
        if panel != self.last_panel {
            if self.auto.take().is_some() { self.actions.extend([[1054, 0, 0], [1050, 0, 0]]); }
            self.trade_err = None;
            self.last_panel = panel;
        }
        let (a, r, n) = *ACTION_RET.lock().unwrap();
        if n != self.seen_ret {
            self.seen_ret = n;
            if matches!(a, 1024 | 1025) { self.trade_err = matches!(r, 9 | 10 | 13..=18).then(|| self.text_of(&format!("TEXT_IN_TRADEPOST_{:03}", r + 1))); }
        }
        if !(st.app_mode == 14 && matches!(st.app_sub_mode, 61 | 62)) { self.confirm = None; } // the troops panel closed
        let mut tip: Option<String> = None;
        let mut rollover: Option<tooltip::Rollover> = None; // a build button / tab under the cursor (GameHoverText)
        let white = [1.0; 4];

        // IngameUIScreens MapLowerEdgeMask (under the HUD): while the camera is in the band boundsFixCamera lets it go
        // below the map's bottom edge, a full-width strip covers the void: black + UI-HUD 001 tiled 173×128 from the
        // strip's top-left (ImageBrush Viewport, TileMode Tile), clipped to the strip
        if self.edge_mask > 0.0 {
            let y0 = h - self.edge_mask * s;
            Self::fill(&mut out, [0.0, y0, w, h], [0.0, 0.0, 0.0, 1.0]);
            if let Some(&[ax, ay, aw, ah]) = self.rects.get("UI-HUD 001") {
                let (tw, th) = (173.0 * s, 128.0 * s);
                let fy = ((h - y0) / th).min(1.0);
                let mut x = 0.0;
                while x < w {
                    let fx = ((w - x) / tw).min(1.0);
                    let uv = [ax / ATLAS, (ATLAS - ay) / ATLAS, (ax + aw * fx) / ATLAS, (ATLAS - ay - ah * fy) / ATLAS];
                    Self::quad(&mut out, 0, [x, y0, x + tw * fx, y0 + th * fy], uv, white, 0.0);
                    x += tw;
                }
            }
        }

        // build-tool ghost under everything else: tile diamonds, green = can build, red = can't (+ the reason)
        if let Some(g) = ghost {
            let t = 1.0 / FONT_PX as f32;
            for (d, ok) in &g.tiles {
                let color = if *ok { [0.1, 0.9, 0.2, 0.45] } else { [1.0, 0.1, 0.1, 0.45] };
                let v = |p: [f32; 2]| Vert { pos: p, uv: [t, t], color, team: [0.0; 4] };
                out.push((1, [v(d[0]), v(d[1]), v(d[2])]));
                out.push((1, [v(d[0]), v(d[2]), v(d[3])]));
            }
            tip = g.why.as_deref().map(|k| self.text_of(k));
        }

        // MainHUD.xaml: HUD_Main, HUD_Troops or HUD_Buildings fill the bottom-right slot (GameData.cs:2285 picks)
        let troops = st.app_mode == 14 && matches!(st.app_sub_mode, 61 | 62);
        self.engi &= troops; // the siege build buttons close with the panel
        if troops {
            self.troops_panel(st, &mut out, cursor);
        } else if st.app_mode == 16 {
            self.building_panel(st, &mut out, cursor);
            if let Some(t) = self.panel_tip.take() { tip = Some(t); }
        } else {
            // ---- HUD_Main (914×166 box, bottom-right). Scroll + build buttons first, the picture frame on top.
            let o = (1920.0 - 914.0, 1080.0 - 166.0);
            let at = |ui: &Self, x: f32, y: f32| ui.br(o.0 + x, o.1 + y);
            // MainScroll: 876×103 grid at (110, 22) overflowing its 804 px slot -> left aligned, centred vertically;
            // TranslateTransform (16, -4) = rest state after the "Swish2" storyboard
            let [x0, y0] = at(self, 110.0 + 16.0, 22.0 + 20.5 - 4.0);
            self.img(&mut out, "UI-HUD 007", [x0, y0, x0 + 876.0 * self.s, y0 + 103.0 * self.s], white);
            // BuildMenuGrid: 496×90 centred in its slot -> (275, 55), inner grid -141, RenderTransform X 16 -> (150, 55)
            let (bx, by) = (150.0, 55.0);
            for i in 0..self.build.len() {
                let b = &self.build[i];
                if b.screen != self.screen || !variant_ok(b.mapper, st) { continue; }
                // explicit Width/Height; a missing one follows the sprite aspect (Image Stretch=Uniform)
                let r = self.rects.get(&b.s1).copied().unwrap_or([0.0, 0.0, 1.0, 1.0]);
                let (bw, bh) = match (b.w, b.h) { (0.0, h) => (h * r[2] / r[3], h), (w, 0.0) => (w, w * r[3] / r[2]), wh => wh };
                let (mx, my) = margin(b.mapper, st).unwrap_or((b.x, b.y));
                let [x0, y0] = at(self, bx + mx, by + my);
                let p = [x0, y0, x0 + bw * self.s, y0 + bh * self.s];
                if !mapper_on(b.mapper, st) { // locked: SpriteLocked, no hit test
                    if self.selected == Some(i) { self.selected = None; }
                    self.img_fit(&mut out, &b.s1, p, [1.0, 1.0, 1.0, 0.5]);
                    continue;
                }
                let hover = Self::inside(p, cursor);
                let sel = self.selected == Some(i);
                if sel { // ours (the game shows the placement cursor instead): the gold UI Player Border around it
                    self.img(&mut out, "UI Player Border", [p[0] - 3.0, p[1] - 3.0, p[2] + 3.0, p[3] + 3.0], [1.0; 4]);
                }
                let key = if hover || sel { &b.s2 } else { &b.s1 };
                self.img_fit(&mut out, key, p, white);
                if hover { // HUD_Main rollover: name, then up to two costs and (what the player has) (tooltip.rs)
                    crate::audio::hover(p, if b.name.starts_with("ButtonBuildSub") && !b.name.ends_with("Return") { crate::audio::ui_id::SUBMENU_HOVER } else { crate::audio::ui_id::BUILD_HOVER }, 1.0);
                    let c = st.costs.0.get(b.mapper.max(0) as usize).copied().unwrap_or_default();
                    let c = if b.mapper > 0 { c.to_vec() } else { vec![] };
                    rollover = Some(tooltip::Rollover::of(self.text_of(&b.help), &b.structure, &c, &st.resources));
                }
                self.hits.push((p, Hit::Build(i)));
            }
            // MainFrameBuildings: UI-HUD 000 (800×144), bottom-right
            let [x0, y0] = at(self, 114.0, 22.0);
            self.img(&mut out, "UI-HUD 000", [x0, y0, x0 + 800.0 * self.s, y0 + 144.0 * self.s], white);
            // BottomTabs1: StackPanel at x 118, bottom-aligned, 30×35 radio buttons with left margins from the XAML
            let mut x = 118.0;
            for t in 0..self.tabs.len() {
                x += self.tabs[t].1;
                let [x0, y0] = at(self, x, 166.0 - 35.0);
                let p = [x0, y0, x0 + 30.0 * self.s, y0 + 35.0 * self.s];
                let k = if tab_of(self.screen) == t + 1 { 2 } else if Self::inside(p, cursor) { 1 } else { 0 };
                self.img_fit(&mut out, &self.tabs[t].0[k].clone(), p, white);
                if Self::inside(p, cursor) {
                    crate::audio::hover(p, crate::audio::ui_id::TAB_HOVER[t], 1.0);
                    let help = ["TEXT_BUBBLE_HELP_TEXT_002", "TEXT_BUBBLE_HELP_TEXT_003", "TEXT_BUBBLE_HELP_TEXT_128",
                        "TEXT_BUBBLE_HELP_TEXT_004", "TEXT_BUBBLE_HELP_TEXT_215", "TEXT_BUBBLE_HELP_TEXT_214"][t];
                    rollover = Some(tooltip::Rollover { name: self.text_of(help), ..Default::default() });
                }
                self.hits.push((p, Hit::Tab(t)));
                x += 30.0;
            }
            // OST_Date: " Jul 1181 ", after the (hidden, space-keeping) editor tabs: 345 + 216 + 262 - 420 = 403
            let date = format!(" {} {} ", self.text_of(&format!("TEXT_MONTHS_{:03}", st.month as i32 + 1)), st.year);
            let [dx, dy] = at(self, 403.0, 166.0);
            let dw = self.text(&mut vec![], &date, 0.0, 0.0, 18.0, 1.0, 0.0, 0.0, white);
            Self::fill(&mut out, [dx, dy - 24.0 * self.s, dx + dw, dy], [0.0, 0.0, 0.0, 0x55 as f32 / 255.0]);
            self.text_sh(&mut out, &date, dx, dy - 5.0 * self.s, 18.0, 0.0, CREAM);
            // SideTabs (HUD_Main.xaml: a right-aligned stack at margin 4,22 of 20 px buttons with right margin 262, so
            // x 632): options, briefing (ButtonGoToBriefingCommand: main.rs opens the mission's briefing), delete tool
            // (pressed look while held), undo (GameAction Undo 1036).
            let del = self.build.iter().position(|b| b.screen == 0 && b.mapper == 0x27);
            let tabs = [(38.0, 36.0, "UI-Buttons A033", "UI-Buttons A034", Some(Hit::Options)), (76.0, 28.0, "UI-Buttons A037", "UI-Buttons A038", Some(Hit::Briefing)),
                (109.0, 21.0, "UI-Buttons A041", "UI-Buttons A042", del.map(Hit::Build)), (136.0, 22.0, "UI-Buttons A045", "UI-Buttons A046", Some(Hit::Act(1036, 0, 0)))];
            for (y, bh, s1, s2, hit) in tabs {
                let held = del.is_some() && self.selected == del && matches!(hit, Some(Hit::Build(_)));
                let (s1, s2) = if held { ("UI-Buttons A043", "UI-Buttons A043") } else { (s1, s2) };
                let [x0, y0] = at(self, 632.0, y);
                self.button(&mut out, [x0, y0, x0 + 20.0 * self.s, y0 + bh * self.s], s1, s2, false, cursor, hit);
            }
            let tab = at(self, 553.0, 166.0 - 35.0);
            self.allies_tab(st, &mut out, cursor, tab); // ButtonAllies (overlays.rs)
            let [x0, y0] = at(self, 0.0, 0.0);
            self.hits.insert(0, ([x0, y0, w, h], Hit::Panel));
        }

        // ---- Reports book (MainHUD.xaml "Reports Book"): scribe C001 holds the book, values are printed on its pages
        // ScribeImage 202×207, margin 0,0,-73,-10 (73 px past the right edge, as in the game): Stretch=Uniform draws
        // the sprite at its fitted size flush right / bottom in that box (WPF aligns the image's render size)
        if let Some(&[_, _, sw, sh]) = self.rects.get("UI-Scribe C001") {
            let k = (202.0 / sw).min(207.0 / sh);
            let [x0, y0] = self.br(1920.0 + 73.0 - sw * k, 1080.0 + 10.0 - sh * k);
            self.img(&mut out, "UI-Scribe C001", [x0, y0, x0 + sw * k * self.s, y0 + sh * k * self.s], white);
        }
        self.book(&mut out, cursor); // the book button (reports.rs)
        // the text grid: 200 wide, right margin 26 (x 1694..1894), bottom-aligned, 95 high (popularity 50 + 45) -> top 985
        // popularity: 26 px, Center in x 1834..1894, top 985, box 50 high; skew -18° about (0.5, 0.25) of the box
        let pop_col = if st.popularity >= 50 { GREEN } else { RED };
        let ptxt = st.popularity.to_string();
        let tw = self.tw(&ptxt, 26.0);
        self.text_xf(&mut out, &ptxt, 1864.0 - tw / 2.0, 985.0, 26.0, [1864.0, 985.0 + 12.5], -18.0, [1.0, 1.0], pop_col);
        let arrow = match st.upcoming_total_popularity { 1.. => Some("Pop_Arrow_Positive"), ..=-1 => Some("Pop_Arrow_Negative"), 0 => None };
        if let Some(a) = arrow {
            let [ax, ay] = self.br(1879.0, 987.0);
            self.img_fit(&mut out, a, [ax, ay, ax + 15.0 * self.s, ay + 25.0 * self.s], white);
        }
        // gold: TextBlock 50×18, text right-aligned, box right / bottom margin 21 / 53 (BookGoldSmall above 9999: 20 /
        // 52, 13 px); skew -25° then scale 0.8×1.2 about the box's top-left. ponytail: FontWeight Bold not synthesised
        let gold = if st.gold > 99999 { "99999+".to_string() } else { st.gold.to_string() };
        let (gs, gr, gb) = if st.gold > 9999 { (13.0, 20.0, 52.0) } else { (16.0, 21.0, 53.0) };
        let (bx, by) = (1894.0 - gr - 50.0, 1080.0 - gb - 18.0);
        let tw = self.tw(&gold, gs);
        self.text_xf(&mut out, &gold, bx + 50.0 - tw, by, gs, [bx, by], -25.0, [0.8, 1.2], GREEN);
        // population / housing: 16 px, auto-sized box Center in x 1816..1894, bottom 1048; same transform as gold
        let pop = format!("{}/{}", if st.housing_cap == 0 { 0 } else { st.population }, st.housing_cap);
        let lh = self.font.horizontal_line_metrics(16.0).map_or(20.0, |m| m.new_line_size);
        let tw = self.tw(&pop, 16.0);
        let (bx, by) = (1855.0 - tw / 2.0, 1048.0 - lh);
        self.text_xf(&mut out, &pop, bx, by, 16.0, [bx, by], -25.0, [0.8, 1.2], if st.overcrowding_popularity == 0 { GREEN } else { RED });

        // ---- Goods bar (HUD_Goods.xaml, 400×120, bottom-left, shown state): icon 25 px + amount per good; the
        // objectives popup takes its place (SetGoodsPopupState(false))
        let [x0, y0] = self.bl(0.0, 1080.0 - 120.0);
        let goods = !(self.goals && st.ev.has_goals());
        if goods { Self::fill(&mut out, [x0, y0, x0 + 400.0 * self.s, h], [0.0, 0.0, 0.0, 0x77 as f32 / 255.0]); }
        for g in (0..self.goods.len()).filter(|_| goods) {
            let (idx, left, bottom, icon) = self.goods[g].clone();
            let [gx, gy] = self.bl(left, 1080.0 - bottom - 50.0); // 50×50 cell
            let ip = [gx + 12.5 * self.s, gy - 5.0 * self.s, gx + 37.5 * self.s, gy + 20.0 * self.s];
            self.img_fit(&mut out, &icon, ip, white);
            self.text_sh(&mut out, &st.resources[idx].to_string(), gx + 25.0 * self.s, gy + 44.0 * self.s, 16.0, 0.5, CREAM);
            if Self::inside([gx, gy, gx + 50.0 * self.s, gy + 50.0 * self.s], cursor) {
                tip = Some(self.text_of(&format!("TEXT_GOODS_{:03}", idx + 1)));
            }
        }
        self.objectives(st, &mut out, cursor);

        // ---- radar (MainHUD.xaml RadarMapGrid: 128² at RadarMargin 0,0,103,8, image 124² inset 2,-4): HUD texture 2,
        // stored bottom row first (FUN_1800f4bb0), so v runs bottom-up
        if self.radar {
            let [rx, ry] = self.br(1920.0 - 103.0 - 128.0 + 2.0, 1080.0 - 8.0 - 128.0 - 2.0);
            self.radar_at = [rx, ry, rx + 124.0 * self.s, ry + 124.0 * self.s];
            Self::quad(&mut out, 2, self.radar_at, [0.0, 1.0, 1.0, 0.0], white, 0.0);
            // the engine's video over the minimap (MainHUD RadarME; video.rs): a building panel's, an advisor's
            crate::video::radar(&mut out, &st.ev.video, self.radar_at, matches!(st.lord_type, 1 | 2 | 6 | 7));
            self.hits.push((self.radar_at, Hit::Radar));
        }

        self.overlays(st, &mut out, cursor, troops);
        if let Some(t) = self.panel_tip.take() { tip = Some(t); }
        self.message_bar(st, &mut out);

        // ---- tooltip (GameHoverText) + replay tick, for checking values against the recording
        if let Some(m) = &self.message { tip = Some(m.clone()); }
        if let (None, Some(r)) = (&tip, &rollover) { self.draw_rollover(&mut out, r, st.lord_type); }
        if let Some(t) = tip {
            let tw = self.text(&mut vec![], &t, 0.0, 0.0, 18.0, 1.0, 0.0, 0.0, white);
            let (tx, ty) = (cursor[0].min(w - tw - 8.0), cursor[1] - 14.0);
            Self::fill(&mut out, [tx - 4.0, ty - 22.0 * self.s, tx + tw + 4.0, ty + 6.0 * self.s], [0.0, 0.0, 0.0, 0.6]);
            self.text_sh(&mut out, &t, tx, ty, 18.0, 0.0, CREAM);
        }
        self.on_screen_text(st, &mut out); // OnScreenText (ost.rs)
        let label = format!("tick {tick}");
        self.text_sh(&mut out, &label, w - 8.0, 22.0, 18.0, 1.0, CREAM);
        if let Some(checked) = self.confirm { self.confirm_popup(&mut out, checked, cursor); }
        self.rclick_draw(&mut out);
        out
    }

    /// HUD_RightClick.Open at window px `at`: all four icons, the compass of rotation `rot`.
    pub fn rclick_open(&mut self, at: [f32; 2], rot: i16) {
        self.rclick = Some(RightClick { at, centre: at, taken: false, rot_at: None, flat_held: false, show: [true; 4], rot, zoomed: false, flat: self.flat });
    }

    /// HUD_RightClick.Update while the right button is held: past 20 px (device px, like Input.mousePosition) the
    /// direction picks one action: right = zoom out (looping), left = UI on / off, down = flatten (on / off by how far
    /// down while held), up = RotateMapRight (again every second). `rot` = the map's rotation now; `paused` blocks
    /// flatten and rotate (Director.Paused).
    pub fn rclick_update(&mut self, cursor: [f32; 2], rot: i16, paused: bool) -> Option<RcAction> {
        let a = self.rclick_step(cursor, rot, paused);
        if let Some(RcAction::Flatten(f)) = a { self.flat = f; }
        a
    }

    fn rclick_step(&mut self, cursor: [f32; 2], rot: i16, paused: bool) -> Option<RcAction> {
        let px = 1.0 / crate::ux::dpr();
        let hidden = &mut self.hidden;
        let r = self.rclick.as_mut()?;
        let (dx, dy) = (cursor[0] - r.centre[0], cursor[1] - r.centre[1]); // dy > 0: below the press
        let second = std::time::Duration::from_secs(1);
        if !r.taken {
            if dx.abs() <= 20.0 * px && dy.abs() <= 20.0 * px { return None; }
            if dx.abs() > dy.abs() {
                r.taken = true;
                if dx > 0.0 {
                    (r.show, r.zoomed) = ([false, false, true, false], true);
                    return Some(RcAction::ZoomOut);
                }
                *hidden = !*hidden; // MainControls.toggleUIVisibility
                r.show = [false, true, false, false];
                return Some(RcAction::ToggleUi);
            }
            if paused { return None; }
            r.taken = true;
            if dy > 0.0 {
                (r.flat, r.flat_held, r.show) = (!r.flat, true, [false, false, false, true]);
                if !r.flat { r.centre = [cursor[0], cursor[1] + 60.0 * px]; } // turned off: on again 20 px past that
                return Some(RcAction::Flatten(r.flat));
            }
            (r.rot, r.rot_at, r.show) = ((rot + 6) % 8, Some(std::time::Instant::now() + second), [true, false, false, false]);
            return Some(RcAction::RotateRight);
        }
        if let Some(t) = r.rot_at {
            if std::time::Instant::now() <= t { return None; }
            (r.rot, r.rot_at) = ((rot + 6) % 8, Some(std::time::Instant::now() + second));
            return Some(RcAction::RotateRight);
        }
        if !r.flat_held { return None; }
        let want = if dy.abs() < 10.0 * px || dy < 0.0 { false } else if dy.abs() > 20.0 * px { true } else { return None };
        if want == r.flat { return None; }
        r.flat = want;
        Some(RcAction::Flatten(want))
    }

    /// HUD_RightClick.xaml: a 190×160 box centred on the press, Rotation 71×56 top centre, UI 60×45 left middle,
    /// Zoom 76×75 right middle, Flatten 62×45 bottom centre (images Stretch=Fill); after a pick only its icon stays.
    pub(crate) fn rclick_draw(&self, out: &mut Vec<HudTri>) {
        let Some(r) = &self.rclick else { return };
        let s = self.s;
        let o = [r.at[0] - 95.0 * s, r.at[1] - 80.0 * s];
        let compass = match r.rot { 2 => "floats 041", 4 => "floats 040", 6 => "floats 039", _ => "floats 038" }; // GameSprites 93..96
        let icons = [
            (compass, [59.5, 0.0, 71.0, 56.0]),
            (if self.hidden { "floats 128" } else { "floats 127" }, [0.0, 57.5, 60.0, 45.0]), // 100 / 101
            (if r.zoomed { "floats 049" } else { "floats 046" }, [114.0, 42.5, 76.0, 75.0]), // 99 / 97
            (if r.flat { "floats 037" } else { "floats 036" }, [64.0, 115.0, 62.0, 45.0]),   // 92 / 91
        ];
        for (_, (key, [x, y, iw, ih])) in icons.into_iter().enumerate().filter(|(k, _)| r.show[*k]) {
            let p = [o[0] + x * s, o[1] + y * s];
            self.img(out, key, [p[0], p[1], p[0] + iw * s, p[1] + ih * s], [1.0; 4]);
        }
    }
}

/// What the right-click menu asks main.rs to do (GameMap.RotateMapRight, PerfectPixelWithZoom.adjustZoom(-0.5, loop),
/// MainControls.toggleUIVisibility (already flipped in `Ui::hidden`), EngineInterface flattened landscape on / off).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RcAction { RotateRight, ZoomOut, ToggleUi, Flatten(bool) }

/// HUD_RightClick's state: where it opened, the press point the moves are measured from, whether an action was
/// taken, the held rotation's next repeat, flatten held, which icons show, the compass / zoom / flatten images.
#[derive(Clone, Debug)]
pub struct RightClick { at: [f32; 2], centre: [f32; 2], taken: bool, rot_at: Option<std::time::Instant>, flat_held: bool, show: [bool; 4], rot: i16, zoomed: bool, flat: bool }

#[test]
fn decode_play_state() {
    let mut b = vec![0u8; 2824];
    b[15 * 4..15 * 4 + 4].copy_from_slice(&7i32.to_le_bytes());
    b[108..112].copy_from_slice(&91i32.to_le_bytes());
    b[116..120].copy_from_slice(&1588i32.to_le_bytes());
    b[1430..1432].copy_from_slice(&6i16.to_le_bytes());
    b[1432..1434].copy_from_slice(&1181i16.to_le_bytes());
    let mut sel = vec![];
    for (id, t) in [(1, 22), (2, 70), (3, 22)] { sel.extend([id, t].map(i32::to_le_bytes).concat()); }
    let s = HudState::decode(&b, &sel).unwrap();
    assert_eq!((s.resources[15], s.popularity, s.gold, s.month, s.year), (7, 91, 1588, 6, 1181));
    assert_eq!(s.troops, vec![(22, 2), (70, 1)]);
    assert!(HudState::decode(&b[..100], &[]).is_none());
}

/// OST text: no "Game Speed" for the first speed seen or for fast-forward, then for a change; "Game Paused" while paused.
#[test]
fn ost_speed_and_pause() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let mut st = HudState { speed: 40.0, ..Default::default() };
    let mut n = |st: &HudState| u.draw(st, 1920.0, 1080.0, [0.0; 2], 0, None).len();
    let base = n(&st);
    st.speed = f32::INFINITY;
    assert_eq!(n(&st), base, "fast-forward shows no speed");
    st.speed = 45.0;
    let speed = n(&st);
    assert!(speed > base, "speed change: no text");
    st.game_paused = 1;
    let paused = n(&st);
    assert!(paused > speed, "paused: no text");
    st.end = Some(Default::default());
    assert!(n(&st) < paused, "the score screen's pause shows no text");
}

/// OST_Message_Bar (MainHUD.xaml, margin 0,0,70,160, text MaxWidth 350): a long speech wraps and its box stays inside
/// the window, its right edge 70 design px in, its bottom on the HUD's top rim (160 vs the 166 px HUD_Main).
#[test]
fn message_bar_on_screen() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let st = HudState::default();
    let a = 0x88 as f32 / 255.0;
    for (w, h) in [(1728.0, 1117.0), (1684.0, 784.0), (1920.0, 1080.0), (1366.0, 768.0)] {
        u.bar = Some(("But she's a terrible ruler and everybody knows it, so my banner will fly atop her keep before this day is out.".into(), std::time::Instant::now()));
        let out = u.draw(&st, w, h, [0.0; 2], 0, None);
        let p: Vec<[f32; 2]> = out.iter().filter(|(t, v)| *t == 1 && v[0].color == [0.0, 0.0, 0.0, a]).flat_map(|(_, v)| v.map(|q| q.pos)).collect();
        assert!(!p.is_empty(), "{w}×{h}: no bar");
        let (x0, x1) = p.iter().fold((f32::MAX, f32::MIN), |m, q| (m.0.min(q[0]), m.1.max(q[0])));
        let (y0, y1) = p.iter().fold((f32::MAX, f32::MIN), |m, q| (m.0.min(q[1]), m.1.max(q[1])));
        let s = u.s;
        assert!(x0 > 0.0 && y0 > 0.0, "{w}×{h}: bar off the window ({x0}, {y0})");
        assert!((x1 - (w - 70.0 * s)).abs() < 0.5 && (y1 - (h - 160.0 * s)).abs() < 0.5, "{w}×{h}: bar at ({x1}, {y1})");
        assert!(x1 - x0 <= 370.0 * s + 0.5 && y1 - y0 > 40.0 * s, "{w}×{h}: not wrapped ({}×{})", x1 - x0, y1 - y0);
    }
}

/// The 2×2 texels bilinear filtering reads for `Ui::fill` (uv (1, 1) / FONT_PX) are all opaque white.
#[test]
fn fill_texels_white() {
    let mut a = vec![0u8; (FONT_PX * FONT_PX * 4) as usize];
    white_block(&mut a);
    for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] { assert_eq!(a[((y * FONT_PX + x) * 4 + 3) as usize], 255, "texel {x},{y}"); }
}

/// FatControler's recruit buttons: hidden when the panel doesn't offer the troop, locked without a free peasant
/// (two for the heavy camel), the gold or the kit.
#[test]
fn recruit_locks() {
    let mut st = HudState { peasants: 1, troop_avail: [1; 8], bed_avail: [1; 8], ..Default::default() };
    st.resources[15] = 30; // gold
    st.resources[17] = 1; // bows
    assert_eq!(recruit_state(22, &st), Some(true), "archer: bow + 12 gold");
    assert_eq!(recruit_state(24, &st), Some(false), "spearman: no spear");
    st.resources[22] = 1;
    st.resources[24] = 1;
    assert_eq!(recruit_state(27, &st), Some(false), "swordsman: 40 gold > 30");
    st.resources[15] = 100;
    assert_eq!((recruit_state(27, &st), recruit_state(28, &st)), (Some(true), Some(false)), "knight needs a horse");
    assert_eq!(recruit_state(83, &st), Some(false), "heavy camel: two peasants");
    assert_eq!(recruit_why(83, &st), Some(Some("TEXT_FEEDBACK_020")));
    st.resources[15] = 10;
    assert_eq!(recruit_why(22, &st), Some(Some("TEXT_FEEDBACK_004")), "archer 12 gold > 10");
    st.troop_avail[0] = 0;
    assert_eq!(recruit_state(22, &st), None, "not offered: hidden");
}


/// UI events from a tick's records: a 0x14 lord speech becomes the message bar's text, 0x0c lines the objectives
/// (target / current split in thousands, −9999 = no current amount), line −2 the timer; the next tick's lines
/// replace them, the message stays until another one comes.
#[test]
fn ui_events() {
    let rec = |f: &[(usize, i16)]| { let mut r = [0i16; 21]; for &(k, v) in f { r[k] = v; } r };
    let speech = rec(&[(0, 0x14), (11, 0xdd), (12, 5), (13, 1), (14, 1)]);
    let gold = rec(&[(0, 0xc), (5, 4), (9, 0), (18, 1), (15, 500), (19, 0), (16, 250)]); // acquire gold 1500 (250)
    let lord = rec(&[(0, 0xc), (5, 3), (9, 1), (17, 5), (19, -9), (16, -999)]); // kill all lords, done
    let timer = rec(&[(0, 0xc), (5, -2), (8, 1), (9, 2345), (17, 2), (15, 0), (19, 0), (16, 0x19)]);
    let mut ev = Events::default();
    ev.scan(&[[0; 21], speech, gold, lord, timer, rec(&[(0, 0x14)])]); // kind 0: greeting, sound only
    let (kind, id, n) = ev.message.unwrap();
    assert_eq!((kind, id), (0xdd, 5));
    assert_eq!(ev.goals, vec![Goal { id: 4, complete: 0, kind: 0, target: 1500, current: 250 }, Goal { id: 3, complete: 1, kind: 5, target: 0, current: -9999 }]);
    assert_eq!(ev.timer, Some([12345, 20000, 0x19]));
    ev.scan(&[gold]);
    assert_eq!((ev.goals.len(), ev.timer, ev.message.map(|m| m.2)), (1, None, Some(n)), "lines per tick, message kept");
    let st = HudState { ev, ..Default::default() };
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    u.draw(&st, 1920.0, 1080.0, [0.0; 2], 0, None);
    assert_eq!(u.bar.as_ref().map(|b| b.0.as_str()), Some("The world has gone crazy... I, I, I should have destroyed you."));
    assert_eq!(u.goal_row(&st.ev.goals[0], false), ("Acquire Gold:".into(), "1500 (250)".into(), false));
    assert_eq!(u.goal_row(&Goal { id: 3, complete: 1, kind: 5, target: 0, current: -9999 }, false), ("Kill All Enemy Lords".into(), "".into(), true));
}


/// Building panels through the real HUD path: the building opened by a map click (cursor tile on it, DLL_TroopSelection
/// press + release), the panel drawn at 1684×784, its buttons clicked by position (`Ui::click`), the sub-mode /
/// actions / delete sent the way main.rs sends them. Market: post (25) -> resources (55) -> wood (57) -> sell, buy;
/// stockpile: Delete.
#[cfg(feature = "live")]
#[test]
fn panel_buttons_via_hud() {
    use crate::live::{Live, TroopIn};
    use shc_sim::buildings as b;
    let save = crate::paths::test_save("Crater Lake-1test.sav");
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    if !std::path::Path::new(&save).exists() { return; }
    let mut l = Live::load(&save);
    l.step();
    let find = |l: &mut Live, kind: i32| (1..b::SLOTS).find(|&i| b::state(l.mem(), i) == 2 && b::owner(l.mem(), i) == 1 && b::kind(l.mem(), i) == kind);
    let open = |l: &mut Live, id: i32| {
        let m = l.mem();
        let (r, n) = (b::rec(id), m.i32(b::rec(id) + 0xf8));
        l.mouse = [m.i16(r + 0xee) as i32 + n / 2, m.i16(r + 0xf0) as i32 + n / 2];
        crate::military::input(l, TroopIn::Press([0.0; 2], [0.0; 2], [0.0; 4]));
        l.step();
        crate::military::input(l, TroopIn::Release);
        l.step();
        l.hud()
    };
    let press = |u: &mut Ui, l: &mut Live, h: &HudState, want: Hit| {
        u.draw(h, 1684.0, 784.0, [0.0; 2], 0, None);
        let p = u.hits.iter().rev().find(|(_, hit)| *hit == want).map(|e| e.0).expect("button not drawn");
        assert!(u.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]), "click not taken");
        for a in u.actions.drain(..) { l.game_action(a[0], a[1], a[2]); } // main.rs: actions, then the sub-mode
        if let Some(m) = u.sub_mode.take() { l.set_app_mode(16, m); }
        if let Some(id) = u.delete.take() { l.delete_building(id); }
        l.step();
        l.hud()
    };
    // market: placed with the market tool beside the stockpile, opened, wood sold and bought back
    let pile = find(&mut l, 0xa).expect("no stockpile");
    let (r, n) = (b::rec(pile), l.mem().i32(b::rec(pile) + 0xf8));
    let (px, py) = (l.mem().i16(r + 0xee) as i32 + n / 2, l.mem().i16(r + 0xf0) as i32 + n / 2);
    let before: Vec<i32> = (1..b::SLOTS).filter(|&i| b::state(l.mem(), i) != 0).collect();
    assert!(shc_sim::campaign::bot::place(l.mem(), 0x4d, px, py), "market not placed");
    l.step();
    let market = (1..b::SLOTS).find(|&i| b::state(l.mem(), i) != 0 && !before.contains(&i)).expect("no market");
    let h = open(&mut l, market);
    assert_eq!((h.app_mode, h.app_sub_mode), (16, 25), "market click: not the trading post");
    let h = press(&mut u, &mut l, &h, Hit::TradeType(2));
    assert_eq!(h.app_sub_mode, 55, "resources list");
    let h = press(&mut u, &mut l, &h, Hit::TradeGood(market, 2));
    assert_eq!((h.app_sub_mode, h.trading[0]), (57, 2), "wood trading panel");
    let (wood, gold, n) = (h.resources[2], h.gold, h.trade_sell_n[2] as i32);
    let h = press(&mut u, &mut l, &h, Hit::Trade(1025, 2));
    println!("sell wood: wood {wood} -> {}, gold {gold} -> {} (lot {n})", h.resources[2], h.gold);
    assert!(n > 0 && h.resources[2] == wood - n && h.gold > gold, "sell");
    let (wood, buy) = (h.resources[2], h.trade_buy_n[2] as i32);
    let h = press(&mut u, &mut l, &h, Hit::Trade(1024, 2));
    assert!(buy > 0 && h.resources[2] == wood + buy, "buy: {wood} + {buy} -> {}", h.resources[2]);
    // the demolish tool: SideTabs' delete button (HUD_Main shows again once the panel is closed), then a click on the
    // market the way main.rs sends it (Cmd::Tool, Cmd::Click)
    for down in [true, false] { crate::military::input(&mut l, TroopIn::Right(down)); l.step(); } // a right click closes the panel
    let h = l.hud();
    assert_ne!(h.app_mode, 16, "panel still open");
    u.draw(&h, 1684.0, 784.0, [0.0; 2], 0, None);
    let del = u.build.iter().position(|b| b.mapper == 0x27).unwrap();
    let p = u.hits.iter().rev().find(|(_, hit)| *hit == Hit::Build(del)).map(|e| e.0).expect("delete tool not drawn");
    assert!(u.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]) && u.selected_mapper() == Some(0x27), "delete tool not picked");
    assert!(l.set_tool(0x27), "delete tool refused");
    let (m, n) = (b::rec(market), l.mem().i32(b::rec(market) + 0xf8));
    let (mx, my) = (l.mem().i16(m + 0xee) as i32 + n / 2, l.mem().i16(m + 0xf0) as i32 + n / 2);
    let (v, chores) = l.click(0x27, mx, my);
    l.step();
    assert!(chores > 0 && b::state(l.mem(), market) != 2, "demolish: {v:?}, {chores} chore(s), market state {}", b::state(l.mem(), market));
    l.set_tool(0);
    // stockpile: the panel's Delete
    let h = open(&mut l, pile);
    assert_eq!(h.app_mode, 16, "stockpile panel not open");
    press(&mut u, &mut l, &h, Hit::Delete(pile));
    assert_ne!(b::state(l.mem(), pile), 2, "stockpile still standing");
}

/// The troops panel's type buttons: a left click keeps only that type (1022), a right click drops it (1021).
#[test]
fn troop_type_filter() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let st = HudState { app_mode: 14, app_sub_mode: 61, troops: vec![(22, 5), (24, 7)], ..Default::default() };
    u.draw(&st, 1920.0, 1080.0, [0.0; 2], 0, None);
    let p = u.hits.iter().rev().find(|(_, h)| *h == Hit::Act(1022, 0, 24)).map(|e| e.0).expect("spearman button not drawn");
    let c = [(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0];
    u.actions.clear();
    assert!(u.click(c));
    assert_eq!(std::mem::take(&mut u.actions), vec![[1022, 0, 24]]);
    assert!(u.rclick(c));
    assert_eq!(std::mem::take(&mut u.actions), vec![[1021, 0, 24]]);
    assert!(!u.rclick([1.0, 1.0]), "a right press off the buttons is not the HUD's");
}

/// HUD_Troops.SetuptroopActionsUI's order buttons by the engine's flags, and the recruit amounts (FatControler:
/// lastTroopsAmountToMakeMax; MakeTroop s = 1 / Shift 5 / Ctrl 1000).
#[test]
fn troop_orders_and_recruit_amounts() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let mut st = HudState { app_mode: 14, app_sub_mode: 61, ..Default::default() };
    let has = |u: &mut Ui, st: &HudState, h: Hit| { u.draw(st, 1920.0, 1080.0, [0.0; 2], 0, None); u.hits.iter().any(|e| e.1 == h) };
    let (attack, reload, cow) = (Hit::Act(1012, 0, 0), Hit::Act(1041, 0, 0), Hit::Act(1013, 0, 0));
    assert!(!has(&mut u, &st, attack) && !has(&mut u, &st, reload) && !has(&mut u, &st, cow), "no orders without the flags");
    for ty in 1..=3 { st.attack_here = ty; assert!(has(&mut u, &st, attack), "attack-here type {ty}"); }
    st.get_ammo = 1;
    assert!(has(&mut u, &st, reload), "reload");
    st.cows = 250;
    assert!(!has(&mut u, &st, cow), "250 cows: hidden");
    st.cows = 3;
    assert!(has(&mut u, &st, cow), "cows");
    // 10 free peasants, 100 gold, 4 bows / 9 spears / 2 leather jerkins
    let mut st = HudState { peasants: 10, gold: 100, ..Default::default() };
    (st.resources[17], st.resources[19], st.resources[21], st.resources[23]) = (4, 9, 5, 2);
    assert_eq!((recruit_max(22, &st), recruit_max(24, &st), recruit_max(26, &st)), (4, 9, 2)); // bows, 100 / 8 gold → peasants, jerkins
    for (shift, ctrl, s) in [(false, false, 1), (true, false, 5), (false, true, 1000)] {
        (u.shift, u.ctrl) = (shift, ctrl);
        u.hits = vec![([0.0, 0.0, 10.0, 10.0], Hit::Recruit(22, None))];
        u.actions.clear();
        assert!(u.click([5.0, 5.0]));
        assert_eq!(u.actions, vec![[1007, s, 22]], "shift {shift} ctrl {ctrl}");
    }
}

/// The building panels' buttons by position (draw, then `Ui::click` on the drawn rect): gate / drawbridge states,
/// workshop output (Shift = all), outpost toggles and sliders, the siege tent's progress line, the price list, the
/// auto-trade view (open, slider, leaving applies it), the trade error line from the action's result, the briefing tab.
#[test]
fn building_panel_buttons() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let press = |u: &mut Ui, st: &HudState, want: Hit| -> Vec<[i32; 3]> {
        u.draw(st, 1684.0, 784.0, [0.0; 2], 0, None);
        u.actions.clear();
        let p = u.hits.iter().rev().find(|(_, h)| *h == want).map(|e| e.0).unwrap_or_else(|| panic!("{want:?} not drawn"));
        assert!(u.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]));
        std::mem::take(&mut u.actions)
    };
    let mut st = HudState { app_mode: 16, in_structure: 7, ..Default::default() };
    for (sub, a) in [(36, 1001), (37, 1008)] {
        st.app_sub_mode = sub;
        assert_eq!(press(&mut u, &st, Hit::Act(a, 7, 10)), vec![[a, 7, 10]]);
        assert_eq!(press(&mut u, &st, Hit::Act(a, 7, 11)), vec![[a, 7, 11]]);
    }
    (st.app_sub_mode, st.can_make) = (14, [1; 6]);
    assert_eq!(press(&mut u, &st, Hit::Workshop(7, 21)), vec![[1023, 7, 21]]);
    u.shift = true;
    assert_eq!(press(&mut u, &st, Hit::Workshop(7, 22)), vec![[1023, -1, 22]], "Shift: every blacksmith");
    u.shift = false;
    (st.app_sub_mode, st.in_structure_type) = (45, 106);
    assert_eq!(press(&mut u, &st, Hit::Act(1055, 7, 3)), vec![[1055, 7, 3]]);
    let a = press(&mut u, &st, Hit::Slider(2, 7));
    assert!(matches!(a[..], [[1056, 7, 2 | 3]]), "size slider clicked in the middle: {a:?}");
    st.ai_clock = 640;
    assert_eq!(u.info_text(&st, 60, 81).as_deref(), Some("50% Complete"), "siege tent progress");
    // market: the price list from the post, a good from it, the auto view, its sell slider, back = apply + resume
    (st.app_sub_mode, st.in_structure_type) = (25, 26);
    press(&mut u, &st, Hit::TradeType(4));
    assert_eq!(u.sub_mode.take(), Some(53));
    st.app_sub_mode = 53;
    st.trade_buy_n[2] = 5;
    press(&mut u, &st, Hit::TradeGood(7, 2));
    (st.app_sub_mode, st.trading) = (57, [2, 4, 24]);
    st.autotrade = [{ let mut a = [0; 25]; a[2] = 50; a }, { let mut a = [0; 25]; a[2] = 300; a }];
    assert_eq!(press(&mut u, &st, Hit::Auto(0, 7, 2)), vec![[1051, 2, 0], [1053, -1, 300], [1052, -1, 50], [1050, 1, 1]]);
    assert_eq!(u.auto, Some(AutoTrade { on: false, buy: 36, sell: 65 }));
    let a = press(&mut u, &st, Hit::Slider(0, 7));
    assert_eq!(a.last(), Some(&[1051, -1, 1]), "a slider turns auto trading on: {a:?}");
    assert_eq!(press(&mut u, &st, Hit::Auto(2, 7, 2)), vec![[1054, 0, 0], [1050, 0, 0]]);
    assert!(u.auto.is_none());
    // a refused Buy: the sim thread's result -> "Not enough gold"
    { let mut r = ACTION_RET.lock().unwrap(); *r = (1024, 9, r.2 + 1); }
    u.draw(&st, 1684.0, 784.0, [0.0; 2], 0, None);
    assert_eq!(u.trade_err.as_deref(), Some("Not enough gold"));
    // HUD_Main's briefing tab
    let st = HudState::default();
    press(&mut u, &st, Hit::Briefing);
    assert!(std::mem::take(&mut u.open_briefing));
}

/// Disband asks first (Options > Confirm Disband Troops, on by default): No sends nothing, Yes sends Troops_Disband.
#[test]
fn disband_confirmation() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    if !crate::skirmish::Settings::load().flag("Confirm_Disband_Troops", true) { return; } // the player turned it off
    let st = HudState { app_mode: 14, app_sub_mode: 61, troops_show_disband: 1, troops: vec![(22, 3)], ..Default::default() };
    let press = |u: &mut Ui, want: Hit| {
        u.draw(&st, 1684.0, 784.0, [0.0; 2], 0, None);
        let p = u.hits.iter().rev().find(|(_, h)| *h == want).map(|e| e.0).unwrap_or_else(|| panic!("{want:?} not drawn"));
        u.click([(p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0]);
    };
    press(&mut u, Hit::Order(0));
    assert!(u.confirm.is_some() && u.actions.is_empty(), "popup first");
    press(&mut u, Hit::Confirm(0));
    assert!(u.confirm.is_none() && u.actions.is_empty(), "No");
    press(&mut u, Hit::Order(0));
    press(&mut u, Hit::Confirm(1));
    assert_eq!(u.actions, vec![[1011, 0, 0]], "Yes");
}

/// HUD_Buildings.GetBuildingShowHelp: the dog cage's (panel 89) help button goes once its dogs are out.
#[test]
fn dog_cage_help() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    if crate::menu::help::building_page(100).is_none() { return; } // no help pages exported
    let mut st = HudState { app_mode: 16, app_sub_mode: 89, in_structure_type: 100, ..Default::default() };
    let help = |u: &mut Ui, st: &HudState| { u.draw(st, 1684.0, 784.0, [0.0; 2], 0, None); u.hits.iter().any(|(_, h)| *h == Hit::Help(100)) };
    assert!(help(&mut u, &st), "closed cage: help shown");
    st.dog_cage = 1;
    assert!(!help(&mut u, &st), "dogs out: no help");
}

/// Sliders drag: pressed on the outpost size slider, the thumb follows the cursor until the button is let go.
#[test]
fn slider_drag() {
    let Some(mut u) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let st = HudState { app_mode: 16, app_sub_mode: 45, in_structure_type: 106, in_structure: 7, ..Default::default() };
    u.draw(&st, 1684.0, 784.0, [0.0; 2], 0, None);
    let p = u.hits.iter().find(|(_, h)| *h == Hit::Slider(2, 7)).unwrap().0;
    let y = (p[1] + p[3]) / 2.0;
    u.click([p[0] + 1.0, y]);
    assert_eq!(std::mem::take(&mut u.actions), vec![[1056, 7, 0]]);
    u.draw(&st, 1684.0, 784.0, [p[2] - 1.0, y], 0, None);
    assert_eq!(std::mem::take(&mut u.actions), vec![[1056, 7, 5]], "dragged to the end");
    u.drag = None; // released
    u.draw(&st, 1684.0, 784.0, [p[0] + 1.0, y], 0, None);
    assert!(u.actions.is_empty(), "released: no more changes");
}
