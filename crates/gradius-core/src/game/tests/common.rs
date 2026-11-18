fn assets() -> Assets {
    Assets {
        tiles: vec![[0u8; 64]; 8 * 256],
        colors: (0..64).map(|_| [0xFF, 0xFF, 0xFF]).collect(),
        rom: Some(crate::rom::testing::rom()),
        stage: Some(crate::assets::pack_stage_assets()),
    }
}

/// A game that has already left the title screen, so gameplay tests are not
/// blocked by the title/attract state.
fn playing() -> Game {
    let mut game = Game::new(assets());
    game.state = State::Playing;
    game.new_run();
    game
}

fn burst_tiles(game: &Game) -> Vec<u8> {
    let mut tiles: Vec<u8> = game
        .build_sprites()
        .iter()
        .map(|sprite| sprite.tile.index & 0xFE)
        .filter(|tile| {
            matches!(
                tile,
                0x74 | 0x76 | 0x78 | 0x7A | 0x7C | 0x7E | 0x80 | 0x82 | 0x84
            )
        })
        .collect();
    tiles.sort_unstable();
    tiles.dedup();
    tiles
}

fn death_tiles(game: &Game) -> Vec<u8> {
    let mut tiles: Vec<u8> = game
        .build_sprites()
        .iter()
        .map(|sprite| sprite.tile.index & 0xFE)
        .filter(|tile| matches!(tile, 0x48 | 0x4A | 0x4C | 0x4E | 0x50 | 0x52 | 0x54 | 0x56))
        .collect();
    tiles.sort_unstable();
    tiles.dedup();
    tiles
}

/// `$977C` entries as button states: bit 0 Right, 1 Left, 2 Down, 3 Up,
/// 4 Start, 5 Select, 6 B, 7 A.
fn continue_presses() -> [Buttons; 10] {
    crate::twoplayer::CONTINUE_CODE.map(|edge| Buttons {
        right: edge & 0x01 != 0,
        left: edge & 0x02 != 0,
        down: edge & 0x04 != 0,
        up: edge & 0x08 != 0,
        start: edge & 0x10 != 0,
        select: edge & 0x20 != 0,
        power: edge & 0x40 != 0,
        fire: edge & 0x80 != 0,
    })
}

/// A 1P game whose last ship has just wrapped to `$FF`: the `$1B = $C0`
/// banner is up and `game_over_timer` is running.
fn at_game_over(score: u32) -> Game {
    let mut game = playing();
    game.phase = Phase::Normal;
    game.score_bcd = Bcd::from_decimal(score);
    game.score = score;
    game.two.slots[0].score = score;
    game.scroll.high = 6;
    game.lives = 0;
    game.begin_death();
    for _ in 0..=120 {
        game.step(Buttons::NONE);
    }
    assert!(game.game_over_timer > 0);
    assert_eq!(game.lives, 0xFF);
    game
}

/// Build a game in normal play with a clean object table and the ship at a
/// known place, so a handler can be driven in isolation.
fn sandbox() -> Game {
    let mut game = playing();
    game.phase = Phase::Normal;
    game.objects = [Object::default(); SLOT_COUNT];
    game.player = Player::new(0x50, 0x60);
    game.scroll_stop = false;
    game
}

/// Stage 5 with its `$14` parent object and two live `$0600` cannons, the
/// `$9658` gate setup.
fn two_cannon_stage_five() -> Game {
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
    game
}

/// First scroll/position whose reconstructed map is solid.
fn solid_spot() -> (Scroll, u8, u8) {
    for high in 0..=0x0C {
        for low in 0..=0xFF {
            let scroll = Scroll {
                fraction: 0,
                low,
                high,
            };
            for y in (0x10u8..=0xC0).step_by(8) {
                for x in (0x10u8..=0xF0).step_by(8) {
                    if terrain_solid(i32::from(x), i32::from(y), scroll) {
                        return (scroll, x, y);
                    }
                }
            }
        }
    }
    panic!("the reconstructed map has solid ground");
}

fn object(kind: u8, x: u8, y: u8) -> Object {
    Object {
        active: true,
        kind,
        x: Fixed::px(x),
        y: Fixed::px(y),
        // `$0120` is nonzero for a live sprite; the dump mirrors the
        // oracle's `$0120 != 0` filter.
        sprite: 1,
        ..Object::default()
    }
}

