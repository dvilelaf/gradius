//! Render the sprite-group tables as a labelled sheet. Decoding lives in
//! `gradius-extract`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use gradius_cart::{Cartridge, Tile};

use crate::pngout;

pub use gradius_extract::sprite_groups::*;

fn glyph(c: char) -> Option<[u8; 5]> {
    Some(match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        '#' => [0b101, 0b111, 0b101, 0b111, 0b101],
        ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        ' ' => [0; 5],
        _ => return None,
    })
}

fn draw_text(rgb: &mut [u8], width: u32, x: u32, y: u32, scale: u32, text: &str, color: [u8; 3]) {
    let mut cursor = x;
    for c in text.chars() {
        if c == ' ' {
            cursor += 4 * scale;
            continue;
        }
        if let Some(rows) = glyph(c) {
            for (row, bits) in rows.iter().enumerate() {
                for col in 0..3 {
                    if bits & (1 << (2 - col)) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = cursor + col * scale + dx;
                            let py = y + row as u32 * scale + dy;
                            let offset = ((py * width + px) * 3) as usize;
                            rgb[offset..offset + 3].copy_from_slice(&color);
                        }
                    }
                }
            }
        }
        cursor += 4 * scale;
    }
}

/// Render every group into a sheet: one row per group, one column per CHR bank.
pub fn render_sheet(
    banks: &[[Tile; 256]],
    groups: &[SpriteGroup],
    max_sprites: usize,
) -> (u32, u32, Vec<u8>) {
    const SCALE: u32 = 2;
    const SPRITE_W: u32 = 8 * SCALE;
    const SPRITE_H: u32 = 16 * SCALE;
    const GAP: u32 = 2;
    const MARGIN: u32 = 6;
    const LABEL: u32 = 68;

    let cell_w = max_sprites.max(1) as u32 * (SPRITE_W + GAP);
    let row_h = SPRITE_H + MARGIN;
    let width = MARGIN + LABEL + banks.len() as u32 * cell_w + MARGIN;
    let height = MARGIN + row_h + groups.len() as u32 * row_h + MARGIN;
    let mut rgb = vec![0x18u8; (width * height * 3) as usize];

    let ink = [0xF0u8, 0xF0, 0xF0];
    let shades = [
        [0x00u8, 0x00, 0x00],
        [0x50, 0x60, 0x80],
        [0x90, 0xB0, 0xD0],
        [0xF0, 0xF0, 0xC0],
    ];

    // Bank headers.
    for (bank, _) in banks.iter().enumerate() {
        let x = MARGIN + LABEL + bank as u32 * cell_w + 2;
        draw_text(&mut rgb, width, x, 2, SCALE, &format!("B{bank}"), ink);
    }

    for (row, group) in groups.iter().enumerate() {
        let y = MARGIN + row_h + row as u32 * row_h;
        let label = format!("#{}:{} {}", group.table, group.index, group.count());
        draw_text(&mut rgb, width, MARGIN, y + 4, SCALE, &label, ink);

        for (bank, tiles) in banks.iter().enumerate() {
            let cell_x = MARGIN + LABEL + bank as u32 * cell_w;
            for (slot, sprite) in group.sprites.iter().enumerate() {
                let origin_x = cell_x + slot as u32 * (SPRITE_W + GAP);
                let top = sprite.tile & 0xFE;
                let flip_v = sprite.attributes & 0x80 != 0;
                let flip_h = sprite.attributes & 0x40 != 0;
                for half in 0..2u32 {
                    let tile_index =
                        top.wrapping_add(if flip_v { 1 - half as u8 } else { half as u8 });
                    let tile = &tiles[tile_index as usize];
                    for py in 0..SPRITE_H / 2 {
                        for px in 0..SPRITE_W {
                            let mut tx = (px / SCALE) as usize;
                            let ty = (py / SCALE) as usize;
                            if flip_h {
                                tx = 7 - tx;
                            }
                            let pixel = tile.pixels[ty * 8 + tx];
                            // Color 0 is transparent; tint the rest by bank.
                            if pixel == 0 {
                                continue;
                            }
                            let color = shades[(pixel as usize + bank) % 4];
                            let ox = origin_x + px;
                            let oy = y + half * SPRITE_H / 2 + py;
                            let offset = ((oy * width + ox) * 3) as usize;
                            rgb[offset..offset + 3].copy_from_slice(&color);
                        }
                    }
                }
            }
        }
    }
    (width, height, rgb)
}

/// Serialise the decoded groups as JSON (hand-rolled: no serde dependency).
pub fn to_json(groups: &[SpriteGroup]) -> String {
    let mut out = String::from("{\n  \"groups\": [\n");
    for (i, group) in groups.iter().enumerate() {
        out.push_str(&format!(
            "    {{\"table\": {}, \"index\": {}, \"address\": \"${:04X}\", \"count\": {}, \"sprites\": [",
            group.table,
            group.index,
            group.address,
            group.count()
        ));
        for (j, sprite) in group.sprites.iter().enumerate() {
            if j > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!(
                "{{\"y\": {}, \"tile\": {}, \"attributes\": {}, \"x\": {}}}",
                sprite.y, sprite.tile, sprite.attributes, sprite.x
            ));
        }
        out.push_str("]}");
        if i + 1 < groups.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

pub fn run(rom: &Path, out: &Path, json: Option<&Path>) -> Result<()> {
    let data = fs::read(rom).with_context(|| format!("reading {}", rom.display()))?;
    let cart = Cartridge::from_ines(&data)?;
    let groups = decode_all(&cart);
    let banks = sprite_banks(&cart);
    let max_sprites = groups.iter().map(SpriteGroup::count).max().unwrap_or(1);
    let (width, height, rgb) = render_sheet(&banks, &groups, max_sprites);
    pngout::write_rgb(out, width, height, &rgb)?;
    if let Some(path) = json {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, to_json(&groups)).with_context(|| format!("writing {}", path.display()))?;
    }
    let drawn = groups.iter().filter(|group| group.count() > 0).count();
    println!(
        "wrote {} ({} x {}, {} groups, {} non-empty, {} CHR banks)",
        out.display(),
        width,
        height,
        groups.len(),
        drawn,
        banks.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_lists_table_index_address_and_sprites() {
        let mut prg = vec![0u8; 0x8000];
        let def = 0x9000usize - 0x8000;
        prg[def] = 1;
        prg[def + 1..def + 5].copy_from_slice(&[0x00, 0x65, 0x22, 0x00]);
        let group = SpriteGroup {
            table: 1,
            index: 9,
            address: 0x9000,
            sprites: decode_group(&prg, 0x9000).unwrap(),
        };
        let json = to_json(&[group]);
        assert!(json.contains("\"index\": 9"));
        assert!(json.contains("\"address\": \"$9000\""));
        assert!(json.contains("\"tile\": 101"));
        assert!(json.contains("\"count\": 1"));
    }
}
