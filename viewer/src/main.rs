//! Minimal SHC DE sprite viewer: real BC7 atlases from the export cache, drawn as instanced quads.
mod audio;
mod cursor;
mod endgame;
mod koth; // the King of the Hill game mode (koth.rs)
mod gestures;
#[cfg(feature = "live")]
mod game_replay; // game replays (game_replay.rs)
mod saves; // save browser, quick / auto saves (saves.rs)
mod hud;
mod keys;
mod lan;
mod live;
#[cfg(target_os = "macos")]
mod macos;
mod map;
mod menu;
mod custom_lords; // DE's CustomLords / CustomMedia folders: custom AI lords (custom_lords.rs)
mod lord_share; // a custom lord's .zip import, custom lords in LAN games (lord_share.rs)
mod paths; // per-OS game / cache / data folders
mod setup; // first run: the user's own copy of the game (Steam download), exported once (setup/)
#[cfg(feature = "live")]
mod military;
mod replay;
mod skirmish;
mod ux;
mod update; // "new version available" notice (update.rs)
mod video;
mod speed; // game speeds above 90 ticks/s, one Frame per display frame (speed.rs)
mod stats; // per-player stats over time: F3 overlay + end screen (stats.rs)
mod loader; // start-up loading off the UI thread (loader.rs)
#[cfg(test)]
mod perf; // load-time / game-speed measurements
#[cfg(test)]
mod lindschied_check; // Lord David von Lindschied: his folder loads, the AI plays his castles
use std::{collections::{BTreeMap, HashMap}, ops::Range, path::PathBuf, sync::Arc, time::Instant};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key, ModifiersState, NamedKey},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Fullscreen, Window, WindowId},
};

const COLS: usize = 12;
const ROWS: usize = 7;
const CELL: f32 = 112.0;
const FPS: f32 = 12.0; // ponytail: guessed playback rate; real rate comes from the sim tick later

/// Layer-3037 ground flag -> (tile set + frame range, flat colour). ALL GUESSES, see README.
/// Layer-3037 ground flag -> (tile set, index range into its frame list, fallback colour).
/// ALL GUESSES from eyeballing GrainBasket + `--sheet` (README). land_macros comes in 152-frame terrain
/// blocks; only the first 20 of a block are used (the rest look like macro/transition pieces).
const GROUND: &[(u8, Option<(&str, Range<usize>)>, [f32; 3])] = &[
    (0, None, [0.30, 0.27, 0.24]),                                    // ? dark: hills/rock along the lake
    (1, Some(("tile_land_macros", 780..800)), [0.75, 0.35, 0.25]),    // ring around the beach: earth + tufts
    (2, Some(("tile_land_macros", 456..476)), [0.35, 0.60, 0.25]),    // grass (meadow)
    (4, Some(("tile_land_macros", 0..20)), [0.20, 0.30, 0.70]),       // scattered at edges: scrub
    (8, Some(("tile_land_macros", 152..172)), [0.78, 0.68, 0.42]),    // earth (most common)
    (16, Some(("tile_sea8", 1105..1159)), [0.15, 0.45, 0.75]),        // central lake: water
    (64, None, [1.0, 0.0, 1.0]),                                      // 6 cells, unknown
    (128, Some(("tile_land_macros", 932..952)), [0.92, 0.88, 0.70]),  // between water and ring: beach sand
];

struct Atlas { name: String, w: u32, h: u32 }
/// `mesh`: triangle list, px offsets from the pivot (y up). Rects overlap neighbours; the mesh doesn't.
struct Sprite { atlas: u32, x: f32, y: f32, w: f32, h: f32, px: f32, py: f32, mesh: Vec<[f32; 2]> }

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
/// `color.a == 0`: solid `color.rgb`, no texture (flat map tiles). Otherwise texture * color, with the atlas mask
/// area tinted by `team.rgb` (`team.a` = 0 disables).
struct Vert { pos: [f32; 2], uv: [f32; 2], color: [f32; 4], team: [f32; 4] }
type Tri = (u32, [Vert; 3]); // (atlas, triangle)

fn cache_dir() -> PathBuf {
    std::env::var_os("SHC_CACHE").map(PathBuf::from).unwrap_or_else(|| paths::cache("shc-viewer"))
}

/// The export's atlases and sprites, then the ones compiled into the app (loader::BUNDLED).
fn load_tables() -> (Vec<Atlas>, Vec<(String, Sprite)>) {
    let mut t = export_tables();
    loader::bundle(&mut t);
    t
}

/// One sprites.tsv row: name, atlas, x, y, w, h, pivot x, y (0..1 from the bottom left), mesh.
fn sprite_row(l: &str) -> (String, Sprite) {
    let c: Vec<&str> = l.split('\t').collect();
    let f = |i: usize| c[i].parse::<f32>().unwrap();
    let mesh = c[8].split(';').map(|p| {
        let (x, y) = p.split_once(',').unwrap();
        [x.parse().unwrap(), y.parse().unwrap()]
    }).collect();
    (c[0].to_string(), Sprite { atlas: c[1].parse().unwrap(), x: f(2), y: f(3), w: f(4), h: f(5), px: f(6), py: f(7), mesh })
}

fn export_tables() -> (Vec<Atlas>, Vec<(String, Sprite)>) {
    let dir = cache_dir();
    if let Some(t) = loader::tables::load(&dir) { return t; } // the parse below, cached as binary (loader.rs)
    let read = |f: &str| std::fs::read_to_string(dir.join(f))
        .unwrap_or_else(|e| panic!("{}: {e} (run re/scripts/export_atlases.py first)", dir.join(f).display()));
    let t = parse_tables(&read("atlases.tsv"), &read("sprites.tsv"));
    loader::tables::save(&dir, &t);
    t
}

/// atlases.tsv + sprites.tsv text (also a custom lord's sprite/, custom_lords::add_sprites).
fn parse_tables(atlases: &str, sprites: &str) -> (Vec<Atlas>, Vec<(String, Sprite)>) {
    let atlases = atlases.lines().map(|l| {
        let c: Vec<&str> = l.split('\t').collect();
        Atlas { name: c[1].into(), w: c[2].parse().unwrap(), h: c[3].parse().unwrap() }
    }).collect();
    let sprites = sprites.lines().map(sprite_row).collect();
    (atlases, sprites)
}

/// SHC_AI=n (screenshots): the lobby's / --skirmish's one AI is lord n (1000 = the first custom lord).
fn dev_ai() -> Option<usize> { std::env::var("SHC_AI").ok()?.parse().ok() }

/// SHC_FIGHT=n[,type[,dx,dy]] (screenshots of a lord's walk / fight / death): n of your troops (default swordsmen,
/// type 27) on player 2's keep roof at the start, waiting for his lord (placed there at ~tick 100); with dx,dy the
/// troops stand that many tiles off the keep on open ground instead (archers there draw the lord out: lord_go_keep).
#[cfg(feature = "live")]
fn dev_fight(l: &mut live::Live) {
    let Ok(v) = std::env::var("SHC_FIGHT") else { return };
    let f: Vec<i16> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    let (Some(&n), ty, (dx, dy)) = (f.first(), f.get(1).copied().unwrap_or(27), (f.get(2).copied().unwrap_or(0), f.get(3).copied().unwrap_or(0))) else { return };
    let Some((kx, ky)) = l.keep_of(2) else { eprintln!("SHC_FIGHT: player 2 has no keep"); return };
    let (x, y) = (kx + dx, ky + dy);
    use shc_sim::terrain as t;
    let m = l.mem();
    let h = m.u8(t::at(t::HEIGHT, 1, t::row_off(m, y as i32) + x as i32)) as i16;
    for k in 0..n { shc_sim::units::helpers::spawn(m, 1, 1, (x + k % 5 - 2) * 8, (y + k / 5 % 5 - 2) * 8, h, ty as i32); } // 5×5 block
    println!("SHC_FIGHT: {n} of type {ty} at {x},{y} h {h} (player 2's keep {kx},{ky})");
}

/// "body_hunter-1716x" -> ("body_hunter-x", 1716). Frames of one animation share the key.
fn split_frame(name: &str) -> Option<(String, u32)> {
    let cut = name.rfind(['-', ' '])?;
    let tail = &name[cut + 1..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n = digits.parse().ok()?;
    Some((format!("{}{}", &name[..=cut], &tail[digits.len()..]), n))
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    screen_buf: wgpu::Buffer,
    inst_buf: wgpu::Buffer,
    groups: Vec<wgpu::BindGroup>,
    static_buf: Option<(wgpu::Buffer, Vec<Run>)>,
    hud: Option<HudGfx>,
    loading: Option<loader::Atlases>, // map atlases still uploading on the worker thread (loader.rs)
}

/// HUD: screen-space uniform, UI atlas + font atlas bind groups (index = `HudTri.0`), own vertex buffer.
struct HudGfx { screen_buf: wgpu::Buffer, buf: wgpu::Buffer, groups: [wgpu::BindGroup; 8], font: wgpu::Texture, radar: wgpu::Texture, video: wgpu::Texture, screen: wgpu::Texture, small: wgpu::Texture, coa: wgpu::Texture }

/// Draw command: (iso row, atlas, vertex range). Rows interleave the static tile buffer with the object buffer.
type Run = (i32, u32, Range<u32>);

/// Consecutive runs of one (row, atlas) -> vertex range. Input must be grouped by row then atlas.
/// The engine's layer delay (k12: its tile's hill shading, up to 5 rows) sorts a unit on open ground that far forward,
/// so the original paints one standing just behind a building over the roof. Ours stops unit (x, y)'s row `sorted`
/// before the first tall tile in front that its sprite overlaps (columns ±1); a unit standing on a tall tile itself
/// (walls, keep, gatehouse) keeps the full delay. ponytail: a column band, not the sprite's real width
fn behind_tall(tall: impl Fn(i32, i32) -> bool, x: i32, y: i32, sorted: i32) -> i32 {
    if tall(x, y) { return sorted; }
    let (row, col) = (x - y, x + y);
    for r in row + 1..=sorted {
        for c in col - 1..=col + 1 {
            if (r + c) % 2 == 0 && tall((c + r) / 2, (c - r) / 2) { return r - 1; }
        }
    }
    sorted
}

#[test]
fn delayed_unit_stays_behind_tall_tiles() {
    // a building on (12, 8) (row 4, column 20); the unit two rows behind it at (10, 8) (row 2, column 18)
    let hut = |x: i32, y: i32| (x, y) == (12, 8);
    assert_eq!(behind_tall(hut, 10, 8, 2 + 5), 2 + 5, "column 18 vs 20: not in front of it");
    assert_eq!(behind_tall(hut, 11, 9, 2 + 5), 3, "column 20, delay 5: stops before the building's row 4");
    assert_eq!(behind_tall(hut, 11, 9, 2 + 1), 3, "a delay short of the building is kept");
    assert_eq!(behind_tall(hut, 12, 8, 4 + 5), 9, "standing on the tall tile: the full delay");
    assert_eq!(behind_tall(|x, y| (x, y) == (12, 9), 11, 9, 2 + 5), 2, "column 21 overlaps too: stops before row 3");
}

/// GameMap.addUpdateFly's digits of a floating number, ones first: GM 202 frames 2..11 = '0'..'9', then 12 = the
/// minus in the next slot (none at 5 digits); capped at 99999.
fn info_digits(v: i32) -> Vec<i16> {
    let mut d: Vec<i16> = v.unsigned_abs().min(99999).to_string().bytes().rev().map(|b| (b - b'0') as i16 + 2).collect();
    if v < 0 && d.len() < 5 { d.push(12); }
    d
}

#[test]
fn floating_number_digits() {
    assert_eq!(info_digits(0), [2]);
    assert_eq!(info_digits(305), [7, 2, 5]);
    assert_eq!(info_digits(-4), [6, 12]);
    assert_eq!(info_digits(123456), [11, 11, 11, 11, 11]);
    assert_eq!(info_digits(-12345), [7, 6, 5, 4, 3]);
}

fn runs(tris: &[Tri], rows: &[i32]) -> Vec<Run> {
    let mut out: Vec<Run> = vec![];
    for (i, (a, _)) in tris.iter().enumerate() {
        let (v, row) = (i as u32 * 3, rows.get(i).copied().unwrap_or(0));
        match out.last_mut() { Some((k, b, r)) if *k == row && b == a => r.end = v + 3, _ => out.push((row, *a, v..v + 3)) }
    }
    out
}

#[derive(Default)]
struct Cam { zoom: f32, off: [f32; 2], cursor: [f32; 2], drag: bool, heights: Vec<i16>, bld: Vec<i16>, use_bld: bool,
    zoom_pos: f32, zoom_anim: Option<(f32, f32, Instant)>, zoom_delay: Option<(bool, Instant)>,
    map: Option<MapBox>, keys: [bool; 4], cursor_in: bool, edge_mask: f32, bounds: Option<[f32; 5]>, ui_hidden: bool }
// map: where the camera may go (None: no limits, e.g. --map / --sheet); keys: arrow keys held (left, right, up, down);
// cursor_in: the mouse is in the window (edge scrolling); edge_mask: MapLowerEdgeMaskHeight in design px (limit);
// bounds: the window centre's travel x0, x1, y0, y1 (world px) + tilemapSize, from the last `limit`; ui_hidden: no
// HUD, so no band under it (boundsFixCamera num2 = 0)

/// The map's place for the camera limits: centre of the drawn tiles in world px (West frame of rotation `rot`) and
/// GameMap.tilemapSize (160 .. 800).
#[derive(Clone, Copy, Debug, PartialEq)]
struct MapBox { centre: [f32; 2], size: f32, rot: i16 }

impl MapBox {
    /// From the drawn tile records: their extent is (size − 4) × 32 by (size − 6) × 16 px (GameMap.getMaxWidthZoom /
    /// getMaxHeightZoom's map in pixels), so the width gives the size, rounded to the sizes maps come in.
    fn of(w: &replay::World) -> Option<Self> {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        // GM 38 = the off-map ring the draw list sends for the rest of the 800×800 grid (never drawn)
        for (i, _) in w.tiles.iter().enumerate().filter(|(_, r)| r[3] != 0 && r[3] != 38) {
            let q = replay::cell_px((i % replay::N) as i16, (i / replay::N) as i16);
            for k in 0..2 { lo[k] = lo[k].min(q[k]); hi[k] = hi[k].max(q[k]); }
        }
        if lo[0] > hi[0] { return None; }
        let n = (hi[0] - lo[0]) / 32.0 + 4.0;
        let size = [160.0f32, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0].into_iter().min_by(|a, b| (a - n).abs().total_cmp(&(b - n).abs())).unwrap();
        Some(MapBox { centre: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0], size, rot: w.rot })
    }
}
// heights: West-frame tile index -> GameMapTile.testHeight ×32 (empty = flat); bld: px a building / tall tile sprite
// stands above it (buildingHeight ×64, else sprite height − 32); both kept by tick_replay for the cursor pick.
// use_bld: CalcMapTileFromMousePos's useBuildingHeight (no build tool held, see `bld_pick`)

/// PerfectPixelWithZoom.Zoom: the scale of each zoomPos (index = zoomPos × 2); scale 1 = one game pixel per device pixel.
const ZOOM_LEVELS: [f32; 11] = [0.125, 0.175, 0.25, 0.35, 0.5, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0];

/// adjustZoom's in-game range for a `w`×`h` device-px window on a map of `size`: 1..=3, or 2..=3 when
/// CanUserExtraZoom says no (big windows; on small maps smaller ones already).
fn zoom_pos_clamp(pos: f32, w: f32, h: f32, size: f32) -> f32 {
    let (mw, mh) = match size as i32 { 300 => (2300.0, 1440.0), 200 => (1500.0, 900.0), 160 => (1300.0, 800.0), _ => (2560.0, 1440.0) };
    let extra = !(w > 2560.0 || h > 1440.0 || w > mw || h > mh);
    pos.clamp(if extra { 1.0 } else { 2.0 }, 3.0)
}

/// CameraControls2D.MoveSpeed: 16 units (1024 game px) a second at scroll speed 5 (GetScrollSpeed 1.0), whatever the
/// zoom; Shift doubles it.
const SCROLL_PX_S: f32 = 1024.0;

impl Cam {
    /// PerfectPixelWithZoom.capZoom + CameraControls2D.boundsFixCamera for a `w`×`h` logical-px window (`sf` device px
    /// each) whose HUD scale is `ui` (logical px per design px): the zoom never shows the map smaller than the window
    /// (unless that is past native size), and the window centre stays where the camera centre may go: one unit (64
    /// px) inside the map's left / right / top edge, its bottom edge down to the HUD's top line (SHLowerUIPoint =
    /// GuideRect, 125 design px), centred on an axis the map doesn't fill; plus adjustBoundsForRotation's nudges.
    fn limit(&mut self, w: f32, h: f32, sf: f32, ui: f32) {
        let Some(m) = self.map else { return };
        let centre = |c: &Cam| [(w / 2.0 - c.off[0]) / c.zoom, (h / 2.0 - c.off[1]) / c.zoom];
        let ui_dev = 125.0 * ui * sf;
        let cap = ((w * sf) / ((m.size - 4.0) * 32.0)).max((h * sf - ui_dev) / ((m.size - 6.0) * 16.0)).min(1.0);
        let mut at = centre(self);
        if self.zoom * sf < cap { self.zoom = cap / sf; }
        let (half_w, half_h) = (w / 2.0 / self.zoom, h / 2.0 / self.zoom);
        let (map_w, map_h) = (m.size * 16.0, m.size * 8.0); // half extents: num / 2, num / 4 units
        let (mut x0, mut x1) = (m.centre[0] - map_w + half_w + 64.0, m.centre[0] + map_w - half_w - 64.0);
        let band = if self.ui_hidden { 0.0 } else { 125.0 * ui / self.zoom }; // SHLowerUIPoint (GuideRect, 125 design px)
        let (mut y0, mut y1) = (m.centre[1] - map_h + half_h + 64.0, m.centre[1] + map_h - half_h - 64.0 + band);
        match m.rot {
            0 => { y0 -= 32.0; if self.zoom * sf != 1.0 { y1 += 16.0 } }
            2 => { y0 -= 32.0; x1 -= 64.0 }
            4 => { y0 -= 32.0; y1 += 16.0 }
            _ => { y0 -= 32.0; y1 += 16.0; x0 += 64.0 }
        }
        self.bounds = Some([x0, x1, y0, y1, m.size]);
        at[0] = if map_w - 128.0 < half_w { m.centre[0] } else { at[0].clamp(x0, x1) };
        at[1] = if y1 <= y0 { (y0 + y1) / 2.0 } else { at[1].clamp(y0, y1) };
        self.off = [w / 2.0 - at[0] * self.zoom, h / 2.0 - at[1] * self.zoom];
        // MapLowerEdgeMask: below bottomNoUI (the band under the HUD line) the mask grows to 125 design px
        self.edge_mask = if band > 0.0 && at[1] > y1 - band { 125.0 * ((at[1] - (y1 - band)) / band).min(1.0) } else { 0.0 };
    }

    /// KeyManager.HorizontalAxis / VerticalAxis: arrow keys held, else the mouse on a window edge (PushMapScrolling,
    /// on by default) scroll at SCROLL_PX_S device px a second for `dt` s.
    fn scroll(&mut self, w: f32, h: f32, sf: f32, shift: bool, dt: f32) {
        let k = self.keys;
        let edge = |lo: bool, hi: bool| if lo { -1.0 } else if hi { 1.0 } else { 0.0 };
        let c = self.cursor;
        let (mut dx, mut dy) = (edge(k[0], k[1]), edge(k[2], k[3]));
        if k[0] == k[1] { dx = if self.cursor_in { edge(c[0] <= 0.0, c[0] >= w - 1.0) } else { 0.0 } }
        if k[2] == k[3] { dy = if self.cursor_in { edge(c[1] <= 0.0, c[1] >= h - 1.0) } else { 0.0 } }
        let v = SCROLL_PX_S / sf * if shift { 2.0 } else { 1.0 } * dt.min(0.1);
        self.off[0] -= dx * v;
        self.off[1] -= dy * v;
    }

    /// Game tile (engine coords of rotation `rot`) under the cursor, as GameMap.CalcMapTileFromMousePos picks it: scan
    /// from 800 screen px below the cursor upwards in 2 px steps; the first tile whose flat cell holds the sample and
    /// whose column (2·testHeight px, + its building height with `use_bld`) reaches the cursor wins, so cliff faces and
    /// building bodies catch the cursor like the game's. Flat inverse of cell_px without heights.
    /// ponytail: moat / pitch drags keep the press height in the game (getFixedHeightMouseOver); not done here
    fn tile_at(&self, rot: i16) -> [i32; 2] { self.tile_at_px(self.cursor, rot) }

