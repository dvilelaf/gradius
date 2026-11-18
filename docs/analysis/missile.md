# Missiles (`$A265`, `$A169`)

The missile is a special player shot: the ship and each option spawn one while
A is held, it drops **down and slightly right**, and it rides along the
background after touching it. The native port currently moves it **up and
right**, which is the bug this note pins down.

Addresses are CPU addresses in the fixed `$8000-$FFFF` PRG window. The listing
was produced with the trace requested by the task card:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-disasm

cargo xtask disasm "$ROM" --from A200 --to A2A0 --covered --frames 1200 \
  --press 180:start --press 240:start --buttons a
```

The trace never owns a missile (`$41 = 0`, `$45 = 0` at frame 1200), so
`$A265` is *not* marked executed in that listing. The spawn and update code
below comes from the same static flow (`$A163: JSR $A265`, `$A169`), and the
shot path over the same arrays was verified live:

```sh
cargo xtask ramdump "$ROM" --watch 700:716 --press 180:start \
  --press 240:start --buttons a --assist --range 0120:0130
cargo xtask ramdump "$ROM" --watch 700:716 --press 180:start \
  --press 240:start --buttons a --assist --range 0360:0370
```

The watch shows shot slots `$0123`/`$0363` (sprite 3) and `$0126`/`$0366`
(sprite 6) spawning and moving +7 px X per frame, confirming the parallel-array
layout used by the update loop.

## Slot mapping

The object tables are parallel arrays indexed by a global slot 0-31:

| slot | owner |
| --- | --- |
| 0 | ship |
| 1-4 | options |
| 3-8 | shots (`$0123+i`, `$0363+i`, `$0323+i`) |
| 9-13 | missiles (`$0129+i`, `$0369+i`, `$0329+i`) |

`$A265` is called with `X` = **emitter** (0 = ship, 1..n = options) and writes
the missile into global slot `9 + X`. `$0129,X` is `$0120 + 9 + X`.

## Spawn (`$A265`, reached from `$A163`)

Firing condition (`$A156`-`$A165`):

* `$41 != 0` (missile power installed), **and**
* `$0129,X == 0` (this emitter's missile slot is free), **and**
* `$9B != 0` (`$9B = $07 & $80`, the A button held).

`$A265` writes no velocity and no lifetime; it only copies the emitter
position and marks the slot:

```text
; X = emitter, global slot = 9 + X
$0369,X = $0360,X          ; X int := emitter X int
$0329,X = $0320,X + 6      ; Y int := emitter Y int + 6  (6 px below)
$0129,X = $0A              ; sprite frame $0A
$0169,X = 3                ; kind / active marker
```

Not written: X fraction (`$0389,X`) and any age/velocity field. The motion
below is recomputed every frame from constant tables.

Sprite frame `$0A` is resolved by `$8AB1` through the descriptor table at
`$8D91` (entry `2 * $0A` -> `$8F58`): one 8x8 sprite, tile **`$65`**,
attribute `$22` (palette 2, behind background), offsets 0/0. The ground-riding
frame `$08` resolves to `$8F53`: tile **`$61`**, attribute `$22`, Y offset 8.

## Update (`$A169`-`$A1E0`)

The loop runs `$A8 = 8, 7, 6`, i.e. global slots **11, 10, 9** (only emitters
0-2 are updated). `$0163,X` is the active marker (`$0169` for slot 9).

```text
for X = 8, 7, 6:                 ; slots 11, 10, 9
    if $0163,X == 0: continue

    if $19 == 4:                 ; boss section: no background probe
        index = 0
        sprite = $0A
    else:
        if background_tile($C3A9) == 0:
            index = 0
            sprite = $0A
        else:
            ; forward point is solid: probe 8 px up and 8 px right
            if background_tile($C3CD, Y-8, X+8) != 0:
                goto despawn
            index = 1
            sprite = 8           ; $0163,X = 8 (ground-ride frame)

    $0123,X = sprite             ; $0129,X for slot 9

    $0323,X += tbl_y[index]      ; Y int
    $0383,X += tbl_xf[index]     ; X fraction, carry ripples into X int
    $0363,X  = $0363,X + tbl_xi[index] + carry

    if $0323,X >= $C8: goto despawn
    if carry or $0363,X >= $F8: goto despawn
    continue

despawn:
    $0123,X = 0                  ; clears $0129,X
    $0163,X = 0                  ; clears $0169,X
```

Motion tables (bytes at `$A19E`, `$A1A0`, `$A1A2`, indexed by `index`):

| index | `$A19E` Y int | `$A1A0` X int | `$A1A2` X frac | motion |
| --- | --- | --- | --- | --- |
| 0 (free flight) | `$02` | `$00` | `$80` | **+2.0 px down, +0.5 px right per frame** |
| 1 (ground ride) | `$00` | `$02` | `$00` | +2.0 px right, no vertical |

Raw bytes: `$A19E = 02 00`, `$A1A0 = 00 02`, `$A1A2 = 80 00`.

So the missile has **no gravity and no acceleration**: a constant 2 px/frame
down and 0.5 px/frame right (X integer gets +1 every second frame through the
fraction carry). After the background probe hits a wall/ground and the shifted
probe is clear, it switches to frame `$08` and slides horizontally at 2 px/
frame instead.

## Lifetime and despawn

There is no timer. A missile despawns when any of these happens:

* `$0329` (Y int) `>= $C8` (200),
* X carry out of `$F8` (248) or `$0369 >= $F8`,
* both the forward and shifted background probes are solid.

With the ship at Y=96 the spawn Y is 102, so free flight lasts about
`(200 - 102) / 2 = 49` frames; the X bound (0.5 px/frame) is never reached
first. Despawn clears both `$0129,X` and `$0169,X`, which frees the slot for
the next held-A frame.

## Port mismatch

`crates/gradius-core/src/game.rs`, `update_shots`, kind `_` arm (`:1276`-
`:1284`) models the missile as an accelerating **up-right** projectile:

```text
shot.age += 1
if age < 12: y -= 4 px; x += 4 px    ; WRONG: up, not down
else:        x += 7 px               ; WRONG: horizontal, not 0.5 px right
if x >= $F8 || y < $10: deactivate   ; WRONG: despawns at the top
```

Correct model:

```text
// no age.
y += 2 px; x += 0.5 px              // Fixed::add(0x0100) / Fixed::add(0x0080)
if x.int >= 0xF8 || y.int >= 0xC8 { active = false }
// optional: on background contact switch to +2 px X / 0 px Y (frame $61)
```

Spawn itself matches the ROM: `y + 6`, X copied from the emitter, kind 3
(`:1206`-`:1220`). `base = 6 + emitter` (`:1211`) is the port's compacted
slot number (0-8); it is slot `9 + emitter` in the ROM's global numbering and
is not itself a motion bug. Secondary mismatch: `art::MISSILE`
(`crates/gradius-core/src/art.rs:153`) uses bank-1 tile `$4C`, but the ROM
frame `$0A` draws tile `$65` (ground-ride `$61`), palette 2, behind the
background.
