#[test]
fn the_big_core_death_scores_one_thousand() {
    // `xtask record-boss`: the score goes 1690 -> 2690 on the death frame.
    let mut game = sandbox();
    game.spawn_boss();
    let slot = game
        .objects
        .iter()
        .position(|object| object.boss)
        .expect("big core");
    game.destroy_boss(slot);
    assert_eq!(game.score, 1000);
    assert_eq!(game.boss_clear, 514);
    assert!(!game.scroll_locked, "the scroll resumes after the kill");
}

#[test]
fn the_overheated_core_keeps_its_debris_after_the_ai_writes_back() {
    // `$BA96` (overheat): `$B95C` runs from inside the core's handler, so
    // the slot-9 copy the `$ADA5` loop writes back must still be the
    // kind-2 debris. Before the handler mutated the local object the write
    // back revived the live `$98` core and stage 2 never ended.
    let mut game = sandbox();
    game.spawn_boss();
    game.objects[9].x = Fixed::px(0xA0);
    game.objects[9].anim = 0xFF;
    game.objects[9].anim_div = 5;
    game.objects[9].timer2 = 0;
    game.update_objects();
    assert_eq!(game.objects[9].kind, 2, "the core became debris");
    assert!(
        game.objects[9].anim_div == 0,
        "debris path 4 starts stepping"
    );
    assert_eq!(game.objects[9].anim, 4, "`$B982` takes debris path 4");
    assert!(!game.objects[7].live() && !game.objects[8].live());
    assert_eq!(game.boss_clear, 514);
    assert!(
        !game.scroll_locked && game.scroll_stop,
        "the death frame holds the scroll, like the damage kill"
    );
}

#[test]
fn the_core_rank_includes_the_stage_bit() {
    // `$9C3F`: rank = weapon + options + shield + `$19 != 0`, with `$9C3F`
    // adding the base the port already has. Stage 2's shield-only run is
    // rank 3, which is `$B8F2[3] = $40` / `$B904[3] = $46`.
    let mut game = sandbox();
    game.power.dev_set_shield(5);
    game.section = 0;
    assert_eq!(game.boss_rank(), 2);
    game.section = 1;
    assert_eq!(game.boss_rank(), 3);
}

#[test]
fn a_rank_two_aim_adds_the_quarter_step() {
    // `$BD38`-`$BD74`: rank 2+ adds `$40` to the major axis and a quarter
    // to the minor slope, with the leftover carry adding one more.
    // Object (176,40) aimed at the ship (22,73) has |dx|=154 and a
    // negative dy of 33, which `$BCE7` complements to 32, so the raw
    // `$83BA` minor slope is 32*256/154 = 53.
    let mut game = sandbox();
    game.player = Player::new(22, 73);
    game.power.dev_set_shield(5);
    game.section = 0;
    let mut core = object(0x0B, 0xB0, 0x28);
    game.aim_object(&mut core);
    assert_eq!(core.x_vel, -0x100, "rank 1 moves the major axis 1 px");
    assert_eq!(core.y_vel, 53);
    game.section = 1;
    let mut core = object(0x0B, 0xB0, 0x28);
    game.aim_object(&mut core);
    assert_eq!(core.x_vel, -0x140, "rank 2 majors at 1.25 px/frame");
    assert_eq!(core.y_vel, 53 + 13 + 1);
}

#[test]
fn the_c540_launcher_aim_matches_the_oracle_y_velocity() {
    // The stage-2 hazard at the `$C567` pair (224,16) aims at the held
    // ship (22,73). dx=202, negative dy of 57 complements to 56, so the
    // minor slope is 56*256/202 = 70 and rank 2 makes it 70+17+1 = 88.
    // The oracle's slot 4 is at y=66 on frame 15968 after 147 moves from
    // y=16, which is exactly 88/256 px/frame.
    let mut game = sandbox();
    game.player = Player::new(22, 73);
    game.power.dev_set_shield(5);
    game.section = 1;
    assert_eq!(game.stage_rank(), 2);
    let mut launcher = object(0x0B, 0xE0, 0x10);
    game.aim_object(&mut launcher);
    assert_eq!(launcher.x_vel, -0x140);
    assert_eq!(launcher.y_vel, 88);
}

