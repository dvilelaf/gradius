#[test]
fn a_new_game_starts_on_the_title_with_zeroed_state_fields() {
    let game = Game::new(patterned_assets());
    assert_eq!(game.state, State::Title);
    assert_eq!(
        game.dump_state(),
        "frame=0 state=Title px=0 pf=0 py=0 pyf=0 speed=0 scroll=0:0:0 score=0 lives=0 capsule=0 meter=0 objects="
    );
}

#[test]
fn the_no_input_attract_window_matches_the_oracle_frames() {
    let mut game = Game::new(patterned_assets());
    for _ in 0..=132 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.frame, 133);
    assert_eq!(game.state, State::Playing);
    assert!(matches!(game.phase, Phase::Attract(0)));
    let line = game.dump_state();
    assert!(
        line.starts_with("frame=133 state=Other(1) px=80 pf=0 py=134 pyf=0 speed=0 scroll=0:0:0"),
        "{line}"
    );
    // Without Start the ROM is still in the 256-frame transition at 264
    // (`$00 = 1`); the demo starts at 390.
    while game.frame < 264 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.frame, 264);
    assert!(matches!(game.phase, Phase::Attract(0)));
    assert!(game.dump_state().starts_with("frame=264 state=Other(1)"));
}

#[test]
fn a_reentered_title_holds_for_130_frames_not_133() {
    // `$80F0`: `$12 = $FE` two frames after `$00 = 0`, then 127 `DEC`
    // pairs and the zero-test frame. Boot keeps 133 because the reset
    // warm-up delays the setup to frame 5.
    let mut game = sandbox();
    game.title_pending = true;
    game.step(Buttons::NONE);
    assert_eq!(game.state, State::Title);
    let mut frames = 0;
    while game.state == State::Title && frames < 200 {
        game.step(Buttons::NONE);
        frames += 1;
    }
    assert_eq!(frames, 130, "`$8187`/`$9749` re-entry title");
    assert!(matches!(game.phase, Phase::Attract(0)));

    // The demo-end path (`$8129`/`$8187`) takes the same 130 frames.
    game.finish_demo();
    let mut frames = 0;
    while game.state == State::Title && frames < 200 {
        game.step(Buttons::NONE);
        frames += 1;
    }
    assert_eq!(frames, 130, "`$8187` demo re-entry");
}

#[test]
fn the_demo_starts_on_the_measured_frames_and_scrolls_at_half_a_pixel() {
    let mut game = Game::new(patterned_assets());
    while game.frame < 389 {
        game.step(Buttons::NONE);
    }
    assert!(game.dump_state().starts_with("frame=389 state=Other(1)"));
    game.step(Buttons::NONE);
    assert_eq!(game.frame, 390);
    let line = game.dump_state();
    assert!(
        line.starts_with("frame=390 state=Other(2) px=80 pf=0 py=134"),
        "{line}"
    );
    assert!(line.contains("lives=0"), "{line}");
    assert!(matches!(game.phase, Phase::DemoStart));

    game.step(Buttons::NONE);
    let line = game.dump_state();
    assert!(
        line.starts_with("frame=391 state=Other(2) px=0 pf=0 py=0"),
        "{line}"
    );
    assert!(line.contains("lives=1"), "{line}");
    game.step(Buttons::NONE);
    assert!(
        game.dump_state()
            .starts_with("frame=392 state=Other(2) px=80 pf=0 py=96"),
        "{}",
        game.dump_state()
    );
    while game.frame < 418 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.scroll.position(), 0);
    game.step(Buttons::NONE);
    assert_eq!(game.frame, 419);
    assert!(matches!(game.phase, Phase::Normal));
    game.step(Buttons::NONE);
    assert_eq!(game.frame, 420);
    assert!(game.dump_state().contains("speed=1"));
    while game.frame < 423 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.scroll.fraction, 0);
    game.step(Buttons::NONE);
    assert_eq!(game.frame, 424);
    assert_eq!(game.scroll.fraction, tables::SCROLL_STEP);
}

