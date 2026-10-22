//! Map tile layers from a `.map` file. Layer meanings are guesses (see README).
use std::path::PathBuf;

pub const CELLS: usize = 160_400;
pub const ROWS: usize = 800;
pub const TILE_W: f32 = 64.0; // DE tile sprites are 64x32 diamonds
pub const TILE_H: f32 = 32.0;

pub struct Tile { pub x: f32, pub y: f32, pub ground: u8, pub rand: u32 }

fn maps_dir() -> PathBuf {
    std::env::var_os("SHC_MAPS").map(PathBuf::from).unwrap_or_else(|| crate::paths::streaming_assets().join("Maps"))
}

/// Screen-space centre of each cell: 800 rows of width 1..400,400..1, each row centred,
/// rows half a tile apart vertically. World origin = top vertex of the diamond.
/// Rows 399 and 400 both have 400 cells, so the bottom half is shifted half a tile to interlock
/// (direction is a guess; left vs right is indistinguishable without in-game reference).
pub fn cell_pos() -> Vec<(f32, f32)> {
    (0..ROWS).flat_map(|r| {
        let w = if r < ROWS / 2 { r + 1 } else { ROWS - r };
        let shift = if r < ROWS / 2 { 0.0 } else { TILE_W / 2.0 };
        (0..w).map(move |i| ((i as f32 - (w as f32 - 1.0) / 2.0) * TILE_W + shift, r as f32 * TILE_H / 2.0))
    }).collect()
}

pub fn load(name: &str) -> Vec<Tile> {
    let path = if name.ends_with(".map") { PathBuf::from(name) } else { maps_dir().join(format!("{name}.map")) };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let m = shc_formats::map::parse(&bytes).expect("map parse");
    let layer = |id: i32| -> &[u8] {
        let s = m.sections.iter().find(|s| s.type_id == id).unwrap_or_else(|| panic!("no section {id}"));
        assert_eq!(s.data.len() % CELLS, 0, "section {id} is not a tile layer");
        &s.data
    };
    // 3037: 2 B/cell, each byte one flag bit (both bytes almost always equal); low byte used.
    let ground = layer(3037);
    let rand = layer(3008);
    cell_pos().into_iter().enumerate().map(|(i, (x, y))| Tile {
        x, y, ground: ground[i * 2],
        rand: u32::from_le_bytes(rand[i * 4..i * 4 + 4].try_into().unwrap()),
    }).collect()
}

#[test]
fn diamond_layout() {
    let p = cell_pos();
    assert_eq!(p.len(), CELLS);
    assert_eq!(p[0], (0.0, 0.0));
    assert_eq!(p[1], (-32.0, 16.0)); // row 1 = two cells, half a tile left/right
    assert_eq!(p[2], (32.0, 16.0));
    assert_eq!(p[CELLS / 2 - 1].0 + 32.0, p[CELLS / 2 + 399].0); // rows 399/400 interlock
    assert_eq!(*p.last().unwrap(), (32.0, 799.0 * 16.0));
}
