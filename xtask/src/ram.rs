//! CPU RAM dumps and per-frame diffs.
//!
//! RAM is 2 KiB mirrored four times in `$0000-$1FFF`; every helper here takes
//! physical offsets (`0..0x800`) and labels them with the `$0000-$07FF`
//! address, because that is what the game's zero-page and absolute operands use.

/// Format `ram[start..end]` as a classic hex/ASCII dump, 16 bytes per line.
pub fn hexdump(ram: &[u8], start: usize, end: usize) -> String {
    let end = end.min(ram.len());
    let start = start.min(end);
    let mut out = String::new();
    out.push_str("      00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F\n");
    let mut addr = start & !0x0F;
    while addr < end {
        out.push_str(&format!("${addr:04X} "));
        for column in 0..16 {
            let at = addr + column;
            if at >= start && at < end {
                out.push_str(&format!("{:02X} ", ram[at]));
            } else {
                out.push_str("   ");
            }
        }
        out.push(' ');
        for column in 0..16 {
            let at = addr + column;
            if at < start || at >= end {
                out.push(' ');
            } else {
                let byte = ram[at];
                out.push(if (0x20..0x7F).contains(&byte) {
                    char::from(byte)
                } else {
                    '.'
                });
            }
        }
        out.push('\n');
        addr += 16;
    }
    out
}

/// Every physical RAM offset in `start..end` whose byte changed.
pub fn diff(old: &[u8], new: &[u8], start: usize, end: usize) -> Vec<(u16, u8, u8)> {
    let end = end.min(old.len()).min(new.len());
    let start = start.min(end);
    (start..end)
        .filter(|offset| old[*offset] != new[*offset])
        .map(|offset| (offset as u16, old[offset], new[offset]))
        .collect()
}

/// One `frame address old -> new` line.
pub fn diff_line(frame: u64, address: u16, old: u8, new: u8) -> String {
    format!("f{frame:05}  ${address:04X}  {old:02X} -> {new:02X}\n")
}

/// Render all changes between two RAM images for a frame.
#[allow(dead_code)]
pub fn diff_text(frame: u64, old: &[u8], new: &[u8], start: usize, end: usize) -> String {
    diff(old, new, start, end)
        .into_iter()
        .map(|(address, old, new)| diff_line(frame, address, old, new))
        .collect()
}

/// Parse `START:END`, both hexadecimal with an optional `$`/`0x` prefix.
pub fn parse_range(spec: &str) -> Option<(usize, usize)> {
    let (start, end) = spec.split_once(':')?;
    let start = parse_hex16(start)? as usize;
    let end = parse_hex16(end)? as usize;
    (start <= end).then_some((start, end))
}

/// Parse a 16-bit hex number, accepting `$1234`, `0x1234` or `1234`.
pub fn parse_hex16(text: &str) -> Option<u16> {
    let text = text.trim();
    let text = text
        .strip_prefix('$')
        .or_else(|| text.strip_prefix("0x"))
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    u16::from_str_radix(text, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexdump_marks_partial_first_and_last_lines() {
        let ram: Vec<u8> = (0u8..=0x2F).collect();
        let text = hexdump(&ram, 0x0E, 0x22);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "      00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F"
        );
        assert!(lines[1].starts_with("$0000 "));
        // Bytes below the start offset are blank in the hex column.
        assert!(lines[1].starts_with("$0000 "));
        // Bytes below the start offset are blank in the hex column.
        assert!(lines[1].contains("0E 0F"));
        assert!(!lines[1].contains("0D 0E"));
        assert!(lines[2].starts_with("$0010 "));
        assert!(lines[2].contains("1E 1F"));
        assert!(lines[3].starts_with("$0020 "));
        assert!(lines[3].contains("20 21"));
        // 0x20 renders as a space and 0x21 as `!` in the ASCII column.
        assert!(lines[3].trim_end().ends_with(" !"));
    }

    #[test]
    fn diff_reports_only_changed_bytes_in_range() {
        let old = vec![0u8; 16];
        let mut new = old.clone();
        new[3] = 0x12;
        new[4] = 0x34;
        new[9] = 0xFF; // outside 0..8
        assert_eq!(
            diff(&old, &new, 0, 8),
            vec![(3, 0x00, 0x12), (4, 0x00, 0x34)]
        );
        assert!(diff(&old, &new, 5, 8).is_empty());
        // Out-of-range input is clamped, never panics.
        assert_eq!(diff(&old, &new, 0, 99).len(), 3);
        assert!(diff(&old, &new, 20, 30).is_empty());
    }

    #[test]
    fn diff_line_is_stable() {
        assert_eq!(
            diff_line(42, 0x300, 0x10, 0x11),
            "f00042  $0300  10 -> 11\n"
        );
        assert_eq!(
            diff_line(0, 0x0000, 0xFF, 0x00),
            "f00000  $0000  FF -> 00\n"
        );
    }

    #[test]
    fn diff_text_prefixes_every_line_with_its_frame() {
        let old = [0u8; 8];
        let mut new = old;
        new[1] = 1;
        new[2] = 2;
        let text = diff_text(7, &old, &new, 0, 8);
        assert_eq!(text, "f00007  $0001  00 -> 01\nf00007  $0002  00 -> 02\n");
    }

    #[test]
    fn parses_ranges_and_hex() {
        assert_eq!(parse_range("0000:0800"), Some((0, 0x800)));
        assert_eq!(parse_range("$0300:$03FF"), Some((0x300, 0x3FF)));
        assert_eq!(parse_range("0x10:20"), Some((0x10, 0x20)));
        assert_eq!(parse_range("0300:0200"), None);
        assert_eq!(parse_range("nope"), None);
        assert_eq!(parse_hex16("$FFFE"), Some(0xFFFE));
        assert_eq!(parse_hex16("0x8000"), Some(0x8000));
        assert_eq!(parse_hex16(" 12ab "), Some(0x12AB));
        assert_eq!(parse_hex16("xyz"), None);
    }
}
