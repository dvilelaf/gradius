# Stage 1 data: enemy spawn tables

The stage-1 spawner is driven by three levels of tables in PRG. All addresses
are CPU addresses; the file offset in `Gradius (E).nes` is `address - $7FF0`.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-disasm

# the spawner and its dispatch (covered listing)
cargo xtask disasm "$ROM" --covered --labels --frames 900 \
  --press 180:start --assist --from 9A00 --to A900

# live spawner state: $6A/$6B is the current stream pointer, $19 the section
cargo xtask ramdump "$ROM" --watch 600:900 --press 180:start --assist \
  --range 0060:006F

# per-section $A65C terrain placements (the stage1_collision.rs fragment)
cargo xtask sections "$ROM"
```

## Table layout

| address | file off | contents |
| --- | --- | --- |
| `$A7CA` | `$27DA` | 8 words: per-section pointer to a column sub-table |
| `$A7D8` + 0x10·section | | 8 words: per-column stream pointer (section 0 is `$A7D8`) |
| `$A83E` … | `$284E` … | event streams, terminated by `$FF` |
| `$A5FC` | `$260C` | 4-byte object definitions, indexed by `type & $3F` |
| `$A65C` | `$266C` | 3-byte terrain/background definitions, indexed by `type` |
| `$A58C` | `$259C` | 2-byte enemy-size/type table used at `$A3DE` |
| `$9BC6`/`$9BCE` | `$1BD6`/`$1BDE` | per-section ship start position (tile-encoded) |
| `$9A30`/`$9A38` | `$1A40`/`$1A48` | per-section progress thresholds / next `$1B` |

`$A7CA` (read at `$A2CF`):

| section | sub-table | file off |
| --- | --- | --- |
| 0 | `$A7D8` | `$27E8` |
| 1 | `$A7E8` | `$27F8` |
| 2 | `$A7F8` | `$2808` |
| 3 | `$A806` | `$2816` |
| 4 | `$A814` | `$2824` |
| 5 | `$A822` | `$2832` |
| 6 | `$A830` | `$2840` |
| 7 | `$A83E` | `$284E` |

Each sub-table is 8 words, one per scroll column group (`$3F & $0E`, i.e. one
entry per two 8-pixel columns / 16 px). Section 0 (`$A7D8`):

| column | stream | file off |
| --- | --- | --- |
| 0 | `$A83E` | `$284E` |
| 1 | `$A853` | `$2863` |
| 2 | `$A874` | `$2884` |
| 3 | `$A89D` | `$28AD` |
| 4 | `$A8C0` | `$28D0` |
| 5 | `$A8E7` | `$28F7` |
| 6 | `$A8E7` | `$28F7` |
| 7 | `$A8E7` | `$28F7` |

Columns 5–7 share the end-of-section stream.

## Stream byte format

The parser is `$A32F`–`$A3DE`; the pointer lives in `$6A/$6B` and is advanced by
`$8407` (`ADC` into the 16-bit word at `$006A`).

* **2-byte record**: `<column_delay> <type>`, first byte `< $F0`. The delay is
  in 16-px columns; the spawner waits until `$3F >= $61 + 2*delay` and then
  consumes the record. Pointer advances 2.
* **5-byte record**: first byte `>= $F0` (commands, large objects). Bytes are
  copied to `$63-$67`, pointer advances 5, and `$64 -= $70`; `$FF` is the
  end-of-stream marker (checked at the first byte).
* `type >= $80` selects a 4-byte definition at
  `$A5FC + ((type & $3F) * 4)`; `type < $80` selects a 3-byte definition at
  `$A65C + type * 3`. The 4-byte copy reads one byte past a 3-byte definition,
  so 3-byte records overlap by one byte; this is observed in the code and is not
  a transcription error.

Section 0, column 0 (`$A83E`, file offset `$284E`) is eleven 2-byte records:
`$80` and `$81` alternate every 32 columns from delay `10` to `70`, four
records of `$82` follow from `A0` to `D0`, one more `$80` at `E0`, then the
`$FF` end marker.

Section 0, column 1 (`$A853`, `$2863`) is seventeen 2-byte records: the same
`$81`/`$80` alternation from delay `00` to `20`, `$82` from `30` to `50`, then
the larger enemies `$83`/`$84`/`$83` at `60`-`80`, three more `$82`, and the
terrain types `$00`/`$01`/`$02` from delay `C0` to `E0`; a 5-byte command
record starts at `F0`.

Types `$81/$82/$83/$84` index the 4-byte table; `$00/$01/$02` index the 3-byte
terrain table. `$A874` (column 2) ends with a 5-byte command record starting at
`$F0`.

## Object definitions (`$A5FC`)

Record bytes are copied to `$64-$67`; the spawner then maps them to object fields
at `$A455-$A490`:

| record byte | object field | meaning (inferred) |
| --- | --- | --- |
| `$64` | `$010C,X` | subtype / tile base |
| `$65` | `$030C,X` | object type |
| `$66` | `$03BC,X` | sub-state / timer |
| `$67` | `$03EC,X` | spawn flag |

First twelve records (index = `type & $3F`), at `$260C` onward: indices 0, 1,
5, 6, 9 and 10 are the small one-tile, five-frame scout; 2 and 7 are the wider
eight-frame sprite; 3, 4, 8 and 11 belong to the large four-byte family. The
port reads them from the ROM at run time (`stage1_tables`), so the bytes
themselves are not copied into the repository.

The stream types seen in section 0 (`$80`, `$81`, `$82`, `$83`, `$84`) therefore
select records 0-4: two `01 05 .. 00` scouts, one `00 08 02 04`, and the
`05 04 ..` family. Type `$83`/`$84` correspond to the wider/larger enemies.

## Terrain definitions (`$A65C`)

First ten 3-byte records (`type < $80`), at `$266C` onward. Records 1/2, 5/8
and 4/9 differ only in the middle byte (the attribute), and 6 and 7 are the
only two with the plain `$00` attribute. The three bytes are probably
`(tile/meta, attribute, palette)`; only the placement is proven.

Column 1 of section 0 uses types `$00`, `$01`, `$02` at delays `C0`, `C8`,
`D0`, i.e. terrain pieces placed near the end of the section. The three bytes
are probably `(tile/meta, attribute, palette)`; only the placement is proven.

## Ship start position (`$9BC6` / `$9BCE`)

`$9B82` computes `Y = L9BCE[($3F >> 1) + L9BC6[$19]]`; it writes
`Y & $F0` to `$0320-$0322` and `Y << 4` to `$0360-$0362`, i.e. the Y and X of
object slots 0-2 (the ship and the two leading trail slots). `$9BC6` is eight
per-section byte offsets into the tile-encoded `$9BCE` table, which stores Y in
its high nibble; the first entry of section 0 is tile `$65`, giving X `$50` (80)
and Y `$60` (96), which is exactly the measured ship position at the start
(`$0360=80`, `$0320=96`).

## Decoding script

The tables above were extracted with:

```python
rom = open(ROM, "rb").read()
def b(a): return rom[16 + a - 0x8000]
def w(a): return b(a) | (b(a + 1) << 8)

