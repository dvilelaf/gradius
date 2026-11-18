# Stage 1 → 2 handoff: measured sequence and the missing reset

Measured 2026-11-09 on `agent/transition` (from `a81d95f`, re-verified after
merging main at `028a616` which contains `ae0a0a6`) with the release harness in
`/tmp/opencode/target-transition`, ROM `Gradius (E).nes`. This document records
the oracle side of the handoff and the one `game.rs` step the port skipped. No
cartridge bytes are stored here.

## TL;DR

The oracle advances `$19` 0 → 1 at **frame 9601**, zeroes `$3F` (`0E → 00`),
and the stage-2 spawner emits its first enemy one frame later (slot 9, kind
`$85` at 9602). Until `ae0a0a6`, the port reached the same 514-frame clear
countdown but called `advance_section()` only when
`section != 0 || stage_clear_ready`. Stage 1's `$39` is never set by the boss
script, so the call was skipped: the port kept scrolling stage 1 (`$3F = 14` at
9601, `18` at 12000), never restarted the spawner on `SECTIONS[1]`, and never
emitted a stage-2 object. That was the 9601 `scroll` divergence
(`oracle 1:0:0`, `native 1:14:0`), followed by slot 9 at 9602.

`ae0a0a6` on main removed the guard and added the one-frame `boss_clear_armed`
offset. After merging that commit this branch's lockstep run is clean from 9500
to 9800 (`0 divergences`, `objects 750/750`), so the handoff itself is fixed.
`advance_section` still does not reset the port-only `boss_spawned` latch; that
does not affect the 9601 window but will block stage 2's own Big Core.

No object or collision clear is missing. The `$90` handler the oracle enters
does **not** clear the `$0300`/`$0100` pages; those are only cleared by the
stage-init entry `$9B38` (new game/death), which the handoff does not call.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-transition
cargo build --release -p xtask -p gradius-diff

# oracle handoff ledger ($19/$1B/$1C/$3D-$3F/ship/objects)
cargo xtask transition "$ROM" --script scripts/boss-script.txt \
    --from 9595 --to 9614 --keep-alive

# lockstep handoff window
/tmp/opencode/target-transition/release/gradius-diff \
    --rom "$ROM" --pack assets/gradius.pak \
    --script scripts/boss-script.txt --keep-alive --native-keep-alive \
    --frames 9900 --from 9500 --to 9800 --report \
    --oracle-out /tmp/opencode/oracle.txt \
    --native-out /tmp/opencode/native.txt
