//! Simultaneous 2P: the second ship and the per-frame passes that fly it.
//!
//! The cartridge only ever has one ship in `$0100-$04FF`, so player one keeps
//! the [`Game`] fields it always had and player two's whole kit lives in
//! [`Ship`]. [`Game::with_ship`] swaps that kit behind the P1 fields, so the
//! movement, weapons, drones, shots and collision passes stay the single
//! implementation they always were and simply run twice. The stage (scroll,
//! objects, enemy fire, power-ups) is shared and never swapped, and with
//! [`Mods::simultaneous_2p`] off nothing here is ever read.

use super::*;

/// The second ship's whole kit while [`Mods::simultaneous_2p`] is on.
///
/// The cartridge only ever has one ship in `$0100-$04FF`, so the port keeps
/// player one's record in the [`Game`] fields it always had (that is what keeps
/// the default save image byte-identical) and player two's whole kit here. The
/// per-frame passes swap the kit into the P1 fields with
/// [`Game::with_ship`], so movement, weapons, drones, shots and collisions stay
/// the single implementation they always were, run twice. The stage (scroll,
/// objects, enemy fire, power-ups) is shared and never swapped.
pub(super) struct Ship {
    pub(super) player: Player,
    pub(super) power: PowerMeter,
    pub(super) options: [OptionDrone; 4],
    pub(super) shots: [Shot; EXTENDED_MAX_SHOTS],
    /// `$C0C1` burst for this ship; the P1 one stays in `Game::death_burst`.
    pub(super) death_burst: Option<DeathBurst>,
}

impl Ship {
    pub(super) fn new() -> Self {
        Self {
            player: Player::new(0x50, 0x60),
            power: PowerMeter::default(),
            options: [OptionDrone::default(); 4],
            shots: [Shot {
                active: false,
                kind: 0,
                x: Fixed::default(),
                y: Fixed::default(),
                age: 0,
            }; EXTENDED_MAX_SHOTS],
            death_burst: None,
        }
    }
}

/// `$C1DA`: the same 120-frame death animation for a simultaneous ship.
pub(super) const SIMUL_DEATH_FRAMES: u8 = 120;

impl Game {
    /// The second ship's position while [`Game::simultaneous`] is on.
    pub fn second_ship(&self) -> Option<(u8, u8, u8, u8)> {
        if !self.simultaneous() {
            return None;
        }
        Some((
            self.p2.player.x.int,
            self.p2.player.x.frac,
            self.p2.player.y.int,
            self.p2.player.y.frac,
        ))
    }

    /// Run `f` with `side`'s kit behind the P1 fields every player pass reads.
    ///
    /// `side == 0` is the cartridge's own ship and never swaps. The stage and
    /// the turn machine's `$18` are put back afterwards, so only the swapped
    /// kit differs inside `f`.
    pub(super) fn with_ship<R>(&mut self, side: usize, f: impl FnOnce(&mut Self) -> R) -> R {
        if side == 0 {
            return f(self);
        }
        core::mem::swap(&mut self.player, &mut self.p2.player);
        core::mem::swap(&mut self.power, &mut self.p2.power);
        core::mem::swap(&mut self.options, &mut self.p2.options);
        core::mem::swap(&mut self.shots, &mut self.p2.shots);
        core::mem::swap(&mut self.death_burst, &mut self.p2.death_burst);
        let current = core::mem::replace(&mut self.two.current, side);
        let out = f(self);
        self.two.current = current;
        core::mem::swap(&mut self.player, &mut self.p2.player);
        core::mem::swap(&mut self.power, &mut self.p2.power);
        core::mem::swap(&mut self.options, &mut self.p2.options);
        core::mem::swap(&mut self.shots, &mut self.p2.shots);
        core::mem::swap(&mut self.death_burst, &mut self.p2.death_burst);
        out
    }

    /// Put P2's ship at the start of the current stage column with a fresh
    /// kit. Called from [`Game::new_run`] and on a continue.
    pub(super) fn place_second_ship(&mut self) {
        let (x, y) = stage::ship_start(self.rom(), self.section, self.scroll.high);
        self.p2 = Ship::new();
        self.p2.player = Player::new(x, y);
        self.p2.player.hull = self.mods.hull_hits.clamp(1, 99);
        self.simul_respawn[1] = 0;
    }

    /// Simultaneous 2P per-frame housekeeping: age both death bursts and bring
    /// a dead ship back after its explosion when it still has lives. The
    /// shared stage is never rebuilt, so the surviving ship keeps playing.
    pub(super) fn tick_simultaneous(&mut self) {
        if let Some(burst) = &mut self.death_burst {
            burst.age = burst.age.saturating_add(1);
        }
        if let Some(burst) = &mut self.p2.death_burst {
            burst.age = burst.age.saturating_add(1);
        }
        for side in 0..2 {
            if self.simul_respawn[side] == 0 {
                continue;
            }
            self.simul_respawn[side] -= 1;
            if self.simul_respawn[side] == 0 && self.two.slots[side].alive {
                self.respawn_ship(side);
            }
        }
    }

    /// Place a dead simultaneous ship back at the current stage column with a
    /// fresh hull and a cleared weapon kit.
    fn respawn_ship(&mut self, side: usize) {
        if side == 1 {
            self.place_second_ship();
            return;
        }
        let (x, y) = stage::ship_start(self.rom(), self.section, self.scroll.high);
        self.death_burst = None;
        self.simul_respawn[0] = 0;
        self.place_player((x, y));
        self.power.reset();
        self.options = [OptionDrone::default(); 4];
        self.shots = Ship::new().shots;
    }

