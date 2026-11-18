//! Frame rendering: the framebuffer/nametable dump, the sprite build and the
//! HUD/OSD overlay.
//!
//! Moved verbatim from `game.rs` (card R1c). Every function keeps its name, its
//! body and its place in the frame; the only edits are the `pub(super)` markers
//! the module boundary needs.

#[path = "render/cannons.rs"]
mod cannons;
#[path = "render/cells.rs"]
mod cells;

use super::*;

impl Game {
    /// `$11`: PPUMASK shadow. `$9B75` writes `$1E`; the six `$0D` init frames
    /// blank the screen first (`$8088`).
    pub fn ppu_mask(&self) -> u8 {
        // `$9B38` keeps PPUMASK at zero through the stage init and the five
        // `$0D` blank frames; the oracle only enables the screen at f296.
        let blank = matches!(
            self.phase,
            Phase::Blank(_) | Phase::StageInit | Phase::StageIntro(_) | Phase::Init(_)
        ) || self.scroll_hold > 0
            || self.entry_blank > 0
            // `$88`..`$8B` build the ending behind a blanked screen. `$9C1E`
            // leaves `$0D = 5`, so the first four `$8C` frames (7052-7055)
            // stay blank and the first visible ending picture is 7056.
            || (self.endgame == Endgame::Ending
                && (self.ending_step < 0x8C
                    || (self.ending_step == 0x8C && self.ending_clock < 5)));
        if blank {
            0
        } else {
            tables::STAGE_PPUMASK
        }
    }
    /// `$07E4+4*$18`: the six BCD digits the HUD shows for `$18`.
    pub(super) fn current_score_digits(&self) -> [u8; 3] {
        if self.two.current == 0 {
            self.score_bcd.digits
        } else {
            Bcd::from_decimal(self.two.slots[1].score).digits
        }
    }

    /// `$20,X`: the lives digit for the player whose turn it is.
    pub(super) fn current_lives(&self) -> u8 {
        self.two.slots[self.two.current].lives.min(9)
    }

