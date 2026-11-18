# Stage 1 art and palette analysis

Evidence for the measured constants in `crates/gradius-core/src/art.rs` and
`crates/gradius-core/src/palettes.rs`. Everything below comes from the oracle
(`crates/gradius-nes`) through `cargo xtask probe`; no cartridge bytes are
reproduced here.

## Cartridge

| field | value |
| --- | --- |
| file | `Gradius (E).nes` (read-only, never committed) |
| size | 65552 bytes (16-byte header + 32 KiB PRG + 32 KiB CHR) |
| SHA-256 | `268dc4b5b344e06e96281eb3159c1f61d3011c4f21dd1a2b4508aa934aff6143` |
| mapper | 3 (CNROM) |
| PRG | 32 KiB, one fixed `$8000-$FFFF` window |
| CHR | 32 KiB, four switchable 8 KiB banks |
| mirroring | vertical (two physical nametables) |
| battery / trainer | no / no |

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-assets

cargo xtask inspect "$ROM"
cargo xtask tiles "$ROM" tiles.png --scale 4

# title
cargo xtask probe "$ROM" --frame 120 --sprites 16 \
  --oam title-oam.bin --nametables title-nt.bin --json title.json --png title.png

# stage 1 (the snapshot used for the measured constants)
cargo xtask probe "$ROM" --frame 400 --press 180:start --press 240:start \
  --sprites 64 --oam stage1-oam.bin --json stage1.json --png stage1.png

# first live gameplay frame reachable by the oracle
cargo xtask probe "$ROM" --frame 800 --press 400:start \
  --sprites 64 --oam stage1-live-oam.bin --json stage1-live.json \
  --png stage1-live.png
