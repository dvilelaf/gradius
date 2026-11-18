//! The stage-6 and ending tables that are not tiles, palette or audio.
//!
//! `extract` copies them out of PRG into the `CHUNK_STAGE` chunk, so the pack
//! keeps only the measured ROM bytes and no cartridge address to follow.
//!
//! * `$CE2B`, 88 bytes: the `$CDAD` encoded tile coordinates the stage-6 wall
//!   erase consumes two at a time (`$66` runs 0..88).
//! * `$CF27`, 33 bytes: eight `$CE8E` pointers and the 19-byte text record
//!   they select (`$CF35..$CF48`: the `$22C8` write address, 16 tiles and the
//!   `$FE` terminator). `$CE8E` clamps the loop to entry 6, so the eighth
//!   pointer is copied but never taken.
//! * `$CF48`, 28 bytes: `$9D88` reads `$CF48[$55]` as a segment index. Each
//!   byte selects one block of the `$CF90` table: `$9D49` holds offsets
//!   `0, $38, $70, ...` so blocks are `$38` bytes. The ending build (`$8B`,
//!   7029-7051) only reaches `$55 = 0` and `1`, and `$CF48[0] = $CF48[1] = 0`,
//!   so the ending uses block 0 only. The pack still carries all fifteen rows
//!   that `$9D49` addresses, because stage 1 scroll reuses the same table.
//! * `$CF90`, 840 bytes: fifteen `$38`-byte blocks of tile-class bytes, indexed
//!   by the column address `$AE` that `$9D88` computes from `$58`. The class
//!   is resolved through the per-stage `$9FD6`/`$9FE6` tables.
//! * `$9FD6`, 32 bytes: eight tile-table pointers (`$9FD6`) then eight
//!   attribute-table pointers (`$9FE6`). The ending runs at `$19 = 0`, so
//!   entry 0 selects `$D772` (tiles) and `$D6F2` (attributes).
//! * `$D6F2`, 128 bytes: one attribute byte per class.
//! * `$D772`, 1605 bytes: 128 little-endian tile-run pointers, then the
//!   zero-terminated tile runs they address. `$9D88` reads the pointer at
//!   `$D772 + 2*class` and walks the run it names.
//! * `$BB7C`, 53 bytes: the kind `$28` motion table read by `$BB09` through
//!   `$0475`. It is 26 two-byte steps plus the `$FF` terminator (the plan's
//!   "38 bytes / 19 entries" predates this measurement). Step byte 0 is a
//!   signed X delta; the high nibble of byte 1 is the Y subtraction and the
//!   low nibble is the sprite tile `$96 + nibble`. `$FF` clears `$0135` and
//!   starts the explosion.
//!
//! Nothing here invents a byte: every slice is read straight from PRG.

use anyhow::{bail, Result};

use gradius_assets::{
    ENDING_CLASS_ATTRIBUTES, ENDING_CLASS_ATTRIBUTES_LEN, ENDING_CLASS_POINTERS,
    ENDING_CLASS_POINTERS_LEN, ENDING_CLASS_POINTER_ENTRIES, ENDING_CLASS_TILES,
    ENDING_CLASS_TILES_ADDR, ENDING_CLASS_TILES_LEN, ENDING_CLASS_TILES_POINTERS_LEN,
    ENDING_MOTION_LEN, ENDING_MOTION_STEPS, ENDING_SCREEN_BLOCKS, ENDING_SCREEN_CLASSES,
    ENDING_SCREEN_CLASSES_LEN, ENDING_SCREEN_STREAM, ENDING_SCREEN_STREAM_LEN, ENDING_SHIP_MOTION,
    ENDING_TEXT_RECORD, ENDING_TILE_CLASSES, STAGE_KIND_CLASS_ATTRIBUTES,
    STAGE_KIND_CLASS_POINTERS, STAGE_KIND_CLASS_TILES, STAGE_KIND_INDEX_BLOCKS,
    STAGE_KIND_INDEX_STREAM, STAGE_KIND_MOTION_TABLE, STAGE_KIND_POINTER_RECORD,
    STAGE_KIND_TILE_COORDS, STAGE_WALL_ERASE,
};

