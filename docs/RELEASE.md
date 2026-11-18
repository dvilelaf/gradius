# Release checklist

A release is a `v*` tag on `main`. The tag starts `.github/workflows/release.yml`,
which builds the AppImage and the Windows zip and publishes them as assets of a
GitHub Release. The release job runs only when both builds pass, so a failure
publishes nothing. Pushing a tag publishes, so do the last step only with
the maintainer's go-ahead.

Work through this on the commit you are going to tag.

1. **Tree.** `git status` is clean, the commit is on `main`, and `version` in
   `[workspace.package]` of `Cargo.toml` is the new one (the file names use it).
2. **Quality gate.** `GRADIUS_ROM=<your Gradius (E).nes> scripts/verify full`
   passes, and `scripts/check-rom-data.sh` reports no ROM data. On a machine with
   the ROM, `scripts/release-check.sh` (the stage 1 and 2 gate, see
   [`DEVELOPMENT.md`](DEVELOPMENT.md)) passes too. CI runs it only when the
   `GRADIUS_ROM_B64` secret is set.
3. **Build.** `scripts/package-appimage.sh` and `scripts/package-windows.sh`
   (see [`PACKAGING.md`](PACKAGING.md)). `ls dist/` shows one AppImage and one zip
   with the same version.
4. **No game data in the artifacts.**
   - Zip: `unzip -Z1 dist/*.zip` lists only the `Gradius-<version>-windows-x86_64/`
     folder with `gradius.exe`, `SDL2.dll` and `README.txt`.
   - AppImage: extract it in an empty directory with `./Gradius-*.AppImage --appimage-extract`
     and check that `squashfs-root` has no `.pak` or `.nes` file.
