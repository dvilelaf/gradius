# The five-byte command record (`$A374`-`$A4F9`)

The stage column streams carry two record shapes:

* `<delay> <type>` — two bytes. `$A345` compares the second byte with `$F0`.
  Types below `$F0` go through the `$A5FC` object table (`$A367`) or the
  `$A65C` terrain table (`$A354`).
* `<delay> <$F0+> <$65> <$66> <$67>` — five bytes. `$A345` takes the `BCS`
  branch to `$A374`, which copies all five bytes to `$63-$67`, advances the
  stream pointer by five (`$A382`/`$A384`), subtracts `$70` from `$64`
  (`$A387`), and jumps to `$A460`.

`$A460` dispatches on `$19`:

```
$A460  LDA $19
$A462  CMP #$02
$A464  BEQ $A469      ; stage 3 (zero-based section 2)
$A466  JMP $A4A0      ; every other stage
```

## The bytes after `$A387`

| byte | source | meaning |
| --- | --- | --- |
| `$63` | stream byte 0 | delay/trigger within the column. Consumed by `$A32F`; `$A460` never reads it. |
| `$64` | stream byte 1 minus `$70` | stage 3: subtype / tile base (`$80-$83`). Other stages: unused; `$A4DB` hardcodes `$010C = $80`. |
| `$65` | stream byte 2 | stage 3: Y. Otherwise: two 4-bit attachment groups. |
| `$66` | stream byte 3 | stage 3: `$03BC`. Otherwise: kind (`$030C`). |
| `$67` | stream byte 4 | stage 3: `$03EC`. Otherwise: Y (`$032C`). |

The command never carries an HP field. `$046C` is the per-object hit counter,
cleared to zero by `$A521` and incremented by the collision pass (`$C1C4`);
handlers compare it with their own constants. Nothing in `$63-$67` seeds it.

## `$19 == 2` (stage 3), `$A469-$A49F`

The path scans slots 9 down to 0 for a free `$030C`, sets `$5D = 1`, clears the
slot with `$A521`, then writes:

```
$A481  $010C,X = $64    subtype
$A486  $032C,X = $65    Y
$A48B  $03BC,X = $66
$A490  $03EC,X = $67
$A495  $030C,X = $96    kind
$A49A  $036C,X = $F0    X
```

`$A521` leaves `$0460,X` (size class), `$012C,X` (sprite) and `$04AC,X` at zero;
this path does not set them. `$03BC:$03EC` is the 16-bit big-endian value used
by the `$C900` handler: `$03BC` is the high byte and `$03EC` the low byte
(`$C9FE-$CA0C` adds `$20` to the pair). `$C900` gates on `$046C >= 3`
(`$C909-$C910`), which is its damage counter, not a command argument.

## Every other stage, `$A4A0-$A4F9`

`$A4A0` finds a free object slot and a free `$0600` attachment struct. The
structs are at `$0600+$90`, `+$60`, `+$30`, `+$00` (`$A4B5`/`$A4BE`), scanned
high to low.

`$A4FA-$A51E` consumes `$65` one nibble at a time, low nibble first, and
`$A4C7-$A4CF` shifts `$65` right four bits after each group:

* a zero nibble allocates nothing;
* a non-zero nibble allocates one `$0600` struct:
  * `$0600+X` = parent slot (`$A506-$A508`);
  * `$0601+X` = nibble minus one (`$A4FA-$A503`);
  * `$0602`, `$0610` and `$0618` runs are cleared.

After the shift loop the parent is written at `$A4D1-$A4F9`:

```
$A4D6  $030C,X = $66    kind
$A4DB  $010C,X = $80    subtype
$A4E0  $036C,X = $F0    X
$A4E5  $032C,X = $67    Y
$A4EA  $012C,X = $89    sprite
$A4EF  $016C,X = $98    number of $0600 records allocated (0-2)
$A4F4  $0460,X = $01    size class
```

`$04AC` is not written here; `$A521` cleared it. `$98` counts the records, so
`$65 = $12` (low 2, high 1) allocates two records and leaves `$016C = 2`, while
`$65 = $01` allocates one and leaves `$016C = 1`.

### What the two views of `$65` miss

