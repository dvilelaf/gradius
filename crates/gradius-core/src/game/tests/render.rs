#[test]
fn the_ppu_shadows_use_the_stage_init_values_and_flip_the_nametable() {
    let mut game = playing();
    assert_eq!(game.ppu_mask(), 0, "blanking");
    assert_eq!(game.ppu_control() & !1, tables::STAGE_PPUCTRL & !1);
    game.scroll.high = 1;
    assert_eq!(game.ppu_control() & 1, 1);
    game.scroll.high = 2;
    assert_eq!(game.ppu_control() & 1, 0);
    game.phase = Phase::Normal;
    assert_eq!(game.ppu_mask(), tables::STAGE_PPUMASK);
}

#[test]
fn the_overlay_is_drawn_after_the_static_screen() {
    let mut game = Game::new_at_stage(patterned_assets(), 2);
    game.scroll.high = 3;
    game.scroll.low = 0x50;
    game.scroll.fraction = 0;
    game.cells.push(stage3_enemies::Cell {
        world_col: 131,
        row: 25,
        tile: 0x3A,
    });
    game.render();
    let screen = crate::stage_bg::background(crate::rom::testing::rom(), 2).unwrap();
    let (row, col) = (25usize, 131usize);
    let attribute_columns = screen.width_tiles / 4;
    let attribute = screen.attributes[(row / 4) * attribute_columns + col / 4];
    let quadrant = ((row & 2) >> 1) * 2 + ((col & 2) >> 1);
    let palette = usize::from((attribute >> (quadrant * 2)) & 3);
    let (_, entries) = tile_style(screen, palette, false);
    let expected = game.assets.color(entries[usize::from(0x3Au8 % 4)]);
    // The renderer lags the scroll latch by one pixel, exactly like the
    // static screen.
    assert_eq!(
        rgba_at(&game, 131 * 8 - 847, (25 * 8 + STAGE_MAP_Y) as usize),
        [expected[0], expected[1], expected[2], 0xFF]
    );
}

#[test]
fn stage_two_tail_uses_chr_bank_two_and_black_palettes() {
    let screen = crate::stage_bg::background(crate::rom::testing::rom(), 1).unwrap();
    let entries = |start: usize| {
        [
            screen.palette[start],
            screen.palette[start + 1],
            screen.palette[start + 2],
            screen.palette[start + 3],
        ]
    };
    // Palettes 0-1 keep their colours on both sides of the cut.
    assert_eq!(tile_style(screen, 0, false), (0, entries(0)));
    assert_eq!(tile_style(screen, 0, true), (4, entries(0)));
    assert_eq!(tile_style(screen, 1, false), (0, entries(4)));
    assert_eq!(tile_style(screen, 1, true), (4, entries(4)));
    // The tail forces palette RAM entries 8-15 to `$0F`.
    assert_eq!(tile_style(screen, 2, false), (0, entries(8)));
    assert_eq!(tile_style(screen, 2, true), (4, [BLACK; 4]));
    assert_eq!(tile_style(screen, 3, false), (0, entries(12)));
    assert_eq!(tile_style(screen, 3, true), (4, [BLACK; 4]));

    // Render check: only CHR bank 4 has pixels in `banked_assets`, so a
    // visible tail tile proves the renderer fetched the tail's bank.
    let mut game = Game::new_at_stage(banked_assets(), 1);
    let probe = (2..=26usize)
        .flat_map(|row| {
            (game.stage_info().tail_column.unwrap()..screen.width_tiles)
                .map(move |column| (column, row))
        })
        .find(|&(column, row)| screen.palette_index(column, row) < 2)
        .expect("a tail tile with palette 0-1");
    let (column, row) = probe;
    let position = (column * 8 + 1) as u16;
    game.scroll.high = (position >> 8) as u8;
    game.scroll.low = position as u8;
    game.scroll.fraction = 0;
    game.render();
    let palette = usize::from(screen.palette_index(column, row));
    let expected = game.assets.color(tile_style(screen, palette, true).1[1]);
    assert_ne!(expected, game.assets.color(BLACK), "bank 4 pays colour 1");
    assert_eq!(
        rgba_at(&game, 0, ((row * 8) as i32 + STAGE_MAP_Y) as usize),
        [expected[0], expected[1], expected[2], 0xFF],
        "the tail fetches CHR bank 2 (pack bank 4)"
    );
}

