//! The extracted pack's audio chunk: every track decodes, plays and re-encodes
//! to the same bytes. Needs the pack: `GRADIUS_ROM=<rom> scripts/extract-data`.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use gradius_audio::chunk::{encode_chunk, AudioBank};
use gradius_audio::{wav, Engine, Music, Sfx, Timeline, SAMPLE_RATE};

mod common;

/// Every music track the pack carries must decode to its own non-empty
/// timeline. A missing or duplicated cue is the failure mode behind "only one
/// track plays".
#[test]
fn a_pack_decodes_all_twelve_distinct_music_tracks() {
    let mut hashes = Vec::new();
    for music in Music::ALL {
        let track = common::music(music);
        assert_eq!(track.name, music.name());
        assert_eq!(track.is_looping(), music.loops(), "{music:?} loop flag");
        assert!(!track.events.is_empty(), "{music:?} has no register writes");
        assert!(track.total_cycles > 0, "{music:?} has no length");
        let mut hasher = DefaultHasher::new();
        track.name.hash(&mut hasher);
        track.total_cycles.hash(&mut hasher);
        track.events.iter().for_each(|event| {
            event.at.hash(&mut hasher);
            event.addr.hash(&mut hasher);
            event.value.hash(&mut hasher);
        });
        let hash = hasher.finish();
        assert!(
            !hashes.contains(&hash),
            "{music:?} decodes to the same data as another track"
        );
        hashes.push(hash);
        println!(
            "{music:?}: name={} cycles={} events={} hash={hash:016X}",
            track.name,
            track.total_cycles,
            track.events.len()
        );
    }
    assert_eq!(hashes.len(), Music::ALL.len());
    for sfx in Sfx::ALL {
        assert_eq!(common::sfx(sfx).name, sfx.name());
    }
}

/// Render one timeline through the real engine and return its WAV bytes.
fn render(timeline: &'static Timeline, music: bool) -> Vec<u8> {
    let samples = (timeline.seconds() * SAMPLE_RATE as f32) as usize + 4_800;
    let mut engine = Engine::new(SAMPLE_RATE);
    if music {
        engine.play_music(timeline);
    } else {
        engine.play_sfx(timeline);
    }
    let mut out = vec![0.0f32; samples];
    engine.fill(&mut out);
    wav::encode(SAMPLE_RATE, &out)
}

fn dump(name: &str, bytes: &[u8]) {
    let Some(directory) = std::env::var_os("AUDIO_DUMP_DIR") else {
        return;
    };
    let path = PathBuf::from(directory).join(format!("{name}.wav"));
    std::fs::write(path, bytes).expect("write dump");
}

/// A second, independent decode of the chunk renders the same WAV bytes.
#[test]
fn a_pack_only_run_plays_every_track_deterministically() {
    let again = AudioBank::from_chunk(common::chunk()).expect("audio chunk decodes");

    for music in Music::ALL {
        let loaded = common::music(music);
        let other = again.music(music).expect("music track in pack");
        assert!(!std::ptr::eq(loaded, other), "{music:?}");
        let wav = render(loaded, true);
        assert_eq!(wav, render(other, true), "{music:?} WAV differs");
        dump(&format!("music-{music:?}"), &wav);
    }

    for sfx in Sfx::ALL {
        let loaded = common::sfx(sfx);
        let other = again.sfx(sfx).expect("sfx track in pack");
        let wav = render(loaded, false);
        assert_eq!(wav, render(other, false), "{sfx:?} WAV differs");
        dump(&format!("sfx-{sfx:?}"), &wav);
    }
}

/// Re-encoding the decoded timelines reproduces the chunk's track records
/// byte for byte (the driver section is the only part not re-encoded).
#[test]
fn the_chunk_re_encodes_to_the_same_bytes() {
    let chunk = common::chunk();
    let music: Vec<Timeline> = Music::ALL.iter().map(|m| *common::music(*m)).collect();
    let sfx: Vec<Timeline> = Sfx::ALL.iter().map(|s| *common::sfx(*s)).collect();
    let tracks = encode_chunk(&music, &sfx, &[]);
    let prefix = tracks.len() - 4;
    assert_eq!(&chunk[..prefix], &tracks[..prefix]);
    if let Some(directory) = std::env::var_os("AUDIO_DUMP_DIR") {
        std::fs::write(PathBuf::from(directory).join("audio.chunk"), chunk).expect("write chunk");
    }
}
