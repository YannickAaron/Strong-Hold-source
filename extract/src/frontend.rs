//! = re/scripts/export_frontend.py + export_trail.py: the main-menu art sheet (frontend.rgba/.tsv) and the
//! full-screen images (screens/*.rgba). Formats: those docstrings.
use crate::tex::{Rgba, Textures};
use crate::{video, Result};
use regex::Regex;
use std::{collections::HashMap, fmt::Write as _, fs, path::Path};

/// Python's round(): half to even.
fn pyround(x: f64) -> usize {
    let r = x.round();
    (if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 { r - x.signum() } else { r }) as usize
}

fn first_frame(path: &Path) -> Result<Rgba> { video::frames(path)?.0.next().ok_or("no frames")? }
fn last_frame(path: &Path) -> Result<Rgba> { video::frames(path)?.0.last_rgba() }

pub fn export(game: &Path, data: &Path, xaml: &HashMap<String, Vec<u8>>, out: &Path) -> Result<()> {
    fs::create_dir_all(out.join("screens"))?;
    let tex = Textures::open(data, &["globalgamemanagers.assets", "sharedassets0.assets", "sharedassets1.assets", "resources.assets"])?;
    let vids = data.join("Assets/GUI/Video");
    let half = |im: Rgba| im.resize(im.w / 2, im.h / 2);
    let mut pieces: Vec<(String, Rgba)> = vec![];
    // the looping main-menu video's first frame (the static MenusBackground if it can't be decoded)
    pieces.push(("backdrop".into(), first_frame(&vids.join("front_end_background.webm")).or_else(|e| {
        eprintln!("frontend: backdrop video: {e}; using MenusBackground");
        tex.get("MenusBackground")
    })?));
    // end-of-game banners: last frame of Banner-<Faction>-<Result>.webm, half size
    for fac in ["Crusader", "Arab", "Bedouin"] {
        for res in ["Victory", "Defeat"] {
            match last_frame(&vids.join(format!("Banner-{fac}-{res}.webm"))) {
                Ok(im) => pieces.push((format!("banner_{}_{}", fac.to_lowercase(), res.to_lowercase()), half(im))),
                Err(e) => eprintln!("frontend: banner {fac} {res}: {e}"),
            }
        }
    }
    pieces.push(("lobby".into(), tex.get("victory_military_small")?));
    pieces.push(("table".into(), tex.get("table")?));
    for n in 1..=8 {
        for c in ["", "b", "c", "d"] { pieces.push((format!("tslice{n}{c}"), half(tex.get(&format!("tslice{n}{c}"))?))); }
    }
    let logo = image::load_from_memory_with_format(&fs::read(game.join("Assets/GUI/Images/CrusaderDE_Logo.png"))?, image::ImageFormat::Png)?.to_rgba8();
    pieces.push(("logo".into(), Rgba { w: logo.width() as usize, h: logo.height() as usize, px: logo.into_raw() }));
    let trail = tex.get("UI-TrailAtlas")?;
    let tx = String::from_utf8_lossy(xaml.get("Assets/GUI/Sprites/UI-TrailAtlas.xaml").ok_or("UI-TrailAtlas.xaml not found")?).into_owned();
    let rects: HashMap<String, [usize; 4]> = Regex::new(r#"x:Key="([^"]+)" Source="\{StaticResource UI-TrailAtlas\}" SourceRect="([^"]+)""#)?
        .captures_iter(&tx).filter_map(|c| {
            let v: Vec<usize> = c[2].split(',').filter_map(|s| s.trim().parse().ok()).collect();
            Some((c[1].to_string(), v.try_into().ok()?))
        }).collect();
    let crop = |k: &str| -> Result<Rgba> { let [x, y, w, h] = *rects.get(k).ok_or_else(|| format!("trail rect {k}"))?; Ok(trail.crop(x, y, w, h)) };
    for k in ["btn_shield_bar_01", "btn_shield_bar_01_rollover", "btn_shield_bar_01_gray", "btn_exit_solo", "btn_exit_rollover_solo",
              "btn_shield_bar_01_yellow", "btn_shield_bar_01_rollover_yellow", "Wood_C", "Wood_T", "Wood_B", "Wood_L", "Wood_R", "Wood_BL"] {
        pieces.push((k.into(), crop(k)?));
    }
    // Crusader Trail art at its 1920-grid display size (FRONT_Trail.xaml / FrontendMenus.cs GenerateSwords)
    let groups: [(Vec<usize>, (Option<usize>, usize)); 5] = [((16..32).collect(), (None, 120)), ((44..56).collect(), (None, 100)),
        ((190..233).filter(|&n| n != 216).collect(), (Some(100), 100)), ((93..143).collect(), (Some(340), 210)), (vec![143, 175], (Some(506), 374))];
    for (keys, (tw, th)) in groups {
        for n in keys {
            let k = format!("skirmish_trail_icons-{n}");
            let im = crop(&k)?;
            let tw = tw.unwrap_or_else(|| pyround(im.w as f64 * th as f64 / im.h as f64));
            pieces.push((k, im.resize(tw, th)));
        }
    }
    for k in ["btn_text_bar_01", "btn_text_bar_02", "btn_text_bar_01_yellow", "btn_text_bar_02_yellow"] {
        let im = crop(k)?;
        let h = pyround(im.h as f64 * 720.0 / im.w as f64);
        pieces.push((k.into(), im.resize(720, h)));
    }
    for (k, w) in [("Trail_Coop3", 500), ("Trail_Coop4", 500), ("Trail_Sands7", 500), ("skirmish_trail_coop_character", 340)] {
        let im = crop(k)?;
        let h = pyround(im.h as f64 * w as f64 / im.w as f64);
        pieces.push((k.into(), im.resize(w, h)));
    }
    // shelf packing, two backdrops per row (keeps the sheet under the 8192 px texture limit)
    let width = 2 * pieces[0].1.w;
    let (mut placed, mut x, mut y, mut shelf) = (vec![], 0, 0, 0);
    for (k, im) in &pieces {
        if x + im.w > width { x = 0; y += shelf; shelf = 0; }
        placed.push((k, x, y, im));
        x += im.w;
        shelf = shelf.max(im.h);
    }
    let mut sheet = Rgba::new(width, y + shelf);
    let mut tsv = format!("size\t{width}\t{}\n", sheet.h);
    for (k, x, y, im) in placed {
        sheet.paste(im, x, y);
        writeln!(tsv, "{k}\t{x}\t{y}\t{}\t{}", im.w, im.h)?;
    }
    fs::write(out.join("frontend.rgba"), &sheet.px)?;
    fs::write(out.join("frontend.tsv"), tsv)?;
    // full-screen images (1920x1080) and the FRONT_Story pages (694 / 561 wide)
    let screen = |name: &str, im: Rgba| fs::write(out.join("screens").join(format!("{name}.rgba")), im.px);
    let mut names = vec!["HistoricalCampaignMenu-Background".to_string(), "BriefingBackground".into(), "HistoricalCampaign-Background".into()];
    names.extend((1..8).map(|c| format!("HistoricalCampaign-Campaign0{c}")));
    for n in &names { screen(n, tex.get(n)?.resize(1920, 1080))?; }
    for k in 1..50 {
        for (kind, w) in [("Drawing", 694), ("Map", 561)] {
            let im = tex.get(&format!("HistoricalCampaign-{kind}-{k:02}"))?;
            let h = pyround(im.h as f64 * w as f64 / im.w as f64);
            screen(&format!("story-{}-{k}", kind.to_lowercase()), im.resize(w, h))?;
        }
    }
    // Crusader Trail screens (export_trail.py): sharedassets1 only
    let sa1 = Textures::open(data, &["sharedassets1.assets"])?;
    let mut trail_names = vec![("trail-bg".to_string(), "SkirmishTrail-Background".to_string()), ("trail-sands-bg".into(), "SkirmishTrail-Sands-Background".into()),
        ("masters-bg".into(), "SkirmishMasters".into()),
        ("trail-map-0".into(), "SkirmishTrailMap".into()), ("trail-map-1".into(), "SkirmishTrailMap2".into()), ("trail-map-2".into(), "SkirmishTrailMap3".into())];
    trail_names.extend((1..8).map(|n| (format!("trail-map-{}", 10 + n), format!("SkirmishSands{n}Map"))));
    trail_names.extend((1..4).map(|n| (format!("trail-map-{}", 20 + n), format!("SkirmishCoop{n}Map"))));
    for (key, t) in trail_names { screen(&key, sa1.get(&t)?.resize(1920, 1080))?; }
    Ok(())
}

#[test]
fn round_half_even() { assert_eq!([0.5, 1.5, 2.5, 2.4, 2.6, 7.0].map(pyround), [0, 2, 2, 2, 3, 7]); }