    /// `$9801`/`$980B`: the game-over banner names `$18`, the player whose
    /// last ship was lost. The ROM's text id is a hard-coded `1P` whose `31`
    /// tile `$980B` patches to `32`.
    pub(super) fn game_over_banner(&self) -> &'static str {
        if self.two.current == 0 {
            "1P GAME OVER"
        } else {
            "2P GAME OVER"
        }
    }

    pub(super) fn render(&mut self) {
        let frame = self.frame;
        let state = self.state;
        let phase = self.phase;
        let digits = self.current_score_digits();
        // `$9B38` rewrites the status row from the incoming bank, so a turn
        // switch shows its digits immediately instead of the queue delay.
        let shown_score = if self.score_switch {
            self.score_switch = false;
            let mut immediate = ScoreDelay::new(0);
            let shown = immediate.step(digits);
            self.score_delay = ScoreDelay::with_pattern(&[5, 4, 3, 8]);
            shown
        } else {
            self.score_delay.step(digits)
        };
        // `$89E8` redraws the meter text on every eighth NMI frame while the
        // ship is alive, and in the stage-entry setup while the screen is blank; the boxes
        // keep the previous sample in between.
        let meter_redraw = !hud::enabled(self.ppu_mask())
            || match phase {
                Phase::Init(_) | Phase::StageIntro(_) => true,
                Phase::Normal | Phase::Transition(_) => self.nmi_frame & 7 == 4,
                _ => false,
            };
        if meter_redraw {
            self.power.sample_empty();
        }
        self.power.record_frame(self.mods.extra_options);
        let current = self.build_sprites();
        let previous = core::mem::replace(&mut self.last_sprites, current);
        let sprites = core::mem::replace(&mut self.earlier_sprites, previous);
        let assets = &self.assets;
        let stars = &self.stars;
        let mask = self.ppu_mask();
        let clip_left = mask & 0x04 == 0;
        let menu_selection = self.menu_selection;
        // `$97F0`/`$9801`: `$1B = $C0` replaces the power-meter row with text
        // id `$1C` naming the player in `$18`.
        let game_over_window = self.game_over_timer > 0 || state == State::GameOver;
        let game_over_banner = self.game_over_banner();
        let rom = self.rom();
        let tables = &rom.tables;
        let stage_screen = stage_bg::background(rom, usize::from(self.section));
        let stage_tail = self.stage_info().tail_column;
        let ending = self.ending_screen.as_ref();
        let hud_lives = self.current_lives();
        let hud_player = self.two.current as u8;
        let second_hud = self
            .simultaneous()
            .then(|| (self.two.slots[1].lives.min(9), self.two.slots[1].score));
        let fb = &mut self.framebuffer;
        fb.clear(assets.color(BLACK));

        let showing_title = state == State::Title
            || matches!(
                phase,
                Phase::Attract(_) | Phase::DemoStart | Phase::Continue(_) | Phase::StageInit
            );
        // The title roll-in is relative to the current title cycle; the
        // attract window keeps the absolute frame clock. Tests that set
        // `frame` directly (with no title cycle yet) keep using it.
        let title_frame = if state == State::Title {
            if self.title_timer != 0 {
                i32::from(self.title_timer)
            } else {
                frame as i32
            }
        } else {
            frame as i32
        };
        if showing_title {
            // `$9B38`: the stage-entry blank window starts on the transition
            // frame, before the title/attract picture would still be visible.
            if mask == 0 && state == State::Playing {
                return;
            }
            // From f135 the Vic Viper sits beside `1 PLAYER` (70..95 x
            // 135..145). It is a sprite; the nametable has no ship. It has
            // attribute bit 5 set, so it must be drawn before the opaque
            // nametable pixels and only shows through colour 0.
            if title_frame >= 136 {
                let mut ship = SpriteList::new();
                // `$8299` places the cursor on `$82AC[$0F]`: 1P row 134, 2P
                // row 150.
                let row = 134 + i32::from(menu_selection) * 16;
                push_group_with(
                    &mut ship,
                    assets,
                    1,
                    80,
                    row,
                    &palettes::title_sprite(tables),
                    3,
                );
                fb.draw_sprites(assets, &ship, true, clip_left);
            }
            // `$8515`: the title rolls in at 2 px/frame from 264 px off screen
            // and is blank for the first 20 frames. The extracted nametable is
            // the whole 32x30 screen.
            {
                let screen = &rom.backgrounds.title;
                if state == State::Title {
                    // The sampler samples one frame after `step`, so the
                    // measured fN offset is `266 - 2*title_frame`.
                    if title_frame >= 21 {
                        let offset = (266 - 2 * title_frame).max(0);
                        draw_title_screen(fb, assets, screen, offset);
                    }
                } else {
                    draw_title_screen(fb, assets, screen, 0);
                }
            }
            // `$812F`: only the Start countdown blinks the selected row,
            // 8 frames off / 8 on; the title menu itself never blanks it.
            if let Phase::Continue(frames) = phase {
                if (frames / 8) % 2 == 0 {
                    let y = 137 + i32::from(menu_selection) * 16;
                    fb.rect(96, y, 88, 7, assets.color(BLACK));
                }
            }
            return;
        }

        // `$9B38`/`$9BBF`: PPUMASK is 0 during the six blank frames.
        if mask == 0 {
            return;
        }

        if self.endgame == Endgame::Ending {
            fb.draw_sprites(assets, &sprites, true, clip_left);
            if let Some(screen) = ending {
                screen.draw(fb, assets, &tables.palettes.ending_background);
            } else {
                starfield(fb, assets, stars);
            }
        } else if let Some(screen) = stage_screen {
            // Behind-background sprites first, then the playfield with its
            // transparent palette entry 0, so attribute bit 5 behaves like the
            // hardware.
            fb.draw_sprites(assets, &sprites, true, clip_left);
            let rows = screen.height_tiles;
            // `$9B75` writes PPU `t = $4020`: coarse Y 1, fine Y 4 = 12 px.
            // That places the map's first row 12 px above the screen.
            // The latch the PPU shows during this frame is the previous one, so
            // the scroll register lags the simulation by one pixel.
            draw_screen(
                fb,
                assets,
                screen,
                self.scroll.position() as i32 - self.scroll_lag,
                rows,
                STAGE_MAP_Y,
                stage_tail,
                &self.broken,
                &self.fresh_breaks,
                &cells::erased_mask(&self.cells, screen.width_tiles),
            );
            // The turret cells are background writes; apply them after the
            // static map so they replace the extracted (possibly captured)
            // art, and before the HUD/front sprites.
            cells::draw_cell_overlay(
                fb,
                assets,
                screen,
                self.scroll.position() as i32 - self.scroll_lag,
                &self.cells,
                rows,
                stage_tail,
            );
        } else {
            starfield(fb, assets, stars);
        }

        // The HUD rows are background tiles in the original, so their palette
        // entry 0 is opaque and hides behind-priority sprites (including the
        // power-meter bar at OAM slot 0). Fill the band before the text.
        fb.rect(
            0,
            hud::HUD_Y,
            WIDTH as i32,
            HEIGHT as i32 - hud::HUD_Y,
            assets.color(BLACK),
        );
        hud::draw_masked(
            fb,
            assets,
            &Hud::new(
                shown_score,
                self.high_score_bcd.digits,
                hud_lives,
                self.power.capsules(),
                &self.power,
            )
            .with_player(hud_player)
            .with_shown(self.power.shown_empty(), self.power.shown_selection()),
            tables,
            mask,
        );
        self.power.commit_selection();
        // Simultaneous 2P: P2's lives and score on the band's free row; the
        // ROM's single status row stays P1's.
        if let Some((lives, score)) = second_hud {
            draw_second_player_hud(fb, assets, lives, score);
        }
        // `$CE8E`: the revealed prefix of the 16-tile `$22C8` line, drawn in
        // the ending's CHR bank 3. The row is background, so it sits above the
        // sprite layer's behind bit and below the front sprites.
        if self.endgame == Endgame::Ending && self.ending_step >= 0x8C {
            draw_ending_text(fb, assets, self.ending_text);
        }

        // Front sprites (and the HUD bar, kept above the port's opaque HUD
        // band) are drawn last.
        fb.draw_sprites(assets, &sprites, false, clip_left);

        if game_over_window {
            // `$9801`: the banner overwrites the power-meter row (text id
            // `$1C` at `$2384`); `$980B` patches its `31` tile to `32` for
            // P2, so it reads "1P GAME OVER" / "2P GAME OVER".
            fb.rect(0, hud::METER_Y, WIDTH as i32, 8, assets.color(BLACK));
            font::draw_text_centered(
                fb,
                hud::METER_Y,
                game_over_banner,
                1,
                assets.color(palettes::hud(tables)),
            );
        } else if let Phase::Cleared(timer) = phase {
            // Stage clear: the same extracted playfield, dimmed at the edges
            // and with the measured white HUD colour.
            let color = assets.color(palettes::hud(assets.tables()));
            font::draw_text_centered(fb, 88, "STAGE CLEAR", 2, color);
            let blink = (timer / 8) & 1 == 0;
            if blink {
                fb.rect(16, 84, WIDTH as i32 - 32, 1, color);
                fb.rect(16, 104, WIDTH as i32 - 32, 1, color);
            }
        }
    }

    /// The mapper's current 8 KiB CHR bank. The stage's background screen
    /// names its dominant bank (stage 3 is bank 1); stages 1-5 switch to bank
    /// 2 from their tail column (416, or 384 for stage 5). In 8x16 mode a
    /// sprite's tile LSB selects the pattern table, so the per-part pack half
    /// is `hardware_bank * 2 + (tile & 1)`.
    pub(super) fn hardware_bank(&self) -> u8 {
        if let Some(column) = self.stage_info().tail_column {
            if self.scroll.position() >= (column * 8) as u16 {
                return STAGE_TAIL_BANK;
            }
        }
        stage_bg::background(self.rom(), usize::from(self.section))
            .map_or(0, |screen| screen.chr_bank)
    }

    /// Build the frame's OAM-like sprite list.
    ///
    /// Every live `$0120` sprite is expanded through its `$8ED9` group, so one
    /// object can push several 8x16 parts. Order approximates `$8B15`: HUD bar
    /// (OAM slot 0), ship, objects ascending, then the separated shot/effect
    /// pools. [`Framebuffer::draw_sprites`] applies the PPU priority rules.
    pub(super) fn build_sprites(&self) -> SpriteList {
        let mut list = SpriteList::new();
        if self.state != State::Playing
            || matches!(
                self.phase,
                Phase::Attract(_)
                    | Phase::DemoStart
                    | Phase::Continue(_)
                    | Phase::StageInit
                    | Phase::Blank(_)
            )
        {
            return list;
        }
        let assets = &self.assets;
        // The mapper's 8 KiB CHR bank also feeds the sprite table; the stage
        // tail switches to hardware bank 2, so its sprite half is pack bank 5.
        let hardware_bank = self.hardware_bank();

        // `$8F4A`: sprite 0 is the power-meter bar, x 248, stored Y 206.
        // Its attribute `$23` sets bit 5, so the HUD background hides it.
        list.push(Sprite {
            x: 248,
            y: 207,
            tile: art::HUD_BAR,
            palette: palette(assets, &palettes::hud_sprite(assets.tables())),
            flip_h: false,
            flip_v: false,
            behind: true,
        });

        // `$9B38` cleared `$0120` and `$987F` keeps it zero: during the ending
        // the OAM holds only the two `$987F` objects. `$A521` cleared the
        // object pages, so the ship, options, shots and effects are gone.
        if self.endgame == Endgame::Ending {
            self.push_ending_sprites(&mut list, assets);
            return list;
        }

        // `$8B15`: slot 0 is the ship. Its `$0120` is the 1/2/3 animation
        // group, each a 24x16 body plus the `$DE/$DF` sprite behind it.
        if !self.player.dying && self.player.sprite_frame != 0 {
            push_group(
                &mut list,
                assets,
                self.player.sprite_frame,
                i32::from(self.player.x.int),
                i32::from(self.player.y.int),
                hardware_bank,
            );
        }

        // `$8B74`: `$1B & #$70` skips the ring. `$9842`/`$99C2`/`$B998` set
        // `$1B = $90` on the last clear frame; the port models that as the
        // frame before `advance_section` (`$96C2`).
        let handoff = self.clear == Clear::Handoff
            || self.endgame == Endgame::Advance
            || self.boss_clear == 1;
        // `$8B70-$8B8B`: `$46` selects one of groups 90-93, a single ring tile
        // at ship `(+15, -1)`, cycling with `($02 >> 2) & 3`.
        if self.power.shield_hits() > 0 && !self.player.dying && !handoff {
            // `$02 >> 2 & 3`: the ring reads the NMI counter itself, so a
            // stage-anchored run lands on the cartridge's quarter.
            let group = 0x5A + ((self.nmi_frame >> 2) & 3);
            push_group(
                &mut list,
                assets,
                group,
                i32::from(self.player.x.int),
                i32::from(self.player.y.int),
                hardware_bank,
            );
        }

        // Options are `$0121`/`$0122` in the original: groups 4 and 5.
        push_options(&mut list, assets, &self.options, hardware_bank);

        // Player shots: `$8F3B` (`$6B`, palette 2) for the normal shot,
        // `$8F4A` group 7 (two `$6D` columns) for the laser, `$8F40` group 36
        // (`$73`) for the rising double shot. Missiles use `$8F58` (`$65`)
        // in free flight and `$8F53` (`$61`) while riding.
        push_shots(&mut list, assets, &self.shots, hardware_bank);

        // Simultaneous 2P: the second ship with its own hull palette, ring,
        // drones and shots. It sits after P1's layer and before the shared
        // objects, and its fire is its own pool.
        if self.simultaneous() {
            let p2 = &self.p2;
            let body = ship_palette(assets, self.mods.p2_sprite_palette);
            if !p2.player.dying && p2.player.sprite_frame != 0 {
                push_ship_group(
                    &mut list,
                    assets,
                    p2.player.sprite_frame,
                    i32::from(p2.player.x.int),
                    i32::from(p2.player.y.int),
                    hardware_bank,
                    &body,
                );
            }
            if p2.power.shield_hits() > 0 && !p2.player.dying && !handoff {
                let group = 0x5A + ((self.nmi_frame >> 2) & 3);
                push_ship_group(
                    &mut list,
                    assets,
                    group,
                    i32::from(p2.player.x.int),
                    i32::from(p2.player.y.int),
                    hardware_bank,
                    &body,
                );
            }
            push_options(&mut list, assets, &p2.options, hardware_bank);
            push_shots(&mut list, assets, &p2.shots, hardware_bank);
        }

        // Objects sit in the sprite slots after the ship, option trail and
        // shot pool; `$8B15` walks them in ascending slot order. The group id
        // lives in `$0120` (`Object::sprite`); a zero slot is not drawn.
        for object in &self.objects {
            if !object.live() || object.sprite == 0 {
                continue;
            }
            push_group(
                &mut list,
                assets,
                object.sprite,
                i32::from(object.x.int),
                i32::from(object.y.int),
                hardware_bank,
            );
        }

        // Enemy shots: `$90BA`, the `$6F/$71` pair in palette 2.
        for bullet in &self.enemy_bullets[..self.enemy_bullet_count] {
            push_group(
                &mut list,
                assets,
                bullet.sprite,
                bullet.x >> 8,
                bullet.y >> 8,
                hardware_bank,
            );
        }

        // The two measured white bursts advance one group every five frames:
        // `$9035/$903E/$9047` (`$81/$83/$85`, 15 frames) for a kill and the
        // shrinking `$9050/$9059/$9062/$9067` (`$75-$7F`, 20 frames) for a
        // ship/core death. Both effects live exactly as long as their sequence.
        for effect in &self.explosions[..self.explosion_count] {
            let age = usize::from(effect.max_life - effect.life) / 5;
            let group = if effect.max_life >= 20 {
                [0x29, 0x2A, 0x2B, 0x2C][age.min(3)]
            } else {
                [0x26, 0x27, 0x28][age.min(2)]
            };
            push_group(&mut list, assets, group, effect.x, effect.y, hardware_bank);
        }

        // `$C0C1`: the 50-frame player-death sequence, then blank.
        if let Some(burst) = &self.death_burst {
            let groups = [0x2D, 0x2E, 0x2F, 0x30, 0x30];
            if let Some(&group) = groups.get(usize::from(burst.age / 10)) {
                push_group(&mut list, assets, group, burst.x, burst.y, hardware_bank);
            }
        }

        // `$8B92`: stage 5 draws the `$0600` arms after every object slot.
        if self.section == 4 {
            cannons::push_cannon_arms(&mut list, assets, &self.cannons, hardware_bank);
        }

        // `$8B15` then walks object slots 22-31, where `$0136 = $8D` is both
        // the collision marker and the `$0120` sprite group: the stage-6
        // beams draw as one `$BD` ball at the box position.
        for beam in &self.beams.slots {
            if beam.active {
                push_group(
                    &mut list,
                    assets,
                    crate::stage6_beams::SPRITE,
                    i32::from(beam.x),
                    i32::from(beam.y),
                    hardware_bank,
                );
            }
        }

        // `$8B15` writes those entries into the rotating OAM ring; the PPU
        // then picks a different eight-per-scanline subset than insertion
        // order would. The HUD bar is the fixed slot 0.
        if self.frame > PLAYING_START {
            // The NMI builds the OAM buffer from the ring offset it bumps on
            // this same frame; the PPU shows it one frame later.
            list.apply_oam_ring(oam_ring_base(self.frame), 1);
        }
        list
    }

    /// `$987F`/`$BB09`: the ending's two objects.
    ///
    /// Slot 8 is the `$9E` mothership at `(116, 128)` until the ship's `$FF`
    /// entry starts the kind-2 explosion in the same slot, which cycles six
    /// groups of five frames (pictures 7380-7409) and then clears. Slot 9 is
    /// the kind-`$28` ship at `$0335/$0375`, its group `$96 + $BB7C` nibble,
    /// hidden before the first step and from picture 7216.
    ///
    /// The list is not put through the OAM ring: the two objects push at most
    /// 14 entries and never exceed the PPU's eight per scanline, so the
    /// insertion order is the visible order.
    fn push_ending_sprites(&self, list: &mut SpriteList, assets: &Assets) {
        // `$8A` leaves CNROM bank 3 latched for the whole ending screen, and
        // `$89E8` uploaded the default sprite palettes, not the stage-1 set.
        let bank = crate::ending_screen::CHR_BANK / 2;
        let mut push = |sprite: u8, x: i32, y: i32| {
            let palette = palettes::ending_sprite(&self.rom().tables);
            push_group_palettes(list, assets, sprite, x, y, bank, palette);
        };
        if let Some(sprite) = ending_ship::explosion_sprite(self.ending_clock) {
            push(sprite, ending_ship::MOTHERSHIP_X, ending_ship::MOTHERSHIP_Y);
        } else if self.ending_clock < ending_ship::EXPLOSION_CLOCK {
            push(
                ending_ship::MOTHERSHIP_SPRITE,
                ending_ship::MOTHERSHIP_X,
                ending_ship::MOTHERSHIP_Y,
            );
        }
        if ending_ship::ship_visible(self.ending_clock) && self.ending_ship.sprite != 0 {
            push(
                self.ending_ship.sprite,
                i32::from(self.ending_ship.x),
                i32::from(self.ending_ship.y),
            );
        }
    }
}

