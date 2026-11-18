# Stage-3 gaps: descender, ring phase, spawner timing, priority residue

Measured 2026-11-10 on `agent/s3desc` with the natural stage-3 window
(`tests/fixtures/stage3-19429-23000.state`, `scripts/boss-script.txt`, both
keep-alives). Complements `stage3.md`; the object-count and pixel gaps listed
there are traced here to addresses.

**Current status.** FIXED: the `$1C` descender's `$B622` sprite, the
stage-3 sprite CHR bank, the `$0D`/`$0E` arc, the `$17` small core's `$B79B`
body, the ten-kill `$39` clear (`stage3_clear.md`) and the 21526 capsule rule
(see the `agent/s3objects` section). The state gate is exact: the whole
19429-23000 fixture (3572 frames) replays with **0 divergences**, so there is
no first divergence left. The shield-ring `$02` phase is correct, but the port
still draws the ring on
19429, one frame before the oracle (ACCEPTED, report-only; see §2). The
19505 sprite-list-lag rule failed and is ACCEPTED: the residue is the
eight-per-scanline priority subset plus the missing `$BBB1`/`$BC3E`
object-fire sprites. Pixels are report-only at **98.9% overall / 98.8% play /
100% HUD**; the worst samples are 21420, 21390 and 21690 at 97.1-97.3%. The
remaining pixel cause is **OPEN**: the `$BBB1` object-fire pass
(`$040C`/`$04EC`/`$0496`, sprite group `$25`/`$8F45`), §3.

```sh
ROM="$GRADIUS_ROM"
CARGO_BUILD_JOBS=2 cargo xtask spawner "$ROM" --script scripts/boss-script.txt \
    --from 19860 --to 19900 --keep-alive
CARGO_BUILD_JOBS=2 cargo xtask disasm "$ROM" --from B4F7 --to B660
./target/debug/gradius-diff --rom "$ROM" --pack assets/gradius.pak \
    --script scripts/boss-script.txt --keep-alive --native-keep-alive \
    --frames 19530 --from 19420 --to 19530 --pixel-report --png-every 1 \
    --pixel-window 19420:19530
```

## 1. `$1C` descender sprite at 19884 `obj.slot`: FIXED

The object is **not missing in the native**; only its sprite byte is.

Stage 3 is `$19 = 2`, so `$A7F8` selects the stage-3 column table and column 0
is `$A9BD`. Oracle spawner ledger around the spawn:

| frame | `$5D` | `$69` | `$6A/$6B` | `$6C` | `$6D` | `$6E` | slot 9 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 19875 | 0 | 0 | `$A9C5` | 0A | F0 | A0 | — |
| 19876 | 1 | 3 | `$A9C7` | 0F | F0 | 2A | `$030C=9C $036C=F0 $032C=2A $012C=00 $048C=80` |
| 19883 | 0 | 3 | `$A9C7` | 08 | F0 | 2A | `$012C=00` |
| 19884 | 0 | 3 | `$A9C7` | 07 | F0 | 2A | `$012C=4B`, x `$E8` |

Record 4 at `$A9C5` is `delay $70, type $8D`. `$A367` masks the type to `$0D`
and reads `$A5FC + 4*$0D = $A630` = `00 1C 11 15`: subtype `$010C = 0`,
kind `$030C = $1C`, formation index `$03BC = $11`, params `$03EC = $15`.
`$A58C + 2*$11 = $A5AE` = `F4 2A` (four members, x `$F0`, y `$2A`) and
`$A5B6[$15]` = `0F 00 5A` (interval 15, Y step 0). Record 5 at `$A9C7`
(type `$8E`) primes the second four-member pair (`$A5FC + 4*$0E = $A634` =
`00 1C 12 15`, formation `$12` = `F4 A0`). Kind `$9C` is `$1C` after
`$AE13` adds `$80`; the port already dispatches it to `ai_descender`.

`$B4F7` decode (handler body and the missing write):