#[test]
fn stage_three_tail_uses_chr_bank_two_and_black_palettes() {
    let screen = crate::stage_bg::background(crate::rom::testing::rom(), 2).unwrap();
    // The catalog row carries the tail quirk; the cut is column 416.
    assert_eq!(
        Game::new_at_stage(patterned_assets(), 2)
            .stage_info()
            .quirks,
        stage_catalog::QUIRK_TAIL
    );
    assert_eq!(tile_style(screen, 0, true).0, 4);
    assert_eq!(
        tile_style(screen, 1, true),
        (
            4,
            [
                screen.palette[4],
                screen.palette[5],
                screen.palette[6],
                screen.palette[7]
            ]
        )
    );
    assert_eq!(tile_style(screen, 2, true), (4, [BLACK; 4]));
    assert_eq!(tile_style(screen, 3, true), (4, [BLACK; 4]));

    let mut game = Game::new_at_stage(banked_assets(), 2);
    let (column, row) = (2..=26usize)
        .flat_map(|row| {
            (game.stage_info().tail_column.unwrap()..screen.width_tiles)
                .map(move |column| (column, row))
        })
        .find(|&(column, row)| screen.palette_index(column, row) < 2)
        .expect("a tail tile with palette 0-1");
    let position = (column * 8 + 1) as u16;
    game.scroll.high = (position >> 8) as u8;
    game.scroll.low = position as u8;
    game.scroll.fraction = 0;
    game.render();
    let palette = usize::from(screen.palette_index(column, row));
    let expected = game.assets.color(tile_style(screen, palette, true).1[1]);
    assert_ne!(expected, game.assets.color(BLACK), "bank 4 pays colour 1");
    assert_eq!(
        rgba_at(&game, 0, ((row * 8) as i32 + STAGE_MAP_Y) as usize),
        [expected[0], expected[1], expected[2], 0xFF],
        "the stage-3 tail fetches CHR bank 2 (pack bank 4)"
    );
}

#[test]
fn stages_four_and_five_render_the_tail_from_their_own_column() {
    // `stage_bgs` records stage 4's switch at world column 416 and stage
    // 5's at 384: both go to CHR bank 2 and force palette RAM entries
    // 8-15 to `$0F`, keeping 0-7.
    for (stage, cut) in [(3u8, 416usize), (4, 384)] {
        let screen = crate::stage_bg::background(crate::rom::testing::rom(), usize::from(stage)).unwrap();
        let info = &stage_catalog::STAGES[usize::from(stage)];
        assert_eq!(info.tail_column, Some(cut));
        assert_ne!(info.quirks & stage_catalog::QUIRK_TAIL, 0);
        // The stage's own palette entries 9-11 are real colours, so the
        // black comparison proves the tail forces them (entry 8 is the
        // shared `$0F` backdrop).
        assert_ne!(screen.palette[9], BLACK);
        assert_eq!(tile_style(screen, 2, true), (4, [BLACK; 4]));
        assert_eq!(tile_style(screen, 3, true), (4, [BLACK; 4]));
        assert_eq!(
            tile_style(screen, 2, false).1,
            [
                screen.palette[8],
                screen.palette[9],
                screen.palette[10],
                screen.palette[11]
            ]
        );

        // Hardware bank: the dominant one a pixel before the cut, bank 2
        // from the cut on.
        let mut game = Game::new_at_stage(banked_assets(), stage);
        let before = (cut * 8 - 1) as u16;
        game.scroll.high = (before >> 8) as u8;
        game.scroll.low = before as u8;
        game.scroll.fraction = 0;
        assert_eq!(game.hardware_bank(), screen.chr_bank, "before the cut");
        let at = (cut * 8) as u16;
        game.scroll.high = (at >> 8) as u8;
        game.scroll.low = at as u8;
        game.scroll.fraction = 0;
        assert_eq!(game.hardware_bank(), STAGE_TAIL_BANK, "at the cut");

        // Render check: only CHR bank 4 (tail bank 2) has pixels in
        // `banked_assets`, so a visible tail tile proves the fetch.
        let (column, row) = (2..=26usize)
            .flat_map(|row| (cut..screen.width_tiles).map(move |column| (column, row)))
            .find(|&(column, row)| screen.palette_index(column, row) < 2)
            .expect("a tail tile with palette 0-1");
        let position = (column * 8 + 1) as u16;
        game.scroll.high = (position >> 8) as u8;
        game.scroll.low = position as u8;
        game.scroll.fraction = 0;
        game.render();
        let palette = usize::from(screen.palette_index(column, row));
        let expected = game.assets.color(tile_style(screen, palette, true).1[1]);
        assert_ne!(expected, game.assets.color(BLACK), "bank 4 pays colour 1");
        assert_eq!(
            rgba_at(&game, 0, ((row * 8) as i32 + STAGE_MAP_Y) as usize),
            [expected[0], expected[1], expected[2], 0xFF],
            "stage {} tail fetches CHR bank 2 (pack bank 4)",
            stage + 1
        );
    }
}