#[test]
fn the_stage_five_large_object_aims_drifts_and_dies_at_the_rank_thresholds() {
    // `$CA58`: kind `$94`, `$048C = 1`, sprite `$81/$82`, a 0.5 px drift
    // and the `$CA51[2] = $50` up-step (plus the `SBC` carry when the
    // attached cannons skip the first `$AEDB`).
    let mut game = sandbox();
    game.section = 4;
    game.power.dev_set_shield(5);
    let mut object = Object {
        active: true,
        kind: 0x94,
        subtype: 0x80,
        x: Fixed::px(0xF0),
        y: Fixed::px(0x80),
        sprite: 0x81,
        size_class: 1,
        anim: 1,
        ..Object::default()
    };
    game.ai_large_object(9, &mut object);
    assert_eq!(object.kind, 0x94);
    assert_eq!(object.ai_state, 1);
    assert_eq!(object.sprite, 0x81, "`$042C & 8` selects the frame");
    assert_eq!((object.x.int, object.x.frac), (0xEF, 0x80), "one `$AEDB`");
    assert_eq!(
        (object.y.int, object.y.frac),
        (0x7F, 0xAF),
        "`$50` plus carry"
    );

    // Morph at `$CA43[2] = 14`: the one-time `$0460` clear re-centres by
    // +8 px and adds two sprite steps.
    object.ai_timer = 14;
    object.x = Fixed::px(0x64);
    object.y = Fixed::px(0x64);
    game.ai_large_object(9, &mut object);
    assert_eq!(object.size_class, 0, "32x32 box becomes 16x16");
    assert_eq!((object.x.int, object.y.int), (0x6B, 0x6B));
    assert_eq!(object.sprite, 0x83);

    // Death at `$CA4A[2] = 28`: 500 points, kind-2 debris, and the
    // attached `$0600` record is released (the measured 500 was a `$14`
    // kill, not the 10-point shot path).
    game.cannons[3] = Cannon {
        active: true,
        parent: 9,
        ..Cannon::default()
    };
    object.ai_timer = 28;
    game.ai_large_object(9, &mut object);
    assert_eq!(game.score, 500);
    assert_eq!(object.kind, 2);
    assert!(!game.cannons[3].active);
    assert_eq!(game.objects[7].kind, 2, "released cannon debris");
}

#[test]
fn the_large_object_keeps_drifting_while_the_scroll_is_locked() {
    // `$CA58` calls `$AEDB` directly, below the `$AED7` `$5B` test, so the
    // `$82` hold (`scroll_locked`) does not freeze it: 8572 goes `6F.00`
    // -> `6E.80` in the oracle.
    let mut game = sandbox();
    game.section = 4;
    game.scroll_locked = true;
    let mut object = Object {
        active: true,
        kind: 0x94,
        subtype: 0x80,
        x: Fixed::px(0x6F),
        y: Fixed::px(0x2E),
        sprite: 0x81,
        size_class: 1,
        anim: 1,
        ..Object::default()
    };
    game.ai_large_object(9, &mut object);
    assert_eq!((object.x.int, object.x.frac), (0x6E, 0x80), "one `$AEDB`");
}

#[test]
fn a_shot_through_the_middle_cannon_part_destroys_it_for_a_hundred() {
    // `$BFDC` -> `$BEED` -> `$BF05`: the probe `(x + 8, y + 8)` tests the
    // six parts bottom-up and only part 2 (`$BF2D`) takes the hit.
    let mut game = sandbox();
    game.section = 4;
    game.objects[9] = object(0x14, 0x80, 0x40);
    game.objects[9].anim = 1;
    game.cannons[3] = Cannon {
        active: true,
        parent: 9,
        index: 0,
        part_x: [0; CANNON_PARTS],
        part_y: [0; CANNON_PARTS],
        ..Cannon::default()
    };
    game.cannons[3].part_x[2] = 0x58;
    game.cannons[3].part_y[2] = 0x3E;
    game.shots[3] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x50),
        y: Fixed::px(0x38),
        age: 0,
    };

    // `$BEE4[rank]`: rank 1 in this sandbox, so the second hit kills.
    game.collide_shots();
    assert!(game.cannons[3].active, "one hit is not enough at rank 1");
    assert_eq!(game.cannons[3].hits, 1);
    assert!(!game.shots[3].active, "`$C0B1` consumes the shot");
    game.shots[3] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x50),
        y: Fixed::px(0x38),
        age: 0,
    };
    game.collide_shots();

    assert_eq!(game.score, 100, "`$BF43` awards 100");
    assert!(!game.cannons[3].active, "the record is cleared");
    assert_eq!(game.objects[9].anim, 0, "`$016C` loses one entry");
    assert!(!game.shots[3].active, "`$C0B1` consumes the shot");
    // `$BF43` writes the debris into the *shot* pool, so the object table
    // keeps nothing: slot 3 is not an object and the parent is untouched.
    assert!(!game.objects[3].live(), "no debris in the object table");
    assert_eq!(game.objects[9].kind, 0x14, "the parent is still `$14`");
}

