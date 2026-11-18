//! Report contiguous runs where two per-frame state dumps disagree on `state=`.
//!
//! `gradius-diff --oracle-out` and `--native-out` write one line per frame in
//! `Game::dump_state` order. Only the `frame=` and `state=` fields are read;
//! field drift is `gradius-diff`'s job. This command turns the state-only
//! mismatches into the frame ranges a root-cause note can cite.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

/// One contiguous run of frames whose `state=` fields differ.
#[derive(Debug, PartialEq, Eq)]
pub struct Run {
    pub start: u64,
    pub end: u64,
    pub oracle: String,
    pub native: String,
}

/// Print every state-only run, then the frame and run counts.
pub fn run(oracle: &Path, native: &Path) -> Result<()> {
    let oracle =
        std::fs::read_to_string(oracle).with_context(|| format!("reading {}", oracle.display()))?;
    let native =
        std::fs::read_to_string(native).with_context(|| format!("reading {}", native.display()))?;
    let runs = runs(&oracle, &native)?;
    let mut frames = 0;
    for run in &runs {
        frames += run.end - run.start + 1;
        println!(
            "{}-{} ({}): oracle={} native={}",
            run.start,
            run.end,
            run.end - run.start + 1,
            run.oracle,
            run.native
        );
    }
    println!("{frames} state-mismatch frame(s) in {} run(s)", runs.len());
    Ok(())
}

/// Collect the contiguous state-only runs from two dump texts.
pub fn runs(oracle: &str, native: &str) -> Result<Vec<Run>> {
    let oracle = states(oracle)?;
    let native = states(native)?;
    if oracle.len() != native.len() {
        bail!(
            "dumps cover different frame counts: {} oracle, {} native",
            oracle.len(),
            native.len()
        );
    }
    let mut runs: Vec<Run> = Vec::new();
    for (&frame, oracle_state) in &oracle {
        let Some(native_state) = native.get(&frame) else {
            bail!("native dump is missing frame {frame}");
        };
        if oracle_state == native_state {
            continue;
        }
        match runs.last_mut() {
            Some(run)
                if run.end + 1 == frame
                    && run.oracle == *oracle_state
                    && run.native == *native_state =>
            {
                run.end = frame;
            }
            _ => runs.push(Run {
                start: frame,
                end: frame,
                oracle: oracle_state.clone(),
                native: native_state.clone(),
            }),
        }
    }
    Ok(runs)
}

fn states(text: &str) -> Result<BTreeMap<u64, String>> {
    let mut out = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let mut frame = None;
        let mut state = None;
        for field in line.split_whitespace() {
            if let Some(value) = field.strip_prefix("frame=") {
                frame = Some(
                    value
                        .parse()
                        .with_context(|| format!("line {}: bad frame `{value}`", index + 1))?,
                );
            } else if let Some(value) = field.strip_prefix("state=") {
                state = Some(value.to_string());
            }
        }
        let (Some(frame), Some(state)) = (frame, state) else {
            bail!("line {}: missing `frame=` or `state=`", index + 1);
        };
        out.insert(frame, state);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORACLE: &str = "\
frame=1 state=Title px=0
frame=2 state=Other(1) px=80
frame=3 state=Other(1) px=80
frame=4 state=Other(2) px=80
frame=5 state=Title px=80
";
    const NATIVE: &str = "\
frame=1 state=Title px=0
frame=2 state=Title px=80
frame=3 state=Title px=80
frame=4 state=Other(2) px=80
frame=5 state=Other(2) px=80
";

    #[test]
    fn reports_contiguous_state_runs() {
        assert_eq!(
            runs(ORACLE, NATIVE).unwrap(),
            vec![
                Run {
                    start: 2,
                    end: 3,
                    oracle: "Other(1)".to_string(),
                    native: "Title".to_string(),
                },
                Run {
                    start: 5,
                    end: 5,
                    oracle: "Title".to_string(),
                    native: "Other(2)".to_string(),
                },
            ]
        );
    }

    #[test]
    fn identical_states_have_no_runs() {
        assert!(runs(ORACLE, ORACLE).unwrap().is_empty());
    }

    #[test]
    fn unequal_frame_counts_are_an_error() {
        let error = runs(ORACLE, "frame=1 state=Title px=0\n").unwrap_err();
        assert!(
            error.to_string().contains("different frame counts"),
            "{error}"
        );
    }
}
