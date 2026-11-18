# Big Core shot cadence: first divergence, ROM rule and fix plan

Measured 2026-11-09 on `agent/bosscad` at `2c872c7` (fast-forwarded to main),
release build in `/tmp/opencode/target-bosscad`, ROM `Gradius (E).nes`. The
first measurements were taken at `9a4f8ec`; the intervening main commits only
add the high-score HUD and stage-2..7 tooling and do not touch collision or
fire code, and the results below reproduce at `2c872c7`. No cartridge bytes
are stored here.

Reproduce the boss window:

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-bosscad cargo build --release -p gradius-diff
/tmp/opencode/target-bosscad/release/gradius-diff \
    --rom "$GRADIUS_ROM" --pack assets/gradius.pak \
    --script scripts/boss-script.txt --keep-alive --native-keep-alive \
    --frames 9800 --from 7500 --to 9700 --report \
    --oracle-out /tmp/opencode/bosscad/oracle.txt \
    --native-out /tmp/opencode/bosscad/native.txt
```

The run reports `score 1440 vs 1170`, `meter 2 vs 1`, object slot match
`4221/6804 (62.0%)`; the native side never damages the core.

## TL;DR

The drift is **not** a cooldown or fire-button-edge bug. It starts in
`Game::collide_shots`: the port rejects a shot whose Y probe is exactly
`ENEMY_BOX_HEIGHT` away from the object, while the cartridge accepts that
edge because its Y compare runs with the borrow from the X compare. The
first victim is object slot 9 at frame 631. The missed shot stays alive to
`X >= $F8`, which freezes its `$03A3`/`$03A6` cooldown for longer than the
cartridge, and every later fire time slips. By the core the fire phase is
arbitrary and the aimed shots miss the core's `$0460 = 3` box, which is only
**16 x 2 pixels** (`$BFD4[3] = $10`, `$BFD8[3] = $02`). That is why the core
never takes damage.

## First shot divergence: frame 631, shot slot 5 vs object slot 9

Oracle RAM after frame 631 (`xtask ramdump --watch 560:680 --range 0300:03B0`):

| byte | value |
| --- | --- |
| `$0126` | `06 -> 00` (shot 2 is consumed this frame) |
| `$0326` | `$38` = 56 (shot Y) |
| `$0366` | `$96` = 150 (shot X) |
| `$030C+9` | `$85` (object kind 133) |
| `$032C+9` | `$30` = 48 (object Y) |
| `$0460+9` | `$00` (16 x 16 box) |

The port has the same shot, from GDB at `collide_shots`, frame 631:

```text
shot[5]  active kind=0 x=150 y=56
object[9] kind=133 x=156 y=48 size_class=0
```

Both sides compute the same probe and X test:

```text
sx = 150 + $BFC8[0] ($08) = 158;  dx = 158 - 156 = 2 < $BFCC[0] ($10)  -> X pass
sy =  56 + $BFD0[0] ($08) =  64;  dy =  64 - 48 = 16
```

Cartridge `$BFDC` (`$C01D`-`$C025`):

```asm
C01A  BE 60 04   LDX $0460,Y       ; X pass left carry clear
C01D  A5 A1      LDA $A1           ; shot Y + $BFD0
C01F  F9 2C 03   SBC $032C,Y       ; subtract object Y AND the borrow
C022  DD D8 BF   CMP $BFD8,X       ; height = $10
C025  B0 03      BCS skip
```

Because the X test passed, carry is clear and `SBC` subtracts one extra:
the compared value is `64 - 48 - 1 = 15`, which is `< 16`, so the cartridge
**hits**. The port compares `dy = 16` and uses `if dy >= height { continue }`,
so it **drops** the hit. In ROM terms the accepted Y band is
`1 <= dy <= height`; the port accepts only `1 <= dy <= height - 1`.

This is the only difference at frame 631: the oracle and native object
dumps are byte-identical through frame 630, and the shot position/kind are
the same. The port also uses the full 16-bit Y subtract
(`(sy << 8) + shot.frac - ((object.y.int << 8) + object.y.frac)`) where
`$C01F` uses integer bytes only; for this shot the fractions are both zero,
so the off-by-one is the decisive part.

Note: the older `479 obj.substate / 498 x-y / 530 slot` divergences do not
reproduce on this commit; the first state-line difference is `score` at
`631` (oracle 40, native 30) caused by this missed shot.

## Cadence drift

`shots1`/`shots2` are `$0123`/`$0126` `00 -> non-zero` spawns; `first` is
the first spawn frame in the bucket, oracle/native. The native list comes
from a breakpoint on `Game::spawn_shot`:

```sh
gdb -batch -ex 'break gradius_core::game::Game::spawn_shot' \
    -ex 'commands' -ex 'silent' \
    -ex 'printf "SPAWN frame=%llu emitter=%llu which=%llu\n", self.frame, emitter, which' \
    -ex 'continue' -ex 'end' -ex run -ex quit \
    --args /tmp/opencode/target-bosscad-debug/debug/gradius-diff \
    --rom "$GRADIUS_ROM" --pack assets/gradius.pak \
    --script scripts/boss-script.txt --keep-alive --native-keep-alive \
    --frames 9200 --from 7500 --to 9100
