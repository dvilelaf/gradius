# Stage-2 pixel gaps: 9601-12000

Analysis of the stage-2 stretch inside the boss-script run: the oracle clears
stage 1 at 9087 and resets into stage 2 at 9601, so frames 9601-12000 are the
first ~2400 frames of stage 2 (`$19 = 1`). The compared simulation is exact
across the whole window (2400 frames, 0 divergences, every field 100%); every
pixel difference below is therefore a renderer or undumped-state gap, not a
simulation divergence.

## Method

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-s2pixels
ROM="$GRADIUS_ROM"
cargo build --release -p gradius-diff

D=/tmp/opencode/target-s2pixels/release/gradius-diff
flock /tmp/verify-heavy.lock "$D" --rom "$ROM" --pack assets/gradius.pak \
  --frames 12000 --from 9601 --to 12000 --script scripts/boss-script.txt \
  --keep-alive --native-keep-alive --pixel-report --pixel-window 9601:12000 \
  --report --json /tmp/opencode/s2-boss-full.json
```

The `--pixel-report` table splits at `y = 208` (`HUD_TOP`); the actual HUD band
top is also 208 (`hud::HUD_Y`), so the two bottom playfield rows that still
show background in the oracle (y 208-209) are counted as HUD.

## Window table

| window | samples | match | play | hud | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| 9601-11040 (current gate) | 1440 | 99.9% | 99.9% | 99.8% | passes (`0.999`) |
| 9601-11199 (last pre-turret frame) | 1599 | 99.86% | 99.87% | 99.83% | still rounds to `0.999` |
| 9601-12000 | 2399 | 99.6% | 99.6% | 99.5% | `0.996`, fails a 99.9% gate |
| 11041-12000 | 959 | 99.1% | 99.1% | 99.1% | the post-gate drop |
| 11200-12000 (turrets) | 800 | 99.0% | 99.0% | 99.0% | dominated by one bug |

Worst samples of the full window: **11596, 11600, 11620 at 98.5%**, bbox
`10,54-255,238`. The current gate's worst samples are 9781, 9805, 10644 at
99.7% (bbox `12,95-210,218` and `14,82-210,218`).

## Divergence timeline

| frames | size | bbox (example) | class |
| --- | --- | --- | --- |
| 9601 only | 60 play + 13 hud | `37,98-210,218` | one-frame stage-entry edge: the oracle still shows the tail of stage 1 on the reset frame |
| 9602-9715 | 13 hud | `205,214-210,218` | constant power-meter row tile gap (see HUD below); playfield 100% |
| 9708, and every score change | 54 hud, 1-2 frames | `112,214-210,227` | score display lags one frame behind the oracle (`shown_score`/`score_delay`) |
| 9716 | 1 play | `130,109` | first playfield pixel: enemy sprite animation frame of a `$85` scout |
| 9717-9746 | 1-89 play | e.g. `135,107`, `40,109` | same enemy-frame mismatches, intermittent |
| 9748-11199 | 1-194 play (typical 30-100) | `22,108-210,218`, `143,84-217,90` | oracle-only palette-3 sprite group `$25` (tile `$69`, behind) plus enemy animation frames |
| 11040 | 86 play | `10,84-210,218` | end of the pre-turret regime |
| 11140-11207 | ~50-117 play | `108,72` etc. | kind `$04` arcs and the same effect sprites |
| 11200-12000 | 180-850 play | `10,14-255,238` | turret sprites `$91/$92` (dominant) |
| 11800-12000 | +150 hud | `10,208-240,238` | background rows y 208-209 in the split band |

Playfield is **exactly 100% from 9602 through 9715**. The first playfield
divergence is therefore **frame 9716 at (130,109)**; the first large one is
**frame 9748** (83 playfield px, 96 px with the HUD row).

## HUD (`hud.rs`)

* **Power-meter row, 13 px, `(205,214)-(210,218)`, every frame.** The oracle
  paints `$12` (background palette 0, blue) there; the port paints tile `0x00`
  (black). This is a missing/wrong entry in `hud::METER_TILES`/the right-hand
  meter columns, not the sprite-0 bar at x 248 (that one matches). Counted as
  HUD because `y >= 208`.
* **Score row lags one frame.** At 9708 the oracle shows `0027000` and the port
  `0026900` after the 9707 kill. `dump_state`'s `score` already matches, so this
  is `Game::render`'s `score_delay`/`shown_score`, only visible for the 1-2
  frames after a score change. Transient 54 px at `(112,214)-(210,227)`.
* **Split band is two rows too high.** The oracle keeps background tiles on
  y 208-209 and starts the fixed HUD rows at y 210; the port clears from
  `hud::HUD_Y = 208` (`fb.rect(0, HUD_Y, ...)` in `game.rs::render`). Late in
  the scroll the terrain reaches those rows, so from ~11800 the HUD rate falls
  to 97.9% (149 px at y 208-209 plus the 13 px meter row). This is the
  `HUD_Y` split boundary, not the meter bar.

## Playfield after 11040

The post-11040 drop is not background, not the tail and not the boss.

**Turrets (`game.rs::ai_turret`, the dominant cause).** Object kinds `$91/$92`
first appear at 11200/11204 and the playfield diff jumps from ~100 px to
~350-850 px. The port picks the wrong `$8ED9` sprite group: it emits a
four-sector table, the cartridge emits two sectors plus a Y adjustment that is
applied **only in the far sector**.

Cartridge `$B020`/`$B032-$B067` (kind `$91`; `$B092` joins at `$B032` for
`$92`):

* `dx >= 0`: `dx < $30 -> 0`, `dx < $60 -> 1`, else `1` (no second step) and
  fall into the Y check.
* `dx < 0`: `backward <= $30 -> 3`, `backward <= $60 -> 4`, else `4` and fall
  into the Y check.
* Y check: if `objY >= playerY` and `dy < $30`, or `objY < playerY` and
  `playerY - objY < $30`, do `INY` (index += 1). It runs only on the far
  branches (`$B053` is reached from `$B046`/the `$B048` fall-through), never on
  the near branches.

`Game::ai_turret` instead uses `0/1/2` and `3/4/5` sectors, applies the Y check
to every sector, uses `index.min(1)` for the downward case and the wrong
`$A0/$D0` thresholds for the left side. Confirmed against oracle RAM `$012C`
at frame 11596 (xtask probe, same script/keep-alive): slots 1 and 9 hold `$72`
and `$74` while the port's formula yields `$75` and `$73`; all other slots
match. Example: slot 1 at `(129,52)`, player `(22,73)` -> cartridge `Y=1` then
`INY` -> group `$72`; port `Y=2` then `+1` -> group `$75` (mirrored sprite).
Route: `game.rs::ai_turret`. This is derived sprite state (`object.sprite`),
so the state dump cannot see it; adding `sprite` to `dump_state` would make the
divergence visible.

**Oracle-only sprites group `$25`.** From 9748 the oracle draws small red/white
sprites that the port never emits. OAM evidence at 9800: entries `(55,106)` and
`(78,106)`, tile `$69`, attr `$23` = sprite group `$25` (single part, `dy=8`,
`dx=8`, palette 3, behind). They enter from the right edge in waves and drift
left/up. `Game::build_sprites` has no path that assigns group `$25` (enemy
bullets use `$3A`, explosions `$26-$2C`, death burst `$2D-$30`). The compared
dump also has no bullet/effect field, so this is invisible to the state gate.
Route: `game.rs` (the spawn/effect that emits `$25`); gating it needs an
enemy-bullet/effect field in the dump.

**Enemy animation frames.** Intermittent 1-90 px diffs from 9716 on `$84/$85/
$88/$8A` objects are sprite animation-frame differences. `subtype`, `anim`,
`anim_div` and `object.sprite` are not in the dump, so these cannot be
separated from the effect sprites without extending `dump_state`.

## What is not the cause

* **Column-416 tail / CHR bank 2 (3328 px):** not reached. Stage-2 scroll starts
  at `0:0:0` at 9601 and is only `4:175` (1199 px) at 11999, well below 3328.
* **Stage-2 core / boss / eruption:** not in this window. No `$98` or `$99`
  objects appear after 9601; the boss threshold is `$9A30 = $0C` (3072 px,
  ~frame 15700+ at the measured 0.5 px/frame). The measured standalone spawn
  (`record-boss --stage 2`: 7979) is ~7715 frames after stage entry, i.e.
  ~17300 here, not ~11140. Stage 2 has no `$8A` eruption at all.
* **Background palette/tiles:** the playfield is 100% from 9602-9715, and the
  stage palette `STAGE2` matches. The only background-class pixels are the
  split rows y 208-209 (above).
* **Compared state:** 0 divergences for the entire 9601-12000 window.

## Gate recommendation

The meaningful stage-2 window for the current build is **9601-11199** (stage-2
entry through the last frame before the first turret at 11200). It is the whole
pre-turret regime, still reads `0.999`/`0.999` (raw 99.86%), and its
boundary is a real landmark, unlike 11040. Keep `9601-12000` as a report-only
number until `ai_turret` is fixed.

After `ai_turret` and the HUD split/meter fixes land, extend the hard gate to
`9601-12000` and expect >= 99.9%; do **not** gate 11200-12000 before then (it
is 99.0% because of the turret sprite bug). Do not add a tail/boss sub-gate to
this window: neither is reachable before frame 12000.

## Saved pairs

Worst/named pairs are in `/tmp/opencode/stage2-pixels/`
(`oracle-*.png` + `native-*.png`):

| frame | why |
| --- | --- |
| 9748 | first large playfield gap: group `$25` effects |
| 11280 | turret wave, 99.3% play |
| 11596 | worst sample, 98.5% |
| 11800 | split-band rows y 208-209 |
| 11998 | end of window, 98.9% play / 97.9% hud |

The full 12000-frame pair dump is in `/tmp/opencode/s2png/` (even frames).
