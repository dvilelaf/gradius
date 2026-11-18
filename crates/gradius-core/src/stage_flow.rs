//! Phase flow: stage boot, section handoff, clear/endgame sequences and the
//! loop-restart gate.
//!
//! Moved verbatim from `game.rs` (card R1b). Every function keeps its name, its
//! body and its place in the frame; the only edits are the `pub(super)` markers
//! the module boundary needs.

use super::TITLE_ATTRACT_REENTRY;
use super::*;

/// `$EC18`'s stage-clear cue on the stage-6 `$83` bridge (`$99B3`). The core
/// has no `Sound` variant for it, so [`Game::music`] carries it.
pub(super) const STAGE_CLEAR_CUE: u8 = 0xAC;

/// `$86`/`$8E`/`$90`: the stage-clear scroll after the core dies with the
/// `$39` gate set (`$9928`). The `$39 = 0` path keeps the 514-frame
/// [`Game::boss_clear`] countdown, which already matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Clear {
    /// No stage-clear scroll is running.
    None,
    /// `$86`: wait for `$3F` to reach `$98F0[$19]`.
    WaitEnd,
    /// `$8E`/`$9842`: the +4 px/frame bonus scroll until `$3F >= $11`.
    Bonus,
    /// `$90`: the +5000 was paid; `$96C4` runs on the next frame.
    Handoff,
}

/// `$1B = $81..$90`: the no-core stage end (`QUIRK_NO_CORE`, stage 6). It is
/// kept apart from [`Phase`] so `$86` can run inside normal play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Endgame {
    /// No `$81..$90` sequence.
    None,
    /// `$81`/`$82`: `$4D:$4C` countdown with the scroll held.
    Hold(u16),
    /// `$83` and `$86`'s first frame: still held, `$CD9F` not running yet.
    Bridge(u8),
    /// `$86`: normal play, wall erase until `$3F` reaches `$98F0[$19]`.
    Erase,
    /// `$90`: the frame `$96C4` advances the stage.
    Advance,
    /// `$87`: `$9865` ran on a `QUIRK_LOOP` stage. [`Game::ending_frame`]
    /// runs `$88`..`$8D` and the stage-1 restart.
    Ending,
}

impl Game {
    /// Frontend courtesy, not cartridge behaviour: restart the title's idle
    /// window, so the attract demo waits the cartridge's re-entry time again
    /// from now. The replays never call it, so the ROM's 133/130-frame timing
    /// is unchanged.
    pub fn hold_title(&mut self) {
        if self.state == crate::State::Title {
            self.title_attract_at = self
                .title_attract_at
                .max(self.title_timer.saturating_add(TITLE_ATTRACT_REENTRY));
        }
    }

    /// Current stage section (`$19`).
    pub fn section(&self) -> u8 {
        self.section
    }

