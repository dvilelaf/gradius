//! Helpers shared by several subcommands.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use gradius_cart::Cartridge;
use gradius_nes::Emulator;

use crate::{oracle, ram};

use crate::parse_buttons;

pub(crate) fn load(rom: &Path) -> Result<Cartridge> {
    let data = fs::read(rom).with_context(|| format!("reading {}", rom.display()))?;
    let cart = Cartridge::from_ines(&data)?;
    if cart.header.mapper != 3 {
        bail!(
            "{} is mapper {}, but this project targets mapper 3 (CNROM)",
            rom.display(),
            cart.header.mapper
        );
    }
    cart.cpu_window()?;
    Ok(cart)
}

/// Report hits that `--keep-alive` masked. With `$46` forced to 5 every frame
/// the shield suppresses enemy, bullet and option contact; a hit here is the
/// `$C2B6` background-map probe, which bypasses the shield.
pub(crate) fn print_lethal_hits(runner: &oracle::Runner) {
    if !runner.keep_alive {
        return;
    }
    println!("keep-alive lethal hits: {}", runner.lethal_hits.len());
    for hit in runner.lethal_hits.iter().take(8) {
        println!(
            "  f{} scroll=${:04X} ship=({},{})",
            hit.frame, hit.scroll, hit.x, hit.y
        );
    }
}

pub(crate) fn warn_machine_state(emulator: &Emulator) {
    if let Some(opcode) = emulator.cpu.unknown_opcode {
        println!("warning: unimplemented opcode {opcode:02X} executed");
    }
    if emulator.cpu.jammed {
        println!(
            "warning: CPU halted (JAM) at ${:04X} on frame {}",
            emulator.cpu.pc, emulator.frames
        );
    }
}

/// Parse `--buttons` and `--press` into a [`oracle::Plan`].
///
/// A press is `FRAME:buttons` (held for six frames, the historical default) or
/// `FRAME:buttons:duration` for a longer hold, e.g. `900:right:60`.
pub(crate) fn input_plan(buttons: Option<&str>, presses: &[String]) -> Result<oracle::Plan> {
    let held = buttons.map(parse_buttons).transpose()?.unwrap_or(0);
    let mut schedule = Vec::new();
    for press in presses {
        let parts: Vec<&str> = press.split(':').collect();
        let (at, spec, duration) = match parts.as_slice() {
            [at, spec] => (*at, *spec, 6),
            [at, spec, duration] => (
                *at,
                *spec,
                duration
                    .parse()
                    .with_context(|| format!("bad duration in `{press}`"))?,
            ),
            _ => bail!("expected FRAME:buttons[:frames], got `{press}`"),
        };
        let at: u64 = at
            .parse()
            .with_context(|| format!("bad frame in `{press}`"))?;
        schedule.push((at, parse_buttons(spec)?, duration));
    }
    schedule.sort_by_key(|(at, _, _)| *at);
    Ok(oracle::Plan { held, schedule })
}

/// Parse a hex address, with or without a `$`/`0x` prefix.
pub(crate) fn parse_addr(text: &str) -> Result<u16> {
    ram::parse_hex16(text).ok_or_else(|| anyhow!("expected a hex address, got `{text}`"))
}

/// Parse one hex byte, with or without a `$`/`0x` prefix.
pub(crate) fn parse_kind(text: &str) -> Result<u8> {
    let value =
        ram::parse_hex16(text).ok_or_else(|| anyhow!("expected a hex byte, got `{text}`"))?;
    u8::try_from(value).map_err(|_| anyhow!("`{text}` does not fit in one byte"))
}

/// Parse `A:B` as two frames, for `--watch`.
pub(crate) fn parse_frames(spec: &str) -> Result<(u64, u64)> {
    let (from, to) = spec
        .split_once(':')
        .ok_or_else(|| anyhow!("expected FROM:TO frames, got `{spec}`"))?;
    let from: u64 = from
        .parse()
        .with_context(|| format!("bad frame in `{spec}`"))?;
    let to: u64 = to
        .parse()
        .with_context(|| format!("bad frame in `{spec}`"))?;
    if to < from {
        bail!("watch window ends before it starts: `{spec}`");
    }
    Ok((from, to))
}
