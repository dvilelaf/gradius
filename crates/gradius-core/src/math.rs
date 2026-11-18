//! Integer-only fixed-point helpers shared by the simulation and the stage
//! script.
//!
//! Positions are stored as 1/256 of a pixel. Angles are quantised to sixteen
//! steps per turn, which is what the original's spawn and movement tables use.
//! Nothing here can introduce a platform-dependent rounding difference.

/// sin(2*pi*k/16) in 8.8 fixed point.
pub(crate) const SINE: [i16; 16] = [
    0, 98, 181, 237, 256, 237, 181, 98, 0, -98, -181, -237, -256, -237, -181, -98,
];

/// `sin` of a sixteenth-turn phase, in 8.8 fixed point.
#[inline]
pub(crate) fn sine(phase: u8) -> i32 {
    i32::from(SINE[usize::from(phase & 15)])
}

/// `cos` of a sixteenth-turn phase, in 8.8 fixed point.
#[inline]
pub(crate) fn cosine(phase: u8) -> i32 {
    sine(phase.wrapping_add(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_is_a_full_turn() {
        assert_eq!(sine(0), 0);
        assert_eq!(sine(4), 256);
        assert_eq!(sine(8), 0);
        assert_eq!(sine(12), -256);
        assert_eq!(cosine(0), 256);
        assert_eq!(cosine(4), 0);
    }
}
