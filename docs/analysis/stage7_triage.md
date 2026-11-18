# Stage 7: triage of the divergences (11,512 before the `$02` fix, 604 after)

Analysis only: no core, fixture or script change. The 11,512-divergence
baseline was measured on 2026-11-11 from `main` `7e9081d` with the checked-in
`tests/fixtures/stage7-297-6050.state` and the replay in
`crates/gradius-diff/tests/stage7_triage.rs`; the 604 count is from `main`
`6e7fe5c` after the `$02` anchor seed landed.

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-s7triage CARGO_BUILD_JOBS=2
cargo test -p gradius-diff --test stage7_triage stage7_divergences_group_by_cause -- --nocapture
```

The window is 297-6050 (5,754 compared frames).

## Result after the `$02` anchor seed

`fa67d3f` added `Game::new_at_stage_seeded(assets, stage, nmi_seed)`, which
stores the seed in `nmi_frame`; `new_at_stage` keeps the power-on `0xFC`. The
harness reads the oracle's `$02` at the anchor and passes it less one, so the
first compared frame reads the cartridge's value (`0x23` for the 297 anchor),
the stage replays pass the same seed, and the shield ring reads
`(nmi_frame >> 2) & 3`.

The replay now reports **604 divergences**, down from 11,512. The first is frame
**5627 `obj.y`** (was 4573 `obj.slot`), and every remaining divergence is that
`obj.y` walker group (frames 5627-5863, below): `obj.slot`, `obj.kind`, `obj.x`
and `obj.substate` are now 0. The turret fire grid is no longer part of the
residue.

Stages 1-3 are unchanged: their replays stay exact, 19429/19430 stay at 0 px,
the stage-2 boss hand-off stays exact and the stage-1 99.9 gate is intact.

## Divergences by field (before the `$02` fix)

| field | divergences |
| --- | ---: |
| obj.kind | 2335 |
| obj.x | 3171 |
| obj.y | 2677 |
| obj.substate | 2226 |
| obj.slot | 1103 |
| px, py, speed, scroll, score, lives, meter | 0 |

The ship, scroll, score, lives and meter are exact over the whole window. Every
remaining divergence was an object-table difference.

## Frame classes: the same objects, or different ones? (before the fix)

For each diverging frame the two object lists are compared as multisets of
`kind:substate:x:y`:

| class | frames | field total | what it means |
| --- | ---: | ---: | --- |
| population differs | 592 | 6059 | the two sides do not hold the same objects (extra/missing/slot reuse) |
| same kinds, state differs | 369 | 2327 | the same kinds are live, but position/substate disagree |
| slot permutation | 330 | 3126 | **identical objects in different slots** |

The last class is not a simulation difference at all: every object exists at
the same position on both sides and only its slot index differs. It appears
from 5297 to 5626, i.e. after the population drift, and is its consequence.
Both classes disappear after the seed: the post-fix window's only class is
`same kinds, state differs` on `obj.y` (237 frames, 5627-5863).

## Root cause: the `$02 & $0F` turret fire grid is 9 frames early (fixed)

This was the cause of 9,185 of the 11,512 divergences; the anchor seed above
removed it. The section is kept as the pre-fix measurement.

Frames 4573-4581, native slot 5, kind `$8C` at `(246,32)`, absent in the
oracle. The object is a kind-`$0C` diver shell:

* the kind-`$90` shooting turret is identical on both sides: it appears at
  frame 4570 on slot 8 (`k144 @239,40 s1`) and drifts left;
* `ai_turret_fire` (`$AF28`/`$AF82`, ROM `$AF92`) fires a `shot = 12` child at
  `(x + 8, y - 10)` only when `gate && nmi_frame & 0x0F == 0`;
* `ai_diver` (`$B3C5`) then sets `kind |= 0x80` (making `$8C`), hovers 20
  frames and dives 2 px/frame.

Measured shell births (slot, frame):

| burst | native | oracle | offset |
| --- | --- | --- | ---: |
| 1 | 4573, 4589, 4605, 4621 | 4582, 4598, 4614, 4630 | +9 |
| 2 | 4733, 4749, 4765, 4781 | 4742, 4758, 4775, 4783 | +9 |

Same 16-frame cadence, constant 9-frame phase: the port fires the first shell
of every burst 9 frames early. Both turrets spawn on the same frame, at the
same X, so the only input left is the `$02` NMI counter: the port's
`nmi_frame` is 9 ahead (mod 16) of the ROM's `$02` at this point.

Because `nmi_frame` starts at `0xFC` (`Game::new`) and only holds on modelled
lag frames (`nmi_lag`, set once by `$9B38`), the first reading of this gap was
**nine `$02` lag frames of the stage-7 run that the port does not model**
(`$8073`: the previous handler overran and `$02` holds). The direct measurement
below refutes that: both counters increment on every compared frame and the
nine frames are the power-on seed not being rebased at the `--stage` anchor.

Consequences, all from this one phase error:

* shells live and dive 9 frames early: `x` 527, `y` 90, `substate` 43,
  `native extra` 48, `native missing` 39 divergences on kind `$8C` = **747**;
* each shell releases its slot 7-9 frames early, so later formations allocate
  other slots; the port then shows a different object in a used slot:
  `obj.kind`/`obj.substate` pairs (e.g. `146`/`147`, `135`/`146`,
  `132`/`135`, `4`/`132`) and, once the sets coincide again, pure
  **slot permutations**: **3,126** more divergences.

Combined, the population class plus the slot-permutation class is
**9,185 of 11,512 (79.8 %)** divergences and 922 of 1,291 diverging frames.

### Fix (landed)

`fa67d3f` adds `new_at_stage_seeded`: the `0xFC` init stays for a run booted at
frame zero, and a `--stage` run seeds `nmi_frame` from the cartridge at the
anchor. The measurement below ruled out missing `$8073` lag frames before the
card; because `new_at_stage` keeps `0xFC`, no other stage's `$02 & 1` gate
moves.

## Measured: the `$02` offset is the power-on seed at the `--stage` anchor

Measured on 2026-11-11 in `agent/s02`. `$02` (`$03` too) was added to the
oracle dump as trailing `nmi=`/`mode=` fields in
`crates/gradius-diff/src/dump.rs`; they are optional, so every existing fixture
still loads and round-trips. The port has no public `nmi_frame`, so its column
was read with a temporary `nmi=` append in `dump_state` (reverted before the
commit) and is checked against the observable turret grid. Reproduce:

```sh
cargo test -p gradius-diff --test stage7_nmi    # needs GRADIUS_ROM + the pack
```

### The two sequences

| frame | oracle `$02` | port `$02` | oracle − port |
| ---: | ---: | ---: | ---: |
| 297 | `0x24` (36) | `0xFD` (253) | 39 |
| 298 | 37 | 254 | 39 |
| 299 | 38 | 255 | 39 |
| 300 | 39 | 0 | 39 |
| 310 | 49 | 10 | 39 |
| 4570 | 213 | 174 | 39 |
| 4572 | 215 | `0xB0` (176) | 39 |
| 4573 | `0xD8` (216) | `0xB1` (177) | 39 |
| 4581 | `0xE0` (224) | 185 | 39 |
| 4630 | 17 | 234 | 39 |

Exact for every compared frame:

* oracle: `$02(F) = (F − 5) & 0xFF` — verified on all 5,754 frames;
* port: `$02(F) = (0xFC + F − 296) & 0xFF = (F − 44) & 0xFF`;
* `oracle − port = 39` on all 5,754 frames (the 4630 row wraps:
  `17 − 234 = −217 ≡ 39`).

`39 ≡ 7 (mod 16)`, so the port hits any `& 0x0F` phase nine frames early. The
measured grids: oracle `$02 & 0x0F == 0` at 4581, 4597, 4613, 4629; port at
4572, 4588, 4604, 4620. First `$8C` shell birth (slot 5): oracle 4582, native
4573. The offset is **constant from the first compared frame**, not something
that accumulates.

### Origin: the seed, not `$8073`

* The oracle's `$02` holds on only four boot frames (1→2, 2→3, 4→5 and
  265→266) — that is the `−5`; from 297 on it increments on every frame. The
  ROM has **no lag frame** in the compared window.
* The port sets `nmi_lag` only on `Phase::StageInit` (`$9B38`) and on a
  respawn. `new_at_stage` enters `Phase::Normal` directly and `dev_keep_alive`
  removes the respawns, so the port also increments on every step (the native
  column has no hold).
* Therefore no `$02` frame is missing. The whole offset exists at creation: at
  oracle frame 296 the ROM holds `296 − 5 = 291 = 0x23` and the port holds the
  seed `0xFC = 252`, i.e. `252 − 291 = −39` counters. Both then count at the
  same rate, so the gap stays 39.
* Decomposed: the `--stage` anchor skips 295 increments (`296 − 1`), and
  `295 ≡ 39 (mod 256)`, `295 ≡ 7 (mod 16)`. `0xFC` is the power-on value that
  suits a run booting at oracle frame 0; it is never rebased when
  `new_at_stage` starts at 297.

### Same family as the shield-ring quarter

The ring phase error in `docs/analysis/shield_ring.md` is a second face of the
anchor problem. The ring reads `(self.frame − 5) >> 2 & 3` while the native
frame is 296 behind the compared frame, so its counter is 296 low;
`296 ≡ 8 (mod 16)` and `8 / 4 = 2` quarters, exactly the documented `+2` index
(12-px sparkle swap) at stage 5. A correctly seeded `nmi_frame` read by the
ring `$8B82-$8B8B` block would fix both symptoms with one counter. The stage-7
`$02` itself is the same on both sides (`0x24` at 297) whether the run pokes
`$26` for stage 5 or 7, because both boot from power-on and reach 297 with the
same four holds.

### The rule

With `A` the anchor frame the harness creates the native game at (native frame
1 = oracle 297 in the checked-in fixture), and `$02(A)` the cartridge counter
read there:

```
nmi_frame = $02(A).wrapping_sub(1)
```

`step` increments before the frame body, so the first compared frame reads
`$02(A)`. `nmi_lag` stays the `$8073` hold (`$9B38` and the respawn); this
window needs no extra hold. Every `$02` consumer reads `nmi_frame` — the `$0F`
turret gate, `$02 & 1` stage gates, the `$02`-skewed cannon aim and the shield
ring (`(nmi_frame >> 2) & 3`). No per-stage constant: a run that lags after
the anchor has to carry the oracle's `$02` per frame, which the optional
`nmi=` field makes possible.

Origin in one line: `0xFC` is the power-on seed for a run booted at oracle
frame 0, and `new_at_stage` never rebases it, so every `--stage` run starts
−39 counters (`295 = 296 − 1` skipped increments, `295 ≡ 39 (mod 256)`).

Evidence field: the oracle dump writes `nmi=` (`$02`) and `mode=` (`$03`) as
optional trailing fields after `objects=`, and a line without them parses as
`None`, so every existing fixture still loads and round-trips unchanged
(`dump.rs::nmi_and_mode_are_trailing_and_optional`). The native line still
omits both because `Game::nmi_frame` is private; the port column above was
read with a temporary `nmi=` append, reverted before the commit.

### Rule for the core card (implemented)

1. **Seed the counter from the cartridge at the anchor.** The harness knows
   the anchor frame `A` (native frame 1 = oracle 297 for the checked-in
   fixture) and reads `$02` there. `step` increments before the frame body, so
   `new_at_stage_seeded(assets, stage, nmi_seed)` takes
   `nmi_seed = $02(A).wrapping_sub(1)`; the first compared frame then reads
   `$02(A)`. For stage 7: `0x24`, i.e. seed `0x23`.
2. **Keep the hold rule.** `nmi_lag` already models `$8073`; this window needs
   no extra hold. Do not bake a per-stage constant: a stage whose run lags
   after the anchor needs the harness to carry the oracle's `$02` (or a lag
   count) per frame, not a fixed seed.
3. **Every `$02` consumer reads `nmi_frame`**, including the shield ring
   (`(nmi_frame >> 2) & 3`), not `frame − 5`.

### Test the fix

`crates/gradius-diff/tests/stage7_nmi.rs` pins the measured sequences (gated on
`GRADIUS_ROM`): the oracle's `$02(F) = (F - 5) & 0xFF` and the seeded native
`(0x23 + frame - 296) & 0xFF` agree on every compared frame, and both `$8C`
shells are born at 4582. The replay uses
`Game::new_at_stage_seeded(assets, 6, 0x23)` and now reports 604 divergences,
the first at 5627 `obj.y`.

## Independent group: the kind-`$87`/`$93` walkers miss floor and ceiling

After the seed the only frame class left is `same kinds, state differs` on
`obj.y`: frames 5627-5863, 237 frames, **604 divergences**, split between two
walker kinds (323 on `$87`, 281 on `$93`). Same kind, same X, same substate;
only Y diverges.

| frame | slot | oracle | native |
| ---: | ---: | --- | --- |
| 5627 | 8 | `k135 (145,157) s152` | `k135 (145,160) s152` |
| 5629 | 7 | `k147 (143,41) s144` | `k147 (143,38) s144` |
| 5654 | 4 | `k147 (131,41) s168` | `k147 (131,38) s168` |
| 5663 | 4 | `k147 (140,41) s168` | `k147 (140,11) s168` |
| 5863 | 4 | `k147` still at Y 41 | Y 11 |

Kind `$93 & $7F = 19` is the ceiling walker `$B741`. It probes
`solid_at(x, y-8)` (`$B74F`): solid -> step/align, empty -> `y -= 3` every
frame. The oracle finds a ceiling at Y 41 (probe row 33) and the port does not,
so the native climbs. Kind `$87 & $7F = 7` is the ground walker `$B6DB`, the
mirror image: the oracle holds Y 157 on the floor and the native falls to 187.
Both use `ai_walker`, and the walker formula matches elsewhere, so the residue
is a **hole in the stage-7 collision map** at the floor and ceiling rows those
walkers probe (samples X 131-151).

### Fix

Correct the `stage7_collision` entry (or wherever the stage-7 nametable solids
come from) for those columns and rows, then recheck both walkers. Effort:
small, data only. This is now the only group left in the fixture.

The measurement below pins it down: the port **does** probe the same cell as
the ROM; the extracted map is missing the whole last 32-column block (world
columns 352-383). See "Measured: which cell the first walker misses".

## Measured: which cell the first walker misses

Measured 2026-11-12 in `agent/walkers` with
`crates/gradius-diff/tests/stage7_walkers.rs` (the oracle half is gated on
`GRADIUS_ROM`). The fixture is the same checked-in window, so the oracle Y
below is also in `tests/fixtures/stage7-297-6050.state`.

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-walkers CARGO_BUILD_JOBS=2
GRADIUS_ROM=$GRADIUS_ROM \
  cargo test -p gradius-diff --test stage7_walkers -- --nocapture
```

