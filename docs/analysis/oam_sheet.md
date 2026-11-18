# OAM sprite sheets, stage 1 frames 400 and 800

The sheets `oam-400.png` and `oam-800.png` are generated in the working directory and are gitignored. Each
renders every non-parked OAM entry as an 8x16 sprite with its slot, X, top Y,
tile byte, attribute, flip flags and sprite palette, using the stage-1 measured
sprite palettes from `stage1.md`.

## Reproduce

```sh
ROM="$GRADIUS_ROM"
python3 tools/oam_sheet.py "$ROM"
```

The script extracts the four 8 KiB CHR banks from the iNES image (16-byte
header + 32 KiB PRG), runs the oracle for both frames and writes the two PNGs
to the current directory:

```sh
cargo xtask probe "$ROM" --frame 400 --press 180:start --press 240:start \
  --oam oam-400.bin
cargo xtask probe "$ROM" --frame 800 --press 180:start --press 240:start \
  --oam oam-800.bin
```

Both dumps report `PPUCTRL=$A8`, `PPUMASK=$1E`, CHR bank 0, 8x16 sprites from
pattern table `$1000` and the same 32-byte palette RAM as `stage1.md`. Tiles are
decoded as `tile&$FE`/`tile|1`, honouring attribute bits 6 (H flip) and 7
(V flip); the game parks unused slots at stored Y `$F4`, so Y `$EF..$FF` is
skipped.

## What is visible

Each sheet has 13 non-parked entries (12 sprite positions plus the HUD bar).

| frame | visible |
| --- | --- |
| 400 | Vic Viper in three sprites at x 72/80/88, top y 97; four ring enemies at the top right (`$18/$19`, `$1A/$1B`, palette 2, y 43); one `$DE/$DF` palette-3 effect overlapping the ship nose; HUD power-meter bar at x 248, y 207 (`$6C/$6D`, palette 3). |
| 800 | Same ship position; four ring enemies moved to the lower centre (`$14-$17`, palette 2, y 161); the same `$DE/$DF` palette-3 effect at (68, 89); same HUD bar. |

The ring enemies are the four spinning objects in the framebuffer; each uses
two OAM entries, which is why eight non-parked slots show the same four rings.
No player shot or bullet sprite is present in either dump: the oracle run never
presses fire, and `stage1.md` notes the sprite-0 split loop blocks the oracle
before a shot can be fired. The `$DE/$DF` palette-3 sprite is the only
explosion-effect tile pair in these frames. The ship tiles are identical in all
four CHR banks, so the sheet renders bank 0, the bank the oracle selects.

## Files

* `tools/oam_sheet.py` - extract, probe and render.
* `oam-400.png`, `oam-800.png` - labelled sheets, generated locally (not committed).
* `docs/analysis/stage1.md` - palette and OAM evidence.
