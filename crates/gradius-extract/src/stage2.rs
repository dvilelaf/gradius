//! Dump stage 2's spawn and terrain tables.
//!
//! Stage 2 is `$19 = 1`. Its spawn root `$A7CA[$19]` is `$A7E8`, a sub-table of
//! eight column-stream pointers selected by `$3F & $0E`. The streams use the
//! same `$A32F` parser as `xtask sections`, so this command walks them with
//! [`sections::decode_records`] and collects the `$A65C` terrain placements
//! (`type < $80`) from all eight columns.
//!
//! ```text
//! cargo xtask stage2-tables "Gradius (E).nes"
//! ```
//!
//! Nothing here invents a byte: the output only counts the streams, their
//! records and the resulting terrain placement list.

use anyhow::{anyhow, bail, Result};

use crate::sections::{self, Record, Terrain};

/// `$19`: the zero-based stage number of stage 2.
const STAGE_INDEX: u16 = 1;
/// `$A7CA`: the per-stage spawn column-pointer table.
const SPAWN_TABLE: u16 = 0xA7CA;
/// `$9A30`: the per-stage boss transition threshold (`$3F` units of 256 px).
const BOSS_THRESHOLD: u16 = 0x9A30;
/// Stage 2 stores eight column pointers.
pub const COLUMN_COUNT: usize = 8;

/// Decoded stage-2 spawn data.
pub struct Stage2 {
    /// `$A7CA[1]`.
    pub spawn: u16,
    /// `$9A30[1]`.
    pub boss: u8,
    /// The eight column pointers, in `$3F & $0E` order.
    pub streams: [u16; COLUMN_COUNT],
    /// Every record of each column stream, in stream order.
    pub columns: [Vec<Record>; COLUMN_COUNT],
}

impl Stage2 {
    /// The `$A65C` terrain placements of all columns, in stream order.
    pub fn terrain(&self) -> Vec<Terrain> {
        self.columns
            .iter()
            .flatten()
            .filter_map(|record| match record {
                Record::Spawn { delay, kind } if *kind < 0x80 => Some(Terrain {
                    column: *delay,
                    kind: *kind,
                }),
                _ => None,
            })
            .collect()
    }
}

pub fn read(prg: &[u8], addr: u16) -> Result<u8> {
    prg.get(usize::from(addr.wrapping_sub(0x8000)))
        .copied()
        .ok_or_else(|| anyhow!("address ${addr:04X} is outside the 32 KiB PRG"))
}

pub fn word(prg: &[u8], addr: u16) -> Result<u16> {
    Ok(u16::from_le_bytes([
        read(prg, addr)?,
        read(prg, addr.wrapping_add(1))?,
    ]))
}

/// Decode stage 2's spawn sub-table and every column stream.
pub fn decode(prg: &[u8]) -> Result<Stage2> {
    let spawn = word(prg, SPAWN_TABLE + 2 * STAGE_INDEX)?;
    if spawn < 0x8000 {
        bail!("stage 2 spawn sub-table ${spawn:04X} is outside the PRG window");
    }

    let mut streams = [0u16; COLUMN_COUNT];
    for (column, pointer) in streams.iter_mut().enumerate() {
        *pointer = word(prg, spawn.wrapping_add(2 * column as u16))?;
        if *pointer < 0x8000 {
            bail!("stage 2 column {column} pointer ${pointer:04X} is outside the PRG window");
        }
    }

    let mut columns: [Vec<Record>; COLUMN_COUNT] = std::array::from_fn(|_| Vec::new());
    for (column, records) in columns.iter_mut().enumerate() {
        *records = sections::decode_records(prg, streams[column])?;
    }

    Ok(Stage2 {
        spawn,
        boss: read(prg, BOSS_THRESHOLD + STAGE_INDEX)?,
        streams,
        columns,
    })
}

/// Summarise the decoded stage-2 spawn data.
pub fn render(stage: &Stage2) -> String {
    let mut text = format!(
        "stage 2 spawn ${:04X} boss threshold ${:02X}\n",
        stage.spawn, stage.boss
    );
    for (column, records) in stage.columns.iter().enumerate() {
        text.push_str(&format!(
            "column {column} ${:04X}: {} records\n",
            stage.streams[column],
            records.len()
        ));
    }
    text.push_str(&format!("terrain placements: {}\n", stage.terrain().len()));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(prg: &mut [u8], addr: u16, value: u16) {
        let offset = usize::from(addr - 0x8000);
        prg[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    /// One synthetic cartridge with an eight-column stage-2 sub-table.
    fn synthetic() -> Vec<u8> {
        let mut prg = vec![0u8; 0x8000];
        write(&mut prg, SPAWN_TABLE + 2 * STAGE_INDEX, 0xA800);
        for column in 0..COLUMN_COUNT {
            let stream = 0xA820 + 0x10 * column as u16;
            write(&mut prg, 0xA800 + 2 * column as u16, stream);
            let offset = usize::from(stream - 0x8000);
            if column == 0 {
                // enemy spawn, terrain, five-byte command, end
                prg[offset..offset + 10]
                    .copy_from_slice(&[0x10, 0x80, 0x20, 0x05, 0x30, 0xF1, 1, 2, 3, 0xFF]);
            } else {
                prg[offset..offset + 3].copy_from_slice(&[0x10, 0x81, 0xFF]);
            }
        }
        prg[usize::from(BOSS_THRESHOLD - 0x8000 + STAGE_INDEX)] = 0x0C;
        prg
    }

    #[test]
    fn decode_reads_the_sub_table_streams_and_thresholds() {
        let stage = decode(&synthetic()).unwrap();
        assert_eq!(stage.spawn, 0xA800);
        assert_eq!(stage.boss, 0x0C);
        assert_eq!(stage.streams[0], 0xA820);
        assert_eq!(stage.streams[7], 0xA820 + 0x10 * 7);
        assert_eq!(stage.columns[0].len(), 3);
        assert_eq!(
            stage.columns[0][1],
            Record::Spawn {
                delay: 0x20,
                kind: 0x05
            }
        );
        assert_eq!(stage.columns[0][2], Record::Command([0x30, 0xF1, 1, 2, 3]));
        assert_eq!(stage.terrain().len(), 1);
    }

    #[test]
    fn decode_rejects_a_column_pointer_outside_prg() {
        let mut prg = synthetic();
        write(&mut prg, 0xA800 + 2, 0x0100);
        assert!(decode(&prg).is_err());
    }

    #[test]
    fn render_counts_every_column_and_the_terrain() {
        let text = render(&decode(&synthetic()).unwrap());
        for column in 0..COLUMN_COUNT {
            assert!(text.contains(&format!("column {column} $")));
        }
        assert!(text.contains("boss threshold $0C"));
        assert!(text.contains("terrain placements: 1"));
    }
}
