#[test]
fn a_shot_hits_at_the_documented_eight_pixel_edge() {
    // `$BFCC` gives the normal shot a width of `$10`, so a shot whose
    // `$BFC8` point is eight pixels past the object still connects.
    let mut game = sandbox();
    // One pixel inside the top edge: this test pins the X edge, and the
    // oracle does not connect a probe exactly on the box top.
    game.objects[5] = object(0x85, 0x64, 0x63);
    game.shots[0] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x64),
        y: Fixed::px(0x5C),
        age: 0,
    };
    game.collide_shots();
    assert_eq!(game.objects[5].kind, 2, "the shot connects at dx = 8");
    assert!(!game.shots[0].active, "and is consumed");
}

#[test]
fn the_shot_y_box_uses_integer_bytes_and_includes_the_edge() {
    // `$C01F`/`$C022` subtract only `$032C,Y` and compare with `BCS`, so
    // the accepted band is `1 <= dy <= height`; fractions do not enter.
    let mut game = sandbox();
    game.objects[5] = object(0x88, 0x64, 0x30);
    game.objects[5].y.frac = 0x80;
    game.shots[0] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x60),
        // Probe Y = 56 + 8 = 64; object Y = 48 -> dy = 16 == height.
        y: Fixed::px(0x38),
        age: 0,
    };
    game.shots[0].y.frac = 0xFF;
    game.collide_shots();
    assert_eq!(game.objects[5].kind, 2, "dy == height connects");
    assert!(!game.shots[0].active, "and is consumed");

    let mut edge = sandbox();
    edge.objects[5] = object(0x88, 0x64, 0x30);
    edge.shots[0] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x60),
        y: Fixed::px(0x39),
        age: 0,
    };
    edge.collide_shots();
    assert_eq!(edge.objects[5].kind, 0x88, "dy == height + 1 misses");
    assert!(edge.shots[0].active, "and the shot survives");

    let mut top = sandbox();
    top.objects[5] = object(0x88, 0x64, 0x40);
    top.shots[0] = Shot {
        active: true,
        kind: 0,
        x: Fixed::px(0x60),
        y: Fixed::px(0x38),
        age: 0,
    };
    top.collide_shots();
    assert_eq!(top.objects[5].kind, 0x88, "the top edge (dy == 0) misses");
}

#[test]
fn enemy_bullets_hit_from_the_box_side_the_rom_tests() {
    // `$C22C`: the compare is `player_edge - bullet`, so a bullet to the
    // right of the ship is outside the box; the X `CMP` leaves the borrow
    // set and the Y band is `1..=height` on `player.y - bullet.y`.
    let mut game = sandbox();
    game.enemy_bullets[0] = EnemyBullet {
        x: (0x50 + 11) << 8,
        y: (0x60 + 7) << 8,
        ..EnemyBullet::default()
    };
    game.enemy_bullet_count = 1;
    game.collide_player_bullets();
    assert!(!game.player.dying, "a bullet to the right/below misses");
    assert_eq!(game.enemy_bullet_count, 1);

    game.enemy_bullets[0] = EnemyBullet {
        x: (0x50 - 3) << 8,
        y: (0x60 - 4) << 8,
        ..EnemyBullet::default()
    };
    game.enemy_bullet_count = 1;
    game.collide_player_bullets();
    assert!(game.player.dying, "a bullet inside the box kills");
}

