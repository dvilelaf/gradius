//! Replay the checked-in stage-3 fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack. The fixture covers the
//! natural stage-3 window, frames 19429-23000, recorded from the boss script
//! with both analysis-only keep-alives. The run replays from reset and keeps
//! the native player alive as well, so the stage-2 -> stage-3 handoff is
//! carried into the window exactly as the harness produced it.
//!
//! The port matches the whole window: every compared field and object slot is
//! exact over 19429-23000, so this test demands a green window.

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::parse_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn native_port_replays_the_recorded_stage_three_window() {
    let fixture_path = root().join("tests/fixtures/stage3-19429-23000.state");
    let script_path = root().join("scripts/boss-script.txt");

    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script_text = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", script_path.display()));
    let script = parse_script(&script_text).expect("malformed script");

    assert_eq!(oracle.len(), 3572, "fixture frame count");
    assert_eq!(oracle.first().map(|s| s.frame), Some(19429));
    assert_eq!(oracle.last().map(|s| s.frame), Some(23000));
    // The window is continuous stage-3 play: it starts on the stage reset and
    // reaches the stage's own scroll range.
    assert!(oracle.iter().all(|snapshot| snapshot.state == "Playing"));
    assert!(oracle.iter().any(|snapshot| snapshot.scroll_high >= 4));

    let mut game = Game::new(testdata::assets());
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for index in 0..23000u64 {
        game.step(core_buttons(script.buttons(index)));
        if index >= 19428 {
            native.push(Snapshot::parse(&game.dump_state()).expect("native state line"));
        }
    }
    assert_eq!(native.len(), 3572);

    // `state` is not a comparator field: the explicit window compares every
    // frame whatever state each side reports, so check it here.
    for (recorded, replays) in oracle.iter().zip(&native) {
        assert_eq!(recorded.state, replays.state, "state at {}", recorded.frame);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((19429, 23000)),
            ..CompareConfig::default()
        },
    );

    assert_eq!(report.frames, 3572);
    assert_eq!(report.compared, 3572);
    assert_eq!(report.requested_window, Some((19429, 23000)));
    // The window exercises the ship, score and object fields.
    assert!(report.fields["px"].checks > 0);
    assert!(report.fields["scroll"].checks > 0);
    assert!(report.fields["score"].checks > 0);
    assert!(report.fields["obj.kind"].checks > 0);

    match report.first() {
        Some(first) => panic!(
            "stage-3 state diverged at frame {}: {} (oracle {}, native {})",
            first.frame, first.field, first.oracle, first.native
        ),
        None => assert!(report.within_tolerance()),
    }
}
