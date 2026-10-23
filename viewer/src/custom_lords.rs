//! DE's custom lords (CustomisationFileManager): `CustomLords/<Name>/` holds `<x>.lordjson` (the 377-field AIC
//! personality as JSON, CustomisationFileManager.NewAIC), `*.aivjson` castles (AIVLoader.SaveData) and `avatar.png`
//! (144²); `CustomMedia/<Name>/` holds the lord's 30 speech wavs, `angry/happy/neutral/sad.webm` and `text.txt`
//! (StreamingAssets/CustomMediaExample/ReadMe.txt). Both folders in our player folder or the real game's.
//!
//! In the lobby a custom lord k is lord index `BASE + k` of `skirmish::Lobby::ais`. It registers as DE does
//! (FRONT_Multiplayer.StartSkirmishGame): lord type 29 + slot, its castles as custom AIVs, then
//! `DLL_SetExtendedLordConfig` (sim `ai::set_extended_lord_config`), which makes the engine's speech lines
//! "*<player>*SK_<KEY>" (hudmsg::f102bc0); audio.rs, video.rs and hud.rs resolve those here.
//! Our extension, also in CustomLords/<Name>/: `help.txt` (the Add-AI panel's 5 rows, `TEXT_AI_LORD_HELP_001 ="..."`
//! .. `_005`, and `TITLE`), `bio.txt` (the backstory page's text), `portrait.png` + `arms.png` (its pictures),
//! `trailer.webm` (its story video, played full screen) and `sprite/` (the lord's own map
//! sprite: `atlases.tsv` + `sprites.tsv` as export_atlases.py writes them, one BC7 atlas `<name>.bc` + BC1 mask
//! `<name>_m.bc`, frames named `body_lord-<n>`), used for that player's lord only.
//! ponytail: the first .lordjson and all .aivjson of a folder (DE's AI Settings popup picks among several); Workshop
//! folders and the duplicate-name nicknames (NICKNAME1..8) are not read; LAN lobbies hide the custom lords (the
//! peers' sims would need the same files).
use crate::menu::trail_menu::masters::J;
use std::{path::{Path, PathBuf}, sync::Mutex};

/// Lobby lord index of custom lord k = BASE + k.
pub const BASE: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Custom {
    pub name: String,
    pub config: Vec<i32>,      // the 377 AIC fields (re/data/aic_fields.tsv order)
    pub aivs: Vec<Vec<i16>>,   // castles as DLL_ImportAIV blobs
    pub avatar: Option<Vec<u8>>, // face.png (alpha) or avatar.png as 130² RGBA (the coat-of-arms face sheet's slot size)
    pub media: Option<PathBuf>,  // CustomMedia/<name>
    pub dir: PathBuf,            // CustomLords/<name>
}

/// The lords found. Tests: one list per thread, as their profile folder is (parallel tests rescan and start games).
fn list() -> std::sync::MutexGuard<'static, Vec<Custom>> {
    #[cfg(test)]
    thread_local! { static L: &'static Mutex<Vec<Custom>> = Box::leak(Box::default()); }
    #[cfg(test)]
    return L.with(|l| *l).lock().unwrap();
    #[cfg(not(test))]
    { static L: Mutex<Vec<Custom>> = Mutex::new(Vec::new()); L.lock().unwrap() }
}
/// Players 1..8 of the running game: their custom lord (set by skirmish::setup). Tests: per thread, as `list`.
fn in_game_all() -> std::sync::MutexGuard<'static, [Option<Custom>; 9]> {
    #[cfg(test)]
    thread_local! { static G: &'static Mutex<[Option<Custom>; 9]> = Box::leak(Box::default()); }
    #[cfg(test)]
    return G.with(|g| *g).lock().unwrap();
    #[cfg(not(test))]
    { static G: Mutex<[Option<Custom>; 9]> = Mutex::new([const { None }; 9]); G.lock().unwrap() }
}

fn roots() -> Vec<PathBuf> { vec![crate::skirmish::profile_dir(), crate::paths::game_user()] }

