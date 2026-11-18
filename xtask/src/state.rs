//! The stable per-frame state line shared with the native core.
//!
//! `Game::dump_state` in `gradius-core` is the reference format; `record-state`
//! writes the same keys and field order from the oracle so the two files can be
//! diffed line by line. Only integers are printed, so the output is
//! byte-stable across runs and platforms.
//!
//! ```text
//! frame=<u64> state=<Title|Playing|GameOver>
//! px=<u8> pf=<u8> py=<u8> pyf=<u8> speed=<u8>
//! scroll=<low>:<high>:<fraction>
//! score=<u32> lives=<u8> capsule=<u8> meter=<u8>
//! objects=<slot>:<type>:<x_int>:<y_int>:<substate>;...
//! ```
//!
//! The line is one whitespace-separated sequence (the layout above wraps only
//! for readability) and has no trailing newline. `objects` is empty when no
//! object is live. The format is append-only: new fields go at the end.

#[cfg(test)]
use anyhow::{anyhow, bail, Context, Result};
use gradius_nes::Emulator;

/// Native-core game states. The oracle reports the ROM's `$00` byte instead,
/// see [`GameState::from_rom`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    Title,
    Playing,
    GameOver,
}

impl GameState {
    pub fn as_str(self) -> &'static str {
        match self {
            GameState::Title => "Title",
            GameState::Playing => "Playing",
            GameState::GameOver => "GameOver",
        }
    }

    /// Map the ROM's `$00` dispatcher value onto the port's three states.
    ///
    /// `$83E9` dispatches 0/1/2 to attract and demo play, 3 to the game-over /
    /// continue countdown, and 4/5/6 to stage entry, the stage handler and the
    /// ending. The port has no separate demo or ending state, so those collapse
    /// into `Title` and `Playing` respectively.
    pub fn from_rom(value: u8) -> Self {
        match value {
            3 => GameState::GameOver,
            4..=6 => GameState::Playing,
            _ => GameState::Title,
        }
    }

    #[cfg(test)]
    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "Title" => Ok(GameState::Title),
            "Playing" => Ok(GameState::Playing),
            "GameOver" => Ok(GameState::GameOver),
            other => bail!("unknown game state `{other}`"),
        }
    }
}

/// One object-table entry, in the native core's `slot:type:x:y:substate` form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateObject {
    pub slot: u8,
    /// Raw `$030C` type byte (bit 7 is the handler-initialised flag).
    pub kind: u8,
    pub x: u8,
    pub y: u8,
    /// `$048C`, the value the native core calls `ai_state`.
    pub substate: u8,
}

/// One line of the recorder's output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateLine {
    pub frame: u64,
    pub state: GameState,
    pub px: u8,
    pub pf: u8,
    pub py: u8,
    pub pyf: u8,
    pub speed: u8,
    pub scroll_low: u8,
    pub scroll_high: u8,
    pub scroll_frac: u8,
    pub score: u32,
    pub lives: u8,
    pub capsule: u8,
    pub meter: u8,
    pub objects: Vec<StateObject>,
}

impl StateLine {
    /// Decode the current oracle frame.
    ///
    /// Fields with no ROM counterpart are fixed:
    /// * `capsule` is always 0. The ROM keeps no collected-capsule counter; it
    ///   is a port-side HUD convenience (`docs/analysis/ram_map.md`, `$42`).
    /// * `meter` is `$42`, the power-meter selection (0-6).
    pub fn from_emulator(emulator: &Emulator) -> Self {
        let player = emulator.player_state();
        let ram = &emulator.bus.ram;
        Self {
            frame: emulator.frames,
            state: GameState::from_rom(ram[0x00]),
            px: player.x_int,
            pf: player.x_frac,
            py: player.y_int,
            pyf: player.y_frac,
            speed: player.speed_level,
            scroll_low: ram[0x3E],
            scroll_high: ram[0x3F],
            scroll_frac: ram[0x3D],
            score: player.score(),
            lives: player.lives,
            capsule: 0,
            meter: ram[0x42],
            objects: emulator
                .objects()
                .into_iter()
                .map(|object| StateObject {
                    slot: object.slot,
                    kind: object.kind,
                    x: object.x_int,
                    y: object.y_int,
                    substate: object.ai_state,
                })
                .collect(),
        }
    }

    /// Render the line without a trailing newline.
    pub fn render(&self) -> String {
        let mut line = format!(
            "frame={} state={} px={} pf={} py={} pyf={} speed={} scroll={}:{}:{} score={} lives={} capsule={} meter={} objects=",
            self.frame,
            self.state.as_str(),
            self.px,
            self.pf,
            self.py,
            self.pyf,
            self.speed,
            self.scroll_low,
            self.scroll_high,
            self.scroll_frac,
            self.score,
            self.lives,
            self.capsule,
            self.meter,
        );
        for (index, object) in self.objects.iter().enumerate() {
            if index > 0 {
                line.push(';');
            }
            line.push_str(&format!(
                "{}:{}:{}:{}:{}",
                object.slot, object.kind, object.x, object.y, object.substate
            ));
        }
        line
    }

