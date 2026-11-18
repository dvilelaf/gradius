//! Exact replay of the stage-5 clear, oracle frames 8574-12003.
//!
//! This is the second half of the stage-5 phase: `stage5_replay` gates
//! 297-8573 from its own fixture and this one gates the clear from 8574 to the
//! `$96C4` `INC $19` into stage 6 at 12003. The split leaves the 297-8573
//! gate and its 1.46 MB fixture untouched; the two files together are 1.9 MB,
//! the same as one continuous 297-12003 fixture would be, so merging them
//! would save nothing and would rewrite the older gate. Recording command:
//!
//! ```sh
//! ROM="$GRADIUS_ROM"
//! CARGO_TARGET_DIR=/tmp/opencode/target-s5clearfx cargo build --release -p gradius-diff
//! /tmp/opencode/target-s5clearfx/release/gradius-diff --rom "$ROM" \
//!   --emit-fixture tests/fixtures/stage5-8574-12003.state \
//!   --stage 5 --frames 12003 --from 8574 --to 12003 --keep-alive
//! ```
//!
//! `--frames 12003` matters twice: the default input script is generated from
//! the run length, and the 1-based oracle frame counter must reach 12003. The
//! replay therefore builds `default_script(12003)` and steps the native port
//! from its frame 1 (oracle 297) through 12003, recording only the frames the
//! fixture carries. `--keep-alive` pokes the oracle (so it is part of the
//! recording) and the replay enables `Game::dev_keep_alive(true)`, like the
//! diagnostic run's `--native-keep-alive`.
//!
//! The recorded window holds the stage-5 clear landmarks from
//! [stage5_boot.md](../../../docs/analysis/stage5_boot.md) §6: the `$3F = $0C`
//! core spawn at 10274, its death at 11489 and the `$96C4` `INC $19` into
//! stage 6 at 12003 (the 514-frame clear). The fixture is 3430 frames, 552 KB;
//! the full window fits, so no key tramo was dropped.
//!
//! The window is exact: 3430 frames, every field at 100% and 7382/7382 object
//! slots. `stage5_injected_clear_divergence_is_caught` checks the gate is not
//! vacuous by moving a slot-9 `obj.x` two pixels at 11000, inside the clear.

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The first recorded frame: the exact gate ends one frame before it.
const FIRST_FRAME: u64 = 8574;
/// The last recorded frame: stage 6 starts here (`$96C4`).
const LAST_FRAME: u64 = 12003;
/// The run length the fixture was recorded with; fixes the default script.
const RECORDED_FRAMES: u64 = LAST_FRAME;
/// The oracle frame the harness pairs with native frame 1.
const FIRST_PAIRED: u64 = 297;
/// The stage-5 zero-page section (`$19 = 4`).
const STAGE_INDEX: u8 = 4;
/// The recorded fixture for [`FIRST_FRAME`]-[`LAST_FRAME`].
const FIXTURE: &str = "tests/fixtures/stage5-8574-12003.state";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_fixture() -> Vec<Snapshot> {
    let path = root().join(FIXTURE);
    let text = read_fixture(&path).unwrap_or_else(|error| panic!("{error}"));
    parse_fixture(&text).expect("malformed fixture")
}

/// Step the native port from oracle 297 to [`LAST_FRAME`] and compare the
/// recorded window. The warm-up from 8574 back to 297 is needed because the
/// fixture starts mid-run and the core has no state seed for that frame.
fn replay() -> Report {
    replay_from(&load_fixture())
}

fn replay_from(oracle: &[Snapshot]) -> Report {
    let script = default_script(RECORDED_FRAMES);

    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    // The diagnostic run uses `--native-keep-alive`: without it a native death
    // freezes scroll and lives while the kept-alive oracle plays on.
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for frame in FIRST_PAIRED..=LAST_FRAME {
        // The harness loop is 0-based and steps the oracle before sampling it,
        // so oracle frame N was run with the buttons of harness frame N - 1.
        game.step(core_buttons(script.buttons(frame - 1)));
        if frame >= FIRST_FRAME {
            let mut snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
            snapshot.frame = frame;
            native.push(snapshot);
        }
    }
    assert_eq!(native.len(), oracle.len(), "replayed frame count");

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, LAST_FRAME)),
            ..CompareConfig::default()
        },
    )
}

