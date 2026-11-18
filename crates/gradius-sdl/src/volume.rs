//! Master volume for the SDL audio sink, independent of the core.

/// Percent added or removed by one `+`/`-` press.
pub const STEP: i32 = 5;

/// A master volume: a 0..=100 percentage plus a mute flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volume {
    percent: u8,
    muted: bool,
}

impl Volume {
    pub fn new(percent: u8) -> Self {
        Self {
            percent: percent.min(100),
            muted: false,
        }
    }

    /// Clamp any integer into the supported range, used by `--volume`.
    pub fn from_int(percent: i64) -> Self {
        Self::new(percent.clamp(0, 100) as u8)
    }

    pub fn percent(self) -> u8 {
        self.percent
    }

    pub fn is_muted(self) -> bool {
        self.muted
    }

    /// Linear gain for the mixer: muted is silent, otherwise percent / 100.
    pub fn gain(self) -> f32 {
        if self.muted {
            0.0
        } else {
            f32::from(self.percent) / 100.0
        }
    }

    /// Move by `delta` percent, clamped to 0..=100. Adjusting unmutes.
    pub fn adjust(&mut self, delta: i32) -> u8 {
        self.percent = (i32::from(self.percent) + delta).clamp(0, 100) as u8;
        self.muted = false;
        self.percent
    }

    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        self.muted
    }

    /// Short text flashed in the window title after a change.
    pub fn notice(self) -> String {
        if self.is_muted() {
            "muted".to_string()
        } else {
            format!("volume {}%", self.percent())
        }
    }
}

impl Default for Volume {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_maps_linearly_to_gain() {
        assert_eq!(Volume::new(0).gain(), 0.0);
        assert_eq!(Volume::new(50).gain(), 0.5);
        assert_eq!(Volume::new(100).gain(), 1.0);
    }

    #[test]
    fn values_are_clamped_to_the_supported_range() {
        assert_eq!(Volume::from_int(-20).percent(), 0);
        assert_eq!(Volume::from_int(150).percent(), 100);
        let mut volume = Volume::new(95);
        assert_eq!(volume.adjust(STEP), 100);
        assert_eq!(volume.adjust(STEP), 100);
        volume = Volume::new(3);
        assert_eq!(volume.adjust(-STEP), 0);
    }

    #[test]
    fn mute_is_silent_and_preserves_the_percentage() {
        let mut volume = Volume::new(70);
        assert!(volume.toggle_mute());
        assert_eq!(volume.gain(), 0.0);
        assert_eq!(volume.notice(), "muted");
        assert!(!volume.toggle_mute());
        assert_eq!(volume.gain(), 0.7);
        assert_eq!(volume.notice(), "volume 70%");
    }

    #[test]
    fn adjusting_while_muted_unmutes() {
        let mut volume = Volume::new(70);
        volume.toggle_mute();
        volume.adjust(-STEP);
        assert!(!volume.is_muted());
        assert_eq!(volume.percent(), 65);
        assert_eq!(volume.gain(), 0.65);
    }
}
