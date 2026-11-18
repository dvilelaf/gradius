#[test]
fn the_option_trail_wraps_and_uses_the_rom_lag() {
    let mut trail = Trail::new(0x50, 0x60);
    for _ in 0..24 {
        trail.push(0x50, 0x60);
    }
    trail.push(0x60, 0x70);
    assert_eq!(trail.index, 1);
    // `$A2A3` subtracts the lag from the current index, so lag 0 is the
    // newest position and every positive lag is older.
    assert_eq!(trail.at(0), (0x60, 0x70));
    assert_eq!(trail.at(1), (0x50, 0x60));
    assert_eq!(trail.at(11), (0x50, 0x60));
}

#[test]
fn speed_and_clamps_match_the_measured_player() {
    let mut game = playing();
    game.phase = Phase::Normal;
    let start = game.player.x.int;
    game.latched_input = Buttons {
        right: true,
        ..Buttons::NONE
    };
    game.step(Buttons {
        right: true,
        ..Buttons::NONE
    });
    assert_eq!(game.player.x.int, start + 1, "speed level 0 is 1 px");

    game.power.select(1);
    game.power.activate(false);
    game.latched_input = Buttons {
        right: true,
        ..Buttons::NONE
    };
    game.step(Buttons {
        right: true,
        ..Buttons::NONE
    });
    // 1.5 px: the fraction is stored, so the integer moves every other frame.
    assert!(game.player.x.int > start);

    // Clamps are applied per axis every frame (`$A022`..`$A075`).
    let mut edge = playing();
    edge.phase = Phase::Normal;
    edge.player.x = Fixed::px(0xEF);
    edge.player.y = Fixed::px(0x11);
    edge.latched_input = Buttons {
        right: true,
        up: true,
        ..Buttons::NONE
    };
    edge.step(Buttons {
        right: true,
        up: true,
        ..Buttons::NONE
    });
    assert_eq!(edge.player.x.int, 0xF0);
    assert_eq!(edge.player.y.int, 0x10);
}

#[test]
fn fire_has_a_twenty_frame_cooldown_and_shots_run_at_seven_pixels() {
    let mut game = playing();
    game.phase = Phase::Normal;
    game.latched_input = Buttons {
        fire: true,
        ..Buttons::NONE
    };
    game.step(Buttons {
        fire: true,
        ..Buttons::NONE
    });
    let shot = game
        .shots
        .iter()
        .find(|shot| shot.active && shot.kind == 0)
        .copied()
        .expect("a shot");
    let x = shot.x.int;
    game.latched_input = Buttons {
        fire: true,
        ..Buttons::NONE
    };
    game.step(Buttons {
        fire: true,
        ..Buttons::NONE
    });
    let moved = game
        .shots
        .iter()
        .find(|shot| shot.active && shot.kind == 0)
        .unwrap();
    assert_eq!(moved.x.int, x + tables::SHOT_SPEED);
    // While the shot is in flight `$0123` is non-zero, so the original
    // skips the cooldown decrement entirely.
    assert_eq!(game.player.cooldown1[0], tables::FIRE_COOLDOWN);
}

#[test]
fn the_shot_table_matches_the_three_emitter_cartridge_layout() {
    // `$A102` walks `$0123..$012B`: three emitters, three entries each
    // (shot 1, shot 2, missile).
    let mut game = playing();
    assert_eq!(EMITTERS, 3);
    assert_eq!(MAX_SHOTS, 9);
    assert_eq!(game.shot_slot(0, 0), 0);
    assert_eq!(game.shot_slot(2, 0), 2);
    assert_eq!(game.shot_slot(0, 1), 3);
    assert_eq!(game.shot_slot(2, 1), 5);
    assert_eq!(game.shot_slot(0, 2), 6);
    assert!(game.is_missile_slot(6));
    assert!(!game.is_missile_slot(5));

    // The port-only four-option loadout keeps the five-emitter stride.
    game.set_mods(Mods {
        extra_options: true,
        ..Mods::default()
    });
    assert_eq!(game.shot_slot(0, 2), 10);
    assert_eq!(game.shot_slot(4, 2), 14);
    assert!(game.is_missile_slot(10));
}

