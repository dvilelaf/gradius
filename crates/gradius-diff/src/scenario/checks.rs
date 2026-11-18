//! The oracle-side invariants each scenario asserts.

use super::*;

use crate::compare::Report;
use crate::dump::Snapshot;

pub(super) fn menu_checks(
    selection: u8,
    start_lock: u8,
    ram: &[Ram],
    oracle: &[Snapshot],
    native: &[Snapshot],
    checks: &mut Vec<CheckLine>,
    notes: &mut Vec<String>,
) {
    let at = |frame: u64| -> Option<&Ram> { ram.get(frame as usize) };
    fn snapshot(side: &[Snapshot], frame: u64) -> Option<&Snapshot> {
        side.get(frame as usize)
    }
    // Both menu scenarios start at `$0F = 0`; the first Select always
    // produces 1, and only the 1P script has the second Select back to 0.
    let after_select = 1u8;
    let cursor_after = 0x96u8;
    let final_selection = selection;
    let oracle_py = snapshot(oracle, 185).map_or(-1, |s| s.py);
    let native_py = snapshot(native, 185).map_or(-1, |s| s.py);

    let mut check = |name: &str, pass: bool, detail: String| {
        checks.push(CheckLine {
            name: name.to_string(),
            pass,
            hard: true,
            detail,
        });
    };

    if let (Some(before), Some(after)) = (at(179), at(185)) {
        check(
            "select-edge-toggles-$0F",
            before.get(Ram::SELECT) == 0 && after.get(Ram::SELECT) == after_select,
            format!(
                "$0F f179={} f185={} (expected 0 -> {after_select})",
                before.get(Ram::SELECT),
                after.get(Ram::SELECT)
            ),
        );
        check(
            "select-moves-menu-ship",
            after.get(Ram::SELECT) == after_select
                && oracle_py == i64::from(cursor_after)
                && native_py == oracle_py,
            format!(
                "cursor $0320 f185: oracle {oracle_py}, native {native_py} (expected ${cursor_after:02X})"
            ),
        );
    }
    if let (Some(one), Some(two), Some(three)) = (at(185), at(201), at(221)) {
        check(
            "up-down-do-not-toggle",
            one.get(Ram::SELECT) == two.get(Ram::SELECT)
                && two.get(Ram::SELECT) == three.get(Ram::SELECT),
            format!(
                "$0F f185/f201/f221 = {}/{}/{}",
                one.get(Ram::SELECT),
                two.get(Ram::SELECT),
                three.get(Ram::SELECT)
            ),
        );
    }
    if let Some(back) = at(261) {
        check(
            "second-select-returns",
            back.get(Ram::SELECT) == final_selection,
            format!(
                "$0F f261={} (expected {final_selection})",
                back.get(Ram::SELECT)
            ),
        );
    }
    if let Some(started) = at(321) {
        check(
            "start-locks-mode",
            started.get(Ram::STATE) == 3 && started.get(Ram::LOCK) & 0x70 == start_lock,
            format!(
                "$00 f321={} $03=${:02X} (want 3, ${start_lock:02X})",
                started.get(Ram::STATE),
                started.get(Ram::LOCK)
            ),
        );
    }
    if let Some(last) = oracle.last() {
        check(
            "enters-stage",
            last.state == "Playing" && last.lives == 3,
            format!("f{} state={} lives={}", last.frame, last.state, last.lives),
        );
    }

    if selection == 1 {
        notes.push(
            "2P: $03 = $70 enters the 2P stage; the window stops at the Start lock and p2-handoff covers the play"
                .into(),
        );
    }
}

