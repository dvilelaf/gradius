# Kind `$14` (`$CA58`), kind `$16` (`$C900`) and the `$0F`/`$10` gate

Evidence for the two missing stage handlers and the missing stage-1 volcano
gate. Everything below was decoded with `cargo xtask disasm` and observed in
the oracle with `cargo xtask probe`; no cartridge bytes are copied. This note
closes the three gaps listed in `stage_enemies.md`:

| kind | handler | stage | what the port has |
| --- | --- | --- | --- |
| `$14` | `$CA58` | 5 (20 command spawns) | missing; `_ => ai_drift`, and the `$A4A0` spawn marks it `boss` |
| `$16` | `$C900` | 3 (53 command spawns) | routed to `ai_big_core` |
| `$0F`/`$10` | `$AF28`/`$AF82` | 1 (and 2/4/6/7) | fire + drift ported; `$046C >= 5` death, `$5F`/`$39` gate missing |

`stage_enemies.md` calls `$16` a "burst turret". That is wrong in one important
way: **`$C900` spawns no projectile and has no OAM sprite.** It animates and
erases a turret painted into the nametable. The two "burst patterns" are the
closed and open tile art of that background turret. Details and frame
evidence are below.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-handlers

# the three routines
cargo xtask disasm "$ROM" --from CA58 --to CB28 --labels --symbols tools/ghidra/symbols.txt
cargo xtask disasm "$ROM" --from C900 --to CA23 --labels --symbols tools/ghidra/symbols.txt
cargo xtask disasm "$ROM" --from AF28 --to B020 --labels --symbols tools/ghidra/symbols.txt

# stage 3 ($19 = 2) and stage 5 ($19 = 4): poking $26 before stage init makes
# $9B68 copy it into $19. One poke per frame until the stage starts.
ST3=$(for f in $(seq 170 320); do printf -- '--poke %d:26:2 ' $f; done)
ST5=$(for f in $(seq 170 320); do printf -- '--poke %d:26:4 ' $f; done)
cargo xtask probe "$ROM" --frame 2000 --press 180:start --keep-alive $ST3 \
  --ram /tmp/opencode/st3.ram --oam /tmp/opencode/st3.oam
cargo xtask probe "$ROM" --frame 3500 --press 180:start --keep-alive $ST5 \
  --ram /tmp/opencode/st5.ram --oam /tmp/opencode/st5.oam --png /tmp/opencode/st5.png

# force one death: poke the object's $046C hit counter (slot N -> $046C+N)
cargo xtask probe "$ROM" --frame 2002 --press 180:start --keep-alive $ST3 \
  --poke 2000:474:3 --ram /tmp/opencode/st3-death.ram --nametables /tmp/opencode/st3.nt
cargo xtask probe "$ROM" --frame 4502 --press 180:start --keep-alive $ST5 \
  --poke 4490:475:FF --ram /tmp/opencode/st5-death.ram
