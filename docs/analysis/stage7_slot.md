# Stage 7: the frame-2069 slot-1 divergence

Analysis only: no core, fixture or script change. Measured on 2026-11-11 with
the checked-in `tests/fixtures/stage7-297-6050.state`, the native replay and
the cartridge on `$GRADIUS_ROM`.

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-s7slot CARGO_BUILD_JOBS=2
cargo test -p gradius-diff --test stage7_replay stage7_replay_reports_the_first_divergence -- --nocapture
cargo run -p xtask -- disasm "$ROM" --from B379 --to B3FC --labels
cargo run -p xtask -- disasm "$ROM" --from BDF4 --to BE68 --labels
```

The stage-7 replay reports 37,917 divergences; before this analysis the first
one was frame 2069 `obj.slot` (oracle absent, native slot 1 kind `$8B`).

## The object

The object is a kind-`$0B` launcher from section 6, column 1 (`$ACE2`), record
13 at `$ACFC` = `$A0 $93` (fires at `$3F = 3` once the low byte reaches `$40`,
oracle frame 1961):

* `$93 & $3F = $13` -> `$A648` `OBJECT_DEFS[19]` = `00 0B 14 13`:
  subtype `$010C = 0`, type `$030C = $0B`, substate `$03BC = $14`,
  formation `$03EC = $13`.
* formation pair `$A5B4` = `B3 2C`: **3 members**, X `$B0` (176);
  `$A5EF` params = `02 20 78`: interval 2, Y step `$20` from `$2C`, so the
  members sit at Y `$4C/$6C/$8C` = 76/108/140.
* At frame 1961 slots 2-9 are occupied, so only the first member allocates:
  slot 1, kind `$0B`, `(176, 76)`. The other two attempts find no free slot.
* `$B379` hovers it for 32 `$04AC` frames (sprite table `$B3BC`), then
  `$B397` stores `$04CC = 1`, clears `$048C`, and `JMP $B0AE` adds `$80` to
  the kind. At frame 1992 the object is kind `$8B`, sprite `$67`.
* Active frames take `$B3A4`: sprite `$67`, `$BCAF` aims once, `$BDF4` moves
  it along the aimed vector, then `$048C = 1` (`substate 1`).

## Measured object table

`slot1:kind:x:y:substate`; `n` is the live-object count.

| frame | oracle slot 1 | native slot 1 | oracle / native n |
| ---: | --- | --- | --- |
| 1961 | `11:176:76:0` | `11:176:76:0` | 9 / 9 |
| 1992 | `139:176:76:0` | `139:176:76:0` | 9 / 9 |
| 2050 | `139:103:24:1` | `139:103:24:1` | 9 / 9 |
| 2060 | `139:91:15:1` | `139:91:15:1` | 9 / 9 |
| 2068 | `139:81:8:1` | `139:81:8:1` | 9 / 9 |
| **2069** | **absent** | **`139:79:7:1`** | **8 / 9** |
| 2070 | absent | `139:78:6:1` | 8 / 9 |
| 2080 | absent | `139:66:0:1` | 8 / 9 |
| 2090 | absent | `139:53:0:1` | 7 / 8 |
| 2095 | `11:176:140:0` | `139:47:0:1` | 9 / 9 |

Both sides spawn the same object at the same frame and position and agree on
every field through 2068; the first difference is the oracle dropping it one
frame after it reaches Y = 8.

## Cause: the missing Y test in `$BDF4`

`$BDF4` moves both axes (`$BE11`-`$BE21`), then checks the *X* integer
(`$BE24`: `CMP #$02 / BCC`, `$BE28`: `CMP #$FC / BCS`) and finally the *Y*
integer:

```text
$BE5C  C9 08     CMP  #$08
$BE5E  90 05     BCC  $BE65     ; Y < 8  -> JMP $AEF2 (despawn)
$BE60  C9 C4     CMP  #$C4
$BE62  B0 01     BCS  $BE65     ; Y >= $C4 -> despawn
$BE64  60        RTS
```

The port's `Game::move_aimed` (`crates/gradius-core/src/game.rs`) only tests X:

```rust
let x = object.x.int;
if !(0x02..0xFC).contains(&x) {
    object.despawn();
}
```

`Fixed::add` clamps Y at 0, so the launcher survives at Y = 0 and keeps
drifting left until X < 2; the native keeps slot 1 occupied until frame 2131
(63 frames too long). The oracle's last sample is `(81, 8)` at 2068; the next
`$BDF4` step would leave Y = 7, so `$BE5E` despawns it at 2069.

This is **not** an extra spawn, and not a slot reused early: spawn frame,
formation and position match. It is an object that does not die when it
should, so its slot is released 63 frames late. The oracle frees slot 1 at
2069; the next formation, record `$AD00` = `$C0 $93` (the same three-member
setup, Y 76/108/140), starts at 2089. The oracle places all three members in
slots 8, 5 and 1; the native finds only slots 8 and 5 and drops the Y = 140
member, which is the 2095 row above.

## Proposed minimal fix

Add the exact `$BDF4` Y test to `Game::move_aimed`, next to the existing X
test:

```rust
let x = object.x.int;
let y = object.y.int;
if !(0x02..0xFC).contains(&x) || !(0x08..0xC4).contains(&y) {
    object.despawn();
}
```

No change to `ai_launcher`: `$B3B3` calls `$BDF4`, and `$B4AD` (the `$B47A`
spin) shares it, so the fix belongs in the shared aimed-move helper. Do not
use `Game::bounds` here: that is `$B24B`, whose X edges are `$04/$F4`, while
`$BDF4` uses `$02/$FC`.

## Proposed test (not implemented)

* Core unit test: build a kind-`$8B` launcher at Y = 9 with `y_vel = -0x100`
  and `substate 0` (see `the_stage_two_launcher_fires_one_group_25_shot` for
  the fixture style), run `ai_launcher` once and assert it is not live; the
  same object at Y = 10 (landing on 9) must stay live. This isolates `$BDF4`.
* Integration regression in `crates/gradius-diff/tests/stage7_replay.rs`:
  replay the fixture and assert (a) frame 2068 both sides have slot 1
  `$8B` at `(81, 8)`, (b) frame 2069 the native objects list has no slot 1
  and 8 entries, and (c) frame 2095 native slot 1 is the third `$C0 $93`
  member, kind `$0B` at `(176, 140)`. It fails before the fix at 2069 and
  passes after it.

## Impact beyond 2069

The same missing Y test explains every kind-`$8B` launcher in this fixture
whose last oracle sample is Y = 8: despawn frames 2069, 2207 (two objects,
slots 8 and 5), 2232, 3199, 5130 and 5140. Each one holds its slot roughly
60 frames too long, so the fix moves both the first divergence and a large
part of the 37,917 count; expect the next first divergence to come from a
different field after it lands.
