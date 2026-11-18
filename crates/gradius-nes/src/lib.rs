//! Analysis oracle for the original cartridge.
//!
//! This crate is a development tool. Nothing here is linked into the game: the
//! shipped runtime consumes the self-contained asset pack produced by
//! `xtask extract`.

pub mod apu;
pub mod bus;
pub mod cpu;
pub mod emulator;
pub mod ppu;

pub use apu::{Apu, ApuWrite, MAX_AUDIO_SAMPLES, SAMPLE_RATE};
pub use bus::{
    Trace, BUTTON_A, BUTTON_B, BUTTON_DOWN, BUTTON_LEFT, BUTTON_RIGHT, BUTTON_SELECT, BUTTON_START,
    BUTTON_UP,
};
pub use cpu::Cpu;
pub use emulator::{Emulator, EmulatorError, ObjectState, PlayerState};
pub use ppu::{
    PpuSnapshot, SpriteZeroHit, VramWrite, FRAMEBUFFER_LEN, HEIGHT as SCREEN_HEIGHT,
    WIDTH as SCREEN_WIDTH,
};
