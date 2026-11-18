//! The bottom status bar: power meter, lives, BCD score and high score.
//!
//! The original paints the bar as background tiles through the `$0700` VRAM
//! queue (`$8450`, `$8479`) and keeps the meter cursor in OAM slot 0. The port
//! copies the same two 32-tile rows from CHR bank 2 and only substitutes the
//! live digits, so the bar is pixel-identical to the cartridge.
//!
//! # Frame timing
//!
//! The bar is background tiles, so it obeys `PPUMASK` like any other
//! background. Callers must draw it through [`draw_masked`], passing the
//! frame's mask, so the stage-entry blank window stays black:
//!
//! ```ignore
//! hud::draw_masked(fb, assets, &hud, &rom.tables, self.ppu_mask());
//! ```
//!
//! A caller that paints the band black before drawing (the software renderer
//! clears the whole frame anyway) needs no extra guard: a black band is what
//! the masked frame shows.
//!
//! The score column is a `$0700` queue write, not an immediate nametable
//! write, so the digits appear 3-8 frames after the BCD value changes. Keep
//! one [`ScoreDelay`] in the game and feed its output to [`Hud::new`]:
//!
//! ```ignore
//! let shown = self.score_delay.step(self.score_bcd.digits);
//! hud::draw_masked(fb, assets, &Hud::new(shown, self.high_score_bcd.digits, lives, capsules, &self.power), &rom.tables, self.ppu_mask());
//! ```

use crate::assets::Assets;
use crate::palettes;
use crate::powerup::PowerMeter;
use crate::rom::Tables;
use crate::video::{Framebuffer, HEIGHT};

/// First row of the bar (`$9A96`: the sprite-0 split reloads scroll 0 on
/// scanline 208, and the background nametable switch lands on scanline 210,
/// so the fixed HUD band is rows 26-29 and only 210..239 are bar tiles).
pub const HUD_Y: i32 = 210;
/// Height of the bar in pixels.
pub const HUD_HEIGHT: i32 = HEIGHT as i32 - HUD_Y;

/// CHR half the original uses for the bar tiles.
const HUD_BANK: u8 = 2;
/// First HUD row on screen: the power-meter labels, measured from the oracle.
pub(crate) const METER_Y: i32 = 212;
/// Second HUD row on screen: lives, score and high score.
const STATUS_Y: i32 = 220;
/// Column of the lives digit in the status row.
const LIVES_COLUMN: usize = 5;
/// First of seven columns holding the score.
const SCORE_COLUMN: usize = 11;
/// First of seven columns holding the high score (`HI 0050000`).
const HIGH_SCORE_COLUMN: usize = 23;
/// The six meter entries are four tiles (`32` px) wide, starting at column 4.
/// `$8A35` writes `$55` into one attribute byte of the HUD row, so the
/// selection is a palette change of that entry, never a sprite.
const METER_FIRST_COLUMN: usize = 4;
const METER_ENTRY_COLUMNS: usize = 4;

/// Meter box with its text removed (text record `$19`).
const EMPTY_BOX: [u8; METER_ENTRY_COLUMNS] = [0x1D, 0x1E, 0x1E, 0x1F];

/// Fixed pixels left in the HUD band's nametable row 3 (y 232-239).
///
/// The original only rewrites the two text rows (26 and 27) after the split;
/// row 3 of the HUD nametable still carries three starfield pixels from the
/// initial fill at x 74, 138 and 234 on y 238. They are constant for the whole
/// stage, so they are stored here rather than in a tile row.
const BAND_STARS: [(i32, i32); 3] = [(74, 238), (138, 238), (234, 238)];

