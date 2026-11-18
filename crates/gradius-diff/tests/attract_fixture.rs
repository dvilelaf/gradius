//! Replay the checked-in attract/demo oracle fixture against the native port.
//!
//! The fixture covers frames 130-800 of `scripts/attract-800.txt`, recorded
//! with `gradius-diff --emit-fixture --from 130 --to 800`. Like the stage-1
//! replay it needs no cartridge, only the extracted asset pack. The compared
//! state does not depend on the art.

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::parse_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

#[test]
fn native_port_matches_the_recorded_attract_fixture() {
    let dir = fixture_dir();
    let fixture_path = dir.join("attract-800.state");
    let script_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/attract-800.txt");

    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script_text = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", script_path.display()));
    let script = parse_script(&script_text).expect("malformed script");

    assert_eq!(oracle.len(), 671, "fixture frame count");
    assert_eq!(oracle.first().map(|s| s.frame), Some(130));
    assert_eq!(oracle.last().map(|s| s.frame), Some(800));
    // The window spans the whole attract sequence, not only demo play.
    for state in ["Title", "Other(1)", "Other(2)"] {
        assert!(
            oracle.iter().any(|snapshot| snapshot.state == state),
            "fixture is missing the {state} state"
        );
    }

    // Replay from reset: the attract window's state depends on the title and
    // transition frames that precede it.
    let mut game = Game::new(testdata::assets());
    let mut native: Vec<Snapshot> = Vec::with_capacity(671);
    for index in 0..800 {
        game.step(core_buttons(script.buttons(index)));
        if index >= 129 {
            native.push(Snapshot::parse(&game.dump_state()).expect("native state line"));
        }
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((130, 800)),
            ..CompareConfig::default()
        },
    );

    assert!(
        report.within_tolerance(),
        "native port diverged from the recorded attract window:\n{}",
        report.detailed()
    );
    assert_eq!(report.divergences.len(), 0);
    assert_eq!(report.frames, 671);
    assert_eq!(report.compared, 671);
    assert_eq!(report.requested_window, Some((130, 800)));
    // The window must exercise the demo ship, the scroll and the object table.
    assert!(report.fields["px"].checks > 0);
    assert!(report.fields["scroll"].checks > 0);
    assert!(report.fields["obj.kind"].checks > 0);
    assert!(report.fields["obj.substate"].checks > 0);
    assert!(report.fields["obj.x"].checks > 0);
    assert!(report.fields["obj.y"].checks > 0);
}
