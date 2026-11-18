//! `tables` family of `CHUNK_ROM`: the byte copies behind the stage-1 formation
//! rows, the explosion streams and the per-stage music and script tables.

use gradius_cart::Cartridge;
use gradius_core::rom::tables::{ExplosionGroup, SpawnRow, EXPLOSION_STREAMS, STAGE_COUNT};
use gradius_core::rom::Tables;
use gradius_nes::{Emulator, BUTTON_START};

use crate::oracle::{Plan, Runner};
use crate::stage2::{read, word};

/// `$A5FC`: four-byte object definitions; type `$80 + i` uses row `i`.
const OBJECT_DEFS: u16 = 0xA5FC;
/// `$A58C`: two-byte size entries, one per type.
const SIZE_ENTRIES: u16 = 0xA58C;
/// Movement entries of types `$80`-`$84` (`stage1_enemies.rs`, `STAGE1_SPAWNS`).
const MOVEMENT: [u16; 5] = [0xA5B6, 0xA5B6, 0xA5C2, 0xA5BC, 0xA5BF];
/// `$C88D`: four stream pointers, selected by `type & 3`.
const EXPLOSION_POINTERS: u16 = 0xC88D;
/// `$C900`: the code after the last explosion stream (the kind `$16` handler).
const EXPLOSION_END: u16 = 0xC900;
/// `$8344`: each stage's second music theme.
const MUSIC: u16 = 0x8344;
/// `$8354`: each stage's music boundary.
const MUSIC_BOUNDARY: u16 = 0x8354;
/// `$C433`: each stage's script root.
const SCRIPT_ROOTS: u16 = 0xC433;
/// `$8BF7`: the stage-5 cannon tip tiles, indexed by `($0615 >> 2) & $0F`.
const CANNON_TIP: u16 = 0x8BF7;
/// `$8C07`: the tip attributes, indexed by `($0615 >> 4) & $03`.
const CANNON_TIP_ATTR: u16 = 0x8C07;
/// `$C520`: the stage-1 eruption nibble script.
const ERUPTION_SCRIPT: u16 = 0xC520;
/// `$C587`: the stage-2 hazard nibble script.
const HAZARD_SCRIPT: u16 = 0xC587;
/// `$C74C`: the stage-6 beam nibble script.
const BEAM_SCRIPT: u16 = 0xC74C;
/// `$C567`: X/Y pairs for stage-2 hazard kind `$0B`.
const HAZARD_POSITIONS: u16 = 0xC567;
/// `$BE68`: destruction sound id by kind low 7 bits.
const DESTROY_SFX: u16 = 0xBE68;
/// `$9BCE`: ship start positions, high nibble Y and low nibble X >> 4.
const SHIP_START: u16 = 0x9BCE;
/// `$9CB1`: the demo input records, button byte then tick count.
const DEMO_INPUT: u16 = 0x9CB1;
/// `$C6C8`: the stage-3 script's `$68`-indexed Y positions.
const STAGE3_SCRIPT_Y: u16 = 0xC6C8;
/// `$CD5F`: cannon part X step by `$0611 & $1F`.
const CANNON_STEP_X: u16 = 0xCD5F;
/// `$CD7F`: cannon part Y step by `$0611 & $1F`.
const CANNON_STEP_Y: u16 = 0xCD7F;
/// `$CA23`: stage-3 turret art, base groups then alternates.
const TURRET_GROUPS: u16 = 0xCA23;
/// `$8653`: the `$85ED` text-record pointer table, indexed by record id.
const TEXT_RECORDS: u16 = 0x8653;
/// Record 6 (`$87DD`): the title's `$3F00` background and `$3F10` sprite rows.
const TITLE_PALETTES: u8 = 6;
/// Record 7 (`$879D`): the stage's `$3F10` sprite palettes.
const STAGE_SPRITE_PALETTES: u8 = 7;
/// Record 8 (`$87B0`): background palettes 2-3, written after `$877F`.
const BACKGROUND_TAIL: u8 = 8;
/// Record 14 (`$87D4`): the ending's background palettes 2-3.
const ENDING_BACKGROUND_TAIL: u8 = 14;
/// Record 16 (`$877F`): `$3F00`, background palettes 0-1.
const STAGE_BACKGROUND_PALETTES: u8 = 16;
/// Record 33 (`$878A`): the ending's `$3F10` sprite palettes.
const ENDING_SPRITE_PALETTES: u8 = 33;
/// Frame to read the HUD rows: stage 1 is playing and the score is still zero.
const HUD_FRAME: u64 = 400;
/// `$8450`/`$8479`: the meter and status rows live in nametable rows 28 and 29.
const HUD_ROWS: [usize; 2] = [28, 29];

fn byte(prg: &[u8], address: u16) -> Result<u8, String> {
    read(prg, address).map_err(|error| error.to_string())
}

