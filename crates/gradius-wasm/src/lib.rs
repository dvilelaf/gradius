//! Minimal `wasm32-unknown-unknown` backend for `gradius-core`.
//!
//! This crate is only an FFI shell: the rules, renderer and pack parser stay
//! in `gradius-core` and `gradius-assets`. JavaScript owns the canvas, the
//! clock and the keyboard; here we only own the [`Game`] and hand out a raw
//! RGBA framebuffer.
//!
//! # Exported C ABI
//!
//! | export | meaning |
//! | --- | --- |
//! | `alloc(len) -> *mut u8` | Reserve `len` bytes for JS to fill with the pack |
//! | `dealloc(ptr, len)` | Release a buffer from `alloc` |
//! | `init(pack_ptr, pack_len) -> u32` | Parse a pack in wasm memory. Returns 0 on success, 1 on a bad pack |
//! | `tick(p1, p2) -> *const u8` | Advance one frame with both pads, render its audio and return the 256x240 RGBA buffer; null before `init` |
//! | `frame_count() -> u32` | Frames simulated since the last `init` |
//! | `audio_ptr() -> *const f32` | Mono 48 kHz samples queued since the last consume; null before `init` |
//! | `audio_len() -> u32` | Samples available at `audio_ptr` |
//! | `audio_consumed(n)` | Drop the first `n` samples; call after copying them out |
//! | `music_cue() -> u32` | Current cue: 0 title, 1 stage 1, 2 stage 1 B, 3 boss, 4 clear, 5 game over |
//! | `state_save() -> *const u8` | Serialize the game into the state buffer; null before `init` |
//! | `state_len() -> u32` | Bytes written by the last `state_save` |
//! | `state_load(ptr, len) -> u32` | Restore an image: 0 loaded, 1 before `init`, 2 refused |
//! | `set_mods(hull_hits, simultaneous, start_stage, stage5_slowdown)` | Take the Options panel's values; they apply outside a run, see [`Backend::set_mods`] |
//!
//! The framebuffer is [`WIDTH`] * [`HEIGHT`] * 4 bytes and stays at the same
//! address until the next `init`, so JS may keep a view into wasm memory. The
//! audio pointer stays valid until the next `tick` or `audio_consumed` call.
//! The state pointer stays valid until the next `state_save` or `init`.
//!
//! A load restarts the current music cue from its beginning: the image carries
//! the simulation, not the mixer's position in the song. See
//! [`Backend::resync_audio`].
//!
//! # Input bits
//!
//! `buttons` is one byte in NES controller order, the same order as
//! [`Buttons::from_nes_bits`]: bit 0 fire, 1 power, 2 select, 3 start,
//! 4 up, 5 down, 6 left, 7 right. The SDL frontend maps the same buttons to
//! Z/J/Space, X/K, Backspace/Tab, Enter and the arrow keys.

use std::cell::RefCell;

use gradius_app::{MusicCue, MusicDirector};
use gradius_assets::Pack;
use gradius_audio::{AudioBank, Engine, Music, Sfx, Timeline, SAMPLE_RATE, SILENCE};
use gradius_core::audio::Sound;
use gradius_core::{Buttons, Game, Mods, State, HEIGHT, TICKS_PER_SECOND, WIDTH};

/// Bytes in one 256x240 RGBA8 frame.
pub const FRAME_BYTES: usize = WIDTH * HEIGHT * 4;

pub const FIRE: u8 = 0x01;
pub const POWER: u8 = 0x02;
pub const SELECT: u8 = 0x04;
pub const START: u8 = 0x08;
pub const UP: u8 = 0x10;
pub const DOWN: u8 = 0x20;
pub const LEFT: u8 = 0x40;
pub const RIGHT: u8 = 0x80;

/// Translate the input byte used by the web backend into core buttons.
pub fn buttons_from_bits(bits: u8) -> Buttons {
    Buttons::from_nes_bits(bits)
}

/// Reserve `len` bytes for JavaScript to write the pack into.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// Release a buffer returned by [`alloc`] with the same length.
///
/// # Safety
///
/// `ptr` must come from [`alloc`] with this exact `len` and must not be used
/// afterwards.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    // SAFETY: `alloc` built a `Box<[u8]>` of exactly `len` bytes.
    unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }
}

