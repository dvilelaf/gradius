# The loop restart after the ending (`$9865` → stage 1, `$28 = 1`)

Measured window **8254-9000** on `Gradius (E).nes`, one frame at a time.
**FIXED on `main` (`d458baf`):** the 27-frame stage-1 boot after `$8D` is
modelled. It corrects the `stage67_boot.md` summary in one place: the ending
does **not** hand over to play at 8282 with no transition -- `$8D` runs the
whole 27-frame stage-1 entry sequence (`$1B = $01..$04`) first.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-s7loop CARGO_BUILD_JOBS=2
cargo build --release -p gradius-diff
xtask record-ending "$ROM" --stage 7 --frames 9100 --out /tmp/opencode/s7loop-oracle.txt
# $1C/$17/$2A/$28 are not in the record line: dump RAM at the boundary frames
pokes=$(for f in $(seq 170 320); do printf -- '--poke %d:26:6 ' $f; done)
xtask probe "$ROM" --frame 8282 --press 180:start --keep-alive $pokes \
  --ram /tmp/opencode/ram-8282.bin --png /tmp/opencode/f8282.png   # also 8254/8300/8500/9000
```

## The restart, frame by frame

`$19` is already 0 on the `$8D` frame (the ending's `$9B38` copied `$26`), so
the stage-1 entry only re-runs the boot, it does not reload a stage.

| frame | `$1B` | `$1C` | what happens / what is drawn |
| ---: | --- | --- | --- |
| 8253 | `8C` | `A5` | last `$8C` frame; `CONGRATULATIONS!` + HUD, starfield |
| 8254 | `8D` | `A5` | `$98D8: LDA #0; STA $1B; JMP $9B38`; screen still the ending picture, score 10830, lives 4 |
| 8255-8256 | `01` | `A5` | stage-1 entry: scroll 0, ship `(80, 96)`, screen blanks during this window (black by 8282) |
| 8257 | `02` | `A5` | entry step |
| 8258 | `03` | `A5` | entry step |
| 8259-8281 | `04` | `A5` | 23-frame `$04` hold, screen black, scroll still `0:0:0` |
| 8282 | `80` | `A5` | normal play, still black; `$28 = 1`, score 10830, lives 4, `$17 = 1`, `$2A = $02` |
| 8287 | `80` | `A5` | first scroll step (`$3E` fraction 128), 5 frames after playable |
| 8290 | `80` | `93` | stage-1 intro cue replaces the ending's `$A5` |
| 8300 | `80` | `93` | starfield, no tiles yet |
| 8500 | `80` | `93` | stage-1 playfield with enemies, scroll 0:107 |
| 9000 | `80` | `93` | scroll 1:101, score 11010 (kills), lives 4, loop 1 |

Counts: `$8D` + `$01..$04` = **28 frames** (8254-8281) between the ending
machine and playable stage 1; +5 frames to the first scroll step (8287).
There is **no intermediate screen**: the ending picture stays up through
`$8D`, the stage-1 boot blanks the PPU, and the black frame at 8282 is the
last blank before the looped stage-1 nametable appears. Music does **not**
restart with the boot: `$A5` (the boss track the ending reuses) survives
`$8D` and `$01..$04`: `$1C` is `$A5` through 8282 and `$93` from 8283.

## What is kept and what is rebuilt

| byte | 8254 (`$8D`) | 8282 (playable) | 9000 | note |
| --- | --- | --- | --- | --- |
| `$19` | 0 | 0 | 0 | `$9B38` copied `$26`, the boot never leaves stage 1 |
| `$1A`/`$28` | 1 | 1 | 1 | loop counter set by `$9865` at 7024, untouched by the restart |
| `$17` | 1 | 1 | 1 | rank rebuilt from the live power state (`$9C3F`), not reset |
| `$2A` | `02` | `02` | `02` | next extend threshold stays at 20000 |
| `$20` lives | 4 | 4 | 4 | the +10000 extend is permanent |
| score | 10830 | 10830 | 11010 | 10830 kept; 180 more come from kills after 8500 |
| `$3F`/`$3E` | 0 | 0 | 1:101 | scroll restarts at 0 and steps from 8287 |
| `$1C` | `A5` | `A5` | `93` | ending music until 8286-8290 |

Nothing is reset except the stage-1 scroll and the object/spawner state that
`$9B38` and the boot already clear. In particular the loop does **not** reload
the difficulty: the run stays at rank 1 for the whole stage-1 replay.

## What the port does now (main, 8254-9000)

`Game::ending_frame` `0x8D` runs `enter_stage(0)`, resets the scroll, places the
ship at `(80, 96)` and arms a 27-frame boot, keeping the ending's `$A5`.
`Phase::Init` then spends the boot: `$1B = $01` at 8255-8256, `$02` at 8257,
`$03` at 8258 and `$04` at 8259-8281, with the PPU blanked and no gameplay.
At 8282 it sets `Phase::Normal` and `$1B = $80`; the screen is still blank and
the scroll is held for five frames, so the first step is at 8287 (`$3E`
fraction 128). `$1C` keeps `$A5` through 8282 and switches to `$93` at 8283.

The long-lived bytes are untouched: `$28 = 1`, `$17 = 1`, `$2A = $02`, lives 4
and score 10830; `$19` stays 0, so the boot never reloads a stage. The state at
8282 is `Playing`, score 10830, lives 4, loop 1 and scroll `0:0:0`.

Tests: `the_loop_restart_runs_the_measured_01_to_04_stage_one_boot` in
`crates/gradius-core/src/game.rs` asserts the `$1B` sequence, the blank PPU,
`$A5` -> `$93` at 8283 and the 8287 first step; `loop_restart.rs` pins the
8282/8287/9000 playable state. Together they fail if `$8D` falls straight into
`Phase::Normal` or restarts the music early.
