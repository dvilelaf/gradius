# Save states and rewind for the clean core

Design for capture, restore and rewind of `gradius-core` state, with the
frontend keeping rendering devices and files. The design was measured on
commit `2caba29` (2026-11-09) with a one-off `size_of` probe. It is now
implemented on `main` `069ed09`; the measured result is in §0, and the rest of
this document keeps the design that led there.

The core is already the right shape for this: all mutable simulation lives in
one `Game`, every value is an integer, there are no clocks, threads or global
random sources, and `render` is a pure function of the state plus `Assets`.
`Game` is 6,096 bytes in memory: 2,398 bytes of simulation fields (plus 2 bytes
of layout padding), 24 bytes of transient audio queue, 3,600 bytes of two-frame
OAM history, 48 bytes of `Assets` handles and the 24-byte `Framebuffer` handle. The RGBA buffer behind
that handle is 256×240×4 = 245,760 bytes and is excluded from every snapshot.

## 0. Implementation status (main `069ed09`)

All five cards (SS-A..SS-E) are **FIXED** on `main`:

* `Game::snapshot()` / `Game::restore()` live in
  `crates/gradius-core/src/snapshot.rs` over the `gradius_core::save` codec.
  The CORE chunk is **2,524 B**, the EXT chunk **41 B** (spawner
  cursor/pending and score-delay state) and the OAM chunk carries the
  two-frame sprite history plus its tail. A typical image is **~2.8 KB**
  inside the `SLOT_BYTES = 8_192` slot; the wasm smoke image is **2,989 B**.
* The file is `GRSS` **v1.0** and stays backward compatible: an older image
  without the EXT chunk or the OAM tail decodes with both empty and only pays
  the documented sprite lag.
* Determinism is exact: **600 -> 1,200**, **786 -> 1,386** with live objects,
  `--stage 5 --rewind-at 300` (400 frames) and `--stage 4 --rewind-at 300`
  all replay byte for byte. `a_snapshot_at_the_start_of_stage_five_replays_exactly`
  fails if the EXT chunk or the OAM tail is removed.
* SDL: F5 saves and F9 loads `savestate.grs` in the working directory, written
  to a sibling `.tmp` and renamed over the slot. A load snapshots the current
  game first and rolls back on a corrupt file, so the state stays
  byte-identical and the overlay shows `LOAD ERROR`.
* Wasm/PWA: `state_save`/`state_len`/`state_load` backed by
  `localStorage["gradius.savestate.v1"]`, with buttons and F5/F9. The smoke run
  saves **2,989 B**, replays **60/60** frames identically and refuses a
  corrupt or truncated image without moving the game or the rendered frame.
* **Audio caveat:** a load resets the audio engine and restarts the current
  music cue **from its beginning**. The APU position is not part of the saved
  state, so a restored run is state- and pixel-identical but its audio does not
  resume mid-cue.

## 1. What must be captured

A snapshot is taken at the **end of a frame**, after `Game::step` has called
`render`, and after the runner has drained `game.audio`. That boundary makes
the capture deterministic and leaves the audio queue empty.

### 1.1 `Game` fields

Sizes are in-memory bytes (Rust layout, may include padding). The "encoded"
column is the version-1 payload width; it differs only where a pointer,
`usize`, `Option` discriminant or padding is packed out. Fields are listed in
declaration order.

