# Stage-2 boss/end pixels: 17280-19430

Classification of the stage-2 boss and end-of-stage pixel residue, on branch
`agent/s2bosspx` (from `main` at `c1f8cdd`), against the cartridge
`Gradius (E).nes` (SHA-256 `268dc4b5b344e06e96281eb3159c1f61d3011c4f21dd1a2b4508aa934aff6143`,
the ROM of `stage2_boss.md` and `stage1_completion.md`), and refreshed after the
visuals pass on `main` at `d891d70`. This is the report-only residue named in
`stage2_boss.md` §4. The compared state is exact (0 divergences, every field and
object slot 100%), so every pixel below is a renderer or derived-sprite gap, not
a simulation divergence.

The natural path is the only reference: reset -> stage 1 -> stage 2 -> the
stage-2 core. The 15968 `$C540` eruption `obj.y` residual is outside this
window and is **fixed** (`2c770c6`: `$BCAF` complements negative deltas to
`|d|-1` and the rank-2 `ADC` leaves carry, so the minor slope is
`q + q/4 + 1`); the natural 12001-19430 window is gated at zero divergences by
`stage2_window`.

The first measurement of this note was **97.3% match / 96.8% play / 100.0%
HUD**; the visuals pass fixed the four causes it named and the window is now
**99.8% / 99.8% / 100.0%**. The current numbers and the two remaining classes
are in [Current status](#current-status); the sections after it keep the
original classification of the fixed causes as history.

## Current status

Measured on `main` at `d891d70` with the method below, 2151 samples:

| window | samples | match | play | hud | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| 17280-19430 (whole) | 2151 | 99.8% | 99.8% | 100.0% | accepted |
| 19037-19430 (hand-off) | 394 | 100.0000% | 100.0000% | 100.0000% | **hard gate** |
| 18917-19036 (death tail) | 120 | 100.0% | 100.0% | 100.0% | exact |
| 19429 (stage-3 frame 1) | 1 | 100.0% | 100.0% | 100.0% | exact; ring skipped (`$1B & #$70`) |
| 19430 | 1 | 100.0% | 100.0% | 100.0% | exact |

1365 of the 2151 samples carry some residue and the worst are
**18716/18720/18724 at 97.8%** (~1.35 k px). Two classes remain, both
presentation-only:

* **Rotating OAM flicker.** The full-width `y 107-154` bands on 17283-17305
  (21 samples at 0.1-0.5%) are the OAM rotation's 8-sprites-per-scanline pick
  landing on a different subset than the port's, so the dropped sprite differs
  along those scanlines. Route: `apply_oam_ring`, `game.rs::build_sprites`.
* **Core-shot alignment.** The samples above 1.0% (36 frames, 1.7-2.2%,
  17559-18847, worst 18716/18720/18724 at 97.8%) are volleys in flight: the
  shots use the cartridge's `$41` group, palette and straight-left direction
  but are drawn off the ROM's OAM pod positions. Shots are not a compared field
  (`dump_state` has no bullet table). Route: `game.rs::ai_big_core`,
  `build_sprites`.

The boundary is exact too: **19429** used to draw the shield ring one frame
before the oracle (52 px, 0.08% of the frame), and the `$1B & #$70` hand-off
guard at `$8B74` now skips it, the same rule as the 9601 stage-2 entry. The
ring's phase (group `$5A` / tile `$B7`) was already correct, so **19429 and
19430 are both pixel-exact**.

Accepted under the no-long-tail policy: neither remaining class changes what the
player sees or does. The hard window is **19037-19430 at `0.999`, measured
100.0000% on all 394 samples**, pinned by
`crates/gradius-diff/tests/stage2_boss_pixels.rs` and `scripts/release-check.sh`,
which also pins 19429 and 19430 per frame (52 px would sit below the rate's
resolution over 394 samples); the whole 17280-19430 window is report-only at its
99.8%.

## Method

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-s2bosspx
ROM="$GRADIUS_ROM"
cargo build --release -p gradius-diff

D=/tmp/opencode/target-s2bosspx/release/gradius-diff
flock /tmp/verify-heavy.lock "$D" --rom "$ROM" --pack assets/gradius.pak \
  --frames 19431 --script scripts/boss-script.txt \
  --keep-alive --native-keep-alive \
  --from 17280 --to 19430 --report \
  --pixel-report --pixel-window 17280:19430 \
  --json /tmp/opencode/s2boss-full.json
```

The report prints `WITHIN TOLERANCE (0 divergences)`, 2151 compared frames. The
pixel table splits at `y = 208` (`HUD_TOP`); the HUD is 100.0% on every sample,
so the whole residue is playfield.

## Window table (before the visuals pass)

The table below is the original 97.3% measurement, kept as the record of the
fixed causes. The current figures are in
[Current status](#current-status).

| window | samples | match | play | hud | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| 17280-19430 (whole) | 2151 | 97.26% | 96.84% | 100.00% | report-only |
| 17280-17281 (pre-spawn) | 2 | 100.00% | 100.00% | 100.00% | exact |
| 17282-18916 (core live) | 1635 | 96.43% | 95.88% | 100.00% | 3 classes below |
| 17282-17357 (core entering) | 76 | 97.55% | 97.18% | 100.00% | core + covers at the edge |
| 17358-18916 (core held) | 1559 | 96.37% | 95.82% | 100.00% | core + shots |
| 18917-19036 (death tail) | 120 | 99.57% | 99.50% | 100.00% | port-only shots |
| 19037-19428 (hand-off) | 392 | 100.00% | 100.00% | 100.00% | **exact** |
| 19429 (stage-3 frame 1) | 1 | 99.92% | 99.92% | 100.00% | one sprite (now exact) |
| 19430 | 1 | 100.00% | 100.00% | 100.00% | exact |

The 19429 row is history: the `$1B & #$70` hand-off guard now skips the ring,
so that frame is exact in [Current status](#current-status).

Worst samples then: **18440 at 95.8% (2560 px)**, 18575 (2544), 18155 (2542),
all in the held phase with a shot volley in flight. The union box of the whole
window is the full playfield, `0,0-255,199`.

## Divergence timeline (before the visuals pass)

| frames | size | class | route | now |
| --- | --- | --- | --- | --- |
| 17282 | 150 | background is 1 px off from the frozen scroll | `game.rs::render` | fixed (`scroll_lag`) |
| 17283-18915 | 1.4-2.6 k | core sprite group, then core + shots | `game.rs::spawn_boss`/`ai_big_core` | fixed (`$6C` frame, `$41` shots) |
| 17283-17305 | up to 440 of the above | covers' parts wrapped to the left edge | `video.rs::draw_sprite` x wrap | fixed (clip at column 255) |
| 18915-18916 | 1.9 k / 627 | last core frame and the cover clear | same as the core class | fixed |
| 18917-19036 | 175-367 | the port's own shots still on screen | `game.rs` shot spawn | exact |
| 19037-19430 | 0 | nothing | - | exact |
| 19429 | 52 | shield-ring phase (`$5A`/`$B7` vs `$5D`/`$C3`) | `game.rs::build_sprites` | phase and `$1B & #$70` skip fixed; now exact |

## 1. First real divergence: 17282, the frozen-scroll background

The first differing sample is **17282**, 150 playfield pixels, box
`1,0-250,199`. It contains no sprite at all: every differing position is a
single-pixel star that moved one column, and they come in symmetric pairs
(oracle black / port colour, oracle colour / port black) - 75 stars. The core
itself first appears in the sample after the spawn.

The scroll state is `0:13:0` on 17281 and 17282 on **both** sides (it steps
from `255:12:128` to `0:13:0` at 17281 and then freezes for the core fight).
The star pattern is therefore not a state difference: it is the one-frame
latch. `Game::render` draws the background and the cell overlay at
`self.scroll.position() as i32 - 1` (game.rs:3987 and 3999), i.e. 3327 for
`0:13:0`. The cartridge's PPU latches the *previous* frame's value: 3327 on
17281 (`255:12:128` -> 3327.5 -> 3327) but 3328 on 17282, because the previous
frame is already `0:13:0`. The -1 is only right while the scroll moves
0.5 px/frame; on the freeze frame it is one pixel short, and the whole
background stays one pixel left for the rest of the frozen arena.

Verified on clean background windows (no sprite content) at 17300, 17430,
18000, 18440 and 18900: identity match is 99.6%, and comparing the oracle's
`x` against the port's `x + 1` matches **100.0000%** in every window. The
offset is exactly one pixel (a two-pixel shift is worse), it starts at 17282
and it is gone as soon as the scroll moves again (18916): the 18917-19036 and
19037-19430 windows have no background difference at all.

Cost: about 140 px/frame (6.5% of the residue, by single-pixel connected
component count), on every frame of the frozen arena. Because it is one full
pixel of the star field it is visually invisible, but it is the first
divergence and it is a real render offset bug.

**Status: fixed on `main`** by the stage-1 tail pass (`d1fc2ae`).
`Game::render` now draws `scroll.position() - scroll_lag`, where `scroll_lag`
is 1 only on a frame that steps the scroll (`$98E1`) and 0 while `$5B` or the
boss lock holds it. That is the latch rule measured here: 17281 steps and
still shows 3327, while 17282 is already held and shows 3328. Route:
`crates/gradius-core/src/game.rs::render`.

## 2. The core's sprite group (dominant, ~1.1-1.7 k px/frame)

The boss cluster is the biggest difference. At 17430 the box
`x 140-256, y 0-120` holds 1674 of the 1791 differing pixels; over the whole
core-live window it is 52% of the residue (1137 px/frame average, 1701 at
18440).

Ground truth is the oracle OAM. At 17430, 18000 and 18440 the cartridge draws
the core from group **`$6C`** (`$929A`, four parts: tiles `$35/$37/$37(flip
Y)/$3B` at dx `-8/0/0/+8`, dy `-6/-14/+2`, palette 2) at the object's base;
the `$046C` armour frames are `$6C`-$6D-$6E-$6F-$70-$71` (the router's
`big_core::SPRITE_FRAMES`). The two `$99` covers are group `$85` (`$9275`, the
nine-part hatch) at the upper cover's base and `$32` (`$926C`) at the lower,
exactly where the compared state puts slots 7/8.

The port instead gives the core `sprite: 0x8F` in `spawn_boss`
(game.rs:1630) - the 16x32 hatch pair `$14/$18/$16/$1A`, two sprites - and
never uses `SPRITE_FRAMES`. The resulting cluster is not a shifted copy of the
cartridge's: at 17430 the two clusters have nearly the same pixel mass (1029
oracle / 1046 port opaque inside `x 140-256, y 0-120`) and the difference is
symmetric (662 oracle-only, 679 port-only, 309 same-position colour changes),
and no 8x16 cell of the cartridge's boss tiles exists anywhere in the port's
frame (best shape match over the whole frame is 0.63-0.80). The port draws the
right object positions with the wrong sprite layout; it is not a missing pass,
a palette remap or a one-pixel offset (every global and per-cell shift up to
+/-3 px leaves the match between 0.37 and 0.40).

Route: `game.rs::spawn_boss` (the core's `sprite` must be the `$046C`-indexed
armour frame, `big_core::SPRITE_FRAMES`) and
`game.rs::ai_big_core`/`sync_boss_parts` for the two `$99` covers; the
fifteen-part cluster also has to keep the cartridge's OAM slot order, because
the eight-sprites-per-scanline pick decides which parts survive (`$8B15` slot
walk, `apply_oam_ring`). Predicted check: with the four-sprite `$6C` core the
17282-18916 window should rise to ~100% on the same state run; the covers'
group data already matches the cartridge, so no cover table change is
expected.

Note: the entry frames 17283-17305 also show the covers' outer parts drawn at
the **left** edge (up to ~440 px/frame) while the oracle clips them at x = 255.
The port wraps sprite X (`video.rs::draw_sprite`, `sprite.x & 0xFF`); this is
the same class as the x = 0 shot column measured in `pixels.md` (frames
655/666/677/688), so fixing the wrap covers both.

**Status: fixed on `main`** by the visuals pass (`5ed1ac3`): the core uses the
`$046C` armour frame, the covers keep the cartridge's order and slot walk, and
`draw_sprite` clips at column 255. The 17282-18916 sub-window rose from 96.43%
to 99.7%; what is left there is the rotating OAM flicker and the shot
alignment in [Current status](#current-status).

## 3. The core's shots (~0.4-0.9 k px/frame)

The cartridge's core fires four 16x16 shots per volley from the four pod
offsets, sprite group **`$41`** (`$9263`, tiles `$0E/$10`, palette 1), moving
left at about 3.5 px/frame: OAM at 18000 has the volley at x = 161/145 and
18003 at x = 151/135, with the row at y 26/39/51/64. The port draws every
enemy shot with group **`$3A`** (`$90BA`, tiles `$6F/$71`, palette 2) and its
shots are somewhere else entirely: at 18000 the oracle's bars are at
x 145-177, y 26-70 while the port's two blobs sit at x 82-109, y 90-130 (it
aims them down the screen instead of straight left).

This class only appears while a volley is on screen, which is why the window
splits cleanly: 17430 has no volley (117 px outside the core box), 18000 has
639, 18440 has 859. After the core dies the port's shots keep flying: from
18917 to 19036 the only differing pixels are port-only sprites (175-367 px,
the last at `241,76-251,101` on 19036), and 19037 is the first clean frame.

Shots are not a compared field (`dump_state` has no bullet table), so this is
invisible to the state gate. Route: `game.rs::ai_big_core`'s
`push_enemy_bullet(..., 0x3A)` and the enemy-shot sprite/spawn/velocity model
(`build_sprites`' enemy-shot loop); the cartridge's core shots are their own
group and go straight left.

**Status: fixed on `main`** by the visuals pass (`5ed1ac3`): the core fires
group `$41` from the four pod offsets, straight left, and the port-only shots
that were the whole 18917-19036 tail are gone (that window is 100%). What is
left is the volley's position while it is in flight, in
[Current status](#current-status).

## 4. Death and hand-off

* **18915** (1920 px) is the last frame of the core cluster: the cartridge
  still draws the ring and the covers just before the overheat self-destruct,
  the port is already a frame into the debris layout. Same routes as §2 and §3.
* **18916** (627 px) keeps the covers' remains and the port-only shots.
* **18917-19036** is §3 alone; the background, the debris sprites
  (kind 2, `156,117-202,162` at 18917 matches exactly), the HUD and the
  scroll are all exact.
* **19037-19428** is **exactly 0 px on all 392 samples** - the clear hold is
  pixel-perfect, and the stage-3 hand-off (`scroll=1:0:0` at 19429) continues
  the run below.
* **19429** (52 px, `37,73-44,88`): the **shield ring**, not an extra ship or
  an option sprite. `$46` is held at 5 by the keep-alive, `$45`/`$5C` are 0 and
  the OAM has no option group; the box is one 8x16 ring sprite at ship
  `(+15,-1)`. The ring **phase** was the first reading here: the ROM cycles
  `$8B70-$8B8B` with `($02 >> 2) & 3` where the port used `self.nmi_frame`, so
  the `$96C2` handoff landed it on a different quarter (oracle group `$5A` /
  tile `$B7`, port `$5D` / `$C3`). That is **fixed**: the port now derives the
  quarter from `frame - 5` and draws `$5A`/`$B7`. The remaining one-frame
  timing is also **fixed**: `build_sprites` now skips the ring under the ROM's
  `$8B74` guard, `$1B & #$70 != 0` (the hand-off state the port models as
  `clear == Clear::Handoff`/`endgame == Advance`/`boss_clear == 1`), so the
  oracle's first ring frame at 19430 is matched and **19429 is pixel-exact**.
  [stage3_gaps.md](stage3_gaps.md) §2 has the corrected measurement
  (the earlier "extra ship/option trail" reading in this note was wrong).
* **19430** is exact.
* **19037-19430** is therefore **exactly 0 px on all 394 samples**; 19429 and
  19430 are also pinned per frame because 52 px would sit below the rate's
  resolution over 394 samples.

**Status: the death and hand-off classes are fixed** by the visuals pass
(`5ed1ac3`) and the `$1B & #$70` ring skip. 18915 now measures 99.5% (the debris
layout starts on the same frame), 18917-19036 is 100% (the port-only shots are
gone), and 19037-19430 is 100.0000% on all 394 samples with both boundary frames
exact.

## What is not the cause

* **HUD:** 100.0% on all 2151 samples, including the split rows (the boss arena
  never scrolls terrain into y 208-209; the `HUD_Y` issue from
  `stage2_pixels.md` is not reachable here).
* **Compared state:** 0 divergences; the core spawn (17282), its `x/y`, the
  covers, the overheat death (18915), the 514-frame clear and the stage-3
  entry (19429) all match frame for frame.
* **Background tiles/palette/terrain:** only the 1 px latch offset (§1, now
  fixed). The stage palette and the generated stage-2 map are exact - proven
  by the 100.0000% shifted match on clean windows and by the exact hand-off
  window.
* **Debris/explosion sprites:** the kind-2 debris is identical from 18917 on
  (same blob, same pixels); the `$29-$2C` death burst never fires on this
  path (the core overheats instead of being shot), so no explosion class
  appears.
* **Object order/`$99` slot handling:** the covers' group data in
  `sprites.rs` is byte-identical to the cartridge's `$85`/`$32`; the residue is
  in what is drawn, not in the tables.

## Gate recommendation

* **Gate 19037-19430 at `0.999`.** It is 394 samples, measured **100.0000%** on
  all three rates, and it is the meaningful end-of-stage landmark: last debris
  frame + 1 through the second stage-3 frame. It is pinned by
  `crates/gradius-diff/tests/stage2_boss_pixels.rs` and by
  `scripts/release-check.sh`, which also pins 19429 and 19430 per frame.
* **Keep 17280-17281 in the report at 100%.**
* **Keep the boss fight 17282-18916 report-only at 99.7%** (was 96.4%): the
  remaining classes are rotating OAM flicker and shot alignment, both accepted
  presentation residue. Do not extend the hard gate to them.
* The whole **17280-19430** window is 99.8% and accepted; it is not a
  substitute for the 19037-19430 gate.

## Saved pairs

`/tmp/opencode/s2boss-pixels/` holds `oracle-*` + `native-*` pairs and
`panel-*.png` (oracle | port | red diff) for the named frames, saved from the
pre-fix run; they are still the right landmarks, but 17282 and 18917-19036 are
exact now.

| frame | why |
| --- | --- |
| 17281 | last exact frame before the spawn |
| 17282 | first divergence: 1 px star-field latch (150 px) |
| 17283 | core entering; covers wrap at the left edge |
| 17300 | entry phase, wrong core group + edge wrap |
| 17430 | core cluster, no shot volley (1674 px in the core box) |
| 18000 | core cluster + one shot volley |
| 18155 | third-worst sample (2542 px) |
| 18440 | worst sample (2560 px, 95.8%) |
| 18575 | second-worst (2544 px) |
| 18915 | overheat death frame (1920 px) |
| 18916 | cover clear + port-only shots (627 px) |
| 18950 | tail, shots only |
| 19036 | last port-only shot at `241,76` (175 px) |
| 19037 | first exact frame of the gated hand-off |
| 19429 | stage-3 frame 1, ring skipped by `$1B & #$70` (was 52 px) |

The full per-frame pair dump is in `/tmp/opencode/s2boss-pngs/` (all 19431
frames).
