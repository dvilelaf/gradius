//! Fixed-step runner shared by every backend.

use gradius_core::audio::Sound;
use gradius_core::{Buttons, Game, Mods, State};

/// Ten seconds of title before the attract demo may start.
const TITLE_HOLD_TICKS: u32 = 600;

use crate::music::MusicDirector;
use crate::MusicCue;

pub use gradius_core::TICKS_PER_SECOND;

/// Seconds per simulation tick (60.0988 Hz).
pub const STEP: f64 = 1.0 / TICKS_PER_SECOND;
/// Never simulate more than a quarter second after a stall.
pub const MAX_CATCH_UP: f64 = 0.25;

/// Both players' button state for one tick.
///
/// Both pads reach [`Game::step_players`]: a 1P game merges them (`$8203`), a
/// 2P game uses the pad of the player in `$18`, exactly like the oracle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputFrame {
    pub p1: Buttons,
    pub p2: Buttons,
}

impl InputFrame {
    pub const NONE: Self = Self {
        p1: Buttons::NONE,
        p2: Buttons::NONE,
    };
}

impl From<Buttons> for InputFrame {
    fn from(p1: Buttons) -> Self {
        Self {
            p1,
            p2: Buttons::NONE,
        }
    }
}

/// Supplies both players' button state for the next tick.
pub trait InputProvider {
    fn buttons(&mut self, frame: u64) -> InputFrame;
}

impl InputProvider for InputFrame {
    fn buttons(&mut self, _frame: u64) -> InputFrame {
        *self
    }
}

impl InputProvider for Buttons {
    fn buttons(&mut self, _frame: u64) -> InputFrame {
        (*self).into()
    }
}

impl<F> InputProvider for F
where
    F: FnMut(u64) -> InputFrame,
{
    fn buttons(&mut self, frame: u64) -> InputFrame {
        self(frame)
    }
}

/// Receives one complete frame as 256x240 RGBA8.
pub trait FrameSink {
    fn frame(&mut self, rgba: &[u8]) -> Result<(), String>;
}

/// Receives semantic sound events and music-cue changes.
pub trait SoundSink {
    fn sound(&mut self, sound: Sound);
    fn music(&mut self, _cue: MusicCue) {}
    /// Called once after every simulation tick with the running tick count.
    fn tick(&mut self, _frame: u64) {}
}

/// The result of a headless run, matching the old `--frames` summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub frames: u64,
    pub hash: u64,
    pub score: u32,
    pub lives: u8,
    pub audio_events: u64,
}

/// Owns the game, the music director and the pacing accumulator.
pub struct Runner {
    game: Game,
    music: MusicDirector,
    accumulator: f64,
    paused: bool,
    /// Player 1's pad on the last tick, for the title's edge detection.
    prev_p1: Buttons,
    /// The title cursor a Up/Down press asked for, until a Select edge lands.
    menu_target: Option<u8>,
    /// Ticks spent on the current title visit.
    title_ticks: u32,
}

