//! The player ship, its shots and its options.
//!
//! Moved verbatim from `game.rs` (card R1g). The `Player` record, the option
//! trail, the shot pool and the `$9FF6` movement/fire pass keep their bodies,
//! names and order; the only edits are the `pub(super)` markers the module
//! boundary needs.

use super::*;

/// The 24-entry option trail (`$07A0`/`$07C0`, index `$0160`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Trail {
    pub(super) x: [u8; tables::OPTION_TRAIL],
    pub(super) y: [u8; tables::OPTION_TRAIL],
    pub(super) index: u8,
}

impl Trail {
    pub(super) fn new(x: u8, y: u8) -> Self {
        Self {
            x: [x; tables::OPTION_TRAIL],
            y: [y; tables::OPTION_TRAIL],
            index: 0,
        }
    }

    /// `$A080`: advance the ring and store the current ship position.
    pub(super) fn push(&mut self, x: u8, y: u8) {
        self.index = (self.index + 1) % tables::OPTION_TRAIL as u8;
        let slot = usize::from(self.index);
        self.x[slot] = x;
        self.y[slot] = y;
    }

    /// `$A2A3`: read the history `lag` entries back, wrapping at 24.
    pub(super) fn at(&self, lag: usize) -> (u8, u8) {
        let slot = (usize::from(self.index) + tables::OPTION_TRAIL - lag % tables::OPTION_TRAIL)
            % tables::OPTION_TRAIL;
        (self.x[slot], self.y[slot])
    }
}

/// One option drone (`$0141+i` animation counter and the shared trail).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OptionDrone {
    pub(super) active: bool,
    pub(super) x: u8,
    pub(super) y: u8,
    pub(super) sprite: u8,
    pub(super) counter: u8,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Player {
    pub(super) x: Fixed,
    pub(super) y: Fixed,
    /// `$0100`: 1 alive, 2 dying.
    pub(super) dying: bool,
    /// `$0120`, selected from the movement direction (1 level, 2 down, 3 up).
    pub(super) sprite_frame: u8,
    /// `$9B`, the direction latched until the 8-frame timer expires.
    pub(super) pending_frame: u8,
    /// `$0140`, reset at 8 (`$A0B4`).
    pub(super) anim_timer: u8,
    /// `$03A3` per emitter: shot-1 cooldown.
    pub(super) cooldown1: [u8; EXTENDED_EMITTERS],
    /// `$03A6` per emitter: shot-2 cooldown.
    pub(super) cooldown2: [u8; EXTENDED_EMITTERS],
    /// `$0160`/`$07A0`/`$07C0`.
    pub(super) trail: Trail,
    /// Hits left before the lethal path; [`crate::mods::Mods::hull_hits`] is
    /// copied here when the ship is placed.
    pub(super) hull: u8,
    /// Frames left in the post-hit window. [`crate::mods::Mods::hit_invulnerable_frames`]
    /// is stored one higher so the tick at the start of the next frame leaves
    /// exactly that many whole frames of protection.
    pub(super) hit_invulnerable: u8,
}

impl Player {
    pub(super) fn new(x: u8, y: u8) -> Self {
        Self {
            x: Fixed::px(x),
            y: Fixed::px(y),
            dying: false,
            sprite_frame: 1,
            pending_frame: 1,
            anim_timer: 0,
            cooldown1: [0; EXTENDED_EMITTERS],
            cooldown2: [0; EXTENDED_EMITTERS],
            trail: Trail::new(x, y),
            hull: 1,
            hit_invulnerable: 0,
        }
    }
}

/// A player shot (`$0123`/`$0163`, with positions `$0363`/`$0323`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Shot {
    pub(super) active: bool,
    pub(super) kind: u8,
    pub(super) x: Fixed,
    pub(super) y: Fixed,
    pub(super) age: u8,
}

impl Game {
    pub(super) fn place_player(&mut self, start: (u8, u8)) {
        let (x, y) = start;
        self.player = Player::new(x, y);
        // A fresh ship starts with the configured hull, clamped to the
        // supported 1..=99 range.
        self.player.hull = self.mods.hull_hits.clamp(1, 99);
    }

