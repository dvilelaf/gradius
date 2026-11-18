//! Replay the ending fixture (oracle frames 7015-8290) against the native port.
//!
//! The fixture is the `xtask record-ending` run: stage 7, fire held on every
//! frame, Start at 180 for six frames and the oracle player kept alive. The
//! same input is checked in as `ending-7015-8290.script`, and the recording was
//! made with:
//!
//! ```sh
//! ROM="$GRADIUS_ROM"
//! cargo xtask record-ending "$ROM" --stage 7 --frames 8290 \
//!   --out /tmp/opencode/endfixture/ending7-8290.txt
//! ./target/release/gradius-diff --rom "$ROM" \
//!   --emit-fixture tests/fixtures/ending-7015-8290.state \
//!   --script tests/fixtures/ending-7015-8290.script \
//!   --stage 7 --frames 8290 --from 7015 --to 8290 --keep-alive
//! ```
//!
//! The trace was trimmed to the same window in `ending-7015-8290.trace`; it
//! carries the `$1B` (`phase`), `$19` (`stage`), `$28` (`loop`) and `$26`
//! (`stagebyte`) evidence the state fixture cannot. The oracle reaches the
//! ending at 7024 (`phase=$87`), runs `$8C` with the +10000 score and lives
//! extend at 7998, returns to stage 1 at 8254-8282 and is playable again at
//! 8282 (`phase=$80`, `loop=1`, score 10830, lives 4).
//!
//! The replay starts from [`Game::new_at_ending`], the measured 7024 `$87`
//! state (`score=830`, `lives=3`, `scroll=0:0:128`, `meter=1`, `loop=1`,
//! `stage=6`). A full stage-7 replay cannot reach 7024 (it is ~64 px behind by
//! 5801 and `advance_section()` clamps at stage 7), so only the ending window
//! from 7025 is compared. The trace's `phase`/`loop` fields pin the `$87`-`$8D`
//! boundaries in `the_trace_is_the_measured_ending_window`.

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::script::{parse_script, Script};
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The first compared frame: the seed is the 7024 `$87` state, so the first
/// stepped frame is `$88` on 7025.
const FIRST_FRAME: u64 = 7025;
/// The last compared frame: stage 1 playable again after the loop.
const LAST_FRAME: u64 = 8290;
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_fixture(name: &str) -> String {
    let path = root().join("tests/fixtures").join(name);
    gradius_diff::fixture::read_fixture(&path).unwrap_or_else(|error| panic!("{error}"))
}

fn ending_script() -> Script {
    parse_script(&read_fixture("ending-7015-8290.script")).expect("malformed ending script")
}

/// Replay the ending from the seeded 7024 state and compare 7025-8290.
fn replay() -> Report {
    let oracle =
        parse_fixture(&read_fixture("ending-7015-8290.state")).expect("malformed ending fixture");
    let script = ending_script();

    let mut game = Game::new_at_ending(testdata::assets());

    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for frame in FIRST_FRAME..=LAST_FRAME {
        // The harness loop is 0-based and steps the oracle before sampling it,
        // so oracle frame N was run with the buttons of harness frame N - 1.
        game.step(core_buttons(script.buttons(frame - 1)));
        let mut snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        snapshot.frame = frame;
        native.push(snapshot);
    }
    assert_eq!(
        native.len(),
        oracle.len() - (FIRST_FRAME - 7015) as usize,
        "window length"
    );

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle
        .iter()
        .filter(|snapshot| snapshot.frame >= FIRST_FRAME)
        .zip(&native)
        .collect();
    compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, LAST_FRAME)),
            // The ending ship and explosion are drawn straight into OAM
            // (`$987F`/`$BB09`); the oracle's ending slots are not part of
            // `dump_state`, so the object table is not compared here. Pixels
            // cover the ship, and `ending_replay` gates the phase machine,
            // text timing and +10000.
            compare_objects: false,
            ..CompareConfig::default()
        },
    )
}

/// One `key=value` field of a trace line.
fn trace_field(trace: &str, frame: u64, key: &str) -> String {
    let prefix = format!("frame={frame} ");
    let line = trace
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("trace has no frame {frame}"));
    let needle = format!("{key}=");
    line.split_whitespace()
        .find_map(|field| field.strip_prefix(&needle))
        .unwrap_or_else(|| panic!("frame {frame} has no {key}"))
        .to_string()
}

