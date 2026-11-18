//! The simulation: the stage-1 object engine, player, weapons, scoring and HUD.
//!
//! One call to [`Game::step`] is exactly one NTSC frame. All arithmetic is
//! integer: positions are 16.8 fixed point with the integer and fraction bytes
//! kept apart, exactly like `$0320/$0340` (Y) and `$0360/$0380` (X) in the
//! original. There are no clocks, threads, allocations in the frame loop or
//! random sources other than a seeded starfield, so a replay is reproducible.
//!
//! ## What is modelled from the disassembly
//!
//! | item | original | here |
//! | --- | --- | --- |
//! | player delta | `$A000`: `min($40+2,16)>>1` | [`powerup::PowerMeter::speed_delta`] |
//! | clamps | `$A022`..`$A075`: X 16..240, Y 16..192 | [`Game::update_player`] |
//! | fire cadence | `$A0F6`..`$A166`, `$35 = 20` | [`Game::fire`] |
//! | shot motion | `$A1E4`..`$A22C` (7 px, despawn 248) | [`Shot`] |
//! | option trail | `$A080`..`$A0A4`, ring 24, lag 11 | [`Trail`] |
//! | option animation | `$A0C7`: `4 + ((counter>>3)&1)` | [`Game::update_player`] |
//! | sprite-frame timer | `$A0A7`: reset at 8 | `Player::anim_timer` |
//! | death timer | `$C1DA`: `$4C = 120` | [`Phase::Dying`] |
//! | scroll | `$98E1`: `$3D += $80` | [`stage::Scroll`] |
//! | spawner | `$A2BA`/`$A32F`, `$A391`, `$A3AB`, `$A3DE` | [`stage::Spawner`] |
//! | object slots | 10 allocated slots in 32-byte pages | [`SLOT_COUNT`] |
//! | enemy AI | `$ADA5` dispatch `$AE16`, handlers `$AED7`..`$C900` | [`Game::update_objects`] |
//! | enemy fire | `$BCAF`/`$83BA`/`$BDF4` aimed 8.8 velocity | [`EnemyBullet`] |
//! | object boxes | `$BFD4`/`$BFD8` size classes, `$C1FC`/`$C200` ship boxes | [`Game::collide`] |
//! | destruction | `$BE8D`: hit becomes kind-2 debris | [`Game::destroy`] |
//! | stage thresholds | `$9A30`/`$9A38` and `$9A28` transition timer | [`stage::transition_frames`] |
//! | score | BCD add `$84AE` | [`Bcd`] |
//!
//! ## Deliberate approximations (see also the report)
//!
//! * The `$A65C` terrain pieces become real objects with the `$A3AB` field
//!   mapping, their `$AED7` drift and the `$BFD4`/`$BFD8` boxes. The handlers
//!   that the terrain types select (`$B6DB` ground walker, `$B741` ceiling
//!   walker, `$AF28`/`$AF82` turrets, `$B020`/`$B092` aiming turrets, `$AF0A`
//!   background pieces) are ported; their rendering still reuses the small
//!   enemy sprite.
//! * The `$AE93` debris path table and the `$B47A`/`$B4F7`/`$B553` region
//!   handlers are reduced to their documented drift/aim/descend behaviour.
//! * The shield (`$46`) is a hit counter; the port also draws the ROM's ring
//!   (`$8B70`: groups 90-93, tiles `$B7`/`$B9`/`$C1`/`$C3`, attribute `$21`,
//!   ship `(+15, -1)`, cycling `($02 >> 2) & 3`).
//! * The 5-byte command path spawns the Big Core for section 2 exactly as
//!   `$A460` does, but the stage-1 stream contains no command record. Stage 1's
//!   end scene runs the `$81`/`$82`/`$83`/`$84` state sequence: the 768-frame
//!   eruption after `$3F = $0C`, then the core at `$3F = $0D`. Its seven-hit
//!   damage follows `$B92C`; the firing cadence is a port-side cycle.
//! * Player shots also die on solid background (`$C29F`/`$C3A9`), even though
//!   `$C39D` (the player probe) reads the all-zero walker map in stage 1.
//! * Title/attract starts first; the title screen is `rom.backgrounds.title`.
//! * The title is drawn as the extracted 32x30 nametable with CHR bank 3 and
//!   its own palette. The 1P/2P cursor is a sprite the nametable dump cannot
//!   hold, so its position (y = 168/184) is a port-side approximation; the
//!   Select edge moves it, Start locks `$0F` into `$03` and enters the stage.
//!   The locked mode picks [`crate::twoplayer::Players::One`] or
//!   [`crate::twoplayer::Players::Two`].
//! * The no-input attract entry reproduces the oracle's timing, not its
//!   contents: the ROM leaves title state 0 at frame 133 and reports the stage
//!   handler (state 5) at frame 264. [`Phase::Attract`] holds the extracted
//!   title screen for that window and the dump reports the oracle's
//!   `Other(1)`/`Other(2)`/`Other(4)` spellings; the ROM's demo scroll and its
//!   exact state-1..4 durations are not reproduced. A Start edge before frame
//!   133 skips straight to the normal six-frame stage init.
//! * Rendering builds an OAM-like list (`$8B15`) and applies the real PPU
//!   rules: 64 entries, eight sprites per scanline in OAM order, attribute
//!   bit 5 behind the background and PPUMASK bit 2 left clipping. Every
//!   `$0120` sprite type is expanded through the cartridge group tables
//!   (`crate::sprites`), so the ship, ring enemies, large groups and debris
//!   composite their real multi-sprite definitions. The order (HUD bar, ship,
//!   objects ascending, then the separated shot and effect pools) still
//!   approximates the original's rotating OAM write pointer. The PPU display
//!   lags the sampled simulation state by two frames, as on hardware, so the
//!   two previous OAM lists are kept.

