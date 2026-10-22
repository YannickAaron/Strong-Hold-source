# shc-viewer

Native macOS (Rust + wgpu → Metal) viewer for Stronghold Crusader DE sprites. Shows a 12×7 grid of
animations from all atlases plus the hunter (`body_hunter-*`, 1728 frames) cycling at 3× size.

## 1. Game data (first start, once)

Nothing of the game is in the repo or the app. On first start the viewer finds the user's own copy (SHC_GAME, an earlier
download, Steam's install: CrossOver's bottle "Steam" on macOS) and exports what it reads (`../extract`, ~2 min, the
Rust port of `re/scripts/export_*.py`: same files) into `~/Library/Application Support/Strong Hold/`
(`shc-viewer`, `shc-audio`). macOS shows a setup window (`src/setup/`) that can also download the game from Steam with
the user's account (QR code or password, Valve's DepotDownloader; file list `steam_files.txt`) or take a chosen folder;
elsewhere the export runs in the terminal. `SHC_CACHE=dir` skips all of it (your own export, e.g. the Python scripts'
`~/Library/Caches/shc-viewer` with `SHC_AUDIO`). By hand: `cargo run --release --manifest-path ../extract/Cargo.toml
--bin extract -- all <game dir> <out>/shc-viewer` (sounds go to `<out>/shc-audio`). Building needs `brew install libvpx`
(linked statically). Dev: `SHC_SETUP_CLICK="Use This Copy"` presses setup buttons by title, `SHC_SETUP_SHOT=dir` saves
the setup window after each change (`setup-N.tiff`).

New files in `steam_files.txt` (or new `DEPOTS` in `src/setup/steam.rs`): copies the setup downloaded lack them, so the
main menu offers "New game files available (N MB) — Download", which restarts into the setup window's update mode
(`SHC_SETUP_UPDATE=1`: sign in, DepotDownloader fetches only the missing files). It knows the files from
`steam_index.txt` (names and sizes the patterns pick from the pinned build): after changing either, run
`SHC_WRITE_INDEX=1 cargo test --release steam_index_current` with that build installed in CrossOver.

## 2. Run

```bash
cargo run --release -- --map GrainBasket    # map terrain: drag / arrows = pan, scroll = zoom, Esc = quit
cargo run --release -- --sheet tile_land8   # every frame of one tile set, 64 per row (frame n at col n%64)
cargo run --release                         # sprite/animation grid
```

`--map` takes a name from the game's `StreamingAssets/Maps` (override dir with `SHC_MAPS=dir`) or a path to a `.map`.
GrainBasket: 160,400 tiles, 320,800 triangles in one static vertex buffer, built in ~0.2 s; 8.3 ms/frame
(vsync-capped 120 Hz) on M1 Max at any zoom.

Env vars: `SHC_NOVSYNC=1` (uncapped frame rate), `SHC_SHOT=out.ppm` (save frame 180 and quit),
`SHC_CACHE=dir` (other export dir), `SHC_ZOOM=6` (start zoomed in around the centre; mouse input is ignored while
`SHC_SHOT` is set so shots are reproducible). Frame time is printed every 120 frames.

## Mac app (double-click to play)

```bash
./bundle.sh                 # -> ~/Applications/Strong Hold.app (or ./bundle.sh <folder>)
./bundle.sh --dmg           # + Strong Hold.dmg next to it, to give to other people
```

Builds `--release` and assembles the app: the viewer, a `launch` script, Valve's DepotDownloader 3.4.0 (GPL-2.0, its
licence in `Resources/`, pinned by SHA-256), `steam_files.txt` and our own icon (`icon/draw_icon.py`). No game data: the
first start runs the setup (section 1), then the main menu. Saves go to `~/Library/Application Support/
Strong Hold/Saves` (`SHC_SAVES`); Load Game also lists the CrossOver saves when that folder exists.
Log: `~/Library/Logs/Strong Hold.log`. Ad-hoc signed: on another Mac the first open needs right-click
> Open (macOS 14) or System Settings > Privacy & Security > Open Anyway (macOS 15+) until we sign with a Developer ID.
Env vars set before launch win, e.g. `open --env SHC_SHOT=/tmp/s.ppm "$HOME/Applications/Strong Hold.app"`.