#[test]
fn aimed_shots_use_one_pixel_on_the_major_axis() {
    // `$BCAF` + `$83BA`: the major axis moves 1 px/frame and the minor axis
    // gets `minor*256/major`. `$BCDE` takes one off a negative delta before
    // the divide and counts a zero delta as one; `$CBCB` fires from `(100,
    // 100)` here and the shot leaves along the vector to the ship.
    let mut game = sandbox();
    game.enemy_bullet_count = 0;
    let mut aim = |ship: (u8, u8)| {
        game.enemy_bullet_count = 0;
        game.player = Player::new(ship.0, ship.1);
        game.push_enemy_bullet(0x64, 0x64, 0, 0x25);
        let bullet = game.enemy_bullets[0];
        (bullet.vx, bullet.vy)
    };
    assert_eq!(aim((0x96, 0x64)), (0x100, -5), "ship to the right");
    assert_eq!(aim((0x00, 0x64)), (-0x100, -2), "ship to the left");
    assert_eq!(aim((0x64, 0x32)), (-5, -0x100), "ship above");
    assert_eq!(aim((0xC8, 0x96)), (0x100, 126), "ship down-right");
    assert_eq!(aim((0x64, 0xC8)), (-2, 0x100), "ship below");
}

#[test]
fn object_boxes_use_the_c1fc_c200_size_classes() {
    // `$BFD4`/`$BFD8`: class 0 is 16x16, class 1 is 32x32, class 2 is
    // 48x48, class 3 is 16x2.
    assert_eq!(tables::ENEMY_BOX_WIDTH, [0x10, 0x20, 0x30, 0x10]);
    assert_eq!(tables::ENEMY_BOX_HEIGHT, [0x10, 0x20, 0x30, 0x02]);
    let mut game = sandbox();
    let mut big = object(0x85, 0x40, 0x40);
    big.size_class = 1;
    game.objects[9] = big;
    // `$C11C` subtracts with the width compare's borrow, so its band is
    // `1..=height`; y = $39 puts the ship point at $41, dy = 1.
    game.player = Player::new(0x40, 0x39);
    game.collide_player_objects();
    // Ship point (x+4, y+8) = ($44, $41) is inside the 32x32 box.
    assert!(game.player.dying);
}

#[test]
fn ship_object_box_accepts_dy_one_through_height() {
    // `$C11C`: the width compare's borrow makes the Y subtract
    // `py - y - 1`, so `dy == height` hits and `dy == 0` misses.
    let mut last = sandbox();
    last.objects[9] = object(0x85, 0x40, 0x40);
    last.objects[9].size_class = 1; // 32x32
    last.player = Player::new(0x40, 0x58); // probe y $60 - $40 = $20
    last.collide_player_objects();
    assert!(last.player.dying, "the last row of the box hits");

    let mut first = sandbox();
    first.objects[9] = object(0x85, 0x40, 0x40);
    first.objects[9].size_class = 1;
    first.player = Player::new(0x40, 0x38); // probe y $40 - $40 = 0
    first.collide_player_objects();
    assert!(!first.player.dying, "the borrow rejects dy == 0");
}

#[test]
fn debris_is_harmless_but_terrain_is_lethal() {
    let mut game = sandbox();
    game.objects[9] = object(0x02, 0x40, 0x38);
    game.player = Player::new(0x40, 0x38);
    game.collide_player_objects();
    assert!(!game.player.dying, "kind-2 debris never hurts");
    // A `$A65C` terrain piece is an ordinary lethal object once its handler
    // has set the bit-7 initialised flag (`$C1B2`).
    game.objects[9] = object(0x86, 0x40, 0x38);
    game.objects[9].terrain = true;
    game.collide_player_objects();
    assert!(game.player.dying, "terrain pieces use their real box");
}

#[test]
fn stage_seven_shots_die_on_its_own_map() {
    // Frame 876: `$0363 = 227` and scroll `$3F:$3E = 1:$22`, so the
    // probe is world column 65, group 1. Only stage 7's map is solid
    // there; the stage-1 map reads zero.
    let scroll = Scroll {
        fraction: 0,
        low: 0x22,
        high: 1,
    };
    let shot = || Shot {
        active: true,
        kind: 0,
        x: Fixed::px(227),
        y: Fixed::px(16),
        age: 0,
    };
    let mut stage7 = sandbox();
    stage7.section = 6;
    stage7.scroll = scroll;
    stage7.shots[0] = shot();
    stage7.collide_shot_terrain();
    assert!(
        !stage7.shots[0].active,
        "the stage-7 wall consumes the shot"
    );

    let mut stage1 = sandbox();
    stage1.scroll = scroll;
    stage1.shots[0] = shot();
    stage1.collide_shot_terrain();
    assert!(stage1.shots[0].active, "stage 1 keeps the stage-1 map");
}