    /// Window px of a game position (engine coords of rotation `rot`, fractional) on the ground: cell_px of the West
    /// frame, raised by the nearest tile's height like a tile sprite (koth.rs's hill).
    fn ground(&self, rot: i16, x: f32, y: f32) -> [f32; 2] {
        const S: f32 = replay::N as f32 - 1.0;
        let (wx, wy) = match rot { 0 => (y, S - x), 2 => (S - x, S - y), 4 => (S - y, x), _ => (x, y) }; // replay::to_west
        let i = (wy.round().clamp(0.0, S) as usize) * replay::N + wx.round().clamp(0.0, S) as usize;
        let h = self.heights.get(i).copied().unwrap_or(0) as f32;
        [self.off[0] - (wx + wy) * 32.0 * self.zoom, self.off[1] + ((wx - wy) * 16.0 - h * 2.0) * self.zoom]
    }

    fn tile_at_px(&self, px: [f32; 2], rot: i16) -> [i32; 2] { self.tile_pick(px, rot, true, self.use_bld) }

    /// The screen-centre tile for the radar's view rectangle: GameMap.PreCalcScreenCentre picks it flat
    /// (useHeight: false), so hills and buildings under the centre don't make the rectangle jump.
    fn centre_flat(&self, px: [f32; 2], rot: i16) -> [i32; 2] { self.tile_pick(px, rot, false, false) }

    /// GameMap.screenTilesWide / High + CameraControls2D.getScreenCentreTileXY for a `w`×`h` logical-px window (`sf`
    /// device px each): DLL_RunTick's screen tiles (64 px columns, 16 px rows at this zoom) and screenCentrePos, the
    /// window centre's place along the camera's travel scaled to the map's columns / rows (the radar rectangle).
    fn screen_view(&self, w: f32, h: f32, sf: f32) -> ([i32; 2], [i32; 2]) {
        let z = self.zoom * sf; // PerfectPixelWithZoom.getZoom: device px per world px
        let tiles = [((w * sf) as i32 + 63) / 64, ((h * sf) as i32 + 15) / 16].map(|n| (n as f32 / z) as i32);
        let Some([x0, x1, y0, y1, size]) = self.bounds else { return ([0, 0], tiles) };
        let at = [(w / 2.0 - self.off[0]) / self.zoom, (h / 2.0 - self.off[1]) / self.zoom];
        let f = |v: f32, lo: f32, hi: f32| if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.5 };
        let (n, [tw, th]) = (size as i32, tiles);
        let x = ((n - tw * 2) as f32 * f(at[0], x0, x1)) as i32 + tw;
        let y = n - th - ((n - th) as f32 * (1.0 - f(at[1], y0, y1))) as i32 - 1 + th / 2; // the original's y runs up
        ([x, y], tiles)
    }

    /// GameData.SetCameraFromGameState: the `w`×`h` window centred on game tile (x, y) of rotation `rot`, raised by its
    /// height (minimap click, map turned). ponytail: the original's +2 / +18 px nudge is left out: it puts the window
    /// centre on the next tile down, so every turn would walk the rotation centre one tile
    fn centre_on(&mut self, w: f32, h: f32, world: &replay::World, rot: i16, x: i32, y: i32) {
        let (wx, wy) = replay::to_west_pub(rot, x as i16, y as i16);
        let [px, py] = replay::cell_px(wx, wy);
        let k = wy as usize * replay::N + wx as usize;
        let py = py - world.tiles.get(k).map_or(0.0, |t| t[5] as f32 * 2.0);
        self.off = [w / 2.0 - px * self.zoom, h / 2.0 - py * self.zoom];
    }

    /// The map-rotation centre: GameMap.getRotationCentre picks it with terrain heights but no building heights.
    fn centre_ground(&self, px: [f32; 2], rot: i16) -> [i32; 2] { self.tile_pick(px, rot, true, false) }

    /// CalcMapTileFromMousePos with its useHeight / useBuildingHeight switches.
    fn tile_pick(&self, px: [f32; 2], rot: i16, height: bool, bld: bool) -> [i32; 2] {
        let (wx, wy) = ((px[0] - self.off[0]) / self.zoom, (px[1] - self.off[1]) / self.zoom);
        // flat cell (West frame) under world point (wx, wy): s = x + y, d = x − y
        let flat = |wy: f32| { let (s, d) = (-wx / 32.0, wy / 16.0); (((s + d) / 2.0).round() as i16, ((s - d) / 2.0).round() as i16) };
        let (mut x, mut y) = flat(wy);
        if height && !self.heights.is_empty() {
            let n = replay::N as i16;
            for i in (0..=400).rev() {
                let j = i as f32 * 2.0 / self.zoom; // world px below the cursor
                let (tx, ty) = flat(wy + j);
                if !(0..n).contains(&tx) || !(0..n).contains(&ty) { continue; }
                let k = ty as usize * replay::N + tx as usize;
                let b = if bld { self.bld.get(k).copied().unwrap_or(0) as f32 } else { 0.0 };
                if 2.0 * self.heights[k] as f32 + b >= j { (x, y) = (tx, ty); break; }
            }
        }
        let (x, y) = replay::from_west(rot, x, y);
        [x as i32, y as i32]
    }
}

/// MainControls.computeMouseMapPosition: building heights catch the cursor unless a build tool is held, except the
/// tools that are placed on buildings (tower engines, flags, brazier, …: the sub-actions it lists).
fn bld_pick(mapper: Option<i32>) -> bool {
    mapper.is_none_or(|m| matches!(m, 120..=129 | 148 | 210 | 211 | 332..=338 | 360..=370 | 391..=398))
}

struct App {
    atlases: Vec<Atlas>,
    sprites: Vec<Sprite>,
    cells: Vec<Vec<usize>>, // per grid cell: sprite indices of one animation, in frame order
    hero: Vec<usize>,       // all body_hunter-* frames in order
    gfx: Option<Gfx>,
    scene: Option<Vec<Tri>>, // --map / --sheet: built once, uploaded once
    scene_rows: Vec<i32>,    // --replay: iso row per scene triangle (empty = all row 0)
    sheet: bool,
    cam: Cam,
    start: Instant,
    last: Instant,
    frame_times: Vec<f32>,
    frames: u32,
    play: Option<Play>,
    ui: Option<hud::Ui>, // --replay only, if the UI export exists
    focus: Option<(i16, i16)>, // start centred on this West-frame tile at 1:1 (--live: own keep, like the game)
    drag_from: Option<[i32; 2]>, // --live drag tool (walls, moat): tile where the press started
    menu: Option<menu::Menu>, // front end / Escape menu shown (no args, --menu; Escape in game)
    last_lobby: Option<skirmish::Lobby>, // the running game's lobby (Restart Mission)
    campaign: Option<(i32, i32, bool)>, // the running campaign mission: id, difficulty, win recorded
    tutorial: Option<menu::tutorial::Tutorial>, // the tutorial is running (game type 4): its panel (tutorial.rs)
    trail: Option<(i32, i32, i32, bool)>, // the running trail mission (menu/trail_menu.rs): trail, mission 0-based, difficulty, win recorded
    screen_image: Option<String>,        // the image in HUD texture menu::SCREEN
    names: Vec<String>,       // sprite names (Gm lookup for games started from the menu)
    end: endgame::Screen,     // --live: Victory / Defeat overlay (endgame.rs)
    intro: Option<video::Intro>, // start-up intro before the main menu (video.rs)
    menu_loop: video::MenuLoop,  // the main menu's backdrop video (video.rs)
    cursors: Option<cursor::Cursors>, // the OS cursor: Director's sword (cursor.rs)
    mods: ModifiersState,     // keyboard modifiers (fullscreen toggle)
    keys: keys::Keys,         // in-game hotkeys (Options > Hotkeys, settings.cfg ||KEYS||)
    #[cfg(feature = "live")]
    rp: Option<game_replay::Controls>, // a playing replay's bar (game_replay.rs)
    ux: ux::Ux,               // Mac UX: trackpad, menu bar, frame pacing, pause in background, F1 sheet (ux.rs)
    stats: stats::Panel,      // F3 / end-screen stats graphs (stats.rs)
    loading: loader::Loading, // a game start whose sim setup runs on a worker thread (loader.rs)
    fps: Option<(Instant, u32, i32, [String; 2])>, // ToggleFrameRate on: since, frames, sim tick then, the text (fps_text)
    editor: Option<menu::editor::Hud>, // the map editor runs (menu/editor.rs): its panel instead of the HUD
}

/// --replay state. Tiles live in fixed 4-triangle slots of the static buffer (cliff side + tile, every tile mesh
/// is <= 2 tris), slots ordered back to front by iso row, so a tile update is one 384-byte buffer write.
/// ponytail: ~181k slots × 768 B = 140 MB; a tile appearing on a cell empty at tick 0 is not drawn
struct Play {
    ticks: Vec<Vec<replay::Rec>>,
    huds: Vec<hud::HudState>, // per tick, what the engine sent the HUD
    at: usize, // next tick to apply
    world: replay::World,
    gm: replay::Gm,
    slot: Vec<u32>, // game tile -> slot (u32::MAX = no slot: not drawn at tick 0)
    dirty: Vec<usize>,
    objs: Vec<Tri>, // rebuilt when a tick is applied, sorted by (row, sub-slot, y, id)
    obj_rows: Vec<i32>, // iso row of each objs triangle
    playing: bool,
    rate: f32, // ticks/s (game speed)
    ff: bool,  // Tab held: fast-forward, ticks as fast as the sim runs
    acc: f32,
    /// --live: the sim's real tick length in s (slides last k7 + 1 ticks), longer than 1 / rate while a big battle
    /// keeps the sim busy; and the (time, sim tick, rate) it was last measured at
    tick_s: f32,
    tick_mark: Option<(Instant, i32, f32)>,
    now: Instant, // this frame's clock for the slides: every rebuild_objs of a frame draws them at the same instant
    misses: HashMap<i16, u32>, // GM file -> records without a sprite
    title: (bool, u32, i64), // (playing, rate bits, tick if paused) the window title last showed
    live: Option<live::Thread>, // --live: records come from the Rust sim thread, `huds` holds just the latest
}

