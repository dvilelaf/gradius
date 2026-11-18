# Stage 2 boss and end (natural path)

First audited 2026-11-09 on branch `agent/s2boss` at `6248097`, against the
cartridge `Gradius (E).nes` (the same ROM as `stage1_completion.md`, SHA-256
`268dc4b5b344e06e96281eb3159c1f61d3011c4f21dd1a2b4508aa934aff6143`). The clear,
rank and formation fixes landed on `agent/s2form` at `ea1ecbe`; this note was
re-checked on `agent/s2truth2` after that merge. The eruption and the whole
12001-19430 window were re-measured on `agent/s2gate` (2026-11-12) after
`2c770c6`. Builds used `CARGO_TARGET_DIR=/tmp/opencode/target-s2truth2`. This
note covers stage 2's
**own** boss and end, i.e. the frames after the already-gated `9601-12000`
handoff window.

## Verdict

* The stage-2 core **spawns exactly**: both sides first show slot 9 kind `$98`
  at **frame 17282**, with the two `$99` covers byte-identical.
* The boss and end now **match on state**: `--from 17280 --to 19430` reports
  `WITHIN TOLERANCE (0 divergences)`, 2151 compared frames, every field and
  object slot at 100%. The old `obj.y` drift from 17370 is gone.
* The core **clear/next hand-off is reproduced**. The oracle and the native
  both self-destruct at **18915** (kind-2 debris at slot 9, no score reward)
  and stage 3 starts at **19429** (`scroll=1:0:0`). The root causes are in §6;
  the fixes are in `ea1ecbe`.
* The post-window formations are fixed: the first divergence was **12161**
  (`$83/$84` two-ship formation 2 frames late, one member ahead), followed by
  the `$88`/late-wave classes at 14210/14564/15076. The natural run is clean
  from 12001 through 15967.
* The **whole natural window is exact**: `--from 12001 --to 19430` reports
  `WITHIN TOLERANCE (0 divergences)`, 7,430 compared frames, every field at
  100%. The former **15968** `$C540` eruption divergence (`obj.y`, 20 frames
  over 12001-19430) is fixed in `2c770c6`: `$BCAF` complements a negative
  delta to `|d|-1` and the rank-2 `ADC` keeps its carry, with slope
  `q + q/4 + 1`. `$C540` needed nothing of its own. The window is gated
  ROM-free by `tests/fixtures/stage2-12001-19430.state` and
  `crates/gradius-diff/tests/stage2_window.rs` (2026-11-12).
* The `$59` stage-2 music cue was not selected when this was first audited;
  `MusicDirector` now selects the stage's `$8344` id at the natural handoff
  (crates/gradius-app), covered by its natural-handoff test.

The boss/end **pixels** are still report-only residue: the 17280-19430 sample
is 97.3% match / 96.8% play / 100.0% HUD, first divergence at the spawn frame
17282. The stage-2 pixel gate remains the documented 9601-11199 window.

## Method

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-s2truth2
ROM="$GRADIUS_ROM"
cargo build --release -p gradius-diff -p xtask

# Natural path: title -> stage 1 -> stage 2 -> stage-2 boss/end.
# boss-script.txt ends at frame 9800, so the stage-2 core is never shot; the
# oracle ends it by overheat, which is the path documented below.
flock /tmp/verify-heavy.lock /tmp/opencode/target-s2truth2/release/gradius-diff \
  --rom "$ROM" --pack assets/gradius.pak --frames 19430 \
  --script scripts/boss-script.txt --keep-alive --native-keep-alive \
  --from 17280 --to 19430 --report \
  --pixel-report --pixel-window 17280:19430 \
  --json /tmp/opencode/s2truth2-boss-pixels.json

# The damage-kill reference (fresh poke boot, adaptive aim).
flock /tmp/verify-heavy.lock /tmp/opencode/target-s2truth2/release/xtask \
  record-boss "$ROM" --stage 2 --frames 10500 --out /tmp/opencode/record-boss-s2.state
