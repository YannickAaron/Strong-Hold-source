//! FRONT_Credits (the main menu's Firefly logo): the game's own credits, read from the user's install like every
//! other asset (nothing ships with the app). The list is FRONT_Credits.xaml's CreditsStack (the game's Noesis XAML,
//! read once and kept as `ui/credits.tsv`); the pictures are Assets/GUI/Images/credits1..6.png and the SH1 logo,
//! decoded in the background. Layout as the XAML (1920 grid): a picture 1042×748 at the left, changing every 10 s
//! with a 1 s fade out / in; the 800 wide column on the right scrolling up at 50 px/s from below the screen, looping;
//! 100 px fades at its top and bottom; Back (B005) bottom left, and a click anywhere leaves (LeaveCredits).
//! Music 15 ("sandalmaker"). ponytail: Space / Down fast scroll not ported.
use super::{pic, quad, Act, Menu, SCREEN, SCREEN_H};
use crate::hud::{HudTri, Ui};
use std::sync::Mutex;
use std::time::Instant;

/// An entry of CreditsStack: a logo (`sh` = the SH1 one) or a 24 px line; `top` = its Margin top; `sh` picks the
/// colours (Brush.TitleSH / BodySH), `title` the title brush; `text` = literal or "@TEXT_KEY".
#[derive(Clone, Debug, PartialEq)]
enum Item { Logo { top: f32, sh: bool }, Text { top: f32, title: bool, sh: bool, text: String } }

/// What the loader has so far: the list, the pictures (1042×748 RGBA) and the SH1 logo (fitted into 604×202).
#[derive(Default)]
struct Data { items: Option<Vec<Item>>, pictures: Vec<Vec<u8>>, sh_logo: Option<(usize, usize, Vec<u8>)>, started: bool }
static DATA: Mutex<Data> = Mutex::new(Data { items: None, pictures: Vec::new(), sh_logo: None, started: false });

const PIC: (usize, usize, usize, usize) = (30, 166, 1042, 748); // x, y, w, h in the 1920 grid
const LINE: f32 = 30.0; // a 24 px Junicode line

#[derive(Clone, Copy)]
pub(super) struct Credits { t0: Instant }

impl Credits {
    /// FRONT_Credits.init: from the top, picture 1, music 15; the data loads in the background once.
    pub fn new() -> Self {
        crate::audio::music("fx\\music\\sandalmaker.raw", 1.0);
        let mut d = DATA.lock().unwrap();
        if !std::mem::replace(&mut d.started, true) { std::thread::spawn(load); }
        Credits { t0: Instant::now() }
    }
    /// The picture shown now (0..6) and its alpha: shown 10 s, 1 s out, the next 1 s in.
    fn picture(&self) -> (usize, f32) {
        let t = self.t0.elapsed().as_secs_f32() + 1.0; // the first one starts fully in
        let p = t % 12.0;
        ((t / 12.0) as usize % 6, if p < 1.0 { p } else if p < 11.0 { 1.0 } else { 12.0 - p })
    }
    /// Menu::screen_image: "credits:<n>" once the pictures are decoded.
    pub fn image(&self) -> Option<String> {
        (DATA.lock().unwrap().pictures.len() == 6).then(|| format!("credits:{}", self.picture().0))
    }
}

/// menu::screen_pixels("credits:<n>"): picture n at its place in the 1920×1080 screen, the SH1 logo below it (rows
/// 1080.., x 0) for the scrolling column.
pub(super) fn pixels(n: usize) -> Option<Vec<u8>> {
    let d = DATA.lock().unwrap();
    let src = d.pictures.get(n)?;
    let mut px = vec![0u8; 1920 * SCREEN_H as usize * 4];
    let (x, y, w, h) = PIC;
    for row in 0..h { px[((y + row) * 1920 + x) * 4..][..w * 4].copy_from_slice(&src[row * w * 4..][..w * 4]); }
    if let Some((lw, lh, l)) = &d.sh_logo {
        for row in 0..*lh { px[((1080 + row) * 1920) * 4..][..lw * 4].copy_from_slice(&l[row * lw * 4..][..lw * 4]); }
    }
    Some(px)
}

