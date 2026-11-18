//! The `$CF90` ending screen, built by `$9D88` one 4x4-tile cell at a time.
//!
//! The ending runs `$9C1E` four times per frame during `$8B`, each time calling
//! the shared column routine `$9D88`. One call paints one cell: the attribute
//! byte at `$23C0 + 8r + c`, and the four tile rows `4r..4r+3`. The build walks
//! nametable 0 attribute columns 0..7 (each with attribute rows 0..6), then
//! nametable 1 columns 0..3, 84 calls in total. `$9D88` sets `$57` on the
//! first call that finds the column position at or past the frozen scroll, so
//! the 84 painted calls need 21 frames and `$57` follows on the 22nd.
//!
//! The contents are the first 32-column screen of the stage-1 background: the
//! ending resets `$19 = $26 = 0` (`$9B38`) before `$8B`, so `$CF48[0]`/`$CF90`
//! block 0 select it. The port keeps that screen pre-decoded in
//! stage 1's `rom.backgrounds` and copies its cells in the same order.
//!
//! Measured on `Gradius (E).nes`: the finished region (tiles `$2000..$22C7`,
//! `$22D8..$237F` and attributes `$23C0..$23F7`) has FNV-1a 64
//! `F3949D72384C0FE1` at frames 7060, 7200, 7600 and 8000.

use crate::assets::Assets;
use crate::stage_bg::Screen;
use crate::video::Framebuffer;

/// CNROM bank 3, pattern table 0 (`$0000`); the pack numbers halves, so `3 * 2`.
pub const CHR_BANK: u8 = 6;
const WIDTH_TILES: usize = 32;
const HEIGHT_TILES: usize = 28;
const ATTRIBUTE_COLUMNS: usize = 8;
const ATTRIBUTE_ROWS: usize = 7;
/// `$2007` writes of one call: one attribute byte and 16 tiles.
const CALL_WRITES: u32 = 17;
/// Calls that paint a cell before `$57`: 56 in nametable 0 and 28 in nametable 1.
#[cfg(test)]
pub const CALLS: usize = 84;

/// The four calls `$9C1E` makes in one frame.
const CALLS_PER_FRAME: usize = 4;

/// `$58`: the offset inside the class block, one 4-row band per call.
pub struct EndingScreen {
    tiles: [u8; WIDTH_TILES * HEIGHT_TILES],
    attributes: [u8; ATTRIBUTE_ROWS * ATTRIBUTE_COLUMNS],
    /// `$54`: bit 7 selects columns 4..7 of the current nametable.
    column_low: u8,
    /// `$55`: bit 0 selects nametable 1.
    column_high: u8,
    /// `$58`: `0x20 * column_in_half + row_band`.
    row: u8,
    /// Painted cells (each one 17 `$2007` writes).
    pub calls: usize,
    /// `$57`.
    pub finished: bool,
}

impl Default for EndingScreen {
    fn default() -> Self {
        Self {
            tiles: [0; WIDTH_TILES * HEIGHT_TILES],
            attributes: [0; ATTRIBUTE_ROWS * ATTRIBUTE_COLUMNS],
            column_low: 0,
            column_high: 0,
            row: 0,
            calls: 0,
            finished: false,
        }
    }
}

impl EndingScreen {
    pub fn new() -> Self {
        Self::default()
    }

    /// `$9C1E`: four `$9D88` calls. Returns the `$2007` writes this frame.
    pub fn step(&mut self, source: &Screen) -> u32 {
        let mut writes = 0;
        for _ in 0..CALLS_PER_FRAME {
            if self.call(source) {
                writes += CALL_WRITES;
            }
        }
        writes
    }

    /// One `$9D88` call. Returns whether it painted a cell.
    fn call(&mut self, source: &Screen) -> bool {
        // `$9D88` clears `$57`, then a column start checks the position
        // against the frozen `$3E/$3F` and sets it again when the build is
        // already ahead of the screen.
        self.finished = false;
        if self.row == 0 && !self.ahead() {
            self.finished = true;
            return false;
        }
        let band = usize::from(self.row & 7);
        let group = usize::from((self.row & 0xF0) >> 5);
        let nametable = self.column_high & 1;
        // Nametable 1 is built ahead for the next screen; it repeats columns
        // 0..3, so it is counted but not copied over the displayed table.
        if nametable == 0 {
            let column = group + 4 * usize::from(self.column_low >> 7);
            let attribute = source.attribute(4 * column, 4 * band);
            self.attributes[band * ATTRIBUTE_COLUMNS + column] = attribute;
            for y in 0..4 {
                for x in 0..4 {
                    let (sx, sy) = (4 * column + x, 4 * band + y);
                    self.tiles[sy * WIDTH_TILES + sx] = source.tile(sx, sy);
                }
            }
        }
        self.calls += 1;
        self.advance();
        true
    }

    /// The `$9D8C`..`$9DA9` column-start check against scroll 0.
    fn ahead(&self) -> bool {
        match self.column_high {
            0 => true,
            1 => self.column_low < 0x80,
            _ => false,
        }
    }