```

`record-boss --stage 2` reproduces the card's reference line exactly:
`spawn 7979, first damage 8544, end 9280 (+1000), stage 3 from 9794`.
The state run above uses the natural stage-1 -> stage-2 transition at 9601
instead, because the fresh poke boot is not frame-exact (next section).

## 1. `--stage 2` fresh boot is not frame-exact

`gradius-diff --stage 2` starts the oracle with the `$26` poke and starts the
native at the oracle's first `Playing` frame. That frame is 292, but the oracle
is not yet at the stage's first simulation frame:

| frame | oracle | native `new_at_stage(1)` |
| ---: | --- | --- |
| 292 | `scroll=0:0:0`, no objects | `scroll=0:0:128`, slot 9 `$85` at (240,42) |
| 293 | `scroll=0:0:0`, no objects | `scroll=1:0:0`, slot 9 `$85` at (238,42) |
| 294 | `scroll=0:0:0`, slot 9 `$85` at (240,42) | `scroll=1:0:128`, slot 9 at (236,42) |
| 297 | `scroll=0:0:128`, slot 9 `$85` at (234,42) | — |

The native is **5 frames ahead in scroll** and **2 frames ahead in object
age**; there is no single frame shift that aligns both. The first comparison
divergence is frame 292 (`scroll`), and every field then diverges.

This is not stage-2-specific: `--stage 1` shows the same first divergence at
frame 292. It is the `Game::new_at_stage` entry, which does not reproduce the
ROM's 5-frame scroll hold / 2-frame first-spawn delay that the natural title
path already includes. The harness's `frame_offset` (pair native frame 1 with
the oracle's first `Playing` frame) is only valid for the reference path.

**Consequence:** use the natural path (reset -> stage 1 -> stage 2) as the
reference. Mapping the fresh-boot landmarks by "stage-2-relative frame" is off
by six frames here: `record-boss` spawn 7979 is +7687 from its frame 292, so a
naive natural prediction is `9601 + 7687 = 17288`; the measured natural spawn
is **17282**, because the natural entry is already at `scroll=1:0:0` on 9601.

## 2. Natural reference timeline

| event | oracle | native | note |
| --- | ---: | ---: | --- |
| stage 2 first frame | 9601 | 9601 | `scroll=1:0:0`; first object 9602 |
| last clean frame before the spawn | 17281 | 17281 | the 15968 eruption class is fixed |
| first post-handoff divergence | none | none | 12001-19430 is 0 divergences |
| stage-2 core spawn | **17282** | **17282** | identical line, incl. covers in 7/8 |
| first core divergence | none | none | boss window 17280-19430 is 0 divergences |
| core self-destruct (overheat) | **18915** | **18915** | slot 9 kind 2; last live 18914 |
| debris lifetime | 18915-18945 | 18915-18945 | kind 2 drifts out and despawns |
| stage 3 begins | **19429** | **19429** | `scroll=1:0:0` |
| final frame recorded | 19430 | 19430 | stage-3 first frame compared |

The oracle score stays at 2760 across the self-destruct (overheat awards no
+1000); the damage-kill reference in `record-boss` is a separate path.

## 3. State, 12001-19430

The natural run is **exact over the whole window**: 7,430 compared frames,
`WITHIN TOLERANCE (0 divergences)`, every field and object slot at 100%. It
keeps `state=Playing` on both sides and there are no state-mismatch frames.

The former eruption class is fixed by `2c770c6`. Before it, the run had 20
divergent frames, all `obj.y`, in four-frame +2 px bursts on slots 4/2/7/5/9
(first **15968** slot 4: oracle 66, native 68; last 16945), during the `$82`
hold's `$C540` eruption hazard. The fix is in `$BCAF`: it complements a
negative delta to `|d|-1` and the rank-2 `ADC` keeps its carry, so the slope is
`q + q/4 + 1`. `$C540` needed no change of its own. The class is now pinned by
`stage2_window`'s negative test, which perturbs the 15968 slot-4 `obj.y` by +2
and requires the gate to reject it.

The boss window (17280-19430) is exact: 2151 compared frames,
`WITHIN TOLERANCE (0 divergences)`, every field and object slot at 100%. The
boss is reached with a clean object table (both sides show the same final
pre-spawn frame at 17281).

The old boss divergences are fixed. `Game::boss_rank` (game.rs:3134) now
includes the stage term that `$9C3F` includes and that the port's own
`stage_rank` already applies, so a stage-2 run uses the ROM rank-2 vertical
speed and fire period instead of rank 1. The self-destruct mutates the local
object, so the `$ADA5` write-back can no longer revive the core, and the death
frame sets `scroll_stop`, so the 514-frame hand-off lands on 19429. Both fixes
are in `ea1ecbe`; §6 keeps the derivations.

## 4. Pixel parity

`--pixel-report` every frame, window 17280-19430, tolerance 1 px. HUD is
100.0% throughout. The report-only sample is **97.3% match / 96.8% play /
100.0% HUD**; frames 17280-17281 are 100% and the first differing pixel is the
spawn frame **17282**. No state field diverges, so the residue is in the
rendered frame, not the simulation.

This window is outside the stage-2 pixel gate, which remains the documented
9601-11199 window. The old per-range table attributed the drop to the core's
Y drift; with the drift fixed the same 2-3% residue remains, so it belongs to
the boss/covers' drawn sprites rather than the core's position.

## 5. Music cue `$59`

`Game::stage_music()` reads `$8344[$19]` and returns **`$59`** for section 1
(asserted by `stage2_replay::new_at_stage_boots_the_stage_two_rows`). When
this was first audited `MusicDirector` in `crates/gradius-app/src/music.rs`
had no per-stage cue and kept `StageClear` from the stage-1 kill. It now maps
the stage's `$8344` id through `stage_theme` and resets its stage cue when the
stage changes, so the `$59` theme starts at the natural handoff; the
`gradius-app` natural-handoff test covers it.

## 6. Root cause of the stage-2 clear and end

This is the pre-fix derivation. The fixes landed in `ea1ecbe`; §3 records the
result.

### 6.1 The oracle's overheat death (frame 18915)

New `xtask boss-trace` reads the core's raw slot-9 counters on the natural path
(`xtask boss-trace "$ROM" --script scripts/boss-script.txt --from 17270 --to
19430 --keep-alive`). `$17` is the rank written by `$9C3F`; `$042C` is the
attack-period counter, `$04AC` the 8-bit overheat counter and `$04CC` its
256-frame period count.

| frame | `$1B` | `$17` | `$042C` | `$04AC` | `$04CC` | `$046C` | slot 9 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 18910 | `$85` | 2 | 61 | 251 | 5 | 0 | `$98` y=114 |
| 18914 | `$85` | 2 | 65 | 255 | 5 | 0 | `$98` y=119 |
| **18915** | **`$86`** | 2 | 0 | **0** | **6** | **0** | **`$02` debris** y=121 |
| 18916 | `$86` | 2 | 1 | 0 | 6 | 0 | `$02` y=121, scroll resumes |

The death is the **overheat path**, not a damage kill: `$046C` stays 0, so
`$B92C CPY #$07` never branches. `$BA6A-$BA96` counts `$04AC` only on frames
where `$042C < $B904[$17]`, increments `$04CC` on every `$04AC` wrap, and at
`$04CC = 6` takes `JMP $B97D`. That last jump is the key contrast with stage 1:

