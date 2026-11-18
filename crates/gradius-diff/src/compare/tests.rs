//! Comparator tests.

use super::*;

fn snapshot(frame: u64) -> Snapshot {
    Snapshot {
        frame,
        state: "Playing".to_string(),
        px: 80,
        pf: 0,
        py: 96,
        pyf: 0,
        speed: 0,
        scroll_low: 10,
        scroll_high: 0,
        scroll_frac: 0,
        score: 100,
        lives: 3,
        capsule: 0,
        meter: 0,
        objects: Vec::new(),
        nmi: None,
        mode: None,
    }
}

fn object(slot: i64, kind: i64, x: i64, y: i64, substate: i64) -> SnapshotObject {
    SnapshotObject {
        slot,
        kind,
        x,
        y,
        substate,
    }
}

fn run(oracle: &[Snapshot], native: &[Snapshot], tolerance: i64) -> Report {
    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(native).collect();
    compare(
        pairs,
        CompareConfig {
            tolerance_px: tolerance,
            ..CompareConfig::default()
        },
    )
}

fn run_window(
    oracle: &[Snapshot],
    native: &[Snapshot],
    tolerance: i64,
    window: (u64, u64),
) -> Report {
    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(native).collect();
    compare(
        pairs,
        CompareConfig {
            tolerance_px: tolerance,
            window: Some(window),
            ..CompareConfig::default()
        },
    )
}

#[test]
fn an_explicit_window_limits_comparison_and_ignores_the_state() {
    let mut oracle: Vec<Snapshot> = (1..=5).map(snapshot).collect();
    let native: Vec<Snapshot> = (1..=5).map(snapshot).collect();
    // Difference entirely outside the window: must not be reported.
    oracle[0].score = 999;
    // Inside the window, one side has left Playing; the fields still
    // compare, so the score difference is reported.
    oracle[2].state = "GameOver".to_string();
    oracle[2].score = 200;
    let report = run_window(&oracle, &native, 1, (2, 4));
    assert_eq!(report.frames, 5);
    assert_eq!(report.compared, 3);
    assert_eq!(report.requested_window, Some((2, 4)));
    assert_eq!(report.window_first, Some(2));
    assert_eq!(report.window_last, Some(4));
    assert_eq!(report.divergences.len(), 1);
    assert_eq!(report.first().unwrap().frame, 3);
    assert_eq!(report.first().unwrap().field, "score");
    assert!(report.summary().contains("(window 2-4)"));
    assert!(report
        .summary()
        .contains("skipped: 2 frames outside the requested window 2-4"));
}

#[test]
fn an_explicit_window_is_inclusive_on_both_ends() {
    let oracle: Vec<Snapshot> = (1..=5).map(snapshot).collect();
    let native: Vec<Snapshot> = (1..=5).map(snapshot).collect();
    let report = run_window(&oracle, &native, 1, (3, 3));
    assert_eq!(report.compared, 1);
    assert_eq!(report.window_first, Some(3));
    assert_eq!(report.window_last, Some(3));
    assert!(report.within_tolerance());

    let empty = run_window(&oracle, &native, 1, (10, 10));
    assert_eq!(empty.compared, 0);
    assert_eq!(empty.result(), "NO COMPARED FRAMES");
    assert!(!empty.within_tolerance());
}

#[test]
fn a_window_compares_frames_the_default_filter_would_skip() {
    let mut oracle: Vec<Snapshot> = (1..=3).map(snapshot).collect();
    let mut native: Vec<Snapshot> = (1..=3).map(snapshot).collect();
    oracle[1].state = "Title".to_string();
    native[1].state = "Other(4)".to_string();
    let default = run(&oracle, &native, 1);
    assert_eq!(default.compared, 2);
    let windowed = run_window(&oracle, &native, 1, (2, 2));
    assert_eq!(windowed.compared, 1);
    assert!(windowed.within_tolerance());
}

#[test]
fn identical_frames_are_within_tolerance() {
    let report = run(&[snapshot(1), snapshot(2)], &[snapshot(1), snapshot(2)], 1);
    assert!(report.within_tolerance());
    assert_eq!(report.frames, 2);
    assert_eq!(report.compared, 2);
    assert_eq!(report.first(), None);
    assert!(report.summary().contains("first divergence: none"));
}

#[test]
fn positions_within_the_pixel_tolerance_pass() {
    let oracle = snapshot(1);
    let mut native = snapshot(1);
    native.px = 81;
    native.pf = 255; // 81.996 - 80.000 = 1.996 px.
    assert!(!run(
        std::slice::from_ref(&oracle),
        std::slice::from_ref(&native),
        1
    )
    .within_tolerance());
    // A sub-pixel difference alone is inside one pixel.
    native.px = 80;
    native.pf = 255;
    assert!(run(
        std::slice::from_ref(&oracle),
        std::slice::from_ref(&native),
        1
    )
    .within_tolerance());
    // ... but tolerance 0 is bit-exact.
    assert!(!run(
        std::slice::from_ref(&oracle),
        std::slice::from_ref(&native),
        0
    )
    .within_tolerance());
    native.px = 81;
    native.pf = 0;
    assert!(run(&[oracle], &[native], 1).within_tolerance());
}

