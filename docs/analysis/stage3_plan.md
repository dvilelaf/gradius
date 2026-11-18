# Stage 3 implementation plan

Executable plan for `$19 = 2` (stage 3), the stage with the extra
background-tile mini-boss. It starts from the same stage-1-shaped runtime as
`stage2_plan.md` and `stage5_plan.md`: `Game::new` always starts at section 0,
`game.rs::render` draws `stage1_bg::STAGE1`, the boss gate uses the stage-1
literals, and `$C433[$19]` is not modelled. Stage 3's streams, terrain and
screen are already decoded; the work is selection, the background/tail rule,
music `$5B`, the stage-3 command branch, `$C900` (kind `$16`) and its nametable
overlay, the `$C680` bonus wave, the shared core gate and the harness.

Stage 3 is the only stage that spawns a non-core boss-class object: the 53
five-byte command records create kind `$96` (`$C900`), a 3-hit, 300-point
background turret with no OAM sprite and no projectile. Ten of them set `$39`,
which selects the `$8E`/`$9842` clear path (+5000, scroll to `$3F = $11`) and
increments `$3A`, which in turn enables the `$C680` bonus wave. The end-of-stage
boss is still the shared Big Core `$98`.

## Stage-3 facts to build against

From `cargo xtask stages`, `cargo xtask sections` and
`cargo xtask stage-enemies` (checked 2026-11-09):

| field | stage 3 value |
| --- | --- |
| `$19` | 2 |
| spawn root `$A7CA[2]` | `$A7F8`, 7 columns |
| `SECTIONS[2]` | 87 records: 53 five-byte commands, 34 two-byte spawns |
| command distribution | C0 0, C1 3, C2 11, C3 12, C4 11, C5 8, C6 8 |
| `$A65C` terrain | 18 placements (`stage1_collision::terrain(2)`) |
| background | `stage_bgs::STAGE3`, 480x28 (3840 px), dominant CHR bank 1 |
| tail | from world column 416 (3328 px): CHR bank 2, palettes 2-3 black |
| palette | the 32-byte `$3F00` page (four background and four sprite palettes), read from the cartridge at run time (`gradius-extract`) |
| tiles / attrs / palette hashes | `D8202719436CC2F9` / `A48987175FAD0477` / `B6BDFDD279505805` |
| music `$8344` | `$5B` (`Music::Stage3`) |
| boundary `$8354` / mode `$834B`/`$2D` | 4 / 2 |
| `$98F0` end | `$0E` |
| `$9A30` boss threshold | `$0C` (spawn at `$3F = $0D`) |
| `$9A28` clear countdown | 4; the rank-2 `$82` hold is `$9A28[$17] * 256` = 1024 frames |
| start row `$9BC6` | `$0A` |
| script `$C433[2]` | `$C680` |
| boss | shared Big Core `$98`, 7 hits, +1000, 514-frame clear on `$39 = 0`; the `$39` route runs `$8E` first |
| mini-boss | kind `$96` (`$16` after the `$AE13` mask), `$C900`, HP 3, 300 points, ten kills set `$39` |

The 34 two-byte records contain the 18 terrain placements and 16 formation
members. Raw stage types resolve to kinds `$04` (4), `$05` (4), `$08` (4),
`$0D` (1), `$0E` (1), `$11` (11), `$12` (4), `$16` (53), `$17` (1), `$1C` (2),
`$27` (1), `$29` (1). Every kind except `$16` already has a `game.rs` arm;
`$17`/`$1C` are the small core and descender already implemented.

Measured with `cargo xtask record-boss "$ROM" --stage 3 --frames 12000`:

```
spawn 7979, scroll stop 7978, x stop 8054, first damage 8390
overheat wraps 8314 / 8574 / 8833 / 9093
death 9126 (+1000), stage 4 from 9639
```

