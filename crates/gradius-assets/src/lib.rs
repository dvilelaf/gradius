//! Runtime reader for `gradius.pak`.
//!
//! The pack is produced once by `xtask extract` and is completely independent
//! of the cartridge afterwards. This crate performs no cartridge I/O and has no
//! knowledge of iNES.

use std::fs;
use std::path::Path;

use gradius_pack::{
    PackFormatError, CHUNK_AUDIO, CHUNK_META, CHUNK_PALETTES, CHUNK_ROM, CHUNK_STAGE, CHUNK_TILES,
    TILES_PER_BANK, TILE_BANKS, TILE_BYTES,
};
use thiserror::Error;

/// `CHUNK_STAGE` entry `$CE2B`: 88 encoded tile coordinates.
pub const STAGE_WALL_ERASE: &str = "stage6_wall_erase";
/// `CHUNK_STAGE` entry `$CF27`: eight pointers then the 19-byte text record.
pub const ENDING_TEXT_RECORD: &str = "ending_text_record";
/// `CHUNK_STAGE` entry `$CF48`: 28 ending-screen column/segment indices.
pub const ENDING_SCREEN_STREAM: &str = "ending_screen_stream";
/// `CHUNK_STAGE` entry `$CF90`: fifteen `$38`-byte ending tile-class blocks.
pub const ENDING_SCREEN_CLASSES: &str = "ending_screen_classes";
/// `CHUNK_STAGE` entry `$BB7C`: kind `$28` motion steps plus a `$FF` terminator.
pub const ENDING_SHIP_MOTION: &str = "ending_ship_motion";
/// `CHUNK_STAGE` entry `$9FD6`: eight tile-table pointers and eight
/// attribute-table pointers, as `$9D88` reads them.
pub const ENDING_CLASS_POINTERS: &str = "ending_class_pointers";
/// `CHUNK_STAGE` entry `$D6F2`: one attribute byte per tile class.
pub const ENDING_CLASS_ATTRIBUTES: &str = "ending_class_attributes";
/// `CHUNK_STAGE` entry `$D772`: 128 tile-run pointers then the runs.
pub const ENDING_CLASS_TILES: &str = "ending_class_tiles";
/// The `$CE2B` wall-erase coordinate table, in ROM order.
pub const STAGE_KIND_TILE_COORDS: u8 = 1;
/// A little-endian pointer table followed by the record it points at.
pub const STAGE_KIND_POINTER_RECORD: u8 = 2;
/// A byte stream indexed by the ending screen's `$55` segment counter.
pub const STAGE_KIND_INDEX_STREAM: u8 = 3;
/// Fixed-size blocks of tile-class bytes selected by an index stream.
pub const STAGE_KIND_INDEX_BLOCKS: u8 = 4;
/// Two-byte motion steps ending on a `$FF` terminator.
pub const STAGE_KIND_MOTION_TABLE: u8 = 5;
/// Eight tile pointers then eight attribute pointers (`$9FD6`).
pub const STAGE_KIND_CLASS_POINTERS: u8 = 6;
/// One attribute byte per tile class (`$D6F2`).
pub const STAGE_KIND_CLASS_ATTRIBUTES: u8 = 7;
/// A tile-run pointer table followed by zero-terminated runs.
pub const STAGE_KIND_CLASS_TILES: u8 = 8;
/// Bytes in the `$CE2B` table.
pub const WALL_ERASE_LEN: usize = 88;
/// Bytes in the `$CF27` region: eight pointers then the text record.
pub const ENDING_TEXT_RECORD_LEN: usize = 33;
/// Little-endian pointers at the head of `$CF27`.
pub const ENDING_POINTERS: usize = 8;
/// `$CF35` starts two bytes inside the last pointer (`$CF27 + 14`).
pub const ENDING_TEXT_OFFSET: usize = 14;
/// Bytes in the `$CF48` stream.
pub const ENDING_SCREEN_STREAM_LEN: usize = 28;
/// Stride of the `$9D49` offset table: the `$38`-byte blocks in `$CF90`.
pub const ENDING_SCREEN_BLOCK_BYTES: usize = 0x38;
/// Rows addressed by the fifteen `$9D49` offsets; the ending stream uses 0..9.
pub const ENDING_SCREEN_BLOCKS: usize = 15;
/// Bytes in `$CF90`: fifteen `$38`-byte blocks.
pub const ENDING_SCREEN_CLASSES_LEN: usize = ENDING_SCREEN_BLOCKS * ENDING_SCREEN_BLOCK_BYTES;
/// Two-byte motion steps in `$BB7C` before the terminator.
pub const ENDING_MOTION_STEPS: usize = 26;
/// Bytes in `$BB7C`: 26 pairs plus the `$FF` terminator.
pub const ENDING_MOTION_LEN: usize = ENDING_MOTION_STEPS * 2 + 1;
/// Pointers in each `$9FD6`/`$9FE6` half (one per stage, plus the terminator).
pub const ENDING_CLASS_POINTER_ENTRIES: usize = 8;
/// Bytes in `$9FD6..$9FF6`: eight tile pointers then eight attribute pointers.
pub const ENDING_CLASS_POINTERS_LEN: usize = 2 * 2 * ENDING_CLASS_POINTER_ENTRIES;
/// Classes addressed by the `$D772` pointer table and the `$D6F2` table.
pub const ENDING_TILE_CLASSES: usize = 128;
/// Bytes in `$D6F2`: one attribute byte per class.
pub const ENDING_CLASS_ATTRIBUTES_LEN: usize = ENDING_TILE_CLASSES;
/// The PRG address the `$D772` pointers are relative to.
pub const ENDING_CLASS_TILES_ADDR: u16 = 0xD772;
/// The `$D772` pointer table's own bytes.
pub const ENDING_CLASS_TILES_POINTERS_LEN: usize = ENDING_TILE_CLASSES * 2;
/// Bytes in `$D772..$DDB7`: the 128 pointers and their runs.
pub const ENDING_CLASS_TILES_LEN: usize = 0x645;

