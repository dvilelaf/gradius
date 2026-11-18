//! The six-slot Gradius power meter, ported from `$8950`/`$8979`/`$898E`.
//!
//! * A collected capsule advances the selection (`$8950`: `INC $42`, wrapping
//!   back to 1 after 6). It is **not** a currency: activation never consumes a
//!   capsule in the original, and neither does this port.
//! * The B button (`$07` bit 6, held) dispatches on the selection (`$898E`):
//!
//!   | `$42` | handler | effect |
//!   | --- | --- | --- |
//!   | 0 | `$8988` | nothing |
//!   | 1 | `$89A6` | `INC $40` (speed) |
//!   | 2 | `$89B4` | `INC $41` (missile, once) |
//!   | 3 | `$89C0` | `$44 = 2` (double) |
//!   | 4 | `$89D4` | `$44 = 1` (laser) |
//!   | 5 | `$89D8` | `INC $45` (option, max 2) |
//!   | 6 | `$899C` | `$46 = 5` (shield, once) |
//!
//! Every successful handler clears `$42` to 0; a no-op leaves the selection
//! alone, so a maxed item can be selected again. The original fires on the
//! *held* button, which means the activation and its reset happen in the same
//! frame.

/// Number of selectable slots.
pub const SLOT_COUNT: usize = 6;
/// The cartridge's option cap: `$89DA` compares `$45` with `#$02`.
pub const MAX_OPTIONS: u8 = 2;
/// The cap when [`crate::mods::Mods::extra_options`] is enabled (the arcade's
/// four-option loadout).
pub const MAX_OPTIONS_EXTENDED: u8 = 4;
/// Shield timer written by `$89A0` (`LDA #$05`).
pub const SHIELD_HITS: u8 = 5;

/// The six meter entries, in the original's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerUp {
    SpeedUp,
    Missile,
    Double,
    Laser,
    Option,
    Shield,
}

/// Slot order for the HUD and [`PowerMeter::selected`].
pub const SLOTS: [PowerUp; SLOT_COUNT] = [
    PowerUp::SpeedUp,
    PowerUp::Missile,
    PowerUp::Double,
    PowerUp::Laser,
    PowerUp::Option,
    PowerUp::Shield,
];

impl PowerUp {
    /// Label shown in the HUD.
    pub fn label(self) -> &'static str {
        match self {
            PowerUp::SpeedUp => "SPEED",
            PowerUp::Missile => "MISSILE",
            PowerUp::Double => "DOUBLE",
            PowerUp::Laser => "LASER",
            PowerUp::Option => "OPTION",
            PowerUp::Shield => "SHIELD",
        }
    }
}

/// Player upgrades and meter cursor. The individual fields are the original's
/// `$40`, `$41`, `$44`, `$45`, `$46` and `$42`.
#[derive(Clone, Debug, Default)]
pub struct PowerMeter {
    selection: u8,
    capsules: u8,
    speed_level: u8,
    missile: bool,
    weapon_level: u8,
    options: u8,
    shield: u8,
    /// Meter boxes drawn empty on screen: the last sample of
    /// [`PowerMeter::empty_slots`], taken every eighth NMI frame.
    shown_empty: [bool; SLOT_COUNT],
    /// Highlighted box on screen (`$42` as last drawn).
    shown_selection: u8,
    /// A pickup's selection, drawn one frame after the pickup.
    queued_selection: Option<u8>,
    /// The meter as it stood at the end of the previous frame: the redraw
    /// reads it before the frame's own player update runs.
    prev_empty: [bool; SLOT_COUNT],
    prev_selection: u8,
}

impl PowerMeter {
    /// `$42`; 0 means "no selection".
    pub fn selection(&self) -> u8 {
        self.selection
    }

    /// The selected power-up, or `None` for selection 0.
    pub fn selected(&self) -> Option<PowerUp> {
        if self.selection == 0 {
            None
        } else {
            SLOTS.get(usize::from(self.selection) - 1).copied()
        }
    }

    /// Capsules collected so far, shown on the HUD.
    pub fn capsules(&self) -> u8 {
        self.capsules
    }

    /// `$8950`: a capsule pickup advances the highlight and wraps 6 -> 1.
    pub fn advance(&mut self) {
        self.capsules = self.capsules.saturating_add(1);
        self.selection += 1;
        if self.selection >= 7 {
            self.selection = 1;
        }
        self.queued_selection = Some(self.selection);
    }

    /// Place the cursor on a raw `$42` value (tests and replay tools).
    pub fn select(&mut self, selection: u8) {
        self.selection = selection.min(6);
    }