#[test]
fn tolerance_zero_is_exact() {
    let oracle = snapshot(1);
    let mut native = snapshot(1);
    native.pf = 1;
    let report = run(
        std::slice::from_ref(&oracle),
        std::slice::from_ref(&native),
        0,
    );
    assert!(!report.within_tolerance());
    assert_eq!(report.first().unwrap().field, "px");
    native.pf = 0;
    native.px = 80;
    assert!(run(&[oracle], &[native], 0).within_tolerance());
}

#[test]
fn exact_fields_are_reported_with_frame_and_field() {
    let mut native = snapshot(5);
    native.score = 101;
    native.lives = 2;
    native.speed = 1;
    native.scroll_low = 11;
    native.meter = 2;
    let report = run(&[snapshot(5)], &[native], 1);
    let fields: Vec<&str> = report
        .divergences
        .iter()
        .map(|divergence| divergence.field.as_str())
        .collect();
    assert_eq!(fields, ["speed", "scroll", "score", "lives", "meter"]);
    assert_eq!(report.first().unwrap().frame, 5);
    assert_eq!(report.first().unwrap().field, "speed");
    assert_eq!(report.failures("score"), 1);
    assert_eq!(report.failures("px"), 0);
}

#[test]
fn a_frame_number_mismatch_is_a_frame_field_divergence() {
    let report = run(&[snapshot(2)], &[snapshot(3)], 1);
    assert_eq!(report.first().unwrap().field, "frame");
    assert_eq!(report.failures("frame"), 1);
}

#[test]
fn capsule_is_not_compared_but_the_scroll_fraction_is() {
    let oracle = snapshot(1);
    let mut native = snapshot(1);
    native.capsule = 7;
    assert!(run(
        std::slice::from_ref(&oracle),
        std::slice::from_ref(&native),
        1
    )
    .within_tolerance());
    native.scroll_frac = 128;
    let report = run(&[oracle], &[native], 1);
    let divergence = report.first().unwrap();
    assert_eq!(divergence.field, "scroll");
    assert_eq!(divergence.oracle, "10:0:0");
    assert_eq!(divergence.native, "10:0:128");
}

#[test]
fn only_frames_where_both_sides_play_are_compared() {
    let mut title_oracle = snapshot(1);
    title_oracle.state = "Title".to_string();
    let mut title_native = snapshot(1);
    title_native.state = "Title".to_string();
    let mut attract_native = snapshot(2);
    attract_native.state = "Other(3)".to_string();
    let mut init_oracle = snapshot(3);
    init_oracle.state = "Other(4)".to_string();
    let mut playing_native = snapshot(4);
    playing_native.score = 5;
    let oracle = [title_oracle, snapshot(2), init_oracle, snapshot(4)];
    let native = [title_native, attract_native, snapshot(3), playing_native];
    let report = run(&oracle, &native, 1);
    assert_eq!(report.frames, 4);
    assert_eq!(report.compared, 1);
    assert_eq!(report.window_first, Some(4));
    assert_eq!(report.window_last, Some(4));
    assert_eq!(report.state_mismatch_frames, 2);
    assert_eq!(report.first().unwrap().frame, 4);
    assert!(report.summary().contains("1 compared (Playing frames 4-4)"));
    assert!(report
        .summary()
        .contains("skipped: 3 frames outside the Playing window (2 with differing states)"));
}

#[test]
fn a_run_without_playing_frames_is_not_within_tolerance() {
    let mut title = snapshot(1);
    title.state = "Title".to_string();
    let report = run(&[title.clone()], &[title], 1);
    assert_eq!(report.compared, 0);
    assert!(!report.within_tolerance());
    assert_eq!(report.result(), "NO PLAYING FRAMES");
    assert_eq!(report.window_first, None);
    assert!(!report.detailed().contains("first divergences"));
    let json = report.to_json();
    assert!(json.contains(
        "\"window\": { \"first\": null, \"last\": null, \"skipped\": 1, \"state_mismatch\": 0 }"
    ));
    assert!(json.contains("\"first_divergence\": null"));
    assert!(json.contains("    \"px\": { \"checks\": 0, \"passes\": 0, \"rate\": null }"));
    assert!(json.contains("\"rate\": null"));
}

#[test]
fn summary_prints_a_pass_rate_table() {
    let report = run(&[snapshot(1)], &[snapshot(1)], 1);
    let summary = report.summary();
    assert_eq!(report.fields.len(), 13);
    assert!(summary.contains("field"));
    assert!(summary.contains("px"));
    assert!(summary.contains("1/1"));
    assert!(summary.contains("100.0%"));
    // A field that never had a comparable pair is visible as `0/0` and `-`.
    let object_x = summary
        .lines()
        .find(|line| line.starts_with("obj.x"))
        .expect("obj.x row");
    assert!(object_x.contains("0/0"), "{object_x}");
    assert!(object_x.trim_end().ends_with('-'), "{object_x}");
    assert!(summary.contains("result: WITHIN TOLERANCE (0 divergences)"));
    assert!(summary.contains("position error: px max 0.000 mean 0.000"));
    assert!(summary.contains("objects: slot match 0/0 (-)"));
}

