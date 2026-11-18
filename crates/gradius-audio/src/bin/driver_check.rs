//! Development check: run the native [`gradius_audio::Driver`] against the
//! cartridge oracle for one music request and one sound effect.
//!
//! Usage:
//!
//! ```text
//! cargo run -p gradius-audio --features capture --bin driver-check -- \
//!     "$GRADIUS_ROM" [music-id] [sfx-id] [frames]
//! ```
//!
//! Nothing here is linked into the runtime game: the sequence bytes and the
//! APU stream are read from the cartridge at run time and never committed.

use gradius_audio::driver::{Driver, Part, RegWrite};
use gradius_cart::Cartridge;
use gradius_nes::Emulator;

const DRIVER: u16 = 0xECFC;
const TRIGGER: u16 = 0xEC18;
const FRAME_CYCLES: u32 = 29_780;
const BANK_BASE: u16 = 0xC000;
const REQUEST_TABLE: u16 = 0xEFC7;

fn main() {
    if let Err(error) = run() {
        eprintln!("driver-check: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let rom = args.next().ok_or("pass the cartridge path")?;
    let music = parse_hex(args.next().unwrap_or_else(|| "90".into()))?;
    let sfx = parse_hex(args.next().unwrap_or_else(|| "06".into()))?;
    let frames: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(120);
    let data = std::fs::read(&rom).map_err(|error| format!("{rom}: {error}"))?;
    let cart = Cartridge::from_ines(&data).map_err(|error| error.to_string())?;
    let bank = &cart.prg[bank_offset()?..];

    check("music", &cart, bank, music, frames)?;
    check("sfx", &cart, bank, sfx, frames)?;
    Ok(())
}

fn parse_hex(value: String) -> Result<u8, String> {
    u8::from_str_radix(value.trim_start_matches('$'), 16).map_err(|error| error.to_string())
}

fn bank_offset() -> Result<usize, String> {
    let offset = usize::from(BANK_BASE) - 0x8000;
    if offset < 0x8000 {
        Ok(offset)
    } else {
        Err("unexpected PRG layout".into())
    }
}

fn request_parts(bank: &[u8], id: u8) -> Vec<Part> {
    let count = usize::from(id >> 6) + 1;
    let mut parts = Vec::with_capacity(count);
    for entry in 0..count {
        let index = usize::from(id & 0x3F) * 3 + entry * 3;
        let at = usize::from(REQUEST_TABLE - BANK_BASE) + index;
        if at + 2 >= bank.len() {
            break;
        }
        let channel = bank[at] >> 2;
        if channel > 3 {
            continue;
        }
        let ptr = u16::from_le_bytes([bank[at + 1], bank[at + 2]]);
        parts.push(Part { channel, ptr });
    }
    parts
}

fn check(label: &str, cart: &Cartridge, bank: &[u8], id: u8, frames: usize) -> Result<(), String> {
    let parts = request_parts(bank, id);
    if parts.is_empty() {
        return Err(format!("${id:02X} has no request parts"));
    }

    let mut emu = quiet_emulator(cart)?;
    call(&mut emu, TRIGGER, id)?;
    let start_writes = writes(&emu);
    emu.bus.apu.clear_writes();

    let mut driver = Driver::new(BANK_BASE, bank);
    let native_start = driver.start(id, &parts);
    report_start(label, id, &start_writes, &native_start);

    let mut oracle_total = 0usize;
    let mut native_total = 0usize;
    let mut bad = 0usize;
    let mut bad_frames = 0usize;
    for frame in 0..frames {
        let begin = emu.cpu.cycles;
        call(&mut emu, DRIVER, 0)?;
        let oracle = writes(&emu);
        emu.bus.apu.clear_writes();
        let spent = (emu.cpu.cycles - begin) as u32;
        if spent < FRAME_CYCLES {
            emu.bus.apu.step(FRAME_CYCLES - spent);
        }
        emu.cpu.cycles = begin + u64::from(FRAME_CYCLES);

        let mut native: Vec<RegWrite> = Vec::new();
        driver.tick(&mut native);

        oracle_total += oracle.len();
        native_total += native.len();
        let mut frame_bad = 0usize;
        for (index, write) in oracle.iter().enumerate() {
            if native.get(index) != Some(write) {
                frame_bad += 1;
            }
        }
        frame_bad += native.len().abs_diff(oracle.len());
        if frame_bad > 0 {
            bad_frames += 1;
            bad += frame_bad;
            if std::env::var_os("DRIVER_CHECK_DIFF").is_some() && bad_frames <= 8 {
                println!("  frame {frame}: oracle {oracle:?}");
                println!("            native {native:?}");
            }
        }
    }
    let rate = if oracle_total == 0 {
        0.0
    } else {
        bad as f64 * 100.0 / oracle_total as f64
    };
    println!(
        "{label} ${id:02X}: {bad}/{oracle_total} writes differ ({rate:.2}%), {bad_frames}/{frames} frames, native {native_total} writes"
    );
    Ok(())
}

fn report_start(label: &str, id: u8, oracle: &[RegWrite], native: &[RegWrite]) {
    let same = oracle.len() == native.len()
        && oracle
            .iter()
            .zip(native)
            .all(|(left, right)| left.addr == right.addr && left.value == right.value);
    println!(
        "{label} ${id:02X} start: oracle {} writes, native {} writes{}",
        oracle.len(),
        native.len(),
        if same { "" } else { " (differ)" }
    );
}

fn writes(emu: &Emulator) -> Vec<RegWrite> {
    emu.apu_writes()
        .iter()
        .filter(|write| (0x4000..=0x4017).contains(&write.addr))
        .map(|write| RegWrite {
            addr: write.addr,
            value: write.value,
        })
        .collect()
}

fn quiet_emulator(cart: &Cartridge) -> Result<Emulator, String> {
    let mut emu = Emulator::new(cart).map_err(|error| error.to_string())?;
    emu.run_frames(4);
    for base in [0xB0usize, 0xC1, 0xD2, 0xE3] {
        for offset in 0..17 {
            emu.bus.ram[base + offset] = 0;
        }
    }
    for addr in [
        0xF0usize, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF8, 0xF9, 0xFA, 0xFB, 0xDD, 0xDE,
    ] {
        emu.bus.ram[addr] = 0;
    }
    emu.bus.ram[0x15] = 0;
    emu.bus.ram[0xC3] = 0x10;
    emu.bus.apu = gradius_nes::Apu::new();
    emu.bus.apu.write(0x4015, 0x00);
    for addr in 0x4000..=0x400F {
        emu.bus.apu.write(addr, 0x30);
    }
    emu.bus.apu.write(0x4015, 0x0F);
    emu.bus.apu.write(0x4017, 0xC0);
    emu.set_capture_apu(true);
    emu.bus.apu.clear_writes();
    Ok(emu)
}

/// Call a routine and stop when it returns to the `$FFF0` sentinel.
fn call(emu: &mut Emulator, addr: u16, a: u8) -> Result<(), String> {
    let push = |emu: &mut Emulator, value: u8| {
        emu.bus.ram[0x100 + usize::from(emu.cpu.sp)] = value;
        emu.cpu.sp = emu.cpu.sp.wrapping_sub(1);
    };
    push(emu, 0xFF);
    push(emu, 0xF0);
    emu.cpu.a = a;
    emu.cpu.pc = addr;
    let mut guard = 0u64;
    loop {
        if emu.cpu.pc == 0xFFF1 {
            return Ok(());
        }
        let before = emu.cpu.cycles;
        emu.bus.apu.cycle = before;
        emu.cpu.step(&mut emu.bus);
        let delta = (emu.cpu.cycles - before) as u32;
        emu.bus.apu.step(delta);
        let stall = emu.bus.take_stall();
        if stall > 0 {
            emu.bus.apu.step(stall);
        }
        emu.cpu.cycles += u64::from(stall);
        guard += 1;
        if guard > 1_000_000 {
            return Err(format!("{addr:04X} did not return (pc={:04X})", emu.cpu.pc));
        }
    }
}
