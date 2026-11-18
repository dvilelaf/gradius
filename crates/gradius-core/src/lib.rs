//! The platform-agnostic game.
//!
//! This crate has no operating-system dependency, no I/O and no global state.
//! It advances in whole NTSC frames and writes RGBA pixels into a caller-owned
//! buffer, so the same code can back an SDL window, a wasm canvas, an iOS
//! `MTKView` or an Android `SurfaceView` without changes.
//!
//! All simulation state is integer fixed-point (1/256 of a pixel). That keeps
//! replays and differential tests bit-identical on every platform.

#![forbid(unsafe_code)]

pub mod art;
pub mod assets;
pub mod audio;
mod ending_screen;
mod ending_ship;
pub mod font;
pub mod game;
pub mod hud;
pub mod input;
pub mod mods;
pub mod palettes;
pub mod powerup;
pub mod rom;
pub mod save;
pub mod snapshot;
mod sprites;
pub mod stage;
pub mod stage1_enemies;
pub mod stage1_tables;
mod stage3_enemies;
mod stage6_beams;
mod stage6_wall;
mod stage7_door;
pub mod stage_bg;
pub mod stage_catalog;
mod stage_collision;
pub mod twoplayer;
pub mod video;

#[allow(dead_code)]
mod math;

pub use assets::Assets;
pub use game::{Game, State};
pub use input::Buttons;
pub use mods::Mods;
pub use powerup::{PowerMeter, PowerUp};
pub use video::{Framebuffer, HEIGHT, WIDTH};

/// The original runs at 60.0988 Hz (NTSC). Frontends use this to pace ticks.
pub const TICKS_PER_SECOND: f64 = 60.098_800_056_442_98;
