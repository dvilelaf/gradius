//! Replay the checked-in stage-5 fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack. The fixture covers the
//! poked stage-5 window, oracle frames 297-8573: `--stage 5` pokes `$26`, so
//! the oracle enters stage 5 at 292 and the native `Game::new_at_stage`
//! starts five frames later at 297 (`STAGE_BOOT_LEAD`), the first frame the
//! harness pairs. The recording used the harness's default input script and the
//! oracle keep-alive, the same settings as the `--stage 5` diagnostic run in
//! `docs/analysis/stage5_boot.md`.
//!
//! Regenerate the fixture from a cartridge with:
//!
//! ```sh
//! ./target/release/gradius-diff --rom "$ROM" \
//!   --emit-fixture tests/fixtures/stage5-297-8573.state \
//!   --stage 5 --frames 8573 --from 297 --to 8573 --keep-alive
//! ```
//!
//! `--keep-alive` pokes the oracle and so is part of the fixture; the replay
//! also enables the port's `Game::dev_keep_alive(true)`, the diagnostic run's
//! `--native-keep-alive`, or a native death would freeze scroll and lives while
//! the kept-alive oracle plays on.
//!
//! The `--frames` value matters: the default script is generated from the run
//! length, so the recording must use the length this test builds with.
//!
//! The window is exact: it covers the `$14` cannons, their `$0600` slots and
//! the `$19 == 4`/`$5C >= 2` alternate chain that halves the scroll to
//! 0.25 px/frame. The core's `$6C` fix closed the earlier missing-`$8E`
//! residue and the `$9A69` tail tick (`$CB84`) fixed the cannon's death frame:
//! it now lands on the oracle's 3055 (score 90) instead of 3043. The first
//! divergence after the window is 8574 `obj.x` slot 9 (oracle 109, native
//! 111), so the exact gate stops one frame short of it.

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The oracle frame the harness pairs with native frame 1.
const FIRST_FRAME: u64 = 297;
/// The last compared frame of the exact gate window.
const LAST_FRAME: u64 = 8573;
/// The recorded fixture for [`FIRST_FRAME`]-[`LAST_FRAME`].
const FIXTURE: &str = "tests/fixtures/stage5-297-8573.state";
/// The stage-5 zero-page section (`$19 = 4`).
const STAGE_INDEX: u8 = 4;
/// Native frame 1 is the oracle's frame 297.
const FRAME_OFFSET: u64 = FIRST_FRAME - 1;
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Read one recorded fixture.
fn load_fixture(fixture: &str) -> Vec<Snapshot> {
    let fixture_path = root().join(fixture);
    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    parse_fixture(&text).expect("malformed fixture")
}

/// Replay the recorded window and compare it over the full 8277 frames.
///
/// The recording was made with `--frames 8573` (`LAST_FRAME`) because the
/// default script is a function of the run length, so the replay builds the
/// same one.
fn replay() -> Report {
    replay_fixture(FIXTURE, LAST_FRAME, LAST_FRAME)
}

/// Replay one recorded window; `recorded` is the run length the fixture was
/// generated with, which fixes the default input script.
fn replay_fixture(fixture: &str, recorded: u64, last: u64) -> Report {
    replay_oracle(&load_fixture(fixture), recorded, last)
}

/// Step the native port over `oracle` and compare over [`FIRST_FRAME`]-`last`.
fn replay_oracle(oracle: &[Snapshot], recorded: u64, last: u64) -> Report {
    let script = default_script(recorded);

    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    // The diagnostic run uses `--native-keep-alive`: without it a native death
    // freezes scroll and lives while the kept-alive oracle plays on.
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for snapshot in oracle {
        // The harness loop is 0-based and steps the oracle before sampling it,
        // so oracle frame N was run with the buttons of harness frame N - 1.
        game.step(core_buttons(script.buttons(snapshot.frame - 1)));
        let mut native_snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        native_snapshot.frame += FRAME_OFFSET;
        native.push(native_snapshot);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, last)),
            ..CompareConfig::default()
        },
    )
}

/// Diagnostics: assert the comparison covers the whole window and exercises
/// the simulation, then publish the summary and the first divergence.
#[test]
fn stage5_replay_reports_the_first_divergence() {
    let report = replay();
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

    eprintln!("stage-5 fixture replay:");
    eprintln!("{}", report.summary());
    match report.first() {
        Some(first) => eprintln!(
            "first divergence: frame {} {} (oracle {}, native {})",
            first.frame, first.field, first.oracle, first.native
        ),
        None => eprintln!("first divergence: none"),
    }
}

/// The gate: the whole 297-8573 window is exact.
///
/// It covers the `$14` large object (`$CA58`), its `$0600` cannons (`$A4A0`
/// slot allocation, the `$CB8B` fire cadence, the 3055 kill through `$CB84`),
/// the `$C64D` script and the `$9658` `$19 == 4`/`$5C >= 2` chain (0.25 px/frame
/// scroll and drift). The earlier missing-`$8E` residue is closed by the
/// core's `$6C` fix. The window ends just before the port's first divergence,
/// frame 8574 `obj.x` slot 9 (oracle 109, native 111).
#[test]
fn stage5_replay_is_exact() {
    let report = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));

    assert!(
        report.within_tolerance(),
        "the stage-5 window diverges at {:?}",
        report.first()
    );

    for field in [
        "frame",
        "px",
        "py",
        "speed",
        "scroll",
        "score",
        "lives",
        "meter",
        "obj.kind",
        "obj.substate",
        "obj.x",
        "obj.y",
    ] {
        let stat = report.fields.get(field).unwrap_or_else(|| {
            panic!("{field} was not compared");
        });
        assert!(stat.checks > 0, "the fixture does not exercise {field}");
        assert_eq!(stat.passes, stat.checks, "{field} diverges");
    }

    // Every field that was checked must pass, including the object table; the
    // coverage counts pin the window so a shorter replay cannot pass silently.
    for (field, stat) in &report.fields {
        assert_eq!(stat.passes, stat.checks, "{field} diverges");
    }
    assert_eq!(report.object_slot_matches, 23481);
    assert_eq!(report.object_slot_pairs, 23481);
    assert_eq!(report.object_count_delta, 0);
    assert_eq!(report.max_object_count_delta, 0);
}

/// A divergence injected in the fixture's second half must fail the gate.
///
/// The tail of the window (past the cannon kill and the 5500+ scroll regime)
/// is what the widened gate adds, so move a late slot-9 object by two pixels,
/// the size of the real 8574 `obj.x` drift. The comparison must report it at
/// that frame, not silently pass or stop early.
#[test]
fn stage5_injected_late_divergence_is_caught() {
    let injected = 8400;
    let mut oracle = load_fixture(FIXTURE);
    let snapshot = oracle
        .iter_mut()
        .find(|snapshot| snapshot.frame == injected)
        .expect("injected frame recorded");
    let object = snapshot
        .objects
        .iter_mut()
        .find(|object| object.slot == 9)
        .expect("slot 9 live at the injected frame");
    object.x += 2;

    let report = replay_oracle(&oracle, LAST_FRAME, LAST_FRAME);
    assert_eq!(report.compared, LAST_FRAME - FIRST_FRAME + 1);
    assert!(!report.within_tolerance(), "the injected divergence passed");
    let first = report.first().expect("injected divergence reported");
    assert_eq!((first.frame, first.field.as_str()), (injected, "obj.x"));
}
