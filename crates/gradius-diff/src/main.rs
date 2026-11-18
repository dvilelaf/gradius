//! `gradius-diff`: run the native port and the cartridge oracle in lockstep.
//!
//! Options are parsed by hand, like `gradius-sdl`, to keep the crate free of
//! argument-parsing dependencies.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use gradius_assets::Pack;
use gradius_cart::Cartridge;
use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig};
use gradius_diff::dump::{keep_alive_poke, oracle_snapshot, Snapshot};
use gradius_diff::fixture::write_fixture;
use gradius_diff::pixel::{write_png, PixelDiff, PixelSummary, PixelWindow};
use gradius_diff::rewind;
use gradius_diff::script::{default_script, parse_script, Script};
use gradius_diff::{core_buttons, nes_button_bits};
use gradius_nes::Emulator;
use sha2::{Digest, Sha256};

const HELP: &str = "\
usage: gradius-diff --rom PATH --pack PATH --frames N [OPTIONS]
       gradius-diff --rom PATH --emit-fixture PATH [--frames N] [--script PATH]

required:
  --rom PATH       original cartridge (iNES)
  --pack PATH      self-contained asset pack for the native port
  --frames N       number of frames to run in lockstep (600 with --emit-fixture)

options:
  --emit-fixture PATH  record the oracle's state lines to PATH and exit; needs no
                       ROM at run time afterwards and no asset pack
  --script PATH    input script (`FRAME:buttons[:duration]` lines, `p2 ` prefix
                   for the second pad; default is documented)
  --stage N        start both sides at stage N (1-7); the oracle pokes `$26`
                   until stage N is playing. The native port enters five frames
                   later, after the oracle's `$0D` scroll hold, so scroll and the
                   ship start together; the first-column spawner lead is still a
                   known diagnostic difference. Without --from/--to the compared
                   window is the whole run from the native's stage entry.
  --from FRAME     compare or record only frames at or after this frame
  --to FRAME       compare or record only frames at or before this frame
  --rewind-at FRAME
                   snapshot the native port at this frame, then after
                   --rewind-span frames restore the snapshot and replay the
                   same inputs; the report says whether the replayed state and
                   the continuous one are identical
  --rewind-span N  frames to replay after the --rewind-at snapshot (600)
  --keep-alive     analysis only: keep the oracle player alive (like xtask)
  --native-keep-alive
                   analysis only: keep the native player alive as well
  --oracle-out F   write one oracle state line per frame to F
  --native-out F   write one native state line per frame to F
  --tolerance PX   allowed position difference in whole pixels (default 1)
  --json PATH      write the summary as JSON to PATH
  --report         also list the first ten divergences
  --png-dir DIR    write oracle-XXXX.png and native-XXXX.png here
  --png-every N    sample every N frames (default every frame with --pixel-report)
  --pixel-report   print a per-region pixel match table
  --pixel-window FROM:TO
                   report only pixel samples in this inclusive frame range
  -h, --help       print this help

By default only frames where the oracle and the port are both in the Playing
state are compared; the report names the window and counts the skipped frames.
--from/--to restrict comparison to an inclusive frame range and compare every
frame in it, even if a side has left Playing at the clear.

exit status:
  0  the run stayed within tolerance
  1  the run diverged
  2  usage or I/O error

default script:
  Start pulses at frames 180 and 240 (6 frames each), then from frame 300 the
  ship holds fire and steers in 60-frame blocks: up, right, left, up, ...

script lines:
  `FRAME:buttons[:duration]` presses P1; prefix the line with `p2 ` to press
  the second controller (`p2 900:start`). The oracle reads both pads
  (`$4016`/`$4017`); the native port still takes P1 only until the two-player
  runtime is wired, so a P2-only press is an oracle-only input today.";

#[derive(Debug)]
struct Options {
    rom: PathBuf,
    pack: Option<PathBuf>,
    frames: u64,
    emit_fixture: Option<PathBuf>,
    script: Option<PathBuf>,
    stage: Option<u8>,
    from: Option<u64>,
    to: Option<u64>,
    rewind_at: Option<u64>,
    rewind_span: u64,
    keep_alive: bool,
    native_keep_alive: bool,
    oracle_out: Option<PathBuf>,
    native_out: Option<PathBuf>,
    json: Option<PathBuf>,
    tolerance_px: i64,
    report: bool,
    png_dir: Option<PathBuf>,
    png_every: Option<u64>,
    pixel_report: bool,
    pixel_window: Option<PixelWindow>,
}

