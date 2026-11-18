//! Gated pixel regression for the stage-2 window.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge and the workspace
//! asset pack exists. The stage-2 state is exact from 9601, and the handoff
//! frame is now pixel-exact: the old 13-pixel HUD gap `(205,214)-(210,218)` is
//! gone after the meter/shield-entry fix.

use std::path::PathBuf;
use std::process::Command;

fn rom() -> Option<PathBuf> {
    std::env::var_os("GRADIUS_ROM").map(PathBuf::from)
}

fn pack() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak");
    path.is_file().then_some(path)
}

#[test]
fn stage_two_handoff_is_pixel_exact() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping pixel probe");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/boss-script.txt");
    let json_path =
        std::env::temp_dir().join(format!("gradius-stage2-pixels-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--frames", "9601"])
        .arg("--script")
        .arg(&script)
        .arg("--keep-alive")
        .arg("--native-keep-alive")
        .arg("--pixel-report")
        .args(["--pixel-window", "9600:9600"])
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
    assert!(stdout.contains("in window 9600-9600"), "{stdout}");

    let json = std::fs::read_to_string(&json_path).expect("cannot read JSON");
    let _ = std::fs::remove_file(&json_path);
    assert!(json.contains("\"window\": [ 9600, 9600 ]"), "{json}");
    assert!(json.contains("\"frame\": 9600"), "{json}");
    assert!(
        json.contains("\"differing\": 0"),
        "the handoff frame still differs:\n{json}"
    );
    assert!(json.contains("\"bbox\": null"), "{json}");
    assert!(
        json.contains("\"playfield\": { \"differing\": 0, \"total\""),
        "the handoff gap left the HUD band:\n{json}"
    );
}
