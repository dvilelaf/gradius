//! Save states: the `GRSS` image writer and reader.
//!
//! Moved verbatim from `game.rs` (card R1j). Every function keeps its name and
//! its body; the only edits are the `use` lines the module boundary needs.

use super::render::ending_replay_steps;
use super::second_ship::{read_ship, write_ship};
use super::*;
use crate::stage6_beams::Beam;

impl Game {
    /// Capture the whole simulation as a `GRSS` save image.
    ///
    /// CORE holds the fixed-width fields written by [`write_core`]. The OAM
    /// chunk carries `earlier_sprites` with its OAM slots and ring flag, the
    /// EXT chunk carries the state owned by helper modules: the spawner's
    /// private RAM, [`ScoreDelay`], the ending-screen replay count and the
    /// stage-6 [`Beams`] boxes, and
    /// PLAYERS carries the `TwoPlayer` turn machine. Older images without the
    /// OAM tail, the EXT chunk or PLAYERS still decode; their spawner/
    /// score-delay state is left at whatever [`Game::new`] set, and the
    /// player banks fall back to CORE's P1 mirror.
    pub fn snapshot(&self) -> Vec<u8> {
        let mut core = crate::snapshot::Writer::new();
        self.write_core(&mut core);
        let mut oam = crate::snapshot::Writer::new();
        let (slotted, slots) = self.earlier_sprites.slot_layout();
        oam.u8(self.earlier_sprites.len() as u8);
        for sprite in self.earlier_sprites.iter() {
            oam.i32(sprite.x);
            oam.i32(sprite.y);
            oam.u8(sprite.tile.bank);
            oam.u8(sprite.tile.index);
            for row in sprite.palette {
                for color in row {
                    oam.u8(color);
                }
            }
            oam.bool(sprite.flip_h);
            oam.bool(sprite.flip_v);
            oam.bool(sprite.behind);
        }
        // Tail added after v1: older images stop before it.
        oam.bool(slotted);
        for &slot in slots {
            oam.u8(slot);
        }
        let mut ext = crate::snapshot::Writer::new();
        let spawner = self.spawner.state();
        ext.u8(spawner.section);
        ext.u8(spawner.column_base);
        ext.u32(spawner.cursor);
        ext.bool(spawner.pending.is_some());
        let pending = spawner.pending.unwrap_or_default();
        ext.u8(pending.remaining);
        ext.u8(pending.interval);
        ext.u8(pending.y);
        ext.u8(pending.y_step);
        ext.u8(pending.kind);
        ext.u8(pending.subtype);
        ext.u8(pending.substate);
        ext.u8(pending.x);
        ext.u8(pending.motion);
        ext.u8(pending.phase);
        ext.bool(pending.attempted);
        ext.u8(spawner.timer);
        ext.u8(spawner.spawn_count);
        ext.u8(spawner.cannon);
        for value in spawner.class_counts {
            ext.u8(value);
        }
        ext.bool(spawner.init_pending);
        ext.bool(spawner.record_read);
        ext.bool(spawner.disabled);
        let delay = self.score_delay.state();
        for value in delay.shown {
            ext.u8(value);
        }
        for value in delay.pending {
            ext.u8(value);
        }
        ext.u8(delay.delay);
        // `pattern_id`: 1 is the measured `[5, 4, 3, 8]` cycle.
        ext.u8(1);
        ext.u32(delay.index);
        ext.u8(delay.countdown);
        ext.u8(ending_replay_steps(self.ending_step, self.ending_timer));
        // Tail added after v1: stage 6's ten collision-only beam boxes. An
        // older image without it restores them empty.
        for beam in self.beams.slots {
            ext.bool(beam.active);
            ext.i16(beam.x);
            ext.i16(beam.y);
            ext.u8(beam.lane);
        }
        // Optional motion tail: the 8.8 velocity and fraction per beam. An
        // image written before it keeps the boxes with a zero velocity.
        for beam in self.beams.slots {
            ext.u16(beam.y_speed);
            ext.u8(beam.y_frac);
        }
        // Tail added with the Start pause, written only while it matters so an
        // unpaused image keeps the older layout: `$15` and an edge in flight.
        if self.paused
            || self.pause_pending > 0
            || self.pause_code != [0; 2]
            || self.pause_edges != 0
        {
            ext.bool(self.paused);
            ext.u8(self.pause_pending);
            ext.u8(self.pause_code[0]);
            ext.u8(self.pause_code[1]);
            ext.u8(self.pause_edges);
        }
        let mut players = crate::snapshot::Writer::new();
        self.two.write_state(&mut players);
        // Simultaneous 2P only: the second ship's kit. Images from a
        // cartridge-parity game have no SIMUL chunk, and neither do any
        // images written before this mod existed.
        let mut simul = crate::snapshot::Writer::new();
        if self.simultaneous() {
            write_ship(&mut simul, &self.p2);
            simul.u8(self.simul_respawn[0]);
            simul.u8(self.simul_respawn[1]);
        }
        let mut cells = crate::snapshot::Writer::new();
        cells.u32(self.cells.len() as u32);
        for cell in &self.cells {
            cells.u16(cell.world_col);
            cells.u8(cell.row);
            cells.u8(cell.tile);
        }
        let mut collision = crate::snapshot::Writer::new();
        collision.u32(self.collision_cells.len() as u32);
        for patch in &self.collision_cells {
            collision.u16(patch.addr);
            collision.u8(patch.value);
        }
        // Tail after the patches: the broken destructible cells, as the
        // non-zero world columns. Older images stop after the patches.
        let columns: Vec<(usize, u32)> = self
            .broken
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, mask)| mask != 0)
            .collect();
        collision.u16(columns.len() as u16);
        for (column, mask) in columns {
            collision.u16(column as u16);
            collision.u32(mask);
        }
        collision.u8(self.fresh_breaks.len() as u8);
        for &(column, row) in &self.fresh_breaks {
            collision.u16(column);
            collision.u8(row);
        }
        crate::snapshot::GameState {
            version: crate::snapshot::VERSION,
            tag: crate::snapshot::TAG,
            frame: self.frame,
            core: core.buf,
            oam: oam.buf,
            cells: cells.buf,
            collision: collision.buf,
            ext: ext.buf,
            players: players.buf,
            simul: simul.buf,
        }
        .to_bytes()
    }

    /// Restore a `GRSS` image written by [`Game::snapshot`].
    ///
    /// The image is fully validated by the codec and [`crate::snapshot`]
    /// before any field is touched. `audio` is cleared and `last_sprites` is
    /// rebuilt; `earlier_sprites` comes from the OAM chunk.
    ///
    /// The PLAYERS chunk restores the `$18`/`$1B` turn machine and, in a 2P
    /// game, both player banks, so a 2P image returns P2's score, lives and
    /// checkpoints. In 1P the CORE P1 mirror stays the authority for `$20` and
    /// the score (`$20` is zero in attract while the dormant machine still
    /// says three), so PLAYERS never overwrites them. An image without the
    /// chunk restores only CORE's P1 mirror, as it always did.
    pub fn restore(&mut self, bytes: &[u8]) -> Result<(), crate::save::SaveError> {
        const CORE_LEN: usize = 2_525;
        let state = crate::snapshot::GameState::from_bytes(bytes)?;
        if state.core.len() != CORE_LEN {
            return Err(crate::save::SaveError::BadPayloadLength);
        }
        let mut r = crate::snapshot::Reader::new(&state.core);
        self.read_core(&mut r)?;
        r.finish()?;
        // Images from before the Start pause carry no flag: they run.
        self.paused = false;
        self.pause_pending = 0;
        self.pause_code = [0; 2];
        self.pause_edges = 0;

        let mut oam = crate::snapshot::Reader::new(&state.oam);
        let len = usize::from(oam.u8()?);
        if len > crate::video::MAX_SPRITES {
            return Err(crate::save::SaveError::BadPayloadLength);
        }
        let mut earlier = SpriteList::new();
        for _ in 0..len {
            let x = oam.i32()?;
            let y = oam.i32()?;
            let bank = oam.u8()?;
            let index = oam.u8()?;
            let mut palette = [[0u8; 3]; 4];
            for row in &mut palette {
                for color in row {
                    *color = oam.u8()?;
                }
            }
            earlier.push(Sprite {
                x,
                y,
                tile: crate::assets::TileRef { bank, index },
                palette,
                flip_h: oam.bool()?,
                flip_v: oam.bool()?,
                behind: oam.bool()?,
            });
        }
        // Optional tail: `slotted` plus one slot per entry. Images written
        // before it default to insertion order, as they did when captured.
        if oam.remaining() > 0 {
            let slotted = oam.bool()?;
            let mut slots = Vec::with_capacity(len);
            for _ in 0..len {
                slots.push(oam.u8()?);
            }
            earlier.set_slot_layout(slotted, &slots);
        }
        oam.finish()?;

        let mut ext = crate::snapshot::Reader::new(&state.ext);
        if !state.ext.is_empty() {
            let spawner = stage::SpawnerState {
                section: ext.u8()?,
                column_base: ext.u8()?,
                cursor: ext.u32()?,
                pending: {
                    let present = ext.bool()?;
                    let pending = stage::PendingState {
                        remaining: ext.u8()?,
                        interval: ext.u8()?,
                        y: ext.u8()?,
                        y_step: ext.u8()?,
                        kind: ext.u8()?,
                        subtype: ext.u8()?,
                        substate: ext.u8()?,
                        x: ext.u8()?,
                        motion: ext.u8()?,
                        phase: ext.u8()?,
                        attempted: ext.bool()?,
                    };
                    present.then_some(pending)
                },
                timer: ext.u8()?,
                spawn_count: ext.u8()?,
                cannon: ext.u8()?,
                class_counts: [ext.u8()?, ext.u8()?, ext.u8()?, ext.u8()?],
                init_pending: ext.bool()?,
                record_read: ext.bool()?,
                disabled: ext.bool()?,
            };
            let delay = crate::hud::ScoreDelayState {
                shown: [ext.u8()?, ext.u8()?, ext.u8()?],
                pending: [ext.u8()?, ext.u8()?, ext.u8()?],
                delay: ext.u8()?,
                index: {
                    if ext.u8()? != 1 {
                        return Err(crate::save::SaveError::BadPayloadLength);
                    }
                    ext.u32()?
                },
                countdown: ext.u8()?,
            };
            let ending_steps = ext.u8()?;
            // Optional tail: the stage-6 beam boxes.
            let mut beams = Beams::default();
            if ext.remaining() > 0 {
                for slot in &mut beams.slots {
                    *slot = Beam {
                        active: ext.bool()?,
                        x: ext.i16()?,
                        y: ext.i16()?,
                        lane: ext.u8()?,
                        ..Beam::default()
                    };
                }
            }
            if ext.remaining() >= 3 * crate::stage6_beams::SLOTS {
                for slot in &mut beams.slots {
                    slot.y_speed = ext.u16()?;
                    slot.y_frac = ext.u8()?;
                }
            }
            let (mut paused, mut pause_pending) = (false, 0);
            let (mut pause_code, mut pause_edges) = ([0; 2], 0);
            if ext.remaining() > 0 {
                paused = ext.bool()?;
                pause_pending = ext.u8()?;
            }
            if ext.remaining() > 0 {
                pause_code = [ext.u8()?, ext.u8()?];
                pause_edges = ext.u8()?;
            }
            ext.finish()?;
            if ending_steps > 23 {
                return Err(crate::save::SaveError::BadPayloadLength);
            }
            self.spawner.set_state(spawner);
            self.score_delay.set_state(delay);
            self.beams = beams;
            self.paused = paused;
            self.pause_code = pause_code;
            self.pause_edges = pause_edges;
            self.pause_pending = pause_pending.min(stage_flow::PAUSE_DELAY + 1);
            self.restore_ending_screen(ending_steps);
        }

        let mut players = crate::snapshot::Reader::new(&state.players);
        if !state.players.is_empty() {
            self.two = TwoPlayer::read_state(&mut players)?;
            players.finish()?;
            // The locked mode lives in both places; the turn machine is the
            // authority once its chunk is present.
            self.players = self.two.mode;
            // Only a 2P game keeps its live banks in the turn machine. In 1P
            // the CORE mirror just read is the authority: the dormant machine
            // still holds the three lives `$82CD` gave it, while attract keeps
            // `$20` at zero.
            if self.players == Players::Two {
                self.lives = self.two.slots[0].lives;
                self.score = self.two.slots[0].score;
            }
        }

        // Optional SIMUL chunk: the second ship of a simultaneous 2P game.
        // Absent in every cartridge-parity image, where P2's kit stays as
        // constructed and is never read.
        let mut simul = crate::snapshot::Reader::new(&state.simul);
        if !state.simul.is_empty() {
            self.p2 = read_ship(&mut simul)?;
            self.simul_respawn = [simul.u8()?, simul.u8()?];
            simul.finish()?;
        }

        let mut cells = crate::snapshot::Reader::new(&state.cells);
        let count = cells.u32()? as usize;
        if count > crate::snapshot::MAX_CELLS {
            return Err(crate::save::SaveError::BadPayloadLength);
        }
        let mut list = Vec::with_capacity(count);
        for _ in 0..count {
            list.push(stage3_enemies::Cell {
                world_col: cells.u16()?,
                row: cells.u8()?,
                tile: cells.u8()?,
            });
        }
        cells.finish()?;

        let mut collision = crate::snapshot::Reader::new(&state.collision);
        let count = collision.u32()? as usize;
        if count > crate::snapshot::MAX_COLLISION_PATCHES {
            return Err(crate::save::SaveError::BadPayloadLength);
        }
        let mut patches = Vec::with_capacity(count);
        for _ in 0..count {
            patches.push(stage3_enemies::CollisionPatch {
                addr: collision.u16()?,
                value: collision.u8()?,
            });
        }
        let mut broken = vec![0u32; crate::stage_collision::COLUMNS];
        let mut fresh = Vec::new();
        if collision.remaining() > 0 {
            for _ in 0..collision.u16()? {
                let column = usize::from(collision.u16()?);
                if column >= broken.len() {
                    return Err(crate::save::SaveError::BadPayloadLength);
                }
                broken[column] = collision.u32()?;
            }
            for _ in 0..collision.u8()? {
                fresh.push((collision.u16()?, collision.u8()?));
            }
        }
        collision.finish()?;

        self.earlier_sprites = earlier;
        self.cells = list;
        self.collision_cells = patches;
        self.broken = broken;
        self.fresh_breaks = fresh;
        self.audio = AudioQueue::default();
        self.last_sprites = self.build_sprites();
        Ok(())
    }

    /// Rebuild the `$CF90` ending nametable from the number of `$8B` steps.
    /// The screen is a pure function of that count and the stage-1 source, so
    /// no pixels are stored.
    fn restore_ending_screen(&mut self, steps: u8) {
        let active = (0x88..=0x8D).contains(&self.ending_step);
        self.ending_screen = if active {
            let mut screen = crate::ending_screen::EndingScreen::new();
            if let Some(source) = crate::stage_bg::background(self.rom(), 0) {
                for _ in 0..steps {
                    screen.step(source);
                }
            }
            Some(screen)
        } else {
            None
        };
    }

    fn write_core(&self, w: &mut crate::snapshot::Writer) {
        w.u32(self.rng.0);
        // Player.
        w.u8(self.player.x.int);
        w.u8(self.player.x.frac);
        w.u8(self.player.y.int);
        w.u8(self.player.y.frac);
        w.bool(self.player.dying);
        w.u8(self.player.sprite_frame);
        w.u8(self.player.pending_frame);
        w.u8(self.player.anim_timer);
        for value in self.player.cooldown1 {
            w.u8(value);
        }
        for value in self.player.cooldown2 {
            w.u8(value);
        }
        for value in self.player.trail.x {
            w.u8(value);
        }
        for value in self.player.trail.y {
            w.u8(value);
        }
        w.u8(self.player.trail.index);
        // Objects.
        for object in &self.objects {
            w.bool(object.active);
            w.u8(object.kind);
            w.u8(object.subtype);
            w.u8(object.x.int);
            w.u8(object.x.frac);
            w.u8(object.y.int);
            w.u8(object.y.frac);
            w.i16(object.x_vel);
            w.i16(object.y_vel);
            w.u16(object.nametable_addr);
            w.u8(object.ai_state);
            w.u8(object.ai_timer);
            w.u8(object.ai_mode);
            w.u8(object.timer2);
            w.u8(object.anim_div);
            w.u8(object.anim);
            w.u8(object.sprite);
            w.u8(object.fire_timer);
            w.u8(object.fire_motion);
            w.u8(object.muzzle);
            w.u8(object.size_class);
            w.u8(object.phase);
            w.bool(object.terrain);
            w.bool(object.boss);
        }
        // Attached cannons.
        for cannon in &self.cannons {
            w.bool(cannon.active);
            w.u8(cannon.parent);
            w.u8(cannon.index);
            w.u8(cannon.timer);
            w.u8(cannon.hits);
            w.u8(cannon.part_counter);
            for value in cannon.phase {
                w.u8(value);
            }
            for value in cannon.part_x {
                w.i32(value);
            }
            for value in cannon.part_y {
                w.i32(value);
            }
        }
        // Drones, shots, bullets, explosions, stars.
        for drone in &self.options {
            w.bool(drone.active);
            w.u8(drone.x);
            w.u8(drone.y);
            w.u8(drone.sprite);
            w.u8(drone.counter);
        }
        for shot in &self.shots {
            w.bool(shot.active);
            w.u8(shot.kind);
            w.u8(shot.x.int);
            w.u8(shot.x.frac);
            w.u8(shot.y.int);
            w.u8(shot.y.frac);
            w.u8(shot.age);
        }
        for bullet in &self.enemy_bullets {
            w.i32(bullet.x);
            w.i32(bullet.y);
            w.i32(bullet.vx);
            w.i32(bullet.vy);
            w.u8(bullet.kind);
            w.u8(bullet.sprite);
        }
        w.u32(self.enemy_bullet_count as u32);
        for explosion in &self.explosions {
            w.i32(explosion.x);
            w.i32(explosion.y);
            w.u8(explosion.life);
            w.u8(explosion.max_life);
        }
        w.u32(self.explosion_count as u32);
        for star in &self.stars {
            w.i32(star.x);
            w.i16(star.y);
            w.u8(star.layer);
        }
        // Power meter through its public getters and replay on restore.
        w.u8(self.power.capsules());
        w.u8(self.power.speed_level());
        w.bool(self.power.has_missile());
        w.u8(self.power.weapon_level());
        w.u8(self.power.options());
        w.u8(self.power.shield_hits());
        w.u8(self.power.selection());
        for value in self.score_bcd.digits {
            w.u8(value);
        }
        for value in self.high_score_bcd.digits {
            w.u8(value);
        }
        w.u8(self.scroll.fraction);
        w.u8(self.scroll.low);
        w.u8(self.scroll.high);
        w.i32(self.scroll_lag);
        w.bool(self.wall_resume);
        let (phase_tag, phase_payload) = match self.phase {
            Phase::Attract(value) => (0u8, value),
            Phase::DemoStart => (1, 0),
            Phase::Continue(value) => (2, value),
            Phase::StageInit => (3, 0),
            Phase::StageIntro(value) => (4, value),
            Phase::Blank(value) => (5, u16::from(value)),
            Phase::Init(value) => (6, u16::from(value)),
            Phase::Normal => (7, 0),
            Phase::Transition(value) => (8, value),
            Phase::Dying(value) => (9, value),
            Phase::Cleared(value) => (10, value),
        };
        w.u8(phase_tag);
        w.u16(phase_payload);
        w.u8(self.section);
        w.u8(self.music);
        w.u32(self.stage_clock);
        w.u32(self.kills);
        w.u16(self.banner);
        w.bool(self.prev_fire);
        w.bool(self.prev_start);
        w.u8(self.debris_folds);
        w.u16(self.title_timer);
        w.u16(self.title_attract_at);
        w.u16(self.attract_countdown);
        w.u8(self.volcano_kills);
        w.bool(self.stage_clear_ready);
        w.u8(self.vram_queue_len);
        w.bool(self.spawn_busy);
        w.bool(self.demo);
        w.u16(self.demo_index);
        w.u8(self.demo_ticks);
        w.u8(self.demo_button);
        w.bool(self.prev_select);
        w.u8(self.menu_selection);
        w.u8(match self.players {
            Players::One => 0,
            Players::Two => 1,
        });
        w.u8(self.menu_blink);
        w.u8(self.menu_select_pending);
        w.bool(self.scroll_stop);
        w.u8(self.continue_pending);
        w.u8(self.scroll_hold);
        w.u8(self.entry_blank);
        let buttons = u8::from(self.latched_input.fire)
            | (u8::from(self.latched_input.power) << 1)
            | (u8::from(self.latched_input.up) << 2)
            | (u8::from(self.latched_input.down) << 3)
            | (u8::from(self.latched_input.left) << 4)
            | (u8::from(self.latched_input.right) << 5)
            | (u8::from(self.latched_input.start) << 6)
            | (u8::from(self.latched_input.select) << 7);
        w.u8(buttons);
        w.bool(self.boss_spawned);
        w.bool(self.dev_keep_alive);
        w.bool(self.mods.extra_options);
        w.u8(self.boss_stage);
        w.u16(self.boss_timer);
        w.u8(self.hazard_index);
        w.u8(self.script_counter);
        w.u16(self.boss_clear);
        w.bool(self.boss_clear_armed);
        w.u8(match self.clear {
            Clear::None => 0,
            Clear::WaitEnd => 1,
            Clear::Bonus => 2,
            Clear::Handoff => 3,
        });
        let (endgame_tag, endgame_payload) = match self.endgame {
            Endgame::None => (0u8, 0),
            Endgame::Hold(value) => (1, value),
            Endgame::Bridge(value) => (2, u16::from(value)),
            Endgame::Erase => (3, 0),
            Endgame::Advance => (4, 0),
            Endgame::Ending => (5, 0),
        };
        w.u8(endgame_tag);
        w.u16(endgame_payload);
        w.u8(self.loop_count);
        w.u8(self.ending_step);
        w.u16(self.ending_timer);
        w.u16(self.ending_clock);
        w.u8(self.ending_text);
        w.bool(self.ending_paid);
        w.u8(self.ending_ship.x);
        w.u8(self.ending_ship.y);
        w.u8(self.ending_ship.sprite);
        w.u8(self.ending_ship.step);
        w.u8(self.wall_erase);
        w.bool(self.bonus_wave);
        w.bool(self.scroll_locked);
        w.u16(self.game_over_timer);
        w.bool(self.title_pending);
        w.u8(self.death_checkpoint);
        w.u8(self.death_burst.is_some() as u8);
        let burst = self
            .death_burst
            .unwrap_or(DeathBurst { x: 0, y: 0, age: 0 });
        w.i32(burst.x);
        w.i32(burst.y);
        w.u16(burst.age);
        w.u8(match self.state {
            State::Title => 0,
            State::Playing => 1,
            State::GameOver => 2,
        });
        w.u64(self.frame);
        w.u8(self.nmi_frame);
        w.bool(self.nmi_lag);
        w.u32(self.score);
        w.u8(self.lives);
        debug_assert_eq!(w.buf.len(), 2_525);
    }

    fn read_core(
        &mut self,
        r: &mut crate::snapshot::Reader<'_>,
    ) -> Result<(), crate::save::SaveError> {
        self.rng.0 = r.u32()?;
        self.player.x.int = r.u8()?;
        self.player.x.frac = r.u8()?;
        self.player.y.int = r.u8()?;
        self.player.y.frac = r.u8()?;
        self.player.dying = r.bool()?;
        self.player.sprite_frame = r.u8()?;
        self.player.pending_frame = r.u8()?;
        self.player.anim_timer = r.u8()?;
        for value in &mut self.player.cooldown1 {
            *value = r.u8()?;
        }
        for value in &mut self.player.cooldown2 {
            *value = r.u8()?;
        }
        for value in &mut self.player.trail.x {
            *value = r.u8()?;
        }
        for value in &mut self.player.trail.y {
            *value = r.u8()?;
        }
        self.player.trail.index = r.u8()?;
        for object in &mut self.objects {
            object.active = r.bool()?;
            object.kind = r.u8()?;
            object.subtype = r.u8()?;
            object.x.int = r.u8()?;
            object.x.frac = r.u8()?;
            object.y.int = r.u8()?;
            object.y.frac = r.u8()?;
            object.x_vel = r.i16()?;
            object.y_vel = r.i16()?;
            object.nametable_addr = r.u16()?;
            object.ai_state = r.u8()?;
            object.ai_timer = r.u8()?;
            object.ai_mode = r.u8()?;
            object.timer2 = r.u8()?;
            object.anim_div = r.u8()?;
            object.anim = r.u8()?;
            object.sprite = r.u8()?;
            object.fire_timer = r.u8()?;
            object.fire_motion = r.u8()?;
            object.muzzle = r.u8()?;
            object.size_class = r.u8()?;
            object.phase = r.u8()?;
            object.terrain = r.bool()?;
            object.boss = r.bool()?;
        }
        for cannon in &mut self.cannons {
            cannon.active = r.bool()?;
            cannon.parent = r.u8()?;
            cannon.index = r.u8()?;
            cannon.timer = r.u8()?;
            cannon.hits = r.u8()?;
            cannon.part_counter = r.u8()?;
            for value in &mut cannon.phase {
                *value = r.u8()?;
            }
            for value in &mut cannon.part_x {
                *value = r.i32()?;
            }
            for value in &mut cannon.part_y {
                *value = r.i32()?;
            }
        }
        for drone in &mut self.options {
            drone.active = r.bool()?;
            drone.x = r.u8()?;
            drone.y = r.u8()?;
            drone.sprite = r.u8()?;
            drone.counter = r.u8()?;
        }
        for shot in &mut self.shots {
            shot.active = r.bool()?;
            shot.kind = r.u8()?;
            shot.x.int = r.u8()?;
            shot.x.frac = r.u8()?;
            shot.y.int = r.u8()?;
            shot.y.frac = r.u8()?;
            shot.age = r.u8()?;
        }
        for bullet in &mut self.enemy_bullets {
            bullet.x = r.i32()?;
            bullet.y = r.i32()?;
            bullet.vx = r.i32()?;
            bullet.vy = r.i32()?;
            bullet.kind = r.u8()?;
            bullet.sprite = r.u8()?;
        }
        let count = r.u32()? as usize;
        if count > MAX_ENEMY_BULLETS {
            return Err(crate::save::SaveError::BadPayloadLength);
        }
        self.enemy_bullet_count = count;
        for explosion in &mut self.explosions {
            explosion.x = r.i32()?;
            explosion.y = r.i32()?;
            explosion.life = r.u8()?;
            explosion.max_life = r.u8()?;
        }
        let count = r.u32()? as usize;
        if count > MAX_EXPLOSIONS {
            return Err(crate::save::SaveError::BadPayloadLength);
        }
        self.explosion_count = count;
        for star in &mut self.stars {
            star.x = r.i32()?;
            star.y = r.i16()?;
            star.layer = r.u8()?;
        }
        let capsules = r.u8()?;
        let speed = r.u8()?;
        let missile = r.bool()?;
        let weapon = r.u8()?;
        let options = r.u8()?;
        let shield = r.u8()?;
        let selection = r.u8()?;
        let mut meter = PowerMeter::default();
        for _ in 0..capsules {
            meter.advance();
        }
        for _ in 0..speed {
            meter.select(1);
            meter.activate(true);
        }
        if missile {
            meter.select(2);
            meter.activate(true);
        }
        match weapon {
            1 => {
                meter.select(4);
                meter.activate(true);
            }
            2 => {
                meter.select(3);
                meter.activate(true);
            }
            _ => {}
        }
        for _ in 0..options {
            meter.select(5);
            meter.activate(true);
        }
        meter.dev_set_shield(shield);
        meter.select(selection);
        self.power = meter;
        for value in &mut self.score_bcd.digits {
            *value = r.u8()?;
        }
        for value in &mut self.high_score_bcd.digits {
            *value = r.u8()?;
        }
        self.scroll.fraction = r.u8()?;
        self.scroll.low = r.u8()?;
        self.scroll.high = r.u8()?;
        self.scroll_lag = r.i32()?;
        self.wall_resume = r.bool()?;
        let tag = r.u8()?;
        let payload = r.u16()?;
        self.phase = match tag {
            0 => Phase::Attract(payload),
            1 => Phase::DemoStart,
            2 => Phase::Continue(payload),
            3 => Phase::StageInit,
            4 => Phase::StageIntro(payload),
            5 => Phase::Blank(payload as u8),
            6 => Phase::Init(payload as u8),
            7 => Phase::Normal,
            8 => Phase::Transition(payload),
            9 => Phase::Dying(payload),
            10 => Phase::Cleared(payload),
            _ => return Err(crate::save::SaveError::BadPayloadLength),
        };
        self.section = r.u8()?;
        self.music = r.u8()?;
        self.stage_clock = r.u32()?;
        self.kills = r.u32()?;
        self.banner = r.u16()?;
        self.prev_fire = r.bool()?;
        self.prev_start = r.bool()?;
        self.debris_folds = r.u8()?;
        self.title_timer = r.u16()?;
        self.title_attract_at = r.u16()?;
        self.attract_countdown = r.u16()?;
        self.volcano_kills = r.u8()?;
        self.stage_clear_ready = r.bool()?;
        self.vram_queue_len = r.u8()?;
        self.spawn_busy = r.bool()?;
        self.demo = r.bool()?;
        self.demo_index = r.u16()?;
        self.demo_ticks = r.u8()?;
        self.demo_button = r.u8()?;
        self.prev_select = r.bool()?;
        self.menu_selection = r.u8()?;
        self.players = match r.u8()? {
            0 => Players::One,
            1 => Players::Two,
            _ => return Err(crate::save::SaveError::BadPayloadLength),
        };
        self.menu_blink = r.u8()?;
        self.menu_select_pending = r.u8()?;
        self.scroll_stop = r.bool()?;
        self.continue_pending = r.u8()?;
        self.scroll_hold = r.u8()?;
        self.entry_blank = r.u8()?;
        let buttons = r.u8()?;
        self.latched_input.fire = buttons & 0x01 != 0;
        self.latched_input.power = buttons & 0x02 != 0;
        self.latched_input.up = buttons & 0x04 != 0;
        self.latched_input.down = buttons & 0x08 != 0;
        self.latched_input.left = buttons & 0x10 != 0;
        self.latched_input.right = buttons & 0x20 != 0;
        self.latched_input.start = buttons & 0x40 != 0;
        self.latched_input.select = buttons & 0x80 != 0;
        self.boss_spawned = r.bool()?;
        self.dev_keep_alive = r.bool()?;
        self.mods.extra_options = r.bool()?;
        self.boss_stage = r.u8()?;
        self.boss_timer = r.u16()?;
        self.hazard_index = r.u8()?;
        self.script_counter = r.u8()?;
        self.boss_clear = r.u16()?;
        self.boss_clear_armed = r.bool()?;
        self.clear = match r.u8()? {
            0 => Clear::None,
            1 => Clear::WaitEnd,
            2 => Clear::Bonus,
            3 => Clear::Handoff,
            _ => return Err(crate::save::SaveError::BadPayloadLength),
        };
        let tag = r.u8()?;
        let payload = r.u16()?;
        self.endgame = match tag {
            0 => Endgame::None,
            1 => Endgame::Hold(payload),
            2 => Endgame::Bridge(payload as u8),
            3 => Endgame::Erase,
            4 => Endgame::Advance,
            5 => Endgame::Ending,
            _ => return Err(crate::save::SaveError::BadPayloadLength),
        };
        self.loop_count = r.u8()?;
        self.ending_step = r.u8()?;
        self.ending_timer = r.u16()?;
        self.ending_clock = r.u16()?;
        self.ending_text = r.u8()?;
        self.ending_paid = r.bool()?;
        self.ending_ship.x = r.u8()?;
        self.ending_ship.y = r.u8()?;
        self.ending_ship.sprite = r.u8()?;
        self.ending_ship.step = r.u8()?;
        self.wall_erase = r.u8()?;
        self.bonus_wave = r.bool()?;
        self.scroll_locked = r.bool()?;
        self.game_over_timer = r.u16()?;
        self.title_pending = r.bool()?;
        self.death_checkpoint = r.u8()?;
        let has_burst = r.u8()? != 0;
        let x = r.i32()?;
        let y = r.i32()?;
        let age = r.u16()?;
        self.death_burst = has_burst.then_some(DeathBurst { x, y, age });
        self.state = match r.u8()? {
            0 => State::Title,
            1 => State::Playing,
            2 => State::GameOver,
            _ => return Err(crate::save::SaveError::BadPayloadLength),
        };
        self.frame = r.u64()?;
        self.nmi_frame = r.u8()?;
        self.nmi_lag = r.bool()?;
        self.score = r.u32()?;
        self.lives = r.u8()?;
        // Images written before the PLAYERS chunk carry only this P1 mirror;
        // the turn machine restarts from the decoded mode. The PLAYERS chunk,
        // when present, overwrites it with both banks below.
        self.two.start(self.players);
        self.two.slots[0].lives = self.lives;
        self.two.slots[0].score = self.score;
        Ok(())
    }
}
