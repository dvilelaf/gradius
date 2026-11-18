//! Save-state container: a versioned header and TLV chunks.
//!
//! This module is a pure byte codec. It knows nothing about `Game`; it only
//! frames a header and opaque chunk payloads into a fixed-size buffer, and
//! validates an existing image before anyone decodes the payloads. The layout
//! is defined in `docs/analysis/savestates.md` §7: 56-byte header, magic
//! `GRSS`, version 1.0, little-endian fields and `id: u16, len: u16` chunks.
//!
//! The checksum is FNV-1a 64 over the whole image except the checksum field
//! itself, so a flipped version byte or header length is caught before any
//! field is trusted. Decoding never allocates and never panics: every read is
//! bounded and returns a [`SaveError`] instead.

use crate::video::{FNV_OFFSET, FNV_PRIME};

/// Magic at offset 0.
pub const MAGIC: [u8; 4] = *b"GRSS";
/// Major version written by this codec.
pub const VERSION_MAJOR: u16 = 1;
/// Minor version written by this codec.
pub const VERSION_MINOR: u16 = 0;
/// Fixed header size in bytes.
pub const HEADER_LEN: usize = 56;
/// Chunk framing size: `id: u16` plus `len: u16`.
pub const CHUNK_HEADER_LEN: usize = 4;
/// Encoded CORE payload width (version 1.0).
pub const CORE_LEN: usize = 2_308;
/// CORE payload plus its chunk framing.
pub const CORE_CHUNK_LEN: usize = CORE_LEN + CHUNK_HEADER_LEN;
/// One rewind-ring slot, or the maximum size of a trimmed save file.
pub const SLOT_BYTES: usize = 8_192;

/// Header flag bit 0: an OAM chunk is present.
pub const FLAG_OAM: u32 = 1 << 0;
/// Header flag bit 1: `asset_fingerprint` is valid.
pub const FLAG_ASSET_FINGERPRINT: u32 = 1 << 1;

/// Core simulation fields, 2,308 encoded bytes.
pub const CHUNK_CORE: u16 = 0x0001;
/// Earlier OAM list.
pub const CHUNK_OAM: u16 = 0x0002;
/// Reserved stage extension.
pub const CHUNK_STAGE_EXT: u16 = 0x0003;
/// Reserved two-player extension.
pub const CHUNK_PLAYERS_EXT: u16 = 0x0004;
/// Stage-3 `cells` vector.
pub const CHUNK_CELLS: u16 = 0x0005;
/// Stage-3 `collision_cells` vector.
pub const CHUNK_COLLISION: u16 = 0x0006;
/// Simultaneous 2P: the second ship's kit. Optional, so images without it
/// (every cartridge-parity game) still decode.
pub const CHUNK_SIMUL: u16 = 0x0008;

/// The fixed part of a save image.
///
/// `chunk_count` and `payload_len` are derived by [`encode`] from the chunk
/// list, so the values passed to it are ignored; [`Snapshot::header`] returns
/// the values actually stored in a decoded image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// Flag bits, see [`FLAG_OAM`] and [`FLAG_ASSET_FINGERPRINT`].
    pub flags: u32,
    /// Number of chunks in the payload.
    pub chunk_count: u16,
    /// Bytes after the header.
    pub payload_len: u32,
    /// Frame counter at capture, for debugging and UI.
    pub frame: u64,
    /// `Game::state_hash()` at capture; debug only.
    pub state_hash: u64,
    /// FNV-1a over the asset pack, or zero in the rewind ring.
    pub asset_fingerprint: u64,
}

impl Header {
    /// A header with the derived fields unset; [`encode`] fills them in.
    pub const fn new(flags: u32, frame: u64, state_hash: u64, asset_fingerprint: u64) -> Self {
        Self {
            flags,
            chunk_count: 0,
            payload_len: 0,
            frame,
            state_hash,
            asset_fingerprint,
        }
    }

    /// Whether the OAM chunk is flagged as present.
    pub fn oam_present(self) -> bool {
        self.flags & FLAG_OAM != 0
    }
}

/// One TLV chunk: a known id and an opaque payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    /// Chunk id, normally one of the `CHUNK_*` constants.
    pub id: u16,
    /// Payload bytes; at most `u16::MAX`.
    pub payload: &'a [u8],
}

