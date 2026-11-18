# Stage 4 implementation plan

Executable plan for `$19 = 3` (stage 4). It starts from the same
stage-1-shaped runtime as `stage2_plan.md`, `stage3_plan.md` and
`stage5_plan.md`: `Game::new` always starts at section 0, `game.rs::render`
draws `stage1_bg::STAGE1`, the boss gate uses the stage-1 literals, and
`$C433[$19]` is not modelled. Stage 4's streams, 63 terrain placements and
`STAGE4` screen are already decoded. The work is selection, the background/tail
rule, music `$5D`, the shared core gate, the `$C5A7` hazard script and the
differential harness.

Stage 4 has **no new AI handler and no five-byte command record**. Its 23 raw
types all resolve to kinds that already have a `game.rs` arm, and its scripted
hazard is kind `$15` (`ai_arc(3, true)` in the port). It is the cheapest stage
to wire after stage 2, and the ROM has no `$19 == 3` branch of its own.

## Stage-4 facts to build against

From `cargo xtask stages`, `cargo xtask sections` and
`cargo xtask stage-enemies` (checked 2026-11-09):

| field | stage 4 value |
| --- | --- |
| `$19` | 3 |
| spawn root `$A7CA[3]` | `$A806`, 7 columns |
| `SECTIONS[3]` | 111 records, all two-byte `<delay> <type>`; 0 commands |
| terrain placements | 63 (`stage1_collision::terrain(3)`) |
| formation members | 48 |
| raw types | 23: `06`x2 `07`x3 `34`x7 `35`x4 `36`x5 `37`x1 `38`x3 `39`x8 `3A`x9 `3B`x17 `6A`x2 `77`x2 `83`x3 `84`x1 `87`x20 `89`x5 `8A`x3 `8B`x7 `8F`x1 `90`x1 `94`x2 `95`x2 `96`x3 |
| kinds | `$04` 18, `$05` 8, `$06` 22, `$07` 9, `$08` 20, `$0D` 1, `$0E` 1, `$0F` 2, `$10` 3, `$11` 7, `$12` 4, `$13` 12, `$27` 2, `$29` 2 |
| background | `stage_bgs::STAGE4`, 480x28 (3840 px), dominant CHR bank 0 |
| background height/base | `$9FB6` `$CF56` / `$9FC6` `$D188` |
| background tiles/attr | `$9FD6` `$D772` (stage 1's tile stream) / `$9FAE` `$40` |
| tail | from world column 416 (3328 px): CHR bank 2, palette entries 8-15 `0F` |
| palette | the 32-byte `$3F00` page (four background and four sprite palettes), read from the cartridge at run time (`gradius-extract`) |
| tiles / attrs / palette hashes | `228C903AE305586D` / `9E8E0821170D5B2A` / `90FA63C90403F3FC` |
| music `$8344` | `$5D` (`Music::Stage4`) |
| boundary `$8354` / mode `$834B`/`$2D` | 4 / 0 |
| `$98F0` end | `$0E` |
| `$9A30` boss threshold | `$0C` (spawn at `$3F = $0D`) |
| `$9A28` clear countdown | 4 (`$82`; `transition_frames(3)` = 7) |
| start row `$9BC6` | `$00` |
| script `$C433[3]` | `$C5A7` |
| boss | shared Big Core `$98`, 7 hits, +1000, 514-frame clear |

Measured with `cargo xtask record-boss "$ROM" --stage 4 --frames 12000`:

```
spawn 7979, scroll stop 7978, x stop 8054, first damage 8482
overheat wraps 8314 / 8574 / 8833 / 9093
death 9218 (+1000), stage 5 from 9732
```

That is the normal clear path (`$39 == 0` -> `$96C4 INC $19`). No located code
sets `$39` for stage 4; the shared `$9928` branch must not gate this stage.
The spawn score was 1600.

## The `$C5A7` hazard

`$C40D` runs every 4th frame (`$02 & 3 == 0`), scans slots 9 down to 0 for a
free `$030C`, clears the slot with `$A521`, and dispatches `$C433[$19]` after
the global `$3A != 0 -> $C680` quirk. Stage 4's handler is `$C5A7`; it is a
scripted object, not the boss, and must not set `boss_spawned`.

`$C5A7` is the same shape as stage 1's `$C480`, with its own tables:

* first call (`$69 == 0`) plays sound `$0F`;
* `$C449` with `X = 4` selects the nibble table at `$C62D`, increments `$69`,
  reads one packed nibble and stores `$A9 = nibble * 2`, `$AA = $69 & 1`;
* the record index is `$A9 + ($A9 >> 1)` into the 16 3-byte entries at
  `$C5FD`;
* the record supplies `$042C`, `$03BC` and the `$048C` offset; `$03BC` is
  decremented twice when `$69 < $1E`;
* `$048C = ($02 & $0F) + record.byte2`, `$04AC = 1`;
* X comes from `$C5FB[$AA]` = `[0x38, 0xB8]`, kind is `$15`, Y is `$2C`;
* the common `$C4DE` tail sets `$044C`/`$03EC = $02 & $3F` and sprite `$58`.

With the measured `$82` window (4 frames) `$C40D` normally runs once, so the
observed spawn is the first call: table index 0, low nibble `$F`, record 15
`FE 08 60`, `$03BC = 6`, X `$B8`. The 16-entry table carries the same shape as
stage 1's `$C480`; only the first entry is reached in stage 4's normal window.
Do not turn `$C5A7` into an every-frame spawner.

## Current code anchors

| gap | current code |
| --- | --- |
| stage entry | `Game::new` hardcodes `section = 0`, `Spawner::new(0, 0)`; only `Game::section()` exists |
| spawner | already indexes `SECTIONS[section]`; `SECTIONS[3]` works if `section = 3`; column 7 is empty |
| terrain | `terrain(3)` is placement evidence; ship-vs-map collision is always on and reads the packed `COLLISION_MAP` |
| background | `game.rs::render` uses `stage1_bg::STAGE1`; `draw_screen` has no tail argument |
| boss gate | `normal_frame` uses `0x0C`, spawn wait `0x0D`, `boss_timer = 771` |
| stage end | `boss_clear` gate is `(section != 0 || stage_clear_ready)`; stage 4 must use the normal `$98F0`/514 path, not `$39` |
| script | `$C40D`/`$C424`/`$C433` absent; no `$3A`, `$68` or `$69` model |
| command path | stage 4 has no five-byte records; do not change `$A4A0`/`$A469` for this stage |
| music | `gradius-app::music` emits only `Stage1`/`Stage1B`; `Music::Stage4` exists in `gradius-audio` |
| harness | `gradius-diff` has no `--stage`; native `Game` has no stage entry |

`stage_bgs::STAGE4` and its hashes are generated data; do not edit the RLE or
hand-maintain a second stage-4 screen. `STAGE4` reuses the `$D772` tile stream
but has stage-4 height/base and palette.

## Implementation steps

1. **Stage record and stage-4 entry.** Add the shared `stage::Stage` record and
   the row for `$19 = 3`: background `stage_bgs::STAGE4`, music `$5D`,
   boundary 4, mode 0, boss threshold `$0C`, end `$0E`, start row `$00`, tail
   column 416, tail bank 2, script `$C5A7`. Add
   `Game::new_at_stage(assets, 3)` (or `Game::enter_stage(3)`) that sets
   `section = 3`, `spawner = Spawner::new(3, 0)` and
   `place_player(stage::ship_start(3, 0))`. Do not add per-stage function
   pointers: the game branches on `$19` inline, like the original. If the
   stage-2/3/5 plan landed first, add this as a data row and reuse its
   constructor.
2. **`SECTIONS[3]` and `terrain(3)`.** Use the existing spawner row: 111
   records (63 terrain, 48 formation members, 0 commands). Column 7 is empty
   and is never selected before the `$0C` threshold. Do not add a
   `stage4_tables` duplicate; `SECTIONS[3]`/`terrain(3)` are the runtime
   source. Do not touch the command branches.
3. **`STAGE4` and the column-416 tail.** Select `STAGES[section].background`
   instead of `stage1_bg::STAGE1`. Add one renderer tail rule: when the
   displayed world position reaches column 416 (3328 px), use CHR bank 2
   (pack bank 4) and force palette RAM entries 8-15 to `0F`; entries 0-7 keep
   the extracted dominant values. The final 512 px are starfield and contain
   no terrain. `STAGE4` stores the dominant bank 0 and the stage-4 palette.
   Preserve stages 1 and 2 exactly.
4. **Music `$5D`.** Expose the stage and its `music`/`music_boundary` from
   `Game`; add `MusicCue::Stage4` in `gradius-app` and map it in the SDL and
   WASM backends. Stage 4 plays the shared intro (`$93`) until boundary 4
   (`$3F >= 4`, `0x0400`), then `$5D`; boss `$A5`, clear `$AC`, game over
   `$AF`. If the shared intro cue exists, use it; otherwise keep stage 1's cue
   list byte-identical.
5. **Shared core gate and stage-4 values.** Use
   `STAGES[section].boss_threshold`/`end_threshold` instead of the `0x0C`/
   `0x0D` literals. Keep the shared `$81 -> $82 -> $83 -> $84 -> $85`
   sequence; stage 4's `$82` is `$9A28`/`transition_frames(3)` (4 frames in a
   7-frame window), **not** the stage-1 771-frame eruption hold. Spawn the
   shared core at `$3F = $0D` (frame 7979), kill it at 7 hits, award +1000,
   and advance 514 frames after the death (9732). Do not require `$39` for
   stage 4. Keep stage 1 byte-identical.
