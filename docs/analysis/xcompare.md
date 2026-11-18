# Native port vs NESRecomp, with the oracle as reference

This note measures the port (`gradius`, `--pack`) against NESRecomp's headless
cycle host and against this repository's own emulator (the oracle used by
`gradius-diff`). It is a picture comparison; input is either attract/no input or
the usual start-and-play script. Background on NESRecomp, its pin and its
limitations is in [`nesrecomp.md`](nesrecomp.md).

The three comparisons are:

| label | A | B | meaning |
| --- | --- | --- | --- |
| (a) | NESRecomp | oracle | does the independent NES model render the original? |
| (b) | native port | oracle | the port's parity claim |
| (c) | native port | NESRecomp | the two reimplementations against each other |

`compare.py` is the measuring tool: pixel-exact percentage, a 16x15 region grid
on the worst frames, first differing pixel. Exact RGB match, no tolerance.

## Build and ROM

```sh
export NESRECOMP_BUILD=/tmp/opencode/nesrecomp-ref
ROM="$GRADIUS_ROM"
export GRADIUS_ROM="$ROM"

# NESRecomp host (once; see nesrecomp.md for the pin and layout)
tools/nesrecomp/setup.sh

# Native side: gradius (standalone) and gradius-diff (lockstep oracle+native)
CARGO_TARGET_DIR=/tmp/opencode/target-docfix \
    cargo build --release -p gradius-sdl -p gradius-diff
```

The ROM is read by both emulators and is identity-checked by NESRecomp; it is
never copied into the repository. `assets/gradius.pak` is needed by the native
side and is ignored by git.

## Commands

Attract / no input, frames 0..600, one sampled frame every 10:

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-docfix \
tools/nesrecomp/compare-port.sh --rom "$ROM" \
    --frames 601 --every 10 --out /tmp/opencode/xcompare-attract
```

Scripted gameplay (Start at 180 and 240, then fire held with up/right/left in
60-frame blocks from 300), frames 0..1200:

```sh
cat > /tmp/opencode/xcompare-play.script <<'EOF'
180:start:6
240:start:6
300:a+up:60
360:a+right:60
420:a+left:60
480:a+up:60
540:a+right:60
600:a+left:60
EOF

CARGO_TARGET_DIR=/tmp/opencode/target-docfix \
tools/nesrecomp/compare-port.sh --rom "$ROM" \
    --frames 1201 --every 10 --script /tmp/opencode/xcompare-play.script \
    --out /tmp/opencode/xcompare-play
