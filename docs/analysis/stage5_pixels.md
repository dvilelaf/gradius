# Stage-5 pixels: the poked 297-8573 gate window and the column-384 tail

Measured 2026-11-12 on `main` at `6020bca`, against the cartridge
`Gradius (E).nes` (SHA-256
`268dc4b5b344e06e96281eb3159c1f61d3011c4f21dd1a2b4508aa934aff6143`) and the
pack built by `xtask extract`. The window is the poked stage-5 run the state
gate uses (`scripts/release-check.sh`, stage 5): `--stage 5 --frames 8573
--from 297 --to 8573 --keep-alive --native-keep-alive`, the harness's default
input script, per-frame pixel report. The state is exact over the whole window
(0 divergences over 8277 compared frames; 23481/23481 object slots), so every
pixel below is renderer residue, not a simulation divergence. The port's first
state divergence is one frame later, **8574** `obj.x` slot 9 (oracle 109,
native 111; the `$AED7`/`scroll_locked` item in `stage5_boot.md` §9), outside
this window.

## Result

| window | samples | differing / total px | match | play | hud |
| --- | ---: | ---: | ---: | ---: | ---: |
| 297-8573 (per-frame, keep-alive) | 8276 | 5,284,974 / 508,477,440 | **99.0%** | 98.8% | 100.0% |

Unrounded, that is 98.96% overall, 98.80% of the playfield and 99.999% of the
HUD. The pixel sample is labelled with the harness frame (0-based), so the
gate's `--pixel-window 297:8573` compares samples 297-8572 (8276 samples); the
state window covers oracle frames 297-8573. The HUD's 763 differing pixels in
32 frames are 0.001% of the band, so "100%" hides a small score-lag residue.

The residue per 1000-frame bucket ("scroll px" is the scroll at the bucket's
last frame, from the oracle dump):

| frames | samples | differing | match | play | hud px | scroll px |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 297-1296 | 1000 | 112,542 | 99.82% | 99.79% | 364 | 500 |
| 1297-2296 | 1000 | 184,409 | 99.70% | 99.65% | 330 | 1,000 |
| 2297-3296 | 1000 | 497,290 | 99.19% | 99.07% | 0 | 1,390 |
| 3297-4296 | 1000 | 998,127 | 98.38% | 98.13% | 69 | 1,640 |
| 4297-5296 | 1000 | 851,928 | 98.61% | 98.40% | 0 | 1,918 |
| 5297-6296 | 1000 | 847,180 | 98.62% | 98.41% | 0 | 2,180 |
| 6297-7296 | 1000 | 816,488 | 98.67% | 98.47% | 0 | 2,458 |
| 7297-8296 | 1000 | 818,148 | 98.67% | 98.46% | 0 | 2,720 |
| 8297-8573 | 276 | 158,862 | 99.06% | 98.92% | 0 | 2,816 |

The rate is not monotonic in scroll: it is worst in the 3297-4296 bucket
(98.13% play) and recovers to about 98.5% while the scroll keeps rising to
2,816 px. Not every frame differs — 422 samples are pixel-identical, all of
them at or before frame 2601; from 2602 on every sample differs. Per-frame
differing pixels run 0 (minimum), 717 (median) to 1,771 (worst).

Worst frames, with the scroll at that frame (from the oracle dump):

| frame | match | differing | play | hud | bbox | scroll px |
| ---: | ---: | ---: | ---: | ---: | --- | ---: |
| 7583 | 97.10% | 1,771 | 1,771 | 0 | 0,0-255,199 | 2,530 |
| 5695 | 97.20% | 1,750 | 1,750 | 0 | 0,0-255,199 | 2,018 |
| 4143 | 97.30% | 1,657 | 1,657 | 0 | 0,0-255,199 | 1,602 |
| 7584 | 97.30% | 1,642 | 1,642 | 0 | 0,19-242,140 | 2,530 |
| 3087 | 97.40% | 1,610 | 1,610 | 0 | 7,0-250,199 | 1,338 |
| 3099 | 97.40% | 1,568 | 1,568 | 0 | 4,0-247,199 | 1,341 |

## The residue is sprite presentation, not a background band

The worst frames' bounding boxes span the whole field (0,0-255,199): starfield
dots scattered across it plus sprite-sized clusters, not one contiguous band.
Stage 5's background is black space throughout — its visible tile columns
sweep 0-383 over the window — and its CHR bank/palette (`chr_bank 1`) matches
the oracle; a wrong bank would light a tile-aligned band, not 8x8/8x16
clusters.

