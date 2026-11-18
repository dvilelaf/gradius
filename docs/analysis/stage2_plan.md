# Stage 2 implementation plan

Executable plan for `$19 = 1` (stage 2). It starts from the same
stage-1-shaped runtime as the stage-5 plan: the spawner indexes
`SECTIONS[stage]`, but `Game::new` always starts at section 0,
`game.rs::render` draws `stage1_bg::STAGE1`, the boss gate uses the stage-1
literals, and `$C433[$19]` is not modelled. Stage 2's data is already decoded;
the work is selection, the background/tail rule, music `$59`, the shared core
gate plus the stage-2 `$39` nuance, the `$C540` script and the differential
harness.

The stage-5 counterpart is `stage5_plan.md`. Stage 2 is the cheap stage: no
command records, no new AI handler, and the same boss threshold/spawn/end as
stage 1.

## Stage-2 facts to build against

From `cargo xtask stages`, `cargo xtask sections` and
`cargo xtask stage-enemies` (checked 2026-11-09):

| field | stage 2 value |
| --- | --- |
| `$19` | 1 |
| spawn root `$A7CA[1]` | `$A7E8`, 8 columns |
| `SECTIONS[1]` | 133 records, all two-byte `<delay> <type>`; 0 commands |
| `$A65C` terrain | 101 placements (`stage1_collision::terrain(1)`) |
| background | `stage2_bg::STAGE2`, 480x28 (3840 px), dominant CHR bank 0 |
| tail | from world column 416 (3328 px): CHR bank 2, palettes 2-3 black |
| palette | the 32-byte `$3F00` page (four background and four sprite palettes), read from the cartridge at run time (`gradius-extract`) |
| tiles / attrs / palette hashes | `F207532E288F4003` / `6988DE65EF5F569F` / `DF2BBDFA95164A70` |
| music `$8344` | `$59` (`Music::Stage2`) |
| boundary `$8354` / mode `$834B` | 4 / 0 |
| `$98F0` end | `$0E` |
| `$9A30` boss threshold | `$0C` (spawn at `$3F = $0D`) |
| start row `$9BC6` | `$05` |
| script `$C433[1]` | `$C540` |
| boss | shared Big Core `$98`, 7 hits, +1000, 514-frame clear |

Stage 2's 43 raw types resolve to kinds `$04` (10), `$05` (10), `$08` (7),
`$0B` (5), `$0F` (7), `$10` (2), `$11` (55), `$12` (35), `$27` (1), `$29` (1).
Every one already has a `game.rs` match arm. There is no stage-2-only AI
handler; `$0F`/`$10` are the only *partial* arms (see risks).

Measured with `cargo xtask record-boss "$ROM" --stage 2 --frames 12000`:

```
spawn 7979, scroll stop 7978, x stop 8054, first damage 8544
overheat wraps 8314 / 8574 / 8833 / 9093
death 9280 (+1000), stage 3 from 9794
```

The advance is 514 frames after the core death, the same shared clear timer as
stage 1. The spawn frame 7979 is the landmark to reproduce; it is **not**
stage 1's 7723.

## Current code anchors

| gap | current code |
| --- | --- |
| stage entry | `Game::new` hardcodes `section = 0`, `Spawner::new(0, 0)`; only `Game::section()` exists |
| spawner | already indexes `SECTIONS[section]`; `SECTIONS[1]` works if `section = 1` |
| terrain | `SECTIONS[1]` resolves its own `$A65C` records; `terrain(1)` is placement evidence, while ship-vs-map collision is always on and reads the packed `COLLISION_MAP` |
| duplicate data | `stage2_tables` duplicates `SECTIONS[1]` / `terrain(1)` and its tests assert equality |
| background | `game.rs::render` uses `stage1_bg::STAGE1`; `draw_screen` has no tail argument |
| boss gate | `normal_frame` uses `0x0C`, spawn wait `0x0D`, `boss_timer = 771` |
| `$39` | only the stage-1 volcano path sets `stage_clear_ready`; no stage-2 `$B95C` condition |
| stage end | `boss_clear == 0 && (section != 0 || stage_clear_ready)` |
| script | `$C40D`/`$C424`/`$C433` absent; `$A2BA` has no `$82` hook |
| music | `gradius-app::music` emits only `Stage1`/`Stage1B`; `Music::Stage2` exists in `gradius-audio` |
| harness | `gradius-diff` has no `--stage`; native `Game` has no stage entry |

