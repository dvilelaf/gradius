//! The `$91`/`$92` aiming turrets, the `$AF28`/`$AF82` volcano turrets,
//! the `$C900` background turret and the `$0600` cannons.
//!
//! Split from `ai.rs` by family (card R1e), bodies verbatim.

use super::*;
use crate::stage1_tables as tables;

impl Game {
    /// `$CB48`: clear every `$0600` record of this parent and leave a kind-2
    /// debris object at each cannon position.
    pub(super) fn release_cannons(&mut self, parent: u8) {
        for index in 0..CANNON_SLOTS {
            if !self.cannons[index].active || self.cannons[index].parent != parent {
                continue;
            }
            let (x, y) = (self.cannons[index].part_x[0], self.cannons[index].part_y[0]);
            self.cannons[index] = Cannon::default();
            if let Some(slot) = (0..=7).rev().find(|&slot| !self.objects[slot].live()) {
                self.objects[slot] = Object {
                    active: true,
                    kind: 2,
                    x: Fixed::px(x as u8),
                    y: Fixed::px(y as u8),
                    anim: 1,
                    anim_div: 0,
                    ..Object::default()
                };
            }
        }
    }

    /// `$CB8B`: place and fire the attached `$0600` cannons. Called from the
    /// stage-5 `$19 == 4` alternate chain on odd `$02` frames (two or more
    /// live) and from the `$9A69` tail on every frame while fewer do.
    pub(in crate::game) fn update_cannons(&mut self) {
        let period = CANNON_PERIOD[usize::from(self.stage_rank()).min(CANNON_PERIOD.len() - 1)];
        let mut fired = false;
        for index in (0..CANNON_SLOTS).rev() {
            let Some(mut cannon) = Some(self.cannons[index]).filter(|c| c.active) else {
                continue;
            };
            let parent = usize::from(cannon.parent);
            if !self.objects[parent].live() {
                self.cannons[index] = Cannon::default();
                continue;
            }
            // `$CC32`: `$0603` counts down on every call and `$CC2D` places the
            // six parts only when the new value is even, so the parts move on
            // every second `$CB8B` call. `$A4A0` starts the counter at zero, so
            // the first call after creation skips the placement.
            cannon.part_counter = cannon.part_counter.wrapping_sub(1);
            if cannon.part_counter & 1 == 0 {
                // `$CC40`: the parent origin plus the `$CC19`/`$CC1D` offsets.
                let parent_object = self.objects[parent];
                let offset = 4 * usize::from(parent_object.size_class.min(1))
                    + usize::from(cannon.index & 1);
                cannon.part_x[0] =
                    i32::from(parent_object.x.int.wrapping_add(CANNON_X_OFFSET[offset]));
                cannon.part_y[0] =
                    i32::from(parent_object.y.int.wrapping_add(CANNON_Y_OFFSET[offset]));
                // `$CC67`: the sway target is the player's column, skewed by the
                // record offset and the frame counter. Both adds carry.
                let base = index as u8 * 0x30;
                let sum = u16::from(base.wrapping_shl(1))
                    + u16::from(self.nmi_frame)
                    + u16::from(base >> 7);
                let target = ((sum as u8) & 0x7F)
                    .wrapping_add(self.player.x.int)
                    .wrapping_add(u8::from(sum > 0xFF));
                let target = target & 0xF0;
                let odd = cannon.index & 1 != 0;
                // `$9B`/`$9C` start as the index's `$CC19`/`$CC1B` range, but
                // `$CCF1` rewrites them from the part just placed, so every
                // later part is clamped to the previous phase +/- 6.
                let (mut lo, mut hi) = (
                    CANNON_PHASE_MIN[usize::from(cannon.index & 1)],
                    CANNON_PHASE_MAX[usize::from(cannon.index & 1)],
                );
                for part in 1..CANNON_PARTS {
                    // `$CC93`: `X` advances with the part, so every part owns a
                    // phase byte and sways on its own. Step it toward the target
                    // column, then clamp to the current `$9B`/`$9C` window.
                    let mut phase = cannon.phase[part];
                    let high = (cannon.part_x[part] as u8) & 0xF0;
                    phase = if odd {
                        if high <= target {
                            phase.wrapping_sub(1)
                        } else {
                            phase.wrapping_add(1)
                        }
                    } else if high < target {
                        phase.wrapping_add(1)
                    } else if high > target {
                        phase.wrapping_sub(1)
                    } else {
                        phase
                    };
                    if phase < lo {
                        phase = lo;
                    }
                    if phase >= hi {
                        phase = hi;
                    }
                    // `$CCCA`: an odd record pins a high phase to `$7C` when
                    // its part is at or below the ship; an even record pins a
                    // low phase to `$44` when the part is above it.
                    let part_y = cannon.part_y[part] as u8;
                    if odd {
                        if phase >= 0x84 && part_y >= self.player.y.int {
                            phase = 0x7C;
                        }
                    } else if phase < 0x3C && part_y < self.player.y.int {
                        phase = 0x44;
                    }
                    cannon.phase[part] = phase;
                    hi = phase.wrapping_add(6);
                    lo = phase.wrapping_sub(6);
                    // `$CD00`: the table index is `$0611 & $1F`; bit 5 selects the
                    // subtract direction for both axes.
                    let step = usize::from(phase & 0x1F);
                    let (mut x, y0) =
                        (cannon.part_x[part - 1] as u8, cannon.part_y[part - 1] as u8);
                    let (step_x, step_y) = (
                        self.rom().tables.cannon_step_x[step],
                        self.rom().tables.cannon_step_y[step],
                    );
                    if phase & 0x20 != 0 {
                        x = x.wrapping_sub(step_x);
                    } else {
                        x = x.wrapping_add(step_x);
                    }
                    let y = if phase & 0x20 != 0 {
                        y0.wrapping_sub(step_y)
                    } else {
                        y0.wrapping_add(step_y)
                    };
                    // `$CD23`/`$CD34`: a part that wraps past the screen edge is
                    // pinned to column 0 while the parent is still off-screen.
                    if x < 0x10 && parent_object.x.int >= 0xA0
                        || x >= 0xF0 && parent_object.x.int < 0x30
                    {
                        cannon.part_x[part] = 0;
                    } else {
                        cannon.part_x[part] = i32::from(x);
                    }
                    cannon.part_y[part] = i32::from(y);
                }
            }
            cannon.timer = cannon.timer.wrapping_add(1);
            if cannon.timer >= period && !fired {
                fired = true;
                cannon.timer = 0;
                // `$CBCB`: one small bullet from the tip (part 5), eight pixels
                // up and left, aimed by the shared `$BCAF` helper. The tip's
                // `$061D`/`$0625` must be on screen (`$10 <= x < $F0`,
                // `y < $D0`) or the shot is skipped.
                let x = cannon.part_x[CANNON_PARTS - 1] as u8;
                let y = cannon.part_y[CANNON_PARTS - 1] as u8;
                if (0x10..0xF0).contains(&x) && y < 0xD0 {
                    self.push_enemy_bullet(x.wrapping_sub(8), y.wrapping_sub(8), 0, 0x86);
                }
            }
            self.cannons[index] = cannon;
        }
    }

