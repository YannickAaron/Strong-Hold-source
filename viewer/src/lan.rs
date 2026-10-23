//! LAN multiplayer over std::net (design + API: side-session-notes lan-mp.md): discovery, the lobby session and the
//! lockstep driver around `Live`.
//!
//! Inside the sim everything is the original's networked MP (state 1, `shc_sim::mp`): every player command is a chore
//! with an exec tick, every peer runs the same chores at the same tick in id order, and the original's own sync check
//! (FUN_180021ff0, `mp::sync_check`) compares the peers. Around it everything is ours (modern, not wire-compatible):
//! - Lockstep: each peer promises "every chore of mine with exec ≤ P is sent" (`Turn.through`). A peer runs the RunTick
//!   at tick T only when every other player's P ≥ T + 2: each chore that can run by T + 1 is then in its inbox before
//!   the parse at the end of RunTick T (FUN_180023ee0 counts exec ≤ tick as late).
//! - Adaptive input delay: exec = tick + delay, the delay from the measured round trips (≥ 3 ticks; 4 on a LAN). It
//!   may shrink because a new chore never gets an exec tick below the last promise (`mp::set_timing_min`).
//! - Hold + state transfer: the host stops everyone before a tick and sends the peers that need it the 4 KB pages of
//!   its engine memory that differ from theirs (page hashes first, lz4). Used for: resync after a desync, a player
//!   rejoining, a spectator joining a running game, a saved game loaded into a LAN lobby, a new host after the host left.
//! - Host migration: every client listens too; when the host is gone the lowest remaining player becomes the host, the
//!   others reconnect to it, it removes the old host (DLL_KickMPPlayer) and resyncs everyone from its state.
//! Topology: a TCP star, the host relays. Discovery: a UDP query to port 47777 (broadcast + this machine; std only, so
//! portable; no Bonjour), or join by address. Wire format: little endian, versioned.
#![cfg_attr(not(feature = "live"), allow(dead_code))] // without the sim only the menu side builds
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{atomic::AtomicBool, atomic::Ordering, mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub const DISCOVERY_PORT: u16 = 47777;
pub const GAME_PORT: u16 = 47778;
const VERSION: u32 = 5;
/// Input delay at the start, before any round trip is measured: the original's LAN floor (FUN_180027f00).
pub const DELAY: i32 = 10;
/// The fewest ticks from a command to its exec tick: the gate needs P ≥ T + 2 with P = tick + delay − 1.
pub const MIN_DELAY: i32 = 3;
pub const MAX_DELAY: i32 = 40;
/// Ticks between two sync checks in a game (5 s at speed 40; each costs a few ms).
pub const HASH_EVERY: i32 = 200;
/// Spectators get ids from 100 (players are 1..8).
pub const SPECTATOR: i32 = 100;
/// How long the game waits for a dropped player to come back before removing it.
/// Pauses per player and game (ours: the original has no player pause in multiplayer).
pub const PAUSES: i32 = 3;
pub const REJOIN_WAIT: Duration = Duration::from_secs(30);
/// Host migration: how long clients try the old host, then the new one.
const RECONNECT_OLD: Duration = Duration::from_secs(3);
const MIGRATE_WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Human {
    pub name: String,
    pub team: i32,
    pub lord: i32, // lord look (DLL_RegisterMultiplayerUser lordType), -1 = the map's
    pub coa: String, // coat of arms (AvatarDesign string, the "lordtype|coa" lobby member data), "" = none
}

/// The lobby the host edits and every peer mirrors. Players: the humans (host first), then the AI lords.
#[derive(Clone, Debug, PartialEq)]
pub struct Lobby {
    pub map: String, // map file name in skirmish::maps_dir(); every machine needs the game's maps
    pub humans: Vec<Human>,
    pub ais: Vec<(usize, i32)>, // (lord index as skirmish::Lobby.ais, team)
    pub fairness: i32,
    pub goods: i32,
    pub outposts: bool,
    pub speed: i32,
    /// A co-op Crusader Trail mission instead of the free lobby: (trail 1..3, mission 0..9, keeps swapped);
    /// `game` then uses `shc_sim::trail::coop_setup` (fixed map, AIs, keeps; humans on team 1).
    pub coop: Option<(i32, i32, bool)>,
    /// The player number of each human (empty: 1, 2, ... in order); a loaded game keeps the save's.
    pub slots: Vec<i32>,
    pub spectators: Vec<String>,
    /// Started from a saved game: the host sends its state to everyone.
    pub loaded: bool,
}

impl Lobby {
    pub fn new(host: &str) -> Self {
        Lobby { map: "Crater Lake.map".into(), humans: vec![Human { name: host.into(), team: 0, lord: -1, coa: crate::menu::coa::mine().code() }], ais: vec![], fairness: 3, goods: 1,
            outposts: true, speed: 40, coop: None, slots: vec![], spectators: vec![], loaded: false }
    }
    pub fn players(&self) -> usize { self.humans.len() + self.ais.len() }
    /// The player number of human k (0 = the host).
    pub fn player_of(&self, k: usize) -> i32 { self.slots.get(k).copied().unwrap_or(k as i32 + 1) }
    /// The human players' numbers.
    pub fn player_numbers(&self) -> Vec<i32> { (0..self.humans.len()).map(|k| self.player_of(k)).collect() }
}

/// How a peer joins: a new player in the lobby, a player coming back to a running game, a spectator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Join { Play, Rejoin(i32), Spectate }

#[derive(Clone, Debug)]
pub enum Msg {
    /// `tick` / `through`: a rejoining peer that kept its game (0 = it needs the whole state).
    Hello { name: String, version: u32, listen: u16, join: Join, tick: i32, through: i32, coa: String },
    Welcome { you: i32 },
    Refused(String),
    Lobby(Lobby),
    /// Every peer's listening address (host migration).
    Peers(Vec<(i32, SocketAddr)>),
    Chat { from: i32, text: String },
    /// In game: a chat line (or an insult, "\u{1}" + its number) to the players in bit mask `to` (bit p = player p);
    /// the host relays it to them (Platform_Multiplayer.SendIngameChat / SendIngameChatInsult).
    Say { from: i32, to: u32, text: String },
    /// `state`: the receiver gets the game state from the host instead of building the start itself.
    Start { lobby: Lobby, seed: i32, delay: i32, state: bool },
    /// After each RunTick: the sim's outgoing chore packets; every chore with exec ≤ `through` has been sent.
    Turn { from: i32, through: i32, packets: Vec<Vec<u8>> },
    /// The sync sums (`mp::sync_check`) after the stepping RunTick that reached `tick`.
    Hash { from: i32, tick: i32, sums: Vec<u32> },
    /// Player `who` leaves before the RunTick at tick `at` (DLL_KickMPPlayer); -1 = the connection dropped (Net → Lockstep).
    Left { who: i32, at: i32 },
    Ping { from: i32, t: u32 },
    Pong { from: i32, to: i32, t: u32 },
    /// Host → all: the longest round trip to a client, ms.
    Rtt { max_ms: u32 },
    /// Host → all: stop before the RunTick at `at`; the peers in `state` send their page hashes and get pages.
    Hold { at: i32, why: String, state: Vec<i32> },
    PageHashes { from: i32, hashes: Vec<u64> },
    /// Host → `to`: pages `idx` (4 KB each, lz4 of their concatenation); `last` ends the transfer.
    Pages { to: i32, last: bool, idx: Vec<u32>, lz: Vec<u8> },
    Synced { from: i32 },
    /// Host → all: the hold at `at` is over; every player's promise and the players still in the game.
    Go { at: i32, through: Vec<(i32, i32)>, active: Vec<i32> },
    /// Net → Lockstep (host): a player came back / a spectator joined the running game.
    Joined { who: i32, tick: i32, through: i32 },
    /// To the host: player `from` pauses (`on`) or resumes the game.
    Pause { from: i32, on: bool },
    /// From the host: the game is paused by player `by` (0 = running again), who has `left` pauses left.
    Paused { by: i32, left: i32 },
    /// To the host: a player asks for game speed `rate`; from the host: everyone runs at `rate` now.
    Speed { rate: i32 },
}

// ---- wire format: [u32 len][kind u8][fields], little endian, strings / blobs u32-length-prefixed

#[derive(Default)]
struct W(Vec<u8>);
impl W {
    fn u8(&mut self, v: u8) -> &mut Self { self.0.push(v); self }
    fn i32(&mut self, v: i32) -> &mut Self { self.0.extend(v.to_le_bytes()); self }
    fn u32(&mut self, v: u32) -> &mut Self { self.0.extend(v.to_le_bytes()); self }
    fn u64(&mut self, v: u64) -> &mut Self { self.0.extend(v.to_le_bytes()); self }
    fn bytes(&mut self, v: &[u8]) -> &mut Self { self.i32(v.len() as i32); self.0.extend(v); self }
    fn str(&mut self, v: &str) -> &mut Self { self.bytes(v.as_bytes()) }
    fn addr(&mut self, a: &SocketAddr) -> &mut Self { self.str(&a.ip().to_string()).u32(a.port() as u32) }
    fn join(&mut self, j: Join) -> &mut Self { match j { Join::Play => self.u8(0).i32(0), Join::Rejoin(p) => self.u8(1).i32(p), Join::Spectate => self.u8(2).i32(0) } }
    fn lobby(&mut self, l: &Lobby) -> &mut Self {
        self.str(&l.map).i32(l.humans.len() as i32);
        for h in &l.humans { self.str(&h.name).i32(h.team).i32(h.lord).str(&h.coa); }
        self.i32(l.ais.len() as i32);
        for &(lord, team) in &l.ais { self.i32(lord as i32).i32(team); }
        self.i32(l.fairness).i32(l.goods).u8(l.outposts as u8).i32(l.speed);
        match l.coop { Some((t, k, sw)) => self.u8(1).i32(t).i32(k).u8(sw as u8), None => self.u8(0) };
        self.i32(l.slots.len() as i32);
        for &s in &l.slots { self.i32(s); }
        self.i32(l.spectators.len() as i32);
        for s in &l.spectators { self.str(s); }
        self.u8(l.loaded as u8)
    }
}

struct R<'a>(&'a [u8]);
impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n { return None; }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> { self.take(1).map(|b| b[0]) }
    fn i32(&mut self) -> Option<i32> { self.take(4).map(|b| i32::from_le_bytes(b.try_into().unwrap())) }
    fn u32(&mut self) -> Option<u32> { self.take(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())) }
    fn u64(&mut self) -> Option<u64> { self.take(8).map(|b| u64::from_le_bytes(b.try_into().unwrap())) }
    fn len(&mut self) -> Option<usize> { self.i32().filter(|&n| (0..1 << 26).contains(&n)).map(|n| n as usize) }
    fn bytes(&mut self) -> Option<Vec<u8>> { let n = self.len()?; self.take(n).map(<[u8]>::to_vec) }
    fn str(&mut self) -> Option<String> { String::from_utf8(self.bytes()?).ok() }
    fn addr(&mut self) -> Option<SocketAddr> { let ip = self.str()?.parse().ok()?; Some(SocketAddr::new(ip, self.u32()? as u16)) }
    fn join(&mut self) -> Option<Join> { let (k, p) = (self.u8()?, self.i32()?); Some(match k { 0 => Join::Play, 1 => Join::Rejoin(p), _ => Join::Spectate }) }
    fn lobby(&mut self) -> Option<Lobby> {
        let map = self.str()?;
        let humans = (0..self.len()?).map(|_| Some(Human { name: self.str()?, team: self.i32()?, lord: self.i32()?, coa: self.str()? })).collect::<Option<_>>()?;
        let ais = (0..self.len()?).map(|_| Some((self.i32()? as usize, self.i32()?))).collect::<Option<_>>()?;
        let (fairness, goods, outposts, speed) = (self.i32()?, self.i32()?, self.u8()? != 0, self.i32()?);
        let coop = if self.u8()? != 0 { Some((self.i32()?, self.i32()?, self.u8()? != 0)) } else { None };
        let slots = (0..self.len()?).map(|_| self.i32()).collect::<Option<_>>()?;
        let spectators = (0..self.len()?).map(|_| self.str()).collect::<Option<_>>()?;
        Some(Lobby { map, humans, ais, fairness, goods, outposts, speed, coop, slots, spectators, loaded: self.u8()? != 0 })
    }
}

impl Msg {
    fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        match self {
            Msg::Hello { name, version, listen, join, tick, through, coa } => { w.u8(1).str(name).u32(*version).u32(*listen as u32).join(*join).i32(*tick).i32(*through).str(coa); }
            Msg::Welcome { you } => { w.u8(2).i32(*you); }
            Msg::Refused(why) => { w.u8(3).str(why); }
            Msg::Lobby(l) => { w.u8(4).lobby(l); }
            Msg::Peers(v) => { w.u8(5).i32(v.len() as i32); for (p, a) in v { w.i32(*p).addr(a); } }
            Msg::Chat { from, text } => { w.u8(6).i32(*from).str(text); }
            Msg::Start { lobby, seed, delay, state } => { w.u8(7).lobby(lobby).i32(*seed).i32(*delay).u8(*state as u8); }
            Msg::Turn { from, through, packets } => {
                w.u8(8).i32(*from).i32(*through).i32(packets.len() as i32);
                for p in packets { w.bytes(p); }
            }
            Msg::Hash { from, tick, sums } => { w.u8(9).i32(*from).i32(*tick).i32(sums.len() as i32); for &s in sums { w.u32(s); } }
            Msg::Left { who, at } => { w.u8(10).i32(*who).i32(*at); }
            Msg::Ping { from, t } => { w.u8(11).i32(*from).u32(*t); }
            Msg::Pong { from, to, t } => { w.u8(12).i32(*from).i32(*to).u32(*t); }
            Msg::Rtt { max_ms } => { w.u8(13).u32(*max_ms); }
            Msg::Hold { at, why, state } => { w.u8(14).i32(*at).str(why).i32(state.len() as i32); for &p in state { w.i32(p); } }
            Msg::PageHashes { from, hashes } => { w.u8(15).i32(*from).i32(hashes.len() as i32); for &h in hashes { w.u64(h); } }
            Msg::Pages { to, last, idx, lz } => { w.u8(16).i32(*to).u8(*last as u8).i32(idx.len() as i32); for &i in idx { w.u32(i); } w.bytes(lz); }
            Msg::Synced { from } => { w.u8(17).i32(*from); }
            Msg::Go { at, through, active } => {
                w.u8(18).i32(*at).i32(through.len() as i32);
                for &(p, t) in through { w.i32(p).i32(t); }
                w.i32(active.len() as i32);
                for &p in active { w.i32(p); }
            }
            Msg::Joined { who, tick, through } => { w.u8(19).i32(*who).i32(*tick).i32(*through); }
            Msg::Pause { from, on } => { w.u8(20).i32(*from).u8(*on as u8); }
            Msg::Paused { by, left } => { w.u8(21).i32(*by).i32(*left); }
            Msg::Say { from, to, text } => { w.u8(22).i32(*from).u32(*to).str(text); }
            Msg::Speed { rate } => { w.u8(23).i32(*rate); }
        }
        let mut out = (w.0.len() as u32).to_le_bytes().to_vec();
        out.extend(w.0);
        out
    }

    fn decode(b: &[u8]) -> Option<Msg> {
        let mut r = R(b);
        Some(match r.u8()? {
            1 => Msg::Hello { name: r.str()?, version: r.u32()?, listen: r.u32()? as u16, join: r.join()?, tick: r.i32()?, through: r.i32()?, coa: r.str()? },
            2 => Msg::Welcome { you: r.i32()? },
            3 => Msg::Refused(r.str()?),
            4 => Msg::Lobby(r.lobby()?),
            5 => Msg::Peers((0..r.len()?).map(|_| Some((r.i32()?, r.addr()?))).collect::<Option<_>>()?),
            6 => Msg::Chat { from: r.i32()?, text: r.str()? },
            22 => Msg::Say { from: r.i32()?, to: r.u32()?, text: r.str()? },
            7 => Msg::Start { lobby: r.lobby()?, seed: r.i32()?, delay: r.i32()?, state: r.u8()? != 0 },
            8 => {
                let (from, through) = (r.i32()?, r.i32()?);
                Msg::Turn { from, through, packets: (0..r.len()?).map(|_| r.bytes()).collect::<Option<_>>()? }
            }
            9 => {
                let (from, tick) = (r.i32()?, r.i32()?);
                Msg::Hash { from, tick, sums: (0..r.len()?).map(|_| r.u32()).collect::<Option<_>>()? }
            }
            10 => Msg::Left { who: r.i32()?, at: r.i32()? },
            11 => Msg::Ping { from: r.i32()?, t: r.u32()? },
            12 => Msg::Pong { from: r.i32()?, to: r.i32()?, t: r.u32()? },
            13 => Msg::Rtt { max_ms: r.u32()? },
            14 => {
                let (at, why) = (r.i32()?, r.str()?);
                Msg::Hold { at, why, state: (0..r.len()?).map(|_| r.i32()).collect::<Option<_>>()? }
            }
            15 => {
                let from = r.i32()?;
                Msg::PageHashes { from, hashes: (0..r.len()?).map(|_| r.u64()).collect::<Option<_>>()? }
            }
            16 => {
                let (to, last) = (r.i32()?, r.u8()? != 0);
                let idx = (0..r.len()?).map(|_| r.u32()).collect::<Option<_>>()?;
                Msg::Pages { to, last, idx, lz: r.bytes()? }
            }
            17 => Msg::Synced { from: r.i32()? },
            18 => {
                let at = r.i32()?;
                let through = (0..r.len()?).map(|_| Some((r.i32()?, r.i32()?))).collect::<Option<_>>()?;
                Msg::Go { at, through, active: (0..r.len()?).map(|_| r.i32()).collect::<Option<_>>()? }
            }
            19 => Msg::Joined { who: r.i32()?, tick: r.i32()?, through: r.i32()? },
            20 => Msg::Pause { from: r.i32()?, on: r.u8()? != 0 },
            21 => Msg::Paused { by: r.i32()?, left: r.i32()? },
            23 => Msg::Speed { rate: r.i32()? },
            _ => return None,
        })
    }
}