/// Drop consumed samples from the front of the queue once this many pile up,
/// so a page that plays for hours does not grow wasm memory without bound.
const COMPACT_AFTER: usize = 8_192;

struct Backend {
    game: Game,
    engine: Engine,
    bank: AudioBank,
    music: MusicDirector,
    /// Mono 48 kHz samples rendered for the frames ticked so far.
    queue: Vec<f32>,
    /// First sample in `queue` not yet handed to the page.
    start: usize,
    /// Samples rendered in total; keeps the audio clock on the frame clock.
    rendered: usize,
    scratch: Vec<f32>,
    /// Latest `GRSS` image, the buffer `state_save` hands to the page.
    image: Vec<u8>,
    /// The options the page chose. They reach `game` once no run is in
    /// progress, so a run keeps the options it started with.
    wanted: Mods,
}

impl Backend {
    fn from_pack(bytes: &[u8]) -> Option<Self> {
        let pack = Pack::parse(bytes).ok()?;
        // No ROM data: refuse to start (the page sees the non-zero status).
        pack.rom.as_ref()?;
        let bank = pack
            .audio
            .as_deref()
            .and_then(|chunk| AudioBank::from_chunk(chunk).ok())
            .unwrap_or_else(|| {
                warn_silent();
                AudioBank::default()
            });
        let mut engine = Engine::new(SAMPLE_RATE);
        engine.play_music(music_for(MusicCue::Title, &bank));
        Some(Self {
            game: Game::new(pack.into_assets()),
            engine,
            bank,
            music: MusicDirector::new(),
            queue: Vec::new(),
            start: 0,
            rendered: 0,
            scratch: Vec::new(),
            image: Vec::new(),
            wanted: Mods::default(),
        })
    }

    /// Take the page's options. A run in progress keeps its own, so they
    /// apply at once on the title or game-over screen and when a run ends.
    fn set_mods(&mut self, mods: Mods) {
        self.wanted = mods;
        self.apply_mods();
    }

    fn apply_mods(&mut self) {
        if self.game.state != State::Playing {
            self.game.set_mods(self.wanted);
        }
    }

    fn tick(&mut self, p1: u8, p2: u8) -> &[u8] {
        self.game
            .step_players(buttons_from_bits(p1), buttons_from_bits(p2));
        self.apply_mods();
        for sound in self.game.audio.drain() {
            // The game-over theme is music, not a one-shot effect.
            if sound != Sound::GameOver {
                self.engine.play_sfx(sfx_for(sound, &self.bank));
            }
            match sound {
                Sound::Pause => self.engine.pause_music(),
                Sound::Resume => self.engine.resume_music(),
                _ => {}
            }
        }
        if let Some(cue) = self.music.update(&self.game) {
            self.engine.play_music(music_for(cue, &self.bank));
        }
        self.render_audio();
        self.game.framebuffer().as_rgba()
    }

    /// Render the samples the mixer owes up to the current frame, so a slow
    /// or fast rAF never drifts away from the simulation clock.
    fn render_audio(&mut self) {
        let target = samples_for_frame(self.game.frame);
        if target > self.rendered {
            self.scratch.resize(target - self.rendered, 0.0);
            self.engine.fill(&mut self.scratch);
            self.queue.extend_from_slice(&self.scratch);
            self.rendered = target;
        }
    }

    /// Serialize the game into the buffer `state_save` exposes.
    fn save(&mut self) -> &[u8] {
        self.image = self.game.snapshot();
        &self.image
    }

    /// Restore a `GRSS` image. A refused image leaves the game, the audio
    /// queue and the mixer exactly as they were.
    fn load(&mut self, bytes: &[u8]) -> Result<(), gradius_core::save::SaveError> {
        let backup = self.game.snapshot();
        match self.game.restore(bytes) {
            Ok(()) => {
                self.resync_audio();
                Ok(())
            }
            Err(error) => {
                // The core validates before it touches fields, so this can
                // only fail if it broke its own format; keep the run alive.
                let _ = self.game.restore(&backup);
                Err(error)
            }
        }
    }

    /// The restored frame sits wherever the save was, so line the audio clock
    /// up with it and drop every sample queued for the old timeline. The
    /// mixer's driver state belongs to the old cue too, so it is cleared and
    /// the cue for the restored state restarts from its beginning.
    fn resync_audio(&mut self) {
        self.engine.reset();
        self.queue.clear();
        self.start = 0;
        self.rendered = samples_for_frame(self.game.frame);
        self.music = MusicDirector::new();
        self.music.update(&self.game);
        self.engine
            .play_music(music_for(self.music.cue(), &self.bank));
    }

