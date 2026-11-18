# Sprite groups

The ROM stores multi-part sprites as small display lists. `$8AB5` looks up a
pointer and `$8AC5` walks the definition, adding the group's `(dy, dx)` to the
object's live position and copying each record into the OAM buffer.

## Table format

A definition is:

```text
count                        ; 0 = the entry draws nothing
count * (dy, tile, attr, dx) ; dy and dx are signed
```

`tile` is the top tile of an 8x16 sprite; the bottom tile is `tile | 1`
(the game stores the odd tile number, e.g. `$65` for the pair `$64/$65`).
`attr` is the raw OAM attribute byte (palette in bits 0-1, flips in 6-7).

Pointers live in two parallel byte tables with a two-byte stride (`$8AB1`
shifts the index left once):

| table | low/high bytes | entries | index range |
| --- | --- | --- | --- |
| primary | `$8D91`/`$8D92` | 128 (`$8D91`-`$8E90`) | 0-127 |
| extended | `$8E91`/`$8E92` | 36 (`$8E91`-`$8ED8`) | 0-35 |

That is **164 entries, 157 of them non-empty** (empty entries still point at a
count-0 definition). Definitions themselves start at `$8ED9`; earlier
definitions are interleaved before the tables.

## Notable entries

| group | definition | sprites | contents |
| --- | --- | --- | --- |
| 1 | `$8F07` | 4 | ship animation frame, tiles `$09/$0B/$0D` + engine `$DF` |
| 2 | `$8F18` | 4 | ship animation frame, tiles `$0F/$11/$13` + engine `$DF` |
| 3 | `$8EF6` | 4 | ship animation frame, tiles `$03/$05/$07` + engine `$DF` |
| 7 | `$8F4A` | 2 | HUD power-meter bar, tile `$6D` twice |
| 8 | `$8F53` | 1 | player shot, tile `$61` |
| 9, 10 | `$8F58` | 1 | player shot, tile `$65`, attr `$22` |
| 90-93 | `$8EE2`-`$8EF1` | 1 each | ring enemy animation, tiles `$B7/$B9/$C1/$C3` |
| 120 | `$9193` | 6 | large enemy / stage-1 Big Core, tiles `$D1/$D3/$D5` |
| ext 34 | `$95EE` | 18 | largest group in the ROM |

**Correction to the task wording:** entry 8/9 is the player shot, not the
ship. The ship's three 8x16 columns are the primary-table entries 1/2/3
(`$8F07`/`$8F18`/`$8EF6`, four sprites each: three hull columns plus the engine
flame).

## Verification against the oracle (frame 800)

`cargo xtask probe "$ROM" --frame 800 --press 180:start --sprites 64`:

* Ship: OAM slots 56/41/26 show `$0D`/`$0B`/`$09` at x 88/80/72, matching the
  `x = +8/0/-8`, attr `$20/$20/$21` records of ship group 1 (`$8F07`).
* Ring enemy: OAM slot 4 shows tile `$C1`, attr `$21` at (95, 95), matching
  group 92 (`$8EEC`: `dy=-1`, `tile=$C1`, `attr=$21`, `dx=15`).
* Shot: OAM slots 1/16/31 show `$65`, attr `$22`, matching group 9/10
  (`$8F58`).

## Rendering

`cargo xtask sprite-groups "$ROM" --out sheet.png --json groups.json`

The sheet has one row per group (label `#table:index count`) and one column per
CNROM hardware bank (sprite pattern-table half `$1000`). Sprites are decoded
from the cartridge CHR; color 0 is transparent and the remaining pixel values
are tinted per bank. The JSON lists every entry, including empty ones, with
signed offsets and the definition address.
