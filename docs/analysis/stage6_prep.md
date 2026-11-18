# Stage 6 clear: measured data to wire the no-core end path

Analysis only: no `gradius-core`, `gradius-diff`, `scripts` or `xtask` change.
Everything below was re-measured on 2026-11-11 on `Gradius (E).nes` from this
worktree, and extends `stage67_boot.md`/`ending.md` with the clear-sequence
bytes, the first-1000-frame spawner and the `$CE2B` wall decode. Where a number
here disagrees with the earlier notes it is called out.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-s6prep CARGO_BUILD_JOBS=2
cargo build --release -p xtask -p gradius-diff
xtask extract "$ROM" /tmp/opencode/s6prep.pak        # prints the two stage tables
xtask record-ending "$ROM" --stage 6 --frames 8000 --out /tmp/opencode/s6prep-ending6.txt
gradius-diff --rom "$ROM" --pack /tmp/opencode/s6prep.pak --stage 6 \
  --frames 1100 --from 260 --to 1099 --keep-alive \
  --oracle-out /tmp/opencode/s6prep-oracle6-fire.txt
gradius-diff ... --script /tmp/opencode/s6prep-nofire.txt   # start only, no fire
xtask screens "$ROM" --frames 7400 --press 180:start --stage 6 \
  --dump /tmp/opencode/s6prep-stage6.grd --sample-scroll 1000 --sample-scroll 2800
