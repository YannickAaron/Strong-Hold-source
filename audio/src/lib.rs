//! Native playback of SHC DE sounds, driven by the engine's draw-list records (re/AUDIO.md).
//! Clips come from `re/scripts/export_audio.py`'s cache dir.
use rodio::{source::ChannelVolume, Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const SPEECH_NAME_OFF: usize = 1536; // PlayStateReturnData.speechFileName[128]
pub const MUSIC_NAME_OFF: usize = 1664; // .musicFileName[128]
pub const BINK_NAME_OFF: usize = 1796; // .binkFileName[128]

/// The Options sliders (ConfigSettings, 0..1) as MyAudioManager mixes them: SFX and ambient × sfx × master, speech ×
/// speech (unit_speech for EnglishSpeech/troops lines) × master, music × music × master (× 0.3 while speech plays
/// when reduce_music_for_speech). Default = the game's defaults.
/// ponytail: no GetMasterVolume 0 while the window is unfocused (Settings_BackgroundAudio).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volumes { pub master: f32, pub music: f32, pub speech: f32, pub unit_speech: f32, pub sfx: f32, pub reduce_music_for_speech: bool }
impl Default for Volumes {
    fn default() -> Self { Volumes { master: 0.8, music: 1.0, speech: 1.0, unit_speech: 1.0, sfx: 1.0, reduce_music_for_speech: true } }
}

pub struct Audio {
    _stream: OutputStream,
    out: OutputStreamHandle,
    dir: PathBuf,
    sounds: Vec<Vec<String>>,  // sound id -> variant resource paths ("fx\chop1 22k")
    ambient: Vec<Vec<String>>, // ambient id -> variants
    volume: HashMap<String, f32>, // lower-case resource path -> 0..1 (fx/volume TextAsset)
    speech: HashMap<String, PathBuf>, // lower-case basename -> wav path
    files: HashMap<PathBuf, Arc<[u8]>>,
    next_variant: HashMap<usize, usize>,
    throttle: HashMap<usize, Instant>,
    ambient_sinks: [Sink; 2],
    speech_sinks: [Sink; 3],
    music: Sink,
    music_vol: (f32, f32), // (game volume, clip volume)
    // MyAudioManager: the follow-on piece (name, game volume, loop), started when the playing one ends; the playing
    // piece loops while `music_keep`; its length and start; musicAboutToLoop (latched in its last second)
    music_next: Option<(String, f32, bool)>,
    music_keep: Arc<AtomicBool>,
    music_len: Option<(Duration, Instant)>,
    music_about: bool,
    video: Sink,
    vols: Volumes,
    // per channel, before the sliders: ambient volume, speech (volume, is unit speech), video volume; music ducked
    ambient_vol: [f32; 2],
    speech_vol: [(f32, bool); 3],
    video_vol: f32,
    ducked: bool,
}

fn tsv(path: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(path).unwrap_or_default().lines().map(|l| {
        let v = l.split('\t').nth(1).unwrap_or("");
        if v.is_empty() { vec![] } else { v.split(',').map(str::to_string).collect() }
    }).collect()
}

/// SFXManager.init: "---Group N -- volume = <offset>" headers, then `"name.wav"  <0..127>` lines.
fn parse_volume(text: &str) -> HashMap<String, f32> {
    let (mut off, mut m) = (0i32, HashMap::new());
    for l in text.lines() {
        if l.starts_with("---") {
            off = l.split('=').nth(1).map(|s| s.replace(['-', ' '], "")).and_then(|s| s.trim_start_matches('+').parse().ok()).unwrap_or(0);
            if l.split('=').nth(1).map_or(false, |s| s.trim_start().starts_with('-')) { off = -off; }
        } else if let Some((name, rest)) = l.trim_start_matches('"').split_once('"') {
            if let Ok(v) = rest.trim().parse::<i32>() {
                m.insert(name.to_lowercase().replace(".wav", ""), ((v + off) as f32 / 127.0).clamp(0.0, 1.0));
            }
        }
    }
    m
}

fn walk(dir: &Path, m: &mut HashMap<String, PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() { walk(&p, m) } else if let Some(n) = p.file_name() { m.insert(n.to_string_lossy().to_lowercase(), p); }
    }
}

