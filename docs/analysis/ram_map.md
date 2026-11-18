# CPU RAM map (stage 1)

Evidence for the zero page, the sprite/object tables and the player state used by
the native port. Every claim below was measured through the oracle with the new
`xtask ramdump` command; `xtask disasm` supplies the code addresses. No
cartridge bytes are copied here.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-disasm

# full 2 KiB dump at a live stage-1 frame
cargo xtask ramdump "$ROM" --frame 900 --press 180:start --assist

# per-frame deltas, with a change histogram
cargo xtask ramdump "$ROM" --watch 700:730 --press 180:start \
  --press 650:right:500 --assist --range 0360:0364

# one page
cargo xtask ramdump "$ROM" --frame 900 --press 180:start --assist \
  --range 0300:0400
```

`--assist` is required for live gameplay: the scanline-at-once PPU model cannot
raise the HUD sprite-0 hit, so the game would spin forever in `$9A96`. The
assist injects the hit only while the CPU polls `$2002` bit 6 (`docs/analysis/stage1_flow.md`).

All RAM addresses below are physical `$0000-$07FF`; the same bytes are mirrored
at `$0800-$1FFF`.

## Zero page

| addr | size | meaning | evidence |
| --- | --- | --- | --- |
| `$00` | 1 | game state (0 title, 5 stage) | `$83E9` dispatch table; dump reads 5 in stage 1 |
| `$01` | 1 | sub-state / stage phase | read at `$80DA`, `$812F`, `$96…` |
| `$02` | 1 | 60 Hz frame counter, free-running | `INC $02` at `$80B6`; increments once per frame |
| `$03` | 1 | menu/held buttons (edge-processed copy) | written only by `$822A`, read by `$80BE`, `$82DC` |
| `$04` | 1 | NMI re-entry guard / work counter | set to 1 at `$8097`, cleared `$80AD`; skips sprite work when non-zero |
| `$05` | 1 | buttons **pressed this frame** (edge): bit0 Right, 1 Left, 2 Down, 3 Up, 4 Start, 5 Select, 6 B, 7 A | `$81F9`–`$81FE`; watch shows `$05=01` for Right, `02` for Left |
| `$06` | 1 | previous-frame edge mask | `$81FA` (`EOR $07,X`) |
| `$07` | 1 | buttons **held** (raw): same bit order | `$81FE`; `$9FF6` reads it every frame |
| `$09` | 1 | pause/IRQ flag (non-zero freezes updates) | checked at `$8363`, `$8474` |
| `$0A` | 1 | player-alive bits (bit0 P1, bit1 P2) | set at `$82F0`, cleared at `$97EC` |
| `$0B` | 1 | number of joined players | `INC $0B` at `$97FC`; `$82CD` clears |
| `$0D` | 1 | screen-blank countdown (PPUMASK forced 0 while running) | `$8088`–`$808E` |
| `$0E` | 1 | VRAM update queue write index | `$864A`, `$8A80` |
| `$10` | 1 | PPUCTRL shadow ($A8 in stage) | uploaded at `$832E`, `$8A64` |
| `$11` | 1 | PPUMASK shadow ($1E in stage) | `$8082`–`$808E` |
| `$12` | 1 | horizontal scroll low byte (= `$3E`) | `$9A6C`–`$9A6E` |
| `$13` | 1 | stage scroll speed constant (12 = `$0C`) | `$9643`–`$9645` |
| `$14` | 1 | Start-button edge latch | `$81DB`–`$81E1` |
| `$15` | 1 | pause flag (non-zero: pause handler at `$9A7F`) | `$964F`, `$9A8B` |
| `$17` | 1 | stage-section index for the event stream | `$9C3F`–`$9C55` |
| `$18` | 1 | current player (0/1) | `$9C41`, `$9FF6` |
| `$19` | 1 | stage section index 0–7 | `$9A40`, `$9F49`; dispatch at `$A2CB` |
| `$1A` | 1 | stage section sub-index | copied to `$1A` at `$9B6C` |
| `$1B` | 1 | stage state machine: `$80` normal play, `$81`/`$82` transitions, `$C0` death | `$981D`, `$9A7B`, `$97F0` |
| `$1C` | 1 | music/SFX id latch | `$83A0`–`$83AD` |
| `$1F` | 1 | sprite-palette animation phase | `$80EE`, `$8B1F` |
| `$20`/`$21` | 1 each | lives, player 1 / player 2 (3 at new game) | `$82F2: LDA #3; STA $20; STA $21` |
| `$2A`/`$2B` | 1 each | player 1 / player 2 enabled (1 at new game) | `$82FA: STA $2A; STA $2B` |
| `$2D` | 1 | palette/attribute upload mode | `$8AA1`, set by `$80E4` |
| `$2F` | 1 | starfield/sprite jitter counter, `+= $44` per frame | `$8B3E`–`$8B48` |
| `$33` | 1 | player sprite animation frame index | `$9758`–`$9775` |
| `$35` | 1 | fire cooldown in frames (20 = `$14`, set at stage start) | `$9B58`; copied to `$03A3`/`$03A6` |
| `$39`/`$3A` | 1 each | death/respawn and pause counters | `$97D0`, `$9790`, `$A2BA` |
| `$3D` | 1 | scroll sub-pixel accumulator, `+= $80` per frame | `$98E1`–`$98E6` |
| `$3E`/`$3F` | 1 each | 16-bit horizontal scroll position (`$3E` low, `$3F` nametable bit) | `$98E1`–`$98ED`; `$9A6C` |
| `$40` | 1 | **speed power level** (0 at start, `INC $40` per pickup) | `$89A6`, speed formula at `$A000`–`$A017` |
| `$41` | 1 | missiles owned (0/1) | `$89B4`; checked `$A156` |
| `$42` | 1 | power-meter selection 0–6 | `$8989`, `$A2BA` |
| `$44` | 1 | weapon level 0/1/2 (double at 2) | `$89C0`, table `$A0DA` |
| `$45` | 1 | options (multiples) owned 0–4 | `$89D4`, loop at `$A0C2` |
| `$46` | 1 | shield/force-field timer (5 at pickup, ticks down) | `$899C`, `$C1B7` |
| `$4C` | 1 | generic countdown (state transitions, death 120) | `$8187`, `$C1DA` |
| `$49` | 1 | option formation phase (2/3) | `$A3EF`–`$A3FC` |
| `$54`/`$55` | 1 each | 16-bit background/scroll target used by the starfield pass | `$9D90`–`$9DAB` |
| `$57` | 1 | "column boundary reached" flag | `$9D88`–`$9DAB` |
| `$58` | 1 | starfield/comet animation counter | `$9F8E`–`$9FAD` |
| `$5B` | 1 | frame-parity/scroll inhibition flag | `$964B`, `$9A8B` |
| `$5C` | 1 | number of active options (count of `$0600/$0630/$0660/$0690`) | `$9656`–`$9676` |
| `$5D` | 1 | stage event/spawn counter | `$A32F` |
| `$5E` | 1 | pause/scroll-split countdown | `$9C07`, `$9930` |
| `$60` | 1 | stage init step (0–4, then `$80` path) | `$A2C1`–`$A2E9`, `$9B70` |
| `$61`/`$62` | 1 each | event-stream column base / sub-index | `$A2DD`, `$A2E0` |
| `$63`–`$67` | 5 | current 5-byte event record scratch | `$A376`–`$A387` |
| `$69`/`$6A`/`$6B` | 1/1/1 | spawn count / event-stream pointer low/high | `$A2CF`–`$A2E9` |
| `$6C`/`$6D`/`$6E` | 1 each | spawn timer / object X high / X offset | `$A425`–`$A434` |
| `$98`–`$9F` | 8 | general scratch (pointer, delta, BCD temp) | throughout |

