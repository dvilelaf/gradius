#[test]
fn stage_init_blanks_for_six_frames() {
    let mut game = playing();
    assert_eq!(game.phase, Phase::Blank(tables::STAGE_BLANK_FRAMES));
    for expected in (0..tables::STAGE_BLANK_FRAMES).rev() {
        game.step(Buttons::NONE);
        assert_eq!(game.phase, Phase::Blank(expected), "remaining {expected}");
    }
    game.step(Buttons::NONE);
    assert_eq!(game.phase, Phase::Init(0));
}

#[test]
fn determinism_over_three_thousand_frames() {
    let mut first = playing();
    let mut second = playing();
    for frame in 0..3_000u32 {
        let input = match frame % 120 {
            0..=59 => Buttons {
                right: true,
                fire: true,
                ..Buttons::NONE
            },
            60..=69 => Buttons {
                up: true,
                fire: true,
                ..Buttons::NONE
            },
            70..=79 => Buttons {
                power: true,
                ..Buttons::NONE
            },
            _ => Buttons {
                left: true,
                fire: true,
                ..Buttons::NONE
            },
        };
        first.step(input);
        second.step(input);
        assert_eq!(first.state_hash(), second.state_hash(), "frame {frame}");
    }
}

#[test]
fn a_kept_alive_run_reaches_the_music_scroll_cuts() {
    // `MusicDirector` (gradius-app) selects Stage1B at scroll `$0400` and
    // Boss at `BOSS_SCROLL` from `scroll().position()`, so the core must
    // actually present those values during a kept-alive run.
    let mut game = sandbox();
    game.dev_keep_alive(true);
    let mut second_half = false;
    let mut boss = false;
    for _ in 0..8_000 {
        game.step(Buttons {
            fire: true,
            ..Buttons::NONE
        });
        let position = game.scroll().position();
        second_half |= position >= 0x0400;
        boss |= position >= 2820;
    }
    assert!(second_half, "the run crosses $0400");
    assert!(boss, "the run reaches the boss scroll cut");
}

#[test]
fn entity_counts_stay_bounded() {
    let mut game = playing();
    let mut best_score = 0;
    for frame in 0..6_000u32 {
        game.step(Buttons {
            right: frame % 240 < 120,
            left: frame % 240 >= 120,
            up: frame % 120 < 60,
            fire: true,
            power: frame % 400 == 0,
            start: frame % 900 == 0,
            ..Buttons::NONE
        });
        assert!(game.objects.iter().filter(|object| object.active).count() <= LIVE_SLOTS);
        assert!(game.enemy_bullet_count <= MAX_ENEMY_BULLETS);
        assert!(game.explosion_count <= MAX_EXPLOSIONS);
        assert!(game.power.options() <= crate::powerup::MAX_OPTIONS);
        best_score = best_score.max(game.score);
    }
    assert!(best_score > 0, "shots must destroy table-spawned enemies");
}

#[test]
fn nmi_frame_holds_on_a_lag_frame() {
    // `$8073`: a frame whose previous NMI overran skips the `$80B6` body,
    // so `$02` does not advance.
    let mut game = sandbox();
    game.nmi_frame = 0x22;
    game.nmi_lag = true;
    game.step(Buttons::NONE);
    assert_eq!(game.nmi_frame, 0x22);
    game.step(Buttons::NONE);
    assert_eq!(game.nmi_frame, 0x23);
}

#[test]
fn a_stage_anchor_seeds_the_nmi_counter_from_the_cartridge() {
    // `$02` on oracle frame 297 is `0x24`; the seed is one less because
    // `step` increments before the frame body.
    let mut seeded = Game::new_at_stage_seeded(patterned_assets(), 6, 0x24 - 1);
    seeded.step(Buttons::NONE);
    assert_eq!(seeded.nmi_frame, 0x24);
    // The default keeps the power-on seed, so a run booted at frame zero
    // is unchanged.
    let mut power_on = Game::new_at_stage(patterned_assets(), 6);
    power_on.step(Buttons::NONE);
    assert_eq!(power_on.nmi_frame, 0xFD);
}

#[test]
fn the_last_life_holds_the_stage_then_returns_to_the_title() {
    // `$97E4`: a wrap skips `$9B38`; the stage stays under `$5B` for the
    // `$4C = 120` hold and the title takes over 398 frames later.
    let mut game = sandbox();
    game.lives = 0;
    game.player.dying = true;
    game.finish_death();
    assert_eq!(game.lives, 0xFF);
    assert_eq!(game.phase, Phase::Normal);
    assert!(game.player.dying, "the last life does not respawn");
    for _ in 0..398 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.state, State::Playing);
    assert_eq!(game.scroll.low, 0);
    assert!(game.objects.iter().all(|object| !object.active));
    assert_eq!(game.player.x.int, 0x50);
    game.step(Buttons::NONE);
    assert_eq!(game.state, State::Title);
}