use crate::art;
use crate::assets::Assets;
use crate::audio::{AudioQueue, Sound};
use crate::ending_ship::{self, EndingShip};
use crate::font;
use crate::hud::{self, Hud, ScoreDelay};
use crate::input::Buttons;
use crate::mods::Mods;
use crate::palettes;
use crate::powerup::PowerMeter;
use crate::stage::{self, Emission, Phase, Scroll};
use crate::stage1_tables as tables;
use crate::stage3_enemies;
use crate::stage6_beams::Beams;
use crate::stage_bg;
use crate::stage_catalog;
use crate::stage_collision::terrain_probe_in;
use crate::twoplayer::{Players, TwoPlayer};
use crate::video::{Framebuffer, Sprite, SpriteList, FNV_PRIME, HEIGHT, WIDTH};

#[path = "ai.rs"]
mod ai;

#[path = "objects.rs"]
mod objects;
use objects::slope;

#[path = "player.rs"]
mod player;

#[path = "render.rs"]
mod render;

#[path = "snapshot_impl.rs"]
mod snapshot_impl;

#[path = "frame.rs"]
mod frame;

#[path = "stage_flow.rs"]
mod stage_flow;

#[path = "types.rs"]
mod types;

#[path = "construct.rs"]
mod construct;

#[path = "second_ship.rs"]
mod second_ship;

use self::objects::Object;
#[cfg(test)]
use self::player::Trail;
use self::player::{OptionDrone, Player, Shot};
use self::second_ship::Ship;
use self::stage_flow::{Clear, Endgame, STAGE_CLEAR_CUE};
use self::types::*;

/// Documented object pages are 32 bytes (`$0100-$04FF`).
pub const SLOT_COUNT: usize = 32;
/// The original allocates slots 0-9 only (`$A3AB: LDX #$09`).
const LIVE_SLOTS: usize = 10;
/// `$0600` has four `$30`-byte records (`$0600/$0630/$0660/$0690`).
const CANNON_SLOTS: usize = 4;
/// The cartridge's `$A102: LDX $45` loop: with options capped at two, the ship
/// plus two drones are three emitters.
const EMITTERS: usize = 3;
/// Shot slots: two shots plus a missile per emitter (`$0123-$012B`).
const MAX_SHOTS: usize = EMITTERS * 3;
/// The port-only four-option loadout needs its own larger pool; it stays
/// behind [`Mods::extra_options`] so the cartridge layout is the default.
const EXTENDED_EMITTERS: usize = 5;
const EXTENDED_MAX_SHOTS: usize = EXTENDED_EMITTERS * 3;
/// Enemy bullets: the cartridge's `$0136` pool holds ten (`$BC53` scans
/// slots 9 down to 0).
const MAX_ENEMY_BULLETS: usize = 10;
/// `$BC2C`: X muzzle offsets indexed by `$0496` (offset 0 is `(0, 0)`).
const FIRE_MUZZLE_X: [i8; 9] = [0, -8, 8, -8, 8, 0, -8, 8, 0];
/// `$BC35`: Y muzzle offsets indexed by `$0496`.
const FIRE_MUZZLE_Y: [i8; 9] = [0, -8, -8, 8, 8, -8, 0, 0, 8];
/// Explosion effects, a port-side bound.
const MAX_EXPLOSIONS: usize = 24;
/// Title/stage background black (NES master palette index).
const BLACK: u8 = 0x0F;
/// Frame at which a no-input run leaves the title screen. The oracle leaves
/// state 0 at frame 133 (`docs/analysis/stage1_flow.md`).
const TITLE_ATTRACT_ENTRY: u16 = 133;
/// Re-entry title: `$12 = $FE` two frames after `$00 = 0`, then 127
/// `DEC $12` pairs and the zero-test frame. The boot title keeps 133 because
/// the reset warm-up delays the setup to frame 5.
const TITLE_ATTRACT_REENTRY: u16 = 130;
/// `$4C/$4D` in `$00 = 1`: the title-to-demo countdown. The oracle enters
/// `$00 = 2` at frame 390, 256 frames after leaving the title at 133.
const DEMO_TRANSITION_FRAMES: u16 = 256;
/// `$1B` 1..4 in the demo's stage handler: frame 392 places the ship and
/// frame 419 reports `$80`. One frame shorter than the Start path because the
/// demo's `$82BF` reset occupies frame 391.
const DEMO_INTRO_FRAMES: u16 = 27;

