//! Dump the per-stage root table.
//!
//! The stage number is the zero-based `$19` (0 = stage 1, 6 = stage 7). The
//! game stores per-player copies at `$26,X`, copies one into `$19` at stage
//! init (`$9B66`) and increments it after the boss and the scroll end
//! (`$992E`). Every per-stage root below is indexed by that byte.
//!
//! ```text
//! cargo xtask stages "Gradius (E).nes"
//! ```
//!
//! The decoder is deliberately strict: every pointer must stay inside the
//! 32 KiB PRG window, the spawn sub-tables must stay ordered with one to eight
//! column pointers, and the `$A7CA` streams are walked with the same parser as
//! `xtask sections`, so a broken table fails instead of printing garbage.
//! Nothing here invents content: the table only names the roots.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Result};

use crate::sections;

/// Playable stages. `$A7CA` has an eighth entry (`$A83E`) that is stage 1's
/// first stream, not a stage.
pub const STAGE_COUNT: usize = 7;

/// `$A7CA`: per-stage column-pointer table (8 words).
const SPAWN_TABLE: u16 = 0xA7CA;
/// `$8344`: per-stage second theme (the first theme is `$93`).
const MUSIC: u16 = 0x8344;
/// `$834B`: per-stage `$2D` palette/attribute upload mode.
const PALETTE_MODE: u16 = 0x834B;
/// `$8354`: coarse scroll at which `$835C` switches to the second theme.
const MUSIC_BOUNDARY: u16 = 0x8354;
/// `$98F0`: per-stage scroll end (`$3F`): stage advance needs `$39` plus this.
const END_THRESHOLD: u16 = 0x98F0;
/// `$9A30`: per-stage `$3F` at which normal play enters the boss transition.
const BOSS_THRESHOLD: u16 = 0x9A30;
/// `$9BC6`: per-stage start row; `$9BCE` is the shared scroll LUT.
const START_ROW: u16 = 0x9BC6;
/// `$9FAE`: per-stage attribute/row parameter (stage 5 is special-cased).
const BG_ATTR: u16 = 0x9FAE;
/// `$9FB6`: per-stage background height-map pointer.
const BG_HEIGHT: u16 = 0x9FB6;
/// `$9FC6`: per-stage base offset added to the shared `$9D49` row offsets.
const BG_BASE: u16 = 0x9FC6;
/// `$9FD6`: per-stage background tile-stream pointer.
const BG_TILES: u16 = 0x9FD6;
/// `$C433`: per-stage collision/attack dispatch reached from `$C42E`.
const COLLISION: u16 = 0xC433;

/// One stage's roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stage {
    /// `$A7CA`: pointer to this stage's column sub-table.
    pub spawn: u16,
    /// Column pointers in the sub-table (the `$A7CA` neighbour difference).
    pub columns: usize,
    /// `$A65C` terrain placements found in the stage's streams.
    pub terrain: usize,
    /// `$9FB6`: background height map.
    pub bg_height: u16,
    /// `$9FC6`: background base offset.
    pub bg_base: u16,
    /// `$9FD6`: background tile stream.
    pub bg_tiles: u16,
    /// `$9FAE`: attribute/row parameter.
    pub bg_attr: u8,
    /// `$8344`: second theme id (masked to six bits by `$EC18`).
    pub music: u8,
    /// `$834B`: `$2D` upload mode.
    pub palette_mode: u8,
    /// `$8354`: scroll boundary that starts the second theme.
    pub music_boundary: u8,
    /// `$98F0`: scroll end.
    pub end: u8,
    /// `$9A30`: boss transition threshold.
    pub boss: u8,
    /// `$9BC6`: start row (tile-encoded, used through `$9BCE`).
    pub start: u8,
    /// `$C433`: per-stage collision/attack handler.
    pub collision: u16,
}

fn read(prg: &[u8], addr: u16) -> Result<u8> {
    prg.get(usize::from(addr.wrapping_sub(0x8000)))
        .copied()
        .ok_or_else(|| anyhow!("address ${addr:04X} is outside the 32 KiB PRG"))
}

