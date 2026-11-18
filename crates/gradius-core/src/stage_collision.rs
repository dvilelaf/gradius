//! Collision map geometry. The maps and terrain records live in
//! `rom.collision` (the `COLL` section of `CHUNK_ROM`).
//!
//! `$C3CD` reads the byte at `($0500 + page*$100) + column*8 + row/4` and
//! masks it with `$C409[row & 3]`; a non-zero field means solid. The entry for
//! absolute tile column `c` and group `g` is `map[c*8 + g]`.

/// Collidable columns (`$98F0 = $0E` -> 3584 px / 8).
pub const COLUMNS: usize = 448;
/// Four-tile row groups per column (32 rows / 4).
pub const GROUPS: usize = 8;

/// `$C3CD`'s packed map for `$19 = section`; sections past the last reuse it.
pub fn map(rom: &crate::rom::RomData, section: usize) -> &[u8] {
    let maps = &rom.collision.maps;
    &maps[section.min(maps.len() - 1)]
}

/// `$C3CD`: the world tile `(column, row)` a screen pixel probes and its 2-bit
/// map field (`$C409` mask, shifted down). Field 2 is a destructible cell
/// (`$C2D2`); any other non-zero field is solid for good.
pub(crate) fn terrain_probe_in(
    map: &[u8],
    x: i32,
    y: i32,
    scroll: crate::stage::Scroll,
) -> Option<(usize, usize, u8)> {
    let probe_x = x + 8 + scroll.position() as i32;
    if probe_x < 0 {
        return None;
    }
    let column = (probe_x as usize / 8) % COLUMNS;
    let row = (y + 0x14).max(0) as usize / 8;
    let group = row / 4;
    if group >= GROUPS {
        return None;
    }
    let byte = map[column * GROUPS + group];
    Some((column, row, (byte >> (2 * (row & 3))) & 0x03))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::testing::rom;

    #[test]
    fn every_stage_map_has_the_documented_shape() {
        for section in 0..7 {
            assert_eq!(map(rom(), section).len(), COLUMNS * GROUPS);
        }
        for section in [0, 1, 2, 3, 5, 6] {
            assert!(
                map(rom(), section).iter().any(|byte| *byte != 0),
                "stage {} map is empty",
                section + 1
            );
            if section != 0 {
                assert_ne!(
                    map(rom(), section),
                    map(rom(), 0),
                    "stage {} must not reuse the stage-1 map",
                    section + 1
                );
            }
        }
    }

    /// Stage 5 is open starfield, so the original never writes a `$0500`/`$0600`
    /// byte there and the map is zero. A non-zero cell would be recycled-window
    /// residue, not ROM terrain.
    #[test]
    fn stage_5_has_no_solid_cells() {
        assert!(
            map(rom(), 4).iter().all(|byte| *byte == 0),
            "stage 5 has no background terrain; the first solid cell is residue"
        );
    }

    /// The opening starfield paints exactly eight palette-2/3 attribute cells
    /// in every 32-column block and no collision terrain (measured on stages
    /// 2-7). A block with more structural cells than that is terrain the
    /// oracle must have written into `$0500`/`$0600`.
    const STARFIELD_PALETTE_CELLS: usize = 8;

    /// Palette-2/3 cells of `screen` in the 32-column block `block`.
    fn structural_cells(screen: &crate::stage_bg::Screen, block: usize) -> usize {
        let attributes_per_row = screen.width_tiles / 4;
        let end = (block * 32 + 32).min(screen.width_tiles);
        let mut cells = 0;
        for column in block * 32..end {
            for row in 0..screen.height_tiles {
                let byte = screen.attributes[(row / 4) * attributes_per_row + column / 4];
                let quadrant = ((row % 4) / 2) * 2 + (column % 4) / 2;
                if (byte >> (quadrant * 2)) & 3 >= 2 {
                    cells += 1;
                }
            }
        }
        cells
    }

    /// Stage 7's block 11 (world columns 352-383) was captured before the
    /// `$0600` page was reloaded, so the whole block was zero while the oracle
    /// has terrain. At frame 5627 `$C3CD` reads `$0601 = $15` for the ceiling
    /// walker and `$0605 = $40` for the ground walker; the port's walkers
    /// climbed and fell from there.
    #[test]
    fn stage_7_block_11_keeps_the_reloaded_page() {
        let map = map(rom(), 6);
        assert_eq!(map[352 * 8 + 1], 0x15, "stage 7 $0601");
        assert_eq!(map[352 * 8 + 5], 0x40, "stage 7 $0605");
        assert!(
            map[352 * 8..384 * 8].iter().any(|byte| *byte != 0),
            "stage 7 block 11 is empty"
        );
    }

    /// Column 64 is the first stage-7 column with terrain; 65 group 1 is the
    /// face the frame-876 shot hits (`$0363 = 227`, scroll `1:$22`).
    #[test]
    fn stage_7_map_keeps_its_first_terrain() {
        let map = map(rom(), 6);
        assert_eq!(
            &map[64 * GROUPS..64 * GROUPS + GROUPS],
            &[0, 0, 0, 0, 0, 0x40, 0x55, 0]
        );
        assert_eq!(map[65 * GROUPS + 1], 0x01);
    }

    /// Every block the stage paints structure into must carry collision bytes.
    /// The 32-column block size is the `$0500`/`$0600` page: a zeroed block
    /// with nametable terrain is a pre-reload extraction residue, the defect
    /// block 11 of stage 7 carried.
    #[test]
    fn no_terrain_block_is_empty() {
        for section in 1..=6 {
            let info = &crate::stage_catalog::STAGES[section];
            let screen = crate::stage_bg::background(rom(), usize::from(info.index))
                .expect("extracted stage background");
            let (map, threshold) = (
                map(rom(), section),
                usize::from(rom().collision.threshold[section]),
            );
            assert!(
                threshold * 32 <= COLUMNS,
                "stage {} boss trigger is outside the map",
                section + 1
            );
            for block in 0..threshold {
                if structural_cells(screen, block) <= STARFIELD_PALETTE_CELLS {
                    continue;
                }
                assert!(
                    map[block * 256..(block + 1) * 256]
                        .iter()
                        .any(|byte| *byte != 0),
                    "stage {} block {block} has nametable terrain but an empty collision block",
                    section + 1
                );
            }
        }
    }

    #[test]
    fn thresholds_and_terrain_kinds_are_the_section_streams() {
        assert_eq!(rom().collision.threshold, [12, 12, 12, 12, 11, 11, 12]);
        // `$A32F`: `type < $80` selects a `$A65C` record, `>= $80` an enemy.
        for (index, records) in rom().collision.terrain.iter().enumerate() {
            for piece in records {
                assert!(
                    piece.kind < 0x80,
                    "section {index} kind ${:02X}",
                    piece.kind
                );
            }
        }
    }
}
