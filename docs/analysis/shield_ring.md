# The shield ring: the animation quarter, not priority or a wrong tile

Measured 2026-11-11 on `agent/ring` (from `main` at `aeba2c5`) against
`Gradius (E).nes` and the pack from `xtask extract`. The run is the stage-5
state-gate window (`--stage 5 --frames 1500 --from 297 --to 1500 --keep-alive
--native-keep-alive`), per-frame with `--png-dir` and `--png-every`
(`--png-every 30` for the sweep, `1` for the entry frames). State is exact over
the window (0 divergences, 7603/7603 object slots), so every pixel below is
renderer residue.

## What `$8B70` actually does

Disassembled from the cartridge (`$8B70-$8B8B`):

```
8B70  LDY $46      ; shield hit counter
8B72  BEQ $8B8E    ; no shield -> no ring
8B74  LDA $1B / AND #$70 / BNE $8B8E   ; death/transition phases skip it
8B7A  CPY #$01 / BNE $8B82 / LDA #$03 / STA $9E  ; one hit left -> $9E = 3
8B82  LDA $02 / LSR / LSR / AND #$03 / CLC / ADC #$5A
8B8B  JSR $8AB1    ; draw group $5A + (($02 >> 2) & 3)
```

One 8x16 sprite, groups 90-93 (`$8EE2`-`$8EF1`), `dx = 15`, `dy = -1`,
attr `$21` (palette 1, behind background), tile pairs `$B6/$B7`,
`$B8/$B9`, `$C0/$C1`, `$C2/$C3` at ship `(+15, -1)`.

The block above is drawn from inside `$8B15`'s object-slot loop (`$8B4C`), right
after slot 0's ship group, so `$8B6C LDX $9D / BNE $8B8E` means "only slot 0
draws the ring": `$9D` is the loop's slot index, not a hand-off flag. The
hand-off guard is `$1B & #$70` at `$8B74`; see the 19429 section below. The
earlier note here read the `$9D` slot test as a one-frame hand-off delay — that
was wrong.

The four tiles are two pairs: `$5A`/`$5C` share the ring arc and differ only in
where the 2x2 white sparkle sits (12 px); `$5B`/`$5D` are the same for the other
arc. `$5A` vs `$5B` differs by 100 px (a different arc). So:

| index bit | `$02` bit | selects |
| --- | --- | --- |
| index bit 0 (groups 90,92 vs 91,93) | 2 | the arc shape |
| index bit 1 (groups 90,91 vs 92,93) | 3 | the sparkle position |