fn word(prg: &[u8], addr: u16) -> Result<u16> {
    Ok(u16::from_le_bytes([
        read(prg, addr)?,
        read(prg, addr.wrapping_add(1))?,
    ]))
}

fn require_prg(addr: u16, what: &str) -> Result<()> {
    if addr < 0x8000 {
        bail!("{what} ${addr:04X} is outside the PRG window");
    }
    Ok(())
}

/// Decode the seven stage roots from a PRG image.
pub fn decode(prg: &[u8]) -> Result<[Stage; STAGE_COUNT]> {
    // This also validates every spawn stream and gives the terrain counts.
    let sections = sections::decode(prg)?;

    let mut pointers = [0u16; STAGE_COUNT + 1];
    for (stage, pointer) in pointers.iter_mut().enumerate() {
        *pointer = word(prg, SPAWN_TABLE.wrapping_add(2 * stage as u16))?;
        require_prg(*pointer, "spawn sub-table")?;
    }

    let mut stages = Vec::with_capacity(STAGE_COUNT);
    for stage in 0..STAGE_COUNT {
        let columns = usize::from(pointers[stage + 1].wrapping_sub(pointers[stage])) / 2;
        if columns == 0 || columns > 8 {
            bail!("stage {} has {columns} spawn columns", stage + 1);
        }
        let bg_height = word(prg, BG_HEIGHT + 2 * stage as u16)?;
        let bg_base = word(prg, BG_BASE + 2 * stage as u16)?;
        let bg_tiles = word(prg, BG_TILES + 2 * stage as u16)?;
        for (what, addr) in [
            ("background height map", bg_height),
            ("background base", bg_base),
            ("background tile stream", bg_tiles),
        ] {
            require_prg(addr, what)?;
        }

        let collision = word(prg, COLLISION + 2 * stage as u16)?;
        require_prg(collision, "collision handler")?;

        let end = read(prg, END_THRESHOLD + stage as u16)?;
        let boss = read(prg, BOSS_THRESHOLD + stage as u16)?;
        if boss > end {
            bail!(
                "stage {} has boss threshold ${boss:02X} past end ${end:02X}",
                stage + 1
            );
        }

        stages.push(Stage {
            spawn: pointers[stage],
            columns,
            terrain: sections.terrain[stage].len(),
            bg_height,
            bg_base,
            bg_tiles,
            bg_attr: read(prg, BG_ATTR + stage as u16)?,
            music: read(prg, MUSIC + stage as u16)?,
            palette_mode: read(prg, PALETTE_MODE + stage as u16)?,
            music_boundary: read(prg, MUSIC_BOUNDARY + stage as u16)?,
            end,
            boss,
            start: read(prg, START_ROW + stage as u16)?,
            collision,
        });
    }

    let count = stages.len();
    stages
        .try_into()
        .map_err(|_| anyhow!("decoded {count} stages, expected {STAGE_COUNT}"))
}

/// Render the root table as fixed-width text.
pub fn render(stages: &[Stage; STAGE_COUNT]) -> String {
    let mut text = String::from(
        "stage spawn cols terr music pmod bnd end boss start bg_height bg_base bg_tiles attr collision\n",
    );
    for (index, stage) in stages.iter().enumerate() {
        text.push_str(&format!(
            "{:5} ${:04X} {:4} {:4} ${:02X} {:4} {:3} ${:02X} ${:02X}  ${:02X}    ${:04X}   ${:04X}   ${:04X}   ${:02X}   ${:04X}\n",
            index + 1,
            stage.spawn,
            stage.columns,
            stage.terrain,
            stage.music,
            stage.palette_mode,
            stage.music_boundary,
            stage.end,
            stage.boss,
            stage.start,
            stage.bg_height,
            stage.bg_base,
            stage.bg_tiles,
            stage.bg_attr,
            stage.collision,
        ));
    }
    text
}

