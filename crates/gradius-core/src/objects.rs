//! The object slot table and the collision passes.
//!
//! Moved verbatim from `game.rs` (card R1d). The `Object` record, the slot
//! allocator, `spawn_object`, the `$BE8D` destruction path and the
//! `$BFDC`/`$C0FB`/`$C204` collision routines keep their bodies, names and
//! order; the only edits are the `pub(super)` markers the module boundary
//! needs.

use super::*;
use crate::audio::Sound;
use crate::stage::SpawnEvent;
use crate::stage1_tables as tables;

/// One object slot. The field names quote the original arrays.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Object {
    /// Live slot. The original tests `$030C != 0`, which [`Self::kind`] keeps
    /// identical: [`Object::live`] is the port's equivalent.
    pub(super) active: bool,
    /// `$030C`. Bit 7 is the "handler initialised" flag, exactly as the
    /// original handlers set it with `LDA #$80; ADC $030C,X`.
    pub(super) kind: u8,
    /// `$010C`. `$80..=$FF` marks a large object (damage-accumulating).
    pub(super) subtype: u8,
    /// `$036C`/`$038C`: X position, 8.8 fixed point.
    pub(super) x: Fixed,
    /// `$032C`/`$034C`: Y position, 8.8 fixed point.
    pub(super) y: Fixed,
    /// `$042C`/`$044C`: X velocity, signed 8.8.
    pub(super) x_vel: i16,
    /// `$03BC`/`$03EC`: Y velocity, signed 8.8.
    pub(super) y_vel: i16,
    /// `$03BC`/`$03EC`: the stage-3 turret's nametable address.
    pub(super) nametable_addr: u16,
    /// `$048C`: per-handler state / damage gate.
    pub(super) ai_state: u8,
    /// `$046C`: per-handler timer (also the large-object hit counter).
    pub(super) ai_timer: u8,
    /// `$04AC`: per-handler mode.
    pub(super) ai_mode: u8,
    /// `$04CC`: secondary timer.
    pub(super) timer2: u8,
    /// `$014C`: animation divider (`$ADEB`: 6).
    pub(super) anim_div: u8,
    /// `$016C`: animation frame index.
    pub(super) anim: u8,
    /// `$012C` enemy sprite type.
    pub(super) sprite: u8,
    /// `$040C`: frames left before this object fires (`$BBB1`).
    pub(super) fire_timer: u8,
    /// `$04EC`: non-zero enables the `$BC3E` fire, seeded from the spawn
    /// motion byte by `$A573` (`& $FE`).
    pub(super) fire_motion: u8,
    /// `$0496`: muzzle offset index into `$BC2C`/`$BC35`.
    pub(super) muzzle: u8,
    /// `$0460`: collision size class (`$BFD4`/`$BFD8`).
    pub(super) size_class: u8,
    /// `$03AC` option/attachment phase.
    pub(super) phase: u8,
    /// A 3-byte `$A65C` background/terrain piece.
    #[allow(dead_code)]
    pub(super) terrain: bool,
    /// The Big Core (`$96`/`$97`).
    pub(super) boss: bool,
}

impl Object {
    /// `$030C != 0`, the original's liveness test.
    pub(super) fn live(&self) -> bool {
        self.active && self.kind != 0
    }

    pub(super) fn move_vel_x(&mut self) {
        let delta = i32::from(self.x_vel);
        self.x.add(delta);
    }

    pub(super) fn move_vel_y(&mut self) {
        let delta = i32::from(self.y_vel);
        self.y.add(delta);
    }

    /// `$AEDB`: `$038C -= $80`, borrow into `$036C`.
    pub(super) fn drift(&mut self) {
        self.x.add(-0x80);
    }

    /// `$AEF2`: clear the slot without an explosion.
    pub(super) fn despawn(&mut self) {
        *self = Object::default();
    }

    pub(super) fn reset_velocities(&mut self, x: i16, y: i16) {
        self.x_vel = x;
        self.y_vel = y;
    }
}