```

430-830 (from the same dumps):

| frames | shots1 O/N | first1 O/N | shots2 O/N | first2 O/N | score diff | slot pairs | live delta |
| --- | --- | --- | --- | --- | ---: | --- | ---: |
| 430-529 | 2/2 | 469/469 | 2/2 | 446/446 | +0 | 7/7 | 0 |
| 530-629 | 2/2 | 555/555 | 3/3 | 534/534 | +0 | 5/5 | 0 |
| 630-729 | 3/2 | 643/643 | 2/2 | 664/666 | +10 | 4/4 | 0 |
| 730-829 | 1/3 | 773/731 | 2/2 | 752/754 | +0 | 4/4 | 0 |

The first fire-time slip is therefore **shot 2 at frame 664 (oracle) vs 666
(native)**. The shot from frame 622 should have died on slot 9 at 631; after
the miss it flies to `X >= $F8`, so `$0126` stays non-zero two frames longer,
`$03A6` is frozen two frames longer, and the next shot 2 is two frames late.
Once a miss consumes a slot to the screen edge, the period grows from ~42
frames (hit-and-recycle) to ~53 frames (edge-to-recycle); that is how a
one-frame collision error accumulates into the boss-window phase.

7500-9100 (boss spawn at 7723/7724, first oracle damage 8066, oracle kill 9087):

| frames | shots1 O/N | first1 O/N | shots2 O/N | first2 O/N | score diff | meter diff | slot pairs |
| --- | --- | --- | --- | --- | ---: | ---: | --- |
| 7500-7699 | 4/3 | 7516/7546 | 4/4 | 7537/7521 | +270 | +1 | 0/4 |
| 7700-7899 | 4/4 | 7728/7705 | 4/4 | 7749/7733 | +270 | +1 | 3/4 |
| 7900-8099 | 5/4 | 7900/7917 | 4/4 | 7921/7938 | +320 | +1 | 3/3 |
| 8100-8299 | 4/4 | 8132/8129 | 5/4 | 8111/8150 | +320 | +1 | 3/3 |
| 8300-8499 | 5/4 | 8322/8319 | 4/4 | 8343/8340 | +420 | +1 | 3/3 |
| 8500-8699 | 4/4 | 8532/8509 | 5/4 | 8511/8530 | +470 | +1 | 3/3 |
| 8700-8899 | 5/4 | 8700/8710 | 4/4 | 8721/8742 | +470 | +1 | 3/3 |
| 8900-9099 | 5/4 | 8921/8911 | 5/4 | 8900/8943 | +1520 | +1 | 1/3 |

The counters show the mechanism: the oracle recycles shots on enemies
(more spawns per 200 frames); the port mostly runs shots to the edge, so it
fires fewer and its spawn phase wanders by tens of frames. `score` and
`meter` are downstream: missed kills keep the port 27 kills behind by 7500,
and the power cursor collects a different capsule sequence. The core itself
spawns at 7723 on the oracle and 7724 on the port, one frame late; with the
open-loop boss aim in `scripts/boss-script.txt` that alone shifts every
aimed shot, and the core's 16 x 2 hit box makes the miss total.

## Cartridge shot-slot rule

The cartridge caps options at 2 (`$89DA CMP #$02`), so there are three
emitters (ship + two options). Player shots live in one 9-entry table,
three entries per emitter:

| entry | address | meaning |
| --- | --- | --- |
| shot 1, emitter `e` | `$0123+e`, `$0163+e`, `$0363+e`, `$0323+e` | sprite/active, kind, X, Y |
| shot 2, emitter `e` | `$0126+e` = shot1 `e+3` | same arrays, index `e+3` |
| missile, emitter `e` | `$0129+e` = shot1 `e+6` | same arrays, index `e+6` |

`$A0E3` fire loop (`$A102`-`$A167`), for `X = $45` down to 0:

```
if $0123,X == 0:                       ; slot free?
    if $9A == 0 and $03A3,X != 0: DEC $03A3,X
    else: spawn shot1; $03A3,X = $35 ($14)
          if $44 != 2: $03A6,X = $35 and jump to missile
if $0126,X == 0:
    if $9A == 0 and $03A6,X != 0: DEC $03A6,X
    else: spawn shot2; $03A6,X = $35
          if $44 != 2: $03A3,X = $35; DEC $03A6,X  ; ends at 19
if $41 != 0 and $0129,X == 0 and $9B != 0: spawn missile
```

Key lifetime point: the `DEC` is **inside** the free-slot test. A live shot
freezes its own cooldown, so the shot's on-screen lifetime (hit or `X >= $F8`)
is part of the cadence. A/D held with a zero cooldown fires without a new
edge. Normal kind 0 moves +7 px/frame and dies at `X >= $F8`; laser kind 1
+12 and `$F0`; double kind 2 uses +4 with a Y correction. Missiles are
`$A265`, kind 3, one per emitter while A is held.

**Port capacity does not match the cartridge.** `Game` uses
`EMITTERS = 5` and `MAX_SHOTS = 5 * 3 = 15`
(`$A102: LDX $45` was read as four options + ship), and
`shot_slot(e, which) = which * 5 + e`. The cartridge table is 9 entries
(`which * 3 + e`). With `Mods::extra_options` off the port only uses emitter
0-2, so the larger pool is not the cause of this run's drift, but it is not
ROM parity: enabling the extended loadout would allocate slots that the
cartridge does not have and change cooldown aliasing. For parity use
`EMITTERS = 3`, `MAX_SHOTS = 9`, stride 3; keep the five-emitter pool only
behind `extra_options` with its own explicit size.

## Fix plan

1. **`gradius-core/src/game.rs::collide_shots` (primary).** Mirror `$C01D`:
   use the integer bytes and the borrow semantics.
   - `let dy = (shot.y.int.wrapping_add(SHOT_BOX_Y[kind])).wrapping_sub(object.y.int);`
   - skip when `dy == 0 || dy > ENEMY_BOX_HEIGHT[size]` (accept `dy == height`).
   - drop the `shot.y.frac`/`object.y.frac` terms for this player-shot path;
     `$C01F` subtracts `$032C,Y` only.
   - Add tests: object y=48, shot y=56, size class 0 must hit (`dy == 16`);
     `dy == 17` and `dy == 0` must miss.
2. **Boss spawn latency.** The core appears at 7724 instead of 7723. Check
   `normal_frame`'s `boss_stage == 3` spawn before the `boss_stage == 2`
   transition sets stage 3; spawn on the transition frame as the ROM's
   `$83/$84` handler does, so object slot 9 and the aim lead line up.
3. **Shot pool parity.** Change `EMITTERS`/`MAX_SHOTS`/`shot_slot` to the
   9-entry, 3-emitter table above; gate the 15-slot layout behind the
   extended-options mod.
4. **Re-run.** `scripts/longrun.sh` boss case should match score/meter/slot
   pass rates, and `record-boss`/`gradius-diff --from 7500 --to 9700` should
   show damage from 8066 and a clear at 9087. If the 1-frame core spawn is
   fixed but damage still fails, compare the shot Y within ±1 of the core's
   `$032C` (16 x 2 box) before changing aim logic.
5. **Follow-up only if needed.** The port runs `update_player` before
   `update_objects`; the ROM runs `$BBB1` (objects) before `$9FF6` (player).
   State lines match exactly through frame 630, so this is not the cause
   here; keep it out of the first fix.

No `gradius-core` files were modified by this analysis. The only repository
change is this document.
