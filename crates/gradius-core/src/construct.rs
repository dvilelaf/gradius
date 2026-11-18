//! Game construction.
//!
//! Moved verbatim from `game.rs` (card R1m): `Game::new` keeps its body and
//! name; the only edit is the module boundary.

use super::*;

impl Game {
    pub fn new(assets: Assets) -> Self {
        let mut rng = Rng(0x5EED_1234);
        let mut stars = [Star {
            x: 0,
            y: 0,
            layer: 0,
        }; 96];
        for (index, star) in stars.iter_mut().enumerate() {
            *star = Star {
                x: rng.range(0, WIDTH as i32) * 256,
                y: rng.range(0, HEIGHT as i32) as i16,
                layer: (index / 32) as u8,
            };
        }
        let spawns = &assets
            .rom
            .expect("the pack has no ROM data; run scripts/extract-data")
            .spawns;
        let mut game = Self {
            assets,
            framebuffer: Framebuffer::new(),
            last_sprites: SpriteList::new(),
            earlier_sprites: SpriteList::new(),
            rng,
            player: Player::new(0x50, 0x60),
            p2: Ship::new(),
            objects: [Object::default(); SLOT_COUNT],
            cannons: [Cannon::default(); CANNON_SLOTS],
            options: [OptionDrone::default(); 4],
            shots: [Shot {
                active: false,
                kind: 0,
                x: Fixed::default(),
                y: Fixed::default(),
                age: 0,
            }; EXTENDED_MAX_SHOTS],
            enemy_bullets: [EnemyBullet {
                x: 0,
                y: 0,
                vx: 0,
                vy: 0,
                kind: 0,
                sprite: 0x3A,
                slot: 0,
            }; MAX_ENEMY_BULLETS],
            enemy_bullet_count: 0,
            bullet_slot_frac: [(0, 0); MAX_ENEMY_BULLETS],
            debris_folds: 0,
            explosions: [Explosion {
                x: 0,
                y: 0,
                life: 0,
                max_life: 0,
            }; MAX_EXPLOSIONS],
            explosion_count: 0,
            stars,
            power: PowerMeter::default(),
            score_bcd: Bcd::default(),
            high_score_bcd: DEFAULT_HIGH_SCORE,
            scroll: Scroll::default(),
            scroll_lag: 1,
            wall_resume: false,
            spawner: stage::Spawner::new(spawns, 0, 0),
            phase: Phase::Blank(tables::STAGE_BLANK_FRAMES),
            section: 0,
            music: SHARED_INTRO_MUSIC,
            stage_clock: 0,
            kills: 0,
            banner: 0,
            prev_fire: false,
            prev_start: false,
            paused: false,
            pause_pending: 0,
            pause_code: [0; 2],
            pause_edges: 0,
            title_timer: 0,
            title_attract_at: TITLE_ATTRACT_ENTRY,
            attract_countdown: 0,
            volcano_kills: 0,
            stage_clear_ready: false,
            cells: Vec::new(),
            collision_cells: Vec::new(),
            broken: vec![0; crate::stage_collision::COLUMNS],
            fresh_breaks: Vec::new(),
            vram_queue_len: 0,
            spawn_busy: false,
            demo: false,
            demo_index: 0,
            demo_ticks: 0,
            demo_button: 0,
            prev_select: false,
            menu_selection: 0,
            players: Players::One,
            two: TwoPlayer::new(),
            menu_blink: 0,
            menu_select_pending: 0,
            scroll_stop: false,
            continue_pending: 0,
            scroll_hold: 0,
            entry_blank: 0,
            latched_input: Buttons::NONE,
            latched_p1: Buttons::NONE,
            latched_p2: Buttons::NONE,
            prev_fire_p2: false,
            prev_pad_p1: Buttons::NONE,
            prev_pad_p2: Buttons::NONE,
            boss_spawned: false,
            dev_keep_alive: false,
            kept_hit: false,
            mods: Mods::default(),
            boss_stage: 0,
            boss_timer: 0,
            hazard_index: 0,
            script_counter: 0,
            boss_clear: 0,
            boss_clear_armed: false,
            clear: Clear::None,
            endgame: Endgame::None,
            loop_count: 0,
            ending_step: 0,
            ending_timer: 0,
            ending_clock: 0,
            ending_text: 0,
            ending_paid: false,
            ending_screen: None,
            ending_ship: EndingShip::default(),
            wall_erase: 0,
            beams: Beams::default(),
            bonus_wave: false,
            scroll_locked: false,
            game_over_timer: 0,
            title_pending: false,
            death_checkpoint: 0,
            death_burst: None,
            simul_respawn: [0; 2],
            score_delay: ScoreDelay::with_pattern(&[5, 4, 3, 8]),
            score_switch: false,
            state: State::Title,
            frame: 0,
            // `$02 = $04` on the first step, less the title screen's missed
            // count, so the first step reads the cartridge's value.
            nmi_frame: 0xFC,
            nmi_lag: false,
            score: 0,
            // No game has started yet; `new_run` loads the ROM's three lives.
            lives: 0,
            audio: AudioQueue::default(),
        };
        if game.state == State::Title {
            game.banner = 0;
        } else {
            game.new_run();
        }
        game.render();
        game
    }
}
