//! A custom lord's backstory page (our extension, custom_lords.rs: portrait.png, arms.png, help.txt, bio.txt, the
//! neutral video), opened from the info buttons of the Add-AI custom list and of a custom lord's lobby row. Laid out as
//! FRONT_Story's pages on the same book (HistoricalCampaign-Background, the SCREEN image): in the drawing's place the
//! lord himself (his neutral video, faded to an oval; portrait.png until it is exported), name and title above, the
//! help rows below; the story typed onto the scroll (turned with the paper), the arms as a seal on its roll. One of his
//! lines greets you. With a trailer.webm, "Watch his story" plays it full screen with its sound (also straight away
//! after his zip is imported: `Menu::open_after_import`). A child module of menu.rs.
use super::*;

/// The open page: the lobby to go back to, the lord (by name), opened at `t0`, `all` = the story shown whole, `live` =
/// when the video first showed (the portrait fades out over it), `watch` = his trailer is (to be) playing.
pub struct Page { pub lobby: Lobby, pub name: String, t0: Instant, pub all: bool, live: Option<Instant>, pub watch: bool }

/// 1920 grid: the portrait (FRONT_Story's drawing rect), the seal on the scroll's lower roll; the scroll's writing
/// area (centre, half size) and its turn (the paper lies ~10° clockwise in the picture).
const PORTRAIT: [f32; 4] = [236.0, 215.0, 694.0, 409.0];
const SEAL: [f32; 4] = [1688.0, 728.0, 136.0, 136.0];
const SCROLL: [f32; 4] = [1545.0, 440.0, 245.0, 225.0];
const TURN: f32 = 10.0 * std::f32::consts::PI / 180.0;
const INK: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
const RED: [f32; 4] = [0.0865, 0.007, 0.007, 0.85]; // FRONT_Story's subtitle red (linear)

impl Menu {
    /// The page of `c`, back to `lobby`; `watch` = start his trailer (when he has one) instead of greeting.
    pub(super) fn lord_open(&mut self, c: crate::custom_lords::Custom, lobby: Lobby, watch: bool) {
        crate::custom_lords::export_videos(&c);
        let watch = watch && crate::custom_lords::has_trailer(&c);
        if !watch { crate::audio::custom_line(&c, "TAUNT1"); }
        self.screen = Screen::Lord(Box::new(Page { lobby, name: c.name, t0: Instant::now(), all: false, live: None, watch }));
    }
    /// Escape / Back: the trailer first, then the page.
    pub(super) fn lord_close(&mut self) {
        if let Screen::Lord(p) = &mut self.screen { if p.watch { p.watch = false; return; } }
        if let Screen::Lord(p) = std::mem::replace(&mut self.screen, Screen::Main) { self.screen = Screen::Lobby(p.lobby); }
    }

    /// lord_share.rs, after a lord's zip was imported: his page (back to the skirmish lobby, or the one open), his
    /// trailer playing when he has one. False = no such custom lord (the import didn't land).
    pub fn open_after_import(&mut self, name: &str) -> bool {
        crate::custom_lords::rescan();
        let Some(c) = crate::custom_lords::find(name) else { return false };
        let lobby = match &self.screen { Screen::Lobby(l) => l.clone(), Screen::Lord(p) => p.lobby.clone(), _ => Lobby::from_settings(PathBuf::new()) };
        self.lord_open(c, lobby, true);
        true
    }