/// A music piece that repeats while `keep` is set (Unity AudioSource.loop, which a follow-on piece turns off).
struct Looping<S: Source> where S::Item: rodio::Sample { first: rodio::source::Buffered<S>, cur: rodio::source::Buffered<S>, keep: Arc<AtomicBool> }
impl<S: Source> Iterator for Looping<S> where S::Item: rodio::Sample {
    type Item = S::Item;
    fn next(&mut self) -> Option<S::Item> {
        self.cur.next().or_else(|| {
            if !self.keep.load(Relaxed) { return None }
            self.cur = self.first.clone();
            self.cur.next()
        })
    }
}
impl<S: Source> Source for Looping<S> where S::Item: rodio::Sample {
    fn current_frame_len(&self) -> Option<usize> {
        match self.cur.current_frame_len() { Some(0) if self.keep.load(Relaxed) => self.first.current_frame_len(), n => n }
    }
    fn channels(&self) -> u16 { self.cur.channels() }
    fn sample_rate(&self) -> u32 { self.cur.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { None }
}

fn clip_file(res: &str) -> String { res.rsplit('\\').next().unwrap_or(res).to_string() }

impl Audio {
    pub fn new(cache_dir: &Path) -> Self {
        let (_stream, out) = OutputStream::try_default().expect("audio output");
        let sink = || Sink::try_new(&out).unwrap();
        let mut speech = HashMap::new();
        walk(&cache_dir.join("speech"), &mut speech);
        Audio {
            sounds: tsv(&cache_dir.join("sounds.tsv")),
            ambient: tsv(&cache_dir.join("ambient.tsv")),
            volume: parse_volume(&std::fs::read_to_string(cache_dir.join("volume.txt")).unwrap_or_default()),
            speech,
            files: HashMap::new(),
            next_variant: HashMap::new(),
            throttle: HashMap::new(),
            ambient_sinks: [sink(), sink()],
            speech_sinks: [sink(), sink(), sink()],
            music: sink(),
            video: sink(),
            vols: Volumes::default(),
            ambient_vol: [1.0; 2],
            speech_vol: [(1.0, false); 3],
            video_vol: 1.0,
            ducked: false,
            music_vol: (1.0, 1.0),
            music_next: None,
            music_keep: Arc::new(AtomicBool::new(false)),
            music_len: None,
            music_about: false,
            dir: cache_dir.to_path_buf(),
            _stream,
            out,
        }
    }

    fn decode(&mut self, path: &Path) -> Option<Decoder<Cursor<Arc<[u8]>>>> {
        let bytes = match self.files.get(path) {
            Some(b) => b.clone(),
            None => {
                let b: Arc<[u8]> = std::fs::read(path).map_err(|e| eprintln!("audio: {}: {e}", path.display())).ok()?.into();
                self.files.insert(path.to_path_buf(), b.clone());
                b
            }
        };
        Decoder::new_wav(Cursor::new(bytes)).ok()
    }

    fn clip_volume(&self, res: &str) -> f32 { self.volume.get(&res.to_lowercase()).copied().unwrap_or(1.0) }

    /// One-shot: `clip` is the clip name (`chop1 22k`) or a resource path (`fx\chop1 22k`); pan −1..1.
    pub fn play(&mut self, clip: &str, volume: f32, pan: f32) {
        let v = volume * self.clip_volume(clip) * self.vols.sfx * self.vols.master;
        if v <= 0.0 { return; }
        let Some(src) = self.decode(&self.dir.join("fx").join(clip_file(clip) + ".wav")) else { return };
        let (l, r) = ((1.0 - pan).min(1.0) * v, (1.0 + pan).min(1.0) * v);
        let _ = self.out.play_raw(ChannelVolume::new(src.convert_samples::<f32>(), vec![l, r]));
    }

