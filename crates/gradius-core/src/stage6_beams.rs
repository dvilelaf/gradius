//! `$C6D8`: the ten beams stage 6's `$82` window spawns.
//!
//! `$A2EA` routes every `$82` frame to `$C40D`, which runs on `$02 & 3 == 0`
//! and dispatches `$C433[5] = $C6D8`. The beams are *not* `$030C` objects: they
//! live in the ten collision slots at object slots 22-31 (`$0136-$013F`), so
//! `$0136` is also those slots' `$0120` sprite group. `$8B15` therefore draws
//! them as sprite group `$8D`, a single `$BD` red ball (`docs/analysis/ending.md`
//! §"Stage 6: the `$C6D8` beam wall").
//!
//! `$BC13` moves the same slots through the shared `$0136` proxy movement:
//! X -4 px/frame and Y by a 8.8 velocity whose sign is `$0476` (`$C6D8` lane)
//! and whose value comes from `$C449`'s nibble at spawn.
//!
//! The card that owns `game.rs` owns the `Beams` field and the frame call
//! sites (`$C40D` inside the `$82` window, the shot/player collision passes);
//! this module is the state and the rules.
/// `$C6D8`: ten collision slots, scanned `$09` down to `$00`.
pub(super) const SLOTS: usize = 10;
/// `$0376`: spawn X.
pub(super) const SPAWN_X: i16 = 152;
/// `$C74A`: lane Y. Index 0 (`$84`, 132) moves down to 0; index 1 (`$42`,
/// 66) moves up. `$02 & 4` selects index 0 when set.
pub(super) const LANE_Y: [i16; 2] = [0x84, 0x42];
/// `$8468`: shooting a beam awards 10 points and plays SFX `$09`.
pub(super) const SHOT_SCORE: u32 = 10;
/// `$C1FC[1]`/`$C200[1]`: the player-vs-beam hit box is 22 x 18, measured
/// from the ship's `(+6, +8)` probe point.
const PLAYER_BOX_WIDTH: u8 = 0x16;
const PLAYER_BOX_HEIGHT: u8 = 0x12;
const PLAYER_PROBE_X: i16 = 6;
const PLAYER_PROBE_Y: i16 = 8;
/// `$8B15`: `$0136 = $8D` is the beams' `$0120` sprite group, a single `$BD`
/// ball at the box position (`$8E91[($8D << 1)]` -> `$9454`).
pub(super) const SPRITE: u8 = 0x8D;
/// `$BF6F`: the vertical band is `shot_y - beam_y < $10`, and the horizontal
/// band is `shot_x - beam_x < $BFCC[kind]` (the shot's own width).
const SHOT_HEIGHT: u8 = 0x10;
/// `$BE24`/`$BE5C`: the ordinary expiry bounds.
const MIN_X: i16 = 0x02;
const MAX_X: i16 = 0xFC;
const MIN_Y: i16 = 0x08;
const MAX_Y: i16 = 0xC4;

/// One `$0136` collision slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Beam {
    pub(super) active: bool,
    /// `$0376`.
    pub(super) x: i16,
    /// `$0336`.
    pub(super) y: i16,
    /// `$0476`: 0 = lane `$84` (Y decreases), 1 = lane `$42` (Y increases).
    pub(super) lane: u8,
    /// `$03C6:$03F6`: the 8.8 Y velocity from `$C6D8`/`$C449`.
    pub(super) y_speed: u16,
    /// `$0396`: the Y sub-pixel accumulator. `$C6D8` clears it at spawn.
    pub(super) y_frac: u8,
}

/// `$0136`: the ten collision-only boxes.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Beams {
    pub(super) slots: [Beam; SLOTS],
}

/// `$C6D8`: the 16-bit 8.8 Y velocity from the `$C449` nibble and `$02`.
///
/// `$A9` is the nibble doubled; four `ASL`s leave the low byte, the two
/// `ROL`s collect the high part (`$98`), and the final `$02 + $A9` sets the
/// low byte and its carry.
fn y_velocity(nibble: u8, nmi: u8) -> u16 {
    let doubled = nibble << 1;
    let high = doubled >> 3;
    let low = doubled << 5;
    let sum = u16::from(nmi) + u16::from(low);
    (u16::from(high + (sum >> 8) as u8) << 8) | u16::from(sum as u8)
}

impl Beams {
    /// `$C6D8`: one spawn attempt on `$C40D`'s cadence (`$02 & 3 == 0`). The
    /// first free slot is the highest (`$09` down to `$00`); when all ten are
    /// busy the attempt is dropped, not queued. `$C449` has already advanced
    /// `$69` before this check, so the nibble is passed in.
    pub(super) fn spawn_attempt(&mut self, nmi: u8, nibble: u8) {
        if nmi & 3 != 0 {
            return;
        }
        let Some(slot) = (0..SLOTS).rev().find(|&slot| !self.slots[slot].active) else {
            return;
        };
        let lane = u8::from(nmi & 4 == 0);
        self.slots[slot] = Beam {
            active: true,
            x: SPAWN_X,
            y: LANE_Y[usize::from(lane)],
            lane,
            y_speed: y_velocity(nibble, nmi),
            y_frac: 0,
        };
    }

