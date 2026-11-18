//! The `$ADA5` dispatch, the shared handlers and the boss ranks.
//!
//! Split from `ai.rs` by family (card R1e), bodies verbatim.

use super::*;
use crate::stage1_tables as tables;

impl Game {
    /// `$ADA5`: per-slot animation then the `$AE16` AI jump table.
    pub(in crate::game) fn update_objects(&mut self) {
        // `$ADA5`: the AI loop runs slots 9 down to 0, so a child spawned into
        // a free slot by a higher-numbered parent is initialised in the same
        // frame (`$AF92`'s shot allocation relies on it).
        for slot in (0..LIVE_SLOTS).rev() {
            if !self.objects[slot].live() {
                continue;
            }
            self.animate_object(slot);
            let mut object = self.objects[slot];
            let kind = object.kind & 0x7F;
            match kind {
                0 => {}
                // `$AED7`/`$AEDB`: pure scroll drift.
                1 | 3 | 39 | 41 => self.ai_drift(&mut object),
                // `$AE93`: debris / background fold-in.
                2 => self.ai_debris(&mut object),
                // `$B1FF`: ballistic arc.
                4 => self.ai_arc(&mut object, 2, false),
                // `$B0A9`: loop scout.
                5 => self.ai_scout(&mut object),
                // `$B192`: heavy ballistic arc.
                6 => self.ai_arc(&mut object, 3, true),
                // `$B6DB`: ground walker.
                7 => self.ai_walker(&mut object, 8),
                // `$B266`: 0.5 px/frame seeker.
                8 => self.ai_seeker(&mut object),
                // `$B30B`: fired shell.
                9 => self.ai_shell(&mut object),
                // `$B369`: eruption hazard. It is not an arc: `$C480` seeds
                // `$042C`/`$03BC` and `$048C`, then `$B1DF` moves and
                // accelerates it until `$B24B` bounds-despawns.
                10 => self.ai_hazard(&mut object),
                // `$B371`: arc variant.
                21 => self.ai_arc(&mut object, 3, true),
                // `$B379`: hover then launch itself at the ship.
                11 => self.ai_launcher(&mut object),
                // `$B3C5`: hover then dive.
                12 => self.ai_diver(&mut object),
                // `$B3FC`/`$B42E`: arc with direction flips.
                13 | 14 => self.ai_arc_toggle(&mut object, kind),
                // `$CA58`: the stage-5 large `$14` object and its cannons.
                20 => self.ai_large_object(slot, &mut object),
                // `$AF28`/`$AF82`: turret that fires once.
                15 | 16 => self.ai_turret_fire(&mut object, kind),
                // `$B020`/`$B092`: aiming turret.
                17 | 18 => self.ai_turret(&mut object, kind),
                // `$B741`: ceiling walker.
                19 => self.ai_walker(&mut object, 0xF8),
                // `$B79B`: the small core (stage-1 boss family).
                23 => self.ai_miniboss(&mut object),
                // `$B47A`: spin/aim/drift cycle.
                26 => self.ai_spin(&mut object),
                // `$B90E`: the Big Core (`$98`).
                24 => self.ai_big_core(&mut object),
                // `$B90D`: the inert `$99` covers; the core writes their
                // position every frame.
                25 => {}
                // `$B4F7`/`$B553`: descending and straight big pieces.
                28 => self.ai_descender(&mut object),
                29 => self.ai_straight(&mut object, 1),
                // `$B563`: the wall that freezes the scroll.
                30 => self.ai_wall(&mut object),
                // `$AF0A`: background piece sprite + drift.
                32..=37 => self.ai_bg_piece(&mut object),
                // `$C900`: the stage-3 background turret (boot slice: drift;
                // the paint/death path lands with stage3_enemies).
                22 => self.ai_background_turret(&mut object),
                _ => self.ai_drift(&mut object),
            }
            if object.active {
                self.objects[slot] = object;
            } else {
                self.objects[slot].despawn();
            }
        }
    }

