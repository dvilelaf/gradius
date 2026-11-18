# The endgame: stage 6's beam wall, stage 7's loop and the game-complete path

This note closes the two stages that have no Big Core. It records the stage-6
collision-beam hazard, the stage-7 ending/loop, the `$1B` state machine and the
per-player bytes that survive a loop. `bosses.md` covers the shared `$98` Big
Core and `stages.md` covers the stage table; this note is the end-of-game half.

Everything below was measured on `Gradius (E).nes` with

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-ending

# stage 6: beams, wall erase, transition to stage 7
cargo xtask record-ending "$ROM" --stage 6 --frames 8000 --out /tmp/ending6.txt

# stage 7: brain, ending, +10000, stage-1 restart
cargo xtask record-ending "$ROM" --stage 7 --frames 8500 --out /tmp/ending7.txt
```

`record-ending` pokes `$26` until the requested stage starts, holds fire and
keeps the player alive. One line per frame carries `$1B` (`phase`), `$19`
(`stage`), `$28` (`loop`), `$26` (`stagebyte`), `$39`/`$3A`, `$66`, the player
position and the live stage-6 beam boxes. Frame numbers include the title and
stage intro; the state sequence and addresses do not. The `$26` poke stops after
the first stage start, so the stage-7 loop is not poked back into stage 7.

## TL;DR

* **Stage 6 has no boss and no `$98` core.** When `$3F` reaches `$9A30[5] =
  $0B`, `$1B` runs `$81 -> $82`; for the whole `$82` window `$C40D` calls
  `$C6D8` every 4th frame. `$C6D8` fills the ten collision-only boxes with
  diagonal energy orbs. The window ends after `$9A28[$17] * 256` frames, then
  `$99B3` sets `$1B = $86`, scroll continues to `$98F0[5] = $0C`, and `$96C4`
  advances to stage 7.
* **While `$1B = $86` on stage 6, `$98F7` calls `$CD9F` every frame.** It erases
  two tiles per frame from the `$CE2B` table (88 entries) and increments `$66`
  by 2, opening a diamond-shaped hole in the organic stage-6 wall.
* **Stage 7 has no boss either.** The giant brain and its cables are part of the
  background tilemap, not a spawned object: no `$98` core is created and
  `$C423` is `RTS`. The stage ends on scroll at `$98F0[6] = $0D`.
* **`$9865` runs at `$19 == 6`.** It increments `$1B` to `$87`, sets `$26 = 0`
  and `$24 = 0`, clears `$3F`, increments the loop counter `$28`, and saves a
  new power-meter value in `$22`. The ending states then run `$87 ... $8D`
  (about 1230 frames), the score gains **+10000**, `CONGRATULATIONS!` is shown,
  and `$9B38` restarts **stage 1** with `$28 = 1`.
* `$19` does **not** get an `INC` at 7025. `$9865` writes `$26 = 0`, and the
  stage-reset handler `$9B38` later copies `$26` into `$19`. There are no
  credits: the text is displayed during `$8C`.

## Status on `main` (`f5275f6`)

The `$87`..`$8D` phase machine runs in `game.rs::ending_frame`.
`Game::new_at_ending` seeds the measured 7024 `$87` state, and
`ending_replay` compares oracle 7025-8290 as a gate: 1266 frames with 0
divergences on section, scroll, score, lives and `px`/`py`; the trace
also pins the phase boundaries (`$88`..`$8C`, `$8D` at 8254) and the +10000 at
7998. Its three tests pass ROM-free.

Implemented: the `$88`/`$89`/`$8A` setup, the 23-frame `$8B` build window, the
1202-frame `$8C` display, the `$CF35` 16-tile row revealed one character every
9 frames from 7538 and complete at 7682, the single +10000 / lives 3 -> 4
payment at 7998, `$8D` at 8254 and stage-1 play again at 8282.

Two gaps remain, both OPEN in the ledger:

1. **Ending screen.** The `$CF90` screen is extracted, but its `$9D88` column
   builder is not ported; the ending draws the port's procedural `starfield()`
   instead. The gate covers state and the text row, not a per-pixel screen
   match.
2. **Ending ship objects.** `$987F` (slot 8 tile `$9E`, slot 9 kind `$28`) and
   the `$BB09`/`$BB7C` motion are not ported. That is why `ending_replay` uses
   `compare_objects: false`: object slots 8/9 and the ship's sprite are
   outside the comparison, while the `$CE8E` text cadence they drive is
   modelled and gated.

The `$CF35` record now comes from the pack, not a constant: `Pack::into_assets`
fills `Assets.stage.ending_text_record` and `draw_ending_text` reads it through
`Assets::ending_text_record`; a pack that predates the chunk returns
`AssetError::Missing("ending_text_record")` and the row is not drawn, instead
of fabricated bytes. The embedded `ENDING_TEXT_RECORD` constant is gone. Of the
extracted stage tables, only the stage-6 wall erase is not wired yet:
`Assets::stage6_wall_erase` exposes `$CE2B`, but `Game::erase_wall` still walks
the embedded `stage6_wall::TABLE`. That rewiring is the one open pack item.

## `$1B`: the state machine

`$1B` is dispatched by two paths:

```
$96A4  LDA #$40; AND $1B; BNE $96EE      ; game over / continue
$96A0  LDA #$20; AND $1B; BNE $96E2      ; death
$969A  LDA #$10; AND $1B; BNE $96C2      ; $90 stage advance -> $96C4
$96AA  LDA $1B; BPL $96B1                ; positive 1..5 -> $96B8 table
$96AE  JMP $981D                         ; $80-$8E -> $9824 table
```

The `$9824` table (index = `$1B & $7F - 1`) and the measured frames:

| `$1B` | routine | meaning | stage 6 | stage 7 |
| --- | --- | --- | --- | --- |
| `$80` | — | normal play; `BPL` skips dispatch | 292 | 292 |
| `$81` | `$9A01` | `$4D = $9A28[$17]`, `$4C = 0`, `$1B = $82`; stage 7 uses `$4D = 1`, `$4C = $CA` | 5929 | 6563 |
| `$82` | `$99DC` | decrement `$4C:$4D`; at zero `INC $1B` | 5930-6954 | 6564-7022 |
| `$83` | `$99B3` | `INC $1B`; `$19 >= 5` -> `$86` (stage 6 plays `$AC`); else `$84` | 6954 | 7022 |
| `$84` | `$9975` | spawn the `$98` Big Core (stages 1-4) | skipped | skipped |
| `$85` | `$9971` | hold scroll while the core lives | skipped | skipped |
| `$86` | `$98F7` | stage end: `$19 == 6` -> `$9865`; `$19 == 5` -> `$CD9F`; wait `$3F >= $98F0[$19]`; else `$90` (or `$8E` if `$39 != 0`) | 6955-7467 | 7023 |
| `$8E` | `$9842` | `$39` quick-kill branch: wait `$3F >= $11`, award `$50` BCD, `$1B = $90` | not seen (forced with a `$39` poke) | — |
| `$90` | `$96C4` | `INC $19`, clear `$39/$3A/$3F`, init the next stage | 7467 | — |
| `$87` | `$9B38` | ending reset: `$19 = $26`, `$1A = $28`, zero-page/pages cleared, ship re-placed | — | 7024 |
| `$88` | `$9BE7` | PPU/attribute/music setup, `INC $1B` | — | 7025 |
| `$89` | `$9C0C` | PPU/attribute updates, `INC $1B` | — | 7027 |
| `$8A` | `$9C18` | palette/starfield update, `INC $1B` | — | 7028 |
| `$8B` | `$987F` | ending ship animation, `INC $1B` | — | 7029 |
| `$8C` | `$98D0` | long starfield / `CONGRATULATIONS!` display | — | 7052-8253 |
| `$8D` | `$98D8` | clear `$1B`, `JMP $9B38` | — | 8254 |
| 1..5 | `$96B8` | post-ending stage-start sequence (`$9B38`, `$9BE7`, `$9C0C`, `$9C18`, `$9C1E`) | — | 8255-8259 |

## Stage 6: the `$C6D8` beam wall

### Trigger and window

`$9A40` raises `$1B` to `$81` when `$3F >= $9A30[5] = $0B`; `$9A01` sets the
clear timer and `$82`. `$A2EA` sends `$1B == $82` to `$C40D`; `$C40D` runs on
every frame whose `$02 & 3 == 0`, finds a free collision slot (`$0136` 9 down
to 0) and calls `$C424`, which dispatches `$C433[5] = $C6D8`.

Measured stage-6 timeline (stage starts at frame 265):

| frame | event |
| ---: | --- |
| 5929 | `$1B = $81`, `$3F = $0B` |
| 5930 | `$1B = $82`, `$4D = $9A28[$17] = 4`, `$4C = 0`; `$66` cleared |
| 5933 | first beam: slot 9, `$C6D8` |
| 5933-6953 | one spawn attempt every 4th frame (256 attempts) |
| 6954 | timer reaches zero: `$1B = $83` |
| 6955 | `$99B3`: `$1B = $86`, `$AC` clear cue requested |
| 6956-6999 | `$CD9F` erases 2 tiles/frame, `$66` 0 -> 88 |
| 7467 | `$3F = $0C`, `$1B = $90` |
| 7468 | `$96C4`: `$19 = 6`, scroll `$3F = 0`, stage 7 |

The window length is `$9A28[$17]` 256-frame ticks. In this run `$9A28[$17] = 4`
(1024 frames); `bosses.md`'s 7569 end came from a run with a different rank /
clear timer. The state sequence is fixed.

### `$C6D8`: what is spawned

```
$C6D8  LDA #$04; LDX #$06; JSR $C449   ; pattern byte -> $A9, parity -> $AA
       LDX #$09                        ; find free $0136 slot, 9 -> 0
       BNE/RTS when all ten are busy
       X = free collision slot
       $03F6,X / $03C6,X = 8.8 Y velocity from the $C449 nibble and $02
       $0316,X = 1                     ; box type
       $0176,X = 1                     ; hit-box type
       $0476,X = 0 or 1                ; lane selector
       $0336,X = $C74A[Y]              ; Y = $84 (132) or $42 (66)
       $0376,X = $98                   ; X = 152
       $0136,X = $8D                   ; collision marker / sprite group