impl App {
    fn new() -> Self {
        let (mut atlases, mut named) = load_tables();
        custom_lords::add_sprites(&mut atlases, &mut named); // the custom lords' own map sprites (their sprite/ folders)
        let mut anims: BTreeMap<String, Vec<(u32, usize)>> = BTreeMap::new();
        for (i, (name, _)) in named.iter().enumerate() {
            if let Some((k, n)) = split_frame(name) { anims.entry(k).or_default().push((n, i)); }
        }
        let mut anims: Vec<(String, Vec<usize>)> = anims.into_iter().map(|(k, mut v)| {
            v.sort();
            (k, v.into_iter().map(|(_, i)| i).collect())
        }).collect();
        let hero = anims.iter().find(|(k, _)| k == "body_hunter-").expect("no body_hunter frames").1.clone();
        // Spread the grid over the whole catalogue so every atlas shows up.
        anims.retain(|(_, v)| v.len() >= 8);
        let step = anims.len() as f32 / (COLS * ROWS) as f32;
        let cells = (0..COLS * ROWS).map(|i| anims[(i as f32 * step) as usize].1.clone()).collect();
        println!("{} sprites, {} animations, hunter has {} frames", named.len(), anims.len(), hero.len());
        let now = Instant::now();
        let names: Vec<String> = named.iter().map(|(n, _)| n.clone()).collect();
        let mut app = Self { atlases, sprites: named.into_iter().map(|(_, s)| s).collect(), cells, hero, gfx: None, scene: None, scene_rows: vec![], sheet: false,
            cam: Cam { zoom: 1.0, use_bld: true, zoom_pos: 3.0, ..Default::default() }, start: now, last: now, frame_times: vec![], frames: 0, play: None, ui: None, focus: None, drag_from: None, menu: None, last_lobby: None, campaign: None, trail: None, tutorial: None, screen_image: None, names: names.clone(), end: Default::default(), intro: None, menu_loop: video::MenuLoop::new(), cursors: None, mods: Default::default(), keys: keys::Keys::load(), #[cfg(feature = "live")] rp: None, ux: Default::default(), stats: Default::default(), loading: Default::default(), fps: None, editor: None };
        let args: Vec<String> = std::env::args().collect();
        let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
        if let Some(m) = arg("--map") { app.scene = Some(app.build_map(&m, &names)); }
        if let Some(f) = arg("--live") {
            let t0 = Instant::now();
            #[allow(unused_mut)]
            let mut l = live::Live::load(&f);
            #[cfg(feature = "live")]
            { l.hill = koth::Hill::read_beside(std::path::Path::new(&f)); } // King of the Hill's score (koth.rs)
            println!("live {f}: loaded in {:.2}s", t0.elapsed().as_secs_f32());
            app.start_live(l, None);
            // --trail 11,4: the save is that trail's mission (0-based) at difficulty 1 (end screens; endgame write_trail_won_saves)
            if let Some((t, id)) = arg("--trail").as_deref().and_then(|v| v.split_once(',')).and_then(|(t, id)| Some((t.parse().ok()?, id.parse().ok()?))) { app.trail = Some((t, id, 1, false)); }
        }
        #[cfg(feature = "live")]
        if let Some(m) = arg("--skirmish") { // --skirmish "Crater Lake": new skirmish vs the Rat, skipping the lobby
            let mut lobby = skirmish::Lobby::from_settings(skirmish::maps_dir().join(format!("{m}.map"))); // your Options colour
            if std::env::var_os("SHC_TEAMS").is_some() { (lobby.team, lobby.ais) = (1, vec![(0, 1), (1, 2)]); } // + the Snake; the Rat on your team
            if let Some(n) = dev_ai() { lobby.ais = vec![(n, 0)]; }
            lobby.hill = std::env::var("SHC_KOTH").ok().and_then(|v| v.parse().ok()).unwrap_or(0); // SHC_KOTH=300: King of the Hill
            match skirmish::start(&lobby) {
                Ok(mem) => { let mut l = live::Live::from_mem(mem).with_hill(lobby.hill); dev_fight(&mut l); app.start_live(l, None) }
                Err(e) => panic!("{e}"),
            }
        }
        #[cfg(feature = "live")]
        if let Some(n) = arg("--mission").and_then(|n| n.parse().ok()) { // --mission 1: campaign mission n, no briefing (screenshots)
            match skirmish::start_mission(n, 1) { Ok(m) => app.start_mission(n, 1, m, false, false), Err(e) => panic!("{e}") }
        }
        #[cfg(feature = "live")]
        if let Some(n) = arg("--freebuild").and_then(|n| n.parse::<usize>().ok()) { // --freebuild 0: the nth built-in Free Build map, extreme powers on (screenshots)
            let (map, _) = skirmish::solo_maps(3, false, true).into_iter().nth(n).expect("no such Free Build map");
            match skirmish::start_solo(&map, &skirmish::FreeBuild { extreme_powers: true, ..Default::default() }, None) { Ok(m) => app.start_live(live::Live::from_mem(m), None), Err(e) => panic!("{e}") }
        }
        #[cfg(feature = "live")]
        if let Some(role) = arg("--lan") { // --lan host / join: a two-player LAN game on this machine, one team vs the Rat (screenshots)
            let mut net = if role == "host" { lan::Net::host("Host", 0).expect("host") } else { lan::Net::join(([127, 0, 0, 1], lan::GAME_PORT).into(), "Guest").expect("join") };
            let t0 = Instant::now();
            while t0.elapsed() < std::time::Duration::from_secs(60) && if net.is_host() { net.lobby.humans.len() < 2 } else { net.started.is_none() } {
                net.poll();
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let (lobby, seed, delay) = if net.is_host() {
                let mut l = net.lobby.clone();
                for h in &mut l.humans { h.team = 1; }
                l.ais = vec![(0, 2)]; // the Rat against them (else the game is won at once)
                l.hill = std::env::var("SHC_KOTH").ok().and_then(|v| v.parse().ok()).unwrap_or(0); // SHC_KOTH=300: King of the Hill
                net.set_lobby(l);
                net.start(1, lan::DELAY)
            } else { net.started.clone().expect("the host did not start") };
            app.lan_start(menu::mp_menu::Started { net, lobby, seed, delay });
        }
        #[cfg(feature = "live")]
        if args.iter().any(|a| a == "--tutorial") { // --tutorial: the tutorial map and its panel (screenshots)
            let m = skirmish::start_tutorial().unwrap_or_else(|e| panic!("{e}"));
            let cam = engine_view(&m);
            app.start_live(live::Live::from_mem(m), cam);
            app.tutorial = Some(Default::default());
        }
        if ["--live", "--replay", "--map", "--sheet", "--skirmish", "--mission", "--freebuild"].iter().all(|k| arg(k).is_none()) && !args.iter().any(|a| a == "--tutorial") && arg("--lan").is_none() || args.iter().any(|a| a == "--menu") {
            app.ui = hud::Ui::load(&cache_dir().join("ui"));
            if app.ui.is_none() { panic!("the menu needs the UI export: run re/scripts/export_ui_atlas.py (viewer/README.md)"); }
            if !cache_dir().join("ui/frontend.rgba").exists() { println!("no main-menu art: run re/scripts/export_frontend.py"); }
            app.menu = Some(menu::Menu::main());
            if std::env::var_os("SHC_LOBBY").is_some() { // SHC_LOBBY=1 (screenshots): straight into the skirmish lobby
                let mut l = skirmish::Lobby::from_settings(PathBuf::new());
                if let Some(n) = dev_ai() { l.ais = vec![(n, 0)]; }
                app.menu = Some(menu::Menu::skirmish_again(l));
            }
            // IntroSequence first; --skip-intro = the original's skipintro setting. Scripted runs (SHC_SHOT / SHC_INPUT)
            // skip it unless SHC_INTRO=1.
            let scripted = std::env::var_os("SHC_SHOT").is_some() || std::env::var_os("SHC_INPUT").is_some();
            if !args.iter().any(|a| a == "--skip-intro") && (!scripted || std::env::var_os("SHC_INTRO").is_some()) { app.intro = video::Intro::new(); }
        }
        if let Some(f) = arg("--replay") {
            let start = arg("--tick").map_or(0, |t| t.parse().unwrap());
            app.start_replay(&f, start, &names);
            app.ui = hud::Ui::load(&cache_dir().join("ui"));
            if app.ui.is_none() { println!("no HUD: run re/scripts/export_ui_atlas.py (viewer/README.md)"); }
        }
        if let Some(p) = arg("--sheet") { app.scene = Some(app.build_sheet(&p, &names)); app.sheet = true; }
        app
    }

    /// --live / menu: run `l` on the sim thread, camera on the own keep, else on `cam` (west-frame tile).
    fn start_live(&mut self, mut l: live::Live, cam: Option<(i16, i16)>) {
        #[cfg(feature = "live")]
        if !l.tap.as_ref().is_some_and(|t| t.playing()) { self.rp = None; } // any other game ends a replay
        self.trail = None; // any other game start leaves the trail (Go::Trail sets it again after this)
        self.tutorial = None; // and the tutorial (Go::Tutorial sets it again)
        self.editor = None; // and the map editor (start_editor sets it again)
        // a new map: camera limits and the cursor pick's heights refill from its first draw list (loads from the
        // pause menu / restarts come here without going through the main menu)
        (self.cam.map, self.cam.heights) = (None, vec![]);
        let recs = l.records();
        println!("live: sim tick {}, {} tile records", l.tick(), recs.len());
        let hud = l.hud();
        // SHC_KEEP=p (screenshots): the camera on player p's keep instead
        let keep = match std::env::var("SHC_KEEP").ok().and_then(|v| v.parse().ok()) { Some(p) => l.keep_of(p), None => l.keep() };
        self.focus = keep.map(|(x, y)| replay::to_west_pub(l.rotation(), x, y)).or(cam);
        let names = std::mem::take(&mut self.names);
        self.start_play(vec![recs], vec![hud], 0, &names);
        self.names = names;
        let p = self.play.as_mut().unwrap();
        p.playing = true;
        p.live = Some(live::Thread::spawn(l, p.playing, p.rate));
        menu::game_started(); // Show Play Time counts from here
        if self.ui.is_none() { self.ui = hud::Ui::load(&cache_dir().join("ui")); }
        if self.gfx.is_some() { self.upload_scene(); } // started from the menu: the window exists already
    }

    /// A campaign mission loaded: run it paused (under its briefing when `briefing`), objective progress hidden
    /// until Start (GameAction HideObjectiveProgress 1045).
    #[cfg(feature = "live")]
    fn start_mission(&mut self, id: i32, d: i32, m: shc_sim::mem::Mem, briefing: bool, from_story: bool) {
        (self.last_lobby, self.campaign) = (None, Some((id, d, false)));
        let cam = engine_view(&m);
        self.start_live(live::Live::from_mem(m), cam);
        if let Some(p) = &mut self.play {
            p.playing = false;
            if let Some(l) = &p.live { l.send(live::Cmd::Action([1045, 1, 1])); }
        }
        if briefing { self.menu = Some(menu::Menu::briefing(id, from_story)); }
    }

    /// Escape: in game the pause menu (sim paused), in the menu one screen back; quits the --replay / --map viewers.
    fn escape(&mut self, el: &ActiveEventLoop) {
        if self.intro.take().is_some() { return; } // KeyManager: Escape ends the start-up intro
        #[cfg(feature = "live")]
        if self.lan_escape() { return; } // closes a LAN game's chat line (lan.rs)
        match self.menu.as_mut() {
            Some(m) => if let Some(go) = m.back() { self.menu_go(go, el) },
            None if self.play.as_ref().is_some_and(|p| p.live.is_some()) => {
                if let Some(p) = &mut self.play { p.playing = false; }
                self.menu = Some(menu::Menu::pause(self.last_lobby.is_some() || self.campaign.is_some() || self.trail.is_some()));
            }
            None => el.exit(),
        }
    }

    /// The cursor is on the tutorial panel (its clicks don't reach the map).
    fn on_tutorial(&self) -> bool {
        let (Some(t), Some(g)) = (&self.tutorial, &self.gfx) else { return false };
        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
        t.over(sz.width, sz.height, self.cam.cursor)
    }
    /// HUD_Tutorial.ButtonClicked: Continue = TutorialAction 20 (and the panel closes once Exit shows: the end), Exit =
    /// back to the front end.
    fn tutorial_click(&mut self, el: &ActiveEventLoop) {
        let (Some(t), Some(g)) = (&self.tutorial, &self.gfx) else { return };
        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
        match t.hit(sz.width, sz.height, self.cam.cursor) {
            Some(menu::tutorial::Button::Continue) => {
                let end = t.buttons[1].is_some();
                self.tutorial_action(20, -1);
                if end { if let Some(t) = &mut self.tutorial { t.visible = false } }
            }
            Some(menu::tutorial::Button::Exit) => self.menu_go(menu::Go::ToMenu, el),
            None => {}
        }
    }
    /// EngineInterface.TutorialAction (game type 4 only): the player did what a tutorial step asks.
    /// In game, right button down / up: DLL_TroopSelection's right press / release (the troop cursor's order; the
    /// engine drops its tool, closes a panel), StopAllPlacement, and on the map HUD_RightClick (EditorDirector).
    fn right_button(&mut self, pressed: bool) {
        if self.editor_right(pressed) { return; } // the map editor (menu/editor.rs)
        let over = self.ui.as_ref().is_some_and(|u| u.over(self.cam.cursor));
        if !pressed {
            if let Some(u) = &mut self.ui { u.rclick = None; } // HUD_RightClick closes with the button
            if let (Some(l), false) = (self.play.as_ref().and_then(|p| p.live.as_ref()), over) {
                l.send(live::Cmd::Troops(live::TroopIn::Right(false)));
            }
            return;
        }
        if self.ui.as_mut().is_some_and(|u| u.rclick(self.cam.cursor)) { return; } // a troop type off the selection
        let tool = self.ui.as_ref().and_then(|u| u.selected_mapper()).is_some();
        if let Some(u) = &mut self.ui { u.selected = None; } // drop the build tool (StopAllPlacement)
        self.drag_from = None;
        if let Some((l, st)) = self.play.as_ref().and_then(|p| Some((p.live.as_ref()?, &p.huds[0]))) {
            l.send(live::Cmd::Troops(live::TroopIn::Right(true)));
            if let (Some(u), false) = (self.ui.as_mut(), hud::Ui::rclick_blocked(st, tool, over, menu::sh1_rts())) { u.rclick_open(self.cam.cursor, l.rot); }
        }
    }

    /// GameMap.RotateMapLeft / Right -> setMapRotation(.., centre = getRotationCentre: the ground tile at the window
    /// centre); left = North -> East (+2), right = North -> West (+6).
    fn rotate_map(&mut self, right: bool) {
        let (Some(g), Some(l)) = (&self.gfx, self.play.as_ref().and_then(|p| p.live.as_ref())) else { return };
        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
        let [x, y] = self.cam.centre_ground([sz.width / 2.0, sz.height / 2.0], l.rot);
        l.send(live::Cmd::Rotate((l.rot as i32 + if right { 6 } else { 2 }) % 8, x, y));
        self.tutorial_action(2, -1); // GameMap.RotateMapLeft / Right
    }

    /// PerfectPixelWithZoom.adjustZoom(-0.5, loop: true) (right-click menu): past the farthest zoom, back to the nearest.
    fn zoom_out_loop(&mut self) {
        let Some(g) = &self.gfx else { return };
        let lo = zoom_pos_clamp(0.0, g.config.width as f32, g.config.height as f32, self.cam.map.map_or(800.0, |m| m.size));
        self.adjust_zoom(if self.cam.zoom_pos - 0.5 < lo { 3.0 - self.cam.zoom_pos } else { -0.5 });
    }

    fn tutorial_action(&self, id: i32, value: i32) {
        if self.tutorial.is_none() { return; }
        if let Some(l) = self.play.as_ref().and_then(|p| p.live.as_ref()) { l.send(live::Cmd::Tutorial(id, value)) }
    }

    /// The end screen's buttons (endgame.rs): Continue = HUD_MissionOver's Exit (`mission_over`; a skirmish: its setup
    /// again), Play Again = Restart Mission, Main Menu. A skirmish-mode game goes into Skirmish Masters (ShowMPScore).
    fn end_exit(&mut self, e: endgame::Exit, el: &ActiveEventLoop) {
        #[cfg(feature = "live")]
        if let Some(k) = menu::trail_menu::masters::kind(self.trail.map(|t| (t.0, t.1)), self.last_lobby.as_ref().map(|l| l.map.as_path())) { menu::trail_menu::masters::record(k); }
        let lobby = self.last_lobby.clone().filter(|_| e == endgame::Exit::Continue && self.trail.is_none() && self.campaign.is_none());
        let go = match e {
            endgame::Exit::Again => menu::Go::Restart,
            endgame::Exit::Menu => { (self.trail, self.campaign) = (None, None); menu::Go::ToMenu }
            endgame::Exit::Continue => self.mission_over(),
        };
        self.menu_go(go, el);
        if let Some(l) = lobby { self.menu = Some(menu::Menu::skirmish_again(l)); }
    }

    /// Continue: a lost campaign mission starts again under its briefing, anything else leaves the game (a won
    /// campaign mission for the next story, see ToMenu).
    fn mission_over(&self) -> menu::Go {
        // a trail mission: won = the next one at once, lost = the same again (HUD_MissionOver)
        if let Some((t, id, d, won)) = self.trail { return menu::trail_menu::after(t, id, d, won) }
        match self.campaign { Some((id, _, false)) => menu::Go::Mission(id), _ => menu::Go::ToMenu }
    }

    /// Menu result: load / start a game, leave it, quit.
    fn menu_go(&mut self, go: menu::Go, el: &ActiveEventLoop) {
        #[cfg(feature = "live")]
        // game starts: sim setup on a worker thread (loader.rs); scripted runs stay inline (frame-numbered clicks)
        let Some(go) = (if scripted() { Some(go) } else { self.load_in_background(go) }) else { return };
        match go {
            menu::Go::Quit => el.exit(),
            menu::Go::Resume => { self.menu = None; if let Some(p) = &mut self.play { p.playing = true; } }
            menu::Go::ToMenu => {
                (self.play, self.scene, self.focus, self.drag_from) = (None, None, None, None); // dropping the Thread stops the sim
                self.editor = None;
                self.cam.heights.clear();
                if let Some(g) = &mut self.gfx { g.static_buf = None; }
                if let Some(u) = &mut self.ui { u.selected = None; u.radar = false; }
                // a won campaign mission goes on to the next one's story (its campaign's closing page after the 5th;
                // HUD_MissionOver), a quit one back to its campaign's mission list
                // a trail mission left: back to its trail map
                if let Some((t, ..)) = self.trail.take() { self.menu = Some(menu::Menu::trail_map(t)); return; }
                self.menu = Some(match self.campaign.take() {
                    Some((id, _, true)) => menu::Menu::story(if id % 5 == 0 { id } else { id + 1 }, id % 5 == 0),
                    Some((id, ..)) => menu::Menu::missions(((id - 1) / 5) as usize),
                    None => menu::Menu::main(),
                });
            }
            menu::Go::QuitMission(_) => self.menu_go(menu::Go::ToMenu, el),
            #[cfg(feature = "live")]
            menu::Go::Editor(s) => self.start_editor(s),
            // Save Game (the pause menu's Save dialog, menu.rs): the sim thread writes the .sav into the save folder, where
            // the real game and Load Game list it
            #[cfg(feature = "live")]
            menu::Go::Save(path) => if let Some(l) = self.play.as_mut().and_then(|p| p.live.as_mut()) { l.send(live::Cmd::Save(path)); },
            #[cfg(feature = "live")]
            menu::Go::Replay(f) => match game_replay::open(&f) {
                Ok((l, c)) => { (self.menu, self.last_lobby, self.campaign) = (None, None, None); self.start_live(l, None); self.rp = Some(c); }
                Err(e) => if let Some(m) = &mut self.menu { m.error = Some(e) },
            },
            #[cfg(feature = "live")]
            menu::Go::Load(f) => match self.loaded(|| loader::load_save(&f)) {
                Ok(m) => {
                    (self.menu, self.last_lobby, self.campaign) = (None, None, None);
                    let mut l = live::Live::from_mem(m);
                    l.hill = koth::Hill::read_beside(&f); // a King of the Hill game's score (koth.rs)
                    self.start_live(l, None);
                }
                Err(e) => if let Some(m) = &mut self.menu { m.error = Some(e) },
            },
            // Restart Mission: the same lobby again
            #[cfg(feature = "live")]
            menu::Go::Restart if self.trail.is_some() => if let Some((t, id, d, _)) = self.trail { self.menu_go(menu::Go::Trail(t, id, d), el) },
            #[cfg(feature = "live")]
            menu::Go::Restart => match (self.campaign, self.last_lobby.clone()) {
                (Some((id, ..)), _) => self.menu_go(menu::Go::Mission(id), el), // back to its briefing
                (None, Some(l)) => self.menu_go(menu::Go::Start(l), el),
                _ => {}
            },
            // campaign mission: loaded at difficulty 1 under its briefing (MainViewModel.StartCampaignMission)
            #[cfg(feature = "live")]
            menu::Go::Mission(id) => match self.loaded(|| skirmish::start_mission(id, 1)) {
                Ok(m) => { let story = self.menu.as_ref().is_some_and(|m| m.is_story()); self.start_mission(id, 1, m, true, story) }
                Err(e) => if let Some(m) = &mut self.menu { m.error = Some(e) },
            },
            // the briefing's Start: another difficulty reloads the mission at it; objectives shown, clock reset
            #[cfg(feature = "live")]
            // (a Custom Scenario's briefing: its difficulty was set before the load, so just start)
            menu::Go::Brief(d) => {
                if let Some((id, ..)) = self.campaign.filter(|&(_, d0, _)| d != d0) {
                    match self.loaded(|| skirmish::start_mission(id, d)) {
                        Ok(m) => self.start_mission(id, d, m, false, false),
                        Err(e) => { if let Some(m) = &mut self.menu { m.error = Some(e) } return; }
                    }
                }
                self.menu = None;
                if let Some(p) = &mut self.play {
                    p.playing = true;
                    if let Some(l) = &p.live { l.send(live::Cmd::Action([1045, 0, 0])); l.send(live::Cmd::Action([1071, 0, 0])); }
                }
            },
            // a Crusader Trail mission (menu/trail_menu.rs): StartSkirmishTrailMission hides the objectives, then its
            // briefing over the paused game
            #[cfg(feature = "live")]
            // the main menu's tutorial book (MainViewModel InitNewScene(Tutorial) -> HUD_Tutorial.StartTutorial): the
            // tutorial map, its panel open (OpenTutorial)
            #[cfg(feature = "live")]
            menu::Go::Tutorial => match self.loaded(skirmish::start_tutorial) {
                Ok(m) => {
                    (self.menu, self.last_lobby, self.campaign) = (None, None, None);
                    let cam = engine_view(&m);
                    self.start_live(live::Live::from_mem(m), cam);
                    self.tutorial = Some(Default::default());
                }
                Err(e) => { let mut m = self.menu.take().unwrap_or_else(menu::Menu::main); m.error = Some(e); self.menu = Some(m); }
            },
            #[cfg(feature = "live")]
            menu::Go::Trail(t, id, d) => match self.loaded(|| menu::trail_menu::start(t, id, d)) {
                Ok(m) => {
                    (self.menu, self.last_lobby, self.campaign) = (None, None, None);
                    let cam = engine_view(&m);
                    self.start_live(live::Live::from_mem(m), cam);
                    self.trail = Some((t, id, d, false));
                    if let Some(l) = self.play.as_ref().and_then(|p| p.live.as_ref()) { for a in menu::trail_menu::start_actions(t) { l.send(live::Cmd::Action(a)); } }
                    // the trail briefing over the paused mission until its Start (trail_menu.rs, ButtonGotoBriefing)
                    if let Some(b) = menu::Menu::trail_brief(t, id, d) { if let Some(p) = &mut self.play { p.playing = false; } self.menu = Some(b); }
                }
                Err(e) => { let mut m = self.menu.take().unwrap_or_else(menu::Menu::main); m.error = Some(e); self.menu = Some(m); }
            },
            #[cfg(feature = "live")]
            menu::Go::Lan(s) => self.lan_start(*s), // a LAN game (lan.rs, menu/mp_menu.rs)
            #[cfg(feature = "live")]
            menu::Go::Start(l) => match self.loaded(|| skirmish::start(&l)) {
                Ok(m) => { let hill = l.hill; (self.menu, self.last_lobby, self.campaign) = (None, Some(l), None); self.start_live(live::Live::from_mem(m).with_hill(hill), None); }
                Err(e) => self.menu.as_mut().unwrap().error = Some(e),
            },
            // Options changed settings.cfg: the running game's speed (Director.SetEngineFrameRate); Apply's screen mode
            menu::Go::Settings(mode) => {
                let speed = skirmish::Settings::load().game_speed();
                if let Some(p) = &mut self.play { if !lan::speed_setting(speed) { p.rate = speed } } // LAN: the host's (lan.rs)
                self.keys = keys::Keys::load();
                if let (Some(mode), Some(g)) = (mode, &self.gfx) { ux::set_mode(&g.window, mode); }
                self.apply_options(); // VSync, cursor lock, background mute (ux.rs)
            }
            // Sandbox Play: Free Build / Custom Scenarios (FRONT_StandaloneMission.StartMap)
            #[cfg(feature = "live")]
            menu::Go::FreeBuild(..) | menu::Go::Scenario(..) => {
                let r = self.loaded(|| match &go { menu::Go::FreeBuild(map, fb) => skirmish::start_solo(map, fb, None), menu::Go::Scenario(map, d) => skirmish::start_solo(map, &Default::default(), Some(*d)), _ => unreachable!() });
                match r {
                    // StartMap: HideObjectiveProgress(1, 1). A Custom Scenario then waits, paused, under its briefing
                    // (PostStartMapMission -> ButtonGotoBriefing); the briefing's Start runs it (Go::Brief)
                    Ok(m) => {
                        (self.menu, self.last_lobby, self.campaign) = (None, None, None);
                        let cam = engine_view(&m);
                        self.start_live(live::Live::from_mem(m), cam);
                        if let Some(l) = self.play.as_ref().and_then(|p| p.live.as_ref()) { l.send(live::Cmd::Action([1045, 1, 1])); }
                        if let menu::Go::Scenario(map, d) = go {
                            if let Some(p) = &mut self.play { p.playing = false; }
                            self.menu = Some(menu::Menu::scenario_briefing(map, d));
                        }
                    }
                    Err(e) => if let Some(m) = &mut self.menu { m.error = Some(e) },
                }
            }
            #[cfg(not(feature = "live"))]
            _ => if let Some(m) = &mut self.menu { m.error = Some("built without the sim (`live` feature)".into()) },
        }
    }

    /// All frames whose name is `<prefix> <n>`, in frame order.
    fn frames_of(names: &[String], prefix: &str) -> Vec<usize> {
        let mut v: Vec<(u32, usize)> = names.iter().enumerate()
            .filter_map(|(i, n)| n.strip_prefix(prefix)?.trim_start().parse().ok().map(|f| (f, i))).collect(); // "name 012" or "body_lord-12"
        v.sort();
        v.into_iter().map(|(_, i)| i).collect()
    }

    /// --sheet tile_land8: every frame of one tile set, 64 per row at 26 px pitch and 0.4x (frame n at col n%64,
    /// row n/64), for eyeballing / measuring which frames belong together.
    fn build_sheet(&mut self, prefix: &str, names: &[String]) -> Vec<Tri> {
        let mut out = vec![];
        for (n, &i) in Self::frames_of(names, prefix).iter().enumerate() {
            self.push(&mut out, i, [13.0 + (n % 64) as f32 * 26.0, 13.0 + (n / 64) as f32 * 26.0], 0.4);
        }
        out
    }

    fn build_map(&mut self, name: &str, names: &[String]) -> Vec<Tri> {
        let t0 = Instant::now();
        let tiles = map::load(name);
        // ponytail: ground flag -> sprite set / colour is a GUESS from eyeballing GrainBasket (README)
        let sets: Vec<(u8, Vec<usize>)> = GROUND.iter().filter_map(|(g, set, _)| {
            let (prefix, r) = set.clone()?;
            let f = Self::frames_of(names, prefix);
            (f.len() >= r.end).then(|| (*g, f[r].to_vec()))
        }).collect();
        let mut out = Vec::with_capacity(tiles.len() * 2);
        for t in &tiles {
            if let Some((_, f)) = sets.iter().find(|(g, _)| *g == t.ground) {
                self.push(&mut out, f[t.rand as usize % f.len()], [t.x, t.y], 1.0);
                continue;
            }
            let c = GROUND.iter().find(|g| g.0 == t.ground).map_or([1.0, 0.0, 1.0], |g| g.2);
            let color = [c[0], c[1], c[2], 0.0]; // sRGB values: sprite.wgsl decodes vertex colours
            let (hw, hh) = (map::TILE_W / 2.0, map::TILE_H / 2.0);
            let v = |dx: f32, dy: f32| Vert { pos: [t.x + dx, t.y + dy], uv: [0.0, 0.0], color, team: [0.0; 4] };
            out.push((0, [v(0.0, -hh), v(hw, 0.0), v(-hw, 0.0)]));
            out.push((0, [v(-hw, 0.0), v(hw, 0.0), v(0.0, hh)]));
        }
        // ponytail: one stable sort by atlas breaks back-to-front order across atlases; fine while tiles are flat
        out.sort_by_key(|(a, _)| *a);
        println!("map {name}: {} tiles, {} tris, built in {:.2}s", tiles.len(), out.len(), t0.elapsed().as_secs_f32());
        out
    }

    fn start_replay(&mut self, path: &str, start: usize, names: &[String]) {
        let t0 = Instant::now();
        let (ticks, huds) = replay::load(path);
        assert!(!ticks.is_empty(), "no DLL_RunTick records in {path}");
        let t0recs = &ticks[0];
        let span = |k: usize| t0recs.iter().filter(|r| r[0] == 0).map(|r| r[k]).fold((i16::MAX, i16::MIN), |(a, b), v| (a.min(v), b.max(v)));
        println!("replay {path}: {} ticks, tick 0 has {} records, tile x {:?} y {:?}, loaded in {:.2}s",
            ticks.len(), t0recs.len(), span(1), span(2), t0.elapsed().as_secs_f32());
        self.start_play(ticks, huds, start, names);
    }

    /// Shared by --replay and --live: tick 0's tiles become the static buffer, then ticks 1..=start are applied.
    fn start_play(&mut self, ticks: Vec<Vec<replay::Rec>>, huds: Vec<hud::HudState>, start: usize, names: &[String]) {
        let mut p = Play { ticks, huds, at: 0, world: replay::World::new(), gm: replay::Gm::new(names), slot: vec![], dirty: vec![],
            objs: vec![], obj_rows: vec![], playing: std::env::var_os("SHC_SHOT").is_none(), rate: skirmish::Settings::load().game_speed(), ff: false, acc: 0.0, tick_s: 0.0, tick_mark: None, now: Instant::now(), misses: HashMap::new(), title: (false, 0, -1), live: None };
        let mut dirty = vec![];
        p.world.apply(&p.ticks[0], &mut dirty);
        // Slots only for cells tick 0 draws (it resends every tile; GM 38 = off-map ring, never drawn).
        // Back to front: screen row = x−y (see replay::cell_px).
        let mut order: Vec<usize> = (0..replay::N * replay::N).filter(|&i| !matches!(p.world.tiles[i][3], 0 | 38)).collect();
        order.sort_by_key(|&i| replay::row(i));
        p.slot = vec![u32::MAX; replay::N * replay::N];
        for (s, &i) in order.iter().enumerate() { p.slot[i] = s as u32; }
        p.at = 1;
        while p.at <= start.min(p.ticks.len() - 1) { p.world.apply(&p.ticks[p.at], &mut dirty); p.at += 1; }
        let scene = order.iter().flat_map(|&i| self.tile_tris(&mut p, i)).collect();
        self.scene_rows = order.iter().flat_map(|&i| [replay::row(i); 4]).collect();
        self.play = Some(p);
        self.rebuild_objs();
        self.scene = Some(scene);
    }

    /// Exactly 4 tris for game tile `i`: cliff side, then tile (unused ones degenerate).
    /// The cursor pick's heights for tile record `r` (GameMap.cs tile update): testHeight ×32 = k5, or k17 on a
    /// construction site (k15 != 0, k17 != 10000); building height px = k18·2, else what a tall tile sprite stands
    /// above its 32 px diamond. ponytail: the game measures the construction-site image there; this the drawn one
    fn pick_heights(&self, p: &Play, r: &replay::Rec) -> (i16, i16) {
        let test = if r[15] != 0 && r[17] != 10000 { r[17] } else { r[5] };
        let sprite = if r[3] != 0 && r[3] != 38 { p.gm.tile(r[3], r[4], r[6]).0 } else { None };
        let bld = if r[18] != 0 { r[18].saturating_mul(2) } else { sprite.map_or(0, |s| (self.sprites[s].h as i16 - 32).max(0)) };
        (test, bld)
    }

    fn tile_tris(&self, p: &mut Play, i: usize) -> [Tri; 4] {
        // ponytail: NaN verts are culled by the rasterizer and skipped by the bbox fit (f32::min ignores NaN)
        let nan = Vert { pos: [f32::NAN; 2], uv: [0.0; 2], color: [0.0; 4], team: [0.0; 4] };
        let mut out = vec![(6, [nan; 3]); 4]; // 6 = AllTileSprites, the one atlas all tile sets live in
        let r = p.world.tiles[i];
        let (file, image, h) = (r[3], r[4], r[5]);
        // GM 38 = debug-logic tiles; image 321 sets tileImage = null in C# (nothing drawn, likely water drawn elsewhere)
        if file != 0 && file != 38 {
            let (sprite, light) = p.gm.tile(file, image, r[6]);
            let c = replay::shade(light, r[7]);
            let color = [c[0], c[1], c[2], 1.0]; // Unity colours are sRGB: sprite.wgsl decodes vertex colours
            match sprite {
                Some(s) => {
                    let [x, y] = replay::cell_px((i % replay::N) as i16, (i / replay::N) as i16);
                    let pos = [x, y - h as f32 * 2.0]; // Unity: tile transform y += k5/32 units
                    let mut t = vec![];
                    self.push_c(&mut t, s, pos, 1.0, color, [0.0; 4]);
                    for (o, t) in out[2..].iter_mut().zip(t) { *o = t; }
                    if let Some(c) = p.gm.chevron(&p.world, i) {
                        let mut t = vec![];
                        self.push_c(&mut t, c, pos, 1.0, color, [0.0; 4]);
                        for (o, t) in out.iter_mut().zip(t) { *o = t; }
                    }
                }
                None => *p.misses.entry(file).or_default() += 1,
            }
        }
        [out[0], out[1], out[2], out[3]]
    }

    fn rebuild_objs(&mut self) {
        /// A chimp's base point (replay::chimp_base), slid toward its 0x34 next tile: GameMap's interp to nextPos over
        /// (k7 + 1) engine frames from when the slide came (nextTimeStart), unless the heights differ by more than
        /// 1.25 units; instant at fast-forward. Past half the way, also that tile and its layer delay (k17):
        /// interpChimp sorts the chimp there from then on (nextBaseSortLayer), so one walking behind a building goes
        /// under it. Not the original's: a frame is the sim's measured tick (`tick_s`), and the slide starts where the
        /// chimp was drawn (`Slide::from`), not at the record's position: a galloping horse no longer stops at the end
        /// of each step the sim takes late, or jumps on one it takes early.
        fn chimp_base(p: &Play, id: i32, r: &replay::Rec) -> ([f32; 2], Option<(i32, i32, i32)>) {
            let b = replay::chimp_base(r, p.world.tele.get(&id).map_or(0, |t| t[17]));
            let Some(s) = p.world.next.get(&id) else { return (settle(p, id, b), None) };
            let n = s.n;
            let dt = if p.ff { 0.0 } else { s.dt.unwrap_or(0.0) };
            if dt == 0.0 || (r[5] - n[5]).abs() as f32 / 32.0 > 1.25 { return (b, None); }
            // DE's slide ends after k7 + 1 ticks, but a step in a journey (k7 0x4000) often takes one more and ticks come
            // in at whole frames: a little longer here, so the horse doesn't stop before each step (it can't fall
            // behind: the next slide starts where it is)
            let extra = if r[7] & 0x4000 != 0 { 1.0 } else { 0.5 };
            let f = (p.now.saturating_duration_since(s.at).as_secs_f32() / ((n[7] as f32 + 1.0 + extra) * dt)).clamp(0.0, 1.0);
            let a = s.from.filter(|a| (a[0] - b[0]).hypot(a[1] - b[1]) < 48.0).unwrap_or(b); // farther: a jump, keep it
            let to = replay::sprite_px(&n, n[8], n[9], -n[5]);
            let next = (f > 0.5).then(|| (n[1] as i32, n[2] as i32, n[17] as i32));
            ([a[0] + (to[0] - a[0]) * f, a[1] + (to[1] - a[1]) * f], next)
        }
        /// Not the original's: a chimp without a slide whose position moved a little since the last frame (a step a
        /// crowd blocked, so the record still holds its start; a step that began elsewhere) eases there over ~30 ms
        /// instead of jumping. 200 knights in a crowd jumped some 35 times a second.
        fn settle(p: &Play, id: i32, b: [f32; 2]) -> [f32; 2] {
            let (Some(d), Some(t)) = (p.world.drawn.get(&id), p.world.drawn_at) else { return b };
            let (e, gap) = (p.now.saturating_duration_since(t).as_secs_f32(), (b[0] - d[0]).hypot(b[1] - d[1]));
            if p.ff || e > 0.1 || !(0.25..48.0).contains(&gap) { return b; }
            let k = 1.0 - (-e / 0.03).exp();
            [d[0] + (b[0] - d[0]) * k, d[1] + (b[1] - d[1]) * k]
        }
        let mut p = self.play.take().unwrap();
        // a tile whose graphic rises above its diamond: the engine's building height k18, or over 100 px (buildings,
        // walls, rocks, crops; the ground sets stay under)
        let n = replay::N as i32;
        let tall = |p: &Play, x: i32, y: i32| {
            if x < 0 || y < 0 || x >= n || y >= n { return false; }
            let t = p.world.tiles[(y * n + x) as usize];
            !matches!(t[3], 0 | 38) && (t[18] != 0 || p.gm.tile(t[3], t[4], t[6]).0.is_some_and(|s| self.sprites[s].h > 100.0))
        };
        // sort key = Unity sortingOrder (−20000 + row*49 + sub, CSHARP_NOTES §2 "Depth"): (row + layerDelay, sub), then
        // Unity z = 200 + y + id%10000/10000 (farther = higher on screen = first)
        let mut objs: Vec<((i32, i32, i32, i32), usize, [f32; 2], [f32; 4], [f32; 4], f32)> = vec![]; // .., scale
        let mut drawn = vec![];
        let dt = p.tick_s.max(1.0 / p.rate.max(1.0));
        for s in p.world.next.values_mut() { s.dt.get_or_insert(dt); }
        for (&(_, id), r) in &p.world.objs {
            let (base, next) = if r[0] == 2 { chimp_base(&p, id, r) } else { ([0.0; 2], None) };
            if r[0] == 2 { drawn.push((id, base)); }
            // GameMap.addUpdateOrg / addUpdateChimp / addUpdateBuildingAnim; k5 = -heightAboveGround×32 for org + chimp
            let pos = match r[0] {
                1 => replay::sprite_px(r, 14, 8, -r[5]),
                2 => { // addUpdateChimp: Chimp.position, sliding toward the 0x34 next tile
                    let v = replay::pivot((r[20] & 0xff) as i32);
                    [base[0] + v[0], base[1] + v[1] + r[17] as f32 * 2.0]
                }
                3 => replay::sprite_px(r, r[8], r[9], -r[5]), // addUpdateFly
                6 => replay::sprite_px(r, 14, 8, 0),              // addUpdateWhiteCap: GM 173
                // addUpdateWallFillin (GM 54): + xoffset k8 / 32 units; addUpdatePixie: + (k8, -k9) / 32 units
                7 | 8 | 9 => { let [x, y] = replay::sprite_px(r, 0, 0, if r[0] == 7 { -r[5] } else { 0 }); [x + r[8] as f32 * 2.0, y + if r[0] == 7 { 0.0 } else { r[9] as f32 * 2.0 }] }
                _ => {
                    // anim: base at tile offset 0, then + (k8, -k9)/32 units (y up) + half pixels k17/64, k18/128 units;
                    // a Sassanid Stable's horses at its own stalls (Gm::stall)
                    let Some([sx, sy]) = p.gm.stall(&p.world, r) else { continue };
                    let [x, y] = replay::sprite_px(r, 0, 0, 0);
                    [x + r[8] as f32 * 2.0 + r[17] as f32 + sx, y + r[9] as f32 * 2.0 + r[18] as f32 * 0.5 + sy]
                }
            };
            let Some(alpha) = replay::obj_alpha(r) else { continue };
            let color = [1.0, 1.0, 1.0, alpha];
            // chimp k20 high byte = chopFeet -> material variant chopFeet/4 (<= 6): _SpriteCutoff 0 or (j+4)/20
            let chop = if r[0] == 2 { ((r[20] as u16 >> 8) as i32 / 4).min(6) } else { 0 };
            let mut team = p.gm.team(r[3], r[7], &p.world.colours);
            team[3] = if chop > 0 { (chop + 4) as f32 / 20.0 } else { 0.0 };
            let (delay, flag) = match r[0] { 2 | 4 => if r[12] >= 50 { (r[12] - 50, true) } else { (r[12], false) }, 3 | 8 | 9 => (r[12], false), _ => (0, false) };
            let sub = match r[0] {
                1 => 4 + (r[1] as i32 + r[2] as i32) % 4, // ponytail: C# uses column % 4 (column numbering unknown); any spread does
                2 => 8 + if delay > 0 || flag { 34 } else { 0 },
                3 => 15, // fly / projectile
                6 => 4,  // whitecap
                7 => 2,  // wall fill-in
                8 => 3,  // pixie on a wall
                9 => 41, // pixie on the ground (hiMode): over the units
                _ => if r[5] >= 25 { 1 } else { 16 + r[5] as i32 },
            };
            let mut key = (replay::row(r[2] as usize * replay::N + r[1] as usize) + delay as i32, sub, (pos[1] * 64.0) as i32, id % 10000);
            if let Some((x, y, d)) = next { (key.0, key.1) = (x - y + d, 8 + if d > 0 { 34 } else { 0 }); } // interpChimp
            if r[0] == 2 {
                let (x, y) = next.map_or((r[1] as i32, r[2] as i32), |n| (n.0, n.1));
                key.0 = behind_tall(|x, y| tall(&p, x, y), x, y, key.0);
            }
            let file = match r[0] { 6 => 173, 7 => 54, _ => r[3] };
            let own = if r[0] == 2 { p.gm.lord(file, r[4], r[7]) } else { None }; // a custom lord's own sprite
            // past half its step (`next`), interpChimp shows the body's in-between frame (altFrame1Set). ponytail: the
            // siege engines' turning lerp, which skips it, isn't ported
            match own.or_else(|| p.gm.body_x(file, r[4], next.is_some())) { Some(s) => objs.push((key, s, pos, color, team, 1.0)), None => *p.misses.entry(file).or_default() += 1 }
            // addUpdateFly, gameObjectType 40..47 (the floating numbers over buildings: keep gold, popularity, market):
            // children of the scroll, in Unity units of 64 px: the icon GM 202 frame k17 at x -0.75 (scale 1 for the
            // faces 128..130, 0.5 for the gold 171, else 0.4; sortingOrder +1) and the number k18 + k19·10⁴ in black
            // digits from x 0.95 leftwards, 0.3 apart (+2). ponytail: one alpha for all (C# fades each child 2 steps more)
            if r[0] == 3 && (40..=47).contains(&r[20]) {
                let at = |dx: f32| [pos[0] + dx * 64.0, pos[1]];
                let k = match r[17] { 128..=130 => 1.0, 171 => 0.5, _ => 0.4 };
                if let Some(s) = p.gm.body(202, r[17]) { objs.push(((key.0, key.1 + 1, key.2, key.3), s, at(-0.75), color, [0.0; 4], k)); }
                for (i, img) in info_digits(r[18] as i32 + r[19] as i32 * 10000).into_iter().enumerate() {
                    if let Some(s) = p.gm.body(202, img) { objs.push(((key.0, key.1 + 2, key.2, key.3), s, at(0.95 - 0.3 * i as f32), [0.0, 0.0, 0.0, alpha], [0.0; 4], 1.0)); }
                }
            }
            // the chimp's other sprites (addUpdateChimp), each in its sortingOrder slot after the body (+8):
            // GO2 / GO3 = file k13 (low 11 bits, colour above) frames k14 / k16 at yOffset2 / yOffset3 (+9 / +10), and
            // from record 0x33: siege flag GM 189 k4 at (k1, k2) (+11), health bar / selection GM 16 k10 at (k8, k9)
            // (+12), oil / recharge k11 at (k8 if > 400, k12) (+13), target GM 172 k17 at (k18, k19) (+14)
            if r[0] == 2 {
                let b = base;
                let v = replay::pivot((r[20] & 0xff) as i32);
                let slot = |k: i32| (key.0, key.1 + k, key.2, key.3);
                let mut add = |k: i32, file: i16, img: i16, at: [f32; 2], a: f32, team: [f32; 4]| match p.gm.body(file, img) {
                    Some(s) if img > 0 => objs.push((slot(k), s, at, [1.0, 1.0, 1.0, a], team, 1.0)),
                    _ => {}
                };
                let (f2, rope) = (r[13] & 0x7ff, p.world.tele.get(&id).map_or(0, |t| t[19]));
                add(1, f2, r[14], [b[0] + v[0], b[1] + v[1] + (r[18] - rope) as f32 * 2.0], alpha, p.gm.team(f2, r[13] >> 11, &p.world.colours));
                add(2, f2, r[16], [b[0] + v[0], b[1] + v[1] + r[19] as f32 * 2.0], alpha, [0.0; 4]);
                if let Some(m) = p.world.marks.get(&id).copied() {
                    let at = |x: i16, y: i16| [b[0] + x as f32 * 2.0, b[1] + y as f32 * 2.0];
                    add(3, 189, m[4], at(m[1], m[2]), alpha, p.gm.team(189, m[7], &p.world.colours));
                    add(4, 16, m[10], at(m[8], m[9]), 1.0, [0.0; 4]);
                    if m[11] > 400 { add(5, 16, m[11] - 400, at(m[8], m[12]), 1.0, [0.0; 4]) }
                    else { add(5, 172, m[11], at(0, m[12]), if m[11] == 332 { 0.5 } else { 1.0 }, [0.0; 4]) }
                    add(6, 172, m[17], at(m[18], m[19]), 1.0, [0.0; 4]);
                }
            }
        }
        // King of the Hill (koth.rs): a standing flag (GM 76, the pole's foot at the pivot) on the hill's centre, waving
        // (frames 8..19): on the tower's top in its owner's colour (neutral gold for the hill player's), else on the
        // ground in the holder's
        if let Some(b) = p.huds.get(p.at.saturating_sub(1)).and_then(|h| h.hill.as_ref()) {
            let (wx, wy) = replay::to_west_pub(p.world.rot, b.hill.x as i16, b.hill.y as i16);
            let i = (wy.clamp(0, replay::N as i16 - 1) as usize) * replay::N + wx.clamp(0, replay::N as i16 - 1) as usize;
            let [x, y] = replay::cell_px(wx, wy);
            let r = &p.world.tiles[i];
            let (owner, lift) = match b.fort {
                0 => ((1..9u8).find(|&q| b.hill.holder > 0 && b.team[q as usize] == b.hill.holder).unwrap_or(0), 0.0),
                f => (if f == b.hill.keeper { 0 } else { f as u8 }, b.height as f32 * 2.0), // on the tower's top
            };
            let frame = 9 + (self.start.elapsed().as_secs_f32() * 10.0) as i16 % 12;
            // over the tower (its roof too): sorted after the footprint's front corner
            let front = [(-2, -2), (3, -2), (-2, 3), (3, 3)].map(|(dx, dy)| { let (a, c) = replay::to_west_pub(p.world.rot, (b.hill.x + dx) as i16, (b.hill.y + dy) as i16); a as i32 - c as i32 });
            let row = if b.fort > 0 { front.into_iter().max().unwrap_or(0) + 1 } else { replay::row(i) };
            if let Some(s) = p.gm.body(76, frame) {
                objs.push(((row, 41, 0, 0), s, [x, y - r[5] as f32 * 2.0 - lift], [1.0; 4], p.gm.team(76, owner as i16, &p.world.colours), 1.0));
            }
        }
        objs.sort_by_key(|o| o.0);
        p.objs.clear();
        p.obj_rows.clear();
        for (key, s, pos, color, team, k) in objs {
            self.push_c(&mut p.objs, s, pos, k, color, team);
            p.obj_rows.resize(p.objs.len(), key.0);
        }
        p.world.drawn.extend(drawn);
        p.world.drawn_at = Some(p.now);
        self.play = Some(p);
    }

    /// Advance the replay by `dt` seconds (or `steps` ticks); upload changed tiles, rebuild objects.
    fn tick_replay(&mut self, dt: f32, steps: usize) {
        #[cfg(feature = "live")]
        if let Some(l) = self.rp.as_mut().and_then(|c| c.poll()) { // a replay seek is in: run on from there
            let keep = self.play.as_ref().map(|p| (p.playing, p.rate));
            self.start_live(l, None);
            if let (Some(p), Some((on, r))) = (&mut self.play, keep) { (p.playing, p.rate) = (on, r); }
        }
        #[cfg(feature = "live")]
        if let (Some(c), Some(p)) = (&mut self.rp, &mut self.play) { c.update(&mut p.rate); } // a fast-forward got there
        if std::mem::take(&mut self.end.save) { // the end screen's Save Replay
            if let Some(l) = self.play.as_ref().and_then(|p| p.live.as_ref()) { l.send(live::Cmd::SaveReplay); }
        }
        if let Some(on) = self.end.graphs.take() { self.stats.on = on; } // the score screen's Graphs
        let Some(p) = self.play.as_mut() else { return };
        if self.end.scoring() { p.playing = false; } // the score screen stops the game (endgame.rs)
        let mut n = steps;
        // at most 0.1 s of ticks per frame: a long frame (load, window drag) must not turn into a catch-up burst
        let rate = if p.ff { f32::INFINITY } else { p.rate };
        if p.playing { p.acc += dt.min(0.1) * rate.min(2000.0); n += p.acc as usize; p.acc = p.acc.fract(); }
        let mut dirty = std::mem::take(&mut p.dirty);
        let mut applied = false;
        if let Some(l) = p.live.as_mut() { // live: the sim thread paces itself; take the ticks it finished
            l.send(live::Cmd::Run { playing: p.playing, rate });
            if steps > 0 { l.send(live::Cmd::Step); }
            if let Some(g) = &self.gfx {
                // radar camera rectangle: game tile at the window centre, tiles across (GameMap.screenTilesWide = px/64)
                let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                let [cx, cy] = self.cam.centre_flat([sz.width / 2.0, sz.height / 2.0], l.rot);
                let c = &self.cam;
                let rect = [-c.off[0] / c.zoom, -c.off[1] / c.zoom, (sz.width - c.off[0]) / c.zoom, (sz.height - c.off[1]) / c.zoom];
                let (pos, [tw, th]) = c.screen_view(sz.width, sz.height, g.window.scale_factor() as f32);
                l.send(live::Cmd::ViewPos(pos));
                l.send(live::Cmd::View([cx, cy, tw, th], rect));
                audio::tick(|x, y| { let (x, y) = replay::to_west_pub(l.rot, x as i16, y as i16); let w = replay::cell_px(x, y); [c.off[0] + w[0] * c.zoom, c.off[1] + w[1] * c.zoom] }, sz.width, sz.height);
            }
            for f in l.frames() {
                // map turned: the engine resets its draw cache and re-sends every object it shows (a building looks
                // different from the new side), so drop the old ones; pick heights refill in the new frame
                if f.rot != p.world.rot { p.world.objs.clear(); self.cam.heights.clear(); }
                p.world.apply(&f.recs, &mut dirty);
                // the tutorial panel's text and buttons (render record 23); EditorDirector.tutorialAppModes: the reports
                // menu's pages (app mode 16, sub mode 71 / 72) once entered
                if let Some(t) = &mut self.tutorial {
                    for r in f.recs.iter().filter(|r| r[0] == 23) { t.record(r) }
                    let mode = (p.huds[0].app_mode as i32, p.huds[0].app_sub_mode as i32);
                    if mode != std::mem::replace(&mut t.mode, mode) {
                        match mode { (16, 71) => l.send(live::Cmd::Tutorial(14, 1)), (16, 72) => l.send(live::Cmd::Tutorial(14, 72)), _ => {} }
                    }
                }
                p.huds[0] = f.hud;
                applied = true;
                // trail mission won: GameData's trail branch (best time, next mission unlocked), once
                if let (Some((t, id, d, false)), Some(1)) = (self.trail, p.huds[0].end.as_ref().map(|e| e.result)) {
                    menu::trail_menu::record_win(t, id, p.huds[0].month as i32, p.huds[0].year as i32, f.tick);
                    self.trail = Some((t, id, d, true));
                }
                // campaign mission won: GameData's progress update (next mission unlocked, best difficulty kept)
                if let (Some((id, d, false)), Some(1)) = (self.campaign, p.huds[0].end.as_ref().map(|e| e.result)) {
                    let mut pr = skirmish::Progress::load();
                    pr.won(id, d);
                    if let Err(e) = pr.save() { eprintln!("campaign progress: {e}"); }
                    self.campaign = Some((id, d, true));
                }
                // minimap click (GameAction RadarClicked → cameraMovedRecalcBounds): centre on that tile
                if let (Some((x, y)), Some(g)) = (f.goto, &self.gfx) {
                    let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                    self.cam.centre_on(sz.width, sz.height, &p.world, f.rot, x, y);
                }
                if let (Some(r), Some(h), Some(u)) = (f.radar, self.gfx.as_ref().and_then(|g| g.hud.as_ref().map(|h| (g, h))), self.ui.as_mut()) {
                    let (g, h) = h;
                    g.queue.write_texture(h.radar.as_image_copy(), bytemuck::cast_slice(&r),
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(live::RADAR_N as u32 * 4), rows_per_image: None }, h.radar.size());
                    u.radar = true;
                }
            }
            // the sim's real tick length (chimp_base): a running average over the ticks that came, per speed
            if applied {
                let now = Instant::now();
                match p.tick_mark {
                    Some((t0, k0, r)) if r == rate && p.playing => {
                        let s = (now - t0).as_secs_f32() / (l.tick - k0).max(1) as f32;
                        if l.tick > k0 && s < 0.25 { p.tick_s += (s - p.tick_s) * 0.1; }
                    }
                    _ => p.tick_s = 1.0 / rate,
                }
                p.tick_mark = Some((now, l.tick, rate));
            }
            n = 0;
        }
        for _ in 0..n {
            if p.at >= p.ticks.len() { p.playing = false; break; }
            p.world.apply(&p.ticks[p.at], &mut dirty);
            p.at += 1;
            applied = true;
        }
        if !applied { p.dirty = dirty; return; }
        dirty.sort_unstable();
        dirty.dedup();
        let mut p = self.play.take().unwrap();
        if self.cam.heights.is_empty() {
            (self.cam.heights, self.cam.bld) = p.world.tiles.iter().map(|r| self.pick_heights(&p, r)).unzip();
        }
        if self.cam.map.is_none_or(|m| m.rot != p.world.rot) { self.cam.map = MapBox::of(&p.world); } // camera limits
        for &i in &dirty { if let Some(r) = p.world.tiles.get(i) { (self.cam.heights[i], self.cam.bld[i]) = self.pick_heights(&p, r); } }
        if let Some(g) = &self.gfx {
            let buf = &g.static_buf.as_ref().unwrap().0;
            // one write per run of consecutive slots: per-tile writes (a staging buffer each) stall on busy ticks
            let mut slots: Vec<(u32, usize)> = dirty.iter().filter(|&&i| p.slot[i] != u32::MAX).map(|&i| (p.slot[i], i)).collect();
            slots.sort_unstable();
            let mut k = 0;
            while k < slots.len() {
                let mut e = k + 1;
                while e < slots.len() && slots[e].0 == slots[e - 1].0 + 1 { e += 1; }
                let tris: Vec<[Vert; 3]> = slots[k..e].iter().flat_map(|&(_, i)| self.tile_tris(&mut p, i).map(|t| t.1)).collect();
                g.queue.write_buffer(buf, slots[k].0 as u64 * 4 * std::mem::size_of::<[Vert; 3]>() as u64, bytemuck::cast_slice(&tris));
                k = e;
            }
            // only on play / pause / rate change and paused steps: macOS set_title blocks the main thread 2-190 ms
            // (the 80-190 ms frame spikes); the HUD shows the running tick
            let key = (p.playing, rate.to_bits(), if p.playing { -1 } else { p.live.as_ref().map_or(p.at as i64, |l| l.tick as i64) });
            if p.title != key {
                p.title = key;
                let pause = if p.playing { "" } else { " (paused)" };
                g.window.set_title(&match &p.live {
                    Some(l) => format!("Strong Hold - sim tick {} @ {:.0}/s{pause}", l.tick, rate),
                    None => format!("SHC DE replay - tick {}/{} @ {:.0}/s{pause}", p.at - 1, p.ticks.len(), rate.min(2000.0)),
                });
            }
        }
        dirty.clear();
        p.dirty = dirty;
        self.play = Some(p);
        self.rebuild_objs();
    }

    /// Left button press / release at the cursor: HUD first, then the --live build tool, else start / stop panning.
    fn left_button(&mut self, down: bool) {
        if self.editor_button(down) { return; } // the map editor (menu/editor.rs)
        #[cfg(feature = "live")]
        if down && self.lan_click(self.cam.cursor) { return; } // the LAN chat panel (lan.rs)
        let on_hud = down && self.ui.as_mut().is_some_and(|u| u.click(self.cam.cursor));
        if !down { if let Some(u) = self.ui.as_mut() { u.drag = None; } } // a dragged HUD slider lets go
        // --live with a building picked in the build menu: a map press is DLL_MapAction mouse state 1 (no pan);
        // the sim validates it and queues the chore only on a valid, affordable spot
        let mapper = self.ui.as_ref().and_then(|u| u.selected_mapper());
        let tool = !on_hud && match (mapper, self.play.as_mut().and_then(|p| p.live.as_mut())) {
            (Some(mp), Some(l)) => {
                let t = self.cam.tile_at(l.rot);
                if live::is_drag(mp) {
                    // DLL_MapAction press (1) / release (3); the moves in between go out from build_ghost (2)
                    if down { self.drag_from = Some(t); l.send(live::Cmd::Drag(1, t[0], t[1])); }
                    else if self.drag_from.take().is_some() { l.send(live::Cmd::Drag(3, t[0], t[1])); }
                    l.hover_at = t;
                } else if down {
                    l.send(live::Cmd::Click(t[0], t[1]));
                }
                true
            }
            // no build tool: press / release on the map is DLL_TroopSelection (building / unit selection, the troop
            // box: a left drag selects own units; no pan in --live: middle drag / arrows pan)
            (None, Some(l)) => {
                let c = &self.cam;
                let w = [(c.cursor[0] - c.off[0]) / c.zoom, (c.cursor[1] - c.off[1]) / c.zoom];
                if down { if !on_hud { l.press(w, c.cursor); } } else { l.release(); }
                !on_hud
            }
            _ => false,
        };
        self.cam.drag = down && !on_hud && !tool;
    }

    /// SHC_SHOT runs ignore the real mouse; SHC_INPUT scripts it instead: `frame:m:x,y` moves the cursor, `frame:d` /
    /// `frame:u` press / release the left button (logical px), `frame:e` presses Escape, `frame:w:W,H` resizes the window, `frame:k:c` types the in-game key c (e.g. `+`, `p`), `frame:r` right-clicks (--live, SH1 controls: drops the troop selection), `frame:rd` / `frame:ru` press / release the right button (right-click menu), `frame:z:-` / `frame:z:+` is a mouse-wheel zoom notch, `frame:Kd:W` / `frame:Ku:W` press / release a key (held scroll), entries separated by `;`.
    fn scripted_input(&mut self, el: &ActiveEventLoop) {
        let Ok(script) = std::env::var("SHC_INPUT") else { return };
        for e in script.split(';') {
            let mut f = e.split(':');
            if f.next().and_then(|v| v.parse::<u32>().ok()) != Some(self.frames) { continue; }
            let (a, v) = (f.next(), f.next());
            if let (Some(d @ ("d" | "u")), Some(u)) = (a, self.ui.as_mut()) { u.down = d == "d"; }
            match (a, v.and_then(|v| v.split_once(','))) {
                (Some("m"), Some((x, y))) => (self.cam.cursor, self.cam.cursor_in) = ([x.parse().unwrap(), y.parse().unwrap()], true),
                (Some("d"), _) if self.menu.is_some() => if let Some(go) = self.menu.as_mut().unwrap().click(self.cam.cursor) { self.menu_go(go, el) },
                (Some("u"), _) if self.menu.is_some() => {}
                (Some("r"), _) if self.menu.is_some() => self.menu.as_mut().unwrap().right_click(self.cam.cursor),
                (Some("e"), _) => self.escape(el),
                // frame:w:W,H resizes the window (logical px), for layout checks
                (Some("w"), Some((w, h))) => if let Some(g) = &self.gfx { let _ = g.window.request_inner_size(LogicalSize::new(w.parse::<f32>().unwrap(), h.parse::<f32>().unwrap())); },
                // a typed key: the map editor's name box while it is open
                (Some("k"), _) if self.menu.is_none() && self.editor.as_ref().is_some_and(|e| e.typing) =>
                    if let Some(e) = &mut self.editor {
                        use winit::keyboard::{Key, NamedKey};
                        e.key(&if v == Some("BS") { Key::Named(NamedKey::Backspace) } else { Key::Character(v.unwrap_or("").into()) })
                    },
                // a typed key: in a live game through the hotkey table ("+" / "-" = the keypad's), else the old keys
                (Some("k"), _) if self.menu.as_ref().is_some_and(|m| m.capturing()) => {
                    if let Some(go) = keys::char_code(v.unwrap_or(""), false).and_then(|k| self.menu.as_mut().unwrap().key(&k)) { self.menu_go(go, el) }
                }
                (Some("k"), _) => if self.menu.is_none() {
                    let c = v.unwrap_or("");
                    let pad = matches!(c, "+" | "-");
                    match keys::char_code(c, pad).filter(|_| self.play.as_ref().is_some_and(|p| p.live.is_some())) {
                        Some(k) => { self.key_event(&k, Some(c), true, false, el); self.key_event(&k, None, false, false, el) }
                        None => self.char_key(c),
                    }
                },
                // frame:K:Return: a Unity key name (keys.rs) pressed and released in a live game, e.g. Return, Space, Alpha1,ctrl
                (Some("K"), _) => if self.menu.is_none() { let k = v.unwrap_or(""); self.key_event(k, None, true, false, el); self.key_event(k, None, false, false, el) },
                // frame:Kd:W / frame:Ku:W: press / release only (a held scroll key)
                (Some("Kd"), _) => if self.menu.is_none() { self.key_event(v.unwrap_or(""), None, true, false, el) },
                (Some("Ku"), _) => if self.menu.is_none() { self.key_event(v.unwrap_or(""), None, false, false, el) },
                #[cfg(feature = "live")]
                (Some("d"), _) if self.rp.as_ref().is_some_and(|c| c.over(self.cam.cursor)) => if let (Some(c), Some(p)) = (&mut self.rp, &mut self.play) { c.click(self.cam.cursor, &mut p.playing, &mut p.rate) },
                (Some("d"), _) if self.menu.is_none() && self.on_tutorial() => self.tutorial_click(el),
                (Some("d"), _) if self.end.over(self.cam.cursor) => if let Some(e) = self.end.click(self.cam.cursor) { self.end_exit(e, el) },
                (Some("d"), _) => self.left_button(true),
                (Some("u"), _) => self.left_button(false),
                (Some("z"), _) if self.menu.is_some() => self.menu.as_mut().unwrap().wheel(if v == Some("+") { 1.0 } else { -1.0 }),
                (Some("z"), _) => self.adjust_zoom(if v == Some("+") { 0.5 } else { -0.5 }), // a mouse-wheel notch in / out
                (Some("rd"), _) => self.right_button(true),
                (Some("ru"), _) => self.right_button(false),
                (Some("r"), _) => if let Some(l) = self.play.as_ref().and_then(|p| p.live.as_ref()) { // right click: troop order
                    l.send(live::Cmd::Troops(live::TroopIn::Right(true)));
                    l.send(live::Cmd::Troops(live::TroopIn::Right(false)));
                },
                _ => panic!("SHC_INPUT entry {e:?}"),
            }
        }
    }

    /// A live game's key (Unity KeyCode name with modifiers, `keys.rs`): the scroll keys while held, a press runs its
    /// bound function (`hotkey`); a key bound to nothing falls back to the viewer's own keys (`char_key`, typed `c`).
    fn key_event(&mut self, k: &str, c: Option<&str>, pressed: bool, repeat: bool, el: &ActiveEventLoop) {
        #[cfg(feature = "live")]
        if self.lan_key(k, c, pressed) { return; } // a LAN game's chat line (lan.rs)
        let code = k.split(',').next().unwrap_or("");
        for (i, f) in ["Left", "Right", "Up", "Down"].into_iter().enumerate() {
            if (pressed && self.keys.action(k) == Some(f)) || (!pressed && self.keys.on_code(f, code)) { self.cam.keys[i] = pressed; }
        }
        if !pressed || repeat { return; }
        match self.keys.action(k).map(str::to_string) {
            // Shift+F1 / Shift+F2: the original's save / load requesters = the port's Cmd+S (pause menu Save) / Load Game
            // ponytail: no file-name prompt (Save names the file by the clock), Load also opens in a LAN game
            Some(f) if f == "Save" || f == "Load" => self.cmd(if f == "Save" { ux::Cmd::Save } else { ux::Cmd::Load }, el),
            Some(f) => self.hotkey(&f),
            None => if let Some(c) = c { self.char_key(c) },
        }
    }

    /// KeyManager.Update's in-game functions (keys.rs: the table, `game_action` for the DLL_GameAction ones, PLACE).
    /// Not ported: FreeBuildEvents (F1 = the port's key sheet), ToggleGoods / ToggleObjectives, ToggleHealthBars (no HP
    /// bars yet), RadarZoom, insults, cheats, editor keys.
    fn hotkey(&mut self, f: &str) {
        #[cfg(feature = "live")]
        if f == "Pause" && self.lan_pause() { return; } // a LAN game: pause / resume for everyone (lan.rs)
        // the screen-centre tile (SetBookmark: CalcMapTileFromMousePos at the centre, no building heights)
        let centre = match (&self.gfx, self.play.as_ref().and_then(|p| p.live.as_ref())) {
            (Some(g), Some(l)) => { let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor()); Some(self.cam.centre_ground([sz.width / 2.0, sz.height / 2.0], l.rot)) }
            _ => None,
        };
        let Some(p) = self.play.as_mut() else { return };
        let (paused, tick) = (!p.playing, p.live.as_ref().map_or(0, |l| l.tick));
        let st = p.huds.first();
        let ctx = keys::Ctx { mode: st.map_or(0, |h| h.app_mode), sub: st.map_or(0, |h| h.app_sub_mode), attack_here: st.is_some_and(|h| h.attack_here > 0),
            sh1_centre: skirmish::Settings::load().flag("SH1CentreControls", true), centre: centre.filter(|t| t.iter().all(|&v| (0..replay::N as i32).contains(&v))) };
        let place = keys::PLACE.iter().find(|q| q.0 == f).map(|q| q.1);
        let available = |m: i32| st.is_none_or(|h| h.available.get(m as usize).copied().unwrap_or(true));
        match f {
            "Pause" => { p.playing = !p.playing; self.tick_replay(0.0, 0) }
            // F5 / F9 (and Home): Saves/Quicksave.sav, written in the background (saves.rs)
            "QuickSave" => if let Some(l) = &p.live { l.send(live::Cmd::Save(saves::quick_path())) },
            #[cfg(feature = "live")]
            "QuickLoad" => { let q = saves::quick_path(); if q.exists() { let _ = self.load_in_background(menu::Go::Load(q)); } }
            "ToggleStats" => self.stats.on = !self.stats.on, // the port's stats overlay (stats.rs), F3 by default (keys.rs EXTRA)
            // a LAN game: the host sets it for everyone (lan.rs)
            "IncreaseEngineSpeed" | "DecreaseEngineSpeed" => { let r = live::speed_step(p.rate, f == "IncreaseEngineSpeed"); if !lan::ask_speed(r) { p.rate = r } }
            // X / Z zoom in / out, Q / E rotate the map left / right (not while paused)
            "ZoomIn" | "ZoomOut" if p.playing => self.adjust_zoom(if f == "ZoomIn" { 0.5 } else { -0.5 }),
            "MapRotateLeft" | "MapRotateRight" if p.playing => self.rotate_map(f == "MapRotateRight"),
            // Space: EngineInterface.toggleFlattenedLandscapeMode (the right-click menu's flatten), not while paused
            "FlattenLandscape" if !paused => if let (Some(u), Some(l)) = (self.ui.as_mut(), &p.live) {
                u.flat = !u.flat;
                l.send(live::Cmd::Flatten(u.flat));
                let t = if u.flat { 4 } else { 5 };
                self.tutorial_action(t, -1);
            },
            // Alt+Z: MainControls.toggleUIVisibility (the right-click menu's left swipe brings it back too)
            "ToggleUI" => if let Some(u) = self.ui.as_mut() {
                u.hidden = !u.hidden;
                if u.hidden { self.tutorial_action(17, -1) }
            },
            // F: EditorDirector.toggleOSTFrameRate, " N fps " / " N sim " top right (`fps_text`)
            "ToggleFrameRate" => self.fps = match self.fps { Some(_) => None, None => Some((Instant::now(), 0, tick, [String::new(), String::new()])) },
            // the port's Ctrl+M (keys.rs EXTRA): every channel silent / the Options > Sound volumes again.
            // ponytail: moving a volume slider while muted unmutes
            "ToggleMuteSounds" => {
                static MUTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                let mute = !MUTED.fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                let (m, mu, sp, us, sfx, red) = skirmish::Settings::load().volumes();
                audio::set_volumes(if mute { 0.0 } else { m }, mu, sp, us, sfx, red);
                self.ux.note(if mute { "Sound off" } else { "Sound on" });
            }
            // Tab with a build tool: GameAction RotateBuilding (MainControls.CurrentAction 3 / 5), the ghost again turned
            "RotateBuilding" if !paused => if let (Some(l), true) = (p.live.as_mut(), self.ui.as_ref().is_some_and(|u| u.selected_mapper().is_some())) {
                l.send(live::Cmd::Action([1049, 0, 0]));
                l.hover_at = [i32::MIN; 2];
            },
            // build hotkeys (C = walls): that tool, if the mission offers it (IsMapperAvailable), not while paused
            _ if place.is_some() => if let (false, Some(m), Some(u)) = (paused, place.filter(|&m| available(m)), self.ui.as_mut()) { u.pick_mapper(m) },
            // the rest: one DLL_GameAction (camera keys centre on its answer, live.rs)
            _ => if let (Some(a), Some(l)) = (keys::game_action(f, &ctx), &p.live) { l.send(live::Cmd::Action(a)) },
        }
    }

