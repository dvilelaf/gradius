# Stage-3 clear gate, `$39` bonus and the stage-4 handoff

Measured 2026-11-10 on `agent/s3clear` with the oracle harness
(`record-boss --stage 3`, `record-ending --stage 3`, `transition` with
`scripts/boss-script.txt`). Complements `stage3.md`, `stage3_gaps.md` and
`stage3_plan.md` (S3-04); it does not change core code.

## TL;DR

Stage 3 has two different endings, selected by `$39`:

| run | ten-kill `$39` | `$3F = $0C` | core spawn | core end | `$3F = $0E` | `$19` change | next section |
| --- | --- | --- | --- | --- | --- | --- | --- |
| natural (no input) | never (`$39 = 0`) | 25572 | 27110 | 28743 self-destruct | 29255 | 29257 | 3 (stage 4) |
| poked stage 3, fixed fire | 5394 (`$5F` 09 -> 0A, `$39` 0 -> 1) | 6441 | 7979 | 9612 self-destruct | 10124 | 10125 (`$8E`) | 3 |
| same run, bonus end | — | — | — | — | — | 11094 (second INC) | 4 (stage 5) |
| `record-boss --stage 3` | set (its `$8E` proves it) | — | 7979 | 9126 damage kill, +1000 | 9638 | 9639 (`$8E`) | 3 |
| same run, bonus end | — | — | — | — | — | 10608 | 4 |

`$39 = 1` takes `$9928/$992E/$9842`: `$19 += 1`, `$3A += 1`, `$3F = 0`,
`$1B = $8E`; `$8E` scrolls at **4.5 px/frame** to `$3F >= $11`, awards **+5000**
(not +500) and sets `$1B = $90`; the next frame `$96C4` does `INC $19`
**again**. **FIXED** (S3-04, `game.rs`; see the status section at the end). Before the
fix the port collapsed all of this to a 514-frame countdown
and advanced one section, so a stage-3 clear with `$39` was one frame
late, lost the 5000 points and played stage 4 instead of stage 5.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-s3clear
cargo build -p xtask

# natural entry and the `$39 = 0` end (stage 3 starts at 19429)
./target/debug/xtask transition "$ROM" --script scripts/boss-script.txt \
    --from 19429 --to 28500 --keep-alive --out /tmp/opencode/s3_natural.txt
./target/debug/xtask transition "$ROM" --script scripts/boss-script.txt \
    --from 28500 --to 30200 --keep-alive --out /tmp/opencode/s3_natural_tail.txt

# poked stage-3 run; fixed fire kills ten `$C900` turrets and sets `$39`
./target/debug/xtask record-ending "$ROM" --stage 3 --frames 21000 \
    --out /tmp/opencode/s3_ending_long.txt

# stage-3 core fight (damage kill); its 9639 handoff is the `$39` branch
./target/debug/xtask record-boss "$ROM" --stage 3 --frames 12000 \
    --out /tmp/opencode/s3_boss.txt

# direct gate check: poke `$26` until stage 3 starts, then read RAM
POKES=$(for f in $(seq 1 400); do printf -- '--poke %d:26:2 ' "$f"; done)
./target/debug/xtask probe "$ROM" --frame 5393 --buttons a --press 180:start \
    --keep-alive $POKES --ram /tmp/opencode/ram5393.bin
./target/debug/xtask probe "$ROM" --frame 5394 --buttons a --press 180:start \
    --keep-alive $POKES --ram /tmp/opencode/ram5394.bin
