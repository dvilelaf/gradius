//! Two-player alternating play, exactly as measured in
//! `docs/analysis/twoplayer.md`.
//!
//! One ship exists at a time. [`TwoPlayer::current`] is `$18`; every
//! per-player value lives in [`PlayerSlot`] and is indexed by it. The stage
//! (scroll, powerups, objects) is shared, so this module only owns turns,
//! lives, scores, checkpoints and the game-over countdown; a caller applies
//! [`TwoPlayer::checkpoint`] to its stage when a turn starts.
//!
//! | item | original | here |
//! | --- | --- | --- |
//! | current player | `$18` | [`TwoPlayer::current`] |
//! | alive bits | `$0A` bits 0/1 | [`PlayerSlot::alive`], [`TwoPlayer::alive_mask`] |
//! | lives | `$20`/`$21` | [`PlayerSlot::lives`] |
//! | 1UP threshold | `$2A`/`$2B` | [`PlayerSlot::extra_life`] |
//! | score | `$07E4-$07E6`/`$07E8-$07EA` | [`PlayerSlot::score`] |
//! | checkpoint | `$24/$26/$28` | [`PlayerSlot::checkpoint`] |
//! | death timer | `$C1DA`: `$4C = 120` | [`DEATH_FRAMES`] |
//! | game-over timer | `$C0` case: `$4C = 120` | [`GAME_OVER_FRAMES`] |
//! | continue code index | `$33` | [`TwoPlayer::continue_index`] |
//! | continue code | `$977C` | [`CONTINUE_CODE`] |
//! | stage rebuild | `$9B38`/`$9C36`: `$1B` 1..4 then `$80` | [`REBUILD_FRAMES`] |
//!
//! The score is kept as its decimal value; on hardware it is six BCD digits
//! at the addresses above. Integer only, no I/O and no global state.
//!
//! The whole turn machine travels in the `PLAYERS` save chunk, encoded by
//! [`TwoPlayer::write_state`] and decoded by [`TwoPlayer::read_state`].

use crate::save::SaveError;
use crate::snapshot::{Reader, Writer};
use crate::stage::ship_start;

/// `$C0C1`: the death animation lasts 120 frames before `$9790` runs.
pub const DEATH_FRAMES: u16 = 120;
/// `$97F4`: the game-over countdown is 120 frames (the sound-engine gate
/// `$B0` is not modelled).
pub const GAME_OVER_FRAMES: u16 = 120;
/// `$9B38`..`$9C36`: the stage rebuild between turns. Measured as P1 handing
/// over at frame 1194 (`$1B = 1`) and P2 reaching normal play (`$1B = $80`)
/// at 1221, i.e. 27 frames (the port's 28-frame stage intro minus the
/// handoff frame itself).
pub const REBUILD_FRAMES: u16 = 27;

/// `$977C`: the ten-press continue code, Down, Up, B, A, B, A, B, A, B, A in
/// `$05` bit order (bit 2 Down, bit 3 Up, bit 6 B, bit 7 A).
pub const CONTINUE_CODE: [u8; 10] = [0x04, 0x08, 0x40, 0x80, 0x40, 0x80, 0x40, 0x80, 0x40, 0x80];

/// `$0F`: which menu entry was locked, `$40`/`$70` in ROM terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Players {
    /// 1P game: only slot 0 is alive and both pads drive the ship.
    One,
    /// 2P game: both slots participate and alternate.
    Two,
}

/// One player's persistent state: everything that survives a turn switch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerSlot {
    /// `$20`/`$21`. Starts at 3 (four ships); the player is out when the
    /// counter wraps to `$FF`.
    pub lives: u8,
    /// Decimal value of the six BCD digits at `$07E4-$07E6` (P1) or
    /// `$07E8-$07EA` (P2).
    pub score: u32,
    /// `$2A`/`$2B`, high score byte at which the next 1UP is awarded.
    pub extra_life: u8,
    /// `$24`/`$26`/`$28`, saved when this player dies.
    pub checkpoint: Checkpoint,
    /// `$0A` bit 0 (P1) or bit 1 (P2).
    pub alive: bool,
}

