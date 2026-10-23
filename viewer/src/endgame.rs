//! End of a game, like the original. The sim decides (game::end_check = FUN_18001d130 for skirmish / MP: every 100
//! ticks a player whose lord is gone is out, PlayerRecord +0x2210; when one team is left every player gets +0x2244
//! = 1 won / 2 lost; scenario actions set +0x2244 in missions). The front end then shows OST_MP_GAME_OVER
//! (OnScreenText.cs): IngameUIScreens.triggerVideos drops the faction's banner video (528 px, top centre) with
//! "Victory" / "Defeat" (TEXT_MAINOPTIONS_017 / 018) faded in on it, and IngameUIScreens.xaml's 500 px BDR_SH1DEb
//! box lists "<name> Wins" per winner in team colour. ~6 s later FUN_180087730's render event 0xc opens the score
//! screen (HUD_MissionOver) and stops the sim.
//! Native: a skirmish-mode / LAN game's end opens the score screen after the banner (masters::Score) over its faction's
//! win / lose video; a campaign mission's plays that video full screen 3 s after the banner (LoadVictoryScreen;
//! the original goes on when it ends or on a click), then the box with the buttons over its last frame. Both wait for
//! the engine's minimap video / speech first (Director.ManageGameOver, `Screen::waited`) and come with
//! ManageGameOver's advisor line and win / lose tune (audio::game_over) and stop the game. A won Sands of Time mission
//! shows no video: its timed results first (trail_menu::Outro, Stats = the table); a trail's last mission won ends
//! with its completion screen (trail_menu::Finale) on Exit. Buttons: the original's Exit
//! (= Continue: the next mission / story / the skirmish setup, a lost mission again) and the port's Play Again, Main
//! Menu, Save Replay. A player who is out while others still fight keeps
//! watching in the original (no banner, only the "lord killed" message bar); here the Defeat banner says so then,
//! with Keep Watching, and the final result shows again when the game ends.
use crate::hud::{HudState, HudTri, Ui};
use crate::menu::trail_menu::{self, Finale, Outro};
use std::time::Instant;

/// What the end screen shows (HudState::end, read on the sim thread).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct End {
    pub result: u8,                 // 0 = you are out, the others still fight; 1 won; 2 lost
    pub winners: Vec<(u8, String)>, // player, name (text key of an AI lord's name, else plain)
    pub colours: [u8; 9],           // the players' colours (`radar_colours`; 0 = the default, `colour`)
}

/// The local player's end state, None while still playing.
#[cfg(feature = "live")]
pub fn read(m: &shc_sim::mem::Mem) -> Option<End> {
    let rec = shc_sim::game::player(m.i32(0x1888e0d60) as u32);
    let result = m.i32(rec + 0x2244) as u8;
    if result == 0 && m.i32(rec + 0x2210) == 0 {
        return None;
    }
    let winners = (1..9u8).filter(|&p| result != 0 && m.i16(0x1837ed8c6 + (p as u64 - 1) * 2) != 0)
        .map(|p| (p, name(m.i32(0x188571c34 + p as u64 * 4), m.i16(0x188571c58 + p as u64 * 4) as i32))).collect();
    Some(End { result, winners, colours: radar_colours(m) })
}

/// The players' colour indices as the game set them up (DLL_SetMPRadarColours' array, skirmish::RADAR_COLOURS: the
/// lobby's picks); 0 = none set.
#[cfg(feature = "live")]
pub fn radar_colours(m: &shc_sim::mem::Mem) -> [u8; 9] {
    std::array::from_fn(|p| m.i32(crate::skirmish::RADAR_COLOURS + p as u64 * 4).clamp(0, 8) as u8)
}

/// OnScreenText.getComputerName(lord kind, name variant) as a strings.tsv key; kind 0 = a human.
/// The human's name: Options > Player Settings (else the login name); kinds 30.. (extended lords) fall back to kind 1.
pub(crate) fn name(kind: i32, v: i32) -> String {
    let key = |sec: &str, base: i32| format!("{sec}_{:03}", base + v + 1); // lookUpText is 0-based
    match kind {
        0 => crate::menu::coa::player_name(),
        1..=16 => key("TEXT_XPLAY_WAITING_ROOM", 111 + 8 * (kind - 1)),
        17..=25 => key("TEXT_NEW_CTEXT", 16 + 8 * (kind - 17)),
        26..=29 => key("TEXT_NEW_CTEXT", [454, 471, 488, 505][kind as usize - 26]),
        _ => key("TEXT_XPLAY_WAITING_ROOM", 111),
    }
}