#[test]
fn the_demo_drives_the_ship_from_the_9cb1_table() {
    let mut game = Game::new(patterned_assets());
    while game.frame < 420 {
        game.step(Buttons::NONE);
    }
    // `$9C58`: the first live frame installs the demo loadout, and `$9CB1`
    // starts with A held for 288 frames.
    assert_eq!(game.demo_button, 0x80);
    assert_eq!(game.power.speed_level(), 1);
    assert_eq!(game.player.x.int, 0x50);
    while game.frame < 707 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.demo_button, 0x80);
    game.step(Buttons::NONE);
    // `$9CB3 = $C4` (A+B+Down) on frame 708: 1.5 px/frame downwards.
    assert_eq!(game.frame, 708);
    assert_eq!(game.demo_button, 0xC4);
    assert_eq!((game.player.y.int, game.player.y.frac), (97, 128));
    while game.frame < 739 {
        game.step(Buttons::NONE);
    }
    assert_eq!((game.player.y.int, game.player.y.frac), (144, 0));
    game.step(Buttons::NONE);
    // `$9CB5 = $80` (A) and `$9CB6 = $00` wrap to a 510-frame hold.
    assert_eq!(game.demo_button, 0x80);
    assert_eq!(game.player.y.int, 144);
}

#[test]
fn demo_input_never_leaks_into_a_real_game() {
    // Starting from the title takes the normal path, not the attract one.
    let mut game = Game::new(patterned_assets());
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert!(!game.demo);
    while game.frame < 300 {
        game.step(Buttons::NONE);
    }
    // The `$9CB1` table never ran: no demo button, no movement, no shots.
    assert_eq!(game.demo_button, 0);
    let (x, y) = (game.player.x.int, game.player.y.int);
    game.step(Buttons::NONE);
    assert_eq!((game.player.x.int, game.player.y.int), (x, y));
    assert!(game.shots.iter().all(|shot| !shot.active));
}

#[test]
fn the_demo_ends_at_the_table_terminator_and_restarts_the_title() {
    let mut game = Game::new(patterned_assets());
    while game.frame < 3630 {
        game.step(Buttons::NONE);
    }
    // The 75-record table plus the `$00` wrap ends the demo here.
    assert_eq!(game.frame, 3630);
    assert_eq!(game.state, State::Title);
    assert!(!game.demo);
    // `$8187` restarts the title roll-in; its 130-frame re-entry and the
    // 256-frame transition put the next demo at 4017.
    while game.frame < 4017 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.frame, 4017);
    assert!(game.demo);
    assert!(matches!(game.phase, Phase::DemoStart));
    assert!(game.dump_state().starts_with("frame=4017 state=Other(2)"));
}

#[test]
fn start_on_the_demo_runs_the_continue_countdown_and_stage_intro() {
    let mut game = Game::new(patterned_assets());
    for _ in 0..180 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.frame, 180);
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert_eq!(game.frame, 181);
    assert!(matches!(game.phase, Phase::Attract(0)));
    game.step(Buttons::NONE);
    assert_eq!(game.frame, 182);
    assert!(matches!(game.phase, Phase::Continue(CONTINUE_FRAMES)));
    assert!(game.dump_state().starts_with("frame=182 state=Other(3)"));
    while game.frame < 262 {
        game.step(Buttons::NONE);
    }
    assert!(game.dump_state().starts_with("frame=262 state=Other(3)"));
    game.step(Buttons::NONE);
    assert!(matches!(game.phase, Phase::StageInit));
    assert!(game.dump_state().starts_with("frame=263 state=Other(4)"));
    game.step(Buttons::NONE);
    assert_eq!(game.frame, 264);
    // `$1B = 0`: the stage handler is running but the ship is still zero.
    assert!(matches!(game.phase, Phase::StageIntro(_)));
    assert!(game
        .dump_state()
        .starts_with("frame=264 state=Other(4) px=0 pf=0 py=0"));
    game.step(Buttons::NONE);
    // The oracle's `$00 = 4` stage-init window runs through frame 291.
    assert!(game
        .dump_state()
        .starts_with("frame=265 state=Other(4) px=0 pf=0 py=0"));
    while game.frame < 291 {
        game.step(Buttons::NONE);
    }
    assert!(game.dump_state().starts_with("frame=291 state=Other(4)"));
    game.step(Buttons::NONE);
    assert!(game
        .dump_state()
        .starts_with("frame=292 state=Playing px=80 pf=0 py=96"));
    // The scroll is gated on `$0D = 5`; first step on frame 297.
    while game.frame < 296 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.scroll.position(), 0);
    game.step(Buttons::NONE);
    assert_eq!(game.frame, 297);
    assert_eq!(game.scroll.fraction, tables::SCROLL_STEP);
}

