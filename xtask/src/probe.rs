//! Structured dump of the oracle's PPU and CPU state at a chosen frame.
//!
//! Everything here is derived from state the emulator already exposes, so a
//! `probe` run is fully reproducible from the command line recorded in the
//! output. Values are presented as NES master-palette indices and raw OAM
//! fields: no cartridge bytes are printed or written.

use gradius_nes::{SCREEN_HEIGHT, SCREEN_WIDTH};

pub use gradius_extract::fnv1a;

/// One decoded OAM slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub index: u8,
    pub x: u8,
    pub y: u8,
    pub tile: u8,
    pub attributes: u8,
}

impl Sprite {
    /// OAM stores `y - 1`, so a stored value of 0 puts the top row at y=1.
    pub fn top(&self) -> u16 {
        u16::from(self.y) + 1
    }

    pub fn palette(&self) -> u8 {
        self.attributes & 0x03
    }

    pub fn behind_background(&self) -> bool {
        self.attributes & 0x20 != 0
    }

    pub fn flip_horizontal(&self) -> bool {
        self.attributes & 0x40 != 0
    }

    pub fn flip_vertical(&self) -> bool {
        self.attributes & 0x80 != 0
    }

    /// Sprites with a stored Y of 0xEF..=0xFF are parked off-screen by the
    /// game; every other value can appear on a scanline.
    pub fn off_screen(&self) -> bool {
        self.y >= 0xEF
    }
}

/// Decode `count` four-byte OAM entries starting at the given slot.
pub fn decode_oam(oam: &[u8; 256], count: usize) -> Vec<Sprite> {
    (0..count)
        .map(|slot| {
            let base = slot * 4;
            Sprite {
                index: slot as u8,
                y: oam[base],
                tile: oam[base + 1],
                attributes: oam[base + 2],
                x: oam[base + 3],
            }
        })
        .collect()
}

/// PPUCTRL bit names, in the order they are printed.
pub fn ctrl_flags(ctrl: u8) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if ctrl & 0x80 != 0 {
        flags.push("nmi");
    }
    flags.push(if ctrl & 0x20 != 0 { "8x16" } else { "8x8" });
    if ctrl & 0x10 != 0 {
        flags.push("bg$1000");
    }
    if ctrl & 0x08 != 0 {
        flags.push("obj$1000");
    }
    if ctrl & 0x04 != 0 {
        flags.push("inc32");
    }
    flags.push(match ctrl & 0x03 {
        0 => "nt0",
        1 => "nt1",
        2 => "nt2",
        _ => "nt3",
    });
    flags
}

/// PPUMASK bit names.
pub fn mask_flags(mask: u8) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if mask & 0x80 != 0 {
        flags.push("emph-blue");
    }
    if mask & 0x40 != 0 {
        flags.push("emph-green");
    }
    if mask & 0x20 != 0 {
        flags.push("emph-red");
    }
    if mask & 0x10 != 0 {
        flags.push("sprites");
    }
    if mask & 0x08 != 0 {
        flags.push("bg");
    }
    if mask & 0x04 != 0 {
        flags.push("left-sprites");
    }
    if mask & 0x02 != 0 {
        flags.push("left-bg");
    }
    if mask & 0x01 != 0 {
        flags.push("greyscale");
    }
    flags
}

/// One contiguous span of executed program counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcRange {
    pub start: u16,
    pub end: u16,
}

/// Compact summary of the coverage trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecSummary {
    pub instructions: u64,
    pub distinct_pcs: usize,
    pub min: Option<u16>,
    pub max: Option<u16>,
    /// Up to `limit` contiguous spans, in address order.
    pub ranges: Vec<PcRange>,
    /// Total number of contiguous spans, even if `ranges` was capped.
    pub range_count: usize,
    /// One bit per executed 4 KiB page (`$8000` is bit 8).
    pub pages: u16,
}

impl ExecSummary {
    /// Collapse the hit map into at most `limit` contiguous ranges.
    pub fn from_hits(hits: &[u32], instructions: u64, limit: usize) -> Self {
        let (min, max) = hits.iter().enumerate().filter(|(_, hits)| **hits > 0).fold(
            (None, None),
            |(min, _), (pc, _)| {
                let pc = pc as u16;
                (Some(min.map_or(pc, |min: u16| min.min(pc))), Some(pc))
            },
        );
        let mut ranges = Vec::new();
        let mut range_count = 0;
        let mut start: Option<u16> = None;
        let mut pages = 0u16;
        for pc in 0u32..=0x1_0000 {
            let executed = pc < 0x1_0000 && hits[pc as usize] > 0;
            if executed {
                pages |= 1 << ((pc as usize) >> 12);
            }
            match (start, executed) {
                (None, true) => start = Some(pc as u16),
                (Some(from), false) => {
                    range_count += 1;
                    if ranges.len() < limit {
                        ranges.push(PcRange {
                            start: from,
                            end: (pc as u16).wrapping_sub(1),
                        });
                    }
                    start = None;
                }
                _ => {}
            }
        }
        Self {
            instructions,
            distinct_pcs: hits.iter().filter(|hits| **hits > 0).count(),
            min,
            max,
            ranges,
            range_count,
            pages,
        }
    }
}