```

The boxes are **not** in the `$030C` object table, but they are **visible**:
the ten slots are object slots 22-31, whose `$0120` sprite-group byte *is*
`$0136-$013F`. `$8B15` therefore draws `$0136 = $8D` as group `$8D`, a single
`$BD` red ball at the box position (`$9454`: Y+8, X+8, attribute `$23`).
`$BF6F` tests player bullets against them and `$C222` tests the player against
them. `$BC13`/`$BDCF` moves the same slots; the `$C449` nibble comes from
`$C74C` and becomes the 8.8 Y velocity in `$03C6:$03F6` (the low byte carries
the spawn frame `$02`), while `$0436/$0456 = 4/0` make X exactly -4 px/frame.

Measured: initial spawn X is `$98 = 152`; the first end-of-frame sample is 148,
and every following frame is `x -= 4`. The `$02 & 4` bit selects the lane
(index 0 when set) and the initial Y:

* lane `$42` (66, index 1): `y` increases, by the pattern speed (0-4 px/frame).
* lane `$84` (132, index 0): `y` decreases, by the pattern speed.

The two lanes cross near `y = 96` (the recorded player row) and continue to the
opposite edge, so the hazard is a pair of crossing diagonal streams, not homing
missiles. At most 10 boxes are live; spawn attempts continue until the `$82`
window ends.

### Damage and clear

* **Player hit:** the player-vs-box test at `$C222` uses hit-box type 1
  (`$C1FC[1] = $16` = 22 px wide, `$C200[1] = $12` = 18 px high) against the
  ship's `(+6, +8)` probe; the X `CMP`'s carry shifts the Y band one pixel
  (`[box + 1, box + 18]`). With no shield the result is the normal lethal hit
  path (`$C1D0`): `$0100 = 2`, `$4C = 120`, `$1B = $A0`. With a shield, `$46`
  is decremented **and `$AEF2` clears the box**, so the same beam cannot be
  shot for ten points on the next frame.
* **Shot:** `$BF6F` accepts a player bullet when `shot_y - box_y < $10` and
  `shot_x - box_x - 1 < $A3` (the Y `CMP` carry), both unsigned. `$0316 = 1`
  is destructible: `$BFA2` clears `$0136/$0176/$0316`, `$8468` adds **+10**
  points and `$EC18` plays SFX `$09`. The shot scan keeps testing the remaining
  boxes after the shot is consumed. `$8D` is the hit type; `$0136 == $59` has
  its own path, but the beams use `$8D`.
* **Off-screen:** a box is removed by the ordinary expiry path when it leaves
  the playfield. In the run the score rises in +10 steps when a box is shot, and
  the last box is gone by frame 6989.
* **Window clear:** the beams stop being created when `$82` ends at 6954.
  Surviving boxes keep moving and can still hit the player until they leave or
  are shot.

### `$CD9F` / `$CDAD`: opening the wall

`$98F7` calls `$CD9F` on every frame with `$19 == 5` while `$1B = $86` (stage 6).
`$CD9F` refuses once `$66 >= $58`; otherwise it calls `$CDAD` twice:

```text
$CDAD  X = $66; if X >= $58 return
       $66 += 1
       tile = $CE2B[X]
       VRAM address = $2400 + (tile >> 4 << 5) + ($F0 | (tile & $0F)),
         with the carry into the high byte (addresses $24FA-$26DB)
       queue one $0700 record: [type 1, addr hi, addr lo, 00, FF]
$CD9F  A = $66; if A >= $58 return; call $CDAD twice
```

The `$CE2B` table is 88 encoded tile coordinates. They form a diamond-shaped
opening 16 rows high and 11 columns wide at the right side of the stage-6 wall:
rows 7-22 of the nametable, columns 21-31. `$CD9F` erases two tiles per frame,
so `$66` runs 0,2,4,...,88 over frames 6956-6999 and the wall is gone before
the scroll reaches `$98F0[5]`. This is the visible "cell wall opens" sequence in
the stage-6 screenshots; it is presentation only and has no collision.

The writes target **physical** table-1 cells (`$24FA-$26DB`), not "the column
under the current scroll pixel": the port keys them at
`(scroll/8) & !31 + column`, so the drifting `$86` scroll does not leave the
cell art behind. The wide background extraction must also keep the pre-erase
art (the port applies `$CD9F` dynamically); otherwise the erase itself bakes
the cell out of the map.

## Stage 7: brain, ending and loop

### Content and end trigger

Stage 7 has no `$98` Big Core and no scripted objects: `$C423` is a bare `RTS`.
The final giant brain, its cables and the organic walls are background tiles
(`stage_bgs::STAGE7` / the `$E0xx` background data), not spawned objects. The
object table never contains kind `$98` in a live slot in this run; `boss=-`
for the whole stage.

Because the normal `$9975` core spawn is skipped by `$99B3` (`$19 >= 5`), the
stage only has the `$81/$82/$83/$86` path and the scroll end:

| frame | event |
| ---: | --- |
| 6563 | `$1B = $81`, `$3F = $0C` (`$9A30[6]`) |
| 6564 | `$1B = $82`; stage 7 special timer `$4D = 1`, `$4C = $CA` = 458 frames |
| 7022 | timer zero: `$1B = $83` |
| 7023 | `$99B3`: `$1B = $86` (no `$AC` requested) |
| 7024 | `$3F` reaches `$98F0[6] = $0D`; `$98F7` jumps to `$9865` |
| 7025 | `$1B = $88`, `$19 = 0` (stage 1), ending sequence running |
| 7998 | `$8C`: path end, score 830 -> 10830 and lives 3 -> 4 |
| 8254 | `$1B = $8D`, no score change |
| 8255 | `$1B = 1`, stage-1 intro |
| 8282 | `$1B = $80`, stage-1 play resumes |

### `$9865`: the wrap to stage 1

```
$9865  INC $1B
       X = $18
       $2001 = 0
       $3F = 0
       $26,X = 0            ; stage 1
       $24,X = 0            ; scroll saved for the reset
       $22,X = ($42 != 0)   ; saved power-meter value (0 or 1)
       INC $28,X            ; loop counter
