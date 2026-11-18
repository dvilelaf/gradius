//! Platform-agnostic application runner.
//!
//! A backend supplies three things: an [`InputFrame`] source, an RGBA
//! [`FrameSink`] and a [`SoundSink`]. The runner owns the fixed-step clock,
//! pause handling, the catch-up budget and music-cue selection, so a new
//! platform only implements the traits around its own I/O.

#![forbid(unsafe_code)]

pub mod music;
mod runner;

pub use gradius_core::audio::Sound;
pub use gradius_core::{Buttons, Framebuffer, Game, Mods, HEIGHT, WIDTH};
pub use music::{MusicCue, MusicDirector, BOSS_SCORE, BOSS_SCROLL, STAGE1_SECOND_HALF_SCROLL};
pub use runner::{
    FrameSink, InputFrame, InputProvider, Runner, SoundSink, Summary, STEP, TICKS_PER_SECOND,
};