`stage_bgs.rs` contains `STAGE3`..`STAGE7`; stage 2's screen is the odd one out
in `stage2_bg.rs`. Keep it there for this plan (folding it into `stage_bgs` is
cosmetic and would rewrite generated data). `stage2_tables` is also redundant
with `SECTIONS[1]`; keep it as an equality test, do not wire it as a second
source.

## Implementation steps

1. **Stage record and stage-2 entry.** Add the stage data record from
   `stages.md` (shared with stage-5 S5-00 if that lands first) and a row for
   `$19 = 1`: background `stage2_bg::STAGE2`, music `$59`, boundary 4, mode 0,
   boss `$0C`, end `$0E`, start row `$05`, tail column 416. Add a public
   constructor/entry (`Game::new_at_stage(assets, 1)` or
   `Game::enter_stage(1)`) that sets `section = 1`,
   `spawner = Spawner::new(1, 0)` and
   `place_player(stage::ship_start(1, 0))`. Do not add per-stage function
   pointers: the game branches on `$19` inline, like the original.
2. **STAGE2 and the column-416 tail.** Select `STAGES[section].background`
   instead of `stage1_bg::STAGE1`. Add a renderer tail rule (the `Screen` only
   stores the dominant state): for stage 2, when the displayed world position
   reaches column 416 (3328 px), use `chr_bank = 2` (pack bank 4) and force
   palette RAM entries 8-15 to `0F`; palettes 0-1 keep the extracted values.
   The final 512 px (columns 416-479) are starfield and contain no terrain.
   Preserve stage 1's `STAGE1` path exactly.
3. **Shared core gate, `$39` and the `$82` window.** Use
   `STAGES[section].boss_threshold` (`$9A30`) instead of `0x0C`/`0x0D`, and
   keep `$98F0` as the normal `$98F7` end check (the core clear advances by
   the 514-frame timer, not by `$3F = $0E`). Keep the
   `$81 -> $82 -> $83 -> $84 -> $85` sequence: `$82` lasts `$9A28[rank]`
   (3 for a single player), then `$83`/`$84` wait for `$3F = $9A30 + 1`; the
   whole `$81-$84` window is `stage::transition_frames(1)` = 6. Do **not**
   inherit the stage-1 771-frame timer: that is the measured `$8A` eruption
   hold, and stage 2 has no eruption objects. Verify the stage-2 threshold to
   spawn against `record-boss --stage 2` (7979); if a hold exists, find its
   stage-2 `$5B` setter instead of copying 771.
   Implement `$B95C` on the damage kill only:
   `if section == 1 && object.anim_div == 1 && object.anim < 0x78 { stage_clear_ready = true; }`
   (`anim` is `$04AC`, `anim_div` is `$04CC`). The overheat path `$BA96 ->
   $B97D` skips `$B95C`, so `boss_self_destruct` must not set the flag. Make
   the end gate require `$39` for sections 0 and 1
   (`section > 1 || stage_clear_ready`), and keep +1000/514 shared.
4. **Music `$59`.** Expose the stage and its `music`/`music_boundary` from
   `Game`; add `MusicCue::Stage2` in `gradius-app` and map it in the SDL and
   WASM backends. Stage 2 plays the shared intro until boundary 4
   (`$3F >= 4`, `0x0400`), then `$59`; boss `$A5`, clear `$AC`, game over
   `$AF`. If the shared `Intro`/`$93` cue is added for stage 5, use it here;
   otherwise keep stage 1's cue byte-identical and start stage 2 at `$59`.
   Do not reuse `BOSS_SCROLL = 2820`: stage 2's boss is at `$9A30 = $0C`
   (3072 px).
