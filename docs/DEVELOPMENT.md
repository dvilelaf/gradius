# Development guide

## Prerequisites

- Rust 1.80 or newer (`rust-version` in `Cargo.toml`).
- SDL2 development libraries, only for the desktop frontend:
  - Debian/Ubuntu: `sudo apt install libsdl2-dev`
  - Fedora: `sudo dnf install SDL2-devel`
  - macOS (Homebrew): `brew install sdl2`
  - Windows: MSVC plus SDL2 via `vcpkg`, or the `sdl2` crate's `bundled`
    feature if you prefer a static build.
- A legally obtained `Gradius (E).nes` dump. The repository contains no data
  from it: the tests and the frontends need the pack that
  `scripts/extract-data` builds from your dump (see [Extracted data](#extracted-data)).
  See [`ROM.md`](ROM.md) and `NOTICE`.
- Optional analysis tools: Ghidra 12.x with GhidraNes for `tools/ghidra`, and
  `git`, `cmake` 3.20+, a C compiler and Python 3.11+ for `tools/nesrecomp`
  (external, PolyForm Noncommercial; see below). No workspace build needs them.

The workspace's `default-members` excludes `gradius-sdl`, `gradius-wasm`,
`gradius-audio` and `gradius-diff`, so all non-frontend commands work without
SDL2 installed. The differential harness is still in the workspace; build it
explicitly with `cargo build -p gradius-diff`.

## Build and run

```sh
# Extract once (reads the ROM; writes the pack and the oracle fixtures):
GRADIUS_ROM="/path/to/Gradius (E).nes" scripts/extract-data

# Windowed (default pack path, 3x scale):
cargo run -p gradius-sdl --release -- --pack assets/gradius.pak

# Other initial scales; the window is resizable and F11 toggles fullscreen:
cargo run -p gradius-sdl --release -- --pack assets/gradius.pak --scale 5

# Undistorted fractional scaling for a window that is not an exact multiple:
cargo run -p gradius-sdl --release -- --pack assets/gradius.pak --fit aspect

# Fill a non-4:3 monitor, accepting distortion:
cargo run -p gradius-sdl --release -- --pack assets/gradius.pak --fit stretch

# No audio device, e.g. over SSH:
cargo run -p gradius-sdl --release -- --pack assets/gradius.pak --no-audio
```

The binary is also directly runnable after `cargo build -p gradius-sdl` as
`./target/debug/gradius` (or `target/release/gradius`).

### Window, scaling and fit

The core renders a fixed 256x240 framebuffer. The frontend never changes it;
it only decides where that image lands inside the window or monitor:

| flag | meaning |
| --- | --- |
| `--scale N` | initial window size, `N` times 256x240 (1..=8, default 3). Resizing the window afterwards is free |
| `--fit integer` | default. Largest whole-number scale that fits, centred. Never a fractional pixel and never distorted |
| `--fit aspect` | largest undistorted size that fits, centred. Fractional scale, so a 1080p window gets 4.5x (1152x1080) |
| `--fit stretch` | fill the whole drawable, distorting 256x240 to the monitor's aspect |
| `--print-viewport` | print `fit`, window size, destination rectangle and scale. Headless runs print the 16:9, 21:9, 4:3 and portrait samples instead of opening a window |
| `--volume N` | master volume, 0..=100 percent (default 100); applied by the SDL audio sink. Headless runs accept and ignore it |

Every mode centres the image and leaves black bars on the unused axis
(letterbox on a taller window, pillarbox on a wider one). `F11` switches to
desktop fullscreen and reuses the same policy, so a 16:9 or 21:9 monitor shows
a centred, undistorted 256x240 image instead of a stretched one. The window
title always shows the current fit and scale, e.g. `[aspect 4.50x]` or
`[integer 4x]`.

This is presentation only. A true widescreen view (showing more of the stage
rather than black bars) is a core-side extension, not a frontend flag: it needs
a wider framebuffer and visible-column change in the PPU model, sprite/HUD
placement and scroll/culling logic that understand the wider viewport, and new
reference snapshots from the oracle. Until then the frontend's job is to
present 256x240 cleanly at any window size, which is all `--fit` controls.

### Headless smoke run

Runs exactly N ticks with no window, no audio device and no vsync wait. It
prints one line and exits 0:

```sh
./target/debug/gradius --pack assets/gradius.pak --frames 180 --no-audio --scale 1
```

Flags that only affect presentation (`--scale`, `--fit`) are accepted and
ignored on this path, so a smoke run can also check the fit arithmetic without
a window:

```sh
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
  ./target/debug/gradius --pack assets/gradius.pak --frames 60 \
  --no-audio --fit aspect --print-viewport
```

That prints one `viewport:` line per sample format and then the usual summary.

The summary contains the frame count, the FNV-1a hash of the RGBA framebuffer,
score, lives and the number of sound events drained. This is the fastest way to
check that a core or pack change still boots. `scripts/smoke.sh` wraps the same
command and is safe on a headless machine.

The headless path never calls `SDL_Init`, so `SDL_VIDEODRIVER` and
`SDL_AUDIODRIVER` do not need to be set. They are still useful when testing the
windowed path without a desktop session:

```sh
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy \
  timeout 2 ./target/debug/gradius --pack assets/gradius.pak --no-audio
```

Never run that command without `SDL_VIDEODRIVER=dummy` on a machine where an
unexpected window or sound would be disruptive.

### Web demo and PWA

The browser build is `gradius-wasm` plus the static page in `web/`. It is not in
`default-members`, so build it explicitly:

```sh
cargo xtask extract "/path/to/Gradius (E).nes" web/gradius.pak
web/build.sh                  # wasm bundle and the extractor wasm
python3 -m http.server -d web 8000
# open http://localhost:8000/
```

`web/build.sh` installs the `wasm32-unknown-unknown` target when needed, copies
`gradius_wasm.wasm` and `gradius_extract_wasm.wasm` next to `index.html` and
checks that `manifest.webmanifest` and
`service-worker.js` exist. The page is an installable offline PWA: the service
worker precaches the wasm, pack and shell files, and installation needs an
`http://localhost` or HTTPS origin (`file://` cannot load the wasm or register
the worker). The page offers integer, aspect and stretch fitting through its own
controls, not the SDL `--fit` flag. See [`PORTABILITY.md`](PORTABILITY.md) for the
wasm ABI and the node web tests.

### Controls

The runner takes both players' buttons in one `gradius_app::InputFrame`
(`p1`, `p2`) and calls `Game::step_players`. In 1P both pads are merged like the
ROM's `$8203`; in 2P only the current player's pad (`$18`) reaches the ship, and
the turn alternates on a death. The design and measured frames are in
[`analysis/twoplayer.md`](analysis/twoplayer.md).

| action | player 1 | player 2 | controller |
| --- | --- | --- | --- |
| Move | arrows or WASD | numpad 8 / 4 / 5 / 6 or T / F / G / H | d-pad or left stick |
| Fire | Z, J or Space | numpad 0 or R | A |
| Power | X or K | numpad `.` or Delete, or Y | B |
| Start | Enter or Numpad Enter | numpad `+` or 1 | Start |
| Select | Backspace or Tab | numpad `-` or 2 | Back |
| Pause | Pause key | — | — |
| Volume | `+` / `-` (5% steps) | — | — |
| Mute | `M` | — | — |
| Debug overlay | F3 (toggle) | — | — |

`--debug-overlay` starts with the overlay already on. It draws capsule count,
selected meter slot, speed level, weapon (normal/double/laser), options, shield
hits and the resulting power flags over the top-left of the frame, plus a
`SELECTED: <name>` line and a one-line flash on each power activation (useful to
see whether B reaches the game). Drawing copies the frame and never writes the core
framebuffer; headless runs ignore the flag.

`+` / `-` change the master volume in 5% steps and unmute, `M` toggles mute,
and `--volume N` sets the starting level. A change is shown in the window title
for 1.5 s. Pause dims the last frame and stamps a `PAUSED` overlay in the
frontend; the core framebuffer is never modified.

The first open game controller drives player 1 and the second drives player 2;
hot-plug events are handled by the SDL event loop. Keyboard and pad state are
unioned per player, so a player can mix both. Player 2 is bound to the numeric
keypad plus a no-numpad fallback (T / F / G / H with R, Y, 1 and 2): no
scancode is shared with player 1, whose aliases stay unchanged. In a 2P game
the pad of the player in turn moves the ship; the other pad is read (so its
Start/Select edges are visible) but does not steer. The wasm/PWA frontend has
the same split: numpad keys and a second gamepad drive `tick(p1, p2)`.

### Keyboard bindings

The windowed frontend loads `keybinds.ini` from the asset pack's directory
(`assets/keybinds.ini` for the default pack path), or from the current
directory when `--pack` is a bare filename. If the file does not exist it is
written with the defaults from the table above plus an explanatory comment
block. Delete the file to restore the defaults.

The format is INI-like: `[player1]` and `[player2]` sections, one
`button = key` line per NES button, and comma-separated aliases where a button
should answer to more than one key. Buttons are `up`, `down`, `left`, `right`,
`fire`, `power`, `start` and `select`. Key names are case-insensitive letters
and digits, `Up`/`Down`/`Left`/`Right`, `Space`, `Return`, `Return2` (numpad
Enter), `Backspace`, `Tab`, `Delete`, `Kp0`..`Kp9`, `KpPeriod`, `KpPlus`,
`KpMinus` and `Num0`..`Num9` (the top-row digits). Lines starting with `#` or
`;` are comments. A section or button omitted from the file keeps its default,
while a listed button replaces that button's whole alias list.

Unknown sections, buttons or key names, a missing `=`, and any key bound twice
(within a player or across players) are parse errors: the frontend prints a
warning naming the line and runs with all defaults. `Pause`, `F11`, `F12`,
`F3`, `Esc`, `M`, `-` and `+` drive the event loop, are rejected as bindings,
and cannot be remapped.

```ini
[player1]
up = Up, W
fire = Q, Space

[player2]
up = Kp8, T
```

`Power` applies while B is held and `$0100 == 1` (`$8979`): a capsule collected
on a frame is consumed on that frame, and a successful handler clears `$42` so
holding B cannot repeat the item. The six-slot meter implements SPEED UP
(repeatable), MISSILE (once), DOUBLE and LASER (toggles), OPTION (cap 2) and
SHIELD (once, five hits). The oracle measurements and the port tests are in
[`analysis/powerup_terrain_verification.md`](analysis/powerup_terrain_verification.md).

## `cargo xtask` reference

`cargo xtask` is an alias for `cargo run --package xtask --`. Every command
reads the cartridge (except `extract` also builds the runtime pack); commands
with an output path write that file. `extract` is the only one that produces a
pack the runtime can load.

| command | purpose |
| --- | --- |
| `inspect <rom>` | Print header fields, mapper, PRG/CHR sizes and the SHA-256 of the file |
| `tiles <rom> <out.png> [--scale 4]` | Render all eight 4 KiB pattern-table halves as one indexed tile sheet |
| `sprite-groups <rom> --out <sheet.png> [--json <groups.json>]` | Render every sprite group from `$8D91`/`$8E91` into a labelled sheet |
| `extract <rom> <out.pak>` | Build the runtime pack (tiles, palettes, audio, provenance metadata) |
| `shot <rom> <out.png> [--frame 180] [--buttons a,b] [--press 180:start]` | Run the oracle and save a screenshot |
| `probe <rom> [--frame 180] [--press 180:start] [--sprites 32] …` | Dump PPU state, palette RAM, OAM, nametables, CPU RAM and FNV-1a hashes. Optional `--json`, `--png`, `--oam`, `--nametables`, `--ram`, `--poke FRAME:ADDR:VALUE` |
| `screens <rom> [--frames 6000] [--press 240:start] [--stage N] [--assist]` | Reconstruct the title screen and a wide stage background from the oracle's PPU VRAM log; `--stage N` (repeatable) pokes the stage copy `$26`; optional `--out`, `--png`, `--dump` |
| `sections <rom> [--out <file>]` | Extract the per-stage `$A65C` terrain placements for stages 1-7 |
| `spawn <rom> [80 81 82 83 84]` | Decode the stage-1 enemy formation for one or more stream types |
| `stages <rom> [--out <file>]` | Dump the per-stage root table (spawn, terrain, scroll end, boss, start, background) for stages 1-7 |
| `stage-enemies <rom> [--out <file>]` | Build the kind × stage enemy matrix, handler addresses and raw stream types for all seven stages |
| `stage2-tables <rom> [--out <file>]` | Dump stage 2's spawn and terrain tables as a `stage2_tables.rs` fragment |
| `ramdump <rom> [--frame 400] [--range 0000:0800] [--watch FROM:TO] [--top 24]` | Dump CPU RAM, or watch its per-frame changes and hottest addresses |
| `disasm <rom> [--from 8000] [--to FFFF] [--covered] [--labels] [--frames N] [--out <file>]` | Disassemble the PRG window; `--covered` marks executed instructions and `--labels` prints branch/data labels |
| `trace <rom> [--frames 300] [--top 40] [--ranges out.txt] [--log N]` | Report executed-address coverage and the hottest code |
| `record-state <rom> --frames N [--out <file>]` | Record one native-format state line per frame from the oracle |
| `record-input <rom> --frames N [--out <file>] [--script <file>]` | Record held buttons, state bytes and scroll per frame; `--script` writes a replay script |
| `record-boss <rom> [--frames 9800] [--stage N] [--out <file>]` | Record a Big Core fight with an appended boss field; `--stage N` pokes `$26` to reach stages 2-7 |
| `verify-sprite0 <rom> [--frames 3000]` | Prove the HUD split's sprite-0 wait passes without an assist and the stage keeps scrolling |
| `state-drift <oracle> <native>` | List the contiguous frame runs where two `gradius-diff --oracle-out`/`--native-out` dumps disagree on `state=` |

Oracle-running commands share `--buttons` (held all run), `--press
FRAME:buttons[:frames]` (timed), `--assist` (inject the HUD split hit the
scanline PPU model cannot make) and `--keep-alive` (hold the shield timer for
analysis runs). These flags exist to reproduce oracle snapshots; they are not
part of the game's runtime input API.

`state-drift` reads only `frame=` and `state=` from the two dumps and prints one
line per contiguous mismatch run (frame range, oracle label, native label) plus
the mismatch-frame and run totals. Field-level drift and object counts remain
`gradius-diff`'s job. On the default 12,000-frame script it now reports no
mismatches: the 22 post-game-over title/attract frames are gone since the
130-frame re-entry title landed, so the demo windows line up with the oracle.
The history and the old per-run frames are in
[`analysis/state_mismatches.md`](analysis/state_mismatches.md), and the fix is
in [`analysis/stage2.md`](analysis/stage2.md).

All seven stages are decoded even though only stage 1 and the stage-2
handoff are wired into the runtime:
`stage1_tables::SECTIONS` (seven spawn rows), `stage1_collision::terrain` and
the `stage1_bg`/`stage2_bg`/`stage_bgs` modules match the ROM, and the stage→data
table and per-stage background hashes are in
[`analysis/stages.md`](analysis/stages.md). The 28 enemy kinds that appear in
the stage streams all have handlers except `$14` and `$16`; the per-stage boss
and clear map is in progress in [`analysis/stage_enemies.md`](analysis/stage_enemies.md).

Examples used to produce `docs/analysis/stage1.md`:

```sh
ROM="/path/to/Gradius (E).nes"

cargo xtask inspect "$ROM"
cargo xtask tiles "$ROM" /tmp/tiles.png --scale 4

cargo xtask probe "$ROM" --frame 400 \
  --press 180:start --press 240:start --sprites 64 \
  --oam /tmp/stage1-oam.bin --json /tmp/stage1.json --png /tmp/stage1.png

cargo xtask shot "$ROM" /tmp/live.png --frame 800 --press 400:start

# Recovered call graph, executed-PC coverage and the boss timeline:
cargo xtask disasm "$ROM" --covered --frames 3000 --press 180:start --out /tmp/prg.asm
cargo xtask record-boss "$ROM" --out /tmp/boss.state
```

### External analysis tools

`tools/ghidra` runs GhidraNes post-scripts against the PRG window and is
documented in [`analysis/ghidra.md`](analysis/ghidra.md); it needs a local
Ghidra install and writes only below its project directory. `tools/nesrecomp`
builds and drives an external NESRecomp checkout, documented in
[`analysis/nesrecomp.md`](analysis/nesrecomp.md). NESRecomp is PolyForm
Noncommercial 1.0.0: use it only for noncommercial research, and never copy its
code or generated output into this repository. Ghidra is Apache-2.0 and
GhidraNes publishes no licence file; neither is vendored. See `NOTICE` for the
development-tool licence summary.

## Differential workflow (`gradius-diff`)

`gradius-diff` is a development-only binary. It loads the same `assets/gradius.pak`
as the port, runs `gradius-nes` and `gradius_core::Game` in lockstep with the
same input, and compares one state line per frame. It is not a runtime
dependency and is not in `default-members`.

```sh
ROM="/path/to/Gradius (E).nes"

# State parity plus the first ten divergences:
cargo run -p gradius-diff --release -- \
  --rom "$ROM" --pack assets/gradius.pak --frames 3000 --report

# Machine-readable summary, sampled PNG pairs and per-region pixel match:
cargo run -p gradius-diff --release -- \
  --rom "$ROM" --pack assets/gradius.pak --frames 700 \
  --report --json /tmp/gradius-diff.json \
  --png-dir /tmp/gradius-diff --png-every 60 --pixel-report

# Record the oracle's state lines as a ROM-free fixture, then replay it:
cargo run -p gradius-diff --release -- \
  --rom "$ROM" --emit-fixture tests/fixtures/stage1-600.state
cargo test -p gradius-diff fixture_replay
```

| flag | meaning |
| --- | --- |
| `--rom PATH` | iNES cartridge for the oracle (required; `--emit-fixture` records from it once) |
| `--pack PATH` | runtime pack for the native port (required unless the run only records a fixture) |
| `--frames N` | number of lockstep frames to run (required; 600 with `--emit-fixture`) |
| `--emit-fixture PATH` | record the oracle's state lines to a fixture and exit; the replay test needs no ROM or pack afterwards |
| `--script PATH` | input script: `FRAME:buttons[:duration]` lines for P1 and `p2 FRAME:buttons[:duration]` for the second pad. The default starts the game at 180/240 and then holds fire and steers |
| `--stage N` | start both sides at stage N (1-7); the oracle pokes `$26` until stage N is playing and the native port boots there. Without `--from`/`--to` the run compares from the first stage-N frame |
| `--from FRAME`, `--to FRAME` | restrict comparison to an inclusive frame range and compare every frame in it, even outside `Playing` |
| `--keep-alive`, `--native-keep-alive` | analysis only: keep the oracle player (and the native player) alive like `xtask`, so the boss window is reachable |
| `--tolerance PX` | allowed position error in whole pixels (default 1); everything else must match exactly |
| `--report` | print the first ten divergences as well as the summary |
| `--json PATH` | write the summary (and pixel summary when `--pixel-report` is set) as one JSON object |
| `--png-dir DIR` | write `oracle-XXXX.png` and `native-XXXX.png` pairs; requires `--png-every` |
| `--png-every N` | sample a pixel comparison every N frames |
| `--pixel-report` | print a per-frame table and an aggregate; regions are playfield (`y < 208`) and HUD (`y >= 208`). Samples every frame unless `--png-every` is set |
| `--pixel-window FROM:TO` | restrict the pixel table, state summary and JSON to an inclusive frame range |
| `--oracle-out F`, `--native-out F` | write one state line per frame for offline inspection |

Only frames where **both** sides report `Playing` are compared by default; the
report prints the compared window, the skipped frames (including how many had
differing states), a per-field pass table, the first divergence, the ship
position error and object-slot match rate. `--from`/`--to` override that and
compare every frame in the range. Exit status is 0 within tolerance, 1 on
divergence and 2 for a usage or I/O error. The state line format and the fields
the oracle cannot supply are documented in `crates/gradius-diff/src/lib.rs`.

The current baseline on a local dump is: a 3,000-frame run with 0 divergences
and every compared field and object slot at 100%; a 700-frame pixel run at
99.9994% overall; and an attract/demo run over frames 130-800 with 0 state
divergences and 99.999% pixels. The game-over flow enters `Title` at 4676, the
same frame as the oracle. Two-player alternating play is measured by the
`p2-handoff` scenario: Select 180 + Start 220, P2's Start inert while P1 plays,
`$18` 0 -> 1 at 1194 with lives 2/3, P2 normal play at 1221 and P2's Right
moving the ship; the row has 0 state divergences from reset. The scenario
matrix is 13 rows, all clean, 0 known. The stage-1 boss takes damage at frame
8066 and clears at 9087 with score 2690; the port matches the oracle through frame 9700,
including the post-clear slot 9
kind-2 debris and the stage-2 scroll reset at 9601. The handoff into stage 2 is
natural: the boss script's stage-1 clear (with the analysis-only keep-alives)
resets into stage 2 at 9601, and state stays exact through 12000 (0 divergences,
every field 100%). The stage-2 pixel gate covers 9601-11199, the whole
pre-turret regime, at >= 99.9% (play 99.9%, HUD 100%); the full 9601-12000
window is a report-only 99.7% (play 99.7%, HUD 100%), with the accepted
deviation from 11200 being enemy animation/effect frames, plus the score row's
1-2 frame lag at 9708-9709. The `s2 full` gate runs the same script through
19430: state 12001-19430 is exact (0 divergences over 7,430 frames), covering
the 15968 `$C540` eruption (fixed in `2c770c6`), the Big Core and the stage-3
entry, with `tests/fixtures/stage2-12001-19430.state` replaying it ROM-free.
The stage-2 boss
hand-off 19037-19430, from the frame after the core's death debris despawns
through the second stage-3 frame, is a second hard pixel gate at >= 99.9%; it
measures 100.0000% on all 394 samples. The `$1B & #$70` ring skip in
`build_sprites` closed the last frame (19429), so both stage-3 entry frames are
exact. The fight 17280-19036 is report-only: the core's `$8F` sprite group
instead of the cartridge's `$6C` armour and its `$3A` shots instead of `$41`
(see [`analysis/stage2_boss_pixels.md`](analysis/stage2_boss_pixels.md)). The
frozen-scroll background latch is fixed (`Game::scroll_lag`), so the frozen
arena is exact. Stage 3 is gated on state: the natural entry is 19429 and the
whole 19429-23000 window is exact (0 divergences, 3572 frames; the generated
`tests/fixtures/stage3-19429-23000.state` fixture replays it ROM-free). The
pixel report is report-only at 98.9% (playfield 98.8%, HUD 100%) with the
accepted residue: the priority subset from 19505 and the missing `$25`
object-fire group from 19530. The fixed entry ring was only 52 of the
window's ~2.31M differing px, so the aggregate stays at 98.9%; 19429 and
19430 are both exact now. Stage 4's poked boot
window is
gated on state too: `--stage 4` over 297-1797 replays 1501 frames with exactly
the shared ship-init residue, 162 `px`/`py` divergences on the 81 `Other(4)`
respawn frames 497-523, 739-765 and 1410-1436, and every other field and all
7167 object slots at 100% (`tests/fixtures/stage4-297-1797.state` replays it
ROM-free). The sampled pixel report (every 30th frame) stays report-only at
99.77% (play 99.75%, HUD 99.92%). Stage 5's poked phase is exact end to end:
`--stage 5` over 297-12003 replays 11707 frames with 0 divergences, every
field at 100% and 30863/30863 object slots. The `$6C` RAM formation timer fix
lets the `$90` -> `$8E` formation spawn at frame 1174, which closed the old
1725-divergence residue, and the `$9A69` -> `$C76C` -> `$CB84` tail tick puts
the `$14` cannon's death on the oracle's 3055 (score 90 -> 190) instead of
3043. The `$AEDB` drift fix (`8ea0595`) and the `$82` hold-countdown fix
retire 8574 and 9597, and the clear itself is exact: the `$3F = $0C` core
spawns at 10274 (its slot-9 `$98` object from 10253), dies at 11489 (+1000)
and the `$96C4` `INC $19` enters stage 6 at 12003 with an empty object table.
Two fixtures replay it ROM-free: `tests/fixtures/stage5-297-8573.state` in
`stage5_replay` and `tests/fixtures/stage5-8574-12003.state` in
`stage5_clear_replay` (whose `stage5_injected_clear_divergence_is_caught`
moves a slot-9 `obj.x` at 11000 and must fail); they are 1.46 MB and 552 KB,
the bytes a single continuous 297-12003 fixture would take. The stage-5 pixel
report (measured over 297-12003) stays report-only at 99.1% (playfield 99.0%,
HUD 100.0%).
Stage 6's poked boot window is exact too: `--stage 6` over 297-1500 replays 1204
frames with 0 divergences, every field at 100% and 6759/6759 object slots. The
no-core clear is beyond that fixture (`$3F = $0B` lands at oracle frame 5928
and holds the scroll from 5929 through 6050):
`stage6_clears_to_stage_seven` continues `new_at_stage(5)` with no input and
pins the first stage-7 frame at native 7171, after the `$81`/`$82` beam window,
the `$86` wall erase and the `$90` advance. `tests/fixtures/stage6-297-1500.state`
replays the window ROM-free and the stage-6 pixel report (measured over
297-1500) stays report-only at 99.7% (playfield 99.7%, HUD 100.0%). Both layers
were checked negatively: a score injected on one native frame makes
`stage6_replay_is_exact` fail at 897, and a truncated `stage6.json` makes the
`release-check` stage-6 row fail. The stage-7 ending (`$87`-`$8D`) is a state
gate through `ending_replay`: it seeds `Game::new_at_ending` at the measured
7024 `$87` state and compares oracle 7025-8290 (1266 frames) with 0
divergences, including the `$CF35` text row timing, the single +10000 at 7998
and stage 1 playable at 8282. Two gaps stay OPEN: the `$CF90` column screen
(the port still draws its procedural starfield) and the `$987F`/`$BB09` ending
ship objects, which is why that gate uses `compare_objects: false`. The full
297-6050 stage-7 replay remains report-only at 11,512 divergences, first at
4573 `obj.slot`. The
stage-entry frames 9601-9603 are exact since the `$1B & #$70` ring skip; the
`$25` launcher shots are still accepted: the port
does emit
the `$25` group for the `$C540` launcher's one-shot, but from the object centre
instead of the `$0496` muzzle, so that wave sits a few px off (see
[`KNOWN_DEVIATIONS.md`](KNOWN_DEVIATIONS.md)). `--stage N` and the
generated `tests/fixtures/stage2-9601-12000.state` give the harness a
stage-aware run and a ROM-free replay of the same window. Music follows the
handoff: the runner resets at the stage change, selects stage 2's `$59` from
the stage's `$8344` id, and has a cue for each of the 12 captured tracks;
`crates/gradius-app/tests/natural_handoff.rs` replays the recorded clear and
asserts the `$59` cue at the first stage-2 frame. Both 12,000-frame long
runs now report `WITHIN TOLERANCE (0 divergences)` and 0 state mismatches
(`scripts/longrun.sh`); what remains there is report-only pixel drift in the
attract and effect frames (see
[`analysis/longrun_drift.md`](analysis/longrun_drift.md)). Remaining visual difference is a
241 px sprite-palette/priority residue (frames 391-460
and 655/666/677/688, see `docs/analysis/pixels.md`); the title ship and the
OAM-order sprite subset are fixed. Do not claim residue fixed without a report
that shows it.

The native side runs with `Mods::default()`, where the only remaining port
convenience (`extra_options`, four options instead of the cartridge's two at
`$89DA`) is off. Ship-vs-map terrain damage is ROM parity and is always on:
`$C29F` always calls `$C39D`, the `map_terrain_collision` mod was removed, and
the `$0500`/`$0600` pages are non-zero in the pre-boss columns. See
`docs/analysis/powerup_terrain_verification.md`.

`tests/fixtures/stage1-600.state` (with its `.script`) is the generated
fixture; `cargo test -p gradius-diff` replays it through the native port, so the
parity job needs no cartridge. Regenerate it with `--emit-fixture` only when the
recorded window or script changes, and keep the provenance header intact.
`tests/fixtures/stage2-9601-12000.state` is the second generated fixture: the
boss script with both keep-alives from the stage-1 clear into stage 2 at 9601
through 12000. Its replay starts from reset, keeps the native player alive and
compares every frame, so the natural handoff is covered without a cartridge.
`tests/fixtures/stage2-12001-19430.state` is the third: the same run extended
to the stage-3 entry (7,430 frames), replayed by `stage2_window` at 0
divergences and pinning the `$C540` eruption, the boss (17282/18915) and the
19429 hand-off. Regenerate any of them with `--emit-fixture`; the stage-2
natural window is:

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-s2gate cargo build --release -p gradius-diff
/tmp/opencode/target-s2gate/release/gradius-diff \
  --rom "$ROM" --emit-fixture tests/fixtures/stage2-12001-19430.state \
  --frames 19430 --from 12001 --to 19430 \
  --script scripts/boss-script.txt --keep-alive
```

## Tests, formatting and lints

```sh
# Default members: core, pack, assets, cart, oracle, runner and xtask.
cargo test

# Runner clock, catch-up, pause, sinks and MusicDirector:
cargo test -p gradius-app

# Harness parsers, comparison rules, pixel regions, flag parsing and the
# ROM-free fixture replay. Needs the extracted data; the oracle runs are
# skipped unless GRADIUS_ROM is set:
cargo test -p gradius-diff

# Frontend unit tests and the end-to-end headless test (synthetic pack):
cargo test -p gradius-sdl

# wasm backend: pack parsing, input bits, audio ABI:
cargo test -p gradius-wasm

# Formatting for the whole workspace:
cargo fmt --all -- --check

# Lints for the default members:
cargo clippy -- -D warnings -A clippy::too_many_arguments

# Strict lints for the frontend only:
cargo clippy -p gradius-sdl --all-targets --no-deps -- -D warnings
```

The `clippy::too_many_arguments` allowance is for the pre-existing
`Framebuffer::tile` renderer helper in `gradius-core`, whose wide signature is
intentional. New frontend code is checked without that allowance.

The frontend tests use a synthetic `GRDPAK01` built with `PackBuilder`, so they
run in CI without a ROM. The headless integration test executes the real
binary and checks the stdout summary and exit codes; it also asserts that a
malformed pack produces an actionable error. `gradius-diff`'s tests read the
extracted pack through `gradius_diff::testdata`, and the generated
`tests/fixtures/stage1-600.state` replays without a cartridge.

## Extracted data

`scripts/extract-data` builds everything derived from your cartridge into the
working tree:

```sh
GRADIUS_ROM="/path/to/Gradius (E).nes" scripts/extract-data          # rebuilds only what is stale
GRADIUS_ROM="/path/to/Gradius (E).nes" scripts/extract-data --force  # rebuilds anyway
```

It runs `xtask extract` into `assets/gradius.pak`, then `scripts/gen-fixtures.sh`
when that script exists. `gen-fixtures.sh` regenerates the oracle fixtures in
`tests/fixtures/` with the command each one records. Neither output is committed.

The stamp in `assets/.data-stamp` is the SHA-256 of the ROM together with the
tracked sources that define the extraction: `xtask/`, `crates/gradius-{pack,assets,audio,nes,cart}/`
and `crates/gradius-core/src/rom/`. Uncommitted edits to those files change it.
A second run with the same stamp does nothing. The extraction is deterministic,
so `--force` writes the same bytes.

Tests that need the pack or the fixtures fail when they are missing, and the
message gives the command to run: `no extracted pack at <path>: run
GRADIUS_ROM=<rom> scripts/extract-data`, or `missing fixture <path>: run
scripts/extract-data`. Nothing skips silently. The tests that run the oracle
skip without `GRADIUS_ROM`. `scripts/verify changed` and `full` refresh the
data first when `GRADIUS_ROM` is set.

### ROM-data guard

`scripts/check-rom-data.sh` runs in every `scripts/verify` mode and in CI. It
reads the tracked files and flags:

- a `.rs` file with, outside test code, a bracketed literal of more than 32
  numbers, a byte string longer than 32 bytes, or more than 256 numbers inside
  bracketed literals in total;
- any tracked `*.nes`, `*.pak`, `tests/fixtures/*.state` or `tests/fixtures/*.trace`.

Test code is excluded: paths under `/tests/`, files named `tests.rs`, and
everything after the first `#[cfg(test)]`.

Each flagged path must be listed in `scripts/rom-data-allow.txt` as
`<path> <card> <reason>`, under a `# <card>` heading with a blank line between
groups. A flagged path that is not listed fails the check. So does an entry the
guard no longer flags, or whose file is gone, so the list cannot rot. Entries
are removed by the change that moves their data out of the repository. Code that
is not ROM data, such as the APU tables, the 6502 opcodes or the scenario
scripts, goes under `# not-rom` with its reason.

To add a new data file, extract the data into the pack instead. Allowlisting it
is only for data that is being moved.

## Verify gate

`scripts/verify` is the repository's quality gate. It serializes cargo with a
lock so parallel workers do not share a target directory.

```sh
scripts/verify fast    # formatting + cargo check --workspace --all-targets
scripts/verify full    # clippy + cargo test --workspace + SDL build + pack smoke
scripts/verify changed # formatting + clippy/test only the crates this branch touched
```

Every mode runs the file-size and ROM-data guards first. `full` and `changed`
also need the extracted data: with `GRADIUS_ROM` set they run `scripts/extract-data`
(a no-op when the stamp matches); without it they stop when `assets/gradius.pak`
is missing and print the command to run.

- **fast** runs `cargo fmt --all -- --check` and
  `cargo check --workspace --all-targets`. It is the normal per-commit check.
- **full** runs formatting, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace`, builds `gradius-sdl`, then runs the
  headless binary for 120 frames against `assets/gradius.pak`. It therefore
  needs `libsdl2-dev` and a pack.
- **changed** finds the crates this branch touched (diff from the merge-base
  with `main`, plus uncommitted files), runs `cargo fmt`, then
  `cargo clippy -p <crate…> --all-targets -- -D warnings` and
  `cargo test -p <crate…>`. It falls back to **fast** when no crate changed, so
  a docs-only branch still checks formatting. This is the per-commit gate for a
  worker; a wave runs **full** once before integration.

All modes use `--workspace` where noted, so they build `gradius-sdl` even though
it is not a default member. The `full` smoke step invokes `./target/debug/gradius`
directly; if you set `CARGO_TARGET_DIR`, point the binary at that directory or
run the smoke command yourself with the same pack. `scripts/check-targets.sh`
is separate from the gate:

```sh
scripts/check-targets.sh          # guard + wasm32 build + host build
scripts/check-targets.sh guard    # no std::fs/net/process/thread/time in gradius-core
scripts/check-targets.sh wasm     # cargo check -p gradius-core --target wasm32-unknown-unknown
```

### File size

No hand-written `crates/**/*.rs` or `xtask/**/*.rs` file may exceed **1000
lines**; `scripts/check-file-size.sh` enforces it in `scripts/verify` and in CI.
The healthy band is 300-800 lines and 1000 is the soft ceiling where a file
stops fitting in one review, so the limit only fires on real outliers.

A generated file (for example the audio timelines in
`crates/gradius-audio/src/data.rs`) declares itself with `@generated` in its
first five lines and is exempt. Any other file that is already over the limit
is listed in `scripts/file-size-allow.txt` as `<path> <lines> <split card>`;
the guard fails on an entry whose file no longer needs it, so the list cannot
rot. Split the file and delete its entry rather than raising the threshold.

## CI

`.github/workflows/ci.yml` runs independent jobs:

- **Linux (default members):** `scripts/check-file-size.sh`,
  `cargo fmt --all -- --check`, clippy with
  `-D warnings` (except the documented allowance), `cargo test`.
- **Audio and parity (needs the ROM secret):** after `scripts/extract-data`,
  `cargo test -p gradius-audio` checks the captured timelines (12 music tracks
  and 8 effects, the per-track triangle and bass checks, the WAV container) and
  `cargo test -p gradius-diff` replays the generated
  `tests/fixtures/stage1-600.state` through the native port. Without the secret
  the job skips these tests and says so.
- **macOS and Windows:** `cargo test` on the default members.
- **wasm:** `scripts/check-targets.sh wasm` and `web/build.sh`, which builds
  `gradius-wasm` for `wasm32-unknown-unknown` and copies the demo bundle, then
  `node web/fit.test.mjs` for the canvas fit math and `node web/smoke.mjs` for
  the headless demo (pack parse, ticking and audio pull).
- **Portability:** `scripts/check-targets.sh guard` (no OS APIs in
  `gradius-core`).
- **Linux SDL:** installs `libsdl2-dev`, builds `gradius-sdl`, runs
  `cargo clippy -p gradius-sdl --all-targets --no-deps -- -D warnings` and
  `cargo test -p gradius-sdl`, then a headless smoke on the extracted pack (when
  the ROM secret is set) that drives
  `gradius --fit aspect --frames 60 --no-audio --scale 1` with dummy SDL
  drivers.
- **Release gate (optional):** decodes the cartridge and runs
  `scripts/release-check.sh --skip-long` once, so one release build feeds every
  stage-1 and stage-2 gate: tests, extract, state parity (`--frames 3000`, 0
  divergences), pixel parity (`--frames 700`, playfield and HUD each
  >= 99.9%), the late stage-1 pixel window (6000-7723, map end through the Big
  Core spawn, >= 99.9%), the boss window (frames 7500-9700, 0 state
  divergences plus the recorded spawn/damage/clear/score/stage-2 markers), the
  stage-2 window (frames 9601-12000, 0 state divergences and the 9601-11199
  pixel window >= 99.9%, with the 9601-12000 tail report-only), the real-input
  scenario matrix (menu/power/eruption/boss/2P relay, 13 clean rows), the
  stage-2 boss hand-off
  (19037-19430 at >= 99.9%, with 17280-19036 report-only), the
  stage-3 window (state 19429-23000 with 0 divergences and the entry/scout/
  descender markers, pixels 19429-23000 report-only at 98.9%), the
  headless soundtrack
  dump checked by
  `scripts/check-wav-triangle.py`, the per-frame perf budget and the wasm gzip
  size (80 µs median / 100 µs p95, 160 KiB; see
  [Perf and size budgets](#perf-and-size-budgets)), and the wasm/node web
  tests. Any
  hard failure makes the script exit 1 and the job fail; the per-gate logs and
  reports under `/tmp/opencode/release-check` are uploaded as
  `release-gate-logs` only when a step fails. It needs the real ROM, so it runs
  only when the repository secret `GRADIUS_ROM_B64` holds the base64-encoded
  iNES image; without it every heavy step is skipped and the job stays green.
  The same secret feeds the cross-compare job below. A run on the current tree
  prints:

  ```
  gate       state detail
  ----       ----- ------
  build      PASS  release binaries
  tests      PASS  workspace tests, including the 448x28 stage-1 map pin
  extract    PASS  /tmp/opencode/release-check/stage1.pak
  state      PASS  3000 frames, 0 divergences
  pixel      PASS  play 100.000% hud 100.000%
  s1 late    PASS  pixel 6000-7723 99.90% (play 99.90% hud 100.00%) over 1724 samples; 6500 playfield exact
  s1late rpt WARN  frame 6500 playfield exact, hud 20 px score-row lag; accepted, docs/KNOWN_DEVIATIONS.md
  boss       PASS  7500-9700, 0 divergences; spawn 7723 damage 8067 clear 9087 score 2690 debris slot 9 kind 2 stage-2 9601
  stage2     PASS  entry 9601 span 1200 px; $C540 rank timer 1027, kind 4 @11140 turrets $91 11200/$92 11204; pixel 9601-11199 99.9% (play 99.9% hud 100.0%)
  s2 tail    WARN  9601-12000 pixel 99.7% (play 99.7% hud 100.0%); report-only: score row 9708-9709, 11200+ animation/effect frames
  scenarios  PASS  scenarios: 13 run, 13 clean pass, 0 known, 0 failed; evidence in /tmp/opencode/release-check/scenarios
  s2boss     PASS  pixel 19037-19430 100.00% (play 100.00% hud 100.00%) over 394 samples; report-only 17280-19036 at 99.7%
  s2boss rpt WARN  17280-19036 core group $8F vs $6C, shots $3A vs $41 (99.7%); 19429-19430 hand-off exact; docs/analysis/stage2_boss_pixels.md
  s2 full    PASS  state 12001-19430 exact (0 divergences, 7430 frames)
  s2 full px WARN  12001-19430 pixel 99.7% (play 99.6% hud 100.0%), every 10th frame; report-only: animation/OAM residue, no state field diverges
  stage3     PASS  state 19429-23000 exact (first divergence none); pixel 19429-23000 98.90% (play 98.80% hud 100.00%) over 3572 samples, report-only
  s3 rpt     WARN  19429-23000 state exact, pixel 98.9% (play 98.8% hud 100.0%) report-only: 19505+ priority subset, $25 object-fire group from 19530; docs/KNOWN_DEVIATIONS.md
  longrun    PASS  default clean; boss clean
  audio      PASS  triangle band present
  perf       PASS  initial median 0.036 ms p95 0.042 ms; stage5 median 0.035 ms p95 0.058 ms; budget 0.08/0.10 ms
  wasm size  PASS  gzip 130349 B (127 KiB), raw 376346 B; budget 160 KiB
  web        PASS  wasm, fit, smoke, pwa

  release-check: 0 hard failure(s), logs in /tmp/opencode/release-check
  ```

  The `tests` row includes `xtask`'s `generated_core_constants_load`, which
  pins the stage-1 map at 448x28 (the 3328 px tail); the older 384x28 pin and
  its stale FNV values are gone.

  A regressed gate turns its row to `FAIL` with the first divergence and the
  script exits 1. The boss gate is hard on both counts: the 7500-9700 state
  window must be clean, and the oracle dump must still show the Big Core spawn,
  the first scored hit, the +1000 clear with its slot-9 kind-2 debris and the
  stage-2 reset. Post-clear debris is part of the compared window, not a
  known-OK exception. The stage-2 gate is hard as well: the same script and
  keep-alives must carry the oracle into stage 2 at 9601, frames 9601-12000
  must have 0 state divergences, and the 9601-11199 pixel window must match at
  least 99.9% overall. It also checks the stage-2 markers (entry at 9601, all
  `Playing`, a >1000 px scroll span, the first kind `$04` spawn at 11140 and
  the `$91`/`$92` turret sectors at 11200/11204) and reports the `$82` rank
  timer 1027 and the `$C540` collision script. The 9601-12000 tail is a
  non-failing `WARN` row at 99.7%: from 11200 the remaining pixels are enemy
  animation/effect frames only, with no state divergence. The
  handoff is natural (no `--stage` poke), and the generated
  `tests/fixtures/stage2-9601-12000.state` fixture replays the same window
  ROM-free. The `s2 full` row extends the same run: state 12001-19430 is
  exact (0 divergences over 7,430 frames), so the `$C540` eruption, the boss
  and the stage-3 hand-off are gated too; its pixels are report-only and
  `tests/fixtures/stage2-12001-19430.state` replays the window ROM-free.
  The `--stage 2` boss/end landmarks 7979/9280/9794 lie outside the
  9601-11199 gate and remain S2-04 work. The stage-2 boss gate is hard on
  the one window that is exact: the same script and keep-alives run to
  19431, and 19037-19430 (the frame after the core's debris despawns
  through the second stage-3 frame) must hold at least 99.9% overall,
  playfield and HUD, measuring 100.0000% on all 394 samples. It is
  pixel-only, so a state regression elsewhere in the window still fails
  the state gates without failing this row. The frozen-scroll background latch
  is fixed (`Game::scroll_lag`), so the arena is exact while the scroll holds.
  The residue before it
  (17280-19036: the core's `$8F` group instead of the cartridge's `$6C`
  armour and its `$3A` shots instead of `$41`) is a
  non-failing `WARN` row naming that cause, and is listed as ACCEPTED
  (report-only) in
  [`KNOWN_DEVIATIONS.md`](KNOWN_DEVIATIONS.md). The two stage-3 entry frames
  are exact: the `$1B & #$70` ring skip closed the old 52 px at 19429.
  Stage-2 music instead runs on the
  natural path: the `$59` cue starts at the stage change, and the
  `gradius-app` natural-handoff test asserts it, so the boundary-4 scroll
  switch is no longer part of the picture. The stage-3 row gates state over the
  whole ported window: the same script and keep-alives run to 23000, and
  19429-23000 must keep 0 divergences (3572 frames), while the oracle must
  still show the 19429 reset entry
  (`scroll=1:0:0`), the first `$85` scout at 19430 and the `$1C` descender at
  19884. Pixels are report-only: 19429-23000 is 98.9% (play 98.8%, HUD 100%),
  with the priority subset from 19505 and the missing `$25` object-fire group
  from 19530 as the `WARN` row; the two entry frames are exact. The long
  run stays soft and is skipped here with `--skip-long`; when it runs, its row
  is `PASS` if both scripts are clean and a non-failing `WARN` otherwise. On a
  full run both scripts report 0 state divergences over 12,000 frames (only
  report-only pixel drift remains), so the row is `PASS`. The game-over handoff now enters `Title` at
  4676, the same frame as the oracle, and the default 12,000-frame run has 0
  state mismatches. A full run without `--skip-wasm` also passes where Node 18+
  is installed; without it the web row is `SKIP`.
- **Cross-compare (optional):** builds NESRecomp and compares it with the
  native port and the cartridge oracle via
  `tools/nesrecomp/compare-port.sh`, uploading the report. It needs the real
  ROM, so it runs only when the repository secret `GRADIUS_ROM_B64` holds the
  base64-encoded iNES image; without it the job skips every heavy step.
- **Long run (nightly, optional):** `.github/workflows/longrun.yml` runs
  `scripts/longrun.sh` on a nightly schedule (`17 4 * * *`, UTC) and on manual
  `workflow_dispatch` with an optional `frames` input (default 12000). It
  decodes the cartridge, builds `gradius-diff`, extracts a pack, runs both the
  default and boss scripts, writes the bucket tables to the run summary and
  uploads the per-script JSON, reports and errors plus `buckets.txt` as
  `longrun-report`. It reads the same `GRADIUS_ROM_B64` secret as the release
  gate and skips every heavy step without it. It is deliberately
  non-blocking: it is its own workflow and the long-run step carries
  `continue-on-error`, so any drift appears in the report and
  never fails a pull request or the release gate. A run prints two
  `## default` / `## boss` tables with one row per 1000 frames (comparable
  frames, matching state, first divergence, live-set and pixel rates) and the
  first divergence per field; the release gate keeps hard-failing on state,
  pixel and boss state, and its own longrun row is `PASS` when both scripts
  report 0 state divergences, otherwise a non-failing `WARN`.

The default members include `gradius-app`, so the runner is covered in the
normal matrix. `gradius-diff`'s real-cartridge tests are skipped without
`GRADIUS_ROM`, so no job downloads a ROM or needs the real pack; only the
optional `release`, `xcompare` and `longrun` jobs use a cartridge, and all
take it from the secret. Anything that needs real assets otherwise stays
local.

`scripts/ci-local.sh` mirrors the workflow locally, reusing `scripts/verify`'s
lock. Pass `fast` for fmt, clippy and the parity test only; the default `full`
adds the tests, wasm build, node web tests, SDL job and the headless smoke on
the extracted pack, skipping SDL when the headers are absent. The wasm build and
node tests use the same Node detection as `scripts/release-check.sh` (see
below); without Node 18+ the web build and node tests are skipped with a
message, while the wasm32 core check still runs.

```sh
scripts/ci-local.sh fast
scripts/ci-local.sh full
```

## Release gate (stages 1-5)

`scripts/release-check.sh` is the repeatable release gate for the stage-1
through stage-5 milestones. It reads one cartridge, extracts a fresh pack into
its output directory and runs every parity, audio and web check in order:

```sh
# Full gate (needs the ROM, SDL2, wasm32-unknown-unknown, node 18+ and python3;
# without node 18+ the web gate reports SKIP instead of failing):
scripts/release-check.sh "/path/to/Gradius (E).nes"

# Accept a ROM from the environment instead:
GRADIUS_ROM="/path/to/Gradius (E).nes" scripts/release-check.sh

# Skip the long run or the wasm/web build while iterating:
scripts/release-check.sh "$ROM" --skip-long
scripts/release-check.sh "$ROM" --skip-wasm
```

| gate | check | hard failure? |
| --- | --- | --- |
| build | `cargo build --release -p gradius-diff -p gradius-sdl` | yes |
| tests | `cargo test --workspace`, including `ending_replay` (oracle 7025-8290, 0 divergences) | yes |
| extract | `cargo xtask extract ROM OUT/stage1.pak` | yes |
| state | `gradius-diff --frames 3000`, must report 0 divergences | yes |
| pixel | `gradius-diff --frames 700 --pixel-report`, playfield and HUD each >= 99.9% | yes |
| s1 late | the boss script through frame 7724: the 6000-7723 pixel window (map end to the Big Core spawn) must hold >= 99.9% overall and playfield, with frame 6500's held-scroll playfield exact | yes |
| boss | frames 7500-9700 with `--keep-alive --native-keep-alive`, must report 0 divergences and the spawn/damage/clear/score/stage-2 markers | yes |
| stage2 | the boss script and keep-alives through frame 12000: state 9601-12000 must report 0 divergences and the 9601-11199 pixel window >= 99.9%; the 9601-12000 tail is a report-only `WARN` | yes |
| s2boss | the same script and keep-alives through frame 19431: the 19037-19430 hand-off pixel window must hold >= 99.9% on all three rates; 17280-19036 is a report-only `WARN` naming the open causes | yes |
| s2 full | the same script through frame 19430: state 12001-19430 must be exact (0 divergences, 7,430 frames, matching `tests/fixtures/stage2-12001-19430.state`); the natural window's pixels are a report-only `WARN` printed from the measured summary (sampled every 10th frame) | yes |
| stage3 | the same script and keep-alives through frame 23000: state 19429-23000 must report 0 divergences with the entry/scout/descender markers; the 19429-23000 pixel window is a report-only `WARN` at 98.9% (play 98.8%, HUD 100%), carrying the 19505+ priority subset and the `$25` object-fire group | yes |
| stage4 | `--stage 4` through frame 1797: state 297-1797 must carry only the shared ship-init residue, `px`/`py` on the 81 `Other(4)` respawn frames 497-523, 739-765 and 1410-1436 (162 divergences, every other field 100% and 7167/7167 object slots); the three respawn ranges are gated separately so a residue cannot move outside them; the 297-1797 pixel report sampled every 30th frame is a report-only `WARN` at 99.77% (play 99.75%, HUD 99.92%) | yes |
| stage5 | `--stage 5` through frame 12003: the generated fixtures (`tests/fixtures/stage5-297-8573.state` and `tests/fixtures/stage5-8574-12003.state`, replayed by `cargo test`) must report 0 divergences over 297-12003 (11707 frames, every field and 30863/30863 object slots, with the entry, the first `$9D` (302) / `$8E` (1174) markers, the cannon kill at 3055, the 8573 marker, the clear's core spawn (10253) / death (11489) and the stage-6 entry at 12003); the script runs the same window, so state is exact and the 297-12003 pixel window is a report-only `WARN` at 99.1% (playfield 99.0%, HUD 100.0%) | yes |
| stage6 | `--stage 6` through frame 1500: the generated fixture (`tests/fixtures/stage6-297-1500.state`, replayed by `cargo test`) must report 0 divergences over 297-1500 (1204 frames, every field and 6759/6759 object slots, with the entry scout and the first `$84` (745) / `$88` (1066) formations) plus the ROM-free clear test (`stage6_clears_to_stage_seven`, first stage-7 frame at 7171); the script runs the same window, so state is exact and the 297-1500 pixel window is a report-only `WARN` at 99.7% (playfield 99.7%, HUD 100.0%) | yes |
| longrun | `scripts/longrun.sh`, prints its buckets; `PASS` when both scripts have 0 state divergences, a non-failing `WARN` for drift or a harness error | no |
| audio | headless `--autoplay --dump-audio`, then `scripts/check-wav-triangle.py` | yes |
| perf | `examples/perf_frames` on the extracted pack, initial and stage 5 with objects alive: the per-frame `Game::step` median must stay <= 0.08 ms and the p95 <= 0.10 ms; the row prints both measurements, and above the budget it is a non-failing `WARN` | no |
| wasm size | the wasm module gzipped must stay <= 160 KiB (163,840 B); the row prints the gzip and raw byte counts | yes, `SKIP` with `--skip-wasm` or without the wasm32 target |
| web | `web/build.sh`, then `node web/fit.test.mjs`, `web/smoke.mjs` and `web/pwa.test.mjs` | yes, SKIP without node 18+ |

Every hard gate passes, including `tests` (the stage-1 pin is now 448x28);
the web gate passes where Node 18+ is installed (`SKIP` without it). A full run
with `--skip-wasm` prints `release-check: 0 hard failure(s)`. The `s1 late` gate reports 99.90%
overall and playfield (HUD 100.00%) over all 1,724 samples, with frame 6500's
held-scroll playfield exact; its `s1late rpt` row names the accepted residue:
20 HUD px on the score row at that frame (the 1-2 frame `score_delay` lag), far
inside the gate. The boss gate reports 0 divergences
across 7500-9700 and confirms the spawn at 7723, first scored hit at 8067 (the
8066 damage frame), clear at 9087 for score 2690, the post-clear slot 9 kind-2
debris and the stage-2 reset at 9601. The stage-2 gate reports 0 divergences
across 9601-12000 with the entry at 9601, a 1200 px scroll span, the first
kind `$04` spawn at 11140 and the `$91`/`$92` turret sectors at 11200/11204;
its 9601-11199 pixel window matches 99.9% (play 99.9%, HUD 100%), and the
9601-12000 tail is a non-failing `WARN` at 99.7% (accepted animation/effect
frames from 11200). The handoff is natural (the boss script's stage-1 clear,
with no `--stage` poke), and `tests/fixtures/stage2-9601-12000.state` replays
the same window ROM-free. The game-over handoff enters `Title` at 4676, the
same frame as the oracle, and the default 12,000-frame run has 0 state
mismatches. The stage-3 gate reports 0 divergences across 19429-23000 with the
entry, first scout and descender markers. The stage-5 phase reports 0
divergences across 297-12003 (11707 frames, every field 100% and 30863/30863
object slots) with the first `$9D` at 302, the first `$8E` at 1174, the cannon
kill at 3055, the core spawn at 10274, its death at 11489 and the stage-6
entry at 12003. The 297-12003 pixel row stays report-only at 99.1% (play
99.0%, HUD 100.0%) over 11706 samples. The stage-6 window reports 0 divergences across
297-1500 (1204 frames, every field 100% and 6759/6759 object slots) with the
first `$84` at 745 and `$88` at 1066, and `stage6_clears_to_stage_seven` pins
the no-core clear to stage 7 at frame 7171; the 297-1500 pixel row stays
report-only at 99.7% (play 99.7%, HUD 100.0%) over 1203 samples. The stage-4
gate
reports the accepted 162 `px`/`py` divergences on the 81 respawn frames
(497-523, 739-765, 1410-1436) with every other field 100% and 7167/7167 object
slots; its report-only sampled pixel row is 99.77% (play 99.75%, HUD 99.92%)
over 50 samples. The 241 px
sprite-palette residue remains but does not fail the gate. The extracted pack
is about 225 KB. The long run is deliberately
soft: a full 12,000-frame run reports 0 state divergences for both the default
and the keep-alive boss script, with only report-only pixel drift; its row is
`PASS` when clean and a non-failing `WARN` otherwise, so the gate still exits
0. `--skip-long` and `--skip-wasm` drop those gates for a
quicker iteration; skipped gates show as `SKIP`.

### Perf and size budgets

The `perf` and `wasm size` rows put the two numbers of
[`docs/PERFORMANCE.md`](PERFORMANCE.md) under the gate, so a phase cannot land
a regression that nobody re-measures:

| row | budget | measured (Ryzen 7 7700X, rustc 1.95, thin LTO) | state above |
| --- | ---: | ---: | --- |
| perf, median | 80 µs (0.08 ms) | 36 µs initial, 34-35 µs stage 5 | `WARN` |
| perf, p95 | 100 µs (0.10 ms) | 42 µs initial, 36-58 µs stage 5 | `WARN` |
| wasm size, gzip -9 | 163,840 B (160 KiB) | 130,349-130,777 B gzip, 376-378 kB raw | `FAIL` |

Both budgets are constants in `scripts/release-check.sh` (`perf_median_us`,
`perf_p95_us`, `wasm_gzip_max`), each with its reasoning written next to it:

- The perf budgets are 2.2x the measured median and 1.7-2.4x the measured p95
  (the p95 moves with the host's load: 36 µs in the idle `PERFORMANCE.md`
  measurement, 58 µs in a gate run). A step-loop regression of 2x (a second pass
  over the object table, a `Vec` that grows per frame) trips the row, while a
  slower machine does not; even the budget sits 166x inside the 16,639 µs frame.
  The row is a non-failing `WARN` because it is wall clock on whatever host ran
  the release check (the same reason `scripts/bench.sh` reports CPU time), and
  because at this headroom it is correctness, not frame timing, that should
  block a release. The row prints the measured median and p95 for both workloads
  either way.
- The wasm budget is about 33,000 B (+25%) over the current 130.3-130.8 kB
  gzip. The last feature phase added 17,317 B (+15.3%), so a normal phase
  passes, while a 30 KiB jump (a new dependency, a lost `opt-level`/LTO
  setting, an exported panic table) fails. A gzipped byte count follows the
  toolchain but not the host's load, so this row is a hard `FAIL`, and it prints
  `gzip N B (M KiB), raw N B`.
- The perf harness (`crates/gradius-app/examples/perf_frames.rs`) runs twice on
  the extracted pack: initial (title/attract) and `--stage 5 --warmup 600` with
  objects alive, 5,000 frames x 3 repeats each.

A feature that legitimately needs more room raises the constant and the
`PERFORMANCE.md` tables in the same change; a change that measures a smaller
number lowers them, so the budget keeps meaning "no regression since the last
phase" rather than "the day we first measured". Re-measure with
`scripts/bench.sh` (CPU time and artifact sizes) and the `perf_frames` example
(median, p95, allocations).

The web gate needs Node.js 18+ (`node:assert/strict` and the other `node:`
specifiers the tests import). `scripts/node-env.sh` takes the `node` on PATH
when its major version is 18 or newer, so an active nvm/fnm/asdf shell works.
When PATH has an older node it scans the installed versions those managers
keep on disk (`$NVM_DIR/versions/node`, fnm's `node-versions`) and picks the
newest, which is how a machine with system node v12 and an nvm v22 still runs
the gate. If no 18+ binary exists anywhere, the web gate prints
`SKIP  node >= 18 not found (have vX.Y.Z)` and the exit stays clean; that is
not a failure. Point `NODE_MIN_MAJOR` at a higher major to require a newer
line.

The script prints a compact `gate state detail` table and exits 1 when any hard
gate fails. It honours `CARGO_TARGET_DIR` and `OUT` (default
`/tmp/opencode/release-check`), where the pack, JSON reports, WAV and per-gate
logs are kept. It stages the extracted pack at `web/gradius.pak` (gitignored)
for the PWA and smoke tests instead of relying on `assets/gradius.pak`.

## Target directory

The default `target/` is fine for one checkout, but parallel worktrees must not
share a target directory. Point Cargo at a per-worker scratch path:

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-$USER
cargo build -p gradius-sdl
```

If you change the target directory, remember that `scripts/verify full`'s smoke
step looks for `./target/debug/gradius`; run that smoke command yourself from
`$CARGO_TARGET_DIR/debug/gradius`.

## Parallel work: cards, worktrees and branches

This repository is developed in parallel by workers, each with one card and one
isolated worktree. The card names the worktree (normally a sibling directory
outside the checkout, `<worktree-parent>/<name>`), the branch (normally
`agent/<name>`), the
integration branch, the files the worker owns, the acceptance criteria and the
deadline. The main checkout is read-only for workers.

The flow for one card:

1. Work inside the card's worktree. Run git without `-C`; the main checkout is
   only a read-only reference.
2. Touch only the files the card owns. If the change needs another file, stop
   and report it instead of editing outside the card.
3. Set a per-worker `CARGO_TARGET_DIR`. Wrap heavy builds or tests outside
   `scripts/verify` in `flock /tmp/verify-heavy.lock <command>` so two workers
   never share one target.
4. Reproduce a bug in a test first, then make the smallest change. Run only the
   single tests you need plus `scripts/verify changed`; integration runs the full
   suite.
5. Commit small commits on the card branch. The pre-commit hook runs
   `scripts/verify fast`; run `scripts/verify changed` yourself so the touched
   crates also get clippy and their tests. A wave runs `scripts/verify full`
   once before integration.
6. `git merge <integration branch>` (normally `main` or the wave branch named
   on the card). Resolve conflicts only in owned files, run
   `scripts/verify changed` again and commit the merge.
7. Report the status, branch and short SHA, changed files, commands run and any
   concerns. Leave unfinished work committed and named in the report.

## Troubleshooting

**`error: cannot read pack …` or `not a Gradius pack (bad magic)`**
The pack is missing or built by an incompatible extractor. Regenerate it:

```sh
GRADIUS_ROM="/path/to/Gradius (E).nes" scripts/extract-data
```

**`The system library sdl2 required by crate sdl2 was not found`**
Install the SDL2 development package listed above. The non-SDL crates still
build without it.

**`warning: audio disabled: …`**
No audio device could be opened; the game continues silently. Use `--mute` to
suppress the attempt entirely.

**The window is too small / too large**
The window is resizable; drag it, or use `--scale 1..8` for the initial size.
`--fit integer` keeps pixels sharp, `--fit aspect` fills more of a window that
is not an exact multiple, and `--fit stretch` removes the bars at the cost of
distortion. F11 uses the same policy on the monitor.

**Screenshots**
F12 writes a 256x240 BMP of the core framebuffer to `screenshots/`, named by
frame number. The directory is created on demand and ignored by git.