#[test]
fn stages_two_to_six_shots_die_on_their_own_maps() {
    let stage_one = crate::stage_collision::map(crate::rom::testing::rom(), 0);
    let solid = |map: &[u8], column: usize, group: usize, row: usize| {
        (map[column * 8 + group] >> (2 * row)) & 0x03 != 0
    };
    for section in 1..=5usize {
        let own = crate::stage_collision::map(crate::rom::testing::rom(), section);
        // Stage 5 is open starfield (its map is all zero): no terrain, so
        // there is no discriminating spot and `terrain_solid` must stay
        // false.
        if section == 4 {
            assert!(own.iter().all(|byte| *byte == 0));
            continue;
        }
        let mut spot = None;
        'search: for block in 4..11usize {
            for column in block * 32..(block + 1) * 32 {
                for group in 0..8usize {
                    for row_in_group in 0..4usize {
                        let row = group * 4 + row_in_group;
                        let y = row as i32 * 8 - 0x14;
                        let x = column as i32 * 8 - block as i32 * 256 - 8;
                        if !(0..=0xBF).contains(&y) || !(0..=0xEF).contains(&x) {
                            continue;
                        }
                        if solid(own, column, group, row_in_group)
                            && !solid(stage_one, column, group, row_in_group)
                        {
                            spot = Some((block as u8, x as u8, y as u8));
                            break 'search;
                        }
                    }
                }
            }
        }
        let (high, x, y) =
            spot.unwrap_or_else(|| panic!("stage {} has no discriminating spot", section + 1));
        let scroll = Scroll {
            fraction: 0,
            low: 0,
            high,
        };
        let shot = || Shot {
            active: true,
            kind: 0,
            x: Fixed::px(x),
            y: Fixed::px(y),
            age: 0,
        };
        let mut own_game = sandbox();
        own_game.section = section as u8;
        own_game.scroll = scroll;
        own_game.shots[0] = shot();
        own_game.collide_shot_terrain();
        assert!(
            !own_game.shots[0].active,
            "stage {} terrain at {x},{y} consumes the shot",
            section + 1
        );

        let mut stage1 = sandbox();
        stage1.scroll = scroll;
        stage1.shots[0] = shot();
        stage1.collide_shot_terrain();
        assert!(
            stage1.shots[0].active,
            "stage 1 keeps the shot at the stage-{} spot {x},{y}",
            section + 1
        );
    }
}

#[test]
fn flying_into_solid_terrain_kills_the_ship() {
    let (scroll, x, y) = solid_spot();
    let mut game = sandbox();
    game.scroll = scroll;
    game.player = Player::new(x, y);
    game.collide_player_terrain();
    assert!(game.player.dying, "solid terrain at {x},{y}");

    // `$C2B6`: the map pass has no shield branch, so `$46 = 5` cannot save the
    // ship (measured: the oracle dies against stage 7's top wall with it set).
    let mut shielded = sandbox();
    shielded.scroll = scroll;
    shielded.power.dev_set_shield(5);
    shielded.player = Player::new(x, y);
    shielded.collide_player_terrain();
    assert!(shielded.player.dying, "the shield does not stop the map");
    assert_eq!(shielded.power.shield_hits(), 5);
}

#[test]
fn ship_vs_map_collision_is_rom_parity() {
    let (scroll, x, y) = solid_spot();
    let mut game = sandbox();
    game.scroll = scroll;
    game.player = Player::new(x, y);
    game.collide();
    assert!(
        game.player.dying,
        "`$C29F`/`$C39D` kill the ship on a solid map byte"
    );
}

