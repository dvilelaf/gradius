//! Frame-by-frame comparison of the two state lines.
//!
//! Only frames where **both** sides report `Playing` are compared. The port
//! zeroes its simulation fields outside play and the original spends its first
//! frames in title/attract states, so comparing those frames measures the
//! transition, not the simulation. Everything excluded is counted and reported
//! as the window's skipped frames.
//!
//! Comparison rules inside the window:
//!
//! * `speed`, `scroll` (`$3F:$3E:$3D`), `score`, `lives` and `meter` must
//!   match exactly.
//! * `px`/`py` compare the full 8.8 fixed-point position (integer byte plus
//!   sub-pixel byte) against `--tolerance` whole pixels.
//! * `capsule` is not compared: the original keeps no capsule counter, see the
//!   crate docs.
//! * Objects match by slot; `kind` and `substate` are exact, `x`/`y` use the
//!   same pixel tolerance. A slot present on only one side is a divergence.

mod frames;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::dump::{Snapshot, SnapshotObject};
pub use frames::{compare, compare_frame};

use frames::{format_rate, format_thousandths, json_rate, json_string, rate_thousandths};

/// Comparison settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompareConfig {
    /// Allowed absolute difference for positions, in whole pixels.
    pub tolerance_px: i64,
    /// Restrict comparison to this inclusive frame range. When set, every
    /// frame in the range is compared even if a side has left `Playing` (for
    /// example during the stage-clear transition). When unset, only frames
    /// where both sides are `Playing` are compared.
    pub window: Option<(u64, u64)>,
    /// Compare the object table. Analysis scenarios that inject a capsule into
    /// different free slots on each side turn this off and check the ship and
    /// power meter only.
    pub compare_objects: bool,
}

impl Default for CompareConfig {
    fn default() -> Self {
        Self {
            tolerance_px: 1,
            window: None,
            compare_objects: true,
        }
    }
}

/// One mismatched field on one frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    pub frame: u64,
    pub field: String,
    pub oracle: String,
    pub native: String,
}

/// How many checks one field received and how many of them passed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FieldStat {
    pub checks: u64,
    pub passes: u64,
}

impl FieldStat {
    /// Pass rate in thousandths, or `None` when the field was never checked.
    pub fn rate(&self) -> Option<i64> {
        rate_thousandths(self.passes, self.checks)
    }
}

/// Absolute position error accumulated over the run, in 1/256 px units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta {
    pub max: i64,
    pub sum: i64,
}

impl Delta {
    fn add(&mut self, fixed: i64) {
        self.max = self.max.max(fixed);
        self.sum += fixed;
    }

    /// Largest error in thousandths of a pixel.
    pub fn max_thousandths(&self) -> i64 {
        self.max * 1000 / 256
    }

    /// Mean error in thousandths of a pixel, over `frames` compared frames.
    pub fn mean_thousandths(&self, frames: u64) -> i64 {
        if frames == 0 {
            0
        } else {
            let count = frames as i64;
            (self.sum * 1000 + count * 128) / (count * 256)
        }
    }
}

/// Every field [`compare_frame`] can check, in report order.
const COMPARED_FIELDS: [&str; 13] = [
    "frame",
    "px",
    "py",
    "speed",
    "scroll",
    "score",
    "lives",
    "meter",
    "obj.slot",
    "obj.kind",
    "obj.x",
    "obj.y",
    "obj.substate",
];

