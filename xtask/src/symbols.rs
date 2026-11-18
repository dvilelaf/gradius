//! Symbol files for the disassembler.
//!
//! Two textual forms are understood:
//!
//! - the stable `tools/ghidra/symbols.txt`: `ADDR NAME [SIZE]`, one per line,
//!   `#`/`;` comments allowed;
//! - the Ghidra export (`functions_full.txt`): a tab-separated
//!   `entry size insns callers name origin evidence` row whose entry column is
//!   `PRG0::8010`.
//!
//! `xtask symbols` merges the export into the stable file. Entries already in
//! the stable file win, so a hand-written name (or a hand-added address) is
//! never replaced by a later export; only missing addresses are appended.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// One named PRG address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub addr: u16,
    pub name: String,
    /// Function size in bytes, when the source knows it.
    pub size: Option<u16>,
}

/// Location used when `--symbols` is not given: the Ghidra export when it is
/// present, then the normalized file.
pub fn default_path() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?;
    [
        "tools/ghidra/functions_full.txt",
        "tools/ghidra/symbols.txt",
    ]
    .iter()
    .map(|relative| root.join(relative))
    .find(|path| path.exists())
}

/// Read and parse a symbol file. An explicitly named but missing file is an
/// error; an absent default is simply no symbols.
pub fn load(path: Option<&Path>) -> Result<(BTreeMap<u16, String>, Option<PathBuf>)> {
    let source = match path {
        Some(path) => {
            if !path.exists() {
                bail!("symbol file {} does not exist", path.display());
            }
            Some(path.to_path_buf())
        }
        None => default_path(),
    };
    let Some(source) = source else {
        return Ok((BTreeMap::new(), None));
    };
    let symbols = read(&source)?;
    let names = symbols
        .into_iter()
        .map(|symbol| (symbol.addr, symbol.name))
        .collect();
    Ok((names, Some(source)))
}

/// Read and parse a symbol file.
pub fn read(path: &Path) -> Result<Vec<Symbol>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Parse the two accepted line forms, ignoring blank and comment lines.
pub fn parse(text: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let parsed = if fields[0].contains("::") {
            // Ghidra export row: entry, size, insns, callers, name, ...
            (fields.len() >= 5).then(|| {
                (
                    parse_addr(fields[0]),
                    fields[4].to_string(),
                    parse_size(fields[1]),
                )
            })
        } else {
            (fields.len() >= 2).then(|| {
                (
                    parse_addr(fields[0]),
                    fields[1].to_string(),
                    fields.get(2).and_then(|size| parse_size(size)),
                )
            })
        };
        let Some((addr, name, size)) = parsed else {
            bail!(
                "expected `addr name [size]` or a Ghidra export row, got `{line}` (line {})",
                index + 1
            );
        };
        let Some(addr) = addr else {
            bail!("bad address `{}` (line {})", fields[0], index + 1);
        };
        symbols.push(Symbol { addr, name, size });
    }
    Ok(symbols)
}

/// Merge a parsed export into the existing stable-file text.
///
/// Existing entries win by address; their size is kept, or filled from the
/// export when the existing line omitted it. New export addresses are added.
pub fn merge(existing: &str, export: &[Symbol]) -> Result<Vec<Symbol>> {
    let mut merged: BTreeMap<u16, Symbol> = parse(existing)?
        .into_iter()
        .map(|symbol| (symbol.addr, symbol))
        .collect();
    for symbol in export {
        match merged.entry(symbol.addr) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if entry.get().size.is_none() {
                    entry.get_mut().size = symbol.size;
                }
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(symbol.clone());
            }
        }
    }
    Ok(merged.into_values().collect())
}

