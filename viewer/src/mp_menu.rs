//! The LAN multiplayer front end, a child module of menu.rs (like trail_menu.rs): the LAN screen (Host a game / the
//! games found on the network / Back) and the multiplayer lobby, which is the skirmish lobby itself (the original
//! shares FRONT_Multiplayer between both) with the humans of the `lan::Net` session in the player rows. The host edits
//! map, AI lords, teams and options; clients follow the host's lobby and wait for Play. Network: crate::lan.
#![cfg_attr(not(feature = "live"), allow(dead_code))] // without the sim only the menu side builds
use super::*;
use crate::lan;
use std::time::{Duration, Instant};

/// The LAN screen (the lobby is `Screen::Lobby` with `Menu.mp.net` set).
#[derive(Clone, Copy, PartialEq)]
pub enum T { Lan }

/// LAN clicks (menu.rs `Act::Mp`).
#[derive(Clone, Copy, PartialEq)]
pub enum A { Open, Host, Join(usize), Type, JoinAddr }

/// A started LAN game for main.rs (`Go::Lan`): the session, the lobby as started, the host's seed and the input delay
/// (whether this peer gets the game state from the host / loads a save: `Net.state` / `Net.save`).
pub struct Started { pub net: lan::Net, pub lobby: lan::Lobby, pub seed: i32, pub delay: i32 }

/// The menu's LAN state: the games found (a background query every 2 s on the LAN screen), the session once hosting
/// or joined, and (host) the lobby last sent.
#[derive(Default)]
pub struct Mp {
    pub net: Option<lan::Net>,
    found: Vec<lan::Found>,
    scan: Option<(std::sync::mpsc::Receiver<Vec<lan::Found>>, Instant)>,
    scanned: bool,
    pushed: Option<lan::Lobby>,
    /// Host: the saved game the lobby loads (the MPLoad button).
    save: Option<PathBuf>,
    /// The LAN screen's "join by address" field: its text, and whether keys go into it.
    addr: String,
    pub(super) typing: bool,
    /// Host picking a saved game: the lobby to return to (screen, map list, its infos, scroll).
    back: Option<(Lobby, Vec<PathBuf>, Vec<skirmish::MapInfo>, usize)>,
    /// The loaded save's human players (`lan::save_humans`).
    save_slots: Vec<i32>,
}

/// The player's name for the lobby: Options > Player Settings, else the Mac user.
fn player_name() -> String { super::coa::player_name() }

/// The skirmish lobby's view of a LAN lobby (map path, AIs, options; your team = the host's).
fn to_skirmish(l: &lan::Lobby, seed: i32) -> Lobby {
    Lobby { map: skirmish::maps_dir().join(&l.map), team: l.humans.first().map_or(0, |h| h.team), ais: l.ais.clone(), fairness: l.fairness,
        goods: l.goods, outposts: l.outposts, seed, ..Lobby::new(skirmish::maps_dir().join(&l.map)) }
}

impl Menu {
    /// Rows of the humans for the lobby's player list in a LAN game: (kind, name), yours marked; None = single player.
    pub(super) fn mp_rows(&self, _ui: &Ui) -> Option<Vec<(String, String)>> {
        let n = self.mp.net.as_ref()?;
        Some(n.lobby.humans.iter().enumerate().map(|(k, h)| {
            let kind = if k == 0 { "Host".to_string() } else { "Player".to_string() };
            let me = if k as i32 + 1 == n.me { format!("{} (you)", h.name) } else { h.name.clone() };
            (kind, me)
        }).collect())
    }

    /// Humans in the lobby (1 in single player).
    pub(super) fn mp_humans(&self) -> usize { self.mp.net.as_ref().map_or(1, |n| n.lobby.humans.len()) }

    /// Only the host may press Play in a LAN lobby, and only with somebody to play against.
    pub(super) fn mp_can_start(&self) -> bool { self.mp.net.as_ref().is_none_or(|n| n.is_host()) }

