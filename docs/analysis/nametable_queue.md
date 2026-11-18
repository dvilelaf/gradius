# Stage 3 `$96`/`$C900`: the nametable-turret queue

The stage-3 mini-boss is a background-tile turret: kind `$96`, handler `$C900`.
It has no OAM sprite (`$012C` is never written) and fires no projectile. It
paints and erases nametable cells through the `$0700` VRAM command queue and
sets the ten-kill `$39` gate. This note decodes that mechanism exactly, records
the frame evidence, and specifies the queue/overlay the software renderer needs
for card **S3-02**. It corrects two points in `handlers_14_16.md`: the `$048C`
state naming is inverted against the art it paints, and `$C81C` writes the
packed collision map (`$0500`/`$0600`), not the nametable.

Everything below is derived with `cargo xtask disasm`, `probe` and the new
`nametable` command; no cartridge bytes are copied into the port.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-nametable

cargo xtask disasm "$ROM" --from C900 --to CA40 --labels --symbols tools/ghidra/symbols.txt
cargo xtask disasm "$ROM" --from C776 --to C8B0 --labels --symbols tools/ghidra/symbols.txt
cargo xtask disasm "$ROM" --from 8A56 --to 8AA0 --labels --symbols tools/ghidra/symbols.txt

ST3=$(for f in $(seq 170 320); do printf -- '--poke %d:26:2 ' $f; done)

# live handler state and nametable cell
cargo xtask probe "$ROM" --frame 1914 --press 180:start --keep-alive $ST3 \
  --ram /tmp/opencode/t1914.ram --nametables /tmp/opencode/t1914.nt

# which VRAM writes the queue produced, frame by frame
cargo xtask nametable "$ROM" --from 1905 --to 1916 --press 180:start \
  --keep-alive $ST3 --range 2000:2FFF --out /tmp/opencode/t-paint.txt
cargo xtask nametable "$ROM" --from 1998 --to 2004 --press 180:start \
  --keep-alive $ST3 --poke 2000:474:3 --out /tmp/opencode/t-death.txt

# ten-kill gate: nine counted kills plus one forced kill
cargo xtask probe "$ROM" --frame 2002 --press 180:start --keep-alive $ST3 \
  --poke 2000:5F:9 --poke 2000:474:3 --ram /tmp/opencode/t-gate.ram
```

`xtask nametable` samples the PPU's `$2007` writes because the `$0700` page is
already cleared by the NMI that flushes it. Slot 8's `$046C` is `0x046C + 8`.

## Command path: how a `$96` object is created

`$A460` reads the five-byte stream record `[delay, type, b2, b3, b4]` after
`$A387` has done `type -= $70`. Stage 3 (`$19 == 2`) takes `$A469`:

| object field | value |
| --- | --- |
| `$030C` kind | `$96` |
| `$010C` subtype | `type - $70` = `$80..$83` (`& $0F` selects the art group) |
| `$036C` X | `$F0` (off the right edge, then `$AED7` drift) |
| `$038C` X fraction | 0 |
| `$032C` Y | `b2` |
| `$03BC` | `b3` (nametable address high) |
| `$03EC` | `b4` (nametable address low) |
| `$012C` sprite | **0** (no OAM) |
| `$0460` size class | **0** (not written; the 16-px box) |
| `$046C` hit counter | 0 |
| `$048C` state | 0 |
| `$04AC` cooldown | 0 |
| boss flag | **none**; `$A469` never touches `$5D`/`boss_spawned` |

The port's current `command()` stores `sprite 0x89`, `size_class 1`,
`boss = true`, and puts `b3:b4` into the *velocity* words. S3-00/S3-02 must
replace that with a dedicated nametable-address field (`nametable_addr: u16`
or `byte_bc`/`byte_ec`) and drop the sprite/size/boss inventions.

## `$C900` state machine

`$048C` is a two-valued phase, but its name is not the visible art:

| `$048C` | art painted when entering the phase | damageable | `$04EC` |
| --- | --- | --- | --- |
| `0` | **base group** `4*type` (`00 8F` … `CF 7C`) | no (`$C06A` skips `$048C == 0`) | 0 |
| `$14` | **alternate group** `4*type + $10` (`20 21` … `28 29`) | yes | `$14` |

The transition `0 -> $14` happens only when the aim sector passes; the
transition `$14 -> 0` happens automatically after the cooldown. `$046C` is
cleared on the `$14 -> 0` transition, so damage cannot survive the invulnerable
window.

```text
C900:
  type = $010C & $0F                 ; $A9, 0..3
  if $046C >= 3: JMP $C776           ; death
  JSR $AED7                          ; X -= 0.5 px/frame, despawn at X < 8

  if $5D != 0: return                ; frame-scoped spawn/effect busy
  if $0E >= 4: return                ; VRAM queue already has bytes
  if $04AC != 0: $04AC -= 1; return  ; animation cooldown

  if $048C != 0:                     ; $14 -> 0
      $048C = 0
      $046C = 0
      $04EC = 0
      $04AC = $C930[rank]
      JMP $C9B4 with group 4*type + $10
  ; $048C == 0
  if not in_sector(type): return     ; retries every frame while $04AC == 0
  $5D += 1
  $040C = 0
  $048C = $14
  $04EC = $14
  $04AC = $C930[rank]
  JMP $C9B4 with group 4*type          ; $C999
