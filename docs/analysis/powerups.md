# Power meter, capsules and player death (stage 1)

All addresses are CPU/PPU addresses in the ROM's `$8000-$FFFF` window or its
zero page. Evidence was read with `xtask disasm`, `xtask probe` and `xtask
ramdump`; the `probe --poke` helper was added for the controlled pickups and
death in this document and is described under "Driving the ROM".

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-powerups

# HUD bar sprite (OAM slot 0)
cargo xtask probe "$ROM" --frame 900 --press 180:start \
  --oam oam/900.bin --nametables nt/900.bin

# Disassembly used below
cargo xtask disasm "$ROM" --from 8950 --to 8A55
cargo xtask disasm "$ROM" --from C0C1 --to C260
```

`probe --poke FRAME:ADDR:VALUE` writes one RAM byte at the start of `FRAME`
(before that frame runs); both numbers are hexadecimal and the option is
repeatable. It is only a test hook: no game code is changed.

## The power meter

### Selection and activation

| item | value |
| --- | --- |
| selection byte | `$42`, 0 = no selection, 1..6 = meter rows |
| pickup | `$8950`: `INC $42`; if `$42 >= 7` set `$42 = 1`; redraw `$8A35` |
| activation | `$8979`: only while `$0100 == 1`, and only on **held** B (`$07` bit 6) |
| dispatcher | `$8989` loads `$42`, `JSR $83E9` indexes the word table at `$898E` |
| redraw | every successful handler clears `$42` and jumps to `$8A35`; a no-op `RTS` leaves `$42` alone |

Dispatch table at `$898E` (7 entries, `$42` 0..6):

| `$42` | handler | effect | no-op condition |
| --- | --- | --- | --- |
| 0 | `$8988` | `RTS` | always |
| 1 | `$89A6` | `INC $40` | never |
| 2 | `$89B4` | `INC $41` | `$41 != 0` |
| 3 | `$89C0` | `$44 = 2` (double) | `$44 == 2` |
| 4 | `$89D4` | `$44 = 1` (laser) | `$44 == 1` (`$89C0` path) |
| 5 | `$89D8` | `INC $45` (option) | `$45 >= 2` |
| 6 | `$899C` | `$46 = 5` (shield) | never |

The handler at `$899C` falls into `$89BA`, which clears `$42`; the sound id is
`$0E`. This matches `$EC18` calls in the handlers.

### Cursor: it is an attribute byte, not OAM

There is no sprite cursor. The only OAM element in the meter is the fixed bar:

* OAM slot 0 at `$0200`: `CE 6D 23 F8` — stored Y `$CE` (top 207), tile `$6D`,
  attributes `$23` (palette 3, behind background), X `$F8` (248).
  Source bytes `$8B11-$8B14`, copied to `$0200-$0203` by `$8B32-$8B3C`; it is
  written only while the palette phase `$1F != 0` and never animates.

The selection cursor is drawn by `$8A35-$8A50`:

1. `LDA #$1A; JSR $85ED` emits text record `$8775`: `23 F8 00 00 00 00 00 00 00 FE`,
   i.e. PPU address `$23F8` (nametable 0 attribute row 7) with seven zero bytes.
2. If `$42 != 0`, `A = 8 - $42`, `X = $0E - A` and `$55` is written into the
   VRAM queue at that index. `$0E` points just past the seven data bytes, so
   the patched byte is the nametable attribute byte `$23F8 + $42`.
3. `$55` (`01` in every 2-bit quadrant) selects background palette 1 for the
   32x32-pixel cell above meter row `$42`. Selection 0 leaves the row cleared.

The seven attribute bytes cover tile columns 0-3, 4-7, ..., 24-27. The meter
labels start at column 4 (`$2384`), so:

| `$42` | attribute byte | value | label cell |
| --- | --- | --- | --- |
| 0 | `$23F8` (all seven cleared) | `$00` | none |
| 1 | `$23F9` | `$55` | SPEED UP |
| 2 | `$23FA` | `$55` | MISSILE |
| 3 | `$23FB` | `$55` | DOUBLE |
| 4 | `$23FC` | `$55` | LASER |
| 5 | `$23FD` | `$55` | OPTION |
| 6 | `$23FE` | `$55` | SHIELD |

Measured with capsules forced at the ship position (`$0360=80`, `$0320=96`,
slot 9 = kind `$030C=1`, subtype `$010C=6`):

