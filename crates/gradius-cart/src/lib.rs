//! Cartridge container handling that is independent of any emulator.
//!
//! Two responsibilities live here:
//!
//! * Reading a classic iNES file well enough to know its mapper, mirroring and
//!   PRG/CHR layout. This is used by the analysis oracle only.
//! * Decoding the immutable parts of that cartridge (CHR tile data) into plain
//!   tiles that `xtask extract` writes into a `gradius-pack` container. After
//!   extraction, nothing downstream ever opens the ROM again.

use std::fmt;

use thiserror::Error;

/// Length of a classic iNES header.
pub const INES_HEADER_LEN: usize = 16;

/// Errors produced while validating a classic iNES container.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CartError {
    #[error("file is too small for an iNES header ({0} bytes)")]
    TooSmall(usize),
    #[error("missing iNES signature (expected 4E 45 53 1A)")]
    BadSignature,
    #[error("NES 2.0 containers are not supported by this tool")]
    Nes2,
    #[error("cartridge declares no PRG ROM")]
    NoPrg,
    #[error("truncated payload: expected {expected} bytes after the header, found {actual}")]
    Truncated { expected: usize, actual: usize },
    #[error("unsupported mapper {0}; only mapper 3 (CNROM) is implemented")]
    UnsupportedMapper(u8),
    #[error("reserved header bytes 12..16 must be zero, found {0:02X?}")]
    DirtyHeader([u8; 4]),
}

/// Nametable arrangement declared by the cartridge header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
    SingleScreenLower,
    SingleScreenUpper,
}

/// Fields of the classic 16-byte iNES header that this project consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub prg_units: u8,
    pub chr_units: u8,
    pub mapper: u8,
    pub mirroring: Mirroring,
    pub has_trainer: bool,
    pub has_battery: bool,
}

impl Header {
    pub fn prg_len(&self) -> usize {
        usize::from(self.prg_units) * 16 * 1024
    }

    pub fn chr_len(&self) -> usize {
        usize::from(self.chr_units) * 8 * 1024
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "mapper:      {}", self.mapper)?;
        writeln!(
            f,
            "PRG ROM:     {} KiB ({} x 16 KiB)",
            self.prg_len() / 1024,
            self.prg_units
        )?;
        writeln!(
            f,
            "CHR ROM:     {} KiB ({} x 8 KiB)",
            self.chr_len() / 1024,
            self.chr_units
        )?;
        writeln!(f, "mirroring:   {:?}", self.mirroring)?;
        writeln!(f, "battery:     {}", self.has_battery)?;
        write!(f, "trainer:     {}", self.has_trainer)
    }
}

/// A validated classic iNES cartridge.
#[derive(Clone, Debug)]
pub struct Cartridge {
    pub header: Header,
    pub prg: Vec<u8>,
    pub chr: Vec<u8>,
    pub trainer: Option<Vec<u8>>,
    /// Bytes after the declared PRG/CHR payload. Preserved, never interpreted.
    pub trailing: Vec<u8>,
}

impl Cartridge {
    /// Parse and validate a classic iNES image.
    pub fn from_ines(data: &[u8]) -> Result<Self, CartError> {
        if data.len() < INES_HEADER_LEN {
            return Err(CartError::TooSmall(data.len()));
        }
        if &data[0..4] != b"NES\x1a" {
            return Err(CartError::BadSignature);
        }
        let flags6 = data[6];
        let flags7 = data[7];
        if (flags7 & 0x0C) == 0x08 {
            return Err(CartError::Nes2);
        }
        let reserved = [data[12], data[13], data[14], data[15]];
        if reserved != [0, 0, 0, 0] {
            return Err(CartError::DirtyHeader(reserved));
        }
        let prg_units = data[4];
        if prg_units == 0 {
            return Err(CartError::NoPrg);
        }
        let chr_units = data[5];
        let mapper = (flags6 >> 4) | (flags7 & 0xF0);
        let mirroring = match (flags6 & 0x08 != 0, flags6 & 0x01 != 0) {
            (true, _) => Mirroring::FourScreen,
            (false, true) => Mirroring::Vertical,
            (false, false) => Mirroring::Horizontal,
        };
        let header = Header {
            prg_units,
            chr_units,
            mapper,
            mirroring,
            has_trainer: flags6 & 0x04 != 0,
            has_battery: flags6 & 0x02 != 0,
        };

        let trainer_len = if header.has_trainer { 512 } else { 0 };
        let expected = trainer_len + header.prg_len() + header.chr_len();
        let actual = data.len() - INES_HEADER_LEN;
        if actual < expected {
            return Err(CartError::Truncated { expected, actual });
        }

        let body = &data[INES_HEADER_LEN..];
        let trainer = header.has_trainer.then(|| body[..trainer_len].to_vec());
        let prg_start = trainer_len;
        let prg_end = prg_start + header.prg_len();
        let chr_end = prg_end + header.chr_len();
        Ok(Self {
            header,
            trainer,
            prg: body[prg_start..prg_end].to_vec(),
            chr: body[prg_end..chr_end].to_vec(),
            trailing: body[chr_end..].to_vec(),
        })
    }