/// The loader thread: the list (cache, else the game's XAML), then the pictures.
fn load() {
    let cache = crate::cache_dir().join("ui/credits.tsv");
    let items = std::fs::read_to_string(&cache).ok().map(|s| from_tsv(&s)).filter(|v| !v.is_empty()).or_else(|| {
        let data = crate::paths::game().join(shc_extract::GAME_DATA);
        let xaml = shc_extract::ui::noesis(&data).map_err(|e| eprintln!("credits: {e}")).ok()?;
        let v = parse(&String::from_utf8_lossy(xaml.get("Assets/GUI/XAMLResources/FRONT_Credits.xaml")?));
        let _ = std::fs::write(&cache, to_tsv(&v));
        Some(v)
    });
    DATA.lock().unwrap().items = Some(items.unwrap_or_default());
    let dir = crate::paths::game().join("Assets/GUI/Images");
    let png = |n: &str| shc_extract::tex::Rgba::png(&dir.join(n)).map_err(|e| eprintln!("credits: {n}: {e}")).ok();
    if let Some(l) = png("CrusaderSH_Logo.png") {
        let k = (604.0 / l.w as f32).min(202.0 / l.h as f32); // Stretch Uniform
        let (w, h) = ((l.w as f32 * k) as usize, (l.h as f32 * k) as usize);
        DATA.lock().unwrap().sh_logo = Some((w, h, l.resize(w, h).px));
    }
    let pics: Vec<Vec<u8>> = (1..=6).filter_map(|i| png(&format!("credits{i}.png"))).map(|p| p.resize(PIC.2, PIC.3).px).collect();
    if pics.len() == 6 { DATA.lock().unwrap().pictures = pics; }
}

/// Attribute `key` of the element text `el` (`key="value"`, the key on its own, not a suffix of another).
fn attr<'a>(el: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("{key}=\"");
    let mut from = 0;
    while let Some(i) = el[from..].find(&pat).map(|i| i + from) {
        if el[..i].ends_with(char::is_whitespace) {
            let v = &el[i + pat.len()..];
            return Some(&v[..v.find('"')?]);
        }
        from = i + 1;
    }
    None
}

/// CreditsStack's Images (CreditsDELogo / CreditsSHLogo) and TextBlocks, comments dropped.
fn parse(xaml: &str) -> Vec<Item> {
    let Some(start) = xaml.find("Name=\"CreditsStack\"") else { return vec![] };
    let mut body = &xaml[start..xaml[start..].find("</StackPanel>").map_or(xaml.len(), |e| start + e)];
    let mut clean = String::new();
    while let Some(a) = body.find("<!--") { // XML comments
        clean.push_str(&body[..a]);
        body = body[a..].find("-->").map_or("", |b| &body[a + b + 3..]);
    }
    clean.push_str(body);
    let mut v = vec![];
    for part in clean.split('<').skip(1) {
        let el = &part[..part.find('>').unwrap_or(part.len())];
        let top = attr(el, "Margin").and_then(|m| m.split(',').nth(1)).and_then(|t| t.trim().parse().ok()).unwrap_or(0.0);
        if el.starts_with("Image") {
            v.push(Item::Logo { top, sh: attr(el, "Name") == Some("CreditsSHLogo") });
        } else if el.starts_with("TextBlock") {
            let Some(text) = attr(el, "Text") else { continue };
            let text = match text.split_once("GameTexts[") { Some((_, k)) => format!("@{}", &k[..k.find(']').unwrap_or(k.len())]), None => xml_text(text) };
            let brush = attr(el, "Foreground").unwrap_or("");
            v.push(Item::Text { top, title: brush.contains("Title"), sh: brush.ends_with("SH}"), text });
        }
    }
    v
}

