# Stage 1 code flow

Static and dynamic evidence for the stage-1 gameplay routines. Addresses are CPU
addresses in the fixed `$8000-$FFFF` PRG window; the file offset in
`Gradius (E).nes` is `address - $7FF0` (16-byte iNES header). The listing was
produced with

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-disasm

cargo xtask disasm "$ROM" --covered --labels --frames 900 \
  --press 180:start --assist --out stage1.disasm
```

`*` marks executed instructions; `; data` marks bytes the trace never reached.
The trace reached 2222 distinct PCs, so the listing is evidence for what actually
runs, not just what is reachable statically.

## State recorder and sprite-0 guard

`xtask record-state` writes one line per frame in exactly the
`Game::dump_state` format of `gradius-core`, so an oracle recording can be
diffed against the native harness:

```sh
cargo xtask record-state "$ROM" --frames 300 --press 180:start --out state300.txt
cargo xtask record-state "$ROM" --frames 3000 --press 180:start > state3000.txt
```

The first 300 frames produce 35,135 bytes (about 117 bytes per line); a release
build records about 3,380 frames/second on the analysis machine. Line fields
are described in the command help. Two of them have no ROM source: `capsule` is
always 0 because the capsule count is port-side HUD state, and `state` collapses
the `$00` dispatcher byte (0-2 attract/demo -> `Title`, 3 -> `GameOver`, 4-6 ->
`Playing`). `objects` uses `$048C` as `substate`, matching the native core.

`xtask verify-sprite0` runs the stage with no `--assist` for 3000 frames and
fails unless the HUD split at `$9A96`/`$9A9D` is satisfied: it prints the count
of `$2002` reads that saw status bit 6, the poll counts at both addresses and
the scroll progress. The current PPU model raises the real sprite-0 hit, so
`--assist` is only needed to reproduce traces made before that fix.

## Boot, NMI and frame loop

| address | file off | routine |
| --- | --- | --- |
| `$8010` | `$0020` | reset |
| `$832E` | `$033E` | PPU/APU init helper |
| `$81A3` | `$01B3` | APU init |
| `$83B0` | `$03C0` | PPU warm-up / palette upload wrapper |
| `$8515` | `$0525` | title/attract entry |
| `$8067` | `$0077` | idle main loop (`JMP $8067`) |
| `$806A` | `$007A` | NMI handler |
| `$80B6` | `$00C6` | per-frame game-state dispatch |
| `$83E9` | `$03F9` | inline jump-table dispatcher |

Reset (`$8010`): `CLD/SEI`, wait two vblanks through `$2002` bit 7, `JSR $832E`,
`TXS #$FF`, clear `$0700-$07FF` via the `($98),Y` pointer loop
(`$8023-$8033`: `$99=7`, then 8 pages of `$00`), validate `$0700-$07EF` against a
pattern, clear `$0000-$00FF`, `$81A3` (APU), `$83B0` (PPU), `$8515` (title), then
`$8067: JMP $8067`. **All gameplay work happens in NMI**; the main loop is idle.

NMI (`$806A`) pseudocode:

```
push A/X/Y, LDA $2002
if $04 != 0: goto restore          ; re-entry guard: NMI work already done
JSR $8335                          ; set PPU addr/latch from $2D/$??
STA $2003                          ; OAM address 0
LDY #2; STY $4014                  ; OAM DMA from $0200
LDA $11; LDX $0D; if $0D: DEC $0D; if !=0 LDA #0
STA $2001                          ; PPUMASK (blank while $0D counts down)
JSR $8A56                          ; flush $0700 VRAM command queue
JSR $8279                          ; palette/attribute upload
INC $04
JSR $ECFC                          ; APU/sound engine
JSR $81B7                          ; controller read -> $05 edge, $07 held
JSR $8B15                          ; build OAM from object arrays
JSR $80B6                          ; game state dispatch (all gameplay)
JSR $8BB0                          ; OAM group/sprite refresh
JSR $8646                          ; queue a HUD/VRAM command
STY $04 = 0
pop Y/X/A, RTI ($80B5)
```