#[test]
fn the_stage_map_origin_is_12_px_and_the_latch_is_one_frame_behind() {
    let mut game = Game::new(patterned_assets());
    game.begin_stage(false);
    for _ in 0..12 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.phase, Phase::Normal);
    let screen = crate::stage_bg::background(crate::rom::testing::rom(), 0).unwrap();
    let tile_pixel = |game: &Game, tile_x: usize, row: usize, column: usize| {
        let tile = screen.tile(tile_x, row);
        let palette_index = screen.palette_index(tile_x, row);
        let value = game.assets.tile(screen.chr_bank * 2, tile).unwrap()[column];
        game.assets
            .color(screen.palette[usize::from(palette_index) * 4 + usize::from(value)])
    };
    let pixel = |game: &Game, x: usize, y: usize| {
        let offset = (y * WIDTH + x) * 4;
        let rgba = game.framebuffer.as_rgba();
        [rgba[offset], rgba[offset + 1], rgba[offset + 2]]
    };

    // Find a latch value where the one-frame-late tile differs from the
    // current `$3E`, so the horizontal origin is actually exercised. The
    // latch only lags on a frame that steps the scroll, so the search
    // steps `$3E` from `candidate - 1` to `candidate` instead of writing
    // the register and rendering.
    let mut low = 0u8;
    for candidate in 1..64u8 {
        game.scroll.high = 0;
        game.scroll.low = candidate - 1;
        game.scroll.fraction = 128;
        game.normal_frame(Buttons::NONE, false);
        let shown = usize::from(candidate) - 1;
        assert_eq!((game.scroll.low, game.scroll.fraction), (candidate, 0));
        game.render();
        let latch = tile_pixel(&game, shown / 8, 0, shown % 8);
        let current = tile_pixel(
            &game,
            usize::from(candidate) / 8,
            0,
            usize::from(candidate) % 8,
        );
        if latch != current {
            low = candidate;
            break;
        }
    }
    assert_ne!(low, 0, "no latch/current pixel pair differs");
    game.scroll.high = 0;
    game.scroll.low = low - 1;
    game.scroll.fraction = 128;
    game.normal_frame(Buttons::NONE, false);
    game.render();
    let shown = usize::from(low) - 1;

    // `$4020`: the map's first row starts 12 px above the screen, so the
    // visible top is map row 1 at column 4.
    let origin = usize::try_from(-STAGE_MAP_Y).unwrap();
    let origin_row = origin / 8;
    let origin_column = origin % 8;
    assert_eq!(
        pixel(&game, 0, 0),
        tile_pixel(&game, shown / 8, origin_row, origin_column)
    );
    // The next map row starts at screen y = 8 - origin_column.
    assert_eq!(
        pixel(&game, 0, 8 - origin_column),
        tile_pixel(&game, shown / 8, origin_row + 1, 0)
    );
}