/// Values the bar needs. Borrowing the meter keeps the HUD read-only.
pub struct Hud<'a> {
    /// Three BCD bytes, most significant first (`$07E4-$07E6`).
    pub score: [u8; 3],
    /// Three BCD bytes, most significant first (`$07E0-$07E2`).
    pub high_score: [u8; 3],
    pub lives: u8,
    pub capsules: u8,
    pub meter: &'a PowerMeter,
    /// Boxes drawn empty (`$1D $1E $1E $1F`); [`Hud::new`] derives them from
    /// the live meter, the game passes the sampled ones.
    pub empty: [bool; crate::powerup::SLOT_COUNT],
    /// Highlighted box (`$42` as drawn); [`Hud::new`] takes the live one.
    pub selection: u8,
    /// `$18`: `0` prints `1P`, `1` prints `2P` (`$8931`).
    pub player: u8,
}

impl Hud<'_> {
    pub fn new(
        score: [u8; 3],
        high_score: [u8; 3],
        lives: u8,
        capsules: u8,
        meter: &PowerMeter,
    ) -> Hud<'_> {
        Hud {
            score,
            high_score,
            lives,
            capsules,
            meter,
            empty: meter.empty_slots(false),
            selection: meter.selection(),
            player: 0,
        }
    }

    pub fn with_shown(mut self, empty: [bool; crate::powerup::SLOT_COUNT], selection: u8) -> Self {
        self.empty = empty;
        self.selection = selection;
        self
    }

    /// `$18`: substitute the `1P`/`2P` label (`$874B`/`$8751`).
    pub fn with_player(mut self, player: u8) -> Self {
        self.player = player;
        self
    }
}

/// Whether the status bar is visible under `PPUMASK`.
///
/// The bar is background tiles, so it needs the background-enable bit. The
/// cartridge keeps `PPUMASK` at zero through stage init and the first five
/// playing frames, which is the whole stage-entry blank window.
pub fn enabled(ppu_mask: u8) -> bool {
    ppu_mask & 0x08 != 0
}

/// [`draw`] gated on `PPUMASK`: the stage-entry blank window draws nothing.
pub fn draw_masked(
    fb: &mut Framebuffer,
    assets: &Assets,
    hud: &Hud,
    tables: &Tables,
    ppu_mask: u8,
) {
    if enabled(ppu_mask) {
        draw(fb, assets, hud, tables);
    }
}

/// The score digits as they appear on screen, lagging the BCD value.
///
/// `$8479` copies the score to the nametable through the `$0700` queue, so the
/// visible digits trail the live BCD value by the measured 3-8 frames. Create
/// one per run, call [`step`](Self::step) once per rendered frame with the
/// live digits, and store the result into [`Hud::score`].
///
/// Unchanged input leaves the displayed value untouched, so a held score is
/// stable frame to frame; a change only becomes visible `delay` frames after
/// the frame it was first seen.
pub struct ScoreDelay {
    shown: [u8; 3],
    pending: [u8; 3],
    delay: u8,
    /// Per-change delays: the measured title/stage-1 sequence is 5, 4, 3, 8.
    pattern: &'static [u8],
    index: usize,
    countdown: u8,
}

/// Save-state view of [`ScoreDelay`]. `pattern` is not stored: it is static
/// configuration, so the owner keeps it across a restore.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScoreDelayState {
    pub shown: [u8; 3],
    pub pending: [u8; 3],
    pub delay: u8,
    pub index: u32,
    pub countdown: u8,
}

impl ScoreDelay {
    /// `delay` is the number of frames between the BCD change and the redraw;
    /// the oracle's measured range is 3-8.
    pub fn new(delay: u8) -> Self {
        ScoreDelay {
            shown: [0; 3],
            pending: [0; 3],
            delay,
            pattern: &[],
            index: 0,
            countdown: 0,
        }
    }

    /// Use a per-change delay cycle (`&[5, 4, 3, 8]` is the measured one).
    pub fn with_pattern(pattern: &'static [u8]) -> Self {
        ScoreDelay {
            pattern,
            ..Self::new(0)
        }
    }

    fn next_delay(&mut self) -> u8 {
        if self.pattern.is_empty() {
            return self.delay;
        }
        let delay = self.pattern[self.index % self.pattern.len()];
        self.index += 1;
        delay
    }

