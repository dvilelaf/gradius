//! Oracle recording commands: shot, state, input, boss, transition.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use gradius_nes::Emulator;

use crate::transition as transition_ledger;
use crate::{boss, input, oracle, pngout, screens, state};

use super::shared::{input_plan, load, warn_machine_state};

pub(crate) fn shot(
    rom: &Path,
    out: &Path,
    frame: u64,
    buttons: Option<&str>,
    presses: &[String],
    keep_alive: bool,
) -> Result<()> {
    let cart = load(rom)?;
    let mut emulator = Emulator::new(&cart)?;
    let plan = input_plan(buttons, presses)?;
    let mut runner = oracle::Runner::new();
    runner.keep_alive = keep_alive;
    runner.run_to(&mut emulator, frame, &plan, false);
    let rgb = emulator.rgb();
    pngout::write_rgb(
        out,
        gradius_nes::SCREEN_WIDTH as u32,
        gradius_nes::SCREEN_HEIGHT as u32,
        &rgb,
    )?;
    println!(
        "frame {} -> {} (instructions: {}, visited PCs: {})",
        frame,
        out.display(),
        emulator.bus.trace.instructions,
        emulator.bus.trace.visited_addresses()
    );
    warn_machine_state(&emulator);
    Ok(())
}

pub(crate) fn record_state(
    rom: &Path,
    frames: u64,
    buttons: Option<&str>,
    presses: &[String],
    out: Option<&Path>,
) -> Result<()> {
    if frames == 0 {
        bail!("--frames must be at least 1");
    }
    let cart = load(rom)?;
    let mut emulator = Emulator::new(&cart)?;
    let plan = input_plan(buttons, presses)?;
    let mut runner = oracle::Runner::new();
    let mut text = String::with_capacity(frames as usize * 120);
    while emulator.frames < frames {
        runner.advance(&mut emulator, &plan, false);
        text.push_str(&state::StateLine::from_emulator(&emulator).render());
        text.push('\n');
    }
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            println!(
                "wrote {} ({} frames, {} bytes)",
                path.display(),
                frames,
                text.len()
            );
        }
        None => print!("{text}"),
    }
    warn_machine_state(&emulator);
    Ok(())
}

/// Record the held buttons and game/stage state once per frame.
pub(crate) fn record_input(
    rom: &Path,
    frames: u64,
    buttons: Option<&str>,
    presses: &[String],
    out: Option<&Path>,
    script: Option<&Path>,
) -> Result<()> {
    if frames == 0 {
        bail!("--frames must be at least 1");
    }
    let cart = load(rom)?;
    let mut emulator = Emulator::new(&cart)?;
    let plan = input_plan(buttons, presses)?;
    let mut runner = oracle::Runner::new();
    let mut text = String::with_capacity(frames as usize * 90);
    let mut samples = Vec::with_capacity(frames as usize);
    while emulator.frames < frames {
        let buttons = runner.advance_sampled(&mut emulator, &plan, 0x07);
        let mut line = input::InputLine::from_emulator(&emulator);
        line.buttons = buttons;
        text.push_str(&line.render());
        text.push('\n');
        samples.push((line.frame.saturating_sub(1), buttons));
    }
    let replay = input::compress_runs(samples);
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            println!(
                "wrote {} ({} frames, {} bytes)",
                path.display(),
                frames,
                text.len()
            );
        }
        None => print!("{text}"),
    }
    if let Some(path) = script {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, &replay).with_context(|| format!("writing {}", path.display()))?;
        println!(
            "wrote {} ({} press lines)",
            path.display(),
            replay.lines().count()
        );
    }
    warn_machine_state(&emulator);
    Ok(())
}