    fn audio(&self) -> &[f32] {
        &self.queue[self.start..]
    }

    fn consume(&mut self, samples: usize) {
        self.start = self.start.saturating_add(samples).min(self.queue.len());
        if self.start >= COMPACT_AFTER {
            self.queue.drain(..self.start);
            self.start = 0;
        }
    }
}

fn samples_for_frame(frame: u64) -> usize {
    ((frame as f64 * f64::from(SAMPLE_RATE)) / TICKS_PER_SECOND).round() as usize
}

/// Say once on stderr (the console, in a browser build) that the pack has no
/// usable audio chunk.
fn warn_silent() {
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| eprintln!("warning: pack has no usable audio chunk; playing in silence"));
}

fn music_for(cue: MusicCue, bank: &AudioBank) -> &'static Timeline {
    let music = match cue {
        MusicCue::Title => Music::Title,
        MusicCue::Stage1 => Music::Stage1,
        MusicCue::Stage1B => Music::Stage1B,
        MusicCue::Stage2 => Music::Stage2,
        MusicCue::Stage3 => Music::Stage3,
        MusicCue::Stage4 => Music::Stage4,
        MusicCue::Stage5 => Music::Stage5,
        MusicCue::Stage6 => Music::Stage6,
        MusicCue::Stage7 => Music::Stage7,
        MusicCue::Boss => Music::Boss,
        MusicCue::StageClear => Music::StageClear,
        MusicCue::GameOver => Music::GameOver,
    };
    bank.music(music).unwrap_or(&SILENCE)
}

fn sfx_for(sound: Sound, bank: &AudioBank) -> &'static Timeline {
    let sfx = match sound {
        Sound::Shot => Sfx::Shot,
        Sound::EnemyHit => Sfx::EnemyHit,
        Sound::Explosion => Sfx::Explosion,
        Sound::PowerUp => Sfx::PowerUp,
        Sound::PlayerDeath => Sfx::PlayerDeath,
        Sound::Capsule => Sfx::Capsule,
        Sound::EnemyShot => Sfx::EnemyShot,
        Sound::GameOver => Sfx::GameOver,
        Sound::BlockBreak => Sfx::BlockBreak,
        Sound::BlockBreakStage6 => Sfx::BlockBreakStage6,
        Sound::Pause => Sfx::Pause,
        Sound::ScreenClear => Sfx::ScreenClear,
        Sound::Resume => return &SILENCE,
    };
    bank.sfx(sfx).unwrap_or(&SILENCE)
}

thread_local! {
    static BACKEND: RefCell<Option<Backend>> = const { RefCell::new(None) };
}

/// Parse a pack from JS linear memory. The bytes are copied, so the caller may
/// free its buffer immediately after the call.
///
/// # Safety
///
/// `pack_ptr` must point to `pack_len` readable bytes in this instance's
/// linear memory for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn init(pack_ptr: *const u8, pack_len: usize) -> u32 {
    if pack_ptr.is_null() {
        return 1;
    }
    // SAFETY: the caller promises a readable region of `pack_len` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(pack_ptr, pack_len) };
    match Backend::from_pack(bytes) {
        Some(backend) => {
            BACKEND.with(|slot| *slot.borrow_mut() = Some(backend));
            0
        }
        None => 1,
    }
}

/// Advance exactly one frame with both pads and return the RGBA framebuffer.
///
/// `p1`/`p2` are one byte each in NES controller order, the same order as
/// [`buttons_from_bits`]. A 1P game merges the two (`$8203`); a 2P game uses
/// the pad of the player in `$18`.
///
/// The pointer is null before [`init`]; the buffer is `FRAME_BYTES` long and
/// remains valid until the next `init`.
#[no_mangle]
pub extern "C" fn tick(p1: u32, p2: u32) -> *const u8 {
    BACKEND.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(backend) = slot.as_mut() else {
            return std::ptr::null();
        };
        backend.tick(p1 as u8, p2 as u8).as_ptr()
    })
}

/// Frames simulated since the last successful [`init`].
#[no_mangle]
pub extern "C" fn frame_count() -> u32 {
    BACKEND.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(0, |backend| backend.game.frame as u32)
    })
}