```sh
cargo xtask probe "$ROM" --frame 902 --press 180:start \
  --poke 900:0315:01 --poke 900:0115:06 \
  --poke 900:0375:50 --poke 900:0335:60 --nametables nt/sel1.bin
# sel6: repeat at 900,902,904,906,908,910 and probe 912
```

Observed `$23F8..$23FE`:

```
sel 1: 00 55 00 00 00 00 00 00
sel 2: 00 00 55 00 00 00 00 00
sel 3: 00 00 00 55 00 00 00 00
sel 4: 00 00 00 00 55 00 00 00
sel 5: 00 00 00 00 00 55 00 00
sel 6: 00 00 00 00 00 00 55 00
```

It does **not** blink: the same byte was still `$55` at frames 902, 940 and
1000 after one pickup at frame 900. The only writes are the pickup/activation
redraws, not a per-frame timer.

## Where power capsules come from

There is no capsule entry in the stage-event tables and no dedicated capsule
spawner. The capsule is the terminal state of object destruction debris:

1. `$BE8D` turns a destroyed object into kind 2 and picks a debris path in
   `$AE6B` from `$016C` (path 1 for ordinary enemies, `29 2A 2B 2C 00`);
   `$014C = 3`, `$042C = 0`.
2. `$AE93` walks that path every six frames. When it reads the `$00`
   terminator, `$AEB6` tests `$03AC`:
   * `$03AC == 0`: the slot is cleared (`$AEF2`) - no capsule;
   * `$03AC != 0`: `$AEBB` sets `$030C = 1` (kind 1) and `$010C = 7` when
     `($47 + 1) & $0F == 0`, otherwise `$010C = 6` (capsule).
3. Collision at `$C1A9`: kind low 7 bits = 1 and `$010C == 6` clears the slot
   (`$C1F7`) and calls `$8950`, advancing `$42`; sound `$36` is queued.
   Kind 1 with any other non-zero subtype goes to `$C186` instead: it is not a
   power capsule but the blue capsule. It clears its own slot (`$C1F7`) and
   plays sound `$0B`; the Y=9..0 loop then runs `$BE8D` on every object whose
   `$010C` bit 7 is clear, `$030C` bit 7 is set and low kind is at least 3.
   Those turn into kind-2 debris without scoring (`$BE8D` never calls `$8468`).
   `$47` is a global fold counter, so **1 in 16 folds is subtype 7**, the blue
   capsule that clears the screen.

Measured with a subtype-7 capsule forced at the ship (`$0360=80`, `$0320=96`,
slot 9 = kind `$030C=1`, subtype `$010C=7`) at frame 900 and a probe at 902:
the six kind-`$85` enemies on screen became kind 2, the score stayed 0, and
`--poke 900:0115:06` (the ordinary capsule) still scored 50.

`$03AC` is not a random chance. It comes from destruction (`$BE9F`) and the
spawner:

* At destruction, `$BE9F` reads `$03AC`: 0 stays 0, 1 stays 1, and 2/3
  decrement the class counter `$0048,class`; only the member that makes the
  counter zero is rewritten to 1.
* The spawner at `$A3E9` gives every member of a formation of **four or more**
  (`$A58C[2*formation] & $0F >= 4`, kind `$65 != $0B`) a phase of 2 or 3, and
  stores the class count in `$0048,phase`. So the **last member of each 4+
  formation** leaves a capsule; earlier members leave plain debris. An object
  that already carries `$03AC = 1` (the measured kind-`$84` example below)
  also leaves one; `$03AC = 0` never does.

Measured example, assisted run with held A and a small vertical dodge script
(`--press 180:start --press 600:up:80 ... --keep-alive`):

```sh
cargo xtask ramdump "$ROM" --watch 1769:1998 --press 180:start \
  --press 600:up:80 --press 800:down:80 --press 1000:up:80 \
  --press 1200:down:80 --press 1400:up:80 --press 1600:down:80 \
  --buttons a --assist --keep-alive --range 0314:0315
```

```
f01769  $0314  00 -> 84        slot 8: kind $84 (kind-4 arc enemy, sprite $21)
f01797  $0314  84 -> 02        shot down: kind 2 debris
f01821  $0314  02 -> 01        path end: kind 1
f01821  $0114  00 -> 06        ... and subtype 6 = capsule
f01997  $0114  06 -> 00        touched: slot cleared
f01997  $0314  01 -> 00
f01997  $0042  00 -> 01        $8950: meter selection advances
```

