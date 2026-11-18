# Stages 6 and 7 implementation plan (the endgame)

Executable plan for `$19 = 5` (stage 6) and `$19 = 6` (stage 7), including the
stage-7 ending, the loop counter and the new-game reset. It starts from the
table-driven runtime that `stage2_plan.md`/`stage3_plan.md`/`stage4_plan.md`/
`stage5_plan.md` introduce: a `Stage` record, a per-stage core gate, `--stage` in the
differential harness, and the stage-1-shaped `$81 -> $82 -> $83 -> $84 -> $85`
boss path. Stages 6 and 7 are the only stages with **no Big Core**: `$99B3`
sends `$19 >= 5` straight to `$86`, and the stage ends on scroll at `$98F0`.

The new mechanics are small; the new *subsystem* is the ending. Stage 6 adds
the `$C6D8` collision-only beam wall and the `$CD9F`/`$CE2B` wall erase. Stage 7
adds no objects at all: the giant brain is background tiles, and the stage's
only content is the normal `SECTIONS[6]`/`terrain(6)` stream plus the `$1E`
scroll-freezing walls and `$20`-`$25` background pieces that already exist.
`$9865` then wraps to stage 1 with `$28` incremented, runs `$87`-`$8D`
(~1230 frames), awards +10000 and restarts stage 1 with score, lives and loop
preserved.

Everything below was checked on `Gradius (E).nes` on 2026-11-09 against
`ending.md`, `bosses.md`, `stages.md`, `stage_enemies.md`, the generated
`stage_bgs.rs`, and the recorded traces `ending6.txt`/`ending7.txt`.

## Stage-6/7 facts to build against

From `cargo xtask stages` and `cargo xtask sections` (checked 2026-11-09):

| field | stage 6 (`$19 = 5`) | stage 7 (`$19 = 6`) |
| --- | --- | --- |
| spawn root `$A7CA[$19]` | `$A822`, 7 columns | `$A830`, 7 columns |
| `SECTIONS[5]`/`[6]` | 104 records, 0 commands | 127 records, 0 commands |
| `$A65C` terrain | 75 (`terrain(5)`) | 89 (`terrain(6)`) |
| background | `stage_bgs::STAGE6`, 416x28, bank 2 | `stage_bgs::STAGE7`, 416x28, bank 3 |
| tiles / attrs / palette hashes | `D852B59DC2CE66D3` / `2B9A4EB2607B8BE9` / `E1DE0E2046002CA8` | `8D2A4FF23151EAB7` / `39721C58E9B24C98` / `959453030221F37D` |
| tail | none (3328 px wide, end at 3072 px, no column-keyed change) | none (same) |
| music `$8344` / boundary `$8354` / mode `$834B` | `$61` / 4 / 1 | `$63` / 2 / 3 |
| `$98F0` end | `$0C` | `$0D` |
| `$9A30` threshold | `$0B` | `$0C` |
| start row `$9BC6` | `$0F` | `$14` |
| script `$C433[$19]` | `$C6D8` (the beam spawner) | `$C423` (bare `RTS`) |
| boss | none | none |
| stage end | `$86`, wall erase, `INC $19` | `$86` -> `$9865` ending |

Kind inventory (all already have a `game.rs` arm except where noted):

* Stage 6: `$04` 15, `$05` 4, `$08` 10, `$0F` 2, `$11` 9, `$12` 9, `$1A` 53,
  `$27` 2. `$1A` is `ai_spin`, `$0F` is the partial `ai_turret_fire` (its
  sprite `$63` stage-6 branch at `$AF48` is **already ported** as
  `(15, 5) => 0x63`), the rest are ported.
* Stage 7: `$04` 24, `$05` 4, `$06` 6, `$07` 18, `$08` 1, `$0B` 9, `$10` 2,
  `$11` 14, `$12` 17, `$13` 18, `$1E` 2, `$20`-`$25` 2 each. `$1E` is
  `ai_wall` (`$B563`, scroll freeze), `$20`-`$25` are `ai_bg_piece` (`$AF0A`),
  the rest are ported.

## Measured landmarks

`cargo xtask record-ending "$ROM" --stage 6 --frames 8000` (`ending6.txt`),
stage 6 starts at frame 265:

| frame | `$1B` | event |
| ---: | --- | --- |
| 5929 | `$81` | `$3F = $0B`; `$9A01` sets `$82` and `$4D = $9A28[$17] = 4` |
| 5930 | `$82` | 1024-frame window; `$66 = 0` |
| 5933 | | first beam: slot 9, `(148, 69)`; one spawn attempt every 4th frame |
| 5933-6953 | | 256 attempts; ten collision slots, lanes 66 (down) and 132 (up) |
| 6954 | `$83` | timer zero |
| 6955 | `$86` | `$99B3` requests `$AC` (stage 6 only) |
| 6956-6999 | | `$CD9F`: two `$CDAD` erases/frame, `$66` 0 -> 88 |
| 7467 | `$90` | `$3F = $0C` |
| 7468 | `$80` | `$19` 5 -> 6, `$66 = 0` |

`cargo xtask record-ending "$ROM" --stage 7 --frames 8500` (`ending7.txt`):

| frame | `$1B` | event |
| ---: | --- | --- |
| 6563 | `$81` | `$3F = $0C` |
| 6564 | `$82` | special timer `$4D = 1`, `$4C = $CA` = 458 frames |
| 7022 | `$83` | timer zero |
| 7023 | `$86` | no `$AC` |
| 7024 | `$87` | `$3F = $0D` -> `$9865`: loop 0 -> 1, `$26 = 0`, `$3F = 0` |
| 7025 | `$88` | PPU/music setup; `$19` already 0 |
| 7027 | `$89` | PPU/attribute updates |
| 7028 | `$8A` | palette/starfield update |
| 7029-7051 | `$8B` | ending ship animation (23 frames) |
| 7052-8253 | `$8C` | starfield + `CONGRATULATIONS!` (1202 frames) |
| 7998 | `$8C` | score 830 -> 10830 (**+10000**) and lives 3 -> 4 |
| 8254 | `$8D` | `$1B = 0`, `JMP $9B38`; no score add here |
| 8255-8281 | `$01`-`$04` | stage-1 restart sequence |
| 8282 | `$80` | normal stage-1 play, loop = 1, score = 10830 |

`ending.md` says the +10000 is added at `$8D`/8254. The trace and the
disassembly disagree: `$98D8` (`$8D`) only clears `$1B` and jumps to `$9B38`;
the +10000 is `$8444` called from `$CE8E`, which the ending ship handler
`$BB09` calls during `$8C` at 7998. The plan follows the trace (see risk 6).

## Current code anchors

| gap | current code |
| --- | --- |
| stage entry | `Game::new` hardcodes `section = 0`; `advance_section` clamps at 7 |
| background | `game.rs::render` uses `stage1_bg::STAGE1` |
| boss gate | `normal_frame` uses `0x0C`, spawn wait `0x0D`, `boss_timer = 771` |
| stage end | `boss_clear == 0 && (section != 0 \|\| stage_clear_ready)`; `0x0E` |
| no-core path | absent; `$99B3`/`$86` are not modelled |
| beams | absent; no collision-only slot array, no shot-vs-beam test |
| wall erase | absent; `$CE2B` is not extracted |
| stage script | `$C40D`/`$C424`/`$C433` absent (shared with stages 2-5) |
| ending | absent; `Phase::Cleared(240)` -> `State::GameOver` is the only terminal |
| loop | no `$28`/`$1A` field |
| score extend | absent: `add_score` never increments lives; `$2A` is not modelled |
| music | `MusicCue` has only `Stage1`/`Stage1B`; `Music::Stage6`/`Stage7` exist |
| harness | `gradius-diff` has no `--stage` and no phase/loop/beam fields |
| oracle tooling | `xtask record-ending --stage 6/7` already exists |

`$AF48` is already handled; do not add a second stage-6 sprite branch. The
ending ship kind `$28` (`$BB09`) is the only missing AI handler, and no stage
stream spawns it.

## Implementation steps

### Stage 6

1. **Stage record and no-core end path.** Add the `$19 = 5` row (background
   `STAGE6`, music `$61`, boundary 4, mode 1, boss `$0B`, end `$0C`, start row
   `$0F`) and `Game::new_at_stage(5)`. At `$3F = 0x0B`, run `$81` then `$82`;
   at the end of `$82` take `$83 -> $86` **without** `$84`/`$85` and without
   creating kind `$98`. The `$86` window is inside normal play: the spawner,
   object AI, collisions and scroll keep running while `$3F` climbs to `$0C`.
   Do not model `$86` as `Phase::Transition` (that would freeze the frame and
   the wall erase/scroll would stop). Keep the `$1B`-shaped sub-state in a
   dedicated `endgame` field, not in `Phase`.
