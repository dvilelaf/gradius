//! In-memory save state for [`crate::Game`].
//!
//! The framing lives in [`crate::save`] (`GRSS`, TLV chunks, FNV-1a 64); this
//! module owns the [`GameState`] wrapper and the little-endian field writer
//! and reader used by `Game::snapshot` / `Game::restore`.
//!
//! A [`GameState`] keeps the CORE payload as opaque bytes because most of the
//! simulation structs are private to `game`; only that module may name their
//! fields. The layout of CORE is therefore written and read there, in
//! declaration order, and pinned by the round-trip tests. `cells` and
//! `collision_cells` are variable-length and travel in their own chunks, the
//! two-frame OAM history travels in OAM, and the `TwoPlayer` turn machine
//! (both banks, `$18`, `$1B`) travels in PLAYERS.
//!
//! Compatibility: the header flags carry a magic/version tag ([`TAG`]) below
//! the codec's own major/minor pair. A reader of another tag refuses the image
//! instead of decoding a reordered CORE. PLAYERS was added after v1 without a
//! version bump because it is optional: an image without it still decodes and
//! the game falls back to the CORE P1 bank and a fresh turn machine.

use crate::save::{
    self, Chunk, Header, SaveError, Snapshot, CHUNK_CELLS, CHUNK_COLLISION, CHUNK_CORE, CHUNK_OAM,
    CHUNK_PLAYERS_EXT, FLAG_OAM, SLOT_BYTES,
};

/// `GRSS` image written by this module: magic word plus state version byte.
pub const TAG: u32 = 0x4753_0000 | ((VERSION as u32) << 8);
/// The bits of [`TAG`] that `decode` compares; bit 0 is the OAM flag.
pub const TAG_MASK: u32 = 0xFFFF_FF00;
/// CORE layout version. A breaking reorder increments it.
pub const VERSION: u16 = 1;
/// Hard caps on the variable-length chunks, from `savestates.md` §7.2.
pub const MAX_CELLS: usize = 512;
/// Hard cap on the stage-3 collision patches.
pub const MAX_COLLISION_PATCHES: usize = 256;
/// Extension chunk: private simulation state (spawner, score delay, ending
/// replay count). Optional, so images without it still decode.
pub const CHUNK_EXT: u16 = 0x0007;

/// One decoded save image: the opaque CORE payload plus the optional chunks.
///
/// `version` and `tag` are filled from the image header on decode, so callers
/// can reject a state captured by a different CORE layout before touching a
/// `Game`.
#[derive(Clone, Debug)]
pub struct GameState {
    /// CORE layout version, [`VERSION`] today.
    pub version: u16,
    /// Magic/version tag, [`TAG`] today.
    pub tag: u32,
    /// `Game::frame` at capture, echoed from the header.
    pub frame: u64,
    /// CORE chunk: the fixed-width simulation fields, in declaration order.
    pub core: Vec<u8>,
    /// OAM chunk: the two-frame-old sprite list.
    pub oam: Vec<u8>,
    /// CELLS chunk.
    pub cells: Vec<u8>,
    /// COLLISION chunk.
    pub collision: Vec<u8>,
    /// EXT chunk: state owned by helper modules, empty on older images.
    pub ext: Vec<u8>,
    /// PLAYERS chunk: the `TwoPlayer` turn machine, empty on older images.
    pub players: Vec<u8>,
    /// SIMUL chunk: the second ship of a simultaneous 2P game, empty when the
    /// mod is off (and on every older image).
    pub simul: Vec<u8>,
}

impl GameState {
    /// Frame the state into a `GRSS` image, trimmed to its used length.
    pub fn to_bytes(&self) -> Vec<u8> {
        let flags = TAG | if self.oam.is_empty() { 0 } else { FLAG_OAM };
        let header = Header::new(flags, self.frame, 0, 0);
        let mut chunks = vec![Chunk {
            id: CHUNK_CORE,
            payload: &self.core,
        }];
        if !self.oam.is_empty() {
            chunks.push(Chunk {
                id: CHUNK_OAM,
                payload: &self.oam,
            });
        }
        chunks.push(Chunk {
            id: CHUNK_CELLS,
            payload: &self.cells,
        });
        chunks.push(Chunk {
            id: CHUNK_COLLISION,
            payload: &self.collision,
        });
        if !self.ext.is_empty() {
            chunks.push(Chunk {
                id: CHUNK_EXT,
                payload: &self.ext,
            });
        }
        if !self.players.is_empty() {
            chunks.push(Chunk {
                id: CHUNK_PLAYERS_EXT,
                payload: &self.players,
            });
        }
        if !self.simul.is_empty() {
            chunks.push(Chunk {
                id: crate::save::CHUNK_SIMUL,
                payload: &self.simul,
            });
        }
        let mut out = vec![0u8; SLOT_BYTES];
        let len = save::encode(&mut out, header, &chunks).expect("snapshot exceeds SLOT_BYTES");
        out.truncate(len);
        out
    }

