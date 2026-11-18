# Stage 2: extracted background and roots

Stage 2 is `$19 = 1`. This note records how the oracle is driven into stage 2,
what background data was extracted, and what still has to be wired into the
port. No gameplay code or stage-1 data was changed.

> **Status: handoff-window DONE; boss, end and the whole natural window FIXED
> (2026-11-12).** The documented handoff window 9601-12000 is exact (0
> divergences, every field 100%); the pixel gate 9601-11199 is 99.9% overall /
> 99.9% play / 100.0% HUD, and the full 9601-12000 report is 99.7% / 99.7% /
> 100.0%. The natural window **12001-19430 is now exact too: 0 divergences
> over 7,430 frames**, including the 15968 `$C540` eruption, the boss
> (spawn 17282, self-destruct 18915) and the stage-3 entry at 19429. It is
> gated ROM-free by `tests/fixtures/stage2-12001-19430.state` and
> `crates/gradius-diff/tests/stage2_window.rs`. The eruption was fixed by
> `2c770c6`, not by a `$C540` change. See "Boss and end" below.

## Reaching stage 2

The game's stage init at `$9B38` copies the per-player stage from `$26,X`
(`X = $18`) into `$19` at `$9B68`, then builds the background from the `$19`
roots. In a normal single-player run the boot path clears `$26` and this read
happens at frame 264, so a fixed poke has to land on that one frame. The
extractor avoids the timing dependency by writing `$26 = 1` every frame until
the game is playing the requested stage; the write stops as soon as
`$00 == 5` and `$19 == 1`.

The frame-264 timing is still useful as a one-shot probe:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stage2
cargo xtask probe "$ROM" --frame 500 --press 180:start \
  --poke 263:26:01 --keep-alive --ram /tmp/stage2-ram.bin
```

At frame 500 the RAM shows `$19 = 1`, `$00 = 5`, and the background palette is
stage 2's rather than stage 1's (bg palette 2 holds the green `$15` instead of
stage 1's `$19`/`$2A`). Poking `$19` directly does not work:
before frame 265 it is overwritten by stage init, and afterwards the background
has already been built. Poking `$26` on frame 265 or later starts too late;
frames 263-264 work, and the per-frame loop used by `screens --stage` removes
the frame guess entirely.

## Extraction command

```sh
cargo xtask screens "$ROM" --stage 2 --frames 20000 --assist \
  --press 180:start --out crates/gradius-core/src/stage2_bg.rs \
  --png /tmp/stage2.png --dump /tmp/stage2.dump