impl Game {
    /// Highest free slot (`$C414`: `$A8 = 9` down to 0).
    pub(super) fn free_slot(&self) -> Option<usize> {
        (0..LIVE_SLOTS)
            .rev()
            .find(|&slot| !self.objects[slot].live())
    }

    /// `$A3AB`/`$A3DE`: allocate the first free slot and fill the fields.
    pub(super) fn spawn_object(&mut self, event: SpawnEvent) -> bool {
        // `$A3AB`/`$A411`: `LDX #$09` then decrement, so the highest free slot
        // is used first (the oracle's first two enemies sit in slots 9 and 8).
        let Some(slot) = (0..LIVE_SLOTS)
            .rev()
            .find(|&slot| !self.objects[slot].live())
        else {
            return false;
        };
        self.objects[slot] = Object {
            active: true,
            kind: event.kind,
            subtype: if event.terrain { 0 } else { event.subtype },
            x: Fixed::px(event.x),
            y: Fixed::px(event.y),
            phase: event.phase,
            // `$A521` clears `$012C`; `$ADDF` supplies the first frame in the
            // same frame the slot is allocated (its `$016C` starts at 0 and
            // is incremented before the table lookup).
            sprite: 0,
            // `$A573`: the spawn motion byte gates the `$BBB1` fire and seeds
            // its first countdown (both keep `& $FE`).
            fire_timer: event.motion & 0xFE,
            fire_motion: event.motion & 0xFE,
            terrain: event.terrain,
            ..Object::default()
        };
        true
    }

    /// `$BE8D` + `$8454`: destroy an object, score it and leave kind-2 debris.
    pub(super) fn destroy(&mut self, object: &mut Object, sfx: u8) {
        let index = usize::from(object.kind & 0x7F).min(self.rom().tables.destroy_sfx.len() - 1);
        let sound = self.rom().tables.destroy_sfx[index];
        if sound != 0 {
            self.audio.push(Sound::EnemyHit);
        }
        // `$BFAB` scores every shot kill through `$8468` (10 points). The
        // larger values are the separate `$8458` (100), `$8454` (300) and
        // `$8450` (500) paths used by cannon parts, the small core and the
        // Big Core.
        let points = match object.kind & 0x7F {
            15 | 16 => 100,
            // `$8468`: every shot kill is 10 points except the `$8458` (100),
            // `$8454` (300) and `$8450` (500) paths. The stage-5 `$1D`
            // straight piece is an ordinary 10-point kill (oracle +10 at
            // frame 425), not the `$8454` 300; the same holds for the
            // five-hit kind `$9A` (`$C0A0` still calls `$8468`).
            22 | 23 | 28 => 300,
            _ => 10,
        };
        // `$8474`: `LDA $09; BNE RTS` — the demo never writes the score.
        if !self.demo {
            self.add_score(points);
        }
        self.kills = self.kills.wrapping_add(1);
        // `$BE8D` turns the slot into kind-2 debris; the debris `$AE93` path
        // draws the white burst itself, so a second effect would double it.
        self.audio.push(Sound::Explosion);
        let _ = sfx;
        self.turn_to_debris(object);
    }

    /// `$BE8D`: the slot becomes animated debris (kind 2). `$016C` selects the
    /// path (`$BEBB`: kind 26 -> 3, kind 5 -> 0, everything else 1), `$014C`
    /// starts at 3 and `$012C`/`$010C`/`$042C` are cleared.
    pub(super) fn turn_to_debris(&mut self, object: &mut Object) {
        let low = object.kind & 0x1F;
        object.anim = match low {
            0x1A => 3,
            0x05 => 0,
            _ => 1,
        };
        object.phase = self.spawner.release_cannon(object.phase);
        object.kind = 2;
        object.subtype = 0;
        object.sprite = 0;
        object.anim_div = 3;
        object.x_vel = 0;
        // `$BEE0` clears `$018C`/`$010C`/`$012C`/`$042C` but not `$048C`, so
        // the debris keeps the killed handler's state byte.
    }