That is the **normal** clear path: the core death plus the shared 514-frame
clear timer and the `$98F0 = $0E` scroll end. The 9639 advance is not evidence
that ten `$16` kills happened; it is the `$39 == 0` path (`$9928` stores `$90`,
then `$96C4` does `INC $19`). The `$39 != 0` path runs `$8E`, awards +5000, and
then takes the second `INC $19` to stage 5 instead of stage 4.

## Current code anchors

| gap | current code |
| --- | --- |
| stage entry | `Game::new` hardcodes `section = 0`, `Spawner::new(0, 0)`; only `Game::section()` exists |
| spawner | already indexes `SECTIONS[section]`; `SECTIONS[2]` works if `section = 2`, including its 53 `Emission::Command` records |
| terrain | `terrain(2)` is placement evidence (`$A65C`); ship-vs-map collision is always on and reads the packed `COLLISION_MAP` |
| background | `game.rs::render` uses `stage1_bg::STAGE1`; `draw_screen` has no tail argument |
| command `$A460` | `command()` treats every `section == 2` command as the Big Core: kind `$96`, `sprite 0x89`, `size_class 1`, `boss: true`, and sets `boss_spawned = true` |
| command `$A4A0` | non-stage-3 branch uses `bytes[2] & 0x0F`, stores it in `ai_mode`, and marks the object `boss: true` |
| kind `$16` | `update_objects` routes `0x16` to `ai_big_core` |
| nametable | no live nametable/VRAM queue; `Screen` is a static RLE map |
| script | `$C40D`/`$C424`/`$C433` absent; no `$3A` model |
| `$39` | only the stage-1 volcano path sets `stage_clear_ready`; no `$C776` ten-kill counter |
| stage end | `boss_clear == 0 && (section != 0 || stage_clear_ready)`; the `$9928` `$39` branch is not modelled |
| boss gate | `normal_frame` uses `0x0C`, spawn wait `0x0D`, `boss_timer = 771` |
| music | `gradius-app::music` emits only `Stage1`/`Stage1B`; `Music::Stage3` exists in `gradius-audio` |
| harness | `gradius-diff` has no `--stage`; native `Game` has no stage entry |

`stage_bgs::STAGE3` and its pinned hashes are generated data; do not edit the
RLE or hand-maintain a second stage-3 screen.

## Implementation steps

1. **Stage record and stage-3 entry.** Add the shared `stage::Stage` record and
   the row for `$19 = 2`: background `stage_bgs::STAGE3`, music `$5B`,
   boundary 4, palette mode 2, boss threshold `$0C`, end `$0E`, start row
   `$0A`, tail column 416, tail bank 2, script `$C680`. Add
   `Game::new_at_stage(assets, 2)` (or `Game::enter_stage(2)`) that sets
   `section = 2`, `spawner = Spawner::new(2, 0)` and
   `place_player(stage::ship_start(2, 0))`. Do not add per-stage function
   pointers: the game branches on `$19` inline, like the original. If the
   stage-2 or stage-5 plan lands first, add this as a data row and reuse its
   constructor.
2. **`SECTIONS[2]` and `terrain(2)`.** Use the existing spawner row; it already
   emits `Emission::Command` for the 53 five-byte records and `SpawnEvent` for
   the 18 terrain pieces and 16 formation members. Column 7 is empty and is
   never selected before the `$0C` threshold. Do not add a
   `stage3_tables` duplicate; `SECTIONS[2]`/`terrain(2)` are the runtime source.
3. **`STAGE3` and the column-416 tail.** Select `STAGES[section].background`
   instead of `stage1_bg::STAGE1`. Add one renderer tail rule: when the
   displayed world position reaches column 416 (3328 px), use CHR bank 2 (pack
   bank 4) and force palette RAM entries 8-15 to `0F`; entries 0-7 keep the
   extracted dominant values. The final 512 px are starfield and contain no
   terrain. `STAGE3` stores the dominant bank 1; the header's several
   `from column 0` entries are stage-init palette/bank uploads, not runtime
   transitions. Preserve stage 1 and stage 2 exactly.
