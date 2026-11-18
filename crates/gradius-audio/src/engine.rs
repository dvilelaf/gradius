//! Multi-voice mixer: one looping music timeline plus transient sound effects.
//!
//! Each effect voice owns its own [`Apu`], so an effect never steals a music
//! channel or another effect's state. A new music voice inherits the previous
//! music APU, because the cartridge sound engine keeps running across cue
//! changes and some captured tracks rely on the triangle linear counter that
//! carried over. Voices are clocked together at the CPU rate and their mixer
//! outputs are summed, then limited to the `-1.0..=1.0` range.

use crate::apu::{Apu, CPU_CLOCK_HZ};
use crate::timeline::{Player, Timeline};

/// NTSC frame rate; music captured from the oracle counts frames, and the
/// player is driven by the CPU cycle clock instead.
pub const TICKS_PER_SECOND: f64 = 60.098_8;

const MUSIC_GAIN: f32 = 0.85;
const SFX_GAIN: f32 = 0.70;
/// Enough overlap for the busiest moments of stage 1.
pub const MAX_SFX: usize = 6;
/// Music is crossfaded over this many milliseconds so a cue change neither
/// cuts a sounding waveform (a click) nor inserts silence (a gap).
const CROSSFADE_MS: u32 = 10;

struct Voice {
    apu: Apu,
    player: Player,
    gain: f32,
}

/// A previous music voice being faded out while its replacement plays.
struct FadingVoice {
    voice: Voice,
    remaining: u32,
    total: u32,
}

impl Voice {
    fn new(timeline: &'static Timeline, gain: f32) -> Self {
        Self {
            apu: Apu::new(),
            player: Player::new(timeline),
            gain,
        }
    }

    /// A music voice that inherits the previous music APU and keeps the
    /// triangle linear counter reloaded.
    fn music(timeline: &'static Timeline, gain: f32, apu: Apu) -> Self {
        Self {
            apu,
            player: Player::new(timeline).with_triangle_reload(),
            gain,
        }
    }

    fn mix(&self) -> f32 {
        self.apu.mix() * self.gain
    }
}

/// Runtime audio engine. Fill it with [`Engine::fill`] at the source rate.
pub struct Engine {
    sample_rate: u32,
    cpu_phase: u64,
    music: Option<Voice>,
    /// Music started automatically when the current one-shot finishes.
    next_music: Option<&'static Timeline>,
    fading: Vec<FadingVoice>,
    /// Samples left in the current music's fade-in.
    fade_in: u32,
    fade_total: u32,
    sfx: Vec<Voice>,
    /// The music voice is frozen (the stage pause); effects keep playing.
    music_paused: bool,
    peak: f32,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(crate::apu::SAMPLE_RATE)
    }
}

impl Engine {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            cpu_phase: 0,
            music: None,
            next_music: None,
            fading: Vec::new(),
            fade_in: 0,
            fade_total: 0,
            sfx: Vec::new(),
            music_paused: false,
            peak: 0.0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Start (or replace) the looping/one-shot music track.
    ///
    /// A track that is already playing is faded out over [`CROSSFADE_MS`] while
    /// the new one fades in, so the switch is continuous even at a non-zero
    /// point in the old waveform.
    pub fn play_music(&mut self, timeline: &'static Timeline) {
        let apu = self.music_apu();
        let replacing = self.retire_music();
        self.music = Some(Voice::music(timeline, MUSIC_GAIN, apu));
        self.next_music = None;
        if replacing {
            self.fade_total = self.crossfade_samples();
            self.fade_in = self.fade_total;
        }
    }

    /// The APU state to hand to the next music voice. The cartridge never
    /// resets the sound engine between cues, so the triangle linear counter
    /// and the other channel registers carry over.
    fn music_apu(&self) -> Apu {
        self.music
            .as_ref()
            .map(|voice| voice.apu.clone())
            .unwrap_or_default()
    }

    /// Move the current music voice to the fade-out list. Returns whether
    /// there was one.
    fn retire_music(&mut self) -> bool {
        let Some(voice) = self.music.take() else {
            return false;
        };
        let total = self.crossfade_samples();
        self.fading.push(FadingVoice {
            voice,
            remaining: total,
            total,
        });
        true
    }

    fn crossfade_samples(&self) -> u32 {
        (self.sample_rate / 1_000 * CROSSFADE_MS).max(1)
    }

    /// Play `first` once, then switch to `next` automatically. Used for the
    /// start jingle followed by the stage-1 BGM.
    pub fn play_music_sequence(
        &mut self,
        first: &'static Timeline,
        next: Option<&'static Timeline>,
    ) {
        let apu = self.music_apu();
        if self.retire_music() {
            self.fade_total = self.crossfade_samples();
            self.fade_in = self.fade_total;
        }
        self.music = Some(Voice::music(first, MUSIC_GAIN, apu));
        self.next_music = next;
    }

