use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::{capture, hex, write_png};

/// `xtask screens` entry point.
#[allow(clippy::too_many_arguments)]
pub fn run(
    rom: &Path,
    frames: u64,
    presses: &[String],
    assist: bool,
    stage_numbers: &[u8],
    png: Option<&Path>,
    dump: Option<&Path>,
    sample_scrolls: &[usize],
) -> Result<()> {
    let requested = if stage_numbers.is_empty() {
        vec![1]
    } else {
        stage_numbers.to_vec()
    };
    let combined = requested.len() > 1;
    if combined && requested.contains(&1) {
        bail!("combined extraction supports stages 2-7 only");
    }
    if combined && (png.is_some() || dump.is_some()) {
        bail!("--png and --dump need a single --stage");
    }
    if combined && !sample_scrolls.is_empty() {
        bail!("--sample-scroll needs a single --stage");
    }

    let cart = crate::load(rom)?;
    let plan = crate::input_plan(None, presses)?;
    if combined {
        for number in &requested {
            let index = stage_index(*number)?;
            let (_title, screen, _classify, builder) =
                capture(&cart, frames, &plan, assist, true, index, &[])?;
            let validation = match (&builder.sample_vram, builder.sample_scroll) {
                (Some(vram), Some(scroll)) => builder.validate_visible(&screen, vram, scroll),
                _ => (0, 0),
            };
            println!(
                "stage {number}: {} x {} tiles ({} px), scroll {}..{} px, {} restarts, missing tiles {}, missing attrs {}",
                screen.width_tiles,
                screen.height_tiles,
                screen.width_tiles * 8,
                builder.first_scroll,
                builder.max_scroll,
                builder.resets,
                screen.missing_tiles,
                screen.missing_attributes
            );
            println!(
                "       chr bank {}, tiles fnv {:016X} attrs fnv {:016X}",
                screen.chr_bank,
                screen.fnv_tiles(),
                screen.fnv_attributes()
            );
            println!(
                "       oracle nametable sample at {} px: {} tile / {} attribute mismatches",
                builder.sample_scroll.unwrap_or(0),
                validation.0,
                validation.1
            );
            if validation.0 != 0 || validation.1 != 0 {
                eprintln!(
                    "warning: stage {number} extraction disagrees with the oracle nametable sample"
                );
            }
        }
        return Ok(());
    }

    let stage_number = requested[0];
    let index = stage_index(stage_number)?;
    let (title, stage, stage_classify, builder) =
        capture(&cart, frames, &plan, assist, true, index, sample_scrolls)?;
    let validation = match (&builder.sample_vram, builder.sample_scroll) {
        (Some(vram), Some(scroll)) => builder.validate_visible(&stage, vram, scroll),
        _ => (0, 0),
    };

    let dominant = stage.palette;
    println!(
        "title  {} x {} tiles",
        title.width_tiles, title.height_tiles
    );
    println!(
        "       palette {} chr bank {} tiles fnv {:016X} attrs fnv {:016X}",
        hex(&title.palette),
        title.chr_bank,
        title.fnv_tiles(),
        title.fnv_attributes()
    );
    println!(
        "stage  {} x {} tiles ({} px), {} tile writes, {} attr writes, {} palette writes",
        stage.width_tiles,
        stage.height_tiles,
        stage.width_tiles * 8,
        builder.tile_writes,
        builder.attribute_writes,
        builder.palette_writes
    );
    println!(
        "       scroll {}..{} px (last {}), {} restarts, missing tiles {}, missing attrs {}",
        builder.first_scroll,
        builder.max_scroll,
        builder.last_scroll,
        builder.resets,
        stage.missing_tiles,
        stage.missing_attributes
    );
    println!(
        "       chr banks {:?}, palette {}",
        builder.chr_banks,
        hex(&dominant)
    );
    println!(
        "       tiles fnv {:016X} attrs fnv {:016X}",
        stage.fnv_tiles(),
        stage.fnv_attributes()
    );
    println!(
        "       oracle nametable sample at {} px: {} tile / {} attribute mismatches",
        builder.sample_scroll.unwrap_or(0),
        validation.0,
        validation.1
    );
    if validation.0 != 0 || validation.1 != 0 {
        eprintln!("warning: extraction disagrees with the oracle nametable sample");
    }
    for sample in &builder.samples {
        let (tiles, attributes) = builder.validate_visible(&stage, &sample.vram, sample.scroll);
        let palette = if sample.palette == stage.palette {
            "dominant".to_string()
        } else {
            format!("differs ({})", hex(&sample.palette))
        };
        println!(
            "       sample at {} px (requested {}): {} tile / {} attribute mismatches, chr bank {}, palette {}",
            sample.scroll, sample.requested, tiles, attributes, sample.chr_bank, palette
        );
        if tiles != 0 || attributes != 0 {
            eprintln!(
                "warning: extraction disagrees with the oracle at {} px",
                sample.scroll
            );
        }
    }
    if stage_number == 2 && !sample_scrolls.is_empty() {
        let tables = crate::stage2::decode(&cart.prg)?;
        let check = builder.check_terrain(&tables.columns, tables.boss);
        println!(
            "       terrain at runtime: {} placements, {} record / {} position mismatches, {} missing, max {} px past threshold",
            check.consumed,
            check.record_mismatches,
            check.position_mismatches,
            check.missing,
            check.max_lag
        );
        if check.record_mismatches != 0 || check.position_mismatches != 0 || check.missing != 0 {
            eprintln!("warning: runtime terrain disagrees with the decoded stage-2 streams");
        }
    }
    println!("       palette changes:");
    for (column, palette, bank) in &builder.palette_changes {
        println!("         col {column:5} bank {bank} {}", hex(palette));
    }
    println!("       top tiles:");
    for (tile, count) in stage.tile_histogram().iter().take(24) {
        println!("         ${tile:02X} x{count}");
    }

    if let Some(path) = dump {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GRD1");
        bytes.extend_from_slice(&(stage_classify.width_tiles as u32).to_le_bytes());
        bytes.extend_from_slice(&(stage_classify.height_tiles as u32).to_le_bytes());
        bytes.extend_from_slice(&stage_classify.tiles);
        bytes.extend_from_slice(&stage_classify.attributes);
        fs::write(path, &bytes).with_context(|| format!("writing {}", path.display()))?;
        println!("stage map dump -> {}", path.display());
    }

    if let Some(path) = png {
        let master = &gradius_cart::NES_MASTER_PALETTE;
        write_png(path, &title, &cart.chr, master)?;
        println!("title png -> {}", path.display());
        let stage_path = path.with_file_name(format!(
            "{}-stage.png",
            path.file_stem().unwrap_or_default().to_string_lossy()
        ));
        write_png(&stage_path, &stage, &cart.chr, master)?;
        println!("stage png -> {}", stage_path.display());
    }

    Ok(())
}

/// Convert the 1-based `--stage` number to the zero-based `$19` value.
pub(crate) fn stage_index(stage_number: u8) -> Result<u8> {
    if !(1..=crate::stages::STAGE_COUNT as u8).contains(&stage_number) {
        bail!(
            "--stage must be 1..={}, got {stage_number}",
            crate::stages::STAGE_COUNT
        );
    }
    Ok(stage_number - 1)
}