    /// Mapper 3 (CNROM) exposes PRG as one fixed 32 KiB window.
    ///
    /// The ROM must be 32 KiB for this project; anything else is rejected
    /// explicitly instead of being silently truncated.
    pub fn cpu_window(&self) -> Result<&[u8], CartError> {
        if self.prg.len() != 32 * 1024 {
            return Err(CartError::Truncated {
                expected: 32 * 1024,
                actual: self.prg.len(),
            });
        }
        Ok(&self.prg)
    }

    /// Decode every 4 KiB pattern-table half into 256 indexed 8x8 tiles.
    ///
    /// A CNROM 8 KiB CHR bank holds two pattern tables: `$0000-$0FFF` and
    /// `$1000-$1FFF`. Sprites normally use the second one ($1000) and the
    /// background the first, so both halves must be addressable. The pack's
    /// "bank" number is the half index: `hardware_bank * 2 + table`, where
    /// `table` is 0 for `$0000` and 1 for `$1000`.
    pub fn chr_banks(&self) -> Vec<[Tile; 256]> {
        self.chr
            .chunks_exact(4 * 1024)
            .map(decode_chr_bank)
            .collect()
    }
}

/// One 8x8 tile with 2-bit color indices (0..=3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub pixels: [u8; 64],
}

/// Decode a single 4 KiB pattern-table half (256 tiles, planar 2bpp).
pub fn decode_chr_bank(bank: &[u8]) -> [Tile; 256] {
    debug_assert_eq!(bank.len(), 4 * 1024);
    std::array::from_fn(|index| {
        let base = index * 16;
        let mut tile = Tile { pixels: [0; 64] };
        for row in 0..8 {
            let low = bank[base + row];
            let high = bank[base + row + 8];
            for col in 0..8 {
                let bit = 7 - col;
                let value = ((low >> bit) & 1) | (((high >> bit) & 1) << 1);
                tile.pixels[row * 8 + col] = value;
            }
        }
        tile
    })
}