#[test]
fn start_enters_the_stage_before_the_attract_timer() {
    let mut game = Game::new(patterned_assets());
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert_eq!(game.state, State::Playing);
    assert_eq!(game.phase, Phase::Blank(tables::STAGE_BLANK_FRAMES));
    // The screen is blanked and the dump keeps the position fields zero.
    let line = game.dump_state();
    assert!(
        line.contains("state=Other(2) px=0 pf=0 py=0 pyf=0"),
        "{line}"
    );
    for _ in 0..12 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.phase, Phase::Normal);
    assert!(game.dump_state().contains("state=Playing px=80 pf=0 py=96"));
}

#[test]
fn select_during_the_title_window_moves_the_cursor_ship() {
    // `$8212`/`$8299`: the menu-active window is `$00 = 1` (phase
    // `Attract` in the port); Select moves `$0320` between 134 and 150.
    let mut game = Game::new(patterned_assets());
    for _ in 0..200 {
        if matches!(game.phase, Phase::Attract(_)) {
            break;
        }
        game.step(Buttons::NONE);
    }
    assert!(matches!(game.phase, Phase::Attract(_)));
    assert_eq!(game.player.y.int, 134, "the ship starts beside 1 PLAYER");

    game.step(Buttons {
        select: true,
        ..Buttons::NONE
    });
    game.step(Buttons::NONE);
    assert_eq!(game.menu_selection, 1);
    assert_eq!((game.player.x.int, game.player.y.int), (0x50, 150));

    game.step(Buttons {
        select: true,
        ..Buttons::NONE
    });
    game.step(Buttons::NONE);
    assert_eq!(game.menu_selection, 0);
    assert_eq!((game.player.x.int, game.player.y.int), (0x50, 134));
}

#[test]
fn select_toggles_the_title_menu_and_start_locks_the_mode() {
    // `$8212`/`$8231`: only the Select edge moves `$0F`. Up/Down are
    // ordinary pad buttons and never touch it.
    let mut game = Game::new(patterned_assets());
    for button in [
        Buttons {
            up: true,
            ..Buttons::NONE
        },
        Buttons {
            down: true,
            ..Buttons::NONE
        },
    ] {
        game.latched_input = button;
        game.step(button);
        assert_eq!(game.menu_selection, 0, "Up/Down do not move the menu");
    }

    game.step(Buttons {
        select: true,
        ..Buttons::NONE
    });
    assert_eq!(game.menu_selection, 1, "Select moves to 2 PLAYERS");
    game.step(Buttons::NONE);
    assert_eq!(game.menu_selection, 1, "holding Select does not repeat");
    game.step(Buttons {
        select: true,
        ..Buttons::NONE
    });
    assert_eq!(game.menu_selection, 0, "a second Select edge toggles back");

    // `$8225`: Start locks `$03 = $824C[$0F]`, i.e. the 2P mode.
    game.step(Buttons::NONE);
    game.step(Buttons {
        select: true,
        ..Buttons::NONE
    });
    assert_eq!(game.menu_selection, 1);
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert_eq!(game.state, State::Playing);
    assert_eq!(game.players(), crate::twoplayer::Players::Two);
}

