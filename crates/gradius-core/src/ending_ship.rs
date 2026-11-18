//! The two sprites `$987F` starts for the ending, and the kind-`$28` walk
//! `$BB09` runs over the `$BB7C` motion table.
//!
//! `$987F` (the last `$8B` frame, oracle 7051) clears slots 8 and 9 and writes
//! two objects straight into the object arrays:
//!
//! | object | position (`$0334`/`$0374`) | sprite (`$0134`) |
//! | --- | --- | --- |
//! | slot 8 | `(116, 128)` | `$9E`, the 3x2 mothership group |
//! | slot 9 | `(164, 136)` | kind `$28`; `$0135` starts at 0 |
//!
//! `$BB09` advances `$014C` (slot 9) once per frame. When it reaches 6 it
//! consumes one `$BB7C` pair and clears the counter, so the walk steps every
//! six frames: `7058 + 6k` for the 26 measured entries. The pair is
//! `[x_delta, y_delta << 4 | tile_index]`: `x += delta` (signed byte),
//! `y -= high nibble`, `$0135 = $96 + low nibble`. The `$FF` entry clears
//! `$0135`, so the ship leaves the screen at 7214 with the last position.
//!
//! The `$FF` entry also starts the slot-8 explosion (`$CB22`, kind 2). Its
//! sound-gated start (`$D4 != 0` skips it) landed on oracle 7377, and the
//! kind-2 handler then cycles six groups in five-frame blocks, 7378-7407:
//! `$A0 $68 $A2 $69 $6A $6B`, then the slot is cleared. Both sprite-tile
//! sequences were read back from the oracle OAM (frames 7060-7407).
//!
//! The `$BB7C` table is read from the pack (`Assets::ending_ship_motion`).

/// `$BB7C` walk length: the entry count before the `$FF` terminator.
pub const ENDING_SHIP_STEPS: usize = 26;
/// `$987F`: the walk starts at `(164, 136)`.
pub const SHIP_X: u8 = 0xA4;
/// `$987F`: `$0335`.
pub const SHIP_Y: u8 = 0x88;
/// `$987F`: the mothership at `$0374`/`$0334`.
pub const MOTHERSHIP_X: i32 = 0x74;
/// `$0334`.
pub const MOTHERSHIP_Y: i32 = 0x80;
/// The mothership sprite group (`$9E` -> table B `$1E`).
pub const MOTHERSHIP_SPRITE: u8 = 0x9E;

/// [`EndingShip::advance`] runs on the frames `$014C` reaches 6: the first
/// step is 6 frames into `$8C` (oracle 7058), then every 6 frames after it.
pub const FIRST_STEP_CLOCK: u16 = 6;
/// `$014C` frame period between two `$BB7C` entries.
pub const STEP_PERIOD: u16 = 6;
/// The `$FF` entry clears `$0135` on the oracle's 7214 (`$8C + 162`). The
/// gate below reads `Game::ending_clock` after the frame body has incremented
/// it (frame `F` shows `F - 7051`), so the entry is one past the state frame:
/// the last ship picture is 7215 and 7216 is the first blank one.
pub const HIDE_CLOCK: u16 = 163;

/// The `$BB09`/`$CB22` explosion: `$0134` becomes `$A0` on the oracle's 7378,
/// its first picture is 7380 (`explosion_sprite` also reads the incremented
/// clock). Six groups of five frames: pictures 7380-7409.
pub const EXPLOSION_CLOCK: u16 = 7378 - 7052 + 1;
/// The kind-2 slot-8 sprite groups in order (`$0134` 7378-7407).
pub const EXPLOSION_SPRITES: [u8; 6] = [0xA0, 0x68, 0xA2, 0x69, 0x6A, 0x6B];
/// Frames each explosion group stays on screen.
pub const EXPLOSION_HOLD: u16 = 5;

/// `$0335`/`$0375`/`$0135`/`$0475` for the kind-`$28` ending ship.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndingShip {
    /// `$0375`: X integer. Wraps, like the ROM's byte add.
    pub x: u8,
    /// `$0335`: Y integer.
    pub y: u8,
    /// `$0135`: sprite type; 0 hides the ship.
    pub sprite: u8,
    /// `$0475`: `$BB7C` entries consumed.
    pub step: u8,
}

impl EndingShip {
    /// `$987F`: the slot-9 initial state, sprite 0 until the first step.
    pub fn new() -> Self {
        Self {
            x: SHIP_X,
            y: SHIP_Y,
            sprite: 0,
            step: 0,
        }
    }

