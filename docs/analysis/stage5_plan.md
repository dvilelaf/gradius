# Stage 5 implementation plan

Executable plan for `$19 = 4` (stage 5). It starts from the branches where
stage 5's data is already decoded but the runtime is still stage-1-shaped:
the spawner indexes `SECTIONS[stage]`, `stage1_collision::terrain(4)` is
empty, `stage_bgs::STAGE5` exists, and `gradius-audio` already carries
`Music::Stage5` (`$5F`). What is missing is selection, the shared core gate,
the `$C433[4]` script, the `$14` handler and the stage-5-only branches.

## Status (2026-11-12)

The stage row and shared core gate, the `$14`/`$CA58` large object (morph and
death, `$CA51` aim with the `SBC` carry, `$AEDB` drift), the `$A4A0`/`$0600`
cannon slots, the `$CB8B` 60-frame cadence at rank 2, the five `$C64D` objects,
the `$9658` `$19 == 4` / `$5C >= 2` chain and the `$9A69`/`$C76C`/`$CB84`
per-frame cannon tick are landed. The `--stage 5` harness and
`tests/fixtures/stage5-297-8573.state` (8277 frames) cover the state window:
the replay is **0 divergences** over oracle frames 297-8573 (8277 compared
frames, every field and 23481/23481 object slots, including the 3055 cannon
kill at score `90 -> 190`), against 2155 divergences with the first at 2601
before the card. The 297-8573 pixel window is report-only: 99.0% overall,
98.8% playfield, HUD 100.0%.

The first remaining divergence is frame **8574** `obj.x` slot 9 (oracle 109,
native 111). Under the `$82` hold the ROM's `$CA58` calls `$AEDB` directly,
past the `$AED7` `$5B` test, while the port routes it through `drift_object()`,
which returns early on `scroll_locked`; the cause and the proposed
`drift_large_object` entry are in `stage5_boot.md` §9.

Not measured yet: the core approach and the clear to stage 6 (oracle spawn
10274, death 11489, stage 6 from 12003). The column-384 tail is still not
applied (`STAGES[4].quirks = 0`), and it lands past this window.

## Stage-5 facts to build against

From `cargo xtask stages "$ROM"` (the row is stable; checked 2026-11-09):

| field | stage 5 value |
| --- | --- |
| `$19` | 4 |
| spawn root `$A7CA[4]` | `$A814`, 7 columns |
| `$A65C` terrain | 0 records (`S4_TERRAIN: [Terrain; 0]`) |
| background | `stage_bgs::STAGE5`, 448x28, bank 1 |
| `$9FB6` height / `$9FC6` base | `$CF7E` / `$CF90` |
| `$9FD6` tiles / `$9FAE` attr | `$D772` / `$40` |
| music `$8344` / boundary `$8354` / mode `$834B` | `$5F` / 4 / 2 |
| script `$C433[4]` | `$C64D` |
| boss threshold `$9A30` | `$0B` (spawn at `$3F = $0C`) |
| end `$98F0` | `$0D` |
| start row `$9BC6` | `$00` |
| full-data hashes | tiles `847ED131654EA7A1`, attrs `0BB9B51B269205C5`, palette `90FA63C90403F3FC` |

Measured with `cargo xtask record-boss "$ROM" --stage 5 --frames 13000`:
spawn 10274, scroll stop 10273, first damage 10753, max damage 6/7, death
11489 (+1000), stage 6 from 12003. The late spawn is the `$C64D` script
slowing the scroll, not a different core.

Stage 5's 20 five-byte stream commands are already decoded in
`stage1_tables::SECTIONS[4]`: columns 2-6 each contain four records with
`kind $14`, `y` `80/40/80/60`, and `b2` `01/12/02/21`. The extra `$C64D`
script uses `$C674` pairs `(02,80) (00,40) (01,80) (00,C0)` with kind `$14`;
`$69 & 6` cycles those four entries.

## Current code anchors

| gap | current code |
| --- | --- |
| background hardcoded | `game.rs::render` uses `stage1_bg::STAGE1`; `draw_screen` always `screen.chr_bank` / `screen.palette` |
| boss gate hardcoded | `game.rs::normal_frame` uses `0x0C`, spawn wait `0x0D`, `boss_timer = 771` |
| stage end hardcoded | `stage::section_done` uses `SECTION_THRESHOLD`; no `STAGE_END[stage]` check |
| command `$A4A0` | `game.rs::command` uses `bytes[2] & 0x0F`, sets `boss: true`, and implements no `$0600` cannons |
| `$14` | `game.rs::update_objects` routes `0x14` through `_ => ai_drift` |
| script `$C433` | absent; `$C40D`/`$C424` are not modelled |
| music | `gradius-app::music` only emits `Stage1`/`Stage1B`; `Music::Stage5` exists |
| stage-5 exceptions | only the missile probe (`section != 4`) and sprite `0x63`/`$AF48` (which is stage 6) are present |

