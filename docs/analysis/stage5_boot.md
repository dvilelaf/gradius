# Stage-5 boot slice: entry state, `$14` cannons, spawner, clear and the first port divergence

Measured 2026-11-10 on `agent/s5analysis` (from `4c16595`) with the release
harness, ROM `Gradius (E).nes`. This is analysis only: it changes no core file
and does not re-extract any data. The stage is `$19 = 4`; it is reached from
stage 3's `$39` clear, which increments `$19` twice: 2 -> 3 at `$992E` and 3 -> 4 at `$96C4`
after the 968-frame `$8E` bonus scroll, so stage index 4 (stage 5) is entered
one frame after the `$90` handoff.

**Status on `main` `fc5378f`:** the `$AEDB` drift fix (`8ea0595`), the `$BF43`
phantom-debris removal (`b675998`) and the `$82` hold-countdown fix on the
cannon-chain frames (`c2e1021`, §11 context) have landed. The clear fixture
(`8574-12003`) is now exact from 8574 through **10670**; the first divergence
is **10671 `obj.x`** (Big Core, oracle slot 7 `166`, native `164`), analysed in
§11. The clear window after 10671 is report-only, so the analysis stays
referenced by an OPEN ledger row. Status notes are in §7, §9, §10 and §11.

Landmarks used throughout: stage 5 starts at **frame 10608** in the
`record-boss --stage 3` run (the double increment); the stage-5-only run
`record-boss --stage 5` starts it at **frame 292**, so stage-relative frames
convert with `absolute = relative + 10316`.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-s5analysis
cargo build --release -p xtask -p gradius-diff

# entry frame and stage-5 timeline through the double increment / the core
cargo xtask record-boss "$ROM" --stage 3 --frames 11000 --out /tmp/opencode/s5_boss3.txt
cargo xtask record-boss "$ROM" --stage 5 --frames 13000 --out /tmp/opencode/s5_boss5.txt

# stage-5-only RAM probes (the $26 poke makes $9B68 copy it into $19)
ST5=$(for f in $(seq 170 320); do printf -- '--poke %d:26:4 ' "$f"; done)
cargo xtask probe "$ROM" --frame 302 --buttons a --press 180:start --keep-alive $ST5 \
  --ram /tmp/opencode/st5-302.ram
cargo xtask probe "$ROM" --frame 2900 --press 180:start --keep-alive $ST5 \
  --ram /tmp/opencode/st5-2900.ram

# background, palette and CHR banks
cargo xtask screens "$ROM" --stage 5 --frames 13000 --assist --press 180:start \
  --sample-scroll 0 --sample-scroll 512 --sample-scroll 1100 \
  --sample-scroll 3072 --sample-scroll 3300

# the exact gate window (the first divergence, 8574, is §9)
cargo xtask extract "$ROM" /tmp/opencode/s5.pak
$CARGO_TARGET_DIR/release/gradius-diff --rom "$ROM" --pack /tmp/opencode/s5.pak --frames 8573 --stage 5 \
  --keep-alive --native-keep-alive --report \
  --oracle-out /tmp/opencode/s5_oracle.txt --native-out /tmp/opencode/s5_native.txt