5. **`$C40D` / `$C540` hazards.** `$A2BA` jumps to `$C40D` only when
   `$1B == $82` or `$3A != 0`. `$C40D` runs every 4th frame (`$02 & 3 == 0`),
   scans slots 9 down to 0, clears the free slot, then:
   `if $3A != 0 -> $C680` (stage-3 hook only; do not implement `$C680`), else
   dispatch `$C433[$19]`.
   Stage 2's handler `$C540` runs every 8th frame (`$02 & 7 == 0`), advances
   `$69`, takes the next nibble from the 64-entry `$C587` ping-pong table
   (`0..15, 15..0, 0..15, 15..0`), doubles it into the 16-entry `$C567` table
   of (x, y) pairs below, and spawns kind `$0B` with sprite `$67` in the free
   slot. The sixteen positions are read from the cartridge; they walk the ring
   enemy across the screen, high-left first and bottom-right last.
   The existing `ai_launcher` (`$B379`) takes over the animation and aiming;
   do not add a new AI arm. There are no five-byte commands and no `$14`
   object in stage 2.
6. **Harness and evidence.** Add `--stage N` to `gradius-diff`, start the
   native `Game` at the same stage as the oracle's `$26` poke, and run the
   state/pixel/boss windows below. Until this exists, native stage-2 evidence
   is unit tests only and oracle evidence is `record-boss --stage 2`.

## Card breakdown

| card | scope | owned files | acceptance | deps | parallel? | h |
| --- | --- | --- | --- | --- | --- | ---: |
| S2-00 | **stage-2 boot slice**: stage record/entry, `SECTIONS[1]`/`terrain(1)`, `STAGE2` + column-416 tail | `crates/gradius-core/src/stage.rs`, `game.rs` (tests in `stage2_bg.rs` unchanged) | `new_at_stage(1)` starts `$19=1`; spawner consumes `SECTIONS[1]` (133 records, 101 terrain); `STAGE2` renders 480x28; at 3328 px bank 2 and palette entries 8-15 `0F`; reaches the shared core at `$0C/$0D` and advances; stage 1 render/hash unchanged | none | no (`game.rs`) | 8 |
| S2-01 | shared core gate, `$82` window, stage-2 `$39` (`$B95C`) | `crates/gradius-core/src/stage.rs`, `game.rs` | `$39` set only for a damage kill with `anim_div==1 && anim<0x78`; self-destruct skips; `$82` is 3 frames (6-frame `$81-$84` window); stage 2 spawns at `$0D` and advances 514 frames after death (9794); stage 1 state hash unchanged | S2-00 | no (`game.rs`) | 5 |
| S2-02 | music `$59`, boundary 4, backend mapping | `crates/gradius-app/src/music.rs`, `crates/gradius-sdl/src/music.rs`, `crates/gradius-wasm/src/lib.rs` | stage 2 selects `Stage2` at `0x0400`, then `Boss`/`StageClear`/`GameOver`; stage 1 cue list unchanged | S2-00 | **yes** | 2 |
| S2-03 | `$C40D` / `$C540` script and `$3A -> $C680` hook | `game.rs` | on `$82` frames only, free-slot scan 9..0; every 8th frame spawns kind `$0B`, sprite `$67` at the next `$C567` point; `$3A != 0` goes to `$C680`; non-`$82` frames emit nothing; stage 1 unchanged | S2-01 | no (`game.rs`) | 4 |
| S2-04 | `--stage 2` differential/pixel/boss harness + evidence run | `crates/gradius-diff/src/main.rs`, `game.rs` stage hook | native and oracle both start at stage 2; state/boss window 0 divergences; pixel playfield and HUD >= 99.9%; landmarks 7979 / 9280 / 9794 | S2-00, S2-01, S2-02, S2-03 | no (green last) | 5 |

Total: **24 h** (about three working days). Critical path is
`S2-00 -> S2-01 -> S2-03 -> S2-04` (~22 h); S2-02 is the only fully parallel
card. S2-04's `--stage` plumbing can start after S2-00, but its green run is
last.

**Smallest first card that makes stage 2 playable end-to-end: S2-00.** It
boots `$19 = 1`, consumes `SECTIONS[1]`/`terrain(1)`, renders `STAGE2` with
its tail, and uses the existing shared core gate; stage 2's `$9A30 = $0C`,
spawn `$0D` and `$98F0 = $0E` already match stage 1, so the stage reaches the
core and advances before the bit-exact `$39`/`$82` work in S2-01. It is not
oracle-exact until S2-01 and S2-04.

