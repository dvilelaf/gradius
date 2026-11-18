//! User-editable keyboard bindings.
//!
//! The windowed frontend reads `keybinds.ini` from the data folder (see
//! `datadir`). If the file is missing
//! it is created with the defaults and comments; parse errors warn and fall
//! back to the defaults, so a bad edit never stops the game from starting.
//! Pause, F3, F5, F9, F11, F12, Esc, M and the volume keys are handled by the
//! event loop and stay reserved.

use std::io::ErrorKind;
use std::path::Path;

use gradius_app::InputFrame;
use gradius_core::Buttons;
use sdl2::keyboard::Scancode;

pub const FILE_NAME: &str = "keybinds.ini";

/// Written when the file is absent. Parsing it must produce the defaults; the
/// `default_template_round_trips` test enforces that.
const DEFAULT_TEMPLATE: &str = "\
# Gradius keyboard bindings.
#
# One line per NES button: `button = key` or a comma-separated list of keys.
# Key names: letters and digits, Up, Down, Left, Right, Space, Return,
# Return2 (numpad Enter), Backspace, Tab, Delete, Kp0..Kp9, KpPeriod,
# KpPlus and KpMinus.
#
# Pause, F3, F5, F9, F11, F12, Esc, M, - and + are reserved hotkeys.
# A key may not appear twice, including across both players.
# Delete this file to restore the defaults.

[player1]
up = Up, W
down = Down, S
left = Left, A
right = Right, D
fire = Z, J, Space
power = X, K
start = Return, Return2
select = Backspace, Tab

[player2]
up = Kp8, T
down = Kp5, G
left = Kp4, F
right = Kp6, H
fire = Kp0, R
power = KpPeriod, Delete, Y
start = KpPlus, Num1
select = KpMinus, Num2
";

