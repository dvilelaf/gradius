//! The seven-stage catalog: one data row per `$19` value.
//!
//! Every field below is a decoded table index or ROM parameter, recorded in
//! `docs/analysis/stages.md` and the per-stage plan documents. The module is
//! data only: it names where each stage's already-decoded data lives and does
//! not change the runtime.
//!
//! * spawn streams: `rom.spawns.sections[index]`
//! * terrain placements: `rom.collision.terrain[index]`
//! * background screens: `rom.backgrounds` (see `stage_bg`)
//! * music `$8344`/`$8354`, boss threshold `$9A30`, script `$C433`:
//!   `rom.tables` and `rom.collision.threshold`, read through `Game`
//!
//! Stage-specific exceptions that the shared runtime must branch on are
//! carried as the [`QUIRK_*`] bits.

/// Stages 1-5: the background tail uses CHR bank 2 and black palettes 2-3
/// (`$834B` mode 0/2). The cut column is [`StageInfo::tail_column`].
pub const QUIRK_TAIL: u8 = 1 << 0;
/// Stage 6: no Big Core; `$99B3` requests `$AC` at the stage end and `$CD9F`
/// erases the beam wall.
pub const QUIRK_NO_CORE: u8 = 1 << 1;
/// Stage 7: `$9865` resets to stage 1 and increments the loop counter `$28`.
pub const QUIRK_LOOP: u8 = 1 << 2;

/// One stage's data roots.
pub struct StageInfo {
    /// `$19`: zero-based stage number.
    pub index: u8,
    /// Row in `rom.collision.terrain` (same as `index`).
    pub terrain_row: usize,
    /// `$98` for stages 1-5; `None` for stages 6-7, which have no core.
    pub boss_kind: Option<u8>,
    /// [`QUIRK_*`] flags for the shared-runtime exceptions.
    pub quirks: u8,
    /// World tile column where the [`QUIRK_TAIL`] tail starts, or `None` when
    /// the stage has no column-keyed tail. Stages 1-4 cut at 416; stage 5's
    /// 448-column map cuts at 384 so its last 64 columns are the tail.
    pub tail_column: Option<usize>,
}

/// One row per `$19` value, 0-6.
pub static STAGES: [StageInfo; 7] = [
    StageInfo {
        index: 0,
        terrain_row: 0,
        boss_kind: Some(0x98),
        quirks: QUIRK_TAIL,
        tail_column: Some(416),
    },
    StageInfo {
        index: 1,
        terrain_row: 1,
        boss_kind: Some(0x98),
        quirks: QUIRK_TAIL,
        tail_column: Some(416),
    },
    StageInfo {
        index: 2,
        terrain_row: 2,
        boss_kind: Some(0x98),
        quirks: QUIRK_TAIL,
        tail_column: Some(416),
    },
    StageInfo {
        index: 3,
        terrain_row: 3,
        boss_kind: Some(0x98),
        quirks: QUIRK_TAIL,
        tail_column: Some(416),
    },
    StageInfo {
        index: 4,
        terrain_row: 4,
        boss_kind: Some(0x98),
        quirks: QUIRK_TAIL,
        tail_column: Some(384),
    },
    StageInfo {
        index: 5,
        terrain_row: 5,
        boss_kind: None,
        quirks: QUIRK_NO_CORE,
        tail_column: None,
    },
    StageInfo {
        index: 6,
        terrain_row: 6,
        boss_kind: None,
        quirks: QUIRK_LOOP,
        tail_column: None,
    },
];