/// Record a stage's Big Core fight frame by frame.
///
/// The script is deliberately small and deterministic:
///
/// * Start is pressed at frame 180 for six frames (the title needs one press;
///   240 is equivalent and is accepted by `record-state`).
/// * A is held for the whole run, so the ship auto-fires.
/// * Before the core: the ship hovers near y = 96 and moves vertically away
///   from the nearest object in a 56 px x 56 px box around it. Enemy
///   formations home on the ship's Y, so this is the dodge.
/// * Once the core is live: the ship leads the core's Y by the shot flight,
///   sweeps across it and stops dodging.
///
/// `stage` is 1-based. Stages after 1 are reached by poking the single-player
/// stage copy `$26` before stage init, matching `screens --stage`.
///
/// The output is one `BossLine` per frame (the native state line plus
/// `boss=<slot>:<x>:<y>:<damage>:<overheat>:<counter>`). Stages 1-5 exit
/// non-zero with the observed counters if the core never spawns or never takes
/// a hit; stages 6-7 exit non-zero only if the stage does not end.
pub(crate) fn record_boss(rom: &Path, frames: u64, stage: u8, out: Option<&Path>) -> Result<()> {
    if frames == 0 {
        bail!("--frames must be at least 1");
    }
    if !(1..=7).contains(&stage) {
        bail!("--stage must be 1-7, got {stage}");
    }
    let target_stage = stage - 1;
    let cart = load(rom)?;
    let mut emulator = Emulator::new(&cart)?;
    let mut runner = oracle::Runner::new();
    // Analysis-only: a purely scripted ship cannot reliably survive to the
    // core, so the run pins the player-death bytes exactly like the ramdump /
    // disasm commands in docs/analysis/enemy_ai.md. No enemy or boss code is
    // changed; the fire/dodge script below still has to aim at the core.
    runner.keep_alive = true;
    let mut text = String::with_capacity(frames as usize * 130);
    let mut timeline = boss::BossTimeline::default();
    let mut last_boss_y = None;
    let mut scroll = emulator.player_state().scroll;
    let mut scroll_stop = None;
    let mut last_scroll_change = 0;
    let mut next_stage = None;
    let mut next_stage_armed = false;
    while emulator.frames < frames {
        let ram = &mut emulator.bus.ram;
        if screens::needs_stage_poke(ram[0x00], ram[0x19], target_stage) {
            // Single-player: X = $18 is 0 at stage init.
            ram[0x26] = target_stage;
        }
        let buttons = boss_buttons(&emulator, &mut last_boss_y, stage);
        let plan = oracle::Plan {
            held: 0,
            schedule: vec![(emulator.frames, buttons, 1)],
        };
        runner.advance(&mut emulator, &plan, false);
        let state = boss::BossLine::from_emulator(&emulator);
        let player = emulator.player_state();
        if emulator.bus.ram[0x19] == target_stage {
            next_stage_armed = true;
        }
        if player.scroll != scroll {
            scroll = player.scroll;
            last_scroll_change = emulator.frames;
        }
        if timeline.spawn_frame.is_none() && state.boss.is_some() {
            scroll_stop = Some(last_scroll_change);
        }
        if next_stage_armed && next_stage.is_none() && emulator.bus.ram[0x19] != target_stage {
            next_stage = Some(emulator.frames);
        }
        timeline.observe(emulator.frames, player.score(), state.boss);
        text.push_str(&state.render());
        text.push('\n');
    }

    let summary = timeline.summary(frames, scroll_stop, next_stage, stage);
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            println!(
                "wrote {} ({} frames, {} bytes)",
                path.display(),
                frames,
                text.len()
            );
        }
        None => print!("{text}"),
    }
    println!("{summary}");
    timeline.check_stage(stage, frames, next_stage)?;
    Ok(())
}

