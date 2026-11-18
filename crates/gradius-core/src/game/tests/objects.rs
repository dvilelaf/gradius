#[test]
fn the_dump_lists_the_real_object_arrays_not_the_shot_pool() {
    // `$A521` clears a slot at `$A8 + $0C`, so slot 7 is `$0307 + $0C`:
    // kind, `$036C` X, `$032C` Y and `$048C` state come from the object,
    // never from the player-shot pool.
    let mut game = sandbox();
    game.objects[7] = Object {
        ai_state: 3,
        ..object(0x85, 0x40, 0x50)
    };
    assert!(!game.shots.iter().any(|shot| shot.active));
    assert!(
        game.dump_state().contains("objects=7:133:64:80:3"),
        "{}",
        game.dump_state()
    );
    // A killed slot is invisible: `$BE8D` clears `$012C` as well as the
    // kind, and the oracle filters on it.
    game.objects[7].sprite = 0;
    assert!(
        game.dump_state().ends_with("objects="),
        "{}",
        game.dump_state()
    );
}

#[test]
fn debris_drifts_every_frame_and_takes_its_sprite_from_the_path() {
    // `$AE93` only steps the `$AE6B` stream every six frames, but the
    // `$AED7` drift runs on all of them.
    let mut game = sandbox();
    let mut wreck = object(0x85, 0x60, 0x50);
    game.destroy(&mut wreck, 0x05);
    assert_eq!(wreck.kind, 2);
    assert_eq!(wreck.sprite, 0, "`$BE8D` clears `$012C`");
    assert_eq!(wreck.anim, 0, "kind 5 takes the `$016C = 0` path");
    let start = wreck.x.int;
    game.objects[5] = wreck;
    game.update_objects();
    assert_eq!(game.objects[5].x.int, start - 1, "drift on a divider frame");
    assert_eq!(game.objects[5].sprite, 0, "`$014C = 3` holds the sprite");
    for _ in 0..3 {
        game.update_objects();
    }
    assert_eq!(game.objects[5].sprite, 0x26, "the first path frame");
    assert!(game.objects[5].x.int < start - 1);
}

#[test]
fn only_the_last_cannon_part_folds_into_the_background() {
    // `$BE9F` leaves `$03AC = 0` while the class still has members; the
    // last one keeps `$03AC = 1`, and `$AED1` then turns the wreck into a
    // kind-1 background piece instead of despawning it.
    let mut game = sandbox();
    let mut last = object(0x85, 0x40, 0x40);
    last.phase = 1;
    game.destroy(&mut last, 0x05);
    assert_eq!(last.phase, 1, "`$BEAF` keeps the last member at 1");
    last.anim_div = 0;
    last.x_vel = 3; // the path terminator
    let mut wreck = last;
    game.ai_debris(&mut wreck);
    assert_eq!(wreck.kind, 1, "the wreck folds into the background");
    assert!(wreck.live());

    let mut plain = object(0x85, 0x40, 0x40);
    game.destroy(&mut plain, 0x05);
    plain.anim_div = 0;
    plain.x_vel = 3;
    game.ai_debris(&mut plain);
    assert!(!plain.live(), "without `$03AC` the wreck despawns");
}

