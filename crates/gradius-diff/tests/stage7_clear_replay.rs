//! Replay the stage-7 clear window (oracle frames 6563-7024) against the port.
//!
//! The stage-7 state gate, `stage7_replay`, ends at 6050, before the clear
//! opens. This fixture covers the window the clear state machine runs in:
//! `$3F = $0C` (`$9A30[6]`) at 6563 with `$1B = $81`, the `$82` special timer
//! `$4D = 1, $4C = $CA` (458 frames), `$83` at 7022, `$86` at 7023 and the
//! `$3F = $0D` / `$1B = $87` hand-off to the ending at 7024. The ending itself
//! from 7025 is gated separately by `ending_replay`.
//!
//! The recording needs the `record-ending` input, not the harness default
//! script. With the default script's steering blocks the oracle never arms the
//! `$81`/`$82`/`$83`/`$86` window: `$3F` reads `$0C` from 6563 but stays there
//! past 7024 and the scroll keeps advancing. The clear runs only with fire
//! held on every frame and Start at 180 for six frames, which reproduces the
//! measured boundaries exactly (`scroll = 0:12:128` through 7023 and
//! `0:0:128` at 7024). The input is checked in as
//! `stage7-6563-7024.script` and the fixture was recorded with:
//!
//! ```sh
//! ROM="$GRADIUS_ROM"
//! ./target/release/gradius-diff --rom "$ROM" \
//!   --emit-fixture tests/fixtures/stage7-6563-7024.state \
//!   --script tests/fixtures/stage7-6563-7024.script \
//!   --stage 7 --frames 7024 --from 6563 --to 7024 --keep-alive
//! ```
//!
//! The phase trace (`$1B`, `$19`, `$28`, `$26`) comes from the same input via
//! `xtask record-ending`; frames 6563-7024 are kept:
//!
//! ```sh
//! cargo run -p xtask -- record-ending "$ROM" --stage 7 --frames 7024 \
//!   --out /tmp/opencode/s7clear.trace
//! ```
//!
//! The native replay starts where the stage-7 gate starts: `new_at_stage_seeded`
//! at the oracle's 297 anchor (`nmi_seed = $24 - 1`, `dev_keep_alive(true)`),
//! and steps the checked-in script with the harness rule
//! `buttons(frame - 1)` up to 7024. Only 6563-7024 is compared, so the earlier
//! stage-7 residue is carried, not re-reported here.
//!
//! This test is **report-only**: it publishes the summary and the first
//! divergence and does not gate the window. It hardens into a gate the same
//! way `stage7_replay` did: once the kind-`$93` ceiling-walker `obj.y` residue
//! is gone and the port enters `$3F = $0C` on the oracle's frame (the port
//! currently reaches it one frame early at 6562), replace the report-only
//! assertion with `assert!(report.within_tolerance())` and drop the report-only
//! row from `docs/KNOWN_DEVIATIONS.md`.

use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::script::{parse_script, Script};
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The first compared frame: the clear opens on `$3F = $0C`, `$1B = $81`.
const FIRST_FRAME: u64 = 6563;
/// The last compared frame: `$3F = $0D`, `$1B = $87` hands off to the ending.
const LAST_FRAME: u64 = 7024;
/// The stage-7 zero-page section (`$19 = 6`).
const STAGE_INDEX: u8 = 6;
/// Native frame 1 is the oracle's stage-entry anchor frame.
const NATIVE_FIRST_FRAME: u64 = 297;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_fixture(name: &str) -> String {
    let path = root().join("tests/fixtures").join(name);
    gradius_diff::fixture::read_fixture(&path).unwrap_or_else(|error| panic!("{error}"))
}

/// The checked-in input is the `record-ending` run.
fn clear_script() -> Script {
    parse_script(&read_fixture("stage7-6563-7024.script")).expect("malformed clear script")
}

/// Replay the stage-7 clear from the 297 anchor and compare 6563-7024.
fn replay() -> Report {
    let oracle =
        parse_fixture(&read_fixture("stage7-6563-7024.state")).expect("malformed clear fixture");
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
    assert_eq!(native.len(), oracle.len(), "clear window length");

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, LAST_FRAME)),
            ..CompareConfig::default()
        },
    )
}

