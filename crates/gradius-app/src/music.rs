//! Music selection from observable game state.
//!
//! The core does not expose its stage sub-state or its live object list through
//! a dedicated audio hook, so the runner derives the cue from public values:
//!
//! * [`State`] selects the title, playing and game-over tracks;
//! * the stage's `$8344` music id selects stages 2-7 and the two stage-1
//!   halves; a stage advance switches to the new stage's theme immediately;
//! * the stage's `$9A30` trigger selects the Big Core;
//! * a one-frame score jump of [`BOSS_SCORE`] marks the Big Core's destruction
//!   and starts the clear fanfare.
//!
//! The backend maps each [`MusicCue`] to its own audio engine; this module
//! stays free of audio devices and captured timelines.

use gradius_core::{Game, State};

/// Scroll position (`$3F:$3E`) where the original switches to sound `$96`.
pub const STAGE1_SECOND_HALF_SCROLL: u16 = 0x0400;
/// Scroll position where the core spawns the stage-1 Big Core.
pub const BOSS_SCROLL: u16 = 2_820;
/// Points added in the frame the Big Core dies (`$C776`).
pub const BOSS_SCORE: u32 = 1000;

/// `$8344` music id of the stage-1 theme.
const STAGE1_MUSIC: u8 = 0x96;

/// One semantic music track for a backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicCue {
    Title,
    Stage1,
    Stage1B,
    Stage2,
    Stage3,
    Stage4,
    Stage5,
    Stage6,
    Stage7,
    Boss,
    StageClear,
    GameOver,
}

/// The catalog's `$8344` music id selects the stage's theme.
fn stage_theme(music: u8) -> MusicCue {
    match music {
        0x59 => MusicCue::Stage2,
        0x5B => MusicCue::Stage3,
        0x5D => MusicCue::Stage4,
        0x5F => MusicCue::Stage5,
        0x61 => MusicCue::Stage6,
        0x63 => MusicCue::Stage7,
        _ => MusicCue::Stage1B,
    }
}

/// Stage 1 keeps its measured `$B04` cut; later stages use their `$9A30`.
fn boss_scroll(music: u8, boss_trigger: u8) -> u16 {
    if music == STAGE1_MUSIC {
        BOSS_SCROLL
    } else {
        u16::from(boss_trigger) << 8
    }
}

/// Tracks the current cue and reports only real changes.
#[derive(Debug)]
pub struct MusicDirector {
    cue: MusicCue,
    previous_state: State,
    previous_stage_music: u8,
    previous_score: u32,
    boss_seen: bool,
    stage_cleared: bool,
}

impl Default for MusicDirector {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicDirector {
    pub fn new() -> Self {
        Self {
            cue: MusicCue::Title,
            previous_state: State::Title,
            previous_stage_music: STAGE1_MUSIC,
            previous_score: 0,
            boss_seen: false,
            stage_cleared: false,
        }
    }

    pub fn cue(&self) -> MusicCue {
        self.cue
    }

    /// Observe one simulation frame; returns the new cue when it changed.
    pub fn update(&mut self, game: &Game) -> Option<MusicCue> {
        self.observe(
            game.state,
            game.scroll().position(),
            game.score,
            game.stage_music(),
            game.stage_boss_trigger(),
        )
    }