    /// SFXManager.playSound: id (10000+ = same id), variant rotation, 3-10 s throttle on a few ids.
    pub fn play_sound(&mut self, mut id: i32, volume: f32, pan: f32) {
        if id > 10000 { id -= 10000; }
        let id = id as usize;
        let Some(variants) = self.sounds.get(id).filter(|v| !v.is_empty()) else { return };
        if matches!(id, 76 | 12 | 307 | 144) { // moo, inn, dancing bear, maypole
            // ponytail: fixed 5 s instead of Random.Range(3,10)
            if self.throttle.get(&id).map_or(false, |t| t.elapsed().as_secs_f32() < 5.0) { return; }
            self.throttle.insert(id, Instant::now());
        }
        let n = variants.len();
        let i = if (304..=306).contains(&id) {
            (Instant::now().elapsed().subsec_nanos() as usize) % n // isSFXRandomVariant
        } else {
            let c = self.next_variant.entry(id).or_insert(0);
            let i = *c % n; *c = (i + 1) % n; i
        };
        let res = variants[i].clone();
        self.play(&res, volume, pan);
    }

    /// Ambient channel 1/2 (wind, ocean, fire …); `loop_` plays until stop_ambient.
    pub fn play_ambient(&mut self, channel: i32, id: i32, volume: f32, loop_: bool) {
        let Some(variants) = self.ambient.get(id.max(0) as usize).filter(|v| !v.is_empty()) else { return };
        let n = variants.len();
        let c = self.next_variant.entry(1000 + id as usize).or_insert(0);
        let res = variants[*c % n].clone(); *c = (*c + 1) % n;
        let v = volume * self.clip_volume(&res);
        let Some(src) = self.decode(&self.dir.join("fx").join(clip_file(&res) + ".wav")) else { return };
        let ch = channel.clamp(1, 2) as usize - 1;
        self.ambient_vol[ch] = v;
        let sink = &self.ambient_sinks[ch];
        sink.stop();
        sink.set_volume(v * self.vols.sfx * self.vols.master);
        if loop_ { sink.append(src.repeat_infinite()) } else { sink.append(src) }
        sink.play();
    }
    pub fn stop_ambient(&mut self, channel: i32) { self.ambient_sinks[channel.clamp(1, 2) as usize - 1].stop(); }
    pub fn set_ambient_volume(&mut self, channel: i32, volume: f32) {
        let ch = channel.clamp(1, 2) as usize - 1;
        self.ambient_vol[ch] = volume;
        self.ambient_sinks[ch].set_volume(volume * self.vols.sfx * self.vols.master);
    }