/// SpriteMapping.shcRemapColours: the default colour index of player p (the map's colour table).
const SHC: [u8; 9] = [0, 1, 3, 4, 2, 6, 5, 7, 8];
/// Player p's colour index (SpriteMapping.remapColours[p]): `colours[p]` as the game set it up, else the default.
pub(crate) fn colour(colours: &[u8], p: u8) -> u8 { match colours.get(p as usize) { Some(&c) if c > 0 => c.min(8), _ => SHC[p.min(8) as usize] } }

/// OnScreenText.MPTeamColours[c] for colour index c (sRGB; sprite.wgsl decodes vertex colours).
pub(crate) fn rgb(c: u8) -> [f32; 4] {
    const MP: [[f32; 3]; 9] = [[1.0, 1.0, 1.0], [0.769, 0.008, 0.008], [0.275, 0.275, 0.784], [0.784, 0.380, 0.024], [0.776, 0.765, 0.0],
        [0.565, 0.0, 0.565], [0.502, 0.502, 0.502], [0.035, 0.757, 0.749], [0.008, 0.784, 0.008]];
    let c = MP[c.min(8) as usize];
    [c[0], c[1], c[2], 1.0]
}

/// GetColourShield(p, 0, ingameRemap): the colour shield of colour index c (GameSprites 107..114; `colour`).
pub(crate) fn shield(c: u8) -> &'static str {
    const BY_COLOUR: [&str; 9] = ["", "UI-Buttons H016", "UI-Buttons H007", "UI-Buttons H010", "UI-Buttons H013", "UI-Buttons H022", "UI-Buttons H019", "UI-Buttons H025", "UI-Buttons H028"];
    BY_COLOUR[c.min(8) as usize]
}

/// HUD_MissionOver victory_videos / defeat_videos by faction (lord_Type 1, 6 Arab; 2, 7 Bedouin).
fn mission_video(lord_type: u8, won: bool) -> String {
    format!("{}_{}", match lord_type { 1 | 6 => "arabian", 2 | 7 => "bedouin", _ => "crusader" }, if won { "win" } else { "lose" })
}

/// What the end screen's buttons ask main.rs for (HUD_MissionOver.ButtonClicked "Exit" = Continue).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Exit { Continue, Again, Menu }

/// FUN_180087730's render event 0xc: the score screen this long after the banner (s).
const SCORE_AFTER: f32 = 6.0;
/// Director.ManageGameOver after a campaign mission's banner: 3 s (then `Screen::waited`).
const MISSION_OVER_AFTER: f32 = 3.0;

/// The overlay's state: which result is up since when, which one Keep Watching closed, the score screen once open.
#[derive(Default)]
pub struct Screen {
    shown: Option<(u8, Instant)>,
    closed: Option<u8>,
    hits: Vec<([f32; 4], u8)>, // 0 = Keep Watching, 1 = Continue, 2 = Save Replay, 3 = Play Again, 4 = Main Menu
    pub save: bool,  // Save Replay clicked: main.rs sends live::Cmd::SaveReplay (game_replay.rs)
    saved: bool,
    score: Option<crate::menu::trail_menu::masters::Score>,
    pub graphs: Option<bool>, // the stats graphs on / off asked for (main.rs sets stats::Panel::on)
    graphs_on: bool,
    over: bool,       // ManageGameOver came: its sounds played, the game stopped
    video_done: bool, // a campaign mission's video ended or was clicked: its box shows over the last frame
    video_hid_graphs: bool, // the stats graphs are off for the video
    gate: Option<(u8, Instant)>, // Director.ManageGameOver's wait (`waited`): stage, since
    pub trail: Option<(i32, i32, i32)>, // main.rs, every frame: the trail mission (trail, id 0-based) and the game tick
    ticks: i32, // the game tick when the end came (the Sands results' time)
    outro: Option<(Outro, bool)>, // a won timed Sands mission's results, up (Stats hides them; the table's Exit brings them back)
    finale: Option<(Finale, Instant)>, // a trail's last mission won: its completion screen since
}