/// The scripted dodge/fire input for one frame.
///
/// Stage 1 keeps the historical alternating-sweep policy behind
/// `scripts/boss-script.txt`; other stages use a sawtooth that crosses the
/// exact hit line (`core y - 7`) once per 32 frames, which is more robust to
/// the stage-dependent power rank changing the core's vertical speed.
pub(crate) fn boss_buttons(emulator: &Emulator, last_boss_y: &mut Option<i32>, stage: u8) -> u8 {
    use gradius_nes::{BUTTON_A, BUTTON_DOWN, BUTTON_LEFT, BUTTON_RIGHT, BUTTON_START, BUTTON_UP};

    let player = emulator.player_state();
    let px = i32::from(player.x_int);
    let py = i32::from(player.y_int);
    let mut buttons = BUTTON_A;
    if (180..186).contains(&emulator.frames) {
        buttons |= BUTTON_START;
    }

    // Fight phase: ignore the incoming fire (the run is analysis-only) and
    // keep the guns on the core.
    let boss_slot = usize::from(boss::BOSS_SLOT);
    if emulator.bus.ram[0x030C + boss_slot] == boss::BOSS_KIND {
        let boss_y = i32::from(emulator.bus.ram[0x032C + boss_slot]);
        // A shot keeps the ship Y at spawn, adds $BFD0[0] = 8, flies for about
        // 21 frames and needs to land in the core's 2 px box ($BFD8[3] = 2).
        // The accepted line is core y - 7 or -6 (the X compare's borrow makes
        // the ROM accept dy 1..2). The core is scanned before its cover parts
        // ($BFDC counts slot 9 down), so passing through a pod still hits the
        // core on that line.
        let velocity = last_boss_y
            .map(|previous| (boss_y - previous).clamp(-2, 2))
            .unwrap_or(0);
        *last_boss_y = Some(boss_y);
        let lead = velocity * 21;
        let target = if stage == 1 {
            // Historical policy behind scripts/boss-script.txt: sweep 14 px
            // either side of core y - 8 every 16 frames.
            let phase = if (emulator.frames / 16) % 2 == 0 {
                14
            } else {
                -14
            };
            boss_y + lead - 8 + phase
        } else {
            // Sawtooth -16..15: crosses the exact hit line once per 32 frames,
            // so a ~20-frame shot cadence lands within a few sweeps even when
            // the lead misses the core's vertical turn at the stage's rank.
            boss_y + lead - 7 + (emulator.frames % 32) as i32 - 16
        }
        .clamp(24, 184);
        if target >= py + 2 && py < 184 {
            buttons |= BUTTON_DOWN;
        } else if target <= py - 2 && py > 24 {
            buttons |= BUTTON_UP;
        }
        return buttons;
    }

    // Nearest object inside the threat box, ignoring the ship and its trail.
    let mut threat: Option<(i32, i32)> = None;
    let mut nearest = i32::MAX;
    for object in emulator.objects() {
        if object.slot == 0 {
            continue;
        }
        let dx = i32::from(object.x_int) - px;
        let dy = i32::from(object.y_int) - py;
        // Options and the option trail sit on the ship.
        if dx.abs() <= 8 && dy.abs() <= 8 {
            continue;
        }
        if !(-40..=56).contains(&dx) || dy.abs() > 28 {
            continue;
        }
        let distance = dx * dx + dy * dy;
        if distance < nearest {
            nearest = distance;
            threat = Some((dx, dy));
        }
    }

    if let Some((dx, dy)) = threat {
        // Step away from the threat vertically; if that would clamp, go the
        // other way. Horizontal separation is used only for a point-blank hit.
        if dy >= 0 && py > 24 || py >= 184 {
            buttons |= BUTTON_UP;
        } else if py < 184 {
            buttons |= BUTTON_DOWN;
        }
        if dx.abs() <= 16 {
            buttons |= if dx >= 0 { BUTTON_LEFT } else { BUTTON_RIGHT };
        }
        return buttons;
    }

    let target = 96;
    if target >= py + 4 && py < 184 {
        buttons |= BUTTON_DOWN;
    } else if target <= py - 4 && py > 24 {
        buttons |= BUTTON_UP;
    }
    buttons
}