const RESERVED: [Scancode; 7] = [
    Scancode::Pause,
    Scancode::F11,
    Scancode::F12,
    Scancode::Escape,
    Scancode::M,
    Scancode::Minus,
    Scancode::Equals,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerKeys {
    pub up: Vec<Scancode>,
    pub down: Vec<Scancode>,
    pub left: Vec<Scancode>,
    pub right: Vec<Scancode>,
    pub fire: Vec<Scancode>,
    pub power: Vec<Scancode>,
    pub start: Vec<Scancode>,
    pub select: Vec<Scancode>,
}

impl PlayerKeys {
    fn buttons(&self, pressed: &dyn Fn(Scancode) -> bool) -> Buttons {
        let any = |codes: &[Scancode]| codes.iter().any(|code| pressed(*code));
        Buttons {
            up: any(&self.up),
            down: any(&self.down),
            left: any(&self.left),
            right: any(&self.right),
            fire: any(&self.fire),
            power: any(&self.power),
            start: any(&self.start),
            select: any(&self.select),
        }
    }

    /// Every bound scancode, in button order.
    #[cfg(test)]
    pub fn scancodes(&self) -> impl Iterator<Item = Scancode> + '_ {
        self.entries()
            .into_iter()
            .flat_map(|(_, codes)| codes.iter().copied())
    }

    fn entries(&self) -> [(&'static str, &[Scancode]); 8] {
        [
            ("up", &self.up),
            ("down", &self.down),
            ("left", &self.left),
            ("right", &self.right),
            ("fire", &self.fire),
            ("power", &self.power),
            ("start", &self.start),
            ("select", &self.select),
        ]
    }

    fn button(&mut self, name: &str) -> Option<&mut Vec<Scancode>> {
        Some(match name {
            "up" => &mut self.up,
            "down" => &mut self.down,
            "left" => &mut self.left,
            "right" => &mut self.right,
            "fire" => &mut self.fire,
            "power" => &mut self.power,
            "start" => &mut self.start,
            "select" => &mut self.select,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keybinds {
    pub p1: PlayerKeys,
    pub p2: PlayerKeys,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            p1: PlayerKeys {
                up: vec![Scancode::Up, Scancode::W],
                down: vec![Scancode::Down, Scancode::S],
                left: vec![Scancode::Left, Scancode::A],
                right: vec![Scancode::Right, Scancode::D],
                fire: vec![Scancode::Z, Scancode::J, Scancode::Space],
                power: vec![Scancode::X, Scancode::K],
                start: vec![Scancode::Return, Scancode::Return2],
                select: vec![Scancode::Backspace, Scancode::Tab],
            },
            p2: PlayerKeys {
                up: vec![Scancode::Kp8, Scancode::T],
                down: vec![Scancode::Kp5, Scancode::G],
                left: vec![Scancode::Kp4, Scancode::F],
                right: vec![Scancode::Kp6, Scancode::H],
                fire: vec![Scancode::Kp0, Scancode::R],
                power: vec![Scancode::KpPeriod, Scancode::Delete, Scancode::Y],
                start: vec![Scancode::KpPlus, Scancode::Num1],
                select: vec![Scancode::KpMinus, Scancode::Num2],
            },
        }
    }
}

impl Keybinds {
    pub fn frame(&self, pressed: &dyn Fn(Scancode) -> bool) -> InputFrame {
        InputFrame {
            p1: self.p1.buttons(pressed),
            p2: self.p2.buttons(pressed),
        }
    }
}

/// Read the bindings from `path`, creating the default file when it is missing.
pub fn load(path: &Path) -> Keybinds {
    match std::fs::read_to_string(path) {
        Ok(text) => match parse(&text) {
            Ok(keybinds) => keybinds,
            Err(error) => {
                eprintln!(
                    "warning: {}: {error}; using default keyboard bindings",
                    path.display()
                );
                Keybinds::default()
            }
        },
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if let Err(error) = std::fs::write(path, DEFAULT_TEMPLATE) {
                eprintln!("warning: cannot create {}: {error}", path.display());
            }
            Keybinds::default()
        }
        Err(error) => {
            eprintln!(
                "warning: cannot read {}: {error}; using default keyboard bindings",
                path.display()
            );
            Keybinds::default()
        }
    }
}

pub fn parse(text: &str) -> Result<Keybinds, String> {
    let mut keybinds = Keybinds::default();
    let mut player: Option<u8> = None;

    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.split(['#', ';']).next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let name = header
                .strip_suffix(']')
                .ok_or_else(|| format!("line {line_no}: malformed section header `{line}`"))?;
            player = Some(match name.trim().to_ascii_lowercase().as_str() {
                "player1" => 1,
                "player2" => 2,
                other => {
                    return Err(format!(
                    "line {line_no}: unknown section `[{other}]` (expected [player1] or [player2])"
                ))
                }
            });
            continue;
        }

        let player =
            player.ok_or_else(|| format!("line {line_no}: `{line}` is outside a section"))?;
        let (button, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {line_no}: expected `button = key[, key...]`"))?;
        let button = button.trim().to_ascii_lowercase();
        let keys = parse_keys(value.trim()).map_err(|error| format!("line {line_no}: {error}"))?;
        let keys_for = match player {
            1 => keybinds.p1.button(&button),
            _ => keybinds.p2.button(&button),
        };
        *keys_for.ok_or_else(|| format!("line {line_no}: unknown button `{button}`"))? = keys;
    }

    check_duplicates(&keybinds)?;
    Ok(keybinds)
}

fn parse_keys(value: &str) -> Result<Vec<Scancode>, String> {
    if value.is_empty() {
        return Err("missing key name".to_string());
    }
    value
        .split(',')
        .map(|part| {
            let name = part.trim();
            let code = parse_key(name).ok_or_else(|| format!("unknown key `{name}`"))?;
            if RESERVED.contains(&code) {
                return Err(format!(
                    "`{name}` is a reserved hotkey (Pause, F11, F12, Esc, M, -, +)"
                ));
            }
            Ok(code)
        })
        .collect()
}

fn check_duplicates(keybinds: &Keybinds) -> Result<(), String> {
    let mut seen: Vec<(Scancode, &'static str, &'static str)> = Vec::new();
    for (player, keys) in [("player1", &keybinds.p1), ("player2", &keybinds.p2)] {
        for (button, codes) in keys.entries() {
            for &code in codes {
                if let Some((_, other_player, other_button)) =
                    seen.iter().find(|(bound, _, _)| *bound == code)
                {
                    return Err(format!(
                        "duplicate binding: {code:?} is used by {other_player} {other_button} and {player} {button}"
                    ));
                }
                seen.push((code, player, button));
            }
        }
    }
    Ok(())
}

fn parse_key(name: &str) -> Option<Scancode> {
    Some(match name.to_ascii_lowercase().as_str() {
        "up" => Scancode::Up,
        "down" => Scancode::Down,
        "left" => Scancode::Left,
        "right" => Scancode::Right,
        "space" => Scancode::Space,
        "return" | "enter" => Scancode::Return,
        "return2" | "kpenter" | "keypadenter" => Scancode::Return2,
        "backspace" | "bksp" => Scancode::Backspace,
        "tab" => Scancode::Tab,
        "delete" | "del" => Scancode::Delete,
        "kp0" => Scancode::Kp0,
        "kp1" => Scancode::Kp1,
        "kp2" => Scancode::Kp2,
        "kp3" => Scancode::Kp3,
        "kp4" => Scancode::Kp4,
        "kp5" => Scancode::Kp5,
        "kp6" => Scancode::Kp6,
        "kp7" => Scancode::Kp7,
        "kp8" => Scancode::Kp8,
        "kp9" => Scancode::Kp9,
        "kpperiod" | "kp." => Scancode::KpPeriod,
        "kpplus" | "kp+" => Scancode::KpPlus,
        "kpminus" | "kp-" => Scancode::KpMinus,
        "num0" => Scancode::Num0,
        "num1" => Scancode::Num1,
        "num2" => Scancode::Num2,
        "num3" => Scancode::Num3,
        "num4" => Scancode::Num4,
        "num5" => Scancode::Num5,
        "num6" => Scancode::Num6,
        "num7" => Scancode::Num7,
        "num8" => Scancode::Num8,
        "num9" => Scancode::Num9,
        "pause" => Scancode::Pause,
        "escape" | "esc" => Scancode::Escape,
        "f11" => Scancode::F11,
        "f12" => Scancode::F12,
        other => return letter_or_digit(other),
    })
}

fn letter_or_digit(name: &str) -> Option<Scancode> {
    let mut chars = name.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(match first {
        'a' => Scancode::A,
        'b' => Scancode::B,
        'c' => Scancode::C,
        'd' => Scancode::D,
        'e' => Scancode::E,
        'f' => Scancode::F,
        'g' => Scancode::G,
        'h' => Scancode::H,
        'i' => Scancode::I,
        'j' => Scancode::J,
        'k' => Scancode::K,
        'l' => Scancode::L,
        'm' => Scancode::M,
        'n' => Scancode::N,
        'o' => Scancode::O,
        'p' => Scancode::P,
        'q' => Scancode::Q,
        'r' => Scancode::R,
        's' => Scancode::S,
        't' => Scancode::T,
        'u' => Scancode::U,
        'v' => Scancode::V,
        'w' => Scancode::W,
        'x' => Scancode::X,
        'y' => Scancode::Y,
        'z' => Scancode::Z,
        '0' => Scancode::Num0,
        '1' => Scancode::Num1,
        '2' => Scancode::Num2,
        '3' => Scancode::Num3,
        '4' => Scancode::Num4,
        '5' => Scancode::Num5,
        '6' => Scancode::Num6,
        '7' => Scancode::Num7,
        '8' => Scancode::Num8,
        '9' => Scancode::Num9,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn frame_for(keybinds: &Keybinds, code: Scancode) -> InputFrame {
        keybinds.frame(&|pressed| pressed == code)
    }

    fn temp_path(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("gradius-keybinds-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("create temp directory");
        directory.join(FILE_NAME)
    }

    #[test]
    fn default_template_round_trips() {
        assert_eq!(
            parse(DEFAULT_TEMPLATE).expect("parse defaults"),
            Keybinds::default()
        );
    }

    #[test]
    fn remaps_player_one_button() {
        let keybinds = parse("[player1]\nfire = Q\n").expect("parse remap");
        assert!(frame_for(&keybinds, Scancode::Q).p1.fire);
        assert!(!frame_for(&keybinds, Scancode::Z).p1.fire);
        // Buttons the file does not mention keep their defaults.
        assert!(frame_for(&keybinds, Scancode::Up).p1.up);
    }

    #[test]
    fn rejects_duplicate_bindings_across_players() {
        let error = parse("[player1]\nfire = Q\n[player2]\npower = q\n").unwrap_err();
        assert!(error.contains("duplicate binding"), "{error}");
        assert!(
            error.contains("player1 fire") && error.contains("player2 power"),
            "{error}"
        );
    }

    #[test]
    fn rejects_reserved_hotkeys() {
        assert!(parse("[player1]\nfire = Esc\n").is_err());
        assert!(parse("[player1]\nfire = F11\n").is_err());
        assert!(parse("[player1]\nfire = Pause\n").is_err());
        assert!(parse("[player1]\nfire = M\n").is_err());
    }

    #[test]
    fn player_two_numpad_and_fallback_aliases_still_work() {
        let keybinds = Keybinds::default();
        let p2 = |code| frame_for(&keybinds, code).p2;
        assert!(p2(Scancode::Kp8).up && p2(Scancode::T).up);
        assert!(p2(Scancode::Kp5).down && p2(Scancode::G).down);
        assert!(p2(Scancode::Kp4).left && p2(Scancode::F).left);
        assert!(p2(Scancode::Kp6).right && p2(Scancode::H).right);
        assert!(p2(Scancode::Kp0).fire && p2(Scancode::R).fire);
        assert!(p2(Scancode::KpPeriod).power && p2(Scancode::Y).power);
        assert!(p2(Scancode::KpPlus).start && p2(Scancode::Num1).start);
        assert!(p2(Scancode::KpMinus).select && p2(Scancode::Num2).select);
    }

    #[test]
    fn missing_file_creates_defaults() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        let keybinds = load(&path);
        assert_eq!(keybinds, Keybinds::default());
        let written = std::fs::read_to_string(&path).expect("created file");
        let _ = std::fs::remove_file(&path);
        assert_eq!(parse(&written).expect("written defaults parse"), keybinds);
    }

    #[test]
    fn parse_errors_fall_back_to_defaults() {
        let path = temp_path("broken");
        std::fs::write(&path, "[player1]\nfire = NotAKey\n").expect("write broken file");
        let keybinds = load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(keybinds, Keybinds::default());
    }
}
