//! End-to-end check of the headless frontend path.
//!
//! The real pack is derived from a cartridge and never committed, so the test
//! builds a synthetic but format-valid pack with the same chunk layout. No SDL
//! window or audio device is opened: `--frames` returns before SDL init.

use std::path::{Path, PathBuf};
use std::process::Command;

use gradius_pack::{PackBuilder, CHUNK_AUDIO, TILES_PER_BANK, TILE_BANKS};

fn synthetic_pack_bytes(audio: Option<&[u8]>) -> Vec<u8> {
    let banks: Vec<Vec<[u8; 64]>> = vec![vec![[0u8; 64]; TILES_PER_BANK]; TILE_BANKS];
    let mut builder = PackBuilder::new()
        .tiles(&banks)
        .palettes(&[[0x0f, 0x0f, 0x0f]])
        .meta("synthetic test pack\n")
        .chunk(gradius_pack::CHUNK_ROM, real_rom_chunk());
    if let Some(chunk) = audio {
        builder = builder.chunk(CHUNK_AUDIO, chunk.to_vec());
    }
    builder.finish()
}

/// The `CHUNK_ROM` of the extracted pack: the core reads its tables from it.
fn real_rom_chunk() -> Vec<u8> {
    let path = std::env::var_os("GRADIUS_PACK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak")
        });
    gradius_pack::read_chunk_from_file(&path, gradius_pack::CHUNK_ROM)
        .unwrap_or_else(|_| panic!("no extracted pack: run GRADIUS_ROM=<rom> scripts/extract-data"))
}

/// The audio chunk of the extracted pack: `GRADIUS_PACK` or `assets/gradius.pak`.
fn real_audio_chunk() -> Vec<u8> {
    let path = std::env::var_os("GRADIUS_PACK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak")
        });
    let bytes = std::fs::read(path).unwrap_or_else(|_| {
        panic!("no extracted pack: run GRADIUS_ROM=<rom> scripts/extract-data")
    });
    gradius_assets::Pack::parse(&bytes)
        .expect("pack parses")
        .audio
        .expect("the pack has no audio chunk")
}

fn synthetic_pack(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "gradius-headless-{}-{name}.pak",
        std::process::id()
    ));
    std::fs::write(&path, synthetic_pack_bytes(None)).expect("write pack");
    path
}

#[test]
fn headless_run_prints_a_summary_and_exits_zero() {
    let pack = synthetic_pack("ok");
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .args([
            "--pack",
            pack.to_str().unwrap(),
            "--frames",
            "60",
            "--no-audio",
            "--scale",
            "1",
        ])
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_AUDIODRIVER", "dummy")
        .output()
        .expect("run gradius");
    let _ = std::fs::remove_file(&pack);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("headless: frames=60"), "stdout: {stdout}");
    assert!(stdout.contains("hash=0x"), "stdout: {stdout}");
}

#[test]
fn headless_accepts_volume_and_never_opens_a_device() {
    let pack = synthetic_pack("volume");
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .args([
            "--pack",
            pack.to_str().unwrap(),
            "--frames",
            "30",
            "--volume",
            "35",
            "--scale",
            "1",
        ])
        // A deliberately unusable driver: headless must not initialise audio.
        .env("SDL_AUDIODRIVER", "no-such-driver")
        .env("SDL_VIDEODRIVER", "no-such-driver")
        .output()
        .expect("run gradius");
    let _ = std::fs::remove_file(&pack);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("headless: frames=30"), "stdout: {stdout}");
}

