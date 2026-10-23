//! Dev check: `cargo run --release --bin extract -- <step> <game dir> <out dir>` (steps: all, dll, ui, audio, atlases,
//! frontend, video, more, webm <file>; `all` writes the sounds to <out dir>/../shc-audio).
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
        "all" => shc_extract::run(game, out, &out.join("../shc-audio"), &mut |l, f| eprint!("\r{:3.0}% {l:30}", f * 100.0))?,
        _ => return Err(format!("unknown step {step}").into()),
    }
    eprintln!("{step}: {:.1} s", t.elapsed().as_secs_f32());
    Ok(())
}
