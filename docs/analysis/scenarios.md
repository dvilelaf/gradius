# Real-input scenario matrix

`gradius-scenarios` drives the cartridge oracle and the native port from reset
with the same real button script, compares state and the object table every
frame (plus sampled pixels), and checks the oracle-side invariant for the user
flow. It complements `gradius-diff`: that harness measures a recorded window,
this one proves a *user action* still does what the original does.

```sh
ROM="$GRADIUS_ROM"
cargo build --release -p gradius-diff --bins
./target/release/gradius-scenarios --rom "$ROM" --pack assets/gradius.pak \
    --out /tmp/opencode/gradius-scenarios

# one scenario, or the table
./target/release/gradius-scenarios --rom "$ROM" --only boss-gate
./target/release/gradius-scenarios --list
```

The runner prints one PASS/FAIL row per scenario with the pixel match, the
check count and the first state divergence. It exits 1 when a **clean** scenario
diverges or any **hard oracle check** fails. A row may also be marked **known**,
a documented mismatch that may diverge but still has to pass its oracle checks,
so a window that silently skips the flow cannot pass with nothing to compare.
A known row also carries a divergence budget (the accepted first frame and
total); diverging earlier or in more fields fails the gate. The matrix has 13
rows, all clean. `scripts/release-check.sh` runs the matrix as the `scenarios`
gate.

## Matrix

| scenario | frames | window | expect | what it proves |
| --- | ---: | --- | --- | --- |
| `menu-select-1p` | 520 | 170-320 | clean | Select at 180 toggles `$0F` 0 -> 1 and `$0320` `$86` -> `$96`; Up/Down at 200/220 change neither; Select at 260 returns to 1P; Start at 320 locks `$03 = $40` |
| `menu-select-2p` | 600 | 170-320 | clean | same Select edge with the cursor at 150 on both sides, Start locks `$03 = $70`; the window stops at the lock, and `p2-handoff` covers the 2P relay beyond it |
| `menu-start-1p` | 600 | Playing | clean | no Select at all: `$0F` stays 0, Start at 180 locks `$03 = $40`; state and pixels are byte-exact from reset |
| `power-collect-hold` | 1200 | 292-1199 | clean | B held from 400 spends all three stage-1 capsules: `$40` reaches 3, `$42` stays 0 |
| `power-real-speed-repeat` | 1800 | 292-1799 | clean | five real capsules at 589/785/1038/1361/1611, one B tap each: five SPEED buys, `$40 = 5` |
| `power-real-slot2` | 1000 | 292-999 | clean | second capsule raises `$42 = 2`, B at 785 buys MISSILE (`$41 = 1`) and clears the meter |
| `power-real-slot3` | 1200 | 292-1199 | clean | third capsule, B at 1038 buys DOUBLE (`$44 = 2`) |
| `power-real-slot4` | 1500 | 292-1499 | clean | fourth capsule, B at 1361 buys LASER (`$44 = 1`) |
| `power-real-slot5` | 1750 | 292-1749 | clean | fifth capsule, B at 1611 buys OPTION (`$45 >= 1`) |
| `power-real-missile-noop` | 1500 | 292-1499 | clean | B at 785 buys MISSILE; a second `$42 = 2` at 1361 and B at 1361 hit the `$89B4` no-op, so `$42` stays 2 and no second missile is granted |
| `eruption-6400` | 7300 | 6400-7299 | clean | stage-1 `$C540` eruption: kind `$8A` objects, the scroll hold at `0:12:128` over 6441-7210, `$5F` counts the eruption |
| `boss-gate` | 9200 | 7500-9199 | clean | Big Core spawns at 7723, first hit at 8066, clear `+1000` at 9087; `$39`/`$5F` gate the volcano |
| `p2-handoff` | 1300 | Playing | clean | Select 180 + Start 220 lock `$03 = $70`; P2 Start at 900 is inert (`$06`, `$18` still 0); P1 dies naturally at 1194 (`$18` 0 -> 1, lives 2/3); P2 reaches normal play at 1221 and its Right from 1230 moves the ship. The port runs the same 2P relay, so state and OAM match every frame from reset |

The two menu rows are clean gates, and so is `p2-handoff`: the port moves the
Select cursor like the ROM, so `$0320` is 150 on both sides after the first
Select, and it runs the real 2P relay past the Start lock. `menu-select-2p`
still stops its window at the lock and does not need to compare the stage it
enters, because `p2-handoff` is the row that covers play after it.