for section in range(8):
    sub = w(0xA7CA + 2 * section)
    print(section, [hex(w(sub + 2 * col)) for col in range(8)])
```

## Per-section terrain placements (`xtask sections`)

`$A7CA` has one sub-table per section; each word in a sub-table points at a
column stream. `xtask sections` walks those streams, treats a record whose type
is `< $80` as a `$A65C` terrain placement, and skips enemy records and the
5-byte command records. The result is the `Terrain { column, kind }` data in
`crates/gradius-core/src/stage1_collision.rs` (`SECTIONS`). `column` is the
record's stream delay, i.e. the 16-px column at which `$A32F` consumes it.

| section | sub-table | column streams | `$9A30` end | `$A65C` records |
| --- | --- | --- | --- | --- |
| 0 | `$A7D8` | 8 | `$0C` | 63 |
| 1 | `$A7E8` | 8 | `$0C` | 101 |
| 2 | `$A7F8` | 7 | `$0C` | 18 |
| 3 | `$A806` | 7 | `$0C` | 63 |
| 4 | `$A814` | 7 | `$0B` | 0 |
| 5 | `$A822` | 7 | `$0B` | 75 |
| 6 | `$A830` | 7 | `$0C` | 89 |

Section 7 (`$A83E`) is not a sub-table; it is the first stream of section 0.
Section 4 has no `$A65C` records: its streams place enemies only.

### Approximation

The port replays one 384x26 `stage1_bg::STAGE1` loop for every section, because
it clears `$3F` when a section ends. Background solidity is therefore the
section-0 `COLLISION_MAP` for all seven sections; sections 1-6 differ only in
their spawner terrain. The extracted records store placement evidence
(`column`, `$A65C` index) and no hitbox is inferred from them.

## Open questions

* The meaning of the 5-byte command records (`first byte >= $F0`) beyond the
  `$FF` terminator; `$A3DE-$A45F` sets up a larger object from `$63-$67` but was
  not exercised in the 900-frame trace.
* Whether the first byte of a 2-byte record is an absolute section column or a
  cumulative delay. The observed values in section 0 (`10, 30, 50, ...`) and the
  comparison at `$A300` (`$61 + 2*delay` versus `$3F`) fit an absolute column.
* Byte 2/3 semantics of the 4-byte definitions (`$66`/`$67`); the field mapping
  into the object arrays is proven, the gameplay meaning is not.