/// The text record `id` writes: its two-byte PPU address and data up to the
/// `$FD`/`$FE`/`$FF` terminator.
fn palette_record(prg: &[u8], id: u8, address: u16) -> Result<Vec<u8>, String> {
    let pointer = word(prg, TEXT_RECORDS + 2 * u16::from(id)).map_err(|e| e.to_string())?;
    let found = (u16::from(byte(prg, pointer)?) << 8) | u16::from(byte(prg, pointer + 1)?);
    if found != address {
        return Err(format!(
            "text record {id} writes ${found:04X}, expected ${address:04X}"
        ));
    }
    let mut data = Vec::new();
    let mut at = pointer + 2;
    loop {
        let value = byte(prg, at)?;
        at += 1;
        if value >= 0xFD {
            break;
        }
        data.push(value);
    }
    Ok(data)
}

/// A continuation record: palette bytes for the next `$2007` write, with no
/// address of its own.
fn continuation_record(prg: &[u8], id: u8) -> Result<Vec<u8>, String> {
    let pointer = word(prg, TEXT_RECORDS + 2 * u16::from(id)).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    let mut at = pointer;
    loop {
        let value = byte(prg, at)?;
        at += 1;
        if value >= 0xFE {
            break;
        }
        data.push(value);
    }
    Ok(data)
}

fn rows(data: &[u8], id: u8, length: usize) -> Result<[[u8; 4]; 4], String> {
    if data.len() != length {
        return Err(format!(
            "text record {id} carries {} palette bytes, expected {length}",
            data.len()
        ));
    }
    Ok(std::array::from_fn(|index| {
        data[4 * index..4 * index + 4].try_into().unwrap()
    }))
}

/// Decode the palette RAM uploads from the `$85ED` text records.
fn palettes(prg: &[u8]) -> Result<gradius_core::rom::Palettes, String> {
    let head = palette_record(prg, STAGE_BACKGROUND_PALETTES, 0x3F00)?;
    let tail = continuation_record(prg, BACKGROUND_TAIL)?;
    let ending_tail = continuation_record(prg, ENDING_BACKGROUND_TAIL)?;
    let mut background = head.clone();
    background.extend_from_slice(&tail);
    let mut ending_background = head;
    ending_background.extend_from_slice(&ending_tail);
    let title = palette_record(prg, TITLE_PALETTES, 0x3F00)?;
    let (title_background, title_sprite) = title.split_at(16);
    Ok(gradius_core::rom::Palettes {
        background: rows(&background, STAGE_BACKGROUND_PALETTES, 16)?,
        sprite: rows(
            &palette_record(prg, STAGE_SPRITE_PALETTES, 0x3F10)?,
            STAGE_SPRITE_PALETTES,
            16,
        )?,
        title_background: rows(title_background, TITLE_PALETTES, 16)?,
        title_sprite: rows(title_sprite, TITLE_PALETTES, 16)?,
        ending_background: rows(&ending_background, STAGE_BACKGROUND_PALETTES, 16)?,
        ending_sprite: rows(
            &palette_record(prg, ENDING_SPRITE_PALETTES, 0x3F10)?,
            ENDING_SPRITE_PALETTES,
            16,
        )?,
    })
}

/// Capture the two HUD rows `$89E8` builds into nametable 0.
fn hud_rows(cart: &Cartridge) -> Result<([u8; 32], [u8; 32]), String> {
    let mut emulator = Emulator::new(cart).map_err(|error| error.to_string())?;
    let plan = Plan {
        held: 0,
        schedule: vec![(180, BUTTON_START, 6), (240, BUTTON_START, 6)],
    };
    let mut runner = Runner::new();
    while emulator.frames < HUD_FRAME {
        runner.advance(&mut emulator, &plan, false);
    }
    let vram = &emulator.bus.ppu.vram;
    let table = usize::from(emulator.bus.ppu.ctrl & 1) * 0x400;
    let read_row = |row: usize| -> Result<[u8; 32], String> {
        let start = table + row * 32;
        vram.get(start..start + 32)
            .ok_or_else(|| format!("no nametable row {row} at frame {HUD_FRAME}"))?
            .try_into()
            .map_err(|_| format!("short nametable row {row}"))
    };
    Ok((read_row(HUD_ROWS[0])?, read_row(HUD_ROWS[1])?))
}