| field | type | bytes | snap? | encoded | notes |
| --- | --- | --- | --- | --- | --- |
| `rng` | `Rng(u32)` | 4 | yes | 4 | starfield respawn only, but stars respawn |
| `player` | `Player` | 67 | yes | 67 | §1.2 |
| `objects` | `[Object; 32]` | 704 | yes | 704 | only slots 0-9 are ever live (`$A3AB`) |
| `options` | `[OptionDrone; 4]` | 20 | yes | 20 | four port-only drones |
| `shots` | `[Shot; 15]` | 105 | yes | 105 | 5 emitters × 3 |
| `enemy_bullets` | `[EnemyBullet; 12]` | 240 | yes | 204 | 17 encoded bytes each |
| `enemy_bullet_count` | `usize` | 8 | yes | 4 | |
| `explosions` | `[Explosion; 24]` | 288 | yes | 240 | 10 encoded bytes each |
| `explosion_count` | `usize` | 8 | yes | 4 | |
| `stars` | `[Star; 96]` | 768 | yes | 672 | 7 encoded bytes each |
| `power` | `PowerMeter` | 7 | yes | 7 | meter, speed, missile, weapon, options, shield |
| `score_bcd` | `Bcd` | 3 | yes | 3 | `$07E4-$07E6` |
| `high_score_bcd` | `Bcd` | 3 | yes | 3 | persists across runs; rewinding rolls it back too |
| `scroll` | `Scroll` | 3 | yes | 3 | `$3D/$3E/$3F` |
| `spawner` | `Spawner` | 48 | yes | 25 | `stream` pointer is derived, see §3.3 |
| `phase` | `Phase` | 4 | yes | 3 | tag + u16 payload, §2.3 |
| `section` | `u8` | 1 | yes | 1 | `$19`, the **stage index** (0-6), not a sub-section |
| `stage_clock` | `u32` | 4 | yes | 4 | |
| `kills` | `u32` | 4 | yes | 4 | |
| `banner` | `u16` | 2 | yes | 2 | |
| `prev_fire` | `bool` | 1 | yes | mask | edge latch |
| `prev_power` | `bool` | 1 | yes | mask | edge latch |
| `prev_start` | `bool` | 1 | yes | mask | edge latch |
| `prev_up` | `bool` | 1 | yes | mask | edge latch |
| `prev_down` | `bool` | 1 | yes | mask | edge latch |
| `prev_select` | `bool` | 1 | yes | mask | edge latch |
| `debris_folds` | `u8` | 1 | yes | 1 | `$47` fold-in alternation |
| `title_timer` | `u16` | 2 | yes | 2 | attract entry at 133 |
| `attract_countdown` | `u16` | 2 | yes | 2 | `$4C/$4D` |
| `volcano_kills` | `u8` | 1 | yes | 1 | `$5F` |
| `stage_clear_ready` | `bool` | 1 | yes | 1 | `$39` |
| `demo` | `bool` | 1 | yes | 1 | `$09` |
| `demo_index` | `u16` | 2 | yes | 2 | `$31` |
| `demo_ticks` | `u8` | 1 | yes | 1 | `$30` |
| `demo_button` | `u8` | 1 | yes | 1 | current demo `$07` |
| `menu_selection` | `u8` | 1 | yes | 1 | 0 = 1P, 1 = 2P |
| `menu_blink` | `u8` | 1 | yes | 1 | title cursor blink |
| `scroll_stop` | `bool` | 1 | yes | 1 | `$5B` |
| `continue_pending` | `u8` | 1 | yes | 1 | |
| `scroll_hold` | `u8` | 1 | yes | 1 | `$0D` |
| `entry_blank` | `u8` | 1 | yes | 1 | PPUMASK blank frames |
| `latched_input` | `Buttons` | 8 | yes | 1 | push a frame late; mandatory |
| `boss_spawned` | `bool` | 1 | yes | 1 | |
| `dev_keep_alive` | `bool` | 1 | yes | 1 | changes collision outcomes |
| `mods` | `Mods` | 1 | yes | 1 | changes `activate`; keep with the run |
| `boss_stage` | `u8` | 1 | yes | 1 | `$1B = $81..$84` |
| `boss_timer` | `u16` | 2 | yes | 2 | |
| `hazard_index` | `u8` | 1 | yes | 1 | `$69` |
| `boss_clear` | `u16` | 2 | yes | 2 | |
| `scroll_locked` | `bool` | 1 | yes | 1 | |
| `game_over_timer` | `u16` | 2 | yes | 2 | |
| `death_checkpoint` | `u8` | 1 | yes | 1 | `$24` captured at death |
| `death_burst` | `Option<DeathBurst>` | 16 | yes | 13 | tag + 12 |
| `score_delay` | `ScoreDelay` | 32 | yes | 13 | `pattern` pointer replaced by an id |
| `state` | `State` | 1 | yes | 1 | Title/Playing/GameOver |
| `frame` | `u64` | 8 | yes | 8 | drives blast/title animation, must not reset |
| `score` | `u32` | 4 | yes | 4 | decimal mirror of `score_bcd` |
| `lives` | `u8` | 1 | yes | 1 | |
| `audio` | `AudioQueue` | 24 | **no** | — | transient, §1.3 |
| `assets` | `Assets` | 48 | **no** | — | pack data, §1.3 |
| `framebuffer` | `Framebuffer` | 24 | **no** | — | derived pixels |
| `last_sprites` | `SpriteList` | 1,800 | **no** | — | recomputable |
| `earlier_sprites` | `SpriteList` | 1,800 | **yes** | 1,473 max | OAM chunk, §1.3 |

Simulation fields sum to 2,398 bytes; packed version-1 core payload is
**2,152 bytes**. Stage 1 has no separate stage field today because `section`
already is `$19`.

### 1.2 Nested structs

| struct | layout | bytes |
| --- | --- | --- |
| `Fixed` | `int: u8, frac: u8` | 2 |
| `Bcd` | `digits: [u8; 3]`, most significant first | 3 |
| `Trail` | `x: [u8; 24], y: [u8; 24], index: u8` | 49 |
| `OptionDrone` | `active, x, y, sprite, counter` (all `u8`/`bool`) | 5 |
| `Player` | `x, y: Fixed`; `dying`; `sprite_frame`; `pending_frame`; `anim_timer`; `cooldown1: [u8; 5]`; `cooldown2: [u8; 5]`; `trail: Trail` | 67 |
| `Object` | `active, kind, subtype, x, y, x_vel, y_vel, ai_state, ai_timer, ai_mode, timer2, anim_div, anim, sprite, size_class, phase, terrain, boss` | 22 |
| `Shot` | `active, kind, x, y, age` | 7 |
| `EnemyBullet` | `x, y, vx, vy: i32`; `kind: u8` | 20 (17 encoded) |
| `Explosion` | `x, y: i32`; `life, max_life: u8` | 12 (10 encoded) |
| `Star` | `x: i32, y: i16, layer: u8` | 8 (7 encoded) |
| `Pending` | `remaining, timer, interval, y, y_step, kind, subtype, substate, x, motion, phase` | 11 |
| `Spawner` | `section, column_base, cursor: usize, stream: &'static [Record], pending: Option<Pending>, spawn_count, cannon, class_counts: [u8; 4], init_pending` | 48 (25 encoded) |
| `Phase` | 11 variants, timers up to `u16` | 4 (3 encoded) |
| `DeathBurst` | `x, y: i32`; `age: u16` | 12 (`Option` 16, encoded 13) |
| `ScoreDelay` | `shown: [u8; 3], pending: [u8; 3], delay, pattern: &'static [u8], index: usize, countdown` | 32 (13 encoded) |
| `PowerMeter` | `selection, capsules, speed_level, missile, weapon_level, options, shield` | 7 |
| `Scroll` | `fraction, low, high` | 3 |
| `Mods` | `extra_options: bool` | 1 |
| `Buttons` | eight `bool` flags | 8 (1 encoded bitmask) |

### 1.3 Derived, transient and static

| field | why excluded | recovery |
| --- | --- | --- |
| `assets` | the pack is immutable input, not game state | required at `Game::new`; disk saves carry an asset fingerprint |
| `framebuffer` | 245,760 derived RGBA bytes | `render` after restore |
| `last_sprites` | built by the current `step`; a pure function of the state | `build_sprites()` on restore |
| `earlier_sprites` | built two frames ago; **not** recomputable from the current state | stored in the OAM chunk |
| `audio.events` | drained by the runner right after `step`; replaying old sounds is wrong | cleared on restore |
| `spawner.stream` | `&'static` slice selected by `section`/`column_base` | recomputed on restore |
| `score_delay.pattern` | always `&[5, 4, 3, 8]` for a real run | pattern id in the payload |

