# Modified mode

The port reproduces the cartridge by default: one hit kills, and two players
take turns. *Modified mode* is the opt-in set of deviations from that
behaviour. It is a per-run choice, never baked into the build, so a fresh
`Game` and the differential harness keep matching the ROM byte for byte.

## Settings

| setting | cartridge / default | modified value | effect |
| --- | --- | --- | --- |
| `--hits N` | `1` | `1..=99` or `inf` | hits a ship survives before dying; `inf` (`hull_hits: 0`, `hits=inf` in `settings.ini`) is invincible: nothing kills the ship. Every frame of contact costs one hit; `hit_invulnerable_frames` (core-only, 0 by default) can add a window after each non-fatal hit |
| `--start-stage N` | `1` | `1..=7` | stage a new game begins at (`start_stage`, `stage=N` in `settings.ini`). Start on the title enters the chosen stage with a new game's three lives, zero score and empty weapon, in 1P, alternating 2P and simultaneous 2P; the attract demo and `--stage` are untouched |
| `--2p-sim` | alternating play | on | both players are on the playfield at the same time, each steered by their own pad, instead of sharing `$18` one at a time |
| `extra_options` | two | four | the arcade loadout instead of the cartridge's two options; core-only, with no command-line flag |
| `stage5_slowdown` | off (port) / on (cartridge) | off | stage 5's `$9658` alternate chain: with two or more live cannons the cartridge runs the ship, object, fire and cannon passes every other frame and halves the scroll. `false` runs them every frame. The port ships without the halving; `--stage5-slowdown` (`slowdown=1` in `settings.ini`) restores the cartridge, and a fresh `Mods::default()` still has it on for the harness |

Every value is a field of `gradius_core::Mods`, which the frontend hands to
`Game::set_mods` before the first tick. `Mods::default()` is the cartridge, so
the differential harness and every `Game` that is not given options keep it
that way. The frontends start from the cartridge except for stage 5: they ship
without its slowdown and offer the flag, the menu row, the `settings.ini` key
and the PWA checkbox to put it back.

## SDL frontend

The same four settings are in the in-game options menu (Esc), saved in
`settings.ini` in the data folder; flags override them for that session, and a
change made during a game applies from the next one. A missing `slowdown` line
means off.

```sh
# Start at stage 5, three hits, stage 5's cartridge halving and simultaneous
# two-player, on one screen.
cargo run -p gradius-sdl --release -- \
  --pack assets/gradius.pak --start-stage 5 --hits 3 --stage5-slowdown --2p-sim
```

`--hits` is validated while parsing: `--hits 0`, `--hits 100` and `--hits many`
exit before a window opens with
`invalid hit count ...: expected 1..=99`. `--start-stage` takes 1..=7 and
rejects anything else the same way. `--2p-sim` and `--stage5-slowdown` are
plain flags.

All four controls appear in `--help`. The headless `--frames` path accepts
them too, so a smoke run can exercise a modified game.

## PWA

The browser build has no command line. The page sets the same values through
the runner API, after creating the runner and before the first tick:

```rust
use gradius_app::{Mods, Runner};

let mut runner = Runner::new(game);
runner.set_mods(Mods {
    hull_hits: 3,
    start_stage: 5,
    simultaneous_2p: true,
    stage5_slowdown: true,
    ..Mods::default()
});
```

The settings panel stores those numbers and passes them the same way, so the
PWA and SDL enable an identical mode. The slowdown checkbox starts unchecked,
which is the port's stage 5.

## What does not change

Score, the lives HUD, difficulty, stage content and the 60.0988 Hz clock are
untouched. `--hits` is per ship, not per run: the count resets when a life is
lost. A `--start-stage` run skips the stages before it, so their score is not
earned, and the attract demo still plays stage 1. The differential harness and
every recorded fixture keep the defaults, so modified mode can never drift the
parity measurements.
