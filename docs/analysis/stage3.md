# Stage 3 background: the turret-free extraction

`stage_bgs::STAGE3` is the scrolling background, not "the PPU nametable after a
turret painted it". The `$96`/`$C900` mini-boss paints the same cells through
the `$0700` queue (`docs/analysis/nametable_queue.md`), so `xtask screens
--stage 3` subtracts the turret writes before folding the nametable into the
wide map. This note records the mechanism and proves the resulting base.

## Extraction

```sh
ROM="$GRADIUS_ROM"
CARGO_TARGET_DIR=/tmp/opencode/target-s3base \
  cargo xtask screens "$ROM" --frames 20000 --assist --stage 3 --press 180:start
```

While sampling, `screens.rs` reads the ten objects after every frame and asks
`nametable::turret_paint_records` for both art phases of every live `$96`
turret (`$CA23` groups `4*type` and `4*type + $10`). After the next frame's
VRAM writes it restores the cells of any *complete* turret record from the
pre-frame nametable, so a background write to the same cell still wins. This is
the `$C900` suppression: no turret tile is ever folded into the wide map.

`$5D` cannot do this. It is frame-scoped and the frame handler clears it before
the turret runs, so a `$5D = 1` poke between frames has no effect (verified with
`probe`: `$5D` is `0` at the end of the paint frames).

## The extracted cells are base art

The type-0 turret at `$03BC:$03EC = $2323` paints world cells `(131,25)`,
`(132,25)` and `(131,26)`:

| `$048C` | group | cells |
| --- | --- | --- |
| `$14` (base, damageable) | `00 8F 00 90` | `(131,25)=00`, `(132,25)=8F`, `(131,26)=90` |
| `0` (alternate, invulnerable) | `20 21 00 22` | `(131,25)=20`, `(132,25)=21`, `(131,26)=22` |