#[test]
fn a_shot_into_a_cannon_stalk_costs_the_shot_but_no_damage() {
    // `$BF31`: a hit on any part other than 2 jumps to `$C0B1`.
    let mut game = sandbox();
    game.section = 4;
    game.objects[9] = object(0x14, 0x80, 0x40);
    game.objects[9].anim = 1;
    game.cannons[3] = Cannon {
        active: true,
        parent: 9,
        index: 0,
        part_x: [0; CANNON_PARTS],
        part_y: [0; CANNON_PARTS],
        ..Cannon::default()
    };
    game.cannons[3].part_x[1] = 0x58;
    game.cannons[3].part_y[1] = 0x3E;
    game.shots[3] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x50),
        y: Fixed::px(0x38),
        age: 0,
    };

    game.collide_shots();

    assert_eq!(game.score, 0, "no points without the weak point");
    assert!(game.cannons[3].active, "the cannon survives");
    assert_eq!(game.cannons[3].hits, 0);
    assert!(!game.shots[3].active, "the shot is still consumed");
}

#[test]
fn the_stage_five_chain_frames_do_not_tick_the_hold_timer() {
    // `$9658`/`$9695` jumps to `$9A7F` past the `$1B` dispatch, so the
    // `$5C >= 2` odd-`$02` chain never reaches `$96E2`'s `DEC $4C`.
    let mut game = sandbox();
    game.section = 4;
    game.boss_stage = 1;
    game.scroll_locked = true;
    game.boss_timer = 100;
    game.cannons[3] = Cannon {
        active: true,
        ..Cannon::default()
    };
    game.cannons[2] = Cannon {
        active: true,
        ..Cannon::default()
    };

    game.frame = 10; // even port frame -> odd `$02`, the chain frame
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.boss_timer, 100, "the chain frame skips `$96E2`");

    game.frame = 11; // even `$02`: the `$9698` path ticks `$4C`
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.boss_timer, 99);
}

#[test]
fn the_attached_cannons_fire_every_thirty_chain_calls_at_rank_two() {
    // `$CB8B`: the counter is incremented once per chain call and fires at
    // `$CBC4[rank]`; rank 2 is 30, and the alternate chain runs every
    // other frame, i.e. the measured 60-frame cadence.
    let mut game = sandbox();
    game.section = 4;
    game.power.dev_set_shield(5);
    game.objects[9] = object(0x85, 0x80, 0x40);
    game.cannons[3] = Cannon {
        active: true,
        parent: 9,
        ..Cannon::default()
    };
    for _ in 0..29 {
        game.update_cannons();
    }
    assert_eq!(game.cannons[3].timer, 29);
    assert_eq!(game.enemy_bullet_count, 0);
    game.update_cannons();
    assert_eq!(game.cannons[3].timer, 0);
    assert_eq!(game.enemy_bullet_count, 1);
    assert_eq!(game.enemy_bullets[0].kind, 0);
    assert_eq!(game.enemy_bullets[0].sprite, 0x86);
}

#[test]
fn the_attached_cannons_fire_from_the_tip_when_it_is_on_screen() {
    // `$CBCB`: the shot reads `$061D`/`$0625` (part 5), subtracts 8 on both
    // axes and is skipped when the tip is off screen (`$10 <= x < $F0`,
    // `y < $D0`). The `$0604` counter still resets on the skipped shot.
    let mut game = sandbox();
    game.section = 4;
    game.power.dev_set_shield(5);
    game.objects[9] = object(0x85, 0x80, 0x40);
    let mut cannon = Cannon {
        active: true,
        parent: 9,
        index: 0,
        timer: 29,
        ..Cannon::default()
    };
    cannon.part_x[0] = 0x90;
    cannon.part_y[0] = 0x90;
    cannon.part_x[CANNON_PARTS - 1] = 0x40;
    cannon.part_y[CANNON_PARTS - 1] = 0x50;
    game.cannons[1] = cannon;
    // `$0603 = 0` makes the first call an odd (no placement) pass.
    game.update_cannons();
    assert_eq!(game.enemy_bullet_count, 1);
    assert_eq!(game.enemy_bullets[0].x >> 8, 0x40 - 8);
    assert_eq!(game.enemy_bullets[0].y >> 8, 0x50 - 8);
    assert_eq!(game.cannons[1].timer, 0);

    game.cannons[1].part_counter = 0;
    game.cannons[1].timer = 29;
    game.cannons[1].part_x[CANNON_PARTS - 1] = 0x08;
    game.update_cannons();
    assert_eq!(game.enemy_bullet_count, 1, "an off-screen tip skips the shot");
    assert_eq!(game.cannons[1].timer, 0, "`$0604` still resets");

    game.cannons[1].part_counter = 0;
    game.cannons[1].timer = 29;
    game.cannons[1].part_x[CANNON_PARTS - 1] = 0x40;
    game.cannons[1].part_y[CANNON_PARTS - 1] = 0xD0;
    game.update_cannons();
    assert_eq!(game.enemy_bullet_count, 1, "`$0625 >= $D0` skips the shot");
}