#[derive(Debug, Error)]
pub enum PackError {
    #[error("cannot read pack: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a Gradius pack (bad magic)")]
    Magic,
    #[error("unsupported pack version {0}")]
    Version(u16),
    #[error("truncated pack")]
    Truncated,
    #[error("missing {0} chunk")]
    MissingChunk(&'static str),
    #[error("malformed {0} chunk")]
    MalformedChunk(&'static str),
}

#[derive(Clone, Debug)]
pub struct Pack {
    pub tiles: Vec<[u8; TILE_BYTES]>,
    pub colors: Vec<[u8; 3]>,
    pub metadata: String,
    /// Raw payload of the audio chunk, decoded by `gradius-audio`. Version 2
    /// carries both the captured timelines and the native driver section.
    pub audio: Option<Vec<u8>>,
    /// The stage-6/ending PRG tables (`$CE2B`, `$CF27`, `$CF48`, `$CF90`,
    /// `$BB7C`, `$9FD6`, `$D6F2`, `$D772`).
    pub stage: Option<StageData>,
    /// The decoded `CHUNK_ROM`.
    pub rom: Option<gradius_core::rom::RomData>,
}

/// One named, typed table from the `CHUNK_STAGE` chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageTable {
    pub name: String,
    /// One of the `STAGE_KIND_*` tags.
    pub kind: u8,
    /// The PRG address the bytes were copied from, for provenance only.
    pub address: u16,
    pub bytes: Vec<u8>,
}

/// The stage tables carried by the pack.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StageData {
    pub tables: Vec<StageTable>,
}

impl StageData {
    pub fn table(&self, name: &str) -> Option<&StageTable> {
        self.tables.iter().find(|table| table.name == name)
    }

    /// `$CE2B`, the 88 encoded tile coordinates erased by `$CD9F`.
    pub fn wall_erase(&self) -> Option<&[u8]> {
        typed(
            self.table(STAGE_WALL_ERASE)?,
            STAGE_KIND_TILE_COORDS,
            WALL_ERASE_LEN,
        )
    }

