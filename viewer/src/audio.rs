//! `--live` sound: plays what the sim queued (`shc_sim::sound`) through `shc_audio` (clips from
//! re/scripts/export_audio.py's cache, ~/Library/Caches/shc-audio), music included (the sim's music manager picks the
//! tracks; `music_state` reports playback back for DLL_RunTick).
//! Positional sounds use GameMap.processTestMap's type-16 maths (re/csharp/GameMap.cs:1388). SHC_MUTE=1 = silent.
//! Front end / HUD: `ui` (SFXManager.playUISound), `hover` (the MouseEnter rollovers), `menu` (front-end music).

/// UI sound ids (sounds.tsv rows) and where the original plays them (re/csharp/CrusaderDE).
#[allow(dead_code)] // used by the menu.rs / hud.rs hooks
pub mod ui_id {
    /// Main-menu shield rollover, FrontendMenus.PlayShieldRollover (Load / Game Options at volume 0.5).
    pub const SHIELD: i32 = 146;
    /// Main-menu exit door rollover, PlayExitRollover.
    pub const EXIT: i32 = 251;
    /// MainViewModel.CommonRedButtonEnter: the red / glow buttons of the lobby, save list, pause menu, popups.
    pub const RED_BUTTON: i32 = 134;
    /// HUD_Main build-menu button rollover (200 ms throttle); sub-menu openers (towers, military, gates, keeps,
    /// good, bad) play SUBMENU_HOVER instead.
    pub const BUILD_HOVER: i32 = 136;
    pub const SUBMENU_HOVER: i32 = 289;
    /// HUD_Main category tabs castle, industry, farms, town, weapons, food: rollover, and click (NewBuildScreen*).
    pub const TAB_HOVER: [i32; 6] = [129, 130, 131, 132, 133, 134];
    pub const TAB_CLICK: [i32; 6] = [123, 124, 127, 125, 128, 126];
    /// HUD_Main.StartScrollSwish: every build-row change (tab click, sub-menu open / return).
    pub const SWISH: i32 = 103;
    /// FRONT_Multiplayer lobby: Fairness1..5 (goldplus 8k / 4k / 2k / 4k / 8k), random AI dice, team faces.
    pub const GOLD: [i32; 5] = [256, 255, 254, 255, 256];
    pub const DICE: i32 = 252;
    pub const TABLE: i32 = 7;
    /// IngameUIScreens.triggerVideos: the end banner's sting (Victory_banner / Defeat_banner), with the banner video.
    pub const VICTORY: i32 = 320;
    pub const DEFEAT: i32 = 321;
}

#[cfg(feature = "live")]
mod imp {
    use shc_sim::sound::{drain, Sound};
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    // music: SFXManager music id we started (8 intro, 3 front end; 0 = the game's own); greet: the start-up greeting,
    // None = not yet due, Some(Some(t)) = due at t, Some(None) = said / skipped; story: the story page's line and when
    struct St { a: shc_audio::Audio, speech_ch: i32, music: i32, greet: Option<Option<Instant>>, story: Option<(Instant, String)>, later: Option<(Instant, String)> }

    /// `f` on the open audio (opened on first use); nothing when muted / no device.
    fn with(f: impl FnOnce(&mut St)) { S.with(|s| if let Some(st) = s.borrow_mut().get_or_insert_with(open) { f(st) }) }

    /// A queued front-end request, played at the next `menu` / `tick`.
    #[derive(Debug, PartialEq)]
    pub enum Req { Sound(i32, f32), Speech(i32, String), File(i32, std::path::PathBuf) }

    thread_local! {
        static S: RefCell<Option<Option<St>>> = const { RefCell::new(None) };
        static UI: RefCell<Vec<Req>> = const { RefCell::new(Vec::new()) };
        static HOVER: Cell<([f32; 4], Option<Instant>, Option<Instant>)> = const { Cell::new(([0.0; 4], None, None)) }; // rect, last seen, last rollover
        // lobby team lines: Genie_13 (Rat) / Genie_14 (Sultan) said, the next random one not before
        static TEAM: Cell<(bool, bool, Option<Instant>)> = const { Cell::new((false, false, None)) };
    }