```

## 1. Entry frame and initial state

The stage-5 entry frame is **10608** (`record-boss --stage 3`): scroll becomes
`5:0:0`, score jumps `6650 -> 11650` on the previous frame (the `$8E` +5000),
the stage-5 column-0 stream starts at **10617**. `$19` is 4 and `$1B` is `$80`.

| byte | value at entry | note |
| --- | --- | --- |
| `$19` | `04` | stage 5 |
| `$1B` | `80` | normal play (stage init finished) |
| `$1C` | `93` | shared intro music |
| `$17` | `02` | rank = shield only (`$46 != 0`, no options/weapon) |
| `$1A` | `00` | stage sub-index |
| `$02` | `6B` | free-running counter = `(frame - 5) & $FF`; measured at 302/323, derived here |
| `$03` | `40` | measured in stage play (not the raw held-button mask) |
| `$3E:$3F:$3D` | `05:00:00` | scroll low:high:fraction (the `$8E` left `$3E = 5`) |
| `$39` | `00` | no stage-5 clear gate |
| `$3A` | `00` | no bonus wave |
| `$5F` | `00` | cleared by `$96C2` on stage entry |
| `$0C` / `$0D` / `$0E` | `00` / `00` / `00` | `$0D` no blank, `$0E` queue empty; `$0C` has no located writer |
| `$5B` / `$5C` | `00` / `00` | scroll-stop flag / attached-cannon count |
| `$68` / `$69` | `00` / `00` | `$C64D` script / stream command counter |

The three bytes are `00` at entry, but note that `$0C` is **not** an identified
game variable: the static flow has no direct writer or reader (only unrelated
`$0C,X` indexed accesses), while `$0D` is the screen-blank countdown and `$0E`
the VRAM queue index. The card's `$0C/$0D/$0E` also match the **stage gate
constants**: `$9A30[4] = $0B` (boss arm), core spawn when `$3F = $0C`, stage
end `$98F0[4] = $0D`. The port's current `0x0C`/`0x0D` literals are the
stage-1 values and must become `$0B`/`$0D` for stage 5.

## 2. `$14` large objects and the `$0600` cannons

Among the three large-object kinds only **`$14`** (`$CA58`) appears in stage 5.
Kinds `$15` and `$16` do not occur here (`$15` is stage 4's `$C5A7` arc, `$16` is stage 3's `$C900`
background turret); the "cannons" are the `$0600` records attached to `$14`.

**Stream command spawns.** `SECTIONS[4]` columns C2..C5 carry four five-byte
records each: `b2 = 01/12/02/21`, `y = 80/40/80/60`, kind `$14`. The observed
spawns (slots 1-9, `record-boss --stage 5`) begin at stage-relative **2309**
(absolute 12917 in the natural run) and repeat through scroll `$3F = $0A`:

| stage frame | scroll `$3E:$3F` | slot | y | b2 | `$0600` records created |
| ---: | --- | ---: | ---: | ---: | ---: |
| 2601 | `00:04` | 9 | `80` | `01` | 1 |
| 2857 | `00:05` | 8 | `40` | `12` | 2 |
| 3368 | `00:05` | 9 | `80` | `02` | 1 |
| 3752 | `00:05` | 7 | `60` | `21` | 2 |

The four-record group then repeats at `$3F = 6/8/A` (frames 4346, 4746, 5256,
5640, 6234, 6634, 7144, 7528, 8122). `$A4A0` consumes **one record per
non-zero nibble of `b2`, low nibble first**: `01` -> 1, `12` -> 2, `02` -> 1,
`21` -> 2. That is the measured authority; the prose note "count is `b2 >> 4`"
in `stage5_plan.md` is wrong (the four `LSR $65` run after the low nibble has
already been used). `$016C` (port field `anim`) stores the number of records
created for that object.

**Attached cannons at frame 2900** (probe, no shooting):

| `$0600` record | parent | index | fire counter `$0604` |
| --- | ---: | ---: | ---: |
| `$0630` | slot 8 | 0 | `$17` |
| `$0660` | slot 8 | 1 | `$17` |
| `$0690` | slot 9 | 0 | `$09` |

`$5C = 3`, `$5B = 1` on that frame. The `$0600` page has only four records
(`$00/$30/$60/$90`), so at most **4** are live; the three above are the
measured peak with two `$14` objects (slot 8 `$016C = 2`, slot 9 `$016C = 1`).

**Fire cadence.** `$CB8B` increments `$0604` once per call and fires when it
reaches `$CBC4[rank]`. At rank 2 that is **30 calls**; the routine is called
only from the stage-5 alternate chain (odd `$02`), so each cannon fires every
**60 frames**. Measured: counters rise by 5 per 10 frames (odd frames only) and
reset 30 -> 0 at 2915/2975/3035; `$0136` bullets (kind `$86`) appear at 2915
(slot 9), 2920 (7), 2975 (6), 2980 (5), 3005 (4), 3040 (3), 3065 (2), 3100
(1). Bullet X/Y are the cannon position minus 8 (`$CBFE-$CC0D`).

**The `$19 == 4` chain.** `$9658` runs when `$19 == 4`: it counts non-zero
`$0600` records into `$5C`; if `$5C >= 2` **and `$02` is odd**, it calls
`$A2BA` (spawner), `$CB8B` (cannons), `$ADA5`, `$BBB1`, `$9FF6`, `$C0C1` and
sets `$5B = 1`, which makes `$9A8F` skip the normal `$98E1` 0.5 px scroll step.
Measured effect: with `$5C = 1` (frame 2605-2857) scroll is 0.5 px/frame; from
`$5C = 3` (2857 onwards) `$5B` alternates 1/0 and scroll is **0.25 px/frame**
(2900-3000: 25 px / 100 frames). That is what makes the core arrive late.

**`$C64D` script.** `$C40D` is reached every 4th frame from `$A2BA` only while
`$1B == $82` (or `$3A != 0`), i.e. during the boss-entry hold. `$C64D`
increments `$68`, and every `$28` calls reads the next `$C674` pair and calls
`$A4A0`. Measured during the `$82` hold (`$3F` frozen at `$0B`, 8521-10273):
five objects at **8681, 8969, 9129, 9449, 9609**, y `80`/`40`/`80`/`C0`/`80`
(the four-entry `$C674` cycle: subtype `02` at y 80, `00` at y 40, `01` at y 80,
`00` at y C0) in slots 8/9/7, one cannon for the
`02`/`01` entries and none for `00`. `$68` stays 0 in normal play, which
confirms `$C40D` is not called there.

## 3. Spawner: first 1500 frames

`gradius-diff --stage 5` oracle ledger (default script; stage start 292).
Frame is the oracle frame; subtract 292 for stage-relative, add 10316 for the
natural entry run. Position is the spawn frame (one update after allocation).
Group is the sprite group `$012C`: `$9D` -> `$52`+`$016C` (variants 0..5),
`$88` -> `$39`/`$3A`, `$8D` -> `$7E`/`$80`, `$8E` -> `$80`, `$02` -> `$2B`/`$2C`.

| frame | slot | kind | x | y |
| ---: | ---: | ---: | ---: | ---: |
| 302 | 9 | `$9D` | 232 | 160 |
| 343 | 8 | `$9D` | 232 | 160 |
| 384 | 7 | `$9D` | 232 | 160 |
| 425 | 6 | `$9D` | 232 | 160 |
| 429 | 5 | `$02` | 238 | 42 |
| 474 | 5 | `$02` | 234 | 42 |
| 515 | 5 | `$9D` | 232 | 42 |
| 554 | 4 | `$88` | 239 | 59 |
| 556 | 9 | `$9D` | 232 | 42 |
| 557 | 3 | `$88` | 239 | 111 |
| 560 | 2 | `$88` | 239 | 163 |
| 618 | 8 | `$88` | 239 | 59 |
| 621 | 7 | `$88` | 239 | 111 |
| 624 | 1 | `$88` | 239 | 163 |
| 634 | 4 | `$02` | 161 | 21 |
| 681 | 6 | `$8D` | 240 | 128 |
| 692 | 4 | `$8D` | 240 | 128 |
| 702 | 8 | `$02` | 157 | 19 |
| 753 | 8 | `$9D` | 232 | 160 |
| 794 | 9 | `$9D` | 232 | 160 |
| 835 | 5 | `$9D` | 232 | 160 |
| 876 | 7 | `$9D` | 232 | 160 |
| 881 | 3 | `$9D` | 232 | 42 |
| 922 | 6 | `$9D` | 232 | 42 |
| 963 | 4 | `$9D` | 232 | 42 |
| 1002 | 2 | `$88` | 239 | 59 |
| 1004 | 8 | `$9D` | 232 | 42 |
| 1005 | 1 | `$88` | 239 | 111 |
| 1066 | 9 | `$88` | 239 | 59 |
| 1069 | 5 | `$88` | 239 | 111 |
| 1089 | 2 | `$02` | 154 | 18 |
| 1129 | 7 | `$8D` | 240 | 128 |
| 1140 | 3 | `$8D` | 240 | 128 |
| 1146 | 9 | `$02` | 161 | 21 |
| 1151 | 2 | `$8D` | 240 | 128 |
| 1162 | 6 | `$8D` | 240 | 128 |
| 1174 | 9 | `$8E` | 240 | 128 |
| 1201 | 4 | `$9D` | 232 | 160 |
| 1242 | 8 | `$9D` | 232 | 160 |
| 1283 | 1 | `$9D` | 232 | 160 |
| 1324 | 5 | `$9D` | 232 | 160 |
| 1366 | 7 | `$9D` | 232 | 42 |
| 1407 | 9 | `$9D` | 232 | 42 |
| 1448 | 6 | `$9D` | 232 | 42 |
| 1450 | 4 | `$9D` | 232 | 160 |
| 1491 | 8 | `$9D` | 232 | 160 |

The raw types in `SECTIONS[4]` are `$87/$8F/$90/$91/$92`; `$91/$92` resolve to
`$9D`, `$90` to `$8E`, `$87` to `$88`, `$8F` to `$8D`. The `$9D` formations
reappear throughout the stage in the slots freed by earlier kills.

## 4. Background, palette and CHR banks

`xtask screens --stage 5` (13 000 frames, live nametable): 448 x 28 tiles
(3584 px), 0 tile / 0 attribute mismatches at 0, 512, 1100, 3072 and 3300 px.
The stored hashes match the extracted data (`847ED131654EA7A1` tiles,
`0BB9B51B269205C5` attrs, `90FA63C90403F3FC` palette).

| world column | CHR bank | palette |
| ---: | ---: | --- |
| 0 (first frames) | 0 | the title page: white/teal logo backgrounds, sprite palettes 0-1 as in play |
| 0 (dominant) / 512 / 1100 | 1 | the stage page: `$3F00` read from the cartridge (bg palettes 0-3, sprite palettes 0-3) |
| 384 (3072 px) and beyond | 2 | same except bg palettes 2-3 are all `0F` (entries 8-15) |

At entry (frame 302) the live state is CHR bank **1** and the bank-1 palette
above. The change to bank 2 / black tail happens at world column **384
(3072 px)**, which in `record-boss --stage 5` is frame **10273**, the frame before the core
spawn at 10274 (`$3F = $0C`). The extractor also
records a brief bank-0 palette at column 0 before the bank-1 palette settles;
`Screen` stores the dominant bank 1 and the renderer needs the column-384 rule.

## 5. Music and SFX

| event | cue | evidence |
| --- | --- | --- |
| stage-5 intro | `$93` | `$1C = $93` at frames 302, 2000, 2344 (probe) |
| stage-5 theme | `$5F` | first `$1C = $5F` at frame **2345**, one frame after scroll hits `$3F = 4` with `$3E = 0` (2344); `$8354[4] = 4` |
| boss | `$A5` | forced `$3F = $0C` probe: `$1C = $A5` (`$835C` selects it above `$9A30 = $0B`; the natural `$82` hold cannot be probed without the sprite-0 assist) |
| clear | `$AC` | shared `$B97D` cue at core death; `record-boss --stage 5` death at 11489 |
| `$14` death | `$0A` | `$CA58` -> `$CB20(A = $0A)`, 500 points (`$8450`) |
| `$14` spawn / cannon fire / `$C64D` | none | `$A4A0`, `$C64D`, `$CB8B`/`$CBCB` issue no `$EC18` |

The `$5F` switch is driven by `$835C`, which requires `$3E == 0`; that is why
it lands one frame after the 1024-px crossing. SFX ids do not change `$1C`
(the music latch), so they are cited from the handler code, not read from RAM.

## 6. Clear condition and next stage

Stage 5 has **no `$39` gate**: no stage-5 code writes `$39`/`$3A`/`$5F` (the
only writers are `$AF78` stage 1, `$B972` stage 2, `$C780` stage 3), and all
three read 0 at entry. The shared Big Core is spawned at `$3F = $0C`
(`$9A30[4] = $0B`) and dies through `$B95C`/`$BA96` with +1000.

`record-boss --stage 5` measured: spawn **10274**, scroll stop 10273, first
damage 10753, max damage 6/7, death **11489** (+1000), stage 6 **from 12003**
(514-frame countdown). The transition is the normal `$96C4` `INC $19`
(`$39 = 0`), not the `$992E` double increment.

Direct clear check (stage-5 probe, poking `$1B = $86`, `$3F = $0D`,
`$39 = $3A = 0`): frame 2601 sets `$1B = $90`; frame 2602 does
`$19 4 -> 5`, `$3F = 0`, `$39/$3A/$5F = 0`. Stage 6's first objects are
12004 (slot 9, kind `$85`, x 240, y 42), 12015 (slot 8), 12026 (slot 7).

## 7. The 297-8573 window: 0 divergences, and the 8574 residue

`gradius-diff --frames 8573 --stage 5 --keep-alive --native-keep-alive`
compares 8277 frames (window 297-8573) with **0 divergences**: every field at
100% and 23481/23481 object slots; the `$14` aim/drift and the 0.25 px/frame
chain match. The checked-in fixture `tests/fixtures/stage5-297-8573.state`
(8277 frames, 1.46 MB) replays the same window ROM-free in
`crates/gradius-diff/tests/stage5_replay.rs`. The same command on the port
before this card reported **2155 divergences**, the first at **2601**, the
first `$14` spawn. The `$8E` residue measured earlier is closed by the `$6C`
spawner fix: `$A3DE` leaves the formation timer alone and only a successful
allocation reloads it, so the missed `$90` member waits its interval.

**Cannon shot collision (fixed on `main`, `11b763c`).** `$CC93` advances `$A9`
once per part, each `$0600` part carrying its own phase byte `$0611..$0615`, and
`$CCF1` rewrites the `$9B/$9C` aim pair from the placed part with the
previous-value ±6 clamp. The measured phases `[00,4A,44,3E,38,32]` match the
oracle's `$0641-$0645` byte for byte, and the part positions at 3040 are exact
(`ce c8 c0 b7 b0 ae`).

**Resolved: the cannon death at 3055.** The `$9A69` per-frame tick
(`$C76C` -> `$CB84`/`$CB8B` while `$5C < 2`, §8) removed the one-frame
placement lag: the player's shot now kills the attached cannon on **3055**
(score `90 -> 190`) like the oracle, instead of 3043. Before it the port
placed a part a frame early (`$A7` vs the oracle's `$A6` at 3035; placements
3036/3040 vs 3038/3042) and banked the 100 points at 3043. One hypothesis —
adding `update_cannons()` to the normal `$9A51` path without the `$5C < 2`
guard — moved the death to 3041 and was reverted.

**Resolved: the 8574 `obj.x` divergence (`8ea0595`).** The first field after
the 297-8573 window was **8574** `obj.x` slot 9 (oracle 109, native 111),
analysed in §9: under the `$82` hold the ROM's `$CA58` calls `$AEDB` directly,
past the `$AED7` `$5B` test, while the port routed it through `drift_object()`,
which returns early on `scroll_locked`. `ai_large_object` now uses the ungated
`$AEDB` entry, so slot 9 drifts through the hold and the first divergence moved
to 8642; §10 then removed that leftover too, moving it to **9597 `scroll`**
(oracle `0:11:128`, native `1:11:0`); the `$82` hold-countdown fix (`c2e1021`)
then removed that one and the window is exact to **10670** (§11).

The clear to stage 6 is recorded in `tests/fixtures/stage5-8574-12003.state`
(core spawn 10274, death 11489, stage 6 at 12003) and replays ROM-free, but
`stage5_clear_replay` is still report-only over 8574-12003: 3,430 frames,
4,537 divergences from 10671, and object slots 7,382/7,603 (97.1%). The hard
gate still ends at 8573; landing the clear as a gate is the next card.

**Pixels.** The 297-8573 pixel window is a report-only `WARN`: 99.0% overall,
98.8% playfield, HUD 100.0%.

## 8. The missing call site: `$C76C` -> `$CB84` -> `$CB8B`

`$CB8B` is reached from **two disjoint routes**, and the port only has one of
them. The second is a stage-5 hook in the common per-frame tail, not in the
`$9684` chain:

```
$9A63  JSR $BFDC      ; collision scan (slot 9 down)
$9A66  JSR $8979      ; power-up dispatch
$9A69  JSR $C76C      ; every frame that reaches this tail
$9A6C  LDA $3E