#[test]
fn cannon_shots_fly_the_rank_scaled_shell_vector() {
    // `$CC10` hands the shot to `$BCAB`, which is the `$BC3E` `$BCAF`/`$BD59`
    // helper: from rank 2 the major axis is `$140` (1.25 px/frame) and the
    // minor axis is `slope + slope/4 + 1`. The shot leaves the tip at
    // `(0x38, 0x48)` with the ship at `(0x50, 0x60)`, a 24 x 24 delta, so
    // both axes move 1.25 px/frame.
    let mut game = sandbox();
    game.section = 4;
    game.power.dev_set_shield(5);
    game.objects[9] = object(0x85, 0x80, 0x40);
    let mut cannon = Cannon {
        active: true,
        parent: 9,
        index: 0,
        timer: 29,
        ..Cannon::default()
    };
    cannon.part_x[CANNON_PARTS - 1] = 0x40;
    cannon.part_y[CANNON_PARTS - 1] = 0x50;
    game.cannons[1] = cannon;
    game.update_cannons();
    assert_eq!(game.enemy_bullet_count, 1);
    let bullet = game.enemy_bullets[0];
    assert_eq!((bullet.vx, bullet.vy), (0x140, 0x141));
}

#[test]
fn cannon_shots_take_the_highest_free_proxy_and_its_leftover_sub_pixel() {
    // `$CBCB` scans `$0136` from 9 down and `$CBFE`/`$CC0D` write only the
    // integer bytes, so a shot lands in the highest free proxy and starts with
    // the fraction that proxy kept from its previous occupant.
    let mut game = sandbox();
    game.section = 4;
    game.power.dev_set_shield(5);
    game.objects[9] = object(0x85, 0x80, 0x40);
    let mut cannon = Cannon {
        active: true,
        parent: 9,
        index: 0,
        timer: 29,
        ..Cannon::default()
    };
    cannon.part_x[CANNON_PARTS - 1] = 0x40;
    cannon.part_y[CANNON_PARTS - 1] = 0x50;
    game.cannons[1] = cannon;
    game.bullet_slot_frac[9] = (0x30, 0x80);
    game.update_cannons();
    assert_eq!(game.enemy_bullet_count, 1);
    let bullet = game.enemy_bullets[0];
    assert_eq!(bullet.slot, 9, "the highest free `$0136` proxy");
    assert_eq!(bullet.x, (0x38 << 8) | 0x30);
    assert_eq!(bullet.y, (0x48 << 8) | 0x80);
}

#[test]
fn two_live_cannons_halve_the_stage_five_scroll_on_even_frames() {
    // `$9658`: `$5C >= 2` plus odd `$02` runs the alternate chain and sets
    // `$5B`; the normal path on the other frames still steps `$98E1`.
    let mut game = sandbox();
    game.section = 4;
    game.objects[9] = object(0x85, 0x80, 0x40);
    for cannon in &mut game.cannons[1..3] {
        *cannon = Cannon {
            active: true,
            parent: 9,
            ..Cannon::default()
        };
    }
    assert_eq!(game.live_cannon_count(), 2);
    game.step(Buttons::NONE);
    assert_eq!((game.scroll.low, game.scroll.fraction), (0, 0x80));
    game.step(Buttons::NONE);
    assert_eq!(
        (game.scroll.low, game.scroll.fraction),
        (0, 0x80),
        "the chain holds `$5B` on the even frame"
    );
    game.step(Buttons::NONE);
    assert_eq!((game.scroll.low, game.scroll.fraction), (1, 0));
}

#[test]
fn a_single_stage_five_cannon_ticks_on_every_frame() {
    // `$9A69` -> `$C76C` -> `$CB84` -> `$CB8B`: while the frame-start
    // `$5C` is below two the tail ticks the cannon every frame, with no
    // `$02` parity. Three `step`s cover an odd and an even `frame`.
    let mut game = sandbox();
    game.section = 4;
    game.objects[9] = object(0x85, 0x80, 0x40);
    game.cannons[3] = Cannon {
        active: true,
        parent: 9,
        ..Cannon::default()
    };
    for expected in [0xFF, 0xFE, 0xFD] {
        game.step(Buttons::NONE);
        assert!(game.cannons[3].active, "the parent stays alive");
        assert_eq!(game.cannons[3].part_counter, expected);
    }
}