## Implementation steps

1. **Stage record and shared core gate.** Add the stage data record (or the
   minimum stage-5 fields) and use `$9A30[stage]` / `$98F0[stage]` instead of
   the `0x0C`/`0x0D` literals. Keep the `$81 -> $82 -> $83 -> $84 -> $85`
   state sequence; stage 5 waits at `$0B`, spawns at `$0C`, ends at `$0D`.
   Re-check the `$82` countdown (`771` is stage-1-shaped) against
   `record-boss --stage 5`; pass it through the stage/rank table if it differs.
   The core itself (`$B95C`, 7 hits, +1000, 514-frame clear, covers) is shared
   and must stay bit-identical for stage 1.
2. **Background and tail.** Select `stage_bgs::STAGE5` when `section == 4`.
   Render the tail from world column 384 (3072 px): switch to CHR bank 2 and
   set background palettes 2-3 to black (`0F` entries 8-15) using the measured
   tail palette in the `stage_bgs` header. Do not regenerate or edit the RLE
   data. Preserve stage 1's `STAGE1` path exactly.
3. **Music.** Add stage-5 cues (`Intro`/`$93` before `$3F >= 4`, then
   `Music::Stage5`/`$5F`, boss `$A5`, clear `$AC`) and map them in the SDL and
   WASM backends. Stage 5's second theme starts at the `$8354` boundary 4, not
   at the boss.
4. **`$A4A0` and the `$0600` cannon records.** Implement the real attachment
   records: every non-zero nibble of `b2` creates one `$0600` record with the
   parent slot and cannon index `nibble - 1`; `$016C` stores the count. Fix
   `command()` accordingly and stop marking `$14` objects as `boss`. Implement
   `$CB8B` placement, `$CBC4[rank]` fire period, `$CBCB` type-0 bullet and
   `$CB48` release as kind-2 debris.
5. **`$14` (`$CA58`).** Add `14 => ai_large_object`. Reproduce the state
   machine in `handlers_14_16.md`, including the `$CA43`/`$CA4A`/`$CA51[rank]`
   thresholds, one-time morph (`$0460 = 0`, x/y +8), sprite `$81/$82` and
   `$83/$84`, re-aim timer, y clamp `$20..$A8`, 500-point death, sound `$0A`
   and cannon release. Do not route it through `ai_drift`.
6. **`$C433[4]` script.** Implement `$C40D` (every 4th frame, free-slot scan
   9 down to 0, `$A521` clear) and `$C424`/`$C433` dispatch. Stage 5's handler
   is `$C64D`: increment `$68`, and every `$28` script calls take the next
   `$C674` `(b2, y)` pair, set kind `$14`, and call `$A4A0`. Preserve the
   `$3A != 0 -> $C680` quirk as a documented hook; stage 3's `$C680` is out of
   scope. Stage 7's `$C423` must stay a bare `RTS`.
7. **Stage-5-only branches.** Add the `$19 == 4` cases: `$9658` alternate
   update chain (`$5C >= 2` and odd `$02`: `$A2BA`, `$CB8B`, `$ADA5`,
   `$BBB1`, `$9FF6`, `$C0C1`, then `$5B = 1`), `$8B92` cannon OAM/trail,
   `$9F4D` attribute skip, `$A178` (already present), `$C031`, `$C259`,
   `$C2A5`, `$C2F4`. Keep each as a small `if section == 4` branch; do not
   refactor shared collision code.
8. **Verification harness.** Add stage selection to the differential harness
   so the native `Game` and the oracle can both start at `$19 = 4`; then run
   the state/pixel/boss windows below. Until that exists, stage-5 native
   evidence is unit tests only and oracle evidence is `record-boss --stage 5`.

`$AF48` is **stage 6** (`CPY #$05` -> sprite `$63`), not stage 5;
`$B95C` is the shared core-death path used by stage 5 and is modified only if
the shared core gate changes. Do not add stage-5 branches for either.

## Card breakdown

