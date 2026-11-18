//! Running one scenario against the cartridge and the port.

use std::path::Path;

use crate::compare::{compare, CompareConfig};
use crate::dump::{keep_alive_poke, oracle_snapshot, Snapshot};
use crate::pixel::{write_png, PixelDiff, PixelSummary};
use crate::script::{parse_script, Script};
use crate::{core_buttons, nes_button_bits};
use gradius_assets::Pack;
use gradius_cart::Cartridge;
use gradius_core::Game;
use gradius_nes::Emulator;

use super::checks::{
    boss_checks, eruption_checks, handoff_checks, menu_checks, menu_start_checks, pause_checks,
    pause_code_checks, pause_transition_checks, power_checks,
};
use super::*;

fn read_script(scenario: &Scenario) -> Result<Script, String> {
    let path = scenario.script_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse_script(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Run one scenario. Writes PNGs to `png_dir` when it is set.
pub fn run(
    rom: &Path,
    pack: &Path,
    scenario: &Scenario,
    png_dir: Option<&Path>,
) -> Result<ScenarioResult, String> {
    let script = read_script(scenario)?;
    let data = std::fs::read(rom)
        .map_err(|error| format!("cannot read ROM {}: {error}", rom.display()))?;
    let cart = Cartridge::from_ines(&data)
        .map_err(|error| format!("cannot parse ROM {}: {error}", rom.display()))?;
    let mut emulator =
        Emulator::new(&cart).map_err(|error| format!("cannot start oracle: {error}"))?;
    emulator.set_audio_enabled(false);
    let pack = Pack::load(pack)
        .map_err(|error| format!("cannot load asset pack {}: {error}", pack.display()))?;
    let assets = pack.into_assets();
    let mut game = Game::new(assets);
    if scenario.native_keep_alive {
        game.dev_keep_alive(true);
    }
    if let Some(dir) = png_dir {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }

    let pixel_window = scenario.pixel_window();
    let mut pixels = PixelSummary::default();
    if let Some(window) = pixel_window {
        pixels.set_window(window);
    }

    let mut oracle_lines: Vec<Snapshot> = Vec::with_capacity(scenario.frames as usize);
    // RAM for the whole run: the checks need per-frame `$42`/`$40`/`$5F`.
    let mut ram = Vec::with_capacity(scenario.frames as usize);
    let mut native_power = Vec::with_capacity(scenario.frames as usize);
    let mut native_lines: Vec<Snapshot> = Vec::with_capacity(scenario.frames as usize);

    let power_plan = match scenario.check {
        Check::Power(plan) => Some(plan),
        _ => None,
    };

    for frame in 0..scenario.frames {
        let mut pads = script.pads(frame);
        if let Some(plan) = power_plan {
            for tap in plan.taps {
                if frame >= *tap && frame - tap < 2 {
                    pads.p1.b = true;
                }
            }
            if let Some(hold) = plan.hold_from {
                if frame >= hold {
                    pads.p1.b = true;
                }
            }
        }
        emulator.set_buttons_p1(nes_button_bits(pads.p1));
        emulator.set_buttons_p2(nes_button_bits(pads.p2));
        emulator.run_frame();
        if scenario.keep_alive {
            keep_alive_poke(&mut emulator);
        }
        let oracle = oracle_snapshot(&emulator);
        // Both pads reach the port: 1P merges them (`$8203`), 2P routes the
        // current player's pad (`$05,X`/`$07,X`).
        game.step_players(core_buttons(pads.p1), core_buttons(pads.p2));
        let native = Snapshot::parse(&game.dump_state())
            .map_err(|error| format!("cannot parse native dump: {error}"))?;
        ram.push(Ram::read(&emulator));
        native_power.push(NativePower::read(game.power()));

        if let Some(window) = pixel_window {
            if window.contains(frame) && frame % scenario.pixel_every == 0 {
                pixels.push(
                    frame,
                    oracle.state.clone(),
                    PixelDiff::compare(emulator.framebuffer(), game.framebuffer().as_rgba()),
                );
            }
        }
        if scenario.png_frames.contains(&frame) {
            if let Some(dir) = png_dir {
                write_png(
                    &dir.join(format!("oracle-{frame:04}.png")),
                    emulator.framebuffer(),
                )?;
                write_png(
                    &dir.join(format!("native-{frame:04}.png")),
                    game.framebuffer().as_rgba(),
                )?;
            }
        }

        oracle_lines.push(oracle);
        native_lines.push(native);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle_lines.iter().zip(&native_lines).collect();
    let report = compare(
        pairs,
        CompareConfig {
            tolerance_px: 1,
            window: scenario.window,
            compare_objects: scenario.compare_objects,
        },
    );

    let mut checks = Vec::new();
    let mut notes = Vec::new();
    match scenario.check {
        Check::Menu {
            selection,
            start_lock,
        } => menu_checks(
            selection,
            start_lock,
            &ram,
            &oracle_lines,
            &native_lines,
            &mut checks,
            &mut notes,
        ),
        Check::MenuStart {
            start_frame,
            start_lock,
        } => menu_start_checks(
            start_frame,
            start_lock,
            &ram,
            &oracle_lines,
            &mut checks,
            &mut notes,
        ),
        Check::Power(plan) => power_checks(
            plan,
            scenario.expected,
            &ram,
            &native_power,
            &oracle_lines,
            &mut checks,
            &mut notes,
        ),
        Check::Eruption => eruption_checks(&oracle_lines, &ram, &mut checks, &mut notes),
        Check::BossGate => boss_checks(&oracle_lines, &ram, &mut checks, &mut notes),
        Check::Handoff => handoff_checks(&oracle_lines, &ram, &report, &mut checks, &mut notes),
        Check::Pause => pause_checks(&oracle_lines, &ram, &mut checks),
        Check::PauseCode => pause_code_checks(&ram, &mut checks),
        Check::PauseTransition => pause_transition_checks(&ram, &mut checks),
    }

    let check_passed = checks.iter().all(|check| !check.hard || check.pass);
    let passed = check_passed && scenario.expected.accepts(&report);
    if let Expected::Known {
        first_frame,
        max_divergences,
    } = scenario.expected
    {
        if report.within_tolerance() {
            notes.push(
                "known mismatch no longer reproduces; consider making this a clean gate".into(),
            );
        } else if !scenario.expected.accepts(&report) {
            let first = report.first().map_or(0, |divergence| divergence.frame);
            notes.push(format!(
                "known budget exceeded: first divergence f{first} (accepted not before {first_frame}), {} divergences (accepted at most {max_divergences})",
                report.divergences.len()
            ));
        }
    }

    Ok(ScenarioResult {
        name: scenario.name,
        expected: scenario.expected,
        report,
        pixels,
        checks,
        notes,
        passed,
        oracle_lines,
        native_lines,
    })
}