2. **The `$82` window is `$9A28[$17]`, not `$9A28[$19]`.** `$9A01` reads
   `$9A28,X` with `X = $17` (rank) and special-cases `$19 == 6` to
   `$4D = 1, $4C = $CA` (458 frames). Stage 6's measured 1024 frames is
   `$9A28[rank] = 4`. Add `phase82_frames(rank, stage)` and make the stage-6/7
   end path use the live `boss_rank()`; leave the existing
   `transition_frames(section)` alone unless the stage-2/3/4/5 refactor replaces
   it. Store no per-stage window in `Stage`.
3. **`$C6D8` beams** (`ending.md`, `bosses.md`). `$C40D` runs only on
   `$02 & 3 == 0`; scan collision slots 9 down to 0 for `$0136 == 0`; write
   `$0376 = $98` (X 152), `$0336 = $C74A[$02 & 4 ? 0 : 1]` (132 or 66),
   `$0316 = 1`, `$0176 = 1`, `$0476 = lane`, `$0136 = $8D`. Move X by -4 per
   frame and Y by +4 (lane 66) or -4 (lane 132); the spawn frame's first
   sample is `(148, 69)`, so the movement runs in the spawn frame. Cap at ten
   live beams. Player hit uses hit-box type 1 (`22 x 18`, the existing
   `PLAYER_BOX_*` row) and is lethal unless shield `$46` absorbs it. A player
   shot within 16 px in X and `$A3` in Y destroys the beam, awards +10 and
   plays `$09`. Beams have **no `$012C` and are invisible**; they must not be
   in the `$030C` object table and must not be drawn.
4. **`$CD9F`/`$CDAD` wall opening.** While `$86` on stage 6, call the erase
   twice per frame from the 88-byte `$CE2B` table until `$66 >= 0x58`, adding
   1 to `$66` per erase. The address is
   `$2400 + ((tile & $F0) << 1) + ($F0 | (tile & $0F))` with the carry into
   the high byte; the decoded entries span nametable rows 7-22 and columns
   21-31 (84 unique positions, 4 duplicates). Extract the 88 bytes into a
   generated `stage6_wall.rs` with a pinned hash; do not edit `STAGE6`'s RLE.
   Apply the erases as a world-fixed tile overlay computed from the live
   scroll when each VRAM record is issued, so the hole opens at the wall the
   scroll is about to show, and pin the 44-frame cadence (6956-6999).
   `$66` is shared scratch in the ROM (the trace shows 172 during stage-5
   play); model the erase index as its own field, cleared when `$82` starts,
   and never reuse a port `$66` scratch byte.
5. **`$AF48` and the other stage-6 exceptions.** `(15,5) => 0x63` already
   exists; add only the test. Audit `$AFF8`/`$BBB7`/`$BBD8`/`$BC42` against the
   stage-2/3/4/5 refactor; do not add a new branch unless a diff shows one.
6. **Music.** Stage 6 plays the intro `$93` until `$3F >= 4` (`0x0400`), then
   `$61`; `$AC` is requested at `$99B3` (after `$82`, before the scroll end);
   the director must not play `Boss`/`StageClear` from the core-scroll inference
   because there is no core. Stage 1's cue list stays byte-identical.

### Stage 7

7. **Stage record and content.** Add the `$19 = 6` row (background `STAGE7`,
   music `$63`, boundary 2, mode 3, boss `$0C`, end `$0D`, start row `$14`)
   and `Game::new_at_stage(6)`. Use `SECTIONS[6]`/`terrain(6)` (127 records,
   89 placements). `$C433[6] = $C423` is a bare `RTS`: no scripted hazard, no
   command path. At `$3F = 0x0C`, run `$81`/`$82` (458 frames), then `$83 ->
   $86`; at `$3F = 0x0D`, run `$9865`. The brain, cables and organic walls are
   `STAGE7` tiles; do not create a brain object. `$1E` walls and `$20`-`$25`
   pieces use the existing `ai_wall`/`ai_bg_piece`; only the stream wiring and
   the two stage-7 display states need checking.
8. **`$9865` and `$9B38`.** Add `loop_count` (`$28` = `$1A`). On `$9865`:
   `loop_count += 1`, `scroll = 0`, `stage_byte = 0`, `$22 = (power != 0)`,
   preserve score and lives. `$9B38` clears zero page and the object/effect
   pages, restores `$35`, `$42` (power), `$19 = 0`, `$1A = loop_count`, and
   re-places the ship for stage 0. In the port this means: reset objects,
   shots, enemy bullets, beams, explosions, options, power meter, speed,
   `stage_clear_ready`, `volcano_kills`, boss/endgame fields and the spawner;
   keep `score`, `score_bcd`, `lives`, `high_score` and `loop_count`. Do not
   call `new_run()` (it zeroes the score/lives).
