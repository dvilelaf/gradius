//! Gated end-to-end probe of the stage-2 boss and end.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge *and* the workspace
//! asset pack exists. It runs the natural path (reset -> stage 1 -> stage 2)
//! with the committed `scripts/boss-script.txt` and the analysis-only
//! keep-alives, then pins the oracle's stage-2 landmarks and the native core
//! spawn. The native's current Y drift and missing clear are described in
//! `docs/analysis/stage2_boss.md`, so they are not asserted here.
//!
//! The full run is ~19,430 oracle frames; in a debug test build this takes
//! about two minutes.

use std::path::PathBuf;
use std::process::Command;

use gradius_diff::Snapshot;

const BOSS_KIND: i64 = 0x98;
const BOSS_SLOT: i64 = 9;

fn rom() -> Option<PathBuf> {
    std::env::var_os("GRADIUS_ROM").map(PathBuf::from)
}

fn pack() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak");
    path.is_file().then_some(path)
}

fn has_boss(snapshot: &Snapshot) -> bool {
    snapshot
        .objects
        .iter()
        .any(|object| object.slot == BOSS_SLOT && object.kind == BOSS_KIND)
}

fn read_state(path: &std::path::Path) -> Vec<Snapshot> {
    std::fs::read_to_string(path)
        .expect("cannot read state file")
        .lines()
        .map(|line| Snapshot::parse(line).expect("state line"))
        .collect()
}

#[test]
fn stage_two_boss_spawns_self_destructs_and_reaches_stage_three() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping stage-2 boss probe");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/boss-script.txt");
    let dir = std::env::temp_dir();
    let stem = format!("gradius-stage2-boss-{}", std::process::id());
    let oracle_path = dir.join(format!("{stem}.oracle"));
    let native_path = dir.join(format!("{stem}.native"));

    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--frames", "19430"])
        .arg("--script")
        .arg(&script)
        .arg("--keep-alive")
        .arg("--native-keep-alive")
        .args(["--from", "17280", "--to", "19430"])
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
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("(window 17280-19430)"), "{stdout}");
    assert!(stdout.contains("2151 compared"), "{stdout}");

    let oracle = read_state(&oracle_path);
    let native = read_state(&native_path);
    let _ = std::fs::remove_file(&oracle_path);
    let _ = std::fs::remove_file(&native_path);
    assert_eq!(oracle.len(), 19430);
    assert_eq!(native.len(), 19430);

    // Both sides spawn the stage-2 core on the same frame, covers and all.
    let spawn = oracle
        .iter()
        .find(|snapshot| snapshot.frame >= 17000 && has_boss(snapshot))
        .expect("the stage-2 Big Core never spawned");
    assert_eq!(spawn.frame, 17282, "stage-2 boss spawn frame");
    assert_eq!(spawn.score, 2760, "spawn score");
    let native_spawn = native
        .iter()
        .find(|snapshot| snapshot.frame >= 17000 && has_boss(snapshot))
        .expect("the native port never spawned the stage-2 core");
    assert_eq!(native_spawn.frame, 17282, "native spawn frame");

    // No input after frame 9800, so the oracle clears by overheat: the core
    // becomes kind-2 debris without the +1000 damage reward.
    let death = oracle
        .iter()
        .find(|snapshot| snapshot.frame > spawn.frame && !has_boss(snapshot))
        .expect("the oracle core never cleared");
    assert_eq!(death.frame, 18915, "self-destruct frame");
    assert_eq!(death.score, spawn.score, "overheat clear has no reward");
    assert!(
        death
            .objects
            .iter()
            .any(|object| object.slot == BOSS_SLOT && object.kind == 2),
        "the cleared core should be kind-2 debris"
    );

    // Stage 3 starts when the scroll high byte returns to zero; the low byte
    // keeps the step that ran on the transition frame.
    let next = oracle
        .iter()
        .find(|snapshot| snapshot.frame > death.frame && snapshot.scroll_high == 0)
        .expect("stage 3 never started");
    assert_eq!(next.frame, 19429, "stage-3 frame");
    assert_eq!(next.scroll_low, 1, "stage-3 first scroll byte");
}
