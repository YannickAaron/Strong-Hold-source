//! In-game Help (HUD_Help.xaml / HUD_Help.cs), a child module of menu.rs (its hit list and drawing helpers). The
//! original shows the game's own pages, StreamingAssets/Help/*.html (help_main.html = the contents, a page per
//! building / unit / concept, missionN.html), in a web view (Vuplex). Here their small HTML subset is parsed into
//! blocks and drawn natively in the same 820×500 panel: titles, headings, paragraphs with links, the pictures.
//! Back / Home / Exit like HUD_Help (Back and Home only once a link was followed). The pages are read straight from
//! the game folder, like the maps (the setup's Steam download keeps StreamingAssets/Help, steam_files.txt).
//! Pictures go into HUD texture `SCREEN` (`screen_image` "help:<page>"), packed left to right from its top.
use super::*;

/// HUD_Help.in_building_help: the page per eStructs kind (in_structure_type); None = no help button.
const BUILDING_HELP: [Option<&str>; 109] = {
    const N: Option<&str> = None;
    [N, Some("st02_house"), Some("st02_house"), Some("st03_woodcutters_hut"), Some("st04_oxen_base"), Some("st05_iron_mine"), Some("st06_pitch_digger"),
    Some("st07_hunters_hut"), Some("st08_mercenary_post"), Some("st08_barracks"), Some("st10_goods_yard"), Some("st11_armoury"), Some("st12_fletchers_workshop"),
    Some("st13_blacksmiths_workshop"), Some("st14_poleturners_workshop"), Some("st15_armourers_workshop"), Some("st16_tanners_workshop"), Some("st17_bakers_workshop"),
    Some("st18_brewers_workshop"), Some("st19_granary"), Some("st20_quarry"), Some("st21_quarrypile"), Some("st22_inn"), Some("st23_healer"), Some("st24_engineers_guild"),
    Some("st25_tunnellers_guild"), Some("st26_tradepost"), Some("st27_well"), Some("st28_oil_smelter"), N, Some("st30_wheatfarm"), Some("st31_hopsfarm"),
    Some("st32_applefarm"), Some("st33_cattlefarm"), Some("st34_mill"), Some("st35_stables"), Some("st36_church"), Some("st36_church"), Some("st36_church"), N,
    Some("st40_keep"), Some("st40_keep"), Some("st40_keep"), Some("st40_keep"), Some("st40_keep"), Some("st60_gatehouse"), Some("st60_gatehouse"), Some("st60_gatehouse"),
    Some("st60_gatehouse"), Some("st49_drawbridge"), Some("st50_tunnel_entrance"), Some("st28_oil_smelter"), Some("st52_signpost"), Some("st24_engineers_guild"),
    Some("st80_siege_tent"), Some("st55_campground"), Some("st09_barracks"), Some("st09_barracks"), Some("st09_barracks"), Some("st25_tunnellers_guild"),
    Some("st60_gatehouse"), Some("st61_tower"), Some("st62_bad_things"), Some("st62_bad_things"), Some("st62_bad_things"), Some("st65_good_things"), Some("st65_good_things"),
    Some("st67_killing_pit"), Some("st68_pitch_ditch"), N, Some("st70_water_pot"), Some("st71_keepdoor_left"), Some("st72_keepdoor_right"), Some("st73_keepdoor"),
    Some("st74_tower1"), Some("st75_tower2"), Some("st76_tower3"), Some("st77_tower4"), Some("st78_tower5"), N, Some("st80_siege_tent"), Some("st80_siege_tent"),
    Some("st80_siege_tent"), Some("st80_siege_tent"), Some("st80_siege_tent"), Some("st50_tunnel_entrance"), N, N, N, N, N, Some("st62_bad_things"), Some("st62_bad_things"),
    Some("st62_bad_things"), Some("st62_bad_things"), Some("st62_bad_things"), Some("st62_bad_things"), Some("st62_bad_things"), Some("st62_bad_things"), Some("st99_dog_cage"),
    Some("st65_good_things"), Some("st65_good_things"), Some("st65_good_things"), Some("st65_good_things"), Some("st65_good_things"), N, N, N, Some("st08_bedouin_stockade")]
};

fn dir() -> PathBuf { crate::paths::streaming_assets().join("Help") }

