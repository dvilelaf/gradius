# Two-player alternating play

This document describes exactly how `Gradius (E)` (mapper 3) alternates between
two players: the title menu, the per-player state, the handoff on death and
game over, what is shared and what is per-player, and the frames and addresses
that were measured through the oracle. It ends with the implementation status,
the card record and the one part that is still not modelled. No game code is
changed by this document.

Evidence was produced with `xtask probe`, `xtask ramdump` and `xtask disasm`
from the branch that added this file. File offsets in the ROM are
`address - $7FF0` (16-byte iNES header).

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-twop

# menu: Select on the title moves the cursor, Start locks 1P/2P
cargo xtask probe "$ROM" --frame 200 \
  --press 180:select --ram /tmp/title_ram.bin --png /tmp/title.png

# 2P start: Select at 180, Start at 220
cargo xtask ramdump "$ROM" --watch 150:340 \
  --range 0000:0030 --press 180:select --press 220:start

# force a P1 death during a 2P game and read $18/$0A/$20/$24-$29
cargo xtask probe "$ROM" --frame 470 \
  --press 180:select --press 220:start \
  --poke 400:20:01 --poke 400:1B:A0 --poke 400:4C:01 \
  --ram /tmp/handoff.bin

# disassembly of the menu, death and handoff routines
cargo xtask disasm "$ROM" --from 80B6 --to 8340 --labels
cargo xtask disasm "$ROM" --from 96E0 --to 9840 --labels
```

`probe` has no `--assist` (it always runs unassisted); `ramdump` takes
`--assist`. All gameplay addresses below are physical `$0000-$07FF`.

## 1. Menu flow and Start

`$80B6` increments the frame clock `$02`, then calls the menu helper `$8212`
while `$00 < 3` **and** `$03 & $40 == 0`:

```
$80B6: if $00 < 3 and ($03 & $40) == 0: JSR $8212
       LDA $00; JSR $83E9            ; dispatch table at $80CC
```

The `$00` dispatch table (`$80CC`) is:

| `$00` | handler | meaning |
| --- | --- | --- |
| 0 | `$80DA` | boot; title rolls in from `$12 = $FE` at 2 px/frame |
| 1 | `$810E` | **title screen, menu active** |
| 2 | `$8119` | attract/demo (`$09 != 0`) |
| 3 | `$812F` | **Start countdown**: blinks the selected title row, then starts |
| 4 | `$815D` | enter stage (`$1B = 0`, `$00 = 5`) |
| 5 | `$9643` | stage handler (gameplay) |
| 6 | `$8164` | ending |

Note: `$00 = 3` is the post-Start countdown, **not** game over. Game over runs
inside `$00 = 5` with `$1B = $C0` (see §3). `xtask/src/state.rs` maps
`$00 = 3` to `GameOver`, which is a mislabel of the start countdown; the
recorder's oracle output is unaffected because the ROM really does hold 3
during those frames.

### Selection state

| item | address | value |
| --- | --- | --- |
| menu selection | `$0F` | 0 = 1 PLAYER, 1 = 2 PLAYERS |
| menu cursor ship Y | `$0320` | `$86` (1P row) or `$96` (2P row) |
| menu cursor ship X / sprite | `$0360` = `$50`, `$0120` = 1 | Vic Viper beside the selected row |
| locked mode mask | `$03` | `$40` = 1P, `$70` = 2P |
| button edge | `$05` | bit 4 = Start, bit 5 = Select |

`$8212` in detail:

```
A = $05 & $30
if A == 0: RTS
JSR $8271                 ; $4C = 0, $4D = 1 (reset the menu timer)
X = $00
if X != 1: JMP $8240      ; not the title: re-enter the title, $00 = 1
if A & $20:               ; Select
    $0F = ($0F + 1) % 2   ; $8231-$823C (A=2, SEC, SBC $0F; 2 -> 0)
    JSR $8299             ; $0360=$50, $0120=1, $0320=$82AC[$0F]