`p2-handoff` was the last known row. It pinned the measured relay drift as a
budget — first divergence not before frame 1233, at most 68 divergences — while
the port was single-player and kept both ships at x=80 after P2's rebuild. The
2P runtime (`TwoPlayer` in `Game`: `$18`, the per-slot lives and score banks,
and the second pad; [`twoplayer.md`](twoplayer.md) §8) closed that: the port
follows the switch, the run has 0 divergences from reset, the budget dropped out
and the row is now `Expected::Clean`. Its oracle checks still prove the flow:
`$03 = $70`, P2's Start inert while P1 plays, `$18` 0 -> 1 at 1194 with lives
2/3, P2 normal play at 1221 and the ship moving under P2's Right.

The effect-heavy eruption/boss
pixel windows differ in presentation while state and OAM match, so their
percentages are reported but not gated. Each run reports the measured
percentage, so a regression is visible rather than hidden.

## Control semantics

Measured through the oracle; the scripts are the record of the input.

### Title menu (`$8212`)

* Select and Start are edge-detected from `$05`; only the title (`$00 = 1`)
  acts on them.
* **Select** (`$05 & $20`) flips `$0F` 0/1, moves the menu ship `$0320`
  between `$86` (1P, pixels 134) and `$96` (2P, pixels 150), and leaves the
  selection alone otherwise.
* **Up/Down do nothing** on the title. They are in the 1P script to prove it.
* **Start** (`$05 & $10`) locks the mode byte `$03` from `$824C[$0F]` (`$40`
  1P, `$70` 2P), sets `$00 = 3` for the 80-frame countdown, then enters the
  stage. `$82CD` initialises both banks: `$20/$21 = 3`, `$0A = $07` for 2P
  (`$01` for 1P), `$18 = 0`, both scores zero. The port starts the same stage
  for both choices and follows the 2P relay (`$18`) when P1's ship dies.

### Two-player game over (`$1B = $C0`)

* A player is out when their own lives counter wraps to `$FF`; the alive bit
  (`$0A` bit 0/1) clears and `$18` names the player who is out. Game over
  starts only when both bits are clear.
* The banner is text id `$1C` at PPU `$2384`, in the power-meter row: five
  blanks, `31 66`/`32 66` ("1P"/"2P") and "GAME OVER". `$980B` patches the
  hard-coded `31` tile to `32` for P2, and the port draws the same row.
* `p2-handoff` proves the relay before this point: `$18` 0 -> 1 at 1194, lives
  2/3, P2 normal play at 1221, with 0 state divergences from reset.

### Power meter (`$42`)

* A capsule pickup advances `$42` by one (up to the six-slot meter); B
  activates the selected slot and clears `$42`.
* A tap and a hold are equivalent: the ROM consumes the selection on the
  pressed edge. `power-collect-hold` holds B and gets one buy per capsule;
  the other power scenarios tap B for two frames on the same pickup frames.
* Slot 1 is `INC $40` (`$89A6`), so repeated buys raise the speed level; the
  movement delta is `($40 + 2) << 7` clamped at level 14 (`$A000`). Five buys
  measure `$40 = 5` and the level-14 clamp is covered by `tests/speed_max.rs`.
* The selection survives a no-op: MISSILE with `$41 != 0` (`$89B4`) leaves
  `$42 = 2`, which is what `power-real-missile-noop` asserts.

### Stage 1 eruption and boss

* `$5F` counts the eruption; the background scroll holds at `0:12:128` while
  the wave runs (6441-7210), which `eruption-6400` pins as one distinct value.
* Big Core is object kind `$98`: spawn 7723, first damage 8066/8067, clear
  `+1000` at 9087. `boss-gate` requires the spawn and score event, so a run
  that skips the fight fails even though the two windows are report-only.

## Evidence

Per scenario, under `--out`:

| file | contents |
| --- | --- |
| `checks.txt` | PASS/FAIL line per oracle check and notes |
| `state.json` | per-field drift report (same shape as `gradius-diff --json`) |
| `pixels.json` | sampled pixel window, per-region and per-state summaries |
| `oracle.txt`, `native.txt` | the full state line per frame, reusable as a fixture |
| `oracle-NNNN.png`, `native-NNNN.png` | screenshot pairs on the scenario's landmark frames |

`summary.json` at the top level collects the verdict, pixel percentage and
first divergence for the whole matrix. CI uploads `$OUT` with the release logs
when the release gate fails.