#[test]
fn two_stage_five_cannons_only_tick_in_the_even_frame_chain() {
    // `$9658`/`$9684`: with `$5C >= 2` only the even-frame chain ticks;
    // `$CB84` returns on the other frames, so `$9A69` must not add a tick.
    let mut game = sandbox();
    game.section = 4;
    game.objects[9] = object(0x85, 0x80, 0x40);
    for cannon in &mut game.cannons[1..3] {
        *cannon = Cannon {
            active: true,
            parent: 9,
            ..Cannon::default()
        };
    }
    let mut previous = game.cannons[1].part_counter;
    let mut ticks = Vec::new();
    for _ in 0..4 {
        game.step(Buttons::NONE);
        assert!(game.cannons[1].active, "the parent stays alive");
        ticks.push((game.frame, game.cannons[1].part_counter != previous));
        previous = game.cannons[1].part_counter;
    }
    for (frame, ticked) in ticks {
        assert_eq!(ticked, frame % 2 == 0, "cannon tick on frame {frame}");
    }
    assert_eq!(previous, 0xFE);
}

#[test]
fn slowdown_off_runs_every_stage_five_pass_each_frame_with_two_cannons() {
    // The `stage5_slowdown` mod drops `$9658`, so the scroll, the ship, the
    // objects and the cannon pass run every frame like on the other stages.
    let mut game = two_cannon_stage_five();
    game.set_mods(crate::mods::Mods {
        stage5_slowdown: false,
        ..crate::mods::Mods::default()
    });
    game.objects[8] = object(4, 0x80, 0x10);
    game.step(Buttons::NONE); // `$B1DF` seeds the kind-4 velocities
    let mut previous = (
        (game.scroll.low, game.scroll.fraction),
        game.objects[8].x.int,
        game.player.x.int,
        game.cannons[1].part_counter,
    );
    for _ in 0..4 {
        game.latched_input = Buttons {
            right: true,
            ..Buttons::NONE
        };
        game.step(Buttons {
            right: true,
            ..Buttons::NONE
        });
        let now = (
            (game.scroll.low, game.scroll.fraction),
            game.objects[8].x.int,
            game.player.x.int,
            game.cannons[1].part_counter,
        );
        assert_ne!(now.0, previous.0, "the scroll steps every frame");
        assert_ne!(now.1, previous.1, "the object moves every frame");
        assert_ne!(now.2, previous.2, "the ship moves every frame");
        assert_ne!(now.3, previous.3, "the cannon ticks every frame");
        previous = now;
    }
    // Two cannons share one `$CBCB` fire latch, so the pair still emits one
    // shot every 30 calls; without the chain it lands after 30 frames.
    let mut game = two_cannon_stage_five();
    game.power.dev_set_shield(5); // rank 2: `$CBC4[2] = 30`
    game.set_mods(crate::mods::Mods {
        stage5_slowdown: false,
        ..crate::mods::Mods::default()
    });
    for _ in 0..29 {
        game.step(Buttons::NONE);
    }
    assert_eq!(
        game.enemy_bullet_count, 0,
        "no shot before the thirtieth frame"
    );
    game.step(Buttons::NONE);
    assert_eq!(
        game.enemy_bullet_count, 1,
        "the cannons still fire, after 30 frames not 60"
    );
}

#[test]
fn slowdown_on_keeps_the_alternate_chain() {
    // The cartridge setting: the held frames skip the scroll step and the
    // cannon tick, so four frames advance the scroll two pixels.
    let mut game = two_cannon_stage_five();
    game.set_mods(crate::mods::Mods {
        stage5_slowdown: true,
        ..crate::mods::Mods::default()
    });
    let mut previous = game.cannons[1].part_counter;
    let mut ticks = 0;
    for _ in 0..4 {
        game.step(Buttons::NONE);
        let ticked = game.cannons[1].part_counter != previous;
        ticks += usize::from(ticked);
        previous = game.cannons[1].part_counter;
    }
    assert_eq!(ticks, 2, "the chain ticks the cannon every other frame");
    assert_eq!((game.scroll.low, game.scroll.fraction), (1, 0));
}