    pub(super) fn lord_draw(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, p: &mut Page, w: f32, h: f32, cursor: [f32; 2]) {
        self.hits.push(([0.0, 0.0, w, h], Act::StoryAdvance));
        // menu.rs filled the window with the book; dim that and lay it again in the grid, so the text sits on it at
        // any window shape (the dimmed copy fills the bars)
        Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.93]);
        quad(out, SCREEN, self.r(0.0, 0.0, 1920.0, 1080.0), SCREEN_UV, [1.0; 4]);
        if let Some(c) = crate::custom_lords::find(&p.name) {
            // the lord: his video once exported, the portrait (SCREEN rows 1080.., `pixels`) before and fading out over it
            let r = self.r(PORTRAIT[0], PORTRAIT[1], PORTRAIT[2], PORTRAIT[3]);
            let shown = crate::custom_lords::vid(&c, "portrait").is_some_and(|f| crate::video::lord(out, &f, r, 1.0));
            if shown { p.live.get_or_insert_with(Instant::now); }
            let fade = p.live.map_or(1.0, |t| 1.0 - (t.elapsed().as_secs_f32() / 0.8).min(1.0));
            let th = SCREEN_H as f32;
            if fade > 0.0 { quad(out, SCREEN, r, [0.0, 1080.0 / th, PORTRAIT[2] / 1920.0, (1080.0 + PORTRAIT[3]) / th], [1.0, 1.0, 1.0, fade]); }
            // the book: name, title, the five rows
            self.label(ui, out, &c.name, 583.0, 112.0, 46.0, 0.5, INK);
            if let Some(t) = crate::custom_lords::help_text(&c, "TITLE") { self.label(ui, out, &t, 583.0, 166.0, 30.0, 0.5, RED); }
            let mut y = PORTRAIT[1] + PORTRAIT[3] + 22.0;
            for k in 0..5 {
                let Some(v) = crate::custom_lords::help(&c, k) else { continue };
                self.label(ui, out, &ui.text_of(&format!("TEXT_AI_LORD_HELP_{:03}", k + 1)), 250.0, y + 10.0, 21.0, 0.0, RED);
                y = self.para(ui, out, &v, 250.0, y + 24.0, 666.0, 22.0, INK) + 10.0;
            }
            // the scroll: the story typed like FRONT_Story's (a click shows it whole), laid out level, then turned with
            // the paper about the writing area's centre
            let from = out.len();
            let [cx, cy, hw, hh] = SCROLL;
            let mut left = if p.all { usize::MAX } else { (p.t0.elapsed().as_millis() / 10) as usize };
            let mut y = cy - hh;
            for para in crate::custom_lords::bio(&c) {
                y = self.para_n(ui, out, &para, cx - hw, y, 2.0 * hw, 21.0, INK, left, [f32::MIN, f32::MAX]) + 10.0;
                left = left.saturating_sub(para.chars().count());
            }
            let (o, (s, k)) = ([self.o[0] + cx * self.s, self.o[1] + cy * self.s], TURN.sin_cos());
            for t in &mut out[from..] {
                for v in &mut t.1 { let (dx, dy) = (v.pos[0] - o[0], v.pos[1] - o[1]); v.pos = [o[0] + dx * k - dy * s, o[1] + dx * s + dy * k]; }
            }
        }
        self.img_button(ui, out, ["UI-Buttons B005", "UI-Buttons B006", "UI-Buttons B005"], 60.0, 984.0, 151.0, 66.0, Some(Act::LordInfoClose), cursor);
        let f = (p.t0.elapsed().as_secs_f32() / 0.8).min(1.0); // FRONT_Story's fade in
        if f < 1.0 { Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, (1.0 - f).powi(3)]); }
        // his story: the button on the table, then the trailer over everything (video.rs's full-screen slot, its sound
        // with it; a click or Escape stops it, it ends by itself)
        let Some(c) = crate::custom_lords::find(&p.name).filter(crate::custom_lords::has_trailer) else { return };
        let file = crate::custom_lords::vid(&c, "trailer");
        let label = if file.is_some() { "Watch his story" } else { "Preparing his story..." };
        self.glow(ui, out, label, 1400.0, 975.0, 380.0, None, file.as_ref().map(|_| Act::LordTrailer), cursor);
        let Some(stem) = file.as_deref().and_then(|f| f.strip_suffix(".vid")).filter(|_| p.watch) else { return };
        match crate::video::mission_over(out, stem, false, w, h) {
            Some(true) => p.watch = false, // over
            _ => self.hits.push(([0.0, 0.0, w, h], Act::LordTrailer)),
        }
    }
}

/// `Menu::screen_image` "lord:<name>": the book with the arms turned onto the scroll's roll (1920×1080), and below it
/// (rows 1080.., SCREEN_H tall in all) the portrait faded to an oval, which `lord_draw` shows until the video runs.
pub fn pixels(name: &str) -> Option<Vec<u8>> {
    let mut px = std::fs::read(crate::cache_dir().join("ui/screens/HistoricalCampaign-Background.rgba")).ok().filter(|p| p.len() >= 1920 * 1080 * 4)?;
    px.resize(1920 * SCREEN_H as usize * 4, 0);
    let Some(c) = crate::custom_lords::find(name) else { return Some(px) };
    let [pw, ph] = [PORTRAIT[2] as usize, PORTRAIT[3] as usize];
    if let Some(pic) = crate::custom_lords::picture(&c, "portrait.png", pw as u32, ph as u32) {
        for y in 0..ph {
            for x in 0..pw {
                let (s, d) = ((y * pw + x) * 4, ((1080 + y) * 1920 + x) * 4);
                px[d..d + 3].copy_from_slice(&pic[s..s + 3]);
                px[d + 3] = (crate::custom_lords::oval(x, y, pw, ph) * 0.95 * pic[s + 3] as f32).round() as u8;
            }
        }
    }
    // the seal: arms.png turned by TURN, sampled bilinearly, over the book picture
    let [x0, y0, w, h] = SEAL.map(|v| v as usize);
    if let Some(pic) = crate::custom_lords::picture(&c, "arms.png", w as u32, h as u32) {
        let (s, k) = TURN.sin_cos();
        // premultiplied, so the transparent surround doesn't darken the edge
        let at = |x: i32, y: i32, i: usize| if (0..w as i32).contains(&x) && (0..h as i32).contains(&y) {
            let q = &pic[(y as usize * w + x as usize) * 4..][..4];
            q[i] as f32 * if i < 3 { q[3] as f32 / 255.0 } else { 1.0 }
        } else { 0.0 };
        let (m, n) = (w as f32 / 2.0, h as f32 / 2.0);
        for y in 0..h + 20 {
            for x in 0..w + 20 {
                let (dx, dy) = (x as f32 - 10.0 - m + 0.5, y as f32 - 10.0 - n + 0.5);
                let (u, v) = (m + dx * k + dy * s - 0.5, n - dx * s + dy * k - 0.5); // inverse turn
                let (ix, iy, fx, fy) = (u.floor() as i32, v.floor() as i32, u - u.floor(), v - v.floor());
                let px4 = |i: usize| (at(ix, iy, i) * (1.0 - fx) + at(ix + 1, iy, i) * fx) * (1.0 - fy) + (at(ix, iy + 1, i) * (1.0 - fx) + at(ix + 1, iy + 1, i) * fx) * fy;
                let a = px4(3) / 255.0;
                if a <= 0.0 { continue; }
                let d = ((y0 + y - 10) * 1920 + x0 + x - 10) * 4;
                for i in 0..3 { px[d + i] = (px[d + i] as f32 * (1.0 - a) + px4(i)).round().clamp(0.0, 255.0) as u8; }
            }
        }
    }
    Some(px)
}