    /// Advance one frame and return the digits to draw.
    pub fn step(&mut self, digits: [u8; 3]) -> [u8; 3] {
        if digits != self.pending {
            self.pending = digits;
            self.countdown = self.next_delay();
        }
        if self.countdown == 0 {
            self.shown = self.pending;
        } else {
            self.countdown -= 1;
        }
        self.shown
    }

    /// Copy the delay counters for a save state.
    pub(crate) fn state(&self) -> ScoreDelayState {
        ScoreDelayState {
            shown: self.shown,
            pending: self.pending,
            delay: self.delay,
            index: self.index as u32,
            countdown: self.countdown,
        }
    }

    /// Restore a state written by [`state`](Self::state); the pattern stays.
    pub(crate) fn set_state(&mut self, state: ScoreDelayState) {
        self.shown = state.shown;
        self.pending = state.pending;
        self.delay = state.delay;
        self.index = state.index as usize;
        self.countdown = state.countdown;
    }
}

/// Draw the whole status bar.
pub fn draw(fb: &mut Framebuffer, assets: &Assets, hud: &Hud, tables: &Tables) {
    let background = palettes::background(tables);
    let palette = background[0].map(|color| assets.color(color));
    let highlight = background[1].map(|color| assets.color(color));
    draw_meter_row(
        fb,
        assets,
        &palette,
        &highlight,
        hud.selection,
        &hud.empty,
        &tables.hud_meter_row,
    );
    draw_row(
        fb,
        assets,
        &status_row(hud, &tables.hud_status_row),
        STATUS_Y,
        &palette,
    );
    let star = palette[1];
    for (x, y) in BAND_STARS {
        fb.pixel(x, y, star);
    }
}

/// `$8950` keeps `$42` in 1..=6; `$8A35` recolors that entry with attribute
/// byte `$55`, i.e. background palette 1 instead of palette 0.
///
/// A box whose upgrade can no longer be bought is redrawn by `$89F5`-`$8A2F`
/// with text record `$19`, the empty frame `$1D $1E $1E $1F`; it keeps the
/// selection palette when it is the selected one.
fn draw_meter_row(
    fb: &mut Framebuffer,
    assets: &Assets,
    palette: &[[u8; 3]; 4],
    highlight: &[[u8; 3]; 4],
    selection: u8,
    empty: &[bool; crate::powerup::SLOT_COUNT],
    tiles: &[u8; 32],
) {
    let entry = usize::from(selection)
        .checked_sub(1)
        .filter(|entry| *entry < crate::powerup::SLOT_COUNT);
    let first = METER_FIRST_COLUMN;
    for (column, &index) in tiles.iter().enumerate() {
        let slot = column
            .checked_sub(first)
            .map(|offset| offset / METER_ENTRY_COLUMNS)
            .filter(|slot| *slot < crate::powerup::SLOT_COUNT);
        let lit = entry.is_some() && slot == entry;
        let index = match slot {
            Some(slot) if empty[slot] => EMPTY_BOX[(column - first) % METER_ENTRY_COLUMNS],
            _ => index,
        };
        fb.tile(
            assets,
            HUD_BANK,
            index,
            column as i32 * 8,
            METER_Y,
            false,
            false,
            if lit { highlight } else { palette },
            false,
        );
    }
}

/// The status row with the live lives digit and score substituted in.
fn status_row(hud: &Hud, base: &[u8; 32]) -> [u8; 32] {
    let mut status = *base;
    status[8] = 0x31 + hud.player.min(1);
    status[LIVES_COLUMN] = 0x30 + hud.lives.min(9);
    status[SCORE_COLUMN..SCORE_COLUMN + 7].copy_from_slice(&score_tiles(hud.score));
    status[HIGH_SCORE_COLUMN..HIGH_SCORE_COLUMN + 7].copy_from_slice(&score_tiles(hud.high_score));
    status
}

/// Seven score tiles: six BCD digits and the constant trailing zero.
fn score_tiles(score: [u8; 3]) -> [u8; 7] {
    let mut tiles = [0x30; 7];
    for (index, byte) in score.iter().enumerate() {
        tiles[index * 2] = 0x30 + ((byte >> 4) & 0x0F);
        tiles[index * 2 + 1] = 0x30 + (byte & 0x0F);
    }
    tiles
}