    /// Simultaneous 2P: the `$9FF6` movement/fire pass for both ships.
    ///
    /// P1 keeps the latched `$07` the cartridge feeds it; P2's pad comes from
    /// its own `$05,X`/`$07,X` latch (`latched_p2`) with its own fire edge.
    pub(super) fn update_players(&mut self, input: Buttons, fire_pressed: bool) {
        if !self.simultaneous() {
            self.update_player(input, fire_pressed);
            return;
        }
        self.tick_simultaneous();
        self.update_player(input, fire_pressed);
        let control = self.latched_p2;
        let fire_pressed = control.fire && !self.prev_fire_p2;
        self.prev_fire_p2 = control.fire;
        self.with_ship(1, |game| game.update_player(control, fire_pressed));
    }

    /// `$A169` for one ship (cartridge) or both (simultaneous 2P).
    pub(super) fn update_players_shots(&mut self) {
        self.update_shots();
        if self.simultaneous() {
            self.with_ship(1, |game| game.update_shots());
        }
    }

    /// `$8979`/`$898E` power activation for one ship or both.
    pub(super) fn apply_players_power(&mut self, held: bool) {
        self.apply_power(held);
        if self.simultaneous() {
            let p2_held = self.latched_p2.power;
            self.with_ship(1, |game| game.apply_power(p2_held));
        }
    }

    /// `$BFDC`/`$C0FB`/`$C204` for one ship (cartridge) or both
    /// (simultaneous 2P). Each ship tests the shared objects and bullets, and
    /// its own shots test the shared objects.
    pub(super) fn collide_players(&mut self) {
        self.collide();
        if self.simultaneous() {
            self.with_ship(1, |game| game.collide());
        }
    }
}

/// Write one simultaneous-2P ship kit into the SIMUL chunk.
pub(super) fn write_ship(w: &mut crate::snapshot::Writer, ship: &Ship) {
    let player = &ship.player;
    w.u8(player.x.int);
    w.u8(player.x.frac);
    w.u8(player.y.int);
    w.u8(player.y.frac);
    w.bool(player.dying);
    w.u8(player.sprite_frame);
    w.u8(player.pending_frame);
    w.u8(player.anim_timer);
    for value in player.cooldown1 {
        w.u8(value);
    }
    for value in player.cooldown2 {
        w.u8(value);
    }
    for value in player.trail.x {
        w.u8(value);
    }
    for value in player.trail.y {
        w.u8(value);
    }
    w.u8(player.trail.index);
    w.u8(player.hull);
    w.u8(player.hit_invulnerable);
    // Power meter through its public getters, exactly like CORE.
    w.u8(ship.power.capsules());
    w.u8(ship.power.speed_level());
    w.bool(ship.power.has_missile());
    w.u8(ship.power.weapon_level());
    w.u8(ship.power.options());
    w.u8(ship.power.shield_hits());
    w.u8(ship.power.selection());
    for drone in &ship.options {
        w.bool(drone.active);
        w.u8(drone.x);
        w.u8(drone.y);
        w.u8(drone.sprite);
        w.u8(drone.counter);
    }
    for shot in &ship.shots {
        w.bool(shot.active);
        w.u8(shot.kind);
        w.u8(shot.x.int);
        w.u8(shot.x.frac);
        w.u8(shot.y.int);
        w.u8(shot.y.frac);
        w.u8(shot.age);
    }
    w.bool(ship.death_burst.is_some());
    let burst = ship
        .death_burst
        .unwrap_or(DeathBurst { x: 0, y: 0, age: 0 });
    w.i32(burst.x);
    w.i32(burst.y);
    w.u16(burst.age);
}

/// The inverse of [`write_ship`].
pub(super) fn read_ship(
    r: &mut crate::snapshot::Reader<'_>,
) -> Result<Ship, crate::save::SaveError> {
    let mut ship = Ship::new();
    let player = &mut ship.player;
    player.x.int = r.u8()?;
    player.x.frac = r.u8()?;
    player.y.int = r.u8()?;
    player.y.frac = r.u8()?;
    player.dying = r.bool()?;
    player.sprite_frame = r.u8()?;
    player.pending_frame = r.u8()?;
    player.anim_timer = r.u8()?;
    for value in &mut player.cooldown1 {
        *value = r.u8()?;
    }
    for value in &mut player.cooldown2 {
        *value = r.u8()?;
    }
    for value in &mut player.trail.x {
        *value = r.u8()?;
    }
    for value in &mut player.trail.y {
        *value = r.u8()?;
    }
    player.trail.index = r.u8()?;
    player.hull = r.u8()?;
    player.hit_invulnerable = r.u8()?;
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
    ship.power = meter;
    for drone in &mut ship.options {
        *drone = OptionDrone {
            active: r.bool()?,
            x: r.u8()?,
            y: r.u8()?,
            sprite: r.u8()?,
            counter: r.u8()?,
        };
    }
    for shot in &mut ship.shots {
        *shot = Shot {
            active: r.bool()?,
            kind: r.u8()?,
            x: Fixed {
                int: r.u8()?,
                frac: r.u8()?,
            },
            y: Fixed {
                int: r.u8()?,
                frac: r.u8()?,
            },
            age: r.u8()?,
        };
    }
    let has_burst = r.bool()?;
    let burst = DeathBurst {
        x: r.i32()?,
        y: r.i32()?,
        age: r.u16()?,
    };
    ship.death_burst = has_burst.then_some(burst);
    Ok(ship)
}