#[test]
fn dump_audio_writes_a_non_silent_wav() {
    let pack =
        std::env::temp_dir().join(format!("gradius-headless-{}-wav.pak", std::process::id()));
    std::fs::write(&pack, synthetic_pack_bytes(Some(&real_audio_chunk()))).expect("write pack");
    let wav =
        std::env::temp_dir().join(format!("gradius-headless-{}-sound.wav", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .args([
            "--pack",
            pack.to_str().unwrap(),
            "--frames",
            "60",
            "--no-audio",
            "--dump-audio",
            wav.to_str().unwrap(),
            "--scale",
            "1",
        ])
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_AUDIODRIVER", "dummy")
        .output()
        .expect("run gradius");
    let _ = std::fs::remove_file(&pack);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&wav).expect("read wav");
    let _ = std::fs::remove_file(&wav);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(
        u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
        48_000
    );
    let pcm: Vec<i16> = bytes[44..]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    assert!(pcm.len() > 40_000, "only {} samples", pcm.len());
    assert!(
        pcm.iter().any(|sample| sample.unsigned_abs() > 100),
        "the soundtrack is silent"
    );
    assert!(pcm.iter().all(|sample| *sample != i16::MIN));
}

#[test]
fn autoplay_dump_is_frame_exact_and_bounded() {
    let frames = 3_000u64;
    let pack = std::env::temp_dir().join(format!(
        "gradius-headless-{}-autoplay-wav.pak",
        std::process::id()
    ));
    std::fs::write(&pack, synthetic_pack_bytes(Some(&real_audio_chunk()))).expect("write pack");
    let wav = std::env::temp_dir().join(format!(
        "gradius-headless-{}-autoplay.wav",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .args([
            "--pack",
            pack.to_str().unwrap(),
            "--frames",
            &frames.to_string(),
            "--autoplay",
            "--no-audio",
            "--dump-audio",
            wav.to_str().unwrap(),
            "--scale",
            "1",
        ])
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_AUDIODRIVER", "dummy")
        .output()
        .expect("run gradius");
    let _ = std::fs::remove_file(&pack);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&wav).expect("read wav");
    let _ = std::fs::remove_file(&wav);
    let pcm: Vec<i16> = bytes[44..]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let expected = ((frames as f64 * f64::from(gradius_audio::SAMPLE_RATE))
        / gradius_audio::TICKS_PER_SECOND)
        .round() as usize;
    assert_eq!(pcm.len(), expected);

    let peak = pcm
        .iter()
        .map(|sample| sample.unsigned_abs())
        .max()
        .unwrap();
    let rms = (pcm
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum::<f64>()
        / pcm.len() as f64)
        .sqrt();
    assert!(peak < i16::MAX as u16, "clipped at {peak}");
    assert!((200.0..8_000.0).contains(&rms), "RMS {rms} out of bounds");
}

#[test]
fn dump_audio_without_frames_is_rejected() {
    let pack = synthetic_pack("wav-no-frames");
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .args([
            "--pack",
            pack.to_str().unwrap(),
            "--dump-audio",
            "unused.wav",
        ])
        .output()
        .expect("run gradius");
    let _ = std::fs::remove_file(&pack);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--dump-audio"), "stderr: {stderr}");
}

fn run_dump(pack: &Path, wav: &Path, frames: u64) -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .arg("--pack")
        .arg(pack)
        .arg("--frames")
        .arg(frames.to_string())
        .arg("--no-audio")
        .arg("--dump-audio")
        .arg(wav)
        .arg("--scale")
        .arg("1")
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_AUDIODRIVER", "dummy")
        .output()
        .expect("run gradius");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(wav).expect("read wav");
    let _ = std::fs::remove_file(wav);
    bytes
}

fn peak_and_rms(wav: &[u8]) -> (u16, f64) {
    let pcm: Vec<i16> = wav[44..]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let peak = pcm
        .iter()
        .map(|sample| sample.unsigned_abs())
        .max()
        .unwrap();
    let rms = (pcm
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum::<f64>()
        / pcm.len() as f64)
        .sqrt();
    (peak, rms)
}

/// A fixed 900-frame no-input run plays the title cue from the pack. The
/// soundtrack must be audible and bounded, and a pack without an audio chunk
/// must play in silence.
#[test]
fn the_pack_audio_chunk_is_audible_and_its_absence_is_silent() {
    let dir = std::env::temp_dir();
    let prefix = format!("gradius-headless-{}-pack-audio", std::process::id());
    let pack_with = dir.join(format!("{prefix}.pak"));
    let pack_without = dir.join(format!("{prefix}-silent.pak"));
    let wav_with = dir.join(format!("{prefix}.wav"));
    let wav_without = dir.join(format!("{prefix}-silent.wav"));
    let chunk = real_audio_chunk();
    std::fs::write(&pack_with, synthetic_pack_bytes(Some(&chunk))).expect("write pack");
    std::fs::write(&pack_without, synthetic_pack_bytes(None)).expect("write pack");

    let from_pack = run_dump(&pack_with, &wav_with, 900);
    let silent = run_dump(&pack_without, &wav_without, 900);
    let _ = std::fs::remove_file(&pack_with);
    let _ = std::fs::remove_file(&pack_without);

    assert_eq!(from_pack.len(), silent.len(), "WAV lengths differ");
    let (peak, rms) = peak_and_rms(&from_pack);
    assert!(peak < i16::MAX as u16, "clipped at {peak}");
    assert!((200.0..8_000.0).contains(&rms), "RMS {rms} out of bounds");
    assert_eq!(peak_and_rms(&silent).0, 0, "no audio chunk must be silent");
}

#[test]
fn fit_aspect_headless_smoke_run_reports_the_viewport() {
    let pack = synthetic_pack("fit-aspect");
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .args([
            "--pack",
            pack.to_str().unwrap(),
            "--frames",
            "60",
            "--fit",
            "aspect",
            "--print-viewport",
            "--no-audio",
            "--scale",
            "1",
        ])
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_AUDIODRIVER", "dummy")
        .output()
        .expect("run gradius");
    let _ = std::fs::remove_file(&pack);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("headless: frames=60"), "stdout: {stdout}");
    // The 16:9 and 21:9 samples must pillarbox the 256x240 image, not stretch.
    assert!(
        stdout.contains("viewport: fit=aspect window=1920x1080 dest=384,0 1152x1080 scale=4.50"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("viewport: fit=aspect window=2560x1080 dest=704,0 1152x1080 scale=4.50"),
        "stdout: {stdout}"
    );
}

#[test]
fn rejects_an_unknown_fit_mode() {
    let pack = synthetic_pack("fit-bad");
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .args(["--pack", pack.to_str().unwrap(), "--fit", "letterbox"])
        .output()
        .expect("run gradius");
    let _ = std::fs::remove_file(&pack);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--fit") || stderr.contains("invalid fit"),
        "stderr: {stderr}"
    );
}

