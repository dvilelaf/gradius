//! Recorded oracle fixtures: the cartridge's state lines, checked in so the
//! native port can be tested without a ROM.
//!
//! A fixture is the plain [`Snapshot::to_line`](crate::Snapshot::to_line) dump,
//! one line per frame, preceded by `#` comment lines. It carries no cartridge
//! bytes: the header records the SHA-256 of the source ROM only as provenance.

use std::path::Path;

use crate::dump::Snapshot;

/// The header written above the recorded state lines.
pub fn header(rom_sha256: &str, frames: u64) -> String {
    format!(
        "# gradius-diff oracle fixture\n\
         # {frames} frames of the recorded harness input script\n\
         # state format: gradius_core::Game::dump_state (see crates/gradius-diff/src/dump.rs)\n\
         # source ROM SHA-256: {rom_sha256}\n"
    )
}

/// Write a fixture: the provenance header followed by one state line per frame.
pub fn write_fixture(
    path: &Path,
    rom_sha256: &str,
    frames: u64,
    snapshots: &[Snapshot],
) -> Result<(), String> {
    let mut out = header(rom_sha256, frames);
    if let (Some(first), Some(last)) = (snapshots.first(), snapshots.last()) {
        out.push_str(&format!("# frame range: {}-{}\n", first.frame, last.frame));
    }
    for snapshot in snapshots {
        out.push_str(&snapshot.to_line());
        out.push('\n');
    }
    std::fs::write(path, out).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Read a fixture file. A missing file names the command that restores it.
pub fn read_fixture(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!(
                "missing fixture {}: run scripts/extract-data",
                path.display()
            )
        } else {
            format!("cannot read {}: {error}", path.display())
        }
    })
}

/// Parse a fixture, ignoring comment and blank lines.
pub fn parse_fixture(text: &str) -> Result<Vec<Snapshot>, String> {
    let mut snapshots = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        snapshots
            .push(Snapshot::parse(line).map_err(|error| format!("line {}: {error}", index + 1))?);
    }
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "frame=0 state=Title px=0 pf=0 py=0 pyf=0 speed=0 scroll=0:0:0 score=0 lives=3 capsule=0 meter=0 objects=";

    #[test]
    fn a_fixture_round_trips_and_documents_the_rom_hash() {
        let snapshot = Snapshot::parse(LINE).unwrap();
        let mut text = header("abc123", 1);
        text.push_str(&snapshot.to_line());
        text.push('\n');
        assert!(text.contains("# source ROM SHA-256: abc123"));
        let parsed = parse_fixture(&text).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], snapshot);
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let text = "# header\n\n   # indented\nframe=0 state=Title px=0 pf=0 py=0 pyf=0 speed=0 scroll=0:0:0 score=0 lives=3 capsule=0 meter=0 objects=\n";
        assert_eq!(parse_fixture(text).unwrap().len(), 1);
    }

    #[test]
    fn a_missing_fixture_names_the_command_that_restores_it() {
        let error = read_fixture(Path::new("/nonexistent/gradius.state")).unwrap_err();
        assert_eq!(
            error,
            "missing fixture /nonexistent/gradius.state: run scripts/extract-data"
        );
    }

    #[test]
    fn a_bad_state_line_names_its_line_number() {
        let error = parse_fixture("# header\nframe=x\n").unwrap_err();
        assert!(error.contains("line 2"), "{error}");
    }
}