#[test]
fn stage_three_turret_paints_base_then_alternate() {
    // `$C900`/`$C999`: state 0 opens on the sector test and writes the
    // base group; the next cooldown closes it and writes `+$10`.
    let mut game = sandbox();
    game.section = 2;
    game.scroll.high = 3;
    game.scroll.low = 0x50;
    game.scroll.fraction = 0;
    game.player.x = Fixed::px(0x10);
    game.player.y = Fixed::px(0x10);
    let mut turret = stage_three_turret(0x2323, 0x80, 0xA0, 0x38);
    game.ai_background_turret(&mut turret);
    assert_eq!(turret.ai_state, 0x14);
    assert_eq!(turret.timer2, 0x14);
    assert_eq!(turret.ai_mode, 75, "`$C930[rank 1]`");
    assert!(game.spawn_busy, "`$5D` is raised on opening");
    assert_eq!(game.vram_queue_len, 11);
    assert_eq!(
        game.cells,
        vec![
            stage3_enemies::Cell {
                world_col: 131,
                row: 25,
                tile: 0x00
            },
            stage3_enemies::Cell {
                world_col: 132,
                row: 25,
                tile: 0x8F
            },
            stage3_enemies::Cell {
                world_col: 131,
                row: 26,
                tile: 0x90
            },
        ]
    );

    // `$C9A4`: cooldown expiry rewrites the alternate art and clears the
    // hit counter.
    turret.ai_state = 0x14;
    turret.ai_timer = 2;
    turret.ai_mode = 0;
    game.spawn_busy = false;
    game.vram_queue_len = 0;
    game.ai_background_turret(&mut turret);
    assert_eq!(turret.ai_state, 0, "invulnerable alternate art");
    assert_eq!(turret.ai_timer, 0);
    assert_eq!(
        game.cells[3..].iter().map(|c| c.tile).collect::<Vec<_>>(),
        vec![0x20, 0x21, 0x22]
    );
}

#[test]
fn only_the_damageable_turret_accumulates_and_clears_hits() {
    let mut game = sandbox();
    game.section = 2;
    game.objects[9] = stage_three_turret(0x2323, 0x80, 0xA0, 0x38);
    game.shots[0] = Shot {
        active: true,
        kind: 0,
        x: game.objects[9].x,
        y: game.objects[9].y,
        age: 0,
    };
    game.shot_hit_object(0, 9);
    assert_eq!(game.objects[9].ai_timer, 0, "`$048C == 0` is invulnerable");

    game.objects[9].ai_state = 0x14;
    for hits in 1..=3 {
        game.shots[0].active = true;
        game.shot_hit_object(0, 9);
        assert_eq!(game.objects[9].ai_timer, hits);
    }
    // `$C776` runs in the handler, not through the 10-point destroy path.
    assert_eq!(game.objects[9].kind, 0x96);
    assert_eq!(game.score, 0);
}

#[test]
fn the_tenth_turret_kill_sets_the_gate_and_a_retry_clears_it() {
    let mut game = sandbox();
    game.section = 2;
    game.volcano_kills = 9;
    let mut turret = stage_three_turret(0x2323, 0x80, 0xA0, 0x38);
    turret.ai_timer = 3;
    game.ai_background_turret(&mut turret);
    assert_eq!(game.volcano_kills, 10);
    assert!(game.stage_clear_ready, "`$39` opens the stage-3 clear");
    assert_eq!(game.score, 300);

    game.lives = 3;
    game.finish_death();
    assert_eq!(game.volcano_kills, 10, "`$5F` survives the retry");
    assert!(!game.stage_clear_ready, "`$97D2` clears `$39` on death");

    // `$5F` still holds ten, so the next kill re-earns the gate.
    let mut turret = stage_three_turret(0x2323, 0x80, 0xA0, 0x38);
    turret.ai_timer = 3;
    game.ai_background_turret(&mut turret);
    assert!(game.stage_clear_ready);
}

#[test]
fn the_small_core_body_runs_every_frame_and_matches_the_oracle_lead_in() {
    // Oracle 20708: the `$2B` record spawns kind `$17` at X `$F0`, Y `$60`
    // and the same-frame `$B79B` body leaves X `$EE`, Y `$5F`.
    let mut game = sandbox();
    game.section = 2;
    game.power.dev_set_shield(5);
    game.player = Player::new(0x16, 0x49);
    game.objects[9] = object(0x17, 0xF0, 0x60);
    game.objects[9].sprite = 0;
    let mut core = game.objects[9];
    game.ai_miniboss(&mut core);
    assert_eq!(core.kind, 0x97);
    assert_eq!(core.subtype, 0x80);
    assert_eq!(core.size_class, 1);
    assert_eq!(core.sprite, 0x3F, "`$B791[$048C]`");
    assert_eq!((core.x.int, core.x.frac), (0xEE, 0xF0), "1 + `$B789`/256");
    assert_eq!((core.y.int, core.y.frac), (0x5F, 0x90), "`$B793`/256");
    assert_eq!(core.ai_state, 0);
    assert_eq!(core.y_vel >> 8, 1, "`$03BC` counts the frame");

    // `$B781[$17] = $32`: `$048C` turns on on the 51st frame at 20758.
    for _ in 0..49 {
        game.ai_miniboss(&mut core);
    }
    assert_eq!(core.y_vel >> 8, 50);
    assert_eq!(core.ai_state, 0);
    game.ai_miniboss(&mut core);
    assert_eq!(core.ai_state, 1, "`$B854` toggles `$048C`");
    assert_eq!(core.sprite, 0x3F, "the sprite follows `$048C` next frame");
    assert_eq!(core.y_vel >> 8, 1);

    // Off again 26 frames later (the extra INC while `$048C != 0`).
    for _ in 0..26 {
        game.ai_miniboss(&mut core);
    }
    assert_eq!(core.ai_state, 0);
    assert_eq!(core.sprite, 0x40, "the last on-frame still drew `$40`");
    game.ai_miniboss(&mut core);
    assert_eq!(core.sprite, 0x3F);
}

