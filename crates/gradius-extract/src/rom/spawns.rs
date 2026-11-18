//! `spawns` family of `CHUNK_ROM`: the `$A7CA` stage scripts and the object,
//! terrain and formation tables read straight from the PRG.

use gradius_cart::Cartridge;
use gradius_core::rom::spawns::{Record, COLUMN_SLOTS, SECTION_COUNT};
use gradius_core::rom::Spawns;

use crate::sections;
use crate::stage2::{read, word};

/// `$A7CA`: eight section sub-table pointers.
const SECTION_POINTERS: u16 = 0xA7CA;
/// `$A5FC`: 64 four-byte enemy definitions.
const OBJECT_DEFS: u16 = 0xA5FC;
/// `$A65C`: 128 three-byte terrain definitions.
const TERRAIN_DEFS: u16 = 0xA65C;
/// `$A58C`: 64 two-byte formation descriptors.
const FORMATION_SUBTYPES: u16 = 0xA58C;
/// `$A5B6`: 22 three-byte formation parameters.
const FORMATION_PARAMS: u16 = 0xA5B6;
/// `$ADBB`: 36 enemy animation tiles.
const ANIMATION_FRAMES: u16 = 0xADBB;
/// `$C4F0`: 16 stage-1 eruption velocities, three bytes each.
const ERUPTION_VELOCITY: u16 = 0xC4F0;
/// `$AE6B`: six pointers to zero-terminated debris sprite streams.
const DEBRIS_PATHS: u16 = 0xAE6B;
const DEBRIS_COUNT: u16 = 6;

fn rows<const N: usize>(prg: &[u8], base: u16, count: u16) -> Result<Vec<[u8; N]>, String> {
    (0..count)
        .map(|row| {
            let mut bytes = [0u8; N];
            for (offset, byte) in bytes.iter_mut().enumerate() {
                *byte =
                    read(prg, base + row * N as u16 + offset as u16).map_err(|e| e.to_string())?;
            }
            Ok(bytes)
        })
        .collect()
}

fn debris_path(prg: &[u8], pointer: u16) -> Result<Vec<u8>, String> {
    let mut path = Vec::new();
    for offset in 0..0x40 {
        let byte = read(prg, pointer + offset).map_err(|e| e.to_string())?;
        path.push(byte);
        if byte == 0 {
            return Ok(path);
        }
    }
    Err(format!("debris stream ${pointer:04X} has no terminator"))
}

pub(crate) fn extract(cart: &Cartridge) -> Result<Spawns, String> {
    let prg = &cart.prg[..];
    let map = |e: anyhow::Error| e.to_string();
    let mut spawns = Spawns::default();

    for (index, pointer) in spawns.section_pointers.iter_mut().enumerate() {
        *pointer = word(prg, SECTION_POINTERS + 2 * index as u16).map_err(map)?;
    }
    for section in 0..SECTION_COUNT {
        let start = spawns.section_pointers[section];
        let columns = usize::from(spawns.section_pointers[section + 1].wrapping_sub(start)) / 2;
        if columns == 0 || columns > COLUMN_SLOTS {
            return Err(format!("section {section} has {columns} column pointers"));
        }
        for column in 0..columns {
            let pointer = word(prg, start + 2 * column as u16).map_err(map)?;
            spawns.column_pointers[section][column] = pointer;
            spawns.sections[section][column] = sections::decode_records(prg, pointer)
                .map_err(map)?
                .into_iter()
                .map(|record| match record {
                    sections::Record::Spawn { delay, kind } => Record::Spawn { delay, kind },
                    sections::Record::Command(bytes) => Record::Command(bytes),
                })
                .collect();
        }
    }

    spawns.object_defs = rows(prg, OBJECT_DEFS, 64)?;
    spawns.terrain_defs = rows(prg, TERRAIN_DEFS, 128)?;
    spawns.formation_subtypes = rows(prg, FORMATION_SUBTYPES, 64)?;
    spawns.formation_params = rows(prg, FORMATION_PARAMS, 22)?;
    spawns.animation_frames = rows::<1>(prg, ANIMATION_FRAMES, 36)?
        .into_iter()
        .map(|[tile]| tile)
        .collect();
    spawns.eruption_velocity = rows::<3>(prg, ERUPTION_VELOCITY, 16)?
        .into_iter()
        .map(|[x, y, accel]| (x as i8, y as i8, accel))
        .collect();
    for index in 0..DEBRIS_COUNT {
        let pointer = word(prg, DEBRIS_PATHS + 2 * index).map_err(map)?;
        spawns.debris_paths.push(debris_path(prg, pointer)?);
    }
    Ok(spawns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_is_deterministic() {
        let Some(rom) = std::env::var_os("GRADIUS_ROM") else {
            return;
        };
        let cart = crate::load_cart(&std::fs::read(rom).unwrap()).unwrap();
        let first = extract(&cart).unwrap().encode();
        assert_eq!(first, extract(&cart).unwrap().encode());
        assert!(!first.is_empty());
    }
}
