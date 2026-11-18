//! Software renderer: indexed tiles and RGB pixels into an RGBA buffer.
//!
//! Rendering is part of the core on purpose. It is cheap at 256x240, it is
//! identical everywhere, and it makes frame comparison against the oracle a
//! byte-for-byte operation. Frontends only upload the finished buffer.

use crate::assets::{Assets, TileRef};

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;

/// FNV-1a offset basis, used for the framebuffer and state hashes.
pub const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
/// FNV-1a prime.
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// The NES evaluates at most this many sprites on one scanline.
pub const SPRITES_PER_SCANLINE: usize = 8;
/// OAM has 64 entries; the port's software list uses the same bound.
pub const MAX_SPRITES: usize = 64;

/// One entry of the software OAM list.
///
/// The fields mirror the hardware sprite attributes the port needs:
/// `tile` is an 8x16 pair, `palette` is the four RGB colors of one sprite
/// palette, `behind` is attribute bit 5 (behind the background) and the two
/// flip bits are attributes 6 and 7. Entries are kept in OAM order; the NES
/// gives the earlier entry priority on a scanline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite {
    /// Left edge, in screen pixels (the hardware X byte).
    pub x: i32,
    /// Top edge, in screen pixels.
    pub y: i32,
    pub tile: TileRef,
    pub palette: [[u8; 3]; 4],
    pub flip_h: bool,
    pub flip_v: bool,
    pub behind: bool,
}

impl Sprite {
    /// An all-zero entry, used to fill the fixed-size list.
    pub const BLANK: Sprite = Sprite {
        x: 0,
        y: 0,
        tile: TileRef { bank: 0, index: 0 },
        palette: [[0; 3]; 4],
        flip_h: false,
        flip_v: false,
        behind: false,
    };
}

/// A fixed-capacity OAM-like list, rebuilt every frame.
///
/// The original rebuilds `$0200-$02FF` in `$8B15` and lets the PPU resolve
/// priority; the port keeps the same order and applies the hardware rules in
/// [`Framebuffer::draw_sprites`]. No allocation happens in the frame loop.
#[derive(Clone, Debug)]
pub struct SpriteList {
    sprites: [Sprite; MAX_SPRITES],
    /// OAM slot of each entry. Only meaningful when `slotted` is set.
    slots: [u8; MAX_SPRITES],
    slotted: bool,
    len: usize,
}

impl Default for SpriteList {
    fn default() -> Self {
        Self::new()
    }
}