impl Runner {
    pub fn new(game: Game) -> Self {
        Self {
            game,
            music: MusicDirector::new(),
            accumulator: 0.0,
            paused: false,
            prev_p1: Buttons::NONE,
            menu_target: None,
            title_ticks: 0,
        }
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Mutable access for frontend state changes such as loading a savestate.
    pub fn game_mut(&mut self) -> &mut Game {
        &mut self.game
    }

    /// Enable the port-only conveniences in [`Mods`].
    ///
    /// SDL maps `--hits`/`--2p-sim` here; the PWA sets the same values from
    /// its settings instead of command-line flags. The default is the
    /// cartridge's alternating play and a single hit.
    pub fn set_mods(&mut self, mods: Mods) {
        self.game.set_mods(mods);
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Pausing discards a partial accumulator, like the old P key did.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.accumulator = 0.0;
    }

    pub fn toggle_pause(&mut self) -> bool {
        self.set_paused(!self.paused);
        self.paused
    }

    /// Feed `delta` seconds of wall clock. Returns the ticks simulated.
    pub fn advance<S>(&mut self, delta: f64, input: InputFrame, sink: &mut S) -> usize
    where
        S: SoundSink + ?Sized,
    {
        if self.paused {
            return 0;
        }
        self.accumulator = (self.accumulator + delta.max(0.0)).min(MAX_CATCH_UP);
        let mut ticks = 0;
        while self.accumulator >= STEP {
            self.accumulator -= STEP;
            self.step(input, sink);
            ticks += 1;
        }
        ticks
    }

    /// One simulation tick, ignoring clock and pause. Returns the number of
    /// sound events drained into the sink.
    pub fn step<S>(&mut self, input: InputFrame, sink: &mut S) -> usize
    where
        S: SoundSink + ?Sized,
    {
        let p1 = self.menu_input(input.p1);
        self.hold_title(p1);
        self.game.step_players(p1, input.p2);
        self.prev_p1 = p1;
        let mut events = 0;
        for sound in self.game.audio.drain() {
            events += 1;
            sink.sound(sound);
        }
        if let Some(cue) = self.music.update(&self.game) {
            sink.music(cue);
        }
        sink.tick(self.game.frame);
        events
    }

    /// A player looking at or using the title menu should not be thrown into
    /// the attract demo: the cartridge rolls into it after ~2 s of no input.
    /// Hold the title for the first [`TITLE_HOLD_TICKS`] of each visit and
    /// while player 1 touches anything; after that the demo runs as on the NES.
    fn hold_title(&mut self, p1: Buttons) {
        if self.game.state != State::Title {
            self.title_ticks = 0;
            return;
        }
        self.title_ticks = self.title_ticks.saturating_add(1);
        if self.title_ticks < TITLE_HOLD_TICKS || p1 != Buttons::NONE {
            self.game.hold_title();
        }
    }

    /// The title moves its cursor only on a Select edge (`$8212`), so a fresh
    /// Up (to `1 PLAYER`) or Down (to `2 PLAYERS`) becomes one Select edge.
    /// A Select already held on the previous tick can't make an edge, so the
    /// press waits until Select is up again.
    fn menu_input(&mut self, mut p1: Buttons) -> Buttons {
        if self.game.state != State::Title {
            self.menu_target = None;
            return p1;
        }
        let selection = self.game.menu_selection();
        if p1.up && !self.prev_p1.up && selection != 0 {
            self.menu_target = Some(0);
        } else if p1.down && !self.prev_p1.down && selection != 1 {
            self.menu_target = Some(1);
        }
        if let Some(target) = self.menu_target {
            if selection == target {
                self.menu_target = None;
            } else if !self.prev_p1.select {
                p1.select = true;
                self.menu_target = None;
            }
        }
        p1
    }

    /// Hand the current framebuffer to a backend's surface.
    pub fn present<S>(&self, sink: &mut S) -> Result<(), String>
    where
        S: FrameSink + ?Sized,
    {
        sink.frame(self.game.framebuffer().as_rgba())
    }

    /// Headless/CI loop: exactly `frames` ticks.
    pub fn run<I, S>(&mut self, frames: u64, input: &mut I, sink: &mut S) -> Summary
    where
        I: InputProvider + ?Sized,
        S: SoundSink + ?Sized,
    {
        let mut audio_events = 0;
        for _ in 0..frames {
            let buttons = input.buttons(self.game.frame);
            audio_events += self.step(buttons, sink) as u64;
        }
        Summary {
            frames: self.game.frame,
            hash: self.game.framebuffer().frame_hash(),
            score: self.game.score,
            lives: self.game.lives,
            audio_events,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_assets::Pack;

    #[derive(Default)]
    struct Recorder {
        sounds: Vec<Sound>,
        cues: Vec<MusicCue>,
        ticks: usize,
        frame_bytes: usize,
    }

    impl SoundSink for Recorder {
        fn sound(&mut self, sound: Sound) {
            self.sounds.push(sound);
        }

        fn music(&mut self, cue: MusicCue) {
            self.cues.push(cue);
        }

        fn tick(&mut self, _frame: u64) {
            self.ticks += 1;
        }
    }

    impl FrameSink for Recorder {
        fn frame(&mut self, rgba: &[u8]) -> Result<(), String> {
            self.frame_bytes = rgba.len();
            Ok(())
        }
    }

    /// The extracted pack, the same one `tests/support` loads.
    fn game() -> Game {
        let path = std::env::var_os("GRADIUS_PACK").map_or_else(
            || {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/gradius.pak")
            },
            std::path::PathBuf::from,
        );
        let pack = Pack::load(&path).unwrap_or_else(|_| {
            panic!(
                "no extracted pack at {}: run GRADIUS_ROM=<rom> scripts/extract-data",
                path.display()
            )
        });
        Game::new(pack.into_assets())
    }

    fn script(frame: u64) -> InputFrame {
        InputFrame::from(Buttons {
            start: frame < 5,
            fire: frame >= 20,
            ..Buttons::NONE
        })
    }

    #[test]
    fn fixed_step_accumulates_a_fake_clock_deterministically() {
        let mut a = Runner::new(game());
        let mut sink = Recorder::default();

        assert_eq!(a.advance(STEP * 0.5, InputFrame::NONE, &mut sink), 0);
        assert_eq!(a.game().frame, 0);
        assert_eq!(a.advance(STEP * 0.5, InputFrame::NONE, &mut sink), 1);
        assert_eq!(a.game().frame, 1);
        assert_eq!(a.advance(0.0, InputFrame::NONE, &mut sink), 0);

        // Two runners fed the same fake clock stay bit-identical.
        let mut b = Runner::new(game());
        let mut c = Runner::new(game());
        for frame in 0..600 {
            b.advance(STEP, script(frame), &mut sink);
            c.advance(STEP, script(frame), &mut sink);
        }
        assert_eq!(b.game().frame, 600);
        assert_eq!(b.game().state_hash(), c.game().state_hash());
        assert_eq!(b.game().score, c.game().score);
    }

    #[test]
    fn catch_up_is_capped_after_a_stall() {
        let mut runner = Runner::new(game());
        let mut sink = Recorder::default();
        let ticks = runner.advance(10.0, InputFrame::NONE, &mut sink);
        assert_eq!(ticks, 15);
        assert!((ticks as f64) <= MAX_CATCH_UP / STEP + 1.0);
        assert_eq!(runner.game().frame, ticks as u64);
    }

    #[test]
    fn frame_sink_receives_one_rgba_frame() {
        let runner = Runner::new(game());
        let mut sink = Recorder::default();
        runner.present(&mut sink).expect("present");
        assert_eq!(sink.frame_bytes, crate::WIDTH * crate::HEIGHT * 4);
    }

    #[test]
    fn audio_events_reach_the_sink() {
        let mut game = game();
        game.audio.push(Sound::Shot);
        game.audio.push(Sound::GameOver);
        let mut runner = Runner::new(game);
        let mut sink = Recorder::default();
        let events = runner.step(InputFrame::NONE, &mut sink);
        assert_eq!(events, 2);
        assert_eq!(sink.sounds, [Sound::Shot, Sound::GameOver]);
    }

    #[test]
    fn music_cue_changes_reach_the_sink() {
        let mut runner = Runner::new(game());
        let mut sink = Recorder::default();
        runner.run(10, &mut script, &mut sink);
        assert_eq!(sink.cues.first(), Some(&MusicCue::Stage1));
        assert_eq!(sink.ticks, 10);
    }

    #[test]
    fn pause_stops_stepping_until_toggled() {
        let mut runner = Runner::new(game());
        let mut sink = Recorder::default();
        runner.set_paused(true);
        assert_eq!(runner.advance(1.0, InputFrame::NONE, &mut sink), 0);
        assert_eq!(runner.game().frame, 0);
        assert_eq!(sink.ticks, 0);

        assert!(!runner.toggle_pause());
        assert_eq!(runner.advance(STEP, InputFrame::NONE, &mut sink), 1);
        assert_eq!(runner.game().frame, 1);
    }

    #[test]
    fn runner_hands_mods_to_the_game() {
        let mut runner = Runner::new(game());
        assert_eq!(runner.game().mods(), Mods::default());
        let mods = Mods {
            extra_options: true,
            ..Mods::default()
        };
        runner.set_mods(mods);
        assert_eq!(runner.game().mods(), mods);
    }

    #[test]
    fn headless_summary_matches_stepping_the_core_directly() {
        let frames = 240;
        let mut oracle = game();
        let mut audio_events = 0u64;
        for frame in 0..frames {
            oracle.step(script(frame).p1);
            audio_events += oracle.audio.drain().count() as u64;
        }
        let expected = Summary {
            frames: oracle.frame,
            hash: oracle.framebuffer().frame_hash(),
            score: oracle.score,
            lives: oracle.lives,
            audio_events,
        };

        let mut sink = Recorder::default();
        let mut runner = Runner::new(game());
        let summary = runner.run(frames, &mut script, &mut sink);
        assert_eq!(summary, expected);
        assert_eq!(sink.sounds.len() as u64, audio_events);
        assert_eq!(runner.game().frame, frames);
    }

    fn every_button() -> Buttons {
        Buttons {
            fire: true,
            power: true,
            up: true,
            down: true,
            left: true,
            right: true,
            start: true,
            select: true,
        }
    }

    #[test]
    fn input_frame_from_buttons_only_drives_p1() {
        let p1 = Buttons {
            fire: true,
            ..Buttons::NONE
        };
        let frame = InputFrame::from(p1);
        assert_eq!(frame.p1, p1);
        assert_eq!(frame.p2, Buttons::NONE);
    }

    #[test]
    fn the_p2_pad_reaches_the_core_like_the_oracle() {
        // `$8203`: a 1P game merges both pads, so the runner must hand the
        // second one to `step_players` instead of dropping it.
        let mut sink = Recorder::default();
        let mut with_p2 = Runner::new(game());
        let mut p1_only = Runner::new(game());
        for frame in 0..300 {
            let input = script(frame);
            with_p2.advance(
                STEP,
                InputFrame {
                    p1: input.p1,
                    p2: every_button(),
                },
                &mut sink,
            );
            p1_only.advance(STEP, input, &mut sink);
        }
        assert_eq!(with_p2.game().frame, 300);
        assert_ne!(with_p2.game().state_hash(), p1_only.game().state_hash());
    }

    #[test]
    fn p2_input_reaches_the_headless_summary() {
        let frames = 240;
        let mut sink = Recorder::default();

        let mut with_p2 = |frame: u64| InputFrame {
            p2: every_button(),
            ..script(frame)
        };
        let mut a = Runner::new(game());
        let a = a.run(frames, &mut with_p2, &mut sink);

        let mut b = Runner::new(game());
        let b = b.run(frames, &mut script, &mut sink);
        assert_ne!(a, b);
    }

    #[test]
    fn an_idle_p2_leaves_a_one_player_run_unchanged() {
        // The smoke hashes come from 1P scripts whose second pad is idle.
        let mut sink = Recorder::default();
        let mut runner = Runner::new(game());
        let summary = runner.run(240, &mut script, &mut sink);

        let mut oracle = game();
        for frame in 0..240 {
            oracle.step_players(script(frame).p1, Buttons::NONE);
        }
        assert_eq!(summary.frames, oracle.frame);
        assert_eq!(summary.hash, oracle.framebuffer().frame_hash());
    }

    fn tick(runner: &mut Runner, p1: Buttons) {
        let mut sink = Recorder::default();
        runner.step(InputFrame::from(p1), &mut sink);
    }

    #[test]
    fn the_title_waits_ten_seconds_then_rolls_into_the_demo() {
        let mut runner = Runner::new(game());
        for _ in 0..TITLE_HOLD_TICKS - 1 {
            tick(&mut runner, Buttons::NONE);
            assert_eq!(runner.game().state, State::Title);
        }
        for _ in 0..400 {
            tick(&mut runner, Buttons::NONE);
        }
        assert_ne!(runner.game().state, State::Title, "the demo never started");
    }

    #[test]
    fn using_the_title_menu_keeps_the_demo_away() {
        let mut runner = Runner::new(game());
        let down = Buttons {
            down: true,
            ..Buttons::NONE
        };
        for frame in 0..TITLE_HOLD_TICKS + 400 {
            let input = if frame % 40 < 2 { down } else { Buttons::NONE };
            tick(&mut runner, input);
            assert_eq!(runner.game().state, State::Title, "demo at tick {frame}");
        }
    }

    #[test]
    fn title_down_and_up_move_the_cursor() {
        let mut runner = Runner::new(game());
        let down = Buttons {
            down: true,
            ..Buttons::NONE
        };
        let up = Buttons {
            up: true,
            ..Buttons::NONE
        };
        tick(&mut runner, down);
        assert_eq!(runner.game().menu_selection(), 1);
        tick(&mut runner, Buttons::NONE);
        tick(&mut runner, up);
        assert_eq!(runner.game().menu_selection(), 0);
    }

    #[test]
    fn holding_a_direction_moves_the_cursor_once() {
        let mut runner = Runner::new(game());
        let down = Buttons {
            down: true,
            ..Buttons::NONE
        };
        let up = Buttons {
            up: true,
            ..Buttons::NONE
        };
        for _ in 0..30 {
            tick(&mut runner, down);
        }
        assert_eq!(runner.game().menu_selection(), 1);
        tick(&mut runner, Buttons::NONE);
        for _ in 0..30 {
            tick(&mut runner, up);
        }
        assert_eq!(runner.game().menu_selection(), 0);
    }

    #[test]
    fn down_on_two_players_makes_no_select() {
        let mut runner = Runner::new(game());
        let down = Buttons {
            down: true,
            ..Buttons::NONE
        };
        tick(&mut runner, down);
        tick(&mut runner, Buttons::NONE);
        assert_eq!(runner.game().menu_selection(), 1);
        assert!(!runner.menu_input(down).select);
    }

    #[test]
    fn up_and_down_make_no_select_outside_the_title() {
        let mut runner = Runner::new(game());
        runner.game_mut().state = State::Playing;
        for p1 in [
            Buttons {
                up: true,
                ..Buttons::NONE
            },
            Buttons {
                down: true,
                ..Buttons::NONE
            },
        ] {
            assert!(!runner.menu_input(p1).select);
        }
    }
}