    /// `$BC13`/`$BDCF`: move every live box. X is exactly -4 px/frame; Y uses
    /// the 8.8 velocity and lane. A box that leaves the playfield expires.
    pub(super) fn step(&mut self) {
        for beam in &mut self.slots {
            if !beam.active {
                continue;
            }
            beam.x -= 4;
            if beam.x < MIN_X || beam.x >= MAX_X {
                *beam = Beam::default();
                continue;
            }
            let low = beam.y_speed as u8;
            let high = (beam.y_speed >> 8) as u8;
            if beam.lane == 0 {
                let (fraction, borrow) = beam.y_frac.overflowing_sub(low);
                beam.y_frac = fraction;
                beam.y -= i16::from(high) + i16::from(borrow);
            } else {
                let (fraction, carry) = beam.y_frac.overflowing_add(low);
                beam.y_frac = fraction;
                beam.y += i16::from(high) + i16::from(carry);
            }
            if beam.y < MIN_Y || beam.y >= MAX_Y {
                *beam = Beam::default();
            }
        }
    }

    /// `$BF6F`: a player shot whose probe point is inside a beam box destroys
    /// it; [`SHOT_SCORE`] is the `$8468` award per box that frame. The
    /// cartridge uses unsigned byte subtraction, so the accepted band starts
    /// at the box and runs right/down, not symmetrically around it. The shot
    /// loop keeps scanning the remaining boxes after the shot is consumed.
    pub(super) fn hit_by_shot(&mut self, x: i16, y: i16, width: u8) -> Option<u32> {
        let mut hits = 0;
        for beam in self.slots.iter_mut().rev() {
            if beam.active
                && (y as u8).wrapping_sub(beam.y as u8) < SHOT_HEIGHT
                // `$BF86`: the Y `CMP` leaves carry clear, so the X `SBC`
                // subtracts one more and the band is `[box + 1, box + width]`.
                && (x as u8)
                    .wrapping_sub(beam.x as u8)
                    .wrapping_sub(1)
                    < width
            {
                *beam = Beam::default();
                hits += 1;
            }
        }
        (hits > 0).then_some(hits * SHOT_SCORE)
    }

    /// `$C222`: the ship's `(+6, +8)` probe against the 22 x 18 box.
    pub(super) fn hits_player(&self, x: i16, y: i16) -> bool {
        let x = (x + PLAYER_PROBE_X) as u8;
        let y = (y + PLAYER_PROBE_Y) as u8;
        self.slots.iter().any(|beam| {
            beam.active
                && x.wrapping_sub(beam.x as u8) < PLAYER_BOX_WIDTH
                // `$C239`: the X `CMP` leaves carry clear, so the Y `SBC`
                // subtracts one more and the band is `[box + 1, box + height]`.
                && y.wrapping_sub(beam.y as u8).wrapping_sub(1) < PLAYER_BOX_HEIGHT
        })
    }

    /// `$C248`/`$AEF2`: a shield absorbs the hit and the box is cleared.
    pub(super) fn hit_player(&mut self, x: i16, y: i16) -> bool {
        let x = (x + PLAYER_PROBE_X) as u8;
        let y = (y + PLAYER_PROBE_Y) as u8;
        let Some(beam) = self.slots.iter_mut().rev().find(|beam| {
            beam.active
                && x.wrapping_sub(beam.x as u8) < PLAYER_BOX_WIDTH
                && y.wrapping_sub(beam.y as u8).wrapping_sub(1) < PLAYER_BOX_HEIGHT
        }) else {
            return false;
        };
        *beam = Beam::default();
        true
    }

    /// Live boxes, for the harness and the cap.
    #[cfg(test)]
    pub(super) fn live(&self) -> usize {
        self.slots.iter().filter(|beam| beam.active).count()
    }