## Format notes

- Atlases are uploaded as-is (`Bc7RgbaUnormSrgb`, no decode). Unity stores rows bottom-up, so atlas
  y maps straight to texture v.
- Sprite rects overlap neighbouring sprites. Each sprite is drawn with its Unity mesh (triangles,
  px offsets from the pivot), exactly like the game, which removes the bleed.
- Team-colour masks (`*_m`, BC1/BC3) are exported too (`masks.tsv`); r = tint area, g = chopFeet cut area.

## Map terrain (what is known vs guessed)

Known: tile layers are 160,400 cells, 800 rows of width 1..400,400..1. DE tile sprites are 64×32 diamonds, pivot at
the diamond centre (taller ones stick up). So each row is drawn 16 px below the last, cells 64 px apart.

Per `re/CSHARP_NOTES.md` §2–3: the C# side never maps map layers to sprites. The engine emits per-tick draw records
of (GM file id, image), and the sprite name is `"<gmname> NNN"`. So the table below can only be a placeholder until
recorded draw lists are replayed.

**Guesses** (all in `GROUND` in `src/main.rs` and `cell_pos` in `src/map.rs`):

- Row 0 is the top vertex; rows 399 and 400 both have 400 cells, so the bottom half is shifted +32 px (half a tile)
  to interlock. Direction of that shift unverified.
- Layer 3037 = ground type. 2 B/cell; each byte is a single flag bit and the two bytes are almost always equal
  (maybe two half-tiles); the low byte is used. GrainBasket counts: 0 (dark band around the lake + outer border),
  8 most common, 2, 16 (central lake), 128 (ring inside 1), 1 (ring around the lake), 4, 64 (6 cells).
- Variant per tile = layer 3008 (random u32) % number of frames in the set.
- `tile_land_macros` (1648 frames) comes in 152-frame terrain blocks (20 + 132); only the first 20 of each block
  are drawn, the 132 look like macro/transition pieces whose placement rule is unknown.

| 3037 flag | Drawn as | Why (guess) |
|---|---|---|
| 8 | land_macros 153–172 (bare earth) | most common, yellow-brown regions |
| 2 | land_macros 457–476 (lush grass) | patches inside earth regions |
| 16 | sea8 list index 1106–1159 (plain teal water) | the central lake |
| 128 | land_macros 933–952 (sand with pebbles) | thin band touching the water = beach |
| 1 | land_macros 781–800 (earth with tufts) | ring around the beach |
| 4 | land_macros 1–20 (sparse scrub) | scattered at land edges |
| 0 | flat dark colour | unknown: dark band between lake and land, likely hills/rock (3005 height not used yet) |
| 64 | magenta | unknown, 6 cells |

Other land_macros blocks seen (frame numbers): 1–152 sparse grass, 153–324 earth, 325–456 earth/sand,
457–608 lush grass, 609–780 sand, 781–932 earth+tufts, 933–1064 sand+pebbles, 1065–1192 dark pits/holes,
1193–1364 mixed grass, 1365–1496 dry yellow grass, 1497–1648 brown dried grass. `tile_land8` (512) is all
lumpy earth; `tile_land_and_stones` (512) is stones on grass.

Not done: heights (3005/3045), cliffs (3007), transitions, depth order across atlases (all tiles are in one atlas today).

## Replay a recorder log

```bash
cargo run --release -- --replay ~/path/rec_123.shcrec [--tick 1500]   # Space play/pause, . step, +/- rate (38 ticks/s default)
```

Applies each `DLL_RunTick` draw list incrementally (tiles dirty-only, objects keyed by objectID, file 0 = delete).
`SHC_FOCUS=x,y` centres a game tile at 1:1 (screenshots). GM id → sheet table: `src/gm.tsv` (from `spriteLoader.addGMFile`).
Projection `x = -(gx+gy)*32, y = (gx-gy)*16` (picked from data, see `replay::cell_px`). Cliff sides (chevrons) drawn.
`SHC_MASK=1` shows the team-colour masks. The shot print lists unit tiles for picking `SHC_FOCUS`.
Not done: flies/pixies/whitecaps, HP bars, chimp 2nd/3rd sprites (k13-16), `Unlit/Foliage` for trees (drawn plain).

