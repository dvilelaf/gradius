//! Replay the checked-in stage-6 fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack. The fixture covers the
//! poked stage-6 window, oracle frames 297-1500: `--stage 6` pokes `$26`, so
//! the oracle boots into stage 6 (`$19 = 5`) at 265 and enters normal play at
//! 292; the native `Game::new_at_stage` starts five frames later at 297
//! (`STAGE_BOOT_LEAD`), the first frame the harness pairs. The recording used
//! the harness's default input script and the oracle keep-alive, the same
//! settings as the `--stage 6` diagnostic run in
//! `docs/analysis/stage67_boot.md`.
//!
//! Regenerate the fixture from a cartridge with:
//!
//! ```sh
//! ./target/release/gradius-diff --rom "$ROM" \
//!   --emit-fixture tests/fixtures/stage6-297-1500.state \
//!   --stage 6 --frames 1500 --from 297 --to 1500 --keep-alive
//! ```
//!
//! `--keep-alive` pokes the oracle and so is part of the fixture; the replay
//! also enables the port's `Game::dev_keep_alive(true)`, or a native death
//! would freeze scroll and lives while the kept-alive oracle plays on.
//!
//! The `--frames` value matters: the default script is generated from the run
//! length, so the recording must use the length this test builds with
//! [`RECORDED_FRAMES`]. The oracle's frame counter is 1-based, so `--frames
//! 1500` records frames 1-1500 and the window ends on the run's last frame.
//!
//! The window is exact: it covers the `$9B38` boot, the stage-6 spawner and the
//! `$AF48` column formations before the no-core clear threshold (`$3F = $0B`,
//! oracle frame 5928, beyond the fixture). After this window the clear is
//! covered separately by [`stage6_clears_to_stage_seven`], which continues the
//! same boot past the fixture without a cartridge.

use std::path::PathBuf;

use gradius_core::{Buttons, Game};
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The oracle frame the harness pairs with native frame 1.
const FIRST_FRAME: u64 = 297;
/// The last compared frame.
const LAST_FRAME: u64 = 1500;
/// The stage-6 zero-page section (`$19 = 5`).
const STAGE_INDEX: u8 = 5;
/// Native frame 1 is the oracle's frame 297.
const FRAME_OFFSET: u64 = FIRST_FRAME - 1;
/// The recording was made with `--frames 1500`; the default script is a
/// function of the run length, so the test must build the same one. The
/// oracle's frame counter is 1-based, so this is also [`LAST_FRAME`].
const RECORDED_FRAMES: u64 = LAST_FRAME;
/// Frames from `new_at_stage(5)` to the first stage-7 frame with no input:
/// the `$3F >= $0B` clear threshold, the 1024-frame `$82` beam window, the
/// `$86` wall erase and the `$90` advance (which `$96C4` runs at the start of
/// the frame after the threshold). Measured, not derived from the oracle,
/// which starts five frames later.
const CLEAR_FRAMES: u64 = 7172;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Replay the recorded window and compare it over the full 1204 frames.
fn replay() -> Report {
    let fixture_path = root().join("tests/fixtures/stage6-297-1500.state");
    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script = default_script(RECORDED_FRAMES);

    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    // The diagnostic run uses `--native-keep-alive`: without it a native death
    // freezes scroll and lives while the kept-alive oracle plays on.
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for snapshot in &oracle {
        // The harness loop is 0-based and steps the oracle before sampling it,
        // so oracle frame N was run with the buttons of harness frame N - 1.
        game.step(core_buttons(script.buttons(snapshot.frame - 1)));
        let mut native_snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        native_snapshot.frame += FRAME_OFFSET;
        native.push(native_snapshot);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, LAST_FRAME)),
            ..CompareConfig::default()
        },
    )
}

/// The gate: the whole 297-1500 window is exact.
///
/// It covers the `$9B38` boot (slot 9 starts six pixels in), the stage-6
/// spawner and the `$AF48` column formations. The no-core clear threshold
/// (`$3F = $0B`) is at oracle frame 5928, beyond this fixture, and is covered
/// by [`stage6_clears_to_stage_seven`].
#[test]
fn stage6_replay_is_exact() {
    let report = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));

    assert!(
        report.within_tolerance(),
        "the stage-6 window diverges at {:?}",
        report.first()
    );

    for field in [
        "frame",
        "px",
        "py",
        "speed",
        "scroll",
        "score",
        "lives",
        "meter",
        "obj.kind",
        "obj.substate",
        "obj.x",
        "obj.y",
    ] {
        let stat = report.fields.get(field).unwrap_or_else(|| {
            panic!("{field} was not compared");
        });
        assert!(stat.checks > 0, "the fixture does not exercise {field}");
        assert_eq!(stat.passes, stat.checks, "{field} diverges");
    }

    // Every field that was checked must pass, including the object table; the
    // coverage counts pin the window so a shorter replay cannot pass silently.
    for (field, stat) in &report.fields {
        assert_eq!(stat.passes, stat.checks, "{field} diverges");
    }
    assert_eq!(report.object_slot_matches, 6759);
    assert_eq!(report.object_slot_pairs, 6759);
    assert_eq!(report.object_count_delta, 0);
    assert_eq!(report.max_object_count_delta, 0);
}

/// The stage-6 no-core clear runs from the same boot with no input: the
/// `$3F >= $0B` threshold arms `$81`, the 1024-frame `$82` beam window ends at
/// `$83`, `$86` erases the wall and requests `$AC`, and `$90`/`$96C4` advances
/// `$19` to stage 7. Pins the measured frame count.
#[test]
fn stage6_clears_to_stage_seven() {
    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    game.dev_keep_alive(true);
    let mut frames = 0;
    while game.section() == STAGE_INDEX {
        game.step(Buttons::NONE);
        frames += 1;
        assert!(
            frames <= CLEAR_FRAMES,
            "stage 6 did not reach stage 7 within {CLEAR_FRAMES} frames"
        );
    }
    assert_eq!(game.section(), STAGE_INDEX + 1, "advanced past stage 7");
    assert_eq!(frames, CLEAR_FRAMES);
}

/// Diagnostics: assert the comparison covers the whole window and exercises
/// the simulation, then publish the summary and the first divergence.
#[test]
fn stage6_replay_reports_the_first_divergence() {
    let report = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));
    // The window must exercise the simulation, not just a static frame.
    for field in ["px", "py", "scroll", "score", "obj.kind", "obj.x", "obj.y"] {
        assert!(
            report.fields[field].checks > 0,
            "the fixture does not exercise {field}"
        );
    }
    assert!(report.object_slot_pairs > 0, "no object slots compared");

    eprintln!("stage-6 fixture replay:");
    eprintln!("{}", report.summary());
    match report.first() {
        Some(first) => eprintln!(
            "first divergence: frame {} {} (oracle {}, native {})",
            first.frame, first.field, first.oracle, first.native
        ),
        None => eprintln!("first divergence: none"),
    }
}