```

`$19` itself is untouched. At 7025 the positive dispatch state `$01` runs
`$9B38`, which copies `$26,X` (`0`) into `$19`, so the observable result is
`$19: 6 -> 0`. This is not an `INC $19`; `$9865` deliberately resets the
per-player stage byte.

### `$9B38` and the ending states

`$9B38` (`$87` and positive state 1) is the big reset:

* clears zero page `$3D-$97`, then pages `$0100`, `$0300`, `$0500`, `$0580`,
  `$0600`, `$0680`;
* restores `$35 = $14` (fire cooldown), `$42 = $22,X` (power meter),
  `$3F = $24,X` (scroll), `$19 = $26,X` (stage 0), `$1A = $28,X` (loop);
* re-places the ship from `$9BC6/$9BCE` for stage 0, clears `$0100`, sets
  `$0D = 6` and jumps to `$83B0`.

Score, lives and the loop counter are **not** cleared. The ending therefore
keeps the run's score and lives, adds 10000, and starts a harder loop 2 at
stage 1.

The ending sequence measured on the recorder:

```text
7024  $87  $9B38  reset: $26=0, $24=0, $3F=0, $28=1, $19 still 6
7025  $88  $9BE7  PPU/music setup, $19 already 0
7027  $89  $9C0C
7028  $8A  $9C18
7029  $8B  $987F  ending ship animation
7052  $8C  $98D0  starfield; "CONGRATULATIONS!" is on screen at frame 8100
7998  $8C  $CE8E  path end: +10000, lives 3 -> 4
8254  $8D  $98D8  $1B = 0, JMP $9B38 (no score add)
8255  $01  $9B38  stage-1 intro; $28=1
8282  $80  normal stage-1 play
```

`$8C` lasts 1202 frames (7052-8253) and is the only "ending screen"; there is
no staff/credits roll. A screenshot at 8100 shows the `CONGRATULATIONS!` text
over the starfield.

### `$992E`: the `$39` branch

`$992E` is the alternate stage advance inside `$98F7`:

```
$9928  A = $90
$992A  X = $39
$992C  BEQ $9938          ; normal: $1B = $90
$992E  INC $19
$9930  INC $3A
$9932  $3F = 0
$9936  A = $8E
$9938  $1B = A
```

It is not taken in the normal stage-6/7 runs (`$39 = 0`). Forcing `$39 = 1`
with `probe --poke 7400:39:01` before the stage-6 end shows the path:

| frame | `$19` | `$1B` | `$39` | `$3A` | `$3F` |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 7468 | 6 | `$8E` | 1 | 1 | 0 |
| 7600 | 6 | `$8E` | 1 | 1 | 2 |
| 9000 | 7 | `$80` | 0 | 0 | 1 |

So `$992E` advances `$19` immediately (5 -> 6), sets `$3A`, and enters `$8E`.
`$8E` (`$9842`) waits for `$3F >= $11`, awards the 500-point bonus and sets
`$1B = $90`; `$90` then runs `$96C4` and increments `$19` **again** (6 -> 7).
This is the documented open item: the `$39` route double-increments `$19` and
runs past the seven-stage table (`$19 = 7`). The normal route (`$39 = 0`) is the
one that is needed for the port.

## Run state across a loop

After `$9865` + `$9B38`:

| state | value |
| --- | --- |
| `$19` stage | 0 (stage 1) |
| `$26` per-player stage | 0 |
| `$24` per-player scroll | 0 |
| `$28` loop counter | incremented (1 after the first loop) |
| `$22` per-player power meter | 0 or 1 (from `$42 != 0`) |
| `$3F` scroll high | 0 |
| `$3A`/`$39` | 0 |
| `$66` erase index | 0 |
| score | unchanged, then +10000 during `$8C` at the path end (7998) |
| lives | unchanged |
| power-ups / options / speed | cleared by the `$9B38` page clears |
| `$1B` | `$01 -> $80` via the ending sequence |

The next loop is the same seven stages with `$28` (and `$1A`) incremented; the
difficulty rank `$17` is rebuilt from the live power state by `$9C3F`, so the
loop itself does not enter the rank table directly.

## The ending cards (7024-8254)

This section is the card plan that produced the port. It assumed stage 7
already reaches `$3F = $0D` (the stage-7 boot slice), and it refines the
ending half of `stage67_plan.md` (`S7-01`..`S7-03`) with the code anchors and
the ROM addresses found since. Frame numbers are the oracle's; a native run
starts five frames later (`STAGE_BOOT_LEAD`).

Landed on `main` (`f5275f6`): E-00's fixture and trace; E-01/E-02's `$9B38`
reset and `$87`..`$8A` states; E-09/E-10/E-11 (`$8C`, +10000, `$8D` and the
stage-1 restart); E-12 (`ending_replay`). E-03's extractor and the pack's
ending tables landed separately, and `b0a630e` carries them into
`Assets.stage`: the `$CF35` text record is consumed by `draw_ending_text`,
while the class tables and `$CE2B` are exposed but not yet read from the pack.
Still OPEN: E-04 (the `$CF90` column screen), E-05's screen build (the text
row's reveal is gated; the columns are not built), and E-06/E-07 (the `$987F`
ship objects and the `$BB09` motion).

### 1. The sequence, frame by frame

| frames | count | `$1B` | screen | state change / source |
| ---: | ---: | --- | --- | --- |
| 7024 | 1 | `$87` | final stage-7 playfield frame | `$9B38`: page clears, `$26=0`, `$24=0`, `$3F=0`, `$28++`, `$19=0`; score/lives kept |
| 7025-7026 | 2 | `$88` | black (PPUMASK 0) | `$9BE7`: SFX `$FC`, PPU/music queues, `$57=0`, `$5E=$3F`, `$1B++` |
| 7027 | 1 | `$89` | black | `$9C0C`: HUD nametable, `$1B++` |
| 7028 | 1 | `$8A` | black | `$9C18`: `$89E8` default palette, `$1B++` |
| 7029-7051 | 23 | `$8B` | black, screen being built behind the mask | `$987F`: four `$9D88` column writes/frame; when `$57 != 0`, initialise the two ending objects and `$1B++` |
| 7052-8253 | 1202 | `$8C` | starfield, Vic Viper (kind `$28`), `$9E` mothership, HUD; text revealed left to right | `$98D0`: `INC $5B` (no scroll), `$ADA5` object AI, `$9A7F`; `$CE8E` runs the ship; +10000 and lives 3->4 at 7998 |
| 8254 | 1 | `$8D` | last ending frame | `$98D8`: `INC $5B`, `$1B=0`, `JMP $9B38` |
| 8255-8259 | 5 | `$01`-`$04` | stage-1 boot | positive dispatch: `$9B38`, `$9BE7`, `$9C0C`, `$9C18`, `$9C1E` |
| 8260-8281 | 22 | 1..4 / `$80` hold | stage-1 intro | `$0D` blank then normal play |
| 8282 | - | `$80` | stage-1 play | `loop=1`, score 10830, lives 4 |

The `$8C` total is **946 + 256**: the ending-ship handler reaches its path end
and calls `$8444` (+10000, lives) at 7998, then `$4C` (cleared to 0 by
`$9B38`) is decremented once per frame from `$FF` and 256 frames later
`INC $1B` reaches `$8D` at 8254. Do not hardcode 1202; pin the path end and
the 256-frame tail separately.

What is deterministic: every phase duration above, the `$9B38` reset values,
the two object spawn positions, the score/lives jump, the final loop values.
What is timer-like but still frame-deterministic: the left-to-right reveal of
the ending screen. It is the NMI draining the `$0700` VRAM queue that `$9D88`
filled during `$8B`; the reveal rate is the queue drain rate, not a scripted
typewriter. It must be measured (`$0E`, `$0700`) and pinned, not guessed.

### 2. What to write, and where each datum comes from

The disassembly below was read from `Gradius (E).nes` in this worktree with
`xtask disasm`; it is evidence for the cards, not a substitute for a probe.

**Ending screen (`$8B`, visible from `$8C`).** `$987F` calls `$9C1E` while
`$57 == 0`; `$9C1E` calls `$9D88` four times per frame. `$9D88` is the
ending screen column builder, not the gameplay scroll path:

* `$9FB6[0] = $CF48`, a 28-byte column-index stream (the next pointer is
  `$CF64`);
* `$9FC6[0] = $CF90`, the tile-column table those indices select;
* `$9D49`/`$9D4A` select the nametable/attribute addresses for the two
  nametable halves; `$9D67`/`$9D69` hold the base addresses.
* `$9FB6[1..7]`/`$9FC6[1..7]` are the other stages' column data; the ending
  runs with `$19 = 0`, so only entry 0 is used.

Both tables are in `assets/gradius.pak` (`ENDING_SCREEN_STREAM` = `$CF48`,
`ENDING_SCREEN_CLASSES` = `$CF90`) and the `$9FD6`/`$9FE6` tile-class tables
are extracted too. What is missing is the runtime column builder: the port
still draws the ending with the procedural `starfield()` in `game.rs`, whose
RNG-star layout is a port invention. The oracle screen is static (nothing in
`$8C` rewrites it), so porting `$9D88` over the two packed tables is the
remaining screen card.

**Ending text.** The `CONGRATULATIONS!` glyphs are CHR tiles already in the
2048-tile pack. The tile layout is part of the `$CF48`/`$CF90` column data
above, so prefer that tilemap to `font::draw_text_centered` (the built-in 3x5
font will not match the oracle pixels). The reveal is now measured: one
character every 9 frames from 7538, all 16 tiles at 7673, and the full-line
rewrite (`$4F = $80`) at 7682. The port draws the revealed prefix from the
`$CF35` record and gates the row's timing.

**Sprites (`$987F`, frame 7029/7052).** The init writes raw object arrays:

| field | value | meaning |
| --- | --- | --- |
| `$0315` | `$28` | slot 9 kind (`$28` ending ship) |
| `$0335` | `$88` | slot 9 y = 0x88 |
| `$0375` | `$A4` | slot 9 x = 0xA4 |
| `$0334` | `$80` | slot 8 y = 0x80 |
| `$0374` | `$74` | slot 8 x = 0x74 |
| `$0134` | `$9E` | slot 8 sprite tile = `$9E` (`$012C+8`) |
| `$0120` | 0 | player sprite disabled |
| `$0100` | 3 | player state (not a new sprite) |

So the *moving* object is kind `$28` at `(0xA4,0x88)`; the static `$9E` sprite
sits at `(0x74,0x80)`. (This corrects the "`$28` ship at `(0x74,0x88)`"
phrasing in `stage67_boot.md`.) `$9E` and the `$96..$99` animation tiles are
in the pack, but the CHR bank during the ending must be probed: the sprite
half is `hardware_bank * 2 + (tile & 1)`.

**Kind `$28` -> tile (`$BB09`).** The table is extracted as
`ENDING_SHIP_MOTION` (26 two-byte pairs plus the `$FF` terminator); the
handler is code and is not ported:

* `$BB7C`: the first byte is a signed X delta added to `$0375`; the high
  nibble of the second is the Y subtraction; the low nibble gives the sprite
  tile as `$96 + low_nibble` (`$96..$9B`). The measured table walks the ship
  up-left after six `+1` frames.
* `$BB09` advances `$014C,9`; after a six-frame warm-up it consumes one
  `$BB7C` entry per frame; the `$FF` entry clears `$0135`, spawns the
  explosion and sets `$4E=$A0`.
* `$987F` only sets the initial position; the tile appears from `$BB09`.

**`$CE8E`/`$CF27`.** `$CE8E` writes bytes into the `$0700` VRAM queue and is
reached from `$BB09` when `$048C,9 != 0`; at its `$FE` terminator it sets
`$4F=$80`, and the next call sets `$4F=$FF` and calls `$8444` (+10000). The
record and cadence are decoded below ("The ending text and the `$8C`
reveal"): `$CF35` is a 19-byte VRAM-write record, one character every 9
frames. The port models that cadence and the +10000 frame directly; it does
not run `$BB09` to reach them.

**Palette and HUD.** `$8A` calls `$89E8`; `$9B38` has cleared `$40-$46`, so it
queues the default power-up-free palette (`0F/15/16/17/18/1B/1A`), identical
to a new stage. `$9C0C` calls `$88BB`/`$88FB`/`$8931`, which rewrite the lives
and score digits; the +10000 score add therefore updates the HUD without a
new renderer. Music is `$A5` for the whole ending (the `Boss` track), queued
by `$9BE7`; it is in the pack.

### 3. Cards

Each card is ~1 h unless noted. `game.rs` is the contention point: run the
cards serially except the extractor and the harness cards.

| card | scope | owned files | measurable acceptance | deps | h |
| --- | --- | --- | --- | --- | ---: |
| E-00 | oracle/evidence baseline: record 8300 frames, add `$0E/$57/$5B/$54-$58/$1F/$2A/$17/$048C[9]/$0475` and slots 8-9 to the trace, emit `ending-7015-8290.state`, probe PNGs at 7024/7030/7052/7100/7200/7300/7400/7500/7600/7800/7998/8100/8254/8282, record decoded-RGB sha256 for each | `xtask/src/ending.rs`, fixture, this note | trace reproduces 7024/7025/7027/7028/7029/7052/7998/8254/8282 exactly; 14 hashes committed; `$0E` drain curve saved | stage-7 boot slice | 1.5 |
| E-01 | `$9865` reset and `loop_count` (`$28`/`$1A`): increment, stage byte 0, scroll 0, power/options/objects cleared, score/lives/loop preserved | `stage.rs`, `game.rs` | at 7024 `loop 0 -> 1`, section `6 -> 0`, scroll `0`, score 830, lives 3; 0 divergences 7015-7026 on loop/section/scroll/score/lives; unit test | E-00 | 1 |
| E-02 | `Phase::Ending(EndingStep)` for `$87`-`$8A`: 1/2/1/1 frames, PPUMASK 0, no `State::GameOver`, render bypasses `stage_catalog::background` | `stage.rs`, `game.rs` | phase at 7024/7025/7027/7028/7029; 7025-7051 screen hash equals black on every frame; durations exact; stage 1-6 state hashes unchanged | E-01 | 1 |
| E-03 | extract `$CF48` (28 B) and `$CF90` tile data as a pack chunk with pinned hash; add `gradius-assets` accessor | `xtask/src/stage_data.rs` (or `stages.rs`), `gradius-assets/src/lib.rs`, new `ending_screen.rs` | chunk bytes/hash pinned; accessor unit test; pack still parses; no runtime change | E-00 | 1.5 |
| E-04 | port `$9D88` column builder / static ending screen from E-03; draw it in `Phase::Ending` instead of `starfield()` | `ending_screen.rs`, `game.rs` | full-screen hash equals oracle at 8100; pixel report 100% for the background region; no call to `starfield()` in the ending | E-03 | 1.5 |
| E-05 | `$8B` 23-frame build and `$0700` reveal: drain the queued columns at the measured rate | `game.rs`, `xtask/src/ending.rs` | phase `$8B` 7029-7051; at 7600 the text crop equals "CONGRAT"; at 7800 it equals the complete line; per-frame visible-column count matches `$0E` 7052-7800 | E-04 | 1.5 |
| E-06 | ending sprites from `$987F`: slot 8 tile `$9E` at (0x74,0x80), slot 9 kind `$28` at (0xA4,0x88); no player sprite | `game.rs` | trace slots 8/9 at 7052 match; pixel region at 7200 100%; no ship/player sprite regression | E-02 | 1 |
| E-07 | kind `$28` handler `$BB09` + `$BB7C` (38 B, 19 entries): six-frame warm-up then one entry per frame, x/y deltas, tile `$96+n`, explosion at end | `game.rs`, new `ending_ship.rs`, extractor | `$BB7C` hash pinned; slot 9 x/y/tile matches the trace 7052-7200 frame by frame; table unit test; object slots 0-7 unchanged | E-06 | 1.5 |
| E-08 | `$CE8E`/`$CF27`: decode the `$0700` record format and the `$4F/$4E` cadence; implement the path/stream and its `$FE` end | `ending_ship.rs`, `game.rs` | `$0700` bytes or the resulting screen match the oracle at 7052/7100/7200; `$4F` reaches `$FF` on the measured frame; no `$CF27` reinterpretation without a test | E-07 | 1.5 |
| E-09 | `$8C` display loop: 1202 frames, `$5B` freeze, object AI still runs, HUD, music `$A5`, no stage-7 bleed | `game.rs`, `gradius-app/src/music.rs`, `gradius-sdl/src/music.rs`, `gradius-wasm/src/lib.rs` | `$8C` at 7052 and 8253; `music=Boss` 7052-8253; state diff 7052-7997 has 0 divergences; `state=Playing` throughout; cue table test | E-05, E-06 | 1 |
| E-10 | +10000 and score extend `$2A`: score 830 -> 10830, lives 3 -> 4, `$2A` 1 -> 2 at 7998; `$84D8` threshold check after every add | `game.rs` | exact frame 7998 and values; unit test awarding 10000 from 830; `$2A=2` at 7998; stage 1-5 score/lives windows unchanged | E-09 | 1 |
| E-11 | `$8D` -> `$9B38` -> stage 1: `$1B=0`, `$19=0`, `loop=1`, `$1A=1`, `$17=1`, `$2A=2`, score/lives preserved; `$80` at 8282 | `game.rs` | phases 8254/8255-8259/8282; state at 8282 equals the measured table; first 600 stage-1 loop-2 frames within tolerance; new game still resets loop 0 | E-10 | 1 |
| E-12 | ending gate: `--ending-report`/fixture test; compare 7015-8290 state and the pixel window; document the start-state mapping if the full stage-7 replay cannot reach 7024 | `gradius-diff/src/main.rs`, tests, `game.rs` accessors | 0 divergences state 7015-8290; per-frame decoded-RGB hash equality after full reveal; pixel >= 99.9% (100% static-screen region); test fails on the pre-ending code because phase never reaches `$87` | E-01..E-11 | 1.5 |

Total: **16.5 h**; the critical path runs from E-00 to E-12 in order.

### 4. Risks and how to catch them in the harness

1. **The oracle window exists, the fixture does not.** `record-ending` already
   records to 8300, but `tests/fixtures/stage7-297-6050.state` stops before
   the ending and `stage7_replay` is report-only. Without an ending fixture
   (7015-8290) the cards have no gate. E-00 must produce it; E-12 must refuse
   to report success if the native run never emits `phase=87`.
2. **The native run may not reach 7024 at all.** `stage7_replay` stops at
   6050 and `advance_section()` clamps at stage 7: before E-01 the native phase
   never reaches `$87`. Detect with
   `gradius-diff --stage 7 --from 7015 --to 8290 --report`; if the first
   divergence is `scroll`/`section` before 7024, the ending window is
   unreachable by replay. Mitigation: the gate starts from a recorded ending
   state (oracle 7024 snapshot + a native `Game::new_at_ending`), and the full
   stage-7 replay stays a separate, later goal. Do not paper over it with a
   tolerance.
3. **The screen may animate.** The plan assumes a static `$9D88` screen. Compare
   oracle PNGs at 7100/7500/8100: if the star pixels move, E-03/E-04 are
   incomplete and there is an animation writer not found in `$8C`; stop and
   locate it rather than adding RNG stars.
4. **Reveal timing is the NMI drain.** If the port draws the full screen at
   7052, the 7600 hash fails. E-00 records `$0E`; E-05 compares the visible
   column count per frame, not just the final screen.
5. **`$CF27` may be a nametable stream, not a ship path.** Trace `$0700` at
   7052+; if the bytes are PPU writes, E-08 is a screen card. A sprite-only
   implementation will show up as a missing pixel region at 7200.
6. **Wrong CHR bank/palette for `$9E`/`$96..$99`.** The ending may not be in
   the stage-7 bank. Detect with the 7200 pixel region; a wrong bank gives the
   right position and the wrong tile, which the state diff cannot see.
7. **The score extend is shared with stages 1-5.** `$84D8`/`$2A` changes the
   common `add_score`; every existing stage replay must stay at 0
   divergences, and a 10,000-point stage-1 award must produce the same
   lives/threshold as the oracle.
8. **`$17=1` after the loop is measured but not derived.** `$9C3F` with the
   cleared power bytes gives rank 0 on paper; the measured value is 1. E-11
   must reproduce it and the loop-2 first 600 frames must stay in tolerance;
   if rank is wrong, spawns drift much later and the state diff at 8282 will
   not show it. Pin `$17` in the trace.
9. **`$1F`/`$60` can leave the ending early.** `$9C1E` has a branch that sets
   `$1B=$80`; E-02/E-05 must prove the 23-frame and 1202-frame windows cannot
   exit through it.
10. **Music `$A5` is the boss track.** The cue director may suppress `Boss`
    when no core exists; E-09 must assert the cue at 7052, not just that no
    error occurs.

## Fixture coverage and the pack inventory

The checked-in `tests/fixtures/stage7-297-6050.state` replays oracle frames
297-6050 through `Game::new_at_stage(6)` and stops at the `$3F = $0B` scroll
hold (5929-6050, resumed at 6051). It therefore covers normal stage-7 play only:
the `$81` clear, the `$9865` wrap and the `$87`-`$8D` ending are all outside it.
`stage7_replay` stays report-only: it reports **11,512 divergences** over 5754
compared frames, first at 4573 `obj.slot` (oracle absent, native slot 5 kind
`$8C`); `scroll` is byte-identical over the whole window (5754/5754, native
freeze 5929-6050) and `$3F = $0C` lands at 6562. No ending frame is exercised
there.

The ending has its own fixture instead. `tests/fixtures/ending-7015-8290.state`
and `.trace` record the oracle run, and `ending_replay` seeds
`Game::new_at_ending` at the measured 7024 `$87` state and gates oracle
7025-8290 (1266 frames) with `compare_objects: false`; see the status section.
The full stage-7 replay still cannot reach 7024 (it is behind by 5801 and
`advance_section()` clamps at stage 7), so the seed is the gate's start state,
not a tolerance.

What the pack already carries for the ending, checked through the
`gradius-assets` API by `stage7_replay.rs` when `assets/gradius.pak` exists:

| item | in the pack? |
| --- | --- |
| all 2048 CHR tiles (both pattern tables, every bank) | yes; the ending ship `$9E` and the `CONGRATULATIONS!` glyphs are pixels in it |
| 64 NES palette colours | yes |
| GAUD audio, including `$A5` (the boss track used for the ending) | yes |
| `$CE2B` 88-byte wall-erase table | yes, `STAGE_WALL_ERASE`, exposed as `Assets::stage6_wall_erase`; `erase_wall` still reads `stage6_wall::TABLE` (the one open pack wiring) |
| `$CF27` ending text record | yes, `ENDING_TEXT_RECORD`: 8 pointers and a 19-byte text record ending on `$FE`; read at runtime through `Assets::ending_text_record` by `draw_ending_text` |
| ending screen/text layout (`$CF48`, `$CF90`) | yes, `ENDING_SCREEN_STREAM` / `ENDING_SCREEN_CLASSES` |
| tile-class resolver (`$9FD6`/`$9FE6`, `$D6F2`, `$D772`) | yes, `ENDING_CLASS_POINTERS` / `ENDING_CLASS_ATTRIBUTES` / `ENDING_CLASS_TILES`; see below |
| starfield parameters | none separate: the screen is built from the `$CF48`/`$CF90` tables; the port's procedural `starfield()` is not the original |
| map from kind `$28` to its sprite tile | yes, `ENDING_SHIP_MOTION` (26 pairs + `$FF`, tile = `$96 + low nibble`); code `$BB09` still has to run it |

All the stage tables arrive in `Assets.stage` (`StageAssets`), copied out of
`CHUNK_STAGE` by `Pack::into_assets`; the accessors return a typed
`AssetError::Missing` on a pack that predates the extraction, never invented
bytes. Consumed today: `ending_text_record`. Exposed for the remaining cards:
`ending_class_pointers`, `ending_class_attributes`, `ending_class_tiles` (E-04)
and `stage6_wall_erase` (the `erase_wall` rewiring).

### The ending tile-class tables (`$9D88`)

`$9D88` turns one `$CF90` class byte into nametable pixels in three steps
(disassembled from `Gradius (E).nes`):

1. `$9FB6[$19]` is `$CF48`; `$55` indexes it and the byte selects a `$CF90`
   block through the `$9D49` `$38`-byte offsets. The offset inside the block
   is `$AE = 8*($58 & 7) + (($58 & $F0) >> 5)`, plus 4 when `$54` has bit 7
   set. The class is the byte at that offset.
2. `$9FD6[$19]` is `$D772`: 128 little-endian pointer words. The word at
   `$D772 + 2*class` addresses a zero-terminated tile run.
3. `$9FE6[$19]` is `$D6F2`: 128 attribute bytes, one per class, read at
   `$D6F2 + class`.

For the ending `$19 = 0`, so the tables are entry 0 of each pointer table:
`$9FD6[0] = $D772`, `$9FE6[0] = $D6F2`. The packed sizes are 32 bytes
(`$9FD6..$9FF6`, eight tile then eight attribute pointers), 128 bytes and
1605 bytes; `ending_class_tiles` keeps the raw pointer table and runs, and its
`run(class)` returns the bytes up to the `$00` terminator.

Run byte decoding: `$00` ends the run; a byte with a nonzero high nibble is a
literal tile (`$3C`, `$3D`, ...); bytes below `$10` are codes resolved through
`$9D6D`, except `$07` (fills `$ED`), `$08` (fills `$00`) and `$09`/`$0A`
(alternate `$40`/`$41`). `$9D6D` is a sixteen-byte table holding one fill tile
per code `$00..$0F`.

The ending build runs during `$8B` (`7029-7051`, four columns per frame). It
only reaches `$55 = 0` and `1`, and `$CF48[0] = $CF48[1] = $00`, so **the whole
ending is block 0 of `$CF90`**. Its 56 class bytes contain only seven classes:

| class | `$D6F2[class]` | resolved tiles |
| ---: | ---: | --- |
| `$00` | `$10` | `$00` |
| `$01` | `$10` | `$00 $00` fill, literal `$3C` |
| `$02` | `$10` | `$00 $00`, literal `$3E`, `$00 $00`, literal `$3F` |
| `$03` | `$14` | `$00`, fill, literal `$3F` |
| `$33` | `$4C` | `$00` (verified, below) |
| `$34` | `$04` | blank fill |
| `$7B` | `$10` | literal `$3D`, `$00 $00`, fill, literal `$3E` |

Oracle check: the first build is `$58 = 0`, `$54 = 0`, so `$AE = 0` and the
class is `$CF90[0] = $33`. Its attribute `$D6F2[$33] = $4C` is written at
`$23C0`, and its run `01` resolves through `$9D6D` to tile `$00`, written at
`$2000..$2003`. The nametable taken at frame 8100 with
`cargo xtask record-ending ... --stage 7 --frames 8100 --nametable e7.nt` has
`$4C` at `$23C0` and `00 00 00 00` at `$2000..$2003`, so the class, attribute
and tile path all match.

**The `$AE = 56` doubt is closed: the maximum is 55.** `$9F96` loads the
literal `$19` (25, not the zero-page stage byte) and `$9F98` adds `$58` with
the carry still set from the `CMP #$06` at `$9F92`, so the step is
`$58 += 26`. Starting at 0 the walk hits `{0..6, 32..38, 64..70, 96..102}`,
never low-three-bits 7, and the largest offset is `8*6 + 3 = 51`, or `55` once
`$54` bit 7 adds the 4. `$AE` never leaves the `$38`-byte row (indices
0..`$37`), so no class byte is read from the following block.

