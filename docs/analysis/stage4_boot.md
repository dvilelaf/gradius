# Stage-4 boot slice: entry state, spawner, background, music

Measured 2026-11-10 on `agent/s4analysis` (from `4c16595`) with the release
oracle harness and `gradius-diff --stage 4`; §7 re-measured 2026-11-11 after
the boot wiring landed on `main` (`b3a9332`). ROM
`Gradius (E).nes`, pack built with `xtask extract`. This note only covers what
the port must model on stage entry and during the first 1500 frames; the clear
path is `stage4_plan.md` and the stage-3 handoff is `stage3_clear.md`.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-s4analysis
cargo build --release -p xtask -p gradius-diff

# natural `$39 = 0` handoff (stage 3 -> 4), 29257 is the entry frame
./target/release/xtask transition "$ROM" --script scripts/boss-script.txt \
  --from 29245 --to 29330 --keep-alive --out /tmp/opencode/s4_nat.txt

# first 1500 stage-4 frames: object kinds, slots and the music boundary
./target/release/xtask transition "$ROM" --script scripts/boss-script.txt \
  --from 29257 --to 30760 --keep-alive --out /tmp/opencode/s4_window.txt
./target/release/xtask transition "$ROM" --script scripts/boss-script.txt \
  --from 30760 --to 31400 --keep-alive --out /tmp/opencode/s4_music.txt

# zero page at the entry frame (boss-script as --press lines)
./target/release/xtask ramdump "$ROM" --frame 29257 --range 0000:0060 \
  --keep-alive $(sed 's/^/--press /' <(grep -v '^#' scripts/boss-script.txt)) \
  --out /tmp/opencode/s4_ram29257.txt

# port diff: `--stage 4` pokes `$26`, so its window is relative to the poked
# entry (oracle 297 = native 1), not to 29257
./target/release/gradius-diff --rom "$ROM" --pack /tmp/opencode/s4.pak \
  --stage 4 --frames 1900 --from 297 --to 1797 --report
