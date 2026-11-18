//! Replay the checked-in stage-2 tail fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack. The fixture covers frames
//! 12161-15000, recorded from the boss script with the analysis-only oracle
//! keep-alive. The run replays from reset and keeps the native player alive as
//! well, so the natural stage-1 -> stage-2 handoff and the later stage-2 play
//! are carried into the window exactly as the harness produced it.
//!
//! The window is inside stage 2: stage 3 starts at 19429, so this fixture is a
//! stage-2 tail, not a stage-3 window. It is exact (0 divergences).

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
fn native_port_matches_the_recorded_stage_two_tail() {
    let fixture_path = root().join("tests/fixtures/stage2-12161-15000.state");
    let script_path = root().join("scripts/boss-script.txt");

    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script_text = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", script_path.display()));
    let script = parse_script(&script_text).expect("malformed script");

    assert_eq!(oracle.len(), 2840, "fixture frame count");
    assert_eq!(oracle.first().map(|s| s.frame), Some(12161));
    assert_eq!(oracle.last().map(|s| s.frame), Some(15000));
    // The window is continuous stage-2 play: it never reaches the stage-3
    // scroll reset at 19429.
    assert!(oracle.iter().all(|snapshot| snapshot.state == "Playing"));
    assert!(oracle.iter().any(|snapshot| snapshot.scroll_high >= 5));
    assert!(oracle.iter().all(|snapshot| snapshot.scroll_high < 14));

    let mut game = Game::new(testdata::assets());
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for index in 0..15000u64 {
        game.step(core_buttons(script.buttons(index)));
        if index >= 12160 {
            native.push(Snapshot::parse(&game.dump_state()).expect("native state line"));
        }
    }
    assert_eq!(native.len(), 2840);

    // `state` is not a comparator field: the explicit window compares every
    // frame whatever state each side reports, so check it here.
    for (recorded, replays) in oracle.iter().zip(&native) {
        assert_eq!(recorded.state, replays.state, "state at {}", recorded.frame);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((12161, 15000)),
            ..CompareConfig::default()
        },
    );

    assert!(
        report.within_tolerance(),
        "native port diverged from the recorded stage-2 tail:\n{}",
        report.detailed()
    );
    assert_eq!(report.divergences.len(), 0);
    assert_eq!(report.frames, 2840);
    assert_eq!(report.compared, 2840);
    assert_eq!(report.requested_window, Some((12161, 15000)));
    // The window must exercise stage-2 play: ship, scroll, score and objects.
    assert!(report.fields["px"].checks > 0);
    assert!(report.fields["py"].checks > 0);
    assert!(report.fields["scroll"].checks > 0);
    assert!(report.fields["score"].checks > 0);
    assert!(report.fields["obj.kind"].checks > 0);
    assert!(report.fields["obj.x"].checks > 0);
    assert!(report.fields["obj.y"].checks > 0);
    assert!(report.fields["obj.substate"].checks > 0);
}
