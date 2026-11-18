//! Decoders for the stage-1 enemy spawn tables.
//!
//! The original builds every multi-object formation from three ROM tables:
//!
//! | table | address | entry | meaning |
//! | --- | --- | --- | --- |
//! | object definitions | `$A5FC` | 4 bytes | `subtype, kind, size index, move index` |
//! | sizes | `$A58C` | 2 bytes | low nibble `count`, high nibble `x`, then `base_y` |
//! | movement | `$A5B6` | 3 bytes | spawn `gap`, `y_step`, motion flags |
//!
//! `$A3DE` reads the definition indexed by the stream type (two `ASL`s drop
//! the type's high bits, so the index is `type & $3F`), `$A3FF` splits the size
//! entry, and `$A421` picks the movement entry with `3 * move_index`. The
//! functions here take the raw table bytes and return the decoded values, so
//! they can be tested with synthetic data and reused by `xtask spawn`.

/// One 4-byte object definition (`$A5FC + 4 * (type & $3F)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectDef {
    /// `$64`: subtype / tile base.
    pub subtype: u8,
    /// `$65`: object kind, the `$030C` dispatch index.
    pub kind: u8,
    /// `$66`: index into the size table at `$A58C`.
    pub size_index: u8,
    /// `$67`: index into the movement table at `$A5B6`.
    pub move_index: u8,
}

/// One 2-byte size entry (`$A58C + 2 * size_index`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeEntry {
    /// Low nibble of the first byte: number of objects in the formation.
    pub count: u8,
    /// High nibble of the first byte: spawn X (always `$F0` in the shipped
    /// tables, i.e. 240, off the right edge).
    pub x: u8,
    /// Second byte: the first object's Y.
    pub base_y: u8,
}

/// One 3-byte movement entry (`$A5B6 + 3 * move_index`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveEntry {
    /// `$6C`: frames between two objects of the same formation.
    pub gap: u8,
    /// Y added per formation object.
    pub y_step: u8,
    /// `$04EC` / `$040C`: motion flags, high bit cleared.
    pub motion: u8,
    /// `$03AC`: formation phase (low bit of the third byte).
    pub phase: u8,
}

/// A fully decoded formation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formation {
    pub kind: u8,
    pub subtype: u8,
    pub count: u8,
    pub x: u8,
    pub ys: Vec<u8>,
    pub gap: u8,
    pub motion: u8,
    pub phase: u8,
}

/// Decode a 4-byte object definition.
pub fn object_def(bytes: &[u8]) -> Option<ObjectDef> {
    let [subtype, kind, size_index, move_index, ..] = bytes else {
        return None;
    };
    Some(ObjectDef {
        subtype: *subtype,
        kind: *kind,
        size_index: *size_index,
        move_index: *move_index,
    })
}

/// Decode a 2-byte size entry.
pub fn size_entry(first: u8, base_y: u8) -> SizeEntry {
    SizeEntry {
        count: first & 0x0F,
        x: first & 0xF0,
        base_y,
    }
}

/// Decode a 3-byte movement entry.
pub fn move_entry(bytes: &[u8]) -> Option<MoveEntry> {
    let [gap, y_step, flags, ..] = bytes else {
        return None;
    };
    Some(MoveEntry {
        gap: *gap,
        y_step: *y_step,
        motion: flags & 0xFE,
        phase: flags & 0x01,
    })
}

