//! Scenario matrix consistency. No cartridge is needed, only the extracted asset pack.
//! These checks run on CI and catch a scenario whose check plan no longer
//! matches the input script it drives.

use std::collections::HashSet;

use gradius_diff::scenario::{repo_root, Check, Expected, Scenario, SCENARIOS};
use gradius_diff::script::{parse_script, Script};
use gradius_diff::{Delta, Divergence, Report};

fn script(scenario: &Scenario) -> Script {
    let path = repo_root().join(scenario.script);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    parse_script(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn matrix_names_are_unique_and_every_script_parses() {
    let mut names = HashSet::new();
    for scenario in SCENARIOS {
        assert!(
            names.insert(scenario.name),
            "duplicate scenario name {:?}",
            scenario.name
        );
        assert!(scenario.frames > 0, "{}: no frames", scenario.name);
        let _ = script(scenario);
    }
}

#[test]
fn every_window_and_png_frame_fits_the_run() {
    for scenario in SCENARIOS {
        if let Some((from, to)) = scenario.window {
            assert!(
                from <= to && to <= scenario.frames,
                "{}: state window {from}-{to} outside 0-{}",
                scenario.name,
                scenario.frames
            );
        }
        if let Some((from, to)) = scenario.pixel_window {
            assert!(
                from <= to && to <= scenario.frames,
                "{}: pixel window {from}-{to} outside 0-{}",
                scenario.name,
                scenario.frames
            );
            assert!(scenario.pixel_every > 0, "{}: pixel_every 0", scenario.name);
        }
        for frame in scenario.png_frames {
            assert!(
                *frame < scenario.frames,
                "{}: PNG frame {frame} outside 0-{}",
                scenario.name,
                scenario.frames
            );
        }
    }
}

/// The menu checks hard-code the documented control edges. A scenario that
/// asks for a Select edge needs a Select in its script; the start-only case
/// must have no Select at all.
#[test]
fn menu_checks_match_their_scripted_edges() {
    for scenario in SCENARIOS {
        let script = script(scenario);
        match scenario.check {
            Check::Menu { selection, .. } => {
                assert!(
                    script.buttons(180).select,
                    "{}: Menu check needs Select at 180",
                    scenario.name
                );
                assert!(
                    script.buttons(320).start,
                    "{}: Menu check needs Start at 320",
                    scenario.name
                );
                assert!(selection <= 1, "{}: $0F is 0 or 1", scenario.name);
            }
            Check::MenuStart {
                start_frame,
                start_lock,
            } => {
                assert!(
                    script.buttons(start_frame).start,
                    "{}: MenuStart check needs Start at {start_frame}",
                    scenario.name
                );
                assert!(
                    !script.presses.iter().any(|press| press.buttons.select),
                    "{}: start-only scenario must not press Select",
                    scenario.name
                );
                assert_eq!(
                    start_lock & 0x70,
                    start_lock,
                    "{}: lock mask",
                    scenario.name
                );
            }
            _ => {}
        }
    }
}

/// A power plan's taps and hold must be present in its script; the runner
/// unions them in defensively, but the script is the record of the input.
#[test]
fn power_checks_match_their_scripted_buttons() {
    for scenario in SCENARIOS {
        let script = script(scenario);
        let Check::Power(plan) = scenario.check else {
            continue;
        };
        assert!(
            (1..=6).contains(&plan.slot),
            "{}: slot {}",
            scenario.name,
            plan.slot
        );
        if plan.slot == 1 && plan.hold_from.is_none() {
            assert_eq!(
                plan.taps.len(),
                plan.buys,
                "{}: one tap per SPEED buy",
                scenario.name
            );
        }
        for tap in plan.taps {
            assert!(
                script.buttons(*tap).b,
                "{}: no B press at {tap}",
                scenario.name
            );
        }
        if let Some(hold) = plan.hold_from {
            assert!(
                script.buttons(hold).b,
                "{}: no held B from {hold}",
                scenario.name
            );
        }
    }
}

/// The menu rows are hard gates: the port now draws the Select cursor on both
/// rows, so `$0320` matches and a menu regression must fail the run.
#[test]
fn menu_rows_are_clean_gates() {
    for name in ["menu-select-1p", "menu-select-2p"] {
        let scenario = SCENARIOS
            .iter()
            .find(|scenario| scenario.name == name)
            .unwrap_or_else(|| panic!("missing scenario {name}"));
        assert_eq!(
            scenario.expected,
            Expected::Clean,
            "{name}: the menu rows are clean once the cursor matches"
        );
    }
}

/// The 2P handoff scenario must drive the second pad and, now that the runtime
/// follows `$18`, be a hard clean gate.
#[test]
fn handoff_scenario_drives_the_second_pad() {
    let scenario = SCENARIOS
        .iter()
        .find(|scenario| matches!(scenario.check, Check::Handoff))
        .expect("missing the p2-handoff scenario");
    assert_eq!(
        scenario.expected,
        Expected::Clean,
        "p2-handoff is clean once the port follows the 2P relay"
    );
    let script = script(scenario);
    assert!(script.buttons(180).select, "Select at 180 picks 2P");
    assert!(script.buttons(220).start, "Start at 220 locks the mode");
    assert!(
        script.p2_buttons(900).start,
        "P2 Start during play must be scripted"
    );
    assert!(
        script.p2_buttons(1230).right,
        "P2 Right after the handoff must be scripted"
    );
    assert!(
        script.buttons(1230).is_empty(),
        "the port's P1 pad must be idle while P2 steers"
    );
}

fn report_at(divergences: Vec<(u64, &str)>) -> Report {
    Report {
        frames: 1300,
        requested_window: None,
        compared: 942,
        window_first: Some(0),
        window_last: Some(1299),
        state_mismatch_frames: 0,
        tolerance_px: 1,
        divergences: divergences
            .into_iter()
            .map(|(frame, field)| Divergence {
                frame,
                field: field.to_string(),
                oracle: "82.00".into(),
                native: "80.00".into(),
            })
            .collect(),
        fields: Default::default(),
        object_count_delta: 0,
        max_object_count_delta: 0,
        object_count_mismatch_frames: 0,
        object_slot_matches: 0,
        object_slot_pairs: 0,
        px_delta: Delta::default(),
        py_delta: Delta::default(),
    }
}

/// A known budget accepts a clean run and any later divergence, and rejects one
/// that arrives earlier or in more fields than measured.
#[test]
fn known_budget_rejects_an_earlier_or_wider_divergence() {
    let budget = Expected::Known {
        first_frame: 1233,
        max_divergences: 68,
    };
    let accepts = |divergences: Vec<(u64, &str)>| budget.accepts(&report_at(divergences));
    assert!(accepts(Vec::new()), "a clean run is accepted");
    assert!(
        accepts(vec![(1233, "px")]),
        "the documented drift is accepted"
    );
    assert!(
        accepts(vec![(1299, "px")]),
        "a later divergence is accepted"
    );
    assert!(!accepts(vec![(1232, "px")]), "an earlier divergence fails");
    assert!(
        !accepts(vec![(1233, "px"); 69]),
        "more divergences than measured fail"
    );
}

#[test]
fn list_prints_every_scenario_without_a_rom() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_gradius-scenarios"))
        .arg("--list")
        .output()
        .expect("failed to run gradius-scenarios --list");
    assert!(output.status.success(), "--list did not succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert_eq!(
        stdout.lines().count(),
        SCENARIOS.len(),
        "--list does not print one line per scenario:\n{stdout}"
    );
    for scenario in SCENARIOS {
        assert!(
            stdout.contains(scenario.name),
            "--list is missing {}:\n{stdout}",
            scenario.name
        );
    }
}