    /// `$9658`: with two or more live cannons on an odd `$02` frame, the
    /// stage-5 chain runs the spawner/AI/player itself and sets `$5B`, which
    /// halves both the scroll and the object drift.
    pub(in crate::game) fn live_cannon_count(&self) -> usize {
        self.cannons.iter().filter(|cannon| cannon.active).count()
    }

    /// `$C900`: the stage-3 background turret.
    ///
    /// `$048C == 0` holds the alternate (`4*type + $10`) art and is not
    /// damageable. Passing `$C954` opens it: state `$14`, the base art and a
    /// cleared hit counter. When the `$C930` cooldown expires it closes again,
    /// rewrites the alternate art and becomes invulnerable.
    pub(in crate::game) fn ai_background_turret(&mut self, object: &mut Object) {
        let type_low = object.subtype & 0x0F;
        if object.ai_timer >= 3 {
            self.turret_death(object);
            return;
        }
        // `$AED7`: frozen while `$5B` is set. The despawn check only runs with
        // the drift, exactly like the shared helper.
        if !self.scroll_stop && !self.scroll_locked {
            object.drift();
            if object.x.int < 0x08 {
                object.despawn();
                return;
            }
        }
        // `$5D != 0` and `$0E >= 4`: a spawn/effect or an earlier turret's
        // queue record blocks this frame's paint.
        if self.spawn_busy || self.vram_queue_len >= 4 {
            return;
        }
        if object.ai_mode != 0 {
            object.ai_mode -= 1;
            return;
        }
        let group = if object.ai_state != 0 {
            // `$C9A4`: `$14 -> 0`, clear the hit counter and paint alternate.
            object.ai_state = 0;
            object.ai_timer = 0;
            object.timer2 = 0;
            self.rom().tables.turret_groups[usize::from(type_low) + 4]
        } else {
            if !stage3_enemies::in_sector(
                type_low,
                i32::from(self.player.x.int),
                i32::from(self.player.y.int),
                i32::from(object.x.int),
                i32::from(object.y.int),
            ) {
                return;
            }
            // `$C999`: open, clear `$046C`, paint the base art.
            self.spawn_busy = true;
            object.x_vel = 0;
            object.ai_state = 0x14;
            object.timer2 = 0x14;
            self.rom().tables.turret_groups[usize::from(type_low)]
        };
        object.ai_mode = stage3_enemies::cooldown(self.stage_rank());
        self.paint_turret(object, type_low, group);
    }

