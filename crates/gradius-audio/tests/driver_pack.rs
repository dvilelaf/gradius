//! Driver-from-pack checks.
//!
//! The non-ROM tests build a synthetic driver section, so they prove the pack
//! path works without the cartridge. The oracle test at the bottom is skipped
//! unless `GRADIUS_ROM` points at Gradius (E); it extracts the real driver,
//! puts it in a pack chunk, and compares the pack driver with the cartridge
//! for every supported request. The captured timelines it checks against come
//! from the extracted pack.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gradius_audio::chunk::{encode_chunk, encode_driver, AudioBank, FRAME_CYCLES};
use gradius_audio::driver::{
    extract_source, Driver, DriverSource, RegWrite, DRIVER_BASE, MUSIC_REQUESTS, PERIOD_TABLE,
    PERIOD_TABLE_LEN, REQUEST_TABLE, REQUEST_TABLE_LEN, SFX_REQUESTS,
};
use gradius_audio::{Music, Sfx, Timeline};

mod common;

fn synthetic_source() -> DriverSource {
    let mut request = vec![0u8; REQUEST_TABLE_LEN];
    // Request $10: one part on pulse 1 at $C100.
    let at = 0x10 * 3;
    request[at] = 0;
    request[at + 1..at + 3].copy_from_slice(&0xC100u16.to_le_bytes());
    // Invented period entries in descending order, the shape the driver reads.
    let period = [
        0x07, 0x10, 0x06, 0xC0, 0x06, 0x74, 0x06, 0x2C, 0x05, 0xE8, 0x05, 0xA8, 0x05, 0x6C, 0x05,
        0x34, 0x04, 0xFE, 0x04, 0xCC, 0x04, 0x9C, 0x04, 0x70,
    ];
    DriverSource {
        base: DRIVER_BASE,
        request_addr: REQUEST_TABLE,
        request,
        period_addr: PERIOD_TABLE,
        period: period.to_vec(),
        channels: [
            (0x00B0, 0x00),
            (0x00C1, 0x04),
            (0x00D2, 0x08),
            (0x00E3, 0x0C),
        ],
        segments: vec![(0xC100, vec![0xD1, 0x7F, 0x00, 0x01, 0xFF])],
    }
}

fn run(driver: &mut Driver<'_>, id: u8, frames: usize) -> Vec<Vec<(u16, u8)>> {
    let _ = driver.start_id(id);
    (0..frames)
        .map(|_| {
            let mut writes: Vec<RegWrite> = Vec::new();
            driver.tick(&mut writes);
            writes.into_iter().map(|w| (w.addr, w.value)).collect()
        })
        .collect()
}

#[test]
fn a_pack_driver_runs_from_pack_memory_alone() {
    let source = synthetic_source();
    let chunk = encode_chunk(&[], &[], &encode_driver(&source));
    let bank = AudioBank::from_chunk(&chunk).expect("chunk decodes");
    assert!(bank.driver_bank().is_some());

    let mut pack_driver = bank.driver().expect("pack driver");
    let pack = run(&mut pack_driver, 0x10, 4);

    // The same source laid out directly, with no chunk.
    let mut image = vec![0u8; 0x4000];
    let request_at = usize::from(REQUEST_TABLE - DRIVER_BASE);
    image[request_at..request_at + REQUEST_TABLE_LEN].copy_from_slice(&source.request);
    let period_at = usize::from(PERIOD_TABLE - DRIVER_BASE);
    image[period_at..period_at + PERIOD_TABLE_LEN].copy_from_slice(&source.period);
    image[0x100..0x105].copy_from_slice(&[0xD1, 0x7F, 0x00, 0x01, 0xFF]);
    let mut direct = Driver::new(DRIVER_BASE, &image);
    let expected = run(&mut direct, 0x10, 4);

    assert_eq!(pack, expected);
    assert_eq!(
        pack[0],
        vec![(0x4000, 0x7F), (0x4003, 0x08), (0x4002, 0x71)]
    );
}

#[test]
fn old_and_empty_chunks_have_no_driver() {
    let chunk = encode_chunk(&[], &[], &[]);
    let bank = AudioBank::from_chunk(&chunk).expect("timeline chunk");
    assert!(bank.driver().is_none());
    assert!(bank.music(Music::Stage1).is_none());
    let mut old = chunk;
    old[4..6].copy_from_slice(&1u16.to_le_bytes());
    old.truncate(old.len() - 4);
    let bank = AudioBank::from_chunk(&old).expect("version 1 chunk");
    assert!(bank.driver().is_none());
}