| card | scope | owned files | acceptance | deps | h |
| --- | --- | --- | --- | --- | ---: |
| S5-00 | stage record/table + shared core/end gate | `crates/gradius-core/src/stage.rs`, `game.rs` | `STAGES[4]` has the row above; stage 5 spawns at `$3F=0x0C`, ends at `0x0D`; stage 1 state hash unchanged; `$B95C` +1000 | none | 6 |
| S5-01 | background selection + column-384 tail | `game.rs`, `stage_bgs.rs` tests | section 4 renders `STAGE5`; at world col 384 bank 2 and tail palettes; stage 1/others unchanged; `$9F4D` path covered | S5-00 | 5 |
| S5-02 | music `$5F` and backend mapping | `crates/gradius-app/src/music.rs`, `crates/gradius-sdl/src/music.rs`, `crates/gradius-wasm/src/lib.rs` | cue table stage 5: intro, `Stage5B` at `$3F>=4`, boss, clear, game over; stage 1 cues unchanged | none | 3 |
| S5-03 | `$A4A0` + `$0600` cannons + `$CB8B`/`$CB48` | `game.rs` | b2 nibbles create 0-2 records, parent/index correct; fire period/by rank; release on death; `$14` no longer `boss` | S5-00 | 8 |
| S5-04 | `$14`/`$CA58` large object | `game.rs` | morph at `$CA43[rank]`, death at `$CA4A[rank]` with 500, sprite/aim/clamp, no drift with cannons; frame-3500 state matches probe | S5-03 | 10 |
| S5-05 | `$C40D`/`$C433[4]`/`$C64D` script | `game.rs` | every 4th frame, free slot scan; `$C64D` emits the four `$C674` objects; `$3A` quirk hook; stage 7 RTS | S5-03 | 6 |
| S5-06 | `$19==4` exception chain and OAM/collision hooks | `game.rs` | `$9658` alternate chain and `$5B`; `$8B92` cannon OAM; `$C031`/`$C259`/`$C2A5`/`$C2F4`; no stage-1 regression | S5-04, S5-05 | 6 |
| S5-07 | differential/pixel/boss harness + evidence run | `crates/gradius-diff/src/main.rs`, `game.rs` stage hook | `--stage 5` starts both sides; boss window 0 divergences; pixel report-only at the measured 99.0% / 98.8% (HUD 100%); `record-boss` summary matches | S5-00, S5-02, S5-06 | 6 |

Total: **50 h** (six to seven working days). Critical path is
`S5-00 -> S5-03 -> S5-04 -> S5-06 -> S5-07` (~36 h); S5-01 and S5-02 can run
in parallel, and S5-05 can start once S5-03 lands.

### Parallel / conflict rules

* **Every card except S5-02 touches `game.rs`.** Do not run two of them in the
  same worktree. S5-01 and S5-03 may be prepared separately but must be
  rebased in the order above; S5-04 and S5-05 both edit the object dispatch
  and `normal_frame` region, so sequence them.
* S5-02 is the only fully parallel card (frontends/music only).
* S5-07 can begin after S5-00 (add `--stage` plumbing) but its green run is
  last.

## Risks and exact evidence

1. **`$A4A0` cannon count is not the prose table.** `handlers_14_16.md`
   says `b2 >> 4` and lists `0/1/0/2`; the disassembly creates one `$0600`
   record for each non-zero nibble, and a stage-5 probe at frame 3500 shows
   slot 8 `$016C = 2` (two records) and slot 9 `$016C = 1` (one record). Use
   `/tmp/opencode/st5.ram` and `cargo xtask disasm --from A4A0 --to A521` as
   the authority. Do not "fix" this to the note.
2. **`$14` field aliasing and drift.** `$042C` is a handler frame counter,
   `$016C` is the cannon count and `$04AC` is the re-aim timer; the port's
   `Object` currently uses `anim` for `$016C` and `ai_mode` for `$04AC`.
   Add explicit fields or map with comments. The ROM calls `$AEDB` twice on
   the no-cannon path; verify the actual x displacement against the probe
   rather than assuming the note's 0.5 px/frame.
3. **Tail palette is not in `Screen`.** `Screen` stores the dominant bank and
   palette; stage 5's column-384 switch to bank 2 with palettes 2-3 black must
   be a renderer rule. `$9F4D` skips the `$9FAE` attribute write for `$19=4`;
   the extracted attributes already contain the measured result.
4. **`$9658` changes frame order and scroll.** The stage-5 alternate chain
   sets `$5B` (scroll stop) after player/collision updates. A wrong gate
   (`$5C`, parity, or `section`) will desynchronise every stage; add a
   stage-1 hash test and a stage-5 scroll test.
5. **Core gate refactor can move stage 1.** `boss_timer = 771` and the
   `0x0C`/`0x0D` literals are stage-1 measurements. Stage 5's threshold is
   `$0B`, and the `$82` countdown source (`$9A28` rank vs stage) must be
   confirmed against the measured 10274 spawn before making it generic.
6. **Music inference.** `MusicDirector` infers boss/clear from scroll and a
   score jump. Stage 5's second theme starts at `$3F >= 4`; the boss is at
   `$0B`. Do not reuse `BOSS_SCROLL = 2820` for every stage.
7. **The native stage-5 diff landed with S5-07.** `gradius-diff --stage 5`
   starts both sides and the checked-in 297-8573 fixture replays it; it was a
   real prerequisite for pixel/state parity, not a formality.