    /// The sim's sounds drained on the sim thread (`shc_sim::sound` is per thread): live::Thread hands them over here,
    /// the next `tick` plays them.
    static FWD: Mutex<Vec<Sound>> = Mutex::new(Vec::new());
    pub fn forward(v: Vec<Sound>) {
        // a speech line counts as playing from the request on (isSpeechPlaying = speechMode != 0, set before the clip
        // loads). ponytail: both channels, the one it lands on is picked at play time; the next `tick` corrects it
        if ON.load(Relaxed) && v.iter().any(|s| matches!(s, Sound::Named(_))) { for f in &SPEECH { f.store(true, Relaxed) } }
        let mut q = FWD.lock().unwrap();
        if q.len() < 1024 { q.extend(v) } // ponytail: nobody plays while no game frame runs (menus), so cap like the sim
    }
    fn sounds() -> Vec<Sound> { [std::mem::take(&mut *FWD.lock().unwrap()), drain()].concat() }

    /// DLL_RunTick's musicPlaying / musicAboutToLoop as of the last `tick` (read on the sim thread).
    static MUSIC: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];
    pub fn music_state() -> (bool, bool) { (MUSIC[0].load(Relaxed), MUSIC[1].load(Relaxed)) }
    /// DLL_RunTick's speech1 / speech2 = C# isSpeechPlaying(1 / 2): the tutorial waits for its line, advisor lines
    /// don't talk over each other (re/SCENARIO.md). Always false while the audio is off.
    static SPEECH: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];
    static ON: AtomicBool = AtomicBool::new(false); // audio opened
    pub fn speech_state() -> (bool, bool) { (SPEECH[0].load(Relaxed), SPEECH[1].load(Relaxed)) }

    /// SFXManager.manageTutorialSpeechAndMissingSpeech with the default settings (push scrolling on, SH1 mouse wheel
    /// off): the file to play, None = none.
    pub(super) fn speech_file(name: &str) -> Option<String> {
        let l = name.to_lowercase();
        if l.contains("tutorial") {
            if l.ends_with("tutorial_31b.wav") { return None; }
            for (a, b) in [("tutorial_29.wav", "tutorial_29a.wav"), ("tutorial_31a.wav", "tutorial_31.wav")] {
                if l.ends_with(a) { return Some(l.replace(a, b)); }
            }
        } else if let Some(k) = ["de_dlc4_m5_1", "de_dlc4_m5_2", "de_dlc4_m5_3"].into_iter().find(|k| l.contains(k)) {
            return Some(l.replace(k, "de_dlc0_0"));
        }
        Some(name.to_string())
    }

    /// SFXManager.playUISound: sound `id` centred at `vol` × its volume.txt level, played at the next `menu` / `tick`.
    /// Options > UI Sounds off, or the app in the background: none (ConfigSettings.Settings_PlayUISFX && isFocused).
    pub fn ui(id: i32, vol: f32) {
        if !crate::menu::opt("PlayUISFX", true) || !crate::ux::focused() { return; }
        UI.with(|q| q.borrow_mut().push(Req::Sound(id, vol)))
    }

    /// SFXManager.playMusic(file, volume, loop): a front-end piece (the credits' music 15); the next `menu(true)`
    /// brings the front-end track back.
    pub fn music(path: &str, vol: f32) {
        with(|st| {
            st.music = -1;
            st.a.play_music(path, vol, true, false);
        })
    }

    /// SFXManager.playSpeech / playGenieSpeech: speech file `name` (bare, looked up in EnglishSpeech/**) on channel
    /// 1..3, played at the next `menu` / `tick`. Menu uses: "General_Quitgame.wav" (quit mission), "General_Saving.wav"
    /// / "General_Loading.wav" (HUD_LoadSaveRequester), lobby "Genie_01.wav" (Teams opens, ch 3), "Genie_02.wav"
    /// (Teams closes with an AI in the lobby, ch 3), "Genie_03.wav" (Add AI opens, ch 2).
    pub fn speech(ch: i32, name: &str) { UI.with(|q| q.borrow_mut().push(Req::Speech(ch, name.to_string()))) }

    /// Speech channel `ch` busy (nothing when the audio is not open yet).
    fn talking(ch: i32) -> bool { S.with(|s| s.borrow().as_ref().and_then(|o| o.as_ref()).is_some_and(|st| st.a.speech_playing(ch))) }

    /// FRONT_Multiplayer AddPlayerSpeech / KickPlayerSpeech prefixes, index = lord (skirmish::LORDS order) + 1.
    const LORD_TAGS: [&str; 28] = ["all", "rt", "sn", "pg", "wf", "sa", "ca", "su", "ri", "fr", "ph", "wa", "em", "ni", "sh", "ma",
        "ab", "je", "se", "no", "ka", "cn", "tr", "sg", "li", "cr", "ba", "bu"];
    fn lord_line(lord: usize, what: &str) -> Option<String> {
        let t = LORD_TAGS.get(lord + 1)?;
        // the four Crusader originals have no _01 suffix
        Some(if lord < 4 { format!("{t}_{what}_player.wav") } else { format!("{t}_{what}_player_01.wav") })
    }
    /// Lobby: AI `lord` added (FRONT_Multiplayer:8150), unless channel 3 is talking.
    pub fn genie_add(lord: usize) {
        if let Some(c) = crate::custom_lords::of_lord(lord) { if let Some(p) = crate::custom_lords::lobby_wav(&c, "ADD_PLAYER").filter(|_| !talking(3)) { UI.with(|q| q.borrow_mut().push(Req::File(3, p))) } return; }
        if let Some(l) = lord_line(lord, "add").filter(|_| !talking(3)) { speech(3, &l) }
    }
    /// Lobby: AI `lord` removed (FRONT_Multiplayer:3467), unless channel 3 is talking.
    pub fn genie_kick(lord: usize) {
        if let Some(c) = crate::custom_lords::of_lord(lord) { if let Some(p) = crate::custom_lords::lobby_wav(&c, "KICK_PLAYER").filter(|_| !talking(3)) { UI.with(|q| q.borrow_mut().push(Req::File(3, p))) } return; }
        if let Some(l) = lord_line(lord, "kick").filter(|_| !talking(3)) { speech(3, &l) }
    }
    /// Lobby Teams: AI `lord` joined a team, `with_you` = yours (FRONT_Multiplayer:3655-3715). Rat / Sultan joining you
    /// say their line once; otherwise one of seven lines, at most every 5 s and not over channel 3.
    pub fn genie_team(lord: usize, with_you: bool) {
        let (mut rat, mut sultan, next) = TEAM.get();
        let first = match lord {
            0 if with_you && !rat => { rat = true; Some("Genie_13.wav") }
            6 if with_you && !sultan => { sultan = true; Some("Genie_14.wav") }
            _ => None,
        };
        let now = Instant::now();
        let mut next = next;
        let line = first.map(str::to_string).or_else(|| (!talking(3) && next.map_or(true, |t| now > t)).then(|| {
            next = Some(now + Duration::from_secs(5));
            let k = now.duration_since(START.with(|s| *s)).subsec_nanos() % 7;
            format!("genie_{:02}.wav", if k == 6 { 4 } else { k + 5 })
        }));
        TEAM.set((rat, sultan, next));
        if let Some(l) = line { speech(3, &l) }
    }
    thread_local! { static START: Instant = Instant::now(); }

    /// MouseEnter rollover: call every frame the cursor is on button rect `at`. Plays `id` when the cursor enters
    /// (a different rect, or none for 250 ms), at most every 200 ms (FrontendMenus.lastRolloverSoundTime).
    /// ponytail: the 200 ms throttle is shared by all buttons; the original only has it on the shields and build buttons.
    pub fn hover(at: [f32; 4], id: i32, vol: f32) {
        let now = Instant::now();
        let ago = |t: Option<Instant>, ms: u64| t.map_or(true, |t| now - t > Duration::from_millis(ms));
        let (rect, seen, played) = HOVER.get();
        let play = (rect != at || ago(seen, 250)) && ago(played, 200);
        HOVER.set((at, Some(now), if play { Some(now) } else { played }));
        if play { ui(id, vol) }
    }

    /// Plays (muted: drops) the queued front-end requests.
    fn flush(st: Option<&mut St>) {
        let q = UI.take();
        let Some(st) = st else { return };
        for r in q {
            match r {
                Req::Sound(id, v) => st.a.play_sound(id, v, 0.0),
                Req::Speech(ch, n) => st.a.play_speech(ch, &n, 1.0),
                Req::File(ch, p) => st.a.play_speech_file(ch, &p, 1.0),
            }
        }
    }

    /// Once per frame while the menu shows. `front_end` = main menu, save list, lobby (not the in-game Escape menu):
    /// FatControler.changeScene(FrontEnd) → playMusic(3) "A Strong Spice", looped, volume 1 (no volume.txt entry), and
    /// once per run playIntroSpeech's greeting 0.5 s after the front end first shows (delayPlaySpeech, music not ducked).
    /// ponytail: no date / player-name greetings.
    pub fn menu(front_end: bool) {
        S.with(|s| {
            let mut s = s.borrow_mut();
            let st = s.get_or_insert_with(open).as_mut();
            let Some(st) = st else { return flush(None) };
            if front_end && st.music != 3 {
                st.music = 3;
                st.a.play_music("fx\\music\\astrongspice.raw", 1.0, true, false);
            }
            let now = Instant::now();
            match st.greet {
                None if front_end => st.greet = Some(Some(now + Duration::from_millis(500))),
                Some(Some(t)) if front_end && now >= t => {
                    st.greet = Some(None);
                    st.a.play_speech(1, "general_startgame.wav", 1.0);
                }
                _ => {}
            }
            if let Some((_, line)) = st.story.take_if(|(t, _)| now >= *t) { st.a.play_speech(3, &line, 1.0) }
            flush(Some(st));
        })
    }

    /// FatControler.Start: playMusic(8) "A Pane in the Glass", looped, from app start under the logos and the intro
    /// video, until `menu(true)` replaces it.
    pub fn startup() {
        with(|st| {
            st.music = 8;
            st.a.play_music("fx\\music\\apaneintheglass.raw", 1.0, true, false);
        })
    }

    /// GameData.ManageGameOver's sounds, as the score screen / mission-over video comes: the advisor on speech channel 1
    /// (won: general_victory1 two times in three, else general_victory2; lost: general_warning2), the win / lose tune
    /// once (PlayWinTune bigwin1..3, PlayLoseTune bigloss1..2, in turn), and on a skirmish / LAN map (multiplayerMap)
    /// setFutureVoiceLine's MP_Victory_1..3 / MP_Defeat_1..6, 2 s on and once channel 1 is quiet (HUD_MissionOver.Update).
    pub fn game_over(won: bool, mp: bool) {
        thread_local! { static TUNES: Cell<(usize, usize)> = const { Cell::new((0, 0)) }; } // last_win_tune, last_lose_tune
        let r = Instant::now().duration_since(START.with(|s| *s)).subsec_nanos() as usize;
        let (win, lose) = TUNES.get();
        let (line, tune) = if won {
            (if r % 3 < 2 { "general_victory1.wav" } else { "general_victory2.wav" }, format!("fx\\music\\bigwin{}.raw", win % 3 + 1))
        } else {
            ("general_warning2.wav", format!("fx\\music\\bigloss{}.raw", lose % 2 + 1))
        };
        TUNES.set(if won { (win + 1, lose) } else { (win, lose + 1) });
        let later = mp.then(|| if won { format!("MP_Victory_{}.wav", r / 7 % 3 + 1) } else { format!("MP_Defeat_{}.wav", r / 7 % 6 + 1) });
        eprintln!("audio: game over: speech {line}, music {tune}{}", later.as_ref().map_or(String::new(), |l| format!(", then {l}")));
        with(|st| {
            st.a.play_speech(1, line, 1.0);
            st.a.play_music(&tune, 1.0, false, false); // st.music stays 0: `tick` leaves it alone
            st.later = later.map(|l| (Instant::now() + Duration::from_secs(2), l));
        })
    }

    /// video.rs: a video's soundtrack (IntroSequence.StartVideo: speech slider × master); None stops it.
    pub fn video(wav: Option<&std::path::Path>) { with(|st| st.a.play_video(wav, 1.0)) }

    /// FRONT_Story page. `music` = engine path ("fx\\music\\crusader.raw"): playMusic(ID, fadePrevious: false, 0.4),
    /// i.e. game volume 0.4 × its volume.txt level, looped, restarted on every call; None leaves the music alone.
    /// `speech` ("Campaign\\M1_Hist.wav") on speech channel 3 after 0.5 s (delayPlaySpeech, music not ducked);
    /// None = StopSpeech(3). Keep calling `menu(false)` while the page shows (it plays the delayed line); the next
    /// `menu(true)` brings the front-end track back.
    pub fn story(music: Option<&str>, speech: Option<&str>) {
        with(|st| {
            if let Some(m) = music {
                st.music = -1;
                st.a.play_music(m, 0.4, true, false);
            }
            st.story = speech.map(|l| (Instant::now() + Duration::from_millis(500), l.to_string()));
            if speech.is_none() { st.a.stop_speech(3) }
        })
    }

    /// Options sliders, 0..1 (ConfigSettings defaults: master 0.8, the rest 1, reduce on); mixed as
    /// `shc_audio::Volumes` says. Applies to what is playing too.
    pub fn set_volumes(master: f32, music: f32, speech: f32, unit_speech: f32, sfx: f32, reduce_music_for_speech: bool) {
        let c = |v: f32| v.clamp(0.0, 1.0);
        let v = shc_audio::Volumes { master: c(master), music: c(music), speech: c(speech), unit_speech: c(unit_speech), sfx: c(sfx), reduce_music_for_speech };
        with(|st| st.a.set_volumes(v))
    }

    #[cfg(test)]
    pub(super) fn queued() -> Vec<Req> { UI.take() }

    fn open() -> Option<St> {
        if std::env::var_os("SHC_MUTE").is_some() { return None; }
        let dir = std::env::var_os("SHC_AUDIO").map(std::path::PathBuf::from) // the .app's bundled copy
            .unwrap_or_else(|| crate::paths::cache("shc-audio"));
        if !dir.join("sounds.tsv").exists() {
            eprintln!("audio: no {} (run re/scripts/export_audio.py), sound off", dir.display());
            return None;
        }
        // shc_audio panics without an output device
        let st = std::panic::catch_unwind(|| St { a: shc_audio::Audio::new(&dir), speech_ch: 1, music: 0, greet: None, story: None, later: None }).map_err(|_| eprintln!("audio: no output device")).ok();
        ON.store(st.is_some(), Relaxed);
        st
    }

    /// GameMap type-16 positional sound: screen point `p` (logical px, y down) in a `w`×`h` window → (volume, pan),
    /// or None when off screen (100 px / 75 px margin).
    pub(super) fn place(p: [f32; 2], w: f32, h: f32) -> Option<(f32, f32)> {
        let (sw, sh, x, y) = (w + 200.0, h + 150.0, p[0] + 100.0, p[1] + 75.0);
        if !(x >= 0.0 && y >= 0.0 && x < sw && y < sh) { return None; }
        let px = x / (sw / 2.0) - 1.0;
        let edge = (1.0 - px.abs()) * 0.66 + 0.33;
        let pan = if px < 0.0 { px * 0.66 - 0.33 } else if px > 0.0 { px * 0.66 + 0.33 } else { 0.0 };
        let vy = y / (sh / 2.0) - 1.0;
        Some(((1.0 - vy.abs()) / 0.66 + 0.33) * edge).map(|v| (v, pan))
    }

    /// Once per frame: play the sim's queued sounds; `to_screen(x, y)` = engine tile → window px.
    pub fn tick(to_screen: impl Fn(i32, i32) -> [f32; 2], w: f32, h: f32) {
        S.with(|s| {
            let mut s = s.borrow_mut();
            let Some(st) = s.get_or_insert_with(open) else { sounds(); return flush(None) };
            flush(Some(st));
            st.greet = Some(None); // ponytail: no greeting once a game ran (--live start, or Start within 0.5 s)
            if st.music != 0 {
                // the game's tick-0 music stop (re/AUDIO.md §2, type 19 mode 10 → stopMusic): a hard cut, no fade; the
                // skirmish track below starts once the channel is idle
                st.music = 0;
                st.a.stop_music();
            }
            for snd in sounds() {
                match snd {
                    Sound::At { x, y, id } => if let Some((v, pan)) = place(to_screen(x, y), w, h) { st.a.play_sound(id, v.min(1.0), pan) }, // ponytail: clamp, Unity's formula reaches 1.85 (rodio would clip)
                    Sound::Id(id) => st.a.play_sound(id, 1.0, 0.0),
                    Sound::Named(n) => {
                        // a custom lord's line "*<player>*SK_<KEY>": its CustomMedia wav (none: the greeting already played)
                        if let Some((p, key)) = crate::custom_lords::split(&n) {
                            if let Some(path) = crate::custom_lords::wav(p, &key) { st.speech_ch = 3 - st.speech_ch; st.a.play_speech_file(st.speech_ch, &path, 1.0); }
                            continue;
                        }
                        let Some(n) = speech_file(&n) else { continue };
                        // tutorial lines: FUN_18000f070 = engine channel 4 = C# channel 2, cutting the line before
                        // (PlaySpeech force); the rest: FUN_18000edf0 alternates channels 1 / 2 (DAT_1802c5728 3, 4)
                        let ch = if n.to_lowercase().starts_with("tutorial") { 2 } else { st.speech_ch = 3 - st.speech_ch; st.speech_ch };
                        st.a.play_speech(ch, &n, 1.0)
                    }
                    Sound::Ambient { ch, id, mode, vol } => {
                        let v = vol as f32 / 100.0;
                        match mode {
                            0 | 1 => st.a.play_ambient(ch, id, v, mode == 1),
                            5 => st.a.set_ambient_volume(ch, v),
                            10 => st.a.stop_ambient(ch),
                            _ => {}
                        }
                    }
                    Sound::Music { name, vol, mode, looped } => match mode {
                        0 | 1 => st.a.play_music(&name, vol as f32 / 100.0, looped, mode == 1),
                        5 => st.a.set_music_volume(vol as f32 / 100.0),
                        10 => st.a.stop_music(),
                        _ => {}
                    },
                }
            }
            // game_over's later line: not before its time, and 1 s after channel 1 last talked
            if let Some((t, _)) = &mut st.later { if st.a.speech_playing(1) { *t = (*t).max(Instant::now() + Duration::from_secs(1)) } }
            if let Some((_, l)) = st.later.take_if(|(t, _)| Instant::now() >= *t) { st.a.play_speech(1, &l, 1.0) }
            st.a.duck_for_speech();
            let (playing, about) = st.a.music_update();
            MUSIC[0].store(playing, Relaxed);
            MUSIC[1].store(about, Relaxed);
            let pending = FWD.lock().unwrap().iter().any(|s| matches!(s, Sound::Named(_))); // forwarded since `sounds()`
            for (k, f) in SPEECH.iter().enumerate() { f.store(pending || st.a.speech_playing(k as i32 + 1), Relaxed) }
        })
    }
}

