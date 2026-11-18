use std::path::Path;

use anyhow::{Context, Result};

use crate::pngout;

use super::ScreenData;

/// Render a screen as a PNG using a CHR bank and palette.
pub fn write_png(
    path: &Path,
    screen: &ScreenData,
    chr: &[u8],
    master: &[[u8; 3]; 64],
) -> Result<()> {
    let width = (screen.width_tiles * 8) as u32;
    let height = (screen.height_tiles * 8) as u32;
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    let bank_offset = usize::from(screen.chr_bank) * 0x2000;
    for row in 0..screen.height_tiles {
        for column in 0..screen.width_tiles {
            let tile = screen.tiles[row * screen.width_tiles + column];
            let attr = screen.attributes[(row / 4) * (screen.width_tiles / 4) + column / 4];
            let quadrant = (row % 4) / 2 * 2 + (column % 4) / 2;
            let palette_index = (attr >> (quadrant * 2)) & 0x03;
            let colors = &screen.palette[usize::from(palette_index) * 4..][..4];
            let tile_offset = bank_offset + usize::from(tile) * 16;
            for y in 0..8 {
                let low = chr.get(tile_offset + y).copied().unwrap_or(0);
                let high = chr.get(tile_offset + y + 8).copied().unwrap_or(0);
                for x in 0..8 {
                    let bit = 7 - x;
                    let pixel = ((low >> bit) & 1) | (((high >> bit) & 1) << 1);
                    let color = master[usize::from(colors[usize::from(pixel)] & 0x3F)];
                    let offset = (((row * 8 + y) * width as usize) + column * 8 + x) * 3;
                    rgb[offset..offset + 3].copy_from_slice(&color);
                }
            }
        }
    }
    pngout::write_rgb(path, width, height, &rgb)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