else:                     ; Start
    $03 = $824C[$0F]      ; $824C = 40 70  -> $40 (1P), $70 (2P)
    $00 = 3 via $8187     ; $4C = $20
```

`$82AC = 86 96`, i.e. the two title rows at pixels 134 and 150. The title
records are text id 1 (`$8720`, row `$222D`: "1 PLAYER") and id 2 (`$872B`,
row `$226D`: "2 PLAYERS"); the title init `$82AE` queues ids 4, 3, 2, 1. The
menu works from **either** controller: while `$03 & $20 == 0` the NMI merges
both pads into `$9C` at `$8203` (`$9C = $9C | $9D`).

### Start countdown (`$812F`)

```
X = $01
if X == 0: SFX $90; $4C = $50 (80); $01 = 1; RTS
X == 1:  DEC $4C
         if $4C == 0: $4C = $20; $01 = 2      ; one more frame
         else: id = (($4C & 8) << 4) | (1 + $0F)
               JSR $85ED                     ; blink the selected row
X == 2:  JSR $82CD; $00 = 4                  ; new game
```

So the selected title row blinks 8 on / 8 off for 80 frames, then the game
starts. The `(($4C & 8) << 4)` sets `$80` in the text id, which makes `$85ED`
emit blank tiles instead of the row (text ids 1/2 are the 1P/2P title rows).

Measured with Select at frame 180 and Start at frame 220:

| frame | observed |
| --- | --- |
| 181 | `$05 = $20` (Select edge) |
| 182 | `$0F` 0 -> 1, `$0320` `$86` -> `$96` |
| 221 | `$05 = $10` (Start edge) |
| 222 | `$00` = 3, `$01` = 1, `$03` = `$70`, `$4C` = `$50` |
| 303 | `$00` 3 -> 4, `$0A` 0 -> `$07`, `$0D` = 16 |
| 304 | `$00` = 5 |
| 305 | `$1B` = 1 (stage rebuild starts) |
| 307-309 | `$1B` = 2, 3, 4 |
| 332 | `$1B` = `$80` (normal play) |
| 336 | `$0D` = 0, scroll starts |

A second Select at 240 moves the cursor back: at frame 242 `$0F = 0`,
`$0320 = $86`.

### New game initialisation (`$82CD`, called from the countdown)

```
JSR $82FF            ; clear $12-$EF, pages $0300-$06FF, $0D = 16, PPU off
X = $0B: $07E4..$07EF = 0           ; both score banks cleared
$09 = 0                             ; demo flag off
if $03 & $02: $1A = 0                ; never for $40/$70
Y = 7 if ($03 & $20) else 1
$0A = Y                             ; bit0 P1 alive, bit1 P2 alive, bit2 2P mode
$20 = $21 = 3                       ; lives
$2A = $2B = 1                       ; next extra-life threshold
```

`$18` is not written here; `$82FF` clears `$12-$EF`, which includes `$18`, so
the first player is P1 (0).

## 2. Per-player state

Only **one ship exists at a time**. `$18` is the current player and every
per-player array is indexed by it. `$18` has exactly one writer: `$97CE`.

| concept | P1 | P2 | notes |
| --- | --- | --- | --- |
| current player | `$18` = 0 | `$18` = 1 | only writer `$97CE` |
| alive bit | `$0A` bit 0 | `$0A` bit 1 | set at new game / continue, cleared at `$97E4` |
| 2P mode flag | `$0A` bit 2 | | set by `$82CD` (7 for 2P); **never read** |
| lives | `$20` | `$21` | 3 at new game; decremented at `$9793`; negative = out |
| next 1UP threshold | `$2A` | `$2B` | high BCD byte of score, 1 = 10 000 points; see below |
| score (BCD) | `$07E4-$07E6` | `$07E8-$07EA` | little-endian; `$07E7`/`$07EB` are unused |
| edge buttons | `$05` | `$06` | newly pressed this frame |
| held buttons | `$07` | `$08` | raw held |
| saved meter flag | `$22` | `$23` | 0/1 from `$42` at death |
| saved checkpoint scroll | `$24` | `$25` | `min($3F & $0E, 8)` |
| saved checkpoint section | `$26` | `$27` | `$19` at death |
| saved checkpoint sub-index | `$28` | `$29` | `$1A` at death |
| pause-code shield count | `$3B` | `$3C` | Konami-code reward counter, see §6 |
| score bank base | `$07E4` | `$07E8` | `$8479`: `$9C = $E4 + 4*$18` |

Button bits in `$05-$08` are game-ordered: bit0 Right, 1 Left, 2 Down, 3 Up,
4 Start, 5 Select, 6 B, 7 A. (The emulator's `BUTTON_*` constants are in shift
order; "down" = `$20` becomes `$05` bit 2 = `$04`.)

### `$2A`/`$2B` are extra-life thresholds, not "enabled"

`$82FA` sets `$2A/$2B = 1`, and `$84D8` compares the current player's score
high byte (`$07E6`/`$07EA`) with `$2A,X`:

```
if score_high >= $2A,X:
    BCD-add 1 to $2A,X        ; $84E2-$84E9, $84AA
    INC $20,X                 ; one extra ship
    SFX $36