### Frame by frame (first walkers, 5618-5635)

| frame | slot 8 `$87` oracle/native | slot 7 `$93` oracle/native |
| ---: | --- | --- |
| 5618-5626 | 157 / 157 | 41 / 41 |
| 5627 | 157 / **160** | 41 / 41 |
| 5628 | 157 / 163 | 41 / 41 |
| 5629 | 157 / 166 | 41 / **38** |
| 5630 | 157 / 169 | 41 / 35 |
| 5631 | 157 / 172 | 41 / 32 |
| 5632-5635 | 157 / 175..184 | 41 / 32 |

The separation is the ground walker at **5627**: one 3-px step down, then one
per frame until Y 184, where it holds. The ceiling walker follows at **5629**
and climbs to 32. Both then move left and later drop again: by 5720 the ground
walker (also slot 2, oracle pinned at 157) is at 250, the ceiling slot 4
(starts at 5654, oracle pinned at 41) reaches 0 by 5670, and at 5863 slot 4 is
still `oracle 41 / native 0`. The oracle holds every walker; the native ones
only pause when their X carries them onto a column the extracted map does
have.

### What each handler does (ROM)

* `$B6DB` ground walker (kind `$07`, `$87` live). Probe `$A4=$036C`
  (object X), `$A5=$032C+8` (object Y + 8); `JSR $C3CD`:
  empty -> load `$03` and `JSR $B705` (velocity `+3` px/frame); solid -> drop
  `$A5` three times (Y + 5) and probe again: solid -> `$FD` (`-3` px/frame),
  empty -> fall through to `$B670` with no vertical move.