    /// `$9F8E`..`$9FA8`: `$58 += 1`, or `+$1A` out of a column with a `$54`
    /// step (and its `$55` carry) at the end of every four columns.
    fn advance(&mut self) {
        if self.row & 7 < 6 {
            self.row += 1;
            return;
        }
        self.row = self.row.wrapping_add(0x1A);
        if self.row < 0x80 {
            return;
        }
        self.row = 0;
        if self.column_low >= 0x80 {
            self.column_low = 0;
            self.column_high = self.column_high.wrapping_add(1);
        } else {
            self.column_low += 0x80;
        }
    }

    /// Draw the 32x28 nametable with its attributes and the ending palette.
    ///
    /// The ending keeps the stage scroll latch `$4020` (coarse Y 1, fine Y 4),
    /// so nametable row 0 is 12 px above the top edge. `palette` is the ending's
    /// `$3F00..$3F0F` background rows from the pack.
    pub fn draw(&self, fb: &mut Framebuffer, assets: &Assets, palette: &[[u8; 4]; 4]) {
        let bank = CHR_BANK;
        for row in 0..HEIGHT_TILES {
            for column in 0..WIDTH_TILES {
                let attribute = self.attributes[(row / 4) * ATTRIBUTE_COLUMNS + column / 4];
                let quadrant = ((row & 2) >> 1) * 2 + ((column & 2) >> 1);
                let palette_index = usize::from((attribute >> (quadrant * 2)) & 3);
                let mut rgb = [[0u8; 3]; 4];
                for (entry, slot) in rgb.iter_mut().enumerate() {
                    *slot = assets.color(palette[palette_index][entry]);
                }
                fb.tile(
                    assets,
                    bank,
                    self.tiles[row * WIDTH_TILES + column],
                    column as i32 * 8,
                    row as i32 * 8 - 12,
                    false,
                    false,
                    &rgb,
                    true,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::testing::rom;
    use crate::stage_bg;

    fn source() -> &'static Screen {
        stage_bg::background(rom(), 0).expect("stage 1")
    }

    fn fnv1a(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    /// One known column from the oracle nametable at 7060: tiles `$2000..` (4
    /// columns wide, 28 rows) and the seven attribute bytes at `$23C0`.
    #[test]
    fn the_first_column_is_the_measured_oracle_column() {
        const TILES: [u8; 112] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x3F, 0x00, 0x3B, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3C, 0x00,
            0x00, 0x3D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3E, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x3C, 0x00, 0x00, 0x3D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x3B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x3E, 0x00, 0x3D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3A, 0x3A, 0x3A, 0x3A,
        ];
        const ATTRIBUTES: [u8; 7] = [0x4C, 0x10, 0x10, 0x10, 0x10, 0x10, 0x04];

        let mut screen = EndingScreen::new();
        screen.step(source());
        screen.step(source());

        let mut tiles = [0u8; 112];
        for row in 0..HEIGHT_TILES {
            for column in 0..4 {
                tiles[row * 4 + column] = screen.tiles[row * WIDTH_TILES + column];
            }
        }
        assert_eq!(tiles, TILES);
        for (row, expected) in ATTRIBUTES.iter().enumerate() {
            assert_eq!(screen.attributes[row * ATTRIBUTE_COLUMNS], *expected);
        }
    }

    /// 84 cells, four calls a frame: 21 frames of 68 writes, then `$57`.
    #[test]
    fn the_build_is_84_cells_at_68_writes_a_frame() {
        let mut screen = EndingScreen::new();
        for frame in 0..21 {
            assert_eq!(screen.step(source()), 68, "frame {frame}");
            assert_eq!(screen.calls, (frame + 1) * 4);
        }
        assert!(!screen.finished);
        assert_eq!(screen.calls, CALLS);
        // The next call finds the position past the scroll and sets `$57`.
        assert_eq!(screen.step(source()), 0);
        assert!(screen.finished);
    }

    /// The finished tiles and attributes are the oracle's background region.
    #[test]
    fn the_finished_screen_matches_the_measured_background_hash() {
        let mut screen = EndingScreen::new();
        for _ in 0..21 {
            screen.step(source());
        }
        let mut bytes = Vec::with_capacity(screen.tiles.len() + screen.attributes.len());
        bytes.extend_from_slice(&screen.tiles);
        bytes.extend_from_slice(&screen.attributes);
        assert_eq!(fnv1a(&bytes), 0x3959_8872_B505_27DD);
        assert_eq!(fnv1a(&screen.tiles), 0x86D9_82DC_D719_30B5);
        assert_eq!(fnv1a(&screen.attributes), 0x6289_65C1_E928_53CD);
        // The measured background region: tiles without the `$22C8` text row
        // plus the attributes, static at 7060/7200/7600/8000.
        let mut region = Vec::with_capacity(840);
        region.extend_from_slice(&screen.tiles[..0x2C8]);
        region.extend_from_slice(&screen.tiles[0x2D8..]);
        region.extend_from_slice(&screen.attributes);
        assert_eq!(fnv1a(&region), 0xF394_9D72_384C_0FE1);
    }
}
