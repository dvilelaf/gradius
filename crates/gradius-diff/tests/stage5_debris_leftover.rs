//! Regression probe for the stage-5 phantom debris at 8642.
//!
//! `collide_shot_cannons` used to write the `$BF43` wreck into
//! `self.objects[shot_index]` — the shot pool index over the object table —
//! creating a phantom slot 3 for 8642-8666 and making that the clear window's
//! first divergence. The fix removes the write; this file keeps the
//! measurements behind
//! [stage5_boot.md §10](../../../docs/analysis/stage5_boot.md#10-the-8642-leftover-the-bf43-cannon-debris-in-the-object-table):
//!
//! * at 8641 both sides score the `$BF43` cannon weak point (+100, 290 ->
//!   390); the port's `collide_shot_cannons` then writes the kind-2 debris
//!   into `self.objects[shot_index]`, and the shot that hit is slot 3 of the
//!   shot pool (second shot of emitter 0), so the debris lands in object
//!   slot 3, which no stage-5 spawn ever uses;
//! * the slot was never initialised, so x/y are 0 and `sprite` is 0, which
//!   keeps it out of the dump on 8641; the first `$AE93` sprite step on 8642
//!   makes it visible;
//! * it lives 8642-8666 (25 dumped frames, 26 live) and despawns on the
//!   `$AE6B` path-2 terminator during 8667. It cannot despawn on x: 8574-9600
//!   is the `$82` pre-boss hold (`scroll = 0:11:128`, frozen), so `$AED7`
//!   never drifts it and x stays 0 (a drift frame would despawn it at once,
//!   `x < 8`).
//!
//! Both pins are report-only in the sense the repo uses: they assert today's
//! measured state. Once the core card removes the `self.objects[shot_index]`
//! write, `native_slot_three` is empty and the test must be replaced by the
//! exact gate in §10 (`object_slot_matches == object_slot_pairs` and
//! `obj.slot` 100% from 8642).
use std::collections::BTreeMap;
use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The first recorded frame of the clear window.
const FIRST_FRAME: u64 = 8574;
/// The last recorded frame: stage 6 starts here.
const LAST_FRAME: u64 = 12003;
/// The oracle frame the harness pairs with native frame 1.
const FIRST_PAIRED: u64 = 297;
/// The stage-5 zero-page section (`$19 = 4`).
const STAGE_INDEX: u8 = 4;
/// The full-run recording the clear window is cut from.
const FIXTURE: &str = "tests/fixtures/stage5-8574-12003.state";
/// The exact gate; its kind-2 debris is the `$BE8D` object kill.
const EXACT_GATE: &str = "tests/fixtures/stage5-297-8573.state";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(name: &str) -> Vec<Snapshot> {
    let path = root().join(name);
    let text = read_fixture(&path).unwrap_or_else(|error| panic!("{error}"));
    parse_fixture(&text).expect("malformed fixture")
}

/// Replay the native port from the warm-up frame and return (oracle, native).
fn replay() -> (Vec<Snapshot>, Vec<Snapshot>) {
    let oracle = load(FIXTURE);
    let script = default_script(LAST_FRAME);
    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    game.dev_keep_alive(true);
    let mut native = Vec::with_capacity(oracle.len());
    for frame in FIRST_PAIRED..=LAST_FRAME {
        game.step(core_buttons(script.buttons(frame - 1)));
        if frame >= FIRST_FRAME {
            let mut snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
            snapshot.frame = frame;
            native.push(snapshot);
        }
    }
    (oracle, native)
}

fn at(snapshots: &[Snapshot], frame: u64) -> &Snapshot {
    snapshots
        .iter()
        .find(|snapshot| snapshot.frame == frame)
        .unwrap_or_else(|| panic!("fixture has no frame {frame}"))
}

/// The 8642 leftover is the `$BF43` cannon debris written into object slot 3.
#[test]
fn the_8642_leftover_is_the_cannon_weak_point_debris() {
    let (oracle, native) = replay();

    // The oracle never has a slot 3 in the clear window: stage 5 uses 2, 4, 5
    // and 7-9, never 3.
    assert!(
        oracle
            .iter()
            .all(|snapshot| snapshot.objects.iter().all(|object| object.slot != 3)),
        "the oracle has a slot 3"
    );

    // 8641 is the `$BF43` kill: +100 on both sides (290 -> 390). The port used
    // to leak a debris record into `objects[shot_index]` here (shot index 3
    // over the object table): a phantom slot 3, kind 2, x/y 0, alive for
    // 8642-8666. The bug is fixed, so neither side may ever show slot 3.
    for snapshots in [&oracle, &native] {
        assert_eq!(at(snapshots, 8640).score, 290);
        assert_eq!(at(snapshots, 8641).score, 390);
    }
    for frame in 8640..=8670 {
        for (side, snapshots) in [("oracle", &oracle), ("native", &native)] {
            assert!(
                at(snapshots, frame)
                    .objects
                    .iter()
                    .all(|object| object.slot != 3),
                "{side} has a phantom slot 3 at {frame}"
            );
        }
    }
    eprintln!("8640-8670: no phantom slot 3 on either side");
}

/// The restos the window does match are `$BE8D` object kills: 20 frames of
/// path 1 in a real object slot, never 3.
#[test]
fn the_matching_stage5_debris_is_twenty_frames_of_bed8d() {
    let gate = load(EXACT_GATE);
    let mut frames: BTreeMap<i64, Vec<u64>> = BTreeMap::new();
    for snapshot in &gate {
        for object in &snapshot.objects {
            if object.kind == 2 {
                frames.entry(object.slot).or_default().push(snapshot.frame);
            }
        }
    }
    let slots: Vec<i64> = frames.keys().copied().collect();
    assert_eq!(slots, [2, 4, 5, 7, 8, 9], "the `$BE8D` debris slots");
    // Nine runs, each the 20 dumped frames of a path-1 wreck (`$014C = 3`).
    let mut runs = Vec::new();
    for run in frames.values() {
        let mut split = vec![run[0]];
        for window in run.windows(2) {
            if window[1] != window[0] + 1 {
                runs.push(split.len());
                split.clear();
            }
            split.push(window[1]);
        }
        runs.push(split.len());
    }
    assert_eq!(runs, [20, 20, 20, 20, 20, 20, 20, 20, 20]);
}