5. **Smoke tests**, on those two files (commands in [`PACKAGING.md`](PACKAGING.md#smoke-tests)):
   - `packaging/linux/smoketest.sh` and `GRADIUS_ROM=<rom> packaging/linux/smoketest.sh --first-run`.
   - `packaging/windows/smoketest.sh <zip> assets/gradius.pak`, and the same with
     `--first-run "<rom>"` at the end.
   - Browser: `web/build.sh`, serve `web/`, and in a clean profile give the ROM on
     the first visit. The game starts. Then **Delete game data** removes the pack.
      The same flow is automated in `web/e2e.mjs` (Playwright, setup in its header):
      `GRADIUS_ROM=<rom> PLAYWRIGHT_CORE=<path> node web/e2e.mjs`.
6. **Version and notes.** After the Release exists, replace its generated notes
   with a short text for players: what changed and how to start.
7. **Tag.** `git tag -a vX.Y.Z -m "Gradius vX.Y.Z"` on that commit, then, with
   the maintainer's go-ahead, `git push origin vX.Y.Z`.
8. **After.** Watch the run in the Actions tab. Open the Release and check that it
   has the AppImage and the zip. To build without publishing, run the workflow by
   hand from the Actions tab (`workflow_dispatch`).

## Public repository

The public repository is not a push of this clone: the history of this clone
holds ROM data from earlier commits, and so do its tags. Build the public
repository from a clean tree with `scripts/prepare-public-repo.sh <new empty directory>`.
It exports `HEAD` (leaving out the `export-ignore` paths in `.gitattributes`)
into a new git repository with one commit on `main`, and checks that it has no
tags, remotes, other branches or game data. The script pushes nothing; it prints
the `git remote add` and `git push` command to run once the empty remote exists.

## Measured build (2026-11-12)

The numbers below were recorded on 2026-11-12, before the packaging scripts
existed and before the browser asked for the ROM. The table's "release binary"
is the plain `cargo build`, not the AppImage or the zip, and the `web/gradius.pak`
row no longer ships: the browser builds its pack itself. Keep them as a
reference for the build sizes and checks.

The pixel/state oracle matrix lives in `scripts/release-check.sh` and `docs/DEVELOPMENT.md`;
this section is only about shipping working binaries.

Reproduce every row below with one command:

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-release scripts/release-smoke.sh
```

It builds both targets, then runs them with `assets/gradius.pak` and no ROM.
Output goes to `$CARGO_TARGET_DIR/release-smoke/`; every result quoted below
comes from that script or from the browser steps noted with it.

## What is built

| artifact | build | size |
| --- | --- | --- |
| `target/release/gradius` — SDL2 desktop binary | `cargo build --release -p gradius-sdl` | 1,072,896 B (1.0 MiB) |
| `web/gradius_wasm.wasm` — wasm backend | `web/build.sh` (release, `wasm32-unknown-unknown`) | 463,686 B raw, 153,718 B gzip |
| `web/gradius.pak` — self-contained asset pack | `cargo xtask extract <rom> <pak>` (source `assets/gradius.pak`) | 228,404 B |

The PWA shell that is uploaded next to the wasm module: `index.html` (4,394 B),
`app.js` (16,353 B), `layout.mjs` (929 B), `audio-worklet.js` (1,477 B),
`manifest.webmanifest` (546 B), `service-worker.js` (2,043 B). A browser
downloads roughly 150 KiB of gzip wasm plus the 228 KiB pack, so about 380 KiB
for a first, uncached load. The gzip module is 153,718 B against the 160 KiB
budget in `scripts/release-check.sh`.

Built and checked on 2026-11-12 with rustc 1.95.0, Node 22.23.4 (the web gates
need Node >= 18; `scripts/node-env.sh` finds one).

## Starting it

Desktop, from the repository root (the default pack path is relative):

```sh
cargo build --release -p gradius-sdl
target/release/gradius                                   # window, integer 3x, audio on
target/release/gradius --fit aspect --scale 4            # any integer scale 1..=8
target/release/gradius --pack /path/to/gradius.pak       # pack anywhere
```

Headless tick run (no SDL, no window, no audio device); prints one summary line
with the framebuffer hash, score, lives and queued sound events:

```sh
target/release/gradius --pack assets/gradius.pak --frames 600 --autoplay --no-audio --scale 1
```

Keys: arrows/WASD move, `Z`/`J`/Space fire, `X`/`K` power, `Enter` start,
`F5` save `savestate.grs` in the working directory, `F9` load it, `F11`
fullscreen, `F12` BMP screenshot, `F3` debug overlay, `Esc` quit.

Browser PWA:

```sh
# any Node >= 18 on PATH; scripts/node-env.sh can find an nvm/fnm install
export PATH="$HOME/.nvm/versions/node/v22.23.4/bin:$PATH"
web/build.sh                    # wasm bundle and extractor wasm into web/
cd web && python3 -m http.server 8777 --bind 127.0.0.1
# open http://127.0.0.1:8777/  (a secure context, so the service worker registers)
```

Ship the whole `web/` directory except the `*.test.mjs` / `smoke.mjs` /
`e2e.mjs` files; `gradius.pak` and `gradius_wasm.wasm` are required even
though `.gitignore` keeps them out of the repository.

## Checks on the release build

| check | command | result |
| --- | --- | --- |
| builds | `cargo build --release -p gradius-sdl && web/build.sh` | exit 0 |
| starts and reaches the title | `gradius --pack assets/gradius.pak --frames 60 --no-audio --scale 1 --screenshot title.bmp` | `hash=0x48f323d620cc2c1e score=0 lives=0`; title BMP 184,374 B |
| Start accepted, frames advance | `gradius ... --frames 600 --autoplay` vs `--frames 600` | autoplay `score=60 lives=3 hash=0xd53359fa716cb585`; attract `score=0`, so the score can only come from the Start press at frame 120 |
| deterministic | the 600-frame run twice | same hash `0xd53359fa716cb585` |
| pack is self-contained | copy only `gradius.pak` to an empty directory and run there | identical hash `0xd53359fa716cb585`; no ROM or `assets/` present |
| F5 saves | Xvfb + `xdotool key F5` | `savestate.grs` 2,877 B, magic `GRSS`, header frame 119 |
| F9 loads | play 1.5 s (~90 frames), `xdotool key F9`, then F5 again | re-saved header frame 129: the slot rewound to the saved frame plus the 10 frames between the two key presses |
| PWA loads and renders | serve `web/`, open `http://127.0.0.1:8777/` | canvas shows the Konami boot logo; status line empty (init finished), `Load` disabled with no slot |
| PWA save to `localStorage` | click `Save` | `Load` becomes enabled (the write went through `state_save` + `localStorage`) |
| PWA load after a reload | open the page again, click `Load` | status "saved state ready (F9 or Load)" after the reload, `Load` click handled with no console error |
| fit modes | `node web/fit.test.mjs` | `integer, fit, stretch and letterbox cases pass` |
| wasm + savestate round trip | `node web/smoke.mjs assets/gradius.pak` | `savestate 3085 B, 60/60 frames match, corrupt refused`; title/stage audio RMS in band |
| manifest, icons, offline | `node web/pwa.test.mjs` | `1 icons, 11 precached files, offline navigation and HTTP load pass` |

The headless frontend has no savestate flag, so the F5/F9 rows drive the real
windowed binary under Xvfb with `xdotool`; that is the same event path a desktop
user takes. The browser `SAVED`/`LOADED` text lives in the status line for two
seconds, so the click result was read from the enabled `Load` control, the
post-reload status and the byte-exact wasm round trip in `web/smoke.mjs` instead
of from that notice.

Not covered here: the pixel/state oracle matrix, perf budget, wasm gzip size
gate and the audio dump belong to `scripts/release-check.sh`, which needs the
cartridge. Run it in CI or on a machine holding the ROM before a tag.
