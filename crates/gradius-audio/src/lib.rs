//! Original-sounding audio for the native Gradius port.
//!
//! The library has no SDL and no cartridge dependency. It contains:
//!
//! * [`apu`] — a runtime NES APU model (two pulses, triangle, noise),
//! * [`timeline`] — captured APU register streams and their player,
//! * [`data`] — the music and sound-effect identities,
//! * [`chunk`] — the versioned audio chunk carried by the asset pack,
//! * [`engine`] — music plus effect mixing at 48 kHz mono,
//! * [`wav`] — a small PCM WAV writer for headless verification.
//!
//! No captured audio lives in the repository. `xtask extract` captures the
//! timelines from the cartridge through the development oracle (`gradius-nes`,
//! module `capture`, feature `capture`) and writes them into the pack's audio
//! chunk, which [`chunk::AudioBank`] loads at runtime. A pack without the chunk
//! plays in silence, and the shipped game never reads the cartridge.

pub mod apu;
#[cfg(feature = "capture")]
pub mod capture;
pub mod chunk;
pub mod data;
pub mod driver;
pub mod engine;
pub mod timeline;
pub mod wav;

pub use apu::{Apu, SAMPLE_RATE};
pub use chunk::{AudioBank, AudioChunkError, AUDIO_CHUNK_MAGIC, AUDIO_CHUNK_VERSION};
pub use data::{Music, Sfx};
pub use driver::{Driver, DriverBank, DriverSource, Part, RegWrite, FRAME_RATE_HZ};
pub use engine::{Engine, TICKS_PER_SECOND};
pub use timeline::{Event, Player, Timeline, SILENCE};
