//! Identities of the music tracks and sound effects.
//!
//! The register timelines themselves are not part of the repository: `xtask
//! extract` captures them from the cartridge into the pack's audio chunk, and
//! [`crate::AudioBank`] loads them at runtime. See `docs/AUDIO.md` for the
//! request ids, loop periods and verification counts.

/// Music tracks available to the frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Music {
    Title,
    Stage1,
    Stage1B,
    Boss,
    StageClear,
    GameOver,
    Stage2,
    Stage3,
    Stage4,
    Stage5,
    Stage6,
    Stage7,
}

impl Music {
    pub const ALL: [Music; 12] = [
        Music::Title,
        Music::Stage1,
        Music::Stage1B,
        Music::Boss,
        Music::StageClear,
        Music::GameOver,
        Music::Stage2,
        Music::Stage3,
        Music::Stage4,
        Music::Stage5,
        Music::Stage6,
        Music::Stage7,
    ];

    /// Timeline name stored in the audio chunk.
    pub const fn name(self) -> &'static str {
        match self {
            Music::Title => "intro",
            Music::Stage1 => "stage1",
            Music::Stage1B => "stage1_b",
            Music::Boss => "boss",
            Music::StageClear => "stage_clear",
            Music::GameOver => "game_over_music",
            Music::Stage2 => "stage2",
            Music::Stage3 => "stage3",
            Music::Stage4 => "stage4",
            Music::Stage5 => "stage5",
            Music::Stage6 => "stage6",
            Music::Stage7 => "stage7",
        }
    }

    /// Whether the original track loops after its lead-in.
    pub const fn loops(self) -> bool {
        !matches!(self, Music::Title | Music::StageClear | Music::GameOver)
    }
}

/// Sound effects available to the frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    Shot,
    EnemyShot,
    EnemyHit,
    Explosion,
    PowerUp,
    Capsule,
    PlayerDeath,
    GameOver,
    /// ROM sound id 3: a destructible cell breaks.
    BlockBreak,
    /// ROM sound id 4: the same break in stage 6 (`$19 == 5`).
    BlockBreakStage6,
    /// ROM sound id `$3B`: the Start pause jingle.
    Pause,
    /// ROM sound id `$0B`: the blue capsule clears the screen.
    ScreenClear,
}

impl Sfx {
    pub const ALL: [Sfx; 12] = [
        Sfx::Shot,
        Sfx::EnemyShot,
        Sfx::EnemyHit,
        Sfx::Explosion,
        Sfx::PowerUp,
        Sfx::Capsule,
        Sfx::PlayerDeath,
        Sfx::GameOver,
        Sfx::BlockBreak,
        Sfx::BlockBreakStage6,
        Sfx::Pause,
        Sfx::ScreenClear,
    ];

    /// Timeline name stored in the audio chunk.
    pub const fn name(self) -> &'static str {
        match self {
            Sfx::Shot => "shot",
            Sfx::EnemyShot => "enemy_shot",
            Sfx::EnemyHit => "enemy_hit",
            Sfx::Explosion => "explosion",
            Sfx::PowerUp => "power_up",
            Sfx::Capsule => "capsule",
            Sfx::PlayerDeath => "player_death",
            Sfx::GameOver => "game_over_sfx",
            Sfx::BlockBreak => "block_break",
            Sfx::BlockBreakStage6 => "block_break_stage6",
            Sfx::Pause => "pause",
            Sfx::ScreenClear => "screen_clear",
        }
    }
}