/// The oracle trace pins the ending boundaries the state fixture alone cannot
/// show: `$1B` (`phase`), `$19` (`stage`), `$28` (`loop`) and `$26`.
#[test]
fn the_trace_is_the_measured_ending_window() {
    let trace = read_fixture("ending-7015-8290.trace");

    // The stage-7 `$82` clear timer is still holding at the window start.
    assert_eq!(trace_field(&trace, 7015, "phase"), "82");
    assert_eq!(trace_field(&trace, 7015, "stage"), "6");
    assert_eq!(trace_field(&trace, 7015, "loop"), "0");

    // `$9B38`: stage byte cleared, loop incremented, scroll reset.
    assert_eq!(trace_field(&trace, 7024, "phase"), "87");
    assert_eq!(trace_field(&trace, 7024, "stage"), "6");
    assert_eq!(trace_field(&trace, 7024, "stagebyte"), "0");
    assert_eq!(trace_field(&trace, 7024, "loop"), "1");
    assert_eq!(trace_field(&trace, 7024, "scroll"), "0:0:128");

    // `$88`/`$89`/`$8A`/`$8B`/`$8C`: black setup, then the display window.
    for (frame, phase) in [(7025, "88"), (7027, "89"), (7028, "8A"), (7029, "8B")] {
        assert_eq!(trace_field(&trace, frame, "phase"), phase, "frame {frame}");
    }
    assert_eq!(trace_field(&trace, 7051, "phase"), "8B");
    assert_eq!(trace_field(&trace, 7052, "phase"), "8C");
    assert_eq!(trace_field(&trace, 8253, "phase"), "8C");

    // `$8444` from the ending ship: +10000 and the score extend at 7998.
    assert_eq!(trace_field(&trace, 7997, "score"), "830");
    assert_eq!(trace_field(&trace, 7997, "lives"), "3");
    assert_eq!(trace_field(&trace, 7998, "score"), "10830");
    assert_eq!(trace_field(&trace, 7998, "lives"), "4");

    // `$8D` clears `$1B`, `$9B38` restarts stage 1, and 8282 is playable.
    assert_eq!(trace_field(&trace, 8254, "phase"), "8D");
    assert_eq!(trace_field(&trace, 8282, "phase"), "80");
    assert_eq!(trace_field(&trace, 8282, "stage"), "0");
    assert_eq!(trace_field(&trace, 8282, "loop"), "1");
    assert_eq!(trace_field(&trace, 8282, "score"), "10830");
    assert_eq!(trace_field(&trace, 8282, "lives"), "4");
}

/// The state fixture is aligned with the trace on the fields both carry.
#[test]
fn the_state_fixture_covers_the_same_ending() {
    let oracle =
        parse_fixture(&read_fixture("ending-7015-8290.state")).expect("malformed ending fixture");
    assert_eq!(oracle.len() as u64, LAST_FRAME - 7015 + 1);
    assert_eq!(oracle.first().unwrap().frame, 7015);
    assert_eq!(oracle.last().unwrap().frame, LAST_FRAME);

    let at = |frame: u64| {
        oracle
            .iter()
            .find(|snapshot| snapshot.frame == frame)
            .unwrap_or_else(|| panic!("fixture has no frame {frame}"))
    };
    assert_eq!(at(7024).scroll_low, 0);
    assert_eq!(at(7024).scroll_high, 0);
    assert_eq!(at(7024).scroll_frac, 128);
    assert_eq!(at(7997).score, 830);
    assert_eq!(at(7998).score, 10830);
    assert_eq!(at(7998).lives, 4);
    assert_eq!(at(8282).state, "Playing");
    assert_eq!(at(8290).scroll_high, 0);
}

/// The seeded ending must reproduce the measured 7025-8290 window: the
/// `$87`-`$8D` sequence, the +10000 at 7998 and the return to stage 1.
#[test]
fn ending_replay_matches_the_seeded_ending_window() {
    let report = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));
    // The window must exercise the simulation, not just a static frame.
    for field in ["px", "py", "scroll", "score", "lives"] {
        assert!(
            report.fields[field].checks > 0,
            "the fixture does not exercise {field}"
        );
    }

    if let Some(first) = report.first() {
        panic!(
            "first divergence: frame {} {} (oracle {}, native {})\n{}",
            first.frame,
            first.field,
            first.oracle,
            first.native,
            report.summary()
        );
    }
    assert!(report.within_tolerance(), "{}", report.summary());
}
