use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use gradius_nes::{Emulator, VramWrite};

use crate::{nametable, oracle};

use super::{
    physical_index, snapshot_screen, visible_attribute_slot, visible_slot, ScreenData,
    HEIGHT_TILES, STAGE_HEIGHT_TILES,
};

/// One live nametable snapshot requested at a world scroll.
#[derive(Clone, Debug)]
pub struct Sample {
    /// The requested scroll in pixels.
    pub requested: usize,
    /// The first stage scroll value at or past the request; the sample holds
    /// the last frame captured at that value.
    pub scroll: usize,
    pub palette: [u8; 32],
    pub chr_bank: u8,
    /// Physical 2 KiB nametable memory at `scroll`.
    pub vram: Vec<u8>,
}

/// One `$A65C` terrain record the spawner consumed during a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainSpawn {
    /// `$61` (`$3F & $0E`): the 512-px column group, 0, 2, ... 14.
    pub group: usize,
    /// Stage scroll in pixels when the stream pointer passed the record.
    pub scroll: usize,
    /// Stream delay byte, the exported `Terrain::column`.
    pub delay: u8,
    /// `$A65C` definition index, the exported `Terrain::kind`.
    pub kind: u8,
}

/// Runtime comparison of consumed terrain records with the decoded streams.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainCheck {
    /// Terrain records the trace saw consumed.
    pub consumed: usize,
    /// Consumed records that disagree with the decoded stream position.
    pub record_mismatches: usize,
    /// Consumed records before their column's scroll threshold, or far past it.
    pub position_mismatches: usize,
    /// Decoded records of a fully pre-boss column that never spawned.
    pub missing: usize,
    /// Largest pixels past the scroll threshold, for frame-granularity evidence.
    pub max_lag: usize,
}

/// Accumulates nametable writes into a wide world map.
#[derive(Default)]
pub struct Builder {
    tiles: BTreeMap<(usize, usize), u8>,
    attributes: BTreeMap<(usize, usize), u8>,
    /// Up to 32 samples of `(scroll, palette, chr_bank)` when the displayed
    /// palette or CHR bank changed, as `(world tile column, palette, chr_bank)`.
    pub palette_changes: Vec<(usize, [u8; 32], u8)>,
    pub chr_banks: BTreeMap<u8, usize>,
    /// How many frames each palette was displayed, for the dominant palette.
    pub palette_counts: BTreeMap<[u8; 32], usize>,
    /// Physical nametable snapshot from the last captured stage frame, kept
    /// for the visible-column validation against the extracted map.
    pub sample_vram: Option<Vec<u8>>,
    /// 16-bit scroll belonging to `sample_vram`.
    pub sample_scroll: Option<usize>,
    /// Requested live nametable samples, taken once each at the first stage
    /// scroll that reaches the request.
    pub samples: Vec<Sample>,
    pending_samples: Vec<usize>,
    /// Terrain records the spawner consumed, when [`Builder::trace_spawner`]
    /// was enabled.
    pub terrain_spawns: Vec<TerrainSpawn>,
    trace_spawner: bool,
    previous_spawner: Option<(usize, u16)>,
    pub tile_writes: usize,
    pub attribute_writes: usize,
    pub palette_writes: usize,
    pub first_scroll: usize,
    pub last_scroll: usize,
    pub max_scroll: usize,
    /// Times the 16-bit scroll went backwards (death, checkpoint restart).
    pub resets: usize,
    previous_scroll: Option<usize>,
    previous_palette: Option<[u8; 32]>,
    previous_bank: Option<u8>,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Request a live nametable sample at the first stage scroll >= each value.
    pub fn request_samples(&mut self, scrolls: &[usize]) {
        self.pending_samples = scrolls.to_vec();
        self.pending_samples.sort_unstable();
        self.pending_samples.dedup();
    }

