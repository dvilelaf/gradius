//! Per-frame simulation: the play loop, hazards, bosses, firing and scoring.
//!
//! Split by responsibility in card R1n into [`hazards`] (the `$C40D`/`$C433`
//! stage scripts, the `$A460` command path and the boss lifecycle) and
//! [`step`] (the winning frame pipeline, enemy fire, scoring, the state dump
//! and the effects). Every function keeps its name, its body and the order it
//! had in `frame.rs`.

use super::*;

// `game.rs` declares this module with `#[path = "frame.rs"]`, so the split
// files under `frame/` carry their own paths (card R1n) instead of `mod x;`.

/// `$C40D`/`$C433` stage scripts, commands and the bosses.
#[path = "frame/hazards.rs"]
mod hazards;

/// The per-frame step loop, enemy fire, scoring, state dump and effects.
#[path = "frame/step.rs"]
mod step;