#[test]
fn shield_contact_turns_a_small_object_into_debris() {
    // `$C1CA`/`$BE8D`: with `$46 != 0` a small object is destroyed into
    // kind-2 debris and no score is awarded; a large object (`$010C` bit 7)
    // only increments its `$046C` damage counter.
    let mut game = sandbox();
    game.power.dev_set_shield(5);
    game.objects[9] = object(0x86, 0x40, 0x38);
    game.player = Player::new(0x40, 0x38);
    let score = game.score;
    game.collide_player_objects();
    assert_eq!(game.objects[9].kind, 2, "small object becomes debris");
    assert_eq!(game.score, score, "the shield kill is not scored");
    game.objects[9] = object(0x86, 0x40, 0x38);
    game.objects[9].subtype = 0x80;
    game.player = Player::new(0x40, 0x38);
    game.collide_player_objects();
    assert_eq!(game.objects[9].kind, 0x86, "large object survives");
    assert_eq!(game.objects[9].ai_timer, 1, "`$046C` takes the hit");
}

#[test]
fn invincible_ship_survives_enemies_bullets_and_terrain() {
    let (scroll, x, y) = solid_spot();
    let mut game = sandbox();
    game.set_mods(Mods {
        hull_hits: 0,
        ..Mods::default()
    });
    game.scroll = scroll;
    game.place_player((x, y));

    game.collide_player_terrain();
    assert!(!game.player.dying, "terrain does not kill");

    game.objects[9] = object(0x86, x, y);
    game.collide_player_objects();
    assert!(!game.player.dying, "an enemy does not kill");

    game.enemy_bullets[0] = EnemyBullet {
        x: (i32::from(x) - 3) << 8,
        y: (i32::from(y) - 4) << 8,
        ..EnemyBullet::default()
    };
    game.enemy_bullet_count = 1;
    game.collide_player_bullets();
    assert!(!game.player.dying, "a bullet does not kill");
    assert_eq!(game.enemy_bullet_count, 0, "the bullet is still consumed");

    game.begin_death();
    assert!(!game.player.dying, "no path starts the death");
}

#[test]
fn one_hit_default_still_dies_on_terrain() {
    let (scroll, x, y) = solid_spot();
    let mut game = sandbox();
    game.scroll = scroll;
    game.place_player((x, y));
    game.collide_player_terrain();
    assert!(game.player.dying);
}

/// A stage-2 map cell with the given 2-bit field, as the screen position and
/// scroll whose `$C3CD` probe lands on it.
fn stage_two_cell(field: u8) -> (Scroll, u8, u8) {
    stage_cell(1, 4 * 32..11 * 32, field)
}

fn stage_cell(stage_index: usize, columns: std::ops::Range<usize>, field: u8) -> (Scroll, u8, u8) {
    let map = crate::stage_collision::map(crate::rom::testing::rom(), stage_index);
    for column in columns {
        for group in 0..8usize {
            for row_in_group in 0..4usize {
                let row = group * 4 + row_in_group;
                let y = row as i32 * 8 - 0x14;
                let block = column / 32;
                let x = column as i32 * 8 - block as i32 * 256 - 8;
                if !(0..=0xBF).contains(&y) || !(0x10..=0xEF).contains(&x) {
                    continue;
                }
                if (map[column * 8 + group] >> (2 * row_in_group)) & 3 == field {
                    let scroll = Scroll {
                        fraction: 0,
                        low: 0,
                        high: block as u8,
                    };
                    return (scroll, x as u8, y as u8);
                }
            }
        }
    }
    panic!("stage {} has no field-{field} cell", stage_index + 1);
}

fn stage_two_shot(kind: u8, x: u8, y: u8) -> Shot {
    Shot {
        active: true,
        kind,
        x: Fixed::px(x),
        y: Fixed::px(y),
        age: 0,
    }
}