The previous note's persistent quiet-frame residue, the shield ring's
animation quarter, is gone: `$8B70-$8B8B` now takes the ring's phase from
`nmi_frame`, so frames 298-308 and 310-320 — which differed by 11-12 px at
`x 97-100` tracking the ship — are pixel-identical. Frame 297 still differs by
59 px in a single 8x16 tile at `95,96-102,111`, and the only other samples in
that stretch are frame 303 (1 px) and frame 309 (145 px at `70,90-95,102`).
What remains is sprite presentation: shot trails and starfield dots.

The HUD residue is 763 px in 32 frames (`426-428, 473, 632-633, 698-704,
1089, 1145, 1773-1777, 2114-2119, 2178-2180, 3559-3561`), all in the HUD band
(`y >= 208`): the `$0700` score queue landing a frame or two off, the same
`ScoreDelay` lag stage 4 documents. Lives stay at 3 through the window, so
there is no lives-digit residue.

## The tail is implemented, and explains none of it

`STAGES[3]` (stage 4) carries `quirks: QUIRK_TAIL` with
`tail_column: Some(416)`, and `STAGES[4]` (stage 5) the same with
`Some(384)`: stage 5's map is 448 tiles (3584 px) and its background changes
a column earlier. `game.rs` applies the cut in the background draw, the cell
overlay and `hardware_bank`: from the cut column on, tiles use CHR bank 2 and
palette entries 8-15 are forced to `0F`. Verified with `xtask screens`
(`--sample-scroll`): stage 5 at 3072, 3328 and 3500 px gives 0 tile / 0
attribute mismatches with bank 2 and the tail palette, while 3000 px still
reports bank 1; stage 4 at 3328 and 3500 px matches the same way.

That closes the gap, but it does not move the gate window. A tail pixel can
only appear once the tail enters the 256 px screen: for stage 5 that is
scroll > 3072 - 256 = 2816 px.

| window | max scroll | frames with a tail pixel visible |
| --- | ---: | ---: |
| stage 5, 297-8573 | 2,816.5 px (8,570-8,573) | 0 / 8,276 |
| stage 4, 297-1797 | 320 px | 0 / 1,501 |

Stage 5 scrolls at about 0.5 px/frame through the entry and at about
0.25 px/frame once the attached-cannon count reaches `$5C >= 2` (frame 2857):
602 px at frame 1500, 1,316 px at 3000 and 2,816.5 px at 8,570-8,573. The
column-384 cut starts at world 3,072 px, and the last screen column at the
final sample is tile column 383 (`floor(3071.5 / 8)`), so no tail tile is drawn
anywhere in the window. The map itself tops out at 3,328 px of scroll. Stage 4
scrolls slower still and resets on each respawn; its documented tail samples
at 3328/3500 px come from the assisted `xtask screens --sample-scroll` probe,
not from the 297-1797 replay.

So of stage 5's **1.0394%** residue (1.1991% of the playfield), the tail
explains **0.0000 pp**; of stage 4's **0.2145%** (0.2333% of the playfield) it
explains **0.0000 pp**. The whole measured residue is presentation: sprite
groups/OAM (shot trails and the remaining sprite parts), the starfield dots,
and the score queue. The tail now matches but cannot move either gate.

## Stage-4 comparison

The same command shape on `--stage 4 --frames 1798 --from 297 --to 1797` (no
keep-alives, as the gate runs it) gives:

| window | samples | differing / total px | match | play | hud | first diff |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| stage 4, 297-1797 | 1501 | 197,832 / 92,221,440 | **99.785%** | 99.767% | 99.908% | frame 303, 7 px at 87,103 |

The classes match `docs/analysis/stage4_pixels.md`: playfield sprite overlap
(186,495 px), the HUD score lag plus the `lives = 0` digit (11,337 px in 373
frames), and the three `Other(4)` respawn samples. Both stages' first
divergence lands at scroll 1-4 px, roughly 3,300 px before the tail column; the
residues are unrelated in cause to the tail, which now matches at the cut on
both.

## Conclusion and routes

- **Tail share: 0%.** It is implemented (columns 416 and 384, verified at
  3072/3328/3500 px), but it sits outside the 297-8573 gate window — which
  ends at scroll 2,816.5 px, half a pixel before the cut — and should not be
  counted as the cause of the 1.04% residue.
- **What remains, all presentation:** `game.rs::build_sprites` /
  `apply_oam_ring` (shot trails and the remaining sprite parts), the
  starfield, and `hud.rs`'s score queue. State is exact, so none of this is a
  simulation bug.