#[cfg(feature = "live")]
pub use imp::{forward, game_over, genie_add, genie_kick, genie_team, hover, menu, music, music_state, set_volumes, speech, speech_state, startup, story, tick, ui, video};
#[cfg(not(feature = "live"))]
pub fn tick(_: impl Fn(i32, i32) -> [f32; 2], _: f32, _: f32) {}
#[cfg(not(feature = "live"))]
pub fn ui(_: i32, _: f32) {}
#[cfg(not(feature = "live"))]
pub fn music_state() -> (bool, bool) { (false, false) }
#[cfg(not(feature = "live"))]
pub fn speech_state() -> (bool, bool) { (false, false) }
#[cfg(not(feature = "live"))]
pub fn hover(_: [f32; 4], _: i32, _: f32) {}
#[cfg(not(feature = "live"))]
pub fn menu(_: bool) {}
#[cfg(not(feature = "live"))]
pub fn startup() {}
#[cfg(not(feature = "live"))]
pub fn video(_: Option<&std::path::Path>) {}
#[cfg(not(feature = "live"))]
pub fn story(_: Option<&str>, _: Option<&str>) {}
#[cfg(not(feature = "live"))]
pub fn set_volumes(_: f32, _: f32, _: f32, _: f32, _: f32, _: bool) {}
#[cfg(not(feature = "live"))]
pub fn speech(_: i32, _: &str) {}
#[cfg(not(feature = "live"))]
pub fn genie_add(_: usize) {}
#[cfg(not(feature = "live"))]
pub fn genie_kick(_: usize) {}
#[cfg(not(feature = "live"))]
pub fn genie_team(_: usize, _: bool) {}
#[cfg(not(feature = "live"))]
pub fn music(_: &str, _: f32) {}
#[cfg(not(feature = "live"))]
pub fn game_over(_: bool, _: bool) {}