4. **Music `$5B`.** Expose the stage and its `music`/`music_boundary` from
   `Game`; add `MusicCue::Stage3` in `gradius-app` and map it in the SDL and
   WASM backends. Stage 3 plays the shared intro (`$93`) until boundary 4
   (`$3F >= 4`, `0x0400`), then `$5B`; boss `$A5`, clear `$AC`, game over `$AF`.
   Do not reuse `BOSS_SCROLL = 2820`: stage 3's boss is at `$9A30 = $0C`
   (3072 px). If the stage-2/5 shared intro cue exists, use it; otherwise keep
   stage 1's cue list byte-identical.
5. **Five-byte command path (`$A460`).** Fix both branches and drop the
   port inventions.
   * `$19 == 2` (`$A469`): scan slots 9 down to 0 for a free `$030C`, set
     `$5D = 1`, clear the slot, then write `$010C = $64` (type minus `$70`,
     `$80-$83`), `$032C = $65` (Y), `$03BC = $66`, `$03EC = $67`,
     `$030C = $96`, `$036C = $F0`. Do **not** set `$012C` (sprite), `$0460`
     (size class) or a boss flag; do not set `boss_spawned`. Store the
     `$03BC:$03EC` pair separately from the object's velocities.
   * otherwise (`$A4A0`): `low = bytes[2] & 0x0F`, `high = bytes[2] >> 4`,
     `records = (low != 0) + (high != 0)`; store the count in `$016C`, not
     `ai_mode`, and allocate one `$0600` record per non-zero nibble, low first,
     with parent slot and `$0601 = nibble - 1`. Keep the real `$A4A0` fields
     (`kind = bytes[3]`, `$010C = $80`, x `$F0`, y `$67`, sprite `$89`,
     `$0460 = 1`), but remove the invented `boss: true`.
   * The existing `a_section_two_command_spawns_the_big_core` test asserts the
     old, wrong behaviour; replace it with the `$A469` turret fields and add
     the two-nibble cases (`[40 F0 12 14 40]` -> 2 records,
     `[40 F0 01 14 80]` -> 1).
6. **`$96`/`$C900` mini-boss and the nametable overlay.** Add
   `22 => ai_background_turret` instead of `ai_big_core`. Implement
   `$C900` from `handlers_14_16.md`:
   * `type = subtype & 0x0F`; `$046C >= 3` -> `$C776` death.
   * Drift `$AED7` (x -= 0.5 px/frame, despawn at x < 8). Return while the
     frame-scoped `$5D` busy flag is set or the VRAM queue `$0E >= 4`.
   * While `$04AC != 0`, decrement and return. Cooldown is
     `$C930[rank] = 80,75,70,65,60,40,30`.
   * Closed state (`$048C == 0`): run the per-type half-plane sector test.
     In-sector -> `$C999`: `$5D += 1`, `$040C = 0`, `$048C = $14`,
     `$04EC = $14`, `$04AC = $C930[rank]`, paint the closed group `4*type`.
     Open state (`$048C != 0`): `$048C = 0`, `$046C = 0`, `$04EC = 0`,
     `$04AC = $C930[rank]`, paint the open group `4*type + $10`.
   * Paint groups (`$CA23`, 4 bytes each): `$80` closed `00 8F` + `90`, open
     `20 21` + `22`; `$81` `D9 DA`, open `2E 2F`; `$82` `B0 00` + `B1`,
     open `24 25` + `27`; `$83` `CF 7C`, open `28 29`. A group's third byte
     zero means a second record at `$03BC:($03EC + $20)` (low byte carry into
     high). Type 2 writes the first record's low address as `$03EC - 1`.
   * Death `$C776`: `$5F += 1`; at `$5F >= 10` set `$39 = 1`; `$5D += 1`;
     score 300; `$C81C` erases attribute bytes (`$0F` then `00` at the
     `$C875` offsets, type-2 offset adjusted by `$29`); `$C7B0` writes the
     `$C88D` explosion records; play `$0C`, then become kind-2 debris.
   * No OAM: `$012C = 0` for its whole life; `build_sprites` must skip kind
     `$96`. Collision is possible only while `$048C != 0`; entering open state
     clears `$046C`, so damage never accumulates across an animation cycle.
   * The overlay must be world-fixed, not screen-fixed: store tile/attribute
     patches keyed by world tile column/row, computed from the object's screen
     x plus the live scroll at paint time (the object drifts 0.5 px/frame to
     compensate the scroll). Apply patches after `draw_screen`, before the HUD.
     Model the `$0E` queue occupancy as a frame counter so the `$0E >= 4`
     throttle is reproduced even though the port paints immediately.
   * `$5D` is not a free-running counter in the oracle (`st3_2000.ram` has
     `$5D = 0` with live turrets); model it as the frame-scoped spawn/effect
     busy flag and verify its lifetime against the 1984/1992/1999/2002 trace.