/// The outcome of a whole run.
#[derive(Clone, Debug)]
pub struct Report {
    /// Frames offered to the comparator, Playing or not.
    pub frames: u64,
    /// The explicit frame range from [`CompareConfig::window`], if any.
    pub requested_window: Option<(u64, u64)>,
    /// Frames that were compared (both `Playing`, or inside an explicit window).
    pub compared: u64,
    /// First and last compared frame number, if any were compared.
    pub window_first: Option<u64>,
    pub window_last: Option<u64>,
    /// Excluded frames where the two sides disagreed about the state.
    pub state_mismatch_frames: u64,
    pub tolerance_px: i64,
    /// Every divergence inside the window, in frame order.
    pub divergences: Vec<Divergence>,
    /// Check and pass count per compared field.
    pub fields: BTreeMap<String, FieldStat>,
    /// Live-object delta (oracle minus native) on the last compared frame.
    pub object_count_delta: i64,
    /// Largest absolute live-object delta seen.
    pub max_object_count_delta: i64,
    /// Frames where the two live-object counts differed.
    pub object_count_mismatch_frames: u64,
    /// Object slots present on both sides, over the whole run.
    pub object_slot_matches: u64,
    /// Object slots present on either side, over the whole run.
    pub object_slot_pairs: u64,
    /// Ship X/Y error over the compared frames.
    pub px_delta: Delta,
    pub py_delta: Delta,
}

impl Report {
    /// True when the run compared at least one frame and nothing diverged.
    pub fn within_tolerance(&self) -> bool {
        self.compared > 0 && self.divergences.is_empty()
    }

    /// The first divergence, if any.
    pub fn first(&self) -> Option<&Divergence> {
        self.divergences.first()
    }