    /// `$ADDF`: animation, divider 6, table `$ADBB` (four frames per subtype).
    fn animate_object(&mut self, slot: usize) {
        let frames = &self.rom().spawns.animation_frames;
        let object = &mut self.objects[slot];
        // `$ADDF`: skip a zero `$010C` and a large object (bit 7); those keep
        // the sprite their handler set. Terrain pieces are no exception once a
        // handler has given them a subtype (the kind-6 arc writes `$010C = 2`).
        if object.subtype == 0 || object.subtype & 0x80 != 0 || object.boss {
            return;
        }
        // `$ADEB`/`$ADF7`: a zero divider starts a new step at 6 and walks
        // `$016C` past zero table entries; `$AE0B` then decrements the divider
        // once, including on the step frame.
        if object.anim_div == 0 {
            object.anim_div = 6;
            loop {
                object.anim = object.anim.wrapping_add(1);
                let index =
                    (usize::from(object.subtype) * 4 + usize::from(object.anim & 3)) % frames.len();
                let sprite = frames[index];
                if sprite != 0 {
                    object.sprite = sprite;
                    break;
                }
            }
        }
        object.anim_div = object.anim_div.saturating_sub(1);
    }

    /// `$B24B`: despawn anything outside `$04..=$F3` × `$08..=$C3`.
    pub(super) fn bounds(&mut self, object: &mut Object) {
        if object.x.int < 0x04
            || object.x.int >= 0xF4
            || object.y.int < 0x08
            || object.y.int >= 0xC4
        {
            object.despawn();
        }
    }

    /// `$AED7`: `$038C -= $80` while `$5B == 0`, then `$AEF2` below X=`$08`.
    pub(in crate::game) fn ai_drift(&mut self, object: &mut Object) {
        // `$AED7: LDA $5B; BNE` freezes the drift for both the `$B563` wall
        // and the `$82` eruption hold (`scroll_locked`).
        if self.scroll_stop || self.scroll_locked {
            return;
        }
        object.drift();
        if object.x.int < 0x08 {
            object.despawn();
        }
    }

    /// `$AE93`: walk the `$AE6B` debris path, then fold into the background.
    ///
    /// `$014C` spaces the sprite steps by six frames; `$016C` picks the path
    /// stream and `$042C` is its cursor. A stream ending in `$00` turns the
    /// slot into a kind-1 background piece when `$03AC` is set, and despawns it
    /// otherwise. The `$AED7` drift runs every frame, including the ones spent
    /// waiting on the divider.
    pub(in crate::game) fn ai_debris(&mut self, object: &mut Object) {
        if object.anim_div != 0 {
            object.anim_div -= 1;
        } else {
            object.anim_div = 5;
            let path = {
                let paths = &self.rom().spawns.debris_paths;
                &paths[usize::from(object.anim) % paths.len()]
            };
            let cursor = usize::from(object.x_vel.max(0) as u16 & 0xFF);
            object.x_vel = object.x_vel.wrapping_add(1);
            match path.get(cursor).copied().filter(|sprite| *sprite != 0) {
                Some(sprite) => {
                    object.sprite = sprite;
                    // `$AED1` falls through to the `$AED4` decrement, so a
                    // step frame still burns one of the five.
                    object.anim_div -= 1;
                }
                None => {
                    if object.phase == 0 {
                        object.despawn();
                    } else {
                        object.kind = 1;
                        self.debris_folds = self.debris_folds.wrapping_add(1);
                        // `$AEBB`: subtype 7 only on the sixteenth fold, 6
                        // otherwise (`$47 & $0F`), not a straight 7/6 alternation.
                        object.subtype =
                            tables::DEBRIS_SUBTYPES[usize::from(self.debris_folds & 0x0F != 0)];
                    }
                    return;
                }
            }
        }
        self.ai_drift(object);
    }