```

`$AED7` also returns immediately while `$5B != 0` (the scroll-hold flag), so a
turret does not drift during a boss/clear hold. The port's existing
`ai_drift`/`Fixed` handles the 0.5 px/frame motion; the turret must not use
`y_vel` for the nametable pair.

### Constants

`$C930`: cooldown in frames, indexed by `$17`:

| rank | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| frames | 80 | 75 | 70 | 65 | 60 | 40 | 30 |

`$17 == 7` reads the first byte of the next table (`$C937 = $A9`, 169), the
same one-byte overrun quirk as `$CA43`/`$CA51`. Record it or clamp it.

Death threshold is `3` (`$C90C`), score `300` (`$8454`), sound `$0C`.

### Aim sector (exact carries)

`$C954` is four half-plane tests. The SBC carry is not always set, so the
effective constants differ by type:

| type | opens (`$048C` set to `$14`) when |
| --- | --- |
| 0 | `player.x + 10 < obj.x` **and** `player.y - 11 < obj.y` |
| 1 | `player.y - 11 >= obj.y` (player below) |
| 2 | `player.x - 10 >= obj.x` **and** `player.y - 10 < obj.y` |
| 3 | `player.y + 11 < obj.y` (player above) |

The `-11` in types 0/1 comes from entering `$C954` with carry clear (the
`$0E < 4` branch at `$C91E`); type 2 keeps the carry set by `CPY #$02`; type 3
keeps it for `ADC #$0A`. `handlers_14_16.md` lists type 2 as `-11` and type 3
as `+10`; the disassembly above is authoritative.

## `$C9B4`: the nametable writer

`$CA23` is eight 4-byte groups. `X = 4*type` for the base group and
`X = 4*type + $10` for the alternate:

| type | base group | alternate group (`+$10`) |
| --- | --- | --- |
| 0 | `00 8F 00 90` | `20 21 00 22` |
| 1 | `D9 DA FE FE` | `2E 2F FE FE` |
| 2 | `B0 00 00 B1` | `24 25 00 27` |
| 3 | `CF 7C FE FE` | `28 29 FF FE` |

Each group is `[tile0, tile1, third, tile2]`. The writer emits one or two
records at `$03BC:$03EC`:

```text
C9B4(group X):
  queue type 1
  queue high = $03BC
  low = $03EC
  if type == 2: low -= 1                  ; 8-bit wrap, high unchanged
  queue low
  queue tile0, tile1
  queue $FF
  if group[2] == 0:
      queue type 1
      queue high = $03BC + carry($03EC + $20)
      queue low  = $03EC + $20
      queue tile2
      queue $FF
  $0E = next free index
```

Two easy-to-miss details:

* The type-2 `-1` applies to **both** art groups and never borrows into the
  high byte. The second record always uses the **unadjusted** low byte + `$20`.
* `group[2]` is tested for zero only. Type 3's alternate ends `28 29 FF FE`;
  the `FF` is a tile-group byte, not a terminator, so it writes one record.

## The `$0700` queue

`$0700-$07FF` is a byte queue; `$0E` is the next free index. NMI `$8A56`
consumes it before gameplay runs:

```text
8A56:
  Y = 0
  loop:
    type = $0700[Y]; if type == 0: clear $0700[0] and $0E; restore PPUCTRL; RTS
    $2000 = ($10 & $18) | $8A50[type]
    Y += 1; $2006 = $0700[Y]         ; address high
    Y += 1; $2006 = $0700[Y]         ; address low
    Y += 1
    loop:
      byte = $0700[Y]; Y += 1
      if byte != $FF: $2007 = byte; continue
      type = $0700[Y]
      if type >= 3: $2007 = $FF      ; the >=3 quirk; unused by $C900
      else: goto loop               ; same type byte is consumed next
```

