# Stages 1-7: the stage table and the `Stage` wiring plan

How the original selects a stage, which data the port already has, and the
order to turn the stage-1-shaped runtime into a table-driven one. Stage 1
gameplay is covered by `stage1_flow.md`, `stage1_data.md`, `stage1.md`,
`enemy_ai.md`; stage 2 by `stage2.md`; the stage 3-7 screens by the header of
`crates/gradius-core/src/stage_bgs.rs`.

## The stage variable

**`$19` is the zero-based stage number** (0 = stage 1, 6 = stage 7). It is not a
within-stage column index. The "section" wording in the stage-1 documents and
in `stage1_tables`/`stage1_collision` is historical: the `$A7CA` rows and the
`SECTIONS`/`terrain` rows are **stages**.

| where | instruction | meaning |
| --- | --- | --- |
| `$9B5C` | `LDX $18` | select the current player |
| `$9B68` | `LDA $26,X` / `STA $19` | stage init copies the per-player stage |
| `$98F7` | `CMP $98F0,Y` | stage end: `$3F` reached the per-stage scroll end |
| `$992E` | `INC $19` | boss-clear branch of the stage end |
| `$96C4` | `INC $19` | the normal end-of-stage branch (state `$90`) |
| `$9865` | `STA $26,X` / `INC $28,X` | stage 7 ending: stage back to 0, loop counter `$28` |
| `$A2CB` | `LDA $19` / `ASL A` / `LDA $A7CA,Y` | spawn root for the stage |

Measured: with the boss script run
(`scripts/boss-script.txt`, `--keep-alive --assist`) `$19` goes 00 -> 01 exactly
once, at frame 9601, on the frame after `$1B` 86 -> 90. `$1B` never enters
`$8E` in that run, so the normal advance is `$96C4` (state `$90`), not
`$992E`. The `$39`-set branch (`$8E`, `$9842`: 500 points, palette mode 1,
scroll to `$3F = $11`) has not been observed yet; see Open items.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stageplan
cargo xtask stages "$ROM"
cargo xtask sections "$ROM"
cargo xtask disasm "$ROM" --from 98F0 --to 9930 --labels
cargo xtask ramdump "$ROM" --watch 8900:9800 --press 180:start \
  --assist --keep-alive --range 0019:003B