    /// The catalog row for the current stage (`$19`).
    pub fn stage_info(&self) -> &'static stage_catalog::StageInfo {
        &stage_catalog::STAGES[self.stage_row()]
    }

    fn stage_row(&self) -> usize {
        usize::from(self.section).min(stage_catalog::STAGES.len() - 1)
    }

    /// `$8344[$19]`: the stage's second music theme.
    pub fn stage_music(&self) -> u8 {
        self.rom().tables.music[self.stage_row()]
    }

    /// `$8354[$19]`: the `$3F` boundary that switches `$1C` from the shared
    /// intro (`$93`) to [`Self::stage_music`].
    pub(crate) fn music_boundary(&self) -> u8 {
        self.rom().tables.music_boundary[self.stage_row()]
    }

    /// `$9A30[$19]`: `$3F` threshold that starts the boss transition.
    pub fn stage_boss_trigger(&self) -> u8 {
        self.rom().collision.threshold[self.stage_row()]
    }

    /// Start a normal run at `$19 = stage` with the catalog's stage data
    /// loaded, skipping the title and intro. This is the native counterpart of
    /// the oracle's per-frame `$26` poke used to inspect a later stage; it
    /// changes nothing about normal play.
    pub fn new_at_stage(assets: Assets, stage: u8) -> Self {
        Self::new_at_stage_seeded(assets, stage, 0xFC)
    }

    /// Move a run [`Game::new_run`] just reset to stage 1 onto
    /// [`Mods::start_stage`], with the same fresh lives, score and weapons.
    ///
    /// The caller has already run `new_run`, so this only re-roots the stage:
    /// the section, both ships' start columns and the `$9B38` boot that follows
    /// a title start (`Blank` then `Init`). The attract demo itself never calls
    /// it; a Start during the demo does, on the `$00 = 3` boot's `StageInit`.
    pub(super) fn apply_start_stage(&mut self) {
        let section = self.mods.start_stage.clamp(1, stage::SECTION_COUNT as u8) - 1;
        if section == 0 {
            return;
        }
        self.enter_stage(section);
        self.scroll = Scroll::default();
        self.place_player(stage::ship_start(self.rom(), section, 0));
        self.place_second_ship();
        // Both slots respawn from the chosen phase, column 0.
        let rom = self.rom();
        for slot in &mut self.two.slots {
            slot.checkpoint = crate::twoplayer::Checkpoint::new(rom, section, 0, 0);
        }
    }

    /// `$8225`: Start during the attract locks `$03 = $824C[$0F]` before the
    /// `$00 = 3` countdown, while the demo itself always ran as 1P.
    pub(super) fn lock_attract_mode(&mut self) {
        let mode = if self.menu_selection == 0 {
            Players::One
        } else {
            Players::Two
        };
        if self.two.mode != mode {
            self.players = mode;
            self.two.start(mode);
        }
    }

    /// [`Game::new_at_stage`] with the `$02` counter seeded from the cartridge
    /// at the anchor. `nmi_seed` is `$02` on the anchor frame less one, because
    /// [`Game::step`] increments before the frame body; `new_at_stage` keeps the
    /// power-on `0xFC` so a run that boots at frame zero is unchanged.
    pub fn new_at_stage_seeded(assets: Assets, stage: u8, nmi_seed: u8) -> Self {
        let mut game = Self::new(assets);
        game.nmi_frame = nmi_seed;
        game.state = State::Playing;
        game.new_run();
        game.scroll = Scroll::default();
        game.enter_stage(stage);
        game.place_player(stage::ship_start(game.rom(), game.section, 0));
        game.phase = Phase::Normal;
        if stage == 3 || stage == 4 || stage == 5 || stage == 6 {
            game.stage_four_boot();
        }
        game.render();
        game
    }

    /// Seed a run at the measured ending entry (oracle 7024): the `$87`
    /// frame has just run `$9865`/`$9B38`, so the next [`Game::step`] is
    /// `$88` on 7025. The score (830), lives (3) and power byte (1) are the
    /// recorded run state the `ending-7015-8290` fixture compares against.
    ///
    /// This stands in for a state restore so the ending gate does not depend
    /// on the full stage-7 replay, which is still ~64 px behind at 5801.
    pub fn new_at_ending(assets: Assets) -> Self {
        let mut game = Self::new(assets);
        game.state = State::Playing;
        game.new_run();
        game.score_bcd = Bcd::default();
        game.score_bcd.add(830);
        game.score = 830;
        game.lives = 3;
        game.two.slots[0].score = 830;
        game.two.slots[0].lives = 3;
        game.loop_count = 1;
        game.music = ENDING_MUSIC;
        game.section = 6;
        game.scroll = Scroll::default();
        game.scroll.fraction = 0x80;
        game.power.select(1);
        game.place_player(stage::ship_start(game.rom(), 0, 0));
        game.phase = Phase::Normal;
        game.endgame = Endgame::Ending;
        game.ending_step = 0x88;
        game.ending_timer = 2;
        game.ending_screen = Some(crate::ending_screen::EndingScreen::new());
        game.frame = 7024;
        game.render();
        game
    }

    /// The three gameplay frames `$9B38` runs before the first `$3E` step of
    /// a stage-4, stage-5 or stage-6 boot: `$A2BA` and the object pass advance while
    /// `$0D` still holds the scroll. Stage 4 was measured at frames
    /// 29294-29296 and stage 5 at 294-296 (its first `$9D` is already moving
    /// when it surfaces at 302); the other stages keep their existing boot
    /// baseline.
    pub(super) fn stage_four_boot(&mut self) {
        self.scroll_hold = STAGE_BOOT_WARMUP;
        for _ in 0..STAGE_BOOT_WARMUP {
            self.normal_frame(Buttons::NONE, false);
        }
    }

    /// `$B95C`/`$9928`: after the core dies, `$39` selects the `$86`/`$8E`
    /// scroll (stage 3) instead of the 514-frame countdown. The `$39 = 0`
    /// path is the `$98F0` end the countdown already models for stages 1-2.
    pub(super) fn start_stage_clear(&mut self) {
        // `$B97B`: `INC $3B,X`, which re-arms the pause code.
        let player = self.two.current.min(1);
        self.pause_code[player] = self.pause_code[player].wrapping_add(1);
        if self.section == 2 && self.stage_clear_ready {
            self.boss_clear = 0;
            self.boss_clear_armed = false;
            self.clear = Clear::WaitEnd;
            self.bonus_wave = false;
        } else {
            self.boss_clear = 514;
            self.boss_clear_armed = false;
        }
    }

    /// `$9865`: stage-7 `$86` jumps straight to the ending entry. It bumps
    /// `$28`, clears `$3F` and `$26`, and preserves score, lives and the
    /// power byte. `$19` keeps the stage until `$9B38` copies `$26`; the
    /// ending states `$87`..`$8D` then run in [`Game::ending_frame`]; the
    /// entry frame itself is the measured 7024 `$87` state.
    pub(super) fn begin_ending(&mut self) {
        self.loop_count = self.loop_count.wrapping_add(1);
        self.scroll.high = 0;
        self.endgame = Endgame::Ending;
        // `$987F`/`$9BE7` queue `$A5` for the whole ending.
        self.music = ENDING_MUSIC;
        // The entry frame is `$87`; the next frame runs `$88`.
        self.ending_step = 0x88;
        self.ending_timer = 2;
        self.ending_clock = 0;
        self.ending_text = 0;
        self.ending_paid = false;
        self.ending_screen = Some(crate::ending_screen::EndingScreen::new());
    }

    /// `$87`..`$8D`: the ending phase machine. The entry frame (`$87`) is
    /// consumed by [`Game::begin_ending`], so the first call here is `$88`.
    ///
    /// The `$8B` build copies the `$CF90` screen one 4x4 cell per `$9D88` call
    /// in [`ending_screen`]; the text row, the +10000 award and the phase
    /// durations are the measured oracle frames.
    pub(super) fn ending_frame(&mut self) {
        match self.ending_step {
            0x88 => {
                if self.ending_timer == 2 {
                    // `$9B38`: copy `$26 = 0` into `$19`, zero `$3E/$3D`
                    // and leave the ship at zero for one frame.
                    self.section = 0;
                    self.scroll = Scroll::default();
                    self.place_player((0, 0));
                } else if self.ending_timer == 1 {
                    // `$9B38` re-places the ship from `$9BC6/$9BCE` on the
                    // second `$88` frame.
                    self.place_player(stage::ship_start(self.rom(), 0, 0));
                }
                self.ending_timer = self.ending_timer.saturating_sub(1);
                if self.ending_timer == 0 {
                    self.ending_step = 0x89;
                }
            }
            0x89 => self.ending_step = 0x8A,
            0x8A => {
                // `$987F` builds the screen in four columns a frame over 23
                // frames (7029-7051).
                self.ending_step = 0x8B;
                self.ending_timer = 23;
            }
            0x8B => {
                // `$9C1E`: four `$9D88` calls a frame. The 84 cells take 21
                // frames (7029-7049); the next frame's calls set `$57`.
                let rom = self.rom();
                if let (Some(screen), Some(source)) =
                    (self.ending_screen.as_mut(), stage_bg::background(rom, 0))
                {
                    screen.step(source);
                }
                self.ending_timer = self.ending_timer.saturating_sub(1);
                if self.ending_timer == 0 {
                    self.ending_step = 0x8C;
                    self.ending_clock = 0;
                    // `$987F` inits the two ending objects on this frame; the
                    // mothership shows at once and `$BB09` starts the walk.
                    self.ending_ship = EndingShip::new();
                    self.place_player(stage::ship_start(self.rom(), 0, 0));
                }
            }
            0x8C => {
                // `$BB09`: `$014C` reaches 6 every six frames, so the 26-entry
                // `$BB7C` walk steps at 7058 + 6k; the `$FF` terminator clears
                // `$0135` at 7214 and leaves the last position.
                if u16::from(self.ending_ship.step) < ending_ship::ENDING_SHIP_STEPS as u16
                    && self.ending_clock >= ending_ship::FIRST_STEP_CLOCK
                    && (self.ending_clock - ending_ship::FIRST_STEP_CLOCK)
                        % ending_ship::STEP_PERIOD
                        == 0
                {
                    if let Ok(motion) = self.assets.ending_ship_motion() {
                        self.ending_ship.advance(motion);
                    }
                }
                if self.ending_clock == ending_ship::HIDE_CLOCK {
                    self.ending_ship.sprite = 0;
                }
                // `$CE8E`: one character every 9 frames from 7538, and the
                // whole prefix is rewritten (the visible result is the same).
                let step = ENDING_TEXT_FIRST + ENDING_TEXT_STEP * u16::from(self.ending_text);
                if self.ending_text < ENDING_TEXT_CHARS && self.ending_clock == step {
                    self.ending_text += 1;
                }
                // `$8444`: +10000 and the score extend once, at 7998.
                if !self.ending_paid && self.ending_clock == ENDING_SCORE_FRAME {
                    self.add_score(10_000);
                    let lives = self.two.slots[self.two.current].lives.wrapping_add(1);
                    self.two.slots[self.two.current].lives = lives;
                    if self.two.current == 0 {
                        self.lives = lives;
                    }
                    self.ending_paid = true;
                }
                self.ending_clock = self.ending_clock.wrapping_add(1);
                if self.ending_clock >= ENDING_DISPLAY_FRAMES {
                    self.ending_step = 0x8D;
                }
            }
            0x8D => {
                // `$98D8`: `$1B = 0`, `JMP $9B38`; the stage-1 boot then runs
                // `$01..$04` for 27 frames (8255-8281) before play resumes.
                self.enter_stage(0);
                // `$9B38` does not restart the music: the ending's `$A5`
                // survives the boot and only `$93` starts around 8283.
                self.music = ENDING_MUSIC;
                self.scroll = Scroll::default();
                self.place_player(stage::ship_start(self.rom(), 0, 0));
                // The `$8D` frame itself is the first restart byte; the boot
                // counter is spent by `restart_boot_frame`.
                self.ending_step = 0x8D;
                self.ending_timer = 27;
            }
            _ => {}
        }
    }

    /// `$9B38`: one frame of the 27-frame stage-1 boot that `$8D` starts.
    /// `$1B` runs `$01` twice, `$02` and `$03` once, then `$04` for 23
    /// frames (measured 8255-8281).
    pub(super) fn restart_boot_frame(&mut self) {
        self.phase = Phase::Init(0);
        self.ending_step = match self.ending_timer {
            27 | 26 => 0x01,
            25 => 0x02,
            24 => 0x03,
            _ => 0x04,
        };
        self.ending_timer -= 1;
    }

    /// The boot is spent: play resumes at 8282 (`$1B = $80`) with the screen
    /// still blanked for five frames, so the scroll first steps at 8287.
    /// `$1C` keeps `$A5` on this frame and switches to `$93` one frame later.
    pub(super) fn end_restart_boot(&mut self) {
        self.phase = Phase::Normal;
        self.ending_step = 0;
        self.ending_timer = 2;
        self.scroll_hold = 5;
        self.stage_clock = 0;
    }

    /// `$CD9F`: two `$CDAD` erases per `$86` frame (44 frames, `$66` 0 -> 88)
    /// while `$66 < $58`, queuing the packed `$CE2B` cells into the live
    /// nametable overlay. The address is keyed by the scroll at issue time.
    pub(super) fn erase_wall(&mut self) {
        // `$CDAD` writes fixed physical nametable cells (`$24FA-$26DB`, table
        // 1), not "the column under this scroll pixel": the cell identity is
        // the world column rounded down to the 32-column table base. Using the
        // live `scroll/8` drifts one column as the `$86` scroll advances and
        // leaves the cell art behind.
        let world_col = (self.scroll.position() / 8) & !31;
        for _ in 0..2 {
            let Some(entry) = self
                .assets
                .stage6_wall_erase()
                .ok()
                .and_then(|table| table.get(usize::from(self.wall_erase)).copied())
            else {
                break;
            };
            self.wall_erase += 1;
            let (row, column) = crate::stage6_wall::cell(entry);
            self.cells.push(stage3_enemies::Cell {
                world_col: world_col + u16::from(column),
                row,
                tile: 0,
            });
        }
    }

    pub(super) fn advance_section(&mut self) {
        // $96C2: `INC $19`, clear `$3F`, redraw the HUD, `$1B = $80`. `$3E`
        // and `$3D` keep their values: the handoff ledger shows `$3E 0 -> 1`
        // on the frame the scroll step runs before this call.
        let next = (self.section + 1).min((stage::SECTION_COUNT - 1) as u8);
        self.scroll.high = 0;
        self.enter_stage(next);
    }

    /// Load a stage row at a section boundary: clear `$39`/`$3A` and the
    /// `$50-$70` block (boss flags, hazard index), root the spawner on the
    /// section's `$A7CA` stream and return to normal play (`$96C2`).
    pub(super) fn enter_stage(&mut self, section: u8) {
        self.section = section.min((stage::SECTION_COUNT - 1) as u8);
        // `$96C2` clears `$39/$3A` and `$50-$70`, and `$1B = $80` leaves the
        // boss sequence, so the next stage arms and clears on its own.
        self.boss_stage = 0;
        self.boss_spawned = false;
        self.boss_timer = 0;
        self.boss_clear = 0;
        self.boss_clear_armed = false;
        self.clear = Clear::None;
        self.endgame = Endgame::None;
        self.wall_erase = 0;
        self.bonus_wave = false;
        self.scroll_locked = false;
        self.scroll_stop = false;
        self.stage_clear_ready = false;
        self.volcano_kills = 0;
        self.cells.clear();
        self.collision_cells.clear();
        self.broken.fill(0);
        self.cannons = [Cannon::default(); CANNON_SLOTS];
        self.hazard_index = 0;
        self.script_counter = 0;
        self.music = SHARED_INTRO_MUSIC;
        self.spawner.restart(self.section, self.scroll.high);
        self.phase = Phase::Normal;
        self.stage_clock = 0;
    }

    /// `$9790`/`$97D0`: lose a life and restart the current section window.
    pub(super) fn finish_death(&mut self) {
        // `$97D2`: a death clears `$39`/`$3A` (but not `$5F`), so the clear
        // gate must be re-earned after a retry.
        self.stage_clear_ready = false;
        self.bonus_wave = false;
        self.clear = Clear::None;
        // `$9790`: the current slot loses a ship. `$97B8` hands the turn to
        // the other player when they are alive; a wrap to `$FF` clears that
        // slot's `$0A` bit. Only the last player out is a game over.
        if self.two.current == 0 {
            // Tests and save states write `Game::lives` directly; it is the
            // authoritative P1 counter.
            self.two.slots[0].lives = self.lives;
        }
        let game_over = self.two.lose_ship();
        self.lives = self.two.slots[0].lives;
        self.score = self.two.slots[0].score;
        if game_over {
            self.game_over_timer = 399;
            self.audio.push(Sound::GameOver);
            self.phase = Phase::Normal;
            return;
        }
        self.rebuild_from_checkpoint();
    }

    /// `$9B38`: rebuild the shared stage from the current player's checkpoint
    /// and start the same intro a respawn or a continue runs.
    pub(super) fn rebuild_from_checkpoint(&mut self) {
        // `$9B38` restarts the shared stage from the *incoming* player's
        // checkpoint (`$24/$26/$28`), not from the live world. The stage is
        // one world; the turn switch does not create a second one.
        let checkpoint = self.two.checkpoint();
        self.death_checkpoint = checkpoint.scroll;
        self.section = checkpoint.section;
        self.scroll.high = checkpoint.scroll;
        self.scroll.low = 0;
        self.scroll.fraction = 0;
        self.power.reset();
        self.reset_section_objects();
        self.cells.clear();
        self.collision_cells.clear();
        self.broken.fill(0);
        self.spawner.restart(checkpoint.section, checkpoint.scroll);
        // `$60` is still 1 from the last column boundary, so the first spawner
        // call after the restart only re-initialises the column pointer.
        self.spawner.defer_init();
        // `$9B38` clears `$0100-$017F` but not the shot cooldowns at `$03A3`,
        // so the pre-death cooldown carries into the respawn.
        let cooldown1 = self.player.cooldown1;
        let cooldown2 = self.player.cooldown2;
        // `$97D0` jumps back into `$9B38`, so the respawn runs the same stage
        // intro as the first entry. `$9B38` places the ship on this frame, so
        // the count starts one step in.
        self.place_player((checkpoint.ship_x, checkpoint.ship_y));
        self.player.cooldown1 = cooldown1;
        self.player.cooldown2 = cooldown2;
        self.phase = Phase::StageIntro(STAGE_INTRO_FRAMES - 1);
        // The incoming player's own `$05,X`/`$07,X` latch is already in
        // `$07`/`$08`; start from that pad, and redraw the HUD bank now.
        self.latched_input = if self.two.current == 0 {
            self.latched_p1
        } else {
            self.latched_p2
        };
        self.prev_fire = self.latched_input.fire;
        self.score_switch = true;
        // `$9B38`'s page upload runs again on the respawn and overruns the
        // NMI once, exactly as at stage entry.
        self.nmi_lag = true;
    }

    /// `$970C`/`$97D0`: the `$977C` code completed on the game-over expiry
    /// frame. `TwoPlayer::continue_game` restores the current player (three
    /// lives, threshold 1, a zeroed bank and checkpoint column 0) and the
    /// stage rebuilds from that checkpoint, the same path a handoff takes.
    pub(super) fn resume_from_continue(&mut self) {
        if self.simultaneous() {
            // Both ships continued: each gets the three lives, threshold 1
            // and column-0 checkpoint the ROM gives a single continue, and
            // both score banks are cleared. The stage rebuild is the shared
            // one; P2's ship is placed beside P1's at the same column.
            for slot in &mut self.two.slots {
                slot.lives = 3;
                slot.extra_life = 1;
                slot.alive = true;
                slot.score = 0;
                slot.checkpoint.scroll = 0;
            }
            self.two.current = 0;
            self.two.continue_index = 0;
            self.two.state = crate::twoplayer::TurnState::Playing;
            self.game_over_timer = 0;
            self.title_pending = false;
            self.score_bcd = Bcd::default();
            self.score = 0;
            self.lives = 3;
            self.rebuild_from_checkpoint();
            self.place_second_ship();
            return;
        }
        let current = self.two.current;
        let _ = self.two.continue_game(self.rom());
        // Game counts the `$1B = 1..4` rebuild in its phase machine, so the
        // turn machine reports normal play as soon as the code is accepted;
        // this is the convention `lose_ship` already uses for a handoff.
        self.two.state = crate::twoplayer::TurnState::Playing;
        self.game_over_timer = 0;
        self.title_pending = false;
        if current == 0 {
            // `$973E`: the continued player's score bank is cleared.
            self.score_bcd = Bcd::default();
        }
        self.lives = self.two.slots[0].lives;
        self.score = self.two.slots[0].score;
        self.rebuild_from_checkpoint();
    }
}

