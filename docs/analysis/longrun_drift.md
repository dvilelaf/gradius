# Pre-boss long-run drift: 1357 and 6503

Root-cause note for the two remaining boss-script divergences reported by
`scripts/release-check.sh` (the soft `longrun` gate) and for the default
script's frame 3159. The measurements are per-frame, so each entry names the
frame, the object slot and the exact ROM address whose rule the port does not
follow. The title one-frame offset (default 4677 vs 4676) is a separate card
and is not analysed here.

Measured 2026-11-09 on the `agent/longdrift` tree, release build in
`/tmp/opencode/target-longdrift`, logs in `/tmp/opencode/longdrift-out`, ROM
`Gradius (E).nes`. The cartridge is read in place; no cartridge bytes are
stored here.

## Reproduce

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-longdrift
export GRADIUS_ROM="$GRADIUS_ROM"
export OUT=/tmp/opencode/longdrift-out

cargo build --release -p gradius-diff -p xtask
cargo xtask extract "$GRADIUS_ROM" "$OUT/stage1.pak"
scripts/longrun.sh --rom "$GRADIUS_ROM" --pack "$OUT/stage1.pak" --out "$OUT/longrun"
```

`longrun.sh` writes one state line per frame to `boss.oracle`/`boss.native` and
`default.oracle`/`default.native`; the frame numbers below are those dump
`frame=` values. To read the same RAM the oracle used, replay the boss script
entries up to the frame of interest (the `--press` list is the script's own
lines; `--assist --keep-alive` matches `gradius-diff --keep-alive`):

```sh
args=$(awk -F: '/^[0-9]+:/ {if ($1+0 < 6505) print "--press "$0}' \
    scripts/boss-script.txt | tr '\n' ' ')
cargo xtask ramdump "$GRADIUS_ROM" --assist --keep-alive $args \
    --frame 6503 --range 0050:0060 --range 030C:0336 --range 036C:0376
```

`xtask ramdump` and `xtask disasm` already cover everything needed; no helper
was added.

## Results

| run | first frame | field | classification |
| --- | ---: | --- | --- |
| boss | 1357 | `obj.slot` | ship-vs-object collision box: Y borrow off by one |
| boss | 6503 | `obj.x` | `$AED7` drift runs during the `$82` scroll hold (`$5B`) |
| default | 3159 | `obj.slot` | `$AF92` turret fire gate phase (`$02 & $0F`), separate card |

The boss run has **0 state-mismatch frames** over all 12,000 frames; the only
firsts are 1357 and 6503 (plus a stage-2 slot drop at 11669, below). Both
diverge transiently and re-converge: 1357 by 1366 and 6503 by 6523.

One visibility trap: both dumps hide an object while its sprite byte is zero.
`$BE8D` clears `$012C` when it turns a killed slot into kind-2 debris, and the
debris handler only writes a sprite three frames later, so a slot can be
missing from the dump for a few frames even though its `$030C` is already `02`.
That is why 1357 reads "oracle absent" instead of "oracle kind 2".

## 1357: shield collision box Y borrow

Slot 6 is a weaving ship (kind `$08`, handler `$B266`), spawned at frame 1005.
It is level with the player and gets destroyed by the shield (`$46 = 5` is
pinned by `--keep-alive`). Per-frame slot 6:

| frame | oracle | native |
| --- | --- | --- |
| 1356 | `$88` at (13,173) | `$88` at (13,173) |
| 1357 | kind `$02` (hidden, sprite 0) at (12,174) | `$88` at (12,174) |
| 1358 | hidden debris | `$88` at (11,174) |
| 1359 | hidden debris | kind `$02` (hidden, sprite 0) at (10,174) |
| 1361 | debris sprite `$29` at (10,174) | hidden |
| 1363 | debris sprite `$29` at (8,175) | debris sprite `$29` at (8,175) |
| 1366 | cleared | cleared (1364) |

No score is added on either side, so this is the shield path `$C1CA`, not the
shot-kill path `$C0A5`. The ROM collision routine is `$C0FF`-`$C12E`:

```
A0 = $0360[p] + 4          ; ship X probe
A1 = $0320[p] + 8          ; ship Y probe
for slot = 9..0:
  if $030C[slot] == 0: continue
  X = $0460[slot]          ; $BFD4/$BFD8 size class
  A = A0 - $036C[slot]     ; SEC
  if borrow: continue      ; dx < 0
  if A >= $BFD4[X]: continue
  A = A1 - $032C[slot]     ; SBC, so the width CMP's borrow is included
  if A < $BFD8[X]: hit     ; $C168/$C1CA