At `f01769` the same slot also carries `$03B4 = 1` (`$03AC` for slot 8), the
precondition for the fold. The object displayed sprite group `$21`
(`$901A`, tiles `$21/$23`, palette 2) while alive.

## Player death explosion

### Trigger and timer

`$C1D0` (enemy contact without a shield, reached from `$C241`, or a lethal
enemy shot) sets:

```
$4C = $78 (120)     death timer, decremented by the $1B = $A0 handler at $96E2
$0100 = 2           player state = dying
$0160 = 0           explosion frame index
$0140 = 0           explosion frame divider
$1B = $A0
$F7 -> $EC18        death sound
```

The ship's own sprite is replaced, not drawn separately. `$C0C1` runs every
frame (via `$C045` while `$5C < 2`) and, when `$0100 >= 2` and `$0140 == 0`:

```
$0140 = 10
X = $0160; $0160++
$0120 = $C0F4[X]
if $0120 == 0: $0121 = $0122 = $0140 = 0
$0140--
```

`$C0F4` is `2D 2E 2F 30 30 00`, so the explosion steps every **10 frames**,
five visible steps, then nothing:

| step | `$0120` | group | tiles (attrs) | frames |
| --- | --- | --- | --- | --- |
| 0 | `$2D` | `$906C` | `$49 $4B $4B $49` (`$03 $03 $C3 $C3`) | 10 |
| 1 | `$2E` | `$907D` | `$4D $4F $4F $4D` (`$03 $03 $C3 $C3`) | 10 |
| 2 | `$2F` | `$908E` | `$51 $53 $53 $51` (`$03 $03 $C3 $C3`) | 10 |
| 3 | `$30` | `$909F` | `$55 $57` (`$03 $03`) | 10 |
| 4 | `$30` | `$909F` | `$55 $57` (`$03 $03`) | 10 |
| 5 | `$00` | - | slot cleared, ship invisible | 1 |

Measured OAM (poke a lethal kind `$83` object at the ship on frame 900; probe
with `--oam` at 905, 921, 931, 941, 961):

```
905/911: $49/$4B at x 72..96, attr $03/$C3  ($2D)
921:     $4D/$4F                             ($2E)
931:     $51/$53                             ($2F)
941/951: $55/$57                             ($30)
961:     $0120 = 0, no ship/explosion sprites
```

Command used:

```sh
cargo xtask probe "$ROM" --frame 921 --press 180:start \
  --poke 900:0315:83 --poke 900:0115:00 \
  --poke 900:0375:50 --poke 900:0335:60 --oam oam/921.bin
```

The `$4C` timer is independent of the 50-frame explosion. Measured after a
frame-900 death:

```
901:  $0100=2 $4C=120 $1B=$A0 lives=3 score=000000 sprite=$01
905:  $0100=2 $4C=116 lives=3 score=000000 sprite=$2D
1019: $0100=2 $4C=2   lives=3 score=000000 sprite=$00
1021: $0100=2 $4C=0   lives=3 score=000000
1030: $0100=1 $4C=0   $1B=$04 lives=2 score=000000 sprite=$01  (respawned)
```

So death itself adds **no score**; the life is decremented when `$4C` reaches
zero (`$9790`: `DEC $20,X`, game over when it goes negative) and the respawn
runs the section intro from the `$3F & $0E` checkpoint captured at death time
(`$97A4`, clamped to 8). With a frame-900 death the observed life loss and
respawn happen at frame 1022.

Caveat: when `$19 == 4` (section 4) and the player has two or more options,
`$C0C1` is called from the alternating-frame path at `$9690` instead of
`$C045`, so the `$0140` divider counts calls: each visual step then lasts
about 20 wall frames.

### Port delta (not changed here)

`gradius-core` currently draws the player death with the heavy *enemy burst*
groups `$29-$2C` (`$9050/$9059/$9062/$9067`) for 20 frames. The ROM uses the
dedicated ship groups `$2D-$30` (`$906C/$907D/$908E/$909F`) for 50 frames, as
the OAM above shows. That mismatch is why the recorded ship explosion looks
wrong; it is a core change, outside this card.

