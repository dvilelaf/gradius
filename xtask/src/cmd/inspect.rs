//! `inspect`, `tiles`, `spawn` and `decode-record`.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Result};
use gradius_cart::Cartridge;

use crate::{command_format, enemy, pngout};

use super::shared::load;

pub(crate) fn inspect(rom: &Path) -> Result<()> {
    let data = fs::read(rom)?;
    let cart = Cartridge::from_ines(&data)?;
    println!("file:        {}", rom.display());
    println!("bytes:       {}", data.len());
    println!("sha256:      {}", pngout::sha256(&data));
    print!("{}", cart.header);
    Ok(())
}

/// Print the fields `$A460` writes for one five-byte stream record.
pub(crate) fn command_format_report(stage: u8, bytes: &[u8]) -> Result<()> {
    let bytes: [u8; 5] = bytes
        .try_into()
        .map_err(|_| anyhow!("expected exactly five record bytes"))?;
    let spawn = command_format::decode(usize::from(stage), bytes);
    println!("record $63-$67 : {:02X?}", bytes);
    println!("subtype $010C  : ${:02X}  ($64 - $70)", spawn.subtype);
    println!("kind    $030C  : ${:02X}", spawn.kind);
    println!("x       $036C  : ${:02X}", spawn.x);
    println!("y       $032C  : ${:02X}", spawn.y);
    if stage == 2 {
        println!("$03BC          : ${:02X}  ($66)", spawn.byte_bc);
        println!(
            "$03EC          : ${:02X}  ($67; $03BC:$03EC is a 16-bit value)",
            spawn.byte_ec
        );
    } else {
        for (index, nibble) in spawn.groups.iter().copied().enumerate() {
            if nibble != 0 {
                println!(
                    "attachment {}   : $0600 = parent slot, $0601 = ${:02X}",
                    index + 1,
                    nibble - 1
                );
            }
        }
        println!(
            "$016C          : ${:02X}  attachment records",
            spawn.records
        );
        println!("$0460          : $01  size class");
    }
    Ok(())
}

/// Decode the `$A5FC`/`$A58C`/`$A5B6` formation for each stream type.
///
/// These are decoded numbers, matching what `$A3DE` would write into the
/// object table; no cartridge bytes are printed.
pub(crate) fn spawn(rom: &Path, kinds: &[u8]) -> Result<()> {
    let cart = load(rom)?;
    let prg = &cart.prg;
    let read = |addr: u16| -> Result<u8> {
        let offset = usize::from(addr.wrapping_sub(0x8000));
        prg.get(offset)
            .copied()
            .ok_or_else(|| anyhow!("${addr:04X} is outside the PRG window"))
    };
    println!("; spawn formations from $A5FC/$A58C/$A5B6");
    for &kind in kinds {
        if kind < 0x80 {
            println!("${kind:02X}: not an object type (below $80 is a terrain piece)");
            continue;
        }
        // Two `ASL`s in `$A367` drop the two high bits, i.e. index = type & $3F.
        let index = usize::from(kind & 0x3F);
        let def_addr = 0xA5FC + 4 * index as u16;
        let def: Vec<u8> = (0..4)
            .map(|offset| read(def_addr + offset))
            .collect::<Result<_>>()?;
        let size_index = def[2];
        let move_index = def[3];
        let size_addr = 0xA58C + 2 * u16::from(size_index);
        let size: Vec<u8> = (0..2)
            .map(|offset| read(size_addr + offset))
            .collect::<Result<_>>()?;
        let move_addr = 0xA5B6 + 3 * u16::from(move_index);
        let movement: Vec<u8> = (0..3)
            .map(|offset| read(move_addr + offset))
            .collect::<Result<_>>()?;
        let formation = enemy::formation(&def, &size, &movement)
            .ok_or_else(|| anyhow!("malformed table entry for type ${kind:02X}"))?;
        println!(
            "${kind:02X}: kind ${:02X} subtype ${:02X} count {} x {} ys {:?} gap {} frames step {} motion ${:02X} phase {}",
            formation.kind,
            formation.subtype,
            formation.count,
            formation.x,
            formation.ys,
            formation.gap,
            movement[1],
            formation.motion,
            formation.phase,
        );
        println!(
            "      def ${def_addr:04X} {:02X?}  size ${size_addr:04X} {:02X?}  move ${move_addr:04X} {:02X?}",
            def, size, movement
        );
    }
    Ok(())
}

pub(crate) fn tiles(rom: &Path, out: &Path, scale: u32) -> Result<()> {
    let cart = load(rom)?;
    let banks = cart.chr_banks();
    let tile_px = 8 * scale;
    let margin = 4;
    let columns = 16u32;
    let rows_per_bank = 16u32;
    let width = columns * tile_px + margin * 2;
    let height = banks.len() as u32 * (rows_per_bank * tile_px + margin) + margin;
    let mut rgb = vec![0x10u8; (width * height * 3) as usize];

    for (bank_index, bank) in banks.iter().enumerate() {
        let bank_y = margin + bank_index as u32 * (rows_per_bank * tile_px + margin);
        for (tile_index, tile) in bank.iter().enumerate() {
            let tx = tile_index as u32 % columns;
            let ty = tile_index as u32 / columns;
            let origin_x = margin + tx * tile_px;
            let origin_y = bank_y + ty * tile_px;
            for y in 0..tile_px {
                for x in 0..tile_px {
                    let pixel = tile.pixels[((y / scale) * 8 + (x / scale)) as usize];
                    let shade = [0x00u8, 0x55, 0xAA, 0xFF][usize::from(pixel)];
                    let offset = ((origin_y + y) * width + origin_x + x) * 3;
                    rgb[offset as usize..offset as usize + 3]
                        .copy_from_slice(&[shade, shade, shade]);
                }
            }
        }
    }
    pngout::write_rgb(out, width, height, &rgb)?;
    println!(
        "wrote {} ({} x {}, {} banks)",
        out.display(),
        width,
        height,
        banks.len()
    );
    Ok(())
}