impl Screen {
    /// Triangles for this frame (none while playing). `st` = the latest HudState; a new game (no `end`) resets.
    /// A campaign mission (`st.ev.campaign`) gets its mission-over video, then the box with its buttons (the original's
    /// ShowVictory / ShowDefeat: no score table); `again`: the game can be restarted (Play Again; not a loaded save).
    pub fn draw(&mut self, st: &HudState, ui: &mut Ui, w: f32, h: f32, cursor: [f32; 2], again: bool) -> Vec<HudTri> {
        self.hits.clear();
        let Some(end) = &st.end else { *self = Self::default(); return vec![] };
        if self.closed == Some(end.result) { return vec![]; }
        // IngameUIScreens.triggerVideos: the banner sting with the banner (320 Victory / 321 Defeat; none for 0, no banner)
        let since = match self.shown { Some((r, t)) if r == end.result => t, _ => {
            if end.result != 0 { crate::audio::ui(if end.result == 1 { crate::audio::ui_id::VICTORY } else { crate::audio::ui_id::DEFEAT }, 1.0) }
            (self.score, self.over, self.video_done, self.video_hid_graphs, self.gate, self.outro, self.finale) = (None, false, false, false, None, None, None);
            self.ticks = self.trail.map_or(0, |t| t.2);
            // the stills the results / the finale show, made while the banner is up (trail_menu::still)
            if let (1, Some((tr, id, _))) = (end.result, self.trail) {
                if (11..=17).contains(&tr) { trail_menu::still("sands-score-bg"); }
                if let Some(Finale::Weasel(_)) = Finale::after(tr, id) { trail_menu::still("weasel"); }
            }
            self.shown.insert((end.result, Instant::now())).1
        } };
        let won = end.result == 1;
        let after = if st.ev.campaign { MISSION_OVER_AFTER } else { SCORE_AFTER };
        // GameData.ManageGameOver: the sim stops, the advisor's line and the win / lose tune (audio::game_over)
        if end.result != 0 && !self.over && since.elapsed().as_secs_f32() >= after && self.waited(st) {
            self.over = true;
            crate::audio::game_over(won, !st.ev.campaign);
        }
        let wait = end.result != 0 && !self.over; // the box's buttons wait for it
        // a skirmish-mode / LAN game's end: the score screen after the banner (ShowMPVictory / ShowMPDefeat)
        if !st.ev.campaign && self.over && self.score.is_none() {
            if let Some(e) = crate::menu::trail_menu::masters::peek() {
                self.score = Some(crate::menu::trail_menu::masters::Score::new(e));
                (self.graphs, self.graphs_on) = (Some(false), false); // they would cover the rows: the Graphs button
                // a won timed Sands mission: InitSands' results first (MO_SandsOutro1), the table behind Stats
                self.outro = self.trail.filter(|_| won).and_then(|(tr, id, _)| Outro::new(tr, id, self.ticks)).map(|o| (o, true));
            }
        }
        let mut out = vec![];
        if let Some((f, t0)) = &self.finale {
            f.draw(ui, &mut out, t0.elapsed().as_secs_f32(), w, h);
            self.hits.push(([0.0, 0.0, w, h], 7)); // InvisibleButton
            return out;
        }
        if let Some(sc) = &mut self.score {
            // LoadMPVictoryScreen / LoadMPDefeatScreen: the same video behind the table (skirmishMasters' lord_type
            // pick = the faction); a won Sands of Time mission has none (its timed results: SandsofTimeScoreBackground)
            if !(won && self.trail.is_some_and(|t| (11..=17).contains(&t.0))) {
                crate::video::mission_over(&mut out, &mission_video(st.lord_type, won), false, w, h);
            } else if self.outro.is_some() {
                trail_menu::backdrop(&mut out, "sands-score-bg", w, h);
            }
            match &mut self.outro {
                Some((o, true)) => o.draw(ui, &mut out, w, h, cursor),
                _ => sc.draw(ui, &mut out, won, again, self.saved, w, h, cursor),
            }
            return out;
        }
        let s = crate::ux::hud_scale(w, h); // the HUD's scale: a different ui.s makes Ui re-rasterise every glyph
        (ui.s, ui.w, ui.h) = (s, w, h);
        // banner: Banner-<faction>-Victory / -Defeat.webm played once, its last frame stays (lord_Type 1, 6 Arab; 2, 7 Bedouin)
        let faction = match st.lord_type { 1 | 6 => "arab", 2 | 7 => "bedouin", _ => "crusader" };
        // a campaign mission's ManageGameOver: HUD_MissionOver.LoadVictoryScreen / LoadDefeatScreen's video (ShowVictory's
        // `screen` = triggerVideos' faction) full screen over a black ground, no banner. A click (InvisibleButton) or its
        // end brings the box over its last frame (the original goes on at once; the box keeps the port's buttons)
        if self.over && st.ev.campaign {
            if let Some(ended) = crate::video::mission_over(&mut out, &mission_video(st.lord_type, won), false, w, h) {
                self.video_done |= ended;
                // the end-screen graphs (stats.rs) wait under the box until the video is done
                if self.video_hid_graphs == self.video_done { self.video_hid_graphs = !self.video_done; self.graphs = Some(self.video_done); }
                if !self.video_done { self.hits.push(([0.0, 0.0, w, h], 6)); return out; }
                return self.end_box(ui, out, end, s, w, h, cursor, again, false);
            }
        }
        let (bw, bh) = (528.0 * s, 307.0 * s);
        let (key, p) = (format!("banner_{faction}_{}", if won { "victory" } else { "defeat" }), [(w - bw) / 2.0, -20.0 * s, (w + bw) / 2.0, bh - 20.0 * s]);
        if !crate::video::banner(&mut out, &key, p) { crate::menu::pic(&mut out, &key, p, 1.0); } // the drop video (video.rs), else its last frame
        // BannerLoopText: 34 px black, #CCC3B85D glow (blur 10), opacity 0 -> 1 from 0.5 to 1 s (BannerTextFade)
        let a = ((since.elapsed().as_secs_f32() - 0.5) / 0.5).clamp(0.0, 1.0);
        let text = ui.text_of(if won { "TEXT_MAINOPTIONS_017" } else { "TEXT_MAINOPTIONS_018" });
        let base = (132.0 + 34.0 * 0.8) * s;
        for (dx, dy) in [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0)] {
            ui.text(&mut out, &text, w / 2.0 + dx * s, base + dy * s, 34.0, 1.0, 0.0, 0.5, [0.765, 0.722, 0.365, 0.5 * a]);
        }
        ui.text(&mut out, &text, w / 2.0, base, 34.0, 1.0, 0.0, 0.5, [0.0, 0.0, 0.0, a]);
        self.end_box(ui, out, end, s, w, h, cursor, again, wait)
    }

    /// The box, centred: 20 px, a 24 px row per winner ("<name> Wins", 18 px), 20 px; then our buttons (none while
    /// the score screen / mission-over video is due, as the original: `wait`).
    #[allow(clippy::too_many_arguments)]
    fn end_box(&mut self, ui: &mut Ui, mut out: Vec<HudTri>, end: &End, s: f32, w: f32, h: f32, cursor: [f32; 2], again: bool, wait: bool) -> Vec<HudTri> {
        let replay = if self.saved { "Replay saved" } else { "Save Replay" };
        let buttons: Vec<(&str, u8)> = match end.result {
            0 => vec![("Main Menu", 4), (replay, 2), ("Keep Watching", 0)],
            _ if wait => vec![],
            _ => [("Continue", 1), ("Play Again", 3), ("Main Menu", 4), (replay, 2)].into_iter().filter(|b| again || b.1 != 3).collect(),
        };
        let bh = 20.0 + 24.0 * end.winners.len() as f32 + 20.0 + 60.0 * buttons.len() as f32;
        let (x0, y0) = ((w - 500.0 * s) / 2.0, (h - bh * s) / 2.0);
        // BDR_SH1DEb: parchment, the carved frame
        ui.img(&mut out, "Panel_BG", [x0, y0, x0 + 500.0 * s, y0 + bh * s], [1.0, 1.0, 1.0, 0.95]);
        ui.frame_c(&mut out, [x0, y0, x0 + 500.0 * s, y0 + bh * s], s);
        let wins = ui.text_of("TEXT_XPLAY_WAITING_ROOM_065");
        for (k, (p, n)) in end.winners.iter().enumerate() {
            let row = format!("{} {wins}", ui.text_of(n));
            ui.text_sh(&mut out, &row, w / 2.0, y0 + (20.0 + 24.0 * k as f32 + 18.0) * s, 18.0, 0.5, rgb(colour(&end.colours, *p)));
        }
        let mut y = y0 + (40.0 + 24.0 * end.winners.len() as f32) * s;
        for (label, b) in buttons {
            let r = [w / 2.0 - 170.0 * s, y, w / 2.0 + 170.0 * s, y + 48.0 * s];
            let hot = cursor[0] >= r[0] && cursor[0] < r[2] && cursor[1] >= r[1] && cursor[1] < r[3];
            ui.text_button(&mut out, label, r, hot, false, b != 2 || !self.saved); // saved: Save Replay does nothing
            self.hits.push((r, b));
            y += 60.0 * s;
        }
        out
    }

    fn hit(&self, c: [f32; 2]) -> Option<u8> {
        self.hits.iter().find(|(r, _)| c[0] >= r[0] && c[0] < r[2] && c[1] >= r[1] && c[1] < r[3]).map(|&(_, m)| m)
    }
    /// The score screen / mission-over video is up: the game stays paused under it (FUN_180087730's render event 0xc,
    /// ManageGameOver's stopSimThread).
    pub fn scoring(&self) -> bool { self.score.is_some() || self.over }
    /// The cursor is on one of the overlay's buttons (last frame's layout); the score screen takes every click.
    pub fn over(&self, c: [f32; 2]) -> bool { self.score.is_some() || self.hit(c).is_some() }
    /// Left press: what main.rs does next, if anything. Keep Watching closes the overlay until the result changes.
    pub fn click(&mut self, c: [f32; 2]) -> Option<Exit> {
        use crate::menu::trail_menu::masters::Btn;
        if self.finale.is_some() { return Some(Exit::Continue); } // the completion screen's InvisibleButton
        if let Some((o, up @ true)) = &mut self.outro {
            match o.click(c)? { true => *up = false, false => return self.finish() } // Stats / Exit
            return None;
        }
        let b = match &mut self.score {
            Some(sc) => match sc.click(c)? { Btn::Exit => 1, Btn::Replay => 2, Btn::Again => 3, Btn::Menu => 4, Btn::Graphs => 5, _ => return None },
            None => self.hit(c)?,
        };
        match b {
            0 => self.closed = self.shown.map(|(r, _)| r),
            6 => self.video_done = true,
            2 => { self.save |= !self.saved; self.saved = true }
            5 => { self.graphs_on = !self.graphs_on; self.graphs = Some(self.graphs_on) }
            3 => return Some(Exit::Again),
            4 => return Some(Exit::Menu),
            // ButtonClicked("Exit") on the table of a Sands mission's results: back to them
            _ if self.outro.is_some() => self.outro.as_mut().unwrap().1 = true,
            _ => return self.finish(),
        }
        None
    }

    /// ButtonClicked("Exit") past the results: a trail's last mission won brings its completion screen first
    /// (`Finale`), anything else goes on.
    fn finish(&mut self) -> Option<Exit> {
        let won = self.shown.is_some_and(|s| s.0 == 1);
        let Some(f) = self.trail.filter(|_| won && self.finale.is_none()).and_then(|(t, id, _)| Finale::after(t, id)) else { return Some(Exit::Continue) };
        f.start();
        self.finale = Some((f, Instant::now()));
        (self.graphs, self.graphs_on) = (Some(false), false);
        None
    }

    /// Director.ManageGameOver's wait once the banner's time is up: while the engine's video plays (RadarME; at most
    /// 15 s), then (it ran over) while speech channel 1 talks (at most 15 s), then 1 s; none playing: at once.
    /// ponytail: past the 15 s the original also stops the video (RadarME_Ended) and the game's sounds; here they only
    /// stop being waited for (the end screen covers the minimap).
    fn waited(&mut self, st: &HudState) -> bool {
        let video = st.ev.video.playing();
        let (stage, t) = match self.gate {
            None if !video => return true,
            None => { self.gate = Some((0, Instant::now())); return false }
            Some(g) => g,
        };
        let s = t.elapsed().as_secs_f32();
        let next = match stage {
            0 if !video => 2,
            0 if s >= 15.0 => 1,
            1 if !crate::audio::speech_state().0 || s >= 15.0 => 2,
            2 if s >= 1.0 => return true,
            _ => return false,
        };
        self.gate = Some((next, Instant::now()));
        false
    }

    /// The full-screen still the screen up now wants in HUD texture SCREEN (main.rs uploads it while no menu is open):
    /// the Sands results' background, the classic trails' finale still. None until it is made (`trail_menu::still`).
    pub fn screen_image(&self) -> Option<String> {
        let name = match (&self.finale, &self.outro) {
            (Some((Finale::Weasel(_), _)), _) => "weasel",
            (None, Some(_)) => "sands-score-bg",
            _ => return None,
        };
        trail_menu::still(name).then(|| name.to_string())
    }
}