    /// `$8979`/`$898E`: apply the selected item on a held B press.
    ///
    /// `extra_options` selects the option cap: `false` follows `$89DA`
    /// (`$45 < 2`), `true` allows the arcade's four options.
    ///
    /// Returns the applied item, or `None` for a no-op. A no-op keeps the
    /// selection, matching the early `RTS` in every handler.
    pub fn activate(&mut self, extra_options: bool) -> Option<PowerUp> {
        let power = self.selected()?;
        let applied = match power {
            PowerUp::SpeedUp => {
                self.speed_level = self.speed_level.saturating_add(1);
                true
            }
            PowerUp::Missile => {
                let new = !self.missile;
                self.missile = true;
                new
            }
            PowerUp::Double => {
                let new = self.weapon_level != 2;
                self.weapon_level = 2;
                new
            }
            PowerUp::Laser => {
                let new = self.weapon_level != 1;
                self.weapon_level = 1;
                new
            }
            PowerUp::Option => {
                let limit = if extra_options {
                    MAX_OPTIONS_EXTENDED
                } else {
                    MAX_OPTIONS
                };
                let new = self.options < limit;
                self.options = (self.options + 1).min(limit);
                new
            }
            PowerUp::Shield => {
                let new = self.shield == 0;
                self.shield = SHIELD_HITS;
                new
            }
        };
        if applied {
            self.selection = 0;
            Some(power)
        } else {
            None
        }
    }

    /// `$40`; the movement delta clamps at `$10` but the counter itself does
    /// not (`$A000`).
    pub fn speed_level(&self) -> u8 {
        self.speed_level
    }

    /// `$A000`-`$A017`: `min($40 + 2, 16) >> 1` in 8.8 fixed point, returned in
    /// 1/256-pixel units. Level 0 is 1.0 px/frame, level 1 is 1.5, ... and the
    /// clamp makes level 14 and above 8.0 px/frame.
    pub fn speed_delta(&self) -> i32 {
        let value = self.speed_level.saturating_add(2).min(16);
        i32::from(value) << 7
    }

    /// `$44`: 0 normal, 1 laser, 2 double.
    pub fn weapon_level(&self) -> u8 {
        self.weapon_level
    }

    /// `$41`.
    pub fn has_missile(&self) -> bool {
        self.missile
    }

    /// `$44 == 2`.
    pub fn has_double(&self) -> bool {
        self.weapon_level == 2
    }

    /// `$44 == 1`.
    pub fn has_laser(&self) -> bool {
        self.weapon_level == 1
    }

    /// `$45`: 0..=[`MAX_OPTIONS`], or 0..=[`MAX_OPTIONS_EXTENDED`] with the
    /// port's extra-options mod.
    pub fn options(&self) -> u8 {
        self.options
    }

    /// Boxes the original blanks because the upgrade can no longer be bought
    /// (`$89F5`-`$8A2F`): missile `$41 != 0`, double `$44 == 2`, laser
    /// `$44 == 1`, option `$45 >= 2` (the cap with `extra_options`) and shield
    /// `$46 != 0`. Speed-up never blanks.
    pub fn empty_slots(&self, extra_options: bool) -> [bool; SLOT_COUNT] {
        let cap = if extra_options {
            MAX_OPTIONS_EXTENDED
        } else {
            MAX_OPTIONS
        };
        [
            false,
            self.missile,
            self.has_double(),
            self.has_laser(),
            self.options >= cap,
            self.shield != 0,
        ]
    }

    /// What the HUD shows: [`empty_slots`](Self::empty_slots) as of the last
    /// [`sample_empty`](Self::sample_empty).
    pub fn shown_empty(&self) -> [bool; SLOT_COUNT] {
        self.shown_empty
    }

    /// The cartridge redraws the meter text only every eighth NMI frame, so
    /// a box blanks or refills up to eight frames after the state changes.
    ///
    /// The same redraw rewrites the highlight from `$42`: spending a power-up
    /// clears `$42` at once, but the box stays lit until the next sample.
    pub fn sample_empty(&mut self) {
        self.shown_empty = self.prev_empty;
        self.shown_selection = self.prev_selection;
    }

    /// The highlighted box as drawn: a pickup lights its box one frame later
    /// (`$8A35` goes through the VRAM queue), a spend only at the next sample.
    pub fn shown_selection(&self) -> u8 {
        self.shown_selection
    }

    /// End of a rendered frame: a queued pickup highlight becomes visible.
    pub fn commit_selection(&mut self) {
        if let Some(selection) = self.queued_selection.take() {
            self.shown_selection = selection;
        }
    }

    /// Keep this frame's meter state for the next sample.
    pub fn record_frame(&mut self, extra_options: bool) {
        self.prev_empty = self.empty_slots(extra_options);
        self.prev_selection = self.selection;
    }

    /// `$46`: shield timer, one unit per absorbed hit.
    pub fn shield_hits(&self) -> u8 {
        self.shield
    }

    /// Analysis-only: refill `$46` so parity runs do not die. The oracle's
    /// poke lands before any meter redraw, so the shield box is empty on
    /// screen from the first frame.
    pub fn dev_set_shield(&mut self, hits: u8) {
        self.shield = hits;
        self.shown_empty[5] = hits != 0;
        self.prev_empty[5] = hits != 0;
    }

    /// `$9C58`: the demo's fixed loadout (`$46 = 5`, `$41 = 1`, `$40 = 1`,
    /// `$45 = 2`). The table drives the same player code as a real game, so the
    /// demo ship starts with one speed-up, a missile, two options and a shield.
    pub(crate) fn demo_loadout(&mut self) {
        self.shield = SHIELD_HITS;
        self.missile = true;
        self.speed_level = 1;
        self.options = 2;
    }

