//! Decode a five-byte `$A374` command record the way `$A460` consumes it.
//!
//! `$A374`-`$A380` copies the five stream bytes to `$63-$67`, `$A387`
//! subtracts `$70` from `$64`, and `$A460` chooses between the stage-3 layout
//! (`$19 == 2`, `$A469`) and the large-object layout (`$A4A0`). The evidence
//! and the oracle reproductions are in `docs/analysis/command_format.md`.

/// One command record's fields after `$A460` has run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spawn {
    /// `$64` after `$A387`: the second stream byte minus `$70`.
    pub subtype: u8,
    /// `$036C+X`: always `$F0` (off the right edge).
    pub x: u8,
    /// `$030C+X`: `$96` for stage 3, otherwise `$66`.
    pub kind: u8,
    /// `$032C+X`: `$65` for stage 3, `$67` otherwise.
    pub y: u8,
    /// `$03BC+X` (`$66`) for stage 3, zero otherwise.
    pub byte_bc: u8,
    /// `$03EC+X` (`$67`) for stage 3, zero otherwise.
    pub byte_ec: u8,
    /// The nonzero nibbles of `$65`, low then high, for a large object.
    /// Each allocates a `$0600` attachment record and its `$0601` field is
    /// the nibble minus one. Empty for stage 3.
    pub groups: [u8; 2],
    /// How many `$0600` records `$65` allocates: `(low != 0) + (high != 0)`.
    pub records: u8,
}

/// `$A460`: fields written for a raw five-byte stream record.
pub fn decode(stage: usize, bytes: [u8; 5]) -> Spawn {
    if stage == 2 {
        Spawn {
            subtype: bytes[1].wrapping_sub(0x70),
            x: 0xF0,
            kind: 0x96,
            y: bytes[2],
            byte_bc: bytes[3],
            byte_ec: bytes[4],
            groups: [0; 2],
            records: 0,
        }
    } else {
        let low = bytes[2] & 0x0F;
        let high = bytes[2] >> 4;
        let mut groups = [0; 2];
        let mut records = 0u8;
        for nibble in [low, high] {
            if nibble != 0 {
                groups[usize::from(records)] = nibble;
                records += 1;
            }
        }
        Spawn {
            // `$A4DB`: the large-object layout hardcodes `$010C = $80`.
            subtype: 0x80,
            x: 0xF0,
            kind: bytes[3],
            y: bytes[4],
            byte_bc: 0,
            byte_ec: 0,
            groups,
            records,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_three_uses_the_second_layout() {
        let spawn = decode(2, [0x9C, 0xF1, 0x60, 0x21, 0xC5]);
        assert_eq!(
            spawn,
            Spawn {
                subtype: 0x81,
                x: 0xF0,
                kind: 0x96,
                y: 0x60,
                byte_bc: 0x21,
                byte_ec: 0xC5,
                groups: [0; 2],
                records: 0,
            }
        );
    }

    #[test]
    fn large_object_nibbles_are_attachment_records() {
        let single = decode(4, [0x40, 0xF0, 0x01, 0x14, 0x80]);
        assert_eq!((single.kind, single.y), (0x14, 0x80));
        assert_eq!(single.subtype, 0x80);
        assert_eq!(single.groups, [1, 0]);
        assert_eq!(single.records, 1);
        assert_eq!(single.groups[0] - 1, 0, "$0601 is the nibble minus one");

        let double = decode(4, [0x80, 0xF0, 0x12, 0x14, 0x40]);
        assert_eq!((double.kind, double.y), (0x14, 0x40));
        assert_eq!(double.groups, [2, 1], "low nibble is allocated first");
        assert_eq!(double.records, 2);

        let high_only = decode(4, [0xF0, 0xF0, 0x20, 0x14, 0x60]);
        assert_eq!(high_only.groups, [2, 0]);
        assert_eq!(high_only.records, 1);

        let none = decode(4, [0xF0, 0xF0, 0x00, 0x14, 0x60]);
        assert_eq!(none.groups, [0; 2]);
        assert_eq!(none.records, 0);
    }
}
