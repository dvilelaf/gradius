//! `SPWN` section of `CHUNK_ROM`: the stage spawn scripts and the object,
//! terrain and formation definitions they refer to.
//!
//! Layout (little endian): the eight `$A7CA` words; the 7x8 column pointers;
//! the 7x8 column streams (`u16` count, then per record a tag byte: 0 =
//! `delay kind`, 1 = five command bytes); then one `u16`-counted list per
//! definition table, in field order.

use super::RomError;

pub const TAG: [u8; 4] = *b"SPWN";

/// Playable sections (`$A7CA` has an eighth word that is not a section).
pub const SECTION_COUNT: usize = 7;
/// Column slots per section; sections 2-6 leave the last one empty.
pub const COLUMN_SLOTS: usize = 8;

const SPAWN: u8 = 0;
const COMMAND: u8 = 1;

/// One decoded record of a column stream (`$A32F`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Record {
    /// `<delay> <type>`: spawn after `2*delay` pixels.
    Spawn { delay: u8, kind: u8 },
    /// Five bytes copied to `$63-$67`.
    Command([u8; 5]),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spawns {
    /// `$A7CA`: the eight section sub-table pointers.
    pub section_pointers: [u16; COLUMN_SLOTS],
    /// Column-stream pointer of each section column (0 for an empty slot).
    pub column_pointers: [[u16; COLUMN_SLOTS]; SECTION_COUNT],
    /// Column streams of each section; unused slots are empty.
    pub sections: [[Vec<Record>; COLUMN_SLOTS]; SECTION_COUNT],
    /// `$A5FC`: 4-byte enemy definitions.
    pub object_defs: Vec<[u8; 4]>,
    /// `$A65C`: 3-byte terrain definitions.
    pub terrain_defs: Vec<[u8; 3]>,
    /// `$A58C`: formation descriptors.
    pub formation_subtypes: Vec<[u8; 2]>,
    /// `$A5B6`: per-formation spawn parameters.
    pub formation_params: Vec<[u8; 3]>,
    /// `$ADBB`: enemy animation tiles.
    pub animation_frames: Vec<u8>,
    /// `$C4F0`: stage-1 eruption velocities `(x, y, y acceleration)`.
    pub eruption_velocity: Vec<(i8, i8, u8)>,
    /// `$AE6B`: the debris sprite streams.
    pub debris_paths: Vec<Vec<u8>>,
}

fn put_len(out: &mut Vec<u8>, len: usize) {
    out.extend_from_slice(&u16::try_from(len).expect("table too long").to_le_bytes());
}

fn put_rows<const N: usize>(out: &mut Vec<u8>, rows: &[[u8; N]]) {
    put_len(out, rows.len());
    rows.iter().for_each(|row| out.extend_from_slice(row));
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], RomError> {
        let (head, tail) = self.0.split_at_checked(len).ok_or(RomError::Truncated)?;
        self.0 = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, RomError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, RomError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn rows<const N: usize>(&mut self) -> Result<Vec<[u8; N]>, RomError> {
        let len = usize::from(self.u16()?);
        let bytes = self.take(len * N)?;
        Ok(bytes
            .chunks_exact(N)
            .map(|c| c.try_into().unwrap())
            .collect())
    }

    fn record(&mut self) -> Result<Record, RomError> {
        match self.u8()? {
            SPAWN => Ok(Record::Spawn {
                delay: self.u8()?,
                kind: self.u8()?,
            }),
            COMMAND => Ok(Record::Command(self.take(5)?.try_into().unwrap())),
            _ => Err(RomError::Truncated),
        }
    }
}

impl Spawns {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for word in self
            .section_pointers
            .iter()
            .chain(self.column_pointers.iter().flatten())
        {
            out.extend_from_slice(&word.to_le_bytes());
        }
        for records in self.sections.iter().flatten() {
            put_len(&mut out, records.len());
            for record in records {
                match record {
                    Record::Spawn { delay, kind } => out.extend_from_slice(&[SPAWN, *delay, *kind]),
                    Record::Command(bytes) => {
                        out.push(COMMAND);
                        out.extend_from_slice(bytes);
                    }
                }
            }
        }
        put_rows(&mut out, &self.object_defs);
        put_rows(&mut out, &self.terrain_defs);
        put_rows(&mut out, &self.formation_subtypes);
        put_rows(&mut out, &self.formation_params);
        put_len(&mut out, self.animation_frames.len());
        out.extend_from_slice(&self.animation_frames);
        put_len(&mut out, self.eruption_velocity.len());
        for (x, y, accel) in &self.eruption_velocity {
            out.extend_from_slice(&[*x as u8, *y as u8, *accel]);
        }
        put_len(&mut out, self.debris_paths.len());
        for path in &self.debris_paths {
            put_len(&mut out, path.len());
            out.extend_from_slice(path);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RomError> {
        let mut r = Reader(bytes);
        let mut spawns = Self::default();
        for word in spawns.section_pointers.iter_mut() {
            *word = r.u16()?;
        }
        for word in spawns.column_pointers.iter_mut().flatten() {
            *word = r.u16()?;
        }
        for records in spawns.sections.iter_mut().flatten() {
            let len = r.u16()?;
            *records = (0..len).map(|_| r.record()).collect::<Result<_, _>>()?;
        }
        spawns.object_defs = r.rows()?;
        spawns.terrain_defs = r.rows()?;
        spawns.formation_subtypes = r.rows()?;
        spawns.formation_params = r.rows()?;
        let len = usize::from(r.u16()?);
        spawns.animation_frames = r.take(len)?.to_vec();
        spawns.eruption_velocity = r
            .rows::<3>()?
            .into_iter()
            .map(|[x, y, accel]| (x as i8, y as i8, accel))
            .collect();
        for _ in 0..r.u16()? {
            let len = usize::from(r.u16()?);
            spawns.debris_paths.push(r.take(len)?.to_vec());
        }
        Ok(spawns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Spawns {
        let mut spawns = Spawns {
            section_pointers: [1, 2, 3, 4, 5, 6, 7, 8],
            object_defs: vec![[1, 2, 3, 4]],
            terrain_defs: vec![[5, 6, 7]],
            formation_subtypes: vec![[8, 9]],
            formation_params: vec![[1, 2, 3]],
            animation_frames: vec![1, 2, 3],
            eruption_velocity: vec![(-2, 7, 0x40)],
            debris_paths: vec![vec![1, 2, 0], vec![3, 0]],
            ..Spawns::default()
        };
        spawns.column_pointers[6][7] = 0xABCD;
        spawns.sections[3][2] = vec![
            Record::Spawn {
                delay: 9,
                kind: 0x81,
            },
            Record::Command([0xF0, 1, 2, 3, 4]),
        ];
        spawns
    }

    #[test]
    fn round_trips() {
        let spawns = sample();
        assert_eq!(Spawns::decode(&spawns.encode()), Ok(spawns));
    }

    #[test]
    fn a_truncated_section_is_rejected() {
        let bytes = sample().encode();
        for len in [0, 10, bytes.len() - 1] {
            assert_eq!(Spawns::decode(&bytes[..len]), Err(RomError::Truncated));
        }
    }

    #[test]
    fn an_unknown_record_tag_is_rejected() {
        let mut bytes = Spawns::default().encode();
        let first = 2 * (COLUMN_SLOTS + SECTION_COUNT * COLUMN_SLOTS);
        bytes[first] = 1;
        bytes.splice(first + 2..first + 2, [9, 0, 0]);
        assert_eq!(Spawns::decode(&bytes), Err(RomError::Truncated));
    }
}