```

## 1. Entry frame and state

Stage 4 (`$19 = 3`) is entered on the normal `$39 = 0` path at **frame 29257**
(`$3F = $0E` at 29255, `$1B = $90` at 29256, `$96C4` at 29257). The port's
514-frame countdown already lands on this frame.

| field | value at 29257 | source |
| --- | --- | --- |
| `$19` | `03` | ledger + RAM |
| `$1B` | `80` (normal play) | ledger |
| `$1C` | `93` (shared intro, **not** `$5D`) | ledger |
| `$39` / `$3A` | `00` / `00` | ledger + RAM |
| `$17` rank | `02` | RAM |
| `$1A` | `00` | RAM |
| `$02` / `$03` | `44` / `40` | RAM |
| `$3E:$3F:$3D` | `01:00:00` (`$3F = 0`, `$3E` carried) | ledger + RAM |
| `$0C` / `$0D` / `$0E` | `00` / `00` / `00` | RAM |
| `$55` / `$5F` | `01` / `00` | RAM |
| `$35` fire cooldown | `14` | RAM |
| ship `$0360,$0320` | `22,73` (`$0100 = 01`) | ledger |
| stage thresholds | arm `$3F = $0C`, core spawn `$0D`, end `$0E` | `stage4_plan.md` |

The `$96C4` handoff does **not** run stage init (`$9B38`): it zeroes `$3F`,
`$39`, `$3A` and `$50-$70`, uploads the HUD, and leaves `$3E`, the ship,
`$0100` and `$0300` untouched. `$0C/$0D/$0E` are the zero-page screen-blank
countdown, VRAM queue index and the unused byte — all quiet at entry; the
"`0C/0D/0E`" that matter for stage 4 are the `$3F` thresholds.

## 2. Spawner, first 1500 frames

70 spawns from 29258 to 30757, from `s4_window.txt` (`$030C` kinds; the high
bit is the `$A3AB` "initialised" bit, so `133 = $05|$80`). X/Y are the spawn
values from the lockstep oracle dump (poked entry 294 = natural 29258) and
match the natural ledger's slot/kind sequence for the whole window.

| frame | kind | slot | X | Y | group |
| ---: | ---: | ---: | ---: | ---: | --- |
| 29258 | 85 | 9 | 240 | 42 | n/d |
| 29269 | 85 | 8 | 240 | 42 | n/d |
| 29280 | 85 | 7 | 240 | 42 | n/d |
| 29291 | 85 | 6 | 240 | 42 | n/d |
| 29302 | 85 | 5 | 240 | 42 | n/d |
| 29335 | 02 | 9 | 92 | 42 | n/d |
| 29389 | 85 | 9 | 240 | 160 | n/d |
| 29400 | 85 | 8 | 240 | 160 | n/d |
| 29411 | 85 | 7 | 240 | 160 | n/d |
| 29422 | 85 | 6 | 240 | 160 | n/d |
| 29433 | 85 | 5 | 240 | 160 | n/d |
| 29490 | 85 | 9 | 240 | 42 | n/d |
| 29501 | 85 | 8 | 240 | 42 | n/d |
| 29512 | 85 | 7 | 240 | 42 | n/d |
| 29523 | 85 | 6 | 240 | 42 | n/d |
| 29534 | 85 | 5 | 240 | 42 | n/d |
| 29621 | 85 | 4 | 240 | 160 | n/d |
| 29632 | 85 | 9 | 240 | 160 | n/d |
| 29643 | 85 | 8 | 240 | 160 | n/d |
| 29654 | 85 | 7 | 240 | 160 | n/d |
| 29665 | 85 | 6 | 240 | 160 | n/d |
| 29732 | 85 | 9 | 240 | 42 | n/d |
| 29743 | 85 | 8 | 240 | 42 | n/d |
| 29754 | 85 | 7 | 240 | 42 | n/d |
| 29765 | 85 | 6 | 240 | 42 | n/d |
| 29776 | 85 | 5 | 240 | 42 | n/d |
| 29838 | 02 | 8 | 116 | 72 | n/d |
| 29861 | 02 | 5 | 108 | 50 | n/d |
| 29863 | 85 | 8 | 240 | 160 | n/d |
| 29874 | 85 | 9 | 240 | 160 | n/d |
| 29885 | 85 | 5 | 240 | 160 | n/d |
| 29896 | 85 | 7 | 240 | 160 | n/d |
| 29907 | 85 | 6 | 240 | 160 | n/d |
| 29991 | 85 | 4 | 240 | 42 | n/d |
| 30002 | 85 | 3 | 240 | 42 | n/d |
| 30004 | 02 | 8 | 156 | 32 | n/d |
| 30013 | 85 | 2 | 240 | 42 | n/d |
| 30024 | 85 | 8 | 240 | 42 | n/d |
| 30035 | 85 | 1 | 240 | 42 | n/d |
| 30119 | 85 | 9 | 240 | 160 | n/d |
| 30130 | 85 | 7 | 240 | 160 | n/d |
| 30141 | 85 | 6 | 240 | 160 | n/d |
| 30152 | 85 | 8 | 240 | 160 | n/d |
| 30163 | 85 | 5 | 240 | 160 | n/d |
| 30247 | 85 | 4 | 240 | 42 | n/d |
| 30258 | 85 | 3 | 240 | 42 | n/d |
| 30269 | 85 | 2 | 240 | 42 | n/d |
| 30280 | 85 | 1 | 240 | 42 | n/d |
| 30291 | 85 | 9 | 240 | 42 | n/d |
| 30312 | 88 | 7 | 239 | 59 | n/d |
| 30315 | 88 | 6 | 239 | 111 | n/d |
| 30403 | 85 | 9 | 240 | 42 | n/d |
| 30414 | 85 | 8 | 240 | 42 | n/d |
| 30425 | 85 | 7 | 240 | 42 | n/d |
| 30436 | 85 | 6 | 240 | 42 | n/d |
| 30447 | 85 | 5 | 240 | 42 | n/d |
| 30513 | 02 | 9 | 139 | 94 | n/d |
| 30534 | 02 | 6 | 113 | 84 | n/d |
| 30534 | 85 | 9 | 240 | 160 | n/d |
| 30545 | 85 | 4 | 240 | 160 | n/d |
| 30556 | 85 | 8 | 240 | 160 | n/d |
| 30567 | 85 | 7 | 240 | 160 | n/d |
| 30578 | 85 | 6 | 240 | 160 | n/d |
| 30662 | 85 | 5 | 240 | 42 | n/d |
| 30673 | 85 | 3 | 240 | 42 | n/d |
| 30684 | 85 | 2 | 240 | 42 | n/d |
| 30686 | 02 | 4 | 156 | 32 | n/d |
| 30695 | 85 | 1 | 240 | 42 | n/d |
| 30706 | 85 | 9 | 240 | 42 | n/d |
| 30712 | 02 | 7 | 168 | 32 | n/d |

Reading: the stream at 1 px/frame emits kind `$85` (`$05|$80`, scout, handler
`$B0A9`) in five-slot waves from X 240, alternating Y 42/160, and the
multi-object records (kind `$02`, X 92-168) interleave every ~130-300 frames.
Kinds `$88` (`$08` seeker) appear at 30312 and recursive waves continue past
1500. The stage-4 raw type list resolves to kinds `$04..$13`, `$27`, `$29`
(`stage_enemies.md`), so all of these already have a `game.rs` arm.

**Gap:** the sprite group (`$0120`) is not in `dump_state`/the oracle snapshot,
so the group column is `n/d` here. The group is set by the handler after
spawn (port: `$ADDF` first frame), and the identity kind -> first group needs
an OAM-side probe per kind; the spawn frame/slot/X/Y are measured and stable.

## 3. Background

`stage_bgs::STAGE4`, 480x28 tiles (3840 px), dominant **CHR bank 0**, and the
32-byte `$3F00` page (four background and four sprite palettes) read from the
cartridge at run time.

Tiles/attributes/palette hashes `228C903AE305586D` / `9E8E0821170D5B2A` /
`90FA63C90403F3FC`; 0 tile/attribute mismatches at 0, 512, 1100, 3328 and
3500 px, and over the full 100-px sweep (`stages.md`). Display changes
measured by `xtask screens`:

| from world column | CHR bank | palette change |
| --- | --- | --- |
| 0 | 0 | first frame shows the alternate load (`0F 30 30 ...`), then the dominant palette |
| 0 | 0 | dominant stage-4 palette (above) |
| 416 (3328 px) | 2 | entries 8-15 forced to `0F`; entries 0-7 keep the dominant values |

The last 512 px are starfield. The tail switch is a renderer rule — `STAGE4`
stores only the dominant bank/palette. Stage 4 reuses stage 1's `$D772` tile
stream, with its own `$CF56`/`$D188` height/base and `$9FD6`/`$D772` tile
pointer pair (`stage4_plan.md`).

## 4. Music and SFX

| frame | event |
| --- | --- |
| 29257 | entry; `$1C` stays `$93` (shared intro). The handoff does not change music. |
| 31304 | `$1C` `93 -> 5D` (`Music::Stage4`) when `$3F` reaches 4 (`$8354[3]`, scroll `00:04:80`, `$3E` increment frame) |

No SFX is triggered by the stage-4 stream in the first 1500 frames. The first
located stage-4 SFX is sound `$0F` from the `$C5A7` hazard on its first call
(`$69 == 0`), which only runs inside a `$82` window after the `$0C` arm — far
outside this window (`stage4_plan.md`). Boss `$A5`, clear `$AC`, game over
`$AF` are the shared cues.

## 5. Triggers

| trigger | stage-4 behaviour |
| --- | --- |
| `$39` | **never set.** No stage-4 setter exists (`$B95C` is stage 2, `$C776` stage 3, volcanoes stage 1). Entry and the whole stage stay `$39 = 0`. |
| `$3A` | stays `0`; it is only incremented by `$992E` when `$39 != 0`. The `$C424`/`$C680` bonus-wave hook is therefore inert for stage 4. |
| `$5F` | cleared at entry by `$96C4`'s `$50-$70` loop. Stage 4 has five `$0F`/`$10` volcanoes; their `$046C >= 5` death and `$5F` count are **not ported** (shared gap, not a stage-4 branch). |
| cannons `$14` | **none.** `$14` (`$CA58`) is stage 5 only (20 records); stage 4 has no command records and no `$14`. |
| Big Core | shared core: arm at `$3F = $0C` (3072 px), spawn at `$3F = $0D`, 7 hits, +1000, stage end at `$3F = $0E`, then the normal 514-frame clear to `$19 = 4`. |
| `$C5A7` hazard | not a boss: one kind-`$15` object in the `$82` window (record 15 `FE 08 60`, X `$B8`, Y `$2C`, sprite `$58`, `$03BC` 8->6), never sets `boss_spawned`. |

## 6. Quirks

Model:

1. **The handoff is not stage init.** `$96C4` keeps the ship, `$0100`,
   `$0300`, `$3E` and `$1C`; only `$3F`, `$39`, `$3A`, `$50-$70` are zeroed
   and the HUD is re-uploaded. The port must not clear objects or reset the
   ship on entering stage 4.
2. **Music changes late and on an `$3E == 0` frame.** `$1C` stays `$93` for
   2047 frames (31304 - 29257) and only then becomes `$5D`; a `$5D` at 29257
   desyncs the ledger.
3. **First-column lead.** The oracle's slot-9 object is already at X 234 on
   the first compared frame while a boot that steps `$3E` immediately spawns
   it at 240: `$9B38` runs three gameplay frames before the first `$3E` step,
   so the spawner is one column (6 px) ahead. `new_at_stage(3)` replays those
   three frames (`stage_four_boot`), which removes the old frame-297 `obj.x`
   divergence; do not "fix" the lead by moving a spawn X.
4. **`$C5A7` fires once, not every frame.** `$C40D` runs only on `$1B == $82`
   frames (`$02 & 3 == 0` inside it) or when `$3A != 0`; stage 4's `$82` is 4
   frames, so one hazard, not a stream.
5. **Volcano deaths do not touch `$39`** in stage 4; do not reuse stage 1's
   gate.

Ignore:

* stage 1's 771-frame `$82` hold and the `$39` bonus branch;
* `$3A`/`$C680` stage-3 bonus wave and `$14` cannons (stage 5);
* the `$0F`/`$10` five-hit death until the diff flags it (3 h shared add-on);
* the tail pixel rule is rendering only — it never feeds object state.

## 7. Port diff (current `main`, boot wired)

Re-measured 2026-11-11 on `main` at `ab31f9d`, after `b3a9332` wired the
stage-4 boot entry, the late `$1C` boundary and the three-frame `$9B38`
warm-up. `gradius-diff --stage 4 --from 297 --to 1797`: 1501 compared frames
from the poked entry; the native `Game` starts 5 frames after the oracle's
first Playing frame (`STAGE_BOOT_LEAD`). **`--from 29257` cannot be used with
`--stage 4`:** the poke makes the oracle enter stage 4 at 292, and a natural
run needs the whole stage-1..3 history the port cannot replay.

```sh
./target/release/gradius-diff --rom "$ROM" --pack assets/gradius.pak \
  --stage 4 --frames 1900 --from 297 --to 1797 --report