## Sprite/object tables (256-byte pages)

The object system is a set of parallel 32-byte arrays, one byte per object slot
0–31, based at `$0300`, plus sprite-side arrays in `$0100`:

| range | meaning | evidence |
| --- | --- | --- |
| `$0100` | player state: 0/1 alive, 2 dying | `$9FF6`–`$9FFB`, `$C1DA`–`$C1E0` |
| `$0103+i` | object/option index for sprite `i` | cleared `$A56F`, `$C0BD` |
| `$010C+i` | object subtype / tile base | `$A455`–`$A457` |
| `$0120+i` | ship/object sprite frame (1..5) | `$A0B1`–`$A0BF`, `$C0DD` |
| `$0123+i` | bullet/weapon sprite type (6/7) | `$A23D`, `$A258`; watch: 0→6 at fire |
| `$0126+i` | second shot sprite type | `$A258` |
| `$0129+i` | missile sprite type (0x0A) | `$A274`–`$A276` |
| `$012C+i` | enemy sprite type | `$C149`, `$C15A` |
| `$0130+i`,`$0134+i` | per-sprite animation counters | writer `$8B15` region; watch shows ±1 per frame |
| `$0136+i` | enemy collision-enable flag | `$C224` |
| `$0140` | player sprite animation timer (`INC`, reset at 8) | `$A0A7`–`$A0BF` |
| `$0141+i` | option animation counters | `$A0C7`–`$A0D5` |
| `$0160` | option-trail ring index, modulo 24 (`$18`) | `$A080`–`$A08C` |
| `$0163+i` | bullet/weapon active flag (1) and kind | `$A23B`, movement `$A176` |
| `$0176+i` | enemy type index into `$A58C` | `$A3DE`, `$C229` |