#[test]
fn advancing_a_section_lets_the_next_core_arm() {
    // `$96C2` clears `$39`/`$5F` and `$1B = $80` leaves the boss sequence.
    // The port-only `boss_spawned` latch must clear too, or stage 2 can
    // never run its own `$0C` arming and core.
    let mut game = sandbox();
    game.boss_spawned = true;
    game.boss_stage = 4;
    game.stage_clear_ready = true;
    game.volcano_kills = 4;
    game.advance_section();
    assert_eq!(game.section, 1);
    assert!(!game.boss_spawned, "stage 2 re-arms the core sequence");
    assert!(!game.stage_clear_ready);
    assert_eq!(game.volcano_kills, 0);
    assert_eq!(game.boss_stage, 0);
    game.scroll.high = 0x0C;
    game.step(Buttons::NONE);
    assert_eq!(game.boss_stage, 1, "stage 2 arms its own core");
}

#[test]
fn advancing_loads_the_next_stage_descriptor_and_resets_scroll() {
    // `$9928` + `$96C2` on the frame the 514-frame clear reaches zero.
    let mut game = sandbox();
    game.scroll.high = 0x0E;
    game.boss_spawned = true;
    game.boss_stage = 4;
    game.boss_timer = 771;
    game.stage_clear_ready = true;
    game.boss_clear = 1;
    game.boss_clear_armed = true;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.section, 1);
    assert_eq!(game.scroll.high, 0, "`$96C2` clears `$3F`");
    assert!(!game.boss_spawned, "the stage-2 core must re-arm");
    assert_eq!(game.boss_stage, 0);
    assert_eq!(game.boss_timer, 0);
    assert!(!game.stage_clear_ready);
    assert_eq!(game.phase, Phase::Normal);
    let info = game.stage_info();
    assert_eq!(info.index, 1);
    assert_eq!(info.terrain_row, 1);
    assert_eq!(game.stage_music(), 0x59);
}

#[test]
fn stage_two_selects_the_stage_two_background() {
    let stage2 = Game::new_at_stage(patterned_assets(), 1);
    let expected = &crate::rom::testing::rom().backgrounds.stages[1] as *const _;
    assert_eq!(
        crate::stage_bg::background(crate::rom::testing::rom(), usize::from(stage2.section()))
            .map(|screen| screen as *const _)
            .unwrap(),
        expected
    );
    // Stage 1 still selects its own extracted screen.
    let stage1 = Game::new_at_stage(patterned_assets(), 0);
    assert_eq!(
        crate::stage_bg::background(crate::rom::testing::rom(), usize::from(stage1.section()))
            .map(|screen| screen as *const _)
            .unwrap(),
        &crate::rom::testing::rom().backgrounds.stages[0] as *const _
    );
}

#[test]
fn stage_three_starts_from_the_catalog_and_reads_its_streams() {
    let game = Game::new_at_stage(patterned_assets(), 2);
    assert_eq!(game.section(), 2);
    assert_eq!((game.player.x.int, game.player.y.int), (0x50, 0x60));
    let info = game.stage_info();
    assert_eq!(info.index, 2);
    assert_eq!(info.terrain_row, 2);
    assert_eq!(game.stage_music(), 0x5B);
    assert_eq!(game.rom().tables.script_root[2], 0xC680);
    assert_eq!(game.stage_boss_trigger(), 0x0C);
    assert_eq!(info.boss_kind, Some(0x98));
    assert_eq!(info.quirks, stage_catalog::QUIRK_TAIL);
    let expected = &crate::rom::testing::rom().backgrounds.stages[2] as *const _;
    assert_eq!(
        crate::stage_bg::background(crate::rom::testing::rom(), usize::from(game.section()))
            .map(|screen| screen as *const _)
            .unwrap(),
        expected
    );
    assert_eq!(crate::rom::testing::rom().collision.terrain[info.terrain_row].len(), 18);
}

