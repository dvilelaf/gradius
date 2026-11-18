//! `$CE2B`: the 88-entry wall-erase table stage 6's `$CD9F` walks during the
//! `$86` end window.
//!
//! The bytes are the pack's `STAGE_WALL_ERASE` chunk (copied from PRG `$CE2B`),
//! read through `Assets::stage6_wall_erase`. Each entry decodes to one
//! nametable cell in [`cell`], and `$CDAD` erases two cells per frame (44
//! frames, `$66` 0 -> 88) before the `$0C` stage end.

/// `$CDAD`: `addr = $2400 + ((tile & $F0) << 1) + ($F0 | (tile & $0F))`.
/// The `$F0` offset is row 7, column 16, so the cell is
/// `(tile >> 4) + 7, (tile & $0F) + 16` on the 32x30 nametable.
pub fn cell(entry: u8) -> (u8, u8) {
    ((entry >> 4) + 7, (entry & 0x0F) + 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_decodes_to_the_documented_diamond() {
        let table = crate::assets::pack_stage_assets().wall_erase.unwrap();
        assert_eq!(table.len(), 88);
        let cells: Vec<(u8, u8)> = table.iter().map(|&entry| cell(entry)).collect();
        // 16 rows x 11 columns with four end-wall cells written twice.
        for &(row, column) in &cells {
            assert!((7..=22).contains(&row), "row {row}");
            assert!((21..=31).contains(&column), "column {column}");
        }
        let mut unique = cells.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 84);
        assert_eq!(
            cells[..4],
            [(15, 22), (16, 26), (14, 28), (11, 26)],
            "the first four `$CE2B` entries"
        );
    }
}
