// The Start pause (`$15`, `docs/analysis/pause.md`).

const START: Buttons = Buttons {
    start: true,
    ..Buttons::NONE
};

/// One Start press and release; the flag flips one step after the edge.
fn tap_start(game: &mut Game) {
    game.step(START);
    game.step(Buttons::NONE);
    game.step(Buttons::NONE);
}

/// The state dump without its leading `frame=` counter, which keeps running.
fn dump_without_frame(game: &Game) -> String {
    let dump = game.dump_state();
    dump.split_once(' ').map_or(dump.clone(), |(_, rest)| rest.to_string())
}

fn running() -> Game {
    let mut game = Game::new(assets());
    game.dev_start_run();
    for _ in 0..40 {
        game.step(Buttons::NONE);
    }
    game
}

#[test]
fn start_pauses_and_resumes() {
    let mut game = running();
    tap_start(&mut game);
    assert!(game.paused());
    tap_start(&mut game);
    assert!(!game.paused());
}

#[test]
fn nothing_advances_while_paused() {
    let mut game = running();
    tap_start(&mut game);
    assert!(game.paused());
    let state = dump_without_frame(&game);
    let pixels = game.framebuffer().as_rgba().to_vec();
    let scroll = game.scroll();
    for _ in 0..120 {
        game.step(Buttons {
            right: true,
            fire: true,
            ..Buttons::NONE
        });
    }
    assert_eq!(game.scroll(), scroll);
    assert_eq!(dump_without_frame(&game), state);
    assert_eq!(game.framebuffer().as_rgba(), &pixels[..]);
    tap_start(&mut game);
    for _ in 0..8 {
        game.step(Buttons::NONE);
    }
    assert_ne!(game.scroll(), scroll, "the stage runs again after resume");
}

#[test]
fn start_is_ignored_where_the_rom_ignores_it() {
    // `$0D` entry blank, `$1B = $A0` death, `$C0` game over, demo `$09`.
    let setups: [fn(&mut Game); 4] = [
        |game| game.scroll_hold = 3,
        |game| game.phase = Phase::Dying(100),
        |game| game.game_over_timer = 100,
        |game| game.demo = true,
    ];
    for setup in setups {
        let mut game = running();
        setup(&mut game);
        tap_start(&mut game);
        assert!(!game.paused());
    }
}

#[test]
fn either_pad_pauses_simultaneous_2p() {
    let mut game = Game::new(assets());
    game.set_mods(Mods {
        simultaneous_2p: true,
        ..Mods::default()
    });
    game.menu_selection = 1;
    game.dev_start_run();
    for _ in 0..40 {
        game.step_players(Buttons::NONE, Buttons::NONE);
    }
    game.step_players(Buttons::NONE, START);
    game.step_players(Buttons::NONE, Buttons::NONE);
    game.step_players(Buttons::NONE, Buttons::NONE);
    assert!(game.paused());
    game.step_players(START, Buttons::NONE);
    game.step_players(Buttons::NONE, Buttons::NONE);
    game.step_players(Buttons::NONE, Buttons::NONE);
    assert!(!game.paused());
}

#[test]
fn pause_after_an_attract_start_takes_only_the_first_pad_in_2p() {
    let mut game = Game::new(assets());
    // The roll-in ends at 133; Select picks 2 PLAYERS and Start enters the
    // `$00 = 3` countdown, which must keep the locked 2P mode.
    for _ in 0..180 {
        game.step_players(Buttons::NONE, Buttons::NONE);
    }
    game.step_players(
        Buttons {
            select: true,
            ..Buttons::NONE
        },
        Buttons::NONE,
    );
    game.step_players(Buttons::NONE, Buttons::NONE);
    game.step_players(START, Buttons::NONE);
    game.step_players(Buttons::NONE, Buttons::NONE);
    for _ in 0..400 {
        game.step_players(Buttons::NONE, Buttons::NONE);
    }
    assert_eq!(game.state, State::Playing);
    assert_eq!(game.phase, Phase::Normal);
    assert_eq!(game.players(), Players::Two);

    let tap = |game: &mut Game, p1: Buttons, p2: Buttons| {
        game.step_players(p1, p2);
        game.step_players(Buttons::NONE, Buttons::NONE);
        game.step_players(Buttons::NONE, Buttons::NONE);
    };
    // `$9ADC` reads `$05`, the raw first pad: P2's Start is inert on either
    // turn, while P1's pad pauses on both.
    tap(&mut game, Buttons::NONE, START);
    assert!(!game.paused(), "P2 cannot pause P1's turn");
    tap(&mut game, START, Buttons::NONE);
    assert!(game.paused());
    tap(&mut game, START, Buttons::NONE);
    assert!(!game.paused());
    game.two.current = 1;
    tap(&mut game, Buttons::NONE, START);
    assert!(!game.paused(), "P2 cannot pause its own turn");
    tap(&mut game, START, Buttons::NONE);
    assert!(game.paused(), "P1's pad still pauses P2's turn");
}

