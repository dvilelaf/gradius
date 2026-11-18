//! Product smoke: the whole front-end flow, on the extracted asset pack.
//!
//! No ROM: the title/menu, a 1P run with movement, fire and a real
//! capsule power-up, a forced game over and the `$977C` continue, a return to
//! the menu for a 2P game with both pads, and a save-state round trip. Every
//! step goes through [`Runner`], the same path the SDL/wasm frontends use, so
//! a panic or a broken phase/`$18` transition fails here.
//!
//! What cannot be provoked without a ROM is documented where it matters: the
//! port has no in-game menu, so the 2P game is reached by letting a second
//! game over expire to the title, exactly as a 1P cabinet would.

use gradius_app::{
    Buttons, FrameSink, Game, InputFrame, MusicCue, Runner, Sound, SoundSink, HEIGHT, WIDTH,
};
use gradius_core::twoplayer::Players;
use gradius_core::State;

mod support;
use support::assets;

const NONE: Buttons = Buttons::NONE;
const NORMAL_PLAY: &str = "state=Playing ";

/// Records what a backend would consume: sounds, music cues and frames.
#[derive(Default)]
struct Recorder {
    sounds: Vec<Sound>,
    cues: Vec<MusicCue>,
    frames: usize,
    frame_bytes: usize,
}

impl SoundSink for Recorder {
    fn sound(&mut self, sound: Sound) {
        self.sounds.push(sound);
    }

    fn music(&mut self, cue: MusicCue) {
        self.cues.push(cue);
    }
}

impl FrameSink for Recorder {
    fn frame(&mut self, rgba: &[u8]) -> Result<(), String> {
        self.frames += 1;
        self.frame_bytes = rgba.len();
        Ok(())
    }
}

/// One frame with P1's pad.
fn step(runner: &mut Runner, sink: &mut Recorder, p1: Buttons) {
    runner.step(InputFrame::from(p1), sink);
}

/// One frame with both pads.
fn step_both(runner: &mut Runner, sink: &mut Recorder, p1: Buttons, p2: Buttons) {
    runner.step(InputFrame { p1, p2 }, sink);
}

/// Press and release: the ROM samples edges, so a tap needs the release frame.
fn tap(runner: &mut Runner, sink: &mut Recorder, button: Buttons) {
    step(runner, sink, button);
    step(runner, sink, NONE);
}

/// A varied 1P script: fire held, vertical weaving and a short left drift.
fn varied(frame: u64) -> Buttons {
    let phase = (frame / 90) % 4;
    Buttons {
        fire: true,
        up: phase == 0,
        down: phase == 2,
        left: phase == 3 && frame % 90 < 20,
        ..NONE
    }
}

/// `$977C`: Down, Up, B, A, B, A, B, A, B, A.
fn continue_code() -> [Buttons; 10] {
    [
        Buttons { down: true, ..NONE },
        Buttons { up: true, ..NONE },
        Buttons {
            power: true,
            ..NONE
        },
        Buttons { fire: true, ..NONE },
        Buttons {
            power: true,
            ..NONE
        },
        Buttons { fire: true, ..NONE },
        Buttons {
            power: true,
            ..NONE
        },
        Buttons { fire: true, ..NONE },
        Buttons {
            power: true,
            ..NONE
        },
        Buttons { fire: true, ..NONE },
    ]
}

/// Step until the dump reports normal play, up to `cap` frames.
fn wait_for_normal_play(runner: &mut Runner, sink: &mut Recorder, cap: u32) {
    let mut frames = 0;
    while !runner.game().dump_state().contains(NORMAL_PLAY) {
        assert!(frames < cap, "never reached normal play in {cap} frames");
        step(runner, sink, NONE);
        frames += 1;
    }
}

/// Crash the current ship until the game-over countdown is up.
///
/// Holding Right flies the ship into the stage-1 formations, which is the
/// quickest death the public pad can cause; the loop is deterministic.
fn force_game_over(runner: &mut Runner, sink: &mut Recorder, cap: u32) {
    let mut frames = 0;
    while !runner.game().two_player().is_game_over() {
        assert!(frames < cap, "game over never arrived in {cap} frames");
        step(
            runner,
            sink,
            Buttons {
                right: true,
                ..NONE
            },
        );
        frames += 1;
    }
}

