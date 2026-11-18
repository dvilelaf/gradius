//! Two-player input through the oracle: the ROM enters 2P mode and the second
//! controller (`$4017`) actually drives the ship after the turn switch.
//!
//! `docs/analysis/twoplayer.md` measures the natural P1 death at frame 1195
//! (Select at 180, Start at 220) and P2's stage rebuild around frame 1230.
//! Frames here are the count of completed `run_frame` calls, which is one
//! lower than the doc's probe output at the death: the handoff is observed
//! after frame 1194 and P2 is in normal play (`$1B = $80`) at 1221.
//!
//! Run with:
//!
//! ```sh
//! GRADIUS_ROM="<path to Gradius (E).nes>" \
//!     cargo test -p gradius-nes --test rom_p2 -- --nocapture
//! ```
//!
//! The test is skipped when the environment variable is absent so CI never
//! needs the ROM.

use std::path::Path;

use gradius_cart::Cartridge;
use gradius_nes::{Emulator, BUTTON_RIGHT, BUTTON_SELECT, BUTTON_START};

fn load() -> Option<Emulator> {
    let path = std::env::var_os("GRADIUS_ROM")?;
    let data = std::fs::read(Path::new(&path)).expect("failed to read GRADIUS_ROM");
    let cart = Cartridge::from_ines(&data).expect("failed to parse cartridge");
    Some(Emulator::new(&cart).expect("unsupported cartridge"))
}

#[test]
fn p2_input_moves_the_ship_after_the_turn_switch() {
    let Some(mut emulator) = load() else {
        eprintln!("GRADIUS_ROM not set; skipping cartridge probe");
        return;
    };

    let mut handoff_frame = None;
    let mut normal_play = None;
    let mut x_at_turn = 0;
    let mut y_at_turn = 0;
    let mut p2_edge = 0;
    let mut p2_held = 0;
    let mut p1_edge = 0;
    let mut x_after = 0;
    let mut y_after = 0;

    for frame in 0..=1261u64 {
        let mut p1 = 0;
        if (180..186).contains(&frame) {
            p1 |= BUTTON_SELECT;
        }
        if (220..226).contains(&frame) {
            p1 |= BUTTON_START;
        }
        // P2 only starts pressing once the rebuild has finished so the input
        // is exercised by the normal-play handler.
        let p2 = match normal_play {
            Some(turn) if frame > turn => BUTTON_RIGHT,
            _ => 0,
        };
        emulator.set_buttons_p1(p1);
        emulator.set_buttons_p2(p2);
        emulator.run_frame();

        let ram = emulator.ram_snapshot();
        if handoff_frame.is_none() && ram[0x18] == 1 {
            handoff_frame = Some(frame);
        }
        if normal_play.is_none() && ram[0x18] == 1 && ram[0x1B] == 0x80 {
            normal_play = Some(frame);
            x_at_turn = ram[0x0360];
            y_at_turn = ram[0x0320];
        }
        if normal_play.map(|turn| turn + 1) == Some(frame) {
            // First frame with P2's buttons held: `$06` is the edge byte.
            p2_edge = ram[0x06];
            p1_edge = ram[0x05];
        }
        if frame == 1260 {
            p2_held = ram[0x08];
            x_after = ram[0x0360];
            y_after = ram[0x0320];
            println!(
                "handoff at {handoff_frame:?}, normal play at {normal_play:?}: \
                 $06={p2_edge:02X} $08={p2_held:02X} x ${x_at_turn:02X} -> ${x_after:02X}, \
                 y ${y_at_turn:02X} -> ${y_after:02X}"
            );
        }
    }

    let handoff = handoff_frame.expect("P1 should die and hand over to P2");
    let normal = normal_play.expect("P2 should reach normal play");
    println!("P1 handoff at frame {handoff}, P2 normal play at {normal}");

    assert!(
        (1190..=1200).contains(&handoff),
        "P1's natural death should hand over around the documented frame 1195, got {handoff}"
    );
    assert!(
        (1215..=1235).contains(&normal),
        "P2 should be in normal play around the documented frame 1230, got {normal}"
    );
    // Game-ordered input bytes: bit 0 is Right, and the edge (`$06`) is set
    // only on the first frame of the press, while the held byte (`$08`)
    // stays set.
    assert_eq!(p2_edge, 0x01, "P2 Right should appear as an edge in $06");
    assert_eq!(p1_edge, 0x00, "P1 is idle, so $05 must stay clear");
    assert_eq!(p2_held, 0x01, "P2 Right should stay held in $08");
    assert!(
        x_after > x_at_turn + 20,
        "P2 Right should move the ship right: x ${x_at_turn:02X} -> ${x_after:02X}"
    );
    assert_eq!(y_after, y_at_turn, "P2 Right must not change Y");
    assert!(!emulator.cpu.jammed, "the CPU must not jam");
}
