//! The OS mouse cursor: Director.setCursor's hardware cursors (Cursor.SetCursor, CursorStyle 0 = the default: the
//! 32×32 `*32` textures, CursorMode.Auto), exported by re/scripts/export_video.py into `video/cursors.rgba`.
//! winit's macOS custom cursor is w×h points (NSImage size = pixel size), so the 32 px image shows at the original's
//! size on Retina too (macOS scales it 2×). Elsewhere winit takes device pixels: there a 2× / 4× screen gets the game's
//! own 64 / 128 px art of the same cursors (sword / swordX ..., `cursors64.rgba` / `cursors128.rgba`).
//! The troop-order icons (move, attack, dig) are not OS cursors: the game draws them as a sprite at the mouse on top
//! of this one (GameMap mouseCursorGO = draw record type 5, main.rs / the HUD session).
use winit::{event_loop::ActiveEventLoop, window::{CustomCursor, Window}};

/// Director's cursorType 0..5 (also the image order in cursors.rgba).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind { Sword, Hand, Delete, DeleteNot, Wait, Scimitar }

/// setCursor's hotspots per cursorType, for the 32 px images (the 64 / 128 px ones are the same art at 2× / 4×).
const HOT: [(u16, u16); 6] = [(2, 1), (1, 1), (10, 10), (10, 10), (16, 16), (1, 1)];
/// Image sizes and their files; macOS only ever uses the 32 px one.
const SIZES: [(u16, &str); 3] = [(32, "cursors.rgba"), (64, "cursors64.rgba"), (128, "cursors128.rgba")];

/// The image size for a window at `scale` (device px per logical px): 32 on macOS (points); else 32 below 1.5×,
/// 64 below 3×, 128 above.
fn px_for(scale: f64, mac: bool) -> u16 {
    if mac || scale < 1.5 { 32 } else if scale < 3.0 { 64 } else { 128 }
}

/// Director.monitorCursors: outside a game the sword; in game the demolish tool (MainControls.CurrentAction 6) →
/// delete; campaign (game_type 0) missions 6..=10 and > 25 → scimitar, other campaign missions → sword; any other
/// game → scimitar for the Arabian / Bedouin lords (Options lord type 1, 2, 6, 7), else the sword.
/// Not used by the original's monitorCursors: hand (map editor), wait (SetWaitCursor has no caller), delete-not.
pub fn kind(in_game: bool, demolish: bool, campaign_mission: Option<i32>, lord_type: i32) -> Kind {
    if !in_game { return Kind::Sword }
    if demolish { return Kind::Delete }
    let arab = match campaign_mission {
        Some(m) => (6..=10).contains(&m) || m > 25,
        None => matches!(lord_type, 1 | 2 | 6 | 7),
    };
    if arab { Kind::Scimitar } else { Kind::Sword }
}

/// Cursor `k` at `px` (32 / 64 / 128): RGBA rows top-down (px × px) and its hotspot. None = not exported.
pub fn image(k: Kind, px: u16) -> Option<(Vec<u8>, (u16, u16))> {
    let file = SIZES.iter().find(|s| s.0 == px)?.1;
    let all = std::fs::read(crate::cache_dir().join("video").join(file)).ok()?;
    let n = px as usize * px as usize * 4;
    let (x, y) = HOT[k as usize];
    Some((all.get(k as usize * n..(k as usize + 1) * n)?.to_vec(), (x * px / 32, y * px / 32)))
}

/// The custom cursors, made once per exported size; `set` changes the window's only when the kind or the size
/// (the window's scale factor: another monitor) changes.
pub struct Cursors { all: Vec<(u16, Vec<CustomCursor>)>, cur: Option<(Kind, u16)> }

