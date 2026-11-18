//! Gated pixel regression for the stage-2 boss/end hand-off.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge and the workspace
//! asset pack exists. The window is the natural stage-2 run, frames
//! 19037-19430: from the frame after the core's death debris despawns through
//! the stage-3 hand-off frames. It is exactly pixel-identical today, the
//! `$1B & #$70` ring skip in `build_sprites` closing the 19429 frame, and is
//! the only part of the boss/end stretch that is gated; the boss fight
//! itself (17282-18916, the wrong core sprite group plus the core's shot
//! sprites) and the pre-hand-off tail (17280-19036) are report-only and
//! documented in `docs/analysis/stage2_boss_pixels.md`.

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

/// The documented stage-2 boss/end hand-off window, in inclusive frames.
const WINDOW: PixelWindow = PixelWindow {
    from: 19037,
    to: 19430,
};

#[test]
fn stage_two_boss_handoff_is_pixel_exact() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping pixel probe");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/boss-script.txt");
    let spec = format!("{}:{}", WINDOW.from, WINDOW.to);
    let json_path = std::env::temp_dir().join(format!(
        "gradius-stage2-boss-pixels-{}.json",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--frames", "19431"])
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
    // The state run still reports the pre-boss eruption divergences, so the
    // harness exits 1 even when this window is exact.
    let status = output.status.code().expect("no exit code");
    assert!(
        matches!(status, 0 | 1),
        "harness failed with {status}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = output.stdout;
    let stdout = String::from_utf8(stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("in window 19037-19430"), "{stdout}");

    let json = std::fs::read_to_string(&json_path).expect("cannot read JSON");
    let _ = std::fs::remove_file(&json_path);
    assert!(json.contains("\"window\": [ 19037, 19430 ]"), "{json}");
    assert!(json.contains("\"samples\": 394"), "{json}");
    // Negative control for the extended gate: 19429 sits inside it because the
    // pre-fix port drew the hand-off shield ring there (52 px at
    // (37,73)-(44,88)), one frame after what used to be the window end. Those
    // 52 px are below the 3-decimal summary's resolution (394 samples still
    // read 1.000), so pin the per-frame sample: a regression fails here.
    for frame in [19429, 19430] {
        assert!(
            json.contains(&format!(
                "{{ \"frame\": {frame}, \"state\": \"Playing\", \"differing\": 0, "
            )),
            "hand-off frame {frame} is missing from the gate or differs:\n{json}"
        );
    }
    assert!(
        json.contains("\"first_divergence\": null }"),
        "the state-side note changed shape:\n{json}"
    );
    assert!(
        json.contains(
            "\"summary\": { \"samples\": 394, \"window\": [ 19037, 19430 ], \"match\": 1.000, \
             \"playfield\": 1.000, \"hud\": 1.000, \"first_divergence\": null }"
        ),
        "the stage-2 boss/end hand-off is no longer pixel-exact:\n{json}"
    );
}

/// The whole stage-2 boss/end window. The armour frame, the `$41` shots, the
/// freeze latch and the tail CHR bank bring it to 99.8%; the remaining
/// residue is the rotating OAM flicker subset.
const BOSS_WINDOW: PixelWindow = PixelWindow {
    from: 17280,
    to: 19430,
};
const BOSS_FLOOR: i64 = 998;

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
fn stage_two_boss_window_stays_above_ninety_nine_point_eight() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping pixel probe");
        return;
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/boss-script.txt");
    let spec = format!("{}:{}", BOSS_WINDOW.from, BOSS_WINDOW.to);
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--frames", "19431"])
        .arg("--script")
        .arg(&script)
        .arg("--keep-alive")
        .arg("--native-keep-alive")
        .arg("--pixel-report")
        .args(["--pixel-window", spec.as_str()])
        .output()
        .expect("failed to run gradius-diff");
    let status = output.status.code().expect("no exit code");
    assert!(
        matches!(status, 0 | 1),
        "harness failed with {status}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("in window 17280-19430"), "{stdout}");
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("pixel summary:"))
        .expect("no pixel summary");
    let matched = thousandths(summary, "match ");
    let playfield = thousandths(summary, "play ");
    assert!(
        matched >= BOSS_FLOOR,
        "the stage-2 boss window fell to {:.1}%: {summary}",
        matched as f64 / 10.0
    );
    assert!(
        playfield >= BOSS_FLOOR,
        "the stage-2 boss playfield fell to {:.1}%: {summary}",
        playfield as f64 / 10.0
    );
}