Pack growth: the three new tables add 32 + 128 + 1605 bytes plus headers;
`assets/gradius.pak` goes from 226557 to 228402 bytes (+1845). Two
`cargo xtask extract` runs from the same ROM produce the same sha256
(`1c6857b5...fbcf7`), so the pack stays reproducible.

The `$89`/`$8A` setup and the `$8C` text reveal are code, not class-table data;
they are measured in the next section.


## The `$CF90` column build: four `$9D88` calls per frame

This is the measured answer to "how does `$CF90` reach the nametable". Every
number below comes from the oracle, not from the class tables:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-cfscreen
ST7=$(for f in $(seq 1 8300); do printf -- '--poke %d:26:06 ' $f; done)

# every $2007 write from 7010 on, with the same poke/press script as the gate
cargo xtask nametable "$ROM" --from 7010 --to 8300 --press 180:start \
  --press 0:a --keep-alive $ST7 --range 2000:2FFF --out /tmp/opencode/nwrites.txt

# physical nametables at the frames the card asks for
for fr in 7060 7200 7600 8000; do
  cargo xtask probe "$ROM" --frame $fr --press 180:start --press 0:a \
    --keep-alive $ST7 --nametables /tmp/opencode/nt$fr.bin
done

cargo xtask disasm "$ROM" --from 9D7D --to 9FB6 --labels \
  --symbols tools/ghidra/symbols.txt