7. **`$C40D`/`$C424`/`$C433[2] = $C680` and the `$3A` bonus wave.** Implement
   `$A2BA`'s hook: on `$3A != 0`, or on `$1B == $82`, call `$C40D` every 4th
   frame (`$02 & 3 == 0`). `$C40D` scans slots 9 down to 0, `$A521`-clears the
   free slot, then routes `$3A != 0` to `$C680`; otherwise it dispatches
   `$C433[$19]` (stage 3 also `$C680`). `$C680` increments `$68`, and when
   `$68 >= $C67E[$3A]` (`$28`/`$0A`) and `$3F < 0x0E`, resets `$68`, stores
   y `$C6C8[$69 & 0x0F]`, `$0460 = 1`, sprite `$C6C4[$3A]` (`$3F`/`$00`),
   kind `$C6C6[$3A]` (`$97`/`$A6` -> dispatch `$17`/`$26`), subtype `$80`,
   x `$F0`.
   * In normal play the `$82` window is only `$9A28[2] = 4` frames, so `$68`
     never reaches `$28`: `$C680`'s `$3A == 0` small-core branch is retained
     for fidelity but is not observed in stage 3. The real stage-3 use is the
     `$3A == 1` bonus wave after a ten-kill `$39` clear, spawning kind `$26`
     every 10 calls while the bonus scroll runs from `$3F = 0` to `$0E`.
   * Preserve the global `$3A != 0 -> $C680` quirk for every stage and stage
     7's bare `RTS`. If stage 2's `$C40D`/`$C540` card landed first, extend it
     rather than duplicating the dispatcher.
8. **Shared core and the `$39` clear branch.** Use
   `STAGES[section].boss_threshold`/`end_threshold` instead of the `0x0C`/
   `0x0D` literals. Keep `$81 -> $82 -> $83 -> $84 -> $85`; stage 3's `$82`
   hold is `$9A28[$17] * 256` = 1024 frames at rank 2, **not** the stage-1
   771-frame eruption hold and not `transition_frames(2)` (that 7-frame value
   belongs to the mid-stage section transition, `$9A28[2] + 3`). Spawn the
   shared core at `$3F = $0D`, kill it at 7 hits, award +1000, and advance 514
   frames later on `$39 = 0` (`9639`).
   * Implement `$9928`: at `$3F >= $98F0` and `($1B & $70) == 0`, set
     `$1B = $90`; if `$39 == 0`, fall through to `$96C4` (`INC $19`). If
     `$39 != 0`, `INC $19`, `INC $3A`, `$3F = 0`, `$1B = $8E`; `$9842` then
     sets palette mode 1, scrolls to `$3F = $11`, awards +5000 once, and
     returns to `$90`. `$96C4` then does the second `INC $19`, so the `$39`
     route advances to stage 5 (record-boss 10607/10608), not stage 4.
   * In the port, `stage_clear_ready` selects this `$39` branch; it must not
     *block* stage 3's normal advance. Keep stage-1/2 behaviour exactly as
     their plans define it and add a stage-3 `$39` test that pokes the flag
     (or kills ten turrets) and checks the later, +5000 path.
