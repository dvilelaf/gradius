# Where the port diverges over a long run

A 12,000-frame lockstep map of `gradius-core` against the cartridge oracle, in
two runs: the harness default script (the unassisted run dies and game-overs)
and the recorded boss script with both ships kept alive. The point is to move
past the first few hundred clean frames and record **where** the state line and
the picture separate, bucket by bucket, so fixes can be ordered by evidence.

Measured 2026-11-09 on `agent/longreport` (`608320b`), release build in
`/tmp/opencode/target-longreport`, logs in `/tmp/opencode/lr-final`, ROM
`Gradius (E).nes`. No cartridge bytes are stored here; the ROM is read in
place. These numbers supersede the `e0d3d93` measurement, which predated the
object and power-up fixes and the state-mismatch verdict.

## Reproduce

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-longreport
export GRADIUS_ROM="$GRADIUS_ROM"
export OUT=/tmp/opencode/lr-final

cargo build --release -p gradius-diff
cargo xtask extract "$GRADIUS_ROM" "$OUT/stage1.pak"
scripts/longrun.sh --rom "$GRADIUS_ROM" --pack "$OUT/stage1.pak" --out "$OUT/longrun"
```

`longrun.sh` runs both cases, samples pixels every 30th frame, and writes:

| file | content |
| --- | --- |
| `default.txt`, `boss.txt` | the harness report (field rates, first divergence, pixel table) |
| `default.json`, `boss.json` | the same summary plus the per-sample pixel data |
| `default.oracle`, `default.native` | one state line per frame (`Game::dump_state` format) |
| `boss.oracle`, `boss.native` | the same for the kept-alive boss run |
| `buckets.txt` | both tables below plus field firsts, state runs and object-kind census |

The two harness invocations are:

```sh
# default script
gradius-diff --rom "$GRADIUS_ROM" --pack "$OUT/stage1.pak" --frames 12000 \
    --json out/default.json --oracle-out out/default.oracle \
    --native-out out/default.native --pixel-report --png-every 30

# boss script, oracle and port both kept alive
gradius-diff --rom "$GRADIUS_ROM" --pack "$OUT/stage1.pak" --frames 12000 \
    --script scripts/boss-script.txt --keep-alive --native-keep-alive \
    --json out/boss.json --oracle-out out/boss.oracle \
    --native-out out/boss.native --pixel-report --png-every 30
