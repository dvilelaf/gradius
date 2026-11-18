//! Replay a stage-2 window in which the ship shoots the destructible pink balls.
//!
//! The scripted run holds fire while sweeping up and down the ball wall, so the
//! window covers dozens of breaks. Core state is ROM-exact until frame 19430;
//! `broken_cells` proves the breaks really happened in the replay.

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
fn native_port_matches_the_window_with_destroyed_balls() {
    let fixture_path = root().join("tests/fixtures/stage2-blocks-9601-19430.state");
    let script_path = root().join("scripts/stage2-blocks-script.txt");
    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script_text = std::fs::read_to_string(&script_path).expect("script");
    let script = parse_script(&script_text).expect("malformed script");
    assert_eq!(oracle.last().map(|s| s.frame), Some(19430));

    let mut game = Game::new(testdata::assets());
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::new();
    let mut broken = 0;
    for index in 0..19430u64 {
        game.step(core_buttons(script.buttons(index)));
        broken = broken.max(game.broken_cells());
        if index >= 9600 {
            native.push(Snapshot::parse(&game.dump_state()).expect("native state line"));
        }
    }
    assert!(broken > 20, "the replay must break balls");

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((9601, 19430)),
            tolerance_px: 0,
            ..CompareConfig::default()
        },
    );
    assert_eq!(report.divergences.len(), 0, "{}", report.detailed());
}