$C76C  LDA $19
$C76E  CMP #$04
$C770  BNE $C775      ; other stages: RTS
$C772  JMP $CB84
$C775  RTS

$CB84  LDA $5C
$CB86  CMP #$02
$CB88  BCC $CB8B      ; `$5C < 2` falls through into the cannon pass
$CB8A  RTS
```

`$C76C` runs after the collision scan and the power-up dispatch, with the
frame's `$5C` already recomputed by `$9658` at the top of the frame. These are
the only two entries into `$CB8B` in the whole PRG (`20 8B CB` at `$9684` and
the `$CB84` fall-through); neither the NMI (`$806A`) nor the IRQ (`$80B5`)
vector reaches it. The
`$9658`/`$9684` chain is the other route and it is only the `$5C >= 2` case:
`$9678 CPX #$02 / BCC $9698` skips it below two, `$967C LDA $02 / LSR /
BCC $9698` skips it on even `$02`, and its `$9695 JMP $9A7F` jumps *past*
`$9A69`. The two routes therefore never overlap:

* `$5C < 2`: `$CB8B` runs **every frame**, via `$9A69` -> `$C76C` -> `$CB84`.
* `$5C >= 2`: `$CB8B` runs every **other** frame, on odd `$02`, at `$9684`;
  on the remaining frames `$C76C` still runs but `$CB84` returns.

The condition that ends the per-frame tick is exactly `$5C` reaching 2 (the
`$9658` count of non-zero `$0600` records).

**Measured cadence (`xtask probe`, stage-5 pokes, `--press 180:start
--keep-alive`).** `$0603` is the part counter (`$CC2D` decrements it; the
six parts are placed when the new value is even), `$0604` the fire counter:

| frame | `$5C` | `$02` | records (parent/index `603` `604`) | tick |
| ---: | --- | --- | --- | --- |
| 2850 | 01 | `1d` | 9/0 `06` `0a` | every frame |
| 2855 | 01 | `22` | 9/0 `01` `0f` | every frame |
| 2856 | 01 | `23` | 9/0 `00` `10` | every frame |
| 2857 | 01 | `24` | 8/0 `ff` `01`, 8/1 `ff` `01`, 9/0 `ff` `11` | spawn frame |
| 2858 | 03 | `25` | 8/0 `fe` `02`, 8/1 `fe` `02`, 9/0 `fe` `12` | chain (odd `$02`) |
| 2859 | 03 | `26` | unchanged | no tick |
| 2860 | 03 | `27` | 8/0 `fd` `03`, 8/1 `fd` `03`, 9/0 `fd` `13` | chain |

`$5C` is counted at `$9658` **before** this frame's `$A2BA` spawn. On 2857 it
is still 1, so `$CB84` enters `$CB8B` even though `$A2BA` has just created the
two parent-8 records; they start at `$0603 = 0`, `$0604 = 0`, so that same pass
leaves them at `FF`/`01`. From 2858 `$5C = 3` and only the odd-`$02` chain
ticks.

**Why the port missed it.** `step_playing` calls `update_cannons()` only in the
`chain` branch (`$9684`), and `chain` requires `live_cannon_count() >= 2` at
the start of the frame. So the port skips every tick while a single cannon is
live (2850-2856) and, critically, the spawn frame 2857: the frame-start count
is 1, the spawn happens later, `update_cannons()` is never reached. For the two
records created at 2857 this is exactly **one missed `DEC $0603`**, so at 3035
the port held `$A7` against the oracle's `$A6`. This is fixed on `main`
(`agent/s5tick`): the `$9A69` route now ticks, and the kill lands on 3055.

Placement happens when the new `$0603` is even. Oracle: 3034 (`A7`->`A6`),
3038 (`A5`->`A4`), 3042. Port, one decrement behind: 3036 (`A7`->`A6`), 3040,
3044 — the measured 3036/3040 vs 3038/3042. The parts were then one placement
phase ahead when the player's shot reached the cannon, and the 100-point kill
landed at 3043 instead of 3055.

**Exact rule.** In `step_playing`, inside `if !chain` (the `$BFDC`/`$8979`
block), immediately after `collide()` and the power-up activation, before the
clear handling:

```rust
// `$9A69` -> `$C76C` -> `$CB84` -> `$CB8B`: stage 5 ticks the cannons on
// every frame while the frame-start `$5C` is below two. Frames with two or
// more cannons already ticked in the `$9684` chain and `$C76C` jumps past
// `$9A69` there.
if self.section == 4 && !cannon_chain {
    self.update_cannons();
}
```

`cannon_chain` is the frame-start `live_cannon_count() >= 2` (the `$5C >= 2`
test), so `!cannon_chain` is exactly the `$CB84` gate. Frame parity must **not**
be part of this condition, and the call must stay after `collide()`: `$BFDC`
runs at `$9A63`, three instructions before `$C76C`, and must keep seeing the
placement left by the previous frame. Adding the call without the
`!cannon_chain` guard also ticks the `$5C >= 2` even-`$02` frames (where
`$CB84` returns), which is why the earlier hypothesis moved the death the
other way, to 3041.

**Test that would prove it.**

1. Core (`game.rs`, `sandbox()` with `section = 4`): one active cannon, `step`
   three frames with no input -> the counter decrements on all three (odd and
   even `self.frame`, so only the `!cannon_chain` route ticks both). Then two
   active cannons -> the counter moves on even `self.frame` only. Transition:
   one cannon at frame start, the second created by the normal-path `$A2BA`
   in the same frame -> both decrement on that frame (the `$0603 = FF` at
   2857).
2. Diff: the checked-in `tests/fixtures/stage5-297-8573.state` gate covers it:
   the whole 297-8573 window is exact and the kill lands at 3055 (score
   `90 -> 190`). A shorter 297-3000 recut would not show it (its first visible
   field was the early 3043 kill), which is why the gate runs to 8573.

## 9. The 8572 `$82` hold and the direct `$AEDB` (FIXED)

**Status:** fixed on `main` (`8ea0595`). `ai_large_object` calls the ungated
`$AEDB` entry, so the `$82` hold no longer freezes slot 9; the first divergence
moved from 8574 to 8642. The text below is the measurement that found it.

With the `$CB8B` tail call in place the window is exact from 297 to **8573**;
the next field that differs is at **8574** (`obj.x`, slot 9, oracle 109 /
native 111). The two-pixel report hides the real first frame: the oracle is
`110` at **8572** and the port stays at `111`, so the drift stops on 8572 and
the `--tolerance 1` gate only reports once the gap reaches two.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-s5x8574
cargo build --release -p xtask -p gradius-diff
/tmp/opencode/target-s5x8574/release/xtask extract "$ROM" /tmp/opencode/s5.pak
/tmp/opencode/target-s5x8574/release/gradius-diff --rom "$ROM" \
  --pack /tmp/opencode/s5.pak --frames 8600 --stage 5 \
  --keep-alive --native-keep-alive --report
# first divergence: frame 8574 obj.x (oracle slot 9: 109, native slot 9: 111)
```