#[test]
fn a_missile_falls_and_drifts_then_rides_after_hitting_the_ground() {
    let mut game = sandbox();
    // `$19 = 4` skips the background probe in the boss section, so the
    // free-flight path is observable on its own.
    game.section = 4;
    game.shots[0] = Shot {
        active: true,
        kind: 3,
        x: Fixed::px(0x40),
        y: Fixed::px(0x40),
        age: 0,
    };
    game.update_shots();
    assert_eq!(game.shots[0].y.int, 0x42, "+2 px down");
    assert_eq!(game.shots[0].x.int, 0x40, "+0.5 px right, fraction");
    assert_eq!(game.shots[0].x.frac, 0x80);
    game.update_shots();
    assert_eq!(game.shots[0].x.int, 0x41, "the fraction carries");
    assert_eq!(game.shots[0].x.frac, 0);
    assert_eq!(game.shots[0].y.int, 0x44);

    // A solid forward probe switches to the `$61` ground-ride frame; the
    // shifted (up-right) probe is clear, so the missile survives.
    game.section = 0;
    // The extracted collision map is empty at scroll 0; world column 128
    // (x 1024) is solid, so bring it to the left edge.
    game.scroll.high = 4;
    game.scroll.low = 0;
    game.scroll.fraction = 0;
    let mut spot = None;
    for y in 0..0xB0u8 {
        for x in 0..0xE0u8 {
            if game.solid_at(x, y) && !game.solid_at(x + 8, y.saturating_sub(8)) {
                spot = Some((x, y));
                break;
            }
        }
        if spot.is_some() {
            break;
        }
    }
    let (x, y) = spot.expect("stage 1 has a solid spot with a clear probe");
    game.shots[0] = Shot {
        active: true,
        kind: 3,
        x: Fixed::px(x),
        y: Fixed::px(y),
        age: 0,
    };
    game.update_shots();
    assert_eq!(game.shots[0].age, 1, "ground-ride frame");
    assert_eq!(game.shots[0].x.int, x + 2, "+2 px right");
    assert_eq!(game.shots[0].y.int, y, "no vertical motion while riding");

    // `$0329 >= $C8` frees the slot.
    game.section = 4;
    game.shots[0] = Shot {
        active: true,
        kind: 3,
        x: Fixed::px(0x40),
        y: Fixed::px(0xC6),
        age: 0,
    };
    game.update_shots();
    assert!(!game.shots[0].active);
}

#[test]
fn a_zero_animation_frame_never_blanks_the_sprite() {
    // `$ADF7` walks `$016C` past a `$00` table entry instead of storing it.
    let mut game = sandbox();
    game.objects[4] = Object {
        subtype: 6,
        anim: 6, // index 6 * 4 + 2 = 26 (0x12), next stop is 27 (0x00)
        anim_div: 0,
        ..object(0x85, 0x40, 0x40)
    };
    game.update_objects();
    assert_eq!(game.objects[4].sprite, 0x10, "the zero entry is skipped");
}

#[test]
fn a_level_seeker_takes_the_double_left_step() {
    // `$B283`/`$B2D5`: level with the ship the seeker skips `$036C`'s
    // decrement and takes the `$FE` X step, so it closes at 2 px/frame.
    let mut game = sandbox();
    game.player = Player::new(0x80, 0x30);
    let mut seeker = object(0x88, 0x60, 0x30);
    seeker.ai_state = 0x80;
    let start = seeker.x.int;
    game.ai_seeker(&mut seeker);
    assert_eq!(seeker.x.int, start - 2);
    assert_eq!(seeker.sprite, 0x3A);
    assert_eq!(seeker.y.int, 0x30, "level, so no vertical step");
}

#[test]
fn a_folded_wreck_becomes_the_collectible_capsule() {
    // `$AEBB`: terminal debris with `$03AC != 0` becomes kind 1, subtype 6
    // (7 on every sixteenth fold). `$C1A9` then advances `$42` on touch.
    let mut game = sandbox();
    game.debris_folds = 0;
    game.objects[9] = Object {
        active: true,
        kind: 2,
        anim: 1,
        anim_div: 0,
        x_vel: i16::MAX,
        phase: 1,
        x: game.player.x,
        y: game.player.y,
        ..Object::default()
    };
    game.update_objects();
    assert_eq!(game.objects[9].kind, 1);
    assert_eq!(game.objects[9].subtype, 6);

    game.collide();
    assert_eq!(game.power.selection(), 1);
    assert_eq!(game.power.capsules(), 1);
    assert!(!game.objects[9].active, "the capsule is consumed");
}

#[test]
fn the_sixteenth_fold_is_not_a_capsule() {
    let mut game = sandbox();
    game.debris_folds = 15;
    game.objects[9] = Object {
        active: true,
        kind: 2,
        anim: 1,
        anim_div: 0,
        x_vel: i16::MAX,
        phase: 1,
        x: game.player.x,
        y: game.player.y,
        ..Object::default()
    };
    game.update_objects();
    assert_eq!(game.objects[9].kind, 1);
    assert_eq!(game.objects[9].subtype, 7);

    game.collide();
    assert_eq!(game.power.selection(), 0, "not a pickup");
    assert_eq!(game.power.capsules(), 0);
    assert!(!game.objects[9].live(), "`$C1F7` clears the blue capsule");
}