fn bytes<const N: usize>(prg: &[u8], base: u16) -> Result<[u8; N], String> {
    let mut out = [0u8; N];
    for (offset, byte) in out.iter_mut().enumerate() {
        *byte = read(prg, base.wrapping_add(offset as u16)).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Each stream runs from its pointer to the next higher pointer; the highest
/// one ends at `$C900`. A group is an offset byte, then tiles up to `$FE`/`$FF`.
fn explosion_streams(prg: &[u8]) -> Result<[Vec<ExplosionGroup>; EXPLOSION_STREAMS], String> {
    let map = |e: anyhow::Error| e.to_string();
    let pointers: Vec<u16> = (0..EXPLOSION_STREAMS as u16)
        .map(|index| word(prg, EXPLOSION_POINTERS + 2 * index).map_err(map))
        .collect::<Result<_, _>>()?;
    let mut streams: [Vec<ExplosionGroup>; EXPLOSION_STREAMS] = Default::default();
    for (index, stream) in streams.iter_mut().enumerate() {
        let start = pointers[index];
        let end = pointers
            .iter()
            .copied()
            .filter(|&pointer| pointer > start)
            .min()
            .unwrap_or(EXPLOSION_END);
        let mut at = start;
        while at < end {
            let magnitude = read(prg, at).map_err(map)?;
            at += 1;
            let mut tiles = Vec::new();
            loop {
                let byte = read(prg, at).map_err(map)?;
                at += 1;
                if byte >= 0xFE {
                    break;
                }
                tiles.push(byte);
            }
            stream.push(ExplosionGroup { magnitude, tiles });
        }
        if at != end {
            return Err(format!(
                "explosion stream {index} at ${start:04X} does not end on a group"
            ));
        }
    }
    Ok(streams)
}

pub(crate) fn extract(cart: &Cartridge) -> Result<Tables, String> {
    let prg = &cart.prg[..];
    let map = |e: anyhow::Error| e.to_string();
    let mut tables = Tables::default();
    for (index, row) in tables.spawns.iter_mut().enumerate() {
        let index = index as u16;
        *row = SpawnRow {
            object: bytes(prg, OBJECT_DEFS + 4 * index)?,
            size: bytes(prg, SIZE_ENTRIES + 2 * index)?,
            movement: bytes(prg, MOVEMENT[usize::from(index)])?,
        };
    }
    tables.explosions = explosion_streams(prg)?;
    tables.music = bytes::<STAGE_COUNT>(prg, MUSIC)?;
    tables.music_boundary = bytes::<STAGE_COUNT>(prg, MUSIC_BOUNDARY)?;
    for (index, root) in tables.script_root.iter_mut().enumerate() {
        *root = word(prg, SCRIPT_ROOTS + 2 * index as u16).map_err(map)?;
    }
    tables.cannon_tip = bytes::<16>(prg, CANNON_TIP)?;
    tables.cannon_tip_attr = bytes::<4>(prg, CANNON_TIP_ATTR)?;
    tables.eruption_script = bytes::<32>(prg, ERUPTION_SCRIPT)?;
    tables.hazard_script = bytes::<32>(prg, HAZARD_SCRIPT)?;
    tables.beam_script = bytes::<32>(prg, BEAM_SCRIPT)?;
    let positions = bytes::<32>(prg, HAZARD_POSITIONS)?;
    tables.hazard_positions =
        std::array::from_fn(|index| (positions[2 * index], positions[2 * index + 1]));
    tables.destroy_sfx = bytes::<22>(prg, DESTROY_SFX)?;
    tables.ship_start = bytes::<26>(prg, SHIP_START)?;
    tables.demo_input = bytes::<152>(prg, DEMO_INPUT)?;
    tables.stage3_script_y = bytes::<16>(prg, STAGE3_SCRIPT_Y)?;
    tables.cannon_step_x = bytes::<32>(prg, CANNON_STEP_X)?;
    tables.cannon_step_y = bytes::<32>(prg, CANNON_STEP_Y)?;
    let groups = bytes::<32>(prg, TURRET_GROUPS)?;
    tables.turret_groups =
        std::array::from_fn(|index| groups[4 * index..4 * index + 4].try_into().unwrap());
    tables.palettes = palettes(prg)?;
    (tables.hud_meter_row, tables.hud_status_row) = hud_rows(cart)?;
    Ok(tables)
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

    #[test]
    fn palettes_and_hud_rows_are_the_cartridge_uploads() {
        let Some(rom) = std::env::var_os("GRADIUS_ROM") else {
            return;
        };
        let cart = crate::load_cart(&std::fs::read(rom).unwrap()).unwrap();
        let tables = extract(&cart).unwrap();
        assert_eq!(tables.palettes.background[0], [0x0F, 0x12, 0x30, 0x0F]);
        assert_eq!(tables.palettes.background[1], [0x0F, 0x27, 0x30, 0x0F]);
        assert_eq!(tables.palettes.sprite[3], [0x0F, 0x06, 0x26, 0x30]);
        assert_eq!(tables.palettes.title_sprite[3], [0x0F, 0x06, 0x0F, 0x30]);
        assert_eq!(
            tables.palettes.ending_background[2],
            [0x0F, 0x21, 0x2B, 0x25]
        );
        assert_eq!(tables.palettes.ending_sprite[0], [0x0F, 0x12, 0x27, 0x31]);
        assert_eq!(&tables.hud_meter_row[4..8], &[0x09, 0x0A, 0x0B, 0x0C]);
        assert_eq!(&tables.hud_status_row[8..10], &[0x31, 0x66]);
    }
}
