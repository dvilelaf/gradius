// Simultaneous 2P (`Mods::simultaneous_2p`) and the single-ship default.
// The palette assets give every master-palette index its own red channel, so a
// framebuffer scan can tell the two ships' sprite palettes apart.

fn palette_assets() -> Assets {
    Assets {
        // Every tile pixel is colour 2, so a sprite's palette entry 2 is
        // painted and can be scanned for. Only the low tile indices the three
        // ship frames use carry pixels; the stage-1 background uses none of
        // them, so the two behind-priority ships are not hidden by the
        // playfield.
        tiles: (0..8 * 256)
            .map(|entry| {
                let index = (entry % 256) as u8;
                if (1..0x20).contains(&index) {
                    [2u8; 64]
                } else {
                    [0u8; 64]
                }
            })
            .collect(),
        colors: (0..64)
            .map(|index| [index as u8, index as u8, index as u8])
            .collect(),
        rom: Some(crate::rom::testing::rom()),
        ..Default::default()
    }
}

/// A 2P game already in normal play with both ships apart, so a pad cannot be
/// confused for the other.
fn simultaneous_with(p2_palette: u8) -> Game {
    let mut game = Game::new(palette_assets());
    game.set_mods(Mods {
        simultaneous_2p: true,
        p2_sprite_palette: p2_palette,
        ..Mods::default()
    });
    game.menu_selection = 1;
    game.dev_start_run();
    game.phase = Phase::Normal;
    game.scroll_hold = 0;
    game.entry_blank = 0;
    game.player.x = Fixed::px(0x40);
    game.player.y = Fixed::px(0x60);
    game.p2.player.x = Fixed::px(0x80);
    game.p2.player.y = Fixed::px(0x60);
    game
}

fn simultaneous() -> Game {
    simultaneous_with(1)
}

#[test]
fn simultaneous_2p_defaults_off_with_palette_one() {
    let game = playing();
    assert!(!game.mods().simultaneous_2p);
    assert_eq!(game.mods().p2_sprite_palette, 1);
    assert!(!game.simultaneous());
    assert_eq!(game.second_ship(), None);
}

#[test]
fn the_mod_alone_does_not_add_a_ship_to_a_one_player_game() {
    let mut game = playing();
    game.set_mods(Mods {
        simultaneous_2p: true,
        ..Mods::default()
    });
    game.phase = Phase::Normal;
    let start = game.player.x.int;
    for _ in 0..8 {
        game.step_players(Buttons { right: true, ..Buttons::NONE }, Buttons::NONE);
    }
    assert!(game.player.x.int > start);
    assert!(!game.simultaneous(), "the locked mode is still 1P");
    assert_eq!(game.second_ship(), None);
}

#[test]
fn an_alternating_two_player_run_keeps_the_single_ship_dump() {
    let mut game = Game::new(assets());
    game.menu_selection = 1;
    game.dev_start_run();
    game.phase = Phase::Normal;
    game.scroll_hold = 0;
    game.entry_blank = 0;
    game.player.x = Fixed::px(0x50);
    game.player.y = Fixed::px(0x60);
    for _ in 0..600 {
        game.step_players(Buttons::NONE, Buttons::NONE);
    }
    assert!(!game.simultaneous());
    assert_eq!(game.second_ship(), None);
    assert_eq!(
        game.dump_state(),
        "frame=600 state=Playing px=80 pf=0 py=96 pyf=0 speed=0 scroll=44:1:0 score=0 lives=3 capsule=0 meter=0 objects=4:133:216:42:0;5:133:194:42:0;6:133:161:92:3;7:133:194:92:3;8:133:227:92:3;9:133:238:42:0"
    );
}

#[test]
fn two_pads_fly_two_ships_with_two_palettes() {
    let mut game = simultaneous();
    assert!(game.simultaneous());
    let p1_start = (game.player.x.int, game.player.y.int);
    let p2_start = (game.p2.player.x.int, game.p2.player.y.int);
    let p1_pad = Buttons {
        right: true,
        ..Buttons::NONE
    };
    let p2_pad = Buttons {
        up: true,
        ..Buttons::NONE
    };
    for _ in 0..12 {
        game.step_players(p1_pad, p2_pad);
    }
    assert!(game.player.x.int > p1_start.0, "P1 follows its own pad");
    assert_eq!(game.player.y.int, p1_start.1);
    assert!(game.p2.player.y.int < p2_start.1, "P2 follows its own pad");
    assert_eq!(game.p2.player.x.int, p2_start.0);

    let pixels = game.framebuffer().as_rgba();
    let drawn_near = |x: u8, y: u8, colour: u8| {
        let (left, top) = (i32::from(x) - 14, i32::from(y) - 10);
        (top..top + 20).any(|row| {
            (left..left + 24).any(|column| {
                let offset = (row as usize * 256 + column as usize) * 4;
                pixels[offset] == colour
            })
        })
    };
    let p1 = (game.player.x.int, game.player.y.int);
    let p2 = (game.p2.player.x.int, game.p2.player.y.int);
    assert!(drawn_near(p1.0, p1.1, crate::palettes::player(&crate::rom::testing::rom().tables)[2]), "P1 hull");
    assert!(
        drawn_near(p2.0, p2.1, crate::palettes::enemy(&crate::rom::testing::rom().tables)[2]),
        "P2 hull uses p2_sprite_palette"
    );

    // The palette field really selects P2's hull palette.
    let mut third = simultaneous_with(2);
    for _ in 0..12 {
        third.step_players(p1_pad, p2_pad);
    }
    let pixels = third.framebuffer().as_rgba();
    let drawn_near = |x: u8, y: u8, colour: u8| {
        let (left, top) = (i32::from(x) - 14, i32::from(y) - 10);
        (top..top + 20).any(|row| {
            (left..left + 24).any(|column| {
                let offset = (row as usize * 256 + column as usize) * 4;
                pixels[offset] == colour
            })
        })
    };
    let p2 = (third.p2.player.x.int, third.p2.player.y.int);
    assert!(drawn_near(p2.0, p2.1, crate::palettes::shot(&crate::rom::testing::rom().tables)[2]));
}

