# Performance budget

The port targets the original NTSC rate, 60.0988 Hz: **16.639 ms per frame**
(usually rounded to 16.6 ms). The simulation and the software renderer must
finish a whole frame inside that budget on the weakest target we care about.

[`scripts/bench.sh`](../scripts/bench.sh) measures the process-wide cost and the
shipping artifact sizes. A second, per-frame timer
([`crates/gradius-app/examples/perf_frames.rs`](../crates/gradius-app/examples/perf_frames.rs))
reports the distribution (median, p95) and counts allocations inside the step
loop, so growth in `game.rs` can be checked without a profiler. Both are
measurement tools, not CI gates, and the result depends on the machine.

## Method

### Process cost and artifact sizes

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-perf CARGO_BUILD_JOBS=2 \
GRADIUS_ROM="Gradius (E).nes" \
    scripts/bench.sh                # --frames 10000 --repeat 3 by default
```

The script:

1. builds the release workspace (`gradius-sdl`, `gradius-nes`, `gradius-cart`,
   `gradius-wasm` for `wasm32-unknown-unknown`);
2. builds `scripts/bench.rs` with `rustc` against the workspace's release
   rlibs, so the loop runs the same optimized code as the shipping binaries
   without adding a crate to the workspace;
3. measures three workloads, interleaved and repeated three times, reporting
   the median:
   - **core** — 10,000 `Game::step(Buttons::NONE)` calls with no window, no
     audio device and no per-frame hashing;
   - **SDL headless** — the real frontend's `--frames 10000 --no-audio` path,
     which also drains sound events, updates the music director and hashes the
     framebuffer;
   - **oracle** — the development-only `gradius-nes` reference emulator,
     10,000 frames. This is the comparison baseline, not a shipping path;
4. prints the wasm module and asset-pack sizes, raw and gzipped.

Time is **CPU time (user + sys)** from `/usr/bin/time`, which equals wall clock
on an idle host and is not distorted when another job shares the machine. Max
RSS is the kernel's peak for the whole process.

### Per-frame distribution and allocations

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-perf CARGO_BUILD_JOBS=2 \
    cargo build --release -p gradius-app --example perf_frames

# Initial state: title -> attract/demo, Game::new + 30,000 steps.
/tmp/opencode/target-perf/release/examples/perf_frames \
    assets/gradius.pak --frames 10000 --repeat 3

# Stage 5 ($19 = 4) with objects alive: 600-frame warm-up, then 30,000 steps.
/tmp/opencode/target-perf/release/examples/perf_frames \
    assets/gradius.pak --frames 10000 --repeat 3 --stage 5 --warmup 600
```

`--stage N` is 1-based like the rest of the tooling; it starts a fresh game with
`Game::new_at_stage(assets, N - 1)` and enables `dev_keep_alive`, matching the
`--stage 5` diagnostic run. Each repeat rebuilds the game, runs the warm-up
(outside the timed window), steps `--frames` times and records one `Instant`
pair per frame. The percentiles are nearest-rank over all `--repeat` ×
`--frames` samples (30,000 here). A counting global allocator records
allocation calls and bytes inside the timed loop; a separate, untimed replay
counts frames whose `dump_state()` has at least one drawable object.

### Wasm size

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-perf CARGO_BUILD_JOBS=2 \
    cargo build --release -p gradius-wasm --target wasm32-unknown-unknown