    /// `$CF27`, eight little-endian pointers then the 19-byte text record.
    /// The record starts two bytes inside the last pointer, as in the ROM.
    pub fn ending_text_record(&self) -> Option<EndingTextRecord<'_>> {
        let table = self.table(ENDING_TEXT_RECORD)?;
        let bytes = typed(table, STAGE_KIND_POINTER_RECORD, ENDING_TEXT_RECORD_LEN)?;
        let mut pointers = [0u16; ENDING_POINTERS];
        for (pointer, pair) in pointers
            .iter_mut()
            .zip(bytes[..ENDING_POINTERS * 2].chunks_exact(2))
        {
            *pointer = u16::from_le_bytes([pair[0], pair[1]]);
        }
        Some(EndingTextRecord {
            pointers,
            text: &bytes[ENDING_TEXT_OFFSET..],
        })
    }

    /// `$CF48`, the 28-byte ending-screen segment stream read by `$9D88`.
    pub fn ending_screen_stream(&self) -> Option<&[u8]> {
        typed(
            self.table(ENDING_SCREEN_STREAM)?,
            STAGE_KIND_INDEX_STREAM,
            ENDING_SCREEN_STREAM_LEN,
        )
    }

    /// `$CF90`, fifteen `$38`-byte blocks of tile-class bytes.
    pub fn ending_screen_classes(&self) -> Option<&[u8]> {
        typed(
            self.table(ENDING_SCREEN_CLASSES)?,
            STAGE_KIND_INDEX_BLOCKS,
            ENDING_SCREEN_CLASSES_LEN,
        )
    }

    /// `$BB7C`, the kind `$28` ship-motion steps and their `$FF` terminator.
    pub fn ending_ship_motion(&self) -> Option<EndingShipMotion> {
        let bytes = typed(
            self.table(ENDING_SHIP_MOTION)?,
            STAGE_KIND_MOTION_TABLE,
            ENDING_MOTION_LEN,
        )?;
        let mut steps = [[0u8; 2]; ENDING_MOTION_STEPS];
        for (step, pair) in steps.iter_mut().zip(bytes.chunks_exact(2)) {
            *step = [pair[0], pair[1]];
        }
        Some(EndingShipMotion {
            steps,
            terminator: bytes[ENDING_MOTION_STEPS * 2],
        })
    }

    /// `$9FD6`/`$9FE6`: the per-stage tile- and attribute-table pointers.
    /// The ending runs at `$19 = 0`, so its class tables are entries 0.
    pub fn ending_class_pointers(&self) -> Option<EndingClassPointers> {
        let bytes = typed(
            self.table(ENDING_CLASS_POINTERS)?,
            STAGE_KIND_CLASS_POINTERS,
            ENDING_CLASS_POINTERS_LEN,
        )?;
        let mut tiles = [0u16; ENDING_CLASS_POINTER_ENTRIES];
        let mut attributes = [0u16; ENDING_CLASS_POINTER_ENTRIES];
        for (pointer, pair) in tiles.iter_mut().zip(bytes[..16].chunks_exact(2)) {
            *pointer = u16::from_le_bytes([pair[0], pair[1]]);
        }
        for (pointer, pair) in attributes.iter_mut().zip(bytes[16..].chunks_exact(2)) {
            *pointer = u16::from_le_bytes([pair[0], pair[1]]);
        }
        Some(EndingClassPointers { tiles, attributes })
    }

    /// `$D6F2`, one attribute byte per class (index = the class byte).
    pub fn ending_class_attributes(&self) -> Option<&[u8]> {
        typed(
            self.table(ENDING_CLASS_ATTRIBUTES)?,
            STAGE_KIND_CLASS_ATTRIBUTES,
            ENDING_CLASS_ATTRIBUTES_LEN,
        )
    }

    /// `$D772`, the class-to-tile-run pointer table and its runs.
    pub fn ending_class_tiles(&self) -> Option<EndingClassTiles<'_>> {
        let bytes = typed(
            self.table(ENDING_CLASS_TILES)?,
            STAGE_KIND_CLASS_TILES,
            ENDING_CLASS_TILES_LEN,
        )?;
        Some(EndingClassTiles { bytes })
    }
}

/// `$9FD6` and `$9FE6` split the way `$9D88` reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndingClassPointers {
    /// `$9FD6`: eight pointers to the per-class tile-run tables.
    pub tiles: [u16; ENDING_CLASS_POINTER_ENTRIES],
    /// `$9FE6`: eight pointers to the per-class attribute bytes.
    pub attributes: [u16; ENDING_CLASS_POINTER_ENTRIES],
}

/// `$D772`: 128 tile-run pointers followed by zero-terminated tile runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndingClassTiles<'a> {
    bytes: &'a [u8],
}