```

`--stage 2` also pokes `$26`; `--assist` is the existing HUD sprite-0
workaround. The run reaches the stage's scroll end (`$3F = $0E`) without a
checkpoint restart:

| quantity | value |
| --- | --- |
| map | 480 x 28 tiles (3840 px wide) |
| scroll | 0..3584 px, last 3584, 0 restarts |
| writes | 73664 tile / 9207 attribute / 64 palette |
| missing | 0 tiles, 0 attributes |
| CHR bank | 0 for the scrolling section (bank 2 from world column 416) |
| tiles FNV-1a | `F207532E288F4003` |
| attributes FNV-1a | `6988DE65EF5F569F` |
| palette FNV-1a | `DF2BBDFA95164A70` |
| palette | the 32-byte `$3F00` page (four background and four sprite palettes), read from the cartridge at run time (`gradius-extract`) |

The module exports `stage2_bg::STAGE2` (same `Screen` type as
`stage1_bg::STAGE1`, re-exported from `stage1_bg`), the RLE streams and the
roots below. Its tests pin the three hashes and the map shape.

## Per-stage roots (from `xtask stages`)

| content | value |
| --- | --- |
| `$19` | 1 |
| spawn `$A7CA[$19]` | `$A7E8`, 8 columns, 101 `$A65C` terrain placements |
| background height map `$9FB6` | `$CF64` |
| background base `$9FC6` | `$D380` |
| background tile stream `$9FD6` | `$D772` |
| background attribute parameter `$9FAE` | `$41` |
| second music theme `$8344` | `$59` |
| collision/attack `$C433` | `$C540` |
| scroll end `$98F0` | `$0E` |
| boss threshold `$9A30` | `$0C` |
| start row `$9BC6` | `$05` |

Stage 2 shares the `$D772` tile stream with stages 1, 4 and 5, but has its own
height/base pair (`$CF64`/`$D380`) and attribute parameter (`$41`), so the
decoded map is not stage 1's.

## Spawn and terrain tables (`stage2_tables`)

`cargo xtask stage2-tables "$ROM"` walks `$A7CA[1]` = `$A7E8` with the same
`$A32F` parser as `xtask sections` and emits the data fragment in
`crates/gradius-core/src/stage2_tables.rs`. The command only names the streams,
their records and the resulting terrain placements; it invents no bytes.

| column (`$3F & $0E`) | stream | records | `$A65C` terrain |
| --- | --- | --- | --- |
| 0 | `$A8FD` | 10 | 0 |
| 1 | `$A912` | 18 | 12 |
| 2 | `$A937` | 25 | 21 |
| 3 | `$A96A` | 20 | 19 |
| 4 | `$A993` | 17 | 11 |
| 5 | `$A9B6` | 3 | 0 |
| 6 | `$A96A` | 20 | 19 |
| 7 | `$A96A` | 20 | 19 |

The stage stores 133 stream records in total, of which 101 are `$A65C` terrain
placements (`type < $80`). Stage 2 has **no** five-byte command records: every
record is a two-byte `<delay> <type>` pair terminated by `$FF`. Columns 3, 6 and
7 share the same end-of-stage stream (`$A96A`); column 7 is selected only at
`$3F & $0E = $0E`, past the `$9A30[1] = $0C` boss threshold, so normal play
never reaches it. The pre-boss playfield is `$0C * 256 = 3072` px; the scroll
end `$98F0[1] = $0E` (3584 px) follows the boss.

The record format is the stage-1 one (`docs/analysis/stage1_data.md`): a spawn
record waits until `$3F >= $61 + 2*delay`, then `type >= $80` selects a 4-byte
`$A5FC` object definition (`type & $3F`), while `type < $80` places a 3-byte
`$A65C` terrain definition. Both definition tables are global, not per-stage;
`stage2_tables` re-exports them from `stage1_tables` instead of duplicating
them. The decoded module exposes `section()`, `column()`, `terrain()`,
`length_px()`, `object_def()` and `terrain_def()`; it stores placement evidence
only (stream delay and definition index) and infers no hitbox.

Its tests pin the counts above and check the streams against
`stage1_tables::SECTIONS[1]` and the terrain against
`stage1_collision::terrain(1)`, both decoded from the same ROM roots. Nothing
here is wired into `game.rs`.

## Display changes

The whole scrolling section uses CHR bank 0 and the palette above. At world
column 416 (3328 px) the oracle switches to CHR bank 2 and blackens background
palettes 2 and 3 (`0F 0F 0F 0F 0F 0F 0F 0F`) for the stage tail; the final
512 px are starfield and contain no terrain. `stage2_bg::STAGE2` stores the
dominant (long-section) bank and palette; the measured change list is also in
the generated module header.

## Runtime verification

`xtask screens --sample-scroll` compares the exported map with the oracle's
live nametable, and `--stage 2` with any sample also traces the spawner. Repeat
the flag once per scroll:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stage2
cargo xtask screens "$ROM" --stage 2 --frames 20000 --assist \
  --press 180:start --sample-scroll 0 --sample-scroll 512 \
  --sample-scroll 1024 --sample-scroll 1500 --sample-scroll 3328 \
  --sample-scroll 3400 --sample-scroll 3584
```

Measured 2026-11-09 on `Gradius (E).nes` (branch `agent/verify2`):