/// `$8225`/`$82CD`: choosing `2 PLAYERS` and pressing Start locks `$03 =
/// $70`, then `$82CD` initialises both banks: `$18 = 0`, `$20/$21 = 3`,
/// both scores zero and `$0A` bits 0/1 set.
#[test]
fn starting_2p_from_the_title_menu_initialises_both_player_banks() {
    let mut game = Game::new(patterned_assets());
    assert_eq!(game.state, State::Title);

    game.step(Buttons {
        select: true,
        ..Buttons::NONE
    });
    assert_eq!(game.menu_selection, 1, "Select picks 2 PLAYERS");
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });

    assert_eq!(game.state, State::Playing);
    assert_eq!(game.players(), Players::Two);
    assert_eq!(game.two_player().mode, Players::Two);
    assert_eq!(game.current_player(), 0, "`$18` starts on P1");
    assert_eq!(game.two_player().alive_mask(), 3, "`$0A` bits 0/1");
    assert_eq!(game.two_player().slots.map(|slot| slot.lives), [3, 3]);
    assert_eq!(game.two_player().slots.map(|slot| slot.score), [0, 0]);
    assert_eq!(game.lives, 3);
    assert_eq!(game.score, 0);
}

#[test]
fn the_title_render_uses_the_extracted_screen_exactly() {
    let mut game = Game::new(patterned_assets());
    // `$8515`: frames 0-19 are blank; the 2 px/frame roll-in settles at
    // sample frame 132 (self.frame 133).
    let blank = game.assets.color(BLACK);
    assert!(
        game.framebuffer
            .as_rgba()
            .chunks_exact(4)
            .all(|p| p[0..3] == blank),
        "the title is off screen before f20"
    );
    game.frame = 133;
    game.render();
    let screen = &crate::rom::testing::rom().backgrounds.title;
    let tile = screen.tile(0, 0);
    let palette_index = screen.palette_index(0, 0);
    let bank = screen.chr_bank * 2;
    let pixel = game.assets.tile(bank, tile).unwrap()[0];
    let expected = game
        .assets
        .color(screen.palette[usize::from(palette_index) * 4 + usize::from(pixel)]);
    assert_eq!(&game.framebuffer.as_rgba()[..3], &expected);
    // Regression hash over the whole title framebuffer.
    assert_eq!(game.framebuffer.frame_hash(), TITLE_RENDER_HASH);
    assert!(
        game.build_sprites().is_empty(),
        "no gameplay sprites on the title"
    );
}

#[test]
fn the_title_rolls_in_two_pixels_per_frame() {
    let mut game = Game::new(patterned_assets());
    let blank = game.assets.color(BLACK);
    game.frame = 21;
    game.render();
    // Offset 224: only the right 32 px of the title are on screen.
    for x in 32..WIDTH {
        for y in (0..240).step_by(9) {
            assert_eq!(rgba_at(&game, x, y)[0..3], blank, "x {x} y {y}");
        }
    }
    assert!(
        (0..32).any(|x| rgba_at(&game, x, 100)[0..3] != blank),
        "the leading 32 px carry the title"
    );
    game.frame = 133;
    game.render();
    assert!(
        (0..WIDTH).any(|x| rgba_at(&game, x, 100)[0..3] != blank),
        "settled title spans the screen"
    );
}

#[test]
fn the_title_draws_no_cursor_sprite() {
    let mut game = Game::new(patterned_assets());
    let assets = patterned_assets();
    let screen = &crate::rom::testing::rom().backgrounds.title;
    let mut expected = Framebuffer::new();
    expected.clear(assets.color(BLACK));
    draw_title_screen(&mut expected, &assets, screen, 266 - 2 * 120);
    game.frame = 120;
    game.render();
    assert_eq!(
        game.framebuffer.as_rgba(),
        expected.as_rgba(),
        "only the extracted title and roll-in offset are drawn"
    );
}

#[test]
fn the_title_ship_appears_at_frame_135() {
    let mut game = Game::new(patterned_assets());
    game.frame = 135;
    game.render();
    let before = title_ship_box(&game);
    game.frame = 136;
    game.render();
    let after = title_ship_box(&game);
    assert_ne!(
        before, after,
        "the Vic Viper sprite appears beside 1 PLAYER"
    );
}