#[test]
fn the_blue_capsule_clears_the_screen_without_scoring() {
    let mut game = sandbox();
    // `$C186`: the fold that lands on subtype 7 is the blue capsule.
    game.objects[0] = Object {
        active: true,
        kind: 1,
        subtype: 7,
        x: game.player.x,
        y: game.player.y,
        sprite: 1,
        ..Object::default()
    };
    game.objects[4] = object(0x85, 0xB0, 0x50);
    game.objects[7] = object(0x85, 0xD0, 0x50);
    // `$010C` bit 7, `$030C` bit 7 clear and low kind below 3 all skip `$BE8D`.
    game.objects[5] = Object {
        subtype: 0x81,
        ..object(0x85, 0xC0, 0x50)
    };
    game.objects[6] = object(0x05, 0xC8, 0x50);
    game.objects[8] = object(0x82, 0xD8, 0x50);
    let score = game.score;
    game.collide();
    assert!(!game.objects[0].live(), "the capsule slot is emptied");
    for slot in [4, 7] {
        assert_eq!(game.objects[slot].kind, 2, "slot {slot} is `$BE8D` debris");
        assert_eq!(game.objects[slot].sprite, 0);
        assert_eq!(game.objects[slot].anim, 0, "kind 5 takes the 0 path");
    }
    assert_eq!(game.objects[5].kind, 0x85);
    assert_eq!(game.objects[5].subtype, 0x81);
    assert_eq!(game.objects[6].kind, 0x05);
    assert_eq!(game.objects[8].kind, 0x82);
    assert_eq!(game.score, score, "`$BE8D` never calls `$8468`");
    assert_eq!(
        game.audio.drain().collect::<Vec<_>>(),
        [Sound::ScreenClear, Sound::EnemyHit, Sound::EnemyHit],
        "`$0B` first, then `$BE8D` from slot 9 down to 0"
    );
}

#[test]
fn a_kind_one_object_without_a_subtype_is_inert() {
    // `$C17D`: `LDA $010C,Y; BEQ` skips kind 1 before the capsule tests.
    let mut game = sandbox();
    game.objects[0] = Object {
        active: true,
        kind: 1,
        x: game.player.x,
        y: game.player.y,
        sprite: 1,
        ..Object::default()
    };
    game.collide();
    assert!(game.objects[0].live(), "no subtype means no collision");
    assert_eq!(game.objects[0].kind, 1);
    assert!(game.audio.is_empty());
}

#[test]
fn player_two_blows_the_blue_capsule_for_both_ships() {
    let mut game = simultaneous();
    game.objects[0] = Object {
        active: true,
        kind: 1,
        subtype: 7,
        x: game.p2.player.x,
        y: game.p2.player.y,
        sprite: 1,
        ..Object::default()
    };
    game.objects[4] = object(0x85, 0xB0, 0x50);
    game.objects[7] = object(0x85, 0xD0, 0x50);
    game.with_ship(1, |game| game.collide());
    assert!(!game.objects[0].live(), "P2's pickup clears the capsule");
    assert_eq!(game.objects[4].kind, 2);
    assert_eq!(game.objects[7].kind, 2);
    let sounds = game.audio.drain().collect::<Vec<_>>();
    assert_eq!(sounds.first(), Some(&Sound::ScreenClear));
}

#[test]
fn a_kill_plays_the_measured_fifteen_frame_burst() {
    let mut game = sandbox();
    game.add_explosion(100, 100, 15);
    let expected: [&[u8]; 3] = [&[0x80], &[0x82], &[0x84]];
    for step in 0..15 {
        assert_eq!(
            burst_tiles(&game),
            expected[step / 5],
            "frame {step} of burst A"
        );
        game.update_explosions();
    }
    assert_eq!(game.explosion_count, 0, "the burst lasts exactly 15 frames");
}