# $39/$5F at 0x39/0x5F: 00/09 at 5393, 01/0A at 5394
```

`record-ending` already prints `x39`/`x3a`, `phase` and `stage`, so no new
xtask helper was needed. The stage-3 start is short by poking `$26` until
`$19 == 2`; the run starts at frame 292 (`phase` `04 -> 80`).

## The ten-turret gate (`$C776`)

```
C776  E6 5F     INC $5F
C778  A5 5F     LDA $5F
C77A  C9 0A     CMP #$0A
C77C  90 04     BCC $C782
C77E  A9 01     LDA #$01
C780  85 39     STA $39        ; stage-3 clear gate
C782  E6 5D     INC $5D
C784  20 54 84  JSR $8454      ; 300
```

`$5F` is the shared byte with the stage-1 volcano counter; it is cleared only
by the stage-advance `$50-$70` loop (`$96D0`). `$97D2` clears `$39`, `$3A` and
`$5D` on a death, not `$5F`, so a retry keeps the kill count but must re-earn
the gate.

In `record-ending --stage 3` the fixed-fire ship reaches ten kills at frame
**5394**. A direct RAM probe of the same input pins it exactly: at frame 5393
`$5F = 9`, `$39 = 0`; at 5394 `$5F = 0A`, `$39 = 01`. The score stream has
eleven 300-point awards by then (1725, 2482, 3282, 3474, 3858, 4050, 4307,
4498, 4818, 5010, 5394; 3858 is a +310 frame), so at least one award is a
non-turret — only the RAM bytes above identify the gate. `$39` then stays 1
through the boss. `record-boss --stage 3` also ends through `$8E` (see 9639
below), so its dodging ship collected ten `$C900` kills as well.

## The clear state machine

`$86` waits for the stage-end scroll and `$9928` selects the branch:

```
9919  A4 19     LDY $19
991B  A5 3F     LDA $3F
991D  D9 F0 98  CMP $98F0,Y     ; $0E for stage 3
9920  90 18     BCC $993A
9922  A5 1B     LDA $1B
9924  29 70     AND #$70
9926  D0 12     BNE $993A
9928  A9 90     LDA #$90
992A  A6 39     LDX $39
992C  F0 0A     BEQ $9938       ; $39 = 0: $1B = $90 only
992E  E6 19     INC $19         ; $39 != 0: advance now
9930  E6 3A     INC $3A         ; bonus wave active
9932  A9 00     LDA #$00
9934  85 3F     STA $3F
9936  A9 8E     LDA #$8E
9938  85 1B     STA $1B
```

`$39 = 0` (natural run): `$3F = $0E` at 29255, `$1B = $90` at 29256, `$96C4`
increments at 29257. The port's 514-frame countdown lands on 29257, so the
normal path already matches.

`$39 = 1` (poked run): `$3F = $0E` at 10124; at 10125 `$19` 2 -> 3,
`$3A` 0 -> 1, `$3F` 0, `$1B = $8E`, scroll `0:0:128`. This is one frame
earlier than the normal path.

`$8E` (`$9842`) runs until `$3F >= $11`:

```
9842  A9 01     LDA #$01
9844  85 2D     STA $2D         ; palette mode 1
9846  A2 3E     LDX #$3E
9848  A9 04     LDA #$04
984A  20 07 84  JSR $8407       ; add 4 to $3E, carry into $3F
984D  A5 3F     LDA $3F
984F  C9 11     CMP #$11
9851  90 0F     BCC $9862
9853  A5 1B     LDA $1B
9855  29 70     AND #$70
9857  D0 09     BNE $9862
9859  A9 50     LDA #$50
985B  20 5A 84  JSR $845A       ; +5000, not +500
985E  A9 90     LDA #$90
9860  85 1B     STA $1B
```

`$8407` (`ADC $00,X`) adds 4 to `$3E` every frame on top of the normal
0.5 px/frame step, so the bonus scroll advances **4.5 px/frame**. From
10125 to 11092 that is 968 frames (17 x 256 px).

The reward is **+5000**: `$845A` takes its argument in the `$9A` high byte, so
`A = $50` is BCD `50 00`. The measured score is 6440 -> 11440 on frame 11093.
The same reward appears in `record-boss` at 10607 (6650 -> 11650).

`$90` then runs `$96C4`:

```
96C2  A6 1B     LDX $1B
96C4  E6 19     INC $19         ; second increment
96C6  A9 00     LDA #$00
96C8  85 39     STA $39
96CA  85 3A     STA $3A
96CC  85 3F     STA $3F
96CE  A2 20     LDX #$20
96D0  95 50     STA $50,X       ; clears $50-$70, including $5F
```

So the `$39` route increments `$19` twice: 2 -> 3 at `$992E` and 3 -> 4 at
`$96C4`. Stage index 4 is stage 5, so **the bonus clear skips stage 4**.
`record-ending` shows the second increment at 11094 (`stage=4`, `phase=80`,
`scroll=5:0:0`, `x39=0`, `x3a=0`). `record-boss` shows the same at 10608.
This is the same double increment already recorded for the forced `$39` route
in `ending.md`; for stage 3 it is not a rare poke but the normal ten-kill
ending.

At 11093 the lives byte also goes 3 -> 4: the 5000-point award crosses an
extend. That is score-system behaviour, not part of the clear branch.

## `$3F` and rank timers

| quantity | address | stage-3 value | measured |
| --- | --- | --- | --- |
| boss arm threshold | `$9A30[2]` | `$0C` | 25572 natural / 6441 poked |
| stage-end threshold | `$98F0[2]` | `$0E` | 29255 natural / 10124 poked |
| `$82` hold | `$9A28[$17] * 256` | rank 2 -> `4 * 256` = 1024 | 25573-26596, 6442-7465 |
| `$81`/`$83`/`$84` | — | 1 / 1 / until `$3F = $0D` | `$84` is 512 frames (26598-27109) |
| core spawn | `$9975` + `$9A30 + 1` | at `$3F = $0D` | 27110 natural / 7979 poked |

Rank `$17` is `$44 != 0` + options + `$46 != 0` + `$19 != 0`; with the
keep-alive shield only it is 2 (`$9A28[2] = 4`, the port's
`stage_rank()`/`CHECKPOINT_TIMER` already match). The `$82` hold is therefore
1024 frames, **not** the 7-frame `transition_frames(2)` value: that 7 belongs
to the mid-stage section transition (`$9A28[2] + 3`), not to the boss window.
The earlier S3-04 wording in `stage3_plan.md` ("`$82` is 7 frames", "+500")
was corrected there to the 1024-frame hold and the 5000 award.

## Port comparison (before the fix)

The core as it stood before `agent/s3finish` (`crates/gradius-core/src/game.rs`):

* `turret_death` already counts `volcano_kills` and sets `stage_clear_ready`
  at 10 — the gate itself is correct.
* `destroy_boss`/`boss_self_destruct` set `boss_clear = 514`; when it expires,
  `normal_frame` calls `advance_section()` unconditionally. `stage_clear_ready`
  is not read there (`ae0a0a6` removed the last use).
* `advance_section()`/`enter_stage()` advance exactly one section, clear
  `stage_clear_ready`/`volcano_kills`, and keep `scroll.low` while zeroing
  `scroll.high` — that is `$96C4`, and it is correct only for `$39 = 0`.
* There is no `$3A` field, no `$8E` state, no 4-px-per-frame scroll, no
  +5000 award, and no second `INC $19`.
* `stage_three_reaches_the_shared_core_and_advances` asserts section 3 after
  `boss_clear`; that encodes the `$39 = 0` path.
* `finish_death` does not clear `stage_clear_ready`, while `$97D2` clears
  `$39` (and keeps `$5F`).
* `STAGE3_BOSS_THRESHOLD`/`STAGE3_END_THRESHOLD` exist in `stage_bgs.rs` but
  are unused; `StageInfo` has `boss_trigger` but no `end_threshold`;

Consequences for a stage-3 clear with `$39 = 1`: the port advances on
death + 514 instead of + 513 (one frame late), awards no 5000 points, leaves
`$3A` at 0 so the `$C680` bonus wave never runs, and enters section 3
(stage 4) instead of section 4 (stage 5).

## Fix plan (`game.rs`, S3-04, implemented)

1. **Replace the 514-frame collapse with the `$86` state.** On core death (kill
   or self-destruct) set a `BossEnd`/clear phase and unfreeze the scroll
   instead of arming `boss_clear`. Each frame, after the scroll step, test
   `scroll.high >= end_threshold(section)` (`$98F0`; add `end_threshold` to
   `StageInfo`, stage 3 = `0x0E`). When true:
   * `$39 == 0`: set a one-frame `stage_clear_arm` (the `$1B = $90` frame),
     then call `advance_section()` on the next frame (`$96C4`).
   * `$39 != 0`: `section += 1` (the `$992E` INC), `$3A = 1`, `scroll.high = 0`,
     and enter the `$8E` bonus phase. Do not clear `$39`/`$5F` yet.
   This reproduces the one-frame-earlier `$39` handoff without a magic 513.
2. **Add the `$8E` bonus phase.** Each frame: normal 0.5 px scroll step plus
   `add_scroll_low(4)` (the `$8407` add with carry into `scroll.high`). When
   `scroll.high >= 0x11` and the phase has not awarded yet, `add_score(5000)`
   and set the one-frame `$90` state. The next frame must run the normal
   `advance_section()` path, i.e. the second `INC $19`; `advance_section()`
   already zeroes `$3F`, clears `$39`/`$3A` and the `$50-$70` block.
3. **Store `$3A`.** Add a `bonus_wave: bool` (or `hazard_bonus`) field, set it
   at `$992E`, clear it in `advance_section()`/`enter_stage()`, and let S3-03
   read it: `$C680` uses `$3A` to select period `$0A`, sprite `$00`, kind `$A6`
   and, with `$3A != 0`, runs from `$3F = 0` to `$0D` during `$8E` as well as
   `$82`.
4. **Clear `$39` on death.** Add `stage_clear_ready = false` (and `bonus_wave`
   if set) to `finish_death`, mirroring `$97D2`; keep `volcano_kills` (`$5F`)
   untouched so the tenth kill still re-sets the gate after a retry.
5. **Use the stage records.** Replace the `0x0C`/`0x0D` literals in
   `normal_frame` with `stage_info().boss_trigger` and the new
   `end_threshold`; stage 1/2 values are unchanged (`0x0C`/`0x0E`).
6. **Tests** (core tests; not edited by this card):
   * stage-3 `$39 = 0` clear still advances one section at death + 514 and
     clears `$39`/`volcano_kills`;
   * stage-3 `$39 = 1` (set by ten `turret_death` calls) advances `$19` 2 -> 3
     one frame before the normal path, sets `$3A`, and awards 5000 exactly once
     when `$3F` reaches `$11` over 968 frames;
   * the same run then advances to section 4 (stage 5), `scroll == 5:0:0`,
     with `$39/$3A` clear — the documented double increment;
   * a death after ten kills clears `$39` but leaves `volcano_kills == 10`,
     and the next turret kill sets `$39` again;
   * stage 1/2 state hashes unchanged.

## Status after `agent/s3finish` (2026-11-10): the clear is FIXED

Implemented in `game.rs`, scoped to stage 3 with the `$39` gate so the
stage-1/2 `boss_clear = 514` path is untouched. `record-boss` reproduces the
oracle exactly: kill 9126, death + 513 handoff 9639, +5000 10607, stage 5
10608.

* `start_stage_clear` selects the `$86` state (`Clear::WaitEnd`) instead of
  the 514 countdown when `section == 2 && stage_clear_ready`.
* The clear handler runs **before** the `$98E1` step (as `$86` does): at
  `$3F >= STAGE_END[2] = $0E` it does the `$992E` branch (`section += 1`,
  `bonus_wave = true`, `$3F = 0`); `$8E`/`$8407` adds 4 to `$3E` each frame
  on top of the 0.5 px step; at `$3F >= $11` it calls `add_score(5000)` once
  and sets `Clear::Handoff`; the next frame `advance_section()` runs `$96C4`.
* `finish_death` clears `$39`/`$3A` and the clear state but not
  `volcano_kills`.
* Boss arming and the normal section transition are gated on
  `Clear::None`.

The new unit tests pin: the `$39 = 1` handoff at death + 513, the +5000 at
+968 after the handoff, stage 5 (section 4) one frame later, a single award,
`$39/$3A` cleared, and the `$39 = 0` 514-frame path unchanged. The
`scroll.low/high/fraction` on the handoff frame is `0:0:$80`, matching the
documented `0:0:128`.

Not done: the `$C680` `$3A` bonus wave (S3-03) is still absent, so the `$8E`
scroll runs without its scripted kind-`$A6` objects; and the `$0C`/`$0D`
literals in `normal_frame` still stand (step 5 of the fix plan).

**Update after the capsule-rule pass (2026-11-10).** The natural 19429-23000
fixture now replays `WITHIN TOLERANCE (0 divergences)`: the 21526 capsule, the
last state divergence left after this clear work, is fixed because `$B09A`
writes `$018C` and not `$03AC`, so the 21525 wreck keeps its cannon phase and
folds (score 3110, meter 4). Stage-3 pixels are report-only at 98.9% overall /
98.8% play / 100.0% HUD over the 119 samples, the only OPEN pixel cause being
the `$BBB1`/`$BC3E` object-fire pass (constants in
[stage3_gaps.md](stage3_gaps.md)). That leaves the `$C680` `$3A` bonus wave
(S3-03) and then the gated stage-3 pixel window (S3-05).

## Out of scope / follow-ups

* The `$C680` `$3A = 1` bonus wave (kind `$A6`, one per 40 frames while
  `$3F < $0E`) is S3-03; the `$39` branch only supplies its `$3A` state.
* The skipped stage is real ROM behaviour. Do not "fix" the second `INC $19`
  in the port unless the parity target deliberately diverges; if it is fixed,
  document it here and in `ending.md`.
* The 514 constant remains correct for `$39 = 0`; the `$86` state replacement
  in step 1 should be verified against the stage-1 9599/9600/9601 ledger and
  the stage-3 29255/29256/29257 ledger before removing it.
