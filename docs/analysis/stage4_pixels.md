# Stage-4 pixels: the poked 297-1797 window

Measured 2026-11-11 on `agent/s4pixels` (from `main` at `342fc5e`), against the
cartridge `Gradius (E).nes` (SHA-256
`268dc4b5b344e06e96281eb3159c1f61d3011c4f21dd1a2b4508aa934aff6143`) and the
pack built by `xtask extract`. The window is the poked stage-4 run the state
gate uses (`crates/gradius-diff/tests/stage4_replay.rs`): oracle frames
297-1797, `--stage 4` (oracle enters stage 4 at 292 and the native
`Game::new_at_stage` starts at 297), the harness's default input script and no
keep-alives.

The compared state is exact on every field and object slot except the shared
ship-init residue: `px`/`py` on the 81 respawn frames (162 divergences). Every
pixel below is renderer residue, not a simulation divergence, except the three
sampled respawn frames where the 0,0 ship position is visible.

## Result

Report-only probe: `crates/gradius-diff/tests/stage4_pixels.rs` samples every
30th frame (50 samples, as `scripts/longrun.sh` does). It asserts only the
window shape, not a rate.

| window | samples | differing / total px | match | play | hud |
| --- | ---: | ---: | ---: | ---: | ---: |
| 297-1797 (sampled, every 30) | 50 | 6,976 / 3,072,000 | **99.773%** | 99.750% | 99.919% |
| 297-1797 (per-frame sweep) | 1502 | 198,053 / 92,282,880 | 99.785% | 99.767% | 99.908% |

The per-frame sweep is the same command with `--png-every 1`; its count is
1502 because the probe labels samples with the harness frame (0-based), so the
first compared oracle frame (297) is sample 296 and the last (1797) is 1796.
The sampled probe is the committed measurement; the sweep confirms the same
rate and the worst-frame order.

Worst sampled frames:

| frame | state | match | differing px | bbox |
| ---: | --- | ---: | ---: | --- |
| 1320 | Playing | 98.7% | 800 | 57,17-252,87 |
| 1080 | Playing | 99.2% | 511 | 38,35-237,57 |
| 1350 | Playing | 99.3% | 410 | 33,17-237,58 |

The per-frame sweep ranks the same cluster (1317 98.5%, 902 px; 1319 98.6%,
856 px; 1320 98.7%, 800 px). The union box for the whole window is
`0,17-255,227`; the HUD band is clean except the digit below.

## What differs

Three classes, all presentation-only:

1. **Playfield sprite residue.** 35 of the 50 samples carry some difference
   (1136 of 1502 in the per-frame sweep). Most are a few hundred pixels in the
   upper playfield, e.g. the 1310-1350 cluster at `x 57-252, y 17-90` where
   the five-slot scout waves (`kind $85`, Y 42/160) are in flight. The object
   table is exact (`obj.kind`/`x`/`y`/`substate` 100%, slot match
   7167/7167), so this is sprite presentation: OAM ordering/rotation picking
   a different subset. Route: `game.rs::build_sprites` / `apply_oam_ring`.
2. **HUD lives digit.** From sample 1470 to the end of the window every sample
   differs by 30 px at `x 40-46, y 224-227`, the `STATUS_TILES` lives column
   (`hud.rs::LIVES_COLUMN`): the port draws a white glyph where the oracle has
   already blanked the digit. A handful of earlier isolated frames (370,
   873-899, 1036-1042) are the `$0700` score queue landing a few frames off
   (the `ScoreDelay` lag). Route: `hud.rs` score queue / lives digit.
3. **Respawn frames.** 3 of the 81 `Other(4)` respawn frames fall on a sample
   (496, 738, 1409) and show 118/22/478 playfield px at `x 6-254, y 0-66`.
   That is the port's fresh-life ship at 0,0 versus the oracle ship held at
   its previous position, i.e. the same `px`/`py` state residue. The other 78
   respawn samples are in the blank window and match exactly.

No hard threshold yet: the residue has no owner. Once the sprite group and
HUD classes have one, pin 99.8% (or the cleaned rate) in the probe, as stage 2
did for its boss window.

## Background: exact, including the column-416 tail

`STAGE4` (`crates/gradius-core/src/stage_bgs.rs`) is 480x28 tiles (3840 px),
dominant CHR bank 0, and the 32-byte `$3F00` page (four background and four
sprite palettes) read from the cartridge at run time.

Verified against the oracle with
`xtask screens <rom> --frames 20000 --assist --stage 4 --press 180:start
--sample-scroll 0 --sample-scroll 512 --sample-scroll 1100 --sample-scroll 3328
--sample-scroll 3500`:

| check | value |
| --- | --- |
| tiles FNV-1a | `228C903AE305586D` |
| attributes FNV-1a | `9E8E0821170D5B2A` |
| dominant palette FNV-1a | `90FA63C90403F3FC` |
| scroll range | 0..3584 px (map 3840 px, last 256 px only reachable at max scroll) |
| CHR banks seen | 0 (7714 frames), 2 (2147 frames) |
| 0 px | 0 tile / 0 attribute mismatches, bank 0, dominant palette |
| 512 px | 0 tile / 0 attribute mismatches, bank 0, dominant palette |
| 1100 px | 0 tile / 0 attribute mismatches, bank 0, dominant palette |
| 3328 px | 0 tile / 0 attribute mismatches, bank 2, tail palette |
| 3500 px | 0 tile / 0 attribute mismatches, bank 2, tail palette |

The oracle switches at world column 416 (3328 px) to CHR bank 2 and forces
palette entries 8-15 (palette slots 2 and 3) to `0F`, keeping 0-7 dominant.

The hashes and the 480x28 shape are also pinned by the core unit tests
(`stage_4_matches_the_oracle_digests`, `stage_4_map_has_the_documented_shape`,
`stage_4_tail_window_matches_the_oracle_digest`).

**The tail is implemented.** `stage_catalog::STAGES[3]` carries
`quirks: QUIRK_TAIL` with `tail_column: Some(416)`, so `Game::render`
(background and cell overlay) and `hardware_bank` switch to CHR bank 2 and
force palette entries 8-15 to `0F` from world column 416 exactly as the oracle
does, and the `xtask screens` rows above confirm it at 3328 and 3500 px. The
297-1797 window ends at scroll ~320 px, far short of the cut, so none of the
residue measured here is tail residue: it is the sprite, HUD and respawn
classes listed above.

## Reproduce

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-s4pixels
ROM="$GRADIUS_ROM"

# committed report-only probe (needs GRADIUS_ROM and assets/gradius.pak)
GRADIUS_ROM="$ROM" cargo test -p gradius-diff --test stage4_pixels -- --nocapture

# the same measurement by hand, per-frame
cargo build --release -p gradius-diff
./target/release/gradius-diff --rom "$ROM" --pack assets/gradius.pak \
  --stage 4 --frames 1798 --from 297 --to 1797 --pixel-report \
  --pixel-window 297:1797 --png-every 1 --json /tmp/opencode/stage4-pixels.json
```
