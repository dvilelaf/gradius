//! Gated stage-3 pixel window report.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge and the workspace
//! asset pack exists. The window is the natural stage-3 run, frames
//! 19429-23000: the boss script with both analysis-only keep-alives from reset.
//! This pins the measured aggregate: the accepted playfield residue is the
//! sprite group `$25` object-fire bullets missing from 19530 and the
//! eight-per-scanline priority subset from 19505; the HUD is exact. The two
//! hand-off frames 19429/19430 are pixel-exact since the `$1B & #$70` ring
//! skip, so they no longer contribute. It prints the playfield/HUD rates and
//! the first differing sample, and fails if the overall rate moves off the
//! documented 99.0%.

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

/// The documented natural stage-3 pixel window. Stage 3 starts at 19429 (the
/// scroll reset after the stage-2 tail); 23000 is the end of the recorded
/// state window, so both checks cover the same frames.
const WINDOW: PixelWindow = PixelWindow {
    from: 19429,
    to: 23000,
};
/// Sample every 30th frame, like `scripts/longrun.sh`. The window is 3572
/// frames, so a full per-frame report would spend minutes on the pixel diff.
const SAMPLE_EVERY: u64 = 30;

#[test]
fn stage_three_pixel_window_reports_playfield_and_hud() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping pixel probe");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/boss-script.txt");
    let window = WINDOW.label();
    let spec = format!("{}:{}", WINDOW.from, WINDOW.to);
    let json_path =
        std::env::temp_dir().join(format!("gradius-stage3-pixels-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--frames", "23001"])
        .arg("--script")
        .arg(&script)
        .arg("--keep-alive")
        .arg("--native-keep-alive")
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
        "the pixel report did not sample the whole stage-3 window:\n{json}"
    );
    assert!(
        json.contains("\"playfield\": { \"differing\":"),
        "the playfield region is missing:\n{json}"
    );
    assert!(
        json.contains("\"hud\": { \"differing\":"),
        "the HUD region is missing:\n{json}"
    );
    // Pin the measured aggregate rather than a per-frame match: the accepted
    // playfield residue keeps play at 98.8% while the HUD is exact over the
    // 119 samples.
    let summary = "\"summary\": { \"samples\": 119, \"window\": [ 19429, 23000 ], \
                   \"match\": 0.990, \"playfield\": 0.988, \"hud\": 1.000";
    assert!(
        json.contains(summary),
        "stage-3 pixel match moved off 99.0% (play 98.8%, hud 100.0%):\n{json}"
    );
    for line in stdout
        .lines()
        .filter(|line| line.starts_with("pixel summary:"))
    {
        eprintln!("{line}");
    }
}