```

`--poke FRAME:ADDR:VALUE` writes one CPU-RAM byte at the start of `FRAME`
(hex). Slot `N`'s `$046C` is `0x046C + N`.

## How the command records reach these kinds

Five-byte records are handled by `$A460` (see `enemy_ai.md`). The record bytes
are `[delay, type, b2, b3, b4]`; the type is `>= $F0`, `$64 = type - $70` and
the kind is taken from `$66`/`$67`:

* `$19 == 2` (stage 3), `$A469`: kind `$96`, `$010C = type - $70`, `y = b2`,
  `$03BC = b3`, `$03EC = b4`, `x = $F0`. The `$AE13` dispatch masks bit 7, so
  the handler index is `$16`.
* otherwise, `$A4A0`: kind `$66 = b3`, subtype `$80`, `x = $F0`, `y = b4`;
  `b2 >> 4` attached cannon records are created in the `$0600` structs
  (`enemy_ai.md` states `$65 & $0F`, but the code at `$A4C7` does four `LSR`s,
  i.e. the high nibble). Stage 5's kind is `$14`.

Stage 3 has 53 such records; stage 5 has 20. Stage 5's four distinct records, repeated once per column across five columns,
all type `$F0` and kind `$14`: delays `$40`, `$80`, `$C0` and `$F0` ascending;
y `$80`, `$40`, `$80` and `$60`; cannon counts 0, 1, 0 and 2 (the `b2` nibble is
`$01`, `$12`, `$02` and `$21`).

Stage 3's records use the whole `$F0..$F3` range (subtypes `$80..$83`) and
`b3`/`b4` as the nametable address; several samples are in the evidence table.

## Kind `$14` (`$CA58`, stage 5 large object)

### Constants

`$CA43`, `$CA4A` and `$CA51` are contiguous 7-entry tables indexed by the rank
`$17` (`$9C3F`: weapon + options + shield + stage, nominally 0..7). Because
the rank can reach 7, the last index reads the first byte of the next table.

| rank | `$CA43` morph HP | `$CA4A` death HP | `$CA51` aim fraction (px/frame) |
| --- | --- | --- | --- |
| 0 | 10 | 20 | 64 = 0.250 |
| 1 | 12 | 24 | 72 = 0.281 |
| 2 | 14 | 28 | 80 = 0.312 |
| 3 | 16 | 32 | 96 = 0.375 |
| 4 | 18 | 36 | 112 = 0.437 |
| 5 | 20 | 40 | 128 = 0.500 |
| 6 | 22 | 44 | 144 = 0.562 |

The tables are 7 entries; a rank of 7 reads the first byte of the next table
(another ROM quirk to reproduce or clamp). Score on death is 500 (`$8450`),
sound id `$0A`, and the object becomes kind-2 debris.

### State machine

```
CA58:
  rank = $17
  morph_hp = $CA43[rank]
  death_hp = $CA4A[rank]
  kind = $94                       ; stays $94 (dispatch kind $14)
  $042C += 1                       ; free-running frame counter
  sprite = $81 or $82              ; INY when ($042C & 8) != 0
  $048C = 1                        ; damageable from the first frame

  if $046C >= morph_hp:            ; first threshold: damaged state
      if $0460 != 0:               ; once only
          $0460 = 0                ; box 32x32 -> 16x16 ($BFD4/8)
          x += 8; y += 8           ; re-centre the smaller box
      sprite += 2                  ; $83/$84
  $012C = sprite

  if $046C >= death_hp:            ; second threshold: death
      score 500
      $CB20(A = $0A)               ; sound + kind-2 debris
      $CB48                        ; release attached cannons
      return

  if $016C == 0:                   ; no attached cannons
      $AEDB                        ; x -= 0.5 px/frame, despawn at x < 8
      if $012C == 0: return

  if $04AC == 0:                   ; re-aim timer
      $04AC = $02 & $3F            ; 0..63 (0 becomes 255 after DEC)
      if y >= player.y: $04CC = 0  ; move up
      else:             $04CC = 1  ; move down
  $04AC -= 1

  if $04CC == 0: y -= $CA51[rank]/256, clamp y >= $20
  else:          y += $CA51[rank]/256, clamp y <= $A8
  $AEDB                            ; drift (only if $016C == 0)