impl SpriteList {
    pub fn new() -> Self {
        Self {
            sprites: [Sprite::BLANK; MAX_SPRITES],
            slots: [0; MAX_SPRITES],
            slotted: false,
            len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.slotted = false;
    }

    /// Append one entry. Entries past OAM's 64 are dropped, like hardware.
    pub fn push(&mut self, sprite: Sprite) {
        if self.len < MAX_SPRITES {
            self.sprites[self.len] = sprite;
            self.slots[self.len] = self.len as u8;
            self.len += 1;
        }
    }

    /// The entry written to `slot`, when the list carries OAM slots.
    fn entry_in_slot(&self, slot: u8) -> Option<usize> {
        (0..self.len).find(|&index| self.slots[index] == slot)
    }

    /// Place the list in the original's OAM ring (`$8B15`/`$8AC5`).
    ///
    /// The first `fixed` entries are written at slots 0.., like the HUD bar.
    /// The rest start at the rotating byte offset `base` and advance by the
    /// `$C4` byte stride (49 OAM slots) each time, wrapping through the page.
    /// A later write replaces an earlier one in the same slot, exactly like
    /// the `$0200` buffer.
    pub fn apply_oam_ring(&mut self, base: u8, fixed: usize) {
        let mut owner = [usize::MAX; MAX_SPRITES];
        for (index, item) in owner.iter_mut().enumerate().take(self.len.min(fixed)) {
            self.slots[index] = index as u8;
            *item = index;
        }
        let mut slot = usize::from(base / 4) % MAX_SPRITES;
        for index in fixed..self.len {
            self.slots[index] = slot as u8;
            owner[slot] = index;
            slot = (slot + 49) % MAX_SPRITES;
            // `$8AF8`: `ADC #$C4` repeats while the byte is zero, so the
            // object ring skips slot 0 and keeps the HUD bar there.
            if slot == 0 {
                slot = 49;
            }
        }
        let mut write = 0;
        for read in 0..self.len {
            if owner[usize::from(self.slots[read])] == read {
                self.sprites[write] = self.sprites[read];
                self.slots[write] = self.slots[read];
                write += 1;
            }
        }
        self.len = write;
        self.slotted = true;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// The OAM slot and ring flag of each live entry, for a save state.
    pub(crate) fn slot_layout(&self) -> (bool, &[u8]) {
        (self.slotted, &self.slots[..self.len])
    }

    /// Restore a layout written by [`slot_layout`](Self::slot_layout).
    pub(crate) fn set_slot_layout(&mut self, slotted: bool, slots: &[u8]) {
        let len = slots.len().min(self.len);
        self.slots[..len].copy_from_slice(&slots[..len]);
        self.slotted = slotted;
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, index: usize) -> Option<&Sprite> {
        self.sprites.get(index).filter(|_| index < self.len)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Sprite> {
        self.sprites[..self.len].iter()
    }

    /// Per-sprite mask of the 16 rows the PPU would actually draw.
    ///
    /// A sprite is evaluated in OAM order; every one of its 16 scanlines
    /// counts towards the eight-sprite limit even when the pixels are
    /// transparent. This is what produces the original's characteristic
    /// flicker when many objects share a scanline.
    pub fn row_masks(&self) -> [u16; MAX_SPRITES] {
        let mut used = [0u8; HEIGHT];
        let mut masks = [0u16; MAX_SPRITES];
        if self.slotted {
            // The PPU evaluates OAM in slot order; the original writes the
            // list into a rotating, strided OAM ring, so insertion order is
            // not the order the eight-per-scanline limit sees.
            for slot in 0..MAX_SPRITES {
                let Some(index) = self.entry_in_slot(slot as u8) else {
                    continue;
                };
                masks[index] = self.mask_for(index, &mut used);
            }
        } else {
            for (index, mask) in masks.iter_mut().enumerate().take(self.len) {
                *mask = self.mask_for(index, &mut used);
            }
        }
        masks
    }

    fn mask_for(&self, index: usize, used: &mut [u8; HEIGHT]) -> u16 {
        let sprite = &self.sprites[index];
        let mut mask = 0u16;
        for line in 0..16 {
            let y = sprite.y + line;
            if !(0..HEIGHT as i32).contains(&y) {
                continue;
            }
            let row = y as usize;
            if used[row] < SPRITES_PER_SCANLINE as u8 {
                used[row] += 1;
                mask |= 1 << line;
            }
        }
        mask
    }
}

/// RGBA pixels, row-major, four bytes per pixel.
pub struct Framebuffer {
    pixels: Vec<u8>,
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; WIDTH * HEIGHT * 4],
        }
    }

    pub fn as_rgba(&self) -> &[u8] {
        &self.pixels
    }

