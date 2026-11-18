//! Replay the checked-in held-B power-up fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack: it reads the recorded state
//! lines under `tests/fixtures/`, drives `gradius_core::Game` with
//! `scripts/powerup-1200.txt` and compares both sides with the harness's own
//! rules and tolerances. The window is the whole Playing span after the start
//! transition, and the script holds B from frame 400 so each of the three
//! stage-1 capsules is activated as it arrives. That is what makes the run
//! protect the activation path: `speed` climbs with each redeemed SPEED
//! power-up and `$42` (the meter) stays at zero.

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
fn native_port_matches_the_recorded_held_b_powerup_fixture() {
    let fixture_path = root().join("tests/fixtures/powerup-1200.state");
    let script_path = root().join("scripts/powerup-1200.txt");

    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script_text = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", script_path.display()));
    let script = parse_script(&script_text).expect("malformed script");

    assert_eq!(oracle.len(), 908, "fixture frame count");
    assert_eq!(oracle.first().map(|s| s.frame), Some(292));
    assert_eq!(oracle.last().map(|s| s.frame), Some(1199));

    // The run must actually redeem the three capsules: each pickup advances
    // the meter and the held B immediately spends it on SPEED. Without that
    // the fixture would not exercise the activation path at all.
    assert!(
        oracle.iter().all(|snapshot| snapshot.meter == 0),
        "the meter should never stay charged while B is held"
    );
    assert_eq!(
        oracle.iter().map(|snapshot| snapshot.speed).max(),
        Some(3),
        "three SPEED power-ups should have been activated"
    );
    assert!(
        oracle.iter().any(|snapshot| snapshot.score > 0),
        "the run should destroy enemies and drop capsules"
    );

    // Replay from reset: the Playing window depends on the start transition
    // frames that precede it.
    let mut game = Game::new(testdata::assets());
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for index in 0..1200 {
        game.step(core_buttons(script.buttons(index)));
        if index >= 291 {
            native.push(Snapshot::parse(&game.dump_state()).expect("native state line"));
        }
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((292, 1199)),
            ..CompareConfig::default()
        },
    );

    assert!(
        report.within_tolerance(),
        "native port diverged from the recorded power-up window:\n{}",
        report.detailed()
    );
    assert_eq!(report.divergences.len(), 0);
    assert_eq!(report.frames, 908);
    assert_eq!(report.compared, 908);
    assert_eq!(report.requested_window, Some((292, 1199)));
    // The window must exercise the ship, the scroll, the object slots and the
    // power-up bookkeeping, not just empty play.
    assert!(report.fields["px"].checks > 0);
    assert!(report.fields["py"].checks > 0);
    assert!(report.fields["scroll"].checks > 0);
    assert!(report.fields["score"].checks > 0);
    assert!(report.fields["meter"].checks > 0);
    assert!(report.fields["obj.kind"].checks > 0);
    assert!(report.fields["obj.substate"].checks > 0);
    assert!(report.fields["obj.x"].checks > 0);
    assert!(report.fields["obj.y"].checks > 0);
}