/// `$1B`/`$1C` gate for the stage-1 loop restart (`docs/analysis/loop.md`).
impl Game {
    /// `$1B`: the stage byte. Modelled for the ending (`$87`..`$8D`), the
    /// `$8D`-then-`$01..$04` stage-1 boot and play (`$80`); other phases
    /// report zero because no test needs them here.
    pub fn stage_byte(&self) -> u8 {
        if self.endgame == Endgame::Ending || self.ending_step != 0 {
            return self.ending_step;
        }
        if self.phase == Phase::Normal {
            0x80
        } else {
            0
        }
    }
}

/// `$9786`: Up, Up, Down, Down, Left, Right, Left, Right, B, A as `$05` bytes.
const PAUSE_CODE: [u8; 10] = [0x08, 0x08, 0x04, 0x04, 0x02, 0x01, 0x02, 0x01, 0x40, 0x80];

/// Steps between the Start edge and the `$15` flip, measured on the oracle.
pub(in crate::game) const PAUSE_DELAY: u8 = 1;

impl Game {
    /// The Start pause is on (`$15 != 0`): the stage is frozen.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// `$9AD4`-`$9B1D`: a Start edge toggles `$15` [`PAUSE_DELAY`] steps
    /// later. Only a live, non-demo stage frame may start it (`$1B` bit 7 set
    /// with bits 4-6 clear, `$09 = $0D = 0`); resuming is always allowed.
    pub(in crate::game) fn pause_tick(&mut self, edge: bool, code_edge: u8) {
        // `$8225`: the `$00 = 3` countdown keeps the mode the attract Start
        // locked; the demo itself always ran as 1P.
        if matches!(self.phase, Phase::Continue(_)) {
            self.lock_attract_mode();
        }
        let was_paused = self.paused;
        if edge && self.pause_pending == 0 && self.state == State::Playing {
            self.pause_pending = PAUSE_DELAY + 1;
        }
        if self.pause_pending > 0 {
            self.pause_pending -= 1;
            if self.pause_pending == 0 {
                if self.paused {
                    self.paused = false;
                    self.audio.push(Sound::Resume);
                } else if self.can_pause() {
                    self.paused = true;
                    self.audio.push(Sound::Pause);
                }
            }
        }
        if was_paused && self.state == State::Playing {
            self.scan_pause_code(self.pause_edges);
        }
        self.pause_edges = code_edge;
    }

