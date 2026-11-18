//! Gated end-to-end probe of the stage-1 boss window.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge *and* the workspace
//! asset pack exists. It runs the committed `scripts/boss-script.txt` with the
//! analysis-only `--keep-alive` poke so the oracle reaches the core, then
//! checks the oracle timeline from the recorded state lines. The native side is
//! still compared over every requested frame; its current divergences are what
//! the report describes, so they are not asserted here.

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

#[test]
fn boss_window_spawns_stops_kills_and_reaches_stage_two() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping boss probe");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/boss-script.txt");
    let dir = std::env::temp_dir();
    let stem = format!("gradius-boss-{}", std::process::id());
    let oracle_path = dir.join(format!("{stem}.state"));
    let json_path = dir.join(format!("{stem}.json"));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .arg("--frames")
        .arg("9700")
        .arg("--script")
        .arg(&script)
        .arg("--keep-alive")
        .args(["--from", "7500", "--to", "9700"])
        .arg("--oracle-out")
        .arg(&oracle_path)
        .arg("--json")
        .arg(&json_path)
        .output()
        .expect("failed to run gradius-diff");
    let status = output.status.code().expect("no exit code");
    assert!(
        matches!(status, 0 | 1),
        "harness failed with {status}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("(window 7500-9700)"), "{stdout}");
    assert!(stdout.contains("2201 compared"), "{stdout}");

    let lines = std::fs::read_to_string(&oracle_path).expect("cannot read oracle state");
    let snapshots: Vec<Snapshot> = lines
        .lines()
        .map(|line| Snapshot::parse(line).expect("oracle state line"))
        .collect();
    let _ = std::fs::remove_file(&oracle_path);
    assert_eq!(snapshots.len(), 9700);

    // The core appears in slot 9 as kind $98 during the freeze.
    let spawn = snapshots
        .iter()
        .find(|snapshot| snapshot.frame >= 7000 && has_boss(snapshot))
        .expect("the Big Core never spawned");
    assert_eq!(spawn.frame, 7723, "boss spawn frame");
    assert_eq!(spawn.score, 1440, "boss spawn score");

    // The stage scroll is frozen at $0D00 from just before the spawn until
    // the entry finishes and the core stops at x = 164.
    let frozen = &snapshots[7722 - 1];
    assert_eq!(frozen.scroll_high, 0x0D, "scroll stop");
    assert_eq!(frozen.scroll_low, 0, "scroll stop");
    let stopped = &snapshots[7798 - 1];
    assert_eq!(
        (stopped.scroll_low, stopped.scroll_high, stopped.scroll_frac),
        (frozen.scroll_low, frozen.scroll_high, frozen.scroll_frac),
        "scroll moved during the boss entry"
    );

    // Seventh hit: the core disappears and the score jumps by the +1000
    // reward. Until then the last live line still shows damage 6.
    let death = snapshots
        .iter()
        .find(|snapshot| {
            snapshot.frame > spawn.frame
                && !has_boss(snapshot)
                && snapshot.score >= spawn.score + 1000
        })
        .expect("the Big Core never died");
    assert_eq!(death.frame, 9087, "boss death frame");
    assert_eq!(death.score, 2690, "score at the clear");

    // Stage 2 starts when the scroll high byte returns to zero.
    let stage_two = snapshots
        .iter()
        .find(|snapshot| snapshot.frame > death.frame && snapshot.scroll_high == 0)
        .expect("stage 2 never started");
    assert_eq!(stage_two.frame, 9601, "stage 2 frame");

    let json = std::fs::read_to_string(&json_path).expect("cannot read JSON");
    let _ = std::fs::remove_file(&json_path);
    assert!(json.contains("\"compared\": 2201"), "{json}");
    assert!(json.contains("\"window\""), "{json}");
}

#[test]
fn native_keep_alive_holds_the_player_through_the_boss_window() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping boss probe");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/boss-script.txt");
    let dir = std::env::temp_dir();
    let stem = format!("gradius-boss-native-{}", std::process::id());
    let native_path = dir.join(format!("{stem}.state"));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .arg("--frames")
        .arg("9700")
        .arg("--script")
        .arg(&script)
        .arg("--keep-alive")
        .arg("--native-keep-alive")
        .args(["--from", "7500", "--to", "9700"])
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
    assert!(stdout.contains("(window 7500-9700)"), "{stdout}");
    assert!(stdout.contains("2201 compared"), "{stdout}");

    let lines = std::fs::read_to_string(&native_path).expect("cannot read native state");
    let snapshots: Vec<Snapshot> = lines
        .lines()
        .map(|line| Snapshot::parse(line).expect("native state line"))
        .collect();
    let _ = std::fs::remove_file(&native_path);
    assert_eq!(snapshots.len(), 9700);

    // Without the flag the native ship dies and loses lives long before the
    // core; with it every frame keeps the three lives it started with.
    let lost = snapshots
        .iter()
        .find(|snapshot| snapshot.state == "Playing" && snapshot.lives != 3)
        .map(|snapshot| (snapshot.frame, snapshot.lives));
    assert_eq!(lost, None, "the native player lost a life at {lost:?}");

    // It also stays in play long enough to reach its own core scene.
    assert!(
        snapshots
            .iter()
            .any(|snapshot| snapshot.frame > 6000 && snapshot.state == "Playing"),
        "the native player never reached the core"
    );
}
