#[test]
fn the_ending_runs_87_to_8d_on_the_measured_frames() {
    let mut game = Game::new_at_ending(patterned_assets());
    // 7024 is the `$87` entry the seed reproduces.
    assert_eq!(game.frame, 7024);
    assert_eq!(game.loop_count, 1);
    assert_eq!(game.section, 6);
    assert_eq!(
        (game.scroll.high, game.scroll.low, game.scroll.fraction),
        (0, 0, 0x80)
    );

    // The step after each frame: a transition state is entered at the
    // end of the last frame of the previous state.
    let expected = [
        (7025, 0x88u8, 0u8),
        (7026, 0x89, 0),
        (7027, 0x8A, 0),
        (7028, 0x8B, 0),
        (7050, 0x8B, 0),
        (7051, 0x8C, 0),
        (7052, 0x8C, 0),
        (8253, 0x8D, 0),
        (8254, 0x8D, 0),
    ];
    for (frame, step, section) in expected {
        step_to(&mut game, frame);
        assert_eq!(game.frame, frame);
        assert_eq!(game.ending_step, step, "frame {frame}");
        assert_eq!(game.section, section, "frame {frame}");
    }
    // `$8D` restarts stage 1 and normal play resumes at 8282.
    step_to(&mut game, 8282);
    assert_eq!(game.endgame, Endgame::None);
    assert_eq!(game.phase, Phase::Normal);
    assert_eq!(game.section, 0);
    assert_eq!(game.loop_count, 1);
    // Score and lives survive the reset.
    assert_eq!((game.score, game.lives), (10830, 4));
}

#[test]
fn the_loop_restart_runs_the_measured_01_to_04_stage_one_boot() {
    let mut game = Game::new_at_ending(patterned_assets());
    // 8254 is the `$8D` frame: stage 1 is being reloaded but still shows
    // the ending picture, and `$1C` is the ending's `$A5`.
    step_to(&mut game, 8254);
    assert_eq!(game.stage_byte(), 0x8D);
    assert_eq!(game.music(), ENDING_MUSIC);
    assert_eq!((game.score, game.lives, game.loop_count), (10830, 4, 1));

    // 8255-8281: the `$9B38` boot, blank and without gameplay. `$1B` runs
    // `$01` twice, `$02` and `$03` once and `$04` for 23 frames.
    for (frame, byte) in [
        (8255u64, 0x01u8),
        (8256, 0x01),
        (8257, 0x02),
        (8258, 0x03),
        (8259, 0x04),
        (8281, 0x04),
    ] {
        step_to(&mut game, frame);
        assert_eq!(game.stage_byte(), byte, "frame {frame}");
        assert_eq!(game.phase, Phase::Init(0), "frame {frame} is the boot");
        assert_eq!(game.ppu_mask(), 0, "frame {frame} is black");
        assert_eq!(game.music(), ENDING_MUSIC, "frame {frame} keeps `$A5`");
        assert_eq!(
            (
                game.scroll().high,
                game.scroll().low,
                game.scroll().fraction
            ),
            (0, 0, 0),
            "frame {frame} does not scroll"
        );
        assert_eq!((game.score, game.lives), (10830, 4), "frame {frame}");
    }

    // 8282: playable again, still blank and held, `$A5` for one more frame.
    step_to(&mut game, 8282);
    assert_eq!(game.stage_byte(), 0x80);
    assert_eq!(game.phase, Phase::Normal);
    assert_eq!(game.music(), ENDING_MUSIC);
    assert_eq!((game.score, game.lives, game.loop_count), (10830, 4, 1));
    assert!(game.dump_state().contains("state=Playing"));
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0, 0, 0)
    );

    // `$93` starts on 8283 and the scroll first steps on 8287.
    step_to(&mut game, 8283);
    assert_eq!(game.music(), SHARED_INTRO_MUSIC);
    step_to(&mut game, 8287);
    assert_eq!(game.scroll().fraction, 128);
    step_to(&mut game, 8290);
    assert_eq!(game.stage_byte(), 0x80);
    assert_eq!(game.music(), SHARED_INTRO_MUSIC);
}

#[test]
fn the_ending_pays_10000_and_a_life_once_at_7998() {
    let mut game = Game::new_at_ending(patterned_assets());
    step_to(&mut game, 7997);
    assert_eq!((game.score, game.lives), (830, 3));
    step_to(&mut game, 7998);
    assert_eq!((game.score, game.lives), (10830, 4));
    // No second award anywhere in the rest of `$8C`.
    step_to(&mut game, 8253);
    assert_eq!((game.score, game.lives), (10830, 4));
    assert!(game.ending_paid);
}

#[test]
fn the_ending_text_reveals_one_character_every_nine_frames() {
    // An invented ending text record with the shape the pack carries: the
    // `$2380` address, sixteen tiles and the `$FE` end.
    let mut assets = patterned_assets();
    assets.stage = Some(StageAssets {
        ending_text_record: Some([
            0x23, 0x80, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
            0x16, 0x17, 0x18, 0x19, 0xFE,
        ]),
        ..crate::assets::pack_stage_assets()
    });
    let record = *assets.ending_text_record().unwrap();
    let mut game = Game::new_at_ending(assets);
    let mut reveals = Vec::new();
    let mut last = 0;
    while game.frame < 8300 {
        game.step(Buttons::NONE);
        if game.ending_text != last {
            reveals.push((game.frame, game.ending_text));
            last = game.ending_text;
        }
    }
    assert_eq!(reveals.len(), usize::from(ENDING_TEXT_CHARS));
    assert_eq!(reveals[0], (7538, 1));
    assert_eq!(reveals[6], (7592, 7), "the seventh tile at 7592");
    assert_eq!(reveals[15], (7673, 16), "the full line at 7673");
    for pair in reveals.windows(2) {
        assert_eq!(pair[1].0 - pair[0].0, 9, "one character per 9 frames");
    }
    // The record's sixteen tile cells sit between the address and the end.
    let tiles: Vec<u8> = record[2..18].to_vec();
    assert_eq!(tiles.len(), 16);
    assert_eq!(record[18], 0xFE);
}

