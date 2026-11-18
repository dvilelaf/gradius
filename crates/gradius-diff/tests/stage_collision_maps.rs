//! Cross-check the pack's stage 2-6 collision maps against the oracle's
//! nametable attributes.
//!
//! The packed `$0500`/`$0600` map is not stored in the cartridge as a table:
//! the oracle fills it while the stage scrolls. The visible nametable is the
//! independent record of where the stage paints structure. For every solid
//! cell (`2-bit field != 0`) the corresponding attribute quadrant must select
//! palette 2 or 3; palettes 0 and 1 are the starfield/backdrop. The converse
//! does not hold (palette 2/3 also decorates non-solid art), so this is a
//! necessary-condition check, not a reconstruction of the map.
//!
//! Stage 5 is open starfield: its nametable has no structural block at all,
//! so the original never writes a collision byte and the map must be zero.

use gradius_core::stage_bg::Screen;

fn map(section: usize) -> Vec<u8> {
    let bytes = gradius_diff::testdata::assets()
        .rom()
        .expect("pack ROM data")
        .collision
        .maps[section]
        .clone();
    assert_eq!(bytes.len(), 448 * 8);
    bytes
}

fn background(section: usize) -> &'static Screen {
    &gradius_diff::testdata::assets()
        .rom()
        .expect("pack ROM data")
        .backgrounds
        .stages[match section {
        1..=4 => section,
        _ => 5,
    }]
}

/// Attribute quadrant value for world tile column/row.
fn quadrant(screen: &Screen, column: usize, row: usize) -> u8 {
    let attributes_per_row = screen.width_tiles / 4;
    let byte = screen.attributes[(row / 4) * attributes_per_row + column / 4];
    let quadrant = ((row % 4) / 2) * 2 + (column % 4) / 2;
    (byte >> (quadrant * 2)) & 0x03
}

fn solid(map: &[u8], column: usize, row: usize) -> bool {
    let byte = map[column * 8 + row / 4];
    (byte >> (2 * (row & 3))) & 0x03 != 0
}

/// `(solid cells, solid cells on palette 2/3, cells in blocks 12-13)`.
fn counts(section: usize) -> (usize, usize, usize) {
    let screen = background(section);
    let map = map(section);
    let columns = screen.width_tiles.min(448);
    let (mut solid_cells, mut backed, mut tail) = (0, 0, 0);
    for column in 0..columns {
        for row in 0..screen.height_tiles {
            if solid(&map, column, row) {
                solid_cells += 1;
                backed += usize::from(quadrant(screen, column, row) >= 2);
                tail += usize::from(column >= 384);
            }
        }
    }
    (solid_cells, backed, tail)
}

#[test]
fn phases_2_4_and_6_only_mark_cells_the_nametable_marks() {
    for section in [1, 2, 3, 5] {
        let (solid_cells, backed, _) = counts(section);
        assert!(solid_cells > 0, "stage {} has no solid cells", section + 1);
        assert_eq!(
            solid_cells,
            backed,
            "stage {} marks {} solid cells but only {} sit on nametable palettes 2/3",
            section + 1,
            solid_cells,
            backed
        );
    }
}

#[test]
fn the_last_two_blocks_carry_no_residue() {
    // Phase 4's block 12 is real terrain; every other phase ends in open
    // starfield, and no phase has solid cells past column 416.
    for section in [1, 2, 4, 5] {
        assert_eq!(counts(section).2, 0, "stage {} tail", section + 1);
    }
    let (solid_cells, backed, tail) = counts(3);
    assert!(solid_cells > 0 && backed == solid_cells);
    assert_eq!(tail, 162, "phase 4 block 12 is the only tail terrain");
}

#[test]
fn phase_5_is_open_starfield_and_its_map_is_zero() {
    let screen = background(4);
    // The oracle's stage-5 nametable is the same starfield block 14 times:
    // palette counts match in every 32-column block, with no structural fill.
    let palette_counts = |block: usize| {
        let mut counts = [0usize; 4];
        for column in block * 32..(block + 1) * 32 {
            for row in 0..screen.height_tiles {
                counts[usize::from(quadrant(screen, column, row))] += 1;
            }
        }
        counts
    };
    for block in 1..14 {
        assert_eq!(
            palette_counts(block),
            palette_counts(0),
            "stage-5 attribute block {block} is not the starfield pattern"
        );
    }

    // The original never reloads `$0500`/`$0600` for this stage, so nothing
    // can be solid. A non-zero cell is stale recycled-window data: the 627
    // cells once checked in all sat on the odd page with identical support,
    // which no real terrain repeats.
    let map = map(4);
    assert!(
        map.iter().all(|byte| *byte == 0),
        "stage 5 has no terrain: the map must be zero"
    );
    assert_eq!(counts(4), (0, 0, 0));
}