/// Run the stage unassisted and check that the HUD sprite-0 split completes.
///
/// The oracle raises the real hit from the rendered sprite/background overlap,
/// so this is a regression guard for the PPU model: if the hit stops arriving,
/// the CPU spins in `$9A96`/`$9A9D` and the stage scroll freezes. Any of those
/// outcomes exits with an error naming the observed counters.
pub(crate) fn verify_sprite0(rom: &Path, frames: u64) -> Result<()> {
    if frames == 0 {
        bail!("--frames must be at least 1");
    }
    let cart = load(rom)?;
    let mut emulator = Emulator::new(&cart)?;
    // Start at the historical title frame, held for six frames like `--press`.
    let plan = oracle::Plan {
        held: 0,
        schedule: vec![(180, gradius_nes::BUTTON_START, 6)],
    };
    let mut runner = oracle::Runner::new();
    let mut stuck_frames = 0u64;
    let mut max_scroll = 0u16;
    let mut max_scroll_frame = 0u64;
    let mut scroll_before = 0u16;
    let check_at = frames.saturating_sub(60);
    while emulator.frames < frames {
        runner.advance(&mut emulator, &plan, false);
        if oracle::WAIT_CLEAR.contains(&emulator.cpu.pc)
            || oracle::WAIT_SET.contains(&emulator.cpu.pc)
        {
            stuck_frames += 1;
        }
        let scroll = emulator.player_state().scroll;
        if scroll > max_scroll {
            max_scroll = scroll;
            max_scroll_frame = emulator.frames;
        }
        if emulator.frames == check_at {
            scroll_before = scroll;
        }
    }

    let scroll = emulator.player_state().scroll;
    let reads = emulator.bus.ppu.sprite_zero_status_reads;
    let wait_clear = emulator.bus.trace.hits[usize::from(0x9A96u16)];
    let wait_set = emulator.bus.trace.hits[usize::from(0x9A9Du16)];

    println!("verify-sprite0: {frames} frames, no assist");
    println!("  $2002 reads with status bit 6 set: {reads}");
    println!("  HUD wait polls: $9A96={wait_clear}, $9A9D={wait_set}");
    println!("  frames ending inside a wait loop: {stuck_frames}");
    println!("  scroll: max {max_scroll} at frame {max_scroll_frame}, final {scroll}");
    println!("  last 60 frames: {scroll_before} -> {scroll}");

    if reads == 0 {
        bail!("the PPU never raised sprite-0 status bit 6");
    }
    if wait_clear == 0 || wait_set == 0 {
        bail!("the HUD split never reached $9A96/$9A9D (clear={wait_clear}, set={wait_set})");
    }
    if max_scroll < 256 {
        bail!("the stage only scrolled {max_scroll} px; the HUD wait is stalling it");
    }
    if stuck_frames != 0 {
        bail!("{stuck_frames} frames ended with the CPU spinning in the HUD wait");
    }
    println!("ok: the HUD sprite-0 wait passes without assist");
    Ok(())
}

pub(crate) fn transition(
    rom: &Path,
    script: &Path,
    from: u64,
    to: u64,
    keep_alive: bool,
    out: Option<&Path>,
) -> Result<()> {
    let cart = load(rom)?;
    let source =
        fs::read_to_string(script).with_context(|| format!("reading {}", script.display()))?;
    let plan = transition_ledger::parse_script(&source)?;
    let mut emulator = Emulator::new(&cart)?;
    let mut runner = oracle::Runner::new();
    runner.keep_alive = keep_alive;
    runner.run_to(&mut emulator, from.saturating_sub(1), &plan, false);
    let mut text = format!(
        "; frames {from}..{to} (exclusive)\n\
         ; stage=$19 phase=$1B music=$1C scroll=$3E:$3F:$3D ship=$0360,$0320 \
         player=$0100 s39=$39 s3a=$3A objects=slot:$030C\n"
    );
    for _ in from..to {
        runner.advance(&mut emulator, &plan, false);
        text.push_str(&transition_ledger::line(emulator.frames, &emulator.bus.ram));
    }
    match out {
        Some(path) => {
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            println!(
                "wrote {} ({} frames)",
                path.display(),
                text.lines().count().saturating_sub(2)
            );
        }
        None => print!("{text}"),
    }
    warn_machine_state(&emulator);
    Ok(())
}