#[test]
fn stage_three_reaches_the_shared_core_and_advances() {
    let mut game = Game::new_at_stage(patterned_assets(), 2);
    game.power.dev_set_shield(5);
    game.scroll.high = tables::SECTION_THRESHOLD[2] - 1;
    game.scroll.low = 0xFF;
    game.scroll.fraction = tables::SCROLL_STEP;
    game.step(Buttons::NONE);
    assert_eq!(game.boss_stage, 1, "stage 3 arms at `$9A30[2] = $0C`");
    game.normal_frame(Buttons::NONE, false);
    assert!(game.scroll_locked);
    game.boss_timer = 1;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.boss_stage, 2);
    game.scroll.high = 0x0D;
    game.scroll.low = 0;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.boss_stage, 4);
    assert!(game.objects[9].boss);
    assert_eq!(game.objects[9].kind, 0x98, "the shared Big Core");
    game.boss_clear = 1;
    game.boss_clear_armed = true;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.section, 3, "stage 3 exits to stage 4");
    assert_eq!(game.scroll.high, 0);
    assert_eq!(game.stage_music(), 0x5D);
    assert_eq!(game.rom().tables.script_root[3], 0xC5A7);
}

#[test]
fn stage_four_keeps_the_shared_intro_until_the_music_boundary() {
    // `$96C4` leaves `$1C` on `$93`; `$8354[3] = 4` switches it to the
    // stage theme 2047 frames into the stage (natural frame 31304).
    let mut game = Game::new_at_stage(patterned_assets(), 3);
    game.dev_keep_alive(true);
    assert_eq!(game.stage_music(), 0x5D);
    assert_eq!(game.music_boundary(), 4);
    assert_eq!(game.music(), SHARED_INTRO_MUSIC);
    let mut frames = 0;
    while game.scroll().high < 4 {
        game.step(Buttons::NONE);
        frames += 1;
        assert!(frames < 3_000, "the stage must cross `$3F = 4`");
        if game.scroll().high < 4 {
            assert_eq!(game.music(), SHARED_INTRO_MUSIC, "`$1C` before `$3F = 4`");
        }
    }
    assert!(frames > 2_000, "the switch is late: {frames} frames");
    assert_eq!(game.music(), 0x5D, "`$1C` after `$3F = 4`");
}

#[test]
fn stage_four_never_opens_the_stage_one_clear_gate() {
    // No `$39` setter exists for stage 4: the volcano gate is stage-1
    // only (`$AF70` checks `$19`), so `$99B3` always takes the 514-frame
    // `$98F0` end, never the stage-3 `$9928` bonus path.
    let mut game = Game::new_at_stage(patterned_assets(), 3);
    game.dev_keep_alive(true);
    for _ in 0..2_000 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.section, 3);
    assert_eq!(game.volcano_kills, 0);
    assert!(!game.stage_clear_ready, "stage 4 never sets `$39`");
    game.stage_clear_ready = true;
    game.start_stage_clear();
    assert_eq!(game.boss_clear, 514, "`$98F0`, not the `$9928` branch");
}

#[test]
fn the_stage_four_handoff_does_not_reset_the_ship_or_the_objects() {
    // `$96C4` is not `$9B38`: it clears `$39/$3A/$3F` and `$50-$70` and
    // re-uploads the HUD, but the ship and the object table survive.
    let mut game = Game::new_at_stage(patterned_assets(), 2);
    game.player.x = Fixed::px(0x63);
    game.player.y = Fixed::px(0x41);
    game.objects[9] = Object {
        active: true,
        kind: 0x85,
        x: Fixed::px(0x70),
        ..Object::default()
    };
    game.boss_clear = 1;
    game.boss_clear_armed = true;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.section, 3);
    assert_eq!((game.player.x.int, game.player.y.int), (0x63, 0x41));
    assert!(game.objects[9].live(), "`$0300` is not cleared");
    assert_eq!(game.objects[9].kind, 0x85);
    assert_eq!(game.music(), SHARED_INTRO_MUSIC, "`$96C4` keeps `$1C`");
}

#[test]
fn stage_four_boot_leads_the_spawner_and_still_holds_the_scroll() {
    // `$9B38` runs three gameplay frames before the first `$3E` step, so
    // the `--stage 4` window opens with slot 9 already six pixels in
    // (oracle frame 297: X 234, scroll `0:0:128`).
    let mut game = Game::new_at_stage(patterned_assets(), 3);
    assert_eq!((game.scroll().high, game.scroll().low), (0, 0));
    assert_eq!(game.scroll().fraction, 0, "the boot holds the scroll");
    let scout = game.objects[9];
    assert!(scout.live());
    assert_eq!(scout.kind, 0x85);
    // `$A411` runs after `$BBB1`, so the spawn frame itself does not move
    // the member: three warm-up frames leave it two steps in.
    assert_eq!(scout.x.int, 236);
    game.step(Buttons::NONE);
    assert_eq!(game.objects[9].x.int, 234);
    assert_eq!((game.scroll().low, game.scroll().fraction), (0, 0x80));
}