    /// Take any requested samples that this scroll has reached.
    ///
    /// A scroll value can persist for several frames (stage init holds 0 while
    /// the background is built), so a sample keeps refreshing until the scroll
    /// moves on. It therefore captures the last frame at the first value that
    /// reaches the request, not the first frame at that value.
    pub fn take_samples(&mut self, vram: &[u8], scroll: usize, palette: [u8; 32], chr_bank: u8) {
        while self
            .pending_samples
            .first()
            .is_some_and(|requested| scroll >= *requested)
        {
            let requested = self.pending_samples.remove(0);
            self.samples.push(Sample {
                requested,
                scroll,
                palette,
                chr_bank,
                vram: vram.to_vec(),
            });
        }
        if let Some(last) = self
            .samples
            .last_mut()
            .filter(|sample| sample.scroll == scroll)
        {
            last.palette = palette;
            last.chr_bank = chr_bank;
            last.vram.copy_from_slice(vram);
        }
    }

    /// Follow the spawner's `$6A` stream pointer and record the `$A65C`
    /// terrain records it consumes.
    pub fn trace_spawner(&mut self) {
        self.trace_spawner = true;
    }

    /// Note the spawner state for this in-stage frame.
    pub fn observe_spawner(&mut self, prg: &[u8], ram: &[u8], scroll: usize) {
        if !self.trace_spawner {
            return;
        }
        let group = usize::from(ram[0x3F] & 0x0E);
        let pointer = u16::from_le_bytes([ram[0x6A], ram[0x6B]]);
        if let Some((previous_group, previous)) = self.previous_spawner {
            // Stage 2 has only two-byte records, so consuming one advances the
            // pointer by two inside the same column group. A group change
            // reloads the pointer from the column table instead.
            if group == previous_group && pointer == previous.wrapping_add(2) {
                let offset = usize::from(previous.wrapping_sub(0x8000));
                if let (Some(&delay), Some(&kind)) = (prg.get(offset), prg.get(offset + 1)) {
                    if kind < 0x80 {
                        self.terrain_spawns.push(TerrainSpawn {
                            group,
                            scroll,
                            delay,
                            kind,
                        });
                    }
                }
            }
        }
        self.previous_spawner = Some((group, pointer));
    }

    /// Compare the runtime terrain trace with decoded column streams.
    ///
    /// `columns` is in `$3F & $0E` order. A record's consumption threshold is
    /// `group * 256 + 2 * delay` pixels, matching `$A32F`. Columns before the
    /// boss threshold must be consumed completely; later ones are truncated by
    /// the stage end. The scroll advances one pixel per frame, so a correct
    /// trace lags its threshold by at most that step.
    pub fn check_terrain(
        &self,
        columns: &[Vec<crate::sections::Record>],
        boss: u8,
    ) -> TerrainCheck {
        let mut check = TerrainCheck::default();
        for (index, records) in columns.iter().enumerate() {
            let group = index * 2;
            let expected: Vec<(u8, u8)> = records
                .iter()
                .filter_map(|record| match record {
                    crate::sections::Record::Spawn { delay, kind } if *kind < 0x80 => {
                        Some((*delay, *kind))
                    }
                    _ => None,
                })
                .collect();
            let observed: Vec<&TerrainSpawn> = self
                .terrain_spawns
                .iter()
                .filter(|spawn| spawn.group == group)
                .collect();
            for (position, spawn) in observed.iter().enumerate() {
                check.consumed += 1;
                if expected.get(position) != Some(&(spawn.delay, spawn.kind)) {
                    check.record_mismatches += 1;
                }
                let threshold = group * 256 + 2 * usize::from(spawn.delay);
                if spawn.scroll < threshold || spawn.scroll - threshold > 4 {
                    check.position_mismatches += 1;
                } else {
                    check.max_lag = check.max_lag.max(spawn.scroll - threshold);
                }
            }
            if group < usize::from(boss) {
                check.missing += expected.len().saturating_sub(observed.len());
            }
        }
        check
    }

