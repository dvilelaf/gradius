//! Maps platform-agnostic music cues to captured audio timelines.

use gradius_app::MusicCue;
use gradius_audio::{AudioBank, Music, Timeline, SILENCE};

/// The pack's timeline for a cue, or silence when the pack does not carry the
/// track.
pub fn music_for(cue: MusicCue, bank: &AudioBank) -> &'static Timeline {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::test_bank;
    use gradius_app::{MusicDirector, BOSS_SCORE, BOSS_SCROLL, STAGE1_SECOND_HALF_SCROLL};
    use gradius_audio::{wav, Engine, SAMPLE_RATE, TICKS_PER_SECOND};
    use gradius_core::State;

    const STAGE1_MUSIC: u8 = 0x96;
    const STAGE1_TRIGGER: u8 = 0x0C;

    const CUES: [MusicCue; 12] = [
        MusicCue::Title,
        MusicCue::Stage1,
        MusicCue::Stage1B,
        MusicCue::Stage2,
        MusicCue::Stage3,
        MusicCue::Stage4,
        MusicCue::Stage5,
        MusicCue::Stage6,
        MusicCue::Stage7,
        MusicCue::Boss,
        MusicCue::StageClear,
        MusicCue::GameOver,
    ];

    #[test]
    fn every_cue_selects_its_own_track() {
        let bank = test_bank();
        let mut names = Vec::new();
        for cue in CUES {
            let timeline = music_for(cue, &bank);
            assert!(!std::ptr::eq(timeline, &SILENCE), "{cue:?} is silent");
            assert!(!names.contains(&timeline.name), "{cue:?} repeats a track");
            names.push(timeline.name);
        }
    }

    #[test]
    fn a_bank_without_tracks_selects_silence_for_every_cue() {
        let bank = AudioBank::default();
        for cue in CUES {
            assert!(std::ptr::eq(music_for(cue, &bank), &SILENCE), "{cue:?}");
        }
    }

    /// 3,000 frames of scripted public game data: title, both stage-1 halves,
    /// the Big Core, the clear fanfare, game over and a restart. The rendered
    /// soundtrack must contain every cue and stay inside full scale.
    #[test]
    fn three_thousand_frame_soundtrack_has_every_cue_without_clipping() {
        let script = |frame: u64| -> (State, u16, u32) {
            match frame {
                0..60 => (State::Title, 0, 0),
                60..660 => (State::Playing, 0, 0),
                660..960 => (State::Playing, STAGE1_SECOND_HALF_SCROLL, 0),
                960..1_260 => (State::Playing, BOSS_SCROLL, 0),
                1_260..1_500 => (State::Playing, BOSS_SCROLL, BOSS_SCORE),
                1_500..1_740 => (State::GameOver, BOSS_SCROLL, BOSS_SCORE),
                _ => (State::Playing, 0, 0),
            }
        };

        let bank = test_bank();
        let mut director = MusicDirector::new();
        let mut engine = Engine::new(SAMPLE_RATE);
        engine.play_music(music_for(director.cue(), &bank));
        let mut samples = Vec::new();
        let mut rendered = 0;
        let mut seen = Vec::new();
        for frame in 0..3_000u64 {
            let (state, scroll, score) = script(frame);
            if let Some(cue) = director.observe(state, scroll, score, STAGE1_MUSIC, STAGE1_TRIGGER)
            {
                seen.push(cue);
                engine.play_music(music_for(cue, &bank));
            }
            let target =
                (((frame + 1) as f64 * f64::from(SAMPLE_RATE)) / TICKS_PER_SECOND).round() as usize;
            let mut buffer = vec![0.0; target - rendered];
            engine.fill(&mut buffer);
            samples.extend_from_slice(&buffer);
            rendered = target;
        }

        assert_eq!(
            seen,
            [
                MusicCue::Stage1,
                MusicCue::Stage1B,
                MusicCue::Boss,
                MusicCue::StageClear,
                MusicCue::GameOver,
                MusicCue::Stage1,
            ]
        );
        assert_eq!(
            samples.len(),
            ((3_000.0 * f64::from(SAMPLE_RATE)) / TICKS_PER_SECOND).round() as usize
        );
        let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak < 1.0, "clipped at {peak}");
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        assert!(rms > 0.01, "soundtrack is silent");
        let bytes = wav::encode(SAMPLE_RATE, &samples);
        assert_eq!(bytes.len(), 44 + samples.len() * 2);
    }
}