    /// Validate and split an image into its chunks.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let snap = Snapshot::decode(bytes)?;
        let header = *snap.header();
        if header.flags & TAG_MASK != TAG {
            return Err(SaveError::BadMagic);
        }
        let core = snap
            .chunk(CHUNK_CORE)
            .ok_or(SaveError::MissingChunk(CHUNK_CORE))?;
        let oam = snap.chunk(CHUNK_OAM).unwrap_or(&[]);
        let cells = snap.chunk(CHUNK_CELLS).unwrap_or(&[]);
        let collision = snap.chunk(CHUNK_COLLISION).unwrap_or(&[]);
        let ext = snap.chunk(CHUNK_EXT).unwrap_or(&[]);
        let players = snap.chunk(CHUNK_PLAYERS_EXT).unwrap_or(&[]);
        let simul = snap.chunk(crate::save::CHUNK_SIMUL).unwrap_or(&[]);
        Ok(Self {
            version: VERSION,
            tag: TAG,
            frame: header.frame,
            core: core.to_vec(),
            oam: oam.to_vec(),
            cells: cells.to_vec(),
            collision: collision.to_vec(),
            ext: ext.to_vec(),
            players: players.to_vec(),
            simul: simul.to_vec(),
        })
    }
}

/// Little-endian field writer. `Vec` growth is once per field, never per frame.
#[derive(Default)]
pub struct Writer {
    /// Bytes written so far.
    pub buf: Vec<u8>,
}

impl Writer {
    /// An empty writer.
    pub fn new() -> Self {
        Self::default()
    }

    /// One byte.
    pub fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    /// A boolean as one byte, 0 or 1.
    pub fn bool(&mut self, value: bool) {
        self.buf.push(u8::from(value));
    }

    /// Two bytes, little-endian.
    pub fn u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Four bytes, little-endian.
    pub fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Eight bytes, little-endian.
    pub fn u64(&mut self, value: u64) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Signed 16-bit, little-endian.
    pub fn i16(&mut self, value: i16) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Signed 32-bit, little-endian.
    pub fn i32(&mut self, value: i32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }
}