```

The script runs, over one frame window:

1. `run.sh` (NESRecomp headless) with `--screenshot ... --shot-every N`.
2. `gradius-diff` with `--png-dir` and the same sample stride; it writes
   `oracle-XXXX.png` and `native-XXXX.png`.
3. the standalone `gradius --pack --frames N --screenshot final.bmp`. It has no
   input flag, so this last step is skipped when `--script` is given.
4. `compare.py` for every sampled frame of each pair, into `pixels.csv` and
   `report.txt`, with per-region 16x15 output for each pair's worst frame.

A `--script` uses `gradius-diff`'s `FRAME:buttons[:duration]` format;
`compare-port.sh` converts it to NESRecomp's `<frame> [buttons]` hold schedule,
so one file drives both. An empty script (the default) is attract/no input.

All three sources index a frame after it has run: sampled files are post-frame
N, so `frame_00NNN.png`, `oracle-0NNN.png` and `native-0NNN.png` with the same N
are the same frame. Only frames `N % every == 0` are written; the last frame is
compared only when `(frames - 1) % every == 0`.

## Measured results

Attract 0..600, 61 samples (2026-11-09, NESRecomp pin
`4662494`, native release in `/tmp/opencode/target-docfix`):

| pair | mean | min | max | worst frame |
| --- | --- | --- | --- | --- |
| (a) NESRecomp / oracle | 90.4104% | 0.0000% | 100.0000% | F0 (power-on grey vs black) |
| (b) native / oracle | 94.3363% | 83.4229% | 100.0000% | F550 (demo vs title) |
| (c) native / NESRecomp | 85.8222% | 0.0000% | 100.0000% | F0 |

Scripted 0..1200, 121 samples:

| pair | mean, all frames | mean, gameplay F>=300 | min, gameplay |
| --- | --- | --- | --- |
| (a) NESRecomp / oracle | 93.9280% | 95.8863% | 94.7347% (F1090) |
| (b) native / oracle | 99.9384% | 99.9186% | 99.3294% (F1080) |
| (c) native / NESRecomp | 93.9244% | 95.8820% | 94.6859% (F1080) |

In gameplay the native/oracle pair is exactly 100.0000% on 56 of the 91 samples
from F300 on; the rest differ by at most 0.6706% (412 pixels, F1080).

### Where the differences are

**F0, NESRecomp.** Its first sampled picture is a uniform `(102,102,102)` grey,
while the oracle and the port are black. This is a power-on/backdrop frame in
NESRecomp's PPU model, not game output; the pair is 0% only because every pixel
differs. F10 is already 100%.

**Title roll-in, F10..130 (native/oracle 100%, NESRecomp follows the roll).**
The oracle slides the title in from the left over roughly 130 frames, and the
port now does the same: every sampled frame F10..130 is 100.0000% for pair (b).
NESRecomp draws the same clipped image, so its gap to the oracle is palette
only; the pair (a) figure falls from 100% at F10 to 86.9303% at F130 as more of
the coloured title fills the screen (94.3913% at F60, where both show the same
clipped logo).

**Stable title, F140..380 (NESRecomp 86.7334%).** Layout matches; the 13.27%
difference is NESRecomp's palette. The 2 px title-ship offset is gone: the
port's left fin at `(72,141)-(73,141)` now matches the oracle's black, and all
132 scripted title samples in the 700-frame run are pixel-identical. On this
no-input path the port still applies the f184 blink where the oracle's title
is static (the selected-row blank), which is the remaining title-path
difference; the scripted run, where Start enters the countdown, matches the
blink. Representative colour pairs at F200:

| oracle | NESRecomp | pixels |
| --- | --- | --- |
| `(0,124,141)` logo teal | `(0,122,201)` blue | 2546 |
| `(181,49,32)` red | `(198,53,0)` red | 1552 |
| `(254,196,234)` pink | `(255,191,219)` pink | 1309 |
| `(255,254,255)` white | `(255,255,255)` white | 2682 |

**Title off, F385..425.** The oracle blanks the screen before the attract demo;
NESRecomp matches the black exactly (100%), the port keeps the title visible
(86.7301%). In the scripted run this blank window now matches: native/oracle is
100.0000% on F270..290 and passes straight into gameplay at F300.

**Attract demo, F430..600.** The oracle and NESRecomp start the self-playing
demo; the port holds the `Other(1)` title window for the whole no-input run.
This is the 83.4..84.0% floor in (b) and (c): a title screen against a gameplay
screen, not a rendering bug. The attract title screen itself - roll-in,
nametable and palette - is now reproduced (see above); what is still not ported
is the demo the ROM enters after it.

**Gameplay, F>=300.** (a) and (c) sit at 95..97%, and that ceiling is the same
palette table: in normal play the two most common pairs are the HUD white
`(255,254,255)` vs `(255,255,255)` (~1030 px) and the HUD blue
`(87,43,255)` vs `(66,64,255)` (~580 px). The port's own diffs are now 241
colour-only pixels over 24 samples of the 700-frame default run: the first is
one pixel at frame 391 (`206,50`), the worst is frame 430 at 38 px (99.9000%,
0.062% of the screen), and every differing position is opaque on both sides,
so the old dropped-sprite/flicker-phase symptom is gone. See `pixels.md`.

## Limitations

- **NESRecomp's PPU/APU are its own models, not the console.** The host reports
  `ppu: implementation=LLE (build-fixed)`. A build-time HLE raster kernel exists
  (`NESRECOMP_PPU_HLE=1`) but was not used, so these numbers describe the LLE
  model only; HLE timing would move the raster and should not be mixed with
  them. The APU is likewise NESRecomp's model; this comparison looks at pixels
  only, so audio is out of scope (`--wav-out` is available for a separate check).
- **Master palette, not just rendering.** Exact RGB comparison penalises
  NESRecomp for its palette table. The measured 95..97% gameplay figure is
  dominated by a handful of colour entries, not by different layout. Treat pair
  (a)/(c) as "layout nearly identical, palette not bit-identical"; the 100%
  frames are black screens, where the palette cancels.
- **ROM and pack required.** The ROM is identity-checked by NESRecomp; the port
  needs `assets/gradius.pak`. Neither is committed and all output stays under
  `--out` (normally `/tmp/opencode/...`).
- **Sampling.** The runs above use `--every 10`. `--every 1` is supported but
  costs 601 (or 1201) `compare.py` invocations per pair. The sample stride must
  divide the window so the last frame is included.
- **Interpreter fallback.** NESRecomp prints the split on every run: 78.8%
  native cycles for the attract window, 46.7% for the scripted one. A picture
  match does not prove the recompiled path covered every routine.
- **Standalone check is final-frame only.** `gradius --screenshot` writes one
  BMP and has no input flag. In the attract run it agreed with the lockstep
  native PNG 100%, so the shipped binary and `gradius-diff` render identically;
  it is not a per-frame check.
- **Scope.** Two windows, one ROM, one pin, one input script. The attract-demo
  divergence is a port behaviour, not an oracle artefact; it is not visible in
  the scripted parity claim because Start is pressed first.

## Verified run

2026-11-09, worktree `docfix`, `CARGO_TARGET_DIR=/tmp/opencode/target-docfix`:

- Both `compare-port.sh` invocations above exited 0 and wrote `pixels.csv`,
  `report.txt` and all PNGs/BMP under `/tmp/opencode/docfix-xa` and
  `/tmp/opencode/docfix-xp`.
- NESRecomp host lines: `ppu: implementation=LLE (build-fixed)`,
  `mode=native frames=601 cycles=17895703 native_cycles=14096434 (78.8%)` and
  `frames=1201 cycles=35764005 native_cycles=16743934 (46.8%)`.
- `gradius-diff`: `first divergence: none`, `objects: slot match 4286/4286
  (100.0%)` on the scripted run.
- Standalone port vs lockstep native at F600: 100.0000%; standalone hash
  `0xb0a7743ea9d6b0ff`.
