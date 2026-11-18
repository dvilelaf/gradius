//! `SPRT` section of `CHUNK_ROM`: the sprite-group definitions behind the two
//! pointer tables at `$8D91` (sprite types `$00`-`$7F`) and `$8E91` (types
//! `$80`-`$A3`, low seven bits).
//!
//! Layout (little endian): `u16` entry count of table A, `u16` entry count of
//! table B; then every entry of A and then of B: a `u8` record count and that
//! many `[dy, tile, attributes, dx]` records. `dy` and `dx` are two's
//! complement offsets. An empty entry has a record count of zero.

use super::RomError;

pub const TAG: [u8; 4] = *b"SPRT";

/// One sprite of a group: `[dy, tile, attributes, dx]`.
pub type Record = [u8; 4];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sprites {
    /// Table A, indexed by sprite type `$00`-`$7F`.
    pub a: Vec<Vec<Record>>,
    /// Table B, indexed by sprite type with the top bit cleared.
    pub b: Vec<Vec<Record>>,
}

impl Sprites {
    pub fn encode(&self) -> Vec<u8> {
        let len = |table: &[Vec<Record>]| u16::try_from(table.len()).expect("table too long");
        let mut out = Vec::new();
        out.extend_from_slice(&len(&self.a).to_le_bytes());
        out.extend_from_slice(&len(&self.b).to_le_bytes());
        for definition in self.a.iter().chain(&self.b) {
            out.push(u8::try_from(definition.len()).expect("definition too long"));
            definition
                .iter()
                .for_each(|record| out.extend_from_slice(record));
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RomError> {
        let mut at = 0;
        let mut take = |len: usize| {
            let part = bytes.get(at..at + len).ok_or(RomError::Truncated)?;
            at += len;
            Ok::<_, RomError>(part)
        };
        let a_len = u16::from_le_bytes(take(2)?.try_into().unwrap());
        let b_len = u16::from_le_bytes(take(2)?.try_into().unwrap());
        let mut definitions = Vec::new();
        for _ in 0..usize::from(a_len) + usize::from(b_len) {
            let count = take(1)?[0];
            let records = take(usize::from(count) * 4)?;
            definitions.push(
                records
                    .chunks_exact(4)
                    .map(|r| r.try_into().unwrap())
                    .collect(),
            );
        }
        let b = definitions.split_off(usize::from(a_len));
        Ok(Self { a: definitions, b })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sprites {
        Sprites {
            a: vec![
                vec![],
                vec![[0xFE, 0x61, 0x22, 0x08], [4, 0x63, 0x23, 0x00]],
            ],
            b: vec![vec![[0, 0x10, 0x01, 0xF0]], vec![]],
        }
    }

    #[test]
    fn encode_then_decode_round_trips() {
        let sprites = sample();
        assert_eq!(Sprites::decode(&sprites.encode()), Ok(sprites));
    }

    #[test]
    fn a_truncated_section_is_rejected() {
        let bytes = sample().encode();
        for len in [0, 1, 3, 5, bytes.len() - 1] {
            assert_eq!(Sprites::decode(&bytes[..len]), Err(RomError::Truncated));
        }
    }
}
