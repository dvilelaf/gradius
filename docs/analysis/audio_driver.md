# The Konami sound driver at `$ECFC`

This note maps the cartridge's sound engine and the native prototype in
`gradius_audio::driver`. The prototype replays extracted sequences without a
6502; the existing register-timeline player is untouched.

## Entry points

| Address | Role |
| --- | --- |
| `$EC18` | Sound request. `A` is the request id. Pushes X/Y, starts up to four channel blocks from `$EFC7`, returns. |
| `$ECFC` | Frame update. Called once per NMI from `$8099`, after PPU/controller work. |
| `$ED40` | Per-channel update: duration countdown, command decode, note write. |
| `$EE2F` | Sustain path: software volume fade while a note is held. |
| `$EDB8` | Effect/raw-period parser, selected by `$09` low bits. |
| `$EE7C` | Music parser, selected by `$09` low bits. |
| `$EFB2` | Twelve-entry period table, big-endian: `06AE 064E 05F4 059E 054E 0501 04B9 0476 0436 03F9 03C0 038A`. |
| `$EFC7` | Request table. Three bytes per entry: register offset, pointer low, pointer high. |

`$ECFC` walks four channel blocks in this order:

| Block | APU registers | Register offset |
| --- | --- | --- |
| `$B0` | `$4000-$4003` pulse 1 | 0 |
| `$C1` | `$4004-$4007` pulse 2 | 4 |
| `$D2` | `$4008-$400B` triangle | 8 |
| `$E3` | `$400C-$400F` noise | 12 |

The blocks are 17 (`$11`) bytes apart. After the fourth block the driver
services the global tempo counter (`$F0-$F3`) and returns.

## Request table and priority

An id's entry index is `(id & $3F) * 3`; the high two bits of the id give the
number of parts minus one (`id >> 6 + 1`). Each part stores:

* byte 0: APU register offset (`0`, `4`, `8`, `$0C`), shifted right twice to
  index the channel table at `$ECAC` (`B0 C1 D2 E3`);
* bytes 1-2: little-endian CPU address of the sequence.

The request id (low six bits) is stored in `$02,X` and doubles as priority. A
new request replaces a channel only when its priority is greater than or equal
to the current one (`$EC43`). A request that lands on `$C1` clears `$F0-$F2`,
restarting the global tempo counter.

Channel start (`$EC47-$EC8D`) sets `$00,X = 1`, `$06,X = 0`, clears the period
cache and sweep fields for non-triangle channels, and writes silence:

* pulse/noise: `$30` to both `$4000+offset` and `$4001+offset`;
* triangle: `0` to `$4008` and `$4009`.

It also inspects the first sequence byte: high nibble `$2` sets `$09,X = 0`
(effect/raw parser), anything else sets `$09,X = 1` (music parser).

## Channel state

| Offset | Meaning |
| --- | --- |
| `$00` | Frames remaining in the current note/rest. |
| `$01` | Default duration set by a `$2x` effect prefix. |
| `$02` | Owning request id / priority, `0` when free. |
| `$03/$04` | Current sequence pointer. |
| `$05` | Instrument byte written to `$4000` (duty, halt, constant volume, volume). |
| `$06` | `$FE` repeat counter. |
| `$07` | Cached high byte of the last period write. |
| `$08` | Current `$4000` value after envelope/fade updates. |
| `$09` | Bit 7: an `$FD` call is active. Low bits select the parser (0 effect, 1 music). |
| `$0A` | Tempo multiplier from the low nibble of `$D0-$DF`. |
| `$0B` | Cached `$4001` sweep value. |
| `$0C` | Offset added to the period low byte (`$4002`). |
| `$0D` | Fade countdown. |
| `$0E` | Fade start delay (`$Dx` parameter high nibble). |
| `$0F` | Fade step (`$Dx` parameter low nibble). |
| `$10` | Octave, 0 highest. Period is shifted right by `4 - octave` (octaves 4 and above do not shift). |

Globals: `$F0` enables the game's tempo scaling, `$F1` counts frames, `$F2`
increments every `$30` frames, `$F3` is the tempo limit; `$F4/$F5` hold a
temporary period, `$F6/$F7` the request start pointer, `$F8/$F9` the current
block/register offset, `$FA/$FB` the current sequence pointer, `$DD/$DE` a
one-level call return address, and `$15` pauses the music.

## Sequence bytecode

Every command starts with a high nibble. Commands advance the pointer and may
consume extra bytes.

### Music parser (`$09` low bits non-zero)

* `00-BF` note: high nibble is the pitch (`0` = C, `B` = B) and low nibble is
  the duration. `C0-CF` is a rest (pitch `C`), written as `$30` to the channel's
  `$4000`. Duration in frames is `tempo` when the nibble is 0, otherwise
  `tempo * (nibble + 1)`.
* `D0-DF` instrument plus note: low nibble is the tempo, next byte is the
  `$4000` config, next byte is only for pulse/noise and splits into fade delay
  (high nibble) and fade step (low nibble). Triangle consumes only the config
  byte and parses the next command in the same frame. If the next byte is
  `E0-EF`, its low nibble is the octave and the following byte is the note;
  otherwise the byte after the params is the note.
* `E0-EF`: low nibble is the octave, the next byte is the note (high nibble
  pitch, low nibble duration).
* `FD lo hi`: one-level call. Saves the address after the operands, jumps to
  `hi:lo`; `$FF` returns.
* `FE count lo hi`: repeat. `count` is total passes. The driver adds one to
  `$06,X`, compares, and continues after the four bytes on equality; otherwise
  it jumps to `hi:lo`. The comparison is the sign bit of `next - count`, so
  `$FF` resets the counter to zero every pass and loops forever instead of
  falling through after 255 passes. The native driver mirrors the bit-7 test.