```

### Who writes, and when

* `$987F` (`$8B`) reads `$57`: while `$57 == 0` it jumps to `$9C1E`; when `$57`
  is set it inits the two ending objects and `INC $1B` (`$8C`).
* `$9C1E` sets `$0D = $05`, then calls `$9D88` **four times per frame** (three
  `JSR` and a tail `JMP`). Its `$57 != 0` branch (`$1F = 1`, `$60 = 1`,
  `$1B = $80`) is the escape hatch, never taken here: it would only run if
  `$9D88` set `$57` *inside* the four calls, which the last frame does not.
* Each `$9D88` call appends one record to the `$0700` VRAM queue: opcode `$01`
  plus the attribute address and byte, then four blocks of opcode `$01` plus
  a tile address and up to four tiles, each block ending on `$FF`. The NMI
  drainer `$8A56` turns that into `$2007` writes. **One call = 17 writes
  (16 tiles + 1 attribute byte); four calls = 68 writes per frame.**

### What one call paints

`$9D88` is the shared per-stage column routine (see below), so its geometry is
not ending-specific:

* `$54/$55` is the source column position. The ending starts it at 0 (`$9B64`
  copies `$55 = $3F = 0`); each *step* adds `$80` (`$9FA4`) and wraps into
  `$55`. After three steps it reads `$0180`, which is `>= $3E/$3F = 0`, so
  `$9DA9` sets `$57` and the build stops.
* `$58` steps through the 28 offsets `{0..6, 32..38, 64..70, 96..102}`; the
  class cell is `$AE = 8*($58 & 7) + (($58 & $F0) >> 5)` (plus 4 for `$54` bit
  7, the second nametable). `$AE` is also the attribute cell index.
* The class is `$CF90[$38*height + $AE]` with height `$CF48[$55]`; for the
  ending `$55` is 0 or 1 and `$CF48[0] = $CF48[1] = 0`, so only **block 0**
  (56 bytes) is used.
* The attribute byte is `$D6F2[class]`; the tile data is the `$D772` run for
  `class`. The run fills the cell **row by row**: a byte `$00` writes one
  `$00`; a byte with a nonzero high nibble writes that literal tile; `$07`
  writes `$ED` twice, `$08` writes `$00` twice, `$09`/`$0A` fill the rest of
  the row with alternating `$40`/`$41`; any other byte `< $10` fills the rest
  of the row with `$9D6D[byte]`. The run cursor does not reset between the
  four rows.

A call therefore paints one 4×4-tile (32×32 px) cell: tile rows `4r..4r+3`,
tile columns `4c..4c+3`, plus its attribute byte at `$23C0 + 8r + c`
(nametable 0) or `$27C0 + 8r + c` (nametable 1), where `r = $AE / 8`,
`c = $AE % 8`.

### Order and rhythm

The build is strictly left-to-right, then top-to-bottom inside each band:

1. nametable 0, attribute columns 0..7 (tile columns 0..3, 4..7, ..., 28..31),
   each with attribute rows 0..6 (tile rows 0..27): **56 calls, frames
   7029-7042**;
2. nametable 1, attribute columns 0..3 only, rows 0..6: **28 calls, frames
   7043-7050** — the second nametable is built ahead but never displayed.

That is 84 calls in 21 frames, 4 per frame, **1428 `$2007` writes total**.
The measured per-frame counts are 64 at 7029 (the queue carries part of the
first record from the previous frame), 68 at 7030-7049 and 4 at 7050. `$8B` is
7029-7051 and `7051` is idle; `$8C` starts at 7052, so the whole screen is
built behind the black PPUMASK window and is static afterwards.

### Oracle check (frames 7060, 7200, 7600, 8000)

Replaying only those 1428 writes over a zeroed `$2000-$23FF` reproduces the
oracle's nametable 0 exactly: 896/896 tiles (`$2000..$237F`) and 56/56
attribute bytes (`$23C0..$23F7`) match, no diff.

The nametable is static from 7060 on. The full table hashes change only because
the `$CE8E` text reveal rewrites row 14, columns 8..23 (`$22C8..$22D7`), and
the HUD rewrites rows 28-29. The background region without those is identical
at all four frames:

| hash (FNV-1a 64) | value |
| --- | --- |
| tiles `$2000..$22C7` + `$22D8..$237F` + attrs `$23C0..$23F7` | `F3949D72384C0FE1` at 7060, 7200, 7600, 8000 |
| decoded tiles, 32×28 row-major (896 B) | `86D982DCD71930B5` |
| decoded attributes, 7×8 row-major (56 B) | `628965C1E92853CD` |
| tiles ++ attributes | `39598872B50527DD` |

### Is it the background engine? Yes in the ROM, no in the port

`$9D88` **is** the stage-background column engine: `$98E1` (scroll) calls it
once per frame during normal play, and the ending calls the same routine four
times per frame with `$3E/$3F` frozen. The tables are the same per-`$19` ones
(`$9FB6`/`$9FC6`/`$9FD6`/`$9FE6`); the ending runs `$19 = 0`, which is why it
picks blocks `0` and the `$D772`/`$D6F2` entries.

The port does **not** run this engine for any stage: `stage1_bg.rs` and
`stage_bgs.rs` hold pre-decoded `Screen` maps (tiles + attributes + palette)
extracted from the oracle, selected by `stage_catalog::background`, and
`stage1_bg::Screen` already has the 32×N tile/attribute/palette shape. So the
ending cannot "reuse the port's column engine" because one does not exist; it
should reuse the `Screen` representation instead. Everything needed to decode
the screen is already in the pack: `ENDING_SCREEN_STREAM` (28 B),
`ENDING_SCREEN_CLASSES` (15×`$38` B), `ENDING_CLASS_POINTERS` (the `$9FD6`/
`$9FE6` words), `ENDING_CLASS_ATTRIBUTES` (`$D6F2`, 128 B) and
`ENDING_CLASS_TILES` (`$D772`, runs).

### Rule for the core card (replaces E-04's sketch)

1. In a new `ending_screen` decoder, take block 0 of `ENDING_SCREEN_CLASSES`
   (`$55 = 0`, `$CF48[0] = 0`).
2. For `cell` in `0..56`: `r = cell / 8`, `c = cell % 8`,
   `class = block0[cell]`, attribute cell `(r, c) = $D6F2[class]`, and decode
   the class run (`$D772` word for `class`, pointer table entry 0) into the
   4×4 tile cell at `tiles[4r + y][4c + x]` with the row-by-row rules above.
3. Expose it as a `Screen`-like 32×28 tile block + 7×8 attribute table with the
   ending palette; draw it in `Phase::Ending` instead of `starfield()`. Rows 28
   and 29 stay HUD; row 14, columns 8..23 is overwritten by the `$CE8E` text
   reveal, so the text row and the background are independent.
4. Test (ROM-free, pack-only): decode and assert
   `fnv1a(tiles ++ attributes) == 0x39598872B50527DD`, plus tile count 896 and
   attribute count 56. Optional oracle probe (skipped without `GRADIUS_ROM`,
   like `stage1_late_pixels`): at frames 7060/7200/7600/8000 the nametable
   region `$2000..$22C7 ++ $22D8..$237F ++ $23C0..$23F7` hashes to
   `F3949D72384C0FE1`; the per-frame hash is the regression that catches a
   mistimed build or a stray `starfield()` write.

No build-order test is needed beyond that: the screen is invisible until 7052
and static after it, so a single decode at `$8B` start matches every visible
frame. If the port later wants the `$0700` cadence (e.g. for a frame-exact VRAM
log), it is 68 writes/frame for 21 frames in the order above.


## The ending text and the `$8C` reveal

This corrects the earlier guess that `$88BB`/`$88FB`/`$8931`/`$89E8` write the
`CONGRATULATIONS!` row. They are the **rotating HUD/meter refresh** and write
the bottom band only. The text row has a different writer, `$CE8E`, reached
from the ending ship's object handler. Everything below was measured on
`Gradius (E).nes` with

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-endtext

# per-frame PPU writes from 7010 on (stage 7 poked every frame until it starts)
cargo xtask nametable "$ROM" --from 7010 --to 8300 --press 0:a:8300 \
  --press 180:start:6 --keep-alive $(for f in $(seq 1 400); do echo --poke "$f:26:06"; done) \
  --out /tmp/opencode/nwrites.txt
# PPUCTRL / CHR bank / palette / nametable at 7537 and 7600
cargo xtask probe "$ROM" --frame 7600 ... --nametable nt7600.bin --png end7600.png
```