#[test]
fn the_small_core_death_scores_three_hundred_and_becomes_cb20_debris() {
    let mut game = sandbox();
    game.section = 2;
    game.power.dev_set_shield(5);
    game.player = Player::new(0x16, 0x49);
    game.objects[9] = object(0x97, 0x40, 0x49);
    game.objects[9].subtype = 0x80;
    game.objects[9].sprite = 0x3F;
    game.objects[9].ai_state = 0;
    game.objects[9].ai_timer = tables::CORE_HITS[2];
    let mut core = game.objects[9];
    let x = core.x.int;
    game.ai_miniboss(&mut core);
    assert_eq!(game.score, 300, "`$8454`");
    assert_eq!(core.kind, 2);
    assert_eq!(core.anim, 2, "`$CB20` uses debris path 2");
    assert_eq!(core.anim_div, 0, "the first debris step is immediate");
    assert_eq!(core.ai_timer, 0);
    assert_eq!(core.sprite, 0x3F, "`$CB20` keeps `$012C`");
    assert_eq!(core.ai_state, 0);
    assert_eq!(core.x.int, x - 2, "the death frame still steps X");
}

#[test]
fn turret_death_writes_the_explosion_and_collision_patch() {
    let mut game = sandbox();
    game.section = 2;
    game.scroll.high = 3;
    game.scroll.low = 0x50;
    game.scroll.fraction = 0;
    let mut turret = stage_three_turret(0x2323, 0x80, 0xA0, 0x38);
    turret.ai_timer = 3;
    game.ai_background_turret(&mut turret);
    assert_eq!(turret.kind, 2, "kind-2 debris");
    assert_eq!(game.cells.len(), 24, "the type-0 `$C7B0` stream");
    assert_eq!(
        game.cells.last(),
        Some(&stage3_enemies::Cell {
            world_col: 136,
            row: 26,
            tile: 0xB8
        })
    );
    assert_eq!(game.collision_cells.len(), 10, "`$C81C`");
    assert_eq!(game.collision_cells[0].addr, 0x051B);
    assert_eq!(game.collision_cells[0].value, 0x0F);
}

#[test]
fn the_big_core_uses_the_armour_frames_and_fires_straight_left() {
    let mut game = sandbox();
    game.spawn_boss();
    assert_eq!(game.objects[9].sprite, 0x6C, "the `$046C = 0` frame");
    assert_eq!(game.objects[7].sprite, 0x32, "lower cover");
    assert_eq!(game.objects[8].sprite, 0x85, "upper cover");

    // `$B941` steps `$046C` into `$B8E9` as the armour opens.
    game.objects[9].x = Fixed::px(0xA3);
    game.objects[9].ai_state = 1;
    game.objects[9].ai_timer = 1;
    let mut core = game.objects[9];
    game.ai_big_core(&mut core);
    assert_eq!(core.sprite, 0x6D);

    // `$BA6A`: four pods, sprite group `$41`, straight left at `$0340`.
    game.power.dev_set_shield(5);
    game.section = 1;
    let mut core = game.objects[9];
    core.x = Fixed::px(0xA3);
    core.y = Fixed::px(0x50);
    core.ai_timer = 0;
    core.ai_mode = 0;
    core.timer2 = 70; // rank 3 -> `$B904[3] = $46`
    game.enemy_bullet_count = 0;
    game.ai_big_core(&mut core);
    assert_eq!(game.enemy_bullet_count, 4);
    for bullet in &game.enemy_bullets[..4] {
        assert_eq!(bullet.sprite, 0x41);
        assert_eq!((bullet.vx, bullet.vy), (-0x340, 0));
    }
}