#[test]
fn stage_five_boot_spawns_the_straight_piece_and_counts_it_ten_points() {
    // `$9B38` warms the stage-5 spawner for three frames like stage 4, so
    // the first `$91` record (delay 0) is already the `$9D` straight piece
    // in slot 9 when the harness window opens.
    let mut game = Game::new_at_stage(patterned_assets(), 4);
    assert_eq!(game.section(), 4);
    assert_eq!((game.scroll().high, game.scroll().low), (0, 0));
    let piece = game.objects[9];
    assert!(piece.live());
    assert_eq!(piece.kind, 0x9D);
    assert_eq!((piece.x.int, piece.y.int), (238, 160));
    assert_eq!(
        piece.sprite, 0,
        "the piece stays in group 0 for eight frames"
    );
    // `$B622` with Y 9: `$014C` divides by eight, then `$012C` walks the
    // six groups `$52..$57` (`$B64A` at +9/+10/+11).
    for _ in 0..6 {
        game.step(Buttons::NONE);
    }
    assert!(game.objects[9].live());
    assert!(
        (0x52..=0x57).contains(&game.objects[9].sprite),
        "sprite {:02X} is outside the `$52..$57` cycle",
        game.objects[9].sprite
    );
    // The `$1D` piece is an ordinary 10-point shot kill (`$8468`), not the
    // `$8454` 300 path.
    let mut object = Object {
        active: true,
        kind: 0x9D,
        sprite: 0x53,
        ..Object::default()
    };
    game.destroy(&mut object, 0);
    assert_eq!(game.score, 10);
}

#[test]
fn stage_six_boot_warms_three_frames_like_stage_four() {
    // `$9B38` runs three gameplay frames before the first `$3E` step: the
    // `--stage 6` window opens with slot 9 already two steps in (oracle
    // frame 297: X 234, scroll `0:0:128`).
    let mut game = Game::new_at_stage(patterned_assets(), 5);
    assert_eq!((game.scroll().high, game.scroll().low), (0, 0));
    assert_eq!(game.scroll().fraction, 0, "the boot holds the scroll");
    let scout = game.objects[9];
    assert!(scout.live());
    assert_eq!(scout.kind, 0x85);
    assert_eq!(scout.x.int, 236);
    game.step(Buttons::NONE);
    assert_eq!(game.objects[9].x.int, 234);
    assert_eq!((game.scroll().low, game.scroll().fraction), (0, 0x80));
}

#[test]
fn stage_six_reads_the_no_core_quirk_instead_of_arming_the_core() {
    // `$99B3` sends `$19 >= 5` straight past `$84`/`$85`: no kind `$98`.
    // Stage 5 still arms the shared core at its own `$9A30 = $0B`.
    let mut stage5 = Game::new_at_stage(patterned_assets(), 4);
    stage5.scroll.high = 0x0B;
    stage5.normal_frame(Buttons::NONE, false);
    assert_eq!(stage5.boss_stage, 1, "stage 5 arms the Big Core");

    let mut game = Game::new_at_stage(patterned_assets(), 5);
    game.scroll.high = 0x0B;
    game.scroll.low = 0;
    game.scroll.fraction = 0;
    game.normal_frame(Buttons::NONE, false);
    // `$9A01`: `$9A28[$17] = 4` at the boot's rank 2, i.e. 1024 frames,
    // not a fixed 1024 or the stage-indexed `$9A28[5] = 5`.
    assert_eq!(game.endgame, Endgame::Hold(1024));
    assert_eq!(game.boss_stage, 0);
    assert!(!game.boss_spawned);
    assert!(game.scroll_locked, "`$81` holds from the next frame");
}