* `$B95C` (shared with the damage path) does `LDA #$10; JSR $845A`, the +1000
  award, and the `$39` bonus test. Stage 1's 7-hit kill enters here, so it
  scores +1000.
* `$BA96` jumps **straight to `$B97D`**, skipping `$845A` and `$39`. The core
  becomes kind-2 debris, `$1B` increments to `$86` (`$B998-$B99F`) and no score
  is awarded. This is why the oracle score stays 2760 at 18915.

The cadence comes from the fire period `$B904[$17]`, not `$9A28`. `$17` from
`$9C3F` is `weapon + options + shield + stage` (no +1 base); this run is
`0 + 0 + 1 + 1 = 2`, so `$B904[2] = $46` (70 frames). Six 256-count wraps of
the 8-bit counter need `6*256/70` periods, which from the vertical start at
17358 lands on 18915. `$9A28` is the pre-boss `$82` hold timer (indexed by the
same rank through `stage_rank`); it does not drive the core death.

The `$86` phase then runs a fixed **514-frame** clear: `advance_section` fires
at `18915 + 514 = 19429`, after the scroll has stepped from `0:13` to `0:14`
and the low byte to `1`, leaving `scroll=1:0:0` for stage 3.

### 6.2 Why the native used to never clear (frame 18913 -> 19427 -> stuck)

Before the fix the port reached the self-destruct **two frames early, at
18913**, and then threw the death away:

1. `ai_big_core` calls `boss_self_destruct` (game.rs:3098), which writes
   `self.objects[9]`, clears covers 7/8 and sets `boss_clear = 514`
   (game.rs:3131).
2. `update_objects` called the handler with a **local copy** and then writes
   that copy back unconditionally (game.rs:2137). The copy predates the call,
   so it restores slot 9 to kind `$98` and the live core state. Only the
   covers (written as slots 7/8, not the local object) stay cleared. This is
   visible in the native dump: slots 7/8 vanish at 18913, slot 9 stays `$98`.
3. `boss_clear` is *not* part of the object, so it keeps counting from 18913
   and `advance_section` runs at `18913 + 514 = 19427`.