```

So the threshold starts at 10 000 points and advances by one high byte
(10 000) per award. There is no per-player "enabled" byte; participation is
the `$0A` alive bit. `$C137` performs a second 1UP check on
`$07E5`/`$07E9` when an enemy is destroyed.

### Input routing

`$81B7` reads both controllers every NMI into `$9C` (P1) and `$9D` (P2).
`$81E3` decides whether to merge them:

* **1P mode** (`$03 = $40`, bit 5 clear): `$9C = $9C | $9D`, then the single
  `$07`/`$05` pair is written. Either pad drives the one ship.
* **2P mode** (`$03 = $70`, bit 5 set): `$9C` and `$9D` are processed
  separately into `$05/$07` and `$06/$08`. The player logic reads `$05,X` /
  `$07,X` with `X = $18`, so only the current player's pad moves the ship.

The oracle exposes controller 2 at `$4017` (`Emulator::set_buttons_p2`), and
`crates/gradius-nes/tests/rom_p2.rs` plus the `p2-handoff` scenario drive both
pads; P2 movement and the relay are measured, not inferred.

### Live ship state is shared and cleared

These are single global bytes, not per-player:

| item | address |
| --- | --- |
| position | `$0320/$0340` (Y), `$0360/$0380` (X), slots 0-2 |
| speed level | `$40` |
| missiles | `$41` |
| power meter cursor | `$42` |
| weapon level | `$44` |
| options | `$45`, option structs `$0600/$0630/$0660/$0690` |
| shield timer | `$46` |
| object/enemy tables | `$0100/$0300/$0500/$0580/$0600/$0680` |
| scroll | `$3D` fraction, `$3E` low, `$3F` high |
| spawner | `$19` section, `$1A` sub-index, `$6A/$6B` pointer |

`$9B38` clears `$3D-$97` (which includes `$40-$46`), so **powerups are lost on
every respawn**, and clears the object pages above, so **all enemies are gone**.
Nothing about the live ship or the world is inherited by the other player.

## 3. Turn switching

### 3.1 A death with lives left

Collision sets (`$C1D0`-`$C1F1`):

```
$0100 = 2      ; dying
$4C   = 120    ; death animation timer
$1B   = $A0    ; stage state "player dying" (bit 5)
$0160 = $0140 = 0
SFX $F7
```

`$1B = $A0` is handled by `$96E2`: `LDA $4C; if != 0 { DEC $4C; end frame }`,
otherwise `JMP $9790`. After the 120 frames:

```
$9790  X = $18
       DEC $20,X                       ; lose a ship
       $22,X = ($42 != 0)              ; save meter flag as 0/1
       $26,X = $19                     ; save section
       $24,X = min($3F & $0E, 8)       ; save checkpoint column
       $28,X = $1A                     ; save sub-index
       if $20,X < 0: JMP $97E4         ; out of ships -> game over
