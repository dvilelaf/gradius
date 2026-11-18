# Power-up visuals in the original (oracle captures)

Six controlled oracle runs, one per power-up, with the meter selection poked
and B held so the ROM's own handler runs. Screenshots, raw OAM images and RAM
dumps are in `/tmp/opencode/oracle-powerups/`:

| power-up | `$42` | screenshot | OAM dump | capture frame |
| --- | --- | --- | --- | --- |
| SPEED | 1 | `speed.png` | `speed-oam.bin` | 960 |
| MISSILE | 2 | `missile.png` | `missile-oam.bin` | 971 |
| DOUBLE | 3 | `double.png` | `double-oam.bin` | 953 |
| LASER | 4 | `laser.png` | `laser-oam.bin` | 971 |
| OPTION | 5 | `option.png` | `option-oam.bin` | 960 |
| SHIELD | 6 | `shield.png` | `shield-oam.bin` | 960 |

The matching `<name>.txt` files are the full `probe` reports (decoded OAM,
palette and framebuffer hash); `<name>-ram.bin` is the 2 KiB CPU RAM image.

## Driving the ROM

The meter byte alone does nothing: `$8979` dispatches `$42` only while
`$0100 == 1` and B is held. Every run below boots with `--press 180:start`,
holds B for the whole run with `--buttons b`, pokes the selection at the start
of frame 900 and holds A from frame 902:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-oraclepower

cargo xtask probe "$ROM" --frame 971 --press 180:start --buttons b \
  --poke 900:0042:02 --press 902:a:70 \
  --oam /tmp/opencode/oracle-powerups/missile-oam.bin \
  --png /tmp/opencode/oracle-powerups/missile.png \
  --ram /tmp/opencode/oracle-powerups/missile-ram.bin
```

`--buttons b` is used rather than a timed `900:b` press because B must be held
when `$42` is written; a held B is harmless once the handler clears `$42`.
`--press 902:a:70` is the "hold A/fire for ~60 frames" part (`--press` supports
`a+up:70` for the OPTION run). No `--keep-alive` is used: the ship survives the
whole window in these runs, and that hook would pin `$46 = 5` and add a shield
to every capture.

At the end of every run `$42 == 0` and `$0100 == 1`; the active state byte is
in the effect field listed below. `$40..$46` after the run:

| run | `$40` | `$41` | `$42` | `$44` | `$45` | `$46` | screenshot player x/y |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SPEED | 1 | 0 | 0 | 0 | 0 | 0 | 168/96 |
| MISSILE | 0 | 1 | 0 | 0 | 0 | 0 | 80/96 |
| DOUBLE | 0 | 0 | 0 | 2 | 0 | 0 | 80/96 |
| LASER | 0 | 0 | 0 | 1 | 0 | 0 | 80/96 |
| OPTION | 0 | 0 | 0 | 0 | 1 | 0 | 80/39 (moved up) |
| SHIELD | 0 | 0 | 0 | 0 | 0 | 5 | 80/96 |

## SPEED: no sprite, only `$40`

`$89A6` does `INC $40` and no visual is drawn. Baseline (`$40 = 0`) and speed
(`$40 = 1`) runs used the same input, with Right held from frame 900:

```sh
cargo xtask probe "$ROM" --frame 960 --press 180:start --buttons b \
  --press 900:right:61 --poke 900:0042:01 --ram speed-ram.bin
