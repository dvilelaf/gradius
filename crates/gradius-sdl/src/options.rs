//! Command-line options.
//!
//! Hand-rolled on purpose: the frontend has six flags and no need for a
//! parser dependency, and this keeps the binary's startup path obvious.

use std::ffi::OsString;
use std::path::PathBuf;

use gradius_core::Mods;

use crate::viewport::Fit;
use crate::volume::Volume;

/// Largest hit count `--hits` accepts; the range is 1..=99.
pub const MAX_HITS: u8 = 99;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Asset pack to use instead of the data folder's `gradius.pak`.
    pub pack: Option<PathBuf>,
    /// A Gradius (E) ROM to build the pack from when there is none yet.
    pub rom: Option<PathBuf>,
    /// Integer window scale, 1..=8. The initial window size; resizing is free.
    pub scale: u32,
    /// How the 256x240 image maps to the window or monitor.
    pub fit: Fit,
    /// Print the resolved viewport at startup; headless runs print the
    /// standard-format samples instead of opening a window.
    pub print_viewport: bool,
    /// `Some(n)` runs headless: exactly `n` ticks, no window, no audio device.
    pub frames: Option<u64>,
    /// Open an audio device. `--no-audio` and `--mute` clear this.
    pub audio: bool,
    /// Master volume for the SDL audio sink, 0..=100 percent.
    pub volume: Volume,
    /// Headless only: write the final framebuffer here as a BMP.
    pub screenshot: Option<PathBuf>,
    /// Headless only: write the rendered soundtrack here as a mono WAV.
    pub dump_audio: Option<PathBuf>,
    /// Headless only: hold fire and weave, so smoke runs exercise gameplay.
    pub autoplay: bool,
    /// Start with the frontend debug overlay (F3 toggles it at runtime).
    pub debug_overlay: bool,
    /// Dev-only (hidden from `--help`): start the run in stage 1..=7.
    pub stage: Option<u8>,
    /// Modified mode: hits the ship survives before dying, 1..=99.
    pub hits: u8,
    /// Modified mode: stage each new game begins at, 1..=7.
    pub start_stage: u8,
    /// Modified mode: both players play at once instead of taking turns.
    pub simultaneous_2p: bool,
    /// Modified mode: run stage 5's `$9658` chain every other frame, as the
    /// cartridge does, instead of every frame as the port does by default.
    pub stage5_slowdown: bool,
    /// `--hits`, `--start-stage` and `--volume` were on the command line, so
    /// they win over `settings.ini`.
    pub hits_given: bool,
    pub start_stage_given: bool,
    pub volume_given: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            pack: None,
            rom: None,
            scale: 3,
            fit: Fit::Integer,
            print_viewport: false,
            frames: None,
            audio: true,
            volume: Volume::default(),
            screenshot: None,
            dump_audio: None,
            autoplay: false,
            debug_overlay: false,
            stage: None,
            hits: 1,
            start_stage: 1,
            simultaneous_2p: false,
            stage5_slowdown: false,
            hits_given: false,
            start_stage_given: false,
            volume_given: false,
        }
    }
}

pub const HELP: &str = "\
usage: gradius [OPTIONS]

options:
  --pack PATH     asset pack (default: gradius.pak in the data folder)
                  data folder: GRADIUS_DATA, else assets/gradius.pak here (dev),
                  else the executable's folder (with gradius.pak or a file named
                  portable; always on Windows), else $XDG_DATA_HOME/gradius
                  or ~/.local/share/gradius
  --rom PATH      your Gradius (E) .nes file; builds the pack when there is none
                  (without it the game asks for the file in its window)
  --scale N       initial integer window scale, 1..=8 (default: 3)
  --fit MODE      integer (default), aspect or stretch; resizing and F11
                  fullscreen keep 256x240 undistorted unless MODE is stretch
  --print-viewport  print the resolved fit/rect at startup (headless: samples)
  --frames N      headless: run N simulation ticks, print a summary and exit
  --no-audio      never open an audio device (alias: --mute)
  --volume N      master volume, 0..=100 percent (default: 100)
  --screenshot P  headless: write the final frame to P as a BMP
  --dump-audio P  headless: write the soundtrack to P as a 48 kHz WAV
  --autoplay      headless: hold fire and weave (demo/smoke runs)
  --debug-overlay start with the frontend debug overlay (F3 toggles)
  --hits N        modified mode: hits before dying, 1..=99 or inf (default: 1)
  --start-stage N modified mode: stage a new game starts at, 1..=7 (default: 1)
  --2p-sim        modified mode: both players play at once, not by turns
  --stage5-slowdown modified mode: stage 5 runs every other frame (the cartridge)
  -h, --help      print this help
  -V, --version   print the version

