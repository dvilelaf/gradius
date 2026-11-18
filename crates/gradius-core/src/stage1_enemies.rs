//! Proven stage-1 enemy and Big Core parameters.
//!
//! Every constant here is decoded from the original ROM's tables or code and
//! has an address comment. The evidence and the reproduction commands live in
//! `docs/analysis/enemy_ai.md`; no cartridge bytes are stored, only these
//! decoded numbers.
//!
//! ## What is covered
//!
//! * the five stage-1 stream formations (`$80`..`$84`) and their spawn tables
//!   (`$A5FC`, `$A58C`, `$A5B6`, spawned by `$A3DE`);
//! * the movement constants of the three handlers they use (`$B0A9`,
//!   `$B266`, `$B1FF`);
//! * the collision extents (`$C1FC`/`$C200`) and the one-shot kill for
//!   ordinary kinds;
//! * the 3-byte terrain placement (`$A3AB`) and the 5-byte command record
//!   discriminator (`$A345`; stage 1 has none);
//! * the stage-1 aimed-fire family (`$AF28`/`$AF82`/`$AF92`) and its cadence;
//! * the Big Core (`$98`): spawn (`$9975`), motion (`$B90E`), fire (`$BA9A`),
//!   damage and self-destruct, and the stage-end trigger.
//!
//! ## Deliberately not modelled
//!
//! * The beam projectile spawned by the four Big Core gun pods. The pods are
//!   proven; the routine that moves the visible `$0E`/`$10` sprite beams is
//!   not located yet, so no projectile velocity is invented here.
//! * The `$99` cover parts. Their handler `$B90D` is a bare `RTS`; whether
//!   they are meant to absorb shots is not proven.
//! * Kind `$97` (`$B79B`) and kind `$96` (`$C900`): neither is reachable from
//!   the stage-1 data, so they are not exposed.
//!
//! All arithmetic is integer; positions are the original's one-byte pixel
//! coordinates.

use crate::rom::tables::SpawnRow;

/// Largest live enemy slot. `$A3AB`/`$A411` search slots 9 down to 0.
pub const LIVE_SLOTS: usize = 10;

/// One decoded stage-1 spawn formation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnType {
    /// Stream type byte as written in the column stream.
    pub stream_kind: u8,
    /// `$030C` dispatch kind (`$B0A9`/`$B266`/`$B1FF`).
    pub object_kind: u8,
    /// `$64` -> `$010C`, subtype / tile base.
    pub subtype: u8,
    /// Low nibble of the size entry: number of objects.
    pub count: u8,
    /// High nibble of the size entry: spawn X (always 240 here).
    pub spawn_x: u8,
    /// Second size byte: the first object's Y.
    pub base_y: u8,
    /// Movement-entry byte 0: frames between two objects.
    pub gap_frames: u8,
    /// Movement-entry byte 1: Y added per object.
    pub y_step: u8,
    /// Movement-entry byte 2 with bit 0 cleared (`$040C`/`$04EC`).
    pub motion: u8,
    /// Movement-entry byte 2 bit 0 (`$03AC` formation phase).
    pub phase: u8,
}

impl SpawnType {
    /// The decoded form of formation type `$80 + index`, from the bytes
    /// `rom.tables.spawns[index]` copies out of the ROM: the object definition
    /// (`$A5FC`), the size entry (`$A58C`) and the movement entry (`$A5B6`,
    /// `$A5BC`, `$A5BF` or `$A5C2`). `$80`/`$81` are definitions 0/1; `$82`,
    /// `$83` and `$84` are 2, 3 and 4.
    pub fn from_row(index: usize, row: &SpawnRow) -> Self {
        Self {
            stream_kind: 0x80 + index as u8,
            object_kind: row.object[1],
            subtype: row.object[0],
            count: row.size[0] & 0x0F,
            spawn_x: row.size[0] & 0xF0,
            base_y: row.size[1],
            gap_frames: row.movement[0],
            y_step: row.movement[1],
            motion: row.movement[2] & !1,
            phase: row.movement[2] & 1,
        }
    }

    /// Y of object `index`: `base_y + index * y_step`, or `None` past the
    /// formation (`$A42E`: `$6E` accumulates once per allocated object).
    pub fn y(&self, index: u8) -> Option<u8> {
        if index >= self.count {
            return None;
        }
        Some(self.base_y.wrapping_add(index.wrapping_mul(self.y_step)))
    }
}