    /// One `$BB7C` entry: `x += delta`, `y -= high nibble`,
    /// `$0135 = $96 + low nibble`. The terminator keeps the last state.
    pub fn advance(&mut self, motion: &[[u8; 2]]) {
        let Some(&[dx, dy_tile]) = motion.get(usize::from(self.step)) else {
            return;
        };
        self.x = self.x.wrapping_add(dx);
        self.y = self.y.wrapping_sub(dy_tile >> 4);
        self.sprite = 0x96 + (dy_tile & 0x0F);
        self.step += 1;
    }
}

/// Whether the kind-`$28` ship still has an OAM entry at `clock` frames into
/// `$8C`. `$0135` is zero until the first step and again from 7214.
pub fn ship_visible(clock: u16) -> bool {
    (FIRST_STEP_CLOCK..HIDE_CLOCK).contains(&clock)
}

/// The kind-2 explosion's sprite group at `clock`, or `None` outside
/// 7378-7407. The slot-8 mothership shows until the explosion starts.
pub fn explosion_sprite(clock: u16) -> Option<u8> {
    let age = clock.checked_sub(EXPLOSION_CLOCK)?;
    if age >= EXPLOSION_HOLD * EXPLOSION_SPRITES.len() as u16 {
        return None;
    }
    Some(EXPLOSION_SPRITES[usize::from(age / EXPLOSION_HOLD)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The oracle's per-step samples at 7058 + 6k: (x, y, sprite).
    const MEASURED: [(u8, u8, u8); ENDING_SHIP_STEPS] = [
        (165, 136, 0x96),
        (166, 136, 0x96),
        (167, 136, 0x96),
        (168, 136, 0x96),
        (169, 136, 0x96),
        (170, 136, 0x96),
        (172, 136, 0x96),
        (174, 136, 0x96),
        (177, 135, 0x96),
        (180, 134, 0x96),
        (182, 131, 0x97),
        (182, 127, 0x97),
        (182, 123, 0x98),
        (180, 117, 0x98),
        (174, 111, 0x99),
        (166, 106, 0x99),
        (157, 102, 0x9A),
        (147, 97, 0x9A),
        (135, 92, 0x9B),
        (121, 87, 0x9B),
        (105, 82, 0x9C),
        (89, 78, 0x9C),
        (72, 73, 0x9C),
        (49, 69, 0x9D),
        (26, 65, 0x9D),
        (0, 62, 0x9D),
    ];

    #[test]
    fn the_bb7c_walk_matches_the_measured_frames() {
        let motion = crate::assets::pack_stage_assets()
            .ending_ship_motion
            .unwrap();
        let mut ship = EndingShip::new();
        assert_eq!((ship.x, ship.y, ship.sprite), (SHIP_X, SHIP_Y, 0));
        for (index, (x, y, sprite)) in MEASURED.iter().enumerate() {
            ship.advance(&motion);
            assert_eq!(
                (ship.x, ship.y, ship.sprite),
                (*x, *y, *sprite),
                "step {index}"
            );
            assert_eq!(usize::from(ship.step), index + 1);
        }
        // A 27th call is the `$FF` terminator: it leaves the last state.
        ship.advance(&motion);
        assert_eq!((ship.x, ship.y, ship.sprite), (0, 62, 0x9D));
        assert_eq!(ship.step, ENDING_SHIP_STEPS as u8);
    }

    #[test]
    fn the_ship_is_hidden_before_the_first_step_and_from_7214() {
        assert!(!ship_visible(0));
        assert!(!ship_visible(FIRST_STEP_CLOCK - 1));
        assert!(ship_visible(FIRST_STEP_CLOCK));
        assert!(ship_visible(HIDE_CLOCK - 1));
        assert!(!ship_visible(HIDE_CLOCK));
        assert!(!ship_visible(HIDE_CLOCK + 1));
        // The last step (clock 156, oracle 7208) is still on screen.
        assert_eq!(FIRST_STEP_CLOCK + STEP_PERIOD * 25, 156);
        assert!(ship_visible(156));
    }

    #[test]
    fn the_explosion_cycles_six_groups_of_five_frames() {
        assert_eq!(explosion_sprite(EXPLOSION_CLOCK - 1), None);
        for (index, sprite) in EXPLOSION_SPRITES.iter().enumerate() {
            for frame in 0..EXPLOSION_HOLD {
                let clock = EXPLOSION_CLOCK + index as u16 * EXPLOSION_HOLD + frame;
                assert_eq!(explosion_sprite(clock), Some(*sprite), "clock {clock}");
            }
        }
        assert_eq!(
            explosion_sprite(EXPLOSION_CLOCK + EXPLOSION_HOLD * 6),
            None,
            "the slot is cleared at 7408"
        );
        // The oracle's first explosion picture is 7380; the port's clock is
        // two frames behind it (`7378 - 7052 + 1`).
        assert_eq!(EXPLOSION_CLOCK, 327);
    }
}