`earlier_sprites` must be stored even though it is "derived". The renderer
draws the OAM list built two frames earlier and rotates two lists per frame; a
restore that starts both from `build_sprites()` makes every later frame draw
one frame ahead of the original timeline, permanently. Keeping one list keeps
frames `N+1` onward bit-identical. The list is at most 64 × 23 = 1,473 encoded
bytes.

`twoplayer::TwoPlayer` is not owned by `Game` today (the module is not wired
into the runtime), so it is out of scope. If it moves into `Game`, it gets its
own chunk (§2.4).

## 2. Snapshot format

### 2.1 Why binary

A text format would be easier to inspect but needs escaping, complete coverage
of 32 objects and 15 shots (the existing `dump_state` prints only live slots
and view fields, so it is not a save format), and a parser that cannot panic.
Binary with fixed little-endian widths is smaller, exact, allocation-free to
decode, and covered by a golden-bytes test. A `--dump-save` text view can be
added for humans without making text canonical.

Rules:

* explicit `to_le_bytes`/`from_le_bytes`; no `memcpy` of a struct, so endianness
  and Rust layout cannot leak into the file;
* no pointers, `usize` (write `u32`), `Vec`, `String`, float or platform type;
* booleans are `0`/`1`, enums are explicit tags, `Option` is tag `0/1` plus the
  payload (fixed width, even when absent);
* decode validates every tag, range and length before touching `Game`; a bad
  snapshot leaves the game unchanged;
* no allocation during capture or restore; the ring is preallocated (§3.1).

### 2.2 Header and chunks

56-byte header, then TLV chunks (`id: u16`, `len: u16`, payload):

| offset | size | field |
| --- | --- | --- |
| 0 | 4 | magic `GRSS` |
| 4 | 2 | version major (1) |
| 6 | 2 | version minor (0) |
| 8 | 4 | flags: bit 0 OAM present, bit 1 assets fingerprint valid, bits 2-31 reserved 0 |
| 12 | 2 | header length (56) |
| 14 | 2 | chunk count |
| 16 | 4 | payload length (bytes after the header) |
| 20 | 8 | `frame` at capture (debug and UI) |
| 28 | 8 | `Game::state_hash()` at capture (debug, not an integrity check) |
| 36 | 8 | asset fingerprint (`u64` FNV-1a over tiles+colors; 0 in the ring) |
| 44 | 8 | checksum: FNV-1a 64 over bytes 52.. and all chunk bytes |
| 52 | 4 | reserved 0 |

| id | chunk | version | contents |
| --- | --- | --- | --- |
| 0x0001 | CORE | 1.0 | §1.1 fields in declaration order, packed widths |
| 0x0002 | OAM | 1.0 | `count: u8` then `count` × `x i32, y i32, tile u8 bank, tile u8 index, palette [u8; 12], flags u8` |
| 0x0003 | STAGE_EXT | reserved | stage script cursor/timer, `$28` loop counter (§2.4) |
| 0x0004 | PLAYERS_EXT | reserved | two-player slots/checkpoint if they become `Game` state |
| 0x0005 | CELLS | 1.0 | stage-3 dynamic background cells |
| 0x0006 | COLLISION | 1.0 | stage collision-map patches |
| 0x0007 | EXT | 1.0 | **41 B**: spawner cursor/pending/timers/counts, score-delay state and ending replay step |
| 0x8000.. | debug | — | ignored by the game loader |

Unknown chunk ids are skipped by `len`, so a newer writer's optional chunks do
not break an older reader. Header `flags` bit 0 says whether OAM follows;
without it the reader clears `earlier_sprites` and accepts one-frame sprite
lag.

### 2.3 Tag assignments

`State`: 0 Title, 1 Playing, 2 GameOver.

`Phase` (payload always one `u16`, so a variant change cannot change the
width): 0 Attract, 1 DemoStart, 2 Continue, 3 StageInit, 4 StageIntro,
5 Blank, 6 Init, 7 Normal, 8 Transition, 9 Dying, 10 Cleared.

`Buttons` bitmask: bit 0 fire, 1 power, 2 up, 3 down, 4 left, 5 right,
6 start, 7 select. The six edge latches use a second byte: bit 0 fire,
1 power, 2 start, 3 up, 4 down, 5 select; bits 6-7 zero.

`ScoreDelay.pattern_id`: 0 = `[5, 4, 3, 8]`, the only pattern a real run uses;
1-255 reserved. `Mods` is one byte, bit 0 `extra_options`.

### 2.4 Keeping stages 2-7 stable

* Stage selection already lives in `section` (`$19`). Stages 2-7 add decoded
  tables, not new `Game` fields, so v1 core stays valid; the stage-specific
  background, music id, boss trigger and quirks come from
  `stage_catalog::stage_info(section)` at render/step time.
* New mutable per-stage data (the `$C433` script cursor for stages 3-7, the
  `$28` loop counter for stage 7, stage-2 tail mode if it needs a latch) goes
  into `STAGE_EXT`, chunk id `0x0003`, introduced as version 1.1. A v1.0
  reader skips it and behaves like stage 1; a v1.1 reader treats a missing
  chunk as all-zero defaults. This is the only compatibility promise: chunk
  layout never changes in place, a breaking core change bumps the major.
* Data identity matters more than code identity: a disk save from a different
  pack or a build with different stage tables may spawn different objects even
  with identical 2,152 bytes. Store the asset fingerprint and the stage; warn
  and refuse a mismatched pack by default, with an explicit `--force-load`.
* A golden test pins the v1 CORE bytes so an accidental field reorder fails.

### 2.5 Size (measured on `main`)

| item | measured |
| --- | --- |
| header | 56 |
| CORE chunk + framing | 2,524 + 4 |
| EXT chunk + framing | 41 + 4 |
| OAM chunk + framing | the two-frame sprite history and tail (a few hundred bytes) |
| **whole snapshot** | **~2.8 KB typical; 2,989 B in the wasm smoke** |