```

The scripted variant (the boss script fed as `--press` entries with `+`
replaced by `,`) reproduces the boss kill at 9087 and the advance at 9600.

## What is already data (verified)

Every stage's spawn streams, terrain placements and scrolling background are
already decoded, not just stage 1's:

* `stage1_tables::SECTIONS` - seven rows (one per stage) of eight column
  streams. Rows 2-6 have an empty column 7 (the threshold fires before
  `$3F & $0E` reaches `$0E`); stages 1 and 2 use all eight.
* `stage1_collision::terrain(index)` / `section(index)` (index 0-6) - the
  `$A65C` placements and `$9A30` threshold per stage.
* `stage_collision::map(section)` - the packed `$0500`/`$0600` collision map
  per stage; sections 0 and 6 keep `stage1_collision` and `stage7_collision`,
  sections 1-5 are their own extracted maps. See "Collision maps per stage".
* `stage1_tables::{SECTION_THRESHOLD, STAGE_END, CHECKPOINT_TIMER,
  SHIP_Y_OFFSET}` - `$9A30`, `$98F0`, `$9A28`, `$9BC6` per stage.
* Background `Screen`s for all seven stages: `stage1_bg::STAGE1`,
  `stage2_bg::STAGE2`, and `stage_bgs::STAGE3`..`STAGE7`. The 3-7 module was
  validated against the oracle's live nametable (0 tile / 0 attribute
  mismatches per stage) and pins per-stage hashes; its header lists the
  measured `(world column, CHR bank, palette RAM)` changes.
* `gradius-audio` now carries all six second themes (`Music::Stage2` ..
  `Music::Stage7`), in addition to `$93`/`$96`/`$A5`/`$AC`/`$AF`.
* `stage::Spawner`, `stage::section_done`, `stage::transition_frames`,
  `stage::ship_start` already take the stage index.

Verified 2026-11-09 by regenerating both data modules from the ROM and diffing:

```sh
python3 crates/gradius-core/tools/gen_stage1_tables.py > /tmp/tables.rs
# normalized diff vs crates/gradius-core/src/stage1_tables.rs: 784/784 record
# tokens identical (all seven rows, all eight columns)
cargo xtask sections "$ROM" > /tmp/terrain.rs
# normalized diff vs stage1_collision.rs: 425/425 Terrain entries identical
# (the 3 extras in the source are test expectations)
```

So stages 3-7 are **not missing data**: their streams, terrain and screens are
present as `SECTIONS[2..6]`, `terrain(2..6)` and `stage_bgs::STAGE3..STAGE7`,
and they match the ROM. What is missing is the runtime half: background
selection, music switching, per-stage script and the end path. Collision is
wired per stage: `$C39D` reads the packed `$0500`/`$0600` map
(`stage_collision::map(section)`), so ship-vs-map terrain damage is always on
and each stage tests its own background (the stage-1 map is zero until frame
1579, so its opening starfield cannot kill).

`stage2_tables` duplicates `SECTIONS[1]`/`terrain(1)` (its tests assert the
equality); `stage2_bg` is the odd one out of the background modules. The plan
below folds the screens into the stage table and drops the duplicate tables.

## Collision maps per stage

Every stage has its own packed collision map. The cartridge keeps 2-bit
terrain fields at `$0500`/`$0600` as a sliding two-page window that reuses a
page for every other 32-column block; `$C39D` tests it with the row masks
`$C409` = `03 0C 30 C0`, so world column `c` and four-row group `g` live at
byte `c * 8 + g`, one 2-bit field per tile row. `terrain_solid` calls
`stage_collision::map(self.section)`; `terrain_solid_in` and the packed layout
are unchanged.

| section (`$19`) | stage | module | nonzero bytes | 32-column blocks with terrain |
| ---: | ---: | --- | ---: | --- |
| 0 | 1 | `stage1_collision::COLLISION_MAP` | - | 448-column reconstruction, see `stage1.md` |
| 1 | 2 | `stage_collision::STAGE2_COLLISION_MAP` | 520 | 4-10 |
| 2 | 3 | `stage_collision::STAGE3_COLLISION_MAP` | 563 | 4-11 |
| 3 | 4 | `stage_collision::STAGE4_COLLISION_MAP` | 650 | 4-13 |
| 4 | 5 | `stage_collision::STAGE5_COLLISION_MAP` | 196 | 5, 7, 9, 11 only |
| 5 | 6 | `stage_collision::STAGE6_COLLISION_MAP` | 1044 | 4-11 |
| 6 | 7 | `stage7_collision::COLLISION_MAP` | - | see the module header |

The counts are taken from the stored maps; they are not a correctness result.
Block 0-3 is the empty opening starfield in the extracted stages.

**Extraction.** `$26` was poked until the requested stage played, the run was
kept alive so it reached the stage end, and the page of the displayed
nametable was read once per frame while the stage scrolled. The window is
recycled every other 32-column block, so an entry can be observed more than
once; where repeated observations disagree at that edge the first value is
kept, the same rule used for the stage-7 map. Section 0 and 6 are not
re-extracted: they keep the stage-1 and stage-7 modules, so those two paths
are unchanged.

**Why the stages cannot share a map.** Each stage paints its own background,
so a stage-1 lookup in another stage reads terrain where the ROM has none, or
misses terrain where it has some. Stage 7 was the first stage that showed it:
it already had its own module, and the first difference is world column 64.
A stage-7 shot that must die in the terrain (oracle frame 876,
`$0363 = 227`, scroll `$3F:$3E = 1:$22`, column 65) ran to the edge while the
stages still used the stage-1 table, which slipped the ship's fire cooldown
and falsified the score: the `score` field matched 2715/5754 frames and
became 5754/5754 (100%), and the report-only stage-7 fixture fell from
**69,769** to **37,917** divergences. The ledger row is in
[KNOWN_DEVIATIONS.md](../KNOWN_DEVIATIONS.md).

**Tests.** `stage_collision` asserts per stage that the map is 3584 bytes,
non-empty, not byte-equal to the stage-1 map, and that `map(section)` returns
it; sections 0 and 6 still return the stage-1 and stage-7 modules. A second
test finds one discriminating spot per section 1-5 (solid in that stage's map,
open in stage 1) and checks that it consumes a shot in its own stage while the
same shot survives in stage 1.

**Pending verification (not accepted).** A verification card is re-extracting
and comparing against the oracle, so the stages 2-6 maps are not a gate result
yet:

* **Stage 5 is sparse.** 196 nonzero bytes, and every one of them is in an
  odd-numbered 32-column block (5, 7, 9, 11). That may be real (stage 5's
  terrain is the volcanic floor plus the `$14` structures, and it has
  `terrain(4) = 0` placements), or it may be a truncated extraction; the
  alternating-block pattern is exactly what a recycling bug would look like.
* **Tail residue.** The last blocks may still carry values from the recycled
  window instead of the final screen, and entries observed twice took the
  first value. Nothing checks those columns against the ROM yet.

Until the verification lands, only stage 7's map has a measured gate effect;
the stages 2-6 maps must not be quoted as verified.

## Runtime verification: stages 3-7

`xtask screens --sample-scroll` compares the stored wide map with the oracle's
live nametable at chosen scrolls, and reports the live palette and CHR bank at
each one. Re-run each stage on its own (the flag needs a single `--stage`):

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stagebgverify
for stage in 3 4 5 6 7; do
  cargo xtask screens "$ROM" --stage "$stage" --frames 20000 --assist \
    --press 180:start --sample-scroll 0 --sample-scroll 512 \
    --sample-scroll 1100 --sample-scroll 3328 --sample-scroll 3500
done
```

