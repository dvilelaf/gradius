//! Stage-3 `$96`/`$C900` background turret data.
//!
//! The turret has no OAM sprite. It paints and erases nametable cells through
//! `$C9B4` and, on death, `$C7B0`. The `$CA23` art groups come from
//! `rom.tables`; the `$C930` cooldown and the `$C954` half-plane sector tests
//! stay here. See `docs/analysis/nametable_queue.md`.

use crate::rom::RomData;

/// `$C930`: animation cooldown in frames, indexed by `$17`.
const COOLDOWN: [u8; 7] = [80, 75, 70, 65, 60, 40, 30];

/// `$C930[rank]`. Rank 7 reads the first byte of the next table (`$C937 =
/// $A9`), the same one-byte overrun quirk as `$CA43`/`$CA51`.
pub fn cooldown(rank: u8) -> u8 {
    if rank >= 7 {
        0xA9
    } else {
        COOLDOWN[usize::from(rank)]
    }
}

/// One world-keyed nametable cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub world_col: u16,
    pub row: u8,
    pub tile: u8,
}

/// `$C9B4`: the cells one `$CA23` group writes at `base` (`$03BC:$03EC`).
pub fn paint_cells(base: u16, type_low: u8, group: [u8; 4], scroll: u16) -> Vec<Cell> {
    let first = if type_low == 2 {
        // The type-2 `-1` applies to both art groups and never borrows into
        // the high byte.
        (base & 0xFF00) | u16::from((base as u8).wrapping_sub(1))
    } else {
        base
    };
    let mut cells = Vec::with_capacity(3);
    cells.push(cell(first, group[0], scroll));
    cells.push(cell(first.wrapping_add(1), group[1], scroll));
    if group[2] == 0 {
        // The second record uses the *unadjusted* low byte plus `$20`, with
        // the carry added to the high byte.
        let low = (base as u8).wrapping_add(0x20);
        let high = (base >> 8).wrapping_add(u16::from(low < 0x20));
        cells.push(cell((high << 8) | u16::from(low), group[3], scroll));
    }
    cells
}

/// Bytes the `$0700` queue gains for one paint (`$0E`).
pub fn queued_bytes(group: [u8; 4]) -> u8 {
    6 + if group[2] == 0 { 5 } else { 0 }
}

fn cell(addr: u16, tile: u8, scroll: u16) -> Cell {
    Cell {
        world_col: world_column(addr, scroll),
        row: row(addr),
        tile,
    }
}

/// World tile column for a nametable address at `$3F:$3E = scroll`.
///
/// With vertical mirroring the table bit is world column bit 5. The result is
/// the first column at or after `scroll / 8` with the same `column % 64`.
pub fn world_column(addr: u16, scroll: u16) -> u16 {
    let encoded = u16::from(((addr >> 10) & 1) as u8) * 32 + (addr & 31);
    let scroll_column = scroll / 8;
    scroll_column + (encoded + 64 - (scroll_column & 63)) % 64
}

/// Nametable row encoded by a tile address.
pub fn row(addr: u16) -> u8 {
    ((addr & 0x03FF) >> 5) as u8
}

/// `$C954`: the four half-plane sector tests. The constants include the
/// carry each `SBC` enters with; see `docs/analysis/nametable_queue.md`.
pub fn in_sector(type_low: u8, player_x: i32, player_y: i32, obj_x: i32, obj_y: i32) -> bool {
    match type_low & 0x03 {
        0 => player_x + 10 < obj_x && player_y - 11 < obj_y,
        1 => player_y - 11 >= obj_y,
        2 => player_x - 10 >= obj_x && player_y - 10 < obj_y,
        _ => player_y + 11 < obj_y,
    }
}

/// `$C7B0`: the explosion cells for one type, keyed to world cells.
pub fn explosion_cells(rom: &RomData, type_low: u8, base: u16, scroll: u16) -> Vec<Cell> {
    let stream = usize::from(type_low & 0x03);
    let mut cells = Vec::new();
    let groups = &rom.tables.explosions[stream];
    for (slot, group) in groups.iter().enumerate() {
        let magnitude = u16::from(group.magnitude);
        // The ROM keeps only the magnitude of an offset, not its sign: every
        // group but the last of a stream sits below the turret address.
        let start = if slot + 1 < groups.len() {
            base.wrapping_sub(magnitude)
        } else {
            base.wrapping_add(magnitude)
        };
        for (index, tile) in group.tiles.iter().enumerate() {
            let addr = start.wrapping_add(index as u16);
            cells.push(cell(addr, *tile, scroll));
        }
    }
    cells
}

/// One `$0500`/`$0600` collision-map byte written by `$C81C`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionPatch {
    pub addr: u16,
    pub value: u8,
}