    /// `$9AF9`-`$9B13`: while paused, `$9758` scans `$05` for the continue
    /// code unless the player's `$3B` is negative. A full code installs the
    /// demo loadout (`$9C58`) and decrements `$3B`.
    fn scan_pause_code(&mut self, edge: u8) {
        let player = self.two.current.min(1);
        if self.pause_code[player] >= 0x80 || edge == 0 {
            return;
        }
        // `$9758` with `$9786`: a match advances `$33`, anything else resets it.
        let index = usize::from(self.two.continue_index);
        self.two.continue_index = if PAUSE_CODE.get(index) == Some(&edge) {
            self.two.continue_index + 1
        } else {
            0
        };
        if usize::from(self.two.continue_index) == PAUSE_CODE.len() {
            self.power.demo_loadout();
            self.pause_code[player] = self.pause_code[player].wrapping_sub(1);
            self.two.continue_index = 0;
        }
    }

    fn can_pause(&self) -> bool {
        self.state == State::Playing
            && matches!(self.phase, Phase::Normal | Phase::Transition(_))
            && !self.demo
            && self.scroll_hold == 0
            && self.game_over_timer == 0
            && self.endgame != Endgame::Ending
    }

    /// `$05`/`$06`: the bits newly pressed this frame, in the ROM's order
    /// (bit 0 Right, 1 Left, 2 Down, 3 Up, 4 Start, 5 Select, 6 B, 7 A).
    /// `$9758` compares this byte with the next `$977C` entry.
    pub(in crate::game) fn button_edges(current: Buttons, previous: Buttons) -> u8 {
        let bit = |on: bool, was: bool| u8::from(on && !was);
        bit(current.right, previous.right)
            | (bit(current.left, previous.left) << 1)
            | (bit(current.down, previous.down) << 2)
            | (bit(current.up, previous.up) << 3)
            | (bit(current.start, previous.start) << 4)
            | (bit(current.select, previous.select) << 5)
            | (bit(current.power, previous.power) << 6)
            | (bit(current.fire, previous.fire) << 7)
    }

