//! `$C9B4`/`$CDAD`: world-keyed nametable cells drawn over the static map.
//!
//! Moved from `render.rs` to keep that file under the line limit. The overlay
//! applies the stage-3 turret paint and stage-6's `$CD9F` erase cells; a tile 0
//! cell blanks the map tile instead of drawing a transparent blank over it.

use super::*;

/// One bit per row for each world column of a tile 0 (`$CDAD` erase) cell.
///
/// The erased nametable cell becomes transparent, so the static map must not
/// draw it: the star tiles and the behind-priority ship show through, as on
/// the PPU. A black rectangle would cover them instead.
pub(super) fn erased_mask(cells: &[stage3_enemies::Cell], width_tiles: usize) -> Vec<u32> {
    if !cells.iter().any(|cell| cell.tile == 0) {
        return Vec::new();
    }
    let mut mask = vec![0u32; width_tiles];
    for cell in cells {
        if cell.tile != 0 {
            continue;
        }
        if let Some(word) = mask.get_mut(usize::from(cell.world_col)) {
            *word |= 1 << cell.row;
        }
    }
    mask
}

/// Apply the turret's world-keyed nametable cells over the static screen.
pub(super) fn draw_cell_overlay(
    fb: &mut Framebuffer,
    assets: &Assets,
    screen: &stage_bg::Screen,
    scroll_px: i32,
    cells: &[stage3_enemies::Cell],
    rows: usize,
    tail: Option<usize>,
) {
    if screen.width_tiles == 0 || screen.tiles.is_empty() {
        return;
    }
    let scroll_px = scroll_px.max(0);
    let rows = rows.min(screen.height_tiles);
    for cell in cells {
        if usize::from(cell.row) >= rows || usize::from(cell.world_col) >= screen.width_tiles {
            continue;
        }
        // `$CD9F`'s erase cells are tile 0: `erased_mask` already kept the map
        // from drawing them, so there is no tile to paint here.
        if cell.tile == 0 {
            continue;
        }
        let x = i32::from(cell.world_col) * 8 - scroll_px;
        if x <= -8 || x >= WIDTH as i32 {
            continue;
        }
        draw_tile(
            fb,
            assets,
            screen,
            usize::from(cell.world_col) % screen.width_tiles,
            usize::from(cell.row),
            cell.tile,
            x,
            i32::from(cell.row) * 8 + STAGE_MAP_Y,
            tail,
        );
    }
}