```
B4F7  LDA $030C,X / BMI $B50A
B4FC  JSR $B0AE          ; kind += $80
B4FF  LDA #$80 / STA $048C,X
B504  LDA #$14 / STA $04AC,X
B50A  LDY #$03
B50C  JSR $B622          ; per-handler sprite animation, Y selects the row
B50F  DEC $036C,X        ; x -= 1
B512  JSR $B24B          ; bounds
B515  LDY $046C,X        ; 0..3 stepper, $B524/$B532/$B540/$B550
B622  INC $014C,X        ; divider
B628  CMP $B64A,Y        ; Y=3 -> $B64D = 08
B62D  ... $016C = ($016C+1) % $B64F[Y]   ; $B64F = 08
B63E  ADC $B64B,Y        ; $B64E = $4A
B641  STA $012C,X        ; sprite = $4A + ($016C & 7)
```

So the first sprite appears eight frames after the spawn (`$4B`, then `$4C`,
...) — exactly frame 19884, the first frame the fixture compares (`$012C != 0`
filter). The native `ai_descender` sets `kind |= $80`, `ai_state = $80`,
`ai_mode = $14` and moves, but never writes `sprite`, so slot 9 stays at
`$012C = 0` and both `dump_state` and `build_sprites` skip it.

**Fixed.** `Game::ai_descender` runs `$B622` before the movement: it
increments the divider (`anim_div`, which is `$014C`) and, at 8, steps `anim`
(`$016C`) modulo 8 and sets `sprite = 0x4A + (anim & 7)`; the
`$B532`/`$B540`/`$B550` timers follow the disassembly. Subtype is 0 for this
definition, so the `$ADDF` pre-pass never runs and `$014C`/`$016C` are free for
the handler. Spawn record `$A9C5`, eight frames in: slot 9 has `kind $9C`,
`$012C $4B`, x `$F0-8`. The replay's first divergence moved from 19884 to
20134 (now fixed, see §3), then 20708 (§4, fixed), and then to 21526 (fixed, see
the `agent/s3objects` section). `$B553`
(`$1D`) uses the same
routine with `Y = 9` (row `$B653`/`$B654` = `08`/`$52`), worth sharing if `$1D`
lands later.

## 2. Pixel gap 19429: shield ring drawn one frame early — ACCEPTED (report-only)

`stage3.md` called this "the native draws its trailing option sprites one
frame into stage 3". That is wrong. Measured oracle RAM for 19426-19507:
`$45` (options owned) is 0, `$5C` is 0, `$0141-$0144` are 0, and the OAM has
no option group (ship group 1 plus the engine only). The bbox
`(37,73)-(44,88)` is the **shield ring** (`$46` is held at 5 by keep-alive):
one 8x16 sprite at ship `(+15,-1)`.

| frame | oracle group | oracle tile | native group | native tile |
| --- | --- | --- | --- | --- |
| 19429 | — (black) | — | `$5A` | `$B7` |
| 19430 | `$5A` | `$B7` | `$5A` | `$B7` |

One frame differs; 19430 onward is identical. The ROM cycles `$8B70-$8B8B`
with `($02 >> 2) & 3`; the port uses `self.nmi_frame` in `build_sprites`
(`0x5A + ((self.nmi_frame >> 2) & 3)`). `game.rs` already documents
`$02 = frame - 5`, so the two counters only disagree around the one-frame
`$96C2` handoff, which is enough to land the ring on a different quarter of
the 4-entry cycle for exactly frame 19429.

**Phase fixed; draw timing still one frame early (ACCEPTED).**
`build_sprites` reads the ring phase from `$02`, i.e.
`(self.frame.wrapping_sub(5) >> 2) & 3`, which is the `$8B70-$8B8B` cycle the
ROM uses, so 19429 now draws group `$5A` / tile `$B7`. But the oracle shows no
ring at all on 19429 and its first ring sprite is 19430, so the port draws it
one frame early: the 52 play px remain, and the `s3gate` run still measures
them. 19429 sits inside the report-only 19429-23000 pixel window (there is no
hard pixel window any more), so the gate keeps this as an accepted,
presentation-only residue. Do not chase an option clear: nothing clears
`$45`/`$5C` in this run because there is nothing to clear.

## 3. Pixel gap from 19505: priority subset and the missing object-fire pass (OPEN cause)

The 19429-19504 window is exact state, and the stage-3 opening formation is
record 0 of `$A9BD`: `type $89` -> `$A5FC[9]` = `01 05 09 05`, formation `$09`
= `F5 2A`, five kind-`$05` scouts from x `$F0`, y `$2A`, drifting left at
2 px/frame. `$B0A9`/`$B0C7` starts the `$B103` U-turn at x `< $60`
(frames ~19501-19510), the "rotating" arc the field shows.

