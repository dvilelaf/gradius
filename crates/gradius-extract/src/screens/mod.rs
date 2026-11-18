//! Reconstruct full screens from the oracle's PPU VRAM write log.
//!
//! The game never stores a complete background in VRAM: with vertical
//! mirroring only two 32x30 nametables exist, and the stage is drawn one
//! 8-pixel column at a time as the scroll advances. `xtask screens` runs the
//! oracle with VRAM write capture enabled, tags every nametable write with the
//! scroll position at the frame it happened, and folds the writes into a wide
//! world map.
//!
//! Column attribution
//! ------------------
//! The PPU renders world pixel `S + x` at screen x, where `S = $3E + 256*$3F`
//! is the 16-bit horizontal scroll. That means the tile covering world column
//! `k` is fetched from physical nametable `(k >> 5) & 1`, column `k & 31`
//! (vertical mirroring exposes exactly those two physical tables). So a write
//! to `(table, column)` belongs to the smallest world column `k >= S >> 3`
//! with that same `(table, column)` pair. Attribute bytes cover 4x4 tiles, so
//! they use the same rule in units of four columns.
//!
//! Nothing here is cartridge data: the output is decoded nametable and
//! attribute bytes as the PPU displayed them.

mod builder;
mod encode;
mod pack;

pub use builder::{capture, needs_stage_poke, Builder, TerrainCheck, TerrainSpawn};
pub use encode::{classify_solid, hex};
pub use pack::capture_backgrounds;

/// Full nametable height in tiles.
pub const HEIGHT_TILES: usize = 30;

/// Visible playfield height during stage 1.
///
/// The HUD split at `$9A96`-`$9AAF` polls sprite 0 (the power-meter bar at
/// y = 207) and then reloads scroll (0,0) for the rest of the frame, so
/// scanlines 208-239 always come from nametable rows 28-29 at scroll 0. Those
/// two rows are the fixed HUD band, which the port draws separately. The
/// playfield window (screen rows 0-207) stretches from nametable row 1 down to
/// row 27, so 28 rows of scrolling background are stored.
pub const STAGE_HEIGHT_TILES: usize = 28;

/// One reconstructed screen.
#[derive(Clone, Debug)]
pub struct ScreenData {
    pub width_tiles: usize,
    pub height_tiles: usize,
    /// Row-major tile indices.
    pub tiles: Vec<u8>,
    /// Attribute table, one byte per 4x4-tile block, row-major.
    pub attributes: Vec<u8>,
    /// Palette RAM as displayed.
    pub palette: [u8; 32],
    /// Hardware CHR bank active when the screen was shown.
    pub chr_bank: u8,
    /// Tiles that no captured write covered (filled with 0 in `tiles`).
    pub missing_tiles: usize,
    /// Attribute bytes that no captured write covered.
    pub missing_attributes: usize,
}

impl ScreenData {
    pub fn fnv_tiles(&self) -> u64 {
        crate::fnv1a(&self.tiles)
    }

    pub fn fnv_attributes(&self) -> u64 {
        crate::fnv1a(&self.attributes)
    }

    /// `(tile, count)` sorted by descending count, for terrain analysis.
    pub fn tile_histogram(&self) -> Vec<(u8, usize)> {
        let mut counts = [0usize; 256];
        for tile in &self.tiles {
            counts[usize::from(*tile)] += 1;
        }
        let mut ranked: Vec<(u8, usize)> = counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(tile, count)| (tile as u8, *count))
            .collect();
        ranked.sort_by_key(|(tile, count)| (std::cmp::Reverse(*count), *tile));
        ranked
    }
}

/// Physical VRAM offset for a `$2000-$2FFF` nametable address.
///
/// Vertical mirroring exposes table 0/2 as physical table 0 and 1/3 as table
/// 1, so the physical table is bit 0 of the nametable index.
fn physical_index(addr: u16) -> usize {
    usize::from((addr >> 10) & 1) * 0x400 + usize::from(addr & 0x03FF)
}