/// `$24/$26/$28`: the position the next turn restarts from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Checkpoint {
    /// `$26`: section index `$19`.
    pub section: u8,
    /// `$24`: scroll high byte `$3F`, `min($3F & $0E, 8)`.
    pub scroll: u8,
    /// `$28`: spawner sub-index `$1A`.
    pub sub: u8,
    /// `$9BC6/$9BCE`: ship X from [`ship_start`].
    pub ship_x: u8,
    /// Ship Y from [`ship_start`].
    pub ship_y: u8,
}

impl Checkpoint {
    /// Section 0, scroll 0: the start of the stage (both slots start here).
    pub const START: Checkpoint = Checkpoint {
        section: 0,
        scroll: 0,
        sub: 0,
        ship_x: 0x50,
        ship_y: 0x60,
    };

    /// Build a checkpoint and derive the ship position the ROM's `$9BC6`
    /// table places at that section and scroll window.
    pub fn new(rom: &crate::rom::RomData, section: u8, scroll: u8, sub: u8) -> Self {
        let (ship_x, ship_y) = ship_start(rom, section, scroll);
        Checkpoint {
            section,
            scroll,
            sub,
            ship_x,
            ship_y,
        }
    }
}

/// `$1B`: the turn state. Only the frame-counted parts are modelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnState {
    /// `$1B = $80`: normal play.
    Playing,
    /// `$1B = $A0`: the 120-frame death animation.
    Dying(u16),
    /// `$9B38`: blank, HUD, palette and stage rebuild for the incoming
    /// player, then normal play.
    Rebuild(u16),
    /// `$1B = $C0`: "1P/2P GAME OVER" and the 120-frame countdown.
    GameOver(u16),
    /// The countdown expired with no player alive: back to the title.
    Title,
}

/// The turn machine. `slots[0]` is P1 and `slots[1]` is P2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TwoPlayer {
    /// `$18`: the player whose ship is on screen.
    pub current: usize,
    /// `$20`/`$21`, `$2A`/`$2B`, scores and checkpoints.
    pub slots: [PlayerSlot; 2],
    /// The mode locked on the title screen.
    pub mode: Players,
    /// `$1B`.
    pub state: TurnState,
    /// `$33`: how many entries of [`CONTINUE_CODE`] matched during the
    /// game-over countdown.
    pub continue_index: u8,
}

impl Default for TwoPlayer {
    fn default() -> Self {
        let mut game = TwoPlayer {
            current: 0,
            slots: [PlayerSlot::default(); 2],
            mode: Players::One,
            state: TurnState::Title,
            continue_index: 0,
        };
        game.start(Players::One);
        game
    }
}

impl TwoPlayer {
    pub fn new() -> Self {
        TwoPlayer::default()
    }