    /// `$C9B4`: queue the two `$0700` records as world-keyed cells.
    fn paint_turret(&mut self, object: &Object, type_low: u8, group: [u8; 4]) {
        let scroll = self.scroll.position();
        self.cells.extend(stage3_enemies::paint_cells(
            object.nametable_addr,
            type_low,
            group,
            scroll,
        ));
        self.vram_queue_len = self
            .vram_queue_len
            .saturating_add(stage3_enemies::queued_bytes(group));
    }

    /// `$C776`: turret death, the ten-kill gate and the nametable explosion.
    fn turret_death(&mut self, object: &mut Object) {
        self.volcano_kills = self.volcano_kills.wrapping_add(1);
        if self.volcano_kills >= 10 {
            self.stage_clear_ready = true;
        }
        self.spawn_busy = true;
        if !self.demo {
            self.add_score(300);
        }
        let scroll = self.scroll.position();
        self.collision_cells
            .extend(stage3_enemies::collision_patches(
                object.subtype & 0x0F,
                object.nametable_addr,
            ));
        self.cells.extend(stage3_enemies::explosion_cells(
            self.rom(),
            object.subtype & 0x0F,
            object.nametable_addr,
            scroll,
        ));
        self.audio.push(Sound::Explosion);
        self.turn_to_debris(object);
    }