/// One TCP connection: blocking writes, a reader thread that decodes frames into a channel (None = closed).
struct Conn {
    w: TcpStream,
    rx: mpsc::Receiver<Option<Msg>>,
}

impl Conn {
    fn new(s: TcpStream) -> std::io::Result<Conn> {
        s.set_nodelay(true)?;
        s.set_nonblocking(false)?;
        let mut r = s.try_clone()?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || loop {
            let mut n = [0u8; 4];
            if r.read_exact(&mut n).is_err() { let _ = tx.send(None); return; }
            let len = u32::from_le_bytes(n) as usize;
            let mut frame = vec![0u8; len.min(1 << 27)];
            let m = if len >= 1 << 27 || r.read_exact(&mut frame).is_err() { None } else { Msg::decode(&frame) };
            if m.is_none() && !frame.is_empty() { eprintln!("lan: bad or cut message ({len} bytes), connection closed"); }
            let stop = m.is_none();
            if tx.send(m).is_err() || stop { return; }
        });
        Ok(Conn { w: s, rx })
    }
    /// Bytes written (0 = the connection is gone).
    fn send(&mut self, m: &Msg) -> usize { let b = m.encode(); if self.w.write_all(&b).is_ok() { b.len() } else { 0 } }
    /// Received messages so far; Err = the connection closed.
    fn recv(&mut self) -> Result<Vec<Msg>, Vec<Msg>> {
        let mut v = vec![];
        loop {
            match self.rx.try_recv() {
                Ok(Some(m)) => v.push(m),
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => return Err(v),
                Err(mpsc::TryRecvError::Empty) => return Ok(v),
            }
        }
    }
}

impl Drop for Conn {
    /// The reader thread holds a clone of the socket: shut it down so the peer sees the close at once.
    fn drop(&mut self) { let _ = self.w.shutdown(std::net::Shutdown::Both); }
}

/// A game found on the network. `running`: started (join to watch or to come back as `rejoin`'s player).
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub addr: SocketAddr,
    pub name: String,
    pub map: String,
    pub players: usize,
    pub running: bool,
    /// Players of a running game whose connection dropped: (player, name), they may rejoin.
    pub lost: Vec<(i32, String)>,
}

/// Ask the LAN (broadcast + this machine) for hosted games; answers within `wait`.
pub fn discover(wait: Duration) -> Vec<Found> { discover_at(DISCOVERY_PORT, wait) }

/// `discover` asking the responders on `port` (tests: a host's private one).
fn discover_at(port: u16, wait: Duration) -> Vec<Found> {
    let Ok(s) = UdpSocket::bind("0.0.0.0:0") else { return vec![] };
    let _ = s.set_broadcast(true);
    let _ = s.set_read_timeout(Some(Duration::from_millis(50)));
    for to in ["255.255.255.255", "127.0.0.1"] {
        let _ = s.send_to(b"SHC-LAN?", (to, port));
    }
    let (mut out, t0, mut buf) = (vec![], Instant::now(), [0u8; 1024]);
    while t0.elapsed() < wait {
        let Ok((n, from)) = s.recv_from(&mut buf) else { continue };
        let text = String::from_utf8_lossy(&buf[..n]).into_owned();
        let f: Vec<&str> = text.split('\n').collect();
        if f.len() >= 7 && f[0] == format!("SHC-LAN {VERSION}") {
            let (Ok(port), Ok(players)) = (f[4].parse::<u16>(), f[3].parse()) else { continue };
            let lost = f[6].split(',').filter_map(|e| e.split_once(':')).filter_map(|(p, n)| Some((p.parse().ok()?, n.to_string()))).collect();
            let found = Found { addr: SocketAddr::new(from.ip(), port), name: f[1].into(), map: f[2].into(), players, running: f[5] == "game", lost };
            // the same host answers the broadcast and the localhost query: keep one
            if !out.iter().any(|o: &Found| o.addr.port() == port && o.name == found.name) { out.push(found); }
        }
    }
    out
}

/// "host:port" or "host" (GAME_PORT) → an address; names are resolved.
pub fn parse_addr(s: &str) -> Option<SocketAddr> {
    use std::net::ToSocketAddrs;
    let s = s.trim();
    if s.is_empty() { return None; }
    let with_port = if s.contains(':') { s.to_string() } else { format!("{s}:{GAME_PORT}") };
    with_port.to_socket_addrs().ok()?.next()
}

/// The host's discovery answer thread: replies to "SHC-LAN?" with the announce text while `stop` is false.
/// On `sock`, else on DISCOVERY_PORT, which may still be held by a host that just closed or by another host on this
/// machine (a second app copy, a test run): retried every 0.3 s; until then joining by address still works.
fn responder(sock: Option<UdpSocket>, announce: Arc<Mutex<String>>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let s = match sock { Some(s) => s, None => loop {
            if stop.load(Ordering::Relaxed) { return; }
            match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) {
                Ok(s) => break s,
                Err(_) => std::thread::sleep(Duration::from_millis(300)),
            }
        } };
        let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
        let mut buf = [0u8; 64];
        while !stop.load(Ordering::Relaxed) {
            if let Ok((n, from)) = s.recv_from(&mut buf) {
                if &buf[..n] == b"SHC-LAN?" {
                    let text = announce.lock().unwrap().clone();
                    let _ = s.send_to(text.as_bytes(), from);
                }
            }
        }
    });
}

/// A listening socket on `port` (0 = any free one), non-blocking.
fn listen(port: u16) -> std::io::Result<TcpListener> {
    let l = if port == 0 { TcpListener::bind("0.0.0.0:0")? } else {
        match (port..port + 20).find_map(|p| TcpListener::bind(("0.0.0.0", p)).ok()) { Some(l) => l, None => TcpListener::bind("0.0.0.0:0")? }
    };
    l.set_nonblocking(true)?;
    Ok(l)
}

/// The session: the host (accepts players and spectators, owns the lobby, relays) or a client (mirrors it). Kept for
/// the whole game (the Lockstep owns it then).
pub struct Net {
    pub me: i32,
    host: bool,
    pub lobby: Lobby,
    pub chat: Vec<(i32, String)>,
    /// Host: Start sent; client: Start received (lobby, seed, delay).
    pub started: Option<(Lobby, i32, i32)>,
    /// Client: the Start says this peer gets the game state from the host (spectator, loaded game, rejoin).
    pub state: bool,
    /// Host: the saved game (.sav) the lobby starts from (`Lobby.loaded`).
    pub save: Option<std::path::PathBuf>,
    pub error: Option<String>,
    listener: Option<TcpListener>, // host: the game port; client: its own (for host migration)
    pending: Vec<Conn>,            // host: connected, no Hello yet
    peers: Vec<(i32, Conn)>,       // host: clients / spectators by id; client: the host
    /// Every peer's listening address (host: what the clients connected to).
    pub addrs: Vec<(i32, SocketAddr)>,
    host_addr: Option<SocketAddr>,
    pub host_id: i32,
    lost: Vec<(i32, String)>, // host, in game: players whose connection dropped (announced for rejoining)
    name: String,
    announce: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
    port: u16,
    pub sent_bytes: u64,
}

impl Drop for Net {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}

impl Net {
    /// Host a game on `port` (0 = GAME_PORT, else the next free one), discoverable on the LAN.
    pub fn host(name: &str, port: u16) -> std::io::Result<Net> { Self::host_with(name, if port == 0 { GAME_PORT } else { port }, None) }

    /// Tests: a host on OS-assigned game and discovery ports, so parallel test runs and apps on this machine can't
    /// collide with it or answer for it.
    #[cfg(test)]
    pub(crate) fn test_host() -> Net { Self::host_with("Host", 0, Some(UdpSocket::bind("127.0.0.1:0").unwrap())).expect("host") }

    /// `host` on game port `port` (0 = any free one) with discovery on `disc` (None = DISCOVERY_PORT).
    fn host_with(name: &str, port: u16, disc: Option<UdpSocket>) -> std::io::Result<Net> {
        let l = listen(port)?;
        let port = l.local_addr()?.port();
        let (announce, stop) = (Arc::new(Mutex::new(String::new())), Arc::new(AtomicBool::new(false)));
        responder(disc, announce.clone(), stop.clone());
        let n = Net { me: 1, host: true, lobby: Lobby::new(name), chat: vec![], started: None, state: false, save: None, error: None, listener: Some(l), pending: vec![],
            peers: vec![], addrs: vec![], host_addr: None, host_id: 1, lost: vec![], name: name.into(), announce, stop, port, sent_bytes: 0 };
        n.set_announce();
        Ok(n)
    }

    /// Join the game at `addr` as a new player.
    #[cfg_attr(not(test), allow(dead_code))] // tests (lan, mp_menu, trail_menu)
    pub fn join(addr: SocketAddr, name: &str) -> Result<Net, String> { Self::join_as(addr, name, Join::Play, 0, 0) }