/// The demo ship starts beside `1 PLAYER`: `$8299` writes `$0360 = $50` and
/// `$0320 = $86`, the same place as the title cursor.
const DEMO_SHIP: (u8, u8) = (0x50, 0x86);
/// `$00 = 3` countdown after Start on the demo: frames 182..262 at the
/// default Start press on frame 180.
const CONTINUE_FRAMES: u16 = 81;
/// `$1B` 0..4 inside the stage handler (`$00 = 5`): frames 264..291. The
/// first frame leaves the ship position at zero, the second places it.
const STAGE_INTRO_FRAMES: u16 = 28;
/// The `$0D = 5` blank countdown inside `$9C1E`; the scroll step is gated on
/// `$0D == 0`, so the first scroll frame is 32 frames after entering the
/// stage handler.
const STAGE_SCROLL_HOLD: u8 = 5;
/// `$9B75`: PPU `t = $4020`, i.e. coarse Y 1 and fine Y 4 = 12 pixels.
const STAGE_MAP_Y: i32 = -12;
/// `$93`: the shared intro cue. Every stage entry leaves `$1C` on it and
/// switches to the stage theme at `$8354[$19]`.
const SHARED_INTRO_MUSIC: u8 = 0x93;
/// `$A5`: the boss track `$9BE7` queues for the whole ending.
const ENDING_MUSIC: u8 = 0xA5;
/// `$CF35`: the ending text record comes from the pack (`ending_text_record`);
/// its first two bytes are the PPU address (`$22C8`: nametable 0, row 22,
/// column 8), then the 16 CHR-bank-3 tiles, then the `$FE` terminator. The port
/// draws the same tiles from pack bank 6 (= CNROM bank 3, low pattern table).
/// `$8C` starts at oracle 7052; the first text step is at 7538.
const ENDING_TEXT_FIRST: u16 = 7538 - 7052;
/// `$CE8E` uses `$4E = 8` plus its eight decrements: one character per 9
/// frames, 16 steps.
const ENDING_TEXT_STEP: u16 = 9;
/// `$CE8E` writes the 16-tile line (`$CF35[2..18]`).
const ENDING_TEXT_CHARS: u8 = 16;
/// `$8444` from `$CE8E`: +10000 and the score extend at oracle 7998.
const ENDING_SCORE_FRAME: u16 = 7998 - 7052;
/// `$8C` lasts 1202 frames (7052-8253); `$4C` then reaches `$8D` at 8254.
const ENDING_DISPLAY_FRAMES: u16 = 1202;
/// The text row `$22C8`: column 8, row 22 with the `$4020` scroll (-12 px).
const ENDING_TEXT_X: i32 = 64;
const ENDING_TEXT_Y: i32 = 22 * 8 - 12;
/// Pack bank 6 is CNROM bank 3, the text font's low pattern table.
const ENDING_TEXT_BANK: u8 = 6;
/// `$CF35`'s row runs on background palette 0 (`$23EA = $10`): `0F 12 30`.
const ENDING_TEXT_PALETTE: [u8; 4] = [0x0F, 0x12, 0x30, 0x30];
/// `$9B38` on the `--stage` boot: the handler runs three gameplay frames
/// (`$A2BA`, `$BBB1`, `$ADA5`) before the first `$3E` step. The oracle's
/// first stage-4 object spawns two frames after `Playing` (29259 in the
/// poked run) and the scroll first moves five frames in, so a boot that
/// skipped them would put the whole object table one column (6 px) behind.
const STAGE_BOOT_WARMUP: u8 = 3;
/// The tail's hardware CHR bank (`gradius_pack` bank 4). The cut column is
/// the stage's [`stage_catalog::StageInfo::tail_column`].
const STAGE_TAIL_BANK: u8 = 2;
/// `$C67E`: scripted-object period for `$3A = 0` (stage 3's small core).
const STAGE3_SCRIPT_PERIOD: u8 = 0x28;
/// `$C6C4[$3A]`: `$3A = 0` sprite for the `$C680` scripted object.
const STAGE3_SCRIPT_SPRITE: u8 = 0x3F;
/// `$C6C6[$3A]`: `$3A = 0` kind for the `$C680` scripted object.
const STAGE3_SCRIPT_KIND: u8 = 0x97;

/// `$C651`: `$C64D` counts `$68` to `$28` before reading a `$C674` pair.
const STAGE5_SCRIPT_PERIOD: u8 = 0x28;
/// `$C674`: the four `(b2, y)` pairs the `$C64D` script cycles through.
const STAGE5_SCRIPT: [u8; 8] = [0x02, 0x80, 0x00, 0x40, 0x01, 0x80, 0x00, 0xC0];
/// `$CA43`: the `$14` morph threshold by rank.
const LARGE_MORPH_HP: [u8; 7] = [10, 12, 14, 16, 18, 20, 22];
/// `$CA4A`: the `$14` death threshold by rank.
const LARGE_DEATH_HP: [u8; 7] = [20, 24, 28, 32, 36, 40, 44];
/// `$CA51`: the `$14` vertical aim fraction by rank.
const LARGE_AIM_FRAC: [u8; 7] = [0x40, 0x48, 0x50, 0x60, 0x70, 0x80, 0x90];
/// `$CBC4`: the `$0600` cannon fire period by rank.
const CANNON_PERIOD: [u8; 7] = [40, 35, 30, 25, 25, 25, 25];
/// `$BEE4`: the hits a `$0600` cannon takes by rank.
const CANNON_HIT_HP: [u8; 9] = [2, 2, 3, 4, 5, 6, 7, 8, 9];
/// The six sprite parts a `$0600` record places (`$CC7C` plus the five
/// `$CD5B` iterations); part 2 is the only weak point (`$BF2D`).
const CANNON_PARTS: usize = 6;
/// `$CC1D`: part-0 X offset by `4 * $0460 + $0601`.
const CANNON_X_OFFSET: [u8; 8] = [0x04, 0x04, 0x04, 0x04, 0x0C, 0x0C, 0x0C, 0x0C];
/// `$CC25`: part-0 Y offset by `4 * $0460 + $0601`.
const CANNON_Y_OFFSET: [u8; 8] = [0xFC, 0x10, 0xFC, 0x10, 0xFC, 0x20, 0xFC, 0x20];
/// `$CC19`/`$CC1B`: the sway phase limits by cannon index.
const CANNON_PHASE_MIN: [u8; 2] = [0x4A, 0x6A];
const CANNON_PHASE_MAX: [u8; 2] = [0x56, 0x76];

/// The two title menu entries (`$2A`/`$2B` in the original).
const MENU_ENTRIES: u8 = 2;
/// Starfield tint for the slow layer.
const STAR_COLOR: u8 = 0x12;
/// Starfield tint for the bright layer.
const STAR_BRIGHT: u8 = 0x30;

type Rect = (i32, i32, i32, i32);

/// Top-level game state. The Start pause is a flag on [`Game`], not a state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Attract/title, only entered when a title screen has been extracted.
    Title,
    Playing,
    GameOver,
}