#[test]
fn shots_despawn_at_the_documented_edge() {
    let mut game = playing();
    game.shots[0] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(tables::SHOT_DESPAWN - 1),
        y: Fixed::px(100),
        age: 0,
    };
    game.update_shots();
    assert!(!game.shots[0].active);
}

#[test]
fn touching_a_capsule_advances_the_meter() {
    let mut game = playing();
    let mut y = game.player.y;
    // `$C11C`'s borrow makes `dy == 0` a miss; one pixel up is `dy = 1`.
    y.add(7 << 8);
    // `$C1A9`: the power capsule is kind 1, subtype 6.
    game.objects[9] = Object {
        active: true,
        kind: 1,
        subtype: 6,
        x: game.player.x,
        y,
        ..Object::default()
    };
    game.collide();
    assert!(!game.objects[9].active);
    assert_eq!(game.power.selection(), 1);
    assert_eq!(game.power.capsules(), 1);
}

#[test]
fn b_activates_while_held() {
    let mut game = sandbox();
    let held = Buttons {
        power: true,
        ..Buttons::NONE
    };
    game.step(held);
    game.step(held);
    // A capsule arrives while B is already down: the held bit applies the
    // new selection and clears `$42` in the same frame.
    game.power.advance();
    game.step(held);
    assert_eq!(game.power.speed_level(), 1, "held B activates the item");
    assert_eq!(game.power.selection(), 0);

    for _ in 0..4 {
        game.step(held);
    }
    assert_eq!(game.power.speed_level(), 1, "still no repeat");
}

#[test]
fn mods_default_to_rom_parity() {
    let game = playing();
    assert_eq!(game.mods(), Mods::default());
    assert!(!game.mods().extra_options);
    assert_eq!(game.mods().hull_hits, 1);
    assert_eq!(game.mods().hit_invulnerable_frames, 0);
}

#[test]
fn one_hull_keeps_the_six_hundred_frame_stage_one_dump() {
    let mut game = Game::new(assets());
    game.dev_start_run();
    for _ in 0..600 {
        game.step(Buttons::NONE);
    }
    assert_eq!(
        game.dump_state(),
        "frame=600 state=Playing px=80 pf=0 py=96 pyf=0 speed=0 scroll=44:1:0 score=0 lives=3 capsule=0 meter=0 objects=4:133:216:42:0;5:133:194:42:0;6:133:161:92:3;7:133:194:92:3;8:133:227:92:3;9:133:238:42:0"
    );
}

#[test]
fn three_hull_points_survive_two_hits_and_die_on_the_third() {
    let mut game = sandbox();
    game.set_mods(Mods {
        hull_hits: 3,
        ..Mods::default()
    });
    game.place_player((0x40, 0x38));
    assert_eq!(game.player.hull, 3);

    for expected in [2, 1] {
        game.objects[9] = object(0x86, 0x40, 0x38);
        game.collide_player_objects();
        assert!(!game.player.dying, "hit leaves {expected} hull point(s)");
        assert_eq!(game.player.hull, expected);
    }

    game.objects[9] = object(0x86, 0x40, 0x38);
    game.collide_player_objects();
    assert!(game.player.dying, "the third hit is lethal");
}

#[test]
fn ninety_nine_hull_points_survive_ninety_eight_hits() {
    let mut game = sandbox();
    game.set_mods(Mods {
        hull_hits: 99,
        ..Mods::default()
    });
    game.place_player((0x40, 0x38));
    assert_eq!(game.player.hull, 99);

    for _ in 0..98 {
        game.objects[9] = object(0x86, 0x40, 0x38);
        game.collide_player_objects();
    }
    assert!(!game.player.dying, "98 hits leave one hull point");
    assert_eq!(game.player.hull, 1);

    game.objects[9] = object(0x86, 0x40, 0x38);
    game.collide_player_objects();
    assert!(game.player.dying, "the 99th hit is lethal");
}

