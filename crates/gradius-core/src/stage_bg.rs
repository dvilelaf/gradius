//! Hand-written scalar roots of each stage's background and the screen helpers.
//!
//! The background pixels themselves (title, stages 1-7, the solidity set)
//! live in `rom.backgrounds`, the `BKGD` section of `CHUNK_ROM`.

/// One extracted screen or scrollable background.
pub type Screen = crate::rom::backgrounds::RomScreen;

impl Screen {
    /// Tile index at `(x, y)`.
    pub fn tile(&self, x: usize, y: usize) -> u8 {
        self.tiles[y * self.width_tiles + x]
    }

    /// NES attribute byte covering tile `(x, y)`.
    pub fn attribute(&self, x: usize, y: usize) -> u8 {
        let row = y / 4;
        let column = x / 4;
        self.attributes[row * (self.width_tiles / 4) + column]
    }

    /// Background palette index (0-3) used by tile `(x, y)`.
    pub fn palette_index(&self, x: usize, y: usize) -> u8 {
        let quadrant = (y % 4) / 2 * 2 + (x % 4) / 2;
        (self.attribute(x, y) >> (quadrant * 2)) & 0x03
    }
}

/// Whether a background tile is solid for the port's terrain collision.
pub fn is_solid(rom: &crate::rom::RomData, tile: u8) -> bool {
    rom.backgrounds.solid[tile as usize] != 0
}

/// The stage's scrolling background, or `None` past stage 7.
pub fn background(rom: &crate::rom::RomData, index: usize) -> Option<&Screen> {
    rom.backgrounds.stages.get(index)
}

/// `$19`: zero-based stage number.
pub const STAGE2_INDEX: u8 = 1;
/// `$A7CA[$19]`: spawn column sub-table.
pub const STAGE2_SPAWN_ROOT: u16 = 0xA7E8;
/// Column pointers in the spawn sub-table.
pub const STAGE2_SPAWN_COLUMNS: usize = 8;
/// `$A65C` terrain placements in this stage's spawn streams.
pub const STAGE2_TERRAIN_COUNT: usize = 101;
/// `$9FB6[$19]`: background height map.
pub const STAGE2_BG_HEIGHT: u16 = 0xCF64;
/// `$9FC6[$19]`: background base offset.
pub const STAGE2_BG_BASE: u16 = 0xD380;
/// `$9FD6[$19]`: background tile stream.
pub const STAGE2_BG_TILES: u16 = 0xD772;
/// `$9FAE[$19]`: background attribute parameter.
pub const STAGE2_BG_ATTR: u8 = 0x41;
/// `$8344[$19]`: second music theme.
pub const STAGE2_MUSIC: u8 = 0x59;
/// `$C433[$19]`: collision/attack handler.
pub const STAGE2_COLLISION: u16 = 0xC540;
/// `$98F0[$19]`: scroll end.
pub const STAGE2_END_THRESHOLD: u8 = 0x0E;
/// `$9A30[$19]`: boss transition threshold.
pub const STAGE2_BOSS_THRESHOLD: u8 = 0x0C;
/// `$9BC6[$19]`: start row parameter.
pub const STAGE2_START_ROW: u8 = 0x05;

pub const STAGE3_INDEX: u8 = 2;
/// `$A7CA[$19]`: spawn column sub-table.
pub const STAGE3_SPAWN_ROOT: u16 = 0xA7F8;
/// Column pointers in the spawn sub-table.
pub const STAGE3_SPAWN_COLUMNS: usize = 7;
/// `$A65C` terrain placements in this stage's spawn streams.
pub const STAGE3_TERRAIN_COUNT: usize = 18;
/// `$9FB6[$19]`: background height map.
pub const STAGE3_BG_HEIGHT: u16 = 0xCF72;
/// `$9FC6[$19]`: background base offset.
pub const STAGE3_BG_BASE: u16 = 0xD498;
/// `$9FD6[$19]`: background tile stream.
pub const STAGE3_BG_TILES: u16 = 0xD604;
/// `$9FAE[$19]`: background attribute parameter.
pub const STAGE3_BG_ATTR: u8 = 0x40;
/// `$8344[$19]`: second music theme.
pub const STAGE3_MUSIC: u8 = 0x5B;
/// `$C433[$19]`: collision/attack handler.
pub const STAGE3_COLLISION: u16 = 0xC680;
/// `$98F0[$19]`: scroll end.
pub const STAGE3_END_THRESHOLD: u8 = 0x0E;
/// `$9A30[$19]`: boss transition threshold.
pub const STAGE3_BOSS_THRESHOLD: u8 = 0x0C;
/// `$9BC6[$19]`: start row parameter.
pub const STAGE3_START_ROW: u8 = 0x0A;