    /// One-line verdict.
    pub fn result(&self) -> &'static str {
        if self.compared == 0 {
            if self.requested_window.is_some() {
                "NO COMPARED FRAMES"
            } else {
                "NO PLAYING FRAMES"
            }
        } else if self.divergences.is_empty() {
            "WITHIN TOLERANCE"
        } else {
            "DIVERGES"
        }
    }

    /// Object slot match rate in thousandths, or `None` without any slot.
    pub fn object_slot_rate(&self) -> Option<i64> {
        rate_thousandths(self.object_slot_matches, self.object_slot_pairs)
    }

    /// How many checks of `field` failed over the run.
    pub fn failures(&self, field: &str) -> u64 {
        self.fields
            .get(field)
            .map_or(0, |stat| stat.checks - stat.passes)
    }

    /// Human-readable report: the window, a per-field pass-rate table and the
    /// position/object summaries.
    pub fn summary(&self) -> String {
        let window = match (self.requested_window, self.window_first, self.window_last) {
            (Some((from, to)), _, _) => format!(" (window {from}-{to})"),
            (None, Some(first), Some(last)) => format!(" (Playing frames {first}-{last})"),
            _ => " (no Playing frames)".to_string(),
        };
        let mut out = format!(
            "gradius-diff: {} frames run, {} compared{}, tolerance {} px\n",
            self.frames, self.compared, window, self.tolerance_px
        );
        let skipped = match self.requested_window {
            Some((from, to)) => format!("requested window {from}-{to}"),
            None => "Playing window".to_string(),
        };
        out.push_str(&format!(
            "skipped: {} frames outside the {} ({} with differing states)\n",
            self.frames - self.compared,
            skipped,
            self.state_mismatch_frames
        ));
        out.push_str(&format!(
            "result: {} ({} divergences)\n",
            self.result(),
            self.divergences.len()
        ));
        out.push_str(&format!("\n{:<14} {:>8} {:>7}\n", "field", "pass", "rate"));
        for (field, stat) in &self.fields {
            out.push_str(&format!(
                "{:<14} {:>8} {:>7}\n",
                field,
                format!("{}/{}", stat.passes, stat.checks),
                format_rate(stat.rate())
            ));
        }
        out.push('\n');
        match self.first() {
            Some(first) => out.push_str(&format!(
                "first divergence: frame {} field {} (oracle {}, native {})\n",
                first.frame, first.field, first.oracle, first.native
            )),
            None => out.push_str("first divergence: none\n"),
        }
        out.push_str(&format!(
            "position error: px max {} mean {}, py max {} mean {}\n",
            format_thousandths(self.px_delta.max_thousandths()),
            format_thousandths(self.px_delta.mean_thousandths(self.compared)),
            format_thousandths(self.py_delta.max_thousandths()),
            format_thousandths(self.py_delta.mean_thousandths(self.compared)),
        ));
        out.push_str(&format!(
            "objects: slot match {}/{} ({}), live count delta last {} max {} on {} frames\n",
            self.object_slot_matches,
            self.object_slot_pairs,
            format_rate(self.object_slot_rate()),
            self.object_count_delta,
            self.max_object_count_delta,
            self.object_count_mismatch_frames
        ));
        out
    }

    /// [`Report::summary`] plus the first ten divergences.
    pub fn detailed(&self) -> String {
        let mut out = self.summary();
        if !self.divergences.is_empty() {
            out.push_str("first divergences:\n");
            out.push_str(&format!(
                "  {:<14} {:<16} {:<16} {}\n",
                "frame", "field", "oracle", "native"
            ));
            for divergence in self.divergences.iter().take(10) {
                out.push_str(&format!(
                    "  {:<14} {:<16} {:<16} {}\n",
                    divergence.frame, divergence.field, divergence.oracle, divergence.native
                ));
            }
        }
        out
    }

    /// The same summary as a JSON object.
    pub fn to_json(&self) -> String {
        let fields: Vec<String> = self
            .fields
            .iter()
            .map(|(field, stat)| {
                format!(
                    "    {}: {{ \"checks\": {}, \"passes\": {}, \"rate\": {} }}",
                    json_string(field),
                    stat.checks,
                    stat.passes,
                    json_rate(stat.rate())
                )
            })
            .collect();
        let fields = format!("  \"fields\": {{\n{}\n  }},\n", fields.join(",\n"));
        let first = match self.first() {
            Some(first) => format!(
                "  \"first_divergence\": {{ \"frame\": {}, \"field\": {}, \"oracle\": {}, \"native\": {} }},\n",
                first.frame,
                json_string(&first.field),
                json_string(&first.oracle),
                json_string(&first.native)
            ),
            None => "  \"first_divergence\": null,\n".to_string(),
        };
        let mut out = String::from("{\n");
        out.push_str(&format!("  \"frames\": {},\n", self.frames));
        out.push_str(&format!("  \"compared\": {},\n", self.compared));
        out.push_str(&format!(
            "  \"window\": {{ \"first\": {}, \"last\": {}, \"skipped\": {}, \"state_mismatch\": {} }},\n",
            self.window_first
                .map_or("null".to_string(), |frame| frame.to_string()),
            self.window_last
                .map_or("null".to_string(), |frame| frame.to_string()),
            self.frames - self.compared,
            self.state_mismatch_frames
        ));
        out.push_str(&format!("  \"tolerance_px\": {},\n", self.tolerance_px));
        out.push_str(&format!("  \"result\": {},\n", json_string(self.result())));
        out.push_str(&format!("  \"divergences\": {},\n", self.divergences.len()));
        out.push_str(&fields);
        out.push_str(&first);
        out.push_str(&format!(
            "  \"position\": {{\n    \"px\": {{ \"max\": {}, \"mean\": {} }},\n    \"py\": {{ \"max\": {}, \"mean\": {} }}\n  }},\n",
            format_thousandths(self.px_delta.max_thousandths()),
            format_thousandths(self.px_delta.mean_thousandths(self.compared)),
            format_thousandths(self.py_delta.max_thousandths()),
            format_thousandths(self.py_delta.mean_thousandths(self.compared)),
        ));
        out.push_str(&format!(
            "  \"objects\": {{ \"matches\": {}, \"pairs\": {}, \"rate\": {}, \"last_delta\": {}, \"max_delta\": {}, \"mismatch_frames\": {} }}\n",
            self.object_slot_matches,
            self.object_slot_pairs,
            json_rate(self.object_slot_rate()),
            self.object_count_delta,
            self.max_object_count_delta,
            self.object_count_mismatch_frames
        ));
        out.push_str("}\n");
        out
    }

    /// Record one field check; returns true when the field passed.
    fn record(&mut self, field: &str, pass: bool) -> bool {
        let stat = self.fields.entry(field.to_string()).or_default();
        stat.checks += 1;
        if pass {
            stat.passes += 1;
        }
        pass
    }
}