/// First frame both sides report `Playing` with the harness script.
const PLAYING_START: u64 = 292;

/// `$9C1E` calls already made by the `$8B` ending build at capture time.
pub(super) fn ending_replay_steps(step: u8, timer: u16) -> u8 {
    match step {
        0x8B => 23u8.saturating_sub(timer.min(23) as u8),
        0x8C..=0x8D => 23,
        _ => 0,
    }
}

/// `$8B3E`: `$2F += $44` each NMI, with a wrap to zero taking the `$04`
/// byte. The OAM ring starts at that byte offset, so it advances 17 slots a
/// frame and settles into a 15-frame cycle.
pub(super) fn oam_ring_base(frame: u64) -> u8 {
    let mut base = 0xB4u8; // `$2F` on frame 292
    let steps = frame.saturating_sub(PLAYING_START);
    let steps = if steps > 20 {
        20 + (steps - 20) % 15
    } else {
        steps
    };
    for _ in 0..steps {
        base = base.wrapping_add(0x44);
        if base == 0 {
            base = 4;
        }
    }
    base
}

/// Push one `$8ED9` sprite group at `(x, y)`. `y` is the hardware stored Y
/// byte, so the rendered top edge is one lower; each part's `dy` is applied to
/// that byte and each `dx` to the X byte. Attribute bits select the palette,
/// priority and the two flips.
fn push_group(
    list: &mut SpriteList,
    assets: &Assets,
    sprite: u8,
    x: i32,
    y: i32,
    hardware_bank: u8,
) {
    push_group_with(
        list,
        assets,
        sprite,
        x,
        y,
        &palettes::hud_sprite(assets.tables()),
        hardware_bank,
    );
}

