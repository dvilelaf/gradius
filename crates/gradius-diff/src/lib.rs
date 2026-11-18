//! Lockstep differential harness: the native port against the original ROM.
//!
//! The harness runs the cartridge oracle ([`gradius_nes::Emulator`]) and the
//! native port ([`gradius_core::Game`]) side by side, one NTSC frame at a time,
//! feeding both the same [`Buttons`]. After every frame it dumps the same
//! state line from each and compares them field by field.
//!
//! The binary is `gradius-diff`:
//!
//! ```text
//! gradius-diff --rom PATH --pack PATH --frames N
//!              [--script PATH] [--stage N] [--from FRAME] [--to FRAME]
//!              [--keep-alive] [--native-keep-alive]
//!              [--oracle-out F] [--native-out F]
//!              [--json PATH] [--tolerance PX] [--report]
//!              [--png-dir DIR --png-every N] [--pixel-report]
//!              [--pixel-window FROM:TO]
//!              [--rewind-at FRAME] [--rewind-span N]
//! ```
//!
//! `--tolerance` is the allowed absolute difference in whole pixels for the
//! ship and object positions (default 1). Everything else that is compared is
//! exact. `--json` writes the same summary as one JSON object. Exit code 0
//! means the run compared at least one frame and stayed inside the tolerance,
//! 1 means it did not, 2 means a usage or I/O error.
//!
//! ## Comparing only the Playing window
//!
//! Only frames where **both** sides report `Playing` are compared. The port
//! zeroes its position, scroll and object fields outside play, and the original
//! spends its first frames in title/attract states, so those frames would
//! measure the transition script rather than the simulation. The report names
//! the window (`Playing frames FIRST-LAST`), how many frames were compared and
//! how many were skipped, including how many of the skipped frames had
//! differing states.
//!
//! `--stage N` is a 1-based stage number (`$19 = N - 1`). The oracle pokes
//! `$26` until the requested stage is playing, and the native port is created
//! with [`gradius_core::Game::new_at_stage_seeded`] five frames after the
//! oracle's first Playing stage frame, seeded with the oracle's `$02` at that
//! anchor. The oracle holds the scroll for `$0D = 5` frames
//! after the stage handler starts, while `new_at_stage` starts in normal play;
//! waiting out that hold makes the scroll, ship, speed, lives and score start
//! together. The native stage boot still lacks the oracle's first-column
//! spawner lead, so `--stage` stays a diagnostic run: object fields can differ
//! from entry until the stage boot hook reproduces the full Blank/Init/Normal
//! sequence. Without `--from`/`--to` the compared window is the native's stage
//! span; the explicit range compares every frame in it, including the
//! stage-init and stage-clear states. The default (no `--stage`) run is
//! unchanged.
//!
//! `--from FRAME --to FRAME` limits comparison to an inclusive frame range.
//! Inside an explicit window every frame is compared even when a side has left
//! `Playing` (the stage clear can change the state), so the window can span the
//! boss fight and the clear. `--rewind-at FRAME` snapshots the native port at
//! oracle frame FRAME, and after `--rewind-span` more frames (default 600)
//! restores the image, replays the same inputs and reports whether the state
//! matches the continuous run. `--keep-alive` applies the same analysis-only
//! player-death poke as `xtask record-boss` to the **oracle**, so a recorded
//! boss script can reach the core; the native side is not poked, so its own
//! death still shows up as a divergence. `--native-keep-alive` additionally
//! enables the port's analysis-only `Game::dev_keep_alive(true)`, so both sides
//! stay alive and the boss window can be compared frame by frame.
//!
//! ## State line
//!
//! Both dumps use the format documented by
//! [`gradius_core::Game::dump_state`]:
//!
//! ```text
//! frame=N state=S px=N pf=N py=N pyf=N speed=N scroll=LOW:HIGH:FRAC
//! score=N lives=N capsule=N meter=N objects=SLOT:KIND:X:Y:SUB;... [nmi=N] [mode=N]
//! ```
//!
//! `px`/`pf` and `py`/`pyf` are the ship's 8.8 fixed-point position, `speed`
//! is `$40`, `scroll` is `$3E`/`$3F`/`$3D`, `capsule` is the port's capsule
//! counter and `meter` is `$42`. Objects are ascending slot order, fields
//! `slot:kind:x:y:substate`; `kind` is `$030C` and `x`/`y` are integer pixels.
//!
//! `nmi` (`$02`, the NMI frame counter) and `mode` (`$03`) are appended after
//! `objects`, the format's append-only position. The oracle writes them; lines
//! without them (old fixtures, and a native `dump_state()` line) parse with
//! both unset. They are measurement data for the `$02`-phase analysis in
//! `docs/analysis/stage7_triage.md`, so the comparator ignores them.
//!
//! ### What the oracle cannot supply
//!
//! [`gradius_nes::PlayerState`] and [`gradius_nes::ObjectState`] cover most of
//! the line. The gaps are filled from the documented RAM map
//! (`docs/analysis/ram_map.md`) and are called out here:
//!
//! * `state`: decoded from `$00` (0 = `Title`, 5 = `Playing`; the stage-init
//!   phases write 1–4, reported as `Other(<n>)`).
//! * `meter`: `$42`, read from [`gradius_nes::Emulator::ram_snapshot`].
//! * `scroll` fraction: `$3D`, read from RAM. `PlayerState` only carries
//!   `$3F:$3E`.
//! * `capsule`: the original keeps no counter. `$42` is the six-slot meter
//!   selection, which advances one step per pickup and resets on use, so the
//!   oracle emits `0` and the comparator does not compare this field.
//! * object `substate`: `ObjectState::substate` is `$03BC`, which the port
//!   stores as the object's Y velocity; the native dump's fifth object field
//!   is `$048C` (`ai_state`). The oracle therefore reads `$048C + slot` from
//!   RAM so both sides compare the same variable.
//! * object slot 0 is the ship in the original's parallel arrays, while the
//!   port keeps the ship outside its object slots. Slot 0 is therefore
//!   excluded from the oracle's object list; the ship is compared through the
//!   top-level `px`/`py` fields. Slots above 9 are excluded too: `$A3AB` only
//!   allocates slots 9 down to 0, and `Emulator::objects` scans 32 bytes, so
//!   beyond slot 9 it reads overlapping position arrays. Like the port's
//!   `Object::live`, an object counts as live when `$030C != 0`. Note that
//!   `Emulator::objects` first filters on the `$0120` sprite-frame byte, so a
//!   slot allocated but not yet given a frame can be absent for one frame.
//!
//! ## Input scripts
//!
//! `--script` reads `FRAME:buttons` lines, one press per line. A line prefixed
//! with `p2` presses the second controller:
//!
//! ```text
//! # comments and blank lines are ignored
//! 180:select
//! 240:start
//! p2 900:start
//! 300:fire:120
//! ```
//!
//! Buttons are `a,b,select,start,up,down,left,right`, separated by commas
//! (`a,right`) or plus signs (`a+right`). A press is held for 6 frames, like
//! `xtask --press`, or for an explicit duration with a third field. Presses
//! that overlap are unioned. Unprefixed lines are P1. The oracle receives both
//! pads (`$4016`/`$4017`); the native port's [`gradius_core::Game::step`] still
//! takes one pad, so a `p2`-only press is an oracle-only input until the
//! two-player runtime is wired.
//!
//! Without `--script` the deterministic default is used: Start pulses at
//! frames 180 and 240 (6 frames each), then from frame 300 the ship holds fire
//! and steers in 60-frame blocks cycling up, right, left, up, ...
//!
//! ## What it measures
//!
//! Every frame inside the window is compared and each mismatch is recorded
//! with its frame and field. One run prints:
//!
//! * the window and the number of frames compared and skipped;
//! * a per-field pass-rate table (`passes/checks`, rate) for the ship
//!   position, speed, scroll, score, lives, meter and the object fields;
//! * the first divergence and, with `--report`, the first ten;
//! * max and mean ship position error in pixels;
//! * the object slot match rate and the live-object-count deltas.
//!
//! `--json PATH` writes the same numbers as one JSON object, so scripts can
//! track a regression instead of reading prose.

