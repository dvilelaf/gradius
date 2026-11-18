//! Report-only triage of the stage-7 fixture divergences.
//!
//! The stage-7 replay reports 604 divergences over the 297-6050 window, down
//! from 11,512 after the `$02` anchor seed landed.
//! This test reruns it and groups the divergences by the object slot, the
//! kind pair and the field, so a fix card can target a whole cause instead of
//! the first frame. It never gates; `docs/analysis/stage7_triage.md` carries
//! the measured table this prints.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot, SnapshotObject};

const FIRST_FRAME: u64 = 297;
const LAST_FRAME: u64 = 6050;
const STAGE_INDEX: u8 = 6;
const FRAME_OFFSET: u64 = FIRST_FRAME - 1;
const RECORDED_FRAMES: u64 = LAST_FRAME;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn replay() -> (Report, Vec<Snapshot>, Vec<Snapshot>) {
    let fixture_path = root().join("tests/fixtures/stage7-297-6050.state");
    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script = default_script(RECORDED_FRAMES);

    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for snapshot in &oracle {
        game.step(core_buttons(script.buttons(snapshot.frame - 1)));
        let mut native_snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        native_snapshot.frame += FRAME_OFFSET;
        native.push(native_snapshot);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    let report = compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, LAST_FRAME)),
            ..CompareConfig::default()
        },
    );
    (report, oracle, native)
}

/// One continuous run of divergences with the same key on the same slot.
#[derive(Default)]
struct Episode {
    start: u64,
    end: u64,
    frames: u64,
    /// First and mid sample: `(frame, oracle, native)` object text.
    samples: Vec<(u64, String, String)>,
}

type Key = (String, i64, i64); // (class, oracle kind, native kind), slot appended

fn object_text(object: Option<&SnapshotObject>) -> String {
    match object {
        Some(object) => format!(
            "k{} ({},{}) s{}",
            object.kind, object.x, object.y, object.substate
        ),
        None => "absent".to_string(),
    }
}

fn push(
    groups: &mut BTreeMap<(Key, i64), Episode>,
    key: &Key,
    slot: i64,
    frame: u64,
    oracle: Option<&SnapshotObject>,
    native: Option<&SnapshotObject>,
) {
    let episode = groups
        .entry((key.clone(), slot))
        .or_insert_with(|| Episode {
            start: frame,
            end: frame,
            ..Episode::default()
        });
    // A gap breaks the episode: the slot was reused by a different object.
    if frame > episode.end + 1 {
        *episode = Episode {
            start: frame,
            end: frame,
            ..Episode::default()
        };
    }
    episode.end = frame;
    episode.frames += 1;
    if episode.samples.len() < 2 && (episode.frames == 1 || episode.frames == 10) {
        episode
            .samples
            .push((frame, object_text(oracle), object_text(native)));
    }
}