/// The end box's buttons: a campaign mission's after the banner and its video (Continue, Play Again unless it cannot
/// restart, Main Menu, Save Replay); a skirmish's none while the score screen is due, then the same box when there
/// are no statistics to score; you out while the others fight: Main Menu / Save Replay / Keep Watching.
#[test]
fn end_buttons() {
    let Some(mut ui) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let st = |result, campaign| HudState { end: Some(End { result, winners: vec![(2, "TEXT_XPLAY_WAITING_ROOM_112".into())], ..Default::default() }), ev: crate::hud::Events { campaign, ..Default::default() }, ..Default::default() };
    let ids = |s: &Screen| s.hits.iter().map(|h| h.1).collect::<Vec<_>>();
    let press = |s: &mut Screen, id: u8| { let r = s.hits.iter().find(|h| h.1 == id).unwrap().0; s.click([(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]) };
    let mut s = Screen::default();
    s.draw(&st(1, true), &mut ui, 1920.0, 1080.0, [0.0; 2], true);
    assert!(ids(&s).is_empty() && !s.scoring(), "the banner first");
    s.shown = Some((1, Instant::now() - std::time::Duration::from_secs(5)));
    s.draw(&st(1, true), &mut ui, 1920.0, 1080.0, [0.0; 2], true);
    assert!(s.scoring(), "ManageGameOver stops the game");
    if ids(&s) == [6] { // the mission-over video plays (exported, pumped): a click brings the box
        assert!(s.over([5.0, 5.0]) && s.click([5.0, 5.0]).is_none());
        s.draw(&st(1, true), &mut ui, 1920.0, 1080.0, [0.0; 2], true);
    }
    assert_eq!(ids(&s), [1, 3, 4, 2]);
    assert_eq!((press(&mut s, 1), press(&mut s, 3), press(&mut s, 4)), (Some(Exit::Continue), Some(Exit::Again), Some(Exit::Menu)));
    assert_eq!((press(&mut s, 2), s.save), (None, true));
    s.draw(&st(1, true), &mut ui, 1920.0, 1080.0, [0.0; 2], false);
    assert_eq!(ids(&s), [1, 4, 2], "a loaded save: no Play Again");
    let mut s = Screen::default();
    s.draw(&st(2, false), &mut ui, 1920.0, 1080.0, [0.0; 2], true);
    assert!(ids(&s).is_empty() && !s.over([960.0, 540.0]), "the score screen is due");
    s.shown = Some((2, Instant::now() - std::time::Duration::from_secs(7)));
    *crate::menu::trail_menu::masters::CAPTURED.lock().unwrap() = None;
    s.draw(&st(2, false), &mut ui, 1920.0, 1080.0, [0.0; 2], true);
    assert_eq!(ids(&s), [1, 3, 4, 2], "nothing to score: the box");
    let mut s = Screen::default();
    s.draw(&st(0, false), &mut ui, 1920.0, 1080.0, [0.0; 2], true);
    assert_eq!(ids(&s), [4, 2, 0]);
    assert_eq!(press(&mut s, 0), None);
    assert!(s.draw(&st(0, false), &mut ui, 1920.0, 1080.0, [0.0; 2], true).is_empty(), "Keep Watching closed it");
}

/// Director.ManageGameOver's wait: none playing = at once; a video that ends = 1 s later; one running over 15 s =
/// then the speech (quiet here), then 1 s.
#[test]
fn game_over_waits_for_the_video() {
    use std::time::Duration;
    let mut st = HudState::default();
    assert!(Screen::default().waited(&st), "none playing: at once");
    st.ev.video.now = Some(("st03_woodcutters_hut".into(), true, 1)); // looped: plays until stopped
    let mut s = Screen::default();
    assert!(!s.waited(&st) && !s.waited(&st));
    s.gate = Some((0, Instant::now() - Duration::from_secs(16)));
    assert!(!s.waited(&st) && s.gate.unwrap().0 == 1, "15 s: the speech next");
    assert!(!s.waited(&st) && s.gate.unwrap().0 == 2, "no speech: the last second");
    s.gate = Some((2, Instant::now() - Duration::from_secs(2)));
    assert!(s.waited(&st));
    let mut s = Screen::default();
    s.waited(&st);
    st.ev.video.now = None;
    assert!(!s.waited(&st) && s.gate.unwrap().0 == 2, "the video ended: 1 s more");
}

/// A won timed Sands mission: its results (no table) → Stats → the table → Exit → the results again → Exit → the
/// trail's last mission: the completion screen → a click goes on; a mission before the last goes on from the results.
#[test]
fn sands_results_and_finale() {
    let Some(mut ui) = Ui::load(&crate::cache_dir().join("ui")) else { return };
    let dir = std::env::temp_dir().join(format!("shc-endgame-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.cfg"), "||SETTINGS||\nSands1Trail:5\n||SETTINGS||\n").unwrap();
    crate::skirmish::set_test_profile(&dir);
    let st = HudState { end: Some(End { result: 1, ..Default::default() }), ..Default::default() };
    for (id, last) in [(4, true), (2, false)] {
        let mut s = Screen { trail: Some((11, id, 2000)), ..Default::default() };
        s.draw(&st, &mut ui, 1920.0, 1080.0, [0.0; 2], true);
        (s.over, s.score) = (true, Some(crate::menu::trail_menu::masters::Score::new(crate::menu::trail_menu::masters::J::Obj(vec![]))));
        s.outro = Outro::new(11, id, s.ticks).map(|o| (o, true));
        s.draw(&st, &mut ui, 1920.0, 1080.0, [0.0; 2], true);
        assert!(s.outro.as_ref().is_some_and(|o| o.1), "the results first");
        assert_eq!(s.click([1830.0, 860.0]), None, "Stats");
        assert!(s.outro.as_ref().is_some_and(|o| !o.1));
        s.draw(&st, &mut ui, 1920.0, 1080.0, [0.0; 2], true);
        assert_eq!(s.click([1808.0, 420.0]), None, "the table's Exit: back to the results");
        assert!(s.outro.as_ref().is_some_and(|o| o.1));
        s.draw(&st, &mut ui, 1920.0, 1080.0, [0.0; 2], true);
        if !last { assert_eq!(s.click([1838.0, 990.0]), Some(Exit::Continue)); continue; }
        assert_eq!(s.click([1838.0, 990.0]), None, "Exit: the completion screen");
        assert_eq!(s.finale.map(|f| f.0), Some(Finale::Desert(11)));
        s.draw(&st, &mut ui, 1920.0, 1080.0, [0.0; 2], true);
        assert!(s.over([5.0, 5.0]) && s.click([5.0, 5.0]) == Some(Exit::Continue));
    }
    assert_eq!([Finale::after(0, 49), Finale::after(2, 19), Finale::after(0, 48), Finale::after(22, 9), Finale::after(90, 0)],
        [Some(Finale::Weasel(false)), Some(Finale::Weasel(true)), None, Some(Finale::Desert(22)), None]);
}

/// SHC_SAVES=<dir> (screenshots): `Won.sav`, a skirmish vs the Rat just won (his lord killed), for the score screen;
/// you picked blue (Options colour 3), so the Rat takes red.
#[cfg(feature = "live")]
#[test]
#[ignore]
fn write_won_save() {
    if std::env::var_os("SHC_SAVES").is_none() { return; }
    let map = crate::skirmish::maps_dir().join("Crater Lake.map");
    if !map.exists() { return; }
    let mut m = crate::skirmish::start(&crate::skirmish::Lobby { ais: vec![(0, 0)], seed: 1234, colour: 3, ..crate::skirmish::Lobby::new(map) }).unwrap();
    let mut ret = vec![0u8; shc_sim::hudstate::RET_LEN];
    let mut tick = |m: &mut shc_sim::mem::Mem| { shc_sim::frontend::run_tick(m, &shc_sim::frontend::TickInput::default(), &mut ret, None); };
    for _ in 0..300 { tick(&mut m); }
    let (me, rat) = (m.i32(shc_sim::game::player(1) + 0x21f8), m.i32(shc_sim::game::player(2) + 0x21f8));
    shc_sim::units::helpers::apply_hit(&mut m, me, rat, 10_000_000);
    for _ in 0..400 { tick(&mut m); }
    assert!(read(&m).is_some_and(|e| e.result == 1));
    std::fs::write(crate::skirmish::saves_dir().join("Won.sav"), shc_sim::save::save_game(&mut m, 0)).unwrap();
}

/// SHC_SAVES=<dir> (screenshots): `MissionWon.sav`, the first historical campaign mission with AI lords just won (their
/// lords killed: campaign::historical decides), for the mission-over video.
#[cfg(feature = "live")]
#[test]
#[ignore]
fn write_mission_won_save() {
    use shc_sim::{frontend, game::player};
    if std::env::var_os("SHC_SAVES").is_none() { return; }
    let mut ret = vec![0u8; shc_sim::hudstate::RET_LEN];
    let mut tick = |m: &mut shc_sim::mem::Mem| { frontend::run_tick(m, &frontend::TickInput::default(), &mut ret, None); };
    for id in 1..=20 {
        let Ok(mut m) = crate::skirmish::start_mission(id, 1) else { continue };
        for _ in 0..300 { tick(&mut m); }
        let me = m.i32(player(1) + 0x21f8);
        let ais: Vec<i32> = (2..9).map(|p| m.i32(player(p) + 0x21f8)).filter(|&l| l > 0).collect();
        if me <= 0 || ais.is_empty() { continue; }
        for &l in &ais { shc_sim::units::helpers::apply_hit(&mut m, me, l, 10_000_000); }
        for _ in 0..1500 { tick(&mut m); if read(&m).is_some_and(|e| e.result == 1) { break; } }
        if !read(&m).is_some_and(|e| e.result == 1) { continue; }
        println!("mission {id} won ({} AI lords)", ais.len());
        std::fs::write(crate::skirmish::saves_dir().join("MissionWon.sav"), shc_sim::save::save_game(&mut m, 0)).unwrap();
        return;
    }
    panic!("no mission won");
}

/// SHC_SAVES=<dir> (screenshots): `TrailWon-<trail>-<id>.sav`, the last Sands of Time mission of the Trail of the Lamb
/// (11, 4) and the classic Crusader Trail's last (0, 49) just won (the AI lords killed); `--live` one with
/// `--trail <trail>,<id>` for the results / the trail's completion screen.
#[cfg(feature = "live")]
#[test]
#[ignore]
fn write_trail_won_saves() {
    use shc_sim::{frontend, game::player};
    if std::env::var_os("SHC_SAVES").is_none() { return; }
    let mut ret = vec![0u8; shc_sim::hudstate::RET_LEN];
    let mut tick = |m: &mut shc_sim::mem::Mem| { frontend::run_tick(m, &frontend::TickInput::default(), &mut ret, None); };
    for (tr, id) in [(11, 4), (0, 49)] {
        let mut m = crate::menu::trail_menu::start(tr, id, 1).unwrap();
        for _ in 0..300 { tick(&mut m); }
        let me = m.i32(player(1) + 0x21f8);
        let ais: Vec<i32> = (2..9).map(|p| m.i32(player(p) + 0x21f8)).filter(|&l| l > 0).collect();
        for l in ais { shc_sim::units::helpers::apply_hit(&mut m, me, l, 10_000_000); }
        for _ in 0..1500 { tick(&mut m); if read(&m).is_some_and(|e| e.result == 1) { break; } }
        assert!(read(&m).is_some_and(|e| e.result == 1), "trail {tr} mission {id} not won");
        std::fs::write(crate::skirmish::saves_dir().join(format!("TrailWon-{tr}-{id}.sav")), shc_sim::save::save_game(&mut m, 0)).unwrap();
    }
}

/// Kill your lord (an enemy lord's hit) in a fresh skirmish: alone vs the Rat the game ends (Defeat, "Rat ... Wins"
/// for player 2); vs Rat + Pig you are out and the game goes on (result 0, no winners yet). Ticks with plain
/// DLL_RunTick (Live::step also diffs every tile record: ~50x slower); Live only for the HudState it sends.
#[cfg(feature = "live")]
#[test]
fn lord_killed_ends_skirmish() {
    use crate::skirmish::{maps_dir, start, Lobby};
    use shc_sim::{frontend, game::player};
    let map = maps_dir().join("Crater Lake.map");
    if !map.exists() { return; } // needs the game install
    let mut ret = vec![0u8; shc_sim::hudstate::RET_LEN];
    let mut tick = |m: &mut shc_sim::mem::Mem| { frontend::run_tick(m, &frontend::TickInput::default(), &mut ret, None); };
    // the default radar colours of you, the Rat (and the Pig)
    for (ais, want) in [(vec![(0, 0)], End { result: 2, winners: vec![(2, "TEXT_XPLAY_WAITING_ROOM_112".into())], colours: [0, 1, 3, 0, 0, 0, 0, 0, 0] }),
        (vec![(0, 0), (2, 0)], End { result: 0, winners: vec![], colours: [0, 1, 3, 4, 0, 0, 0, 0, 0] })] {
        let mut m = start(&Lobby { ais, seed: 1234, ..Lobby::new(map.clone()) }).unwrap();
        for _ in 0..300 { tick(&mut m); }
        assert_eq!(read(&m), None, "still playing");
        let (me, rat) = (m.i32(player(1) + 0x21f8), m.i32(player(2) + 0x21f8));
        assert!(me > 0 && rat > 0, "lords {me} {rat}");
        shc_sim::units::helpers::apply_hit(&mut m, rat, me, 10_000_000);
        // the lord's despawn marks you out after end_check ran that tick: the result comes one tick later
        let mut t = 0;
        while m.i32(player(1) + 0x2210) == 0 && t < 1000 { tick(&mut m); t += 1; }
        tick(&mut m);
        assert_eq!(m.i32(0x1837ebe84), (want.result != 0) as i32, "end latch (game::end_check decided), {t} ticks after the hit");
        let lost = want.result == 2;
        assert_eq!(crate::live::Live::from_mem(m).hud().end, Some(want), "{t} ticks after the hit");
        // the score screen's rows from the captured statistics: the Rat (gold, your lord) the greatest lord
        if lost { assert_eq!(crate::menu::trail_menu::masters::Score::new(crate::menu::trail_menu::masters::peek().unwrap()).rows(), [2, 1]); }
    }
}