f=/tmp/opencode/target-perf/wasm32-unknown-unknown/release/gradius_wasm.wasm
wc -c "$f"; gzip -9 -c "$f" | wc -c
```

These numbers were taken on an AMD Ryzen 7 7700X (8 cores / 16 threads), Linux,
Rust 1.95, `--release` with thin LTO and one codegen unit. The "before" column
is the previous measurement from this document, taken on the same machine
model and toolchain.

## Measurements

### Process cost

| workload | before (ms/frame) | now (ms/frame) | change | budget used | headroom | max RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| core, 10k `Game::step` | 0.030 | 0.033 | +10% | 0.20% | 504x | 2.8 MB |
| SDL headless, 10k frames | 0.033 | 0.035 | +6% | 0.21% | 475x | 7.4 MB |
| `gradius-nes` oracle, 10k frames | 0.318 | n/a | — | 1.91% | 52x | 2.5 MB |

The oracle was not re-run here (no cartridge in this worktree); its previous
row is kept for scale. Both moving rows use the same `scripts/bench.sh`
CPU-time method as the old table, so the change column is comparable.

### Per-frame distribution

| workload | frames | median µs | p95 µs | mean µs | alloc calls | frames with an alloc |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| initial, no warm-up | 30,000 | 35.97 | 42.47 | 35.59 | 9 | 9 |
| stage 5, 600-frame warm-up | 30,000 | 34.07 | 36.24 | 34.12 | 0 | 0 |

The stage-5 run has a drawable object in 23,866 of 30,000 frames (79.6%), with
at most 9 at once; the initial run includes the title and the attract/demo, so
it is not a gameplay worst case. Wall-clock timing adds one `Instant` pair per
frame (tens of nanoseconds), well below the spread; the CPU-time row above is
the reference for absolute cost.

Allocations are not per frame. The initial run made 9 allocations (168 bytes
total) in 9 of 30,000 frames while bounded `Vec`s grew for the first time; the
stage-5 loop made **zero** allocations after its warm-up. No `Vec` grows per
frame in the hot path.

### Artifacts

| artifact | before | now | change |
| --- | ---: | ---: | ---: |
| wasm module, raw | 263,337 (257 KiB) | 378,345 (369 KiB) | +115,008 (+43.7%) |
| wasm module, gzip -9 | 113,460 (111 KiB) | 130,777 (128 KiB) | +17,317 (+15.3%) |
| asset pack, raw | 225,384 (220 KiB) | 225,384 (220 KiB) | 0 |
| asset pack, gzip -9 | 76,304 (75 KiB) | 76,304 (75 KiB) | 0 |
| wasm + pack, gzip | 189,764 (185 KiB) | 207,081 (202 KiB) | +17,317 (+9.1%) |

The SDL row is the honest end-to-end figure: the frontend runner adds about
6% over the bare core (sound events, music cues, the framebuffer hash), and
the process peak RSS is under 8 MB. The wasm module and pack together are now
about **202 KiB gzipped**, which is the initial download for a browser build.

The workloads use no input, so the initial run stays on the title/attract/demo
path; the stage-5 run exercises its `$14`/`$0600` objects but still no player
input. Busier stage-1 frames have more sprites, but the software renderer is
bounded by a 256x240 tile/sprite pass, not by object count; re-run the bench on
a build that interests you rather than treating these numbers as a worst case.

## What this says

The growth of `game.rs` (phases 3-5, object fire, the `$14`/`$0600` cannons and
the save codec) is visible but small: the core went from **0.030 to 0.033
ms/frame** by the CPU-time method (+10%), and the per-frame wall median is
**0.036 ms** with a **0.042 ms p95**. On a 16.6 ms frame that is 0.20% of the
budget, so there is no frame-rate regression in practice: over 99.8% of the
frame is still idle, and even the p95 is 392x inside the budget.

The wasm download is the number that moved: **+17.3 KiB gzipped** (+15.3%),
from 185 to 202 KiB for wasm + pack. That is real but modest growth; it is the
metric to watch, not the frame budget.

## What the headroom means

The core uses about **0.033 ms of a 16.6 ms frame**, so over 99.8% of the frame
is idle on this machine. That margin is the point of the design: all
simulation state is integer fixed-point and the renderer writes into a
caller-owned RGBA buffer, with no allocation or operating-system call per
frame.

- **Desktop / console-like targets** have no frame-rate risk even with a
  debug build, a profiler attached or a slow interpreter layer.
- **Mobile (Android `SurfaceView`, iOS `MTKView`)** would need to be ~500x
  slower than this desktop core before simulation became the bottleneck. The
  real mobile cost is elsewhere: one 256x240 RGBA upload per frame
  (245,760 bytes, about 14.8 MB/s at 60 Hz), vsync/thermal pacing, and audio
  scheduling. Those are backend concerns; the core budget itself is not close.
- **Wasm in a browser** typically adds a 1.5-3x penalty over native, plus the
  canvas upload. Even at 5x, the core would use under 1% of a frame. The
  202 KiB gzipped download is the more relevant mobile constraint.
- **The oracle** runs the original 6502/PPU generically and is 10x more
  expensive than the port, as expected. Its 52x headroom is a sanity check on
  the baseline, not a shipping requirement.

## Caveats

- One machine, two run shapes (initial and stage 5, 10k frames, no input).
  Numbers move with CPU, compiler version and stage content; treat the tables
  as an order of magnitude, not a benchmark score.
- CPU time ignores time spent descheduled, while the per-frame table is wall
  clock. On a loaded host the wall median can run a few µs above the CPU-time
  figure; the observed worst case was still two orders of magnitude inside the
  budget.
- `gradius-audio` is a float DSP model of the APU. The SDL headless run drains
  semantic sound events but does not mix samples; mixing cost belongs to the
  backend and is not included here.
- The oracle row needs the cartridge (`GRADIUS_ROM`); without it the script
  skips that row and still measures the port and the artifact sizes.