```

`probe` prints PPUCTRL/PPUMASK, the selected CHR bank, the 32-byte palette RAM,
the first N OAM entries decoded with the **hardware** byte order
(`Y, tile, attribute, X`), the FNV-1a hashes below and the executed-PC span.
`--json` writes the same report; `--oam`, `--nametables`, `--ram` and `--png`
write the raw dumps for further tooling.

## PPU state and hashes

`probe` hashes the physical 2 KiB nametable memory (FNV-1a, the same 2 KiB that
vertical mirroring exposes at `$2000-$2FFF`) and the 256x240 RGBA framebuffer.

| frame | screen | PPUCTRL | PPUMASK | CHR bank | nametable FNV-1a | framebuffer FNV-1a |
| --- | --- | --- | --- | --- | --- | --- |
| 120 | title | `$A8` | `$1E` | 3 | `$02B2CA76947303BA` | `$DB38B05C900179B8` |
| 400 | stage 1 snapshot (start at 180 and 240) | `$A8` | `$1E` | 0 | `$5BB6DFFFB3CAE15A` | `$82D82AA40E40A99B` |
| 800 | first live gameplay frame (start at 400) | `$A8` | `$1E` | 0 | `$1ED2D4A974927001` | `$F93F3AEC9A97D006` |

`PPUCTRL = $A8` is NMI on, 8x16 sprites, sprite pattern table `$1000`,
nametable 0. `PPUMASK = $1E` enables background, sprites and both leftmost
8-pixel columns. Both screens therefore fetch sprites from the selected CHR
bank's `$1000` table.

## Palette RAM

The game uploads these exact 32 bytes. Values are NES master-palette indices.

### Stage 1, frame 400

The page is the cartridge's: `gradius-extract` reads all 32 bytes of
`$3F00..$3F1F` at run time and names them (`palettes::background[0..4]`,
`palettes::sprite[0..4]`).

| palette | use |
| --- | --- |
| bg 0 | starfield, HUD text (`30` white) |
| bg 1 | background details |
| bg 2 | background details |
| bg 3 | background details |
| spr 0 | Vic Viper body/nose; HUD power-meter sprite |
| spr 1 | Vic Viper rear; ring enemy |
| spr 2 | player shot / spark |
| spr 3 | HUD bar, explosion |

Entry 0 of every row is `0F`, black, and the measured sprite rows are identical
at frames 400 and 800.

### Title, frame 120

The title keeps sprite palettes 0 and 1 from gameplay but replaces backgrounds
0-3 (white/red/teal logo colours) and uses different sprite palettes 2 and 3.
The native port's stage constants use the stage-1 page.

## OAM findings

Byte order in OAM is `Y (stored top-1), tile, attributes, X`. `probe` decodes it
that way. Stored `Y` values `$EF..$FF` park a sprite off-screen; the game parks
unused slots at `$F4` (top 245).

### Player ship (frame 400)

| slot | X | stored Y | top | tile | attribute | meaning |
| --- | --- | --- | --- | --- | --- | --- |
| 2 | 72 | 96 | 97 | `$09` | `$21` | rear, 8x16 pair `$08/$09`, sprite palette 1 |
| 17 | 80 | 96 | 97 | `$0B` | `$20` | middle, 8x16 pair `$0A/$0B`, sprite palette 0 |
| 32 | 88 | 96 | 97 | `$0D` | `$20` | nose, 8x16 pair `$0C/$0D`, sprite palette 0 |

The Vic Viper is therefore **three 8x16 sprites** at x = 72/80/88, top y = 97
(stored Y = 96), i.e. 24x16 pixels. Sprite `0` is the HUD power-meter bar, not
the ship. At frame 800 the same three parts live in slots 34, 49 and 15 (the
game rebuilds the OAM buffer in a different order); the tile, attribute and
position values are unchanged.

### Stage 1 HUD

| slot | X | stored Y | tile | attribute | meaning |
| --- | --- | --- | --- | --- | --- |
| 0 | 248 | 206 | `$6D` | `$23` | power-meter bar, 8x16 pair `$6C/$6D`, sprite palette 3 |

The bar is two solid rows at the right edge of the HUD. HUD text is not a
sprite: it is background tiles, white `$30` from bg palette 0.

### Live frame 800 (enemy, shot, effects)

Frame 800 is the first live gameplay frame the oracle reaches. Visible slots
other than the ship parts:

| slot | X | stored Y | tile | attribute | interpretation |
| --- | --- | --- | --- | --- | --- |
| 4 | 95 | 95 | `$C1` | `$21` | ring enemy, 8x16 pair `$C0/$C1`, sprite palette 1 |
| 1, 16, 31 | 81 | 108 | `$65` | `$22` | 8x16 pair `$64/$65`, sprite palette 2 |
| 8, 23, 38, 53 | 72/80 | 96 | `$59` | `$23`/`$63` | 8x16 pair `$58/$59`, sprite palette 3 |
| 27, 42, 46, 57, 61 | 94/101 | 103 | `$6B` | `$22` | 8x16 pair `$6A/$6B`, sprite palette 2 |
| 19 | 68 | 88 | `$DF` | `$23` | 8x16 pair `$DE/$DF`, sprite palette 3 |

The enemy at (95, 95) overlaps the ship nose (x 72-96, y 97-112); the red
`$58/$59` sprites and `$6A/$6B` sparks sit on the ship at the same frame, which
is consistent with the first collision/explosion. The `$64/$65` sprite is the
smallest distinct projectile-like sprite in the frame and is the value used for
`art::BULLET`.

The oracle stalls on its sprite-0 split loop before a player shot can be fired
(see Limitations), so `BULLET` is taken from the game's own sprite-group table
(`$8D92`, entry 8/9 -> definition `$8F58`: one 8x16 sprite, tile byte `$65`,
attribute `$22`) and cross-checked against those frame-800 OAM slots.

## CHR bank usage and tile sheet

* Title screen: hardware CHR bank 3 selected, sprite table `$1000`.
* Stage 1: hardware CHR bank 0 selected, sprite table `$1000`.
* Background always uses pattern table `$0000`; sprites always `$1000`.
* The sprite tiles used by the ship (`$08-$0D`), shot (`$64/$65`), ring enemy
  (`$C0/$C1`) and the sprite-table animation frames are **identical in all four
  8 KiB CHR banks** (verified by decoding the same offsets in every bank), so
  the ship renders the same whichever bank the game has selected. That is why
  the constant bank below is stable even though the mapper can switch banks
  mid-frame.

The pack stores tiles per **4 KiB pattern-table half**, not per 8 KiB bank:
`cargo xtask tiles` writes eight 256-tile blocks. Half `h` is
`hardware_bank = h / 2`, `table = h % 2` (`0` -> `$0000`, `1` -> `$1000`).
`art::TileRef.bank` uses the same numbering, so all measured sprite constants
use **bank 1** (hardware bank 0, sprite table).

Tile-sheet interpretation (`tiles.png`, 16x16 tiles per half):

| half | hardware bank | table | contents |
| --- | --- | --- | --- |
| 0 | 0 | `$0000` | background, font, HUD digits |
| 1 | 0 | `$1000` | sprites: ship `$08-$0D`, shot `$64/$65`, enemy `$C0/$C1`, large enemy `$D0-$D5` |
| 2, 3 | 1 | `$0000`, `$1000` | second 8 KiB bank |
| 4, 5 | 2 | `$0000`, `$1000` | third 8 KiB bank |
| 6, 7 | 3 | `$0000`, `$1000` | fourth 8 KiB bank |

On half 1: ship parts are the first row's tiles `$08`-`$0D`; the shot is at
`$64/$65`; the ring enemy pair is `$C0/$C1`; the large enemy block is `$D0-$D5`.
`cargo xtask tiles` uses shades of grey for pixel values 0-3, so the actual
colors come from the palette dump above, not from the sheet.

## Notable PRG addresses

Identified while following the sprite/palette evidence; these are addresses,
not bytes.

| address | role |
| --- | --- |
| `$8050` | NMI handler (OAM DMA, per-frame sprite work) |
| `$8067` | main-loop self-jump (`JMP $8067`); all work happens in NMI |
| `$8335` | PPU setup helper (`$2006`, `$2001`, `$2000` writes) |
| `$8D92` | little-endian pointer table to sprite-group definitions |
| `$8ED9` | sprite-group definitions: count byte, then `(dy, tile, attr, dx)` quads |
| `$8EF6`, `$8F07`, `$8F18` | the three ship animation frames (tiles `$03/$05/$07`, `$09/$0B/$0D`, `$0F/$11/$13`) |
| `$8F29`, `$8F32`, `$8F3B` | explosion frames (`$59/$59`, `$5B/$5B`, `$6B`) |
| `$8F4A` | HUD power-meter bar (`$6D/$6D`) |
| `$8F58` | player-shot sprite (tile `$65`, attribute `$22`) |
| `$8EEC`-`$8EF2` | ring-enemy sprites (`$B7/$B9/$C1/$C3`) |
| `$9193` | large enemy group: six sprites, tiles `$D1/$D3/$D5` |
| `$9A98`, `$9A9D` | sprite-0 hit polling loop used for the HUD scroll split |
| `$ECFC` | sprite/OAM update helper called from NMI |

The measured constants, with provenance:

| constant | value | evidence |
| --- | --- | --- |
| `art::PLAYER` | bank 1, index `$0A` (pair `$0A/$0B`) | frame 400 OAM slots 2/17/32, full set `$08-$0D` |
| `art::BULLET` | bank 1, index `$64` (pair `$64/$65`) | frame 800 OAM slots 1/16/31; definition `$8F58` |
| `art::ENEMY_SMALL` | bank 1, index `$C0` (pair `$C0/$C1`) | frame 800 OAM slot 4 at (95, 95) |
| `art::ENEMY_LARGE` | bank 1, index `$D0` (pair `$D0/$D1`) | sprite group `$9193` (`$D1/$D3/$D5`) |
| `palettes::PLAYER` | stage sprite palette 0 | ship body/nose |
| `palettes::ENEMY` | stage sprite palette 1 | ring enemy, attr `$21` |
| `palettes::SHOT` | stage sprite palette 2 | shot, attr `$22` |
| `palettes::HUD` | `30` | stage bg palette 0, white text |
| `palettes::BACKGROUND` | the four stage bg rows | `$3F00..$3F0F` |

## Limitations of the oracle

* **OAM byte order in the PPU model is swapped.** `ppu.rs` reads the attribute
  from OAM byte 3 and X from byte 2, while the hardware (and `probe`) use byte 2
  for attributes and byte 3 for X. As a result screenshots put every sprite at
  `x = attribute` (typically 32-35 instead of the real 72-95) and use the X
  byte as the palette/flip flags. `probe`'s OAM table is hardware-correct; the
  framebuffer hashes reflect the current (swapped) render.
* **Live gameplay stalls at the sprite-0 split (unassisted runs).** The HUD
  scroll split in `$9A96/$9A9D` busy-waits on `$2002` bit 6. The
  scanline-at-once PPU model cannot produce the hit at the cycle the game
  expects, so the CPU spends most of each frame in that loop and the game state
  stops advancing after the first live frame. The title and the frame-400
  snapshot are unaffected, and frame 800 is the earliest live gameplay state
  without help. `xtask disasm --assist`, `xtask ramdump --assist` and
  `xtask trace --assist` inject the hit only while the CPU polls the split loop
  (see `docs/analysis/stage1_flow.md`); with the assist the game runs
  indefinitely and live state (player X/Y, bullets, spawner, score) can be
  measured. The framebuffer hashes below are from unassisted runs and stay valid
  for those runs.
* The frame-400 reproduction is a static screen. Live play stalls at the
  sprite-0 split independently of input (a single Start at frame 180 produces
  the same framebuffer hash), so this snapshot is not proof that the second
  Start pauses the game; it is the last presented frame before the stall. It
  still carries the correct stage-1 palettes, CHR bank, HUD and ship OAM, which
  is what the constants were measured from.

## Screens

`cargo xtask screens` turns the oracle's PPU state into the two `Screen`
constants in `crates/gradius-core/src/stage1_bg.rs` (title and the wide stage-1
background). It is the only generator for that file; the arrays are not edited
by hand.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-bg

# regenerate the shipped constants (about 20 s of emulation)
cargo xtask screens "$ROM" --frames 7800 --press 180:start --assist \
  --out crates/gradius-core/src/stage1_bg.rs

# optional artefacts
cargo xtask screens "$ROM" --frames 7800 --press 180:start --assist \
  --png screens.png --dump stage.grd
```

