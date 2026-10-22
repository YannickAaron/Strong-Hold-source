//! = re/scripts/export_ui.py (XAML, font, strings) + export_ui_atlas.py (HUD atlas and tables), straight from the
//! asset files: XAML only lives in memory. C#-derived tables come from re/data/cs_*.tsv (gen_cs_tables.py).
use crate::classes::{self, MONO_BEHAVIOUR, MONO_SCRIPT, TEXTURE2D};
use crate::unity::SFile;
use crate::Result;
use regex::Regex;
use std::{collections::HashMap, fmt::Write as _, fs, path::Path};

const SECTIONS: &str = include_str!("../../re/data/cs_text_sections.tsv");
const BUILD_BUTTONS: &str = include_str!("../../re/data/cs_build_buttons.tsv");
const BUILD_LISTS: &str = include_str!("../../re/data/cs_build_lists.tsv");
const MAPPERS: &str = include_str!("../../re/data/cs_mappers.tsv");
const CHIMPS: &str = include_str!("../../re/data/cs_chimps.tsv");
const TITLES: &str = include_str!("../../re/data/cs_building_titles.tsv");

/// NoesisXaml / NoesisFont objects: source path ("Assets/GUI/Sprites/UI-MasterAtlas.xaml") -> content.
pub fn noesis(data: &Path) -> Result<HashMap<String, Vec<u8>>> {
    let ggm = SFile::open(&data.join("globalgamemanagers.assets"))?;
    let mut scripts = HashMap::new();
    for r in ggm.each(MONO_SCRIPT) {
        let (o, b) = r?;
        scripts.insert(o.path_id, classes::name(&b));
    }
    let mut out = HashMap::new();
    for f in ["globalgamemanagers.assets", "sharedassets1.assets", "resources.assets"] {
        let file = if f == "globalgamemanagers.assets" { None } else { Some(SFile::open(&data.join(f))?) };
        for r in file.as_ref().unwrap_or(&ggm).each(MONO_BEHAVIOUR) {
            let (_, b) = r?;
            let (script, name, mut rd) = classes::mono_behaviour(&b);
            if !matches!(scripts.get(&script).map(String::as_str), Some("NoesisXaml" | "NoesisFont")) { continue; }
            let source = rd.str();
            let content = rd.bytes().to_vec();
            out.insert(if source.is_empty() { format!("{name}.xaml") } else { source }, content);
        }
    }
    Ok(out)
}

fn re(s: &str) -> Regex { Regex::new(s).unwrap() }

