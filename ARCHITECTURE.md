# Architecture

This document describes the boundaries the project relies on and the contract a
new platform backend has to satisfy. If a change breaks an invariant here, it is
a design bug, not a refactor.

## Goals and invariants

1. **The core has zero OS dependencies.** `gradius-core` does not link libc
   APIs for time, input, files, threads or audio. It compiles for
   `wasm32-unknown-unknown`, Android and iOS without cfg gates. No file handles
   or device handles cross its API.
2. **Determinism is bit-exact.** All simulation state is integer fixed-point
   (1/256 of a pixel) with a deterministic PRNG seeded per `Game`. The same
   sequence of `Buttons` produces the same framebuffer on every platform. This
   is what makes replays, differential testing and oracle comparison possible.
3. **The pack is self-contained and local.** `assets/gradius.pak`, built on the
   developer's machine by `scripts/extract-data`, holds every tile, colour,
   table and sound timeline the runtime needs. The repository ships none of that
   data. No code path opens the ROM; `gradius-cart` and `gradius-nes` are
   development-only and are not dependencies of the runtime.
4. **The oracle is development-only.** `gradius-nes` exists to measure the
   original: frames, OAM, nametable hashes, executed code. It is never a runtime
   dependency and never ships in a frontend.
5. **The runner owns timing.** `gradius-app::Runner` owns the fixed-step
   accumulator, pause state, catch-up cap and music-cue selection. A backend
   supplies wall-clock deltas, input, a destination for a 256x240 RGBA buffer
   and a sound sink. Frontends contain no game or timing rules.
6. **Differential testing is development-only.** `gradius-diff` links the
   oracle and reads a cartridge. No runtime crate depends on it; it is the only
   shipped tool that links both implementations. Recorded oracle fixtures carry
   state lines and a ROM hash, never cartridge bytes.

## Crate boundaries

```text
(runtime)
gradius.pak ─► gradius-assets ─► gradius-core ─► gradius-app ─► backend
                                                                  │
                                                                  ├─► gradius-audio (optional)
                                                                  └─► gradius-wasm (wasm32 FFI shell)

(development only)
ROM ─► gradius-cart ─► xtask ─► gradius.pak
          │
          ▼
      gradius-nes ─┐
                   ├─► gradius-diff  (also links gradius-core + gradius-assets)
gradius-cart ──────┘

tools/ghidra ─────► static analysis of the PRG window (no runtime code)
tools/nesrecomp ──► external second oracle, compared by pixel and hash
```

| crate | may depend on | must not depend on |
| --- | --- | --- |
| `gradius-core` | nothing (std only) | anything in this workspace, SDL, time, I/O, an audio device |
| `gradius-pack` | std | `gradius-core`, cartridge code |
| `gradius-assets` | `gradius-pack`, `gradius-core` (types) | `gradius-cart`, `gradius-nes`, a backend |
| `gradius-app` | `gradius-assets`, `gradius-core` | SDL, audio devices, `gradius-cart`, `gradius-nes`, `xtask` |
| `gradius-audio` | std; the `capture` feature adds `gradius-cart` and `gradius-nes` at build time | `gradius-core`, a backend, a device |
| `gradius-cart` | std | frontends, the runner |
| `gradius-nes` | `gradius-cart` | runtime crates, SDL, the runner |
| `gradius-diff` | `gradius-core`, `gradius-assets`, `gradius-cart`, `gradius-nes` | shipping in a runtime crate or a backend |
| `gradius-sdl` | `gradius-core`, `gradius-assets`, `gradius-app`, `gradius-audio`, SDL2 | `gradius-cart`, `gradius-nes`, `xtask` |
| `gradius-wasm` | `gradius-core`, `gradius-assets`, `gradius-app`, `gradius-audio` | `gradius-cart`, `gradius-nes`, `xtask`, host I/O |
| `xtask` | cart, pack, oracle (`gradius-core` as a dev-dependency) | SDL |

The workspace `default-members` omits `gradius-sdl`, `gradius-wasm`,
`gradius-audio` and `gradius-diff`, so plain `cargo test` and `cargo clippy` work
on machines without SDL2 development libraries and without a ROM.

