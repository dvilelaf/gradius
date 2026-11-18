//! Saved options: `settings.ini` in the data folder, plain `key=value` lines.
//!
//! Written by the in-game menu, read at startup. Command-line flags win for
//! the session that gave them (see [`Settings::apply_to`]); a bad or missing
//! file just means the defaults.

use std::io::ErrorKind;
use std::path::Path;

use crate::options::{Options, MAX_HITS};
use crate::volume::Volume;

pub const FILE_NAME: &str = "settings.ini";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub hits: u8,
    pub start_stage: u8,
    pub simultaneous_2p: bool,
    pub stage5_slowdown: bool,
    pub volume: u8,
    pub fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hits: 1,
            start_stage: 1,
            simultaneous_2p: false,
            // The port ships without stage 5's `$9658` halving; the cartridge
            // is the opt-in.
            stage5_slowdown: false,
            volume: 100,
            fullscreen: false,
        }
    }
}

impl Settings {
    /// Unknown keys and unparsable values are ignored, out-of-range ones clamped.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            let flag = matches!(value, "1" | "true" | "on");
            match key {
                "hits" => {
                    // Out-of-range numbers snap to the nearest limit; only
                    // non-numbers keep the default.
                    if value == "inf" {
                        settings.hits = 0;
                    } else if let Ok(hits) = value.parse::<i64>() {
                        settings.hits = hits.clamp(1, i64::from(MAX_HITS)) as u8;
                    }
                }
                "stage" => {
                    // Out-of-range numbers snap to the nearest limit; only
                    // non-numbers keep the default.
                    if let Ok(stage) = value.parse::<i64>() {
                        settings.start_stage = stage.clamp(1, 7) as u8;
                    }
                }
                "2p_sim" => settings.simultaneous_2p = flag,
                "slowdown" => settings.stage5_slowdown = flag,
                "volume" => {
                    if let Ok(volume) = value.parse::<i64>() {
                        settings.volume = Volume::from_int(volume).percent();
                    }
                }
                "fullscreen" => settings.fullscreen = flag,
                _ => {}
            }
        }
        settings
    }

    pub fn to_text(self) -> String {
        let hits = match self.hits {
            0 => "inf".to_string(),
            hits => hits.to_string(),
        };
        format!(
            "hits={}\nstage={}\n2p_sim={}\nslowdown={}\nvolume={}\nfullscreen={}\n",
            hits,
            self.start_stage,
            u8::from(self.simultaneous_2p),
            u8::from(self.stage5_slowdown),
            self.volume,
            u8::from(self.fullscreen),
        )
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(error) => {
                if error.kind() != ErrorKind::NotFound {
                    eprintln!("warning: cannot read {}: {error}", path.display());
                }
                Self::default()
            }
        }
    }

    pub fn save(self, path: &Path) {
        if let Err(error) = std::fs::write(path, self.to_text()) {
            eprintln!("warning: cannot write {}: {error}", path.display());
        }
    }

    /// The settings the session starts with: saved values, except for what
    /// the command line set.
    pub fn startup(saved: Self, options: &Options) -> Self {
        Self {
            hits: if options.hits_given {
                options.hits
            } else {
                saved.hits
            },
            start_stage: if options.start_stage_given {
                options.start_stage
            } else {
                saved.start_stage
            },
            simultaneous_2p: options.simultaneous_2p || saved.simultaneous_2p,
            stage5_slowdown: options.stage5_slowdown || saved.stage5_slowdown,
            volume: if options.volume_given {
                options.volume.percent()
            } else {
                saved.volume
            },
            fullscreen: saved.fullscreen,
        }
    }

    pub fn mods(self) -> gradius_core::Mods {
        gradius_core::Mods {
            hull_hits: self.hits,
            start_stage: self.start_stage,
            simultaneous_2p: self.simultaneous_2p,
            stage5_slowdown: self.stage5_slowdown,
            ..gradius_core::Mods::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        let settings = Settings {
            hits: 3,
            start_stage: 4,
            simultaneous_2p: true,
            stage5_slowdown: true,
            volume: 40,
            fullscreen: true,
        };
        assert_eq!(Settings::parse(&settings.to_text()), settings);
        assert_eq!(
            Settings::parse(&Settings::default().to_text()),
            Settings::default()
        );
    }

    #[test]
    fn invincible_round_trips_as_inf() {
        let settings = Settings {
            hits: 0,
            ..Settings::default()
        };
        assert!(settings.to_text().starts_with("hits=inf\n"));
        assert_eq!(Settings::parse(&settings.to_text()), settings);
    }

    #[test]
    fn round_trips_through_a_file() {
        let path =
            std::env::temp_dir().join(format!("gradius-settings-{}.ini", std::process::id()));
        let settings = Settings {
            hits: 7,
            ..Settings::default()
        };
        settings.save(&path);
        assert_eq!(Settings::load(&path), settings);
        let _ = std::fs::remove_file(&path);
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn garbage_is_ignored_and_ranges_clamped() {
        let settings = Settings::parse(
            "hits=250\nstage=9\nvolume=500\nnonsense\nfoo=bar\n2p_sim=maybe\nslowdown=maybe",
        );
        assert_eq!(settings.hits, 99);
        assert_eq!(settings.start_stage, 7);
        assert_eq!(settings.volume, 100);
        assert!(!settings.simultaneous_2p);
        assert!(!settings.stage5_slowdown);
        assert_eq!(Settings::parse("hits=0").hits, 1);
        assert_eq!(Settings::parse("hits=many").hits, 1);
        assert_eq!(Settings::parse("stage=0").start_stage, 1);
        assert_eq!(Settings::parse("stage=many").start_stage, 1);
    }

    #[test]
    fn stage_five_slowdown_is_off_unless_the_file_asks_for_it() {
        assert!(!Settings::default().stage5_slowdown);
        assert!(!Settings::parse("").stage5_slowdown);
        assert!(Settings::parse("slowdown=1").stage5_slowdown);
        assert!(Settings::parse("slowdown=on").stage5_slowdown);
        assert!(!Settings::parse("slowdown=0").stage5_slowdown);
    }

    #[test]
    fn defaults_are_the_port_except_stage_five() {
        assert_eq!(
            Settings::default().mods(),
            gradius_core::Mods {
                stage5_slowdown: false,
                ..gradius_core::Mods::default()
            }
        );
    }

    #[test]
    fn command_line_flags_win_over_saved_values() {
        let saved = Settings {
            hits: 5,
            start_stage: 4,
            simultaneous_2p: false,
            stage5_slowdown: false,
            volume: 30,
            fullscreen: false,
        };
        let plain = Options::default();
        assert_eq!(Settings::startup(saved, &plain), saved);

        let flags = Options::parse(
            [
                "--hits",
                "2",
                "--start-stage",
                "3",
                "--2p-sim",
                "--stage5-slowdown",
                "--volume",
                "80",
            ]
            .iter()
            .map(std::ffi::OsString::from),
        )
        .unwrap();
        let started = Settings::startup(saved, &flags);
        assert_eq!(started.hits, 2);
        assert_eq!(started.start_stage, 3);
        assert!(started.simultaneous_2p);
        assert!(started.stage5_slowdown);
        assert_eq!(started.volume, 80);
    }
}
