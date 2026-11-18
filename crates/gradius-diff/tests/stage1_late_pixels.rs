//! Gated pixel regression for the late stage-1 window.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge and the workspace
//! asset pack exists. The window starts at the map end: the scroll reaches
//! `$0C00` (world column 416) and `$5B` freezes it there until the core entry,
//! so every sampled frame exercises the extracted tail, the held scroll latch
//! and the mountain rows the old 384-tile map wrapped away.

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

/// Every frame from the map end to the Big Core spawn.
const WINDOW: PixelWindow = PixelWindow {
    from: 6000,
    to: 7723,
};

/// `99.9%` in thousandths, the hard floor for the window.
const FLOOR: i64 = 999;

/// Read one `label NN.N%` rate out of the pixel summary line.
fn thousandths(summary: &str, label: &str) -> i64 {
    let start = summary
        .find(label)
        .unwrap_or_else(|| panic!("no `{label}` in {summary}"))
        + label.len();
    let rate = summary[start..]
        .split('%')
        .next()
        .expect("rate without a percent sign");
    (rate.trim().parse::<f64>().expect("rate is a number") * 10.0).round() as i64
}

#[test]
fn the_late_stage_one_window_stays_above_ninety_nine_point_nine() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping pixel probe");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/boss-script.txt");
    let window = WINDOW.label();
    let spec = format!("{}:{}", WINDOW.from, WINDOW.to);
    let json_path =
        std::env::temp_dir().join(format!("gradius-stage1-late-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--frames", "7724"])
        .arg("--script")
        .arg(&script)
        .arg("--keep-alive")
        .arg("--native-keep-alive")
        .arg("--pixel-report")
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

    let samples = (WINDOW.from..=WINDOW.to).count();
    assert!(
        stdout.contains(&format!("{samples} samples in window {window}")),
        "the pixel report did not sample the whole window:\n{stdout}"
    );
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("pixel summary:"))
        .expect("no pixel summary");
    let matched = thousandths(summary, "match ");
    let playfield = thousandths(summary, "play ");
    assert!(
        matched >= FLOOR,
        "the late stage-1 window fell to {:.1}%: {summary}",
        matched as f64 / 10.0
    );
    assert!(
        playfield >= FLOOR,
        "the late stage-1 playfield fell to {:.1}%: {summary}",
        playfield as f64 / 10.0
    );

    // Frame 6500 is the card's screenshot: the held scroll must show the
    // mountains, not the starfield or one column of them.
    let json = std::fs::read_to_string(&json_path).expect("cannot read JSON");
    let _ = std::fs::remove_file(&json_path);
    let sample = json
        .lines()
        .find(|line| line.contains("\"frame\": 6500,"))
        .expect("frame 6500 was not sampled");
    assert!(
        sample.contains("\"playfield\": { \"differing\": 0,"),
        "the playfield differs at frame 6500:\n{sample}"
    );
}