```

Notes:

* The horizontal motion is the shared `$AEDB` drift (0.5 px/frame, remove at
  `x < 8`). A stage-5 object with attached cannons (`$016C != 0`) does **not**
  drift; the cannon system owns it until the cannons are gone.
* Vertical motion is a fixed 8.8 fraction from `$CA51`, toward the player's Y,
  re-chosen every `$02 & $3F` frames (not tied to the fire timer).
* Sprite groups `$81`/`$82` are 6-part 32x24 hulls; `$83`/`$84` are the
  damaged draw. `$0460 = 1` selects the 32x32 box (`$BFD4[1]`/`$BFD8[1]`).

### Attached cannons (`$CB8B`, called from `$9684`)

`$A4A0` creates up to four `$0600` records (stride `$30`), one per
`(b2 >> 4)`; each stores its parent slot in `$0600,X` and a cannon index in
`$0601,X`. Every frame `$CB8B`:

* skips everything while the option count `$5C >= 2`;
* positions each cannon around the parent with the `$CC19`/`$CC1D` offsets and
  the parent's `$0460` box class;
* increments `$0604` and, when it reaches `$CBC4[rank]`, resets it and calls
  `$CBCB` to launch one enemy bullet from the cannon position into the `$0136`
  proxy array (type `$0176 = 0`), if a free entry exists.

Cannon fire period `$CBC4` by rank: `40, 35, 30, 25, 25, 25, 25` frames. A
cannon takes `$BEE4[rank]` hits (`2, 2, 3, 4, 5, 6, 7, 8, 9`) and is worth 100
(`$BF43`). `$CB48` releases any still-attached cannon as kind-2 debris when the
parent dies.

The port currently records `ai_mode = attachments` but neither places nor
fires the cannons; `ai_big_core` shows the enemy-bullet helper to use.

### Observed

Frame evidence (stage 5, `$19 = 4`, `--keep-alive`, no input):

| frame | slot | `$030C` | `$010C` | x | y | `$048C` | `$046C` | `$0460` | note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 3500 | 8 | `$94` | `$80` | `$4E` | `$5A` | 1 | 13 | 1 | two 32x32 objects on screen |
| 3500 | 9 | `$94` | `$80` | `$CE` | `$6A` | 1 | 0 | 1 | |
| 4500 | 9 | `$94` | `$80` | `$8E` | `$74` | 1 | 0 | 1 | `$04CC = 1` (moving down), `$04AC = 176` |
| 4502 | 9 | `$02` | 0 | `$96` | `$79` | 1 | 0 | 0 | `$046C` poked; score 1000 -> 1500 (+500), `$016C = 2` |

The six OAM parts at frame 3500 are the `$81` group (`$35/$37/$3B/$39/$3D/$3F`)
around the two object origins. The object starts at `x = $F0` and enters from
the right.

## Kind `$16` (`$C900`, stage 3 background turret)

### Constants and data

`$CA23` is read in 4-byte groups. `$A9 = $010C & $0F` (0..3) selects the
group; the first `$C9B4` call uses group `4*type + $10` (open art), the second
uses `4*type` (closed art). Each group is `tile0, tile1, second_present,
tile2`:

| type (`$010C`) | closed group (base) | open group (+$10) |
| --- | --- | --- |
| `$80` (0) | `00 8F`, second `90` | `20 21`, second `22` |
| `$81` (1) | `D9 DA`, no second | `2E 2F`, no second |
| `$82` (2) | `B0 00`, second `B1` | `24 25`, second `27` |
| `$83` (3) | `CF 7C`, no second | `28 29`, no second |

A "second" tile is written `$20` down in the same nametable one tile later:
address high `$03BC`, low `$03EC + $20` (carry into `$03BC`).

`$C930` is the rank-indexed animation cooldown: `80, 75, 70, 65, 60, 40, 30`
frames. `$046C >= 3` is the death threshold (`$C90C`). Score on death is 300
(`$8454`).

### State machine

```
C900:
  type = $010C & $0F               ; 0..3, picks the tile group
  if $046C >= 3: JMP $C776        ; death / ten-kill gate
  $AED7                            ; drift x -= 0.5, despawn at x < 8

  if $5D != 0: return              ; global spawn/effect counter busy
  if $0E >= 4: return              ; VRAM queue nearly full
  if $04AC != 0: $04AC -= 1; return ; animation cooldown

  if $048C == 0:                   ; closed state: aim sector test
      if in_sector(): JMP $C999
      return
  ; $048C != 0: open state
  $048C = 0
  $046C = 0                        ; (!) clears the hit counter when it opens
  $04EC = 0
  $04AC = $C930[rank]
  JMP $C9B4 with group 4*type + $10 (open art)

C999:                                ; reached from the aim test
  $5D += 1
  $040C = 0
  $048C = $14
  $04EC = $14
  $04AC = $C930[rank]
  write group 4*type (closed art)