#[test]
fn the_title_row_does_not_blink_but_the_start_countdown_does() {
    // `$812F`: the selected row blinks only during the Start countdown;
    // the title menu (`$00 = 1`) never blanks it.
    let mut game = Game::new(patterned_assets());
    let blank = game.assets.color(BLACK);
    game.frame = 183;
    game.render();
    let mut on = Vec::new();
    for x in 96..184 {
        on.push(rgba_at(&game, x, 140));
    }
    for frame in 184..=199 {
        game.frame = frame;
        game.render();
        for (i, expected) in on.iter().enumerate() {
            assert_eq!(&rgba_at(&game, 96 + i, 140), expected, "frame {frame}");
        }
    }

    game.phase = Phase::Continue(80);
    game.frame = 183;
    game.render();
    for x in 96..184 {
        assert_eq!(
            rgba_at(&game, x, 140)[0..3],
            blank,
            "the first countdown row is off"
        );
    }
    game.phase = Phase::Continue(79);
    game.render();
    for (i, expected) in on.iter().enumerate() {
        assert_eq!(
            &rgba_at(&game, 96 + i, 140),
            expected,
            "the next 8 frames are on"
        );
    }
}

const TITLE_RENDER_HASH: u64 = 14_168_249_185_726_605_221;

#[test]
fn start_stage_enters_the_chosen_stage_from_the_title() {
    let mut game = Game::new(patterned_assets());
    game.set_mods(Mods {
        start_stage: 5,
        ..Mods::default()
    });
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert_eq!(game.state, State::Playing);
    assert_eq!(game.players(), Players::One);
    // A new game's stage, lives, score and ship column.
    assert_eq!(game.section(), 4);
    assert_eq!(game.stage_info().index, 4);
    assert_eq!(game.lives, 3);
    assert_eq!(game.score, 0);
    let (x, y) = stage::ship_start(crate::rom::testing::rom(), 4, 0);
    assert_eq!((game.player.x.int, game.player.y.int), (x, y));
    for _ in 0..12 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.phase, Phase::Normal);
    assert_eq!(game.section(), 4, "the boot keeps the chosen stage");
}

#[test]
fn start_stage_applies_to_both_two_player_modes() {
    // Alternating 2P: the lock comes from the title cursor, as usual.
    let mut alternating = Game::new(patterned_assets());
    alternating.set_mods(Mods {
        start_stage: 2,
        ..Mods::default()
    });
    alternating.menu_selection = 1;
    alternating.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert_eq!(alternating.players(), Players::Two);
    assert_eq!(alternating.section(), 1);
    assert_eq!(alternating.two.slots[0].lives, 3);
    assert_eq!(alternating.two.slots[1].lives, 3);
    assert_eq!(alternating.two.slots[1].score, 0);

    // Simultaneous 2P: the second ship starts on the chosen stage too.
    let mut both = Game::new(patterned_assets());
    both.set_mods(Mods {
        start_stage: 2,
        simultaneous_2p: true,
        ..Mods::default()
    });
    both.menu_selection = 1;
    both.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert!(both.simultaneous());
    assert_eq!(both.section(), 1);
    let (x, y) = stage::ship_start(crate::rom::testing::rom(), 1, 0);
    let second = both.second_ship().expect("both ships fly");
    assert_eq!((second.0, second.2), (x, y));
}

#[test]
fn start_stage_puts_both_two_player_checkpoints_on_the_chosen_stage() {
    let mut game = Game::new(patterned_assets());
    game.set_mods(Mods {
        start_stage: 4,
        ..Mods::default()
    });
    game.menu_selection = 1;
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    // Both slots respawn from the chosen phase, column 0.
    let want = crate::twoplayer::Checkpoint::new(crate::rom::testing::rom(), 3, 0, 0);
    assert_eq!(game.two.slots[0].checkpoint, want);
    assert_eq!(game.two.slots[1].checkpoint, want);
    assert_eq!(game.two.current, 0);
    assert_eq!(game.two.slots[0].checkpoint.ship_x, stage::ship_start(crate::rom::testing::rom(), 3, 0).0);
}

#[test]
fn start_stage_defaults_to_the_cartridge_start() {
    for start_stage in [1u8, 0, 255] {
        let mut game = Game::new(patterned_assets());
        game.set_mods(Mods {
            start_stage,
            ..Mods::default()
        });
        game.step(Buttons {
            start: true,
            ..Buttons::NONE
        });
        let expected = (start_stage.clamp(1, 7) - 1) as usize;
        assert_eq!(game.section() as usize, expected, "start_stage {start_stage}");
    }
    // The default leaves the boot exactly as it was.
    let mut plain = Game::new(patterned_assets());
    plain.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert_eq!(plain.section(), 0);
    assert_eq!(plain.stage_info().index, 0);
    assert_eq!(Mods::default().start_stage, 1);
}