/// The whole game. Fixed storage only for simulation entities.
pub struct Game {
    assets: Assets,
    framebuffer: Framebuffer,
    /// The previous frame's OAM list. The cartridge builds OAM in the NMI at
    /// the start of a frame, so what the PPU displays is one frame behind the
    /// simulation state that the harness samples.
    last_sprites: SpriteList,
    /// One frame older than [`Self::last_sprites`]; the OAM the PPU shows while
    /// the harness samples the state is two frames behind it.
    earlier_sprites: SpriteList,
    rng: Rng,
    player: Player,
    /// Player two's kit while [`Mods::simultaneous_2p`] runs; unused (and
    /// untouched) in every other mode.
    p2: Ship,
    objects: [Object; SLOT_COUNT],
    /// `$0600`: the four attached-cannon records (`$14` parents).
    cannons: [Cannon; CANNON_SLOTS],
    options: [OptionDrone; 4],
    shots: [Shot; EXTENDED_MAX_SHOTS],
    enemy_bullets: [EnemyBullet; MAX_ENEMY_BULLETS],
    enemy_bullet_count: usize,
    /// `$034C`/`$038C` on the ten `$0136` proxy slots. `$CBCB` writes only the
    /// integer position bytes, so the sub-pixel fractions survive a despawn
    /// and the next shot into the same slot starts with them.
    bullet_slot_frac: [(u8, u8); MAX_ENEMY_BULLETS],
    explosions: [Explosion; MAX_EXPLOSIONS],
    explosion_count: usize,
    stars: [Star; 96],
    power: PowerMeter,
    score_bcd: Bcd,
    /// `$07E0-$07E2`: the high score, a 6-digit BCD buffer that survives a new
    /// game. Cold boot seeds it with `$50` in the middle byte (`$8052`), i.e.
    /// 005000, displayed as `0050000` with the constant trailing zero.
    high_score_bcd: Bcd,
    scroll: Scroll,
    /// Horizontal latch lag for the frame being rendered: 1 while the scroll
    /// steps (`$98E1`), 0 while `$5B`/the boss lock holds it, where the latch
    /// already equals `$3F:$3E`.
    scroll_lag: i32,
    /// `$B563`: the stage-7 wall released the scroll on the frame just run, so
    /// the first step after the hold draws the held latch, not `position - 1`.
    wall_resume: bool,
    spawner: stage::Spawner,
    phase: Phase,
    section: u8,
    /// `$1C`: the music id latch. Every stage starts on the shared intro
    /// `$93` and switches to `$8344[$19]` at `$3F = $8354[$19]`.
    music: u8,
    stage_clock: u32,
    kills: u32,
    banner: u16,
    prev_fire: bool,
    prev_start: bool,
    /// `$47`: alternates the `$AE93` background fold-in subtype.
    debris_folds: u8,
    /// Frames spent on the title screen, used for the automatic attract entry.
    title_timer: u16,
    /// Frame count at which this title entry leaves for attract: 133 at boot
    /// and 130 on every game-over/demo re-entry (`$12 = $FE` roll-in).
    title_attract_at: u16,
    /// `$4C/$4D` in `$00 = 1`: frames left before the demo starts.
    attract_countdown: u16,
    /// `$5F`: stage-1 volcano (`$0F`/`$10`) kills and stage-3 turret kills.
    volcano_kills: u8,
    /// `$39`: stage clear gate (four volcanoes or ten turrets).
    stage_clear_ready: bool,
    /// World-keyed live nametable overlay written by `$C9B4`.
    cells: Vec<stage3_enemies::Cell>,
    /// `$0500`/`$0600` bytes written by the turret death (`$C81C`). Stage 3
    /// has no extracted collision map yet, so the port keeps them as data.
    #[allow(dead_code)]
    collision_cells: Vec<stage3_enemies::CollisionPatch>,
    /// Destructible background cells (`$C329`) that shots have broken, one
    /// bit per tile row for each world column. The map byte is cleared and the
    /// nametable cell blanked in the original; the port keeps the world-keyed
    /// edit and applies it to the probe and to the background.
    broken: Vec<u32>,
    /// Cells broken this frame as `(column, row)`. The VRAM write waits for the
    /// next NMI, so the background still shows them until the next frame.
    fresh_breaks: Vec<(u16, u8)>,
    /// `$0E`: bytes in this frame's VRAM command queue.
    vram_queue_len: u8,
    /// `$5D`: a spawn/effect happened this frame.
    spawn_busy: bool,
    /// `$09`: the demo flag. Only the attract path sets it, so the `$9CB1`
    /// table can never drive a real game.
    demo: bool,
    /// `$31`: the demo input table offset (even byte index).
    demo_index: u16,
    /// `$30`: video-frame pairs left in the current demo record.
    demo_ticks: u8,
    /// The current record's raw `$07` byte.
    demo_button: u8,
    /// `$05` bit 5: the Select edge that also ends the demo.
    prev_select: bool,
    /// `$0F`: title menu selection, 0 = 1 PLAYER, 1 = 2 PLAYERS. Only the
    /// Select edge (`$8231`) moves it; Start locks it into [`Players`].
    menu_selection: u8,
    /// `$03` locked by Start: `$40`/`$70`, i.e. [`Players::One`] or
    /// [`Players::Two`].
    players: Players,
    /// `$18`/`$0A`/`$20`/`$21`: the alternating-turn machine. The stage stays
    /// single and shared; only the player state and the turn are here.
    two: TwoPlayer,
    /// Title menu cursor blink phase.
    menu_blink: u8,
    /// `$8212` applies `$0F` one frame after the sampled Select edge: the
    /// ROM's NMI sets `$05` a frame late.
    menu_select_pending: u8,
    /// `$5B`: the stage has stopped scrolling (`$B563`). Every `$AED7` drift
    /// handler and the scroll step are gated on it.
    scroll_stop: bool,
    /// One-frame delay between the Start edge in the demo and `$00 = 3`.
    continue_pending: u8,
    /// `$0D` blank countdown at the start of normal play; the scroll step is
    /// gated on it (`$9A87`).
    scroll_hold: u8,
    /// `$15` (Start pause) and the steps left before an edge flips it.
    paused: bool,
    pause_pending: u8,
    /// `$3B`/`$3C`: the Konami-code reward counter per player. Boss kills
    /// increment it, a code decrements it, and a negative value blocks the code.
    pause_code: [u8; 2],
    /// `$05` as the pause handler sees it: the edges of the previous step.
    pause_edges: u8,
    /// Extra PPUMASK blank frames after the stage intro: the oracle keeps the
    /// screen black for the first five playing frames (through f295).
    entry_blank: u8,
    /// The controller latch: `$81B7` reads the pad during NMI and `$9FF6`
    /// consumes the previous frame's `$07`, so movement and fire are one
    /// frame behind the harness input. In 2P this is the current player's pad.
    latched_input: Buttons,
    /// The previous frame's raw pads, so a handoff starts on the incoming
    /// player's `$05,X`/`$07,X` instead of the outgoing player's.
    latched_p1: Buttons,
    latched_p2: Buttons,
    /// P2's fire edge in simultaneous 2P; P1 keeps using `prev_fire`.
    prev_fire_p2: bool,
    /// The previous frame's raw pads, for the simultaneous continue scan.
    prev_pad_p1: Buttons,
    prev_pad_p2: Buttons,
    /// Port-side guard so the stage-1 boss is spawned once.
    boss_spawned: bool,
    /// Dev-only: keep the player alive so an analysis run can reach the boss.
    dev_keep_alive: bool,
    /// A lethal hit was suppressed by [`Self::dev_keep_alive`] this frame.
    /// The oracle's keep-alive poke restores `$1B = $80` after the frame, so
    /// the `$A0` the hit wrote still cancels the `$81` stage-clear latch the
    /// frame had armed. [`Game::normal_frame`] uses this to skip that arming.
    kept_hit: bool,
    /// Opt-in deviations from ROM behaviour; all false is cartridge parity.
    mods: Mods,
    /// `$1B = $81`/`$82`/`$83`/`$84` boss sequence: 0 idle, 1 eruption,
    /// 2 waiting for `$3F = $0D`, 3 spawn this frame, 4 spawned.
    boss_stage: u8,
    /// Frames left in the 768-frame eruption (`$82`).
    boss_timer: u16,
    /// `$69`: stage-1 eruption hazard script index.
    hazard_index: u8,
    /// `$68`: `$C680` scripted-object counter.
    script_counter: u8,
    /// Frames left before the next stage after the core dies.
    boss_clear: u16,
    /// Skip the countdown on the frame the core sets it (`$1B++` runs before
    /// the scroll step), so the 514-frame window ends on the next stage frame.
    boss_clear_armed: bool,
    /// `$86`/`$8E`/`$90` scroll after a `$39` core kill (stage 3).
    clear: Clear,
    /// `$1B = $81..$90` on the no-core stages (stage 6) and the stage-7
    /// ending entry.
    endgame: Endgame,
    /// `$28`: the loop counter incremented by `$9865`.
    loop_count: u8,
    /// `$1B` while [`Endgame::Ending`] runs: `$88`..`$8D`.
    ending_step: u8,
    /// Frames left in the current ending step.
    ending_timer: u16,
    /// Frames since the `$8C` display loop started (7052).
    ending_clock: u16,
    /// Revealed characters of the 16-tile `$22C8` line.
    ending_text: u8,
    /// The `$8444` +10000 and score extend were paid.
    ending_paid: bool,
    /// The `$CF90` ending nametable, built by `$9D88` during `$8B`.
    ending_screen: Option<crate::ending_screen::EndingScreen>,
    /// `$987F`/`$BB09`: the two ending sprites, `(0x74,0x80)` mothership and
    /// the kind-`$28` ship `$8C` walks over `$BB7C`.
    ending_ship: EndingShip,
    /// `$66`: the dedicated `$CD9F` erase index (0 -> 88), never the port's
    /// scratch byte of the same name.
    wall_erase: u8,
    /// `$0136/$0336/$0376`: stage 6's ten beam boxes. They are not `$030C`
    /// objects, but `$0136 = $8D` is also their sprite group, so they draw as
    /// red balls; `$C6D8` fills them during the `$82`
    /// window and `$BF6F`/`$C222` test shots and the ship against them.
    beams: Beams,
    /// `$3A`: the `$C680` bonus wave is armed by the `$992E` clear branch.
    bonus_wave: bool,
    /// The scroll stops at `$0D00` for the core fight.
    scroll_locked: bool,
    /// Frames left in the game-over sequence after the lives counter wraps.
    game_over_timer: u16,
    /// The next step switches to the title (one frame after the world reset).
    title_pending: bool,
    /// `$24`: `min($3F & $0E, 8)` captured when the death starts.
    death_checkpoint: u8,
    /// `$C0C1` death effect; drawn from [`DeathBurst::age`].
    death_burst: Option<DeathBurst>,
    /// Simultaneous 2P: frames left before each ship respawns after a death.
    simul_respawn: [u8; 2],
    /// `$8479`: the score digits lag the BCD value by 5/4/3/8 frames.
    score_delay: ScoreDelay,
    /// The next rendered frame redraws the HUD from the incoming player's
    /// bank without the queue delay (`$9B38` rewrites the row on the handoff).
    score_switch: bool,
    pub state: State,
    pub frame: u64,
    /// `$02`: the cartridge's NMI frame counter. It is not the same as
    /// [`Game::frame`] because the NMI body is skipped when the previous
    /// handler overran (`$8073` tests `$04`), so `$02` holds on a lag frame.
    /// The port models the entries it can see: one per stage entry (the
    /// `$9B38` upload) and one per respawn. The boot title's missed count is
    /// folded into the power-on value.
    nmi_frame: u8,
    /// The next step is a `$02` lag frame, so [`Game::nmi_frame`] holds.
    nmi_lag: bool,
    /// Decimal mirror of [`Game::score_bcd`], for frontends and tests.
    pub score: u32,
    pub lives: u8,
    pub audio: AudioQueue,
}