#[test]
fn the_pause_survives_a_save_state() {
    let mut game = running();
    tap_start(&mut game);
    let image = game.snapshot();
    let mut other = running();
    other.restore(&image).expect("restore");
    assert!(other.paused());
    let mut resumed = running();
    resumed.restore(&running().snapshot()).expect("restore");
    assert!(!resumed.paused());
}

fn press(game: &mut Game, buttons: Buttons) {
    game.step(buttons);
    game.step(Buttons::NONE);
}

/// Up, Up, Down, Down, Left, Right, Left, Right, B, A.
fn konami(game: &mut Game) {
    let code = [
        Buttons { up: true, ..Buttons::NONE },
        Buttons { up: true, ..Buttons::NONE },
        Buttons { down: true, ..Buttons::NONE },
        Buttons { down: true, ..Buttons::NONE },
        Buttons { left: true, ..Buttons::NONE },
        Buttons { right: true, ..Buttons::NONE },
        Buttons { left: true, ..Buttons::NONE },
        Buttons { right: true, ..Buttons::NONE },
        Buttons { power: true, ..Buttons::NONE },
        Buttons { fire: true, ..Buttons::NONE },
    ];
    for buttons in code {
        press(game, buttons);
    }
    // The handler reads `$05` a step late.
    game.step(Buttons::NONE);
}

#[test]
fn pausing_and_resuming_emit_the_music_events() {
    let mut game = running();
    game.audio.drain().for_each(drop);
    tap_start(&mut game);
    assert_eq!(game.audio.drain().collect::<Vec<_>>(), [Sound::Pause]);
    tap_start(&mut game);
    assert_eq!(game.audio.drain().collect::<Vec<_>>(), [Sound::Resume]);
}

#[test]
fn the_konami_code_in_pause_gives_the_demo_loadout_once() {
    let mut game = running();
    tap_start(&mut game);
    konami(&mut game);
    let power = game.power();
    assert_eq!(
        (power.shield_hits(), power.has_missile(), power.speed_level(), power.options()),
        (5, true, 1, 2)
    );
    // `$3B` is now negative: a second code does nothing.
    game.power.dev_set_shield(0);
    konami(&mut game);
    assert_eq!(game.power().shield_hits(), 0);
    // A boss kill (`INC $3B`) re-arms it.
    game.start_stage_clear();
    konami(&mut game);
    assert_eq!(game.power().shield_hits(), 5);
}

#[test]
fn the_konami_code_does_nothing_outside_pause() {
    let mut game = running();
    konami(&mut game);
    assert_eq!(game.power().shield_hits(), 0);
    assert!(!game.power().has_missile());
}

#[test]
fn pause_works_in_the_section_transition_holds() {
    let mut game = running();
    game.phase = Phase::Transition(100);
    tap_start(&mut game);
    assert!(game.paused());
    let phase = game.phase;
    for _ in 0..30 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.phase, phase);
    assert!(matches!(phase, Phase::Transition(_)));
}

#[test]
fn the_pause_code_state_survives_a_save_state() {
    let mut game = running();
    tap_start(&mut game);
    konami(&mut game);
    let mut other = running();
    other.restore(&game.snapshot()).expect("restore");
    other.power.dev_set_shield(0);
    konami(&mut other);
    assert_eq!(other.power().shield_hits(), 0, "`$3B` stays negative");
}
