# Harness gaps: are the gates blind to collision slot 0?

Measured 2026-11-12 on `agent/slot0aude` with the cartridge
(`GRADIUS_ROM=$GRADIUS_ROM`), reproducer
`crates/gradius-diff/tests/slot0_probe.rs`:

```sh
CARGO_TARGET_DIR=/tmp/opencode/target-slot0aud CARGO_BUILD_JOBS=2 \
GRADIUS_ROM="$ROM" cargo test --release -p gradius-diff --test slot0_probe -- --nocapture
```

The probe replays the oracle with each window's own recording settings (stage
poke, input script, `--keep-alive`) and reads the object table straight out of
RAM: `$012C + slot` (frame counter, `objects()`' liveness test), `$030C + slot`
(kind), `$032C`/`$036C + slot` (Y/X). No fixture is involved, because the
fixtures are themselves filtered.

## Answer

**Yes.** Slot 0 is a real enemy slot, it is live in every window the harness
gates, and both dumps skip it:

* the oracle, `gradius-diff/src/dump.rs::oracle_snapshot` filters
  `(1..=9).contains(&object.slot)`;
* the port, `gradius-core/src/game.rs::dump_state` loops `for slot in 1..LIVE_SLOTS`.

Both carry the same wrong comment ("slot 0 is the ship in the original"). The
player block (`$0320`/`$0360` index 0) is a *different* array, one 12-byte page
before the object table: `$A521` clears an object at `$A8 + $0C`, `$A3DE`
allocates "slots 9 down to 0" (`LDX #$09`, `docs/analysis/enemy_ai.md:73`), and
the table's kind/substate/position bytes are `$030C`/`$03BC`/`$032C`/`$036C + slot`
(`docs/analysis/ram_map.md`). The probe sees enemy kinds and enemy motion there,
not a ship.

## Measurements

Slot-0 objects per gated window (`n` = frames in the window):

| window | n | frames with a live slot-0 object | kinds (`$030C`, decimal/hex) | runs |
| --- | --- | --- | --- | --- |
| default 297-12000 | 11704 | 676 (5.8%) | 136/`$88` ×263, 137/`$89` ×333, 2/`$02` ×80 | 1136-1371, 3207-3231, 3736-3754, 4265-4446, 6381-6387, 6392-6411, 7644-7696, 7701-7720, 10009-10028, 10033-10052, 11294-11347, 11352-11371 |
| boss 19429-23000 | 3572 | 432 (12.1%) | 156/`$9C` ×206, 142/`$8E` ×206, 2/`$02` ×20 | 19948-20153, 20158-20177, 20189-20394 |
| stage 5, 297-8573 | 8277 | 1243 (15.0%) | 136/`$88` ×708, 141/`$8D` ×206, 157/`$9D` ×229, 2/`$02` ×100 | 703-908, 1008-1243, 1325-1553, 1697-1932, 2032-2267, 3056-3105, 3558-3607 |
| stage 7, 297-6050 | 5754 | 2292 (39.8%) | 132/`$84` ×732, 145/`$91` ×464, 147/`$93` ×514, 139/`$8B` ×352, 11/`$0B` ×124, 140/`$8C` ×100, 4/`$04` ×6 | 815-937, 1516-1638, 1964-2075, 2988-3102, 3116-3227, 3436-3558, 3564-3686, 3769-4232, 4335-4848, 4870-4969, 5039-5175, 5424-5546, 5548-5670 |
| ending 7025-8290 | 1266 | 106 (8.4%) | 138/`$8A` ×66, 2/`$02` ×40 | 7089-7097, 7102-7121, 7137-7143, 7148-7167, 7173-7222 |
| **total** | **30573** | **4749 (15.5%)** | | |

`0x02` runs are the `$BE8D` debris state (the dump already documents that
`$BE8D` turns a killed object into kind-2 debris and clears `$012C`); the other
kinds are ordinary enemies, several of them multi-second waves.

## The ghost slot 20

`Emulator::objects()` scans `0..32`, so out-of-range slots whose frame counter
byte is non-zero are reported too. That byte set is *not* empty in any gated
window: slots 10-19 alias the collision/hitbox arrays (`$0136`-`$013F`) and slot
20 aliases the player block — `$030C + 20 = $0320`, so its "kind" is literally
the ship's Y integer (96 in every sample), with X/Y fraction `0`.

Ghost slot 20 is live in every gated window: default 9469 frames (297-11933),
stage 5 7242, boss 2916, ending 1108, stage 7 710. Slots 21/22 also fire in the
default run (6402 frames each, from 5093).

Today it cannot mask anything, because `(1..=9)` excludes it. But it *would*
have masked a widened filter: switching the oracle to `0..=31` to "recover slot
0" would inject a phantom object whose third field tracks the player's Y, on
~30-40% of frames, and `object_count_delta` would swing by ±1 constantly,
burying a real -1. The safe range is `0..=9`, never `0..=31`.

## Impact

* The gate compares 9 of the 10 allocated slots. On 15.5% of the gated frames
  the ROM has a live object in the one slot nobody dumps.
* A gate can pass with **one object fewer**: if the port fails to spawn, kills
  early/late, or puts a different kind or position in slot 0, the object list
  lengths agree after the filter and no field is compared. The stage-5 window
  (0 divergences, 297-8573) contains 1243 such frames, so those "exact" gates
  are exact over 9 slots only.
* It also removes a consistency tripwire: the port allocates with
  `free_slot()`/`spawn_object()` "highest free slot first", the ROM with `$A3DE`
  "slots 9 down to 0", so slot 0 is only used when 1-9 are full. A port that
  fills slots in a different order would show up as slot-0-vs-slot-N drift,
  which the current dumps cannot express.

## Correction (not applied here: `crates/gradius-core/**` and `crates/gradius-diff/src/**` are outside this card)

1. `crates/gradius-core/src/game.rs::dump_state`: `for slot in 0..LIVE_SLOTS`,
   and delete the "slot 0 is the ship" comment (the ship is `$0320`/`$0360`
   index 0, one page before `$030C`/`$032C`/`$036C`).
2. `crates/gradius-diff/src/dump.rs::oracle_snapshot`: `(0..=9)` instead of
   `(1..=9)`; keep the `slot > 9` cap and rewrite the comment. Slot 0 needs no
   special case: the ship is already compared through `px/py/pf/pyf`.
3. Re-record the fixtures. Cost is small in machine time (the five windows are
   ~18 s of oracle in release plus one `--emit-fixture` run each, all
   `--keep-alive` settings unchanged) but every object-carrying fixture changes
   its `objects=` field on the frames above: `stage5-297-8573`,
   `stage5-8574-12003`, `stage7-297-6050`, `stage3-19429-23000`,
   `stage2-9601-12000`, `stage2-12161-15000`, `stage4-297-1797`,
   `stage6-297-1500`, `ending-7015-8290` and the stage-1/attract long runs.
   Gates that pin `Report` totals (`object_slot_pairs`, divergence counts,
   first-divergence frame) need re-baselining, and any real slot-0 mismatch the
   port has will surface as new divergences. Budget: half a day, mostly
   re-baselining, plus whatever the first full run uncovers.
4. Optional follow-up: teach the comparator to report a slot-count delta per
   frame (`objects.len()` vs the ROM's `$A8`-slots) so a slot-range regression
   fails loudly instead of silently shrinking the compared table.