#[test]
fn the_hit_window_blocks_exactly_the_configured_frames() {
    let (scroll, x, y) = solid_spot();
    let mut game = sandbox();
    game.set_mods(Mods {
        hull_hits: 3,
        hit_invulnerable_frames: 3,
        ..Mods::default()
    });
    game.scroll = scroll;
    game.place_player((x, y));

    game.collide_player_terrain();
    assert_eq!(game.player.hull, 2, "the first impact costs one point");
    for frame in 1..=3 {
        game.update_player(Buttons::NONE, false);
        game.collide_player_terrain();
        assert!(!game.player.dying);
        assert_eq!(game.player.hull, 2, "frame {frame} is inside the window");
    }
    game.update_player(Buttons::NONE, false);
    game.collide_player_terrain();
    assert_eq!(game.player.hull, 1, "the window has expired");
}

#[test]
fn the_shield_absorbs_before_the_hull_and_they_do_not_share_a_counter() {
    let mut game = sandbox();
    game.set_mods(Mods {
        hull_hits: 3,
        ..Mods::default()
    });
    game.place_player((0x40, 0x38));
    game.power.dev_set_shield(2);

    game.objects[9] = object(0x86, 0x40, 0x38);
    game.collide_player_objects();
    assert_eq!(game.power.shield_hits(), 1, "the shield takes the hit");
    assert_eq!(game.player.hull, 3, "the hull is untouched");
    assert_eq!(game.objects[9].kind, 2, "the shield destroys the object");
}

#[test]
fn option_cap_follows_the_extra_options_mod() {
    let mut parity = sandbox();
    collect_options(&mut parity);
    assert_eq!(parity.power.options(), crate::powerup::MAX_OPTIONS);

    let mut port = sandbox();
    port.set_mods(Mods {
        extra_options: true,
        ..Mods::default()
    });
    collect_options(&mut port);
    assert_eq!(port.power.options(), crate::powerup::MAX_OPTIONS_EXTENDED);
}

#[test]
fn capsule_advances_the_selection_and_press_b_applies_speed() {
    let mut game = sandbox();
    collect_capsules(&mut game, 1);
    assert_eq!(game.power.selection(), 1);
    assert_eq!(game.score, 50, "$8460 capsule score");
    let before = game.power.speed_delta();
    press_b(&mut game);
    assert_eq!(game.power.selection(), 0);
    assert_eq!(game.power.speed_level(), 1);
    assert!(game.power.speed_delta() > before);
    // Holding B must not re-apply the now-empty selection.
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    assert_eq!(game.power.speed_level(), 1);
}

#[test]
fn held_b_before_a_capsule_arrives_activates_on_the_pickup_frame() {
    let mut game = sandbox();
    // B is already held while `$42 == 0`, so there is no rising edge
    // left when the capsule advances the cursor.
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    assert_eq!(game.power.selection(), 0);

    place_capsule(&mut game);
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });

    assert_eq!(game.power.selection(), 0, "the pickup frame consumed $42");
    assert_eq!(
        game.power.speed_level(),
        1,
        "`$8979` reads held B after the collision"
    );
}

#[test]
fn held_b_activates_all_six_power_ups() {
    for slot in 1..=6u8 {
        let mut game = dev_playing();
        // Establish the hold before selecting, so only the held path can
        // activate the item.
        game.step(Buttons {
            power: true,
            ..Buttons::NONE
        });
        game.step(Buttons {
            power: true,
            ..Buttons::NONE
        });
        game.dev_power_mut().select(slot);
        game.step(Buttons {
            power: true,
            ..Buttons::NONE
        });

        assert_eq!(game.power.selection(), 0, "slot {slot} was not consumed");
        match slot {
            1 => assert_eq!(game.power.speed_level(), 1),
            2 => assert!(game.power.has_missile()),
            3 => assert!(game.power.has_double()),
            4 => assert!(game.power.has_laser()),
            5 => assert_eq!(game.power.options(), 1),
            6 => assert_eq!(game.power.shield_hits(), crate::powerup::SHIELD_HITS),
            _ => unreachable!(),
        }
    }
}

