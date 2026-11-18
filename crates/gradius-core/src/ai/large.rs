//! The `$14` large object, the `$98` Big Core, the launchers and the
//! ballistic arcs.
//!
//! Split from `ai.rs` by family (card R1e), bodies verbatim.

use super::*;
use crate::stage1_tables as tables;

impl Game {
    /// `$CA58`: the stage-5 `$14` large object. It counts damage in `$046C`,
    /// morphs to the smaller box at `$CA43[rank]`, dies at `$CA4A[rank]`,
    /// drifts at `$AEDB` speed and steers its Y toward the ship with
    /// `$CA51[rank]`. With attached cannons (`$016C != 0`) it only takes the
    /// trailing drift, i.e. 0.5 px/frame.
    pub(in crate::game) fn ai_large_object(&mut self, slot: usize, object: &mut Object) {
        let rank = usize::from(self.stage_rank()).min(LARGE_MORPH_HP.len() - 1);
        let morph_hp = LARGE_MORPH_HP[rank];
        let death_hp = LARGE_DEATH_HP[rank];
        object.kind = 0x94;
        // `$042C` is a free-running frame counter (the port has no separate
        // byte; `anim_div` is unused for large objects).
        object.anim_div = object.anim_div.wrapping_add(1);
        let mut sprite = 0x81 + u8::from(object.anim_div & 0x08 != 0);
        if object.ai_timer >= morph_hp {
            if object.size_class != 0 {
                object.size_class = 0;
                object.x.add(8 << 8);
                object.y.add(8 << 8);
            }
            sprite += 2;
        }
        object.sprite = sprite;
        object.ai_state = 1;
        if object.ai_timer >= death_hp {
            self.large_object_death(slot as u8, object);
            return;
        }
        let mut carry = false;
        if object.anim == 0 {
            self.drift_large_object(object);
            if !object.active {
                return;
            }
            // `$AEDB` leaves carry set unless the X borrows below 8; the
            // following `SBC $CA51` therefore subtracts exactly the table
            // byte when the drift ran.
            carry = object.x.int >= 0x08;
            if object.sprite == 0 {
                return;
            }
        }
        if object.ai_mode == 0 {
            object.ai_mode = self.frame as u8 & 0x3F;
            object.timer2 = u8::from(object.y.int < self.player.y.int);
        }
        object.ai_mode = object.ai_mode.wrapping_sub(1);
        let frac = i32::from(LARGE_AIM_FRAC[rank]);
        if object.timer2 == 0 {
            // `$CAE3`: `SBC $CA51,Y` with the carry left by the drift.
            let step = frac + if carry { 0 } else { 1 };
            object.y.sub(step);
            object.y.int = object.y.int.max(0x20);
        } else {
            // `$CAFD`: `ADC $CA51,Y`.
            let step = frac + i32::from(carry);
            object.y.add(step);
            object.y.int = object.y.int.min(0xA8);
        }
        self.drift_large_object(object);
    }

    /// `$AEDB`: `$CA58` calls the drift routine directly, past the `$AED7`
    /// `$5B` test, so the `$82` hold does not freeze a `$14` object. The
    /// gated entry is `ai_drift`.
    fn drift_large_object(&mut self, object: &mut Object) {
        object.drift();
        if object.x.int < 0x08 {
            object.despawn();
        }
    }

    /// `$CA58` death: 500 points, sound `$0A`, kind-2 debris, then `$CB48`
    /// releases the still-attached `$0600` records as their own debris.
    fn large_object_death(&mut self, parent: u8, object: &mut Object) {
        if !self.demo {
            self.add_score(500);
        }
        self.audio.push(Sound::EnemyHit);
        self.audio.push(Sound::Explosion);
        self.release_cannons(parent);
        self.turn_to_debris(object);
    }