    /// Join the game at `addr`: as a new player (lobby), back as player p (`tick` / `through` of the game it kept, 0 =
    /// it needs the whole state), or to watch. Waits up to 3 s for the host's welcome (and lobby / start).
    pub fn join_as(addr: SocketAddr, name: &str, join: Join, tick: i32, through: i32) -> Result<Net, String> {
        let own = listen(0).map_err(|e| e.to_string())?;
        let listen_port = own.local_addr().map_err(|e| e.to_string())?.port();
        let s = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| format!("{addr}: {e}"))?;
        let mut c = Conn::new(s).map_err(|e| e.to_string())?;
        c.send(&Msg::Hello { name: name.into(), version: VERSION, listen: listen_port, join, tick, through, coa: crate::menu::coa::mine().code() });
        let t0 = Instant::now();
        let (mut me, mut lobby, mut started, mut addrs, mut state) = (0, None, None, vec![], false);
        while t0.elapsed() < Duration::from_secs(3) && (me == 0 || (lobby.is_none() && started.is_none() && !(matches!(join, Join::Rejoin(_)) && tick > 0))) {
            for m in c.recv().map_err(|_| "the host closed the connection".to_string())? {
                match m {
                    Msg::Welcome { you } => me = you,
                    Msg::Lobby(l) => lobby = Some(l),
                    Msg::Start { lobby: l, seed, delay, state: s } => { lobby = Some(l.clone()); started = Some((l, seed, delay)); state = s }
                    Msg::Peers(v) => addrs = v,
                    Msg::Refused(why) => return Err(why),
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if me == 0 { return Err("no answer from the host".into()); }
        let lobby = lobby.unwrap_or_else(|| Lobby::new(""));
        Ok(Net { me, host: false, lobby, chat: vec![], started, state, save: None, error: None, listener: Some(own), pending: vec![], peers: vec![(0, c)], addrs,
            host_addr: Some(addr), host_id: 1, lost: vec![], name: name.into(), announce: Default::default(), stop: Arc::new(AtomicBool::new(true)),
            port: listen_port, sent_bytes: 0 })
    }

    pub fn is_host(&self) -> bool { self.host }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn port(&self) -> u16 { self.port }

    fn set_announce(&self) {
        let l = &self.lobby;
        let name = l.humans.first().map_or(self.name.as_str(), |h| h.name.as_str());
        let lost: Vec<String> = self.lost.iter().map(|(p, n)| format!("{p}:{}", n.replace([',', ':', '\n'], " "))).collect();
        *self.announce.lock().unwrap() = format!("SHC-LAN {VERSION}\n{name}\n{}\n{}\n{}\n{}\n{}", l.map.trim_end_matches(".map"), l.players(), self.port,
            if self.started.is_some() { "game" } else { "lobby" }, lost.join(","));
    }

    /// Host: to every peer except `but`; client: to the host.
    fn send(&mut self, m: &Msg, but: i32) {
        for (p, c) in &mut self.peers {
            if *p != but { self.sent_bytes += c.send(m) as u64; }
        }
    }

    /// Host: to peer `to` only.
    fn send_to(&mut self, to: i32, m: &Msg) {
        if let Some((_, c)) = self.peers.iter_mut().find(|(p, _)| *p == to) { self.sent_bytes += c.send(m) as u64; }
    }

    /// Host: change the lobby (map, AIs, teams, options) and show it to everyone.
    pub fn set_lobby(&mut self, l: Lobby) {
        if !self.is_host() || self.started.is_some() { return; }
        self.lobby = l;
        self.set_announce();
        let m = Msg::Lobby(self.lobby.clone());
        self.send(&m, 0);
    }

    /// In game: `text` to the players in mask `to` (Msg::Say); it shows here at once (recieveIngameChat on Send).
    pub fn say_to(&mut self, text: &str, to: u32) {
        self.chat.push((self.me, text.into()));
        let m = Msg::Say { from: self.me, to, text: text.into() };
        if self.host { self.relay(&m) } else { self.send(&m, self.me) }
    }

    /// Host: a Say to the peers it names, not back to its sender.
    fn relay(&mut self, m: &Msg) {
        let Msg::Say { from, to, .. } = m else { return };
        for k in 0..self.peers.len() {
            let p = self.peers[k].0;
            if p != *from && (1..32).contains(&p) && to & (1 << p) != 0 { self.sent_bytes += self.peers[k].1.send(m) as u64; }
        }
    }

    pub fn say(&mut self, text: &str) {
        let m = Msg::Chat { from: self.me, text: text.into() };
        self.chat.push((self.me, text.into()));
        self.send(&m, self.me);
    }

    /// Host: start the game for everyone; returns (lobby, seed, delay). Players of a loaded game and spectators get
    /// the state from the host (Start.state).
    pub fn start(&mut self, seed: i32, delay: i32) -> (Lobby, i32, i32) {
        let s = (self.lobby.clone(), seed, delay);
        self.started = Some((s.0.clone(), seed, delay));
        for k in 0..self.peers.len() {
            let state = self.lobby.loaded || self.peers[k].0 >= SPECTATOR;
            let m = Msg::Start { lobby: s.0.clone(), seed, delay, state };
            self.sent_bytes += self.peers[k].1.send(&m) as u64;
        }
        self.set_announce();
        s
    }

    fn send_peers(&mut self) {
        let m = Msg::Peers(self.addrs.clone());
        self.send(&m, 0);
    }

    /// Lobby / session housekeeping (call every frame, the Lockstep calls it in game): new players / spectators /
    /// rejoining players, lobby / chat / start messages, peers leaving. Game messages are returned.
    pub fn poll(&mut self) -> Vec<Msg> {
        let mut out = vec![];
        if self.host {
            if let Some(l) = &self.listener {
                while let Ok((s, _)) = l.accept() {
                    if let Ok(c) = Conn::new(s) { self.pending.push(c); }
                }
            }
        }
        // host: Hello → a human slot / back as player p / a spectator
        let mut i = 0;
        while i < self.pending.len() {
            let r = self.pending[i].recv();
            let hello = r.as_ref().map_or_else(|v| v, |v| v).iter().find_map(|m| match m {
                Msg::Hello { name, version, listen, join, tick, through, coa } => Some((name.clone(), *version, *listen, *join, *tick, *through, coa.clone())),
                _ => None,
            });
            let Some((name, v, listen_port, join, tick, through, coa)) = hello else {
                if r.is_err() { self.pending.remove(i); } else { i += 1; }
                continue;
            };
            let mut c = self.pending.remove(i);
            let ip = c.w.peer_addr().map(|a| a.ip()).unwrap_or([127, 0, 0, 1].into());
            let refuse = |c: &mut Conn, why: &str| { c.send(&Msg::Refused(why.into())); };
            if v != VERSION { refuse(&mut c, "Different game version."); continue; }
            let taken = |p: i32, peers: &[(i32, Conn)]| peers.iter().any(|(q, _)| *q == p);
            let id = match join {
                Join::Play if self.started.is_none() && self.lobby.players() < 8 => {
                    let me = (2..=8).find(|p| !taken(*p, &self.peers) && !self.lobby.player_numbers().contains(p)).unwrap_or(8);
                    let mut l = self.lobby.clone();
                    l.humans.push(Human { name, team: 0, lord: -1, coa });
                    l.ais.truncate(8 - l.humans.len());
                    self.peers.push((me, c));
                    self.set_lobby(l);
                    me
                }
                Join::Play => { refuse(&mut c, if self.started.is_some() { "The game is running: watch it instead." } else { "The game is full." }); continue; }
                Join::Rejoin(p) if self.started.is_some() && self.lobby.player_numbers().contains(&p) && !taken(p, &self.peers) && p != self.me => {
                    self.peers.push((p, c));
                    self.lost.retain(|q| q.0 != p);
                    out.push(Msg::Joined { who: p, tick, through });
                    p
                }
                Join::Rejoin(_) => { refuse(&mut c, "No such player is missing in that game."); continue; }
                Join::Spectate => {
                    let id = (SPECTATOR..).find(|p| !taken(*p, &self.peers)).unwrap();
                    self.peers.push((id, c));
                    if self.started.is_none() {
                        let mut l = self.lobby.clone();
                        l.spectators.push(name);
                        self.set_lobby(l);
                    } else {
                        out.push(Msg::Joined { who: id, tick: 0, through: 0 });
                    }
                    id
                }
            };
            self.send_to(id, &Msg::Welcome { you: id });
            if let Some((l, seed, delay)) = self.started.clone() {
                if tick == 0 { self.send_to(id, &Msg::Start { lobby: l, seed, delay, state: true }); }
            } else {
                let m = Msg::Lobby(self.lobby.clone());
                self.send_to(id, &m);
            }
            self.addrs.retain(|(p, _)| *p != id);
            self.addrs.push((id, SocketAddr::new(ip, listen_port)));
            self.send_peers();
            self.set_announce();
        }
        let mut gone = vec![];
        for k in 0..self.peers.len() {
            let from = self.peers[k].0;
            let (msgs, closed) = match self.peers[k].1.recv() { Ok(v) => (v, false), Err(v) => (v, true) };
            for m in msgs {
                match &m {
                    Msg::Lobby(l) if !self.host => self.lobby = l.clone(),
                    Msg::Welcome { you } if !self.host => self.me = *you, // renumbered: a player before us left the lobby
                    Msg::Peers(v) if !self.host => self.addrs = v.clone(),
                    Msg::Chat { from: f, text } => {
                        self.chat.push((*f, text.clone()));
                        if self.host { self.send(&m, from); }
                    }
                    Msg::Say { from: f, to, text } => {
                        if !self.host || to & (1 << self.me.clamp(0, 31)) != 0 { self.chat.push((*f, text.clone())); }
                        if self.host { self.relay(&m); }
                    }
                    Msg::Start { lobby, seed, delay, state } if !self.host => { self.started = Some((lobby.clone(), *seed, *delay)); self.state = *state }
                    Msg::Turn { .. } | Msg::Hash { .. } => {
                        if self.host { self.send(&m, from); }
                        out.push(m);
                    }
                    Msg::Hello { .. } | Msg::Welcome { .. } | Msg::Lobby(_) | Msg::Peers(_) | Msg::Start { .. } | Msg::Refused(_) => {}
                    _ => out.push(m),
                }
            }
            if closed { gone.push(from); }
        }
        for p in gone {
            self.peers.retain(|(q, _)| *q != p);
            self.addrs.retain(|(q, _)| *q != p);
            if !self.host {
                self.error = Some("The host left the game.".into());
            } else if self.started.is_none() {
                // a player left the lobby: their slot goes (players after it move up); a spectator just goes
                let mut l = self.lobby.clone();
                if p < SPECTATOR {
                    if let Some(k) = l.player_numbers().iter().position(|&q| q == p) { l.humans.remove(k); }
                    self.renumber(p);
                }
                self.set_lobby(l);
            } else if p < SPECTATOR {
                let name = self.lobby.player_numbers().iter().position(|&q| q == p).and_then(|k| self.lobby.humans.get(k)).map_or(String::new(), |h| h.name.clone());
                self.lost.push((p, name));
                self.set_announce();
                out.push(Msg::Left { who: p, at: -1 }); // the Lockstep waits for a rejoin, then removes the player
            }
        }
        out
    }

    /// Host, lobby: player `gone` left; the clients after it get the next lower number (their Welcome again).
    fn renumber(&mut self, gone: i32) {
        for (p, c) in &mut self.peers {
            if *p > gone && *p < SPECTATOR {
                *p -= 1;
                c.send(&Msg::Welcome { you: *p });
            }
        }
    }

    /// Host migration: this client becomes the host (on its own listening socket), with no peers yet.
    fn become_host(&mut self) {
        (self.host, self.host_id, self.error) = (true, self.me, None);
        self.peers.clear();
        if let Some(l) = &self.listener { self.port = l.local_addr().map_or(0, |a| a.port()); }
        self.stop = Arc::new(AtomicBool::new(false));
        responder(None, self.announce.clone(), self.stop.clone());
        self.set_announce();
    }

    /// Host migration / rejoin: connect to `addr` as player `me` that kept its game at `tick` / `through`.
    fn reconnect(&mut self, addr: SocketAddr, host_id: i32, tick: i32, through: i32) -> bool {
        let Ok(s) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else { return false };
        let Ok(mut c) = Conn::new(s) else { return false };
        c.send(&Msg::Hello { name: self.name.clone(), version: VERSION, listen: self.port, join: Join::Rejoin(self.me), tick, through, coa: crate::menu::coa::mine().code() });
        (self.peers, self.host_addr, self.host_id, self.error) = (vec![(0, c)], Some(addr), host_id, None);
        true
    }
}

// ---- the game start

/// The sim for an MP game as player `me`: the lobby as the original's MP start (DLL_PreInitMap_Multiplayer
/// skirmishGame = false, DLL_RegisterMultiplayerUser per human with its own `isSelf`, RegisterSkirmishUser per AI,
/// the host's seed via DLL_SetMPRandSeed), sync events off (the lockstep schedules), DLL_StartMultiplayerGameSynced.
#[cfg(feature = "live")]
pub fn game(l: &Lobby, me: i32, seed: i32) -> Result<shc_sim::mem::Mem, String> {
    if let Some((t, k, swapped)) = l.coop {
        // a co-op trail mission (trail session): the original's fixed MP-1 start for it
        let c = shc_sim::trail::COOP.get((t - 1) as usize).and_then(|c| c.get(k as usize)).ok_or(format!("no co-op mission {t}/{k}"))?;
        let lords = [0, 1].map(|h| l.humans.get(h).map_or(-1, |u| u.lord));
        let s = shc_sim::trail::coop_setup(shc_sim::data::aivs(), t, k, lords, me, swapped, seed).ok_or("co-op setup")?;
        let map = crate::skirmish::maps_dir().join(format!("{}.map", c.map));
        let file = std::fs::read(&map).map_err(|e| format!("{}: {e}", map.display()))?;
        return Ok(start_mem(&s, &file, seed));
    }
    let map = crate::skirmish::maps_dir().join(&l.map);
    let base = crate::skirmish::Lobby {
        ais: l.ais.clone(), fairness: l.fairness, goods: l.goods, outposts: l.outposts, seed,
        ..crate::skirmish::Lobby::new(map.clone())
    };
    let n = crate::skirmish::start_positions(&map).ok_or(format!("{} is not a skirmish map", l.map))?;
    if (n as usize) < l.players() { return Err(format!("{} has {n} start positions for {} players", l.map, l.players())); }
    let mut s = crate::skirmish::setup(&base);
    s.preinit[0] = 0; // networked MP (MP state 1)
    let h = l.humans.len() as i32;
    for a in &mut s.ais { a.0 += h - 1; }
    for a in &mut s.aivs { a.0 += h - 1; }
    s.humans = l.humans.iter().enumerate().map(|(k, u)| (k as i32 + 1, u.team, k as i32 + 1 == me, u.lord)).collect();
    // start position k → player k + 1 (MultiplayerSetupTransferData words 10..17)
    for k in 0..8 {
        let v = if k < l.players() { k as i32 } else { -10 };
        s.positions[k] = v as i8;
        s.mp_setup[(10 + k) * 4..(11 + k) * 4].copy_from_slice(&v.to_le_bytes());
    }
    s.mp_setup[4..8].copy_from_slice(&l.speed.to_le_bytes()); // starting_gamespeed
    let file = std::fs::read(&map).map_err(|e| format!("{}: {e}", map.display()))?;
    shc_formats::map::parse(&file).map_err(|e| format!("{}: {e:?}", l.map))?;
    let mut m = start_mem(&s, &file, seed);
    if me >= SPECTATOR { local_bytes(&mut m, 0, false); }
    Ok(m)
}

/// The MP-1 sim from a setup: the map load (new_skirmish = DLL_LoadMapToPlay), the seed, sync events off, synced start.
#[cfg(feature = "live")]
fn start_mem(s: &shc_sim::setup::Setup, file: &[u8], seed: i32) -> shc_sim::mem::Mem {
    let mut m = shc_sim::mem::Mem::default();
    shc_sim::terrain::init_static(&mut m);
    shc_sim::setup::new_skirmish(&mut m, s, file, 1, Some(seed));
    m.set_u8(0x1887eb430, 0); // MP+0x27a120: the DE sync events (host-scheduled chores) off
    shc_sim::mp::start_synced(&mut m, 0);
    m
}

/// The human players of a saved game, in player order: those with a keep and no AI lord (section 1100,
/// 0x188571c34[p] = lord + 1). (The loader resets the human-slot table to single player, so it can't tell.)
#[cfg(feature = "live")]
pub fn save_humans(file: &[u8]) -> Result<Vec<i32>, String> {
    use shc_sim::buildings as b;
    let mut m = shc_sim::mem::Mem::default();
    shc_sim::world::load_save(&mut m, file).map_err(|e| format!("{e:?}"))?;
    let keep = |p: i32| (1..b::SLOTS).any(|i| b::state(&m, i) != 0 && b::owner(&m, i) == p && (0x28..=0x2c).contains(&b::kind(&m, i)));
    Ok((1..9).filter(|&p| m.i32(0x188571c34 + p as u64 * 4) == 0 && keep(p)).collect())
}

/// A saved game (.sav, the platform-independent native format) as a LAN game, loaded the way the original loads an MP
/// save (`setup::load_mp_save`: the lobby's humans registered as players 1.. in lobby order, DLL_LoadMapToPlay with
/// multiplayerSave), then the lobby's humans on the save's human players (`lobby.slots`; any others no longer human:
/// the original's C# remap does this), this peer = `me`, the host's DLL_StartMultiplayerGame(fromSave) and the synced
/// start, as `game` does.
#[cfg(feature = "live")]
pub fn load_game(file: &[u8], lobby: &Lobby, me: i32) -> Result<shc_sim::mem::Mem, String> {
    let mut m = shc_sim::mem::Mem::default();
    shc_sim::terrain::init_static(&mut m);
    let players = lobby.player_numbers();
    let humans: Vec<(i32, i32, bool)> = lobby.humans.iter().zip(&players).enumerate().map(|(k, (h, &p))| (k as i32 + 1, h.team, p == me)).collect();
    shc_sim::setup::load_mp_save(&mut m, file, lobby.speed, &humans).map_err(|e| format!("{e:?}"))?;
    for p in 1..9u64 {
        m.set_i32(0x188571bbc + p * 4, if players.contains(&(p as i32)) { p as i32 } else { -1 });
    }
    m.set_u8(0x1887eb430, 0);
    local_bytes(&mut m, me, me == lobby.player_of(0));
    shc_sim::mp::start_multiplayer_game(&mut m, true, 0);
    shc_sim::mp::start_synced(&mut m, 0);
    Ok(m)
}

/// The engine bytes that belong to this peer: the local player (and its copies), the host flag (spectators: 0).
#[cfg(feature = "live")]
fn local_bytes(m: &mut shc_sim::mem::Mem, me: i32, host: bool) {
    m.set_i32(0x1888e0d60, me); // local player = MP+0x36fa50
    m.set_i32(0x188571bb8, me);
    m.set_i32(0x18033d9b4, me); // setup snapshot (FUN_180021bb0 +0x74)
    m.set_u32(0x188571cb0, host as u32);
}

// ---- state transfer: engine memory in 4 KB pages

const PAGE: usize = 4096;
/// Engine memory a peer keeps its own of when it gets the host's state into a running game: its draw cache and render
/// / UI events, UI timers, sound, its outgoing chore buffer.
const OWN: [(u64, u64); 5] = [
    (0x1809a133c, 0x1809a136c),
    (0x1809a23dc, 0x180a96624),
    (0x180a967b0, 0x181ac67b0),
    (0x181b60600, 0x181b61d00),
    (0x1888e42c0, 0x1888e8700),
];

#[cfg(feature = "live")]
fn pages() -> usize { (shc_sim::mem::END - shc_sim::mem::BASE) as usize / PAGE }

/// Per page: a 64-bit hash (0 for pages that lie inside OWN: never sent).
#[cfg(feature = "live")]
fn page_hashes(m: &shc_sim::mem::Mem) -> Vec<u64> {
    (0..pages()).map(|i| {
        let at = shc_sim::mem::BASE + (i * PAGE) as u64;
        if OWN.iter().any(|&(a, b)| at >= a && at + PAGE as u64 <= b) { return 0; }
        let mut h = 0x9e3779b97f4a7c15u64;
        for c in m.slice(at, PAGE).chunks_exact(8) {
            h = (h ^ u64::from_le_bytes(c.try_into().unwrap())).wrapping_mul(0xff51afd7ed558ccd).rotate_left(31);
        }
        h ^ (h >> 29)
    }).collect()
}

/// The pages `idx` as messages to `to`: lz4 of up to 1024 pages each, the last one flagged.
#[cfg(feature = "live")]
fn page_messages(m: &shc_sim::mem::Mem, to: i32, idx: &[u32]) -> Vec<Msg> {
    let chunks: Vec<&[u32]> = if idx.is_empty() { vec![&[]] } else { idx.chunks(1024).collect() };
    let n = chunks.len();
    chunks.into_iter().enumerate().map(|(k, c)| {
        let mut raw = Vec::with_capacity(c.len() * PAGE);
        for &i in c { raw.extend_from_slice(m.slice(shc_sim::mem::BASE + i as u64 * PAGE as u64, PAGE)); }
        Msg::Pages { to, last: k + 1 == n, idx: c.to_vec(), lz: lz4_flex::compress_prepend_size(&raw) }
    }).collect()
}

/// Write received pages into `m`; with `keep_own` the OWN ranges keep this peer's bytes.
#[cfg(feature = "live")]
fn apply_pages(m: &mut shc_sim::mem::Mem, chunks: &[(Vec<u32>, Vec<u8>)], keep_own: bool) -> Result<usize, String> {
    let saved: Vec<Vec<u8>> = if keep_own { OWN.iter().map(|&(a, b)| m.slice(a, (b - a) as usize).to_vec()).collect() } else { vec![] };
    let mut n = 0;
    for (idx, lz) in chunks {
        let raw = lz4_flex::decompress_size_prepended(lz).map_err(|e| format!("pages: {e}"))?;
        if raw.len() != idx.len() * PAGE { return Err("pages: bad size".into()); }
        for (k, &i) in idx.iter().enumerate() {
            m.slice_mut(shc_sim::mem::BASE + i as u64 * PAGE as u64, PAGE).copy_from_slice(&raw[k * PAGE..(k + 1) * PAGE]);
        }
        n += idx.len();
    }
    for (&(a, _), v) in OWN.iter().zip(&saved) { m.slice_mut(a, v.len()).copy_from_slice(v); }
    Ok(n)
}

/// Save sections whose memory lies in pages `idx` (desync diagnosis after a resync).
#[cfg(feature = "live")]
fn sections_of(idx: &[u32]) -> Vec<i32> {
    let mut out = BTreeSet::new();
    for (id, at, n) in shc_sim::world::section_slots() {
        let (a, b) = (((at - shc_sim::mem::BASE) / PAGE as u64) as u32, ((at + n as u64 - 1 - shc_sim::mem::BASE) / PAGE as u64) as u32);
        if idx.iter().any(|&i| i >= a && i <= b) { out.insert(id); }
    }
    out.into_iter().collect()
}

// ---- the lockstep

/// What the UI shows of a LAN game (shared with the sim thread's `Lockstep`, `Lockstep::status`): the error that
/// stopped it, the players it waits for, the chat, what a hold is about; `say` = lines the UI wants sent.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub error: Option<String>,
    pub waiting_for: Vec<i32>,
    pub waiting_since: Option<Instant>,
    pub players: Vec<(i32, String)>,
    pub chat: Vec<(i32, String)>,
    pub say: Vec<(String, u32)>, // lines (insults "\u{1}" + n) and their recipients' mask
    pub me: i32,
    pub teams: Vec<(i32, i32)>, // the humans' lobby teams (0 = alone)
    pub holding: Option<String>,
    /// UI → game: the Pause key was pressed (pause, or resume when paused).
    pub pause: bool,
    /// Paused by (player, pauses that player has left).
    pub paused: Option<(i32, i32)>,
    /// UI → game: this player asks for game speed n (keys, Options slider; the host decides).
    pub speed: Option<i32>,
    /// Game → UI: the speed everyone runs at from now on (main.rs `rate`).
    pub rate: Option<i32>,
    /// The Options GameSpeed setting last seen in this game: only a change of it is a speed request (`speed_setting`).
    pub cfg_speed: i32,
    pub delay: i32,
    pub rtt_ms: f32,
    /// The players' colours (`replay::colours`, from the engine after each tick; 0 = the default).
    pub colours: [u8; 9],
}

/// A stop of every peer before the RunTick at `at` (see the module doc).
#[derive(Default)]
struct Hold {
    at: i32,
    why: String,
    state: BTreeSet<i32>,               // peers that get the host's state
    sent: bool,                          // client: page hashes sent / host: inbox parsed
    pending: Vec<(i32, Vec<u64>)>,       // host: page hashes to answer
    synced: BTreeSet<i32>,               // host: peers that applied the state
    chunks: Vec<(Vec<u32>, Vec<u8>)>,    // client: pages received
    done: bool,                          // client: last page received
    applied: bool,                       // client: state applied
    go: Option<(Vec<(i32, i32)>, Vec<i32>)>, // client: the host's Go
    kick: Option<i32>,                   // host migration: the old host, removed at `at`
    pages_sent: usize,
}

/// The lockstep driver of one peer's game (owned by its `Live`): `ready` gates each RunTick (and runs holds),
/// `before` / `after` wrap it.
pub struct Lockstep {
    net: Net,
    pub me: i32,
    pub spectator: bool,
    pub delay: i32,
    /// Adapt the delay to the measured round trips (else it stays as started).
    pub adaptive: bool,
    /// A desync resyncs everyone from the host's state (else it stops the game).
    pub resync: bool,
    pub hash_every: i32,
    /// How long a dropped player is waited for (REJOIN_WAIT).
    pub rejoin_wait: Duration,
    promised: i32,
    through: BTreeMap<i32, i32>, // other players' promises
    active: Vec<i32>,            // human players in the game
    inbox: Vec<(i32, Vec<u8>)>,
    kicks: Vec<(i32, i32)>, // (player, before the RunTick at this tick)
    own: BTreeMap<i32, Vec<u32>>,
    theirs: BTreeMap<i32, Vec<(i32, Vec<u32>)>>,
    last: i32,
    pub error: Option<String>,
    pub hashes_checked: u64,
    pub resyncs: u32,
    /// What happened (holds, resyncs and their differing save sections, rejoins, migrations), for logs and tests.
    pub log: Vec<String>,
    status: Arc<Mutex<Status>>,
    epoch: Instant,
    rtt: BTreeMap<i32, f32>,
    max_rtt: f32,
    pinged: Instant,
    adapted: Instant,
    steps: Vec<f32>, // recent wall-clock ms between stepping RunTicks
    stepped: Option<Instant>,
    hold: Option<Hold>,
    lost: BTreeMap<i32, Instant>,     // host: dropped players and when we stop waiting for them
    migrate: Option<(Instant, i32)>,  // client: the host is gone (since, old host)
    retried: Instant,                 // the last reconnect attempt
    dead_host: Option<i32>,           // new host: the old one, kicked at the migration hold
    paused_by: i32,                   // the player who paused the game (0 = running)
    pauses: BTreeMap<i32, i32>,       // host: pauses used per player
    speed: i32,                       // the game speed everyone runs at (ticks/s; the lobby's at the start)
}

#[cfg(feature = "live")]
impl Lockstep {
    /// `tick` = the game's tick at the start (the same on every peer).
    pub fn new(net: Net, delay: i32, tick: i32) -> Lockstep {
        let active = net.lobby.player_numbers();
        let players = net.lobby.humans.iter().enumerate().map(|(k, h)| (net.lobby.player_of(k), h.name.clone())).collect();
        let teams = net.lobby.humans.iter().enumerate().map(|(k, h)| (net.lobby.player_of(k), h.team)).collect();
        let status = Arc::new(Mutex::new(Status { players, teams, me: net.me, chat: net.chat.clone(), delay, ..Default::default() }));
        let through = active.iter().map(|&p| (p, tick + delay - 1)).collect();
        let (spectator, speed) = (net.me >= SPECTATOR, net.lobby.speed);
        let mut ls = Lockstep { me: net.me, spectator, net, delay, adaptive: true, resync: true, hash_every: HASH_EVERY, rejoin_wait: REJOIN_WAIT, promised: tick + delay - 1,
            through, active, inbox: vec![], kicks: vec![], own: Default::default(), theirs: Default::default(), last: tick, error: None,
            hashes_checked: 0, resyncs: 0, log: vec![], status, epoch: Instant::now(), rtt: Default::default(), max_rtt: 0.0,
            pinged: Instant::now(), adapted: Instant::now(), steps: vec![], stepped: None, hold: None, lost: Default::default(), migrate: None, retried: Instant::now(),
            dead_host: None, paused_by: 0, pauses: Default::default(), speed };
        // the host of a loaded game / with spectators: they get its state at the first tick
        if ls.net.is_host() {
            let state: Vec<i32> = ls.net.peers.iter().map(|(p, _)| *p).filter(|&p| ls.net.lobby.loaded || p >= SPECTATOR).collect();
            if !state.is_empty() { ls.start_hold(tick, "the game state for the players joining", state); }
        }
        ls
    }