#[test]
fn stage7_divergences_group_by_cause() {
    let (report, oracle, native) = replay();
    // The window is exact now: the turret `$02` grid, the launcher `Y` guard,
    // the stage-7 collision map (block 11) and the cannon tick are all fixed.
    // Any divergence here is a regression; `stage7_replay` gates it and this
    // probe keeps the grouping machinery for the next hunt.
    assert_eq!(
        report.divergences.len(),
        0,
        "stage 7 must replay exactly; see docs/analysis/stage7_triage.md"
    );

    let mut groups: BTreeMap<(Key, i64), Episode> = BTreeMap::new();
    // frame class -> (frames, first frame, last frame, divergence fields)
    type Class = (u64, u64, u64, BTreeMap<String, u64>);
    let mut classes: BTreeMap<&'static str, Class> = BTreeMap::new();

    for (oracle_frame, native_frame) in oracle.iter().zip(&native) {
        let frame = native_frame.frame;
        if !(FIRST_FRAME..=LAST_FRAME).contains(&frame) {
            continue;
        }
        let oracle_slots: BTreeMap<i64, &SnapshotObject> =
            oracle_frame.objects.iter().map(|o| (o.slot, o)).collect();
        let native_slots: BTreeMap<i64, &SnapshotObject> =
            native_frame.objects.iter().map(|o| (o.slot, o)).collect();

        // Do the two sides hold the same objects, and in the same slots?
        let key_of = |object: &SnapshotObject| {
            format!(
                "{}:{}:{}:{}",
                object.kind, object.substate, object.x, object.y
            )
        };
        let mut oracle_set: Vec<String> = oracle_frame.objects.iter().map(key_of).collect();
        let mut native_set: Vec<String> = native_frame.objects.iter().map(key_of).collect();
        oracle_set.sort();
        native_set.sort();
        let mut oracle_kinds: Vec<i64> = oracle_frame.objects.iter().map(|o| o.kind).collect();
        let mut native_kinds: Vec<i64> = native_frame.objects.iter().map(|o| o.kind).collect();
        oracle_kinds.sort();
        native_kinds.sort();
        let class = if oracle_set == native_set {
            "slot permutation"
        } else if oracle_kinds == native_kinds {
            "same kinds, state differs"
        } else {
            "population differs"
        };

        let slots: std::collections::BTreeSet<i64> = oracle_slots
            .keys()
            .chain(native_slots.keys())
            .copied()
            .collect();
        let mut frame_fields: BTreeMap<String, u64> = BTreeMap::new();
        for slot in slots {
            match (oracle_slots.get(&slot), native_slots.get(&slot)) {
                (Some(o), None) => {
                    *frame_fields.entry("obj.slot".to_string()).or_default() += 1;
                    push(
                        &mut groups,
                        &("native missing".to_string(), o.kind, -1),
                        slot,
                        frame,
                        Some(o),
                        None,
                    )
                }
                (None, Some(n)) => {
                    *frame_fields.entry("obj.slot".to_string()).or_default() += 1;
                    push(
                        &mut groups,
                        &("native extra".to_string(), -1, n.kind),
                        slot,
                        frame,
                        None,
                        Some(n),
                    )
                }
                (Some(o), Some(n)) => {
                    if o.kind != n.kind {
                        *frame_fields.entry("obj.kind".to_string()).or_default() += 1;
                        push(
                            &mut groups,
                            &("kind".to_string(), o.kind, n.kind),
                            slot,
                            frame,
                            Some(o),
                            Some(n),
                        );
                    }
                    if (o.x - n.x).abs() > 1 {
                        *frame_fields.entry("obj.x".to_string()).or_default() += 1;
                        let key = ("x".to_string(), o.kind, n.kind);
                        push(&mut groups, &key, slot, frame, Some(o), Some(n));
                    }
                    if (o.y - n.y).abs() > 1 {
                        *frame_fields.entry("obj.y".to_string()).or_default() += 1;
                        let key = ("y".to_string(), o.kind, n.kind);
                        push(&mut groups, &key, slot, frame, Some(o), Some(n));
                    }
                    if o.substate != n.substate {
                        *frame_fields.entry("obj.substate".to_string()).or_default() += 1;
                        push(
                            &mut groups,
                            &("substate".to_string(), o.kind, n.kind),
                            slot,
                            frame,
                            Some(o),
                            Some(n),
                        );
                    }
                }
                (None, None) => unreachable!(),
            }
        }

        if frame_fields.is_empty() {
            continue;
        }
        let entry = classes
            .entry(class)
            .or_insert_with(|| (0, frame, frame, BTreeMap::new()));
        entry.0 += 1;
        entry.2 = frame;
        for (field, count) in frame_fields {
            *entry.3.entry(field).or_default() += count;
        }
    }

    let mut totals: BTreeMap<Key, (u64, usize)> = BTreeMap::new();
    for ((key, _slot), episode) in &groups {
        let entry = totals.entry(key.clone()).or_default();
        entry.0 += episode.frames;
        entry.1 += 1;
    }

    let mut ordered: Vec<_> = totals.into_iter().collect();
    ordered.sort_by_key(|(_, (frames, _))| std::cmp::Reverse(*frames));

    eprintln!("\n=== divergence field totals ===");
    eprintln!("total divergences: {}", report.divergences.len());
    for field in [
        "px",
        "py",
        "speed",
        "scroll",
        "score",
        "lives",
        "meter",
        "obj.slot",
        "obj.kind",
        "obj.x",
        "obj.y",
        "obj.substate",
    ] {
        let fired = report
            .divergences
            .iter()
            .filter(|divergence| divergence.field == field)
            .count();
        eprintln!(
            "{:<14} failures {:>6}   divergences {:>6}",
            field,
            report.failures(field),
            fired
        );
    }

    eprintln!("\n=== frame classes ===");
    for (class, (frames, first, last, fields)) in &classes {
        let fields: Vec<String> = fields
            .iter()
            .map(|(field, count)| format!("{field} {count}"))
            .collect();
        eprintln!(
            "{class:<24} {frames:>5} frames, {first}-{last}: {}",
            fields.join(", ")
        );
    }

    eprintln!("\n=== object groups (class, oracle kind, native kind) ===");
    for ((class, oracle_kind, native_kind), (frames, episodes)) in &ordered {
        eprintln!(
            "{class:<13} o.kind {oracle_kind:>3} n.kind {native_kind:>3}: {frames:>6} frames, {episodes:>3} episodes"
        );
        let mut shown = 0;
        for ((key, slot), episode) in &groups {
            if key != &(class.clone(), *oracle_kind, *native_kind) || shown >= 12 {
                continue;
            }
            shown += 1;
            let samples: Vec<String> = episode
                .samples
                .iter()
                .map(|(frame, oracle, native)| format!("{frame}: o[{oracle}] n[{native}]"))
                .collect();
            eprintln!(
                "    slot {slot}: {}-{} ({} frames) {}",
                episode.start,
                episode.end,
                episode.frames,
                samples.join(" ")
            );
        }
    }
    eprintln!();
}
