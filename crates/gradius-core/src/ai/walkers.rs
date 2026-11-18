//! The `$87`/`$93` ceiling and ground walkers and the terrain solidity
//! probes.
//!
//! Split from `ai.rs` by family (card R1e), bodies verbatim.

use super::*;
use crate::stage1_tables as tables;

impl Game {
    /// `$B6DB`/`$B741`: walk along the terrain, 3 px/frame, then drift.
    pub(super) fn ai_walker(&mut self, object: &mut Object, probe_offset: u8) {
        // `$B6DB` (kind 7) and `$B741` (kind 19); `$B6E2`/`$B76E` init.
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            if probe_offset != 8 {
                object.ai_mode = 1;
            }
            object.ai_state = self.walker_target_x();
            return;
        }
        if object.ai_timer & 1 != 0 {
            // `$B71D`: drift, refresh the sprite, then retarget after `$3C`.
            self.ai_drift(object);
            self.walker_sprite(object);
            object.timer2 = object.timer2.wrapping_add(1);
            if object.timer2 < 0x3C {
                return;
            }
            object.ai_timer = object.ai_timer.wrapping_add(1);
            if object.ai_timer < 7 {
                object.ai_state = self.walker_target_x();
            } else {
                object.ai_state = 0;
            }
            return;
        }
        // `$C3CD`: probe two rows for solidity before stepping vertically.
        if probe_offset == 8 {
            // Kind 7 probes below the piece (`$B6EF`).
            if !self.solid_at(object.x.int, object.y.int.wrapping_add(8)) {
                object.y.add(0x300);
            } else if self.solid_at(object.x.int, object.y.int.wrapping_add(5)) {
                object.y.add(-0x300);
            }
        } else {
            // Kind 19 probes above it (`$B74F`).
            if !self.solid_at(object.x.int, object.y.int.wrapping_sub(8)) {
                object.y.add(-0x300);
            } else if self.solid_at(object.x.int, object.y.int.wrapping_sub(5)) {
                object.y.add(0x300);
            }
        }
        self.walker_advance(object);
    }

    /// `$B670`: step toward `$048C` one pixel and pick the walk frame.
    fn walker_advance(&mut self, object: &mut Object) {
        let rounded = object.x.int & 0xF8;
        if rounded == object.ai_state {
            // `$B69C`: aligned, so stop stepping and show the fixed frame.
            object.ai_timer = object.ai_timer.wrapping_add(1);
            object.subtype = 0;
            object.timer2 = 0;
            self.walker_sprite(object);
        } else if rounded < object.ai_state {
            // `$B691`: one pixel right.
            object.x.int = object.x.int.wrapping_add(1);
            object.subtype = 4;
        } else {
            // `$B68D`: two pixels left, or despawn once past `$08`.
            object.x.int = object.x.int.wrapping_sub(2);
            if object.x.int < 8 {
                object.despawn();
            } else {
                object.subtype = 3;
            }
        }
    }

    /// `$B6B2`: the walker frame depends on the facing and the mode.
    fn walker_sprite(&mut self, object: &mut Object) {
        let mut index = usize::from(object.ai_mode);
        if object.x.int < self.player.x.int {
            index += 2;
        }
        object.sprite = tables::WALKER_FRAMES[index.min(tables::WALKER_FRAMES.len() - 1)];
    }

    fn walker_target_x(&self) -> u8 {
        ((self.player.x.int.wrapping_add(0x30)) & 0xF8).clamp(0x20, 0xF0)
    }

    /// `$C3CD`: stage-1 background solidity at a screen pixel.
    pub(in crate::game) fn solid_at(&self, x: u8, y: u8) -> bool {
        self.terrain_solid(i32::from(x), i32::from(y))
    }

    /// `$C3CD` against the current stage's packed collision map.
    pub(in crate::game) fn terrain_solid(&self, x: i32, y: i32) -> bool {
        self.terrain_probe(x, y)
            .is_some_and(|(_, _, field)| field != 0)
    }

    /// `$C3CD` with the broken cells applied: `(column, row, field)`.
    pub(in crate::game) fn terrain_probe(&self, x: i32, y: i32) -> Option<(usize, usize, u8)> {
        let (column, row, field) = terrain_probe_in(
            crate::stage_collision::map(self.rom(), usize::from(self.section)),
            x,
            y,
            self.scroll,
        )?;
        let broken = self.broken[column] >> row & 1 != 0;
        // `$B60C`: a painted door row is solid for good (`$FF`/`$C3`), and the
        // wall writes it with the same paint that shows the tile.
        let door = self.section == crate::stage7_door::SECTION
            && crate::stage7_door::covers(column, row)
            && self
                .cells
                .iter()
                .any(|cell| usize::from(cell.row) == row && usize::from(cell.world_col) == column);
        Some((
            column,
            row,
            if door {
                3
            } else if broken {
                0
            } else {
                field
            },
        ))
    }
}