#[test]
fn wall_freezes_only_after_its_x_drops_below_b0() {
    // `$B563`: `$AED7` drifts the wall left every frame; `$5B` is raised
    // only once the integer X drops below `$B0`. `$046C` then counts a
    // 20-frame step from 1, and at 7 the wall despawns (the despawn frame
    // still freezes).
    let mut game = sandbox();
    let mut wall = object(0x1E, 0xB0, 0x40);
    wall.x = Fixed {
        int: 0xB0,
        frac: 0x80,
    };
    game.ai_wall(&mut wall);
    assert!(!game.scroll_stop, "X = $B0 is still on the threshold");
    assert_eq!(wall.ai_timer, 0);

    game.scroll_stop = false;
    game.ai_wall(&mut wall);
    assert!(game.scroll_stop, "X = $AF:$80 freezes the scroll");
    assert_eq!(wall.ai_timer, 1);

    for _ in 0..6 {
        for _ in 0..0x14 {
            game.scroll_stop = false;
            game.ai_wall(&mut wall);
            assert!(game.scroll_stop);
            assert!(wall.live());
        }
    }
    assert_eq!(wall.ai_timer, 7);

    game.scroll_stop = false;
    game.ai_wall(&mut wall);
    assert!(game.scroll_stop, "the despawn frame still freezes");
    assert!(!wall.live(), "the wall despawns at $046C = 7");
}

#[test]
fn scout_enters_and_tracks_the_ship_y() {
    // `$B0A9`: type `$80` enters at X=`$F0`, drifts left 2 px/frame until
    // X < `$60`, then homes in Y at 2 px/frame.
    let mut game = sandbox();
    let mut scout = object(0x05, 0xF0, 0x2A);
    game.ai_scout(&mut scout);
    assert_eq!(scout.kind & 0x80, 0x80, "first frame sets the init flag");
    assert_eq!(scout.x.int, 0xF0);
    for _ in 0..100 {
        game.ai_scout(&mut scout);
        if !scout.active {
            break;
        }
    }
    assert!(scout.ai_state >= 1, "entered the homing phase");
    assert!(
        scout.ai_timer > 0 && scout.ai_timer <= 0x40,
        "64-frame homing budget, got {}",
        scout.ai_timer
    );
}

#[test]
fn descender_writes_the_b622_sprite_every_eight_frames() {
    // `$B4F7`: the init frame only flags the kind; `$B622` (Y = 3) then
    // counts the `$014C` divider to 8, steps `$016C` mod 8 and writes
    // `sprite = $4A + anim`, so the first write is `$4B`.
    let mut game = sandbox();
    let mut descender = object(0x1C, 0xE8, 0x2A);
    descender.sprite = 0;
    game.ai_descender(&mut descender);
    assert_eq!(descender.kind, 0x9C, "the init frame sets the handler bit");
    assert_eq!(descender.sprite, 0, "`$B622` waits for the first step");
    for _ in 0..7 {
        game.ai_descender(&mut descender);
    }
    assert_eq!(descender.sprite, 0, "seven divider steps");
    assert_eq!(descender.x.int, 0xE8 - 7, "X decrements every live frame");
    game.ai_descender(&mut descender);
    assert_eq!(descender.anim_div, 0, "the divider resets at eight");
    assert_eq!(descender.anim, 1);
    assert_eq!(descender.sprite, 0x4B, "$4A + the first `$016C` step");
}

#[test]
fn kind_four_arc_steps_the_decoded_half_pixel_acceleration() {
    // `$B1FF`: `$048C = $20` (0.5 px/frame²), Y velocity starts at 2.0.
    let mut game = sandbox();
    let mut arc = object(0x04, 0x80, 0x60);
    game.ai_arc(&mut arc, 2, false);
    assert_eq!(arc.y_vel, 0x0200);
    assert_eq!(arc.ai_state, 0x20);
    let y = arc.y.int;
    game.ai_arc(&mut arc, 2, false);
    assert_eq!(arc.y.int, y - 2, "the first frame rises 2 px");
    assert_eq!(arc.y_vel, 0x01E0, "velocity loses 0.5 px/frame");
}

#[test]
fn heavy_arc_starts_at_three_pixels_per_frame() {
    let mut game = sandbox();
    let mut arc = object(0x06, 0x80, 0x60);
    game.ai_arc(&mut arc, 3, true);
    assert_eq!(arc.y_vel, 0x0300);
    assert_eq!(arc.x_vel, -0x200);
}

