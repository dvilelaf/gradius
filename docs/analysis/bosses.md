# Bosses of stages 1-7

Stage 1's Big Core is documented in `enemy_ai.md`. This note maps the same
end-of-stage machinery across the other stages and records which stages reach
it, which reach it late because of a scripted hazard, and which never spawn it.
The per-kind object table and the stage-stream inventory are in
`stage_enemies.md`.

Everything below was measured on `Gradius (E).nes` with `xtask record-boss
--stage N` and checked against the disassembly. Frame numbers depend on the
scripted run (they include the title, the stage intro and any checkpoint
replay), but the state-machine sequence and the ROM addresses do not.

## TL;DR

* Stages **1-5** end with the shared **Big Core**: kind `$98`, slot 9, spawned
  at `$9975` after `$3F` passes `$9A30[$19]`.
* Stage **3** additionally spawns the only other boss-class object, kind `$96`
  (`$C900`), from the `$A460` command path. It is a 3-hit object with a
  proximity-triggered ring/particle attack and is not the end-of-stage core.
* Stages **6 and 7 do not spawn a Big Core**. `$99B3` sets `$1B = $86`
  directly for `$19 >= 5`, skipping `$9975`; the stage ends on scroll at
  `$98F0`. Stage 6's `$C6D8` script is a collision-beam hazard; stage 7's
  `$C423` script is a bare `RTS`.

## Trigger and spawn

`$A2BA` / `$9A40` run the normal stage handler. When `$3F` reaches the stage's
boss threshold `$9A30[$19]`, the state machine leaves state 0:

```
$9A40: if $3F >= $9A30[$19] -> $1B = $9A38[$19] = $81
$9A01: $4D = $9A28[$17], $4C = 0, $1B = $82        ; clear timer
$99DC: decrement $4C:$4D each frame, then $1B = $83
$99B3: $1B = $84 (stages 1-4) or $86 (stages 6-7)
$9975: while $3F == $9A30[$19] : cleanup, stay in state
       when $3F != $9A30[$19] : spawn the core, $1B = $85
$9971: hold the scroll ($5B != 0), keep the core live
```

The spawn guard is exact: the core appears on the frame `$3F` becomes
`$9A30[$19] + 1`. The handler writes slot 9:

```
$9990  JSR $A521          ; clear slot 9
$9993  LDA #$98; STA $0315 ; kind $98
$9998  LDA #$80; STA $0335 ; Y = 128
$999D  LDA #$F0; STA $0375 ; X = 240
```

| stage `$19` | `$9A30` threshold | spawn `$3F` | `$98F0` end | `$C433` handler |
| --- | --- | --- | --- | --- |
| 1 (0) | `$0C` | `$0D` | `$0E` | `$C480` |
| 2 (1) | `$0C` | `$0D` | `$0E` | `$C540` |
| 3 (2) | `$0C` | `$0D` | `$0E` | `$C680` |
| 4 (3) | `$0C` | `$0D` | `$0E` | `$C5A7` |
| 5 (4) | `$0B` | `$0C` | `$0D` | `$C64D` |
| 6 (5) | `$0B` | **skipped** | `$0C` | `$C6D8` |
| 7 (6) | `$0C` | **skipped** | `$0D` | `$C423` |

`$99B3` is the skip:

```
$99B5  LDA $19
$99B7  CMP #$05
$99B9  BCC $99C6          ; stages 1-4: $1B = $84, then $9975
$99BB  D0 05             ; stage 7: $1B = $86
$99BD  LDA #$AC; JSR $EC18 ; stage 6: request the clear cue
$99C2  LDA #$86; STA $1B
```

## Stage x boss table (measured)

`spawn`/`x stop`/`first damage`/`death` are frames from the `record-boss
--stage N` runs. `x stop` is the first frame the core's `$036C` reaches 164.
`max` is the highest live `$046C`; the next hit clears the core. Stages 6 and
7 have no core, so their row is the measured stage transition.

