# Audio

The port plays the original Konami sound driver's register stream rather than a
re-synthesis. `gradius-audio` contains a runtime NES APU model, and every music
and effect timeline is a list of real `$4000-$4017` writes captured through the
`gradius-nes` oracle. The capture lives behind the `capture` feature of
`gradius-audio` and runs at extraction time: `xtask extract` stores the
timelines in the pack's audio chunk. The cartridge is not read at runtime, and
the repository contains no timelines. `crates/gradius-audio/src/data.rs` keeps
only the track and effect identities.

## Capture

```sh
GRADIUS_ROM="/path/to/Gradius (E).nes" \
  cargo run -p gradius-audio --features capture --bin capture-audio
```

The tool only reports; it writes nothing. The pack is the one place the
timelines are stored. The tool:

1. injects a `JSR $EC18` sound request into a quiet driver and runs `$ECFC`
   once per frame, exactly as the NMI does;
2. locates the same request in a 12,000-frame assisted game run (the assist is
   analysis-only invulnerability, so the run reaches the Big Core);
3. finds the first frame where the injected write stream repeats, keeps the
   lead-in, and loops from that frame;
4. compares the injected stream against the cartridge's real writes at the
   request (`trigger`) and prints mismatch counts.

| track | request | writes | length | loop | real-game validation |
| --- | --- | --- | --- | --- | --- |
| title/attract fanfare | `$90` | 120 | 1.00 s | one-shot | matches the request window |
| stage 1 | `$93` | 2349 | 25.69 s | 520-frame lead-in + 1024-frame loop (17.04 s) | 0 / 162 writes |
| stage 1, second half | `$96` | 2274 | 27.97 s | 433-frame lead-in + 1248-frame loop (20.77 s) | 0 / 225 writes |
| stage 2 | `$59` | 1501 | 22.38 s | 1-frame lead-in + 1344-frame loop (22.36 s) | 0 / 1499 driver; run 239 / 283* |
| stage 3 | `$5B` | 1352 | 20.57 s | 8-frame lead-in + 1228-frame loop (20.44 s) | 0 / 1350 driver; run 130 / 258* |
| stage 4 | `$5D` | 1219 | 20.78 s | 1-frame lead-in + 1248-frame loop (20.77 s) | 0 / 1217 driver; no run window (stage clears early) |
| stage 5 | `$5F` | 1126 | 17.97 s | 1080-frame loop (17.97 s) | 0 / 1124 driver; run 0 / 262 |
| stage 6 | `$61` | 1914 | 33.94 s | 2040-frame loop (33.94 s) | 0 / 1912 driver; run 209 / 230* |
| stage 7 | `$63` | 997 | 23.69 s | 1-frame lead-in + 1423-frame loop (23.68 s) | 0 / 995 driver; run 159 / 166* |
| Big Core theme | `$A5` | 776 | 7.70 s | 79-frame lead-in + 384-frame loop (6.39 s) | 0 / 224 writes |
| stage clear | `$AC` | 388 | 1.40 s | one-shot | 0 / 374 writes |
| game over | `$AF` | 701 | 4.94 s | one-shot | 0 / 312 writes |

`*` The assisted run overlays sound effects on the same four channels, so a
music write can be suppressed while an effect owns the channel. The driver /
capture column is the exact one: the native driver over the pack reproduces
every capture write. Stage 5 happened to have no conflicting effects in its
window. The 80,000-frame assisted run reaches `$59`, `$5B`, `$5F`, `$61` and
`$63`; stage 4's `$5D` never appears because that stage clears inside one
sample, so it is the only injection-only theme (marked above).

Each loop also passes two independent checks:

* a second fresh injection reproduces the first write-for-write (0 mismatches);
* the loop period repeats register-for-register over two periods (0 mismatches,
  allowing a write to straddle a frame boundary).

The original stage-1 BGM is a 1024-frame loop after an eight-second lead-in, not
the 512-frame fragment used by the first audio milestone. At 48 kHz the loop
renders through the same APU and wraps without a discontinuity; the test
`stage_one_loop_seam_matches_the_first_pass` compares the largest sample step at
the seam with the same position on the first pass.

### Sound effects

All eight core events map to captured driver effects:

| event | request | duration |
| --- | --- | --- |
| `Shot` | `$01` | 0.60 s |
| `EnemyShot` | `$06` | 0.60 s |
| `EnemyHit` | `$05` | 0.45 s |
| `Explosion` | `$09` | 0.60 s |
| `PowerUp` | `$0D` | 0.85 s |
| `Capsule` | `$0E` | 1.05 s |
| `PlayerDeath` | `$36` | 1.51 s |
| `GameOver` | `$35` | 0.42 s |
| `ScreenClear` | `$0B` | 1.26 s |

`GameOver` is kept as a captured effect for completeness, but the frontend uses
the longer `$AF` game-over music instead.

## The pack carries the soundtrack

`xtask extract` writes the captured timelines into the pack, so a
`gradius.pak` is self-contained:

```sh
cargo xtask extract "/path/to/Gradius (E).nes" assets/gradius.pak
```

Payload chunk id `4` (`gradius_pack::CHUNK_AUDIO`) has this layout, documented
in full in `gradius_audio::chunk`:

```text
magic      4  "GAUD"
version    u16  currently 2 (1 = timelines only)
tracks     u16
per track:
  id       u8   Music / Sfx discriminant
  kind     u8   0 = music, 1 = sound effect
  flags    u16  bit 0 = loops
  len      u32  bytes after this header (lets a reader skip the track)
  name_len u8, name (UTF-8)
  total_cycles u32, loop_cycles u32, loop_index u32, event_count u32
  events: event_count * (frame u16, cycle u16, addr u8, value u8)
driver_len u32  0 when absent
driver:
  magic 4 "KDRV", version u16
  base u16 ($C000)
  request_addr u16, request_len u16, request bytes ($EFC7, 192)
  period_addr  u16, period_len  u16, period bytes ($EFB2, 24)
  channel_len u8 (17), channel_count u8 (4)
  per channel: block base u16, APU register offset u8
  segments u16
  per segment: addr u16, len u16, bytes
```

Events store `frame` and `cycle` within the 29,780-cycle NTSC frame; the loader
reconstructs the absolute cycle offset. Every track record carries its own
length, so a reader can skip a track id or kind it does not know and still load
the rest.

The driver section carries the request table, the period table, and only the
sequence bytes that the twelve music requests (`$90`, `$93`, `$96`, `$A5`,
`$AC`, `$AF`, `$59`, `$5B`, `$5D`, `$5F`, `$61`, `$63`) and eight effects (`$01`,
`$05`, `$06`, `$09`, `$0D`, `$0E`, `$35`, `$36`) actually read while running.
`xtask extract` runs each request through a traced native driver until its read
set is stable for 100 s, drops the table addresses and merges adjacent bytes
into segments. For Gradius (E) that is 3,033 sequence bytes in 7 segments; the
complete `KDRV` payload is 3,309 bytes and the audio chunk is 93,618 bytes
(14,940 events). No other cartridge bytes are copied. The four 17-byte channel
blocks at `$B0`, `$C1`, `$D2`, `$E3` are mutable runtime state (all zero at
request time), so the section records their base addresses and APU register
offsets rather than ROM contents.

`xtask extract` also writes provenance into `CHUNK_META`: `audio_format=GAUD`,
`audio_version`, `audio_tracks`, `audio_sfx`, `audio_events`,
`audio_driver_bytes`, `audio_driver_sequences`, `audio_sha256` (the SHA-256 of
the chunk payload) and `audio_source=oracle-capture $ECFC; driver=$EFC7/$EFB2;
sequences=12 music + 8 SFX`. Encoding is deterministic.

At runtime `gradius_audio::AudioBank::from_chunk` decodes the chunk:

```rust
let pack = gradius_assets::Pack::load(path)?;
let bank = gradius_audio::AudioBank::from_chunk(pack.audio.as_deref().unwrap())?;
if let Some(timeline) = bank.music(Music::Stage1) {
    engine.play_music(timeline);
}
if let Some(timeline) = bank.sfx(Sfx::Shot) {
    engine.play_sfx(timeline);
}

// The native Konami driver reads its tables and sequences from the pack.
if let Some(mut driver) = bank.driver() {
    let start_writes = driver.start_id(0x90);
    // ... call driver.tick(&mut writes) once per frame.
}
```