The NMI's own OAM builder writes `$0200-$02FF`; `$8A56` consumes the `$0700`
queue (records terminate with `$FF`), and `$8B15` walks object slots 0-31:
active at `$0120,X`, sprite frame at `$0180,X`, position from `$0320,X`/`$0360,X`.

## Game state machine

`$80B6`: `INC $02`; if state < 3 and Start-edge (`$05 & $10`) not set, poll the
menu helper `$8212`; then `LDA $00; JSR $83E9`. The inline table at `$80CC`:

| index | handler | meaning |
| --- | --- | --- |
| 0 | `$80DA` | attract/title |
| 1 | `$810E` | title → demo transition |
| 2 | `$8119` | demo/attract play |
| 3 | `$812F` | game-over / continue countdown |
| 4 | `$815D` | enter stage |
| 5 | `$9643` | **stage 1 handler** |
| 6 | `$8164` | ending |

`$83E9` doubles A, pops the JSR return address, reads the word at
`return + 2*A` and jumps through `$0098`. The table therefore sits immediately
after the `JSR`: `$80CC` for `$80B6`, `$96B8` for the stage handler, `$9822` for
the `$1B` sub-state, `$898E` for the power meter.

## Stage handler entry (`$9643`, file `$1653`)

```
$13 = $0C                    ; scroll speed constant
$5B = $5C = $5D = 0
if $15 != 0: JMP $9A7F      ; pause
if $19 == 4: count non-zero $0600/$0630/$0660/$0690 -> $5C
if $1B & $70 == 0: JSR $83E9 with table $96B8
else if $1B & $80: JMP $981D
```

`$1B` is the stage state byte:

| value | action |
| --- | --- |
| 0 | `$9B38` clear RAM/object tables, draw stage, `$1B=1` |
| 1 | `$9BE7`/`$9BEA` HUD/status text, `$1B=2` |
| 2 | `$9C0C` palette + HUD setup (`$88BB/$88FB/$8931`), `$1B=3` |
| 3 | `$9C18` `JSR $89E8` power-meter init, `$1B=4` |
| 4 | `$9C1E` four `JSR $9D88` scroll/starfield passes; `$57` decides |
| `$80` | normal play, dispatch table `$9822` |
| `$81`,`$82` | section transition / next wave |
| `$C0` | player death (`$97F0`) |

`$9B38` is the stage init: clears `$3D-$97`, then `$0100`, `$0300`, `$0500`,
`$0580`, `$0600` and `$0680` (128 bytes each); sets the fire cooldown
`$35 = $14`; copies player fields from `$22,X/$24,X/$26,X/$28,X` into
`$42/$3F/$19/$1A`; `$1B++`; `JSR $8831` (palette); PPUMASK `$1E`, PPUCTRL `$A8`;
sets the ship/trail positions from the per-section table `$9BC6/$9BCE`
(`$0320-$0322`/`$0360-$0362`); `$0D = 6`;
`JMP $83B0`.

## Normal play frame (`$9822` table → `$9A40`)

```
X = $19
if $3F >= L9A30[X]: $1B = L9A38[X]     ; section/wave advance
JSR $835C                               ; background/section transition
if $5C < 2:
    JSR $A2BA      ; scroll position + event-stream scheduler
    JSR $BBB1      ; enemy/object update
    JSR $9FF6      ; player update (movement, fire, options)
    JSR $ADA5      ; enemy AI/weapons
JSR $BFDC          ; collision / object cleanup
JSR $8979          ; B button: activate selected power
JSR $C76C          ; stage-section hook
$12 = $3E          ; fine scroll for sprite conversion
PPUCTRL.bit0..1 = $3F bit0 (nametable)
if $1B bit7 clear: $9ABE (HUD split path) else $9A7F
```

`L9A30 = 0C 0C 0C 0C 0B 0B 0C 02` and `L9A38 = 81 81 81 81 81 81 81 81`
(at `$9A30`, file `$1A40`) are the per-section progress thresholds and the state
written when the threshold is crossed.