/// HUD_Help.getBuildingHelpURL / doesBuildingHelpExist: the help page of building kind `t` (file name), if the game
/// has it. ponytail: the Arab lords' church -> st100_mosque.html swap is not made (the HUD state has no lord type).
pub fn building_page(t: i32) -> Option<String> {
    static HAVE: OnceLock<Vec<bool>> = OnceLock::new();
    let have = HAVE.get_or_init(|| BUILDING_HELP.iter().map(|p| p.is_some_and(|p| dir().join(format!("{p}.html")).is_file())).collect());
    let t = usize::try_from(t).ok().filter(|&t| t < BUILDING_HELP.len() && have[t])?;
    Some(format!("{}.html", BUILDING_HELP[t]?))
}

/// Help clicks (menu.rs `Act::Help`): `Link(k)` follows the page's k-th href.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum A { Open, Link(u16), Back, Home, Exit }

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Style { Title, Heading, Plain }

/// A page's blocks: a centred line (`<font size=7>` title, `<h4>` heading, `<center>` text), a paragraph (text runs,
/// '\n' = `<br>`, each with its link: an index into `Page::links`), a picture (`logo` = the contents page's banner,
/// in the flow; else beside the paragraph), a `<br>` between blocks, a `clearfix` (below the picture).
#[derive(Debug, PartialEq)]
pub enum Block { Line(String, Style), Para(Vec<(String, Option<u16>)>), Image(String, bool), Gap, Clear }

#[derive(Debug, Default)]
pub struct Page { pub blocks: Vec<Block>, pub links: Vec<String> }

/// `name="v"` (or unquoted) in tag text, %20 decoded.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let i = lower.find(&format!("{name}="))? + name.len() + 1;
    let v = &tag[i..];
    let v = match v.strip_prefix('"') { Some(q) => q.split('"').next()?, None => v.split([' ', '/']).next()? };
    Some(v.replace("%20", " "))
}

