//! Report-only measurement of the `$02` NMI-counter offset on stage 7.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge and the workspace
//! asset pack exists. The run is the checked-in stage-7 fixture's recording
//! command (`--stage 7 --frames 6050 --keep-alive --native-keep-alive`), with
//! `--oracle-out`/`--native-out` so both `$02` sequences can be read.
//!
//! `docs/analysis/stage7_triage.md` records what this pins down: the oracle's
//! `$02` increments every frame (no `$8073` hold after the anchor) and equals
//! `(frame - 5) & 0xFF`. The harness seeds the port from the cartridge's `$02`
//! at the anchor, so the port's counter is the same sequence and the turret
//! grid no longer lands nine frames early.
//!
//! The port exposes no `nmi_frame` in `dump_state`, so the native `$02` column
//! is the counter's rule (`new_at_stage_seeded` takes `$02` at the anchor less
//! one, one increment per step, no `$8073` lag reachable under keep-alive). It
//! is checked against the measured native turret grid below, which is
//! observable.

use std::path::PathBuf;
use std::process::Command;

use gradius_diff::{parse_fixture, Snapshot};

const FIRST_FRAME: u64 = 297;
const LAST_FRAME: u64 = 6050;
/// `$02` on the anchor frame (oracle 297); the seed is one less because
/// `step` increments before the frame body.
const ANCHOR_NMI: u64 = 0x24;
/// Native frame 1 is the oracle's frame 297.
const FRAME_OFFSET: u64 = FIRST_FRAME - 1;

fn rom() -> Option<PathBuf> {
    std::env::var_os("GRADIUS_ROM").map(PathBuf::from)
}

fn pack() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak");
    path.is_file().then_some(path)
}

fn oracle_nmi(frame: u64) -> u64 {
    frame.wrapping_sub(5) & 0xFF
}

fn port_nmi(frame: u64) -> u64 {
    (ANCHOR_NMI - 1).wrapping_add(frame - FRAME_OFFSET) & 0xFF
}

fn first_kind_140(dump: &[Snapshot], from: u64) -> Option<u64> {
    dump.iter()
        .filter(|snapshot| snapshot.frame >= from)
        .find(|snapshot| {
            snapshot
                .objects
                .iter()
                .any(|object| object.slot == 5 && object.kind == 140)
        })
        .map(|snapshot| snapshot.frame)
}

#[test]
fn the_stage7_anchor_seed_tracks_the_oracle_counter() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping `$02` probe");
        return;
    };
    let dir = std::env::temp_dir().join(format!("gradius-s02-nmi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("cannot create temp dir");
    let oracle_path = dir.join("oracle.state");
    let native_path = dir.join("native.state");
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--stage", "7", "--frames", "6050"])
        .args(["--keep-alive", "--native-keep-alive"])
        .arg("--oracle-out")
        .arg(&oracle_path)
        .arg("--native-out")
        .arg(&native_path)
        .output()
        .expect("failed to run gradius-diff");
    let status = output.status.code().expect("no exit code");
    assert!(
        matches!(status, 0 | 1),
        "harness failed with {status}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let read = |path: &PathBuf| {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        parse_fixture(&text).expect("malformed state dump")
    };
    let oracle = read(&oracle_path);
    let native = read(&native_path);
    let _ = std::fs::remove_dir_all(&dir);

    // The oracle's `$02` is recorded: one increment per frame, no `$8073` hold
    // in the window, and the boot's five holds already folded into `-5`.
    for snapshot in &oracle {
        if (FIRST_FRAME..=LAST_FRAME).contains(&snapshot.frame) {
            assert_eq!(
                snapshot.nmi,
                Some(oracle_nmi(snapshot.frame) as i64),
                "oracle `$02` at frame {}",
                snapshot.frame
            );
            assert_eq!(
                snapshot.mode,
                Some(0x40),
                "oracle `$03` at {}",
                snapshot.frame
            );
        }
    }

    // The seeded counter tracks the oracle on every compared frame.
    for snapshot in &native {
        let delta = (oracle_nmi(snapshot.frame) as i64 - port_nmi(snapshot.frame) as i64) & 0xFF;
        assert_eq!(delta, 0, "offset at frame {}", snapshot.frame);
    }

    // Measured turret grid: both sides fire when `$02 & 0x0F == 0` (4581,
    // 4597, ...), so the first `$8C` shell is born on the same frame.
    let oracle_birth = first_kind_140(&oracle, 4560).expect("oracle shell");
    let native_birth = first_kind_140(&native, 4560).expect("native shell");
    assert_eq!(oracle_nmi(oracle_birth - 1) & 0x0F, 0);
    assert_eq!(port_nmi(native_birth - 1) & 0x0F, 0);
    assert_eq!((oracle_birth, native_birth), (4582, 4582));
}
