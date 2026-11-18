//! Decode the `$0700` VRAM command queue and the `$C900` turret patches.
//!
//! The cartridge flushes `$0700` in the NMI (`$8A56`): each record is
//! `[type, addr_hi, addr_lo, tile..., $FF]`, a type byte of zero ends the
//! queue, and `type` selects the PPU increment through the `$8A50` table
//! (`1` = horizontal, `2` = vertical). `$C900`/`$C9B4` only queues type `1`.
//! See `docs/analysis/nametable_queue.md`.

/// One decoded `$0700` record.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VramRecord {
    /// Command type byte: `1` horizontal increment, `2` vertical.
    pub kind: u8,
    /// PPU address of the first data byte.
    pub addr: u16,
    /// Tile/data bytes written at `addr`, `addr+1`, ...
    pub data: Vec<u8>,
}

/// Decode a `$0700` page. Stops at the first zero type byte.
#[allow(dead_code)]
pub fn decode_queue(page: &[u8]) -> Vec<VramRecord> {
    let mut records = Vec::new();
    let mut index = 0;
    while index < page.len() {
        let kind = page[index];
        index += 1;
        if kind == 0 {
            break;
        }
        if index + 2 > page.len() {
            break;
        }
        let addr = (u16::from(page[index]) << 8) | u16::from(page[index + 1]);
        index += 2;
        let mut data = Vec::new();
        while index < page.len() && page[index] != 0xFF {
            data.push(page[index]);
            index += 1;
        }
        if index >= page.len() {
            break;
        }
        index += 1;
        records.push(VramRecord { kind, addr, data });
    }
    records
}

/// One nametable record `$C900` paints.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurretWrite {
    pub addr: u16,
    pub tiles: Vec<u8>,
}

/// Decode one `$CA23` four-byte group into the records `$C9B4` queues.
///
/// `type_low` is `$010C & 0x0F`. Type 2 writes the first record one cell to
/// the left (`$03EC - 1`, high byte unchanged). A zero third byte appends a
/// second record at the *unadjusted* `base + $20`.
#[allow(dead_code)]
pub fn turret_writes(base: u16, type_low: u8, group: [u8; 4]) -> Vec<TurretWrite> {
    let first = if type_low == 2 {
        (base & 0xFF00) | u16::from((base as u8).wrapping_sub(1))
    } else {
        base
    };
    let mut writes = vec![TurretWrite {
        addr: first,
        tiles: vec![group[0], group[1]],
    }];
    if group[2] == 0 {
        let low = (base as u8).wrapping_add(0x20);
        let high = (base >> 8).wrapping_add(u16::from(low < 0x20));
        writes.push(TurretWrite {
            addr: (high << 8) | u16::from(low),
            tiles: vec![group[3]],
        });
    }
    writes
}

/// `$CA23` art groups: base `4*type` then alternate `4*type + $10`.
pub fn turret_groups(prg: &[u8]) -> [[u8; 4]; 8] {
    std::array::from_fn(|index| {
        let at = usize::from(0xCA23u16 - 0x8000) + 4 * index;
        prg[at..at + 4].try_into().unwrap()
    })
}

/// Every `$(nametable address, tile bytes)` record a live `$96` turret can paint.
///
/// Both art phases are included because the handler flips between them; the
/// extraction run uses this to subtract turret art from the nametable it
/// samples. Slots are the ten `$0300`-page objects.
pub fn turret_paint_records(ram: &[u8], groups: &[[u8; 4]; 8]) -> Vec<TurretWrite> {
    let mut records = Vec::new();
    for slot in 0..10 {
        if ram[0x030C + slot] != 0x96 {
            continue;
        }
        let type_low = ram[0x010C + slot] & 0x0F;
        if type_low >= 4 {
            continue;
        }
        let base = (u16::from(ram[0x03BC + slot]) << 8) | u16::from(ram[0x03EC + slot]);
        for group in [usize::from(type_low), usize::from(type_low) + 4] {
            records.extend(turret_writes(base, type_low, groups[group]));
        }
    }
    records
}

/// Nametable row (`0..31`) encoded by a tile address.
#[allow(dead_code)]
pub fn row(addr: u16) -> u8 {
    ((addr & 0x03FF) >> 5) as u8
}