#[test]
fn a_shot_breaks_a_destructible_stage_two_cell() {
    let (scroll, x, y) = stage_two_cell(2);
    let mut game = sandbox();
    game.section = 1;
    game.scroll = scroll;
    game.player = Player::new(x, y);
    assert!(game.terrain_solid(i32::from(x), i32::from(y)), "solid before");
    let score = game.score_bcd();

    game.shots[0] = stage_two_shot(0, x, y);
    game.collide_shot_terrain();
    assert!(!game.shots[0].active, "`$C2E2`: the shot is spent");
    assert!(!game.terrain_solid(i32::from(x), i32::from(y)), "map cleared");
    assert_eq!(game.score_bcd(), score, "the ROM awards no points");

    // Not solid for the ship, nor for the next shot.
    game.collide_player_terrain();
    assert!(!game.player.dying, "the ship flies through the gap");
    game.shots[1] = stage_two_shot(0, x, y);
    game.collide_shot_terrain();
    assert!(game.shots[1].active, "the next shot passes");
}

#[test]
fn breaking_a_cell_plays_the_rom_break_effect() {
    let (scroll, x, y) = stage_two_cell(2);
    let mut game = sandbox();
    game.section = 1;
    game.scroll = scroll;
    game.shots[0] = stage_two_shot(0, x, y);
    game.audio.drain().for_each(drop);
    game.collide_shot_terrain();
    let sounds: Vec<_> = game.audio.drain().collect();
    assert_eq!(sounds, [crate::audio::Sound::BlockBreak], "`$EC18` id 3");

    // Stage 6 (`$19 == 5`) uses id 4.
    let mut game = sandbox();
    let (scroll, x, y) = stage_cell(5, 0..11 * 32, 2);
    game.section = 5;
    game.scroll = scroll;
    game.shots[0] = stage_two_shot(0, x, y);
    game.audio.drain().for_each(drop);
    game.collide_shot_terrain();
    let sounds: Vec<_> = game.audio.drain().collect();
    assert_eq!(sounds, [crate::audio::Sound::BlockBreakStage6]);
}

#[test]
fn a_laser_pierces_destructible_cells_but_not_solid_ones() {
    let (scroll, x, y) = stage_two_cell(2);
    let mut game = sandbox();
    game.section = 1;
    game.scroll = scroll;
    game.shots[0] = stage_two_shot(1, x.wrapping_sub(0x0A), y);
    game.collide_shot_terrain();
    assert!(game.shots[0].active, "`$C2DB`: the laser keeps flying");
    assert!(!game.terrain_solid(i32::from(x), i32::from(y)));

    let (scroll, x, y) = stage_two_cell(1);
    let mut game = sandbox();
    game.section = 1;
    game.scroll = scroll;
    game.shots[0] = stage_two_shot(1, x.wrapping_sub(0x0A), y);
    game.collide_shot_terrain();
    assert!(!game.shots[0].active, "field 1 stops even a laser");
    assert!(game.terrain_solid(i32::from(x), i32::from(y)), "and stays");
}

#[test]
fn broken_cells_survive_a_save_state_and_clear_with_the_stage() {
    let (scroll, x, y) = stage_two_cell(2);
    let mut game = sandbox();
    game.section = 1;
    game.scroll = scroll;
    game.shots[0] = stage_two_shot(0, x, y);
    game.collide_shot_terrain();
    let image = game.snapshot();

    let mut restored = Game::new(assets());
    restored.restore(&image).unwrap();
    assert!(!restored.terrain_solid(i32::from(x), i32::from(y)));
    assert_eq!(restored.snapshot(), image);

    let mut other = sandbox();
    other.section = 1;
    other.scroll = scroll;
    other.shots[0] = stage_two_shot(0, x, y);
    other.collide_shot_terrain();
    assert!(!other.terrain_solid(i32::from(x), i32::from(y)));
    other.restore(&Game::new(assets()).snapshot()).unwrap();
    assert!(
        other.broken.iter().all(|mask| *mask == 0),
        "restoring a clean image repairs the cells"
    );
}