    /// `$BFDC`, `$C0FB`, `$C204`: collisions with the documented size tables.
    pub(super) fn collide(&mut self) {
        self.collide_vs_objects();
        // `$C2F9`: runs last, also while the ship is dying.
        if self.bullet_terrain_pass_runs() {
            self.collide_bullet_terrain();
        }
    }

    fn collide_vs_objects(&mut self) {
        self.collide_shots();
        if self.terrain_pass_runs() {
            self.collide_shot_terrain();
        }
        self.collide_ship();
    }

    /// `$C0C1`/`$C0FB`/`$C204`: the ship half of the collision pass. The
    /// stage-5 alternate chain (`$9690`) calls `$C0C1` itself, so these tests
    /// run on its frames too; only `$BFDC`'s shot scan is skipped there.
    pub(super) fn collide_ship(&mut self) {
        if self.player.dying {
            return;
        }
        self.collide_player_objects();
        if self.player.dying {
            return;
        }
        self.collide_player_bullets();
        if self.player.dying {
            return;
        }
        self.collide_player_beams();
        if self.player.dying {
            return;
        }
        // `$C29F` calls `$C39D` every frame in stage 1 (`$19 == 0`); the
        // `$0500`/`$0600` pages are not empty once the scroll reaches the
        // pre-boss columns, so the ship does crash into the map.
        if self.terrain_pass_runs() {
            self.collide_player_terrain();
        }
    }

    /// `$C29F`: stage 5 (`$19 == 4`) returns at once, and stage 3 (`$19 == 2`)
    /// jumps past the ship and shot map tests on even `$02` frames.
    fn terrain_pass_runs(&self) -> bool {
        match self.section {
            4 => false,
            2 => self.nmi_frame & 1 != 0,
            _ => true,
        }
    }

    /// `$C29F`: stage 5 returns before the bullet loop. In stage 3 an odd
    /// frame with the ship alive (`$0100 < 2`) takes the `RTS` at `$C2F8`
    /// after the shot loop; even frames jump straight to `$C2F9`.
    fn bullet_terrain_pass_runs(&self) -> bool {
        match self.section {
            4 => false,
            2 => self.nmi_frame & 1 == 0 || self.player.dying,
            _ => true,
        }
    }

    /// `$C2F9`: enemy bullets against the map. `$C3CD` probes the bullet at
    /// `$0376`, `$0336` (`+8` when `$0316 == 0`); any non-zero field removes
    /// it (`$AEF2`). Unlike a player shot it never breaks a cell.
    pub(super) fn collide_bullet_terrain(&mut self) {
        let mut index = 0;
        while index < self.enemy_bullet_count {
            let bullet = self.enemy_bullets[index];
            let x = bullet.x >> 8;
            let y = (bullet.y >> 8) as u8;
            let y = if bullet.kind == 0 {
                y.wrapping_add(8)
            } else {
                y
            };
            if self.terrain_solid(x, i32::from(y)) {
                self.enemy_bullet_count -= 1;
                self.enemy_bullets
                    .copy_within(index + 1..=self.enemy_bullet_count, index);
                self.enemy_bullets[self.enemy_bullet_count] = EnemyBullet::default();
            } else {
                index += 1;
            }
        }
    }

