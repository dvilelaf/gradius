# Start pause

What the cartridge does when Start is pressed in play, measured with the oracle
(`cargo xtask ramdump "$ROM" --watch 280:340 --press 180:start:3 --press 285:start:3
--press 297:start:3 --press 310:start:3 --assist --range 0014:0016`, and the
same with `--range 001B:001C`), plus `xtask disasm`. No cartridge bytes are
copied here.

## The ROM

| addr | meaning |
| --- | --- |
| `$05` bit 4 | Start edge (`$81B7` pad read, `$81F9`-`$81FE`) |
| `$14` | `$14 ^= $05 & $10` at `$81DB`-`$81E1`: a Start toggle latch. It flips on every Start edge, in or out of play |
| `$15` | the pause flag, set at `$9AE8`, cleared at `$9B1D` |

The per-frame stage handler ends at `$9AC1`-`$9B37`. The Start code is reached
only when `$1B` bit 7 is set with bits 4-6 clear (`$9ACB`-`$9AD1`) and
`$09 | $16 | $0D == 0` (`$9AD4`-`$9ADA`):

* `$15 == 0`, `$05 & $10`: `$15 = 1`, copy `$B0..$C0` (sound-engine channel
  state) to `$01A0..`, start sound `$3B` through `$EC18` (the pause jingle).
* `$15 != 0` (`$9AF9`): the Konami continue code is scanned (`$9758`, table
  `$9778`) on `$05`; a full code (`$33 == $0A`) runs `$9C58` and `DEC $3B,X`.
  Start (`$9B15`): `$15 = 0`, `INC $5B`, `$B2 = 0`, silence `$4000`/`$4008`,
  copy `$01A0..` back to `$B0..`.

While `$15 != 0`, `$964F` jumps straight to `$9A7F`, past the object, spawner and
player passes, and `$9A8B` skips the scroll step (`$98E1`). The NMI, the HUD
(`$889D`), the sprite builder (`$9D7D`) and the frame counter `$02` keep running,
so the screen stays identical and only the music engine moves (the `$3B` jingle
plays out in `$B0`, then the channels stay quiet).

## Frames (oracle, press = `FRAME:start:3`)

| press | `$05` bit 4 | `$15` | effect |
| --- | --- | --- | --- |
| 700 | f701 | 0 -> 1 at f702 | scroll `$3E` last steps at f702 and holds until f744 |
| 740 | f741 | 1 -> 0 at f742 | `$5B` is 1 for f742 only; the scroll steps again at f744 |
| 285 (entry blank, `$0D != 0`) | f287 | stays 0 | `$14` toggles at f287, no pause |
| 297 | f298 | 0 -> 1 at f299 | `$1B` became `$80` at f292, so Start works from ~f297 |
| 1100/1140 (death, `$1B = $A0`, f1087-1207) | | stays 0 | ignored |

So the flag flips one step after the edge the port sees, and the step that sets
or clears it still runs as it was (a pause lets one more stage frame through; a
resume holds one more frozen frame).

## Alternating 2P (oracle)

`$9ADC` loads `$05` with no index, so the Start toggle never follows `$18`.
In a 2P game (`$03 = $70`) `$05` is the raw first pad: P1's Start pauses on
either player's turn and P2's Start never does. Measured on `p2-handoff`
(Select 180, Start 220, P1 dies at 1194, P2 plays from 1221):

| frame | pad | `$05` | `$06` | `$18` | `$15` |
| --- | --- | --- | --- | --- | --- |
| 900 | P2 Start | 00 | 10 | 0 | 0 (inert) |
| 1240 | P2 Start | 00 | 10 | 1 | 0 (inert) |
| 1240 | P1 Start | 10 | 00 | 1 | 1 at 1241 |

In 1P the same `$05` is the `$8203` merge, so P2's pad pauses there (P2 Start
at 900 gives `$05 = $10` and `$15 = 1` at 901). The port used to feed the
current player's routed pad to the pause; it now takes `$05` as above.

The attract Start path also had to lock `$03 = $824C[$0F]` (`$8225`): the demo
runs as 1P, and the `$00 = 3` countdown kept that mode, so a 2P lock entered
through the attract played as 1P and let the second pad reach `$05`.

## The Konami code in 2P (oracle)

The pause scan at `$9758` is reached from `$9AF9` with `X = $18`, so unlike
the Start toggle it reads the *current* player's byte (`$05,X`: `$05`/`$06`).
In the same paused P2 turn, the code typed on P2 advances `$33` 1..9 and
`$3C = $FF`; the same code on P1 leaves `$33` at 0. The port already passes
the routed pad edge to `scan_pause_code`.

## The port

`Game::paused()`, `pause_tick` in `crates/gradius-core/src/stage_flow.rs`: an
edge arms `PAUSE_DELAY = 1`; one step later the flag flips if `can_pause()`
(phase `Normal`, no demo, `scroll_hold == 0`, no game-over timer, not the
ending). `step_playing` does not run while paused, so scroll, objects, timers,
stars and the spawner all hold; only `frame` and the NMI counter advance. The
pause byte is `$05`: in a 1P game that is the `$8203` merge of both pads, in
2P only the raw first pad (see below), and the simultaneous mod keeps either
pad. `Game::pause_edge` implements exactly that, independent of `$18`.

The flag is in the save state as an optional EXT tail written only while it
matters; an unpaused image is byte-identical to the older layout.