```

After the width compare succeeds, carry is clear, so the Y subtraction is
`A1 - Y - 1`. The cartridge therefore accepts plain `dy` in **1..=height**,
not `0..=height-1`. At frame 1357: `A0 = 22`, `A1 = 190`, slot 6 at (12,174),
`$0460 = 0` (`$BFD4[0] = $10`, `$BFD8[0] = $10`), so `dx = 10` and
`dy = 190 - 174 - 1 = 15 < 16` -> hit. At frame 1358 `dy = 191 - 174 - 1 = 16`
and at 1359 `dy = 190 - 174 - 1 = 15` again (the ship's Y oscillates between
182 and 183), so a no-borrow port hits two frames late, exactly what the
native dump shows.

The port rule is in `crates/gradius-core/src/game.rs`, `collide_player_objects`
(around line 3058):

```rust
let dy = py.wrapping_sub(i32::from(object.y.int)) as u8;
if i32::from(dy) >= i32::from(tables::ENEMY_BOX_HEIGHT[size]) {
    continue;
}
```

That is the `0..=height-1` band: the borrow is not modelled, so `dy == height`
misses. `collide_shots` already models the same ROM borrow correctly
(`dy == 0 || dy > height` accepts 1..=height); this site did not get the same
treatment.

**Fix:** use the same accepted band in `collide_player_objects`:

```rust
if dy == 0 || i32::from(dy) > i32::from(tables::ENEMY_BOX_HEIGHT[size]) {
    continue;
}
```

One fix covers both the missed `dy == height` hit and the wrongly accepted
`dy == 0` (which the ROM rejects because `A1 - Y - 1` wraps to `$FF`).

## 6503: debris drifts during the `$82` scroll hold

During the eruption (`$1B = $82`, frames 6441-7210) the scroll is frozen. The
ROM's `$82` state `$98D0` does `INC $5B` and every `$AED7` drift then returns
immediately. `$AED7`:

```
$AED7  LDA $5B
$AED9  BNE $AF03      ; non-zero: skip drift *and* the x<8 despawn
$AEDB  ... $038C -= $80, borrow into $036C
$AEEE  CMP #$08
$AEF0  BCS $AF03
$AEF2  clear $030C/$010C/$012C/$014C/$016C
```

The eruption fireball in slot 1 (kind `$8A`) is killed by the shield at frame
6499 on both sides (same frame, same score: still 1260). The shield leaves
kind-2 debris at x = 155. Reading the oracle RAM at frame 6503 gives slot 1
`$030C = 02`, `$036C = $9B` (155), `$032C = $5F` (95), and **`$005B = 01`**.
The oracle debris therefore holds at x = 155 for its whole life; the native
debris slides 0.5 px/frame:

| frame | oracle slot 1 | native slot 1 |
| --- | --- | --- |
| 6498 | `$8A` at (157,97) | `$8A` at (157,97) |
| 6499 | kind `$02` (hidden) at (155,95) | kind `$02` (hidden) at (155,95) |
| 6503 | `$02` at (155,95) | `$02` at (153,95) |
| 6504 | `$02` at (155,95) | `$02` at (152,95) |
| 6522 | `$02` at (155,95) | `$02` at (143,95) |
| 6523 | cleared | cleared |

The port gates `scroll.step()` with `scroll_locked` but `ai_drift`
(`game.rs`, around line 1896) only tests `scroll_stop`:

```rust
fn ai_drift(&mut self, object: &mut Object) {
    if self.scroll_stop {
        return;
    }
    ...
}
```

`scroll_stop` is only set by the `$B563` wall and the Big Core handler; the
`$82` hold uses `scroll_locked` (`normal_frame`, around line 1282). So every
`$AED7` object (debris, background pieces, surviving turrets, shells) drifts
left through the eruption hold on the port while the cartridge freezes it.
The debris is the first visible case because it has the longest life.

**Fix:** the `$82` hold is the port's model of `$5B`, so `ai_drift` must honour
it too:

```rust
if self.scroll_stop || self.scroll_locked {
    return;
}
```

(Equivalently, set `scroll_stop = true` while `boss_stage == 1 && scroll_locked`;
the `ai_drift` test is the smaller change and matches `$AED7` exactly.)

## 3159 is a different cause

The default script's first divergence is not related to either bug above. At
frame 3159 the oracle has slot 5 kind `$89` (a `$09` shell from the `$AF28`
turret in slot 7) at (247,171); the port has no shell until frame 3174. The
shell is spawned in `$AF92`, whose gate is `($02 & $0F) == 0`. Replaying the
default script to frame 3159 gives `$02 = $51` (81) at frame 3159, i.e.
`$02 = frame - 6` here, not the `frame - 5` assumed in
`ai_turret_fire` (`game.rs`, around line 2246). The correct gate is therefore
`frame % 16 == 6`, not `frame % 16 == 5`; the port fires 15 frames (one period
minus one) late. That is a cooldown-phase bug in the turret gate, owned by the
separate 3159/Title card.

## Shared cause?

No. The three firsts have three independent rules:

* 1357 - collision box band in `collide_player_objects` (`$C0FF`/`$C11C`;
  the width compare's borrow feeds the Y subtract).
* 6503 - `$AED7` checks `$5B`, and the port's `$82` hold sets `scroll_locked`,
  not `scroll_stop`.
* 3159 - `$AF92`'s `$02 & $0F == 0` gate has the wrong frame phase.

1357 and 6503 are both pre-boss and both look like "an object disappears a few
frames early", but they need two separate one-line fixes in different
routines. There is no single shared fix.

## Remaining after these two fixes

The boss run is clean through 11668. At 11669 (stage 2) the oracle keeps slot 7
kind `$92` at (21,121) while the port drops it; slot pairs then stay at 100%
and the run re-converges. That is a stage-2 object-lifetime item, outside the
stage-1 pre-boss scope of this note. The default script still has the 3159
cooldown-phase item and the title offset.