/// Mono 48 kHz samples queued by the ticks since the last consume.
///
/// Null before [`init`]; otherwise the pointer stays valid until the next
/// [`tick`] or [`audio_consumed`]. Copy the samples out before either call.
#[no_mangle]
pub extern "C" fn audio_ptr() -> *const f32 {
    BACKEND.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |backend| backend.audio().as_ptr())
    })
}

/// Number of samples readable at [`audio_ptr`].
#[no_mangle]
pub extern "C" fn audio_len() -> u32 {
    BACKEND.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(0, |backend| backend.audio().len() as u32)
    })
}

/// Drop the first `samples` samples. Out-of-range counts are clamped.
#[no_mangle]
pub extern "C" fn audio_consumed(samples: u32) {
    BACKEND.with(|slot| {
        if let Some(backend) = slot.borrow_mut().as_mut() {
            backend.consume(samples as usize);
        }
    });
}

/// Current music cue, the same one the native frontends select.
#[no_mangle]
pub extern "C" fn music_cue() -> u32 {
    BACKEND.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(0, |backend| cue_index(backend.music.cue()))
    })
}

/// Serialize the game into a `GRSS` image and return it as a wasm buffer.
///
/// Null before [`init`]. The pointer is valid until the next `state_save` or
/// `init`; read [`state_len`] bytes and copy them out before either call.
#[no_mangle]
pub extern "C" fn state_save() -> *const u8 {
    BACKEND.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(backend) = slot.as_mut() else {
            return std::ptr::null();
        };
        backend.save().as_ptr()
    })
}

/// Bytes written by the last [`state_save`]. 0 before `init` or any save.
#[no_mangle]
pub extern "C" fn state_len() -> u32 {
    BACKEND.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(0, |backend| backend.image.len() as u32)
    })
}

/// Restore a `GRSS` image from wasm memory.
///
/// Returns 0 on success, 1 before [`init`] and 2 when the image is refused. A
/// refused image leaves the game, the framebuffer and the audio queue exactly
/// as they were, so the page keeps running.
///
/// # Safety
///
/// `ptr` must point to `len` readable bytes in this instance's linear memory
/// for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn state_load(ptr: *const u8, len: usize) -> u32 {
    if ptr.is_null() {
        return 1;
    }
    // SAFETY: the caller promises a readable region of `len` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    BACKEND.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(backend) = slot.as_mut() else {
            return 1;
        };
        match backend.load(bytes) {
            Ok(()) => 0,
            Err(_) => 2,
        }
    })
}

/// Take the page's options: `hull_hits` (0 is invincible, otherwise clamped
/// to 99), `simultaneous`, `start_stage` (1..=7) and `stage5_slowdown`
/// (non-zero restores the cartridge's halving; zero is the port's default).
///
/// Before `init` this does nothing; the page calls it again after `init`.
#[no_mangle]
pub extern "C" fn set_mods(hull_hits: u32, simultaneous: u32, start_stage: u32, slowdown: u32) {
    BACKEND.with(|slot| {
        if let Some(backend) = slot.borrow_mut().as_mut() {
            backend.set_mods(Mods {
                hull_hits: hull_hits.min(99) as u8,
                start_stage: start_stage.clamp(1, 7) as u8,
                simultaneous_2p: simultaneous != 0,
                stage5_slowdown: slowdown != 0,
                ..Mods::default()
            });
        }
    });
}