/// `$CE2B`: the stage-6 wall-erase coordinate table.
pub const WALL_ERASE_ADDR: u16 = 0xCE2B;
pub const WALL_ERASE_LEN: usize = gradius_assets::WALL_ERASE_LEN;
/// `$CF27`: eight little-endian pointers (`$CF27..$CF37`), then the 19-byte
/// text record `$CE8E` reads (`$CF35..$CF48`, ending on `$FE`). The record
/// starts two bytes inside the last pointer, exactly as the ROM stores it.
pub const ENDING_TEXT_RECORD_ADDR: u16 = 0xCF27;
pub const ENDING_TEXT_RECORD_LEN: usize = gradius_assets::ENDING_TEXT_RECORD_LEN;
/// `$FE`: the text record terminator at `$CF47`.
pub const TEXT_END: u8 = 0xFE;
/// `$CF48`: the ending-screen segment stream.
pub const ENDING_SCREEN_STREAM_ADDR: u16 = 0xCF48;
/// `$CF90`: the fifteen ending tile-class blocks.
pub const ENDING_SCREEN_CLASSES_ADDR: u16 = 0xCF90;
/// `$BB7C`: the kind `$28` ship-motion steps.
pub const ENDING_MOTION_ADDR: u16 = 0xBB7C;
/// `$FF`: the motion terminator.
pub const MOTION_END: u8 = 0xFF;
/// `$9FD6`: eight stage tile-table pointers then eight attribute pointers.
pub const ENDING_CLASS_POINTERS_ADDR: u16 = 0x9FD6;
/// `$D6F2`: one attribute byte per tile class.
pub const ENDING_CLASS_ATTRIBUTES_ADDR: u16 = 0xD6F2;

/// One named ROM table, ready for the `CHUNK_STAGE` payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageTable {
    pub name: &'static str,
    pub kind: u8,
    pub address: u16,
    pub bytes: Vec<u8>,
}

/// The two extracted tables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageData {
    pub tables: Vec<StageTable>,
}

impl StageData {
    pub fn wall_erase(&self) -> &[u8] {
        self.table(STAGE_WALL_ERASE).bytes.as_slice()
    }

    pub fn ending_text_record(&self) -> &[u8] {
        self.table(ENDING_TEXT_RECORD).bytes.as_slice()
    }

    pub fn ending_screen_stream(&self) -> &[u8] {
        self.table(ENDING_SCREEN_STREAM).bytes.as_slice()
    }

    pub fn ending_screen_classes(&self) -> &[u8] {
        self.table(ENDING_SCREEN_CLASSES).bytes.as_slice()
    }

    pub fn ending_motion(&self) -> &[u8] {
        self.table(ENDING_SHIP_MOTION).bytes.as_slice()
    }

    pub fn ending_class_pointers(&self) -> &[u8] {
        self.table(ENDING_CLASS_POINTERS).bytes.as_slice()
    }

    pub fn ending_class_attributes(&self) -> &[u8] {
        self.table(ENDING_CLASS_ATTRIBUTES).bytes.as_slice()
    }

    pub fn ending_class_tiles(&self) -> &[u8] {
        self.table(ENDING_CLASS_TILES).bytes.as_slice()
    }

    fn table(&self, name: &str) -> &StageTable {
        self.tables
            .iter()
            .find(|table| table.name == name)
            .expect("extracted table is present")
    }
}