#[test]
fn stage_three_tests_the_map_on_odd_frames_and_stage_five_never() {
    // `$C29F`: stage 3 skips the shot and ship map tests on even `$02`
    // frames; stage 5 returns before them.
    let (scroll, x, y) = stage_cell(2, 0..11 * 32, 1);
    let run = |section: u8, nmi_frame: u8| {
        let mut game = sandbox();
        game.section = section;
        game.scroll = scroll;
        game.nmi_frame = nmi_frame;
        game.player = Player::new(x, y);
        game.shots[0] = stage_two_shot(0, x, y);
        game.collide();
        (game.shots[0].active, game.player.dying)
    };
    assert_eq!(run(2, 1), (false, true), "odd frame: shot dies, ship crashes");
    assert_eq!(run(2, 2), (true, false), "even frame: skipped");
    let mut game = sandbox();
    game.section = 4;
    game.scroll = scroll;
    game.player = Player::new(x, y);
    game.shots[0] = stage_two_shot(0, x, y);
    game.collide();
    assert!(game.shots[0].active && !game.player.dying, "stage 5 returns early");
}

fn bullet_at(kind: u8, x: u8, y: u8) -> EnemyBullet {
    EnemyBullet {
        x: i32::from(x) << 8,
        y: i32::from(y) << 8,
        kind,
        ..EnemyBullet::default()
    }
}

#[test]
fn enemy_bullets_die_on_the_map_and_never_break_it() {
    // `$C2F9`: `$C3CD` probes the bullet (y + 8 for kind 0); a non-zero
    // field, destructible or not, removes it and leaves the cell standing.
    let (scroll, x, y) = stage_two_cell(2);
    let mut game = sandbox();
    game.section = 1;
    game.scroll = scroll;
    game.player = Player::new(0xE0, 0xB0);
    game.enemy_bullets[0] = bullet_at(1, x, y);
    game.enemy_bullets[1] = bullet_at(0, x, y.wrapping_sub(8));
    let free_y = (0x10..0xB0u8)
        .find(|y| !game.terrain_solid(0x80, i32::from(*y)))
        .expect("an empty spot");
    game.enemy_bullets[2] = bullet_at(1, 0x80, free_y);
    game.enemy_bullet_count = 3;
    game.collide_bullet_terrain();
    assert_eq!(game.enemy_bullet_count, 1, "both bullets on the cell die");
    assert_eq!(game.enemy_bullets[0].x >> 8, 0x80, "the free bullet keeps its slot order");
    assert_eq!(game.broken_cells(), 0, "bullets break nothing");
    assert!(game.terrain_solid(i32::from(x), i32::from(y)));

    game.enemy_bullets[0] = bullet_at(0, x, y);
    game.collide_bullet_terrain();
    assert_eq!(game.enemy_bullet_count, 1, "kind 0 probes 8 px lower and misses");
}

#[test]
fn stage_three_odd_frames_skip_the_bullet_loop_while_the_ship_lives() {
    // `$C2F8`: stage 3 returns after the shot loop on odd `$02`; even frames
    // jump to `$C2F9`; a dying ship (`$0100 >= 2`) falls through to it.
    // Stage 5 returns at `$C2A9`.
    let (scroll, x, y) = stage_cell(2, 0..11 * 32, 1);
    let survivors = |section: u8, nmi_frame: u8, dying: bool| {
        let mut game = sandbox();
        game.section = section;
        game.scroll = scroll;
        game.nmi_frame = nmi_frame;
        game.player = Player::new(0xE0, 0xB0);
        game.player.dying = dying;
        game.enemy_bullets[0] = bullet_at(1, x, y);
        game.enemy_bullet_count = 1;
        game.collide();
        game.enemy_bullet_count
    };
    assert_eq!(survivors(2, 1, false), 1, "odd frame, ship alive: RTS");
    assert_eq!(survivors(2, 2, false), 0, "even frame: bullets tested");
    assert_eq!(survivors(2, 1, true), 0, "dying ship: loop runs");
    assert_eq!(survivors(4, 2, false), 1, "stage 5 never tests");
}