    /// `$82CD`: a fresh game in the given mode. Both slots get three lives,
    /// a 1UP threshold of 1 and a zero score at the stage start; the first
    /// player is P1. In 1P mode P2's alive bit stays clear.
    pub fn start(&mut self, mode: Players) {
        self.mode = mode;
        self.current = 0;
        self.state = TurnState::Playing;
        // `$82FF` clears `$12-$EF`, which includes `$33`.
        self.continue_index = 0;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            *slot = PlayerSlot {
                lives: 3,
                score: 0,
                extra_life: 1,
                checkpoint: Checkpoint::START,
                alive: index == 0 || mode == Players::Two,
            };
        }
    }

    /// `$97D0` with the same mode: a brand-new game.
    pub fn restart(&mut self) {
        self.start(self.mode);
    }

    /// `$0A` bits 0/1, as a mask.
    pub fn alive_mask(&self) -> u8 {
        u8::from(self.slots[0].alive) | (u8::from(self.slots[1].alive) << 1)
    }

    /// The incoming player's checkpoint (`$24/$26/$28`); `$9B38` restores
    /// `$3F/$19/$1A` from it when a turn starts.
    pub fn checkpoint(&self) -> Checkpoint {
        self.slots[self.current].checkpoint
    }

    /// `$8931`: the HUD label is `1P`/`2P` for `$18`.
    pub fn hud_label(&self) -> &'static str {
        if self.current == 0 {
            "1P"
        } else {
            "2P"
        }
    }

    pub fn is_game_over(&self) -> bool {
        matches!(self.state, TurnState::GameOver(_))
    }

    /// A collision during normal play. The dead player's checkpoint is
    /// captured for the *current* slot (`$97A4`: `$24 = min($3F & $0E, 8)`,
    /// `$26 = $19`, `$28 = $1A`), then `$1B = $A0` starts the 120-frame
    /// animation. Ignored unless the game is in normal play.
    pub fn death(&mut self, rom: &crate::rom::RomData, section: u8, scroll: u8, sub: u8) {
        if !matches!(self.state, TurnState::Playing) {
            return;
        }
        let scroll = (scroll & 0x0E).min(8);
        self.slots[self.current].checkpoint = Checkpoint::new(rom, section, scroll, sub);
        self.state = TurnState::Dying(DEATH_FRAMES);
    }

    /// Advance one NTSC frame. Death, stage rebuild and game-over countdowns
    /// all tick here; gameplay itself is the caller's.
    pub fn tick(&mut self) {
        self.state = match self.state {
            TurnState::Dying(0) => self.finish_death(),
            TurnState::Dying(frames) => TurnState::Dying(frames - 1),
            TurnState::Rebuild(0) | TurnState::Rebuild(1) => TurnState::Playing,
            TurnState::Rebuild(frames) => TurnState::Rebuild(frames - 1),
            TurnState::GameOver(0) | TurnState::GameOver(1) => self.finish_game_over(),
            TurnState::GameOver(frames) => TurnState::GameOver(frames - 1),
            state => state,
        };
    }

    /// `$97B8`: hand control to the other player when they are alive,
    /// otherwise let the current player continue. Either way the stage
    /// rebuilds from the incoming player's checkpoint.
    pub fn next_turn(&mut self) {
        let other = 1 - self.current;
        if self.slots[other].alive {
            self.current = other;
        }
        // `$97D0` clears `$33` on every handoff.
        self.continue_index = 0;
        self.state = TurnState::Rebuild(REBUILD_FRAMES);
    }

    /// `$9758`: compare the current player's `$05,X` edge byte with the next
    /// entry of [`CONTINUE_CODE`]. A match advances `$33`; any other edge
    /// resets it to 0. Edges after the tenth are ignored.
    pub fn scan_continue(&mut self, edge: u8) {
        let Some(&expected) = CONTINUE_CODE.get(usize::from(self.continue_index)) else {
            return;
        };
        self.continue_index = if edge == expected {
            self.continue_index + 1
        } else {
            0
        };
    }

    /// `$970C`: the code was completed (`$33 == 10`) during the game-over
    /// countdown.
    pub fn continue_ready(&self) -> bool {
        self.is_game_over() && usize::from(self.continue_index) == CONTINUE_CODE.len()
    }

    /// `$970C`: the continue code completed during the game-over countdown.
    /// The current player resumes with three lives, threshold 1, a zeroed
    /// score bank and checkpoint column 0; the other player's bank and
    /// checkpoint are untouched (`$973C`, `$97D0`). Returns false outside
    /// the game-over state.
    pub fn continue_game(&mut self, rom: &crate::rom::RomData) -> bool {
        if !self.is_game_over() {
            return false;
        }
        let slot = &mut self.slots[self.current];
        slot.lives = 3;
        slot.extra_life = 1;
        slot.alive = true;
        slot.score = 0;
        slot.checkpoint.scroll = 0;
        let (ship_x, ship_y) = ship_start(rom, slot.checkpoint.section, 0);
        slot.checkpoint.ship_x = ship_x;
        slot.checkpoint.ship_y = ship_y;
        // `$97D0` clears `$33` when the continue resumes the player.
        self.continue_index = 0;
        self.state = TurnState::Rebuild(REBUILD_FRAMES);
        true
    }

    /// `$9790`/`$97B8` without the intermediate `$C0` banner: lose the
    /// current ship and hand over to the other player at once. Returns true
    /// only when the last player is out. The death animation itself is
    /// [`DEATH_FRAMES`] and is counted by the caller.
    pub fn lose_ship(&mut self) -> bool {
        let slot = &mut self.slots[self.current];
        slot.lives = slot.lives.wrapping_sub(1);
        if slot.lives == 0xFF {
            slot.alive = false;
        }
        if self.alive_mask() == 0 {
            self.state = TurnState::GameOver(GAME_OVER_FRAMES);
            return true;
        }
        let other = 1 - self.current;
        if self.slots[other].alive {
            self.current = other;
        }
        self.state = TurnState::Playing;
        false
    }

    /// `$9790`: lose a ship. The ROM lets the counter go negative; a wrap to
    /// `$FF` clears the alive bit and enters `$1B = $C0`.
    fn finish_death(&mut self) -> TurnState {
        let slot = &mut self.slots[self.current];
        slot.lives = slot.lives.wrapping_sub(1);
        if slot.lives == 0xFF {
            slot.alive = false;
            return TurnState::GameOver(GAME_OVER_FRAMES);
        }
        self.next_turn();
        self.state
    }

    /// Encoded width of [`TwoPlayer::write_state`], length prefix included.
    pub(crate) const STATE_LEN: usize = 2 + 1 + 1 + 3 + 1 + 2 * 12;

    /// Write the whole turn machine for the PLAYERS save chunk: `$18`, the
    /// locked mode, `$1B` with its countdown, `$33` and both player banks
    /// (`$20`, score, `$2A`, `$24/$26/$28`, `$0A`).
    ///
    /// The leading `u16` is the encoded length, so a reader rejects a chunk
    /// from a different layout instead of mis-parsing it.
    pub(crate) fn write_state(&self, w: &mut Writer) {
        w.u16(Self::STATE_LEN as u16);
        w.u8(self.current as u8);
        w.u8(match self.mode {
            Players::One => 0,
            Players::Two => 1,
        });
        let (tag, frames) = match self.state {
            TurnState::Playing => (0, 0),
            TurnState::Dying(frames) => (1, frames),
            TurnState::Rebuild(frames) => (2, frames),
            TurnState::GameOver(frames) => (3, frames),
            TurnState::Title => (4, 0),
        };
        w.u8(tag);
        w.u16(frames);
        w.u8(self.continue_index);
        for slot in &self.slots {
            w.u8(slot.lives);
            w.u32(slot.score);
            w.u8(slot.extra_life);
            w.u8(slot.checkpoint.section);
            w.u8(slot.checkpoint.scroll);
            w.u8(slot.checkpoint.sub);
            w.u8(slot.checkpoint.ship_x);
            w.u8(slot.checkpoint.ship_y);
            w.bool(slot.alive);
        }
    }

    /// The inverse of [`TwoPlayer::write_state`].
    pub(crate) fn read_state(r: &mut Reader<'_>) -> Result<Self, SaveError> {
        if usize::from(r.u16()?) != Self::STATE_LEN {
            return Err(SaveError::BadPayloadLength);
        }
        let current = usize::from(r.u8()?);
        if current > 1 {
            return Err(SaveError::BadPayloadLength);
        }
        let mode = match r.u8()? {
            0 => Players::One,
            1 => Players::Two,
            _ => return Err(SaveError::BadPayloadLength),
        };
        let tag = r.u8()?;
        let frames = r.u16()?;
        let state = match tag {
            0 => TurnState::Playing,
            1 => TurnState::Dying(frames),
            2 => TurnState::Rebuild(frames),
            3 => TurnState::GameOver(frames),
            4 => TurnState::Title,
            _ => return Err(SaveError::BadPayloadLength),
        };
        let mut slots = [PlayerSlot::default(); 2];
        let continue_index = r.u8()?;
        for slot in &mut slots {
            *slot = PlayerSlot {
                lives: r.u8()?,
                score: r.u32()?,
                extra_life: r.u8()?,
                checkpoint: Checkpoint {
                    section: r.u8()?,
                    scroll: r.u8()?,
                    sub: r.u8()?,
                    ship_x: r.u8()?,
                    ship_y: r.u8()?,
                },
                alive: r.bool()?,
            };
        }
        Ok(TwoPlayer {
            current,
            slots,
            mode,
            state,
            continue_index,
        })
    }

    /// `$96EE` after the countdown: the other alive player takes over,
    /// otherwise the game returns to the title.
    fn finish_game_over(&mut self) -> TurnState {
        let other = 1 - self.current;
        if self.slots[other].alive {
            self.current = other;
            // `$97B8` falls into `$97D0`, which clears `$33`.
            self.continue_index = 0;
            TurnState::Rebuild(REBUILD_FRAMES)
        } else {
            TurnState::Title
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::testing::rom;

    /// Run the frame loop until normal play, game over or the title.
    fn to_playing(game: &mut TwoPlayer) {
        while matches!(game.state, TurnState::Dying(_) | TurnState::Rebuild(_)) {
            game.tick();
        }
    }

    fn expire_game_over(game: &mut TwoPlayer) {
        assert!(game.is_game_over());
        for _ in 0..GAME_OVER_FRAMES {
            game.tick();
        }
    }

    #[test]
    fn the_stage_start_checkpoint_matches_the_ship_table() {
        assert_eq!(Checkpoint::START, Checkpoint::new(rom(), 0, 0, 0));
        assert_eq!(
            (Checkpoint::START.ship_x, Checkpoint::START.ship_y),
            (0x50, 0x60)
        );
    }

    #[test]
    fn a_two_player_start_sets_both_slots() {
        let mut game = TwoPlayer::new();
        game.start(Players::Two);

        assert_eq!(game.mode, Players::Two);
        assert_eq!(game.current, 0);
        assert_eq!(game.state, TurnState::Playing);
        assert_eq!(game.alive_mask(), 3);
        assert_eq!(game.hud_label(), "1P");
        for slot in game.slots {
            assert_eq!(slot.lives, 3);
            assert_eq!(slot.score, 0);
            assert_eq!(slot.extra_life, 1);
            assert_eq!(slot.checkpoint, Checkpoint::START);
            assert!(slot.alive);
        }
    }

    #[test]
    fn a_one_player_game_never_switches_players() {
        let mut game = TwoPlayer::new();
        game.start(Players::One);
        assert!(!game.slots[1].alive);

        for expected in [2, 1, 0] {
            game.death(rom(), 0, 0, 0);
            to_playing(&mut game);
            assert_eq!(game.current, 0);
            assert_eq!(game.slots[0].lives, expected);
            assert_eq!(game.alive_mask(), 1);
        }

        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert!(game.is_game_over());
        assert_eq!(game.current, 0);
        assert_eq!(game.alive_mask(), 0);
        // `$20` wrapped to `$FF`; P2 was never in the game.
        assert_eq!(game.slots[0].lives, 0xFF);
        assert_eq!(game.slots[1].lives, 3);
    }

    #[test]
    fn a_death_hands_the_turn_to_the_other_alive_player_at_their_checkpoint() {
        let mut game = TwoPlayer::new();
        game.start(Players::Two);
        game.slots[0].score = 332_211;
        game.slots[1].score = 665_544;

        game.death(rom(), 2, 6, 1);
        to_playing(&mut game);

        assert_eq!(game.current, 1);
        assert_eq!(game.hud_label(), "2P");
        assert_eq!(game.slots[0].lives, 2);
        assert_eq!(game.slots[0].score, 332_211);
        assert_eq!(game.slots[0].checkpoint, Checkpoint::new(rom(), 2, 6, 1));
        assert_eq!(game.slots[1].lives, 3);
        // P2 has never played: section 0, scroll 0, start of the stage.
        assert_eq!(game.checkpoint(), Checkpoint::START);

        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);

        assert_eq!(game.current, 0);
        assert_eq!(game.checkpoint(), Checkpoint::new(rom(), 2, 6, 1));
        assert_eq!(game.slots[1].lives, 2);
    }

    #[test]
    fn a_dead_player_keeps_the_turn_when_the_other_is_out() {
        let mut game = TwoPlayer::new();
        game.start(Players::Two);

        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert_eq!(game.current, 1);

        // P2's last ship: game over for P2 while P1 is still alive.
        game.slots[1].lives = 0;
        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert!(game.is_game_over());
        expire_game_over(&mut game);
        to_playing(&mut game);
        assert_eq!(game.current, 0);
        assert!(game.slots[0].alive);

        // P1 dies with the other player already out: the turn stays on P1.
        game.slots[0].lives = 1;
        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert_eq!(game.current, 0);
        assert_eq!(game.slots[0].lives, 0);
        assert!(!game.is_game_over());

        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert!(game.is_game_over());
        assert_eq!(game.alive_mask(), 0);
    }

    #[test]
    fn both_players_out_returns_to_the_title() {
        let mut game = TwoPlayer::new();
        game.start(Players::Two);
        game.slots[0].lives = 0;
        game.slots[1].lives = 0;

        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert!(game.is_game_over());
        expire_game_over(&mut game);
        to_playing(&mut game);
        assert_eq!(game.current, 1);
        assert_eq!(game.state, TurnState::Playing);

        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert!(game.is_game_over());
        expire_game_over(&mut game);
        assert_eq!(game.state, TurnState::Title);
        assert_eq!(game.alive_mask(), 0);
    }

    #[test]
    fn continue_restores_only_the_current_player_and_restart_clears_everything() {
        let mut game = TwoPlayer::new();
        game.start(Players::Two);
        game.slots[0].score = 332_211;
        game.slots[0].lives = 0;
        game.slots[1].score = 665_544;
        game.slots[1].lives = 0;

        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        expire_game_over(&mut game);
        to_playing(&mut game);
        assert_eq!(game.current, 1);

        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert!(game.is_game_over());
        assert_eq!(game.current, 1);

        assert!(game.continue_game(rom()));
        assert_eq!(game.state, TurnState::Rebuild(REBUILD_FRAMES));
        to_playing(&mut game);
        assert_eq!(game.current, 1);
        assert_eq!(game.slots[1].lives, 3);
        assert_eq!(game.slots[1].extra_life, 1);
        assert_eq!(game.slots[1].score, 0);
        assert!(game.slots[1].alive);
        assert_eq!(game.slots[1].checkpoint.scroll, 0);
        // The other player's bank, lives and checkpoint are untouched.
        assert_eq!(game.slots[0].score, 332_211);
        assert_eq!(game.slots[0].lives, 0xFF);
        assert!(!game.continue_game(rom()));

        game.restart();
        assert_eq!(game.mode, Players::Two);
        assert_eq!(game.current, 0);
        assert_eq!(game.state, TurnState::Playing);
        assert_eq!(game.alive_mask(), 3);
        assert_eq!(game.slots[0].score, 0);
        assert_eq!(game.slots[1].score, 0);
    }

    #[test]
    fn a_death_captures_the_rom_checkpoint_window() {
        let mut game = TwoPlayer::new();
        game.start(Players::One);
        game.death(rom(), 2, 0x0F, 1);
        assert_eq!(game.slots[0].checkpoint, Checkpoint::new(rom(), 2, 8, 1));
        to_playing(&mut game);
        game.death(rom(), 2, 0x05, 1);
        assert_eq!(game.slots[0].checkpoint, Checkpoint::new(rom(), 2, 0x04, 1));
    }

    #[test]
    fn the_continue_code_advances_on_matches_and_resets_on_a_mismatch() {
        let mut game = TwoPlayer::new();
        game.start(Players::One);
        game.slots[0].lives = 0;
        game.death(rom(), 0, 0, 0);
        to_playing(&mut game);
        assert!(game.is_game_over());

        for &expected in &CONTINUE_CODE[..4] {
            game.scan_continue(expected);
        }
        assert_eq!(game.continue_index, 4);
        game.scan_continue(CONTINUE_CODE[1]);
        assert_eq!(game.continue_index, 0);

        for &expected in &CONTINUE_CODE {
            game.scan_continue(expected);
        }
        assert_eq!(game.continue_index, 10);
        assert!(game.continue_ready());
        // Edges after the tenth are ignored, not a reset.
        game.scan_continue(CONTINUE_CODE[0]);
        assert!(game.continue_ready());

        assert!(game.continue_game(rom()));
        assert!(!game.continue_ready());
        assert_eq!(game.continue_index, 0);
    }

    #[test]
    fn the_state_chunk_round_trips_every_field() {
        let mut game = TwoPlayer::new();
        game.start(Players::Two);
        game.current = 1;
        game.slots[0] = PlayerSlot {
            lives: 2,
            score: 332_211,
            extra_life: 3,
            checkpoint: Checkpoint::new(rom(), 2, 6, 1),
            alive: true,
        };
        game.slots[1] = PlayerSlot {
            lives: 1,
            score: 665_544,
            extra_life: 5,
            checkpoint: Checkpoint::new(rom(), 3, 8, 2),
            alive: false,
        };
        game.state = TurnState::Dying(37);
        game.continue_index = 4;

        let mut w = Writer::new();
        game.write_state(&mut w);
        assert_eq!(w.buf.len(), TwoPlayer::STATE_LEN);

        let mut r = Reader::new(&w.buf);
        assert_eq!(TwoPlayer::read_state(&mut r).unwrap(), game);
        r.finish().unwrap();

        // A chunk from another layout is refused, not mis-parsed.
        let mut wrong = w.buf.clone();
        wrong[0] ^= 0xFF;
        assert!(TwoPlayer::read_state(&mut Reader::new(&wrong)).is_err());
    }

    /// The natural 2P run from `docs/analysis/twoplayer.md`: with Select at
    /// frame 180 and Start at 220, P1's first collision lands at frame 1074.
    /// 120 death frames later, frame 1194 is the handoff (`$18` 0 -> 1,
    /// `$20` 3 -> 2, `$1B` = 1) and frame 1221 is P2 in normal play
    /// (`$1B` = $80). Frames are completed `tick` calls, matching
    /// `crates/gradius-nes/tests/rom_p2.rs`.
    #[test]
    fn natural_handoff_matches_the_measured_frames() {
        let mut game = TwoPlayer::new();
        game.start(Players::Two);

        let mut handoff = None;
        let mut p2_playing = None;
        for frame in 0..1230u16 {
            if frame == 1074 {
                game.death(rom(), 0, 0, 0);
            }
            game.tick();
            if frame == 1194 {
                handoff = Some((game.current, game.slots[0].lives, game.state));
            }
            if frame == 1221 {
                p2_playing = Some((game.current, game.state));
            }
        }

        assert_eq!(handoff, Some((1, 2, TurnState::Rebuild(REBUILD_FRAMES))));
        assert_eq!(p2_playing, Some((1, TurnState::Playing)));
        assert_eq!(game.checkpoint(), Checkpoint::START);
        assert_eq!(game.hud_label(), "2P");
    }
}