fn draw_row(
    fb: &mut Framebuffer,
    assets: &Assets,
    tiles: &[u8; 32],
    y: i32,
    palette: &[[u8; 3]; 4],
) {
    for (column, &index) in tiles.iter().enumerate() {
        fb.tile(
            assets,
            HUD_BANK,
            index,
            column as i32 * 8,
            y,
            false,
            false,
            palette,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Assets {
        Assets {
            tiles: vec![[1u8; 64]; 8 * 256],
            colors: (0..64).map(|_| [0xFF, 0xFF, 0xFF]).collect(),
            ..Default::default()
        }
    }

    fn tables() -> &'static crate::rom::Tables {
        &crate::rom::testing::rom().tables
    }

    fn render(fb: &mut Framebuffer, assets: &Assets, hud: &Hud) {
        draw(fb, assets, hud, tables());
    }

    fn render_masked(fb: &mut Framebuffer, assets: &Assets, hud: &Hud, mask: u8) {
        draw_masked(fb, assets, hud, tables(), mask);
    }

    #[test]
    fn the_bar_never_draws_above_its_top_edge() {
        let assets = assets();
        let meter = PowerMeter::default();
        let mut fb = Framebuffer::new();
        render(
            &mut fb,
            &assets,
            &Hud::new([0x12, 0x34, 0x56], [0x00, 0x50, 0x00], 3, 2, &meter),
        );
        let above = &fb.as_rgba()[..(HUD_Y as usize * 256) * 4];
        assert!(above.chunks_exact(4).all(|pixel| pixel[3] == 0));
    }

    #[test]
    fn the_status_row_prints_the_bcd_score_with_a_trailing_zero() {
        let meter = PowerMeter::default();
        let hud = Hud::new([0x00, 0x00, 0x40], [0x00, 0x50, 0x00], 3, 0, &meter);
        let tiles = status_row(&hud, &tables().hud_status_row);
        assert_eq!(
            &tiles[SCORE_COLUMN..SCORE_COLUMN + 7],
            &[0x30, 0x30, 0x30, 0x30, 0x34, 0x30, 0x30]
        );
        assert_eq!(tiles[LIVES_COLUMN], 0x33);
    }

    #[test]
    fn the_status_row_prints_the_current_player_label() {
        let meter = PowerMeter::default();
        let p1 = Hud::new([0; 3], [0; 3], 3, 0, &meter);
        assert_eq!(
            &status_row(&p1, &tables().hud_status_row)[8..10],
            &[0x31, 0x66],
            "1P"
        );
        let p2 = Hud::new([0; 3], [0; 3], 3, 0, &meter).with_player(1);
        assert_eq!(
            &status_row(&p2, &tables().hud_status_row)[8..10],
            &[0x32, 0x66],
            "2P"
        );
    }

    #[test]
    fn the_band_keeps_its_three_star_pixels() {
        let assets = assets();
        let meter = PowerMeter::default();
        let mut fb = Framebuffer::new();
        render(
            &mut fb,
            &assets,
            &Hud::new([0, 0, 0], [0x00, 0x50, 0x00], 3, 0, &meter),
        );
        for (x, y) in BAND_STARS {
            let offset = (y as usize * crate::video::WIDTH + x as usize) * 4;
            assert_eq!(
                &fb.as_rgba()[offset..offset + 4],
                &[0xFF, 0xFF, 0xFF, 0xFF],
                "star at {x},{y}"
            );
        }
    }

    #[test]
    fn an_owned_shield_blanks_the_sixth_meter_entry_glyph() {
        // Bank 2 tiles: the `?` glyphs are value 3, the blank `$1E` tile is
        // value 1, and each master-palette index gets its own grey so the two
        // are distinguishable.
        let mut assets = Assets {
            tiles: vec![[1u8; 64]; 8 * 256],
            colors: (0..64).map(|index| [index as u8; 3]).collect(),
            ..Default::default()
        };
        assets.tiles[2 * 256 + 0x62] = [3; 64];
        assets.tiles[2 * 256 + 0x63] = [3; 64];
        assets.tiles[2 * 256 + 0x1E] = [1; 64];
        let sample = |shielded: bool| {
            let mut meter = PowerMeter::default();
            if shielded {
                meter.dev_set_shield(5);
            }
            let mut fb = Framebuffer::new();
            render(
                &mut fb,
                &assets,
                &Hud::new([0, 0, 0], [0x00, 0x50, 0x00], 3, 0, &meter),
            );
            let offset = (METER_Y as usize + 2) * crate::video::WIDTH * 4 + 201 * 4;
            fb.as_rgba()[offset]
        };
        // Value 3 maps to `$0F`, value 1 to `$12`.
        assert_eq!(sample(false), 0x0F, "no shield keeps the `?` glyphs");
        assert_eq!(sample(true), 0x12, "`$46 != 0` blanks the entry");
    }

    #[test]
    fn an_empty_box_draws_the_frame_tiles_and_keeps_the_highlight() {
        // Distinct grey per tile (`$1D $1E $1E $1F` vs the label tiles) and
        // per palette, read at the first pixel of the missile box.
        let mut assets = Assets {
            tiles: vec![[1u8; 64]; 8 * 256],
            colors: (0..64).map(|index| [index as u8; 3]).collect(),
            ..Default::default()
        };
        assets.tiles[2 * 256 + 0x0D] = [3; 64];
        assets.tiles[2 * 256 + 0x1D] = [1; 64];
        let sample = |empty: bool, selection: u8| {
            let meter = PowerMeter::default();
            let mut flags = [false; crate::powerup::SLOT_COUNT];
            flags[1] = empty;
            let mut fb = Framebuffer::new();
            render(
                &mut fb,
                &assets,
                &Hud::new([0, 0, 0], [0, 0, 0], 3, 0, &meter).with_shown(flags, selection),
            );
            // Missile box starts at column 8 (x 64).
            fb.as_rgba()[(METER_Y as usize + 2) * crate::video::WIDTH * 4 + 65 * 4]
        };
        let full = sample(false, 0);
        let empty = sample(true, 0);
        let lit = sample(true, 2);
        assert_ne!(full, empty, "the label tile is replaced by the empty frame");
        assert_ne!(empty, lit, "the selected empty box keeps palette 1");
    }

    #[test]
    fn the_bar_is_hidden_while_the_background_is_masked() {
        let assets = assets();
        let meter = PowerMeter::default();
        let mut fb = Framebuffer::new();
        fb.clear([1, 2, 3]);
        let before = fb.frame_hash();
        render_masked(
            &mut fb,
            &assets,
            &Hud::new([0, 0, 0], [0x00, 0x50, 0x00], 3, 0, &meter),
            0x00,
        );
        render_masked(
            &mut fb,
            &assets,
            &Hud::new([0, 0, 0], [0x00, 0x50, 0x00], 3, 0, &meter),
            0x10,
        );
        assert_eq!(fb.frame_hash(), before, "masked frames keep the band black");
        render_masked(
            &mut fb,
            &assets,
            &Hud::new([0, 0, 0], [0x00, 0x50, 0x00], 3, 0, &meter),
            0x08,
        );
        assert_ne!(fb.frame_hash(), before, "background enable shows the bar");
    }

    #[test]
    fn the_mask_contract_is_the_background_enable_bit() {
        assert!(!enabled(0x00));
        assert!(!enabled(0x10), "sprites alone do not show background tiles");
        assert!(enabled(0x08));
        assert!(enabled(0x1E));
    }

    #[test]
    fn score_digits_land_after_the_nametable_write() {
        let mut delay = ScoreDelay::new(5);
        let old = [0x00, 0x00, 0x00];
        let new = [0x00, 0x01, 0x00];
        assert_eq!(delay.step(old), old);
        // The measured lag: the oracle still shows 000000 for five frames.
        for frame in 0..5 {
            assert_eq!(delay.step(new), old, "frame {frame} is still early");
        }
        assert_eq!(delay.step(new), new);
    }

    #[test]
    fn the_score_delay_cycles_through_the_measured_five_four_three_eight() {
        let mut delay = ScoreDelay::with_pattern(&[5, 4, 3, 8]);
        let mut digits = [0u8; 3];
        for expected in [5i32, 4, 3, 8] {
            digits[2] = digits[2].wrapping_add(1);
            let mut frames = 0;
            loop {
                frames += 1;
                if delay.step(digits) == digits {
                    break;
                }
                assert!(frames < 20, "the redraw never lands");
            }
            assert_eq!(frames, expected + 1, "change {digits:?}");
        }
    }

    #[test]
    fn a_held_score_is_stable_across_unchanged_frames() {
        let mut delay = ScoreDelay::new(3);
        let digits = [0x00, 0x00, 0x40];
        for _ in 0..8 {
            delay.step(digits);
        }
        let settled = delay.step(digits);
        for _ in 0..30 {
            assert_eq!(delay.step(digits), settled, "unchanged input never redraws");
        }
    }

    fn band_assets() -> Assets {
        let mut colors = vec![[0u8; 3]; 64];
        colors[0x0F] = [0, 0, 0];
        colors[0x12] = [40, 50, 60];
        colors[0x30] = [70, 80, 90];
        let mut tiles = vec![[0u8; 64]; 8 * 256];
        for index in 0..256 {
            tiles[2 * 256 + index] = [2; 64];
        }
        Assets {
            tiles,
            colors,
            ..Default::default()
        }
    }

    #[test]
    fn the_bottom_band_covers_exactly_rows_210_to_239() {
        let assets = band_assets();
        let meter = PowerMeter::default();
        let mut fb = Framebuffer::new();
        // `Game::render` paints the whole band black before the tile rows.
        fb.rect(
            0,
            HUD_Y,
            crate::video::WIDTH as i32,
            HUD_HEIGHT,
            assets.color(0x0F),
        );
        render(
            &mut fb,
            &assets,
            &Hud::new([0x12, 0x34, 0x56], [0x00, 0x50, 0x00], 3, 2, &meter),
        );
        for y in 0..HUD_Y as usize {
            for x in 0..crate::video::WIDTH {
                let offset = (y * crate::video::WIDTH + x) * 4;
                assert_eq!(fb.as_rgba()[offset + 3], 0, "row {y} is outside the band");
            }
        }
        for y in HUD_Y as usize..crate::video::HEIGHT {
            for x in 0..crate::video::WIDTH {
                let expected = if BAND_STARS.contains(&(x as i32, y as i32)) {
                    [40, 50, 60, 0xFF] // palette 1, the extracted star pixels
                } else if (METER_Y as usize..STATUS_Y as usize + 8).contains(&y) {
                    [70, 80, 90, 0xFF] // palette 2, the two tile rows
                } else {
                    [0, 0, 0, 0xFF] // black band fill
                };
                let offset = (y * crate::video::WIDTH + x) * 4;
                assert_eq!(
                    &fb.as_rgba()[offset..offset + 4],
                    &expected,
                    "pixel {x},{y}"
                );
            }
        }
    }

    #[test]
    fn the_meter_row_carries_the_six_labels() {
        let row = &tables().hud_meter_row;
        assert_eq!(&row[4..8], &[0x09, 0x0A, 0x0B, 0x0C]);
        assert_eq!(&row[25..28], &[0x62, 0x63, 0x1F]);
    }

    #[test]
    fn the_status_row_prints_the_high_score_in_seven_digits() {
        let meter = PowerMeter::default();
        let mut hud = Hud::new([0x00, 0x00, 0x40], [0x00, 0x50, 0x00], 3, 0, &meter);
        let tiles = status_row(&hud, &tables().hud_status_row);
        // `$8052` seeds `$07E1 = $50`, so the cartridge shows 0050000.
        assert_eq!(
            &tiles[HIGH_SCORE_COLUMN..HIGH_SCORE_COLUMN + 7],
            &[0x30, 0x30, 0x35, 0x30, 0x30, 0x30, 0x30]
        );
        hud.high_score = [0x01, 0x23, 0x45];
        assert_eq!(
            &status_row(&hud, &tables().hud_status_row)[HIGH_SCORE_COLUMN..HIGH_SCORE_COLUMN + 7],
            &[0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x30]
        );
    }

    /// Tiles carry their index as a pixel value, and every master-palette
    /// index has its own color, so a different digit changes rendered pixels.
    fn digit_assets() -> Assets {
        let colors = (0..64)
            .map(|i| [i as u8, i as u8 * 3, 255 - i as u8])
            .collect();
        let mut tiles = vec![[0u8; 64]; 8 * 256];
        for index in 0..256 {
            tiles[2 * 256 + index] = [index as u8 & 3; 64];
        }
        Assets {
            tiles,
            colors,
            ..Default::default()
        }
    }

    #[test]
    fn a_high_score_change_only_repaints_the_hi_region() {
        let assets = digit_assets();
        let meter = PowerMeter::default();
        let mut before = Framebuffer::new();
        let mut after = Framebuffer::new();
        render(
            &mut before,
            &assets,
            &Hud::new([0, 0, 0], [0x00, 0x50, 0x00], 3, 0, &meter),
        );
        render(
            &mut after,
            &assets,
            &Hud::new([0, 0, 0], [0x12, 0x34, 0x56], 3, 0, &meter),
        );
        let left = (HIGH_SCORE_COLUMN as i32 * 8) as usize;
        let right = left + 7 * 8;
        let top = STATUS_Y as usize;
        let bottom = top + 8;
        let mut changed = 0;
        for y in 0..crate::video::HEIGHT {
            for x in 0..crate::video::WIDTH {
                let offset = (y * crate::video::WIDTH + x) * 4;
                if before.as_rgba()[offset..offset + 4] == after.as_rgba()[offset..offset + 4] {
                    continue;
                }
                changed += 1;
                assert!(
                    (left..right).contains(&x) && (top..bottom).contains(&y),
                    "high score repainted pixel {x},{y}"
                );
            }
        }
        assert!(changed > 0, "the high score digits are visible");
    }

    fn meter_palette(meter: &PowerMeter, x: usize) -> [u8; 3] {
        let assets = Assets {
            tiles: vec![[1u8; 64]; 8 * 256],
            colors: (0..64).map(|i| [i, 0, 0]).collect(),
            ..Default::default()
        };
        let mut fb = Framebuffer::new();
        render(
            &mut fb,
            &assets,
            &Hud::new([0, 0, 0], [0x00, 0x50, 0x00], 3, 0, meter),
        );
        let offset = (METER_Y as usize * 256 + x) * 4;
        [
            fb.as_rgba()[offset],
            fb.as_rgba()[offset + 1],
            fb.as_rgba()[offset + 2],
        ]
    }

    #[test]
    fn the_meter_highlights_the_selected_entry_with_palette_one() {
        let assets = Assets {
            tiles: vec![[1u8; 64]; 8 * 256],
            colors: (0..64).map(|i| [i, 0, 0]).collect(),
            ..Default::default()
        };
        let normal = assets.color(tables().palettes.background[0][1]);
        let lit = assets.color(tables().palettes.background[1][1]);

        let mut meter = PowerMeter::default();
        for entry in 0..crate::powerup::SLOT_COUNT {
            let x = 32 + entry * 32;
            assert_eq!(meter_palette(&meter, x), normal, "entry {entry} unselected");
            meter.select(entry as u8 + 1);
            assert_eq!(meter_palette(&meter, x), lit, "entry {entry} selected");
            assert_eq!(
                meter_palette(&meter, x + 32),
                normal,
                "the next entry stays unselected"
            );
        }
        meter.select(0);
        assert_eq!(meter_palette(&meter, 32), normal, "nothing selected");
    }
}