6. **`$C40D`/`$C424`/`$C433[3] = $C5A7`.** Implement the dispatcher from
   `stages.md`: every 4th frame, free-slot scan 9 down to 0, `$A521` clear,
   `$3A != 0 -> $C680` (hook only; stage 3's body is out of scope), otherwise
   `$C433[$19]`. Add the stage-4 branch `$C5A7` exactly as decoded above:
   sound `$0F` on the first call, `$C449` with `X = 4`, `$C5FD` records,
   `$C62D` nibble table, `$C5FB` X table, kind `$15`, Y `$2C`, sprite `$58`,
   `$042C`/`$03BC`/`$048C`/`$04AC`/`$044C`/`$03EC` fields. Do not set
   `boss_spawned`, do not award boss points, and keep stage 7's `$C423` a bare
   `RTS`. If stage 2's `$C40D` or stage 3's dispatcher landed first, extend it
   rather than duplicate it.
7. **Stage-4 exceptions.** There is no `$19 == 3` code branch; do not invent
   one. Audit the shared `$AFF8`, `$BBB7`, `$BBD8`, `$BC42` sites for the
   `$19 != 0` branch and add tests. `$9F4D` is stage 5 only, `$AF48` is stage
   6 only, `$C76C` is stage 5, `$9865` is stage 7 and `$CD9F` is stage 6.
   The shared `$0F`/`$10` `$046C >= 5` death and `$5F` counter are still
   partial; stage 4 has five such volcanoes (2 `$0F`, 3 `$10`). If the diff
   flags them, add the shared death as a 3 h add-on, not a stage-4 branch and
   not the stage-1 `$39` increment.
8. **Harness and evidence.** Add `--stage N` to `gradius-diff`, start the
   native `Game` at the same stage as the oracle's `$26` poke, and run the
   state/pixel/boss windows below. Until this exists, native stage-4 evidence
   is unit tests only and oracle evidence is `record-boss --stage 4` plus a
   frame-2000 probe.

## Card breakdown

| card | scope | owned files | acceptance | deps | parallel? | h |
| --- | --- | --- | --- | --- | --- | ---: |
| S4-00 | **stage-4 boot slice**: stage record/entry, `SECTIONS[3]`/`terrain(3)`, `STAGE4` + column-416 tail | `crates/gradius-core/src/stage.rs`, `game.rs` | `new_at_stage(3)` starts `$19 = 3`; spawner consumes 111 records / 63 terrain / 48 formations / 0 commands; `STAGE4` renders 480x28; at 3328 px bank 2 and palette entries 8-15 `0F`; shared core path reached and stage advances; stages 1/2 render hashes unchanged | none | no (`game.rs`) | 8 |
| S4-01 | music `$5D`, boundary 4, backend mapping | `crates/gradius-app/src/music.rs`, `crates/gradius-sdl/src/music.rs`, `crates/gradius-wasm/src/lib.rs` | stage 4 selects `Stage4` at `0x0400`, then `Boss`/`StageClear`/`GameOver`; stage 1/2 cue lists unchanged | S4-00 | **yes** | 2 |
| S4-02 | `$C40D`/`$C424`/`$C433[3] = $C5A7` and the `$3A` hook | `game.rs` | every 4th frame; free-slot scan 9..0; `$3A != 0 -> $C680`; stage-4 `$69 == 0` sound `$0F`; first call low nibble `$F`, record 15 `FE 08 60`, kind `$15`, X `$B8`, Y `$2C`, sprite `$58`, `$03BC = 6`; `$03BC -= 2` under `$1E`; no boss flag; stage 7 RTS; other stages unchanged | S4-00 | no (`game.rs`) | 4 |
| S4-03 | shared core gate, stage-4 values, `$0F`/`$10` audit | `crates/gradius-core/src/stage.rs`, `game.rs` | spawn 7979 / death 9218 / advance 9732; `$82` is 4 frames (not 771); no `$39` gate; stage-4 palette mode 0; shared `$AFF8/$BBB7/$BBD8/$BC42` branches covered; stages 1/2 unchanged | S4-00, S4-02 | no (`game.rs`) | 4 |
| S4-04 | `--stage 4` differential/pixel/boss harness + evidence run | `crates/gradius-diff/src/main.rs`, `game.rs` stage hook | native and oracle both start stage 4; state/boss window 0 divergences; pixel playfield and HUD >= 99.9%; landmarks 7979 / 9218 / 9732; tail sample | S4-00, S4-01, S4-02, S4-03 | no (green last) | 5 |

Total: **23 h** (about three working days). Critical path is
`S4-00 -> S4-02 -> S4-03 -> S4-04` = 21 h; S4-01 is the only fully parallel
card and fills the 2 h of slack.

**First card that makes stage 4 playable end-to-end: S4-00.** It boots
`$19 = 3`, consumes `SECTIONS[3]`/`terrain(3)`, renders `STAGE4` with its
column-416 tail, and uses the existing shared core path. Stage 4's
`$9A30 = $0C`, spawn `$0D` and `$98F0 = $0E` already match stage 1, so the
stage reaches the core and advances before the exact `$82` timing. It is not
oracle-exact until S4-03 and S4-04; the `$C5A7` hazard is still absent until
S4-02.

### Parallel / conflict rules

* **Every card except S4-01 touches `game.rs`.** Do not run two of them in the
  same worktree. S4-02 and S4-03 both edit `update_objects`, `normal_frame`
  and the stage machine; sequence them. S4-00 and S4-04 both add the stage
  entry/hook, so rebase in that order.
* S4-01 is frontend/music-only and can run from the start; it depends on
  S4-00 only for the stage selection API.
* S4-04 can start after S4-00 (add `--stage` plumbing) but its green run is
  the last thing on the branch.
* If stage 2's S2-00/S2-01/S2-04, stage 3's S3-00/S3-03 or stage 5's
  S5-00/S5-02/S5-07 land first, reuse the `Stage` record, core gate,
  dispatcher and `--stage` hook; S4-00 drops to ~5 h, S4-02 to ~2 h and the
  total to ~17-18 h. If stage 4 lands first, the later stages reuse its record
  and harness but still need their own `$C433` handlers.

## Risks and exact evidence

1. **There is no `$19 == 3` branch.** Stage 4's "theme" is stage 1's `$D772`
   tile stream with stage-4 height/base (`$CF56`/`$D188`), plus music `$5D`.
   Do not add a code branch just because the stage differs visually.
2. **`$C5A7` is a hazard script, not the boss.** It spawns kind `$15`, and the
   port already routes kind `$15` to `ai_arc(3, true)`. Set its sprite `$58`
   and its `$042C`/`$03BC`/`$048C`/`$04AC`/`$044C`/`$03EC` fields; do not set
   `boss_spawned`, do not award the core's +1000, and do not let it suppress
   the shared core.
3. **The `$C5A7` tables are easy to misread.** `$C5FD` holds 16 triples, not
   pairs; `$C62D` holds 16 packed nibbles; `$C5FB` is only `[0x38, 0xB8]`.
   `$C449` increments `$69` before selecting the nibble, and bit 0 of `$69`
   selects X. The normal first spawn is record 15 (`FE 08 60`) at `X = $B8`,
   `Y = $2C`, sprite `$58`, with `$03BC` reduced from 8 to 6. Do not copy
   stage 1's extra `$69 < 0x0A` decrement.
4. **`$C40D` is not an every-frame hook.** It is reached from `$A2BA` only on
   `$1B == $82` frames or when `$3A != 0`; `$C424` sends `$3A != 0` to
   `$C680` regardless of `$19`. Stage 4's `$82` is 4 frames, so `$C5A7`
   normally emits one object, not a stream. Running it every frame would
   desynchronise the stage.
5. **The 771-frame timer is stage-1-shaped.** It models the `$8A` eruption
   hold; stage 4 has no eruption. Use `transition_frames(3)`/`$9A28` and the
   scroll wait to `$0D`, then check the 7979 spawn. Do not inherit stage 1's
   771.
6. **The tail palette is not in `Screen`.** `STAGE4` stores the dominant bank
   0 and palette. The column-416 switch (bank 2, entries 8-15 `0F`) must be a
   renderer rule. Do not regenerate or hand-edit the RLE.
7. **Do not gate stage 4 on `$39`.** No located code sets it for stage 4;
   `$9928` with `$39 == 0` stores `$90` and `$96C4` increments `$19`. The
   existing `(section != 0 || stage_clear_ready)` gate must become the shared
   normal path for section 3, not the stage-1 volcano gate.
8. **`$0F`/`$10` are partial.** Stage 4 has five of them. Their `$046C >= 5`
   death and `$5F` counter are not ported; if the diff flags them, add the
   shared death as a 3 h add-on. Do not add stage 1's `$39` increment.
9. **Music and tail values are per-stage.** Stage 4 switches to `$5D` at
   boundary 4 (`0x0400`); the boss is at `$9A30 = $0C` (3072 px). Do not reuse
   stage 1's `BOSS_SCROLL = 2820` or stage 5's column-384/`$5F` values.
10. **The data is already correct.** `SECTIONS[3]` has 111 records and 63
    terrain placements; `STAGE4` is 480x28 with the pinned hashes. Use the
    generated modules as the source of truth and do not add a stage-4 tables
    duplicate.

## Evidence commands

Use the card target directory everywhere:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stage4plan
```

Data and roots:

```sh
cargo xtask stages "$ROM" | grep '^    4'
cargo xtask sections "$ROM" | grep -E 'S3_TERRAIN'
cargo xtask stage-enemies "$ROM" | grep -E '^\$06|^\$07|^\$0F|^\$10|^\$13|^\$27|^\$29'
```

Script and handler disassembly:

```sh
cargo xtask disasm "$ROM" --from C40D --to C480 --labels   # dispatcher + $C449
cargo xtask disasm "$ROM" --from C480 --to C540 --labels   # stage-1 shape
cargo xtask disasm "$ROM" --from C5A7 --to C62D --labels   # $C5A7 + tables
cargo xtask disasm "$ROM" --from C62D --to C64D --labels   # stage-4 nibble table
cargo xtask disasm "$ROM" --from 99DC --to 9A30 --labels   # $82 countdown
```

Stage-4 oracle and RAM:

```sh
cargo xtask record-boss "$ROM" --stage 4 --frames 12000 --out /tmp/opencode/boss4.txt
# frame 2000: stage/palette check; frame 7800: the $82 threshold window
cargo xtask probe "$ROM" --frame 2000 --press 180:start \
  --poke 263:26:03 --keep-alive \
  --ram /tmp/opencode/st4.ram --nametables /tmp/opencode/st4.nt \
  --png /tmp/opencode/st4.png
cargo xtask probe "$ROM" --frame 7800 --press 180:start \
  --poke 263:26:03 --keep-alive \
  --ram /tmp/opencode/st4-boss.ram --nametables /tmp/opencode/st4-boss.nt
```

Expected `record-boss` summary: `spawn 7979 ... end 9218 ... stage 5 from
9732`. The frame-2000 probe shows `$19 = 3` and the stage-4 palette; the
frame-7800 probe covers the `$82` window from which the `$C5A7` hazard is
taken.

## Verification windows

After S4-04 adds `--stage 4`:

```sh
PACK=/tmp/opencode/stage4.pak
cargo xtask extract "$ROM" "$PACK"
flock /tmp/verify-heavy.lock cargo build --release -p gradius-diff

# state / boss window: threshold through the next stage
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage4plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 10000 --stage 4 \
  --from 7800 --to 9900 --keep-alive --native-keep-alive \
  --report --json /tmp/opencode/stage4-boss.json

# pixel window: column-416 tail, then core entry
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage4plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 10000 --stage 4 \
  --pixel-report --pixel-window 3200:3800 --png-dir /tmp/opencode/stage4-png \
  --png-every 30 --json /tmp/opencode/stage4-pixel.json
```

Pass criteria: the state/boss window reports `WITHIN TOLERANCE (0
divergences)`; pixel playfield and HUD are each `>= 99.9%`; tail samples at
and after 3328 px show `chr_bank 2` and palette entries 8-15 = `0F`; stage 4
exits to `$19 = 4` at the measured 9732 frame. The `$C5A7` probe must show
the first hazard (kind `$15`, sprite `$58`, X `$B8`, Y `$2C`) and no
`boss_spawned`. Run `scripts/verify changed` in each code worktree; do not run
the full workspace suite there.

## Definition of done / out of scope

Done when a stage-4 run starts at `$19 = 3`, consumes `SECTIONS[3]`/63 terrain
placements, renders `STAGE4` plus the column-416 bank-2/black tail, plays `$5D`
after boundary 4, runs `$C5A7` (one kind-`$15` hazard in the normal `$82`
window), reaches the shared Big Core at `$0C`/spawn `$0D`, awards +1000, and
advances at 9732 without changing stages 1 and 2.

Out of scope: stage 3's `$C680` bonus wave, stage 5's `$14`/`$A4A0`/`$0600`
cannons and `$19 == 4` chain, stage 6's `$CD9F`/beam script, stage 7's
`$9865` loop, the full `$0F`/`$10` kill gate (3 h add-on only if the diff
flags it), and any change to the
generated `STAGE4` RLE.

## Why 23 h: stage 2 is 24 h, stage 3 is 40 h, stage 5 is 50 h

Stage 4 is stage 2 minus the stage-2 `$39` quick-kill nuance, plus a slightly
larger script table:

* vs stage 2, **-1 h**. Both pay the same boot/background/music/harness cost
  (S4-00 8 h, music 2 h, harness 5 h). Stage 2's S2-01 is 5 h because of the
  `$B95C` damage-kill `$39` condition; stage 4 has no `$39` setter, so its
  core card is 4 h. Stage 4's `$C5A7` is 4 h like `$C540`, with a 16-entry
  triple table and a packed-nibble table but no five-byte commands. 24 - 1 =
  23.
* vs stage 3, **-17 h**. Stage 3 spends ~14 h on `$96`/`$C900` and its
  nametable overlay, and ~3 h on the `$C680`/`$39`/`$3A` bonus wave. Stage 4
  has no new AI handler, no `$96`, no overlay and no `$C680` body. 40 - 17 =
  23.
* vs stage 5, **-27 h**. Stage 5 spends ~10 h on `$14`/`$CA58`, ~8 h on
  `$A4A0`/`$0600` cannons and ~6 h on the `$19 == 4` chain, plus more data.
  Stage 4 has none of those; its only script is one `$C5A7` handler. 50 - 27 =
  23.

The shared `Stage` record, core gate and `--stage` harness are common
infrastructure: whichever of stages 2/3/4/5 lands first pays for them once.
If stage 2 or stage 5 is already merged, stage 4's incremental cost is about
17-18 h; if stage 4 lands first, the later stages reuse its record and
harness but still own their `$C433` handlers.