```

Measured ship X (`$0360`) at ten-frame intervals, same deterministic script:

| frame | `$40 = 0` | `$40 = 1` |
| --- | --- | --- |
| 900 | 80 | 80 |
| 910 | 89 | 93 |
| 920 | 99 | 108 |
| 930 | 109 | 123 |
| 940 | 119 | 138 |
| 950 | 129 | 153 |
| 960 | 139 | 168 |

Over frames 900-960 the baseline moves 59 px and the upgraded ship 88 px
(about 1.0 vs 1.5 px/frame). The screenshots differ only in the ship position:
SPEED writes no OAM entries, so a still frame cannot show it.

## MISSILE: tile `$65`, palette 2, below the ship

`$41 = 1`. The ship emitter writes global missile slot 9: `$0129 = 0A`
(sprite frame/group), `$0169 = 03`, `$0369 = 59` (89), `$0329 = 8C` (140).
Frame 971 OAM:

```
slot 4  x 89  stored y 138  top 139  tile $65  attr $22
```

Sprite frame `$0A` resolves to group 10 (`$8F58`, one sprite, tile `$65`,
attr `$22`). The visible missile is that single sprite; the other `$65` entries
in the same OAM are off-screen stale slots (`y = $F4`). It spawns six pixels
below the ship and by frame 971 has fallen down-right to top 139, as documented
in `missile.md`.

## DOUBLE: straight shot plus a rising shot

`$44 = 2`. The ROM emits one normal forward shot and one rising shot per volley:

* slot 3: `$0123 = 06`, `$0163 = 00` - group 6 (`$8F3B`), tile `$6B`,
  attr `$22`, `dy = 7`, `dx = 0`; moves **+7 px X/frame**.
* slot 6: `$0126 = 24` (36), `$0166 = 02` - group 36 (`$8F40`), tile `$73`,
  attr `$22`, `dy = 0`, `dx = 8`; kind 2 moves **-4 px Y and +4 px X/frame**
  and despawns below `y = $10`.

Frame 953 OAM:

```
slot 32  x 120  stored y 64   top 65   tile $73  attr $22   (rising)
slot 47  x 115  stored y 103  top 104  tile $6B  attr $22   (straight)
```

So DOUBLE is not two parallel bullets: the second bullet climbs away from the
first. That is why it leaves the top of the screen so quickly; frame 959 in
this run already has only the straight `$6B` shot.

## LASER: group 7, a 16-pixel beam made of two `$6D` sprites

`$44 = 1`. Shot slot 3 holds `$0123 = 07`, `$0163 = 01`. Group 7 (`$8F4A`)
contains two sprites:

```
dy 7, tile $6D, attr $21, dx 0
dy 7, tile $6D, attr $21, dx 8
```

Frame 971 OAM, relative to the ship at x 80 / top 97:

```
slot 19  x 128  stored y 103  top 104  tile $6D  attr $21
slot  4  x 136  stored y 103  top 104  tile $6D  attr $21
```

The pair is one 16-px laser beam. Kind 1 advances it **+12 px X/frame** and
despawns at `x >= $F0`. The same group 7 definition is also used for the HUD
power-meter bar, so the laser is literally reusing the HUD's two-sprite
definition.

## OPTION: groups 4/5, two sprites per option

`$45 = 1`. An option is one 16x16 object made of two 8x16 sprites. The
animation toggles between groups 4 (`$8F29`, tiles `$59`) and 5
(`$8F32`, tiles `$5B`); both have `dx = -8` attr `$23` and `dx = 0` attr `$63`,
palette 3, behind background. At rest the option sits on the ship, so the
capture used `--press 902:a+up:70` to separate it vertically. Frame 960 OAM:

```
slot 24  x 72  stored y 51  top 52  tile $5B  attr $23
slot 39  x 80  stored y 51  top 52  tile $5B  attr $63
```

The ship is in its up-facing pose at x 72/80/88, top 45; both option sprites
are 7 px below the ship, trailing the upward move. `$0121 = 05` is the option
sprite frame selecting group 5. At frame 940/950 the same slots show group 4
tile `$59` instead (`$0121 = 04`), the option's two-frame blink.

## SHIELD: one visible ring sprite from groups 90-93

`$46 = 5`. Frame 960 OAM:

```
slot 39  x 95  stored y 95  top 96  tile $C1  attr $21
```

The shield is a single 8x16 sprite at ship position `(+15, -1)`: the ship is
at x 72/80/88, top 97, so this tile sits at the right edge. Groups 90-93
(`$8EE2`, `$8EE7`, `$8EEC`, `$8EF1`) are four one-sprite definitions with the
same `dx = 15, dy = -1` and tiles `$B7/$B9/$C1/$C3`, attr `$21`. The draw path
`$8B70-$8B8B` reads `$46`, skips while `$1B & $70 != 0`, and calls group
`$5A + (($02 >> 2) & 3)` - exactly one of groups 90-93 per frame - which is
why only one ring tile is visible. The four `$C1/$C3` entries in the frame are
that visible sprite plus three stale off-screen copies at the same X.

A direct `--poke 900:0046:05` at frame 900, with no `$42` poke and no B press,
reproduces the same sprite, so the visual is driven by `$46` itself and not by
the meter handler. The original does not draw four simultaneous ring sprites
in these captures; only one tile is visible per frame.

## Summary

* SPEED shows nothing in OAM; only `$40` and the measured X movement above.
* MISSILE: `$65/$22`, one sprite, group 10.
* DOUBLE: `$6B/$22` straight (group 6) plus `$73/$22` rising (group 36).
* LASER: two `$6D/$21` sprites 8 px apart, group 7.
* OPTION: two `$5B/$23,$63` sprites (groups 4/5), one option.
* SHIELD: one `$B7/$B9/$C1/$C3` `$21` ring sprite from groups 90-93 at ship
  `(+15, -1)`.