/// `$19` for stage 4: zero-based stage number.
pub const STAGE4_INDEX: u8 = 3;
/// `$A7CA[$19]`: spawn column sub-table.
pub const STAGE4_SPAWN_ROOT: u16 = 0xA806;
/// Column pointers in the spawn sub-table.
pub const STAGE4_SPAWN_COLUMNS: usize = 7;
/// `$A65C` terrain placements in this stage's spawn streams.
pub const STAGE4_TERRAIN_COUNT: usize = 63;
/// `$9FB6[$19]`: background height map.
pub const STAGE4_BG_HEIGHT: u16 = 0xCF56;
/// `$9FC6[$19]`: background base offset.
pub const STAGE4_BG_BASE: u16 = 0xD188;
/// `$9FD6[$19]`: background tile stream.
pub const STAGE4_BG_TILES: u16 = 0xD772;
/// `$9FAE[$19]`: background attribute parameter.
pub const STAGE4_BG_ATTR: u8 = 0x40;
/// `$8344[$19]`: second music theme.
pub const STAGE4_MUSIC: u8 = 0x5D;
/// `$C433[$19]`: collision/attack handler.
pub const STAGE4_COLLISION: u16 = 0xC5A7;
/// `$98F0[$19]`: scroll end.
pub const STAGE4_END_THRESHOLD: u8 = 0x0E;
/// `$9A30[$19]`: boss transition threshold.
pub const STAGE4_BOSS_THRESHOLD: u8 = 0x0C;
/// `$9BC6[$19]`: start row parameter.
pub const STAGE4_START_ROW: u8 = 0x00;

/// `$19` for stage 5: zero-based stage number.
pub const STAGE5_INDEX: u8 = 4;
/// `$A7CA[$19]`: spawn column sub-table.
pub const STAGE5_SPAWN_ROOT: u16 = 0xA814;
/// Column pointers in the spawn sub-table.
pub const STAGE5_SPAWN_COLUMNS: usize = 7;
/// `$A65C` terrain placements in this stage's spawn streams.
pub const STAGE5_TERRAIN_COUNT: usize = 0;
/// `$9FB6[$19]`: background height map.
pub const STAGE5_BG_HEIGHT: u16 = 0xCF7E;
/// `$9FC6[$19]`: background base offset.
pub const STAGE5_BG_BASE: u16 = 0xCF90;
/// `$9FD6[$19]`: background tile stream.
pub const STAGE5_BG_TILES: u16 = 0xD772;
/// `$9FAE[$19]`: background attribute parameter.
pub const STAGE5_BG_ATTR: u8 = 0x40;
/// `$8344[$19]`: second music theme.
pub const STAGE5_MUSIC: u8 = 0x5F;
/// `$C433[$19]`: collision/attack handler.
pub const STAGE5_COLLISION: u16 = 0xC64D;
/// `$98F0[$19]`: scroll end.
pub const STAGE5_END_THRESHOLD: u8 = 0x0D;
/// `$9A30[$19]`: boss transition threshold.
pub const STAGE5_BOSS_THRESHOLD: u8 = 0x0B;
/// `$9BC6[$19]`: start row parameter.
pub const STAGE5_START_ROW: u8 = 0x00;

