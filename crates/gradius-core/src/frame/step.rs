//! The per-frame step loop: play, demo, enemy fire, scoring, state dump, effects.
use super::*;

impl Game {
    pub(in crate::game) fn step_playing(
        &mut self,
        input: Buttons,
        fire_pressed: bool,
        start_pressed: bool,
        select_pressed: bool,
    ) {
        self.update_stars();
        // A phase that does not run `$9A51` leaves the scroll latch alone.
        self.scroll_lag = 0;
        match self.phase {
            Phase::Attract(_) => {
                // `$8212`/`$8231`: Select toggles `$0F` and `$8299`
                // re-places the menu ship on `$82AC[$0F]` (134/150).
                if select_pressed {
                    self.menu_select_pending = 1;
                } else if self.menu_select_pending > 0 {
                    self.menu_select_pending -= 1;
                    if self.menu_select_pending == 0 {
                        self.menu_selection = (self.menu_selection + 1) % MENU_ENTRIES;
                        let y = DEMO_SHIP.1.wrapping_add(16 * self.menu_selection);
                        self.place_player((DEMO_SHIP.0, y));
                        // `$8271` resets the title-to-demo countdown.
                        self.attract_countdown = DEMO_TRANSITION_FRAMES;
                    }
                }
                if start_pressed {
                    // `$8212`/`$815D`: the demo's Start edge is one frame
                    // behind the `$00 = 3` transition at frame 182.
                    self.continue_pending = 1;
                } else if self.continue_pending > 0 {
                    self.continue_pending -= 1;
                    if self.continue_pending == 0 {
                        self.phase = Phase::Continue(CONTINUE_FRAMES);
                    }
                } else if self.attract_countdown > 0 {
                    // `$8193`/`$8411`: the 256-frame `$4C/$4D` countdown.
                    self.attract_countdown -= 1;
                } else {
                    // `$817E`: `$00 = 2`; the next frame runs `$82BF`.
                    self.demo = true;
                    self.phase = Phase::DemoStart;
                }
                self.update_explosions();
            }
            Phase::DemoStart => {
                // `$82BF`/`$82FF`: clear the demo's RAM and object pages, then
                // `INC $20` gives the demo its single ship. `$9B38` runs on the
                // next frame (`$00 = 5` with `$1B = 0`).
                self.new_run();
                self.set_lives(1);
                self.banner = 0;
                self.place_player((0, 0));
                self.demo = true;
                self.demo_index = 0;
                self.demo_ticks = 0;
                self.demo_button = 0;
                self.phase = Phase::StageInit;
                self.update_explosions();
            }
            Phase::Continue(frames) => {
                if frames > 1 {
                    self.phase = Phase::Continue(frames - 1);
                } else {
                    // `$815D`: the single enter-stage frame; `$82F2` loads
                    // the three lives here, not during the countdown.
                    self.phase = Phase::StageInit;
                    self.set_lives(3);
                    self.stage_clock = 0;
                }
                self.update_explosions();
            }
            Phase::StageInit => {
                // `$9B38` runs on this frame; the ship position is still zero
                // and the stage handler places it on its second frame.
                if self.demo {
                    // The demo's `$9B38` places the ship on this frame and the
                    // shorter intro reports `$80` at frame 419.
                    self.set_lives(1);
                    self.place_player(self.ship_start(self.section, self.scroll.high));
                    self.phase = Phase::StageIntro(DEMO_INTRO_FRAMES);
                } else {
                    self.set_lives(3);
                    self.place_player((0, 0));
                    self.phase = Phase::StageIntro(STAGE_INTRO_FRAMES);
                }
                // `$9B38`'s page upload overruns the NMI once.
                self.nmi_lag = true;
                self.stage_clock = 0;
                self.update_explosions();
            }
            Phase::StageIntro(frames) => {
                if frames > 1 {
                    self.phase = Phase::StageIntro(frames - 1);
                    if frames == STAGE_INTRO_FRAMES {
                        // `$9B82`: the ship start from `$9BC6/$9BCE`.
                        self.place_player(self.ship_start(self.section, self.scroll.high));
                    }
                } else {
                    self.phase = Phase::Normal;
                    self.scroll_hold = STAGE_SCROLL_HOLD - 1;
                    // The oracle unmasks on f296; `+1` here blanked one
                    // extra frame in the port.
                    self.entry_blank = STAGE_SCROLL_HOLD;
                    self.stage_clock = 0;
                }
                self.update_explosions();
            }
            Phase::Blank(frames) => {
                if frames > 0 {
                    self.phase = Phase::Blank(frames - 1);
                } else {
                    self.phase = Phase::Init(0);
                    self.stage_clock = 0;
                }
                self.update_explosions();
            }
            Phase::Init(step) => {
                if self.endgame == Endgame::None && self.ending_step != 0 {
                    // The 27-frame stage-1 boot after `$8D`, not the normal
                    // 5-frame `$9B38` init the `step` counter models.
                    if self.ending_timer > 0 {
                        self.restart_boot_frame();
                        self.update_explosions();
                    } else {
                        self.end_restart_boot();
                        self.normal_frame(input, fire_pressed);
                    }
                } else if step < 4 {
                    self.phase = Phase::Init(step + 1);
                    self.update_explosions();
                } else {
                    self.phase = Phase::Normal;
                    self.spawner.restart(self.section, self.scroll.high);
                    self.stage_clock = 0;
                    self.update_explosions();
                }
            }
            Phase::Transition(frames) => {
                if frames > 0 {
                    self.phase = Phase::Transition(frames - 1);
                } else {
                    self.advance_section();
                }
                self.update_explosions();
            }
            Phase::Dying(frames) => {
                // `$1B = $A0` still dispatches to the `$9A51` tail, so the
                // spawner, object AI, collisions and scroll keep running while
                // the player explosion counts down.
                if let Some(burst) = &mut self.death_burst {
                    burst.age = burst.age.saturating_add(1);
                }
                self.normal_frame(input, fire_pressed);
                if frames > 0 {
                    self.phase = Phase::Dying(frames - 1);
                } else {
                    self.finish_death();
                }
                self.update_explosions();
            }
            Phase::Cleared(frames) => {
                if frames > 0 {
                    self.phase = Phase::Cleared(frames - 1);
                } else {
                    self.state = State::GameOver;
                }
                self.update_explosions();
            }
            Phase::Normal => {
                if self.demo {
                    let (control, end) = self.demo_control(start_pressed, select_pressed);
                    // `$9C82` writes the held byte into `$05` as well as `$07`,
                    // so the ROM's `$9A`/`$9B` fire bits are both set every
                    // frame of a held demo record.
                    self.normal_frame(control, control.fire);
                    if end {
                        self.finish_demo();
                    }
                } else {
                    self.normal_frame(input, fire_pressed);
                }
            }
        }
    }