- **Next probe:** the 297-8573 pixel gate stays report-only until the sprite
  classes have an owner.

## Reproduce

```sh
# the gate window, per frame
$DIFF --rom "$ROM" --pack assets/gradius.pak --frames 8573 --stage 5 \
  --from 297 --to 8573 --keep-alive --native-keep-alive --report \
  --pixel-report --pixel-window 297:8573 \
  --oracle-out /tmp/s5.oracle --json /tmp/s5.json

# stage 4, as the gate runs it
$DIFF --rom "$ROM" --pack assets/gradius.pak --stage 4 --frames 1798 \
  --from 297 --to 1797 --report --pixel-report --pixel-window 297:1797 \
  --json /tmp/s4.json
```

The report-only probe `crates/gradius-diff/tests/stage5_pixels.rs` samples
every 30th frame; it still runs the earlier 297-1500 window.

## The `$0600` cannon arms (`$8BDE`/`$8C0B`)

Measured 2026-11-18 against the same cartridge. The stage-5 `$14` large
objects carry up to four `$0600` records (`$CB8B` places six parts each), and
the port's `Game::cannons` already held their positions; the renderer did not
draw them. The oracle's OAM at frame 3000 (`xtask probe --frame 3000`, the
default script and `--poke <frame>:26:04` before the stage init) names the
writer.

`$8B15` builds `$0200` every NMI. After the `$0120` object slots it tests
`$19 == 4` at `$8B92`; only stage 5 takes the `$8BDE` branch:

* `$8BE2`: walks the `$0600` records from `+$90` down to `+$00` (draw order
  `$0690`, `$0660`, `$0630`, `$0600`).
* `$8C0B`: for each active record, reads the part-5 phase `$0615` once.
  `$AB = $8BF7[($0615 >> 2) & $0F]` is the tip tile and
  `$AC = $8C07[($0615 >> 4) & 3]` its attribute (palette 2, flips).
* `$8C28`: walks parts 0-5. X = `$0618,Y`, stored Y = `$0620,Y`; an X byte of
  zero or `>= $F4` is skipped without consuming an OAM slot.
* Parts 0-4 are tile `$F7` (8x16 pair `$F6/$F7`), attribute `$01` (palette 1),
  except part 2, the weak point, attribute `$02` (palette 2). Part 5 is the
  tip from `$8BF7/$8C07`; when its attribute bit 7 is set the stored Y is
  reduced by 8 (`$8C50`).

Measured OAM at frame 3000 (stored Y; the rendered top edge is +1), record
`$0690` first:

| record | X bytes | stored Y bytes | tip |
| --- | --- | --- | --- |
| `$0690` | `58 5e 66 6f 76 7d` | `1c 15 12 13 1a 20` | `$FB` attr `$C2` |
| `$0660` | `d8 d2 cb c2 bb b8` | `49 50 56 56 5c 64` | `$F9` attr `$82` |
| `$0630` | `d8 d2 ca c1 ba b8` | `25 1e 1b 1c 23 2c` | `$F9` attr `$82` |

The 18 parts are the last writes in the `$8B15` ring (slots 40, 25, 10, 59,
44, 29, 14, 63, 48, 33, 18, 3, 52, 37, 22, 7, 56, 41 for that order), after
every object slot. The pack carries the `$8BF7` tip tiles and `$8C07`
attributes in `TABL`, so no cartridge byte is a Rust literal.

Measuring the arm positions exposed a missing `$CCCA` rule in
`Game::update_cannons`: an odd record pins a phase `>= $84` to `$7C` when its
part is at or below the ship, and an even record pins a phase `< $3C` to `$44`
when its part is above it. Without it, `$0660`'s last two parts (odd record)
curl the wrong way and the arm does not coincide with the oracle. The rule is
now in `ai/turrets.rs`; the measured phases at frame 3000 are
`[00,56,5c,62,68,67]`, `[00,76,79,7f,79,73]` and `[00,4a,44,3e,38,32]`,
matching the oracle byte for byte.

The acceptance probe `--stage 5 --frames 3010 --from 2900 --to 3010
--keep-alive --native-keep-alive --pixel-report` goes from **99.4% to 99.5%**
overall (playfield **99.3% to 99.4%**, 110 samples); frame 3000 alone goes from
**430 to 156 differing pixels** (99.30% to 99.75%). The remaining 3000-frame
residue is not the arms: it is shot sprites at their own positions plus the
`$0120`-slot object animation.