/// `push_group` with an explicit sprite palette 3, for the title screen where
/// `$3F1D..$3F1F` differs from the stage's.
fn push_group_with(
    list: &mut SpriteList,
    assets: &Assets,
    sprite: u8,
    x: i32,
    y: i32,
    palette3: &[u8; 4],
    hardware_bank: u8,
) {
    let mut sprite_palettes = palettes::stage_sprite(assets.tables());
    sprite_palettes[3] = *palette3;
    push_group_palettes(list, assets, sprite, x, y, hardware_bank, &sprite_palettes);
}

/// The gameplay sprite palette an index names (`Mods::p2_sprite_palette`).
fn ship_palette(assets: &Assets, index: u8) -> [u8; 4] {
    palettes::stage_sprite(assets.tables())[usize::from(index & 3)]
}

/// `push_group` with an explicit hull palette, for the second simultaneous
/// ship.
///
/// The group's parts name palette 0 (body), 1 (tail) and 3 (flame); only the
/// body is substituted so the port's second ship reads as the same ship in a
/// different colour instead of a different sprite set.
fn push_ship_group(
    list: &mut SpriteList,
    assets: &Assets,
    sprite: u8,
    x: i32,
    y: i32,
    hardware_bank: u8,
    body: &[u8; 4],
) {
    let mut sprite_palettes = palettes::stage_sprite(assets.tables());
    sprite_palettes[0] = *body;
    push_group_palettes(list, assets, sprite, x, y, hardware_bank, &sprite_palettes);
}