Measured 19504-19507: the native objects and the oracle RAM agree on
`kind`, x, y, `$048C` and `$012C` for slots 5-9 (e.g. 19504 slot 5: both
`k $85, x $B2, y $2A, spr $0E`). The pixel diff is only the lower tail of the
8x16 parts, because the scout's upper body is symmetric: a 2-px horizontal
offset is invisible there and shows only in the asymmetric tail.

The offset is in the drawn list, not the simulation: at frame 19505 the
native draws `earlier_sprites` built at internal frame 19503 (slot 5 x = 180)
while the oracle OAM read after frame 19505 has x = 176. `Game::render`
rotates `last_sprites`/`earlier_sprites` and draws the two-frames-old list;
that model is right for stage 1 (`savestates.md`), so verify here before
changing it: compare one object's x in the drawn list against the oracle OAM
for the same visible frame on a stage-1 formation (they match) and on this
stage-3 one (one step behind). The bboxes are 5-6 px tall, so this is a small
latency/phase difference, not a wrong group or palette: the pixel colours
differ only where the native tail pixel maps to another sprite's colour.

**Hypothesis (rejected).** Instrument `build_sprites` and the oracle
`$8AB1`/NMI write for frames 19503-19506 and pin which state frame each side
shows. If the native is one step behind only when the object moves 2 px/frame,
the draw should use `last_sprites` for the same frame the ROM's OAM was built;
if the oracle really shows a two-frame-old list here, the sprite phase differs
because the object was spawn-ordered by `$A2BA` on a different frame. Either way
keep the stage-1/stage-2 pixel tests as the regression gate.

**Measured 2026-11-10 (`agent/s3impl`).** The two-frame-old list is correct.
The NMI at frame N copies the shadow OAM built at NMI N-1 from the RAM state
left by frame N-2, so the visible frame and the port's `earlier_sprites` both
carry state F-2; the fixture's slot-5 x at 19504 (drawn) matches the oracle
OAM, and the 19504/19505 state x is identical on both sides. The 5-6 px tail
residue is the eight-per-scanline priority subset where the five stacked
scouts plus ship/shield exceed eight sprites on one scanline, so which tail
the PPU drops depends on OAM slot order. One-frame and zero-frame lag, ring
phase `+1`, `+2`, and a four-fixed-slot ring (`fixed = 4`) each make the
19500-19530 window worse, so `Game::render` is left as it is.

The real stage-3 sprite pixel cause found while fixing the descender: stage 3's
dominant CHR bank is **1** (`stage3_plan.md`; `STAGE3.chr_bank = 1`), but
`Game::hardware_bank` returned 0 for the whole scrolling section. Every
stage-3 sprite therefore fetched pattern-table bank 0, which is why the `$1C`
descender drew pack group `$4B` from the wrong tiles (a checkerboard, not the
ROM's enemy). `hardware_bank` now reads `screen.chr_bank` (0 on stages 1-2, so
their gates are unchanged) with the column-416 bank-2 tail override. Over the
119 sampled stage-3 frames the differing pixels fall 108,013 -> 101,517; the
19890-20130 samples drop sharply (19920: 403 -> 89, 19950: 682 -> 128).

**Measured 2026-11-10 (`agent/s3snext`).** After the `$0D` arc fix the
19530-20700 samples carry 44-286 differing pixels each. A component split of
19530 (`oracle-19530.png` vs `native-19530.png`) gives three 24-px components
at `(100,55)`, `(84,59)` and `(70,64)` plus 2-px tails; the state is exact.
The sprite is group `$25` / `$8F45` (tile `$69`, attr `$23`, pack group
`$25`), **not** the player double shot (that is group `$24` / tile `$73`,
attr `$22`). The same three sprites are in the oracle OAM at 19530 (slots
2/36/51) but not in the port's drawn list. Group `$25` is the `$BC3E`
object-fire bullet: `$BBB1` walks every object with `$04EC != 0`, counts
`$040C` down by `$98` per frame and spawns a `$0176` bullet whose `$0136`
sprite is `$BC5E[subtype in $80..$8F ? 1 : 0]` (`$59`/`$25`). The port has
no `$040C`/`$04EC` fields and no `$BBB1` fire pass, so it never emits these
bullets; this is the same accepted residue recorded for stage 2 in
`stage2_pixels.md` (9748-11199). The residue is therefore not the
eight-per-scanline subset alone: it mixes that subset with the missing
object-fire bullets. The priority subset stays ACCEPTED (`Game::render` is
untouched); the object-fire pass is the remaining **OPEN** cause of the
stage-3 pixel gap and its own card (`$BBB1`, state `$040C`/`$04EC`/`$0496`,
sprites `$59`/`$25`).

