//! Sprite groups decoded from the cartridge tables at `$8D91`/`$8E91`.
//!
//! A sprite type (`$0120`) below `$80` indexes table A; `$80` and above
//! index table B with the low seven bits. Each definition is a count then
//! `(dy, tile, attr, dx)` quads (`$8ED9`), exactly as `$8AB1` walks them.
//! `tile` is the low half of an 8x16 pair, so drawing uses `tile & !1`.

/// One entry of a sprite-group definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    /// Y offset from the object position (`$0320`).
    pub dy: i8,
    /// Bottom tile of the 8x16 pair; the top tile is `tile & 0xFE`.
    pub tile: u8,
    /// OAM attribute: palette, priority, flips.
    pub attr: u8,
    /// X offset from the object position (`$0360`).
    pub dx: i8,
}

use crate::rom::RomData;

/// The sprite group for a `$0120` sprite type, read from `rom.sprites`.
/// A type past the end of its table has no parts.
pub fn group(rom: &RomData, sprite: u8) -> impl Iterator<Item = Part> + '_ {
    let records = if sprite < 0x80 {
        rom.sprites.a.get(usize::from(sprite))
    } else {
        rom.sprites.b.get(usize::from(sprite & 0x7F))
    };
    records
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|&[dy, tile, attr, dx]| Part {
            dy: dy as i8,
            tile,
            attr,
            dx: dx as i8,
        })
}
