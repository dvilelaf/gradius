//! Data extracted from the cartridge, carried by `CHUNK_ROM` of the pack.
//!
//! Payload: `"GROM"`, `u16` version, `u16` section count; per section a 4-byte
//! tag, a `u32` little-endian length and the payload. Each family owns the
//! encoding of its section. Unknown tags are skipped.

use std::fmt;

pub mod backgrounds;
pub mod collision;
pub mod spawns;
pub mod sprites;
pub mod tables;
#[cfg(test)]
pub mod testing;

pub use backgrounds::Backgrounds;
pub use collision::Collision;
pub use spawns::Spawns;
pub use sprites::Sprites;
pub use tables::{Palettes, Tables};

const MAGIC: [u8; 4] = *b"GROM";
const VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RomError {
    /// Not a `GROM` payload.
    Magic,
    Version(u16),
    Truncated,
    /// A section the reader needs is absent.
    Missing([u8; 4]),
}

impl fmt::Display for RomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Magic => write!(f, "not a ROM data chunk"),
            Self::Version(version) => write!(f, "unsupported ROM data version {version}"),
            Self::Truncated => write!(f, "truncated ROM data chunk"),
            Self::Missing(tag) => write!(
                f,
                "ROM data section {} is missing",
                String::from_utf8_lossy(tag)
            ),
        }
    }
}

impl std::error::Error for RomError {}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RomData {
    pub collision: Collision,
    pub backgrounds: Backgrounds,
    pub sprites: Sprites,
    pub spawns: Spawns,
    pub tables: Tables,
}

impl RomData {
    pub fn encode(&self) -> Vec<u8> {
        let sections = [
            (collision::TAG, self.collision.encode()),
            (backgrounds::TAG, self.backgrounds.encode()),
            (sprites::TAG, self.sprites.encode()),
            (spawns::TAG, self.spawns.encode()),
            (tables::TAG, self.tables.encode()),
        ];
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(sections.len() as u16).to_le_bytes());
        for (tag, payload) in sections {
            out.extend_from_slice(&tag);
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&payload);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RomError> {
        if bytes.get(..4) != Some(&MAGIC) {
            return Err(RomError::Magic);
        }
        let version = u16::from_le_bytes(read(bytes, 4, 2)?.try_into().unwrap());
        if version != VERSION {
            return Err(RomError::Version(version));
        }
        let count = u16::from_le_bytes(read(bytes, 6, 2)?.try_into().unwrap());
        let mut sections = Vec::new();
        let mut offset = 8;
        for _ in 0..count {
            let tag: [u8; 4] = read(bytes, offset, 4)?.try_into().unwrap();
            let len = u32::from_le_bytes(read(bytes, offset + 4, 4)?.try_into().unwrap());
            let payload = read(bytes, offset + 8, len as usize)?;
            sections.push((tag, payload));
            offset += 8 + len as usize;
        }
        let find = |tag: [u8; 4]| {
            sections
                .iter()
                .find(|(found, _)| *found == tag)
                .map(|(_, payload)| *payload)
                .ok_or(RomError::Missing(tag))
        };
        Ok(Self {
            collision: Collision::decode(find(collision::TAG)?)?,
            backgrounds: Backgrounds::decode(find(backgrounds::TAG)?)?,
            sprites: Sprites::decode(find(sprites::TAG)?)?,
            spawns: Spawns::decode(find(spawns::TAG)?)?,
            tables: Tables::decode(find(tables::TAG)?)?,
        })
    }

    /// Keep the data for the rest of the process, so the game can hold a
    /// `&'static` without global state.
    pub fn leak(self) -> &'static RomData {
        Box::leak(Box::new(self))
    }
}

fn read(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], RomError> {
    let end = offset.checked_add(len).ok_or(RomError::Truncated)?;
    bytes.get(offset..end).ok_or(RomError::Truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let rom = RomData::default();
        assert_eq!(RomData::decode(&rom.encode()), Ok(rom));
    }

    #[test]
    fn a_missing_section_is_reported_by_tag() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        assert_eq!(
            RomData::decode(&bytes),
            Err(RomError::Missing(collision::TAG))
        );
    }

    #[test]
    fn unknown_tags_are_skipped() {
        let mut bytes = RomData::default().encode();
        bytes[6] += 1;
        bytes.extend_from_slice(b"ZZZZ");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3]);
        assert_eq!(RomData::decode(&bytes), Ok(RomData::default()));
    }

    #[test]
    fn an_unknown_version_is_rejected() {
        let mut bytes = RomData::default().encode();
        bytes[4] = 2;
        assert_eq!(RomData::decode(&bytes), Err(RomError::Version(2)));
    }
}