#[test]
fn background_scroll_has_no_seam_and_uses_the_tile_at_the_scroll_offset() {
    let mut game = Game::new(patterned_assets());
    game.begin_stage(false);
    for _ in 0..12 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.phase, Phase::Normal);
    let screen = crate::stage_bg::background(crate::rom::testing::rom(), 0).unwrap();
    for fine in 0..8u8 {
        game.scroll.low = fine;
        game.scroll.fraction = 0;
        game.render();
        // A render with no scroll step shows the current `$3E`, so the
        // tile sample uses `scroll`; the map starts 12 px above the
        // screen, so the top visible row is map row 1 at y = 4.
        let shown = usize::from(fine);
        let origin = usize::try_from(-STAGE_MAP_Y).unwrap();
        let row = origin / 8 + 1;
        let y = 8 - origin % 8;
        for x in 0..WIDTH as i32 {
            let tile_x = (usize::try_from(x).unwrap() + shown) / 8;
            if tile_x >= screen.width_tiles {
                continue;
            }
            let tile = screen.tile(tile_x, row);
            let palette_index = screen.palette_index(tile_x, row);
            let column = (usize::try_from(x).unwrap() + shown) % 8;
            let pixel = game.assets.tile(screen.chr_bank * 2, tile).unwrap()[column];
            let expected = game
                .assets
                .color(screen.palette[usize::from(palette_index) * 4 + usize::from(pixel)]);
            let offset = (y * WIDTH + x as usize) * 4;
            assert_eq!(
                &game.framebuffer.as_rgba()[offset..offset + 3],
                &expected,
                "fine={fine} x={x}"
            );
        }
    }
}

#[test]
fn a_held_scroll_draws_the_current_latch_not_the_previous_column() {
    let mut game = Game::new(patterned_assets());
    game.begin_stage(false);
    for _ in 0..12 {
        game.step(Buttons::NONE);
    }
    assert_eq!(game.phase, Phase::Normal);
    let screen = crate::stage_bg::background(crate::rom::testing::rom(), 0).unwrap();
    let tile_pixel = |game: &Game, tile_x: usize, row: usize, column: usize| {
        let tile = screen.tile(tile_x, row);
        let palette_index = screen.palette_index(tile_x, row);
        let value = game.assets.tile(screen.chr_bank * 2, tile).unwrap()[column];
        game.assets
            .color(screen.palette[usize::from(palette_index) * 4 + usize::from(value)])
    };
    let pixel = |game: &Game, x: usize, y: usize| {
        let offset = (y * WIDTH + x) * 4;
        let rgba = game.framebuffer.as_rgba();
        [rgba[offset], rgba[offset + 1], rgba[offset + 2]]
    };

    // `$5B` holds `$3E` during the eruption, so the latch is the current
    // scroll and the left edge is column `scroll / 8`, not one pixel back.
    let row = 2;
    let mut column = None;
    for candidate in 1..96usize {
        let current = tile_pixel(&game, candidate, row, 0);
        let previous = tile_pixel(&game, candidate - 1, row, 7);
        if current != previous {
            column = Some(candidate);
            break;
        }
    }
    let column = column.expect("a column whose first pixel differs from its predecessor");
    let position = (column * 8) as u16;
    game.scroll.high = (position >> 8) as u8;
    game.scroll.low = position as u8;
    game.scroll.fraction = 0;
    game.scroll_locked = true;
    game.normal_frame(Buttons::NONE, false);
    game.render();

    // Map row `r` starts at `y = 8*r - 12`, so row 2 starts at y = 4.
    assert_eq!(
        pixel(&game, 0, 4),
        tile_pixel(&game, column, row, 0),
        "a held scroll must not render one column behind"
    );
}

#[test]
fn oam_order_puts_the_hud_bar_first_then_ship_then_ascending_slots() {
    let mut game = sandbox();
    game.objects[9] = object(0x05, 0x40, 0x40);
    game.objects[9].sprite = 0x05;
    game.objects[3] = object(0x05, 0x60, 0x40);
    game.objects[3].sprite = 0x05;
    let sprites = game.build_sprites();
    assert_eq!(sprites.get(0).unwrap().tile, art::HUD_BAR);
    // `$0120 = 1`: the ship group is four sprites, nose to rear, plus the
    // `$DE/$DF` part eight pixels above and twelve left of the rear.
    let px = i32::from(game.player.x.int);
    assert_eq!(sprites.get(1).unwrap().tile.index, 0x0D & 0xFE);
    assert_eq!(sprites.get(1).unwrap().x, px + 8);
    assert_eq!(sprites.get(2).unwrap().x, px);
    assert_eq!(sprites.get(3).unwrap().x, px - 8);
    assert_eq!(sprites.get(4).unwrap().tile.index, 0xDE);
    assert_eq!(sprites.get(4).unwrap().x, px - 12);
    // Object slot 3 precedes slot 9, both after the ship. Group `$05` is
    // two sprites at `dx = 0` and `dx = -8`.
    assert_eq!(sprites.get(5).unwrap().x, 0x60);
    assert_eq!(sprites.get(6).unwrap().x, 0x58);
    assert_eq!(sprites.get(7).unwrap().x, 0x40);
    assert_eq!(sprites.get(8).unwrap().x, 0x38);
}

