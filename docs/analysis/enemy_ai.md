# Stage-1 enemy and Big Core behaviour

Evidence for `crates/gradius-core/src/stage1_enemies.rs`. Everything here was
measured with the oracle (`crates/gradius-nes`) through `cargo xtask disasm`,
`ramdump`, `probe` and `shot`; no cartridge bytes are reproduced. The ROM is
read-only: `$GRADIUS_ROM`.

The two previous documents (`stage1_data.md`, `ram_map.md`) describe the
spawner and the object arrays. This one closes the per-type behaviour and the
Big Core, and corrects one earlier assumption: **stage 1 has no 5-byte command
records and the Big Core is not spawned from the stream.** It is spawned by the
stage-state handler at `$9975` (see below).

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-title

# decoded formations for the five stage-1 stream types (new `spawn` command)
cargo xtask spawn "$ROM"

# the spawner, the per-type dispatch and the object setup
cargo xtask disasm "$ROM" --covered --labels --frames 3000 \
  --press 180:start --from A2B0 --to A5F0 --out spawn.txt
cargo xtask disasm "$ROM" --covered --labels --frames 3000 \
  --press 180:start --from AE00 --to B760 --out types.txt

# per-type state: slot 0-9 objects start at $030C/$032C/$036C
cargo xtask ramdump "$ROM" --frame 374 --press 180:start --keep-alive \
  --range 030C:0376

# the stage end and the Big Core. `--keep-alive` keeps the player alive so the
# stage is observed end-to-end (it only pokes the player-death bytes, see
# `xtask/src/oracle.rs`).
cargo xtask ramdump "$ROM" --watch 6400:9000 --press 180:start --keep-alive \
  --buttons a --range 0019:001C
cargo xtask disasm "$ROM" --covered --labels --frames 8600 --press 180:start \
  --keep-alive --buttons a --from B740 --to BC00 --out boss.txt
cargo xtask probe "$ROM" --frame 8500 --press 180:start --keep-alive \
  --buttons a --sprites 64
cargo xtask shot "$ROM" big-core.png --frame 8500 --press 180:start \
  --keep-alive --buttons a