    /// `$81E3`: a 1P game ORs both pads (`$8203`), a 2P game feeds
    /// `$05,X`/`$07,X` from the player in `$18` only. On the title and the
    /// attract/continue windows either pad drives the menu.
    pub(super) fn player_input(&self, p1: Buttons, p2: Buttons) -> Buttons {
        let in_play = self.state == State::Playing
            && !matches!(
                self.phase,
                Phase::Attract(_)
                    | Phase::DemoStart
                    | Phase::Continue(_)
                    | Phase::StageInit
                    | Phase::Blank(_)
                    | Phase::Init(_)
            );
        if in_play && self.players == Players::Two {
            if self.two.current == 0 {
                p1
            } else {
                p2
            }
        } else {
            // `$8203`: `$9C = $9C | $9D`.
            Buttons {
                fire: p1.fire || p2.fire,
                power: p1.power || p2.power,
                up: p1.up || p2.up,
                down: p1.down || p2.down,
                left: p1.left || p2.left,
                right: p1.right || p2.right,
                start: p1.start || p2.start,
                select: p1.select || p2.select,
            }
        }
    }

    /// `$9FF6`: player movement, option trail and fire.
    pub(super) fn update_player(&mut self, input: Buttons, fire_pressed: bool) {
        if self.player.dying {
            // $9FF9: `$0100 >= 2` jumps straight to the shot loop, so no new
            // shots are fired while the ship explodes.
            return;
        }
        self.player.hit_invulnerable = self.player.hit_invulnerable.saturating_sub(1);

        let delta = self.power.speed_delta();
        let mut moved = false;
        if input.right {
            self.player.x.add(delta);
            if self.player.x.int > 0xF0 {
                self.player.x.int = 0xF0;
            }
            moved = true;
        }
        if input.left {
            self.player.x.sub(delta);
            if self.player.x.int < 0x10 {
                self.player.x.int = 0x10;
            }
            moved = true;
        }
        if input.down {
            self.player.y.add(delta);
            if self.player.y.int > 0xC0 {
                self.player.y.int = 0xC0;
            }
            moved = true;
            self.player.pending_frame = 2;
        }
        if input.up {
            self.player.y.sub(delta);
            if self.player.y.int < 0x10 {
                self.player.y.int = 0x10;
            }
            moved = true;
            self.player.pending_frame = 3;
        }
        if !input.down && !input.up {
            self.player.pending_frame = 1;
        }

        if moved {
            // $A080: only a movement frame writes the 24-entry history.
            self.player.trail.push(self.player.x.int, self.player.y.int);
        }

        // $A0A7: engine sprite frame timer, reset at 8.
        self.player.anim_timer = self.player.anim_timer.wrapping_add(1);
        if self.player.anim_timer >= 8 {
            self.player.anim_timer = 0;
            self.player.sprite_frame = self.player.pending_frame;
        }

        // $A0C7: option frame `4 + ((counter >> 3) & 1)`.
        let options = usize::from(self.power.options()).min(self.options.len());
        for option in self.options.iter_mut().take(options) {
            option.active = true;
            option.counter = option.counter.wrapping_add(1);
            option.sprite = 4 + ((option.counter >> 3) & 1);
        }
        for option in self.options.iter_mut().skip(options) {
            option.active = false;
        }
        // `$9B82` seeds `$0361/$0362` with the ship start, so options that
        // arrive without a movement frame still fire from the ship. The trail
        // only changes on movement, so this is idempotent on other frames.
        self.place_options();

        self.fire(fire_pressed, input.fire);
    }

    /// `$8979`/`$898E`: `$0100 == 1` plus held B (`$07` bit 6) applies the
    /// selected power. A successful handler clears it, so held B cannot repeat
    /// it.
    pub(super) fn apply_power(&mut self, held: bool) {
        if held && !self.player.dying {
            if let Some(power) = self.power.activate(self.mods.extra_options) {
                self.audio.push(Sound::PowerUp);
                let _ = power;
            }
        }
    }

