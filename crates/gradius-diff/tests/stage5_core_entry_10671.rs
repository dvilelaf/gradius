//! Probe for the stage-5 first divergence at 10671 `obj.x`: the Big Core's
//! entry branch (`$B9A5`) must fall through when the damage gate is open.
//!
//! The clear-window replay is exact through 10670. On 10671 the oracle's core
//! (slot 9, kind `$98`, `$048C = 1`) is at x 166, y 76 while the port holds
//! x 164, y 74; the two `$99` covers in slots 7/8 inherit the same shift
//! through `$BA65 JSR $B9B1`, and the report lists slot 7 first because the
//! slots are compared in ascending order.
//!
//! ROM (`$B9A2`-`$B9AF`):
//!
//! ```text
//! $B9A2 LDA $036C,X
//! $B9A5 CMP #$A4
//! $B9A7 BCC $BA04      ; x < $A4: straight to the fight path
//! $B9A9 DEC $036C,X    ; entry: one pixel left
//! $B9AC LDA $048C,X
//! $B9AF BNE $BA04      ; damage gate open: still runs the whole fight path
//! $B9B1 ...            ; gate closed: rebuild the covers, RTS
//! ```
//!
//! The port returns unconditionally after the `DEC`, so a core at x > `$A3`
//! never tracks, re-aims, moves vertically or advances its timers on that
//! frame. At 10670 the player reaches the core's x and the fight path's
//! `$BA0C`/`$BA0F` +2 leaves the core at 165; from 10671 the ROM spends
//! -1 (entry) then +2 (track, player now at 164 -> 166) = +1 per frame, so the
//! oracle core x is `px + 2` and its y keeps the rank velocity. The port spends
//! 10671 and 10672 in the entry branch (x 165 -> 164 -> 163) and only reaches
//! the fight path on every third frame, which is why its y advances 1 px per
//! three frames.
//!
//! Both measurements are pinned below. When the core card lets the entry
//! branch fall through on `$048C != 0`, the 10671 pins must become an exact
//! window (see [stage5_boot.md §11](../../../docs/analysis/stage5_boot.md)).
use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The first recorded frame of the clear window.
const FIRST_FRAME: u64 = 8574;
/// The last recorded frame; the fixture does not carry anything past it.
const LAST_FRAME: u64 = 12003;
/// The replay stops here: enough to cover the divergence and its mechanism.
const PROBE_LAST: u64 = 10690;
/// The oracle frame the harness pairs with native frame 1.
const FIRST_PAIRED: u64 = 297;
/// The stage-5 zero-page section (`$19 = 4`).
const STAGE_INDEX: u8 = 4;
/// The full-run recording the clear window is cut from.
const FIXTURE: &str = "tests/fixtures/stage5-8574-12003.state";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load() -> Vec<Snapshot> {
    let path = root().join(FIXTURE);
    let text = read_fixture(&path).unwrap_or_else(|error| panic!("{error}"));
    parse_fixture(&text).expect("malformed fixture")
}

/// Replay the native port from the warm-up frame through [`PROBE_LAST`].
fn replay() -> (Vec<Snapshot>, Vec<Snapshot>) {
    let oracle = load();
    // The fixture was recorded at `--frames 12003`; the script length is part
    // of the recording, so it must not shrink with the probe window.
    let script = default_script(LAST_FRAME);
    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    game.dev_keep_alive(true);
    let mut native = Vec::with_capacity(oracle.len());
    for frame in FIRST_PAIRED..=PROBE_LAST {
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

/// `(x, y, substate, kind)` of a slot, or `None` when it is not live.
fn object(snapshot: &Snapshot, slot: i64) -> Option<(i64, i64, i64, i64)> {
    snapshot
        .objects
        .iter()
        .find(|object| object.slot == slot)
        .map(|object| (object.x, object.y, object.substate, object.kind))
}

/// Regression: the Big Core takes the fight path once the entry gate opens.
#[test]
fn the_core_takes_the_fight_path_at_10671() {
    let (oracle, native) = replay();
    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle
        .iter()
        .filter(|snapshot| snapshot.frame <= PROBE_LAST)
        .zip(&native)
        .collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, PROBE_LAST)),
            ..CompareConfig::default()
        },
    );
    assert!(
        report.first().is_none(),
        "the window must replay exactly: {:?}",
        report.first()
    );

    // 10670 is the last frame of the entry: both sides stop at x 163 and track
    // the player (+2 at `$BA0C`) to 165.
    for snapshots in [&oracle, &native] {
        assert_eq!(object(at(snapshots, 10670), 9), Some((165, 74, 1, 0x98)));
    }

    // 10671 starts at x 165 > `$A3` with `$048C = 1`: the entry -1 is followed
    // by the fight path (+2, player now at 164) and the rank velocity moves y
    // 74 -> 76. Both sides must land on the same values.
    for snapshots in [&oracle, &native] {
        assert_eq!(object(at(snapshots, 10671), 9), Some((166, 76, 1, 0x98)));
    }

    // The core tracks the ship: core x == px + 2 while the player crosses it.
    for frame in 10671..=10681 {
        for (side, snapshots) in [("oracle", &oracle), ("native", &native)] {
            let snapshot = at(snapshots, frame);
            let core = object(snapshot, 9).expect("core").0;
            assert_eq!(
                core,
                snapshot.px + 2,
                "{side} core x follows the ship at {frame}"
            );
        }
    }

    // The covers are synced from the core (`$BA65 JSR $B9B1`), so slots 7 and
    // 8 carry the same x error; the upper cover sits 20 px above.
    for frame in [10671, 10672] {
        for (side, snapshots) in [("oracle", &oracle), ("native", &native)] {
            let snapshot = at(snapshots, frame);
            let core = object(snapshot, 9).expect("core");
            let lower = object(snapshot, 7).expect("lower cover");
            let upper = object(snapshot, 8).expect("upper cover");
            assert_eq!((lower.0, lower.1), (core.0, core.1), "{side} {frame}");
            assert_eq!((upper.0, upper.1), (core.0, core.1 - 20), "{side} {frame}");
        }
    }
}