#![forbid(unsafe_code)]

pub mod compare;
pub mod dump;
pub mod fixture;
pub mod pixel;
pub mod rewind;
pub mod scenario;
pub mod script;
pub mod testdata;

pub use compare::{compare, CompareConfig, Delta, Divergence, FieldStat, Report};
pub use dump::{Snapshot, SnapshotObject};
pub use fixture::{parse_fixture, write_fixture};
pub use pixel::{PixelDiff, PixelSample, PixelSummary, PixelWindow};
pub use rewind::{compare_states, FieldDiff, StateDiff};
pub use script::{default_script, parse_script, ButtonSet, Pads, Press, Script};

use gradius_core::Buttons;

/// The oracle's button bits, in the order the NES controller shifts them out.
pub mod nes_buttons {
    pub const A: u8 = 0x01;
    pub const B: u8 = 0x02;
    pub const SELECT: u8 = 0x04;
    pub const START: u8 = 0x08;
    pub const UP: u8 = 0x10;
    pub const DOWN: u8 = 0x20;
    pub const LEFT: u8 = 0x40;
    pub const RIGHT: u8 = 0x80;
}

/// Translate the harness button set into the core's plain input value.
pub fn core_buttons(set: ButtonSet) -> Buttons {
    Buttons {
        fire: set.a,
        power: set.b,
        select: set.select,
        start: set.start,
        up: set.up,
        down: set.down,
        left: set.left,
        right: set.right,
    }
}

/// Translate the harness button set into the oracle's raw controller byte.
pub fn nes_button_bits(set: ButtonSet) -> u8 {
    let mut bits = 0;
    for (pressed, bit) in [
        (set.a, nes_buttons::A),
        (set.b, nes_buttons::B),
        (set.select, nes_buttons::SELECT),
        (set.start, nes_buttons::START),
        (set.up, nes_buttons::UP),
        (set.down, nes_buttons::DOWN),
        (set.left, nes_buttons::LEFT),
        (set.right, nes_buttons::RIGHT),
    ] {
        if pressed {
            bits |= bit;
        }
    }
    bits
}