## Empty boxes: what the meter hides once an upgrade is owned

Measured with `xtask probe` (stage 1, `--press 180:start`, state poked with
`--poke FRAME:ADDR:VALUE`) and with `gradius-diff --script`. The meter is
row 28 of the physical nametable dump (offset `$0384`), the status row is 29.

### Which boxes blank, and the condition

`$89E8` (RTS while `$0100 >= 2`) rewrites the meter row. For each box it picks
text record `$15..$1B`, or the empty frame `$19` when the condition holds
(`$89F5-$8A2F`):

| box (cols) | condition (RAM) | tiles when empty |
| --- | --- | --- |
| SPEED UP (4-7) | never | - |
| MISSILE (8-11) | `$41 != 0` | `1D 1E 1E 1F` |
| DOUBLE (12-15) | `$44 == 2` | `1D 1E 1E 1F` |
| LASER (16-19) | `$44 == 1` | `1D 1E 1E 1F` |
| OPTION (20-23) | `$45 >= 2` (3 also blank) | `1D 1E 1E 1F` |
| SHIELD (24-27) | `$46 != 0` | `1D 1E 1E 1F` (full: `1D 62 63 1F`) |

Measured at probe frame 910 with one poke at 900 each: `$41=1`, `$44=2`,
`$44=1`, `$45=2`, `$46=5` blank exactly their box; `$45=1` and `$40=$0E, $10,
$40, $FF` (speed at and past the `$A000` clamp) blank nothing. Double and laser
share `$44`, so only the owned one empties and the other stays labelled. Row
before: `09..1C 1D 62 63 1F`; after missile: `... 0C 1D 1E 1E 1F 11 ...`.

### When the screen follows

The redraw is periodic, not tied to the pickup or the activation. A poke at
the start of frame N shows at the first probe frame that is a multiple of 8
after it (poke 900-903 -> 904, poke 904-907 -> 912, 920 -> 928 for the
restore). The same redraw rewrites the highlight attribute byte from `$42`
(`$8A35`), so:

* Activation (frame 802 in `scripts/meter-2400.txt`, `$41` 1, `$42` 0 from
  802 on) leaves the box lit and still labelled until the next redraw (808);
  then both change in the same frame.
* A pickup lights its box one frame after `$42` changes (`$8A35` through the
  VRAM queue), not at the next redraw.
* The oracle samples the state of the previous frame: a capsule at 784 is
  not in the 784 redraw.
* The boxes are also redrawn in the stage-entry setup, so after death and
  respawn they are full again when the HUD unblanks (f1315), not eight
  frames later.
* Death (`$97D0`) clears the upgrades but **not `$42`**: it stayed `1` from
  f1275 through death at f1283 and respawn.

A selected box that is empty (selection 2 with `$41 = 1`, probe `--poke
900:41:01 --poke 900:42:02`, frame 920) keeps its `$55` attribute: tiles
`1D 1E 1E 1F` in palette 1.

### Four-option mod

`Mods::extra_options` raises the cap from 2 to 4 (`MAX_OPTIONS_EXTENDED`); the
OPTION box blanks at the cap in force (`$45 >= 2`, or `>= 4` with the mod).

### Port

`PowerMeter::empty_slots` gives the conditions; `sample_empty` copies the
previous frame's state when `nmi_frame & 7 == 4` (phase fitted against the
oracle: blank visible at f808 with activation at f802) and on every
`Init`/`StageIntro` frame. `shown_selection` is the highlight as drawn. Both
live in `PowerMeter` so the snapshot format is unchanged; after a snapshot
restore the latch holds until the next sample, at most eight frames.

### Pixel comparison

```sh
target/release/gradius-diff --rom "$GRADIUS_ROM" --pack assets/gradius.pak \
  --frames 2400 --script scripts/meter-2400.txt --keep-alive \
  --native-keep-alive --pixel-report
```

The meter row (y 212-219) matches in all 2400 frames: missile bought at f802
and emptied at f808, the owned-missile box selected at f1361, an option bought
at f1620, the shield box empty through keep-alive. Without keep-alive and with
`scripts/meter-2400.txt` cut at 1400 frames the ship dies at f1283 and the
refill after respawn matches too. A second option (box empty at the cap) needs
five more capsules; the next ones arrive after f10000 and the run diverges
before then, so that case is measured only with pokes (above) and covered by
core tests.
