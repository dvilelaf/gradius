//! Hand-written rule tables and scalar constants of the engine: hit boxes,
//! timers, per-stage parameters and the stage hazard scripts. The ROM data
//! tables (spawn streams, object and terrain definitions, animation frames)
//! live in [`crate::rom::spawns`].

/// Enemy collision widths, indexed by `$0176` (`$C1FC`).
pub static ENEMY_WIDTH: [u8; 4] = [0x10, 0x16, 0x16, 0x16];
/// Enemy collision heights, indexed by `$0176` (`$C200`).
pub static ENEMY_HEIGHT: [u8; 4] = [0x08, 0x12, 0x12, 0x10];

/// Terrain attribute-quadrant masks (`$C409`).
pub static TERRAIN_MASK: [u8; 4] = [0x03, 0x0C, 0x30, 0xC0];

/// `$3F` threshold per section (`$9A30`).
pub static SECTION_THRESHOLD: [u8; 8] = [0x0C, 0x0C, 0x0C, 0x0C, 0x0B, 0x0B, 0x0C, 0x02];
/// Stage state written at the threshold (`$9A38`).
pub static SECTION_NEXT_STATE: [u8; 8] = [0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81];
/// Per-section row offset into the ship starts (`$9BC6`).
pub static SHIP_Y_OFFSET: [u8; 8] = [0x00, 0x05, 0x0A, 0x00, 0x00, 0x0F, 0x14, 0x00];

/// Stage init blanking frames (`$9B38: $0D = 6`).
pub const STAGE_BLANK_FRAMES: u8 = 6;
/// PPUMASK uploaded by the stage init (`$9B75`).
pub const STAGE_PPUMASK: u8 = 0x1E;
/// PPUCTRL uploaded by the stage init (`$9B79`).
pub const STAGE_PPUCTRL: u8 = 0xA8;
/// Fire cooldown in frames (`$9B58: $35 = $14`).
pub const FIRE_COOLDOWN: u8 = 20;
/// Normal shot speed in pixels per frame (`$A1EE: ADC #$07`).
pub const SHOT_SPEED: u8 = 7;
/// Normal shot despawn at X >= `$F8` (`$A1CC`).
pub const SHOT_DESPAWN: u8 = 0xF8;
/// Laser shot speed in pixels per frame (`$A21A`).
pub const LASER_SPEED: u8 = 12;
/// Laser shot despawn at X >= `$F0` (`$A225`).
pub const LASER_DESPAWN: u8 = 0xF0;
/// Scroll fraction added per frame (`$98E1: ADC #$80`).
pub const SCROLL_STEP: u8 = 0x80;
/// Option trail ring length (`$A086: CMP #$18`).
pub const OPTION_TRAIL: usize = 24;
/// Option trail lag between consecutive drones (`$A2A3: SBC #$0B`).
pub const OPTION_LAG: usize = 11;

mod ai;
mod combat;
pub use self::ai::*;
pub use self::combat::*;
