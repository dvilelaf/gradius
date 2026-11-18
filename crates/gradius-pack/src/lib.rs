//! The self-contained asset pack format.
//!
//! A pack is a flat, little-endian container with a magic header and a table of
//! chunks. It contains everything the runtime needs and nothing that points
//! back at the cartridge: once `xtask extract` has produced it, the `.nes` file
//! can be deleted.
//!
//! Layout:
//!
//! ```text
//! magic       8 bytes  "GRDPAK01"
//! version     u16      currently 1
//! chunks      u16
//! per chunk:
//!   id        u32
//!   len       u32
//!   payload   len bytes
//! ```
//!
//! Chunks may appear in any order and unknown chunks must be skipped by
//! readers, which keeps the format forward compatible.

use std::path::Path;

pub const MAGIC: [u8; 8] = *b"GRDPAK01";
pub const VERSION: u16 = 1;

pub const CHUNK_TILES: u32 = 1;
pub const CHUNK_PALETTES: u32 = 2;
pub const CHUNK_META: u32 = 3;
/// The versioned audio chunk: captured register timelines, plus the version 2
/// driver section (request table, period table and the sequence bytes the
/// native Konami driver reads).
///
/// Layout is defined in `gradius_audio::chunk` and documented in
/// `docs/AUDIO.md`. `xtask extract` fills it; readers that do not know a track
/// skip it by its length, and an older chunk or an empty driver section keeps
/// the timeline fallback.
pub const CHUNK_AUDIO: u32 = 4;
/// Reserved for stage scripts extracted from PRG.
pub const CHUNK_STAGE: u32 = 5;
/// Data extracted from the cartridge for the core: collision maps,
/// backgrounds, sprites, spawn scripts and tables.
///
/// Layout is defined in `gradius_core::rom`.
pub const CHUNK_ROM: u32 = 6;

/// Tiles are stored per 4 KiB pattern-table half, not per 8 KiB CNROM bank:
/// each 8 KiB bank holds a `$0000-$0FFF` and a `$1000-$1FFF` table and the
/// game uses both. A `TileRef` bank number is therefore
/// `hardware_bank * 2 + pattern_table`.
pub const TILE_BANKS: usize = 8;
pub const TILES_PER_BANK: usize = 256;
pub const TILE_BYTES: usize = 64;

/// The container is malformed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackFormatError {
    Magic,
    Version(u16),
    Truncated,
}

impl std::fmt::Display for PackFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Magic => write!(f, "not a Gradius pack (bad magic)"),
            Self::Version(version) => write!(f, "unsupported pack version {version}"),
            Self::Truncated => write!(f, "truncated pack"),
        }
    }
}

impl std::error::Error for PackFormatError {}

/// Split a pack into `(id, payload)` pairs, validating magic, version and
/// lengths.
pub fn read_chunks(data: &[u8]) -> Result<Vec<(u32, &[u8])>, PackFormatError> {
    if data.len() < 12 || data[..8] != MAGIC {
        return Err(PackFormatError::Magic);
    }
    let version = u16::from_le_bytes([data[8], data[9]]);
    if version != VERSION {
        return Err(PackFormatError::Version(version));
    }
    let count = usize::from(u16::from_le_bytes([data[10], data[11]]));
    let word = |offset: usize| {
        data.get(offset..offset + 4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .ok_or(PackFormatError::Truncated)
    };
    let mut chunks = Vec::with_capacity(count);
    let mut offset = 12;
    for _ in 0..count {
        let id = word(offset)?;
        let length = word(offset + 4)? as usize;
        let start = offset + 8;
        let end = start
            .checked_add(length)
            .ok_or(PackFormatError::Truncated)?;
        chunks.push((id, data.get(start..end).ok_or(PackFormatError::Truncated)?));
        offset = end;
    }
    Ok(chunks)
}

/// Reading one chunk from a pack file failed.
#[derive(Debug)]
pub enum PackFileError {
    /// The file is missing or cannot be read.
    Read(std::io::Error),
    /// The file is not a valid pack.
    Format(PackFormatError),
    /// The pack has no chunk with the requested id.
    MissingChunk(u32),
}

impl std::fmt::Display for PackFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "cannot read pack: {error}"),
            Self::Format(error) => write!(f, "{error}"),
            Self::MissingChunk(id) => write!(f, "pack has no chunk {id}"),
        }
    }
}

impl std::error::Error for PackFileError {}

/// Read the payload of chunk `id` from the pack file at `path`.
pub fn read_chunk_from_file(path: &Path, id: u32) -> Result<Vec<u8>, PackFileError> {
    let bytes = std::fs::read(path).map_err(PackFileError::Read)?;
    let chunks = read_chunks(&bytes).map_err(PackFileError::Format)?;
    chunks
        .into_iter()
        .find(|(chunk, _)| *chunk == id)
        .map(|(_, payload)| payload.to_vec())
        .ok_or(PackFileError::MissingChunk(id))
}