/// Read all five tables from a 32 KiB PRG image.
pub fn decode(prg: &[u8]) -> Result<StageData> {
    let wall = slice(prg, WALL_ERASE_ADDR, WALL_ERASE_LEN)?;
    let text = slice(prg, ENDING_TEXT_RECORD_ADDR, ENDING_TEXT_RECORD_LEN)?;
    if text.last() != Some(&TEXT_END) {
        bail!(
            "the $CF27 text record does not end with $FE; the measured 33-byte layout does not fit this ROM"
        );
    }
    let stream = slice(prg, ENDING_SCREEN_STREAM_ADDR, ENDING_SCREEN_STREAM_LEN)?;
    if stream
        .iter()
        .any(|&index| usize::from(index) >= ENDING_SCREEN_BLOCKS)
    {
        bail!("a $CF48 index is outside the ten packed $CF90 blocks");
    }
    let classes = slice(prg, ENDING_SCREEN_CLASSES_ADDR, ENDING_SCREEN_CLASSES_LEN)?;
    let motion = slice(prg, ENDING_MOTION_ADDR, ENDING_MOTION_LEN)?;
    if motion[ENDING_MOTION_STEPS * 2] != MOTION_END
        || motion[..ENDING_MOTION_STEPS * 2].contains(&MOTION_END)
    {
        bail!("the $BB7C motion table is not 26 steps followed by $FF");
    }
    let class_pointers = slice(prg, ENDING_CLASS_POINTERS_ADDR, ENDING_CLASS_POINTERS_LEN)?;
    let tile_table = word(&class_pointers, 0);
    let attribute_table = word(&class_pointers, ENDING_CLASS_POINTER_ENTRIES * 2);
    if tile_table != ENDING_CLASS_TILES_ADDR || attribute_table != ENDING_CLASS_ATTRIBUTES_ADDR {
        bail!(
            "$9FD6 entry 0 is ${tile_table:04X} and $9FE6 entry 0 is ${attribute_table:04X}; \
             the ending class tables are not at $D772/$D6F2"
        );
    }
    let class_attributes = slice(
        prg,
        ENDING_CLASS_ATTRIBUTES_ADDR,
        ENDING_CLASS_ATTRIBUTES_LEN,
    )?;
    let class_tiles = slice(prg, ENDING_CLASS_TILES_ADDR, ENDING_CLASS_TILES_LEN)?;
    validate_class_tiles(&class_tiles)?;
    Ok(StageData {
        tables: vec![
            StageTable {
                name: STAGE_WALL_ERASE,
                kind: STAGE_KIND_TILE_COORDS,
                address: WALL_ERASE_ADDR,
                bytes: wall,
            },
            StageTable {
                name: ENDING_TEXT_RECORD,
                kind: STAGE_KIND_POINTER_RECORD,
                address: ENDING_TEXT_RECORD_ADDR,
                bytes: text,
            },
            StageTable {
                name: ENDING_SCREEN_STREAM,
                kind: STAGE_KIND_INDEX_STREAM,
                address: ENDING_SCREEN_STREAM_ADDR,
                bytes: stream,
            },
            StageTable {
                name: ENDING_SCREEN_CLASSES,
                kind: STAGE_KIND_INDEX_BLOCKS,
                address: ENDING_SCREEN_CLASSES_ADDR,
                bytes: classes,
            },
            StageTable {
                name: ENDING_SHIP_MOTION,
                kind: STAGE_KIND_MOTION_TABLE,
                address: ENDING_MOTION_ADDR,
                bytes: motion,
            },
            StageTable {
                name: ENDING_CLASS_POINTERS,
                kind: STAGE_KIND_CLASS_POINTERS,
                address: ENDING_CLASS_POINTERS_ADDR,
                bytes: class_pointers,
            },
            StageTable {
                name: ENDING_CLASS_ATTRIBUTES,
                kind: STAGE_KIND_CLASS_ATTRIBUTES,
                address: ENDING_CLASS_ATTRIBUTES_ADDR,
                bytes: class_attributes,
            },
            StageTable {
                name: ENDING_CLASS_TILES,
                kind: STAGE_KIND_CLASS_TILES,
                address: ENDING_CLASS_TILES_ADDR,
                bytes: class_tiles,
            },
        ],
    })
}

