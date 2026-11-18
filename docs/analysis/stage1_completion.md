# Stage 1 completion audit

Audited 2026-11-09 on branch `agent/stage1audit` at `5bf4cd9`, against the
cartridge `Gradius (E).nes` (SHA-256
`268dc4b5b344e06e96281eb3159c1f61d3011c4f21dd1a2b4508aa934aff6143`). The
commands below were run for this audit with
`CARGO_TARGET_DIR=/tmp/opencode/target-stage1audit`; logs are under
`/tmp/opencode/stage1audit-release` and `/tmp/opencode/stage1audit-bench`.
No source file was changed.

**Re-checked 2026-11-09 on the `ci6` tree (`e0d3d93`)** after the boss, debris
and stage-2 handoff fixes, with `CARGO_TARGET_DIR=/tmp/opencode/target-ci6`
and logs in `/tmp/opencode/ci6-release-long`. The gate, long-run and
game-over rows below are from that re-run; the performance row is still the
`5bf4cd9` measurement.

**Verdict: stage 1 is releasable.** The state, pixel, attract, audio, terrain,
power-up, high-score, game-over and performance checks pass, the Big Core
timeline is correct, and the 7500-9700 boss gate reports 0 divergences. The
release gate reports 0 hard failures. The 12,000-frame long run remains a soft
`WARN`: the default script drifts from 3159 and the keep-alive boss script from
1357 and 6503, all outside the gated window.

## Reproduce

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-ci6
ROM=$GRADIUS_ROM
OUT=/tmp/opencode/ci6-release-long

# every release gate, with per-gate logs under $OUT
OUT=$OUT scripts/release-check.sh "$ROM"

# ROM-free fixtures
cargo test -p gradius-diff --test attract_fixture --test fixture_replay

# terrain probes (map collision must bypass the shield)
cargo xtask probe "$ROM" --frame 2170 --press 180:start --buttons down --shield \
  --ram /tmp/opencode/s1dn.bin
cargo xtask probe "$ROM" --frame 2746 --press 180:start --buttons up --shield \
  --ram /tmp/opencode/s1up.bin

# power-ups, high score and map collision
cargo test -p gradius-core --lib -- \
  game::tests::capsule_advances_the_selection \
  game::tests::missile_is_a_one_shot \
  game::tests::double_and_laser_alternate \
  game::tests::options_stop_at_two \
  game::tests::shield_is_a_one_shot \
  game::tests::the_cartridge_seeds_the_high_score \
  game::tests::ship_vs_map_collision \
  game::tests::flying_into_solid_terrain

cargo test -p gradius-audio --test pack --test tracks

# long run, 12,000 frames, both scripts
scripts/longrun.sh --rom "$ROM" --pack "$OUT/stage1.pak" --out "$OUT/longrun"

# performance and artifact sizes (5bf4cd9 audit tree, target-stage1audit)
GRADIUS_ROM="$ROM" scripts/bench.sh --frames 10000 --repeat 3
```

## Checklist

| item | result | evidence |
| --- | --- | --- |
| State parity, 3000 frames | **PASS** | `state` gate: 2,682 `Playing` frames compared, `result: WITHIN TOLERANCE (0 divergences)`, every field 100% |
| Pixel parity, 700 frames | **PASS** | `pixel` gate: playfield 100.000%, HUD 100.000%, 409 compared frames; tolerance 1 px. The 700-sample report matches 100.0% (play 100.0%, HUD 100.0%), first difference a single pixel at frame 391 (`206,50`) |
| Attract fixture, 130-800 | **PASS** | `attract_fixture`: 671 recorded frames replay with 0 divergences; `Title`, `Other(1)` and `Other(2)` all covered. `fixture_replay` (600-frame stage fixture) also passes |
| Boss spawn | **PASS** | oracle and port both first show slot 9 kind `$98` at frame **7723** |
| Boss damage | **PASS** | both sides first award the +50 damage points at the state line for frame **8067** (the hit lands on **8066**; `$0475` damage 1) |
| Boss clear | **PASS** | both sides remove the core at frame **9087** and jump to score **2690** (+1000) |
| Boss window state, 7500-9600 | **PASS** | 0 divergences; the 7500-9700 gate run compares 2,201 frames and every field at 100% |
| Boss release gate, 7500-9700 | **PASS** | `result: WITHIN TOLERANCE (0 divergences)`; spawn 7723, damage 8067, clear 9087, score 2690, debris slot 9 kind 2, stage-2 reset 9601; slot match 4,453/4,453 |
| Post-clear residues | **PASS** | slot 9 keeps kind-2 debris through **9117** and clears at **9118** on both sides; both reset scroll `0:14:128 -> 1:0:0` at **9601** |
| Terrain collision, floor | **PASS** | frame 2170, ship Y=192, `$0100=02`, `$1B=$A0`, `$46=5`: dead on the floor map byte even with a full shield |
| Terrain collision, ceiling | **PASS** | frame 2746, ship Y=16, `$0100=02`, `$1B=$A0`, `$46=5`: dead on the ceiling map byte |
| Six power-ups | **PASS** | `capsule_advances...` (SPEED), `missile_is_a_one_shot`, `double_and_laser_alternate` (both directions), `options_stop_at_two`, `shield_is_a_one_shot_five_hit_upgrade`; the five power-up tests plus three high-score/terrain tests all pass (8/8 filtered) |
| High score (`HI`) | **PASS** | cold boot seeds `0050000`; score is copied only when it passes the seed; a new run clears score but keeps the high score |
| Game-over flow | **PASS** (WARN: one frame late) | port reaches `Title` at frame **4677**; oracle at **4676**. Five lives wrap at 4278, then the port plays on and returns to the title |
| Audio: 12 music / 8 SFX | **PASS** | `a_pack_decodes_all_twelve_distinct_music_tracks` (12/12 distinct, non-empty), `every_music_track_renders_audibly`, `every_effect_is_audible` (8/8); 16 audio tests pass |
| Triangle / bass | **PASS** | `every_stage_theme_has_an_audible_triangle_bass`; release-check `audio` gate `triangle band present` |
| Long run, 12,000 frames | **WARN** | default: 7,250 divergences over 4,249 compared frames, drift from **3159**; boss: 527 divergences over 11,709 compared frames, drift from **1357** and **6503**; soft gate only |
| Release gate overall | **PASS** | 0 hard failures; longrun is WARN and web is SKIP (`--skip-wasm`) in the re-run |
| Performance | **PASS** | core 0.030 ms/frame (555x headroom), SDL headless 0.033 ms/frame (504x), oracle 0.318 ms/frame (52x); 16.639 ms budget |

The release-gate table as printed:

```text
gate       state detail
----       ----- ------
build      PASS  release binaries
tests      PASS  cargo test --workspace
extract    PASS  /tmp/opencode/ci6-release-long/stage1.pak
state      PASS  3000 frames, 0 divergences
pixel      PASS  play 100.000% hud 100.000%
boss       PASS  7500-9700, 0 divergences; spawn 7723 damage 8067 clear 9087 score 2690 debris slot 9 kind 2 stage-2 9601
longrun    WARN  default drift from frame 3159 (obj.slot); boss drift from frame 1357 (obj.slot)
audio      PASS  triangle band present
web        SKIP  --skip-wasm