#[test]
fn kind_thirteen_arc_starts_at_two_pixels_and_restarts_at_minus_two() {
    // `$B3FC`: the init seeds `$042C = $FE` (-2 px/frame) with Y velocity
    // 2.0. `$03BC` is the Y velocity's high byte, so the cycle restarts
    // (with no movement that frame) once it reads `$FD`, i.e. just below
    // -2 px/frame. The stage-3 `$8F` record spawns this object.
    let mut game = sandbox();
    let mut arc = object(0x0D, 0xF0, 0x60);
    game.ai_arc_toggle(&mut arc, 0x0D);
    assert_eq!(arc.kind, 0x8D);
    assert_eq!(arc.x_vel, -0x200);
    assert_eq!(arc.y_vel, 0x0200);
    assert_eq!(arc.x.int, 0xF0, "the init frame does not move");
    game.ai_arc_toggle(&mut arc, 0x0D);
    assert_eq!((arc.x.int, arc.y.int), (0xEE, 0x5E), "two pixels a frame");
    assert_eq!(arc.y_vel, 0x01E0);
    for _ in 0..32 {
        game.ai_arc_toggle(&mut arc, 0x0D);
    }
    assert_eq!((arc.x.int, arc.y.int), (0xAE, 0x60));
    let before = (arc.x.int, arc.y.int);
    game.ai_arc_toggle(&mut arc, 0x0D);
    assert_eq!((arc.x.int, arc.y.int), before, "the restart frame is still");
    assert_eq!(arc.ai_mode, 1);
    assert_eq!(arc.y_vel, 0x0200, "the cycle re-arms at 2 px/frame");
}

#[test]
fn turret_fires_on_the_gate_without_breaking_up() {
    // `$AF28`/`$AF92`: init, then a kind-9 shell on a `$02 & $0F == 0`
    // frame while `x >= $C8`; the handler only drifts and never destroys
    // the turret.
    let mut game = sandbox();
    game.objects = [Object::default(); SLOT_COUNT];
    game.objects[9] = object(0x0F, 0xF0, 0x80);
    game.nmi_frame = 0; // $02 & 0x0F == 0
    game.update_objects();
    assert_eq!(game.objects[9].kind & 0x7F, 0x0F, "init keeps the kind");
    game.nmi_frame = 0x10; // next `$02 & 0x0F == 0`
    game.update_objects();
    assert!(game.objects.iter().any(|o| o.live() && o.kind & 0x7F == 9));
    for _ in 0..6 {
        game.update_objects();
    }
    assert_eq!(
        game.objects[9].kind & 0x7F,
        0x0F,
        "the turret never self-destructs"
    );
}

#[test]
fn the_stage_two_launcher_fires_one_group_25_shot() {
    let mut game = sandbox();
    game.player.x = Fixed::px(0x50);
    game.player.y = Fixed::px(0x50);
    game.objects[9] = Object {
        active: true,
        kind: 0x8B,
        sprite: 0x67,
        ai_state: 0,
        x: Fixed::px(0x80),
        y: Fixed::px(0x60),
        ..Object::default()
    };
    let mut launcher = game.objects[9];
    game.ai_launcher(&mut launcher);
    assert_eq!(game.enemy_bullet_count, 1);
    assert_eq!(game.enemy_bullets[0].sprite, 0x25, "`$8F45` group");
    assert_eq!(launcher.ai_state, 1, "`$B3B6` closes the one-shot gate");
    game.ai_launcher(&mut launcher);
    assert_eq!(game.enemy_bullet_count, 1, "the launcher fires once");
}

#[test]
fn the_aimed_move_despawns_outside_the_y_window() {
    // `$BE5C`/`$BE60`: after the move, Y < $08 or Y >= $C4 despawns.
    let mut game = sandbox();
    let moved = |game: &mut Game, y: u8, y_vel: i16| {
        let mut object = Object {
            active: true,
            kind: 0x8B,
            x: Fixed::px(0x50),
            y: Fixed::px(y),
            y_vel,
            ..Object::default()
        };
        game.move_aimed(&mut object);
        object
    };
    // A full `$100` step from 9 lands on 8 and survives; from 8 it lands
    // on 7 and despawns.
    let landed = moved(&mut game, 9, -0x100);
    assert!(landed.active);
    assert_eq!(landed.y.int, 8);
    assert!(!moved(&mut game, 8, -0x100).active);
    // The integer window is `8..$C4`.
    assert!(!moved(&mut game, 7, 0).active);
    assert!(moved(&mut game, 0xC3, 0).active);
    assert!(!moved(&mut game, 0xC4, 0).active);
}