* `$B741` ceiling walker (kind `$13`, `$93` live). Probe `$A5=$032C-8`
  (object Y - 8); `JSR $C3CD`: empty -> `$FD` (`-3` px/frame); solid -> raise
  `$A5` three times (Y - 5) and probe again: solid -> `$03` (`+3`), empty ->
  `$B670` with no vertical move.
* Both probe X unchanged and let `$C3CD` add 8 and `$3E`, and compute
  row `(probe_y + $14) >> 3`; `$3F` plus the add's carry selects page
  `$05`/`$06`. `ai_walker`/`terrain_solid_in` in the port implement exactly
  this (`x + 8 + scroll`, `(y + 0x14) / 8`, same masks `03 0C 30 C0`), so the
  handler and probe formula are **not** the defect.

### The exact cell of the 5627 step

Before its 5627 step the ground walker is `x=144, y=157`; `$3E=$69`,
`$3F=$0A`. `$C3CD` sees `144 + 8 + $69 = $0102`: low column `$00` with carry
1, so page `$0600`, group 5, row 23, mask `$C0`. The oracle's RAM at that
frame is `$0605 = $40` -> solid -> then row 22 (`$A5` 162) `$40 & $30 = 0`
-> empty -> no vertical move, Y stays 157. The port's `COLLISION_MAP` at the
same world column (`(144+8+$0A69)/8 = 352`) is zero for all eight groups, so
it takes the empty branch and adds 3.