#[test]
fn one_ship_dying_leaves_the_other_playing() {
    let mut game = simultaneous();
    game.two.slots[1].lives = 0;
    game.with_ship(1, |game| game.hull_hit());
    assert!(game.p2.player.dying);
    assert_eq!(game.two.slots[1].lives, 0xFF);
    assert!(!game.two.slots[1].alive);
    assert_eq!(game.game_over_timer, 0, "P1 still has ships");
    assert_eq!(game.phase, Phase::Normal, "the shared stage is not rebuilt");

    let start = game.player.x.int;
    for _ in 0..12 {
        game.step_players(
            Buttons {
                right: true,
                ..Buttons::NONE
            },
            Buttons::NONE,
        );
    }
    assert!(game.player.x.int > start, "P1 keeps flying");
    assert!(game.p2.player.dying, "P2 is out for good: no lives left");
}

#[test]
fn a_ship_with_lives_left_respawns_after_its_explosion() {
    let mut game = simultaneous();
    game.with_ship(1, |game| game.hull_hit());
    assert_eq!(game.two.slots[1].lives, 2);
    assert!(game.p2.player.dying);
    for _ in 0..121 {
        game.step_players(Buttons::NONE, Buttons::NONE);
    }
    assert!(!game.p2.player.dying, "the second ship is back");
    assert_eq!(game.two.slots[1].lives, 2);
}

#[test]
fn the_last_ship_out_starts_the_shared_game_over() {
    let mut game = simultaneous();
    game.two.slots[0].lives = 0;
    game.two.slots[1].lives = 0;
    game.with_ship(0, |game| game.hull_hit());
    assert_eq!(game.game_over_timer, 0, "P2 is still alive");
    game.with_ship(1, |game| game.hull_hit());
    assert!(game.game_over_timer > 0, "both ships out");
    assert!(game.two.is_game_over());
}

#[test]
fn a_simultaneous_snapshot_restores_both_ships() {
    let mut game = simultaneous();
    let p1_pad = Buttons {
        right: true,
        fire: true,
        ..Buttons::NONE
    };
    let p2_pad = Buttons {
        up: true,
        fire: true,
        ..Buttons::NONE
    };
    for _ in 0..20 {
        game.step_players(p1_pad, p2_pad);
    }
    let image = game.snapshot();
    let state = crate::snapshot::GameState::from_bytes(&image).unwrap();
    assert!(!state.simul.is_empty(), "the second ship travels in SIMUL");

    let mut restored = Game::new(palette_assets());
    restored.restore(&image).unwrap();
    restored.set_mods(game.mods());
    assert_eq!(restored.second_ship(), game.second_ship());
    assert_eq!(restored.dump_state(), game.dump_state());
    for _ in 0..40 {
        game.step_players(p1_pad, p2_pad);
        restored.step_players(p1_pad, p2_pad);
        assert_eq!(restored.dump_state(), game.dump_state());
    }
}

#[test]
fn the_second_player_hud_row_appears_only_in_simultaneous_mode() {
    // The free band row is black fill (`$0F`) except for P2's text (`$30`).
    let band_written = |game: &Game| {
        let pixels = game.framebuffer().as_rgba();
        (8..56).any(|x| pixels[(228 * 256 + x) * 4] == crate::palettes::hud(&crate::rom::testing::rom().tables))
    };
    let mut both = simultaneous();
    both.step_players(Buttons::NONE, Buttons::NONE);
    assert!(band_written(&both), "P2's lives and score are on the band");

    let mut one = Game::new(palette_assets());
    one.dev_start_run();
    one.phase = Phase::Normal;
    one.scroll_hold = 0;
    one.entry_blank = 0;
    one.step(Buttons::NONE);
    assert!(!band_written(&one), "the ROM's HUD is unchanged");
}

#[test]
fn a_second_ship_kill_scores_for_player_two_only() {
    let mut game = simultaneous();
    game.p2.shots[0] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x64),
        y: Fixed::px(0x60),
        age: 0,
    };
    game.objects[5] = object(0x85, 0x64, 0x66);
    game.with_ship(1, |game| game.collide_shots());
    assert_eq!(game.two.slots[1].score, 10, "P2's kill is P2's score");
    assert_eq!(game.score, 0, "P1's bank is untouched");
    assert_eq!(game.two.slots[0].score, 0);
    // The object turned into debris, so the shared world saw the kill.
    assert_eq!(game.objects[5].kind, 2);
}

#[test]
fn an_alternating_image_still_has_no_second_ship_chunk() {
    let mut game = Game::new(assets());
    game.menu_selection = 1;
    game.dev_start_run();
    game.phase = Phase::Normal;
    for _ in 0..8 {
        game.step_players(Buttons::NONE, Buttons::NONE);
    }
    let image = game.snapshot();
    let state = crate::snapshot::GameState::from_bytes(&image).unwrap();
    assert!(state.simul.is_empty());
    let mut restored = Game::new(assets());
    restored.restore(&image).unwrap();
    assert_eq!(restored.dump_state(), game.dump_state());
}