/// Scan the CustomLords folders (the lobby opens): the lords found, by name.
pub fn rescan() -> usize {
    let mut v: Vec<Custom> = vec![];
    for root in roots() {
        let Ok(dirs) = std::fs::read_dir(root.join("CustomLords")) else { continue };
        let mut dirs: Vec<PathBuf> = dirs.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for d in dirs {
            if let Some(c) = load(&d) {
                if !v.iter().any(|o| o.name == c.name) { v.push(c); }
            }
        }
    }
    let n = v.len();
    *list() = v;
    n
}
pub fn count() -> usize { list().len() }
pub fn names() -> Vec<String> { list().iter().map(|c| c.name.clone()).collect() }
pub fn get(k: usize) -> Option<Custom> { list().get(k).cloned() }
pub fn find(name: &str) -> Option<Custom> { list().iter().find(|c| c.name == name).cloned() }
/// Lobby lord index `n` as a custom lord index.
pub fn lord_of(n: usize) -> Option<usize> { n.checked_sub(BASE) }
/// The custom lord of lobby lord index `n` (a stale list, e.g. another thread's rescan in the tests, is scanned again).
pub fn of_lord(n: usize) -> Option<Custom> { let k = lord_of(n).filter(|&k| k < 900)?; get(k).or_else(|| { rescan(); get(k) }) } // 900..: a LAN lord missing here (lord_share)
/// The built-in lord (`skirmish::LORDS` index) a custom lord was made from: AIC opponent_type is the lord + 1 for the
/// built-ins and DE's editor keeps it; falls back to the Rat. Its default castles / face stand in where needed.
pub fn parent(c: &Custom) -> usize {
    let n = c.config.first().copied().unwrap_or(0) - 1;
    if (0..crate::skirmish::LORDS.len() as i32).contains(&n) { n as usize } else { 0 }
}

