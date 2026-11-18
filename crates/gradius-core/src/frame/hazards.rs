//! Stage hazards, scripted spawns and the boss lifecycle.
//!
//! Split from `frame.rs` (card R1n); every function keeps its name, its body
//! and its relative order.

use super::*;

impl Game {
    pub(in crate::game) fn eruption_spawn(&mut self) {
        // `$C40D`: `$02 & 3 == 0`. `$02 = frame - 5`, so this is
        // `frame % 4 == 1`.
        if self.frame % 4 != 1 {
            return;
        }
        if self.section == 1 {
            self.stage_two_hazard();
            return;
        }
        if self.section == 2 {
            self.stage_three_script();
            return;
        }
        if self.section == 4 {
            self.stage_five_script();
            return;
        }
        let Some(slot) = self.free_slot() else {
            return;
        };
        let nibble = self.hazard_nibble(&self.rom().tables.eruption_script);
        // `$C480`: `$C4EE[$69 & 1]` picks the X edge.
        let parity = self.hazard_index & 1;
        let (x_int, mut y_int, accel) = self.rom().spawns.eruption_velocity[usize::from(nibble)];
        if self.hazard_index < 0x1E {
            y_int = y_int.wrapping_sub(2);
        }
        if self.hazard_index < 0x0A {
            y_int = y_int.wrapping_sub(2);
        }
        // `$C4DE`: both velocity fractions are `$02 & $3F`.
        let fraction = i16::from(self.frame.wrapping_sub(5) as u8 & 0x3F);
        self.objects[slot] = Object {
            active: true,
            kind: 0x0A,
            x: Fixed::px(tables::ERUPTION_X[usize::from(parity)]),
            y: Fixed::px(0x90),
            x_vel: (i16::from(x_int) << 8) | fraction,
            y_vel: (i16::from(y_int) << 8) | fraction,
            ai_state: accel,
            ai_mode: 1,
            sprite: 0x58,
            ..Object::default()
        };
    }

    /// `$C540`: every 8th frame (`$02 & 7 == 0`) spawns kind `$0B` from the
    /// `$C567` position table, indexed by the `$C587` nibble script.
    fn stage_two_hazard(&mut self) {
        if self.frame.wrapping_sub(5) & 7 != 0 {
            return;
        }
        let Some(slot) = self.free_slot() else {
            return;
        };
        let nibble = self.hazard_nibble(&self.rom().tables.hazard_script);
        let (x, y) = self.rom().tables.hazard_positions[usize::from(nibble)];
        self.objects[slot] = Object {
            active: true,
            kind: 0x0B,
            x: Fixed::px(x),
            y: Fixed::px(y),
            sprite: 0x67,
            ..Object::default()
        };
        self.spawn_busy = true;
    }

    /// `$C680`: stage 3's `$C433[2]` scripted object. It counts `$C40D`
    /// calls in `$68` and, every `$C67E[$3A]`, spawns the `$C6C4`/`$C6C6`
    /// entry at the `$C6C8` Y while the scroll is still before `$0E`.
    /// `$3A` is zero on the normal path, so this is the small-core branch.
    pub(in crate::game) fn stage_three_script(&mut self) {
        let Some(slot) = self.free_slot() else {
            return;
        };
        self.script_counter = self.script_counter.wrapping_add(1);
        if self.script_counter < STAGE3_SCRIPT_PERIOD {
            return;
        }
        if self.scroll.high >= 0x0E {
            return;
        }
        self.script_counter = 0;
        let index = self.hazard_index & 0x0F;
        self.hazard_index = self.hazard_index.wrapping_add(1);
        self.objects[slot] = Object {
            active: true,
            kind: STAGE3_SCRIPT_KIND,
            subtype: 0x80,
            x: Fixed::px(0xF0),
            y: Fixed::px(self.rom().tables.stage3_script_y[usize::from(index)]),
            sprite: STAGE3_SCRIPT_SPRITE,
            size_class: 1,
            ..Object::default()
        };
        self.spawn_busy = true;
    }

    /// `$C449`: advance `$69`, index `script` with the *old* value and select
    /// its high nibble on an even post-increment parity, low on odd.
    pub(super) fn hazard_nibble(&mut self, script: &[u8]) -> u8 {
        let old = if self.hazard_index == 0xFF {
            0x7F
        } else {
            self.hazard_index
        };
        let index = old.wrapping_add(1);
        self.hazard_index = index;
        let byte = script[usize::from((old & 0x3F) >> 1)];
        if index & 1 == 0 {
            byte >> 4
        } else {
            byte & 0x0F
        }
    }