8. **`$AF48`/`$B95C` labels.** The card input calls them stage-5 exceptions;
   the disassembly shows `$AF48` is `CPY #$05` (stage 6) and `$B95C` is the
   shared core death. Keep them out of `$19 == 4` branches.

## Evidence commands

Use the card target directory everywhere:

```sh
ROM="$GRADIUS_ROM"
export CARGO_TARGET_DIR=/tmp/opencode/target-stage5plan
```

Data and roots:

```sh
cargo xtask stages "$ROM" | grep '^    5'
cargo xtask sections "$ROM" | grep -E 'S4_TERRAIN|S5_TERRAIN'
cargo xtask stage-enemies "$ROM" | grep '^\$14'
```

Handler, script and exception disassembly:

```sh
cargo xtask disasm "$ROM" --from C40D --to C700 --labels --symbols tools/ghidra/symbols.txt
cargo xtask disasm "$ROM" --from C64D --to C6A0 --labels
cargo xtask disasm "$ROM" --from CA58 --to CB28 --labels --symbols tools/ghidra/symbols.txt
cargo xtask disasm "$ROM" --from CB48 --to CC30 --labels
cargo xtask disasm "$ROM" --from 9640 --to 96B0 --labels
cargo xtask disasm "$ROM" --from 8B80 --to 8C50 --labels
cargo xtask disasm "$ROM" --from 9F40 --to 9F90 --labels
cargo xtask disasm "$ROM" --from A160 --to A1A8 --labels
cargo xtask disasm "$ROM" --from C020 --to C060 --labels
cargo xtask disasm "$ROM" --from C250 --to C300 --labels
cargo xtask disasm "$ROM" --from AF28 --to AF60 --labels   # $AF48 is CPY #5
cargo xtask disasm "$ROM" --from B940 --to B980 --labels   # $B95C shared death
```

Stage-5 oracle and `$14` state:

```sh
cargo xtask record-boss "$ROM" --stage 5 --frames 13000 --out /tmp/opencode/boss5.txt
ST5=$(for f in $(seq 170 320); do printf -- '--poke %d:26:4 ' "$f"; done)
cargo xtask probe "$ROM" --frame 3500 --press 180:start --keep-alive $ST5 \
  --ram /tmp/opencode/st5.ram --oam /tmp/opencode/st5.oam --png /tmp/opencode/st5.png
```

Expected `record-boss` summary: `spawn 10274 ... end 11489 ... stage 6 from
12003`. In `/tmp/opencode/st5.ram`, slot 8/9 are kind `$94`, `$016C = 2/1`,
`$0460 = 1`, `$048C = 1`, and `$0600` records parent slots are 8,8,9.

## Verification windows

After S5-07 adds `--stage 5` to the harness:

```sh
PACK=/tmp/opencode/stage5.pak
cargo xtask extract "$ROM" "$PACK"
flock /tmp/verify-heavy.lock cargo build --release -p gradius-diff

# state / boss window: spawn through next stage
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage5plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 12010 --stage 5 \
  --from 10250 --to 12010 --keep-alive --native-keep-alive \
  --report --json /tmp/opencode/stage5-boss.json

# pixel window: tail after world column 384 (scroll 3072 px), then boss entry
flock /tmp/verify-heavy.lock /tmp/opencode/target-stage5plan/release/gradius-diff \
  --rom "$ROM" --pack "$PACK" --frames 12010 --stage 5 \
  --pixel-report --pixel-window 3000:3600 --png-dir /tmp/opencode/stage5-png \
  --png-every 30 --json /tmp/opencode/stage5-pixel.json
```

Pass criteria: boss/state window has `result: WITHIN TOLERANCE (0
divergences)`; the checked-in 297-8573 pixel window stays a report-only `WARN`
at the measured 99.0% overall / 98.8% playfield / HUD 100.0%; the tail samples
show bank 2 and palette entries 8-15 = `0F`; stage 5 exits to `$19 = 5` at
the measured 12003 frame (± the harness script). Run `scripts/verify changed`
in each code worktree; do not run the full workspace suite there.

## Definition of done / out of scope

Done when a stage-5 run selects `SECTIONS[4]`, renders `STAGE5` plus its
column-384 tail, plays `$5F` after the boundary, runs `$C64D` and `$14`
objects, reaches the shared Big Core at `$0B/$0C`, awards +1000, advances at
`$0D`, and reproduces the 10274/10753/11489 oracle landmarks without changing
stage 1.

Out of scope: stage 3's `$C680`/`$C900` turret, stage 6 `$AF48`/`$CD9F`,
stage 7's `$9865` loop, and the `$39`-set stage-2 quick-kill branch. They are
shared hooks only and get their own plans.