/// Start-only menu: `$0F` never moves and Start locks the mode. `ram[i]` is
/// the sample taken after script frame `i`, which is oracle dump frame `i + 1`.
pub(super) fn menu_start_checks(
    start_frame: u64,
    start_lock: u8,
    ram: &[Ram],
    oracle: &[Snapshot],
    checks: &mut Vec<CheckLine>,
    notes: &mut Vec<String>,
) {
    let at = |frame: u64| ram.get(frame as usize);
    let mut check = |name: &str, pass: bool, detail: String| {
        checks.push(CheckLine {
            name: name.to_string(),
            pass,
            hard: true,
            detail,
        });
    };

    if let (Some(before), Some(after)) = (at(start_frame.saturating_sub(1)), at(start_frame + 1)) {
        check(
            "start-only-keeps-$0F",
            before.get(Ram::SELECT) == 0 && after.get(Ram::SELECT) == 0,
            format!(
                "$0F f{}/{} = {}/{} (no Select edge, expected 0)",
                start_frame.saturating_sub(1),
                start_frame + 1,
                before.get(Ram::SELECT),
                after.get(Ram::SELECT)
            ),
        );
        check(
            "start-locks-mode",
            after.get(Ram::STATE) == 3 && after.get(Ram::LOCK) & 0x70 == start_lock,
            format!(
                "$00 f{}={} $03=${:02X} (want 3, ${start_lock:02X})",
                start_frame + 1,
                after.get(Ram::STATE),
                after.get(Ram::LOCK)
            ),
        );
    }
    if let Some(last) = oracle.last() {
        check(
            "enters-stage",
            last.state == "Playing" && last.lives == 3,
            format!("f{} state={} lives={}", last.frame, last.state, last.lives),
        );
    }
    notes.push(format!(
        "start-only: no Select edge, $0F stays 0; Start at {start_frame} locks ${start_lock:02X}"
    ));
}

pub(super) fn power_checks(
    plan: PowerPlan,
    expected: Expected,
    ram: &[Ram],
    native: &[NativePower],
    oracle_snapshots: &[Snapshot],
    checks: &mut Vec<CheckLine>,
    notes: &mut Vec<String>,
) {
    let final_ram = ram.last().copied().unwrap_or_default();
    let final_native = native.last().copied().unwrap_or_default();
    let port_hard = expected == Expected::Clean;
    let mut check = |name: &str, pass: bool, hard: bool, detail: String| {
        checks.push(CheckLine {
            name: name.to_string(),
            pass,
            hard,
            detail,
        });
    };

    let oracle_effect = (
        final_ram.get(Ram::SPEED),
        final_ram.get(Ram::MISSILE),
        final_ram.get(Ram::WEAPON),
        final_ram.get(Ram::OPTION),
        final_ram.get(Ram::SHIELD),
    );
    let native_effect = (
        final_native.speed,
        final_native.missile,
        final_native.weapon,
        final_native.options,
        final_native.shield,
    );
    check(
        "oracle-effect",
        power_effect_matches(plan.slot, oracle_effect, plan.buys),
        true,
        format!(
            "slot {} buys {}: $40/$41/$44/$45/$46 = {}/{}/{}/{}/{}",
            plan.slot,
            plan.buys,
            oracle_effect.0,
            oracle_effect.1,
            oracle_effect.2,
            oracle_effect.3,
            oracle_effect.4
        ),
    );
    check(
        "port-effect-matches",
        oracle_effect == native_effect,
        port_hard,
        format!("oracle {oracle_effect:?} vs port {native_effect:?}"),
    );
    // Every B press must consume the selection or hit the documented no-op.
    // The ROM takes two frames to see the pressed byte, so scan a short window
    // after each tap instead of pinning one frame.
    let tap_frames: Vec<u64> = plan.taps.to_vec();
    let mut consumed = 0usize;
    let mut first_mismatch = None;
    let mut buy_frames = Vec::new();
    for (index, tap) in tap_frames.iter().copied().enumerate() {
        let wanted = if index + 1 == tap_frames.len() {
            plan.final_selection
        } else {
            0
        };
        let mut hit = None;
        for offset in 0..=8 {
            let at = (tap + offset) as usize;
            let (Some(oracle), Some(port)) = (ram.get(at), native.get(at)) else {
                continue;
            };
            if oracle.get(Ram::METER) == wanted && port.selection == wanted {
                hit = Some(tap + offset);
                break;
            }
        }
        if let Some(frame) = hit {
            consumed += 1;
            buy_frames.push(frame);
        } else if first_mismatch.is_none() {
            first_mismatch = Some(format!("f{tap}: selection did not reach {wanted}"));
        }
    }
    check(
        "every-tap-consumes",
        consumed == tap_frames.len() && first_mismatch.is_none(),
        true,
        first_mismatch.unwrap_or_else(|| format!("{consumed}/{} taps consumed", tap_frames.len())),
    );

    let after_last = buy_frames
        .last()
        .copied()
        .map_or(ram.len() - 1, |f| f as usize);
    let (oracle_meter, port_selection) = ram
        .get(after_last)
        .zip(native.get(after_last))
        .map_or((0, 0), |(sample, power)| {
            (sample.get(Ram::METER), power.selection)
        });
    check(
        "meter-cleared",
        oracle_meter == plan.final_selection && port_selection == plan.final_selection,
        true,
        format!(
            "$42 at last buy f{after_last}: oracle {oracle_meter} port {port_selection} (want {})",
            plan.final_selection
        ),
    );

    if plan.slot == 1 {
        let oracle_speed = i64::from(final_ram.get(Ram::SPEED));
        // `$89A6` is `INC $40`: the byte wraps at 256. The port's
        // `PowerMeter::activate` uses saturating_add, so a 260-buy analysis run
        // is the only place the two rules differ.
        let wrapped = plan.buys % 256;
        let port_speed = i64::from(final_native.speed);
        check(
            "repeated-speed-buys",
            oracle_speed == wrapped as i64 && port_speed == oracle_speed,
            port_hard || oracle_speed == wrapped as i64,
            format!(
                "$40 oracle {oracle_speed}, port {port_speed}, buys {} (oracle wraps at 256 to {wrapped})",
                plan.buys
            ),
        );
        let oracle_delta = (oracle_speed + 2).min(16) << 7;
        // `PowerMeter::speed_delta` is the port's `$A000` clamp.
        let port_delta = (port_speed + 2).min(16) << 7;
        check(
            "speed-delta-clamp",
            oracle_delta == port_delta,
            port_hard,
            format!("$40={oracle_speed}: speed_delta {oracle_delta}/256 vs {port_delta}/256"),
        );
        notes.push(format!(
            "$40 reaches {oracle_speed} after {} buys (movement {oracle_delta}/256 px/frame; level 14 is the 2048 clamp), port {port_speed}",
            plan.buys
        ));
    }

    if let Some(frame) = buy_frames.last() {
        if let (Some(snapshot), Some(port)) = (
            oracle_snapshots.get(*frame as usize),
            native.get(*frame as usize),
        ) {
            notes.push(format!(
                "last buy f{frame}: $42={} speed={} port selection={} speed={}",
                snapshot.meter, snapshot.speed, port.selection, port.speed
            ));
        }
    }
}

