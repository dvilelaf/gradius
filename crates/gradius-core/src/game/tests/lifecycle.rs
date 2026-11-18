#[test]
fn a_death_plays_the_measured_fifty_frame_sequence_then_blanks() {
    // `$C0F4 = 2D 2E 2F 30 30 00`, one group per ten frames.
    let mut game = sandbox();
    game.begin_death();
    let expected: [&[u8]; 5] = [
        &[0x48, 0x4A],
        &[0x4C, 0x4E],
        &[0x50, 0x52],
        &[0x54, 0x56],
        &[0x54, 0x56],
    ];
    for step in 0..50 {
        assert_eq!(death_tiles(&game), expected[step / 10], "step {step}");
        game.step(Buttons::NONE);
    }
    assert!(death_tiles(&game).is_empty(), "blank after 50 frames");
}

#[test]
fn a_death_plays_the_measured_twenty_frame_burst() {
    let mut game = sandbox();
    game.add_explosion(100, 100, 20);
    let expected: [&[u8]; 4] = [&[0x74, 0x76], &[0x78, 0x7A], &[0x7C], &[0x7E]];
    for step in 0..20 {
        assert_eq!(
            burst_tiles(&game),
            expected[step / 5],
            "frame {step} of burst B"
        );
        game.update_explosions();
    }
    assert_eq!(game.explosion_count, 0, "the burst lasts exactly 20 frames");
}

#[test]
fn death_takes_a_life_and_restarts_the_section_window() {
    let mut game = playing();
    game.phase = Phase::Normal;
    game.lives = 3;
    game.begin_death();
    assert_eq!(game.phase, Phase::Dying(120));
    for _ in 0..=120 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.lives, 2);
    assert_eq!(game.phase, Phase::StageIntro(STAGE_INTRO_FRAMES - 1));
    assert_eq!(game.scroll.high, game.death_checkpoint);
}

#[test]
fn a_two_player_death_hands_over_and_keeps_both_banks() {
    let mut game = playing();
    game.players = Players::Two;
    game.new_run();
    game.phase = Phase::Normal;
    game.two.slots[0].score = 332_211;
    game.score_bcd = Bcd::from_decimal(332_211);
    game.score = 332_211;
    game.two.slots[1].score = 665_544;
    game.two.slots[1].checkpoint = crate::twoplayer::Checkpoint::new(crate::rom::testing::rom(), 2, 6, 1);
    // P1 dies at the start of the stage; the incoming player's checkpoint
    // is the one `$9B38` rebuilds from.
    game.section = 0;
    game.scroll.high = 2;
    game.begin_death();
    for _ in 0..=120 {
        game.step(Buttons::NONE);
    }

    assert_eq!(game.current_player(), 1, "`$18` 0 -> 1");
    assert_eq!(game.two_player().slots[0].lives, 2);
    assert_eq!(game.two_player().slots[0].score, 332_211);
    assert_eq!(game.lives, 2, "the dump keeps the P1 `$20` bank");
    assert_eq!(game.two_player().slots[1].lives, 3);
    assert_eq!(game.two_player().slots[1].score, 665_544);
    assert_eq!(game.section(), 2);
    assert_eq!(game.scroll().high, 6);
    // `$8931`: the HUD follows `$18`, while the dump keeps P1's bank.
    assert_eq!(game.current_lives(), 3);
    assert_eq!(
        game.current_score_digits(),
        Bcd::from_decimal(665_544).digits
    );
    assert_eq!(game.score_bcd(), Bcd::from_decimal(332_211).digits);
}

#[test]
fn game_over_needs_both_players_out() {
    let mut game = playing();
    game.players = Players::Two;
    game.new_run();
    game.phase = Phase::Normal;
    game.two.slots[0].lives = 0;
    game.lives = 0;
    game.two.slots[1].lives = 0;

    // P1's last ship: P2 is still alive, so this is a handoff only.
    game.begin_death();
    for _ in 0..=120 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.current_player(), 1);
    assert_eq!(game.two_player().slots[0].lives, 0xFF);
    assert_eq!(game.game_over_timer, 0);
    assert_eq!(game.two_player().alive_mask(), 2);

    // P2's last ship: both bits are clear, so `$1B = $C0`.
    game.begin_death();
    for _ in 0..=120 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.two_player().slots[1].lives, 0xFF);
    assert!(game.game_over_timer > 0);
    assert_eq!(game.two_player().alive_mask(), 0);
    // `$980B`: the last ship was P2's, so the banner patches the `31`
    // tile to `32`.
    assert_eq!(game.current_player(), 1);
    assert_eq!(game.game_over_banner(), "2P GAME OVER");
}