A ring slot is a fixed **8,192 bytes** (`SLOT_BYTES`), the measured image plus
headroom for the per-stage data. If `STAGE_EXT` for stages 3-7 grows beyond
that, raise `MAX_SNAPSHOT_BYTES` and `SLOT_BYTES` together; do not silently
truncate. A quick-save file is trimmed to the used length (~2.8 KB typical).

## 3. Rewind

### 3.1 Capture and the ring

* Capture point: `Runner::step`, in this order — `game.step(input)`, drain
  `game.audio` into the sink, `rewind.capture(&game)`. Headless runs keep the
  ring disabled unless asked (`Runner::enable_rewind(n)`), so `gradius-diff`
  and the 10k-frame tests pay nothing.
* Ring: `SLOT_BYTES × N` `Box<[u8]>` allocated up front; a `head`, `len` and
  `cursor` index. No allocation, growth or filesystem access in the frame loop.
* Default `N = 600` frames (9.98 s at 60.0988 Hz), per-frontend configurable.

| N (frames) | seconds | memory |
| --- | --- | --- |
| 180 | 3.0 | 0.70 MiB |
| 300 | 5.0 | 1.17 MiB |
| 600 | 10.0 | **2.34 MiB** |
| 1200 | 20.0 | 4.69 MiB |

Capture cost is one 2.2 KB serialise plus at most 1.5 KB of OAM per frame; at
60 Hz that is under 0.25 MB/s, far below the renderer. Restore cost is one
decode plus one redraw, and happens only on a rewind gesture.

### 3.2 Semantics

* `rewind(k)` restores the k-th newest snapshot and **truncates** the frames
  newer than it. Rewinding therefore branches: new input from that point
  overwrites the old future. This avoids pretending the discarded inputs can
  be replayed.
* Rewinding past the oldest snapshot clamps to the oldest.
* Entering rewind pauses the runner. Releasing the key stays paused so the
  restored frame can be inspected; the existing Pause key resumes.
* Snapshot/restore are also exposed as a single-frame API (no ring) for
  tools, tests and future netplay rollback.

### 3.3 Restore algorithm

1. Validate header, version, checksum, chunk ids/lengths and the CORE fields.
   Decode into locals; do not mutate `Game` until all checks pass.
2. Write all `Game` fields from the CORE chunk.
3. Recompute `spawner.stream = SECTIONS[section][column_base >> 1]`; reject
   `section >= 7`, a non-even `column_base`, a column past the row, or
   `cursor > stream.len()`. Recompute `score_delay.pattern` from `pattern_id`.
4. Clear `audio.events` (a snapshot never carries pending sounds).
5. `earlier_sprites` from the OAM chunk (or clear it); then
   `last_sprites = build_sprites()`.
6. Redraw the framebuffer. `render` currently advances `ScoreDelay` and rotates
   OAM; split it into a draw pass taking the shown digits and the sprite list.
   Restore calls the draw pass with `score_delay.shown` and the restored
   `earlier_sprites`, without advancing the delay or the rotation. This makes
   the presented picture immediate and keeps the delay counter exact.

The picture drawn after a restore is the OAM list from `N-1` (the closest one
the snapshot can know), while the frame originally shown at `N` used `N-2`.
The difference is invisible in practice, and frame `N+1` onward is
bit-identical. If a frontend ever needs the exact frame-`N` picture (for
example, a thumbnail), add a `SHOWN` chunk with the list actually drawn; it is
another 1.5 KB and does not affect simulation.

### 3.4 Audio

Audio devices are frontend state, and their sample position cannot be rewound.

* Before the first restore of a gesture: discard `game.audio` contents (the
  runner already drained them), call a new `AudioBackend::reset()` that uses
  `gradius-audio::Engine::reset()` and drops all voices, and stop the music.
* After restore: `MusicDirector::resync(&game)` sets `previous_state`,
  `previous_score` and `cue` from the restored `Game` **without** emitting a
  change, and returns the cue; the SDL/wasm backend restarts that cue from its
  beginning. Music position restarts; it is not continuous across a rewind.
* Repeated restores during a held rewind share one reset; resync only on
  release (or when the cue actually changes) to avoid clicks.
* No sound is pushed by the restore or redraw path.

### 3.5 Pixel framebuffer and UI

The framebuffer is never snapshotted. 600 × 245,760 bytes would be 140 MiB,
and the buffer is already rebuilt by the core redraw. The frontend keeps
presenting `game.framebuffer()` unchanged, so no SDL/wasm texture path changes.
Screenshot (F12) and the paused overlay already copy the core buffer and are
unaffected.

Optional scrubbing thumbnails: every 30th snapshot can carry a 64×60 image
(15,360 B RGBA, or 3,840 B indexed + 768 B palette) for a timeline scrubber.
None of that is in the budget above and it is out of scope for v1.

Keys (reserved in `keybinds.rs`, documented in `DEVELOPMENT.md` and the
`keybinds.ini` template):

| key | action |
| --- | --- |
| `F4` (hold) | rewind one snapshot per rendered frame; overlay shows `REWIND 3.2s` |
| `F5` | quick save to `gradius.quick.sav` next to the pack |
| `F9` | load the quick save (validated; rejected snapshots are ignored with a notice) |
| `Shift` + `F4` | rewind 10 snapshots per rendered frame |
| existing `Pause` | resume after a rewind |

`F4`, `F5` and `F9` join the reserved scancode list so player bindings cannot
collide; the title gains a ` - rewound`/` - paused` state like the current
pause marker.

### 3.6 Disk save states

Same bytes as the ring slot, trimmed, written atomically (temp file then
rename) by the app frontend; the core never opens a file. The header adds the
asset fingerprint and the frame number so a load can explain a mismatch. The
format is versioned and rejected wholesale on a bad magic, major version,
checksum, chunk length, enum tag or range; no partial loads. Saves contain no
ROM bytes, only derived state.

## 4. Determinism contract and tests