The ceiling walker at 5629 is the same cell one group up: `x=142`, `$3E=$6A`,
`142+8+$6A = $0100` -> page `$0600`, column 0; row 6 (`Y-8=33`) mask `$30`,
oracle `$0601 = $15` -> solid; row 7 (`Y-5=36`) mask `$C0`, `$15 & $C0 = 0`
-> no vertical move, Y stays 41. Port map `[352*8+1] = 0` -> climbs.

### Is the map faithful at that cell? No

Block by block, the checked-in `stage7_collision::COLLISION_MAP` holds
non-zero bytes for blocks 2-10 (world columns 64-351) and **0 of 256 bytes**
for block 11 (columns 352-383) and for 12-13 (outside the 384-tile stage).
The oracle page and the displayed nametable both say block 11 has terrain:

| source | col 352, group 1 (rows 4-7) | col 352, group 5 (rows 20-23) |
| --- | --- | --- |
| oracle `$0600` at 5627 | `$15` (rows 4,5,7 set, 6 clear) | `$40` (row 23 set) |
| port extracted map | `$00` | `$00` |
| `xtask screens --stage 7` nametable palettes rows 5/6/7 | 3/2/0 | rows 22/23/24 = 2/2/3 |

So the hole is **extraction data, not the ROM and not the probe**: the
extraction (`/tmp/opencode/collmap-extract`) maps `world = $3F*32 + column`
with first-value-wins. Page `$0600` is shared by blocks 9 and 11; block 11 is
only valid after its reload, which happens when `$3F` reaches 11 at scroll
2816 (oracle frame 5929 in the fixture). The extractor stopped at/just before
that boundary, after having already recorded the pre-reload zeros for block
11.