fn entities(s: &str) -> String { s.replace("&nbsp;", " ").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&") }

/// The help pages' HTML subset (see `Block`) into blocks; other tags are dropped.
pub fn parse(html: &str) -> Page {
    let mut page = Page::default();
    let (mut line, mut para, mut style, mut logo, mut link) = (String::new(), None::<Vec<(String, Option<u16>)>>, Style::Plain, false, None::<u16>);
    fn flush(out: &mut Vec<Block>, line: &mut String, style: Style) {
        let t = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !t.is_empty() { out.push(Block::Line(t, style)); }
        line.clear();
    }
    for (i, part) in html.split('<').enumerate() {
        let (tag, text) = if i == 0 { ("", part) } else { part.split_once('>').unwrap_or((part, "")) };
        let lower = tag.to_ascii_lowercase();
        let name = lower.split_whitespace().next().unwrap_or("").trim_end_matches('/');
        let b = &mut page.blocks;
        match name {
            "p" => { flush(b, &mut line, style); para = Some(vec![]); }
            "/p" => if let Some(p) = para.take() { b.push(Block::Para(p)) },
            "br" => match &mut para { Some(p) => p.push(("\n".into(), link)), None => { flush(b, &mut line, style); b.push(Block::Gap) } },
            "h4" => { flush(b, &mut line, style); style = Style::Heading; }
            "font" if lower.contains("size=7") => { flush(b, &mut line, style); style = Style::Title; }
            "/h4" | "/font" if style != Style::Plain => { flush(b, &mut line, style); style = Style::Plain; }
            "center" | "/center" => flush(b, &mut line, style),
            "img" => { flush(b, &mut line, style); if let Some(src) = attr(tag, "src") { b.push(Block::Image(src, logo)) } }
            "div" if lower.contains("clearfix") => { flush(b, &mut line, style); b.push(Block::Clear) }
            "div" => logo = lower.contains("logo"),
            "/div" => logo = false,
            "a" => link = attr(tag, "href").map(|h| { page.links.push(h); page.links.len() as u16 - 1 }),
            "/a" => link = None,
            _ => {}
        }
        let text = entities(text);
        match &mut para { Some(p) if !text.is_empty() => p.push((text, link)), Some(_) => {} None => line.push_str(&text) }
    }
    flush(&mut page.blocks, &mut line, style);
    if let Some(p) = para { page.blocks.push(Block::Para(p)); }
    page
}

/// The page's pictures (unique, in order).
fn pictures(page: &Page) -> Vec<&str> {
    let mut v: Vec<&str> = vec![];
    for b in &page.blocks { if let Block::Image(s, _) = b { if !v.contains(&s.as_str()) { v.push(s) } } }
    v
}

/// PNG size from its IHDR.
fn png_size(path: &std::path::Path) -> Option<(u32, u32)> {
    let mut head = [0u8; 24];
    std::io::Read::read_exact(&mut std::fs::File::open(path).ok()?, &mut head).ok()?;
    (&head[1..4] == b"PNG").then(|| (u32::from_be_bytes(head[16..20].try_into().unwrap()), u32::from_be_bytes(head[20..24].try_into().unwrap())))
}

/// Shelf packing into the 1920 × SCREEN_H texture: each picture's top-left (None = doesn't fit).
fn pack(dims: &[Option<(u32, u32)>]) -> Vec<Option<[u32; 2]>> {
    let (mut x, mut y, mut row) = (0, 0, 0);
    dims.iter().map(|d| {
        let (w, h) = (*d)?;
        if x + w > 1920 { (x, y, row) = (0, y + row, 0); }
        if w > 1920 || y + h > SCREEN_H { return None; }
        let at = [x, y];
        (x, row) = (x + w, row.max(h));
        Some(at)
    }).collect()
}

/// `screen_pixels("help:<page>")`: the page's pictures packed as `Help` expects them (1920-wide RGBA rows).
pub fn pixels(page: &str) -> Option<Vec<u8>> {
    let d = dir();
    let p = parse(&String::from_utf8_lossy(&std::fs::read(d.join(page)).ok()?));
    let imgs: Vec<Option<image::RgbaImage>> = pictures(&p).iter().map(|s| {
        image::load_from_memory_with_format(&std::fs::read(d.join(s)).ok()?, image::ImageFormat::Png).ok().map(|i| i.to_rgba8())
    }).collect();
    let at = pack(&imgs.iter().map(|i| i.as_ref().map(|i| i.dimensions())).collect::<Vec<_>>());
    let rows = imgs.iter().zip(&at).filter_map(|(i, a)| Some(a.as_ref()?[1] + i.as_ref()?.height())).max().unwrap_or(1);
    let mut px = vec![0u8; 1920 * rows as usize * 4];
    for (img, a) in imgs.iter().zip(&at) {
        let (Some(img), Some([x, y])) = (img, a) else { continue };
        for (r, line) in img.as_raw().chunks_exact(img.width() as usize * 4).enumerate() {
            let o = ((*y as usize + r) * 1920 + *x as usize) * 4;
            px[o..o + line.len()].copy_from_slice(line);
        }
    }
    Some(px)
}

/// The open help: `page` (file name) parsed, its pictures' uv rects and size in `SCREEN`, the pages Back returns to
/// (with their scroll), `home` = the page it opened on; `scroll` / `height` of the page in design px;
/// `from_menu` = opened from the in-game menu (Exit returns to it), else from a building panel (Exit closes).
pub struct Help { page: String, doc: Page, pics: HashMap<String, ([f32; 4], f32, f32)>, back: Vec<(String, f32)>, home: String, scroll: f32, height: f32, from_menu: bool }

impl Help {
    pub fn open(page: &str, from_menu: bool) -> Help {
        let mut h = Help { page: String::new(), doc: Page::default(), pics: HashMap::new(), back: vec![], home: page.into(), scroll: 0.0, height: 0.0, from_menu };
        h.load(page);
        h
    }
    fn load(&mut self, page: &str) {
        let d = dir();
        self.doc = match std::fs::read(d.join(page)) {
            Ok(b) => parse(&String::from_utf8_lossy(&b)),
            Err(_) => Page { blocks: vec![Block::Line(format!("Help page not found: {}", d.join(page).display()), Style::Plain)], links: vec![] },
        };
        let srcs: Vec<String> = pictures(&self.doc).into_iter().map(String::from).collect();
        let dims: Vec<_> = srcs.iter().map(|s| png_size(&d.join(s))).collect();
        let (tw, th) = (1920.0, SCREEN_H as f32);
        self.pics = srcs.into_iter().zip(dims.iter().zip(pack(&dims))).filter_map(|(s, (d, a))| {
            let ((w, h), [x, y]) = ((*d)?, a?);
            let (w, h, x, y) = (w as f32, h as f32, x as f32, y as f32);
            Some((s, ([x / tw, y / th, (x + w) / tw, (y + h) / th], w, h)))
        }).collect();
        (self.page, self.scroll, self.height) = (page.into(), 0.0, 0.0);
    }
    /// The image `Menu::screen_image` names for this page.
    pub fn image(&self) -> String { format!("help:{}", self.page) }
    pub fn wheel(&mut self, lines: f32) { self.scroll = (self.scroll - lines * 40.0).clamp(0.0, (self.height - BOX[3]).max(0.0)); }
}

/// The page box (MainHelpTexture 800×450) in the 1920×1080 grid: x, y, w, h. The panel is 820×500, centred.
const BOX: [f32; 4] = [560.0, 300.0, 800.0, 450.0];
const PAGE_CREAM: [f32; 4] = [1.0, 0.955, 0.604, 1.0]; // #FFFACC
const GOLDENROD: [f32; 4] = [0.701, 0.376, 0.017, 1.0]; // #DAA520
const LINK: [f32; 4] = [0.231, 0.624, 0.258, 1.0]; // #84CF8B
const LINK_HOT: [f32; 4] = [0.6, 0.9, 0.6, 1.0];

impl Menu {
    /// Menu::help: the Help panel on `page`.
    pub fn help(page: &str, from_menu: bool, can_restart: bool) -> Menu {
        Menu { screen: Screen::Help(Box::new(Help::open(page, from_menu))), in_game: true, can_restart, ..Menu::main() }
    }

    pub(super) fn help_click(&mut self, a: A) -> Option<Go> {
        if a == A::Open { *self = Menu::help("help_main.html", true, self.can_restart); return None; }
        let Screen::Help(h) = &mut self.screen else { return None };
        match a {
            A::Link(k) => if let Some(to) = h.doc.links.get(k as usize).cloned() {
                h.back.push((h.page.clone(), h.scroll));
                h.load(&to);
            },
            A::Back => if let Some((p, s)) = h.back.pop() { h.load(&p); h.scroll = s; },
            A::Home => { let home = h.home.clone(); h.back.push((h.page.clone(), h.scroll)); h.load(&home); }
            A::Exit | A::Open => return self.help_close(),
        }
        None
    }

    /// HUD_Help.Close: back to the in-game menu when opened from it, else to the game.
    pub(super) fn help_close(&mut self) -> Option<Go> {
        match &self.screen {
            Screen::Help(h) if h.from_menu => { *self = Menu::pause(self.can_restart); None }
            _ => Some(Go::Resume),
        }
    }

    /// Words of `runs` wrapped to `width` design px at `size`: lines of (word, link, x, width).
    fn help_lines(&self, ui: &mut Ui, runs: &[(String, Option<u16>)], size: f32, width: f32) -> Vec<Vec<(String, Option<u16>, f32, f32)>> {
        let w = |ui: &mut Ui, t: &str| ui.text(&mut vec![], t, 0.0, 0.0, size, 1.0, 0.0, 0.0, [1.0; 4]) / self.s;
        let space = w(ui, " ");
        let (mut lines, mut cur, mut x) = (vec![], vec![], 0.0);
        let (mut word, mut gap, mut wlink) = (String::new(), false, None);
        let put = |ui: &mut Ui, word: &mut String, gap: bool, link: Option<u16>, lines: &mut Vec<_>, cur: &mut Vec<_>, x: &mut f32| {
            if word.is_empty() { return; }
            let ww = w(ui, word);
            let sp = if gap && *x > 0.0 { space } else { 0.0 };
            if *x > 0.0 && *x + sp + ww > width { lines.push(std::mem::take(cur)); *x = 0.0; }
            let sp = if *x > 0.0 { sp } else { 0.0 };
            cur.push((std::mem::take(word), link, *x + sp, ww));
            *x += sp + ww;
        };
        for (text, link) in runs {
            // a word cut by a link's edge: two words, no space between
            if !word.is_empty() && wlink != *link { put(ui, &mut word, gap, wlink, &mut lines, &mut cur, &mut x); gap = false; }
            for c in text.chars() {
                if c == '\n' || c.is_whitespace() {
                    put(ui, &mut word, gap, wlink, &mut lines, &mut cur, &mut x);
                    gap = true;
                    if c == '\n' { lines.push(std::mem::take(&mut cur)); (x, gap) = (0.0, false); }
                } else {
                    if word.is_empty() { wlink = *link; }
                    word.push(c);
                }
            }
        }
        put(ui, &mut word, gap, wlink, &mut lines, &mut cur, &mut x);
        if !cur.is_empty() { lines.push(cur); }
        lines
    }

    /// HUD_Help: the 820×500 panel (#44000000, BDR_SH1DEc), the page in its 800×450 box (body #80000000; titles
    /// 24 px #DAA520, h4 24 px, text 16 px #FFFACC, links #84CF8B, pictures at half size left of the 600 px wide
    /// paragraphs, like the web view's 2× page), wheel-scrolled; the 30 px bar with Back, Home, Exit (BTN_SH_GlowS).
    pub(super) fn help_draw(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, h: &mut Help, cursor: [f32; 2]) {
        let [bx, by, bw, bh] = BOX;
        let (px, py) = (bx - 10.0, by - 10.0);
        Ui::fill(out, self.r(px, py, 820.0, 500.0), [0.0, 0.0, 0.0, 0.5]); // #44000000 (linear blending)
        self.frame(out, px + 5.0, py + 5.0, 810.0, 490.0, false);
        Ui::fill(out, self.r(bx, by, bw, bh), [0.0, 0.0, 0.0, 0.9]); // the page's #80000000 is 0.78 in linear blending; ours darker, readable over the map
        let clip = [by, by + bh];
        let top = by - h.scroll;
        let (mut y, mut pic_bottom) = (top, top);
        for b in &h.doc.blocks {
            match b {
                Block::Gap => y += 18.0,
                Block::Clear => y = y.max(pic_bottom),
                Block::Line(t, st) => {
                    let (size, color) = match st { Style::Title => (24.0, GOLDENROD), Style::Heading => (24.0, PAGE_CREAM), Style::Plain => (20.0, PAGE_CREAM) };
                    for line in self.help_lines(ui, &[(t.clone(), None)], size, bw - 20.0) {
                        let lw = line.last().map_or(0.0, |l| l.2 + l.3);
                        if y >= clip[0] && y + size * 1.25 <= clip[1] {
                            for (word, _, x, _) in &line { self.label(ui, out, word, bx + (bw - lw) / 2.0 + x, y + size * 0.62, size, 0.0, color); }
                        }
                        y += size * 1.25;
                    }
                    if *st == Style::Heading { y += 5.0; }
                }
                Block::Para(runs) => {
                    let (size, x0) = (16.0, bx + bw - 5.0 - 600.0); // float right, 75 %, margin 14 / 10 at 2×
                    y += 7.0;
                    for line in self.help_lines(ui, runs, size, 600.0) {
                        if y >= clip[0] && y + size * 1.25 <= clip[1] {
                            for (word, link, x, ww) in &line {
                                let r = self.r(x0 + x, y, *ww, size * 1.25);
                                let hot = link.is_some() && Self::inside(r, cursor);
                                let color = if link.is_none() { PAGE_CREAM } else if hot { LINK_HOT } else { LINK };
                                self.label(ui, out, word, x0 + x, y + size * 0.62, size, 0.0, color);
                                if let Some(k) = link { self.hits.push((r, Act::Help(A::Link(*k)))); }
                            }
                        }
                        y += size * 1.25;
                    }
                    y += 7.0;
                }
                Block::Image(src, logo) => {
                    let Some(&(uv, pw, ph)) = h.pics.get(src) else { continue };
                    // the banner: 49 % wide, centred, in the flow; else natural size (at 2×) in a 20 % column, top
                    // centred (object-fit none), beside the paragraph
                    let (w, ih, x, iy) = if *logo { let w = bw * 0.49; (w, w * ph / pw, bx + (bw - w) / 2.0, y) } else {
                        let k = (160.0 / pw).min(0.5);
                        (pw * k, ph * k, bx + 2.5 + (160.0 - pw * k) / 2.0, y + 5.0)
                    };
                    let (t, b) = (iy.max(clip[0]), (iy + ih).min(clip[1]));
                    if b > t {
                        let f = |v: f32| uv[1] + (uv[3] - uv[1]) * (v - iy) / ih;
                        quad(out, SCREEN, self.r(x, t, w, b - t), [uv[0], f(t), uv[2], f(b)], [1.0; 4]);
                    }
                    if *logo { y += ih; } else { pic_bottom = pic_bottom.max(iy + ih + 5.0); }
                }
            }
        }
        h.height = y.max(pic_bottom) - top + 10.0;
        h.scroll = h.scroll.min((h.height - bh).max(0.0));
        // the web view's scroll bar (14 px at 2×): track and thumb
        if h.height > bh {
            Ui::fill(out, self.r(bx + bw - 7.0, by, 7.0, bh), [0.0, 0.0, 0.0, 1.0]);
            let th = (bh * bh / h.height).max(20.0);
            Ui::fill(out, self.r(bx + bw - 7.0, by + (bh - th) * h.scroll / (h.height - bh), 7.0, th), [0.246, 0.246, 0.246, 1.0]); // #888
        }
        Ui::fill(out, self.r(px, py + 460.0, 820.0, 30.0), [0.0, 0.0, 0.0, 0.8]); // #88000000
        let by = py + 460.5;
        if !h.back.is_empty() {
            self.glow(ui, out, &ui.text_of("TEXT_GAME_OPTIONS_018"), px + 20.0, by, 220.0, None, Some(Act::Help(A::Back)), cursor);
            self.glow(ui, out, &ui.text_of("TEXT_NEW_TEXT2_167"), px + 300.0, by, 220.0, None, Some(Act::Help(A::Home)), cursor);
        }
        self.glow(ui, out, &ui.text_of("TEXT_HELP_003"), px + 580.0, by, 220.0, None, Some(Act::Help(A::Exit)), cursor);
    }
}

#[test]
fn parse_help_page() {
    let html = r#"<html><head><link href="style/help.css" rel="stylesheet"></head><body><font size=7 color=#DAA520><center>Barracks</center></font><font size=3 color=#DAA520><img src="Images/ST08_Barracks.png" align="left" /></font><p>You need <a href="contents08_military_units.html">troops.</a>  Then &amp; more<br>next</p><div class="clearfix"></div><br><h4><center>Head</center></h4><a href="unit%20stances.html">x</a></body></html>"#;
    let p = parse(html);
    assert_eq!(p.links, ["contents08_military_units.html", "unit stances.html"]);
    assert_eq!(p.blocks[0], Block::Line("Barracks".into(), Style::Title));
    assert_eq!(p.blocks[1], Block::Image("Images/ST08_Barracks.png".into(), false));
    let Block::Para(runs) = &p.blocks[2] else { panic!("{:?}", p.blocks[2]) };
    assert_eq!(runs.iter().map(|r| r.0.as_str()).collect::<String>(), "You need troops.  Then & more\nnext");
    assert_eq!(runs[1], ("troops.".into(), Some(0)));
    assert_eq!(&p.blocks[3..6], &[Block::Clear, Block::Gap, Block::Line("Head".into(), Style::Heading)]);
    assert_eq!(p.blocks.len(), 7); // the stray link text outside a paragraph: a plain line
    assert_eq!(pack(&[Some((1000, 10)), Some((1000, 20)), Some((5000, 1))]), [Some([0, 0]), Some([0, 10]), None]);
}

/// Every page of the installed game parses; the contents page links to pages that exist.
#[test]
fn help_pages_parse() {
    let Ok(rd) = std::fs::read_dir(dir()) else { return }; // needs the game install
    let main = parse(&std::fs::read_to_string(dir().join("help_main.html")).unwrap());
    assert!(main.links.len() >= 10 && main.links.iter().all(|l| dir().join(l).is_file()), "{:?}", main.links);
    assert!(main.blocks.iter().any(|b| matches!(b, Block::Image(_, true))));
    for e in rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "html")) {
        let p = parse(&String::from_utf8_lossy(&std::fs::read(e.path()).unwrap()));
        assert!(!p.blocks.is_empty(), "{:?}", e.path());
    }
    assert!(building_page(9).is_some_and(|p| p == "st08_barracks.html"));
}
