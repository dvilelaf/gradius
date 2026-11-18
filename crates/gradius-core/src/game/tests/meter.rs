// Power-meter boxes that can no longer be bought are drawn empty. The ROM's
// redraw (`$89E8`) runs every eighth NMI frame, so the HUD follows the state
// with a delay of up to eight frames. See `docs/analysis/powerups.md`.

/// Step `frames` idle frames.
fn idle(game: &mut Game, frames: u32) {
    for _ in 0..frames {
        game.step(Buttons::NONE);
    }
}

/// Buy the upgrade in meter slot `selection` (1..=6).
fn buy(game: &mut Game, selection: u8) {
    game.power.select(selection);
    let extra = game.mods.extra_options;
    game.power.activate(extra);
}

#[test]
fn an_owned_upgrade_empties_its_meter_box() {
    // Slot indices: speed 0, missile 1, double 2, laser 3, option 4, shield 5.
    for (selection, slot) in [(2, 1), (3, 2), (4, 3), (6, 5)] {
        let mut game = sandbox();
        buy(&mut game, selection);
        idle(&mut game, 8);
        let mut expected = [false; 6];
        expected[slot] = true;
        assert_eq!(game.power.shown_empty(), expected, "selection {selection}");
    }
}

#[test]
fn the_box_empties_on_the_next_redraw_not_at_once() {
    let mut game = sandbox();
    idle(&mut game, 8);
    buy(&mut game, 2);
    // The ROM redraws on one frame in eight, so within eight frames the box
    // changes exactly once and stays changed.
    let mut changes = 0;
    let mut last = game.power.shown_empty();
    for _ in 0..16 {
        idle(&mut game, 1);
        if game.power.shown_empty() != last {
            changes += 1;
            last = game.power.shown_empty();
        }
    }
    assert_eq!(changes, 1);
    assert!(last[1]);
}

#[test]
fn speed_up_never_empties() {
    let mut game = sandbox();
    for _ in 0..20 {
        buy(&mut game, 1);
    }
    idle(&mut game, 8);
    assert!(!game.power.shown_empty()[0]);
}

#[test]
fn the_option_box_empties_at_the_cap_with_and_without_the_mod() {
    let mut parity = sandbox();
    collect_options(&mut parity);
    idle(&mut parity, 8);
    assert_eq!(parity.power.options(), 2);
    assert!(parity.power.shown_empty()[4], "two options fill the ROM cap");

    let mut port = sandbox();
    port.set_mods(Mods {
        extra_options: true,
        ..Mods::default()
    });
    buy(&mut port, 5);
    buy(&mut port, 5);
    idle(&mut port, 8);
    assert_eq!(port.power.options(), 2);
    assert!(
        !port.power.shown_empty()[4],
        "two options are not the cap with four-option mod"
    );
    collect_options(&mut port);
    idle(&mut port, 8);
    assert_eq!(port.power.options(), 4);
    assert!(port.power.shown_empty()[4]);
}

#[test]
fn the_boxes_refill_after_the_ship_dies() {
    let mut game = sandbox();
    buy(&mut game, 2);
    buy(&mut game, 3);
    idle(&mut game, 8);
    assert!(game.power.shown_empty()[1] && game.power.shown_empty()[2]);

    // `$97D0`: death clears the upgrades; the screen only follows at the
    // next redraw.
    game.power.reset();
    idle(&mut game, 8);
    assert_eq!(game.power.shown_empty(), [false; 6]);
}

#[test]
fn death_keeps_the_meter_cursor() {
    // The oracle keeps `$42` through death and respawn (frames 1275-1400).
    let mut game = sandbox();
    game.power.select(3);
    game.power.reset();
    assert_eq!(game.power.selection(), 3);
}

#[test]
fn spending_the_cursor_clears_its_highlight_at_the_next_redraw() {
    let mut game = sandbox();
    game.power.advance();
    game.power.advance();
    idle(&mut game, 2);
    assert_eq!(game.power.shown_selection(), 2, "a pickup lights its box");
    let extra = game.mods.extra_options;
    game.power.activate(extra);
    assert_eq!(game.power.selection(), 0);
    idle(&mut game, 8);
    assert_eq!(game.power.shown_selection(), 0);
}