The extracted map stores `20 21 22` at those cells. That is base art written
by the stage background stream, not a leftover turret paint. Proof: patch a
copy of the ROM at `$C916` to `RTS` (the handler's paint entry) and run `xtask
screens` on it with the turret filter disabled; the wide map is byte-for-byte
identical to the current `STAGE3`, including `(131,25)=20`, `(132,25)=21`,
`(131,26)=22`, and the same tile FNV-1a `D8202719436CC2F9`. The background
stream writes `$20` to `$2323` at frame 1591, before any turret is live. The
turret's base group `00 8F 00 90` and its alternate group `20 21 22` are both
legitimate art; neither leaks into the background.

So the pinned hash stays `D8202719436CC2F9`, and no clean re-extraction is
needed. `nametable_queue.md` previously called this map "not turret-free"; that
claim came from the tile values alone and is corrected there.

## What the runtime still has to paint

The static map holds base art at those cells; the port's runtime overlay
replaces it with the turret's live phase:

* base phase `00 8F 90` and alternate phase `20 21 22` at the cells above,
  keyed by world column (`xtask::nametable::world_column`, `(131,25)` here),
  not by the raw `$2323` address (which aliases every 64 columns);
* the `$C776` explosion records and the `$C81C` packed collision patch;
* the `$C930[2] = 70` frame phase cycle, the `$5D`/`$0E` gates, and the
  ten-kill `$39` gate (S3-02).

Applying these after `draw_screen` changes only the cells a live turret owns;
when the turret scrolls off, the background underneath is already the correct
phase-independent art.

## S3-02 implementation status

The runtime side is implemented in `game.rs` with the decoded tables in
`stage3_enemies.rs`:

* `$A469` stores the dedicated `nametable_addr = b3:b4`; sprite, size class,
  velocities and the boss flag stay clear.
* `$C900` runs the `$AED7` drift, the `$5D`/`$0E >= 4` gates, the `$C930[rank]`
  cooldown (`80,75,70,65,60,40,30`; rank 7 reads `$A9`) and the `$C954`
  half-plane sector tests. `$048C == 0` is the invulnerable alternate art;
  `$048C == $14` is the damageable base art and clears `$046C` on entry.
* `$C9B4` maps both records to world cells with `world_column(addr, $3F:$3E)`.
  Type 2 starts the first record at `low - 1` without borrowing; the second
  row uses the unadjusted `low + $20` and its carry.
* Damage is three hits while `$048C == $14`; the shot path lets the handler run
  `$C776` instead of the generic 10-point destroy.
* Death records `$5F += 1`, sets `$39` at 10, scores 300, writes the `$C7B0`
  explosion cells and the ten `$C81C` collision bytes, then becomes kind-2
  debris. `$5F` is the shared `volcano_kills` byte and survives death/retry.
* `draw_cell_overlay` runs after `draw_screen` and before the HUD. It uses the
  extracted screen's CHR bank, attributes and column-416 tail rule and never
  mutates the static `STAGE3` RLE.

Unit coverage lives in `stage3_enemies::tests` and the game tests: paint/erase,
type-2 `-1` and second row, third-byte gate, all four sector half-planes,
damage gating and hit clearing, ten-kill `$39` and retry survival, death
explosion plus `$C81C`, and the framebuffer overlay.

## Natural stage-3 window

Stage 3 starts at frame **19429** in the natural run from reset
(`scripts/boss-script.txt`, both keep-alives); the stage-2 tail runs to 19428.
`tests/fixtures/stage3-19429-23000.state` records frames 19429-23000 from that
run (3572 frames), and `crates/gradius-diff/tests/stage3_replay.rs` replays it
from reset with no cartridge, so the stage-2 -> stage-3 handoff is covered
ROM-free. The previous `stage3-12161-15000.state` fixture was a stage-2 tail
before the stage reset; it is now
`tests/fixtures/stage2-12161-15000.state`, and its replay asserts the window
exact (0 divergences). Stage 3 does not begin until 19429.

The state replay of 19429-23000 is exact over the whole window: **0
divergences** across all 3572 frames, so there is no first divergence left (the
21526 capsule is fixed; see [stage3_gaps.md](stage3_gaps.md)). The `$1C`
descender's `$B622` sprite (the old 19884 divergence), the stage-3 sprite CHR
bank, the arc velocity (the old 20134 divergence) and the `$17` small core's
`$B79B` body (the old 20708 divergence) are all fixed. The shield-ring `$02`
phase is correct, and the `$1B & #$70` hand-off guard at `$8B74` now skips the
ring on 19429, so the entry frame is exact: **the ring is no longer part of the
residue**. The frame was 52 px of the window's ~2.31 M differing px, so the
aggregate moves only 98.9473% -> 98.9474% (play 98.7923% -> 98.7924%, HUD
100%). 19429 and 19430 are now inside the stage-2 hard hand-off gate
19037-19430 (394 samples) and are pinned per frame as a negative control.

The same window has a pixel check (`stage3_pixels.rs`, every 30th frame, 119
samples) that pins the aggregate: **98.9%** overall (play 98.8%, HUD 100%).
The stage-3 CHR bank 1 fix cut 108,013 -> 101,517, the `$0D` arc fix to 94,246,
and the `$B79B` small-core fix to the present rate. The remaining 19530-20700
residue is sprite group `$25` (`$8F45`, tile `$69`), the `$BC3E` object-fire
bullet (`$040C`/`$04EC`/`$0496`), not the player double shot (group `$24`,
tile `$73`): the port has no `$BBB1`/`$040C`/`$04EC` fire pass, so it never
emits these (stage2_pixels.md records the same residue for stage 2). This is
the only remaining **OPEN** pixel cause; it mixes with
the eight-per-scanline priority subset (stage3_gaps §3). The 20730+ small-core
window is now state-exact. The release gate is state-only: 19429-23000 must
stay at 0 divergences, and the whole 19429-23000 pixel window is report-only.
The worst pixel samples are 21420, 21390 and 21690 at 97.1-97.3%; the HUD is
100% across the window. No `$96` turret is live in 19429-23000, so this window
does not exercise the `$C900` overlay.

Remaining after this card: the `$BBB1`/`$BC3E` object-fire bullets (the main
pixel gap, **OPEN**), the `$C40D`/`$C680` `$3A` bonus wave (S3-03), and the
green `--stage 3` gate (S3-05). The `$17` small core (stage3_gaps §4) and the
stage-3 `$39` clear (stage3_clear.md) are **FIXED**, and the fixture now
replays with 0 divergences, the 21526 capsule included.