    /// `$9C82`: the demo's `$07` byte uses the ROM's reverse button order
    /// (A bit 7, Right bit 0).
    fn demo_buttons(bits: u8) -> Buttons {
        Buttons {
            fire: bits & 0x80 != 0,
            power: bits & 0x40 != 0,
            select: bits & 0x20 != 0,
            start: bits & 0x10 != 0,
            up: bits & 0x08 != 0,
            down: bits & 0x04 != 0,
            left: bits & 0x02 != 0,
            right: bits & 0x01 != 0,
        }
    }

    /// `$9C67`: advance the demo input table. The returned flag is `$0B`
    /// becoming non-zero (table terminator or a Start/Select edge).
    fn demo_control(&mut self, start_pressed: bool, select_pressed: bool) -> (Buttons, bool) {
        // `$9C6C`/`$9C58`: before the first record, install the demo loadout.
        if self.demo_index == 0 {
            self.power.demo_loadout();
        }
        // `$9C73`: a Start/Select edge ends the demo before the next record.
        if start_pressed || select_pressed {
            return (Self::demo_buttons(self.demo_button), true);
        }
        // `$9C79`: `$02`'s low bit alternates. `$02 = frame - 5`, so an even
        // port frame is the event/odd-`$02` path that loads a record and
        // decrements its tick.
        if self.frame % 2 == 0 {
            if self.demo_index == 0 || self.demo_ticks == 0 {
                let ticks = self.rom().tables.demo_input[usize::from(self.demo_index) + 1];
                if ticks == 0xFF {
                    return (Self::demo_buttons(self.demo_button), true);
                }
                self.demo_ticks = ticks;
                self.demo_index += 2;
            }
            // `$9C82`: `$9CAF,Y` is the button byte for this record.
            self.demo_button = self.rom().tables.demo_input[usize::from(self.demo_index) - 2];
            self.demo_ticks = self.demo_ticks.wrapping_sub(1);
        }
        (Self::demo_buttons(self.demo_button), false)
    }

    /// `$8187`: `$0B` ended the demo; `$00 = 0` restarts the title roll-in.
    pub(in crate::game) fn finish_demo(&mut self) {
        self.state = State::Title;
        self.demo = false;
        self.phase = Phase::Attract(0);
        self.title_timer = 0;
        self.title_attract_at = TITLE_ATTRACT_REENTRY;
        self.continue_pending = 0;
        self.demo_index = 0;
        self.demo_ticks = 0;
        self.demo_button = 0;
        self.set_lives(0);
    }