    /// True when no music is playing.
    pub fn music_finished(&self) -> bool {
        self.music.is_none()
    }

    /// Queue a sound effect. The oldest effect is dropped if the pool is full.
    pub fn play_sfx(&mut self, timeline: &'static Timeline) {
        if self.sfx.len() >= MAX_SFX {
            self.sfx.remove(0);
        }
        self.sfx.push(Voice::new(timeline, SFX_GAIN));
    }

    /// Freeze the music where it is and silence it; effects keep playing.
    /// `$9AE8` saves the sound engine's state, `$9B1D` restores it.
    pub fn pause_music(&mut self) {
        self.music_paused = true;
    }

    /// Continue the music from the point [`Engine::pause_music`] froze it.
    pub fn resume_music(&mut self) {
        self.music_paused = false;
    }

    /// Stop all sound.
    pub fn reset(&mut self) {
        self.music_paused = false;
        self.music = None;
        self.next_music = None;
        self.fading.clear();
        self.fade_in = 0;
        self.fade_total = 0;
        self.sfx.clear();
        self.cpu_phase = 0;
        self.peak = 0.0;
    }

    /// Largest absolute sample produced since the last [`Engine::take_peak`].
    pub fn take_peak(&mut self) -> f32 {
        std::mem::take(&mut self.peak)
    }

    pub fn active_voices(&self) -> usize {
        usize::from(self.music.is_some()) + self.sfx.len()
    }

    /// Render exactly `out.len()` samples.
    pub fn fill(&mut self, out: &mut [f32]) {
        for sample in out.iter_mut() {
            self.cpu_phase += u64::from(CPU_CLOCK_HZ);
            while self.cpu_phase >= u64::from(self.sample_rate) {
                self.cpu_phase -= u64::from(self.sample_rate);
                if let Some(voice) = self.music.as_mut().filter(|_| !self.music_paused) {
                    voice.player.advance(&mut voice.apu, 1);
                    voice.apu.step(1);
                }
                for fading in &mut self.fading {
                    fading.voice.player.advance(&mut fading.voice.apu, 1);
                    fading.voice.apu.step(1);
                }
                for voice in &mut self.sfx {
                    voice.player.advance(&mut voice.apu, 1);
                    voice.apu.step(1);
                }
            }

            let mut mix = 0.0;
            if let Some(voice) = self.music.as_ref().filter(|_| !self.music_paused) {
                mix += voice.mix() * self.music_fade();
            }
            for fading in &self.fading {
                let gain = fading.remaining as f32 / fading.total as f32;
                mix += fading.voice.mix() * gain;
            }
            for voice in &self.sfx {
                mix += voice.mix();
            }
            let value = mix.clamp(-1.0, 1.0);
            self.peak = self.peak.max(value.abs());
            *sample = value;
            self.fade_in = self.fade_in.saturating_sub(1);
            for fading in &mut self.fading {
                fading.remaining = fading.remaining.saturating_sub(1);
            }
            if self
                .music
                .as_ref()
                .is_some_and(|voice| voice.player.is_finished())
            {
                self.start_queued_music();
            }
        }
        self.fading.retain(|fading| fading.remaining > 0);
        self.retire_finished();
    }

    fn music_fade(&self) -> f32 {
        if self.fade_total == 0 {
            1.0
        } else {
            (self.fade_total - self.fade_in) as f32 / self.fade_total as f32
        }
    }

    fn retire_finished(&mut self) {
        self.sfx.retain(|voice| !voice.player.is_finished());
        if self
            .music
            .as_ref()
            .is_some_and(|voice| voice.player.is_finished())
        {
            self.start_queued_music();
        }
    }

    /// Replace a finished music voice with the queued one, carrying the APU.
    fn start_queued_music(&mut self) {
        let apu = self.music_apu();
        self.music = None;
        if let Some(next) = self.next_music.take() {
            self.music = Some(Voice::music(next, MUSIC_GAIN, apu));
            self.fade_total = self.crossfade_samples();
            self.fade_in = self.fade_total;
        }
    }

