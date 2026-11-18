//! Checks on the pack's captured payload and the runtime rendering path.

use gradius_audio::apu::CPU_CLOCK_HZ;
use gradius_audio::timeline::{find_loop_start, Event, Player};
use gradius_audio::{wav, Apu, Engine, Music, Sfx, Timeline, SAMPLE_RATE};

mod common;
use common::{music as music_track, sfx as sfx_track};

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

fn samples_for(cycles: u32) -> usize {
    (f64::from(cycles) * f64::from(SAMPLE_RATE) / f64::from(CPU_CLOCK_HZ)).round() as usize
}

fn assert_well_formed(timeline: &Timeline) {
    assert!(
        !timeline.events.is_empty(),
        "{} has no events",
        timeline.name
    );
    assert!(
        timeline.total_cycles > 0,
        "{} has no duration",
        timeline.name
    );
    let mut last = 0;
    for event in timeline.events {
        assert!(
            event.at >= last,
            "{} events are not sorted at {}",
            timeline.name,
            event.at
        );
        last = event.at;
        assert!(
            (0x4000..=0x4017).contains(&event.register()),
            "{} writes outside the APU: ${:04X}",
            timeline.name,
            event.register()
        );
        assert!(
            event.at < timeline.total_cycles,
            "{} has an event past its end",
            timeline.name
        );
    }
    if let Some(point) = timeline.loop_point {
        assert!(
            point < timeline.total_cycles,
            "{} loop point is past the end",
            timeline.name
        );
        let index = usize::from(timeline.loop_index);
        assert!(
            index < timeline.events.len(),
            "{} loop index",
            timeline.name
        );
        assert!(
            timeline.events[index].at >= point,
            "{} loop_index does not point at the loop",
            timeline.name
        );
        if index > 0 {
            assert!(
                timeline.events[index - 1].at < point,
                "{} loop_index skips an event",
                timeline.name
            );
        }
    }
}

#[test]
fn captured_payload_is_well_formed() {
    for music in Music::ALL {
        assert_well_formed(music_track(music));
    }
    for effect in Sfx::ALL {
        assert_well_formed(sfx_track(effect));
    }
}

#[test]
fn music_lengths_match_the_original() {
    let intro = music_track(Music::Title);
    let stage1 = music_track(Music::Stage1);
    let stage1_b = music_track(Music::Stage1B);
    let boss = music_track(Music::Boss);
    let stage_clear = music_track(Music::StageClear);
    let game_over = music_track(Music::GameOver);
    // Title fanfare: a short one-shot.
    assert!((intro.seconds() - 1.0).abs() < 0.1, "{}", intro.seconds());
    assert!(!intro.is_looping());

    // Stage 1: 520-frame lead-in plus a 1024-frame loop (17.04 s).
    assert!(
        (stage1.seconds() - 25.69).abs() < 0.05,
        "{}",
        stage1.seconds()
    );
    assert_eq!(stage1.loop_point, Some(15_485_600));
    assert_eq!(
        samples_for(stage1.total_cycles - stage1.loop_point.unwrap()),
        samples_for(1_024 * 29_780)
    );

    // Second half of stage 1: 433-frame lead-in plus a 1248-frame loop.
    assert!(
        (stage1_b.seconds() - 27.97).abs() < 0.05,
        "{}",
        stage1_b.seconds()
    );
    assert_eq!(stage1_b.loop_point, Some(12_894_740));

    // Big Core theme: 79-frame lead-in plus a 384-frame loop.
    assert!((boss.seconds() - 7.70).abs() < 0.05, "{}", boss.seconds());
    assert_eq!(boss.loop_point, Some(2_352_620));

    // Stages 2-7 second themes. Stage 5 and 6 loop from the first frame; the
    // others have a one- or eight-frame lead-in.
    for (music, seconds, period, lead_in) in [
        (Music::Stage2, 22.38, 1_344, 1),
        (Music::Stage3, 20.57, 1_228, 8),
        (Music::Stage4, 20.78, 1_248, 1),
        (Music::Stage5, 17.97, 1_080, 0),
        (Music::Stage6, 33.94, 2_040, 0),
        (Music::Stage7, 23.69, 1_423, 1),
    ] {
        let music = music_track(music);
        assert!(
            (music.seconds() - seconds).abs() < 0.05,
            "{}: {}",
            music.name,
            music.seconds()
        );
        assert_eq!(
            music.loop_point,
            Some(lead_in * 29_780),
            "{} loop point",
            music.name
        );
        assert_eq!(
            samples_for(music.total_cycles - music.loop_point.unwrap()),
            samples_for(period * 29_780),
            "{} loop period",
            music.name
        );
    }

    assert!((stage_clear.seconds() - 1.40).abs() < 0.05);
    assert!(!stage_clear.is_looping());
    assert!((game_over.seconds() - 4.94).abs() < 0.05);
    assert!(!game_over.is_looping());

    for effect in Sfx::ALL {
        assert!(!sfx_track(effect).is_looping());
        assert!(sfx_track(effect).seconds() > 0.1, "{effect:?} is too short");
    }
}