    /// `$A2A3`: the option drones follow the 24-entry trail at lag 11.
    fn place_options(&mut self) {
        let count = usize::from(self.power.options()).min(self.options.len());
        for index in 0..count {
            let lag = tables::OPTION_LAG * (index + 1);
            let (x, y) = self.player.trail.at(lag);
            self.options[index].x = x;
            self.options[index].y = y;
            self.options[index].active = true;
        }
    }

    /// `$A0E3`: fire dispatcher for the ship and every option.
    fn fire(&mut self, fire_pressed: bool, held: bool) {
        let emitters = self.emitters_used();
        let weapon = self.power.weapon_level();
        for emitter in (0..emitters).rev() {
            // $0123: shot 1 active?
            let shot1 = self.shot_slot(emitter, 0);
            if !self.shots[shot1].active {
                if !fire_pressed {
                    if self.player.cooldown1[emitter] != 0 {
                        self.player.cooldown1[emitter] -= 1;
                        self.second_shot(emitter, weapon, fire_pressed, held);
                        continue;
                    }
                    if !held {
                        self.second_shot(emitter, weapon, fire_pressed, held);
                        continue;
                    }
                }
                self.spawn_shot(emitter, 0, weapon);
                self.player.cooldown1[emitter] = tables::FIRE_COOLDOWN;
                if weapon != 2 {
                    self.player.cooldown2[emitter] = tables::FIRE_COOLDOWN;
                    self.missile(emitter, held);
                    continue;
                }
            }
            self.second_shot(emitter, weapon, fire_pressed, held);
        }
    }

    fn second_shot(&mut self, emitter: usize, weapon: u8, fire_pressed: bool, held: bool) {
        let slot = self.shot_slot(emitter, 1);
        if self.shots[slot].active {
            self.missile(emitter, held);
            return;
        }
        if !fire_pressed {
            if self.player.cooldown2[emitter] != 0 {
                self.player.cooldown2[emitter] -= 1;
                self.missile(emitter, held);
                return;
            }
            if !held {
                self.missile(emitter, held);
                return;
            }
        }
        self.spawn_shot(emitter, 1, weapon);
        self.player.cooldown2[emitter] = tables::FIRE_COOLDOWN;
        if weapon != 2 {
            // $A14E: force the first cooldown and decrement the second.
            self.player.cooldown1[emitter] = tables::FIRE_COOLDOWN;
            self.player.cooldown2[emitter] = self.player.cooldown2[emitter].saturating_sub(1);
        }
        self.missile(emitter, held);
    }

    /// `$A265`: one missile per emitter while the button is held.
    fn missile(&mut self, emitter: usize, held: bool) {
        if !self.power.has_missile() || !held {
            return;
        }
        let base = 2 * self.shot_stride() + emitter;
        if base >= self.shots.len() || self.shots[base].active {
            return;
        }
        let (x, y) = self.emitter_position(emitter);
        self.shots[base] = Shot {
            active: true,
            kind: 3,
            x: Fixed::px(x),
            y: Fixed::px(y.wrapping_add(6)),
            age: 0,
        };
    }

    fn emitter_position(&self, emitter: usize) -> (u8, u8) {
        if emitter == 0 {
            (self.player.x.int, self.player.y.int)
        } else {
            let option = self.options[(emitter - 1).min(self.options.len() - 1)];
            (option.x, option.y)
        }
    }

    /// The active emitter count. The cartridge's `$45` cap gives three;
    /// [`Mods::extra_options`] keeps the port's five-emitter pool.
    fn emitters_used(&self) -> usize {
        let count = usize::from(self.power.options()) + 1;
        if self.mods.extra_options {
            count.min(EXTENDED_EMITTERS)
        } else {
            count.min(MAX_SHOTS / 3)
        }
    }

    /// Shot-table stride: three entries per emitter on the cartridge, five in
    /// the extended-options pool.
    pub(super) fn shot_stride(&self) -> usize {
        if self.mods.extra_options {
            EXTENDED_EMITTERS
        } else {
            EMITTERS
        }
    }