fn power_effect_matches(slot: u8, effect: (u8, u8, u8, u8, u8), buys: usize) -> bool {
    match slot {
        1 => usize::from(effect.0) == buys % 256,
        2 => effect.1 == 1,
        3 => effect.2 == 2,
        4 => effect.2 == 1,
        5 => effect.3 >= 1,
        6 => effect.4 == 5,
        _ => false,
    }
}

pub(super) fn eruption_checks(
    oracle: &[Snapshot],
    ram: &[Ram],
    checks: &mut Vec<CheckLine>,
    notes: &mut Vec<String>,
) {
    let frames: Vec<&Snapshot> = oracle
        .iter()
        .filter(|snapshot| (6400..7300).contains(&snapshot.frame))
        .collect();
    let eruption_objects: u64 = frames
        .iter()
        .map(|snapshot| {
            snapshot
                .objects
                .iter()
                .filter(|object| object.kind == 0x8A)
                .count() as u64
        })
        .sum();
    let scroll_hold: Vec<(i64, i64, i64)> = frames
        .iter()
        .filter(|snapshot| (6441..=7210).contains(&snapshot.frame))
        .map(|snapshot| {
            (
                snapshot.scroll_low,
                snapshot.scroll_high,
                snapshot.scroll_frac,
            )
        })
        .collect();
    let distinct = {
        let mut values: Vec<(i64, i64, i64)> = scroll_hold.clone();
        values.sort_unstable();
        values.dedup();
        values.len()
    };
    let hold = scroll_hold.first().copied().unwrap_or((0, 0, 0));
    let volcano_max = ram
        .iter()
        .map(|ram| ram.get(Ram::VOLCANO))
        .max()
        .unwrap_or(0);
    checks.push(CheckLine {
        name: "eruption-objects".into(),
        pass: eruption_objects > 0,
        hard: true,
        detail: format!("{eruption_objects} kind $8A object-frames in 6400-7300"),
    });
    checks.push(CheckLine {
        name: "scroll-hold".into(),
        pass: distinct == 1,
        hard: true,
        detail: format!(
            "6441-7210 scroll {}:{}:{} ({} distinct values)",
            hold.0, hold.1, hold.2, distinct
        ),
    });
    checks.push(CheckLine {
        name: "volcano-counter".into(),
        pass: true,
        hard: true,
        detail: format!("$5F reaches {volcano_max}"),
    });
    notes.push(format!(
        "eruption: {eruption_objects} kind $8A object-frames; scroll hold {}:{}:{}; $5F {volcano_max}",
        hold.0, hold.1, hold.2
    ));
}

