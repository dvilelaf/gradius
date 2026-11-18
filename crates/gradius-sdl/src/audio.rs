//! SDL2 audio backend.
//!
//! The mixer itself lives in `gradius-audio`: a runtime NES APU model playing
//! captured Konami-driver register timelines. This module only owns the SDL
//! device, converts the engine's `f32` output to the callback's `i16`, and
//! maps the core's semantic [`Sound`] events to captured sound effects.
//!
//! A missing device, `--no-audio` or `--mute` leave the game fully playable:
//! the backend becomes a no-op and never fails a frame.

use gradius_audio::{AudioBank, Engine, Sfx, Timeline, SAMPLE_RATE, SILENCE};
use gradius_core::audio::Sound;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::time::{Duration, Instant};

/// Decode the pack's audio chunk. A pack without one (or with one this build
/// cannot read) gives an empty bank: the game runs in silence, and this says so
/// once.
pub fn load_audio_bank(chunk: Option<&[u8]>) -> AudioBank {
    match chunk {
        Some(chunk) => AudioBank::from_chunk(chunk).unwrap_or_else(|error| {
            eprintln!("warning: unusable audio chunk: {error}; playing in silence");
            AudioBank::default()
        }),
        None => {
            eprintln!(
                "warning: pack has no audio chunk; playing in silence (run scripts/extract-data)"
            );
            AudioBank::default()
        }
    }
}

/// Map a core event to the pack's original sound effect; silence when the pack
/// does not carry it.
pub fn sfx_for(sound: Sound, bank: &AudioBank) -> &'static Timeline {
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

/// The SDL callback: renders the engine into the requested `i16` buffer.
pub struct SoundMixer {
    engine: Engine,
    scratch: Vec<f32>,
    music: &'static Timeline,
    gain: f32,
}

impl SoundMixer {
    pub fn new(sample_rate: i32, music: &'static Timeline) -> Self {
        let rate = if sample_rate > 0 {
            sample_rate as u32
        } else {
            SAMPLE_RATE
        };
        let mut engine = Engine::new(rate);
        engine.play_music(music);
        Self {
            engine,
            // The SDL callback asks for a few hundred samples at a time; a
            // fixed buffer keeps allocation off the audio thread.
            scratch: vec![0.0; 4096],
            music,
            gain: 1.0,
        }
    }

    /// Master gain applied to the finished mix, so it covers music and effects.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 1.0);
    }

    /// Switch the looping/one-shot music track. Repeating the current track is
    /// a no-op, so a frame-by-frame director can call this every tick.
    pub fn set_music(&mut self, music: &'static Timeline) {
        if !std::ptr::eq(music, self.music) {
            self.music = music;
            self.engine.play_music(music);
        }
    }

    pub fn play(&mut self, effect: &'static Timeline) {
        self.engine.play_sfx(effect);
    }

    /// Apply a core pause event: `Pause` freezes the music and `Resume`
    /// continues it. Other events are ignored.
    pub fn music_event(&mut self, sound: Sound) {
        match sound {
            Sound::Pause => self.engine.pause_music(),
            Sound::Resume => self.engine.resume_music(),
            _ => {}
        }
    }

    fn render(&mut self, out: &mut [i16]) {
        if self.scratch.len() < out.len() {
            self.scratch.resize(out.len(), 0.0);
        }
        let buffer = &mut self.scratch[..out.len()];
        buffer.fill(0.0);
        self.engine.fill(buffer);
        for (target, sample) in out.iter_mut().zip(buffer.iter()) {
            *target = (sample.clamp(-1.0, 1.0) * self.gain * f32::from(i16::MAX)) as i16;
        }
    }
}

impl AudioCallback for SoundMixer {
    type Channel = i16;

    fn callback(&mut self, out: &mut [i16]) {
        self.render(out);
    }
}

/// An optional SDL audio device. When it is missing every call is a no-op.
pub struct AudioBackend {
    device: Option<AudioDevice<SoundMixer>>,
    /// What a reopened device must resume with.
    music: &'static Timeline,
    gain: f32,
    /// Set while a device was lost and has not come back yet.
    retry_at: Option<Instant>,
}

/// How long to wait before trying to reopen a device that failed to reopen.
const REOPEN_RETRY: Duration = Duration::from_secs(2);

impl AudioBackend {
    /// Open 48 kHz mono. Failure is reported once and playback continues
    /// silently, as the task requires.
    pub fn open(sdl: &sdl2::Sdl, music: &'static Timeline, gain: f32) -> Self {
        match Self::try_open(sdl, music, gain) {
            Ok(device) => Self {
                device: Some(device),
                music,
                gain,
                retry_at: None,
            },
            Err(error) => {
                eprintln!("warning: audio disabled: {error}");
                Self::disabled()
            }
        }
    }

    /// Used by `--no-audio`, `--mute` and headless runs.
    pub fn disabled() -> Self {
        Self {
            device: None,
            music: &SILENCE,
            gain: 1.0,
            retry_at: None,
        }
    }