9. **Ending state machine.** Add ending states with the measured durations:
   `$87` reset (1), `$88` PPU/music (2), `$89` PPU/attributes (1), `$8A`
   palette/starfield (1), `$8B` ship (23), `$8C` starfield (1202), `$8D`
   reset+jump (1), then the existing `$01`-`$04` stage-entry sequence (27
   frames) and `$80` at 8282. Keep `State::Playing` throughout (the trace is
   `state=Playing`), so do not reuse `Phase::Cleared`/`GameOver`. A dedicated
   `Phase::Ending(EndingStep)` is acceptable because normal play really is
   suspended, unlike `$86`.
10. **Ending ship `$28`/`$BB09`/`$CE8E` and the bonus.** Port the ending ship
    as kind `$28` at X `$74`, Y `$88`, sprite `$9E` (`$987F` clears slots 9/8
    and sets `$0100 = 3`). `$CE8E` moves it every 8 frames from the `$CF27`
    table indexed by `$1A` (the loop counter), so this card depends on
    `loop_count`. `$CECC`/`$8444` adds **10000** once during `$8C` (measured
    at 7998); reproduce the score jump at the same phase offset.
11. **Score extend (`$2A`).** The new game path sets lives `$20 = 3` and the
    next-extend threshold `$2A = 1` (10,000 points, BCD high byte). `$84D8`
    compares `$07E0-$07E2` with `$2A` after every score add, increments
    `$20`, adds the next threshold and plays `$36`. Without this the ending's
    +10000 does not produce the measured 3 -> 4 life. Add the threshold and
    the check to `add_score` (shared, stage-1 code); measure the second
    threshold (the `$84E2` path) and pin it in a test.
12. **Ending display.** During `$8C` draw the starfield and
    `CONGRATULATIONS!` with `font::draw_text_centered` (the port already has
    `starfield()`); the text window is measured from oracle PNGs (the note's
    sample at 8100 shows it). `$8B` shows the ending ship over the same
    starfield. Do not leave the stage-7 playfield on screen: `$9BE7` uploads a
    fresh nametable/attributes, so the ending frame is not `STAGE7`.
13. **Loop and new game.** After `$8D` restart stage 1 with `loop_count = 1`,
    section 0, scroll 0, score/lives preserved. A later new game from the
    title must reset `loop_count = 0`, stage 0, score 0, lives 3 (extend
    `new_run()`), while the loop reset deliberately does not.
14. **Music `$63`.** Stage 7 plays the intro `$93` until `$3F >= 2` (512 px),
    then `$63`; no boss cue and no `$AC`. The ending's music is whatever
    `$9BE7` requests (not a stage theme); match the trace/audio capture and
    do not invent a track. Keep stage 1-5 cue lists unchanged.

## Card breakdown

Stage 6 (`$19 = 5`):

| card | scope | owned files | acceptance | deps | h |
| --- | --- | --- | --- | --- | ---: |
| S6-00 | **stage-6 boot slice**: Stage row, entry, `SECTIONS[5]`/`terrain(5)`, `STAGE6`, no-core `$99B3`/`$86` end path, `$82` rank timer | `crates/gradius-core/src/stage.rs`, `game.rs` | `new_at_stage(5)` starts `$19=5`; 104 records / 75 terrain; `STAGE6` renders 416x28 with bank 2; `$3F=0x0B` enters `$81/$82`; no kind `$98` object ever appears; `$3F=0x0C` advances to `$19=6` at 7468; stages 1-5 hashes unchanged | none (shared `Stage` from stages 2-5 if landed; +4 h if not) | 8 |
| S6-01 | `$C6D8` collision-only beams | `game.rs`, new `crates/gradius-core/src/stage6_beams.rs` | every 4th frame, free slot 9..0; X 152 -> -4/frame; lanes 66 down / 132 up by 4; cap 10; 22x18 lethal hit unless shield; shot destroys +10 and `$09`; invisible; beam list equals `ending6.txt` 5933-6990 | S6-00 | 10 |
| S6-02 | `$CD9F`/`$CDAD` wall opening and `$CE2B` | `game.rs`, new `crates/gradius-core/src/stage6_wall.rs` | 88 bytes pinned (rows 7-22, cols 21-31, 84 unique); 2 erases/frame for 44 frames; erase index 0 -> 88 while `$86`; world-fixed overlay; PNG at 6970/6990/7200/7450 shows the diamond opening; `STAGE6` RLE untouched | S6-00 | 6 |
| S6-03 | music `$61`, boundary 4, `$AC` at `$99B3` | `crates/gradius-app/src/music.rs`, `crates/gradius-sdl/src/music.rs`, `crates/gradius-wasm/src/lib.rs` | stage 6 cue table intro/`Stage6` at `0x0400`/clear at `$99B3`, no Boss; stage 1 cues unchanged | S6-00 (accessor) | 2 |
| S6-04 | `--stage 6`, ending report and evidence run | `crates/gradius-diff/src/main.rs`, new `crates/gradius-diff/src/ending.rs`, `game.rs` accessors | native and oracle both start stage 6; ending/state window 0 divergences; beams and `$66` match the trace; pixel >= 99.9%; wall PNG window | S6-00, S6-01, S6-02 | 5 |