### Rule for the core card

1. Re-extract stage 7's collision map so world columns 352-383 carry the
   `$0600` page content from after the block-11 reload (scroll >= 2816, i.e.
   `$3F = 11`). Use last-value-wins from the reload frame (or only record a
   block once its page owner matches); do not keep the stale pre-reload
   zero. Blocks 0-1 (open space) stay zero; blocks 12-13 are outside the
   384-tile stage.
2. Do not change `ai_walker` or `terrain_solid_in`: both already match
   `$B6DB`/`$B741` and `$C3CD` byte for byte.
3. Test it: a `stage7_collision` unit test pins the measured page, e.g.
   `COLLISION_MAP[352*8+1] == 0x15` and `COLLISION_MAP[352*8+5] == 0x40`
   (plus `block 11` non-zero), and `stage7_replay` becomes the exact gate:
   0 divergences, with the first falling walker holding Y 157 from 5627 and
   the ceiling walkers holding Y 41 through 5863. The gradius-diff probe
   `stage7_walkers.rs` records the current (pre-fix) separation for
   comparison; its assertions flip when the data lands.

## Clusters that were pending the turret fix: gone

The 5161-5464, 5305-5354 and 5552-5798 clusters were the shell cascade's slot
permutations. After the seed they no longer appear: the window's only class is
`same kinds, state differs` on `obj.y` (237 frames, 5627-5863). No further
triage is needed for them.

## Outside the fixture: the gap 6051-6562 and the clear's carried `+10`

The fixture ends at 6050, before the clear opens, so this window was measured
separately on `main` at `05400fc` with the fire replay
(`stage7_clear_replay`, which starts at the 297 anchor and holds fire every
frame) and with `record-ending`:

* **No score event lives in 6051-6562.** At 6051 the oracle already reads
  **830** and the port **840**, and both stay constant through 6562; neither
  side scores after **5944**.
* Every simulated field over those frames (`px`/`py`/`scroll`/`speed`/
  `lives`/`meter`/`obj.kind`/`obj.x`/`obj.y`/`obj.substate`) is exact. Only
  `score` and `obj.slot` are carried, the latter from **6243**.
* The first real divergence of the fire replay is frame **390**: the port
  kills the slot-9 scout, kind `$85`, at (116,86), and the oracle keeps it,
  killing a different scout at **406**. The net is one extra 10-point kill in
  the port, which is the `+10` (840 vs 830) already present at 6051.
* Death times move ±30 frames when the score delta is changed, so this is the
  **death-timing rule**, not a single object: there is no stray object to fix,
  and the residue is **ACCEPTED** in `docs/KNOWN_DEVIATIONS.md` (the stage-7
  state window 297-6050 is exact and the clear 6563-7024 is exact in
  `px`/`py`/`scroll`).

## Recommended order for the next cards

1. **Stage-7 collision block 11** (604 divergences, all in 5627-5863). Small
   data fix: re-extract world columns 352-383 after the page reload; the
   walkers already probe the right cells.
2. **Re-run this triage** (and `stage7_replay`): with the walkers fixed the
   fixture should reach 0 divergences, which turns the report-only replay into
   an exact gate.

The turret `$02` grid card is done. Do not card the slot-permutation class on
its own: it dissolved with the seed.
