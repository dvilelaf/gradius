//! Decode the object kinds each stage's column streams spawn.
//!
//! A stream record carries a *type*, not an object kind. `$A3DE` resolves a
//! type `>= $80` through the 4-byte object table at `$A5FC` (index `type &
//! $3F`) and its second byte is the `$030C` kind. A type `< $80` selects a
//! 3-byte `$A65C` terrain definition; its first byte, minus `$A0` or `$D0`,
//! becomes the kind (`$A391`/`$A3AB`).
//!
//! Five-byte command records bypass both tables: `$A460` spawns kind `$96`
//! when `$19 == 2` (stage 3), and otherwise a large object whose kind is the
//! fourth command byte (`$66`).
//!
//! ```text
//! cargo xtask stage-enemies "Gradius (E).nes"
//! ```
//!
//! The command prints the union of kinds used across stages 1-7, their `$AE16`
//! dispatch handler, and the per-stage raw type list. It is evidence for
//! `docs/analysis/stage_enemies.md`; it invents no kind or handler.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Result};

use crate::sections::{decode_records, Record};

/// Playable stages (`$19` 0..6).
pub const STAGE_COUNT: usize = 7;
/// `$A7CA`: per-stage column-pointer table (8 words).
const SPAWN_TABLE: u16 = 0xA7CA;
/// `$A5FC`: 24 four-byte object definitions (`$A5FC`..`$A65C`).
const OBJECT_DEFS: u16 = 0xA5FC;
/// `$A65C`: three-byte terrain definitions.
const TERRAIN_DEFS: u16 = 0xA65C;
/// `$AE16`: the `$AE13` dispatch table, one word per kind.
const AI_TABLE: u16 = 0xAE16;
/// Number of `$AE16` entries (kind `$00`..`$29`).
pub const AI_KIND_COUNT: usize = 42;

/// One resolved kind, with the raw stream type that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KindUse {
    /// Raw stream type byte.
    pub raw: u8,
    /// `$030C` object kind.
    pub kind: u8,
    /// True when the type selected a `$A65C` terrain definition.
    pub terrain: bool,
    /// Number of records of this raw type in the stage.
    pub count: u16,
}

/// The kinds and command records of one stage.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct StageKinds {
    /// Spawn records, sorted by raw type.
    pub uses: Vec<KindUse>,
    /// Kinds produced by five-byte command records, with counts.
    pub commands: Vec<(u8, u16)>,
}

/// Every stage plus the `$AE16` handler table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub stages: [StageKinds; STAGE_COUNT],
    pub handlers: [u16; AI_KIND_COUNT],
}

fn read(prg: &[u8], addr: u16) -> Result<u8> {
    prg.get(usize::from(addr.wrapping_sub(0x8000)))
        .copied()
        .ok_or_else(|| anyhow!("address ${addr:04X} is outside the 32 KiB PRG"))
}

fn word(prg: &[u8], addr: u16) -> Result<u16> {
    Ok(u16::from_le_bytes([
        read(prg, addr)?,
        read(prg, addr.wrapping_add(1))?,
    ]))
}

/// Resolve one stream type to its `$030C` kind.
pub fn spawn_kind(prg: &[u8], raw: u8) -> Result<u8> {
    if raw >= 0x80 {
        // `$A367`: two `ASL`s, so the index is `type & $3F`; `$65` is the kind.
        Ok(read(
            prg,
            OBJECT_DEFS.wrapping_add(4 * u16::from(raw & 0x3F) + 1),
        )?)
    } else {
        // `$A391` copies the 3-byte definition; `$A3AB` maps its tile base.
        let tile = read(prg, TERRAIN_DEFS.wrapping_add(3 * u16::from(raw)))?;
        let offset = tile.wrapping_sub(0xA0);
        Ok(if offset < 0x30 {
            offset
        } else {
            offset.wrapping_sub(0x30)
        })
    }
}

/// Resolve the kind a five-byte command record spawns.
///
/// Stage 3's command writes kind `$96` (`$A495`); `$030C` keeps the high bit
/// but `$AE13` drops it, so the dispatch kind is `$16` (`$C900`).
pub fn command_kind(stage: usize, bytes: [u8; 5]) -> u8 {
    if stage == 2 {
        0x96 & 0x7F
    } else {
        // `$A4A0`: the large object's kind is `$66`, the fourth command byte.
        bytes[3] & 0x7F
    }
}

fn push_use(uses: &mut Vec<KindUse>, raw: u8, kind: u8, terrain: bool) {
    if let Some(entry) = uses.iter_mut().find(|entry| entry.raw == raw) {
        entry.count += 1;
    } else {
        uses.push(KindUse {
            raw,
            kind,
            terrain,
            count: 1,
        });
    }
}

fn command_counts(stage: usize, records: &[Record]) -> Vec<(u8, u16)> {
    let mut out: Vec<(u8, u16)> = Vec::new();
    for record in records {
        if let Record::Command(bytes) = record {
            let kind = command_kind(stage, *bytes);
            match out.iter_mut().find(|(used, _)| *used == kind) {
                Some((_, count)) => *count += 1,
                None => out.push((kind, 1)),
            }
        }
    }
    out.sort_unstable();
    out
}

