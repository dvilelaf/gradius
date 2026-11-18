//! Replay the checked-in stage-1 oracle fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack: it reads the recorded state
//! lines under `tests/fixtures/`, drives `gradius_core::Game` with the same
//! input script and compares both sides with the harness's own rules and
//! tolerances. The fixture holds behavioural data only (see the header for the
//! source ROM SHA-256); the core renders with zeroed tiles, which is enough
//! because the compared state does not depend on the art.

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
fn native_port_matches_the_recorded_stage_one_fixture() {
    let dir = fixture_dir();
    let fixture_path = dir.join("stage1-600.state");
    let script_path = dir.join("stage1-600.script");

    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script_text = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", script_path.display()));
    let script = parse_script(&script_text).expect("malformed script");

    assert_eq!(oracle.len(), 600, "fixture frame count");
    assert_eq!(
        oracle.first().map(|s| s.frame),
        Some(1),
        "fixture starts at frame 1"
    );
    assert_eq!(
        oracle.last().map(|s| s.frame),
        Some(600),
        "fixture ends at frame 600"
    );

    let mut game = Game::new(testdata::assets());
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for index in 0..oracle.len() {
        game.step(core_buttons(script.buttons(index as u64)));
        native.push(Snapshot::parse(&game.dump_state()).expect("native state line"));
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(pairs, CompareConfig::default());

    assert!(
        report.within_tolerance(),
        "native port diverged from the recorded oracle:\n{}",
        report.detailed()
    );
    assert_eq!(report.frames, 600);
    assert!(report.compared > 0, "no Playing frames were compared");
    // The fixture must exercise positions, scroll and the object table, not
    // just the title frames.
    assert!(report.fields["px"].checks > 0);
    assert!(report.fields["scroll"].checks > 0);
    assert!(report.fields["obj.kind"].checks > 0);
    assert!(report.fields["obj.substate"].checks > 0);
}