fn attrs(s: &str) -> HashMap<String, String> {
    re(r#"([\w:.]+)="([^"]*)""#).captures_iter(s).map(|c| (c[1].to_string(), c[2].to_string())).collect()
}

/// csv.writer(delimiter="\t") field: quoted when it holds a tab, quote or line break.
fn csv_field(s: &str) -> String {
    if s.contains(['\t', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn parse_strings(path: &Path, secs: &[&str]) -> Result<(String, Vec<(String, String)>)> {
    let text = String::from_utf8_lossy(&fs::read(path)?).into_owned();
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let lang = lines.first().map_or("", |l| l.trim()).to_string();
    let mut i = lines.iter().position(|l| l.to_lowercase() == ">>textstart").unwrap_or(lines.len()) + 1;
    let (mut out, mut sec, mut n) = (vec![], -1i32, 0);
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        if l.is_empty() { continue; }
        if l.chars().count() >= 3 && l.starts_with(">>") { sec += 1; n = 0; continue; }
        if sec >= 370 { break; }
        n += 1;
        let s = secs.get(sec.max(0) as usize).copied().unwrap_or("");
        let s = if sec < 0 { secs.last().copied().unwrap_or("") } else { s }; // Python secs[-1]
        out.push((format!("{s}_{n:03}"), l.to_string()));
    }
    Ok((lang, out))
}

/// strings.tsv: key + one column per language file (crusader.txt, then crusader_*.txt sorted).
pub fn strings(game: &Path) -> Result<String> {
    let secs: Vec<&str> = SECTIONS.lines().collect();
    let tdir = game.join("Assets/Text");
    let mut files = vec![tdir.join("crusader.txt")];
    let mut extra: Vec<String> = fs::read_dir(&tdir)?.flatten().map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|f| f.to_lowercase().starts_with("crusader_") && f.to_lowercase().ends_with(".txt")).collect();
    extra.sort();
    files.extend(extra.iter().map(|f| tdir.join(f)));
    let tables = files.iter().map(|p| parse_strings(p, &secs)).collect::<Result<Vec<_>>>()?;
    let maps: Vec<HashMap<&str, &str>> = tables.iter().map(|(_, t)| t.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()).collect();
    let mut s = String::new();
    let head: Vec<String> = std::iter::once("key".to_string()).chain(tables.iter().map(|t| t.0.clone())).map(|f| csv_field(&f)).collect();
    writeln!(s, "{}", head.join("\t"))?;
    let mut seen = std::collections::HashSet::new();
    for (k, _) in &tables[0].1 {
        if !seen.insert(k.as_str()) { continue; } // dict keys: first position, last value
        let row: Vec<String> = std::iter::once(k.to_string())
            .chain(maps.iter().map(|m| m.get(k.as_str()).copied().unwrap_or("").replace('\t', " "))).map(|f| csv_field(&f)).collect();
        writeln!(s, "{}", row.join("\t"))?;
    }
    Ok(s)
}

pub fn export(game: &Path, data: &Path, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    // 1. atlas pixels: inline BC7, all mips
    let ggm = SFile::open(&data.join("globalgamemanagers.assets"))?;
    let mut atlas = None;
    for r in ggm.each(TEXTURE2D) {
        let (_, b) = r?;
        if classes::name(&b) == "UI-MasterAtlas" { atlas = Some(classes::texture2d(&b)); break; }
    }
    let t = atlas.ok_or("UI-MasterAtlas not found")?;
    let size: usize = (0..t.mips).map(|m| ((t.w >> m) / 4).max(1) * ((t.h >> m) / 4).max(1) * 16).sum();
    if t.format != 25 || t.image.len() != size { return Err("UI-MasterAtlas: unexpected format".into()); }
    fs::write(out.join("UI-MasterAtlas.bc"), &t.image)?;

    let x = noesis(data)?;
    let xaml = |rel: &str| -> Result<String> {
        Ok(String::from_utf8_lossy(x.get(&format!("Assets/GUI/{rel}")).ok_or_else(|| format!("{rel} not found"))?).into_owned())
    };
    // 2. rects
    let mut s = String::new();
    for c in re(r#"x:Key="([^"]+)" Source="\{StaticResource UI-MasterAtlas\}" SourceRect="(\d+),(\d+),(\d+),(\d+)""#).captures_iter(&xaml("Sprites/UI-MasterAtlas.xaml")?) {
        writeln!(s, "{}\t{}\t{}\t{}\t{}", &c[1], &c[2], &c[3], &c[4], &c[5])?;
    }
    fs::write(out.join("rects.tsv"), s)?;
    // 3. goods bar
    let mut s = String::new();
    for c in re(concat!(r#"(?s)<Grid\s+HorizontalAlignment="Left"\s+VerticalAlignment="Bottom"\s+Width="50"\s+Height="50"\s+"#,
        r#"Margin="(-?\d+),0,0,(-?\d+)"\s+Visibility="\{Binding DisplayHudGoodsBool\[(\d+)\].*?Sprite1="\{StaticResource ([^}]+)\}""#))
        .captures_iter(&xaml("XAMLResources/HUD_Goods.xaml")?) {
        writeln!(s, "{}\t{}\t{}\t{}", &c[3], &c[1], &c[2], &c[4])?;
    }
    fs::write(out.join("goods.tsv"), s)?;
    // 4. build menu
    let hm = xaml("XAMLResources/HUD_Main.xaml")?;
    let mut buttons: HashMap<String, HashMap<String, String>> = HashMap::new();
    for c in re(r"(?s)<(?:Button|RadioButton)\b(.*?)/?>").captures_iter(&hm) {
        let a = attrs(&c[1]);
        if let Some(n) = a.get("Name") { buttons.insert(n.clone(), a); }
    }
    let ids: HashMap<&str, &str> = BUILD_BUTTONS.lines().filter_map(|l| l.split_once('\t')).collect();
    let mappers: HashMap<&str, &str> = MAPPERS.lines().filter_map(|l| l.split_once('\t')).collect();
    let sprite = |v: &str| v.strip_prefix("{StaticResource ").and_then(|v| v.strip_suffix('}')).unwrap_or(v).to_string();
    let tag_re = re(r"GameTexts\[(\w+)\]");
    let mut s = String::new();
    for (screen, row) in BUILD_LISTS.lines().enumerate() {
        for bid in row.split('\t') {
            if bid.parse::<i64>().unwrap_or(0) <= 0 { break; }
            let Some(b) = ids.get(bid).and_then(|n| buttons.get(*n)) else { continue };
            let g = |k: &str| b.get(k).map_or("", String::as_str);
            let mg: Vec<i64> = b.get("Margin").map_or("0,0,0,0", String::as_str).split(',').map(|v| v.trim().parse::<f64>().unwrap_or(0.0) as i64).collect();
            let tag = tag_re.captures(g("Tag")).map_or(String::new(), |c| c[1].to_string());
            let dflt = |k: &str| b.get(k).cloned().unwrap_or_else(|| "0".into());
            writeln!(s, "{screen}\t{bid}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{tag}\t{}", g("Name"), g("CommandParameter"),
                sprite(g("local:PropEx.Sprite1")), sprite(g("local:PropEx.Sprite2")), mg[0], mg.get(1).unwrap_or(&0),
                dflt("Width"), dflt("Height"), mappers.get(g("CommandParameter")).unwrap_or(&"0"))?;
        }
    }
    fs::write(out.join("build.tsv"), s)?;
    // 5. category tabs
    let tabs = re(r#"(?s)Name="BottomTabs1"(.*?)Name="RadioButtonMERuins""#).captures(&hm).ok_or("BottomTabs1")?[1].to_string();
    let mut s = String::new();
    for c in re(r#"Name="(TabBuild\w+)""#).captures_iter(&tabs) {
        let b = buttons.get(&c[1]).ok_or("tab button")?;
        let sp: Vec<String> = (1..=4).map(|i| sprite(b.get(&format!("local:PropEx.Sprite{i}")).map_or("", String::as_str))).collect();
        writeln!(s, "{}\t{}\t{}", &c[1], sp.join("\t"), b.get("Margin").map_or("", |m| m.split(',').next().unwrap_or("")))?;
    }
    fs::write(out.join("tabs.tsv"), s)?;
    // 6. troop picture buttons
    let chimps: Vec<&str> = CHIMPS.lines().collect();
    let ht = xaml("XAMLResources/HUD_Troops.xaml")?;
    let sel = re(r#"(?s)Name="TroopSelectionControls"(.*?)Name="TroopSelectionNumbers""#).captures(&ht).ok_or("TroopSelectionControls")?[1].to_string();
    let spr_re = re(r"^\{(?:StaticResource|Binding) (?:UIButtons(\w))?(.*)\}$");
    let spr = |v: &str| match spr_re.captures(v) {
        Some(c) => match c.get(1) { Some(l) => format!("UI-Buttons {}{}", l.as_str(), &c[2]), None => c[2].to_string() },
        None => v.to_string(),
    };
    let mut s = String::new();
    for c in re(r"(?s)<Button\b(.*?)>").captures_iter(&sel) {
        let a = attrs(&c[1]);
        let Some(p) = a.get("CommandParameter") else { continue };
        let g = |k: &str| a.get(k).map_or("", String::as_str);
        let i = chimps.iter().position(|c| c == p).ok_or("chimp")?;
        writeln!(s, "{i}\t{}\t{}\t{}\t{}", g("Width"), g("Height"), spr(g("local:PropEx.Sprite1")), spr(g("local:PropEx.Sprite2")))?;
    }
    fs::write(out.join("troops.tsv"), s)?;
    // 7. titles, strings, font
    fs::write(out.join("titles.tsv"), TITLES)?;
    fs::write(out.join("strings.tsv"), strings(game)?)?;
    fs::write(out.join("font.ttf"), x.get("Assets/Resources/JunicodeTwoBeta-Medium.ttf").ok_or("font not found")?)?;
    Ok(())
}
