//! `probe` and `nametable`.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use gradius_nes::Emulator;

use crate::probe as probe_mod;
use crate::{oracle, pngout, ram};

use super::ramdump::parse_pokes;
use super::shared::{input_plan, load, print_lethal_hits, warn_machine_state};

#[allow(clippy::too_many_arguments)]
pub(crate) fn probe(
    rom: &Path,
    frame: u64,
    buttons: Option<&str>,
    presses: &[String],
    oam_path: Option<&Path>,
    png_path: Option<&Path>,
    nametable_path: Option<&Path>,
    ram_path: Option<&Path>,
    json_path: Option<&Path>,
    sprite_count: usize,
    keep_alive: bool,
    shield: bool,
    pokes: &[String],
) -> Result<()> {
    let cart = load(rom)?;
    let mut emulator = Emulator::new(&cart)?;
    let plan = input_plan(buttons, presses)?;
    let pokes = parse_pokes(pokes)?;
    let mut runner = oracle::Runner::new();
    runner.keep_alive = keep_alive;
    runner.shield = shield;
    let mut next_poke = 0;
    while emulator.frames < frame {
        while let Some((at, address, value)) = pokes.get(next_poke) {
            if *at > emulator.frames {
                break;
            }
            emulator.bus.ram[usize::from(*address) & 0x07FF] = *value;
            next_poke += 1;
        }
        runner.advance(&mut emulator, &plan, false);
    }

    let ppu = &emulator.bus.ppu;
    let nametables = ppu.vram[..0x800].to_vec();
    let report = probe_mod::Report {
        frame,
        ctrl: ppu.ctrl,
        mask: ppu.mask,
        chr_bank: ppu.chr_bank,
        palette: ppu.palette,
        oam: ppu.oam,
        nametable_fnv: probe_mod::fnv1a(&nametables),
        framebuffer_fnv: probe_mod::fnv1a(emulator.framebuffer()),
        exec: probe_mod::ExecSummary::from_hits(
            &emulator.bus.trace.hits,
            emulator.bus.trace.instructions,
            16,
        ),
        nametables,
    };

    // `--sprites` only changes how many rows the text table shows.
    print!("{}", report.to_text(sprite_count));

    if let Some(path) = oam_path {
        fs::write(path, report.oam).with_context(|| format!("writing {}", path.display()))?;
        println!("\nraw OAM written to {}", path.display());
    }
    if let Some(path) = png_path {
        pngout::write_rgb(
            path,
            gradius_nes::SCREEN_WIDTH as u32,
            gradius_nes::SCREEN_HEIGHT as u32,
            &emulator.rgb(),
        )?;
        println!("framebuffer written to {}", path.display());
    }
    if let Some(path) = nametable_path {
        fs::write(path, &report.nametables)
            .with_context(|| format!("writing {}", path.display()))?;
        println!("nametables written to {}", path.display());
    }
    if let Some(path) = ram_path {
        fs::write(path, emulator.bus.ram).with_context(|| format!("writing {}", path.display()))?;
        println!("CPU RAM written to {}", path.display());
    }
    if let Some(path) = json_path {
        fs::write(path, report.to_json()).with_context(|| format!("writing {}", path.display()))?;
        println!("JSON written to {}", path.display());
    }
    print_lethal_hits(&runner);
    warn_machine_state(&emulator);
    Ok(())
}

/// Log nametable VRAM writes for a frame window.
#[allow(clippy::too_many_arguments)]
pub(crate) fn nametable_writes(
    rom: &Path,
    from: u64,
    to: u64,
    presses: &[String],
    keep_alive: bool,
    pokes: &[String],
    range: &str,
    out: Option<&Path>,
) -> Result<()> {
    if to <= from {
        bail!("--to must be greater than --from");
    }
    let (start, end) = ram::parse_range(range)
        .ok_or_else(|| anyhow!("expected a PPU address window LO:HI, got `{range}`"))?;
    let cart = load(rom)?;
    let mut emulator = Emulator::new(&cart)?;
    emulator.set_capture_vram(true);
    let plan = input_plan(None, presses)?;
    let pokes = parse_pokes(pokes)?;
    let mut runner = oracle::Runner::new();
    runner.keep_alive = keep_alive;
    let mut next_poke = 0;
    let mut log = String::new();
    while emulator.frames < to {
        while let Some((at, address, value)) = pokes.get(next_poke) {
            if *at > emulator.frames {
                break;
            }
            emulator.bus.ram[usize::from(*address) & 0x07FF] = *value;
            next_poke += 1;
        }
        runner.advance(&mut emulator, &plan, false);
        let writes: Vec<gradius_nes::VramWrite> = emulator.vram_writes().to_vec();
        emulator.bus.ppu.clear_vram_writes();
        if emulator.frames < from {
            continue;
        }
        for write in writes {
            let addr = usize::from(write.addr);
            if (start..=end).contains(&addr) {
                log.push_str(&format!(
                    "frame {:5} ${:04X} <- ${:02X}\n",
                    write.frame, write.addr, write.value
                ));
            }
        }
    }
    if let Some(path) = out {
        fs::write(path, &log).with_context(|| format!("writing {}", path.display()))?;
        println!("nametable writes written to {}", path.display());
    } else {
        print!("{log}");
    }
    Ok(())
}