fn xml_text(s: &str) -> String { s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&") }

fn to_tsv(v: &[Item]) -> String {
    v.iter().map(|i| match i {
        Item::Logo { top, sh } => format!("logo\t{top}\t{}\t\n", if *sh { "SH" } else { "DE" }),
        Item::Text { top, title, sh, text } => format!("text\t{top}\t{}{}\t{text}\n", if *title { "T" } else { "B" }, if *sh { "S" } else { "D" }),
    }).collect()
}
fn from_tsv(s: &str) -> Vec<Item> {
    s.lines().filter_map(|l| {
        let c: Vec<&str> = l.splitn(4, '\t').collect();
        let top = c.get(1)?.parse().ok()?;
        let style = *c.get(2)?;
        Some(match c[0] { "logo" => Item::Logo { top, sh: style == "SH" }, _ => Item::Text { top, title: style.starts_with('T'), sh: style.ends_with('S'), text: c.get(3)?.to_string() } })
    }).collect()
}

impl Menu {
    pub(super) fn credits_screen(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, c: &Credits, w: f32, h: f32, cursor: [f32; 2]) {
        // vertex colours are linear here (menu.rs encodes on the way out): #DDDDDD, #EFF3C6, #EF8252, #D6D3A5
        const COLOURS: [[f32; 4]; 4] = [[0.723, 0.723, 0.723, 1.0], [0.863, 0.896, 0.565, 1.0], [0.863, 0.223, 0.084, 1.0], [0.672, 0.651, 0.376, 1.0]];
        Ui::fill(out, [0.0, 0.0, w, h], [0.0, 0.0, 0.0, 1.0]);
        self.hits.push(([0.0, 0.0, w, h], Act::Back)); // LeaveCredits: the whole screen
        let d = DATA.lock().unwrap();
        let th = SCREEN_H as f32;
        if d.pictures.len() == 6 {
            let (x, y, pw, ph) = (PIC.0 as f32, PIC.1 as f32, PIC.2 as f32, PIC.3 as f32);
            let a = c.picture().1;
            quad(out, SCREEN, self.r(x, y, pw, ph), [x / 1920.0, y / th, (x + pw) / 1920.0, (y + ph) / th], [1.0, 1.0, 1.0, a]);
        }
        let cx = 1920.0 - 30.0 - 400.0; // the column's centre
        match &d.items {
            None => { self.label(ui, out, "Loading\u{2026}", cx, 540.0, 24.0, 0.5, COLOURS[1]); }
            Some(items) => {
                // the stack's height, then the scroll (offset += ms / 20, back to 0 at the end)
                let total: f32 = items.iter().map(|i| match i { Item::Logo { top, .. } => top + 202.0, Item::Text { top, .. } => top + LINE }).sum();
                let len = (total - 1080.0).max(1.0);
                let off = (c.t0.elapsed().as_millis() as f32 / 20.0) % (len - 5.0).max(1.0);
                let mut y = -off;
                for i in items {
                    match i {
                        Item::Logo { top, sh } => {
                            y += top;
                            if y < 1080.0 && y + 202.0 > 0.0 {
                                if *sh {
                                    if let Some((lw, lh, _)) = d.sh_logo.as_ref().filter(|_| d.pictures.len() == 6) {
                                        let (lw, lh) = (*lw as f32, *lh as f32);
                                        quad(out, SCREEN, self.r(cx - lw / 2.0, y + (202.0 - lh) / 2.0, lw, lh), [0.0, 1080.0 / th, lw / 1920.0, (1080.0 + lh) / th], [1.0; 4]);
                                    }
                                } else if let Some(&[_, _, lw, lh]) = super::art().2.get("logo") {
                                    let k = (604.0 / lw).min(202.0 / lh);
                                    pic(out, "logo", self.r(cx - lw * k / 2.0, y + (202.0 - lh * k) / 2.0, lw * k, lh * k), 1.0);
                                }
                            }
                            y += 202.0;
                        }
                        Item::Text { top, title, sh, text } => {
                            y += top;
                            if y < 1080.0 && y + LINE > 0.0 {
                                let s = match text.strip_prefix('@') { Some(k) => ui.text_of(k), None => text.clone() };
                                self.label(ui, out, &s, cx, y + LINE / 2.0, 24.0, 0.5, COLOURS[*sh as usize * 2 + !*title as usize]);
                            }
                            y += LINE;
                        }
                    }
                }
            }
        }
        drop(d);
        // the column's top and bottom fades (#000 -> transparent over 100 px)
        for k in 0..20 {
            let a = 1.0 - k as f32 / 20.0;
            Ui::fill(out, self.r(1090.0, k as f32 * 5.0, 800.0, 5.0), [0.0, 0.0, 0.0, a]);
            Ui::fill(out, self.r(1090.0, 1075.0 - k as f32 * 5.0, 800.0, 5.0), [0.0, 0.0, 0.0, a]);
        }
        let back = self.r(60.0, 984.0, 151.0, 66.0);
        ui.img_fit(out, if Self::inside(back, cursor) { "UI-Buttons B006" } else { "UI-Buttons B005" }, back, [1.0; 4]);
    }
}

#[test]
fn credits_xaml() {
    let x = r#"<Grid><ScrollViewer><StackPanel
                Name="CreditsStack">
                <Image Name="CreditsDELogo" Margin="0,1080,0,0" Width="604" Height="202"/>
                <!--  <TextBlock Text="hidden" Foreground="{StaticResource Brush.BodyDE}" /> -->
                <TextBlock Margin="0,20,0,0" Text="{Binding GameTexts[TEXT_CREDITS_068], Source={x:Static local:Translate.Instance}}" Foreground="{StaticResource Brush.TitleDE}" />
                <TextBlock Margin="0,0,0,0" Text="Paul &amp; Harris" TextAlignment="Center" Foreground="{StaticResource Brush.BodyDE}"/>
                <Image Name="CreditsSHLogo" Margin="0,100,0,0" Width="604" Height="202"/>
                <TextBlock Margin="0,30,0,0" Text="Simon Bradbury" Foreground="{StaticResource Brush.BodySH}"/>
            </StackPanel></ScrollViewer><TextBlock Text="outside"/></Grid>"#;
    let v = parse(x);
    assert_eq!(v, vec![
        Item::Logo { top: 1080.0, sh: false },
        Item::Text { top: 20.0, title: true, sh: false, text: "@TEXT_CREDITS_068".into() },
        Item::Text { top: 0.0, title: false, sh: false, text: "Paul & Harris".into() },
        Item::Logo { top: 100.0, sh: true },
        Item::Text { top: 30.0, title: false, sh: true, text: "Simon Bradbury".into() },
    ]);
    assert_eq!(from_tsv(&to_tsv(&v)), v);
}