/// Render the stable `ADDR NAME SIZE` form.
pub fn render(symbols: &[Symbol]) -> String {
    let mut out = String::new();
    out.push_str("# Gradius PRG symbols: address name size\n");
    out.push_str("# Written by `cargo xtask symbols`; edit names here and they survive a merge.\n");
    for symbol in symbols {
        let _ = match symbol.size {
            Some(size) => writeln!(out, "{:04x}  {:<24} 0x{size:X}", symbol.addr, symbol.name),
            None => writeln!(out, "{:04x}  {}", symbol.addr, symbol.name),
        };
    }
    out
}

fn parse_addr(text: &str) -> Option<u16> {
    let text = text.trim_end_matches(':');
    let text = text.rsplit("::").next().unwrap_or(text);
    crate::ram::parse_hex16(text)
}

fn parse_size(text: &str) -> Option<u16> {
    crate::ram::parse_hex16(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_stable_form_with_prefixes_comments_and_sizes() {
        let text = "\
# address name size
$8010 reset 0x5A

; hand annotation
0x9000 player_update
a2cb stage_section_spawner 0x2D
";
        let symbols = parse(text).unwrap();
        assert_eq!(
            symbols,
            vec![
                Symbol {
                    addr: 0x8010,
                    name: "reset".into(),
                    size: Some(0x5A)
                },
                Symbol {
                    addr: 0x9000,
                    name: "player_update".into(),
                    size: None
                },
                Symbol {
                    addr: 0xA2CB,
                    name: "stage_section_spawner".into(),
                    size: Some(0x2D)
                },
            ]
        );
    }

    #[test]
    fn parses_a_ghidra_export_row() {
        let export = "\
# entry\tsize_bytes\tinsns\tcallers\tname\torigin\tevidence
PRG0::8010\t0x5A\t45\t0\treset\tdirect\t-
PRG0::80da\t0x34\t25\t0\tFUN_PRG0__80da\tptr\tinline JSR $83E9 @ 80C9 (+0)
";
        let symbols = parse(export).unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(
            symbols[1],
            Symbol {
                addr: 0x80DA,
                name: "FUN_PRG0__80da".into(),
                size: Some(0x34)
            }
        );
    }

    #[test]
    fn merge_keeps_existing_names_and_adds_missing_addresses() {
        let existing = "\
8010  reset  0x5A
9000  player_update
";
        let export = parse(
            "\
# entry size insns callers name origin evidence
PRG0::8010\t0x60\t45\t0\treset_from_export\tdirect\t-
PRG0::9000\t0x10\t2\t1\tFUN_PRG0__9000\tcall\t-
PRG0::a2cb\t0x2D\t20\t0\tFUN_PRG0__a2cb\tcall\t-
",
        )
        .unwrap();

        let merged = merge(existing, &export).unwrap();
        let by_addr: BTreeMap<u16, &Symbol> =
            merged.iter().map(|symbol| (symbol.addr, symbol)).collect();
        assert_eq!(by_addr[&0x8010].name, "reset");
        assert_eq!(by_addr[&0x8010].size, Some(0x5A));
        // An existing line without a size inherits the export size.
        assert_eq!(by_addr[&0x9000].name, "player_update");
        assert_eq!(by_addr[&0x9000].size, Some(0x10));
        // New export addresses are appended.
        assert_eq!(by_addr[&0xA2CB].name, "FUN_PRG0__a2cb");
        assert_eq!(by_addr.len(), 3);
    }

    #[test]
    fn render_round_trips_through_parse() {
        let symbols = vec![
            Symbol {
                addr: 0x8010,
                name: "reset".into(),
                size: Some(0x5A),
            },
            Symbol {
                addr: 0x9000,
                name: "player_update".into(),
                size: None,
            },
        ];
        let text = render(&symbols);
        assert!(text.contains("8010  reset"));
        assert!(text.contains("9000  player_update"));
        assert_eq!(parse(&text).unwrap(), symbols);
    }

    #[test]
    fn rejects_a_line_without_a_name() {
        assert!(parse("8010\n").is_err());
        assert!(parse("PRG0::8010\t0x5A\t45\t0\n").is_err());
    }
}