/// `$19` for stage 6: zero-based stage number.
pub const STAGE6_INDEX: u8 = 5;
/// `$A7CA[$19]`: spawn column sub-table.
pub const STAGE6_SPAWN_ROOT: u16 = 0xA822;
/// Column pointers in the spawn sub-table.
pub const STAGE6_SPAWN_COLUMNS: usize = 7;
/// `$A65C` terrain placements in this stage's spawn streams.
pub const STAGE6_TERRAIN_COUNT: usize = 75;
/// `$9FB6[$19]`: background height map.
pub const STAGE6_BG_HEIGHT: u16 = 0xDF80;
/// `$9FC6[$19]`: background base offset.
pub const STAGE6_BG_BASE: u16 = 0xDDC0;
/// `$9FD6[$19]`: background tile stream.
pub const STAGE6_BG_TILES: u16 = 0xE003;
/// `$9FAE[$19]`: background attribute parameter.
pub const STAGE6_BG_ATTR: u8 = 0x42;
/// `$8344[$19]`: second music theme.
pub const STAGE6_MUSIC: u8 = 0x61;
/// `$C433[$19]`: collision/attack handler.
pub const STAGE6_COLLISION: u16 = 0xC6D8;
/// `$98F0[$19]`: scroll end.
pub const STAGE6_END_THRESHOLD: u8 = 0x0C;
/// `$9A30[$19]`: boss transition threshold.
pub const STAGE6_BOSS_THRESHOLD: u8 = 0x0B;
/// `$9BC6[$19]`: start row parameter.
pub const STAGE6_START_ROW: u8 = 0x0F;

