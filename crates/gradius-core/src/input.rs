//! Input state, independent of any device API.

/// The two NES buttons plus the d-pad, as a plain value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons {
    pub fire: bool,
    pub power: bool,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub start: bool,
    pub select: bool,
}

impl Buttons {
    pub const NONE: Self = Self {
        fire: false,
        power: false,
        up: false,
        down: false,
        left: false,
        right: false,
        start: false,
        select: false,
    };

    /// Translate the NES controller bit order used by the oracle.
    pub fn from_nes_bits(bits: u8) -> Self {
        Self {
            fire: bits & 0x01 != 0,
            power: bits & 0x02 != 0,
            select: bits & 0x04 != 0,
            start: bits & 0x08 != 0,
            up: bits & 0x10 != 0,
            down: bits & 0x20 != 0,
            left: bits & 0x40 != 0,
            right: bits & 0x80 != 0,
        }
    }

    pub fn axis_x(self) -> i32 {
        (self.right as i32) - (self.left as i32)
    }

    pub fn axis_y(self) -> i32 {
        (self.down as i32) - (self.up as i32)
    }
}