    /// `$BFDC`: each player shot tests one point against every object box.
    ///
    /// The box is `$BFD4[$0460]` × `$BFD8[$0460]` and the shot point is
    /// `shot + ($BFC8/$BFD0)[kind]`; the X test uses `$BFCC[kind]` as its width.
    pub(super) fn collide_shots(&mut self) {
        // `$BFDC`: the shot scan runs `$0123+8` down to `$0123`, so when two
        // shots overlap the same object the higher slot is consumed first.
        for shot_index in (0..self.shots.len()).rev() {
            if !self.shots[shot_index].active {
                continue;
            }
            let shot = self.shots[shot_index];
            let kind = usize::from(shot.kind).min(tables::SHOT_BOX_X.len() - 1);
            let sx = i32::from(shot.x.int.wrapping_add(tables::SHOT_BOX_X[kind]));
            let sy = shot.y.int.wrapping_add(tables::SHOT_BOX_Y[kind]);
            let width = i32::from(tables::SHOT_BOX_WIDTH[kind]);
            // `$C009`: the object scan runs slot 9 down to 0, so overlapping
            // boxes resolve in the original order.
            for slot in (0..LIVE_SLOTS).rev() {
                let object = self.objects[slot];
                // `$C00B`: only initialised objects (`$030C` bit 7) collide.
                if !object.live() || object.kind & 0x80 == 0 {
                    continue;
                }
                let dx = sx.wrapping_sub(i32::from(object.x.int)) as u8;
                if i32::from(dx) >= width {
                    continue;
                }
                let size = usize::from(object.size_class).min(tables::ENEMY_BOX_HEIGHT.len() - 1);
                // `$C01F`/`$C022`: both probes are integer bytes and the X
                // compare's borrow carries into the Y subtract, so the accepted
                // band is `1 <= dy <= height` (the cartridge hits `dy ==
                // height`). Fractions are not part of this path.
                let dy = sy.wrapping_sub(object.y.int);
                if dy == 0 || dy > tables::ENEMY_BOX_HEIGHT[size] {
                    continue;
                }
                self.shot_hit_object(shot_index, slot);
                if !self.shots[shot_index].active {
                    break;
                }
            }
            // `$C031`: on stage 5 the same shot then tests the `$0600`
            // cannons, but only while the object pass left it alive.
            if self.section == 4 && self.shots[shot_index].active {
                self.collide_shot_cannons(shot_index);
            }
            // `$BF6F`: the shot also tests the ten `$0136` collision-only
            // boxes; `$BFA2` clears the box and `$8468` pays ten points.
            if self.shots[shot_index].active {
                if let Some(points) = self.beams.hit_by_shot(sx as i16, sy as i16, width as u8) {
                    self.spawn_busy = true;
                    if !self.demo {
                        self.add_score(points);
                    }
                    if kind == 1 {
                        self.shots[shot_index].age = self.shots[shot_index].age.wrapping_add(1);
                    } else {
                        self.shots[shot_index].active = false;
                    }
                    self.audio.push(Sound::EnemyHit);
                }
            }
        }
    }

    /// `$BEED`/`$BF05`: one shot point against the six parts of every live
    /// `$0600` record.
    ///
    /// The hit band is the object path's: `dx < $BFCC[kind]` and
    /// `1 <= dy <= 10` in whole pixels (`$BF20` compares against `#$0A`).
    /// Part 2 is the weak point (`$BF2D`); a hit on any other part still
    /// consumes the shot (`$C0B1`), so a stalk hit costs the shot without
    /// damaging the cannon.
    fn collide_shot_cannons(&mut self, shot_index: usize) {
        let shot = self.shots[shot_index];
        let kind = usize::from(shot.kind).min(tables::SHOT_BOX_X.len() - 1);
        let sx = i32::from(shot.x.int.wrapping_add(tables::SHOT_BOX_X[kind]));
        let sy = shot.y.int.wrapping_add(tables::SHOT_BOX_Y[kind]);
        let width = i32::from(tables::SHOT_BOX_WIDTH[kind]);
        for record in (0..CANNON_SLOTS).rev() {
            if !self.cannons[record].active {
                continue;
            }
            for part in (0..CANNON_PARTS).rev() {
                let cannon = self.cannons[record];
                let dx = sx.wrapping_sub(cannon.part_x[part]) as u8;
                if i32::from(dx) >= width {
                    continue;
                }
                let dy = sy.wrapping_sub(cannon.part_y[part] as u8);
                if dy == 0 || dy > 0x0A {
                    continue;
                }
                if part != 2 {
                    self.consume_shot(shot_index);
                    return;
                }
                // `$BF36`: the weak point counts a hit against `$BEE4[rank]`.
                let hp = CANNON_HIT_HP[usize::from(self.stage_rank()).min(CANNON_HIT_HP.len() - 1)];
                self.cannons[record].hits = self.cannons[record].hits.wrapping_add(1);
                if self.cannons[record].hits < hp {
                    self.audio.push(Sound::EnemyHit);
                    self.consume_shot(shot_index);
                    return;
                }
                // `$BF43`: 100 points, the parent loses a `$016C` entry and the
                // record is cleared. The kind-2 debris belongs to the shot
                // pool, so the object table is left alone.
                if !self.demo {
                    self.add_score(100);
                }
                let parent = usize::from(cannon.parent);
                self.objects[parent].anim = self.objects[parent].anim.wrapping_sub(1);
                self.cannons[record] = Cannon::default();
                self.audio.push(Sound::EnemyHit);
                self.consume_shot(shot_index);
                return;
            }
        }
    }