$97B8  choose next player:
       X = $18
       if X == 1: if $0A & 1: X = 0 else X = 1
       else:      if $0A & 2: X = 1 else X = 0
$97CE  STX $18
$97D0  $39=$3A=$5D=$33=$1B=$1C=0
       JSR $9C03                      ; $57 = 0, $5E = $3F
       JMP $9B38
```

So the *other* player plays next if they are still alive; otherwise the dead
player respawns. With both players alive this is strict alternation: every
death hands the controller to the other player, and the next death hands it
back. In a 1P game `$0A` bit 1 is clear, so `$18` stays 0.

`$9B38` rebuilds the stage for the new current player:

```
clear $3D-$97                         ; scroll, powerups, timers ($40-$46)
clear $0100/$0300/$0500/$0580/$0600/$0680   (128 bytes each)
$35 = 20                              ; fire cooldown
X = $18
$42 = $22,X;  $3F = $55 = $24,X
$19 = $26,X;  $1A = $28,X
$1B++                                 ; stage-init step 1
palette/PPU, ship from $9BC6/$9BCE at ($19, $3F)
$0100 = 1; $0D = 6
```

Then the four stage-init steps run (`$9BEA`, `$9C0C`, `$9C18`, `$9C1E`), and
`$9C36` sets `$1B = $80` (normal play). During the rebuild the screen is
blanked by `$0D`.

The checkpoint is captured **for the player who died**, and the stage is
restored from **the checkpoint of the player who starts**. Each player
therefore has an independent frontier through the stage:

* P1 dies at section 2, sub-index 1, `$3F = 6` -> `$24 = 06`, `$26 = 02`,
  `$28 = 01`.
* P2's first turn starts at `$24/$26/$28 = 0/0/0`: section 0, scroll 0, the
  beginning of the stage.
* P2 dies; P1 resumes at section 2, sub-index 1, `$3F = 6`.

Measured (2P, P1's death forced at frame 400 with `$20 = 1`, P2's at frame
700 with `$21 = 1`; the pokes set `$1B = $A0` and `$4C = 1` to skip the
120-frame animation):

| frame | `$18` | `$19/$1A/$3F` | `$25/$27/$29` (P2) | `$24/$26/$28` (P1) | `$20/$21` |
| --- | --- | --- | --- | --- | --- |
| 401 | 0 | 02/01/06 | 00/00/00 | 00/00/00 | 01/03 |
| 403 | 1 | 00/00/00 | 00/00/00 | 06/02/01 | 00/03 |
| 430 | 1 | 00/00/00 | 00/00/00 | 06/02/01 | 00/03 |
| 703 | 0 | 02/01/06 | 00/00/00 | 06/02/01 | 00/00 |

(`$1B` reaches `$80` at frame 430 after the first handoff. The measured
rebuild steps are `$1B` = 1 at 403, 4 at 415 (the step-4 scroll pass waits for
`$57`), `$80` at 430. On a fresh 2P start the same steps are `$1B` = 1 at 305,
2/3/4 at 307-309, `$80` at 332.)

A natural 2P run (no pokes) confirms the same sequence. The frame convention
here is the count of completed `run_frame` calls used by
`crates/gradius-nes/tests/rom_p2.rs`; the probe tables above sample one frame
later. With Select at 180 and Start at 220, P1's first ship is lost on frame
1194, where `$18` goes 0 -> 1, `$20` 3 -> 2 and `$1B` = 1, and P2 reaches
normal play (`$1B` = `$80`) on frame 1221, with P2's checkpoint still
`$24/$26/$28 = 0/0/0` and `$19/$1A/$3F = 0/0/0`. The same test drives P2 Right
after the handoff and sees `$06 = $08 = 01` and x `$50` -> `$76`, Y unchanged.

### 3.2 Losing the last ship (game over for one player)

When `DEC $20,X` produces `$FF`:

```
$97E4  A = $FE (P1) or $FD (P2)
$97EC  $0A &= A                        ; clear that player's alive bit
$97F0  $1B = $C0                       ; stage state "game over"
$97F4  if $09 == 0 (not demo):
$9801      $4C = $78 (120)
           text id $1C                 ; "1P GAME OVER", tile patched to '1'/'2'
           $0D = 16                    ; blank
           SFX $AF
       else (demo): $0D = 5; INC $0B; JMP $9C03