#[test]
fn the_ending_objects_are_the_measured_mothership_ship_and_explosion() {
    // Oracle OAM tile multisets (sorted, the HUD bar `$6D` included), read
    // back at frames 7060/7200/7380/7390/7400/7420.
    let tiles = |game: &Game| {
        let mut tiles: Vec<u8> = game
            .build_sprites()
            .iter()
            .map(|sprite| sprite.tile.index)
            .collect();
        tiles.sort_unstable();
        tiles
    };
    let mut game = Game::new_at_ending(patterned_assets());

    // The oracle OAM stores the tile byte with bit 0 selecting the pattern
    // table; the port's `TileRef::index` is that pair's base, so `& 0xFE`.
    let expect = |oracle: &[u8]| {
        let mut tiles: Vec<u8> = oracle.iter().map(|tile| tile & 0xFE).collect();
        tiles.sort_unstable();
        tiles
    };

    // 7052-7057: only the `$9E` mothership; `$0135` is still zero.
    step_to(&mut game, 7055);
    assert_eq!(
        tiles(&game),
        expect(&[0x6D, 0xBD, 0xBF, 0xD6, 0xD6, 0xBD, 0xDD]),
        "mothership before the first step"
    );

    // 7058: the first `$BB7C` step, group `$96` = the one-tile `$01`, at
    // `(164, 136)` plus the step's `+1` X.
    step_to(&mut game, 7058);
    assert_eq!(
        tiles(&game),
        expect(&[0x6D, 0xBD, 0xBF, 0xD6, 0xD6, 0xBD, 0xDD, 0x01]),
        "ship sprite from the first step"
    );
    let ship = game
        .build_sprites()
        .iter()
        .find(|sprite| sprite.tile.index == 0x00)
        .copied()
        .expect("the ending ship sprite");
    assert_eq!((ship.x, ship.y), (165, 137), "`$0120` at (164, 136)");

    // 7200 is after 24 `$BB7C` entries: group `$9D` (tiles `$40..$4E`) at
    // the measured `(49, 69)`. The old player ship at (80, 96) is gone.
    step_to(&mut game, 7200);
    assert_eq!(
        tiles(&game),
        expect(&[
            0x6D, 0xBD, 0xBF, 0xD6, 0xD6, 0xBD, 0xDD, 0x40, 0x42, 0x44, 0x46, 0x48, 0x4A, 0x4C,
            0x4E,
        ]),
        "the walked ship and mothership"
    );
    let head = game
        .build_sprites()
        .iter()
        .find(|sprite| sprite.tile.index == 0x48)
        .copied()
        .expect("the `$48` part");
    assert_eq!((head.x, head.y), (49, 70), "`$0335/$0375` at 7200");

    // 7378-7407: the kind-2 slot-8 explosion replaces the mothership.
    step_to(&mut game, 7380);
    assert_eq!(
        tiles(&game),
        expect(&[0x6D, 0xD8, 0x2E, 0x62, 0xDA, 0x2E, 0x62]),
        "`$A0` explosion group"
    );
    step_to(&mut game, 7390);
    assert_eq!(
        tiles(&game),
        expect(&[
            0x6D, 0x8D, 0x95, 0x91, 0x91, 0x97, 0x8D, 0x8D, 0xAB, 0xA3, 0x8D, 0x9D, 0xAB, 0x93,
            0x9F, 0x8D, 0x99, 0xA7, 0x95,
        ]),
        "`$A2` explosion group"
    );
    step_to(&mut game, 7400);
    assert_eq!(
        tiles(&game),
        expect(&[0x6D, 0xAF, 0xAB, 0xB1, 0xA7, 0xAD, 0xA9]),
        "`$6A` explosion group"
    );

    // 7408: both slots are empty; only the HUD bar sprite is left.
    step_to(&mut game, 7420);
    assert_eq!(tiles(&game), vec![0x6C], "nothing but the HUD bar");
}

#[test]
fn the_ending_display_is_blank_until_7056() {
    // `$9C1E` leaves `$0D = 5`: the first four `$8C` frames stay black and
    // the measured first visible ending picture is 7056.
    let mut game = Game::new_at_ending(patterned_assets());
    let screen_off = game.assets.color(BLACK);
    let black = |game: &Game| {
        game.framebuffer()
            .as_rgba()
            .chunks(4)
            .all(|pixel| pixel[..3] == screen_off)
    };
    for frame in 7025..=7060 {
        step_to(&mut game, frame);
        if (7052..=7055).contains(&frame) {
            assert_eq!(game.ppu_mask(), 0, "frame {frame} is blank");
            assert!(black(&game), "frame {frame} is black");
        }
        if frame == 7056 {
            assert_ne!(game.ppu_mask(), 0, "7056 is the first visible frame");
            assert!(!black(&game), "7056 draws the ending screen");
        }
    }
}