The contract: with the same `Assets`, restoring a snapshot at frame `N` and
stepping the same inputs reproduces the exact `Game` bytes (including OAM) for
every frame `>= N+1`. `state_hash()` equality starts at `N+1`; at `N` itself
the redraw uses the `N-1` OAM list by design.

Tests, all through public APIs:

1. **Round trip**: run 500 frames of scripted input that reaches play, spawns
   enemies, fires, dies and re-enters; save; run another 1,000 frames; restore;
   step the same inputs and compare `save_into` bytes for each of the next 300
   frames with the original run's bytes.
2. **Golden bytes**: a fixed 300-frame run produces a pinned FNV-1a of the
   v1 CORE payload (2,152 bytes) and of the whole slot. Catches field reorder,
   width and endianness drift.
3. **Corruption**: truncated, bad magic, wrong major, bad checksum, unknown
   chunk id (must be skipped), bad enum tag, `section = 7`, `cursor` past the
   stream. `Game` must be byte-identical after every rejection.
4. **OAM history**: restoring and stepping gives an OAM chunk equal to the
   original; a snapshot with the OAM flag clear still simulates identically
   but is documented as one frame of sprite lag.
5. **Ring**: wrap at capacity, clamp at the oldest, truncation on resume, and
   disabled-by-default in `Runner::run`.
6. **Stage extension**: a v1.1 snapshot with an unknown `STAGE_EXT` decodes to
   all-zero stage extension and plays stage 1; a v1.0 snapshot with no
   `STAGE_EXT` does the same.
7. **No allocation**: code review plus a test allocator on the app side; the
   core codec only indexes fixed slices and uses `get`/`Result`, never panic.

## 5. Card breakdown

| id | card | files | deliverable | deps | h |
| --- | --- | --- | --- | --- | --- |
| SS-01 | Core snapshot codec v1 | `gradius-core/src/snapshot.rs` (new), `game.rs` (`save_into`), `lib.rs` | 56-byte header, CORE chunk, packed widths, validation, golden core bytes, round-trip of every field | — | 6 |
| SS-02 | Restore, OAM and redraw | `game.rs` (restore, `build_sprites` use, render split), `video.rs` if needed | field restore, spawner stream/pattern recompute, OAM chunk, draw without advancing `ScoreDelay`, frames `N+1..` byte-identical | SS-01 | 4 |
| SS-03 | App rewind ring | `gradius-app/src/rewind.rs` (new), `runner.rs` | `SLOT_BYTES × N` preallocated ring, capture after step+drain, `rewind(k)`, truncate, enable/disable, capacity config | SS-01, SS-02 | 5 |
| SS-04 | Audio and music resync | `gradius-app/src/music.rs`, `gradius-sdl/src/audio.rs` | `Engine::reset` wrapper, `MusicDirector::resync`, no stale voice/cue after rewind | SS-03 | 2 |
| SS-05 | SDL keys, pause and overlay | `gradius-sdl/src/main.rs`, `keybinds.rs`, `overlay.rs`, `DEVELOPMENT.md` | hold-to-rewind, F5/F9 quick save/load wiring, reserved keys, `REWIND 3.2s` overlay, title state | SS-03, SS-04 | 3 |
| SS-06 | Disk save files | `gradius-app/src/rewind.rs` (or `savestate.rs`), `gradius-sdl` | asset fingerprint, atomic write/read, trim, version/range rejection, mismatch warning | SS-01, SS-03 | 3 |
| SS-07 | Determinism and compatibility tests | core/app tests | tests 1-7 above, `STAGE_EXT` placeholder, big-endian-safe golden test | SS-01..SS-06 | 4 |

Total **27 h**. Suggested order: SS-01, SS-02, SS-03, then SS-04/SS-05/SS-06 in
parallel, SS-07 last. SS-01 and SS-02 touch only `gradius-core`; SS-03 onward
are app/frontend and can land while stage cards touch `game.rs`, provided
`save_into`/`restore` are kept at the end of the file to minimise conflicts.

Out of scope for this batch: thumbnails/scrubbing, netplay rollback, APU
position continuity, and rewind in the differential harness.

## 6. Risks and open decisions

* **Snapshot boundary vs. `render`**: `render` mutates `ScoreDelay`, so the
  restore redraw must call a split draw pass. This is the only invasive core
  change; SS-02 owns it.
* **`earlier_sprites` is genuinely needed** for future-frame parity; omitting
  it is not just cosmetic for `state_hash`/pixel tests, it shifts the OAM
  timeline by one frame forever.
* **High score rewinds** with everything else. If the product wants a
  monotonic high score, keep it in the frontend and merge on load; that is a
  UI decision, not a format one.
* **Disk saves across packs/builds** are not guaranteed despite the
  fingerprint; only stage-1 parity is promised in v1.
* **Memory on wasm**: 2.34 MiB for N = 600 may be too much on 32-bit; set
  `N = 300` there via the same constructor argument.

## 7. Implementation plan for the current tree (2026-11-11)

Sections 1-6 are the design. This section is the dispatch plan: the exact
inventory of the tree that the cards will touch, the corrections §1 needs
before anyone writes code, and five cards of roughly one agent-hour each.
§7.1 replaces the field table of §1.1 because `Game` has grown since the
`2caba29` measurement; everything else in §2-§4 stands unless §7.2 says
otherwise.

### 7.1 Exact inventory

`Game` now has **76 fields**: 4 are not snapshotted (`assets`, `framebuffer`,
`last_sprites`, `audio`), **72 are**. Of those, 71 go in the CORE chunk and
`earlier_sprites` goes in OAM. The pre-EXT payload was **2,308 bytes**; the
implemented CORE is **2,524 B** (§0), and the table below is the inventory at
the time it was written.

Top-level CORE fields, declaration order. Widths are the v1.0 encoded width,
not the in-memory size; pointers and `usize` are packed out as noted.