    /// A peer that gets its game from the host (spectator, player of a loaded game, a rejoining player that lost its
    /// game): page hashes of an empty memory, the host's pages, then the Go. Blocks up to `wait`.
    pub fn join(net: Net, delay: i32, wait: Duration) -> Result<(Lockstep, shc_sim::mem::Mem), String> {
        let mut m = shc_sim::mem::Mem::default();
        let mut ls = Lockstep::new(net, delay, 0);
        ls.hold = Some(Hold { at: 0, why: "getting the game".into(), state: [ls.me].into(), ..Default::default() });
        ls.net.send(&Msg::PageHashes { from: ls.me, hashes: page_hashes(&m) }, ls.me);
        ls.hold.as_mut().unwrap().sent = true;
        let t0 = Instant::now();
        while ls.hold.is_some() {
            if t0.elapsed() > wait { return Err("the host did not send the game".into()); }
            ls.poll();
            if let Some(e) = &ls.error { return Err(e.clone()); }
            ls.hold_client(&mut m, false);
            std::thread::sleep(Duration::from_millis(2));
        }
        let tick = m.i32(shc_sim::core::TICK_COUNTER);
        (ls.last, ls.stepped) = (tick, None);
        local_bytes(&mut m, if ls.spectator { 0 } else { ls.me }, false);
        Ok((ls, m))
    }

    #[cfg_attr(not(test), allow(dead_code))] // tests
    pub fn players(&self) -> &[i32] { &self.active }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_host(&self) -> bool { self.net.is_host() }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn sent_bytes(&self) -> u64 { self.net.sent_bytes }

    /// The UI's view of this game; keep a clone before the Lockstep moves into the sim thread.
    pub fn status(&self) -> Arc<Mutex<Status>> { self.status.clone() }

    fn now_ms(&self) -> u32 { self.epoch.elapsed().as_millis() as u32 }
    fn log(&mut self, s: String) { eprintln!("lan {}: {s}", self.me); self.log.push(s); }

    fn poll(&mut self) {
        let (say, pause, speed) = { let mut s = self.status.lock().unwrap(); (std::mem::take(&mut s.say), std::mem::take(&mut s.pause), s.speed.take()) };
        for (text, to) in say { self.net.say_to(&text, to); }
        if pause && !self.spectator {
            let on = self.paused_by == 0;
            if self.net.is_host() { self.pause(self.me, on) } else { self.net.send(&Msg::Pause { from: self.me, on }, self.me) }
        }
        if let Some(rate) = speed.filter(|_| !self.spectator) {
            if self.net.is_host() { self.set_speed(rate) } else { self.net.send(&Msg::Speed { rate }, self.me) }
        }
        let msgs = self.net.poll();
        { let mut s = self.status.lock().unwrap(); if s.chat.len() != self.net.chat.len() { s.chat = self.net.chat.clone(); } } // also while held
        if self.net.error.is_some() && !self.net.is_host() && self.migrate.is_none() && self.error.is_none() {
            // the host is gone: try it again, then host migration (`migrate_step`)
            let old = self.net.host_id;
            self.migrate = Some((Instant::now(), old));
            self.net.error = None;
            self.log(format!("lost the host (player {old})"));
        }
        for m in msgs {
            match m {
                Msg::Turn { from, through, packets } => {
                    let t = self.through.entry(from).or_insert(through);
                    *t = (*t).max(through);
                    self.inbox.extend(packets.into_iter().map(|p| (from, p)));
                }
                Msg::Hash { from, tick, sums } => {
                    self.theirs.entry(tick).or_default().push((from, sums));
                    self.compare(tick);
                }
                Msg::Left { who, at } if at < 0 => self.lost_player(who),
                Msg::Left { who, at } => {
                    self.active.retain(|&p| p != who);
                    self.through.remove(&who);
                    if !self.kicks.contains(&(who, at)) { self.kicks.push((who, at)); }
                    self.log(format!("player {who} leaves at tick {at}"));
                }
                Msg::Ping { from, t } => {
                    let pong = Msg::Pong { from: self.me, to: from, t };
                    if self.net.is_host() { self.net.send_to(from, &pong) } else { self.net.send(&pong, self.me) }
                }
                Msg::Pong { from, to, t } if to == self.me => {
                    let ms = self.now_ms().wrapping_sub(t) as f32;
                    let r = self.rtt.entry(from).or_insert(ms);
                    *r = *r * 0.7 + ms * 0.3;
                }
                Msg::Rtt { max_ms } => self.max_rtt = max_ms as f32,
                Msg::Hold { at, why, state } if !self.net.is_host() => {
                    let state: BTreeSet<i32> = state.into_iter().collect();
                    match &mut self.hold {
                        // the same hold, or a joiner's (at 0: its page hashes are out already)
                        Some(h) if h.at == at || h.at == 0 => { h.at = at; h.state.extend(state) }
                        _ => self.hold = Some(Hold { at, why: why.clone(), state, ..Default::default() }),
                    }
                    self.log(format!("hold at {at}: {why}"));
                }
                Msg::PageHashes { from, hashes } if self.net.is_host() => {
                    if let Some(h) = &mut self.hold { h.pending.push((from, hashes)) }
                }
                Msg::Pages { to, last, idx, lz } if to == self.me => {
                    if let Some(h) = &mut self.hold {
                        if h.chunks.is_empty() {
                            // everything that came before the host's state is in it
                            self.inbox.clear();
                        }
                        h.chunks.push((idx, lz));
                        h.done |= last;
                    }
                }
                Msg::Synced { from } if self.net.is_host() => { if let Some(h) = &mut self.hold { h.synced.insert(from); } }
                Msg::Go { at, through, active } if !self.net.is_host() => {
                    if let Some(h) = &mut self.hold { if h.at == at || h.at == 0 { h.go = Some((through, active)); } }
                }
                Msg::Pause { from, on } if self.net.is_host() => self.pause(from, on),
                Msg::Paused { by, left } if !self.net.is_host() => self.set_paused(by, left),
                Msg::Speed { rate } if self.net.is_host() => self.set_speed(rate),
                Msg::Speed { rate } => { self.speed = rate; self.status.lock().unwrap().rate = Some(rate); }
                Msg::Joined { who, tick, through } if self.net.is_host() => {
                    self.lost.remove(&who);
                    if who < SPECTATOR {
                        self.through.insert(who, through.max(*self.through.get(&who).unwrap_or(&0)));
                        if !self.active.contains(&who) { self.active.push(who); }
                    }
                    self.log(format!("{} {who} joined at their tick {tick}", if who >= SPECTATOR { "spectator" } else { "player" }));
                    let at = self.hold.as_ref().map_or(self.promised + 1, |h| h.at);
                    self.start_hold(at, if who >= SPECTATOR { "a spectator joins" } else { "a player comes back" }, vec![who]);
                }
                _ => {}
            }
        }
    }

    /// Host: stop everyone before the RunTick at `at` (no peer can be past it: they stop at our promise − 2), the
    /// peers in `state` get our state. Joins a hold that is already on.
    fn start_hold(&mut self, at: i32, why: &str, state: Vec<i32>) {
        let h = self.hold.get_or_insert_with(|| Hold { at, why: why.into(), ..Default::default() });
        h.state.extend(state);
        if !h.why.contains(why) { h.why = format!("{}; {why}", h.why); }
        let m = Msg::Hold { at: h.at, why: h.why.clone(), state: h.state.iter().copied().collect() };
        self.net.send(&m, self.me);
        let s = format!("hold at {}: {}", h.at, h.why);
        self.log(s);
    }

    /// Host: player `from` pauses the game for everyone (`on`; PAUSES times per game) or resumes it (any player): a
    /// hold without state transfer whose Go waits for the resume. The original has no player pause in multiplayer
    /// (Director.SetPausedState ignores it); this one is ours, outside the sim.
    fn pause(&mut self, from: i32, on: bool) {
        if !self.active.contains(&from) || on == (self.paused_by != 0) { return; }
        let used = self.pauses.get(&from).copied().unwrap_or(0);
        if on && used >= PAUSES {
            let text = "No pauses left (3 per game).".to_string();
            if from == self.me { self.net.chat.push((0, text)) } else { self.net.send_to(from, &Msg::Chat { from: 0, text }) }
            return;
        }
        if on {
            self.pauses.insert(from, used + 1);
            let at = self.hold.as_ref().map_or(self.promised + 1, |h| h.at);
            let name = self.status.lock().unwrap().players.iter().find(|q| q.0 == from).map_or(format!("player {from}"), |q| q.1.clone());
            self.start_hold(at, &format!("paused by {name}"), vec![]);
        }
        let (by, left) = (if on { from } else { 0 }, PAUSES - self.pauses.get(&from).copied().unwrap_or(0));
        self.net.send(&Msg::Paused { by, left }, self.me);
        self.set_paused(by, left);
    }

    /// Host: a player's speed request, 10..90 by 5 like the original's MP: everyone runs at it from now on. Ours (the
    /// original keeps the lobby's speed in multiplayer); pacing only, the sim never sees it. The lockstep gate holds
    /// every peer to the slowest, so one shared speed is the only one that takes effect.
    fn set_speed(&mut self, rate: i32) {
        let rate = (rate / 5 * 5).clamp(10, 90);
        if rate == self.speed { return; }
        self.speed = rate;
        self.net.send(&Msg::Speed { rate }, self.me);
        self.status.lock().unwrap().rate = Some(rate);
        self.log(format!("game speed {rate}"));
    }

    fn set_paused(&mut self, by: i32, left: i32) {
        self.paused_by = by;
        self.status.lock().unwrap().paused = (by != 0).then_some((by, left));
        self.log(if by != 0 { format!("paused by player {by} ({left} left)") } else { "resumed".into() });
    }

    /// Host: player `who`'s connection dropped: everyone stops at its promise − 1 (the last tick the gate lets anyone
    /// reach without it) and waits up to `rejoin_wait`.
    fn lost_player(&mut self, who: i32) {
        if !self.active.contains(&who) { return; }
        self.lost.insert(who, Instant::now() + self.rejoin_wait);
        let at = self.through.get(&who).copied().unwrap_or(self.promised) - 1;
        self.start_hold(at, &format!("waiting for player {who} to come back"), vec![]);
    }

    /// Wall-clock pings to measure round trips; the delay follows them (`adaptive`).
    fn timing(&mut self) {
        if self.pinged.elapsed() > Duration::from_millis(500) {
            self.pinged = Instant::now();
            let ping = Msg::Ping { from: self.me, t: self.now_ms() };
            self.net.send(&ping, self.me);
            if self.net.is_host() {
                let max = self.rtt.values().fold(0.0f32, |a, &b| a.max(b));
                self.net.send(&Msg::Rtt { max_ms: max as u32 }, self.me);
            }
        }
        if !self.adaptive || self.adapted.elapsed() < Duration::from_millis(500) { return; }
        self.adapted = Instant::now();
        // our chores → host → the farthest client. The round trips include the peers' tick processing (pings are
        // answered between RunTicks): what a chore really waits for
        let far = if self.net.is_host() { self.rtt.values().fold(0.0f32, |a, &b| a.max(b)) } else { self.max_rtt };
        let to_host = if self.net.is_host() { 0.0 } else { self.rtt.values().next().copied().unwrap_or(far) };
        let path = to_host / 2.0 + far.max(to_host) / 2.0;
        let tick_ms = self.steps.iter().copied().fold(f32::MAX, f32::min).clamp(4.0, 200.0);
        let target = (MIN_DELAY + (path * 1.5 / tick_ms).ceil() as i32 + 1).clamp(MIN_DELAY, MAX_DELAY);
        // up at once (never unsafe), down one tick per half second
        self.delay = if target > self.delay { target } else if target < self.delay { self.delay - 1 } else { self.delay };
        if std::env::var_os("SHC_LAN_DEBUG").is_some() { eprintln!("lan {}: rtt {:?} max {} tick_ms {tick_ms} target {target} delay {}", self.me, self.rtt, self.max_rtt, self.delay); }
        let mut s = self.status.lock().unwrap();
        (s.delay, s.rtt_ms) = (self.delay, path * 2.0);
    }

