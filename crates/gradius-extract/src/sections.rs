//! Extract the per-section `$A65C` terrain placement evidence.
//!
//! `$A7CA` points at seven sub-tables of column-stream pointers (eight columns
//! for sections 0-1, seven for sections 2-6). Each stream is a list of 2-byte
//! `<delay> <type>` records and 5-byte command records, terminated by `$FF`.
//! A record whose type is below `$80` selects a 3-byte `$A65C` terrain
//! definition, so the stream is exactly the terrain placement evidence for
//! that section.
//!
//! ```text
//! cargo xtask sections "Gradius (E).nes"
//! ```
//!
//! The command prints a summary; `xtask extract` stores the records in the
//! `COLL` section of the pack. Nothing below invents a position or a hitbox: a
//! record only carries the spawn column and the `$A65C` index.

use anyhow::{anyhow, bail, Result};

/// Playable sections (`$A7CA` section 7 is unused).
pub const SECTION_COUNT: usize = 7;

/// `$A7CA`, the per-section column-pointer table.
const SECTION_POINTERS: u16 = 0xA7CA;
/// `$9A30`, the per-section `$3F` end threshold.
const SECTION_THRESHOLDS: u16 = 0x9A30;
/// End-of-stream marker, checked at the first byte of a record.
const END: u8 = 0xFF;

/// One `$A65C` terrain record, as it appears in a section's column stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terrain {
    /// Stream delay: the 16-px column at which `$A32F` consumes the record.
    pub column: u8,
    /// `$A65C` definition index (record type `< $80`).
    pub kind: u8,
}

/// One decoded record of a column stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Record {
    /// `<delay> <type>`: spawn after `2*delay` pixels.
    Spawn { delay: u8, kind: u8 },
    /// Five bytes copied to `$63-$67`; `$64` is reduced by `$70` by the caller.
    Command([u8; 5]),
}

/// Decoded section tables.
pub struct Sections {
    /// `$9A30`: section end in `$3F` units (256 px each).
    pub thresholds: [u8; SECTION_COUNT],
    /// Terrain records of each section, in stream order.
    pub terrain: [Vec<Terrain>; SECTION_COUNT],
}

fn read(prg: &[u8], addr: u16) -> Result<u8> {
    prg.get(usize::from(addr.wrapping_sub(0x8000)))
        .copied()
        .ok_or_else(|| anyhow!("address ${addr:04X} is outside the 32 KiB PRG"))
}

fn word(prg: &[u8], addr: u16) -> Result<u16> {
    Ok(u16::from_le_bytes([
        read(prg, addr)?,
        read(prg, addr.wrapping_add(1))?,
    ]))
}

/// Walk one column stream and return every record, in stream order.
pub fn decode_records(prg: &[u8], mut addr: u16) -> Result<Vec<Record>> {
    let mut out = Vec::new();
    // The longest stream is a few dozen bytes; this bound catches a bad pointer.
    for _ in 0..0x400 {
        let first = read(prg, addr)?;
        if first == END {
            return Ok(out);
        }
        let second = read(prg, addr.wrapping_add(1))?;
        if second >= 0xF0 {
            // Five-byte command record: `$63-$67`.
            let mut bytes = [0u8; 5];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = read(prg, addr.wrapping_add(index as u16))?;
            }
            out.push(Record::Command(bytes));
            addr = addr.wrapping_add(5);
        } else {
            out.push(Record::Spawn {
                delay: first,
                kind: second,
            });
            addr = addr.wrapping_add(2);
        }
    }
    bail!("stream at ${addr:04X} has no ${END:02X} terminator")
}

/// Walk one column stream and return its terrain records.
pub fn decode_stream(prg: &[u8], addr: u16) -> Result<Vec<Terrain>> {
    Ok(decode_records(prg, addr)?
        .into_iter()
        .filter_map(|record| match record {
            Record::Spawn { delay, kind } if kind < 0x80 => Some(Terrain {
                column: delay,
                kind,
            }),
            _ => None,
        })
        .collect())
}