`$8A50` bytes are `60 00 04 00 04 00`: index 1 -> `0`, index 2 -> `$04`
(vertical increment), and higher indices alternate. `$C900` only uses type 1,
so the emitted stream for the f1991 open paint is exactly:

```text
01 23 23 20 21 FF  01 23 43 22 FF
```

which is what `xtask nametable` observes (one frame later than the handler
builds it): `$2323 <- $20`, `$2324 <- $21`, `$2343 <- $22`.

`$0E >= 4` is a byte-occupancy gate, not a record count. Once any earlier
system has queued a record, the turret skips its paint that frame.

`$5D` is a frame-scoped "an event/spawn happened" flag. The turret checks it
before painting and increments it when it opens and when it dies. It is not a
free-running counter: `probe` shows `$5D = 0` with two live turrets and `$5D = 1`
on the frames a turret painted/died. Model it as a per-frame busy bit that any
spawn/effect sets.

## Death (`$C776`) and the ten-kill gate

```text
C776:
  $5F += 1
  if $5F >= 10: $39 = 1          ; stage-3 clear gate
  $5D += 1
  score += 300                   ; $8454
  $C81C                          ; packed collision-map patch
  $98/$99 = $03BC/$03EC
  sound/debris A = $0C; $CB20
  $A9 *= 2; pointer = $C88D[$A9] ; $C895/$C8EB/$C8B7/$C8DA
  $C7B0                          ; explosion nametable records
```

`$5F` is the same byte as stage 1's volcano counter. It is **not** cleared on
death/retry (`$97D2` clears `$39`, `$3A`, `$5D`); only the stage-advance clear
loop (`$96C2`) clears it. Ten turret kills set `$39`; `$39 != 0` selects the
`$8E`/`$9842` clear path (+500, `$3A += 1`) at `$9928`.

Evidence: poking `$5F = 9` and forcing one kill leaves `$5F = 10`, `$39 = 1`,
score 0 -> 300 (and 300 -> 600 with the normal one kill).

### `$C81C` is the collision map, not VRAM

`$C81C` builds a pointer into `$0500`/`$0600` — the packed 2-bit-per-tile
collision map that `$C39D` reads — from the turret's nametable address. It
writes `$0F` at offsets `00 08 10 18 20` (`$C875`) and `$00` at offsets
`07 0F 17 1F 27` (`$C87B`) from the derived cell. Type 2 starts at `- $29`
(`$C869`). `$0F` and `$00` are collision bytes, not nametable tiles.

The port's `terrain_solid` currently reads only the stage-1 `COLLISION_MAP`
(`0x0500`-style packing, 448 columns x 8 groups). Stage 3 has no extracted
collision map yet, so S3-02 can either skip this write (visual/gameplay is
unaffected for the turret itself) or add a sparse world-keyed override when a
stage-3 map exists. Do **not** write it into the nametable overlay.

### `$C7B0` explosion records

The pointer table `$C88D` is `[C895, C8EB, C8B7, C8DA]` for types 0..3. Each
stream is groups of `[offset, tiles...]`; a group terminator `$FF` means "next
group subtracts this offset from the base", `$FE` means "one final group adds
its offset", and the last `$FF` ends. Decoded:

| type | groups (addressed from `$03BC:$03EC`) |
| --- | --- |
| 0 | four groups of blanks (3, 5, 4 and 4 cells) above the turret, then a 4-cell row (one blank, three explosion tiles) and the 8-tile strip at `+$1E` |
| 1 | an 8-tile row at `-$25`, a 6-cell mixed row at `-$04`, one blank cell |
| 2 | the same four blanking groups (3, 5, 4 and 5 cells), then the 5-cell row of type 3 and the strip at `+$1B` |
| 3 | a 5-cell row (blank, three tiles, blank) and the 8-tile strip at `+$1B` |

The subtraction groups blank the cells above the turret; the final add group
draws the explosion. Verified at frame 2000 for the type-0 turret at
`$2323`: writes `$22C6..C8`, `$22E4..E8`, `$2304..07`, `$2323..26`,
`$2341..48` with exactly the bytes above.

## Frame evidence

Stage 3 (`$19 = 2`, `--keep-alive`, no input); slot 8 is the turret at
`$03BC:$03EC = $2323`.