pub fn set_in_game(p: usize, c: Option<Custom>) {
    if let Some(c) = &c { export_videos(c); }
    if (1..9).contains(&p) { in_game_all()[p] = c; }
}
/// The reaction videos as minimap clips (video.rs plays .vid files only), off the UI thread (export_custom keeps the
/// ones exported before).
pub fn export_videos(c: &Custom) {
    let slug = slug(&c.name);
    // the story trailer (our extension): full screen, the backstory page's "Watch his story"
    // (one export per lord at a time: the page can be opened again while it runs, ~1 min)
    static BUSY: Mutex<Vec<String>> = Mutex::new(Vec::new());
    if let Some(src) = file_in(&c.dir, "trailer.webm") {
        let stem = format!("custom_{slug}_trailer");
        if !BUSY.lock().unwrap().contains(&stem) {
            BUSY.lock().unwrap().push(stem.clone());
            std::thread::spawn(move || {
                if let Err(e) = shc_extract::video::export_custom_full(&src, &crate::cache_dir().join("video"), &stem) { eprintln!("custom lord trailer {}: {e}", src.display()); }
                BUSY.lock().unwrap().retain(|s| *s != stem);
            });
        }
    }
    let Some(m) = c.media.clone() else { return };
    std::thread::spawn(move || for mood in ["angry", "happy", "neutral", "sad"] {
        let (dir, stem) = (crate::cache_dir().join("video"), format!("custom_{slug}_{mood}"));
        if let Some(src) = file_in(&m, &format!("{mood}.webm")) {
            if let Err(e) = shc_extract::video::export_custom(&src, &dir, &stem) { eprintln!("custom lord video {}: {e}", src.display()); }
            // the neutral one again as the backstory page's living portrait (lord_info.rs): video.rs's banner slot
            // (528×308), the oval fade in its alpha
            if mood == "neutral" {
                if let Err(e) = shc_extract::video::export_custom_as(&src, &dir, &format!("custom_{slug}_portrait"), (528, 308), portrait_frame) { eprintln!("custom lord portrait video {}: {e}", src.display()); }
            }
        }
    });
}
/// A video frame as the portrait clip's: scaled to cover 528×308 (a fifth of the overhang off the top), faded to an oval.
fn portrait_frame(f: shc_extract::tex::Rgba) -> shc_extract::tex::Rgba {
    let (w, h) = (528, 308);
    let k = (w as f32 / f.w as f32).max(h as f32 / f.h as f32);
    let (sw, sh) = (((f.w as f32 * k).ceil() as usize).max(w), ((f.h as f32 * k).ceil() as usize).max(h));
    let mut o = f.resize(sw, sh).crop((sw - w) / 2, (sh - h) / 5, w, h);
    for y in 0..h { for x in 0..w { o.px[(y * w + x) * 4 + 3] = (oval(x, y, w, h) * 255.0).round() as u8; } }
    o
}
/// The backstory portrait's fade at pixel (x, y) of `w`×`h`: 1 inside 60 % of the ellipse, smoothly 0 at its edge.
pub fn oval(x: usize, y: usize, w: usize, h: usize) -> f32 {
    let (dx, dy) = ((x as f32 + 0.5) / w as f32 * 2.0 - 1.0, (y as f32 + 0.5) / h as f32 * 2.0 - 1.0);
    let t = ((1.0 - (dx * dx + dy * dy).sqrt()) / 0.4).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
/// The lord has a story trailer (trailer.webm), exported or not yet.
pub fn has_trailer(c: &Custom) -> bool { file_in(&c.dir, "trailer.webm").is_some() }
/// The lord's `mood` video ("angry".., "portrait", "trailer") as video.rs's .vid name, once exported (`export_videos`).
pub fn vid(c: &Custom, mood: &str) -> Option<String> {
    let f = format!("custom_{}_{mood}.vid", slug(&c.name));
    crate::cache_dir().join("video").join(&f).exists().then_some(f)
}
pub fn in_game(p: usize) -> Option<Custom> { in_game_all().get(p).cloned().flatten() }
/// The custom lord behind an engine lord kind (computer_register / PlayStateReturnData: 30 + slot = player + 0x1d).
pub fn by_kind(kind: i32) -> Option<Custom> { (30..=37).contains(&kind).then(|| in_game((kind - 29) as usize)).flatten() }

/// Lord speech ids 1..33 (FUN_180102bc0) → the SK_ key, set 0 of the engine's wav / bik tables (0x1803143f0).
const KEYS: [&str; 34] = ["", "TAUNT1", "TAUNT2", "TAUNT3", "TAUNT4", "ANGRY_SIEGE_LOST", "ANGRY_CASTLE_DAMAGED", "DEFEAT", "NERV_PRE_SIEGE",
    "NERV_WEAK", "VICTORY_GOOD", "VICTORY_HARASS", "KILL_PLAYER", "KILL_NPC", "REQUEST_GOODS", "THANK_GOODS", "DIE_ALLY", "CONGRATS_ON_KILL",
    "BOAST_OF_KILL", "ALLY_NEED_HELP", "COOKIE", "KICK_PLAYER", "ADD_PLAYER", "ABOUT2SIEGE", "CANT_ATTACK", "WONT_ATTACK", "CANT_HELP", "WONT_HELP",
    "NOT_SENDING_GOODS", "SENT_GOODS", "TEAM_WINNING", "TEAM_LOSING", "WILL_SEND_TROOPS", "WILL_ATTACK_ENEMY"];
pub fn key(id: i32) -> Option<&'static str> { KEYS.get(id as usize).copied().filter(|k| !k.is_empty()) }
/// SFXManager.customLordCustomBinks: the reaction video of a line (key without "SK_", any case).
pub fn mood(key: &str) -> &'static str {
    match key.trim_start_matches("SK_").trim_start_matches("sk_").to_ascii_uppercase().as_str() {
        "TAUNT1" | "TAUNT2" | "TAUNT4" | "ANGRY_SIEGE_LOST" | "VICTORY_HARASS" | "KILL_PLAYER" | "WONT_HELP" | "NOT_SENDING_GOODS" => "angry",
        "ANGRY_CASTLE_DAMAGED" | "DEFEAT" | "DIE_ALLY" | "CANT_ATTACK" | "WONT_ATTACK" | "CANT_HELP" | "TEAM_LOSING" => "sad",
        "VICTORY_GOOD" | "THANK_GOODS" | "CONGRATS_ON_KILL" | "BOAST_OF_KILL" | "TEAM_WINNING" | "WILL_ATTACK_ENEMY" => "happy",
        _ => "neutral",
    }
}
/// An engine speech / video name "*<player>*SK_<KEY>[.wav]" → (player, KEY).
pub fn split(name: &str) -> Option<(usize, String)> {
    let mut it = name.trim_end_matches(".wav").trim_end_matches(".bik").trim_end_matches(".webm").split('*');
    let (_, p, key) = (it.next()?, it.next()?.parse().ok()?, it.next()?);
    Some((p, key.trim_start_matches("SK_").trim_start_matches("sk_").to_ascii_uppercase()))
}
/// Player p's line KEY as its CustomMedia wav.
pub fn wav(p: usize, key: &str) -> Option<PathBuf> { file_in(&in_game(p)?.media?, &format!("{}.wav", key.to_ascii_lowercase())) }
/// Player p's line KEY as its exported reaction video's stem (video.rs), once the export is there.
pub fn video_stem(p: usize, key: &str) -> Option<String> {
    let c = in_game(p)?;
    c.media.as_ref()?;
    let stem = format!("custom_{}_{}", slug(&c.name), mood(key));
    crate::cache_dir().join("video").join(format!("{stem}.vid")).exists().then_some(stem)
}
/// Player p's line KEY as its text.txt line.
pub fn text(p: usize, key: &str) -> Option<String> { kv(&std::fs::read_to_string(file_in(&in_game(p)?.media?, "text.txt")?).ok()?, key) }
/// `KEY ="text"` lines (DE's text.txt): KEY's text.
fn kv(t: &str, key: &str) -> Option<String> {
    t.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim().eq_ignore_ascii_case(key)).then(|| v.trim().trim_matches('"').to_string())
    })
}
/// The Add-AI panel's row k (0..5, label TEXT_AI_LORD_HELP_00<k+1>) from help.txt.
pub fn help(c: &Custom, k: usize) -> Option<String> { help_text(c, &format!("TEXT_AI_LORD_HELP_{:03}", k + 1)) }
/// help.txt's line `key`: the 5 rows, and `TITLE` (the backstory page's subtitle).
pub fn help_text(c: &Custom, key: &str) -> Option<String> {
    kv(&std::fs::read_to_string(file_in(&c.dir, "help.txt")?).ok()?, key).filter(|v| !v.is_empty())
}
/// bio.txt: the backstory, paragraphs split by blank lines.
pub fn bio(c: &Custom) -> Vec<String> {
    let t = file_in(&c.dir, "bio.txt").and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default().replace("\r\n", "\n");
    t.split("\n\n").map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()).collect()
}
/// Picture `file` of the lord's folder (portrait.png, arms.png) as `w`×`h` RGBA, scaled to cover and cut to fit: centred
/// across, a fifth of the overhang taken from the top (faces sit high). None without one.
pub fn picture(c: &Custom, file: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    let img = image::open(file_in(&c.dir, file)?).ok()?;
    let k = (w as f32 / img.width() as f32).max(h as f32 / img.height() as f32);
    let img = img.resize_exact(((img.width() as f32 * k).ceil() as u32).max(w), ((img.height() as f32 * k).ceil() as u32).max(h), image::imageops::FilterType::Triangle);
    Some(img.crop_imm((img.width() - w) / 2, (img.height() - h) / 5, w, h).to_rgba8().into_raw())
}

