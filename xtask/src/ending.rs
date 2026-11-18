//! The stage-6 hazard and the stage-7 loop, one line per frame.
//!
//! `record-ending` pokes `$26` until the requested stage starts (stage 1 needs
//! no poke) and then records the bytes the stage-end state machine uses:
//!
//! ```text
//! frame=<n> state=<Title|Playing|GameOver> phase=<hh> stage=<u8>
//! scroll=<low>:<high>:<fraction> score=<u32> lives=<u8> loop=<u8>
//! stagebyte=<u8> px=<u8> py=<u8> x39=<u8> x3a=<u8> x66=<u8>
//! beams=<slot>:<x>:<y>;...
//! ```
//!
//! * `phase` is `$1B`, the stage state machine (`$80` normal play, `$86` the
//!   no-core stage path, `$87` the stage-7 ending, `$90` stage clear).
//! * `stage` is `$19` (0 = stage 1, 5 = stage 6, 6 = stage 7).
//! * `loop` is `$28`, the per-player loop counter incremented by `$9865`.
//! * `stagebyte` is `$26`, the per-player stage copied to `$19` at init.
//! * `x39`/`x3a` are `$39`/`$3A` (the `$992E` stage-end branch), `x66` is
//!   `$66`, the `$CD9F`/index into the `$CE2B` erase table.
//! * `beams` lists the stage-6 collision-only hazards: collision slot `i`
//!   whose `$0136 = $8D`, with X `$0376` and Y `$0336`. `beams=` is empty
//!   when none are live.
//!
//! The run holds fire, presses Start at frame 180 and pins the player-death
//! bytes (`--keep-alive` behaviour) so a scripted ship reaches the endgame.

use anyhow::{bail, Context, Result};
use gradius_nes::{Emulator, BUTTON_A, BUTTON_START};
use std::fs;
use std::path::Path;

use crate::screens;

/// `$0136` value written by the stage-6 `$C6D8` beam spawner.
pub const BEAM_MARKER: u8 = 0x8D;

/// A live stage-6 collision beam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Beam {
    pub slot: u8,
    pub x: u8,
    pub y: u8,
}

/// Every collision slot whose `$0136` is the stage-6 beam marker.
pub fn beams(ram: &[u8]) -> Vec<Beam> {
    (0..10u8)
        .filter(|slot| ram[0x0136 + usize::from(*slot)] == BEAM_MARKER)
        .map(|slot| Beam {
            slot,
            x: ram[0x0376 + usize::from(slot)],
            y: ram[0x0336 + usize::from(slot)],
        })
        .collect()
}

/// One recorded frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndingLine {
    pub frame: u64,
    pub state: crate::state::GameState,
    pub phase: u8,
    pub stage: u8,
    pub scroll_low: u8,
    pub scroll_high: u8,
    pub scroll_frac: u8,
    pub score: u32,
    pub lives: u8,
    pub loop_count: u8,
    pub stage_byte: u8,
    pub px: u8,
    pub py: u8,
    pub x39: u8,
    pub x3a: u8,
    pub x66: u8,
    pub beams: Vec<Beam>,
}

impl EndingLine {
    pub fn from_emulator(emulator: &Emulator) -> Self {
        let ram = &emulator.bus.ram;
        let player = emulator.player_state();
        Self {
            frame: emulator.frames,
            state: crate::state::GameState::from_rom(ram[0x00]),
            phase: ram[0x1B],
            stage: ram[0x19],
            scroll_low: ram[0x3E],
            scroll_high: ram[0x3F],
            scroll_frac: ram[0x3D],
            score: player.score(),
            lives: player.lives,
            loop_count: ram[0x28],
            stage_byte: ram[0x26],
            px: player.x_int,
            py: player.y_int,
            x39: ram[0x39],
            x3a: ram[0x3A],
            x66: ram[0x66],
            beams: beams(ram),
        }
    }

    pub fn render(&self) -> String {
        let mut line = format!(
            "frame={} state={} phase={:02X} stage={} scroll={}:{}:{} score={} lives={} loop={} stagebyte={} px={} py={} x39={} x3a={} x66={} beams=",
            self.frame,
            self.state.as_str(),
            self.phase,
            self.stage,
            self.scroll_low,
            self.scroll_high,
            self.scroll_frac,
            self.score,
            self.lives,
            self.loop_count,
            self.stage_byte,
            self.px,
            self.py,
            self.x39,
            self.x3a,
            self.x66,
        );
        for (index, beam) in self.beams.iter().enumerate() {
            if index > 0 {
                line.push(';');
            }
            line.push_str(&format!("{}:{}:{}", beam.slot, beam.x, beam.y));
        }
        line
    }
}