    /// Before the RunTick at `tick` (see the module doc); runs holds and host migration. Takes the engine memory
    /// because a hold may read or replace it.
    pub fn ready(&mut self, m: &mut shc_sim::mem::Mem) -> bool {
        self.poll();
        self.timing();
        let tick = m.i32(shc_sim::core::TICK_COUNTER);
        if self.migrate_step(m, tick) { return false; }
        if self.net.is_host() {
            let now = Instant::now();
            let gone: Vec<i32> = self.lost.iter().filter(|(_, &d)| d < now).map(|(&p, _)| p).collect();
            for p in gone {
                self.lost.remove(&p);
                let at = self.hold.as_ref().map_or(tick, |h| h.at);
                self.net.send(&Msg::Left { who: p, at }, self.me);
                self.active.retain(|&q| q != p);
                self.through.remove(&p);
                self.kicks.push((p, at));
                if let Some(h) = &mut self.hold { h.state.remove(&p); }
                self.log(format!("player {p} did not come back: removed at tick {at}"));
            }
        }
        if self.hold.as_ref().is_some_and(|h| tick >= h.at) {
            if self.net.is_host() { self.hold_host(m, tick) } else { self.hold_client(m, true) }
            self.status.lock().unwrap().holding = self.hold.as_ref().map(|h| h.why.clone());
            return false;
        }
        let waiting: Vec<i32> = self.active.iter().copied().filter(|&p| p != self.me && self.through.get(&p).is_none_or(|&t| t < tick + 2)).collect();
        let ok = self.error.is_none() && waiting.is_empty() && self.kicks.iter().all(|&(_, at)| at >= tick);
        let mut s = self.status.lock().unwrap();
        s.waiting_since = if waiting.is_empty() { None } else { s.waiting_since.or(Some(Instant::now())) };
        (s.error, s.waiting_for, s.holding) = (self.error.clone(), waiting, None);
        if s.chat.len() != self.net.chat.len() { s.chat = self.net.chat.clone(); }
        ok
    }

    /// Host at a hold: everything received goes in first (it is all in our state then), then answer page hashes with
    /// the differing pages; Go once every state peer applied it and nobody is still awaited.
    fn hold_host(&mut self, m: &mut shc_sim::mem::Mem, tick: i32) {
        let now = self.now_ms();
        if !self.inbox.is_empty() || !self.hold.as_ref().unwrap().sent {
            for (p, pk) in std::mem::take(&mut self.inbox) { shc_sim::mp::receive_chore(m, p, &pk); }
            shc_sim::mp::f235f0(m, now);
        }
        let h = self.hold.as_mut().unwrap();
        if !h.sent {
            h.sent = true;
            // kicks due now and the old host of a migration happen before the state goes out
            for &(p, _) in self.kicks.iter().filter(|&&(_, at)| at <= tick) { shc_sim::mp::kick(m, p); }
            self.kicks.retain(|&(_, at)| at > tick);
            if let Some(old) = self.dead_host.take() {
                shc_sim::mp::kick(m, old);
                shc_sim::mp::promote_host(m, self.me, now);
                h.kick = Some(old);
            }
        }
        let pending = std::mem::take(&mut h.pending);
        if !pending.is_empty() {
            let mine = page_hashes(m);
            for (from, theirs) in pending {
                let idx: Vec<u32> = (0..mine.len()).filter(|&i| theirs.get(i) != Some(&mine[i])).map(|i| i as u32).collect();
                let secs = sections_of(&idx);
                let msgs = page_messages(m, from, &idx);
                let bytes: usize = msgs.iter().map(|x| if let Msg::Pages { lz, .. } = x { lz.len() } else { 0 }).sum();
                for x in msgs { self.net.send_to(from, &x); }
                let h = self.hold.as_mut().unwrap();
                h.pages_sent += idx.len();
                self.log(format!("state to {from}: {} pages ({} KB lz4), save sections {:?}", idx.len(), bytes / 1024, &secs[..secs.len().min(24)]));
            }
        }
        let h = self.hold.as_ref().unwrap();
        let waiting_lost = self.lost.keys().any(|p| self.active.contains(p));
        if !waiting_lost && self.paused_by == 0 && h.state.iter().all(|p| h.synced.contains(p)) {
            let mut through: Vec<(i32, i32)> = self.through.iter().map(|(&p, &t)| (p, t)).collect();
            through.push((self.me, self.promised));
            let at = h.at;
            let go = Msg::Go { at, through, active: self.active.clone() };
            self.net.send(&go, self.me);
            self.net.send(&Msg::Speed { rate: self.speed }, self.me); // joiners start at the lobby's
            let why = h.why.clone();
            if !h.state.is_empty() && why.contains("desync") { self.resyncs += 1; }
            self.hold = None;
            self.own.retain(|&t, _| t < at);
            self.theirs.retain(|&t, _| t < at);
            self.log(format!("go at {at} ({why})"));
        }
    }

    /// Client at a hold: our outgoing chores are dropped (our state will be replaced), page hashes out once, the pages
    /// in, Synced, then wait for Go. `running`: the game exists already (keep our OWN memory).
    fn hold_client(&mut self, m: &mut shc_sim::mem::Mem, running: bool) {
        let me = self.me;
        let h = self.hold.as_mut().unwrap();
        let mine = h.state.contains(&me);
        if !h.sent {
            h.sent = true;
            if mine {
                shc_sim::mp::take_outgoing(m);
                let hashes = page_hashes(m);
                self.net.send(&Msg::PageHashes { from: me, hashes }, me);
            }
        }
        let h = self.hold.as_mut().unwrap();
        if mine && h.done && !h.applied {
            let chunks = std::mem::take(&mut h.chunks);
            match apply_pages(m, &chunks, running) {
                Ok(n) => {
                    h.applied = true;
                    local_bytes(m, if self.spectator { 0 } else { me }, false);
                    shc_sim::mp::take_outgoing(m);
                    self.net.send(&Msg::Synced { from: me }, me);
                    self.log(format!("applied {n} pages of the host's state"));
                }
                Err(e) => self.error = Some(e),
            }
        }
        let h = self.hold.as_mut().unwrap();
        if let Some((through, active)) = h.go.take() {
            if !mine || h.applied {
                let at = h.at;
                for (p, t) in through { if p != me { self.through.insert(p, t); } else { self.promised = self.promised.max(t); } }
                self.active = active;
                if mine && h.why.contains("desync") { self.resyncs += 1; }
                self.hold = None;
                self.own.retain(|&t, _| t < at);
                self.theirs.retain(|&t, _| t < at);
                self.log(format!("go at {at}"));
            } else {
                h.go = Some((through, active));
            }
        }
    }

    /// Client without its host: try the old host for RECONNECT_OLD, then the lowest remaining player hosts and the
    /// others connect to it. True while migrating (no ticks).
    fn migrate_step(&mut self, m: &mut shc_sim::mem::Mem, tick: i32) -> bool {
        let Some((since, old)) = self.migrate else { return false };
        let addr_of = |net: &Net, p: i32| net.addrs.iter().find(|(q, _)| *q == p).map(|(_, a)| *a);
        if since.elapsed() < RECONNECT_OLD {
            if let Some(a) = self.net.host_addr {
                let due = self.retried.elapsed() > Duration::from_millis(400);
                if due { self.retried = Instant::now(); }
                if due && self.net.reconnect(a, old, tick, self.promised) {
                    self.migrate = None;
                    self.log("back with the host".into());
                    return false;
                }
            }
            return true;
        }
        let new = self.active.iter().copied().filter(|&p| p != old).min();
        let Some(new) = new else { self.error = Some("The host left the game.".into()); self.migrate = None; return true };
        if new == self.me {
            self.net.become_host();
            self.active.retain(|&p| p != old);
            self.through.remove(&old);
            self.dead_host = Some(old);
            self.migrate = None;
            self.log(format!("now the host (player {old} left)"));
            let others: Vec<i32> = self.active.iter().copied().filter(|&p| p != self.me).collect();
            for &p in &others { self.lost.insert(p, Instant::now() + MIGRATE_WAIT); }
            // everyone (once back) gets our state: the old host's last chores may have reached some peers only
            let at = self.promised.max(tick) + 1;
            self.start_hold(at, "a new host (the host left)", vec![]);
            if others.is_empty() {
                // alone now: remove the old host at once
                shc_sim::mp::kick(m, old);
                shc_sim::mp::promote_host(m, self.me, self.now_ms());
                self.dead_host = None;
                self.hold = None;
            }
            return false;
        }
        if since.elapsed() > RECONNECT_OLD + MIGRATE_WAIT {
            self.error = Some("The host left the game.".into());
            self.migrate = None;
            return true;
        }
        if let Some(a) = addr_of(&self.net, new) {
            let due = self.retried.elapsed() > Duration::from_millis(400);
            if due { self.retried = Instant::now(); }
            if due && self.net.reconnect(a, new, tick, self.promised) {
                self.migrate = None;
                self.active.retain(|&p| p != old);
                self.through.remove(&old);
                self.log(format!("joined the new host, player {new}"));
            }
        }
        true
    }

    /// Before a RunTick: the peers' chores into DLL_ReceiveChore, due kicks, the exec-tick timing (never below our
    /// promise). A spectator drops anything its own UI queued.
    pub fn before(&mut self, m: &mut shc_sim::mem::Mem) {
        self.poll();
        for (p, pk) in std::mem::take(&mut self.inbox) {
            shc_sim::mp::receive_chore(m, p, &pk);
        }
        let tick = m.i32(shc_sim::core::TICK_COUNTER);
        let due: Vec<i32> = self.kicks.iter().filter(|&&(_, at)| at <= tick).map(|&(p, _)| p).collect();
        for p in due {
            shc_sim::mp::kick(m, p);
            self.net.chat.push((0, format!("player {p} left the game")));
        }
        self.kicks.retain(|&(_, at)| at > tick);
        if self.spectator { shc_sim::mp::drop_chores_of(m, 0); }
        shc_sim::mp::set_timing_min(m, self.delay, self.promised + 1);
    }

    /// After a RunTick: the outgoing chores and our promise to the peers; a sync check every `hash_every` ticks.
    pub fn after(&mut self, m: &mut shc_sim::mem::Mem) {
        let tick = m.i32(shc_sim::core::TICK_COUNTER);
        self.status.lock().unwrap().colours = crate::replay::colours(m);
        if shc_sim::mp::late(m) > 0 && self.error.is_none() {
            self.error = Some(format!("Lockstep error: a chore arrived after its tick ({tick})"));
        }
        let packets: Vec<Vec<u8>> = shc_sim::mp::take_outgoing(m).into_iter().map(|(_, p)| p).collect();
        // during a hold whose state replaces ours, what we queued is gone with it: not sent
        let replaced = self.hold.as_ref().is_some_and(|h| h.state.contains(&self.me) && !h.applied);
        if !self.spectator {
            self.promised = self.promised.max(shc_sim::mp::next_exec(m) - 1);
            let packets = if replaced { vec![] } else { packets };
            self.net.send(&Msg::Turn { from: self.me, through: self.promised, packets }, self.me);
        }
        if tick != self.last {
            if let Some(t) = self.stepped { self.steps.push(t.elapsed().as_secs_f32() * 1000.0); }
            if self.steps.len() > 40 { self.steps.remove(0); }
            self.stepped = Some(Instant::now());
            if tick % self.hash_every == 0 {
                // the original's sync check (FUN_180021ff0): per-part checksums that leave out what differs by design
                if let Some(sums) = shc_sim::mp::sync_check(m) {
                    if !self.spectator { self.net.send(&Msg::Hash { from: self.me, tick, sums: sums.to_vec() }, self.me); }
                    self.own.insert(tick, sums.to_vec());
                    self.compare(tick);
                }
            }
        }
        self.last = tick;
    }

    fn compare(&mut self, tick: i32) {
        let Some(mine) = self.own.get(&tick).cloned() else { return };
        for (p, s) in self.theirs.remove(&tick).unwrap_or_default() {
            self.hashes_checked += 1;
            if s == mine { continue; }
            let parts = shc_sim::mp::SYNC_PARTS;
            let bad: Vec<&str> = mine.iter().zip(&s).zip(parts).filter(|((a, b), _)| a != b).map(|(_, n)| n).collect();
            let why = format!("desync at tick {tick} with player {p}: {} differ", bad.join(", "));
            if self.resync && self.net.is_host() {
                if self.hold.is_none() {
                    self.log(why.clone());
                    let state: Vec<i32> = self.net.peers.iter().map(|(q, _)| *q).collect();
                    let at = self.promised.max(tick) + 1;
                    self.start_hold(at, &format!("resync after a {why}"), state);
                }
            } else if self.resync {
                self.log(format!("{why} (the host resyncs)"));
            } else if self.error.is_none() {
                self.error = Some(format!("Desync at tick {tick} with player {p}: {} differ", bad.join(", ")));
            }
        }
        // keep a few ticks for late hashes, drop the rest
        while self.own.len() > 64 { let k = *self.own.keys().next().unwrap(); self.own.remove(&k); }
    }
}

/// The running LAN game's status for the UI thread (`App::lan_tick`); one game per app.
static UI: Mutex<Option<Arc<Mutex<Status>>>> = Mutex::new(None);
/// The LAN game's chat line and messages (UI thread).
static CHAT: Mutex<ChatUi> = Mutex::new(ChatUi::NEW);

/// The in-game chat (HUD_MPChatPanel / HUD_MPChatMessages): the OpenChat key opens the panel (ToggleMultiplayerChat),
/// typed keys fill its line, Return / Send sends it to the selected players, an insult button sends that insult,
/// Escape / Exit closes it. Messages: the last 5 for 20 s each (received insults 10 s; all 5 while the panel is open),
/// the sender's name in its colour; muted players, Mute All Multiplayer Chat and the MuteInsults / MuteInsultSpeech
/// options filter what arrives (Platform_Multiplayer case 5 / 6).
pub(crate) struct ChatUi {
    pub open: bool,
    pub line: String,
    log: Vec<(Instant, u64, i32, String)>, // shown messages: when, how long (s), sender, text ("\u{1}n" = insult n)
    seen: usize,                            // Status.chat entries taken
    to: [bool; 9],                          // multiplayerChatSelectedPlayers (InitNewMPGame: all)
    muted: [bool; 9],                       // Platform_Multiplayer.IsChatMute per player
    off: bool,                              // MPChatMuted (Mute All Multiplayer Chat)
    hits: Vec<([f32; 4], usize)>,           // the panel's click targets of the last frame (chat_panel.rs codes)
    rect: Option<[f32; 4]>,                 // where the panel was drawn last
}

impl Default for ChatUi { fn default() -> Self { Self::NEW } }

impl ChatUi {
    const NEW: ChatUi = ChatUi { open: false, line: String::new(), log: Vec::new(), seen: 0, to: [true; 9], muted: [false; 9], off: false, hits: Vec::new(), rect: None };

    /// Messages of `st` not seen yet into the log, unless muted; an insult plays its speech (Insult1..20.wav).
    fn sync(&mut self, st: &Status) {
        let now = Instant::now();
        for (p, t) in st.chat.iter().skip(self.seen) {
            let own = *p == st.me || *p <= 0;
            if !own && (self.off || self.muted.get(*p as usize).copied().unwrap_or(false)) { continue; }
            match insult(t) {
                Some(n) => {
                    if crate::menu::opt("MuteInsults", false) { continue; }
                    if !crate::menu::opt("MuteInsultSpeech", false) { crate::audio::speech(1, &insult_wav(n)); }
                    self.log.push((now, if own { 20 } else { 10 }, *p, t.clone()));
                }
                None => self.log.push((now, 20, *p, t.clone())),
            }
        }
        self.seen = st.chat.len();
        let n = self.log.len().saturating_sub(5); // HUD_MPChatMessages keeps 5
        self.log.drain(..n);
    }

