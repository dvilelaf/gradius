#[test]
fn the_table_driven_spawner_produces_stage_one_enemies() {
    let mut game = playing();
    let mut seen = false;
    for _ in 0..1_600 {
        game.step(Buttons::NONE);
        seen |= game
            .objects
            .iter()
            .any(|object| object.active && !object.terrain);
    }
    assert!(seen, "section 0 column 0 must spawn by $3E = $20");
}

#[test]
fn the_first_formation_matches_the_oracle_slot_and_y_layout() {
    // Frame 380 of the oracle: enemies in slots 9 and 8, type $05, Y $2A,
    // entry X $F0.
    let mut game = playing();
    for _ in 0..400 {
        game.step(Buttons::NONE);
        if game.objects[9].active {
            break;
        }
    }
    let object = game.objects[9];
    assert!(object.active);
    // `$030C` keeps the handler's bit-7 "initialised" flag, exactly as the
    // original adds `$80` on the first AI frame.
    assert_eq!(object.kind & 0x7F, 0x05);
    // `$A42E`: pair Y `$2A` + the formation Y add `$00`.
    assert_eq!(object.y.int, 0x2A);
    assert!(
        (0xE0..=0xF0).contains(&object.x.int),
        "x={:#04X}",
        object.x.int
    );
}

#[test]
fn a_section_two_command_spawns_a_background_turret() {
    // `$A469`: kind `$96`, subtype `$64`, Y `$65`, `$03BC:$03EC` in the
    // Y-velocity pair, X `$F0`. `$A521` leaves sprite/size/boss clear.
    let mut game = playing();
    game.section = 2;
    game.command([0x92, 0xF0, 0x38, 0x21, 0x23]);
    let turret = game
        .objects
        .iter()
        .find(|object| object.live())
        .expect("the command creates a turret");
    assert_eq!(turret.kind, 0x96);
    assert_eq!(turret.y.int, 0x38);
    assert_eq!(turret.subtype, 0x80);
    assert_eq!(turret.x.int, 0xF0);
    // `$03BC` is the high byte and `$03EC` the low byte of the pair.
    assert_eq!(turret.nametable_addr, 0x2123);
    assert_eq!((turret.x_vel, turret.y_vel), (0, 0), "velocities stay free");
    assert_eq!(turret.sprite, 0, "no OAM sprite");
    assert_eq!(turret.size_class, 0, "`$0460` stays zero");
    assert!(!turret.boss, "not the end-of-stage core");
    assert!(!game.boss_spawned, "the shared core must still arm");
}

#[test]
fn a_non_stage_three_command_counts_both_nibbles() {
    // `$A4A0`: low nibble first, `$016C` = number of `$0600` records.
    let mut two = playing();
    two.command([0x40, 0xF0, 0x12, 0x14, 0x40]);
    let parent = two.objects[9];
    assert_eq!(parent.kind, 0x14);
    assert_eq!(parent.subtype, 0x80);
    assert_eq!((parent.x.int, parent.y.int), (0xF0, 0x40));
    assert_eq!((parent.sprite, parent.size_class), (0x89, 1));
    assert_eq!(parent.anim, 2, "two non-zero nibbles");
    assert!(!parent.boss);
    // `$A4FA`: the low nibble allocates the highest free `$0600` record
    // first; slot 8's measured pair is `$0630` index 0, `$0660` index 1.
    let live: Vec<_> = two
        .cannons
        .iter()
        .filter(|cannon| cannon.active)
        .map(|cannon| (cannon.parent, cannon.index))
        .collect();
    assert_eq!(live, vec![(9, 0), (9, 1)]);
    assert!(two.cannons[3].active && two.cannons[2].active);

    let mut one = playing();
    one.command([0x40, 0xF0, 0x01, 0x14, 0x80]);
    let parent = one.objects[9];
    assert_eq!(parent.anim, 1, "only the low nibble is non-zero");
    assert_eq!(parent.y.int, 0x80);
    assert!(!parent.boss);
}

#[test]
fn the_stage_one_threshold_starts_the_boss_sequence() {
    let mut game = playing();
    game.phase = Phase::Normal;
    game.scroll.high = tables::SECTION_THRESHOLD[0] - 1;
    game.scroll.low = 0xFF;
    game.scroll.fraction = tables::SCROLL_STEP;
    game.step(Buttons::NONE);
    // `$1B = $81`: stage 1 does not advance to another section here; the
    // eruption/boss sequence runs instead.
    assert_eq!(game.phase, Phase::Normal);
    assert_eq!(game.boss_stage, 1);
}

#[test]
fn a_full_run_is_table_driven_and_reaches_the_stage_three_command_path() {
    // The stage-3 command records are the `$96` turret creator; there is
    // no invented wave anywhere. This only checks the table hooks exist.
    assert!(crate::rom::testing::rom().spawns.sections[2].iter().any(|stream| stream
        .iter()
        .any(|record| matches!(record, crate::rom::spawns::Record::Command(_)))));
    assert_eq!(crate::rom::testing::rom().spawns.object_defs[0][1], 0x05);
}