/// Record `frames` frames of the endgame starting from 1-based `stage`.
pub fn run(
    rom: &Path,
    frames: u64,
    stage: u8,
    out: Option<&Path>,
    nametable: Option<&Path>,
) -> Result<()> {
    if frames == 0 {
        bail!("--frames must be at least 1");
    }
    if !(1..=7).contains(&stage) {
        bail!("--stage must be 1-7, got {stage}");
    }
    let target = stage - 1;
    let cart = crate::load(rom)?;
    let mut emulator = Emulator::new(&cart)?;
    let mut runner = crate::oracle::Runner::new();
    runner.keep_alive = true;
    let mut started = false;
    let mut text = String::with_capacity(frames as usize * 110);
    while emulator.frames < frames {
        let ram = &mut emulator.bus.ram;
        // Poke only before the first start; after the stage-7 loop `$19` is 0
        // and poking again would restart stage 7 instead of stage 1.
        if !started && screens::needs_stage_poke(ram[0x00], ram[0x19], target) {
            ram[0x26] = target;
        }
        let mut buttons = BUTTON_A;
        if (180..186).contains(&emulator.frames) {
            buttons |= BUTTON_START;
        }
        let plan = crate::oracle::Plan {
            held: 0,
            schedule: vec![(emulator.frames, buttons, 1)],
        };
        runner.advance(&mut emulator, &plan, false);
        if emulator.bus.ram[0x00] == 5 && emulator.bus.ram[0x19] == target {
            started = true;
        }
        text.push_str(&EndingLine::from_emulator(&emulator).render());
        text.push('\n');
    }
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            println!(
                "wrote {} ({} frames, {} bytes)",
                path.display(),
                frames,
                text.len()
            );
        }
        None => print!("{text}"),
    }
    if let Some(path) = nametable {
        fs::write(path, &emulator.bus.ppu.vram[..0x800])
            .with_context(|| format!("writing {}", path.display()))?;
        println!("nametable written to {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_scan_uses_the_marker_and_the_collision_arrays() {
        let mut ram = vec![0u8; 0x800];
        ram[0x0136] = BEAM_MARKER;
        ram[0x0376] = 152;
        ram[0x0336] = 66;
        ram[0x0137] = 1; // ordinary collision flag: not a beam
        ram[0x0138] = BEAM_MARKER;
        ram[0x0378] = 152;
        ram[0x0338] = 132;
        assert_eq!(
            beams(&ram),
            vec![
                Beam {
                    slot: 0,
                    x: 152,
                    y: 66
                },
                Beam {
                    slot: 2,
                    x: 152,
                    y: 132
                },
            ]
        );
    }

    #[test]
    fn ending_line_renders_the_endgame_fields() {
        let line = EndingLine {
            frame: 7000,
            state: crate::state::GameState::Playing,
            phase: 0x86,
            stage: 5,
            scroll_low: 0,
            scroll_high: 0x0C,
            scroll_frac: 128,
            score: 920,
            lives: 3,
            loop_count: 0,
            stage_byte: 5,
            px: 80,
            py: 96,
            x39: 0,
            x3a: 0,
            x66: 20,
            beams: vec![
                Beam {
                    slot: 0,
                    x: 152,
                    y: 66,
                },
                Beam {
                    slot: 1,
                    x: 152,
                    y: 132,
                },
            ],
        };
        assert_eq!(
            line.render(),
            "frame=7000 state=Playing phase=86 stage=5 scroll=0:12:128 score=920 \
             lives=3 loop=0 stagebyte=5 px=80 py=96 x39=0 x3a=0 x66=20 \
             beams=0:152:66;1:152:132"
        );
    }

    #[test]
    fn an_empty_beam_list_renders_an_empty_field() {
        let line = EndingLine {
            frame: 1,
            state: crate::state::GameState::Title,
            phase: 0,
            stage: 0,
            scroll_low: 0,
            scroll_high: 0,
            scroll_frac: 0,
            score: 0,
            lives: 0,
            loop_count: 0,
            stage_byte: 0,
            px: 0,
            py: 0,
            x39: 0,
            x3a: 0,
            x66: 0,
            beams: Vec::new(),
        };
        assert!(line.render().ends_with(" beams="));
    }
}