release-check: 0 hard failure(s), logs in /tmp/opencode/ci6-release-long
```

## Long-run 12k known gaps

Two runs of 12,000 frames (`scripts/longrun.sh`), state compared only while both
sides report `Playing`.

### Default script

First divergence by field:

| field | first frame |
| --- | ---: |
| `obj.slot` | 3159 |
| `obj.y` | 3174 |
| `obj.kind`, `obj.x`, `obj.substate` | 3189 |
| `px`, `py`, `scroll` | 4305 |
| `score` | 4349 |
| `lives` | 4647 |

- 87 state-mismatch frames: 4278-4304, 4620-4646, 4676, 4806-4809, 5063-5066,
  8304-8307, 8434-8440, 8691-8697, 11932-11937.
- The object bookkeeping from 3159 is the root; the later lives/state windows
  and the one-frame title offset are downstream.
- Game-over/attract then runs: port `Title` 4677-4809, `Other(1)` 4810-5066,
  `Other(2)` 5067-8307, second cycle and `Title` 11938-12000. The oracle's
  first `Title` is 4676.

### Boss script with keep-alive

| field | first frame |
| --- | ---: |
| `obj.slot` | 1357 |
| `obj.x` | 6503 |
| `obj.slot` again | 11669 |

State type matches for all 12,000 frames; the divergences are fields. The 527
divergences all fall outside the gated 7500-9700 window, which is clean, and
the 9601 section reset now matches. The `obj.slot` differences at 1357 and
11669 are spawn races; the `obj.x` drift starts at 6503 in the eruption wave
and does not reappear after frame 8000.

## What is left for stage 1

1. **Default-run object parity after 3159 (soft WARN).** Slots, x/y, kind and
   substate drift in ordinary stage-1 play, giving 7,250 divergences over
   4,249 compared frames; everything later (score, lives, the game-over blanks
   and title frame) follows from it. The 3000-frame and boss-window gates are
   unaffected, but a 12k parity claim is not true.
2. **Boss-script pre-window drift (soft WARN).** 527 divergences over 11,709
   compared frames: the `obj.slot` spawn races at 1357 and 11669 and the
   `obj.x` eruption-wave drift from 6503. All are outside the gated window.
3. **Game-over handoff, one frame late (soft WARN).** The port reaches `Title`
   at 4677 against the oracle's 4676, and the attract cycles run 2-6 frames
   late (87 state-mismatch frames in the 12k default run).
4. **Cosmetic residuals** (inside the 1 px pixel tolerance, not gate
   failures): the 700-frame pixel report matches 100.0%, with one differing
   pixel at frame 391 (`206,50`) and worst sample frame 430 at 99.9%. The
   boss script's attract sample at frame 150 still differs in an 8x16
   ship/cursor box (`95,134-102,149`).

## Extra work, not stage 1

- **Two-player play.** `gradius-core::twoplayer` models turns, checkpoints and
  game over, but no runtime ship exists and neither frontend drives player 2.
- **Save states / rewind.** `docs/analysis/savestates.md` is a design only;
  there is no save/restore code.
- **Stages 2-7.** Spawn, terrain and background data plus all stage themes are
  extracted, and the shared runtime now follows stage 2 from 9601 to 12,000
  with matching scroll, score and object kinds, but only stage 1 has a gate.
  `docs/analysis/stage_transition.md` records the remaining stage-2 boss work.
- **Ending, continue music and stage-number music selection** in the
  frontends remain out of scope (`docs/AUDIO.md`).

## Documentation drift found by the audit

The audit found `README.md` and `docs/DEVELOPMENT.md` claiming the boss gate
was green before the fix, and `docs/analysis/longrun.md` still claiming the
boss was never damaged and the port never left `GameOver`. Both were corrected:
`README.md` and `docs/DEVELOPMENT.md` now record the green 7500-9700 gate and
the soft long-run WARN, and this revision brings `longrun.md` to the same
evidence. The measured numbers above are the current evidence.