9. **Stage-3 exceptions.** Keep them small and local:
   * `$834B`/`$2D = 2`: stage 3 takes the mode-2 palette/attribute upload
     path. `STAGE3.palette` is already the extracted result; verify the tail
     and HUD swatches in the pixel window. `$9F4D`'s attribute skip is
     `$19 == 4` only and does not apply.
   * `$AFF8`, `$BBB7`, `$BBD8`, `$BC42`: stage 3 takes the `$19 != 0` /
     `$19 >= 2` branches. The port already has several `section != 0` /
     `section != 4` guards; audit them, add tests, and do not refactor shared
     collision or AI code.
   * `$C776`'s `$5F` is the same byte as stage 1's volcano counter. Make the
     counter persistent across death/retry (cleared only on stage advance) and
     do not reuse the stage-1 `volcano_kills` reset semantics for stage 3.
10. **Harness and evidence.** Add `--stage N` to `gradius-diff`, start the
    native `Game` at the same stage as the oracle's `$26` poke, and run the
    state/pixel/boss windows below. Until this exists, native stage-3 evidence
    is unit tests only and oracle evidence is `record-boss --stage 3` plus the
    frame-2000 probe.

## Card breakdown

| card | scope | owned files | acceptance | deps | parallel? | h |
| --- | --- | --- | --- | --- | --- | ---: |
| S3-00 | **stage-3 boot slice**: stage record/entry, `SECTIONS[2]`/`terrain(2)`, `STAGE3` + column-416 tail, command path (`$A469` + two-nibble) | `crates/gradius-core/src/stage.rs`, `game.rs` | `new_at_stage(2)` starts `$19 = 2`; 87 records / 53 commands / 18 terrain; `STAGE3` renders 480x28; at 3328 px bank 2 and palette entries 8-15 `0F`; stage-3 command is kind `$96` x `$F0` with `$010C/$032C/$03BC/$03EC`, no sprite/size/boss; non-stage-3 two-nibble rule; reaches the shared core and advances; stages 1/2 hashes unchanged | none | no (`game.rs`) | 11 |
| S3-01 | music `$5B`, boundary 4, backend mapping | `crates/gradius-app/src/music.rs`, `crates/gradius-sdl/src/music.rs`, `crates/gradius-wasm/src/lib.rs` | stage 3 selects `Stage3` at `0x0400`, then `Boss`/`StageClear`/`GameOver`; stage 1/2 cue lists unchanged | S3-00 | **yes** | 2 |
| S3-02 | `$96`/`$C900` background turret, nametable overlay, ten-kill `$39` | `game.rs`, new `crates/gradius-core/src/stage3_enemies.rs` data | 53 turrets spawn/drift with no OAM; the four `$CA23` groups paint/erase/explode (`$C81C`/`$C7B0`); type-2 `-1`; half-plane aim; HP 3 / 300; `$5F` persists, tenth sets `$39`; `$5D`/`$0E` gates; frame 1984/1992/1999/2002 tiles match | S3-00 | no (`game.rs`) | 14 |
| S3-03 | `$C40D`/`$C424`/`$C433[2] = $C680` and `$3A` bonus wave | `game.rs` | every 4th frame; free-slot scan 9..0; `$3A != 0` -> `$C680`; `$68` thresholds; `$C6C8`/`$C6C4`/`$C6C6` fields; `$3A == 1` spawns kind `$26`; stage 7 RTS; other stages unchanged | S3-00, S3-02, S3-04 | no (`game.rs`) | 3 |
| S3-04 | shared core gate, stage-3 `$82`, `$39`/`$992E`/`$9842`, stage-3 exceptions | `stage.rs`, `game.rs` | spawn 7979 / death 9126 / `$39 = 0` advance 9639; `$82` hold `$9A28[$17] * 256` = 1024 frames at rank 2 (not 7); `$39` selects `$8E`/`$9842` +5000 and `$3A`, then the second `INC $19` reaches stage 5 at 10608; normal `$98F0` end does not require `$39`; palette mode 2; stages 1/2 unchanged | S3-02 | no (`game.rs`) | 5 |
| S3-05 | `--stage 3` differential/pixel/boss harness + evidence run | `crates/gradius-diff/src/main.rs`, `game.rs` stage hook | native and oracle both start stage 3; state/boss window 0 divergences; pixel playfield and HUD >= 99.9%; landmarks 7979 / 9126 / 9639; turret nametable sample | S3-00, S3-01, S3-02, S3-03, S3-04 | no (green last) | 5 |

