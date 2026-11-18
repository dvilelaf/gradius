# NESRecomp as a second oracle

NESRecomp is a static recompiler: it translates the ROM's 6502 code to C and
runs it on its own cycle-stepped NES machine. A Gradius (E) build is a useful
independent check on this repo's emulator and xtask oracle, but it is a second
opinion, not hardware, and it is not part of this repository.

Reference: <https://github.com/mstan/nesrecomp>, pinned at
`466249489898be9f92aa369de4bbc5aad0d8feb2` (2026-11-07, "Merge mod
dependencies and fullscreen shortcuts controller").

## Requirement and ROM identity

- Linux, `git`, `cmake` 3.20+, a C compiler, and Python 3.11+ (NESRecomp's CMake
  bridge imports `tomllib`). Ninja is used when present; `SDL2` is not needed
  for the headless host.
- Tested ROM: `Gradius (E).nes`, 65,552 bytes, mapper 3 (CNROM), 32 KiB PRG and
  32 KiB CHR, SHA-256
  `268dc4b5b344e06e96281eb3159c1f61d3011c4f21dd1a2b4508aa934aff6143`.
- The ROM is read at runtime and its identity is checked by the generated host.
  It is never copied into the repository or generated code.

The ROM path in the commands below is the local test image; substitute your own
legal copy:

```sh
ROM="$GRADIUS_ROM"
export NESRECOMP_BUILD=/tmp/opencode/nesrecomp-ref
```

## Setup

`setup.sh` clones the pinned checkout (unless `NESRECOMP_SRC` already points at
one), builds the C11 recompiler, writes a minimal `game.toml`, and configures
and builds the headless cycle host. Code generation runs at CMake configure
time, so the generated C stays in the build tree.

```sh
cd "$NESRECOMP_SRC"   # the cloned checkout, or a directory that setup.sh fills
NESRECOMP_BUILD=/tmp/opencode/nesrecomp-ref \
GRADIUS_ROM="$ROM" \
tools/nesrecomp/setup.sh
```

Environment:

| Variable | Meaning |
| --- | --- |
| `NESRECOMP_BUILD` | Required artifact root; everything is written below it |
| `GRADIUS_ROM` | Required iNES image |
| `NESRECOMP_SRC` | Optional checkout path (default `$NESRECOMP_BUILD/nesrecomp`) |
| `NESRECOMP_COMMIT` | Optional pin override (default above) |
| `NESRECOMP_JOBS` | Optional parallel build jobs |

Layout:

```
$NESRECOMP_BUILD/nesrecomp/                   pinned source checkout
$NESRECOMP_BUILD/recompiler/NESRecomp         recompiler
$NESRECOMP_BUILD/game.toml                    [game] output_prefix/cycle_accurate
$NESRECOMP_BUILD/game/nes_game                headless cycle host
$NESRECOMP_BUILD/game/cycle-nes_game/<key>/generated/game_cyc*.c
```

The generated `game.toml` is only:

```toml
[game]
output_prefix = "gradius-e"
cycle_accurate = true
```

## Run and compare

`run.sh` supplies the generated host and ROM. It translates `--interp` to
`--interp-only`; every other flag goes to NESRecomp's runner unchanged.

```sh
cat > /tmp/opencode/nesrecomp-ref/route.txt <<'EOF'
0 -
240 START
260 -
300 RIGHT A
EOF

export NESRECOMP_BUILD=/tmp/opencode/nesrecomp-ref
export GRADIUS_ROM="$ROM"

tools/nesrecomp/run.sh --frames 3600 --input /tmp/opencode/nesrecomp-ref/route.txt \
    --hash-out /tmp/opencode/nesrecomp-ref/native.hash \
    --screenshot /tmp/opencode/nesrecomp-ref/native.png

tools/nesrecomp/run.sh --interp --frames 3600 \
    --input /tmp/opencode/nesrecomp-ref/route.txt \
    --hash-out /tmp/opencode/nesrecomp-ref/interp.hash \
    --screenshot /tmp/opencode/nesrecomp-ref/interp.png

cmp /tmp/opencode/nesrecomp-ref/native.hash /tmp/opencode/nesrecomp-ref/interp.hash
```

One `--hash-out` line per frame covers the observable trace, CPU RAM/picture,
CPU registers and hardware-internal hashes. `--screenshot` writes the last
frame as 256x240 PNG. Useful extras are `--shot-every N`, `--input` (the
`<frame> [buttons]` schedule above), `--miss-log`, `--frame-log` and
`--wav-out`; `runner/cyc/cyc_host.c` documents the full set.

`compare.py` reports pixel-exact percentage, a per-region grid and the first
differing pixel. It handles PNGs and raw row-major RGBA dumps (the latter need
`--size`); raw input does not need Pillow, PNG input does.

```sh
python3 tools/nesrecomp/compare.py native.png interp.png
python3 tools/nesrecomp/compare.py native.rgba interp.rgba --size 256x240
python3 tools/nesrecomp/compare.py native.png interp.png --regions 16x15
```

Exit status is `0` identical, `1` pixels differ, `2` unusable input.

## What this backend is

- Cycle backend (`cycle_accurate = true`): each CPU cycle of each ROM
  instruction is spelled out in C, and the rest of the console is a
  cycle-stepped model. The older function-level backend is not used here.
- Mapper 3 CNROM is supported: fixed 32 KiB PRG and the 8 KiB CHR bank latch.
  Gradius uses no cartridge IRQ, expansion audio or battery RAM. NESRecomp's
  mapper coverage is broader (original IDs plus many banked boards), but only
  CNROM is exercised by this setup.
- The generated host is headless; it needs no window, launcher, ROM picker or
  controller. Input comes from the schedule file, so a run is repeatable.
- `--interp-only` executes the same discovered code through NESRecomp's
  interpreter. Equal hashes between the two runs check the recompiler's
  control-flow and bank folding, not two independent CPU implementations.

## Limitations

- **Interpreter fallback.** Code that static discovery did not reach, and RAM
  code with no compiled view, runs on NESRecomp's interpreter. This is not a
  failure, but it means a hash match does not prove full native coverage. In
  the 3,600-frame check here, 46.2% of cycles ran native and 53.8% fell back
  (`interpreted: ROM ... (53.8%, add to cycle_seed_file)`). Use `--miss-log`
  and `[game] cycle_seed_file` (or `--capture-log`) to compile those paths,
  rebuild, and confirm the percentage rises. The host prints the split.
- **Framework PPU/APU, not hardware.** The PPU and APU are NESRecomp's own C
  models (`hw_ppu.c`, `hw_apu.c`), not recompiled game code. The headless build
  reported `ppu: implementation=LLE` (the default); a build-time HLE raster
  kernel exists (`NESRECOMP_PPU_HLE=1`) but was not used here. Picture and audio
  comparisons are therefore against NESRecomp's timing/palette/audio model, not
  against a console or this repo's emulator.
- **ROM needed, and identity-checked.** The host refuses a different or modified
  image; generated code is specific to the pinned build and ROM hash.
- **Beta and platform skew.** NESRecomp labels itself beta; Windows is the
  primary platform and Linux is less tested. This setup exercises only the
  headless Linux path.
- **Licence.** NESRecomp is PolyForm Noncommercial 1.0.0. The checkout is
  external and is not vendored. Use it for noncommercial research/testing only;
  do not copy its code or generated output into this repository.
- **Scope of evidence.** One 3,600-frame route from boot into the stage-1
  opening matches exactly here. That does not establish the whole game, and
  other routes/timings need their own comparison.

## Keeping the repository clean

`setup.sh` writes only below `NESRECOMP_BUILD` (normally `/tmp/opencode/...`).
No ROM, generated `game_cyc*.c`, screenshot, hash log or binary belongs in
Git. The repository's `.gitignore` already rejects `*.nes`; the tools do not
create files under the worktree. Record commands, commit hashes and results in
this document instead of committing artifacts.

## Verified run

On 2026-11-09, using the commands above with the pinned commit and ROM:

- `setup.sh` completed and built `$NESRECOMP_BUILD/game/nes_game`.
- 3,600 frames with the route above: native 10.50 s (~343 fps), interpreter
  10.49 s; both wrote 3,600 hash lines and `cmp` reported them identical.
- `compare.py native.png interp.png` reported `identical: 256x240, 61440
  pixels`.
- Host coverage line: `native_cycles=49556664 (46.2%)`, interpreted ROM
  53.8%.