/// Decode the seven sections from a PRG image.
pub fn decode(prg: &[u8]) -> Result<Sections> {
    let mut pointers = [0u16; SECTION_COUNT + 1];
    for (section, pointer) in pointers.iter_mut().enumerate() {
        *pointer = word(prg, SECTION_POINTERS.wrapping_add(2 * section as u16))?;
    }

    let mut thresholds = [0u8; SECTION_COUNT];
    for (section, threshold) in thresholds.iter_mut().enumerate() {
        *threshold = read(prg, SECTION_THRESHOLDS.wrapping_add(section as u16))?;
    }

    let mut terrain: [Vec<Terrain>; SECTION_COUNT] = std::array::from_fn(|_| Vec::new());
    for (section, records) in terrain.iter_mut().enumerate() {
        let start = pointers[section];
        let columns = usize::from(pointers[section + 1].wrapping_sub(start)) / 2;
        if columns == 0 || columns > 8 {
            bail!("section {section} has {columns} column pointers");
        }
        for column in 0..columns {
            let pointer = word(prg, start.wrapping_add(2 * column as u16))?;
            records.extend(decode_stream(prg, pointer)?);
        }
    }

    Ok(Sections {
        thresholds,
        terrain,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(prg: &mut [u8], addr: u16, value: u16) {
        let offset = usize::from(addr - 0x8000);
        prg[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn a_stream_keeps_only_terrain_records_and_stops_at_ff() {
        let mut prg = vec![0u8; 0x8000];
        let stream: &[u8] = &[
            0x10, 0x80, // enemy spawn: ignored
            0x20, 0xF0, 0x01, 0x02, 0x03, // five-byte command: skipped
            0x30, 0x2B, // terrain
            0xFF, 0x00, // end, then junk
        ];
        let offset = 0xA900usize - 0x8000;
        prg[offset..offset + stream.len()].copy_from_slice(stream);

        assert_eq!(
            decode_stream(&prg, 0xA900).unwrap(),
            [Terrain {
                column: 0x30,
                kind: 0x2B
            }]
        );
    }

    #[test]
    fn decode_records_keeps_spawns_and_commands_in_order() {
        let mut prg = vec![0u8; 0x8000];
        let stream: &[u8] = &[
            0x10, 0x80, // enemy spawn
            0x20, 0xF0, 0x01, 0x02, 0x03, // five-byte command
            0x30, 0x2B, // terrain
            0xFF,
        ];
        let offset = 0xA900usize - 0x8000;
        prg[offset..offset + stream.len()].copy_from_slice(stream);

        assert_eq!(
            decode_records(&prg, 0xA900).unwrap(),
            [
                Record::Spawn {
                    delay: 0x10,
                    kind: 0x80
                },
                Record::Command([0x20, 0xF0, 0x01, 0x02, 0x03]),
                Record::Spawn {
                    delay: 0x30,
                    kind: 0x2B
                },
            ]
        );
    }

    #[test]
    fn decode_reads_the_pointer_table_and_thresholds() {
        let mut prg = vec![0u8; 0x8000];
        // Eight sub-tables of one column pointer each; the last word ends
        // section 6 and is not a table.
        for section in 0..=SECTION_COUNT {
            write(
                &mut prg,
                SECTION_POINTERS + 2 * section as u16,
                0xA800 + 2 * section as u16,
            );
        }
        for section in 0..SECTION_COUNT {
            let stream = 0xA840 + 8 * section as u16;
            write(&mut prg, 0xA800 + 2 * section as u16, stream);
            prg[usize::from(stream - 0x8000)] = END;
        }
        // Section 0 gets two terrain records, section 3 one.
        for (stream, bytes) in [
            (0xA840u16, &[0x08u8, 0x01, 0x0C, 0x02, END][..]),
            (0xA858u16, &[0x40, 0x03, END][..]),
        ] {
            let offset = usize::from(stream - 0x8000);
            prg[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
        for (section, threshold) in [0x0Cu8, 0x0B, 0x0C, 0x0C, 0x0B, 0x0C, 0x0C]
            .into_iter()
            .enumerate()
        {
            prg[usize::from(SECTION_THRESHOLDS - 0x8000) + section] = threshold;
        }

        let sections = decode(&prg).unwrap();
        assert_eq!(
            sections.thresholds,
            [0x0C, 0x0B, 0x0C, 0x0C, 0x0B, 0x0C, 0x0C]
        );
        assert_eq!(
            sections.terrain[0],
            [
                Terrain {
                    column: 0x08,
                    kind: 0x01
                },
                Terrain {
                    column: 0x0C,
                    kind: 0x02
                }
            ]
        );
        assert_eq!(
            sections.terrain[3],
            [Terrain {
                column: 0x40,
                kind: 0x03
            }]
        );
        assert!(sections.terrain[1].is_empty());
        assert!(sections.terrain[6].is_empty());
    }
}