    /// True for the missile entry (`which == 2`) of any emitter.
    pub(super) fn is_missile_slot(&self, slot: usize) -> bool {
        slot >= 2 * self.shot_stride()
    }

    pub(super) fn shot_slot(&self, emitter: usize, which: usize) -> usize {
        // `$A22F`/`$A24A`: one block of shots per emitter, two shots then the
        // emitter's missile.
        which * self.shot_stride() + emitter
    }

    /// `$A22F`/`$A24A`: copy the emitter position and set the sprite kind.
    fn spawn_shot(&mut self, emitter: usize, which: usize, weapon: u8) {
        let slot = self.shot_slot(emitter, which);
        let (x, y) = self.emitter_position(emitter);
        // `$A23B`/`$A240`: the second double shot is kind 2 (rising).
        let kind = if weapon == 2 && which == 1 {
            2
        } else {
            weapon & 1
        };
        self.shots[slot] = Shot {
            active: true,
            kind,
            x: Fixed::px(x),
            y: Fixed::px(y),
            age: 0,
        };
        self.audio.push(Sound::Shot);
    }

    /// `$A169`/`$A1E4`: shot motion. 7 px/frame normal, 12 laser, missile
    /// accelerates up and right.
    pub(super) fn update_shots(&mut self) {
        for slot in 0..self.shots.len() {
            if !self.shots[slot].active {
                continue;
            }
            let mut shot = self.shots[slot];
            match shot.kind {
                0 => {
                    shot.x.add(i32::from(tables::SHOT_SPEED) << 8);
                    if shot.x.int >= tables::SHOT_DESPAWN {
                        shot.active = false;
                    }
                }
                1 => {
                    shot.x.add(i32::from(tables::LASER_SPEED) << 8);
                    if shot.x.int >= tables::LASER_DESPAWN {
                        shot.active = false;
                    }
                }
                2 => {
                    // `$A205`-`$A218`: the second double shot rises 4 px and
                    // drifts 4 px right, despawning above `y = $10` or at
                    // `x >= $F8`.
                    shot.y.sub(4 << 8);
                    if shot.y.int < 0x10 {
                        shot.active = false;
                    } else {
                        shot.x.add(4 << 8);
                        if shot.x.int >= tables::SHOT_DESPAWN {
                            shot.active = false;
                        }
                    }
                }
                _ => {
                    // `$A169`/`$A1E0`: missiles fall 2 px and drift 0.5 px
                    // right. A solid forward probe switches to the `$61`
                    // ground-ride frame (+2 px right, no fall); a solid probe
                    // 8 px up and right despawns it. `$19 = 4` skips the probe.
                    if self.section != 4 {
                        let x = i32::from(shot.x.int);
                        let y = i32::from(shot.y.int);
                        if self.solid_at(x as u8, y as u8) {
                            if self.solid_at((x + 8).min(255) as u8, (y - 8).max(0) as u8) {
                                shot.active = false;
                                self.shots[slot] = shot;
                                continue;
                            }
                            shot.age = 1;
                        } else {
                            shot.age = 0;
                        }
                    } else {
                        shot.age = 0;
                    }
                    if shot.age != 0 {
                        shot.x.add(2 << 8);
                    } else {
                        shot.y.add(2 << 8);
                        shot.x.add(0x80);
                    }
                    if shot.x.int >= 0xF8 || shot.y.int >= 0xC8 {
                        shot.active = false;
                    }
                }
            }
            self.shots[slot] = shot;
        }
    }

    /// True while the post-hit window swallows impacts. While it is up the
    /// power shield is not consumed either, so the shield and the hull never
    /// absorb the same hit (`$C1B7` still checks the shield first once the
    /// window is over).
    pub(super) fn player_invulnerable(&self) -> bool {
        self.player.hit_invulnerable > 0
    }