Total: **40 h** (about five working days). Critical path is
`S3-00 -> S3-02 -> S3-04 -> S3-03 -> S3-05` = 38 h; S3-01 is the only fully
parallel card and fills the 2 h of slack.

**First card that makes stage 3 playable end-to-end: S3-00.** It boots
`$19 = 2`, consumes `SECTIONS[2]`/`terrain(2)`, renders `STAGE3` with its tail,
and fixes the command branch that would otherwise set `boss_spawned` on the
first turret and suppress the shared core. Stage 3's `$9A30 = $0C`, spawn
`$0D` and `$98F0 = $0E` already match stage 1, so the stage reaches the core
and advances before the `$39`/`$82`/nametable work. It is not oracle-exact until
S3-02 and S3-05; the `$96` objects still run `ai_big_core` until S3-02.

### Parallel / conflict rules

* **Every card except S3-01 touches `game.rs`.** Do not run two of them in the
  same worktree. S3-02 and S3-03 both edit `update_objects`, `normal_frame` and
  the command path; sequence them. S3-04 edits `normal_frame` and the stage
  machine, so it must land before or after S3-03, never concurrently.
* S3-01 is frontend/music-only and can run from the start; it depends on
  S3-00 only for the stage selection API.
* S3-05 can start after S3-00 (add `--stage` plumbing) but its green run is
  last.
* If stage 2's S2-00/S2-01/S2-04 or stage 5's S5-00/S5-07 land first, reuse the
  `Stage` record, core gate and `--stage` hook; S3-00 drops to ~8 h and the
  total to ~37 h. If stage 3 lands first, stages 2 and 5 reuse its record and
  harness.

## Risks and exact evidence

1. **The command branch is a stage-3 blocker, not a cosmetic bug.** The current
   `command()` treats the first stage-3 command as the Big Core and sets
   `boss_spawned = true`; the real core never spawns. Fix this in S3-00 and
   replace the existing `a_section_two_command_spawns_the_big_core` test, which
   asserts the invention. `command_format.md` is the authority: `$A469` sets no
   `$012C`, `$0460` or boss flag.
2. **`$96` is not the end-of-stage boss.** Bosses.md calls it the only other
   boss-class object, but it is a 3-hit background turret. Do not route it to
   `ai_big_core`, do not award +1000, and do not let it set `boss_spawned`.
3. **Nametable paint is the only new subsystem.** A sprite approximation needs
   new art and is wrong (the tiles are `$20/$21/$22`, `$D9/$DA`, `$B0/$B1`,
   `$CF/$7C`, not OAM groups). Implement a sparse, world-fixed tile/attribute
   overlay; do not regenerate `STAGE3` or hand-edit its RLE. The type-2
   `$03EC - 1` special case and the optional second row (`+$20`) are easy to
   miss.