#[test]
fn stage_two_spawner_reads_the_stage_two_streams() {
    let mut game = Game::new_at_stage(patterned_assets(), 1);
    assert_eq!(game.section(), 1);
    // `rom.spawns.sections[1]` column 0 starts `$89` at delay 0 -> kind `$05`.
    game.scroll.high = 0;
    game.scroll.low = 0xFF;
    game.scroll.fraction = 0;
    let event = match game.spawner.step(game.scroll) {
        Some(Emission::Spawn(event)) => event,
        other => panic!("expected a stage-2 spawn, got {other:?}"),
    };
    assert_eq!(event.kind, game.rom().spawns.object_defs[0x89 & 0x3F][1]);
    assert_eq!(event.kind, 0x05);
}

#[test]
fn the_c64d_script_cycles_the_four_pairs_every_forty_calls() {
    // `$C64D`: `$68` to `$28`, then the `$C674` pair cycle `(02,80)`,
    // `(00,40)`, `(01,80)`, `(00,C0)` through `$A4A0`.
    let mut game = sandbox();
    for _ in 0..STAGE5_SCRIPT_PERIOD - 1 {
        game.stage_five_script();
    }
    assert!(game.objects.iter().all(|object| !object.live()));
    game.stage_five_script();
    assert_eq!(game.objects[9].kind, 0x14);
    assert_eq!(game.objects[9].y.int, 0x80);
    assert_eq!(game.objects[9].anim, 1, "`$02` has one cannon nibble");
    assert_eq!(game.script_counter, 0);

    for _ in 0..STAGE5_SCRIPT_PERIOD {
        game.stage_five_script();
    }
    assert_eq!(game.objects[8].kind, 0x14);
    assert_eq!(game.objects[8].y.int, 0x40);
    assert_eq!(game.objects[8].anim, 0, "`$00` has no cannon");

    for _ in 0..STAGE5_SCRIPT_PERIOD {
        game.stage_five_script();
    }
    assert_eq!(game.objects[7].y.int, 0x80);
    assert_eq!(game.objects[7].anim, 1);
}

#[test]
fn stage_three_hazard_script_spawns_the_c680_small_core() {
    // `$C680`: `$68` counts every `$C40D` call; the `$3A = 0` period is
    // `$28`, then the `$C6C8[0]` entry appears at X = `$F0`.
    let mut game = Game::new_at_stage(patterned_assets(), 2);
    game.phase = Phase::Normal;
    game.boss_stage = 1;
    game.scroll_locked = true;
    game.scroll.high = 0x0C;
    for _ in 0..STAGE3_SCRIPT_PERIOD - 1 {
        game.stage_three_script();
    }
    assert!(game.objects.iter().all(|object| !object.live()));
    game.stage_three_script();
    let spawned = game.objects[9];
    assert_eq!(spawned.kind, 0x97);
    assert_eq!(spawned.subtype, 0x80);
    assert_eq!((spawned.x.int, spawned.y.int), (0xF0, 0x40));
    assert_eq!((spawned.sprite, spawned.size_class), (0x3F, 1));
    assert_eq!(game.script_counter, 0);
    assert_eq!(game.hazard_index, 1);
}

#[test]
fn the_eruption_hold_follows_the_rank_table() {
    // `$9A01`: `$4D = $9A28[$17]`, so stage 1 with a pinned shield is
    // rank 1 (`3 * 256`) and stage 2 adds the stage bit -> rank 2
    // (`4 * 256`). The port's +3 is the `$81`/`$83`/lock offset that keeps
    // the stage-1 spawn at 7723.
    let mut stage1 = sandbox();
    stage1.power.dev_set_shield(5);
    stage1.scroll.high = 0x0B;
    stage1.scroll.low = 0xFF;
    stage1.scroll.fraction = tables::SCROLL_STEP;
    stage1.normal_frame(Buttons::NONE, false);
    assert_eq!(stage1.boss_stage, 1);
    assert_eq!(stage1.boss_timer, 3 * 256 + 3);

    let mut stage2 = Game::new_at_stage(patterned_assets(), 1);
    stage2.phase = Phase::Normal;
    stage2.power.dev_set_shield(5);
    stage2.scroll.high = 0x0B;
    stage2.scroll.low = 0xFF;
    stage2.scroll.fraction = tables::SCROLL_STEP;
    stage2.normal_frame(Buttons::NONE, false);
    assert_eq!(stage2.boss_stage, 1);
    assert_eq!(stage2.boss_timer, 4 * 256 + 3, "`$9A28[2] = 4`");

    let mut stage3 = Game::new_at_stage(patterned_assets(), 2);
    stage3.phase = Phase::Normal;
    stage3.power.dev_set_shield(5);
    stage3.scroll.high = 0x0B;
    stage3.scroll.low = 0xFF;
    stage3.scroll.fraction = tables::SCROLL_STEP;
    stage3.normal_frame(Buttons::NONE, false);
    assert_eq!(stage3.boss_stage, 1);
    assert_eq!(stage3.boss_timer, 4 * 256 + 3, "`$9A28[2] = 4`");
}

