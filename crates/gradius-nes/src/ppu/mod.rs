//! A small, deterministic PPU model.
//!
//! It implements what the oracle needs: the CPU-visible registers, nametable
//! and palette memory, CHR bank selection (mapper 3), background scrolling
//! with the loopy `v`/`t` registers, sprites with the 8-per-line limit,
//! sprite-0 hit and vblank/NMI timing. Rendering happens once per scanline so
//! mid-frame scroll splits work the way the game expects.
//!
//! This is not a cycle-exact PPU. It is precise enough to boot and render the
//! game frame by frame and to compare state with the native implementation.

mod registers;
mod render;
#[cfg(test)]
mod tests;

use gradius_cart::Mirroring;

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;
pub const FRAMEBUFFER_LEN: usize = WIDTH * HEIGHT * 4;
/// Number of OAM bytes exposed by [`PpuSnapshot`].
pub const SNAPSHOT_OAM_LEN: usize = 32;

/// Where sprite 0 first met an opaque background pixel in a frame.
///
/// `line` is the visible scanline (0-239) and `dot` is the pixel output dot
/// (1-256) at which the overlap is detected. The HUD split in Gradius waits
/// for this event, so recording it lets differential tooling check that the
/// flag is raised late in the expected line instead of anywhere on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteZeroHit {
    pub line: usize,
    pub dot: u16,
}

/// A read-only view of the PPU state, for analysis tooling.
///
/// This intentionally exposes the fields a differential debugger needs
/// without handing out mutable access to the PPU internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpuSnapshot {
    pub ctrl: u8,
    pub mask: u8,
    pub status: u8,
    pub chr_bank: u8,
    pub w: bool,
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    pub palette: [u8; 32],
    /// First [`SNAPSHOT_OAM_LEN`] bytes of OAM, in hardware order (Y, tile,
    /// attributes, X per sprite).
    pub oam: [u8; SNAPSHOT_OAM_LEN],
    /// FNV-1a over the 4 KiB of internal VRAM (the two physical nametables
    /// including attribute tables).
    pub nametable_checksum: u64,
    /// First sprite-0 hit of the frame, or `None` if there was none.
    pub sprite_zero_hit: Option<SpriteZeroHit>,
}

/// One CPU write to VRAM (nametable or palette) through `$2007`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VramWrite {
    /// Video frame in which the write happened.
    pub frame: u64,
    /// Scanline (0-261) in which the write happened.
    pub line: usize,
    /// PPU address written, before the auto-increment.
    pub addr: u16,
    /// Value written.
    pub value: u8,
}

/// Hard cap on retained VRAM writes.
pub const MAX_CAPTURED_VRAM_WRITES: usize = 65_536;

/// FNV-1a hash used by [`PpuSnapshot`] and the emulator's frame hash.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

#[derive(Clone, Debug)]
pub struct Ppu {
    pub ctrl: u8,
    pub mask: u8,
    pub status: u8,
    pub oam: [u8; 256],
    pub oam_addr: u8,
    /// Current VRAM address (15 bits).
    pub v: u16,
    /// Temporary VRAM address (15 bits).
    pub t: u16,
    pub fine_x: u8,
    pub w: bool,
    pub read_buffer: u8,
    pub vram: Vec<u8>,
    pub palette: [u8; 32],
    pub chr: Vec<u8>,
    pub chr_bank: u8,
    pub mirroring: Mirroring,

    /// Frame and scanline, maintained by the emulator for write timestamping.
    pub frame: u64,
    pub line: usize,
    /// First sprite-0 overlap of the current frame, kept for debug tooling.
    pub sprite_zero_hit: Option<SpriteZeroHit>,
    /// Number of `$2002` reads that observed the sprite-0 flag (`bit 6`) set.
    ///
    /// The HUD split polls this flag, so the count is direct evidence that the
    /// wait loops were satisfied instead of spinning. It is cumulative for the
    /// lifetime of the PPU and never cleared by `end_vblank`.
    pub sprite_zero_status_reads: u64,
    /// Opt in to VRAM write capture.
    pub capture_vram: bool,
    pub vram_writes: Vec<VramWrite>,
    /// Writes evicted from the bounded capture buffer.
    pub vram_writes_dropped: u64,
}
