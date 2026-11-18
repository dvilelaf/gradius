//! The natural stage-1 clear must hand the soundtrack to stage 2's `$59`.
//!
//! `scripts/boss-script.txt` is the recorded `gradius-diff --keep-alive` run:
//! the Big Core dies at 9087 and stage 2 starts at 9601. This test replays it
//! through the real core and checks the cue the runner would emit.

use gradius_app::{Buttons, Game, MusicCue, MusicDirector};

mod support;
use support::assets;

const FRAMES: u64 = 10_000;

fn script(text: &str, frames: u64) -> Vec<Buttons> {
    let mut out = vec![Buttons::NONE; frames as usize];
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(':');
        let at: u64 = parts.next().expect("frame").trim().parse().expect("frame");
        let spec = parts.next().expect("buttons");
        let duration: u64 = parts
            .next()
            .map_or(6, |d| d.trim().parse().expect("duration"));
        let mut press = Buttons::NONE;
        for name in spec.split('+') {
            match name {
                "a" => press.fire = true,
                "b" => press.power = true,
                "select" => press.select = true,
                "start" => press.start = true,
                "up" => press.up = true,
                "down" => press.down = true,
                "left" => press.left = true,
                "right" => press.right = true,
                other => panic!("unknown button {other}"),
            }
        }
        for frame in at..(at + duration).min(frames) {
            let held = &mut out[frame as usize];
            held.fire |= press.fire;
            held.power |= press.power;
            held.select |= press.select;
            held.start |= press.start;
            held.up |= press.up;
            held.down |= press.down;
            held.left |= press.left;
            held.right |= press.right;
        }
    }
    out
}

#[test]
fn the_natural_stage_two_handoff_selects_the_stage_two_theme() {
    let frames = script(include_str!("../../../scripts/boss-script.txt"), FRAMES);
    let mut game = Game::new(assets());
    game.dev_keep_alive(true);
    let mut director = MusicDirector::new();
    let mut seen = Vec::new();
    let mut handoff_cue = None;

    for &buttons in &frames {
        game.step(buttons);
        if let Some(cue) = director.update(&game) {
            seen.push(cue);
            if game.section() == 1 && handoff_cue.is_none() {
                handoff_cue = Some(cue);
            }
        }
    }

    assert_eq!(game.section(), 1, "the run must reach stage 2");
    assert_eq!(game.stage_music(), 0x59, "stage 2's `$8344` theme");
    assert!(
        seen.contains(&MusicCue::Stage1B),
        "stage 1 second half never played"
    );
    assert!(
        seen.contains(&MusicCue::StageClear),
        "the stage-1 clear fanfare never played"
    );
    assert_eq!(
        handoff_cue,
        Some(MusicCue::Stage2),
        "stage 2 must start on `$59`, not the stale clear cue"
    );
    assert_eq!(director.cue(), MusicCue::Stage2);
}
