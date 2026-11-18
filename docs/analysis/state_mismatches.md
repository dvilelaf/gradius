# Title/attract state-only drift: 22 frames

The unassisted default 12,000-frame run has 0 field divergences and 0 object
count mismatches, but 22 frames whose per-frame `state=` labels differ. Every
one of them is after the run's game over, in the title/attract loop; the boss
run, which never dies, has 0. This note names each transition frame, the ROM
rule the port does not follow, and the smallest fix in `game.rs`.

Measured 2026-11-09 on `agent/statemismatch` (`786793d`), release build in
`/tmp/opencode/target-statemismatch`, ROM `Gradius (E).nes`. The cartridge is
read in place; no cartridge bytes are stored here.

## Reproduce

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-statemismatch
export GRADIUS_ROM="$GRADIUS_ROM"
export OUT=/tmp/opencode/sm
cargo build --release -p gradius-diff -p xtask

gradius-diff --rom "$GRADIUS_ROM" --pack assets/gradius.pak --frames 12000 \
    --oracle-out "$OUT/default.oracle" --native-out "$OUT/default.native"

cargo xtask state-drift "$OUT/default.oracle" "$OUT/default.native"
```

`state-drift` (added with this note) reads only `frame=` and `state=` and prints
one line per contiguous run:

```text
4806-4808 (3): oracle=Other(1) native=Title
5063-5065 (3): oracle=Other(2) native=Other(1)
8304-8305 (2): oracle=Title native=Other(2)
8434-8438 (5): oracle=Other(1) native=Title
8691-8695 (5): oracle=Other(2) native=Other(1)
11932-11935 (4): oracle=Title native=Other(2)
22 state-mismatch frame(s) in 6 run(s)
```

`gradius-diff`'s default script is Start at 180/240 then 60-frame blocks of
fire plus up/right/left from 300. `ramdump` needs the same presses, so build
them once when reading `$00`/`$01`/`$12` around a boundary:

```sh
args=()
for at in $(seq 300 60 11940); do
    case $(( ((at - 300) / 60) % 3 )) in
        0) dir=up ;; 1) dir=right ;; 2) dir=left ;;
    esac
    args+=(--press "$at:a,$dir:60")
done
cargo xtask ramdump "$GRADIUS_ROM" --frame 4806 --range 0000:0014 \
    --press 180:start:6 --press 240:start:6 "${args[@]}"