```

`record-ending`/`gradius-diff` print the scroll triple as **decimal** bytes
(`low:high:frac`), so `scroll=1:11:128` is `$3E=1, $3F=$0B, $3D=$80` = 2817.5 px.

## 1. The clear sequence without a core

Stage 6 starts at 265 (`$19 = 5`, normal play at 292). The end path is
`$81 -> $82 -> $83 -> $86 -> $90`, with no `$84`/`$85` and no kind `$98`.
Measured frame by frame (`s6prep-ending6.txt`; one row per change):

| frame | `$1B` | `$3F` | `$3E:$3D` | `$66` | event |
| ---: | ---: | ---: | ---: | ---: | --- |
| 5927 | `$80` | `$0A` | `$FF:$80` | 172 | last normal frame before the threshold |
| 5928 | `$80` | `$0B` | `$00:$00` | 172 | `$3F` reaches `$9A30[5] = $0B` |
| 5929 | `$81` | `$0B` | `$00:$80` | 172 | `$9A01`: clear timer armed, `$1B = $82` next |
| 5930 | `$82` | `$0B` | `$00:$80` | 0 | `$82` window starts; `$66` cleared |
| 5933 | `$82` | `$0B` | `$00:$80` | 0 | first beam, slot 9, first sample `(148, 69)` |
| 6953 | `$82` | `$0B` | `$00:$80` | 0 | last of the 1024 `$82` frames |
| 6954 | `$83` | `$0B` | `$00:$80` | 0 | 16-bit timer reached zero |
| 6955 | `$86` | `$0B` | `$00:$80` | 0 | `$99B3`: stage 6 requests `$AC` |
| 6956 | `$86` | `$0B` | `$01:$00` | 2 | scroll resumes; first wall erase pair |
| 6999 | `$86` | `$0B` | `$16:$80` | 88 | `$66` reaches `$58`, last erase |
| 7466 | `$86` | `$0C` | `$00:$00` | 88 | `$3F` reaches `$98F0[5] = $0C` |
| 7467 | `$90` | `$0C` | `$00:$80` | 88 | one frame of `$96C4` (stage advance) |
| 7468 | `$80` | `$00` | `$01:$00` | 0 | `$19 = 6`, stage 7 normal play |

Exact `$3F` byte sequence: `0A -> 0B` at 5928, held `0B` for 1538 frames
through 7465 (the `$82` part is the frozen 1024), `0B -> 0C` at 7466, then
reset to `0` by the stage-7 init in 7468. The `$81/$82` window is inside normal
play: the scroll is frozen at `$3F:$3E:$3D = 0B:00:80` (2944.5 px) from 5929
until 6955, and the spawner, object AI and collisions keep running (the beam
list changes every frame).

What each handler does (`xtask disasm`; `ending.md` has the same mapping):

* `$81` (`$9A01`): `X = $17`; stage 7 (`$19 == 6`) overrides the timer with
  `$4D = 1, $4C = $CA`; otherwise `$4D = $9A28[$17]` and `$4C = 0`. `INC $1B`,
  `$62 = 1`, then `$99D2` clears `$63-$6F`. Stage 6's `$17 = 4` in this run, so
  the timer is `$04:$00` = 1024 frames.
* `$82` (`$99DC`): `INC $5B`; `$8411` decrements the 16-bit `$4D:$4C`; at zero
  `INC $1B` (to `$83`). While `$1B == $82`, `$A2EA` sends the frame to `$C40D`,
  which spawns the `$C6D8` beams every 4th frame. Sfx `$3F` is requested only
  when `$19` is 0 or 3, so neither stage 6 nor 7 plays it here.
* `$83` (`$99B3`): `INC $1B`; for `$19 >= 5` it skips `$84`/`$85` and the
  `$98` core entirely: stage 6 (`$19 == 5`) calls `$EC18` with `$AC`, then sets
  `$1B = $86`, `$62 = 2` and clears `$63-$6F` again.
* `$86` (`$98F7`): stage 7 jumps straight to `$9865`; stage 6 calls `$CD9F`
  every frame (the `$CE2B` wall erase, guarded at `$58`), does its music check
  at `$9907`, and compares `$3F` with `$98F0[$19]` (`$0C` for stage 6).
  When `$3F >= $0C` it sets `$1B = $90` (`$8E` only if `$39 != 0`).
* `$90` (`$96C4`): `INC $19`, `INC $3A`, `$3F = 0`, stage init. This is why the
  first stage-7 frame already samples `$3E = 1` (see below).

Measured boundary state (the target the port must match):

* At the clear trigger (7466/7467): `$19 = 5`, `$1B = $90`, `$3F = $0C`,
  `$66 = 88`, score 1690, lives 3.
* At 7468, the first stage-7 play frame: `$19 = 6`, `$1B = $80`,
  scroll `$3E:$3F:$3D = 1:00:00` (**not** `0:00:00`), `$66 = 0`. `$66 = 9` on
  7469 and later: that is stage-7 scratch, not a leftover erase index.

The beam window: 256 spawn attempts 5933-6953 (one every 4th frame), at most
10 live boxes, X 152 -> -4/frame, lanes 66 (down) and 132 (up). Spawning stops
at 6954 but the beams keep moving through `$86`; the last live beam is at 6989
(one box), and the score rises in +10 steps while they are shot (last +10 at
6944). So `$86` must not clear the collision slots on entry.

## 2. Spawner, first 1000 frames

Reference run: default `gradius-diff` script (Start at 180/240, fire from 300,
`--keep-alive`), so slots show the shot-down debris. `slot` is the object-table
index, `X`/`Y` are the values on the first frame the kind is non-zero; `scroll`
is the integer world scroll at that frame.

| frame | kind | slot | X | Y | scroll px |
| ---: | --- | ---: | ---: | ---: | ---: |
| 294 | `$85` | 9 | 240 | 42 | 0 |
| 305 | `$85` | 8 | 240 | 42 | 4 |
| 316 | `$85` | 7 | 240 | 42 | 10 |
| 327 | `$85` | 6 | 240 | 42 | 15 |
| 338 | `$85` | 5 | 240 | 42 | 21 |
| 371 | `$02` | 9 | 92 | 42 | 37 |
| 380 | `$02` | 8 | 96 | 42 | 42 |
| 387 | `$02` | 7 | 104 | 42 | 45 |
| 391 | `$02` | 6 | 118 | 42 | 47 |
| 402 | `$02` | 5 | 118 | 42 | 53 |
| 417 | `$01` | 5 | 111 | 42 | 60 |
| 425 | `$85` | 9 | 240 | 160 | 64 |
| 436 | `$85` | 8 | 240 | 160 | 70 |
| 447 | `$85` | 7 | 240 | 160 | 75 |
| 458 | `$85` | 6 | 240 | 160 | 81 |
| 469 | `$85` | 5 | 240 | 160 | 86 |
| 553 | `$85` | 4 | 240 | 42 | 128 |
| 564 | `$85` | 3 | 240 | 42 | 134 |
| 575 | `$85` | 2 | 240 | 42 | 139 |
| 586 | `$85` | 1 | 240 | 42 | 145 |
| 597 | `$85` | 9 | 240 | 42 | 150 |
| 681 | `$85` | 8 | 240 | 160 | 192 |
| 692 | `$85` | 7 | 240 | 160 | 198 |
| 703 | `$85` | 6 | 240 | 160 | 203 |
| 714 | `$85` | 5 | 240 | 160 | 209 |
| 725 | `$85` | 9 | 240 | 160 | 214 |
| 745 | `$84` | 4 | 240 | 56 | 224 |
| 748 | `$84` | 3 | 240 | 104 | 226 |
| 751 | `$84` | 2 | 240 | 152 | 227 |
| 809 | `$84` | 1 | 240 | 56 | 256 |
| 873 | `$84` | 8 | 240 | 56 | 288 |
| 876 | `$84` | 7 | 240 | 104 | 290 |
| 879 | `$84` | 6 | 240 | 152 | 291 |
| 937 | `$84` | 9 | 240 | 56 | 320 |
| 940 | `$84` | 5 | 240 | 104 | 322 |
| 943 | `$84` | 4 | 240 | 152 | 323 |

Reading the table:

* The `$85` waves start at scroll **0, 64, 128, 192 px** (every 64 px, 128
  frames at the normal 0.5 px/frame): 5 members, 11-frame gap, X always 240,
  Y 42/160 alternating by wave. The 5 slots are 9..5 except where the previous
  wave's debris still occupies them (553 starts at slot 4).
* The `$84` waves start at scroll **224, 256, 288, 320 px** (every 32 px, 64
  frames): 3 members 3 frames apart, X 240, Y 56/104/152. At 809 only **one**
  member exists because the object table was full: see §3.
* The `$02` rows are kind-2 debris from `$85` ships shot down by the reference
  script, not stream records; a no-fire run has none of the 371-402 rows and
  different `$02` rows later (833, 961, 1025) when ships fly off-screen.
* 417 is not a new spawn: slot 5 changes `$02 -> $01` in place at
  `(111, 42)` (a dying-ship/effect morph). Do not put it in the spawner stream.
* Transient `$04 <-> $84` kind changes at x~108 (813-820, 877-878, 941-948)
  are the same objects changing handler/effect; they are not spawns either.

## 3. The `$85` wave and the formation gate

`stage67_boot.md` says spawns after ~740 depend on the formation gate and that
`$A325` waits for a free slot. The measurement corrects the second half:

* **Spawn frames do not depend on kills.** Fire vs no-fire runs produce the
  same wave-start frames and the same kind/Y sequence up to 943 (and the same
  `$84` starts at 1066/1069/1072 with kind `$88`): 294/425/553/681, 745/809/873/937.
  Only the slot indices differ, because debris occupies slots in the fire run.
* **The gate is a gap timer, not a free-slot wait.** `$A325` reads `$6C` (the
  movement-entry gap), decrements it and returns; the free-slot search is
  `$A3DE` -> `$A411`, and when no slot is free `$A3B5` returns *without*
  spawning that member. So a full table **drops a member**, which is why the
  809 `$84` formation only shows one of its three ships (slots 5-9 held the
  `$85` wave). Kill timing therefore changes *which members exist and in which
  slot*, never the wave frame.
* **Pin by scroll, not by frame.** The wave starts are at fixed world X
  (64 px for `$85`, 32 px for `$84`); the frame follows only because normal
  play scrolls at 0.5 px/frame. Any port that changes the scroll rate or adds a
  pause must key the spawner on `scroll >= threshold` plus the `$6C` gap.
  The formation-gate state (`$69` count, `$6C` gap) must also survive the
  `$82` freeze: no wave spawns during 5930-6953 because `$A2EA` routes `$82`
  to `$C40D`, not to `$A2BA`.

Wiring consequence: compare spawn *events* by (frame, kind, X, Y) and accept
slot differences caused by kills; do not gate on slot equality unless the
reference run uses the same kill pattern.

## 4. The `$CE2B` wall

Already in the pack: `xtask extract` reports `stage6_wall_erase (88 bytes at
$CE2B)` (chunk `STAGE_WALL_ERASE`, `stage6_wall_erase`); no extraction step is
missing. The table is 88 bytes at CPU address `$CE2B` in the ROM: one byte per erase
step, read in order by `$CDAD`. Each byte is a nametable tile code whose high
and low nibbles select the cell. The bytes are not reproduced here; read them
from the pack (`stage6_wall_erase`) or from the ROM at `$CE2B`.

What `$CD9F`/`$CDAD` do (disasm `$CD9F-$CE26`):

* `$CD9F`: if `$66 >= $58` return; otherwise call `$CDAD` twice. So two erases
  per frame, 44 frames, `$66` 0 -> 88 (measured 6956-6999, exactly 2/frame).
* `$CDAD`: read `tile = $CE2B[$66]`, `INC $66`; queue one `$0700` record
  `[1, addr_hi, addr_lo, 00, FF]` for
  `addr = $2400 + ((tile & $F0) << 1) + ($F0 | (tile & $0F))` (carry into the
  high byte); then mask two bits out of the attribute byte at `$9C` with
  `$CE27 = FC F3 CF 3F` indexed by `tile & 3`, and write it back. So each entry
  erases the tile and clears its attribute quadrant.
* Decode: **84 unique positions** in nametable rows **7-22**, columns **21-31**
  (16 rows x 11 cols = 176 cells; the table is a diamond, not a rectangle).
  Four positions appear twice: `(13,31)`, `(14,31)`, `(15,31)`, `(16,31)`.
  The 88 entries are: `(15,22) (16,26) (14,28) (11,26) (13,25) (19,27) ...`
  (order is the ROM order, not row/column order).
* Applied at frames 6956-6999 while the scroll runs 2817 -> 2838 px, the
  writes at screen columns 21-31 land on world X roughly **2988-3086 px**, i.e.
  the end-area wall just before the `$0C`/3072-3200 px stage end. It is
  presentation only: the wall is background tiles, and ship-vs-map collision
  uses the packed `COLLISION_MAP` / `$A65C` terrain, not the nametable.
* The 4 duplicate entries are harmless (same tile erased twice); pin the 88
  bytes and the 84-position decode, but do not "fix" the duplicates.

The wall is *not* a new object and must not be routed through `$030C`; it is a
VRAM/attribute patch after `draw_screen`, keyed by the live scroll when each
`$0700` record is issued.

## 5. Background residue at 2800 px

Measured with `xtask screens --stage 6 --frames 7400 --sample-scroll 1000
--sample-scroll 2800`:

| sample | tiles | attributes |
| --- | ---: | ---: |
| 1000 px | 0 | 0 |
| **2800 px** | **73** | 0 |
| 3039 px (automatic last sample) | 0 | 0 |

`stages.md` already records the same 73/0 at 2800 and 0/0 at 2900, 3000 and
3072. This is a **capture-order artifact, not missing data**: the live
nametable at the 2800-px sample has not yet received all the end-area wall
tiles that the wide-map extraction already contains; one sample later the wall
is painted and the maps agree. The stored `STAGE6` map does contain the wall
(the tail window hashes in `stage_bgs.rs` are pinned to it).

Gameplay effect: **none**. The 73 cells are background wall art, not objects
and not the collision map; the scroll, spawner and object table are unaffected
(the same run's state at 2800 px matches the oracle). The only exposure is a
port that paints the whole RLE map in one frame instead of at the ROM's
left-edge cadence: it would differ for one frame at the 2800-px crossing. That
is a pixel-report note, not a state gate.

## 6. Gaps (explicitly not measured here)

* The per-cell identity of the 73 mismatches at 2800 px (which wall tiles lag)
  was not dumped; only the count and the clean 2900/3000/3072 samples.
* The world-fixed ordering of the 88 erases was not proven frame by frame. The
  decoded positions and the 2988-3086 px world span are measured; the exact
  per-entry scroll pairing relies on the `$0700` issue frame and is approximate
  by one frame.
* A run that fills all ten object slots at a formation attempt was not forced;
  the "member dropped" behaviour is observed at 809 (one of three `$84`) and
  follows from `$A3DE`/`$A3B5`, but the drop threshold itself was not swept.
* Stage-7 spawning/`$1E` timing beyond frame 1000 is out of scope here; the
  stage-6 card only needs the `$19 = 6` handoff.

## 7. What the core wiring had to change (now implemented)

Items 1-4 are implemented on `main`; item 5's stage advance is implemented,
but its `$AC` cue / no-`Boss` requirement is still open (§8).

1. `Stage` row for `$19 = 5`: `$9A30 = $0B`, `$98F0 = $0C`, background
   `STAGE6`, music `$61`, no core. Gate the end path on `$3F >= 0x0B`, run
   `$81 -> $82 -> $83 -> $86` and never enter `$84`/`$85` or spawn kind `$98`.
2. `$82` length is `$9A28[$17]` (rank), read live, with the `$19 == 6` special
   `458`; stage 6 measured 1024 frames at rank 4. Do not hard-code 1024 or use
   a per-section constant.
3. Keep `$86` inside normal play: scroll, spawner, object AI and beam
   collisions all run 6955-7467. Do not map `$86` to `Phase::Transition`.
4. Wall erase: two entries/frame from the packed 88 bytes while `$86` and
   `$66 < 88`, as a nametable+attribute overlay; `$66` is a dedicated erase
   index, never the port's `$66` scratch (stage 7 already writes 9 there).
5. Advance at `$3F >= 0x0C`: `$1B = $90` for one frame, then `$19 = 6`,
   `$3F = 0`, and stage-7 init. Request `$AC` at `$99B3` and do not play
   `Boss`/`StageClear`.

Top wiring risks: (a) rank timer vs hard-coded 1024; (b) treating `$86` as a
transition and freezing the scroll/beams/wall; (c) validating with pixels:
the beams are **visible** sprite group `$8D` (`$0136` is also `$0120` for slots
22-31), and spawner slots shift with kills, so gate on the ending report
(beams, `$66`, phase), on (kind, X, Y) spawn events, and on the cell/ball
pixels, not on slot equality alone.

## 8. Current state: the no-core clear is wired and gated

Phase 6 is no longer report-only. On `main` (core wiring `bbebf79`):

* `new_at_stage(5)` runs the same three-gameplay-frame `$9B38` warm-up as
  stages 4-5 (`stage_four_boot`), so the first compared frame has slot 9 at
  X 234 like the oracle; the 6 px lead from the old report is gone.
* `QUIRK_NO_CORE` is read. At `$3F >= 0x0B` the stage arms `Endgame::Hold`
  with the live `$9A28[$17]` rank timer (1024 frames at the measured rank),
  holds the scroll and never spawns kind `$98`; `$1B` never reaches
  `$84`/`$85`.
* `$83` bridges to `$86`. `$86` runs inside normal play: the scroll resumes on
  its first frame, the spawner/object AI/collision keep running, and `$CD9F`
  queues two `$CE2B` cells per frame (88 entries, 44 frames) into the
  nametable overlay, guarded at `$66 = $58`.
* At `$3F >= $98F0[5] = 0x0C` the `$90` frame calls `$96C4`: `$19 = 6`,
  `$3F = 0`, and `$3E/$3D` survive (the first stage-7 frame is at `1:00`).

The gates:

| gate | where | result |
| --- | --- | --- |
| `stage6_replay_is_exact` | `crates/gradius-diff/tests/stage6_replay.rs` over `tests/fixtures/stage6-297-1500.state` | exact over 297-1500: 1204 frames, 0 divergences, every field 100%, object slots 6759/6759, live-count delta 0 |
| `stage6_clears_to_stage_seven` | same file, ROM-free | `new_at_stage(5)` with no input reaches `section() == 6` in exactly 7171 frames |
| release-check `stage6` row | `scripts/release-check.sh` | runs `--stage 6` 297-1500 with `--report --pixel-report`; state must be exact, and it pins the entry scout at X 234 plus the first `$84` (745) and `$88` (1066) formations; the pixel line stays `WARN` |

Both layers were checked negatively: a score injected on one native frame makes
`stage6_replay_is_exact` fail at 897, and a truncated `stage6.json` makes the
release row fail.

Pixels are report-only: 297-1500 is **99.7%** overall (playfield 99.7%, HUD
100.0%) over 1203 samples. It is below the plan's 99.9% target; the remaining
0.3% is not itemized per frame or cell.

Explicit gaps (not implemented or not verified):

* **`$C6D8` beams are ported** (`stage6_beams.rs`): visible group `$8D` red
  balls, the pattern-driven 8.8 Y movement, the 22x18/16x18 collision bands,
  the shield clear and the +10 shot payout. `stage6_replay` only reaches 1500,
  so the beam window is gated by the differential harness (5900-7000).
* `$AC` (the stage-clear cue) has no `Sound` event in the core and no music
  test. The app music director's behaviour across the stage-6 `$82`/`$86`
  window is unverified: the ROM requests `$AC` and no `Boss`, while the cue
  selection keys off the stage's `boss_trigger = 0x0B`.
* The 0.3% pixel residue has no frame/cell list.
* The measured-only items from §4-§5 still stand: the 73 cells at the 2800 px
  sample are a capture-order artifact without a per-cell dump, and the
  per-entry world pairing of the 88 erases is approximate by one frame.
* Stage 7 takes its own no-core 458-frame path to `$9865`/`loop_count`, but
  the `$87`-`$8D` ending states and the stage-1 loop replay are not modelled;
  the stage-7 fixture remains report-only.

## The spawner stall: kind `$1A` (`$B47A`) and the pickups

`gradius-diff --stage 6 --keep-alive --native-keep-alive --frames 13000`
(native 297-13000, stage 6 then stage 7 and the loop) used to collapse after
oracle frame ~2800: the port kept filling the object table with invisible
kind-`$9A` pieces and every later spawn failed. The measured chain, in ROM
addresses:

* `$B47A` (`$1A` spin piece, spawned by the stage-6 terrain records at
  `2*$DA = $3B0`, oracle 2185, slot 9): the port had the state machine
  inverted. `$048C` 2 drifts (`$B4C2`), 0 re-aims with `$BCAF` (`$B49A`) and
  1 moves along `$042C`/`$03BC` (`$B4A8`); the port drifted in neither state
  and never aimed, so `$042C` stayed 0 and the piece sat at its spawn point.
  `$B492`'s `$B622` with Y 6 (`$B64A[6] = $0A`, `$B64B[6] = $42`,
  `$B64C[6] = 8`) was missing too, so `$012C` stayed 0 and the dump (which,
  like the oracle, only lists `$012C != 0` slots) hid the leak. `$04CC` is
  `$B4DE[$17]`/`$B4E5[$17]` with `$17` the live rank, not a player index:
  stage 6 at rank 2 is `$40`, not `$50`.
* `$C137`/`$C153`: a `$27`/`$29` pickup is not scored and removed. The slot
  becomes the kind-1 item (`$012C = $A3` for the 1UP, `$A1` for the 500-point
  one) and keeps drifting; `$27` pays `INC $20` (a life) and `$29` pays
  `$8450` (500). Both gates read the `$07E5` score byte: `$27` is skipped
  while bit 0 is set (oracle 5631, score 1570 = `$15`, is skipped; 5731,
  score 1620 = `$16`, lands).
* `$C0A0` pays `$8468` (10) for every shot kill, the five-hit kind `$9A`
  included; the 300 of `$8454` belongs to the small core's own handler.
* `$8950`'s `$42 = 7` wrap calls `$CE83`, which returns the low nibble of
  the same `$07E5` byte; a 5 pays `$845A` with `A = $10` (1000) before the
  capsule's 50 (oracle 5125, score 520).

With those four fixed the 13000-frame run has 0.0 % score/lives/meter
divergence, 100 % `obj.kind`, and the enemy peak per 500-frame window matches
the oracle in every window of stages 6 and 7 (9/9, 8/8, 7/7, ... 9/9). The
only residue is the first stage-7 frame: the port's `$90` advance resets
`$3F` one frame before the ROM (oracle 7467 still reads `$0C`, the port `0`),
so the first `$85` scout sits two pixels left on 7469-7635 and frame 7467's
`scroll` is the single scroll mismatch. That timing lives in
`advance_section`/`enter_stage` (`stage_flow.rs`).
