//! The full ROM extraction as a library: the bytes of a Gradius (E) ROM go
//! in, the bytes of `gradius.pak` come out. No files, no paths.
//!
//! The captures run the cartridge in the oracle emulator. With the `parallel`
//! feature (default, never on `wasm32`) the background captures use one thread
//! per stage; without it everything is sequential and the pack is identical.

use std::fmt;

use gradius_cart::{Cartridge, NES_MASTER_PALETTE};
use gradius_pack::PackBuilder;
use sha2::{Digest, Sha256};

pub mod nametable;
pub mod oracle;
mod rom;
pub mod screens;
pub mod sections;
pub mod sprite_groups;
pub mod stage2;
pub mod stage_data;

/// SHA-256 of PRG+CHR (the file without its 16-byte header) of Gradius (E).
/// A hash identifies the ROM; it is not data from it.
pub const GRADIUS_E_PAYLOAD_SHA256: [u8; 32] = [
    0x3a, 0xa9, 0x7b, 0x51, 0x9a, 0x51, 0x91, 0x76, 0x6a, 0x53, 0xae, 0x51, 0xc2, 0x8a, 0x51, 0xc1,
    0x77, 0x78, 0x71, 0x6b, 0xc0, 0x2e, 0xf5, 0x8b, 0x9c, 0x82, 0xaf, 0xc7, 0x61, 0xe2, 0x03, 0x29,
];

const INES_HEADER_LEN: usize = 16;
const TRAINER_LEN: usize = 512;

/// The header the extraction reads Gradius (E) with: mapper 3, vertical
/// mirroring, 32 KiB PRG and 32 KiB CHR, no trainer, no reserved bytes. A
/// dump's own header is ignored, so every dump of the same PRG+CHR gives the
/// same pack.
const CANONICAL_HEADER: [u8; INES_HEADER_LEN] = [
    b'N', b'E', b'S', 0x1a, 0x02, 0x04, 0x31, 0x00, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[derive(Debug)]
pub enum ExtractError {
    /// The bytes are not an iNES file.
    NotInes(String),
    /// A valid iNES file, but not Gradius (E).
    WrongRom { expected: [u8; 32], found: [u8; 32] },
    /// The extraction itself failed.
    Failed(String),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInes(why) => write!(f, "this is not an iNES ROM file ({why})"),
            Self::WrongRom { expected, found } => write!(
                f,
                "this is not Gradius (E): expected SHA-256 {} for PRG+CHR, read {}",
                hex(expected),
                hex(found)
            ),
            Self::Failed(why) => write!(f, "extraction failed: {why}"),
        }
    }
}

impl std::error::Error for ExtractError {}