What matches the game (all from `re/csharp`, rec 3 tick 5000: 0 sprite misses, 8.3 ms/frame vsync 120 Hz, 3.9 ms uncapped
on M1 Max, full map):

- **Tile images** `Gm::tile` = `SpriteMapping.setGenericBuildingTileGraphic`, incl. old sea GM 5 (animation/light
  variants folded into `image`, light via `CalcLightValueForSea` -> 50..55) and new sea 166, buildings1, goods.
- **Tile shading** `replay::shade` = `gameTile.setTileColour`: light 1..7 `shadow`, 11..17 `shadowB`, 50 white,
  51..55 `waterShadow`; `floatColour` (k7) 55..63 overrides with `tileCol*` (TilemapManager.cs).
- **Team colour** = `Unlit/TeamColour` (DXBC pulled from resources.assets with UnityPy and read by hand, no source in
  the build): `rgb = mix(tex, tex * colour, mask.r)`, `a = sat(tex.a - smoothstep(sat(10 * (cutoff + mask.g - 1)))) * colour.a`.
  `colour` = `GetGMMaterial` palette entry (gm.tsv col 4, `SetBodySprite`; white for plain materials), `cutoff` =
  `_SpriteCutoff` of material variant chopFeet/4 (0, else (j+4)/20). `remapColours` is taken as identity.
- **Draw order** = Unity `sortingOrder = -20000 + row*49 + sub`, row = `replay::row` (x−y, grows toward the front)
  + layerDelay (k12, ≥50 → −50 and the +34 chimp flag). Sub-slots: tiles 0, building anim `animLayer>=25 ? 1 : 16+animLayer`,
  org 4+(x+y)%4, chimp 8 (+34). Ties: screen y, then id%10000 (Unity z). The static tile buffer is ordered by row and
  `draw` merges its per-row ranges with the per-row object runs (~1.6k + ~200 draw calls), so units walk behind walls.

