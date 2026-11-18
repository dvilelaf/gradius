//! Which pack palette each part of the screen uses.
//!
//! The colours are the palette RAM rows the cartridge uploads (`$877F`,
//! `$879D`, `$87B0`, `$87DD`, `$878A`, `$87D4`); the extractor decodes those
//! `$85ED` text records into [`Tables::palettes`]. This module only names the
//! entries the renderer needs.

use crate::rom::Tables;

/// Stage background palettes (`$3F00..$3F0F`).
pub fn background(tables: &Tables) -> &[[u8; 4]; 4] {
    &tables.palettes.background
}

/// Stage sprite palettes (`$3F10..$3F1F`), in attribute `& 3` order.
pub fn stage_sprite(tables: &Tables) -> [[u8; 4]; 4] {
    tables.palettes.sprite
}

/// Vic Viper: sprite palette 0 (`$3F11..$3F13`), used by the middle and nose
/// sprites of the three-sprite ship. The rear sprite uses sprite palette 1.
pub fn player(tables: &Tables) -> [u8; 4] {
    tables.palettes.sprite[0]
}

/// Small enemies: sprite palette 1 (`$3F15..$3F17`). The ring enemy at
/// `$C0/$C1` was observed with attribute `$21`.
pub fn enemy(tables: &Tables) -> [u8; 4] {
    tables.palettes.sprite[1]
}

/// Player shot: sprite palette 2 (`$3F19..$3F1B`), attribute `$22`.
pub fn shot(tables: &Tables) -> [u8; 4] {
    tables.palettes.sprite[2]
}

/// HUD text: background palette 0, entry 2 (white). The power-meter bar is a
/// sprite and uses sprite palette 3.
pub fn hud(tables: &Tables) -> u8 {
    tables.palettes.background[0][2]
}

/// Stage sprite palette 3 (`$3F1D..$3F1F`): the HUD power-meter bar, the
/// explosion frames and the player engine flame.
pub fn hud_sprite(tables: &Tables) -> [u8; 4] {
    tables.palettes.sprite[3]
}

/// Title sprite palette 3 (`$3F1D..$3F1F`): entry 2 is black on the title
/// screen, unlike the stage's [`hud_sprite`]. The menu ship's engine flame uses
/// this palette.
pub fn title_sprite(tables: &Tables) -> [u8; 4] {
    tables.palettes.title_sprite[3]
}

/// The ending's `$8A` upload of `$3F10..$3F1F` (`$89E8`, power-up free): the
/// mothership and the `$28` ship use these sprite palettes, not the stage-1
/// gameplay set.
pub fn ending_sprite(tables: &Tables) -> &[[u8; 4]; 4] {
    &tables.palettes.ending_sprite
}