### Parallel / conflict rules

* **Every card except S2-02 touches `game.rs`.** Do not run two of them in the
  same worktree. S2-01 and S2-03 both edit `normal_frame` and the stage
  machine, so sequence them; S2-00 and S2-04 both add the stage entry/hook, so
  rebase in that order.
* S2-02 is the only frontend/music-only card and can run from the start.
* S2-04 starts with S2-00's `--stage` plumbing but its green run is the last
  thing on the branch.
* If stage-5 S5-00 lands first, S2-00's stage record is a data row and its
  estimate drops to ~5 h; if S2-00 lands first, stage 5 reuses it. Coordinate
  through `stage.rs`.

## Risks and exact evidence

1. **The 771-frame timer is stage-1-shaped.** `boss_timer = 771` models the
   measured `$8A` eruption hold, not `$9A28` (3-6 frames). Stage 2 has no
   eruption; use `transition_frames(1)`/`$9A28` and the scroll wait to `$0D`,
   and check the 7979 spawn. Do not make stage 2 wait 771 frames.
2. **`$39` is read from the core object.** `$04AC` is the port's `anim`,
   `$04CC` is `anim_div`; the stage-2 gate is `section == 1 &&
   anim_div == 1 && anim < 0x78`. The overheat path jumps to `$B97D`, so it
   must not set the flag. A wrong gate blocks the 9794 advance.
3. **Tail palette is not in `Screen`.** `stage2_bg::STAGE2` stores the
   dominant bank 0 and palette. The column-416 switch (bank 2, palette 2-3
   `0F`) must be a renderer rule. Do not regenerate or hand-edit the RLE.
4. **Map collision is always on; `terrain(1)` is placement evidence only.**
   `Mods::map_terrain_collision` was removed and `$C29F` always calls `$C39D`
   against the packed `$0500`/`$0600` map (`stage1_collision::COLLISION_MAP`).
   The map is zero only at the start and first loads at frame 1579; natural
   runs then die on the floor/ceiling (stage 1: 2169/2745; stage 2 with
   `$26 = 1` poked at 263: 2553/2617). `SECTIONS[1]` already spawns its own
   `$A65C` objects, so do not add a second hitbox or a stage-2 solid table.
   Verify the packed map stays on with those floor/ceiling kills; see
   `powerup_terrain_verification.md`.
5. **`$C40D` is not a normal-frame hook.** It is reached from `$A2BA` only on
   `$1B == $82` frames or when `$3A != 0`; `$C424` sends `$3A != 0` to
   `$C680` regardless of `$19`. Running `$C540` every frame would spawn
   launchers continuously and desynchronise the stage. Preserve the
   `$3A -> $C680` quirk as a hook; stage 3's `$C680` body is out of scope.
6. **`stage2_tables` duplicates `SECTIONS[1]`.** Its tests already assert
   equality; use `SECTIONS[1]`/`terrain(1)` as the runtime source and keep the
   module only as decoded evidence. Do not wire both.
7. **`$0F`/`$10` are partial.** Stage 2 has 9 of them. Their `$046C >= 5`
   death and `$5F` counter are not ported; the stage can be completed without
   them (the `$39` gate is the core, not these kills). If S2-04's diff flags
   them, add the shared `$046C >= 5` death as a 3 h add-on; do not add the
   stage-1 `$39` increment for stage 2.
8. **Music boundary and boss scroll.** Stage 2 switches at boundary 4
   (`0x0400`), not 0x0B04; the boss is at `$9A30 = $0C` (3072 px). Expose the
   per-stage values rather than keeping the stage-1 constants.

## Evidence commands