`--png` writes `screens.png` (title) and `screens-stage.png` (the whole wide
map, one pixel per nametable pixel); `--dump` writes `GRD1`, width, height,
row-major tiles and the attribute table as raw bytes. `--frames` defaults to
6000, which stops short of the scroll hold, so the shipped run uses
`--frames 7800` (it reaches the core spawn at 7723); `--assist` is required (the
sprite-0 HUD split otherwise stalls the CPU, see Limitations).

### Reconstruction

The oracle runs with `Emulator::set_capture_vram(true)`, so every `$2007` write
is logged with its VRAM address. The game draws the background one 4x4-tile
block at a time into the off-screen nametable before it scrolls in, so a
logged address alone is ambiguous (one physical slot stands for world column
`k` and `k + 64`). Instead the command samples what the PPU is actually
showing: at every frame the 32 tile columns of the current window are copied
out of the physical nametables using the 16-bit scroll `S = $3E + 256*$3F`
(`$3E/$3F` advance 0.5 px/frame). Under vertical mirroring world column `k`
lives in physical table `(k >> 5) & 1`, column `k & 31`; attribute bytes use the
same rule in units of four columns. The write log drives the statistics below.

* **Title**: one snapshot at frame 120, before Start. The whole 32x30
  nametable selected by PPUCTRL (table 0) plus its 64-byte attribute table,
  the 32-byte palette and CHR bank 3.