#[test]
fn stage_six_hold_runs_the_rank_window_and_the_wall_erase() {
    let mut game = Game::new_at_stage(patterned_assets(), 5);
    // The threshold frame is the one whose step lands exactly on `$0B:00`.
    game.scroll.high = 0x0A;
    game.scroll.low = 0xFF;
    game.scroll.fraction = 0x80;
    game.normal_frame(Buttons::NONE, false);
    // `$9A40` compares the frame-start `$3F`, so the threshold frame only
    // steps; the next one arms `$81`.
    assert_eq!(game.endgame, Endgame::None);
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0x0B, 0, 0)
    );
    // `$81`: the final half-pixel step runs, then `$5B` holds the scroll.
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.endgame, Endgame::Hold(1024));
    assert!(game.scroll_locked);
    assert_eq!((game.scroll().low, game.scroll().fraction), (0, 0x80));
    let held = game.scroll();
    let mut frames = 0;
    while game.endgame != Endgame::Erase {
        game.normal_frame(Buttons::NONE, false);
        frames += 1;
        assert!(frames < 1_100, "the `$82` window must end");
        if game.scroll_locked {
            assert_eq!(game.scroll(), held, "`$1B = $82` holds the scroll");
        }
    }
    // 1024 countdown frames, the `$83` frame and the two held `$86`
    // frames; the erase starts on the frame the scroll resumes.
    assert_eq!(frames, 1024 + 3);
    // `$86` resumes the scroll on the frame the erase starts, and no core
    // was ever created.
    assert_eq!((game.scroll().low, game.scroll().fraction), (1, 0));
    assert_eq!(game.wall_erase, 2);
    assert!(!game.boss_spawned);
    assert!(game.objects.iter().all(|object| object.kind != 0x98));
}

#[test]
fn stage_six_erases_two_cells_per_frame_until_the_eighty_eight_bytes() {
    let mut game = Game::new_at_stage(patterned_assets(), 5);
    game.endgame = Endgame::Erase;
    game.wall_erase = 0;
    game.scroll.high = 0x0B;
    game.scroll.low = 0;
    game.scroll.fraction = 0;
    game.cells.clear();
    for _ in 0..44 {
        game.normal_frame(Buttons::NONE, false);
        assert!(game.scroll.high < 0x0C, "the stage end must not fire");
    }
    assert_eq!(game.wall_erase, 88);
    assert_eq!(game.cells.len(), 88, "`$CD9F` queues both `$CDAD` calls");
    // `$CDAD` writes physical table-1 cells, so the erased world columns are
    // the 32-column base plus the entry column even after the `$86` scroll
    // moves: the table's 21..31 lands on 373..383, not on 375..385.
    let columns: std::collections::BTreeSet<u16> =
        game.cells.iter().map(|cell| cell.world_col).collect();
    assert_eq!(columns.iter().next().copied(), Some(373));
    assert_eq!(columns.iter().next_back().copied(), Some(383));
    assert_eq!(game.wall_erase, 88, "`$58` guards the index");
    assert_eq!(game.cells.len(), 88);
}

#[test]
fn stage_six_beams_spawn_every_fourth_frame_and_a_shot_pays_ten() {
    let mut game = Game::new_at_stage(patterned_assets(), 5);
    // The threshold frame arms `$81`; `$82` then holds the scroll and
    // dispatches `$C6D8`.
    game.scroll.high = 0x0A;
    game.scroll.low = 0xFF;
    game.scroll.fraction = 0x80;
    game.step(Buttons::NONE);
    assert_eq!(game.endgame, Endgame::None, "the threshold frame only steps");
    game.step(Buttons::NONE);
    assert_eq!(game.endgame, Endgame::Hold(1024));
    assert!(game.scroll_locked);

    let mut spawn_frames = Vec::new();
    let mut live = game.beams.live();
    // Twenty frames stay inside the first lifetime of a box (it leaves
    // the playfield after about 32 steps), so every spawn shows as an
    // increase.
    for _ in 0..20 {
        game.step(Buttons::NONE);
        let now = game.beams.live();
        if now > live {
            spawn_frames.push(game.frame);
        }
        live = now;
    }
    assert!(spawn_frames.len() >= 4, "{spawn_frames:?}");
    for pair in spawn_frames.windows(2) {
        assert_eq!(pair[1] - pair[0], 4, "spawn cadence: {spawn_frames:?}");
    }

    // `$BF6F`: a shot inside the box clears it and `$8468` pays ten.
    game.objects = [Object::default(); SLOT_COUNT];
    game.beams.clear();
    game.beams.spawn_attempt(0, 5);
    game.shots[0] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(152),
        y: Fixed::px(66),
        age: 0,
    };
    let score = game.score;
    game.collide_shots();
    assert_eq!(game.score, score + 10);
    assert_eq!(game.beams.live(), 0);
    assert!(!game.shots[0].active, "the shot is consumed");

    // `$C222`: the same box is lethal to a ship without a shield.
    game.beams.spawn_attempt(0, 5);
    game.player.x.int = 146;
    game.player.y.int = 59;
    game.collide_player_beams();
    assert!(game.player.dying, "the ship hit the 22 x 18 box");
}

