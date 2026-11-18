//! Runs the original cartridge (when `GRADIUS_ROM` points at it) and reports
//! what the APU/PPU capture APIs see during the title screen and stage 1.
//!
//! The test is skipped when the environment variable is absent so CI never
//! needs the ROM. No ROM data is written anywhere.

use std::path::Path;

use gradius_cart::Cartridge;
use gradius_nes::{ApuWrite, Emulator, VramWrite, BUTTON_B, BUTTON_START};

fn load() -> Option<Emulator> {
    let path = std::env::var_os("GRADIUS_ROM")?;
    let data = std::fs::read(Path::new(&path)).expect("failed to read GRADIUS_ROM");
    let cart = Cartridge::from_ines(&data).expect("failed to parse cartridge");
    let mut emulator = Emulator::new(&cart).expect("unsupported cartridge");
    emulator.set_capture_apu(true);
    emulator.set_capture_vram(true);
    Some(emulator)
}

fn buttons(frame: u64) -> u8 {
    let mut value = 0;
    for start in [180u64, 240] {
        if frame >= start && frame < start + 6 {
            value |= BUTTON_START;
        }
    }
    if frame >= 240 {
        value |= BUTTON_B; // fire held through stage 1
    }
    value
}

#[derive(Default)]
struct FrameStats {
    apu_writes: u64,
    vram_writes: u64,
    nametable_writes: u64,
    palette_writes: u64,
    audio_samples: u64,
    audio_energy: f64,
    audio_peak: f32,
}

#[test]
fn rom_apu_and_vram_capture() {
    let Some(mut emulator) = load() else {
        eprintln!("GRADIUS_ROM not set; skipping cartridge probe");
        return;
    };

    let mut stats = FrameStats::default();
    let mut apu_per_frame = vec![0u64; 901];
    let mut vram_per_frame = vec![0u64; 901];
    let mut addr_histogram = [0u64; 0x18];
    let mut first_writes: Vec<ApuWrite> = Vec::new();

    for frame in 0..=900u64 {
        emulator.set_buttons(buttons(frame));
        emulator.run_frame();
        emulator.frames = frame + 1;

        let writes = emulator.apu_writes();
        apu_per_frame[frame as usize] = writes.len() as u64;
        for write in writes {
            addr_histogram[usize::from(write.addr - 0x4000)] += 1;
        }
        if frame == 120 && first_writes.is_empty() {
            first_writes = writes.iter().copied().take(12).collect();
        }
        stats.apu_writes += writes.len() as u64;
        emulator.bus.apu.clear_writes();

        let vram: &[VramWrite] = emulator.vram_writes();
        vram_per_frame[frame as usize] = vram.len() as u64;
        stats.vram_writes += vram.len() as u64;
        for write in vram {
            if write.addr >= 0x3F00 {
                stats.palette_writes += 1;
            } else if (0x2000..0x3000).contains(&write.addr) {
                stats.nametable_writes += 1;
            }
        }
        emulator.bus.ppu.clear_vram_writes();

        let samples = emulator.drain_audio();
        stats.audio_samples += samples.len() as u64;
        for sample in &samples {
            stats.audio_energy += f64::from(*sample) * f64::from(*sample);
            stats.audio_peak = stats.audio_peak.max(sample.abs());
        }
    }

    let sum = |from: usize, to: usize| -> u64 { apu_per_frame[from..to].iter().sum() };
    let active = |from: usize, to: usize| -> usize {
        apu_per_frame[from..to]
            .iter()
            .filter(|count| **count > 0)
            .count()
    };
    let title = sum(0, 120);
    let gameplay = sum(600, 900);
    let active_after_start = active(250, 900);
    let vram_gameplay: u64 = vram_per_frame[600..800].iter().sum();

    println!("frames run:            {}", emulator.frames);
    println!("cpu cycles:            {}", emulator.cpu.cycles);
    println!(
        "jammed / unknown:      {} / {:?}",
        emulator.cpu.jam_count, emulator.cpu.unknown_opcode
    );
    println!("apu writes total:      {}", stats.apu_writes);
    println!("apu writes 0..120:     {title}");
    println!("apu writes frame 119:  {}", apu_per_frame[119]);
    println!("apu writes frame 799:  {}", apu_per_frame[799]);
    println!("apu writes 600..900:   {gameplay}");
    println!("frames 250..900 with writes: {active_after_start} / 650");
    println!(
        "largest gap between musical frames: {}",
        largest_gap(&apu_per_frame[250..900])
    );
    println!(
        "vram writes total:     {} (nametable {}, palette {})",
        stats.vram_writes, stats.nametable_writes, stats.palette_writes
    );
    println!("vram writes 600..800:  {vram_gameplay}");
    println!(
        "audio samples:         {} (peak {:.3}, rms {:.3})",
        stats.audio_samples,
        stats.audio_peak,
        (stats.audio_energy / stats.audio_samples.max(1) as f64).sqrt()
    );
    println!("apu writes per 100 frames:");
    for chunk in 0..9 {
        let from = chunk * 100;
        let sum: u64 = apu_per_frame[from..from + 100].iter().sum();
        println!("  {from:>3}..{}: {sum}", from + 100);
    }
    println!("apu register histogram:");
    for (offset, count) in addr_histogram.iter().enumerate() {
        if *count > 0 {
            println!("  ${:04X}: {count}", 0x4000 + offset);
        }
    }
    println!(
        "writes around start (240..260): {:?}",
        &apu_per_frame[240..260]
    );
    println!("first apu writes at frame 120:");
    for write in &first_writes {
        println!(
            "  frame {} cycle {:>8} ${:04X} = ${:02X}",
            write.frame, write.cycle, write.addr, write.value
        );
    }

    assert!(title > 0, "title screen should program the APU");
    assert!(gameplay > 0, "gameplay should keep writing the APU");
    assert!(
        active_after_start > 500,
        "music should write APU registers on nearly every frame, got {active_after_start}/650"
    );
    assert!(vram_gameplay > 0, "stage scrolling should write VRAM");
    assert!(
        stats.audio_samples > 0,
        "audio capture should produce samples"
    );
    assert!(stats.audio_peak > 0.01, "audio should be non-silent");
    assert!(stats.audio_peak <= 1.0, "audio must stay bounded");
}

fn largest_gap(counts: &[u64]) -> usize {
    let mut best = 0;
    let mut current = 0;
    for count in counts {
        if *count == 0 {
            current += 1;
            best = best.max(current);
        } else {
            current = 0;
        }
    }
    best
}