fn step_to(game: &mut Game, target: u64) {
    while game.frame < target {
        game.step(Buttons::NONE);
    }
}

fn stage_three_turret(addr: u16, subtype: u8, x: u8, y: u8) -> Object {
    Object {
        active: true,
        kind: 0x96,
        subtype,
        x: Fixed::px(x),
        y: Fixed::px(y),
        nametable_addr: addr,
        sprite: 0,
        ..Object::default()
    }
}

/// Assets whose CHR bank 4 is solid colour 1 and every other bank is
/// empty, so a drawn pixel proves which pattern bank the renderer used.
fn banked_assets() -> Assets {
    let tiles = (0..8 * 256)
        .map(|index| {
            let mut data = [0u8; 64];
            if index / 256 == 4 {
                data.fill(1);
            }
            data
        })
        .collect();
    let colors = (0..64).map(|index| [index as u8, 0, 0]).collect();
    Assets {
        tiles,
        colors,
        rom: Some(crate::rom::testing::rom()),
        stage: Some(crate::assets::pack_stage_assets()),
    }
}

/// Drop one capsule on the ship and step until `collide` consumes it.
fn place_capsule(game: &mut Game) {
    game.objects[1] = Object {
        active: true,
        kind: 1,
        subtype: 6,
        x: game.player.x,
        y: game.player.y,
        sprite: 1,
        ..Object::default()
    };
}

fn collect_capsule(game: &mut Game) {
    place_capsule(game);
    game.step(Buttons::NONE);
    assert_eq!(game.objects[1].kind, 0, "the capsule is collected");
}

fn collect_capsules(game: &mut Game, count: u8) {
    for _ in 0..count {
        collect_capsule(game);
    }
}

/// One B edge after the harness's one-frame input latch.
fn press_b(game: &mut Game) {
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    game.step(Buttons::NONE);
}

/// A clean normal-play game reached through the dev start hook.
fn dev_playing() -> Game {
    let mut game = Game::new(assets());
    game.dev_start_run();
    game
}

fn select_and_activate(game: &mut Game, slot: u8) {
    game.dev_power_mut().select(slot);
    game.step(Buttons {
        power: true,
        ..Buttons::NONE
    });
    game.step(Buttons::NONE);
}

fn fixed_px(value: Fixed) -> i32 {
    (i32::from(value.int) << 8) + i32::from(value.frac)
}

fn shot_px(shot: &Shot) -> (i32, i32) {
    (fixed_px(shot.x), fixed_px(shot.y))
}

/// Select OPTION and press B through the latched-input path until the cap.
fn collect_options(game: &mut Game) {
    for _ in 0..6 {
        game.power.select(5);
        game.step(Buttons {
            power: true,
            ..Buttons::NONE
        });
        game.step(Buttons::NONE);
        game.step(Buttons::NONE);
    }
}

/// Assets with a deterministic pixel pattern in every tile and a unique
/// RGB value for every NES palette index. Used by the render tests.
fn patterned_assets() -> Assets {
    let tiles = (0..8 * 256)
        .map(|tile| {
            let mut data = [0u8; 64];
            for (pixel, value) in data.iter_mut().enumerate() {
                *value = ((tile + pixel) % 4) as u8;
            }
            data
        })
        .collect();
    let colors = (0..64)
        .map(|index| [index as u8, (index * 2) as u8, (index * 3) as u8])
        .collect();
    Assets {
        tiles,
        colors,
        rom: Some(crate::rom::testing::rom()),
        stage: Some(crate::assets::pack_stage_assets()),
    }
}

fn rgba_at(game: &Game, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * WIDTH + x) * 4;
    let p = &game.framebuffer.as_rgba()[offset..offset + 4];
    [p[0], p[1], p[2], p[3]]
}

fn title_ship_box(game: &Game) -> Vec<[u8; 4]> {
    let mut pixels = Vec::new();
    for y in 135..146 {
        for x in 70..96 {
            pixels.push(rgba_at(game, x, y));
        }
    }
    pixels
}