    /// ToggleFrameRate's two lines (OnScreenText OST_FRAMERATE: CurrentFPS / CurrentTicks per second), top right;
    /// counted here every frame, the text redone once a second.
    fn fps_text(fps: &mut Option<(Instant, u32, i32, [String; 2])>, ui: Option<&mut hud::Ui>, tick: i32, w: f32, mut hud: Vec<hud::HudTri>) -> Vec<hud::HudTri> {
        let (Some((t0, n, tick0, text)), Some(u)) = (fps.as_mut(), ui) else { return hud };
        *n += 1;
        let dt = t0.elapsed().as_secs_f32();
        if dt >= 1.0 {
            *text = [format!(" {} fps ", (*n as f32 / dt).round()), format!(" {} sim ", ((tick - *tick0) as f32 / dt).round())];
            (*t0, *n, *tick0) = (Instant::now(), 0, tick);
        }
        // below the port's tick label (hud.rs, top right)
        for (i, t) in text.iter().enumerate() { u.text_sh(&mut hud, t, w - 10.0 * u.s, (62.0 + 28.0 * i as f32) * u.s, 22.0, 1.0, [1.0; 4]); }
        hud
    }

    /// The replay viewer's keys, and in a live game the viewer's own keys on keys the table leaves free.
    fn char_key(&mut self, c: &str) {
        let Some(p) = self.play.as_mut() else { return };
        let live = p.live.is_some();
        match c {
            "." => { p.playing = false; self.tick_replay(0.0, 1) }
            // replays: + / - speed, P pause (a live game has them in the hotkey table)
            "+" | "=" | "-" if !live => p.rate = live::speed_step(p.rate, c != "-"),
            "p" if !live => { p.playing = !p.playing; self.tick_replay(0.0, 0) }
            // --live: tax up/down (t/g), rations cycle (r) -> Command::Tax / Command::Rations
            "t" | "g" | "r" if p.live.is_some() => {
                let l = p.live.as_ref().unwrap();
                l.send(match c { "t" => live::Cmd::Tax(1), "g" => live::Cmd::Tax(-1), _ => live::Cmd::Rations });
            }
            _ => {}
        }
    }

