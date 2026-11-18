# Sprite animations, stage 1 (frames 700-1100)

Observed, not guessed: every sequence below was read out of the oracle's OAM
dump and the sprite-group table it uses. See `tools/anim.json` for the same data
in machine-readable form.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-anim

# one headless run per frame; probe replays from boot, so 700..1100 = 401 runs
for n in $(seq 700 1100); do
  cargo xtask probe "$ROM" --frame "$n" --press 180:start --buttons a \
    --oam "oam/$n.bin" >/dev/null
done
```

Start at 180 and start at 240 only shift the game timeline by 60 frames (both
pass the title); the animation phases below are tied to the game's own frame
counter, not to the press frame.

OAM in this run: 8x16 sprites from pattern table `$1000`, stored Y is `top-1`.
Unused slots hold `$F4`. Slot indices are **not** stable between frames (the game
rebuilds the buffer in a different order), so objects were clustered by tile
family and position, not by slot.

## Ship (24x16)

Three 8x16 body sprites at `x`, `x+8`, `x+16` plus an 8x16 exhaust flame 4 px
behind the tail. The frame is chosen by input direction; it never changed with
time (constant for all 401 frames at rest and in every held-direction probe).

| group | tiles (left to right) | attrs | when |
| --- | --- | --- | --- |
| `$8EF6` | `$03 $05 $07` | `$21 $20 $20` | Up held |
| `$8F07` | `$09 $0B $0D` | `$21 $20 $20` | no direction, Right, Left, Down |
| `$8F18` | `$0F $11 $13` | `$21 $20 $20` | never in this window |

Exhaust: tile `$DF`, attr `$23`, at `(x-4, y-8)` from the body, every frame.
Measured at frame 700: body x 72/80/88, stored y 96; exhaust x 68, y 88.

## Ring enemy (16x16, spinning)

Two 8x16 sprites side by side (`$15/$17`, then `$19/$1B`). Four phases of
**6 frames each**, a 24-frame cycle. Phases 3 and 4 reuse phases 1 and 2 with
both flips set and the two sprites swapped.

| phase | group | tiles (left, right) | attrs | duration |
| --- | --- | --- | --- | --- |
| 1 | `$8F5D` | `$15 $17` | `$22 $22` | 6 |
| 2 | `$8F66` | `$19 $1B` | `$22 $22` | 6 |
| 3 | `$8F6F` | `$17 $15` | `$E2 $E2` | 6 |
| 4 | `$8F78` | `$1B $19` | `$E2 $E2` | 6 |

Tracked object, frames 705-752: `17/15 E2` 6 f, `1B/19 E2` 6 f, `15/17 22` 6 f,
`19/1B 22` 6 f, `17/15 E2` 6 f, `1B/19 E2` 6 f, `15/17 22` 6 f, `19/1B 22` 6 f,
`17/15 E2` 4 f (object left the screen). The cycle is 24 frames.

## Player bullet

Weapon level 0 shot, group `$8F3B`: one 8x16 sprite, tile `$6B`, attr `$22`,
stored y 103 (top 104). It spawns at stored x 87, moves **+7 px/frame** and is
gone after stored x 241, i.e. 23 frames of flight (shorter when it hits
something). With A held a new one appears every 21-23 frames.

```
frame 700: x185  701: x192 ... 706: x227  707: x87 and x234 (second shot, same object)
```

At frame 1009 a shot spawns at x87 and dies at x136 on frame 1016: a bullet that
hits an enemy disappears well before the 23-frame limit.

The enemy's projectile is a different group: `$90BA` = tiles `$6F $71`, attrs
`$22 $22`, two sprites 8 px apart, moving **-2 px/frame** (frames 1011-1060,
1075-1096). It is not the player shot.

## Explosion / burst

Both effects use sprite palette 0 (white), sit still in world space (x drifts
left with the 0.5 px/frame scroll) and advance one step every **5 frames**.

Burst A (complete ring, two 8x16 sprites 7 px apart, one flipped):

| group | tiles | attrs | duration |
| --- | --- | --- | --- |
| `$9035` | `$81 $81` | `$00 $C0` | 5 |
| `$903E` | `$83 $83` | `$00 $C0` | 5 |
| `$9047` | `$85 $85` | `$00 $C0` | 5 |

Total 15 frames. Events observed at 741-755, 761-775, 870-884, 890-904,
996-1010, 1021-1035.

Burst B (debris ring, shrinking: pair, pair, single, single):

| group | tiles | attrs | duration |
| --- | --- | --- | --- |
| `$9050` | `$75 $77` | `$00 $00` | 5 |
| `$9059` | `$79 $7B` | `$00 $00` | 5 |
| `$9062` | `$7D` | `$00` | 5 |
| `$9067` | `$7F` | `$00` | 5 |

Total 20 frames. Events observed at 968-987, 1038-1057, 1065-1084, 1079-1098.

## What could not be clustered

* **`$31/$33` enemy.** A separate two-sprite object (groups `$90A8` attrs `$22`,
  `$90B1` attrs `$A2`) that stays in one pose for 8-60+ frames while drifting
  down-left. It is not a ring phase: the tracked ring object ran four full
  24-frame cycles without ever showing these tiles. Identical objects overlap on
  the same scanline, so nearest-neighbour tracking lost them.
* **Owner of the bursts.** Both burst types start where no enemy was visible on
  the previous frame (the enemy body is already gone), so OAM alone cannot say
  whether they are an enemy kill, a bullet impact or a boss-part effect.
* **Unused groups.** `$B7/$B9/$C1/$C3` (ring enemy, attrs `$21`, groups
  `$8EE2`-`$8EF1`) and `$59/$5B` (red explosion, `$8F29`/`$8F32`) are in the
  ROM's group table but never became visible in this window.
* **HUD.** The power-meter sprite (`$6D`, attr `$23`, x 248, stored y 206) is
  present every frame; it is static, not an animation.
