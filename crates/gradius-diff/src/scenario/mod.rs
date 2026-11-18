//! Oracle-driven user-action scenarios.
//!
//! `gradius-scenarios` drives the cartridge and the native port from reset with
//! the same real button script, then reports the first state divergence, the
//! sampled pixel match and a small set of oracle-side invariants for the user
//! flow being exercised (menu Select edges, real capsule pickups, B taps, the
//! eruption wave, the boss gate and the 2P handoff). Scenario scripts are
//! listed by the table in [`SCENARIOS`]; most live in `scripts/scenarios/`. A
//! `p2`-prefixed script line drives the second controller, which the oracle
//! reads from `$4017`; the native port still receives P1 only until the
//! two-player runtime cards land, so [`Check::Handoff`] is a known scenario
//! whose documented divergence is bounded to the measured drift.
//!
//! The menu and power scenarios use only the pad. The real capsule route buys
//! SPEED five times (fixture-backed); the controlled `$40` range measurement
//! lives in `tests/speed_max.rs` because it needs held B and a forced `$42`
//! write without the scenario runner's frame-offset bookkeeping.

mod checks;
mod run;
mod scenarios;

use std::path::PathBuf;

use crate::compare::Report;
use crate::dump::Snapshot;
use crate::pixel::{PixelSummary, PixelWindow};

use gradius_core::PowerMeter;
use gradius_nes::Emulator;

pub use run::run;
pub use scenarios::SCENARIOS;

/// Directory holding the scenario scripts, relative to the repository root.
pub const SCRIPT_DIR: &str = "scripts/scenarios";

/// Whether a scenario is expected to match the port exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    /// A hard gate: any divergence fails the run.
    Clean,
    /// A documented mismatch: the run may diverge, but only inside the
    /// measured budget, so a regression that diverges earlier or in more
    /// fields still fails.
    Known {
        /// Earliest accepted first-divergence frame.
        first_frame: u64,
        /// Most accepted divergences over the run.
        max_divergences: usize,
    },
}

impl Expected {
    pub fn label(self) -> &'static str {
        match self {
            Expected::Clean => "clean",
            Expected::Known { .. } => "known",
        }
    }

    /// Whether a run stays inside what this expectation accepts. A known row
    /// that stops diverging is accepted too, so it can be promoted to a gate.
    pub fn accepts(self, report: &Report) -> bool {
        match self {
            Expected::Clean => report.within_tolerance(),
            Expected::Known {
                first_frame,
                max_divergences,
            } => match report.first() {
                None => true,
                Some(first) => {
                    first.frame >= first_frame && report.divergences.len() <= max_divergences
                }
            },
        }
    }
}

/// The power-meter part of a scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerPlan {
    /// Meter slot being bought (1..=6).
    pub slot: u8,
    /// Real B taps written in the scenario script, in frame order.
    pub taps: &'static [u64],
    /// Successful activations expected.
    pub buys: usize,
    /// `$42` after the run: 0 after a successful buy, the selected slot on a
    /// no-op.
    pub final_selection: u8,
    /// Hold B from this frame (the hold-vs-tap scenario).
    pub hold_from: Option<u64>,
}

/// What the oracle-side check for a scenario must prove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// Title menu: Select toggles `$0F`; Up/Down do not; Start locks `$03`.
    Menu {
        /// `$0F` after the Select at 180 (0 or 1).
        selection: u8,
        /// `$03 & $70` after Start at 320.
        start_lock: u8,
    },
    /// Start-only menu: no Select edge, `$0F` stays on 1P, Start locks `$03`.
    MenuStart {
        /// Frame of the scripted Start. The ROM shows `$00 = 3` one frame later.
        start_frame: u64,
        /// `$03 & $70` after Start.
        start_lock: u8,
    },
    /// A power-up purchase and its effect.
    Power(PowerPlan),
    /// Stage-1 eruption: kind `$8A` objects and the scroll hold.
    Eruption,
    /// Stage-1 boss gate: Big Core spawn/kill and the `$5F`/`$39` volcano gate.
    BossGate,
    /// 2P relay: P2 Start is inert during P1's play, P1's death hands `$18`
    /// to P2 and P2's pad then moves the ship.
    Handoff,
    /// Start pauses and resumes the stage (`$15`).
    Pause,
    /// The Konami code in the pause gives the demo loadout once (`$46`).
    PauseCode,
    /// Start pauses inside the `$82` checkpoint wait (`$1B = $82`).
    PauseTransition,
}

