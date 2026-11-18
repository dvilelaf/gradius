//! Replay the whole natural stage-2 window against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack. The fixture covers frames
//! 12001-19430 recorded from the boss script with the analysis-only oracle
//! keep-alive: the stage-2 play after the gated hand-off (9601-12000), the
//! `$C540` eruption under the `$82` hold, the Big Core fight (spawn 17282,
//! self-destruct 18915) and the stage-3 entry at 19429. The replay starts from
//! reset and keeps the native player alive as well, so the natural stage-1 ->
//! stage-2 handoff is carried into the window exactly as the harness produced
//! it.
//!
//! The whole window is exact (0 divergences). The single fixture is 7,430
//! frames, in line with the checked-in stage-5 fixture (8,277 frames), so it
//! does not need the tramo split the card allowed.
//!
//! Regenerate (the ROM is only needed for this step):
//!
//! ```sh
//! export CARGO_TARGET_DIR=/tmp/opencode/target-s2gate
//! cargo build --release -p gradius-diff
//! "$CARGO_TARGET_DIR/release/gradius-diff" \
//!   --rom "$GRADIUS_ROM" \
//!   --emit-fixture tests/fixtures/stage2-12001-19430.state \
//!   --frames 19430 --from 12001 --to 19430 \
//!   --script scripts/boss-script.txt --keep-alive
//! ```

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::parse_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

const ENTRY: u64 = 12001;
const END: u64 = 19430;
const BOSS_KIND: i64 = 0x98;
const BOSS_SLOT: i64 = 9;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> Vec<Snapshot> {
    let fixture_path = root().join("tests/fixtures/stage2-12001-19430.state");
    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    parse_fixture(&text).expect("malformed fixture")
}

/// Replay from reset and keep the native player alive, like the recording run.
fn replay(frames: u64) -> Vec<Snapshot> {
    let script_path = root().join("scripts/boss-script.txt");
    let script_text = std::fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", script_path.display()));
    let script = parse_script(&script_text).expect("malformed script");

    let mut game = Game::new(testdata::assets());
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity((frames - ENTRY + 1) as usize);
    for index in 0..frames {
        game.step(core_buttons(script.buttons(index)));
        if index + 1 >= ENTRY {
            native.push(Snapshot::parse(&game.dump_state()).expect("native state line"));
        }
    }
    native
}

#[test]
fn native_port_matches_the_whole_natural_stage_two_window() {
    let oracle = fixture();
    assert_eq!(oracle.len(), 7430, "fixture frame count");
    assert_eq!(oracle.first().map(|s| s.frame), Some(ENTRY));
    assert_eq!(oracle.last().map(|s| s.frame), Some(END));
    assert!(oracle.iter().all(|snapshot| snapshot.state == "Playing"));

    let native = replay(END);
    assert_eq!(native.len(), 7430);

    // `state` is not a comparator field: the explicit window compares every
    // frame whatever state each side reports, so check it here.
    for (recorded, replays) in oracle.iter().zip(&native) {
        assert_eq!(recorded.state, replays.state, "state at {}", recorded.frame);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((ENTRY, END)),
            ..CompareConfig::default()
        },
    );

    assert!(
        report.within_tolerance(),
        "native port diverged from the natural stage-2 window:\n{}",
        report.detailed()
    );
    assert_eq!(report.divergences.len(), 0);
    assert_eq!(report.frames, 7430);
    assert_eq!(report.compared, 7430);
    assert_eq!(report.requested_window, Some((ENTRY, END)));
    // The window must exercise stage-2 play: ship, scroll, score and objects.
    assert!(report.fields["px"].checks > 0);
    assert!(report.fields["py"].checks > 0);
    assert!(report.fields["scroll"].checks > 0);
    assert!(report.fields["score"].checks > 0);
    assert!(report.fields["obj.kind"].checks > 0);
    assert!(report.fields["obj.x"].checks > 0);
    assert!(report.fields["obj.y"].checks > 0);
    assert!(report.fields["obj.substate"].checks > 0);

    // The natural landmarks the window was recorded for: the core spawns and
    // self-destructs inside it, and stage 3 starts on its last two frames.
    let has_boss = |snapshot: &Snapshot| {
        snapshot
            .objects
            .iter()
            .any(|object| object.slot == BOSS_SLOT && object.kind == BOSS_KIND)
    };
    let spawn = oracle
        .iter()
        .find(|snapshot| has_boss(snapshot))
        .expect("the stage-2 Big Core never spawned");
    assert_eq!(spawn.frame, 17282, "stage-2 boss spawn frame");
    assert_eq!(spawn.score, 2760, "spawn score");
    assert!(
        oracle
            .iter()
            .all(|snapshot| snapshot.frame <= 18914 || !has_boss(snapshot)),
        "the core should self-destruct at 18915"
    );
    let entry = oracle
        .iter()
        .find(|snapshot| snapshot.frame > 18915 && snapshot.scroll_high == 0)
        .expect("stage 3 never started");
    assert_eq!(entry.frame, 19429, "stage-3 frame");
    assert_eq!(entry.scroll_low, 1, "stage-3 first scroll byte");
}

#[test]
fn the_window_gate_rejects_the_old_eruption_drift() {
    // The 15968 `$C540` eruption class was 20 frames of `obj.y` +2 px. The
    // gate must still catch it if it ever comes back.
    let oracle = fixture();
    let mut perturbed = oracle.clone();
    let at = perturbed
        .iter_mut()
        .find(|snapshot| snapshot.frame == 15968)
        .expect("frame 15968 in the fixture");
    let slot = at
        .objects
        .iter_mut()
        .find(|object| object.slot == 4)
        .expect("slot 4 at 15968");
    slot.y += 2;

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&perturbed).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((ENTRY, END)),
            ..CompareConfig::default()
        },
    );

    assert!(
        !report.within_tolerance(),
        "the gate accepted a +2 px obj.y"
    );
    assert_eq!(report.divergences[0].frame, 15968);
    assert_eq!(report.divergences[0].field, "obj.y");
}