#[test]
fn every_stage_theme_resolves_to_its_expected_cue() {
    use gradius_audio::driver::MUSIC_REQUESTS;

    // Twelve music cues in chunk-id order; ids 8-11 are the stage 4-7 themes.
    assert_eq!(Music::ALL.len(), 12);
    assert_eq!(MUSIC_REQUESTS.len(), Music::ALL.len());

    for (music, request, name) in [
        (Music::Stage1, 0x93, "stage1"),
        (Music::Stage2, 0x59, "stage2"),
        (Music::Stage3, 0x5B, "stage3"),
        (Music::Stage4, 0x5D, "stage4"),
        (Music::Stage5, 0x5F, "stage5"),
        (Music::Stage6, 0x61, "stage6"),
        (Music::Stage7, 0x63, "stage7"),
        (Music::Boss, 0xA5, "boss"),
    ] {
        assert_eq!(music_track(music).name, name, "{music:?}");
        let id = Music::ALL
            .iter()
            .position(|cue| *cue == music)
            .expect("cue in ALL");
        let expected = MUSIC_REQUESTS
            .iter()
            .position(|id| *id == request)
            .expect("request in the driver table");
        assert_eq!(id, expected, "{music:?} is not in the ${request:02X} slot");
    }
}

#[test]
fn timeline_enables_the_apu_before_the_first_note() {
    for music in Music::ALL {
        let first = music_track(music).events.first().expect("events");
        assert_eq!(first.register(), 0x4015);
        assert_eq!(first.value & 0x0F, 0x0F);
    }
}

#[test]
fn every_music_track_renders_audibly() {
    for music in Music::ALL {
        let timeline = music_track(music);
        let mut engine = Engine::new(SAMPLE_RATE);
        engine.play_music(timeline);
        let samples = (timeline.seconds() * SAMPLE_RATE as f32) as usize + 4_800;
        let mut out = vec![0.0f32; samples];
        engine.fill(&mut out);
        let level = rms(&out);
        assert!(level > 0.005, "{music:?} was silent (RMS {level})");
        assert!(out.iter().all(|sample| sample.abs() <= 1.0));
    }
}

#[test]
fn every_effect_is_audible() {
    for effect in Sfx::ALL {
        let timeline = sfx_track(effect);
        let mut engine = Engine::new(SAMPLE_RATE);
        engine.play_sfx(timeline);
        let samples = (timeline.seconds() * SAMPLE_RATE as f32) as usize + 4_800;
        let mut out = vec![0.0f32; samples];
        engine.fill(&mut out);
        let level = rms(&out);
        assert!(level > 0.0005, "{effect:?} was silent (RMS {level})");
        assert!(out.iter().all(|sample| sample.abs() <= 1.0));
    }
}

