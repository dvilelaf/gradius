//! Data the game needs, in a form that has nothing to do with the cartridge.
//!
//! `gradius-assets` decodes the pack into these plain structs; the frontend
//! hands them to [`crate::Game::new`]. No file handles or ROM bytes leak in.

use std::fmt;

use crate::rom::RomData;

/// Indexed 8x8 tiles, laid out as `bank * 256 + index`.
#[derive(Clone, Debug, Default)]
pub struct Assets {
    pub tiles: Vec<[u8; 64]>,
    /// RGB colors, indexed by NES master-palette index.
    pub colors: Vec<[u8; 3]>,
    /// The `CHUNK_STAGE` tables copied from the pack. `None` on a pack that
    /// predates the extraction; the accessors then return [`AssetError`].
    pub stage: Option<StageAssets>,
    /// The `CHUNK_ROM` data, kept for the whole process. `None` on a pack
    /// without it; [`Assets::rom`] then reports it.
    pub rom: Option<&'static RomData>,
}

/// The stage tables the runtime reads, copied out of the pack by
/// `gradius-assets` when the pack carries them.
#[derive(Clone, Debug, Default)]
pub struct StageAssets {
    /// `$CF35`: the `$22C8` address, the 16 text tiles and the `$FE` end.
    pub ending_text_record: Option<[u8; 19]>,
    /// `$9FD6`/`$9FE6`: the per-stage tile- and attribute-table pointers.
    pub ending_class_pointers: Option<EndingClassPointers>,
    /// `$D6F2`: one attribute byte per tile class.
    pub ending_class_attributes: Option<Vec<u8>>,
    /// `$D772`: 128 tile-run pointers followed by their runs.
    pub ending_class_tiles: Option<Vec<u8>>,
    /// `$CE2B`: the 88 encoded coordinates stage 6's `$CD9F` erases.
    pub wall_erase: Option<Vec<u8>>,
    /// `$BB7C`: the kind-`$28` ending ship's `[x_delta, y_and_tile]` steps.
    pub ending_ship_motion: Option<Vec<[u8; 2]>>,
}

/// `$9FD6` and `$9FE6` split the way `$9D88` reads them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndingClassPointers {
    /// `$9FD6`: eight pointers to the per-class tile-run tables.
    pub tiles: [u16; 8],
    /// `$9FE6`: eight pointers to the per-class attribute bytes.
    pub attributes: [u16; 8],
}

/// A pack that does not carry a table the runtime needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetError {
    /// The named chunk or `CHUNK_STAGE` table is absent (an older pack).
    Missing(&'static str),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(name) => write!(f, "pack does not carry the {name} stage table"),
        }
    }
}

impl std::error::Error for AssetError {}

impl Assets {
    pub fn tile(&self, bank: u8, index: u8) -> Option<&[u8; 64]> {
        self.tiles.get(usize::from(bank) * 256 + usize::from(index))
    }

    pub fn color(&self, index: u8) -> [u8; 3] {
        self.colors
            .get(usize::from(index & 0x3F))
            .copied()
            .unwrap_or([0xFF, 0x00, 0xFF])
    }

    pub fn rom(&self) -> Result<&'static RomData, AssetError> {
        self.rom.ok_or(AssetError::Missing("CHUNK_ROM"))
    }

    /// The `TABL` tables from the pack. Rendering already requires the ROM
    /// data, so a missing pack is the fatal error [`Assets::rom`] reports.
    pub fn tables(&self) -> &'static crate::rom::Tables {
        &self
            .rom
            .expect("the pack has no ROM data; run scripts/extract-data")
            .tables
    }

    fn stage(&self) -> Result<&StageAssets, AssetError> {
        self.stage
            .as_ref()
            .ok_or(AssetError::Missing("CHUNK_STAGE"))
    }

    /// `$CF35`: the `$22C8` address, the 16 text tiles and the `$FE` end.
    pub fn ending_text_record(&self) -> Result<&[u8; 19], AssetError> {
        self.stage()?
            .ending_text_record
            .as_ref()
            .ok_or(AssetError::Missing("ending_text_record"))
    }

    /// `$9FD6`/`$9FE6`: the per-stage tile- and attribute-table pointers.
    pub fn ending_class_pointers(&self) -> Result<&EndingClassPointers, AssetError> {
        self.stage()?
            .ending_class_pointers
            .as_ref()
            .ok_or(AssetError::Missing("ending_class_pointers"))
    }

    /// `$D6F2`: one attribute byte per tile class.
    pub fn ending_class_attributes(&self) -> Result<&[u8], AssetError> {
        self.stage()?
            .ending_class_attributes
            .as_deref()
            .ok_or(AssetError::Missing("ending_class_attributes"))
    }

    /// `$D772`: the class-to-tile-run pointer table and its runs.
    pub fn ending_class_tiles(&self) -> Result<&[u8], AssetError> {
        self.stage()?
            .ending_class_tiles
            .as_deref()
            .ok_or(AssetError::Missing("ending_class_tiles"))
    }

    /// `$CE2B`: the 88 encoded coordinates stage 6's `$CD9F` erases.
    pub fn stage6_wall_erase(&self) -> Result<&[u8], AssetError> {
        self.stage()?
            .wall_erase
            .as_deref()
            .ok_or(AssetError::Missing("stage6_wall_erase"))
    }

    /// `$BB7C`: the ending ship's walk, one `[x_delta, y_and_tile]` per step.
    pub fn ending_ship_motion(&self) -> Result<&[[u8; 2]], AssetError> {
        self.stage()?
            .ending_ship_motion
            .as_deref()
            .ok_or(AssetError::Missing("ending_ship_motion"))
    }
}

/// A tile in a specific CHR bank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRef {
    pub bank: u8,
    pub index: u8,
}

/// The two `CHUNK_STAGE` tables the core reads, parsed from the extracted pack
/// for tests (`gradius-assets` does this for the frontends).
#[cfg(test)]
pub(crate) fn pack_stage_assets() -> StageAssets {
    let path = std::env::var_os("GRADIUS_PACK").map_or_else(
        || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak"),
        std::path::PathBuf::from,
    );
    let payload = gradius_pack::read_chunk_from_file(&path, gradius_pack::CHUNK_STAGE)
        .expect("no extracted pack: run GRADIUS_ROM=<rom> scripts/extract-data");
    let mut stage = StageAssets::default();
    let (&count, mut rest) = payload.split_first().unwrap();
    for _ in 0..count {
        let name_len = usize::from(rest[0]);
        let name = std::str::from_utf8(&rest[1..1 + name_len]).unwrap();
        let length = usize::from(u16::from_le_bytes([rest[name_len + 4], rest[name_len + 5]]));
        let bytes = &rest[name_len + 6..name_len + 6 + length];
        match name {
            "stage6_wall_erase" => stage.wall_erase = Some(bytes.to_vec()),
            "ending_ship_motion" => {
                stage.ending_ship_motion = Some(
                    bytes
                        .chunks_exact(2)
                        .map(|pair| [pair[0], pair[1]])
                        .collect(),
                );
            }
            _ => {}
        }
        rest = &rest[name_len + 6 + length..];
    }
    stage
}