The oracle trace uses the same default script as the harness (`a` held from
300, steering up/right/left in 60-frame blocks) plus the `$26 = 4` pokes:

```sh
ST5=$(for f in $(seq 170 320); do printf -- '--poke %d:26:4 ' "$f"; done)
PR=$(python3 -c "
d = ['up', 'right', 'left']
for i, at in enumerate(range(300, 8760, 60)):
    print('--press %d:a+%s:60' % (at, d[i % 3]), end=' ')")
cargo xtask probe "$ROM" --frame 8574 --press 180:start --press 240:start \
  $PR --keep-alive $ST5 --ram /tmp/opencode/s5-8574.ram
```

| frame | `$1B` | `$5B` | slot 9 kind | `$016C` | `$0460` | x | y |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8171 | 80 | 00 | `$94` | 1 | 1 | `EF.80` | `7F.B0` |
| 8400 | 80 | 01 | `$94` | 1 | 1 | `B6.00` | `5B.50` |
| 8568 | 80 | 00 | `$94` | 1 | 1 | `70.80` | `2F.58` |
| 8569 | 80 | 00 | `$94` | 1 | 1 | `70.00` | `2F.07` |
| 8570 | **81** | 00 | `$94` | 1 | 1 | `6F.80` | `2E.B6` |
| 8571 | **82** | **01** | `$94` | 1 | 1 | `6F.00` | `2E.65` |
| 8572 | 82 | 01 | `$94` | 1 | 1 | `6E.80` | `2E.14` |
| 8573 | 82 | 01 | `$94` | 1 | 1 | `6E.00` | `2D.C3` |
| 8574 | 82 | 01 | `$94` | 1 | 1 | `6D.80` | `2D.72` |