Measured 2026-11-09 on `Gradius (E).nes` (branch `agent/stagebgverify`); the
tail and near-end requests were stage-specific (below). Counts are tile /
attribute mismatches between the extracted map and the live nametable.

| stage | map | 0 px | 512 px | 1100 px | tail | near end | 100-px sweep |
| --- | --- | ---: | ---: | ---: | --- | --- | --- |
| 3 | 480x28, 3584 px | 0/0 | 0/0 | 3/0 | 3328: 0/0 | 3500: 0/0 | 23 of 38 samples nonzero, tile-only, max 32 |
| 4 | 480x28, 3584 px | 0/0 | 0/0 | 0/0 | 3328: 0/0 | 3500: 0/0 | all 0 |
| 5 | 448x28, 3328 px | 0/0 | 0/0 | 0/0 | 3072: 0/0 | 3300: 0/0 | all 0 |
| 6 | 416x28, 3072 px | 0/0 | 0/0 | 0/0 | 2000: 0/0 | 3000: 0/0 | 2800: 73/0; all others 0 |
| 7 | 416x28, 3072 px | 0/0 | 0/0 | 0/0 | 2000: 0/0 | 3000: 0/0 | 2800: 24/4; all others 0 |

The 100-px sweep covers 0, 100, ... up to each stage's last scroll, plus the
last scroll itself (38 samples for stages 3-4, 36 for stage 5, 32 for stages
6-7). No attribute mismatch was found outside the single stage-7 transient, and
no sample reported an unexpected palette or CHR bank. Attributes, palette and
bank otherwise followed the dominant or documented tail state below.

### Tail behaviour

* Stage 3 and stage 4 switch at world column 416 (3328 px) from the dominant
  CHR bank (1 and 0) to bank 2 and the empty-tail palette
  `0F 12 30 0F 0F 27 30 0F 0F 0F ... 0F 06 26 30`. Both samples 3328 and
  3500 confirm bank 2 and that palette.