impl Cursors {
    pub fn new(el: &ActiveEventLoop) -> Option<Self> {
        let kinds = [Kind::Sword, Kind::Hand, Kind::Delete, Kind::DeleteNot, Kind::Wait, Kind::Scimitar];
        let all: Vec<_> = SIZES.iter().filter(|s| !cfg!(target_os = "macos") || s.0 == 32).filter_map(|&(px, _)| {
            let set = kinds.iter().map(|&k| {
                let (rgba, (x, y)) = image(k, px)?;
                CustomCursor::from_rgba(rgba, px, px, x, y).ok().map(|c| el.create_custom_cursor(c))
            }).collect::<Option<Vec<_>>>()?;
            Some((px, set))
        }).collect();
        if all.is_empty() { println!("no cursors: run re/scripts/export_video.py (system arrow for now)"); return None; }
        Some(Cursors { all, cur: None })
    }

    pub fn set(&mut self, w: &Window, k: Kind) {
        let want = px_for(w.scale_factor(), cfg!(target_os = "macos"));
        // the wanted size, else the nearest exported one (32 px is always first)
        let (px, set) = self.all.iter().min_by_key(|(px, _)| px.abs_diff(want)).unwrap();
        if self.cur != Some((k, *px)) { w.set_cursor(set[k as usize].clone()); self.cur = Some((k, *px)); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exported sword is the 32 px cursor with its blade tip at Director's hotspot (2, 1) (skips if not exported).
    /// A screenshot can't show the OS cursor, so this is the check that the sword is what the window gets.
    #[test]
    fn sword_cursor_loads() {
        let Some((px, hot)) = image(Kind::Sword, 32) else { return };
        assert_eq!((px.len(), hot), (32 * 32 * 4, (2, 1)));
        let a = |x: usize, y: usize| px[(y * 32 + x) * 4 + 3];
        // the tip: opaque pixels within 2 px of the hotspot, nothing above-left of it, the bottom-right corner clear
        assert!((0..4).any(|y| (0..5).any(|x| a(x, y) > 128)), "no tip near the hotspot");
        assert!(a(31, 0) == 0 && a(0, 31) == 0, "corners should be transparent");
        assert!(CustomCursor::from_rgba(px, 32, 32, hot.0, hot.1).is_ok());
        assert_eq!(image(Kind::Scimitar, 32).unwrap().1, (1, 1));
        assert_eq!(image(Kind::Delete, 32).unwrap().1, (10, 10));
    }

    /// Outside macOS a 2× / 4× screen gets the 64 / 128 px art, its hotspot scaled with it (the sword's tip stays
    /// under it); macOS always the 32 px one.
    #[test]
    fn hidpi_cursor_sizes() {
        assert_eq!([1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0].map(|s| px_for(s, false)), [32, 32, 64, 64, 64, 128, 128]);
        assert_eq!([1.0, 2.0, 4.0].map(|s| px_for(s, true)), [32; 3]);
        for px in [64u16, 128] {
            let Some((rgba, hot)) = image(Kind::Sword, px) else { return };
            let n = px as usize;
            assert_eq!((rgba.len(), hot), (n * n * 4, (2 * px / 32, px / 32)));
            let a = |x: usize, y: usize| rgba[(y * n + x) * 4 + 3];
            let r = 2 * n / 32 + 2; // the tip within ~2 source px of the hotspot, as at 32 px
            assert!((0..r).any(|y| (0..r).any(|x| a(x, y) > 128)), "{px} px: no tip near the hotspot");
            assert!(CustomCursor::from_rgba(rgba, px, px, hot.0, hot.1).is_ok());
            assert_eq!(image(Kind::Delete, px).unwrap().1, (10 * px / 32, 10 * px / 32));
        }
    }

    #[test]
    fn monitor_cursors_rules() {
        assert_eq!(kind(false, true, Some(7), 1), Kind::Sword); // front end: always the sword
        assert_eq!(kind(true, false, None, 0), Kind::Sword); // skirmish, Crusader lord (Options default)
        assert_eq!(kind(true, false, None, 6), Kind::Scimitar); // Arabian lady
        assert_eq!(kind(true, true, None, 6), Kind::Delete);
        assert_eq!(kind(true, false, Some(8), 1), Kind::Scimitar); // campaign mission 8 (Arabian side), lord ignored
        assert_eq!(kind(true, false, Some(11), 1), Kind::Sword);
        assert_eq!(kind(true, false, Some(26), 0), Kind::Scimitar);
    }
}
