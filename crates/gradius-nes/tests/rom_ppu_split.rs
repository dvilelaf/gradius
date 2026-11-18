//! Runs the original cartridge (when `GRADIUS_ROM` points at it) through the
//! stage-1 HUD sprite-0 split without the `--assist` workaround.
//!
//! The split routine at `$9A96`/`$9A9D` waits for the power-meter bar (sprite
//! 0, OAM `CE 6D 23 F8`) to overlap an opaque background pixel on line 207.
//! `$9AA4` is the instruction immediately after the wait, so its execution
//! count is direct evidence that the wait was left instead of spinning.
//!
//! The test is skipped when the environment variable is absent so CI never
//! needs the ROM. No ROM data is written anywhere.

use std::path::Path;

use gradius_cart::Cartridge;
use gradius_nes::{Emulator, ObjectState, PlayerState, BUTTON_A, BUTTON_RIGHT, BUTTON_START};

fn load() -> Option<Emulator> {
    let path = std::env::var_os("GRADIUS_ROM")?;
    let data = std::fs::read(Path::new(&path)).expect("failed to read GRADIUS_ROM");
    let cart = Cartridge::from_ines(&data).expect("failed to parse cartridge");
    Some(Emulator::new(&cart).expect("unsupported cartridge"))
}

/// Start at frames 180 and 240, fire from frame 300 and steer right briefly.
fn buttons(frame: u64) -> u8 {
    let mut value = 0;
    for start in [180u64, 240] {
        if frame >= start && frame < start + 6 {
            value |= BUTTON_START;
        }
    }
    if frame >= 300 {
        value |= BUTTON_A;
    }
    if (300..520).contains(&frame) {
        value |= BUTTON_RIGHT;
    }
    value
}

fn score(player: &PlayerState) -> u32 {
    player.score()
}

#[test]
fn hud_split_passes_without_assist_and_the_game_keeps_running() {
    let Some(mut emulator) = load() else {
        eprintln!("GRADIUS_ROM not set; skipping cartridge probe");
        return;
    };

    let mut at_300: Option<PlayerState> = None;
    let mut at_520: Option<PlayerState> = None;
    let mut at_900: Option<PlayerState> = None;
    let mut at_1500: Option<PlayerState> = None;
    let mut objects_900: Vec<ObjectState> = Vec::new();
    let mut wait_passes_900 = 0u32;

    for frame in 0..3000u64 {
        emulator.set_buttons(buttons(frame));
        emulator.run_frame();

        match frame {
            299 | 519 | 899 | 1499 => {
                let state = emulator.player_state();
                match frame {
                    299 => at_300 = Some(state),
                    519 => at_520 = Some(state),
                    899 => {
                        wait_passes_900 = emulator.bus.trace.hits[0x9AA4];
                        objects_900 = emulator.objects();
                        at_900 = Some(state);
                    }
                    _ => at_1500 = Some(state),
                }
            }
            _ => {}
        }
    }

    let start = at_300.expect("frame 300 state");
    let after_steer = at_520.expect("frame 520 state");
    let at_900 = at_900.expect("frame 900 state");
    let at_1500 = at_1500.expect("frame 1500 state");
    let final_state = emulator.player_state();
    let hits = &emulator.bus.trace.hits;

    println!("player at 300:   {start:?}");
    println!("player at 520:   {after_steer:?}");
    println!("player at 900:   {at_900:?}");
    println!("player at 1500:  {at_1500:?}");
    println!("player at 3000:  {final_state:?}");
    println!(
        "wait loop:       entered {} times, left {} times by frame 900, {} times overall",
        hits[0x9A9D], wait_passes_900, hits[0x9AA4]
    );
    println!(
        "sprite poll:     $9A96 {} hits, $9A9D {} hits, $9AA4 {} hits",
        hits[0x9A96], hits[0x9A9D], hits[0x9AA4]
    );
    println!(
        "jam:             {:?} (count {}), unknown opcode {:?}",
        emulator.cpu.jammed, emulator.cpu.jam_count, emulator.cpu.unknown_opcode
    );
    println!("objects at 900:  {} live slots", objects_900.len());

    // The HUD split loop must be entered and, crucially, left: `$9AA4` only
    // executes after the sprite-0 wait succeeds.
    assert!(
        wait_passes_900 > 400,
        "the HUD split should pass on virtually every live frame, got {wait_passes_900} exits by frame 900"
    );
    assert!(
        hits[0x9AA4] > 1500,
        "the split should keep passing through frame 3000, got {} exits",
        hits[0x9AA4]
    );
    assert!(!emulator.cpu.jammed, "the CPU must not jam");
    assert_eq!(emulator.cpu.jam_count, 0, "no JAM instructions");
    assert_eq!(emulator.cpu.unknown_opcode, None, "no unknown opcodes");

    // The stage must keep scrolling at 0.5 px/frame instead of freezing on
    // the first live frame.
    assert!(
        start.scroll < at_900.scroll && at_900.scroll < final_state.scroll,
        "scroll should advance across frames: {} -> {} -> {}",
        start.scroll,
        at_900.scroll,
        final_state.scroll
    );
    assert!(
        final_state.scroll > 0x0300,
        "almost 3000 frames of stage 1 should scroll past 0x0300, got ${:04X}",
        final_state.scroll
    );

    // Holding right steers the ship.
    assert!(
        after_steer.x_int >= start.x_int + 50,
        "the player should move right: x {} -> {}",
        start.x_int,
        after_steer.x_int
    );

    // Firing starts scoring once enemies are destroyed.
    assert_eq!(score(&start), 0, "no score before the first shots land");
    assert!(
        score(&at_1500) > 0 && score(&final_state) > 0,
        "firing should score: 1500 = {}, 3000 = {}",
        score(&at_1500),
        score(&final_state)
    );
    assert_ne!(final_state.score_bcd, [0; 3]);

    // State APIs see the live object table.
    //
    // `Emulator::objects` numbers the game's object slots: their parallel
    // arrays start one 12-byte player block past the `$0120`/`$0320`/`$0360`
    // bases (`$010C`/`$030C`/`$032C`/`$036C`, what `$A521` clears), so the
    // ship is that first block and not a slot - `player_state()` above is its
    // view. Slot 0 is therefore an ordinary enemy slot; this assertion asked
    // for the ship in it, which `objects()` cannot ever report.
    //
    // At frame 900 the four pods of the first wave are the live slots, kind
    // `$85` on line 160. Slots above 9 are the overlapping-array read that
    // `gradius-diff` documents (`$A3AB` only allocates 9 down to 0), so they
    // are ignored here.
    let live: Vec<(u8, u8, u8)> = objects_900
        .iter()
        .filter(|object| object.slot < 10)
        .map(|object| (object.slot, object.kind, object.y_int))
        .collect();
    assert_eq!(
        live,
        [
            (5, 0x85, 160),
            (7, 0x85, 160),
            (8, 0x85, 160),
            (9, 0x85, 160)
        ],
        "the object table should hold the first wave's pods: {objects_900:?}"
    );
    assert_eq!(start.lives, 3, "a new game starts with three lives");
    assert!(
        (16..=240).contains(&start.x_int) && (16..=192).contains(&start.y_int),
        "ship should be inside the playfield: {start:?}"
    );
    assert_eq!(emulator.frames, 3000);
}
