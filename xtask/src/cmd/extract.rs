//! `extract`: build the self-contained asset pack.

use std::fs;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};

use crate::stage_data;

use super::shared::load;

pub(crate) fn extract(rom: &Path, out: &Path) -> Result<()> {
    let bytes = fs::read(rom).with_context(|| format!("reading {}", rom.display()))?;
    let started = Instant::now();
    let pack = gradius_extract::extract(&bytes, &mut |step, done, total| {
        if done == total {
            println!("{step:?}: {:.1}s", started.elapsed().as_secs_f32());
        }
    })?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, &pack).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "wrote {} ({} bytes, {} tiles)",
        out.display(),
        pack.len(),
        gradius_pack::TILE_BANKS * 256
    );
    println!("the pack is self-contained: it does not read the ROM at runtime");
    let cart = load(rom)?;
    let stage = stage_data::decode(&cart.prg)?;
    println!(
        "stage tables: {} ({} bytes at $CE2B), {} ({} bytes at $CF27), {} ({} bytes at $CF48), {} ({} bytes at $CF90), {} ({} bytes at $BB7C)",
        gradius_assets::STAGE_WALL_ERASE,
        stage.wall_erase().len(),
        gradius_assets::ENDING_TEXT_RECORD,
        stage.ending_text_record().len(),
        gradius_assets::ENDING_SCREEN_STREAM,
        stage.ending_screen_stream().len(),
        gradius_assets::ENDING_SCREEN_CLASSES,
        stage.ending_screen_classes().len(),
        gradius_assets::ENDING_SHIP_MOTION,
        stage.ending_motion().len(),
    );
    println!(
        "ending class tables: {} ({} bytes at $9FD6), {} ({} bytes at $D6F2), {} ({} bytes at $D772)",
        gradius_assets::ENDING_CLASS_POINTERS,
        stage.ending_class_pointers().len(),
        gradius_assets::ENDING_CLASS_ATTRIBUTES,
        stage.ending_class_attributes().len(),
        gradius_assets::ENDING_CLASS_TILES,
        stage.ending_class_tiles().len(),
    );
    Ok(())
}