    /// The skirmish list's lobby (as Act::Skirmish opens it) on `map` (None: Crater Lake, else the first).
    fn mp_open_lobby(&mut self, map: Option<&str>) {
        let (maps, infos): (Vec<PathBuf>, Vec<_>) = skirmish::list(&skirmish::maps_dir(), "map").into_iter()
            .filter_map(|m| skirmish::map_info(&m).filter(|i| i.players >= 2).map(|i| (m, i))).unzip();
        let want = map.unwrap_or("Crater Lake.map");
        let Some(i) = maps.iter().position(|p| p.file_name().is_some_and(|f| f == want)).or((!maps.is_empty()).then_some(0)) else {
            self.error = Some(format!("no maps in {}", skirmish::maps_dir().display()));
            return;
        };
        let net = self.mp.net.take();
        let mut l = Lobby::new(maps[i].clone());
        l.ais.clear(); // a LAN game starts with just the humans
        self.open(Screen::Lobby(l), maps);
        self.mp.net = net;
        self.infos = infos;
        self.scroll = i.saturating_sub(LIST_ROWS / 2).min(self.files.len().saturating_sub(LIST_ROWS));
        self.select(i);
    }

    /// Clicks that LAN games handle before menu.rs: the LAN screen, Play / Back in a LAN lobby, and a client's edits
    /// (the host owns the lobby). None = not ours.
    pub(super) fn mp_click_first(&mut self, a: Act) -> Option<Option<Go>> {
        if let Act::Mp(a) = a {
            return Some(self.mp_click(a));
        }
        self.mp.net.as_ref()?;
        // the host picks a saved game for the lobby (the MPLoad button → the Load screen → back with it)
        if let (Screen::Load, Some(_)) = (&self.screen, &self.mp.back) {
            match a {
                Act::Pick(i) => {
                    let f = self.files[i].clone();
                    #[cfg(feature = "live")]
                    let humans = std::fs::read(&f).map_err(|e| e.to_string()).and_then(|b| lan::save_humans(&b));
                    #[cfg(not(feature = "live"))] // no sim, no LAN game (lan_start is live-only)
                    let humans: Result<Vec<i32>, String> = Err("needs the live build".into());
                    let need = self.mp.net.as_ref().map_or(1, |n| n.lobby.humans.len());
                    match humans {
                        Ok(h) if h.len() >= need => {
                            self.mp_back();
                            (self.mp.save, self.mp.save_slots) = (Some(f.clone()), h);
                            self.error = Some(format!("Loads {} (players: humans in the save's order)", f.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned())));
                        }
                        Ok(h) => self.error = Some(format!("That save has {} human players, the lobby {need}", h.len())),
                        Err(e) => self.error = Some(format!("Can't load: {e}")),
                    }
                    return Some(None);
                }
                Act::Back => { self.mp_back(); return Some(None); }
                _ => return None,
            }
        }
        let net = self.mp.net.as_mut()?;
        if !matches!(self.screen, Screen::Lobby(_)) { return None; }
        match a {
            Act::Start if net.is_host() => {
                let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() as i32);
                self.mp_push();
                let net = self.mp.net.as_mut().unwrap();
                let (lobby, seed, delay) = net.start(seed, lan::DELAY);
                let net = self.mp.net.take().unwrap();
                let mut net = net;
                net.save = self.mp.save.clone();
                Some(Some(Go::Lan(Box::new(Started { net, lobby, seed, delay }))))
            }
            Act::Back | Act::Quit => None,
            Act::LoadList if net.is_host() => {
                if let Screen::Lobby(l) = &self.screen { self.mp.back = Some((l.clone(), self.files.clone(), self.infos.clone(), self.scroll)); }
                let _ = crate::saves::files(); // (resets the Load screen's shown preview)
                self.open(Screen::Load, Self::mp_saves(&skirmish::saves_dir()));
                Some(None)
            }
            Act::LoadList => Some(None),
            _ if !net.is_host() => Some(None),
            _ => None,
        }
    }

    fn mp_click(&mut self, a: A) -> Option<Go> {
        match a {
            A::Open => {
                *self = Menu { screen: Screen::Mp(T::Lan), ..Self::main() };
                self.mp_scan();
            }
            A::Host => match lan::Net::host(&player_name(), 0) {
                Ok(net) => {
                    self.mp = Mp { net: Some(net), ..Default::default() };
                    self.mp_open_lobby(None);
                }
                Err(e) => self.error = Some(format!("Can't host: {e}")),
            },
            A::Join(i) => if let Some(f) = self.mp.found.get(i).cloned() {
                // a running game: back in as the player whose name we have, else watch it
                let me = player_name();
                let join = match f.lost.iter().find(|(_, n)| *n == me) {
                    Some(&(p, _)) => lan::Join::Rejoin(p),
                    None if f.running => lan::Join::Spectate,
                    None => lan::Join::Play,
                };
                return self.mp_join(f.addr, join, &f.name);
            },
            A::Type => self.mp.typing = true,
            A::JoinAddr => match lan::parse_addr(&self.mp.addr) {
                Some(a) => { let s = self.mp.addr.clone(); return self.mp_join(a, lan::Join::Play, &s); }
                None => self.error = Some(format!("Not an address: {:?} (e.g. 192.168.1.20 or 192.168.1.20:47778)", self.mp.addr)),
            },
        }
        None
    }

    /// Join the game at `addr`: a lobby opens (new player), or the running game starts at once (spectator / back in).
    fn mp_join(&mut self, addr: std::net::SocketAddr, join: lan::Join, what: &str) -> Option<Go> {
        match lan::Net::join_as(addr, &player_name(), join, 0, 0) {
            Ok(net) => {
                if let Some((lobby, seed, delay)) = net.started.clone() {
                    return Some(Go::Lan(Box::new(Started { net, lobby, seed, delay })));
                }
                let (map, coop) = (net.lobby.map.clone(), net.lobby.coop);
                self.mp = Mp { net: Some(net), ..Default::default() };
                match coop { // a co-op trail game: its co-op lobby (trail_menu.rs)
                    Some((t, k, _)) => self.screen = Screen::Trail(super::trail_menu::T::Map(20 + t, k + 1)),
                    None => self.mp_open_lobby(Some(&map)),
                }
            }
            Err(e) => self.error = Some(format!("Can't join {what}: {e}")),
        }
        None
    }

    /// Keys go into the address field (menu.rs `capturing` / `key`).
    pub(super) fn mp_typing(&self) -> bool { self.mp.typing && matches!(self.screen, Screen::Mp(T::Lan)) }

    /// A key (keys.rs Unity name) in the address field: digits, '.', ':', letters and '-' for host names, Backspace,
    /// Return joins.
    pub(super) fn mp_key(&mut self, k: &str) -> Option<Go> {
        let (code, shift) = (k.split(',').next().unwrap_or(""), k.contains("shift"));
        let c = match code {
            "Backspace" | "Delete" => { self.mp.addr.pop(); return None }
            "Return" | "KeypadEnter" => { self.mp.typing = false; return self.mp_click(A::JoinAddr) }
            "Period" | "KeypadPeriod" if shift => ':',
            "Semicolon" => ':', // ":" with or without Shift (layouts differ); ";" never occurs in an address
            "Period" | "KeypadPeriod" => '.',
            "Minus" => '-',
            _ => match code.strip_prefix("Alpha").or(code.strip_prefix("Keypad")).and_then(|d| d.parse::<u8>().ok()) {
                Some(d) if d < 10 => (b'0' + d) as char,
                _ if code.len() == 1 && code.as_bytes()[0].is_ascii_alphabetic() => code.to_ascii_lowercase().chars().next().unwrap(),
                _ => return None,
            },
        };
        if self.mp.addr.len() < 64 { self.mp.addr.push(c); }
        None
    }

        /// The LAN lobby's Load list: the multiplayer saves (.msv, as the original keeps them apart from .sav; a LAN game
    /// saves as .msv, live.rs) in `dir`, newest first.
    fn mp_saves(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("msv"))).collect();
        let t = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        v.sort_by_key(|p| std::cmp::Reverse(t(p)));
        v
    }

    /// Back from the Load screen to the LAN lobby as it was.
    fn mp_back(&mut self) {
        if let Some((l, files, infos, scroll)) = self.mp.back.take() {
            let net = self.mp.net.take();
            self.open(Screen::Lobby(l), files);
            (self.mp.net, self.infos, self.scroll) = (net, infos, scroll);
        }
    }

    /// Ask the network for games in the background (answers within 0.4 s).
    fn mp_scan(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(lan::discover(Duration::from_millis(400))); });
        self.mp.scan = Some((rx, Instant::now()));
    }

    /// Host: the lobby screen's state to the clients when it changed.
    fn mp_push(&mut self) {
        let (Some(net), Screen::Lobby(l)) = (self.mp.net.as_mut(), &self.screen) else { return };
        if !net.is_host() { return; }
        let speed = skirmish::Settings::load().game_speed() as i32;
        let mut humans = net.lobby.humans.clone();
        if let Some(h) = humans.first_mut() { h.team = l.team; }
        let map = l.map.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned());
        let ais = l.ais.iter().take(8usize.saturating_sub(humans.len())).copied().collect();
        let new = lan::Lobby { map, humans, ais, fairness: l.fairness, goods: l.goods, outposts: l.outposts, speed, coop: net.lobby.coop,
            slots: self.mp.save_slots.iter().copied().take(net.lobby.humans.len()).collect(), spectators: net.lobby.spectators.clone(),
            loaded: self.mp.save.is_some() };
        if self.mp.pushed.as_ref() != Some(&new) || new != net.lobby {
            net.set_lobby(new.clone());
            self.mp.pushed = Some(new);
        }
    }

    /// Every frame (main.rs): the LAN screen's game list, the lobby session (players joining / leaving, the host's
    /// edits to the clients, the clients following them), a client's game start. Some(Go::Lan) = start it.
    pub fn mp_poll(&mut self) -> Option<Go> {
        if matches!(self.screen, Screen::Trail(_)) { return self.trail_poll(); } // the co-op trail lobby (trail_menu.rs)
        if let Screen::Mp(T::Lan) = self.screen {
            if let Some((rx, t0)) = &self.mp.scan {
                if let Ok(v) = rx.try_recv() { (self.mp.found, self.mp.scanned) = (v, true); }
                if t0.elapsed() > Duration::from_secs(2) { self.mp_scan(); }
            }
            return None;
        }
        self.mp.net.as_ref()?;
        if !matches!(self.screen, Screen::Lobby(_)) { return None; }
        self.mp_push();
        let net = self.mp.net.as_mut().unwrap();
        net.poll();
        if let Some(e) = net.error.clone() {
            *self = Menu { screen: Screen::Mp(T::Lan), error: Some(e), ..Self::main() };
            self.mp_scan();
            return None;
        }
        if net.is_host() { return None; }
        if let Some((lobby, seed, delay)) = net.started.clone() {
            let net = self.mp.net.take().unwrap();
            return Some(Go::Lan(Box::new(Started { net, lobby, seed, delay })));
        }
        // a client mirrors the host's lobby (the map list follows its map)
        let want = to_skirmish(&net.lobby, 0);
        if let Screen::Lobby(l) = &mut self.screen {
            let new_map = l.map != want.map;
            (l.map, l.team, l.ais, l.fairness, l.goods, l.outposts) = (want.map.clone(), want.team, want.ais.clone(), want.fairness, want.goods, want.outposts);
            if new_map {
                if let Some(i) = self.files.iter().position(|f| *f == want.map) { self.select(i); }
            }
        }
        None
    }

    /// The LAN screen (1920×1080 grid, like Load Game): Host, the games found, Back.
    pub(super) fn mp_draw(&mut self, ui: &mut Ui, out: &mut Vec<HudTri>, _t: T, cursor: [f32; 2]) {
        self.panel(ui, out, 460.0, 120.0, 1000.0, 860.0);
        self.text(ui, out, "Multiplayer (LAN)", 960.0, 200.0, 56.0, 0.5, GOLD);
        self.button(ui, out, "Host a game", 760.0, 250.0, 400.0, 64.0, Act::Mp(A::Host), cursor);
        self.text(ui, out, "Games on your network", 960.0, 380.0, 34.0, 0.5, GOLD);
        let found = self.mp.found.clone();
        if found.is_empty() {
            let s = if self.mp.scanned { "No games found yet. Ask the other player to host." } else { "Searching the network..." };
            self.text(ui, out, s, 960.0, 460.0, 28.0, 0.5, DIM);
        }
        let me = player_name();
        for (i, f) in found.iter().enumerate().take(5) {
            let what = match f.lost.iter().find(|(_, n)| *n == me) {
                Some(_) => "rejoin".to_string(),
                None if f.running => "watch".to_string(),
                None => format!("{} players", f.players),
            };
            let label = format!("{}  -  {}  ({what})  -  {}", f.name, f.map, f.addr.ip());
            self.button(ui, out, &label, 520.0, 420.0 + 72.0 * i as f32, 880.0, 60.0, Act::Mp(A::Join(i)), cursor);
        }
        // join by address (another network segment, or the broadcast is blocked)
        self.text(ui, out, "Or join by address", 960.0, 790.0, 28.0, 0.5, GOLD);
        let blink = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() / 500) % 2 == 0;
        let caret = if self.mp.typing && blink { "|" } else { "" };
        let field = if self.mp.addr.is_empty() && !self.mp.typing { "click, type e.g. 192.168.1.20".to_string() } else { format!("{}{caret}", self.mp.addr) };
        self.button(ui, out, &field, 560.0, 810.0, 560.0, 56.0, Act::Mp(A::Type), cursor);
        self.button(ui, out, "Join", 1140.0, 810.0, 220.0, 56.0, Act::Mp(A::JoinAddr), cursor);
        self.button(ui, out, "Back", 840.0, 890.0, 240.0, 60.0, Act::Back, cursor);
    }
}

