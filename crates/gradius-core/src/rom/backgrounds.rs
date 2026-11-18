//! `BKGD` section of `CHUNK_ROM`: the title screen and the scrolling
//! backgrounds of stages 1-7 as the PPU displayed them, plus the stage-1
//! terrain solidity set.
//!
//! Payload: one screen for the title, `u8` stage count, that many stage
//! screens, then 256 solidity bytes (1 = solid). A screen is `u16` width and
//! height in tiles, `u8` CHR bank, 32 palette bytes, then the tiles and the
//! attributes, each as `u32` length plus an RLE stream. RLE: control byte
//! `c < 0x80` copies the next `c + 1` bytes, `c >= 0x80` repeats the next byte
//! `(c & 0x7F) + 1` times.

use super::{read, RomError};

pub const TAG: [u8; 4] = *b"BKGD";

/// One extracted screen or scrollable background (`stage_bg::Screen`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RomScreen {
    pub width_tiles: usize,
    pub height_tiles: usize,
    /// Nametable tile indices, row-major.
    pub tiles: Vec<u8>,
    /// One NES attribute byte per 4x4-tile block, row-major.
    pub attributes: Vec<u8>,
    /// Palette RAM (32 NES master-palette indices) as displayed.
    pub palette: [u8; 32],
    /// Hardware CHR bank active for this screen.
    pub chr_bank: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Backgrounds {
    pub title: RomScreen,
    /// Stage 1 first.
    pub stages: Vec<RomScreen>,
    /// One byte per tile index: 1 = solid stage-1 terrain.
    pub solid: [u8; 256],
}

impl Default for Backgrounds {
    fn default() -> Self {
        Self {
            title: RomScreen::default(),
            stages: Vec::new(),
            solid: [0; 256],
        }
    }
}

impl RomScreen {
    fn attribute_len(width: usize, height: usize) -> usize {
        (width / 4) * height.div_ceil(4)
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.width_tiles as u16).to_le_bytes());
        out.extend_from_slice(&(self.height_tiles as u16).to_le_bytes());
        out.push(self.chr_bank);
        out.extend_from_slice(&self.palette);
        for data in [&self.tiles, &self.attributes] {
            let packed = rle_encode(data);
            out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            out.extend_from_slice(&packed);
        }
    }

    fn decode(bytes: &[u8], offset: &mut usize) -> Result<Self, RomError> {
        let head = read(bytes, *offset, 37)?;
        let width_tiles = usize::from(u16::from_le_bytes([head[0], head[1]]));
        let height_tiles = usize::from(u16::from_le_bytes([head[2], head[3]]));
        let chr_bank = head[4];
        let palette: [u8; 32] = head[5..37].try_into().unwrap();
        *offset += 37;
        let mut stream = |expected: usize| -> Result<Vec<u8>, RomError> {
            let len = u32::from_le_bytes(read(bytes, *offset, 4)?.try_into().unwrap());
            let packed = read(bytes, *offset + 4, len as usize)?;
            *offset += 4 + len as usize;
            rle_decode(packed, expected)
        };
        let tiles = stream(width_tiles * height_tiles)?;
        let attributes = stream(Self::attribute_len(width_tiles, height_tiles))?;
        Ok(Self {
            width_tiles,
            height_tiles,
            tiles,
            attributes,
            palette,
            chr_bank,
        })
    }
}

impl Backgrounds {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.title.encode(&mut out);
        out.push(self.stages.len() as u8);
        for stage in &self.stages {
            stage.encode(&mut out);
        }
        out.extend_from_slice(&self.solid);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RomError> {
        let mut offset = 0;
        let title = RomScreen::decode(bytes, &mut offset)?;
        let count = read(bytes, offset, 1)?[0];
        offset += 1;
        let mut stages = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            stages.push(RomScreen::decode(bytes, &mut offset)?);
        }
        let solid = read(bytes, offset, 256)?.try_into().unwrap();
        Ok(Self {
            title,
            stages,
            solid,
        })
    }
}

/// Encode bytes with the run-length scheme described in the module doc.
pub fn rle_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < data.len() {
        let mut run = 1;
        while index + run < data.len() && data[index + run] == data[index] && run < 128 {
            run += 1;
        }
        if run >= 3 {
            out.push(0x80 | (run as u8 - 1));
            out.push(data[index]);
            index += run;
        } else {
            let start = index;
            let mut literal = 0;
            while index < data.len() && literal < 128 {
                let mut ahead = 1;
                while index + ahead < data.len()
                    && data[index + ahead] == data[index]
                    && ahead < 128
                {
                    ahead += 1;
                }
                if ahead >= 3 || literal + ahead > 128 {
                    break;
                }
                literal += ahead;
                index += ahead;
            }
            out.push(literal as u8 - 1);
            out.extend_from_slice(&data[start..start + literal]);
        }
    }
    out
}

/// Decode an RLE stream that must expand to exactly `expected` bytes.
fn rle_decode(packed: &[u8], expected: usize) -> Result<Vec<u8>, RomError> {
    let mut out = Vec::with_capacity(expected);
    let mut index = 0;
    while index < packed.len() {
        let control = packed[index];
        index += 1;
        if control < 0x80 {
            let count = usize::from(control) + 1;
            out.extend_from_slice(read(packed, index, count)?);
            index += count;
        } else {
            let byte = *packed.get(index).ok_or(RomError::Truncated)?;
            index += 1;
            out.resize(out.len() + usize::from(control & 0x7F) + 1, byte);
        }
        if out.len() > expected {
            return Err(RomError::Truncated);
        }
    }
    if out.len() != expected {
        return Err(RomError::Truncated);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Backgrounds {
        let screen = |seed: u8| RomScreen {
            width_tiles: 8,
            height_tiles: 6,
            tiles: (0..48u8).map(|n| n / 5 + seed).collect(),
            attributes: vec![seed, 0, 0, 7],
            palette: [seed; 32],
            chr_bank: seed,
        };
        let mut solid = [0; 256];
        solid[0xDD] = 1;
        Backgrounds {
            title: screen(1),
            stages: vec![screen(2), screen(3)],
            solid,
        }
    }

    #[test]
    fn round_trips() {
        let backgrounds = sample();
        assert_eq!(
            Backgrounds::decode(&backgrounds.encode()),
            Ok(backgrounds.clone())
        );
        assert_eq!(
            Backgrounds::decode(&Backgrounds::default().encode()),
            Ok(Backgrounds::default())
        );
    }

    #[test]
    fn a_truncated_section_is_rejected() {
        let bytes = sample().encode();
        for cut in [0, 10, bytes.len() / 2, bytes.len() - 1] {
            assert_eq!(
                Backgrounds::decode(&bytes[..cut]),
                Err(RomError::Truncated),
                "cut at {cut}"
            );
        }
    }

    #[test]
    fn rle_handles_runs_and_long_literals() {
        let mut data: Vec<u8> = (0..127u16).map(|n| n as u8).collect();
        data.extend_from_slice(&[200, 200]);
        data.extend_from_slice(&[7; 300]);
        data.extend((0..600u32).map(|n| (n % 251) as u8));
        let packed = rle_encode(&data);
        assert_eq!(rle_decode(&packed, data.len()), Ok(data));
    }
}
