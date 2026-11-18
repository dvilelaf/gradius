# Stage-1 capsule drops: oracle vs native port

The user report "power-ups do not work" was first read as "maybe no capsules
drop". This note checks that end of the chain. **Capsules do drop, are
collected, and advance `$42` in the port exactly as in the cartridge.** The
actual missing rule is one step later: activation.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-capsuledrop
```

Oracle, phase (`$03AC`) and subtype (`$010C`) writes for the run used below:

```sh
cargo run -q -p xtask -- ramdump "$ROM" --watch 300:1050 \
  --range 03AC:03B6 --buttons a --press 180:start --press 300:up:54 \
  --assist --keep-alive
cargo run -q -p xtask -- ramdump "$ROM" --watch 300:1050 \
  --range 010C:0116 --buttons a --press 180:start --press 300:up:54 \
  --assist --keep-alive
```

Lockstep oracle/native state, same input (script `top`):

```sh
printf '0:a:1400\n180:start\n300:up:54\n' > /tmp/top.txt
cargo run -q -p gradius-diff -- --rom "$ROM" --pack assets/gradius.pak \
  --frames 1200 --script /tmp/top.txt --keep-alive --native-keep-alive \
  --oracle-out /tmp/oracle.txt --native-out /tmp/native.txt --report
```

The native `dump_state` line exposes the `$03AC` consequence, not the byte:
`objects=slot:kind:x:y:substate` and `capsule=N meter=N`. The port's
`Object::phase` is private, so the `$03AC` value is read in the oracle and its
effect (kind 2 → kind 1, `subtype == 6`, `$42++`) is read on both sides.

## Where a capsule comes from (oracle)

`$AE93` walks the `$AE6B` debris path. At the `$00` terminator `$AEB6` tests
`$03AC`:

* `$03AC == 0` → `$AEF2` clears the slot, no capsule;
* `$03AC != 0` → `$AEBB` writes kind 1 and `$010C = 6` (capsule), or
  `$010C = 7` when `($47 + 1) & $0F == 0`.

`$03AC` is set at spawn by `$A3E9`/`$A573`, and reduced at destruction by
`$BE9F`: 0 stays 0, 1 stays 1, 2/3 decrement the `$0048,class` counter and only
the member that reaches zero becomes 1. So in stage 1:

| stream type | object | count | `$03AC` at spawn | capsule? |
| --- | --- | --- | --- | --- |
| `$80`, `$81` | kind `$05`, subtype `$01` | 4 | class phase `3` / `2` | **last member killed** |
| `$84` | kind `$04`, subtype `$05` | 2 | motion phase `1` (`$C9`) | **every killed member** |
| `$82` | kind `$08` | 3 | `0` | no |
| `$83` | kind `$04`, subtype `$05` | 2 | `0` (`$C8`) | no |

The `$96` stream record is the Big Core command, not a capsule source. The
earlier measurement in `powerups.md` (type `$84`, slot 8, f1769 → fold f1821 →
pickup f1997) is the second column of the table, not the only source.

## First three capsules of a normal run

Input above: hold A, press Start at 180, move up 54 frames to y=42. No pokes.
All three capsules come from 4-member `$80` formations at y=42:

| # | last-member release (`$03AC` 3→1) | fold (kind 2→1) | capsule at fold | pickup (`$42`) |
| --- | --- | --- | --- | --- |
| 1 | f433 slot 6 `$03B2` 03→01 | f452 slot 6 | (153, 42) | f589, 0→1 |
| 2 | f709 slot 8 `$03B4` 03→01 | f728 slot 8 | (113, 42) | f785, 1→2 |
| 3 | f966 slot 8 `$03B4` 03→01 | f985 slot 8 | (111, 42) | f1038, 2→3 |

Before destruction the slot displays kind `$85` (low kind `$05` plus the `$80`
initialised bit). It must have been destroyed by a shot rather than despawned
off-screen: an off-screen despawn does not call `$BE9F`, so it cannot reduce the
`$0048` class counter or turn `$03AC` into the 1 seen at the release. `$8950`
then advances `$42` when the capsule touches the ship.

## Native port

The same script through `gradius-diff` produces the same object transitions and
the same pickups:

```
f452 slot6 kind 2 -> 1 x=153 y=42
f728 slot8 kind 2 -> 1 x=113 y=42
f985 slot8 kind 2 -> 1 x=111 y=42
 capsule= -> 1 at f589, 2 at f785, 3 at f1038
 meter=   -> 1 at f589, 2 at f785, 3 at f1038
```

The run reports 0 divergences (909 compared frames, `obj.kind`, `obj.x`,
`obj.y`, `obj.substate`, `score`, `meter` all 100%). Capsule generation in
`stage.rs`/`game.rs` is therefore correct: the 4-member class counter is
decremented on every kill, the last member folds to kind 1/subtype 6, and
`$C1A9` advances `$42`.

## The missing rule: B is held, not edge-triggered

`$8979` is entered every frame during play:

```
$8979  AD 00 01  LDA $0100
$897C  C9 01     CMP #$01
$897E  D0 08     BNE $8988       ; not alive -> RTS
$8980  A6 18     LDX $18
$8982  B5 07     LDA $07,X
$8984  29 40     AND #$40        ; held B
$8986  D0 01     BNE $8989       ; dispatch $42
$8988  60        RTS
```

`game.rs` instead derives `power_pressed = control.power && !prev_power` and
calls `activate()` only on that rising edge. A player who holds B never
activates anything: `$42` is never consumed.

Reproduction, same run plus B held from f400 (after the title screen):

```sh
printf '0:a:1400\n180:start\n300:up:54\n400:b:1000\n' > /tmp/holdb.txt
cargo run -q -p gradius-diff -- --rom "$ROM" --pack assets/gradius.pak \
  --frames 1200 --script /tmp/holdb.txt --keep-alive --native-keep-alive \
  --oracle-out /tmp/oraclehb.txt --native-out /tmp/nativehb.txt --report
```

```
first divergence: frame 589 field speed (oracle 1, native 0)
oracle:  speed=1 after f621, meter=0 throughout (activation consumed $42)
native:  speed=0, meter=1/2/3 after f589/785/1038 (capsules collected, never used)
```

Frame 589 is exactly the first capsule pickup. The cartridge sees held B and
`$42 == 1` on that frame and applies SPEED UP immediately; the port waits for a
new B press, so the meter just climbs.

## Fix plan (`game.rs`, not edited here)

1. Replace the edge with the cartridge condition: `control.power` (held) and
   the port's `$0100 == 1` gate (the normal-playing path, e.g.
   `self.phase == Phase::Normal` / `!self.player.dying`; the activation block
   currently runs in every state). Keep the block after `collide()` so a
   capsule collected on frame N is consumed on frame N while B is held.
2. Delete `power_pressed` and the now-unused `prev_power` latch, and the comment
   that claims edge behaviour is intended.
3. Keep `PowerMeter::activate` unchanged: it clears `$42` on success and leaves
   it on a no-op, so held B does not repeat an item without a new `$42`.
4. Regression test (core test, fails before the fix): hold B through a capsule
   pickup and assert the pickup frame applies slot 1 (`selection() == 0`,
   `speed_level() == 1`); with B released the selection stays 1.
5. Re-run the `holdb.txt` diff; the expected result is no divergence at f589
   and speed matching the oracle at 1/2/3.

## Notes

* The 1-in-16 fold that writes subtype 7 is authentic (`$47` counter) and is
  not a missing capsule: that fold is not a pickup by design.
* `docs/analysis/powerups.md` already documents the fold and the `$42` meter;
  this note adds the spawn-class evidence and the port delta.