/// `$19` for stage 7: zero-based stage number.
pub const STAGE7_INDEX: u8 = 6;
/// `$A7CA[$19]`: spawn column sub-table.
pub const STAGE7_SPAWN_ROOT: u16 = 0xA830;
/// Column pointers in the spawn sub-table.
pub const STAGE7_SPAWN_COLUMNS: usize = 7;
/// `$A65C` terrain placements in this stage's spawn streams.
pub const STAGE7_TERRAIN_COUNT: usize = 89;
/// `$9FB6[$19]`: background height map.
pub const STAGE7_BG_HEIGHT: u16 = 0xE701;
/// `$9FC6[$19]`: background base offset.
pub const STAGE7_BG_BASE: u16 = 0xE541;
/// `$9FD6[$19]`: background tile stream.
pub const STAGE7_BG_TILES: u16 = 0xE774;
/// `$9FAE[$19]`: background attribute parameter.
pub const STAGE7_BG_ATTR: u8 = 0x40;
/// `$8344[$19]`: second music theme.
pub const STAGE7_MUSIC: u8 = 0x63;
/// `$C433[$19]`: collision/attack handler.
pub const STAGE7_COLLISION: u16 = 0xC423;
/// `$98F0[$19]`: scroll end.
pub const STAGE7_END_THRESHOLD: u8 = 0x0D;
/// `$9A30[$19]`: boss transition threshold.
pub const STAGE7_BOSS_THRESHOLD: u8 = 0x0C;
/// `$9BC6[$19]`: start row parameter.
pub const STAGE7_START_ROW: u8 = 0x14;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::testing::rom;

    fn fnv1a(bytes: &[u8]) -> u64 {
        let mut hash = crate::video::FNV_OFFSET;
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(crate::video::FNV_PRIME);
        }
        hash
    }

    /// The values `xtask screens` recorded for each extracted screen: tile,
    /// attribute and palette digests. Pinned so an edit to the pack's
    /// backgrounds shows up.
    #[test]
    fn the_extracted_screens_match_the_oracle_digests() {
        let backgrounds = &rom().backgrounds;
        let title = &backgrounds.title;
        assert_eq!(fnv1a(&title.tiles), 0x9E836F0B444A5F1E);
        assert_eq!(fnv1a(&title.attributes), 0xA6EDECBFF5D998D9);
        assert_eq!(fnv1a(&title.palette), 0x681756AB770FFE1B);
        let digests = [
            (0x05740E863899C5AD, 0x053ABF48AA5F3BF5, 0x90FA63C90403F3FC),
            (0xF207532E288F4003, 0x6988DE65EF5F569F, 0xDF2BBDFA95164A70),
            (0xD8202719436CC2F9, 0xA48987175FAD0477, 0xB6BDFDD279505805),
            (0x228C903AE305586D, 0x9E8E0821170D5B2A, 0x90FA63C90403F3FC),
            (0x847ED131654EA7A1, 0x0BB9B51B269205C5, 0x90FA63C90403F3FC),
            (0xBCD5B9379F9F53EE, 0x2B9A4EB2607B8BE9, 0xE1DE0E2046002CA8),
            (0xACB8F84FFDFFB8C3, 0x39721C58E9B24C98, 0x959453030221F37D),
        ];
        assert_eq!(backgrounds.stages.len(), digests.len());
        for (index, (tiles, attributes, palette)) in digests.into_iter().enumerate() {
            let stage = &backgrounds.stages[index];
            assert_eq!(fnv1a(&stage.tiles), tiles, "stage {} tiles", index + 1);
            assert_eq!(
                fnv1a(&stage.attributes),
                attributes,
                "stage {} attributes",
                index + 1
            );
            assert_eq!(
                fnv1a(&stage.palette),
                palette,
                "stage {} palette",
                index + 1
            );
        }
    }

    #[test]
    fn the_stage_maps_have_the_documented_shape() {
        // (width in tiles, CHR bank) per stage; every map is 28 rows.
        let shapes = [
            (448, 0),
            (480, 0),
            (480, 1),
            (480, 0),
            (448, 1),
            (416, 2),
            (416, 3),
        ];
        for (index, (width, chr_bank)) in shapes.into_iter().enumerate() {
            let stage = background(rom(), index).unwrap();
            assert_eq!(stage.width_tiles, width, "stage {} width", index + 1);
            assert_eq!(stage.height_tiles, 28);
            assert_eq!(stage.tiles.len(), width * 28);
            assert_eq!(stage.attributes.len(), (width / 4) * 7);
            assert_eq!(stage.chr_bank, chr_bank, "stage {} CHR bank", index + 1);
        }
        assert!(background(rom(), 7).is_none());
        let title = &rom().backgrounds.title;
        assert_eq!((title.width_tiles, title.height_tiles), (32, 30));
        assert_eq!(title.chr_bank, 3);
    }

    /// Last 64 tile columns (the palette/CHR-bank tail) and the last 16
    /// attribute columns of stages 3-7. Pinned so a tail edit shows up.
    #[test]
    fn the_stage_tail_windows_match_the_oracle_digests() {
        let digests = [
            (2, 0x52409443334DB4C9, 0x1BFA42F1830FBD05),
            (3, 0x52409443334DB4C9, 0x1BFA42F1830FBD05),
            (4, 0x52409443334DB4C9, 0x1BFA42F1830FBD05),
            (5, 0xD95031480FA3FBFA, 0x6B87449079331FC0),
            (6, 0x8B8D202E6853468D, 0xBC3CA980F3AE3F47),
        ];
        for (index, tail_tiles, tail_attributes) in digests {
            let stage = background(rom(), index).unwrap();
            let width = stage.width_tiles;
            let mut tiles = Vec::new();
            for row in 0..stage.height_tiles {
                tiles.extend_from_slice(&stage.tiles[row * width + width - 64..(row + 1) * width]);
            }
            assert_eq!(fnv1a(&tiles), tail_tiles, "stage {} tail tiles", index + 1);
            let attribute_width = width / 4;
            let mut attributes = Vec::new();
            for row in 0..stage.height_tiles.div_ceil(4) {
                attributes.extend_from_slice(
                    &stage.attributes
                        [row * attribute_width + attribute_width - 16..(row + 1) * attribute_width],
                );
            }
            assert_eq!(
                fnv1a(&attributes),
                tail_attributes,
                "stage {} tail attributes",
                index + 1
            );
        }
    }

    #[test]
    fn starfield_tiles_are_not_solid() {
        for tile in 0x3A..=0x3F {
            assert!(
                !is_solid(rom(), tile),
                "tile ${tile:02X} is a star, not terrain"
            );
        }
        for tile in [0xDC, 0xDD, 0xBF] {
            assert!(is_solid(rom(), tile), "tile ${tile:02X} is terrain");
        }
        assert!(!is_solid(rom(), 0x00));
    }
}