Total: **31 h**. Critical path `S6-00 -> S6-01 -> S6-02 -> S6-04` = 29 h;
S6-03 is the only parallel card.

**First card that makes stage 6 playable end-to-end: S6-00.** It boots
`$19 = 5`, consumes `SECTIONS[5]`/`terrain(5)`, renders `STAGE6`, skips the
core, and advances at `$3F = 0x0C`. The stage is playable but easy (no beams)
and visually incomplete (no wall opening) until S6-01/S6-02.

Stage 7 (`$19 = 6`) and the ending:

| card | scope | owned files | acceptance | deps | h |
| --- | --- | --- | --- | --- | ---: |
| S7-00 | **stage-7 boot slice**: Stage row, entry, `SECTIONS[6]`/`terrain(6)`, `STAGE7`, `$C423` RTS, 458-frame `$82`, `$3F=0x0D` hook | `stage.rs`, `game.rs` | `new_at_stage(6)` starts `$19=6`; 127 records / 89 terrain; `STAGE7` renders 416x28 bank 3; no script hazard and no core; ends at `$3F=0x0D`; stage 1-6 unchanged | S6-00 | 7 |
| S7-01 | `$9865`/`$9B38` ending state machine, loop counter, new-game reset | `stage.rs`, `game.rs` | `$87`-`$8D` durations 1/2/1/1/23/1202/1; `$19=0`, loop=1 at 7024; score/lives preserved; objects/power/options cleared; stage 1 normal at 8282; new game resets loop 0 and score/lives | S7-00 | 10 |
| S7-02 | ending ship `$28`/`$BB09`/`$CE8E`, +10000 (`$8444`), score extend (`$2A`/`$84D8`) | `game.rs`, `stage.rs` | kind `$28` at `(0x74, 0x88)`, `$CF27[loop]` path, `$8B` window 23 frames; the handler keeps running during `$8C` and the score goes 830 -> 10830 at 7998; lives 3 -> 4 via the 10,000 extend; threshold survives the loop | S7-01 | 8 |
| S7-03 | ending display: `$8C` starfield, `CONGRATULATIONS!`, `$8B` ship | `game.rs` (render) | `$8C` is 1202 frames; text visible at 8100 and absent before 7052; `State::Playing` throughout; no `STAGE7` playfield bleed | S7-01 | 5 |
| S7-04 | music `$63`, boundary 2 | `gradius-app/src/music.rs`, `gradius-sdl/src/music.rs`, `gradius-wasm/src/lib.rs` | stage 7 intro -> `Stage7` at `0x0200`; no Boss/StageClear; stage 1 unchanged | S7-00 (accessor) | 2 |
| S7-05 | `--stage 7`, ending/loop evidence run | `crates/gradius-diff/src/main.rs`, `crates/gradius-diff/src/ending.rs`, `game.rs` accessors | state/ending window 0 divergences; phase/loop/score at 7024/7998/8254/8282; loop-2 stage-1 window stays in tolerance; pixel >= 99.9% | S7-00, S7-01, S7-02, S7-03 | 5 |

Total: **37 h**. Critical path `S7-00 -> S7-01 -> S7-02 -> S7-03 -> S7-05`
= 35 h; S7-04 is the only parallel card.

**First card that makes stage 7 playable end-to-end: S7-00.** It boots
`$19 = 6`, plays the 127-record stream, renders `STAGE7`, and reaches
`$3F = 0x0D`. Until S7-01 the end falls back to the existing clear/game-over
path, so the stage is playable but the ending/loop is missing.

