//! Audit the object table's edges: collision slot 0 and the out-of-range
//! "ghost" slots the oracle's `objects()` view can report.
//!
//! `Emulator::objects()` scans slots `0..32` and keeps the ones whose frame
//! counter (`$012C + slot`) is non-zero, but the cartridge only allocates ten
//! slots: `$A521` clears `$0100,X`..`$04E0,X` with `X = $A8 + 12` and `$A3AB`
//! loads `#$09`. Slot 0 is therefore a real enemy slot, and slots 10 and above
//! read unrelated RAM. `dump::oracle_snapshot` filters `(1..=9)`, so a live
//! slot-0 object never reaches a fixture or a comparison.
//!
//! This probe replays the oracle over every window the harness gates and
//! reports, frame by frame:
//!
//! * slot-0 objects (frame counter non-zero, and the subset with `$030C != 0`
//!   that the dump filter drops);
//! * ghost slots 10..=31 (frame counter non-zero outside the real table).
//!
//! The pinned counts at the end are the measured 2026-11-12 answer: they are
//! what makes the audit reproducible and what fails if the cartridge's slot
//! usage ever changes beneath the fixtures.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge; CI never needs it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gradius_cart::Cartridge;
use gradius_diff::dump::keep_alive_poke;
use gradius_diff::script::{default_script, Script};
use gradius_diff::{nes_button_bits, parse_script};
use gradius_nes::Emulator;

/// `$012C + slot`: the per-object frame counter, and `objects()`' liveness test.
const SLOT_FRAME: usize = 0x012C;
/// `$030C + slot`: the object kind.
const SLOT_KIND: usize = 0x030C;
/// `$036C + slot`: X integer.
const SLOT_X: usize = 0x036C;
/// `$032C + slot`: Y integer.
const SLOT_Y: usize = 0x032C;

fn rom() -> Option<PathBuf> {
    std::env::var_os("GRADIUS_ROM").map(PathBuf::from)
}

fn repo_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name)
}

/// One live object the audit saw.
#[derive(Clone, Copy, Debug)]
struct Hit {
    frame: u64,
    slot: u8,
    kind: u8,
    x: u8,
    y: u8,
}

/// One unbroken run of frames with a live slot-0 object.
struct Run {
    start: u64,
    end: u64,
    /// Position on the run's first frame, to show it moves like an object.
    x: u8,
    y: u8,
    /// Frame count per kind over the run.
    kinds: BTreeMap<u8, u64>,
}

/// What one window produced.
#[derive(Default)]
struct Audit {
    /// Frames whose slot-0 frame counter is non-zero (slot 0 is "live").
    slot0: Vec<Hit>,
    /// The subset of those with a non-zero kind: exactly what the dump drops.
    slot0_kind: Vec<Hit>,
    /// Frames with a live slot 10 or above.
    ghost: Vec<Hit>,
    /// Live-frame count per ghost slot.
    ghost_slots: BTreeMap<u8, u64>,
}

impl Audit {
    fn record(&mut self, frame: u64, ram: &[u8; 2048]) {
        if ram[SLOT_FRAME] != 0 {
            let hit = Hit {
                frame,
                slot: 0,
                kind: ram[SLOT_KIND],
                x: ram[SLOT_X],
                y: ram[SLOT_Y],
            };
            self.slot0.push(hit);
            if hit.kind != 0 {
                self.slot0_kind.push(hit);
            }
        }
        for slot in 10..32usize {
            if ram[SLOT_FRAME + slot] != 0 {
                *self.ghost_slots.entry(slot as u8).or_default() += 1;
                self.ghost.push(Hit {
                    frame,
                    slot: slot as u8,
                    kind: ram[SLOT_KIND + slot],
                    x: ram[SLOT_X + slot],
                    y: ram[SLOT_Y + slot],
                });
            }
        }
    }
}

/// Replay the oracle over one gated window and audit the object table.
///
/// `stage` pokes `$26` like `--stage`; `script` is the recording's own input;
/// `keep_alive` mirrors the `--keep-alive` poke the fixture was recorded with.
fn scan(
    rom: &Cartridge,
    stage: Option<u8>,
    script: &Script,
    frames: u64,
    from: u64,
    to: u64,
    keep_alive: bool,
) -> Audit {
    let mut emulator = Emulator::new(rom).expect("oracle starts");
    emulator.set_audio_enabled(false);
    let mut audit = Audit::default();
    for frame in 0..frames {
        if let Some(target) = stage {
            if !(emulator.bus.ram[0x00] == 5 && emulator.bus.ram[0x19] == target) {
                emulator.bus.ram[0x26] = target;
            }
        }
        let pads = script.pads(frame);
        emulator.set_buttons_p1(nes_button_bits(pads.p1));
        emulator.set_buttons_p2(nes_button_bits(pads.p2));
        emulator.run_frame();
        if keep_alive {
            keep_alive_poke(&mut emulator);
        }
        // `frame` is 0-based; the dump's `frame=` field is 1-based.
        let dumped = frame + 1;
        if dumped >= from && dumped <= to {
            audit.record(dumped, &emulator.bus.ram);
        }
    }
    audit
}