/// `$C81C`: the packed collision-map patch for the turret's address. Types 1
/// and 3 return immediately; stage 3 has no extracted collision map yet, so
/// the port keeps the writes as data but does not consume them.
pub fn collision_patches(type_low: u8, base: u16) -> Vec<CollisionPatch> {
    if type_low != 0 && type_low != 2 {
        return Vec::new();
    }
    let high = (base >> 8) as u8;
    let page: u8 = if high & 0x04 != 0 { 0x06 } else { 0x05 };
    // `$C82D`-`$C841`: `(high * 2 + carry, low << 3)` keeps `high & 7` in
    // bits 8-10 and `(low << 3) & $F8` in bits 3-7.
    let mut high = u16::from(high);
    let mut low = u16::from(base as u8);
    low <<= 1;
    if low & 0x100 != 0 {
        high = high.wrapping_add(1) & 0xFF;
        low &= 0xFF;
    }
    low = (low << 2) & 0xFF;
    let mut cell = ((high & 0x07) as u8) | ((low & 0xF8) as u8);
    if type_low == 2 {
        cell = cell.wrapping_sub(0x29);
    }
    let base_addr = (u16::from(page) << 8) | u16::from(cell);
    let mut patches = Vec::with_capacity(10);
    for offset in [0x00, 0x08, 0x10, 0x18, 0x20] {
        patches.push(CollisionPatch {
            addr: base_addr.wrapping_add(offset),
            value: 0x0F,
        });
    }
    for offset in [0x07, 0x0F, 0x17, 0x1F, 0x27] {
        patches.push(CollisionPatch {
            addr: base_addr.wrapping_add(offset),
            value: 0x00,
        });
    }
    patches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::testing::rom;

    fn groups() -> [[u8; 4]; 8] {
        rom().tables.turret_groups
    }

    #[test]
    fn type_zero_paints_base_then_alternate_world_cells() {
        let base = paint_cells(0x2323, 0, groups()[0], 848);
        assert_eq!(
            base,
            vec![
                Cell {
                    world_col: 131,
                    row: 25,
                    tile: 0x00
                },
                Cell {
                    world_col: 132,
                    row: 25,
                    tile: 0x8F
                },
                Cell {
                    world_col: 131,
                    row: 26,
                    tile: 0x90
                },
            ]
        );
        let alternate = paint_cells(0x2323, 0, groups()[4], 848);
        assert_eq!(
            alternate.iter().map(|c| c.tile).collect::<Vec<_>>(),
            vec![0x20, 0x21, 0x22]
        );
        // The same address 512 px later aliases the next 64-column block.
        assert_eq!(
            paint_cells(0x2323, 0, groups()[0], 848 + 512)[0].world_col,
            195
        );
    }

    #[test]
    fn type_two_starts_one_cell_left_and_the_second_row_is_unadjusted() {
        let cells = paint_cells(0x2323, 2, groups()[2], 848);
        assert_eq!(
            cells,
            vec![
                Cell {
                    world_col: 130,
                    row: 25,
                    tile: 0xB0
                },
                Cell {
                    world_col: 131,
                    row: 25,
                    tile: 0x00
                },
                Cell {
                    world_col: 131,
                    row: 26,
                    tile: 0xB1
                },
            ]
        );
    }

    #[test]
    fn the_third_byte_gates_the_second_record() {
        assert_eq!(paint_cells(0x2323, 0, groups()[1], 848).len(), 2);
        assert_eq!(paint_cells(0x2323, 0, groups()[0], 848).len(), 3);
        assert_eq!(queued_bytes(groups()[0]), 11);
        assert_eq!(queued_bytes(groups()[1]), 6);
    }

    #[test]
    fn every_type_opens_in_its_half_plane() {
        // Player below/above/right/left of the object.
        assert!(in_sector(0, 100, 100, 111, 112));
        assert!(!in_sector(0, 101, 100, 111, 112));
        assert!(in_sector(1, 100, 100, 120, 89));
        assert!(!in_sector(1, 100, 100, 120, 90));
        assert!(in_sector(2, 100, 100, 90, 101));
        assert!(!in_sector(2, 100, 100, 91, 101));
        assert!(in_sector(3, 100, 100, 120, 112));
        assert!(!in_sector(3, 100, 100, 120, 111));
    }

    #[test]
    fn the_type_zero_explosion_matches_the_f2000_order() {
        let cells = explosion_cells(crate::rom::testing::rom(), 0, 0x2323, 848);
        assert_eq!(cells.len(), 24);
        assert_eq!(
            &cells[..3],
            &[
                Cell {
                    world_col: 134,
                    row: 22,
                    tile: 0x00
                },
                Cell {
                    world_col: 135,
                    row: 22,
                    tile: 0x00
                },
                Cell {
                    world_col: 136,
                    row: 22,
                    tile: 0x00
                },
            ]
        );
        assert_eq!(
            *cells.last().unwrap(),
            Cell {
                world_col: 136,
                row: 26,
                tile: 0xB8
            }
        );
    }

    #[test]
    fn collision_patches_are_the_ten_c81c_bytes() {
        let patches = collision_patches(0, 0x2323);
        assert_eq!(patches.len(), 10);
        assert_eq!(
            patches[0],
            CollisionPatch {
                addr: 0x051B,
                value: 0x0F
            }
        );
        assert_eq!(
            patches[4],
            CollisionPatch {
                addr: 0x053B,
                value: 0x0F
            }
        );
        assert_eq!(
            patches[5],
            CollisionPatch {
                addr: 0x0522,
                value: 0x00
            }
        );
        assert_eq!(
            patches[9],
            CollisionPatch {
                addr: 0x0542,
                value: 0x00
            }
        );
        // Types 1 and 3 take the `$C84D` RTS.
        assert!(collision_patches(1, 0x2323).is_empty());
        assert!(collision_patches(3, 0x2323).is_empty());
    }
}