fn failed(error: impl fmt::Display) -> ExtractError {
    ExtractError::Failed(format!("{error:#}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Lower-case hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// FNV-1a, 64 bit. Small, stable and dependency-free; used for the nametable
/// and framebuffer digests.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The hashes of an accepted ROM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RomInfo {
    /// SHA-256 of the whole file.
    pub sha256: [u8; 32],
    /// SHA-256 of PRG+CHR.
    pub payload_sha256: [u8; 32],
}

/// Where PRG+CHR sit in an iNES file: after the header and any trainer.
fn payload_range(rom: &[u8]) -> Result<std::ops::Range<usize>, ExtractError> {
    let not_ines = |why: &str| ExtractError::NotInes(why.to_string());
    if rom.len() < INES_HEADER_LEN || &rom[..4] != b"NES\x1a" {
        return Err(not_ines("missing the NES 1A signature"));
    }
    let trainer = if rom[6] & 0x04 != 0 { TRAINER_LEN } else { 0 };
    let start = INES_HEADER_LEN + trainer;
    let end = start + usize::from(rom[4]) * 0x4000 + usize::from(rom[5]) * 0x2000;
    if end > rom.len() {
        return Err(not_ines("the file is shorter than its header says"));
    }
    Ok(start..end)
}

/// Check that `rom` is an iNES file holding Gradius (E). Only PRG+CHR is
/// hashed, so dumps that differ in the header bytes are accepted.
pub fn identify(rom: &[u8]) -> Result<RomInfo, ExtractError> {
    let payload = &rom[payload_range(rom)?];
    let payload_sha256: [u8; 32] = Sha256::digest(payload).into();
    if payload_sha256 != GRADIUS_E_PAYLOAD_SHA256 {
        return Err(ExtractError::WrongRom {
            expected: GRADIUS_E_PAYLOAD_SHA256,
            found: payload_sha256,
        });
    }
    Ok(RomInfo {
        sha256: Sha256::digest(rom).into(),
        payload_sha256,
    })
}

/// The extraction steps, reported through the progress callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Tiles,
    Audio,
    Backgrounds,
    Collision,
    Spawns,
    Sprites,
    Tables,
    Stage,
    Pack,
}

/// Parse a ROM for the captures with [`CANONICAL_HEADER`] in place of the
/// dump's header (its mirroring, reserved bytes and trainer are dropped). The
/// content was already checked by [`identify`].
pub fn load_cart(rom: &[u8]) -> anyhow::Result<Cartridge> {
    let payload = &rom[payload_range(rom)?];
    let data = [&CANONICAL_HEADER[..], payload].concat();
    let cart = Cartridge::from_ines(&data)?;
    anyhow::ensure!(
        cart.header.mapper == 3,
        "mapper {}, but this project targets mapper 3 (CNROM)",
        cart.header.mapper
    );
    cart.cpu_window()?;
    Ok(cart)
}

/// Build `gradius.pak` from the bytes of a Gradius (E) ROM.
///
/// `progress(step, done, total)` is called at least when each step starts and
/// when it ends.
pub fn extract(
    rom: &[u8],
    progress: &mut dyn FnMut(Step, u32, u32),
) -> Result<Vec<u8>, ExtractError> {
    let info = identify(rom)?;
    let cart = load_cart(rom).map_err(failed)?;

    progress(Step::Tiles, 0, 1);
    let banks = cart.chr_banks();
    if banks.len() != gradius_pack::TILE_BANKS {
        return Err(failed(format!(
            "expected {} pattern-table halves, found {}",
            gradius_pack::TILE_BANKS,
            banks.len()
        )));
    }
    let bank_pixels: Vec<Vec<[u8; 64]>> = banks
        .iter()
        .map(|bank| bank.iter().map(|tile| tile.pixels).collect())
        .collect();
    let bank_start = usize::from(gradius_audio::driver::DRIVER_BASE - 0x8000);
    let bank = cart
        .prg
        .get(bank_start..bank_start + 0x4000)
        .ok_or_else(|| failed("the driver bank is not mapped at $C000"))?;
    let source = gradius_audio::driver::extract_source(gradius_audio::driver::DRIVER_BASE, bank);
    let driver_segments: usize = source.segments.iter().map(|(_, bytes)| bytes.len()).sum();
    let driver = gradius_audio::chunk::encode_driver(&source);
    progress(Step::Tiles, 1, 1);

    progress(Step::Audio, 0, 1);
    let capture = gradius_audio::capture::capture_all(&cart).map_err(failed)?;
    let audio = gradius_audio::chunk::encode_chunk(&capture.music, &capture.sfx, &driver);
    progress(Step::Audio, 1, 1);

    progress(Step::Stage, 0, 1);
    let stage = stage_data::decode(&cart.prg).map_err(failed)?;
    progress(Step::Stage, 1, 1);
    let rom_data = rom::extract(&cart, progress).map_err(failed)?;

    progress(Step::Pack, 0, 1);
    let meta = format!(
        "project=gradius\nformat=indexed-8x8-2bpp\nmapper={}\nprg_bytes={}\nchr_bytes={}\nsource_sha256={}\naudio_format=GAUD\naudio_version={}\naudio_tracks={}\naudio_sfx={}\naudio_events={}\naudio_driver_bytes={}\naudio_driver_sequences={}\naudio_sha256={}\naudio_source=oracle-capture $ECFC; driver=$EFC7/$EFB2; sequences=12 music + 10 SFX\n",
        cart.header.mapper,
        cart.prg.len(),
        cart.chr.len(),
        hex(&info.payload_sha256),
        gradius_audio::AUDIO_CHUNK_VERSION,
        gradius_audio::Music::ALL.len(),
        gradius_audio::Sfx::ALL.len(),
        capture.events(),
        driver.len(),
        driver_segments,
        sha256_hex(&audio),
    );
    let pack = PackBuilder::new()
        .tiles(&bank_pixels)
        .palettes(&NES_MASTER_PALETTE)
        .chunk(gradius_pack::CHUNK_AUDIO, audio)
        .chunk(gradius_pack::CHUNK_STAGE, stage_data::encode(&stage))
        .chunk(gradius_pack::CHUNK_ROM, rom_data.encode())
        .meta(meta)
        .finish();
    progress(Step::Pack, 1, 1);
    Ok(pack)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ROM named by `GRADIUS_ROM`; tests that need it skip without it.
    fn rom() -> Option<Vec<u8>> {
        std::fs::read(std::env::var_os("GRADIUS_ROM")?).ok()
    }

    #[test]
    fn identify_accepts_gradius_e() {
        let Some(rom) = rom() else { return };
        let info = identify(&rom).unwrap();
        assert_eq!(info.payload_sha256, GRADIUS_E_PAYLOAD_SHA256);
        assert_eq!(info.sha256, <[u8; 32]>::from(Sha256::digest(&rom)));
    }

    #[test]
    fn identify_ignores_the_header_padding() {
        let Some(mut rom) = rom() else { return };
        rom[12] = 0x5a;
        rom[15] = 0xff;
        assert!(identify(&rom).is_ok());
    }

    #[test]
    fn extraction_ignores_the_header() {
        let Some(rom) = rom() else { return };
        let mut altered = rom.clone();
        altered[6] ^= 0x01;
        altered[7..16].fill(0xa5);
        assert!(identify(&altered).is_ok());
        let original = extract(&rom, &mut |_, _, _| {}).unwrap();
        let other = extract(&altered, &mut |_, _, _| {}).unwrap();
        assert!(original == other, "the pack changed with the header");
    }

    #[test]
    fn identify_rejects_a_changed_byte() {
        let Some(mut rom) = rom() else { return };
        rom[INES_HEADER_LEN + 1000] ^= 1;
        let error = identify(&rom).unwrap_err();
        assert!(matches!(error, ExtractError::WrongRom { .. }));
        let text = error.to_string();
        assert!(text.contains(&hex(&GRADIUS_E_PAYLOAD_SHA256)), "{text}");
    }

    #[test]
    fn identify_rejects_files_that_are_not_ines() {
        for bytes in [&b""[..], b"not a rom at all, just text", &[0u8; 64]] {
            let error = identify(bytes).unwrap_err();
            assert!(matches!(error, ExtractError::NotInes(_)), "{error}");
        }
        let mut short = b"NES\x1a\x02\x04\x31\x00".to_vec();
        short.resize(100, 0);
        assert!(matches!(identify(&short), Err(ExtractError::NotInes(_))));
    }
}