/// Kind `$05` (`$B0A9`): the four-ship column.
pub mod kind05 {
    /// Leftward entry speed in px/frame (`$B15E` with `A = $FE`).
    pub const ENTRY_SPEED: u8 = 2;
    /// X at which the U-turn starts (`$B0CC: CMP #$60`).
    pub const TURN_X: u8 = 0x60;
    /// Frames the U-turn lasts (`$B0D0: LDA #$40`).
    pub const TURN_FRAMES: u8 = 0x40;
    /// Vertical speed during the U-turn (`$B176` with `A = ±2`).
    pub const TURN_Y_SPEED: u8 = 2;
    /// Y at or above which the U-turn goes up instead of down
    /// (`$B0D5: CMP #$80` selects state 2).
    pub const TURN_Y_THRESHOLD: u8 = 0x80;
    /// Exit speed to the right (`$B0FD: LDA #$03`).
    pub const EXIT_SPEED: u8 = 3;
}

/// Kind `$08` (`$B266`): the weaving ship.
pub mod kind08 {
    /// Leftward drift in px/frame (`$B297: DEC $036C`).
    pub const DRIFT_X: u8 = 1;
    /// Direction flip period in frames (`$B292: LDA #$1E`).
    pub const AIM_PERIOD: u8 = 0x1E;
    /// Y accumulator step (`$048C = $80`, i.e. 0.5 px/frame).
    pub const VERTICAL_STEP: u8 = 0x80;
    /// Exit speed when aligned with the player (`$B0FD`).
    pub const EXIT_SPEED: u8 = 3;
}

/// Kind `$04` (`$B1FF`): the arcing ship.
pub mod kind04 {
    /// Leftward drift in px/frame (`$042C = $FE`, `$B14E`).
    pub const DRIFT_X: u8 = 2;
    /// Initial downward velocity (`$B1AB: LDA #$03`).
    pub const VERTICAL_START: u8 = 3;
    /// Velocity change per frame (`$048C = $20`, i.e. 0.125 px/frame²).
    pub const VERTICAL_STEP: u8 = 0x20;
    /// Arc direction sequence indexed by `$04AC` (`$B1FA`): a `1` makes the
    /// next arc move right, `0` keeps it moving left.
    pub const REVERSE_TABLE: [u8; 5] = [0x00, 0x00, 0x01, 0x00, 0x00];
}

/// Enemy collision extents `(width, height)` indexed by `$0176`
/// (`$C1FC`/`$C200`). Ordinary spawns leave `$0176 = 0`.
pub const ENEMY_HITBOX: [(u8, u8); 4] = [(0x10, 0x08), (0x16, 0x12), (0x16, 0x12), (0x16, 0x10)];

/// Ordinary enemies die to one player shot. `$04AC` is only a hit counter for
/// kind `$9A` (`$C08F`).
pub const NORMAL_HIT_POINTS: u8 = 1;

/// Kind `$9A` armour by rank `$17` (`$BFBF`): 5,5,5,5,6,7,8,9,10 hits.
pub const ARMORED_HIT_POINTS: [u8; 9] = [5, 5, 5, 5, 6, 7, 8, 9, 10];

/// The stage-1 formations are not shooters. Fire comes from kinds `$8F`/`$90`.
pub const STAGE1_FORMATIONS_FIRE: bool = false;

/// Shooter cadence (`$AFB2`: fire only when `($02 & $0F) == 0`).
pub const SHOOTER_FIRE_MASK: u8 = 0x0F;
/// Shots per burst before the phase flips (`$AFBE: CMP #$05`).
pub const SHOOTER_BURST: u8 = 5;
/// X gate for shooter phase 0 (`$AFAE: CMP #$C8`).
pub const SHOOTER_GATE_PHASE0: u8 = 0xC8;
/// X gate for shooter phase 1 (`$AFA3: CMP #$A0`).
pub const SHOOTER_GATE_PHASE1: u8 = 0xA0;
/// `$AF28` shot kind, spawned at `(x + 8, y + 8)`.
pub const AIMED_SHOT_HIGH_KIND: u8 = 0x09;
/// `$AF82` shot kind, spawned at `(x + 8, y - 10)`.
pub const AIMED_SHOT_LOW_KIND: u8 = 0x0C;
/// X offset of both aimed shots (`$AFE8: ADC #$08`).
pub const AIMED_SHOT_DX: i8 = 8;
/// Y offset of the `$AF28` shot (`$AF3D: LDY #$08`).
pub const AIMED_SHOT_HIGH_DY: i8 = 8;
/// Y offset of the `$AF82` shot (`$AF87: LDY #$F6`).
pub const AIMED_SHOT_LOW_DY: i8 = -10;
/// Frames before the aimed shots start steering, indexed by kind
/// (`$B310: LDA #$0A` for `$09`, `$B3CA: LDA #$14` for `$0C`).
pub const AIMED_SHOT_HOME_DELAY: [u8; 2] = [0x0A, 0x14];