```

Before `ae0a0a6` the diff reported `first divergence: frame 9601 field scroll
(oracle 1:0:0, native 1:14:0)` and, from 9602, `obj.slot slot 9: kind 133
absent`. After merging that commit the same window reports
`WITHIN TOLERANCE (0 divergences)`, and the 9602/9613 object lines are
byte-identical.

## Oracle ledger, frames 9595-9613

```
f09595 stage=00 phase=86 music=93 scroll=FE:0D:00 ship=22,98 player=01 s39=00 s3a=00 objects=
f09599 stage=00 phase=86 music=93 scroll=00:0E:00 ship=22,98 player=01 s39=00 s3a=00 objects=
f09600 stage=00 phase=90 music=93 scroll=00:0E:80 ship=22,98 player=01 s39=00 s3a=00 objects=
f09601 stage=01 phase=80 music=93 scroll=01:00:00 ship=22,98 player=01 s39=00 s3a=00 objects=
f09602 stage=01 phase=80 music=93 scroll=01:00:80 ship=22,98 player=01 s39=00 s3a=00 objects=09:85
f09613 stage=01 phase=80 music=93 scroll=07:00:00 ship=22,98 player=01 s39=00 s3a=00 objects=08:85 09:85
```

`scroll` is `$3E:$3F:$3D`; `objects` lists `slot:$030C` for slots 1-9 (slot 9
is `$0315`, slot 8 is `$0314`). Field-by-field:

| frame | event | evidence |
| --- | --- | --- |
| 9087 | Big Core dies, +1000, score 2690; 514-frame clear timer starts | `bosses.md`, diff run |
| 9599 | scroll reaches `$3F = $0E` (`$98F0[0]`); `$3E` wraps `FF → 00` | ledger |
| 9600 | `$1B` `$86 → $90` (`$9928`, `$39 = 0` so no `INC $19` yet) | ledger |
| 9601 | `$19` `$00 → $01`; `$1B` `$90 → $80`; `$3F` `$0E → $00`; `$3E` `$00 → $01`; `$39/$3A` stay 0; ship unchanged | ledger |
| 9602 | stage-2 stream spawns slot 9 kind `$85` at x = 240 | `$0315 00 → 85` |
| 9613 | slot 8 kind `$85` | `$0314 00 → 85` |
| 11648 | `$1C` `$93 → $59`: stage-2 theme, when `$3F` reaches `$8354[1] = 4` | `ramdump --watch` |

Music does **not** change at the handoff: `$1C` stays `$93` (the shared intro)
through 9601 and until 11648. `$835C` selects `$8344[$19]` (`$59` for stage 2)
only once `$3F` reaches the stage boundary, and only on an `$3E == 0` frame.

## The ROM sequence

The boss-clear path enters the `$90` handler, not a dedicated stage-init:

```asm
; $9919: normal stage-end check
9919  A4 19     LDY $19
991B  A5 3F     LDA $3F
991D  D9 F0 98  CMP $98F0,Y     ; $0E for stage 1
9920  90 18     BCC $993A
9922  A5 1B     LDA $1B
9924  29 70     AND #$70
9926  D0 12     BNE $993A
9928  A9 90     LDA #$90        ; enter $1B = $90
992A  A6 39     LDX $39
992C  F0 0A     BEQ $9938       ; $39 = 0 -> no INC $19 here
992E  E6 19     INC $19
9930  E6 3A     INC $3A
9932  A9 00     LDA #$00
9934  85 3F     STA $3F
9936  A9 8E     LDA #$8E
9938  85 1B     STA $1B
```

The next frame the `$1B = $90` handler runs `$96C2`:

```asm
96C2  A6 1B     LDX $1B
96C4  E6 19     INC $19         ; stage 2
96C6  A9 00     LDA #$00
96C8  85 39     STA $39         ; clear stage-1 clear gate
96CA  85 3A     STA $3A         ; clear hazard counter
96CC  85 3F     STA $3F         ; scroll high / nametable = 0
96CE  A2 20     LDX #$20
96D0  95 50     STA $50,X       ; clear $50-$70 (incl. $69 spawn counter)
96D2  CA        DEX
96D3  10 FB     BPL $96D0
96D5  A9 01     LDA #$01
96D7  85 55     STA $55
96D9  20 EA 9B  JSR $9BEA       ; HUD/status upload
96DC  20 36 9C  JSR $9C36       ; $1B = $80, normal play
```

`$96C2` writes no `$0100` or `$0300` byte. The full page clear lives in
`$9B38` (`$3D-$97`, `$0100-$017F`, `$0300-$037F`, `$0500-$06FF`) and is
reached from `$9B68` on a fresh stage entry, not from `$90`. The oracle
confirms it: objects are already empty at 9601, and the only `$0300-$031F`
writes around the handoff are the two stage-2 spawns.

Collision data is ROM/const; there is no RAM collision map to clear. The ship
also keeps its stage-1 position: `$0360/$0320 = 22,98` before and after 9601.
The start-row placement (`$9B86-$9BA9`) is inside `$9B38` and does not run.

## The port bug (fixed on main in `ae0a0a6`)

Before the fix, `crates/gradius-core/src/game.rs::normal_frame` read:

```rust
if self.boss_clear > 0 {
    self.boss_clear -= 1;
    if self.boss_clear == 0 && (self.section != 0 || self.stage_clear_ready) {
        // `$9928`: stage 1 only advances when the four volcano kills
        // set `$39`.
        self.advance_section();
    }
}
```

The `$39` test was misplaced. `$39` decides only whether `$9928` itself does
`INC $19`/`STA $3F`; when `$39 = 0` the ROM still loads `$90` into `$1B`, and
the next frame `$96C2` increments `$19` and zeroes `$3F` **unconditionally**.
The port collapses `$90` + `$96C2` into `advance_section`, so its call must be
unconditional once the 514-frame timer reaches zero. With the guard, stage 1
never left the boss sequence: `scroll.high` stayed `$0E`, the spawner was never
restarted on `SECTIONS[1]`, and no stage-2 object existed.

`ae0a0a6` now reads:

```rust
if self.boss_clear > 0 {
    // `$98F7`/`$96C4`: the normal `$98F0` end advances the stage
    // without `$39`; `$39` only selects the longer `$9928` bonus
    // branch. The death frame itself does not count, so 514 frames
    // after the kill is the first stage frame.
    if self.boss_clear_armed {
        self.boss_clear -= 1;
        if self.boss_clear == 0 {
            self.advance_section();
        }
    } else {
        self.boss_clear_armed = true;
    }
}
```

The post-merge diff over 9500-9800 is clean:

```
gradius-diff: 9900 frames run, 301 compared (window 9500-9800), tolerance 1 px
result: WITHIN TOLERANCE (0 divergences)
objects: slot match 750/750 (100.0%)
```

## Fix plan (game.rs)

1. **Landed in `ae0a0a6`.** Drop the
   `(self.section != 0 || self.stage_clear_ready)` condition; call
   `advance_section()` whenever `boss_clear` reaches 0. The `boss_clear_armed`
   flag accounts for the death frame, so the call lands on 9601 and runs after
   `scroll.step()`, reproducing `$3E 0 → 1`, `$3F → 0`.
2. **Still to do for stage 2's own boss:** make `advance_section` mirror the
   rest of `$96C2` before the next stage runs:
   - `stage_clear_ready = false` (`$39 = 0`) and `hazard_index = 0`
     (`$69` is inside the `$50-$70` clear);
   - `boss_spawned = false`, `boss_timer = 0`, `boss_stage = 0`,
     `scroll_locked = false`, `scroll_stop = false` (per-stage boss state).
     `boss_spawned` is the important one: it is still true after the stage-1
     kill, so the `boss_stage == 0 && !boss_spawned` gate can never spawn the
     stage-2 core.
   - keep `section += 1`, `scroll.high = 0`, `spawner.restart(section, 0)`,
     `phase = Normal`, `stage_clock = 0`.
3. Do **not** clear `objects`, `shots`, `enemy_bullets` or the collision map.
   The ROM does not clear them here, and `destroy_boss` already removes slots
   7-9; the object page is empty by 9601.

Out of scope for the reset line but required before stage 2 plays correctly:
`render` still selects `stage1_bg::STAGE1`, terrain/collision still use stage 1,
the boss gate still uses the stage-1 `$8A` eruption hold, and the frontend
music director still keys off scroll rather than `Game::stage()`; stage-2 `$59`
only starts at 11648 and must not be forced at 9601. These are the S2-00/01/02
items in `stage2_plan.md`.

## Tests for the reset

* Regression test: `advance_section` reached from a stage-1 `boss_clear`
  countdown with `stage_clear_ready == false` makes `section == 1`,
  `scroll == 1:0:0`, clears `boss_spawned`/`stage_clear_ready`, and restarts
  the spawner on `SECTIONS[1]`.
* Same run through 9602: slot 9 is kind `$85` at x = 240. The post-merge diff
  already shows this on both sides (`9602` native and oracle lines are equal),
  with no stage-1 column-`$0E` object.
* A stage-1 volcano clear (`stage_clear_ready == true`) still advances at the
  same countdown frame.
* `xtask transition` has synthetic tests for the ledger format and the
  `+`/`,` script parser (`xtask/src/transition.rs`).