#[test]
fn held_b_does_not_repeat_a_consumed_selection() {
    let mut game = sandbox();
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    place_capsule(&mut game);
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    assert_eq!(game.power.speed_level(), 1);
    assert_eq!(game.power.selection(), 0);

    for _ in 0..120 {
        game.step(Buttons {
            power: true,
            ..Buttons::NONE
        });
    }
    assert_eq!(game.power.speed_level(), 1, "held B re-applied SPEED");
    assert_eq!(game.power.selection(), 0, "the empty cursor stays empty");
}

#[test]
fn held_b_does_not_activate_while_dying() {
    let mut game = sandbox();
    game.power.select(1);
    game.player.dying = true;
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    assert_eq!(game.power.speed_level(), 0, "`$0100 == 2` skips $8979");
    assert_eq!(game.power.selection(), 1, "the selection is not consumed");
}

#[test]
fn missile_is_a_one_shot_upgrade() {
    let mut game = sandbox();
    collect_capsules(&mut game, 2);
    press_b(&mut game);
    assert!(game.power.has_missile());
    assert_eq!(game.power.selection(), 0);
    collect_capsules(&mut game, 2);
    press_b(&mut game);
    assert!(game.power.has_missile());
    assert_eq!(game.power.selection(), 2, "a maxed handler is a no-op");
}

#[test]
fn double_and_laser_alternate() {
    let mut game = sandbox();
    collect_capsules(&mut game, 3);
    press_b(&mut game);
    assert!(game.power.has_double());
    collect_capsules(&mut game, 4);
    press_b(&mut game);
    assert!(game.power.has_laser());
    assert!(!game.power.has_double());
}

#[test]
fn options_stop_at_two() {
    let mut game = sandbox();
    collect_capsules(&mut game, 5);
    press_b(&mut game);
    assert_eq!(game.power.options(), 1);
    collect_capsules(&mut game, 5);
    press_b(&mut game);
    assert_eq!(game.power.options(), 2);
    collect_capsules(&mut game, 5);
    press_b(&mut game);
    assert_eq!(game.power.options(), 2);
    assert_eq!(game.power.selection(), 5, "the capped handler is a no-op");
}

#[test]
fn shield_is_a_one_shot_five_hit_upgrade() {
    let mut game = sandbox();
    collect_capsules(&mut game, 6);
    press_b(&mut game);
    assert_eq!(game.power.shield_hits(), crate::powerup::SHIELD_HITS);
    collect_capsules(&mut game, 6);
    press_b(&mut game);
    assert_eq!(game.power.shield_hits(), crate::powerup::SHIELD_HITS);
    assert_eq!(
        game.power.selection(),
        6,
        "an already-shielded ship is a no-op"
    );
}

#[test]
fn speed_up_raises_the_level_and_the_measured_ship_delta() {
    let mut game = dev_playing();
    let level = game.power.speed_level();
    let delta = game.power.speed_delta();
    select_and_activate(&mut game, 1);
    assert_eq!(game.power.speed_level(), level + 1);
    assert!(game.power.speed_delta() > delta);

    let start = fixed_px(game.player.x);
    for _ in 0..60 {
        game.step(Buttons {
            fire: true,
            right: true,
            ..Buttons::NONE
        });
    }
    let travelled = fixed_px(game.player.x) - start;
    // Level 1 is 1.5 px/frame; the one-frame input latch leaves ~88 px.
    assert!(
        travelled >= 80 * 256,
        "speed level 1 travelled only {} px in 60 frames",
        travelled / 256
    );
    for _ in 0..30 {
        game.step(Buttons {
            fire: true,
            ..Buttons::NONE
        });
    }
}