    /// `name` as the engine gives it (`fx\speech\abow_s3.wav`) or a bare file name; looked up in EnglishSpeech/**.
    pub fn play_speech(&mut self, channel: i32, name: &str, volume: f32) {
        let base = clip_file(name).to_lowercase();
        if base.contains("null.") { return; }
        let Some(path) = self.speech.get(&base).cloned() else { eprintln!("audio: no speech {base}"); return };
        let Some(src) = self.decode(&path) else { return };
        let ch = channel.clamp(1, 3) as usize - 1;
        // SFXManager.playSpeech: lines from the "troops" folder are unit speech (their own slider)
        let unit = path.parent().and_then(|p| p.file_name()).map_or(false, |f| f.eq_ignore_ascii_case("troops"));
        self.speech_vol[ch] = (volume, unit);
        let sink = &self.speech_sinks[ch];
        sink.stop(); sink.set_volume(self.speech_gain(ch)); sink.append(src); sink.play();
    }
    /// MyAudioManager.StopSpeech.
    pub fn stop_speech(&mut self, channel: i32) { self.speech_sinks[channel.clamp(1, 3) as usize - 1].stop(); }
    /// MyAudioManager.isSpeechPlaying.
    pub fn speech_playing(&self, channel: i32) -> bool { !self.speech_sinks[channel.clamp(1, 3) as usize - 1].empty() }
    fn speech_gain(&self, ch: usize) -> f32 {
        let (v, unit) = self.speech_vol[ch];
        v * if unit { self.vols.unit_speech } else { self.vols.speech } * self.vols.master
    }
    fn music_gain(&self) -> f32 {
        self.music_vol.0 * self.music_vol.1 * self.vols.music * self.vols.master * if self.ducked { 0.3 } else { 1.0 }
    }
    /// Options sliders: applies to new sounds and to the playing music, ambient, speech and video channels.
    pub fn set_volumes(&mut self, v: Volumes) {
        self.vols = v;
        self.music.set_volume(self.music_gain());
        for ch in 0..2 { self.ambient_sinks[ch].set_volume(self.ambient_vol[ch] * v.sfx * v.master); }
        for ch in 0..3 { self.speech_sinks[ch].set_volume(self.speech_gain(ch)); }
        self.video.set_volume(self.video_vol * v.speech * v.master);
    }

