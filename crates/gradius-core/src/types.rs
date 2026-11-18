//! Small value types shared by the simulation.
//!
//! Moved verbatim from `game.rs` (card R1m): the fixed-point position, the BCD
//! score, the starfield RNG and the entity records keep their bodies and
//! names; the only edits are the `pub(super)` markers the module boundary
//! needs.

use super::{Rect, CANNON_PARTS};
use crate::stage1_tables as tables;

/// 16.8 fixed point, mirroring `$0320/$0340` (or `$0360/$0380`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Fixed {
    pub(super) int: u8,
    pub(super) frac: u8,
}

impl Fixed {
    pub(super) fn px(int: u8) -> Self {
        Self { int, frac: 0 }
    }

    /// Add an 8.8 delta (1/256 px) with a carry into the integer byte.
    pub(super) fn add(&mut self, delta: i32) {
        let value = (i32::from(self.int) << 8) + i32::from(self.frac) + delta;
        let value = value.clamp(0, 0xFFFF);
        self.int = (value >> 8) as u8;
        self.frac = value as u8;
    }

    /// Subtract an 8.8 delta with a borrow from the integer byte.
    pub(super) fn sub(&mut self, delta: i32) {
        self.add(-delta);
    }
}

/// Six-digit binary-coded decimal score, `$07E4-$07E6`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Bcd {
    /// Most significant byte first.
    pub(super) digits: [u8; 3],
}

impl Bcd {
    pub(super) fn value(self) -> u32 {
        let mut value = 0;
        for byte in self.digits {
            value = value * 100 + u32::from((byte >> 4) & 0x0F) * 10 + u32::from(byte & 0x0F);
        }
        value
    }

    /// Six digits from a decimal value, most significant byte first.
    pub(super) fn from_decimal(mut value: u32) -> Self {
        value = value.min(999_999);
        let mut digits = [0u8; 3];
        for digit in digits.iter_mut().rev() {
            let pair = (value % 100) as u8;
            *digit = ((pair / 10) << 4) | (pair % 10);
            value /= 100;
        }
        Bcd { digits }
    }

    /// `$84AE`: add a decimal value digit-pair by digit-pair, carrying at
    /// `$9A`, and saturate at 999999 (`$84A1` fills `99` on overflow).
    pub(super) fn add(&mut self, amount: u32) {
        let bcd = |value: u32| -> u8 {
            let pair = (value % 100) as u8;
            ((pair / 10) << 4) | (pair % 10)
        };
        let addend = [bcd(amount / 10_000), bcd(amount / 100), bcd(amount)];
        let mut carry = 0u16;
        for index in (0..3).rev() {
            let low =
                u16::from(self.digits[index] & 0x0F) + u16::from(addend[index] & 0x0F) + carry;
            let mut high = u16::from(self.digits[index] >> 4) + u16::from(addend[index] >> 4);
            if low >= 10 {
                high += 1;
            }
            let low_digit = (low % 10) as u8;
            carry = high / 10;
            self.digits[index] = (((high % 10) as u8) << 4) | low_digit;
        }
        if carry > 0 {
            self.digits = [0x99, 0x99, 0x99];
        }
    }
}

/// `$8052`: cold boot writes `$50` into `$07E1`, so the high-score buffer
/// starts at BCD `005000`, displayed as `0050000`.
pub(super) const DEFAULT_HIGH_SCORE: Bcd = Bcd {
    digits: [0x00, 0x50, 0x00],
};

/// Deterministic xorshift32, used only for the starfield. Seeded per run,
/// never by wall-clock time.
#[derive(Clone, Debug)]
pub(super) struct Rng(pub(super) u32);

impl Rng {
    pub(super) fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    pub(super) fn range(&mut self, low: i32, high: i32) -> i32 {
        debug_assert!(high > low);
        low + (self.next_u32() % (high - low) as u32) as i32
    }
}

/// One `$0600` attached cannon record (`$A4A0` creates them, `$CB8B` runs
/// them). The parent field is `$0600`, the index `$0601` (the nibble minus
/// one), and the counter `$0604`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Cannon {
    pub(super) active: bool,
    pub(super) parent: u8,
    pub(super) index: u8,
    pub(super) timer: u8,
    /// `$0605`: hits taken against `$BEE4[rank]`.
    pub(super) hits: u8,
    /// `$0603`: decremented on every call; the parts are only placed on even
    /// values, so they move once per two `$CB8B` calls.
    pub(super) part_counter: u8,
    /// `$0610`/`$0611`..`$0615`: one sway phase per part. `$CC93` advances `X`
    /// with the part, so `$0611,X` walks the whole six-byte run; entry 0
    /// (`$0610`) is unused and stays zero. Parts 1..5 use entries 1..5.
    pub(super) phase: [u8; CANNON_PARTS],
    /// `$0618`/`$0620`: the six part positions, part 0 at the parent origin.
    pub(super) part_x: [i32; CANNON_PARTS],
    pub(super) part_y: [i32; CANNON_PARTS],
}

/// One enemy shot. The original keeps these in the `$0136` proxy array with
/// type `$0176` (0 = small, 1 = large) and an 8.8 velocity along the major
/// axis (`$BCAF`/`$BDF4`).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct EnemyBullet {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) vx: i32,
    pub(super) vy: i32,
    pub(super) kind: u8,
    /// `$0120` of the shot: `$3A` for the `$90BA` two-tile pair, `$25` for the
    /// stage-2 launcher's single red/white cross.
    pub(super) sprite: u8,
    /// `$CBCB`: which of the ten `$0136` proxies the shot occupies (`$A9`).
    /// The position bytes outlive the shot, so the slot decides the sub-pixel
    /// fraction the next shot inherits.
    pub(super) slot: u8,
}

impl EnemyBullet {
    /// `$C204`: the player box for this shot type, as `(x offset, y offset,
    /// width, height)` relative to `$0360`/`$0320`.
    pub(super) fn player_box(self) -> Rect {
        let kind = usize::from(self.kind).min(tables::PLAYER_BOX_WIDTH.len() - 1);
        (
            i32::from(tables::PLAYER_BOX_X[kind]),
            i32::from(tables::PLAYER_BOX_Y[kind]),
            i32::from(tables::PLAYER_BOX_WIDTH[kind]),
            i32::from(tables::PLAYER_BOX_HEIGHT[kind]),
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Explosion {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) life: u8,
    pub(super) max_life: u8,
}

/// `$C0C1`: the player-death effect, a fixed 10-frame-per-step OAM sequence
/// (`$C0F4 = 2D 2E 2F 30 30 00`) at the death position.
#[derive(Clone, Copy, Debug)]
pub(super) struct DeathBurst {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) age: u16,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Star {
    pub(super) x: i32,
    pub(super) y: i16,
    pub(super) layer: u8,
}