Use the card target directory everywhere:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stage2plan
```

Data and roots:

```sh
cargo xtask stages "$ROM" | grep '^    2'
cargo xtask sections "$ROM" | grep -E 'S1_TERRAIN|S2_TERRAIN'
cargo xtask stage-enemies "$ROM" | grep -E '^\$0B|^\$11|^\$12'
cargo xtask stage2-tables "$ROM" > /tmp/opencode/st2-tables.rs
```

Handler, script and `$39` disassembly:

```sh
cargo xtask disasm "$ROM" --from C40D --to C5A7 --labels
cargo xtask disasm "$ROM" --from C567 --to C5A7 --labels   # path tables
cargo xtask disasm "$ROM" --from B95C --to B990 --labels   # $39 gate
cargo xtask disasm "$ROM" --from BA8B --to BA9A --labels   # overheat skips $B95C
cargo xtask disasm "$ROM" --from A2BA --to A300 --labels   # $82 hook
cargo xtask disasm "$ROM" --from B379 --to B3FC --labels   # kind $0B
```

Stage-2 oracle and RAM:

```sh
cargo xtask record-boss "$ROM" --stage 2 --frames 12000 --out /tmp/opencode/boss2.txt
cargo xtask probe "$ROM" --frame 500 --press 180:start \
  --poke 263:26:01 --keep-alive \
  --ram /tmp/opencode/st2.ram --oam /tmp/opencode/st2.oam --png /tmp/opencode/st2.png
```

Expected `record-boss` summary: `spawn 7979 ... end 9280 ... stage 3 from
9794`. The frame-500 probe shows `$19 = 1`, `$00 = 5`, and the stage-2 palette.

## Verification windows

After S2-04 adds `--stage 2`:

```sh
PACK=/tmp/opencode/stage2.pak
cargo xtask extract "$ROM" "$PACK"
flock /tmp/verify-heavy.lock cargo build --release -p gradius-diff

# state / boss window: threshold through the next stage
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage2plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 10000 --stage 2 \
  --from 7800 --to 9900 --keep-alive --native-keep-alive \
  --report --json /tmp/opencode/stage2-boss.json

# pixel window: column-416 tail, then core entry
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage2plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 10000 --stage 2 \
  --pixel-report --pixel-window 3200:3800 --png-dir /tmp/opencode/stage2-png \
  --png-every 30 --json /tmp/opencode/stage2-pixel.json
```

Pass criteria: the state/boss window reports `WITHIN TOLERANCE (0
divergences)`; pixel playfield and HUD are each `>= 99.9%`; tail samples at
and after 3328 px show `chr_bank 2` and palette entries 8-15 = `0F`; stage 2
exits to `$19 = 2` at the measured 9794 frame (± the harness script). Run
`scripts/verify changed` in each code worktree; do not run the full workspace
suite there.

## Definition of done / out of scope

Done when a stage-2 run starts at `$19 = 1`, consumes `SECTIONS[1]`/101
terrain placements, renders `STAGE2` plus the column-416 bank-2/black tail,
plays `$59` after boundary 4, runs `$C540`, reaches the shared Big Core at
`$0C`/spawn `$0D`, sets `$39` by the `$B95C` condition, awards +1000, and
advances at 9794 without changing stage 1.

Out of scope: stage 3's `$C680` body (only the `$3A` hook), stage 5's
`$14`/`$A4A0`/`$0600` work, the full `$0F`/`$10` kill gate unless the diff
flags it, and the `$8A` eruption path (stage 1 only).

## Why this is 24 h and stage 5 is 50 h

The 26 h difference is stage 5's new mechanics and data volume:

* `$14`/`$CA58` (10 h) and `$A4A0`/`$0600` cannons (8 h): stage 2 has no
  command records and no new handler.
* `$19 == 4` exception chain (6 h): stage 2 has only the `$B95C` `$39`
  nuance and the `$C540` script.
* Stage 2's background and `Music::Stage2` are already extracted; stage 5's
  background/tail and music still need the same wiring but with a 448x28
  screen, a column-384 tail and boundary 2.
* The shared `Stage` record, core gate and `--stage` harness are common
  infrastructure; stage 2 pays for them once (S2-00/S2-01/S2-04) and stage 5
  reuses them. If stage 5's S5-00 lands first, stage 2's incremental cost is
  about 19 h; if stage 2 lands first, stage 5 is the one that reuses the
  record and harness.