```

`$00` remains 5; the stage handler keeps running but `$1B = $C0` selects
`$96EE`, which does:

```
INC $5B
LDA $B0
if $B0 != 0:
    JMP $9750            ; sound engine busy: only scan the continue code, end frame
if $0A & 3 == 0 and $05 & $10:   ; all players out + P1 Start
    JSR $82CD; $00 = 4           ; brand new game, same 1P/2P mask ($03)
    RTS
if $4C != 0: DEC $4C; end frame  ; game-over countdown
$970C:
    X = $18; Y = $33
    if Y == $0A:                 ; continue code completed
        $20,X = 3; $2A,X = 1
        $0A |= $973C[X]          ; $973C = 01 02
        $24,X = 0                ; checkpoint column reset
        clear player X's score bank ($07E4+4X .. +3)
        JMP $97D0                ; resume the same player
    else if $0A & 3 != 0:
        JMP $97B8                ; hand over to the other alive player
    else:
        JSR $9B38; $00 = 0       ; back to the title
```

`$B0` is the first byte of the sound engine's 17-byte state block
(`$B0-$C0`, entered with `LDX #$B0` at `$ECFC`). While music/SFX are still
active `$B0` is non-zero, so the countdown and the "Start = new game" check
only run when the sound engine is idle. This is why the wall-clock game-over
screen lasts longer than the logical 120 frames. Measured:

| scenario | banner / lives -> `$FF` | `$B0` gate ends | countdown | result |
| --- | --- | --- | --- | --- |
| 1P idle | 3865 | 4142 | 4142..4261 | title at 4262 |
| 2P, P1 out | 6535 (`$0A` 07 -> 06) | 6811 | 6811..6931 | handoff to P2 at 6932 |
| 2P, P2 out | 7822 (`$0A` 06 -> 04) | 8098 | 8098..8218 | title at 8219 |

The gate is exactly 276 frames in the last two runs (277 in the 1P run), but
it depends on the music/SFX timeline. A port that does not model the sound
engine should use the logical 120 frames.

For the P1-out handoff: at 6932 `$18` flips to 1 and `$1B` = 1, the P2 stage
rebuild runs (`$1B` = 2 at 6934, 4 at 6936), and P2 is in normal play at
6960. P2 had zero lives at that point (`$21 = 0`) but was still alive; the
next P2 death is the one that clears the bit. Pressing Start at 8200 in the
both-out window instead starts a new 2P game: at 8205 `$00 = 5`, `$0A = 7`,
`$20 = $21 = 3`, `$18 = 0`, `$1B` = 1, and `$03` is still `$70`.

The continue code is the ten-byte table at `$977C`:
`04 08 40 80 40 80 40 80 40 80` = **Down, Up, B, A, B, A, B, A, B, A**.
`$9758` compares the current player's `$05,X` edge with the next entry,
advances `$33` on a match and resets it to 0 on a mismatch; `$33 == 10` at
countdown expiry triggers the continue branch. Because `$9750` is only reached
when `$B0 != 0`, the code is only scanned while a sound is playing; the port has
no `$B0`, so it scans the whole 399-frame game-over window instead.
(Verified by poking `$33 = $0A` at frame 6900 during the P1 countdown: the
player was restored to 3 lives and play resumed at `$1B = $80` on the frame
the countdown expired.)

The game-over banner is text id `$1C` at PPU `$2384` (row 28): five blanks,
then the tile pair `31 66` ("1P" in the stage CHR bank), then "GAME OVER".
The number is patched in the queue: after `$85ED` returns, `X` is the final
`$0700` write index, and `$980B` stores `$18 + $31` at `$06EC,X`, which is
exactly `$0700` + the offset of the `31` tile inside the queued record. So the
banner reads "1P GAME OVER" or "2P GAME OVER" even though the ROM record is a
hard-coded `31`. Measured row 28 at a P2 game over:
`... 00 00 32 66 00 00 01 02 ...` -> "2P".

