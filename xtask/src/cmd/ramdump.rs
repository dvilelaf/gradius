//! `ramdump` and the `--poke` parser.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use gradius_nes::Emulator;

use crate::{oracle, ram};

use super::shared::{input_plan, load, parse_frames, warn_machine_state};

#[allow(clippy::too_many_arguments)]
pub(crate) fn ramdump(
    rom: &Path,
    frame: u64,
    buttons: Option<&str>,
    presses: &[String],
    range: Option<&str>,
    watch: Option<&str>,
    top: usize,
    assist: bool,
    keep_alive: bool,
    shield: bool,
    out: Option<&Path>,
) -> Result<()> {
    let cart = load(rom)?;
    let (start, end) = match range {
        Some(spec) => ram::parse_range(spec)
            .ok_or_else(|| anyhow!("expected --range START:END in hex, got `{spec}`"))?,
        None => (0, 0x800),
    };
    if end > 0x800 {
        bail!(
            "CPU RAM is ${:04X} bytes, but --range ends at ${end:04X}",
            0x800
        );
    }
    let plan = input_plan(buttons, presses)?;
    let mut emulator = Emulator::new(&cart)?;
    let mut runner = oracle::Runner::new();
    runner.keep_alive = keep_alive;
    runner.shield = shield;
    let mut text = String::new();

    match watch {
        Some(window) => {
            let (from, to) = parse_frames(window)?;
            runner.run_to(&mut emulator, from, &plan, assist);
            let mut old = emulator.bus.ram.to_vec();
            let mut counts: BTreeMap<u16, u32> = BTreeMap::new();
            text.push_str(&format!(
                "; RAM watch frames {from}..{to}, ${start:04X}-${:04X}\n",
                end.saturating_sub(1)
            ));
            for _ in from..to {
                runner.advance(&mut emulator, &plan, assist);
                for (address, before, after) in ram::diff(&old, &emulator.bus.ram, start, end) {
                    text.push_str(&ram::diff_line(emulator.frames, address, before, after));
                    *counts.entry(address).or_default() += 1;
                }
                old.copy_from_slice(&emulator.bus.ram);
            }
            let mut ranked: Vec<(u16, u32)> = counts.into_iter().collect();
            ranked.sort_by_key(|(address, count)| (std::cmp::Reverse(*count), *address));
            text.push_str(&format!(
                "; {} distinct addresses changed in {} frames\n",
                ranked.len(),
                to - from
            ));
            for (address, count) in ranked.iter().take(top) {
                text.push_str(&format!("; ${address:04X} changed {count} times\n"));
            }
        }
        None => {
            runner.run_to(&mut emulator, frame, &plan, assist);
            text.push_str(&format!(
                "; frame {}, CPU RAM ${start:04X}-${:04X}\n",
                emulator.frames,
                end.saturating_sub(1)
            ));
            text.push_str(&ram::hexdump(&emulator.bus.ram, start, end));
        }
    }

    text.push_str(&format!(
        "; sprite-0 poll hits: {}{}\n",
        emulator.bus.trace.hits[0x9A96],
        if assist {
            format!(", {} assist pulses", runner.assist_pulses)
        } else {
            String::new()
        }
    ));
    if runner.keep_alive {
        text.push_str(&format!(
            "; keep-alive lethal hits: {}\n",
            runner.lethal_hits.len()
        ));
        for hit in runner.lethal_hits.iter().take(8) {
            text.push_str(&format!(
                "; f{} scroll=${:04X} ship=({},{})\n",
                hit.frame, hit.scroll, hit.x, hit.y
            ));
        }
    }
    match out {
        Some(path) => {
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            println!("wrote {} ({} lines)", path.display(), text.lines().count());
        }
        None => print!("{text}"),
    }
    warn_machine_state(&emulator);
    Ok(())
}

/// Parse a `FRAME:ADDR:VALUE` poke spec, both numbers hexadecimal.
pub(crate) fn parse_poke(spec: &str) -> Result<(u64, u16, u8)> {
    let parts: Vec<&str> = spec.split(':').collect();
    let [frame, address, value] = parts.as_slice() else {
        bail!("expected FRAME:ADDR:VALUE, got `{spec}`");
    };
    let frame: u64 = frame
        .parse()
        .with_context(|| format!("bad frame in `{spec}`"))?;
    let address =
        ram::parse_hex16(address).ok_or_else(|| anyhow!("expected a hex address in `{spec}`"))?;
    let value =
        ram::parse_hex16(value).ok_or_else(|| anyhow!("expected a hex value in `{spec}`"))?;
    let value = u8::try_from(value).map_err(|_| anyhow!("`{value}` does not fit in one byte"))?;
    Ok((frame, address, value))
}

/// Parse, sort and validate the `--poke` list.
pub(crate) fn parse_pokes(specs: &[String]) -> Result<Vec<(u64, u16, u8)>> {
    let mut pokes = specs
        .iter()
        .map(|spec| parse_poke(spec))
        .collect::<Result<Vec<_>>>()?;
    pokes.sort_by_key(|(frame, _, _)| *frame);
    Ok(pokes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poke_specs_accept_hex_and_sort_by_frame() {
        let pokes = parse_pokes(&[
            "902:0309:01".to_owned(),
            "900:0109:06".to_owned(),
            "900:$0329:$60".to_owned(),
        ])
        .unwrap();
        assert_eq!(
            pokes,
            vec![
                (900, 0x0109, 0x06),
                (900, 0x0329, 0x60),
                (902, 0x0309, 0x01)
            ]
        );
    }

    #[test]
    fn poke_specs_reject_malformed_or_oversized_values() {
        assert!(parse_poke("900:0309").is_err());
        assert!(parse_poke("z:0309:01").is_err());
        assert!(parse_poke("900:0309:1ff").is_err());
    }
}
