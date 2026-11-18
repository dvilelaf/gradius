# Stages 6-7 and the ending: measured boot timeline

Minimal measured data to wire stage 6, stage 7 and the ending without
re-investigating. Analysis only: no core, script or pack change. Everything
below was re-measured on 2026-11-10 on `Gradius (E).nes` with the oracle
tooling in this worktree; it agrees with `ending.md`/`stage67_plan.md` except
where noted.

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-s67 CARGO_BUILD_JOBS=2
xtask record-ending "$ROM" --stage 6 --frames 8000 --out /tmp/opencode/s67-ending6.txt
xtask record-ending "$ROM" --stage 7 --frames 8500 --out /tmp/opencode/s67-ending7.txt
gradius-diff --rom "$ROM" --pack /tmp/opencode/s67.pak --stage 6 \
  --frames 7600 --from 260 --to 1265 --keep-alive --native-keep-alive \
  --oracle-out /tmp/opencode/s67-oracle6.txt        # same with --stage 7
xtask probe "$ROM" --frame N --press 180:start --keep-alive \
  $(for f in $(seq 170 320); do printf -- '--poke %d:26:5 ' $f; done) --png ...
```

`probe` with `--poke f:26:5` / `:6` is the only way to *picture* a later stage;
`record-ending` is the deterministic timeline (pokes `$26`, holds A, pins the
death bytes).

## Stage 6 (`$19 = 5`)

| frame | event |
| ---: | --- |
| 263 | `$00 = 4`, stage entry; `$19` already 5 |
| 265 | `$1B = $01`, scroll 0, ship `(80, 96)`, lives 3, score 0 |
| 267-269 | `$1B` `$02`/`$03`/`$04` (one frame each) |
| 292 | `$1B = $80`, normal play; scroll starts |
| 294 | first object: slot 9, kind `$85`, `(240, 42)` |
| 5929 | `$1B = $81`; `$3F = $0B` = `$9A30[5]`; scroll freezes at `0B:80` |
| 5930 | `$1B = $82`; `$66` cleared 172 -> 0; timer `$9A28[$17] = 4` (1024 f) |
| 5933-6953 | one `$C6D8` beam attempt every 4th frame, 10 live boxes, X 152 -> -4/f |
| 6954 | `$1B = $83` (timer zero), 7 beams still live |
| 6955 | `$1B = $86`, `$99B3` requests `$AC` (stage-clear cue) |
| 6956-6999 | `$CD9F`: 2 tile erases/frame, `$66` 0 -> 88 |
| 7467 | `$3F = $0C`, `$1B = $90` |
| 7468 | `$19 = 6`, scroll `$3F = 0`, `$1B = $80` (stage 7 starts) |

Special: **no Big Core.** `$99B3` sends `$19 >= 5` straight to `$86`, so
`$84`/`$85` and kind `$98` never run; the stage ends on scroll (`$98F0[5] =
$0C`). The `$81`/`$82` window is *inside* normal play (object AI, spawner,
collisions and scroll lock all run).

Background/music: `stage_bgs::STAGE6` (416x28, bank 2); `$1C = $93` while
`$3F < 4`, then `$61`. Measured flip between frame 2300 (`$3F = 3`, `$93`) and
4300 (`$3F = 7`, `$61`), i.e. the `$8354[5] = 4` boundary (~2313). `$AC` at
6955 is the clear jingle, not a boss cue.

### Spawner, first 1000 frames

Measured wave table (identical in the stage-6 and stage-7 `--stage` oracle
runs up to ~frame 740; the diff run's autopilot then perturbs slot reuse):

| frame | spawns |
| ---: | --- |
| 294, 305, 316, 327, 338 | `$85` x5, slot 9..5, `x = 240`, `y = 42` |
| 371, 380, 387, 391, 402 | kind `$02` debris (the wave is shot down) |
| 417 | slot 5 kind `$01` |
| 425, 436, 447, 458, 469 | `$85` x5, `y = 160` |
| 553, 564, 575, 586, 597 | `$85` x5, `y = 42` |
| 681, 692, 703, 714, 725 | `$85` x5, `y = 160` |
| 745, 748, 751, 809 | `$84` (132) x4, `y = 56/104/152` (stage 6) |

So the opening stream is a 5-ship `$85` wave every 11 frames, two y lanes
(42/160), repeated by column; the first section of stages 6 and 7 is the same
formation (`$A822`/`$A830` both start with it: the two oracle dumps are byte
identical through frame 744). Spawn frames beyond ~740 depend on the formation
gate `$A325` (it waits for a free slot), so pin the *scroll* key, not the
frame, when the player's kills differ.

## Stage 7 (`$19 = 6`)

Same entry shape: `$19 = 6` at 263-265, `$1B = $80` at 292.

| frame | event |
| ---: | --- |
| 5929-6050 | **`$1E` wall**: scroll held at `0B:00` from 5929 through 6050 (122 frames, `$B563`); the `stage7-297-6050` fixture records the hold and the scroll resumes at 6051 |
| 6563 | `$3F = $0C` = `$9A30[6]`, `$1B = $81` |
| 6564 | `$1B = $82`; special timer `$4D = 1, $4C = $CA` = 458 frames |
| 7022 | `$1B = $83` |
| 7023 | `$1B = $86`, no `$AC` |
| 7024 | `$98F7` with `$19 = 6` jumps straight to `$9865` (the `$98F0` wait is stage 6's): `$1B = $87`, `$3F = 0`, `$28 = 1` |

Only **one** scroll freeze happens in this run (`$3F = $0B`), not two, even
though the stream has two `$1E` objects; the second never stopped the scroll
before the `$82` window. The checked-in `stage7-297-6050` fixture pins the hold
as 5929-6050 (122 frames) with the scroll at `0B:00` throughout and resuming at
6051. Do not assume two stops when pinning `ai_wall`. The `$C433[6] = $C423` script is a bare `RTS`: no hazard, no object
spawned by `$C40D`, no `$98`.

Content: the brain (screenshot at 6900, `s67-p6-f6900.png`) and the cable
"organic" walls are `stage_bgs::STAGE7` tiles (416x28, bank 3), plus the `$1E`
walls and `$20`-`$25` background pieces. Music: `$1C = $93` while `$3F < 2`,
then `$63` (measured: `$3F = 1` -> `$93` at 1100, `$3F = 2` -> `$63` at 1500,
boundary `$8354[6] = 2`, ~frame 1289).

## Stage-7 fixture and measured divergences

`tests/fixtures/stage7-297-6050.state` (5754 oracle frames, 297-6050) and
`crates/gradius-diff/tests/stage7_replay.rs` replay the poked window with no
cartridge: `Game::new_at_stage(6)` with `dev_keep_alive(true)`. The test is report-only:
its fixture ends at 6050, before the clear window opens at 6563, so it
publishes the summary instead of gating; the regeneration command is in its
module docs.

Measured with `cargo test -p gradius-diff --test stage7_replay` on `main` at
`f97b240`: **11,512 divergences** over 5754 compared frames. The first is
frame **4573 `obj.slot`** (oracle absent, native slot 5 kind `$8C`). The
residue is triaged in `stage7_triage.md`: 79.8% is the `$02` anchor seed, whose
`$0F` turret grid fires 9 frames early (a constant 39-counter offset over all
5754 frames, with a core card in flight).

`scroll` is **byte-identical over the whole fixture, 5754/5754**. `$B563` now
freezes only once the wall's X drops below `$B0`, so the native scroll holds
from **5929 through 6050** like the oracle; the old native freeze at 5801 is
gone. The port reaches `$3F = $0C` at **6562**.

### The gap 6051-6562 and the carried `+10` score

Measured on `main` at `05400fc` with the clear replay (which starts at the 297
anchor, holds fire on every frame, and compares 6563-7024) and
`record-ending`:

* **6051-6562 carries no score event.** At 6051 the oracle already reads
  **830** and the port **840**, and both stay constant through 6562; neither
  side scores after **5944**.
* Every simulated field over that gap (`px`/`py`/`scroll`/`speed`/`lives`/
  `meter`/`obj.kind`/`obj.x`/`obj.y`/`obj.substate`) is exact. Only `score`
  and `obj.slot` are carried, the latter from **6243**.
* The first real divergence of the fire replay is frame **390**: the port kills
  the slot-9 scout, kind `$85`, at (116,86), and the oracle keeps it, killing a
  different scout at **406**. The net is one extra 10-point kill in the port,
  which is the `+10` (840 vs 830) already present at the 6051 seed.
* Death times move ±30 frames when the score delta is changed, so the cause is
  the **death-timing rule**, not one stray object: no single object can be
  pointed at. It does not live in the 6051-6562 gap.

## Ending (`$9865`)

| frame | `$1B` | what happens / what is drawn |
| ---: | --- | --- |
| 7024 | `$87` | `$9B38`: stage byte 0, scroll 0, `$1A = $28 = 1`, score 830 and lives 3 kept. Screen still the stage-7 frame |
| 7025 | `$88` | `$9BE7` PPU/attribute/music setup; `$19 = 0`; screen **black** |
| 7027 | `$89` | PPU/attribute update; black |
| 7028 | `$8A` | palette/starfield setup; black |
| 7029-7051 | `$8B` | ending-ship animation, screen **black** (probe 7030 = all black) |
| 7052-8253 | `$8C` | 1202 frames: starfield, Vic Viper flies up-left, the `$28` ending ship sits at `(0x74, 0x88)`, `CONGRATULATIONS!` types in |
| 7998 | `$8C` | +10000 and lives 3 -> 4 in the same frame (score 830 -> 10830) |
| 8254 | `$8D` | `$1B = 0`, `JMP $9B38` |
| 8255-8259 | `$01`-`$04` | stage-1 entry sequence (`$1B` `$01`/`$02`/`$03`/`$04`) |
| 8282 | `$80` | stage-1 play with `loop = 1`, score 10830, lives 4 |

Duration: `$87`..`$8D` = **1231 frames** (7024-8254); stage 7 end to playable
stage 1 = **1258 frames** (7024 -> 8282). `$8C` alone is 1202.

What is drawn, measured from PNGs:

* 7030, 7052, 7450: black or bare starfield, **no** text (so `$8B` draws
  nothing visible; the 23-frame ship animation is off/behind the blanked
  screen).
* 7200: starfield + the player's Vic Viper flying up-left + the ending
  mothership sprite (the `$28`/`$BB09` object).
* 7600: `CONGRAT` (text is typed out character by character), 7800 and 8100:
  `CONGRATULATIONS!` complete. The HUD stays on screen (score/lives row), the
  stage-7 playfield is gone (`$9BE7` uploads a fresh nametable).
* Music: `$1C = $A5` for the whole ending (measured at 7080, 7900, 8254):
  `$A5` is the *boss* track in `gradius-audio` (`BOSS_ID`), not a new theme.
  At 8300 `$1C = $93` (stage-1 intro again).

After the ending (measured with `probe --ram` at 8254 and 8300):

| byte | value | note |
| --- | --- | --- |
| `$19` | 0 | stage 1 |
| `$1A`/`$28` | 1 | loop counter, copied by `$9B38`/incremented by `$9865` |
| `$17` | 1 | difficulty rank, rebuilt from the live power state (`$9C3F`) |
| `$20` | 4 | lives (3 + the 10 000-point extend) |
| `$2A` | `$02` | next extend threshold raised to 20 000 (BCD high byte) |
| score | 10830 | kept, HUD `000001` x10 in the BCD dump of the run |
| `$3F`/`$3E` | 0 / 7 | stage-1 scroll restarts |
| `$1C` | `$93` | stage-1 intro cue |

## Port status on this branch (explicit)

Stage 6 is wired end to end; stage 7's clear path and the ending are wired,
with the ending screen and ship objects still open:

* Stage 6 boots and clears: `new_at_stage(5)` runs the three-frame `$9B38`
  warm-up, `QUIRK_NO_CORE` is read at `$3F = 0x0B`, the `$81`/`$82` window uses
  the live `$9A28[$17]` rank timer, `$83` goes to `$86` inside normal play,
  `$CD9F` erases the `$CE2B` wall at two cells/frame, and `$90` advances at
  `$3F = 0x0C` keeping `$3E/$3D`. No kind `$98` is spawned.
* The `--stage 6` window is an exact gate, not a diagnostic: 297-1500 replays
  with 0 divergences, every field 100% and 6759/6759 object slots, and the
  ROM-free `stage6_clears_to_stage_seven` pins the first stage-7 frame at 7172
  (`$90`/`$96C4` advances at the start of the next frame, oracle 7468).
  Pixels 297-1500 stay report-only at 99.7% (play 99.7%, HUD 100.0%, 1203
  samples). Details in `stage6_prep.md` §8 and the `stage6` row of
  `docs/DEVELOPMENT.md`.
* Stage 6 still lacks the `$C6D8` collision-only beams (S6-01) and the `$AC`
  clear cue (S6-03); neither is covered by the state gate.
* Stage 7 now runs the same three-frame `$9B38` warm-up and takes the
  `QUIRK_LOOP` no-core `$81`/`$82`/`$83`/`$86` path with `$9A01`'s fixed
  `$4D = 1, $4C = $CA` timer (458 frames). `$3F` reaches `$0C` at 6563; on the
  first `$86` frame `$98F7` tests `$19 == 6` *before* the `$98F0` threshold and
  jumps straight to `$9865` (no `$3F = $0D` wait), `loop_count` increments,
  `$3F` clears, and score/lives/`$19` stay at the measured 7024 state. The
  `$87`-`$8D` ending machine then runs: `$88`/`$89`/`$8A` setup, the 23-frame
  `$8B`, the 1202-frame `$8C` with the `$CF35` row revealed one character every
  9 frames from 7538, the +10000 / lives 3 -> 4 at 7998, `$8D` at 8254 and
  stage 1 playable at 8282. `ending_replay` seeds that measured 7024 `$87`
  state and gates oracle 7025-8290 (1266 frames) with 0 divergences on
  phase/scroll/score/lives and `px`/`py`; it uses `compare_objects: false`
  because the `$CF90` column screen (the port still draws its procedural
  `starfield()`) and the `$987F`/`$BB09` ending ship objects are not ported.
* `--stage 7` is an exact gate over the fixture window: 297-6050 replays with
  **0 divergences** after the block-11 collision-map fix (the old 11,512
  divergences and the kind-`$87`/`$93` walker `obj.y` class are gone), and
  `scroll` is byte-identical over the whole window (5754/5754, freeze
  5929-6050). The kept-alive 297-7100 run is now exact too (0 divergences,
  every field and object slot 100%): `$AF0A` hides background pieces 32-37 for
  `$02 & $1F >= $1A` like the oracle's `$012C` filter, a hit on the frame
  `$9A40` arms `$81` clears `$60` (`$C1D0`, the spawner then never reads its
  stream), and the extractor now keeps stage 7's last two collision blocks
  (12-13, the top wall the kept-alive ship is pinned against). The clear replay
  6563-7024 stays report-only only because the carried `score` (`+10`, 840 vs
  830) is a death-timing rule, now accepted in `docs/KNOWN_DEVIATIONS.md`; its
  `px`/`py`/`scroll` are exact and its former `obj.slot` residue is gone.
  Its checked-in fixture is described above.

So the port work left is S6-01/S6-03 plus the ending's `$CF90` screen and
`$987F`/`$BB09` ship objects in `stage67_plan.md`; the fixture settles the
measured residue, and the inventory below is the current data status.

## Data the pack carries and what is still missing

The pack (`xtask extract`) carries all 2048 CHR tiles (both pattern tables,
every bank), the 64 NES palette colours, the GAUD audio tracks and the decoded
stage tables. For stages 6/7 and the ending:

| item | in the pack? | where it lives |
| --- | --- | --- |
| stage-6/7 tiles (beam wall, cell wall, brain, cables) | yes, banks 2/3 are in the 2048-tile set | `gradius-pack` |
| `STAGE6`/`STAGE7` screens (416x28) | no, and not needed at runtime: generated Rust | `stage_bgs::STAGE6/7` |
| stage-6 beam sprite | none exists (beams are invisible, `$012C` never set) | - |
| ending ship sprite `$9E` and the `CONGRATULATIONS!` glyphs | yes: CHR pixels (the ship tiles and the text row in CNROM bank 3) | `gradius-pack` |
| ending text source `$CF27` | **yes**: `ending_text_record`, 8 pointers plus the record selecting the `$CF35` source; `draw_ending_text` reads it from `Assets` | `gradius-assets` |
| starfield screen | yes, `$CF48`/`$CF90` stream and classes; the port still draws its procedural `starfield()` | `gradius-assets` (column builder not ported) |
| kind `$28` -> sprite tile map | **yes**, `ENDING_SHIP_MOTION`: 26 pairs + `$FF`, tile = `$96 + low nibble` | `$BB09` handler runs it |
| music `$61`/`$63`/`$93` | yes | `gradius-audio` `Music::Stage6`/`Stage7` |
| ending music `$A5` | yes, it is the **boss** track | `Music::Boss` |
| `$CE2B` 88-byte wall-erase table | **yes**, decoded as `STAGE_WALL_ERASE` | `gradius-assets` |
| `$CF27` ending text record | **yes**, decoded as `ENDING_TEXT_RECORD` (`ending_text_record`): 8 pointers plus the record selecting the `$CF35` source, read by `draw_ending_text` | `gradius-assets` |
| `$CF48`/`$CF90` ending screen stream and classes | **yes**, decoded as `ENDING_SCREEN_STREAM` (28 B) and `ENDING_SCREEN_CLASSES` (15 x `$38` B) | `gradius-assets` |
| ending tile-class tables `$9FD6`/`$9FE6`, `$D6F2`, `$D772` | **yes**, 32 + 128 + 1605 B: `ENDING_CLASS_POINTERS` / `ENDING_CLASS_ATTRIBUTES` / `ENDING_CLASS_TILES` | `gradius-assets` |
| `$9A28[$17]` rank timers, `$98F0`, `$9A30`, `$9BC6` | no | already in `stage1_tables`, rows 5/6 |

`stage7_replay.rs` checks this through the pack API when `assets/gradius.pak`
exists. `$CE2B`, `$CF27`, `$CF48`/`$CF90` and the tile-class tables are
extracted; the pack is 228,402 bytes. The text is wired: `draw_ending_text`
reads `ending_text_record` from `Assets` and the embedded constant is gone.
The remaining ending gaps are code, not data: the `$CF90` column screen builder
and the `$987F`/`$BB09` ship objects.

## Stage 7's door (`$B563` + `$B600`)

The `$1E` wall does not only freeze the scroll: it closes the brain-chamber
gate. `$046C` counts a 20-frame step (`$04AC`), and each step queues two
4-cell nametable rows through the `$0700` queue, from the outside in:

| `$046C` | rows (NT1) | tiles | collision (`$B60C`) |
| ---: | --- | --- | --- |
| 2 | 11, 16 | `$C5`/`$C3` | groups 2, 4 = `$FF` |
| 3 | 11, 16 | `$C4`/`$C2` | — |
| 4 | 12, 15 | `$C5`/`$C3` | group 3 = `$C3` |
| 5 | 12, 15 | `$C4`/`$C2` | — |
| 6 | 13, 14 | `$C5`/`$C3` | group 3 = `$FF` |
| 7 | 13, 14 | `$C4`/`$C2` | — |

Measured: the wall spawns at 5801, crosses `$B0` at 5928 (the scroll freeze),
the queues land at 5948, 5968 ... 6048 and the NMI shows each one a frame
later, so the corridor goes black -> closed on 5949..6049. The collision bytes
`$06C2`/`$06C3`/`$06C4` (column 376 = `$2578 & $1F` + `$3F = $0B` block) become
`$FF`/`$C3`/`$FF` on the 5928-block page as each pair lands.

The wide `STAGE7` map was sampled after the wall painted, so it carried the
closed door (`$C4` rows 11-13, `$C2` rows 14-16) and the port drew it closed
from the start. The extractor now drops the wall's writes to those 24 cells
(`screens/builder.rs`), the map holds the open corridor, and the port replays
the paint into its `cells` overlay (`stage7_door`, called from
`frame/step.rs` before the AI pass so the counter is the one the frame
started with). A painted cell is solid for the ship, the walkers and enemy
bullets (`$B60C`), and the drawn tile follows the same step.

Two one-pixel latch frames come with the hold: the frame the wall starts
freezing (5928) draws the previous latch, and the first step after the despawn
(6050) draws the held one. `scroll_lag` follows both (`wall_resume`).

Measured after the change with the card's command: 0 state divergences and at
most 8 differing pixels per frame inside the door's rect (176-240 x 68-132) over
5600-6120, none above 10.
