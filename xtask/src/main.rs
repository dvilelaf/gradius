//! Development tooling.
//!
//! `xtask extract` is the only command that reads a cartridge. Its output is a
//! self-contained `gradius.pak`; every other command either analyses the ROM in
//! the oracle or inspects the resulting pack.

mod boss;
mod cmd;
mod command_format;
mod disasm;
mod ending;
mod enemy;
mod input;
mod pngout;
mod powerup;
mod probe;
mod ram;
mod screens;
mod sections;
mod sprite_groups;
mod stage2;
mod stage_enemies;
mod stages;
mod state;
mod state_drift;
mod symbols;
mod transition;

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use gradius_extract::{oracle, stage_data};

use cmd::extract::extract;
use cmd::inspect::{command_format_report, inspect, spawn, tiles};
use cmd::probe::{nametable_writes, probe};
use cmd::ramdump::ramdump;
use cmd::record::{record_boss, record_input, record_state, shot, transition, verify_sprite0};
pub(crate) use cmd::shared::{input_plan, load};
use cmd::shared::{parse_addr, parse_kind};
use cmd::trace::{disassemble, symbolize, trace};

#[derive(Parser, Debug)]
#[command(about = "Gradius analysis and asset pipeline", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print cartridge header fields and the SHA-256 of the file.
    Inspect { rom: PathBuf },
    /// Render every 4 KiB pattern-table half as a PNG tile sheet.
    Tiles {
        rom: PathBuf,
        out: PathBuf,
        /// Integer scale factor.
        #[arg(long, default_value_t = 4)]
        scale: u32,
    },
    /// Render every sprite group from `$8D91`/`$8E91` into a labelled sheet.
    SpriteGroups {
        rom: PathBuf,
        /// Sheet PNG (one row per group, one column per CHR bank).
        #[arg(long)]
        out: PathBuf,
        /// Also write the decoded groups as JSON.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Build the self-contained asset pack. The ROM is not needed afterwards.
    Extract { rom: PathBuf, out: PathBuf },
    /// Render the six power-ups from the pack and a labelled contact sheet.
    PowerupShots {
        pack: PathBuf,
        /// Directory for `power-<name>.png` and `power-contact-sheet.png`.
        #[arg(long)]
        out: PathBuf,
    },
    /// Record the stage-transition ledger from the oracle.
    ///
    /// Prints one line per frame with `$19`, `$1B`, `$1C`, the `$3E:$3F:$3D`
    /// scroll, the ship position, `$0100` and the live object kinds. See
    /// `docs/analysis/stage_transition.md`.
    Transition {
        rom: PathBuf,
        /// Input script (`FRAME:buttons[:duration]`, `#` comments; `,` or `+`).
        #[arg(long)]
        script: PathBuf,
        /// First frame to record (at least 1).
        #[arg(long, default_value_t = 1)]
        from: u64,
        /// Frame after the last recorded frame.
        #[arg(long, default_value_t = 600)]
        to: u64,
        /// Keep the player alive (`$46`, `$0100`, `$1B`), like `gradius-diff`.
        #[arg(long)]
        keep_alive: bool,
        /// Write the ledger here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run the oracle headlessly and save a screenshot.
    Shot {
        rom: PathBuf,
        out: PathBuf,
        /// Frame to capture (the game boots around frame 120).
        #[arg(long, default_value_t = 180)]
        frame: u64,
        /// Hold these buttons for the whole run, e.g. `start`, `up,right`.
        #[arg(long)]
        buttons: Option<String>,
        /// Additional timed press, `FRAME:buttons` (e.g. `180:start`).
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Keep the shield timer (`$46`) at 5 every frame (analysis only).
        #[arg(long)]
        keep_alive: bool,
    },
    /// Record one native-format state line per frame from the oracle.
    ///
    /// The line matches `Game::dump_state` in `gradius-core` byte for byte so a
    /// recorder file can be diffed against the native harness:
    ///
    /// ```text
    /// frame=<n> state=<Title|Playing|GameOver> px=<u8> pf=<u8> py=<u8>
    /// pyf=<u8> speed=<u8> scroll=<low>:<high>:<fraction> score=<u32>
    /// lives=<u8> capsule=<u8> meter=<u8> objects=<slot>:<type>:<x>:<y>:<substate>;...
    /// ```
    ///
    /// The line is one whitespace-separated sequence with no trailing newline;
    /// the file has one such line plus `\n` per frame. `type` is `$030C`,
    /// `substate` is `$048C`, `meter` is the `$42` power selection and the
    /// scroll triple is `$3E:$3F:$3D`.
    ///
    /// The ROM keeps no collected-capsule counter (it is a port-side HUD
    /// convenience), so `capsule` is always 0. `state` maps the `$00`
    /// dispatcher byte: 0-2 attract/demo -> `Title`, 3 -> `GameOver`, 4-6
    /// stage entry/play/ending -> `Playing`. No `--assist` is used: the oracle
    /// raises the real HUD sprite-0 hit.
    RecordState {
        rom: PathBuf,
        /// Number of frames to record (one line each).
        #[arg(long)]
        frames: u64,
        /// Hold these buttons for the whole run, e.g. `start`, `up,right`.
        #[arg(long)]
        buttons: Option<String>,
        /// Timed press, `FRAME:buttons[:frames]` (default hold: 6 frames).
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Write the lines here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Record the held P1 buttons (`$07`), the `$00`/`$1B` state bytes and the
    /// scroll triple once per frame.
    ///
    /// Used to capture the attract/demo timeline. Each line is
    ///
    /// ```text
    /// <frame>:<buttons>  # state=<hh> stage=<hh> scroll=<low>:<high>:<frac>
    /// ```
    ///
    /// where `<buttons>` is a comma-separated `a,b,select,start,up,down,left,right`
    /// list (empty when nothing is held). The comment is ignored by
    /// `gradius-diff --script`, so dropping the empty-button frames leaves a
    /// replay script.
    RecordInput {
        rom: PathBuf,
        /// Number of frames to record (one line each).
        #[arg(long)]
        frames: u64,
        /// Hold these buttons for the whole run, e.g. `start`, `up,right`.
        #[arg(long)]
        buttons: Option<String>,
        /// Timed press, `FRAME:buttons[:frames]` (default hold: 6 frames).
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Write the lines here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the replay script (`FRAME:buttons:1` press lines only).
        #[arg(long)]
        script: Option<PathBuf>,
    },
    /// List contiguous runs where two per-frame state dumps disagree on `state=`.
    ///
    /// Reads the files written by `gradius-diff --oracle-out` and
    /// `--native-out`. Field-level drift is reported by `gradius-diff`; this is
    /// the state-only view, with one line per run of mismatch frames.
    StateDrift {
        /// Oracle dump (`--oracle-out`).
        oracle: PathBuf,
        /// Native dump (`--native-out`).
        native: PathBuf,
    },
    /// Record a stage's Big Core fight.
    ///
    /// The run presses Start at frame 180, holds fire and flies a deterministic
    /// dodge script until the core spawns; then it holds the ship on the core's
    /// Y so shots connect. `--stage N` pokes `$26` until stage `N` starts
    /// (stage 1 needs no poke). One line per frame is written in the native
    /// `Game::dump_state` format with an appended
    /// `boss=<slot>:<x>:<y>:<damage>:<overheat>:<counter>` field. Stages 1-5
    /// fail if the core is never reached or never hit; stages 6-7 have no core
    /// (`$99B3` skips the spawn) and succeed when the stage transition is seen.
    RecordBoss {
        rom: PathBuf,
        /// Frame limit (the stage-1 core spawns at frame 7723).
        #[arg(long, default_value_t = 9800)]
        frames: u64,
        /// Stage to start, 1-7 (`$19 = stage - 1`).
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=7))]
        stage: u8,
        /// Write the lines here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Trace the Big Core's raw counters from the oracle for a frame window.
    ///
    /// Unlike `record-boss`, this reads the fight off the natural path so a
    /// scripted run (stage 1 -> stage 2) can be followed frame by frame. Each
    /// line carries the slot-9 kind, position, `$046C` damage, `$04AC`
    /// overheat counter, `$04CC` periods and `$042C` attack period, plus the
    /// `$9C3F` rank inputs (`$44`, `$45`, `$46`, `$19`). See
    /// `docs/analysis/stage2_boss.md`.
    BossTrace {
        rom: PathBuf,
        /// Input script (`FRAME:buttons[:duration]`, P1 lines of
        /// `gradius-diff --script`).
        #[arg(long)]
        script: PathBuf,
        /// First frame to record (inclusive).
        #[arg(long, default_value_t = 1)]
        from: u64,
        /// Frame after the last recorded frame.
        #[arg(long, default_value_t = 20500)]
        to: u64,
        /// Keep the player alive (`$46`, `$0100`, `$1B`), like `gradius-diff`.
        #[arg(long)]
        keep_alive: bool,
        /// Write the trace here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Record the stage-6 beam hazard and the stage-7 loop frame by frame.
    ///
    /// Pokes `$26` until the requested stage starts (stage 1 needs no poke),
    /// holds fire, presses Start at frame 180 and keeps the player alive. The
    /// line carries `$1B` (`phase`), `$19` (`stage`), `$28` (`loop`), `$26`
    /// (`stagebyte`) and the stage-6 collision beams. See
    /// `docs/analysis/ending.md`.
    RecordEnding {
        rom: PathBuf,
        /// Frame limit (stage 6 ends around 7467, stage 7 loops at 7025; both
        /// move with the per-rank clear timer).
        #[arg(long, default_value_t = 9000)]
        frames: u64,
        /// Stage to start, 1-7 (`$19 = stage - 1`).
        #[arg(long, default_value_t = 6, value_parser = clap::value_parser!(u8).range(1..=7))]
        stage: u8,
        /// Write the lines here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write the physical 2 KiB nametable memory at the final frame here.
        #[arg(long)]
        nametable: Option<PathBuf>,
    },
    /// Prove the HUD sprite-0 wait passes with no assist.
    /// The stage handler's split routine polls the sprite-0 flag at `$9A96`
    /// (wait for clear) and `$9A9D` (wait for set). A scanline-at-once PPU that
    /// never raised the flag would spin there forever. This command runs
    /// unassisted for `--frames` frames, counts `$2002` reads that saw status
    /// bit 6 and checks that the stage keeps scrolling; it exits non-zero with
    /// the observed counters if the wait did not pass.
    VerifySprite0 {
        rom: PathBuf,
        /// Frames to run, starting from reset with Start pressed at 180.
        #[arg(long, default_value_t = 3000)]
        frames: u64,
    },
    /// Run the oracle and report executed code coverage.
    Trace {
        rom: PathBuf,
        #[arg(long, default_value_t = 300)]
        frames: u64,
        /// Number of hottest addresses to print.
        #[arg(long, default_value_t = 40)]
        top: usize,
        /// Annotate PRG addresses from a symbol file (default: the Ghidra map).
        #[arg(long, value_name = "FILE")]
        symbols: Option<PathBuf>,
        /// Write the covered address ranges to this file.
        #[arg(long)]
        ranges: Option<PathBuf>,
        /// Print the first N instructions with register state.
        #[arg(long, default_value_t = 0)]
        log: usize,
        /// Hold these buttons for the whole run, e.g. `start`, `up,right`.
        #[arg(long)]
        buttons: Option<String>,
        /// Additional timed press, `FRAME:buttons[:frames]`.
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Inject the HUD-split sprite-0 hit the scanline PPU model cannot make.
        #[arg(long)]
        assist: bool,
        /// Keep the shield timer (`$46`) at 5 every frame (analysis only).
        #[arg(long)]
        keep_alive: bool,
    },
    /// Decode the enemy formation for one or more stage-1 stream types.
    Spawn {
        rom: PathBuf,
        /// Stream types as hex bytes (default: the five stage-1 types).
        #[arg(value_parser = parse_kind, default_values = ["80", "81", "82", "83", "84"])]
        kinds: Vec<u8>,
    },
    /// Disassemble the 32 KiB PRG window.
    Disasm {
        rom: PathBuf,
        /// First address (hex, `$`/`0x` optional).
        #[arg(long, value_parser = parse_addr, default_value = "8000")]
        from: u16,
        /// Last address (inclusive).
        #[arg(long, value_parser = parse_addr, default_value = "FFFF")]
        to: u16,
        /// Run the oracle and mark executed instructions; annotate the rest as data.
        #[arg(long)]
        covered: bool,
        /// Print `Lxxxx` labels for branch/jump targets and PRG data references.
        #[arg(long)]
        labels: bool,
        /// Name PRG operands and function entries from a symbol file
        /// (default: `tools/ghidra/functions_full.txt` when present).
        #[arg(long, value_name = "FILE")]
        symbols: Option<PathBuf>,
        /// Frames to run before taking the coverage trace (implies the trace).
        #[arg(long)]
        frames: Option<u64>,
        /// Hold these buttons for the whole trace run.
        #[arg(long)]
        buttons: Option<String>,
        /// Additional timed press, `FRAME:buttons[:frames]`.
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Inject the HUD-split sprite-0 hit (needed for live gameplay traces).
        #[arg(long)]
        assist: bool,
        /// Keep the shield timer (`$46`) at 5 every frame (analysis only).
        #[arg(long)]
        keep_alive: bool,
        /// Write the listing here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Merge the Ghidra function export into the stable `tools/ghidra/symbols.txt`.
    ///
    /// Existing entries in the output file win, so hand-written names survive;
    /// addresses new to the export are appended.
    Symbols {
        /// Ghidra export to read.
        #[arg(
            long,
            value_name = "FILE",
            default_value = "tools/ghidra/functions_full.txt"
        )]
        merge: PathBuf,
        /// Stable symbol file to update.
        #[arg(long, value_name = "FILE", default_value = "tools/ghidra/symbols.txt")]
        out: PathBuf,
    },
    /// Dump CPU RAM, or watch its per-frame changes.
    Ramdump {
        rom: PathBuf,
        /// Frame to snapshot (ignored with --watch).
        #[arg(long, default_value_t = 400)]
        frame: u64,
        /// Hold these buttons for the whole run.
        #[arg(long)]
        buttons: Option<String>,
        /// Additional timed press, `FRAME:buttons[:frames]`.
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Physical range to show, `START:END` in hex (default `0000:0800`).
        #[arg(long)]
        range: Option<String>,
        /// Per-frame diff window, `FROM:TO`.
        #[arg(long, value_name = "FROM:TO")]
        watch: Option<String>,
        /// With --watch, also print the N most frequently changed addresses.
        #[arg(long, default_value_t = 24)]
        top: usize,
        /// Inject the HUD-split sprite-0 hit (needed for live gameplay).
        #[arg(long)]
        assist: bool,
        /// Keep the shield timer (`$46`) at 5 every frame (analysis only).
        #[arg(long)]
        keep_alive: bool,
        /// Keep only the shield timer (`$46`) at 5: enemy contact cannot kill,
        /// but a map collision still does (analysis only).
        #[arg(long)]
        shield: bool,
        /// Write the dump here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run the oracle to a frame and dump PPU, palette, OAM and hash state.
    Probe {
        rom: PathBuf,
        /// Frame at which to take the snapshot (the game boots around frame 120).
        #[arg(long, default_value_t = 180)]
        frame: u64,
        /// Hold these buttons for the whole run, e.g. `start`, `up,right`.
        #[arg(long)]
        buttons: Option<String>,
        /// Additional timed press, `FRAME:buttons` (held for six frames).
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Write the raw 256-byte OAM image here.
        #[arg(long)]
        oam: Option<PathBuf>,
        /// Write the framebuffer as a PNG (exactly the state dumped below).
        #[arg(long)]
        png: Option<PathBuf>,
        /// Write the physical 2 KiB nametable memory here.
        #[arg(long)]
        nametables: Option<PathBuf>,
        /// Write the 2 KiB CPU RAM image here.
        #[arg(long)]
        ram: Option<PathBuf>,
        /// Also write the whole report as JSON here.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Number of OAM entries to print (max 64).
        #[arg(long, default_value_t = 32)]
        sprites: usize,
        /// Keep the shield timer (`$46`) at 5 every frame (analysis only).
        #[arg(long)]
        keep_alive: bool,
        /// Keep only the shield timer (`$46`) at 5: enemy contact cannot kill,
        /// but a map collision still does (analysis only).
        #[arg(long)]
        shield: bool,
        /// Write one CPU-RAM byte at the start of a frame,
        /// `FRAME:ADDR:VALUE` (hex for both). Repeatable.
        #[arg(long = "poke", value_name = "FRAME:ADDR:VALUE")]
        poke: Vec<String>,
    },
    /// Reconstruct the title screen and a wide stage background from the
    /// oracle's PPU VRAM write log.
    Screens {
        rom: PathBuf,
        /// Frame to stop at (the stage must be scrolling for most of the run).
        #[arg(long, default_value_t = 6000)]
        frames: u64,
        /// Additional timed press, `FRAME:buttons` (e.g. `180:start`).
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Inject the HUD-split sprite-0 hit (needed for live gameplay).
        #[arg(long)]
        assist: bool,
        /// Stage number to extract, 1-7. Stages after 1 are reached by poking
        /// the single-player stage copy `$26` before stage init. Repeat the
        /// flag to report several stages (2-7).
        #[arg(long = "stage", value_name = "N")]
        stage: Vec<u8>,
        /// Write `<name>.png` (title) and `<name>-stage.png` (wide map).
        #[arg(long)]
        png: Option<PathBuf>,
        /// Write the raw stage map (`GRD1`, width, height, tiles, attributes).
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Validate the extracted map against the live nametable at the first
        /// scroll >= this many pixels. Repeatable.
        #[arg(long = "sample-scroll", value_name = "PX")]
        sample_scroll: Vec<usize>,
    },
    /// Extract the per-section `$A65C` terrain placements for sections 0-6.
    ///
    /// Prints a summary; `extract` stores the records in the pack. See
    /// `docs/analysis/stage1_data.md`.
    Sections { rom: PathBuf },
    /// Summarise stage 2's spawn and terrain tables.
    ///
    /// Stage 2 is `$19 = 1`: `$A7CA[1]` is `$A7E8`, eight column streams with
    /// 101 `$A65C` terrain placements. See `docs/analysis/stage2.md`.
    Stage2Tables {
        rom: PathBuf,
        /// Write the summary here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Dump the per-stage root table for stages 1-7.
    ///
    /// The stage index is `$19` (0 = stage 1). See `docs/analysis/stages.md`.
    Stages {
        rom: PathBuf,
        /// Write the table here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Decode a five-byte command record the way `$A460` does.
    #[command(name = "command-format")]
    DecodeRecord {
        /// Stage index `$19` (0 = stage 1, 2 = stage 3).
        #[arg(long, default_value_t = 0)]
        stage: u8,
        /// The five stream bytes: `<delay> <$F0+marker> <$65> <$66> <$67>`.
        #[arg(value_parser = parse_kind, num_args = 5)]
        bytes: Vec<u8>,
    },
    /// Decode the object kinds spawned by each stage's column streams.
    ///
    /// Resolves every stream type through `$A5FC`/`$A65C`, resolves the
    /// five-byte command spawns, and prints the `$AE16` handler table. See
    /// `docs/analysis/stage_enemies.md`.
    StageEnemies {
        rom: PathBuf,
        /// Write the table here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Log the oracle's nametable VRAM writes for a frame window.
    ///
    /// The `$0700` queue is empty again by the end of NMI, so this samples the
    /// PPU writes instead. Use it to catch a `$C900` paint/erase/explosion
    /// frame. See `docs/analysis/nametable_queue.md`.
    Nametable {
        rom: PathBuf,
        /// First frame to record (inclusive).
        #[arg(long, default_value_t = 1)]
        from: u64,
        /// Frame after the last recorded frame.
        #[arg(long, default_value_t = 600)]
        to: u64,
        /// Additional timed press, `FRAME:buttons` (e.g. `180:start`).
        #[arg(long = "press", value_name = "FRAME:BUTTONS")]
        press: Vec<String>,
        /// Keep the shield timer (`$46`) at 5 every frame (analysis only).
        #[arg(long)]
        keep_alive: bool,
        /// Write one CPU-RAM byte at the start of a frame,
        /// `FRAME:ADDR:VALUE` (hex for both). Repeatable.
        #[arg(long = "poke", value_name = "FRAME:ADDR:VALUE")]
        poke: Vec<String>,
        /// PPU address window, `LO:HI` (default all nametable writes).
        #[arg(long, default_value = "2000:2FFF")]
        range: String,
        /// Write the log here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Inspect { rom } => inspect(&rom),
        Command::Tiles { rom, out, scale } => tiles(&rom, &out, scale),
        Command::SpriteGroups { rom, out, json } => sprite_groups::run(&rom, &out, json.as_deref()),
        Command::Extract { rom, out } => extract(&rom, &out),
        Command::PowerupShots { pack, out } => powerup::run(&pack, &out),
        Command::Spawn { rom, kinds } => spawn(&rom, &kinds),
        Command::Transition {
            rom,
            script,
            from,
            to,
            keep_alive,
            out,
        } => transition(&rom, &script, from, to, keep_alive, out.as_deref()),
        Command::Shot {
            rom,
            out,
            frame,
            buttons,
            press,
            keep_alive,
        } => shot(&rom, &out, frame, buttons.as_deref(), &press, keep_alive),
        Command::RecordState {
            rom,
            frames,
            buttons,
            press,
            out,
        } => record_state(&rom, frames, buttons.as_deref(), &press, out.as_deref()),
        Command::RecordInput {
            rom,
            frames,
            buttons,
            press,
            out,
            script,
        } => record_input(
            &rom,
            frames,
            buttons.as_deref(),
            &press,
            out.as_deref(),
            script.as_deref(),
        ),
        Command::StateDrift { oracle, native } => state_drift::run(&oracle, &native),
        Command::RecordBoss {
            rom,
            frames,
            stage,
            out,
        } => record_boss(&rom, frames, stage, out.as_deref()),
        Command::BossTrace {
            rom,
            script,
            from,
            to,
            keep_alive,
            out,
        } => boss::trace(&rom, &script, from, to, keep_alive, out.as_deref()),
        Command::RecordEnding {
            rom,
            frames,
            stage,
            out,
            nametable,
        } => ending::run(&rom, frames, stage, out.as_deref(), nametable.as_deref()),
        Command::VerifySprite0 { rom, frames } => verify_sprite0(&rom, frames),
        Command::Trace {
            rom,
            frames,
            top,
            symbols,
            ranges,
            log,
            buttons,
            press,
            assist,
            keep_alive,
        } => trace(
            &rom,
            frames,
            top,
            symbols.as_deref(),
            ranges.as_deref(),
            log,
            buttons.as_deref(),
            &press,
            assist,
            keep_alive,
        ),
        Command::Disasm {
            rom,
            from,
            to,
            covered,
            labels,
            symbols,
            frames,
            buttons,
            press,
            assist,
            keep_alive,
            out,
        } => disassemble(
            &rom,
            from,
            to,
            covered,
            labels,
            symbols.as_deref(),
            frames,
            buttons.as_deref(),
            &press,
            assist,
            keep_alive,
            out.as_deref(),
        ),
        Command::Symbols { merge, out } => symbolize(&merge, &out),
        Command::Ramdump {
            rom,
            frame,
            buttons,
            press,
            range,
            watch,
            top,
            assist,
            keep_alive,
            shield,
            out,
        } => ramdump(
            &rom,
            frame,
            buttons.as_deref(),
            &press,
            range.as_deref(),
            watch.as_deref(),
            top,
            assist,
            keep_alive,
            shield,
            out.as_deref(),
        ),
        Command::Probe {
            rom,
            frame,
            buttons,
            press,
            oam,
            png,
            nametables,
            ram,
            json,
            sprites,
            keep_alive,
            shield,
            poke,
        } => probe(
            &rom,
            frame,
            buttons.as_deref(),
            &press,
            oam.as_deref(),
            png.as_deref(),
            nametables.as_deref(),
            ram.as_deref(),
            json.as_deref(),
            sprites,
            keep_alive,
            shield,
            &poke,
        ),
        Command::Screens {
            rom,
            frames,
            press,
            assist,
            stage,
            png,
            dump,
            sample_scroll,
        } => screens::run(
            &rom,
            frames,
            &press,
            assist,
            &stage,
            png.as_deref(),
            dump.as_deref(),
            &sample_scroll,
        ),
        Command::Sections { rom } => sections::run(&rom),
        Command::Stage2Tables { rom, out } => stage2::run(&rom, out.as_deref()),
        Command::Stages { rom, out } => stages::run(&rom, out.as_deref()),
        Command::DecodeRecord { stage, bytes } => command_format_report(stage, &bytes),
        Command::StageEnemies { rom, out } => stage_enemies::run(&rom, out.as_deref()),
        Command::Nametable {
            rom,
            from,
            to,
            press,
            keep_alive,
            poke,
            range,
            out,
        } => nametable_writes(
            &rom,
            from,
            to,
            &press,
            keep_alive,
            &poke,
            &range,
            out.as_deref(),
        ),
    }
}

pub(crate) fn parse_buttons(spec: &str) -> Result<u8> {
    let mut value = 0u8;
    for name in spec
        .split([',', '+'])
        .map(|part| part.trim().to_ascii_lowercase())
    {
        value |= match name.as_str() {
            "a" => gradius_nes::BUTTON_A,
            "b" => gradius_nes::BUTTON_B,
            "select" => gradius_nes::BUTTON_SELECT,
            "start" => gradius_nes::BUTTON_START,
            "up" => gradius_nes::BUTTON_UP,
            "down" => gradius_nes::BUTTON_DOWN,
            "left" => gradius_nes::BUTTON_LEFT,
            "right" => gradius_nes::BUTTON_RIGHT,
            other => bail!("unknown button `{other}`"),
        };
    }
    Ok(value)
}