    /// `$B369`: the stage-1 eruption hazard (`$C480`). `$B1DF`/`$B11A` move it
    /// by the seeded 8.8 velocity and subtract `$048C` from the Y velocity
    /// each frame.
    fn ai_hazard(&mut self, object: &mut Object) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            return;
        }
        object.x.sub(i32::from(object.x_vel));
        object.y.sub(i32::from(object.y_vel));
        object.y_vel = object.y_vel.wrapping_sub(i16::from(object.ai_state));
        self.bounds(object);
    }

    /// `$B0A9`: enter from the right, track the ship's Y, loop right.
    pub(in crate::game) fn ai_scout(&mut self, object: &mut Object) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            return;
        }
        match object.ai_state {
            0 => {
                object.x.add(-0x200);
                if object.x.int >= 0x60 {
                    return;
                }
                object.ai_timer = 0x40;
                if object.y.int >= 0x80 {
                    object.ai_state = object.ai_state.wrapping_add(1);
                }
                object.ai_state = object.ai_state.wrapping_add(1);
            }
            1 => {
                if object.y.int >= self.player.y.int {
                    object.ai_state = 3;
                }
                object.y.add(0x200);
                object.x.int = object.x.int.wrapping_add(1);
                object.ai_timer = object.ai_timer.wrapping_sub(1);
                if object.ai_timer == 0 {
                    object.ai_state = 3;
                }
            }
            2 => {
                if object.y.int < self.player.y.int {
                    object.ai_state = 3;
                }
                object.y.add(-0x200);
                object.x.int = object.x.int.wrapping_add(1);
                object.ai_timer = object.ai_timer.wrapping_sub(1);
                if object.ai_timer == 0 {
                    object.ai_state = 3;
                }
            }
            _ => {
                object.x.add(0x300);
                self.bounds(object);
            }
        }
    }

    /// `$B266`: drift left at 1 px/frame and home in Y at 0.5 px/frame.
    pub(in crate::game) fn ai_seeker(&mut self, object: &mut Object) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.ai_state = 0x80;
            object.ai_mode = 0;
            return;
        }
        let target = self.player.y.int;
        if object.y.int == target {
            // `$B283`/`$B2D5`: level with the ship the seeker skips the
            // `$036C` decrement and takes the `$FE` X step instead.
            object.sprite = 0x3A;
            object.x.add(-0x200);
            self.bounds(object);
            return;
        }
        object.x.int = object.x.int.wrapping_sub(1);
        object.sprite = if object.y.int < target { 0x39 } else { 0x38 };
        if object.y.int < target {
            object.y.add(0x80);
        } else {
            object.y.add(-0x80);
        }
        self.bounds(object);
    }

    /// `$B30B`: a fired shell. Waits ten frames, then dives or charges left.
    fn ai_shell(&mut self, object: &mut Object) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.timer2 = 0x0A;
            return;
        }
        // `$B318`: 8-frame animation from `$B335`, `$018C = $80` after frame 4.
        object.anim_div = object.anim_div.wrapping_add(1);
        let frame = (object.anim_div >> 2) & 0x07;
        object.sprite = tables::SPRITE_KIND9[usize::from(frame)];
        // `$B345`: the `$04CC` hover timer. While it runs the shell climbs
        // (`$B35C`: Y -= 2) and only drifts in X.
        if object.ai_mode == 0 {
            object.timer2 = object.timer2.wrapping_sub(1);
            if object.timer2 != 0 {
                object.y.add(-0x200);
                self.ai_drift(object);
                return;
            }
            object.ai_mode = 1;
        }
        // `$B34F`: once moving, a shell at or above the ship leaves left.
        if object.ai_state != 0 || self.player.y.int >= object.y.int {
            object.ai_state = 1;
            object.x.add(-0x200);
            self.bounds(object);
        } else {
            object.y.add(-0x200);
            self.ai_drift(object);
        }
    }

    /// `$B3C5`/`$B30B` dive: hover, then fall 2 px/frame while drifting.
    fn ai_diver(&mut self, object: &mut Object) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.timer2 = 0x14;
            return;
        }
        object.anim_div = object.anim_div.wrapping_add(1);
        let frame = (object.anim_div >> 2) & 0x07;
        object.sprite = tables::SPRITE_KIND9[usize::from(frame)];
        // `$B3DA`: the `$04CC` hover timer. While it runs the shell dives
        // (`$B3F1`: Y += 2) and only drifts in X.
        if object.ai_mode == 0 {
            object.timer2 = object.timer2.wrapping_sub(1);
            if object.timer2 != 0 {
                object.y.add(0x200);
                self.ai_drift(object);
                return;
            }
            object.ai_mode = 1;
        }
        // `$B3E4`: once moving, a shell at or below the ship leaves left.
        if object.ai_state != 0 || object.y.int >= self.player.y.int {
            object.ai_state = 1;
            object.x.add(-0x200);
            self.bounds(object);
        } else {
            object.y.add(0x200);
            self.ai_drift(object);
        }
    }

    /// `$B79B`: the small core. Unlike a normal handler this body runs every
    /// frame: `$B791[$048C]` is the sprite, X tracks the ship then steps
    /// `1 + $B789[rank]/256` px left, Y eases `$B793[rank]/256` px toward the
    /// ship and clamps `$14..$AC`, `$03BC` (the `y_vel` high byte) toggles
    /// `$048C` at `$B781[rank]`, and `$046C >= $B84C[rank]` scores 300 and
    /// runs `$CB20`.
    pub(in crate::game) fn ai_miniboss(&mut self, object: &mut Object) {
        let rank = usize::from(self.stage_rank()).min(tables::CORE_HITS.len() - 1);
        object.kind = 0x97;
        object.subtype = 0x80;
        object.size_class = 1;
        object.sprite = tables::CORE_FRAMES[usize::from(object.ai_state) & 7];
        if object.timer2 < 0x28 && self.player.x.int >= object.x.int {
            // `$B7C6`: the ship has caught up; crawl right until `$04CC`
            // fills, but only from below `$F0`.
            object.timer2 = object.timer2.wrapping_add(1);
            if object.x.int < 0xF0 {
                object.x.int = object.x.int.wrapping_add(2);
            }
        } else {
            // `$B7D9`: `$038C -= $B789[$17]`, `$036C -= 1 + borrow`; `$B68A`
            // despawns the slot when the high byte would underflow.
            let step = 0x100 + i32::from(tables::CORE_X_STEP[rank]);
            let x = (i32::from(object.x.int) << 8) + i32::from(object.x.frac) - step;
            if x < 0 {
                object.despawn();
                return;
            }
            object.x.int = (x >> 8) as u8;
            object.x.frac = x as u8;
        }
        // `$B7F0`: ease Y toward the ship and clamp the integer byte only,
        // keeping the fraction.
        if object.y.int >= self.player.y.int {
            object.y.sub(i32::from(tables::CORE_STEP[rank]));
        } else {
            object.y.add(i32::from(tables::CORE_STEP[rank]));
        }
        object.y.int = object.y.int.clamp(0x14, 0xAC);
        // `$B826`: the sub-state toggles `$048C`. The `$B854` branch that
        // fires the three `$0176` bullets when it turns on is not modelled.
        let threshold = tables::CORE_SUBSTATE[rank];
        let mut substate = (object.y_vel >> 8) as u8;
        if substate >= threshold {
            substate = if object.ai_state == 0 {
                object.ai_state = 1;
                1
            } else {
                object.ai_state = 0;
                0
            };
        } else if object.ai_timer >= tables::CORE_HITS[rank] {
            // `$B830`: `$8454` (300) then `$CB20`, which keeps `$012C` and
            // makes `$016C = 2` kind-2 debris: 0.5 px/frame left, first
            // sprite step immediately, and no `$BE8D` cannon release.
            self.add_score(300);
            self.kills = self.kills.wrapping_add(1);
            self.audio.push(Sound::Explosion);
            object.kind = 2;
            object.subtype = 0;
            object.anim = 2;
            object.anim_div = 0;
            object.ai_timer = 0;
            object.ai_mode = 0;
            object.x_vel = 0;
            object.phase = 0;
            return;
        } else {
            substate = substate.wrapping_add(1);
            if object.ai_state != 0 {
                substate = substate.wrapping_add(1);
            }
        }
        object.y_vel = i16::from(substate) << 8;
    }

    /// `$B47A`: aim at the ship for `$B4DE[player]` frames, then drift.
    fn ai_spin(&mut self, object: &mut Object) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.ai_state = 2;
            object.timer2 = tables::SPIN_DWELL[usize::from(self.stage_rank()) % 7];
            return;
        }
        // `$B492`: `$B622` with Y 6 - divider `$B64A[6]`, sprite base
        // `$B64B[6] = $42`, `$B64C[6]` frames. Without it the piece keeps
        // `$012C = 0`, so it is never drawn and never leaves the table.
        object.anim_div = object.anim_div.wrapping_add(1);
        if object.anim_div >= tables::ANIM_DELAY[6] {
            object.anim_div = 0;
            object.anim = if object.anim + 1 >= tables::ANIM_DELAY[8] {
                0
            } else {
                object.anim + 1
            };
            object.sprite = tables::ANIM_DELAY[7].wrapping_add(object.anim);
        }
        // `$B49A`: only the state-0 frame re-aims; states 1-2 keep `$042C`/
        // `$03BC` until the next `$BCAF`.
        if object.ai_state == 0 {
            self.aim_object(object);
        }
        if object.ai_state == 2 {
            // `$B4C2`: drift while the `$B4E5` return timer runs, then aim
            // again from state 0.
            self.ai_drift(object);
            object.timer2 = object.timer2.wrapping_sub(1);
            if object.timer2 == 0 {
                object.timer2 = tables::SPIN_RETURN[usize::from(self.stage_rank()) % 7];
                object.ai_state = 0;
            } else {
                object.ai_state = 2;
            }
            self.bounds(object);
        } else {
            // `$B4A8`: move along the aim for `$B4DE` frames, then drift.
            object.timer2 = object.timer2.wrapping_sub(1);
            if object.timer2 == 0 {
                object.timer2 = tables::SPIN_DWELL[usize::from(self.stage_rank()) % 7];
                object.ai_state = 2;
            } else {
                self.move_aimed(object);
                object.ai_state = 1;
            }
        }
    }

    /// `$B4F7`: descend until aligned with the ship, then fall away.
    pub(in crate::game) fn ai_descender(&mut self, object: &mut Object) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.ai_state = 0x80;
            object.ai_mode = 0x14;
            return;
        }
        // `$B622` with `Y = 3`: `$B64A+3` is the delay (8), `$B64B+3` the
        // sprite base (`$4A`). The divider counts to 8, then `$016C` steps
        // modulo 8 and the sprite follows; the first write is `$4B`.
        object.anim_div = object.anim_div.wrapping_add(1);
        if object.anim_div == tables::ANIM_DELAY[3] {
            object.anim_div = 0;
            object.anim = object.anim.wrapping_add(1) & 0x07;
            object.sprite = tables::ANIM_DELAY[4].wrapping_add(object.anim);
        }
        object.x.int = object.x.int.wrapping_sub(1);
        self.bounds(object);
        match object.ai_timer {
            0 => {
                object.ai_mode = object.ai_mode.wrapping_sub(1);
                if object.ai_mode == 0 {
                    object.ai_timer = 1;
                }
            }
            1 => {
                // `$B532`: at or below the ship it climbs to align (timer 2);
                // above the ship it falls away (timer 3).
                object.ai_timer = if object.y.int >= self.player.y.int {
                    2
                } else {
                    3
                };
            }
            2 => {
                // `$B540`: 0.5 px/frame up until the integer Y matches.
                object.y.add(-0x80);
                if object.y.int == self.player.y.int {
                    object.ai_timer = 4;
                }
            }
            3 => object.y.add(0x80), // `$B550`: falls away.
            _ => {}
        }
    }

    /// `$B553`: a large piece that just moves left.
    fn ai_straight(&mut self, object: &mut Object, speed: u8) {
        if object.kind & 0x80 == 0 {
            object.kind |= 0x80;
            object.ai_state = 0x80;
            object.ai_mode = 0x14;
            return;
        }
        // `$B553` -> `$B622` with Y 9: `$014C` divides by 8 and then
        // `$012C = $52 + $016C` walks the six sprite groups `$52..$57`. The
        // object is invisible (group 0) for the first eight frames.
        object.anim_div = object.anim_div.wrapping_add(1);
        if object.anim_div >= tables::STRAIGHT_ANIM_DIVIDER {
            object.anim_div = 0;
            object.anim = if object.anim + 1 >= tables::STRAIGHT_ANIM_FRAMES {
                0
            } else {
                object.anim + 1
            };
            object.sprite = tables::STRAIGHT_ANIM_SPRITE.wrapping_add(object.anim);
        }
        object.x.int = object.x.int.wrapping_sub(speed);
        self.bounds(object);
    }

    /// `$B563`: the wall. It drifts left every frame and freezes the scroll
    /// only once X drops below `$B0`; `$046C` then counts a 20-frame step from
    /// 1 and the wall despawns at 7, releasing the scroll. Stage 7: the
    /// `$0A:$C0` record spawns it at 5801, it crosses `$B0` at 5928 and holds
    /// the scroll from 5929 through 6050, despawned on 6051.
    pub(in crate::game) fn ai_wall(&mut self, object: &mut Object) {
        self.ai_drift(object);
        if object.x.int >= 0xB0 {
            return;
        }
        self.scroll_stop = true;
        if object.ai_timer == 0 {
            object.ai_timer = 1;
        } else if object.ai_timer >= 7 {
            object.despawn();
        } else {
            object.ai_mode = object.ai_mode.wrapping_add(1);
            if object.ai_mode >= 0x14 {
                object.ai_mode = 0;
                object.ai_timer = object.ai_timer.wrapping_add(1);
            }
        }
    }

    /// `$AF0A`: a background piece with a fixed sprite.
    ///
    /// `$AF0C` blanks it while `$02 & $1F >= $1A`, the six-frame window of
    /// each 32-frame cycle where the piece is not drawn. The object stays
    /// live (the dump's `$012C` test sees it as invisible, like the oracle).
    fn ai_bg_piece(&mut self, object: &mut Object) {
        let index = usize::from(object.kind & 0x07).min(tables::SPRITE_BG_PIECE.len() - 1);
        object.sprite = if self.nmi_frame & 0x1F < 0x1A {
            tables::SPRITE_BG_PIECE[index]
        } else {
            0
        };
        self.ai_drift(object);
    }

    /// `$BCAF`: point the object at the ship. Major axis 1 px/frame, minor
    /// axis `minor*256/major` per frame (`$83BA`), direction in `$046C`.
    ///
    /// `$BCDE-$BCF9` subtracts the target from the object with `SEC`; a
    /// negative delta takes the complement of `A - M`, which is `|d| - 1`, and
    /// a zero delta is forced to 1. Both axes use that adjusted magnitude.
    ///
    /// `$BD38`-`$BD74` scale both axes: `$BD3C` gates a half-step on the
    /// section sub-index `$1A`, which is 0 in the compared windows, so only
    /// the `$BD55` quarter applies. At rank 2+ (`$17`) the major axis also
    /// gains `$40` (1.25 px/frame) and the minor axis gains a quarter of the
    /// slope. `$BD61` is an `ADC` whose carry is still set from the `CMP #$02`
    /// rank test, so the scaled minor velocity is `q + q/4 + 1`, not
    /// `q*5/4`. Stage 1's rank is 1, so it stays at exactly 1 px/frame and
    /// the raw `$83BA` slope.
    pub(in crate::game) fn aim_object(&mut self, object: &mut Object) {
        let dx = i32::from(object.x.int) - i32::from(self.player.x.int);
        let dy = i32::from(object.y.int) - i32::from(self.player.y.int);
        let mut flags = 0u8;
        if dy < 0 {
            flags |= 1;
        }
        if dx < 0 {
            flags |= 2;
        }
        object.ai_timer = flags;
        let (x_vel, y_vel) = aim_vector(dx, dy, self.stage_rank() >= 2);
        object.x_vel = x_vel;
        object.y_vel = y_vel;
    }

    /// `$BDF4`: move along the aimed vector and despawn at the screen edge.
    pub(in crate::game) fn move_aimed(&mut self, object: &mut Object) {
        object.move_vel_x();
        object.move_vel_y();
        let x = object.x.int;
        let y = object.y.int;
        if !(0x02..0xFC).contains(&x) || !(0x08..0xC4).contains(&y) {
            object.despawn();
        }
    }

    /// `$BCAF`/`$BD59`: the aimed 8.8 velocity for a source at `(x, y)`, as
    /// the movement deltas `$BDF4` applies. Both `$BC3E`'s object shots and
    /// `$CBCB`'s cannon shots come through here.
    pub(in crate::game) fn fire_velocity(&self, x: u8, y: u8) -> (i32, i32) {
        let dx = i32::from(x) - i32::from(self.player.x.int);
        let dy = i32::from(y) - i32::from(self.player.y.int);
        let (vx, vy) = aim_vector(dx, dy, self.stage_rank() >= 2);
        (i32::from(vx), i32::from(vy))
    }

    /// `$BC3E`/`$CBCB`: aim one enemy shot from `(x, y)` at the ship.
    pub(in crate::game) fn push_enemy_bullet(&mut self, x: u8, y: u8, kind: u8, sprite: u8) {
        let (vx, vy) = self.fire_velocity(x, y);
        self.push_enemy_bullet_with_velocity(x, y, vx, vy, kind, sprite);
    }

    /// Spawn an enemy shot with an explicit 8.8 velocity (the core's pods).
    pub(in crate::game) fn push_enemy_bullet_with_velocity(
        &mut self,
        x: u8,
        y: u8,
        vx: i32,
        vy: i32,
        kind: u8,
        sprite: u8,
    ) {
        if self.place_enemy_bullet(x, y, vx, vy, kind, sprite) {
            self.audio.push(Sound::EnemyShot);
        }
    }

    /// `$CBCB`/`$BC55`: the scan takes the first free `$0136` entry from 9
    /// down, so a shot always lands in the highest unused proxy slot.
    /// `$CBFE-$CC0D` writes only the integer position bytes: the slot's
    /// leftover fractions supply the sub-pixel start.
    pub(in crate::game) fn place_enemy_bullet(
        &mut self,
        x: u8,
        y: u8,
        vx: i32,
        vy: i32,
        kind: u8,
        sprite: u8,
    ) -> bool {
        let Some(slot) = (0..MAX_ENEMY_BULLETS).rev().find(|slot| {
            !self.enemy_bullets[..self.enemy_bullet_count]
                .iter()
                .any(|bullet| usize::from(bullet.slot) == *slot)
        }) else {
            return false;
        };
        let (fx, fy) = self.bullet_slot_frac[slot];
        self.enemy_bullets[self.enemy_bullet_count] = EnemyBullet {
            x: (i32::from(x) << 8) | i32::from(fx),
            y: (i32::from(y) << 8) | i32::from(fy),
            vx,
            vy,
            kind,
            sprite,
            slot: slot as u8,
        };
        self.enemy_bullet_count += 1;
        true
    }

    /// `$BC13`/`$BDCF`: move the ten `$0136` proxies and drop the ones off
    /// screen. The X axis moves first and `$BE24` tests it before the Y step,
    /// so a shot that leaves through the sides keeps its Y fraction bytes at
    /// their previous value.
    pub(in crate::game) fn update_enemy_bullets(&mut self) {
        let mut write = 0;
        for read in 0..self.enemy_bullet_count {
            let mut bullet = self.enemy_bullets[read];
            let slot = usize::from(bullet.slot);
            bullet.x += bullet.vx;
            self.bullet_slot_frac[slot].0 = (bullet.x & 0xFF) as u8;
            // `$BE24`/`$BE28`: despawn outside x 2..$FC.
            let mut live = (2..0xFC).contains(&(bullet.x >> 8));
            if live {
                bullet.y += bullet.vy;
                self.bullet_slot_frac[slot].1 = (bullet.y & 0xFF) as u8;
                // `$BE5C`/`$BE60`: despawn outside y 8..$C4.
                live = (8..0xC4).contains(&(bullet.y >> 8));
            }
            if live {
                self.enemy_bullets[write] = bullet;
                write += 1;
            }
        }
        self.enemy_bullet_count = write;
    }

    /// `$9C3F`: rank = `$44 != 0` + `$45` options + `$46 != 0` + `$19 != 0`.
    /// `$9A28` (the `$82`/clear timer) is indexed by this value. Unlike
    /// [`Game::boss_rank`] it has no extra base: a shield-only stage-1 run is
    /// rank 1 (`$9A28[1] = 3`, 768 frames) and stage 2 adds the stage bit.
    pub(in crate::game) fn stage_rank(&self) -> u8 {
        (u8::from(self.power.weapon_level() != 0)
            + self.power.options()
            + u8::from(self.power.shield_hits() > 0)
            + u8::from(self.section != 0))
        .min(7)
    }

    /// `$9C3F`: rank = stage + weapon + options + shield.
    pub(in crate::game) fn boss_rank(&self) -> u8 {
        // `$9C3F`: the shield contributes one; with `$46 = 5` and no weapon or
        // options a stage-1 core uses rank 2 (`$B8F2[2] = $20`) and a stage-2
        // core rank 3 (`$B8F2[3] = $40`, `$B904[3] = $46`), matching the
        // measured stage-2 Y drift.
        (1 + u8::from(self.power.weapon_level() != 0)
            + self.power.options()
            + u8::from(self.power.shield_hits() > 0)
            + u8::from(self.section != 0))
        .min(8)
    }

    /// `$B95C`/`$BA96`: the 1536-frame timer runs out. The core becomes kind-2
    /// debris without a score bonus, the covers are cleared and the stage
    /// handoff starts like a normal kill.
    pub(super) fn boss_self_destruct(&mut self, object: &mut Object) {
        object.kind = 2;
        object.boss = false;
        object.ai_state = 0;
        object.sprite = 1;
        object.anim = 4;
        object.anim_div = 0;
        object.phase = 0;
        object.subtype = 0;
        self.objects[7] = Object::default();
        self.objects[8] = Object::default();
        self.scroll_locked = false;
        // `$B95C`: like the damage kill, the death frame itself does not step
        // the scroll; the first half-pixel step runs on the next frame.
        self.scroll_stop = true;
        self.start_stage_clear();
        self.audio.push(Sound::Explosion);
    }

    /// Keep the two `$99` covers on the core (slot 7 at its Y, slot 8 `$14`
    /// above) while the core moves.
    pub(super) fn sync_boss_parts(&mut self, x: u8, y: u8) {
        for (slot, offset, sprite) in [(7usize, 0u8, 0x32u8), (8, 0x14, 0x85)] {
            let part = &mut self.objects[slot];
            if part.kind & 0x7F == 0x19 {
                part.x = Fixed::px(x);
                part.y = Fixed::px(y.wrapping_sub(offset));
                part.sprite = sprite;
            }
        }
    }
}

/// `$BCAF`/`$BD59`: the aimed 8.8 velocity for a delta of `(dx, dy)` between
/// the source and the ship, in the sign convention `$BDF4` moves with.
fn aim_vector(dx: i32, dy: i32, scaled: bool) -> (i16, i16) {
    let magnitude = |delta: i32| -> u32 {
        if delta > 0 {
            delta as u32
        } else if delta < 0 {
            ((-delta) as u32).saturating_sub(1).max(1)
        } else {
            1
        }
    };
    let (ax, ay) = (magnitude(dx), magnitude(dy));
    let major: i16 = if scaled { 0x140 } else { 0x100 };
    let minor = |major_axis: u32, minor_axis: u32| -> i16 {
        let raw = i32::from(slope(major_axis, minor_axis));
        (if scaled { raw + raw / 4 + 1 } else { raw }) as i16
    };
    if ax >= ay {
        let vx = if dx < 0 { major } else { -major };
        let vy = minor(ax, ay);
        (vx, if dy < 0 { vy } else { -vy })
    } else {
        let vy = if dy < 0 { major } else { -major };
        let vx = minor(ay, ax);
        (if dx < 0 { vx } else { -vx }, vy)
    }
}