    /// `$9A40` then `$9A51`: spawner, objects, player, AI, collision, power.
    pub(in crate::game) fn normal_frame(&mut self, input: Buttons, fire_pressed: bool) {
        // `$87`..`$8D`: the ending runs its own phase machine; `$9A51` and
        // the gameplay passes do not run.
        if self.endgame == Endgame::Ending {
            self.ending_frame();
            return;
        }
        // `$01..$04`: the `$8D` frame left the stage-1 boot armed; the entry
        // runs 27 frames at `Phase::Init` without touching gameplay.
        if self.endgame == Endgame::None && self.ending_step != 0 {
            if self.ending_timer > 0 {
                self.restart_boot_frame();
                return;
            }
            self.end_restart_boot();
        }
        // The frame the scroll first locks still shows the previous latch,
        // and the frame after the `$B563` wall despawns keeps the held one.
        let mut freeze_latch = false;
        let resume_latch = self.wall_resume;
        self.wall_resume = false;
        self.kept_hit = false;
        if self.banner > 0 {
            self.banner -= 1;
        }
        self.stage_clock = self.stage_clock.wrapping_add(1);
        // `$9649`: `$5B = 0` every frame; handlers (the `$B563` wall, the
        // checkpoint states) set it again to freeze this frame's scroll.
        self.scroll_stop = false;
        // `$5D` and `$0E` are frame-scoped: the NMI clears the VRAM queue and
        // every spawn/effect raises the busy flag for the turret handlers.
        self.spawn_busy = false;
        self.vram_queue_len = 0;
        // `$1B = $C0`: the game-over handler keeps `$5B` set, so the stage
        // animates but the scroll and every `$AED7` drift stay frozen.
        if self.game_over_timer > 0 {
            self.scroll_stop = true;
        }

        // `$9658`: stage 5's `$19 == 4` alternate chain. With two or more live
        // `$0600` cannons on an odd `$02` frame (port frame even) the spawner,
        // object AI, player, cannon and scroll passes run; `$9A51` skips them
        // on the other frames. `stage5_slowdown` drops that halving.
        let cannon_chain =
            self.section == 4 && self.mods.stage5_slowdown && self.live_cannon_count() >= 2;
        let chain = cannon_chain && self.frame % 2 == 0;

        // `$1B = $81..$90` on the no-core stages. `$81`/`$82` hold the scroll
        // for `$9A28[$17]` frames; `$83` and `$86`'s first frame keep holding
        // it, then `$86` runs as normal play (`$A2EA` only routes `$82` away
        // from the column streams) and the wall erase starts.
        // `$C40D` still dispatches on the frame `$82` hands over to `$83`
        // (oracle 6953), before this match moves the label.
        let beam_window =
            self.section == 5 && matches!(self.endgame, Endgame::Hold(_)) && self.scroll_locked;
        match self.endgame {
            Endgame::Hold(timer) if self.scroll_locked => {
                self.endgame = if timer <= 1 {
                    Endgame::Bridge(2)
                } else {
                    Endgame::Hold(timer - 1)
                };
            }
            Endgame::Bridge(frames) if self.scroll_locked => {
                if frames == 0 {
                    // `$98F7` on stage 7 jumps to `$9865` on the frame *after*
                    // the `$86` one; the ending states then take over.
                    if self.stage_info().quirks & stage_catalog::QUIRK_LOOP != 0 {
                        self.begin_ending();
                        return;
                    }
                    self.endgame = Endgame::Erase;
                    self.scroll_locked = false;
                } else {
                    // `$99B3`/`$EC18`: stage 6 requests the stage-clear cue
                    // `$AC` on the `$83` -> `$86` bridge (oracle 6955), one
                    // frame before the first `$CD9F` erase pair.
                    if frames == 1 && self.section == 5 {
                        self.music = STAGE_CLEAR_CUE;
                    }
                    self.endgame = Endgame::Bridge(frames - 1);
                }
            }
            Endgame::Advance => {
                // `$90`: `$96C2` runs at the start of the frame, so the new
                // stage's spawner, objects and scroll step all run this frame.
                // `$3E`/`$3D` keep their values, so the first stage-7 frame
                // samples `$3E:$3D = 1:00`.
                self.advance_section();
                // `$9C36` leaves `$60 = 1`, so this frame's `$A2BA` call only
                // re-initialises the column pointer and the first spawn lands
                // on the next frame (measured 7468/7469).
                self.spawner.defer_init();
                self.endgame = Endgame::None;
            }
            _ => {}
        }
        if self.endgame == Endgame::Erase {
            self.erase_wall();
        }

        // `$81`/`$82` eruption, then `$83`/`$84` waits for `$3F = $0D`.
        // `$84` spawns the core on the transition frame itself, so the slot
        // exists at 7723 rather than one frame later.
        if self.boss_stage == 1 && self.scroll_locked {
            // `$82`'s 768-frame countdown only starts once `$5B` is set, one
            // frame after `$81` arms the sequence. `$9695` jumps to `$9A7F`,
            // past `$96E2`'s `DEC $4C`, on the `$5C >= 2` odd-`$02` chain
            // frames, so the hold counter does not advance there.
            if !chain {
                self.boss_timer = self.boss_timer.saturating_sub(1);
            }
            if self.boss_timer == 0 {
                self.boss_stage = 2;
                self.scroll_locked = false;
            }
        } else if self.boss_stage == 2 && self.scroll.high > self.stage_boss_trigger() {
            self.boss_stage = 3;
            self.scroll_locked = true;
            freeze_latch = true;
        }
        if self.boss_stage == 3 {
            self.spawn_boss();
            self.boss_stage = 4;
        }

        // `$C40D`/`$C433[5] = $C6D8`: stage 6's `$82` window spawns a beam
        // every fourth `$02` frame. `$C449` advances `$69` before the free-slot
        // scan, so the nibble is consumed even when all ten boxes are busy.
        // The boxes move every frame, including the ones that outlive the
        // window, until they leave the playfield.
        if beam_window {
            let nmi = self.nmi_frame;
            if nmi & 3 == 0 {
                let nibble = self.hazard_nibble(&self.rom().tables.beam_script);
                self.beams.spawn_attempt(nmi, nibble);
            }
        }
        self.beams.step();

        // $A2BA: event stream. During the `$82` eruption `$A2BA` jumps to
        // `$C40D`, the per-stage hazard spawner, instead of the column streams.
        if self.boss_stage == 1 && self.scroll_locked {
            if !cannon_chain || chain {
                self.eruption_spawn();
            }
        } else if self.boss_stage == 0
            && self.game_over_timer == 0
            && !cannon_chain
            && !matches!(self.endgame, Endgame::Hold(_) | Endgame::Bridge(_))
        {
            // `$1B = $C0` does not run `$A2BA`; the stage is frozen anyway.
            // `$82`/`$83` route `$A2EA` to `$C40D` instead of the streams.
            if let Some(emission) = self.spawner.step(self.scroll) {
                match emission {
                    Emission::Spawn(event) => {
                        let spawned = self.spawn_object(event);
                        self.spawner.commit(spawned);
                    }
                    Emission::Command(bytes) => self.command(bytes),
                }
            }
            // `$A32F`: consuming a stream record raises `$5D`; waiting for a
            // formation member (`$A325`) does not.
            if self.spawner.record_read {
                self.spawn_busy = true;
            }
        } else if chain && self.game_over_timer == 0 {
            // `$9681`: the chain runs the spawner itself.
            if let Some(emission) = self.spawner.step(self.scroll) {
                match emission {
                    Emission::Spawn(event) => {
                        let spawned = self.spawn_object(event);
                        self.spawner.commit(spawned);
                    }
                    Emission::Command(bytes) => self.command(bytes),
                }
            }
            if self.spawner.record_read {
                self.spawn_busy = true;
            }
        }

        if chain {
            // `$9684`/`$9687`/`$968A`/`$968D`: cannons, object AI, object
            // fire, player (in the ROM's order), then `$C0C1`.
            self.update_cannons();
            self.update_objects();
            self.update_object_fire();
            self.update_players(input, fire_pressed);
            self.update_players_shots();
            self.update_enemy_bullets();
            // `$9690`: the chain's own `$C0C1`, the ship half of the
            // collision pass. `$BFDC`'s shot scan is the skipped part.
            self.collide_ship();
        } else if !cannon_chain {
            // `$BBB1`: the object-fire scan runs after `$A2BA` and before
            // `$9FF6`, so it sees last frame's object and ship positions.
            self.update_object_fire();

            // $9FF6: player movement, trail, options, fire. The original runs
            // it between `$BBB1` (object motion) and `$ADA5` (enemy AI), so
            // the AI sees this frame's ship position.
            self.update_players(input, fire_pressed);

            // `$B563`'s door paints one frame after the wall's `$046C` steps
            // (the `$0700` queue flushes in the next NMI), so it reads the
            // counter this frame starts with.
            let door_timer = self.stage7_wall_timer();

            // $BBB1: object motion/animation, $ADA5: enemy AI and fire.
            self.update_objects();
            freeze_latch |= self.paint_stage7_door(door_timer);

            // $A169: player shots.
            self.update_players_shots();
            self.update_enemy_bullets();
        }
        self.update_explosions();

        if !chain {
            // $BFDC/$C0FB/$C204: collisions and cleanup. Stage 1 also runs the
            // `$C39D` ship-vs-map pass (see `collide_player_terrain`); the map
            // pages become non-zero in the pre-boss columns.
            self.collide_players();

            // $8979/$898E: `$0100 == 1` plus held B (`$07` bit 6) applies the
            // selected power on the same frame as the collision that advanced
            // `$42`. A successful handler clears it, so held B cannot repeat it.
            self.apply_players_power(input.power);

            // `$9A69` -> `$C76C` -> `$CB84` -> `$CB8B`: stage 5 ticks the
            // cannons every frame while the frame-start `$5C` is below two.
            // Frames with two or more cannons already ticked in the `$9684`
            // chain and `$C76C` jumps past `$9A69` there. No frame parity.
            if self.section == 4 && !cannon_chain {
                self.update_cannons();
            }
        }

        // `$86`/`$9928`/`$9842`/`$96C4`: with the `$39` gate the clear scroll
        // runs *before* the half-pixel step, so the frame whose `$3F` first
        // equals `$98F0[$19]` is the frame that advances `$19`.
        match self.clear {
            Clear::WaitEnd => {
                let end =
                    tables::STAGE_END[usize::from(self.section).min(tables::STAGE_END.len() - 1)];
                if self.scroll.high >= end {
                    // `$992E`: `INC $19`, `$3A = 1`, `$3F = 0`, `$1B = $8E`.
                    self.section = (self.section + 1).min((stage::SECTION_COUNT - 1) as u8);
                    self.bonus_wave = true;
                    self.scroll.high = 0;
                    self.clear = Clear::Bonus;
                }
            }
            Clear::Bonus => {
                // `$8407`: `$3E += 4` with carry into `$3F`, on top of `$98E1`.
                let sum = u16::from(self.scroll.low) + 4;
                self.scroll.low = sum as u8;
                self.scroll.high = self.scroll.high.wrapping_add((sum >> 8) as u8);
                if self.scroll.high >= 0x11 {
                    // `$845A` with `A = $50`: BCD 5000, once.
                    self.add_score(5000);
                    self.clear = Clear::Handoff;
                }
            }
            Clear::Handoff => {
                // `$96C4`: the second `INC $19` clears `$39/$3A/$3F`; stage 4
                // is stage 5, so the bonus clear skips stage 4.
                self.advance_section();
                self.clear = Clear::None;
            }
            Clear::None => {}
        }

        // `$86`: `$98F7` compares `$3F` with `$98F0[$19]` (`$0C` for stage
        // 6) and hands the next frame to `$90`.
        if self.endgame == Endgame::Erase
            && self.scroll.high
                >= tables::STAGE_END[usize::from(self.section).min(tables::STAGE_END.len() - 1)]
        {
            self.endgame = Endgame::Advance;
        }

        // `$9A40` compares the `$3F` the frame *started* with, before the
        // scroll step: the arm decision is taken now and committed below.
        let pre_step_high = self.scroll.high;
        // $9A8B/$98E1: scroll last in the frame. `$5B` freezes it (`$B563`)
        // and `$9A87` gates it until the `$0D` blank countdown ends. The
        // stage-5 chain raises `$5B` after its AI pass (`$9693`).
        if chain {
            self.scroll_stop = true;
        }
        self.scroll_lag = 0;
        if self.scroll_hold > 0 {
            self.scroll_hold -= 1;
        } else if !self.scroll_stop && !self.scroll_locked {
            self.scroll.step();
            self.scroll_lag = if resume_latch { 0 } else { 1 };
        } else if freeze_latch {
            // The PPU latch is one frame behind on the freeze frame.
            self.scroll_lag = 1;
        }

        // `$1C`: the loop restart keeps the ending's `$A5` on the first
        // playable frame (8282) and queues the shared intro `$93` one frame
        // later, rather than restarting it on the `$8D` frame.
        if self.ending_timer > 0 && self.music == ENDING_MUSIC {
            self.ending_timer -= 1;
            if self.ending_timer == 0 {
                self.music = SHARED_INTRO_MUSIC;
            }
        }

        // `$8354`: `$1C` leaves the shared intro for the stage theme when
        // `$3F` first reaches the stage's boundary (stage 4: frame 31304).
        if self.music == SHARED_INTRO_MUSIC && self.scroll.high >= self.music_boundary() {
            self.music = self.stage_music();
        }

        // `$1B = $81` arms on the frame `$3F` reaches `$0C`; `$82` then sets
        // `$5B` on the *next* frame, so the final half-pixel step still runs
        // (the oracle holds at `$0C:128`, not `$0C:00`).
        let mut armed_now = false;
        let quirks = self.stage_info().quirks;
        let loop_stage = quirks & stage_catalog::QUIRK_LOOP != 0;
        let no_core = quirks & (stage_catalog::QUIRK_NO_CORE | stage_catalog::QUIRK_LOOP) != 0;
        if no_core
            && self.endgame == Endgame::None
            && self.clear == Clear::None
            && !self.boss_spawned
            // The kept-alive oracle is hit every frame here; the `$A0` write
            // cancels the `$81` this frame armed, so the clear never sticks.
            && !self.kept_hit
            && pre_step_high >= self.stage_boss_trigger()
        {
            // `$99B3` skips `$84`/`$85`: no core, `$81 -> $82 -> $83 -> $86`
            // and the stage ends on `$98F0[$19]`. `$9A01` reads the timer
            // from the live rank, not the stage: stage 6 at rank 2 is
            // `$9A28[2] = 4` (1024 frames). Stage 7 overrides it with the
            // fixed `$4D = 1, $4C = $CA` (458 frames).
            self.endgame = Endgame::Hold(if loop_stage {
                458
            } else {
                let rank = usize::from(self.boss_rank()).min(tables::CHECKPOINT_TIMER.len() - 1);
                u16::from(tables::CHECKPOINT_TIMER[rank]) * 256
            });
            // `$99D2` clears `$66`.
            self.wall_erase = 0;
            self.beams.clear();
            armed_now = true;
        } else if !no_core
            && self.boss_stage == 0
            && self.clear == Clear::None
            && !self.boss_spawned
            && self.scroll.high >= self.stage_boss_trigger()
        {
            self.boss_stage = 1;
            // `$9A01`: `$4D = $9A28[$17]`, `$4C = 0`. The 16-bit countdown
            // is therefore `$9A28[$17]` 256-frame ticks (768 in stage 1 with
            // the pinned shield, 1024 in stage 2 because `$19 != 0` adds one
            // to the rank). The extra 3 frames are the `$81`/`$83` states and
            // the frame the scroll lock engages, which the port folds into
            // this counter so stage 1 stays byte-identical.
            let rank = usize::from(self.stage_rank()).min(tables::CHECKPOINT_TIMER.len() - 1);
            self.boss_timer = u16::from(tables::CHECKPOINT_TIMER[rank]) * 256 + 3;
            armed_now = true;
        }
        if !armed_now && self.boss_stage == 1 && !self.scroll_locked {
            // `$82`: the 768-frame volcano eruption holds the scroll.
            self.scroll_locked = true;
        }
        if armed_now && matches!(self.endgame, Endgame::Hold(_)) {
            // `$81`: the clear window holds the scroll from the next frame.
            // The arm itself models `$9A40` on this frame, so the next frame
            // is `$82` (which this flag models) and `$81`'s timer starts
            // decrementing there.
            self.scroll_locked = true;
        }
        if self.boss_clear > 0 {
            // `$98F7`/`$96C4`: the normal `$98F0` end advances the stage
            // without `$39`; `$39` only selects the longer `$9928` bonus
            // branch. The death frame itself does not count, so 514 frames
            // after the kill is the first stage frame.
            if self.boss_clear_armed {
                self.boss_clear -= 1;
                if self.boss_clear == 0 {
                    self.advance_section();
                }
            } else {
                self.boss_clear_armed = true;
            }
        } else if self.boss_stage == 0
            && self.clear == Clear::None
            && self.endgame == Endgame::None
            // A kept-alive hit also cancels `$81` on a no-core stage, so the
            // shared `$9A30` transition must not run in its place.
            && !self.kept_hit
            // The no-core stages end through `$99B3`/`$9865`, never the
            // shared section transition (which would otherwise fire on the
            // post-step `$3F` of the arm frame).
            && !no_core
            // Stage 1 ends through the `$81`-`$84` core sequence, not the
            // normal section transition (which would fire on the frame the
            // scroll first reaches `$0C00`, one frame before `$82` arms).
            && self.section != 0
            && stage::section_done(self.section, self.scroll)
            && !self.player.dying
        {
            self.phase = Phase::Transition(stage::transition_frames(self.section));
        }
        self.entry_blank = self.entry_blank.saturating_sub(1);
    }