#[test]
fn missile_power_spawns_a_visible_missile_that_moves_down_right() {
    let mut game = dev_playing();
    select_and_activate(&mut game, 2);
    assert!(game.power.has_missile());

    let mut first = None;
    for _ in 0..90 {
        game.step(Buttons {
            fire: true,
            right: true,
            ..Buttons::NONE
        });
        if let Some(missile) = game.shots.iter().find(|shot| shot.active && shot.kind == 3) {
            first = Some(shot_px(missile));
            // Free flight uses `$8F58`/group `$0A` (tile `$65`); the
            // ground-ride frame is group 8 (tile `$61`).
            let (tile, dy) = if missile.age != 0 {
                (0x60, 9)
            } else {
                (0x64, 1)
            };
            let sprites = game.build_sprites();
            assert!(
                sprites.iter().any(|sprite| {
                    sprite.tile.index == tile
                        && sprite.x == i32::from(missile.x.int)
                        && sprite.y == i32::from(missile.y.int) + dy
                }),
                "the missile has no rendered sprite"
            );
            break;
        }
    }
    let (x0, y0) = first.expect("no missile was ever spawned");

    for _ in 0..6 {
        game.step(Buttons {
            fire: true,
            right: true,
            ..Buttons::NONE
        });
    }
    let missile = game
        .shots
        .iter()
        .find(|shot| shot.active && shot.kind == 3)
        .expect("the missile despawned before it could move");
    let (x1, y1) = shot_px(missile);
    assert!(x1 > x0, "the missile did not drift right");
    assert!(y1 > y0, "the missile did not fall");

    for _ in 0..84 {
        game.step(Buttons {
            fire: true,
            right: true,
            ..Buttons::NONE
        });
    }
}

#[test]
fn double_power_fires_a_straight_and_a_rising_shot() {
    let mut game = dev_playing();
    select_and_activate(&mut game, 3);
    assert!(game.power.has_double());

    let mut pair = None;
    for _ in 0..90 {
        game.step(Buttons {
            fire: true,
            ..Buttons::NONE
        });
        let straight = game
            .shots
            .iter()
            .find(|shot| shot.active && shot.kind == 0)
            .copied();
        let rising = game
            .shots
            .iter()
            .find(|shot| shot.active && shot.kind == 2)
            .copied();
        if let (Some(straight), Some(rising)) = (straight, rising) {
            pair = Some((straight, rising));
            break;
        }
    }
    let (straight, rising) = pair.expect("double never produced both shots");
    let sprites = game.build_sprites();
    assert!(
        sprites.iter().any(|sprite| {
            sprite.tile.index == 0x6A
                && sprite.x == i32::from(straight.x.int)
                && sprite.y == i32::from(straight.y.int) + 8
        }),
        "the straight `$6B` shot has no sprite"
    );
    assert!(
        sprites.iter().any(|sprite| {
            sprite.tile.index == 0x72
                && sprite.x == i32::from(rising.x.int) + 8
                && sprite.y == i32::from(rising.y.int) + 1
        }),
        "the rising `$73` shot has no sprite"
    );

    let (x0, y0) = (rising.x.int, rising.y.int);
    for _ in 0..5 {
        game.step(Buttons {
            fire: true,
            ..Buttons::NONE
        });
    }
    let rising = game
        .shots
        .iter()
        .find(|shot| shot.active && shot.kind == 2)
        .expect("the rising shot despawned too early");
    assert!(rising.x.int > x0, "the rising shot did not drift right");
    assert!(rising.y.int < y0, "the rising shot did not climb");
}

#[test]
fn laser_power_fires_the_wide_two_column_beam() {
    let mut game = dev_playing();
    select_and_activate(&mut game, 4);
    assert!(game.power.has_laser());

    let mut seen = false;
    for _ in 0..90 {
        game.step(Buttons {
            fire: true,
            ..Buttons::NONE
        });
        let Some(laser) = game
            .shots
            .iter()
            .find(|shot| shot.active && shot.kind == 1)
            .copied()
        else {
            continue;
        };
        seen = true;
        let sprites = game.build_sprites();
        for x in [i32::from(laser.x.int), i32::from(laser.x.int) + 8] {
            assert!(
                sprites.iter().any(|sprite| {
                    sprite.tile.index == 0x6C
                        && sprite.x == x
                        && sprite.y == i32::from(laser.y.int) + 8
                }),
                "the laser is missing its $6D column at x = {x}"
            );
        }
    }
    assert!(seen, "the laser shot was never spawned");
}