    /// Note the scroll for this frame; returns whether it went backwards.
    pub fn observe_frame(&mut self, scroll: usize, palette: [u8; 32], chr_bank: u8) {
        if let Some(previous) = self.previous_scroll {
            if scroll < previous {
                self.resets += 1;
            }
        } else {
            self.first_scroll = scroll;
        }
        self.previous_scroll = Some(scroll);
        self.last_scroll = scroll;
        self.max_scroll = self.max_scroll.max(scroll);
        if self.previous_palette != Some(palette) || self.previous_bank != Some(chr_bank) {
            self.palette_changes.push((scroll / 8, palette, chr_bank));
            self.previous_palette = Some(palette);
            self.previous_bank = Some(chr_bank);
        }
        *self.palette_counts.entry(palette).or_default() += 1;
        *self.chr_banks.entry(chr_bank).or_default() += 1;
    }

    /// Palette and CHR bank held for the most frames.
    ///
    /// Stages that switch bank or palette only for a short tail (stage 2's boss
    /// area) still export the long scrolling section's display state.
    pub fn dominant_display(&self) -> ([u8; 32], u8) {
        let palette = self
            .palette_counts
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(palette, _)| *palette)
            .unwrap_or([0; 32]);
        let bank = self
            .chr_banks
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(bank, _)| *bank)
            .unwrap_or(0);
        (palette, bank)
    }

    /// Fold one captured `$2007` write into the write statistics.
    ///
    /// Placement is done from the per-frame visible snapshot: the write log is
    /// the trigger and the scroll tells us which world columns the screen is
    /// showing, which is unambiguous even though the game pre-draws 4x4 blocks
    /// into the off-screen nametable.
    pub fn push_write(&mut self, write: &VramWrite) {
        if write.addr >= 0x3F00 {
            self.palette_writes += 1;
            return;
        }
        if !(0x2000..0x3000).contains(&write.addr) {
            return;
        }
        let offset = usize::from(write.addr & 0x03FF);
        if offset < 0x3C0 {
            self.tile_writes += 1;
        } else {
            self.attribute_writes += 1;
        }
    }

    /// Copy the 32 tile columns that are on screen at this scroll.
    ///
    /// The PPU shows world pixels `S..S+256`, so screen tile column `x` is
    /// world column `S/8 + x`. Reading the nametable after the frame's writes
    /// have been applied gives exactly what the player sees, including the
    /// starfield animation.
    pub fn paint_visible(&mut self, vram: &[u8], scroll: usize) {
        self.paint_visible_with(vram, scroll, false);
    }

    /// [`Builder::paint_visible`] that keeps a tile the map already holds.
    ///
    /// Stage 6's `$CD9F` wall erase (`$1B = $86`) blanks the same nametable
    /// cells the boss-cell art was painted into, and the map keeps being
    /// repainted at the live scroll while those writes land. The port erases
    /// the cell dynamically, so the wide map must keep the pre-erase art;
    /// columns that are still unseen (the scroll moves on to `$0C`) are added
    /// normally.
    pub fn paint_visible_keeping_painted(&mut self, vram: &[u8], scroll: usize) {
        self.paint_visible_with(vram, scroll, true);
    }

    fn paint_visible_with(&mut self, vram: &[u8], scroll: usize, keep_painted: bool) {
        let left = scroll / 8;
        for screen_column in 0..32 {
            let k = left + screen_column;
            let (table, column) = visible_slot(k);
            let base = table * 0x400;
            for row in 0..HEIGHT_TILES {
                let value = vram[base + row * 32 + column];
                if keep_painted {
                    self.tiles.entry((k, row)).or_insert(value);
                } else {
                    self.tiles.insert((k, row), value);
                }
            }
            let (attr_table, attr_column) = visible_attribute_slot(k);
            let attr_base = attr_table * 0x400 + 0x3C0;
            for row in 0..8 {
                let value = vram[attr_base + row * 8 + attr_column];
                if keep_painted {
                    self.attributes.entry((k >> 2, row)).or_insert(value);
                } else {
                    self.attributes.insert((k >> 2, row), value);
                }
            }
        }
    }

    /// Number of distinct world tile columns seen.
    pub fn columns(&self) -> usize {
        self.tiles
            .keys()
            .map(|(k, _)| *k)
            .max()
            .map_or(0, |k| k + 1)
    }

    /// Compare the finished map with a physical nametable snapshot on the
    /// columns visible at `scroll`.
    ///
    /// This samples the oracle's live nametable rather than the write log, so
    /// a folding error in the wide map shows up as a mismatch. Returns
    /// `(tile mismatches, attribute mismatches)`.
    pub fn validate_visible(
        &self,
        screen: &ScreenData,
        vram: &[u8],
        scroll: usize,
    ) -> (usize, usize) {
        let left = scroll / 8;
        let attribute_width = screen.width_tiles / 4;
        let attribute_rows = screen.height_tiles.div_ceil(4);
        let mut tile_mismatches = 0;
        let mut attribute_mismatches = 0;
        for screen_column in 0..32 {
            let k = left + screen_column;
            if k >= screen.width_tiles {
                continue;
            }
            let (table, column) = visible_slot(k);
            for row in 0..screen.height_tiles {
                let expected = vram[table * 0x400 + row * 32 + column];
                if screen.tiles[row * screen.width_tiles + k] != expected {
                    tile_mismatches += 1;
                }
            }
            let (attr_table, attr_column) = visible_attribute_slot(k);
            for row in 0..attribute_rows {
                let expected = vram[attr_table * 0x400 + 0x3C0 + row * 8 + attr_column];
                if screen.attributes[row * attribute_width + k / 4] != expected {
                    attribute_mismatches += 1;
                }
            }
        }
        (tile_mismatches, attribute_mismatches)
    }

    /// Finish the map. Width is rounded up to a multiple of four because one
    /// attribute byte covers four columns.
    /// Finish with an explicit playfield height, dropping rows that the
    /// original's sprite-0 HUD split hides.
    pub fn finish_with_height(&self, palette: [u8; 32], chr_bank: u8, height: usize) -> ScreenData {
        let width = self.columns().div_ceil(4) * 4;
        let mut tiles = vec![0u8; width * height];
        let mut filled = vec![false; width * height];
        for ((k, row), value) in &self.tiles {
            if *k < width && *row < height {
                tiles[row * width + k] = *value;
                filled[row * width + k] = true;
            }
        }
        let attribute_width = width / 4;
        let attribute_rows = height.div_ceil(4);
        let mut attributes = vec![0u8; attribute_width * attribute_rows];
        let mut attribute_filled = vec![false; attribute_width * attribute_rows];
        for ((k, row), value) in &self.attributes {
            if *k < attribute_width && *row < attribute_rows {
                attributes[row * attribute_width + k] = *value;
                attribute_filled[row * attribute_width + k] = true;
            }
        }
        ScreenData {
            width_tiles: width,
            height_tiles: height,
            tiles,
            attributes,
            palette,
            chr_bank,
            missing_tiles: filled.iter().filter(|filled| !**filled).count(),
            missing_attributes: attribute_filled.iter().filter(|filled| !**filled).count(),
        }
    }
}