#[test]
fn a_ring_enemy_expands_to_its_two_sprite_definition() {
    let mut game = sandbox();
    // `$0120 = $0D`: the two-column ring enemy, palette 2, behind.
    game.objects[5] = object(0x85, 0x50, 0x40);
    game.objects[5].sprite = 0x0D;
    let sprites = game.build_sprites();
    assert_eq!(sprites.len(), 7, "HUD + four ship parts + two ring parts");
    let left = sprites.get(5).unwrap();
    let right = sprites.get(6).unwrap();
    assert_eq!((left.x, left.y), (0x50, 0x41));
    assert_eq!((right.x, right.y), (0x58, 0x41));
    assert_eq!(left.tile.index, 0x19 & 0xFE);
    assert_eq!(right.tile.index, 0x1B & 0xFE);
    assert!(left.behind && right.behind);
    assert!(!left.flip_h && !left.flip_v);
    assert_eq!(left.palette, palette(&game.assets, &crate::palettes::enemy(&crate::rom::testing::rom().tables)));
    assert_eq!(right.palette, palette(&game.assets, &crate::palettes::enemy(&crate::rom::testing::rom().tables)));
}

#[test]
fn the_oam_ring_base_advances_before_each_build() {
    // `$2F` at frames 649..651, as measured from the oracle dump.
    assert_eq!(oam_ring_base(649), 0xE0);
    assert_eq!(oam_ring_base(650), 0x24);
    assert_eq!(oam_ring_base(651), 0x68);
}

#[test]
fn each_cannon_part_sways_on_its_own_phase_clamped_to_the_previous_part() {
    // `$CC93`: `$A9` walks `$0611,X` with the part, so the six records hold
    // one phase each. `$CCF1` rewrites `$9B`/`$9C` from the part just
    // placed, so every later part is clamped to the previous phase +/- 6
    // rather than to the `$CC19`/`$CC1B` range.
    let mut game = sandbox();
    game.section = 4;
    game.nmi_frame = 0;
    game.objects[9] = object(0x14, 0x80, 0x40);
    game.cannons[1] = Cannon {
        active: true,
        parent: 9,
        index: 0,
        ..Cannon::default()
    };
    // `$0603` starts at zero, so the first call only decrements it.
    game.update_cannons();
    assert_eq!(game.cannons[1].part_counter, 0xFF);
    assert_eq!(game.cannons[1].phase, [0; CANNON_PARTS]);
    game.update_cannons();
    // First placement: part 1 clamps to `$CC19[0] = $4A`, then each later
    // part is clamped to `$-6`/`+6` of the one before and starts at 0, so
    // the run descends `$4A, $44, $3E`, then `$CCCA` pins part 4 to `$44`
    // because its previous Y (0) is above the ship at `$60`.
    assert_eq!(game.cannons[1].phase, [0x00, 0x4A, 0x44, 0x3E, 0x44, 0x3E]);
}

#[test]
fn the_cannon_phase_pins_to_the_ship_row() {
    // `$CCCA`: a phase at or past `$84` on an odd record with its part at or
    // below the ship rewrites to `$7C`; a phase below `$3C` on an even record
    // with its part above the ship rewrites to `$44`.
    let mut game = sandbox();
    game.section = 4;
    game.nmi_frame = 0;
    game.player = Player::new(0x00, 0x60);
    game.objects[9] = object(0x14, 0x80, 0x40);
    game.cannons[1] = Cannon {
        active: true,
        parent: 9,
        index: 1,
        part_counter: 1,
        phase: [0, 0x70, 0x76, 0x7E, 0x84, 0x88],
        part_x: [0x80; CANNON_PARTS],
        part_y: [0x80; CANNON_PARTS],
        ..Cannon::default()
    };
    game.update_cannons();
    assert_eq!(game.cannons[1].phase[5], 0x7C, "the odd record's high pin");
}