# the whole fight in one command: state line per frame plus boss HP
cargo xtask record-boss "$ROM" --out boss-fight.txt
```

`--keep-alive` is analysis-only. It keeps `$46 = 5` (the game's own shield
timer) and restores `$0100/$4C/$1B` when a hit takes the lethal `$C1D0` path.
Without it the stationary test ship dies at frame 5817 and the stage restarts
from the checkpoint at scroll 2048, so the Big Core is never seen.

The oracle runs without `--assist`; the HUD sprite-0 split no longer stalls.

## Object tables and the spawner

The spawner `$A2BA` reads a 2-byte record `<delay> <type>` from the active
column stream and calls `$A32F`. The second byte is the type; a type `>= $80`
selects a 4-byte definition at `$A5FC` (index = `type & $3F`, because `$A367`
does two `ASL`s), and the definition selects one entry of each of two more
tables:

* `$64` (`$010C`) = subtype / tile base,
* `$65` (`$030C`) = kind, the per-type dispatch index,
* `$66` = index into the size table `$A58C` (2 bytes),
* `$67` = index into the movement table `$A5B6` (3 bytes).

Size entry: low nibble = object count, high nibble = spawn X, second byte =
first Y. Movement entry: spawn gap in frames, Y step, motion flags (bit 0 goes
to `$03AC`, the rest to `$040C/$04EC`).

`$A3DE` then allocates one object per call, searching slots 9 down to 0 for a
free `$030C` (`$A411`); `$A325` waits the movement entry's gap frames before
the next object of the same formation. Slots are 0-9 only; a full table drops
the spawn.

### The five stage-1 types

All five spawn at `x = $F0` (240, off the right edge) with type:

| stream | object kind | subtype | count | base Y | gap | Y step | formation Y | flags |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `$80` | `$05` | `$01` | 4 | `$2A` | 10 | 0 | 42, 42, 42, 42 | `$C8` |
| `$81` | `$05` | `$01` | 4 | `$A0` | 10 | 0 | 160, 160, 160, 160 | `$C8` |
| `$82` | `$08` | `$00` | 3 | `$08` | 2 | `$34` | 8, 60, 112 | `$C8` |
| `$83` | `$04` | `$05` | 2 | `$08` | 2 | `$40` | 8, 72 | `$C8` |
| `$84` | `$04` | `$05` | 2 | `$28` | 2 | `$40` | 40, 104 | `$C9` |

`$80` and `$81` are the same four-ship column at two heights (`$2A` and
`$A0`); because the ships move left at 2 px/frame and the gap is 10 frames,
the column is 20 px apart in X. The other three are two/three-ship diagonal
formations: the Y step already separates them, and the 2-frame gap adds only
4 px of X spacing at their drift speed.

Measured confirmation: the first `$80` object appears at frame 361/362 with
`$032C = $2A`, `$036C = $EE` (it starts at `$F0` and has already moved 2 px).
The next appears 10 frames later at the same Y, then the two remaining ones.

## The 3-byte terrain records

For a stream type below `$80`, `$A354` computes `3 * type` and `$A391` copies
four bytes from `$A65C + 3 * type` to `$64-$67` (the fourth byte is the first
byte of the next record and is not used). `$A3AB` then spawns a single object:

```
type = $64 - $A0, x = $F0     when $A0 <= $64 < $D0
type = $64 - $D0, x = $10     when $64 >= $D0
y    = $66
$A573($65)                    ; motion flags / phase
```

So the byte order is `(subtype, motion, y)`. Stage-1 examples (decoded from the
`$A65C` table): type `$00` = `B2 80 12` -> kind `$12`, x = 240, y = 18; type
`$04` = `E3 80 10` -> kind `$13`, x = 16, y = 16 (the end-of-stage volcanoes);
type `$69` = `C7 00 60` -> kind `$27`, x = 240, y = 96. Terrain types are not
shootable and are not part of the four enemy formations.

## The 5-byte command records

A record whose *second* byte is `>= $F0` is a command (`$A345`). `$A374`
copies all five bytes to `$63-$67`, advances the pointer by 5, subtracts
`$70` from `$64` (`$A387`) and jumps to `$A460`:

* `$19 == 2` (stage 3): spawn kind `$96` in slot 9, `x = $F0`, `y = $65`,
  state `$66`, `$67` (`$A469`). This is the only code path that creates
  kind `$96` (`$C900`).
* otherwise: spawn a big object of kind `$66` at `x = $F0`, `y = $67`,
  sprite group `$89`, `$010C = $80`, plus `($65 >> 4)` attached cannon
  records in the `$0600` option-like structs (`$A4A0`).

**Stage 1 contains no such record.** Scanning all eight column streams of
`$A7D8` gives zero matches for a second byte `>= $F0`; the first commands are
in stage 3 at `$A9E0`. This is why the Big Core is spawned by the state
machine, not by the stream.

## Common pre-pass `$ADA5` and drift `$AED7`

`$ADA5` is not AI: it loops slots 9 down to 0 and runs `$ADDF`, the sprite
animation pre-pass. For a live object with subtype `$010C != 0` and timer
`$014C == 0` it sets the timer to 6, advances `$016C`, and loads
`$012C = $ADBB[4 * subtype + ($016C & 3)]` (zero entries are skipped). The
36-byte table is nine groups of four animation sprites.

`$AED7` is the shared drift/despawn handler used by kinds 1, 2, 3, 27 and 29:
when the scroll is not frozen (`$5B == 0`) it subtracts `$80` from the X
fraction `$038C` (0.5 px/frame), borrows into `$036C`, and destroys the
object when `x < 8` (`$AEF2` clears `$030C/$010C/$012C/$014C/$016C`).

## Per-kind behaviour (`$AE16` dispatch)

`$AE13` dispatches on `$030C` through the table at `$AE15` (base is the pushed
return address, not `$AE16`):

| kind | handler | stage-1 use |
| --- | --- | --- |
| `$05` | `$B0A9` | `$80`, `$81` |
| `$08` | `$B266` | `$82` |
| `$04` | `$B1FF` | `$83`, `$84` |
| `$06` | `$B192` | later streams (hatches/waves) |
| `$0F`, `$10` | `$AF28`, `$AF82` | stage-1 shooters |
| `$13` | `$B741` | end-of-stage volcanoes |
| `$98` | `$B90E` | Big Core core |
| `$99` | `$B90D` (`RTS`) | Big Core cover parts |

Every handler starts by adding `$80` to a positive kind (`$B0AE`) and then runs
on the negative value, so the ROM's `$030C` is `$85/$88/$84/...` after the
first frame. Kinds `$80`-`$9F` are the same handlers.

### Kind `$05` — fan/column ship (`$B0A9`)

`$048C` is the state:

* state 0: `x -= 2` per frame (`$B15E` with `A = $FE`). When `x < $60`, set
  `$046C = $40` and go to state 1, or state 2 when `y >= $80`.
* state 1: turn towards the player in Y (`$B103`), then for 64 frames
  `x += 1`, `y += 2` (`$B176` with `A = 2`).
* state 2: same with `y -= 2`.
* state 3: `x += 3` per frame.
* `$B24B` destroys the object when `x < 4`, `x >= $F4`, `y < 8` or `y >= $C4`.

So it flies in from the right at 2 px/frame, stops at x = 96 and performs a
64-frame U-turn up or down towards the player, then leaves to the right at
3 px/frame. It does not fire.

### Kind `$08` — weaving ship (`$B266`)

* initial `$048C = $80`, `$04AC = 0`.
* `x -= 1` per frame while `$046C`/`$04AC` timers run.
* Y is moved by a 16-bit accumulator (`$03BC` integer, `$03EC` fraction) in
  steps of `$048C = $80`, i.e. 0.5 px/frame, via `$B11A`/`$B12A`.
* if `y < player y` it sets `$04AC = $1E` and descends; if `y > player y` it
  sets `$046C = $1E` and climbs. The direction flips every 30 frames.
* when `y == player y` it takes `$B2D5`: `x += 3` and leaves.

It does not fire. Sprites `$38/$39/$3A` are the animation.

### Kind `$04` — arcing ship (`$B1FF`)

* initial `$048C = $20`, `$03BC = 3.0` (Y velocity), `$042C = $FE` (X = -2).
* X moves by `$042C:$044C` each frame: left at 2 px/frame, or right when
  `$046C != 0`.
* Y velocity starts at 3 px/frame and is reduced by `$048C = $20` per frame
  (`$B11A`), i.e. 0.125 px/frame²; it becomes negative and reverses the arc.
* when the velocity integer underflows below `$FD`, `$04AC` advances through
  `$B1FA = 00 00 01 00 00`; a `1` makes the next arc move right instead of
  left. The result is a left-left-right-left-left weave.
* the same `$B24B` bounds destroy it off-screen.

It does not fire.

## Firing in stage 1

The four formations above are non-shooters. Stage-1 fire comes from kinds
`$8F`/`$90` (`$AF28`/`$AF82`) and the later hatches/volcanoes:

* `$AF92` only runs a shot when `($02 & $0F) == 0`, i.e. every 16 frames.
* it fires a burst of at most 5 shots (`$044C < 5`), then increments `$042C`
  and switches the X gate: phase 0 at `x >= $C8`, phase 1 at `x >= $A0`.
* `$AF28` spawns shot kind `$09` at `(x + 8, y + 8)`; `$AF82` spawns kind
  `$0C` at `(x + 8, y - 10)`.
* both shot kinds home for a while: kind `$09` has a `$0A`-frame delay and
  kind `$0C` `$14` frames (`$B30B`, `$B3C5`). They steer on Y towards the
  player and then leave (`$B2D5` / `$B39C`).
* the "spread" in stage 1 is not a fan pattern: it is aimed single shots from
  each shooter, staggered by their X position and the 16-frame gate.

## Hitboxes and hit points

* Player shot vs enemy: `$C1FC` widths `10 16 16 16`, `$C200` heights
  `08 12 12 10`, indexed by `$0176`. Normal spawns leave `$0176 = 0`, so
  **16x8** for every `$80`-`$84` object.
* Player vs enemy: `$C222` uses the same `$0176` index (`$C1FC/$C200`).
* `$BFD4`/`$BFD8` are a second pair of extents (`10 20 30 10`, `10 20 30 02`)
  indexed by `$0460` (the formation/trail index). The Big Core's guns use
  `$0460 = 1` (32x32) and the core `$0460 = 3` (48x2? see open questions).
* normal enemies (`$80`-`$84`, and most kinds) die in **one** player shot.
  The hit counter is only used for kind `$9A`: `$C093` increments `$04AC` and
  compares it with `$BFBF[$17]`, a nine-entry rank-dependent armour table that
  runs from 5 hits at rank 0 to 10 at rank 8. This is the only "HP" table in
  the ordinary enemy path.
* collision with the player is a rectangle test at `$C222`; the extents are
  read from the same `$C1FC/$C200` tables.

## Score values (measured)

The score is BCD at `$07E4-$07E6` (low pair first). `$845A(A)` loads `A` into
the middle pair, `$8460` uses `$50` in the low pair and `$8468` uses `$10` in
the low pair. The shot-kill path `$C0A0` always calls `$8468`, so **every
ordinary stage-1 enemy is worth 10 points**, independent of its type:

| stream | object kind | score | path |
| --- | --- | --- | --- |
| `$80`, `$81` | `$05` | 10 | `$C08A` -> `$C0A0` -> `$8468` |
| `$82` | `$08` | 10 | `$C08A` -> `$C0A0` -> `$8468` |
| `$83`, `$84` | `$04` | 10 | `$C08A` -> `$C0A0` -> `$8468` |
| `$29` pickup | `$29` | 500 | `$C153` -> `$8450` |
| `$27` pickup | `$27` | 0 (extra life) | `$C137`, increments `$20` |
| Big Core | `$98` | 1000 | `$B95C` -> `$845A` with `A = $10` |

Evidence:

```sh
ROM="$GRADIUS_ROM"
cargo xtask disasm "$ROM" --from 8450 --to 8478 --labels   # $845A/$8460/$8468
cargo xtask disasm "$ROM" --from C040 --to C0B0 --labels   # every shot kill -> $8468
# measured: +10 at every ordinary kill (frames 473, 494, 604, ...), and
# +1000 when the Big Core dies at frame 9037:
cargo xtask record-state "$ROM" --frames 1200 --buttons a --press 180:start \
  --out /tmp/state.txt
