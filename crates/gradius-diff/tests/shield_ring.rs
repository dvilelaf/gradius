//! Report-only probe for the shield ring in the stage-5 pixel window.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge and the workspace
//! asset pack exists. The run is the poked stage-5 state gate window
//! (`--stage 5`, keep-alives, the harness's default input script), narrowed to
//! the quiet entry frames 297-339: only the ship and the ring are on screen.
//!
//! `docs/analysis/shield_ring.md` records the fix: the `--stage` anchor seeds
//! `nmi_frame` from the cartridge's `$02`, so the ring draws the ROM's
//! animation quarter. Frames 298-339 are pixel-exact now, except the three
//! frames in `EXCEPTIONS`, each pinned to its exact differing box. Any other
//! box, or a change in one of those three, fails the test.

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

/// The stage-5 state-gate window: `--from 297` is the first compared frame.
/// Frames 298-339 are the quiet entry stretch measured in the note.
const WINDOW: PixelWindow = PixelWindow { from: 297, to: 340 };

/// Listed residue on the entry frames, as the harness's exact bounding box.
/// Ship at (80, 94) on 303: one pixel beside the ring cell. Ship at (80, 88)
/// on 309: the ship's edge; the box reaches column 95, the ring cell's left
/// column. Ship at (80, 72) on 325: six pixels inside the ring cell (95-102,
/// 71-86), not the 4x12 animation-quarter swap; its cause is not yet traced.
const EXCEPTIONS: [(u64, &str); 3] = [
    (303, "87,104-87,104"),
    (309, "70,90-95,102"),
    (325, "95,82-101,84"),
];

#[test]
fn stage_five_entry_frames_are_pixel_exact_but_listed_residue() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping pixel probe");
        return;
    };
    let spec = format!("{}:{}", WINDOW.from, WINDOW.to);
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
        stdout.contains("0 divergences"),
        "the ring probe is only meaningful on an exact state window:\n{stdout}"
    );

    // Per-frame rows look like `298      0.0%  100.0%    100.0%    100.0%  -`;
    // the last field is the differing box, `-` when the frame is exact.
    let mut sampled = 0;
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(frame) = fields.first().and_then(|f| f.parse::<u64>().ok()) else {
            continue;
        };
        if !(298..=339).contains(&frame) {
            continue;
        }
        let Some(bbox) = fields.get(5) else { continue };
        let expected = EXCEPTIONS
            .iter()
            .find(|(listed, _)| *listed == frame)
            .map_or("-", |(_, text)| *text);
        assert_eq!(
            *bbox, expected,
            "frame {frame}: differs outside the listed residue"
        );
        sampled += 1;
    }
    assert_eq!(sampled, 42, "the probe did not sample frames 298-339");
    eprintln!(
        "shield ring: 42 entry frames, 39 pixel-exact, 3 listed residue frames \
         (303, 309, 325)"
    );
}