    /// `$046C` of the live `$1E` wall, read before the AI pass.
    pub(in crate::game) fn stage7_wall_timer(&self) -> Option<u8> {
        if self.section != crate::stage7_door::SECTION {
            return None;
        }
        self.objects
            .iter()
            .find(|object| object.live() && object.kind == 0x1E)
            .map(|object| object.ai_timer)
    }

    /// `$B563`/`$B600`: the stage-7 door closes as the wall's `$046C` steps.
    ///
    /// `$04AC` counts twenty frames and `$046C` picks the row pair. The ROM
    /// queues the paint on the frame the counter moves and the NMI applies it
    /// at the start of the next one, so `timer` is the value the frame started
    /// with. Returns true on the wall's first freeze frame: the PPU still
    /// shows the previous scroll latch there, the one-pixel lag `scroll_lag`
    /// also keeps for the boss lock. On the despawn frame it arms
    /// [`Game::wall_resume`] for the first step after the hold.
    pub(in crate::game) fn paint_stage7_door(&mut self, timer: Option<u8>) -> bool {
        let Some(timer) = timer else {
            return false;
        };
        // The wall freezes the scroll for the first time on this frame; the
        // AI pass has already raised `$046C` to 1.
        let latch = self.objects.iter().any(|object| {
            object.live() && object.kind == 0x1E && object.ai_timer == 1 && object.ai_mode == 0
        });
        // `$046C` reaches 7 on the frame the wall despawns.
        self.wall_resume = timer >= 7;
        let step = timer.saturating_sub(1).min(crate::stage7_door::STEPS);
        for row in crate::stage7_door::ROWS {
            let Some(tile) = crate::stage7_door::tile(step, row) else {
                continue;
            };
            for column in crate::stage7_door::COLUMNS {
                match self
                    .cells
                    .iter_mut()
                    .find(|cell| cell.row == row && cell.world_col == column)
                {
                    Some(cell) => cell.tile = tile,
                    None => self.cells.push(crate::stage3_enemies::Cell {
                        world_col: column,
                        row,
                        tile,
                    }),
                }
            }
        }
        latch
    }