/// Physical nametable slot that displays world tile column `k`.
///
/// Vertical mirroring means world columns 0-31 live in physical table 0,
/// 32-63 in table 1, 64-95 in table 0 again, and so on. The PPU derives the
/// same mapping from `$3F` bit 0 (the game folds it into PPUCTRL), which is
/// why `k >> 5 & 1` is the table select.
pub fn visible_slot(k: usize) -> (usize, usize) {
    ((k >> 5) & 1, k & 31)
}

/// Physical attribute slot (4x4 tiles) for world tile column `k`.
pub fn visible_attribute_slot(k: usize) -> (usize, usize) {
    let group = k >> 2;
    ((group >> 3) & 1, group & 7)
}

/// Reconstruct one 32x30 screen from a physical 2 KiB VRAM snapshot.
pub fn snapshot_screen(
    vram: &[u8],
    palette: [u8; 32],
    chr_bank: u8,
    base_table: usize,
) -> ScreenData {
    let table = base_table & 1;
    let tiles = vram[table * 0x400..table * 0x400 + 0x3C0].to_vec();
    let attributes = vram[table * 0x400 + 0x3C0..table * 0x400 + 0x400].to_vec();
    ScreenData {
        width_tiles: 32,
        height_tiles: HEIGHT_TILES,
        tiles,
        attributes,
        palette,
        chr_bank,
        missing_tiles: 0,
        missing_attributes: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_poke_stops_once_the_target_stage_starts() {
        assert!(needs_stage_poke(3, 0, 1));
        assert!(needs_stage_poke(5, 0, 1));
        assert!(!needs_stage_poke(5, 1, 1));
        assert!(!needs_stage_poke(5, 0, 0));
    }

    #[test]
    fn display_changes_track_chr_bank_switches() {
        let mut builder = Builder::new();
        let palette = [0x0F; 32];
        builder.observe_frame(0, palette, 0);
        builder.observe_frame(8, palette, 2);
        assert_eq!(builder.palette_changes.len(), 2);
        assert_eq!(builder.palette_changes[1], (1, palette, 2));
    }

    #[test]
    fn samples_refresh_until_the_scroll_moves_on() {
        let mut builder = Builder::new();
        builder.request_samples(&[512, 0, 512]);
        let title = [0x0F; 32];
        let mut stage = [0x12; 32];
        stage[0] = 0x0F;
        let early = [0xAAu8; 0x800];
        let late = [0x55u8; 0x800];

        // Stage init holds scroll 0 while it builds the background, so the
        // request at 0 picks up the last frame at that scroll value rather
        // than the first.
        builder.observe_frame(0, title, 0);
        builder.take_samples(&early, 0, title, 0);
        assert_eq!(builder.samples.len(), 1);
        builder.observe_frame(0, stage, 0);
        builder.take_samples(&late, 0, stage, 0);
        assert_eq!(builder.samples.len(), 1);
        assert_eq!(builder.samples[0].vram, late);
        assert_eq!(builder.samples[0].palette, stage);

        // Once the scroll advances, the sample is final.
        builder.observe_frame(1, stage, 0);
        builder.take_samples(&early, 1, stage, 0);
        assert_eq!(builder.samples.len(), 1);
        assert_eq!(builder.samples[0].vram, late);

        // The deduplicated 512 request fires at the first scroll past it.
        builder.observe_frame(512, stage, 0);
        builder.take_samples(&early, 512, stage, 0);
        assert_eq!(builder.samples.len(), 2);
        assert_eq!(builder.samples[1].requested, 512);
        assert_eq!(builder.samples[1].scroll, 512);
    }

    fn terrain(delay: u8, kind: u8) -> crate::sections::Record {
        crate::sections::Record::Spawn { delay, kind }
    }

    #[test]
    fn spawner_trace_records_terrain_and_checks_positions() {
        let mut prg = vec![0u8; 0x8000];
        // $8100: delay $10 kind $05 (terrain), then delay $20 kind $82 (enemy).
        prg[0x100..0x104].copy_from_slice(&[0x10, 0x05, 0x20, 0x82]);
        let mut builder = Builder::new();
        builder.trace_spawner();
        let mut ram = [0u8; 0x800];
        ram[0x6A] = 0x00;
        ram[0x6B] = 0x81;
        builder.observe_spawner(&prg, &ram, 0);

        ram[0x6A] = 0x02;
        builder.observe_spawner(&prg, &ram, 32);
        ram[0x6A] = 0x04;
        builder.observe_spawner(&prg, &ram, 64);

        assert_eq!(
            builder.terrain_spawns,
            vec![TerrainSpawn {
                group: 0,
                scroll: 32,
                delay: 0x10,
                kind: 0x05,
            }]
        );
        let columns = vec![vec![terrain(0x10, 0x05), terrain(0x20, 0x82)]];
        assert_eq!(
            builder.check_terrain(&columns, 0x0C),
            TerrainCheck {
                consumed: 1,
                record_mismatches: 0,
                position_mismatches: 0,
                missing: 0,
                max_lag: 0,
            }
        );
    }

    #[test]
    fn terrain_check_flags_wrong_records_positions_and_missing() {
        let mut builder = Builder::new();
        builder.terrain_spawns.push(TerrainSpawn {
            group: 0,
            scroll: 20,
            delay: 0x10,
            kind: 0x06,
        });
        let columns = vec![vec![terrain(0x10, 0x05), terrain(0x18, 0x05)]];
        let check = builder.check_terrain(&columns, 0x0C);
        assert_eq!(check.consumed, 1);
        assert_eq!(check.record_mismatches, 1);
        assert_eq!(check.position_mismatches, 1);
        assert_eq!(check.missing, 1);
    }

    #[test]
    fn visible_column_validation_reports_an_altered_nametable() {
        let mut vram = vec![0u8; 0x800];
        for column in 0..32 {
            for row in 0..HEIGHT_TILES {
                vram[row * 32 + column] = column as u8;
            }
        }
        let mut builder = Builder::new();
        builder.paint_visible(&vram, 0);
        let screen = builder.finish_with_height([0; 32], 0, STAGE_HEIGHT_TILES);
        assert_eq!(builder.validate_visible(&screen, &vram, 0), (0, 0));

        vram[3 * 32 + 5] = 0xFF;
        assert_eq!(builder.validate_visible(&screen, &vram, 0), (1, 0));
    }

    #[test]
    fn visible_slot_matches_the_two_nametable_layout() {
        assert_eq!(visible_slot(0), (0, 0));
        assert_eq!(visible_slot(31), (0, 31));
        assert_eq!(visible_slot(32), (1, 0));
        assert_eq!(visible_slot(63), (1, 31));
        assert_eq!(visible_slot(64), (0, 0));
        assert_eq!(visible_slot(95), (0, 31));
        assert_eq!(visible_slot(96), (1, 0));
    }

    #[test]
    fn attribute_slots_cover_four_tiles() {
        assert_eq!(visible_attribute_slot(0), (0, 0));
        assert_eq!(visible_attribute_slot(3), (0, 0));
        assert_eq!(visible_attribute_slot(4), (0, 1));
        assert_eq!(visible_attribute_slot(31), (0, 7));
        assert_eq!(visible_attribute_slot(32), (1, 0));
        assert_eq!(visible_attribute_slot(64), (0, 0));
    }

    #[test]
    fn painting_visible_columns_follows_the_scroll() {
        let mut vram = vec![0u8; 0x800];
        // Table 0 shows world columns 0-31, table 1 shows 32-63.
        for column in 0..32 {
            for row in 0..HEIGHT_TILES {
                vram[row * 32 + column] = 0x10 + column as u8;
                vram[0x400 + row * 32 + column] = 0x30 + column as u8;
            }
        }
        vram[0x3C0] = 0xE4;
        vram[0x400 + 0x3C0] = 0x1B;

        let mut builder = Builder::new();
        builder.observe_frame(0, [0; 32], 0);
        builder.paint_visible(&vram, 0);
        // Scrolling one tile column to the left brings column 32 on screen.
        builder.paint_visible(&vram, 8);
        let screen = builder.finish_with_height([0; 32], 0, HEIGHT_TILES);
        assert_eq!(screen.width_tiles, 36); // 33 columns rounded up to 4
        assert_eq!(screen.height_tiles, HEIGHT_TILES);
        assert_eq!(screen.tiles[0], 0x10);
        assert_eq!(screen.tiles[32], 0x30);
        assert_eq!(screen.tiles[1], 0x11);
        // Column 32 is the first column of the second physical table.
        assert_eq!(screen.tiles[32], 0x30);
        // Attribute groups: columns 0-3 -> group 0, 32-35 -> group 8.
        assert_eq!(screen.attributes[0], 0xE4);
        assert_eq!(screen.attributes[8], 0x1B);
        // Columns 33-35 were never on screen, so they stay unfilled.
        assert_eq!(screen.missing_tiles, 3 * HEIGHT_TILES);
    }

    #[test]
    fn samples_cover_the_whole_stage_including_the_tail() {
        const WIDTH: usize = 480;
        const HEIGHT: usize = 28;
        let tile = |k: usize, row: usize| (k * 7 + row) as u8;
        let attribute = |group: usize, row: usize| (group as u8) ^ ((row * 0x11) as u8);

        // Paint one 32-column window at a time, as the oracle does while the
        // scroll advances. A physical table slot is reused every 64 columns,
        // but a window is only 32 wide, so each column is captured once.
        let mut vram = vec![0u8; 0x800];
        let mut builder = Builder::new();
        for left in 0..=WIDTH - 32 {
            for k in left..left + 32 {
                let (table, column) = visible_slot(k);
                for row in 0..HEIGHT {
                    vram[table * 0x400 + row * 32 + column] = tile(k, row);
                }
                if k % 4 == 0 {
                    let (attr_table, attr_column) = visible_attribute_slot(k);
                    for row in 0..HEIGHT.div_ceil(4) {
                        vram[attr_table * 0x400 + 0x3C0 + row * 8 + attr_column] =
                            attribute(k / 4, row);
                    }
                }
            }
            builder.paint_visible(&vram, left * 8);
        }
        let screen = builder.finish_with_height([0x0F; 32], 1, HEIGHT);
        assert_eq!(screen.width_tiles, WIDTH);
        assert_eq!(screen.missing_tiles, 0);

        // Every requested scroll, including the stage-3/4 tail boundary and
        // the last full window, must match the live nametable.
        for scroll in [0, 512, 1100, 3328, 3500] {
            let left = scroll / 8;
            let mut live = vec![0u8; 0x800];
            for k in left..(left + 32).min(WIDTH) {
                let (table, column) = visible_slot(k);
                for row in 0..HEIGHT {
                    live[table * 0x400 + row * 32 + column] = tile(k, row);
                }
                let (attr_table, attr_column) = visible_attribute_slot(k);
                for row in 0..HEIGHT.div_ceil(4) {
                    live[attr_table * 0x400 + 0x3C0 + row * 8 + attr_column] =
                        attribute(k / 4, row);
                }
            }
            assert_eq!(
                builder.validate_visible(&screen, &live, scroll),
                (0, 0),
                "scroll {scroll}"
            );
        }
    }

    #[test]
    fn height_truncation_drops_the_hud_band() {
        let mut vram = vec![0u8; 0x800];
        for row in 0..HEIGHT_TILES {
            vram[row * 32] = row as u8;
        }
        let mut builder = Builder::new();
        builder.paint_visible(&vram, 0);
        let screen = builder.finish_with_height([0; 32], 0, STAGE_HEIGHT_TILES);
        assert_eq!(screen.height_tiles, STAGE_HEIGHT_TILES);
        assert_eq!(screen.tiles[27 * screen.width_tiles], 27);
        assert_eq!(screen.attributes.len(), (32 / 4) * 7);
    }

    #[test]
    fn solidity_follows_connected_terrain_only() {
        // A floor blob touching the ground band, plus one isolated twinkle.
        let width = 16;
        let height = 26;
        let mut tiles = vec![0u8; width * height];
        for x in 0..8 {
            for y in 22..height {
                tiles[y * width + x] = 0xDD;
            }
        }
        tiles[5 * width + 12] = 0x3A; // isolated star
        let screen = ScreenData {
            width_tiles: width,
            height_tiles: height,
            tiles,
            attributes: vec![0; (width / 4) * 7],
            palette: [0; 32],
            chr_bank: 0,
            missing_tiles: 0,
            missing_attributes: 0,
        };
        let solid = classify_solid(&screen);
        assert!(solid[0xDD]);
        assert!(!solid[0x3A]);
        assert!(!solid[0]);
    }
}