/// Two menus on localhost: one hosts (its lobby gets an AI and another map), the other finds and joins it; both poll
/// for 6 s like the UI does every frame. The client mirrors the host's lobby, nobody loses the connection, and the
/// host's Play starts the game on both (Go::Lan).
#[cfg(feature = "live")]
#[test]
fn lan_lobby_menus() {
    if !skirmish::maps_dir().join("Crater Lake.map").exists() { return; }
    let mut host = Menu::main();
    host.mp_click(A::Host);
    let Some(net) = host.mp.net.as_ref() else { panic!("host: {:?}", host.error) };
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], net.port()));
    let mut client = Menu::main();
    client.mp_click(A::Open);
    client.mp.found = vec![lan::Found { addr, name: "Host".into(), map: "Crater Lake".into(), players: 1, running: false, lost: vec![] }];
    let join = std::thread::spawn(move || { client.mp_click(A::Join(0)); client });
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(500) { host.mp_poll(); std::thread::sleep(Duration::from_millis(16)); }
    let mut client = join.join().unwrap();
    assert!(client.mp.net.is_some(), "client: {:?}", client.error);
    if let Screen::Lobby(l) = &mut host.screen { l.ais.push((4, 0)); } // the host adds Saladin
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(6) {
        assert!(host.mp_poll().is_none());
        assert!(client.mp_poll().is_none());
        std::thread::sleep(Duration::from_millis(16));
    }
    assert_eq!(host.error, None);
    assert_eq!(client.error, None, "client lost the host");
    let Screen::Lobby(cl) = &client.screen else { panic!("client left the lobby") };
    assert_eq!(cl.ais, vec![(4, 0)], "client mirrors the host's AI");
    assert_eq!(client.mp_humans(), 2);
    let go = host.mp_click_first(Act::Start).flatten();
    assert!(matches!(go, Some(Go::Lan(_))), "host Play");
    let t0 = Instant::now();
    let mut started = None;
    while started.is_none() && t0.elapsed() < Duration::from_secs(3) { started = client.mp_poll(); std::thread::sleep(Duration::from_millis(16)); }
    assert!(matches!(started, Some(Go::Lan(_))), "client got the start");
}