/// The stage's data roots, or `None` past stage 7.
pub fn stage_info(index: usize) -> Option<&'static StageInfo> {
    STAGES.get(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::spawns::Record;
    use crate::rom::testing::rom;

    #[test]
    fn every_stage_has_exactly_one_row() {
        assert_eq!(STAGES.len(), 7);
        for (index, stage) in STAGES.iter().enumerate() {
            assert_eq!(usize::from(stage.index), index);
            assert_eq!(stage_info(index).map(|row| row.index), Some(index as u8));
            assert_eq!(stage.terrain_row, index);
        }
        assert!(stage_info(7).is_none());
    }

    #[test]
    fn section_rows_are_the_decoded_stage_streams() {
        // Rows 3-7 have an empty eighth column; stages 1-2 use all eight.
        let columns = [8, 8, 7, 7, 7, 7, 7];
        for (index, expected) in columns.iter().enumerate() {
            let used = rom().spawns.sections[index]
                .iter()
                .filter(|column| !column.is_empty())
                .count();
            assert_eq!(used, *expected, "stage {} columns", index + 1);
        }
    }

    #[test]
    fn terrain_rows_hold_the_documented_placement_counts() {
        let counts = [63, 101, 18, 63, 0, 75, 89];
        for (index, stage) in STAGES.iter().enumerate() {
            let terrain = &crate::rom::testing::rom().collision.terrain[stage.terrain_row];
            assert_eq!(terrain.len(), counts[index], "stage {} terrain", index + 1);
        }
    }

    #[test]
    fn backgrounds_have_the_documented_dimensions() {
        let shapes = [
            (448, 28),
            (480, 28),
            (480, 28),
            (480, 28),
            (448, 28),
            (416, 28),
            (416, 28),
        ];
        for (index, (width, height)) in shapes.iter().copied().enumerate() {
            let screen = crate::stage_bg::background(crate::rom::testing::rom(), index)
                .expect("stage screen");
            assert_eq!(screen.width_tiles, width, "stage {} width", index + 1);
            assert_eq!(screen.height_tiles, height, "stage {} height", index + 1);
            assert_eq!(screen.tiles.len(), width * height);
            assert_eq!(screen.attributes.len(), (width / 4) * (height / 4));
        }
    }

    #[test]
    fn music_ids_match_the_documented_themes() {
        let music = [0x96, 0x59, 0x5B, 0x5D, 0x5F, 0x61, 0x63];
        for (index, expected) in music.iter().copied().enumerate() {
            assert_eq!(
                rom().tables.music[index],
                expected,
                "stage {} music",
                index + 1
            );
        }
    }

    #[test]
    fn boss_and_script_roots_match_the_stage_table() {
        let triggers = [0x0C, 0x0C, 0x0C, 0x0C, 0x0B, 0x0B, 0x0C];
        let scripts = [0xC480, 0xC540, 0xC680, 0xC5A7, 0xC64D, 0xC6D8, 0xC423];
        for (index, stage) in STAGES.iter().enumerate() {
            assert_eq!(rom().collision.threshold[index], triggers[index]);
            assert_eq!(rom().tables.script_root[index], scripts[index]);
            let core = if index < 5 { Some(0x98) } else { None };
            assert_eq!(stage.boss_kind, core, "stage {} core", index + 1);
        }
    }

    #[test]
    fn music_boundaries_are_the_8354_row() {
        // `$8354`: every stage switches at `$3F = 4` except stage 7 (`2`).
        let boundaries = [4, 4, 4, 4, 4, 4, 2];
        for (index, boundary) in boundaries.iter().enumerate() {
            assert_eq!(
                rom().tables.music_boundary[index],
                *boundary,
                "stage {}",
                index + 1
            );
        }
    }

    #[test]
    fn stage_four_has_no_command_records_so_no_14_cannons() {
        // `$14` (`$CA58`) is a stage-5 command record; stage 4's stream is
        // terrain (`< $80`) and formation members only.
        for column in &rom().spawns.sections[3] {
            for record in column {
                assert!(
                    !matches!(record, Record::Command(_)),
                    "stage 4 has no `$A374` command records"
                );
            }
        }
    }

    #[test]
    fn only_stages_1_to_5_carry_the_tail_quirk() {
        // `$834B`: stages 1-5 end in the empty-tail art; 6-7 have no
        // column-keyed change (their palette writes happen at column 0).
        let cuts = [
            Some(416),
            Some(416),
            Some(416),
            Some(416),
            Some(384),
            None,
            None,
        ];
        for (index, stage) in STAGES.iter().enumerate() {
            let expected = match index {
                0..=4 => QUIRK_TAIL,
                5 => QUIRK_NO_CORE,
                6 => QUIRK_LOOP,
                _ => 0,
            };
            assert_eq!(stage.quirks, expected, "stage {} quirks", index + 1);
            assert_eq!(stage.tail_column, cuts[index], "stage {} tail", index + 1);
            assert_eq!(
                stage.quirks & QUIRK_TAIL != 0,
                stage.tail_column.is_some(),
                "stage {} tail flag/column",
                index + 1
            );
        }
    }
}