* `enemy_ai.md` says `($65 >> 4)` attached records. That is only the high-nibble
  group and drops the low-nibble group.
* `game.rs command()` uses `bytes[2] & 0x0F`. That is only the low-nibble
  group and drops the high-nibble group, and it stores the value in `ai_mode`
  (`$04AC`), which the command path does not write.

The exact rule is `records = u8::from(low != 0) + u8::from(high != 0)`, with the
individual group size `nibble - 1` in that group's `$0601`.

## Oracle reproductions

Both runs start from reset, press Start at frame 180, and keep the invulnerability
counter at 5 so the stage scrolls:

```sh
cargo xtask probe "Gradius (E).nes" --frame 4000 --press 180:start \
    --keep-alive --poke 400:19:02 --ram /tmp/s3.ram

cargo xtask probe "Gradius (E).nes" --frame 2610 --press 180:start \
    --keep-alive --poke 400:19:04 --ram /tmp/s5.ram
```

`$19` is the live stage section byte; poking it after stage 1 has started makes
the spawner read the stage-3/stage-5 column streams. `$26` (the per-player copy
copied to `$19` at stage init) is the alternative used by `cargo xtask screens`.

### Stage 3, raw `[9C F1 60 21 C5]`

At frame 4000 RAM holds `$63-$67 = 9C 81 60 21 C5` (the raw marker `$F1` has
been biased to `$81`). Slot 8:

```
$010C=81  $030C=96  $032C=60  $036C=EC
$03BC=21  $03EC=C5  $0460=00  $012C=00
```

That is subtype `$64`, kind `$96`, Y `$65`, `$03BC` `$66`, `$03EC` `$67`, and
X drifted from `$F0` by four half-pixels. Slot 9 at the same frame came from
raw `[9C F3 38 21 25]` and holds `$010C=83 $030C=96 $032C=38 $03BC=21
$03EC=25`.

### Stage 5, raw `[40 F0 01 14 80]`

At frame 2610 RAM holds `$63-$67 = 40 80 00 14 80`; `$65` is `0` because the
`$A4C7` loop shifted it away after consuming the low nibble. Slot 9:

```
$010C=80  $030C=94  $032C=7C  $036C=EB  $012C=82
$016C=01  $0460=01  $046C=00
$0600[$90]=09  $0601[$90]=00
```

The `$A460` kind `$14` becomes `$94` after the `$CA58` handler sets the high
bit. Y started at `$67 = $80` and is moving; X started at `$F0`. `$016C = 1`
and the single `$0601 = 0` are the low nibble `1` (one record, count 0). At
frame 2900 the two-group record `[80 F0 12 14 40]` has left a parent with
`$016C = 2` and two `$0600` records, matching `$65 = $12`.

## The precise `game.rs command()` fix

The stage-3 branch already extracts `$64-$67` correctly (including
`from_be_bytes([bytes[3], bytes[4]])`, which matches the `$03BC` high /
`$03EC` low pair). Two corrections remain:

1. In the non-stage-3 branch, replace

   ```rust
   let attachments = bytes[2] & 0x0F;
   ```

   with the two-nibble rule:

   ```rust
   let low = bytes[2] & 0x0F;
   let high = bytes[2] >> 4;
   let records = u8::from(low != 0) + u8::from(high != 0);
   ```

   and set the parent's `$016C` (`Object::anim` in the port) to `records`, not
   `ai_mode`. If the `$0600` structs are modelled, allocate one per non-zero
   nibble, low first, with `$0600 = parent slot` and `$0601 = nibble - 1`.

2. The ROM's `$A469` path does not set `$012C` (sprite), `$0460` (size class)
   or any `boss` flag. The port's `sprite: 0x89`, `size_class: 1` and
   `boss: true` there are port inventions; drop them if ROM fidelity is the
   goal. The larger mismatch is that the port dispatches kind `$16` to
   `ai_big_core` (`$B9A5`) instead of the `$C900` handler this command path
   feeds; that is a separate core change.

`cargo xtask command-format --stage 4 40 F0 01 14 80` prints this decoding, and
`xtask/src/command_format.rs` pins the two layouts with unit tests.
