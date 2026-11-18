//! Hit boxes, timers and the hazard scripts.

/// `$BFC8`: shot hit test X offset by shot kind.
pub static SHOT_BOX_X: [u8; 4] = [0x08, 0x10, 0x08, 0x08];
/// `$BFCC`: shot hit test width by shot kind.
pub static SHOT_BOX_WIDTH: [u8; 4] = [0x10, 0x30, 0x10, 0x10];
/// `$BFD0`: shot hit test Y offset by shot kind.
pub static SHOT_BOX_Y: [u8; 4] = [0x08, 0x08, 0x08, 0x00];
/// `$BFD4`: object box width by `$0460` size class.
pub static ENEMY_BOX_WIDTH: [u8; 4] = [0x10, 0x20, 0x30, 0x10];
/// `$BFD8`: object box height by `$0460` size class.
pub static ENEMY_BOX_HEIGHT: [u8; 4] = [0x10, 0x20, 0x30, 0x02];
/// `$C1FC`: player box width by enemy-shot type `$0176`.
pub static PLAYER_BOX_WIDTH: [u8; 2] = [0x10, 0x16];
/// `$C200`: player box height by enemy-shot type `$0176`.
pub static PLAYER_BOX_HEIGHT: [u8; 2] = [0x08, 0x12];
/// `$C204`: player box X offset by enemy-shot type.
pub static PLAYER_BOX_X: [u8; 2] = [0x00, 0x06];
/// `$C204`: player box Y offset by enemy-shot type.
pub static PLAYER_BOX_Y: [u8; 2] = [0x00, 0x08];
/// `$B017`: motion byte for the shell spawned by `$AF92`, by player/stage.
pub static CHILD_MOTION: [u8; 5] = [0x64, 0x46, 0x3C, 0x37, 0x32];
/// `$C930`: Big Core hatch timer by player index.
pub static BIG_CORE_TIMER: [u8; 5] = [0x50, 0x4B, 0x46, 0x41, 0x3C];
/// `$9A28`: checkpoint timer by stage (`$19`).
pub static CHECKPOINT_TIMER: [u8; 8] = [0x03, 0x03, 0x04, 0x04, 0x05, 0x05, 0x06, 0x06];
/// `$98F0`: scroll high byte that ends the stage, by stage.
pub static STAGE_END: [u8; 7] = [0x0E, 0x0E, 0x0E, 0x0E, 0x0D, 0x0C, 0x0D];
/// `$C4EE`: stage-1 eruption spawn X, indexed by `$69 & 1`.
pub static ERUPTION_X: [u8; 2] = [0x38, 0xB8];
