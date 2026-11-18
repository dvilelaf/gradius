//! Replay the stage-7 gap (oracle frames 6051-6562) against the port.
//!
//! The stage-7 gate, `stage7_replay`, stops at 6050 and the clear fixture,
//! `stage7_clear_replay`, starts at 6563; the 512 frames between them were not
//! recorded. The two fixtures also use different inputs: the stage gate uses
//! the default script generated from its 6050-frame run, while the clear uses
//! the checked-in `stage7-6563-7024.script` with fire held from frame 0. The
//! gap is therefore recorded with the clear script, so it continues the clear
//! replay's trajectory from the 297 anchor; the default-script run cannot be
//! extended past its own 6050.
//!
//! The fixture was recorded with:
//!
//! ```sh
//! ROM="$GRADIUS_ROM"
//! ./target/release/gradius-diff --rom "$ROM" \
//!   --emit-fixture tests/fixtures/stage7-6051-6562.state \
//!   --script tests/fixtures/stage7-6563-7024.script \
//!   --stage 7 --frames 6562 --from 6051 --to 6562 --keep-alive
//! ```
//!
//! What the window shows: the clear's `+10` at 6563 (oracle 830, native 840) is
//! already there on the gap's first frame, so it is carried in, not made here.
//! Neither side scores once in 6051-6562: the oracle's score is 830 on all 512
//! frames and both sides last scored at 5944. The simulation matches exactly
//! (`px`, `py`, `scroll`, `speed`, `lives`, `meter` and every matched object's
//! `kind`/`x`/`y`/`substate`); only the carried `score` and the report-only
//! `obj.slot` residue diverge.
//!
//! Replaying the same all-fire script from the 297 anchor puts the first score
//! divergence at frame 390 (oracle 0, native 10): the port destroys the
//! kind-`$85` scout in slot 9 at `(116,86)` that the oracle keeps, and the
//! oracle instead kills a different scout at 406. Kill timing then oscillates
//! in `score` for the rest of the stage; the net is one extra native 10-point
//! kill by 5593, which is the `+10` the clear window carries.
//!
//! Like the clear replay this test is **report-only** apart from the structure
//! it pins; the module reports the first divergence and the fields that do
//! pass.

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::script::{parse_script, Script};
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The first recorded frame: the first frame after the `$3F = $0B` freeze.
const FIRST_FRAME: u64 = 6051;
/// The last recorded frame: the clear opens one frame later, at 6563.
const LAST_FRAME: u64 = 6562;
/// The stage-7 zero-page section (`$19 = 6`).
const STAGE_INDEX: u8 = 6;
/// Native frame 1 is the oracle's stage-entry anchor frame.
const NATIVE_FIRST_FRAME: u64 = 297;
/// The oracle still scores nothing in the gap; the gap starts from the last
/// score event at 5944.
const GAP_SCORE: u64 = 830;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_fixture(name: &str) -> String {
    let path = root().join("tests/fixtures").join(name);
    gradius_diff::fixture::read_fixture(&path).unwrap_or_else(|error| panic!("{error}"))
}

/// The checked-in input is the `record-ending` run, shared with the clear.
fn clear_script() -> Script {
    parse_script(&read_fixture("stage7-6563-7024.script")).expect("malformed clear script")
}

/// Replay the stage-7 gap from the 297 anchor and compare 6051-6562.
fn replay() -> Report {
    let oracle = parse_fixture(&read_fixture("stage7-6051-6562.state"))
        .expect("malformed stage-7 gap fixture");
    let script = clear_script();

    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    game.dev_keep_alive(true);

    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for frame in NATIVE_FIRST_FRAME..=LAST_FRAME {
        // The harness loop is 0-based and steps the oracle before sampling it,
        // so oracle frame N was run with the buttons of harness frame N - 1.
        game.step(core_buttons(script.buttons(frame - 1)));
        if frame < FIRST_FRAME {
            continue;
        }
        let mut snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        snapshot.frame = frame;
        native.push(snapshot);
    }
    assert_eq!(native.len(), oracle.len(), "gap window length");

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, LAST_FRAME)),
            ..CompareConfig::default()
        },
    )
}