/// Why an image could not be written or read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The input is shorter than the header or a chunk declares.
    Truncated,
    /// Bytes 0-3 are not `GRSS`.
    BadMagic,
    /// The major/minor pair is not supported.
    UnsupportedVersion { major: u16, minor: u16 },
    /// The header length field is not [`HEADER_LEN`].
    BadHeaderLength(u16),
    /// The FNV-1a 64 checksum does not match.
    Checksum,
    /// The declared payload length does not match the chunk framing.
    BadPayloadLength,
    /// A known chunk id appears more than once.
    DuplicateChunk(u16),
    /// A mandatory chunk is absent.
    MissingChunk(u16),
    /// The output buffer cannot hold the encoded image.
    BufferTooSmall,
    /// A chunk payload is wider than its `u16` length field.
    ChunkTooLong,
    /// More chunks than the `u16` count field can hold.
    TooManyChunks,
    /// The payload does not fit the header's `u32` length field.
    PayloadTooLarge,
}

/// A validated save image and its header.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot<'a> {
    header: Header,
    chunk_bytes: &'a [u8],
}

impl<'a> Snapshot<'a> {
    /// Validates and frames an image without decoding any chunk.
    ///
    /// The image may be a full 8 KiB ring slot; bytes after the declared
    /// payload are ignored. A bad image is rejected before any caller sees a
    /// chunk.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, SaveError> {
        if bytes.len() < HEADER_LEN {
            return Err(SaveError::Truncated);
        }
        if bytes[0..4] != MAGIC {
            return Err(SaveError::BadMagic);
        }
        let major = read_u16(bytes, 4);
        let minor = read_u16(bytes, 6);
        let flags = read_u32(bytes, 8);
        let header_len = read_u16(bytes, 12);
        if header_len as usize != HEADER_LEN {
            return Err(SaveError::BadHeaderLength(header_len));
        }
        let chunk_count = read_u16(bytes, 14);
        let payload_len = read_u32(bytes, 16) as usize;
        let total = HEADER_LEN
            .checked_add(payload_len)
            .ok_or(SaveError::Truncated)?;
        if bytes.len() < total {
            return Err(SaveError::Truncated);
        }
        let stored = read_u64(bytes, 44);
        if stored != checksum(&bytes[..total]) {
            return Err(SaveError::Checksum);
        }
        if major != VERSION_MAJOR || minor != VERSION_MINOR {
            return Err(SaveError::UnsupportedVersion { major, minor });
        }
        let chunk_bytes = &bytes[HEADER_LEN..total];
        validate_chunks(chunk_bytes, chunk_count, flags)?;
        Ok(Self {
            header: Header {
                flags,
                chunk_count,
                payload_len: payload_len as u32,
                frame: read_u64(bytes, 20),
                state_hash: read_u64(bytes, 28),
                asset_fingerprint: read_u64(bytes, 36),
            },
            chunk_bytes,
        })
    }

    /// The validated header.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The payload of the first chunk with this id, or `None`.
    pub fn chunk(&self, id: u16) -> Option<&'a [u8]> {
        let mut pos = 0;
        for _ in 0..self.header.chunk_count {
            let rest = self.chunk_bytes.get(pos..)?;
            if rest.len() < CHUNK_HEADER_LEN {
                return None;
            }
            let chunk_id = u16::from_le_bytes([rest[0], rest[1]]);
            let len = u16::from_le_bytes([rest[2], rest[3]]) as usize;
            let start = pos + CHUNK_HEADER_LEN;
            let end = start + len;
            let payload = self.chunk_bytes.get(start..end)?;
            if chunk_id == id {
                return Some(payload);
            }
            pos = end;
        }
        None
    }
}