`music(..)` and `sfx(..)` return `None` for a track the chunk does not carry;
the caller plays silence in that case (the SDL frontend does this for sound
effects). A version 1 chunk (timelines only) or a version 2 chunk with an empty
driver section keeps working the same way; `bank.driver()` is simply `None`.
Track ids the reader does not know are skipped by their recorded length. The
decoded names and event arrays are leaked once into the `&'static` form that
`Engine` takes; this is the same lifetime a game asset pack has.

The test `crates/gradius-audio/tests/pack.rs::a_pack_only_run_plays_every_track_deterministically`
builds a pack, loads only `Pack::audio`, and checks that every one of the 12
music tracks and 8 effects renders the same way on each run.
`a_pack_decodes_all_twelve_distinct_music_tracks` additionally fails if any of
the twelve cues is missing, empty or has the same name/length/event hash as
another. `the_chunk_re_encodes_to_the_same_bytes` checks that the encoding is
deterministic.

### Driver validation from the pack

`crates/gradius-audio/tests/driver_pack.rs` builds the same chunk from the
cartridge when `GRADIUS_ROM` is set and compares the pack driver with a driver
over the cartridge bank for all twenty requests (zero write differences). It
also compares the writes with the oracle captures taken from the cartridge at
extraction time, frame by frame:
title `$90` 0/112 (after the captured 1 s real-game lead-in), stage 1 `$93`
0/2347, stage 1-B `$96` 0/2272, Big Core `$A5` 0/774, stage clear `$AC`
0/386, game over `$AF` 0/699, stages 2-7 `$59`/`$5B`/`$5D`/`$5F`/`$61`/`$63`
0/1499, 0/1350, 0/1217, 0/1124, 0/1912, 0/995, and every effect 0. Stage 4 is
included even though the assisted run never requests it, because the native
driver reads `$5D` straight from the cartridge bank. The former 21 stage-1-B
differences came from the `$FE` repeat counter; the native driver now mirrors
the cartridge's bit-7 comparison (and also models `$F0` tempo scaling and the
`$F0-$FC` period aliases).

### Stage 4-7 and the ending against the oracle

The stage 4-7 themes and the Big Core cue were checked against a fresh oracle
capture from the cartridge at extraction time. The native driver over the pack
reproduces every write the cartridge makes for these requests
(`driver_pack.rs`, 0 mismatches).

| cue | pack track (music id) | APU frames (lead-in + loop) | result |
| --- | --- | --- | --- |
| `$5D` | `stage4` (8) | 1249 (1 + 1248) | exact |
| `$5F` | `stage5` (9) | 1080 (0 + 1080) | exact |
| `$61` | `stage6` (10) | 2040 (0 + 2040) | exact |
| `$63` | `stage7` (11) | 1424 (1 + 1423) | exact |
| `$A5` | `boss` (3) | 463 (79 + 384) | exact |

Frames are `total_cycles / 29,780`, and each one is a whole frame. The ending
has no theme of its own: it plays the Big Core cue `$A5`. Stage 4 requests
`$5D` at the equivalent frame 31304, not on stage entry: `$1C` switches `$93`
to `$5D` when `$8354[3]` reaches 4 (see `docs/analysis/stage4_boot.md`).

### Bass and the triangle channel

Several captured tracks (title, stage 1 second half, Big Core, game over and
most stage themes) barely write bit 7 of `$4008`, the triangle linear-counter
reload. On a fresh APU the linear counter stays zero and the bass line is
silent; the cartridge only sounds because its sound engine keeps running
between cues. `Engine` now hands the previous music APU to the next music
voice, and `Player` supplies the reload bit for tracks whose capture leaves it
clear on most `$4008` writes. Tracks that set it on most notes (stage 1) keep
their exact articulation. The test
`engine::tests::every_music_track_has_an_audible_triangle_channel` renders all
twelve cues through one engine and fails if any triangle RMS is at or below
0.1; `tracks::tests::every_stage_theme_has_an_audible_triangle_bass` repeats
the check for the six stage themes.

## Frontend selection

`gradius-app::MusicDirector` derives a cue from public core state only;
`gradius-sdl/src/music.rs` maps that cue to a captured timeline:

