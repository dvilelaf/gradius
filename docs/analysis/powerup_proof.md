# Power-up proof

All six power-ups are proved twice: a core integration test asserts the
simulation *and* the sprite/framebuffer output, and `xtask powerup-shots`
renders one screenshot per power-up plus a labelled contact sheet. The six
oracle captures and the port-vs-ROM comparison are in
[`powerup_oracle_shots.md`](powerup_oracle_shots.md); capsule drops are verified
in [`capsule_drop.md`](capsule_drop.md).

## Reproduce

```sh
export CARGO_TARGET_DIR=/tmp/opencode/target-powerproof

cargo test -p gradius-core --lib power
cargo test -p gradius-core --lib speed_up_raises
cargo test -p gradius-core --lib a_real_capsule_pickup

cargo run -p xtask -- powerup-shots assets/gradius.pak --out /tmp/opencode/powerproof
```

The command writes `power-speed.png`, `power-missile.png`, `power-double.png`,
`power-laser.png`, `power-option.png`, `power-shield.png` and
`power-contact-sheet.png`. Each contact-sheet cell is labelled with the power
name and the state (`SPEED`, `BULLETS`, `OPTIONS`, `SHIELD`).

## Per-power assertions

Every test starts from the same hook: `Game::dev_start_run()` puts the game in
`Playing`/`Phase::Normal` at the stage-1 ship start, `dev_power_mut().select()`
places `$42`, B is held while `$0100 == 1` so the normal `$8979` path applies the
item, and fire is held for 90 frames. A successful handler clears `$42`, so the
held B cannot repeat the item.

| power | slot | asserted effect |
| --- | --- | --- |
| SPEED | 1 | `speed_level` +1 and `speed_delta` grows; right-held ship travels >= 80 px in 60 frames |
| MISSILE | 2 | an active kind-3 shot exists, its group sprite (`$65` free flight / `$61` riding) is in `build_sprites`, and after 6 frames x and y both increased |
| DOUBLE | 3 | two active kind-0 shots exist at once and both `$6B` sprites are in the sprite list |
| LASER | 4 | a kind-1 shot exists and both 8 px columns (`x` and `x + 8`) have `$6B` sprites |
| OPTION | 5 | one active drone trails the ship after 90 right-held frames and its group-4/5 sprite is in the sprite list |
| SHIELD | 6 | `$46 = 5` and one group-90..93 ring sprite (tile `$B7/$B9/$C1/$C3`, attr `$21`) at ship `(+15, -1)` |

The real capsule path is covered separately: `dev_spawn_capsule_at_player()`
inserts a real kind-1/subtype-6 object, the normal collision pass consumes it
and advances `$42` to 1, then held B applies SPEED on the pickup frame.

## Shield ring

The shield ring is the cartridge's own visual, not a port addition. The SHIELD
oracle capture in [`powerup_oracle_shots.md`](powerup_oracle_shots.md) shows one
8x16 sprite at ship `(+15, -1)`: groups 90-93 (`$8EE2/$8EE7/$8EEC/$8EF1`) share
`dx = 15, dy = -1` and tiles `$B7/$B9/$C1/$C3`, attr `$21`. The draw path
`$8B70-$8B8B` reads `$46` and selects one group per frame as
`$5A + (($02 >> 2) & 3)`, so exactly one ring tile is visible and it cycles.
The port reproduces this, with `$46` still the five-hit counter. The ring is
presentation only: it never changes simulation, collision or rank.

The drop-to-activation chain is in [`capsule_drop.md`](capsule_drop.md):
capsules drop, are collected frame-exact, and `$8979` applies the selected item
while B is held and `$0100 == 1`.