/// One scenario in the matrix.
pub struct Scenario {
    pub name: &'static str,
    /// Script path relative to the repository root.
    pub script: &'static str,
    pub frames: u64,
    /// State comparison window. `None` compares the Playing window.
    pub window: Option<(u64, u64)>,
    /// Analysis-only keep-alive poke on the oracle.
    pub keep_alive: bool,
    /// Analysis-only keep-alive on the port.
    pub native_keep_alive: bool,
    /// Compare the object table (off for injected capsules, which land in
    /// different free slots on the two sides).
    pub compare_objects: bool,
    /// Pixel sample range; `None` disables the pixel report.
    pub pixel_window: Option<(u64, u64)>,
    /// Sample every N frames inside `pixel_window`.
    pub pixel_every: u64,
    /// Always write oracle/native PNG pairs on these frames.
    pub png_frames: &'static [u64],
    pub expected: Expected,
    pub check: Check,
}

impl Scenario {
    pub fn script_path(&self) -> PathBuf {
        repo_root().join(self.script)
    }

    pub fn pixel_window(&self) -> Option<PixelWindow> {
        self.pixel_window.map(|(from, to)| PixelWindow { from, to })
    }
}

/// `$0F`, `$00`, `$03`, `$40`..`$46`, `$5F`, `$39`: the oracle bytes the
/// checks need. Indexed by [`Ram::*`]. The 2P handoff check also needs `$18`,
/// `$1B`, `$20`/`$21` and P2's `$06`/`$08`.
#[derive(Clone, Copy, Debug, Default)]
struct Ram {
    flags: [u8; 12],
    /// `$18`, the current player.
    player: u8,
    /// `$1B`, the stage state.
    stage: u8,
    /// `$20`/`$21`, P1/P2 lives.
    lives: [u8; 2],
    /// P2's edge (`$06`) and held (`$08`) bytes.
    p2_input: [u8; 2],
}

impl Ram {
    const SELECT: usize = 0;
    const STATE: usize = 1;
    const LOCK: usize = 2;
    const SPEED: usize = 3;
    const MISSILE: usize = 4;
    const METER: usize = 5;
    const WEAPON: usize = 6;
    const OPTION: usize = 7;
    const SHIELD: usize = 8;
    const VOLCANO: usize = 9;
    const GATE: usize = 10;
    const PAUSE: usize = 11;

    fn read(emulator: &Emulator) -> Self {
        let ram = emulator.ram_snapshot();
        Self {
            flags: [
                ram[0x0F], ram[0x00], ram[0x03], ram[0x40], ram[0x41], ram[0x42], ram[0x44],
                ram[0x45], ram[0x46], ram[0x5F], ram[0x39], ram[0x15],
            ],
            player: ram[0x18],
            stage: ram[0x1B],
            lives: [ram[0x20], ram[0x21]],
            p2_input: [ram[0x06], ram[0x08]],
        }
    }

    fn get(&self, index: usize) -> u8 {
        self.flags[index]
    }
}

/// The port's power state, for the same effect comparison.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NativePower {
    selection: u8,
    speed: u8,
    missile: u8,
    weapon: u8,
    options: u8,
    shield: u8,
}

impl NativePower {
    fn read(power: &PowerMeter) -> Self {
        Self {
            selection: power.selection(),
            speed: power.speed_level(),
            missile: u8::from(power.has_missile()),
            weapon: power.weapon_level(),
            options: power.options(),
            shield: power.shield_hits(),
        }
    }
}

/// One named assertion with evidence.
#[derive(Clone, Debug)]
pub struct CheckLine {
    pub name: String,
    pub pass: bool,
    /// Whether a failure is a hard gate. The known `$40` wrap scenario marks
    /// its port-parity checks soft because the mismatch is the measurement.
    pub hard: bool,
    pub detail: String,
}

/// Everything one scenario produced.
pub struct ScenarioResult {
    pub name: &'static str,
    pub expected: Expected,
    pub report: Report,
    pub pixels: PixelSummary,
    pub checks: Vec<CheckLine>,
    pub notes: Vec<String>,
    pub oracle_lines: Vec<Snapshot>,
    pub native_lines: Vec<Snapshot>,
    /// The scenario passes when every oracle check passes and a clean
    /// scenario has no state divergence.
    pub passed: bool,
}

impl ScenarioResult {
    pub fn first_divergence(&self) -> String {
        match self.report.first() {
            Some(divergence) => format!(
                "f{} {} (oracle {}, native {})",
                divergence.frame, divergence.field, divergence.oracle, divergence.native
            ),
            None => "none".to_string(),
        }
    }

    pub fn check_passed(&self) -> bool {
        self.checks.iter().all(|check| !check.hard || check.pass)
    }
}

/// Repository root from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
