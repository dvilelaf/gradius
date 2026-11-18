# Portability

`gradius-core` is the whole game: rules, collision, stage script, HUD and the
software renderer. It has no operating-system dependency, so the same crate is
meant to back a desktop window, a browser canvas, an Android `SurfaceView` and
an iOS `MTKView` without `cfg` gates or platform forks.

One command proves the current state:

```sh
scripts/check-targets.sh        # guard + wasm32 build + host build
scripts/check-targets.sh guard  # OS-API grep only  (CI: portability job)
scripts/check-targets.sh wasm   # wasm32 build only (CI: wasm job)
```

The script exits non-zero with a clear message on the first failure.

## What the core guarantees

| guarantee | how it holds |
| --- | --- |
| No OS dependency | `gradius-core` never references `std::fs`, `std::net`, `std::process`, `std::thread` or `std::time`, and has no dependencies of its own. `scripts/check-targets.sh guard` greps for those modules; CI fails if one appears |
| Builds for wasm | `cargo check -p gradius-core --target wasm32-unknown-unknown` passes today; CI runs it in the `wasm` job |
| No `unsafe` | `#![forbid(unsafe_code)]` at the crate root |
| Bit-exact simulation | all simulation state is integer fixed-point (1/256 of a pixel) with a deterministic per-`Game` PRNG. There is no floating point in the simulation, so the same `Buttons` sequence produces the same framebuffer on every target |
| RGBA sink, no windowing assumptions | `Game::step(Buttons)` renders one complete 256x240 RGBA8 frame into a caller-owned buffer, read with `game.framebuffer().as_rgba()` at row pitch `WIDTH * 4` (1024 bytes). The backend decides where those pixels go |
| Events, not samples | `game.audio.drain()` yields semantic `Sound` events; the backend owns the device and the mixing |
| Cross-target comparison | `Game::state_hash()` FNV-1a hashes framebuffer plus score, lives, section, scroll and frame; the headless runner prints the framebuffer hash in the same format on every platform |

The core uses `std` (`Vec`, collections), not `no_std`. It is portable to any
target with a `std` implementation; a bare-metal target would need an allocator
crate first, which is out of scope.

Audio is the one deliberate exception to bit-exactness: `gradius-audio` is a
float DSP model of the NES APU, so sample values are not part of any frame
hash. The frame hash covers the picture, which is what the oracle compares.

## The backend contract

A backend is a crate outside `default-members` that depends on `gradius-core`,
`gradius-assets` and `gradius-app`, plus `gradius-audio` if it plays the
captured tracks. The runner owns the clock, pause and catch-up; the backend
implements the three runner traits and supplies the platform I/O:

1. **Pack bytes.** `gradius_assets::Pack::parse(&[u8])` is pure: no I/O, no
   path. (`Pack::load(path)` uses `std::fs` and is host-only convenience.)
   Build `Game::new(pack.into_assets())` and wrap it in
   `gradius_app::Runner::new`.
2. **Input.** Implement `gradius_app::InputProvider` and map keys, touch or
   gamepads onto `gradius_core::Buttons`. The core never reads a device.
3. **A surface.** Implement `gradius_app::FrameSink` and upload the RGBA slice
   at row pitch `WIDTH * 4`, at an integer scale.
4. **An audio sink.** Implement `gradius_app::SoundSink`: handle `sound(Sound)`,
   map `music(MusicCue)` to `gradius_audio::Engine`, and optionally render in
   `tick(frame)`. A missing device is a warning, not an error.
5. **A clock.** Windowed: pass a monotonic delta to
   `Runner::advance(delta, buttons, &mut sink)`; the runner accumulates at
   `TICKS_PER_SECOND` (60.0988 Hz) and clamps catch-up after a stall. Headless:
   call `Runner::run(frames, &mut input, &mut sink)` and print the `Summary`.
6. **Errors and lifecycle.** Pack errors are the backend's to print; a failed
   device open must not abort a `--frames`-style headless run. Wire pause to
   `Runner::set_paused` / `toggle_pause`, not a second clock.

Game rules and rendering live in the core; pacing and music-cue selection live
in the runner. A backend must not reimplement them and must not read the ROM.

## Current backends