### Parallel / conflict rules

* **Every card except S6-03 and S7-04 touches `game.rs`.** Run them in one
  worktree, in the order above; do not run two concurrently.
* S6-01 and S6-02 both edit `normal_frame` and the collision/render paths.
  Sequence S6-01 before S6-02; both must leave the stage-1 state hash
  unchanged.
* S6-04/S7-05 edit `gradius-diff` and add `Game` accessors; they can start
  after S6-00/S7-00 (plumbing), but their green run is last.
* S6-03 and S7-04 are frontend/music-only and depend on the stage accessor
  added by S6-00/S7-00; they are the only cards that can run beside a
  `game.rs` card.
* Reuse the `Stage` record, core gate and `--stage` hook from the
  stage-2/3/4/5 plans if they land first. If stage 6/7 lands first, S6-00 and
  S6-04 pay about +4 h and +3 h for the record and harness; S7-05 reuses it.
* Do not refactor shared collision or AI code for either stage. The beam
  system is a separate collision-only array, not a new `update_objects` arm;
  the stage-6 wall is a tile overlay, not a new object.

## Estimate vs the earlier stages and the full-game total

| stage | plan | h | why |
| --- | --- | --- | ---: |
| 1 | done, tail only | 14 | boss shot-phase drift 8, title ship offset 1, scanline order 2, score extend 3 |
| 2 | `stage2_plan.md` | 24 | no commands, no new handler, `$C540` + core |
| 3 | `stage3_plan.md` | 40 | `$96`/`$C900` + nametable overlay + `$C680` |
| 4 | `stage4_plan.md` | 23 | no code branch, `$C5A7` only, reuses `$D772` |
| 5 | `stage5_plan.md` | 50 | `$14`/`$CA58` + `$A4A0` cannons + `$19 == 4` chain |
| 6 | this plan | 31 | boot + beams (10) + wall (6) + music + harness; no core |
| 7 | this plan | 37 | boot + ending state machine (10) + ship/bonus/extend (8) + display (5) + harness |

Stages 2-7 = **205 h**; plus the stage-1 tail = **219 h** total
(~27 working days). About 10-15 h of that is shared infrastructure counted in
more than one plan (the `Stage` record, the core gate and `--stage`), so the
de-duplicated critical-path total is about **209 h**.

How stage 6/7 compare:

* **Stage 6 (31 h) is +7 h over stage 2 (24 h), +8 h over stage 4 (23 h)**
  and -9 h under stage 3 (40 h). It has no boss, but it pays for a new hazard
  representation (collision-only boxes, shot-vs-beam, invisible so pixel diff
  cannot check it) and for the wall-erase presentation. If stages 2-5 are
  already merged, its incremental cost is about 26 h.
* **Stage 7 (37 h) is -3 h under stage 3, +6 h over stage 6 and +14 h over
  stage 4.** It has no new hazard handler and no boss, but the ending is a new
  subsystem: a seven-state machine, the ending ship handler, +10000, the
  missing score extend, a 1202-frame display and the loop/new-game reset. The
  ending alone is ~23 h of the 37.
* Stage 5 (50 h) remains the most expensive because of `$14`/`$CA58` and the
  `$A4A0` cannon system.

## Risks and exact evidence

1. **`$82` is a rank timer.** `$9A01` is `LDA $9A28,X` with `X = $17`, and
   stage 7 overrides it with `$4D = 1, $4C = $CA`. Stage 6's 1024 frames is
   `$9A28[rank] = 4`. The stage-2/3/4/5 plans' `$9A28[$19]` wording is wrong;
   do not copy it into `Stage`. Use `CHECKPOINT_TIMER[rank]` plus the stage-7
   special case and verify the native `boss_rank()` matches the oracle's
   `$17` on the reference run.
2. **`$66` is shared scratch.** `ending6.txt` shows `$66 = 172` during
   stage-6 play before `$82` and `$66 = 0` at `$82`. Do not reuse a port `$66`
   byte; keep the wall erase index separate and clear it when `$82` starts.
3. **Beams are invisible.** `$C6D8` never writes `$012C`, so there is no OAM
   sprite and the pixel diff cannot see the hazard. Compare the beam dump
   (`record-ending` line / `gradius-diff --ending-report`), not pixels. The
   first sample is `(148, 69)`: the spawn writes 152/66, then the spawn frame
   moves it to 148/69; pin the trace rather than the prose.
