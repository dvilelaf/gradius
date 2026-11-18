//! Scripted controller input.
//!
//! A script is a list of timed presses. Each press has a start frame, a button
//! set and a duration (6 frames unless the line gives one explicitly, matching
//! `xtask --press`). Presses that overlap on the same frame are unioned, so a
//! long "hold fire" line and a short steering line can coexist.
//!
//! A line prefixed with `p2` presses the second controller instead:
//!
//! ```text
//! 180:select
//! p2 220:start:6
//! ```
//!
//! Unprefixed lines are P1, so existing scripts keep their meaning. The two
//! pads are tracked separately and returned together by [`Script::pads`].

/// One frame's worth of requested buttons, independent of any device API.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonSet {
    pub a: bool,
    pub b: bool,
    pub select: bool,
    pub start: bool,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl ButtonSet {
    pub const NONE: Self = Self {
        a: false,
        b: false,
        select: false,
        start: false,
        up: false,
        down: false,
        left: false,
        right: false,
    };

    pub fn is_empty(self) -> bool {
        self == Self::NONE
    }

    /// Parse `a,right` or `a+right`. Names are case-insensitive.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut set = Self::NONE;
        for name in spec.split([',', '+']) {
            let name = name.trim().to_ascii_lowercase();
            match name.as_str() {
                "" => continue,
                "a" => set.a = true,
                "b" => set.b = true,
                "select" => set.select = true,
                "start" => set.start = true,
                "up" => set.up = true,
                "down" => set.down = true,
                "left" => set.left = true,
                "right" => set.right = true,
                other => {
                    return Err(format!(
                        "unknown button `{other}` (expected a,b,select,start,up,down,left,right)"
                    ))
                }
            }
        }
        Ok(set)
    }
}

impl std::ops::BitOr for ButtonSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self {
            a: self.a || rhs.a,
            b: self.b || rhs.b,
            select: self.select || rhs.select,
            start: self.start || rhs.start,
            up: self.up || rhs.up,
            down: self.down || rhs.down,
            left: self.left || rhs.left,
            right: self.right || rhs.right,
        }
    }
}

impl std::ops::BitOrAssign for ButtonSet {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

/// One scripted press, held from `at` for `duration` frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press {
    pub at: u64,
    pub buttons: ButtonSet,
    pub duration: u64,
}

/// A full input script.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub presses: Vec<Press>,
    /// Controller 2 presses, written with a `p2` line prefix.
    pub p2_presses: Vec<Press>,
}

/// Both pads' buttons for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pads {
    pub p1: ButtonSet,
    pub p2: ButtonSet,
}

impl Script {
    /// P1 buttons active on `frame` (0-based): the union of every press whose
    /// window contains it.
    pub fn buttons(&self, frame: u64) -> ButtonSet {
        held(&self.presses, frame)
    }

    /// P2 buttons active on `frame`, from the `p2`-prefixed lines.
    pub fn p2_buttons(&self, frame: u64) -> ButtonSet {
        held(&self.p2_presses, frame)
    }

    /// Both pads active on `frame`.
    pub fn pads(&self, frame: u64) -> Pads {
        Pads {
            p1: self.buttons(frame),
            p2: self.p2_buttons(frame),
        }
    }
}

fn held(presses: &[Press], frame: u64) -> ButtonSet {
    let mut set = ButtonSet::NONE;
    for press in presses {
        if frame >= press.at && frame - press.at < press.duration {
            set |= press.buttons;
        }
    }
    set
}

/// Parse a script file. See [`crate`] for the format.
pub fn parse_script(text: &str) -> Result<Script, String> {
    let mut presses = Vec::new();
    let mut p2_presses = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (second_pad, line) = match line.split_once(char::is_whitespace) {
            Some((first, rest)) if first.eq_ignore_ascii_case("p2") => (true, rest.trim()),
            _ => (false, line),
        };
        let parts: Vec<&str> = line.split(':').collect();
        let press = match parts.as_slice() {
            [at, buttons] => press_from(at, buttons, 6),
            [at, buttons, duration] => press_from(
                at,
                buttons,
                duration
                    .trim()
                    .parse()
                    .map_err(|_| format!("line {}: bad duration in `{line}`", index + 1))?,
            ),
            _ => {
                return Err(format!(
                    "line {}: expected [p2] FRAME:buttons[:duration], got `{line}`",
                    index + 1
                ))
            }
        }
        .map_err(|message| format!("line {}: {message}", index + 1))?;
        if second_pad {
            p2_presses.push(press);
        } else {
            presses.push(press);
        }
    }
    presses.sort_by_key(|press| press.at);
    p2_presses.sort_by_key(|press| press.at);
    Ok(Script {
        presses,
        p2_presses,
    })
}