#[test]
fn json_summary_carries_the_window_stats_and_deltas() {
    let oracle = [snapshot(1), snapshot(2)];
    let mut first = snapshot(1);
    first.score = 101;
    first.px = 81; // one whole pixel: inside tolerance, tracked as a delta
    let native = [first, snapshot(2)];
    let report = run(&oracle, &native, 1);
    let json = report.to_json();
    assert!(json.starts_with('{'));
    assert!(json.ends_with("}\n"));
    assert!(json.contains("\"frames\": 2"));
    assert!(json.contains("\"compared\": 2"));
    assert!(json.contains(
        "\"window\": { \"first\": 1, \"last\": 2, \"skipped\": 0, \"state_mismatch\": 0 }"
    ));
    assert!(json.contains("\"result\": \"DIVERGES\""));
    assert!(json.contains("\"divergences\": 1"));
    assert!(json.contains("    \"score\": { \"checks\": 2, \"passes\": 1, \"rate\": 0.500 }"));
    assert!(json.contains(
            "\"first_divergence\": { \"frame\": 1, \"field\": \"score\", \"oracle\": \"100\", \"native\": \"101\" }"
        ));
    assert!(json.contains("\"px\": { \"max\": 1.000, \"mean\": 0.500 }"));
    assert!(json.contains("\"py\": { \"max\": 0.000, \"mean\": 0.000 }"));
    assert!(json.contains("\"objects\": { \"matches\": 0, \"pairs\": 0, \"rate\": null"));
}

#[test]
fn objects_compare_by_slot_kind_position_and_substate() {
    let mut oracle = snapshot(9);
    oracle.objects = vec![object(9, 5, 40, 50, 0), object(8, 6, 20, 20, 1)];
    let mut native = snapshot(9);
    native.objects = vec![object(9, 5, 41, 50, 0), object(7, 6, 20, 20, 1)];
    let report = run(&[oracle], &[native], 1);
    let fields: Vec<&str> = report
        .divergences
        .iter()
        .map(|divergence| divergence.field.as_str())
        .collect();
    assert_eq!(fields, ["obj.slot", "obj.slot"]);
    assert_eq!(report.object_count_delta, 0);
    assert_eq!(report.object_count_mismatch_frames, 0);
    assert_eq!(report.object_slot_matches, 1);
    assert_eq!(report.object_slot_pairs, 3);
    assert_eq!(report.object_slot_rate(), Some(333));
}

#[test]
fn matched_objects_compare_kind_position_and_substate() {
    let mut oracle = snapshot(7);
    oracle.objects = vec![object(9, 5, 40, 50, 0)];
    let mut native = snapshot(7);
    native.objects = vec![object(9, 6, 42, 51, 2)];
    let report = run(&[oracle], &[native], 1);
    let fields: Vec<&str> = report
        .divergences
        .iter()
        .map(|divergence| divergence.field.as_str())
        .collect();
    assert_eq!(fields, ["obj.kind", "obj.x", "obj.substate"]);
    assert_eq!(report.object_slot_matches, 1);
    // y moved by one pixel, which the default tolerance allows.
    assert_eq!(report.failures("obj.y"), 0);
}

#[test]
fn object_count_delta_tracks_the_last_frame_and_the_maximum() {
    let mut oracle = snapshot(1);
    oracle.objects = vec![object(9, 5, 0, 0, 0), object(8, 5, 0, 0, 0)];
    let native = snapshot(1);
    let mut oracle2 = snapshot(2);
    oracle2.objects = vec![object(9, 5, 0, 0, 0)];
    let mut native2 = snapshot(2);
    native2.objects = vec![object(9, 5, 0, 0, 0), object(8, 5, 0, 0, 0)];
    let report = run(&[oracle, oracle2], &[native, native2], 1);
    assert_eq!(report.object_count_delta, -1);
    assert_eq!(report.max_object_count_delta, 2);
    assert_eq!(report.object_count_mismatch_frames, 2);
}

#[test]
fn detailed_report_lists_at_most_ten_divergences() {
    let oracle: Vec<Snapshot> = (0..15).map(snapshot).collect();
    let mut natives: Vec<Snapshot> = (0..15).map(snapshot).collect();
    for native in &mut natives {
        native.score += 1;
    }
    let report = run(&oracle, &natives, 1);
    assert_eq!(report.divergences.len(), 15);
    assert_eq!(report.failures("score"), 15);
    let detailed = report.detailed();
    assert_eq!(detailed.matches("field score").count(), 1);
    assert_eq!(detailed.matches("  score ").count(), 10);
    assert!(detailed.contains("DIVERGES"));
    assert!(!report.summary().contains("first divergences"));
}