    /// `$C64D`: stage 5's `$C433[4]` script. `$68` counts every `$C40D` call
    /// and every `$28` spawns the next `$C674` pair through `$A4A0`.
    pub(in crate::game) fn stage_five_script(&mut self) {
        self.script_counter = self.script_counter.wrapping_add(1);
        if self.script_counter < STAGE5_SCRIPT_PERIOD {
            return;
        }
        self.script_counter = 0;
        let index = usize::from(self.hazard_index & 0x06);
        self.hazard_index = self.hazard_index.wrapping_add(2);
        let b2 = STAGE5_SCRIPT[index];
        let y = STAGE5_SCRIPT[index + 1];
        self.command([0x00, 0xF0, b2, 0x14, y]);
    }

    /// `$B1FF`: the ballistic arc.
    ///
    /// `$03BC:$03EC` starts at `y_speed`, is decremented by `$048C` (`$20` =
    /// 0.5 px/frame²) every frame and resets once it passes -2.0 px/frame.
    pub(in crate::game) fn ai_arc(&mut self, object: &mut Object, y_speed: i16, flip: bool) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.ai_state = 0x20;
            object.ai_mode = 0;
            object.ai_timer = 0;
            if flip {
                // `$B199`: the kind-6 handler writes subtype 2, so the next
                // frame's `$ADDF` pass supplies `$012C` (sprite `$17`).
                object.subtype = 2;
            }
            object.reset_velocities(-0x200, y_speed << 8);
            return;
        }
        if flip {
            // `$B192`: the `$B1D4` x-direction flip based on `$B1FA`.
            let index = usize::from(object.ai_mode).min(tables::AI_TIMER_KIND6.len() - 1);
            object.ai_timer = tables::AI_TIMER_KIND6[index];
            if object.y_vel < -0x300 {
                object.ai_mode = object.ai_mode.wrapping_add(1);
                object.ai_state = 0x20;
                object.reset_velocities(-0x200, y_speed << 8);
                return;
            }
            if object.ai_timer == 0 {
                object.move_vel_x();
            } else {
                let delta = i32::from(object.x_vel);
                object.x.add(-delta);
            }
            let delta = i32::from(object.y_vel);
            object.y.add(-delta);
            object.y_vel = object.y_vel.wrapping_sub(i16::from(object.ai_state));
            self.bounds(object);
            return;
        }
        let hi = (object.y_vel >> 8) as u8;
        if object.ai_timer != 0 {
            if hi & 0x80 == 0 || hi >= 0xFE {
                self.arc_fall(object);
            } else {
                // `$B22D`/`$B204`: the kind-4 restart runs `$B0AE`, which adds
                // `$80` to the raw kind, so the initialised bit clears for one
                // frozen frame and comes back in the init path next frame.
                if object.kind & 0x7F == 4 {
                    object.kind = object.kind.wrapping_add(0x80);
                } else {
                    object.kind |= 0x80;
                }
                object.ai_timer = 0;
                object.ai_state = 0x20;
                object.reset_velocities(-0x200, y_speed << 8);
            }
        } else if hi & 0x80 == 0 || hi >= 0xFE {
            self.arc_rise(object);
        } else {
            object.ai_timer = object.ai_timer.wrapping_add(1);
            object.reset_velocities(-0x200, y_speed << 8);
        }
    }

    /// `$B1D9`: X += velocity, Y -= velocity (the rising half of the arc).
    fn arc_rise(&mut self, object: &mut Object) {
        object.move_vel_x();
        let delta = i32::from(object.y_vel);
        object.y.add(-delta);
        object.y_vel = object.y_vel.wrapping_sub(i16::from(object.ai_state));
        self.bounds(object);
    }

    /// `$B1EB`: X += velocity, Y += velocity (the falling half).
    fn arc_fall(&mut self, object: &mut Object) {
        object.move_vel_x();
        object.move_vel_y();
        object.y_vel = object.y_vel.wrapping_sub(i16::from(object.ai_state));
        self.bounds(object);
    }

    /// `$B3FC`/`$B42E`: the ballistic arc with a per-mode X direction.
    pub(in crate::game) fn ai_arc_toggle(&mut self, object: &mut Object, kind: u8) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.ai_mode = 0;
            object.ai_state = 0x20;
            object.reset_velocities(-0x200, 0x0200);
            return;
        }
        let table = if kind == 13 {
            &tables::AI_TIMER_KIND13
        } else {
            &tables::AI_TIMER_KIND14
        };
        let index = usize::from(object.ai_mode).min(table.len() - 1);
        object.ai_timer = table[index];
        // `$B3FC`/`$B42E`: `$03BC` holds the high byte of the Y velocity, so
        // the restart fires once that velocity drops below -2 px/frame (the
        // high byte `$FD`) and skips the movement on that frame.
        if object.y_vel < -0x200 {
            object.ai_mode = object.ai_mode.wrapping_add(1);
            object.ai_state = 0x20;
            object.reset_velocities(-0x200, 0x0200);
            return;
        }
        let delta = i32::from(object.x_vel);
        if object.ai_timer == 0 {
            object.x.add(delta);
        } else {
            object.x.add(-delta);
        }
        if kind == 13 {
            let delta = i32::from(object.y_vel);
            object.y.add(-delta);
        } else {
            object.move_vel_y();
        }
        object.y_vel = object.y_vel.wrapping_sub(i16::from(object.ai_state));
        self.bounds(object);
    }

    /// `$B379`/`$B6xx`: aim at the ship (`$BCAF`) then fly along that vector
    /// (`$BDF4`), despawn off screen.
    pub(in crate::game) fn ai_launcher(&mut self, object: &mut Object) {
        if object.kind & 0x80 == 0 {
            object.timer2 = 0;
            object.ai_mode = object.ai_mode.wrapping_add(1);
            let frame = (object.ai_mode >> 2).min(8);
            object.sprite = tables::LAUNCH_FRAMES[usize::from(frame)];
            if frame == 8 {
                object.timer2 = 1;
                object.ai_state = 0;
                object.kind |= 0x80;
                object.sprite = 0x67;
                self.aim_object(object);
            }
            return;
        }
        if object.ai_state == 0 {
            // `$B3A4`/`$BCAF`: the first activated frame also fires one aimed
            // enemy shot; `$B3B6` then sets `$048C = 1` so it happens once.
            self.push_enemy_bullet(object.x.int, object.y.int, 0, 0x25);
            self.aim_object(object);
        }
        object.ai_state = 1;
        self.move_aimed(object);
    }

    /// `$B90E`/`$BA12`: the Big Core. Enters at 1 px/frame, then tracks the
    /// player horizontally, re-aims vertically every fire period and opens its
    /// hatch to attack. The two `$99` covers follow the core's position.
    pub(in crate::game) fn ai_big_core(&mut self, object: &mut Object) {
        // `$B9A2`: while x >= $A4 the entry loop spends one pixel per frame.
        // `$B9AC` then tests `$048C`: with the gate open the frame falls
        // through into `$BA04` (track, re-aim, vertical step, timers); only
        // `$048C == 0` rebuilds the covers at `$B9B1` and returns.
        if object.x.int > 0xA3 {
            object.x.add(-0x100);
            if object.ai_state == 0 {
                self.sync_boss_parts(object.x.int, object.y.int);
                return;
            }
        }
        // `$B941`: the handler frame after a hit sees the new `$046C` and
        // awards 50 for each armour sprite change (`$B8E9[0..5]`).
        if object.ai_timer > object.ai_mode {
            if object.ai_mode < 5 {
                self.add_score(50);
            }
            object.ai_mode = object.ai_timer;
            // `$046C` selects the `$B8E9` armour frame.
            let frame = usize::from(object.ai_mode.min(5));
            object.sprite = crate::stage1_enemies::big_core::SPRITE_FRAMES[frame];
        }
        // `$B929`: the damage counter is checked before anything else, and
        // `$B8E9[6] == 0` takes the `$B95C` death path one frame after the
        // sixth hit lands.
        if object.ai_timer >= 6 {
            self.destroy_boss(9);
            // `$CB20` then `$B982`: the core becomes kind-2 debris on path 4
            // with `$014C = 0`, and `$CB20` clears `$042C`, `$046C`, `$04AC`,
            // `$010C` and `$03AC`. `$048C` survives, which the dump shows as
            // substate 1.
            object.kind = 2;
            object.boss = false;
            object.sprite = 1;
            object.ai_state = 1;
            object.anim = 4;
            object.anim_div = 0;
            object.x_vel = 0;
            object.ai_timer = 0;
            object.subtype = 0;
            object.phase = 0;
            return;
        }
        self.scroll_stop = true;
        let rank = self.boss_rank();
        // `$BA04`/`$BA0C`: only a player on the right pulls the core right.
        if crate::stage1_enemies::boss_tracks_right(object.x.int, self.player.x.int) {
            object
                .x
                .add(i32::from(crate::stage1_enemies::big_core::TRACK_X) << 8);
        }
        // `$042C` counts up from 0; the re-aim and the shot happen on the
        // frame it reaches `$B904[rank]`, then it resets. Phase starts at 0
        // (up), so the core climbs during entry until the first period ends.
        let period = crate::stage1_enemies::boss_fire_period(rank)
            .unwrap_or(1)
            .max(1);
        let ready = object.timer2 >= period;
        if ready {
            // `$BA1C`: only the fire frame re-aims; other frames keep the
            // previously stored `$03EC` direction.
            object.phase = if object
                .y
                .int
                .wrapping_sub(crate::stage1_enemies::big_core::AIM_OFFSET)
                >= self.player.y.int
            {
                0
            } else {
                1
            };
        }
        let (fraction, integer) =
            crate::stage1_enemies::boss_vertical_velocity(rank).unwrap_or((0, 1));
        // `$BA1A` branches here with carry clear (`CMP` below the period), so
        // the upward `SBC` borrows one extra 1/256. On the fire frame the
        // `$BA24` compare leaves carry set when the core goes up (`BCS`), so
        // that frame borrows nothing; the downward `ADC` never gets the extra.
        let delta = (i32::from(integer) << 8)
            + i32::from(fraction)
            + if object.phase == 0 && !ready { 1 } else { 0 };
        if object.phase == 0 {
            object.y.sub(delta);
        } else {
            object.y.add(delta);
        }
        object.y.int = object.y.int.clamp(
            crate::stage1_enemies::big_core::MIN_Y,
            crate::stage1_enemies::big_core::MAX_Y,
        );
        self.sync_boss_parts(object.x.int, object.y.int);

        // `$BA6A`: the same counter fires the four gun pods. On the fire frame
        // it resets instead of advancing `$04AC`/`$04CC`; those only step on
        // the other frames.
        if ready {
            object.timer2 = 0;
            for (dx, dy) in crate::stage1_enemies::big_core::GUN_OFFSETS {
                let x = object.x.int.wrapping_add(dx as u8);
                let y = object.y.int.wrapping_add(dy as u8);
                // `$9263` group `$41`: a 16x16 pair that flies straight left
                // at `$0340` (3.25 px/frame), not the aimed `$3A` pair.
                self.push_enemy_bullet_with_velocity(x, y, -0x340, 0, 0, 0x41);
            }
        } else {
            object.timer2 = object.timer2.wrapping_add(1);
            // `$04AC`/`$04CC`: an 8-bit frame counter and its 256-frame
            // periods. `$048C` (the dumped substate) is 1 from period 1 to 4,
            // then the core self-destructs at period 6.
            object.anim = object.anim.wrapping_add(1);
            if object.anim == 0 {
                object.anim_div = object.anim_div.wrapping_add(1);
                match object.anim_div {
                    1 => object.ai_state = 1,
                    5 => object.ai_state = 0,
                    6 => self.boss_self_destruct(object),
                    _ => {}
                }
            }
        }
    }
}
