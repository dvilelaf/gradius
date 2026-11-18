# Terrain collision and power-up verification

Measured 2026-11-09 on `agent/drift` against `Gradius (E).nes`. The ROM was
read in place; no cartridge bytes are stored here.

## 1. Ship vs. background map

**Decision: the original kills. `Mods::map_terrain_collision` has been
removed and the ship now always tests `$C39D`.**

### Oracle code path

`$C29F` runs the player collision pass. For stage 1 (`$19 = 0`) it reaches:

```text
$C2AF  LDA $0100
$C2B2  CMP #$02
$C2B4  BCS $C2BE        ; already dying
$C2B6  JSR $C39D        ; player point -> $C3CD -> ($0500/$0600)
$C2B9  BEQ $C2BE        ; map byte 0 -> no hit
$C2BB  JMP $C1D0        ; non-zero -> death, no shield check
```

`$C39D` copies `$0320` (ship Y) and `$0360` (ship X) to `$A5`/`$A4`, then
`$C3CD` probes `(x + 8 + $3E) & $F8` and `(y + $14) >> 3` in the page selected
by `($3F + carry) & 1 + 5`. For `$19 == 4` the handler returns early; stage 1
does not.

### Evidence that the page is not always zero

Earlier notes in `xtask screens` claimed the `$0500`/`$0600` pages stay zero
"to frame 20000". That measurement only covered runs that die and restart
before the late stage. In a surviving run the pages are populated:

```text
# gradius-diff --script scripts/boss-script.txt --keep-alive, frame 6003
frame=6003 ... scroll=37:11:128
$0500 50 00 00 00 00 00 50 00 50 ...
$0520 50 00 00 00 00 00 54 00 50 ...
```

`$0526 = $54`, i.e. group 6 of column 388 is solid for row 25. The probe is
exactly the one `$C39D` reads at scroll `$0B:25`.

### Oracle poke test

Set the map byte and the ship position, then run one frame with Down held:

```sh
xtask probe "$ROM" --frame 303 --press 180:start --buttons down \
  --poke 300:003F:0B --poke 300:003E:25 \
  --poke 300:0360:F7 --poke 300:0320:C0 \
  --poke 300:0526:30 --ram /tmp/ram.bin
# $0100 = 02 (dying), $4B = $76, $1B = $A0, $46 = 00
```

The ship dies on the frame the map byte is non-zero, before any shield test.
Port parity is now the `ship_vs_map_collision_is_rom_parity` unit test (no
mod), and `collide()` always calls `collide_player_terrain()`.

## 2. Power-ups, capsule -> selection -> B

Oracle matrix, measured by poking `$42` at frame 300 and pressing B once:

```sh
xtask probe "$ROM" --frame 303 --press 180:start \
  --poke 300:0042:0N --press 300:b --ram /tmp/ram.bin
```

| `$42` | item | handler | oracle effect | port result / test |
| ---: | --- | --- | --- | --- |
| 1 | SPEED | `$89A6` | `$40 += 1` (repeatable) | `capsule_advances_the_selection_and_press_b_applies_speed` |
| 2 | MISSILE | `$89B4` | `$41 = 1`; once (already 1 is a no-op) | `missile_is_a_one_shot_upgrade` |
| 3 | DOUBLE | `$89C0` | `$44 = 2` | `double_and_laser_alternate` |
| 4 | LASER | `$89D4` | `$44 = 1` (alternates with DOUBLE) | `double_and_laser_alternate` |
| 5 | OPTION | `$89D8` | `$45 += 1`, cap 2 | `options_stop_at_two` |
| 6 | SHIELD | `$899C` | `$46 = 5`; once | `shield_is_a_one_shot_five_hit_upgrade` |

Oracle probe output (one activation each):

```text
slot 1: $40=1   slot 2: $41=1   slot 3: $44=2
slot 4: $44=1   slot 5: $45=1   slot 6: $46=5
```

Every applied handler clears `$42` to 0. A capped/already-owned item leaves
`$42` alone (`pmis sel=2`, `popt sel=5`, `pshield sel=6`).

### Port behaviour

No port no-op was found: all six effects are already implemented in
`PowerMeter::activate`, and the new `Game::step` integration tests above drive
capsule pickup, selection, the one-frame input latch and the B edge end to end.
B is edge-triggered (`power_pressed = control.power && !prev_power`), so
holding B cannot re-activate a fresh selection; the tests hold B after an
activation and assert the state is unchanged.

The only documented difference is the HUD capsule counter: the oracle keeps no
`capsules` counter, so the port's `capsule=` field is not compared by the
differential harness.

## 3. Natural-run confirmation and stage-2 comparison

`agent/powerqa` reproduced the map collision without poking the map: with the
ship held against the floor or ceiling, a surviving run reaches the populated
map and dies. `--shield` is an analysis-only hook added for this check: it
writes `$46 = 5` after every frame, so enemy, bullet and option contact only
decrements the shield (`$C241`, `$C1B7`, `$C286`), while the `$C2B6` map probe
calls `$C1D0` directly and ignores `$46`. A death in a `--shield` run is
therefore the map probe; `$0100/$4C/$1B` and the normal life-loss path run.

The stage-2 runs poke `$26 = 1` at frame 263, the one-shot stage-entry frame
from `stage2.md`.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-powerqa