    /// The device went away (SDL_AUDIODEVICEREMOVED: the sound server dropped
    /// the stream, a sink was unplugged or restarted). SDL never revives the
    /// stream, so without this the game stays silent for good. Opens a fresh
    /// device that resumes the current music and volume; on failure it is
    /// retried from [`AudioBackend::retry_lost`].
    pub fn reopen(&mut self, sdl: &sdl2::Sdl) {
        self.device = None;
        match Self::try_open(sdl, self.music, self.gain) {
            Ok(device) => {
                eprintln!("audio: device reopened");
                self.device = Some(device);
                self.retry_at = None;
            }
            Err(error) => {
                eprintln!("warning: audio lost: {error}; retrying");
                self.retry_at = Some(Instant::now() + REOPEN_RETRY);
            }
        }
    }

    /// Called every frame: retries a failed [`AudioBackend::reopen`] when due.
    pub fn retry_lost(&mut self, sdl: &sdl2::Sdl) {
        if self.retry_at.is_some_and(|at| Instant::now() >= at) {
            self.reopen(sdl);
        }
    }

    fn try_open(
        sdl: &sdl2::Sdl,
        music: &'static Timeline,
        gain: f32,
    ) -> Result<AudioDevice<SoundMixer>, String> {
        let audio = sdl.audio()?;
        let desired = AudioSpecDesired {
            freq: Some(SAMPLE_RATE as i32),
            channels: Some(1),
            // 512 (10 ms) left half a callback of slack against PipeWire's
            // 256-sample quantum; 1024 costs 10 ms more latency and survives
            // scheduling hiccups that made the music stutter.
            samples: Some(1024),
        };
        let device = audio
            .open_playback(None, &desired, |spec| {
                let mut mixer = SoundMixer::new(spec.freq, music);
                mixer.set_gain(gain);
                mixer
            })
            .map_err(|error| format!("cannot open audio device: {error}"))?;
        // SDL opens playback devices paused; without this the mixer runs but
        // nothing reaches the speakers.
        device.resume();
        Ok(device)
    }

    pub fn play(&mut self, effect: &'static Timeline) {
        if let Some(device) = &mut self.device {
            device.lock().play(effect);
        }
    }

    /// Forward `Sound::Pause`/`Sound::Resume` to the music voice.
    pub fn music_event(&mut self, sound: Sound) {
        if let Some(device) = &mut self.device {
            device.lock().music_event(sound);
        }
    }

    pub fn set_music(&mut self, music: &'static Timeline) {
        self.music = music;
        if let Some(device) = &mut self.device {
            device.lock().set_music(music);
        }
    }

    /// Apply the master volume live. Without a device this is a no-op.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 1.0);
        if let Some(device) = &mut self.device {
            device.lock().set_gain(gain);
        }
    }
}