4. **`$C433[5]` is the beam spawner, not an enemy script.** `$C433[6]` is a
   bare `RTS`. Do not route them through an object-spawning script; do not
   let stage 7's `$C40D` call spawn anything.
5. **The wall is a tilemap patch.** `Screen` is static and the generated RLE
   must not be edited. Decode `$CE2B` to 88 world-fixed `(row, column)`
   positions (rows 7-22, columns 21-31) and apply them after `draw_screen`.
   The 4 duplicate entries are harmless; pin the bytes and a hash.
6. **The +10000 is not at `$8D`.** The trace has 830 -> 10830 and lives
   3 -> 4 at frame 7998 (`phase=$8C`); `$98D8` only clears `$1B`. The add is
   `$8444` (10,000) from `$CE8E`, called by the ending ship handler `$BB09`.
   Implement the trace, and if the exact call frame drifts, pin it from a
   `record-ending` rerun rather than the note.
7. **The score extend is missing, and the ending exposes it.** New game sets
   `$20 = 3` and `$2A = 1` (`$82CD`); `$84D8` compares the score with `$2A`
   after every add, does `INC $20` and raises the threshold. With no extend,
   the ending's +10000 gives the wrong lives. Add it to `add_score` and test
   the first threshold with a 10,000-point award.
8. **The ending is `Playing`, not `GameOver`.** Do not reuse
   `Phase::Cleared(240)`; the ending is 1202 frames of `$8C` plus the reset
   and the 27-frame stage-1 entry. Score/lives/loop survive; power, options,
   objects and effects do not.
9. **Do not reuse `new_run()` for the loop.** It zeroes score and lives. Add
   a separate `loop_reset()` for `$9B38` and keep `new_run()` for the title,
   where `loop_count` must go back to 0.
10. **The ending ship needs the loop counter.** `$CE8E` indexes `$CF27` with
    `$1A` (copied from `$28`), so S7-02 cannot be finished before S7-01.
11. **`$AF48` is already ported.** `(15, 5) => 0x63` exists in
    `ai_turret_fire`; only add the regression test. Do not add a second
    stage-6 sprite branch.
12. **The brain is background.** `STAGE7` carries the brain, cables and
    organic walls; there is no `$98` and no script. `$1E`/`$B563` freezes the
    scroll and `$20`-`$25`/`$AF0A` are background pieces; both handlers are
    already ported. A separate brain object would be a port invention.
13. **No scroll-keyed tail for stages 6/7.** The screens are 416 tiles wide
    (3328 px) and the stages end at 3072 px. The extra `from column 0`
    display changes in `stage_bgs.rs` look like stage-init/checkpoint
    re-uploads (1 and 2 checkpoint replays are recorded); render the dominant
    `Screen` for the whole stage and let the pixel window prove it. If a
    sample drifts, measure the trigger instead of hand-patching tiles.
14. **`$1E` wall timing is observable.** Stage 7 has two `$1E` objects; a
    wrong `scroll_stop` changes every later spawn frame. Pin the stage-7
    scroll trace before touching `ai_wall`.

## Evidence commands

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stage67plan
```

Data and oracle traces (the tooling already exists):

```sh
cargo xtask stages "$ROM" | grep -E '^    [67]'
cargo xtask sections "$ROM" | grep -E 'S5_TERRAIN|S6_TERRAIN'
cargo xtask stage-enemies "$ROM" | grep -E '^\$1A|^\$1E|^\$2[0-5]'
cargo xtask record-ending "$ROM" --stage 6 --frames 8000 --out /tmp/opencode/ending6.txt
cargo xtask record-ending "$ROM" --stage 7 --frames 8500 --out /tmp/opencode/ending7.txt
```

Disassembly of every routine this plan changes:

```sh
cargo xtask disasm "$ROM" --from 99F0 --to 9A50 --labels   # $9A01, $9A28, $A2BA hook
cargo xtask disasm "$ROM" --from C6D8 --to C750 --labels   # $C6D8/$C74A beams
cargo xtask disasm "$ROM" --from CD9F --to CE2B --labels   # $CD9F/$CDAD
cargo xtask disasm "$ROM" --from 9865 --to 98E1 --labels   # $9865, $987F, $98D0, $98D8
cargo xtask disasm "$ROM" --from 9B38 --to 9C30 --labels   # $9B38 reset
cargo xtask disasm "$ROM" --from BB09 --to BB80 --labels   # ending ship
cargo xtask disasm "$ROM" --from CE8E --to CF30 --labels   # ship path, $CF27, $8444
cargo xtask disasm "$ROM" --from 8444 --to 8520 --labels   # score add, $84D8 extend
```

Wall and ending screenshots:

```sh
# poke $26 every frame until stage init copies it into $19 (like record-ending)
ST6=$(for f in $(seq 170 320); do printf -- '--poke %d:26:5 ' "$f"; done)
ST7=$(for f in $(seq 170 320); do printf -- '--poke %d:26:6 ' "$f"; done)
cargo xtask probe "$ROM" --frame 7450 --press 180:start --keep-alive $ST6 \
  --nametables /tmp/opencode/st6.nt --png /tmp/opencode/st6-7450.png