/// Startup (main.rs, after load_tables): every custom lord's `sprite/` atlas and frames appended, the atlas named by
/// its full path (loader.rs reads `<name>.bc` / `<name>_m.bc`), the frames as `<slug>/<frame>` for `Gm::lord`.
pub fn add_sprites(atlases: &mut Vec<crate::Atlas>, named: &mut Vec<(String, crate::Sprite)>) {
    rescan();
    for c in list().iter() {
        let dir = c.dir.join("sprite");
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
        let (Some(a), Some(s)) = (read("atlases.tsv"), read("sprites.tsv")) else { continue };
        let Ok((mut a, s)) = std::panic::catch_unwind(|| crate::parse_tables(&a, &s)) else { eprintln!("custom lord {}: bad sprite/ tables", c.name); continue };
        let base = atlases.len() as u32;
        let ok = a.iter().all(|a| dir.join(format!("{}.bc", a.name)).is_file()) && s.iter().all(|(_, p)| (p.atlas as usize) < a.len());
        if !ok { eprintln!("custom lord {}: sprite/ incomplete, not used", c.name); continue; }
        for x in &mut a { x.name = dir.join(&x.name).to_string_lossy().into_owned(); }
        let n = s.len();
        named.extend(s.into_iter().map(|(k, mut p)| { p.atlas += base; (format!("{}/{k}", slug(&c.name)), p) }));
        atlases.extend(a);
        println!("custom lord {}: {n} sprite frames", c.name);
    }
}
/// Player p's custom lord's sprite prefix (`add_sprites`), when it has one.
pub fn sprite_of(p: usize) -> Option<String> { in_game(p).filter(|c| c.dir.join("sprite/sprites.tsv").is_file()).map(|c| slug(&c.name)) }
/// Lobby: the lord's ADD_PLAYER / KICK_PLAYER wav (FRONT_Multiplayer AddCustomLord).
pub fn lobby_wav(c: &Custom, key: &str) -> Option<PathBuf> { file_in(c.media.as_ref()?, &format!("{key}.wav")) }

/// `name` in `dir`, case-blind (DE's example files are upper case).
fn file_in(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(name)))
}
fn slug(name: &str) -> String { name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect() }

/// The sheet face of a custom lord (avatar.png), false = none: the caller draws the parent's face.
pub fn draw_face(out: &mut Vec<crate::hud::HudTri>, c: &Custom, p: [f32; 4], color: [f32; 4]) -> bool {
    let Some(px) = &c.avatar else { return false };
    crate::menu::coa::draw_picture(out, &format!("custom:{}", c.name), || Some(px.clone()), p, color)
}

