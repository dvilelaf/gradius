# Attract/demo pixel differences

Per-frame evidence for the remaining framebuffer differences between the
cartridge oracle (`gradius-nes`) and the native port (`gradius-core`) over the
attract/demo window. State parity is exact in this window: 0 divergences on
frames 130-850; every remaining difference is pixels only.

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-docsfix4
ROM='$GRADIUS_ROM'

# The card command: sampled table plus PNG pairs every 10 frames.
gradius-diff --rom "$ROM" --pack assets/gradius.pak --frames 900 \
  --pixel-window 130:850 --pixel-report \
  --png-dir /tmp/opencode/attractpx --png-every 10

# Dense pass used for the table and worst frames below. `--png-every` also
# thins the pixel table, so it is omitted here; the dense pass has 721 samples.
gradius-diff --rom "$ROM" --pack assets/gradius.pak --frames 900 \
  --pixel-window 130:850 --pixel-report --json /tmp/opencode/attractpx/report-frame.json
```

Samples are taken after `game.step`, so table frame *N* is the same frame the
state dump calls *N*.

## Per-state totals

| state | frames | samples | diff px | match | playfield | HUD |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Title | 130-131 | 2 | 0 | 100.0000% | 100.0000% | 100.0000% |
| Attract | 132-180 | 49 | 0 | 100.0000% | 100.0000% | 100.0000% |
| Continue | 181-261 | 81 | 0 | 100.0000% | 100.0000% | 100.0000% |
| StageInit | 262-290 | 29 | 0 | 100.0000% | 100.0000% | 100.0000% |
| Playing | 291-850 | 560 | 241 | 99.9993% | 99.9992% | 100.0000% |
| All | 130-850 | 721 | 241 | 99.9995% | 99.9994% | 100.0000% |

All 241 differing pixels are in Playing; 697 of the 721 samples are
pixel-perfect. Title, Attract, Continue and StageInit are exact.

## Worst frames

| frame | state | diff px | match | bbox | breakdown |
| ---: | --- | ---: | ---: | --- | --- |
| 430 | Playing | 38 | 99.9000% | `171,45-180,56` | moving sprite overlap, palette only |
| 396 | Playing | 26 | 100.0000% | `196,45-201,56` | moving sprite overlap, palette only |
| 413 | Playing | 26 | 100.0000% | `183,46-189,56` | moving sprite overlap, palette only |
| 410 | Playing | 17 | 100.0000% | `189,47-195,56` | moving sprite overlap, palette only |
| 391 | Playing | 1 | 100.0000% | `206,50-206,50` | first divergence |

## Classification and routing

### Remaining: 241 colour-only pixels

Every differing position is opaque on both sides; the counts of opaque oracle
pixels and opaque port pixels in the diff mask are both 241. This is palette
attribution, not presence/absence, a shifted sprite, or the eight-per-scanline
OAM subset.

- **Frames 391-460, 225 px, `x 158-207, y 44-56`.** A moving sprite overlap:
  the oracle uses shot blue `(100,176,255)` where the port uses player/enemy
  palette entries such as `(0,64,77)` and `(254,129,112)`. Frame 430 is the
  worst sample (38 px); the rest are 1-26 px.
- **Frames 655, 666, 677, 688, 4 px each, 16 px, `x=0, y=39-42`.** The oracle
  is black there and the port draws shot blue `(100,176,255)`.

- Route: `crates/gradius-core/src/game.rs`, sprite palette/priority selection.
  The sprite subset now matches, so compare palette attribution on overlapping
  sprites (frames 391-460) and the left-edge shot (655/666/677/688) rather than
  OAM list order.

### Fixed since the previous report (now 0 px)

- **Title/demo ship left fin (was 2 px, frames 135-261).** The port no longer
  draws `(254,129,112)` at `(72,141)-(73,141)`; Attract and Continue are
  pixel-identical.
- **Stage-entry unmask (was 1,751 px on frame 296).** The port now unmasks on
  the oracle's frame; 296 is exact and the HUD is 100% on every sample.
- **Enemy-row OAM subset (was 202 frames, 23,094 px).** No frame shows
  oracle-only or port-only sprite pixels; only the palette residue above
  remains.
- **Shot-spawn priority (was 7 px on frame 303).** Frame 303 is now exact.

## Ruled out in this window

- **Menu blink phase.** Title samples 130-131 and every Continue sample are
  exact; the 8-off/8-on selected-row blanking matches on this scripted run.
  The no-input path's later blink phase is a separate measurement in
  `xcompare.md`.
- **HUD bar / meter selection.** The HUD is identical on all 721 samples; there
  is no meter-highlight or bar-colour difference.
- **Score redraw.** No sample has a HUD digit difference; the delayed
  `ScoreDelay` output matches the oracle throughout Playing.

## Suggested core cards

1. `game.rs` sprite palette/priority: frames 391-460 (225 px, moving overlap)
   and the `x=0` shot on 655/666/677/688 (16 px). Both sides draw the same
   opaque positions; this is the last pixel residue.

## Harness note

`PixelSummary::states` used to rank the worst frames by the rounded
one-decimal match rate, so any state whose worst sample was below 0.05%
reported clean frames as "worst" (Attract's three worst were 100.0% frames
132-134). It now ranks by raw differing pixels; the test
`worst_frames_rank_by_differing_pixels_not_the_rounded_rate` locks that in.
