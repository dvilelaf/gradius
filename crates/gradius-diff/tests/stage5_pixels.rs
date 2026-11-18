//! Report-only stage-5 pixel window.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge and the workspace
//! asset pack exists. The window is the poked stage-5 run used by the state
//! gate in `scripts/release-check.sh`: `--stage 5 --frames 8573 --from 297
//! --to 8573` with the oracle and native keep-alives, the harness's default
//! input script and a per-frame pixel report (8,276 samples; about a minute in
//! a debug build, so the emulation dominates and `--png-every` buys nothing).
//!
//! This test prints the playfield/HUD rates and the worst frames but does not
//! gate on them: `docs/analysis/stage5_pixels.md` records the measurement
//! (2026-11-12; 99.0% overall, 98.8% playfield, 100.0% HUD) and shows that the
//! residue is sprite presentation — shot trails, starfield dots and the score
//! queue — not the column-384 background tail. The window tops out at scroll
//! 2,816.5 px, below the tail cut at 3,072 px, so no tail tile is ever drawn
//! here (tail columns reach the screen only past 3,072 - 256 px). State is
//! exact over the whole window (0 divergences over 8,277 compared frames).
//! A hard threshold is added once the sprite residue has an owner.

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

/// The stage-5 state-gate window. The pixel sample is labelled with the
/// harness frame (0-based), so `--frames 8573` never emits a sample at 8573:
/// the compared pixels are frames 297-8572 (8,276 samples).
const WINDOW: PixelWindow = PixelWindow {
    from: 297,
    to: 8573,
};

#[test]
fn stage_five_pixel_window_reports_playfield_and_hud() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping pixel probe");
        return;
    };
    let window = WINDOW.label();
    let spec = format!("{}:{}", WINDOW.from, WINDOW.to);
    let json_path =
        std::env::temp_dir().join(format!("gradius-stage5-pixels-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--stage", "5"])
        .arg("--frames")
        .arg(WINDOW.to.to_string())
        .args(["--from", &WINDOW.from.to_string()])
        .args(["--to", &WINDOW.to.to_string()])
        .args(["--keep-alive", "--native-keep-alive", "--pixel-report"])
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
    let samples = (WINDOW.from..WINDOW.to).count();
    assert!(
        json.contains(&format!(
            "\"summary\": {{ \"samples\": {samples}, \"window\": [ {}, {} ]",
            WINDOW.from, WINDOW.to
        )),
        "the pixel report did not sample the whole stage-5 window:\n{json}"
    );
    assert!(
        json.contains("\"playfield\": { \"differing\":"),
        "the playfield region is missing:\n{json}"
    );
    assert!(
        json.contains("\"hud\": { \"differing\":"),
        "the HUD region is missing:\n{json}"
    );
    // Pin the report-only measurement documented in
    // `docs/analysis/stage5_pixels.md`: 99.7% overall, 99.7% playfield and
    // 100.0% HUD, printed to three decimals.
    assert!(
        json.contains(
            "\"match\": 0.997, \"playfield\": 0.997, \"hud\": 1.000, \"first_divergence\""
        ),
        "the measured rates moved away from 99.7/99.7/100.0:\n{json}"
    );
    for line in stdout
        .lines()
        .filter(|line| line.starts_with("pixel summary:") || line.starts_with("  worst"))
    {
        eprintln!("{line}");
    }
}