    /// PerfectPixelWithZoom.adjustZoom in game (map locked, ExtraZoom on): zoomPos ±`d` within 1..=3 (2..=3 when the
    /// window is over 2560×1440 device px, CanUserExtraZoom), then a 0.2 s glide to that level about the window centre.
    /// ponytail: CanUserExtraZoom's small-map limits (160 / 200 / 300) and capZoom (map wider than the screen) skipped.
    fn adjust_zoom(&mut self, d: f32) {
        self.tutorial_action(3, -1); // PerfectPixelWithZoom.adjustZoom
        let Some(g) = &self.gfx else { return };
        let (w, h, sf) = (g.config.width as f32, g.config.height as f32, g.window.scale_factor() as f32);
        let c = &mut self.cam;
        c.zoom_pos = zoom_pos_clamp(c.zoom_pos + d, w, h, c.map.map_or(800.0, |m| m.size));
        c.zoom_anim = Some((c.zoom, ZOOM_LEVELS[(c.zoom_pos * 2.0).round() as usize] / sf, Instant::now()));
    }

    /// --live build tool: tell the sim thread about tool changes (DLL_StartMapAction) and the hovered tile (DLL_MapAction
    /// state 0), and turn its verdict (or a drag line) into a screen-space ghost. None = no tool / cursor on the HUD.
    fn build_ghost(&mut self) -> Option<hud::Ghost> {
        let u = self.ui.as_ref()?;
        let l = self.play.as_mut()?.live.as_mut()?;
        let want = u.selected_mapper();
        self.cam.use_bld = bld_pick(want);
        if want != l.tool {
            l.send(live::Cmd::Tool(want.unwrap_or(0)));
            (l.tool, l.hover_at) = (want, [i32::MIN; 2]);
            self.drag_from = None;
        }
        let mp = want?;
        if u.over(self.cam.cursor) && self.drag_from.is_none() { return None; }
        if mp == 0x27 { return None; } // the demolish tool: no placement ghost, the delete cursor shows it (cursor.rs)
        let (rot, t) = (l.rot, self.cam.tile_at(l.rot));
        let c = &self.cam;
        let dia = |x: i32, y: i32| {
            let (wx, wy) = replay::to_west_pub(rot, x as i16, y as i16);
            let [cx, cy] = replay::cell_px(wx, wy);
            let (sx, sy, z) = (c.off[0] + cx * c.zoom, c.off[1] + cy * c.zoom, c.zoom);
            [[sx, sy - 16.0 * z], [sx + 32.0 * z, sy], [sx, sy + 16.0 * z], [sx - 32.0 * z, sy]]
        };
        if t != l.hover_at {
            l.send(if self.drag_from.is_some() { live::Cmd::Drag(2, t[0], t[1]) } else { live::Cmd::Hover(t[0], t[1]) });
            l.hover_at = t;
        }
        // until the sim answers for this tool, show nothing
        let v = l.verdict().filter(|v| v.tool == mp)?;
        // tools the sim runs draw their own ghost into the preview layer (building sprite, wall line, moat brush, at
        // the right height): only the reason text here; the outline is for tools it can't run
        if v.why != live::NOT_PORTED { return Some(hud::Ghost { tiles: vec![], why: v.text() }); }
        let tiles = (0..v.size * v.size).map(|k| (dia(v.x + k % v.size, v.y + k / v.size), v.ok)).collect();
        Some(hud::Ghost { tiles, why: v.text() })
    }