#[test]
fn the_sprite_list_never_exceeds_oam_and_flickers_like_the_ppu() {
    let mut list = SpriteList::new();
    let sprite = Sprite {
        x: 0,
        y: 16,
        tile: art::ENEMY_SMALL,
        palette: [[1, 2, 3]; 4],
        flip_h: false,
        flip_v: false,
        behind: false,
    };
    for _ in 0..(crate::video::MAX_SPRITES + 8) {
        list.push(sprite);
    }
    assert_eq!(list.len(), crate::video::MAX_SPRITES);
    let masks = list.row_masks();
    assert_eq!(
        masks.iter().take(8).filter(|mask| **mask == 0xFFFF).count(),
        8
    );
    assert!(masks.iter().skip(8).all(|mask| *mask == 0));
}

/// A 2P image must restore P2's bank and `$18`, not just P1's mirror. The
/// image is captured with P2 flying and distinct banks, the source then
/// advances; the restore must reproduce `dump_state`, the whole
/// `TwoPlayer` and an identical image, and 300 more 2P frames must stay
/// byte-identical.
#[test]
fn a_two_player_image_restores_both_banks_and_replays_exactly() {
    let p1 = Buttons {
        fire: true,
        right: true,
        ..Buttons::NONE
    };
    let p2 = Buttons {
        fire: true,
        left: true,
        ..Buttons::NONE
    };

    let mut game = playing();
    game.players = Players::Two;
    game.new_run();
    game.phase = Phase::Normal;
    // `$18 = 1`: P2 is flying, and both banks differ.
    game.two.current = 1;
    game.two.slots[0].lives = 2;
    game.two.slots[0].score = 332_211;
    game.two.slots[0].checkpoint = crate::twoplayer::Checkpoint::new(crate::rom::testing::rom(), 1, 4, 1);
    game.two.slots[1].lives = 1;
    game.two.slots[1].score = 665_544;
    game.two.slots[1].checkpoint = crate::twoplayer::Checkpoint::new(crate::rom::testing::rom(), 2, 6, 1);
    game.lives = 2;
    game.score = 332_211;
    for _ in 0..40 {
        game.step_players(p1, p2);
    }

    let dump = game.dump_state();
    let banks = *game.two_player();
    let image = game.snapshot();
    // Time passes before the restore; the image is still the state.
    for _ in 0..30 {
        game.step_players(p1, p2);
    }

    let mut restored = Game::new(assets());
    restored.restore(&image).unwrap();
    assert_eq!(restored.dump_state(), dump);
    assert_eq!(*restored.two_player(), banks);
    assert_eq!(restored.snapshot(), image);
    assert_eq!(restored.players(), Players::Two);
    assert_eq!(restored.current_player(), 1, "`$18`");
    assert_eq!(
        restored.two_player().slots.map(|slot| slot.score),
        [332_211, 665_544]
    );
    assert_eq!(restored.two_player().slots.map(|slot| slot.lives), [2, 1]);

    // 300 frames against the source game, restored from the same image.
    let image = game.snapshot();
    let mut restored = Game::new(assets());
    restored.restore(&image).unwrap();
    for frame in 0..300 {
        game.step_players(p1, p2);
        restored.step_players(p1, p2);
        assert_eq!(
            restored.dump_state(),
            game.dump_state(),
            "dump diverged {} frames after the 2P restore",
            frame + 1
        );
        assert_eq!(
            *restored.two_player(),
            *game.two_player(),
            "turn machine diverged {} frames after the 2P restore",
            frame + 1
        );
        assert_eq!(
            restored.snapshot(),
            game.snapshot(),
            "save image diverged {} frames after the 2P restore",
            frame + 1
        );
    }
}