### 3.3 Timers

| timer | value | role |
| --- | --- | --- |
| `$4C` = 120 | `$1B = $A0` | death animation; then `$9790` |
| `$4C` = 120 | `$1B = $C0` | game-over countdown (only while `$B0 == 0`) |
| `$0D` = 6 | `$9B38` | stage rebuild blank |
| `$0D` = 5, 4 | `$9C1E` | scroll/init blank |
| `$35` = 20 | stage init | fire cooldown |
| `$5E` = `$3F` | `$9C03` | column/scroll split countdown |

### 3.4 What happens to the stage on a handoff

The stage does **not** continue from the live state. Every handoff runs
`$9B38`: powerups, enemies, shots, options, scroll and the spawner are reset,
then the world is rebuilt from the incoming player's checkpoint
(`$24`/`$26`/`$28` -> `$3F`/`$19`/`$1A`). Between turns only the checkpoint,
lives, score and the `$22` meter flag survive. The music is restarted
(`$1C = 0`, then the stage handler sets `$93`).

## 4. Shared vs per-player

| per player | shared (single ship / single world) |
| --- | --- |
| `$18` current index | `$0320/$0340`, `$0360/$0380` position |
| `$20/$21` lives | `$40` speed, `$41` missiles, `$42` meter, `$44` weapon, `$45` options, `$46` shield |
| `$07E4-$07E6` / `$07E8-$07EA` score | object tables `$0100/$0300/$0500/$0580/$0600/$0680` |
| `$2A/$2B` 1UP threshold | scroll `$3D/$3E/$3F`, section `$19/$1A` |
| `$05/$07` and `$06/$08` input | spawner `$6A/$6B`, stage stream |
| `$22/$23` meter flag | starfield, palette, timers |
| `$24-$29` checkpoint | `$35` fire cooldown, `$5D` spawn counter |
| `$3B/$3C` pause-code count | |
| `$0A` bit 0/1 alive | |

The second player inherits **nothing** except the game mode: their first turn
starts at their own checkpoint (zero = stage start) with a bare, unpowered
ship, three ships, and a zero score. Their score bank, lives and checkpoint
then persist across their turns, and the HUD follows them.

## 5. HUD

| item | source | refresh |
| --- | --- | --- |
| "1P "/"2P " label at PPU `$23A8` (row 29, col 8) | text id `$13 + $18` (`$874B`/`$8751` = `31 66 00` / `32 66 00`) | `$8931`, called by `$9C0C` at every stage init |
| score digits at `$23AB..$23B1` | `$07E4`/`$07E8`, `$8931` loop `Y=2..0`, two BCD digits per byte | `$8931` |
| lives digit | `$20,X` in `$88C4-$88FA` | `$88BB`, via `$9C0C` |
| game-over banner row 28 `$2384` | text id `$1C`, tile `31` patched to `32` for P2 at `$980B` | on the game-over frame |

Because every turn starts with `$9B38` -> stage init, the HUD is redrawn with
the incoming player's label, lives and score. Measured: P2's turn row 29 is
`... 32 66 00 36 36 35 35 34 34 ...` ("2P" + score 665544 when the P2 bank
was poked with 44 55 66); P1's turn is `... 31 66 00 33 33 32 32 31 31 ...`
("1P" + 332211).

The power meter is sprite 0 (`$0200`: `CE 6D 23 F8`) and is not per-player;
`$42` is restored from the `$22` flag (0/1) on respawn, and `$9B38` has already
cleared the actual powerups.

## 6. Quirks and corrections to earlier notes

* `$2A/$2B` are extra-life thresholds, not "player enabled"
  (`docs/analysis/ram_map.md` is wrong there).
* `$09` is the attract/demo flag, not pause. Pause is `$15`; the pause handler
  is `$9A7F`, and the NMI's pause branch is `$9ACB`.