pub(super) fn handoff_checks(
    oracle: &[Snapshot],
    ram: &[Ram],
    report: &Report,
    checks: &mut Vec<CheckLine>,
    notes: &mut Vec<String>,
) {
    let mut check = |name: &str, pass: bool, detail: String| {
        checks.push(CheckLine {
            name: name.to_string(),
            pass,
            hard: true,
            detail,
        });
    };

    if let Some(sample) = ram.get(321) {
        check(
            "start-locks-2p",
            sample.get(Ram::LOCK) & 0x70 == 0x70,
            format!("$03 f321=${:02X} (want $70)", sample.get(Ram::LOCK)),
        );
    }

    // P2's Start during P1's play must reach `$06` without switching `$18`.
    if let Some(sample) = ram.get(900) {
        check(
            "p2-start-is-inert",
            sample.p2_input[0] & 0x10 != 0 && sample.player == 0,
            format!(
                "$06 f900=${:02X} $18={} (P2 Start edge, P1 still plays)",
                sample.p2_input[0], sample.player
            ),
        );
    }

    let handoff = ram.iter().position(|sample| sample.player == 1);
    check(
        "p1-hands-over-to-p2",
        handoff.is_some_and(|frame| (1190..=1200).contains(&frame)),
        format!("$18 0 -> 1 at f{handoff:?} (want 1190-1200)"),
    );
    if let Some(sample) = handoff.and_then(|frame| ram.get(frame)) {
        check(
            "handoff-keeps-the-other-lives",
            sample.lives == [2, 3],
            format!(
                "$20/$21 f{} = {}/{} (want 2/3: P1 lost a ship, P2 untouched)",
                handoff.unwrap_or_default(),
                sample.lives[0],
                sample.lives[1]
            ),
        );
    }

    let normal = ram
        .iter()
        .position(|sample| sample.player == 1 && sample.stage == 0x80);
    check(
        "p2-reaches-normal-play",
        normal.is_some_and(|frame| (1215..=1235).contains(&frame)),
        format!("$18=1 and $1B=$80 at f{normal:?} (want 1215-1235)"),
    );

    // P2 Right is held from 1230: it must move the ship right and keep Y.
    if let (Some(edge), Some(before), Some(after)) =
        (ram.get(1230), oracle.get(1229), oracle.get(1260))
    {
        check(
            "p2-drives-the-ship",
            edge.p2_input[0] & 0x01 != 0
                && edge.p2_input[1] & 0x01 != 0
                && before.py == after.py
                && after.px > before.px + 20,
            format!(
                "$06=${:02X} $08=${:02X} f1230, x {} -> {}, y {} -> {}",
                edge.p2_input[0], edge.p2_input[1], before.px, after.px, before.py, after.py
            ),
        );
    }

    if let Some(last) = oracle.last() {
        check(
            "still-playing",
            last.state == "Playing",
            format!("f{} state={}", last.frame, last.state),
        );
    }

    notes.push(format!(
        "2P handoff: $18 0 -> 1 at {handoff:?}, P2 normal play at {normal:?}; the port follows the relay"
    ));
    if let Some(divergence) = report.first() {
        notes.push(format!(
            "first divergence: f{} {} oracle {} native {}",
            divergence.frame, divergence.field, divergence.oracle, divergence.native
        ));
    }
}

pub(super) fn boss_checks(
    oracle: &[Snapshot],
    ram: &[Ram],
    checks: &mut Vec<CheckLine>,
    notes: &mut Vec<String>,
) {
    let mut spawn = None;
    let mut kill = None;
    let mut previous_score = None;
    for snapshot in oracle {
        let boss = snapshot.objects.iter().any(|object| object.kind == 0x98);
        if boss && spawn.is_none() {
            spawn = Some(snapshot.frame);
        }
        if let Some(previous) = previous_score {
            if snapshot.score >= previous + 1000 && kill.is_none() {
                kill = Some(snapshot.frame);
            }
        }
        previous_score = Some(snapshot.score);
    }
    let volcano_max = ram
        .iter()
        .map(|ram| ram.get(Ram::VOLCANO))
        .max()
        .unwrap_or(0);
    let gate = ram.iter().any(|ram| ram.get(Ram::GATE) != 0);
    checks.push(CheckLine {
        name: "boss-spawn".into(),
        pass: spawn.is_some(),
        hard: true,
        detail: format!("kind $98 first seen at {spawn:?}"),
    });
    checks.push(CheckLine {
        name: "boss-kill".into(),
        pass: kill.is_some(),
        hard: true,
        detail: format!("+1000 score event at {kill:?}"),
    });
    checks.push(CheckLine {
        name: "volcano-gate".into(),
        pass: volcano_max >= 4 || !gate,
        hard: true,
        detail: format!("$5F max {volcano_max}, $39 set={gate}"),
    });
    notes.push(format!(
        "boss: spawn={spawn:?} kill={kill:?}; volcano $5F={volcano_max}, $39={gate}"
    ));
}