    /// `$C0B1`: the shot-scan exit clears the shot slot; a laser (`$0163 == 1`)
    /// survives the pass (`$C0AF`).
    fn consume_shot(&mut self, shot_index: usize) {
        if self.shots[shot_index].kind != 1 {
            self.shots[shot_index].active = false;
        }
    }

    /// `$C29F`: a normal shot dies when its probe touches solid background.
    ///
    /// `$C3A9` shifts a laser 10 px along X; the probe then uses `$C3CD`, which
    /// is the same map test as [`terrain_solid`]. Missiles (`$0129`+) are
    /// checked by their own `$A169` loop and are not part of this pass.
    pub(super) fn collide_shot_terrain(&mut self) {
        for slot in 0..2 * self.shot_stride() {
            let shot = self.shots[slot];
            if !shot.active {
                continue;
            }
            let x = if shot.kind == 1 {
                i32::from(shot.x.int) + 0x0A
            } else {
                i32::from(shot.x.int)
            };
            let Some((column, row, field)) = self.terrain_probe(x, i32::from(shot.y.int)) else {
                continue;
            };
            if field == 0 {
                continue;
            }
            // `$C2D2`: a field of 2 is a destructible cell (`$C329` clears its
            // map bits and blanks the tile). A laser keeps flying through it
            // (`$C2DB`); every other field kills even the laser.
            if field == 2 {
                self.broken[column] |= 1 << row;
                self.fresh_breaks.push((column as u16, row as u8));
                // `$C329` calls `$EC18` with id 3, or 4 when `$19 == 5`.
                self.audio.push(if self.section == 5 {
                    Sound::BlockBreakStage6
                } else {
                    Sound::BlockBreak
                });
                if shot.kind == 1 {
                    continue;
                }
            }
            self.shots[slot].active = false;
        }
    }