/// Everything `probe` prints. Consecutive frames reuse the same shape so the
/// output can be diffed.
#[derive(Clone, Debug)]
pub struct Report {
    pub frame: u64,
    pub ctrl: u8,
    pub mask: u8,
    pub chr_bank: u8,
    pub palette: [u8; 32],
    pub oam: [u8; 256],
    pub nametable_fnv: u64,
    pub framebuffer_fnv: u64,
    pub exec: ExecSummary,
    /// Physical 2 KiB nametable memory, checksummed above and optionally saved.
    pub nametables: Vec<u8>,
}

impl Report {
    pub fn sprites(&self, count: usize) -> Vec<Sprite> {
        decode_oam(&self.oam, count.min(64))
    }

    pub fn palette_hex(&self) -> String {
        self.palette
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn to_text(&self, sprite_rows: usize) -> String {
        let mut out = String::new();
        out.push_str(&format!("frame:        {}\n", self.frame));
        out.push_str(&format!(
            "ppu ctrl:     ${:02X}  [{}]\n",
            self.ctrl,
            ctrl_flags(self.ctrl).join(", ")
        ));
        out.push_str(&format!(
            "ppu mask:     ${:02X}  [{}]\n",
            self.mask,
            mask_flags(self.mask).join(", ")
        ));
        out.push_str(&format!("chr bank:     {}\n", self.chr_bank));
        out.push_str(&format!(
            "sprite mode:  {}\n",
            if self.ctrl & 0x20 != 0 {
                "8x16 (tile bit 0 ignored)"
            } else {
                "8x8"
            }
        ));
        out.push_str(&format!(
            "sprites:      pattern table ${:04X}  bg pattern table ${:04X}\n",
            if self.ctrl & 0x08 != 0 { 0x1000 } else { 0 },
            if self.ctrl & 0x10 != 0 { 0x1000 } else { 0 },
        ));
        out.push_str(&format!(
            "nametable:    fnv1a ${:016X} ({} bytes, physical tables)\n",
            self.nametable_fnv,
            self.nametables.len()
        ));
        out.push_str(&format!(
            "framebuffer:  fnv1a ${:016X} ({}x{} RGBA)\n",
            self.framebuffer_fnv, SCREEN_WIDTH, SCREEN_HEIGHT
        ));

        out.push_str("\npalette RAM (NES master indices):\n");
        out.push_str("  $3F00..$3F0F bg :");
        for byte in &self.palette[..16] {
            out.push_str(&format!(" {byte:02X}"));
        }
        out.push('\n');
        out.push_str("  $3F10..$3F1F spr:");
        for byte in &self.palette[16..] {
            out.push_str(&format!(" {byte:02X}"));
        }
        out.push('\n');

        out.push_str(&format!(
            "\noam (first {} of 64 entries; y is stored Y-1):\n",
            sprite_rows.min(64)
        ));
        out.push_str("  #   x    y  top  tile attr  use  flags\n");
        for sprite in self.sprites(sprite_rows) {
            let visible = !sprite.off_screen();
            let mut flags = Vec::new();
            if sprite.behind_background() {
                flags.push("behind".to_string());
            }
            flags.push(format!("pal{}", sprite.palette()));
            if sprite.flip_horizontal() {
                flags.push("flipH".to_string());
            }
            if sprite.flip_vertical() {
                flags.push("flipV".to_string());
            }
            // In 8x16 mode OAM's tile byte selects a pair: `tile & 0xFE` and
            // `tile | 1`, with bit 0 ignored by the PPU.
            if self.ctrl & 0x20 != 0 && visible {
                flags.push(format!(
                    "pair=${:02X}/{:02X}",
                    sprite.tile & 0xFE,
                    sprite.tile | 1
                ));
            }
            out.push_str(&format!(
                "  {:2}  {:3}  {:3}  {:3}  ${:02X}  ${:02X}   {}   {}\n",
                sprite.index,
                sprite.x,
                sprite.y,
                sprite.top(),
                sprite.tile,
                sprite.attributes,
                if visible { "spr" } else { "off" },
                flags.join(" ")
            ));
        }

        out.push_str("\nexecuted code:\n");
        out.push_str(&format!(
            "  instructions: {}\n  distinct pcs: {}\n",
            self.exec.instructions, self.exec.distinct_pcs
        ));
        if let (Some(min), Some(max)) = (self.exec.min, self.exec.max) {
            out.push_str(&format!("  span:         ${min:04X}-${max:04X}\n"));
        }
        let mut pages = Vec::new();
        for page in 0..16 {
            if self.exec.pages & (1 << page) != 0 {
                pages.push(format!("${:X}000", page));
            }
        }
        out.push_str(&format!("  pages:        {}\n", pages.join(" ")));
        out.push_str(&format!(
            "  spans:        {}{}\n",
            self.exec.range_count,
            if self.exec.range_count > self.exec.ranges.len() {
                format!(
                    " (first {}: {})",
                    self.exec.ranges.len(),
                    self.exec
                        .ranges
                        .iter()
                        .map(|range| format!("${:04X}-${:04X}", range.start, range.end))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            } else {
                format!(
                    " ({})",
                    self.exec
                        .ranges
                        .iter()
                        .map(|range| format!("${:04X}-${:04X}", range.start, range.end))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            }
        ));
        out
    }

    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        out.push_str(&format!("  \"frame\": {},\n", self.frame));
        out.push_str(&format!("  \"ctrl\": {},\n", self.ctrl));
        out.push_str(&format!("  \"mask\": {},\n", self.mask));
        out.push_str(&format!("  \"chr_bank\": {},\n", self.chr_bank));
        out.push_str(&format!(
            "  \"sprite_height\": {},\n",
            if self.ctrl & 0x20 != 0 { 16 } else { 8 }
        ));
        out.push_str(&format!(
            "  \"sprite_pattern_table\": {},\n",
            if self.ctrl & 0x08 != 0 { 0x1000 } else { 0 }
        ));
        out.push_str(&format!(
            "  \"background_pattern_table\": {},\n",
            if self.ctrl & 0x10 != 0 { 0x1000 } else { 0 }
        ));
        out.push_str(&format!("  \"palette\": \"{}\",\n", self.palette_hex()));
        out.push_str(&format!(
            "  \"nametable_fnv1a\": \"{:016X}\",\n",
            self.nametable_fnv
        ));
        out.push_str(&format!(
            "  \"framebuffer_fnv1a\": \"{:016X}\",\n",
            self.framebuffer_fnv
        ));
        out.push_str(&format!(
            "  \"instructions\": {},\n  \"distinct_pcs\": {},\n",
            self.exec.instructions, self.exec.distinct_pcs
        ));
        out.push_str("  \"oam\": [\n");
        let sprites = self.sprites(64);
        for (i, sprite) in sprites.iter().enumerate() {
            out.push_str(&format!(
                "    {{\"index\": {},\"x\": {},\"y\": {},\"tile\": {},\"attr\": {}}}{}\n",
                sprite.index,
                sprite.x,
                sprite.y,
                sprite.tile,
                sprite.attributes,
                if i + 1 == sprites.len() { "" } else { "," }
            ));
        }
        out.push_str("  ],\n");
        out.push_str("  \"exec_ranges\": [");
        for (i, range) in self.exec.ranges.iter().enumerate() {
            out.push_str(&format!(
                "{}[{},{}]",
                if i == 0 { "" } else { "," },
                range.start,
                range.end
            ));
        }
        out.push_str("]\n}\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_known_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn decodes_oam_fields() {
        let mut oam = [0u8; 256];
        // Slot 3: y=0x77 -> top 0x78, tile 0x24, attr 0xE3, x=0x28.
        oam[12] = 0x77;
        oam[13] = 0x24;
        oam[14] = 0xE3;
        oam[15] = 0x28;
        let sprites = decode_oam(&oam, 32);
        let ship = sprites[3];
        assert_eq!(ship.top(), 0x78);
        assert_eq!(ship.tile, 0x24);
        assert_eq!(ship.x, 0x28);
        assert_eq!(ship.palette(), 3);
        assert!(ship.behind_background());
        assert!(ship.flip_horizontal());
        assert!(ship.flip_vertical());
        assert!(!ship.off_screen());
        // Slot 0 was never written: y=0 is on-screen at the top edge.
        assert!(!sprites[0].off_screen());
        // A parked sprite.
        oam[0] = 0xF0;
        assert!(decode_oam(&oam, 1)[0].off_screen());
    }

    #[test]
    fn collapses_coverage_into_ranges() {
        let mut hits = vec![0u32; 0x10000];
        hits[0x8000..=0x8005].fill(1);
        hits[0x9000..=0x9002].fill(2);
        let exec = ExecSummary::from_hits(&hits, 42, 8);
        assert_eq!(exec.distinct_pcs, 9);
        assert_eq!(
            exec.ranges,
            vec![
                PcRange {
                    start: 0x8000,
                    end: 0x8005
                },
                PcRange {
                    start: 0x9000,
                    end: 0x9002
                },
            ]
        );
        assert_eq!(exec.min, Some(0x8000));
        assert_eq!(exec.max, Some(0x9002));
        assert_eq!(exec.range_count, 2);
        assert_eq!(exec.pages, (1 << 8) | (1 << 9));
    }

    #[test]
    fn flags_are_named() {
        assert!(ctrl_flags(0x90).contains(&"nmi"));
        assert!(ctrl_flags(0x90).contains(&"8x8"));
        assert!(ctrl_flags(0x30).contains(&"8x16"));
        assert!(ctrl_flags(0x18).contains(&"bg$1000"));
        assert!(mask_flags(0x1E).contains(&"sprites"));
        assert!(mask_flags(0x1E).contains(&"bg"));
    }
}