#[test]
fn the_object_fire_pass_counts_down_and_fires_one_muzzled_bullet() {
    // `$BBB1`/`$BC3E`: stage 3 (rank 2), odd `$02` so `$040C` drops by 1.
    let mut game = sandbox();
    game.section = 2;
    game.power.dev_set_shield(5);
    game.nmi_frame = 1;
    game.player.x = Fixed::px(0x50);
    game.player.y = Fixed::px(0x60);
    let mut scout = object(0x85, 0xB2, 0x2A);
    scout.fire_timer = 0;
    scout.fire_motion = 0x6E;
    game.objects[9] = scout;
    let mut lower = object(0x85, 0xB2, 0x2A);
    lower.fire_timer = 0;
    lower.fire_motion = 0x6E;
    game.objects[8] = lower;

    game.update_object_fire();
    assert_eq!(game.enemy_bullet_count, 1, "one object fires per frame");
    let bullet = game.enemy_bullets[0];
    assert_eq!((bullet.x >> 8, bullet.y >> 8), (0xB2, 0x2A));
    assert_eq!((bullet.sprite, bullet.kind), (0x25, 0));
    // `$BD59`: the major axis is 1.25 px/frame and the minor axis is
    // `q + q/4 + 1`, with the bullet to the ship's right and above it.
    assert_eq!(bullet.vx, -0x140);
    assert_eq!(bullet.vy, 0xAD);
    assert_eq!(game.objects[9].fire_timer, 0x6E, "`$BC03` reloads `$04EC`");
    assert_eq!(
        game.objects[8].fire_timer, 0,
        "`$BC13` stops the scan at the first fire"
    );
    assert!(game.spawn_busy, "`$83BA` raises `$5D`");

    // `$0496` selects the muzzle offset: index 2 is `(+8, -8)`.
    game.enemy_bullet_count = 0;
    game.spawn_busy = false;
    game.objects[9].fire_timer = 0;
    game.objects[9].muzzle = 2;
    game.update_object_fire();
    assert_eq!(game.enemy_bullet_count, 1);
    let bullet = game.enemy_bullets[0];
    assert_eq!((bullet.x >> 8, bullet.y >> 8), (0xBA, 0x22));
}

#[test]
fn aiming_turrets_keep_the_turret_sprite_group() {
    // `$B067` writes `$B080,Y` to `$012C`; the `$B086`/`$B08C` values go
    // to `$0496`, the muzzle direction, and must not replace the sprite.
    let mut game = sandbox();
    game.player.x = Fixed::px(0x50);
    game.player.y = Fixed::px(0x50);
    game.objects[9] = object(0x12, 0x60, 0x40);
    game.objects[9].phase = 0;
    for kind in [0x11u8, 0x12] {
        game.objects[9].kind = kind;
        let mut turret = game.objects[9];
        game.ai_turret(&mut turret, kind);
        // dx = $10 (inside $30 -> sector 0); the near vertical does not
        // adjust a near sector.
        assert_eq!(turret.sprite, 0x74, "kind ${kind:02X}");
        // `$B020` pins `$040C` above the ship for `$91`; `$B092` does it
        // below instead, so only the `$91` turret matches this geometry.
        assert_eq!(
            turret.fire_timer,
            if kind == 0x11 { 0x0A } else { 0 },
            "kind ${kind:02X} pin"
        );
        // `$B086[0] = 1`; the `$92` handler sets `$018C` bit 7, so it
        // takes `$B08C[0] = 3`.
        assert_eq!(
            turret.muzzle,
            if kind == 0x11 { 1 } else { 3 },
            "kind ${kind:02X} muzzle"
        );
        // dx = $70 (far right) and dy = -$10 adds the vertical step.
        turret.x = Fixed::px(0xC0);
        game.ai_turret(&mut turret, kind);
        assert_eq!(turret.sprite, 0x72, "kind ${kind:02X} far");
        assert_eq!(turret.muzzle, 6, "kind ${kind:02X} far muzzle");
    }
}

