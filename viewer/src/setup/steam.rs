//! Downloads the user's own copy of the game with Valve's DepotDownloader (SteamRE, GPL-2.0, a separate program the
//! .app carries next to us: viewer/dist.sh). We only drive its console: the login goes from DepotDownloader straight
//! to Steam, the password is written to its stdin once and never stored (no -remember-password).
//! Its output strings: re/NOTES.md "First-run Steam setup spike".
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};

pub const APP: &str = "3024040";
/// Build 24816905 (V2.8.2, 2026-10-17): the build extract/data/crusaderde.delta was made from.
pub const DEPOTS: [(&str, &str); 2] = [("3024041", "5418660808748515994"), ("3024042", "8845825763831995554")];
pub const STORE: &str = "https://store.steampowered.com/app/3024040/";

#[derive(Debug, PartialEq)]
pub enum Ev {
    /// The sign-in QR code, true = dark module (refreshes every ~30 s).
    Qr(Vec<Vec<bool>>),
    Password,
    /// Steam Guard: code from the authenticator app / sent by email.
    TwoFactor,
    EmailCode,
    WrongCode,
    /// Steam Guard: approve the sign-in in the phone app (no code needed).
    ApproveOnPhone,
    LoggedIn,
    /// (depot number 1.., 0..1 of that depot)
    Progress(usize, f32),
    Exit(Result<(), Fail>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum Fail { NotOwned, WrongPassword, ManifestGone, DiskFull, Offline, TooManyTries, Other(String) }

impl Fail {
    /// What to tell the user.
    pub fn message(&self) -> String {
        match self {
            Fail::NotOwned => "This Steam account doesn't own Stronghold Crusader: Definitive Edition. Sign in with the account that bought it.".into(),
            Fail::WrongPassword => "Steam didn't accept that user name or password.".into(),
            Fail::ManifestGone => "Steam no longer offers the game version this app was made for.".into(),
            Fail::DiskFull => "The disk is full. Free about 9.5 GB and try again.".into(),
            Fail::Offline => "Couldn't reach Steam. Check the internet connection and try again.".into(),
            Fail::TooManyTries => "Steam refused for now: too many sign-in attempts. Wait a few minutes, then try again.".into(),
            Fail::Other(s) => format!("The download stopped: {s}"),
        }
    }
}

pub enum Login { Qr, Password(String) }

pub struct Download { child: Child, stdin: Option<ChildStdin>, pub rx: Receiver<Ev> }

/// DepotDownloader: next to our executable (the .app's Contents/MacOS), else $SHC_DEPOTDOWNLOADER, else on PATH.
pub fn tool() -> PathBuf {
    let name = if cfg!(windows) { "DepotDownloader.exe" } else { "DepotDownloader" };
    std::env::var_os("SHC_DEPOTDOWNLOADER").map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join(name))).filter(|p| p.exists()))
        .unwrap_or_else(|| name.into())
}

impl Download {
    /// `pinned`: the build we support (`DEPOTS`' manifests); false = Steam's current one.
    pub fn start(dir: &Path, filelist: &Path, login: Login, pinned: bool) -> std::io::Result<Download> {
        std::fs::create_dir_all(dir)?;
        let mut c = Command::new(tool());
        c.args(["-app", APP, "-os", "windows", "-osarch", "64", "-max-downloads", "8", "-depot"]).args(DEPOTS.map(|d| d.0));
        if pinned { c.arg("-manifest").args(DEPOTS.map(|d| d.1)); }
        c.arg("-dir").arg(dir).arg("-filelist").arg(filelist);
        match &login { Login::Qr => { c.arg("-qr"); } Login::Password(u) => { c.args(["-username", u]); } }
        let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let (tx, rx) = channel();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx2, done_tx) = (tx.clone(), tx.clone());
        let err = std::thread::spawn(move || pump(stderr, tx2));
        std::thread::spawn(move || {
            let mut p = pump(stdout, tx);
            p.extend(err.join().unwrap_or_default());
            let _ = done_tx.send(Ev::Exit(classify(&p)));
        });
        Ok(Download { stdin: child.stdin.take(), child, rx })
    }

    /// Answer a prompt (password, Steam Guard code). Never logged.
    pub fn answer(&mut self, s: &str) {
        if let Some(i) = &mut self.stdin { let _ = writeln!(i, "{s}"); let _ = i.flush(); }
    }

    /// Exit code (after `Ev::Exit`).
    pub fn wait_ok(&mut self) -> bool { self.child.wait().is_ok_and(|s| s.success()) }
}

impl Drop for Download { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }

/// Reads a pipe, sends events, returns the lines (for the failure reason). Prompts end without a newline.
fn pump(mut r: impl Read, tx: Sender<Ev>) -> Vec<String> {
    let (mut buf, mut lines, mut chunk, mut p) = (String::new(), vec![], [0u8; 8192], Parser::default());
    loop {
        let n = match r.read(&mut chunk) { Ok(0) | Err(_) => break, Ok(n) => n };
        buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
        while let Some(i) = buf.find('\n') {
            let line = buf[..i].trim_end_matches('\r').to_string();
            buf.drain(..=i);
            if !line.chars().all(|c| c == '█' || c == ' ') { eprintln!("steam: {line}"); } // not the QR code
            for e in p.line(&line) { let _ = tx.send(e); }
            lines.push(line);
        }
        if let Some(e) = prompt(&buf) { eprintln!("steam: {}", buf.trim()); buf.clear(); let _ = tx.send(e); }
    }
    lines
}

fn prompt(s: &str) -> Option<Ev> {
    let s = s.trim_end();
    if !s.ends_with(':') { return None; }
    if s.contains("Enter account password") { Some(Ev::Password) }
    else if s.contains("2-factor auth code") || s.contains("2 factor auth code") { Some(Ev::TwoFactor) }
    else if s.contains("sent to the email") || s.contains("sent to your email") { Some(Ev::EmailCode) }
    else { None }
}

#[derive(Default)]
struct Parser { qr: Option<Vec<Vec<bool>>>, depot: usize }

impl Parser {
    fn line(&mut self, l: &str) -> Vec<Ev> {
        let mut out = vec![];
        let is_row = !l.is_empty() && l.chars().all(|c| c == '█' || c == ' ');
        if let Some(rows) = &mut self.qr {
            if is_row {
                let c: Vec<char> = l.chars().collect();
                rows.push(c.chunks(2).map(|m| m[0] == '█').collect());
                if rows.len() >= rows[0].len() { out.push(Ev::Qr(self.qr.take().unwrap())); } // square: done (no line follows)
                return out;
            }
            let rows = self.qr.take().unwrap();
            if rows.iter().flatten().any(|&d| d) { out.push(Ev::Qr(rows)); }
        }
        if l.contains("sign in with this QR code") { self.qr = Some(vec![]); }
        else if l.contains("Use the Steam Mobile App to confirm") { out.push(Ev::ApproveOnPhone); }
        else if l.contains("previous 2-factor auth code you have provided is incorrect") { out.push(Ev::WrongCode); }
        else if l.contains("Got session token") || l.starts_with("Success!") || l.contains("Got ") && l.contains("licenses for account") { out.push(Ev::LoggedIn); }
        else if l.starts_with("Downloading depot ") { self.depot += 1; out.push(Ev::Progress(self.depot, 0.0)); }
        else if let Some(pct) = l.trim_start().split_once('%').and_then(|(n, _)| n.trim().parse::<f32>().ok()) {
            out.push(Ev::Progress(self.depot.max(1), (pct / 100.0).clamp(0.0, 1.0)));
        }
        out
    }
}

fn classify(lines: &[String]) -> Result<(), Fail> {
    let has = |s: &str| lines.iter().any(|l| l.contains(s));
    if has("is not available from this account") { return Err(Fail::NotOwned); }
    if has("InvalidPassword") || has("Failed to authenticate with Steam") { return Err(Fail::WrongPassword); }
    if has("RateLimitExceeded") || has("AccountLoginDeniedThrottle") { return Err(Fail::TooManyTries); }
    if has("for depot manifest") || has("No manifest request code") || has("Unable to download manifest") { return Err(Fail::ManifestGone); }
    if has("No space left") || has("Disk full") { return Err(Fail::DiskFull); }
    if has("Could not connect to Steam") || has("Connection to Steam failed") && !has("Done!") { return Err(Fail::Offline); }
    if has("Total downloaded:") && !has("was not completely downloaded") { return Ok(()); }
    let last = lines.iter().rev().find(|l| !l.trim().is_empty() && !l.contains('█')).cloned().unwrap_or_else(|| "no output".into());
    Err(Fail::Other(last))
}

#[test]
fn parses_depotdownloader_output() {
    let mut p = Parser::default();
    let mut ev = vec![];
    for l in ["Connecting to Steam3... Done!", "Logging in with QR code...", "Use the Steam Mobile App to sign in with this QR code:",
              "    ", "  ██", "Got 12 licenses for account!", "Downloading depot 3024041",
              " 12.50% Stronghold Crusader Definitive Edition_Data/resources.assets", "100.00% x"] {
        ev.extend(p.line(l));
    }
    assert_eq!(ev, vec![Ev::Qr(vec![vec![false, false], vec![false, true]]),
        Ev::LoggedIn, Ev::Progress(1, 0.0), Ev::Progress(1, 0.125), Ev::Progress(1, 1.0)]);
    assert_eq!(prompt("Enter account password for \"bob\": "), Some(Ev::Password));
    assert_eq!(prompt("STEAM GUARD! Please enter your 2-factor auth code from your authenticator app: "), Some(Ev::TwoFactor));
    assert_eq!(classify(&["Depot 3024041 is not available from this account.".into()]), Err(Fail::NotOwned));
    assert_eq!(classify(&["Total downloaded: 5 bytes (5 bytes uncompressed) from 2 depots".into()]), Ok(()));
}