#[test]
fn the_big_core_enters_to_x_164_and_dies_to_six_hits() {
    let mut game = sandbox();
    game.spawn_boss();
    assert!(game.objects[9].boss);
    assert_eq!(game.objects[9].kind, 0x98);
    assert_eq!(game.objects[9].x.int, 0xF0);
    assert_eq!(game.objects[9].y.int, 0x80);
    // `$B91F` writes `$0460,X = 3`, so the core uses the 16x2 box.
    assert_eq!(game.objects[9].size_class, 3);
    // `$B9A5`: one pixel per frame until the recorded stop at x = 164.
    for _ in 0..300 {
        game.update_objects();
        if game.objects[9].x.int <= 0xA4 {
            break;
        }
    }
    assert_eq!(game.objects[9].x.int, 0xA4);
    game.update_objects();
    assert_eq!(game.objects[9].x.int, 0xA3, "entry stops");
    game.update_objects();
    assert_eq!(game.objects[9].x.int, 0xA3, "entry stays stopped");
    assert!(game.scroll_stop, "the scroll freezes for the fight");
    // `$C06A`: the core only takes damage while `$048C != 0`.
    game.objects[9].ai_state = 1;
    for _ in 0..5 {
        game.shots[0] = Shot {
            active: true,
            kind: 0,
            x: game.objects[9].x,
            y: game.objects[9].y,
            age: 0,
        };
        game.shot_hit_object(0, 9);
        // The handler frame after the hit awards `$8460` (50).
        game.update_objects();
        assert!(game.objects[9].live());
    }
    assert_eq!(game.objects[9].ai_timer, 5);
    assert_eq!(game.score, 5 * 50, "five armour hits");
    // The sixth hit (`$046C = 6`) is the one `$B8E9[6] == 0` turns into
    // the `$B95C` death on the next handler frame.
    game.shots[0] = Shot {
        active: true,
        kind: 0,
        x: game.objects[9].x,
        y: game.objects[9].y,
        age: 0,
    };
    game.shot_hit_object(0, 9);
    assert_eq!(game.objects[9].ai_timer, 6);
    game.update_objects();
    assert_eq!(game.objects[9].kind, 2, "the core becomes debris");
    assert_eq!(game.objects[9].sprite, 1);
    assert_eq!(game.objects[9].ai_state, 1);
    assert_eq!(game.objects[9].anim, 4);
    assert_eq!(game.objects[9].anim_div, 0);
    assert_eq!(game.objects[9].x_vel, 0);
    assert_eq!(game.objects[9].ai_timer, 0);
    assert_eq!(game.objects[9].subtype, 0);
    assert_eq!(game.objects[9].phase, 0);
    assert_eq!(game.score, 5 * 50 + 1000);
    assert_eq!(game.boss_clear, 514);
}

#[test]
fn the_boss_waits_for_the_scroll_end_after_the_eruption() {
    let mut game = sandbox();
    game.scroll.high = 0x0C;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.boss_stage, 1, "the eruption starts at $3F = $0C");
    // `$82` sets `$5B` on the next frame, after that frame's half step.
    game.normal_frame(Buttons::NONE, false);
    assert!(game.scroll_locked, "the eruption holds the scroll");
    game.boss_timer = 1;
    game.normal_frame(Buttons::NONE, false);
    assert_eq!(game.boss_stage, 2);
    game.scroll.high = 0x0D;
    game.scroll.low = 0;
    game.normal_frame(Buttons::NONE, false);
    // `$84` freezes the scroll and spawns the core on the same frame, so
    // the oracle sees slot 9 at 7723 rather than 7724.
    assert!(game.boss_spawned);
    assert_eq!(game.boss_stage, 4);
    assert!(game.scroll_locked);
    assert_eq!(game.objects[9].kind, 0x98);
    assert!(game.objects[9].boss);
}

#[test]
fn the_core_climb_borrows_one_extra_256th_but_the_dive_does_not() {
    let mut game = sandbox();
    game.power.dev_set_shield(5); // `$46 = 5` keep-alive, rank 2
    game.objects[9] = Object {
        active: true,
        kind: 0x98,
        boss: true,
        x: Fixed::px(0xA3),
        y: Fixed { int: 100, frac: 0 },
        ai_state: 1,
        phase: 0,
        ..Object::default()
    };
    game.update_objects();
    let climb = game.objects[9].y;
    assert_eq!((climb.int, climb.frac), (98, 0xDF), "1 + $20 + 1 / 256 up");
    game.objects[9].timer2 = 0;
    game.objects[9].phase = 1;
    game.objects[9].y = Fixed { int: 100, frac: 0 };
    game.update_objects();
    let dive = game.objects[9].y;
    assert_eq!((dive.int, dive.frac), (101, 0x20), "1 + $20 / 256 down");
}
