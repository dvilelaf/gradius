//! Stage-1 loop restart after the ending: the measured oracle window
//! 8282-9000, i.e. what the port does once `$8D` jumps to `$9B38`.
//!
//! The seed is the measured 7024 `$87` state and the inputs are the same
//! script as `ending_replay` (fire held, Start at 180). The oracle numbers
//! come from `xtask record-ending --stage 7 --frames 9100` and
//! `xtask probe --ram` (see `docs/analysis/loop.md`). Analysis only: this
//! test reports the looped stage-1 state, it does not gate pixels.

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::script::{parse_script, Script};
use gradius_diff::testdata;
use gradius_diff::{core_buttons, Snapshot};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ending_script() -> Script {
    let path = root().join("tests/fixtures/ending-7015-8290.script");
    parse_script(&std::fs::read_to_string(path).expect("ending script"))
        .expect("malformed ending script")
}

/// One sampled frame of the looped stage 1.
struct Row {
    frame: u64,
    snapshot: Snapshot,
    section: u8,
    music: u8,
}

#[test]
fn stage_one_loop_restart_after_the_ending() {
    let script = ending_script();
    let mut game = Game::new_at_ending(testdata::assets());
    let mut rows: Vec<Row> = Vec::new();
    for frame in 7025u64..=9000 {
        game.step(core_buttons(script.buttons(frame - 1)));
        let mut snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        snapshot.frame = frame;
        if [8254, 8282, 8287, 8290, 8600, 9000].contains(&frame) {
            rows.push(Row {
                frame,
                snapshot,
                section: game.section(),
                music: game.music(),
            });
        }
    }
    for row in &rows {
        println!(
            "port {}: state={} section={} music=${:02X} score={} lives={} scroll={}:{}:{} px={} py={}",
            row.frame,
            row.snapshot.state,
            row.section,
            row.music,
            row.snapshot.score,
            row.snapshot.lives,
            row.snapshot.scroll_high,
            row.snapshot.scroll_low,
            row.snapshot.scroll_frac,
            row.snapshot.px,
            row.snapshot.py
        );
    }
    let at = |frame: u64| {
        rows.iter()
            .find(|row| row.frame == frame)
            .unwrap_or_else(|| panic!("no frame {frame}"))
    };
    // The measured 8282 state: playable stage 1, score and lives kept; the
    // first scroll step is at 8287.
    let playable = at(8282);
    assert_eq!(playable.snapshot.state, "Playing");
    assert_eq!(playable.section, 0);
    assert_eq!(playable.snapshot.score, 10830);
    assert_eq!(playable.snapshot.lives, 4);
    assert_eq!(at(8287).snapshot.scroll_frac, 128);
    assert!(
        at(9000).snapshot.scroll_high > 0,
        "the looped stage 1 must scroll"
    );
}