/// `cargo xtask stages <rom> [--out table.txt]`.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write(prg: &mut [u8], addr: u16, value: u16) {
        let offset = usize::from(addr - 0x8000);
        prg[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    /// A minimal but valid cartridge image: one spawn column per stage, one
    /// terrain record in each stream, and distinct values in every root table.
    fn synthetic() -> Vec<u8> {
        let mut prg = vec![0u8; 0x8000];

        // Spawn sub-tables, one word each. Stage 7 is the table terminator.
        for stage in 0..=STAGE_COUNT {
            write(
                &mut prg,
                SPAWN_TABLE + 2 * stage as u16,
                0xA800 + 2 * stage as u16,
            );
        }
        for stage in 0..STAGE_COUNT {
            let sub = 0xA800 + 2 * stage as u16;
            let stream = 0xB000 + 0x10 * stage as u16;
            write(&mut prg, sub, stream);
            let offset = usize::from(stream - 0x8000);
            prg[offset..offset + 3].copy_from_slice(&[0x10, 0x01, 0xFF]);
        }

        for stage in 0..STAGE_COUNT {
            let n = stage as u16;
            write(&mut prg, BG_HEIGHT + 2 * n, 0xC000 + 0x20 * n);
            write(&mut prg, BG_BASE + 2 * n, 0xC010 + 0x20 * n);
            write(&mut prg, BG_TILES + 2 * n, 0xC020 + 0x20 * n);
            write(&mut prg, COLLISION + 2 * n, 0xC100 + 0x10 * n);

            let index = usize::from(END_THRESHOLD - 0x8000 + n);
            prg[index] = 0x0E;
            let index = usize::from(BOSS_THRESHOLD - 0x8000 + n);
            prg[index] = 0x0C;

            prg[usize::from(MUSIC - 0x8000 + n)] = 0x90 + stage as u8;
            prg[usize::from(PALETTE_MODE - 0x8000 + n)] = stage as u8;
            prg[usize::from(MUSIC_BOUNDARY - 0x8000 + n)] = 4;
            prg[usize::from(START_ROW - 0x8000 + n)] = 0x05 * stage as u8;
            prg[usize::from(BG_ATTR - 0x8000 + n)] = 0x40;
        }
        prg
    }

    #[test]
    fn decode_reads_every_per_stage_root() {
        let stages = decode(&synthetic()).unwrap();

        assert_eq!(stages[0].spawn, 0xA800);
        assert_eq!(stages[0].columns, 1);
        assert_eq!(stages[0].terrain, 1);
        assert_eq!(stages[0].bg_height, 0xC000);
        assert_eq!(stages[0].bg_base, 0xC010);
        assert_eq!(stages[0].bg_tiles, 0xC020);
        assert_eq!(stages[0].bg_attr, 0x40);
        assert_eq!(stages[0].music, 0x90);
        assert_eq!(stages[0].collision, 0xC100);
        assert_eq!(stages[0].end, 0x0E);
        assert_eq!(stages[0].boss, 0x0C);

        assert_eq!(stages[6].spawn, 0xA800 + 2 * 6);
        assert_eq!(stages[6].columns, 1);
        assert_eq!(stages[6].music, 0x96);
        assert_eq!(stages[6].bg_height, 0xC000 + 0x20 * 6);
        assert_eq!(stages[6].collision, 0xC100 + 0x10 * 6);
    }

    #[test]
    fn decode_rejects_a_spawn_pointer_outside_prg() {
        let mut prg = synthetic();
        write(&mut prg, SPAWN_TABLE + 2, 0x0100);
        assert!(decode(&prg).is_err());
    }

    #[test]
    fn decode_rejects_a_stage_without_columns() {
        let mut prg = synthetic();
        // Stage 2 points at the same sub-table as stage 3: zero columns.
        write(&mut prg, SPAWN_TABLE + 2 * 2, 0xA800 + 2);
        assert!(decode(&prg).is_err());
    }

    #[test]
    fn render_names_every_stage() {
        let text = render(&decode(&synthetic()).unwrap());
        assert!(text.starts_with("stage spawn"));
        for stage in 1..=STAGE_COUNT {
            assert!(text.contains(&format!("\n{stage:5} $")));
        }
    }
}