fn word(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

/// Every class pointer must address a run inside the packed region, and the
/// region must end exactly at the last run's terminator.
fn validate_class_tiles(bytes: &[u8]) -> Result<()> {
    let base = usize::from(ENDING_CLASS_TILES_ADDR);
    let mut furthest = 0;
    for class in 0..ENDING_TILE_CLASSES {
        let pointer = usize::from(word(bytes, class * 2));
        let start = pointer
            .checked_sub(base)
            .filter(|start| *start >= ENDING_CLASS_TILES_POINTERS_LEN && *start < bytes.len())
            .ok_or_else(|| anyhow::anyhow!("$D772 class {class} points at ${pointer:04X}"))?;
        let end = bytes[start..]
            .iter()
            .position(|&byte| byte == 0)
            .ok_or_else(|| anyhow::anyhow!("$D772 class {class} run has no $00 terminator"))?;
        furthest = furthest.max(start + end + 1);
    }
    if furthest != bytes.len() {
        bail!(
            "the $D772 runs end at ${:04X}, not at the packed region end ${:04X}",
            base + furthest,
            base + bytes.len()
        );
    }
    Ok(())
}

/// Encode the `CHUNK_STAGE` payload: a count, then
/// `u8 name_len, name, u8 kind, u16 address, u16 len, bytes` per entry.
pub fn encode(data: &StageData) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.push(u8::try_from(data.tables.len()).expect("too many stage tables"));
    for table in &data.tables {
        payload.push(u8::try_from(table.name.len()).expect("table name too long"));
        payload.extend_from_slice(table.name.as_bytes());
        payload.push(table.kind);
        payload.extend_from_slice(&table.address.to_le_bytes());
        payload.extend_from_slice(&(table.bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(&table.bytes);
    }
    payload
}

fn slice(prg: &[u8], address: u16, len: usize) -> Result<Vec<u8>> {
    let start = usize::from(address.wrapping_sub(0x8000));
    prg.get(start..start + len)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| anyhow::anyhow!("${address:04X}+{len} is outside the 32 KiB PRG image"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PRG image where every extracted byte has a distinct nonzero value.
    fn synthetic() -> Vec<u8> {
        let mut prg: Vec<u8> = (0..0x8000).map(|offset| (offset % 251) as u8 + 1).collect();
        let record = usize::from(ENDING_TEXT_RECORD_ADDR - 0x8000);
        prg[record + ENDING_TEXT_RECORD_LEN - 1] = TEXT_END;
        let stream = usize::from(ENDING_SCREEN_STREAM_ADDR - 0x8000);
        for (index, byte) in prg[stream..stream + ENDING_SCREEN_STREAM_LEN]
            .iter_mut()
            .enumerate()
        {
            *byte = (index % ENDING_SCREEN_BLOCKS) as u8;
        }
        let motion = usize::from(ENDING_MOTION_ADDR - 0x8000);
        prg[motion + ENDING_MOTION_STEPS * 2] = MOTION_END;
        // `$9FD6`: invented tile and attribute pointer words, well-formed for
        // this image. The first tile pointers address the runs written below.
        const FIRST_RUNS: [usize; 3] = [0x140, 0x144, 0x14B];
        let mut class_pointers = [0u8; ENDING_CLASS_POINTERS_LEN];
        for class in 0..ENDING_CLASS_POINTER_ENTRIES {
            // Entry 0 of each half must name the table the decoder checks.
            let pointer = if class == 0 {
                ENDING_CLASS_TILES_ADDR
            } else if let Some(offset) = FIRST_RUNS.get(class) {
                ENDING_CLASS_TILES_ADDR + *offset as u16
            } else {
                ENDING_CLASS_TILES_ADDR + *FIRST_RUNS.last().unwrap() as u16
            };
            class_pointers[2 * class..2 * class + 2].copy_from_slice(&pointer.to_le_bytes());
            let word = if class == 0 {
                ENDING_CLASS_ATTRIBUTES_ADDR
            } else {
                0x8400 + 2 * class as u16
            };
            class_pointers[16 + 2 * class..18 + 2 * class].copy_from_slice(&word.to_le_bytes());
        }
        let pointers = usize::from(ENDING_CLASS_POINTERS_ADDR - 0x8000);
        prg[pointers..pointers + ENDING_CLASS_POINTERS_LEN].copy_from_slice(&class_pointers);
        // `$D6F2`: invented leading attributes; the rest is sentinel pattern.
        let attributes = usize::from(ENDING_CLASS_ATTRIBUTES_ADDR - 0x8000);
        prg[attributes..attributes + 4].copy_from_slice(&[0x04, 0x05, 0x06, 0x07]);
        // `$D772`: a valid pointer table. Classes 0..2 address the invented
        // runs written below; the rest point at distinct zero bytes and class
        // 127 at the last byte so the region ends exactly at its terminator.
        let tiles = usize::from(ENDING_CLASS_TILES_ADDR - 0x8000);
        prg[tiles..tiles + ENDING_CLASS_TILES_LEN].fill(0);
        let first_tile_pointers: Vec<u16> = FIRST_RUNS
            .iter()
            .map(|offset| ENDING_CLASS_TILES_ADDR + *offset as u16)
            .chain(std::iter::once(ENDING_CLASS_TILES_ADDR + 0x14B))
            .collect();
        for class in 0..ENDING_TILE_CLASSES {
            let pointer = if let Some(pointer) = first_tile_pointers.get(class) {
                *pointer
            } else if class == ENDING_TILE_CLASSES - 1 {
                ENDING_CLASS_TILES_ADDR + (ENDING_CLASS_TILES_LEN as u16 - 1)
            } else {
                ENDING_CLASS_TILES_ADDR + ENDING_CLASS_TILES_POINTERS_LEN as u16 + class as u16
            };
            prg[tiles + 2 * class..tiles + 2 * class + 2].copy_from_slice(&pointer.to_le_bytes());
        }
        // The invented runs the first class pointers address.
        prg[tiles + 0x140] = 0x0A;
        prg[tiles + 0x144..tiles + 0x148].copy_from_slice(&[0x0A, 0x0A, 0x14, 0x3C]);
        prg[tiles + 0x14B..tiles + 0x14F].copy_from_slice(&[0x0A, 0x0A, 0x3E, 0x0A]);
        prg
    }

    #[test]
    fn decode_reads_the_measured_addresses_and_lengths() {
        let prg = synthetic();
        let data = decode(&prg).unwrap();
        assert_eq!(data.wall_erase().len(), 88);
        assert_eq!(data.ending_text_record().len(), 33);
        assert_eq!(data.ending_screen_stream().len(), 28);
        assert_eq!(data.ending_screen_classes().len(), 840);
        assert_eq!(data.ending_motion().len(), 53);
        assert_eq!(
            data.ending_class_pointers().len(),
            ENDING_CLASS_POINTERS_LEN
        );
        assert_eq!(
            data.ending_class_attributes().len(),
            ENDING_CLASS_ATTRIBUTES_LEN
        );
        assert_eq!(data.ending_class_tiles().len(), ENDING_CLASS_TILES_LEN);
        assert_eq!(data.wall_erase(), &prg[0x4E2B..0x4E2B + 88]);
        assert_eq!(data.ending_text_record(), &prg[0x4F27..0x4F27 + 33]);
        assert_eq!(data.ending_screen_stream(), &prg[0x4F48..0x4F48 + 28]);
        assert_eq!(data.ending_screen_classes(), &prg[0x4F90..0x4F90 + 840]);
        assert_eq!(data.ending_motion(), &prg[0x3B7C..0x3B7C + 53]);
        // The pointer words the sentinel pattern left alone.
        assert_eq!(
            &data.ending_class_pointers()[..2],
            &ENDING_CLASS_TILES_ADDR.to_le_bytes()
        );
        assert_eq!(
            &data.ending_class_pointers()[16..18],
            &ENDING_CLASS_ATTRIBUTES_ADDR.to_le_bytes()
        );
        assert_eq!(
            &data.ending_class_tiles()[..8],
            &[0xB2, 0xD8, 0xB6, 0xD8, 0xBD, 0xD8, 0xBD, 0xD8]
        );
        assert_eq!(
            &data.ending_class_attributes()[..4],
            &[0x04, 0x05, 0x06, 0x07]
        );
        assert_eq!(data.ending_text_record()[32], TEXT_END);
        assert_eq!(data.ending_motion()[52], MOTION_END);
        assert_eq!(data.tables[0].address, 0xCE2B);
        assert_eq!(data.tables[1].name, "ending_text_record");
        assert_eq!(data.tables[2].name, "ending_screen_stream");
        assert_eq!(data.tables[3].name, "ending_screen_classes");
        assert_eq!(data.tables[4].name, "ending_ship_motion");
        assert_eq!(data.tables[5].name, "ending_class_pointers");
        assert_eq!(data.tables[6].name, "ending_class_attributes");
        assert_eq!(data.tables[7].name, "ending_class_tiles");
        assert_eq!(data.tables[5].address, ENDING_CLASS_POINTERS_ADDR);
        assert_eq!(data.tables[7].address, ENDING_CLASS_TILES_ADDR);
        // Not zero-filled: the first bytes are the sentinel pattern.
        assert_ne!(data.wall_erase()[..4], [0, 0, 0, 0]);
        assert_ne!(data.ending_screen_classes()[..4], [0, 0, 0, 0]);
    }

    #[test]
    fn decode_rejects_a_text_record_without_its_terminator() {
        let mut prg = synthetic();
        let end = usize::from(ENDING_TEXT_RECORD_ADDR - 0x8000) + ENDING_TEXT_RECORD_LEN - 1;
        prg[end] = 0x00;
        assert!(decode(&prg).is_err());
    }

    #[test]
    fn decode_rejects_a_stream_index_outside_the_packed_blocks() {
        let mut prg = synthetic();
        let stream = usize::from(ENDING_SCREEN_STREAM_ADDR - 0x8000);
        prg[stream] = ENDING_SCREEN_BLOCKS as u8;
        assert!(decode(&prg).is_err());
    }

    #[test]
    fn decode_rejects_a_motion_table_without_its_terminator() {
        let mut prg = synthetic();
        let motion = usize::from(ENDING_MOTION_ADDR - 0x8000);
        prg[motion + ENDING_MOTION_STEPS * 2] = 0x00;
        assert!(decode(&prg).is_err());
    }

    #[test]
    fn decode_rejects_a_class_pointer_outside_the_packed_runs() {
        let mut prg = synthetic();
        let tiles = usize::from(ENDING_CLASS_TILES_ADDR - 0x8000);
        prg[tiles + 2 * 7..tiles + 2 * 7 + 2].copy_from_slice(&0xD700u16.to_le_bytes());
        assert!(decode(&prg).is_err());
    }

    #[test]
    fn the_encoded_chunk_parses_back_through_the_pack() {
        let data = decode(&synthetic()).unwrap();
        let bank = vec![[3u8; gradius_pack::TILE_BYTES]; gradius_pack::TILES_PER_BANK];
        let pack = gradius_pack::PackBuilder::new()
            .tiles(&vec![bank; gradius_pack::TILE_BANKS])
            .palettes(&[[1, 2, 3]])
            .chunk(gradius_pack::CHUNK_STAGE, encode(&data))
            .finish();
        let staged = gradius_assets::Pack::parse(&pack).unwrap().stage.unwrap();
        assert_eq!(staged.wall_erase(), Some(data.wall_erase()));
        let record = staged.ending_text_record().unwrap();
        assert_eq!(record.text, &data.ending_text_record()[14..]);
        assert_eq!(
            staged.ending_screen_stream(),
            Some(data.ending_screen_stream())
        );
        assert_eq!(
            staged.ending_screen_classes(),
            Some(data.ending_screen_classes())
        );
        assert_eq!(
            staged.ending_ship_motion().unwrap().steps[0],
            [data.ending_motion()[0], data.ending_motion()[1]]
        );
        assert_eq!(staged.ending_ship_motion().unwrap().terminator, MOTION_END);
        assert_eq!(
            staged.ending_class_pointers().unwrap().tiles[0],
            ENDING_CLASS_TILES_ADDR
        );
        assert_eq!(
            staged.ending_class_pointers().unwrap().attributes[0],
            ENDING_CLASS_ATTRIBUTES_ADDR
        );
        assert_eq!(
            staged.ending_class_attributes(),
            Some(data.ending_class_attributes())
        );
        assert_eq!(
            staged.ending_class_tiles().unwrap().run(0),
            Some(&[0x0Au8][..])
        );
        assert_eq!(
            staged.ending_class_tiles().unwrap().run(1),
            Some(&[0x0Au8, 0x0A, 0x14, 0x3C][..])
        );
    }
}