| frame | `$048C` | `$046C` | `$04AC` | `$04EC` | x | nametable writes |
| --- | --- | --- | --- | --- | --- | --- |
| 1914 | `$14` | 0 | 70 | `$14` | 239 | base group already present (painted earlier) |
| 1991 | `$14` | 0 | 0 | `$14` | 200 | handler will flip to alternate |
| 1991 | — | — | — | — | — | `$2323<-20`, `$2324<-21`, `$2343<-22` |
| 1992 | 0 | 0 | 70 | 0 | 200 | alternate group visible |
| 2064 | 0 | 0 | 0 | 0 | 164 | handler will flip back to base |
| 2064 | — | — | — | — | — | `$2323<-00`, `$2324<-8F`, `$2343<-90` |
| 2065 | `$14` | 0 | 70 | `$14` | 163 | base group visible |
| 2000/2001 | `$02` (dead) | — | — | — | — | explosion writes above; score +300, `$5F` 0 -> 1 |

The f1913 base paint observed through the queue is `00 8F 90`; the f1991
alternate paint is `20 21 22`. The `$04AC` value returns to `$46` (70) after
each paint, and the 1992 -> 2064 cycle is 72 frames (70-frame cooldown plus
`$5D`/`$0E` skips).

## Interaction with the extracted `STAGE3` map

The extracted 480x28 map holds the stage background's own art, not a nametable
sampled after a turret painted it. `stage_bgs::STAGE3.tile()` returns `0x20,
0x21, 0x22` at `(131,25)`, `(132,25)`, `(131,26)`, but the background stream
writes those cells, not `$C900`. Proof: patch `$C916` to `RTS` so the turret
never paints, then run `xtask screens` on that ROM with the turret filter
disabled. The wide map is byte-for-byte identical to the shipped `STAGE3`,
those cells included, and hashes to `D8202719436CC2F9`. Both the turret's base
group `00 8F 00 90` and its alternate group `20 21 22` are legitimate art;
neither leaks into the background. `xtask screens --stage 3` still subtracts
turret writes, but on this stage there is nothing to subtract.

Consequences for S3-02:

1. The static background holds base art at the turret world cells, and the
   runtime overlay replaces it with the turret's live phase; without the
   overlay the first frames of a turret's life show the captured art.
2. Overlays must be **world-keyed**. `$03BC:$03EC` is a rolling 2-page
   nametable address: the same address aliases a different world cell every 64
   tile columns (512 px). Convert the record to a world cell at paint time
   (`world_column(addr, scroll)`, or equivalently `(scroll + obj.x) / 8`); never
   index collision or tile overrides by the raw address.
3. No explicit background restore is needed. The NES cell is overwritten when
   the background stream next writes the column 64 columns later, which for a
   turret is long after it has scrolled off the left edge (lifetime is 464
   frames = 232 px < 512 px). A world-keyed port overlay can be dropped when
   its column leaves the camera; keeping it is observationally equivalent
   because that world cell is never revisited. The `$C81C` collision patch has
   the same lifetime.

`world_column` in `xtask/src/nametable.rs` implements the address conversion:
`encoded = ((addr >> 10) & 1) * 32 + (addr & 31)` is `column % 64`; the first
matching column at/after `scroll / 8` is the live one. `row = (addr & 0x3FF) >> 5`.

## What a clean port needs

### 1. A sparse nametable overlay

```rust
struct CellPatch { world_col: u16, row: u8, tile: u8 }

// Game-owned, per frame and persistent:
pending_cells: Vec<CellPatch>,   // queue-ordered, applied this render
cells: Vec<CellPatch>,           // live overlay (or a HashMap)
vram_queue_len: u8,              // models $0E for the >= 4 gate
```

`queue_cell(world_col, row, tile)` pushes to `pending_cells` and advances
`vram_queue_len`; a turret refuses to paint when `vram_queue_len >= 4`.
`pending_cells` is drained into `cells` after the frame's handlers, in order
(later writes win). Reset `vram_queue_len` each frame, like the NMI flush.

### 2. Renderer hook

Apply the overlay immediately after `draw_screen` and before the HUD band and
front sprites, so it is background and sprites still draw over it:

```rust
draw_screen(fb, assets, screen, scroll - 1, rows, STAGE_MAP_Y, tail);
draw_cell_overlay(fb, assets, screen, scroll - 1, &self.cells);
```

`draw_cell_overlay` reuses `draw_screen`'s tile/attribute/palette lookup
(CHR bank 1, the extracted `STAGE3` attribute table) and draws each patch at
`x = world_col * 8 - scroll`, `y = row * 8 + STAGE_MAP_Y`. It must skip cells
outside `0..WIDTH` and past the column-416 tail rule. Do not mutate the static
`STAGE3` RLE.

### 3. Turret handler and fields

* Add `nametable_addr: u16` to `Object` (S3-00), or reuse two new `$03BC/$03EC`
  bytes; keep `x_vel`/`y_vel` free.