#[test]
fn the_83ba_divide_rounds_like_the_rom() {
    // Measured on the oracle: major 135, minor 32 gave the stage-2 launcher a
    // `$4D` step, which is `61 + 61/4 + 1`; truncation would give 60.
    assert_eq!(slope(135, 32), 61);
    assert_eq!(slope(135, 33), 63);
    assert_eq!(slope(129, 15), 30);
    assert_eq!(slope(100, 50), 128);
    assert_eq!(slope(0, 5), 0);
}

#[test]
fn stage_seven_last_blocks_are_the_solid_top_wall() {
    // A kept-alive ship pinned at `y = 16` is inside the top wall from
    // `$3F = $0C` (oracle 6563) through `$0D`; the oracle's `$0500`/`$0600`
    // pages there read `$15`. The extractor used to drop these two blocks as
    // past the playable end.
    let mut game = sandbox();
    game.section = 6;
    for high in [0x0C, 0x0D] {
        game.scroll = Scroll {
            fraction: 0,
            low: 0,
            high,
        };
        assert!(
            game.terrain_solid(80, 16),
            "stage 7 top wall at $3F = {high:#04X}"
        );
    }
}

#[test]
fn the_stage_7_door_paints_and_its_collision_follows() {
    // `$B600`: the wall paints the door's row pairs from the outside in, one
    // pair every twenty frames, and `$B60C` writes the collision bytes with
    // the same step. The port keeps both in `cells`: a painted cell is solid.
    let mut game = sandbox();
    game.section = 6;
    game.scroll = Scroll {
        fraction: 0,
        low: 0,
        high: 0x0B,
    };
    // World column 376 sits at screen x = 184 with the scroll at `$0B00`; the
    // `$C3CD` probe row is `(y + $14) / 8`.
    let collision = |game: &Game, row: u8| -> u8 {
        let (column, probed, field) = game
            .terrain_probe(184, i32::from(row) * 8 - 20)
            .expect("the door is inside the collision map");
        assert_eq!(column, 376);
        assert_eq!(probed, usize::from(row));
        field
    };
    let tile = |game: &Game, row: u8| -> Option<u8> {
        game.cells
            .iter()
            .find(|cell| cell.world_col == 376 && cell.row == row)
            .map(|cell| cell.tile)
    };
    // The extracted map leaves the whole corridor open; the wall closes it.
    for row in 11..=16 {
        assert_eq!(tile(&game, row), None, "row {row} starts open");
        assert_eq!(collision(&game, row), 0, "row {row} starts passable");
    }
    // Each call reads the counter the frame started with: step `n` is
    // `$046C = n + 1`. The odd paint is the `$C5`/`$C3` stripe, the even one
    // the `$C4`/`$C2` partner.
    for (step, rows) in [
        (1u8, [(11u8, 0xC5u8), (16, 0xC3)]),
        (2, [(11, 0xC4), (16, 0xC2)]),
        (3, [(12, 0xC5), (15, 0xC3)]),
        (4, [(12, 0xC4), (15, 0xC2)]),
        (5, [(13, 0xC5), (14, 0xC3)]),
        (6, [(13, 0xC4), (14, 0xC2)]),
    ] {
        game.paint_stage7_door(Some(step + 1));
        for (row, painted) in rows {
            assert_eq!(tile(&game, row), Some(painted), "row {row} tile at step {step}");
        }
        // `$B60C`: a painted row is solid for good, and the wall writes the
        // bytes with the same step.
        for (row, _) in rows {
            assert_ne!(collision(&game, row), 0, "row {row} solid at step {step}");
        }
    }
    for row in 11..=16 {
        assert_ne!(collision(&game, row), 0, "the closed door is solid at row {row}");
    }
    // Only stage 7 has the door: other stages never read a wall timer.
    let mut other = sandbox();
    other.section = 5;
    other.objects[9] = object(0x1E, 0xAF, 0x40);
    assert_eq!(other.stage7_wall_timer(), None);
}
