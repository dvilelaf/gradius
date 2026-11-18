# Technical notes

How the port is built, tested and organised. To play the game, see the [README](../README.md).

## Build from source

You need Rust 1.80 or newer, and on Linux the SDL2 development package (`sudo apt install libsdl2-dev`). Then:

```sh
cargo run -p gradius-sdl --release
```

The first run asks for your ROM, as above. The guides are:

- [`docs/DEVELOPMENT.md`](DEVELOPMENT.md): developer setup, tests and tools.
- [`docs/PACKAGING.md`](PACKAGING.md): how the AppImage and the Windows zip are built.
- [`docs/RELEASE.md`](RELEASE.md): the checklist for publishing a release.

---

## Project details

A native Rust reimplementation of **Gradius (E)** for the NES. The goal is not
to emulate the console at runtime: the game logic and renderer are rewritten
from observed behaviour, the art and palettes are extracted once into a small
asset pack, and the runtime never touches a cartridge image.

The project is split so the game can run on more than one platform. The
simulation is a plain Rust crate with no operating-system dependency; the
fixed-step runner knows nothing about a window; the only SDL2 code lives in
`gradius-sdl`.

> **Status: the whole game runs against recorded fixtures.** Stages 1-7 replay
> from the cartridge's own state, the ending and the loop restart are gated, and
> two-player alternating play and save states work on both frontends. Stages 1,
> 4 and 5 are closed, with their presentation-only residue accepted in
> [`docs/KNOWN_DEVIATIONS.md`](KNOWN_DEVIATIONS.md); stage 2's handoff, boss
> and end are done (the 15968 eruption `obj.y` class is still being fixed) and
> stage 6 runs end to end, its beam wall and clear cue included; stage 3 is state-exact
> from reset over 19429-23000 with report-only pixels; stage 5 is state-exact
> over 297-12003, clear, core and stage-6 entry included; stage 7 is exact over
> 297-6050, its clear 6563-7024 is exact in `px`/`py`/`scroll`, and the ending
> gate runs 7025-8290 with the `$CF90` screen and the `$987F`/`$BB09` ships
> ported. The clear's carried `+10` score is a death-timing rule, accepted in
> the ledger.
> Two-player covers the runtime, the `$0F`/`$0320` menu, the `$2384` banner, the
> `$977C` continue code and both pads, and the 13-row real-input matrix is 13/13
> clean. Save states round-trip byte-exactly (determinism over the 300-frame
> rewind probes and 600-frame replays) on SDL F5/F9 and the PWA, with a 32 B
> `PLAYERS` chunk for the 2P turn machine; the only gap is audio, which restarts
> the current cue. The core, harness and tooling have been split into modules;
> the two files still over the 1,000-line cap (`ai.rs`, `game.rs`) each carry a
> split card, and `scripts/check-file-size.sh` enforces the cap in `verify` and CI.

The stage definition of done and the accepted presentation residue are tracked
in [`docs/KNOWN_DEVIATIONS.md`](KNOWN_DEVIATIONS.md).

## Current status