    /// `$C04F`: apply one hit to an object.
    ///
    /// Ordinary objects die in one hit. Large objects (`$010C` bit 7) only
    /// take damage while `$048C != 0` (`$C06A`), accumulate in `$046C`, and
    /// die at three (`$C776`). Kind `$9A` (`$C08A`) has its own hit counter.
    pub(super) fn shot_hit_object(&mut self, shot_index: usize, slot: usize) {
        let object = self.objects[slot];
        let laser = self.shots[shot_index].kind == 1;
        // `$BF99`: a shot hit raises the frame-scoped `$5D` effect flag.
        self.spawn_busy = true;
        if !laser {
            self.shots[shot_index].active = false;
        } else {
            self.shots[shot_index].age = self.shots[shot_index].age.wrapping_add(1);
        }
        if object.boss {
            let mut object = object;
            // `$C06A`: the core only takes damage while `$048C != 0`, and
            // `$B941` awards the 50-point hit through `$8460`.
            if object.ai_state != 0 {
                let mut add = 1u8;
                if object.size_class != 0 && self.is_missile_slot(shot_index) {
                    add = 2;
                }
                object.ai_timer = object.ai_timer.wrapping_add(add);
                self.objects[slot] = object;
                // `$B8E9[6] == 0`: the core dies on the next handler frame
                // once the counter reaches 6, not inside the collision pass.
                if object.ai_timer < 6 {
                    self.audio.push(Sound::EnemyHit);
                }
            }
            return;
        }
        if object.subtype & 0x80 != 0 {
            let mut object = object;
            if object.ai_state == 0 {
                return;
            }
            let mut add = 1u8;
            if object.size_class != 0 && self.is_missile_slot(shot_index) {
                add = 2;
            }
            object.ai_timer = object.ai_timer.wrapping_add(add);
            let low = object.kind & 0x7F;
            if low == 20 {
                // `$CA58` owns its own morph/death thresholds; the generic
                // three-hit large-object path does not apply.
                self.objects[slot] = object;
                self.audio.push(Sound::EnemyHit);
                return;
            }
            let threshold = if low == 15 || low == 16 {
                // `$AF5D`: the volcano dies at `$046C >= 5`, not the generic 3.
                5
            } else {
                3
            };
            if object.ai_timer >= threshold {
                if low == 15 || low == 16 || low == 22 || low == 23 {
                    // `$AF51`/`$C776`/`$B830`: these handlers score and die in
                    // the *next* handler frame, not inside the collision pass.
                    self.objects[slot] = object;
                    self.audio.push(Sound::EnemyHit);
                    return;
                }
                self.destroy(&mut object, 0x05);
                self.objects[slot] = object;
            } else {
                self.objects[slot] = object;
                self.audio.push(Sound::EnemyHit);
            }
            return;
        }
        if object.kind & 0x7F == 26 {
            // `$C08A`: kind `$9A` counts hits in `$04AC` against `$BFBF[player]`.
            let mut object = object;
            object.ai_mode = object.ai_mode.wrapping_add(1);
            if object.ai_mode >= 5 {
                self.destroy(&mut object, 0x0C);
                self.objects[slot] = object;
            } else {
                self.objects[slot] = object;
                self.audio.push(Sound::EnemyHit);
            }
            return;
        }
        let mut object = object;
        self.destroy(&mut object, 0x05);
        self.objects[slot] = object;
    }