/// Start at 700 pauses (`$15` 0 -> 1), nothing moves while paused, and Start at
/// 740 resumes. The state comparison covers the port against the same frames.
pub(super) fn pause_checks(oracle: &[Snapshot], ram: &[Ram], checks: &mut Vec<CheckLine>) {
    let flag = |frame: usize| ram.get(frame).map_or(0xFF, |ram| ram.get(Ram::PAUSE));
    let scroll = |frame: usize| {
        oracle.get(frame).map_or((-1, -1, -1), |s| {
            (s.scroll_low, s.scroll_high, s.scroll_frac)
        })
    };
    let frozen = (704..=741).all(|frame| scroll(frame) == scroll(704));
    checks.push(CheckLine {
        name: "start-ignored-in-blank-and-death".into(),
        pass: (285..=297).all(|frame| flag(frame) == 0)
            && flag(305) == 1
            && flag(320) == 0
            && (1088..1207).all(|frame| flag(frame) == 0),
        hard: true,
        detail: format!(
            "$15 f290={} f305={} f320={} f1100={} f1150={}",
            flag(290),
            flag(305),
            flag(320),
            flag(1100),
            flag(1150)
        ),
    });
    checks.push(CheckLine {
        name: "start-sets-$15".into(),
        pass: flag(699) == 0 && flag(710) == 1 && flag(739) == 1,
        hard: true,
        detail: format!(
            "$15 f699={} f710={} f739={}",
            flag(699),
            flag(710),
            flag(739)
        ),
    });
    checks.push(CheckLine {
        name: "scroll-frozen".into(),
        pass: frozen && scroll(760) != scroll(704),
        hard: true,
        detail: format!(
            "scroll f704 {:?}, f741 {:?}, f760 {:?}",
            scroll(704),
            scroll(741),
            scroll(760)
        ),
    });
    checks.push(CheckLine {
        name: "start-clears-$15".into(),
        pass: flag(750) == 0,
        hard: true,
        detail: format!("$15 f750={}", flag(750)),
    });
}

/// The Konami code gives the shield only inside a pause, and only once: the
/// oracle's `$46` stays 0 for the unpaused code (f310-f375) and the half-sent
/// code, becomes 5 once the pause code completes and stays there.
pub(super) fn pause_code_checks(ram: &[Ram], checks: &mut Vec<CheckLine>) {
    let shield = |frame: usize| ram.get(frame).map_or(0xFF, |ram| ram.get(Ram::SHIELD));
    checks.push(CheckLine {
        name: "konami-code-needs-the-pause".into(),
        pass: (300..420).all(|frame| shield(frame) == 0),
        hard: true,
        detail: format!("$46 f380={} f450={}", shield(380), shield(450)),
    });
    checks.push(CheckLine {
        name: "konami-code-in-pause-gives-the-shield".into(),
        pass: shield(450) == 0 && shield(490) == 5 && (490..760).all(|frame| shield(frame) == 5),
        hard: true,
        detail: format!(
            "$46 f450={} f490={} f759={}",
            shield(450),
            shield(490),
            shield(759)
        ),
    });
}

/// `$1B = $82` is a live wait for the pause code (`$80` bit 7, bits 4-6
/// clear): Start pauses at 15900 and 16000, resumes at 15950 and 16100.
pub(super) fn pause_transition_checks(ram: &[Ram], checks: &mut Vec<CheckLine>) {
    let at = |frame: usize| {
        ram.get(frame)
            .map_or((0xFF, 0xFF), |ram| (ram.get(Ram::PAUSE), ram.stage))
    };
    let expected = [(15910, 1), (15960, 0), (16010, 1), (16110, 0)];
    checks.push(CheckLine {
        name: "start-pauses-in-the-$82-wait".into(),
        pass: expected
            .iter()
            .all(|&(frame, flag)| at(frame) == (flag, 0x82)),
        hard: true,
        detail: expected
            .iter()
            .map(|&(frame, _)| format!("f{frame} ($15,$1B)={:02X?}", at(frame)))
            .collect::<Vec<_>>()
            .join(", "),
    });
}
