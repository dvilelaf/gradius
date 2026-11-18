//! Port-side conveniences that intentionally deviate from the cartridge.
//!
//! Every mod is off by default, so a freshly built [`crate::Game`] behaves like
//! the original. Enable one only when you want the port's arcade-style
//! liberties; the differential harness keeps the defaults so its lockstep
//! comparison stays exact.

/// Opt-in deviations from ROM behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mods {
    /// Allow four options instead of the cartridge's two.
    ///
    /// `$89DA` compares `$45` with `#$02`; the arcade loadout has four. With
    /// this off, [`crate::powerup::PowerMeter::activate`] stops at 2.
    pub extra_options: bool,
    /// Hits the ship's hull absorbs before the lethal `$C1D0` path, clamped to
    /// 1..=99 when a ship is placed. The default 1 is the cartridge's
    /// one-touch death. Zero is invincible: nothing kills the ship.
    pub hull_hits: u8,
    /// Whole frames after a hull hit on which further impacts are ignored.
    /// Zero disables the window.
    pub hit_invulnerable_frames: u8,
    /// Stage 5's `$9658` alternate chain: with two or more live `$0600`
    /// cannons the cartridge runs the player, object, fire and cannon passes
    /// only on every other frame, which halves the scroll too. `false` runs
    /// every frame like the other stages. On by default, the cartridge.
    pub stage5_slowdown: bool,
    /// Stage (`1`..=`7`) a new game begins at, so a player can skip ahead
    /// without the hidden `--stage` path. The default 1 is the cartridge's
    /// start; the attract demo and [`crate::Game::new_at_stage`] ignore it.
    pub start_stage: u8,
    /// Both ships play at once instead of taking turns.
    ///
    /// Only meaningful in a 2P game: each ship keeps its own pad, hull,
    /// lives and score, while the stage (scroll, objects, enemy fire) stays
    /// shared. Off by default, which is the cartridge's alternating 2P.
    pub simultaneous_2p: bool,
    /// Sprite palette index (0..=3) for the second ship's hull, so the two
    /// ships are told apart on screen. The cartridge never draws two ships.
    pub p2_sprite_palette: u8,
}

impl Default for Mods {
    fn default() -> Self {
        Self {
            extra_options: false,
            hull_hits: 1,
            hit_invulnerable_frames: 0,
            stage5_slowdown: true,
            start_stage: 1,
            simultaneous_2p: false,
            p2_sprite_palette: 1,
        }
    }
}