* **Stage 1**: the run starts at frame 180 (`--press 180:start`) and stops at
  frame 7800, past the scroll hold at 3328 px and the core entry at 7723.
  After every frame the visible window is folded into a wide map.
  The ship is kept alive by writing 5 to `$46` (the shield timer the game
  itself uses for temporary invulnerability) each frame; without it the ship
  dies at the first enemy and the stage never scrolls far enough. This changes
  no nametable content.
* The HUD split (`$9A96`-`$9AAF`) reloads scroll (0,0) after sprite 0 (the
  power-meter bar at y = 207), so scanlines 208-239 are the fixed HUD band and
  are not part of the scrolling map. The playfield window (screen rows 0-207)
  covers nametable rows 1-27, so the map stores 28 rows (0-27); the port draws
  the HUD itself.

### Measured result

| screen | tiles | pixels | palette | CHR bank | tiles FNV-1a | attribute FNV-1a | palette FNV-1a |
| --- | --- | --- | --- | --- | --- | --- | --- |
| title | 32x30 | 256x240 | title palette below | 3 | `9E836F0B444A5F1E` | `A6EDECBFF5D998D9` | `681756AB770FFE1B` |
| stage 1 | 448x28 | 3584x224 | stage palette below | 0 | `05740E863899C5AD` | `053ABF48AA5F3BF5` | `90FA63C90403F3FC` |

Counts for the stage run: 59852 tile writes, 7422 attribute writes, 64 palette
writes, scroll 0..3328 px, 0 checkpoint replays. The map is complete (no
missing tiles or attribute bytes). The stage stops scrolling at 3328 px and
`$5B` holds that latch through the core entry (7723), so the map carries the
window visible at the stop. The shipped module is the `--frames 7800` run.

* title palette `$3F00-$3F1F`: the title page above (logo backgrounds, sprite
  palettes 0-1 shared with play)
* stage palette `$3F00-$3F1F`: the stage-1 page above (identical at frame 400,
  512, 1100, 3328 and at the stop)

The stage palette is the only one measured during the whole stage (the only
palette change in the run is the title -> stage transition), so `Screen.palette`
needs no per-section split. The four background sub-palettes still change per
16x16 block; that is the stored attribute table, not palette RAM.

### Tail and held scroll

The stage is 448 columns: 0-415 are sampled while the scroll steps and 416-447
are the 32-column window visible at the 3328 px stop. From world column 416 the
map carries the same `$834B` mode-0 tail as stages 2 and 3
(`stage_catalog::QUIRK_TAIL`): CHR bank 2 and the empty-tail palette, i.e. the
stage page with background sub-palettes 2 and 3 black (`$0F`).

