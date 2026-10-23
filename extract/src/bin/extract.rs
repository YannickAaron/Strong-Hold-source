//! Dev check: `cargo run --release --bin extract -- <step> <game dir> <out dir>` (steps: all, dll, ui, audio, atlases,
//! frontend, video, more, webm <file>, sc2 (game dir = the SC2 install); `all` writes the sounds to <out dir>/../shc-audio).
fn main() -> shc_extract::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (step, game, out) = (&a[1], std::path::Path::new(&a[2]), std::path::Path::new(&a[3]));
    let data = game.join("Stronghold Crusader Definitive Edition_Data");
    let t = std::time::Instant::now();
    match step.as_str() {
        "atlases" => shc_extract::atlases::export(&data, out)?,
        "audio" => { let n = shc_extract::audio::export(&data, out)?; eprintln!("{n} clips failed"); }
        "webm" => {
            let w = shc_extract::webm::parse(&std::fs::read(out)?)?;
            let mut d = shc_extract::vpx::Decoder::vp8()?;
            let (mut n, mut alpha) = (0, 0);
            for (f, a) in &w.frames { d.decode(f)?; n += 1; alpha += a.is_some() as usize; }
            eprintln!("video {}x{} {} frames ({} with alpha), audio {:?} {} packets", w.video.w, w.video.h, n, alpha,
                w.audio.as_ref().map(|a| (a.codec.clone(), a.rate, a.channels, a.private.len())), w.audio_packets.len());
        }
        "video" => shc_extract::video::export(&data, &out.join("../ui/font.ttf"), out, &mut |d, n| eprint!("\rvideo {d}/{n}"))?,
        "more" => shc_extract::video::export_more(&data, out, &mut |d, n| eprint!("\rmore {d}/{n}"))?,
        "frontend" => shc_extract::frontend::export(game, &data, &shc_extract::ui::noesis(&data)?, out)?,
        "dll" => shc_extract::dll::export(&data, out)?,
        "tex" => { // dev: one texture as RGBA -> <out>.rgba (args: tex <game> <out> <name> <files...>)
            let files: Vec<&str> = a[5..].iter().map(String::as_str).collect();
            let im = shc_extract::tex::Textures::open(&data, &files)?.get(&a[4])?;
            eprintln!("{}x{}", im.w, im.h);
            std::fs::write(out, im.px)?;
        }
        "ui" => shc_extract::ui::export(game, &data, out)?,
        "sc2" => shc_extract::sc2::export(game, out)?,
        "sc2png" => { // dev: one clip as a contact sheet <out>.png, 8 facings × counters (args: sc2png <SC2> <out> <clip> <n>)
            let look = shc_extract::sc2::render::DE_LOOK;
            let k = shc_extract::sc2::Knight::load(game, &look)?;
            let n: u32 = a[5].parse()?;
            let dur = k.duration(&a[4])?;
            let fr: Vec<_> = (0..n * 8).map(|i| k.frame(&a[4], dur * (i / 8) as f32 / n as f32, true, i % 8, &look)).collect::<shc_extract::Result<_>>()?;
            let (cw, ch) = (fr.iter().map(|f| f.w).max().unwrap(), fr.iter().map(|f| f.h).max().unwrap());
            let mut im = image::RgbaImage::from_pixel(cw as u32 * n, ch as u32 * 8, image::Rgba([88, 104, 60, 255]));
            for (i, f) in fr.iter().enumerate() {
                let (ox, oy) = ((i as u32 / 8) * cw as u32, (i as u32 % 8) * ch as u32);
                for (j, p) in f.rgba.chunks(4).enumerate() {
                    let (x, y) = (ox + (j % f.w) as u32, oy + (j / f.w) as u32);
                    let a = p[3] as f32 / 255.0;
                    let d = im.get_pixel_mut(x, y);
                    for c in 0..3 { d[c] = (p[c] as f32 * a + d[c] as f32 * (1.0 - a)) as u8; }
                }
            }
            im.save(out)?;
        }
        "all" => shc_extract::run(game, out, &out.join("../shc-audio"), &mut |l, f| eprint!("\r{:3.0}% {l:30}", f * 100.0))?,
        _ => return Err(format!("unknown step {step}").into()),
    }
    eprintln!("{step}: {:.1} s", t.elapsed().as_secs_f32());
    Ok(())
}