| stage | type | spawn | scroll stop | x stop | first damage | max | overheat wraps 1/2/3/4 | end | reward | next stage from |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: |
| 1 | `$98` Big Core | 7723 | 7722 | 7798 | 8066 | 6/7 | 8058/8317/8576/8835 | 9087 | +1000 | 9601 |
| 2 | `$98` Big Core | 7979 | 7978 | 8054 | 8544 | 6/7 | 8314/8574/8833/9093 | 9280 | +1000 | 9794 |
| 3 | `$98` Big Core | 7979 | 7978 | 8054 | 8390 | 6/7 | 8314/8574/8833/9093 | 9126 | +1000 | 9639 |
| 4 | `$98` Big Core | 7979 | 7978 | 8054 | 8482 | 6/7 | 8314/8574/8833/9093 | 9218 | +1000 | 9732 |
| 5 | `$98` Big Core | 10274 | 10273 | 10349 | 10753 | 6/7 | 10609/10869/11128/11388 | 11489 | +1000 | 12003 |
| 6 | none (scroll end) | - | - | - | - | - | - | 7569 | - | 7569 |
| 7 | none (loop) | - | - | - | - | - | - | 7025 | - | 7025 |

Stage 5's late spawn is the measured `$C64D` big-object script slowing the
scroll between `$3F = $0B` and `$3F = $0C` (the threshold-to-spawn gap takes
about 1750 frames instead of about 512). The core itself is identical.

The spawn scores at the measured runs were 1440 (1), 930 (2), 5400 (3), 1600
(4) and 700 (5); the reward is always +1000 on the damage kill. A
self-destruct (overheat, no hits) clears the stage but awards no +1000; the
stage-1 no-hit run self-destructs at frame 9354.

## The Big Core (`$B90E`, kind `$98`)

### Entry and movement

* Entry: X 240 -> 164 at one px/frame (`$B9A5`), Y starts 128. Entry takes 76
  frames in every measured run.
* After entry, X tracks the player to the right (`$BA0C`, +2 px/frame only
  when the player is at or past the core); the player is normally left of it,
  so the core stays at 164.
* Vertical: every `$B904[rank]` frames the core picks a direction from the
  player's Y (`$BA12`) and moves by `$B8F2[rank]`/`$B8FB[rank]` per frame
  (`$B8F2` fraction, `$B8FB` integer), clamped to `$18..$A8` (24..168).
* `rank = $17` is built at `$9C3F` from weapon level, options, shield and
  stage. The three rank tables are nine entries each, indexed by `$17`: the fire period
at `$B904` holds 2 for rank 0 and then falls from 90 frames to 35; the fraction
at `$B8F2` steps up by `$20` from 0 to `$F0`; the integer at `$B8FB` is 0 at
rank 0 and 1 for every other rank. `gradius-extract` reads all three from the
cartridge.

### Attack, barriers, overheat

* Every fire period the core spawns four collidable laser barrels in slots
  3..0 at offsets (`$BAF1`, indexed by slot):

```
slot 0: (+8,-15)   slot 1: (-8,-2)   slot 2: (-8,+10)   slot 3: (+8,+23)
```

  They get `$0136 = $41`, `$0176 = 2`, `$0436 = $BAF9[rank]` and
  `$0456 = $BB01[rank]`. They are hazards, not damageable parts.
* The two cover pods are created by the core at `$B9B1` and re-attached every
  frame by `$BA65`. They are slots 8 and 7, kind `$99`, sprite `$80`, Y =
  core Y and core Y - 20. Kind `$99`'s handler is `$B90D` = `RTS`, so the
  pods are invulnerable. The core is scanned first in `$BFDC` (slot 9 down),
  so a shot on a pod's line still tests the core.
* `$046C` is the armour counter. Player shots add 1 or 2; the core dies at
  `$046C >= 7` (`$B92C`). The recorded `max 6/7` is the last live line: the
  seventh hit clears the core in the same frame.
* `$048C` is the damage gate: hits are ignored while `$048C = 0`. It toggles
  on at `$04CC == 1` and off at `$04CC >= 5`, so the core is only vulnerable
  during overheat wraps 1-4.
* `$04AC` counts frames; each 256-frame wrap increments `$04CC`. At `$04CC >=
  6` the core self-destructs (`$BA96 -> $B97D`): 6 x 256 = 1536 frames after
  the counter starts. No +1000 is awarded on that path.
* Death by damage (`$B95C`) awards `$845A` with A = `$10`, i.e. **+1000**,
  increments `$3B`, plays `$AC` through `$CB20`, clears slots 9/8/7, and if
  the player is alive (`$0100 < 2`) increments `$1B`. `$98F7` then waits for
  `$3F >= $98F0[$19]`; `$96C4 INC $19` (normal) or `$992E` (`$39` branch)
  advances the stage. Stage 7 uses `$9865` to wrap to stage 1 and increment
  `$28`.

## Per-stage script

`$C433[$19]` is the per-stage scripted-hazard handler, called every 4th frame
from `$C40D`. `$C424` sends the boss-clear window `$3A != 0` to `$C680`
regardless of stage; that is the bonus-object wave, not the boss.