| scroll | tiles / attributes | CHR bank | palette |
| ---: | ---: | ---: | --- |
| 0 | 0 / 0 | 0 | dominant |
| 512 | 0 / 0 | 0 | dominant |
| 1024 | 0 / 0 | 0 | dominant |
| 1500 | 0 / 0 | 0 | dominant |
| 3328 | 0 / 0 | 2 | tail (palettes 2-3 `0F`) |
| 3400 | 0 / 0 | 2 | tail |
| 3584 | 0 / 0 | 2 | tail |

The comparison samples the 32 world tile columns visible at each scroll, so the
four scrolling samples cover columns 0-31, 64-95, 128-159 and 187-218, and the
tail samples cover 416-479. No tile or attribute mismatch was found and the
three hashes above are unchanged.

A scroll value can last several frames while stage init builds the background,
so a request keeps refreshing until the scroll moves on and compares the last
frame at that value. This matters at 0: the first stage frame still shows the
title nametable and its palette, which produced 110 tile and 224 attribute
false mismatches before the sample was settled. The extraction itself needed no
change.

### Terrain records

The same run follows the `$A32F` stream pointer `$6A/$6B`, reads each consumed
two-byte record from PRG, and compares every `kind < $80` record with
`stage2_tables`' column stream and with its consumption threshold
`($3F & $0E) * 256 + 2 * delay` pixels. Result:

| quantity | value |
| --- | --- |
| terrain records consumed | 63 |
| record mismatches | 0 |
| position mismatches | 0 |
| missing pre-boss records | 0 |
| worst frame lag past threshold | 0 px |

Only columns 0-5 are reached: the boss transition starts at
`$9A30 = $0C` (3072 px) and the spawner returns early during `$81`/`$82`. The
remaining 38 placements belong to columns 6 and 7, at or past that transition,
and were not consumed while the boss gate was active. That is the documented
column-7 rule, not missing data. Columns 3, 6 and 7 share `$A96A`; the trace
uses `$61` to attribute each consumption to the right column group.

## What remains for gameplay

* Background build/selection: use `STAGE2` and the `$CF64`/`$D380` keys instead
  of stage 1's map. The rendered tail needs the bank-2/palette switch above.
* Spawner/terrain: the `$A7E8` column streams and all 101 `$A65C` placements
  are decoded in `stage2_tables`; what remains is the port-side spawner that
  consumes them (the current spawner reads `stage1_tables`).
* Collision: disassemble and port `$C540`; stage 2 also has the `$B95C`
  stage-gated site that sets `$39` when its mid-stage object dies.
* Audio: add theme `$59` or keep the shared `$93` intro until it exists.
* Stage flow: stage 2 uses the shared `$9664`/`$9A40`/`$9822` path; only the
  start row (`$05`) and the roots differ.
* Terrain solidity: `stage2_bg` has no `is_solid` table yet; stage 2's object
  definitions have to be extracted before a port-side approximation is useful.

## S2-00 boot slice (2026-11-09)

`Game::advance_section` now goes through `Game::enter_stage`: it increments
`$19`, clears `$3F` and the `$50-$70` block (`boss_stage`, the port-only
`boss_spawned` latch, `boss_timer`, `scroll_locked`, `$39`, `hazard_index`),
roots the spawner on `SECTIONS[$19]` and returns to `Phase::Normal`. It keeps
`$3E`/`$3D`, matching the handoff ledger (`$3E 0 -> 1` at 9601), and does not
clear the object pages, which `$96C2` leaves alone. `Game::stage_info()` and
`Game::stage_music()` expose the `stage_catalog` row; `Game::new_at_stage`
starts a run at any stage for tests and offline tooling.

`Game::render` selects `stage_catalog::background($19)` instead of the
hardcoded `STAGE1`. Stage 2's `$834B` mode-0 tail is a renderer rule: from
world column 416 the playfield fetches hardware CHR bank 2 (pack bank 4) and
palette RAM entries 8-15 are forced to `$0F`; palettes 0-1 keep the extracted
values. Stage 1's path is byte-identical (`tail = false`).

