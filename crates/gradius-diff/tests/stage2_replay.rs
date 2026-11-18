//! Replay the checked-in stage-2 oracle fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack. The fixture covers the
//! stage-2 handoff window, frames 9601-12000, recorded from the boss script
//! with both analysis-only keep-alives. The replay therefore starts from reset
//! and keeps the native player alive as well: the stage-2 entry carries the
//! stage-1 scroll fraction, score, ship position and (cleared) object pages,
//! so `Game::new_at_stage` cannot reproduce that handoff from a clean stage
//! boot. It is exercised separately below for its own boot contract.

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
fn native_port_matches_the_recorded_stage_two_window() {
    let fixture_path = root().join("tests/fixtures/stage2-9601-12000.state");
    let script_path = root().join("scripts/boss-script.txt");

    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script_text = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", script_path.display()));
    let script = parse_script(&script_text).expect("malformed script");

    assert_eq!(oracle.len(), 2400, "fixture frame count");
    assert_eq!(oracle.first().map(|s| s.frame), Some(9601));
    assert_eq!(oracle.last().map(|s| s.frame), Some(12000));
    // The window really is stage-2 play: the first frame is after the stage
    // reset and the fixture reaches the stage's own scroll range.
    assert!(oracle.iter().all(|snapshot| snapshot.state == "Playing"));
    assert!(oracle.iter().any(|snapshot| snapshot.scroll_high >= 4));

    let mut game = Game::new(testdata::assets());
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for index in 0..12000u64 {
        game.step(core_buttons(script.buttons(index)));
        if index >= 9600 {
            native.push(Snapshot::parse(&game.dump_state()).expect("native state line"));
        }
    }
    assert_eq!(native.len(), 2400);

    // `state` is not a comparator field: the explicit window compares every
    // frame whatever state each side reports, so check it here.
    for (recorded, replays) in oracle.iter().zip(&native) {
        assert_eq!(recorded.state, replays.state, "state at {}", recorded.frame);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((9601, 12000)),
            ..CompareConfig::default()
        },
    );

    assert!(
        report.within_tolerance(),
        "native port diverged from the recorded stage-2 window:\n{}",
        report.detailed()
    );
    assert_eq!(report.divergences.len(), 0);
    assert_eq!(report.frames, 2400);
    assert_eq!(report.compared, 2400);
    assert_eq!(report.requested_window, Some((9601, 12000)));
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

#[test]
fn new_at_stage_boots_the_stage_two_rows() {
    let game = Game::new_at_stage(testdata::assets(), 1);
    assert_eq!(game.section(), 1, "`$19 = 1` is stage 2");
    assert_eq!(game.stage_info().index, 1);
    assert_eq!(game.stage_music(), 0x59);
    assert!(game.dump_state().contains("state=Playing"));
}