/// One CustomLords/<Name> folder; None without a .lordjson (DE also wants an .aivjson; we fall back to the parent's
/// castles).
fn load(dir: &Path) -> Option<Custom> {
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    files.sort();
    let ext = |p: &Path, e: &str| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
    let config = files.iter().filter(|p| ext(p, "lordjson")).find_map(|p| lordjson(&std::fs::read_to_string(p).ok()?))?;
    let aivs = files.iter().filter(|p| ext(p, "aivjson")).filter_map(|p| aivjson(&std::fs::read_to_string(p).ok()?)).collect();
    // face.png (our extension: the figure on alpha, the faces' stone wall shows around him) before DE's opaque avatar.png
    let avatar = ["face.png", "avatar.png"].iter().find_map(|n| files.iter().find(|p| p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(n))).and_then(|p| avatar(&std::fs::read(p).ok()?)));
    let media = roots().into_iter().map(|r| r.join("CustomMedia").join(&name)).find(|d| d.is_dir());
    Some(Custom { name, config, aivs, avatar, media, dir: dir.to_path_buf() })
}

/// A .lordjson (LordRoot { lord: NewAIC }) as the 377 AIC fields: EngineInterface.CreateAILordConfigData copies
/// NewAIC's scalars by name and its arrays by `name` + index (farm_types1 = farm_types[0]); extendedLordParent and
/// the free fields stay 0 (the engine sets the parent).
pub fn lordjson(text: &str) -> Option<Vec<i32>> {
    let j = J::parse(text)?;
    let lord = j.get("lord").unwrap_or(&j);
    if !matches!(lord, J::Obj(_)) { return None; }
    let mut cfg = vec![0i32; 377];
    for (k, line) in include_str!("../../re/data/aic_fields.tsv").lines().skip(1).enumerate().take(280) {
        let name = line.split('\t').nth(3).unwrap_or("");
        if k == 276 { continue; } // extendedLordParent: the engine's
        let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let v = match (digits > 0).then(|| (&name[..name.len() - digits], name[name.len() - digits..].parse::<usize>().ok())) {
            Some((base, Some(i))) if matches!(lord.get(base), Some(J::Arr(_))) => lord.ints(base).get(i.wrapping_sub(1)).copied().unwrap_or(0),
            _ => lord.int(name),
        };
        cfg[k] = v as i32;
    }
    Some(cfg)
}

/// An .aivjson (AIVLoader.SaveData) as the DLL_ImportAIV blob.
pub fn aivjson(text: &str) -> Option<Vec<i16>> {
    let j = J::parse(text)?;
    let objs = |k: &str| -> Vec<&J> { if let Some(J::Arr(a)) = j.get(k) { a.iter().collect() } else { vec![] } };
    let frames: Vec<(i16, Vec<i16>, bool)> = objs("frames").iter()
        .map(|f| (f.int("itemType") as i16, f.ints("tilePositionOfsets").iter().map(|&v| v as i16).collect(), f.int("shouldPause") != 0)).collect();
    if frames.is_empty() { return None; }
    let misc: Vec<(i16, i16, i16)> = objs("miscItems").iter().map(|m| (m.int("itemType") as i16, m.int("positionOfset") as i16, m.int("number") as i16)).collect();
    Some(shc_formats::aiv::encode_save(j.int("pauseDelayAmount") as i16, &frames, &misc))
}