/// `$9801`: with the last ship gone the banner replaces the power-meter
/// row, centred under the playfield; it is not the old centred box.
#[test]
fn the_game_over_banner_replaces_the_meter_row() {
    let mut game = Game::new(patterned_assets());
    game.state = State::Playing;
    game.new_run();
    game.phase = Phase::Normal;
    game.lives = 0;
    game.begin_death();
    for _ in 0..=120 {
        game.step(Buttons::NONE);
    }
    assert!(game.game_over_timer > 0);
    assert_eq!(game.game_over_banner(), "1P GAME OVER");

    game.game_over_timer = 0;
    game.render();
    let without = game.framebuffer.as_rgba().to_vec();
    game.game_over_timer = 399;
    game.render();
    let with = game.framebuffer.as_rgba().to_vec();
    let row_differs = |y: usize| {
        (0..WIDTH).any(|x| {
            let at = (y * WIDTH + x) * 4;
            with[at..at + 3] != without[at..at + 3]
        })
    };
    assert!(
        row_differs(hud::METER_Y as usize),
        "the meter row is replaced"
    );
    assert!(
        row_differs(hud::METER_Y as usize + 4),
        "the banner spans the row"
    );
}

#[test]
fn the_continue_code_restores_the_last_player_and_resumes_play() {
    let mut game = at_game_over(332_211);

    for press in continue_presses() {
        game.step(press);
    }
    assert_eq!(game.two_player().continue_index, 10);

    for _ in 0..400 {
        if game.game_over_timer == 0 {
            break;
        }
        game.step(Buttons::NONE);
    }
    assert_eq!(game.state, State::Playing);
    assert_eq!(game.lives, 3);
    assert_eq!(game.two_player().slots[0].lives, 3);
    assert_eq!(game.two_player().slots[0].extra_life, 1);
    assert_eq!(game.two_player().slots[0].score, 0);
    assert!(game.two_player().slots[0].alive);
    // `$24 = 0`: only the checkpoint column is cleared.
    assert_eq!(game.two_player().slots[0].checkpoint.scroll, 0);
    assert_eq!(game.score_bcd, Bcd::default());
    assert_eq!(game.score, 0);
    assert!(!game.title_pending);
    // `$97D0` -> `$9B38`: the stage rebuild starts, then `$1B = $80`.
    for _ in 0..STAGE_INTRO_FRAMES {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.stage_byte(), 0x80);
}

#[test]
fn a_wrong_continue_press_resets_the_code() {
    let mut game = at_game_over(0);
    let presses = continue_presses();
    for press in presses[..4].iter().copied() {
        game.step(press);
    }
    assert_eq!(game.two_player().continue_index, 4);

    // Expected B (`$40`), but Down (`$04`) arrives: `$33` resets to 0.
    game.step(Buttons {
        down: true,
        ..Buttons::NONE
    });
    assert_eq!(game.two_player().continue_index, 0);

    for _ in 0..400 {
        if game.title_pending {
            break;
        }
        game.step(Buttons::NONE);
    }
    assert!(game.title_pending);
    game.step(Buttons::NONE);
    assert_eq!(game.state, State::Title);
}

#[test]
fn a_game_over_without_the_code_still_returns_to_the_title() {
    let mut game = at_game_over(0);
    for _ in 0..400 {
        if game.state == State::Title {
            break;
        }
        game.step(Buttons::NONE);
    }
    assert_eq!(game.state, State::Title);
    assert_eq!(game.lives, 0);
    assert_eq!(game.two_player().alive_mask(), 0);
    game.step(Buttons::NONE);
    assert_eq!(game.state, State::Title);
    assert_eq!(game.lives, 0);
}

#[test]
fn a_turn_switch_does_not_reset_the_shared_stage() {
    let mut game = playing();
    game.players = Players::Two;
    game.new_run();
    game.section = 2;
    game.scroll.high = 4;
    game.scroll.low = 0x20;
    game.objects[9] = Object {
        active: true,
        kind: 0x85,
        ..Object::default()
    };

    game.two.next_turn();

    assert_eq!(game.current_player(), 1);
    assert_eq!(game.section(), 2);
    assert_eq!(game.scroll().high, 4);
    assert_eq!(game.scroll().low, 0x20);
    assert!(game.objects[9].live());
}

#[test]
fn dev_keep_alive_suppresses_death_and_is_off_by_default() {
    let lethal = || {
        let mut game = sandbox();
        game.objects[9] = object(0x86, 0x40, 0x38);
        game.objects[9].terrain = true;
        game.player = Player::new(0x40, 0x38);
        game
    };
    let mut normal = lethal();
    normal.collide_player_objects();
    assert!(normal.player.dying);
    let mut kept = lethal();
    kept.dev_keep_alive(true);
    kept.collide_player_objects();
    assert!(!kept.player.dying);
}