impl Options {
    fn parse() -> Result<Option<Self>, String> {
        let mut rom = None;
        let mut pack = None;
        let mut frames = None;
        let mut emit_fixture = None;
        let mut script = None;
        let mut stage = None;
        let mut from = None;
        let mut to = None;
        let mut rewind_at = None;
        let mut rewind_span = 600;
        let mut keep_alive = false;
        let mut native_keep_alive = false;
        let mut oracle_out = None;
        let mut native_out = None;
        let mut json = None;
        let mut tolerance_px = 1;
        let mut report = false;
        let mut png_dir = None;
        let mut png_every = None;
        let mut pixel_report = false;
        let mut pixel_window = None;
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            let arg = arg
                .into_string()
                .map_err(|_| "argument is not valid UTF-8")?;
            match arg.as_str() {
                "--rom" => rom = Some(PathBuf::from(next_value(&mut args, "--rom")?)),
                "--pack" => pack = Some(PathBuf::from(next_value(&mut args, "--pack")?)),
                "--frames" => {
                    let value = next_value(&mut args, "--frames")?;
                    frames = Some(
                        value
                            .parse()
                            .map_err(|_| format!("bad --frames value `{value}`"))?,
                    );
                }
                "--emit-fixture" => {
                    emit_fixture = Some(PathBuf::from(next_value(&mut args, "--emit-fixture")?))
                }
                "--script" => script = Some(PathBuf::from(next_value(&mut args, "--script")?)),
                "--stage" => {
                    let value = next_value(&mut args, "--stage")?;
                    let parsed = value
                        .parse::<u8>()
                        .map_err(|_| format!("bad --stage value `{value}`"))?;
                    if !(1..=7).contains(&parsed) {
                        return Err(format!("--stage must be 1-7, got {parsed}"));
                    }
                    stage = Some(parsed);
                }
                "--from" => {
                    let value = next_value(&mut args, "--from")?;
                    from = Some(
                        value
                            .parse()
                            .map_err(|_| format!("bad --from value `{value}`"))?,
                    );
                }
                "--to" => {
                    let value = next_value(&mut args, "--to")?;
                    to = Some(
                        value
                            .parse()
                            .map_err(|_| format!("bad --to value `{value}`"))?,
                    );
                }
                "--rewind-at" => {
                    let value = next_value(&mut args, "--rewind-at")?;
                    rewind_at = Some(
                        value
                            .parse()
                            .map_err(|_| format!("bad --rewind-at value `{value}`"))?,
                    );
                }
                "--rewind-span" => {
                    let value = next_value(&mut args, "--rewind-span")?;
                    rewind_span = value
                        .parse()
                        .map_err(|_| format!("bad --rewind-span value `{value}`"))?;
                }
                "--keep-alive" => keep_alive = true,
                "--native-keep-alive" => native_keep_alive = true,
                "--oracle-out" => {
                    oracle_out = Some(PathBuf::from(next_value(&mut args, "--oracle-out")?))
                }
                "--native-out" => {
                    native_out = Some(PathBuf::from(next_value(&mut args, "--native-out")?))
                }
                "--json" => json = Some(PathBuf::from(next_value(&mut args, "--json")?)),
                "--tolerance" => {
                    let value = next_value(&mut args, "--tolerance")?;
                    tolerance_px = value
                        .parse::<u32>()
                        .map(i64::from)
                        .map_err(|_| format!("bad --tolerance value `{value}`"))?;
                }
                "--report" => report = true,
                "--png-dir" => png_dir = Some(PathBuf::from(next_value(&mut args, "--png-dir")?)),
                "--png-every" => {
                    let value = next_value(&mut args, "--png-every")?;
                    png_every = Some(
                        value
                            .parse()
                            .map_err(|_| format!("bad --png-every value `{value}`"))?,
                    );
                }
                "--pixel-report" => pixel_report = true,
                "--pixel-window" => {
                    pixel_window = Some(PixelWindow::parse(&next_value(
                        &mut args,
                        "--pixel-window",
                    )?)?)
                }
                "-h" | "--help" => {
                    println!("{HELP}");
                    return Ok(None);
                }
                other => return Err(format!("unknown argument `{other}`\n\n{HELP}")),
            }
        }
        if png_every == Some(0) {
            return Err("--png-every must be at least 1".to_string());
        }
        if png_dir.is_some() && png_every.is_none() {
            return Err("--png-dir needs --png-every".to_string());
        }
        if pixel_window.is_some() && !pixel_report {
            return Err("--pixel-window needs --pixel-report".to_string());
        }
        if let (Some(from), Some(to)) = (from, to) {
            if from > to {
                return Err(format!("--from {from} is after --to {to}"));
            }
        }
        let frames = match frames {
            Some(frames) => frames,
            // A fixture is a fixed-length recording; 600 is the stage-1 window.
            None if emit_fixture.is_some() => 600,
            None => return Err("missing --frames".to_string()),
        };
        let pack = match emit_fixture {
            // Recording the oracle never starts the native port.
            Some(_) => pack,
            None => Some(pack.ok_or("missing --pack")?),
        };
        if rewind_span == 0 {
            return Err("--rewind-span must be at least 1".to_string());
        }
        if let Some(at) = rewind_at {
            if at == 0 {
                return Err("--rewind-at must be at least 1".to_string());
            }
            if at + rewind_span > frames {
                return Err(format!(
                    "--rewind-at {at} plus --rewind-span {rewind_span} runs past --frames {frames}"
                ));
            }
        }
        Ok(Some(Self {
            rom: rom.ok_or("missing --rom")?,
            pack,
            frames,
            emit_fixture,
            script,
            stage,
            from,
            to,
            rewind_at,
            rewind_span,
            keep_alive,
            native_keep_alive,
            oracle_out,
            native_out,
            json,
            tolerance_px,
            report,
            png_dir,
            png_every,
            pixel_report,
            pixel_window,
        }))
    }
}