/// avatar.png (DE wants 144²; any size is sampled) as the face sheet's 130² RGBA.
fn avatar(png: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 { return None; }
    let mut out = Vec::with_capacity(130 * 130 * 4);
    for y in 0..130 {
        for x in 0..130 {
            let p = img.get_pixel((x * w / 130) as u32, (y * h / 130) as u32).0;
            out.extend_from_slice(&p);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A built-in lord's config row (re/data/ai_lords.tsv, lord index as skirmish::LORDS) as DE's editor would save it:
    /// a field whose name ends in digits is an array entry (the rule CreateAILordConfigData follows, checked
    /// against its 279 assignments).
    pub fn lordjson_of(lord: usize) -> String {
        let row = include_str!("../../re/data/ai_lords.tsv").lines().nth(lord + 2).unwrap();
        let vals: Vec<i32> = row.split('\t').skip(1).map(|v| v.parse().unwrap()).collect();
        let mut fields: Vec<(String, Vec<i32>, bool)> = vec![];
        for (k, line) in include_str!("../../re/data/aic_fields.tsv").lines().skip(1).enumerate().take(280) {
            let name = line.split('\t').nth(3).unwrap();
            if k == 276 { continue; }
            let base = name.trim_end_matches(|c: char| c.is_ascii_digit());
            if base.len() < name.len() {
                if let Some(f) = fields.iter_mut().find(|f| f.0 == base) { f.1.push(vals[k]); continue; }
                fields.push((base.into(), vec![vals[k]], true));
            } else {
                fields.push((name.into(), vec![vals[k]], false));
            }
        }
        let body: Vec<String> = fields.iter().map(|(n, v, arr)| if *arr { format!("\"{n}\":[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")) } else { format!("\"{n}\":{}", v[0]) }).collect();
        format!("{{\"lord\":{{{}}}}}", body.join(","))
    }

    #[test]
    fn lordjson_fields_by_name_and_array() {
        let j = r#"{"lord":{"opponent_type":17,"opponent_type_for_speech":17,"lord_gfx_type":0,"flag_type":13,"farm_types":[32,31,33,0,0,0,0,0],"min_tax":3,"max_tax":11,"sell_all":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"starting_troops_crusader":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28],"lord_hps_percent":100,"siege_max_troops":200}}"#;
        let c = lordjson(j).unwrap();
        assert_eq!((c.len(), c[0], c[3], c[11], c[13], c[9], c[10]), (377, 17, 13, 32, 33, 3, 11));
        assert_eq!((c[246], c[273], c[275], c[277], c[276]), (1, 28, 100, 200, 0), "crusader1, crusader28, hps, siege_max_troops; parent left to the engine");
        assert!(lordjson("{}").is_none() || lordjson("{}").unwrap().iter().all(|&v| v == 0));
        // a shipped lord round-trips through the editor's layout (field order = the C# copy order)
        let jewel = lordjson(&lordjson_of(16)).unwrap();
        let row: Vec<i32> = include_str!("../../re/data/ai_lords.tsv").lines().nth(18).unwrap().split('\t').skip(1).map(|v| v.parse().unwrap()).collect();
        assert_eq!((&jewel[..276], &jewel[277..280]), (&row[..276], &row[277..280]));
    }

    #[test]
    fn aivjson_to_blob() {
        let j = r#"{"pauseDelayAmount":100,"frames":[{"itemType":61,"tilePositionOfsets":[5643],"shouldPause":false},{"itemType":25,"tilePositionOfsets":[5741,5742],"shouldPause":true}],"miscItems":[{"positionOfset":4000,"itemType":9006,"number":0}]}"#;
        let d = aivjson(j).unwrap();
        assert_eq!(d, vec![100, 2, 0, 3, 2, 61, 5643, -25, 2, 5741, 5742, 1, 6, 4000, 0]);
        assert!(shc_formats::aiv::parse(&d).is_ok());
        assert!(aivjson(r#"{"frames":[]}"#).is_none());
    }

    #[test]
    fn speech_keys_and_media_names() {
        assert_eq!((key(1), key(22), key(33), key(0), key(34)), (Some("TAUNT1"), Some("ADD_PLAYER"), Some("WILL_ATTACK_ENEMY"), None, None));
        assert_eq!(split("*3*SK_TAUNT2.wav"), Some((3, "TAUNT2".into())));
        assert_eq!(split("*3*sk_nerv_weak"), Some((3, "NERV_WEAK".into())));
        assert_eq!(split("rt_taunt_01.wav"), None);
        assert_eq!((mood("SK_TAUNT3"), mood("sk_defeat"), mood("THANK_GOODS"), mood("TAUNT1")), ("neutral", "sad", "happy", "angry"));
        assert_eq!(slug("Lord David von Lindschied"), "lord_david_von_lindschied");
    }

    /// A CustomLords folder made from the Jewel (her shipped config and first castle) + the game's CustomMediaExample
    /// (hers too) loads, and the lobby's lord index maps to it.
    /// A player folder with CustomLords/Test Lord (the Jewel's config, one castle, a grey avatar) and, when the game
    /// install is there, CustomMedia/Test Lord = the game's CustomMediaExample (hers too).
    fn test_folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shc-custom-lord-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let lord = dir.join("CustomLords/Test Lord");
        std::fs::create_dir_all(&lord).unwrap();
        std::fs::write(lord.join("Test Lord.lordjson"), lordjson_of(16)).unwrap();
        std::fs::write(lord.join("castle.aivjson"), r#"{"pauseDelayAmount":100,"frames":[{"itemType":61,"tilePositionOfsets":[5643],"shouldPause":false}],"miscItems":[]}"#).unwrap();
        let mut png = Vec::new();
        image::write_buffer_with_format(&mut std::io::Cursor::new(&mut png), &vec![200u8; 144 * 144 * 4], 144, 144, image::ExtendedColorType::Rgba8, image::ImageFormat::Png).unwrap();
        std::fs::write(lord.join("avatar.png"), png).unwrap();
        let example = crate::paths::streaming_assets().join("CustomMediaExample");
        if example.is_dir() {
            let media = dir.join("CustomMedia/Test Lord");
            std::fs::create_dir_all(&media).unwrap();
            for f in std::fs::read_dir(&example).unwrap().flatten() { std::fs::copy(f.path(), media.join(f.file_name())).unwrap(); }
        }
        dir
    }

    #[test]
    fn folder_scan() {
        let dir = test_folder("scan");
        let example = crate::paths::streaming_assets().join("CustomMediaExample");
        crate::skirmish::set_test_profile(dir.clone());
        {
            assert_eq!(rescan(), 1);
            let c = of_lord(BASE).unwrap();
            assert_eq!((c.name.as_str(), c.config[0], c.aivs.len(), c.avatar.as_ref().map(|a| a.len()), parent(&c)), ("Test Lord", 17, 1, Some(130 * 130 * 4), 16));
            set_in_game(2, Some(c.clone()));
            assert_eq!(by_kind(31).map(|c| c.name), Some("Test Lord".into()));
            if example.is_dir() {
                assert!(wav(2, "TAUNT1").is_some_and(|p| p.exists()), "TAUNT1.wav");
                assert_eq!(text(2, "DEFEAT").as_deref(), Some("I die as a girl should, waging bloody warfare!!"));
                assert!(lobby_wav(&c, "ADD_PLAYER").is_some());
            }
            set_in_game(2, None);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Our own files: help.txt rows (DE's text.txt format), bio.txt paragraphs (CRLF too), portrait.png, and the
    /// sprite/ folder appended to the sprite tables and worn by player 2's lord only.
    #[test]
    fn help_bio_portrait_sprite() {
        let dir = test_folder("own");
        let lord = dir.join("CustomLords/Test Lord");
        std::fs::write(lord.join("help.txt"), "TEXT_AI_LORD_HELP_001 =\"Cold steel.\"\ntext_ai_lord_help_005 = \"“Gnade? Nein.”\"\nTEXT_AI_LORD_HELP_003 =\"\"\nTITLE =\"the Cold\"\n").unwrap();
        std::fs::write(lord.join("bio.txt"), "A second son,\r\nno land.\r\n\r\nHe burns  farms.\r\n").unwrap();
        let mut png = Vec::new();
        image::write_buffer_with_format(&mut std::io::Cursor::new(&mut png), &[9u8, 8, 7, 255].repeat(20 * 10), 20, 10, image::ExtendedColorType::Rgba8, image::ImageFormat::Png).unwrap();
        std::fs::write(lord.join("portrait.png"), png).unwrap();
        crate::skirmish::set_test_profile(dir.clone());
        assert_eq!(rescan(), 1);
        let c = get(0).unwrap();
        assert_eq!((0..5).map(|k| help(&c, k)).collect::<Vec<_>>(), [Some("Cold steel.".into()), None, None, None, Some("“Gnade? Nein.”".into())]);
        assert_eq!(help_text(&c, "TITLE").as_deref(), Some("the Cold"));
        assert!(!has_trailer(&c));
        std::fs::write(lord.join("Trailer.webm"), b"").unwrap(); // found case-blind, like DE's media files
        assert!(has_trailer(&c) && vid(&c, "trailer").is_none(), "not exported yet: the page says it is preparing");
        assert_eq!(bio(&c), ["A second son, no land.", "He burns farms."]);
        assert_eq!(picture(&c, "portrait.png", 6, 4).map(|p| (p.len(), p[..4].to_vec())), Some((6 * 4 * 4, vec![9, 8, 7, 255])));
        assert_eq!(picture(&c, "portrait.png", 4, 6).map(|p| p.len()), Some(4 * 6 * 4), "cut to fit the other way");
        assert!(picture(&c, "arms.png", 4, 4).is_none());
        // the living portrait's frames: video.rs's banner slot size, opaque middle, clear corners
        let f = portrait_frame(shc_extract::tex::Rgba { w: 348, h: 348, px: vec![200; 348 * 348 * 4] });
        assert_eq!((f.w, f.h, f.px[(154 * 528 + 264) * 4 + 3], f.px[3], f.px[(307 * 528 + 527) * 4 + 3]), (528, 308, 255, 0, 0));
        assert_eq!(find("Test Lord").map(|c| c.dir), Some(lord.clone()));
        // sprite/ without its atlas file: not used
        let sp = lord.join("sprite");
        std::fs::create_dir_all(&sp).unwrap();
        std::fs::write(sp.join("atlases.tsv"), "0\tbody_lord\t4\t4\t25\n").unwrap();
        std::fs::write(sp.join("sprites.tsv"), "body_lord-0\t0\t0\t0\t4\t4\t0.5\t0.5\t-2,2;-2,-2;2,-2\nbody_lord-1\t0\t0\t0\t4\t4\t0.5\t0.5\t-2,2;-2,-2;2,-2\n").unwrap();
        let (mut atlases, mut named) = (vec![crate::Atlas { name: "AllTileSprites".into(), w: 8, h: 8 }], vec![]);
        add_sprites(&mut atlases, &mut named);
        assert_eq!((atlases.len(), named.len()), (1, 0));
        std::fs::write(sp.join("body_lord.bc"), [0u8; 16]).unwrap();
        add_sprites(&mut atlases, &mut named);
        assert_eq!((atlases.len(), atlases[1].name.clone(), named.len()), (2, sp.join("body_lord").to_string_lossy().into_owned(), 2));
        assert_eq!((named[1].0.as_str(), named[1].1.atlas), ("test_lord/body_lord-1", 1));
        // only the lords of the player who has it: any lord skin, frame = image - 1, owner = colour k7
        let names: Vec<String> = ["body_lord-0", "body_lord-1"].iter().map(|s| s.to_string()).chain(named.iter().map(|n| n.0.clone())).collect();
        set_in_game(2, Some(c));
        let gm = crate::replay::Gm::new(&names);
        assert_eq!((gm.lord(100, 2, 2), gm.lord(226, 1, 2), gm.lord(100, 2, 3), gm.lord(44, 2, 2), gm.lord(100, 9, 2)), (Some(3), Some(2), None, None, None));
        set_in_game(2, None);
        assert_eq!(crate::replay::Gm::new(&names).lord(100, 2, 2), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A skirmish with the custom lord as player 2 starts as DE registers it (lord 29 + slot → the engine's lord slot
    /// 2 + 0x1d, its config in that slot with the parent fields set, the Pig after it untouched), ticks, and its speech
    /// goes the custom way ("*2*SK_TAUNT1", the lobby player's CustomMedia wav).
    #[cfg(feature = "live")]
    #[test]
    fn custom_lord_skirmish() {
        let map = crate::skirmish::maps_dir().join("Crater Lake.map");
        if !map.exists() { return; } // needs the game install
        let dir = test_folder("game");
        crate::skirmish::set_test_profile(dir.clone());
        assert_eq!(rescan(), 1);
        let lobby = crate::skirmish::Lobby { ais: vec![(BASE, 0), (2, 0)], seed: 1234, ..crate::skirmish::Lobby::new(map) };
        let mut l = crate::live::Live::from_mem(crate::skirmish::start(&lobby).unwrap());
        let m = l.mem();
        let lords: Vec<i32> = (0..9).map(|p| m.i32(0x188571c34 + p * 4)).collect();
        assert_eq!(&lords[2..4], &[2 + 0x1d, 3], "custom lord as player 2, the Pig as 3");
        let at = shc_sim::ai::F + (2 + 0x1d) * 0x5e4;
        assert_eq!((m.i32(at), m.i32(at + 4), m.i32(at + 0x450), m.i32(at + 8)), (17, 2 + 0x1d, 2 + 0x1d, 4), "the Jewel's opponent_type, speech set / parent = the registered lord 29 + slot + 1, her gfx type");
        assert_eq!(in_game(2).map(|c| c.name), Some("Test Lord".into()));
        assert!(in_game(3).is_none());
        let t0 = l.tick();
        for _ in 0..100 { l.step(); }
        assert_eq!(l.tick() - t0, 100);
        shc_sim::sound::drain();
        shc_sim::hudmsg::f102bc0(l.mem(), 0x181b60e70, 2, 2 + 0x1d, 1); // TAUNT1 as the AI posts it
        let named: Vec<String> = shc_sim::sound::drain().into_iter().filter_map(|s| if let shc_sim::sound::Sound::Named(n) = s { Some(n) } else { None }).collect();
        assert!(named.iter().any(|n| n.contains("general_message1")), "the greeting: {named:?}");
        // the line itself waits in the message box (its head, or queue entry 0: hudmsg::f1023b0) as "*2*SK_TAUNT1", kind -2
        let cstr = |m: &shc_sim::mem::Mem, a: u64| String::from_utf8_lossy(&m.slice(a, 32).iter().copied().take_while(|&c| c != 0).collect::<Vec<u8>>()).into_owned();
        let (o, m) = (0x181b60e70u64, l.mem());
        let queued = [(m.i32(o + 4), cstr(m, o + 0x70)), (m.i32(o + 0x37 * 4), cstr(m, o + 0x14f * 4))];
        assert!(queued.iter().any(|(k, w)| *k == -2 && split(w) == Some((2, "TAUNT1".into()))), "the custom line: {queued:?}");
        set_in_game(2, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