| backend | crate / entry point | notes |
| --- | --- | --- |
| SDL2 desktop (Linux, macOS, Windows) | `crates/gradius-sdl` over `gradius-app` | Window, keyboard and hot-plugged controllers, integer scaling, fullscreen, pause, BMP screenshots, audio device. Excluded from `default-members` so the rest of the workspace builds without SDL2 |
| WebAssembly (browser) | `crates/gradius-wasm` + `web/` | `cdylib` over the core and pack reader; keyboard and touch input, responsive 2D canvas (integer, fit or stretch), fullscreen, pause. Excluded from `default-members` like the other frontends |
| Headless app runner | `gradius-app::Runner::run` via `gradius-sdl --frames N` | No window and no audio device: calls `step` exactly N times, prints one summary line with the framebuffer hash. `scripts/smoke.sh` and the SDL CI job use it |
| Differential harness | `crates/gradius-diff` | Development-only lockstep runner; compares the port against the oracle state and pixels. Not a backend and never linked into a frontend |
| Development host tools | `xtask`, `gradius-cart`, `gradius-nes` | Read the cartridge, build the pack, run the oracle. They are host-only by design and never linked into a frontend |

There is no Android or iOS frontend yet; those are the next steps below.

## Next steps

### WebAssembly (`wasm32-unknown-unknown`)

The shipped backend is `crates/gradius-wasm` plus the static page in `web/`: a
`cdylib` over `gradius-core` and `gradius-assets` that links no SDL and reads
no ROM. It exports a raw `extern "C"` ABI (no `wasm-bindgen`):

| export | meaning |
| --- | --- |
| `alloc(len) -> ptr` | reserve a buffer in wasm memory for the pack |
| `dealloc(ptr, len)` | free that buffer |
| `init(pack_ptr, pack_len) -> u32` | parse the pack from wasm memory; 0 is success |
| `tick(buttons) -> ptr` | advance one frame and return the 256x240 RGBA8 buffer |
| `frame_count() -> u32` | frames simulated since `init` |

`buttons` is one byte in NES controller order (`fire=0x01`, `power=0x02`,
`select=0x04`, `start=0x08`, `up=0x10`, `down=0x20`, `left=0x40`,
`right=0x80`), the same order as `Buttons::from_nes_bits` and the SDL
frontend's keys. The page maps arrows to the d-pad, Z to fire, X to power,
Enter to start, P to pause, M to mute and F to toggle fullscreen. It drives
`tick` from `requestAnimationFrame` with the same 60.0988 Hz fixed step and
quarter-second catch-up cap as `gradius_app::Runner`, then uploads the RGBA
slice to a 2D canvas. `web/app.js` is plain JavaScript; there are no frameworks
and no runtime requests beyond the wasm module, the pack and the small shell
files precached by the service worker.

### Web audio from the pack

`init` copies the pack, parses it with `gradius_assets::Pack` and decodes the
`GAUD` audio chunk with `gradius_audio::AudioBank`. A version 2 chunk yields all
14 captured timelines and the `KDRV` driver bank, so the same extract that feeds
the native frontends feeds the page. The wasm backend plays the timelines with
the same cue map as the SDL frontend; the driver bank is decoded and covered by
the Rust test but is not used for playback. The triangle linear-counter reload
lives in `gradius_audio::Engine`, so the pack's title, stage 1 second half, Big
Core and game-over tracks keep their bass line on the web too.

`web/smoke.mjs` loads `web/gradius.pak`, renders two seconds of the title and
two seconds of stage 1 through the exported ABI, and asserts a non-empty queue,
RMS in `0.01..=0.5`, finite samples and a low band (below 200 Hz) RMS of at
least `0.02`. The bass check fails with a message naming the triangle channel,
so a pack whose triangle reload is missing is caught before it ships. With the
current pack the title renders at RMS 0.114 / bass 0.102 and stage 1 at RMS
0.098 / bass 0.086 (peak 0.401 and 0.192). The Rust tests also decode a
synthetic version 2 chunk with a `KDRV` section, check the timeline and driver
banks, and repeat the title/stage queues and RMS bounds.

The canvas keeps a 256x240 backing buffer and is scaled with
`image-rendering: pixelated`. The **Fit** selector in the page controls the CSS
box:

| mode | behaviour |
| --- | --- |
| `integer` (default) | whole-pixel steps that fit the window; below 1:1 it falls back to a fit so a phone still shows the whole frame |
| `fit` | largest aspect-correct box, fractional scale |
| `stretch` | fills the window, distorting the 16:15 aspect |

In `integer` and `fit` the free space around the canvas is black, so 16:9,
21:9 and portrait screens letterbox instead of cropping. The selected mode is
saved in `localStorage` (`gradius.fit`). The fullscreen button and F key use the
Fullscreen API on the canvas stage. On touch devices (`navigator.maxTouchPoints`
or a coarse pointer) the page also shows an on-screen d-pad plus Fire, Power and
Start buttons; they feed the same input byte and are hidden on mouse-only
devices.