impl<'a> EndingClassTiles<'a> {
    /// The raw `$D772` bytes: the 128 pointers followed by their runs.
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The run for `class`: the bytes `$9D88` walks until its `$00` terminator.
    pub fn run(&self, class: u8) -> Option<&'a [u8]> {
        let index = usize::from(class) * 2;
        let pair = self.bytes.get(index..index + 2)?;
        let pointer = usize::from(u16::from_le_bytes([pair[0], pair[1]]));
        let start = pointer.checked_sub(usize::from(ENDING_CLASS_TILES_ADDR))?;
        if start < ENDING_CLASS_TILES_POINTERS_LEN || start >= self.bytes.len() {
            return None;
        }
        let rest = &self.bytes[start..];
        let end = rest.iter().position(|&byte| byte == 0)?;
        Some(&rest[..end])
    }
}

/// `$CF27` split the way `$CE8E` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndingTextRecord<'a> {
    /// The eight little-endian pointers; `$CE8E` clamps the loop to entry 6.
    pub pointers: [u16; ENDING_POINTERS],
    /// The 19-byte record the clamped pointer selects: the `$22C8` write
    /// address, the 16 text tiles and the `$FE` terminator.
    pub text: &'a [u8],
}

/// `$BB7C` split the way `$BB09` reads it.
///
/// Each step is `[x_delta, y_and_tile]`: the first byte is a signed X delta
/// added to `$0375`; the high nibble of the second is subtracted from
/// `$0335`, and the low nibble is the sprite tile `$96 + low_nibble`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndingShipMotion {
    pub steps: [[u8; 2]; ENDING_MOTION_STEPS],
    /// `$FF`: ends the walk, clears `$0135` and starts the explosion.
    pub terminator: u8,
}

fn typed(table: &StageTable, kind: u8, len: usize) -> Option<&[u8]> {
    (table.kind == kind && table.bytes.len() == len).then_some(table.bytes.as_slice())
}

impl Pack {
    pub fn load(path: &Path) -> Result<Self, PackError> {
        Self::parse(&fs::read(path)?)
    }

    pub fn parse(data: &[u8]) -> Result<Self, PackError> {
        let chunks = gradius_pack::read_chunks(data).map_err(|error| match error {
            PackFormatError::Magic => PackError::Magic,
            PackFormatError::Version(version) => PackError::Version(version),
            PackFormatError::Truncated => PackError::Truncated,
        })?;
        let mut tiles = None;
        let mut colors = None;
        let mut metadata = String::new();
        let mut audio = None;
        let mut stage = None;
        let mut rom = None;
        for (id, payload) in chunks {
            match id {
                CHUNK_TILES => tiles = Some(parse_tiles(payload)?),
                CHUNK_PALETTES => colors = Some(parse_palettes(payload)?),
                CHUNK_META => metadata = String::from_utf8_lossy(payload).into_owned(),
                CHUNK_AUDIO => audio = Some(payload.to_vec()),
                CHUNK_STAGE => stage = Some(parse_stage(payload)?),
                CHUNK_ROM => {
                    rom = Some(
                        gradius_core::rom::RomData::decode(payload)
                            .map_err(|_| PackError::MalformedChunk("rom"))?,
                    );
                }
                _ => {}
            }
        }
        Ok(Self {
            tiles: tiles.ok_or(PackError::MissingChunk("tiles"))?,
            colors: colors.ok_or(PackError::MissingChunk("palettes"))?,
            metadata,
            audio,
            stage,
            rom,
        })
    }

    /// Convert to the plain data the core consumes.
    ///
    /// A pack that predates the ending extraction yields `stage = None`; the
    /// `Assets` accessors then report the missing table instead of fabricating
    /// bytes.
    pub fn into_assets(self) -> gradius_core::Assets {
        let Pack {
            tiles,
            colors,
            stage,
            rom,
            ..
        } = self;
        let stage = stage
            .as_ref()
            .map(|stage| gradius_core::assets::StageAssets {
                ending_text_record: stage.ending_text_record().map(|record| {
                    let mut bytes = [0u8; 19];
                    bytes.copy_from_slice(record.text);
                    bytes
                }),
                ending_class_pointers: stage.ending_class_pointers().map(|pointers| {
                    gradius_core::assets::EndingClassPointers {
                        tiles: pointers.tiles,
                        attributes: pointers.attributes,
                    }
                }),
                ending_class_attributes: stage.ending_class_attributes().map(<[u8]>::to_vec),
                ending_class_tiles: stage
                    .ending_class_tiles()
                    .map(|tiles| tiles.as_bytes().to_vec()),
                wall_erase: stage.wall_erase().map(<[u8]>::to_vec),
                ending_ship_motion: stage
                    .ending_ship_motion()
                    .map(|motion| motion.steps.to_vec()),
            });
        gradius_core::Assets {
            tiles,
            colors,
            stage,
            rom: rom.map(gradius_core::rom::RomData::leak),
        }
    }
}

