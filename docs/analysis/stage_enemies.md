# Stage enemies: kinds, handlers and port gaps

What each stage's spawn streams place, which `$AE16` handler runs it, and what
the Rust port still lacks. Stage 1 behaviour is covered by `enemy_ai.md`; this
note covers stages 1-7 together so the remaining stage work can be costed.

Everything below is decoded from the ROM with the new decoder, not copied from
the cartridge. The handler addresses come from the `$AE16` table itself; the
port status comes from the `game.rs` kind match in `Game::update_objects`.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stageai

# matrix: kind x stage, handler address, raw types per stage
cargo xtask stage-enemies "$ROM"

# the per-kind dispatch table and the command paths used below
cargo xtask disasm "$ROM" --from AE00 --to AE70 --labels
cargo xtask disasm "$ROM" --from A460 --to A521 --labels
```

## How a stream type becomes a kind

The stream byte is a **type**, not the `$030C` kind:

* Type `>= $80`: `$A367` drops the high bits (`type & $3F`) and reads four
  bytes at `$A5FC`; the second byte is the kind.
* Type `< $80`: `$A391` reads three bytes at `$A65C`; the first byte is the
  tile base. `$A3AB` maps it: `base - $A0` when `$A0 <= base < $D0`, otherwise
  `base - $D0`. The result is the kind.
* Five-byte command (`second byte >= $F0`): `$A460`. For `$19 == 2` (stage 3)
  it always writes kind `$96` (`$A495`); `$AE13` drops bit 7, so the dispatch
  kind is `$16`. Every other stage takes `$A4A0`: a large object whose kind is
  the fourth command byte (`$66`) plus `($65 >> 4)` cannon records.

Only stages 3 and 5 have command records: 53 in stage 3, 20 in stage 5.
Stages 1, 2, 4, 6 and 7 have none (this matches `enemy_ai.md` for stage 1 and
`stage2.md` for stage 2).

## Stage x kind matrix

Counts are spawn records plus command spawns. The handler is the word at
`$AE16 + 2*kind`. Port status is the `game.rs` match arm.

| kind | handler | port | st1 | st2 | st3 | st4 | st5 | st6 | st7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `$04` | `$B1FF` | `ai_arc` | 19 | 10 | 4 | 18 | . | 15 | 24 |
| `$05` | `$B0A9` | `ai_scout` | 8 | 10 | 4 | 8 | . | 4 | 4 |
| `$06` | `$B192` | `ai_arc` (flip) | 22 | . | . | 22 | . | . | 6 |
| `$07` | `$B6DB` | `ai_walker(8)` | 8 | . | . | 9 | . | . | 18 |
| `$08` | `$B266` | `ai_seeker` | 22 | 7 | 4 | 20 | 8 | 10 | 1 |
| `$0B` | `$B379` | `ai_launcher` | . | 5 | . | . | . | . | 9 |
| `$0D` | `$B3FC` | `ai_arc_toggle` | . | . | 1 | 1 | 3 | . | . |
| `$0E` | `$B42E` | `ai_arc_toggle` | . | . | 1 | 1 | 3 | . | . |
| `$0F` | `$AF28` | `ai_turret_fire` (**partial**) | 3 | 7 | . | 2 | . | 2 | . |
| `$10` | `$AF82` | `ai_turret_fire` (**partial**) | 1 | 2 | . | 3 | . | . | 2 |
| `$11` | `$B020` | `ai_turret` | 6 | 55 | 11 | 7 | . | 9 | 14 |
| `$12` | `$B092` | `ai_turret` | 5 | 35 | 4 | 4 | . | 9 | 17 |
| `$13` | `$B741` | `ai_walker($F8)` | 14 | . | . | 12 | . | . | 18 |
| `$14` | `$CA58` | **missing** (drift) | . | . | . | . | 20 | . | . |
| `$16` | `$C900` | **wrong** (`ai_big_core`) | . | . | 53 | . | . | . | . |
| `$17` | `$B79B` | `ai_miniboss` | . | . | 1 | . | . | . | . |
| `$1A` | `$B47A` | `ai_spin` | . | . | . | . | . | 53 | . |
| `$1C` | `$B4F7` | `ai_descender` | . | . | 2 | . | . | . | . |
| `$1D` | `$B553` | `ai_straight` | . | . | . | . | 10 | . | . |
| `$1E` | `$B563` | `ai_wall` | . | . | . | . | . | . | 2 |
| `$20`-`$25` | `$AF0A` | `ai_bg_piece` | . | . | . | . | . | . | 2 each |
| `$27` | `$AED7` | `ai_drift` | 3 | 1 | 1 | 2 | . | 2 | . |
| `$29` | `$AED7` | `ai_drift` | 1 | 1 | 1 | 2 | . | . | . |

The shared boss and cover kinds are not in the streams:

| kind | handler | port | spawn |
| --- | --- | --- | --- |
| `$18` | `$B90E` | `ai_big_core` | `$9975` state `$84`, x `$F0`, y `$80` |
| `$19` | `$B90D` (`RTS`) | inert | two covers created by `$B9B1`, slots 7/8 |

## Raw types per stage

From `cargo xtask stage-enemies` (terrain types `< $80`, formations `>= $80`):

| stage | types | commands |
| --- | --- | --- |
| 1 | 18 types (12 terrain, 6 formations): `00`-`09`, `69`, `74`, `80`-`84`, `96` | 0 |
| 2 | 43 types (35 terrain, 8 formations): `0A`-`2A`, `71 75`, `82 83 85 86 87 88 8C 96` | 0 |
| 3 | 18 types (10 terrain, 8 formations): `2B`-`31 33`, `72 76`, `87 89 8A 8B 8D 8E 8F 90` | 53, all -> `$96`/`$16` |
| 4 | 23 types (12 terrain, 11 formations): `06 07 34`-`3B`, `6A 77`, `83 84 87 89 8A 8B 8F 90 94 95 96` | 0 |
| 5 | 5 types (0 terrain, 5 formations): `87 8F 90 91 92` | 20, all -> `$14` |
| 6 | 34 types (29 terrain, 5 formations): `3C`-`40`, `4C`-`62`, `73 78`, `83 87 89 8A 8B` | 0 |
| 7 | 32 types (23 terrain, 9 formations): `41`-`4B`, `63`-`68`, `6B`-`70`, `83 87 88 89 8A 93 94 95 97` | 0 |

The important consequence: **only kinds `$14` and `$16` are unimplemented or
mis-implemented content in the stage streams.** Every other stream kind already
has a `game.rs` handler. Kinds `$1B` (`$B4EC`), `$26` (`$B618`) and `$28`
(`$BB09`) have no port handler but no stage stream spawns them either; `$28`
is the `$BB09` stage-handoff object.

## Missing or partial handlers

| kind | ROM | size | status |
| --- | --- | --- | --- |
| `$14` | `$CA58` | `$C8` | missing; `_ => ai_drift` |
| `$16` | `$C900` | `$11C` | routed to `ai_big_core` instead of the turret routine |
| `$0F`/`$10` | `$AF28`/`$AF82` | `$5A`/`$95` | fire and drift are ported; the `$046C >= 5` death, `$5F` counter and stage-1 `$39` increment are not |
| `$1B` | `$B4EC` | `$0B` | missing (unused by stages) |
| `$26` | `$B618` | `$0A` | missing (unused by stages) |
| `$28` | `$BB09` | `$73` | missing (unused by stages) |

`$14` (`$CA58`): indexed by rank `$17`, it takes its hit limit from `$CA43`
and its second threshold from `$CA4A`, drifts with `$AEDB`, and aims up/down
at the player at `$CA51[rank]`; at `$046C >= $99` it awards `$8450` (500),
plays sound `$0A` and runs `$CB48`. This is the stage-5 command object.

`$16` (`$C900`): subtype `($010C - $70) & $0F` selects one of four burst
patterns (`$CA23`), the object drifts (`$AED7`), and when the player comes
within 10 px vertically it fires the pattern once (`$5D++`). `$046C >= 3`
jumps to `$C776`: `$5F++`, and at `$5F >= 10` it sets `$39`. That is the
stage-3 boss-clear gate.

## Stage bosses and unique objects

| stage | boss / clear gate | kind / site | port |
| --- | --- | --- | --- |
| 1 | Big Core; `$39` needs four kind-`$0F`/`$10` kills (`$AF70`-`$AF78`) | `$98` at `$9975`; `$0F`/`$10` | core yes; volcano kill gate no |
| 2 | Big Core; `$39` only when it dies with `$04CC == 1` and `$04AC < $78` (`$B95C`) | `$98` at `$9975` | core yes; `$39` condition no |
| 3 | Big Core plus ten kind-`$16` kills (`$C776`); 53 command turrets | `$98` at `$9975`; `$96` from `$A460` | core yes; `$16` wrong |
| 4 | Big Core | `$98` at `$9975` | core yes |
| 5 | Big Core plus 20 command objects | `$98` at `$9975`; `$14` from `$A4A0` | core yes; `$14` missing |
| 6 | no `$84` core: `$99B7` jumps to `$86`; clear effect `$CD9F` | -- | clear path not modelled |
| 7 | no `$84` core: `$98F7` jumps to the ending `$9865` | -- | ending/loop not modelled |

The `$9975` spawn is reached only through `$81 -> $82 -> $83 -> $84`. Stage 6
(`$19 == 5`) and stage 7 (`$19 == 6`) take the `$99B7` branch and never reach
`$84`; by the disassembly they have no `$98` object. `stages.md` still lists
the shared core for those stages, so this needs one oracle confirmation rather
than a code change. Their stage-advance still checks `$39` at `$9928`, but no
located code sets `$39` for stages 4-7. That is an open question, not an
assumption this note makes.

`$C433[$19]` is the per-stage scripted *hazard* spawner (`$C40D` every fourth
frame, see `stages.md`), not a boss: stage 1 `$C480`, stage 2 `$C540`, stage 3
`$C680`, stage 4 `$C5A7`, stage 5 `$C64D`, stage 6 `$C6D8`, stage 7 `$C423`
(RTS). Those scripts account for several of the "unique objects" listed below
and are the other half of the per-stage enemy work.

Unique objects worth calling out:

* Stage 1: `$0F`/`$10` end-of-stage volcanoes and `$13` ceiling walkers.
* Stage 2: massed terrain emplacements (kinds `$11`/`$12`/`$0F`/`$10`), one
  `$0B` launcher formation (type `$8C`).
* Stage 3: 53 kind-`$16` command turrets, two `$1C` descenders, one `$17`
  small core.
* Stage 4: `$0F`/`$10` volcanoes plus the `$06`/`$07`/`$13` walker families.
* Stage 5: 20 kind-`$14` large objects and ten `$1D` straight pieces.
* Stage 6: 53 kind-`$1A` spin/aim objects; no boss object in the streams.
* Stage 7: six background-piece kinds `$20`-`$25` (types `$63`-`$67`) and two
  `$1E` scroll-freezing walls.

## Effort estimate (AI/boss work only)

Data, backgrounds and music are costed in `stages.md`; this column is only the
enemy/boss code.

| stage | work | estimate |
| --- | --- | --- |
| 1 | volcano `$046C` kill gate and `$5F`/`$39` counter | small (half a day) |
| 2 | stage-2 `$39` condition on core death | small |
| 3 | port `$C900` (`$16`) and its ten-kill `$39` counter; keep the `$A460` branch | medium (1-2 days) |
| 4 | no new handler; wire `$39` (blocked by the open setter above) | small once the gate is known |
| 5 | port `$CA58` (`$14`), 20 command spawns, fix `$A4A0` attachment nibble | large (3-4 days) |
| 6 | no new handler; `$86` clear path and `$CD9F` effect; `$39` gate | medium |
| 7 | no new handler; ending `$9865`/loop and `$39` gate | medium (mostly flow) |

The three biggest gaps are **`$14` (`$CA58`, stage 5)**, **`$16` (`$C900`,
stage 3)** and the missing **`$0F`/`$10` kill counter** that is the stage-1
boss gate. `$BB09` (`$28`) is the largest unimplemented routine after those,
but no stage stream spawns it.

## Open questions

* Where `$39` is set for stages 4-7; `$9928` requires it before the stage
  advance, but the only writers found are `$AF78` (stage 1), `$B972` (stage 2)
  and `$C780` (stage 3).
* The exact behaviour of the `$99` covers (inert in the ROM; the port syncs
  their positions).
* Whether kind `$0A`/`$15` (`$B369`/`$B371`) are distinct arc variants; the
  port folds both into `ai_arc`.
* The `$A4A0` cannon attachment count: the note in `enemy_ai.md` says
  `($65 >> 4)`, while `game.rs` currently uses `bytes[2] & 0x0F`.