The page is also an installable PWA. `web/manifest.webmanifest` names the app
and points at `icon.svg`, an original drawing: the title-screen art cannot be
shipped as an icon.
`web/service-worker.js` precaches `index.html`, `app.js`, `layout.mjs`,
`audio-worklet.js`, `gradius_wasm.wasm`, the manifest, `icon.svg` and the
extractor files into a versioned cache (`gradius-pwa-v1`). The pack is never
cached: it lives in IndexedDB. Fetches are cache-first, and a
navigation request that misses the network falls back to the cached page, so a
second visit works offline. Bump `VERSION` when a cached file changes; the
activate handler drops older caches. When the browser fires
`beforeinstallprompt`, the page shows an **Install** button next to Fullscreen.

Build and serve:

```sh
web/build.sh                 # release build for wasm32-unknown-unknown
python3 -m http.server -d web 8000
# open http://localhost:8000/
```

On the first visit the page asks for the player's own `Gradius (E).nes`. The
extractor (`gradius_extract_wasm.wasm`, built by `web/build.sh` from
`gradius-extract`) runs in a Web Worker, and the resulting pack is kept in
IndexedDB, so later visits start without the ROM. **Delete game data** removes
it. The pack is not in `web/` or on the server.

`web/build.sh` installs the `wasm32-unknown-unknown` target through rustup if
missing, copies `gradius_wasm.wasm` and `gradius_extract_wasm.wasm` next to
`index.html`, and checks that the manifest and service worker exist. A
`web/gradius.pak`, if one is there, is used as a local shortcut and is never
precached. Serve the directory over HTTP: `file://` cannot fetch the wasm or register a service
worker, and offline caching needs an `http://localhost` or HTTPS origin. The
game itself reads only the pack; the ROM is read once, by the extractor in the
browser.

Tests and the target check:

```sh
cargo test -p gradius-wasm
CARGO_TARGET_DIR=/tmp/opencode/target-wasm \
    cargo build -p gradius-wasm --release --target wasm32-unknown-unknown
node web/fit.test.mjs        # canvas fit math for all three modes
node web/smoke.mjs           # after web/build.sh: pack, ticks and audio
node web/pwa.test.mjs        # manifest, icons, HTTP load, offline reload
```

The unit tests cover the input bit mapping, one RGBA frame, pack version 2
audio (timelines plus the `KDRV` driver bank) and the title/stage queues with
RMS bounds; the headless build check proves the crate links for the target. The
Node tests need no packages: `fit.test.mjs` checks the letterbox and stretch
geometry, `smoke.mjs` drives the built module with the extracted pack and checks
the queued audio and bass band, and `pwa.test.mjs`
parses the manifest, checks the icon, loads every runtime file over HTTP and
runs the service worker in a sandbox to answer an offline navigation.
`web/build.sh` does not run `make-icons.mjs`; the icon is `icon.svg`.

### Android

A JNI shell (Kotlin `Activity` plus a Rust `staticlib`) or a `winit`/`wgpu`
`NativeActivity`:

- build a small `cdylib` wrapper crate that re-exports the core; keep
  `gradius-core` itself unchanged;
- read the pack from `assets/` through `AAssetManager` into a `Vec<u8>` and
  call `Pack::parse`;
- present to a `SurfaceView`/`SurfaceTexture` (GLES) or a `wgpu` surface, at
  integer scale;
- pace from `Choreographer` frame callbacks through the same `Runner`;
- map `KeyEvent` and `InputDevice` gamepad axes onto `Buttons`, plus on-screen
  touch controls;
- render audio in an Oboe/AAudio callback via `Engine::fill`; silence is allowed;
- handle pause/resume in the shell only: the core holds no device or file
  handles, so there is nothing to restore.

### iOS

A Swift/Objective-C shell around a Rust `staticlib` (`aarch64-apple-ios`):

- bundle the pack as a resource, read it with `NSData` and call `Pack::parse`;
- present with `MTKView` (Metal) or a `wgpu` surface at integer scale;
- pace with `CADisplayLink` through the same `Runner`;
- map `GCController` and a touch overlay onto `Buttons`;
- render audio through `AVAudioEngine` or `AudioQueue` via `Engine::fill`.

For all three, the runner traits and the contract items above are the complete
list. If a port needs a new item, the contract is incomplete and this document
is wrong.

## Verifying a new backend

1. `scripts/check-targets.sh` still passes; add the backend's target to the CI
   `wasm` job (or a sibling job) once it exists.
2. `cargo test -p gradius-core -p gradius-app` covers the core rules and runner;
   a backend adds an end-to-end run with a synthetic pack, like the SDL
   headless test.
3. Run the headless runner on two platforms with the same pack and compare the
   printed framebuffer hash. Equal hashes are the determinism claim, not a
   sample of it.
4. Never `cfg`-gate the core. If a platform needs different rules, the rule
   belongs in the backend, not in `gradius-core`.
