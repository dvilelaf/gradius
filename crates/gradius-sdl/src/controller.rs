//! Game controller support.
//!
//! SDL's game-controller layer normalises the physical pad, so one mapping
//! covers Xbox, DualShock, Switch Pro and most 8BitDo pads: d-pad or left
//! stick to move, A to fire, B to power, Start and Back to the NES buttons.
//! Hot-plug is driven by the frontend's event loop.

use gradius_app::InputFrame;
use gradius_core::Buttons;
use sdl2::controller::{Axis, Button, GameController};
use sdl2::{GameControllerSubsystem, JoystickSubsystem};

/// Xbox 360 layout for joysticks SDL has no mapping for (evdev button and axis
/// order of the kernel's xpad-style devices).
const GENERIC_MAPPING: &str = "a:b0,b:b1,x:b2,y:b3,leftshoulder:b4,rightshoulder:b5,back:b6,start:b7,guide:b8,leftstick:b9,rightstick:b10,leftx:a0,lefty:a1,lefttrigger:a2,rightx:a3,righty:a4,righttrigger:a5,dpup:h0.1,dpright:h0.2,dpdown:h0.4,dpleft:h0.8";

/// Ignore small stick drift. SDL axes span -32768..=32767.
const DEAD_ZONE: i16 = 12_000;

struct OpenPad {
    /// Joystick index, used to reject duplicate `ControllerDeviceAdded`.
    index: u32,
    controller: GameController,
}

pub struct Controllers {
    subsystem: GameControllerSubsystem,
    joysticks: Option<JoystickSubsystem>,
    open: Vec<OpenPad>,
}

impl Controllers {
    /// `None` means SDL has no game-controller subsystem at all; the keyboard
    /// still works.
    pub fn new(
        subsystem: Option<GameControllerSubsystem>,
        joysticks: Option<JoystickSubsystem>,
    ) -> Option<Self> {
        let subsystem = subsystem?;
        let mut controllers = Self {
            subsystem,
            joysticks,
            open: Vec::new(),
        };
        controllers.open_existing();
        Some(controllers)
    }

    fn open_existing(&mut self) {
        let count = self.subsystem.num_joysticks().unwrap_or(0);
        for index in 0..count {
            self.joy_added(index);
        }
    }

    /// A joystick SDL has no game-controller mapping for gets the generic
    /// Xbox 360 one, so any pad the kernel exposes as a joystick works.
    pub fn joy_added(&mut self, index: u32) {
        if !self.subsystem.is_game_controller(index) {
            let Some(joysticks) = &self.joysticks else {
                return;
            };
            let (Ok(guid), Ok(name)) = (
                joysticks.device_guid(index),
                joysticks.name_for_index(index),
            ) else {
                return;
            };
            let name = name.replace(',', " ");
            let mapping = format!("{},{name},{GENERIC_MAPPING}", guid.string());
            if let Err(error) = self.subsystem.add_mapping(&mapping) {
                eprintln!("warning: could not map joystick {index}: {error}");
                return;
            }
        }
        if self.subsystem.is_game_controller(index) {
            self.open_index(index);
        }
    }

    fn open_index(&mut self, index: u32) {
        if self.open.iter().any(|pad| pad.index == index) {
            return;
        }
        match self.subsystem.open(index) {
            Ok(controller) => {
                println!("controller: {}", controller.name());
                self.open.push(OpenPad { index, controller });
            }
            Err(error) => eprintln!("warning: could not open controller {index}: {error}"),
        }
    }

    /// `which` is the joystick index carried by `ControllerDeviceAdded`.
    pub fn added(&mut self, which: u32) {
        self.open_index(which);
    }

    /// `which` is the joystick instance id carried by `ControllerDeviceRemoved`.
    pub fn removed(&mut self, which: u32) {
        self.open
            .retain(|pad| pad.controller.instance_id() != which);
    }

    /// `simultaneous` is [`gradius_core::Game::simultaneous`]: only then does
    /// each pad keep its own ship.
    pub fn buttons(&self, simultaneous: bool) -> InputFrame {
        assign(
            self.open.iter().map(|pad| pad_buttons(&pad.controller)),
            simultaneous,
        )
    }
}

/// In simultaneous 2P, controller #1 drives P1 and #2 drives P2 and further
/// pads are ignored. Otherwise every pad drives P1 (and #2 still feeds P2's
/// turn): one physical pad can show up as two SDL devices (an XInput pad with
/// a second HID interface), and the silent one must not take P1.
fn assign(pads: impl IntoIterator<Item = Buttons>, simultaneous: bool) -> InputFrame {
    let pads: Vec<Buttons> = pads.into_iter().collect();
    let second = pads.get(1).copied().unwrap_or(Buttons::NONE);
    let p1 = if simultaneous {
        pads.first().copied().unwrap_or(Buttons::NONE)
    } else {
        pads.iter().fold(Buttons::NONE, |all, pad| union(all, *pad))
    };
    InputFrame { p1, p2: second }
}