* `State::Title` -> title fanfare;
* `State::Playing` on stage 1 and scroll `< $0400` -> stage 1;
* `State::Playing` on stage 1 and scroll `>=$0400` -> stage 1 second half;
* `State::Playing` on stage 2-7 -> that stage's `$8344` theme
  (`$59`/`$5B`/`$5D`/`$5F`/`$61`/`$63`) from the first stage-2 frame;
* `State::Playing` and scroll at the stage's `$9A30` trigger -> Big Core theme
  (stage 1 keeps its measured `2820`/`$B04` cut);
* a one-frame score jump of 1,000 while in the boss cue -> stage-clear fanfare;
* `State::GameOver` -> game-over theme (the clear fanfare covers the 240-frame
  cleared phase that precedes it, so a completed run still ends on game over);
* a new run or a stage advance resets to the new stage's theme.

`2820` mirrors the private `BOSS_TRIGGER` in `gradius-core` (`$B04`), and `$0400`
is where the original requests `$96`. The 1,000-point jump is the core's Big
Core reward (`$C776`). These are the only heuristics in the audio path; the
tracks themselves are exact. A cue change crossfades the old and new music over
10 ms in `gradius-audio::Engine`, so a switch at a non-zero point in the old
waveform neither clicks nor leaves a silent gap.

### Core hook that would replace the heuristics

The frontend needs no heuristics once the core exposes its stage sub-state:

```rust
// In gradius-core:
pub fn music_cue(&self) -> Option<MusicCue>; // Title / Stage1 / Stage1B / Stage2..7 / Boss / Clear / GameOver
pub fn stage_phase(&self) -> Phase;          // already private today
pub fn boss_alive(&self) -> bool;
```

The clean replacement is one `game.music_cue()` call per frame in
`MusicDirector::update`; until then the scroll thresholds and the score jump are
the only way to observe the boss and the clear from public data.

## What is exact and what is missing

Exact:

* title/attract fanfare;
* both stage-1 music halves, including their captured lead-ins and loops;
* all six per-stage second themes (`$59`, `$5B`, `$5D`, `$5F`, `$61`, `$63`);
* the Big Core theme;
* the stage-clear and game-over music;
* the eight sound effects listed above.

Missing or approximated:

* the continue/name-entry music and the stage 8 loop themes (`$94`, `$95`,
  ...); the ending has no theme of its own and plays `$A5`;
* the core exposes no music or boss hook, so the runner derives the boss and
  clear cues from scroll and score (see above). A future core change should add
  `Game::music_cue()` or expose the stage phase and live boss flag;
* the original driver multiplexes music and effects on the same four channels;
  the runtime mixer gives every voice its own APU. Simultaneous effects can
  therefore sound fuller than the cartridge and never steal a music channel;
* DMC is not synthesised (Gradius never enables it).

## Headless verification

```sh
cargo run -p gradius-sdl -- --pack assets/gradius.pak --frames 900 \
  --no-audio --dump-audio /tmp/opencode/x.wav
```

The dump is 900 frames / 60.0988 Hz = 14.9753 s, 718,816 mono samples at
48 kHz. With the title screen left alone the fanfare occupies the first 0.85 s
and the rest is correctly silent (peak 19.2 %, RMS 1.6 %, no clipped samples).
With `--autoplay` the same length contains stage-1 music and effects: peak
50.3 %, RMS 12.3 %, no clipped samples.

A 3,000-frame autoplay dump is 49.9178 s / 2,396,054 samples: title, the first
stage-1 half and the `$0400` second half all appear (peak 61.4 %, RMS 13.8 %,
no clipped samples). The Big Core and clear cues are later than that window:
scroll advances one pixel every two frames, so the boss entry at 2820 needs
about 5,800 frames of continuous play. `music::tests::
a_real_core_run_selects_every_cue_transition` drives a real `Game` through the
boss, clear, game over and restart; the 3,000-frame scripted render test covers
the same cue order in one WAV.

The dummy-backend smoke re-runs the 900-frame dump with
`SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy`: 14.9753 s, 718,816 samples,
`audio_events=153`, peak 81.9 %, RMS 16.9 %, no clipped samples. The stage-2
handoff itself is covered by `tests/natural_handoff.rs`, which replays the
recorded boss script through the real core and asserts the first stage-2 frame
selects `Stage2` (`$59`) instead of the stale clear fanfare.
