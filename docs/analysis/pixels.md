# Pixel parity by state: the last 0.0006%

Evidence for the framebuffer comparison in `crates/gradius-diff`, and the
breakdown of the remaining pixel differences between `gradius-nes` (oracle)
and `gradius-core` (port). Both render the same 256x240 RGBA frame; a pixel
matches when the two RGBA values are equal. No cartridge bytes are reproduced.

Run over 700 frames with the default script (Start at 180 and 240), the port
matches **99.9994% overall**: 241 of 43,008,000 pixels differ (playfield
99.9994%, HUD 100.0000%). The CLI prints `match 100.0%` in its one-line
summary; that is one-decimal rounding, and only frame 430 (99.9000%, 38 px)
drops even to 99.9%. The residual is confined to Playing: a moving sprite
overlap on frames 391-460 (225 px) and a 4 px shot column at `x=0, y=39-42` on
frames 655, 666, 677 and 688 (16 px). Every differing position is opaque on
both sides - the difference is palette attribution, not presence/absence. The
title ship, the stage-entry blank window and the enemy-row OAM subset are
exact.

## Reproduce

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-docsfix4
ROM=/path/to/Gradius\ \(E\).nes

# full run, per-frame table plus the per-state summary
cargo run -p gradius-diff --release -- --rom "$ROM" --pack assets/gradius.pak \
  --frames 700 --pixel-report --json /tmp/pixels.json

# one state only (inclusive window; needs --pixel-report)
cargo run -p gradius-diff --release -- --rom "$ROM" --pack assets/gradius.pak \
  --frames 700 --pixel-report --pixel-window 291:699

# worst frames as PNG pairs
cargo run -p gradius-diff --release -- --rom "$ROM" --pack assets/gradius.pak \
  --frames 700 --pixel-report --png-dir /tmp/pixels --png-every 1
```

`--pixel-window FROM:TO` filters the samples in the table, the state summary
and the JSON. `--pixel-report` also prints a `pixel states` block: frames,
diff/match/play/hud rates, the union box and the three worst frames with their
boxes, grouped by the oracle's own state label (`Other(1)` Attract,
`Other(3)` Continue, `Other(4)` StageInit, `Playing`). Samples are taken after
the step, so sample frame *N* is the dump line's frame *N+1*. The table below
uses the exact `differing/total` counts from the JSON; the CLI rounds rates to
one decimal.

| state | pixel frames | samples | diff | match | play | hud |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Title | 0-131 | 132 | 0 | 100.0000% | 100.0000% | 100.0000% |
| Attract | 132-180 | 49 | 0 | 100.0000% | 100.0000% | 100.0000% |
| Continue | 181-261 | 81 | 0 | 100.0000% | 100.0000% | 100.0000% |
| StageInit | 262-290 | 29 | 0 | 100.0000% | 100.0000% | 100.0000% |
| Playing | 291-699 | 409 | 241 | 99.9990% | 99.9989% | 100.0000% |
| All | 0-699 | 700 | 241 | 99.9994% | 99.9994% | 100.0000% |

Playing holds all 241 differing pixels. State names are the oracle's:
`Other(4)` covers both stage init and the one-frame init phase. The state
compare skips 291 frames outside its Playing window; in this run none are
skipped because the two sides disagree on the state (0 state mismatches).

## Title 0-131: roll-in reproduced, 100%

All 132 title samples are pixel-identical, including the palette. The oracle is
black for frames 0-19; from frame 20 the title slides in at 2 px/frame from a
264 px offset, and the overlap is exact. The port applies the same offset in
`render()` (`offset = (266 - 2*frame)`), so the old 13.9% title bucket is gone;
there is no missing art, no font difference and no colour difference.

## Attract/Continue 132-261: exact

All 49 Attract samples (132-180) and all 81 Continue samples (181-261) are
pixel-identical. The port's Vic Viper left fin no longer draws a port-only
colour at `(72,141)-(73,141)`; the oracle's black is reproduced. The menu
cursor and the selected-row blink match on this scripted run (Start at 180
enters the countdown where the oracle blinks). The no-input attract path's
later blink phase is measured in `xcompare.md` and is not part of this
scripted run.

## StageInit 262-290: blank window reproduced

From frame 262 the oracle clears PPUMASK and keeps the screen black through
frame 295, and the port follows. All 29 StageInit samples and the Playing
samples 291-295 are pixel-identical (0 differing). The state labels also agree:
this run reports 0 state mismatches, so the previous 263-290 label difference
is gone. Frame 296, the old one-frame unmask residue (1,751 px), is now exact
as well; the HUD is exact on every Playing frame including 296.

## Playing 291-699: a palette-only tail

409 playing samples: 385 are pixel-perfect (94.1%), 24 differ. The longest
perfect run is 194 frames (461-654); the run from 291 to 390 is another 100.

- **391 (1 px, `206,50`)**: the first difference, one pixel where the oracle
  uses shot blue `(100,176,255)` and the port uses `(255,254,255)`.
- **391-460 (225 px)**: a moving overlap in `x 158-207, y 44-56`. Both sides
  draw the same opaque positions; the oracle uses the shot blue
  `(100,176,255)` where the port uses player/enemy palette entries such as
  `(0,64,77)` and `(254,129,112)`. Frame 430 is the worst sample at 38 px
  (99.9000%); 396 and 413 each carry 26 px, and every other frame in this
  cluster is 1-17 px. No frame in the cluster is below 99.9% at one decimal.
- **655, 666, 677, 688 (4 px each, 16 px)**: at `x=0, y=39-42` the oracle is
  black and the port draws shot blue `(100,176,255)`. This is the only
  difference after frame 460.

Every one of the 241 differing positions has alpha 255 on both sides, and the
counts of opaque oracle pixels and opaque port pixels in the diff mask are
both 241: the old sprite-subset presence/absence symptom is gone. What remains
is palette/priority selection on overlapping sprites, plus the left-edge shot.

## What is already correct

- Palette mapping: each remaining difference is a palette entry on the same
  opaque pixel, never a colour remap of the whole screen; the title/logo
  colours and the HUD colours are byte-identical.
- Text and nametable extraction: the roll-in, the settled title glyphs,
  `PLAY SELECT`, the player rows and the copyright block match exactly, and the
  HUD matches on every frame.
- Stage-entry mask: the whole blank window 262-295 is reproduced, including the
  transition frame 262 and the unmask at 296.
- Sprite scanline selection: the old enemy-row subset difference (the
  eight-sprites-per-scanline OAM order) is fixed; no frame shows port-only or
  oracle-only sprite pixels.
- Playfield gameplay: 99.9989% over 409 frames, with 385 pixel-perfect frames
  and the longest perfect run 194 frames (461-654).
