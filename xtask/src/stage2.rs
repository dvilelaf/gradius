//! Stage-2 table decoding lives in `gradius-extract`; this file is the CLI.

use std::fs;
use std::path::Path;

use anyhow::Result;

pub use gradius_extract::stage2::*;

/// `cargo xtask stage2-tables <rom> [--out summary.txt]`.
pub fn run(rom: &Path, out: Option<&Path>) -> Result<()> {
    let cart = crate::load(rom)?;
    let text = render(&decode(&cart.prg)?);
    match out {
        Some(path) => {
            fs::write(path, &text)?;
            eprintln!("wrote {}", path.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}