    /// Parse a line produced by [`Self::render`].
    #[cfg(test)]
    pub fn parse(text: &str) -> Result<Self> {
        let fields: Vec<&str> = text.split_ascii_whitespace().collect();
        if fields.len() != 13 {
            bail!(
                "expected 13 fields in a state line, found {} (`{text}`)",
                fields.len()
            );
        }
        let value = |index: usize, key: &str| -> Result<&str> {
            fields[index]
                .strip_prefix(key)
                .ok_or_else(|| anyhow!("field {} should be `{key}` in `{text}`", index + 1))
        };
        let number = |index: usize, key: &str| -> Result<&str> {
            let raw = value(index, &format!("{key}="))?;
            Ok(raw)
        };
        let parse_u8 = |index: usize, key: &str| -> Result<u8> {
            number(index, key)?
                .parse()
                .with_context(|| format!("bad {key} in `{text}`"))
        };

        let frame: u64 = number(0, "frame")?
            .parse()
            .with_context(|| format!("bad frame in `{text}`"))?;
        let state = GameState::parse(number(1, "state")?)?;
        let scroll = number(7, "scroll")?;
        let scroll_parts: Vec<&str> = scroll.split(':').collect();
        if scroll_parts.len() != 3 {
            bail!("expected LOW:HIGH:FRAC in `{text}`");
        }
        let scroll_component = |index: usize| -> Result<u8> {
            scroll_parts[index]
                .parse()
                .with_context(|| format!("bad scroll component in `{text}`"))
        };
        let score: u32 = number(8, "score")?
            .parse()
            .with_context(|| format!("bad score in `{text}`"))?;

        let objects_field = number(12, "objects")?;
        let mut objects = Vec::new();
        if !objects_field.is_empty() {
            for entry in objects_field.split(';') {
                let parts: Vec<&str> = entry.split(':').collect();
                if parts.len() != 5 {
                    bail!("expected slot:type:x:y:substate, got `{entry}`");
                }
                let component = |index: usize| -> Result<u8> {
                    parts[index]
                        .parse()
                        .with_context(|| format!("bad object component in `{entry}`"))
                };
                objects.push(StateObject {
                    slot: component(0)?,
                    kind: component(1)?,
                    x: component(2)?,
                    y: component(3)?,
                    substate: component(4)?,
                });
            }
        }

        Ok(Self {
            frame,
            state,
            px: parse_u8(2, "px")?,
            pf: parse_u8(3, "pf")?,
            py: parse_u8(4, "py")?,
            pyf: parse_u8(5, "pyf")?,
            speed: parse_u8(6, "speed")?,
            scroll_low: scroll_component(0)?,
            scroll_high: scroll_component(1)?,
            scroll_frac: scroll_component(2)?,
            score,
            lives: parse_u8(9, "lives")?,
            capsule: parse_u8(10, "capsule")?,
            meter: parse_u8(11, "meter")?,
            objects,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StateLine {
        StateLine {
            frame: 1234,
            state: GameState::Playing,
            px: 80,
            pf: 128,
            py: 96,
            pyf: 64,
            speed: 2,
            scroll_low: 0x2E,
            scroll_high: 1,
            scroll_frac: 0x80,
            score: 70200,
            lives: 3,
            capsule: 0,
            meter: 4,
            objects: vec![
                StateObject {
                    slot: 0,
                    kind: 1,
                    x: 80,
                    y: 96,
                    substate: 0,
                },
                StateObject {
                    slot: 5,
                    kind: 0x8A,
                    x: 200,
                    y: 64,
                    substate: 0x20,
                },
            ],
        }
    }

    #[test]
    fn render_matches_the_native_line_exactly() {
        assert_eq!(
            sample().render(),
            "frame=1234 state=Playing px=80 pf=128 py=96 pyf=64 speed=2 \
             scroll=46:1:128 score=70200 lives=3 capsule=0 meter=4 \
             objects=0:1:80:96:0;5:138:200:64:32"
        );
    }

    #[test]
    fn render_and_parse_round_trip() {
        let line = sample();
        let parsed = StateLine::parse(&line.render()).expect("parse");
        assert_eq!(parsed, line);
        assert_eq!(parsed.render(), line.render());
    }

    #[test]
    fn an_empty_object_list_round_trips() {
        let mut line = sample();
        line.objects.clear();
        let text = line.render();
        assert!(text.ends_with("objects="));
        assert_eq!(StateLine::parse(&text).expect("parse"), line);
    }

    #[test]
    fn game_state_maps_from_the_rom_dispatch_byte() {
        assert_eq!(GameState::from_rom(0), GameState::Title);
        assert_eq!(GameState::from_rom(2), GameState::Title);
        assert_eq!(GameState::from_rom(3), GameState::GameOver);
        assert_eq!(GameState::from_rom(4), GameState::Playing);
        assert_eq!(GameState::from_rom(5), GameState::Playing);
        assert_eq!(GameState::from_rom(6), GameState::Playing);
    }

    #[test]
    fn malformed_lines_are_rejected() {
        assert!(StateLine::parse("frame=1 state=Playing").is_err());
        let text = sample().render().replace("state=Playing", "state=Demo");
        assert!(StateLine::parse(&text).is_err());
        let text = sample().render().replace("objects=0:1", "objects=0:1:2");
        assert!(StateLine::parse(&text).is_err());
        let text = sample().render().replace("scroll=46:1:128", "scroll=46:1");
        assert!(StateLine::parse(&text).is_err());
    }
}