    /// `$C0FB`: the ship point `(X+4, Y+8)` against every object box.
    pub(super) fn collide_player_objects(&mut self) {
        let px = i32::from(self.player.x.int.wrapping_add(4));
        let py = i32::from(self.player.y.int.wrapping_add(8));
        // `$C111`: the ship's object scan also runs slot 9 down to 0.
        for slot in (0..LIVE_SLOTS).rev() {
            let object = self.objects[slot];
            // `$C111`: the ship tests every non-zero `$030C`, including the
            // frame before a handler sets its bit-7 initialised flag.
            if !object.live() {
                continue;
            }
            let size = usize::from(object.size_class).min(tables::ENEMY_BOX_WIDTH.len() - 1);
            let dx = px.wrapping_sub(i32::from(object.x.int)) as u8;
            if i32::from(dx) >= i32::from(tables::ENEMY_BOX_WIDTH[size]) {
                continue;
            }
            // `$C0FF`/`$C11C`: after the width compare the carry is clear, so
            // the Y subtract is `py - y - 1`; the accepted band is
            // `1..=height`, exactly as in `collide_shots`.
            let dy = py.wrapping_sub(i32::from(object.y.int)) as u8;
            if dy == 0 || i32::from(dy) > i32::from(tables::ENEMY_BOX_HEIGHT[size]) {
                continue;
            }
            let low = object.kind & 0x7F;
            if low == 0x27 || low == 0x29 {
                // `$C137`/`$C153`: the pickup's slot becomes the kind-1 item
                // that keeps drifting (`$012C = $A3` for the 1UP, `$A1` for the
                // 500-point one); nothing is despawned or scored beyond that.
                let mut object = object;
                if low == 0x27 {
                    // `$C137`: `LDA $07E5,X; LSR A; BCS` skips the pickup
                    // while bit 0 of the score's `$07E5` BCD byte is set.
                    if (self.score / 100) % 10 % 2 == 1 {
                        continue;
                    }
                    // `$C14E`: `INC $20,X`.
                    self.lives = self.lives.wrapping_add(1);
                    object.sprite = 0xA3;
                } else {
                    // `$C153`: `JSR $8450`, the 500-point path.
                    self.add_score(500);
                    object.sprite = 0xA1;
                }
                object.kind = 1;
                self.audio.push(Sound::Capsule);
                self.objects[slot] = object;
                continue;
            }
            if low == 1 {
                // `$C17D`: a kind-1 object without a subtype is inert.
                if object.subtype == 0 {
                    continue;
                }
                if object.subtype == 6 {
                    // `$C1A9`: the power capsule. `$8950` advances `$42` and
                    // awards 50 points through `$8460`.
                    self.objects[slot].despawn();
                    let selection = self.power.selection();
                    self.power.advance();
                    // `$8958`-`$8967`: on the `$42 = 7` wrap `$CE83` reads the
                    // low nibble of the `$07E5` score byte (the hundreds digit)
                    // and a 5 pays 1000 points through `$845A` before the 50.
                    if selection == 6 && (self.score / 100) % 10 == 5 {
                        self.add_score(1_000);
                    }
                    self.add_score(50);
                    self.audio.push(Sound::Capsule);
                } else {
                    // `$C186`: the blue capsule clears its own slot (`$C1F7`)
                    // and plays `$0B`; the Y=9..0 loop then runs `$BE8D` over
                    // every object with subtype bit 7 clear, kind bit 7 set and
                    // a low kind of at least 3. `$BE8D` scores nothing and
                    // never plays the `$09` burst.
                    self.objects[slot].despawn();
                    self.audio.push(Sound::ScreenClear);
                    for target in (0..LIVE_SLOTS).rev() {
                        let mut object = self.objects[target];
                        if object.subtype & 0x80 != 0 || object.kind & 0x80 == 0 {
                            continue;
                        }
                        let kind = object.kind & 0x7F;
                        if kind < 3 {
                            continue;
                        }
                        let index = usize::from(kind).min(self.rom().tables.destroy_sfx.len() - 1);
                        if self.rom().tables.destroy_sfx[index] != 0 {
                            self.audio.push(Sound::EnemyHit);
                        }
                        self.turn_to_debris(&mut object);
                        self.objects[target] = object;
                    }
                }
                continue;
            }
            // `$C175`: kind 2 (debris) and anything below 1 are harmless.
            if low < 3 {
                continue;
            }
            // `$C1B2`: only an initialised object (bit 7 of `$030C`) collides.
            if object.kind & 0x80 == 0 {
                continue;
            }
            // `$C1B7`: the shield (`$46`) absorbs first, otherwise the hull
            // takes the hit. A hull hit leaves the object alive; only the
            // shield's `$C1CA` path destroys it.
            if self.player_invulnerable() {
                continue;
            }
            if !self.power.take_shield_hit() {
                self.hull_hit();
                if self.player.dying {
                    return;
                }
                continue;
            }
            let mut object = object;
            if object.subtype & 0x80 != 0 {
                // `$C1C2`: a large object only takes a shield hit's damage.
                object.ai_timer = object.ai_timer.wrapping_add(1);
                self.objects[slot] = object;
            } else {
                // `$C1CA`/`$BE8D`: the shield destroys it into kind-2 debris.
                self.turn_to_debris(&mut object);
                self.objects[slot] = object;
            }
        }
    }

    /// `$C39D` against the ship. `$C29F` calls it every frame for stage 1;
    /// the `$0500`/`$0600` pages stay zero early but become nonzero in the
    /// pre-boss columns (see `docs/analysis/powerup_terrain_verification.md`).
    /// The options are invulnerable and are not tested.
    pub(super) fn collide_player_terrain(&mut self) {
        if self.player.dying {
            return;
        }
        let x = i32::from(self.player.x.int);
        let y = i32::from(self.player.y.int);
        if !self.terrain_solid(x, y) {
            return;
        }
        if self.player_invulnerable() {
            return;
        }
        // `$C2B6`: the map pass checks no shield (the oracle dies against the
        // top wall with `$46 = 5`). With one hull point `hull_hit` is `$C1D0`;
        // the hits mod keeps charging one point per frame inside the map.
        self.hull_hit();
    }