impl Game {
    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn assets(&self) -> &Assets {
        &self.assets
    }

    pub(crate) fn rom(&self) -> &'static crate::rom::RomData {
        self.assets
            .rom
            .expect("the pack has no ROM data; run scripts/extract-data")
    }

    /// `$9B82`: ship Y and X from the pack's per-section table.
    fn ship_start(&self, section: u8, scroll_high: u8) -> (u8, u8) {
        crate::stage::ship_start(self.rom(), section, scroll_high)
    }

    /// Read-only meter, for frontends that draw their own overlays.
    pub fn power(&self) -> &PowerMeter {
        &self.power
    }

    /// `$03`: the mode locked by Start (1P or 2P).
    pub fn players(&self) -> Players {
        self.players
    }

    pub fn menu_selection(&self) -> u8 {
        self.menu_selection
    }

    /// `$18`: the player whose ship is on screen and whose pad drives it.
    pub fn current_player(&self) -> usize {
        self.two.current
    }

    /// The per-player bank (`$20`/`$21`, `$07E4`/`$07E8`, checkpoints).
    pub fn two_player(&self) -> &TwoPlayer {
        &self.two
    }

    /// Three BCD bytes, most significant first (`$07E4-$07E6`).
    pub fn score_bcd(&self) -> [u8; 3] {
        self.score_bcd.digits
    }

    /// The 6-digit BCD high score (`$07E0-$07E2`) as a decimal value.
    ///
    /// The HUD prints it with the same constant trailing zero as the score, so
    /// the cold-boot value 5000 is shown as `0050000`.
    pub fn high_score(&self) -> u32 {
        self.high_score_bcd.value()
    }

    /// Three BCD bytes, most significant first (`$07E0-$07E2`).
    pub fn high_score_bcd(&self) -> [u8; 3] {
        self.high_score_bcd.digits
    }

    /// `$1C`: the live music id. The shared intro `$93` until `$3F` reaches
    /// `$8354[$19]`, then the stage theme; the stage handoff resets it to
    /// `$93` (`$96C4` leaves `$1C` alone).
    pub fn music(&self) -> u8 {
        self.music
    }

    /// Destructible background cells (`$C329`) broken so far in this stage.
    pub fn broken_cells(&self) -> u32 {
        self.broken.iter().map(|mask| mask.count_ones()).sum()
    }

    /// Current scroll position `$3F:$3E` and sub-pixel byte `$3D`.
    pub fn scroll(&self) -> Scroll {
        self.scroll
    }

    /// `$10`: PPUCTRL shadow. The stage init writes `$A8` (`$9B79`) and
    /// `$9A70` folds `$3F` bit 0 into the nametable bits.
    pub fn ppu_control(&self) -> u8 {
        tables::STAGE_PPUCTRL | self.scroll.nametable()
    }

    /// FNV-1a over the framebuffer plus the run state that is not visible in
    /// it (score, lives, section, scroll, frame).
    pub fn state_hash(&self) -> u64 {
        let mut hash = self.framebuffer.frame_hash();
        let mut mix = |value: u64| {
            for byte in value.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
            }
        };
        mix(u64::from(self.score));
        mix(u64::from(self.lives));
        mix(u64::from(self.section));
        mix(u64::from(self.scroll.high));
        mix(u64::from(self.scroll.low));
        mix(self.frame);
        hash
    }

    /// Dev-only: suppress the player death sequence for analysis runs.
    ///
    /// Off by default, and it never changes normal play. Enabling it keeps the
    /// ship alive when a hit would take the lethal `$C1D0` path, so a scripted
    /// run can reach and compare the Big Core without the fixed checkpoint
    /// restart. This is the native counterpart of the oracle's `--keep-alive`
    /// analysis poke.
    pub fn dev_keep_alive(&mut self, on: bool) {
        self.dev_keep_alive = on;
    }

    /// Dev-only: start a normal run at the stage-1 ship start, skipping the
    /// title, intro, blank and scroll-hold phases.
    ///
    /// This is the native counterpart of starting a harness run and waiting
    /// ~300 frames for `$1B = $80`. It changes nothing about normal play; the
    /// power-up proof tools and tests use it to reach `Playing` immediately.
    pub fn dev_start_run(&mut self) {
        self.begin_stage(false);
        self.phase = Phase::Normal;
        self.scroll_hold = 0;
        self.entry_blank = 0;
        self.scroll_stop = false;
        self.place_player(stage::ship_start(self.rom(), 0, 0));
        // A fresh run must see the next button press as an edge.
        self.latched_input = Buttons::NONE;
        self.latched_p1 = Buttons::NONE;
        self.latched_p2 = Buttons::NONE;
        self.prev_fire = false;
        self.prev_start = false;
        self.prev_select = false;
    }

    /// Dev-only: mutable access to the power meter, for proof tools and tests
    /// that place the meter cursor without collecting six capsules.
    pub fn dev_power_mut(&mut self) -> &mut PowerMeter {
        &mut self.power
    }

    /// Dev-only: put a real kind-1 subtype-6 capsule in the first free live
    /// slot at the ship. The normal collision path consumes it and advances
    /// `$42`, exactly like a capsule left by a destroyed formation leader.
    pub fn dev_spawn_capsule_at_player(&mut self) {
        let Some(slot) = (0..LIVE_SLOTS).find(|&slot| !self.objects[slot].live()) else {
            return;
        };
        self.objects[slot] = Object {
            active: true,
            kind: 1,
            subtype: 6,
            x: self.player.x,
            y: self.player.y,
            sprite: 1,
            ..Object::default()
        };
    }

    /// Dev-only: active player bullets (normal or laser), excluding missiles.
    pub fn dev_bullet_count(&self) -> usize {
        self.shots
            .iter()
            .filter(|shot| shot.active && shot.kind != 3)
            .count()
    }

    /// Enable the port-only conveniences in [`Mods`]. The default is all off,
    /// which matches the cartridge.
    pub fn set_mods(&mut self, mods: Mods) {
        self.mods = mods;
    }

    /// The active [`Mods`] (all off unless [`Game::set_mods`] changed them).
    pub fn mods(&self) -> Mods {
        self.mods
    }

    /// Both ships fly at once: [`Mods::simultaneous_2p`] on a 2P game.
    ///
    /// Every other mode, including a 1P game with the mod on, keeps the
    /// cartridge's single ship and drawer.
    pub fn simultaneous(&self) -> bool {
        self.mods.simultaneous_2p && self.players == Players::Two
    }

    /// Advance exactly one frame.
    pub fn step(&mut self, input: Buttons) {
        self.step_players(input, Buttons::NONE);
    }

    /// One frame with both pads. `$81E3`: a 1P game ORs both (`$8203`), a 2P
    /// game feeds `$05,X`/`$07,X` from the player in `$18` only. On the title
    /// and the attract/continue windows either pad drives the menu.
    pub fn step_players(&mut self, p1: Buttons, p2: Buttons) {
        // The oracle's analysis poke rewrites `$46 = 5` every frame, so the
        // native side must pin the same shield for the boss rank to match.
        if self.dev_keep_alive {
            self.power.dev_set_shield(5);
        }
        let input = self.player_input(p1, p2);
        // `$81B7` writes `$07`; `$9FF6` reads it on the *next* NMI, so the
        // harness input reaches movement and fire one frame late.
        let control = self.latched_input;
        self.latched_input = input;
        self.latched_p1 = p1;
        self.latched_p2 = p2;
        let pad_p1_prev = self.prev_pad_p1;
        let pad_p2_prev = self.prev_pad_p2;
        self.prev_pad_p1 = p1;
        self.prev_pad_p2 = p2;
        let fire_pressed = control.fire && !self.prev_fire;
        let start_pressed = input.start && !self.prev_start;
        let p1_start_pressed = p1.start && !self.prev_start;
        let select_pressed = input.select && !self.prev_select;
        self.prev_fire = control.fire;
        self.prev_start = self.pause_start_level(input, p1);
        self.prev_select = input.select;

        self.frame += 1;
        self.fresh_breaks.clear();
        // `$80B6`: the NMI only increments `$02` when the previous handler
        // finished; a lag frame keeps the value.
        if self.nmi_lag {
            self.nmi_lag = false;
        } else {
            self.nmi_frame = self.nmi_frame.wrapping_add(1);
        }
        // `$82CD`/`$817E`: the title appears one frame after the game-over
        // world reset, without running another stage frame.
        if self.title_pending {
            self.title_pending = false;
            self.state = State::Title;
            self.set_lives(0);
            self.title_timer = 0;
            self.title_attract_at = TITLE_ATTRACT_REENTRY;
            self.continue_pending = 0;
            self.render();
            return;
        }
        match self.state {
            State::Title => {
                self.update_stars();
                self.menu_blink = self.menu_blink.wrapping_add(1);
                if self.title_timer == 0 {
                    // `$80DA`/`$824E`/`$8299`: the first title frame places
                    // the menu ship beside `1 PLAYER`.
                    self.place_player(DEMO_SHIP);
                }
                // `$8212`/`$8231`: the Select edge moves `$0F` 0 <-> 1.
                // Up/Down are not menu input.
                if select_pressed {
                    self.menu_selection = (self.menu_selection + 1) % MENU_ENTRIES;
                }
                if start_pressed {
                    self.begin_stage(false);
                } else {
                    self.title_timer = self.title_timer.saturating_add(1);
                    if self.title_timer == self.title_attract_at {
                        // No input: the ROM leaves the title on the `$12`
                        // roll-in (133 frames at boot, 130 on re-entry).
                        self.begin_stage(true);
                    }
                }
            }
            State::GameOver => {
                self.update_stars();
                self.update_explosions();
                if start_pressed {
                    self.new_run();
                    self.apply_start_stage();
                    self.state = State::Playing;
                }
            }
            State::Playing if self.paused => {}
            State::Playing => {
                // Start during the title demo keeps the `$00 = 3` boot, so its
                // `$9B38` frame is where a title start's `$82CD` would land:
                // re-root the run on `Mods::start_stage` there too.
                if !self.demo && self.phase == Phase::StageInit {
                    self.apply_start_stage();
                    self.phase = Phase::StageInit;
                }
                self.step_playing(control, fire_pressed, start_pressed, select_pressed)
            }
        }
        let edge = if self.simultaneous() {
            // Simultaneous 2P: either pad may enter the continue code.
            let p1_edge = Self::button_edges(p1, pad_p1_prev);
            if p1_edge != 0 {
                p1_edge
            } else {
                Self::button_edges(p2, pad_p2_prev)
            }
        } else {
            Self::button_edges(input, control)
        };
        let pads = [(p1, pad_p1_prev), (p2, pad_p2_prev)];
        let start = self.pause_edge(pads, start_pressed, p1_start_pressed);
        self.pause_tick(start, edge);
        // `$96EE`: the `$1B = $C0` window scans the `$977C` continue code on
        // the current player's `$05,X` edge. The port has no `$B0` sound
        // engine, so the scan covers the whole game-over window instead of
        // only the frames where the APU block is busy.
        if self.state == State::Playing && self.game_over_timer > 0 {
            if self.two.is_game_over() {
                self.two.scan_continue(edge);
            }
            self.game_over_timer -= 1;
            if self.game_over_timer == 1 {
                if self.two.continue_ready() {
                    self.resume_from_continue();
                } else {
                    self.reset_for_title();
                    self.title_pending = true;
                }
            }
        }
        self.render();
    }

    /// Leave the title and enter the stage.
    ///
    /// `attract` is the demo path the ROM takes on its own at frame 133
    /// (`$00 = 1`). Start during the demo moves to the `$00 = 3` continue
    /// countdown rather than starting the stage directly.
    fn begin_stage(&mut self, attract: bool) {
        // `$8225`: `$03 = $824C[$0F]` locks 1P/2P before `$82CD` starts the
        // run; the attract demo is always 1P.
        self.players = if attract || self.menu_selection == 0 {
            Players::One
        } else {
            Players::Two
        };
        self.new_run();
        if !attract {
            // Modified mode: a player start may begin further in.
            self.apply_start_stage();
        }
        self.state = State::Playing;
        self.continue_pending = 0;
        self.scroll_hold = 0;
        self.entry_blank = 0;
        self.demo = false;
        self.demo_index = 0;
        self.demo_ticks = 0;
        self.demo_button = 0;
        if attract {
            self.set_lives(0);
            // `$8299`: the transition places the demo ship beside `1 PLAYER`.
            self.place_player(DEMO_SHIP);
            self.attract_countdown = DEMO_TRANSITION_FRAMES;
            self.phase = Phase::Attract(0);
        } else {
            self.attract_countdown = 0;
            self.phase = Phase::Blank(tables::STAGE_BLANK_FRAMES);
        }
    }

    /// Reset a whole run (`$973E`/`$82CD`: three lives, score cleared).
    fn new_run(&mut self) {
        // `$82CD`: both slots get three lives, a zero score and the stage
        // start; in 1P P2's alive bit stays clear.
        self.two.start(self.players);
        self.score_bcd = Bcd::default();
        self.score = 0;
        self.set_lives(3);
        self.kills = 0;
        self.section = 0;
        self.stage_clock = 0;
        self.banner = 90;
        self.paused = false;
        self.pause_pending = 0;
        self.pause_code = [0; 2];
        self.pause_edges = 0;
        self.scroll_stop = false;
        self.volcano_kills = 0;
        self.stage_clear_ready = false;
        self.cells.clear();
        self.collision_cells.clear();
        self.broken.fill(0);
        self.continue_pending = 0;
        self.scroll_hold = 0;
        self.boss_spawned = false;
        self.boss_stage = 0;
        self.boss_timer = 0;
        self.hazard_index = 0;
        self.script_counter = 0;
        self.boss_clear = 0;
        self.boss_clear_armed = false;
        self.clear = Clear::None;
        self.endgame = Endgame::None;
        self.loop_count = 0;
        self.ending_step = 0;
        self.ending_timer = 0;
        self.ending_clock = 0;
        self.ending_text = 0;
        self.ending_paid = false;
        self.ending_ship = EndingShip::default();
        self.wall_erase = 0;
        self.beams.clear();
        self.bonus_wave = false;
        self.scroll_locked = false;
        self.game_over_timer = 0;
        self.menu_select_pending = 0;
        self.death_burst = None;
        self.simul_respawn = [0; 2];
        self.score_delay = ScoreDelay::with_pattern(&[5, 4, 3, 8]);
        self.power.reset_all();
        self.reset_section_objects();
        self.scroll = Scroll::default();
        self.spawner.restart(0, 0);
        self.phase = Phase::Blank(tables::STAGE_BLANK_FRAMES);
        self.place_player(stage::ship_start(self.rom(), 0, 0));
        self.place_second_ship();
        self.prev_fire_p2 = false;
    }

    /// `$20`: the P1 lives counter and its slot mirror.
    fn set_lives(&mut self, lives: u8) {
        self.lives = lives;
        self.two.slots[0].lives = lives;
    }

    fn reset_section_objects(&mut self) {
        self.objects = [Object::default(); SLOT_COUNT];
        self.cannons = [Cannon::default(); CANNON_SLOTS];
        self.options = [OptionDrone::default(); 4];
        self.shots = [Shot {
            active: false,
            kind: 0,
            x: Fixed::default(),
            y: Fixed::default(),
            age: 0,
        }; EXTENDED_MAX_SHOTS];
        self.enemy_bullet_count = 0;
    }
}

/// `$C39D`: is the background solid at this screen position?
///
/// The original converts the object X plus `$3E` into an attribute byte in the
/// nametable and masks it with `$C409[$row & 3]` (`03 0C 30 C0`). That selects
/// the 2-bit quadrant of the attribute for the tile row. The extracted screen
/// stores attributes the same way, so the mask applies unchanged. Without an
/// extracted screen the port has no collision map and returns false.
#[cfg(test)]
fn terrain_solid(x: i32, y: i32, scroll: Scroll) -> bool {
    terrain_probe_in(&crate::rom::testing::rom().collision.maps[0], x, y, scroll)
        .is_some_and(|(_, _, field)| field != 0)
}

#[cfg(test)]
#[path = "game/tests.rs"]
mod tests;