Continuous position arrays (one byte per slot, 32 slots each):

| range | meaning | evidence |
| --- | --- | --- |
| `$0320+i` | **Y integer** | ship Y=96 (`$60`) at slot 0; `$9FF6` reads `$0320` for clamps |
| `$0340+i` | Y fraction (8.8 fixed point) | `$A27F`–`$A28D` |
| `$0360+i` | **X integer** | ship X=80 (`$50`) at slot 0; watch `right`/`left` |
| `$0380+i` | X fraction (8.8) | `$A54E`, `$9FF6` speed delta |
| `$03A0+i` | weapon cooldown timer (shot 1) | `$A10D`–`$A12B` |
| `$03A3+i` | weapon cooldown timer (shot 1, alternate base) | `$A119`, `$A14B` |
| `$03A6+i` | weapon cooldown timer (shot 2) | `$A137`–`$A153` |
| `$03AC+i` | option formation phase | `$A452` |
| `$03BC+i` | enemy spawn sub-state / timer | `$A488` |
| `$03EC+i` | enemy spawn sub-state | `$A48C`–`$A490` |
| `$0460+i` | trail/formation index for options and enemies | `$A528`, `$CC4A` |
| `$0496+i` | option trail base | `$A525` |
| `$04E0+i` | object kind/flags | written at `$A56F` |
| `$04F0+i` | object flags (0xC8 for the first six slots) | dump frame 900 |

`$0500-$05FF` and `$0600-$06FF` are two more 128-byte object/option blocks. The
option (multiple) structs are `0x30` bytes at `$0600`, `$0630`, `$0660`, `$0690`:
`$0600+i` active, `$0601+i` trail index, `$0603+i` update divider,
`$0618/$0619` and `$0620/$0621` a 16-bit position pair, `$0611/$0621` trail
offset. See `$CB84-$CD40`.

## Player state and movement (measured)

| item | value | evidence |
| --- | --- | --- |
| player object slot | 0 | ship X/Y written at `$0320`/`$0360` by `$9B82`–`$9BB8`; `$C0FF` reads them; at rest slots 1/2 hold the same value |
| position | X `$0360` + `$0380`/256, Y `$0320` + `$0340`/256 | `$A27F`/`$A291` add/sub to a 16-bit pair at base `+$40` (X) and `+$00` (Y) |
| trail slots | `$0361/$0362` (and `$0321/$0322`) are the first option/trail slots; they follow slot 0 with a lag | watch `--press 650:right:100`: `$0360=134`, `$0361=123`, `$0362=112` while the OAM ship is at X 125/133/141 |
| move delta per frame | `min($40+2, 16)` in 8.8 → **1.0 px at speed 0, 1.5, 2.0 … 8.0 px** | `$A000`–`$A017`: `ADC #2`, clamp 16, `LSR/ROR` |
| X clamp | `$10..$F0` (16..240) | `$A022`–`$A02A`, `$A034`–`$A03C` |
| Y clamp | `$10..$C0` (16..192) | `$A049`–`$A053`, `$A063`–`$A075` |
| input bits | Right=1, Left=2, Down=4, Up=8 | `$A019`–`$A0A4` (`AND #$01/#$02/#$04/#$08`) |
| speed power | `$40`, `INC` per pickup, no explicit cap in the increment | `$89A6` |
| option trail | 24-entry ring at `$07A0`(X)/`$07C0`(Y), index `$0160` | `$A080`–`$A09C` |

Measured with `--watch 700:730 --press 650:right:500`: `$0360` increments by
exactly 1 every frame at speed 0. `--press 650:up:500` decrements `$0320` by 1;
down increments it; the ship stops at 16/240 (X) and 16/192 (Y).

## Weapons

Firing reads the A button, both edge (`$05` bit7) and held (`$07` bit7), at
`$A0F6`–`$A100`. Shots are spawned into object slots 3..6, with the twin shot
three slots apart (`$A22F` writes `$0363,X`, `$A24A` writes `$0366,X`):