/// `$A0C7` option drones for one ship (`$0121`/`$0122`, groups 4 and 5).
fn push_options(
    list: &mut SpriteList,
    assets: &Assets,
    options: &[OptionDrone],
    hardware_bank: u8,
) {
    for option in options {
        if !option.active {
            continue;
        }
        push_group(
            list,
            assets,
            option.sprite,
            i32::from(option.x),
            i32::from(option.y),
            hardware_bank,
        );
    }
}

/// `$8F3B` player shots for one ship: the normal shot, laser, rising double
/// shot, and the two missile frames.
fn push_shots(list: &mut SpriteList, assets: &Assets, shots: &[Shot], hardware_bank: u8) {
    for shot_instance in shots {
        if !shot_instance.active {
            continue;
        }
        let x = i32::from(shot_instance.x.int);
        let y = i32::from(shot_instance.y.int);
        match shot_instance.kind {
            1 => push_group(list, assets, 7, x, y, hardware_bank),
            2 => push_group(list, assets, 0x24, x, y, hardware_bank),
            3 => {
                let group = if shot_instance.age != 0 { 8 } else { 0x0A };
                push_group(list, assets, group, x, y, hardware_bank);
            }
            _ => push_group(list, assets, 6, x, y, hardware_bank),
        }
    }
}