4. **`$046C = 0` on opening, and the closed state is un-damageable.** `$C937`
   clears accumulated damage when the turret opens; `$C06A` skips objects with
   `$048C == 0`. A kill test must first trigger the sector (or poke `$048C`)
   and then poke `$046C`; poking `$046C` alone on a closed turret does nothing.
5. **`$5D` lifetime is not the doc's prose.** The decoded code increments `$5D`
   at many spawn/effect sites and never decrements it, but the oracle RAM at
   frame 2000 has `$5D = 0` with two live turrets. Model the observable
   frame-scoped busy flag, and confirm against the 1984/1992/1999/2002 trace.
   Do not copy an always-nonzero counter.
6. **`$5F` persists across death/retry.** `$C776` increments it; only the
   `$96C2` clear loop on stage advance clears it. `$97D2` clears `$39`, `$3A`,
   `$5D`, not `$5F`. If the port resets the counter on death, the ten-kill gate
   becomes unpassable.
7. **`$39` selects the clear path; it does not gate the normal advance.** At
   `$9928`, `$39 == 0` stores `$90` and `$96C4` increments `$19`; `$39 != 0`
   goes to `$8E`/`$9842`, increments `$3A`, scrolls to `$3F = $11` and awards
   +5000. The measured 9639 is the normal path. Do not block stage 3 on ten
   kills without an oracle run that shows it; test both branches separately.
   The current port's `(section != 0 || stage_clear_ready)` gate is
   stage-1-shaped and must not be copied to stage 3.
8. **The 771-frame timer is stage-1-shaped.** It models the `$8A` eruption
   hold; stage 3 has no eruption. Use the rank-2 `$82` hold (`$9A28[$17] * 256`
   = 1024 frames; `transition_frames(2)` is the 7-frame mid-stage section
   transition only) and the `$0D` wait, then check the 7979 spawn. Do not
   inherit 771.
9. **`$C680` is mostly the bonus wave.** `$C40D` runs only on `$1B == $82` or
   `$3A != 0`; the stage-3 `$82` hold is 1024 frames at rank 2
   (`$9A28[$17] * 256`). The observed use is `$3A == 1` after a `$39` clear,
   spawning kind `$26` every 10 calls. Keep the `$3A == 0` branch for fidelity
   but do not build a test that assumes it fires in normal play.
10. **Tail and music values are per-stage.** The tail is column 416, bank 2,
    entries 8-15 `0F`; the second theme starts at `0x0400`; the boss is at
    `0x0C00`. Do not reuse stage 1's `BOSS_SCROLL = 2820` or stage 5's 384
    column.

## Evidence commands

Use the card target directory everywhere:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stage3plan
```

Data and roots:

```sh
cargo xtask stages "$ROM" | grep '^    3'
cargo xtask sections "$ROM" | grep -E 'S2_TERRAIN|S3_TERRAIN'
cargo xtask stage-enemies "$ROM" | grep -E '^\$16|^\$17|^\$1C'
cargo xtask disasm "$ROM" --from A460 --to A521 --labels   # command paths
cargo xtask disasm "$ROM" --from C40D --to C6D8 --labels   # script dispatch
cargo xtask disasm "$ROM" --from C776 --to C8A0 --labels   # ten-kill gate
cargo xtask disasm "$ROM" --from C900 --to CA40 --labels   # $96 handler
cargo xtask disasm "$ROM" --from 9928 --to 9940 --labels   # $39 branch
```

Stage-3 oracle and RAM:

```sh
cargo xtask record-boss "$ROM" --stage 3 --frames 12000 --out /tmp/opencode/boss3.txt
ST3=$(for f in $(seq 170 320); do printf -- '--poke %d:26:2 ' "$f"; done)
cargo xtask probe "$ROM" --frame 2002 --press 180:start --keep-alive $ST3 \
  --poke 2000:474:3 --ram /tmp/opencode/st3-death.ram \
  --nametables /tmp/opencode/st3.nt --png /tmp/opencode/st3.png