```

The aim test at `$C954` is a half-plane test, not "within 10 px", and the
carry from `$C91E` makes the `SBC` arithmetic one px larger than the textbook
value:

| type | fires (writes the closed pattern) when |
| --- | --- |
| 0 | `player.x + 10 < obj.x` and `player.y < obj.y + 11` |
| 1 | `player.y - 11 >= obj.y` (player below) |
| 2 | `player.x - 11 >= obj.x` and `player.y - 10 < obj.y` |
| 3 | `player.y + 10 < obj.y` (player above) |

Note `$C937` sets `$046C = 0`: entering the open state erases accumulated
damage. The turret is also un-damageable while `$048C == 0`, because the shot
path `$C06A` skips objects whose state byte is zero. So a `$16` must be
triggered by the player entering its sector before it can be killed.

### Nametable writes (`$C9B4`-`$CA22`)

`$0700` is the VRAM command queue and `$0E` its write index. The writer emits
one or two records of the form

```
<increment flag 01> <addr high> <addr low> <tile...> FF
```

`addr = $03BC:$03EC` (the command record's `b3:b4`), and the optional second
record is at `$03BC:( $03EC + $20 )`, i.e. one nametable row down, when the
group's third byte is zero.

Because the object drifts left at exactly the stage's 0.5 px/frame, the painted
cells and the collision box stay aligned: the tile patch is world-fixed, the
scripted screen position only compensates for the scroll.

### Death (`$C776`) and the ten-kill gate

```
C776:
  $5F += 1
  if $5F >= 10: $39 = 1            ; stage-3 clear gate
  $5D += 1
  score 300                        ; $8454
  $C81C                            ; erase/explode the nametable cells
  A = $0C; $CB20                   ; sound + kind-2 debris
  write explosion tiles around $03BC:$03EC
```

`$C81C` writes `$0F` at offsets `00 08 10 18 20` and `00` at offsets
`07 0F 17 1F 27` from the turret address, then `$C7B0` overwrites them with the
explosion tiles. The turret leaves no OAM sprite: `$A469` never writes
`$012C`, so `$012C = 0` for its whole life.

### Observed

Stage 3 (`$19 = 2`, `--keep-alive`, no input):

| frame | slot | `$030C` | `$010C` | x | y | `$046C` | nametable `$2323/$2324/$2343` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1984 | 8 | `$96` | `$80` | ~`$CB` | `$B8` | 0 | `00 / 8F / 90` (closed) |
| 1992 | 8 | `$96` | `$80` | `$C8` | `$B8` | 0 | `20 / 21 / 22` (open) |
| 1999 | 8 | `$96` | `$80` | `$C4` | `$B8` | 0 | `20 / 21 / 22` |
| 2002 | 8 | `$02` | 0 | `$C3` | `$B8` | 0 | erased/explosion (`00 / B2 / 82`) |

The kill at frame 2002 (after poking `$046C` to 3 at frame 2000) raised the
score from 300 to 600 (+300) and `$5F` from 0 to 1. There is no matching OAM
group at the object's position; the sampled OAM around `($C4,$B8)` contains
only unrelated sprites.

## Kind `$0F`/`$10` (`$AF28`/`$AF82`) and the stage-1 gate

### Init and per-frame body

```
AF28 (kind 0F)                 AF82 (kind 10)
  if kind positive:              if kind positive: JMP AF2D
      $0460 = 1; $048C = 1        ...
      $010C = $80
      JMP $B0AE                  ; add $80 to kind (initialised flag)
  sprite = $78 ($63 if $19 == 5) ; sprite = $79
  $AF92(Y = 8,  shot = $09)      $AF92(Y = -10, shot = $0C)
  $012C = sprite
  ; then $AF51 (shared)
```

`$AF51` is the shared tail and the missing part:

```
AF51:
  hp = $046C
  if hp >= 3: $018C = 3            ; damage flash / state
  if hp < 5:  JMP $AF25            ; $AED7 drift, return
  if $19 != 0: JMP AF7A            ; only stage 1 counts
  y = $07E5 + 4*$18                ; player score, hundreds byte
  if (y & 1) != 0: JMP AF7A        ; parity guard (see below)
  $5F += 1
  if $5F >= 4: $39 = 1             ; stage-1 clear gate