## 4. First state divergence, 20708 `obj.slot`: the `$17` small core — FIXED

`S2_C1` (stage-3 column 1) record 2 is `delay $40, kind $2B`; `$A3AB` reads
terrain def `$B7 00 60`, so it spawns kind `$17` at X `$F0`, Y `$60` when
scroll reaches high 2, low `$80` (frame 20708 on both sides). The ROM's
`$B79B` body runs **every** frame, not an init-then-body split:

```
B79B  LDX $A8 / $010C=$80 / $0460=1 / $030C=$97
B7AC  LDA $B791,Y (Y = $048C) -> $012C      ; sprite table 3F 40 60 60 70 80 90 A0
B7B5  Y = $17 (rank)                         ; the tables below are per rank
B7BA  $04CC < $28 and player x >= object x?
B7C6    INC $04CC; if x < $F0: x += 2
B7D9  else X -= 1 + $B789[$17]/256           ; $B789 = 00 08 10 18 20 20 20 20
B7ED    on carry, JMP $B68A                  ; despawn
B7F0  Y -= / += $B793[$17]/256, clamp 14..AC ; $B793 = 60 60 70 80 90 A0 B0 C0
B826  Y = $17; $03BC >= $B781[$17]? -> $B854 ; $B781 = 3C 37 32 2D 28 23 23 23
B830  $046C >= $B84C[$17]? -> $8454 + $CB20  ; +300 then kind-2 debris
B840  INC $03BC (+1 again when $048C != 0)
```

For rank 2 that is an X step of 1.0625 px/frame, a Y step of 0.4375, a
`$048C` toggle every 50/26 frames, and four hits. The earlier note read
`$B781` as `02 03 04 05 06 07 08 08`; that is the neighbouring `$B84C`
table, and the real `$B781` is the descending 3C/37/32/2D/28/23/23/23 row
(measured from the ROM at `$B781-$B788`).

**Fixed.** `Game::ai_miniboss` now runs the whole body every frame: the
`$B791[$048C]` sprite, the X crawl/step with the underflow despawn, the Y
ease with the integer clamp, the `$03BC` sub-state (stored in the `y_vel`
high byte where the port keeps `$03BC`) toggling `$048C` at
`CORE_SUBSTATE[rank]`, and the `$046C >= CORE_HITS[rank]` death as `$CB20`
kind-2 debris (`anim = 2`, `anim_div = 0`, sprite kept, +300). The hit
counter now accumulates in `shot_hit_object` and dies in the handler, as
`$C04F` does for large objects. The `$B854` three-bullet spread is not
modelled yet (see §3). The fixture's 20708-20912 window is exact; the first
divergence moved to 21526, since fixed (see the `agent/s3objects` section), and
the stage-1/2 gates are unaffected (their streams never spawn `$17`).

## Status after `agent/s3snext` (2026-11-10)

* `ai_arc_toggle` (`$B3FC`/`$B42E`, kinds `$0D`/`$0E`) now seeds
  `$042C = $FE`, i.e. -2 px/frame, and restarts the cycle when the Y-velocity
  high byte (`$03BC`) falls below `$FE` (port: `y_vel < -0x200`, no movement
  on that frame). This clears the old 20134 `obj.x` divergence, which was
  **not** a reused-slot spawn: the `$8F` record spawns kind `$0D` at 20132 on
  both sides and only the X speed differed (oracle -2, port -1, then the
  restart one frame late). A unit test pins the first move, the 33 moving
  frames and the restart frame.