```

```
result: DIVERGES (162 divergences, down from 9131 before the warm-up)
first divergence: frame 497 field px (oracle 80, native 0)
field rates: obj.kind 100%, obj.x 100%, obj.y 100%, obj.substate 100%,
             scroll 100%, score 100%, lives 100%, meter 100%, speed 100%,
             px 94.6%, py 94.6%
objects: slot match 7167/7167 (100.0%), live-count delta max 0
```

State is exact over the whole window: object kind, slot, x, y and substate
(7167/7167), plus scroll, score, lives, meter and speed. The 162 divergences
are `px` and `py` only, on 81 frames — runs 497-523, 739-765 and 1410-1436 —
every one in the `Other(4)` state (`$1B` 1-4, the init/respawn path): the
oracle reports the placed ship at `80,96`, the port's dump reports `0,0`.
That is the shared ship-init path (`$1B` 1-4), not a stage-4 branch; the
error is bounded by the respawn placement (px max 80, py max 96). Accepted in
[KNOWN_DEVIATIONS.md](../KNOWN_DEVIATIONS.md).

The boot now runs the three gameplay frames `$9B38` executes before the first
`$3E` step (`stage_four_boot`), reproducing the ROM's 6 px first-column
spawner lead; the old frame-297 `obj.x` divergence and the scroll drift that
followed from it are gone. Nothing in this window reaches the `$5D` music
boundary (equivalent frame 31304), the `$C5A7` hazard or the core, so the
S4-01..S4-04 acceptance gates keep their own windows; this note fixes the
entry ledger and the first 1500-frame spawn list those cards are tested
against.

Not measured (gaps): sprite group per spawn (`$0120` is not in the state
dump) and the `$C5A7` hazard frame (needs a run past `$3F = $0C`, ~35400).