The PPU horizontal latch is the previous frame's `$3E/$3F` only on a frame that
steps the scroll. `$5B` holds the scroll at the stop until the core entry
(7723), and on those held frames the latch already equals the current
`$3E:$3F`, so a held frame is drawn without the one-pixel lag. The port models
this with `Game.scroll_lag` (`game.rs::render`): 1 on a step frame, 0 otherwise.

The late window 6000-7723 (up to the core spawn) is gated at
>= 99.9% overall and playfield in
`crates/gradius-diff/tests/stage1_late_pixels.rs`, and frame 6500 is pinned to
0 differing playfield pixels. Before the fix the same window measured 88.1%
match / 86.6% play: the old 384-column map wrapped to column 0 and frames
6000-7723 rendered as black starfield. The window's only HUD residue is 20 px
from the score row lagging one frame after a score change
(`shown_score`/`score_delay`).

### Terrain collision (`stage1_collision`)

The original **does** collide with the packed map. It keeps a 2-bit-per-tile
map at `$0500`/`$0600` and `$C39D` reads it with the row mask table at `$C409`
(`03 0C 30 C0`, one 2-bit field per tile row). `$C2AF` calls it every frame in
stage 1; a non-zero field jumps straight to `$C1D0` (death, no shield check).

The map is zero only at the start: the first writes land at frame **1579**
(`$0500 = $50`) when the volcano section loads, so the opening starfield cannot
kill. After that it is a populated packed map, and natural runs show that it
kills: hugging the floor dies at frame **2169**, hugging the ceiling at
**2745** (stage 2, with `$26 = 1` poked at 263: **2553** and **2617**). The
kill sets `$0100 = 2`, `$1B = $A0` and runs the normal life-loss path
(checkpoint respawn, lives 3 -> 2). The map reads, scroll positions and RAM
dumps are in
[`powerup_terrain_verification.md`](powerup_terrain_verification.md).

The port always tests the map (`collide_player_terrain`) against the map of
the current stage: `stage_collision::map($19)`, which returns
`stage1_collision::COLLISION_MAP` for `$19 = 0` and each other stage's own
extracted map (sections 0 and 6 keep the stage-1 and stage-7 modules).
`Mods::map_terrain_collision` no longer exists. The per-stage extraction, the
block layout and its open doubts are in
[stages.md](stages.md#collision-maps-per-stage). `stage1_bg::is_solid` is a
leftover geometric classification of
background tiles (measured ranges `$20-$27`, `$33-$36`, `$5A-$5F`, `$7A-$96`,
`$98-$C4`, `$D1-$D2`, `$D8-$D9`, `$DC-$DD`, `$F8-$FF`) and is not what the
cartridge collides with.

### Approximations and limits

* The collision map has 448 columns: 0-383 come from the reconstructed
  background, 384-447 were read from the live `$0500`/`$0600` pages during the
  boss-approach tail (the cartridge keeps loading collision columns through the
  core entry). This describes stage 1 only. The stages share the loop but not
  the map: sections 1-6 had stage 1's map until the per-stage extraction of
  2026-11-11, and now each reads its own (`stage_collision::map`), so the
  per-stage variation is not spawn placement only ([stages.md](stages.md#collision-maps-per-stage)).
* The stage map ends where the scroll stops (3328 px, world column 416) and
  includes the 32-column window visible at that stop, i.e. 448 columns. No
  columns are invented past the last frame the game shows.
* Rows 28-29 (HUD) are not stored. If the port draws a shorter HUD than the
  original, it must cover rows 28-29 itself; the ground art continues into the
  hidden rows in the original.
* The run is deterministic; different `--frames` values above ~7800 give the
  same map, but a value below the stage end would truncate it.

## Ghidra cross-check (static)

Ghidra 12.1.4 + GhidraNes (`tools/ghidra/README.md`) confirms the loader/mapper and adds a
static view. Corrections to earlier notes:

- `$8D91` is the **low byte** of the sprite-group pointer table; `$8D92` is the high byte.
  The OAM builder at `$8AB1` indexes it with an even Y (`$8F07`, `$8F18`, `$8EF6`, ...).
- `$A7CA/$A7CB` are the low/high pair of the stage-section table read by the spawner.
- `$8C7D-$EC17` is not pure data: it contains code reachable only through pointer tables
  (for example `$A2CB`). Static call-graph counts are therefore a lower bound.
- 47 direct functions / 2,489 instructions were recovered statically, versus 2,456 distinct
  executed instructions in a 3,000-frame trace.
