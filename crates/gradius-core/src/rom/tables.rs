//! `TABL` section of `CHUNK_ROM`: byte copies of the small tables the port reads
//! straight from the PRG. Structure (masks, signs, which field is which) stays
//! in code (`stage1_enemies`, `stage3_enemies`, `Game::stage_music`).
//!
//! Payload (little endian): for each of the five stage-1 formation types
//! `$80`-`$84`, its 4-byte object definition (`$A5FC`), 2-byte size entry
//! (`$A58C`) and 3-byte movement entry; then for each of the four explosion
//! streams a `u8` group count and per group a `u8` magnitude, a `u8` tile count
//! and the tiles; then the seven music ids (`$8344`), the seven music
//! boundaries (`$8354`) and the seven script roots (`$C433`, `u16`); then the
//! `$0600` cannon arms' tip tiles (`$8BF7`) and tip attributes (`$8C07`); and
//! finally the eruption (`$C520`), stage-2 hazard (`$C587`) and beam (`$C74C`)
//! nibble scripts with the `$C567` hazard positions; and lastly the loose
//! byte tables: destruction sounds, ship starts, the demo input, the stage-3
//! script Y positions, the cannon-arm steps and the `$CA23` turret groups.
//!
//! The last two groups are the palette RAM the cartridge uploads through
//! `$85ED` text records (`$877F`, `$879D`, `$87B0`, `$87DD`, `$878A`, `$87D4`)
//! and the two HUD nametable rows `$89E8` builds at `$2384`/`$23A2`, captured
//! from the running cartridge at frame 400.

use super::RomError;

pub const TAG: [u8; 4] = *b"TABL";

/// Playable stages (`$19` 0..6).
pub const STAGE_COUNT: usize = 7;
/// Stage-1 formation types `$80`-`$84`.
pub const SPAWN_TYPES: usize = 5;
/// Explosion streams, selected by the type's low two bits.
pub const EXPLOSION_STREAMS: usize = 4;

/// The raw bytes one stage-1 formation type reads from the ROM.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpawnRow {
    /// `$A5FC`: object definition, four bytes per type.
    pub object: [u8; 4],
    /// `$A58C`: size entry, two bytes per type.
    pub size: [u8; 2],
    /// Movement entry, three bytes, at the address the port documents for the
    /// type; see `SpawnType::from_row`.
    pub movement: [u8; 3],
}

/// One explosion group: the magnitude of its offset and its tiles. The sign of
/// the offset is not a byte of the stream, so it is not stored here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExplosionGroup {
    pub magnitude: u8,
    pub tiles: Vec<u8>,
}

/// One `$3F00`/`$3F10` palette RAM upload, four background or sprite palettes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Palettes {
    /// `$87B0`+`$877F`: the stage's `$3F00..$3F0F` background palettes.
    pub background: [[u8; 4]; 4],
    /// `$879D`: the stage's `$3F10..$3F1F` sprite palettes.
    pub sprite: [[u8; 4]; 4],
    /// `$87DD`: the title's `$3F00..$3F0F` background palettes.
    pub title_background: [[u8; 4]; 4],
    /// `$87DD`: the title's `$3F10..$3F1F` sprite palettes.
    pub title_sprite: [[u8; 4]; 4],
    /// `$87D4`+`$877F`: the ending's `$3F00..$3F0F` background palettes.
    pub ending_background: [[u8; 4]; 4],
    /// `$878A`: the ending's `$3F10..$3F1F` sprite palettes.
    pub ending_sprite: [[u8; 4]; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    pub spawns: [SpawnRow; SPAWN_TYPES],
    /// `$C88D`: the four explosion streams, indexed by `type & 3`.
    pub explosions: [Vec<ExplosionGroup>; EXPLOSION_STREAMS],
    /// `$8344`: each stage's second music theme.
    pub music: [u8; STAGE_COUNT],
    /// `$8354`: the `$3F` boundary that switches to the second theme.
    pub music_boundary: [u8; STAGE_COUNT],
    /// `$C433`: each stage's scripted-object handler.
    pub script_root: [u16; STAGE_COUNT],
    /// `$8BF7`: the stage-5 cannon tip tiles, by `($0615 >> 2) & $0F`.
    pub cannon_tip: [u8; 16],
    /// `$8C07`: the tip's attribute, by `($0615 >> 4) & $03` (palette 2 and
    /// the direction flips).
    pub cannon_tip_attr: [u8; 4],
    /// `$C520`: stage-1 eruption nibble script, one byte per two calls.
    pub eruption_script: [u8; 32],
    /// `$C587`: the stage-2 hazard nibble script.
    pub hazard_script: [u8; 32],
    /// `$C74C`: the stage-6 beam nibble script.
    pub beam_script: [u8; 32],
    /// `$C567`: X/Y pairs for stage-2 hazard kind `$0B`, by nibble.
    pub hazard_positions: [(u8, u8); 16],
    /// `$BE68`: destruction sound id by kind low 7 bits (0 = silent).
    pub destroy_sfx: [u8; 22],
    /// `$9BCE`: ship start positions, high nibble Y and low nibble X >> 4.
    pub ship_start: [u8; 26],
    /// `$9CB1`: the demo input records, button byte then tick count.
    pub demo_input: [u8; 152],
    /// `$C6C8`: the `$68`-indexed Y positions of the stage-3 `$C680` script.
    pub stage3_script_y: [u8; 16],
    /// `$CD5F`: cannon part X step by `$0611 & $1F`.
    pub cannon_step_x: [u8; 32],
    /// `$CD7F`: cannon part Y step by `$0611 & $1F`.
    pub cannon_step_y: [u8; 32],
    /// `$CA23`: stage-3 turret art; base groups then their alternates.
    pub turret_groups: [[u8; 4]; 8],
    /// The palette RAM the stage, title and ending screens upload.
    pub palettes: Palettes,
    /// `$2384` row 28: the `$89E8` meter labels with their blank border.
    pub hud_meter_row: [u8; 32],
    /// `$23A2` row 29: the status row with the default lives and digits.
    pub hud_status_row: [u8; 32],
}