fn parse_tiles(payload: &[u8]) -> Result<Vec<[u8; TILE_BYTES]>, PackError> {
    if payload.len() < 4 {
        return Err(PackError::MalformedChunk("tiles"));
    }
    let banks = usize::from(u16::from_le_bytes([payload[0], payload[1]]));
    let per_bank = usize::from(u16::from_le_bytes([payload[2], payload[3]]));
    if per_bank != TILES_PER_BANK || banks != TILE_BANKS {
        return Err(PackError::MalformedChunk("tiles"));
    }
    let expected = 4 + banks * per_bank * TILE_BYTES;
    if payload.len() != expected {
        return Err(PackError::MalformedChunk("tiles"));
    }
    let mut tiles = Vec::with_capacity(banks * per_bank);
    for tile in payload[4..].chunks_exact(TILE_BYTES) {
        let mut pixels = [0u8; TILE_BYTES];
        pixels.copy_from_slice(tile);
        tiles.push(pixels);
    }
    Ok(tiles)
}

fn parse_palettes(payload: &[u8]) -> Result<Vec<[u8; 3]>, PackError> {
    let Some((&count, rest)) = payload.split_first() else {
        return Err(PackError::MalformedChunk("palettes"));
    };
    let count = usize::from(count);
    if rest.len() < count * 3 {
        return Err(PackError::MalformedChunk("palettes"));
    }
    Ok(rest[..count * 3]
        .chunks_exact(3)
        .map(|rgb| [rgb[0], rgb[1], rgb[2]])
        .collect())
}