#[test]
fn an_aiming_turret_keeps_its_cannon_phase_for_the_capsule_rule() {
    // `$B09A` writes bit 7 of `$018C`, not `$03AC`. The port used to OR it
    // into the cannon phase, which `$BE9F` then read as a class index: the
    // 21526 wreck despawned instead of folding into a capsule.
    let mut game = sandbox();
    game.player.x = Fixed::px(0x50);
    game.player.y = Fixed::px(0x50);
    game.objects[9] = object(0x12, 0x60, 0x40);
    game.objects[9].phase = 1;
    let mut turret = game.objects[9];
    game.ai_turret(&mut turret, 0x12);
    assert_eq!(turret.phase, 1, "`$03AC` is untouched");
    game.turn_to_debris(&mut turret);
    assert_eq!(turret.phase, 1, "`$BE8D` keeps the phase");
}

#[test]
fn drift_holds_during_the_boss_scroll_hold() {
    // `$AED7: LDA $5B; BNE` - the `$82` hold sets the port's
    // `scroll_locked`, so the debris must not slide left.
    let mut game = sandbox();
    let mut piece = object(0x02, 0x80, 0x60);
    game.scroll_locked = true;
    game.ai_drift(&mut piece);
    game.ai_drift(&mut piece);
    assert_eq!(piece.x.int, 0x80);
    game.scroll_locked = false;
    game.ai_drift(&mut piece);
    game.ai_drift(&mut piece);
    assert!(piece.x.int < 0x80);
}

#[test]
fn dump_state_has_the_stable_harness_format() {
    let mut game = sandbox();
    game.objects[9] = object(0x85, 0x40, 0x50);
    game.objects[9].ai_state = 3;
    // Slots 3..=9 mirror the oracle's shot-pool view (`$0123-$0129`).
    game.shots[6] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x40),
        y: Fixed::px(0x50),
        age: 0,
    };
    let line = game.dump_state();
    assert!(
        line.starts_with(
            "frame=0 state=Playing px=80 pf=0 py=96 pyf=0 speed=0 scroll=0:0:0 score=0 lives=3 capsule=0 meter=0 objects="
        ),
        "{line}"
    );
    assert!(line.ends_with("9:133:64:80:3"), "{line}");
    assert!(!line.contains('\n'));
    // A second live object is separated by `;`, ascending slot order.
    game.objects[8] = object(0x05, 0x50, 0x50);
    game.shots[5] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x50),
        y: Fixed::px(0x50),
        age: 0,
    };
    let line = game.dump_state();
    assert!(line.contains("8:5:80:80:0;9:133:64:80:3"), "{line}");
}

#[test]
fn destroy_turns_an_object_into_kind_two_debris() {
    let mut game = sandbox();
    let mut target = object(0x85, 0x60, 0x60);
    let score = game.score;
    game.destroy(&mut target, 0x05);
    assert_eq!(target.kind, 2);
    assert_eq!(target.anim_div, 3);
    assert!(game.score > score);
    assert_eq!(game.kills, 1);
}

#[test]
fn a_shot_killed_kind_1a_pays_ten_points() {
    // `$C0A0` always calls `$8468`: the five-hit kind `$9A` (`$C08A`) is an
    // ordinary 10-point kill, not the `$8454` 300 of the small core.
    let mut game = sandbox();
    game.score_bcd = Bcd::default();
    game.score = 0;
    let mut spin = object(0x9A, 0x60, 0x60);
    game.destroy(&mut spin, 0x0C);
    assert_eq!(game.score, 10);
}

#[test]
fn a_kind_1a_piece_aims_moves_and_walks_its_own_sprite() {
    // `$B47A`: the init frame only sets `$048C = 2` and the `$B4DE[$17]`
    // dwell; every later frame runs `$B622` with Y 6 (`$B64A[6]` divider,
    // sprite base `$B64B[6] = $42`) and the state machine. Before the fix the
    // piece never aimed, drifted or animated: `$012C` stayed 0, so it was
    // never drawn and its slot never freed.
    let mut game = sandbox();
    game.section = 5;
    game.objects[5] = object(0x1A, 0xF0, 0x40);
    game.update_objects();
    assert_eq!(game.objects[5].kind, 0x9A, "`$B0AE` adds the $80 bit");
    assert_eq!(game.objects[5].ai_state, 2, "`$B482`: `$048C = 2`");
    // Ten frames of `$B622` walk `$016C` to 1, so the sprite is `$42 + 1`.
    for _ in 0..10 {
        game.update_objects();
    }
    assert_eq!(game.objects[5].sprite, 0x43, "`$012C` follows `$B64B[6]`");
    // State 2 drifts (`$B4C2`) with the scroll and counts `$04CC` down.
    let x = game.objects[5].x.int;
    for _ in 0..4 {
        game.update_objects();
    }
    assert!(game.objects[5].x.int < x, "the piece drifts");
}