    /// The Start edge that feeds [`Game::pause_tick`]: `$9ADC` reads `$05`
    /// without indexing by `$18`, so a 2P game takes only the first pad; a 1P
    /// game's `$05` is the `$8203` merge of both, and the simultaneous mod
    /// pauses from either.
    pub(in crate::game) fn pause_edge(
        &self,
        pads: [(Buttons, Buttons); 2],
        start_pressed: bool,
        p1_start_pressed: bool,
    ) -> bool {
        let edge = |(now, before): (Buttons, Buttons)| Self::button_edges(now, before) & 0x10 != 0;
        if self.simultaneous() {
            pads.iter().any(|&pad| edge(pad))
        } else if self.players == Players::One {
            start_pressed
        } else {
            // `$05`: the raw first pad, even on P2's turn.
            p1_start_pressed
        }
    }

    /// What `prev_start` remembers: in 2P play the raw first pad's Start
    /// level, so its edge survives a save state; everywhere else the routed
    /// input's, as the menus use it.
    pub(in crate::game) fn pause_start_level(&self, input: Buttons, p1: Buttons) -> bool {
        if self.state == State::Playing && self.players == Players::Two {
            p1.start
        } else {
            input.start
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Assets {
        Assets {
            tiles: vec![[0u8; 64]; 8 * 256],
            colors: (0..64).map(|_| [0xFF, 0xFF, 0xFF]).collect(),
            rom: Some(crate::rom::testing::rom()),
            stage: Some(crate::assets::pack_stage_assets()),
        }
    }

    /// `$99B3`: the `$83` bridge requests the stage-clear cue `$AC` on the
    /// frame before the first `$CD9F` erase pair (oracle 6955, the `$86`
    /// entry), one frame ahead of the erase (6956).
    #[test]
    fn stage_six_requests_the_clear_cue_ac() {
        // `new_at_stage_seeded` pairs native frame 1 with oracle 297.
        const FRAME_OFFSET: u64 = 296;
        let mut game = Game::new_at_stage_seeded(assets(), 5, 0x23);
        game.dev_keep_alive(true);
        let mut cue_at = None;
        let mut first_beam = None;
        let mut frames = 0;
        while game.endgame != Endgame::Erase {
            game.step(Buttons::NONE);
            frames += 1;
            if game.music() == STAGE_CLEAR_CUE && cue_at.is_none() {
                cue_at = Some(game.frame);
            }
            if first_beam.is_none() && game.beams.live() > 0 {
                first_beam = Some(game.frame);
            }
            assert!(frames < 7000, "stage 6 never reached $86");
        }
        // The measured timeline: first beam 5933, `$AC` 6955, erase 6956.
        assert_eq!(first_beam.map(|frame| frame + FRAME_OFFSET), Some(5933));
        assert_eq!(cue_at.map(|frame| frame + FRAME_OFFSET), Some(6955));
        assert_eq!(
            game.frame + FRAME_OFFSET,
            6956,
            "the erase starts one frame later"
        );
        assert_eq!(game.music(), STAGE_CLEAR_CUE);
        assert_eq!(game.wall_erase, 2, "the first erase pair runs this frame");
    }
}