#[test]
fn stage_six_beams_survive_a_snapshot() {
    let mut game = Game::new_at_stage(patterned_assets(), 5);
    game.beams.spawn_attempt(0, 5);
    game.beams.step();
    game.beams.spawn_attempt(4, 8);
    assert_eq!(game.beams.live(), 2);
    let image = game.snapshot();
    let mut restored = Game::new(patterned_assets());
    restored.restore(&image).unwrap();
    assert_eq!(restored.snapshot(), image);

    // An image written before the EXT tail restores the boxes empty. The
    // beam tail is 60 bytes of boxes plus 30 bytes of motion.
    let mut state = crate::snapshot::GameState::from_bytes(&image).unwrap();
    state.ext.truncate(state.ext.len() - 90);
    let mut old = Game::new(patterned_assets());
    old.restore(&state.to_bytes()).unwrap();
    assert_eq!(old.beams.live(), 0);
}

#[test]
fn stage_six_advances_to_stage_seven_on_the_98f0_byte() {
    // The `$86` end compares `$3F` with `$98F0[5] = $0C`; `$90` then steps
    // the scroll once more before `$96C4` clears `$3F` on the *next* frame,
    // which runs the new stage's spawner and scroll. `$3E` survives, so the
    // first stage-7 frame samples `$3E:$3D = 1:00`.
    let mut game = Game::new_at_stage(patterned_assets(), 5);
    game.endgame = Endgame::Erase;
    game.wall_erase = 88;
    game.scroll.high = 0x0B;
    game.scroll.low = 0xFF;
    game.scroll.fraction = 0x80;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(
        (game.section, game.scroll().high),
        (5, 0x0C),
        "`$3F` reaches `$0C` during `$86`"
    );
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.section, 5, "`$86` only arms `$90`");
    assert_eq!(game.endgame, Endgame::Advance);
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0x0C, 0, 0x80),
        "the threshold frame still steps the scroll"
    );
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.section, 6, "`$96C4` advances to stage 7");
    assert_eq!(game.endgame, Endgame::None);
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0, 1, 0),
        "`$3F = 0` keeps `$3E`/`$3D` and the frame steps once"
    );
    assert_eq!(game.phase, Phase::Normal);
}

#[test]
fn stage_six_plays_through_to_stage_seven_without_a_core() {
    // The whole measured end path from normal play: `$0B` (2816 px),
    // 1024 held frames, the two `$83`/`$86` frames, the 510-step `$86`
    // run to `$0C`, then `$90`. `$98` never appears.
    let mut game = Game::new_at_stage(patterned_assets(), 5);
    game.dev_keep_alive(true);
    let mut frames = 0;
    while game.section() == 5 {
        game.step(Buttons::NONE);
        frames += 1;
        assert!(frames < 8_000, "stage 6 must reach `$98F0`");
        assert!(
            game.objects.iter().all(|object| object.kind != 0x98),
            "stage 6 has no Big Core"
        );
    }
    assert_eq!(game.section(), 6, "`$96C4` advances to stage 7");
    assert_eq!(frames, 7_172);
    // The `$90` frame runs `$96C4` at its start and still steps the scroll,
    // so the first stage-7 play frame samples `$3E:$3D = 1:00`.
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0, 1, 0)
    );
    game.step(Buttons::NONE);
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0, 1, 0x80)
    );
}

#[test]
fn stage_seven_boot_warms_three_frames_like_stage_six() {
    // `$9B38` runs three gameplay frames before the first `$3E` step, so
    // the `--stage 7` window opens with slot 9 already two steps in
    // (oracle frame 297: X 234, scroll `0:0:128`).
    let mut game = Game::new_at_stage(patterned_assets(), 6);
    assert_eq!((game.scroll().high, game.scroll().low), (0, 0));
    assert_eq!(game.scroll().fraction, 0, "the boot holds the scroll");
    let scout = game.objects[9];
    assert!(scout.live());
    assert_eq!(scout.kind, 0x85);
    assert_eq!(scout.x.int, 236);
    game.step(Buttons::NONE);
    assert_eq!(game.objects[9].x.int, 234);
    assert_eq!((game.scroll().low, game.scroll().fraction), (0, 0x80));
}