* `FF`: return from an active call, otherwise free the channel and write `$30`
  (triangle: `0` to `$4008`).

Notes are written in this order: the config/volume byte first, then `$4003`
(high period with length-counter bit 3 set) and `$4002` (low period). The high
byte is skipped when it equals `$07,X` and the volume byte has bit 4 set with no
sweep, which is why many repeated notes only write the low register.

A note whose pitch nibble is `$F` (`$F0-$FC`) indexes past the twelve-entry
period table: `$EFB2 + $1E = $EFD0`, inside the request table (the start of
entry `$03`). Its period is therefore whatever those cartridge bytes are, not
zero. The native `period` lookup reads the same address from the memory image.

### Effect parser (`$09` low bits zero)

Used by the sound effects, whose first byte has high nibble `$2`.

* `20-2F`: sets `$01` (default duration) from the low nibble and writes the
  next byte to `$4000`/`$08`.
* `10 value`: writes `value` to `$4001` and caches it in `$0B` (sweep).
* `11 value`: stores `value` in `$0C` and continues parsing the next command.
* Any other byte is a raw-period pair: low three bits of the command are the
  period high byte and the next byte is the period low byte. `$0C` is added for
  pulse channels (carrying into the high byte), then the pair is written. When
  `$08` bit 4 is set, the command's high nibble (or the byte after `$F8`) is
  or-ed into the volume high nibble after masking `$08` with `$F0`.

## Instrument envelope

`$D0-$DF` writes the APU config to `$05`/`$08` and the `$4000` register. For
pulse and noise, `$0D = duration + $0E`. On each held frame `$0D` is decremented
once (twice when it does not equal the remaining duration); when the countdown
crosses zero the low volume nibble of `$08` is decremented by one and `$4000`
is rewritten. The `$0F` nibble skips a fade step by subtracting from the
countdown. A previous `Cx` rest suppresses the fade, and the triangle has no
software envelope.

## Tempo and frame counter

The driver is clocked by the NMI at 60.0988 Hz. Note lengths are counted in
frames by `$00,X`, decremented at `$ED4A`; a parse happens when it reaches zero.

The global `$F1` counter advances once per frame only while `$F0` is set (the
game sets it in gameplay at `$8399`). When `$F1` reaches `$30` (48) the driver
increments `$F2`, clears `$F1`, and, when the tempo limit `$F3` is below 7,
clears `$D4` (the triangle's owning id) and writes `0` to `$4008`/`$4009`. A
request starting on `$C1` clears `$F0-$F2`.

Every pulse-2 note while `$F0` is set applies the branch at `$EEE0-$EF1E`: it
clamps `$F2` to `$0B`, sets the fade delay to 6 and the fade step to `$0D` when
game mode `$C3` is `$13` (otherwise `$05`), then sets
`$F3 = max(0, ($05 & $0F) - $F2)` and uses `($05 & $F0) | $F3` as the volume
when that byte is non-zero. Pulse 1, triangle and noise are unaffected.

`Driver::set_tempo_scaling(true, mode)` enables the branch and supplies `$C3`.
The captured timelines come from the injected quiet driver with `$F0 = 0`, so
they do not exercise it; the cartridge comparison does.

## The pack carries the driver

`gradius_audio::driver::extract_source` copies the request table at `$EFC7`,
the period table at `$EFB2` and every sequence byte the fourteen supported
requests read into the pack's chunk (audio chunk version 2). The extraction
runs a traced driver for each request until its read set has been stable for
6,000 frames, excludes the table ranges, and merges adjacent addresses into
segments. For Gradius (E) that is 1,276 sequence bytes in 10 segments and a
1,564-byte `KDRV` payload. The four 17-byte channel blocks are runtime state,
so only their base addresses and register offsets are recorded.

`xtask extract` writes that section; `AudioBank::from_chunk` reconstructs a
`$C000` memory image with the tables and segments, leaks it once, and
`AudioBank::driver()` returns a fresh native driver over it. A version 1 chunk
or an empty driver section leaves `driver()` as `None` and the timelines remain
the fallback.

## Validation against the oracle

`crates/gradius-audio/src/bin/driver_check.rs` (feature `capture`) extracts the
request table and sequence bytes from the cartridge at run time, runs the
native `Driver`, and compares every APU write with a quiet `gradius-nes` run of
`$ECFC`:

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-audiodrv \
cargo run -p gradius-audio --features capture --bin driver-check -- \
  "/path/to/Gradius (E).nes" 90 06 60
```

Results (60 frames, before any ROM byte is committed):

* title fanfare `$90` (three channels): `0/112` writes differ, `0/60` frames;
* shot effect `$06` (one channel): `0/33` writes differ, `0/60` frames;
* effects `$01`, `$05`, `$09`, `$0D`, `$0E`, `$0F`, `$35`, `$36`: `0` writes
  differ over 60-90 frames each.

`crates/gradius-audio/tests/driver_pack.rs` repeats this from the pack path when
`GRADIUS_ROM` is set: driver-from-pack equals driver-from-ROM for all fourteen
requests (zero writes), and against the committed captures the counts are title
`0/112` after the sliced title's 1 s lead-in, stage 1 `0/2347`, stage 1-B
`0/2272`, Big Core `0/774`, stage clear `0/386`, game over `0/699`, and `0` for
every effect.

The former 21 stage-1-B differences were traced to the `$FE` repeat counter: a
`$FE $FF` block was treated as 255 passes and fell through late in the track.
The bit-7 comparison above makes it loop like the cartridge and leaves the
period aliases, which already read the right bytes, unchanged. The cartridge is
read by the development tools and the opt-in test only and is never embedded.