    /// `$BBB1`: the object-fire scan, which runs after `$A2BA` and before
    /// `$9FF6` (so it aims with last frame's object and ship positions).
    ///
    /// With `$5D` clear, walk slots 9 down to 0 and count `$040C` down. The
    /// first underflow whose `$04EC` is non-zero fires one `$BC3E` bullet and
    /// ends the scan for the frame.
    pub(in crate::game) fn update_object_fire(&mut self) {
        // The attract demo is a pinned input playback, not a ROM parity
        // target; its bullets would change the recorded title timeline.
        if self.demo || self.spawn_busy {
            return;
        }
        let decrement = self.fire_decrement();
        for slot in (0..LIVE_SLOTS).rev() {
            let object = self.objects[slot];
            if object.kind & 0x7F < 3 {
                continue;
            }
            if object.fire_timer >= decrement {
                self.objects[slot].fire_timer = object.fire_timer - decrement;
                continue;
            }
            // `$BBFE`: a zero `$04EC` leaves `$040C` untouched and the scan
            // goes on; otherwise `$BC03` reloads `$040C` from `$04EC` and the
            // object fires.
            if object.fire_motion != 0 {
                self.objects[slot].fire_timer = object.fire_motion;
                self.fire_object(slot);
                break;
            }
        }
    }