#[test]
fn the_stage_five_cannon_arms_draw_six_parts_per_record() {
    // `$CB8B` places six parts per `$0600` record; `$8BDE`/`$8C0B` writes
    // them after the object slots. Positions and the part-5 tip below are the
    // oracle's at stage frame 3000 (record `$0690`).
    let mut game = sandbox();
    game.section = 4;
    game.cannons[3] = Cannon {
        active: true,
        parent: 9,
        phase: [0, 0, 0, 0, 0, 0x67],
        part_x: [0x58, 0x5E, 0x66, 0x6F, 0x76, 0x7D],
        part_y: [0x1C, 0x15, 0x12, 0x13, 0x1A, 0x20],
        ..Cannon::default()
    };
    let sprites = game.build_sprites();
    let arms: Vec<_> = sprites
        .iter()
        .filter(|sprite| matches!(sprite.tile.index, 0xF6 | 0xFA))
        .collect();
    assert_eq!(arms.len(), 6, "one 8x16 part per placement");
    for (part, y) in [0x1D, 0x16, 0x13, 0x14, 0x1B].into_iter().enumerate() {
        let sprite = arms[part];
        assert_eq!(sprite.x, [0x58, 0x5E, 0x66, 0x6F, 0x76][part]);
        assert_eq!(sprite.y, y, "the stored Y byte is drawn one lower");
        assert_eq!(sprite.tile.index, 0xF6, "the `$F7` segment");
        assert_eq!(sprite.tile.bank, 3, "stage 5 background CHR bank 1");
        assert!(!sprite.flip_h && !sprite.flip_v);
    }
    // `$0615 = $67`: tip tile `$8BF7[9] = $FB`, attribute `$8C07[2] = $C2`
    // (palette 2, both flips), and the vertical flip moves the stored Y by 8.
    let tip = arms[5];
    assert_eq!((tip.x, tip.y), (0x7D, 0x19));
    assert_eq!(tip.tile.index, 0xFA);
    assert_eq!(tip.tile.bank, 3);
    assert!(tip.flip_h && tip.flip_v);
}

#[test]
fn cannon_arms_belong_to_stage_five_and_skip_off_screen_parts() {
    let mut game = sandbox();
    game.cannons[3] = Cannon {
        active: true,
        parent: 9,
        phase: [0, 0, 0, 0, 0, 0],
        part_x: [0, 0x5E, 0xF4, 0x6F, 0x76, 0x7D],
        part_y: [0x10, 0x15, 0x12, 0x13, 0x1A, 0x20],
        ..Cannon::default()
    };
    game.section = 3;
    assert!(
        !game
            .build_sprites()
            .iter()
            .any(|sprite| matches!(sprite.tile.index, 0xF6 | 0xF8 | 0xFA | 0xFC)),
        "`$8B92` draws the arms only when `$19 == 4`"
    );
    game.section = 4;
    let arms = game
        .build_sprites()
        .iter()
        .filter(|sprite| matches!(sprite.tile.index, 0xF6 | 0xF8 | 0xFA | 0xFC))
        .count();
    assert_eq!(arms, 4, "X bytes 0 and `$F4` are not written");
}

#[test]
fn an_erased_cell_leaves_the_ship_behind_it_visible() {
    let mut game = Game::new_at_stage(patterned_assets(), 2);
    game.phase = Phase::Normal;
    game.scroll.high = 3;
    game.scroll.low = 0x50;
    game.scroll.fraction = 0;
    game.player.x = Fixed::px(100);
    game.player.y = Fixed::px(100);
    // The ship is a behind-priority sprite: only a blank nametable cell lets
    // it show. `$CDAD`'s tile 0 cells erase one this way.
    let hull = game.assets.color(crate::palettes::stage_sprite(&game.rom().tables)[0][1]);
    let ship_pixels = |game: &Game| -> usize {
        (97..128)
            .flat_map(|x| (100..116).map(move |y| (x, y)))
            .filter(|&(x, y)| rgba_at(game, x, y)[..3] == hull)
            .count()
    };
    // The map tile is mostly opaque, so only its colour 0 pixels show the ship.
    game.render();
    game.render();
    game.render();
    let painted = ship_pixels(&game);
    for world_col in 118..=121 {
        for row in 12..=16 {
            game.cells.push(stage3_enemies::Cell {
                world_col,
                row,
                tile: 0,
            });
        }
    }
    game.render();
    game.render();
    game.render();
    assert!(
        ship_pixels(&game) > painted,
        "the erased cell is transparent, so more of the ship shows through"
    );
    // Nothing paints the erased cells: no tile colour and no fill is left.
    let black = game.assets.color(BLACK);
    let ship_colors: Vec<[u8; 3]> = game
        .build_sprites()
        .iter()
        .flat_map(|sprite| sprite.palette)
        .collect();
    for x in 97..128 {
        for y in 100..116 {
            let pixel = rgba_at(&game, x, y);
            assert!(
                pixel[..3] == black || ship_colors.contains(&[pixel[0], pixel[1], pixel[2]]),
                "the erased cell leaves only the background and the ship: {pixel:?} at {x},{y}"
            );
        }
    }
}