| field | type | enc. B |
| --- | --- | --- |
| `rng` | `Rng(u32)` | 4 |
| `player` | `Player` | 67 |
| `objects` | `[Object; 32]` | 864 |
| `options` | `[OptionDrone; 4]` | 20 |
| `shots` | `[Shot; 15]` | 105 |
| `enemy_bullets` | `[EnemyBullet; 10]` | 180 |
| `enemy_bullet_count` | `usize` → `u32` | 4 |
| `explosions` | `[Explosion; 24]` | 240 |
| `explosion_count` | `usize` → `u32` | 4 |
| `stars` | `[Star; 96]` | 672 |
| `power` | `PowerMeter` | 7 |
| `score_bcd` | `Bcd` | 3 |
| `high_score_bcd` | `Bcd` | 3 |
| `scroll` | `Scroll` | 3 |
| `scroll_lag` | `i32` | 4 |
| `spawner` | `Spawner` | 27 |
| `phase` | `Phase` | 3 |
| `section` | `u8` | 1 |
| `music` | `u8` | 1 |
| `stage_clock` | `u32` | 4 |
| `kills` | `u32` | 4 |
| `banner` | `u16` | 2 |
| `prev_fire` | `bool` | 1 |
| `prev_start` | `bool` | 1 |
| `debris_folds` | `u8` | 1 |
| `title_timer` | `u16` | 2 |
| `title_attract_at` | `u16` | 2 |
| `attract_countdown` | `u16` | 2 |
| `volcano_kills` | `u8` | 1 |
| `stage_clear_ready` | `bool` | 1 |
| `cells` | `Vec<Cell>` | CELLS chunk |
| `collision_cells` | `Vec<CollisionPatch>` | COLL. chunk |
| `vram_queue_len` | `u8` | 1 |
| `spawn_busy` | `bool` | 1 |
| `demo` | `bool` | 1 |
| `demo_index` | `u16` | 2 |
| `demo_ticks` | `u8` | 1 |
| `demo_button` | `u8` | 1 |
| `prev_select` | `bool` | 1 |
| `menu_selection` | `u8` | 1 |
| `players` | `Players` | 1 |
| `menu_blink` | `u8` | 1 |
| `menu_select_pending` | `u8` | 1 |
| `scroll_stop` | `bool` | 1 |
| `continue_pending` | `u8` | 1 |
| `scroll_hold` | `u8` | 1 |
| `entry_blank` | `u8` | 1 |
| `latched_input` | `Buttons` | 1 bitmask |
| `boss_spawned` | `bool` | 1 |
| `dev_keep_alive` | `bool` | 1 |
| `mods` | `Mods` | 1 |
| `boss_stage` | `u8` | 1 |
| `boss_timer` | `u16` | 2 |
| `hazard_index` | `u8` | 1 |
| `script_counter` | `u8` | 1 |
| `boss_clear` | `u16` | 2 |
| `boss_clear_armed` | `bool` | 1 |
| `clear` | `Clear` | 1 |
| `bonus_wave` | `bool` | 1 |
| `scroll_locked` | `bool` | 1 |
| `game_over_timer` | `u16` | 2 |
| `title_pending` | `bool` | 1 |
| `death_checkpoint` | `u8` | 1 |
| `death_burst` | `Option<DeathBurst>` | 11 |
| `score_delay` | `ScoreDelay` | 13 |
| `state` | `State` | 1 |
| `frame` | `u64` | 8 |
| `nmi_frame` | `u8` | 1 |
| `nmi_lag` | `bool` | 1 |
| `score` | `u32` | 4 |
| `lives` | `u8` | 1 |
| **CORE** | | **2,308** |

Nested widths and member order:

| struct | members, in encoded order | enc. B |
| --- | --- | --- |
| `Fixed` | `int`, `frac` | 2 |
| `Bcd` | `digits[3]`, most significant first | 3 |
| `Trail` | `x[24]`, `y[24]`, `index` | 49 |
| `OptionDrone` | `active`, `x`, `y`, `sprite`, `counter` | 5 |
| `Player` | `x`, `y`, `dying`, `sprite_frame`, `pending_frame`, `anim_timer`, `cooldown1[5]`, `cooldown2[5]`, `trail` | 67 |
| `Object` | `active`, `kind`, `subtype`, `x`, `y`, `x_vel`, `y_vel`, `nametable_addr`, `ai_state`, `ai_timer`, `ai_mode`, `timer2`, `anim_div`, `anim`, `sprite`, `fire_timer`, `fire_motion`, `muzzle`, `size_class`, `phase`, `terrain`, `boss` | 27 |
| `Shot` | `active`, `kind`, `x`, `y`, `age` | 7 |
| `EnemyBullet` | `x`, `y`, `vx`, `vy` (`i32`), `kind`, `sprite` | 18 |
| `Explosion` | `x`, `y` (`i32`), `life`, `max_life` | 10 |
| `Star` | `x` (`i32`), `y` (`i16`), `layer` | 7 |
| `Pending` | `remaining`, `interval`, `y`, `y_step`, `kind`, `subtype`, `substate`, `x`, `motion`, `phase`, `attempted` | 11 |
| `Spawner` | `section`, `column_base`, `cursor` (`u32`), `pending` tag + Pending, `timer`, `spawn_count`, `cannon`, `class_counts[4]`, `init_pending`, `record_read` | 27 |
| `Phase` | tag + `u16` payload (always present) | 3 |
| `DeathBurst` | `x`, `y` (`i32`), `age` (`u16`); `Option` tag is 1 B | 10 |
| `ScoreDelay` | `shown[3]`, `pending[3]`, `delay`, `pattern_id`, `index` (`u32`), `countdown` | 13 |
| `PowerMeter` | `selection`, `capsules`, `speed_level`, `missile`, `weapon_level`, `options`, `shield` | 7 |
| `Scroll` | `fraction`, `low`, `high` | 3 |
| `Mods` | `extra_options` bit 0 | 1 |
| `Buttons` | 8 flags, bitmask | 1 |
| `Cell` | `world_col` (`u16`), `row`, `tile` | 4 |
| `CollisionPatch` | `addr` (`u16`), `value` | 3 |
| `Sprite` | `x`, `y` (`i32`), `tile.bank`, `tile.index`, `palette[3][4]`, `flip_h`, `flip_v`, `behind` | 23 |
| `SpriteList` | `len` (`u8`), `slotted` (`u8`), then `len` × (`slot` + `Sprite`) | 2 + 24·len |