4. The still-live core runs `ai_big_core` every frame and sets
   `scroll_stop = true` (game.rs:2919), so the scroll never moves. At 19427
   `advance_section` sets `scroll.high = 0`; `enter_stage` does **not** clear
   the object table, so the stale core remains in slot 9 and keeps freezing
   the fresh stage-3 scroll at `0:0:0`. The stage-2 re-arm needs
   `scroll.high >= $0C`, which never happens again. At 20500 the core is still
   kind `$98`.

So the stage-2 "never clears" is one local-copy bug amplified by the 514-frame
timer, not a missing `enter_stage` clear.

### 6.3 Rank/table offset: the Y drift and the 2-frame-early death

The ROM tables are indexed directly by `$17`:

| table | rank 0..7 |
| --- | --- |
| `$B8F2` fraction | `00 20 40 60 80 A0 C0 F0` |
| `$B8FB` integer | `01 01 01 01 01 01 01 01` |
| `$B904` fire period | `5A 50 46 3C 32 28 23 23` |

The port tables in `stage1_enemies::big_core` each carry one **dummy leading
entry** (`FIRE_PERIOD[0] = $02`, `VERTICAL_FRACTION[0] = 0`, integer `0`), so
the port index must be **ROM rank + 1**. `Game::boss_rank` (game.rs:3118) is
`1 + weapon + options + shield`, which equals ROM rank + 1 only while
`section == 0`. With `section == 1` it stays 2 instead of 3:

* current `boss_rank = 2` -> fraction `$20`, fire `$50` (ROM rank 1 values);
* correct `boss_rank = 3` -> fraction `$40`, fire `$46` (ROM rank 2 values).

The wrong fraction gives the `+1/8 px/frame` Y drift from 17370, and the wrong
fire period makes the overheat wraps finish at 18913 instead of 18915. Stage 1
is unaffected because its ROM rank already is 0 (or 1 with a shield), so
`boss_rank` and the dummy entries cancel.

### 6.4 Formation cadence `$83/$84` (pre-boss, separate)

The first post-window divergence at 12161 is not a boss artefact. The oracle
emits the formation's first member (`kind $84` after the handler sets bit 7)
at scroll `1:5:0`: slot 9, y=72. The native emits a member at 12163 with
y=136, exactly one `y_step` (`$40`) lower, and no member at 12161. This is the
stage.rs spawner's pending-formation/`restart_window` path (`resolve` already
adds `y_step` once for the first member, then `pending_step` adds it again for
each further member), and the later 14210/14564/15076 waves are the same
one-member-ahead / late class. It must be fixed for stage-2 parity but is
independent of the boss clear.

### 6.5 The fix that landed

1. **Persist the self-destruct** (game.rs `ai_big_core` ~3098,
   `boss_self_destruct` ~3131, `update_objects` ~2137). Pass the local
   `object` to `boss_self_destruct` and mutate it there (kind=2, `boss=false`,
   `ai_state=0`, sprite=0, `anim=1`, `anim_div=3`, phase/subtype=0) instead of
   writing `self.objects[9]`; keep clearing slots 7/8 through `self`. The
   handler's write-back then persists the death. Do not add cleanup to
   `enter_stage`.
2. **Freeze the death frame** (same function). `boss_self_destruct` must set
   `self.scroll_stop = true` (like `destroy_boss`) as well as
   `scroll_locked = false`. The oracle holds scroll at `0:13:0` on 18915 and
   takes its first half-step at 18916; without this the port steps on the
   death frame and the 514-frame handoff lands one pixel early.
3. **`Game::boss_rank` stage term** (game.rs:3118). Compute
   `(1 + u8::from(self.section != 0) + weapon + options + shield).min(8)`.
   This yields index 3 for this run, restoring `$B8F2[2] = $40` and
   `$B904[2] = $46`, and moves the overheat end to 18915. Stage 1 is unchanged.
4. **Do not touch the debris values**: `anim=1`, `anim_div=3`, kind 2 and
   `x_vel=0` already match `turn_to_debris`/`ai_debris`; the oracle debris
   drifts at 0.5 px/frame through `ai_drift` once `scroll_stop` clears on
   18916.

The remaining, non-boss gaps from the earlier audit:

1. **Stage-2 stream formations were one member ahead / late** (stage.rs). At
   12161 and 14210 the port emitted the *second* formation member (y = first +
   `y_step`) two frames after the oracle's first; later waves were 5-13 frames
   late. Route: `stage.rs::resolve`/`pending_step`, `restart_window` (a pending
   formation across a `$3F & $0E` window change) and the `$A304` schedule in
   `stage.rs::step`. (Fixed in `ea1ecbe`.)
2. **Stage-2 `$C540` eruption hazard** (game.rs / stage2_tables). During the
   `$82` hold (15785-16969) the kind `$0B/$8B` launchers were out of phase and
   position. Route: `stage_two_hazard`, `HAZARD_SCRIPT`/`HAZARD_POSITIONS`, the
   `$02 & 7 == 0` gate (cf. the stage-1 `$AF92` phase note in
   `longrun_drift.md`). The phase gap is gone; the last 15968 `obj.y` residual
   was fixed in `2c770c6` at `$BCAF` (negative-delta complement to `|d|-1`,
   `ADC` carry, slope `q + q/4 + 1`), not in `$C540`.
3. **Per-stage music cue** (crates/gradius-app). The director now selects the
   stage's `$8344` id and resets its cue when the stage changes.
4. **Harness:** keep `--stage N` for extraction/probes, not for parity. A
   parity run must start from reset. The `--stage` window/offset handling in
   `crates/gradius-diff/src/main.rs` is otherwise correct; it is the
   `new_at_stage` entry that cannot reproduce the natural entry.

### Harness artefacts vs core gaps

* The fresh-boot offset (§1) is a `Game::new_at_stage`/harness-pairing artefact
  exposed by `--stage`; the natural 9601 handoff is exact.
* The `obj.slot` misses at 18913-18914 are the dump's sprite-0 visibility rule
  (`dump_state` skips `sprite == 0`) hitting the covers at different times, not
  a simulation difference.
* `capsule` is not a comparator field (the oracle has no capsule counter); the
  native line shows `capsule=3` during the stage-2 boss while the oracle shows
  0. This is expected and does not affect the compared state.

## 7. Regression anchors

* `crates/gradius-diff/tests/stage2_replay.rs` replays the checked-in
  `tests/fixtures/stage2-9601-12000.state` and pins the exact handoff window.
* `crates/gradius-diff/tests/stage2_window.rs` replays
  `tests/fixtures/stage2-12001-19430.state` (7,430 frames) and pins the whole
  natural window at 0 divergences plus the 17282/18915/19429 landmarks and the
  stage-3 entry scroll. It also carries the negative test for the old 15968
  `obj.y` eruption.
* `crates/gradius-diff/tests/stage2_boss_window.rs` (ROM-gated, skipped without
  `GRADIUS_ROM`) runs the natural path to 19430 and pins the oracle landmarks
  above: spawn 17282, self-destruct 18915, stage 3 at 19429, and the native
  core spawn at 17282. It does not yet assert the native's own death or
  stage-3 hand-off; `stage2_window` covers both ROM-free.

## 8. Tests that pin the fix

* **Core (`crates/gradius-core/src/game.rs`):**
  - `the_overheated_core_keeps_its_debris_after_the_ai_writes_back` forces the
    sixth overheat wrap and checks slot 9 becomes kind 2, slots 7/8 are not
    live, `boss_clear == 514`, and the death frame holds `scroll_stop` with
    `scroll_locked == false`.
  - `the_core_rank_includes_the_stage_bit` pins `boss_rank()` 2 at section 0
    and 3 at section 1 (the `$B8F2`/`$B904` rank-2 values).
  - `a_rank_two_aim_adds_the_quarter_step` pins the rank-2 aim gains.
  - `stage_two_reaches_the_shared_core_and_resets` pins the shared 514-frame
    clear advancing the section.
* **Spawner (`crates/gradius-core/src/stage.rs`):**
  - `a_failed_formation_allocation_retries_at_the_first_y` pins the 12161
    `$83/$84` class: a failed slot allocation retries at the formation's first
    Y instead of one `y_step` down.
* **xtask (no ROM):** `boss::tests::stage_rank_includes_the_stage_bit` pins
  `$9C3F` against the port's rank+1 table indexing;
  `boss::tests::trace_line_renders_the_core_counters` pins the `boss-trace`
  line format. `boss-trace` is the reproduction tool; the reference frames are
  18915 (kind `$02`) and 19429 (`scroll=1:0:0`).
* **ROM-gated parity:** the `stage2_boss_window` test above still pins the
  oracle side and the native spawn. The measured `--from 17280 --to 19430`
  run is `WITHIN TOLERANCE (0 divergences)`.