    /// Sprite `idx` with its pivot at screen `pos` (logical px, y down), scaled by `k`.
    fn push(&self, out: &mut Vec<Tri>, idx: usize, pos: [f32; 2], k: f32) { self.push_c(out, idx, pos, k, [1.0; 4], [0.0; 4]) }

    /// `push` with a vertex colour (multiplied in) and team colour (applied through the atlas mask).
    fn push_c(&self, out: &mut Vec<Tri>, idx: usize, pos: [f32; 2], k: f32, color: [f32; 4], team: [f32; 4]) {
        let s = &self.sprites[idx];
        let a = &self.atlases[s.atlas as usize];
        let (ox, oy) = (s.x + s.px * s.w, s.y + s.py * s.h); // pivot in atlas px
        // Unity texture rows start at the image bottom, so atlas y maps straight to v (no flip).
        let v = |m: &[f32; 2]| Vert { pos: [pos[0] + m[0] * k, pos[1] - m[1] * k], uv: [(ox + m[0]) / a.w as f32, (oy + m[1]) / a.h as f32], color, team };
        out.extend(s.mesh.chunks_exact(3).map(|t| (s.atlas, [v(&t[0]), v(&t[1]), v(&t[2])])));
    }

    fn build_tris(&self, t: f32) -> Vec<Tri> {
        let f = (t * FPS) as usize;
        let mut out = Vec::new();
        for (i, frames) in self.cells.iter().enumerate() {
            let (c, r) = (i % COLS, i / COLS);
            let anchor = [c as f32 * CELL + CELL * 0.5, r as f32 * CELL + CELL * 0.85];
            let s = &self.sprites[frames[f % frames.len()]];
            self.push(&mut out, frames[f % frames.len()], anchor, (CELL * 0.9 / s.w.max(s.h)).min(1.0));
        }
        self.push(&mut out, self.hero[f % self.hero.len()], [COLS as f32 * CELL + 170.0, ROWS as f32 * CELL * 0.75], 3.0);
        out.sort_by_key(|(a, _)| *a);
        out
    }
}

/// The camera without an own keep (campaign mission 1, Free Build): the engine's view tile (view object +0x7c / +0x80,
/// set by setup.rs world_post_load -> FUN_180045310; on the keep in a skirmish), west frame.
#[cfg(feature = "live")]
fn engine_view(m: &shc_sim::mem::Mem) -> Option<(i16, i16)> {
    let (x, y, rot) = (m.i32(0x183a0fd50 + 0x7c), m.i32(0x183a0fd50 + 0x80), m.i32(shc_sim::terrain::ROT) as i16);
    ((0..replay::N as i32).contains(&x) && (0..replay::N as i32).contains(&y)).then(|| replay::to_west_pub(rot, x as i16, y as i16))
}

/// A scripted run (SHC_SHOT / SHC_INPUT): clicks and the shot go by frame number, so loading stays synchronous.
fn scripted() -> bool { std::env::var_os("SHC_SHOT").is_some() || std::env::var_os("SHC_INPUT").is_some() }

/// Borderless fullscreen, or a window of Options' Resolution (menu::window_size), else 80 % of the screen.
fn set_windowed(window: &Window, windowed: bool) {
    window.set_fullscreen((!windowed).then_some(Fullscreen::Borderless(None)));
    if windowed {
        if let Some([w, h]) = menu::window_size() { let _ = window.request_inner_size(LogicalSize::new(w, h)); }
        else if let Some(m) = window.current_monitor() { let s = m.size(); let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(s.width * 4 / 5, s.height * 4 / 5)); }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() { return; }
        // like the original's first run (Unity): fullscreen at the screen's size. SHC_SHOT runs and --sheet keep the old
        // 1684×784 window (scripted SHC_INPUT coordinates assume it); SHC_WINDOW=WxH (logical px) or =full overrides.
        let attrs = Window::default_attributes().with_title("Strong Hold");
        let fixed = std::env::var_os("SHC_SHOT").is_some() || self.sheet;
        let attrs = match std::env::var("SHC_WINDOW").ok().as_deref().map(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))) {
            Some(Some((w, h))) => attrs.with_inner_size(LogicalSize::new(w, h)).with_position(LogicalPosition::new(0, 0)),
            None if fixed => attrs.with_inner_size(LogicalSize::new(COLS as f32 * CELL + 340.0, ROWS as f32 * CELL)).with_position(LogicalPosition::new(40, 60)),
            // Options > Screen Mode (settings.cfg FullscreenType 3 = windowed)
            _ if skirmish::Settings::load().num("FullscreenType", 1.0) as i32 == 3 => attrs,
            _ => attrs.with_fullscreen(Some(Fullscreen::Borderless(None))),
        };
        let window = Arc::new(el.create_window(attrs).unwrap());
        if !fixed && std::env::var_os("SHC_WINDOW").is_none() && window.fullscreen().is_none() { set_windowed(&window, true); }
        // Options > Sound: the saved volumes (ConfigSettings; the game's defaults without a settings.cfg)
        let (m, mu, sp, us, sfx, red) = skirmish::Settings::load().volumes();
        audio::set_volumes(m, mu, sp, us, sfx, red);
        self.cursors = cursor::Cursors::new(el);
        // the HUD atlas is uploaded whenever the UI export exists: the menu can start a game later
        let with_hud = self.ui.is_some() || cache_dir().join("ui/UI-MasterAtlas.bc").exists();
        self.gfx = Some(pollster::block_on(init_gfx(window, &self.atlases, with_hud)));
        self.ux_resumed(); // menu bar, UI scale, Retina factor (ux.rs)
        self.upload_scene();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        self.event(el, ev)
    }

    // menu-bar commands, throttled frames while hidden / in the background (ux.rs)
    fn about_to_wait(&mut self, el: &ActiveEventLoop) { self.ux_about_to_wait(el) }
}

impl App {
    /// The built scene (map / sheet / game tiles) -> static vertex buffer; camera fit / focus.
    fn upload_scene(&mut self) {
        let Some(g) = self.gfx.as_mut() else { return };
        if let Some(scene) = self.scene.take() {
            let raw: Vec<[Vert; 3]> = scene.iter().map(|(_, t)| *t).collect();
            let buf = g.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene"), contents: bytemuck::cast_slice(&raw), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST });
            // fit the scene's bounding box into the window
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for v in raw.iter().flatten() { for k in 0..2 { lo[k] = lo[k].min(v.pos[k]); hi[k] = hi[k].max(v.pos[k]); } }
            let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
            let z = (sz.width / (hi[0] - lo[0])).min(sz.height / (hi[1] - lo[1])).min(1.0);
            self.cam.zoom = z;
            self.cam.off = [(sz.width - (hi[0] + lo[0]) * z) / 2.0, (sz.height - (hi[1] + lo[1]) * z) / 2.0];
            // SHC_ZOOM=4: start zoomed in around the window centre (for screenshots)
            if let Some(k) = std::env::var("SHC_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()) {
                let c = &mut self.cam;
                for (o, s) in c.off.iter_mut().zip([sz.width / 2.0, sz.height / 2.0]) { *o = s - (s - *o) * k; }
                c.zoom *= k;
            }
            // SHC_FOCUS=296,493: centre on that game tile at 1:1 (replay screenshots of units)
            let focus = std::env::var("SHC_FOCUS").ok().and_then(|v| v.split_once(',').map(|(x, y)| (x.parse().unwrap(), y.parse().unwrap())));
            if let Some((x, y)) = focus.or(self.focus) {
                let [px, mut py] = replay::cell_px(x, y);
                if let Some(p) = &self.play { py -= p.world.tiles[y as usize * replay::N + x as usize][5] as f32 * 2.0; }
                // live game: the original's start scale 1 = one game pixel per screen (device) pixel
                let live = self.play.as_ref().is_some_and(|p| p.live.is_some());
                (self.cam.zoom, self.cam.zoom_pos) = (if live { 1.0 / g.window.scale_factor() as f32 } else { 1.0 }, 3.0);
                self.cam.off = [sz.width / 2.0 - px * self.cam.zoom, sz.height / 2.0 - py * self.cam.zoom];
            }
            if self.sheet { (self.cam.zoom, self.cam.off) = (1.0, [0.0, 0.0]); } // 1:1, top-left
            g.static_buf = Some((buf, runs(&scene, &self.scene_rows)));
        }
    }

