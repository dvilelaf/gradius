//! Replay the checked-in stage-4 fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack. The fixture covers the
//! poked stage-4 window, oracle frames 297-1797: `--stage 4` pokes `$26`, so
//! the oracle enters stage 4 at 292 and the native `Game::new_at_stage`
//! starts five frames later at 297 (`STAGE_BOOT_LEAD`), the first frame the
//! harness pairs. The recording used the harness's default input script and
//! no keep-alives, the same settings as the `--stage 4` diagnostic run in
//! `docs/analysis/stage4_boot.md`.
//!
//! Regenerate the fixture from a cartridge with:
//!
//! ```sh
//! ./target/release/gradius-diff --rom "$ROM" \
//!   --emit-fixture tests/fixtures/stage4-297-1797.state \
//!   --stage 4 --frames 1798 --from 297 --to 1797
//! ```
//!
//! The `--frames` value matters: the default script is generated from the run
//! length, so the recording must use the length this test builds with
//! [`RECORDED_FRAMES`] (its blocks are aligned so any length covering frame
//! 1797 gives the same input inside the window).
//!
//! The gate is `stage4_replay_is_exact_outside_the_respawn_frames`: every field
//! and object slot must match except `px`/`py` on the respawn frames, where the
//! port's fresh-life init leaves the ship at 0,0 while the oracle keeps the last
//! position. That residue is the shared ship-init model, not the stage-4 boot;
//! see the accepted-residue ledger in `docs/KNOWN_DEVIATIONS.md`. The other
//! test prints the full summary for diagnostics.

use std::collections::BTreeSet;
use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The oracle frame the harness pairs with native frame 1.
const FIRST_FRAME: u64 = 297;
/// The last compared frame.
const LAST_FRAME: u64 = 1797;
/// The stage-4 zero-page section (`$19 = 3`).
const STAGE_INDEX: u8 = 3;
/// Native frame 1 is the oracle's frame 297.
const FRAME_OFFSET: u64 = FIRST_FRAME - 1;
/// The recording was made with `--frames 1798`; the default script is a
/// function of the run length, so the test must build the same one.
const RECORDED_FRAMES: u64 = LAST_FRAME + 1;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Replay the recorded window and compare it over the full 1501 frames. Returns
/// the report and the native snapshots it compared.
fn replay() -> (Report, Vec<Snapshot>) {
    let fixture_path = root().join("tests/fixtures/stage4-297-1797.state");
    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script = default_script(RECORDED_FRAMES);

    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for snapshot in &oracle {
        // The harness loop is 0-based and steps the oracle before sampling it,
        // so oracle frame N was run with the buttons of harness frame N - 1.
        game.step(core_buttons(script.buttons(snapshot.frame - 1)));
        let mut native_snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        native_snapshot.frame += FRAME_OFFSET;
        native.push(native_snapshot);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, LAST_FRAME)),
            ..CompareConfig::default()
        },
    );
    (report, native)
}

#[test]
fn stage4_replay_reports_the_first_divergence() {
    let (report, _) = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));
    // The window must exercise the simulation, not just a static frame.
    for field in ["px", "py", "scroll", "score", "obj.kind", "obj.x", "obj.y"] {
        assert!(
            report.fields[field].checks > 0,
            "the fixture does not exercise {field}"
        );
    }
    assert!(report.object_slot_pairs > 0, "no object slots compared");

    eprintln!("stage-4 fixture replay:");
    eprintln!("{}", report.summary());
    match report.first() {
        Some(first) => eprintln!(
            "first divergence: frame {} {} (oracle {}, native {})",
            first.frame, first.field, first.oracle, first.native
        ),
        None => eprintln!("first divergence: none"),
    }
}

/// The gate: the whole window must match except for the ship's `px`/`py` on the
/// respawn frames. Those frames are the ones the port itself labels `Other(4)`
/// (the `$1B` init values 1-4): the oracle keeps the ship at its previous
/// position while a new life is still at 0,0 in the port, so both positions
/// differ by construction. That is the shared ship-init model, not the stage-4
/// boot, and it is the residue class to record in `docs/KNOWN_DEVIATIONS.md`.
/// Any divergence in another field, or any frame outside this set, fails.
#[test]
fn stage4_replay_is_exact_outside_the_respawn_frames() {
    let (report, native) = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);

    let respawn_frames: BTreeSet<u64> = native
        .iter()
        .filter(|snapshot| snapshot.state == "Other(4)")
        .map(|snapshot| snapshot.frame)
        .collect();
    let diverged_frames: BTreeSet<u64> = report
        .divergences
        .iter()
        .map(|divergence| divergence.frame)
        .collect();

    for divergence in &report.divergences {
        assert!(
            divergence.field == "px" || divergence.field == "py",
            "unexpected {} divergence at frame {} (oracle {}, native {})",
            divergence.field,
            divergence.frame,
            divergence.oracle,
            divergence.native
        );
        assert!(
            respawn_frames.contains(&divergence.frame),
            "{} divergence at frame {} is not a respawn frame",
            divergence.field,
            divergence.frame
        );
    }
    // The residue is exactly `px` and `py` on every respawn frame: 81 frames,
    // 162 divergences. Losing the residue is a win, but it must update this
    // gate (and the ledger) rather than silently widen it.
    assert_eq!(respawn_frames.len(), 81);
    assert_eq!(diverged_frames, respawn_frames);
    assert_eq!(report.divergences.len(), respawn_frames.len() * 2);
    for snapshot in native.iter().filter(|s| respawn_frames.contains(&s.frame)) {
        assert_eq!(snapshot.state, "Other(4)");
        assert_eq!(
            (snapshot.px, snapshot.py),
            (0, 0),
            "frame {}",
            snapshot.frame
        );
    }
}