/// Grouped capture writes, dropping the APU setup writes and the real-game
/// lead-in in the sliced title capture.
fn captured(timeline: &Timeline) -> BTreeMap<u32, Vec<(u16, u8)>> {
    let mut events: Vec<_> = timeline
        .events
        .iter()
        .filter(|event| !matches!(event.addr, 0x15 | 0x17))
        .collect();
    let mut shift = 0;
    if timeline.name == "intro" {
        if let Some(index) = events
            .iter()
            .position(|event| event.value != 0x30 && event.value != 0x00)
        {
            shift = events[index].at;
            events.drain(..index);
        }
    }
    let mut grouped = BTreeMap::new();
    for event in events {
        grouped
            .entry((event.at - shift) / FRAME_CYCLES)
            .or_insert_with(Vec::new)
            .push((0x4000 | u16::from(event.addr), event.value));
    }
    grouped
}

fn mismatches(native: &[Vec<(u16, u8)>], timeline: &Timeline) -> (usize, usize) {
    let expected = captured(timeline);
    let last = expected.keys().next_back().copied().unwrap_or(0);
    let frames = (native.len() as u32).max(last.saturating_add(1));
    let mut bad = 0;
    let mut total = 0;
    for frame in 0..frames {
        let want = expected.get(&frame).cloned().unwrap_or_default();
        let got = native.get(frame as usize).cloned().unwrap_or_default();
        total += want.len();
        bad += (0..want.len().max(got.len()))
            .filter(|&index| want.get(index) != got.get(index))
            .count();
    }
    (bad, total)
}

fn rom_bank() -> Option<Vec<u8>> {
    let path = std::env::var_os("GRADIUS_ROM").map(PathBuf::from)?;
    let data = std::fs::read(&path).expect("read GRADIUS_ROM");
    let cart = gradius_cart::Cartridge::from_ines(&data).expect("parse cartridge");
    let start = usize::from(DRIVER_BASE - 0x8000);
    Some(cart.prg.get(start..start + 0x4000)?.to_vec())
}

/// Stage 1-B without a cartridge: the pack carries both the driver section
/// and the captured timeline, which is the oracle.
#[test]
fn pack_driver_reproduces_the_stage1b_capture() {
    let timeline = common::music(Music::Stage1B);
    let frames = usize::try_from(timeline.total_cycles / FRAME_CYCLES).unwrap();
    let mut driver = common::bank().driver().expect("pack driver");
    let writes = run(&mut driver, 0x96, frames);
    let (bad, total) = mismatches(&writes, timeline);
    assert_eq!(bad, 0, "stage 1-B: {bad}/{total} capture writes differ");
}

#[test]
fn driver_from_pack_equals_driver_from_rom_and_captures() {
    let Some(rom) = rom_bank() else {
        eprintln!("GRADIUS_ROM not set; skipping cartridge driver comparison");
        return;
    };
    let source = extract_source(DRIVER_BASE, &rom);
    let chunk = encode_chunk(&[], &[], &encode_driver(&source));
    let bank = AudioBank::from_chunk(&chunk).expect("driver chunk decodes");

    let mut expected_bad = Vec::new();
    for (id, music) in MUSIC_REQUESTS.iter().zip(Music::ALL) {
        let timeline = common::music(music);
        let frames = usize::try_from(timeline.total_cycles / FRAME_CYCLES).unwrap();
        let mut pack = bank.driver().expect("pack driver");
        let mut original = Driver::new(DRIVER_BASE, &rom);
        let pack_writes = run(&mut pack, *id, frames);
        let rom_writes = run(&mut original, *id, frames);
        assert_eq!(pack_writes, rom_writes, "pack vs ROM for music ${id:02X}");
        let (bad, total) = mismatches(&pack_writes, timeline);
        println!("${id:02X} {music:?}: {bad}/{total} capture writes differ");
        expected_bad.push((*id, bad, total));
    }
    for (id, sfx) in SFX_REQUESTS.iter().zip(Sfx::ALL) {
        let timeline = common::sfx(sfx);
        let frames = usize::try_from(timeline.total_cycles / FRAME_CYCLES).unwrap();
        let mut pack = bank.driver().expect("pack driver");
        let mut original = Driver::new(DRIVER_BASE, &rom);
        let pack_writes = run(&mut pack, *id, frames);
        let rom_writes = run(&mut original, *id, frames);
        assert_eq!(pack_writes, rom_writes, "pack vs ROM for sfx ${id:02X}");
        let (bad, total) = mismatches(&pack_writes, timeline);
        println!("${id:02X} {sfx:?}: {bad}/{total} capture writes differ");
        expected_bad.push((*id, bad, total));
    }

    // Every music and effect request must reproduce its capture exactly.
    for (id, bad, _) in expected_bad {
        assert_eq!(bad, 0, "unexpected capture mismatch for ${id:02X}");
    }
}