#[test]
fn stage_two_hazard_script_spawns_the_c567_points() {
    let mut game = Game::new_at_stage(patterned_assets(), 1);
    game.phase = Phase::Normal;
    game.boss_stage = 1;
    game.scroll_locked = true;

    // `$C540`: only `$02 & 7 == 0` frames spawn. `$02 = frame - 5`.
    game.frame = 6;
    game.eruption_spawn();
    assert!(game.objects.iter().all(|object| !object.live()));

    game.frame = 5;
    game.eruption_spawn();
    let first = game.objects[9];
    assert_eq!((first.kind, first.sprite), (0x0B, 0x67));
    // First nibble is `$C587[0]`'s low nibble (1): `$C567` pair 1.
    assert_eq!((first.x.int, first.y.int), (0x80, 0x50));

    game.frame = 13;
    game.eruption_spawn();
    let second = game.objects[8];
    // Next call reads `$C587[0]`'s high nibble (0).
    assert_eq!((second.x.int, second.y.int), (0x20, 0x30));
    assert_eq!(game.hazard_index, 2);
}

#[test]
fn the_eruption_spawner_uses_the_old_script_index_and_new_parity() {
    let mut game = sandbox();
    game.boss_stage = 1;
    game.frame = 5;
    game.eruption_spawn();
    // `$C449` reads script byte 0 with the *old* `$69 = 0`, but selects
    // the nibble with the incremented `$69 = 1` (low nibble `$F`).
    let first = game.objects[9];
    assert_eq!((first.kind, first.x.int, first.y.int), (0x0A, 0xB8, 0x90));
    assert_eq!(first.ai_state, 0x60, "velocity nibble $F");
    assert_eq!(first.x_vel, -0x200);
    assert_eq!(first.y_vel, 0x400, "$69 < $0A subtracts 2 twice");
    game.frame = 9;
    game.eruption_spawn();
    // Second call: old `$69 = 1` indexes script byte 0 again, while the
    // new even parity selects its high nibble `$A`.
    let second = game.objects[8];
    assert_eq!((second.kind, second.x.int), (0x0A, 0x38));
    assert_eq!(second.ai_state, 0x40);
    assert_eq!(second.x_vel, 0x304);
    assert_eq!(second.y_vel, 0x404);
    assert_eq!(game.hazard_index, 2);
}

#[test]
fn the_stage_one_eruption_hold_spawns_several_8a_fireballs() {
    // `$C480`: during `$82` the four-frame script fills the object table
    // with kind `$0A`, which the handler raises to `$8A` on entry. The
    // scroll holds at `0:12:128`; see longrun.md's 6445-7258 census.
    let mut game = sandbox();
    game.dev_keep_alive = true;
    game.scroll = Scroll {
        fraction: tables::SCROLL_STEP,
        low: 0xFF,
        high: 0x0B,
    };
    game.step(Buttons::NONE);
    assert_eq!(game.boss_stage, 1);
    game.step(Buttons::NONE);
    assert!(game.scroll_locked);
    assert_eq!(
        (game.scroll.low, game.scroll.high, game.scroll.fraction),
        (0, 0x0C, 128),
        "the last half-pixel step runs before the lock"
    );

    let mut live_max = 0;
    let mut seen = 0u32;
    for _ in 0..120 {
        game.step(Buttons::NONE);
        let live = game
            .objects
            .iter()
            .filter(|object| object.live() && object.kind == 0x8A)
            .count();
        live_max = live_max.max(live);
        seen += live as u32;
    }
    assert!(live_max >= 2, "several fireballs share the object table");
    assert!(seen > 0, "the eruption keeps spawning through the hold");
    assert_eq!(
        (game.scroll.low, game.scroll.high, game.scroll.fraction),
        (0, 0x0C, 128),
        "`$82` freezes the scroll"
    );
}

#[test]
fn a_kept_hit_on_the_clear_arming_frame_disables_the_spawner() {
    // `$C1D0` clears `$60` when the hit lands on a frame whose `$9A40`
    // threshold already armed `$81`; `$A2C1` then returns at `$A2C5`, so the
    // rest of the stage spawns nothing (the analysis poke never restores it).
    let mut game = sandbox();
    let start = Scroll {
        high: 0,
        low: 0,
        fraction: 0,
    };
    game.spawner.restart(6, 0);
    assert!(
        game.spawner.step(start).is_some(),
        "the stage-7 column-0 stream is live before the hit"
    );
    game.spawner.restart(6, 0);
    game.dev_keep_alive(true);
    game.section = 6;
    game.scroll.high = game.stage_boss_trigger();
    game.begin_death();
    assert!(game.kept_hit, "the kept-alive hit is remembered");
    assert!(
        game.spawner.step(start).is_none(),
        "`$60 = 0` stops the stream"
    );
}