cargo xtask probe "$ROM" --frame 8100 --press 180:start --keep-alive $ST7 \
  --png /tmp/opencode/st7-8100.png
```

## Verification windows

After S6-04/S7-05 add `--stage` and `--ending-report`:

```sh
PACK=/tmp/opencode/stage67.pak
cargo xtask extract "$ROM" "$PACK"
flock /tmp/verify-heavy.lock cargo build --release -p gradius-diff

# stage 6 state: threshold -> next stage
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage67plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 7500 --stage 6 \
  --from 5850 --to 7480 --keep-alive --native-keep-alive \
  --report --json /tmp/opencode/stage6-state.json

# stage 6 beams + wall index (invisible hazard: trace, not pixels)
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage67plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 7500 --stage 6 \
  --from 5920 --to 7000 --ending-report --json /tmp/opencode/stage6-beams.json

# stage 6 wall opening pixels
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage67plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 7500 --stage 6 \
  --pixel-report --pixel-window 6930:7480 --png-dir /tmp/opencode/stage6-png \
  --png-every 20 --json /tmp/opencode/stage6-pixel.json

# stage 7 terrain/brain and end
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage67plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 7060 --stage 7 \
  --from 6500 --to 7040 --keep-alive --native-keep-alive \
  --pixel-report --pixel-window 6000:7040 --json /tmp/opencode/stage7.json

# ending: phase/loop/score/lives + display
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage67plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 8300 --stage 7 \
  --from 7015 --to 8290 --keep-alive --native-keep-alive --ending-report \
  --report --json /tmp/opencode/ending.json

flock /tmp/verify-heavy.lock /tmp/opencode/target-stage67plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 8300 --stage 7 \
  --pixel-report --pixel-window 7020:8280 --png-dir /tmp/opencode/ending-png \
  --png-every 30 --json /tmp/opencode/ending-pixel.json
```

Pass criteria: both state windows report `WITHIN TOLERANCE (0 divergences)`;
the beam/`$66` report matches `ending6.txt` frame by frame (5933-6990); the
wall PNGs show the diamond opening grow and vanish into `STAGE7`; playfield
and HUD pixels are each `>= 99.9%`; the ending report has `phase=$87` at
7024, score 10830 and lives 4 at 7998, `phase=$8D` at 8254 and `phase=$80`
at 8282 with `loop=1`; the 8100 PNG shows `CONGRATULATIONS!`; a 600-frame
stage-1 window after 8282 stays in tolerance with `loop=1`.

## Definition of done / out of scope

Done when a stage-6 run consumes `SECTIONS[5]`/`terrain(5)`, renders
`STAGE6`, runs the `$82` beams and the `$CE2B` wall erase, skips the core,
and advances at `$3F = 0x0C`; a stage-7 run consumes `SECTIONS[6]`/
`terrain(6)`, renders `STAGE7`, runs the two `$1E` walls and the background
pieces, skips the core/script, and at `$3F = 0x0D` runs `$9865` into the
`$87`-`$8D` ending (+10000, `CONGRATULATIONS!`), then restarts stage 1 with
`loop = 1` and the score/lives preserved, and a new game after that resets
the loop; stages 1-5 stay bit-identical.

Out of scope: the `$39`/`$992E` double-increment route (documented in
`ending.md`; the normal route is `$39 = 0`); a loop-difficulty table (the
trace shows rank rebuilt from the live power state, not from `$28`); a
stage-6/7 solid collision map (ship-vs-map collision is always on against the
packed `COLLISION_MAP`, whose sections 1-6 share the same loop; `$A65C`
terrain stays spawn-only); redrawing
`CONGRATULATIONS!` from extracted tiles if the font text matches the oracle
pixel window; and the stage-1 boss shot-phase drift itself (costed in the
tail, not implemented here).