/// The 64 colors of the NES master palette, in RGB.
///
/// This is a widely used approximation of the 2C02 output; it is not derived
/// from the cartridge and therefore safe to ship in the pack as a fallback.
pub const NES_MASTER_PALETTE: [[u8; 3]; 64] = [
    [0x66, 0x66, 0x66],
    [0x00, 0x2A, 0x88],
    [0x14, 0x12, 0xA7],
    [0x3B, 0x00, 0xA4],
    [0x5C, 0x00, 0x7E],
    [0x6E, 0x00, 0x40],
    [0x6C, 0x06, 0x00],
    [0x56, 0x1D, 0x00],
    [0x33, 0x35, 0x00],
    [0x0B, 0x48, 0x00],
    [0x00, 0x52, 0x00],
    [0x00, 0x4F, 0x08],
    [0x00, 0x40, 0x4D],
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00],
    [0xAD, 0xAD, 0xAD],
    [0x15, 0x5F, 0xD9],
    [0x42, 0x40, 0xFF],
    [0x75, 0x27, 0xFE],
    [0xA0, 0x1A, 0xCC],
    [0xB7, 0x1E, 0x7B],
    [0xB5, 0x31, 0x20],
    [0x99, 0x4E, 0x00],
    [0x6B, 0x6D, 0x00],
    [0x38, 0x87, 0x00],
    [0x0C, 0x93, 0x00],
    [0x00, 0x8F, 0x32],
    [0x00, 0x7C, 0x8D],
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00],
    [0xFF, 0xFE, 0xFF],
    [0x64, 0xB0, 0xFF],
    [0x92, 0x90, 0xFF],
    [0xC6, 0x76, 0xFF],
    [0xF3, 0x6A, 0xFF],
    [0xFE, 0x6E, 0xCC],
    [0xFE, 0x81, 0x70],
    [0xEA, 0x9E, 0x22],
    [0xBC, 0xBE, 0x00],
    [0x88, 0xD8, 0x00],
    [0x5C, 0xE4, 0x30],
    [0x45, 0xE0, 0x82],
    [0x48, 0xCD, 0xDE],
    [0x4F, 0x4F, 0x4F],
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00],
    [0xFF, 0xFE, 0xFF],
    [0xC0, 0xDF, 0xFF],
    [0xD3, 0xD2, 0xFF],
    [0xE8, 0xC8, 0xFF],
    [0xFB, 0xC2, 0xFF],
    [0xFE, 0xC4, 0xEA],
    [0xFE, 0xCC, 0xC5],
    [0xF7, 0xD8, 0xA5],
    [0xE4, 0xE5, 0x94],
    [0xCF, 0xEF, 0x96],
    [0xBD, 0xF4, 0xAB],
    [0xB3, 0xF3, 0xCC],
    [0xB5, 0xEB, 0xF2],
    [0xB8, 0xB8, 0xB8],
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00],
];

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_ines(prg_units: u8, chr_units: u8, flags6: u8) -> Vec<u8> {
        let mut data = vec![0u8; INES_HEADER_LEN];
        data[0..4].copy_from_slice(b"NES\x1a");
        data[4] = prg_units;
        data[5] = chr_units;
        data[6] = flags6;
        data.resize(
            INES_HEADER_LEN + usize::from(prg_units) * 16 * 1024 + usize::from(chr_units) * 8192,
            0,
        );
        data
    }

    #[test]
    fn parses_vertical_mapper3() {
        let cart = Cartridge::from_ines(&minimal_ines(2, 4, 0x31)).unwrap();
        assert_eq!(cart.header.mapper, 3);
        assert_eq!(cart.header.mirroring, Mirroring::Vertical);
        assert_eq!(cart.prg.len(), 32 * 1024);
        assert_eq!(cart.chr.len(), 32 * 1024);
        // Four 8 KiB CNROM banks, each split into two 4 KiB pattern tables.
        assert_eq!(cart.chr_banks().len(), 8);
    }

    #[test]
    fn rejects_nes2() {
        let mut data = minimal_ines(2, 4, 0x31);
        data[7] = 0x08;
        assert!(matches!(Cartridge::from_ines(&data), Err(CartError::Nes2)));
    }

    #[test]
    fn rejects_truncated_payload() {
        let mut data = minimal_ines(2, 4, 0x31);
        data.truncate(INES_HEADER_LEN + 100);
        assert!(matches!(
            Cartridge::from_ines(&data),
            Err(CartError::Truncated { .. })
        ));
    }

    #[test]
    fn chr_glyph_decoding_is_2bpp() {
        let mut bank = vec![0u8; 4096];
        // Row 0: low plane all ones, high plane all ones -> color 3.
        bank[0] = 0xFF;
        bank[8] = 0xFF;
        let tiles = decode_chr_bank(&bank);
        assert_eq!(&tiles[0].pixels[..8], &[3; 8]);
        assert_eq!(tiles[1].pixels, [0; 64]);
    }

    #[test]
    fn every_pattern_table_half_is_decoded() {
        let mut ines = minimal_ines(2, 4, 0x31);
        // First PRG byte of the sprite table of hardware bank 0 is at CHR
        // offset 0x1000 -> pack half 1, tile 0.
        ines[INES_HEADER_LEN + 32 * 1024 + 0x1000] = 0xFF;
        let cart = Cartridge::from_ines(&ines).unwrap();
        let halves = cart.chr_banks();
        assert_eq!(halves.len(), 8);
        assert_eq!(&halves[1][0].pixels[..8], &[1; 8]);
        assert_eq!(halves[0][0].pixels, [0; 64]);
    }
}