# Stage 1 floor: hit at 2169, one frame after: $0100=02 $4C=$78 $1B=$A0 lives 3
cargo xtask probe "$ROM" --frame 2170 --press 180:start --buttons down \
  --shield --ram /tmp/s1dn.bin
# 120 frames later: lives 2, respawn at checkpoint $0200
cargo xtask probe "$ROM" --frame 2290 --press 180:start --buttons down \
  --shield --ram /tmp/s1dn2.bin

# Stage 1 ceiling: hit at 2745
cargo xtask probe "$ROM" --frame 2746 --press 180:start --buttons up \
  --shield --ram /tmp/s1up.bin

# Stage 2 floor (hit at 2553) and ceiling (hit at 2617)
cargo xtask probe "$ROM" --frame 2555 --press 180:start --poke 263:26:01 \
  --buttons down --shield --ram /tmp/s2dn.bin
cargo xtask probe "$ROM" --frame 2619 --press 180:start --poke 263:26:01 \
  --buttons up --shield --ram /tmp/s2up.bin
```

| stage | contact | hit frame | scroll | ship | map read one frame earlier | after 120 frames |
| --- | --- | ---: | --- | --- | --- | --- |
| 1 | floor | 2169 | `$03A8` | 80,192 | `$0506=$50`, mask `$30` -> `$10` | lives 2, checkpoint `$0200` |
| 1 | ceiling | 2745 | `$04C8` | 80,16 | `$0621=$01`, mask `$03` -> `$01` | lives 2, checkpoint `$0400` |
| 2 | floor | 2553 | `$0468` | 80,192 | `$05C6=$50`, mask `$30` -> `$10` | lives 2, checkpoint `$0400` |
| 2 | ceiling | 2617 | `$0488` | 80,16 | `$05E1=$01`, mask `$03` -> `$01` | lives 2, checkpoint `$0400` |

The `$0500/$0600` pages are zero only until the volcano loads: the first writes
are at frame **1579** (`$0500 = $50`), so the opening starfield cannot kill.
This extends the "not always zero" finding above with the exact load frame.
For stage 2 the pages are non-zero by frame 1600 (scroll `$028C`).

A death while hugging the ceiling *before* the map loads (frame 1059) is an
enemy: object slot 9 carried kind `$88` at X 83 / Y 16, spawn type `$88`
(record 8, `05 04 03 07`), not a map hit.

## 4. Second purchase, OAM and score

Method: `$42` is poked at frame 700 and again at 740, each with a six-frame B
hold. A successful handler clears `$42`; a no-op `RTS` leaves it on the row.
Second-purchase observations (`agent/powerqa`, `CARGO_TARGET_DIR` as above):

| item | first purchase | second purchase | class |
| --- | --- | --- | --- |
| SPEED UP | `$40 0 -> 1` | `$40 -> 2`, `$42 = 0` | repeatable, no cap in the handler |
| MISSILE | `$41 0 -> 1` | no-op, `$41 = 1`, `$42` stays 2 | one-shot |
| DOUBLE | `$44 0 -> 2` | no-op when already 2, `$42` stays 3 | mutually exclusive with LASER |
| LASER | `$44 0 -> 1` | no-op when already 1, `$42` stays 4 | mutually exclusive with DOUBLE |
| OPTION | `$45 0 -> 1` | `$45 -> 2`; third purchase no-op, `$42` stays 5 | repeatable, capped at 2 |
| SHIELD | `$46 0 -> 5` | no-op while `$46 != 0`, `$42` stays 6 | one charge per empty shield |

The shield row above refines "once": `$899C` is `LDA $46 / BNE $8988 / LDA #5 /
STA $46`, so the second press is a no-op only while the timer is non-zero.
Poking `$46 = 0` at the second press makes it succeed (`$46 = 5`, `$42 = 0`).
Likewise `$89C0`/`$89D4` only no-op when the requested mode already matches:
LASER then DOUBLE leaves `$44 = 2`, and DOUBLE then LASER leaves `$44 = 1`.

Command for the two-purchase runs:

```sh
cargo xtask probe "$ROM" --frame 746 --press 180:start \
  --press 700:b:6 --press 740:b:6 \
  --poke 700:42:0N --poke 740:42:0N --ram /tmp/pw.bin
```

No purchase changes score (`$07E4-$07E6 = 000000`) or lives (3), and `$0100`
stays 1. Standby OAM at frame 706 is identical for SPEED, MISSILE, DOUBLE and
LASER (FNV-1a `57c51ce0ded8e68c66554c49e91fda9d`); those weapons only appear
when firing. OPTION adds tile `$59` option sprites (slots 41/56, X 72/80,
Y 96 for one option). SHIELD adds the ring sprite tile `$C3` (slot 56, X 95,
Y 95, attribute `$21`; it animates to `$C1` and blinks).

Firing once (A at frame 708, read at 712) shows the slot fields:

| item | fields |
| --- | --- |
| MISSILE | `$0129[6] = $0A`, `$0163[6] = 3`, object at X 81 / Y 108 |
| DOUBLE | `$0123[3] = $24`, `$0163[3] = 2`, object at X 92 / Y 84 |
| LASER | `$0123[0] = 7`, `$0163[0] = 1`, shot at X 116 |