cargo xtask record-boss "$ROM" --out /tmp/boss.txt
```

The five stage-1 formation types all land on the `_ => 10` branch. The
larger `$8458` (100), `$8454` (300) and `$8450` (500) paths belong to cannon
parts and the small core, not to `$80`-`$84`.

## The Big Core (`$98` / `$99`)

### It is not a stream command

I scanned every column stream of all eight stage tables (`$A7CA`, entries
`$A7D8`..`$A858`): stage 1 (`$A7D8`) has **no 5-byte command record**, i.e. no
record whose *second* byte is `>= $F0` (`$A345`). The `$A460` command path is
therefore never taken in stage 1, and `$A495` (`kind $96`) is never reached.
The first command records are in stage 3 (`$A9E0` onward).

Instead the Big Core is hard-coded in the stage-state machine. `$1B` is
dispatched at `$981D`; the state sequence at the end of stage 1 is:

```
$80 -> $81 (scroll reached $3F >= $9A30[stage] = $0C)
$81 -> $82 ($9A01: sets the clear timer)
$82: volcano eruption, 768 frames ($99DC)
$83 -> $84 ($99B3)
$84 ($9975): if $3F != $9A30[$19] then spawn the Big Core
             else skip (JSR $993D cleanup)
```

The guard is exact: at frame 7722 `$3F` becomes `$0D`, and at 7723 slot 9
becomes kind `$98`. The spawn writes:

```
$9990  JSR $A521          ; clear slot 9
$9993  LDA #$98; STA $0315 ; kind $98 (core)
$9998  LDA #$80; STA $0335 ; Y = $80 (128)
$999D  LDA #$F0; STA $0375 ; X = $F0 (240)
```

The two `$99` cover objects are created by the core itself at `$B9B1`:
slots 8 and 7, `$030B/$030A = $99`, `$010B/$010A = $80`, sprite groups `$85`
and `$32`; the second one is placed at `y - $14`.

### Entry and movement (`$B90E`)

* the core enters at `x = $F0` and moves left one pixel per frame until
  `x = $A4` (164); `$B9A5`.
* after that it tracks the player horizontally: if `player x >= boss x` it
  adds 2 px/frame (`$BA0C`).
* every `$B904[rank]` frames it re-aims vertically: `$03EC = 0` when
  `boss y - $10 >= player y` (move up), else 1 (move down), then the Y
  accumulator `$034C` is moved by `$B8F2[rank]`/`$B8FB[rank]` (fraction and
  integer part of 1.0-1.9375 px/frame) and clamped to `$18..$A8`
  (24..168).
* `rank = $17` is the power-up counter built at `$9C3F`: weapon level != 0,
  plus options, plus shield, plus stage; it is 1 in the reproduction run
  because `--keep-alive` pins the shield.

The three tables are nine entries each, indexed by `rank` 0..8: the fraction at
`$B8F2` steps up by `$20` from 0 to `$F0`, the integer at `$B8FB` is 0 at rank 0
and 1 for every other rank, and the fire period at `$B904` holds 2 at rank 0 and
then falls from 90 frames to 35. All three come from the cartridge at run time.

Rank 0 is normally unreachable in stage 1 without the shield; the `02` entry
is what the table contains.

### Attack (`$BA12`, `$BA9A`)

Every fire period the core spawns four collidable gun pods into slots 3..0
(the loop fills slot 3 first). The offsets below are indexed by slot:

```
slot 0: (+8,-15)   slot 1: (-8,-2)   slot 2: (-8,+10)   slot 3: (+8,+23)
```

They get `$0136 = $41` (collision enabled), `$0176 = 2` (22x18 hitbox),
`$0436 = $BAF9[rank]` (eight entries: 2 at rank 0, 3 for ranks 1-4, 4 for
ranks 5-8) and `$0456 = $BB01[rank]` (the barrel offset cycles `-$40`, 0,
`$40` and `$80` twice over the eight ranks). These are the four laser barrels. The visible
beams are sprite groups `$0E`/`$10` in sprite palette 1 (measured in the
frame-8500 OAM at x = 2..26, y = 111/149); the routine that moves those
beams is **not yet located** — the pod kind is 0 and the `$BB09` handler is
not a per-frame pod update (open question).

After each fire the counter `$042C` resets to 0 and the vertical direction
stays until the next fire period.

### Damage, barriers and stage end

* `$046C` is the damage counter. Player shots add 1 or 2 (`$C081`-`$C084`),
  and the sprite group steps through `$B8E9 = 6C 6D 6E 6F 70 71` as the
  armour opens. At `$046C >= 7` (`$B92C`) the core dies.
* `$04AC` is a free-running 8-bit counter; it increments every frame the core
  survives. Each wrap increments `$04CC`; at `$04CC >= 6` the core overheats
  and self-destructs (`$BA96` -> `$B97D`). That is **6 * 256 = 1536 frames**
  (~25.6 s).
* during the fight `$048C` toggles at `$04CC == 1` and `$04CC >= 5`, which
  drives the cover animation.
* the two `$99` parts are re-attached every frame by `$B9B1/$BA65`; their
  handler is a bare `RTS`, so they are not damageable objects themselves.
* on death `$B95C` awards `$845A` with `A = $10`, i.e. **1000 points**
  (measured 1690 -> 2690 at frame 9087), increments `$3B`, plays sound `$AC`
  through `$CB20`, clears the three slots and, when the player is alive
  (`$0100 < 2`), increments `$1B`. That starts the stage-clear sequence
  (`$81..$90`) which sets `$19 = $19 + 1` and loads the next stage
  (`$96C4`/`$9865`).
* the same `$048C` toggle is a damage gate: the shot path `$C06A` ignores the
  core while `$048C = 0`, so hits only register between the first and fifth
  overheat wrap (frames 8058-9094 in the reproduction).
* stage-end condition for the port: **destroy the core (7 damage) or wait
  1536 frames**, while the ship is alive. There is no "scroll reaches the
  end" boss trigger independent of `$1B = $84`.

### Measured fight (`xtask record-boss`)

`record-boss` presses Start at 180, holds fire and flies the documented
dodge/aim script. It writes one line per frame in the `Game::dump_state`
format with an appended
`boss=<slot>:<x>:<y>:<damage>:<overheat>:<counter>` field (`boss=-` when no
core is live) and prints a summary. `x`/`y` are the true `$036C/$032C` table
bytes; the player's `$0320/$0360` pair is outside the object table (`$A521`
clears `slot + 12`, i.e. `$012C/$032C/$036C`). The last live line shows damage
6 because the seventh hit increments `$046C` and clears the core in the same
frame. It fails if the core is never spawned or never hit; it uses
`--keep-alive` internally for the same analysis reason as the commands above.

```sh
cargo xtask record-boss "$ROM" --out boss-fight.txt
# boss: spawn 7723 score 1440, scroll stop 7722, first damage 8066,
#       max damage 6/7, end 9087 (boss reward +1000), stage 2 from 9601,
#       final score 2710, 9800 frames recorded
```

| event | frame | evidence |
| --- | --- | --- |
| scroll stops at `$0D00` | 7722 | `$3E/$3F` |
| core spawns, slot 9 kind `$98`, x 240 y 128 | 7723 | `$0315/$0335/$0375` |
| entry ends, x = 164 | 7798 | `$0375` |
| overheat wraps 1/2/3/4 | 8058 / 8317 / 8576 / 8835 | `$04D5` |
| first damage (damage 1) | 8066 | `$0475` |
| last live line (damage 6, 1 HP left) | 9086 | `boss=9:163:168:6:4:248` |
| seventh hit kills the core, +1000 | 9087 | score 1690 -> 2690 |
| scroll resumes | 9088 | `$3E/$3F` |
| stage 2 starts (scroll high back to 0) | 9601 | `$3E/$3F` |
| self-destruct if never hit | 9354 | separate no-hit run, `$04D5` reaches 6 |

The core's collision box is 2 px high (`$BFD8[$0460=3] = 02`), so the script
leads the core's Y by the ~21-frame shot flight and sweeps 16 px either side
every 16 frames; a fixed aim line is shadowed by the four gun pods.

## Corrections to the previous notes

* `$C76C` is not the Big Core: it is a stage hook that runs only when
  `$19 == 4` (stage 5) and calls `$CB84`. `$CB84` checks `$5C` (options held);
  when fewer than two options exist it falls into `$CB8B`, which updates the
  four `$0600/$0630/$0660/$0690` option structs. It is the low-option trail
  update, not a boss.
* Kind `$96` (`$C900`) is only spawned by the `$A460` command path when
  `$19 == 2` (stage 3). It does not occur in stage 1.
* `$B79B` (kind `$97`) is a second core-style object with its own sprite
  table `$B791`; no stage-1 code path reaches it. It is left out of the
  module.

## Proven vs open

Proven:

* the five formation records, their tables and the spawn geometry
  (`spawn` output, `$A3DE`-`$A460`);
* the three movement handlers and their constants (`$B0A9`/`$B266`/`$B1FF`);
* no formation `$80`-`$84` fires; stage-1 aimed fire is the `$AF92` family;
* hitbox tables `$C1FC/$C200` and the one-shot kill for ordinary kinds;
* the Big Core spawn site, entry, stop, tracking, vertical aim, fire period,
  gun offsets, damage limit, 1536-frame self-destruct, the `$0460 = 3` 2 px
  core box and the death trigger;
* the stage-clear sequence and the fact that stage 1 has no command records.

Open / not proven:

* the exact beam projectile object and its motion (sprite groups `$0E/$10`);
* the meaning of `$0436`/`$0456` on the gun pods beyond "per-rank phase/period";
* whether the `$99` covers are meant to absorb shots (their handler is inert,
  and the shot path only tests `$0136`, which they do not set);
* the exact rank values used in a real unassisted run (`$17` depends on the
  power-ups held when the boss spawns).