/// Decode a formation from the three raw table entries.
pub fn formation(def: &[u8], size: &[u8], movement: &[u8]) -> Option<Formation> {
    let def = object_def(def)?;
    let size = size_entry(*size.first()?, *size.get(1)?);
    let movement = move_entry(movement)?;
    let ys = (0..size.count)
        .map(|index| {
            size.base_y
                .wrapping_add(index.wrapping_mul(movement.y_step))
        })
        .collect();
    Some(Formation {
        kind: def.kind,
        subtype: def.subtype,
        count: size.count,
        x: size.x,
        ys,
        gap: movement.gap,
        motion: movement.motion,
        phase: movement.phase,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_an_object_definition() {
        let def = object_def(&[0x05, 0x04, 0x03, 0x02]).unwrap();
        assert_eq!(
            def,
            ObjectDef {
                subtype: 0x05,
                kind: 0x04,
                size_index: 0x03,
                move_index: 0x02,
            }
        );
        assert!(object_def(&[0x05, 0x04, 0x03]).is_none());
    }

    #[test]
    fn splits_the_size_nibbles() {
        let size = size_entry(0xF2, 0x28);
        assert_eq!(size.count, 2);
        assert_eq!(size.x, 0xF0);
        assert_eq!(size.base_y, 0x28);

        let size = size_entry(0xF4, 0xA0);
        assert_eq!(size.count, 4);
        assert_eq!(size.base_y, 0xA0);
    }

    #[test]
    fn decodes_movement_flags_and_phase() {
        let movement = move_entry(&[0x0A, 0x00, 0xC9]).unwrap();
        assert_eq!(movement.gap, 10);
        assert_eq!(movement.y_step, 0);
        assert_eq!(movement.motion, 0xC8);
        assert_eq!(movement.phase, 1);

        let movement = move_entry(&[0x02, 0x40, 0xC8]).unwrap();
        assert_eq!(movement.gap, 2);
        assert_eq!(movement.y_step, 0x40);
        assert_eq!(movement.motion, 0xC8);
        assert_eq!(movement.phase, 0);
    }

    #[test]
    fn builds_a_formation_from_synthetic_tables() {
        // subtype $01, kind $05, size 0, move 0.
        let def = [0x01, 0x05, 0x00, 0x00];
        let size = [0xF4, 0x2A];
        let movement = [0x0A, 0x00, 0xC8];
        let formation = formation(&def, &size, &movement).unwrap();
        assert_eq!(formation.kind, 0x05);
        assert_eq!(formation.subtype, 0x01);
        assert_eq!(formation.count, 4);
        assert_eq!(formation.x, 0xF0);
        assert_eq!(formation.ys, vec![0x2A; 4]);
        assert_eq!(formation.gap, 10);
        assert_eq!(formation.motion, 0xC8);
        assert_eq!(formation.phase, 0);
    }

    #[test]
    fn formation_y_wraps_and_steps() {
        let def = [0x05, 0x04, 0x03, 0x02];
        let size = [0xF2, 0x08];
        let movement = [0x02, 0x40, 0xC8];
        let first = formation(&def, &size, &movement).unwrap();
        assert_eq!(first.count, 2);
        assert_eq!(first.ys, vec![0x08, 0x48]);

        // A synthetic step that wraps the byte keeps the original wraparound.
        let wrapped = formation(&def, &[0xF2, 0xF0], &[0x02, 0x40, 0xC8]).unwrap();
        assert_eq!(wrapped.ys, vec![0xF0, 0x30]);
    }

    #[test]
    fn rejects_short_inputs() {
        assert!(formation(&[1, 2, 3], &[0xF4], &[0, 0, 0]).is_none());
        assert!(formation(&[1, 2, 3, 4], &[0xF4, 0], &[0, 0]).is_none());
    }

    #[test]
    fn core_module_matches_the_decoded_tables() {
        use gradius_core::stage1_enemies as core;
        // The five `$A5FC`/`$A58C`/movement entries as `rom.tables.spawns` holds them.
        let rows = [
            ([0x01, 0x05, 0x00, 0x00], [0xF4, 0x2A], [0x0A, 0x00, 0xC8]),
            ([0x01, 0x05, 0x01, 0x00], [0xF4, 0xA0], [0x0A, 0x00, 0xC8]),
            ([0x00, 0x08, 0x02, 0x04], [0xF3, 0x08], [0x02, 0x34, 0xC8]),
            ([0x05, 0x04, 0x03, 0x02], [0xF2, 0x08], [0x02, 0x40, 0xC8]),
            ([0x05, 0x04, 0x04, 0x03], [0xF2, 0x28], [0x02, 0x40, 0xC9]),
        ];
        let spawns: Vec<core::SpawnType> = rows
            .iter()
            .enumerate()
            .map(|(index, &(object, size, movement))| {
                let row = gradius_core::rom::tables::SpawnRow {
                    object,
                    size,
                    movement,
                };
                core::SpawnType::from_row(index, &row)
            })
            .collect();
        assert_eq!(spawns.len(), 5);
        assert_eq!(spawns[0].stream_kind, 0x80);
        assert_eq!(spawns[4].stream_kind, 0x84);
        // The decoder and the const table must agree for every entry.
        for spawn in &spawns {
            assert_eq!(
                core::formation_y(spawn, 0),
                Some(spawn.base_y),
                "type ${:02X}",
                spawn.stream_kind
            );
            assert_eq!(core::formation_y(spawn, spawn.count), None);
            let last = spawn
                .base_y
                .wrapping_add((spawn.count - 1).wrapping_mul(spawn.y_step));
            assert_eq!(core::formation_y(spawn, spawn.count - 1), Some(last));
        }
        assert_eq!(spawns[0].y(1), Some(0x2A));
        assert_eq!(spawns[2].y(2), Some(0x70));
        assert_eq!(spawns[3].y(1), Some(0x48));
        assert_eq!(spawns[4].y(1), Some(0x68));
    }

    #[test]
    fn core_module_exposes_the_measured_boss_numbers() {
        use gradius_core::stage1_enemies as core;
        assert_eq!(core::big_core::KIND, 0x98);
        assert_eq!(core::big_core::PART_KIND, 0x99);
        assert_eq!(core::big_core::SPAWN_X, 0xF0);
        assert_eq!(core::big_core::SPAWN_Y, 0x80);
        assert_eq!(core::big_core::STOP_X, 0xA4);
        assert_eq!(core::big_core::DAMAGE_LIMIT, 7);
        assert_eq!(core::big_core::SELF_DESTRUCT_FRAMES, 1536);
        assert_eq!(core::boss_fire_period(1), Some(0x5A));
        assert_eq!(core::boss_fire_period(8), Some(0x23));
        assert_eq!(core::boss_vertical_velocity(1), Some((0x00, 0x01)));
        assert_eq!(core::boss_vertical_velocity(8), Some((0xF0, 0x01)));
        assert_eq!(core::boss_gun_offset(0), Some((8, -15)));
        assert_eq!(core::boss_gun_offset(3), Some((8, 23)));
        assert_eq!(core::enemy_hitbox(0), Some((0x10, 0x08)));
        assert_eq!(core::enemy_hitbox(1), Some((0x16, 0x12)));
        assert!(core::enemy_hitbox(4).is_none());
        assert!(core::boss_tracks_right(0xA4, 0xA4));
        assert!(!core::boss_tracks_right(0xA4, 0xA3));
    }

    #[test]
    fn core_module_decodes_terrain_and_commands() {
        use gradius_core::stage1_enemies as core;
        assert_eq!(core::terrain_spawn(0xB2), Some((0x12, 0xF0)));
        assert_eq!(core::terrain_spawn(0xE3), Some((0x13, 0x10)));
        assert_eq!(core::terrain_spawn(0xC7), Some((0x27, 0xF0)));
        assert_eq!(core::terrain_spawn(0x9F), None);
        assert!(!core::is_command_record(0x80));
        assert!(!core::is_command_record(0xEF));
        assert!(core::is_command_record(0xF0));
        assert!(core::is_command_record(0xFF));
        assert_eq!(core::COMMAND_RECORD_LEN, 5);
        assert_eq!(core::COMMAND_SUBTYPE_BIAS, 0x70);
    }
}