    fn event(&mut self, el: &ActiveEventLoop, ev: WindowEvent) {
        // trackpad, focus / visibility, close button, F1 (ux.rs); whatever it doesn't take goes on below
        if self.ux_event(el, &ev) { return; }
        // the held left button: the button under it looks pressed (Ui::text_button); scripted runs: SHC_INPUT d / u
        if let (WindowEvent::MouseInput { button: MouseButton::Left, state, .. }, Some(u), false) = (&ev, self.ui.as_mut(), scripted()) { u.down = state.is_pressed(); }
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            // a custom lord's .zip dropped on the window: imported (lord_share.rs); the menu's Extras screen shows the result
            WindowEvent::DroppedFile(p) if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) => {
                lord_share::import_ui(&p, false);
                eprintln!("lord import {}: {:?}", p.display(), lord_share::MESSAGE.lock().unwrap());
            }
            WindowEvent::Resized(sz) => if let Some(g) = &mut self.gfx {
                g.config.width = sz.width.max(1);
                g.config.height = sz.height.max(1);
                g.surface.configure(&g.device, &g.config);
            },
            // screenshots must not depend on whatever the real mouse does over the window
            WindowEvent::CursorMoved { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::MouseInput { .. }
                if std::env::var_os("SHC_SHOT").is_some() => {}
            // start-up intro: a left click skips the logos / ends the video; the menu under it gets no clicks
            WindowEvent::MouseInput { button, state, .. } if self.intro.is_some() => {
                if (button, state) == (MouseButton::Left, ElementState::Released) && !self.intro.as_mut().unwrap().click() { self.intro = None; }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = position.to_logical::<f32>(self.gfx.as_ref().map_or(1.0, |g| g.window.scale_factor()));
                let c = &mut self.cam;
                if c.drag { c.off[0] += p.x - c.cursor[0]; c.off[1] += p.y - c.cursor[1]; }
                c.cursor = [p.x, p.y];
                c.cursor_in = true;
            }
            WindowEvent::MouseInput { button: MouseButton::Left, state, .. } if self.menu.is_some() => {
                if state == ElementState::Pressed { if let Some(go) = self.menu.as_mut().unwrap().click(self.cam.cursor) { self.menu_go(go, el) } }
            }
            // MouseRightButtonUp: a Sandbox option steps back
            WindowEvent::MouseInput { button: MouseButton::Right, state: ElementState::Released, .. } if self.menu.is_some() => self.menu.as_mut().unwrap().right_click(self.cam.cursor),
            WindowEvent::MouseWheel { delta, .. } if self.menu.is_some() => {
                let dy = match delta { MouseScrollDelta::LineDelta(_, y) => y, MouseScrollDelta::PixelDelta(p) => p.y as f32 / 30.0 };
                self.menu.as_mut().unwrap().wheel(dy);
            }
            // Options > Hotkeys waiting for a key (KeyManager.HotKeySelectorMode): the press goes to the menu
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && self.menu.as_ref().is_some_and(|m| m.capturing())
                && event.logical_key != Key::Named(NamedKey::Escape) => {
                if let Some(go) = keys::unity_key(&event, self.mods).and_then(|k| self.menu.as_mut().unwrap().key(&k)) { self.menu_go(go, el) }
            }
            WindowEvent::MouseInput { .. } | WindowEvent::KeyboardInput { .. } if self.menu.as_ref().is_some_and(|m| !m.is_pause()) && !matches!(&ev,
                WindowEvent::KeyboardInput { event, .. } if event.logical_key == Key::Named(NamedKey::Escape)) => {}
            #[cfg(feature = "live")]
            WindowEvent::MouseInput { button: MouseButton::Left, state, .. } if self.menu.is_none() && self.rp.as_ref().is_some_and(|c| c.over(self.cam.cursor)) => {
                if let (true, Some(c), Some(p)) = (state == ElementState::Pressed, &mut self.rp, &mut self.play) { c.click(self.cam.cursor, &mut p.playing, &mut p.rate) }
            }
            WindowEvent::MouseInput { button: MouseButton::Left, state, .. } if self.menu.is_none() && self.on_tutorial() => {
                if state == ElementState::Pressed { self.tutorial_click(el) }
            }
            WindowEvent::MouseInput { button: MouseButton::Left, state, .. } if self.menu.is_none() && self.stats.over(self.cam.cursor) => if state == ElementState::Pressed { self.stats.click(self.cam.cursor); }, // stats tabs (stats.rs), over the score screen
            WindowEvent::MouseInput { button: MouseButton::Left, state, .. } if self.menu.is_none() && self.end.over(self.cam.cursor) => {
                if state == ElementState::Pressed { if let Some(e) = self.end.click(self.cam.cursor) { self.end_exit(e, el) } }
            }
            WindowEvent::MouseInput { button: MouseButton::Left, state, .. } => self.left_button(state == ElementState::Pressed),
            WindowEvent::MouseInput { button: MouseButton::Middle, state, .. } => self.cam.drag = state == ElementState::Pressed,
            // --live: right release (DLL_TroopSelection rightUp)
            WindowEvent::MouseInput { button: MouseButton::Right, state, .. } => self.right_button(state == ElementState::Pressed),
            // in game: the original's zoom levels (CameraControls2D: ±0.5 per notch, 150 ms apart unless the direction flips)
            WindowEvent::MouseWheel { delta, .. } if self.play.as_ref().is_some_and(|p| p.live.is_some()) => {
                let dy = match delta { MouseScrollDelta::LineDelta(_, y) => y, MouseScrollDelta::PixelDelta(p) => p.y as f32 };
                let now = Instant::now();
                let up = dy > 0.0;
                if dy != 0.0 && self.cam.zoom_delay.is_none_or(|(u, t)| u != up || now > t) {
                    self.cam.zoom_delay = Some((up, now + std::time::Duration::from_millis(150)));
                    self.adjust_zoom(if up { 0.5 } else { -0.5 });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta { MouseScrollDelta::LineDelta(_, y) => y * 40.0, MouseScrollDelta::PixelDelta(p) => p.y as f32 };
                let c = &mut self.cam;
                let f = (dy * 0.005).exp().clamp(0.5, 2.0);
                let nz = (c.zoom * f).clamp(0.01, 8.0);
                // keep the world point under the cursor fixed
                for k in 0..2 { c.off[k] = c.cursor[k] - (c.cursor[k] - c.off[k]) * nz / c.zoom; }
                c.zoom = nz;
            }
            // shift / ctrl / alt for DLL_RunTick (troop cursor: attack-move, ...)
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                if let Some(l) = self.play.as_mut().and_then(|p| p.live.as_mut()) {
                    let k = m.state();
                    l.keys = [k.shift_key(), k.control_key(), k.alt_key()];
                }
            }
            // fullscreen on / off: Cmd+Ctrl+F (macOS) or Alt+Enter (Windows, Unity's player)
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed
                && ((self.mods.super_key() && self.mods.control_key() && event.logical_key == Key::Character("f".into()))
                    || (self.mods.alt_key() && event.logical_key == Key::Named(NamedKey::Enter))) => {
                if let Some(g) = &self.gfx {
                    let full = g.window.fullscreen().is_none();
                    set_windowed(&g.window, !full);
                }
            }
            // the map editor's name box takes the keys while it is open
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && self.menu.is_none() && self.editor.as_ref().is_some_and(|e| e.typing) => {
                if let Some(e) = &mut self.editor { e.key(&event.logical_key) }
            }
            // a live game: keys go through the hotkey table (KeyManager); Escape stays the menu key (OptionsMenu)
            WindowEvent::KeyboardInput { event, .. } if self.menu.is_none() && self.play.as_ref().is_some_and(|p| p.live.is_some())
                && event.logical_key != Key::Named(NamedKey::Escape) => {
                let c = match &event.logical_key { Key::Character(c) => Some(c.to_string()), _ => None };
                if let Some(k) = keys::unity_key(&event, self.mods) { self.key_event(&k, c.as_deref(), event.state == ElementState::Pressed, event.repeat, el) }
            }
            WindowEvent::KeyboardInput { event, .. } if event.logical_key == Key::Named(NamedKey::Tab) => {
                if let Some(p) = &mut self.play { p.ff = event.state == ElementState::Pressed }
            }
            // arrow keys scroll while held (Cam::scroll)
            WindowEvent::KeyboardInput { event, .. } if matches!(event.logical_key, Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight | NamedKey::ArrowUp | NamedKey::ArrowDown)) => {
                let k = [NamedKey::ArrowLeft, NamedKey::ArrowRight, NamedKey::ArrowUp, NamedKey::ArrowDown].iter().position(|n| event.logical_key == Key::Named(*n)).unwrap();
                self.cam.keys[k] = event.state == ElementState::Pressed;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    // Escape: in game the pause menu (sim paused), in the menu one screen back
                    Key::Named(NamedKey::Escape) => self.escape(el),
                    _ if self.menu.is_some() => {}
                    Key::Named(NamedKey::Space) => if let Some(p) = &mut self.play { p.playing = !p.playing; self.tick_replay(0.0, 0) },
                    Key::Character(ref c) => self.char_key(c.as_str()),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                if let (Some((from, to, t0)), Some(g)) = (self.cam.zoom_anim, &self.gfx) {
                    let t = ((now - t0).as_secs_f32() / 0.2).min(1.0); // smoovZoomDuration
                    let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                    let c = &mut self.cam;
                    let nz = from + (to - from) * t;
                    for (k, m) in [sz.width / 2.0, sz.height / 2.0].into_iter().enumerate() { c.off[k] = m - (m - c.off[k]) * nz / c.zoom; }
                    c.zoom = nz;
                    if t >= 1.0 { c.zoom_anim = None; }
                }
                // in game: scrolling (arrows, window edges) and the original's camera limits after every move / zoom
                if let (Some(g), true, None) = (&self.gfx, self.play.is_some(), &self.menu) {
                    let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                    let (sf, dt) = (g.window.scale_factor() as f32, (now - self.last).as_secs_f32());
                    let before = self.cam.off;
                    self.cam.scroll(sz.width, sz.height, sf, self.mods.shift_key(), dt);
                    if self.cam.off != before { self.tutorial_action(1, -1) } // CameraControls2D: the camera scrolled
                    self.cam.ui_hidden = self.ui.as_ref().is_some_and(|u| u.hidden);
                    self.cam.limit(sz.width, sz.height, sf, ux::hud_scale(sz.width, sz.height));
                }
                self.frame_times.push((now - self.last).as_secs_f32() * 1000.0);
                self.last = now;
                if self.frame_times.len() == 60 {
                    let avg = self.frame_times.iter().sum::<f32>() / 60.0;
                    let max = self.frame_times.iter().cloned().fold(0.0, f32::max);
                    println!("frame {}: time avg {avg:.2} ms, max {max:.2} ms", self.frames);
                    self.frame_times.clear();
                }
                if let Some(p) = &mut self.play { p.now = now; }
                if self.play.is_some() && self.frames > 0 { self.tick_replay(self.frame_times.last().copied().unwrap_or(0.0) / 1000.0, 0); }
                // units sliding between tiles (record 0x34) move every frame, not just every tick (unless a tick just did)
                if self.play.as_ref().is_some_and(|p| !p.world.next.is_empty() && p.world.drawn_at != Some(now)) { self.rebuild_objs(); }
                self.scripted_input(el);
                self.loading_tick(el); // loader.rs: finished map atlases in, a background game start that is ready
                #[cfg(feature = "live")]
                self.lan_tick(el); // lan.rs: the LAN lobby session / a client's game start / a LAN game that stopped
                // DLL_RunTick mouse tile: hover (building interiors, panels on click); off the map over the HUD
                if let Some(l) = self.play.as_mut().and_then(|p| p.live.as_mut()) {
                    let over = self.ui.as_ref().is_some_and(|u| u.over(self.cam.cursor));
                    let t = if over { [-1, -1] } else { self.cam.tile_at(l.rot) };
                    if t != l.mouse_at { l.send(live::Cmd::Mouse(t[0], t[1])); l.mouse_at = t; }
                    let c = &self.cam;
                    l.cursor_at([(c.cursor[0] - c.off[0]) / c.zoom, (c.cursor[1] - c.off[1]) / c.zoom], c.cursor); // troop cursor
                }
                // HUD_RightClick.Update while it shows (FatControler): one action per press, rotation repeating
                let (paused, rot) = self.play.as_ref().map_or((true, 0), |p| (!p.playing, p.live.as_ref().map_or(0, |l| l.rot)));
                if let Some(a) = self.ui.as_mut().and_then(|u| u.rclick_update(self.cam.cursor, rot, paused)) {
                    match a {
                        hud::RcAction::RotateRight => self.rotate_map(true),
                        hud::RcAction::ZoomOut => self.zoom_out_loop(),
                        hud::RcAction::ToggleUi => self.tutorial_action(17, -1), // the HUD hid itself (Ui::hidden)
                        hud::RcAction::Flatten(f) => {
                            if let Some(l) = self.play.as_ref().and_then(|p| p.live.as_ref()) { l.send(live::Cmd::Flatten(f)) }
                            self.tutorial_action(if f { 4 } else { 5 }, -1); // EngineInterface.toggleFlattenedLandscapeMode
                        }
                    }
                }
                self.editor_frame(el); // the map editor's strokes (menu/editor.rs)
                let ghost = self.build_ghost();
                // panel buttons -> DLL_GameAction on the sim thread
                // SideTabs' options button: the in-game menu, as Escape opens it (HUD_Main.InGameOptions)
                if self.ui.as_mut().is_some_and(|u| std::mem::take(&mut u.open_menu)) && self.menu.is_none() { self.escape(el); }
                // a building panel's help button: its page over the running game (HUD_Help.OpenHelp(fromMenu: false))
                if let Some(page) = self.ui.as_mut().and_then(|u| u.help.take()).filter(|_| self.menu.is_none()) { self.menu = Some(menu::Menu::help(&page, false, false)); }
                // SideTabs' briefing button (ButtonGotoBriefing): a campaign mission's briefing over the paused game
                if self.ui.as_mut().is_some_and(|u| std::mem::take(&mut u.open_briefing)) && self.menu.is_none() {
                    if let (Some((id, d, _)), Some(p)) = (self.campaign, &mut self.play) { p.playing = false; self.menu = Some(menu::Menu::briefing_mid(id, d)); }
                }
                if let (Some(u), Some(l)) = (self.ui.as_mut(), self.play.as_ref().and_then(|p| p.live.as_ref())) {
                    (u.shift, u.ctrl) = (self.mods.shift_key(), self.mods.control_key());
                    // actions before the sub-mode: ButtonReports closes the troops panel (1040) before SetAppMode(16, 71);
                    // the market's SetCurrentTradedGood works either way (hud::panel_buttons_via_hud)
                    for a in u.actions.drain(..) { l.send(live::Cmd::Action(a)); }
                    for a in u.actions4.drain(..) { l.send(live::Cmd::Action4(a)); }
                    if let Some(s) = u.sub_mode.take() { l.send(live::Cmd::AppMode(16, s)); } // EditorDirector.directSetAppSubMode
                    if let Some(id) = u.delete.take() { l.send(live::Cmd::Delete(id)); }
                    if let Some([x, y]) = u.radar_click.take() { l.send(live::Cmd::Radar(x, y)); }
                }
                let g = self.gfx.as_ref().unwrap();
                // Director.monitorCursors (cursor.rs): the delete cursor while the demolish tool (SideTabs, mapper 0x27) is
                // held; campaign / Options lord not in the port yet
                let demolish = self.ui.as_ref().and_then(|u| u.selected_mapper()) == Some(0x27);
                if let Some(c) = &mut self.cursors { c.set(&g.window, cursor::kind(self.play.is_some(), demolish, None, 0)); }
                let built;
                let (tris, rows): (&[Tri], &[i32]) = match (&self.play, &g.static_buf) {
                    (Some(p), _) => (&p.objs, &p.obj_rows),
                    (None, Some(_)) => (&[], &[]),
                    (None, None) if self.menu.is_some() => (&[], &[]),
                    (None, None) => { built = self.build_tris(self.start.elapsed().as_secs_f32()); (&built, &[]) }
                };
                // --live: the sim's cursor sprite (draw record type 5: offset 1/2, body 3/4, second body 13/14) at the
                // mouse, half size, on top (GameMap.Update / mouseCursorGO). × the window's scale factor: our world starts
                // at one sprite px per device px, so on a Retina screen the original's half size came out half as big
                // as the game shows it on a normal screen (the pointer next to it is 32 points either way)
                let (cur, cur_rows): (Vec<Tri>, Vec<i32>);
                let (tris, rows) = match self.play.as_ref().and_then(|p| Some((p, p.live.as_ref()?.cursor?))) {
                    Some((p, r)) => {
                        let (c, k) = (&self.cam, g.window.scale_factor() as f32);
                        let pos = [(c.cursor[0] - c.off[0]) / c.zoom + r[1] as f32 * 2.0 * k, (c.cursor[1] - c.off[1]) / c.zoom + r[2] as f32 * 2.0 * k];
                        let mut t = tris.to_vec();
                        // mouseCursorGO with the record's transparency k11 (the move cursor's faint first sprite), GO2 opaque
                        let a = replay::obj_alpha(&r).unwrap_or(0.0);
                        for (f, i, a) in [(r[3], r[4], a), (r[13], r[14], 1.0)] {
                            if let Some(s) = p.gm.body(f, i).filter(|_| f > 0) { self.push_c(&mut t, s, pos, 0.5 * k, [1.0, 1.0, 1.0, a], [0.0; 4]); }
                        }
                        cur_rows = rows.iter().copied().chain(std::iter::repeat(i32::MAX)).take(t.len()).collect();
                        cur = t;
                        (&cur[..], &cur_rows[..])
                    }
                    None => (tris, rows),
                };
                self.frames += 1;
                // SHC_SHOT=out.ppm: save frame 180 (SHC_SHOT_AT=n: frame n) and quit (proof screenshots without grabbing the desktop).
                // A '#' in the name + SHC_SHOT_TO=m: every frame n..=m, '#' = the frame number (video clips: ffmpeg -i f#####.ppm)
                let at = std::env::var("SHC_SHOT_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(180);
                let to = std::env::var("SHC_SHOT_TO").ok().and_then(|v| v.parse().ok()).unwrap_or(at);
                let shot = std::env::var("SHC_SHOT").ok().filter(|_| (at..=to).contains(&self.frames))
                    .map(|p| p.replace('#', &format!("{:05}", self.frames)));
                let hud = match (&mut self.ui, &self.play) {
                    (Some(u), Some(p)) => {
                        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                        let t = p.at - 1;
                        let label = p.live.as_ref().map_or(t, |l| l.tick as usize);
                        u.edge_mask = self.cam.edge_mask;
                        match &mut self.editor {
                            Some(e) => e.draw(u, &p.huds.get(t).cloned().unwrap_or_default(), sz.width, sz.height, self.cam.cursor, u.radar),
                            None => {
                                let mut hud = u.draw(&p.huds.get(t).cloned().unwrap_or_default(), sz.width, sz.height, self.cam.cursor, label, ghost.as_ref());
                                // King of the Hill (koth.rs): the hill on the ground under the HUD, the score bar over it
                                // until the game is decided (the end banner takes the top then)
                                let st = p.huds.get(t);
                                match st.and_then(|h| h.hill.as_ref()) {
                                    Some(b) => {
                                        let mut under = vec![];
                                        koth::ring(&mut under, b, |x, y| self.cam.ground(p.world.rot, x, y), self.start.elapsed().as_secs_f32());
                                        if st.and_then(|h| h.end.as_ref()).is_none_or(|e| e.result == 0) { koth::bar(u, &mut hud, b, sz.width, sz.height); }
                                        [under, hud].concat()
                                    }
                                    None => hud,
                                }
                            }
                        }
                    }
                    _ => vec![],
                };
                // game end (endgame.rs): Victory / Defeat banner, then the score screen, over the running game
                #[cfg(feature = "live")]
                { self.end.trail = self.trail.map(|t| (t.0, t.1, self.play.as_ref().and_then(|p| p.live.as_ref()).map_or(0, |l| l.tick))); }
                let hud = match (&mut self.ui, &self.play) {
                    (Some(u), Some(p)) if self.menu.is_none() => {
                        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                        let st = p.huds.get(p.at - 1).cloned().unwrap_or_default();
                        [hud, self.end.draw(&st, u, sz.width, sz.height, self.cam.cursor, self.last_lobby.is_some() || self.campaign.is_some() || self.trail.is_some())].concat()
                    }
                    _ => hud,
                };
                // the tutorial's panel (tutorial.rs)
                let hud = match (&self.tutorial, &mut self.ui, &self.play) {
                    (Some(t), Some(u), Some(p)) if self.menu.is_none() => {
                        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                        let st = p.huds.get(p.at - 1).cloned().unwrap_or_default();
                        [hud, t.draw(u, &st, sz.width, sz.height, self.cam.cursor)].concat()
                    }
                    _ => hud,
                };
                // a LAN game's chat and pause / waiting text (lan.rs)
                #[cfg(feature = "live")]
                let hud = match &mut self.ui {
                    Some(u) if self.menu.is_none() && self.play.is_some() => [hud, lan::overlay(u, self.cam.cursor)].concat(),
                    _ => hud,
                };
                // Options > Show Local Time / Show Play Time (menu.rs)
                let hud = match &mut self.ui {
                    Some(u) if self.menu.is_none() && self.play.as_ref().is_some_and(|p| p.live.is_some()) => {
                        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                        [hud, menu::clock_overlay(u, sz.width, sz.height)].concat()
                    }
                    _ => hud,
                };
                // a replay's bar (game_replay.rs)
                #[cfg(feature = "live")]
                let hud = match (&mut self.rp, &mut self.ui, &self.play) {
                    (Some(c), Some(u), Some(p)) if self.menu.is_none() => {
                        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                        [hud, c.draw(u, sz.width, sz.height, self.cam.cursor, p.playing, p.rate)].concat()
                    }
                    _ => hud,
                };
                let hud = stats::overlay(&mut self.stats, self.ui.as_mut(), &self.play, self.menu.is_some(), g, self.cam.cursor, hud); // F3 / end-screen stats (stats.rs)
                if let (Some(u), Some(h)) = (&mut self.ui, &g.hud) {
                    if u.font_dirty {
                        g.queue.write_texture(h.font.as_image_copy(), &u.font_rgba,
                            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(hud::FONT_PX * 4), rows_per_image: None }, h.font.size());
                        u.font_dirty = false;
                    }
                }
                // campaign menus / briefing: the screen's full-screen image (re/scripts/export_frontend.py: ui/screens); in a
                // game the end screens' (endgame.rs)
                if let Some(h) = &g.hud {
                    let want = match &self.menu { Some(m) => m.screen_image(), None => self.end.screen_image() };
                    if want != self.screen_image {
                        let px = want.as_deref().and_then(menu::screen_pixels);
                        if let Some(px) = px {
                            let rows = (px.len() / (1920 * 4)) as u32; // 1080, or SCREEN_H with a story page's art
                            g.queue.write_texture(h.screen.as_image_copy(), &px, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(1920 * 4), rows_per_image: None },
                                wgpu::Extent3d { width: 1920, height: rows, depth_or_array_layers: 1 });
                        }
                        self.screen_image = want;
                    }
                }
                // the coat-of-arms sheet's new parts (menu::coa)
                if let Some(h) = &g.hud {
                    for ([x, y, w, hh], px) in menu::coa::take_uploads() {
                        g.queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &h.coa, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
                            &px, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None }, wgpu::Extent3d { width: w, height: hh, depth_or_array_layers: 1 });
                    }
                }
                // lobby: the selected map's 200×200 radar preview into the radar texture's top-left corner
                if let (Some(px), Some(h)) = (self.menu.as_mut().and_then(|m| m.preview.take()), &g.hud) {
                    let bgra: Vec<u8> = px.chunks(4).flat_map(|c| [c[2], c[1], c[0], c[3]]).collect();
                    g.queue.write_texture(h.radar.as_image_copy(), &bgra, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(800), rows_per_image: None },
                        wgpu::Extent3d { width: 200, height: 200, depth_or_array_layers: 1 });
                }
                let hud = match (&mut self.menu, &mut self.ui) {
                    (Some(m), Some(u)) if self.intro.is_none() => {
                        // front end: its backdrop video's frame due now (menu.rs draws it)
                        if let (false, false, Some(h)) = (m.is_pause(), video::full_busy(), &g.hud) { self.menu_loop.update(&g.queue, &h.video); }
                        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                        // a briefing lists the running game's objectives (FatControler.UpdateObjectiveRows)
                        if let Some(st) = self.play.as_ref().and_then(|p| p.huds.get(p.at.saturating_sub(1))) { m.goals = (st.ev.goals.clone(), st.ev.campaign, st.ev.timer); }
                        [hud, m.draw(u, sz.width, sz.height, self.cam.cursor)].concat()
                    }
                    _ => hud,
                };
                // start-up intro (video.rs) instead of the menu until it's over
                let intro = match (&mut self.intro, &g.hud) {
                    (Some(i), Some(h)) => {
                        let sz = g.window.inner_size().to_logical::<f32>(g.window.scale_factor());
                        Some(i.draw(&g.queue, &h.video, sz.width, sz.height))
                    }
                    _ => None,
                };
                let hud = match intro { Some(Some(t)) => t, Some(None) => { self.intro = None; hud } None => hud };
                #[cfg(feature = "live")]
                let hud = match self.play.as_ref().and_then(|p| p.live.as_ref()).and_then(|l| l.press_w) {
                    Some(a) => [hud, military::box_tris([self.cam.off[0] + a[0] * self.cam.zoom, self.cam.off[1] + a[1] * self.cam.zoom], self.cam.cursor)].concat(),
                    None => hud,
                };
                let hud = if self.intro.is_none() { loader::overlay(&self.loading, g, self.ui.as_mut(), hud) } else { hud }; // "Loading..." (loader.rs)
                // the Sands of Time timer / results (trail_menu.rs), not under a menu
                #[cfg(feature = "live")]
                let hud = {
                    let (p, sz) = (self.play.as_ref(), g.window.inner_size().to_logical::<f32>(g.window.scale_factor()));
                    let over = p.and_then(|p| p.huds.first()).is_some_and(|h| h.end.is_some());
                    menu::trail_menu::sands_overlay(self.trail.filter(|_| self.menu.is_none()), p.and_then(|p| p.live.as_ref()).map(|l| l.tick), over, [sz.width, sz.height], self.ui.as_mut(), hud)
                };
                if let Some(h) = &g.hud { video::pump(&g.queue, &h.small, &h.video); } // the in-game videos the HUD / end screen asked for
                let tick = self.play.as_ref().and_then(|p| p.live.as_ref()).map_or(0, |l| l.tick);
                let hud = if self.menu.is_none() { Self::fps_text(&mut self.fps, self.ui.as_mut(), tick, g.window.inner_size().to_logical::<f32>(g.window.scale_factor()).width, hud) } else { hud }; // F: fps / sim rate
                let hud = ux::overlay(&mut self.ux, self.ui.as_mut(), &self.keys, &g.window, hud); // F1 sheet, notices
                draw(g, &self.cam, tris, rows, &hud, shot.as_deref());
                if shot.is_some() && self.frames >= to {
                    if let Some(p) = &self.play {
                        println!("shot at tick {}, rotation {}, {} objects, sprite misses by GM file: {:?}", p.at - 1, p.world.rot, p.world.objs.len(), p.misses);
                        println!("hud {:?}", p.huds.get(p.at - 1));
                        // unit tiles (West frame), for picking SHC_FOCUS
                        let units: Vec<(i16, i16)> = p.world.objs.iter().filter(|((t, _), _)| *t == 2).map(|(_, r)| (r[1], r[2])).collect();
                        println!("units at {units:?}");
                    }
                    el.exit();
                }
                if self.ux.continuous() { g.window.request_redraw(); } // else ux_about_to_wait paces the frames
            }
            _ => {}
        }
    }
}

