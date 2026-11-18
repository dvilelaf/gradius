//! `--rewind-at`: snapshot the native port, replay the span, compare.
//!
//! The cartridge case is gated on `GRADIUS_ROM` and the workspace asset pack,
//! like the other end-to-end probes. The synthetic case needs no cartridge,
//! only the pack: it snapshots a `Game`, replays the same span and compares
//! the state line.

use std::path::PathBuf;
use std::process::Command;

use gradius_core::{Buttons, Game};
use gradius_diff::rewind;
use gradius_diff::testdata;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rom() -> Option<PathBuf> {
    std::env::var_os("GRADIUS_ROM").map(PathBuf::from)
}

fn pack() -> Option<PathBuf> {
    let path = root().join("assets/gradius.pak");
    path.is_file().then_some(path)
}

/// The clean end-to-end case from the card: a stage-5 run that snapshots at
/// frame 600 and replays the last 600 frames exactly.
#[test]
fn a_rom_rewind_is_identical() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping rewind probe");
        return;
    };
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--stage", "5", "--frames", "1200", "--rewind-at", "600"])
        .output()
        .expect("failed to run gradius-diff");
    let status = output.status.code().expect("no exit code");
    assert!(
        matches!(status, 0 | 1),
        "harness failed with {status}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.contains("rewind: identical (frames 600-1200"),
        "{stdout}"
    );
}

/// Snapshot at frame 20, replay frames 21-40 with the same (empty) inputs: the
/// result must equal the continuous frame-40 line. Tampering with the
/// continuous line then has to name the field.
#[test]
fn a_replay_is_compared_field_by_field() {
    let mut continuous_game = Game::new(testdata::assets());
    for _ in 0..40 {
        continuous_game.step(Buttons::default());
    }
    let continuous = continuous_game.dump_state();

    let mut snapshot_game = Game::new(testdata::assets());
    for _ in 0..20 {
        snapshot_game.step(Buttons::default());
    }
    let snapshot = snapshot_game.snapshot();
    let inputs = [Buttons::default(); 20];

    let diff =
        rewind::check(&mut continuous_game, &snapshot, &inputs, &continuous).expect("rewind");
    assert!(
        diff.is_identical(),
        "a clean rewind differs:\n{}",
        diff.report()
    );

    let tampered = continuous.replace("frame=40 ", "frame=39 ");
    let diff = rewind::check(&mut continuous_game, &snapshot, &inputs, &tampered).expect("rewind");
    assert!(!diff.is_identical());
    assert_eq!(diff.differences.len(), 1, "{}", diff.report());
    assert_eq!(diff.differences[0].field, "frame");
    assert_eq!(diff.differences[0].continuous, "39");
    assert_eq!(diff.differences[0].replayed, "40");
}