/// One `key=value` field of a trace line.
fn trace_field(trace: &str, frame: u64, key: &str) -> String {
    let prefix = format!("frame={frame} ");
    let line = trace
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("trace has no frame {frame}"));
    let needle = format!("{key}=");
    line.split_whitespace()
        .find_map(|field| field.strip_prefix(&needle))
        .unwrap_or_else(|| panic!("frame {frame} has no {key}"))
        .to_string()
}

/// The oracle trace pins the clear boundaries the state fixture cannot show.
#[test]
fn the_trace_is_the_measured_clear_window() {
    let trace = read_fixture("stage7-6563-7024.trace");
    assert_eq!(trace.lines().count() as u64, LAST_FRAME - FIRST_FRAME + 1);

    // `$9A30[6]`: the clear opens, the special timer starts the next frame.
    assert_eq!(trace_field(&trace, 6563, "phase"), "81");
    assert_eq!(trace_field(&trace, 6563, "stage"), "6");
    assert_eq!(trace_field(&trace, 6563, "scroll"), "0:12:128");
    assert_eq!(trace_field(&trace, 6563, "loop"), "0");
    assert_eq!(trace_field(&trace, 6563, "stagebyte"), "6");
    assert_eq!(trace_field(&trace, 6564, "phase"), "82");

    // The `$82` timer runs 458 frames: `$83` at 7022, `$86` at 7023.
    assert_eq!(trace_field(&trace, 7022, "phase"), "83");
    assert_eq!(trace_field(&trace, 7023, "phase"), "86");
    // The scroll is frozen at `$3F = $0C` through the last `$86` frame.
    assert_eq!(trace_field(&trace, 7023, "scroll"), "0:12:128");

    // `$98F7` -> `$9865`: stage byte cleared, loop incremented, scroll reset.
    assert_eq!(trace_field(&trace, 7024, "phase"), "87");
    assert_eq!(trace_field(&trace, 7024, "stage"), "6");
    assert_eq!(trace_field(&trace, 7024, "scroll"), "0:0:128");
    assert_eq!(trace_field(&trace, 7024, "loop"), "1");
    assert_eq!(trace_field(&trace, 7024, "stagebyte"), "0");
    assert_eq!(trace_field(&trace, 7024, "score"), "830");
    assert_eq!(trace_field(&trace, 7024, "lives"), "3");
}

/// The state fixture covers the same 462 frames, from the `$81` frame to the
/// `$87` hand-off.
#[test]
fn the_state_fixture_covers_the_same_clear_window() {
    let oracle =
        parse_fixture(&read_fixture("stage7-6563-7024.state")).expect("malformed clear fixture");
    assert_eq!(oracle.len() as u64, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(oracle.first().unwrap().frame, FIRST_FRAME);
    assert_eq!(oracle.last().unwrap().frame, LAST_FRAME);

    let at = |frame: u64| {
        oracle
            .iter()
            .find(|snapshot| snapshot.frame == frame)
            .unwrap_or_else(|| panic!("fixture has no frame {frame}"))
    };
    // `$3F = $0C` holds the scroll through 7023; `$3F = $0D` resets it at 7024.
    assert_eq!(at(6563).scroll_high, 0x0C);
    assert_eq!(at(7023).scroll_high, 0x0C);
    assert_eq!(at(7023).scroll_low, 0);
    assert_eq!(at(7023).scroll_frac, 128);
    assert_eq!(at(7024).scroll_high, 0);
    assert_eq!(at(7024).scroll_low, 0);
    assert_eq!(at(7024).scroll_frac, 128);
    assert_eq!(at(7024).score, 830);
}

/// Report-only: replay the window and publish the first divergence.
///
/// This does not fail on a divergence. It pins the shape of the comparison so
/// the report stays honest; see the module docs for the hardening path.
#[test]
fn stage7_clear_replay_reports_the_first_divergence() {
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

    eprintln!("stage-7 clear window replay (report-only):");
    eprintln!("{}", report.summary());
    match report.first() {
        Some(first) => eprintln!(
            "first divergence: frame {} {} (oracle {}, native {})",
            first.frame, first.field, first.oracle, first.native
        ),
        None => eprintln!("first divergence: none"),
    }
    // Per-field first divergences split the carried stage-7 residue from the
    // clear timing (`scroll` pins `$3F`).
    for field in ["scroll", "px", "py", "score", "obj.slot", "obj.y"] {
        if let Some(first) = report.divergences.iter().find(|d| d.field == field) {
            eprintln!(
                "first {field} divergence: frame {} (oracle {}, native {})",
                first.frame, first.oracle, first.native
            );
        }
    }
}