/// `$0D = 5`: the oracle holds the scroll for five frames after the stage
/// handler starts. `Game::new_at_stage` starts in normal play, so the native
/// stage entry waits this long after the oracle's first Playing stage frame to
/// start from the same scroll, ship and score state. The spawner still lacks
/// the oracle's first-column lead, so a `--stage` run stays a diagnostic.
const STAGE_BOOT_LEAD: u64 = 5;

/// The oracle frame on which a `--stage` run creates the native `Game`.
fn stage_entry_frame(first_playing: u64) -> u64 {
    first_playing + STAGE_BOOT_LEAD
}

fn next_value(args: &mut impl Iterator<Item = OsString>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} needs a value"))?
        .into_string()
        .map_err(|_| format!("{flag} value is not valid UTF-8"))
}

/// Whether this frame still needs the analysis-only `$26` poke. Stage init
/// (`$9B68`) copies `$26,X` into `$19`; writing it before the first stage
/// start makes the oracle boot into the requested stage. Stage 1 (`$19 = 0`)
/// needs no poke.
fn needs_stage_poke(emulator: &Emulator, target: u8) -> bool {
    target > 0 && !(emulator.bus.ram[0x00] == 5 && emulator.bus.ram[0x19] == target)
}

/// Human label for the oracle's dumped state, for the per-state pixel summary.
fn state_label(state: &str) -> &str {
    match state {
        "Other(1)" => "Attract",
        "Other(2)" => "Blank",
        "Other(3)" => "Continue",
        "Other(4)" => "StageInit",
        other => other,
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("gradius-diff: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let Some(options) = Options::parse()? else {
        return Ok(ExitCode::SUCCESS);
    };
    let script = load_script(&options)?;
    let data = std::fs::read(&options.rom)
        .map_err(|error| format!("cannot read ROM {}: {error}", options.rom.display()))?;
    let cart = Cartridge::from_ines(&data)
        .map_err(|error| format!("cannot parse ROM {}: {error}", options.rom.display()))?;
    if let Some(path) = &options.emit_fixture {
        let sha256 = format!("{:x}", Sha256::digest(&data));
        return emit_fixture(path, &cart, &script, &options, &sha256);
    }
    let pack_path = options.pack.as_ref().ok_or("missing --pack")?;
    let mut emulator =
        Emulator::new(&cart).map_err(|error| format!("cannot start oracle: {error}"))?;
    emulator.set_audio_enabled(false);
    let pack = Pack::load(pack_path)
        .map_err(|error| format!("cannot load asset pack {}: {error}", pack_path.display()))?;
    // `--stage N` is 1-based like the rest of the tooling; `$19` is N - 1.
    let stage = options.stage.map(|number| number - 1);
    let assets = pack.into_assets();
    let mut game = if stage.is_none() {
        let mut game = Game::new(assets.clone());
        if options.native_keep_alive {
            game.dev_keep_alive(true);
        }
        Some(game)
    } else {
        None
    };
    // The oracle boots through title/init, so the native stage entry waits for
    // the oracle's stage-init frame and then steps in lockstep. `frame_offset`
    // maps the native's frame 1 onto the oracle's stage-entry frame.
    let mut frame_offset = 0u64;
    let mut stage_first_playing = None;

    let sample_every = match options.png_every {
        Some(every) => Some(every),
        None if options.pixel_report => Some(1),
        None => None,
    };
    if let Some(dir) = &options.png_dir {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }
    let mut pixels = PixelSummary::default();
    if let Some(window) = options.pixel_window {
        pixels.set_window(window);
    }
    let mut oracle_lines = Vec::with_capacity(options.frames as usize);
    let mut native_lines = Vec::with_capacity(options.frames as usize);
    // Oracle lines that have a native counterpart, for a `--stage` run.
    let mut paired_oracle: Vec<Snapshot> = Vec::with_capacity(options.frames as usize);
    // `--rewind-at`: the image taken at that frame, the inputs of the span and
    // the continuous run's final state line.
    let mut rewind_state: Option<Vec<u8>> = None;
    let mut rewind_inputs: Vec<gradius_core::Buttons> = Vec::new();
    let mut rewind_continuous: Option<String> = None;
    for frame in 0..options.frames {
        let pads = script.pads(frame);
        if let Some(target) = stage {
            if needs_stage_poke(&emulator, target) {
                // `$9B68` copies `$26,X` into `$19` at stage init.
                emulator.bus.ram[0x26] = target;
            }
        }
        emulator.set_buttons_p1(nes_button_bits(pads.p1));
        emulator.set_buttons_p2(nes_button_bits(pads.p2));
        emulator.run_frame();
        if options.keep_alive {
            keep_alive_poke(&mut emulator);
        }
        let oracle = oracle_snapshot(&emulator);
        if let Some(target) = stage {
            if game.is_none() && oracle.state == "Playing" {
                debug_assert_eq!(emulator.bus.ram[0x19], target);
                let first = *stage_first_playing.get_or_insert(oracle.frame);
                if oracle.frame >= stage_entry_frame(first) {
                    // `$02` at the anchor, less one: `step` increments before
                    // the frame body, so the first compared frame reads the
                    // cartridge's value.
                    let nmi_seed = (oracle.nmi.expect("oracle `$02`") as u8).wrapping_sub(1);
                    let mut started = Game::new_at_stage_seeded(assets.clone(), target, nmi_seed);
                    if options.native_keep_alive {
                        started.dev_keep_alive(true);
                    }
                    // Native frame 1 is the oracle's first scrolling frame.
                    frame_offset = oracle.frame - 1;
                    game = Some(started);
                }
            }
        }
        let Some(active) = game.as_mut() else {
            oracle_lines.push(oracle);
            continue;
        };
        active.step(core_buttons(pads.p1));
        let dump = active.dump_state();
        let mut native =
            Snapshot::parse(&dump).map_err(|error| format!("cannot parse native dump: {error}"))?;
        native.frame += frame_offset;
        if let Some(at) = options.rewind_at {
            if oracle.frame == at {
                rewind_state = Some(active.snapshot());
                rewind_inputs.clear();
            } else if oracle.frame > at && oracle.frame == at + options.rewind_span {
                rewind_inputs.push(core_buttons(pads.p1));
                rewind_continuous = Some(dump);
            } else if oracle.frame > at && oracle.frame < at + options.rewind_span {
                rewind_inputs.push(core_buttons(pads.p1));
            }
        }
        if sample_every.is_some_and(|every| frame % every == 0) {
            let oracle_frame = emulator.framebuffer();
            let native_frame = active.framebuffer().as_rgba();
            if let Some(dir) = &options.png_dir {
                write_png(&dir.join(format!("oracle-{frame:04}.png")), oracle_frame)?;
                write_png(&dir.join(format!("native-{frame:04}.png")), native_frame)?;
            }
            if options.pixel_report {
                pixels.push(
                    frame,
                    state_label(&oracle.state),
                    PixelDiff::compare(oracle_frame, native_frame),
                );
            }
        }
        paired_oracle.push(oracle.clone());
        oracle_lines.push(oracle);
        native_lines.push(native);
    }

    if let Some(path) = &options.oracle_out {
        write_lines(path, &oracle_lines)?;
    }
    if let Some(path) = &options.native_out {
        write_lines(path, &native_lines)?;
    }

    let mut rewind_identical = true;
    if let Some(at) = options.rewind_at {
        let end = at + options.rewind_span;
        let snapshot = rewind_state
            .ok_or_else(|| format!("--rewind-at {at} came before the native port started"))?;
        let continuous = rewind_continuous.ok_or_else(|| {
            format!(
                "--rewind-at {at} plus --rewind-span {} runs past the last frame",
                options.rewind_span
            )
        })?;
        let active = game.as_mut().ok_or("no native game to rewind")?;
        let diff = rewind::check(active, &snapshot, &rewind_inputs, &continuous)?;
        if diff.is_identical() {
            println!(
                "rewind: identical (frames {at}-{end}, {} replayed)",
                options.rewind_span
            );
        } else {
            rewind_identical = false;
            println!(
                "rewind: diverged (frames {at}-{end}, {} replayed)",
                options.rewind_span
            );
            println!("{}", diff.report());
        }
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = if stage.is_some() {
        paired_oracle.iter().zip(&native_lines).collect()
    } else {
        oracle_lines.iter().zip(&native_lines).collect()
    };
    let window = match (options.from, options.to) {
        (None, None) if stage.is_some() => {
            // A stage run compares its whole span: the stage-init transition
            // and the stage clear are part of the behaviour being checked.
            match (native_lines.first(), native_lines.last()) {
                (Some(first), Some(last)) => Some((first.frame, last.frame)),
                _ => None,
            }
        }
        (None, None) => None,
        (from, to) => Some((from.unwrap_or(0), to.unwrap_or(u64::MAX))),
    };
    let report = compare(
        pairs,
        CompareConfig {
            tolerance_px: options.tolerance_px,
            window,
            ..CompareConfig::default()
        },
    );
    if let Some(path) = &options.json {
        let json = if options.pixel_report {
            merge_json(&report.to_json(), &pixels.to_json())
        } else {
            report.to_json()
        };
        std::fs::write(path, json)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    if options.report {
        print!("{}", report.detailed());
    } else {
        print!("{}", report.summary());
    }
    if options.pixel_report {
        print!("{}", pixels.table());
    }
    Ok(if report.within_tolerance() && rewind_identical {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// Record the oracle's state lines for `frames` frames and write a fixture.
/// `window` keeps only the inclusive frame range, like `--from`/`--to` do for
/// a comparison. This needs the cartridge but never the asset pack or the
/// native port.
fn emit_fixture(
    path: &std::path::Path,
    cart: &Cartridge,
    script: &Script,
    options: &Options,
    rom_sha256: &str,
) -> Result<ExitCode, String> {
    let frames = options.frames;
    let mut emulator =
        Emulator::new(cart).map_err(|error| format!("cannot start oracle: {error}"))?;
    emulator.set_audio_enabled(false);
    let mut snapshots = Vec::with_capacity(frames as usize);
    for frame in 0..frames {
        if let Some(target) = options.stage.map(|number| number - 1) {
            if needs_stage_poke(&emulator, target) {
                emulator.bus.ram[0x26] = target;
            }
        }
        let pads = script.pads(frame);
        emulator.set_buttons_p1(nes_button_bits(pads.p1));
        emulator.set_buttons_p2(nes_button_bits(pads.p2));
        emulator.run_frame();
        if options.keep_alive {
            keep_alive_poke(&mut emulator);
        }
        snapshots.push(oracle_snapshot(&emulator));
    }
    if let (Some(from), Some(to)) = (options.from, options.to) {
        snapshots.retain(|snapshot| snapshot.frame >= from && snapshot.frame <= to);
    } else if let Some(from) = options.from {
        snapshots.retain(|snapshot| snapshot.frame >= from);
    } else if let Some(to) = options.to {
        snapshots.retain(|snapshot| snapshot.frame <= to);
    }
    let kept = snapshots.len() as u64;
    write_fixture(path, rom_sha256, kept, &snapshots)?;
    println!(
        "gradius-diff: wrote {kept} frames to {} (ROM SHA-256 {rom_sha256})",
        path.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn load_script(options: &Options) -> Result<Script, String> {
    match &options.script {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read script {}: {error}", path.display()))?;
            parse_script(&text).map_err(|error| format!("{}: {error}", path.display()))
        }
        None => Ok(default_script(options.frames)),
    }
}

/// Append one member to the end of a single-line-terminated JSON object.
fn merge_json(report: &str, member: &str) -> String {
    let trimmed = report.trim_end();
    let body = trimmed.strip_suffix('}').unwrap_or(trimmed);
    format!("{body},\n{member}\n}}\n")
}

fn write_lines(path: &std::path::Path, snapshots: &[Snapshot]) -> Result<(), String> {
    let mut out = String::new();
    for snapshot in snapshots {
        out.push_str(&snapshot.to_line());
        out.push('\n');
    }
    std::fs::write(path, out).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stage_run_waits_out_the_oracle_scroll_hold() {
        // The oracle holds the scroll for `$0D = 5` frames after the stage
        // handler reports Playing; `Game::new_at_stage` starts in Normal.
        assert_eq!(stage_entry_frame(292), 292 + STAGE_BOOT_LEAD);
        assert_eq!(stage_entry_frame(0), STAGE_BOOT_LEAD);
    }
}