fn parse_stage(payload: &[u8]) -> Result<StageData, PackError> {
    let malformed = || PackError::MalformedChunk("stage");
    let (&count, mut rest) = payload.split_first().ok_or_else(malformed)?;
    let mut tables = Vec::new();
    for _ in 0..count {
        let (&name_len, tail) = rest.split_first().ok_or_else(malformed)?;
        let name_len = usize::from(name_len);
        if tail.len() < name_len + 5 {
            return Err(malformed());
        }
        let name = String::from_utf8(tail[..name_len].to_vec()).map_err(|_| malformed())?;
        let kind = tail[name_len];
        let address = u16::from_le_bytes([tail[name_len + 1], tail[name_len + 2]]);
        let length = usize::from(u16::from_le_bytes([tail[name_len + 3], tail[name_len + 4]]));
        let start = name_len + 5;
        let end = start.checked_add(length).ok_or_else(malformed)?;
        let bytes = tail.get(start..end).ok_or_else(malformed)?.to_vec();
        tables.push(StageTable {
            name,
            kind,
            address,
            bytes,
        });
        rest = &tail[end..];
    }
    if !rest.is_empty() {
        return Err(malformed());
    }
    Ok(StageData { tables })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_core::assets::AssetError;
    use gradius_pack::PackBuilder;

    #[test]
    fn parses_a_generated_pack() {
        let bank = vec![[2u8; TILE_BYTES]; TILES_PER_BANK];
        let bytes = PackBuilder::new()
            .tiles(&vec![bank; TILE_BANKS])
            .palettes(&[[1, 2, 3]])
            .meta("ok")
            .finish();
        let pack = Pack::parse(&bytes).unwrap();
        assert_eq!(pack.tiles.len(), TILE_BANKS * TILES_PER_BANK);
        assert_eq!(pack.tiles[0], [2; 64]);
        assert_eq!(pack.colors, vec![[1, 2, 3]]);
        assert_eq!(pack.metadata, "ok");
        assert!(pack.audio.is_none());
    }

    #[test]
    fn keeps_the_audio_chunk_for_the_runtime() {
        let bank = vec![[2u8; TILE_BYTES]; TILES_PER_BANK];
        let bytes = PackBuilder::new()
            .tiles(&vec![bank; TILE_BANKS])
            .palettes(&[[1, 2, 3]])
            .chunk(CHUNK_AUDIO, b"GAUD\x01\x00\x00\x00".to_vec())
            .finish();
        let pack = Pack::parse(&bytes).unwrap();
        assert_eq!(
            pack.audio.as_deref(),
            Some(b"GAUD\x01\x00\x00\x00".as_slice())
        );
    }

    #[test]
    fn parses_the_stage_tables_from_the_pack() {
        // One table per shape the pack carries. The bytes are this test's
        // own ramp, not cartridge data: the parser only checks a table's
        // kind and length, and a ramp cannot pass as zero padding.
        let wall = ramp(WALL_ERASE_LEN, 1);
        let mut text_record = ramp(ENDING_TEXT_RECORD_LEN, 2);
        let pointers = [
            0x2340u16, 0x2341, 0x2342, 0x2343, 0x2344, 0x2345, 0x2346, 0x2347,
        ];
        for (index, pointer) in pointers.iter().enumerate() {
            text_record[2 * index..2 * index + 2].copy_from_slice(&pointer.to_le_bytes());
        }
        let stream = ramp(ENDING_SCREEN_STREAM_LEN, 3);
        let classes = ramp(ENDING_SCREEN_CLASSES_LEN, 4);
        let mut motion = ramp(ENDING_MOTION_LEN, 5);
        *motion.last_mut().unwrap() = 0xFF;
        let stage = pack_with_stage(stage_payload(&[
            (STAGE_WALL_ERASE, STAGE_KIND_TILE_COORDS, 0xCE2B, &wall),
            (
                ENDING_TEXT_RECORD,
                STAGE_KIND_POINTER_RECORD,
                0xCF27,
                &text_record,
            ),
            (
                ENDING_SCREEN_STREAM,
                STAGE_KIND_INDEX_STREAM,
                0xCF48,
                &stream,
            ),
            (
                ENDING_SCREEN_CLASSES,
                STAGE_KIND_INDEX_BLOCKS,
                0xCF90,
                &classes,
            ),
            (ENDING_SHIP_MOTION, STAGE_KIND_MOTION_TABLE, 0xBB7C, &motion),
        ]))
        .stage
        .unwrap();

        assert_eq!(stage.wall_erase(), Some(&wall[..]));
        let record = stage.ending_text_record().unwrap();
        assert_eq!(record.pointers, pointers);
        assert_eq!(record.text, &text_record[ENDING_TEXT_OFFSET..]);
        assert_eq!(record.text.len(), 19);

        assert_eq!(stage.ending_screen_stream(), Some(&stream[..]));
        let table = stage.ending_screen_classes().unwrap();
        assert_eq!(table.len(), ENDING_SCREEN_CLASSES_LEN);
        assert_eq!(table, &classes[..]);

        let motion_table = stage.ending_ship_motion().unwrap();
        assert_eq!(motion_table.steps.len(), ENDING_MOTION_STEPS);
        assert_eq!(motion_table.steps[0], [motion[0], motion[1]]);
        assert_eq!(
            motion_table.steps[ENDING_MOTION_STEPS - 1],
            [motion[50], motion[51]]
        );
        assert_eq!(motion_table.terminator, 0xFF);
    }

    #[test]
    fn resolves_the_ending_class_tables_from_the_pack() {
        // `$9FD6`: eight tile pointers then eight attribute pointers, entry 0
        // first in each half. Invented words: the parser only splits them.
        let words: [u16; 2 * ENDING_CLASS_POINTER_ENTRIES] =
            std::array::from_fn(|index| 0x8200 + index as u16);
        let mut pointer_table = [0u8; ENDING_CLASS_POINTERS_LEN];
        for (index, word) in words.iter().enumerate() {
            pointer_table[2 * index..2 * index + 2].copy_from_slice(&word.to_le_bytes());
        }

        // `$D772`: the 128-pointer table followed by the runs they address.
        const RUN_BASE: usize = 0x120;
        let mut tiles = [0u8; ENDING_CLASS_TILES_LEN];
        let runs: [&[u8]; 3] = [
            &[0x0A],
            &[0x0A, 0x0A, 0x14, 0x3C],
            &[0x0A, 0x0A, 0x3E, 0x0A],
        ];
        let mut next = RUN_BASE;
        for (class, bytes) in runs.iter().enumerate() {
            let pointer = ENDING_CLASS_TILES_ADDR + next as u16;
            tiles[2 * class..2 * class + 2].copy_from_slice(&pointer.to_le_bytes());
            tiles[next..next + bytes.len()].copy_from_slice(bytes);
            next += bytes.len() + 1;
        }

        // `$D6F2`: one attribute byte per class; only the first four matter.
        let mut attributes = [0u8; ENDING_CLASS_ATTRIBUTES_LEN];
        attributes[..4].copy_from_slice(&[0x00, 0x11, 0x22, 0x33]);

        let stage = pack_with_stage(stage_payload(&[
            (
                ENDING_CLASS_POINTERS,
                STAGE_KIND_CLASS_POINTERS,
                0x9FD6,
                &pointer_table,
            ),
            (
                ENDING_CLASS_ATTRIBUTES,
                STAGE_KIND_CLASS_ATTRIBUTES,
                0xD6F2,
                &attributes,
            ),
            (ENDING_CLASS_TILES, STAGE_KIND_CLASS_TILES, 0xD772, &tiles),
        ]))
        .stage
        .unwrap();

        let pointers = stage.ending_class_pointers().unwrap();
        assert_eq!(pointers.tiles[0], words[0]);
        assert_eq!(pointers.attributes[0], words[8]);
        assert_eq!(pointers.tiles, words[..8]);
        assert_eq!(pointers.attributes, words[8..]);
        assert_eq!(
            &stage.ending_class_attributes().unwrap()[..4],
            &attributes[..4]
        );
        let class_tiles = stage.ending_class_tiles().unwrap();
        assert_eq!(class_tiles.run(0), Some(runs[0]));
        assert_eq!(class_tiles.run(1), Some(runs[1]));
        assert_eq!(class_tiles.run(2), Some(runs[2]));
        // Classes without a written pointer point at the table's first byte.
        assert_eq!(class_tiles.run(3), None);
    }

    /// `len` bytes that are neither zero nor constant, so a zero-filled or
    /// truncated table cannot pass a length check by accident.
    fn ramp(len: usize, seed: u8) -> Vec<u8> {
        (0..len)
            .map(|index| seed.wrapping_add((index as u8).wrapping_mul(3)))
            .collect()
    }

    #[test]
    fn into_assets_carries_the_ending_stage_tables() {
        let mut record = [0u8; ENDING_TEXT_RECORD_LEN];
        for (index, byte) in record.iter_mut().enumerate() {
            *byte = index as u8;
        }
        let wall = [7u8; WALL_ERASE_LEN];
        let pointers = [9u8; ENDING_CLASS_POINTERS_LEN];
        let attributes = [3u8; ENDING_CLASS_ATTRIBUTES_LEN];
        let tiles = vec![5u8; ENDING_CLASS_TILES_LEN];
        let assets = pack_with_stage(stage_payload(&[
            (STAGE_WALL_ERASE, STAGE_KIND_TILE_COORDS, 0xCE2B, &wall),
            (
                ENDING_TEXT_RECORD,
                STAGE_KIND_POINTER_RECORD,
                0xCF27,
                &record,
            ),
            (
                ENDING_CLASS_POINTERS,
                STAGE_KIND_CLASS_POINTERS,
                0x9FD6,
                &pointers,
            ),
            (
                ENDING_CLASS_ATTRIBUTES,
                STAGE_KIND_CLASS_ATTRIBUTES,
                0xD6F2,
                &attributes,
            ),
            (ENDING_CLASS_TILES, STAGE_KIND_CLASS_TILES, 0xD772, &tiles),
        ]))
        .into_assets();

        let mut expected = [0u8; 19];
        expected.copy_from_slice(&record[ENDING_TEXT_OFFSET..]);
        assert_eq!(assets.ending_text_record().unwrap(), &expected);
        assert_eq!(assets.stage6_wall_erase().unwrap(), &wall);
        let pointers = assets.ending_class_pointers().unwrap();
        assert_eq!(pointers.tiles, [0x0909; ENDING_CLASS_POINTER_ENTRIES]);
        assert_eq!(pointers.attributes, [0x0909; ENDING_CLASS_POINTER_ENTRIES]);
        assert_eq!(assets.ending_class_attributes().unwrap(), &attributes);
        assert_eq!(assets.ending_class_tiles().unwrap(), &tiles);
    }

    #[test]
    fn into_assets_reports_a_pack_without_the_ending_tables() {
        let bank = vec![[2u8; TILE_BYTES]; TILES_PER_BANK];
        let assets = Pack::parse(
            &PackBuilder::new()
                .tiles(&vec![bank; TILE_BANKS])
                .palettes(&[[1, 2, 3]])
                .finish(),
        )
        .unwrap()
        .into_assets();

        assert_eq!(
            assets.ending_text_record(),
            Err(AssetError::Missing("CHUNK_STAGE"))
        );
        assert_eq!(
            assets.ending_class_pointers(),
            Err(AssetError::Missing("CHUNK_STAGE"))
        );
        assert_eq!(
            assets.ending_class_attributes(),
            Err(AssetError::Missing("CHUNK_STAGE"))
        );
        assert_eq!(
            assets.ending_class_tiles(),
            Err(AssetError::Missing("CHUNK_STAGE"))
        );
        assert_eq!(
            assets.stage6_wall_erase(),
            Err(AssetError::Missing("CHUNK_STAGE"))
        );
        assert_eq!(
            assets.ending_text_record().unwrap_err().to_string(),
            "pack does not carry the CHUNK_STAGE stage table"
        );
    }

    #[test]
    fn into_assets_reports_the_single_table_a_partial_pack_omits() {
        let wall = [7u8; WALL_ERASE_LEN];
        let assets = pack_with_stage(stage_payload(&[(
            STAGE_WALL_ERASE,
            STAGE_KIND_TILE_COORDS,
            0xCE2B,
            &wall,
        )]))
        .into_assets();

        assert_eq!(assets.stage6_wall_erase(), Ok(&wall[..]));
        assert_eq!(
            assets.ending_text_record(),
            Err(AssetError::Missing("ending_text_record"))
        );
    }

    fn pack_with_chunks(rom: Option<Vec<u8>>) -> Vec<u8> {
        let bank = vec![[2u8; TILE_BYTES]; TILES_PER_BANK];
        let mut builder = PackBuilder::new()
            .tiles(&vec![bank; TILE_BANKS])
            .palettes(&[[1, 2, 3]]);
        if let Some(rom) = rom {
            builder = builder.chunk(CHUNK_ROM, rom);
        }
        builder.finish()
    }

    #[test]
    fn into_assets_decodes_the_rom_chunk() {
        let rom = gradius_core::rom::RomData::default();
        let pack = Pack::parse(&pack_with_chunks(Some(rom.encode()))).unwrap();
        assert_eq!(*pack.into_assets().rom().unwrap(), rom);
    }

    #[test]
    fn into_assets_reports_a_pack_without_the_rom_chunk() {
        let pack = Pack::parse(&pack_with_chunks(None)).unwrap();
        assert_eq!(
            pack.into_assets().rom().unwrap_err(),
            gradius_core::assets::AssetError::Missing("CHUNK_ROM")
        );
    }

    #[test]
    fn a_malformed_rom_chunk_is_rejected() {
        assert!(matches!(
            Pack::parse(&pack_with_chunks(Some(b"junk".to_vec()))),
            Err(PackError::MalformedChunk("rom"))
        ));
    }

    fn pack_with_stage(payload: Vec<u8>) -> Pack {
        let bank = vec![[2u8; TILE_BYTES]; TILES_PER_BANK];
        Pack::parse(
            &PackBuilder::new()
                .tiles(&vec![bank; TILE_BANKS])
                .palettes(&[[1, 2, 3]])
                .chunk(CHUNK_STAGE, payload)
                .finish(),
        )
        .unwrap()
    }

    fn stage_payload(tables: &[(&str, u8, u16, &[u8])]) -> Vec<u8> {
        let mut payload = vec![tables.len() as u8];
        for (name, kind, address, bytes) in tables {
            payload.push(name.len() as u8);
            payload.extend_from_slice(name.as_bytes());
            payload.push(*kind);
            payload.extend_from_slice(&address.to_le_bytes());
            payload.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            payload.extend_from_slice(bytes);
        }
        payload
    }

    #[test]
    fn rejects_foreign_files() {
        assert!(matches!(
            Pack::parse(b"not a pack at all"),
            Err(PackError::Magic)
        ));
    }
}