impl Default for Tables {
    fn default() -> Self {
        Self {
            spawns: Default::default(),
            explosions: Default::default(),
            music: [0; STAGE_COUNT],
            music_boundary: [0; STAGE_COUNT],
            script_root: [0; STAGE_COUNT],
            cannon_tip: [0; 16],
            cannon_tip_attr: [0; 4],
            eruption_script: [0; 32],
            hazard_script: [0; 32],
            beam_script: [0; 32],
            hazard_positions: [(0, 0); 16],
            destroy_sfx: [0; 22],
            ship_start: [0; 26],
            demo_input: [0; 152],
            stage3_script_y: [0; 16],
            cannon_step_x: [0; 32],
            cannon_step_y: [0; 32],
            turret_groups: [[0; 4]; 8],
            palettes: Palettes::default(),
            hud_meter_row: [0; 32],
            hud_status_row: [0; 32],
        }
    }
}

impl Tables {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for row in &self.spawns {
            out.extend_from_slice(&row.object);
            out.extend_from_slice(&row.size);
            out.extend_from_slice(&row.movement);
        }
        for stream in &self.explosions {
            out.push(u8::try_from(stream.len()).expect("too many groups"));
            for group in stream {
                out.push(group.magnitude);
                out.push(u8::try_from(group.tiles.len()).expect("too many tiles"));
                out.extend_from_slice(&group.tiles);
            }
        }
        out.extend_from_slice(&self.music);
        out.extend_from_slice(&self.music_boundary);
        for root in &self.script_root {
            out.extend_from_slice(&root.to_le_bytes());
        }
        out.extend_from_slice(&self.cannon_tip);
        out.extend_from_slice(&self.cannon_tip_attr);
        out.extend_from_slice(&self.eruption_script);
        out.extend_from_slice(&self.hazard_script);
        out.extend_from_slice(&self.beam_script);
        for (x, y) in &self.hazard_positions {
            out.push(*x);
            out.push(*y);
        }
        out.extend_from_slice(&self.destroy_sfx);
        out.extend_from_slice(&self.ship_start);
        out.extend_from_slice(&self.demo_input);
        out.extend_from_slice(&self.stage3_script_y);
        out.extend_from_slice(&self.cannon_step_x);
        out.extend_from_slice(&self.cannon_step_y);
        for group in &self.turret_groups {
            out.extend_from_slice(group);
        }
        for palettes in [
            &self.palettes.background,
            &self.palettes.sprite,
            &self.palettes.title_background,
            &self.palettes.title_sprite,
            &self.palettes.ending_background,
            &self.palettes.ending_sprite,
        ] {
            for palette in palettes {
                out.extend_from_slice(palette);
            }
        }
        out.extend_from_slice(&self.hud_meter_row);
        out.extend_from_slice(&self.hud_status_row);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RomError> {
        let mut at = 0;
        let mut take = |len: usize| {
            let part = bytes.get(at..at + len).ok_or(RomError::Truncated)?;
            at += len;
            Ok::<_, RomError>(part)
        };
        let mut tables = Self::default();
        for row in &mut tables.spawns {
            row.object = take(4)?.try_into().unwrap();
            row.size = take(2)?.try_into().unwrap();
            row.movement = take(3)?.try_into().unwrap();
        }
        for stream in &mut tables.explosions {
            for _ in 0..take(1)?[0] {
                let magnitude = take(1)?[0];
                let count = take(1)?[0];
                let tiles = take(usize::from(count))?.to_vec();
                stream.push(ExplosionGroup { magnitude, tiles });
            }
        }
        tables.music = take(STAGE_COUNT)?.try_into().unwrap();
        tables.music_boundary = take(STAGE_COUNT)?.try_into().unwrap();
        for root in &mut tables.script_root {
            *root = u16::from_le_bytes(take(2)?.try_into().unwrap());
        }
        tables.cannon_tip = take(16)?.try_into().unwrap();
        tables.cannon_tip_attr = take(4)?.try_into().unwrap();
        tables.eruption_script = take(32)?.try_into().unwrap();
        tables.hazard_script = take(32)?.try_into().unwrap();
        tables.beam_script = take(32)?.try_into().unwrap();
        let positions = take(32)?;
        tables.hazard_positions =
            std::array::from_fn(|index| (positions[2 * index], positions[2 * index + 1]));
        tables.destroy_sfx = take(22)?.try_into().unwrap();
        tables.ship_start = take(26)?.try_into().unwrap();
        tables.demo_input = take(152)?.try_into().unwrap();
        tables.stage3_script_y = take(16)?.try_into().unwrap();
        tables.cannon_step_x = take(32)?.try_into().unwrap();
        tables.cannon_step_y = take(32)?.try_into().unwrap();
        let groups = take(32)?;
        tables.turret_groups =
            std::array::from_fn(|index| groups[4 * index..4 * index + 4].try_into().unwrap());
        for palettes in [
            &mut tables.palettes.background,
            &mut tables.palettes.sprite,
            &mut tables.palettes.title_background,
            &mut tables.palettes.title_sprite,
            &mut tables.palettes.ending_background,
            &mut tables.palettes.ending_sprite,
        ] {
            for palette in palettes {
                *palette = take(4)?.try_into().unwrap();
            }
        }
        tables.hud_meter_row = take(32)?.try_into().unwrap();
        tables.hud_status_row = take(32)?.try_into().unwrap();
        Ok(tables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Tables {
        Tables {
            spawns: std::array::from_fn(|index| SpawnRow {
                object: [index as u8, 0x05, 0x00, 0x01],
                size: [0xF4, 0x2A + index as u8],
                movement: [0x0A, 0x00, 0xC8 + index as u8],
            }),
            explosions: std::array::from_fn(|stream| {
                vec![
                    ExplosionGroup {
                        magnitude: 0x5D,
                        tiles: vec![0, 0, 0],
                    },
                    ExplosionGroup {
                        magnitude: stream as u8,
                        tiles: vec![0xB2, 0xB3, 0xB4, 0xB5],
                    },
                ]
            }),
            music: [0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27],
            music_boundary: [4, 4, 4, 4, 4, 4, 2],
            script_root: [0x9100, 0x9120, 0x9140, 0x9160, 0x9180, 0x91A0, 0x91C0],
            cannon_tip: std::array::from_fn(|index| 0x80 + index as u8),
            cannon_tip_attr: std::array::from_fn(|index| 0x40 + index as u8),
            eruption_script: [0x5A; 32],
            hazard_script: [0xA5; 32],
            beam_script: [0x3C; 32],
            hazard_positions: std::array::from_fn(|index| (index as u8, 0x40 + index as u8)),
            destroy_sfx: std::array::from_fn(|index| index as u8 & 0x0F),
            ship_start: [0x65; 26],
            demo_input: std::array::from_fn(|index| (index % 5) as u8),
            stage3_script_y: [0x40; 16],
            cannon_step_x: std::array::from_fn(|index| index as u8),
            cannon_step_y: std::array::from_fn(|index| 0xF0 | index as u8),
            turret_groups: std::array::from_fn(|index| [index as u8, 0, 0, 0]),
            palettes: Palettes {
                background: std::array::from_fn(|index| [0x0F, index as u8, 0x30, 0x0F]),
                sprite: std::array::from_fn(|index| [0x0F, 0x0C, index as u8, 0x30]),
                title_background: std::array::from_fn(|index| [0x0F, 0x30, index as u8, 0x0F]),
                title_sprite: std::array::from_fn(|index| [0x0F, 0x06, index as u8, 0x30]),
                ending_background: std::array::from_fn(|index| [0x0F, 0x11, index as u8, 0x2C]),
                ending_sprite: std::array::from_fn(|index| [0x0F, 0x12, index as u8, 0x31]),
            },
            hud_meter_row: std::array::from_fn(|index| index as u8),
            hud_status_row: std::array::from_fn(|index| (index as u8) << 4),
        }
    }

    #[test]
    fn encode_then_decode_round_trips() {
        let tables = sample();
        assert_eq!(Tables::decode(&tables.encode()), Ok(tables));
    }

    #[test]
    fn a_truncated_section_is_rejected() {
        let bytes = sample().encode();
        for len in [0, 1, 20, bytes.len() / 2, bytes.len() - 1] {
            assert_eq!(Tables::decode(&bytes[..len]), Err(RomError::Truncated));
        }
    }
}