    /// `$BBB0-$BBE6`: the per-frame `$040C` decrement. An odd `$02` frame
    /// takes 1. `$1A` is 0 in the measured windows, so an even frame takes the
    /// `$BBD4` branch: the shield term adds one at stage 2+. Rank 3+ adds one
    /// more.
    fn fire_decrement(&self) -> u8 {
        let mut decrement = 1;
        if self.section >= 2 && self.nmi_frame & 1 == 0 && self.power.shield_hits() != 0 {
            decrement += 1;
        }
        if self.stage_rank() >= 3 {
            decrement += 1;
        }
        decrement
    }

    /// `$BC3E`: allocate one `$0136` bullet at the `$0496` muzzle and aim it
    /// with `$BCAF`/`$BD59`.
    fn fire_object(&mut self, slot: usize) {
        let object = self.objects[slot];
        // `$BC42`: stage 1 only lets an object ahead of the ship fire.
        if self.section == 0 && self.player.x.int >= object.x.int {
            return;
        }
        // `$BC5E`/`$BC60`: subtype `$80..$8F` is the large `$59` bullet.
        let large = object.subtype & 0x80 != 0 && object.subtype < 0x90;
        let (sprite, kind) = if large { (0x59, 1) } else { (0x25, 0) };
        let index = usize::from(object.muzzle).min(FIRE_MUZZLE_X.len() - 1);
        let x = object.x.int.wrapping_add(FIRE_MUZZLE_X[index] as u8);
        let y = object.y.int.wrapping_add(FIRE_MUZZLE_Y[index] as u8);
        let (vx, vy) = self.fire_velocity(x, y);
        if !self.place_enemy_bullet(x, y, vx, vy, kind, sprite) {
            return;
        }
        // `$83BA` increments `$5D` for every aimed shot.
        self.spawn_busy = true;
    }