    /// The messages shown, oldest first: (sender, text); sender 0 = the game.
    pub fn shown(&self) -> Vec<(i32, String)> {
        self.log.iter().filter(|(t, d, ..)| self.open || t.elapsed() < Duration::from_secs(*d)).map(|(_, _, p, s)| (*p, s.clone())).collect()
    }

    /// The selected players (multiplayerIngameChatRecipients) as Msg::Say's mask.
    fn mask(&self, st: &Status) -> u32 {
        st.players.iter().filter(|(p, _)| self.to.get(*p as usize).copied().unwrap_or(false)).fold(0, |m, (p, _)| m | 1 << p)
    }

    /// A key press while the panel is open (Unity key name with modifiers, the typed text): Return sends the line to
    /// the selected players (DetectEnterTextBox: not while chat is muted) and closes the panel.
    pub fn key(&mut self, st: &Mutex<Status>, k: &str, c: Option<&str>) {
        match k.split(',').next().unwrap_or("") {
            "Return" | "KeypadEnter" => {
                let line = std::mem::take(&mut self.line);
                let mut s = st.lock().unwrap();
                let to = self.mask(&s);
                if !line.trim().is_empty() && !self.off && to != 0 { s.say.push((line.trim().to_string(), to)); }
                self.open = false;
            }
            "Backspace" | "Delete" => { self.line.pop(); }
            "Space" => self.line.push(' '),
            _ => if let Some(c) = c.filter(|c| !k.contains(",ctrl") && !c.chars().any(char::is_control)) {
                if self.line.chars().count() < 100 { self.line.push_str(c); } // MaxLength 100
            },
        }
    }

    /// HUD_MPChatPanel.ButtonClicked for a click target (chat_panel.rs codes).
    fn click(&mut self, st: &Mutex<Status>, code: usize) {
        use crate::menu::chat_panel as cp;
        let mut s = st.lock().unwrap();
        let teams = s.teams.clone();
        let team = |p: i32| teams.iter().find(|t| t.0 == p).map_or(100 + p, |t| if t.1 > 0 { t.1 } else { 100 + p });
        let mine = team(s.me);
        let players: Vec<i32> = s.players.iter().map(|p| p.0).collect();
        match code {
            c if (cp::PLAYER..cp::PLAYER + 8).contains(&c) => if let Some(&p) = players.get(c - cp::PLAYER) { self.to[p.clamp(0, 8) as usize] ^= true },
            c if (cp::MUTE..cp::MUTE + 8).contains(&c) => if let Some(&p) = players.get(c - cp::MUTE) { self.muted[p.clamp(0, 8) as usize] ^= true },
            cp::ALL => self.to = [true; 9],
            cp::TEAM => for p in 1..9 { self.to[p] = team(p as i32) == mine },
            cp::ENEMIES => for p in 1..9 { self.to[p] = team(p as i32) != mine },
            c if (cp::INSULT + 1..=cp::INSULT + 20).contains(&c) => {
                let to = self.mask(&s);
                if to != 0 { s.say.push((format!("\u{1}{}", c - cp::INSULT), to)); }
            }
            cp::SEND => {
                let to = self.mask(&s);
                let line = std::mem::take(&mut self.line);
                if !line.trim().is_empty() && !self.off && to != 0 { s.say.push((line.trim().to_string(), to)); }
            }
            cp::EXIT => self.open = false,
            c if (cp::CHECK..cp::CHECK + 3).contains(&c) => {
                let k = c - cp::CHECK;
                if k == 2 { self.off ^= true; return; }
                let key = ["MuteInsults", "MuteInsultSpeech"][k];
                if let Err(e) = crate::menu::set_opt(key, !crate::menu::opt(key, false)) { eprintln!("settings.cfg: {e}"); }
            }
            _ => {}
        }
    }
}

/// An insult line: "\u{1}" + its number 1..20 (nothing a player can type: the line takes no control characters).
fn insult(t: &str) -> Option<usize> { t.strip_prefix('\u{1}').and_then(|n| n.parse().ok()).filter(|n| (1..=20).contains(n)) }

/// SFXManager.insultSpeech: Insult1..5.wav, insult6..10.wav, Insult11..20.wav.
fn insult_wav(n: usize) -> String { if (6..=10).contains(&n) { format!("insult{n}.wav") } else { format!("Insult{n}.wav") } }

const CREAM: [f32; 4] = [0xef as f32 / 255.0, 0xf3 as f32 / 255.0, 0xc6 as f32 / 255.0, 1.0];

/// Player `p`'s chat colour: HUD_MPChatMessages.MPTeamColours[remapColours[p]] (`colours`, Status::colours); 0 = the
/// game's messages (gold).
fn chat_colour(p: i32, colours: &[u8; 9]) -> [f32; 4] {
    const C: [[u8; 3]; 9] = [[255, 172, 43], [196, 2, 2], [70, 70, 200], [200, 97, 6], [198, 195, 0], [144, 0, 144], [128, 128, 128], [9, 193, 191], [2, 200, 2]];
    let c = C[if p <= 0 { 0 } else { crate::endgame::colour(colours, p.min(8) as u8) as usize }];
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]
}

/// main.rs, every frame of a LAN game: the chat messages (HUD_MPChatMessages, bottom left above the goods), the chat
/// panel while open, and in the middle why the game stands: paused by whom (OST_GAME_PAUSED's text), another hold's
/// reason, or a slow peer ("Communication delay. Waiting for other players.", TEXT_MULTIPLAYER_CONNECTION_024).
pub(crate) fn overlay(u: &mut crate::hud::Ui, cursor: [f32; 2]) -> Vec<crate::hud::HudTri> {
    let Some(st) = UI.lock().unwrap().clone() else { return vec![] };
    let st = st.lock().unwrap().clone();
    let mut chat = CHAT.lock().unwrap();
    chat.sync(&st);
    let name = |p: i32| st.players.iter().find(|q| q.0 == p).map_or(format!("player {p}"), |q| q.1.clone());
    let text = |u: &crate::hud::Ui, t: &str| insult(t).map_or(t.to_string(), |n| u.text_of(&format!("TEXT_INSULTS_{:03}", n + 1)));
    let (s, w, h, mut out) = (u.s, u.w, u.h, vec![]);
    // MainHUD HUD_MPChatMessages: left, margin bottom 160; a row of 27 per message, oldest on top; 16 px, the name
    // (its colour) + " " + the text (#DDDDDD) on #99000000 as wide as the line
    let shown = chat.shown();
    let asc = 16.0 * 0.8;
    for (k, (p, t)) in shown.iter().enumerate() {
        let top = h - (160.0 + 27.0 * (shown.len() - k) as f32) * s;
        let who = if *p == 0 { String::new() } else { format!("{} ", name(*p)) };
        let t = text(u, t);
        let tw = u.text(&mut vec![], &format!("{who}{t}"), 0.0, 0.0, 16.0, 1.0, 0.0, 0.0, CREAM); // (measure)
        crate::hud::Ui::fill(&mut out, [0.0, top, tw.min(800.0 * s), top + 27.0 * s], [0.0, 0.0, 0.0, 0x99 as f32 / 255.0]);
        let y = top + (5.0 + asc) * s;
        let nw = u.text(&mut out, &who, 0.0, y, 16.0, 1.0, 0.0, 0.0, chat_colour(*p, &st.colours));
        u.text(&mut out, &t, nw, y, 16.0, 1.0, 0.0, 0.0, if *p == 0 { chat_colour(0, &st.colours) } else { [0.867, 0.867, 0.867, 1.0] });
    }
    // HUD_MPChatPanel
    if chat.open {
        use crate::menu::chat_panel as cp;
        let lin = |c: [f32; 4]| [c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2), 1.0]; // the menu draws in linear colour
        let teams = |p: i32| st.teams.iter().find(|t| t.0 == p).map_or(100 + p, |t| if t.1 > 0 { t.1 } else { 100 + p });
        let mine = teams(st.me);
        let hover = chat.hits.iter().find(|(r, _)| cursor[0] >= r[0] && cursor[0] < r[2] && cursor[1] >= r[1] && cursor[1] < r[3]).map(|h| h.1);
        let v = cp::View {
            players: st.players.iter().map(|(p, n)| (*p, n.clone(), lin(chat_colour(*p, &st.colours)), chat.to[(*p).clamp(0, 8) as usize], chat.muted[(*p).clamp(0, 8) as usize])).collect(),
            team: st.players.iter().filter(|(p, _)| teams(*p) == mine).count() >= 2,
            line: chat.line.clone(),
            insult: hover.filter(|c| (cp::INSULT + 1..=cp::INSULT + 20).contains(c)).map(|c| u.text_of(&format!("TEXT_INSULTS_{:03}", c - cp::INSULT + 1))),
            send: !chat.line.trim().is_empty() && chat.mask(&st) != 0 && !chat.off,
            checks: [crate::menu::opt("MuteInsults", false), crate::menu::opt("MuteInsultSpeech", false), chat.off],
        };
        let (tris, hits) = cp::draw(u, &v, w, h, cursor);
        out.extend(tris);
        (chat.hits, chat.rect) = (hits, Some(cp::rect(w, h)));
    } else { (chat.hits, chat.rect) = (vec![], None); }
    let long = st.waiting_since.is_some_and(|t| t.elapsed() > Duration::from_secs(1));
    let centre = match (&st.paused, &st.holding) {
        (Some((by, left)), _) => Some(format!("{} - {} ({left} left)", u.text_of("TEXT_FEEDBACK_023"), name(*by))),
        (None, Some(why)) => Some(why.clone()),
        (None, None) if long => Some(u.text_of("TEXT_MULTIPLAYER_CONNECTION_024")),
        _ => None,
    };
    if let Some(t) = centre { u.text_sh(&mut out, &t, w / 2.0, 220.0 * s, 32.0, 0.5, CREAM); }
    out
}

#[cfg(feature = "live")]
impl crate::App {
    /// menu.rs Go::Lan: every player builds the same MP-1 game (`game`), or the host loads the saved game, or the peer
    /// gets the host's state (spectators, players of a loaded game); the Lockstep runs it at the lobby's speed.
    pub(crate) fn lan_start(&mut self, s: crate::menu::mp_menu::Started) {
        let crate::menu::mp_menu::Started { net, lobby, seed, delay } = s;
        let (state, save) = (net.state, net.save.clone());
        let me = net.me;
        let r: Result<(shc_sim::mem::Mem, Option<Lockstep>), String> = if state {
            Lockstep::join(net, delay, Duration::from_secs(60)).map(|(ls, m)| (m, Some(ls)))
        } else if let Some(path) = save {
            let m = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display())).and_then(|f| load_game(&f, &lobby, me));
            m.map(|m| { let t = m.i32(shc_sim::core::TICK_COUNTER); (m, Some(Lockstep::new(net, delay, t))) })
        } else {
            game(&lobby, me, seed).map(|m| {
                let ls = Lockstep::new(net, delay, m.i32(shc_sim::core::TICK_COUNTER));
                (m, Some(ls))
            })
        };
        match r {
            Ok((m, Some(ls))) => {
                (self.menu, self.last_lobby, self.campaign) = (None, None, None);
                let mut l = crate::live::Live::from_mem(m);
                if ls.spectator { l.me = 0; }
                ls.status().lock().unwrap().cfg_speed = crate::skirmish::Settings::load().game_speed() as i32;
                *UI.lock().unwrap() = Some(ls.status());
                *CHAT.lock().unwrap() = ChatUi::default();
                l.net = Some(ls);
                self.start_live(l, None);
                self.trail = lobby.coop.map(|(t, k, _)| (20 + t, k, 1, false)); // co-op trail: win → progress, back to its map
                if let Some(p) = &mut self.play {
                    p.rate = lobby.speed as f32;
                    if let Some(t) = &p.live { t.send(crate::live::Cmd::Run { playing: true, rate: p.rate }); }
                }
            }
            Ok((_, None)) => {}
            Err(e) => { let mut m = self.menu.take().unwrap_or_else(crate::menu::Menu::main); m.error = Some(e); self.menu = Some(m); }
        }
    }

    /// main.rs key_event: while the chat line is open it takes the key presses; the OpenChat key (KeyManager: Return in
    /// multiplayer, keypad Enter) opens it. True = the key is taken.
    pub(crate) fn lan_key(&mut self, k: &str, c: Option<&str>, pressed: bool) -> bool {
        let Some(st) = UI.lock().unwrap().clone() else { return false };
        let mut chat = CHAT.lock().unwrap();
        if chat.open {
            if pressed && !self.mods.super_key() { chat.key(&st, k, c); }
            return pressed; // releases still end held scroll keys
        }
        let open = pressed && self.keys.get("OpenChat").iter().any(|b| !b.is_empty() && b.trim_end_matches(",mp") == k);
        chat.open |= open;
        open
    }

    /// main.rs left press at window px `c`: a click on the open chat panel (HUD_MPChatPanel buttons). True = taken.
    pub(crate) fn lan_click(&mut self, c: [f32; 2]) -> bool {
        let Some(st) = UI.lock().unwrap().clone() else { return false };
        let mut chat = CHAT.lock().unwrap();
        let inside = |r: &[f32; 4]| c[0] >= r[0] && c[0] < r[2] && c[1] >= r[1] && c[1] < r[3];
        if !chat.rect.as_ref().is_some_and(inside) { return false; }
        if let Some(code) = chat.hits.iter().find(|(r, _)| inside(r)).map(|h| h.1) { chat.click(&st, code); }
        true
    }

    /// main.rs escape: Escape closes an open chat line first (KeyManager: MPChatVisible = false).
    pub(crate) fn lan_escape(&mut self) -> bool {
        let mut c = CHAT.lock().unwrap();
        if !c.open { return false; }
        (c.open, c.line) = (false, String::new());
        true
    }

    /// main.rs hotkey "Pause": in a LAN game it pauses or resumes everyone (Lockstep::pause). True = handled.
    pub(crate) fn lan_pause(&mut self) -> bool {
        UI.lock().unwrap().as_ref().map(|st| st.lock().unwrap().pause = true).is_some()
    }
}

/// main.rs speed keys: in a LAN game ask the host for game speed `rate` (Lockstep::set_speed; it comes back as
/// Status.rate). True = handled.
pub(crate) fn ask_speed(rate: f32) -> bool {
    UI.lock().unwrap().as_ref().map(|st| st.lock().unwrap().speed = Some(rate as i32)).is_some()
}

/// main.rs Options changed: in a LAN game only a moved GameSpeed slider asks for a speed (another option keeps the
/// game's). True = in a LAN game.
pub(crate) fn speed_setting(cfg: f32) -> bool {
    let Some(st) = UI.lock().unwrap().clone() else { return false };
    let mut s = st.lock().unwrap();
    if s.cfg_speed != cfg as i32 { (s.cfg_speed, s.speed) = (cfg as i32, Some(cfg as i32)); }
    true
}

#[cfg(feature = "live")]
impl crate::App {

