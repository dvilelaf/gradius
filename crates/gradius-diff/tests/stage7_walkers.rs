//! Regression test for the stage-7 kind-`$87`/`$93` walkers.
//!
//! They used to separate at 5627 because the collision map was missing block
//! 11 (world columns 352-383): the ground walker climbed where the ROM sees
//! solid ground. With block 11 restored the walkers must stay in lockstep for
//! the whole window, and both must still be holding at 5863 (the ceiling
//! walker used to keep climbing). `docs/analysis/stage7_triage.md` and
//! `docs/analysis/shield_ring.md` carry the measurements.
//!
//! The replay is the same as `stage7_triage`: the checked-in fixture, the
//! `$02`-seeded stage-7 game and the port's keep-alive.

use std::path::PathBuf;

use gradius_cart::Cartridge;
use gradius_core::Game;
use gradius_diff::dump::keep_alive_poke;
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, nes_button_bits, parse_fixture, Snapshot, SnapshotObject};
use gradius_nes::Emulator;

const FIRST_FRAME: u64 = 297;
const LAST_FRAME: u64 = 6050;
const STAGE_INDEX: u8 = 6;
const FRAME_OFFSET: u64 = FIRST_FRAME - 1;
const RECORDED_FRAMES: u64 = LAST_FRAME;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn replay() -> (Vec<Snapshot>, Vec<Snapshot>) {
    let fixture_path = root().join("tests/fixtures/stage7-297-6050.state");
    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script = default_script(RECORDED_FRAMES);

    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for snapshot in &oracle {
        game.step(core_buttons(script.buttons(snapshot.frame - 1)));
        let mut native_snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        native_snapshot.frame += FRAME_OFFSET;
        native.push(native_snapshot);
    }
    (oracle, native)
}

/// `$87 & $7F = 7` ground walker and `$93 & $7F = 19` ceiling walker.
fn is_walker(object: &SnapshotObject) -> bool {
    matches!(object.kind & 0x7F, 7 | 19)
}

fn walkers(snapshot: &Snapshot) -> Vec<&SnapshotObject> {
    let mut found: Vec<&SnapshotObject> = snapshot
        .objects
        .iter()
        .filter(|object| is_walker(object))
        .collect();
    found.sort_by_key(|object| object.slot);
    found
}

fn walker_y(snapshot: &Snapshot, slot: i64) -> Option<i64> {
    walkers(snapshot)
        .into_iter()
        .find(|object| object.slot == slot)
        .map(|object| object.y)
}

fn print_range(oracle: &[Snapshot], native: &[Snapshot], from: u64, to: u64, step: u64) {
    eprintln!("frame | slot kind  o.y n.y | slot kind  o.y n.y");
    for frame in (from..=to).step_by(step as usize) {
        let index = (frame - FIRST_FRAME) as usize;
        let (o, n) = (&oracle[index], &native[index]);
        let mut line = format!("{frame:>5} |");
        let mut slots: Vec<i64> = o
            .objects
            .iter()
            .filter(|object| is_walker(object))
            .map(|object| object.slot)
            .chain(
                n.objects
                    .iter()
                    .filter(|object| is_walker(object))
                    .map(|object| object.slot),
            )
            .collect();
        slots.sort_unstable();
        slots.dedup();
        for slot in slots {
            let kind = o
                .objects
                .iter()
                .find(|object| object.slot == slot && is_walker(object))
                .map(|object| object.kind)
                .or_else(|| {
                    n.objects
                        .iter()
                        .find(|object| object.slot == slot && is_walker(object))
                        .map(|object| object.kind)
                })
                .unwrap_or(0);
            line.push_str(&format!(
                " {slot:>2} k{kind:<3} {:>3} {:>3} |",
                walker_y(o, slot).unwrap_or(-1),
                walker_y(n, slot).unwrap_or(-1),
            ));
        }
        eprintln!("{line}");
    }
}

#[test]
fn walkers_stay_in_lockstep() {
    let (oracle, native) = replay();
    print_range(&oracle, &native, 5618, 5645, 1);
    eprintln!();
    print_range(&oracle, &native, 5650, 5880, 10);

    // No walker may drift past the comparator's 1 px tolerance anywhere in the
    // window: block 11 of the stage-7 collision map keeps both on the terrain.
    let mut first = None;
    for (o, n) in oracle.iter().zip(&native) {
        if !(FIRST_FRAME..=LAST_FRAME).contains(&n.frame) {
            continue;
        }
        for object in walkers(o) {
            if let Some(native_y) = walker_y(n, object.slot) {
                if (object.y - native_y).abs() > 1 {
                    first = Some((n.frame, object.slot, object.kind, object.y, native_y));
                    break;
                }
            }
        }
        if first.is_some() {
            break;
        }
    }
    eprintln!("\nfirst walker divergence: {first:?}");
    assert!(first.is_none(), "walkers diverge: {first:?}");

    // Both walkers hold at 41 frames later while the native walks away.
    let index = (5863 - FIRST_FRAME) as usize;
    eprintln!(
        "5863: oracle {:?} native {:?}",
        walkers(&oracle[index]),
        walkers(&native[index])
    );
    for object in walkers(&oracle[index]) {
        if let Some(native_y) = walker_y(&native[index], object.slot) {
            assert_eq!(native_y, object.y, "walker y must match at 5863");
        }
    }
}

/// The oracle's `$0600` page at frame 5627, at the cell `$C3CD` reads for the
/// first falling walker. Ground walker slot 8: `x = 144`, `y = 157`,
/// `$3E = $69`; `(144 + 8 + $69) & $FF = $02` carries into `$3F`, so the probe
/// reads page `$0600`, column 0, group 5 (row 23) with mask `$C0`.
///
/// This is the cell the checked-in stage-7 map has zero: block 11 (world
/// columns 352-383) was never extracted.
#[test]
fn oracle_block_11_cell_is_solid() {
    let Some(rom) = std::env::var_os("GRADIUS_ROM").map(PathBuf::from) else {
        eprintln!("GRADIUS_ROM not set; skipping the stage-7 collision-page probe");
        return;
    };
    let data = std::fs::read(&rom).expect("read GRADIUS_ROM");
    let cart = Cartridge::from_ines(&data).expect("parse cartridge");
    let mut emulator = Emulator::new(&cart).expect("start oracle");
    emulator.set_audio_enabled(false);
    let script = default_script(RECORDED_FRAMES);
    // Oracle frame 5627: the state line after the 5627th `run_frame`.
    for frame in 0..5627u64 {
        if !(emulator.bus.ram[0x00] == 5 && emulator.bus.ram[0x19] == STAGE_INDEX) {
            emulator.bus.ram[0x26] = STAGE_INDEX;
        }
        let pads = script.pads(frame);
        emulator.set_buttons_p1(nes_button_bits(pads.p1));
        emulator.set_buttons_p2(nes_button_bits(pads.p2));
        emulator.run_frame();
        keep_alive_poke(&mut emulator);
    }
    let ram = &emulator.bus.ram;
    let ground = ram[0x0605];
    let ceiling = ram[0x0601];
    eprintln!(
        "frame 5627: $3F={:02X} $3E={:02X} page $0600+05={:02X} +01={:02X}",
        ram[0x3F], ram[0x3E], ground, ceiling
    );
    assert_ne!(ground & 0xC0, 0, "ground probe row 23 must be solid");
    assert_ne!(ceiling & 0x30, 0, "ceiling probe row 6 must be solid");
    assert_eq!(ceiling & 0xC0, 0, "ceiling row 7 must be empty");
}