#[cfg(all(test, feature = "live"))]
mod tests {
    #[test]
    fn place() {
        let (w, h) = (1000.0, 600.0);
        let (v, pan) = super::imp::place([500.0, 300.0], w, h).unwrap(); // centre: full volume, centred
        assert!(pan.abs() < 1e-3 && (v - (1.0 / 0.66 + 0.33) * 0.99).abs() < 1e-3, "{v} {pan}");
        assert!(super::imp::place([-150.0, 300.0], w, h).is_none());
        assert!(super::imp::place([50.0, 300.0], w, h).unwrap().1 < -0.33); // left edge pans left
    }

    /// UI sounds queue the original's ids; a rollover plays once per entry, 200 ms apart; SHC_MUTE drops them.
    #[test]
    fn ui_sounds() {
        use super::{hover, imp::{queued, Req::*}, ui, ui_id::*};
        let (a, b) = ([0.0, 0.0, 10.0, 10.0], [10.0, 0.0, 20.0, 10.0]);
        hover(a, SHIELD, 1.0);
        hover(a, SHIELD, 1.0); // still on it
        hover(b, EXIT, 1.0); // entered within 200 ms: throttled
        std::thread::sleep(std::time::Duration::from_millis(210));
        hover(a, SHIELD, 0.5); // back on the Load shield
        ui(TAB_CLICK[2], 1.0); // farms tab: metpush13 + swish
        ui(SWISH, 1.0);
        assert_eq!(queued(), [Sound(146, 1.0), Sound(146, 0.5), Sound(127, 1.0), Sound(103, 1.0)]);
        // the clips behind them (export_audio.py's sounds.tsv), when exported
        let tsv = std::fs::read_to_string(crate::paths::cache("shc-audio").join("sounds.tsv")).unwrap_or_default();
        let clip = |id: i32| tsv.lines().find_map(|l| l.strip_prefix(&format!("{id}\t"))).map(str::to_string);
        if !tsv.is_empty() {
            assert_eq!(clip(SHIELD).as_deref(), Some("fx\\shieldrollover,fx\\shieldrollover"));
            assert_eq!(clip(EXIT).as_deref(), Some("fx\\exitrollover"));
            assert_eq!(clip(RED_BUTTON).as_deref(), Some("fx\\metrollover12"));
            assert_eq!(clip(SWISH).as_deref(), Some("fx\\menusl_1,fx\\menusl_2,fx\\menusl_3"));
            assert_eq!(clip(TAB_CLICK[0]).as_deref(), Some("fx\\metpush7"));
        }
        std::env::set_var("SHC_MUTE", "1"); // ponytail: process-wide; nothing else in the tests opens audio
        ui(EXIT, 1.0);
        super::menu(true);
        assert!(queued().is_empty());
    }