    pub(in crate::game) fn add_score(&mut self, points: u32) {
        // `$8479`: the bank is `$07E4 + 4*$18`; the public score stays the
        // P1 mirror the harness and frontends read.
        let current = self.two.current;
        let slot = &mut self.two.slots[current];
        if current == 0 {
            self.score_bcd.add(points);
            self.score = self.score_bcd.value();
            slot.score = self.score;
        } else {
            slot.score = slot.score.saturating_add(points);
        }
        let score = slot.score;
        let value = if current == 0 {
            self.score_bcd
        } else {
            Bcd::from_decimal(score)
        };
        // `$84FC-$8514`: after every add, compare the score with
        // `$07E0-$07E2` from the most significant byte down and copy the
        // score over when it is larger. The high score never decreases and
        // is not cleared by a new game; only the BCD overflow saturation can
        // cap it.
        if score > self.high_score_bcd.value() {
            self.high_score_bcd = value;
        }
    }

    /// One line of stable, machine-readable simulation state for the
    /// differential harness.
    ///
    /// Format (fields separated by single spaces, no trailing newline):
    ///
    /// ```text
    /// frame=<u64> state=<Title|Playing|GameOver>
    /// px=<u8> pf=<u8> py=<u8> pyf=<u8> speed=<u8>
    /// scroll=<low>:<high>:<fraction>
    /// score=<u32> lives=<u8> capsule=<u8> meter=<u8>
    /// objects=<slot>:<type>:<x_int>:<y_int>:<substate>;...
    /// ```
    ///
    /// `type` is the raw `$030C` byte (bit 7 is the handler-initialised flag),
    /// `substate` is `$048C`, positions are integer pixels. `objects` is empty
    /// when no object is live. The format is append-only: new fields go at the
    /// end so existing parsers keep working.
    ///
    /// While the stage is not running (title, the attract window and the six
    /// blank/five init frames) the oracle's `$0360`/`$0320`/`$3E` pages are
    /// untouched. The native port keeps its real ship position internally but
    /// dumps zeros there, so the harness does not report a frame-1 artifact.
    /// The stage-entry phases use the oracle's own `Other(n)` spelling:
    /// `Other(1)` for the attract window, `Other(2)` during the blank frames
    /// and `Other(4)` for the enter-stage init, matching `$00` values 1-4.
    pub fn dump_state(&self) -> String {
        let state = match self.state {
            State::Title => "Title",
            State::GameOver => "GameOver",
            State::Playing if self.demo => "Other(2)",
            State::Playing => match self.phase {
                Phase::Attract(_) => "Other(1)",
                Phase::DemoStart => "Other(2)",
                Phase::Continue(_) => "Other(3)",
                Phase::StageInit => "Other(4)",
                Phase::StageIntro(_) => "Other(4)",
                Phase::Blank(_) => "Other(2)",
                Phase::Init(_) => "Other(4)",
                _ => "Playing",
            },
        };
        let live = match self.state {
            // `$824E`/`$8299` place the title cursor ship on the first frame;
            // frame 0 (before the first step) is still all zeroes.
            State::Title => self.title_timer > 0,
            State::GameOver => false,
            State::Playing => {
                self.demo
                    // The `$01..$04` loop restart reports `$1B` through
                    // `Phase::Init` but the ship and scroll are live.
                    || (self.endgame == Endgame::None && self.ending_step != 0)
                    || !matches!(
                        self.phase,
                        Phase::Continue(_)
                            | Phase::StageInit
                            | Phase::StageIntro(_)
                            | Phase::Blank(_)
                            | Phase::Init(_)
                    )
            }
        };
        let (px, pf, py, pyf, speed, low, high, fraction) = if live {
            (
                self.player.x.int,
                self.player.x.frac,
                self.player.y.int,
                self.player.y.frac,
                self.power.speed_level(),
                self.scroll.low,
                self.scroll.high,
                self.scroll.fraction,
            )
        } else {
            (0, 0, 0, 0, 0, 0, 0, 0)
        };
        let mut line = format!(
            "frame={} state={} px={} pf={} py={} pyf={} speed={} scroll={}:{}:{} score={} lives={} capsule={} meter={} objects=",
            self.frame,
            state,
            px,
            pf,
            py,
            pyf,
            speed,
            low,
            high,
            fraction,
            self.score,
            self.lives,
            self.power.capsules(),
            self.power.selection(),
        );
        let mut first = true;
        // `$A521` clears a free slot at `$A8 + $0C`, so the object arrays start
        // at `$010C`/`$030C` and the position pairs at `$032C+s`/`$036C+s`: one
        // 12-byte player block past the `$0120`/`$0320`/`$0360` bases. Slot 0
        // is the ship in the original, which the port keeps outside the table.
        for (slot, object) in self.objects.iter().enumerate().take(LIVE_SLOTS).skip(1) {
            // `$030C != 0` is the liveness test and `$012C != 0` the "has a
            // sprite" test; `$BE8D` clears `$012C` when it turns a killed
            // object into kind-2 debris, so both sides agree on what the table
            // is showing. The aiming turrets (`$B067`) keep `$012C = $B080`
            // and put the drawable `$B086/$B08C` group - zero at index 5 - in
            // `$0496`, so the port's `sprite` is not their `$012C`.
            let drawable = object.kind & 0x7F;
            let group_only = drawable == 0x11 || drawable == 0x12;
            if !live || !object.live() || (object.sprite == 0 && !group_only) {
                continue;
            }
            if !first {
                line.push(';');
            }
            first = false;
            line.push_str(&format!(
                "{}:{}:{}:{}:{}",
                slot, object.kind, object.x.int, object.y.int, object.ai_state
            ));
        }
        line
    }