/// The Big Core is a hard-coded stage-state spawn, not a stream command.
pub mod big_core {
    /// Core object kind (`$9993: LDA #$98`).
    pub const KIND: u8 = 0x98;
    /// Cover object kind (`$B9F7: LDA #$99`); its handler is `RTS`.
    pub const PART_KIND: u8 = 0x99;
    /// Slot the core is spawned into (`$998C: LDX #$09`).
    pub const SLOT: usize = 9;
    /// Spawn X (`$999D: LDA #$F0`).
    pub const SPAWN_X: u8 = 0xF0;
    /// Spawn Y (`$9998: LDA #$80`).
    pub const SPAWN_Y: u8 = 0x80;
    /// The core stops moving left at this X (`$B9A5: CMP #$A4`).
    pub const STOP_X: u8 = 0xA4;
    /// Horizontal tracking speed towards a player on the right (`$BA0C`).
    pub const TRACK_X: u8 = 2;
    /// Vertical clamp (`$BA56: CMP #$18`, `$BA5C: CMP #$A8`).
    pub const MIN_Y: u8 = 0x18;
    /// Vertical clamp.
    pub const MAX_Y: u8 = 0xA8;
    /// Aim comparison offset (`$BA22: SBC #$10`).
    pub const AIM_OFFSET: u8 = 0x10;
    /// Vertical velocity fraction by rank (`$B8F2`).
    pub const VERTICAL_FRACTION: [u8; 9] = [0x00, 0x00, 0x20, 0x40, 0x60, 0x80, 0xA0, 0xC0, 0xF0];
    /// Vertical velocity integer by rank (`$B8FB`).
    pub const VERTICAL_INTEGER: [u8; 9] = [0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01];
    /// Frames between attacks by rank (`$B904`): 2, 90, 80, 70, 60, 50, 40,
    /// 35, 35. Rank 0 is normally only reachable without a shield.
    pub const FIRE_PERIOD: [u8; 9] = [0x02, 0x5A, 0x50, 0x46, 0x3C, 0x32, 0x28, 0x23, 0x23];
    /// Gun-pod period by rank (`$BAF9`). Only ranks 0..7 are meaningful; rank 8
    /// reads the first byte of the next table (`$BB01 = $C0`), as the original
    /// does.
    pub const GUN_PERIOD: [u8; 8] = [0x02, 0x03, 0x03, 0x03, 0x03, 0x04, 0x04, 0x04];
    /// Gun-pod phase by rank (`$BB01`).
    pub const GUN_PHASE: [u8; 8] = [0xC0, 0x00, 0x40, 0x80, 0xC0, 0x00, 0x40, 0x80];
    /// Damage counter at which the core dies (`$B92C: CPY #$07`).
    pub const DAMAGE_LIMIT: u8 = 7;
    /// `$04CC` periods before the overheat self-destruct (`$BA8B: CPY #$05`,
    /// `$BA92: CPY #$06`).
    pub const SELF_DESTRUCT_PERIODS: u8 = 6;
    /// Frames between `$04CC` increments: `$04AC` is 8-bit.
    pub const COUNTER_FRAMES: u16 = 256;
    /// Self-destruct after 6 * 256 frames (~25.6 s at 60 Hz).
    pub const SELF_DESTRUCT_FRAMES: u16 = SELF_DESTRUCT_PERIODS as u16 * COUNTER_FRAMES;
    /// Sprite groups as the armour opens, indexed by `$046C` (`$B8E9`).
    pub const SPRITE_FRAMES: [u8; 6] = [0x6C, 0x6D, 0x6E, 0x6F, 0x70, 0x71];
    /// Sprite group of the first cover part (`$B9BD: LDA #$85`) and the second
    /// (`$B9CE: LDA #$32`).
    pub const PART_SPRITES: (u8, u8) = (0x85, 0x32);
    /// Y distance of the second cover part (`$B9E3: SBC #$14`).
    pub const PART_Y_OFFSET: u8 = 0x14;
    /// The four gun-pod offsets indexed by slot 0..3 (the `$BA9A` loop fills
    /// slot 3 first, then 2, 1, 0): `$BAF1` = 08 F8 F8 08, `$BAF5` = F1 FE 0A 17.
    pub const GUN_OFFSETS: [(i8, i8); 4] = [(8, -15), (-8, -2), (-8, 10), (8, 23)];
    /// `$0176` of a gun pod (`$BAC4: LDA #$02`), i.e. the 22x18 hitbox.
    pub const GUN_HITBOX_INDEX: u8 = 2;
    /// `$0136` of a gun pod (`$BAD1: LDA #$41`).
    pub const GUN_COLLISION_FLAG: u8 = 0x41;
    /// `$1B` state that spawns the core (`$1B = $84` -> `$9975`).
    pub const SPAWN_STATE: u8 = 0x84;
    /// `$9A30[0]`: scroll block at which stage 1 ends.
    pub const STAGE1_END_SCROLL: u8 = 0x0C;
}