/// The address field: typed keys (as keys.rs names them) become "127.0.0.1:port", Return joins that host's lobby.
/// The LAN lobby's Load list has only the multiplayer saves (.msv), newest first.
#[test]
fn lan_load_list_msv() {
    let dir = std::env::temp_dir().join(format!("shc-mpsaves-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["single.sav", "old.msv", "replay.shcreplay"] { std::fs::write(dir.join(f), b"x").unwrap(); }
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(dir.join("new.MSV"), b"x").unwrap();
    assert_eq!(Menu::mp_saves(&dir), vec![dir.join("new.MSV"), dir.join("old.msv")]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "live")]
#[test]
fn join_by_address() {
    let mut host = Menu::main();
    host.mp_click(A::Host);
    let port = host.mp.net.as_ref().expect("host").port();
    let mut client = Menu::main();
    client.mp_click(A::Open);
    client.mp_click(A::Type);
    assert!(client.capturing(), "keys go to the field");
    assert!(client.back().is_none() && !client.capturing() && matches!(client.screen, Screen::Mp(T::Lan)), "Esc closes the field");
    client.mp_click(A::Type);
    let mut keys: Vec<String> = "127.0.0.1".chars().map(|c| if c == '.' { "Period".into() } else { format!("Alpha{c}") }).collect();
    keys.push("Semicolon".into());
    keys.extend(port.to_string().chars().map(|c| format!("Keypad{c}")));
    keys.extend(["Alpha9".into(), "Backspace".into()]);
    for k in &keys { client.key(k); }
    assert_eq!(client.mp.addr, format!("127.0.0.1:{port}"));
    let join = std::thread::spawn(move || { client.key("Return"); client });
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(800) { host.mp_poll(); std::thread::sleep(Duration::from_millis(10)); }
    let client = join.join().unwrap();
    assert!(client.mp.net.is_some(), "joined: {:?}", client.error);
    assert!(!client.capturing());
    assert_eq!(host.mp_humans(), 2);
}
