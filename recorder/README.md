# shc-recorder

Proxy `CrusaderDE.dll` for Stronghold Crusader DE. It exports all 77 `DLL_*` functions, forwards each call to `CrusaderDE_orig.dll` in the same folder, and logs every call. The log is the oracle for native playback and for differential tests of a Rust sim.

## Build / install
```bash
cd recorder
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo +stable build --release --lib --target x86_64-pc-windows-gnu
```
Needs `brew install rustup mingw-w64` and `rustup toolchain install stable --target x86_64-pc-windows-gnu`. The linker is set in `.cargo/config.toml`.

Game folder: `…/Stronghold Crusader Definitive Edition_Data/Plugins/x86_64/`
- Install: `./install.sh`. It renames `CrusaderDE.dll` → `CrusaderDE_orig.dll` unless that file is already the proxy, then copies the proxy in. **Re-run it after every Steam update**: Steam overwrites the proxy.
- Undo: `./install.sh --undo`, or Steam → Verify integrity.

Logs go to `%USERPROFILE%\shc_recordings\rec_<unix secs>.shcrec`. Under CrossOver that's `~/Library/Application Support/CrossOver/Bottles/Steam/drive_c/users/crossover/shc_recordings/`. Override with the env var `SHC_REC_DIR`. Each time the game loads the DLL, a new file starts.

## Read
```bash
cargo run --release --bin dump -- <file.shcrec>       # summary, every 100th tick
cargo run --release --bin dump -- <file.shcrec> -v    # every call
cargo run --release --bin dump -- <file.shcrec> -s 500  # all section hashes of tick 500
```

## Format
The whole file is one **lz4 frame** (`lz4_flex::frame`). It's flushed every ~1 s straight to the file (no BufWriter: `FrameEncoder::flush` only pushes the pending block to its inner writer, and a 1 MiB buffer there once swallowed the last minutes of three recordings, save calls included). If the game crashes or quits, only the last ≤1 s is cut and the reader stops cleanly there. All integers are little-endian.

Header:
```
"SHCREC02"  u32 n_calls
n_calls × { u8 name_len, name, u8 n_scalars, u8 has_ret }    // index = call_id
u32 n_sections
n_sections × { i16 type_id, u64 addr, u32 size }             // engine save-section table, live addresses
```
`SHCREC01` files (before 2026-10-18) have no section table and no state-hash blob; `dump` reads both.
`call_id` follows the line order of `re/managed/dll_signatures.txt` (0 = PreInitMap_Editor … 36 = RunTick … 76 = GetLayerData).

Records, one per call, written **after** the original returns:
```
u8 call_id  u32 tick  u32 payload_len
payload = n_scalars × i32   (scalar args in signature order; pointers skipped; bools as raw 4-byte BOOL)
          [i32 ret]         (if has_ret; bool returns = raw EAX)
          blobs*            (u32 len + bytes, until payload end)
```
`tick` is the number of `DLL_RunTick` calls completed before this call. A RunTick record therefore carries its own 0-based tick index.

Blobs per call (any call not listed has none). A null pointer or length ≤ 0 gives an empty blob.

| call | blobs |
|---|---|
| RunTick | draw list (`n × 21 i16`, n = ret, or a scan to the first `rec[0] < 0` if ret < 0), `retData` (2836 B `PlayStateReturnData`), selectedChimps (`numSelectedChimps`@retData+100 × (id,type) i32 pairs), state hashes (below) |
| LoadMapToPlay | fileName, mapName, `retData` (156 B `LoadMapReturnData`, after the call) |
| LoadSaveGame | path (UTF-16), `retData` (156 B `LoadMapReturnData`) |
| SaveSaveGame | path (UTF-16). The save itself is the `.sav` the engine writes there. |
| PreInitMap_Editor | `retData` (156 B `LoadMapReturnData`) |
| SetPath | data, autoData, saveFolderData |
| PreInitMap_Invasion / JustBuild | restartInfo |
| PreInitMap_Multiplayer | restartInfo, `retData` (464 B `MultiplayerSetupTransferData`) |
| CreateTrailMission | fileName, restartInfo |
| CampaignLevel | path |
| Apply/GetMultiplayerSetupData | 464 B `MultiplayerSetupTransferData` (Get: after the call) |
| SetExtendedLordConfig | 1508 B `AILordConfigTransferData` |
| RegisterMultiplayerUser | name |
| ReceiveChore, TriggerMPSave/Load | data |
| ImportAIV | data (length × i16) |
| TroopSelection | selected, underCursor, onScreen (i32 arrays) |
| TroopSelectionChanged | selected |
| SetUTF8MissionText / SetUTF8MapName | text(s) |