/// Decode all seven stages from a PRG image.
pub fn decode(prg: &[u8]) -> Result<Decoded> {
    let mut pointers = [0u16; STAGE_COUNT + 1];
    for (stage, pointer) in pointers.iter_mut().enumerate() {
        *pointer = word(prg, SPAWN_TABLE.wrapping_add(2 * stage as u16))?;
    }

    let mut stages: [StageKinds; STAGE_COUNT] = std::array::from_fn(|_| StageKinds::default());
    for stage in 0..STAGE_COUNT {
        let start = pointers[stage];
        let columns = usize::from(pointers[stage + 1].wrapping_sub(start)) / 2;
        if columns == 0 || columns > 8 {
            bail!("stage {} has {columns} column pointers", stage + 1);
        }
        let mut uses = Vec::new();
        let mut commands = Vec::new();
        for column in 0..columns {
            let pointer = word(prg, start.wrapping_add(2 * column as u16))?;
            let records = decode_records(prg, pointer)?;
            commands.extend(command_counts(stage, &records));
            for record in records {
                if let Record::Spawn { kind, .. } = record {
                    push_use(&mut uses, kind, spawn_kind(prg, kind)?, kind < 0x80);
                }
            }
        }
        uses.sort_unstable();
        // The same command kind can appear in several columns.
        let mut merged: Vec<(u8, u16)> = Vec::new();
        for (kind, count) in commands {
            match merged.iter_mut().find(|(used, _)| *used == kind) {
                Some((_, total)) => *total += count,
                None => merged.push((kind, count)),
            }
        }
        merged.sort_unstable();
        stages[stage] = StageKinds {
            uses,
            commands: merged,
        };
    }

    let mut handlers = [0u16; AI_KIND_COUNT];
    for (kind, handler) in handlers.iter_mut().enumerate() {
        *handler = word(prg, AI_TABLE.wrapping_add(2 * kind as u16))?;
    }

    Ok(Decoded { stages, handlers })
}

fn stage_count(stage: &StageKinds, kind: u8) -> u16 {
    let spawns = stage
        .uses
        .iter()
        .filter(|entry| entry.kind == kind)
        .map(|entry| entry.count)
        .sum::<u16>();
    let commands = stage
        .commands
        .iter()
        .filter(|(used, _)| *used == kind)
        .map(|(_, count)| *count)
        .sum::<u16>();
    spawns + commands
}

/// Render the kind × stage matrix and the raw type lists.
pub fn render(decoded: &Decoded) -> String {
    let mut kinds: Vec<u8> = decoded
        .stages
        .iter()
        .flat_map(|stage| {
            stage
                .uses
                .iter()
                .map(|entry| entry.kind)
                .chain(stage.commands.iter().map(|(kind, _)| *kind))
        })
        .collect();
    kinds.sort_unstable();
    kinds.dedup();

    let mut text = String::from("kind handler  st1 st2 st3 st4 st5 st6 st7\n");
    for kind in kinds {
        let handler = decoded
            .handlers
            .get(usize::from(kind))
            .copied()
            .unwrap_or(0);
        text.push_str(&format!("${kind:02X}  ${handler:04X} "));
        for stage in &decoded.stages {
            let count = stage_count(stage, kind);
            if count == 0 {
                text.push_str("   .");
            } else {
                text.push_str(&format!("{count:4}"));
            }
        }
        text.push('\n');
    }

    for (index, stage) in decoded.stages.iter().enumerate() {
        let terrain = stage.uses.iter().filter(|entry| entry.terrain).count();
        let formations = stage.uses.len() - terrain;
        text.push_str(&format!(
            "\n; stage {}: {} raw types ({terrain} terrain, {formations} formations), {} commands\n",
            index + 1,
            stage.uses.len(),
            stage.commands.iter().map(|(_, count)| count).sum::<u16>(),
        ));
        text.push_str(";  ");
        for entry in &stage.uses {
            text.push_str(&format!(" ${:02X}", entry.raw));
        }
        text.push('\n');
        if !stage.commands.is_empty() {
            text.push_str(";  commands ->");
            for (kind, count) in &stage.commands {
                text.push_str(&format!(" ${kind:02X} x{count}"));
            }
            text.push('\n');
        }
    }
    text
}