#[test]
fn start_stage_applies_again_after_a_game_over() {
    let mut game = Game::new(patterned_assets());
    game.set_mods(Mods {
        start_stage: 3,
        ..Mods::default()
    });
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert_eq!(game.section(), 2);

    // `$82CD`/`$817E`: the world reset puts the title back.
    game.title_pending = true;
    game.step(Buttons::NONE);
    assert_eq!(game.state, State::Title);
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    assert_eq!(game.section(), 2);
    assert_eq!(game.lives, 3);
    assert_eq!(game.score, 0);
}

/// Press Start in the title's `$00 = 1` attract and run the `$00 = 3` boot to
/// normal play.
fn start_during_the_title_demo(game: &mut Game) {
    while !matches!(game.phase, Phase::Attract(_)) {
        game.step(Buttons::NONE);
    }
    game.step(Buttons {
        start: true,
        ..Buttons::NONE
    });
    for _ in 0..140 {
        if game.phase == Phase::Normal {
            return;
        }
        game.step(Buttons::NONE);
    }
}

#[test]
fn start_stage_enters_the_chosen_stage_from_the_title_demo() {
    let mut game = Game::new(patterned_assets());
    game.set_mods(Mods {
        start_stage: 5,
        ..Mods::default()
    });
    start_during_the_title_demo(&mut game);
    // A new game's stage, lives, score and ship column, as from the title.
    assert_eq!(game.phase, Phase::Normal);
    assert_eq!(game.players(), Players::One);
    assert_eq!(game.section(), 4);
    assert_eq!(game.stage_info().index, 4);
    assert_eq!(game.lives, 3);
    assert_eq!(game.score, 0);
    let (x, y) = stage::ship_start(crate::rom::testing::rom(), 4, 0);
    assert_eq!((game.player.x.int, game.player.y.int), (x, y));
}

#[test]
fn start_stage_applies_to_both_two_player_modes_from_the_demo() {
    // Alternating 2P: the attract Start locks the title cursor's mode.
    let mut alternating = Game::new(patterned_assets());
    alternating.set_mods(Mods {
        start_stage: 2,
        ..Mods::default()
    });
    alternating.menu_selection = 1;
    start_during_the_title_demo(&mut alternating);
    assert_eq!(alternating.players(), Players::Two);
    assert_eq!(alternating.section(), 1);
    assert_eq!(alternating.lives, 3);
    assert_eq!(alternating.score, 0);
    assert_eq!(alternating.two.slots[1].lives, 3);
    assert_eq!(alternating.two.slots[1].score, 0);

    // Simultaneous 2P: the second ship starts on the chosen stage too.
    let mut both = Game::new(patterned_assets());
    both.set_mods(Mods {
        start_stage: 2,
        simultaneous_2p: true,
        ..Mods::default()
    });
    both.menu_selection = 1;
    start_during_the_title_demo(&mut both);
    assert!(both.simultaneous());
    assert_eq!(both.section(), 1);
    let (x, y) = stage::ship_start(crate::rom::testing::rom(), 1, 0);
    let second = both.second_ship().expect("both ships fly");
    assert_eq!((second.0, second.2), (x, y));
}

#[test]
fn start_stage_default_keeps_a_demo_start_on_stage_one() {
    let mut game = Game::new(patterned_assets());
    start_during_the_title_demo(&mut game);
    assert_eq!(game.phase, Phase::Normal);
    assert_eq!(game.section(), 0);
    assert_eq!(game.lives, 3);
    assert_eq!(game.score, 0);
}

#[test]
fn start_stage_leaves_the_attract_demo_on_stage_one() {
    let mut game = Game::new(patterned_assets());
    game.set_mods(Mods {
        start_stage: 6,
        ..Mods::default()
    });
    for _ in 0..400 {
        game.step(Buttons::NONE);
    }
    assert!(game.demo, "the title rolled into the demo");
    assert_eq!(game.section(), 0);
    assert!(matches!(game.state, State::Playing));
}