    /// Consume one shield unit. Returns true when the hit was blocked.
    pub fn take_shield_hit(&mut self) -> bool {
        if self.shield == 0 {
            return false;
        }
        self.shield -= 1;
        true
    }

    /// Lose every upgrade on death (`$97D0`). The cursor `$42` survives: the
    /// oracle still holds `$42 = 1` through death and respawn (frames
    /// 1275-1400 of `docs/analysis/powerups.md`, "Empty boxes").
    pub fn reset(&mut self) {
        let capsules = self.capsules;
        let selection = self.selection;
        let shown_empty = self.shown_empty;
        let shown_selection = self.shown_selection;
        let (prev_empty, prev_selection) = (self.prev_empty, self.prev_selection);
        *self = Self::default();
        // The capsule counter is a port-side HUD convenience; the ROM keeps no
        // such count, so keep it for display continuity.
        self.capsules = capsules;
        self.selection = selection;
        // The boxes stay blank until the next redraw sample.
        self.shown_empty = shown_empty;
        self.shown_selection = shown_selection;
        self.prev_empty = prev_empty;
        self.prev_selection = prev_selection;
    }

    /// Full reset including the capsule counter (new game).
    pub fn reset_all(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capsule_advances_the_highlight_and_wraps_at_six() {
        let mut meter = PowerMeter::default();
        for expected in 1..=6 {
            meter.advance();
            assert_eq!(meter.selection(), expected);
        }
        meter.advance();
        assert_eq!(meter.selection(), 1);
        assert_eq!(meter.capsules(), 7);
    }

    #[test]
    fn the_speed_table_matches_the_rom() {
        let mut meter = PowerMeter::default();
        let expected = [
            256, 384, 512, 640, 768, 896, 1024, 1152, 1280, 1408, 1536, 1664, 1792, 1920, 2048,
            2048,
        ];
        for (level, delta) in expected.into_iter().enumerate() {
            meter.speed_level = level as u8;
            assert_eq!(meter.speed_delta(), delta, "level {level}");
        }
    }

    #[test]
    fn activation_resets_the_selection_but_a_no_op_keeps_it() {
        let mut meter = PowerMeter::default();
        meter.select(3);
        assert_eq!(meter.activate(false), Some(PowerUp::Double));
        assert_eq!(meter.weapon_level(), 2);
        assert_eq!(meter.selection(), 0);

        meter.select(3);
        assert_eq!(meter.activate(false), None, "already double");
        assert_eq!(meter.selection(), 3);
    }

    #[test]
    fn options_stop_at_two_by_default_and_four_with_the_mod() {
        let mut meter = PowerMeter::default();
        for _ in 0..8 {
            meter.select(5);
            meter.activate(false);
        }
        assert_eq!(meter.options(), MAX_OPTIONS);

        let mut extended = PowerMeter::default();
        for _ in 0..8 {
            extended.select(5);
            extended.activate(true);
        }
        assert_eq!(extended.options(), MAX_OPTIONS_EXTENDED);
    }

    #[test]
    fn shield_is_once() {
        let mut meter = PowerMeter::default();
        meter.select(6);
        assert_eq!(meter.activate(false), Some(PowerUp::Shield));
        meter.select(6);
        assert_eq!(meter.activate(false), None);
        assert_eq!(meter.shield_hits(), SHIELD_HITS);
    }

    #[test]
    fn speed_up_stacks_and_weapon_modes_alternate() {
        let mut meter = PowerMeter::default();
        for expected in 1..=4 {
            meter.select(1);
            assert_eq!(meter.activate(false), Some(PowerUp::SpeedUp));
            assert_eq!(meter.speed_level(), expected);
        }
        // Double and Laser replace each other; selecting the active one is a
        // no-op and keeps the selection.
        meter.select(3);
        assert_eq!(meter.activate(false), Some(PowerUp::Double));
        meter.select(4);
        assert_eq!(meter.activate(false), Some(PowerUp::Laser));
        assert_eq!(meter.weapon_level(), 1);
        meter.select(3);
        assert_eq!(meter.activate(false), Some(PowerUp::Double));
        assert_eq!(meter.weapon_level(), 2);
        meter.select(3);
        assert_eq!(meter.activate(false), None);
        assert_eq!(meter.selection(), 3);
    }

    #[test]
    fn missile_is_installed_once() {
        let mut meter = PowerMeter::default();
        meter.select(2);
        assert_eq!(meter.activate(false), Some(PowerUp::Missile));
        meter.select(2);
        assert_eq!(meter.activate(false), None);
        assert!(meter.has_missile());
    }

    #[test]
    fn shield_absorbs_one_hit_per_unit() {
        let mut meter = PowerMeter::default();
        meter.select(6);
        meter.activate(false);
        for _ in 0..SHIELD_HITS {
            assert!(meter.take_shield_hit());
        }
        assert!(!meter.take_shield_hit());
    }
}