```

Expected `record-boss` summary: `spawn 7979 ... end 9126 ... stage 4 from
9639`. The frame-2002 probe shows `$5F = 1`, score +300, and the erased /
explosion tiles around `$2323`.

## Verification windows

After S3-05 adds `--stage 3`:

```sh
PACK=/tmp/opencode/stage3.pak
cargo xtask extract "$ROM" "$PACK"
flock /tmp/verify-heavy.lock cargo build --release -p gradius-diff

# state / boss window: threshold through the next stage
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage3plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 10000 --stage 3 \
  --from 7800 --to 9750 --keep-alive --native-keep-alive \
  --report --json /tmp/opencode/stage3-boss.json

# pixel window: column-416 tail, then core entry
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage3plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 10000 --stage 3 \
  --pixel-report --pixel-window 3200:3800 --png-dir /tmp/opencode/stage3-png \
  --png-every 30 --json /tmp/opencode/stage3-pixel.json
```

Pass criteria: the state/boss window reports `WITHIN TOLERANCE (0
divergences)`; pixel playfield and HUD are each `>= 99.9%`; tail samples at and
after 3328 px show `chr_bank 2` and palette entries 8-15 = `0F`; stage 3 exits
to `$19 = 3` at the measured 9639 frame (normal path). The separate turret
probe must match the oracle's nametable sample at frames 1984/1992/1999/2002
and show `$5F` 0 -> 1 with +300. A ten-kill / poked-`$39` run must take the
longer `$8E` path, award +5000, increment `$3A`, and produce the `$C680` bonus
wave. Run `scripts/verify changed` in each code worktree; do not run the full
workspace suite there.

## Definition of done / out of scope

Done when a stage-3 run starts at `$19 = 2`, consumes `SECTIONS[2]` and the 18
terrain placements, renders `STAGE3` plus the column-416 bank-2/black tail,
plays `$5B` after boundary 4, handles the 53 commands as `$96` background
turrets with the `$CA23` paint/erase and the ten-kill `$39`, runs the `$C680`
bonus wave, reaches the shared Big Core at `$0C`/spawn `$0D`, awards +1000 and
advances at 9639 on the `$39 = 0` path (or at 10608 on the `$39` path, after
the 5000-point `$8E` scroll),
without changing stages 1 and 2.

Out of scope: stage 4's `$C5A7`, stage 5's `$14`/`$A4A0`/`$CB8B` and the
`$19 == 4` chain, stage 6's `$CD9F`/`$C6D8` beams, stage 7's `$9865` loop,
the full `$0F`/`$10` kill gate, and a
sprite substitute for the `$96` background art.

## Why this is 40 h, between stage 2 (24 h) and stage 5 (50 h)

Stage 2 is the cheap stage because it has no command records and no new AI
handler; its `$C540` script and shared core are small. Stage 5 is the expensive
one because of `$14` (`$CA58`), the `$A4A0`/`$0600` cannon system and the large
`$19 == 4` exception chain.

Stage 3 sits between them:

* vs stage 2, **+16 h**. The stage-3 boot slice is +3 h (53 command records and
  the `$A469`/two-nibble fix); `$96`/`$C900` plus the nametable overlay is
  +14 h; the `$C680` bonus wave is +3 h; stage 2's `$C540` script (-4 h) is not
  needed. 24 + 3 + 14 + 3 - 4 = 40.
* vs stage 5, **-10 h**. Stage 5 pays for the `$14` state machine, the four
  `$0600` cannon records, placement/fire/release and the `$19 == 4` chain;
  stage 3's `$C900` is one handler plus a tile overlay, and its only new stage
  branch is `$C680`/`$39`/`$3A`. Stage 5 also carries more background and music
  wiring. The shared `Stage` record, core gate and `--stage` harness are common
  infrastructure: whichever of stages 2/3/5 lands first pays for them once.
  With stage 2 or stage 5 already merged, stage 3's incremental cost is about
  32-37 h.
