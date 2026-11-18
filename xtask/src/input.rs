//! Per-frame controller and state trace used by `record-input`.
//!
//! The trace exists to capture the attract/demo timeline: the held P1 buttons
//! (`$07`), the `$00` dispatcher, the stage state `$1B` and the scroll triple
//! `$3E:$3F:$3D`, one line per frame.
//!
//! ```text
//! <frame>:<buttons>  # state=<hh> stage=<hh> scroll=<low>:<high>:<frac> held=<hh>
//! ```
//!
//! `<buttons>` is a comma-separated list in the standard
//! `a,b,select,start,up,down,left,right` order and is empty when no button is
//! held; `held` is the raw `$07` byte. The ROM's controller loop
//! (`$81C2`-`$820D`) shifts the pad in bit-reversed, so in `$07` A is bit 7 and
//! Right is bit 0. The names are logical: they undo that layout, so the
//! `FRAME:buttons:1` replay lines feed the same buttons back through
//! `gradius-diff --script`.
//!
//! The sample is taken at the end of the visible scanlines, before the vblank
//! NMI latches the next pad read. That is the byte the frame's player update
//! actually used, including the attract demo's own writes to `$07`. A trace
//! line labelled `N` therefore describes frame `N - 1`, which is the frame the
//! replay line uses.

use gradius_nes::Emulator;

/// `$07` bit and standard button name, in script order. The ROM stores the
/// pad bit-reversed: A is bit 7, Right is bit 0.
const BUTTON_NAMES: [(u8, &str); 8] = [
    (0x80, "a"),
    (0x40, "b"),
    (0x20, "select"),
    (0x10, "start"),
    (0x08, "up"),
    (0x04, "down"),
    (0x02, "left"),
    (0x01, "right"),
];

/// One frame of the trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputLine {
    pub frame: u64,
    /// `$07`, the buttons held on P1 (ROM bit layout).
    pub buttons: u8,
    /// `$00`, the game-state dispatcher.
    pub state: u8,
    /// `$1B`, the stage state byte.
    pub stage: u8,
    /// `$3E`, `$3F`, `$3D`.
    pub scroll_low: u8,
    pub scroll_high: u8,
    pub scroll_frac: u8,
}

impl InputLine {
    /// Decode the current oracle frame.
    pub fn from_emulator(emulator: &Emulator) -> Self {
        let ram = &emulator.bus.ram;
        Self {
            frame: emulator.frames,
            buttons: ram[0x07],
            state: ram[0x00],
            stage: ram[0x1B],
            scroll_low: ram[0x3E],
            scroll_high: ram[0x3F],
            scroll_frac: ram[0x3D],
        }
    }

    /// Render the line without a trailing newline.
    pub fn render(&self) -> String {
        format!(
            "{}:{}  # state={:02X} stage={:02X} scroll={}:{}:{} held={:02X}",
            self.frame,
            button_names(self.buttons),
            self.state,
            self.stage,
            self.scroll_low,
            self.scroll_high,
            self.scroll_frac,
            self.buttons,
        )
    }

    /// Parse a line produced by [`Self::render`].
    #[cfg(test)]
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let (head, tail) = text
            .split_once("  # ")
            .ok_or_else(|| anyhow::anyhow!("missing trace comment in `{text}`"))?;
        let (frame, buttons) = head
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("missing FRAME:buttons in `{text}`"))?;
        let frame = frame
            .parse()
            .map_err(|_| anyhow::anyhow!("bad frame in `{text}`"))?;
        let mut fields = tail.split_ascii_whitespace();
        let state = parse_hex_field(fields.next(), "state", text)?;
        let stage = parse_hex_field(fields.next(), "stage", text)?;
        let scroll = fields
            .next()
            .and_then(|field| field.strip_prefix("scroll="))
            .ok_or_else(|| anyhow::anyhow!("missing scroll in `{text}`"))?;
        let parts: Vec<&str> = scroll.split(':').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("expected LOW:HIGH:FRAC in `{text}`"));
        }
        let component = |index: usize| -> anyhow::Result<u8> {
            parts[index]
                .parse()
                .map_err(|_| anyhow::anyhow!("bad scroll component in `{text}`"))
        };
        let held = parse_hex_field(fields.next(), "held", text)?;
        Ok(Self {
            frame,
            buttons: held,
            state,
            stage,
            scroll_low: component(0)?,
            scroll_high: component(1)?,
            scroll_frac: component(2)?,
        })
        .and_then(|line| {
            if button_names(line.buttons) != buttons {
                return Err(anyhow::anyhow!("buttons and held disagree in `{text}`"));
            }
            Ok(line)
        })
    }
}

