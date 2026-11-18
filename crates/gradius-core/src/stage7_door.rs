//! `$B563`'s door: the stage-7 brain-chamber gate.
//!
//! When the `$1E` wall reaches X `$B0` the scroll freezes and the wall paints
//! the door of the brain chamber into nametable 1, four cells per row, top and
//! bottom toward the middle (`$B600` rows `$2578`, `$2618`, `$2598`, `$25F8`,
//! `$25B8`, `$25D8`). One paint lands every `$14` frames; `$046C` counts them
//! and the wall despawns at 7. `$B5C2` picks the tile pair from the counter's
//! parity (`$C3`/`$C2`, then the `+2` partner `$C5`/`$C4`) and `$B597` writes
//! `$FF`, `$C3`, `$FF` to the packed collision map's groups 2-4 on the even
//! steps, so a painted row is solid.
//!
//! The wide stage map holds the open corridor (the extractor drops the wall's
//! writes), so the door is drawn and probed from [`ROWS`]/[`COLUMNS`] alone.
//! Measured on `Gradius (E)`: the wall spawns at 5801, crosses `$B0` at 5928,
//! the first paint lands on 5949 and the last closes the corridor at 6049.

/// `$19` of the stage the wall belongs to.
pub const SECTION: u8 = 6;
/// The six door rows, top to bottom (`$B600` halves the pairs).
pub const ROWS: [u8; 6] = [11, 12, 13, 14, 15, 16];
/// The four door cells of each row as world columns (`$2578 & $1F` = 24 at
/// `$3F = $0B` -> 352 + 24).
pub const COLUMNS: [u16; 4] = [376, 377, 378, 379];
/// Paints the wall makes before it despawns (`$046C` = 1..6).
pub const STEPS: u8 = 6;

/// Whether `(column, row)` is one of the door's cells.
pub fn covers(column: usize, row: usize) -> bool {
    COLUMNS.contains(&(column as u16)) && ROWS.contains(&(row as u8))
}

/// `$B5C2`: the tile `row` shows after `step` paints, `None` while it is open.
/// The pair index is the row's distance from the outside; the odd paint is the
/// `$C5`/`$C3` stripe and the even one the `$C4`/`$C2` stripe.
pub fn tile(step: u8, row: u8) -> Option<u8> {
    let distance = row.checked_sub(ROWS[0])?;
    let pair = if distance < 3 { distance } else { 5 - distance };
    if pair > 2 || step < 2 * pair + 1 {
        return None;
    }
    let top = row < ROWS[3];
    Some(if step == 2 * pair + 1 {
        if top {
            0xC5
        } else {
            0xC3
        }
    } else if top {
        0xC4
    } else {
        0xC2
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_door_rows_pair_from_the_outside_in() {
        let painted = |step: u8| -> Vec<(u8, u8)> {
            let mut rows = Vec::new();
            for row in ROWS {
                if let Some(tile) = tile(step, row) {
                    rows.push((row, tile));
                }
            }
            rows
        };
        assert_eq!(painted(0), vec![]);
        assert_eq!(painted(1), vec![(11, 0xC5), (16, 0xC3)]);
        assert_eq!(painted(2), vec![(11, 0xC4), (16, 0xC2)]);
        assert_eq!(
            painted(3),
            vec![(11, 0xC4), (12, 0xC5), (15, 0xC3), (16, 0xC2)]
        );
        assert_eq!(
            painted(STEPS),
            vec![
                (11, 0xC4),
                (12, 0xC4),
                (13, 0xC4),
                (14, 0xC2),
                (15, 0xC2),
                (16, 0xC2)
            ]
        );
    }

    #[test]
    fn the_extracted_map_keeps_the_corridor_open() {
        // The wall's own writes are dropped, so the map holds the art the
        // background stream left in the door cells (measured on the oracle
        // before the wall reached `$B0`).
        let screen = crate::stage_bg::background(crate::rom::testing::rom(), usize::from(SECTION))
            .expect("stage 7 background");
        let open = [
            [0x00, 0x00, 0x00, 0x00],
            [0x22, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00],
            [0x22, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00],
        ];
        for (offset, row) in ROWS.into_iter().enumerate() {
            for (index, column) in COLUMNS.into_iter().enumerate() {
                assert!(covers(usize::from(column), usize::from(row)));
                assert_eq!(
                    screen.tile(usize::from(column), usize::from(row)),
                    open[offset][index],
                    "the extracted map keeps the corridor open at ({column}, {row})"
                );
            }
        }
        assert!(!covers(375, 12), "column 375 is the wall, not the door");
        assert!(!covers(376, 10), "row 10 is the wall, not the door");
    }
}