/// World tile column for a nametable address at a given `$3F:$3E` scroll.
///
/// With vertical mirroring the address's table bit is world column bit 5.
/// The result is the first column at or after `scroll / 8` with the same
/// `column % 64`; a turret is only painted while that offset is on screen.
#[allow(dead_code)]
pub fn world_column(addr: u16, scroll: u16) -> u16 {
    let encoded = u16::from(((addr >> 10) & 1) as u8) * 32 + (addr & 31);
    let scroll_column = scroll / 8;
    scroll_column + (encoded + 64 - (scroll_column & 63)) % 64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Invented art groups with the shape `$CA23` carries: base `4*type`
    /// then the alternate. A zero third byte adds the second record.
    fn groups() -> [[u8; 4]; 8] {
        [
            [0x11, 0x12, 0x00, 0x13],
            [0x14, 0x15, 0x01, 0x16],
            [0x17, 0x18, 0x01, 0x19],
            [0x1A, 0x1B, 0x01, 0x1C],
            [0x21, 0x22, 0x00, 0x23],
            [0x24, 0x25, 0x01, 0x26],
            [0x27, 0x28, 0x01, 0x29],
            [0x2A, 0x2B, 0x01, 0x2C],
        ]
    }

    #[test]
    fn queue_decodes_two_turret_records() {
        let page = [
            0x01, 0x23, 0x23, 0x20, 0x21, 0xFF, 0x01, 0x23, 0x43, 0x22, 0xFF, 0x00,
        ];
        assert_eq!(
            decode_queue(&page),
            vec![
                VramRecord {
                    kind: 1,
                    addr: 0x2323,
                    data: vec![0x20, 0x21],
                },
                VramRecord {
                    kind: 1,
                    addr: 0x2343,
                    data: vec![0x22],
                },
            ]
        );
    }

    #[test]
    fn queue_stops_at_the_zero_terminator() {
        let page = [
            0x01, 0x20, 0x00, 0xAA, 0xBB, 0xFF, 0x00, 0x01, 0x99, 0x99, 0x00, 0xFF,
        ];
        let records = decode_queue(&page);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].addr, 0x2000);
    }

    #[test]
    fn type_two_writes_one_cell_left_and_keeps_the_unadjusted_second_row() {
        let writes = turret_writes(0x21C5, 2, [0xB0, 0x00, 0x00, 0xB1]);
        assert_eq!(
            writes,
            vec![
                TurretWrite {
                    addr: 0x21C4,
                    tiles: vec![0xB0, 0x00],
                },
                TurretWrite {
                    addr: 0x21E5,
                    tiles: vec![0xB1],
                },
            ]
        );
    }

    #[test]
    fn type_two_low_byte_wraps_without_borrowing_into_the_high_byte() {
        let writes = turret_writes(0x2300, 2, [0x24, 0x25, 0x00, 0x27]);
        assert_eq!(writes[0].addr, 0x23FF);
        assert_eq!(writes[1].addr, 0x2320);
    }

    #[test]
    fn a_nonzero_third_byte_queues_one_record() {
        let writes = turret_writes(0x2323, 1, [0xD9, 0xDA, 0xFE, 0xFE]);
        assert_eq!(
            writes,
            vec![TurretWrite {
                addr: 0x2323,
                tiles: vec![0xD9, 0xDA],
            }]
        );
    }

    #[test]
    fn address_decodes_to_row_and_world_column() {
        assert_eq!(row(0x2323), 25);
        assert_eq!(row(0x2343), 26);
        assert_eq!(world_column(0x2323, 848), 131);
        // One full nametable (64 columns = 512 px) later the same address is
        // the next world column with that low six bits.
        assert_eq!(world_column(0x2323, 848 + 512), 131 + 64);
    }

    #[test]
    fn live_turrets_expose_both_art_phases() {
        let mut ram = [0u8; 0x800];
        ram[0x030C + 8] = 0x96;
        ram[0x010C + 8] = 0x80;
        ram[0x03BC + 8] = 0x23;
        ram[0x03EC + 8] = 0x23;
        let records = turret_paint_records(&ram, &groups());
        assert!(records.contains(&TurretWrite {
            addr: 0x2323,
            tiles: vec![0x11, 0x12],
        }));
        assert!(records.contains(&TurretWrite {
            addr: 0x2343,
            tiles: vec![0x13],
        }));
        assert!(records.contains(&TurretWrite {
            addr: 0x2323,
            tiles: vec![0x21, 0x22],
        }));
        assert!(records.contains(&TurretWrite {
            addr: 0x2343,
            tiles: vec![0x23],
        }));
        // A non-turret slot contributes nothing.
        let mut empty = [0u8; 0x800];
        empty[0x030C + 8] = 0x92;
        assert!(turret_paint_records(&empty, &groups()).is_empty());
    }
}