    /// `$C204`: enemy shots against the player box, by shot type.
    pub(super) fn collide_player_bullets(&mut self) {
        let mut index = 0;
        while index < self.enemy_bullet_count {
            let bullet = self.enemy_bullets[index];
            let (ox, oy, width, height) = bullet.player_box();
            let x = bullet.x >> 8;
            let y = bullet.y >> 8;
            // `$C22C`-`$C23C`: the box test subtracts the bullet from the
            // player edge, and the accepted Y band is `1..=height` because
            // the failed width `CMP` leaves the borrow set for the Y `SBC`.
            let dx = i32::from(self.player.x.int.wrapping_add(ox as u8)).wrapping_sub(x) as u8;
            let dy = i32::from(self.player.y.int.wrapping_add(oy as u8)).wrapping_sub(y) as u8;
            if i32::from(dx) < width && dy != 0 && i32::from(dy) <= height {
                self.enemy_bullet_count -= 1;
                self.enemy_bullets[index] = self.enemy_bullets[self.enemy_bullet_count];
                self.enemy_bullets[self.enemy_bullet_count] = EnemyBullet::default();
                // The overlapping bullet is always removed; the window then
                // swallows the damage without spending the shield or hull.
                if self.player_invulnerable() {
                    continue;
                }
                if !self.power.take_shield_hit() {
                    self.hull_hit();
                    if self.player.dying {
                        return;
                    }
                } else {
                    self.audio.push(Sound::EnemyHit);
                }
                continue;
            }
            index += 1;
        }
    }

    /// `$C222`: the ship's `(+6, +8)` probe against the ten `$0136`
    /// collision-only boxes. The 22 x 18 hit box is lethal without a shield.
    pub(super) fn collide_player_beams(&mut self) {
        if self.player.dying {
            return;
        }
        let (x, y) = (i16::from(self.player.x.int), i16::from(self.player.y.int));
        if self.player_invulnerable() || !self.beams.hits_player(x, y) {
            return;
        }
        if self.power.take_shield_hit() {
            // `$C248`: the shield absorbs the hit and `$AEF2` clears the box,
            // so the same beam cannot be shot for ten points next frame.
            self.beams.hit_player(x, y);
            self.audio.push(Sound::EnemyHit);
        } else {
            self.hull_hit();
        }
    }
}

/// `$83BA`: the 17-iteration shift/subtract divide of `minor << 16` by the
/// major axis, quotient in `$99:$9A`. A major axis of `$80` or more is first
/// halved together with the dividend (`$83C6`), and the last iteration leaves
/// the quotient rounded rather than truncated (60.68 gives 61, 62.58 gives 63).
pub(super) fn slope(major: u32, minor: u32) -> i16 {
    if major == 0 {
        return 0;
    }
    let (mut hi, mut mid, mut lo, mut divisor) = (minor as u8, 0u8, 0u8, major as u8);
    if divisor & 0x80 != 0 {
        lo = (lo >> 1) | (mid << 7);
        mid = (mid >> 1) | (hi << 7);
        hi >>= 1;
        divisor >>= 1;
    }
    let (mut acc, mut carry) = (0u8, 0u8);
    for step in 0..17 {
        for byte in [&mut lo, &mut mid, &mut hi] {
            let value = (u16::from(*byte) << 1) | u16::from(carry);
            *byte = value as u8;
            carry = (value >> 8) as u8;
        }
        if step == 16 {
            break;
        }
        acc = (acc << 1) | carry;
        if acc >= divisor {
            acc -= divisor;
            carry = 1;
        } else {
            carry = 0;
        }
    }
    (u16::from(mid) << 8 | u16::from(lo)).min(0x7FFF) as i16
}