The run reproduces the checked-in `ending-7015-8290.trace` frame for frame
(phase `$8C` 7052-8253, +10000 at 7998, `$8D` at 8254), so the write log and
the fixture describe the same ending.

### The four `$89`/`$8A` routines: HUD and power meter only

`$9C0C` (`$1B = $89`, frame 7027) calls `$88BB`, `$88FB` and `$8931` once each
to seed the bottom two nametable rows. `$9C18` (`$8A`, frame 7028) calls
`$89E8`. During the whole ending the same four records are re-queued by the
per-frame HUD refresher `$889D` (called from `$9A7F`): on every even frame it
dispatches `$48 & 3` to one of `$88BB`/`$88FB`/`$89E8`/`$8931`, i.e. each field
is rewritten every 8 frames. All are `$0700` queue writes (`$85ED`); `$8A56`
drains them in NMI, so the visible write lands in the same frame.

| routine | queue record (data starts at) | tiles written | meaning |
| --- | --- | --- | --- |
| `$88BB` | `$873E`: `$23A2` + 4 zeros, patched in the queue | `00 61 00 30+lives` at `$23A2-$23A5` (row 29, cols 2-5) | ship icon `$61` and the lives digit from `$20,X` (`$18` = player) |
| `$88FB` | `$8745`: `$23B4 64 65 00`, then appended digits | `64 65 00` + 6 BCD digits + `30` at `$23B4-$23BD` (row 29, cols 20-29) | `HI ` + high score `$07E0-$07E2` (6 digits) + constant low `0` |
| `$8931` | `$874B`/`$8751`: `$23A8 31 66 00` or `$32 66 00`, then digits | `1P `/`2P ` + 6 BCD digits + `30` at `$23A8-$23B1` (row 29, cols 8-17) | score `$07E4-$07E6` (P1) / `$07E8-$07EA` (P2); also keeps up the `$42` power bookkeeping |
| `$89E8` | `$8737`: `$2384` + the five power groups, ending `$23F8` + 7 zeros | 24 tiles at `$2384-$239B` (row 28, cols 4-27) and the attribute bytes `$23F8-$23FE` | power-meter row; the group of the selected entry (`$42`) is replaced by `1D 1E 1E 1F`, and one attribute byte is set to `$55`; bails when `$0100 >= 2` |