**The object.** Slot 9 is a stage-5 `$14` large object: kind `$94`, subtype
`$80`, `$048C = 1`, size class 1 (`$0460 = 1`, the 32x32 box), `$016C = 1`
(one attached `$0600` record). It is spawned at **8170** with x `$F0`,
y `$80` and one cannon, i.e. the `$A460` record `[delay $40, type $F0,
b2 $01, kind $14, y $80]`; the one clear `$0600` record is the parent-9,
index-0 one at `$0690` (`$0600 = 09`, `$0601 = 00`). It drifts left from
8170 at 0.5 px/frame and aims Y up toward the ship (`$04CC = 0`) with the
rank-2 `$CA51 = $50` step.

**Which rule moves it.** `$CA58` has two `$AEDB` calls: the first is guarded
by `$016C` (`$CAAD: LDA $016C,X; BNE $CABB; JSR $AEDB`), the second at
`$CB11` is unconditional. Both are direct `JSR $AEDB`, so both enter the
routine **below** the `$AED7` `$5B` test:

```
$AED7  LDA  $5B
$AED9  BNE  $AF03      ; only this entry checks the hold
$AEDB  LDX  $A8        ; $038C -= $80, borrow into $036C
...
$CAAD  LDA  $016C,X
$CAB0  BNE  $CABB
$CAB2  JSR  $AEDB      ; $016C == 0: extra drift
...
$CB11  JSR  $AEDB      ; always, even with cannons and $5B = 1
```