| area | state |
| --- | --- |
| Asset pipeline | `scripts/extract-data` builds `assets/gradius.pak` from your ROM (`cargo xtask extract` is the same step); `inspect`, `tiles`, `probe`, `shot`, stage-aware `screens --stage N`, `stages`, `stage-enemies`, `stage2-tables`, `sprite-groups`, `spawn`, `sections`, `ramdump`, `disasm`, `trace` and the `record-*` tools support analysis |
| NES oracle | `gradius-nes` is a small CPU/PPU emulator that boots the original and dumps frames, OAM, nametables and hashes for measurements. Development-only: it is not in the runtime |
| Core | Title roll-in and selected-row blink, attract demo, stages 1-7 runtime, the ending and loop restart, always-on ship-vs-map collision, score/lives HUD, deterministic 60.0988 Hz stepping |
| High score | Real in-session high score in the HUD, cold-boot seeded to `0050000`; it is not persisted between runs |
| Two-player | Alternating play: `TwoPlayer` owns `$18`, the per-slot lives, BCD score banks and the `$24/$26/$28` checkpoints. A death runs the 120-frame `$C1DA` animation and rebuilds the shared stage from the incoming player's checkpoint; game over needs both alive bits clear. The HUD and the `$2384` banner follow `$18`, and both frontends feed the second pad. The `$977C` continue code (Down, Up, B, A, B, A, B, A, B, A) is scanned during the 399-frame game-over window and restores the player at `$1B = $80` with 3 lives, threshold 1, score 0 and column 0; the per-player `$C0` window is not modelled. Measured: `$18` 0 -> 1 at 1194, P2 normal play at 1221, `p2-handoff` 0 divergences from reset |
| Runner | `gradius-app` owns the fixed-step clock, pause, catch-up cap, `InputProvider`/`FrameSink`/`SoundSink` traits and `MusicDirector`; backends stay thin I/O shells |
| Save states | `Game::snapshot`/`restore` (`GRSS` v1.0; CORE 2,524 B + 41 B EXT + `PLAYERS` + OAM tail, ~2.8 KB typical in an 8,192 B slot). The `PLAYERS` chunk (32 B) carries both player banks, `$18`, the `$1B` turn state and `$33`, so a mid-2P save round-trips; an older 31 B chunk is refused, and images without the chunk still load. Determinism is byte-exact over 600 -> 1,200, 786 -> 1,386 and `--stage 5/4 --rewind-at 300`. SDL F5/F9 writes `savestate.grs` atomically and rolls a corrupt load back; wasm/PWA uses `localStorage` (2,989 B smoke, 60/60 frames). Both frontends refuse corrupt or truncated images. Audio is the one gap: a load restarts the current cue from its beginning |
| SDL2 frontend | Window, keyboard, hot-plug game controllers, integer/aspect/stretch fitting, fullscreen, pause, BMP screenshots, captured NES APU audio, WAV dump, headless smoke mode |
| Title screen / menus | Title roll-in and selected-row blink reproduced; attract/demo frames 130-800 have 0 state divergences and 99.999% pixels. The title ship matches, the Select cursor follows the selected row (`$0320` at pixel y 150 for 1P and 2P), and Start locks `$03 = $40`/`$70`; the 2P entry sets `$20/$21 = 3`, both `$0A` alive bits, `$18 = 0` and zeroes both score banks |
| Stage 1 script | Ported through the Big Core and the stage-clear transition; 3,000-frame state parity with no divergence and a 99.9994% pixel match over 700 frames, see below. Boss damage starts at frame 8066 and the clear lands at 9087 with score 2690; the gated 7500-9700 window has 0 divergences and matches the post-clear slot-9 debris and stage-2 reset, while the 12,000-frame long run stays a soft WARN. The 448-column map carries the `$834B` tail (CHR bank 2, black sub-palettes 2-3) from world column 416, and the late window 6000-7723 is gated at >= 99.9% with frame 6500's playfield exact; its only HUD residue is a 20 px score-row lag |
| Stage 2 | Handoff window done: the natural entry at 9601 is state-exact through 12000, the 9601-11199 pixel gate matches 99.9% (play 99.9%, HUD 100.0%), and the >11199 tail is report-only at 99.7%. `--stage N` plus the `stage2-9601-12000` fixture exercise the window. Boss/end are fixed: 17280-19430 has 0 divergences, both sides self-destruct at 18915 and stage 3 starts at 19429, and the 12161 formation class is fixed; the only remaining natural divergence begins at 15968 (`$C540` eruption `obj.y`), being fixed in the `s2erupt` pass. Boss/end pixels are 99.8% / 99.8% / 100.0% and accepted: the core group (`$6C`), the `$41` shots, the frozen-scroll latch, the x-wrap and the ring phase are fixed, and the hand-off 19037-19428 is exactly 100%; the remainder is rotating OAM flicker and shot alignment. Stage-2 music is done: the natural run starts the `$59` theme at the stage change (covered by the `gradius-app` natural-handoff test), and stages 2-7 are selected from the stage's `$8344` id |
| Stage 3 | Natural window 19429-23000 replays from reset with no cartridge: `WITHIN TOLERANCE (0 divergences)` over the whole fixture, the 21526 capsule included. The `$B622` descender sprite, the `$17` small core's `$B79B` body, the aiming turret's `$03AC` cannon phase, the shield-ring `$02` phase and the stage-3 sprite CHR bank are fixed. Pixels are report-only at 98.9% overall (play 98.8%, HUD 100.0%) over 119 samples; the residue is the missing `$BBB1`/`$BC3E` object-fire sprites (group `$25`, OPEN), the eight-per-scanline priority subset; the 19429 entry-frame ring is fixed by the ROM's `$1B & #$70` hand-off skip at `$8B74`, so 19429 and 19430 are exact, as in [KNOWN_DEVIATIONS.md](KNOWN_DEVIATIONS.md). The `$C900` turret overlay is wired, but no turret is live in this window. The ten-kill `$39` clear is wired: death + 513, +5000, the `$C680` `$3A` bonus wave and stage 5 |
| Stage 4 | Boot entry wired: `new_at_stage(3)` runs the `$9B38` three-frame warm-up (the ROM's 6 px first-column spawner lead), `$1C` stays `$93` until the `$8354[3] = 4` boundary selects `$5D`, `$39` is never set and the stream has no `$14` command records. The poked 297-1797 diff is state-exact: objects 7167/7167 (kind/slot/x/y/substate), scroll, score, lives, meter and speed 100%; the only residue is 162 `px`/`py` on 81 `Other(4)` respawn frames where the port reports the ship at 0,0, accepted in [KNOWN_DEVIATIONS.md](KNOWN_DEVIATIONS.md). Pixels are report-only at 99.773% overall (play 99.750%, HUD 99.919%) over 50 samples, residue the scout-wave OAM subset, the HUD lives digit and the sampled respawn frames. `$C5A7` and the core gate are not wired, and `STAGES[3].quirks = 0` leaves the column-416 tail (CHR bank 2) unapplied past 3328 px |
| Stage 5 | Boot and cannon chain wired: section 4 gets the same `$9B38` three-frame warm-up as stage 4, so the column-0 stream surfaces with the `$9D` straight piece in slot 9; `$B553` walks the `$52..$57` groups through the `$B64A` Y=9 row and the `$1D` kill is the ordinary `$8468` ten-point path. `--stage 5 --frames 8573` (window 297-8573) is `WITHIN TOLERANCE (0 divergences)` over 8277 compared frames, against 2155 divergences (first at 2601) before the cannon chain. The window covers the `$14` large object (`$CA58`: morph/death thresholds, `$CA51` aim with the `SBC` carry, `$AEDB` drift), its `$0600` cannons (`$A4A0` slot allocation, `$CB8B` 60-frame cadence at rank 2), the five `$C64D` objects and the `$9658` `$19 == 4` / `$5C >= 2` chain (0.25 px/frame scroll and drift). The `$AEDB` drift through the `$82` hold and the `$BF43` phantom debris are fixed, and the recorded clear fixture (8574-12003) now replays exactly: the Big Core's entry branch (`$B9A2`-`$B9AF`) falls through to the `$BA04` fight path when `$048C != 0` and `x >= $A4` instead of returning after the `DEC`, so the 10671 `obj.x` class is gone and the whole run 297-12003 is exact. The fixture covers the core spawn (10274), death (11489) and stage 6 (12003); `stage5_clear_replay` is still report-only, and the 8574-10690 probe is the hard assert. Pixels are report-only: the 297-1500 per-frame sweep is **99.8% overall / 99.8% play / 100.0% HUD** (was 99.7 / 99.6 / 100.0), and the first ring-related pixel divergence moved from frame 300 to 450. The shield ring's `$02` phase is FIXED: `new_at_stage_seeded` seeds `nmi_frame` from the cartridge at the anchor (`0x23` for the 297 anchor) and `build_sprites` reads `(nmi_frame >> 2) & 3` ([shield_ring.md](analysis/shield_ring.md), [stage7_triage.md](analysis/stage7_triage.md) §"The rule"). See [KNOWN_DEVIATIONS.md](KNOWN_DEVIATIONS.md) |
| Stage 6 | Wired end to end: `new_at_stage(5)` runs the three-frame `$9B38` warm-up, `QUIRK_NO_CORE` is read at `$3F = 0x0B`, `$83` goes to `$86` inside normal play, `$CD9F` erases the `$CE2B` wall at two cells/frame and `$90` advances at `$3F = 0x0C` keeping `$3E/$3D`. The 297-1500 `--stage 6` window is an exact gate, not a diagnostic: 0 divergences, every field 100%, 6759/6759 object slots, and `stage6_clears_to_stage_seven` pins the first stage-7 frame at 7171. Pixels stay report-only at 99.7% (play 99.7%, HUD 100.0%). `$C6D8`'s ten beams are live collision slots drawn as the red-ball sprite group `$8D` (`stage6_beams.rs`) and the `$AC` stage-clear cue is wired; see [KNOWN_DEVIATIONS.md](KNOWN_DEVIATIONS.md) and [stage6_prep.md](analysis/stage6_prep.md) §8 |
| Stage 7 | Clear path and ending machine wired: the three-frame `$9B38` warm-up and `QUIRK_LOOP`/`QUIRK_NO_CORE` take the `$81`/`$82`/`$83`/`$86` sequence with the `$4D = 1, $4C = $CA` timer (458 frames) into `$9865` on the frame after `$86`, so the ending starts at **7024**; `$3F = $0C` opens the clear at 6563 and 6563-7024 is exact in `px`/`py`/`scroll` (`stage7_clear_replay`; its carried `score` `+10` and `obj.slot` residue is the death-timing rule accepted in [KNOWN_DEVIATIONS.md](KNOWN_DEVIATIONS.md)). The `$87`-`$8D` phase machine then runs the 23-frame `$8B` build, the 1202-frame `$8C` display with the `$CF35` row revealed one character every nine frames from 7538, the single +10000 / lives 3 -> 4 at 7998, and the 27-frame stage-1 boot into play at 8282. `ending_replay` seeds the measured 7024 `$87` state and gates oracle 7025-8290 (1266 frames) with 0 divergences. The `$CF90` column screen (`ending_screen.rs`) and the `$987F`/`$BB09` ending ships (`ending_ship.rs`) are ported; `ending_replay` still uses `compare_objects: false` because the oracle's ending slots are not part of `dump_state`, and pixels cover the ship. The 297-6050 `stage7_replay` fixture is an exact gate with 0 divergences after the stage-7 collision map filled the block 11 hole under the kind-`$87`/`$93` walkers ([stage7_triage.md](analysis/stage7_triage.md), [stage7_walkers.rs](../crates/gradius-diff/tests/stage7_walkers.rs)). The pack's stage tables now reach the runtime through `Assets.stage`: the `$CF35` ending text record is read from the pack (an old pack returns `AssetError::Missing`, no fabricated bytes), while `stage6_wall_erase` is exposed but `erase_wall` still reads the embedded `stage6_wall::TABLE`. The ending timeline is tracked in [KNOWN_DEVIATIONS.md](KNOWN_DEVIATIONS.md), [stage67_boot.md](analysis/stage67_boot.md) and [ending.md](analysis/ending.md) |
| Original sound engine | Captured Konami driver register streams: 12 music tracks (title, both stage-1 halves, all six stage 2-7 themes, Big Core, stage clear, game over) and nine effects (the pause jingle included, the music freezes under it); the native driver reproduces every capture with 0 mismatches and the triangle/bass line is verified. The timelines are captured from your ROM at extraction and stored in the pack, not in the repository. See [`docs/AUDIO.md`](AUDIO.md) |
| Stage content | All seven stages' spawn/terrain sections and backgrounds extracted and verified; the stage→data table and background hashes are in [`docs/analysis/stages.md`](analysis/stages.md). Runtime wiring covers stage 1, stage 2's boot/handoff/boss/end, stage 3's boot, `$C900` turret overlay, `$B622` descender, `$17` small core and ten-kill `$39` clear, stage 4's boot entry with the late `$5D` boundary, stage 5's boot with the `$9D` piece and `$14` cannon chain, stage 6's no-core clear end to end, and stage 7's warm-up, `$81`-`$86`/`$9865` clear and `$87`-`$8D` ending machine with its `$CF90` screen and `$987F`/`$BB09` ships |
| Enemy coverage | 28 kinds appear in the stage streams; every one has a port handler except `$16`. The per-stage boss/clear map is in progress; see [`docs/analysis/stage_enemies.md`](analysis/stage_enemies.md) |
| Differential harness | `gradius-diff` runs the native port and the cartridge oracle in lockstep and compares state and pixels; `--stage N` starts both sides at a later stage, and it also emits JSON summaries, PNG pairs and ROM-free fixtures for the measured stage windows (stage 1, stage 2, stages 3-7 and the ending). `gradius-scenarios` replays the real menu, power-up, boss and 2P flows from reset; all 13 rows of its matrix are clean, 0 known and 0 failed (every row 100% pixels, 0 state divergences, with the menu cursor on the selected row and `p2-handoff` running the 2P relay from reset: `$18` 0 -> 1 at 1194, P2 normal play at 1221). The matrix and control semantics are in [`docs/analysis/scenarios.md`](analysis/scenarios.md) |
| Ghidra recovery | `tools/ghidra` recovers the indirect call graph: 214 functions and 6,636 instructions, covering 99.9% of the oracle's executed PCs in stage 1 |
| Second oracle | `tools/nesrecomp` compares the port against a NESRecomp build: 3,600/3,600 hashes match and gameplay pixels agree 99.94% |
| WebAssembly demo | `gradius-wasm` plus `web/` play the core in a browser canvas with Web Audio; `web/build.sh` builds the bundle |
| Power meter / upgrade options | Six-slot meter with SPEED UP (repeatable), MISSILE (once), DOUBLE and LASER (toggle), OPTION (cap 2) and SHIELD (once, five hits); all six verified end to end; B held while `$0100 == 1` applies the selected item |

The measured constants and the evidence for them live in
`docs/analysis/stage1.md`.

### Measured parity

`gradius-diff` runs the port and the oracle one frame at a time with the same
input script. Only frames where both sides report `Playing` are compared; the
title and attract transitions are counted as skipped frames. These numbers come
from the standard stage-1 scripts (a 3,000-frame state run and a 700-frame pixel
run), the stage-2 handoff run and the 2P scenario rows, on a local
`Gradius (E)` dump and pack:

| measurement | result |
| --- | --- |
| Title and attract demo | title roll-in and selected-row blink reproduced; demo frames 130-800: 0 state divergences and 99.999% pixels |
| 2P menu and start | Select flips `$0F` 0 -> 1 and moves `$0320` 134 -> 150 px; Start locks `$03 = $70`; the new game sets `$20/$21 = 3`, `$0A = $07`, `$18 = 0` and zeroes both score banks |
| 2P handoff | `p2-handoff` (1300 frames from reset): Select 180 + Start 220, P2's Start inert while P1 plays, `$18` 0 -> 1 at 1194 with lives 2/3 and P2 normal play at 1221; 0 state divergences and 100% pixels |
| Game-over banner | "1P GAME OVER"/"2P GAME OVER" in the power-meter row (text id `$1C` at `$2384`, tile `31` -> `32` via `$980B`), not the old centred box; the window starts only when both alive bits are clear |
| State parity | 3,000-frame lockstep run with 0 divergences; every compared field and object slot at 100% |
| Pixel parity | 99.9994% overall over 700 frames (241 of 43,008,000 pixels); playfield 99.9994%, HUD 100.0000%. First divergence frame 391 (1 px at `206,50`); worst frame 430 (38 px, 99.9000%) |
| Game-over flow | runs through to the title; the port reaches `Title` at 4676, the same frame as the oracle, and the title/attract re-entry is exact (0 state mismatches in the default 12,000-frame run). A 2P game over additionally names the player in `$18` in the `$2384` banner |
| Captured audio | 12 music tracks and 8 effects; the native driver reproduces every captured write with 0 mismatches, and the triangle/bass line is verified |
| Big Core timeline | spawn frame 7723, first damage 8066, clear 9087 (+1000 points, score 2690); stage 2 starts at 9601. The gated 7500-9700 window has 0 divergences and matches the post-clear slot-9 kind-2 debris and the stage-2 reset |
| Stage-1 late window | 6000-7723 (up to the core spawn): >= 99.9% overall and playfield, from 88.1% match / 86.6% play before the 448-column tail and held-latch fix; frame 6500's playfield is 0 differing px, and the HUD residue is a 20 px score-row lag |
| Stage-2 handoff | natural entry at 9601 after the stage-1 clear; state exact through 12000 and the 9601-11199 pixel window at 99.9% (play 99.9%, HUD 100.0%); the full 9601-12000 report is 99.7% and the >11199 tail is report-only |
| Stage-2 boss/end | 17280-19430 is `WITHIN TOLERANCE (0 divergences)`, every field and object slot at 100%; the natural core spawns at 17282 on both sides, both self-destruct at 18915 (kind-2 debris, no +1000) and stage 3 starts at 19429. First post-window divergence 15968 (`$C540` eruption `obj.y`, being fixed); boss/end pixels are 99.8% match / 99.8% play / 100.0% HUD and accepted, with 19037-19428 exactly 100% and the remainder the rotating OAM flicker and the shot alignment |
| Stage-3 natural window | 19429-23000 replays from reset with no cartridge: `WITHIN TOLERANCE (0 divergences)` over the whole fixture, the 21526 capsule included (score 3110, meter 4). Pixels report-only at 98.9% overall (play 98.8%, HUD 100.0%) over 119 samples, worst samples 21420/21390/21690 at 97.1-97.3%; residue the missing `$BBB1`/`$BC3E` object-fire sprites (group `$25`, OPEN) and the eight-per-scanline sprite priority subset. The `$B622` descender, the `$17` small core, the `$03AC` turret phase, ring phase, sprite CHR bank and the ten-kill `$39` clear are fixed |
| Stage-4 boot window | `--stage 4 --frames 1900 --from 297 --to 1797` (1501 compared frames): 162 divergences, down from 9131 before the boot warm-up. Objects are 7167/7167 at 100% (kind/slot/x/y/substate), and scroll, score, lives, meter and speed are 100%. All residue is `px`/`py` on 81 `Other(4)` respawn frames (497-523, 739-765, 1410-1436) where the oracle reports the ship at `80,96` and the port at `0,0`; accepted. The `$9B38` three-frame warm-up reproduces the 6 px spawner lead, and `$1C` switches `$93` -> `$5D` at `$8354[3] = 4` (equivalent frame 31304) |
| Stage-4 pixels | 297-1797 sampled every 30th frame (50 samples): 99.773% overall / 99.750% play / 99.919% HUD; the per-frame sweep (1,502 samples) is 99.785% / 99.767% / 99.908%, worst samples 1320 (98.7%), 1080 (99.2%), 1350 (99.3%). Report-only: the residue is the scout-wave OAM subset, the HUD lives digit from sample 1470 and three sampled respawn frames. The background is exact through the column-416 tail (CHR bank 2, entries 8-15 at `0F`) and 0 mismatches at 0/512/1100/3328/3500 px, but the port's `STAGES[3].quirks = 0` does not apply the tail yet |
| Stage-5 boot window | `--stage 5 --frames 8573` (297-8573): `WITHIN TOLERANCE (0 divergences)` over 8277 compared frames, against 2155 divergences with the first at 2601 before the cannon chain landed. The window covers the `$14` objects, the 0.25 px/frame `$19 == 4` chain and the `$0600` cannons on the measured 60-frame cadence. The `$6C` member timer is RAM that only a successful allocation reloads (`$A42C`), and the 3055 cannon kill matches after the `$CB8B` tail call. The recorded clear fixture 8574-12003 now replays exactly (0 divergences, core 10274, death 11489, stage 6 at 12003): the Big Core's entry branch falls through to the `$BA04` fight path when `$048C != 0` and `x >= $A4`, so 10671 `obj.x` is gone and 297-12003 is exact |
| Stage-6 boot window | `stage6_replay` (window 297-1500): **0 divergences**, every field at 100% and 6759/6759 object slots, after the `$9B38` warm-up and the `QUIRK_NO_CORE` `$81`/`$82`/`$83`/`$86` clear; `stage6_clears_to_stage_seven` pins the first stage-7 frame at 7171. Pixels are report-only at 99.7% (play 99.7%, HUD 100.0%, 1203 samples). `$C6D8`'s ten beams are live collision slots drawn as the red-ball sprite group `$8D` and the `$AC` stage-clear cue is wired |
| Stage-7 window | `stage7_replay` (297-6050, 5754 frames): **0 divergences**, an exact gate after the block 11 collision-map fix. `stage7_clear_replay` (6563-7024, 462 frames) has `px`, `py` and `scroll` at 100% (max error 0); its carried `score` (840 vs 830) and `obj.slot` are accepted residue: the `+10` is a death-timing rule (first real divergence frame 390, kind `$85`), not the 6051-6562 gap |
| Ending | `ending_replay` seeds the measured 7024 `$87` state and gates oracle 7025-8290 (1266 frames) with 0 divergences. The `$CF90` column screen and the `$987F`/`$BB09` ships are ported; `compare_objects: false` because the oracle's ending slots are not part of `dump_state` |

The boss timeline is an oracle measurement of the original, and the port now
follows it: it takes damage at frame 8066, clears at 9087 with score 2690, and
its stage-1 clear hands off naturally into stage 2 at 9601. The gated 7500-9700
window matches exactly (0 divergences, the post-clear slot-9 kind-2 debris and
the stage-2 scroll reset), state stays exact through 12000, and the 9601-11199
pixel window matches 99.9% (HUD 100.0%). The 448-column map's tail and held
latch are gated separately over 6000-7723 at >= 99.9%, with frame 6500's
playfield exact. The full 9601-12000 report is 99.7%
and the tail beyond 11199 is report-only. The stage-2 boss and end now match:
17280-19430 has 0 divergences, both sides self-destruct at 18915 and stage 3
starts at 19429, and the 12161 formation class is fixed. The boss/end pixels
follow: 17280-19430 is 99.8% match / 99.8% play / 100.0% HUD, the hand-off
19037-19428 is exactly 100% on all 392 samples and stays the hard gate, and the
remaining rotating OAM flicker and shot alignment are accepted. The only
remaining natural divergence begins at 15968 (`$C540` eruption `obj.y`).
Stage-2 music
is exact: the runner switches to `$59` at the stage change, and stages 2-7
come from the stage's `$8344` id.
Outside the compared windows the 12,000-frame long run still warns (default
script drift from 3159, boss script before the window from 1357 and 6503), and
a 241 px sprite-palette residue remains in the 700-frame run; the game-over
handoff enters `Title` at 4676 like the oracle.

### Port conveniences

The cartridge is the default, including `$C29F`/`$C39D` ship-vs-map terrain
damage, which is now always enabled: the `map_terrain_collision` mod was
removed. `$89DA` stops options at two; the only opt-in liberty in
`gradius_core::Mods` is `extra_options`, which raises the cap to four and
defaults to false. `Game::set_mods` toggles it.

## Quickstart

This repository contains no data from the cartridge: no ROM, no extracted pack
and no oracle fixtures. You build them once, locally, from your own
`Gradius (E).nes`. What you build stays on your machine.

Requirements:

- Rust 1.80 or newer.
- SDL2 development libraries for the desktop frontend (on Debian/Ubuntu:
  `sudo apt install libsdl2-dev`).
- Your own iNES dump of Gradius (E). `cargo xtask inspect "$ROM"` prints its
  SHA-256 so you can check it against the one in your notes.

```sh
# 1. Once, from your own dump. Writes assets/gradius.pak and the oracle fixtures:
GRADIUS_ROM="/path/to/Gradius (E).nes" scripts/extract-data

# 2. Play:
cargo run -p gradius-sdl --release -- --pack assets/gradius.pak

# 3. Test. The tests need the data from step 1:
cargo test --workspace
```

Without step 1 the frontends open the first-run screen, which asks for the ROM and
builds the same pack in the data folder (see [How to play](#how-to-play)). The
tests that need the data fail and name the command to run. `scripts/extract-data` is
idempotent: it rebuilds only when the ROM or the extraction sources change.

The runtime never reads the `.nes` file, so you can move the ROM afterwards.
`gradius-diff` is a development-only tool that reads the cartridge and is never
a dependency of a frontend. For parity against the oracle, run it with the same
ROM:

```sh
cargo run -p gradius-diff --release -- \
  --rom "/path/to/Gradius (E).nes" --pack assets/gradius.pak \
  --frames 3000 --report
```

### Headless smoke run

`--frames N` runs exactly N simulation ticks, does not initialize SDL, and
prints one summary line. It is meant for CI and quick sanity checks:

```sh
cargo run -p gradius-sdl --release -- \
  --pack assets/gradius.pak --frames 180 --no-audio --scale 1
# headless: frames=180 hash=0x3864234cd1a6e199 score=0 lives=3 audio_events=0
```

The hash is FNV-1a over the 256x240 RGBA framebuffer, so two builds that render
the same pixels print the same value. The runner's `Summary` exposes the same
fields through `gradius_app::Runner::run`.

## Controls

| action | player 1 keyboard | player 2 keyboard | game controller |
| --- | --- | --- | --- |
| Move | Arrow keys or WASD | Numpad 8 / 4 / 5 / 6 or T / F / G / H | D-pad or left stick |
| Fire | Z, J or Space | Numpad 0 or R | A |
| Power | X or K | Numpad `.` or Delete, or Y | B |
| Start | Enter or Numpad Enter | Numpad `+` or 1 | Start |
| Select | Backspace or Tab | Numpad `-` or 2 | Back |
| Up/Down or Select: choose 1 or 2 players | Arrow keys, Backspace or Tab | — | D-pad up/down or Back |
| Pause | Pause key | — | — |
| Volume | `+` / `-` (5% steps) | — | — |
| Mute | `M` (toggle) | — | — |
| Fullscreen | F11 | — | — |
| Screenshot | F12 (writes `screenshots/*.bmp`) | — | — |
| Save state | F5 (writes `savestate.grs`) | — | — |
| Load state | F9 (shows `LOADED` or `LOAD ERROR`) | — | — |
| Debug overlay | F3 (toggle; `--debug-overlay` starts it) | — | — |
| Options menu | Esc | Guide | — |
| Quit | Alt+F4 / window close (or Quit in the menu) | — | — |

`+` / `-` change the master volume in 5% steps (and unmute), `M` toggles mute;
the new level is flashed in the window title for a second and a half. Pausing
dims the last frame with a `PAUSED` overlay in the frontend; the core
framebuffer is never modified. `F3` (or `--debug-overlay` at startup) draws the
capsule count, meter slot, speed level, weapon, options, shield hits and active
power flags in the top-left corner, plus a `SELECTED:` line and a short flash
whenever a power is applied. It is frontend-only and leaves headless runs
unchanged.

Controllers are opened through SDL's game-controller layer and hot-plugged; the
first pad that is open drives player 1 and the second drives player 2. A pad
that is already mapped by SDL works without configuration. In a 1P game both
pads drive the one ship (the ROM's `$8203` merge); in a 2P game only the current
player's pad drives it, and the turn alternates when a ship is lost. Player 2
uses the numeric keypad, or T / F / G / H with R, Y, 1 and 2 on a keyboard
without one; no key is shared with player 1's defaults.

### Keyboard bindings

Keyboard bindings live in `keybinds.ini` in the data folder (`assets/keybinds.ini`
in a development checkout) and are created with the defaults above when the file is
missing. The file has `[player1]` and `[player2]` sections, one `button = key`
line per NES button (`up`, `down`, `left`, `right`, `fire`, `power`, `start`,
`select`), and comma-separated aliases where one button uses several keys.
Unknown sections, buttons or key names, a missing `=`, and any key bound twice
(within one player or across both) are rejected: the game prints a warning
naming the line and runs with all defaults. `Pause`, `F3`, `F5`, `F9`, `F11`, `F12`, `Esc`,
`M`, `-` and `+` are reserved hotkeys and cannot be bound. The full key-name list
and examples are in [`docs/DEVELOPMENT.md`](DEVELOPMENT.md#keyboard-bindings).

`Power` applies while B is held and `$0100 == 1` (`$8979`): a capsule collected
on a frame is consumed on that frame, and a successful handler clears `$42` so
holding B cannot repeat the item. The six-slot meter has one item per slot:

| slot | item | semantics |
| --- | --- | --- |
| 1 | SPEED UP | repeatable, +1 each purchase |
| 2 | MISSILE | one-shot |
| 3 | DOUBLE | toggle, mutually exclusive with LASER |
| 4 | LASER | toggle, mutually exclusive with DOUBLE |
| 5 | OPTION | repeatable, capped at 2 |
| 6 | SHIELD | one charge, five hits; no-op while active |

The oracle measurements and the differential tests for each row are in
[`docs/analysis/powerup_terrain_verification.md`](analysis/powerup_terrain_verification.md).

`--fit integer` (default) keeps whole pixels, `--fit aspect` fills a
non-multiple window without distortion and `--fit stretch` fills it, accepting
distortion. `F11` applies the same policy in fullscreen. See
[`docs/DEVELOPMENT.md`](DEVELOPMENT.md) for the viewport examples.

## Options

```
--pack PATH     asset pack (default: gradius.pak in the data folder)
--rom PATH      your Gradius (E) .nes; builds the pack when there is none
--scale N       integer window scale, 1..=8 (default: 3)
--fit MODE      integer (default), aspect or stretch
--print-viewport print the fit, window size, destination rectangle and scale
--frames N      headless: run N ticks, print a summary and exit
--no-audio      never open an audio device (alias: --mute)
--volume N      master volume, 0..=100 percent (default: 100)
--screenshot P  headless: write the final frame to P as a BMP
--dump-audio P  headless: write the soundtrack to P as a 48 kHz mono WAV
--autoplay      headless: hold fire and weave (demo/smoke runs)
--debug-overlay start with the debug overlay (F3 toggles)
--hits N        modified mode: hits before dying, 1..=99 or inf (default: 1)
--start-stage N modified mode: stage a new game starts at, 1..=7 (default: 1)
--2p-sim        modified mode: both players play at once, not by turns
--stage5-slowdown modified mode: stage 5 runs every other frame (the cartridge)
-h, --help
-V, --version
```

If the audio device cannot be opened the game prints a warning and continues
silently. `--frames` never opens a window or an audio device.

## Modified mode

The default is the cartridge: one hit kills and two players take turns. The
modified mode keeps the game playable without the coin-op difficulty and lets
two people play at once, on the same screen:

```sh
cargo run -p gradius-sdl --release -- --pack assets/gradius.pak --hits 3 --2p-sim
```

`--hits` takes 1..=99 and is validated before the window opens; `--2p-sim` makes
both pads live at the same time. Stage 5 is the one place where the port
departs from the cartridge out of the box: the game ships without the `$9658`
frame halving, and `--stage5-slowdown` puts the cartridge's slower stage 5
back. The PWA exposes the same settings through
`gradius_app::Runner::set_mods`. See [`docs/MODS.md`](MODS.md) for what
each one changes and what stays identical to the original.

### Options

Esc (or the pad's Guide button) opens an options menu over the paused game; it
never quits. Arrows or d-pad move and change values, Enter or A picks, Esc or B
goes back. It has Resume, `Start stage` (1..7), `Ship hits` (1..99, then `Invincible`),
`2 players at once`, `Stage 5 slowdown` (Off/On), `Volume`, `Fullscreen` and
`Quit` (asks for confirmation). The choices
are saved in `settings.ini` in the data folder and `--hits`, `--start-stage`,
`--2p-sim`, `--stage5-slowdown` and `--volume` win for the session that gives
them. Hits, start stage, the slowdown and 2-players apply on the title or from
the next game. Choose 2 PLAYERS on
the title to use two ships.

## Audio

The runtime does not synthesise approximations: `gradius-audio` plays captured
`$4000-$4017` register streams from the original Konami driver through a small
NES APU model. The pack carries 12 music tracks (title fanfare, both
stage-1 halves, the six stage 2-7 themes, Big Core, stage clear and game over)
and all eight sound effects, with their real lead-ins and loops. Extraction
captures them from your ROM; without the audio chunk the game plays in silence. The native
driver reproduces every captured write with 0 mismatches, and both the triangle
channel and the stage-theme bass line are checked for audibility.
`gradius-app::MusicDirector` selects a `MusicCue` from public game state and the
SDL backend maps each cue to its captured timeline; the two derived cues are
documented in [`docs/AUDIO.md`](AUDIO.md).

What is still missing: the ending and continue music, stage-number music
selection in the frontends (the six second themes are in the pack, but SDL and
wasm still map only the original cues), and a dedicated `Game::music_cue()`
hook in the core. The current boss/clear cues use the scroll position and the
Big Core's 1,000-point reward because the core does not expose its stage
sub-state.

## ROM and asset policy

Nothing derived from the cartridge is committed: not the ROM, not the pack, not
the oracle fixtures, not the captured audio, and no data blocks taken from the
cartridge. The game logic is reimplemented from observed behaviour; small
constants and rule tables that describe it (for example in
`crates/gradius-core/src/stage1_tables/`) are functional values, not cartridge
data. `scripts/check-rom-data.sh` runs in `scripts/verify` and in CI and
fails on a tracked file that looks like extracted data.

`scripts/extract-data` builds these locally, from your ROM:

- `assets/gradius.pak`: tiles, palettes and metadata; the stage tables
  (`CHUNK_STAGE`); the ROM data (`CHUNK_ROM`: collision, backgrounds, sprites,
  spawn scripts and loose tables); and the captured audio (`CHUNK_AUDIO`). The
  runtime is self-contained once it exists.
- `tests/fixtures/*.state|*.trace`: the oracle fixtures, regenerated by
  `scripts/gen-fixtures.sh`.

The pack and the fixtures are for your own use. Do not redistribute them; see
[`NOTICE`](../NOTICE).

`tools/nesrecomp` drives an external NESRecomp checkout, which is licensed
PolyForm Noncommercial 1.0.0. It is not vendored and nothing it generates is
committed: use it only for noncommercial research and testing, and do not copy
its code or output into this repository.

## Crate map

| crate | responsibility |
| --- | --- |
| `gradius-cart` | iNES parsing and CHR bank extraction. Used only by development tools |
| `gradius-pack` | Self-contained `GRDPAK01` container format and builder |
| `gradius-assets` | Runtime pack reader; no cartridge knowledge |
| `gradius-nes` | Development-only NES oracle (CPU, PPU, mappers) for measurements |
| `gradius-core` | Platform-agnostic, deterministic game and software renderer. No OS deps |
| `gradius-app` | Platform-agnostic fixed-step runner: input/frame/sound traits, pause, catch-up, music cues |
| `gradius-audio` | Runtime NES APU model and captured Konami register timelines |
| `gradius-diff` | Development-only lockstep differential harness against the oracle; state lines, pixel reports, PNG pairs and ROM-free fixtures |
| `gradius-sdl` | SDL2 desktop backend: window, input, audio, headless mode |
| `gradius-wasm` | Minimal `wasm32-unknown-unknown` backend: FFI shell around the core, runner and audio for the browser demo |
| `xtask` | Analysis and asset pipeline CLI (`cargo xtask …`) |

Dependency direction: `gradius-core` depends on nothing; `gradius-assets`
depends on `gradius-pack` and core types; `gradius-app` depends on the core and
assets; `gradius-audio` is a standalone APU model; `gradius-sdl` depends on the
core, assets, runner and audio crates; `gradius-wasm` exports the same stack as
a C ABI to `web/app.js`; `gradius-diff`, `xtask` and the optional
`capture-audio` tool read a cartridge, and none is a runtime dependency.

## Development tooling

| tool | purpose |
| --- | --- |
| `gradius-diff` | Lockstep state and pixel comparison against the cartridge oracle; per-field pass rates, JSON summary, PNG pairs, `--stage N` stage runs, `--emit-fixture` for ROM-free replay tests |
| `cargo xtask` | Asset extraction and analysis: `inspect`, `tiles`, `sprite-groups`, `extract`, `shot`, `probe`, stage-aware `screens --stage N`, `stages`, `stage-enemies`, `stage2-tables`, `sections`, `spawn`, `ramdump`, `disasm`, `trace`, `record-state`, `record-input`, `record-boss`, `verify-sprite0`, `state-drift` |
| `tools/ghidra` | GhidraNes scripts and recovered function list; 214 functions and 6,636 instructions, 99.9% of the executed-PC set. See [`docs/analysis/ghidra.md`](analysis/ghidra.md) |
| `tools/nesrecomp` | Second oracle: builds a NESRecomp host, replays a route and compares pixels. 3,600/3,600 hashes and 99.94% gameplay pixels. See [`docs/analysis/nesrecomp.md`](analysis/nesrecomp.md) |
| `scripts/verify fast\|full\|changed` | Quality gate: formatting and checks, or the full clippy/test/SDL/pack run, or only the crates a branch touched; every mode also runs `scripts/check-file-size.sh` and `scripts/check-rom-data.sh`; `changed` and `full` also refresh the extracted data first when `GRADIUS_ROM` is set, and stop with the command to run when there is no pack |
| `scripts/release-check.sh` | Stage-1/stage-2 release gate: build, tests, extract, state/pixel/boss/stage-2 parity, long run, audio and web/PWA checks. The full `--skip-wasm` run reports 0 hard failures; the 7500-9700 boss window is clean, the 9601-12000 stage-2 state window is clean with a 99.9% 9601-11199 pixel window, and the long run is a soft WARN (default drift 3159, boss pre-window 1357/6503) |
| `scripts/check-targets.sh` | No-OS guard for `gradius-core`, wasm32 build and host build |
| `web/` | Browser demo: `web/build.sh` builds `gradius-wasm` and copies the bundle next to `index.html` |

## Platforms

| platform | state | backend |
| --- | --- | --- |
| Linux, macOS, Windows | Playable full game, stages 1-7 plus the ending and loop restart | `crates/gradius-sdl`, SDL2 window, input and audio over the shared runner |
| Headless (CI, any host) | Supported | `gradius-sdl --frames N` or `gradius-diff`; no window or audio device |
| WebAssembly | Playable core demo in a browser: `gradius-wasm` + `web/`, with canvas video, keyboard input, Web Audio, integer/aspect/stretch fit and an installable offline PWA | `web/build.sh`, `scripts/check-targets.sh` |
| Android, iOS | Not started | planned shells around the same core and runner |

`gradius-core` has no OS dependency: no files, sockets, threads, clock or
global state. `gradius-app` owns the tick clock and backend contract, so a
platform supplies input, a 256x240 RGBA surface and an audio sink and nothing
else. `scripts/check-targets.sh` proves the wasm build and the absence of OS
calls in one command; the backend contract and concrete wasm/Android/iOS steps
are in [`docs/PORTABILITY.md`](PORTABILITY.md).

The browser demo is also an installable offline PWA. Extract the pack to
`web/gradius.pak` with `cargo xtask extract`, run `web/build.sh` to build the
wasm modules, then serve `web/` over HTTP (for example
`python3 -m http.server -d web 8000`). The page's Install button installs it,
and its service worker precaches the wasm, pack and shell files so later visits
work offline. `file://` does not work: the wasm, pack and service worker need
an `http://localhost` or HTTPS origin.

## Development

```sh
cargo test                          # default members: core, pack, assets, cart, oracle, runner, xtask
cargo test -p gradius-app           # runner clock, sinks and MusicDirector
cargo test -p gradius-diff          # harness parsers, comparisons and fixture replay (oracle runs skip without GRADIUS_ROM)
cargo test -p gradius-sdl           # frontend unit + headless tests
scripts/verify fast                 # fmt + cargo check --workspace --all-targets
scripts/verify full                 # clippy + workspace tests + SDL build + pack smoke
scripts/verify changed              # fmt + clippy/test only the crates this branch touched
scripts/check-targets.sh            # core no-OS guard + wasm32 check + host build
```

Tests that need the pack or the fixtures fail, with the command to run, when
they are missing; none is skipped silently. The tests that run the oracle skip
without `GRADIUS_ROM`. `scripts/verify full` needs `libsdl2-dev` and `assets/gradius.pak`; `fast` needs
the SDL development library because the workspace includes the frontend.
`changed` is the per-commit gate for parallel workers: it derives the touched
crates from the merge-base with `main` and runs clippy and tests on those
packages only, falling back to `fast` when nothing changed. Full command
reference, including the `gradius-diff` flags, every `cargo xtask` subcommand
and the worker/worktree flow, is in `docs/DEVELOPMENT.md`.

## Roadmap

1. **Stage-1 polish.** The boss takes damage at frame 8066 and clears at 9087
   with score 2690, and the gated 7500-9700 window is clean, including the
   post-clear slot-9 debris and the stage-2 reset at 9601. Remaining items are
   the soft long-run drift (default 3159, boss pre-window 1357/6503) and the
   241 px sprite-palette residue; the stage-1 tail is estimated at 14 h.
2. **Two-player alternating play. Done.** `TwoPlayer` in `Game` owns `$18`,
   the per-slot lives and BCD score banks, the checkpoint triple and the
   both-out game over; a death runs the 120-frame `$C1DA` animation and rebuilds
   the shared stage from the incoming player's checkpoint. The HUD and the
   `$2384` game-over banner follow `$18` (tile `31` -> `32` via `$980B`), and
   both frontends feed the second pad (SDL keyboard/pad, wasm/PWA keyboard and
   gamepad). `p2-handoff` is a clean 0-divergence row from reset: `$18` 0 -> 1
   at 1194, P2 normal play at 1221. The `$977C` continue code is scanned in the
   game-over window and resumes the player at `$1B = $80`; the per-player `$C0`
   window is not modelled. Design, measurements and the save-state notes are in
   [`docs/analysis/twoplayer.md`](analysis/twoplayer.md).
3. **Stages 2-7 runtime.** All seven stages' spawn, terrain and background
   data is extracted; stages 2 and 6 are closed, and stages 3, 5 and 7 replay
   from recorded state (stage 3 exact from reset over 19429-23000, stage 5
   exact over 297-12003 including the clear, stage 7 exact over 297-6050 plus
   the 6563-7024 clear and the 7025-8290 ending gate). Open runtime work:
   stage 3's object-fire pass, stage 4's spawner tail and stage 5's
   column-384 tail. Plans in
   [`docs/analysis/stage67_plan.md`](analysis/stage67_plan.md).
4. **Packaging.** The Linux AppImage and the Windows zip are built by
   `scripts/package-appimage.sh` and `scripts/package-windows.sh` (see
   [`docs/PACKAGING.md`](PACKAGING.md)). A macOS build and a stable
   installable wasm PWA bundle are still open.
5. **Widescreen / viewport abstraction.** Generalise the fixed 256x240 renderer
   so a backend can present a wider viewport without changing the core rules.
6. **Save states and rewind. Done.** `Game::snapshot`/`restore` over
   `GRSS` v1.0 (CORE 2,524 B + 41 B EXT + `PLAYERS` + OAM tail, ~2.8 KB typical
   in the 8,192 B slot) replays byte-exactly over 600 -> 1,200, 786 -> 1,386 and
   `--stage 5/4 --rewind-at 300`. The 32 B `PLAYERS` chunk carries both player
   banks, the `$18`/`$1B` turn machine and `$33`, so a mid-2P save round-trips;
   an older 31 B chunk is refused, and images without the chunk still load. SDL
   F5/F9 saves/loads `savestate.grs` (atomic sibling `.tmp` + rename, rollback
   on corrupt) and the wasm/PWA frontend saves to
   `localStorage["gradius.savestate.v1"]` (2,989 B smoke, 60/60 frames,
   corrupt/truncated refused). The only caveat is audio: a load restarts the
   current cue from its beginning, not the saved APU position. Format and
   measurements are in
   [`docs/analysis/savestates.md`](analysis/savestates.md) §0.

## License

Dual-licensed under either the MIT License ([`LICENSE-MIT`](../LICENSE-MIT)) or the
Apache License, Version 2.0 ([`LICENSE-APACHE`](../LICENSE-APACHE)), at your option,
matching the `license` field in `Cargo.toml`. Gradius and its original assets are
trademarks and copyrights of Konami. This project reimplements the game's logic
from observed behaviour and does not copy its code; it contains no Konami binary
code, art, music or ROM data.