    /// Every frame: the menu's LAN session (a client's game start), a LAN game that stopped (desync without resync,
    /// host gone for good: back to the menu with the reason), what the game waits for in the window title.
    pub(crate) fn lan_tick(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        if let Some(go) = self.menu.as_mut().and_then(|m| m.mp_poll()) { self.menu_go(go, el); }
        let Some(st) = UI.lock().unwrap().clone() else { return };
        let Some(p) = &mut self.play else { *UI.lock().unwrap() = None; return };
        let s = { let mut s = st.lock().unwrap(); let c = s.clone(); s.rate = None; c };
        if let Some(r) = s.rate { p.rate = r as f32; } // the host set everyone's game speed (Lockstep::set_speed)
        if let Some(e) = s.error {
            *UI.lock().unwrap() = None;
            self.menu_go(crate::menu::Go::ToMenu, el);
            if let Some(m) = &mut self.menu { m.error = Some(e); }
            return;
        }
        // "waiting for …" / the hold's reason in the window title, set only when it changes (macOS set_title blocks)
        static SHOWN: Mutex<String> = Mutex::new(String::new());
        let long = s.waiting_since.is_some_and(|t| t.elapsed() > Duration::from_secs(1));
        let waiting: Vec<&str> = s.waiting_for.iter().filter_map(|p| s.players.iter().find(|q| q.0 == *p).map(|q| q.1.as_str())).collect();
        let text = match &s.holding {
            Some(why) => why.clone(),
            None if long && !waiting.is_empty() => format!("waiting for {}", waiting.join(", ")),
            None => String::new(),
        };
        let mut shown = SHOWN.lock().unwrap();
        if *shown != text {
            if let Some(g) = &self.gfx { g.window.set_title(&format!("Strong Hold{}{text}", if text.is_empty() { "" } else { " - " })); }
            *shown = text;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let mut l = Lobby::new("host");
        l.humans.push(Human { name: "zwei".into(), team: 2, lord: 5, coa: "0222110191010".into() });
        l.ais.push((4, 1));
        (l.coop, l.slots, l.spectators, l.loaded) = (Some((2, 7, true)), vec![1, 3], vec!["watcher".into()], true);
        let a: SocketAddr = "192.168.1.20:47790".parse().unwrap();
        for m in [Msg::Hello { name: "a".into(), version: 2, listen: 51000, join: Join::Rejoin(3), tick: 77, through: 90, coa: "0222110191010".into() }, Msg::Lobby(l.clone()),
            Msg::Start { lobby: l, seed: -7, delay: 10, state: true }, Msg::Peers(vec![(2, a)]),
            Msg::Turn { from: 2, through: 77, packets: vec![vec![1, 2, 3], vec![]] }, Msg::Hash { from: 1, tick: 40, sums: vec![1, u32::MAX] },
            Msg::Left { who: 2, at: 99 }, Msg::Chat { from: 1, text: "hallo ü".into() }, Msg::Ping { from: 1, t: 5 }, Msg::Pong { from: 2, to: 1, t: 5 },
            Msg::Rtt { max_ms: 3 }, Msg::Hold { at: 9, why: "x".into(), state: vec![2, 100] }, Msg::PageHashes { from: 2, hashes: vec![0, u64::MAX] },
            Msg::Pages { to: 2, last: true, idx: vec![4, 9], lz: vec![1, 2] }, Msg::Synced { from: 2 },
            Msg::Go { at: 9, through: vec![(1, 20)], active: vec![1, 2] }, Msg::Joined { who: 100, tick: 0, through: 0 }, Msg::Pause { from: 2, on: true }, Msg::Paused { by: 2, left: 1 },
            Msg::Speed { rate: 65 }] {
            let e = m.encode();
            assert_eq!(u32::from_le_bytes(e[..4].try_into().unwrap()) as usize, e.len() - 4);
            assert_eq!(format!("{:?}", Msg::decode(&e[4..]).unwrap()), format!("{m:?}"));
        }
    }

    #[test]
    fn addresses_parse() {
        assert_eq!(parse_addr("127.0.0.1"), Some(SocketAddr::from(([127, 0, 0, 1], GAME_PORT))));
        assert_eq!(parse_addr(" 10.0.0.7:5000 "), Some(SocketAddr::from(([10, 0, 0, 7], 5000))));
        assert_eq!(parse_addr(""), None);
        assert_eq!(parse_addr("not an address:x"), None);
    }

    /// Host and client on localhost: discovery finds the host, the client joins, the lobby and chat reach both.
    #[test]
    fn lobby_on_localhost() {
        // OS-assigned game and discovery ports: other test runs / apps on this machine can't take or answer them.
        // Deadlines are generous for a loaded machine; the loops end as soon as the condition holds.
        let disc = UdpSocket::bind("127.0.0.1:0").unwrap();
        let disc_port = disc.local_addr().unwrap().port();
        let mut h = Net::host_with("Host", 0, Some(disc)).expect("host");
        let addr = SocketAddr::from(([127, 0, 0, 1], h.port()));
        let join = std::thread::spawn(move || Net::join(addr, "Gast").expect("join"));
        let t0 = Instant::now();
        while h.lobby.humans.len() < 2 && t0.elapsed() < Duration::from_secs(10) { h.poll(); std::thread::sleep(Duration::from_millis(5)); }
        let mut c = join.join().unwrap();
        assert_eq!(c.me, 2);
        let mut l = h.lobby.clone();
        l.ais.push((0, 0));
        h.set_lobby(l.clone());
        h.say("hi");
        let t0 = Instant::now();
        while (c.lobby != l || c.chat.is_empty()) && t0.elapsed() < Duration::from_secs(10) { c.poll(); std::thread::sleep(Duration::from_millis(5)); }
        assert_eq!(c.lobby, l);
        assert_eq!(c.chat, vec![(1, "hi".to_string())]);
        let (t0, mut found) = (Instant::now(), vec![]);
        while !found.iter().any(|f: &Found| f.addr.port() == h.port()) && t0.elapsed() < Duration::from_secs(10) { found = discover_at(disc_port, Duration::from_millis(300)); }
        assert!(found.iter().any(|f| f.addr.port() == h.port() && f.name == "Host" && f.players == l.players() && !f.running), "{found:?}");
    }

    /// In-game chat to some players (HUD_MPChatPanel recipients): a client's line to one other client goes through the
    /// host without showing there; an insult to everyone reaches all; the panel's mask / team picks and insult lines.
    #[test]
    fn chat_recipients() {
        let mut h = Net::test_host();
        let addr = SocketAddr::from(([127, 0, 0, 1], h.port()));
        let mut cs = vec![];
        for k in 0..2 {
            let j = std::thread::spawn(move || Net::join(addr, &format!("Gast{k}")).expect("join"));
            let t0 = Instant::now();
            while h.lobby.humans.len() < k + 2 && t0.elapsed() < Duration::from_secs(10) { h.poll(); std::thread::sleep(Duration::from_millis(2)); }
            cs.push(j.join().unwrap());
        }
        assert_eq!((cs[0].me, cs[1].me), (2, 3));
        cs[0].say_to("psst", 1 << 3);
        cs[1].say_to("\u{1}3", 1 << 1 | 1 << 2);
        let t0 = Instant::now();
        while (cs[1].chat.len() < 2 || cs[0].chat.len() < 2 || h.chat.is_empty()) && t0.elapsed() < Duration::from_secs(10) {
            h.poll();
            for c in &mut cs { c.poll(); }
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(100));
        h.poll();
        assert_eq!(h.chat, vec![(3, "\u{1}3".to_string())], "the host gets the insult, not the private line");
        assert!(cs[1].chat.contains(&(2, "psst".to_string())) && cs[0].chat.contains(&(3, "\u{1}3".to_string())));
        let st = Status { players: vec![(1, "Host".into()), (2, "Gast0".into()), (3, "Gast1".into())], teams: vec![(1, 1), (2, 1), (3, 2)], me: 1, ..Default::default() };
        let m = Mutex::new(st.clone());
        let mut ui = ChatUi::NEW;
        assert_eq!(ui.mask(&st), 0b1110);
        ui.click(&m, crate::menu::chat_panel::TEAM);
        assert_eq!(ui.mask(&st), 0b0110, "Your Team: players 1 and 2");
        ui.click(&m, crate::menu::chat_panel::ENEMIES);
        assert_eq!(ui.mask(&st), 0b1000);
        ui.click(&m, crate::menu::chat_panel::INSULT + 3);
        assert_eq!(m.lock().unwrap().say, vec![("\u{1}3".to_string(), 0b1000)]);
        assert_eq!((insult("\u{1}3"), insult("\u{1}21"), insult("hi")), (Some(3), None, None));
        assert_eq!(insult_wav(7), "insult7.wav");
    }

    /// Both Lives in their own threads for `ms` (stepping whenever the lockstep lets them); returns them.
    #[cfg(feature = "live")]
    fn run_for(ls: Vec<Live>, ms: u64) -> Vec<Live> {
        let stop = Arc::new(AtomicBool::new(false));
        let hs: Vec<_> = ls.into_iter().map(|mut l| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) { if l.net_ready() { l.step(); } else { std::thread::sleep(Duration::from_micros(300)); } }
                l
            })
        }).collect();
        std::thread::sleep(Duration::from_millis(ms));
        stop.store(true, Ordering::Relaxed);
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    }

    /// The pause (ours): the client's Pause key stops both games at the same tick, they stay there; the host's
    /// Pause key resumes, both go on in sync; a player's 4th pause is refused (a game message to them).
    #[cfg(feature = "live")]
    #[test]
    fn pause_and_resume() {
        if !maps() { return; }
        let ls = start(1, 10);
        let st: Vec<_> = ls.iter().map(|l| l.net.as_ref().unwrap().status()).collect();
        let mut ls = run_for(ls, 200);
        st[1].lock().unwrap().pause = true;
        let t0 = Instant::now();
        loop {
            ls = run_for(ls, 200);
            let paused = st.iter().all(|s| s.lock().unwrap().paused == Some((2, PAUSES - 1)));
            if paused && ls[0].tick() == ls[1].tick() { break; }
            assert!(t0.elapsed() < Duration::from_secs(30), "not paused: {} / {}", log(&ls[0]), log(&ls[1]));
        }
        let at = ls[0].tick();
        ls = run_for(ls, 500);
        assert_eq!((ls[0].tick(), ls[1].tick()), (at, at), "stays paused");
        let why = st[1].lock().unwrap().holding.clone();
        assert!(why.as_deref().is_some_and(|w| w.contains("paused by Gast0")), "{why:?}");
        st[0].lock().unwrap().pause = true; // anyone resumes
        let mut ls = run_all(ls, at + 200);
        assert_eq!(sums(&mut ls[0]), sums(&mut ls[1]));
        assert!(st.iter().all(|s| s.lock().unwrap().paused.is_none()));
        println!("paused at {at}, resumed: {}", log(&ls[1]));
        // the quota, on the host: player 2 used 1 of PAUSES
        let n = ls[0].net.as_mut().unwrap();
        for _ in 1..PAUSES { n.pause(2, true); assert_eq!(n.paused_by, 2); n.pause(2, false); }
        n.pause(2, true);
        assert_eq!(n.paused_by, 0, "4th pause refused");
    }

    /// The game speed (ours): the client's speed key reaches everyone through the host, 10..90 by 5; the games stay
    /// in sync.
    #[cfg(feature = "live")]
    #[test]
    fn speed_for_everyone() {
        if !maps() { return; }
        let ls = start(1, 10);
        let st: Vec<_> = ls.iter().map(|l| l.net.as_ref().unwrap().status()).collect();
        let rates = || st.iter().map(|s| s.lock().unwrap().rate.take()).collect::<Vec<_>>();
        st[1].lock().unwrap().speed = Some(63);
        let ls = run_for(ls, 300);
        assert_eq!(rates(), [Some(60), Some(60)], "{} / {}", log(&ls[0]), log(&ls[1]));
        st[0].lock().unwrap().speed = Some(500);
        let ls = run_for(ls, 300);
        assert_eq!(rates(), [Some(90), Some(90)]);
        st[1].lock().unwrap().speed = Some(90); // unchanged: nothing sent
        let ls = run_for(ls, 300);
        assert_eq!(rates(), [None, None]);
        let at = ls[0].tick().max(ls[1].tick()) + 100;
        let mut ls = run_all(ls, at);
        assert_eq!(sums(&mut ls[0]), sums(&mut ls[1]));
    }

    /// The in-game chat: a line typed on the client (the chat line's keys) reaches the host, whose message list
    /// shows it.
    #[cfg(feature = "live")]
    #[test]
    fn chat_in_game() {
        if !maps() { return; }
        let ls = start(1, 10);
        let st: Vec<_> = ls.iter().map(|l| l.net.as_ref().unwrap().status()).collect();
        let mut ui = ChatUi { open: true, ..Default::default() };
        for (k, c) in [("H,shift", Some("H")), ("I", Some("i")), ("Space", None), ("Alpha1,shift", Some("!")), ("X", Some("x")), ("Backspace", None), ("Return", None)] {
            ui.key(&st[1], k, c);
        }
        assert!(!ui.open && ui.line.is_empty());
        let t0 = ls[0].tick();
        let ls = run_all(ls, t0 + 100);
        assert_eq!(st[0].lock().unwrap().chat.last(), Some(&(2, "Hi !".to_string())));
        let mut host = ChatUi::default();
        host.sync(&st[0].lock().unwrap());
        assert_eq!(host.shown().last(), Some(&(2, "Hi !".to_string())));
        drop(ls);
    }

    /// A LAN game saves as .msv (the original's multiplayer saves; the LAN lobby lists only those).
    #[cfg(feature = "live")]
    #[test]
    fn lan_saves_are_msv() {
        if !maps() { return; }
        let dir = std::env::temp_dir().join(format!("shc-msv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut ls = start(1, 10);
        ls[0].save(&dir.join("lan game.sav")).unwrap();
        let t0 = Instant::now();
        while !dir.join("lan game.msv").exists() && t0.elapsed() < Duration::from_secs(30) { std::thread::sleep(Duration::from_millis(50)); }
        assert!(dir.join("lan game.msv").exists() && !dir.join("lan game.sav").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "live")]
    mod games {
        use super::*;
        use crate::live::Live;

        pub fn maps() -> bool { crate::skirmish::maps_dir().join("Crater Lake.map").exists() }

        /// A host and `n` clients on localhost, started on Crater Lake with one AI (Rat): their Lives.
        pub fn start(n: usize, hash_every: i32) -> Vec<Live> {
            let mut host = Net::test_host();
            let addr = SocketAddr::from(([127, 0, 0, 1], host.port()));
            let mut clients = vec![];
            for k in 0..n {
                let j = std::thread::spawn(move || Net::join(addr, &format!("Gast{k}")).expect("join"));
                let t0 = Instant::now();
                while host.lobby.humans.len() < k + 2 && t0.elapsed() < Duration::from_secs(5) { host.poll(); std::thread::sleep(Duration::from_millis(2)); }
                clients.push(j.join().unwrap());
            }
            let mut l = host.lobby.clone();
            l.ais = vec![(0, 0)];
            host.set_lobby(l);
            let (lobby, seed, delay) = host.start(1792400000, DELAY);
            let t0 = Instant::now();
            while clients.iter().any(|c| c.started.is_none()) && t0.elapsed() < Duration::from_secs(10) {
                for c in &mut clients { c.poll(); }
                std::thread::sleep(Duration::from_millis(2));
            }
            std::iter::once(host).chain(clients).map(|net| {
                let mut l = Live::from_mem(game(&lobby, net.me, seed).unwrap());
                l.view = Some([0, 40, 0, 40]);
                let mut ls = Lockstep::new(net, delay, l.tick());
                ls.hash_every = hash_every;
                l.net = Some(ls);
                l
            }).collect()
        }

        /// Run `l` until tick `until` (and no hold is on), 120 s at most. Returns it and its error, if any.
        pub fn run(mut l: Live, until: i32) -> (Live, Option<String>) {
            let t0 = Instant::now();
            while (l.tick() < until || l.net.as_ref().is_some_and(|n| n.hold.is_some())) && t0.elapsed() < Duration::from_secs(120) {
                if let Some(e) = l.net.as_ref().and_then(|n| n.error.clone()) { return (l, Some(e)); }
                if l.net_ready() { l.step(); } else { std::thread::sleep(Duration::from_micros(300)); }
            }
            let e = l.net.as_ref().and_then(|n| n.error.clone());
            (l, e)
        }

        /// `run` at the game's pace: at most one tick per `tick_ms`.
        pub fn run_paced(mut l: Live, until: i32, tick_ms: u64) -> (Live, Option<String>) {
            let mut next = Instant::now();
            while l.tick() < until {
                if let Some(e) = l.net.as_ref().and_then(|n| n.error.clone()) { return (l, Some(e)); }
                // like the app's sim thread: the lockstep is polled every loop, the tick runs when due
                if l.net_ready() && Instant::now() >= next {
                    l.step();
                    next = (next + Duration::from_millis(tick_ms)).max(Instant::now() - Duration::from_millis(100));
                } else { std::thread::sleep(Duration::from_micros(300)); }
            }
            (l, None)
        }

        /// Every Live in its own thread until `until`; panics on an error.
        pub fn run_all(ls: Vec<Live>, until: i32) -> Vec<Live> {
            let hs: Vec<_> = ls.into_iter().map(|l| std::thread::spawn(move || run(l, until))).collect();
            hs.into_iter().map(|h| {
                let (l, e) = h.join().unwrap();
                assert!(e.is_none(), "player {}: {e:?}", l.me);
                l
            }).collect()
        }

        pub fn sums(l: &mut Live) -> [u32; 14] {
            let mut m = l.mem().clone();
            shc_sim::mp::sync_check(&mut m).unwrap()
        }

        pub fn log(l: &Live) -> String { l.net.as_ref().map_or(String::new(), |n| n.log.join(" | ")) }
    }
    #[cfg(feature = "live")]
    use games::*;
    #[cfg(feature = "live")]
    use crate::live::Live;

    /// Adaptive delay + compact messages: on localhost the delay drops from the start's 10 to ≤ 5 ticks; the peers stay
    /// in sync; a quiet tick costs few bytes.
    #[cfg(feature = "live")]
    #[test]
    fn adaptive_delay_compact() {
        if !maps() { return; }
        let ls = start(1, 1);
        let t0 = ls[0].tick();
        // at the game's default pace (40 ticks/s), as the app runs it
        let hs: Vec<_> = ls.into_iter().map(|l| std::thread::spawn(move || run_paced(l, t0 + 600, 25))).collect();
        let mut ls: Vec<Live> = hs.into_iter().map(|h| { let (l, e) = h.join().unwrap(); assert!(e.is_none(), "{e:?}"); l }).collect();
        let (a, b) = (sums(&mut ls[0]), sums(&mut ls[1]));
        let n = ls[0].net.as_ref().unwrap();
        let per_tick = n.sent_bytes() as f32 / 600.0;
        println!("delay {} / {}, rtt {:.2} ms, host sent {:.1} B/tick (sync check every tick: 70 B of it), checks {}", n.delay,
            ls[1].net.as_ref().unwrap().delay, n.status.lock().unwrap().rtt_ms, per_tick, n.hashes_checked);
        assert_eq!(a, b);
        // localhost: 5 when idle. A loaded machine (parallel builds) raises the round trip and with it the delay, rightly,
        // so the drop is checked only when both measured < 20 ms (target ≤ 5 at 25 ms ticks).
        // ponytail: skips under load; a pure-fn test of timing() if this needs coverage on busy CI
        let rtts: Vec<f32> = ls.iter().map(|l| l.net.as_ref().unwrap().status.lock().unwrap().rtt_ms).collect();
        if rtts.iter().all(|&r| r < 20.0) {
            assert!(n.delay < DELAY && ls[1].net.as_ref().unwrap().delay < DELAY, "delay did not adapt (rtt {rtts:?} ms)");
        } else {
            println!("delay check skipped: loaded machine, rtt {rtts:?} ms");
        }
        assert!(per_tick < 200.0, "{per_tick} B/tick");
        assert!(n.hashes_checked >= 590);
    }

    /// A desync (one building byte changed on the client) is found by the original's sync check and repaired from the
    /// host's state; the game goes on in sync.
    #[cfg(feature = "live")]
    #[test]
    fn desync_resyncs() {
        if !maps() { return; }
        let ls = start(1, 10);
        let t0 = ls[0].tick();
        let mut ls = run_all(ls, t0 + 150);
        let b = shc_sim::buildings::rec(1) + 0x100;
        let m = ls[1].mem();
        m.set_u8(b, m.u8(b) ^ 0x55);
        let mut ls = run_all(ls, t0 + 400);
        let (a, b) = (sums(&mut ls[0]), sums(&mut ls[1]));
        println!("host: {}\nclient: {}", log(&ls[0]), log(&ls[1]));
        assert_eq!(a, b, "after the resync");
        assert!(ls[1].net.as_ref().unwrap().resyncs >= 1, "no resync");
    }

    /// A player drops out and comes back (its app lost the game: it gets the host's state); everyone waited for it.
    /// A player that does not come back is removed and the others play on.
    #[cfg(feature = "live")]
    #[test]
    fn rejoin_after_drop() {
        if !maps() { return; }
        let ls = start(1, 10);
        let t0 = ls[0].tick();
        let mut ls = run_all(ls, t0 + 100).into_iter();
        let (host, client) = (ls.next().unwrap(), ls.next().unwrap());
        let addr = SocketAddr::from(([127, 0, 0, 1], host.net.as_ref().unwrap().net.port()));
        drop(client); // its app quit
        let h = std::thread::spawn(move || run(host, t0 + 300));
        std::thread::sleep(Duration::from_millis(500));
        let net = Net::join_as(addr, "Gast0", Join::Rejoin(2), 0, 0).expect("rejoin");
        let (_, _, delay) = net.started.clone().expect("start");
        assert!(net.state);
        let (ls2, m) = Lockstep::join(net, delay, Duration::from_secs(30)).expect("state");
        let mut c = Live::from_mem(m);
        c.view = Some([0, 40, 0, 40]);
        c.net = Some(ls2);
        let (c, e) = run(c, t0 + 300);
        assert!(e.is_none(), "{e:?}");
        let (mut host, e) = h.join().unwrap();
        assert!(e.is_none(), "{e:?}");
        let mut c = c;
        println!("host: {}", log(&host));
        assert_eq!(sums(&mut host), sums(&mut c));
        assert!(log(&host).contains("comes back"));
        // now it quits for good: removed after the wait, the host plays on
        drop(c);
        host.net.as_mut().unwrap().rejoin_wait = Duration::from_secs(1);
        let (host, e) = run(host, t0 + 450);
        assert!(e.is_none(), "{e:?}");
        assert_eq!(host.net.as_ref().unwrap().players(), &[1]);
        assert_eq!(host.mem_ref().i32(0x188571bbc + 2 * 4), -1, "player 2 removed (human slot -1)");
    }

    /// A spectator joins a running game (the host's state), then follows it: its sync sums equal the players'.
    #[cfg(feature = "live")]
    #[test]
    fn spectator_joins() {
        if !maps() { return; }
        let ls = start(1, 10);
        let t0 = ls[0].tick();
        let ls = run_all(ls, t0 + 100);
        let addr = SocketAddr::from(([127, 0, 0, 1], ls[0].net.as_ref().unwrap().net.port()));
        let hs: Vec<_> = ls.into_iter().map(|l| std::thread::spawn(move || run(l, t0 + 350))).collect();
        let net = Net::join_as(addr, "Zuschauer", Join::Spectate, 0, 0).expect("spectate");
        assert!(net.me >= SPECTATOR);
        let (ls3, m) = Lockstep::join(net, DELAY, Duration::from_secs(30)).expect("state");
        let mut s = Live::from_mem(m);
        s.me = 0;
        s.net = Some(ls3);
        let (mut s, e) = run(s, t0 + 350);
        assert!(e.is_none(), "{e:?}");
        let mut ls: Vec<Live> = hs.into_iter().map(|h| { let (l, e) = h.join().unwrap(); assert!(e.is_none(), "{e:?}"); l }).collect();
        let (a, b, c) = (sums(&mut ls[0]), sums(&mut ls[1]), sums(&mut s));
        assert_eq!(a, b);
        assert_eq!(a, c, "spectator");
        assert_eq!(s.mem_ref().i32(0x1888e0d60), 0, "a spectator is nobody");
    }

    /// A LAN game saved (native .sav) and loaded into a new LAN lobby: the host loads it, the client gets the state,
    /// both play on in sync.
    #[cfg(feature = "live")]
    #[test]
    fn save_and_load_lan_game() {
        if !maps() { return; }
        let ls = start(1, 10);
        let t0 = ls[0].tick();
        let mut ls = run_all(ls, t0 + 150);
        let sav = shc_sim::save::save_game(ls[0].mem(), 60_000);
        drop(ls);
        let humans = save_humans(&sav).expect("save");
        assert_eq!(humans, vec![1, 2]);
        let mut host = Net::test_host();
        let addr = SocketAddr::from(([127, 0, 0, 1], host.port()));
        let j = std::thread::spawn(move || Net::join(addr, "Gast0").expect("join"));
        let t1 = Instant::now();
        while host.lobby.humans.len() < 2 && t1.elapsed() < Duration::from_secs(5) { host.poll(); std::thread::sleep(Duration::from_millis(2)); }
        let mut client = j.join().unwrap();
        let mut l = host.lobby.clone();
        (l.loaded, l.slots) = (true, humans);
        host.set_lobby(l);
        let (lobby, _, delay) = host.start(0, DELAY);
        let m = load_game(&sav, &lobby, 1).expect("load");
        let tick = m.i32(shc_sim::core::TICK_COUNTER);
        let mut h = Live::from_mem(m);
        h.view = Some([0, 40, 0, 40]);
        h.net = Some(Lockstep::new(host, delay, tick));
        let hh = std::thread::spawn(move || run(h, tick + 200));
        while client.started.is_none() { client.poll(); std::thread::sleep(Duration::from_millis(2)); }
        assert!(client.state, "the client gets the state");
        let (ls2, m2) = Lockstep::join(client, delay, Duration::from_secs(60)).expect("state");
        assert_eq!(m2.i32(shc_sim::core::TICK_COUNTER), tick);
        let mut c = Live::from_mem(m2);
        c.view = Some([0, 40, 0, 40]);
        c.net = Some(ls2);
        let (mut c, e) = run(c, tick + 200);
        assert!(e.is_none(), "{e:?}");
        let (mut h, e) = hh.join().unwrap();
        assert!(e.is_none(), "{e:?}");
        assert_eq!(sums(&mut h), sums(&mut c));
        assert_eq!(c.mem_ref().i32(0x1888e0d60), 2);
    }

    /// The host leaves a 3-player game: the lowest remaining player hosts, the other reconnects to it, the old host
    /// is removed and both play on in sync. With 2 players, the one left plays on alone.
    #[cfg(feature = "live")]
    #[test]
    fn host_migration() {
        if !maps() { return; }
        let ls = start(2, 10);
        let t0 = ls[0].tick();
        let mut ls = run_all(ls, t0 + 150).into_iter();
        drop(ls.next()); // the host quits
        let mut ls = run_all(ls.collect(), t0 + 400);
        println!("p2: {}\np3: {}", log(&ls[0]), log(&ls[1]));
        assert!(ls[0].net.as_ref().unwrap().is_host(), "player 2 hosts");
        assert_eq!(ls[0].net.as_ref().unwrap().players(), &[2, 3]);
        let (a, b) = (sums(&mut ls[0]), sums(&mut ls[1]));
        assert_eq!(a, b);
        assert_eq!(ls[1].mem_ref().i32(0x188571bbc + 4), -1, "player 1 removed");
        // and player 3 is left alone when player 2 quits too
        let mut ls = ls.into_iter();
        drop(ls.next());
        let (p3, e) = run(ls.next().unwrap(), t0 + 500);
        assert!(e.is_none(), "{e:?}");
        assert!(p3.net.as_ref().unwrap().is_host());
    }

    /// One peer of `lockstep_two_peers`: builds near its keep, recruits Arab archers at its mercenary post, box-selects
    /// its troops and sends them at the AI's keep, changes tax and rations; a sync check every tick. Returns
    /// (player, final tick, sync checks compared, final sync sums).
    #[cfg(feature = "live")]
    fn peer(net: Net, lobby: Lobby, seed: i32, delay: i32, ticks: i32, done: Arc<std::sync::Barrier>) -> (i32, i32, u64, [u32; 14]) {
        use crate::live::{Live, TroopIn};
        let me = net.me;
        let mut l = Live::from_mem(game(&lobby, me, seed).expect("game"));
        let t0 = l.tick();
        let mut ls = Lockstep::new(net, delay, t0);
        ls.hash_every = 1;
        l.net = Some(ls);
        let keep = |l: &Live, p: i32| {
            use shc_sim::buildings as b;
            let m = l.mem_ref();
            (1..b::SLOTS).filter(|&i| matches!(b::state(m, i), 1 | 2) && b::owner(m, i) == p && (0x28..=0x2c).contains(&b::kind(m, i))).map(|i| {
                let (r, n) = (b::rec(i), m.i32(b::rec(i) + 0xf8));
                (m.i16(r + 0xee) as i32 + n / 2, m.i16(r + 0xf0) as i32 + n / 2)
            }).next()
        };
        let (kx, ky) = keep(&l, me).expect("own keep");
        let px = |l: &Live, x: i32, y: i32| { let (a, b) = crate::replay::to_west_pub(l.rotation(), x as i16, y as i16); crate::replay::cell_px(a, b) };
        let c = px(&l, kx, ky);
        let view = [c[0] - 1400.0, c[1] - 800.0, c[0] + 1400.0, c[1] + 800.0];
        l.view = Some(crate::live::camera(view));
        l.records();
        let mut builds = [51, 72, 54, 86, 78, 56, 54, 72].into_iter(); // woodcutter, apples, hovel, mercenary post, hunter, quarry, ...
        let mut next_build = 40;
        let (mut waited, mut stepping) = (Duration::ZERO, Duration::ZERO);
        while l.tick() < t0 + ticks {
            if let Some(e) = l.net.as_ref().and_then(|n| n.error.clone()) { panic!("player {me}: {e}"); }
            let w = Instant::now();
            if !l.net_ready() { std::thread::sleep(Duration::from_micros(200)); waited += w.elapsed(); continue; }
            let k = l.tick() - t0;
            if k == 10 || k % 1000 == 500 { l.set_tax((k / 1000 + me) % 8); }
            if k % 1000 == 900 { l.set_rations((k / 1000 + me) % 4); }
            if k == next_build {
                if let Some(tool) = builds.next() {
                    l.set_tool(tool);
                    let spot = (6..30).flat_map(|r| (-r..=r).flat_map(move |d| [(kx + d, ky - r), (kx + d, ky + r), (kx - r, ky + d), (kx + r, ky + d)]))
                        .find(|&(x, y)| l.hover(tool, x, y).ok);
                    if let Some((x, y)) = spot { l.click(tool, x, y); }
                    l.clear_preview();
                    l.set_tool(0);
                    next_build += 120;
                }
            }
            if k >= 1000 && k % 150 == 0 { l.game_action(1007, 1, 70 + (k / 150) % 3); } // Arab archer / slave / slinger
            // troop orders: box-select around the keep, then a click on a tile toward the AI's keep
            if k >= 1500 {
                let (ax, ay) = keep(&l, 3).unwrap_or((200, 200));
                let s = |w: [f32; 2]| [w[0] + 5000.0, w[1] + 5000.0];
                match k % 400 {
                    0 => l.troop_event(TroopIn::Right(true)),
                    1 => l.troop_event(TroopIn::Right(false)),
                    2 => {
                        let (a, b) = ([c[0] - 700.0, c[1] - 450.0], [c[0] + 700.0, c[1] + 450.0]);
                        l.troop_event(TroopIn::Press(a, s(a), view));
                        l.troop_event(TroopIn::Cursor(b, s(b), [false; 3]));
                    }
                    3 => l.troop_event(TroopIn::Release),
                    4 => {
                        let f = ((k / 400) % 4 + 1) as i32;
                        l.mouse = [kx + (ax - kx) * f / 5, ky + (ay - ky) * f / 5];
                        let w = [-1e6, -1e6];
                        l.troop_event(TroopIn::Cursor(w, [1.0, 1.0], [false; 3]));
                        l.troop_event(TroopIn::Press(w, [1.0, 1.0], view));
                    }
                    5 => { l.troop_event(TroopIn::Release); l.mouse = [-1, -1]; }
                    _ => {}
                }
            }
            let s = Instant::now();
            l.step();
            stepping += s.elapsed();
        }
        println!("player {me}: RunTicks {stepping:?}, waiting for the peer {waited:?}");
        // let the other peer finish and compare the last sync checks
        let t1 = Instant::now();
        while t1.elapsed() < Duration::from_secs(5) {
            l.net_ready();
            let n = l.net.as_ref().unwrap();
            if n.hashes_checked >= ticks as u64 - 2 { break; }
            std::thread::sleep(Duration::from_millis(5));
        }
        let n = l.net.as_ref().unwrap();
        assert!(n.error.is_none(), "player {me}: {:?}", n.error);
        let checked = n.hashes_checked;
        let sums = shc_sim::mp::sync_check(l.mem()).unwrap();
        done.wait(); // neither peer drops its connection before both are through
        (me, l.tick(), checked, sums)
    }

    /// Step 5 of the LAN task: two sims in one process, connected through the real net code on localhost (host +
    /// client), players 1 and 2 human, 3 = Rat; scripted orders on both sides; the original's sync sums compared every
    /// tick. SHC_MP_TICKS = ticks (default 10,000).
    #[cfg(feature = "live")]
    #[test]
    fn lockstep_two_peers() {
        if !crate::skirmish::maps_dir().join("Crater Lake.map").exists() { return; }
        let ticks: i32 = std::env::var("SHC_MP_TICKS").ok().and_then(|v| v.parse().ok()).unwrap_or(10_000);
        let mut host = Net::test_host();
        let addr = SocketAddr::from(([127, 0, 0, 1], host.port()));
        let join = std::thread::spawn(move || Net::join(addr, "Gast").expect("join"));
        let t0 = Instant::now();
        while host.lobby.humans.len() < 2 && t0.elapsed() < Duration::from_secs(5) { host.poll(); std::thread::sleep(Duration::from_millis(2)); }
        let mut client = join.join().unwrap();
        let mut l = host.lobby.clone();
        l.ais = vec![(0, 0)];
        host.set_lobby(l);
        let (lobby, seed, delay) = host.start(1792400000, DELAY);
        while client.started.is_none() && t0.elapsed() < Duration::from_secs(10) { client.poll(); std::thread::sleep(Duration::from_millis(2)); }
        let (cl, cs, cd) = client.started.clone().expect("client got Start");
        assert_eq!((&cl, cs, cd), (&lobby, seed, delay));
        let start = Instant::now();
        let done = Arc::new(std::sync::Barrier::new(2));
        let d2 = done.clone();
        let a = std::thread::spawn(move || peer(host, lobby, seed, delay, ticks, done));
        let b = std::thread::spawn(move || peer(client, cl, cs, cd, ticks, d2));
        let (a, b) = (a.join().expect("host peer"), b.join().expect("client peer"));
        println!("{ticks} ticks in {:?}: host {:?}, client {:?}", start.elapsed(), (a.0, a.1, a.2), (b.0, b.1, b.2));
        assert_eq!(a.3, b.3, "final sync sums differ");
        assert!(a.2 >= ticks as u64 - 2 && b.2 >= ticks as u64 - 2, "sync checks compared: {} / {}", a.2, b.2);
    }
}