#[test]
fn stage_seven_arms_the_fixed_four_hundred_fifty_eight_frame_hold() {
    // `$9A01` special-cases `$19 == 6` with `$4D = 1, $4C = $CA` instead
    // of `$9A28[$17]`; `$99B3` never creates a core on stage 7.
    let mut game = Game::new_at_stage(patterned_assets(), 6);
    game.scroll.high = 0x0B;
    game.scroll.low = 0xFF;
    game.scroll.fraction = 0x80;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.endgame, Endgame::None, "the threshold frame only steps");
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0x0C, 0, 0)
    );
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.endgame, Endgame::Hold(458));
    assert_eq!(game.boss_stage, 0);
    assert!(!game.boss_spawned);
    assert!(game.scroll_locked, "`$81` holds from the next frame");
}

#[test]
fn stage_seven_holds_458_frames_then_runs_9865_at_7024() {
    // From the `$3F = $0C` frame: `$81`, the 458-frame `$82` window, the
    // `$83` and `$86` frames, then `$9865` (loop 0 -> 1, `$3F = 0`).
    let mut game = Game::new_at_stage(patterned_assets(), 6);
    game.dev_keep_alive(true);
    game.scroll.high = 0x0B;
    game.scroll.low = 0xFF;
    game.scroll.fraction = 0x80;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.endgame, Endgame::None, "the threshold frame only steps");
    // `$81`: the final half-pixel step runs, then `$5B` holds the scroll.
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.endgame, Endgame::Hold(458));
    assert!(game.scroll_locked);
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0x0C, 0, 0x80)
    );
    let held = game.scroll();
    let mut frames = 0;
    while game.endgame != Endgame::Ending {
        game.normal_frame(Buttons::NONE, false);
        frames += 1;
        assert!(frames < 500, "the `$82` window must end");
        if game.endgame != Endgame::Ending {
            assert_eq!(game.scroll(), held, "`$1B = $82` holds the scroll");
        }
        if game.endgame == Endgame::Bridge(1) {
            // `$9865` waits one frame after `$86`; the port must not enter
            // the ending on the `$86` frame itself.
            assert_eq!(game.loop_count, 0, "`$9865` runs one frame after `$86`");
        }
    }
    // 458 `$82` frames, the `$83` and `$86` frames, then the `$9865`
    // hand-off frame; the measured oracle enters the ending on 7024.
    assert_eq!(frames, 458 + 3);
    assert_eq!(game.loop_count, 1);
    assert_eq!(game.section, 6, "`$19` keeps the stage until `$9B38`");
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0, 0, 0x80),
        "`$3F = 0` keeps `$3E`/`$3D`"
    );
    assert_eq!((game.score, game.lives), (0, 3), "score/lives survive");
    assert!(!game.boss_spawned);
    assert!(game.objects.iter().all(|object| object.kind != 0x98));
    // The ending states now run: `$88` on 7025 zeroes `$3E/$3D`, and
    // `$9B38` re-places the ship on the second `$88` frame.
    game.normal_frame(Buttons::NONE, false);
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.endgame, Endgame::Ending);
    assert_eq!(game.ending_step, 0x89);
    assert_eq!(game.loop_count, 1);
    assert_eq!(
        (
            game.scroll().high,
            game.scroll().low,
            game.scroll().fraction
        ),
        (0, 0, 0)
    );
}

#[test]
fn the_stage_three_gate_clear_double_increments_to_stage_five() {
    // `$39 = 1`: the `$86` handoff is 513 frames after the core death,
    // `$8E` scrolls 4.5 px/frame for 968 frames, `$845A` pays 5000 once,
    // and `$96C4` advances `$19` a second time into stage 5.
    let mut game = Game::new_at_stage(patterned_assets(), 2);
    game.phase = Phase::Normal;
    game.dev_keep_alive(true);
    game.scroll = Scroll {
        fraction: 0,
        low: 0,
        high: 0x0D,
    };
    game.stage_clear_ready = true;
    game.objects[9] = Object {
        active: true,
        kind: 0x98,
        subtype: 0x80,
        x: Fixed::px(0xA0),
        y: Fixed::px(0x60),
        sprite: 0x32,
        boss: true,
        ..Object::default()
    };
    game.destroy_boss(9);
    assert_eq!(game.clear, Clear::WaitEnd);

    let mut previous = game.score;
    let mut section_three = None;
    let mut award = None;
    let mut award_delta = 0;
    let mut stage_five = None;
    for _ in 0..2000 {
        game.step(Buttons::NONE);
        if section_three.is_none() && game.section == 3 {
            section_three = Some(game.frame);
            assert!(game.bonus_wave, "`$3A` arms with the `$992E` branch");
            assert_eq!(
                (game.scroll.low, game.scroll.high, game.scroll.fraction),
                (0, 0, 0x80),
                "`$3F = 0` on the handoff frame"
            );
        }
        if award.is_none() && game.clear == Clear::Handoff {
            award = Some(game.frame);
            award_delta = game.score - previous;
        }
        if stage_five.is_none() && game.section == 4 {
            stage_five = Some(game.frame);
        }
        previous = game.score;
    }
    assert_eq!(
        section_three,
        Some(513),
        "one frame before the `$39 = 0` path"
    );
    assert_eq!(award, Some(513 + 968));
    assert_eq!(award_delta, 5000, "`$845A` pays 5000 exactly once");
    assert_eq!(stage_five, Some(513 + 969), "the second `INC $19`");
    assert!(!game.stage_clear_ready, "`$96C4` clears `$39`");
    assert!(!game.bonus_wave, "`$96C4` clears `$3A`");
    assert_eq!(game.clear, Clear::None);
}