### Map rotation
The engine picks building tile images for the current camera rotation (`DLL_SetMapRotation(rot, cx, cy)`,
rot = C# `Enums.Dircs` − 1: 0 N, 2 E, 4 S, 6 W). The loader turns that call into a synthetic record
(`replay::ROTATION`) and `World` converts all tile coords into the West frame the projection was tuned on
(`to_west`, = W⁻¹∘R from `GameMap.mapGameTileToTilemapCoord`). Without it, non-West recordings draw buildings
as jagged columns. Checked: rec 3 (North) and rec 1 (West).

## In-game HUD (replay)

```bash
uv run --with UnityPy python -I ../re/scripts/export_ui_atlas.py   # once, ~1 s; needs re/ui/ from export_ui.py
cargo run --release -- --replay rec.shcrec --tick 1064
```

Writes `~/Library/Caches/shc-viewer/ui/`: `UI-MasterAtlas.bc` (8192² BC7, all 14 mips, 89 MB), `rects.tsv` (2222
`x:Key` → `SourceRect`), `goods.tsv` / `build.tsv` / `tabs.tsv` / `troops.tsv` / `titles.tsv` (layout pulled from HUD_Goods.xaml, HUD_Main.xaml,
HUD_Troops.xaml, `HUD_Main.BuildIconLists`, `HUD_Buildings.BuildingTitles`), `strings.tsv`, `font.ttf` (Junicode). Without it the replay runs with no HUD.

`src/hud.rs` draws immediate-mode from that atlas. Its only input is `HudState`, decoded from the 2824-byte
`PlayStateReturnData` the engine returned with each `DLL_RunTick` (blob 1; `replay::load`). The Rust sim can fill the
same struct later. Layout is the XAML's 1920×1080 design space, scaled uniformly like the Noesis Viewbox and anchored
to the corners the XAML anchors to:

- **Book** (bottom-right, scribe `UI-Scribe C001`): popularity (green ≥ 50, else red; trend arrow from
  `upcoming_total_popularity`), gold (99999+ cap), population/housing (red when `overcrowding_popularity != 0`).
- **Date**: engine `month` (0-based) / `year` → `TEXT_MONTHS_(month+1)`, as OnScreenText.cs does. Rec 1792268794
  tick 1064: raw month 7 → "Aug 1181".
- **Build panel**: frame `UI-HUD 000`, scroll `UI-HUD 007`, 6 category tabs, the category's `UI-Buildings` buttons at
  their XAML margins. Click a tab = switch category, click a building = highlight (no placement yet). Hover shows the
  game's bubble-help text.
- **Goods bar** (bottom-left, shown state): the 20 goods HUD_Goods.xaml lists (`resources[i]` = `TEXT_GOODS_(i+1)`;
  0, 1, 5, 8, 15 have no slot), icon + amount, name on hover.
- **Panel switch** (GameData.cs:2285): `app_mode` 14 with `app_sub_mode` 61/62 → troops panel, `app_mode` 16 → building
  panel chosen by `app_sub_mode` (MainViewModel.setUpInbuilding), else the build panel. Not `numSelectedChimps > 0`
  (a building can be open with troops still selected).
- **Troops panel** (HUD_Troops.xaml, frame `UI-HUD 006` + left padding `008`): counts per type from `selectedChimps`
  (RunTick blob 2, `(id, type)` pairs), first 8 types ascending in the C# slots (type 55 skipped, page 2+ not drawn),
  picture buttons from `troops.tsv`; 3 stance tabs, Stop/Patrol (+ Disband when `troops_show_disband`). Clicks only
  highlight (no chore). Rec 1792328742 tick 1900: 40/10/4/6/2/8/1/2 = blob counts.
- **Building panel** (HUD_Buildings.xaml): barracks/merc post/stockade (sub 1/44/63) = frame `005/009/010` only;
  others = frame `003` + title (`titles.tsv` = `BuildingTitles[in_structure_type]`) + workers line
  (FatControler.addChimpActions). Keep (sub 2): population, income, tax bar, handle at `tax_rate*25`, tax text +
  face (`tax_popularity/25`). Granary (sub 4): food clock bar, units/months, types eaten, ration dial + hand, ration
  text + face. Rec 1792251064 tick 2800 = Keep (18, 0, "No taxes", +1). No recording opens a granary or a building
  with workers; both were checked by forcing the sub-mode on rec 1792328742 tick 1374 (59 units, 28 months).
- Top right: replay tick. `SHC_SHOT` now pauses the replay so the shot shows `--tick N`.

Checked rec 1792268794 tick 1064: gold 1588, population 10/10, popularity 91 (all match). 8.3 ms/frame vsync,
3.7 ms uncapped on M1 Max. Not done: popularity pulse, sub-menus (towers/gates/…), radar (green square is empty),
availability greying, layout margins from `GetMapperCoords` (the engine moves some buttons; XAML defaults used),
building sketches, recruiting/market/other building panels, troop page 2, unit tooltips, player-colour troop sprites.

## Live: run the Rust sim (`--live`)

```bash
cargo run --release -- --live "$HOME/Library/Application Support/CrossOver/Bottles/Steam/drive_c/users/crossover/AppData/LocalLow/Firefly Studios/Stronghold Crusader Definitive Edition/Saves/Crater Lake-1test.sav"
```

`src/live.rs` loads the save exactly like `sim/src/bin/runsave.rs` and runs the sim on its own thread
(`live::Thread`) at 38 ticks/s (Space / `.` / `+` `-` as in replay). Per tick: `core::tick`, then
`draw::run_tick` (the ported render pass, whole map; `full` only on the first frame) and `hudstate::fill` (the
ported PlayStateReturnData writer, decoded with `HudState::decode` like a recording). The records go through the replay
renderer unchanged. The draw pass only walks the window's part of the map, like the engine (`live::camera`: world px →
screen-row columns / rows, fitted by `camera_probe`; objects leaving the view get delete records and come back when
it returns). Crater Lake, window at 1:1: sim ~2 ms + draw ~2 ms per tick (the whole map would be ~80 ms), HUD 0.02 ms;
window 8.3 ms/frame (vsync 120 Hz). Starts centred on player 1's keep at 1:1, like the game (`Live::keep`). `SHC_TILES_ONLY=1` swaps the draw pass for tile records read straight from
`terrain::GFX` (no units / trees / anims).