    /// The last game-over frame: `$9B38` clears the stage and places the ship
    /// at the checkpoint, one frame before `$817E` shows the title.
    pub(in crate::game) fn reset_for_title(&mut self) {
        self.scroll.high = self.death_checkpoint;
        self.scroll.low = 0;
        self.scroll.fraction = 0;
        self.power.reset();
        self.reset_section_objects();
        self.cells.clear();
        self.collision_cells.clear();
        self.broken.fill(0);
        self.spawner.restart(self.section, self.scroll.high);
        self.place_player(self.ship_start(self.section, self.scroll.high));
    }

    pub(in crate::game) fn add_explosion(&mut self, x: i32, y: i32, life: u8) {
        if self.explosion_count >= MAX_EXPLOSIONS {
            return;
        }
        self.explosions[self.explosion_count] = Explosion {
            x,
            y,
            life,
            max_life: life,
        };
        self.explosion_count += 1;
    }

    pub(in crate::game) fn update_explosions(&mut self) {
        let mut write = 0;
        for read in 0..self.explosion_count {
            let mut explosion = self.explosions[read];
            explosion.life = explosion.life.saturating_sub(1);
            if explosion.life > 0 {
                self.explosions[write] = explosion;
                write += 1;
            }
        }
        self.explosion_count = write;
    }

    pub(in crate::game) fn update_stars(&mut self) {
        let Self { stars, rng, .. } = self;
        for star in stars.iter_mut() {
            let speed = [64, 128, 256][usize::from(star.layer)];
            star.x -= speed;
            if star.x < 0 {
                star.x += WIDTH as i32 * 256;
                star.y = rng.range(0, HEIGHT as i32) as i16;
            }
        }
    }
}