#[test]
fn the_stage_three_natural_clear_keeps_the_514_frame_countdown() {
    let mut game = Game::new_at_stage(patterned_assets(), 2);
    game.phase = Phase::Normal;
    game.objects[9] = Object {
        active: true,
        kind: 0x98,
        subtype: 0x80,
        x: Fixed::px(0xA0),
        y: Fixed::px(0x60),
        sprite: 0x32,
        boss: true,
        ..Object::default()
    };
    game.destroy_boss(9);
    assert!(!game.stage_clear_ready);
    assert_eq!(game.clear, Clear::None);
    assert_eq!(game.boss_clear, 514);
}

#[test]
fn the_stage_two_reaches_the_shared_core_and_resets() {
    let mut game = Game::new_at_stage(patterned_assets(), 1);
    game.scroll.high = tables::SECTION_THRESHOLD[1] - 1;
    game.scroll.low = 0xFF;
    game.scroll.fraction = tables::SCROLL_STEP;
    game.step(Buttons::NONE);
    assert_eq!(game.boss_stage, 1, "stage 2 arms at `$9A30[1] = $0C`");
    // `$82` sets `$5B` on the next frame.
    game.normal_frame(Buttons::NONE, false);
    assert!(game.scroll_locked);
    game.boss_timer = 1;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.boss_stage, 2);
    game.scroll.high = 0x0D;
    game.scroll.low = 0;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.boss_stage, 4);
    assert!(game.objects[9].boss);
    assert_eq!(game.objects[9].kind, 0x98, "the shared Big Core");
    // `$98F7`: the shared 514-frame clear advances to stage 3.
    game.boss_clear = 1;
    game.boss_clear_armed = true;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.section, 2, "the stage-2 reset advances");
    assert!(!game.boss_spawned);
    assert_eq!(game.scroll.high, 0);
}

#[test]
fn four_volcano_kills_set_the_stage_one_clear_gate() {
    // `$AF5D`/`$AF70`/`$AF78`: `$046C >= 5` destroys the volcano, `$5F`
    // increments, and the fourth kill sets `$39`.
    let mut game = sandbox();
    for kill in 1..=4 {
        game.objects[5] = object(0x8F, 0x80, 0x80);
        game.objects[5].subtype = 0x80;
        game.objects[5].ai_state = 1;
        game.objects[5].size_class = 1;
        for _ in 0..5 {
            game.shots[0] = Shot {
                active: true,
                kind: 0,
                x: game.objects[5].x,
                y: game.objects[5].y,
                age: 0,
            };
            game.shot_hit_object(0, 5);
        }
        // `$AF51` runs in the next handler frame.
        game.update_objects();
        assert_eq!(game.volcano_kills, kill);
        assert_eq!(game.objects[5].kind, 2, "the volcano breaks up");
        assert_eq!(game.objects[5].anim, 2, "$016C = 2");
        assert_eq!(game.objects[5].anim_div, 0, "$014C = 0");
        assert_eq!(game.objects[5].sprite, 0x78, "$012C keeps $78");
    }
    assert!(game.stage_clear_ready, "the fourth kill sets $39");
    assert_eq!(game.score, 400, "$8458 awards 100 per volcano");
}

#[test]
fn the_section_transition_is_six_frames_for_stage_one() {
    // `$81` (1) + `$82` (`$9A28[0] = 3`) + `$83` (1) + `$84` (1).
    assert_eq!(stage::transition_frames(0), 6);
    assert_eq!(stage::transition_frames(0), stage::TRANSITION_FRAMES);
    assert_eq!(stage::transition_frames(2), 7);
}