- **Minimap**: `hudstate::radar` (FUN_1800f4bb0) at 796 px (the game's zoomed-out radar, whole map), every 8th tick
  (~7–25 ms), drawn in the radar frame (MainHUD.xaml RadarMapGrid). The white rectangle follows the window centre.
- **Tick label** (top right): the engine tick counter (`core::TICK_COUNTER`, the tick the recorder logs); Crater
  Lake was saved at 1064 = Aug 1181 (raw month 7), a month is 800 ticks.
- **Build**: pick a building in the build menu, click the map → `Command::Place` as player 1, centred on the tile
  under the cursor (size from FUN_180069400, `live::footprint`; height ignored when picking the tile). Right click
  drops the tool. The sim's validator is a stub, so it places anywhere. Each placement prints the command.
- **Keys**: `t` / `g` tax up / down (`Command::Tax`), `r` cycles rations (`Command::Rations`).
- **Building panels** (hud.rs, HUD_Buildings.xaml): gate / drawbridge, workshop output, inn, church, cathedral /
  tunnellers' guild recruiting, outpost, siege-tent progress, market price list + auto trade + trade errors, ZZZ /
  Repair (original sprites and places; Delete is ours, the demolish tool's sprite). `SHC_OPEN=<eMappers id>` places
  that building next to player 1's keep and opens its panel (scripted screenshots, e.g. `SHC_OPEN=50` fletcher).
- **Map editor** (main menu shield, `src/editor.rs`): FRONT_EditorSetup (size, type, Load Map File: own maps, then the
  game's), then the map runs in editor mode (shc_sim::editor) with HUD_Main's map-editor panel: the six tool menus,
  brush size (click cycles 1..9), the delete key; strokes are DLL_MapAction inGame 0 (hover, press, drag per tile,
  height brushes repeat on the same tile after 1 s, then every 50 ms, release). Save Map writes
  `<profile>/Maps/<name>.map` (click the name box to type), which the Sandbox lists. Not yet: buildings / keeps /
  units for players, map settings, ripple, the right-click menu while editing.
- `cargo build --no-default-features` builds the viewer without `../sim` (useful while the sim doesn't compile).

## Linux / Windows

The viewer builds there too (`cargo check --target x86_64-unknown-linux-gnu` / `x86_64-pc-windows-gnu`). Mac-only: the
menu bar, trackpad pinch / rotate and the quit sheet (`macos.rs`, `#[cfg(target_os = "macos")]`); elsewhere Ctrl+S /
Ctrl+O / Ctrl+, / Ctrl+= / Ctrl+- / Ctrl+/ and F11 do the menu bar's jobs, the wheel zooms, Q / E turn the map.
Default folders are in `src/paths.rs` (the SHC_* env vars still win; SHC_GAME = the game's install folder):

| | Linux | Windows |
|---|---|---|
| game | `~/.steam/steam/steamapps/common/Stronghold Crusader Definitive Edition` (also `~/.local/share/Steam`, Flatpak Steam; other libraries: SHC_GAME) | `C:\Program Files (x86)\Steam\steamapps\common\…` |
| the game's saves / settings | Proton prefix `steamapps/compatdata/3024040/pfx/drive_c/users/steamuser/AppData/LocalLow/Firefly Studios/…` | `%USERPROFILE%\AppData\LocalLow\Firefly Studios\…` |
| exports (first start) | `~/.local/share/Strong Hold/shc-viewer`, `…/shc-audio` | `%APPDATA%\Strong Hold\shc-viewer`, `…\shc-audio` |
| own settings / progress / saves | `~/.local/share/Strong Hold` | `%APPDATA%\Strong Hold` |

Linux, from a Steam (Proton) install: `sudo apt install libasound2-dev pkg-config libvpx-dev` (sound, videos), then
`VPX_LIB_DIR=/usr/lib/x86_64-linux-gnu cargo run --release`: the first start exports from the Steam copy in the terminal.

