//! Rewind check: replay a span from a save image and compare the result with
//! the continuous run.
//!
//! `--rewind-at FRAME` makes the harness snapshot the native port at FRAME and
//! keep running to FRAME + `--rewind-span`; it then restores the image and
//! replays the same inputs. [`check`] does the restore and replay, and
//! [`compare_states`] names the state-line fields that differ from the
//! continuous run's final line.

use gradius_core::{Buttons, Game};

/// One `key=value` field that differs between two state lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDiff {
    /// The field's key, `objects` for the whole object table.
    pub field: String,
    /// The continuous run's value.
    pub continuous: String,
    /// The replayed run's value.
    pub replayed: String,
}

/// The comparison of a continuous state line with a replayed one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateDiff {
    /// The differing fields, in state-line order.
    pub differences: Vec<FieldDiff>,
}

impl StateDiff {
    /// Whether the replayed state line is exactly the continuous one.
    pub fn is_identical(&self) -> bool {
        self.differences.is_empty()
    }

    /// One `field: continuous=... replayed=...` line per difference.
    pub fn report(&self) -> String {
        self.differences
            .iter()
            .map(|diff| {
                format!(
                    "  {}: continuous={} replayed={}",
                    diff.field, diff.continuous, diff.replayed
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Restore `snapshot` into `game`, step `inputs`, and compare the resulting
/// state line with `continuous`.
pub fn check(
    game: &mut Game,
    snapshot: &[u8],
    inputs: &[Buttons],
    continuous: &str,
) -> Result<StateDiff, String> {
    game.restore(snapshot)
        .map_err(|error| format!("cannot restore snapshot: {error:?}"))?;
    for input in inputs {
        game.step(*input);
    }
    Ok(compare_states(continuous, &game.dump_state()))
}

/// Compare two `Game::dump_state` lines token by token.
pub fn compare_states(continuous: &str, replayed: &str) -> StateDiff {
    let continuous: Vec<&str> = continuous.split_whitespace().collect();
    let replayed: Vec<&str> = replayed.split_whitespace().collect();
    let mut differences = Vec::new();
    for (left, right) in continuous.iter().zip(&replayed) {
        if left != right {
            differences.push(FieldDiff {
                field: field_name(left),
                continuous: field_value(left).to_string(),
                replayed: field_value(right).to_string(),
            });
        }
    }
    if continuous.len() != replayed.len() {
        differences.push(FieldDiff {
            field: "fields".to_string(),
            continuous: continuous.len().to_string(),
            replayed: replayed.len().to_string(),
        });
    }
    StateDiff { differences }
}

/// The key of a `key=value` token; the whole token if it has no `=`.
fn field_name(token: &str) -> String {
    token
        .split_once('=')
        .map_or(token, |(key, _)| key)
        .to_string()
}

/// The value of a `key=value` token; the whole token if it has no `=`.
fn field_value(token: &str) -> &str {
    token.split_once('=').map_or(token, |(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "frame=1200 state=Playing px=100 pf=0 py=50 pyf=128 \
        speed=3 scroll=4:0:0 score=300 lives=3 capsule=1 meter=2 \
        objects=1:5:10:20:0;9:9:30:40:2";

    #[test]
    fn an_identical_line_has_no_differences() {
        let diff = compare_states(LINE, LINE);
        assert!(diff.is_identical());
        assert_eq!(diff.report(), "");
    }

    #[test]
    fn differing_fields_are_named_with_both_values() {
        let replayed = LINE.replace("score=300", "score=301").replace(
            "objects=1:5:10:20:0;9:9:30:40:2",
            "objects=1:5:10:20:0;9:9:30:41:2",
        );
        let diff = compare_states(LINE, &replayed);
        assert!(!diff.is_identical());
        let fields: Vec<&str> = diff
            .differences
            .iter()
            .map(|diff| diff.field.as_str())
            .collect();
        assert_eq!(fields, ["score", "objects"]);
        assert_eq!(diff.differences[0].continuous, "300");
        assert_eq!(diff.differences[0].replayed, "301");
        assert!(diff.report().contains("objects: continuous="));
    }

    #[test]
    fn a_missing_field_is_a_difference() {
        let diff = compare_states(LINE, &LINE.replace(" meter=2", ""));
        assert_eq!(diff.differences.last().unwrap().field, "fields");
    }
}