| stage | handler | script / observed route |
| --- | --- | --- |
| 1 | `$C480` | no 5-byte commands; core from the state machine |
| 2 | `$C540` | every 8th frame spawns kind `$0B` from `$C567`; `$B95C` sets `$39` only when the core dies at `$04CC == 1 && $04AC < $78` (quick-kill route: +500, `$8E`, scroll to `$3F = $11`) |
| 3 | `$C680` | `$A460` command path (`$19 == 2`) spawns kind `$96` at X = `$F0`; `$C900` is a 3-hit object with a proximity-triggered ring/particle attack. `$C776` sets `$39` after 10 explosion frames. The end-of-stage core is still `$98` |
| 4 | `$C5A7` | same shape as `$C480`, own table `$C5FD` |
| 5 | `$C64D` | every `$28` frames spawns big object kind `$14` from `$C674` via `$A4A0`; `$C76C` is the low-option trail hook, not a boss. Threshold `$0B`; the script slows the scroll, so the core spawns at `$3F = $0C` late |
| 6 | `$C6D8` | every 4th frame fills a free shot slot with a collision-only beam (`$0136 = $8D`, X = `$98`, Y from `$C74A`). No `$98` object is ever created; `$99B3` skips `$9975` |
| 7 | `$C423` | bare `RTS`: no scripted hazards. `$99B3` skips `$9975`; stage end goes through `$9865` |

The stage-6 skip is visible frame by frame in `record-boss --stage 6`: `$3F`
runs 0..`$0B`, then `$99B3` sets `$1B = $86`, the scroll continues to `$98F0
= $0C`, and `$19` goes 5 -> 6 at frame 7569 with no kind-`$98` object ever in
slots 0-9. Stage 7 is the same with `$19` 6 -> 0 at frame 7025.

## Reproduce without a full playthrough

`record-boss` now takes `--stage N` (1-7). It pokes the single-player stage
copy `$26` every frame until `$00 == 5 && $19 == N - 1`, exactly like
`screens --stage`, then keeps `$46 = 5` and restores `$0100/$4C/$1B` on the
lethal-death path (`--keep-alive` behaviour). No full playthrough is needed.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-boss

# stages 1-5: spawn, damage, death and next stage
cargo xtask record-boss "$ROM" --stage 2 --frames 12000 --out /tmp/boss2.txt
# stages 6-7: no core; succeeds when the stage transition is observed
cargo xtask record-boss "$ROM" --stage 6 --frames 13000 --out /tmp/boss6.txt

# one-shot stage poke (264 is the stage-init frame in the reference run)
cargo xtask probe "$ROM" --frame 500 --press 180:start \
  --poke 263:26:01 --keep-alive --ram /tmp/stage2-ram.bin
```

The run writes one `BossLine` per frame: the native `Game::dump_state` line
plus `boss=<slot>:<x>:<y>:<damage>:<overheat>:<counter>` (`boss=-` when no
core is live). `--stage` is 1-based; the poke is `$19 = stage - 1`.

## Porting checklist

1. Select the boss threshold from `STAGES[stage].boss_threshold` (`$9A30`),
   not the hardcoded `$0C`.
2. Run the `$81 -> $82 -> $83 -> $84 -> $85` state sequence for stages 1-5.
   On entry to `$84`, wait until `$3F != threshold`, then spawn slot 9 kind
   `$98` at X 240, Y 128.
3. For stages 6-7, enter `$86` directly at `$99B3` and do **not** spawn a
   core; end at `$98F0`.
4. Spawn the pods (slot 8 at core Y, slot 7 at core Y-20) as invulnerable
   kind `$99`; they are presentation/collision only.
5. Model the 2 px core box (`$0460 = 3`, `$BFD8[3] = 2`) and the accepted
   shot line `shot_y + 8 - core_y` in 1..=2 (the X compare's borrow makes the
   cartridge accept dy 1..2).
6. Keep the `$048C` damage gate and the 7-point armour; do not award points
   on overheat self-destruct, award +1000 on the damage kill.
7. Carry the stage-specific routes: stage 2 `$39` quick-kill, stage 3 `$96`
   `$C900` mini-boss and its `$39` explosion path, stage 5 big-object script
   and delayed spawn, stage 6 beams, stage 7 loop through `$9865`.
8. Use `$B904/$B8F2/$B8FB` for the per-rank fire period and vertical speed,
   and `$BAF1/$BAF9/$BB01` for the four-barrel offsets.