* Stage 5 switches at world column 384 (3072 px) from bank 1 to bank 2 with the
  same empty-tail palette; samples 3072 and 3300 confirm it.
* Stage 6 keeps bank 2 and stage 7 bank 3 through their 3072-px last scroll;
  their tails are the end-area palette changes already listed in the module
  header (stages 6/7 also replay a checkpoint at the boss).
* The last 64 tile columns share the empty-tail art in stages 3-5 (tail tile
  hash `52409443334DB4C9`, attribute hash `1BFA42F1830FBD05`). Stage 6
  (`42AB179D707835DB` / `6B87449079331FC0`) and stage 7
  (`D04F33E639D27029` / `BC3CA980F3AE3F47`) have their own end-area tails.
  These tail-window hashes and every stage's full-map hashes are pinned by the
  tests in `stage_bgs.rs`.

### Caveats

These are dynamic-object writes, not RLE or wide-map folding errors; none of
the extracted hashes changed and no data was re-extracted.

* **Stage 3.** The `$C900` background turrets and the eruption effects write
  nametable cells while they are on screen. The wide map is a display capture,
  so it stores whichever turret state (closed art, open art, erased cells) was
  visible at extraction time. A later sample over the same world columns sees a
  different turret state, giving tile-only mismatches across 900-3000 px
  (3 at 1100 px, at most 32). Attributes are unaffected.
* **Stage 6 at 2800 px.** The first sample catches the end-area wall one frame
  before the wide map has painted it (73 tile mismatches); 2900, 3000 and 3072
  are clean, so the stored map does contain the wall.
* **Stage 7 at 2800 px.** The reverse: dynamic tiles (`C2`/`C4`, and `22` in
  the attributes) survive in the wide map from an earlier pass while the live
  nametable has cleared them (24 tile / 4 attribute mismatches); later samples
  are clean.

## Definitive stage -> data table

`background` is the module that renders the stage today. The screens store the
dominant palette/CHR bank; the tail switch (stage 2/3/4 at world column 416,
stage 5 at 384) is listed in each module header and is not modelled by
`Screen`.

| stage | `$19` | spawn row | cols | terrain | background | music `$8344` | `$2D`/bnd `$834B`/`$8354` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 0 | `SECTIONS[0]` | 8 | `terrain(0)` = 63 | `stage1_bg::STAGE1` | `$96` | 0 / 4 |
| 2 | 1 | `SECTIONS[1]` | 8 | `terrain(1)` = 101 | `stage2_bg::STAGE2` | `$59` | 0 / 4 |
| 3 | 2 | `SECTIONS[2]` | 7 | `terrain(2)` = 18 | `stage_bgs::STAGE3` | `$5B` | 2 / 4 |
| 4 | 3 | `SECTIONS[3]` | 7 | `terrain(3)` = 63 | `stage_bgs::STAGE4` | `$5D` | 0 / 4 |
| 5 | 4 | `SECTIONS[4]` | 7 | `terrain(4)` = 0 | `stage_bgs::STAGE5` | `$5F` | 2 / 4 |
| 6 | 5 | `SECTIONS[5]` | 7 | `terrain(5)` = 75 | `stage_bgs::STAGE6` | `$61` | 1 / 4 |
| 7 | 6 | `SECTIONS[6]` | 7 | `terrain(6)` = 89 | `stage_bgs::STAGE7` | `$63` | 3 / 2 |

Shared music everywhere: intro `$93`, boss `$A5`, clear `$AC`, game over `$AF`.
The shared clear cue `$AC` starts at the Big Core death (`$B97D`) for every
stage; the stage-clear path `$99B3` requests `$AC` again only at `$19 == 5`
(stage 6).

Raw roots, from `cargo xtask stages` (addresses are the decoder's output and
were re-run for this note):

