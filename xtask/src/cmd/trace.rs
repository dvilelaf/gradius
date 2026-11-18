//! `trace`, `disasm` and `symbols`.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use gradius_nes::Emulator;

use crate::{disasm, oracle, symbols};

use super::shared::{input_plan, load, warn_machine_state};

#[allow(clippy::too_many_arguments)]
pub(crate) fn trace(
    rom: &Path,
    frames: u64,
    top: usize,
    symbols: Option<&Path>,
    ranges: Option<&Path>,
    log: usize,
    buttons: Option<&str>,
    presses: &[String],
    assist: bool,
    keep_alive: bool,
) -> Result<()> {
    let cart = load(rom)?;
    let (names, _) = symbols::load(symbols)?;
    let mut emulator = Emulator::new(&cart)?;
    for _ in 0..log {
        let pc = emulator.cpu.pc;
        let opcode = emulator.opcode_at(pc);
        let name = names
            .get(&pc)
            .map(|name| format!("  {name}"))
            .unwrap_or_default();
        println!(
            "${pc:04X}{name}  {opcode:02X}  A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X} CYC={}",
            emulator.cpu.a,
            emulator.cpu.x,
            emulator.cpu.y,
            emulator.cpu.p,
            emulator.cpu.sp,
            emulator.cpu.cycles
        );
        emulator.step_instruction();
        if emulator.cpu.jammed {
            println!("CPU halted at ${pc:04X}");
            break;
        }
    }
    if log > 0 {
        return Ok(());
    }
    let plan = input_plan(buttons, presses)?;
    let mut runner = oracle::Runner::new();
    runner.keep_alive = keep_alive;
    runner.run_to(&mut emulator, frames, &plan, assist);
    let trace = &emulator.bus.trace;
    println!("frames:        {frames}");
    println!("instructions:  {}", trace.instructions);
    println!("visited PCs:   {}", trace.visited_addresses());
    if let Some(opcode) = emulator.cpu.unknown_opcode {
        println!("unknown opcode: {opcode:02X}");
    }
    if assist {
        println!("sprite-0 assist pulses: {}", runner.assist_pulses);
    }

    let mut hot: Vec<(u16, u32)> = trace
        .hits
        .iter()
        .enumerate()
        .filter(|(_, hits)| **hits > 0)
        .map(|(pc, hits)| (pc as u16, *hits))
        .collect();
    hot.sort_by_key(|(_, hits)| std::cmp::Reverse(*hits));
    println!("\nhottest PCs:");
    for (pc, hits) in hot.iter().take(top) {
        let name = names
            .get(pc)
            .map(|name| format!("  {name}"))
            .unwrap_or_default();
        println!(
            "  ${pc:04X}{name}  opcode {:02X}  {hits} hits",
            trace.opcodes[usize::from(*pc)]
        );
    }

    if let Some(path) = ranges {
        let mut text = String::new();
        let mut start: Option<u16> = None;
        for pc in 0u32..=0x10000 {
            let executed = pc < 0x10000 && trace.was_executed(pc as u16);
            match (start, executed) {
                (None, true) => start = Some(pc as u16),
                (Some(from), false) => {
                    text.push_str(&format!(
                        "${from:04X}-${:04X}\n",
                        (pc as u16).wrapping_sub(1)
                    ));
                    start = None;
                }
                _ => {}
            }
        }
        fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        println!("\nranges written to {}", path.display());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn disassemble(
    rom: &Path,
    from: u16,
    to: u16,
    covered: bool,
    labels: bool,
    symbols: Option<&Path>,
    frames: Option<u64>,
    buttons: Option<&str>,
    presses: &[String],
    assist: bool,
    keep_alive: bool,
    out: Option<&Path>,
) -> Result<()> {
    if from > to {
        bail!("--from ${from:04X} is after --to ${to:04X}");
    }
    let cart = load(rom)?;
    let image = disasm::Image::new(&cart.prg);
    let vectors = image.vectors();
    let (names, symbol_source) = symbols::load(symbols)?;
    let mut header = String::new();
    header.push_str("; xtask disasm\n");
    header.push_str(&format!(
        "; PRG $8000-$FFFF ({} bytes), {} instructions in the static flow\n",
        cart.prg.len(),
        disasm::analyze(&image, vectors).instruction_count()
    ));
    if let Some(source) = &symbol_source {
        header.push_str(&format!(
            "; symbols: {} names from {}\n",
            names.len(),
            source.display()
        ));
    }

    let hits = if covered || frames.is_some() {
        let frame_count = frames.unwrap_or(300);
        let plan = input_plan(buttons, presses)?;
        let mut emulator = Emulator::new(&cart)?;
        let mut runner = oracle::Runner::new();
        runner.keep_alive = keep_alive;
        runner.run_to(&mut emulator, frame_count, &plan, assist);
        header.push_str(&format!(
            "; coverage: {} frames, {} instructions, {} distinct PCs{}\n",
            emulator.frames,
            emulator.bus.trace.instructions,
            emulator.bus.trace.visited_addresses(),
            if assist {
                format!(", {} sprite-0 assist pulses", runner.assist_pulses)
            } else {
                String::new()
            }
        ));
        let hits = emulator.bus.trace.hits.clone();
        warn_machine_state(&emulator);
        Some(hits)
    } else {
        None
    };

    let mut seeds: Vec<u16> = vectors.to_vec();
    if let Some(hits) = &hits {
        seeds.extend(
            hits.iter()
                .enumerate()
                .filter(|(_, hits)| **hits > 0)
                .map(|(pc, _)| pc as u16),
        );
    }
    let analysis = disasm::analyze_with(&image, seeds, hits.as_deref());
    let label_set = if labels {
        let mut set = analysis.labels();
        set.extend(vectors.iter().copied());
        set
    } else {
        std::collections::BTreeSet::new()
    };

    let listing_analysis = if covered { Some(&analysis) } else { None };
    header.push_str(&format!(
        "; vectors: nmi=L{:04X} reset=L{:04X} irq=L{:04X}\n",
        vectors[0], vectors[1], vectors[2]
    ));
    if covered && hits.is_some() {
        header.push_str("; * = executed; unexecuted bytes are shown as `; data`\n");
    }
    header.push('\n');

    let listing = disasm::render(
        &image,
        from,
        to,
        listing_analysis,
        &label_set,
        disasm::Render {
            hits: hits.as_deref(),
            labels,
            names: Some(&names),
        },
    );
    let text = header + &listing;

    match out {
        Some(path) => {
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            println!(
                "wrote {} (${from:04X}-${to:04X}, {} lines)",
                path.display(),
                text.lines().count()
            );
        }
        None => print!("{text}"),
    }
    Ok(())
}

/// Write the merged stable symbol file.
pub(crate) fn symbolize(merge: &Path, out: &Path) -> Result<()> {
    let export = symbols::read(merge)?;
    let existing = if out.exists() {
        fs::read_to_string(out).with_context(|| format!("reading {}", out.display()))?
    } else {
        String::new()
    };
    let merged = symbols::merge(&existing, &export)
        .with_context(|| format!("merging into {}", out.display()))?;
    fs::write(out, symbols::render(&merged))
        .with_context(|| format!("writing {}", out.display()))?;
    println!(
        "wrote {} ({} symbols from {})",
        out.display(),
        merged.len(),
        merge.display()
    );
    Ok(())
}
