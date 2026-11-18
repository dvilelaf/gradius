//! The comparison itself, and the number formatting its report uses.

use super::*;

/// Compare a whole run. Pairs are `(oracle, native)` for each frame; only
/// frames where both sides are `Playing` are compared.
pub fn compare<'a, I>(pairs: I, config: CompareConfig) -> Report
where
    I: IntoIterator<Item = (&'a Snapshot, &'a Snapshot)>,
{
    let mut report = Report {
        frames: 0,
        requested_window: config.window,
        compared: 0,
        window_first: None,
        window_last: None,
        state_mismatch_frames: 0,
        tolerance_px: config.tolerance_px,
        divergences: Vec::new(),
        // Every field the comparator knows about, so a field that never had a
        // comparable pair (no matched object slot, say) shows as `0/0` instead
        // of silently disappearing from the report.
        fields: COMPARED_FIELDS
            .iter()
            .map(|field| (field.to_string(), FieldStat::default()))
            .collect(),
        object_count_delta: 0,
        max_object_count_delta: 0,
        object_count_mismatch_frames: 0,
        object_slot_matches: 0,
        object_slot_pairs: 0,
        px_delta: Delta::default(),
        py_delta: Delta::default(),
    };
    for (oracle, native) in pairs {
        report.frames += 1;
        let inside = config.window.map_or(true, |(from, to)| {
            native.frame >= from && native.frame <= to
        });
        // An explicit window compares whatever state the sides are in; the
        // default window only compares frames where both sides are Playing.
        let comparable = inside
            && (config.window.is_some()
                || (oracle.state == "Playing" && native.state == "Playing"));
        if !comparable {
            if oracle.state != native.state {
                report.state_mismatch_frames += 1;
            }
            continue;
        }
        report.compared += 1;
        report.window_first.get_or_insert(native.frame);
        report.window_last = Some(native.frame);
        compare_frame(oracle, native, config, &mut report);
    }
    report
}

/// Compare one frame inside the Playing window, appending to the report's
/// divergence list, counts and deltas.
pub fn compare_frame(
    oracle: &Snapshot,
    native: &Snapshot,
    config: CompareConfig,
    report: &mut Report,
) {
    let frame = native.frame;
    exact_i(
        report,
        frame,
        "frame",
        oracle.frame as i64,
        native.frame as i64,
    );
    let px_delta = position(
        report,
        frame,
        "px",
        oracle.x_fixed(),
        native.x_fixed(),
        config,
    );
    report.px_delta.add(px_delta);
    let py_delta = position(
        report,
        frame,
        "py",
        oracle.y_fixed(),
        native.y_fixed(),
        config,
    );
    report.py_delta.add(py_delta);
    exact_i(report, frame, "speed", oracle.speed, native.speed);
    exact_s(
        report,
        frame,
        "scroll",
        &format!(
            "{}:{}:{}",
            oracle.scroll_low, oracle.scroll_high, oracle.scroll_frac
        ),
        &format!(
            "{}:{}:{}",
            native.scroll_low, native.scroll_high, native.scroll_frac
        ),
    );
    exact_i(
        report,
        frame,
        "score",
        oracle.score as i64,
        native.score as i64,
    );
    exact_i(report, frame, "lives", oracle.lives, native.lives);
    exact_i(report, frame, "meter", oracle.meter, native.meter);
    if config.compare_objects {
        compare_objects(oracle, native, config, report);
    }
}

fn note(report: &mut Report, frame: u64, field: &str, oracle: String, native: String) {
    report.divergences.push(Divergence {
        frame,
        field: field.to_string(),
        oracle,
        native,
    });
}

fn exact_s(report: &mut Report, frame: u64, field: &str, oracle: &str, native: &str) {
    if !report.record(field, oracle == native) {
        note(report, frame, field, oracle.to_string(), native.to_string());
    }
}

fn exact_i(report: &mut Report, frame: u64, field: &str, oracle: i64, native: i64) {
    if !report.record(field, oracle == native) {
        note(report, frame, field, oracle.to_string(), native.to_string());
    }
}

/// Compare one 8.8 fixed-point position; returns the absolute error.
fn position(
    report: &mut Report,
    frame: u64,
    field: &str,
    oracle: i64,
    native: i64,
    config: CompareConfig,
) -> i64 {
    let delta = (oracle - native).abs();
    if !report.record(field, delta <= config.tolerance_px * 256) {
        note(
            report,
            frame,
            field,
            format_fixed(oracle),
            format_fixed(native),
        );
    }
    delta
}