/// Writes a whole image into `out` and returns its used length.
///
/// Nothing is written unless `out` is large enough. `chunk_count` and
/// `payload_len` are derived from `chunks`; the matching header fields are
/// ignored. Chunk ids are written as given, including unknown ones.
pub fn encode(out: &mut [u8], header: Header, chunks: &[Chunk<'_>]) -> Result<usize, SaveError> {
    let count = u16::try_from(chunks.len()).map_err(|_| SaveError::TooManyChunks)?;
    let mut total = HEADER_LEN;
    for chunk in chunks {
        if chunk.payload.len() > u16::MAX as usize {
            return Err(SaveError::ChunkTooLong);
        }
        total = total
            .checked_add(CHUNK_HEADER_LEN + chunk.payload.len())
            .ok_or(SaveError::PayloadTooLarge)?;
    }
    let payload_len = u32::try_from(total - HEADER_LEN).map_err(|_| SaveError::PayloadTooLarge)?;
    if out.len() < total {
        return Err(SaveError::BufferTooSmall);
    }

    out[0..4].copy_from_slice(&MAGIC);
    write_u16(out, 4, VERSION_MAJOR);
    write_u16(out, 6, VERSION_MINOR);
    write_u32(out, 8, header.flags);
    write_u16(out, 12, HEADER_LEN as u16);
    write_u16(out, 14, count);
    write_u32(out, 16, payload_len);
    write_u64(out, 20, header.frame);
    write_u64(out, 28, header.state_hash);
    write_u64(out, 36, header.asset_fingerprint);
    out[44..52].fill(0);
    out[52..56].fill(0);

    let mut pos = HEADER_LEN;
    for chunk in chunks {
        pos += encode_chunk(&mut out[pos..total], *chunk)?;
    }
    let sum = checksum(&out[..total]);
    out[44..52].copy_from_slice(&sum.to_le_bytes());
    Ok(total)
}

/// Writes one chunk header and its payload into `out`.
///
/// This frames a chunk alone, for callers that build the header separately;
/// [`encode`] returns the whole image instead.
pub fn encode_chunk(out: &mut [u8], chunk: Chunk<'_>) -> Result<usize, SaveError> {
    if chunk.payload.len() > u16::MAX as usize {
        return Err(SaveError::ChunkTooLong);
    }
    let total = CHUNK_HEADER_LEN + chunk.payload.len();
    if out.len() < total {
        return Err(SaveError::BufferTooSmall);
    }
    out[0..2].copy_from_slice(&chunk.id.to_le_bytes());
    out[2..4].copy_from_slice(&(chunk.payload.len() as u16).to_le_bytes());
    out[4..total].copy_from_slice(chunk.payload);
    Ok(total)
}

fn validate_chunks(chunk_bytes: &[u8], count: u16, flags: u32) -> Result<(), SaveError> {
    let mut pos = 0;
    let mut seen = 0u32;
    let mut core = false;
    let mut oam = false;
    for _ in 0..count {
        let rest = chunk_bytes.get(pos..).ok_or(SaveError::Truncated)?;
        if rest.len() < CHUNK_HEADER_LEN {
            return Err(SaveError::Truncated);
        }
        let id = u16::from_le_bytes([rest[0], rest[1]]);
        let len = u16::from_le_bytes([rest[2], rest[3]]) as usize;
        let start = pos + CHUNK_HEADER_LEN;
        let end = start.checked_add(len).ok_or(SaveError::Truncated)?;
        if end > chunk_bytes.len() {
            return Err(SaveError::Truncated);
        }
        if is_known(id) {
            let bit = 1u32 << (id - 1);
            if seen & bit != 0 {
                return Err(SaveError::DuplicateChunk(id));
            }
            seen |= bit;
            match id {
                CHUNK_CORE => core = true,
                CHUNK_OAM => oam = true,
                _ => {}
            }
        }
        pos = end;
    }
    if pos != chunk_bytes.len() {
        return Err(SaveError::BadPayloadLength);
    }
    if !core {
        return Err(SaveError::MissingChunk(CHUNK_CORE));
    }
    if flags & FLAG_OAM != 0 && !oam {
        return Err(SaveError::MissingChunk(CHUNK_OAM));
    }
    Ok(())
}

fn is_known(id: u16) -> bool {
    (CHUNK_CORE..=CHUNK_COLLISION).contains(&id) || id == CHUNK_SIMUL
}

/// FNV-1a 64 over everything except the checksum field (bytes 44-51).
fn checksum(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    let mut update = |part: &[u8]| {
        for &byte in part {
            hash = (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
        }
    };
    update(&bytes[..44]);
    update(&bytes[52..]);
    hash
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn read_u64(bytes: &[u8], at: usize) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(raw)
}

fn write_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal image is header + one zero-filled CORE chunk.
    const MINIMAL_LEN: usize = HEADER_LEN + CORE_CHUNK_LEN;

    fn minimal_header() -> Header {
        Header::new(0, 0xC0, 0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210)
    }

    fn minimal_image() -> ([u8; MINIMAL_LEN], usize) {
        let core = [0u8; CORE_LEN];
        let chunks = [Chunk {
            id: CHUNK_CORE,
            payload: &core,
        }];
        let mut buf = [0u8; MINIMAL_LEN];
        let len = encode(&mut buf, minimal_header(), &chunks).unwrap();
        (buf, len)
    }

    fn reseal(buf: &mut [u8], total: usize) {
        let sum = checksum(&buf[..total]);
        buf[44..52].copy_from_slice(&sum.to_le_bytes());
    }

    fn fnv(bytes: &[u8]) -> u64 {
        let mut hash = FNV_OFFSET;
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
        }
        hash
    }

    /// Pinned structural header bytes; the checksum at 44-51 is covered by the
    /// whole-image FNV below.
    const GOLDEN_PREFIX: [u8; 44] = [
        b'G', b'R', b'S', b'S', // magic
        1, 0, // major
        0, 0, // minor
        0, 0, 0, 0, // flags
        56, 0, // header length
        1, 0, // chunk count
        8, 9, 0, 0, // payload length: 2,312
        0xC0, 0, 0, 0, 0, 0, 0, 0, // frame
        0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01, // state hash
        0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, // fingerprint
    ];

    #[test]
    fn slot_size_and_core_widths_are_pinned() {
        assert_eq!(SLOT_BYTES, 8_192);
        assert_eq!(CORE_LEN, 2_308);
        assert_eq!(CORE_CHUNK_LEN, 2_312);

        let core = [0u8; CORE_LEN];
        let mut buf = [0u8; CORE_CHUNK_LEN];
        let len = encode_chunk(
            &mut buf,
            Chunk {
                id: CHUNK_CORE,
                payload: &core,
            },
        )
        .unwrap();
        assert_eq!(len, 2_312);
    }

    #[test]
    fn minimal_snapshot_has_golden_header_and_hash() {
        let (buf, len) = minimal_image();
        assert_eq!(len, MINIMAL_LEN);
        assert_eq!(&buf[..44], &GOLDEN_PREFIX);
        assert_eq!(&buf[52..56], &[0, 0, 0, 0]);
        assert_eq!(fnv(&buf[..len]), 0xB054_9A7A_657C_B463);
    }

    #[test]
    fn round_trip_preserves_header_and_chunks() {
        let core = [0x5Au8; CORE_LEN];
        let oam = [0x11u8, 0x22, 0x33, 0x44];
        let chunks = [
            Chunk {
                id: CHUNK_CORE,
                payload: &core,
            },
            Chunk {
                id: CHUNK_OAM,
                payload: &oam,
            },
        ];
        let mut buf = [0u8; 4096];
        let len = encode(&mut buf, Header::new(FLAG_OAM, 7, 9, 11), &chunks).unwrap();
        let snapshot = Snapshot::decode(&buf[..len]).unwrap();

        let header = snapshot.header();
        assert_eq!(header.flags, FLAG_OAM);
        assert!(header.oam_present());
        assert_eq!(header.chunk_count, 2);
        assert_eq!(header.payload_len as usize, CORE_CHUNK_LEN + 4 + oam.len());
        assert_eq!(header.frame, 7);
        assert_eq!(header.state_hash, 9);
        assert_eq!(header.asset_fingerprint, 11);
        assert_eq!(snapshot.chunk(CHUNK_CORE), Some(&core[..]));
        assert_eq!(snapshot.chunk(CHUNK_OAM), Some(&oam[..]));
        assert_eq!(snapshot.chunk(CHUNK_CELLS), None);
    }

    #[test]
    fn unknown_chunk_is_skipped() {
        let core = [0u8; CORE_LEN];
        let mystery = [9u8; 5];
        let chunks = [
            Chunk {
                id: CHUNK_CORE,
                payload: &core,
            },
            Chunk {
                id: 0x0009,
                payload: &mystery,
            },
        ];
        let mut buf = [0u8; 4096];
        let len = encode(&mut buf, Header::new(0, 1, 2, 3), &chunks).unwrap();
        let snapshot = Snapshot::decode(&buf[..len]).unwrap();
        assert_eq!(snapshot.header().chunk_count, 2);
        assert_eq!(snapshot.chunk(0x0009), Some(&mystery[..]));
        assert_eq!(snapshot.chunk(CHUNK_CORE), Some(&core[..]));
    }

    #[test]
    fn decode_rejects_every_truncation() {
        let (buf, len) = minimal_image();
        for cut in 0..len {
            assert_eq!(
                Snapshot::decode(&buf[..cut]).unwrap_err(),
                SaveError::Truncated,
                "cut at {cut}"
            );
        }
    }

    #[test]
    fn encode_does_not_write_past_a_short_buffer() {
        let core = [0u8; CORE_LEN];
        let chunks = [Chunk {
            id: CHUNK_CORE,
            payload: &core,
        }];
        let mut buf = [0xAAu8; MINIMAL_LEN + 16];
        for cap in 0..MINIMAL_LEN {
            let before = buf;
            assert_eq!(
                encode(&mut buf[..cap], minimal_header(), &chunks).unwrap_err(),
                SaveError::BufferTooSmall,
                "cap {cap}"
            );
            assert_eq!(buf, before, "wrote past cap {cap}");
        }
        assert_eq!(
            encode(&mut buf[..MINIMAL_LEN], minimal_header(), &chunks).unwrap(),
            MINIMAL_LEN
        );
    }

    #[test]
    fn bad_magic_is_rejected() {
        let (mut buf, len) = minimal_image();
        buf[0] = b'X';
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::BadMagic
        );
    }

    #[test]
    fn unknown_version_is_rejected_after_a_valid_checksum() {
        let (mut buf, len) = minimal_image();
        buf[4..6].copy_from_slice(&2u16.to_le_bytes());
        reseal(&mut buf, len);
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::UnsupportedVersion { major: 2, minor: 0 }
        );

        buf[4..6].copy_from_slice(&1u16.to_le_bytes());
        buf[6..8].copy_from_slice(&1u16.to_le_bytes());
        reseal(&mut buf, len);
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::UnsupportedVersion { major: 1, minor: 1 }
        );
    }

    #[test]
    fn corrupted_checksum_is_rejected() {
        let (mut buf, len) = minimal_image();
        buf[100] ^= 0xFF;
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::Checksum
        );

        let (mut buf, len) = minimal_image();
        buf[4] ^= 0x01;
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::Checksum
        );
    }

    #[test]
    fn bad_header_length_is_rejected() {
        let (mut buf, len) = minimal_image();
        buf[12..14].copy_from_slice(&55u16.to_le_bytes());
        reseal(&mut buf, len);
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::BadHeaderLength(55)
        );
    }

    #[test]
    fn payload_length_beyond_input_is_truncated() {
        let (mut buf, len) = minimal_image();
        buf[16..20].copy_from_slice(&2_313u32.to_le_bytes());
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::Truncated
        );
    }

    #[test]
    fn chunk_count_shorter_than_payload_is_rejected() {
        let (mut buf, len) = minimal_image();
        buf[14..16].copy_from_slice(&0u16.to_le_bytes());
        reseal(&mut buf, len);
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::BadPayloadLength
        );
    }

    #[test]
    fn chunk_length_beyond_payload_is_truncated() {
        let (mut buf, len) = minimal_image();
        buf[58..60].copy_from_slice(&(CORE_LEN as u16 + 1).to_le_bytes());
        reseal(&mut buf, len);
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::Truncated
        );
    }

    #[test]
    fn duplicate_chunk_is_rejected() {
        let core = [0u8; CORE_LEN];
        let chunks = [
            Chunk {
                id: CHUNK_CORE,
                payload: &core,
            },
            Chunk {
                id: CHUNK_CORE,
                payload: &core,
            },
        ];
        let mut buf = [0u8; 8192];
        let len = encode(&mut buf, minimal_header(), &chunks).unwrap();
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::DuplicateChunk(CHUNK_CORE)
        );
    }

    #[test]
    fn missing_core_chunk_is_rejected() {
        let oam = [0u8; 4];
        let chunks = [Chunk {
            id: CHUNK_OAM,
            payload: &oam,
        }];
        let mut buf = [0u8; 128];
        let len = encode(&mut buf, minimal_header(), &chunks).unwrap();
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::MissingChunk(CHUNK_CORE)
        );
    }

    #[test]
    fn flagged_oam_must_be_present() {
        let (mut buf, len) = minimal_image();
        buf[8] = 1;
        reseal(&mut buf, len);
        assert_eq!(
            Snapshot::decode(&buf[..len]).unwrap_err(),
            SaveError::MissingChunk(CHUNK_OAM)
        );
    }
}