The `$82` boss-entry hold is the frame set where the ROM sets `$5B = 1`
(`$1B = 81` at 8570, `$1B = 82` at 8571); the oracle keeps drifting that
same slot through it (8400 already has `$5B = 1` and x keeps moving). The port
models the `$82` hold as `scroll_locked` and `ai_large_object` reaches
`$AEDB` through `drift_object()`, which returns early:

```rust
fn drift_object(&mut self, object: &mut Object) {
    if self.scroll_stop || self.scroll_locked { return; }   // $AED7 gate
    object.drift();
    ...
}
```

That gate belongs to `$AED7` callers, not to `$CA58`. It adds no error at
8400 (the chain's transient `$5B` is not `scroll_locked`) but freezes slot 9
on 8572, the first frame whose object pass sees `scroll_locked`.

**This is a new cause, not `move_aimed`/the border.** There is no Y guard or
X clamp involved: Y keeps matching the oracle for the rest of the window
(only `obj.x` diverges), and the horizontal motion is the shared `$AEDB`
drift. The only difference is which entry of the drift routine each caller
uses: `$AED7` (gated) versus `$AEDB` (ungated). It is also not a cannon
count/formation or `$C64D` issue: `$016C = 1` and the cannon record match
from spawn through the hold.

**Not a two-pixel blip: it cascades through the phase.** Native slot 9 is
pinned at x = 111 while the oracle reaches x = 8 and despawns at ~9400; the
frozen `$14` occupies slot 9, so every later `$A460`/`$C64D` spawn takes a
different slot. Over 297-10350 the run drops to 87.0% slot matches and up to
four extra live objects. An unrelated spawn difference resurfaces at 10106,
so a later window will need its own analysis after this fix.

### Rule for the core card

In `ai_large_object` both drift calls must use the `$AEDB` entry, not the
`$AED7` one. The two-call structure and the `$016C`/`$012C` guards stay as
they are:

```rust
// $AEDB: called directly from $CA58, past the $AED7 `$5B` test, so the
// `$82` hold does not freeze a $14 object.
fn drift_large_object(&mut self, object: &mut Object) {
    object.drift();
    if object.x.int < 0x08 {
        object.despawn();
    }
}
```

Replace the two `self.drift_object(object)` calls inside `ai_large_object`
with `self.drift_large_object(object)`. Do not change `drift_object`, `ai_drift`
or `ai_background_turret`: those model `$AED7` and must keep the
`scroll_stop || scroll_locked` gate (see `longrun_drift.md`).

### Test that would prove it

1. Core (`game.rs`, `sandbox()` with `section = 4` and `scroll_locked = true`):
   a kind-`$94` object with `anim = 1` (attached cannon) must still lose
   `$80` from its X fraction per `ai_large_object` call (oracle `6F.00` ->
   `6E.80` at 8571 -> 8572), and with `anim = 0` it must lose `$100` (the two
   `$AEDB` calls). Today the first case leaves X unchanged.
2. Diff: recut the stage-5 fixture as `--frames 10350 --from 297` and require
   `stage5_object_drift_window_is_exact`: `within_tolerance()` over
   297-10350, `obj.x.slot 9` matching from 8572 on, and
   `object_slot_matches == object_slot_pairs` (it is 26027/29904 today). A
   cheaper gate is `--frames 8600`, where only `obj.x` diverges. The current
   297-8573 gate already covers 8572; it passes only because the 1 px gap is
   inside the default `--tolerance 1`, so the new test must compare `obj.x`
   exactly (or extend past 8574) rather than rely on `within_tolerance()`.

## 10. The 8642 leftover: the `$BF43` cannon debris in the object table (FIXED)

**Status:** fixed on `main` (`b675998`). `collide_shot_cannons` no longer
writes the shot record into `objects[]`; `stage5_debris_leftover.rs` pins the
phantom slot 3 absent. With §9 and §10 in place the clear window's first
divergence was 9597 `scroll` (oracle `0:11:128`, native `1:11:0`), not 8642;
the hold-countdown fix (`c2e1021`) closed it and the next one is 10671 (§11).

With the `$AEDB` fix in place the clear window
(`tests/fixtures/stage5-8574-12003.state`) is exact from 8574 to **8641**. The
next divergence is **8642 `obj.slot`**: the oracle has no slot 3 and the port
has slot 3 kind 2. The report is 7506 divergences and object slots
6826/7829 (87.2%).

**The object.** Native slot 3, kind 2, x 0, y 0, substate 0, from 8642 to
8666 (25 dumped frames). The oracle never has a slot 3 in the whole window:
stage 5 uses slots 2, 4, 5 and 7-9 only.

**When it is born.** At 8641 both sides score the `$BF43` cannon weak point
(290 -> 390). Slot 9 is the `$14`/`$94` large object of §9 with one `$0600`
cannon; the shot that hits is shot-pool slot 3 (second shot of emitter 0,
`$A22F`: `which * 3 + emitter`). The ROM writes the kind-2 debris into that
*shot* slot; the port writes the object table with the shot index:

```rust
// game.rs, collide_shot_cannons ($BF43)
let parent = usize::from(cannon.parent);
self.objects[parent].anim = self.objects[parent].anim.wrapping_sub(1);
self.cannons[record] = Cannon::default();
let debris = &mut self.objects[shot_index]; // shot index into the object table
debris.active = true;
debris.kind = 2;
debris.anim = 2;                            // $AE6B path 2, $014C = 0
debris.anim_div = 0;
...
```

Shot index 3 therefore lands in object slot 3, which no stage-5 spawn uses:
the record is uninitialised, so x/y stay 0 and `sprite` stays 0. `sprite == 0`
keeps it out of the dump on the kill frame, and the first `$AE93` sprite step
on 8642 makes it visible: that is why the report starts at 8642, not 8641.

**How long it lives.** 26 live frames, 25 dumped (8642-8666). `$BF43` sets
`$014C = 0` (`anim_div = 0`) and `$016C = 2` (`anim = 2`, the five-sprite
`$AE6B` path `6B 6A 69 68 6A 00`), so the sprite changes immediately and then
every five frames, and the slot despawns on the `$00` terminator during
**8667**. It cannot despawn on x: 8574-9600 is the `$82` pre-boss hold
(scroll `0:11:128`, frozen), so `$AED7` is gated and x never leaves 0; on an
ungated frame the `x < 8` test would despawn it at once.

**Why the other restos coincide.** In 297-8573 the oracle has nine kind-2
runs, all in slots 2, 4, 5, 7, 8 and 9 and all 20 dumped frames long. Those
are `$BE8D`/`turn_to_debris` object kills: they keep the killed object's
position, set `$014C = 3` and take path 1 (`anim = 1`, four sprites), i.e.
three hidden frames plus four sprite steps. The 8642 leftover differs in
three conditions: (a) it comes from the `$BF43` cannon weak point, not from
`$BE8D`; (b) that debris belongs to the shot pool, not to the object table;
(c) the port indexes the object table with a shot index, and slot 3 has never
been a stage-5 object slot (none of the nine restos touches it). The x 0, y 0
and substate 0 are only the symptoms of writing an uninitialised slot.

**Rule for the core card.** `collide_shot_cannons` must never index
`self.objects` with `shot_index`. Keep the +100, the parent's `$016C`
decrement (`anim -= 1`), the `Cannon::default()` clear and `consume_shot`.
The `$BF43` debris is the shot record, and the port's shot kinds 0-3 are shot
types (`2` is the rising double shot), not debris kinds, so it must not be
stored in `self.shots` either: the object-table record is simply not created.
Delete the `let debris = &mut self.objects[shot_index];` block and its field
writes. A visible burst, if it is wanted later, belongs to the separated
effect pool (`Explosion`), never to `objects[]`.

**Test that would prove it.**

1. Core (`game.rs`, `sandbox()` with `section = 4` and one `Cannon` record on
   an object in slot 9 whose part 2 is inside the shot box): an active shot in
   `shots[3]` must score +100, clear the record, consume the shot, and leave
   `game.objects[3]` not live with slot 9 still kind `$94`. Today
   `objects[3]` becomes kind 2 at x 0, y 0.
2. Diff: `crates/gradius-diff/tests/stage5_debris_leftover.rs` pins both
   measurements above. After the fix, replace the `frames == 8642..=8666`
   assertion with the exact gate used by `stage5_clear_replay`:
   `object_slot_matches == object_slot_pairs` and every field at 100% from
   8642; the window's `object_slot_pairs` drops by 25. The window is still
   not exact after this fix: the next divergence was **9597 `scroll`**
   (oracle `0:11:128`, native `1:11:0`), closed later by `c2e1021`; today the
   first residue is **10671 `obj.x`** (§11). The 11885/11920 debris alignment
   recorded before the `$AEDB` fix is no longer the first residue.

## 11. The 10671 `obj.x`: the Big Core entry branch must fall through (`$B9AC`)

**Status:** open. With §9 and §10 in place and the `$82` hold countdown fixed on
the chain frames (`c2e1021`), the clear fixture `tests/fixtures/stage5-8574-12003.state`
is exact from 8574 through **10670**. The first divergence is **10671 `obj.x`**:
oracle slot 7 `166`, native slot 7 `164` (report: 7382/7603 object slots,
`scroll` still 100%).

| frame | oracle px | oracle core (x,y) | native core (x,y) | note |
| ---: | ---: | --- | --- | --- |
| 10668 | 161 | 163,72 | 163,72 | parked at `$A3`, fight path |
| 10669 | 162 | 163,73 | 163,73 | player reaches the core |
| 10670 | 163 | **165,74** | **165,74** | `$BA0C`/`$BA0F` +2, last exact frame |
| 10671 | 164 | **166,76** | **164,74** | first divergence |
| 10672 | 165 | 167,77 | 163,74 | native in the entry branch |
| 10673 | 166 | 168,78 | 165,76 | native back on the fight path |
| 10674 | 167 | 169,79 | 164,76 | 3-frame cycle visible |
| 10675 | 168 | 170,81 | 163,76 | |

**The object.** Slot 9 is the shared Big Core, kind `$98`, `$048C = 1`
(`substate` in the dump), spawned at 10274 (`$3F = $0C`, §6). Slots 7 and 8 are
its two `$99` covers; `$BA65 JSR $B9B1` re-attaches them from the core every
fight frame (lower at the core's y, upper at y - 20), so they inherit the same
x/y and the report names slot 7 first only because slots are compared in
ascending order. Nothing else is involved: no `$0600` record, no cannon, no
shot.

**Where the two paths separate.** The ROM's handler (`$B90E`) starts with the
armour/death checks (`$B929`), then the entry/fight split at `$B9A2`:

```text
$B9A2 LDA $036C,X
$B9A5 CMP #$A4
$B9A7 BCC $BA04      ; x < $A4 -> fight path
$B9A9 DEC $036C,X    ; entry: one pixel left per frame
$B9AC LDA $048C,X
$B9AF BNE $BA04      ; damage gate open -> the frame still runs the fight path
$B9B1 ...            ; gate closed -> rebuild the covers, RTS
```

The port's `ai_big_core` models the first half and then **returns
unconditionally**:

```rust
// `$B9A5`: one pixel per frame until the recorded stop at x = $A3.
if object.x.int > 0xA3 {
    object.x.add(-0x100);
    self.sync_boss_parts(object.x.int, object.y.int);
    return;                                   // missing the `$B9AF` branch
}
```

That return is invisible while the gate is closed, which is why the window
matched this far: `$048C` is 0 through the entry (it starts toggling at the
first `$04CC` wrap) and the core then parks at x `$A3`, where `CMP #$A4` takes
the `BCC` and both sides run the fight path. The first frame that begins with
x > `$A3` **and** the gate open is 10671: the `+2` track on 10670 leaves the
core at 165. From there the ROM spends -1 (entry) then +2 (`$BA0C`, player now
at 164) = **+1 per frame**, so the oracle core tracks the ship as `x = px + 2`
(10670-10681, then back down with the ship) and keeps its rank vertical
velocity (1 px + fraction per frame). The port spends 10671 and 10672 in the
entry branch (165 -> 164 -> 163) and only reaches the fight path when x is
back at `$A3`, i.e. on one frame in three: its y therefore advances 1 px per
three frames and its x cycles 165/164/163.

**This is a new cause, not a misapplied one of the fixed rules.** There is no
aimed vector (`move_aimed`/`$BDF4`), no Y clamp (`$18..$A8`; the oracle's y 76
is well inside and the port's error is a *skipped* vertical step, not a clamp
edge), no `$AEDB` drift (`$036C` here is written by `$BA0C`, and no `$038C`
drift call is involved) and no `$CB8B` cannon tick (`$5C`/`$0600` play no part).
It is the core's own handler, and the previous fixes only moved the window
here.

### Rule for the core card

In `ai_big_core`, the entry branch must fall through into the fight path when
`$048C != 0`; only the closed gate rebuilds the covers and returns:

```rust
// `$B9A2`: while x >= $A4 the entry loop spends one pixel per frame. `$B9AC`
// then tests `$048C`: with the gate open the frame falls through into
// `$BA04` (track, re-aim, vertical step, timers); only `$048C == 0` rebuilds
// the covers at `$B9B1` and returns.
if object.x.int > 0xA3 {
    object.x.add(-0x100);
    if object.ai_state == 0 {
        self.sync_boss_parts(object.x.int, object.y.int);
        return;
    }
}
```

`ai_state` is the port's `$048C` (set at `$04CC == 1`, cleared at `>= 5`), so
the test is exactly the ROM's `BNE`. The `$AA`-side effects stay where they
are: the track uses the already-decremented x, and the fight path's own
`sync_boss_parts` call is the `$BA65 JSR $B9B1` that re-attaches the covers.

**Caveat (unmeasured, not part of this fix).** The ROM runs the armour and
death checks (`$B929`-`$B947`) *before* `$B9A2`; the port runs the entry block
first. No measured frame in 8574-12003 exercises the combination (an armour
change or the seventh hit while x > `$A3`), since the gate-open frames with
x > `$A3` start at 10671 and the kill is at 11489. If the entry block is moved
at all, put it after the death/armour block so the order matches the ROM; the
minimal fix is only the gate above.

### Test that would prove it

1. Core (`game.rs`, `sandbox()` with `section = 4`): a Big Core at x 165
   (`$A5`) with `ai_state = 1` (gate open) and a player at x 164 steps
   `ai_big_core` once -> x must be **166** (the -1 entry then the +2 track)
   and `timer2` must have advanced (the `$BA6A` counter), and y must have
   moved by the rank velocity. With `ai_state = 0` the same call must leave
   x 164 and `timer2` untouched (the `$B9B1` return). Today the first case
   gives x 164 and a frozen timer.
2. Diff: `crates/gradius-diff/tests/stage5_core_entry_10671.rs` pins the
   measurements above (window 8574-10690, first divergence 10671 `obj.x`,
   oracle 166 / native 164, oracle `x = px + 2`, native cycle 165/164/163).
   After the fix, `stage5_clear_replay` must be extended from 8574-12003 to a
   new `--frames 10690` cut and the next divergence pinned; the 297-8573 gate
   is unaffected (x stays `$A3` there).

## Gaps and caveats

* The natural stage-5 entry state is a mix of `record-boss --stage 3` (frame
  10608, scroll, score) and a forced short path (`$1B = $86` at a chosen frame
  with `$19 = 2` -> `$8E` -> `$96C2`) for the zero-page bytes. The forced path
  skips the 968-frame `$8E` scroll, so its `$3E` is 17 instead of the natural
  5; `$02` is therefore derived as `(frame - 5) & $FF` and cross-checked at
  frames 302 and 323, and against `stage4_boot.md` (`$02 = 44` at frame 29257,
  i.e. 29252 & `$FF`).
* `probe` has no `--assist`; it cannot enter the `$1B = $82`/`$8E` sprite-0
  wait. The `$82`-hold script and the boss-cue timing were read from
  `record-boss` objects and a forced `$3F = $0C` probe, not from a natural
  assisted probe. `record-ending --stage 5 --frames 13000` did not reach the
  boss (fixed fire stalls at scroll ~617 px), so it was not used for the clear.
* The "count is `b2 >> 4`" note in `stage5_plan.md` contradicts `$A4FA` (the
  low nibble is consumed first) and the 2900 probe (slot 8, `b2 = $12` -> 2
  records; slot 9, `b2 = $01` -> 1).
* `$0C` has no located writer (it reads `00`); the card's `$0C/$0D/$0E` also
  match the `$9A30` / spawn / `$98F0` gate constants for stage 5.