* `$0B` counts demo game-overs (`$97FC` only runs when `$09 != 0`); it is not
  "joined players". `$8125` ends the demo when it is non-zero.
* `$0A` bit 2 is set for a 2P game and never read.
* `$00 = 3` is the Start countdown, not game over.
* In a 1P game both controllers drive the ship (`$8203` OR); in 2P they are
  separate.
* `$9B38` clears `$3D-$97`, so shields, options and speed are lost on every
  death and every turn switch. This is why the second player never inherits
  powerups.
* The game gives four ships with `$20 = 3`: the value is decremented on each
  death and the player is only out when it goes negative (`$FF`). The port now
  uses the same wrap and clears the alive bit only at `$FF`.
* Controller 2 is wired through `$4017`; P2 movement was verified with `set_buttons_p2` (see `tests/rom_p2.rs`) and the `p2-handoff` scenario. `$05/$06` edges and `$07/$08` held bytes are read
  correctly, so the ROM paths themselves are covered.

## 7. Implementation status

The turn machine is `crates/gradius-core/src/twoplayer.rs` and `Game` owns it
(`two: TwoPlayer`); the port now follows the measured flow:

* **State.** `TwoPlayer` owns `$18` (`current`), both `PlayerSlot`s (lives,
  decimal score, extra-life threshold, checkpoint, alive bit), the mode and the
  `$1B` turn state. `Game` keeps one shared ship/power/scroll/object table, and
  `score`/`lives` remain P1's mirrors for `dump_state` and the harness.