fn compare_objects(
    oracle: &Snapshot,
    native: &Snapshot,
    config: CompareConfig,
    report: &mut Report,
) {
    let frame = native.frame;
    let delta = oracle.objects.len() as i64 - native.objects.len() as i64;
    report.object_count_delta = delta;
    report.max_object_count_delta = report.max_object_count_delta.max(delta.abs());
    if delta != 0 {
        report.object_count_mismatch_frames += 1;
    }
    let oracle_slots: BTreeMap<i64, &SnapshotObject> = oracle
        .objects
        .iter()
        .map(|object| (object.slot, object))
        .collect();
    let native_slots: BTreeMap<i64, &SnapshotObject> = native
        .objects
        .iter()
        .map(|object| (object.slot, object))
        .collect();
    let slots: std::collections::BTreeSet<i64> = oracle_slots
        .keys()
        .chain(native_slots.keys())
        .copied()
        .collect();
    for slot in slots {
        report.object_slot_pairs += 1;
        match (oracle_slots.get(&slot), native_slots.get(&slot)) {
            (Some(oracle_object), Some(native_object)) => {
                report.object_slot_matches += 1;
                if !report.record("obj.kind", oracle_object.kind == native_object.kind) {
                    note(
                        report,
                        frame,
                        "obj.kind",
                        format!("slot {slot}: {}", oracle_object.kind),
                        format!("slot {slot}: {}", native_object.kind),
                    );
                }
                let tolerance = config.tolerance_px;
                if !report.record(
                    "obj.x",
                    (oracle_object.x - native_object.x).abs() <= tolerance,
                ) {
                    note(
                        report,
                        frame,
                        "obj.x",
                        format!("slot {slot}: {}", oracle_object.x),
                        format!("slot {slot}: {}", native_object.x),
                    );
                }
                if !report.record(
                    "obj.y",
                    (oracle_object.y - native_object.y).abs() <= tolerance,
                ) {
                    note(
                        report,
                        frame,
                        "obj.y",
                        format!("slot {slot}: {}", oracle_object.y),
                        format!("slot {slot}: {}", native_object.y),
                    );
                }
                if !report.record(
                    "obj.substate",
                    oracle_object.substate == native_object.substate,
                ) {
                    note(
                        report,
                        frame,
                        "obj.substate",
                        format!("slot {slot}: {}", oracle_object.substate),
                        format!("slot {slot}: {}", native_object.substate),
                    );
                }
            }
            (Some(oracle_object), None) => {
                report.record("obj.slot", false);
                note(
                    report,
                    frame,
                    "obj.slot",
                    format!("slot {slot}: kind {}", oracle_object.kind),
                    "absent".to_string(),
                );
            }
            (None, Some(native_object)) => {
                report.record("obj.slot", false);
                note(
                    report,
                    frame,
                    "obj.slot",
                    "absent".to_string(),
                    format!("slot {slot}: kind {}", native_object.kind),
                );
            }
            (None, None) => unreachable!(),
        }
    }
}

pub(super) fn rate_thousandths(passes: u64, checks: u64) -> Option<i64> {
    (passes * 1000 + checks / 2)
        .checked_div(checks)
        .map(|rate| rate as i64)
}

/// `1.000px`-style percentage with one decimal, or `-` without checks.
pub(super) fn format_rate(rate: Option<i64>) -> String {
    match rate {
        Some(rate) => format!("{}.{}%", rate / 10, rate % 10),
        None => "-".to_string(),
    }
}

pub(super) fn format_thousandths(value: i64) -> String {
    format!("{}.{:03}", value.div_euclid(1000), value.rem_euclid(1000))
}

pub(super) fn json_rate(rate: Option<i64>) -> String {
    match rate {
        Some(rate) => format_thousandths(rate),
        None => "null".to_string(),
    }
}

pub(super) fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// `80` -> `80.00`, used to print 8.8 fixed-point positions.
pub(super) fn format_fixed(value: i64) -> String {
    format!(
        "{}.{:02}",
        value.div_euclid(256),
        value.rem_euclid(256) * 100 / 256
    )
}
