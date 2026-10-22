# Strong Hold: source code

The source of [Strong Hold](https://github.com/YannickAaron/Strong-Hold), the native Mac engine for Stronghold
Crusader: Definitive Edition, **except the simulation** (`sim/`). It's here so you can see what the app does on your
Mac before you trust the DMG: every line that uses the network, your Steam sign-in, your disk or other programs is in
this repository.

Snapshot v0.8.1 (one commit per release; tags match the [releases](https://github.com/YannickAaron/Strong-Hold/releases)).

## What's here

| Folder | What |
|---|---|
| `viewer/` | The app: window, Metal renderer, menus, HUD, first-start setup (`src/setup/`), Steam download (`src/setup/steam.rs`), update check (`src/update.rs`), LAN multiplayer (`src/lan.rs`), file locations (`src/paths.rs`), `bundle.sh` (builds the .app and DMG) |
| `extract/` | Reads the game's Unity files, sounds and videos from your own copy and converts them once |
| `formats/` | Map and save file formats, PKWARE DCL decompressor |
| `audio/` | Sound playback |
| `recorder/` | Development tool: a proxy DLL that logs the original engine's calls (Windows, under CrossOver) |
| `re/data/`, `extract/data/`, `formats/data/` | Small tables the code above reads at build time: hotkeys, file name lists, UI ids, a 110 KB delta that rebuilds the engine's data from your own copy of the game. No graphics, sounds or code from the game, and no game text besides map names |
| `sim/` | **A placeholder** with the real `Cargo.toml` dependencies (only `formats`). See below |

## What's not here, and why

The simulation (`sim/`, the castle, economy, units and AI) is a port of the original engine's logic, reverse engineered
from the game. Its source isn't published for now. It has no dependencies besides `formats/`, and it
contains no network, file-writing or program-launching code: everything the app does on your Mac goes through the
code here.

Also not here: game files of any kind. The app gets those from your own copy of the game on first start.

## Where to look

- **Steam sign-in:** `viewer/src/setup/steam.rs`. The app starts Valve's open-source
  [DepotDownloader](https://github.com/SteamRE/DepotDownloader) 3.4.0 (bundled, pinned by SHA-256 in `viewer/bundle.sh`).
  Your password goes to it once through its input, never as a command-line argument, never to a file or the log.
- **Network:** `viewer/src/update.rs` (once a day: the latest release from GitHub's API), `viewer/src/setup/steam.rs`
  (Steam, only when you download), `viewer/src/lan.rs` (local network, only in LAN games: UDP 47777, TCP 47778).
- **Files:** `viewer/src/paths.rs`. The app writes to `~/Library/Application Support/Strong Hold` and
  `~/Library/Logs/Strong Hold.log`.

## Build it

The part here builds on its own (without the simulation, so it can't play a game):

```bash
brew install libvpx
cd viewer && cargo build --release --no-default-features
```

The DMG on the releases page is built from this code plus `sim/` with `viewer/bundle.sh --dmg`.

## Legal

Unofficial, free, non-commercial fan project, not affiliated with, endorsed or sponsored by Firefly Studios.
Stronghold and Stronghold Crusader are trademarks of Firefly Studios. No licence is granted yet: you may read and build
this code; please ask before redistributing it. Firefly Studios, or anyone with a concern:
[open an issue](https://github.com/YannickAaron/Strong-Hold/issues).