The data for all seven stages comes from the pack: the stage tables from
`CHUNK_STAGE`, and the collision maps, backgrounds, sprites, spawn scripts and
loose tables from `CHUNK_ROM` (see [Extracted data](#extracted-data)).
`gradius-core` keeps the rules and the constants and tables of 32 numbers or
fewer. The stage→data table and background hashes are in
`docs/analysis/stages.md`; the kind × stage enemy matrix is in
`docs/analysis/stage_enemies.md`.

## Development tooling

The tools below are development-only: none is linked into a shipping frontend,
and none may become a dependency of `gradius-core`, `gradius-app` or
`gradius-assets`. `web/` is the browser demo, listed here because `web/build.sh`
is the wasm build step.

| tool | role |
| --- | --- |
| `gradius-diff` | Runs the port and the oracle in lockstep with one input script. Compares state lines per frame (per-field pass rates, object slots) and pixels per region, prints a report, writes a JSON summary and PNG pairs, and can record an oracle fixture with `--emit-fixture` so the native side is tested later without a ROM |
| `cargo xtask` | Reads the cartridge and writes analysis output: `extract` builds the pack; `inspect`, `tiles`, `sprite-groups`, `probe`, `shot`, stage-aware `screens --stage N`, `stages`, `stage-enemies`, `stage2-tables`, `sections`, `spawn`, `ramdump`, `disasm`, `trace` analyse it; `record-state`, `record-input` and `record-boss` record oracle timelines; `verify-sprite0` checks the HUD split |
| `tools/ghidra` | GhidraNes post-scripts that recover the indirect call graph and pointer tables. Current result: 214 functions, 6,636 instructions and 99.9% of the executed-PC set in a 3,000-frame stage-1 trace, with the evidence in `tools/ghidra/` |
| `tools/nesrecomp` | Builds and drives an external NESRecomp host as a second oracle, then compares the port against the oracle. Result: 3,600/3,600 hashes and 99.94% gameplay pixels |
| `scripts/verify` | The quality gate (`fast`, `full`, `changed`); see `docs/DEVELOPMENT.md` |
| `scripts/check-targets.sh` | Proves `gradius-core` stays OS-free and builds for wasm32 |
| `web/` | The browser demo: `web/build.sh` builds `gradius-wasm` and copies the bundle beside `index.html` |

The tool scripts and the oracle capture keep the repository clean: no ROM, pack,
generated C, screenshot or hash log is committed. `tools/nesrecomp` drives an
external PolyForm Noncommercial 1.0.0 checkout; its code and generated output
stay outside the repository.

## Data flow

```text
gradius.pak
  │  gradius_assets::Pack::load or Pack::parse (validates magic, version, chunks)
  ▼
gradius_core::Assets { tiles, colors, stage, rom, … }
  │
  ▼
Game::new(assets)
  │
  ▼
gradius_app::Runner::new(game)
  │
  │  windowed loop:
  │      Runner::advance(delta, Buttons, &mut SoundSink)
  │        └─ accumulator at TICKS_PER_SECOND (60.0988 Hz), capped catch-up
  │      Runner::present(&mut FrameSink) ──► present (SDL texture / canvas)
  │
  │  headless loop:
  │      Runner::run(frames, InputProvider, SoundSink) -> Summary
  │
  └─ every tick, Runner::step:
       Game::step(Buttons)
         ├─ integer fixed-point simulation
         ├─ software render into Framebuffer (RGBA8, 256x240)
         └─ queue Sound events in AudioQueue
       drain AudioQueue ───────────────► SoundSink::sound
       MusicDirector::update(&Game) ───► SoundSink::music (only on change)
       ─────────────────────────────────► SoundSink::tick(frame)
```

The frontend never asks the core for partial state. One `step` call is one
simulation tick and one complete frame, which makes pacing and headless runs
identical: `Runner::run(N, …)` calls `step` exactly N times with an
`InputProvider`.

### Input

`gradius_core::Buttons` is a plain bool struct, deliberately not an SDL enum.
The SDL backend maps keyboard and up to several hot-plugged controllers onto it
and ORs them together. A backend implements
`gradius_app::InputProvider::buttons(&mut self, frame) -> Buttons`; the runner
also accepts a `Buttons` value or a `FnMut(u64) -> Buttons` closure. Note that
the core `Buttons::from_nes_bits` uses the oracle's controller bit order;
backends should build the struct directly.

### Audio

The core's `AudioQueue` carries semantic events (`Shot`, `EnemyHit`,
`Explosion`, `PowerUp`, `PlayerDeath`, ...), not samples. `gradius-audio`
replays Konami-driver register timelines through a runtime NES APU model.
`xtask extract` captures them from the cartridge, through the `capture` feature
of `gradius-audio`, into `CHUNK_AUDIO`: 12 music tracks, eight effects and the
driver's tables. The repository embeds none of them, and without the chunk the
game plays in silence.
`gradius-app::MusicDirector` selects a platform-neutral `MusicCue` from public
game state; the backend's `SoundSink::music` maps that cue to its own engine and
`SoundSink::tick` lets an offline sink render exactly one slice per simulation
tick. The event vocabulary does not change when more captured tracks are added.

Simultaneous effects are mixed as independent APU voices, so they stack instead
of stealing a music channel as the original driver does. See `docs/AUDIO.md`
for the captured tracks, loop periods, validation counts and the two derived
cues (boss and stage clear).

If no audio device is available, the backend drains and drops events. This is a
supported mode, not an error.

## Extracted data

Everything the runtime takes from the cartridge is one pack, built locally by
`xtask extract` (run by `scripts/extract-data`). `gradius-pack` owns the
container and the chunk ids; `gradius-assets` decodes the chunks into
`gradius_core::Assets`.

| chunk | owner of the format | contents |
| --- | --- | --- |
| `CHUNK_TILES`, `CHUNK_PALETTES`, `CHUNK_META` | `gradius-pack`, `gradius-assets` | CHR tiles, palettes, provenance |
| `CHUNK_STAGE` | `gradius_core` stage tables | per-stage root table and the ending and stage 6 tables |
| `CHUNK_AUDIO` | `gradius_audio::chunk` | music tracks, effects and the Konami driver's tables |
| `CHUNK_ROM` | `gradius_core::rom` | the five families below |

`CHUNK_ROM` is a `"GROM"` payload: a version and a list of sections, each a
four-byte tag, a length and its bytes. Each family owns its tag and its encoding
in `crates/gradius-core/src/rom/<family>.rs`, with `encode` and `decode`:

| family | tag | contents |
| --- | --- | --- |
| `collision` | `COLL` | the stage collision maps |
| `backgrounds` | `BKGD` | title, stage and ending screens: tiles, attributes and the solid-tile set |
| `sprites` | `SPRT` | the sprite groups: banks A and B of `Part` lists |
| `spawns` | `SPWN` | section scripts, object and terrain definitions, formation and animation tables |
| `tables` | `TABL` | the loose tables: enemy spawns and explosions, per-stage parameters, the ending ship's motion |

A reader skips tags it does not know, and a section it needs but the pack lacks
is `RomError::Missing(tag)`. The extractor reads the cartridge for each family
in `xtask/src/rom/<family>.rs`, and runs the oracle where a table can only be
reached by running the game. The `xtask` modules that wrote Rust source with
data no longer do.

`gradius_assets` decodes `CHUNK_ROM` once and leaks it with `Box::leak`, so the
data lives for the whole process. It is carried in `Assets.rom` as
`Option<&'static RomData>`, not in a global. `Game::rom()` returns the
`&'static` reference without borrowing the game, so the core's reads do not
keep `self` borrowed. Without the chunk, `Assets::rom()` returns
`AssetError::Missing("CHUNK_ROM")`; the SDL and wasm frontends refuse to start
with a message that says to run `scripts/extract-data`.

Tests read the same data through `gradius_core::rom::testing::rom()`, which loads
`GRADIUS_PACK` or `assets/gradius.pak` and fails with the extraction command when
the pack is missing.

## Adding a backend

A backend is a crate that is not in `default-members` and depends on
`gradius-core`, `gradius-assets` and `gradius-app`, plus `gradius-audio` if it
plays the captured tracks. The SDL one is the reference shape; `gradius-wasm`
is the minimal example for a platform without threads or a device:

```text
crates/gradius-sdl/
  Cargo.toml
  src/main.rs        event loop, window/surface, screenshots, headless mode
  src/options.rs     command-line flags
  src/controller.rs  keyboard/gamepad mapping to Buttons
  src/audio.rs       optional sound sink and device
  src/music.rs       MusicCue to captured timeline
  tests/headless.rs  end-to-end headless test with a synthetic pack
```

```text
crates/gradius-wasm/
  src/lib.rs         C ABI: alloc/init/tick/audio_*/music_cue for web/app.js
```

The wasm shell keeps no event loop: JavaScript owns the canvas, the clock and
the keyboard, calls `tick(buttons)` once per frame and copies the framebuffer
and queued audio samples out of wasm memory.

`crates/gradius-sdl/src/main.rs` is the canonical starting point: copy its
structure, replace SDL with the platform's own I/O, and keep the runner and
core untouched. The responsibilities are:

1. Parse the pack path and backend-specific flags. Keep the pack error
   actionable (tell the user to run `cargo xtask extract`).
2. Load `gradius_assets::Pack`, convert with `into_assets()`, construct
   `Game::new`, then `gradius_app::Runner::new(game)`.
3. Implement `gradius_app::InputProvider` for keyboard, touch or gamepad input.
4. Implement `gradius_app::FrameSink::frame(&[u8])` and upload the 256x240 RGBA
   buffer with row pitch `WIDTH * 4`.
5. Implement `gradius_app::SoundSink`: handle `sound(Sound)`, map
   `music(MusicCue)` to the platform's audio engine and optionally render in
   `tick(frame)`. A missing device must not be fatal.
6. Windowed: pace with a monotonic clock, call
   `runner.advance(delta, buttons, &mut sink)`, then
   `runner.present(&mut frame_sink)`. Headless: call
   `runner.run(frames, &mut input, &mut sink)` and print the returned
   `Summary` (`frames`, framebuffer hash, score, lives, audio events).
7. Wire pause to `Runner::set_paused` / `toggle_pause`; do not implement a
   second clock or accumulator.

Do not reimplement rendering, pacing or music selection in the backend; those
belong to the core and the runner. Do not add ROM-reading code to a backend.

The wasm backend follows the same split: it implements the trait-free FFI the
browser demo needs and swaps in a canvas and a Web Audio sink. For wgpu, Android
and iOS the same trait implementations apply: swap the surface creation, input
source and audio device. The core and runner need no `cfg` changes.

## Testing strategy

| layer | method |
| --- | --- |
| Core rules | `cargo test -p gradius-core`: determinism, bounds, collision and input invariants |
| Pack format | `cargo test -p gradius-pack -p gradius-assets`: round-trip and malformed input |
| Asset pipeline | `cargo test -p xtask`: cartridge parsing and pack construction |
| Oracle | `cargo test -p gradius-nes`: CPU/PPU instruction behaviour |
| Runner | `cargo test -p gradius-app`: fixed-step accumulator, catch-up cap, pause, sink delivery and the headless `Summary` |
| Frontend units | `cargo test -p gradius-sdl`: option parsing, mixer envelope, input union |
| Frontend end-to-end | `crates/gradius-sdl/tests/headless.rs`: runs the real binary with a synthetic pack and checks stdout/exit status |
| Differential harness | `cargo test -p gradius-diff`: state-line parsing, compare rules, pixel regions, fixture replay of `tests/fixtures/stage1-600.state`, and flag parsing. The fixtures and the pack are required; the oracle runs skip without `GRADIUS_ROM` |
| WebAssembly | `cargo test -p gradius-wasm` (pack parsing, input bits, audio ABI) and `web/build.sh` |
| Portability | `scripts/check-targets.sh`: no-OS grep, wasm32 build of `gradius-core`, host build |

A real parity run is the `gradius-diff` binary, not a unit test:

```sh
cargo run -p gradius-diff --release -- --rom "$ROM" --pack assets/gradius.pak \
  --frames 3000 --report --pixel-report
```

It runs the native port and the cartridge oracle in lockstep, compares the
same state line each frame and reports per-field pass rates plus the pixel
match table. See `docs/DEVELOPMENT.md` for the full flag set and report format.

CI runs the default members on Linux, macOS and Windows; the SDL job adds
`gradius-sdl`. When the `GRADIUS_ROM_B64` secret is set, the test jobs decode
the cartridge, run `scripts/extract-data` and then the tests. Without the secret
they skip the tests that need data and say so in the job summary; formatting,
clippy, the file-size guard and the ROM-data guard always run. The oracle's CPU
and PPU units use synthetic programs and need no ROM.

## Failure policy

- Missing or malformed pack: a single clear error, exit 1, with the extract
  command in the message.
- No display: SDL initialization fails with a message; `--frames` still works
  and is the supported headless path.
- No audio: warning, continue silently.
- Renderer without vsync: fall back to a software-paced loop.
- Controller disconnect: drop it from the list; the keyboard keeps working.
- Differential divergence: `gradius-diff` exits 1 and prints the first
  divergence; by default only frames where both sides report `Playing` are
  compared, and `--from`/`--to` compare an explicit range instead.