keys:
  player 1        arrows / WASD move
                  Z / J / Space fire
                  X / K power
                  Enter start
                  Backspace / Tab select
  player 2        numpad 8 / 4 / 5 / 6 or T / F / G / H move
                  numpad 0 or R fire
                  numpad . / Delete or Y power
                  numpad + or 1 start
                  numpad - or 2 select
  Pause key       pause
  + / -           volume up/down (5%)
  M               mute / unmute
  F11             fullscreen
  F12             screenshot (screenshots/*.bmp)
  F3              debug overlay (power meter, weapon, flags)
  Esc or pad Guide  options menu (Alt+F4 closes the game)

game controller:
  d-pad or left stick, A fire, B power, Start, Back (Select);
  controller #1 drives player 1, controller #2 drives player 2";

impl Options {
    pub fn parse<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let arg = arg
                .into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_string())?;
            match arg.as_str() {
                "--pack" => {
                    let value = next_arg(&mut args, "--pack")?;
                    options.pack = Some(PathBuf::from(value));
                }
                "--rom" => options.rom = Some(PathBuf::from(next_arg(&mut args, "--rom")?)),
                "--scale" => {
                    let value = next_arg(&mut args, "--scale")?;
                    let scale: u32 = value
                        .parse()
                        .map_err(|_| format!("invalid scale `{value}`: expected 1..=8"))?;
                    if !(1..=8).contains(&scale) {
                        return Err(format!("invalid scale `{scale}`: expected 1..=8"));
                    }
                    options.scale = scale;
                }
                "--fit" => {
                    let value = next_arg(&mut args, "--fit")?;
                    options.fit = Fit::parse(&value)?;
                }
                "--print-viewport" => options.print_viewport = true,
                "--frames" => {
                    let value = next_arg(&mut args, "--frames")?;
                    let frames: u64 = value
                        .parse()
                        .map_err(|_| format!("invalid frame count `{value}`"))?;
                    options.frames = Some(frames);
                }
                "--no-audio" | "--mute" => options.audio = false,
                "--volume" => {
                    let value = next_arg(&mut args, "--volume")?;
                    let percent: i64 = value
                        .parse()
                        .map_err(|_| format!("invalid volume `{value}`: expected 0..=100"))?;
                    options.volume = Volume::from_int(percent);
                    options.volume_given = true;
                }
                "--screenshot" => {
                    options.screenshot = Some(PathBuf::from(next_arg(&mut args, "--screenshot")?));
                }
                "--dump-audio" => {
                    options.dump_audio = Some(PathBuf::from(next_arg(&mut args, "--dump-audio")?));
                }
                "--autoplay" => options.autoplay = true,
                "--debug-overlay" => options.debug_overlay = true,
                "--hits" => {
                    let value = next_arg(&mut args, "--hits")?;
                    let hits: u8 = if value == "inf" {
                        0
                    } else {
                        let hits = value.parse().map_err(|_| {
                            format!("invalid hit count `{value}`: expected 1..=99 or inf")
                        })?;
                        if !(1..=MAX_HITS).contains(&hits) {
                            return Err(format!(
                                "invalid hit count `{hits}`: expected 1..=99 or inf"
                            ));
                        }
                        hits
                    };
                    options.hits = hits;
                    options.hits_given = true;
                }
                "--start-stage" => {
                    let value = next_arg(&mut args, "--start-stage")?;
                    options.start_stage = match value.parse() {
                        Ok(stage @ 1..=7) => stage,
                        _ => return Err(format!("--start-stage needs 1..=7, got `{value}`")),
                    };
                    options.start_stage_given = true;
                }
                "--stage" => {
                    let value = next_arg(&mut args, "--stage")?;
                    options.stage = match value.parse() {
                        Ok(stage @ 1..=7) => Some(stage),
                        _ => return Err(format!("--stage needs 1..=7, got `{value}`")),
                    };
                }
                "--2p-sim" => options.simultaneous_2p = true,
                "--stage5-slowdown" => options.stage5_slowdown = true,
                "--help" | "-h" => {
                    println!("{HELP}");
                    std::process::exit(0);
                }
                "--version" | "-V" => {
                    println!("gradius {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument `{other}` (try --help)")),
            }
        }
        Ok(options)
    }

    /// The core [`Mods`] the command line asks for.
    ///
    /// `--hits N` sets the hull, `--start-stage N` the stage a new game
    /// begins at, `--2p-sim` turns the alternating 2P mode into two ships on
    /// screen and `--stage5-slowdown` restores the cartridge's stage-5
    /// halving. Without flags the game is the port's default: [`Mods::default`]
    /// except for the faster stage 5.
    pub fn mods(&self) -> Mods {
        Mods {
            hull_hits: self.hits,
            start_stage: self.start_stage,
            simultaneous_2p: self.simultaneous_2p,
            stage5_slowdown: self.stage5_slowdown,
            ..Mods::default()
        }
    }
}

fn next_arg<I>(args: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = OsString>,
{
    args.next()
        .ok_or_else(|| format!("{flag} needs a value"))?
        .into_string()
        .map_err(|_| format!("{flag} needs a valid UTF-8 value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn defaults_are_windowed_3x_with_audio() {
        let options = parse(&[]).unwrap();
        assert_eq!(options, Options::default());
        assert_eq!(options.scale, 3);
        assert_eq!(options.fit, Fit::Integer);
        assert!(!options.print_viewport);
        assert!(options.audio);
        assert_eq!(options.volume, Volume::default());
        assert_eq!(options.frames, None);
        assert!(!options.debug_overlay);
    }

    #[test]
    fn stage_is_a_hidden_dev_flag_in_range() {
        assert_eq!(parse(&["--stage", "2"]).unwrap().stage, Some(2));
        assert!(parse(&["--stage", "8"]).is_err());
        assert!(parse(&[]).unwrap().stage.is_none());
    }

    #[test]
    fn parses_every_flag() {
        let options = parse(&[
            "--pack",
            "other.pak",
            "--scale",
            "5",
            "--fit",
            "aspect",
            "--print-viewport",
            "--frames",
            "180",
            "--no-audio",
            "--volume",
            "40",
            "--screenshot",
            "shot.bmp",
            "--dump-audio",
            "out.wav",
            "--autoplay",
            "--debug-overlay",
            "--hits",
            "3",
            "--2p-sim",
            "--stage5-slowdown",
        ])
        .unwrap();
        assert_eq!(options.pack, Some(PathBuf::from("other.pak")));
        assert_eq!(options.scale, 5);
        assert_eq!(options.fit, Fit::Aspect);
        assert!(options.print_viewport);
        assert_eq!(options.frames, Some(180));
        assert!(!options.audio);
        assert_eq!(options.volume.percent(), 40);
        assert_eq!(options.screenshot, Some(PathBuf::from("shot.bmp")));
        assert_eq!(options.dump_audio, Some(PathBuf::from("out.wav")));
        assert!(options.autoplay);
        assert!(options.debug_overlay);
        assert_eq!(options.hits, 3);
        assert!(options.simultaneous_2p);
        assert!(options.stage5_slowdown);
    }

    #[test]
    fn modified_mode_defaults_to_one_hit_alternating_play_and_no_slowdown() {
        let options = parse(&[]).unwrap();
        assert_eq!(options.hits, 1);
        assert!(!options.simultaneous_2p);
        assert_eq!(
            options.mods(),
            Mods {
                stage5_slowdown: false,
                ..Mods::default()
            }
        );
    }

    #[test]
    fn stage_five_slowdown_is_a_plain_flag() {
        assert!(!parse(&[]).unwrap().stage5_slowdown);
        let options = parse(&["--stage5-slowdown"]).unwrap();
        assert!(options.stage5_slowdown);
        assert!(options.mods().stage5_slowdown);
        assert!(!options.mods().simultaneous_2p);
    }

    #[test]
    fn parses_every_hit_count_in_range() {
        assert_eq!(parse(&["--hits", "1"]).unwrap().hits, 1);
        assert_eq!(parse(&["--hits", "3"]).unwrap().hits, 3);
        assert_eq!(parse(&["--hits", "9"]).unwrap().hits, 9);
        assert_eq!(parse(&["--hits", "99"]).unwrap().hits, 99);
        let invincible = parse(&["--hits", "inf"]).unwrap();
        assert_eq!(invincible.hits, 0);
        assert!(invincible.hits_given);
    }

    #[test]
    fn rejects_out_of_range_or_missing_hit_counts() {
        let error = parse(&["--hits", "0"]).unwrap_err();
        assert!(error.contains("1..=99"), "{error}");
        assert!(parse(&["--hits", "100"]).is_err());
        assert!(parse(&["--hits", "300"]).is_err());
        assert!(parse(&["--hits", "many"]).is_err());
        assert!(parse(&["--hits"]).is_err());
    }

    #[test]
    fn two_player_simultaneous_is_a_flag() {
        assert!(parse(&["--2p-sim"]).unwrap().simultaneous_2p);
        assert!(!parse(&[]).unwrap().simultaneous_2p);
    }

    #[test]
    fn start_stage_is_a_modified_mode_flag_in_range() {
        assert_eq!(parse(&[]).unwrap().start_stage, 1);
        assert!(!parse(&[]).unwrap().start_stage_given);
        assert_eq!(parse(&["--start-stage", "5"]).unwrap().start_stage, 5);
        assert!(parse(&["--start-stage", "5"]).unwrap().start_stage_given);
        assert_eq!(
            parse(&["--start-stage", "7"]).unwrap().mods().start_stage,
            7
        );
        let error = parse(&["--start-stage", "8"]).unwrap_err();
        assert!(error.contains("1..=7"), "{error}");
        assert!(parse(&["--start-stage", "0"]).is_err());
        assert!(parse(&["--start-stage"]).is_err());
        // The hidden dev flag is separate and still starts without the title.
        assert!(parse(&["--start-stage", "2"]).unwrap().stage.is_none());
    }

    #[test]
    fn parses_each_fit_mode() {
        assert_eq!(parse(&["--fit", "integer"]).unwrap().fit, Fit::Integer);
        assert_eq!(parse(&["--fit", "aspect"]).unwrap().fit, Fit::Aspect);
        assert_eq!(parse(&["--fit", "stretch"]).unwrap().fit, Fit::Stretch);
    }

    #[test]
    fn rejects_unknown_fit_modes() {
        assert!(parse(&["--fit", "fractional"]).is_err());
        assert!(parse(&["--fit", "4:3"]).is_err());
        assert!(parse(&["--fit"]).is_err());
    }

    #[test]
    fn mute_is_an_alias_for_no_audio() {
        assert!(!parse(&["--mute"]).unwrap().audio);
    }

    #[test]
    fn volume_clamps_and_rejects_non_numbers() {
        assert_eq!(parse(&["--volume", "150"]).unwrap().volume.percent(), 100);
        assert_eq!(parse(&["--volume", "-5"]).unwrap().volume.percent(), 0);
        assert_eq!(parse(&["--volume", "0"]).unwrap().volume.gain(), 0.0);
        assert!(parse(&["--volume", "loud"]).is_err());
        assert!(parse(&["--volume"]).is_err());
    }

    #[test]
    fn rejects_out_of_range_scale() {
        assert!(parse(&["--scale", "0"]).is_err());
        assert!(parse(&["--scale", "9"]).is_err());
        assert!(parse(&["--scale", "two"]).is_err());
        assert!(parse(&["--scale"]).is_err());
    }

    #[test]
    fn dump_audio_needs_a_path() {
        assert!(parse(&["--dump-audio"]).is_err());
    }

    #[test]
    fn rejects_missing_and_unknown_arguments() {
        assert!(parse(&["--pack"]).is_err());
        assert!(parse(&["--frames"]).is_err());
        assert!(parse(&["--frobnicate"]).is_err());
    }
}