Offsets measured from the ROM and the oracle: `$02 = frame - 5` (probe at
frame 300 reads `$02 = 39 = 295 mod 256`; the phase at frame 300 is `($02>>2)&3
= 1`, group `$5B`, matching the oracle's sparkle rows 2-3/12-13).

## Stage 5: a two-quarter phase error (fixed)

At every quiet entry frame the ring was present on **both** sides inside
`(px+15, py-1)` (45-57 coloured pixels of the 8x16 cell) and the arc shape
matched; only the white sparkle moved. That is the 12-px signature of the
`$5A <-> $5C` / `$5B <-> $5D` pair, i.e. the phase index differed by exactly
2 mod 4.

Frame 300: oracle sparkle rows 2-3 and 12-13 (group `$5B`); the old native drew
rows 4-5 and 10-11 (group `$5D`). `(300-5)>>2 & 3 = 1`; the old port's
`(self.frame - 5)>>2 & 3` with `self.frame = 300 - 296 = 4` gives
`(-1 mod 16)>>2 = 3`. `-296 mod 16 = 8`, so the index is +2.

Why: the harness creates the native game at oracle frame 297
(`frame_offset = 296`, so native `self.frame` is 296 behind the compared
frame). `$02` is a free-running power-on counter, so `frame - 5` is only the
ROM value when the game was booted from frame 0. `--stage 5` never does that.

This is the same anchor seed measured end to end on stage 7: the oracle reads
`$02(F) = (F - 5) & 0xFF` and the port read `(F - 44) & 0xFF`, a constant
**39** (7 mod 16) on all 5,754 compared frames, present from the first one
because `new_at_stage` skipped 295 increments of the `0xFC` power-on seed and
`295 ≡ 39 (mod 256)`. No `$8073` lag frame was missing. The fix landed on `main`
(`6e7fe5c`, core commit `fa67d3f`): `new_at_stage_seeded` seeds `nmi_frame` from
the cartridge at the anchor (`$02(A).wrapping_sub(1)`; `0x23` for the 297
anchor) and `build_sprites` now draws the ring from `(nmi_frame >> 2) & 3`, so
it reads the cartridge's quarter instead of `frame - 5`.

Measured after the fix: the 297-1500 per-frame pixel sweep goes from 99.7%
overall / 99.6% playfield / 100.0% HUD to **99.8% / 99.8% / 100.0%**, and the
first ring-related pixel divergence moves from frame 300 to 450. Stage 4 is
unchanged at 99.8% / 99.8% / 99.9%; stage 6 has no pixel probe.

Before the fix, over the 40 sampled frames of 297-1499: the ring-rect difference
was 12 px on 36 samples, 11 on 2 (the swapped pixel coincides with the black
background/star), and 15/20 on the 2 where an enemy overlaps the cell.
Extrapolated: 12 px x ~1203 frames = 14,400 px, **5.8%** of the window residue —
the whole ring share the stage-5 note attributed to it. That share is now gone.

## The breakdown the card asks for (pre-fix)

| class | frames | pixels | share of the stage-5 residue |
| --- | ---: | ---: | ---: |
| animation quarter (`$02` phase) | 1203 | ~14,400 | 5.8% |
| 8-per-scanline drop (priority) | 0 | 0 | 0 |
| wrong tile / attribute / palette | 0 | 0 | 0 |

Priority: the ring is never dropped on either side (both cells always draw
45-57 px; a drop would leave a 52-60 px hole and a 8x16 union box). Tile/attr:
the arc shape (phase bit 0) matches on every sample and both sides use palette
1, so the only difference is phase bit 1.

## Stages 2 and 3 are not the same symptom: they are the `$1B & #$70` guard

`stage2_boss_pixels.md` and `stage3_gaps.md` run from reset with the boss
script (no `--stage`), so `frame_offset = 0`, `self.frame` equals the compared
frame and the phase is exact: 19037-19428 is 0 px on 392 samples. The ring's
only residue there is the single 19429 frame (52 px). It is **not** an extra
frame, not a `$9D` condition and not a sprite-order problem: it is a
conditional the port never evaluates at all, the `$1B & #$70` skip at `$8B74`.

### The ROM rule

The `$8B70-$8B8B` block lives inside `$8B15`'s slot loop, after slot 0:

```
8B4C  LDX $9D / CPX #$20 / BCS $8B92   ; $9D = object slot
8B52  LDA $0120,X / BEQ $8B8E          ; inactive slot
8B66  LDA $0120,X / JSR $8AB1          ; draw the slot's group
8B6C  LDA $9D / BNE $8B8E              ; only slot 0 (the ship) draws a ring
8B70  LDY $46 / BEQ $8B8E              ; no shield
8B74  LDA $1B / AND #$70 / BNE $8B8E   ; <-- hand-off / death / game-over skip
8B82  LDA $02 ... ADC #$5A / JSR $8AB1 ; group $5A + (($02 >> 2) & 3)
8B8E  INC $9D / BNE $8B4C
```

`$1B` takes the `$70` bits only as `$90` (bit 4), `$A0` (bit 5, dying) and
`$C0` (bit 6, game over). `$90` is the hand-off frame: the `$86` clear handler
`$9842` sets it when `$3F` first reaches `$11` (`$9851-$9860`); on the next
frame the stage handler's `$9698` bit-4 test takes `$96C2`, which does
`INC $19` and sets `$1B = $80` (`$9C36/$9C3A`). So the ring is skipped in
exactly **one OAM build per `$86` clear** (`$39` core kill): the build that
runs while `$1B = $90`, the frame before `INC $19`. `$86`, `$8E`, `$80-$84`
all have `$70` clear and draw the ring.

### Measured, `xtask probe` with `scripts/boss-script.txt --keep-alive`

`$46 = 05` throughout. The OAM column is the build made at that frame's NMI
(from the previous frame's state); the PNG column is the build DMA'd one NMI
earlier, which is what the diff's row shows.

| frame | `$1B` | `$02` | `$19` | OAM ring slot at top 73 | ring in PNG |
| ---: | ---: | ---: | ---: | --- | --- |
| 19427 | `86` | `DE` | 1 | `x37 top73 tile C3 attr 21` ($5D) | yes |
| 19428 | `90` | `DF` | 1 | `x37 top73 tile C3 attr 21` ($5D) | yes |
| 19429 | `80` | `E0` | 2 | **none** (only stale `top245` ring tiles) | yes |
| 19430 | `80` | `E1` | 2 | `x37 top73 tile B7 attr 21` ($5A) | **no** |
| 19431 | `80` | `E2` | 2 | `x37 top73 tile B7 attr 21` ($5A) | yes |

The port has no `$1B` model in `build_sprites`, so at diff row 19429 it draws
`B(19428)` from state `S(19428)` — the `$1B = $90` state, `$02 = $DF`, which
its `frame - 5` phase maps to `$5D`/`$C3`... except that the build is taken one
frame later in the port's clock, so it draws `$5A`/`$B7` at `(37,73)` while the
oracle is black there. Row 19430 uses `S(19429)` (`$19 = 2`, `$1B = $80`) on
both sides, hence exact. The `$02` phase itself is right: `(0xE0 >> 2) & 3 = 0`
→ `$5A`/`$B7`, and `(0xDF >> 2) & 3 = 3` → `$5D`/`$C3`.

### It is not a one-off: every `$86` clear does this

The same guard produces the accepted 60 px residue at the stage-2 entry
(`stage2_pixels.md`: "9601 only", `37,98`): probe 9600 has `$1B = 90` and
`$19 = 0`; probe 9601 has `$19 = 1`, and its OAM (built from `S(9600)`) has no
ring at top 98; probe 9602 is the visible black frame, 9603 draws it again.
Any other stage hand-off that passes through the `$86`/`$90` frame will show
the same single-frame 8x16 box at ship `(+15,-1)`.

### Proposed core fix (not implemented)

Model `$1B` (or just a `ring_suppressed` flag set where the ROM sets `$1B`
with a `$70` bit) and skip the ring in `build_sprites` when
`$1B & #$70 != 0`. In today's state that is the frame whose clear handler
reaches `$3F = $11`: the last `$86` clear frame, `boss_clear == 1` just before
`advance_section`, plus the `Clear::*` hand-off frames if they map to `$90`
(stage-1 `$99C2` and stage-2 `$B998` clear both do). Keep the rest of the draw
as is: the phase, `dx = 15`/`dy = -1` and attr `$21` already match.

Acceptance test: run the natural boss-script window with `--pixel-report`
sampled per frame around 19428-19430 and assert row 19429's box is
`(37,73)-(44,88)` today and `None` after the fix, while 19430 stays exact; add
the 9601 row for the stage-1 clear. `crates/gradius-diff/tests/stage3_pixels.rs`
already runs the 19429-23000 window, so a narrower `shield_ring` probe with
`--from 19427 --to 19431 --png-every 1` is the cheap form (still ~19.4 k
oracle frames, so report-only/ignored without `GRADIUS_ROM`).

**So one rule does not cover both symptoms.** One rule covered stage 5 (the
`$02` phase, 5.8%, now fixed by the anchor seed); a second covers the 19429
frame (the `$1B & #$70` hand-off skip, 52 px once, and the 9601 twin). Priority
and tile/attribute explain none of it.

## The minimal core fix (landed)

Rule, exactly as `$8B82-$8B8B`: `group = 0x5A + (($02 >> 2) & 3)`, with `$02`
the cartridge counter on the compared frame.

`fa67d3f` implements it: `game.rs::build_sprites` draws the phase from
`self.nmi_frame`, and `new_at_stage_seeded` seeds that counter from the
cartridge at the anchor (the harness reads `$02` on the anchor frame and passes
it less one; the stage replays pass `0x23`). `new_at_stage` keeps the power-on
`0xFC` for a run booted at frame zero, so every other stage's `$02 & 1` gate is
unchanged.

`crates/gradius-diff/tests/shield_ring.rs` asserts this: frames 298-339 are
pixel-exact except 303 (one pixel), 309 (ship edge) and 325 (six pixels inside
the ring cell), which it pins to their exact boxes (it is skipped without
`GRADIUS_ROM`).

## Reproduce

```sh
ROM="$GRADIUS_ROM"
$DIFF --rom "$ROM" --pack assets/gradius.pak --stage 5 --frames 340 \
  --from 297 --to 340 --keep-alive --native-keep-alive --pixel-report \
  --png-every 1 --png-dir /tmp/s5ring \
  --oracle-out /tmp/s5ring.oracle --native-out /tmp/s5ring.native

$DIFF --rom "$ROM" --pack assets/gradius.pak --stage 5 --frames 1500 \
  --from 297 --to 1500 --keep-alive --native-keep-alive --pixel-report \
  --png-every 30 --png-dir /tmp/s5win --json /tmp/s5win.json

cargo xtask disasm "$ROM" --from 8B70 --to 8B90 --labels
cargo test -p gradius-diff --test shield_ring   # needs GRADIUS_ROM
```
