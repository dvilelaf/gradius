//! SPEED maximum: repeated purchases up to the `$40` wrap.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge. The probe starts a 1P
//! run, then holds B and forces `$42 = 1` every frame, so each frame buys one
//! SPEED. The ROM's `$89A6` is `INC $40` (wraps at 256); the port's
//! `PowerMeter::activate` saturates. Both movement deltas clamp at level 14.
//!
//! The 256 wrap is the only analysis difference: it is asserted here as the
//! documented divergence, so the parity check stops at buy 255.

use std::path::PathBuf;

use gradius_cart::Cartridge;
use gradius_core::PowerMeter;
use gradius_diff::dump::keep_alive_poke;
use gradius_diff::{nes_button_bits, ButtonSet};
use gradius_nes::Emulator;

const BUYS: u16 = 260;
/// Frames of title/countdown before the forced purchases start.
const INTRO: u64 = 400;

fn rom() -> Option<PathBuf> {
    std::env::var_os("GRADIUS_ROM")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

/// `$A000`: `min($40 + 2, 16) << 7` in 8.8 fixed point.
fn speed_delta(level: u8) -> i32 {
    i32::from(level.saturating_add(2).min(16)) << 7
}

#[test]
fn speed_level_clamps_at_fourteen_and_wraps_at_256_on_the_rom() {
    let Some(rom) = rom() else {
        eprintln!("GRADIUS_ROM not set; skipping SPEED max probe");
        return;
    };
    let data = std::fs::read(&rom).expect("read GRADIUS_ROM");
    let cart = Cartridge::from_ines(&data).expect("parse cartridge");
    let mut emulator = Emulator::new(&cart).expect("start oracle");
    emulator.set_audio_enabled(false);

    // Start a 1P run. B is idle, so no capsule activates during the intro.
    for frame in 0..INTRO {
        let mut buttons = ButtonSet::NONE;
        buttons.start = (180..186).contains(&frame);
        emulator.set_buttons(nes_button_bits(buttons));
        emulator.run_frame();
        keep_alive_poke(&mut emulator);
    }
    assert_eq!(
        emulator.ram_snapshot()[0x00],
        5,
        "the intro did not reach the stage handler"
    );
    assert_eq!(
        emulator.ram_snapshot()[0x42],
        0,
        "meter charged before the probe"
    );

    // The controller is latched for the next frame: hold B for a few frames
    // with an empty meter so the first forced purchase is seen immediately.
    for _ in 0..3 {
        let mut buttons = ButtonSet::NONE;
        buttons.b = true;
        emulator.set_buttons(nes_button_bits(buttons));
        emulator.run_frame();
        keep_alive_poke(&mut emulator);
    }
    assert_eq!(
        emulator.ram_snapshot()[0x40],
        0,
        "the warm-up bought a SPEED"
    );

    let mut port = PowerMeter::default();
    let mut oracle_levels = Vec::with_capacity(usize::from(BUYS));
    let mut port_levels = Vec::with_capacity(usize::from(BUYS));
    let mut port_deltas = Vec::with_capacity(usize::from(BUYS));
    for _ in 0..BUYS {
        let mut buttons = ButtonSet::NONE;
        buttons.b = true;
        emulator.bus.ram[0x42] = 1;
        emulator.set_buttons(nes_button_bits(buttons));
        emulator.run_frame();
        keep_alive_poke(&mut emulator);
        oracle_levels.push(emulator.ram_snapshot()[0x40]);

        port.select(1);
        assert_eq!(
            port.activate(false),
            Some(gradius_core::PowerUp::SpeedUp),
            "the port refused a forced SPEED buy"
        );
        port_levels.push(port.speed_level());
        port_deltas.push(port.speed_delta());
    }

    // Buys 1..=255 are identical: the counter climbs and the delta clamps.
    for level in 1..=255u16 {
        let index = usize::from(level) - 1;
        assert_eq!(
            oracle_levels[index], level as u8,
            "oracle $40 after {level} buys"
        );
        assert_eq!(
            port_levels[index], level as u8,
            "port level after {level} buys"
        );
        assert_eq!(
            port_deltas[index],
            speed_delta(oracle_levels[index]),
            "movement delta at level {level}"
        );
    }
    assert_eq!(speed_delta(14), 2048, "level 14 is the 8.0 px/frame clamp");
    assert_eq!(
        port_deltas[255], 2048,
        "the port stays clamped at level 255"
    );

    // The documented difference: `INC $40` wraps, `saturating_add` does not.
    assert_eq!(
        oracle_levels[255], 0,
        "the ROM's $40 should wrap 255 -> 0 after the 256th buy"
    );
    assert_eq!(
        port_levels[255], 255,
        "the port saturates instead of wrapping"
    );
    eprintln!(
        "SPEED max: level 255 -> delta 2048; ROM wraps to {} after {BUYS} buys, port saturates at {}",
        oracle_levels[BUYS as usize - 1],
        port.speed_level()
    );
}