async fn init_gfx(window: Arc<Window>, atlases: &[Atlas], with_hud: bool) -> Gfx {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface), power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default()
    }).await.unwrap();
    println!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        required_limits: adapter.limits(),
        ..Default::default()
    }).await.expect("BC texture compression unsupported");

    let size = window.inner_size();
    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
    let present_mode = ux::present_mode(); // Options > VSync; SHC_NOVSYNC=1 = uncapped
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, format, width: size.width, height: size.height,
        present_mode, alpha_mode: caps.alpha_modes[0], view_formats: vec![], desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    let t0 = Instant::now();
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
        ],
    });
    let screen_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None, size: 32, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
    });
    let black = device.create_texture_with_data(&queue, &wgpu::TextureDescriptor {
        label: Some("no mask"), size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
        dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
    }, Default::default(), &[0, 0, 0, 255]).create_view(&Default::default());
    // ponytail: one texture + bind group per atlas, one draw per atlas; texture arrays if draw count ever matters
    // the map atlases + masks (1.3 GB of BC blocks) upload on a worker thread (loader.rs); transparent until each is in
    let mut loading = loader::Atlases::start(&device, &queue, &bgl, &screen_buf, &sampler, &black, atlases);
    let mut groups = vec![loading.placeholder.clone(); atlases.len()];
    // scripted runs (SHC_SHOT / SHC_INPUT click and shoot by frame number): every atlas in before the first frame
    if scripted() { while loading.progress().is_some() { std::thread::sleep(std::time::Duration::from_millis(5)); loading.take(&mut groups); } }
    println!("map atlas bind groups in {:.2}s, the atlases upload on a worker thread", t0.elapsed().as_secs_f32());

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None, source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&bgl], push_constant_ranges: &[] });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None, layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader, entry_point: Some("vs"), compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vert>() as u64, step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader, entry_point: Some("fs"), compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None, multisample: Default::default(), multiview: None, cache: None,
    });
    let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None, size: 64 << 20, // ponytail: fixed 64 MB (~700k tris, draw() truncates past that); grow if a replay needs more
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
    });
    let hud = with_hud.then(|| {
        let t0 = Instant::now();
        let tex = |label, w: u32, mips: u32, format| device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label), size: wgpu::Extent3d { width: w, height: w, depth_or_array_layers: 1 }, mip_level_count: mips, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        // UI-MasterAtlas: 8192² BC7, 14 mips back to back (re/scripts/export_ui_atlas.py); the HUD draws it at ~0.4×
        let data = std::fs::read(cache_dir().join("ui/UI-MasterAtlas.bc")).unwrap();
        let atlas = tex("UI-MasterAtlas", 8192, 14, wgpu::TextureFormat::Bc7RgbaUnormSrgb);
        let mut off = 0;
        for m in 0..14 {
            let blocks = (8192u32 >> m).div_ceil(4);
            let len = (blocks * blocks * 16) as usize;
            queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &atlas, mip_level: m, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &data[off..off + len], wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(blocks * 16), rows_per_image: Some(blocks) },
                wgpu::Extent3d { width: (8192 >> m).max(1), height: (8192 >> m).max(1), depth_or_array_layers: 1 }.physical_size(wgpu::TextureFormat::Bc7RgbaUnormSrgb));
            off += len;
        }
        let font = tex("font", hud::FONT_PX, 1, wgpu::TextureFormat::Rgba8Unorm);
        let screen_buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 32, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mip_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let group = |t: &wgpu::Texture| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: screen_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&t.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&mip_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&black) },
            ],
        });
        // minimap from the sim (--live): ARGB u32 little-endian = BGRA bytes
        let radar = tex("radar", live::RADAR_N as u32, 1, wgpu::TextureFormat::Bgra8UnormSrgb);
        // main-menu art sheet (menu::ART, re/scripts/export_frontend.py); 1×1 when not exported
        let (aw, ah, px) = menu::art_pixels().unwrap_or((1, 1, vec![0; 4]));
        let art = device.create_texture_with_data(&queue, &wgpu::TextureDescriptor {
            label: Some("frontend"), size: wgpu::Extent3d { width: aw, height: ah, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8UnormSrgb, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        }, Default::default(), &px);
        let video = video::texture(&device); // video::TEX: intro / menu backdrop frames (video.rs)
        // menu::SCREEN: the current front-end screen's 1920×1080 image (campaign menus, briefing), swapped per screen
        let screen = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("screen"), size: wgpu::Extent3d { width: 1920, height: menu::SCREEN_H, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8UnormSrgb, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        let small = video::small_texture(&device); // video::SMALL = 6: building panel video + end banner (video.rs)
        // menu::coa::TEX = 7: the coat-of-arms sheet (the editor's preview, faces, grid icons), filled by take_uploads
        let coa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("coa"), size: wgpu::Extent3d { width: menu::coa::W, height: menu::coa::H, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8UnormSrgb, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        let groups = [group(&atlas), group(&font), group(&radar), group(&art), group(&video), group(&screen), group(&small), group(&coa)];
        let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("hud"), size: 4 << 20, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        println!("uploaded UI atlas in {:.2}s", t0.elapsed().as_secs_f32());
        HudGfx { screen_buf, buf, groups, font, radar, video, screen, small, coa }
    });
    Gfx { window, surface, device, queue, config, pipeline, screen_buf, inst_buf, groups, static_buf: None, hud, loading: Some(loading) }
}

fn draw(g: &Gfx, cam: &Cam, tris: &[Tri], rows: &[i32], hud: &[hud::HudTri], shot: Option<&str>) {
    let scale = g.window.scale_factor() as f32;
    g.queue.write_buffer(&g.screen_buf, 0, bytemuck::cast_slice(&[g.config.width as f32 / scale, g.config.height as f32 / scale, cam.zoom, std::env::var_os("SHC_MASK").is_some() as u32 as f32, cam.off[0], cam.off[1], 0.0, 0.0]));
    let tris = &tris[..tris.len().min(g.inst_buf.size() as usize / std::mem::size_of::<[Vert; 3]>())];
    let raw: Vec<[Vert; 3]> = tris.iter().map(|(_, t)| *t).collect();
    g.queue.write_buffer(&g.inst_buf, 0, bytemuck::cast_slice(&raw));
    let dynamic = runs(tris, rows);
    // merge by row: tiles of a row (sub 0) first, then that row's objects
    let stat: &[Run] = g.static_buf.as_ref().map_or(&[], |(_, r)| r);
    let (mut i, mut j) = (0, 0);
    let mut cmds: Vec<(bool, &Run)> = Vec::with_capacity(stat.len() + dynamic.len());
    while i < stat.len() || j < dynamic.len() {
        if j >= dynamic.len() || (i < stat.len() && stat[i].0 <= dynamic[j].0) { cmds.push((true, &stat[i])); i += 1; } else { cmds.push((false, &dynamic[j])); j += 1; }
    }
    let Ok(frame) = g.surface.get_current_texture() else { return };
    let view = frame.texture.create_view(&Default::default());
    let mut enc = g.device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.09, g: 0.1, b: 0.08, a: 1.0 }), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&g.pipeline);
        let (mut cur_buf, mut cur_atlas) = (None, u32::MAX);
        for (is_static, (_, a, r)) in cmds {
            if cur_buf != Some(is_static) {
                pass.set_vertex_buffer(0, if is_static { g.static_buf.as_ref().unwrap().0.slice(..) } else { g.inst_buf.slice(..) });
                cur_buf = Some(is_static);
            }
            if cur_atlas != *a { pass.set_bind_group(0, &g.groups[*a as usize], &[]); cur_atlas = *a; }
            pass.draw(r.clone(), 0..1);
        }
        // HUD on top, screen space (zoom 1, no offset), same pipeline
        if let (Some(h), false) = (&g.hud, hud.is_empty()) {
            let hud = &hud[..hud.len().min(h.buf.size() as usize / std::mem::size_of::<[Vert; 3]>())];
            g.queue.write_buffer(&h.screen_buf, 0, bytemuck::cast_slice(&[g.config.width as f32 / scale, g.config.height as f32 / scale, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
            let raw: Vec<[Vert; 3]> = hud.iter().map(|(_, t)| *t).collect();
            g.queue.write_buffer(&h.buf, 0, bytemuck::cast_slice(&raw));
            pass.set_vertex_buffer(0, h.buf.slice(..));
            for (_, a, r) in runs(hud, &[]) {
                pass.set_bind_group(0, &h.groups[a as usize], &[]);
                pass.draw(r, 0..1);
            }
        }
    }
    let (w, h) = (g.config.width, g.config.height);
    let row = (w * 4).div_ceil(256) * 256;
    let readback = shot.map(|_| {
        let buf = g.device.create_buffer(&wgpu::BufferDescriptor {
            label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(frame.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
            frame.texture.size());
        buf
    });
    g.queue.submit([enc.finish()]);
    if let (Some(path), Some(buf)) = (shot, readback) {
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| ());
        g.device.poll(wgpu::PollType::Wait).unwrap();
        let data = buf.slice(..).get_mapped_range();
        let bgra = matches!(g.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut ppm = format!("P6 {w} {h} 255\n").into_bytes();
        for y in 0..h as usize {
            for px in data[y * row as usize..][..w as usize * 4].chunks_exact(4) {
                ppm.extend(if bgra { [px[2], px[1], px[0]] } else { [px[0], px[1], px[2]] });
            }
        }
        std::fs::write(path, ppm).unwrap();
        println!("saved {path}");
    }
    frame.present();
}

fn main() {
    paths::app_bundle_start();
    setup::ensure();
    update::start();
    let mut app = App::new();
    let el = EventLoop::new().unwrap();
    el.run_app(&mut app).unwrap();
}

#[test]
fn frame_names() {
    assert_eq!(split_frame("body_hunter-1716x"), Some(("body_hunter-x".into(), 1716)));
    assert_eq!(split_frame("tile_workshops 1335"), Some(("tile_workshops ".into(), 1335)));
    assert_eq!(split_frame("body_hunter-12"), Some(("body_hunter-".into(), 12)));
    assert_eq!(split_frame("nonumber"), None);
}

/// The cursor pick prefers a raised tile drawn over the cursor (ground height k5) to the flat tile behind it.
#[test]
fn pick_raised_tile() {
    let mut c = Cam { zoom: 1.0, heights: vec![0; replay::N * replay::N], ..Default::default() };
    let (x, y) = (400i16, 300i16);
    c.heights[y as usize * replay::N + x as usize] = 40; // drawn 80 px (5 rows) higher
    let [cx, cy] = replay::cell_px(x, y);
    c.cursor = [cx, cy - 80.0];
    let flat = Cam { zoom: 1.0, cursor: c.cursor, ..Default::default() };
    assert_eq!(c.tile_at(6), [x as i32, y as i32]); // rot 6 = West frame
    assert_ne!(flat.tile_at(6), [x as i32, y as i32]);
}

/// GameMap.CalcMapTileFromMousePos: a raised tile catches the cursor anywhere on its column, side faces included,
/// not only on its top diamond (the cursor half-way up a cliff face picks the cliff's tile, not the flat one behind).
#[test]
fn pick_column_face() {
    let mut c = Cam { zoom: 1.0, heights: vec![0; replay::N * replay::N], ..Default::default() };
    let (x, y) = (400i16, 300i16);
    c.heights[y as usize * replay::N + x as usize] = 40; // column 80 px high
    let [cx, cy] = replay::cell_px(x, y);
    c.cursor = [cx, cy - 40.0];
    assert_eq!(c.tile_at(6), [x as i32, y as i32]);
}

/// With useBuildingHeight (no build tool) a building's body catches the cursor; with a build tool only the ground does.
#[test]
fn pick_building_body() {
    let mut c = Cam { zoom: 1.0, heights: vec![0; replay::N * replay::N], bld: vec![0; replay::N * replay::N], ..Default::default() };
    let (x, y) = (400i16, 300i16);
    c.bld[y as usize * replay::N + x as usize] = 60; // a 60 px tall building part on flat ground
    let [cx, cy] = replay::cell_px(x, y);
    c.cursor = [cx, cy - 50.0];
    c.use_bld = bld_pick(None);
    assert_eq!(c.tile_at(6), [x as i32, y as i32]);
    c.use_bld = bld_pick(Some(32)); // a building tool: ground only
    assert_ne!(c.tile_at(6), [x as i32, y as i32]);
    assert!(bld_pick(Some(210))); // mangonel: placed on a tower, so the tower body counts
    // the radar's screen centre is flat (PreCalcScreenCentre, useHeight false), the rotation centre ground-only
    c.use_bld = true;
    assert_ne!(c.centre_flat(c.cursor, 6), [x as i32, y as i32]);
    assert_ne!(c.centre_ground(c.cursor, 6), [x as i32, y as i32]);
    c.heights[y as usize * replay::N + x as usize] = 40; // now the ground itself is raised 80 px
    assert_eq!(c.centre_ground(c.cursor, 6), [x as i32, y as i32]);
    assert_ne!(c.centre_flat(c.cursor, 6), [x as i32, y as i32]);
}

/// GameMap.RotateMapLeft 4× (setMapRotation around getRotationCentre, then SetCameraFromGameState): the tile at the
/// window centre stays there after every turn, and four turns give the starting view back.
#[cfg(feature = "live")]
#[test]
fn rotate_keeps_centre_tile() {
    let map = skirmish::maps_dir().join("Crater Lake.map");
    if !map.exists() { return; }
    let mut l = live::Live::from_mem(skirmish::start(&skirmish::Lobby::new(map)).unwrap());
    let mut w = replay::World::new();
    let run = |l: &mut live::Live, w: &mut replay::World| for _ in 0..4 { w.apply(&l.step(), &mut vec![]) };
    run(&mut l, &mut w);
    let heights = |w: &replay::World| w.tiles.iter().map(|r| if r[15] != 0 && r[17] != 10000 { r[17] } else { r[5] }).collect();
    let (sw, sh) = (1684.0, 784.0);
    let centre = [sw / 2.0, sh / 2.0];
    let mut c = Cam { zoom: 0.5, heights: heights(&w), ..Default::default() };
    c.centre_on(sw, sh, &w, w.rot, 400, 400);
    let t0 = c.centre_ground(centre, w.rot);
    c.centre_on(sw, sh, &w, w.rot, t0[0], t0[1]);
    let off0 = c.off;
    for turn in 1..=4 {
        let t = c.centre_ground(centre, w.rot);
        let rot = (w.rot as i32 + 2) % 8;
        l.rotate(rot, t[0], t[1]);
        run(&mut l, &mut w);
        assert_eq!(w.rot as i32, rot, "turn {turn}: the map didn't turn");
        c.heights = heights(&w);
        c.centre_on(sw, sh, &w, w.rot, t[0], t[1]);
        assert_eq!((c.centre_ground(centre, w.rot), t), (t0, t0), "turn {turn}: the centre tile moved");
    }
    assert_eq!(c.off, off0, "four turns: a different view");
}

/// Every map's draw list fills the rest of the 800×800 grid with GM 38 (the off-map ring, never drawn): the camera
/// limits must come from the real map only (campaign mission 1 = 200, not 800).
#[test]
fn map_box_ignores_off_map_ring() {
    let mut w = replay::World::new();
    for (i, r) in w.tiles.iter_mut().enumerate() {
        let (x, y) = (i % replay::N, i / replay::N);
        r[3] = if (300..=398).contains(&x) && (300..=398).contains(&y) { 1 } else { 38 }; // x + y spans 196 = 200 − 4
    }
    assert_eq!(MapBox::of(&w).unwrap().size, 200.0);
}

#[test]
fn zoom_steps_like_the_original() {
    // MacBook window (3456 device px wide): 0.5..1 of native; a 1920×1080 screen may go down to 0.25
    let mut pos = 3.0;
    for _ in 0..10 { pos = zoom_pos_clamp(pos - 0.5, 3456.0, 2234.0, 800.0); }
    assert_eq!(ZOOM_LEVELS[(pos * 2.0) as usize], 0.5);
    for _ in 0..10 { pos = zoom_pos_clamp(pos + 0.5, 3456.0, 2234.0, 800.0); }
    assert_eq!(ZOOM_LEVELS[(pos * 2.0) as usize], 1.0, "never past native in game");
    assert_eq!(ZOOM_LEVELS[(zoom_pos_clamp(-5.0, 1920.0, 1080.0, 800.0) * 2.0) as usize], 0.25);
    assert_eq!([2.5f32, 2.0, 1.5].map(|p| ZOOM_LEVELS[(p * 2.0) as usize]), [0.7, 0.5, 0.35]);
}

/// The original's camera limits on a 200, 400 and 800 map, at every zoom step the window allows, from far outside in
/// all directions: the view never shows past the map's left, right or top edge (it stops one unit inside, a map
/// narrower than the window is centred) and past the bottom edge only under the HUD; zoom out stops where the map
/// fills the window (capZoom).
#[test]
fn camera_stays_on_the_map() {
    for size in [200.0f32, 400.0, 800.0] {
        for (w, h, sf) in [(1684.0f32, 784.0f32, 2.0f32), (1920.0, 1080.0, 1.0), (1280.0, 720.0, 1.0)] {
            let ui = (w / 1920.0).min(h / 1080.0);
            let (lo, hi) = (zoom_pos_clamp(-9.0, w * sf, h * sf, size), 3.0);
            let mut pos = lo;
            while pos <= hi {
                for (fx, fy) in [(-1.0f32, -1.0f32), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (0.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)] {
                    let m = MapBox { centre: [-25568.0, 0.0], size, rot: 0 };
                    let mut c = Cam { map: Some(m), zoom: ZOOM_LEVELS[(pos * 2.0) as usize] / sf, ..Default::default() };
                    // window centre far outside the map in direction (fx, fy)
                    let far = [m.centre[0] + fx * size * 40.0, m.centre[1] + fy * size * 20.0];
                    c.off = [w / 2.0 - far[0] * c.zoom, h / 2.0 - far[1] * c.zoom];
                    c.limit(w, h, sf, ui);
                    let cap = ((w * sf) / ((size - 4.0) * 32.0)).max((h * sf - 125.0 * ui * sf) / ((size - 6.0) * 16.0)).min(1.0);
                    assert!(c.zoom * sf >= cap - 1e-4, "size {size} pos {pos}: zoom {} under capZoom {cap}", c.zoom * sf);
                    let view = [-c.off[0] / c.zoom, -c.off[1] / c.zoom, (w - c.off[0]) / c.zoom, (h - c.off[1]) / c.zoom];
                    let map = [m.centre[0] - size * 16.0, m.centre[1] - size * 8.0, m.centre[0] + size * 16.0, m.centre[1] + size * 8.0];
                    let at = format!("size {size} window {w}x{h}@{sf} zoom {} from {fx},{fy}: view {view:?} map {map:?}", c.zoom * sf);
                    if view[2] - view[0] < map[2] - map[0] - 256.0 {
                        assert!(view[0] >= map[0] + 63.9 && view[2] <= map[2] - 63.9, "{at}: past a side edge");
                    } else {
                        assert!(((view[0] + view[2]) / 2.0 - m.centre[0]).abs() < 0.1, "{at}: not centred across");
                    }
                    let ui_world = 125.0 * ui / c.zoom;
                    if view[3] - view[1] - ui_world < map[3] - map[1] - 256.0 {
                        assert!(view[1] >= map[1] + 64.0 - 32.1, "{at}: past the top edge");
                        assert!(view[3] - ui_world <= map[3] - 64.0 + 16.1, "{at}: past the bottom edge above the HUD");
                    }
                }
                pos += 0.5;
            }
        }
    }
}


/// What GameMap draws reaches the viewer from a recording and finds its sprite: flies / projectiles (record 3), wall
/// fill-ins (7, GM 54), pixies (8 / 9: order flags and rings), the chimps' second sprite (k13 / k14), record 51's siege
/// flag (GM 189), health bar (GM 16), oil / recharge and target (GM 172), and record 52's slides.
#[test]
fn recording_types_drawn() {
    let rec = format!("{}/Library/Application Support/CrossOver/Bottles/Steam/drive_c/users/crossover/shc_recordings/rec_1792328742.shcrec", std::env::var("HOME").unwrap());
    if !std::path::Path::new(&rec).exists() || !cache_dir().join("atlases.tsv").exists() { return; }
    let (ticks, _) = replay::load(&rec);
    let names: Vec<String> = load_tables().1.into_iter().map(|(n, _)| n).collect();
    let gm = replay::Gm::new(&names);
    let mut w = replay::World::new();
    let mut seen: std::collections::BTreeMap<&str, (u32, u32)> = Default::default(); // kind -> (records, sprite found)
    let mut count = |k: &'static str, ok: bool| { let e = seen.entry(k).or_default(); e.0 += 1; e.1 += ok as u32; };
    for (t, recs) in ticks.iter().enumerate() {
        w.apply(recs, &mut vec![]);
        if t % 25 != 0 { continue; }
        for (&(ty, id), r) in &w.objs {
            match ty {
                3 => count("3 fly / projectile", gm.body(r[3], r[4]).is_some()),
                6 => count("6 whitecap", gm.body(173, r[4]).is_some()),
                7 => count("7 wall fill-in", gm.body(54, r[4]).is_some()),
                8 | 9 => count("8/9 pixie", gm.body(r[3], r[4]).is_some()),
                2 if r[14] > 0 => count("2 chimp GO2", gm.body(r[13] & 0x7ff, r[14]).is_some()),
                _ => {}
            }
            if ty != 2 { continue; }
            if let Some(m) = w.marks.get(&id) {
                if m[4] > 0 { count("51 siege flag", gm.body(189, m[4]).is_some()) }
                if m[10] > 0 { count("51 health bar", gm.body(16, m[10]).is_some()) }
                if m[11] > 400 { count("51 recharge bar", gm.body(16, m[11] - 400).is_some()) } else if m[11] > 0 { count("51 oil", gm.body(172, m[11]).is_some()) }
                if m[17] > 0 { count("51 target", gm.body(172, m[17]).is_some()) }
            }
            if w.next.contains_key(&id) { count("52 slide", true) }
        }
    }
    for (k, (n, ok)) in &seen { println!("{k:20} {n:7} records, sprite found {ok:7}"); }
    for k in ["3 fly / projectile", "8/9 pixie", "2 chimp GO2", "51 health bar", "51 recharge bar", "51 target", "52 slide"] {
        let (n, ok) = seen.get(k).copied().unwrap_or_default();
        assert!(n > 0 && ok * 100 >= n * 95, "{k}: {ok} of {n} drawn");
    }
}