## Player update (`$9FF6`, file `$2006`)

```
if $0100 >= 2: JMP $A169            ; dying: skip movement
$99 = min($40 + 2, 16); $98 = 0
LSR $99; ROR $98                    ; 16-bit delta = $99:$98
                                    ;   $40=0 -> $0100 = 1.0 px/frame
                                    ;   $40=1 -> $0180 = 1.5 px/frame
                                    ;   ... $40=14 -> $0800 = 8.0 px/frame
X = $18                             ; current player
if $07,X & $01: JSR $A27F (add delta, Y=$40); if $0360 > $F0: $0360=$F0
if $07,X & $02: JSR $A291 (sub delta, Y=$40); if $0360 < $10: $0360=$10
if $07,X & $04: if $0320 < $C0: add delta (Y=$00); clamp $0320 to $C0
if $07,X & $08: if $0320 > $10: sub delta (Y=$00); clamp $0320 to $10
if $07,X & $0F != 0:
    $0160 = ($0160 + 1) % 24
    $07A0[$0160] = $0360; $07C0[$0160] = $0320   ; option trail history
    JSR $A2A3                                     ; place the trailing sprites
$0140++; if $0140 >= 8: $0120 = $9B (ship sprite frame), $0140 = 0
for i in 0..$45:                                  ; options
    $0141+i++; $0121+i = 4 + (($0141+i >> 3) & 1)
JSR $A0E3                                         ; fire / missiles
```

`$A27F` adds a 16-bit delta at base `+$40` (X integer/fraction at
`$0360/$0380`) and `$A291` subtracts; with `Y = 0` the same helpers move
`$0320/$0340` (Y). This is why the ship moves exactly 1 px/frame at speed 0 and
1.5 px at speed 1.

## Fire (`$A0E3`, file `$20F3`)

```
X = $44                       ; weapon level
$98 = $A0DA[X]                ; shot sprite type (6 normal, 7 double)
$99 = $A0E0[X]                ; fire SFX id
$9A = $05 & $80               ; A pressed this frame
$9B = $07 & $80               ; A held
for X = $45 down to 0:        ; options fire too
    if $0123,X == 0:                            ; shot 1 free
        if $9A == 0 and $03A3,X != 0: DEC $03A3,X
        else: JSR $A22F (spawn); $03A3,X = $35
             if $44 == 2: also spawn shot 2 with $03A6,X = $35
    if $0126,X == 0: same for shot 2 / $03A6,X
    if $41 != 0 and $0129,X == 0 and $9B != 0: JSR $A265 (missile)
```

`$35` is 20 (`$14`). Shot objects start at `$0363,X`/`$0366,X` (three slots
apart), get type `$98` and kind `$44 & 1`; in flight their X advances 7 px/frame
and despawns at `$F8` (`$A1CC`). The power-meter dispatcher (`$898E`) maps the
selected item to `INC $40` (speed), `INC $41` (missile), `$44=2` (double),
`INC $45` (option), `$46=5` (shield), `$89D8` (laser).

## Spawner (`$A2BA`, `$A32F`, file `$22CA`/`$233F`)

`$A2BA`: if `$3A != 0` go to the pause handler `$C40D`. Otherwise `$A2C6`
initialises `$6A/$6B` from `$A7CA[$19]` and sets `$61 = $3F & $0E`; `$A2F8`
compares `$3F` with the next column (`$61 + 2*delay`). When the column is
reached, `$A32F` consumes one record:

```
INC $5D
Y = 0
if ($6A),Y == $FF: return                 ; end of stream
Y = 1
$98 = ($6A),Y                             ; type / command byte
if $98 < $F0:
    advance pointer by 2                   ; $8407 adds to $6A/$6B
    if $98 >= $80: record = $A5FC + (($98 & $3F) * 4)   ; 4-byte definition
    else:          record = $A65C + ($98 * 3)           ; 3-byte definition
    copy 4 bytes into $64-$67
else:
    copy 5 bytes from the stream into $63-$67
    advance pointer by 5
    $64 -= $70
    ... (large-object/boss setup at $A3DE)
```