    /// `$99D2` clears the collision slots; the `$82` window starts empty.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    #[cfg(test)]
    fn slot(&self, slot: usize) -> Beam {
        self.slots[slot]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_attempt_every_fourth_nmi_frame() {
        let mut beams = Beams::default();
        for nmi in 0..24u8 {
            let before = beams.live();
            beams.spawn_attempt(nmi, 0);
            if nmi & 3 == 0 {
                assert_eq!(beams.live(), before + 1, "attempt on $02 = {nmi}");
            } else {
                assert_eq!(beams.live(), before, "no attempt on $02 = {nmi}");
            }
            beams.step();
        }
    }

    #[test]
    fn the_first_slot_is_nine_and_the_lane_alternates() {
        let mut beams = Beams::default();
        // `$02 = 0` (`$02 & 4` clear) picks table index 1: lane 1, `$42`, 152.
        beams.spawn_attempt(0, 5);
        assert_eq!(
            beams.slot(9),
            Beam {
                active: true,
                x: SPAWN_X,
                y: LANE_Y[1],
                lane: 1,
                y_speed: 0x0140,
                y_frac: 0,
            }
        );
        // `$02 = 4` (`$02 & 4` set) picks index 0: lane 0, `$84`.
        beams.spawn_attempt(4, 8);
        assert_eq!(beams.slot(8).lane, 0);
        assert_eq!(beams.slot(8).y, LANE_Y[0]);
        beams.step();
        // Spawn then one step: lane 1 at `$42` moves down, lane 0 at `$84` up.
        assert_eq!(beams.slot(9).x, 148);
        assert!(beams.slot(9).y > LANE_Y[1]);
        assert!(beams.slot(8).y < LANE_Y[0]);
    }

    #[test]
    fn the_cap_is_ten_live_boxes() {
        let mut beams = Beams::default();
        for _ in 0..40 {
            beams.spawn_attempt(0, 0);
            assert!(beams.live() <= SLOTS, "more than ten beams live");
        }
        assert_eq!(beams.live(), SLOTS);
    }

    #[test]
    fn a_shot_destroys_a_beam_and_awards_ten() {
        let mut beams = Beams::default();
        beams.spawn_attempt(0, 5);
        let (bx, by) = (SPAWN_X, LANE_Y[1]);
        // `$BF6F`: only the right/down band `[box, box + width)` hits.
        assert_eq!(beams.hit_by_shot(bx + 15, by + 15, 16), Some(SHOT_SCORE));
        assert_eq!(beams.live(), 0);
        assert_eq!(beams.hit_by_shot(bx + 1, by, 16), None, "already gone");

        beams.spawn_attempt(0, 5);
        assert_eq!(beams.hit_by_shot(bx, by, 16), None, "left of the box");
        assert_eq!(beams.hit_by_shot(bx + 17, by, 16), None, "past the width");
        assert_eq!(beams.hit_by_shot(bx, by - 1, 16), None, "above the box");
        assert_eq!(beams.hit_by_shot(bx, by + 16, 16), None, "past the height");
        assert_eq!(beams.hit_by_shot(bx + 9, by, 8), None, "narrow shot");
        assert_eq!(beams.hit_by_shot(bx + 8, by, 8), Some(SHOT_SCORE));
    }

    #[test]
    fn two_overlapping_boxes_are_both_destroyed_by_one_shot() {
        let mut beams = Beams::default();
        beams.spawn_attempt(0, 5);
        beams.spawn_attempt(0, 5);
        beams.slots[8].x = SPAWN_X;
        beams.slots[8].y = LANE_Y[1];
        assert_eq!(
            beams.hit_by_shot(SPAWN_X + 1, LANE_Y[1], 16),
            Some(2 * SHOT_SCORE)
        );
        assert_eq!(beams.live(), 0);
    }

    #[test]
    fn the_player_probe_is_the_ship_plus_six_and_eight() {
        let mut beams = Beams::default();
        beams.spawn_attempt(0, 5);
        let (bx, by) = (SPAWN_X, LANE_Y[1]);
        let (sx, sy) = (bx - PLAYER_PROBE_X, by - PLAYER_PROBE_Y + 1);
        assert!(beams.hits_player(sx, sy));
        assert!(!beams.hits_player(sx - 1, sy), "left of the box");
        assert!(!beams.hits_player(sx, sy - 1), "above the box");
        assert!(!beams.hits_player(sx + i16::from(PLAYER_BOX_WIDTH), sy));
        assert!(!beams.hits_player(sx, sy + i16::from(PLAYER_BOX_HEIGHT)));
    }

    #[test]
    fn a_shield_hit_clears_the_box() {
        let mut beams = Beams::default();
        beams.spawn_attempt(0, 5);
        let (bx, by) = (SPAWN_X, LANE_Y[1]);
        let (sx, sy) = (bx - PLAYER_PROBE_X, by - PLAYER_PROBE_Y + 1);
        assert!(beams.hit_player(sx, sy));
        assert_eq!(beams.live(), 0);
        assert!(!beams.hit_player(sx, sy), "already gone");
    }

    #[test]
    fn boxes_expire_at_the_playfield_edges() {
        let mut beams = Beams::default();
        beams.spawn_attempt(0, 5);
        for _ in 0..(SPAWN_X / 4 + 1) {
            beams.step();
        }
        assert_eq!(beams.live(), 0, "the box left the playfield");
    }
}