* Route kind `$16` (`0x16`) to `ai_background_turret`, not `ai_big_core`.
* Keep `sprite = 0`; `build_sprites` already skips it. Do not set `boss`.
* Use `ai_state = $048C`, `ai_timer = $046C`, `ai_mode = $04AC`,
  `timer2 = $04EC`.
* Hit path: large objects already gate on `ai_state != 0` and accumulate in
  `ai_timer`; add `low == 22` to the "let the handler die" exception (like
  kinds 15/16) so `$C776` runs in the handler, not the generic 100-point
  `destroy`.
* `$5D`: model as `spawn_busy: bool`, set by any spawn/effect and cleared at
  frame end. The turret skips when set, and sets it when it opens/dies.
* `$5F`/`$39`: a stage-3 persistent kill counter (do not reset on death); at
  10 set `stage_clear_ready`. The stage-3 `$39` branch is S3-04's job; S3-02
  only maintains the counter.

### 4. Overlay data per type

Each of the eight types is four overlay tiles read from the cartridge at
`$CA23` (32 bytes): two records for the top and bottom halves of the turret
cell and two more for the cell to its right, with `$FE` padding the unused pair
and `$FF` closing the last group. `gradius-extract` loads them as
`tables.turret_groups`, so they are not repeated here.

Use `xtask::nametable::turret_writes(base, type_low, group)` (already tested)
to derive the two records and the type-2 `-1`/second-row addresses; then map
each record address to a world cell at paint time.

## S3-02 card breakdown

| card | scope | owned files | acceptance | deps | h |
| --- | --- | --- | --- | --- | ---: |
| S3-02 | `$96`/`$C900` handler + overlay + ten-kill `$39` | `game.rs`, new `crates/gradius-core/src/stage3_enemies.rs` | 53 turrets spawn/drift with no OAM; base/alternate groups paint on the `$C930[2] = 70` cycle; type-2 `-1` and second row; half-plane aim (exact carries); HP 3/300; `$5F` persists and tenth sets `$39`; `$5D`/`$0E` gates; f1913/f1991/f2064 tiles match; death explosion matches f2000 | S3-00 | 12 |

Acceptance tests for S3-02:

* spawn: `command()` leaves `sprite == 0`, `boss == false`, `nametable_addr ==
  b3:b4`, y `b2`, subtype `$80..$83`.
* `turret_writes` unit tests (already in `xtask`): type-2 `-1`, low-byte wrap,
  unadjusted second row, nonzero third byte.
* paint: a type-0 turret at `$2323` paints `$2323=00/$2324=8F/$2343=90` then
  `$2323=20/$2324=21/$2343=22` after the cooldown; a type-2 turret paints at
  `base-1`.
* aim: one test per type with the exact constants above, including the
  `-11`/`-10`/`+11` carries.
* damage: shots are ignored while `ai_state == 0`; three hits while `$14`;
  the counter clears on the `$14 -> 0` transition.
* gate: nine kills then a tenth sets `$39`; the counter survives a simulated
  death/retry.
* overlay: world-keyed cells applied after `draw_screen`; the same nametable
  address at scroll + 512 px maps to `world_col + 64`, never to the same cell.
* death: the type-specific explosion records and the `$C81C` collision patch
  are emitted in the f2000 order.

## Risks / open items

1. **`$5D` is global, not per turret.** The oracle shows it toggling on
   paint/death frames while two turrets are live. Model a frame-scoped busy
   flag; do not gate on a counter that never clears.
2. **The static map is stage art, not a turret snapshot.** `(131,25)=20` etc.
   come from the background stream (an `$C916`-RTS ROM extracts the same
   bytes), so no clean re-extraction is needed. The overlay still replaces
   those cells while a turret is live.
3. **The `$C81C` collision map is a different subsystem.** It is not a
   nametable erase and must not be implemented as one. Stage 3 has no
   extracted collision map yet.
4. **`$03BC:$03EC` aliases every 64 columns.** Convert to a world cell at
   paint time; indexing the static overlay by the raw address would corrupt
   a later turret.
5. **`$17 == 7` cooldown overrun.** `$C930[7]` is code (`$A9`), not 30; decide
   to reproduce or clamp and note it next to the `$CA43`/`$CA51` quirks.

## Corrected statements in `handlers_14_16.md`

* `$048C == 0` is the **alternate/`+$10` art and invulnerable** state;
  `$048C == $14` is the **base art and damageable** state. The doc's
  "closed/open" prose is backwards.
* `$C81C` writes the packed `$0500`/`$0600` collision map; only `$C7B0`
  writes nametable explosion records.
* Type 2's aim X constant is `-10`, not `-11`; type 3's Y constant is `+11`,
  not `+10`.