Blob sizes are checked against the C# allocations in `re/CSHARP_NOTES.md` §1 and `EngineInterface.cs` structs; `dump` prints min..max bytes per blob and call in its summary.

Not captured: output buffers of unknown size (`LoadMapReturnData`, scenario structs, score data, radar map, `choreBuffer`), plus `Pack`/`Unpack`/`CRC` input bytes. For those, only the size and return value are logged.

## State hashes (per-tick oracle)
After every `DLL_RunTick` returns, the proxy hashes the original engine's global state in place and
appends it as RunTick blob 3:
```
u32 engine_tick      // the engine's own tick counter = save section 1046 (Ghidra 0x183663ee8)
u32 hash_micros      // wall time spent hashing this tick
n × { i16 type_id, u32 crc }
```
- **Regions** = the engine's save/map section descriptor table (`re/sections.tsv`, 212 entries of
  `{u64 addr, i32, i32 size, i16, i16 type_id}`, Ghidra 0x180334ab0). The proxy finds it at startup by
  scanning the loaded `CrusaderDE_orig.dll` image for the type-id run 1003,1037,1043 at stride 24, so it
  survives Steam updates: the 2026-10-17 DLL has the table at +0x2070 and the arrays moved by +0x2070,
  +0x3010 or +0x2000 depending on region (`re/decomp/recorder_state/sections_steam_2026-10-17.tsv`).
  The pointers in the table are relocated by the loader, so the runtime values are hashed directly.
  Entries pointing outside the image are dropped. The table (live addresses) is in the file header.
- **Hash** = the engine's `DLL_CRC` (IEEE CRC-32 table, register init 0, no final xor; same as
  `shc_formats::map::engine_crc`), computed with `crc32fast` (`log::engine_crc`, test
  `engine_crc_matches_table_crc`). The sim can therefore compare its own arrays with `engine_crc`.
- **Schedule**: type ids < 3000 with size ≤ 3 MB (object arrays: units 1015, buildings 1013, RNG 1040,
  game-state scalars; ~20 MB) every tick. The rest (3xxx/4xxx large tile layers, 2016, 1171; ~60 MB)
  every 10th tick (`tick % 10 == 0`) or every tick with `SHC_REC_FULL=1`. The pairs of a tick are in
  table order; a missing id means "not hashed this tick".
- Measured (rec_1792328742, 4934 ticks, skirmish): normal tick ~0.8 ms median, full tick (every 10th) ~3 ms,
  max 7 ms. `engine_tick` = proxy tick + 2 in a fresh skirmish. RNG 1040, units 1015, buildings 1013 and
  the tick counter 1046 change on every tick; ~140 of the 212 sections never changed in that game.
- `dump` prints engine tick, µs, RNG / units / buildings CRCs on its per-100-tick lines, a cost
  summary (median / p90 / max µs) and which ids ever changed. `-s <tick>` lists every hash of one tick.

## ABI notes
- Built against game V2.8.2, where `DLL_RunTick` has `bool troopModeMode` after `SH1RtsControls`. If an update changes a signature, re-decompile `EngineInterface` and diff the `static extern` lines.
- x64 Windows has one calling convention, so `extern "C"` matches.
- `EngineInterface.cs` has no `[MarshalAs]` on any `bool`. Mono therefore marshals each one as a 4-byte Win32 BOOL, and the native code (`re/decomp`) only reads the low byte. The proxy types every bool as `i32` and forwards it untouched, and does the same for bool returns.