* **Menu.** Start locks `$03 = $40`/`$70`; the 2P init sets `$20/$21 = 3`,
  both `$0A` alive bits (the ROM's `$0A = $07`), `$18 = 0` and zeroes both
  score banks. The attract/demo path stays 1P.
* **Input.** `Game::step_players(p1, p2)` merges both pads in 1P (`$8203`) and
  uses `$18`'s pad in 2P. `gradius-app`, SDL and wasm/PWA pass both pads.
* **Death and handoff.** `begin_death` stores the `$24/$26/$28` checkpoint for
  the dying slot; after the 120-frame `$C1DA` animation `finish_death`
  decrements that slot, hands over to the other alive slot (or respawns the
  same one) and rebuilds the shared stage from the incoming checkpoint.
* **Game over.** The wrap to `$FF` on the last alive slot starts the
  `game_over_timer`; `current` keeps naming the player who is out, and the HUD
  and the `$2384` banner read "1P GAME OVER"/"2P GAME OVER" (tile `31` -> `32`
  via `$980B`).
* **HUD.** `Hud` takes the current slot's label, lives and BCD digits;
  `score_switch` resets the delay on a handoff.

Measured on `main` at `a2ed7a2`: with Select at 180 and Start at 220, `$18`
goes 0 -> 1 at 1194 with lives 2/3, P2 reaches normal play at 1221 and P2's
Right moves the ship. `p2-handoff` is a clean row: 0 state divergences and
100% pixels from reset, and the scenario matrix is 13/13 clean, 0 known.

The `$977C` continue code is wired: `TwoPlayer` carries `$33`, the game-over
window in `Game::step` scans the current pad's `$05` edge every frame, and
`continue_game` restores the player at `$1B = $80` with 3 lives, threshold 1, a
zeroed score bank and checkpoint column 0. The port has no `$B0` sound-engine
block, so the scan covers the whole 399-frame window rather than only the frames
where the APU is busy (the documented equivalence), and it runs only for a 1P
game over or a both-out 2P one. The per-player `$C0` window is still not
modelled; the per-player save bank is, since `3120d6d` (the 32 B `PLAYERS`
chunk). This is the one part left in
[`../KNOWN_DEVIATIONS.md`](../KNOWN_DEVIATIONS.md).

## 8. Implementation (cards)

The K1-K8 chain landed in this order. The line numbers in the plan were from
`14a0802`; the route names below are the ones in the code today. The tests that
pin the flow are in `game.rs`
(`a_two_player_death_hands_over_and_keeps_both_banks`,
`game_over_needs_both_players_out`, the menu-start bank test), `twoplayer.rs`
(`continue_restores_only_the_current_player_and_restart_clears_everything`) and
`crates/gradius-diff/tests/scenarios.rs` (`p2-handoff`).

| card | what landed | evidence |
| --- | --- | --- |
| K1 | `TwoPlayer` in `Game`; `$18`; `players()` from the locked mode; both slots initialised by `new_run` | 2P start has `$18 = 0`, `$20/$21 = 3`, zero scores, both `$0A` alive bits |
| K2 | `step_players(p1, p2)`; runner, SDL and wasm/PWA pass both pads; the scenario runner feeds both | 1P merges pads, 2P uses `$18`'s pad |
| K3 | per-slot lives/score; `add_score` writes the current slot; BCD copy for the HUD | P1's `332211`/P2's `665544` banks stay separate across a handoff |
| K4 | checkpoint triple, handoff rebuild, `nmi_lag`, incoming pad latch | `$18`/lives at 1194, P2 normal at 1221, and the rebuild starts from the incoming slot's checkpoint (section 2, scroll 6 in the handoff test) |
| K5 | both-out game over and the `$2384` banner naming `$18` | `game_over_needs_both_players_out`; row-28 tile |
| K6 | `TwoPlayer::continue_game` (`$977C`, 3 lives, threshold 1, score 0, column 0) | wired: `Game` scans the edge in the game-over window and resumes at `$1B = $80`; `the_continue_code_restores_the_last_player_and_resumes_play`, `a_wrong_continue_press_resets_the_code` |
| K7 | HUD label, lives and digits from the current slot; delay reset on switch | first P2 playing frame starts `32 66 00` with P2's digits |
| K8 | `p2:` script lane, `set_buttons_p2`, `step_players`; `p2-handoff` promoted to clean | 0 divergences, 100% pixels from reset |

**What must not change:** one ship and one world (`player`, `power`, `scroll`,
`spawner` and `objects` stay single and `$9B38` rebuilds them),
`high_score_bcd` stays global, and the 1P path keeps the `p1 | p2` merge and the
four-ship `$FF` wrap.

### 8.1 Risks from the plan

* **R1 hidden state between turns.** The rebuild clears the stage fields and
  keeps `volcano_kills`; the handoff test pins both banks and the checkpoint.
* **R2 save states.** The per-player slots and the `$18` turn state are still
  not in the image (see `KNOWN_DEVIATIONS.md`), so a 2P snapshot is not yet a
  round trip.
* **R3 determinism and port-side constants.** `Game` keeps the existing
  `game_over_timer` (the measured 1P/2P hold) and does not run `TwoPlayer`'s
  120-frame `GameOver` a second time; the relay assertions are frame-exact at
  1194/1221.
* **R4 1P merge regression.** Covered by the 1P tests and `rom_p2`.
* **R5 HUD lag on a switch.** `score_switch` redraws the row from the incoming
  bank on the first playing frame.

### 8.2 Harness note

`dump_state` deliberately keeps the P1 `score`/`lives` pair, because the
oracle's state line exposes `$07E4-$07E6` and `$20`; the scenario's 2P oracle
checks read `$18`, `$20/$21`, `$1B` and P2's `$06/$08` separately. That is why
the `p2-handoff` row can be clean while the state line still reports P1's
bank.

## 9. Open questions

* The exact reason `$B0` stays non-zero for ~277 frames after the game-over
  banner was not decoded further (it is the APU state block `$B0-$C0`); the
  logical timer is 120 frames.
* The continue-code entry was exercised by poking `$33 = $0A`, not by typing
  the ten buttons through the oracle; the table, the edge comparison at `$9758`
  and the `continue_game` rewards are static evidence, and the port's scan is
  covered by its own tests.
* `$22/$23` store only a 0/1 copy of `$42`, so the exact power-meter cursor is
  not preserved per player; only "meter was active" survives.