Gate: `gradius-scenarios --only pause-start` runs the oracle and the port from
reset with the presses above and compares state, objects and pixels every frame
(clean, 100% pixels). `PAUSE_DELAY` 0 and 2 both diverge at f702/f703.

## Music and jingle

`$9AE8`-`$9AF6` copies `$B0..$C0` to `$01A0..` and requests `$3B` through
`$EC18`; `$9B1D`-`$9B35` copies it back and rewrites `$4000`/`$4008`. So the
music engine state is frozen across the pause and the track continues where it
stopped; only the `$3B` jingle sounds meanwhile.

The core emits `Sound::Pause` on the step the flag turns on and `Sound::Resume`
on the step it turns off. A frontend plays `Sfx::Pause` (the captured `$3B`
timeline, `pause` in the audio chunk) on `Pause`, freezes the music voice
(`Engine::pause_music`) and unfreezes it on `Resume`. Desktop (`gradius-sdl`)
and web (`gradius-wasm`) do the same through the engine.

## The Konami code in the pause (oracle)

`$9AF9`-`$9B13`: while `$15 != 0` and `$3B,X >= 0` (`X = $18`), `$9758` scans
`$05,X` with the table at `$9786`: Up, Up, Down, Down, Left, Right, Left, Right,
B, A (`$9786` holds the ten button masks the scan compares against). A wrong edge resets `$33` to
0, a zero `$05` is ignored. A full code (`$33 == 10`) runs `$9C58` and
`DEC $3B,X`, `$33 = 0`.

`$9C58` stores `$46 = 5`, `$41 = $40 = 1`, `$45 = 2`: the demo loadout (shield
of five hits, one speed-up, missile, two options). It sets, it does not add.
`$3B` is the per-player counter (`$3B`/`$3C`), cleared at a new game and
incremented only by `INC $3B,X` at `$B97B` in the core's death routine. A code
therefore works once per stage cleared: the first one leaves `$3B = $FF`, and
every later code in that stage is skipped before `$9758` (`$33` stays 0).

Measured (`--press 180:start:6 --press 297:start:3`, then the code with
`FRAME:button:3` every 6 frames from 320): `$33` goes 1..9 at f322..f370 and
back to 0 at f376, and `$3B 00 -> FF`, `$40 = $41 = 1`, `$45 = 2`, `$46 = 5`
all change at f376 (A pressed at f374; the handler sees `$05` one step after
the port's edge, like the pause toggle). A second code from f420 in the same
pause changes nothing. The same code outside a pause changes nothing (the scan
sits behind `$15`).

Port: `pause_tick` scans the previous step's edges while the flag was already
set; `Game::pause_code` holds `$3B`/`$3C`, `start_stage_clear` increments it.
`Game::pause_edges` and `pause_code` are saved in the pause EXT tail (extended
in place; older images restore with zero). Gate: `gradius-scenarios --only
pause-konami` (code outside the pause, code inside, resume, second pause and
code) matches the oracle in state, objects and pixels (100%).

## Pause in the transition holds (oracle)

The gate at `$9ACB`-`$9AD1` only needs `$1B` bit 7 set and bits 4-6 clear, so
`$81`..`$86` all pause. Measured on the boss script (`scripts/boss-script.txt`,
`--assist --keep-alive`): `$1B = $86` from f9087 to f9600 (the stage-1 clear
scroll), and `$81` at f15744, `$82` f15745-f16769 (stage 2's checkpoint wait).
Start at f9120/f9180/... toggles `$15` through the whole `$86` run (flag 1 at
9122, 0 at 9182, and so on), and Start at f15744/15900/15950 pauses and resumes
in `$82` (`$15 1` at f15746, `0` at f15902, `1` at f15952). `$90` (bit 4) is
ignored, as before.

The port already paused `$86` (it runs inside `Phase::Normal`); `can_pause` now
also accepts `Phase::Transition`, the `$81`..`$84` section hold. Gate:
`gradius-scenarios --only pause-transition` (boss script plus Start at 15900,
15950, 16000, 16100 in the `$82` wait): flag and `$1B` as above, state clean in
the window. Pixels are 99.8%: a stage-2 sprite difference of about 170 pixels
starts at f15783, 120 frames before the first pause, and stays the same through
the pause; it is not caused by it.

## Not modelled

* The jingle and the music state are not a ROM-level `$B0` engine: the
  frontends freeze the captured music timeline and overlay the captured `$3B`
  effect.
* `$1B` values other than `$80`..`$86` are treated as "no pause". `$81` itself
  lasts one frame and was not pressed on in the oracle; it passes the same gate.
* `$16` is never written by a direct store in the PRG; it reads 0 in play.

## Frontend check (AppImage, Xvfb, `SDL_AUDIODRIVER=disk`)

`scripts/package-appimage.sh`, then the AppImage on its own Xvfb with
`GRADIUS_DATA` pointing at a temporary pack and the SDL disk audio driver
(mono 48 kHz s16). Enter starts, then in stage 1: Enter (pause), the Konami
code, Enter (resume). RMS per 50 ms of the captured output: the stage music at
about 3,400 until 13.75 s; from there the `$3B` jingle (about 1,700, decaying
to 0 at 14.55 s); exact silence from 14.55 s to 23.25 s (the whole pause, with
the code typed in it); the music back at 23.3 s, starting in the same quiet
bar where it was cut.
