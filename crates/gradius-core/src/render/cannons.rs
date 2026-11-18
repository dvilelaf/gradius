//! `$8BDE`/`$8C0B`: the stage-5 `$0600` cannon arms.
//!
//! `$8B92` takes this branch only when `$19 == 4`. After the `$0120` object
//! slots, `$8B15` walks the four `$0600` records from `+$90` down and writes
//! one 8x16 part per placement (`$0618/$0620` runs). Part 5 is the tip: its
//! tile and attribute come from the part-5 phase (`$0615`), and attribute bit
//! 7 drops the stored Y by 8. Parts 0-4 are the `$F7` segment in palette 1,
//! except part 2 (the weak point) in palette 2.

use super::*;

pub(super) fn push_cannon_arms(
    list: &mut SpriteList,
    assets: &Assets,
    cannons: &[Cannon],
    hardware_bank: u8,
) {
    let Some(rom) = assets.rom else {
        return;
    };
    let sprite_palettes = [
        palettes::player(&rom.tables),
        palettes::enemy(&rom.tables),
        palettes::shot(&rom.tables),
        palettes::hud_sprite(&rom.tables),
    ];
    // `$8BE2`: `$A8` starts at `$90` and steps down by the `$30` record
    // stride, so slot 3 of the port's array is drawn first.
    for cannon in cannons.iter().rev().filter(|cannon| cannon.active) {
        // `$8C13`: the tip's tile and attribute read the part-5 phase once.
        let phase = cannon.phase[CANNON_PARTS - 1];
        let tip_tile = rom.tables.cannon_tip[usize::from((phase >> 2) & 0x0F)];
        let tip_attr = rom.tables.cannon_tip_attr[usize::from((phase >> 4) & 0x03)];
        for part in 0..CANNON_PARTS {
            // `$8C2C`: a zero or `>= $F4` X byte is not written at all.
            let x = cannon.part_x[part] as u8;
            if x == 0 || x >= 0xF4 {
                continue;
            }
            let y = cannon.part_y[part] as u8;
            let (tile, attr, y) = if part == CANNON_PARTS - 1 {
                // `$8C4E`: a vertically flipped tip is anchored 8 px higher.
                let y = if tip_attr & 0x80 != 0 {
                    y.wrapping_sub(8)
                } else {
                    y
                };
                (tip_tile, tip_attr, y)
            } else {
                // `$8C5C`: every segment is `$F7`, palette 1; part 2 reads
                // palette 2 because it is the only weak point.
                (0xF7, if part == 2 { 0x02 } else { 0x01 }, y)
            };
            list.push(Sprite {
                x: i32::from(x),
                y: i32::from(y) + 1,
                tile: crate::assets::TileRef {
                    // 8x16 mode: the tile LSB selects the pattern table.
                    bank: hardware_bank * 2 + (tile & 1),
                    index: tile & 0xFE,
                },
                palette: palette(assets, &sprite_palettes[usize::from(attr & 3)]),
                flip_h: attr & 0x40 != 0,
                flip_v: attr & 0x80 != 0,
                behind: attr & 0x20 != 0,
            });
        }
    }
}