/// The recording is the measured clear: 3430 frames, the core spawn, its death
/// and the stage-6 handoff. Pins the provenance the test header cites.
#[test]
fn the_fixture_is_the_measured_clear_window() {
    let oracle = load_fixture();
    assert_eq!(oracle.len() as u64, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(oracle.first().unwrap().frame, FIRST_FRAME);
    assert_eq!(oracle.last().unwrap().frame, LAST_FRAME);

    let at = |frame: u64| {
        oracle
            .iter()
            .find(|snapshot| snapshot.frame == frame)
            .unwrap_or_else(|| panic!("fixture has no frame {frame}"))
    };

    // The window opens on the frame the exact gate stops before, mid-scroll.
    assert_eq!(at(FIRST_FRAME).state, "Playing");
    assert_eq!(at(FIRST_FRAME).scroll_high, 11);

    // 10274: the `$3F = $0C` core spawn, with the object table populated.
    assert!(!at(10274).objects.is_empty());
    // 11489: the core's death, still Played over; stage 6 starts at 12003
    // with the object table erased by the clear.
    assert_eq!(at(11489).state, "Playing");
    assert!(!at(11489).objects.is_empty());
    assert!(at(LAST_FRAME).objects.is_empty());
}

/// Diagnostics: assert the comparison covers the window and exercises the
/// simulation, then publish the summary and first divergence.
#[test]
fn the_clear_window_replay_reports_its_first_divergence() {
    let report = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));
    // The window must exercise the simulation, not just a static frame: the
    // ship, the scroll, the score and the object table all move in it.
    for field in ["px", "py", "scroll", "score", "obj.kind", "obj.x", "obj.y"] {
        assert!(
            report.fields[field].checks > 0,
            "the fixture does not exercise {field}"
        );
    }
    assert!(report.object_slot_pairs > 0, "no object slots compared");

    eprintln!("stage-5 clear replay ({FIRST_FRAME}-{LAST_FRAME}):");
    eprintln!("{}", report.summary());
    eprintln!(
        "object slots: {}/{} matched",
        report.object_slot_matches, report.object_slot_pairs
    );
    match report.first() {
        Some(first) => eprintln!(
            "first divergence: frame {} {} (oracle {}, native {})",
            first.frame, first.field, first.oracle, first.native
        ),
        None => eprintln!("first divergence: none"),
    }
}

/// The gate: the whole 8574-12003 clear window is exact.
///
/// It covers the `$3F = $0C` core spawn at 10274, its `$B95C`/`$BA96` death at
/// 11489 (+1000) and the 514-frame countdown to the `$96C4` `INC $19` into
/// stage 6 at 12003, where the object table is empty.
#[test]
fn stage5_clear_replay_is_exact() {
    let report = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));

    assert!(
        report.within_tolerance(),
        "the stage-5 clear diverges at {:?}",
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

    for (field, stat) in &report.fields {
        assert_eq!(stat.passes, stat.checks, "{field} diverges");
    }
    assert_eq!(report.object_slot_matches, 7382);
    assert_eq!(report.object_slot_pairs, 7382);
    assert_eq!(report.object_count_delta, 0);
    assert_eq!(report.max_object_count_delta, 0);
}

/// A divergence injected inside the clear must fail the gate.
///
/// 11000 is between the core spawn and its death, so the window cannot pass
/// without comparing the clear itself. The slot-9 `obj.x` moves two pixels,
/// the size of the real 8574 drift, and the comparison must report it there.
#[test]
fn stage5_injected_clear_divergence_is_caught() {
    let injected = 11000;
    let mut oracle = load_fixture();
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

    let report = replay_from(&oracle);
    assert_eq!(report.compared, LAST_FRAME - FIRST_FRAME + 1);
    assert!(!report.within_tolerance(), "the injected divergence passed");
    let first = report.first().expect("injected divergence reported");
    assert_eq!((first.frame, first.field.as_str()), (injected, "obj.x"));
}