The digit writer `$891A` splits a BCD byte into two `$30+n` tiles, most
significant nibble first, and both score fields end with a constant `$30`, so
the on-screen score is the traced BCD value times ten (830 shows as `0008300`).
None of the four touches OAM or the text row.

### Where the text comes from

The text row is a 48-byte record at **`$CF35`**, selected by the pointer table
**`$CF27`** (eight words, all `$CF35`; `$CE8E` clamps the loop index `$1A` to
6). The record is:

```
$CF35: 22 C8   little-endian PPU address $22C8 (nametable 0, row 22, col 8)
       ...     sixteen tile indices follow, then $FE closes the record
```

`$CE8E` prefixes the queue record with `01` (the `$8A56` control byte:
PPUCTRL increment 1), then `22 C8`, then the characters, and closes with `$FF`.
So the row is one fixed line, **16 tiles at `$22C8-$22D7`** (screen x 64,
y 176), background palette 0 (attribute `$23EA = $10`, colours `0F 12 30`).

**The font is CHR bank 3 (CNROM), pattern table 0.** The probe at 7600 reads
`ppu ctrl = $A8` (bit 4 clear: background from `$0000`) and `chr bank = 3`,
and the glyphs live there. There is **no width table**: every character is one
8x8 tile, spaces are the empty tile `$00`, and the 16 tiles are contiguous.
Measured character -> tile map:

| char | tile | char | tile | char | tile | char | tile |
| --- | ---: | --- | ---: | --- | ---: | --- | ---: |
| space | `00` | A | `02` | G | `01` | N | `0F` |
| C | `0B` | I | `11` | L | `08` | O | `05` |
| R | `07` | S | `0A` | T | `0C` | U | `1C` |
| ! | `39` |  |  |  |  |  |  |

The HUD band uses the *other* font: CHR bank 1, pattern table 0 (digits
`$30-$39`; `$61` ship icon, `$64 $65` = `HI`, `$31`/`$32` + `$66` = `1P`/`2P`).
The oracle therefore switches the CNROM latch mid-frame: bank 3 above the
sprite-0 split, bank 1 for the HUD band. The 7600 PNG shows both correct at
once (`chr bank = 3` in the vblank snapshot, bank-1 HUD digits on screen). The
port already draws the HUD with pack bank 2 (= CNROM bank 1, `HUD_BANK` in
`hud.rs`), so the ending needs bank 6 (= CNROM bank 3, `$0000` half) for the
screen and text.

### The reveal rule (`$8C`)

The reveal is *not* in the `$8C` phase routine (`$98D0` only bumps `$5B` and
runs the object/PPU update). It is driven by the ending ship's handler
**`$BB09`** (kind `$28`, slot 9):

```
$BB0B  LDY $048C,X     ; $0495, the ship's alive/explosion flag
$BB0E  BEQ $BB23       ; ship gone: motion/explosion path
$BB10  LDA $4F
$BB12  CMP #$FF
$BB14  BEQ $BB19       ; finished: DEC $4C, then $AEF2 and INC $1B -> $8D
$BB16  JMP $CE8E       ; text step
```

Once the 19-entry path is done (`$0475 >= $1A`, the `$FF` entry sets
`$4E = $A0`), `$CE8E` runs once per frame:

1. if `$4E != 0`, `DEC $4E` and return;
2. else `$4E = 8`, and queue a **fresh complete record** with `$4F + 1`
   characters (the whole prefix, not just the new tile) at `$22C8`, then
   `INC $4F`.

So the row is rewritten from the start at every step while one more character
becomes visible; a port that only writes the new tile gives the same pixels.
The cadence is **one character every 9 frames** (`$4E = 8` plus eight
decrements). When the writer reaches the `$FE` terminator it sets `$4F = $80`
and rewrites the full 16-character line once more. `$4F = $80` is the pause
before the score award; `$4F = $FF` means finished.

Measured timeline (one text line, so this is the whole text table):

| frame | event |
| ---: | --- |
| 7024 | `$87` reset: `$9B38` clears 2 KiB of nametable plus the HUD (2280 writes) |
| 7025 | `$88` (24 writes at `$28E8`) |
| 7027 | `$89`: `$9C0C` queues lives/high score/score (`$23A2`, `$23B4`, `$23A8`) |
| 7028 | `$8A`: `$89E8` queues the meter row and attributes (`$2384`, `$23F8`) |
| 7029-7050 | `$8B`: static screen build, four columns per frame; last column 7050 |
| 7052 | `$8C` starts; HUD fields keep rotating every 2 frames |
| ~7377 | ship path `$FF` entry sets `$4E = $A0` (inferred: `$4E = 1` at 7537) |
| **7538** | first character `C` (`$22C8 <- $0B`) |
| 7592 | seventh character: `CONGRAT` complete |
| **7600** | `CONGRAT` on screen (7 of 16) |
| **7673** | 16th character `!`; `CONGRATULATIONS!` complete |
| **7682** | `$4F = $80`: full line rewritten, record closed; reveal done |
| **7998** | `$4F = $FF`: `$8444` adds +10000 (score 830 -> 10830, lives 3 -> 4) |
| **8253** | last `$8C` frame |
| **8254** | `$8D` (`$4C` ran out): 2304 writes clear the screen, stage 1 rebuild starts |
| 8282 | stage 1 playable again |

Write counts at the reveal steps are exactly 1, 2, ..., 16, then 16 again
(7538 + 9n), which confirms the whole-prefix rewrite.

### What the port has to implement (code, not data)

1. One text line, not a per-column screen reveal: on each step queue
   `[01][22][C8]` + the first n characters + `[FF]`, or just the new tile
   `$22C8+n-1`; the visible result is identical. First step at 7538, every 9
   frames, 16 steps, final close at 7682.
2. The trigger is the ship path end plus `$4E = $A0` (160 frames), not `$8C`'s
   start. Do not start the text at 7052: the 7600 check fails.
3. The text uses CHR bank 3, PT0; the HUD band stays on CHR bank 1, PT0. The
   ending needs the mid-frame latch switch at the sprite-0 split (see gap 1).
4. The reveal shares `$4F`/`$4E`/`$4C` with the score award and the `$8D`
   transition: text 7538-7682, +10000 at 7998, `$8D` at 8254 (256 frames of
   `$4C` after `$4F = $FF`).
5. `gradius-assets::ENDING_TEXT_RECORD` names `$CF27`/`$CF35` correctly: it is
   the text record and its pointer table, not a ship path. The ship motion is
   `$BB7C` (`ENDING_SHIP_MOTION`).

### Gaps from this pass

1. **The CNROM latch store is not in the static disassembly.** No `STA/STX/STY`
   with an operand in `$8000-$FFFF` exists anywhere in the 32 KiB PRG (absolute,
   `,X`, `,Y`, indirect or `(zp),Y` all checked). The behaviour is measured
   (bank 3 above the split, bank 1 for the HUD band) but the instruction that
   switches it was not located; do not assume the split handler writes it.
2. **The `$FD` escape in `$85F8`** (used by string `$8800` at `$987F`) emits
   `$FF $01` and sets `$9B = 2`. Its effect on the background palette record is
   not decoded; the *measured* palette is in the probe, but the record semantics
   are missing.
3. **The ship path `$FF` frame is inferred, not traced.** `$4E = 1` at 7537 and
   `$4E = $A0` at the `$FF` entry put it at ~7377; the trace does not carry
   `$0475`/`$0495`/`$4E` (E-00's extended trace would close this).
4. `$88BB`'s `LDA #$1E; STA $9A` before `$85ED` is dead (`$85F8` overwrites
   `$9A` with the string index). Harmless, but do not port it as meaningful.


## Porting checklist

1. For `$19 >= 5`, `$99B3` goes straight to `$86`: do not spawn the `$98` core,
   and do not run `$9975`/`$9971`.
2. Stage 6: run `$C40D` on `$02 & 3 == 0` while `$1B == $82`. Spawn one
   collision-only box in the first free `$0136` slot: X = 152, Y = 66 or 132
   (alternate on `$02 & 4`), `$0176 = 1`, `$0316 = 1`, `$0136 = $8D`.
3. Move boxes X -4/frame; move lane 66 down and lane 132 up with the
   `$C449`/`$02` 8.8 speed from `$03C6:$03F6` (0-4 px/frame). Cap at ten live
   boxes and draw group `$8D`.
4. Player hit: lethal unless shield `$46`; use the `(+6,+8)` probe with the
   22 x 18 box, and the shield clears the box. Shot: one hit clears the box
   and awards 10 points.
5. End the `$82` window with the per-rank timer (`$9A28[$17] * 256`; stage 7
   uses 458 frames = `$01CA`), then set `$1B = $86`.
6. Stage 6: while `$1B == $86`, queue two tile erases per frame from the
   `$CE2B` table and increment `$66` by 2; stop at `$58` (88 entries). Keep the
   shape and the frame cadence for the wall opening.
7. Stage 7: no object spawns and no boss. End when `$3F >= $98F0[6] = $0D`.
8. At `$19 == 6`, run the `$9865` reset: `$1B++`, `$3F = 0`, `$26 = 0`,
   `$24 = 0`, `$22 = ($42 != 0)`, `$28++`; then the `$87-$8D` ending states.
9. In `$9B38`, reset the listed pages and bytes, preserve score/lives, copy
   `$26 -> $19`, `$28 -> $1A`, and restart at stage 1. The +10000 is not here:
   `$8444` adds it from `$CE8E` at the ending ship's path end during `$8C`
   (7998); `$8D` only clears `$1B` and jumps to `$9B38`.
10. Keep `$39`/`$992E` out of the normal path; if it is ever ported, reproduce
    the double `INC $19` (or deliberately fix it and document the divergence).