    /// Spawn the stage-1 Big Core at the documented entry point.
    pub(in crate::game) fn spawn_boss(&mut self) {
        // `$B91F` writes `$0460,X = 3` (16x2) every handler frame; the spawn's
        // `$A521` clear only matters before that first update.
        self.objects[9] = Object {
            active: true,
            kind: 0x98,
            subtype: 0x80,
            x: Fixed::px(0xF0),
            y: Fixed::px(0x80),
            sprite: 0x6C,
            size_class: 3,
            boss: true,
            ..Object::default()
        };
        // `$B9B1`: the core creates its two `$99` covers in slots 7 and 8,
        // sprites `$85` and `$32`, the second `$14` px above the first.
        self.objects[7] = Object {
            active: true,
            kind: 0x99,
            subtype: 0x80,
            x: Fixed::px(0xF0),
            y: Fixed::px(0x80),
            sprite: 0x32,
            ..Object::default()
        };
        self.objects[8] = Object {
            active: true,
            kind: 0x99,
            subtype: 0x80,
            x: Fixed::px(0xF0),
            y: Fixed::px(0x80 - 0x14),
            sprite: 0x85,
            // `$B9B1`'s `$045F,X` with X = 9 lands in slot 8's `$0460`, so
            // the upper cover keeps the ROM's inherited size class 2.
            size_class: 2,
            ..Object::default()
        };
        self.boss_spawned = true;
        self.spawn_busy = true;
        self.audio.push(Sound::EnemyShot);
    }

    /// `$A460`: the five-byte command path.
    pub(in crate::game) fn command(&mut self, bytes: [u8; 5]) {
        // `$A387`: `$64 -= $70`.
        let subtype = bytes[1].wrapping_sub(0x70);
        if self.section == 2 {
            // `$A469`: the stage-3 command creates the `$96` background
            // turret. `$64 -> $010C`, `$65 -> $032C` (Y), `$66:$67 ->
            // $03BC:$03EC`, kind `$96`, X `$F0`. `$A521` leaves the sprite,
            // size class and boss flag clear; this is not the end-of-stage
            // core.
            let Some(slot) = (0..LIVE_SLOTS)
                .rev()
                .find(|&slot| !self.objects[slot].live())
            else {
                return;
            };
            self.objects[slot] = Object {
                active: true,
                kind: 0x96,
                subtype,
                x: Fixed::px(0xF0),
                y: Fixed::px(bytes[2]),
                nametable_addr: u16::from_be_bytes([bytes[3], bytes[4]]),
                ..Object::default()
            };
            self.spawn_busy = true;
        } else {
            // `$A4A0`: a large object (`$66`) plus one `$0600` record per
            // non-zero nibble of `$65`, low nibble first. `$016C` (`anim`)
            // stores the record count.
            let low = bytes[2] & 0x0F;
            let high = bytes[2] >> 4;
            let kind = bytes[3];
            let y = bytes[4];
            let Some(slot) = (0..LIVE_SLOTS)
                .rev()
                .find(|&slot| !self.objects[slot].live())
            else {
                return;
            };
            let mut records = 0u8;
            for nibble in [low, high] {
                if nibble == 0 {
                    continue;
                }
                let Some(cannon) = (0..CANNON_SLOTS)
                    .rev()
                    .find(|&cannon| !self.cannons[cannon].active)
                else {
                    continue;
                };
                self.cannons[cannon] = Cannon {
                    active: true,
                    parent: slot as u8,
                    index: nibble - 1,
                    ..Cannon::default()
                };
                records = records.wrapping_add(1);
            }
            self.objects[slot] = Object {
                active: true,
                kind,
                subtype: 0x80,
                x: Fixed::px(0xF0),
                y: Fixed::px(y),
                sprite: 0x89,
                size_class: 1,
                anim: records,
                ..Object::default()
            };
            self.spawn_busy = true;
        }
    }

    /// `$AF70`/`$AF78`: a stage-1 volcano death (`$046C >= 5`). The fourth
    /// kill sets `$39`, the stage-1 clear gate used at `$9928`.
    pub(in crate::game) fn volcano_kill(&mut self) {
        if self.section != 0 {
            return;
        }
        self.volcano_kills = self.volcano_kills.wrapping_add(1);
        if self.volcano_kills >= 4 {
            self.stage_clear_ready = true;
        }
    }

    /// `$AF92`: allocate one child object at a relative position.
    pub(in crate::game) fn spawn_child(&mut self, x: u8, y: u8, kind: u8, size_class: u8) {
        let Some(slot) = (0..LIVE_SLOTS)
            .rev()
            .find(|&slot| !self.objects[slot].live())
        else {
            return;
        };
        let motion =
            tables::CHILD_MOTION[usize::from(self.section).min(tables::CHILD_MOTION.len() - 1)];
        self.objects[slot] = Object {
            active: true,
            kind,
            x: Fixed::px(x),
            y: Fixed::px(y),
            size_class,
            phase: motion & 1,
            ..Object::default()
        };
    }

    /// `$C776`: the Big Core's death: score, explosion, stage clear.
    pub(in crate::game) fn destroy_boss(&mut self, slot: usize) {
        let object = self.objects[slot];
        // `$C784`/`$8454` is the 500 path plus the recorded Big Core bonus:
        // `xtask record-boss` shows 1690 -> 2690 across the death frame.
        self.add_score(1000);
        self.kills = self.kills.wrapping_add(1);
        self.add_explosion(i32::from(object.x.int), i32::from(object.y.int), 20);
        self.audio.push(Sound::Explosion);
        self.objects[slot].active = false;
        self.objects[slot].kind = 0;
        // `$B95C`: the two `$99` covers go with the core.
        self.objects[7] = Object::default();
        self.objects[8] = Object::default();
        // `$B95C`: with the player alive the stage-clear sequence starts; the
        // scroll holds this frame and resumes on the next, and stage 2 begins
        // 514 frames after the kill.
        self.scroll_locked = false;
        self.scroll_stop = true;
        self.start_stage_clear();
    }
}
