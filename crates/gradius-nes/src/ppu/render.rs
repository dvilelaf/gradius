//! Scanline rendering: background, sprites and the framebuffer.

use super::*;
use gradius_cart::NES_MASTER_PALETTE;

impl Ppu {
    /// Render one visible scanline into an RGBA framebuffer.
    pub fn render_line(&mut self, y: usize, framebuffer: &mut [u8]) {
        debug_assert!(y < HEIGHT);
        let backdrop = NES_MASTER_PALETTE[usize::from(self.palette[0] & 0x3F)];
        let line = &mut framebuffer[y * WIDTH * 4..(y + 1) * WIDTH * 4];

        let bg_enabled = self.mask & 0x08 != 0;
        let sprites_enabled = self.mask & 0x10 != 0;
        if !bg_enabled && !sprites_enabled {
            for pixel in line.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[backdrop[0], backdrop[1], backdrop[2], 0xFF]);
            }
            return;
        }

        let show_left_bg = self.mask & 0x02 != 0;
        let show_left_sprite = self.mask & 0x04 != 0;

        // Horizontal scroll is reloaded from `t` at dot 257 of the preceding
        // line, i.e. before this line's fetches start. CPU writes made during
        // the previous line (including mid-line sprite-0 splits and HBlank)
        // therefore apply to this line.
        self.v = (self.v & !0x041F) | (self.t & 0x041F);

        // Background. Fetch one tile more than the screen width so fine X can
        // shift the whole line without re-walking the nametable per pixel.
        let background = if bg_enabled {
            let mut tiles = [(0u8, 0u8, 0u8); 33];
            let mut v = self.v;
            for tile in &mut tiles {
                *tile = self.background_tile(v);
                v = increment_coarse_x(v);
            }
            let fine_x = usize::from(self.fine_x);
            let mut colors = [0u8; WIDTH];
            for (x, color) in colors.iter_mut().enumerate() {
                if x < 8 && !show_left_bg {
                    continue;
                }
                let shifted = x + fine_x;
                let (lo, hi, palette) = tiles[shifted >> 3];
                let bit = 7 - (shifted & 7);
                let value = ((lo >> bit) & 1) | (((hi >> bit) & 1) << 1);
                if value != 0 {
                    *color = palette * 4 + value;
                }
            }
            colors
        } else {
            [0u8; WIDTH]
        };

        let mut sprite_layer = [0u8; WIDTH];
        let mut sprite_priority = [0u8; WIDTH];
        let mut sprite_opaque = [false; WIDTH];
        if sprites_enabled {
            let height = if self.ctrl & 0x20 != 0 { 16 } else { 8 };
            let mut drawn = 0usize;
            for sprite in 0..64 {
                let base = sprite * 4;
                // Hardware sprites are one scanline below their OAM Y value.
                let top = i16::from(self.oam[base]) + 1;
                let row = y as i16 - top;
                if !(0..height as i16).contains(&row) {
                    continue;
                }
                // Only the first eight sprites in OAM order are evaluated.
                // The ninth sets the (simplified) overflow flag, like hardware
                // does for the common non-buggy case.
                drawn += 1;
                if drawn > 8 {
                    self.status |= 0x20;
                    break;
                }

                let attributes = self.oam[base + 2];
                let palette = usize::from(attributes & 0x03);
                let behind = attributes & 0x20 != 0;
                let flip_h = attributes & 0x40 != 0;
                let flip_v = attributes & 0x80 != 0;
                let row = if flip_v {
                    height - 1 - row as usize
                } else {
                    row as usize
                };

                // In 8x16 mode the pattern table is selected by the sprite's
                // tile bit 0, and PPUCTRL bit 3 has no effect.
                let tile_field = self.oam[base + 1];
                let (tile, pattern_table) = if height == 16 {
                    let mut tile = tile_field & 0xFE;
                    if row >= 8 {
                        tile = tile.wrapping_add(1);
                    }
                    let table = if tile_field & 0x01 != 0 { 0x1000 } else { 0 };
                    (tile, table)
                } else {
                    let table = if self.ctrl & 0x08 != 0 { 0x1000 } else { 0 };
                    (tile_field, table)
                };

                let pattern = pattern_table
                    + usize::from(tile) * 16
                    + (row & 7)
                    + usize::from(self.chr_bank) * 0x2000;
                if pattern + 8 > self.chr.len() {
                    continue;
                }
                let lo = self.chr[pattern];
                let hi = self.chr[pattern + 8];
                let left = i16::from(self.oam[base + 3]);
                for px in 0..8usize {
                    let x = left + px as i16;
                    if !(0..WIDTH as i16).contains(&x) {
                        continue;
                    }
                    let x = x as usize;
                    if x < 8 && !show_left_sprite {
                        continue;
                    }
                    let column = if flip_h { 7 - px } else { px };
                    let bit = 7 - column;
                    let value = ((lo >> bit) & 1) | (((hi >> bit) & 1) << 1);
                    if value == 0 {
                        continue;
                    }

                    // Sprite-0 hit needs an opaque sprite-0 pixel over an
                    // opaque background pixel, and is suppressed in the left
                    // eight pixels unless both left-clip bits are set.
                    if sprite == 0
                        && background[x] != 0
                        && (x >= 8 || (show_left_bg && show_left_sprite))
                    {
                        self.status |= 0x40;
                        if self.sprite_zero_hit.is_none() {
                            self.sprite_zero_hit = Some(SpriteZeroHit {
                                line: y,
                                // Pixels are output on dots 1-256.
                                dot: x as u16 + 1,
                            });
                        }
                    }

                    // Lower OAM indices have priority, so never let a later
                    // sprite overwrite a pixel an earlier one already drew.
                    if sprite_opaque[x] {
                        continue;
                    }
                    let index = usize::from(self.palette[16 + palette * 4 + value as usize] & 0x3F);
                    sprite_layer[x] = index as u8;
                    sprite_priority[x] = u8::from(behind);
                    sprite_opaque[x] = true;
                }
            }
        }

        for (x, pixel) in line.chunks_exact_mut(4).enumerate() {
            let bg_index = if background[x] == 0 {
                0xFF
            } else {
                usize::from(self.palette[usize::from(background[x]) & 0x1F] & 0x3F)
            };
            let color_index = if sprite_opaque[x] && (background[x] == 0 || sprite_priority[x] == 0)
            {
                usize::from(sprite_layer[x] & 0x3F)
            } else if bg_index != 0xFF {
                bg_index
            } else {
                usize::from(self.palette[0] & 0x3F)
            };
            let rgb = NES_MASTER_PALETTE[color_index & 0x3F];
            pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 0xFF]);
        }

        // Advance the vertical part of the background address as the real PPU
        // does at dot 256 of each rendered line.
        if bg_enabled || sprites_enabled {
            self.v = increment_y(self.v);
        }
    }

    /// Fetch pattern bytes and palette for the tile addressed by `v`.
    pub(super) fn background_tile(&self, v: u16) -> (u8, u8, u8) {
        let tile = self.read_vram(0x2000 | (v & 0x0FFF));
        let attribute_address = 0x23C0 | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
        let attribute = self.read_vram(attribute_address);
        let shift = ((v >> 4) & 0x04) | (v & 0x02);
        let palette = (attribute >> shift) & 0x03;
        let fine_y = usize::from((v >> 12) & 0x07);
        let pattern_table: u16 = if self.ctrl & 0x10 != 0 { 0x1000 } else { 0 };
        let pattern_addr = pattern_table + u16::from(tile) * 16 + fine_y as u16;
        (
            self.read_pattern(pattern_addr),
            self.read_pattern(pattern_addr + 8),
            palette,
        )
    }
}

pub(super) fn increment_coarse_x(v: u16) -> u16 {
    if v & 0x001F == 31 {
        (v & !0x001F) ^ 0x0400
    } else {
        v + 1
    }
}

pub(super) fn increment_y(v: u16) -> u16 {
    if v & 0x7000 != 0x7000 {
        v + 0x1000
    } else {
        let mut v = v & !0x7000;
        let mut coarse_y = (v >> 5) & 0x1F;
        if coarse_y == 29 {
            coarse_y = 0;
            v ^= 0x0800;
        } else if coarse_y == 31 {
            coarse_y = 0;
        } else {
            coarse_y += 1;
        }
        (v & !0x03E0) | (coarse_y << 5)
    }
}