    /// Pure state transition, split out so the decision table is unit-testable.
    pub fn observe(
        &mut self,
        state: State,
        scroll: u16,
        score: u32,
        stage_music: u8,
        boss_trigger: u8,
    ) -> Option<MusicCue> {
        if state == State::Playing
            && (self.previous_state != State::Playing || stage_music != self.previous_stage_music)
        {
            self.boss_seen = false;
            self.stage_cleared = false;
            self.previous_score = score;
        }
        let score_jump = score.saturating_sub(self.previous_score);
        let boss_scroll = boss_scroll(stage_music, boss_trigger);

        let desired = match state {
            State::Title => MusicCue::Title,
            // The clear fanfare covers the 240-frame cleared phase. When the
            // core stops the run (state GameOver) the game-over theme takes
            // over, so a completed run still has its end cue.
            State::GameOver => MusicCue::GameOver,
            State::Playing => {
                if self.stage_cleared {
                    MusicCue::StageClear
                } else if self.boss_seen || scroll >= boss_scroll {
                    self.boss_seen = true;
                    if score_jump >= BOSS_SCORE {
                        self.stage_cleared = true;
                        MusicCue::StageClear
                    } else {
                        MusicCue::Boss
                    }
                } else if stage_music == STAGE1_MUSIC {
                    if scroll >= STAGE1_SECOND_HALF_SCROLL {
                        MusicCue::Stage1B
                    } else {
                        MusicCue::Stage1
                    }
                } else {
                    stage_theme(stage_music)
                }
            }
        };

        self.previous_state = state;
        self.previous_stage_music = stage_music;
        self.previous_score = score;
        if desired != self.cue {
            self.cue = desired;
            Some(desired)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_core::Buttons;
    use gradius_pack::{PackBuilder, TILES_PER_BANK, TILE_BANKS};

    const STAGE1_TRIGGER: u8 = 0x0C;

    fn observe(
        director: &mut MusicDirector,
        state: State,
        scroll: u16,
        score: u32,
    ) -> Option<MusicCue> {
        director.observe(state, scroll, score, STAGE1_MUSIC, STAGE1_TRIGGER)
    }

    #[test]
    fn title_then_play_selects_the_stage_theme() {
        let mut director = MusicDirector::new();
        assert_eq!(observe(&mut director, State::Title, 0, 0), None);
        assert_eq!(director.cue(), MusicCue::Title);
        assert_eq!(
            observe(&mut director, State::Playing, 0, 0),
            Some(MusicCue::Stage1)
        );
    }

    #[test]
    fn scroll_selects_the_second_half_and_the_boss() {
        let mut director = MusicDirector::new();
        observe(&mut director, State::Playing, 0, 0);
        assert_eq!(
            observe(
                &mut director,
                State::Playing,
                STAGE1_SECOND_HALF_SCROLL - 1,
                0
            ),
            None
        );
        assert_eq!(
            observe(&mut director, State::Playing, STAGE1_SECOND_HALF_SCROLL, 0),
            Some(MusicCue::Stage1B)
        );
        assert_eq!(
            observe(&mut director, State::Playing, BOSS_SCROLL - 1, 0),
            None
        );
        assert_eq!(
            observe(&mut director, State::Playing, BOSS_SCROLL, 0),
            Some(MusicCue::Boss)
        );
    }

    #[test]
    fn boss_score_starts_the_clear_fanfare_then_the_game_over_theme() {
        let mut director = MusicDirector::new();
        observe(&mut director, State::Playing, BOSS_SCROLL, 0);
        assert_eq!(
            observe(&mut director, State::Playing, BOSS_SCROLL, BOSS_SCORE),
            Some(MusicCue::StageClear)
        );
        assert_eq!(
            observe(&mut director, State::GameOver, BOSS_SCROLL, BOSS_SCORE),
            Some(MusicCue::GameOver)
        );
    }

    #[test]
    fn the_full_lifecycle_selects_each_cue_once() {
        let mut director = MusicDirector::new();
        let mut seen = Vec::new();
        let mut step = |state, scroll, score| {
            if let Some(cue) = observe(&mut director, state, scroll, score) {
                seen.push(cue);
            }
        };
        step(State::Title, 0, 0);
        step(State::Playing, 0, 0);
        step(State::Playing, STAGE1_SECOND_HALF_SCROLL, 0);
        step(State::Playing, BOSS_SCROLL, 0);
        step(State::Playing, BOSS_SCROLL, BOSS_SCORE);
        step(State::GameOver, BOSS_SCROLL, BOSS_SCORE);
        step(State::Playing, 0, 0);
        assert_eq!(
            seen,
            [
                MusicCue::Stage1,
                MusicCue::Stage1B,
                MusicCue::Boss,
                MusicCue::StageClear,
                MusicCue::GameOver,
                MusicCue::Stage1,
            ]
        );
    }

    #[test]
    fn repeated_frames_never_retrigger_the_same_cue() {
        let mut director = MusicDirector::new();
        assert_eq!(observe(&mut director, State::Title, 0, 0), None);
        for _ in 0..120 {
            assert_eq!(observe(&mut director, State::Title, 0, 0), None);
        }
        observe(&mut director, State::Playing, 0, 0);
        for _ in 0..120 {
            assert_eq!(observe(&mut director, State::Playing, 0, 0), None);
        }
        assert_eq!(
            observe(&mut director, State::Playing, BOSS_SCROLL - 1, 0),
            Some(MusicCue::Stage1B)
        );
        for _ in 0..120 {
            assert_eq!(
                observe(&mut director, State::Playing, BOSS_SCROLL - 1, 0),
                None
            );
        }
        assert_eq!(
            observe(&mut director, State::Playing, BOSS_SCROLL, 0),
            Some(MusicCue::Boss)
        );
        for _ in 0..120 {
            assert_eq!(observe(&mut director, State::Playing, BOSS_SCROLL, 0), None);
        }
        assert_eq!(
            observe(&mut director, State::Playing, BOSS_SCROLL, BOSS_SCORE),
            Some(MusicCue::StageClear)
        );
        for _ in 0..120 {
            assert_eq!(
                observe(&mut director, State::Playing, BOSS_SCROLL, BOSS_SCORE),
                None
            );
        }
        assert_eq!(
            observe(&mut director, State::GameOver, BOSS_SCROLL, BOSS_SCORE),
            Some(MusicCue::GameOver)
        );
        for _ in 0..120 {
            assert_eq!(
                observe(&mut director, State::GameOver, BOSS_SCROLL, BOSS_SCORE),
                None
            );
        }
    }

    #[test]
    fn death_game_over_selects_the_game_over_theme() {
        let mut director = MusicDirector::new();
        observe(&mut director, State::Playing, 0, 0);
        observe(&mut director, State::Playing, 1_500, 1_200);
        assert_eq!(
            observe(&mut director, State::GameOver, 1_500, 1_200),
            Some(MusicCue::GameOver)
        );
    }

    #[test]
    fn a_new_run_restarts_from_the_stage_theme() {
        let mut director = MusicDirector::new();
        observe(&mut director, State::Playing, BOSS_SCROLL, 0);
        observe(&mut director, State::Playing, BOSS_SCROLL, BOSS_SCORE);
        observe(&mut director, State::GameOver, BOSS_SCROLL, BOSS_SCORE);
        assert_eq!(
            observe(&mut director, State::Playing, 0, 0),
            Some(MusicCue::Stage1)
        );
    }

    const STAGE_CUES: [MusicCue; 7] = [
        MusicCue::Stage1B,
        MusicCue::Stage2,
        MusicCue::Stage3,
        MusicCue::Stage4,
        MusicCue::Stage5,
        MusicCue::Stage6,
        MusicCue::Stage7,
    ];

    #[test]
    fn every_catalog_stage_music_id_maps_to_its_theme() {
        let game = synthetic_game();
        let music = game.assets().rom().unwrap().tables.music;
        for (index, (id, cue)) in music.into_iter().zip(STAGE_CUES).enumerate() {
            assert_eq!(stage_theme(id), cue, "stage {} music", index + 1);
        }
    }

    #[test]
    fn a_stage_advance_starts_the_next_stage_theme_at_the_handoff() {
        let mut director = MusicDirector::new();
        observe(&mut director, State::Title, 0, 0);
        assert_eq!(
            observe(&mut director, State::Playing, BOSS_SCROLL, 0),
            Some(MusicCue::Boss)
        );
        assert_eq!(
            observe(&mut director, State::Playing, BOSS_SCROLL, BOSS_SCORE),
            Some(MusicCue::StageClear)
        );

        // `enter_stage` increments `$19` to 1 and clears `$3F`; the new
        // stage's `$59` theme must replace the stale clear fanfare now.
        assert_eq!(
            director.observe(State::Playing, 0, BOSS_SCORE, 0x59, STAGE1_TRIGGER),
            Some(MusicCue::Stage2)
        );
        assert_eq!(
            director.observe(State::Playing, 0x0C00, BOSS_SCORE, 0x59, STAGE1_TRIGGER),
            Some(MusicCue::Boss)
        );
        assert_eq!(
            director.observe(State::Playing, 0x0C00, BOSS_SCORE * 2, 0x59, STAGE1_TRIGGER),
            Some(MusicCue::StageClear)
        );
    }

    fn synthetic_game() -> Game {
        let banks: Vec<Vec<[u8; 64]>> = vec![vec![[0u8; 64]; TILES_PER_BANK]; TILE_BANKS];
        let pack = PackBuilder::new()
            .tiles(&banks)
            .palettes(&[[0x0f, 0x0f, 0x0f]])
            .meta("synthetic test pack\n")
            .finish();
        let mut assets = gradius_assets::Pack::parse(&pack)
            .expect("parse pack")
            .into_assets();
        // The core reads its collision maps from the extracted pack's ROM data.
        let path = std::env::var_os("GRADIUS_PACK").map_or_else(
            || {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/gradius.pak")
            },
            std::path::PathBuf::from,
        );
        assets.rom = gradius_assets::Pack::load(&path)
            .unwrap_or_else(|_| panic!("no extracted pack: run scripts/extract-data"))
            .into_assets()
            .rom;
        Game::new(assets)
    }

    /// Play a complete run's public state, scroll and score sequence through a
    /// real `Game`. The core exposes scroll read-only and needs more than 7,000
    /// frames to reach the Big Core, so the documented thresholds are supplied
    /// directly instead of depending on a long, balance-sensitive playthrough.
    #[test]
    fn a_real_core_run_selects_every_cue_transition() {
        let mut game = synthetic_game();
        let mut director = MusicDirector::new();
        let mut seen = vec![director.cue()];
        let mut record = |cue: Option<MusicCue>| {
            if let Some(cue) = cue {
                seen.push(cue);
            }
        };

        // The real game starts on the title screen.
        record(director.update(&game));

        game.state = State::Playing;
        record(director.update(&game));

        // Scroll-derived cues: the core's scroll cannot be set from outside.
        record(observe(
            &mut director,
            game.state,
            STAGE1_SECOND_HALF_SCROLL,
            game.score,
        ));
        record(observe(&mut director, game.state, BOSS_SCROLL, game.score));
        game.score = BOSS_SCORE;
        record(observe(&mut director, game.state, BOSS_SCROLL, game.score));

        game.state = State::GameOver;
        record(director.update(&game));
        game.state = State::Playing;
        game.score = 0;
        record(director.update(&game));

        assert_eq!(
            seen,
            [
                MusicCue::Title,
                MusicCue::Stage1,
                MusicCue::Stage1B,
                MusicCue::Boss,
                MusicCue::StageClear,
                MusicCue::GameOver,
                MusicCue::Stage1,
            ]
        );
    }

    /// End-to-end smoke: run the actual core with a simple script and check
    /// that the director follows it. It asserts nothing after game over, so
    /// the core's balance can change without breaking the cue contract.
    #[test]
    fn a_real_core_run_is_an_end_to_end_smoke() {
        let mut game = synthetic_game();
        let mut director = MusicDirector::new();
        let mut seen = Vec::new();
        for frame in 0..600u64 {
            game.step(Buttons {
                fire: frame > 20,
                start: frame == 120,
                ..Buttons::NONE
            });
            if let Some(cue) = director.update(&game) {
                seen.push(cue);
            }
            if game.state == State::GameOver {
                break;
            }
        }
        assert_ne!(
            game.state,
            State::Title,
            "the real core never left the title"
        );
        assert_eq!(seen.first(), Some(&MusicCue::Stage1));
        match game.state {
            State::GameOver => assert_eq!(director.cue(), MusicCue::GameOver),
            State::Playing => assert!(matches!(
                director.cue(),
                MusicCue::Stage1 | MusicCue::Stage1B | MusicCue::Boss | MusicCue::StageClear
            )),
            State::Title => unreachable!("asserted above"),
        }
    }
}