fn pad_buttons(controller: &GameController) -> Buttons {
    let pressed = |button: Button| controller.button(button);
    let axis_x = controller.axis(Axis::LeftX);
    let axis_y = controller.axis(Axis::LeftY);
    Buttons {
        fire: pressed(Button::A),
        power: pressed(Button::B),
        up: pressed(Button::DPadUp) || axis_y < -DEAD_ZONE,
        down: pressed(Button::DPadDown) || axis_y > DEAD_ZONE,
        left: pressed(Button::DPadLeft) || axis_x < -DEAD_ZONE,
        right: pressed(Button::DPadRight) || axis_x > DEAD_ZONE,
        start: pressed(Button::Start),
        select: pressed(Button::Back),
    }
}

/// Combine keyboard and pad state per player. `Buttons` only has public
/// fields, so this stays in the frontend and the core keeps its device-free
/// API.
pub fn union_frame(keyboard: InputFrame, pads: InputFrame) -> InputFrame {
    InputFrame {
        p1: union(keyboard.p1, pads.p1),
        p2: union(keyboard.p2, pads.p2),
    }
}

/// Combine keyboard and pad state. `Buttons` only has public fields, so this
/// stays in the frontend and the core keeps its device-free API.
pub fn union(a: Buttons, b: Buttons) -> Buttons {
    Buttons {
        fire: a.fire || b.fire,
        power: a.power || b.power,
        up: a.up || b.up,
        down: a.down || b.down,
        left: a.left || b.left,
        right: a.right || b.right,
        start: a.start || b.start,
        select: a.select || b.select,
    }
}

#[cfg(test)]
mod tests {
    use super::{assign, union, union_frame};
    use gradius_app::InputFrame;
    use gradius_core::Buttons;

    #[test]
    fn union_combines_both_sources() {
        let keyboard = Buttons {
            left: true,
            fire: true,
            ..Buttons::NONE
        };
        let pad = Buttons {
            right: true,
            fire: true,
            start: true,
            ..Buttons::NONE
        };
        let combined = union(keyboard, pad);
        assert!(combined.left && combined.right && combined.fire && combined.start);
        assert!(!combined.power && !combined.select && !combined.up && !combined.down);
    }

    #[test]
    fn controller_one_drives_p1_and_controller_two_drives_p2() {
        let pad1 = Buttons {
            left: true,
            fire: true,
            ..Buttons::NONE
        };
        let pad2 = Buttons {
            right: true,
            start: true,
            ..Buttons::NONE
        };
        let frame = assign([pad1, pad2], true);
        assert_eq!(frame.p1, pad1);
        assert_eq!(frame.p2, pad2);

        // A third pad is ignored, not merged into either player.
        assert_eq!(
            assign([pad1, pad2, pad1], true),
            InputFrame { p1: pad1, p2: pad2 }
        );
        assert_eq!(
            assign([pad1], true),
            InputFrame {
                p1: pad1,
                p2: Buttons::NONE
            }
        );
        assert_eq!(assign([], true), InputFrame::NONE);
    }

    #[test]
    fn outside_simultaneous_2p_every_pad_drives_p1() {
        let silent = Buttons::NONE;
        let live = Buttons {
            right: true,
            start: true,
            ..Buttons::NONE
        };
        // The same physical pad seen twice, the silent device first.
        let frame = assign([silent, live], false);
        assert_eq!(frame.p1, live);
        assert_eq!(frame.p2, live);
        assert_eq!(assign([live], false).p1, live);
        assert_eq!(assign([], false), InputFrame::NONE);
    }

    #[test]
    fn union_frame_keeps_the_two_players_separate() {
        let keyboard = InputFrame {
            p1: Buttons {
                left: true,
                ..Buttons::NONE
            },
            p2: Buttons {
                start: true,
                ..Buttons::NONE
            },
        };
        let pads = InputFrame {
            p1: Buttons {
                fire: true,
                ..Buttons::NONE
            },
            p2: Buttons {
                right: true,
                ..Buttons::NONE
            },
        };
        let combined = union_frame(keyboard, pads);
        assert!(combined.p1.left && combined.p1.fire);
        assert!(!combined.p1.right && !combined.p1.start);
        assert!(combined.p2.start && combined.p2.right);
        assert!(!combined.p2.left && !combined.p2.fire);
    }
}