### Evidence

Run with the boss script and both keep-alives:

```sh
gradius-diff --rom "$ROM" --pack assets/gradius.pak \
  --script scripts/boss-script.txt --keep-alive --native-keep-alive \
  --frames 12001 --from 9500 --to 12000 --report --json ...
```

* state window 9500-12000: 2501 frames compared, `WITHIN TOLERANCE
  (0 divergences)`; every field 100%, including `obj.kind` 13479/13479 and
  `scroll` 2501/2501. Stage 2 starts at 9601 and runs its own `SECTIONS[1]`
  stream through 12000.
* pixel window 9600-12000 (21 samples): 99.6% mean match, playfield 99.6%,
  HUD 99.5%. **First divergence: frame 9600 in the HUD at
  `(205,214)-(210,218)`, 13 px** (the known power-meter bar area). The
  playfield is 99.9-100% through 11040 and drops to 98.6-99.2% from 11280
  (bbox reaching y 238), with the state still exact; this is background/sprite
  rendering, not simulation divergence.
* stage-1 regression: 3000 frames, 2682 `Playing` compared, `0 divergences`,
  every field 100%.

### Remaining for stage 2

* Music `$59`/boundary 4 (S2-02) and the `--stage` harness (S2-04) are
  unchanged.
* Pixel residuals after 11200 are presentation-only: enemy animation frames
  and effect sprites whose animation fields are not in `dump_state`. See the
  S2-01/S2-03 close-out below.

## S2-01/S2-03 close-out (2026-11-09)

Implemented in `game.rs`/`hud.rs`/`stage2_tables.rs`:

* **`$82` clear timer is rank-based.** `$9A01` latches `$4D = $9A28[$17]`
  and `$4C = 0`, so the hold is `$9A28[rank] * 256` frames. The port now
  computes the ROM rank `(weapon != 0) + options + ($46 != 0) + ($19 != 0)`
  and uses `CHECKPOINT_TIMER[rank] * 256 + 3`. Stage 1 with the pinned shield
  is rank 1 -> `3*256+3 = 771`, byte-identical to the old constant. Stage 2
  adds the stage bit -> rank 2 -> `4*256+3 = 1027`, exactly one 256-frame
  tick longer, matching the measured `record-boss --stage 2` spawn being 256
  frames after stage 1's.
* **`$C540` launcher script.** `$C40D`'s stage-2 dispatch now runs every 8th
  frame (`$02 & 7 == 0`), advances `$69` through the `$C587` ping-pong table
  (`stage2_tables::HAZARD_SCRIPT`) and spawns kind `$0B`, sprite `$67` at the
  `$C567` pair (`HAZARD_POSITIONS`). The `$3A != 0 -> $C680` stage-3 hook is
  documented but not implemented (stage 3 is out of scope).
* **Launcher fire.** `$B3A4`/`$BCAF` fires one aimed shot on the first
  activated frame; the port now spawns it with the `$8F45` group `$25`
  (red/white cross), separately from the `$90BA` group `$3A` used by the
  player-visible enemy bullets. The muzzle offset (`$0496`) is approximated
  by the object centre, so bullet positions can differ by a few pixels.
* **HUD.** An owned shield (`$46 != 0`) blanks the sixth meter entry's `$62`/
  `$63` glyph tiles (`$1E`), and the fixed bar starts at `HUD_Y = 210`; rows
  208-209 keep the playfield background as measured. This closes the 13 px
  `(205,214)-(210,218)` gap and the late-window y 208-209 rows.
* **Aiming turrets.** `$B020` stores `$B080,Y` in `$012C`; `$B086`/`$B08C`
  are the `$0496` muzzle direction, not sprites. The port now keeps the
  turret group and applies the vertical `INY` only in the far sector, matching
  oracle RAM (`$72/$74` vs the old `$75/$73`).
