//! Decode the ROM's sprite-group tables and render them as a labelled sheet.
//!
//! `$8AB5` reads a group pointer from two parallel byte tables and `$8AC5`
//! walks the definition it points at. The definition is a count byte followed
//! by `count` four-byte sprite records:
//!
//! ```text
//! count, (y, tile, attributes, x) * count
//! ```
//!
//! `y` and `x` are signed offsets added to the object's base position. A count
//! of zero means the entry draws nothing. Sprites are 8x16 pairs, so `tile`
//! names the top tile and `tile | 1` the bottom one.
//!
//! Two pointer tables exist, both with a two-byte stride (the caller shifts
//! the index left once before indexing):
//!
//! | table | low/high bytes | entries | index range |
//! | --- | --- | --- | --- |
//! | primary | `$8D91`/`$8D92` | 128 | 0-127 |
//! | extended | `$8E91`/`$8E92` | 36 | 0-35 |

use gradius_cart::{Cartridge, Tile};

/// Low/high bytes of the primary group-pointer table.
pub const PRIMARY_TABLE: u16 = 0x8D91;
/// Number of entries in the primary table (it ends at `$8E90`).
pub const PRIMARY_COUNT: usize = 128;
/// Low/high bytes of the extended group-pointer table.
pub const EXTENDED_TABLE: u16 = 0x8E91;
/// Number of entries in the extended table (it ends at `$8ED8`).
pub const EXTENDED_COUNT: usize = 36;

/// One 4-byte sprite record from a group definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupSprite {
    /// Signed Y offset from the object's base Y.
    pub y: i8,
    /// Top tile of the 8x16 sprite.
    pub tile: u8,
    /// OAM attribute byte (palette, flips, priority).
    pub attributes: u8,
    /// Signed X offset from the object's base X.
    pub x: i8,
}

/// One decoded group definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpriteGroup {
    /// 1 for the primary table, 2 for the extended one.
    pub table: u8,
    /// Entry index within its table.
    pub index: usize,
    /// Address of the definition in the PRG window.
    pub address: u16,
    pub sprites: Vec<GroupSprite>,
}

impl SpriteGroup {
    pub fn count(&self) -> usize {
        self.sprites.len()
    }
}

fn read(prg: &[u8], address: u16) -> Option<u8> {
    let offset = usize::from(address.checked_sub(0x8000)?);
    prg.get(offset).copied()
}

fn pointer(prg: &[u8], table: u16, index: usize) -> Option<u16> {
    let low = read(prg, table.checked_add(index as u16 * 2)?)?;
    let high = read(prg, table.checked_add(index as u16 * 2 + 1)?)?;
    Some(u16::from_le_bytes([low, high]))
}

/// Decode the definition at `address` (`count, (y, tile, attr, x) * count`).
pub fn decode_group(prg: &[u8], address: u16) -> Option<Vec<GroupSprite>> {
    let count = usize::from(read(prg, address)?);
    let mut sprites = Vec::with_capacity(count);
    for i in 0..count {
        let base = address.checked_add(1 + (i as u16) * 4)?;
        sprites.push(GroupSprite {
            y: read(prg, base)? as i8,
            tile: read(prg, base.checked_add(1)?)?,
            attributes: read(prg, base.checked_add(2)?)?,
            x: read(prg, base.checked_add(3)?)? as i8,
        });
    }
    Some(sprites)
}

/// Decode every entry of one pointer table.
pub fn decode_table(
    prg: &[u8],
    table: u16,
    count: usize,
    table_id: u8,
) -> Vec<Option<SpriteGroup>> {
    (0..count)
        .map(|index| {
            let address = pointer(prg, table, index)?;
            let sprites = decode_group(prg, address)?;
            Some(SpriteGroup {
                table: table_id,
                index,
                address,
                sprites,
            })
        })
        .collect()
}

/// Decode both tables, keeping empty (count 0) entries too.
pub fn decode_all(rom: &Cartridge) -> Vec<SpriteGroup> {
    let prg = &rom.prg;
    decode_table(prg, PRIMARY_TABLE, PRIMARY_COUNT, 1)
        .into_iter()
        .chain(decode_table(prg, EXTENDED_TABLE, EXTENDED_COUNT, 2))
        .flatten()
        .collect()
}

/// Sprite pattern-table halves: one per CNROM hardware bank.
pub fn sprite_banks(rom: &Cartridge) -> Vec<[Tile; 256]> {
    rom.chr_banks().into_iter().skip(1).step_by(2).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    /// Build a PRG window where the primary table has one pointer and the
    /// extended table is empty (all pointers zero).
    fn synthetic() -> Vec<u8> {
        let mut prg = vec![0u8; 0x8000];
        let def = 0x9000usize - 0x8000;
        prg[def] = 2;
        prg[def + 1..def + 5].copy_from_slice(&[0xFE, 0x61, 0x22, 0x08]); // y=-2
        prg[def + 5..def + 9].copy_from_slice(&[0x04, 0x63, 0x23, 0x00]);
        let table = (PRIMARY_TABLE - 0x8000) as usize;
        prg[table] = 0x00;
        prg[table + 1] = 0x90;
        prg
    }

    #[test]
    fn decodes_count_and_signed_offsets() {
        let prg = synthetic();
        let groups = decode_table(&prg, PRIMARY_TABLE, 4, 1);
        let group = groups[0].as_ref().unwrap();
        assert_eq!(group.address, 0x9000);
        assert_eq!(group.count(), 2);
        assert_eq!(
            group.sprites[0],
            GroupSprite {
                y: -2,
                tile: 0x61,
                attributes: 0x22,
                x: 8
            }
        );
        assert_eq!(
            group.sprites[1],
            GroupSprite {
                y: 4,
                tile: 0x63,
                attributes: 0x23,
                x: 0
            }
        );
    }

    #[test]
    fn empty_and_out_of_range_entries_are_kept_as_none() {
        let prg = synthetic();
        let groups = decode_table(&prg, PRIMARY_TABLE, 4, 1);
        // Entry 1 points at $0000, outside the PRG window.
        assert!(groups[1].is_none());
    }

    #[test]
    fn zero_count_entry_draws_nothing() {
        let mut prg = vec![0u8; 0x8000];
        let def = 0x9000usize - 0x8000;
        prg[def] = 0;
        let table = (PRIMARY_TABLE - 0x8000) as usize;
        prg[table] = 0x00;
        prg[table + 1] = 0x90;
        let group = decode_group(&prg, 0x9000).unwrap();
        assert!(group.is_empty());
    }

    #[test]
    fn sprite_banks_are_four_sprite_pattern_halves() -> Result<()> {
        let mut ines = vec![0u8; 16 + 32 * 1024 + 32 * 1024];
        ines[0..4].copy_from_slice(b"NES\x1a");
        ines[4] = 2; // two 16 KiB PRG banks
        ines[5] = 4; // four 8 KiB CHR banks
        ines[6] = 0x30; // mapper 3, vertical mirroring
        let cart = Cartridge::from_ines(&ines)?;
        let banks = sprite_banks(&cart);
        assert_eq!(banks.len(), 4);
        Ok(())
    }
}
