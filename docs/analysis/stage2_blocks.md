# Destructible background cells (stage 2 pink balls, stage 6)

Measured on the oracle (ROM run, `xtask nametable|probe --press 180:start --keep-alive --poke F:26:1`,
and the natural boss-script run, stage 2 from frame 9601).

## Which cells break

The collision map (`$0500/$0600`, 2 bits per tile) has these field values: 1 = solid and
indestructible, 2 = solid and destructible by a shot. Cells with field 2 exist only in
stage 2 (tile `0x40`, 386 cells) and stage 6 (`$19 == 5`, tiles `0x40/0x41`, 1242 cells).
Stages 1, 3, 4, 7 have only field 1; stage 5 has none.

## ROM routine

`$C29F` runs the player-shot loop at `$C2BE` (X = 5..0). `$C3A9` probes the map at the shot,
the field is shifted out at `$C2CB`:

- field 2: `JSR $C329`. It clears the two map bits (`STA ($A0,X)`, mask table `$C395` =
  `FC F3 CF 3F`), queues a 5-byte `$0700` VRAM record writing tile `$00` at that nametable cell,
  and calls sound `$EC18` with id 3 (id 4 when `$19 == 5`). A laser shot (`$0163 == 1`) keeps
  flying; any other shot dies in `$C0B7`.
- any other non-zero field: the shot dies (`$C0B7`), laser included.
- the ship terrain check (`$C2B6`, `$C39D`) kills on any non-zero field, field 2 too. Only
  shooting clears a ball.

Each hit erases exactly one cell, one VRAM write of `$00` (e.g. `$21CE`..`$21D8` at poked-run
frames 5146, 5169, 5210). The score (`$07E4`) does not change: no points.
The erase becomes visible one frame after the break, because the VRAM queue is flushed at the
next NMI. The map bits clear immediately.

## Port

`Game::broken` (bit per tile row per world column) masks the map for ship, shots and walkers;
`fresh_breaks` delays the visible erase by one frame. Both live in the save state (tail of the
collision chunk). Tests: `game/tests/collision.rs`, `gradius-diff/tests/stage2_blocks_replay.rs`.

## Replay figures (`scripts/stage2-blocks-script.txt`, fire held while sweeping the wall)

| | first divergence | divergences | play pixel match |
|---|---|---|---|
| before | frame 11529 (score) | 54718 | 99.5% |
| breaks only | frame 16028 (score 3490 vs 3480) | 1244 | 99.6% |
| `$83BA` rounding | frame 17558 (stage-2 boss covers, y one pixel low) | | |

State is exact (tolerance 0) over 9601..19430 with 51+ breaks (first at 11262). Pixels over frames up to
12100: 0 mismatch frames (the ball mask matches the oracle on every frame; before the
one-frame delay, 37 break frames mismatched by 23 pixels). The fixture
`stage2-blocks-9601-19430.state` is recorded by `scripts/gen-fixtures.sh`.

## Sound and stage gates

- `$C329` plays sound id 3 (id 4 in stage 6). The port captures both from the ROM driver
  (`Sfx::BlockBreak`, `Sfx::BlockBreakStage6`, appended last) and the core emits
  `Sound::BlockBreak`/`BlockBreakStage6` on every break.
- `$C29F` opens with `$19 == 4` (stage 5): `RTS`, no map tests at all; `$19 == 2` (stage 3):
  on even `$02` the ship and shot map tests are skipped (jump to `$C2F9`). Modelled in
  `Game::terrain_pass_runs`; stage 3/5 replays stay exact.

## `$C2F9`: enemy bullets against the map

`$C29F` falls into `$C2F9` after the shot loop (or jumps to it on even `$02` in stage 3).
For each live bullet `$C3CD` probes `($0376, $0336 + 8 if $0316 == 0)`; any non-zero field
(solid or destructible) clears the bullet through `$AEF2`. Nothing is broken and no sound plays.
`$C2F8` is an `RTS` for stage 3 (`$19 == 2`) with the ship alive (`$0100 < 2`), so in stage 3
odd frames test ship and shots, even frames test bullets, and a dying ship tests both.
Stage 5 returns at `$C2A9` before everything. Modelled in `Game::collide_bullet_terrain` and
`bullet_terrain_pass_runs`; tests in `game/tests/collision.rs`.

## The 15959/16028 divergence

Not a block bug. The aimed launcher (`$BCAF`) calls `$83BA` for the minor-axis slope, which
is not `minor*256/major` truncated: for a major axis of `$80`+ it halves dividend and
divisor first, and the 17th iteration rounds. Slot 2 kind `$8B`, aimed at frame 15837 with
`ax = 135`, `ay = 32` (ship at 152,49; `$BD5B` rank 2 so `q + q/4 + 1`): the oracle's `$044C`
fraction step is `$4D` = 77, i.e. `q = 61` (60.68 rounded), the port's 76 (`q = 60`).
The y pixel then flips one frame late (15846 vs 15847), and the same offset in slot 7 made the
shot kill land four frames late at 16028. `slope` in `game.rs` now runs the ROM's divide.

## Gaps

- `$0E` (vram queue length) is not updated by breaks; it only matters to stage-3 turrets.
## The 17558 divergence (stage-2 core, 2 covers)

The core (slot 9) and its covers (slots 7, 8) sat one pixel low from 17558. Not blocks: `$BA1C`
(the fire-frame re-aim, every `$B904[rank]` frames) ends in `CMP $0320` with `BCS` taken when the
core goes up, so carry is set at the upward `SBC $B8F2,Y` (`$BA38`) and that frame borrows no
extra 1/256. Non-fire frames reach `$BA2E` from `BCC $BA1A` with carry clear and do borrow it.
The port added the extra borrow on every upward frame; the sub-pixel lag made the 2-pixel step
land one frame early (17558 vs 17559). Fixed in `ai_big_core` (`ai/large.rs`). The replay is now
exact at tolerance 0 over 9601..19430 (the stage-3 handoff); the earlier test ran at the default
1 px, which hid the drift.