#[cfg(test)]
fn parse_hex_field(field: Option<&str>, key: &str, text: &str) -> anyhow::Result<u8> {
    let raw = field
        .and_then(|field| field.strip_prefix(&format!("{key}=")))
        .ok_or_else(|| anyhow::anyhow!("missing {key} in `{text}`"))?;
    u8::from_str_radix(raw, 16).map_err(|_| anyhow::anyhow!("bad {key} in `{text}`"))
}

/// Render a held-button byte as a comma-separated name list.
pub fn button_names(buttons: u8) -> String {
    BUTTON_NAMES
        .iter()
        .filter(|(bit, _)| buttons & bit != 0)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(",")
}

/// Compress per-frame `(frame, buttons)` samples into `FRAME:buttons:duration`
/// lines. Zero-button frames contribute no press and break a run.
pub fn compress_runs(samples: impl IntoIterator<Item = (u64, u8)>) -> String {
    let mut lines = Vec::new();
    let mut run: Option<(u64, u8, u64)> = None;
    for (frame, buttons) in samples {
        if let Some((start, held, len)) = run {
            if held == buttons && buttons != 0 && start + len == frame {
                run = Some((start, held, len + 1));
                continue;
            }
            lines.push(format!("{start}:{}:{len}", button_names(held)));
        }
        run = (buttons != 0).then_some((frame, buttons, 1));
    }
    if let Some((start, held, len)) = run {
        lines.push(format!("{start}:{}:{len}", button_names(held)));
    }
    lines.join("\n") + if lines.is_empty() { "" } else { "\n" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> InputLine {
        InputLine {
            frame: 133,
            // A (bit 7) and Start (bit 4) in the ROM's layout.
            buttons: 0x90,
            state: 2,
            stage: 0x80,
            scroll_low: 0x2E,
            scroll_high: 1,
            scroll_frac: 0x80,
        }
    }

    #[test]
    fn renders_the_frame_buttons_and_state_columns() {
        assert_eq!(
            sample().render(),
            "133:a,start  # state=02 stage=80 scroll=46:1:128 held=90"
        );
    }

    #[test]
    fn render_and_parse_round_trip() {
        let line = sample();
        assert_eq!(InputLine::parse(&line.render()).unwrap(), line);
    }

    #[test]
    fn empty_button_frames_round_trip() {
        let line = InputLine {
            buttons: 0,
            ..sample()
        };
        let text = line.render();
        assert_eq!(text, "133:  # state=02 stage=80 scroll=46:1:128 held=00");
        assert_eq!(InputLine::parse(&text).unwrap(), line);
    }

    #[test]
    fn runs_compress_only_when_frames_and_buttons_are_contiguous() {
        let script = compress_runs([
            (0, 0),
            (1, 0x80),
            (2, 0x80),
            (3, 0x80),
            (4, 0xC4),
            (5, 0),
            (6, 0x80),
        ]);
        assert_eq!(script, "1:a:3\n4:a,b,down:1\n6:a:1\n");
        assert_eq!(compress_runs([(0, 0), (1, 0)]), "");
    }

    #[test]
    fn button_names_undo_the_rom_bit_reversal() {
        assert_eq!(button_names(0), "");
        // $07 bit 0 is Right, bit 7 is A.
        assert_eq!(button_names(0x01), "right");
        assert_eq!(button_names(0x80), "a");
        assert_eq!(button_names(0x10), "start");
        assert_eq!(button_names(0x90), "a,start");
        assert_eq!(button_names(0xFF), "a,b,select,start,up,down,left,right");
    }

    #[test]
    fn malformed_lines_are_rejected() {
        assert!(InputLine::parse("133:a").is_err());
        assert!(InputLine::parse("x:a  # state=02 stage=80 scroll=0:0:0 held=80").is_err());
        assert!(InputLine::parse("133:a  # state=02 stage=80 scroll=0:0 held=80").is_err());
        // Names and the raw byte must describe the same button set.
        assert!(InputLine::parse("133:a  # state=02 stage=80 scroll=0:0:0 held=00").is_err());
    }
}