* **Title re-entry.** The `$12` roll-in gives a 130-frame re-entry title
  (133 at boot); `title_attract_at` is set on the game-over and demo-end
  paths. The default 12,000-frame run now has **0 state mismatches** (was 22)
  and the demo windows line up.

Evidence (boss script, both keep-alives, `--pixel-window 9601:12000`):
state `9500-12000` is still `WITHIN TOLERANCE (0 divergences)`, every field
100%. Pixel: `9601-11199` is `99.9%` mean match (raw 99.87%, the pre-turret
gate), `9601-12000` is `99.7%`; HUD is `100.0%`. Residuals from 11200 are
enemy animation/effect frames only; no state field diverges. Accepted residue:
the 9601 entry frame (60 play px of stage-1 tail), the `$25` launcher shots
drawn from the object centre instead of the `$0496` muzzle, and the >11280
animation/effect frames; the report-only 9601-12000 window is 99.7%.

## Boss and end (natural path, 2026-11-09)

The handoff window ends at 12000; stage 2's own boss and end were re-audited on
the natural path in [stage2_boss.md](stage2_boss.md) after the `agent/s2form`
fix (`ea1ecbe`) and now match:

* **Spawn matches.** Both sides first show slot 9 kind `$98` at frame
  **17282**, with byte-identical `$99` covers in slots 7/8. The pre-spawn
  object table is clean (same final frame at 17281).
* **Boss window is exact.** `--from 17280 --to 19430` reports `WITHIN
  TOLERANCE (0 divergences)` over 2151 compared frames, every field and object
  slot at 100%. The old `obj.y` drift from 17370 is gone: `Game::boss_rank`
  now includes the stage term that `$9C3F`/`stage_rank` apply, so the run uses
  the ROM rank-2 vertical speed and fire period.
* **Clear and hand-off reproduce.** Both sides self-destruct at **18915**
  (slot 9 kind-2 debris, no score reward; score stays 2760, the overheat path)
  and stage 3 begins at **19429** with `scroll=1:0:0`. The `$ADA5` write-back
  no longer revives the core, and the death frame holds the scroll, matching
  the 514-frame hand-off.
* **Post-window formations are fixed.** The first divergence used to be
  **12161**: the stage-2 `$83/$84` two-ship formation spawned 2 frames late and
  one member ahead (native y = oracle y + `y_step` = +0x40). The `$88`
  formation at 14210 and the 14564/15076 waves were the same class; the
  spawner fix (charge `$69` before the slot search, retry a failed allocation
  without advancing `$6C/$6E`, report via `Spawner::commit`) removes them, and
  12001-15967 is clean.
* **Exact natural window.** `--from 12001 --to 19430` reports `WITHIN
  TOLERANCE (0 divergences)` over 7,430 compared frames, every field at 100%.
  The former 15968 `obj.y` divergence is gone: `2c770c6` has `$BCAF`
  complement a negative delta to `|d|-1` and lets the rank-2 `ADC` keep its
  carry, with slope `q + q/4 + 1`; `$C540` needed no change of its own.

The boss/end **pixels** remain report-only: the 17280-19430 sample is 97.3%
match / 96.8% play / 100.0% HUD, first divergence at the spawn frame 17282,
with no compared state field diverging. The stage-2 pixel gate stays the
documented 9601-11199 window.

The `--stage 2` landmarks quoted elsewhere (spawn 7979, first damage 8544, end
9280, stage 3 at 9794) are the fresh-poke **damage-kill** reference, not the
natural parity path: `Game::new_at_stage` cannot reproduce the natural entry
(it is 5 frames ahead in scroll and 2 frames ahead in object age). The natural
core is never shot in the boss-script run, so the oracle ends it by overheat.

Consequence for the ledger: stage 2 is handoff-window DONE and boss/end FIXED;
the 12161 formation class is FIXED and the 15968 `obj.y` eruption is FIXED in
`2c770c6` ([KNOWN_DEVIATIONS.md](../KNOWN_DEVIATIONS.md)), so the whole natural
window 12001-19430 is exact and gated by `stage2_window.rs`.