    /// `$AF28`/`$AF82`: a turret that launches a `$B30B` shell and dies.
    pub(super) fn ai_turret_fire(&mut self, object: &mut Object, kind: u8) {
        // `$AF28`/`$AF82`: the init branch sets `$0460 = 1`, `$048C = 1` and
        // `$010C = $80`, then `$B0AE` adds the bit-7 initialised flag `$80`.
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.size_class = 1;
            object.ai_state = 1;
            object.subtype = 0x80;
            return;
        }
        // `$AF44`: sprite `$78` (`$63` in section 5) or `$79`.
        // `$AF51`: after five hits the volcano breaks up and counts toward the
        // stage-1 `$39` gate. The check runs in the handler, one frame after
        // the shot that reached `$046C = 5`.
        if object.ai_timer >= 5 {
            self.volcano_kill();
            self.destroy(object, 0x0A);
            // `$CB22`: the volcano debris keeps `$012C`, starts the `$AE6B`
            // path at `$016C = 2` and clears `$014C`/`$03AC`, so it is in the
            // object table on the kill frame.
            object.anim = 2;
            object.anim_div = 0;
            object.phase = 0;
            object.sprite = match (kind, self.section) {
                (15, 5) => 0x63,
                (15, _) => 0x78,
                _ => 0x79,
            };
            return;
        }
        object.sprite = match (kind, self.section) {
            (15, 5) => 0x63,
            (15, _) => 0x78,
            _ => 0x79,
        };
        // `$AF92`: phase 0 fires at `x >= $C8`, phase 1 at `x >= $A0`; both
        // only on a `$02 & $0F == 0` frame. `$044C` counts the burst; the
        // fifth fire resets it and advances `$042C`.
        let gate = match object.ai_mode {
            0 => object.x.int >= 0xC8,
            1 => object.x.int >= 0xA0,
            _ => false,
        };
        // `$02` is the NMI counter, which holds on the cartridge's lag frames;
        // [`Game::nmi_frame`] carries the same value.
        if gate && self.nmi_frame & 0x0F == 0 {
            object.timer2 = object.timer2.wrapping_add(1);
            if object.timer2 >= 5 {
                object.timer2 = 0;
                object.ai_mode = object.ai_mode.wrapping_add(1);
            } else {
                let (shot, dy) = if kind == 15 { (9, 8) } else { (12, -10) };
                let x = object.x.int.wrapping_add(8);
                let y = object.y.int.wrapping_add_signed(dy);
                // `$AFD3`/`$A521` clears `$0460`, so the shell uses class 0.
                self.spawn_child(x, y, shot, 0);
            }
        }
        // `$AF25`: the surviving turret drifts.
        self.ai_drift(object);
    }

    /// `$B020`/`$B092`: an aiming turret. The sprite follows the ship sector.
    pub(in crate::game) fn ai_turret(&mut self, object: &mut Object, kind: u8) {
        object.kind = if kind == 17 { 0x91 } else { 0x92 };
        // `$B020` holds `$040C` at `$0A` while the turret is on its blind
        // side (`$B025`: above the ship for `$91`); `$B092` takes the same
        // `$B02D` write from below for `$92`. The write lands after `$BBB1`
        // in the frame, so it pins the next frame's countdown.
        let blind = if kind == 17 {
            object.y.int < self.player.y.int
        } else {
            object.y.int >= self.player.y.int
        };
        if blind {
            object.fire_timer = 0x0A;
        }
        let dx = object.x.int.wrapping_sub(self.player.x.int);
        // `$B032`..`$B051`: the right sector is `0` inside `$30` and `1`
        // beyond it; the left sector is `3`/`4`. The vertical block at
        // `$B053` only runs for the far sectors (`dx >= $60` on the right,
        // `|dx| > $60` on the left).
        let (mut index, vertical): (u8, bool) = if object.x.int >= self.player.x.int {
            if dx < 0x30 {
                (0, false)
            } else if dx < 0x60 {
                (1, false)
            } else {
                (1, true)
            }
        } else if dx >= 0xD0 {
            (3, false)
        } else if dx >= 0xA0 {
            (4, false)
        } else {
            (4, true)
        };
        let dy = object.y.int.wrapping_sub(self.player.y.int);
        let near_y = (object.y.int >= self.player.y.int && dy < 0x30)
            || (object.y.int < self.player.y.int && dy >= 0xD0);
        if vertical && near_y {
            index += 1;
        }
        // `$B067` stores `$B080,Y` (the visible ring/turret group) in
        // `$012C` and `$B06D-$B07A` the aim's muzzle index in `$0496`:
        // `$B086,Y` normally, `$B08C,Y` when `$018C` bit 7 is set (`$B09A`
        // sets it for the `$92` whole-body turret) and that entry is non-zero.
        let index = usize::from(index).min(5);
        object.sprite = tables::SPRITE_TURRET[index];
        let flagged = tables::SPRITE_TURRET_FLAG[index];
        object.muzzle = if object.kind == 0x92 && flagged != 0 {
            flagged
        } else {
            tables::SPRITE_TURRET_LEFT[index]
        };
        self.ai_drift(object);
    }
}