#[test]
fn the_one_up_pickup_becomes_a_drifting_kind_one_item() {
    // `$C137`: the slot keeps its position, turns into the `$A3` kind-1 item
    // and pays one life. `$07E5` bit 0 gates it: $16 (an even hundreds digit)
    // lets the pickup land.
    let mut game = sandbox();
    game.score_bcd = Bcd::from_decimal(1620);
    game.score = 1620;
    let lives = game.lives;
    game.objects[4] = object(0x27, 0x54, 0x60);
    game.collide_player_objects();
    assert_eq!(game.lives, lives + 1, "`$C14E`: `INC $20,X`");
    assert_eq!(game.score, 1620, "the 1UP pays no points");
    let item = game.objects[4];
    assert_eq!(item.kind, 1, "`$C144`: `$030C = 1`");
    assert_eq!(item.sprite, 0xA3, "`$C147`: `$012C = $A3`");
    assert!(item.live(), "the slot keeps drifting in the table");
}

#[test]
fn the_one_up_pickup_waits_while_the_07e5_bit_is_set() {
    // `$C137`: `LDA $07E5,X; LSR A; BCS $C130` skips the pickup while bit 0
    // of the score's `$07E5` byte is set; $15 is odd, so nothing happens.
    let mut game = sandbox();
    game.score_bcd = Bcd::from_decimal(1570);
    game.score = 1570;
    let lives = game.lives;
    game.objects[4] = object(0x27, 0x54, 0x60);
    game.collide_player_objects();
    assert_eq!(game.lives, lives, "no life on the odd hundred");
    assert_eq!(game.objects[4].kind, 0x27, "the pickup stays in the table");
}

#[test]
fn the_500_point_pickup_becomes_a_kind_one_item() {
    // `$C153`: `JSR $8450` pays 500 and `$012C = $A1`; the slot keeps going.
    let mut game = sandbox();
    game.score_bcd = Bcd::default();
    game.score = 0;
    game.objects[4] = object(0x29, 0x54, 0x60);
    game.collide_player_objects();
    assert_eq!(game.score, 500);
    let item = game.objects[4];
    assert_eq!(item.kind, 1);
    assert_eq!(item.sprite, 0xA1);
}

#[test]
fn the_meter_wrap_pays_a_thousand_when_07e5_holds_five() {
    // `$8950`: the `$42 = 7` wrap calls `$CE83`, which returns the low nibble
    // of the `$07E5` score byte; a 5 pays 1000 through `$845A` before the
    // capsule's own 50.
    let mut game = sandbox();
    game.score_bcd = Bcd::from_decimal(520);
    game.score = 520;
    game.power.select(6);
    game.objects[4] = Object {
        subtype: 6,
        ..object(1, 0x54, 0x60)
    };
    game.collide_player_objects();
    assert_eq!(game.score, 520 + 1000 + 50);
    assert_eq!(game.power.selection(), 1, "`$896A`: `$42 = 1`");
}

#[test]
fn background_pieces_hide_for_the_last_six_frames_of_the_cycle() {
    // `$AF0C`/`$AF20`: `$02 & $1F >= $1A` writes `$012C = 0`, so the slot is
    // live but not drawn (the oracle's dump hides it on those frames too).
    let mut game = sandbox();
    game.objects[5] = object(0x20, 0x60, 0x50);
    game.nmi_frame = 0x19;
    game.update_objects();
    assert_eq!(game.objects[5].sprite, 0x89, "the 26 drawn frames");
    game.nmi_frame = 0x1A;
    game.update_objects();
    assert_eq!(game.objects[5].sprite, 0, "the six hidden frames");
    assert!(game.objects[5].live(), "the piece stays live");
    game.nmi_frame = 0x00;
    game.update_objects();
    assert_eq!(game.objects[5].sprite, 0x89, "the next cycle draws it");
}