fn cue_index(cue: MusicCue) -> u32 {
    match cue {
        MusicCue::Title => 0,
        MusicCue::Stage1 => 1,
        MusicCue::Stage1B => 2,
        MusicCue::Stage2 => 6,
        MusicCue::Stage3 => 7,
        MusicCue::Stage4 => 8,
        MusicCue::Stage5 => 9,
        MusicCue::Stage6 => 10,
        MusicCue::Stage7 => 11,
        MusicCue::Boss => 3,
        MusicCue::StageClear => 4,
        MusicCue::GameOver => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_core::State;

    fn pack_with_audio(audio: Option<Vec<u8>>) -> Vec<u8> {
        use gradius_pack::{PackBuilder, TILES_PER_BANK, TILE_BANKS, TILE_BYTES};
        let bank = vec![[0u8; TILE_BYTES]; TILES_PER_BANK];
        let mut pack = PackBuilder::new()
            .tiles(&vec![bank; TILE_BANKS])
            .palettes(&[[0, 0, 0]])
            .chunk(gradius_pack::CHUNK_ROM, real_rom_chunk());
        if let Some(audio) = audio {
            pack = pack.chunk(gradius_pack::CHUNK_AUDIO, audio);
        }
        pack.finish()
    }

    /// The `CHUNK_ROM` of the extracted pack: the core reads its tables from it.
    fn real_rom_chunk() -> Vec<u8> {
        let path = std::env::var_os("GRADIUS_PACK")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/gradius.pak")
            });
        gradius_pack::read_chunk_from_file(&path, gradius_pack::CHUNK_ROM).unwrap_or_else(|_| {
            panic!("no extracted pack: run GRADIUS_ROM=<rom> scripts/extract-data")
        })
    }

    /// A held pulse tone: loud enough for the audibility checks, small enough
    /// to write by hand.
    static TONE_EVENTS: [gradius_audio::Event; 4] = [
        gradius_audio::Event::new(0, 0x4015, 0x0F),
        gradius_audio::Event::new(0, 0x4000, 0xBF),
        gradius_audio::Event::new(0, 0x4002, 0x70),
        gradius_audio::Event::new(0, 0x4003, 0x08),
    ];

    fn tone(looping: bool) -> Timeline {
        Timeline {
            name: "tone",
            events: &TONE_EVENTS,
            total_cycles: 60 * 29_780,
            loop_point: looping.then_some(0),
            loop_index: 0,
        }
    }

    fn synthetic_audio(driver: &[u8]) -> Vec<u8> {
        gradius_audio::chunk::encode_chunk(
            &vec![tone(true); Music::ALL.len()],
            &vec![tone(false); Sfx::ALL.len()],
            driver,
        )
    }

    fn pack_bytes() -> Vec<u8> {
        pack_with_audio(Some(synthetic_audio(&[])))
    }

    fn assert_audible(samples: &[f32], label: &str) {
        assert!(
            samples.len() >= 48_000,
            "{label}: only {} samples after a second",
            samples.len()
        );
        let sum: f64 = samples
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum();
        let rms = (sum / samples.len() as f64).sqrt();
        assert!(
            rms > 0.01 && rms <= 1.0,
            "{label}: RMS {rms:.3} is outside the audible, unclipped range"
        );
        assert!(
            samples
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0),
            "{label}: a sample is not finite or exceeds full scale"
        );
    }

    #[test]
    fn input_bits_follow_nes_controller_order() {
        assert_eq!(
            [FIRE, POWER, SELECT, START, UP, DOWN, LEFT, RIGHT],
            [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80]
        );
        assert_eq!(buttons_from_bits(0), Buttons::NONE);
        assert!(buttons_from_bits(FIRE).fire);
        assert!(buttons_from_bits(POWER).power);
        assert!(buttons_from_bits(SELECT).select);
        assert!(buttons_from_bits(START).start);
        assert!(buttons_from_bits(UP).up);
        assert!(buttons_from_bits(DOWN).down);
        assert!(buttons_from_bits(LEFT).left);
        assert!(buttons_from_bits(RIGHT).right);
        assert_eq!(
            buttons_from_bits(0xFF),
            Buttons {
                fire: true,
                power: true,
                select: true,
                start: true,
                up: true,
                down: true,
                left: true,
                right: true,
            }
        );
    }

    #[test]
    fn one_tick_returns_a_full_rgba_frame() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        let frame = backend.tick(0, 0);
        assert_eq!(frame.len(), FRAME_BYTES);
        assert_eq!(FRAME_BYTES, 256 * 240 * 4);
        assert_eq!(backend.game.frame, 1);
    }

    #[test]
    fn the_second_pad_reaches_the_core() {
        // `$8203`: a 1P game merges both pads, so p2 bits must reach
        // `step_players` instead of being dropped by the frontend.
        let mut with_p2 = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        let mut p1_only = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        for _ in 0..120 {
            with_p2.tick(0, START);
            p1_only.tick(0, 0);
        }
        assert_ne!(with_p2.game.state_hash(), p1_only.game.state_hash());
    }

    #[test]
    fn a_pack_without_rom_data_is_refused() {
        use gradius_pack::{PackBuilder, TILES_PER_BANK, TILE_BANKS, TILE_BYTES};
        let bank = vec![[0u8; TILE_BYTES]; TILES_PER_BANK];
        let bytes = PackBuilder::new()
            .tiles(&vec![bank; TILE_BANKS])
            .palettes(&[[0, 0, 0]])
            .finish();
        assert!(Backend::from_pack(&bytes).is_none());
    }

    #[test]
    fn a_foreign_pack_is_rejected() {
        assert!(Backend::from_pack(b"not a pack").is_none());
    }

    #[test]
    fn a_tick_queues_a_frame_of_audio_and_consuming_accounts_for_it() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        assert!(backend.audio().is_empty());

        backend.tick(0, 0);
        let first = backend.audio().len();
        let frame_samples = f64::from(SAMPLE_RATE) / TICKS_PER_SECOND;
        assert!(
            (first as f64 - frame_samples).abs() < 1.0,
            "one frame is about {frame_samples} samples, got {first}"
        );

        let remaining = backend.audio().len();
        backend.consume(100);
        assert_eq!(backend.audio().len(), remaining - 100);

        backend.tick(0, 0);
        assert!(backend.audio().len() > remaining - 100);

        // Over-range consumes are clamped and the consumed prefix is dropped.
        backend.consume(usize::MAX);
        assert!(backend.audio().is_empty());
        backend.tick(0, 0);
        assert!(!backend.audio().is_empty());
    }

    #[test]
    fn a_version_two_pack_decodes_timelines_and_the_driver_bank() {
        use gradius_audio::chunk::encode_driver;
        use gradius_audio::driver::{
            DriverSource, CHANNEL_BLOCK_LEN, DRIVER_BASE, PERIOD_TABLE, PERIOD_TABLE_LEN,
            REQUEST_TABLE, REQUEST_TABLE_LEN,
        };
        let source = DriverSource {
            base: DRIVER_BASE,
            request_addr: REQUEST_TABLE,
            request: vec![0xAB; REQUEST_TABLE_LEN],
            period_addr: PERIOD_TABLE,
            period: (0..PERIOD_TABLE_LEN as u8).collect(),
            channels: [
                (0x00B0, 0x00),
                (0x00C1, 0x04),
                (0x00D2, 0x08),
                (0x00E3, 0x0C),
            ],
            segments: vec![(0xC100, vec![0xD1, 0x7F, 0x00, 0x01, 0xFF])],
        };
        let audio = synthetic_audio(&encode_driver(&source));
        let mut backend =
            Backend::from_pack(&pack_with_audio(Some(audio))).expect("version 2 pack parses");

        assert!(backend.bank.music(Music::Title).is_some());
        assert!(backend.bank.sfx(Sfx::Explosion).is_some());
        let driver = backend
            .bank
            .driver_bank()
            .expect("the KDRV driver section was decoded");
        assert_eq!(driver.channel_block_len(), CHANNEL_BLOCK_LEN);

        // The decoded timelines are what `tick` plays: 61 frames queue a
        // second of audible title music.
        for _ in 0..61 {
            backend.tick(0, 0);
        }
        assert_eq!(backend.music.cue(), MusicCue::Title);
        assert_audible(backend.audio(), "version 2 title");
    }

    #[test]
    fn title_and_stage_ticks_queue_bounded_audible_samples() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        for _ in 0..61 {
            backend.tick(0, 0);
        }
        assert_eq!(backend.music.cue(), MusicCue::Title);
        assert_audible(backend.audio(), "title");

        backend.consume(usize::MAX);
        backend.game.state = State::Playing;
        for _ in 0..61 {
            backend.tick(0, 0);
        }
        assert_eq!(backend.music.cue(), MusicCue::Stage1);
        assert_audible(backend.audio(), "stage 1");
    }

    #[test]
    fn a_second_of_ticks_produces_audible_samples() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        for _ in 0..61 {
            backend.tick(0, 0);
        }
        let samples = backend.audio();
        assert!(samples.len() >= 48_000);
        assert!(
            samples.iter().any(|sample| sample.abs() > 0.0),
            "the title music was silent"
        );
        assert!(samples
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
    }

    #[test]
    fn music_cues_follow_the_same_public_state_as_the_native_frontend() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        assert_eq!(backend.music.cue(), MusicCue::Title);
        backend.tick(0, 0);
        assert_eq!(backend.music.cue(), MusicCue::Title);

        backend.game.state = State::Playing;
        backend.tick(0, 0);
        assert_eq!(backend.music.cue(), MusicCue::Stage1);

        backend.game.state = State::GameOver;
        backend.tick(0, 0);
        assert_eq!(backend.music.cue(), MusicCue::GameOver);
    }

    #[test]
    fn a_pack_without_audio_plays_in_silence() {
        let mut backend =
            Backend::from_pack(&pack_with_audio(None)).expect("a pack without audio parses");
        for _ in 0..61 {
            backend.tick(0, 0);
        }
        assert!(backend.audio().len() >= 48_000);
        assert!(backend.audio().iter().all(|sample| *sample == 0.0));
        let bank = AudioBank::default();
        assert!(std::ptr::eq(music_for(MusicCue::Boss, &bank), &SILENCE));
        assert!(std::ptr::eq(sfx_for(Sound::Shot, &bank), &SILENCE));
    }

    #[test]
    fn every_cue_selects_its_own_pack_track() {
        let bank = AudioBank::from_chunk(&synthetic_audio(&[])).expect("synthetic chunk");
        let cues = [
            (MusicCue::Title, Music::Title),
            (MusicCue::Stage1, Music::Stage1),
            (MusicCue::Stage1B, Music::Stage1B),
            (MusicCue::Stage2, Music::Stage2),
            (MusicCue::Stage3, Music::Stage3),
            (MusicCue::Stage4, Music::Stage4),
            (MusicCue::Stage5, Music::Stage5),
            (MusicCue::Stage6, Music::Stage6),
            (MusicCue::Stage7, Music::Stage7),
            (MusicCue::Boss, Music::Boss),
            (MusicCue::StageClear, Music::StageClear),
            (MusicCue::GameOver, Music::GameOver),
        ];
        for (cue, track) in cues {
            assert!(
                std::ptr::eq(music_for(cue, &bank), bank.music(track).unwrap()),
                "{cue:?}"
            );
        }
        assert!(std::ptr::eq(
            sfx_for(Sound::Explosion, &bank),
            bank.sfx(Sfx::Explosion).unwrap()
        ));
    }

    #[test]
    fn state_round_trips_and_the_audio_clock_resyncs() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        for _ in 0..30 {
            backend.tick(0, 0);
        }
        let saved = backend.save().to_vec();
        assert_eq!(saved, backend.game.snapshot());
        assert!(saved.len() <= gradius_core::save::SLOT_BYTES);

        backend.consume(usize::MAX);
        for _ in 0..5 {
            backend.tick(0, 0);
        }
        assert_ne!(backend.game.snapshot(), saved, "the game moved on");

        backend.load(&saved).expect("restore the image");
        assert_eq!(backend.game.snapshot(), saved);
        assert!(
            backend.audio().is_empty(),
            "samples queued for the old timeline survived the load"
        );
        assert_eq!(backend.rendered, samples_for_frame(backend.game.frame));

        // The next tick queues one frame, not a backlog from the saved frame.
        backend.tick(0, 0);
        let frame_samples = f64::from(SAMPLE_RATE) / TICKS_PER_SECOND;
        let queued = backend.audio().len() as f64;
        assert!(
            (queued - frame_samples).abs() <= 1.0,
            "expected one frame of {frame_samples} samples, got {queued}"
        );
    }

    #[test]
    fn loading_restarts_the_cue_of_the_restored_state() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        let title = backend.save().to_vec();
        backend.game.state = State::Playing;
        for _ in 0..5 {
            backend.tick(0, 0);
        }
        assert_eq!(backend.music.cue(), MusicCue::Stage1);

        backend.load(&title).expect("restore the title screen");
        assert_eq!(backend.music.cue(), MusicCue::Title);
        assert_eq!(
            backend.engine.active_voices(),
            1,
            "the title music did not restart"
        );
        backend.tick(0, 0);
        assert!(
            !backend.audio().is_empty(),
            "no audio after the cue was restarted"
        );
    }

    #[test]
    fn a_corrupt_image_is_refused_and_leaves_the_run_untouched() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        for _ in 0..10 {
            backend.tick(0, 0);
        }
        let before = backend.save().to_vec();
        let frame = backend.game.frame;
        backend.consume(usize::MAX);

        for junk in [&b"not a GRSS image"[..], &before[..before.len() / 2]] {
            assert!(backend.load(junk).is_err());
            assert_eq!(
                backend.game.snapshot(),
                before,
                "a refused load changed state"
            );
            assert_eq!(backend.game.frame, frame);
            assert!(
                backend.audio().is_empty(),
                "a refused load touched the queue"
            );
        }
        assert_eq!(backend.save(), before.as_slice());

        backend.tick(0, 0);
        assert!(
            !backend.audio().is_empty(),
            "the game stopped after a refusal"
        );
    }

    #[test]
    fn the_exported_state_calls_round_trip_through_linear_memory() {
        let pack = pack_bytes();
        // SAFETY: `pack` outlives the call and the backend copies it.
        assert_eq!(unsafe { init(pack.as_ptr(), pack.len()) }, 0);
        for _ in 0..4 {
            tick(0, 0);
        }
        let ptr = state_save();
        assert!(!ptr.is_null());
        let len = state_len() as usize;
        assert!(len > 0 && len <= gradius_core::save::SLOT_BYTES);
        // SAFETY: `state_save` exposes `state_len` readable bytes.
        let image = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
        let saved_frame = frame_count();

        for _ in 0..3 {
            tick(0, 0);
        }
        assert_ne!(frame_count(), saved_frame);

        let copy = image.clone();
        // SAFETY: `copy` is readable for its whole length.
        assert_eq!(unsafe { state_load(copy.as_ptr(), copy.len()) }, 0);
        assert_eq!(
            frame_count(),
            saved_frame,
            "the load did not restore the frame count"
        );

        let junk = b"not a GRSS image";
        // SAFETY: `junk` is a readable array.
        assert_eq!(unsafe { state_load(junk.as_ptr(), junk.len()) }, 2);
        assert_eq!(
            frame_count(),
            saved_frame,
            "a refused image moved the game on"
        );

        // SAFETY: a null pointer is rejected before it is dereferenced.
        assert_eq!(unsafe { state_load(std::ptr::null(), 0) }, 1);
    }

    fn game_mods() -> Mods {
        BACKEND.with(|slot| slot.borrow().as_ref().expect("initialised").game.mods())
    }

    #[test]
    fn default_options_do_not_change_the_game() {
        let mut plain = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        let mut chosen = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        chosen.set_mods(Mods::default());
        for _ in 0..120 {
            plain.tick(0, START);
            chosen.tick(0, START);
        }
        assert_eq!(chosen.game.state_hash(), plain.game.state_hash());
    }

    #[test]
    fn options_chosen_on_the_title_apply_at_once() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        backend.set_mods(Mods {
            hull_hits: 3,
            simultaneous_2p: true,
            ..Mods::default()
        });
        assert_eq!(backend.game.mods().hull_hits, 3);
        assert!(backend.game.mods().simultaneous_2p);
    }

    #[test]
    fn options_wait_for_the_end_of_a_run() {
        let mut backend = Backend::from_pack(&pack_bytes()).expect("synthetic pack parses");
        backend.game.state = State::Playing;
        backend.set_mods(Mods {
            hull_hits: 3,
            simultaneous_2p: true,
            ..Mods::default()
        });
        assert_eq!(
            backend.game.mods(),
            Mods::default(),
            "a run changed its options"
        );

        backend.game.state = State::GameOver;
        backend.tick(0, 0);
        assert_eq!(backend.game.mods().hull_hits, 3);
        assert!(backend.game.mods().simultaneous_2p);
    }

    #[test]
    fn set_mods_export_clamps_every_value() {
        // Before `init` the call is a no-op rather than a panic.
        set_mods(3, 1, 2, 1);
        let pack = pack_bytes();
        // SAFETY: `pack` outlives the call and the backend copies it.
        assert_eq!(unsafe { init(pack.as_ptr(), pack.len()) }, 0);

        set_mods(420, 1, 2, 1);
        let mods = game_mods();
        assert_eq!((mods.hull_hits, mods.start_stage), (99, 2));
        assert!(mods.simultaneous_2p && mods.stage5_slowdown);

        set_mods(9, 0, 0, 0);
        let mods = game_mods();
        assert_eq!((mods.hull_hits, mods.start_stage), (9, 1), "0 snaps up");
        assert!(!mods.stage5_slowdown);

        set_mods(0, 0, 99, 0);
        let mods = game_mods();
        assert_eq!((mods.hull_hits, mods.start_stage), (0, 7));
        assert!(!mods.simultaneous_2p);
    }
}
