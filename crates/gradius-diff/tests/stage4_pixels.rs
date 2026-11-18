//! Report-only stage-4 pixel window.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge and the workspace
//! asset pack exists. The window is the poked stage-4 run used by the state
//! gate (`stage4_replay.rs`), frames 297-1797, with the harness's default
//! input script and no keep-alives.
//!
//! This test prints the playfield/HUD rates and the worst frames but does not
//! gate on them: `docs/analysis/stage4_pixels.md` records the measured rate
//! (99.8% overall) and the open residues (sprite overlap, the HUD score lag
//! and the `lives = 0` digit). A hard threshold is added once the residue has
//! an owner.

use std::path::PathBuf;
use std::process::Command;

use gradius_diff::PixelWindow;

fn rom() -> Option<PathBuf> {
    std::env::var_os("GRADIUS_ROM").map(PathBuf::from)
}

fn pack() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak");
    path.is_file().then_some(path)
}

/// The stage-4 state-gate window: oracle frame 297 is native frame 1.
const WINDOW: PixelWindow = PixelWindow {
    from: 297,
    to: 1797,
};
/// Sample every 30th frame, like `scripts/longrun.sh` and the stage-3 pixel
/// probe. The per-frame report is the measurement in the analysis note; this
/// probe stays cheap.
const SAMPLE_EVERY: u64 = 30;

#[test]
fn stage_four_pixel_window_reports_playfield_and_hud() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping pixel probe");
        return;
    };
    let window = WINDOW.label();
    let spec = format!("{}:{}", WINDOW.from, WINDOW.to);
    let json_path =
        std::env::temp_dir().join(format!("gradius-stage4-pixels-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--stage", "4"])
        .arg("--frames")
        .arg((WINDOW.to + 1).to_string())
        .args(["--from", &WINDOW.from.to_string()])
        .args(["--to", &WINDOW.to.to_string()])
        .arg("--pixel-report")
        .args(["--png-every", &SAMPLE_EVERY.to_string()])
        .args(["--pixel-window", spec.as_str()])
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
    assert!(stdout.contains(&format!("in window {window}")), "{stdout}");

    let json = std::fs::read_to_string(&json_path).expect("cannot read JSON");
    let _ = std::fs::remove_file(&json_path);
    assert!(
        json.contains(&format!("\"window\": [ {}, {} ]", WINDOW.from, WINDOW.to)),
        "{json}"
    );
    let samples = (WINDOW.from..=WINDOW.to)
        .filter(|frame| frame % SAMPLE_EVERY == 0)
        .count();
    assert!(
        json.contains(&format!("\"samples\": {samples}")),
        "the pixel report did not sample the whole stage-4 window:\n{json}"
    );
    assert!(
        json.contains("\"playfield\": { \"differing\":"),
        "the playfield region is missing:\n{json}"
    );
    assert!(
        json.contains("\"hud\": { \"differing\":"),
        "the HUD region is missing:\n{json}"
    );
    for line in stdout
        .lines()
        .filter(|line| line.starts_with("pixel summary:") || line.starts_with("  worst"))
    {
        eprintln!("{line}");
    }
}