/// `$B600`: the six NT1 rows of the stage-7 brain-chamber door. `$B563`
/// queues four identical cells per row (`$B5EE`), top and bottom toward the
/// middle, twenty frames apart; the port paints the door frame by frame, so
/// the wide map keeps the open corridor and drops these writes.
const STAGE7_DOOR_ROWS: [u16; 6] = [0x2578, 0x2598, 0x25B8, 0x25D8, 0x25F8, 0x2618];
/// `$B5C2`: the door's tile pair, `$C3`/`$C2` and the `+2` partner.
const STAGE7_DOOR_TILES: std::ops::RangeInclusive<u8> = 0xC2..=0xC5;

/// Whether this frame still needs the analysis-only `$26` poke.
///
/// Stage init (`$9B68`) copies the per-player stage at `$26,X` into `$19`.
/// Poking that byte before the first stage start makes the game boot directly
/// into the requested stage. Stage 0 is stage 1, so no poke is needed then.
pub fn needs_stage_poke(state: u8, current_stage: u8, target: u8) -> bool {
    target > 0 && !(state == 5 && current_stage == target)
}

/// Run the oracle and return `(title, stage)`.
///
/// `stage` is the zero-based `$19` to start, matching `stages::Stage`. For a
/// non-zero stage the run pokes `$26` until the stage starts; see
/// [`needs_stage_poke`].
pub fn capture(
    cart: &gradius_cart::Cartridge,
    frames: u64,
    plan: &oracle::Plan,
    assist: bool,
    survive: bool,
    stage: u8,
    sample_scrolls: &[usize],
) -> Result<(ScreenData, ScreenData, ScreenData, Builder)> {
    const TITLE_FRAME: u64 = 120;
    if frames < TITLE_FRAME {
        bail!("--frames must be at least {TITLE_FRAME} so the title screen is reached");
    }
    let mut emulator = Emulator::new(cart)?;
    emulator.set_capture_vram(true);
    let mut runner = oracle::Runner::new();
    runner.keep_alive = survive;
    let mut builder = Builder::new();
    builder.request_samples(sample_scrolls);
    if stage == 1 && !sample_scrolls.is_empty() {
        builder.trace_spawner();
    }
    let mut title: Option<ScreenData> = None;
    let mut painted_left: Option<usize> = None;
    // Stage 6's `$86` window erases the boss-cell nametable cells; once it
    // starts, later repaints must not blank the art the map already holds.
    let mut keep_painted = false;

    while emulator.frames < frames {
        if emulator.frames == TITLE_FRAME {
            let ppu = &emulator.bus.ppu;
            let base = usize::from(ppu.ctrl & 0x03);
            let table = match base {
                0 | 2 => 0,
                _ => 1,
            };
            title = Some(snapshot_screen(&ppu.vram, ppu.palette, ppu.chr_bank, table));
        }

        if needs_stage_poke(emulator.bus.ram[0x00], emulator.bus.ram[0x19], stage) {
            // Single-player: X = $18 is 0 at stage init.
            emulator.bus.ram[0x26] = stage;
        }
        // Stage 3's `$96`/`$C900` turrets paint their art into the live
        // nametable. `$5D` is cleared by the frame handler before the turret
        // runs, so it cannot suppress the paint; instead, remember which
        // records the live turrets can paint and drop those writes below. The
        // port re-applies the turrets dynamically.
        let turret_records = if stage == 2 {
            nametable::turret_paint_records(
                &emulator.bus.ram,
                &nametable::turret_groups(&emulator.bus.prg),
            )
        } else {
            Vec::new()
        };
        let turret_vram = (!turret_records.is_empty()).then(|| emulator.bus.ppu.vram.to_vec());
        // Stage 7's `$B563` wall paints the brain-chamber door into NT1. The
        // port animates it, so remember the cells and undo the wall's writes
        // below; the wide map then holds the open corridor the stage builds.
        let door_before: Vec<(u16, u8)> = if stage == 6 {
            STAGE7_DOOR_ROWS
                .iter()
                .flat_map(|row| (0..4).map(move |offset| row + offset))
                .map(|addr| (addr, emulator.bus.ppu.vram[physical_index(addr)]))
                .collect()
        } else {
            Vec::new()
        };
        runner.advance(&mut emulator, plan, assist);
        if survive {
            // The shield timer (`$46`) is the game's own invulnerability
            // counter: a pickup sets it to 5 and each hit decrements it while
            // `$46 != 0`. Keeping it at 5 keeps the ship alive so the stage
            // scrolls through its whole length; the only scroll reset left is
            // the stage's own checkpoint replay at the boss. It does not
            // change what the background renderer writes.
            emulator.bus.ram[0x46] = 5;
        }

        let writes: Vec<VramWrite> = emulator.vram_writes().to_vec();
        emulator.bus.ppu.clear_vram_writes();
        for (addr, before) in door_before {
            if writes
                .iter()
                .any(|write| write.addr == addr && STAGE7_DOOR_TILES.contains(&write.value))
            {
                emulator.bus.ppu.vram[physical_index(addr)] = before;
            }
        }
        if let Some(before) = &turret_vram {
            // Restore only cells whose last write this frame is a complete
            // turret record, so a background column write to the same cell
            // still wins and a coincidental single byte never matches.
            let mut last_write: BTreeMap<u16, u8> = BTreeMap::new();
            for write in &writes {
                if (0x2000..0x3000).contains(&write.addr) {
                    last_write.insert(write.addr, write.value);
                }
            }
            let mut turret_cells = std::collections::BTreeSet::new();
            for record in &turret_records {
                let complete = record.tiles.iter().enumerate().all(|(offset, tile)| {
                    let addr = record.addr.wrapping_add(offset as u16);
                    last_write.get(&addr) == Some(tile)
                });
                if complete {
                    for offset in 0..record.tiles.len() {
                        turret_cells.insert(record.addr.wrapping_add(offset as u16));
                    }
                }
            }
            for addr in turret_cells {
                let offset = physical_index(addr);
                emulator.bus.ppu.vram[offset] = before[offset];
            }
        }
        let scroll =
            usize::from(emulator.bus.ram[0x3E]) + 256 * usize::from(emulator.bus.ram[0x3F]);
        let in_stage = emulator.bus.ram[0x00] == 5
            && emulator.frames >= 200
            && emulator.bus.ram[0x19] == stage;
        if in_stage {
            let palette = emulator.bus.ppu.palette;
            let chr_bank = emulator.bus.ppu.chr_bank;
            builder.observe_frame(scroll, palette, chr_bank);
            for write in &writes {
                builder.push_write(write);
            }
            if stage == 5 && emulator.bus.ram[0x1B] == 0x86 {
                keep_painted = true;
            }
            let left = scroll / 8;
            if painted_left != Some(left) || !writes.is_empty() {
                if keep_painted {
                    builder.paint_visible_keeping_painted(&emulator.bus.ppu.vram, scroll);
                } else {
                    builder.paint_visible(&emulator.bus.ppu.vram, scroll);
                }
                painted_left = Some(left);
            }
            builder.take_samples(&emulator.bus.ppu.vram, scroll, palette, chr_bank);
            builder.observe_spawner(&cart.prg, &emulator.bus.ram, scroll);
            match &mut builder.sample_vram {
                Some(buffer) => buffer.copy_from_slice(&emulator.bus.ppu.vram),
                None => builder.sample_vram = Some(emulator.bus.ppu.vram.to_vec()),
            }
            builder.sample_scroll = Some(scroll);
        }
    }
    let title = title.context("title snapshot was not taken")?;
    let (stage_palette, stage_bank) = builder.dominant_display();
    let stage = builder.finish_with_height(stage_palette, stage_bank, STAGE_HEIGHT_TILES);
    let stage_classify = builder.finish_with_height(stage_palette, stage_bank, 28);
    Ok((title, stage, stage_classify, builder))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeping_painted_columns_holds_the_pre_erase_art() {
        // Scroll 0 shows world columns 0-31 from physical table 0.
        let mut vram = vec![0u8; 0x800];
        vram[7 * 32 + 21] = 0x8d;
        let mut builder = Builder::new();
        builder.paint_visible(&vram, 0);
        assert_eq!(builder.tiles.get(&(21, 7)), Some(&0x8d));

        // `$CD9F` blanks the same cell after the art was painted; the wide map
        // must keep the art so the port can erase it dynamically.
        vram[7 * 32 + 21] = 0;
        builder.paint_visible_keeping_painted(&vram, 0);
        assert_eq!(builder.tiles.get(&(21, 7)), Some(&0x8d));

        // The ordinary repaint still blanks it.
        builder.paint_visible(&vram, 0);
        assert_eq!(builder.tiles.get(&(21, 7)), Some(&0));
    }
}
