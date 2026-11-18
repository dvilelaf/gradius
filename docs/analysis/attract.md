# Attract / demo timeline

Per-frame evidence for the boot attract loop: the title roll-in, the
title-to-demo transition and the scripted demo input. Recorded with the oracle
(`gradius-nes`) from reset, no pad input.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-attract

cargo xtask record-input "$ROM" --frames 800 \
  --out /tmp/attract-800.trace --script scripts/attract-800.txt
cargo xtask record-input "$ROM" --frames 800 --press 180:start --out /tmp/attract-s180.trace
cargo xtask record-input "$ROM" --frames 800 --press 240:start --out /tmp/attract-s240.trace
```

## Trace format

`record-input` writes one line per frame:

```text
<frame>:<buttons>  # state=<hh> stage=<hh> scroll=<low>:<high>:<frac> held=<hh>
```

* `frame` is the completed-frame count (the file starts at 1).
* `buttons` is the logical held set (`a,b,select,start,up,down,left,right`);
  `held` is the raw `$07` byte.
* `state` is `$00`, `stage` is `$1B`, `scroll` is `$3E:$3F:$3D`.
* The sample is taken at the end of the visible scanlines, **before** the
  vblank NMI latches the next pad read (`$81B7`). That byte is what the
  frame's player update used. A line labelled `N` therefore describes frame
  `N - 1`, and the replay script labels it `N - 1`.

The ROM's controller read (`$81C2`-`$820D`) shifts the pad in bit-reversed:
in `$07`, A is bit 7, B bit 6, Select bit 5, Start bit 4, Up bit 3, Down bit 2,
Left bit 1, Right bit 0. `record-input` undoes that in the names so
`FRAME:buttons` lines can be fed back through `gradius-diff --script`.

## No-input timeline

| frame | `$00` | `$1B` | what happens |
| --- | --- | --- | --- |
| 1-132 | 0 | 0 | title and X-scroll roll-in |
| 133 | 1 | 0 | title-to-demo transition starts |
| 390 | 2 | 0 | demo play starts |
| 392-393 | 2 | 1 | stage init (`$9B38`) |
| 394 | 2 | 2 | HUD text (`$9BE7`) |
| 395 | 2 | 3 | palette/HUD (`$9C0C`) |
| 396-418 | 2 | 4 | power meter + scroll passes (`$9C18`, `$9C1E`) |
| 419 | 2 | `$80` | normal play; demo input generator enabled |
| 424 | 2 | `$80` | scroll first advances (`0:0:128`, then 0.5 px/frame) |

`$07` is zero on frames 1-419. From 419 the demo drives itself with a ROM
input table (below); the pad is never read into the player update.

## State machine

`$00` is dispatched at `$80B6` through the inline table at `$80CC`:
0 `$80DA` title, 1 `$810E` title-to-demo, 2 `$8119` demo play, 3 `$812F`
start countdown, 4 `$815D` enter stage, 5 `$9643` stage handler, 6 `$8164`
ending.

* **Title (0), `$80DA`.** First frame: `$8831` draws the title, `$824E`
  sets up sound/timers, `$13 = 0`, `$12 = $FE`, `$01 = 1`. Later frames run
  `$80F7`: `$12 -= 2` per frame and `$8279` writes `$12` into PPUSCROLL X, so
  the title rolls in from X = 254 to 0 over 127 frames. When `$12` reaches 0,
  `$817E` sets `$00 = 1`. Reset warm-up puts frame 133 at the transition.
* **Transition (1), `$810E`.** `$8299` places the demo ship at X `$0360 =
  $50` and Y `$0320 = $82AC[$0F]` (`$86`/`$96`), then `$8193` waits for the
  16-bit countdown `$4C/$4D` to reach 0. `$8271`/`$8276` load `$4C = 0`,
  `$4D = 1` (0x0100 = 256 frames) and `$8411` decrements it once per frame.
  With no Start, `$817E` sets `$00 = 2` at frame 390.
* **Demo (2), `$8119`.** First frame jumps to `$82BF` (clear score/objects,
  `INC $20/$09`). Later frames call `$9640` = `JSR $9C67` (demo input) then
  `$9643` (stage handler). `$0B` non-zero ends the demo and `$8187` returns to
  `$00 = 0` (title). A Start/Select edge during demo also ends it
  (`$9CAB` sets `$0B`).

## Demo input is scripted

`$9C67` runs only while `$1B` bit 7 is set. Every other frame (`$02` bit 0)
it consumes one record from the table at `$9CB1`:

| offset | byte | meaning |
| --- | --- | --- |
| `$9CB1` | `$80` | buttons: A |
| `$9CB2` | `$90` | ticks: 144 (= 288 video frames) |
| `$9CB3` | `$C4` | buttons: A+B+Down |
| `$9CB4` | `$10` | ticks: 16 (= 32 video frames) |
| `$9CB5` | `$80` | buttons: A |
| `$9CB6` | `$00` | ticks 0; `DEC $30` wraps to `$FF` (= 510 frames) |
| `$9CB7` | `$82` | buttons: A+Left |
| `$9CB8` | `$25` | ticks: 37 (= 74 video frames) |

`$9C82` copies the record's button byte into `$05` and `$07`, so the same
player-update code runs in the demo and in play. `$9CAB` increments `$0B` on
a Start/Select edge or on the `$FF` end marker.

Observed 0-800 input, matching `scripts/attract-800.txt`:

| frame | `$07` | buttons |
| --- | --- | --- |
| 419-706 | `$80` | A |
| 707-738 | `$C4` | A+B+Down |
| 739-799 | `$80` | A |

## Pressing Start during the attract

Start is sampled from `$05 & $30` (ROM layout: Start/Select). In `$00 = 1`,
`$8212` does not start the game immediately: `$8225` loads `$03 =
$824C[$0F]` and `$8187` sets `$00 = 3`, the 80-frame start countdown.
`$812F` counts `$4C = $50` frames, then `$82CD` resets score/objects and
`$817E` moves to `$00 = 4`, then 5.

| run | `$00` transitions | `$1B` setup | scroll starts |
| --- | --- | --- | --- |
| no input | 0 -> 1 at 133, 1 -> 2 at 390 | 1 at 392 .. `$80` at 419 | 424 |
| Start at 180 | 0 -> 1 at 133, 1 -> 3 at 182, 4 at 263, 5 at 264 | 1 at 265 .. `$80` at 292 | 297 |
| Start at 240 | 0 -> 1 at 133, 1 -> 3 at 242, 4 at 323, 5 at 324 | 1 at 325 .. `$80` at 352 | 357 |

A Start press during `$00 = 0` (before the roll-in ends) is handled by
`$8240`: it sets `$00 = 1` and reloads the `$4C/$4D` countdown; the next
Start, in state 1, enters the countdown. Pressing during state 1 alone is
enough once the automatic 0 -> 1 transition has happened.

## Port guidance

`gradius-core`'s current `Phase::Attract` holds the extracted title for the
oracle's window and reports the matching `$00` spellings, but it does not
reproduce the demo scroll or the exact state-1..4 durations. This trace fills
those gaps:

* Title: set X scroll 254, subtract 2 per frame for 127 frames, then enter
  the transition at frame 133. There is no pad input.
* Transition: place the demo ship at X 80 / Y 134 and count 256 frames to
  frame 390. A Start edge here enters `$00 = 3` (81 frames) and then the
  `$1B` 0..4 stage intro, not the demo.
* Demo: at frame 390 initialise the stage with the `$1B` 0 -> 1 -> 2 -> 3 ->
  4 -> `$80` sequence (0 at 390-391, 1 at 392, 2 at 394, 3 at 395, 4 at 396,
  `$80` at 419), then start the 0.5 px/frame scroll at frame 424.
* Drive the player from the `$9CB1` table (A, then A+B+Down, then A) instead
  of the pad, and end the demo to title when `$0B` would be set (Start/Select
  edge or the table's `$FF` terminator).
* `scripts/attract-800.txt` is the effective input for frames 0-800; feed it
  through the normal input path to reproduce the demo without implementing
  the demo table first.