#[test]
fn malformed_pack_fails_with_an_extract_hint() {
    let path =
        std::env::temp_dir().join(format!("gradius-headless-{}-bad.pak", std::process::id()));
    std::fs::write(&path, b"not a pack").expect("write bad pack");
    let output = Command::new(env!("CARGO_BIN_EXE_gradius"))
        .args(["--pack", path.to_str().unwrap(), "--frames", "1"])
        .output()
        .expect("run gradius");
    let _ = std::fs::remove_file(&path);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("bad magic"), "stderr: {stderr}");
    assert!(stderr.contains("scripts/extract-data"), "stderr: {stderr}");
}

/// The overlay is a windowed frontend feature; the flag must parse but leave
/// the headless summary byte-identical, so smoke hashes never change.
#[test]
fn debug_overlay_flag_leaves_headless_output_unchanged() {
    let pack = synthetic_pack("overlay");
    let run = |extra: &[&str]| {
        let mut args = vec![
            "--pack",
            pack.to_str().unwrap(),
            "--frames",
            "30",
            "--no-audio",
            "--scale",
            "1",
        ];
        args.extend_from_slice(extra);
        Command::new(env!("CARGO_BIN_EXE_gradius"))
            .args(&args)
            .env("SDL_VIDEODRIVER", "dummy")
            .env("SDL_AUDIODRIVER", "dummy")
            .output()
            .expect("run gradius")
    };
    let plain = run(&[]);
    let debug = run(&["--debug-overlay"]);
    let _ = std::fs::remove_file(&pack);

    assert!(
        plain.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert!(
        debug.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&debug.stderr)
    );
    assert_eq!(plain.stdout, debug.stdout);
    assert!(String::from_utf8_lossy(&plain.stdout).contains("headless: frames=30"));
}
