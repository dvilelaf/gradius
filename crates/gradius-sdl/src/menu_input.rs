//! Keyboard keys, pad buttons and the left stick as options-menu input.

use sdl2::controller::{Axis, Button};
use sdl2::keyboard::Scancode;

use crate::menu;

/// Keys that drive the open menu.
pub fn menu_key(scancode: Scancode) -> Option<menu::Input> {
    use menu::Input;
    Some(match scancode {
        Scancode::Up | Scancode::W => Input::Up,
        Scancode::Down | Scancode::S => Input::Down,
        Scancode::Left | Scancode::A => Input::Left,
        Scancode::Right | Scancode::D => Input::Right,
        Scancode::Return | Scancode::Return2 | Scancode::KpEnter | Scancode::Space => Input::Accept,
        Scancode::Escape | Scancode::Backspace => Input::Back,
        _ => return None,
    })
}

/// Pad buttons that drive the open menu: d-pad, A accepts, B and Guide go back.
pub fn menu_button(button: Button) -> Option<menu::Input> {
    use menu::Input;
    Some(match button {
        Button::DPadUp => Input::Up,
        Button::DPadDown => Input::Down,
        Button::DPadLeft => Input::Left,
        Button::DPadRight => Input::Right,
        Button::A | Button::Start => Input::Accept,
        Button::B | Button::Back | Button::Guide => Input::Back,
        _ => return None,
    })
}

/// The left stick as menu arrows: one step each time it leaves the centre.
#[derive(Default)]
pub struct MenuStick {
    x: i8,
    y: i8,
}

impl MenuStick {
    const THRESHOLD: i16 = 16_000;

    pub fn menu_input(&mut self, axis: Axis, value: i16) -> Option<menu::Input> {
        use menu::Input;
        let direction = if value > Self::THRESHOLD {
            1
        } else if value < -Self::THRESHOLD {
            -1
        } else {
            0
        };
        let (last, negative, positive) = match axis {
            Axis::LeftX => (&mut self.x, Input::Left, Input::Right),
            Axis::LeftY => (&mut self.y, Input::Up, Input::Down),
            _ => return None,
        };
        let changed = direction != *last;
        *last = direction;
        match (changed, direction) {
            (true, 1) => Some(positive),
            (true, -1) => Some(negative),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stick_steps_once_per_push() {
        let mut stick = MenuStick::default();
        assert_eq!(
            stick.menu_input(Axis::LeftY, 30_000),
            Some(menu::Input::Down)
        );
        assert_eq!(stick.menu_input(Axis::LeftY, 32_000), None);
        assert_eq!(stick.menu_input(Axis::LeftY, 0), None);
        assert_eq!(
            stick.menu_input(Axis::LeftY, -30_000),
            Some(menu::Input::Up)
        );
        assert_eq!(
            stick.menu_input(Axis::LeftX, 30_000),
            Some(menu::Input::Right)
        );
        assert_eq!(stick.menu_input(Axis::RightX, 30_000), None);
    }
}