Object definition bytes map to object fields as `$64 -> $010C,X`,
`$65 -> $030C,X`, `$66 -> $03BC,X`, `$67 -> $03EC,X` (`$A455`–`$A490`), then
`$A521` allocates an object slot and `$A573` writes the sprite/position.
See `docs/analysis/stage1_data.md` for the tables and decoded streams.

## Collision (`$C0FB`, `$C204`, `$C39D`)

* `$C0FB` builds the player box from `$0360`/`$0320` (`+4`, `+8`) and compares
  with enemy positions and sizes from `$BFD4`/`$BFD8`.
* `$C204` compares the ship and options with objects flagged at `$0136,Y`,
  type `$0176,Y`, positions `$0376,Y`/`$0336,Y`, using the size table at
  `$C1FC` (`10 16 16 16 08 12 12 10`) and `$C200` (`?`).
* `$C39D` converts the object's X to a nametable address using `$3E`/`$3F`,
  reads the background tile and masks it with `$C409`, i.e. terrain collision.
* A hit sets `$0100 = 2`, `$4C = 120`, `$1B = $A0`, `$0160 = $0140 = 0`,
  plays SFX `$F7` and jumps to the death path `$C1D0`.

## HUD and score (`$8450`, `$8479`, `$84AE`)

The score is a 6-digit BCD number in `$07E4-$07E6` (P1) and `$07E8-$07EA` (P2).
`$8450` selects the buffer (`$9C = $E4` or `$E8`, `$9D = 7`), `$84AE` performs
the BCD add, and `$8479` copies the 3 bytes to the nametable via the `$0700`
queue. Lives are `$20`/`$21` (3 at new game, `$82F2`), players enabled
`$2A`/`$2B`. The power meter is sprite 0 (`$0200`: `CE 6D 23 F8`).

## Scroll and starfield (`$98E1`, `$9D88`, file `$18F1`)

```
$3D += $80          ; sub-pixel accumulator
carry += 0; add carry to $3E and propagate into $3F     ; $8407 with X=$3E
```

Called once per frame, so the stage scrolls 0.5 px/frame. `$9D88` compares
`$54/$55` with `$3E/$3F`, sets `$57` when the next 0x80 boundary is reached and
then draws the incoming background column with the `$07A0/$07C0` buffers.
`$9A70` folds `$3F` bit 0 into PPUCTRL's nametable bits so the scroll wraps.

## Timing constants

| constant | value | where |
| --- | --- | --- |
| scroll | 0.5 px/frame | `$98E1` (`ADC #$80`) |
| speed levels | 1.0, 1.5, 2.0 … 8.0 px/frame | `$A000`–`$A017` |
| fire cooldown | 20 frames | `$35 = $14`, `$9B58` |
| shot speed | 7 px/frame | measured `$0363` watch |
| shot despawn | X ≥ 248 | `$A1CC` |
| ship clamps | X 16..240, Y 16..192 | `$A022`–`$A075` |
| death timer | 120 frames | `$C1DA` |
| option trail | 24 entries | `$A084`–`$A08C` |
| option animation | frame = 4 + ((counter>>3)&1) | `$A0C7`–`$A0D5` |
| stage init blank | 6 frames | `$0D = 6`, `$9BBF` |
| sprite flag timer | reset at 8 | `$A0B4` |

## Open questions

* `$5C` counts non-zero entries among the four `0x30` structs at
  `$0600/$0630/$0660/$0690`; the main chain runs when it is < 2 and the
  alternate chain at `$967C` when it is >= 2. Whether those structs are players
  or options is not settled.
* `$A19E-$A1A3` gives `(dy, dx_int, dx_frac)` = `(2,0,$80)` for shot kind 0 and
  `(0,2,0)` for kind 1, but the measured bullet advances 7 px/frame, so another
  path must update player shots; confirm with a longer fire trace.
* The exact semantics of `$66`/`$67` in the 4-byte spawn definitions.