| item | value | evidence |
| --- | --- | --- |
| fire cooldown | 20 frames (`$35 = $14`) | `$9B58`, `$A119`, `$A143` |
| weapon level | `$44` 0/1/2 (2 = double) | `$89C0`, table `$A0DA` |
| shot kind | `$0163,X = $44 & 1` | `$A240`–`$A244` |
| shot sprite type | `$98` from `$A0DA,X` = 6 or 7 | `$A0E5`–`$A0E8`, `$A23B`; watch shows 0→6 |
| shot speed | +7 px/frame measured in X | watch `$0363`/`$0366` at fire |
| shot despawn | X ≥ `$F8` (248) | `$A1CC`–`$A1CE` |
| missiles | `$41`, slot `$0129,X`, sprite 0x0A | `$89B4`, `$A156`–`$A163`, `$A274` |
| power meter | `$42` selected item 0–6 | `$8989` dispatch table `$898E` |

Power-up actions (dispatch table at `$898E`, selected by `$42`): `$89A6`
`INC $40` (speed), `$89B4` `INC $41` (missile), `$89C0` `$44 = 2` (double),
`$89D4` `INC $45` (option), `$899C` `$46 = 5` (shield), plus `$89D8` (laser).
B activates the selected item (`$8979`–`$8986` checks `$07` bit6).

## Enemies, collision and spawner

| item | value | evidence |
| --- | --- | --- |
| enemy collision flag | `$0136+i` | `$C224` |
| enemy type | `$0176+i`, index into `$A58C` (2 bytes/type) | `$A3DE`–`$A406`, `$C229` |
| enemy position for collision | X `$0376+i`, Y `$0336+i` | `$C22F`–`$C23C` |
| player hitbox | X `$0360+4`, Y `$0320+8`, then `+6`, `+8`, `+4` extents | `$C0FF`–`$C220` |
| hit result | `$0100 = 2`, `$4C = 120`, `$1B = $A0`, sound `$F7` | `$C1D0`–`$C1F1` |
| background collision | nametable tile at (`$A0 + $3E`, `$3F` carry), mask table `$C409` | `$C39D`–`$C406` |
| spawn pointer | `$6A/$6B` | `$A2CF`–`$A2E9` |
| spawn tables | `$A7CA` section table, 8-word column sub-tables, streams | `docs/analysis/stage1_data.md` |
| spawn counter | `$5D` (`INC` on each event) | `$A32F` |

## HUD, score, lives

| item | value | evidence |
| --- | --- | --- |
| score | 6-digit BCD, 3 bytes at `$07E4` (P1) and `$07E8` (P2) | `$8450`–`$8492` sets `$9C=$E4/$E8`, `$9D=7`; BCD add at `$84AE` |
| score add | BCD add helper `$84AE`, called from `$BFAB`, `$C0A0`, `$C15D` | code |
| lives | `$20`/`$21` = 3 at new game | `$82F2`; dump reads 3 |
| player enabled | `$2A`/`$2B` = 1 | `$82FA` |
| HUD/power bar | sprite 0, OAM `Y=$CE, tile=$6D, attr=$23, X=$F8` | `$0200` shadow; `docs/analysis/stage1.md` |
| VRAM queue | `$0700` page, commands terminated by `$FF`, pointer `$0E` | `$8A56`–`$8AA6` |

## Scroll and timing

* `$3D` accumulates `$80` per frame; the carry increments `$3E` (`$98E1`), so the
  stage scrolls **0.5 px/frame**. `$3F` is the high byte; its bit 0 selects the
  nametable in PPUCTRL (`$9A70`–`$9A79`).
* `$02` increments once per NMI (`$80B6`) and is the frame clock. The game uses
  it as a cheap pseudo-random source (`$8B82`, `$967C`).
* `$2F += $44` per frame (`$8B3E`) drives starfield/sprite jitter.
* `$58` drives the starfield burst: normally `+1`, but when `$58 & 7 >= 6` it adds
  `$19 + $58`, wrapping at `$80` (`$9F8E`–`$9FAD`).

## Open questions

* `$0123`/`$0126` versus `$0163`/`$0166`: the sprite-type and movement-active
  arrays use different bases; the remaining slots 7/8 are the missile and the
  second option pair. Confirm with a trace that fires the laser.
* `$04E0`/`$04F0` flags are only observed as constants (0xC8) for the first six
  slots; their meaning is not decoded.
* `$06xx` option structs use `$0618/$0620` as a 16-bit pair; whether that is the
  option's own X/Y or a copy of the ship's is not proven.