```

`--keep-alive` applies the analysis-only death poke to the oracle, the same one
`xtask record-boss` uses; `--native-keep-alive` does the equivalent to the port.
Both are required for the boss script to reach the core on both sides.

## How to read the buckets

Frame numbers are the dump's own `frame=` field: dump line `N` is the state
after frame `N`, and bucket `1-1000` holds lines 1-1000. Pixel samples are
labelled with the 0-based step, so sample `N` is the same picture as dump line
`N+1`; samples are taken every 30 frames.

* **cmp** - frames in the bucket where both sides report `Playing`; only these
  are compared by `gradius-diff` unless an explicit `--from/--to` window is
  given. The default run stops being comparable after frame 4675.
* **state** - frames where the two `state=` fields are equal.
* **first state** - first frame in the bucket where `state=` differs, with the
  oracle and port values. State-only drift is not a compared field, so it must
  be read from this column; `buckets.txt` counts it per run and the release
  gate WARNs on any non-zero count.
* **first in window** - first frame in the bucket with a divergence inside the
  `Playing` window, with the first differing field in compare order. `cont.`
  means the divergence started earlier and is still present at the bucket edge.
* **slot pairs** - object slots present on both sides over all slots present on
  either side (the harness `obj.slot` match rate).
* **live set** - frames where the set of occupied slots is identical.
* **pixel / play / hud** - mean pixel match of the 1/30 samples over the whole
  frame, the playfield and the HUD.

`buckets.txt` also prints `object count mismatch frames (Playing)`, the
object-coverage counterpart of the state count; it is the harness's live-object
delta restricted to compared frames. `capsule` is not compared (the original
keeps no counter). The port zeroes its ship position outside play while the
oracle keeps the title/attract ship position, so raw dumps differ at `px`/`py`
from frame 5 even in clean buckets; those frames are outside the compared
window and are not reported below.

## Default script, 0-12000

Run summary: 4,303 compared frames (window 292-4,675), 0 divergences.
Field pass rates: frame/lives/meter/obj.kind/obj.substate/obj.x/obj.y/px/py/
score/scroll/speed 100%. Object slot match 22,133/22,133 (100.0%), live count
delta 0 on 0 frames. Pixels: 97.1% overall (play 97.1%, HUD 96.8%), first
pixel difference at 1620. There are 22 state mismatch frames and 0 object
count mismatch frames.

| bucket | cmp | state | first state | first in window | slot pairs | live set | pixel | play | hud | samples |
| --- | ---: | ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1-1000 | 709 | 1000/1000 | - | - | 2911/2911 (100.0%) | 1000/1000 | 100.0% | 100.0% | 100.0% | 34 |
| 1001-2000 | 1000 | 1000/1000 | - | - | 6286/6286 (100.0%) | 1000/1000 | 100.0% | 100.0% | 99.9% | 33 |
| 2001-3000 | 973 | 1000/1000 | - | - | 4921/4921 (100.0%) | 1000/1000 | 99.5% | 99.8% | 97.3% | 33 |
| 3001-4000 | 946 | 1000/1000 | - | - | 4522/4522 (100.0%) | 1000/1000 | 99.4% | 99.7% | 96.9% | 34 |
| 4001-5000 | 675 | 997/1000 | 4806 Other(1)->Title | - | 3493/3493 (100.0%) | 1000/1000 | 96.4% | 97.0% | 92.6% | 33 |
| 5001-6000 | 0 | 997/1000 | 5063 Other(2)->Other(1) | - | 3714/4071 (91.2%) | 705/1000 | 98.1% | 98.1% | 98.0% | 33 |
| 6001-7000 | 0 | 1000/1000 | - | - | 4924/5861 (84.0%) | 453/1000 | 97.2% | 97.2% | 97.4% | 34 |
| 7001-8000 | 0 | 1000/1000 | - | - | 4664/6348 (73.5%) | 270/1000 | 93.2% | 92.9% | 94.7% | 33 |
| 8001-9000 | 0 | 988/1000 | 8304 Title->Other(2) | - | 1431/2841 (50.4%) | 501/1000 | 96.9% | 96.9% | 97.2% | 33 |
| 9001-10000 | 0 | 1000/1000 | - | - | 4397/5808 (75.7%) | 329/1000 | 97.2% | 97.1% | 97.7% | 34 |
| 10001-11000 | 0 | 1000/1000 | - | - | 4736/6359 (74.5%) | 268/1000 | 94.6% | 94.4% | 95.5% | 33 |
| 11001-12000 | 0 | 996/1000 | 11932 Title->Other(2) | - | 3159/5813 (54.3%) | 95/1000 | 92.2% | 91.9% | 94.0% | 33 |

First divergences by field: none. Every compared field matches over the whole
4,303-frame window.

Reading the run:

* Buckets 1-1000 through 3001-4000 are clean: 100% object slot match and
  99.4-100.0% pixels. The title roll-in is gone (pixel sample 0 is 100%) and
  every default sample through frame 1740 is 100%.
* 4001-5000 is the last bucket with compared frames. Slot pairs stay at 100%
  and the first state mismatch is 4806, where the oracle has moved
  `Title -> Other(1)` while the port stays on `Title` until 4808. The port
  follows the oracle into `Title` at 4676, the same frame as the oracle.
* After play ends the only difference is the title/attract phase: the port
  reaches `Other(1)` and `Other(2)` 3-5 frames late at 4806-4808, 5063-5065,
  8304-8305, 8434-8438, 8691-8695 and 11932-11935. That is all 22 state
  mismatch frames; they are phase offsets, not a stuck port, and they sit
  outside the compared window. Object count and slot coverage match on every
  compared frame.
* The slot-pair and live-set columns after 5000 cover non-`Playing` frames, so
  they measure attract/debris bookkeeping: the port and oracle hold different
  title objects there, whereas the compared window is exact. Pixel match after
  5000 stays at 92.2-98.1%.

## Boss script, 0-12000 (both kept alive)

Run summary: 11,709 compared frames (window 292-12,000), 0 divergences.
Field pass rates: all compared fields 100%. Object slot match
53,989/53,989 (100.0%), live count delta 0 on 0 frames. Pixels: 97.5% overall
(play 97.4%, HUD 98.3%), first pixel difference at frame 150. There are 0
state mismatch frames and 0 object count mismatch frames.

| bucket | cmp | state | first state | first in window | slot pairs | live set | pixel | play | hud | samples |
| --- | ---: | ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1-1000 | 709 | 1000/1000 | - | - | 2920/2920 (100.0%) | 1000/1000 | 100.0% | 100.0% | 99.8% | 34 |
| 1001-2000 | 1000 | 1000/1000 | - | - | 5306/5306 (100.0%) | 1000/1000 | 100.0% | 100.0% | 99.7% | 33 |
| 2001-3000 | 1000 | 1000/1000 | - | - | 5704/5704 (100.0%) | 1000/1000 | 99.2% | 99.5% | 97.1% | 33 |
| 3001-4000 | 1000 | 1000/1000 | - | - | 5151/5151 (100.0%) | 1000/1000 | 99.4% | 99.8% | 96.7% | 34 |
| 4001-5000 | 1000 | 1000/1000 | - | - | 4960/4960 (100.0%) | 1000/1000 | 99.3% | 99.7% | 96.7% | 33 |
| 5001-6000 | 1000 | 1000/1000 | - | - | 6158/6158 (100.0%) | 1000/1000 | 98.7% | 99.0% | 96.7% | 33 |
| 6001-7000 | 1000 | 1000/1000 | - | - | 4367/4367 (100.0%) | 1000/1000 | 87.0% | 85.5% | 96.7% | 34 |
| 7001-8000 | 1000 | 1000/1000 | - | - | 2655/2655 (100.0%) | 1000/1000 | 91.7% | 90.7% | 98.3% | 33 |
| 8001-9000 | 1000 | 1000/1000 | - | - | 3000/3000 (100.0%) | 1000/1000 | 96.6% | 96.2% | 99.1% | 33 |
| 9001-10000 | 1000 | 1000/1000 | - | - | 1952/1952 (100.0%) | 1000/1000 | 99.6% | 99.6% | 99.7% | 34 |
| 10001-11000 | 1000 | 1000/1000 | - | - | 5214/5214 (100.0%) | 1000/1000 | 99.8% | 99.8% | 99.8% | 33 |
| 11001-12000 | 1000 | 1000/1000 | - | - | 6602/6602 (100.0%) | 1000/1000 | 99.2% | 99.2% | 99.1% | 33 |

First divergences by field: none. The state type and every simulation field
match for all 12,000 frames, and every bucket has 100% slot pairs and an
identical live set. The gated 7500-9700 window reports 0 divergences over 2,201
compared frames.

The early spawn race at 1357 and the `obj.x` drift from 6503 that the previous
measurement reported are gone. What remains is a picture-only difference in the
eruption wave (6001-8000 reads 87.0-91.7% pixels) even though the state and
objects are identical; it is a renderer/palette gap, not state drift.

## Section transitions and events

Default run, oracle timeline (`Other(1)` attract, `Other(3)` continue,
`Other(4)` stage init):

| event | oracle | port |
| --- | --- | --- |
| Title | 1-132 | 1-132 |
| Attract/Continue | 133-262 | 133-262 |
| Stage init blank | `Other(4)` 263-291, `Playing` from 292 | same |
| three deaths / re-entry | 2709, 3232, 3755, each `Other(4)` for 27 frames | same |
| after 0 lives | `Playing` to 4675 (`lives` 255 at 4278), Title at 4676 | same; Title at 4676 |
| attract/demo | `Other(2)` 5063-8303, second cycle 8691-11931, Title at 11932 | `Other(2)` 5066-8305, second cycle 8696-11935, Title at 11936 |

Every oracle death resets the scroll to the `$0400` checkpoint and blanks for 27
frames in `Other(4)`; the port does the same, and the play-to-`Title` handoff is
now the same frame (4676). The 22 state-mismatch frames listed above are the
later title/attract phase offsets, 3-5 frames late.

Boss run, event timeline:

| event | oracle | port |
| --- | --- | --- |
| scroll reaches `$0C00` | 6440 | 6440 |
| eruption scroll hold | `0:12:128` 6441-7210 | same |
| kind `$8A` eruption objects | 6445-7258, 5,374 object-frames | same |
| scroll reaches `$0D00` (stop) | 7722 | 7722 |
| Big Core spawn, slot 9 kind `$98` | 7723 | 7723 |
| `$99` cover pods, slots 7/8 | 7723-9086 | 7723-9086 |
| first damage +50 | 8066 (state line 8067) | 8066 |
| core death, +1000 | 9087, score 2690 | 9087, score 2690 |
| post-clear slot 9 | kind 2 through 9117, cleared 9118 | same |
| section reset | 9601, scroll `0:14:128` → `1:0:0` | same |
| stage 2 | runs from 9601 | same reset; state matches to 12,000 |

The eruption wave matches on both sides, and so does the boss: spawned,
damaged and cleared correctly, the kind-2 debris runs to 9117 and is cleared at
9118, and both sides reset the scroll at 9601 for stage 2. The 7500-9700
release-gate window reports 0 divergences across 2,201 compared frames, and
the whole 12,000-frame run has 0 state and 0 object-count mismatches. The port
follows stage 2 from 9601 to 12,000 with matching scroll, score and object
kinds, but stage 2 has no gate of its own.

## Root causes and remaining items

### 1. Default-run play is exact

The object drift from 3159 is gone: every compared frame has a 100% object slot
match and 0 field divergences over the 4,303-frame window. The default run now
reports 0 divergences, so only the state handoff outside play is left.

### 2. Boss run is exact

The spawn race at 1357 and the `obj.x` drift from 6503 are gone. The boss run
has 0 divergences, 0 state mismatches and 0 object-count mismatches over 11,709
compared frames; the gate window stays at 0.

### 3. Default-run game-over/attract phase offsets

The port enters `Title` at 4676 on the same frame as the oracle, but then runs
`Other(1)` and `Other(2)` 3-5 frames late. That is all 22 state-mismatch frames,
first at 4806 (`Title -> Other(1)`; the port stays on `Title` until 4808), not
a stuck `GameOver`. These frames sit outside the compared window; the release
gate now WARNs on the non-zero count instead of calling the run clean.

## Fixed since the pre-fix audit

The two boss-window blockers are closed. Both sides now turn the dead core into
kind-2 debris at 9087, keep it through 9117 and clear slot 9 at 9118, and both
reset scroll `0:14:128` → `1:0:0` at 9601 when stage 2 begins. The old audit
measured 282 divergences through 9600 and 712 in the 7500-9700 gate; the gate
is now 0. The default object drift from 3159 and the boss pre-window drift at
1357/6503 are also closed.

## Smaller observations

* The title/attract picture matches at the 30-frame sampling: sample 0 is 100%
  in both runs, and in the default run every sample through frame 1740 is 100%.
  The first picture difference is 0.1% at frame 150 in the boss run (an 8x16
  attract ship/cursor box, `95,134-102,149`) and at frame 1620 in the default
  run (single pixel `74,238-74,238`).
* `obj.slot` shows `0/0 (-)` in both reports because the comparator only
  records failures for that field; use the slot-pair column above for the rate.
* Pixel match is a weak signal even when state and objects agree: the
  6001-8000 boss buckets read 87.0-91.7% while every field and object matches,
  because the eruption wave renders different sprite pixels over shared
  background art.
* The scene is not audio-checked, and no per-frame PNGs are kept. Add
  `--png-dir DIR` with `--png-every 30` if a picture dump is needed for the
  frames named above.