/// Port-side HUD for the second simultaneous ship.
///
/// The cartridge's bottom band has one status row (`<ship> <lives> <1P/2P>
/// <score>`), which the alternating 2P reuses because only one ship is on
/// screen. Simultaneous play needs both at once, so P1 keeps the ROM row and
/// P2's lives and score go on the free pixels of the same band (y 228), which
/// `Game::render` already paints black; the six digits have no trailing zero.
fn draw_second_player_hud(fb: &mut Framebuffer, assets: &Assets, lives: u8, score: u32) {
    let color = assets.color(palettes::hud(assets.tables()));
    font::draw_text(fb, 8, 228, "2P", color);
    let digit = [b'0' + (lives % 10)];
    font::draw_text(
        fb,
        16,
        228,
        core::str::from_utf8(&digit).unwrap_or("?"),
        color,
    );
    let mut text = *b"000000";
    let mut value = score.min(999_999);
    for index in (0..6).rev() {
        text[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    font::draw_text(
        fb,
        24,
        228,
        core::str::from_utf8(&text).unwrap_or("------"),
        color,
    );
}

/// `push_group` with a whole sprite-palette set, for the ending where `$8A`
/// uploads `$89E8`'s default palettes instead of the stage-1 gameplay set.
fn push_group_palettes(
    list: &mut SpriteList,
    assets: &Assets,
    sprite: u8,
    x: i32,
    y: i32,
    hardware_bank: u8,
    sprite_palettes: &[[u8; 4]; 4],
) {
    let rom = assets
        .rom
        .expect("the pack has no ROM data; run scripts/extract-data");
    for part in crate::sprites::group(rom, sprite) {
        let attr = part.attr;
        list.push(Sprite {
            x: x + i32::from(part.dx),
            y: y + i32::from(part.dy) + 1,
            tile: crate::assets::TileRef {
                // 8x16 mode: the tile LSB selects the pattern table.
                bank: hardware_bank * 2 + (part.tile & 1),
                index: part.tile & 0xFE,
            },
            palette: palette(assets, &sprite_palettes[usize::from(attr & 3)]),
            flip_h: attr & 0x40 != 0,
            flip_v: attr & 0x80 != 0,
            behind: attr & 0x20 != 0,
        });
    }
}

pub(super) fn palette(assets: &Assets, indices: &[u8; 4]) -> [[u8; 3]; 4] {
    [
        assets.color(indices[0]),
        assets.color(indices[1]),
        assets.color(indices[2]),
        assets.color(indices[3]),
    ]
}

fn starfield(fb: &mut Framebuffer, assets: &Assets, stars: &[Star]) {
    for layer in 0..3u8 {
        let rgb = assets.color(if layer == 2 { STAR_BRIGHT } else { STAR_COLOR });
        for star in stars.iter().filter(|star| star.layer == layer) {
            fb.pixel(star.x >> 8, i32::from(star.y), rgb);
        }
    }
}

/// Draw the revealed prefix of the `$CF35` text line at `$22C8`.
///
/// Every character is one 8x8 tile in CHR bank 3 (pack bank 6); there is no
/// width table and the spaces are the empty tile `$00`. The record's first
/// two bytes are the PPU address, the next sixteen are the tiles.
fn draw_ending_text(fb: &mut Framebuffer, assets: &Assets, revealed: u8) {
    let Ok(record) = assets.ending_text_record() else {
        return;
    };
    let shown = revealed.min(ENDING_TEXT_CHARS);
    let mut palette = [[0u8; 3]; 4];
    for (slot, index) in palette.iter_mut().zip(ENDING_TEXT_PALETTE) {
        *slot = assets.color(index);
    }
    for index in 0..shown {
        let tile = record[2 + usize::from(index)];
        let x = ENDING_TEXT_X + i32::from(index) * 8;
        fb.tile(
            assets,
            ENDING_TEXT_BANK,
            tile,
            x,
            ENDING_TEXT_Y,
            false,
            false,
            &palette,
            true,
        );
    }
}

/// Draw the title nametable shifted left by `offset` pixels without wrapping.
///
/// `$8515` scrolls the 256-px title in from x = +264 to 0; the same 2-px/frame
/// motion is a pure horizontal translation, not a nametable wrap.
pub(super) fn draw_title_screen(
    fb: &mut Framebuffer,
    assets: &Assets,
    screen: &stage_bg::Screen,
    offset: i32,
) {
    if offset >= WIDTH as i32 || screen.tiles.is_empty() {
        return;
    }
    let attribute_columns = (screen.width_tiles / 4).max(1);
    let bank = screen.chr_bank.saturating_mul(2);
    for row in 0..screen.height_tiles {
        for column in 0..screen.width_tiles {
            let x = column as i32 * 8 - offset;
            if x <= -8 || x >= WIDTH as i32 {
                continue;
            }
            let tile_index = row * screen.width_tiles + column;
            let Some(tile) = screen.tiles.get(tile_index).copied() else {
                continue;
            };
            let block = (row / 4) * attribute_columns + column / 4;
            let attribute = screen.attributes.get(block).copied().unwrap_or(0);
            let quadrant = ((row & 2) >> 1) * 2 + ((column & 2) >> 1);
            let palette_index = usize::from((attribute >> (quadrant * 2)) & 3);
            let mut rgb = [[0u8; 3]; 4];
            for (entry, slot) in rgb.iter_mut().enumerate() {
                let index = screen.palette[palette_index * 4 + entry];
                *slot = assets.color(index);
            }
            fb.tile(
                assets,
                bank,
                tile,
                x,
                row as i32 * 8,
                false,
                false,
                &rgb,
                true,
            );
        }
    }
}

/// The CHR bank and four palette indices for one tile column.
///
/// A `$834B` tail column (`tailed`) switches to hardware CHR bank 2 and
/// forces palette RAM entries 8-15 to `$0F`; palettes 0-1 keep the extracted
/// screen's colours.
pub(super) fn tile_style(
    screen: &stage_bg::Screen,
    palette_index: usize,
    tailed: bool,
) -> (u8, [u8; 4]) {
    let bank = if tailed {
        STAGE_TAIL_BANK.saturating_mul(2)
    } else {
        screen.chr_bank.saturating_mul(2)
    };
    let mut entries = [0u8; 4];
    for (entry, slot) in entries.iter_mut().enumerate() {
        let absolute = palette_index * 4 + entry;
        *slot = if tailed && absolute >= 8 {
            BLACK
        } else {
            screen.palette[absolute]
        };
    }
    (bank, entries)
}

/// Draw an extracted nametable, scrolled horizontally by `scroll_px`.
///
/// `rows` is the number of 8-pixel tile rows to draw: the stage passes its
/// stored height, the title its full screen.
/// Tiles are fetched from the background pattern table of `screen.chr_bank`
/// (`hardware_bank * 2` in the pack's per-half numbering) and coloured by the
/// stored attribute table, so the extracted per-block palettes are exact.
/// `tail` is the tail's first world column, if the stage has one. `broken`
/// holds one bit per row for each world column of destroyed cells; the `fresh`
/// ones were broken this frame and are still drawn (the NMI blanks them next).
/// `erased` holds the `$CD9F` tile 0 cells: their nametable cell is blank.
#[allow(clippy::too_many_arguments)]
fn draw_screen(
    fb: &mut Framebuffer,
    assets: &Assets,
    screen: &stage_bg::Screen,
    scroll_px: i32,
    rows: usize,
    y_offset: i32,
    tail: Option<usize>,
    broken: &[u32],
    fresh: &[(u16, u8)],
    erased: &[u32],
) {
    if screen.width_tiles == 0 || screen.height_tiles == 0 || screen.tiles.is_empty() {
        return;
    }
    let rows = rows.min(screen.height_tiles);
    // One extra column covers the 1..7 px overhang on the right; tile
    // colour 0 stays transparent so behind-background sprites show through.
    let columns = WIDTH / 8 + 1;
    let base = (scroll_px.max(0) / 8) as usize;
    let fine = scroll_px.max(0) % 8;
    for row in 0..rows {
        for column in 0..columns {
            let tile_x = (base + column) % screen.width_tiles;
            // `$C329` blanked this cell: the starfield shows through.
            if broken.get(tile_x).is_some_and(|mask| mask >> row & 1 != 0)
                && !fresh.contains(&(tile_x as u16, row as u8))
            {
                continue;
            }
            // `$CDAD` erased this cell: blank, so behind sprites show.
            if erased.get(tile_x).is_some_and(|mask| mask >> row & 1 != 0) {
                continue;
            }
            draw_screen_tile(
                fb,
                assets,
                screen,
                tile_x,
                row,
                column as i32 * 8 - fine,
                row as i32 * 8 + y_offset,
                tail,
            );
        }
    }
}

/// Draw one static-map tile at an absolute framebuffer position.
#[allow(clippy::too_many_arguments)]
fn draw_screen_tile(
    fb: &mut Framebuffer,
    assets: &Assets,
    screen: &stage_bg::Screen,
    tile_x: usize,
    row: usize,
    x: i32,
    y: i32,
    tail: Option<usize>,
) {
    let tile_index = row * screen.width_tiles + tile_x;
    let Some(tile) = screen.tiles.get(tile_index).copied() else {
        return;
    };
    draw_tile(fb, assets, screen, tile_x, row, tile, x, y, tail);
}

/// Draw one tile byte with the extracted screen's attribute/palette.
#[allow(clippy::too_many_arguments)]
fn draw_tile(
    fb: &mut Framebuffer,
    assets: &Assets,
    screen: &stage_bg::Screen,
    tile_x: usize,
    row: usize,
    tile: u8,
    x: i32,
    y: i32,
    tail: Option<usize>,
) {
    let attribute_columns = screen.width_tiles / 4;
    let block = (row / 4) * attribute_columns + tile_x / 4;
    let attribute = screen.attributes.get(block).copied().unwrap_or(0);
    let quadrant = ((row & 2) >> 1) * 2 + ((tile_x & 2) >> 1);
    let palette_index = usize::from((attribute >> (quadrant * 2)) & 3);
    let tailed = tail.is_some_and(|column| tile_x >= column);
    let (bank, entries) = tile_style(screen, palette_index, tailed);
    let mut rgb = [[0u8; 3]; 4];
    for (slot, index) in rgb.iter_mut().zip(entries) {
        *slot = assets.color(index);
    }
    fb.tile(assets, bank, tile, x, y, false, false, &rgb, true);
}