* The fixture's first divergence is now **20708** (`obj.slot`, slot 9): the
  `$2B` record (terrain def `$B7` -> kind `$17`) spawns the small core on both
  sides at 20708, but `ai_miniboss` returns on its init branch without the
  sprite/movement the ROM's `$B79B` body does, so the native dump skips it
  (`$012C == 0`). §4.
* The shield-ring phase reads `$02` (`frame - 5`) and 19429 draws the right
  group `$5A` / tile `$B7`, but the port still draws the ring one frame before
  the oracle (oracle black on 19429, first ring 19430): the 52 px remain as an
  ACCEPTED report-only residue, see §2.
* `hardware_bank` uses the stage screen's `chr_bank`; stage-3 differing pixels
  are down 108,013 -> 101,517 -> **94,246** (98.7% overall, play 98.7%, HUD
  98.8%) over the 119 samples.
* The 19505 tail residue is not a drawn-list lag; see §3 for why it is
  accepted rather than pinned to one OAM rule.

## Status after `agent/s3finish` (2026-11-10)

* `ai_miniboss` now runs the `$B79B` body every frame (§4). The fixture's
  20708-20912 window, including the 20913 `$CB20` debris, is exact; the first
  divergence is **21526** (`score` 3110 vs 3060, `meter` 3 vs 4: the oracle
  collects a capsule the port misses).
* The stage-3 `$39` clear branch is implemented (`stage3_clear.md`): the
  `$86` handoff is 513 frames after the core death, `$8E` scrolls
  4.5 px/frame for 968 frames, `$845A` pays 5000 once, and the second
  `INC $19` lands on stage 5. `finish_death` clears `$39`/`$3A` but not
  `$5F`. Unit tests pin 513/968/969 and the `$39 = 0` 514-frame path.
* Stage-3 pixels are 98.8% (play 98.8%, HUD 98.8%) over the 119 samples; the
  19530 first differing sample is group `$25` (§3), not the small core.

## Status after `agent/s3objects` (2026-11-10)

* **21526 capsule fixed.** `$B092` ORs bit 7 into `$018C` (`$B09A`), never into
  `$03AC`; `ai_turret` was writing `phase |= $80`, so `$BE9F` read the ORed
  value as a cannon class index, decremented a zero class counter and left the
  wreck with phase 0. `ai_debris` then despawned it instead of folding it to a
  capsule. With the write removed the 21525 slot-7 `$92` wreck keeps `$03AC=1`
  and folds at 21526: score 3110 and meter 4 match. The full stage-3 fixture
  (19429-23000) now replays with **0 divergences**; no first divergence is left
  inside the recorded window. Test:
  `an_aiming_turret_keeps_its_cannon_phase_for_the_capsule_rule`.
* The `$BBB1`/`$BC3E` object-fire card was started and reverted: its measured
  constants are recorded here for the next card. `$040C` is decremented by
  `$98` = 1 on an odd `$02` frame and 2 on an even frame in stage 3 (the `$46`
  shield term never applies inside `$BBB1` in this run); `$04EC` keeps the
  spawn motion byte `& $FE`. `$BC3E` fires at most one object per frame
  (highest slot) and stops the scan. The bullet starts at the object plus the
  `$0496` muzzle offset (tables `$BC2C`/`$BC35`; offset 0 is `(0,0)`), and
  `$BD59` adds a quarter to both aimed-velocity components when `$17 >= 2`, so
  stage 2+ bullets travel at 1.25 px/frame. A fire burst sets `$5D` via
  `$83BA`. The reverted prototype reproduced the oracle's 19504/19515/19527
  fires and made many 19530-window frames pixel-exact, but it changed the
  `$96` nametable overlay timing through `$5D` and left the pinned stage-3
  aggregate at 98.7% play, so it is not in this commit.

## Remaining work

1. `$BBB1`/`$BC3E` object-fire bullets (group `$25`/`$59`) — the only
   remaining pixel cause, **OPEN**; the report-only 19429-23000 window is 98.9%
   overall / 98.8% play / 100% HUD (worst 21420/21390/21690 at 97.1-97.3%).
   A separate card (`$040C`/`$04EC`/`$0496` state, constants above).
2. The `$B854` three-bullet spread of the small core (same `$0176` pool).
3. `$C40D`/`$C680` `$3A` bonus wave (S3-03), then the `--stage 3` pixel gate
   (S3-05).