    /// Menu speech: lobby advisor lines by lord (FRONT_Multiplayer tables), the team lines once / throttled, all files
    /// in the export.
    #[test]
    fn menu_speech() {
        use super::{genie_add, genie_kick, genie_team, imp::{queued, Req::*}, speech};
        let sp = |ch: i32, n: &str| Speech(ch, n.to_string());
        genie_add(0); // Rat
        genie_add(6); // Sultan
        genie_kick(26); // Bullseye
        genie_add(99); // no such lord: nothing
        speech(1, "General_Saving.wav");
        assert_eq!(queued(), [sp(3, "rt_add_player.wav"), sp(3, "su_add_player_01.wav"), sp(3, "bu_kick_player_01.wav"), sp(1, "General_Saving.wav")]);
        genie_team(0, true); // the Rat joins you: Genie_13 once
        genie_team(0, true); // then a random line
        genie_team(3, false); // within 5 s: nothing
        let q = queued();
        assert!(q.len() == 2 && q[0] == sp(3, "Genie_13.wav") && matches!(&q[1], Speech(3, n) if n.starts_with("genie_")), "{q:?}");
        // every line names a file of the speech export (EnglishSpeech, basenames unique), when it is there
        let mut have = std::collections::HashSet::new();
        fn walk(d: &std::path::Path, have: &mut std::collections::HashSet<String>) {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() { walk(&p, have) } else { have.insert(e.file_name().to_string_lossy().to_lowercase()); }
            }
        }
        walk(&crate::paths::cache("shc-audio").join("speech"), &mut have);
        if have.is_empty() { return; }
        let mut lines: Vec<String> = ["General_Saving.wav", "General_Loading.wav", "General_Quitgame.wav", "Genie_01.wav", "Genie_02.wav",
            "Genie_03.wav", "Genie_13.wav", "Genie_14.wav"].map(String::from).into();
        lines.extend((4..=10).map(|k| format!("genie_{k:02}.wav")));
        for lord in 0..crate::skirmish::LORDS.len() {
            genie_add(lord);
            genie_kick(lord);
        }
        lines.extend(queued().into_iter().filter_map(|r| if let Speech(_, n) = r { Some(n) } else { None }));
        let missing: Vec<_> = lines.iter().filter(|n| !have.contains(&n.to_lowercase())).collect();
        assert!(missing.is_empty() && lines.len() == 15 + 2 * crate::skirmish::LORDS.len(), "missing {missing:?}");
    }

    /// ManageGameOver's files (audio::game_over) are in the export: the advisor's lines, the MP lines, the tunes.
    #[test]
    fn game_over_files() {
        let dir = crate::paths::cache("shc-audio");
        let dir = std::env::var_os("SHC_AUDIO").map(std::path::PathBuf::from).unwrap_or(dir);
        if !dir.join("speech/Scribe").is_dir() { return; }
        let mut need: Vec<String> = ["General_Victory1", "General_Victory2", "General_Warning2"].map(|n| format!("speech/Scribe/{n}.wav")).into();
        need.extend((1..=3).map(|k| format!("speech/Scribe/MP_Victory_{k}.wav")));
        need.extend((1..=6).map(|k| format!("speech/Scribe/MP_Defeat_{k}.wav")));
        need.extend(["bigwin1", "bigwin2", "bigwin3", "bigloss1", "bigloss2"].map(|n| format!("music/{n}.wav")));
        let missing: Vec<_> = need.iter().filter(|f| !dir.join(f).exists()).collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    /// Tutorial / campaign speech names as SFXManager.manageTutorialSpeechAndMissingSpeech maps them (default settings).
    #[test]
    fn speech_files() {
        use super::imp::speech_file;
        assert_eq!(speech_file("Tutorial_29.wav").as_deref(), Some("tutorial_29a.wav")); // SH1 mouse wheel off
        assert_eq!(speech_file("Tutorial_31a.wav").as_deref(), Some("tutorial_31.wav"));
        assert_eq!(speech_file("Tutorial_31b.wav"), None);
        assert_eq!(speech_file("Tutorial_2.wav").as_deref(), Some("Tutorial_2.wav")); // push scrolling on
        assert_eq!(speech_file("de_dlc4_m5_2_intro.wav").as_deref(), Some("de_dlc0_0_intro.wav"));
        assert_eq!(speech_file("Genie_35.wav").as_deref(), Some("Genie_35.wav"));
        assert_eq!(super::speech_state(), (false, false)); // audio never opened here: never busy
    }

    /// Mission sounds that campaigncheck GAPS lists as unported (0x180002940 sound id, 0x1800039a0 named sound,
    /// 0x1800041c0 ambient): `units::ext` queues them for the viewer (`sound::note`) before that hook, so missions play
    /// them. Mission 2's opening 200 ticks: a named warning (t71) and the ambient loops (from t133); its battle horn
    /// Id(291) only comes at t19762, so that path is checked on `ext` directly.
    #[test]
    fn mission_sounds() {
        use shc_sim::sound::Sound;
        shc_sim::sound::drain();
        shc_sim::units::ext(&mut shc_sim::mem::Mem::default(), 0x180002940, &[0, 291]);
        assert_eq!(shc_sim::sound::drain(), [Sound::Id(291)]);
        let Ok(m) = crate::skirmish::start_mission(2, 1) else { return };
        let mut l = crate::live::Live::from_mem(m);
        shc_sim::sound::drain();
        let got: Vec<Sound> = (0..200).flat_map(|_| { l.step(); shc_sim::sound::drain() }).collect();
        assert!(got.iter().any(|s| matches!(s, Sound::Named(_))), "no named sound");
        assert!(got.iter().any(|s| matches!(s, Sound::Ambient { .. })), "no ambient");
    }

    /// The live sim feeds the queue (needs the user's Crater Lake save, like live::live_runs_crater_lake).
    #[test]
    fn live_queues_sounds() {
        let p = crate::paths::test_save("Crater Lake-1test.sav");
        if !std::path::Path::new(&p).exists() { return; }
        let mut l = crate::live::Live::load(&p);
        l.centre = [408, 123, 54, 140]; // the recording's camera (DLL_RunTick centre tile, tiles wide / high)
        shc_sim::sound::drain();
        let mut got = vec![];
        for _ in 0..400 { l.step(); got.extend(shc_sim::sound::drain()); }
        use shc_sim::sound::Sound;
        let sfx = got.iter().filter(|s| matches!(s, Sound::At { .. } | Sound::Id(_))).count();
        let other: Vec<_> = got.iter().filter(|s| !matches!(s, Sound::At { .. } | Sound::Id(_))).collect();
        // recording rec_1792268794 over the same ticks (1064..1464): 26 type-16, 2 speech (player clicks), no ambient
        println!("{} sounds in 400 ticks: {sfx} sfx, first {:?}; speech / ambient {other:?}", got.len(), &got[..got.len().min(8)]);
        assert!((10..=60).contains(&sfx), "{sfx} sfx");
        // the recording's opening camera (573, 537): the stream loop fades in on ambient channel 2 (ticks 1..44 there)
        l.centre = [573, 537, 54, 140];
        let amb: Vec<_> = (0..60).flat_map(|_| { l.step(); shc_sim::sound::drain() }).filter(|s| matches!(s, Sound::Ambient { .. })).collect();
        println!("ambient at (573, 537): {amb:?}");
        assert!(amb.contains(&Sound::Ambient { ch: 2, id: 5, mode: 1, vol: 1 }), "no stream loop");
    }
}