| stage | spawn | terrain | `$2D` | bnd | end `$98F0` | boss `$9A30` | start `$9BC6` | bg height `$9FB6` | bg base `$9FC6` | bg tiles `$9FD6` | attr `$9FAE` | script `$C433` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `$A7D8` | 63 | 0 | 4 | `$0E` | `$0C` | `$00` | `$CF48` | `$CF90` | `$D772` | `$40` | `$C480` |
| 2 | `$A7E8` | 101 | 0 | 4 | `$0E` | `$0C` | `$05` | `$CF64` | `$D380` | `$D772` | `$41` | `$C540` |
| 3 | `$A7F8` | 18 | 2 | 4 | `$0E` | `$0C` | `$0A` | `$CF72` | `$D498` | `$D604` | `$40` | `$C680` |
| 4 | `$A806` | 63 | 0 | 4 | `$0E` | `$0C` | `$00` | `$CF56` | `$D188` | `$D772` | `$40` | `$C5A7` |
| 5 | `$A814` | 0 | 2 | 4 | `$0D` | `$0B` | `$00` | `$CF7E` | `$CF90` | `$D772` | `$40` | `$C64D` |
| 6 | `$A822` | 75 | 1 | 4 | `$0C` | `$0B` | `$0F` | `$DF80` | `$DDC0` | `$E003` | `$42` | `$C6D8` |
| 7 | `$A830` | 89 | 3 | 2 | `$0D` | `$0C` | `$14` | `$E701` | `$E541` | `$E774` | `$40` | `$C423` |

`bnd` is the coarse scroll at which `$835C` switches from the shared intro to
the second theme; stage 7 switches at 2, the rest at 4.

## Per-stage trigger, script and end