/// A bank of small synthetic tracks, each named after the music or effect it
/// stands for, so tests do not need the extracted pack.
#[cfg(test)]
pub(crate) fn test_bank() -> AudioBank {
    use gradius_audio::{Event, Music};
    static TONE: [Event; 4] = [
        Event::new(0, 0x4015, 0x0F),
        Event::new(0, 0x4000, 0xBF),
        Event::new(0, 0x4002, 0x70),
        Event::new(0, 0x4003, 0x08),
    ];
    let track = |name: &'static str, looping: bool| Timeline {
        name,
        events: &TONE,
        total_cycles: 60 * 29_780,
        loop_point: looping.then_some(0),
        loop_index: 0,
    };
    let music: Vec<Timeline> = Music::ALL
        .iter()
        .map(|music| track(music.name(), music.loops()))
        .collect();
    let sfx: Vec<Timeline> = Sfx::ALL
        .iter()
        .map(|sfx| track(sfx.name(), false))
        .collect();
    AudioBank::from_chunk(&gradius_audio::chunk::encode_chunk(&music, &sfx, &[]))
        .expect("synthetic audio chunk")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_audio::Music;

    fn title() -> &'static Timeline {
        test_bank().music(Music::Title).expect("title")
    }

    const SOUNDS: [Sound; 8] = [
        Sound::Shot,
        Sound::EnemyHit,
        Sound::Explosion,
        Sound::PowerUp,
        Sound::PlayerDeath,
        Sound::Capsule,
        Sound::EnemyShot,
        Sound::GameOver,
    ];

    #[test]
    fn every_event_maps_to_its_own_effect() {
        let bank = test_bank();
        let names: Vec<&str> = SOUNDS.iter().map(|s| sfx_for(*s, &bank).name).collect();
        assert_eq!(
            names,
            [
                "shot",
                "enemy_hit",
                "explosion",
                "power_up",
                "player_death",
                "capsule",
                "enemy_shot",
                "game_over_sfx"
            ]
        );
    }

    #[test]
    fn the_pause_events_freeze_the_music_and_play_the_jingle() {
        let bank = test_bank();
        assert_eq!(sfx_for(Sound::Pause, &bank).name, "pause");
        let mut mixer = SoundMixer::new(SAMPLE_RATE as i32, title());
        mixer.music_event(Sound::Pause);
        let mut out = vec![0i16; 4_800];
        mixer.render(&mut out);
        assert!(
            out.iter().all(|sample| *sample == 0),
            "the music keeps playing"
        );
        mixer.play(sfx_for(Sound::Pause, &bank));
        mixer.render(&mut out);
        assert!(out.iter().any(|sample| *sample != 0), "no jingle");
        mixer.music_event(Sound::Resume);
        mixer.render(&mut out);
        assert!(out.iter().any(|sample| *sample != 0));
    }

    #[test]
    fn a_missing_or_unreadable_chunk_plays_in_silence() {
        let cases: [Option<&[u8]>; 2] = [None, Some(b"nope")];
        for chunk in cases {
            let bank = load_audio_bank(chunk);
            assert!(bank.music(Music::Boss).is_none());
            assert!(bank.sfx(Sfx::Shot).is_none());
            assert!(std::ptr::eq(sfx_for(Sound::Shot, &bank), &SILENCE));
        }
    }

    #[test]
    fn silence_renders_nothing() {
        let mut mixer = SoundMixer::new(SAMPLE_RATE as i32, &SILENCE);
        mixer.play(&SILENCE);
        let mut out = vec![1i16; 4_800];
        mixer.render(&mut out);
        assert!(out.iter().all(|sample| *sample == 0));
    }

    #[test]
    fn mixer_starts_with_the_title_music_and_is_audible() {
        let mut mixer = SoundMixer::new(SAMPLE_RATE as i32, title());
        let mut out = vec![0i16; 4_800];
        mixer.render(&mut out);
        assert!(out.iter().any(|sample| *sample != 0));
    }

    #[test]
    fn every_music_cue_can_be_selected() {
        let bank = test_bank();
        let mut mixer = SoundMixer::new(SAMPLE_RATE as i32, title());
        for music in Music::ALL {
            let timeline = bank.music(music).expect("music track");
            mixer.set_music(timeline);
            assert!(std::ptr::eq(mixer.music, timeline));
            let mut out = vec![0i16; 1_024];
            mixer.render(&mut out);
        }
    }

    #[test]
    fn events_mix_without_clipping() {
        let bank = test_bank();
        let mut mixer = SoundMixer::new(SAMPLE_RATE as i32, title());
        for sfx in [Sfx::Shot, Sfx::Explosion, Sfx::EnemyHit] {
            mixer.play(bank.sfx(sfx).expect("sfx"));
        }
        let mut out = vec![0i16; 9_600];
        mixer.render(&mut out);
        assert_eq!(out.len(), 9_600);
    }

    #[test]
    fn master_gain_scales_the_finished_mix() {
        let mut full = SoundMixer::new(SAMPLE_RATE as i32, title());
        let mut half = SoundMixer::new(SAMPLE_RATE as i32, title());
        half.set_gain(0.5);
        let mut loud = vec![0i16; 4_800];
        let mut quiet = vec![0i16; 4_800];
        full.render(&mut loud);
        half.render(&mut quiet);
        assert!(loud.iter().any(|sample| *sample != 0));
        for (loud, quiet) in loud.iter().zip(quiet.iter()) {
            let doubled = 2 * i32::from(*quiet);
            assert!(
                (i32::from(*loud) - doubled).abs() <= 2,
                "{loud} is not twice {quiet}"
            );
        }

        full.set_gain(0.0);
        full.render(&mut loud);
        assert!(loud.iter().all(|sample| *sample == 0));
    }

    #[test]
    fn disabled_backend_accepts_events() {
        let mut backend = AudioBackend::disabled();
        let bank = test_bank();
        backend.play(bank.sfx(Sfx::Shot).expect("sfx"));
        backend.play(bank.sfx(Sfx::GameOver).expect("sfx"));
        backend.set_gain(0.0);
    }

    #[test]
    fn disabled_backend_never_opens_a_device() {
        let mut backend = AudioBackend::disabled();
        let bank = test_bank();
        backend.set_music(bank.music(Music::Boss).expect("music"));
        backend.play(bank.sfx(Sfx::Explosion).expect("sfx"));
        assert!(backend.device.is_none());
    }

    /// A lost device (SDL_AUDIODEVICEREMOVED) used to leave the game mute for
    /// good. Reopening must bring the sound back with the music and volume the
    /// player had at that moment, not the ones the game started with.
    #[test]
    fn a_reopened_device_resumes_the_current_music_and_volume() {
        sdl2::hint::set("SDL_AUDIODRIVER", "dummy");
        let sdl = sdl2::init().expect("sdl");
        let bank = test_bank();
        let boss = bank.music(Music::Boss).expect("boss");
        let mut backend = AudioBackend::open(&sdl, title(), 1.0);
        assert!(backend.device.is_some());

        backend.set_music(boss);
        backend.set_gain(0.5);
        backend.reopen(&sdl);

        let device = backend.device.as_mut().expect("device is back");
        let mixer = device.lock();
        assert!(std::ptr::eq(mixer.music, boss));
        assert_eq!(mixer.gain, 0.5);
    }
}