/// Builds a pack in memory.
#[derive(Debug, Default)]
pub struct PackBuilder {
    chunks: Vec<(u32, Vec<u8>)>,
}

impl PackBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append or replace a chunk with the same id.
    pub fn chunk(mut self, id: u32, payload: Vec<u8>) -> Self {
        self.chunks.retain(|(existing, _)| *existing != id);
        self.chunks.push((id, payload));
        self
    }

    /// Encode CHR banks as indexed 8x8 tiles.
    pub fn tiles(mut self, banks: &[Vec<[u8; TILE_BYTES]>]) -> Self {
        let mut payload = Vec::with_capacity(4 + banks.len() * TILES_PER_BANK * TILE_BYTES);
        payload.extend_from_slice(&(banks.len() as u16).to_le_bytes());
        payload.extend_from_slice(&(TILES_PER_BANK as u16).to_le_bytes());
        for bank in banks {
            for tile in bank {
                payload.extend_from_slice(tile);
            }
        }
        self = self.chunk(CHUNK_TILES, payload);
        self
    }

    /// Encode a palette list as RGB triples.
    pub fn palettes(mut self, palettes: &[[u8; 3]]) -> Self {
        let mut payload = Vec::with_capacity(1 + palettes.len() * 3);
        payload.push(u8::try_from(palettes.len()).unwrap_or(u8::MAX));
        for color in palettes.iter().take(usize::from(u8::MAX)) {
            payload.extend_from_slice(color);
        }
        self = self.chunk(CHUNK_PALETTES, payload);
        self
    }

    /// Free-form provenance metadata. Never contains cartridge bytes.
    pub fn meta(mut self, text: impl Into<String>) -> Self {
        self = self.chunk(CHUNK_META, text.into().into_bytes());
        self
    }

    pub fn finish(self) -> Vec<u8> {
        let chunks = u16::try_from(self.chunks.len()).expect("too many chunks");
        let body_len: usize = self.chunks.iter().map(|(_, data)| 8 + data.len()).sum();
        let mut out = Vec::with_capacity(12 + body_len);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&chunks.to_le_bytes());
        for (id, data) in self.chunks {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&data);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_header() {
        let bank = vec![[1u8; TILE_BYTES]; TILES_PER_BANK];
        let pack = PackBuilder::new()
            .tiles(std::slice::from_ref(&bank))
            .palettes(&[[1, 2, 3]])
            .meta("hello")
            .finish();
        assert_eq!(&pack[..8], &MAGIC);
        assert_eq!(u16::from_le_bytes([pack[8], pack[9]]), VERSION);
        assert_eq!(u16::from_le_bytes([pack[10], pack[11]]), 3);
    }

    #[test]
    fn read_chunks_returns_every_chunk_and_rejects_bad_containers() {
        let pack = PackBuilder::new()
            .chunk(CHUNK_META, b"a".to_vec())
            .chunk(CHUNK_ROM, b"bb".to_vec())
            .finish();
        assert_eq!(
            read_chunks(&pack).unwrap(),
            vec![(CHUNK_META, &b"a"[..]), (CHUNK_ROM, &b"bb"[..])]
        );
        assert_eq!(
            read_chunks(&pack[..pack.len() - 1]),
            Err(PackFormatError::Truncated)
        );
        assert_eq!(read_chunks(b"junk junk junk"), Err(PackFormatError::Magic));
        let mut bad = pack.clone();
        bad[8] = 9;
        assert_eq!(read_chunks(&bad), Err(PackFormatError::Version(9)));
    }

    #[test]
    fn read_chunk_from_file_reports_missing_file_chunk_and_returns_payload() {
        let dir = std::env::temp_dir().join(format!("gradius-pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.pak");
        let missing = read_chunk_from_file(&path, CHUNK_ROM);
        assert!(matches!(missing, Err(PackFileError::Read(_))));

        std::fs::write(
            &path,
            PackBuilder::new().chunk(CHUNK_META, b"a".to_vec()).finish(),
        )
        .unwrap();
        let absent = read_chunk_from_file(&path, CHUNK_ROM);
        assert!(matches!(
            absent,
            Err(PackFileError::MissingChunk(CHUNK_ROM))
        ));

        std::fs::write(
            &path,
            PackBuilder::new().chunk(CHUNK_ROM, b"bb".to_vec()).finish(),
        )
        .unwrap();
        assert_eq!(read_chunk_from_file(&path, CHUNK_ROM).unwrap(), b"bb");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn replacing_a_chunk_keeps_only_one() {
        let pack = PackBuilder::new()
            .chunk(CHUNK_META, b"a".to_vec())
            .chunk(CHUNK_META, b"bb".to_vec())
            .finish();
        assert_eq!(u16::from_le_bytes([pack[10], pack[11]]), 1);
    }
}