    /// `name` like `fx\music\oud_07.raw` → StreamingAssets/Music/oud_07.wav (MyAudioManager.PlayMusic). `followon`
    /// while a piece plays: the next piece (replacing an earlier one), started when the playing one ends, whose loop
    /// it turns off.
    pub fn play_music(&mut self, name: &str, volume: f32, loop_: bool, followon: bool) {
        if followon && !self.music.empty() {
            self.music_keep.store(false, Relaxed);
            self.music_next = Some((name.to_string(), volume, loop_));
            return;
        }
        let base = clip_file(name).to_lowercase().replace(".raw", ".wav");
        if base == "null.wav" { return self.stop_music(); }
        let clip_vol = self.clip_volume(name);
        let Some(path) = std::fs::read_dir(self.dir.join("music")).ok().and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.file_name().map_or(false, |f| f.to_string_lossy().to_lowercase() == base))) else { eprintln!("audio: no music {base}"); return };
        // ponytail: music WAVs are up to 100 MB, streamed from file instead of cached
        let Some(src) = std::fs::File::open(&path).ok().and_then(|f| Decoder::new_wav(std::io::BufReader::new(f)).ok()) else { return };
        self.music.stop();
        self.music_next = None;
        self.music_len = src.total_duration().map(|d| (d, Instant::now()));
        self.music_about = false;
        self.music_keep = Arc::new(AtomicBool::new(loop_));
        self.music_vol = (volume, clip_vol);
        self.music.set_volume(self.music_gain());
        if loop_ {
            let b = src.buffered();
            self.music.append(Looping { first: b.clone(), cur: b, keep: self.music_keep.clone() })
        } else {
            self.music.append(src)
        }
        self.music.play();
    }
    pub fn stop_music(&mut self) {
        self.music.stop();
        self.music_next = None;
    }
    /// Once per frame (MyAudioManager.Update): starts the follow-on piece when the playing one ended. Returns
    /// (playing, about to loop) = DLL_RunTick's musicPlaying / musicAboutToLoop.
    /// ponytail: the piece's position from the wall clock since it started.
    pub fn music_update(&mut self) -> (bool, bool) {
        if self.music.empty() {
            if let Some((n, v, l)) = self.music_next.take() { self.play_music(&n, v, l, false) }
        }
        if let Some((len, t0)) = self.music_len.filter(|(l, _)| !l.is_zero()) {
            if t0.elapsed().as_nanos() % len.as_nanos() + 1_000_000_000 > len.as_nanos() { self.music_about = true }
        }
        (!self.music.empty(), self.music_about)
    }
    /// A video's soundtrack (a WAV file, e.g. re/scripts/export_video.py's intro.wav) on its own channel, streamed;
    /// None stops it.
    pub fn play_video(&mut self, wav: Option<&Path>, volume: f32) {
        self.video.stop();
        let Some(src) = wav.and_then(|p| std::fs::File::open(p).ok()).and_then(|f| Decoder::new_wav(std::io::BufReader::new(f)).ok()) else { return };
        self.video_vol = volume;
        self.video.set_volume(volume * self.vols.speech * self.vols.master); // IntroSequence.StartVideo: speech volume
        self.video.append(src);
        self.video.play();
    }
    /// Nothing playing or queued on the music channel.
    pub fn music_idle(&self) -> bool { self.music.empty() }
    pub fn set_music_volume(&mut self, volume: f32) { self.music_vol.0 = volume; self.music.set_volume(self.music_gain()); }
    /// MyAudioManager.setMusicFadedState (ReduceMusicVolumeForSpeech, on by default): music at 0.3× while a speech
    /// channel plays. Call once per frame.
    pub fn duck_for_speech(&mut self) {
        self.ducked = self.vols.reduce_music_for_speech && self.speech_sinks.iter().any(|s| !s.empty());
        self.music.set_volume(self.music_gain());
    }

    /// Handle one draw-list record of type 16/17/18/19/21 (GameMap.processTestMap). `ret_data` is the
    /// tick's `PlayStateReturnData` blob (speech/music/bink file names live there). Returns a log line.
    pub fn play_record(&mut self, rec: &[i16; 21], ret_data: &[u8]) -> Option<String> {
        let name = |off: usize| -> String {
            let b = ret_data.get(off..off + 128).unwrap_or(&[]);
            String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned()
        };
        let (vol, p12) = (rec[11] as f32 / 100.0, rec[12]);
        Some(match rec[0] {
            16 => {
                let id = rec[3] as i32;
                if rec[11] >= 0 && rec[12] >= 0 {
                    self.play_sound(id, vol, (p12 as f32 - 64.0) / 64.0);
                    format!("sfx {id} vol {vol} pan {p12}")
                } else {
                    // ponytail: positional (tile 1,2) sound; the real game pans/attenuates by camera, we have none
                    self.play_sound(id, 0.7, 0.0);
                    format!("sfx {id} @({},{})", rec[1], rec[2])
                }
            }
            17 => {
                let (id, mode, ch) = (rec[3] as i32, rec[4], p12 as i32);
                match mode {
                    0 | 1 => self.play_ambient(ch, id, vol, mode == 1),
                    5 => self.set_ambient_volume(ch, vol),
                    10 => self.stop_ambient(ch),
                    _ => {}
                }
                format!("ambient ch{ch} id {id} mode {mode} vol {vol}")
            }
            18 => { let n = name(SPEECH_NAME_OFF); self.play_speech(p12 as i32, &n, vol); format!("speech ch{p12} {n}") }
            19 => {
                let n = name(MUSIC_NAME_OFF);
                match p12 {
                    0 | 1 => self.play_music(&n, vol, rec[3] > 0, p12 == 1),
                    5 => self.set_music_volume(vol),
                    10 => self.stop_music(),
                    _ => {}
                }
                format!("music mode {p12} {n} vol {vol} loop {}", rec[3])
            }
            21 => format!("bink {} (video not played)", if rec[3] < 0 { "stop".into() } else { name(BINK_NAME_OFF) }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn volume_file() {
        let m = super::parse_volume("-------Group 01 -- volume = -10 ----\n\"fx\\chop1 22k.wav\"\t\t127\n-------Group 02 -- volume = +30 ----\n\"fx\\music\\oud_07.raw\"\t\t97\n");
        assert_eq!(m["fx\\chop1 22k"], 117.0 / 127.0);
        assert_eq!(m["fx\\music\\oud_07.raw"], 1.0);
    }
}