Not snapshotted: `assets` (immutable pack input), `framebuffer` (245,760
derived RGBA), `last_sprites` (recomputed by `build_sprites()`), `audio`
(drained by the runner at the capture point), `spawner.stream` (recomputed
from `section`/`column_base`), `score_delay.pattern` (a `pattern_id`). The
`earlier_sprites` list is in OAM: 24 bytes per entry because `SpriteList`
also carries `slots[64]` and `slotted`, which `row_masks()` needs for the
eight-sprites-per-scanline order.

### 7.2 Corrections to §1/§2 before coding

* CORE was 2,308 B at this point, not 2,152 (the implemented payload is
  2,524 B, §0). §1.1 predates `scroll_lag`, `music`,
  `title_attract_at`, `players`, `menu_select_pending`, `script_counter`,
  `boss_clear_armed`, `clear`, `bonus_wave`, `title_pending`, `nmi_frame`,
  `nmi_lag`, `cells`, `collision_cells`, `vram_queue_len`, `spawn_busy`,
  `Object.nametable_addr/fire_timer/fire_motion/muzzle`,
  `EnemyBullet.sprite`, `Spawner.timer/record_read` and `Pending.attempted`.
  `prev_up`, `prev_down` and `prev_power` in §1.1 do not exist; only
  `prev_fire`, `prev_start` and `prev_select` do.
* `MAX_ENEMY_BULLETS` is 10, not 12; `Object` is 27 B (864 total), not 22.
* OAM must store `slots` and `slotted`. Without them a restored frame sorts
  the OAM ring by insertion order and the scanline sprite limit flickers
  differently from frame `N+1` on. Worst case is 1,538 B, not 1,473.
* `cells` and `collision_cells` are `Vec`s and break the fixed-slot
  assumption. Bound them: `MAX_CELLS = 512` (2,048 B payload) and
  `MAX_COLLISION_PATCHES = 256` (768 B). `collision_cells` is currently dead
  (`#[allow(dead_code)]`, never read) but is still state; capture it for
  forward compatibility. Both live in their own chunks, not CORE.
* With those chunks the worst case is 6,738 B, so `SLOT_BYTES` becomes
  **8,192**, not 4,096. N = 600 costs 4.7 MiB; N = 300 on wasm costs 2.3 MiB.
  Typical stage-1 snapshots stay near 2.8 KB (the wasm smoke image is
  2,989 B).
* `ScoreDelay.index` is `usize` and only ever grows; encode it as `u32`.
* Checksum coverage in §2.2 misses bytes 0-43. Compute FNV-1a 64 over the
  whole header except the checksum field itself (bytes 0-43 and 52-55) and
  over every chunk, so a flipped version byte is caught even before the
  version check.
* The codec cannot live in a sibling module of `game`: the fields are
  private. Put it in `src/game/snapshot.rs`, declared as `mod snapshot;` at
  the end of `game.rs`, so it is `game::snapshot` and can see the fields
  without widening visibility.

### 7.3 Format

Keep §2.2 as written, with the corrections above: 56-byte header, magic
`GRSS`, major 1 / minor 0, TLV chunks `CORE` (0x0001), `OAM` (0x0002),
`CELLS` (0x0005), `COLLISION` (0x0006); unknown ids skipped by `len`;
offsets 44-51 hold FNV-1a 64 and are zero while hashing.

* **Endianness: little-endian, field by field.** The NES data and every
  `frame_hash`/`state_hash` are little-endian, all writers use
  `to_le_bytes`, and the golden-bytes test pins the bytes, so a big-endian
  host would fail loudly instead of silently swapping. No `memcpy`, no
  `repr(C)`, no `cfg(target_endian)`.
* **Checksum: FNV-1a 64.** The crate already ships `FNV_PRIME` and uses it
  in `Framebuffer::frame_hash` and `Game::state_hash`, so saves and the
  differential harness share one primitive and there is no dependency. CRC32
  catches the same accidental corruption but needs a table or a crate;
  xxHash/SipHash are faster than 2-3 KB/frame needs. It is a corruption
  check, not authentication.
* **Binary TLV, not text or a flat struct.** Text needs escaping and a
  complete field dump (`dump_state` prints only live slots), while a flat
  struct fixes the layout forever. TLV lets a 1.1 writer add
  `STAGE_EXT`/`CELLS` without breaking a 1.0 reader, and decode is
  fixed-width indexing with `get()`, so a bad file returns an error instead
  of panicking.
* Versions: CORE layout never changes inside major 1. A breaking reorder
  bumps the major; new data gets a new chunk id. Disk saves carry the asset
  fingerprint; a mismatch refuses to load unless `--force-load`.

### 7.4 Cards (~1 h each, 5 total)

Sizes were optimistic agent-hours; A and B were the risky pair. **All five
cards landed on `main`**; §0 has the measured acceptance. A and B map to
§5's SS-01/SS-02, C extends SS-07, D is SS-05; E is new (§5 left wasm out).
SS-03/04/06 (ring, music, disk chunks) stay as future cards and are not
needed for the first usable save/load.