```

## The attract cycle

Transitions are dump frame numbers (the line after that frame); the oracle and
port agree on the first title entry at 4676 and disagree on everything later.
The first row is the shared entry into the cycle, not one of the 22 frames.

| stage | oracle transition | port transition | mismatch frames |
| --- | --- | --- | --- |
| game over -> title | `Title` 4676 | `Title` 4676 | - |
| title -> attract | `Other(1)` 4806 | `Other(1)` 4809 | 4806-4808 (3) |
| attract -> demo | `Other(2)` 5063 | `Other(2)` 5066 | 5063-5065 (3) |
| demo -> title | `Title` 8304 | `Title` 8306 | 8304-8305 (2) |
| title -> attract | `Other(1)` 8434 | `Other(1)` 8439 | 8434-8438 (5) |
| attract -> demo | `Other(2)` 8691 | `Other(2)` 8696 | 8691-8695 (5) |
| demo -> title | `Title` 11932 | `Title` 11936 | 11932-11935 (4) |

The oracle's windows are:

| window | frames | length |
| --- | --- | ---: |
| `Title` | 4676-4805 | 130 |
| `Other(1)` | 4806-5062 | 257 |
| `Other(2)` | 5063-8303 | 3241 |
| `Title` | 8304-8433 | 130 |
| `Other(1)` | 8434-8690 | 257 |
| `Other(2)` | 8691-11931 | 3241 |
| `Title` | 11932-... | 130+ |

So the re-entry title is consistently 130 frames, while the port holds every
title for its boot constant `TITLE_ATTRACT_ENTRY = 133`. The odd three-frame
offset makes the later runs 5/5/4 rather than 3/3/2: each title adds three
frames, and the demo cycle is one frame shorter relative to its entry (next
section).

## State bytes at the transitions

`$00` is the state dispatcher, `$01` its substep/index, `$12` the title
roll-in X counter. Measured with `ramdump` (column `$12` is at `$0012`):

| frame | `$00` | `$01` | `$12` | event |
| ---: | ---: | ---: | ---: | --- |
| 4675 | 05 | 00 | F6 | last play frame |
| 4676 | 00 | 00 | 00 | game over writes `$00 = 0` (`$9749`) |
| 4677 | 00 | 00 | 00 | no title handler effect yet |
| 4678 | 00 | 01 | FE | title setup (`$80DA`/`$824E`) |
| 4805 | 00 | 01 | 00 | roll-in reaches zero |
| 4806 | 01 | 00 | 00 | `$817E` `INC $00` -> attract |
| 5062 | 01 | 00 | 00 | last attract frame |
| 5063 | 02 | 00 | 00 | `$8193`/`$817E` -> demo |
| 8303 | 02 | 01 | 43 | last demo frame (demo X in `$12`) |
| 8304 | 00 | 00 | 43 | demo end `$8129`/`$812D`/`$8187` |
| 8305 | 00 | 00 | 00 | |
| 8306 | 00 | 01 | FE | title setup |
| 8433 | 00 | 01 | 00 | roll-in reaches zero |
| 8434 | 01 | 00 | 00 | -> attract |
| 8691 | 02 | 00 | 00 | -> demo |
| 11931 | 02 | 01 | 43 | last demo frame |
| 11932 | 00 | 00 | 43 | -> title |
| 11933 | 00 | 00 | 00 | |
| 11934 | 00 | 01 | FE | title setup |

The setup lands two frames after `$00 = 0` every time (4676->4678,
8304->8306, 11932->11934); the roll-in then takes 128 frames to the `INC $00`.

## ROM rule the port misses

State 0 is dispatched at `$80B6`/`$83E9` to `$80DA`:

* First handler frame (`$01 == 0`): `$8831` draws the title, `$824E` resets
  sound/PPU state, then `$80F0` writes `$12 = $FE` and `$80F4` sets `$01 = 1`.
* Later frames (`$80F7`): `LDA $12; BEQ $817E`; otherwise `DEC $12` twice and
  return. `$817E` is `INC $00`, i.e. title -> attract.

The title-to-attract handoff is therefore the **`$12` roll-in reaching zero**,
not a fixed frame count from `$00 = 0`. `$FE` decremented by two reaches zero
after 127 frames; the zero test consumes one more, so the transition is 128
frames after the setup. On re-entry the setup is two frames after `$00 = 0`,
giving a 130-frame title. At boot the reset warm-up delays the first handler
frame to 5 (`$12 = $FE` is written there), which is why the boot title looks
like 133 frames. The port copied the boot number for every entry.

The rest of the loop already matches and must not be touched:

* State 1 (`$810E`): `$8299` places the demo ship, `$8271` loads
  `$4C/$4D = 0/1` (256), `$8193` decrements. Attract is 257 frames on both
  sides.
* State 2 (`$8119`): `$82BF` resets on the first frame; `$9640` then runs
  `$9C67` (the `$9CB1` input table) and the stage handler. `$FF` in a tick byte
  sets `$0B`, `$8129` clears `$01`, and `$8187` writes `$00 = 0`.

### Why the demo run is one frame shorter

The demo itself is not off. `demo_control` gates the table on
`self.frame % 2 == 0` (`$02` bit 0, `$9C79`). With the three-frame title
delay, the port enters `Other(2)` at 5066 instead of 5063; because three is
odd, its first `Normal` frame is 5095 (odd), so the first table event fires on
5096. The oracle's `$1B = $80` arrives on the even frame 5092 and its first
event is 5094. Both sides then run the same table: 1,606 event slots, ending
at 8306 and 8304 respectively. The one-frame "shortfall" is the odd title
offset changing which absolute frame satisfies the `$02` parity gate; it
disappears when the title entry frame matches. Do not change
`DEMO_TRANSITION_FRAMES` (256) or `DEMO_INPUT`.

## Gameplay relevance

All 22 frames are outside the compared window: `gradius-diff` only compares
frames where both sides are `Playing`, and the default run stops being
comparable at 4675. The run reports 0 field divergences and 0 object-count
mismatches, and the boss run (which never dies) has 0 state mismatches over
12,000 frames. None of these frames changes the score, the simulation or the
objects.

They are presentation/timing only, but not entirely invisible:

* the title holds three frames too long and the whole attract cycle starts
  three frames late;
* a Start edge inside that three-frame window takes a different path (the port
  is still on `Title` and calls `begin_stage(false)`, while the ROM is in
  state 1 and goes through the `$00 = 3` countdown);
* `release-check.sh` WARNs on the non-zero `state_mismatch` count, so the
  default run is not reported clean.

## Fix plan for `game.rs`

The smallest change that keeps the tests and the tuned boot roll-in intact is
a per-entry attract threshold:

1. Add next to `TITLE_ATTRACT_ENTRY`:

   ```rust
   /// Re-entry title: `$12 = $FE` two frames after `$00 = 0`, then 127
   /// `DEC $12` pairs and the zero-test frame. The boot title keeps 133
   /// because the reset warm-up delays the setup to frame 5.
   const TITLE_ATTRACT_REENTRY: u16 = 130;
   ```

2. Add `title_attract_at: u16` to `Game`, initialised to
   `TITLE_ATTRACT_ENTRY` in `Game::new`.
3. Set `self.title_attract_at = TITLE_ATTRACT_REENTRY` in the `title_pending`
   branch (the game-over handoff, around line 952) and in `finish_demo`
   (around line 1293). Both are the `$8187`/`$9749` re-entry paths.
4. Change the comparison around line 980 to
   `if self.title_timer == self.title_attract_at`.
5. Leave `title_timer`, its use as the render/roll-in clock, and
   `TITLE_ATTRACT_ENTRY` unchanged, so the title render hash and the boot
   `Title` 1-132 / `Other(1)` 133 sequence stay as tested.

Expected after the change (same default script):

| transition | frame |
| --- | ---: |
| `Title` -> `Other(1)` | 4806 |
| `Other(1)` -> `Other(2)` | 5063 |
| `Other(2)` -> `Title` | 8304 |
| `Title` -> `Other(1)` | 8434 |
| `Other(1)` -> `Other(2)` | 8691 |
| `Other(2)` -> `Title` | 11932 |

and `cargo xtask state-drift` prints `0 state-mismatch frame(s) in 0 run(s)`.
Because the corrected entry frame is odd like the oracle's, the demo parity
lines up and the demo windows become 3241 frames without any demo change.

A more ROM-faithful alternative is to replace the timer with the `$12` counter
(2-frame warm-up, `$12 = $FE`, two `DEC`s per frame, transition on zero) and
add three warm-up frames only to the boot path. That touches the title
rendering clock and the render hash; it is not required to close the 22 frames.

### Tests

* `gradius-core`: drive a `Game` through the default script's game over and
  assert `dump_state` reports `Title` at 4676, `4805`, and `Other(1)` at 4806;
  then `Other(2)` at 5063 and `Title` at 8304. A shorter synthetic version can
  set `title_pending`/`finish_demo` and step 130 frames.
* `xtask`: the `state_drift` module has synthetic tests for contiguous runs,
  equal dumps and a frame-count mismatch; run
  `cargo test -p xtask state_drift`.