#[test]
fn music_and_a_pile_of_effects_stay_in_range() {
    let mut engine = Engine::new(SAMPLE_RATE);
    engine.play_music(music_track(Music::Stage1));
    for effect in Sfx::ALL {
        engine.play_sfx(sfx_track(effect));
    }
    let mut out = vec![0.0f32; SAMPLE_RATE as usize];
    engine.fill(&mut out);
    assert!(out
        .iter()
        .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
    assert!(rms(&out) > 0.01);
}

#[test]
fn looping_tracks_wrap_to_their_loop_point() {
    for music in [
        Music::Stage1,
        Music::Stage1B,
        Music::Boss,
        Music::Stage2,
        Music::Stage3,
        Music::Stage4,
        Music::Stage5,
        Music::Stage6,
        Music::Stage7,
    ] {
        let timeline = music_track(music);
        let point = timeline.loop_point.expect("looping track");
        let mut apu = Apu::new();
        let mut player = Player::new(timeline);
        player.advance(&mut apu, timeline.total_cycles);
        assert!(!player.is_finished(), "{music:?} finished");
        assert_eq!(player.position(), point, "{music:?} wrapped wrong");
    }
}

/// RMS of the triangle channel alone, normalised to `0.0..=1.0`.
fn triangle_rms(timeline: &'static Timeline) -> f32 {
    let mut player = Player::new(timeline).with_triangle_reload();
    let mut apu = Apu::new();
    let samples = (timeline.seconds() * SAMPLE_RATE as f32) as usize;
    let cycles_per_sample = f64::from(CPU_CLOCK_HZ) / f64::from(SAMPLE_RATE);
    let mut phase = 0.0f64;
    let mut sum = 0.0f64;
    for _ in 0..samples {
        phase += cycles_per_sample;
        let step = phase as u32;
        phase -= f64::from(step);
        player.advance(&mut apu, step);
        apu.step(step);
        let value = f64::from(apu.triangle.output()) / 15.0;
        sum += value * value;
    }
    (sum / samples.max(1) as f64).sqrt() as f32
}

#[test]
fn every_stage_theme_has_an_audible_triangle_bass() {
    for music in [
        Music::Stage2,
        Music::Stage3,
        Music::Stage4,
        Music::Stage5,
        Music::Stage6,
        Music::Stage7,
    ] {
        let level = triangle_rms(music_track(music));
        assert!(level > 0.1, "{music:?} triangle RMS {level} is too quiet");
    }
}

#[test]
fn stage_one_loop_seam_matches_the_first_pass() {
    assert_loop_seam(Music::Stage1);
}

#[test]
fn stage_theme_loop_seams_match_the_first_pass() {
    for music in [
        Music::Stage2,
        Music::Stage3,
        Music::Stage4,
        Music::Stage5,
        Music::Stage6,
        Music::Stage7,
    ] {
        assert_loop_seam(music);
    }
}

/// Render the lead-in, one loop, and a second loop, then compare the same
/// musical span in the first and second pass. The write streams repeat
/// exactly, so only APU phase can differ.
fn assert_loop_seam(music: Music) {
    let timeline = music_track(music);
    let point = timeline.loop_point.expect("looping track");
    let loop_samples = samples_for(timeline.total_cycles - point);
    let start = samples_for(point);
    let total = samples_for(timeline.total_cycles);
    let mut engine = Engine::new(SAMPLE_RATE);
    engine.play_music(timeline);
    let mut out = vec![0.0f32; total + loop_samples];
    engine.fill(&mut out);

    // Oscillator phase is free-running across the wrap, so compare the
    // largest sample-to-sample step in a window at the seam with the step at
    // the same musical position on the first pass. A cut or a dropped note
    // shows up here as a much larger step.
    let step = |from: usize| -> f32 {
        out[from..from + 2_048]
            .windows(2)
            .fold(0.0f32, |peak, pair| peak.max((pair[1] - pair[0]).abs()))
    };
    let first_step = step(start.saturating_sub(1_024));
    let seam_step = step(total.saturating_sub(1_024));
    assert!(
        seam_step <= first_step * 1.2 + 0.001,
        "{music:?} loop seam step {seam_step} exceeds first pass {first_step}"
    );
    assert!(out
        .iter()
        .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
}

#[test]
fn switching_music_crossfades_without_a_click_or_a_gap() {
    // Switching at 500 ms lands on a non-zero part of the stage-1 waveform;
    // cutting the old voice there produces a step far larger than the
    // waveform's own per-sample motion.
    let mut engine = Engine::new(SAMPLE_RATE);
    engine.play_music(music_track(Music::Stage1));
    let mut head = vec![0.0f32; 24_000];
    engine.fill(&mut head);
    engine.play_music(music_track(Music::Boss));
    let mut tail = vec![0.0f32; 4_800];
    engine.fill(&mut tail);

    let step = (tail[0] - head[head.len() - 1]).abs();
    let local = head[head.len() - 256..]
        .windows(2)
        .fold(0.0f32, |peak, pair| peak.max((pair[1] - pair[0]).abs()));
    assert!(
        step <= local * 1.5 + 0.01,
        "cue switch clicks: step {step} vs local {local}"
    );
    assert!(rms(&tail[..480]) > 0.005, "cue switch left a gap");
}

#[test]
fn find_loop_start_recovers_synthetic_lead_in() {
    // Synthetic capture: 5 frames of intro, then eight frames that repeat
    // three times. A frame is 100 cycles and holds one write.
    const FRAME: u32 = 100;
    const PERIOD: u32 = 8;
    const LEAD: u32 = 5;
    let pattern = [0x70u8, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77];
    let mut events = Vec::new();
    for frame in 0..(LEAD + 3 * PERIOD) {
        let value = if frame < LEAD {
            0x40 + frame as u8
        } else {
            pattern[((frame - LEAD) % PERIOD) as usize]
        };
        events.push(Event::new(frame * FRAME + 7, 0x4000, value));
    }
    assert_eq!(find_loop_start(&events, FRAME, PERIOD, 16), Some(LEAD));

    // A stream with no repeated period must not produce a loop.
    let mut unique = Vec::new();
    for frame in 0..(3 * PERIOD) {
        unique.push(Event::new(frame * FRAME + 7, 0x4000, frame as u8));
    }
    assert_eq!(find_loop_start(&unique, FRAME, PERIOD, 16), None);
}

#[test]
fn dumped_wav_has_the_expected_container() {
    let mut engine = Engine::new(SAMPLE_RATE);
    engine.play_music(music_track(Music::Boss));
    let mut out = vec![0.0f32; 9_600];
    engine.fill(&mut out);
    let wav = wav::encode(SAMPLE_RATE, &out);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
        SAMPLE_RATE
    );
    assert_eq!(
        u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]),
        (out.len() * 2) as u32
    );
    assert!(wav.len() > 44 + 1_000);
}