| id | card | files | measurable acceptance | deps |
| --- | --- | --- | --- | --- |
| SS-A | **Struct + pure codec + tests.** `SnapshotError`, writer/reader over `&[u8]`, `CORE_LEN = 2,308`, chunk framing, tag tables, golden bytes. No `Game` methods yet. | `core/src/game/snapshot.rs` (new), `mod snapshot;` in `game.rs`, `lib.rs` if it must re-export the error | `cargo test -p gradius-core snapshot` passes. A test asserts `CORE_LEN == 2_308` and that a write fits a 2,312 B buffer. A pinned FNV-1a of a 300-frame CORE payload matches the committed constant. Truncating the buffer at every offset returns `Err` and writes no byte past the end. Bad magic, major, checksum and each enum tag return `Err`; an unknown chunk id is skipped. Codec has no `Vec`, `String`, `unsafe` or allocation. | — |
| SS-B | **`Game::snapshot` / `restore` + round trip.** `save_into(&mut [u8]) -> Result<usize>` and `restore(&mut Game, &[u8]) -> Result<(), _>`; recompute `spawner.stream`, `score_delay.pattern`, `last_sprites`; clear `audio`; OAM chunk; draw pass that does not advance `ScoreDelay` or rotate lists. | `game/snapshot.rs`, `game.rs` (`restore`, render split), `video.rs` (a `draw`-only entry, if needed) | A `Game` driven 500 frames with scripted input saves; a second save is byte-identical. `restore` then `save_into` returns the same 2,308 CORE bytes and the same OAM bytes. Every rejection (corrupt, truncated, `section = 7`, odd `column_base`, `cursor` past the stream) leaves the full serialised image byte-identical. After restore `audio.is_empty()` is true. `cargo test -p gradius-core` green. | SS-A |
| SS-C | **Determinism.** Save at frame 500; run 600 more frames recording `save_into` bytes each frame; restore and replay the same inputs; compare all 600 frames. Add a 2,000-frame variant so the starfield RNG wraps and respawns. | `core/tests/` or `game.rs` tests; `gradius-diff` gets `--rewind-at F --rewind-after K` (native-vs-native) | 600/600 and 2,000/2,000 frames compare equal byte for byte, including OAM, with 0 divergences. A control run with the RNG byte zeroed diverges (proves the test is sensitive). `gradius-diff --rewind-at 500 --rewind-after 300` exits 0 on the stage-1 script and still matches the oracle in the same window. | SS-B |
| SS-D | **SDL keys and quick-save file.** `F5` save, `F9` load, trim to used length, write `gradius.quick.sav` next to the pack via temp file + rename, notice on rejection; add F4/F5/F9 to `RESERVED`; optionally `--load-state PATH` for headless checks. | `gradius-sdl/src/main.rs`, `options.rs`, `keybinds.rs`, `DEVELOPMENT.md` | Unit test: write a snapshot to a temp dir, read it back, `save_into` bytes equal, file length equals the trimmed length, a truncated file is rejected and the game bytes are unchanged. `keybinds` test shows F5/F9 cannot be bound. Headless `--frames 600 --save-state P` then `--load-state P --frames 60` reproduces the continuous run's 60-frame hash sequence. | SS-B |
| SS-E | **wasm/PWA.** `state_save() -> *const u8`, `state_len() -> u32`, `state_load(ptr, len) -> u32`; JS F5/F9 to `localStorage`; on load reset the audio engine, clear the sample queue and set `rendered` from the restored `frame`, restart the music cue. | `gradius-wasm/src/lib.rs`, `web/app.js`, `web/smoke.mjs`, `web/index.html` help text | `web/smoke.mjs` saves at frame F, reloads the page, loads, ticks 60 frames and compares the per-frame `state_hash` chain with the uninterrupted run; 60/60 equal. A corrupt `localStorage` entry leaves `frame_count()` and the rendered frame unchanged. `npm test` (`pwa.test.mjs`) passes. | SS-B |

### 7.5 Real risks and how the harness catches them

| risk | where it hides | detection |
| --- | --- | --- |
| Hidden frontend state | `MusicDirector::{previous_state, previous_score, cue}` lives in `Runner`, not `Game`, so a restore can emit a spurious cue; wasm keeps `rendered`, `queue`, `start` as audio clock | SS-E resets the engine and re-derives `rendered` from `frame`; SS-B asserts no `Sound` is pushed by restore. A test saves while the stage-1 cue is playing, restores into the boss and checks exactly one `MusicCue` change on the next `update`. |
| PRNG | `rng` is captured, but it is consumed only when a star wraps (layer 0 takes ~1,024 frames), so a 600-frame test can pass without ever touching it | SS-C's 2,000-frame variant, plus one test that sets a star near `x = 0` before saving. The control run with `rng` zeroed must diverge. |
| `cells` / `collision_cells` | stage-3 `Vec`s; `collision_cells` is never read today, `cells` changes the stage-3 background and the `frame_hash` | CELLS/COLLISION chunks with hard caps; SS-B rejects a count above the cap. SS-C runs the stage-3 script too, so a missed chunk shows as a pixel divergence. |
| Render mutates state | `render` advances `ScoreDelay` and rotates `last`/`earlier_sprites`; a snapshot taken before `render` is not a frame boundary | Capture point is after `step` + audio drain only. SS-B checks that `restore` + draw leaves `score_delay` and the two lists unchanged. |
| Assets / stage tables | same 2,308 bytes with a different pack or stage table can spawn different objects | Header fingerprint (FNV over `Assets::tiles` + `colors`); SS-D refuses a mismatch, `--force-load` overrides. SS-C fails if the fingerprint is not mixed into the header. |
| Audio determinism | voices have no rewind, sample position cannot be replayed | Never compare audio in the determinism test; SS-B asserts the queue is empty after restore and SS-E asserts the queue is cleared before the first post-load tick. |
| High score | `high_score_bcd` is part of `Game` and rewinds with everything else | SS-B round-trips it; the plan keeps the product decision (monotonic vs rewound) out of the format. |

`state_hash()` is **not** a determinism oracle: it covers only the
framebuffer, score, lives, section, scroll and frame, so a wrong `rng`,
`spawner.timer` or `ScoreDelay.index` would pass. Every determinism check in
SS-C and SS-D must compare the full `save_into` bytes (CORE + OAM).

### 7.6 Order and stop conditions

SS-A, then SS-B, then SS-C and SS-D in parallel; SS-E after SS-B. The stop
conditions did not trigger: the CORE landed at 2,524 B with the 41 B EXT
chunk, and no stage-3 run exceeded `MAX_CELLS`. Out of scope stays as
in §5: thumbnails, netplay rollback, APU position continuity, rewind inside
`gradius-diff`'s oracle side.