`$C433[$19]` is **not** a boss handler. `$A2BA` becomes
`LDA $3A / BEQ stream / JMP $C40D` once `$3A` is nonzero (the boss-clear
window). `$C40D` runs every 4th frame, finds a free slot (9 down to 0) and
calls `$C424`, which dispatches through `$C433[$19]` - the per-stage scripted
object spawner (the "stage hazards"). When `$3A != 0`, `$C424` jumps to `$C680`
(stage 3's handler) regardless of `$19`, which is a quirk to preserve.

| stage | script handler | what it does | boss / trigger | stage end |
| --- | --- | --- | --- | --- |
| 1 | `$C480` | object slot 0, script table `$C4F0`; plays `$0F` once then spawns kind `$0A` at Y `$90` | shared `$98` Big Core when `$3F` passes `$9A30[0] = $0C`; 7 hits or 1536-frame overheat | normal `$98F0 = $0E` -> `$1B = $90` -> `$96C4 INC $19` |
| 2 | `$C540` | object slot 2; every 8th frame spawns kind `$0B`, sprite `$67`, from table `$C567` | shared `$98`; `$39` is set only when `$04CC == 1 && $04AC < $78` (`$B95C`, inside the core death path) | normal; background tail switches to CHR bank 2 / palettes 2-3 black |
| 3 | `$C680` | spawns once `$68 >= $C67E[$3A]` and `$3F < $0E`: x `$F0`, kind `$C6C6[$3A]`, sprite `$C6C4[$3A]` | stream command at `$A460` spawns kind `$96` (slot 9, x `$F0`, y `$65`, state `$66`, `$67`); it is the only `$96` creator, and the shared `$98` also spawns | normal |
| 4 | `$C5A7` | object slot 4, same shape as `$C480`, script table `$C5FD` | shared `$98` | normal; background reuses `$D772` tiles with its own height/base |
| 5 | `$C64D` | every `$28` frames, `$C674` table -> `$65/$67`, then `$A4A0` (big object plus cannons) | shared `$98`; `$C76C` stage hook: with fewer than two options it calls `$CB84`/`$CB8B` (option trail), not a boss | normal with `$98F0 = $0D`, boss at `$9A30 = $0B`; most `$19 == 4` special cases |
| 6 | `$C6D8` | object slot 6, free-slot scan 9 down to 0, `$A9 << 4`, tables `$C6C4`/`$C6C6` indexed by `$3A` | shared `$98` | `$99B3/$99B7` for `$19 >= 5`: `$1B = $86`, and `$19 == 5` requests `$AC`; `$98F7` at `$19 == 5` calls `$CD9F` (calls `$CDAD` twice, reading `$CE2B[$66]`) before advancing |
| 7 | `$C423` | bare `RTS`: no scripted hazards | shared `$98` | `$98F7` at `$19 == 6` jumps to `$9865`: `$26,X = 0`, `$3F = 0`, `$28,X += 1`, `$1B += 1` (loop/ending) |

Stage-specific exceptions beyond the table:

* stage 4 (`$19 == 3`): no code branch of its own. Its "theme" is stage 1's
  background tile stream `$D772` with stage-4 height/base (`$CF56`/`$D188`)
  and music `$5D`, which is why it is the cheapest stage to wire.
* stage 5 (`$19 == 4`): 4-option chain `$9658` (needs `$5C >= 2`, runs
  `$A2BA`/`$CB8B`/`$ADA5`/`$BBB1`/`$9FF6`/`$C0C1` on even frames), plus
  `$8B92`, `$9F4D` (attribute skip), `$A178`, `$C031`, `$C259`, `$C2A5`,
  `$C2F4`; the missile ground probe is skipped and the `$AF44` turret uses
  sprite `$63`.
* stage 6 (`$19 == 5`): `$AF48`, and the `$99B7` `>= 5` clear path above.
* any: `$C42E` script dispatch, `$C40D` every 4th frame, `$9F49` attribute
  split, `$AFF8`, `$BBB7`, `$BBD8`, `$BC42`.

## The plan: a `Stage` abstraction in `game.rs`

### Shape

One data record per stage, in `stage.rs` (data only, no behaviour), plus a
lookup. Do not use function pointers: the game already branches on `$19`
inline and that matches the original.

```rust
pub struct Stage {
    pub index: u8,                        // $19
    pub background: Option<&'static crate::stage1_bg::Screen>, // STAGE1..STAGE7
    pub collision: Option<&'static [u8]>, // port-side map, per stage (stage_collision::map)
    pub music: u8,                        // $8344
    pub music_boundary: u8,               // $8354
    pub palette_mode: u8,                 // $834B
    pub boss_threshold: u8,               // $9A30
    pub end_threshold: u8,                // $98F0
}
pub static STAGES: [Stage; 7] = [ ... ];
pub fn stage(index: u8) -> &'static Stage;
```

`spawner`, `section_done`, `transition_frames` and `ship_start` already read
the per-stage tables and only need to move under `stage()` for a single source
of truth.

### What changes per stage

| aspect | per-stage | source |
| --- | --- | --- |
| spawn streams | yes, already wired | `SECTIONS[stage]`, `Spawner::new(stage, ...)` |
| terrain placements | yes, already wired | `terrain(stage)`, per-stage `$A65C` records |
| collision map | yes, wired per stage | `stage_collision::map(stage)`; sections 1-5 are their own extracted maps (2026-11-11), 0 and 6 keep their modules. See "Collision maps per stage" |
| background | yes, data present, not wired | the seven `Screen`s; `Screen` holds the dominant tail state |
| palette | background comes with `Screen`; sprite palettes are shared | `Screen.palette`, `$834B` mode |
| music | yes, already wired | the runner picks the theme from `stage_music()` (`$8344`), boundary `$8354`, shared `$93/$A5/$AC`; all 12 cues, stage 2 `$59` verified at the handoff |
| boss trigger | yes, currently hardcoded `$0C` | `$9A30` (`tables::SECTION_THRESHOLD`) |
| stage end | yes, currently unused | `$98F0` (`tables::STAGE_END`), plus `$99B3/$9865` |
| script `$C433` | yes, entirely missing | tables `$C4F0/$C567/$C5FD/$C674/$C67E/$C6C4/$C6C8` |
| start position | yes, already wired | `ship_start(stage, scroll)` |

### What stays shared

Stage flow `$9643/$9A40/$9822`; the spawner and `$A65C`/`$A5FC` tables; the AI
pre-pass and `$AE16` dispatch; drift; the background renderer (`draw_screen`
already takes a `Screen`); HUD; player, options, power meter; the Big Core
`$98` and its `$99` covers. No new subsystem is needed for stages 3-7; they are
branches and data.

### Order to wire it

Each step is independently testable and leaves stage 1 bit-identical.

1. **Name and table.** Rename `Game::section` to `stage` (keep a
   `section()`/`stage()` accessor) and add `stage::Stage` + `STAGES`, filled
   from the roots table. Assert every field against `xtask stages`; the seven
   screens keep their pinned hashes. No behaviour change.
2. **Boss trigger and end threshold.** Replace the hardcoded `0x0C`/`0x0D`
   (boss wait) with `STAGES[stage].boss_threshold`, and make the end path
   compare `$3F` with `STAGES[stage].end_threshold` (use `tables::STAGE_END`).
   Test: stage 5 boss at `$0B`, stage 7 end at `$0D`, and a scripted run that
   reaches stage 2 with `$19` +1 once.
3. **Background selection.** `render` reads `STAGES[stage].background` instead
   of `stage1_bg::STAGE1`. The screens already exist; fold
   `stage2_bg::STAGE2` into `stage_bgs` (or keep it) and drop
   `stage2_tables`. Add the per-stage tail switch (bank 2 + black palettes
   2-3 at column 416, stage 5 at 384) because `Screen` stores the dominant
   state only. Test: per-stage render hash from the oracle screenshot, and the
   tail columns for stages 2-5.
4. **Collision per stage.** `terrain_solid` takes the stage map (or derives
   from the `Screen`). Ship-vs-map terrain damage is ROM parity and always on
   (`$C29F`/`$C39D`); `Mods::map_terrain_collision` no longer exists. Test: the
   stage-1 map is unchanged; a stage-2/5 lookup uses its own map. **Done:** the
   maps are in `stage_collision`, with the shape and shot tests above.
5. **Music.** Extend `MusicCue`/`MusicDirector` (gradius-app) with the stage
   themes that now exist (`Music::StageN`), and expose `Game::stage()` plus
   the stage's `music`/boundary so the director can pick intro -> second theme
   at `$8354[$19]` -> boss -> clear. Test: the cue table per stage, including
   stage 6's `$AC` at the stage end only.
6. **Stage script `$C433`.** Extract the seven tables, then implement `$C40D`
   (every 4th frame, free-slot scan) and the handlers in `game.rs`, including
   the `$3A != 0 -> $C680` quirk. This is the only new mechanics code. Test:
   each handler's first object and frame, stage 7 emits nothing.
7. **End paths.** `$99B3` (`$19 >= 5`, `$AC` only at `$19 == 5`), `$CD9F` at
   `$19 == 5`, `$9865` at `$19 == 6` (reset `$26`, `$28 += 1`, continue into a
   new loop) instead of clamping at stage 6. Test: stage 6 -> 7 -> 1 with the
   loop counter, and no seventh-stage clamp.
8. **`$19` exceptions.** Stage 5's list above, stage 6 `$AF48`, the shared
   `$AFF8/$BBB7/$BBD8/$BC42` sites, and stage 2's `$B95C` nuance. Keep each as
   a small branch with its own test; do not refactor the shared routines.

### Open items before steps 6-7

* The `$39`-set stage-end branch (`$992E` -> `$8E` -> `$90`) appears to
  increment `$19` twice; no assisted run has set `$39` yet. Measure it once
  (poke `$39`, or a stage-2 run that hits `$B972`) before porting that branch.
* `$CD9F`'s effect (two `$CDAD` calls reading `$CE2B[$66]`) is not decoded; it
  needs one disassembly pass.
* `Screen` stores only the dominant CHR bank/palette; the measured tail
  switches are in the generated module headers and need explicit handling in
  the renderer, including stage 5's `$834B` mode 2 and `$9F4D` attribute skip.
* `CHILD_MOTION` (`$B017`) is indexed by player/stage and only has five
  entries; confirm the index is `$19` before relying on it for stages 6-7.