#[test]
fn the_whole_game_flow_survives_menu_one_player_continue_two_player_and_a_restore() {
    let mut sink = Recorder::default();
    let mut run = Runner::new(Game::new(assets()));

    // -- Boot: the extracted title is present, so the game starts in the menu.
    assert_eq!(run.game().state, State::Title, "boot enters the title");
    assert_eq!(run.game().players(), Players::One, "the menu starts on 1P");
    assert_eq!(run.game().current_player(), 0, "`$18` is P1");
    assert_eq!(run.game().lives, 0, "no run has started yet");
    assert_eq!(run.game().section(), 0);
    run.present(&mut sink).expect("present");
    assert_eq!(sink.frames, 1);
    assert_eq!(sink.frame_bytes, WIDTH * HEIGHT * 4, "one RGBA frame");

    // -- Menu: Select cycles 1P -> 2P -> 1P, Start locks 1P.
    tap(
        &mut run,
        &mut sink,
        Buttons {
            select: true,
            ..NONE
        },
    );
    tap(
        &mut run,
        &mut sink,
        Buttons {
            select: true,
            ..NONE
        },
    );
    tap(
        &mut run,
        &mut sink,
        Buttons {
            start: true,
            ..NONE
        },
    );
    assert_eq!(run.game().state, State::Playing, "Start leaves the title");
    assert_eq!(run.game().players(), Players::One, "Start locked 1P");
    assert_eq!(run.game().current_player(), 0, "`$18` stays P1");

    wait_for_normal_play(&mut run, &mut sink, 600);
    assert_eq!(run.game().section(), 0, "the run starts on stage 1");
    assert_eq!(run.game().lives, 3, "three spare ships");
    assert_eq!(run.game().score, 0, "a fresh run is scoreless");
    assert!(
        sink.cues.contains(&MusicCue::Stage1),
        "the stage-1 theme reached the sink"
    );

    // -- 1200 frames of 1P play: move, shoot, and take a hit or two.
    let before = run.game().frame;
    for frame in 0..1_200 {
        step(&mut run, &mut sink, varied(frame));
        assert_eq!(run.game().state, State::Playing, "play stays in Playing");
        assert_eq!(run.game().current_player(), 0, "`$18` stays P1 in 1P");
        assert!(
            run.game().lives <= 4,
            "lives counter stays sane while playing: {}",
            run.game().lives
        );
        assert!(
            !run.game().two_player().is_game_over(),
            "the 1P script must not exhaust all ships"
        );
    }
    assert_eq!(run.game().frame, before + 1_200, "1200 whole frames");
    assert_eq!(run.game().section(), 0, "the script never left stage 1");
    assert!(run.game().score > 0, "firing connected with enemies");

    // -- Power-up: collect a real capsule, then redeem it with B.
    run.game_mut().dev_spawn_capsule_at_player();
    let mut picked = false;
    for _ in 0..120 {
        step(&mut run, &mut sink, NONE);
        if run.game().power().capsules() > 0 {
            picked = true;
            break;
        }
    }
    assert!(picked, "the capsule at the ship is collected");
    assert_eq!(run.game().power().selection(), 1, "`$42` highlights SPEED");
    // `$07` reaches the power handler one NMI late, so B must be held twice.
    step(
        &mut run,
        &mut sink,
        Buttons {
            power: true,
            ..NONE
        },
    );
    step(&mut run, &mut sink, NONE);
    assert_eq!(
        run.game().power().speed_level(),
        1,
        "`$40` speed-up applied"
    );
    assert_eq!(run.game().power().selection(), 0, "`$42` cleared");
    assert!(
        sink.sounds.contains(&Sound::PowerUp),
        "the power-up cue reached the sink"
    );

    // -- Force the game over and complete the `$977C` continue code.
    force_game_over(&mut run, &mut sink, 20_000);
    assert_eq!(run.game().current_player(), 0, "`$18` is P1 at game over");
    assert_eq!(run.game().lives, 0xFF, "the ROM lets `$20` wrap past zero");
    assert_eq!(run.game().two_player().alive_mask(), 0, "both bits clear");
    assert_eq!(
        run.game().two_player().continue_index,
        0,
        "no continue entered yet"
    );
    assert!(
        sink.sounds.contains(&Sound::PlayerDeath),
        "deaths reached the sink"
    );

    for press in continue_code() {
        step(&mut run, &mut sink, press);
    }
    assert_eq!(
        run.game().two_player().continue_index,
        10,
        "all ten entries matched"
    );

    let mut resumed = 0;
    while run.game().two_player().is_game_over() || !run.game().dump_state().contains(NORMAL_PLAY) {
        assert!(resumed < 1_000, "the continue never resumed play");
        step(&mut run, &mut sink, NONE);
        resumed += 1;
    }
    assert_eq!(run.game().state, State::Playing);
    assert_eq!(run.game().players(), Players::One, "same 1P game");
    assert_eq!(run.game().current_player(), 0, "`$18` resumed on P1");
    assert_eq!(run.game().lives, 3, "`$973E` restores three ships");
    assert_eq!(run.game().two_player().slots[0].lives, 3);
    assert_eq!(run.game().two_player().slots[0].extra_life, 1);
    assert_eq!(run.game().two_player().slots[0].score, 0, "score reset");
    assert_eq!(run.game().score, 0);
    assert_eq!(
        run.game().two_player().slots[0].checkpoint.scroll,
        0,
        "`$24` cleared to the checkpoint column"
    );
    assert_eq!(
        run.game().two_player().continue_index,
        0,
        "`$97D0` clears `$33`"
    );
    assert_eq!(run.game().section(), 0, "the stage rebuilt from the start");

    // -- Play again for a while, then reach the menu the only way there is:
    //    a second game over, because the port has no in-game menu.
    for frame in 0..300 {
        step(&mut run, &mut sink, varied(frame + 1_200));
    }
    assert_eq!(run.game().state, State::Playing);

    force_game_over(&mut run, &mut sink, 20_000);
    let mut window = 0;
    while run.game().state != State::Title {
        assert!(window < 1_000, "the game-over window never expired");
        step(&mut run, &mut sink, NONE);
        window += 1;
    }
    assert_eq!(run.game().lives, 0, "the title loads no ships");

    // -- 2P: Select once locks the second menu entry.
    tap(
        &mut run,
        &mut sink,
        Buttons {
            select: true,
            ..NONE
        },
    );
    tap(
        &mut run,
        &mut sink,
        Buttons {
            start: true,
            ..NONE
        },
    );
    assert_eq!(run.game().state, State::Playing, "Start begins the 2P game");
    assert_eq!(run.game().players(), Players::Two, "Start locked 2P");
    assert_eq!(run.game().current_player(), 0, "`$18` starts on P1");
    wait_for_normal_play(&mut run, &mut sink, 600);
    assert_eq!(
        run.game().two_player().slots.map(|slot| slot.lives),
        [3, 3],
        "both banks get three ships"
    );

    // P1 dies on purpose: `$18` must hand over to P2 with P1's bank saved.
    let mut handoff = 0;
    while run.game().current_player() == 0 {
        assert!(handoff < 2_000, "P1 never handed over to P2");
        step_both(
            &mut run,
            &mut sink,
            Buttons {
                right: true,
                ..NONE
            },
            Buttons { left: true, ..NONE },
        );
        handoff += 1;
    }
    assert_eq!(run.game().current_player(), 1, "`$18` 0 -> 1");
    assert_eq!(
        run.game().two_player().slots[0].lives,
        2,
        "P1 lost one ship"
    );
    assert_eq!(run.game().two_player().slots[1].lives, 3, "P2 untouched");
    assert_eq!(run.game().lives, 2, "the dump keeps P1's `$20` bank");
    assert_eq!(
        run.game().two_player().alive_mask(),
        3,
        "both players alive"
    );

    // -- 1200 frames with both pads. The current player's pad drives the ship;
    //    the other pad must stay inert until the turn changes.
    let mut seen_p2 = false;
    for frame in 0..1_200 {
        step_both(&mut run, &mut sink, varied(frame), varied(frame + 45));
        let current = run.game().current_player();
        assert!(current < 2, "`$18` is a player index");
        seen_p2 |= current == 1;
        assert_eq!(run.game().state, State::Playing, "2P stays in Playing");
        assert!(
            !run.game().two_player().is_game_over(),
            "1200 2P frames must not exhaust both players"
        );
        assert!(
            run.game().two_player().slots[current].alive,
            "the player in `$18` is alive"
        );
        assert!(
            run.game()
                .two_player()
                .slots
                .iter()
                .all(|slot| slot.lives <= 4),
            "both life counters stay sane"
        );
    }
    assert!(seen_p2, "the 2P game used the second pad's turn");
    assert!(
        run.game()
            .two_player()
            .slots
            .iter()
            .any(|slot| slot.score > 0),
        "the two banks scored"
    );

    // -- Save state: snapshot, advance, restore in place and in a fresh game.
    let saved = run.game().dump_state();
    let saved_player = run.game().current_player();
    let image = run.game().snapshot();
    assert!(image.len() > 56, "a framed GRSS image");

    for frame in 0..120 {
        step_both(&mut run, &mut sink, varied(frame + 7), varied(frame + 52));
    }
    assert_ne!(run.game().dump_state(), saved, "the game advanced");

    run.game_mut().restore(&image).expect("restore in place");
    assert_eq!(run.game().dump_state(), saved, "restore rewinds the state");
    assert_eq!(
        run.game().current_player(),
        saved_player,
        "`$18` survives the restore"
    );

    // A fresh game restored from the same image must match frame by frame.
    let mut restored = Game::new(assets());
    restored.restore(&image).expect("restore into a fresh game");
    assert_eq!(restored.dump_state(), saved, "same state after restore");
    assert_eq!(restored.current_player(), saved_player, "`$18` restored");
    assert_eq!(
        restored.snapshot(),
        image,
        "the image re-encodes identically"
    );

    for frame in 0..60 {
        let p1 = varied(frame + 300);
        let p2 = varied(frame + 345);
        step_both(&mut run, &mut sink, p1, p2);
        restored.step_players(p1, p2);
        assert_eq!(
            restored.dump_state(),
            run.game().dump_state(),
            "dump diverged {} frames after the restore",
            frame + 1
        );
        assert_eq!(
            restored.state_hash(),
            run.game().state_hash(),
            "framebuffer diverged {} frames after the restore",
            frame + 1
        );
        assert_eq!(restored.lives, run.game().lives);
        assert_eq!(restored.current_player(), run.game().current_player());
    }
    assert_eq!(
        run.game().frame,
        restored.frame,
        "the restored game advanced the same 60 frames"
    );
    assert!(
        !sink.sounds.is_empty() && sink.frames == 1,
        "the sink saw sounds and exactly the one presented frame"
    );
}
