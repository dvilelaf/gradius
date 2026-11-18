//! `COLL` section of `CHUNK_ROM`: the packed `$0500`/`$0600` collision map of
//! each stage and the per-section `$A65C` terrain placements.
//!
//! Payload: `u8` stage count, `u8` section count, the section thresholds, then
//! per stage `MAP_LEN` map bytes, then per section a `u16` record count and
//! `(column, kind)` pairs.

use super::RomError;

pub const TAG: [u8; 4] = *b"COLL";

/// Playable stages (`$19` 0..6).
pub const STAGES: usize = 7;
/// Bytes of a packed map: 448 columns x 8 four-row groups.
pub const MAP_LEN: usize = 3584;

/// One `$A65C` terrain piece placed by a section's column stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terrain {
    /// Stream delay (16-px units) at which `$A32F` consumes the record.
    pub column: u8,
    /// `$A65C` terrain definition index.
    pub kind: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Collision {
    /// `$9A30`: section end in `$3F` units of 256 px.
    pub threshold: [u8; STAGES],
    /// Terrain records of each section's `$A7CA` streams, in stream order.
    pub terrain: [Vec<Terrain>; STAGES],
    /// Packed map of each stage, `MAP_LEN` bytes, byte `column * 8 + group`.
    pub maps: Vec<Vec<u8>>,
}

impl Default for Collision {
    fn default() -> Self {
        Self {
            threshold: [0; STAGES],
            terrain: Default::default(),
            maps: vec![vec![0; MAP_LEN]; STAGES],
        }
    }
}

impl Collision {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![STAGES as u8, STAGES as u8];
        out.extend_from_slice(&self.threshold);
        for map in &self.maps {
            out.extend_from_slice(map);
        }
        for records in &self.terrain {
            out.extend_from_slice(&(records.len() as u16).to_le_bytes());
            for record in records {
                out.extend_from_slice(&[record.column, record.kind]);
            }
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
        if take(2)? != [STAGES as u8, STAGES as u8] {
            return Err(RomError::Truncated);
        }
        let threshold = take(STAGES)?.try_into().unwrap();
        let mut maps = Vec::with_capacity(STAGES);
        for _ in 0..STAGES {
            maps.push(take(MAP_LEN)?.to_vec());
        }
        let mut terrain: [Vec<Terrain>; STAGES] = Default::default();
        for records in &mut terrain {
            let count = u16::from_le_bytes(take(2)?.try_into().unwrap());
            for pair in take(usize::from(count) * 2)?.chunks_exact(2) {
                records.push(Terrain {
                    column: pair[0],
                    kind: pair[1],
                });
            }
        }
        Ok(Self {
            threshold,
            terrain,
            maps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Collision {
        Collision {
            threshold: [12, 12, 12, 12, 11, 11, 12],
            terrain: std::array::from_fn(|stage| {
                (0..stage as u8)
                    .map(|n| Terrain {
                        column: n,
                        kind: n + 1,
                    })
                    .collect()
            }),
            maps: (0..STAGES)
                .map(|stage| (0..MAP_LEN).map(|n| (n + stage) as u8).collect())
                .collect(),
        }
    }

    #[test]
    fn encode_then_decode_round_trips() {
        let collision = sample();
        assert_eq!(Collision::decode(&collision.encode()), Ok(collision));
    }

    #[test]
    fn a_truncated_section_is_rejected() {
        let bytes = sample().encode();
        for len in [0, 1, 9, bytes.len() / 2, bytes.len() - 1] {
            assert_eq!(Collision::decode(&bytes[..len]), Err(RomError::Truncated));
        }
    }
}