fn press_from(at: &str, buttons: &str, duration: u64) -> Result<Press, String> {
    let at = at.trim();
    let at: u64 = at.parse().map_err(|_| format!("bad frame `{at}`"))?;
    let buttons = ButtonSet::parse(buttons).map_err(|message| message.to_string())?;
    if buttons.is_empty() {
        return Err(format!("press at frame `{at}` has no buttons"));
    }
    if duration == 0 {
        return Err(format!("press at frame `{at}` lasts zero frames"));
    }
    Ok(Press {
        at,
        buttons,
        duration,
    })
}

/// The deterministic default described in the crate docs: Start at 180 and
/// 240, then fire held and up/right/left alternating in 60-frame blocks from
/// frame 300.
pub fn default_script(frames: u64) -> Script {
    let mut presses = vec![
        Press {
            at: 180,
            buttons: ButtonSet {
                start: true,
                ..ButtonSet::NONE
            },
            duration: 6,
        },
        Press {
            at: 240,
            buttons: ButtonSet {
                start: true,
                ..ButtonSet::NONE
            },
            duration: 6,
        },
    ];
    let steering = [
        ButtonSet {
            up: true,
            ..ButtonSet::NONE
        },
        ButtonSet {
            right: true,
            ..ButtonSet::NONE
        },
        ButtonSet {
            left: true,
            ..ButtonSet::NONE
        },
    ];
    let fire = ButtonSet {
        a: true,
        ..ButtonSet::NONE
    };
    let mut at = 300;
    let mut block = 0;
    while at < frames {
        presses.push(Press {
            at,
            buttons: fire | steering[block % steering.len()],
            duration: 60,
        });
        at += 60;
        block += 1;
    }
    Script {
        presses,
        p2_presses: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_buttons_in_any_case_and_separator() {
        let set = ButtonSet::parse("A, Right+up").unwrap();
        assert!(set.a && set.right && set.up);
        assert!(!set.b && !set.start);
        assert_eq!(set, ButtonSet::parse("up+right+a").unwrap());
    }

    #[test]
    fn rejects_unknown_buttons() {
        let error = ButtonSet::parse("jump").unwrap_err();
        assert!(error.contains("unknown button `jump`"), "{error}");
    }

    #[test]
    fn parses_lines_comments_and_durations() {
        let script =
            parse_script("\n# boot\n180:start\n240:start:2\n  300:a, right : 90  # long hold\n")
                .unwrap();
        assert_eq!(script.presses.len(), 3);
        assert_eq!(script.presses[0].at, 180);
        assert_eq!(script.presses[0].duration, 6);
        assert_eq!(script.presses[1].duration, 2);
        assert_eq!(script.presses[2].duration, 90);
        assert!(script.presses[2].buttons.a && script.presses[2].buttons.right);
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse_script("180").unwrap_err().contains("expected"));
        assert!(parse_script("x:start").unwrap_err().contains("bad frame"));
        assert!(parse_script("180:start:0")
            .unwrap_err()
            .contains("zero frames"));
        assert!(parse_script("180:").unwrap_err().contains("no buttons"));
    }

    #[test]
    fn presses_are_held_for_their_window_and_overlaps_union() {
        let script = parse_script("10:start\n12:a:10\n20:right").unwrap();
        assert_eq!(script.buttons(9), ButtonSet::NONE);
        assert!(script.buttons(10).start);
        assert!(!script.buttons(16).start);
        // The fire press overlaps the tail of the Start press.
        assert!(script.buttons(12).a && script.buttons(12).start);
        assert!(script.buttons(21).a && !script.buttons(21).start);
        assert!(script.buttons(21).right);
        assert_eq!(script.buttons(100), ButtonSet::NONE);
    }

    #[test]
    fn parses_second_pad_lines_separately() {
        let script = parse_script("# 2P\n180:start\np2 220:start\np2 1230:right:60").unwrap();
        assert_eq!(script.presses.len(), 1);
        assert_eq!(script.p2_presses.len(), 2);
        assert!(script.buttons(180).start);
        assert!(script.p2_buttons(220).start);
        assert!(script.p2_buttons(1230).right);
        assert_eq!(script.buttons(1230), ButtonSet::NONE);
        let pads = script.pads(220);
        assert!(pads.p1.is_empty());
        assert!(pads.p2.start);
    }

    #[test]
    fn rejects_a_line_that_only_names_the_second_pad() {
        assert!(parse_script("p2").unwrap_err().contains("expected"));
    }

    #[test]
    fn default_script_has_the_documented_shape() {
        let script = default_script(600);
        assert_eq!(script.buttons(179), ButtonSet::NONE);
        assert!(script.buttons(180).start);
        assert!(!script.buttons(186).start);
        assert!(script.buttons(240).start);
        assert!(!script.buttons(300).start);
        assert!(script.buttons(300).a);
        assert!(script.buttons(300).up);
        assert!(script.buttons(359).up);
        assert!(script.buttons(360).right);
        assert!(script.buttons(420).left);
        assert!(script.buttons(480).up);
        assert!(script.buttons(599).a);
        // Fire is held from 300 to the end of the run.
        assert!((300..600).all(|frame| script.buttons(frame).a));
    }
}