/// `cargo xtask stage-enemies <rom> [--out table.txt]`.
pub fn run(rom: &Path, out: Option<&Path>) -> Result<()> {
    let cart = crate::load(rom)?;
    let text = render(&decode(&cart.prg)?);
    match out {
        Some(path) => {
            fs::write(path, &text)?;
            eprintln!("wrote {}", path.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(prg: &mut [u8], addr: u16, value: u16) {
        let offset = usize::from(addr - 0x8000);
        prg[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put(prg: &mut [u8], addr: u16, bytes: &[u8]) {
        let offset = usize::from(addr - 0x8000);
        prg[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    #[test]
    fn spawn_kind_reads_the_object_and_terrain_tables() {
        let mut prg = vec![0u8; 0x8000];
        // Type $82 -> object index 2, kind from its second byte.
        put(&mut prg, OBJECT_DEFS + 4 * 2, &[0x00, 0x08, 0x00, 0x00]);
        // Terrain tile bases: $B2 -> $12, $E3 -> $13, $C0 -> $20, $D0 -> $00.
        put(&mut prg, TERRAIN_DEFS, &[0xB2, 0x80, 0x12]);
        put(&mut prg, TERRAIN_DEFS + 3 * 4, &[0xE3, 0x80, 0x10]);
        put(&mut prg, TERRAIN_DEFS + 3 * 8, &[0xC0, 0x00, 0x60]);
        put(&mut prg, TERRAIN_DEFS + 3 * 9, &[0xD0, 0x00, 0x60]);

        assert_eq!(spawn_kind(&prg, 0x82).unwrap(), 0x08);
        assert_eq!(spawn_kind(&prg, 0x00).unwrap(), 0x12);
        assert_eq!(spawn_kind(&prg, 0x04).unwrap(), 0x13);
        assert_eq!(spawn_kind(&prg, 0x08).unwrap(), 0x20);
        assert_eq!(spawn_kind(&prg, 0x09).unwrap(), 0x00);
    }

    #[test]
    fn command_kind_is_stage_three_only() {
        assert_eq!(command_kind(2, [0x10, 0xF0, 0x38, 0x21, 0x23]), 0x16);
        assert_eq!(command_kind(4, [0x40, 0xF0, 0x01, 0x14, 0x80]), 0x14);
    }

    #[test]
    fn decode_collects_raw_types_and_stage_three_commands() {
        let mut prg = vec![0u8; 0x8000];
        put(&mut prg, OBJECT_DEFS + 4 * 2, &[0x00, 0x08, 0x00, 0x00]);
        put(&mut prg, TERRAIN_DEFS, &[0xB2, 0x80, 0x12]);
        // One column per stage; sub-tables are 2 bytes, the eighth word ends
        // stage 7.
        for stage in 0..=STAGE_COUNT {
            write(
                &mut prg,
                SPAWN_TABLE + 2 * stage as u16,
                0xA800 + 2 * stage as u16,
            );
        }
        let streams = [
            (0x9000u16, &[0x10u8, 0x82, 0x20, 0x00, 0xFF][..]),
            (0x9020, &[0xFF][..]),
            (0x9040, &[0x10, 0xF0, 0x38, 0x21, 0x23, 0xFF][..]),
            (0x9060, &[0xFF][..]),
            (0x9080, &[0x40, 0xF0, 0x01, 0x14, 0x80, 0xFF][..]),
            (0x90A0, &[0xFF][..]),
            (0x90C0, &[0xFF][..]),
        ];
        for (stage, (stream, bytes)) in streams.iter().enumerate() {
            write(&mut prg, 0xA800 + 2 * stage as u16, *stream);
            put(&mut prg, *stream, bytes);
        }
        // Every handler entry must be readable.
        for kind in 0..AI_KIND_COUNT {
            write(&mut prg, AI_TABLE + 2 * kind as u16, 0xB000 + kind as u16);
        }

        let decoded = decode(&prg).unwrap();
        assert_eq!(
            decoded.stages[0].uses,
            [
                KindUse {
                    raw: 0x00,
                    kind: 0x12,
                    terrain: true,
                    count: 1
                },
                KindUse {
                    raw: 0x82,
                    kind: 0x08,
                    terrain: false,
                    count: 1
                },
            ]
        );
        assert_eq!(decoded.stages[2].commands, [(0x16, 1)]);
        assert_eq!(decoded.stages[4].commands, [(0x14, 1)]);
        assert_eq!(decoded.handlers[8], 0xB008);
    }

    #[test]
    fn render_prints_the_union_matrix_and_raw_types() {
        let mut prg = vec![0u8; 0x8000];
        put(&mut prg, OBJECT_DEFS + 4 * 2, &[0x00, 0x08, 0x00, 0x00]);
        for stage in 0..=STAGE_COUNT {
            write(
                &mut prg,
                SPAWN_TABLE + 2 * stage as u16,
                0xA800 + 2 * stage as u16,
            );
        }
        write(&mut prg, 0xA800, 0x9000);
        put(&mut prg, 0x9000, &[0x10, 0x82, 0xFF]);
        for stage in 1..STAGE_COUNT {
            write(
                &mut prg,
                0xA800 + 2 * stage as u16,
                0x9000 + 0x20 * stage as u16,
            );
            put(&mut prg, 0x9000 + 0x20 * stage as u16, &[0xFF]);
        }
        for kind in 0..AI_KIND_COUNT {
            write(&mut prg, AI_TABLE + 2 * kind as u16, 0xB000 + kind as u16);
        }

        let text = render(&decode(&prg).unwrap());
        assert!(text.starts_with("kind handler  st1 st2"));
        assert!(text.contains("$08  $B008 "));
        assert!(text.contains("; stage 1: 1 raw types (0 terrain, 1 formations), 0 commands"));
        assert!(text.contains(";   $82"));
    }
}
