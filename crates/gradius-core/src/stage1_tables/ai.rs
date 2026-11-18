//! Enemy AI and hit-box rule tables (`$ADA5` dispatch, `$AF04`..`$B84C`).

/// Number of entries in the `$AE16` AI jump table (kind `$00`..`$28`).
pub const AI_KIND_COUNT: usize = 41;

/// `$B1FA`: kind-6 `$046C` timer by `$04AC` mode.
pub static AI_TIMER_KIND6: [u8; 5] = [0x00, 0x00, 0x01, 0x00, 0x00];
/// `$B429`: kind-13 `$046C` (x direction) timer by mode.
pub static AI_TIMER_KIND13: [u8; 5] = [0x00, 0x00, 0x00, 0x01, 0x01];
/// `$B456`: kind-14 `$046C` (x direction) timer by mode.
pub static AI_TIMER_KIND14: [u8; 5] = [0x00, 0x00, 0x00, 0x01, 0x01];
/// `$B3BC`: kind-11 launch sprites, one per four animation frames.
pub static LAUNCH_FRAMES: [u8; 9] = [0x64, 0x64, 0x64, 0x65, 0x65, 0x65, 0x66, 0x66, 0x66];
/// `$B335`: kind-9 (`$B30B`) animation sprites.
pub static SPRITE_KIND9: [u8; 8] = [0x5E, 0x5F, 0x60, 0x61, 0x62, 0x61, 0x60, 0x5F];
/// `$B080`: kind-17/18 turret sprite by aim quadrant.
pub static SPRITE_TURRET: [u8; 6] = [0x74, 0x73, 0x72, 0x75, 0x76, 0x77];
/// `$B086`: kind-17/18 sprite when the object is facing left.
pub static SPRITE_TURRET_LEFT: [u8; 6] = [0x01, 0x01, 0x06, 0x05, 0x05, 0x00];
/// `$B08C`: kind-17/18 sprite when the object is flagged `$018C & $80`.
pub static SPRITE_TURRET_FLAG: [u8; 6] = [0x03, 0x03, 0x06, 0x08, 0x08, 0x00];
/// `$AF04`: kind 32..37 background-piece sprites.
pub static SPRITE_BG_PIECE: [u8; 6] = [0x89, 0x87, 0x8C, 0x8B, 0x8A, 0x88];
/// `$B64A`: animation helper tables (delay, sprite base, frame count) for Y=3 and Y=9.
pub static ANIM_DELAY: [u8; 12] = [
    0x06, 0x8E, 0x08, 0x08, 0x4A, 0x08, 0x0A, 0x42, 0x08, 0x08, 0x52, 0x06,
];
/// `$B64A+9`: the `$B553` straight piece divider (8 frames per frame).
pub const STRAIGHT_ANIM_DIVIDER: u8 = ANIM_DELAY[9];
/// `$B64A+10`: the `$B553` sprite group base (`$52`).
pub const STRAIGHT_ANIM_SPRITE: u8 = ANIM_DELAY[10];
/// `$B64A+11`: the `$B553` frame count (six groups `$52..$57`).
pub const STRAIGHT_ANIM_FRAMES: u8 = ANIM_DELAY[11];
/// `$B6D3`: kind-7/19 walker frame by `$04AC` facing (0/1 right, 2/3 left).
pub static WALKER_FRAMES: [u8; 4] = [0x1C, 0x1C, 0x1F, 0x1F];
/// `$B4DE`: kind-26 dwell timer by player index.
pub static SPIN_DWELL: [u8; 7] = [0x50, 0x50, 0x40, 0x30, 0x20, 0x10, 0x10];
/// `$B4E5`: kind-26 return timer by player index.
pub static SPIN_RETURN: [u8; 7] = [0x60, 0x60, 0x50, 0x40, 0x30, 0x20, 0x20];
/// `$B791`: kind-23 sprite by `$048C` (`3F`/`40` for the small core).
pub static CORE_FRAMES: [u8; 8] = [0x3F, 0x40, 0x60, 0x60, 0x70, 0x80, 0x90, 0xA0];
/// `$B84C`: kind-23 `$046C` hits before it breaks, by rank `$17`.
pub static CORE_HITS: [u8; 8] = [0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x08];
/// `$B781`: kind-23 `$03BC` sub-state threshold, by rank `$17`. `$048C`
/// toggles at it (`$B854`); the analysis note that read `$B781` as
/// `02 03 04 05 06 07 08 08` mistook the neighbouring `$B84C`.
pub static CORE_SUBSTATE: [u8; 8] = [0x3C, 0x37, 0x32, 0x2D, 0x28, 0x23, 0x23, 0x23];
/// `$B793`: kind-23 vertical step (8.8 low byte) by rank `$17`.
pub static CORE_STEP: [u8; 8] = [0x60, 0x60, 0x70, 0x80, 0x90, 0xA0, 0xB0, 0xC0];
/// `$B789`: kind-23 X step (8.8 low byte) by rank `$17`; the instruction
/// subtracts `1 + $B789[$17]/256` px per frame.
pub static CORE_X_STEP: [u8; 8] = [0x00, 0x08, 0x10, 0x18, 0x20, 0x20, 0x20, 0x20];

/// `$AEBB`: the `$47++ & $0F` alternation picks subtype 7, else 6.
pub static DEBRIS_SUBTYPES: [u8; 2] = [0x07, 0x06];