#[test]
fn option_power_places_a_trailing_visible_drone() {
    let mut game = dev_playing();
    select_and_activate(&mut game, 5);
    assert_eq!(game.power.options(), 1);

    for _ in 0..90 {
        game.step(Buttons {
            fire: true,
            right: true,
            ..Buttons::NONE
        });
    }
    let drone = game.options[0];
    assert!(drone.active, "the option drone is not active");
    assert!(
        drone.x != game.player.x.int || drone.y != game.player.y.int,
        "the option drone is not trailing the ship"
    );
    let sprites = game.build_sprites();
    assert!(
        sprites.iter().any(|sprite| {
            (sprite.tile.index == 0x58 || sprite.tile.index == 0x5A)
                && (sprite.x == i32::from(drone.x) || sprite.x == i32::from(drone.x) - 8)
                && sprite.y == i32::from(drone.y) + 1
        }),
        "the option drone has no rendered sprite"
    );
}

#[test]
fn shield_draws_the_rom_ring_group_at_the_ship_nose() {
    // `$8B70-$8B8B`: `$46` draws one of groups 90-93, tiles
    // `$B7/$B9/$C1/$C3`, at ship `(+15, -1)`.
    let mut game = dev_playing();
    select_and_activate(&mut game, 6);
    assert_eq!(game.power.shield_hits(), crate::powerup::SHIELD_HITS);

    let ship_x = i32::from(game.player.x.int);
    let ship_y = i32::from(game.player.y.int);
    let tile = |game: &Game| {
        let phase = ((game.nmi_frame >> 2) & 3) as usize;
        let index = [0xB6u8, 0xB8, 0xC0, 0xC2][phase];
        game.build_sprites()
            .iter()
            .find(|sprite| {
                sprite.tile.index == index && sprite.x == ship_x + 15 && sprite.y == ship_y
            })
            .map(|sprite| sprite.tile.index)
    };
    let first = tile(&game).expect("the ring tile is not at the ship nose");

    game.nmi_frame = game.nmi_frame.wrapping_add(4);
    let second = tile(&game).expect("the ring did not cycle");
    assert_ne!(first, second, "the ring stays on one group");
}

#[test]
fn the_clear_handoff_skips_the_shield_ring() {
    // `$8B74`: `$1B & #$70` skips the ring. `$1B = $90` is the clear
    // hand-off frame, modelled here as `boss_clear == 1`; `$1B = $80`
    // (normal play) still draws it.
    let mut game = dev_playing();
    select_and_activate(&mut game, 6);
    let ring = |game: &Game| {
        game.build_sprites().iter().any(|sprite| {
            matches!(sprite.tile.index, 0xB6 | 0xB8 | 0xC0 | 0xC2)
                && sprite.x == i32::from(game.player.x.int) + 15
        })
    };
    game.boss_clear = 1;
    assert!(!ring(&game), "the ring is drawn while $1B = $90");
    game.boss_clear = 0;
    assert!(ring(&game), "the ring is missing at $1B = $80");
}

#[test]
fn a_real_capsule_pickup_advances_the_meter_and_b_activates() {
    let mut game = dev_playing();
    game.dev_spawn_capsule_at_player();
    game.step(Buttons::NONE);
    assert_eq!(game.power.selection(), 1, "the capsule advanced $42");
    assert_eq!(game.power.capsules(), 1);

    let level = game.power.speed_level();
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    game.step(Buttons::NONE);
    assert_eq!(game.power.speed_level(), level + 1, "B activated SPEED");
    assert_eq!(game.power.selection(), 0);
}