/// The fixture is the span between the two gated recordings.
#[test]
fn the_gap_fixture_joins_the_stage_gate_and_the_clear() {
    let oracle = parse_fixture(&read_fixture("stage7-6051-6562.state"))
        .expect("malformed stage-7 gap fixture");
    assert_eq!(oracle.len() as u64, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(oracle.first().unwrap().frame, FIRST_FRAME);
    assert_eq!(oracle.last().unwrap().frame, LAST_FRAME);

    // `$3F = $0B` is released at 6051: the scroll resumes and runs to the
    // clear's frozen `0:12:128` value at 6563.
    let at = |frame: u64| {
        oracle
            .iter()
            .find(|snapshot| snapshot.frame == frame)
            .unwrap_or_else(|| panic!("fixture has no frame {frame}"))
    };
    assert_eq!(at(FIRST_FRAME).scroll_high, 0x0B);
    assert_eq!(
        (at(FIRST_FRAME).scroll_low, at(FIRST_FRAME).scroll_frac),
        (0, 128)
    );
    assert_eq!(
        (at(LAST_FRAME).scroll_high, at(LAST_FRAME).scroll_low),
        (0x0C, 0)
    );
    assert_eq!(at(LAST_FRAME).scroll_frac, 0);

    // No score event in the gap: the clear's 830 is the gap's 830.
    assert!(oracle.iter().all(|snapshot| snapshot.score == GAP_SCORE));
    let clear = parse_fixture(&read_fixture("stage7-6563-7024.state"))
        .expect("malformed stage-7 clear fixture");
    assert_eq!(clear.first().unwrap().frame, LAST_FRAME + 1);
    assert_eq!(clear.first().unwrap().score, GAP_SCORE);
}

/// Report-only: replay the gap and publish the first divergence.
///
/// The gap does not create the clear's `+10`; it carries it. The first
/// divergence is therefore the carried score on the first frame, and every
/// simulated field matches. A core fix to the all-fire kill timing moves the
/// score first divergence back to frame 390 and this pin will need updating.
#[test]
fn stage7_gap_replay_reports_the_carried_ten_points() {
    let report = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));
    for field in ["px", "py", "scroll", "score", "obj.kind", "obj.x", "obj.y"] {
        assert!(
            report.fields[field].checks > 0,
            "the fixture does not exercise {field}"
        );
    }

    eprintln!("stage-7 gap replay (report-only):");
    eprintln!("{}", report.summary());
    match report.first() {
        Some(first) => eprintln!(
            "first divergence: frame {} {} (oracle {}, native {})",
            first.frame, first.field, first.oracle, first.native
        ),
        None => eprintln!("first divergence: none"),
    }
    for field in ["score", "obj.slot", "obj.y"] {
        if let Some(first) = report.divergences.iter().find(|d| d.field == field) {
            eprintln!(
                "first {field} divergence: frame {} (oracle {}, native {})",
                first.frame, first.oracle, first.native
            );
        }
    }

    // The simulation matches over the whole gap: only the carried `+10` score
    // diverges. The old report-only object-slot residue from 6243 is gone:
    // `$AF0A` hides the background pieces for `$02 & $1F >= $1A` like the
    // oracle, so the dumps list the same slots.
    assert_eq!(report.failures("score"), report.compared);
    for field in [
        "px",
        "py",
        "scroll",
        "speed",
        "lives",
        "meter",
        "obj.kind",
        "obj.x",
        "obj.y",
        "obj.substate",
    ] {
        assert_eq!(report.failures(field), 0, "the gap diverges on {field}");
    }
    assert_eq!(
        report.failures("obj.slot"),
        0,
        "the object-slot residue is gone"
    );

    let first = report.first().expect("the carried score diverges");
    assert_eq!(first.frame, FIRST_FRAME);
    assert_eq!(first.field, "score");
    assert_eq!(
        (first.oracle.as_str(), first.native.as_str()),
        ("830", "840")
    );
}