    /// Number of samples in one NTSC frame at this rate, rounded down.
    pub fn samples_per_frame(&self) -> usize {
        (f64::from(self.sample_rate) / TICKS_PER_SECOND) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{Event, Timeline};

    const MUSIC_EVENTS: [Event; 5] = [
        Event::new(0, 0x4015, 0x05),
        Event::new(0, 0x4000, 0xBF),
        Event::new(0, 0x4002, 0x40),
        Event::new(0, 0x4003, 0x08),
        Event::new(0, 0x4008, 0xFF),
    ];
    const MUSIC: Timeline = Timeline {
        name: "music",
        events: &MUSIC_EVENTS,
        total_cycles: 2_000_000,
        loop_point: Some(0),
        loop_index: 0,
    };
    const SFX_EVENTS: [Event; 3] = [
        Event::new(0, 0x4015, 0x09),
        Event::new(0, 0x400C, 0x1F),
        Event::new(0, 0x400E, 0x01),
    ];
    const SFX: Timeline = Timeline {
        name: "sfx",
        events: &SFX_EVENTS,
        total_cycles: 20_000,
        loop_point: None,
        loop_index: 0,
    };

    const TONE: Timeline = Timeline {
        name: "tone",
        events: &MUSIC_EVENTS,
        total_cycles: 400_000,
        loop_point: None,
        loop_index: 0,
    };

    #[test]
    fn silent_engine_renders_silence() {
        let mut engine = Engine::new(48_000);
        let mut out = [1.0f32; 256];
        engine.fill(&mut out);
        assert!(out.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn music_is_audible_and_bounded() {
        let mut engine = Engine::new(48_000);
        engine.play_music(&MUSIC);
        let mut out = vec![0.0f32; 4_800];
        engine.fill(&mut out);
        let peak = out
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.01, "music was silent");
        assert!(peak <= 1.0);
    }

    #[test]
    fn every_music_track_has_an_audible_triangle_channel() {
        let mut engine = Engine::new(48_000);
        let pack = std::fs::read(
            std::env::var_os("GRADIUS_PACK")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../assets/gradius.pak")
                }),
        )
        .unwrap_or_else(|_| {
            panic!("no extracted pack: run GRADIUS_ROM=<rom> scripts/extract-data")
        });
        let chunk = gradius_assets::Pack::parse(&pack)
            .unwrap()
            .audio
            .expect("audio chunk");
        let bank = crate::AudioBank::from_chunk(&chunk).unwrap();
        for music in crate::data::Music::ALL {
            let timeline = bank.music(music).expect("music track");
            engine.play_music(timeline);
            let voice = engine.music.as_mut().expect("music voice");
            let mut sum = 0.0f64;
            let mut samples = 0u64;
            let mut remaining = timeline.total_cycles;
            while remaining > 0 {
                let step = remaining.min(16);
                voice.player.advance(&mut voice.apu, step);
                voice.apu.step(step);
                remaining -= step;
                let out = f64::from(voice.apu.triangle.output());
                sum += out * out;
                samples += 1;
            }
            let rms = (sum / samples as f64).sqrt();
            println!("{music:?}: triangle RMS {rms:.3}");
            assert!(
                rms > 0.1,
                "{music:?} triangle RMS is {rms:.3}; the bass channel is silent"
            );
        }
    }

    #[test]
    fn a_paused_music_is_silent_and_resumes_where_it_stopped() {
        let peak = |engine: &mut Engine| {
            let mut out = vec![0.0f32; 4_800];
            engine.fill(&mut out);
            out.iter()
                .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
        };
        let mut engine = Engine::new(48_000);
        engine.play_music(&MUSIC);
        peak(&mut engine);
        engine.pause_music();
        assert_eq!(peak(&mut engine), 0.0);
        let position = engine
            .music
            .as_ref()
            .map(|voice| voice.player.is_finished());
        assert_eq!(position, Some(false));
        engine.play_sfx(&TONE);
        assert!(peak(&mut engine) > 0.01, "effects still play while paused");
        engine.resume_music();
        assert!(peak(&mut engine) > 0.01);
    }

    #[test]
    fn effects_are_retired_when_finished() {
        let mut engine = Engine::new(48_000);
        engine.play_sfx(&SFX);
        assert_eq!(engine.active_voices(), 1);
        let mut out = vec![0.0f32; 48];
        engine.fill(&mut out); // 1 ms, far shorter than the 11 ms effect
        assert_eq!(engine.active_voices(), 1);
        let mut out = vec![0.0f32; 4_800];
        engine.fill(&mut out);
        assert_eq!(engine.active_voices(), 0);
    }

    #[test]
    fn overlapping_voices_never_clip() {
        let mut engine = Engine::new(48_000);
        engine.play_music(&MUSIC);
        for _ in 0..MAX_SFX + 3 {
            engine.play_sfx(&SFX);
        }
        assert!(engine.sfx.len() <= MAX_SFX);
        let mut out = vec![0.0f32; 4_800];
        engine.fill(&mut out);
        assert_eq!(out.len(), 4_800);
        assert!(out
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
    }

    #[test]
    fn fill_writes_every_sample() {
        let mut engine = Engine::new(44_100);
        engine.play_music(&MUSIC);
        let mut out = vec![f32::NAN; 999];
        engine.fill(&mut out);
        assert!(out.iter().all(|sample| sample.is_finite()));
    }
}