    pub fn clear(&mut self, rgb: [u8; 3]) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 0xFF]);
        }
    }

    #[inline]
    pub fn pixel(&mut self, x: i32, y: i32, rgb: [u8; 3]) {
        if !(0..WIDTH as i32).contains(&x) || !(0..HEIGHT as i32).contains(&y) {
            return;
        }
        let offset = (y as usize * WIDTH + x as usize) * 4;
        self.pixels[offset..offset + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 0xFF]);
    }

    pub fn rect(&mut self, x: i32, y: i32, width: i32, height: i32, rgb: [u8; 3]) {
        for py in y..y + height {
            for px in x..x + width {
                self.pixel(px, py, rgb);
            }
        }
    }

    /// Draw one 8x8 tile. `palette[0]` is the transparent color when
    /// `transparent` is set.
    #[allow(clippy::too_many_arguments)]
    pub fn tile(
        &mut self,
        assets: &Assets,
        bank: u8,
        index: u8,
        x: i32,
        y: i32,
        flip_h: bool,
        flip_v: bool,
        palette: &[[u8; 3]; 4],
        transparent: bool,
    ) {
        let Some(tile) = assets.tile(bank, index) else {
            return;
        };
        for row in 0..8 {
            for col in 0..8 {
                let color = tile[row * 8 + col];
                if transparent && color == 0 {
                    continue;
                }
                let sx = if flip_h { 7 - col } else { col };
                let sy = if flip_v { 7 - row } else { row };
                self.pixel(x + sx as i32, y + sy as i32, palette[usize::from(color)]);
            }
        }
    }

    /// Draw one 8x16 hardware sprite: the top half is `tile`, the bottom half
    /// is the next tile in the same pattern table.
    ///
    /// Vertical flip is applied to the whole 16-pixel-tall sprite, matching the
    /// NES: the top tile lands in the bottom half and vice versa. Used for the
    /// player, shots, enemies and power-up capsules.
    #[allow(clippy::too_many_arguments)]
    pub fn sprite16(
        &mut self,
        assets: &Assets,
        tile: TileRef,
        x: i32,
        y: i32,
        flip_h: bool,
        flip_v: bool,
        palette: &[[u8; 3]; 4],
        transparent: bool,
    ) {
        for line in 0..16 {
            self.sprite16_line(
                assets,
                tile,
                x,
                y,
                line,
                flip_h,
                flip_v,
                palette,
                transparent,
                false,
            );
        }
    }

    /// Draw one scanline of an 8x16 hardware sprite. `line` is 0 at the top
    /// and 15 at the bottom; vertical flip is applied to the whole pair.
    #[allow(clippy::too_many_arguments)]
    pub fn sprite16_line(
        &mut self,
        assets: &Assets,
        tile: TileRef,
        x: i32,
        y: i32,
        line: usize,
        flip_h: bool,
        flip_v: bool,
        palette: &[[u8; 3]; 4],
        transparent: bool,
        clip_left: bool,
    ) {
        let line = line.min(15);
        let source_row = if flip_v { 15 - line } else { line };
        let index = tile.index.wrapping_add((source_row / 8) as u8);
        let Some(source) = assets.tile(tile.bank, index) else {
            return;
        };
        let row = source_row % 8;
        for col in 0..8 {
            let color = source[row * 8 + col];
            if transparent && color == 0 {
                continue;
            }
            let sx = x + if flip_h { 7 - col as i32 } else { col as i32 };
            // PPUMASK bit 2 hides sprite pixels in the leftmost eight columns.
            if clip_left && sx < 8 {
                continue;
            }
            self.pixel(sx, y + line as i32, palette[usize::from(color)]);
        }
    }

    /// Draw the selected OAM entries, honouring the eight-sprite-per-scanline
    /// limit, the OAM order and the left-8-pixel clip.
    ///
    /// Call once with `behind = true` before the background and once with
    /// `behind = false` after it; a behind sprite therefore only shows where
    /// the background is transparent, exactly like attribute bit 5.
    pub fn draw_sprites(
        &mut self,
        assets: &Assets,
        sprites: &SpriteList,
        behind: bool,
        clip_left: bool,
    ) {
        let masks = sprites.row_masks();
        if sprites.slotted {
            // Lower OAM slots win a same-priority overlap, so draw from the
            // highest slot down.
            for slot in (0..MAX_SPRITES).rev() {
                let Some(index) = sprites.entry_in_slot(slot as u8) else {
                    continue;
                };
                let sprite = &sprites.sprites[index];
                if sprite.behind != behind {
                    continue;
                }
                self.draw_sprite(assets, sprite, masks[index], clip_left);
            }
        } else {
            for (index, sprite) in sprites.iter().enumerate() {
                if sprite.behind != behind {
                    continue;
                }
                self.draw_sprite(assets, sprite, masks[index], clip_left);
            }
        }
    }

    fn draw_sprite(&mut self, assets: &Assets, sprite: &Sprite, mask: u16, clip_left: bool) {
        if mask == 0 {
            return;
        }
        // The ROM's sprite builder clips a part whose byte would exceed
        // 255 instead of wrapping it to the left edge.
        if !(0..WIDTH as i32).contains(&sprite.x) {
            return;
        }
        // Inside the byte range the hardware wraps: x = 250 draws the
        // right edge and continues at x = 0.
        let x = sprite.x & 0xFF;
        for line in 0..16 {
            if mask & (1 << line) == 0 {
                continue;
            }
            self.sprite16_line(
                assets,
                sprite.tile,
                x,
                sprite.y,
                line,
                sprite.flip_h,
                sprite.flip_v,
                &sprite.palette,
                true,
                clip_left,
            );
            if x + 8 > WIDTH as i32 {
                self.sprite16_line(
                    assets,
                    sprite.tile,
                    x - WIDTH as i32,
                    sprite.y,
                    line,
                    sprite.flip_h,
                    sprite.flip_v,
                    &sprite.palette,
                    true,
                    clip_left,
                );
            }
        }
    }

    /// FNV-1a over the finished RGBA buffer.
    pub fn frame_hash(&self) -> u64 {
        let mut hash = FNV_OFFSET;
        for byte in &self.pixels {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_outside_the_screen_are_ignored() {
        let mut fb = Framebuffer::new();
        fb.pixel(-1, 0, [1, 2, 3]);
        fb.pixel(0, -1, [1, 2, 3]);
        fb.pixel(WIDTH as i32, 0, [1, 2, 3]);
        assert!(fb.as_rgba().chunks_exact(4).all(|p| p[3] == 0));
    }

    #[test]
    fn rect_is_clipped() {
        let mut fb = Framebuffer::new();
        fb.rect(-4, -4, 8, 8, [9, 9, 9]);
        assert_eq!(&fb.as_rgba()[..4], &[9, 9, 9, 0xFF]);
    }

    fn two_tile_assets() -> Assets {
        let mut assets = Assets {
            tiles: vec![[0u8; 64]; 8 * 256],
            colors: vec![[0, 0, 0]; 64],
            ..Default::default()
        };
        assets.tiles[256 + 0x10] = [1; 64];
        assets.tiles[256 + 0x11] = [2; 64];
        assets
    }

    const PALETTE: [[u8; 3]; 4] = [[0, 0, 0], [10, 0, 0], [0, 20, 0], [0, 0, 30]];

    #[test]
    fn sprite16_stacks_the_pair() {
        let assets = two_tile_assets();
        let mut fb = Framebuffer::new();
        fb.sprite16(
            &assets,
            TileRef {
                bank: 1,
                index: 0x10,
            },
            0,
            0,
            false,
            false,
            &PALETTE,
            true,
        );
        assert_eq!(&fb.as_rgba()[..4], &[10, 0, 0, 0xFF]);
        assert_eq!(
            &fb.as_rgba()[(8 * WIDTH) * 4..(8 * WIDTH) * 4 + 4],
            &[0, 20, 0, 0xFF]
        );
        assert_eq!(
            &fb.as_rgba()[(15 * WIDTH) * 4..(15 * WIDTH) * 4 + 4],
            &[0, 20, 0, 0xFF]
        );
    }

    #[test]
    fn sprite16_flips_the_whole_16_pixels() {
        let assets = two_tile_assets();
        let mut fb = Framebuffer::new();
        fb.sprite16(
            &assets,
            TileRef {
                bank: 1,
                index: 0x10,
            },
            0,
            0,
            false,
            true,
            &PALETTE,
            true,
        );
        // The bottom tile (green) moves to the top half.
        assert_eq!(&fb.as_rgba()[..4], &[0, 20, 0, 0xFF]);
        assert_eq!(
            &fb.as_rgba()[(7 * WIDTH) * 4..(7 * WIDTH) * 4 + 4],
            &[0, 20, 0, 0xFF]
        );
        assert_eq!(
            &fb.as_rgba()[(8 * WIDTH) * 4..(8 * WIDTH) * 4 + 4],
            &[10, 0, 0, 0xFF]
        );
        assert_eq!(
            &fb.as_rgba()[(15 * WIDTH) * 4..(15 * WIDTH) * 4 + 4],
            &[10, 0, 0, 0xFF]
        );
    }

    #[test]
    fn sprite16_flips_horizontally() {
        let mut assets = Assets {
            tiles: vec![[0u8; 64]; 8 * 256],
            colors: vec![[0, 0, 0]; 64],
            ..Default::default()
        };
        // One pixel in each corner of the top tile, distinct colours.
        assets.tiles[256 + 0x10][0] = 1; // top-left
        assets.tiles[256 + 0x10][7] = 2; // top-right
        let mut fb = Framebuffer::new();
        fb.sprite16(
            &assets,
            TileRef {
                bank: 1,
                index: 0x10,
            },
            0,
            0,
            true,
            false,
            &PALETTE,
            true,
        );
        assert_eq!(&fb.as_rgba()[..4], &[0, 20, 0, 0xFF]);
        assert_eq!(&fb.as_rgba()[7 * 4..7 * 4 + 4], &[10, 0, 0, 0xFF]);
    }

    fn sprite_at(x: i32, y: i32) -> Sprite {
        Sprite {
            x,
            y,
            tile: TileRef {
                bank: 1,
                index: 0x10,
            },
            palette: PALETTE,
            flip_h: false,
            flip_v: false,
            behind: false,
        }
    }

    #[test]
    fn a_sprite_past_column_255_is_clipped_not_wrapped() {
        let assets = two_tile_assets();
        let mut fb = Framebuffer::new();
        let mut list = SpriteList::new();
        list.push(sprite_at(256, 0));
        // A sprite at x = 256 is entirely off screen; the ROM clips it, so
        // nothing appears at the left edge.
        fb.draw_sprites(&assets, &list, false, false);
        assert!(
            fb.as_rgba().chunks_exact(4).all(|p| p[3] == 0),
            "the x = 256 part must not wrap to column 0"
        );
    }

    #[test]
    fn oam_limits_eight_sprites_per_scanline() {
        let mut list = SpriteList::new();
        for _ in 0..12 {
            list.push(sprite_at(0, 40));
        }
        let masks = list.row_masks();
        for mask in masks.iter().take(8) {
            assert_eq!(*mask, 0xFFFF, "the first eight own every row");
        }
        for mask in masks.iter().skip(8) {
            assert_eq!(*mask, 0, "the ninth and later are dropped entirely");
        }
    }

    #[test]
    fn the_oam_ring_rotates_the_eight_sprite_priority() {
        let mut list = SpriteList::new();
        for index in 0..9 {
            list.push(sprite_at(index * 8, 0));
        }
        // `$2F` byte offsets advance 49 OAM slots per sprite part. Starting
        // at slot 1, the nine entries land at 1, 50, 35, 20, 5, 54, 39, 24, 9;
        // the PPU evaluates slot order, so the sixth inserted sprite (slot 54)
        // is the one that misses the scanline.
        list.apply_oam_ring(4, 0);
        let masks = list.row_masks();
        assert_eq!(masks[5], 0, "the sixth entry loses its only row");
        assert!(masks[..9]
            .iter()
            .enumerate()
            .all(|(index, mask)| (index == 5) || *mask == 0xFFFF));
    }

    #[test]
    fn the_ninth_sprite_still_draws_on_free_scanlines() {
        let mut list = SpriteList::new();
        for _ in 0..8 {
            list.push(sprite_at(0, 0));
        }
        list.push(sprite_at(0, 8));
        let masks = list.row_masks();
        // The last sprite spans screen rows 8..23. Rows 8..15 are already
        // taken by the first eight sprites; rows 16..23 are free.
        assert_eq!(masks[8], 0xFF00);
    }

    #[test]
    fn the_left_eight_pixels_are_clipped_when_ppumask_says_so() {
        let assets = two_tile_assets();
        let mut fb = Framebuffer::new();
        let mut list = SpriteList::new();
        // Columns 4..11: the hardware hides the part at x < 8.
        list.push(sprite_at(4, 0));
        fb.draw_sprites(&assets, &list, false, true);
        assert!(
            (4..8).all(|x| fb.as_rgba()[x * 4 + 3] == 0),
            "left 8 px hidden"
        );
        assert_eq!(&fb.as_rgba()[8 * 4..8 * 4 + 4], &[10, 0, 0, 0xFF]);

        let mut clipped = Framebuffer::new();
        clipped.draw_sprites(&assets, &list, false, false);
        assert_eq!(&clipped.as_rgba()[4 * 4..4 * 4 + 4], &[10, 0, 0, 0xFF]);
    }

    #[test]
    fn a_sprite_at_the_right_edge_wraps_to_column_zero() {
        let assets = two_tile_assets();
        let mut list = SpriteList::new();
        list.push(sprite_at(250, 0));
        let mut fb = Framebuffer::new();
        fb.draw_sprites(&assets, &list, false, false);
        // Columns 250..255 are drawn ...
        assert_eq!(&fb.as_rgba()[(250 * 4)..(250 * 4 + 4)], &[10, 0, 0, 0xFF]);
        // ... and the last two columns wrap to x = 0 and 1.
        assert_eq!(&fb.as_rgba()[..4], &[10, 0, 0, 0xFF]);
        assert_eq!(&fb.as_rgba()[4..8], &[10, 0, 0, 0xFF]);
        assert_eq!(&fb.as_rgba()[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    fn frame_hash_is_stable() {
        let mut first = Framebuffer::new();
        let mut second = Framebuffer::new();
        first.clear([1, 2, 3]);
        second.clear([1, 2, 3]);
        assert_eq!(first.frame_hash(), second.frame_hash());
        second.pixel(0, 0, [4, 5, 6]);
        assert_ne!(first.frame_hash(), second.frame_hash());
    }
}