**The shot origin (`$CBCB`).** The same measurement caught the arms' shots:
`$CBD8` reads `$061D`/`$0625`, the *tip* (part 5), not part 0, and skips the
shot unless the tip is on screen (`$10 <= x < $F0`, `y < $D0`); only then does
it subtract 8 on both axes and call `$BCAB`. `Game::update_cannons` fired from
`part_x[0]`/`part_y[0]` with no range test, so every shot appeared at the cell
instead of the tip. Fixed in `ai/turrets.rs`; the counter still resets on a
skipped shot, like `$0604` before the `JSR $CBCB`. That strays outside the
harness's state dump (no `$0136` field), which is why the state gate never saw
it.

**The `$0600` page, frame by frame.** Instrumenting the harness to compare
`$0600-$06BF` with `Game::cannons` over 2600-3010 (411 frames, 718 record
samples) gives **0 differences in `part_x` and `phase`** and 8 records on 8
frames where `part_y` is one lower: 2698-2699, 2796-2797 and 2894-2897. In
all of them the oracle's parent `$032C` byte is itself one higher than the
port's `Object::y.int`: the `$14` object's Y carries a constant ~5/256 px
sub-pixel bias (the state dump compares integers with a 1 px tolerance), and
the cannon reads the integer. It is an object-drift residue, not a cannon
rule; the placement tables, the `$CCF1` +/-6 clamp, the `$CCCA` pin (which
does fire in this window: 181 part samples land on `$7C`, 293 on `$44`) and
the part-0 `$CC1D`/`$CC25` offsets are byte-exact.

**The shot velocity (`$BCAF`).** `$CBAB`-`$CC10` hands the new shot to
`$BCAB`, which is the same `$BCAF`/`$BD59` aim helper `$BC3E` uses, so a
cannon shot flies at the shell speed: the major axis is 1 px/frame plus the
rank quarter (`$BD5F`'s `ADC` runs with the carry of the `CMP #$02`, so the
minor axis is `slope + slope/4 + 1` and the major fraction becomes `$40`/`$41`),
1.25 px/frame from rank 2. `Game::push_enemy_bullet` was still using the plain
`aimed_velocity` (1 px major, no scale), so every cannon shot flew at 0.8x the
original's speed: same heading, 1 px behind after one frame and 25% short of
the original's travel over a second. The port now routes both `$BC3E` and
`$CBCB` through `Game::fire_velocity`; `aimed_velocity` is gone.

**The `$0136` proxy slots.** `$CBCB` takes the *first free* `$0136` entry from
9 down, and the shot lives in object slot 22+i of the parallel arrays
(`$0336+i` Y, `$0376+i` X, `$03C6+i`/`$03F6+i`/`$0436+i`/`$0456+i` speeds).
`$CBFE`/`$CC0D` write only the integer bytes: the sub-pixel fractions at
`$0356+i`/`$0396+i` are whatever the previous occupant of that slot left
behind, so a shot starts up to a pixel off the cell it was fired from. The
port kept a compacted list and started every shot at fraction zero, which put
a 1 px Y (and now and then X) offset on nearly every cannon shot: 633 of the
671 frames from the first shot to 3300 differed. `EnemyBullet::slot`, the
highest-free allocation and the per-slot `bullet_slot_frac` now mirror
`$0136`-`$0196`, and the frame-by-frame comparison of tile, integer position
and both fraction bytes is **0 differences over 300-3300**.

**The move order (`$BDCF`).** The shot moves X, tests the X band (`$BE24`)
and only then moves Y and tests the Y band (`$BE5C`), so a shot that leaves
through the sides keeps its previous Y fraction for the next shot to inherit.
Moving both axes before testing left the last 9 one-frame residues.

**The chain's `$C0C1`.** `$9690` runs `$C0C1` on the `$19 == 4` alternate
chain frames; that entry point falls into `$C0FB`/`$C204`, the ship half of the
collision pass (ship vs objects, ship vs enemy shots, the `$C2F9` shot-vs-map
loop). Only `$BFDC`'s shot scan is skipped there. The port skipped the whole
pass on chain frames, so an enemy shot that reached the ship on a chain frame
survived one frame longer. `Game::collide_ship` splits the half out of
`Game::collide` and the chain calls it.

The `stage5_pixels` report probe pins the full 297-8573 summary. With the
shot-origin fix that window measured **99.6% overall, 99.5% playfield, 100.0%
HUD** over 8276 samples; with the aim and slot fixes it is **99.7% / 99.7% /
100.0%**, so the pinned `0.996/0.995/1.000` line has to move to
`0.997/0.997/1.000` (the diff crate owns it; was 99.0/98.8/100.0 before the
shot-origin fix).
