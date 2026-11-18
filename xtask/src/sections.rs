//! Section decoding lives in `gradius-extract`; this file is the CLI summary.

use std::path::Path;

use anyhow::Result;

pub use gradius_extract::sections::*;

/// `cargo xtask sections <rom>`: a summary of the decoded sections.
pub fn run(rom: &Path) -> Result<()> {
    let cart = crate::load(rom)?;
    let sections = decode(&cart.prg)?;
    for (section, records) in sections.terrain.iter().enumerate() {
        println!(
            "section {section}: threshold ${:02X}, {} terrain records",
            sections.thresholds[section],
            records.len()
        );
    }
    Ok(())
}
