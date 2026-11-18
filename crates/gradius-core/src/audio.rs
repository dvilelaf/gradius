//! Sound events emitted by the simulation.
//!
//! The core never opens an audio device. It queues semantic events; a frontend
//! maps them to whatever mixer it has. Keeping the vocabulary this small lets
//! the ported Konami sound engine (next milestone) replace the placeholder
//! playback without touching game logic.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// The player fired a volley.
    Shot,
    /// A shot connected with an enemy.
    EnemyHit,
    /// An enemy was destroyed.
    Explosion,
    /// A power-up was redeemed from the meter.
    PowerUp,
    /// A power capsule was collected.
    Capsule,
    /// The blue capsule (`$0B`) destroyed every enemy on screen.
    ScreenClear,
    /// An enemy fired at the player.
    EnemyShot,
    /// The player's ship was destroyed.
    PlayerDeath,
    /// The last life was lost.
    GameOver,
    /// A destructible background cell was shot away.
    BlockBreak,
    /// The same, in stage 6.
    BlockBreakStage6,
    /// Start paused the stage: the music stops and the `$3B` jingle plays.
    Pause,
    /// Start resumed the stage: the music continues where it stopped.
    Resume,
}

/// A bounded queue; if a frontend stalls, the oldest events win.
#[derive(Clone, Debug, Default)]
pub struct AudioQueue {
    events: Vec<Sound>,
}

impl AudioQueue {
    pub fn push(&mut self, sound: Sound) {
        if self.events.len() < 32 {
            self.events.push(sound);
        }
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Sound> + '_ {
        self.events.drain(..)
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}