/// Bounded little-endian field reader; every short read is `Err`.
pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// A reader positioned at the start of `bytes`.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], SaveError> {
        let end = self.pos.checked_add(len).ok_or(SaveError::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(SaveError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    /// One byte.
    pub fn u8(&mut self) -> Result<u8, SaveError> {
        Ok(self.take(1)?[0])
    }

    /// A boolean byte; values other than 0 are `true`.
    pub fn bool(&mut self) -> Result<bool, SaveError> {
        Ok(self.u8()? != 0)
    }

    /// Two bytes, little-endian.
    pub fn u16(&mut self) -> Result<u16, SaveError> {
        let raw = self.take(2)?;
        Ok(u16::from_le_bytes([raw[0], raw[1]]))
    }

    /// Four bytes, little-endian.
    pub fn u32(&mut self) -> Result<u32, SaveError> {
        let raw = self.take(4)?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    /// Eight bytes, little-endian.
    pub fn u64(&mut self) -> Result<u64, SaveError> {
        let raw = self.take(8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(raw);
        Ok(u64::from_le_bytes(bytes))
    }

    /// Signed 16-bit, little-endian.
    pub fn i16(&mut self) -> Result<i16, SaveError> {
        Ok(self.u16()? as i16)
    }

    /// Signed 32-bit, little-endian.
    pub fn i32(&mut self) -> Result<i32, SaveError> {
        Ok(self.u32()? as i32)
    }

    /// `len` raw bytes.
    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8], SaveError> {
        self.take(len)
    }

    /// Bytes left before the end of the payload.
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    /// `true` when every byte was consumed.
    pub fn finish(&self) -> Result<(), SaveError> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(SaveError::BadPayloadLength)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::save::HEADER_LEN;
    use crate::snapshot::{GameState, Reader};
    use crate::twoplayer::{Players, TwoPlayer};
    use crate::{Assets, Buttons, Game};

    fn assets() -> Assets {
        Assets {
            tiles: vec![[0u8; 64]; 8 * 256],
            colors: (0..64).map(|_| [0xFF, 0xFF, 0xFF]).collect(),
            rom: Some(crate::rom::testing::rom()),
            ..Default::default()
        }
    }

    fn run(game: &mut Game, frames: usize, input: Buttons) {
        for _ in 0..frames {
            game.step(input);
        }
    }

    /// Step a restored game and the original side by side, comparing the
    /// harness dump and the full save image every frame.
    fn assert_replay_matches(game: &mut Game, image: &[u8], frames: usize, input: Buttons) {
        let mut restored = Game::new(assets());
        restored.restore(image).unwrap();
        for frame in 0..frames {
            game.step(input);
            restored.step(input);
            assert_eq!(
                restored.dump_state(),
                game.dump_state(),
                "dump diverged {} frames after the restore",
                frame + 1
            );
            assert_eq!(
                restored.snapshot(),
                game.snapshot(),
                "save image diverged {} frames after the restore",
                frame + 1
            );
        }
    }

    #[test]
    fn six_hundred_frames_after_a_restore_are_byte_identical() {
        let mut game = Game::new(assets());
        run(&mut game, 600, Buttons::NONE);
        let image = game.snapshot();
        assert_replay_matches(&mut game, &image, 600, Buttons::NONE);
    }

    #[test]
    fn a_restore_with_live_objects_stays_identical_for_six_hundred_frames() {
        let mut game = Game::new(assets());
        // Start reaches normal play and the fire/right input keeps spawning
        // enemies, shots and effects.
        let input = Buttons {
            start: true,
            fire: true,
            right: true,
            ..Buttons::NONE
        };
        run(&mut game, 700, input);
        // Frame 700 is still in the first lull; advance until an enemy is
        // live, then snapshot mid-wave.
        for _ in 0..200 {
            if !game.dump_state().ends_with("objects=") {
                break;
            }
            game.step(input);
        }
        assert!(!game.dump_state().ends_with("objects="));
        let image = game.snapshot();
        assert_replay_matches(&mut game, &image, 600, input);
    }

    #[test]
    fn a_snapshot_restores_dump_state_after_500_frames() {
        let mut game = Game::new(assets());
        run(&mut game, 500, Buttons::NONE);
        let image = game.snapshot();
        assert!(image.len() > HEADER_LEN);
        assert_eq!(image.len(), game.snapshot().len());

        let mut restored = Game::new(assets());
        restored.restore(&image).unwrap();
        assert_eq!(restored.dump_state(), game.dump_state());
    }

    #[test]
    fn in_game_fields_survive_a_round_trip() {
        let mut game = Game::new(assets());
        // Fire + right reaches normal play and spawns objects.
        let input = Buttons {
            start: true,
            fire: true,
            right: true,
            ..Buttons::NONE
        };
        run(&mut game, 700, input);
        assert_eq!(game.frame, 700);
        assert!(game.dump_state().contains("score="));
        let image = game.snapshot();

        let mut restored = Game::new(assets());
        restored.restore(&image).unwrap();
        assert_eq!(restored.state, game.state);
        assert_eq!(restored.frame, game.frame);
        assert_eq!(restored.score, game.score);
        assert_eq!(restored.lives, game.lives);
        assert_eq!(restored.scroll(), game.scroll());
        assert_eq!(restored.dump_state(), game.dump_state());
        // Every serialised byte, including `$02`, `$1B` and the object slots.
        assert_eq!(restored.snapshot(), image);
        assert!(restored.audio.is_empty());
    }

    /// The measured rewind probe: `--stage 5 --rewind-at 300` snapshots four
    /// frames into the stage, while the boot's first spawner records and the
    /// OAM ring are still live. The EXT chunk and the OAM tail are what make
    /// that early image replay exactly; CORE alone does not.
    #[test]
    fn a_snapshot_at_the_start_of_stage_five_replays_exactly() {
        let input = Buttons {
            fire: true,
            right: true,
            ..Buttons::NONE
        };
        // Stage 5 is index 4; four frames match `--stage 5 --rewind-at 300`.
        let mut game = Game::new_at_stage_seeded(assets(), 4, 0x22);
        run(&mut game, 4, input);
        let image = game.snapshot();
        let state = crate::snapshot::GameState::from_bytes(&image).unwrap();
        assert!(
            !state.ext.is_empty(),
            "EXT chunk must carry the spawner and score-delay state"
        );
        assert!(
            usize::from(state.oam[0]) > 0,
            "OAM chunk must carry the two-frame sprite history"
        );

        let mut restored = Game::new(assets());
        restored.restore(&image).unwrap();
        assert_eq!(restored.dump_state(), game.dump_state());
        for frame in 0..400 {
            game.step(input);
            restored.step(input);
            assert_eq!(
                restored.dump_state(),
                game.dump_state(),
                "dump diverged {} frames after the restore",
                frame + 1
            );
            assert_eq!(
                restored.snapshot(),
                game.snapshot(),
                "save image diverged {} frames after the restore",
                frame + 1
            );
            assert_eq!(
                restored.framebuffer().as_rgba(),
                game.framebuffer().as_rgba(),
                "framebuffer diverged {} frames after the restore",
                frame + 1
            );
        }
    }

    #[test]
    fn the_players_chunk_survives_the_container() {
        let mut two = TwoPlayer::new();
        two.start(Players::Two);
        two.current = 1;
        two.slots[0].score = 332_211;
        two.slots[1].score = 665_544;
        let mut players = crate::snapshot::Writer::new();
        two.write_state(&mut players);

        let state = GameState::from_bytes(&Game::new(assets()).snapshot()).unwrap();
        let with = GameState {
            players: players.buf.clone(),
            ..state
        };
        let decoded = GameState::from_bytes(&with.to_bytes()).unwrap();
        assert_eq!(decoded.players, players.buf);
        assert_eq!(
            TwoPlayer::read_state(&mut Reader::new(&decoded.players)).unwrap(),
            two
        );
    }

    /// A v1 image written before the PLAYERS chunk still loads: it carries
    /// only CORE's P1 mirror, so the turn machine restarts in the decoded mode
    /// and slot 0 takes those lives and that score.
    #[test]
    fn an_image_without_the_players_chunk_still_restores() {
        let input = Buttons {
            start: true,
            fire: true,
            right: true,
            ..Buttons::NONE
        };
        let mut game = Game::new(assets());
        run(&mut game, 700, input);
        let mut state = GameState::from_bytes(&game.snapshot()).unwrap();
        state.players.clear();
        let legacy = state.to_bytes();

        let mut restored = Game::new(assets());
        restored.restore(&legacy).unwrap();
        assert_eq!(restored.dump_state(), game.dump_state());
        assert_eq!(restored.lives, game.lives);
        assert_eq!(restored.score, game.score);
        assert_eq!(restored.two_player().slots[0].lives, game.lives);
        assert_eq!(restored.two_player().slots[0].score, game.score);
    }

    /// In attract the turn machine is dormant (three lives from `$82CD`)
    /// while `$20` is zero. A 1P restore must keep CORE's lives and score,
    /// not adopt the dormant bank from PLAYERS.
    #[test]
    fn a_one_player_restore_keeps_the_core_lives_and_score() {
        let mut game = Game::new(assets());
        run(&mut game, 20, Buttons::NONE);
        assert_eq!(game.lives, 0);
        assert_eq!(game.two_player().slots[0].lives, 3);
        let image = game.snapshot();

        let mut restored = Game::new(assets());
        restored.restore(&image).unwrap();
        assert_eq!(restored.lives, 0);
        assert_eq!(restored.dump_state(), game.dump_state());
        assert_eq!(restored.snapshot(), image);
    }

    /// A 2P image still takes both banks from PLAYERS, P1's CORE mirror
    /// included.
    #[test]
    fn a_two_player_restore_takes_both_banks_from_the_players_chunk() {
        let mut two = TwoPlayer::new();
        two.start(Players::Two);
        two.current = 1;
        two.slots[0].lives = 2;
        two.slots[0].score = 332_211;
        two.slots[1].lives = 1;
        two.slots[1].score = 665_544;
        let mut players = crate::snapshot::Writer::new();
        two.write_state(&mut players);

        let state = GameState::from_bytes(&Game::new(assets()).snapshot()).unwrap();
        let image = GameState {
            players: players.buf,
            ..state
        }
        .to_bytes();

        let mut restored = Game::new(assets());
        restored.restore(&image).unwrap();
        assert_eq!(restored.players(), Players::Two);
        assert_eq!(restored.lives, 2);
        assert_eq!(restored.score, 332_211);
        assert_eq!(restored.two_player().slots.map(|slot| slot.lives), [2, 1]);
        assert_eq!(
            restored.two_player().slots.map(|slot| slot.score),
            [332_211, 665_544]
        );
        assert_eq!(restored.two_player().current, 1);
    }

    #[test]
    fn a_corrupt_or_truncated_image_is_rejected() {
        let game = Game::new(assets());
        let mut image = game.snapshot();
        image[0] ^= 0xFF;
        assert!(Game::new(assets()).restore(&image).is_err());

        let image = game.snapshot();
        assert!(Game::new(assets()).restore(&image[..HEADER_LEN]).is_err());
    }
}