fn report(name: &str, audit: &Audit) {
    eprintln!(
        "{name}: slot-0 live {} frame(s) (kind!=0: {}), ghosts {} frame(s) over {:?}",
        audit.slot0.len(),
        audit.slot0_kind.len(),
        audit.ghost.len(),
        audit.ghost_slots
    );
    // Fold the slot-0 frames into runs and name the kinds each run holds.
    let mut runs: Vec<Run> = Vec::new();
    for hit in &audit.slot0 {
        match runs.last_mut() {
            Some(run) if hit.frame == run.end + 1 => {
                run.end = hit.frame;
                *run.kinds.entry(hit.kind).or_default() += 1;
            }
            _ => runs.push(Run {
                start: hit.frame,
                end: hit.frame,
                x: hit.x,
                y: hit.y,
                kinds: BTreeMap::from([(hit.kind, 1)]),
            }),
        }
    }
    for run in runs {
        let Run {
            start,
            end,
            x,
            y,
            kinds,
        } = run;
        eprintln!("  slot-0 {start}-{end} from x={x} y={y}, kinds {kinds:?}");
    }
    let mut ghosts: BTreeMap<u8, (u64, u64, u64)> = BTreeMap::new();
    for hit in &audit.ghost {
        let entry = ghosts.entry(hit.slot).or_insert((hit.frame, hit.frame, 0));
        entry.1 = hit.frame;
        entry.2 += 1;
    }
    for (slot, (first, last, count)) in ghosts {
        eprintln!("  ghost slot {slot}: {count} frame(s), {first}-{last}");
    }
}

fn cart() -> Option<Cartridge> {
    let Some(path) = rom() else {
        eprintln!("GRADIUS_ROM unset; skipping slot-0 audit");
        return None;
    };
    let data = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    Some(Cartridge::from_ines(&data).expect("cartridge parses"))
}

#[test]
fn slot_zero_is_live_in_the_default_window() {
    let Some(cart) = cart() else { return };
    // Window 1: the default-script stage-1 run, `--frames 12000`.
    let audit = scan(
        &cart,
        None,
        &default_script(12000),
        12000,
        297,
        12000,
        false,
    );
    report("default 297-12000", &audit);
    // Measured 2026-11-12: slot 0 is a real enemy slot and carries live
    // objects in this window, which `(1..=9)` drops.
    assert_eq!(audit.slot0.len(), 676, "slot-0 live frames");
    assert_eq!(audit.slot0_kind.len(), 676, "slot-0 frames with a kind");
    // Every ghost slot is above 9, so the dump's cap keeps them out.
    assert!(audit.ghost.iter().all(|hit| hit.slot > 9));
    assert!(
        !audit.ghost.is_empty(),
        "no ghost slot at all: the scan is not running"
    );
}

#[test]
fn slot_zero_is_live_in_the_boss_window() {
    let Some(cart) = cart() else { return };
    // Window 2: the natural boss-script run to the stage-3 window
    // (`tests/fixtures/stage3-19429-23000.state`).
    let text = std::fs::read_to_string(repo_file("scripts/boss-script.txt")).expect("boss script");
    let script = parse_script(&text).expect("boss script parses");
    let audit = scan(&cart, None, &script, 23000, 19429, 23000, true);
    report("boss 19429-23000", &audit);
    assert_eq!(audit.slot0.len(), 432, "slot-0 live frames");
    assert_eq!(audit.slot0_kind.len(), 432, "slot-0 frames with a kind");
}

#[test]
fn slot_zero_is_live_in_the_poked_stage_windows() {
    let Some(cart) = cart() else { return };
    for (name, stage, frames, last, expected) in [
        ("stage5 297-8573", 5u8, 8573u64, 8573u64, 1243usize),
        ("stage7 297-6050", 7, 6050, 6050, 2292),
    ] {
        let script = default_script(frames);
        let audit = scan(&cart, Some(stage - 1), &script, frames, 297, last, true);
        report(name, &audit);
        assert_eq!(audit.slot0.len(), expected, "slot-0 live frames in {name}");
    }
}

#[test]
fn slot_zero_is_live_in_the_ending_window() {
    let Some(cart) = cart() else { return };
    // Window 5: the natural run to the ending (`ending-7015-8290.state`).
    let text = std::fs::read_to_string(repo_file("tests/fixtures/ending-7015-8290.script"))
        .expect("ending script");
    let script = parse_script(&text).expect("ending script parses");
    let audit = scan(&cart, None, &script, 8290, 7025, 8290, true);
    report("ending 7025-8290", &audit);
    assert_eq!(audit.slot0.len(), 106, "slot-0 live frames");
}
