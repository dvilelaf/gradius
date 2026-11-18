//! `sprites` family of `CHUNK_ROM`: the two sprite-group pointer tables and
//! the definitions they point at, decoded from the PRG.

use gradius_cart::Cartridge;
use gradius_core::rom::sprites::Record;
use gradius_core::rom::Sprites;

use crate::sprite_groups::{
    decode_table, EXTENDED_COUNT, EXTENDED_TABLE, PRIMARY_COUNT, PRIMARY_TABLE,
};

pub(crate) fn extract(cart: &Cartridge) -> Result<Sprites, String> {
    Ok(Sprites {
        a: table(&cart.prg, PRIMARY_TABLE, PRIMARY_COUNT, 1)?,
        b: table(&cart.prg, EXTENDED_TABLE, EXTENDED_COUNT, 2)?,
    })
}

fn table(prg: &[u8], base: u16, count: usize, table: u8) -> Result<Vec<Vec<Record>>, String> {
    decode_table(prg, base, count, table)
        .into_iter()
        .enumerate()
        .map(|(index, group)| -> Result<Vec<Record>, String> {
            let group = group.ok_or_else(|| {
                format!("table {table} entry {index} points outside the PRG window")
            })?;
            Ok(group
                .sprites
                .iter()
                .map(|s| [s.y as u8, s.tile, s.attributes, s.x as u8])
                .collect())
        })
        .collect()
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