AF7A:
  A = $0A; JSR $CB20               ; sound + kind-2 debris
  JMP $8458                        ; +100
```

`$AF92` fires at most ten aimed shots and then stops:

```
AF92:
  phase = $042C
  phase 0: gate x >= $C8
  phase 1: gate x >= $A0
  phase >= 2: return
  ; only on ($02 & $0F) == 0
  $044C += 1
  if $044C >= 5:
      $044C = 0; $042C += 1
  else:
      spawn shot kind A at (x+8, y+Y)
```

So `$0F` fires five shells of kind `$09` from `y+8`, then five of kind `$09`
from the lower X gate, then stops; `$10` does the same with kind `$0C` from
`y-10`. One shot every 16 frames, 80 frames per gate. The shells are the
ordinary homing shells `$B30B` (`$09`) and `$B3C5` (`$0C`) that the port
already has as `ai_shell`/`ai_diver`.

`$0F`/`$10` are damage-accumulating (`$010C = $80`, `$0460 = 1` -> 32x32 box),
take five ship shots (or three option shots, which add 2) and award 100. The
port currently routes `$046C >= 3` through the generic large-object death, so
it dies too early and never touches `$5F`/`$39`.

### The `$5F` parity guard

Only a kill that leaves the player's hundreds-of-points digit even increments
`$5F`. Because each volcano awards exactly 100, consecutive volcano kills
alternate parity: in a clean pass at most two of the four stage-1 volcanoes
count. This is not a port bug to "fix" blindly:

* `$5F` is **not cleared on death/retry** (`$97D2` clears `$39`, `$3A`, `$5D`
  but not `$5F`); it is cleared only by the stage-advance clear loop
  (`$96C2`: `STA $50,X`, X = `$20..0`, i.e. `$50-$70`).
* Therefore the volcano gate is satisfied across retries: two counted kills per
  attempt reach four after the second pass. A retry restarts the checkpoint
  before the volcanoes, so the same four respawn.
* Other 100-point awards (cannon parts in other stages) also flip the parity.

The port must either reproduce the guard and the persistent `$5F`, or take the
simpler equivalent (count every volcano kill) and note the behaviour change.

### Observed

Stage 1, `--keep-alive --buttons a --poke 400:320:A0` (player pinned at
`y = $A0` so the lower volcanoes are in the shot line):

| frame | `$1B` | `$5F` | `$39` | score | objects |
| --- | --- | --- | --- | --- | --- |
| 3000 | `$80` | 1 | 0 | 000900 | slot 5 kind `$02` debris at `($73,$A2)` |
| 5500 | `$80` | 1 | 0 | 003050 | slot 7 kind `$8F` at `($B6,$A5)` hp 2 |
| 6000 | `$80` | 2 | 0 | 003230 | second volcano killed |
| 6500 | `$82` | 2 | 0 | 003290 | eruption phase, no turrets |

The four stage-1 volcanoes are the terrain records: three type `$06` -> kind
`$0F` at `y = $A5`, one type `$07` -> kind `$10` at `y = $12`. The top one
needs the ship at `y = $12`; a single fixed `y` can kill at most the three
lower ones (the second pass supplies the rest through the persistent `$5F`).

## `$39` / `$5F` lifetime summary

| variable | written | cleared | used |
| --- | --- | --- | --- |
| `$39` | `$AF78` (stage 1), `$B972` (stage 2), `$C780` (stage 3) | `$96C8` stage advance, `$97D2` death/retry | `$9928` stage advance |
| `$5F` | `$AF70` (stage 1), `$C776` (stage 3) | `$96D0` clear loop `$50-$70` on stage advance only | `$AF74` (>= 4), `$C778` (>= 10) |
| `$5D` | `$A32F`, `$A476`, `$BF99`, `$C782`, `$C999` | `$9649` stage init, `$97D6` death | `$C916` (skip animation while non-zero) |

`$5D` is an event/spawn busy counter, not a score counter; `$C999` incrementing
it is the shared-effect request, not a projectile count.

## Porting checklist

`$14` (`CA58`):

1. Add `14 => self.ai_large_object(...)` instead of `_ => ai_drift`; remove the
   `boss: true` from the `$A4A0` spawn (it currently makes the object take the
   boss hit path and die at 7 hits).
2. Keep `$030C = $94` each frame, `$048C = 1`, alternate sprite `$81/$82` on
   `$042C & 8`, and `$83/$84` after the morph.
3. Morph at `$046C >= $CA43[rank]`: one-time `$0460 = 0`, `x += 8`, `y += 8`.
4. Die at `$046C >= $CA4A[rank]`: 500 points, sound `$0A`, kind-2 debris,
   release the `$0600` cannons.
5. Aim: re-arm `$04AC = $02 & 0x3F`, pick up/down from `y` vs player `y`,
   move by `$CA51[rank]/256`, clamp `$20..$A8`.
6. Drift `$AEDB` only while `$016C == 0` (no attached cannons).
7. `$A4A0` fixes: cannon count is `b2 >> 4` (not `b2 & 0x0F`); keep `$016C` =
   cannon count.
8. Implement the `$CB8B` cannon placement/fire loop if stage-5 parity depends
   on it (fire period `$CBC4[rank]`, cannon HP `$BEE4[rank]`).

`$16` (`C900`):

1. Route `22` to a new `ai_background_turret`, not `ai_big_core`.
2. `type = subtype & 0x0F`; `$046C >= 3` -> death (300 points, `$5F += 1`,
   `$39 = 1` at `$5F >= 10`, erase/explode cells, kind-2 debris).
3. Drift `$AED7`; skip animation while `$5D != 0` or `$0E >= 4`.
4. Animate every `$C930[rank]` frames; only start when the player is in the
   sector (`$048C == 0`, the per-type half-plane tests above); on opening set
   `$046C = 0`.
5. Paint the `$CA23` groups into a live nametable map at `$03BC:$03EC` (plus
   `$20` for the optional second tile); `$C81C` erases with `$0F`/`00` and
   `$C7B0` overwrites with explosion tiles. The port has no live nametable
   queue yet, so this is the main new machinery. A sprite-group approximation
   needs new art because these are background tiles (`$20/$21/$22`, `$D9/$DA`,
   `$B0/$B1`, `$CF/$7C`), not OAM groups.
6. Remember `$16` is un-damageable until `$048C != 0`, and that its
   `$012C = 0` means no OAM.

`$0F`/`$10`:

1. Change the death threshold from the generic 3 to 5 for kinds `$0F`/`$10`.
2. Add the `$018C = 3` marker at 3 hits (cosmetic unless the port uses it).
3. Add the stage-1 gate: at `hp >= 5` and `$19 == 0`, apply the `$07E5`
   parity check, increment a persistent `$5F`, and set `$39` at 4. Either copy
   the retry-persistent counter or count every kill and document the change.
4. Keep the existing fire/drift: five kind-`$09` shells from `y+8` and five
   kind-`$0C` from `y-10`, one per 16-frame gate, then stop.

## Corrections to `stage_enemies.md`

* `$16` is a **background-tile turret**, not a bullet turret: `$C900` writes
  only the `$0700` VRAM queue and never calls `$A521`/`$B30B`/`$B3C5`. The
  "four burst patterns" are the closed/open nametable art.
* `$16`'s aim uses half-plane tests, not "within 10 px vertically"; see the
  table above.
* The port's `$C776` comment in `game.rs` ("the Big Core's death") is wrong:
  `$C776` is the `$16` death and the ten-kill stage-3 gate; the Big Core dies
  through `$B95C`/`$BA96`.
* `$5F` is not cleared on death/retry; it is cleared only by the `$96C2` clear
  loop on stage advance. This is what makes the stage-1 parity guard passable.