/// A stream record whose second byte is at least this value is a 5-byte
/// command, not a `<delay> <type>` spawn (`$A345`).
pub const COMMAND_SECOND_BYTE_MIN: u8 = 0xF0;
/// Length of a command record (`$A376`-`$A380` copies five bytes).
pub const COMMAND_RECORD_LEN: usize = 5;
/// `$A387`: the command's first data byte is biased by this value.
pub const COMMAND_SUBTYPE_BIAS: u8 = 0x70;

/// `true` for a 5-byte command record.
pub fn is_command_record(second_byte: u8) -> bool {
    second_byte >= COMMAND_SECOND_BYTE_MIN
}

/// Decode the single-object terrain placement at `$A3AB`.
///
/// `subtype` is the record's first byte (`$64`). A subtype in `$A0..=$CF`
/// spawns at `x = 240`, a subtype in `$D0..=$FF` at `x = 16`; the kind is the
/// biased subtype. Subtypes below `$A0` are not used by the stage-1 records.
pub fn terrain_spawn(subtype: u8) -> Option<(u8, u8)> {
    match subtype {
        0xA0..=0xCF => Some((subtype - 0xA0, 0xF0)),
        0xD0..=0xFF => Some((subtype - 0xD0, 0x10)),
        _ => None,
    }
}

/// Y of formation object `index`, if the formation has that object.
pub fn formation_y(spawn: &SpawnType, index: u8) -> Option<u8> {
    spawn.y(index)
}

/// Collision extents for a `$0176` index.
pub fn enemy_hitbox(index: u8) -> Option<(u8, u8)> {
    ENEMY_HITBOX.get(usize::from(index)).copied()
}

/// Big Core vertical velocity as `(fraction, integer)` for a rank.
pub fn boss_vertical_velocity(rank: u8) -> Option<(u8, u8)> {
    let index = usize::from(rank);
    Some((
        *big_core::VERTICAL_FRACTION.get(index)?,
        *big_core::VERTICAL_INTEGER.get(index)?,
    ))
}

/// Big Core attack period in frames for a rank.
pub fn boss_fire_period(rank: u8) -> Option<u8> {
    big_core::FIRE_PERIOD.get(usize::from(rank)).copied()
}

/// Offset `(dx, dy)` of gun pod `slot` (0..3), from the core's centre.
pub fn boss_gun_offset(slot: u8) -> Option<(i8, i8)> {
    big_core::GUN_OFFSETS.get(usize::from(slot)).copied()
}

/// `true` when the core tracks right: the player is at or right of the core
/// (`$BA04`).
pub fn boss_tracks_right(boss_x: u8, player_x: u8) -> bool {
    player_x >= boss_x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::testing::rom;

    #[test]
    fn spawn_types_decode_from_the_pack_rows() {
        let spawns = &rom().tables.spawns;
        let first = SpawnType::from_row(0, &spawns[0]);
        assert_eq!(
            (first.stream_kind, first.object_kind, first.subtype),
            (0x80, 0x05, 0x01)
        );
        assert_eq!((first.count, first.spawn_x, first.base_y), (4, 0xF0, 0x2A));
        assert_eq!((first.gap_frames, first.y_step), (0x0A, 0x00));
        assert_eq!((first.motion, first.phase), (0xC8, 0));
        assert_eq!(first.y(3), Some(0x2A));
        assert_eq!(first.y(4), None);
        let last = SpawnType::from_row(4, &spawns[4]);
        assert_eq!((last.count, last.base_y, last.y_step), (2, 0x28, 0x40));
        assert_eq!((last.motion, last.phase), (0xC8, 1));
        assert_eq!(last.y(1), Some(0x68));
    }
}