    /// The unshielded damage path: the hull takes the hit while more than one
    /// point is left, and only the last point starts `$C1D0`. This is called
    /// after the `$C1B7` shield check in every collision pass.
    pub(super) fn hull_hit(&mut self) {
        if self.player.hull > 1 {
            self.player.hull -= 1;
            if self.mods.hit_invulnerable_frames > 0 {
                // The tick at the start of the next frame consumes the extra
                // point, leaving exactly `hit_invulnerable_frames` frames.
                self.player.hit_invulnerable = self.mods.hit_invulnerable_frames.saturating_add(1);
            }
            self.audio.push(Sound::EnemyHit);
        } else {
            self.begin_death();
        }
    }

    /// `$C1D0`: death flag, 120-frame timer, `$1B = $A0`.
    pub(super) fn begin_death(&mut self) {
        if self.player.dying || self.mods.hull_hits == 0 {
            return;
        }
        if self.dev_keep_alive {
            // The analysis poke restores `$1B = $80` after the frame, but the
            // `$A0` the hit wrote this frame still cancels a `$81` clear the
            // stage handler armed; remember it so the arming can be undone.
            self.kept_hit = true;
            // `$C1D0`: a hit on a frame whose `$9A30` threshold armed `$81`
            // also clears `$60`. The poke leaves it clear, so `$A2BA` stops
            // spawning for the rest of the stage.
            if self.scroll.high >= self.stage_boss_trigger() {
                self.spawner.disable();
            }
            return;
        }
        if self.simultaneous() {
            self.begin_death_simultaneous();
            return;
        }
        self.player.dying = true;
        self.phase = Phase::Dying(120);
        // `$97A4`: the checkpoint is the current `$3F` window rounded down to
        // an even number and clamped to 8, captured now, not 120 frames later.
        self.death_checkpoint = (self.scroll.high & 0x0E).min(8);
        // `$C1D0`: `$1B = $A0`, `$4C = 120`, and the full `$24/$26/$28`
        // triple is saved for the player who is dying.
        self.two
            .death(self.rom(), self.section, self.scroll.high, 0);
        self.player.trail.index = 0;
        self.player.anim_timer = 0;
        // `$C0C1`: the death effect is its own 50-frame OAM sequence, not the
        // generic kind-2 burst.
        self.death_burst = Some(DeathBurst {
            x: i32::from(self.player.x.int) + 4,
            y: i32::from(self.player.y.int) + 8,
            age: 0,
        });
        self.audio.push(Sound::Explosion);
        self.audio.push(Sound::PlayerDeath);
    }

    /// `$C1D0` while both ships fly: only `$18`'s ship dies.
    ///
    /// The shared stage never rebuilds — that would drop the other ship in the
    /// middle of a wave — so the dead ship explodes in place and comes back at
    /// the current stage column after 120 frames if it still has lives. The
    /// `$C0` game over starts only when the last ship is out.
    fn begin_death_simultaneous(&mut self) {
        let side = self.two.current.min(1);
        self.player.dying = true;
        self.player.trail.index = 0;
        self.player.anim_timer = 0;
        self.simul_respawn[side] = crate::game::second_ship::SIMUL_DEATH_FRAMES;
        // `$C0C1`: the explosion is its own 50-frame sequence at the ship.
        self.death_burst = Some(DeathBurst {
            x: i32::from(self.player.x.int) + 4,
            y: i32::from(self.player.y.int) + 8,
            age: 0,
        });
        // `$97A4`: the checkpoint is captured now, for this player only.
        let rom = self.rom();
        let slot = &mut self.two.slots[side];
        slot.lives = slot.lives.wrapping_sub(1);
        if slot.lives == 0xFF {
            slot.alive = false;
        }
        slot.checkpoint = crate::twoplayer::Checkpoint::new(
            rom,
            self.section,
            (self.scroll.high & 0x0E).min(8),
            0,
        );
        self.lives = self.two.slots[0].lives;
        self.audio.push(Sound::Explosion);
        self.audio.push(Sound::PlayerDeath);
        if self.two.alive_mask() == 0 {
            self.game_over_timer = 399;
            self.two.state =
                crate::twoplayer::TurnState::GameOver(crate::twoplayer::GAME_OVER_FRAMES);
            self.audio.push(Sound::GameOver);
        }
    }
}
