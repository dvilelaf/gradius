//! The CPU-visible side: `$2000`-`$2007`, VRAM, palette and the CHR window.

use super::*;
use gradius_cart::Cartridge;

impl Ppu {
    pub fn new(cart: &Cartridge) -> Self {
        Self {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam: [0; 256],
            oam_addr: 0,
            v: 0,
            t: 0,
            fine_x: 0,
            w: false,
            read_buffer: 0,
            vram: vec![0; 0x1000],
            palette: [0x0F; 32],
            chr: cart.chr.clone(),
            chr_bank: 0,
            mirroring: cart.header.mirroring,
            frame: 0,
            line: 0,
            sprite_zero_hit: None,
            sprite_zero_status_reads: 0,
            capture_vram: false,
            vram_writes: Vec::new(),
            vram_writes_dropped: 0,
        }
    }

    pub fn start_vram_capture(&mut self) {
        self.capture_vram = true;
    }

    pub fn stop_vram_capture(&mut self) {
        self.capture_vram = false;
    }

    pub fn clear_vram_writes(&mut self) {
        self.vram_writes.clear();
        self.vram_writes_dropped = 0;
    }

    pub fn vram_writes(&self) -> &[VramWrite] {
        &self.vram_writes
    }

    pub fn set_chr_bank(&mut self, bank: u8) {
        self.chr_bank = bank;
    }

    pub fn snapshot(&self) -> PpuSnapshot {
        let mut oam = [0u8; SNAPSHOT_OAM_LEN];
        oam.copy_from_slice(&self.oam[..SNAPSHOT_OAM_LEN]);
        PpuSnapshot {
            ctrl: self.ctrl,
            mask: self.mask,
            status: self.status,
            chr_bank: self.chr_bank,
            w: self.w,
            v: self.v,
            t: self.t,
            fine_x: self.fine_x,
            palette: self.palette,
            oam,
            nametable_checksum: fnv1a(&self.vram),
            sprite_zero_hit: self.sprite_zero_hit,
        }
    }

    /// Copy the scroll latch into the current VRAM address.
    ///
    /// The real PPU reloads the horizontal bits of `v` at dot 257 of the
    /// pre-render line and the vertical bits during dots 280-304, so by the
    /// time the first visible line is fetched `v` equals `t`. Doing this only
    /// at the end of visible lines loses scroll writes made during vblank.
    pub fn reload_scroll(&mut self) {
        self.v = self.t;
    }

    pub fn nmi_enabled(&self) -> bool {
        self.ctrl & 0x80 != 0
    }

    pub fn start_vblank(&mut self) {
        self.status |= 0x80;
    }

    pub fn end_vblank(&mut self) {
        self.status &= !0x80;
        self.status &= !0x40;
        self.status &= !0x20;
        self.sprite_zero_hit = None;
    }

    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr & 7 {
            2 => {
                let value = self.status;
                if value & 0x40 != 0 {
                    self.sprite_zero_status_reads += 1;
                }
                self.status &= !0x80;
                self.w = false;
                value
            }
            4 => self.oam[usize::from(self.oam_addr)],
            7 => {
                let addr = self.v & 0x3FFF;
                let value = if addr >= 0x3F00 {
                    let palette = self.read_palette(addr);
                    // Palette reads bypass the buffer, which is filled from the
                    // nametable byte underneath the palette space.
                    self.read_buffer = self.read_vram(addr - 0x1000);
                    palette
                } else {
                    let buffered = self.read_buffer;
                    self.read_buffer = self.read_vram(addr);
                    buffered
                };
                self.increment_v();
                value
            }
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr & 7 {
            0 => {
                self.ctrl = value;
                self.t = (self.t & !0x0C00) | (u16::from(value & 0x03) << 10);
            }
            1 => self.mask = value,
            3 => self.oam_addr = value,
            4 => {
                self.oam[usize::from(self.oam_addr)] = value;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            5 => {
                if self.w {
                    self.t = (self.t & !0x73E0)
                        | (u16::from(value & 0x07) << 12)
                        | (u16::from(value & 0xF8) << 2);
                } else {
                    self.fine_x = value & 0x07;
                    self.t = (self.t & !0x001F) | (u16::from(value) >> 3);
                }
                self.w = !self.w;
            }
            6 => {
                if self.w {
                    self.t = (self.t & !0x00FF) | u16::from(value);
                    self.v = self.t;
                } else {
                    self.t = (self.t & 0x00FF) | (u16::from(value & 0x3F) << 8);
                }
                self.w = !self.w;
            }
            7 => {
                let addr = self.v & 0x3FFF;
                if self.capture_vram && addr >= 0x2000 {
                    if self.vram_writes.len() >= MAX_CAPTURED_VRAM_WRITES {
                        let drop = MAX_CAPTURED_VRAM_WRITES / 4;
                        self.vram_writes.drain(..drop);
                        self.vram_writes_dropped += drop as u64;
                    }
                    self.vram_writes.push(VramWrite {
                        frame: self.frame,
                        line: self.line,
                        addr,
                        value,
                    });
                }
                if addr >= 0x3F00 {
                    self.write_palette(addr, value);
                } else {
                    self.write_vram(addr, value);
                }
                self.increment_v();
            }
            _ => {}
        }
    }

    pub(super) fn increment_v(&mut self) {
        let step = if self.ctrl & 0x04 != 0 { 32 } else { 1 };
        self.v = self.v.wrapping_add(step) & 0x7FFF;
    }

    pub(super) fn read_palette(&self, addr: u16) -> u8 {
        let index = palette_index(addr);
        self.palette[index]
    }

    pub(super) fn write_palette(&mut self, addr: u16, value: u8) {
        let index = palette_index(addr);
        self.palette[index] = value & 0x3F;
    }

    pub(super) fn nametable_offset(&self, addr: u16) -> usize {
        let table = ((addr >> 10) & 0x03) as usize;
        let offset = usize::from(addr & 0x03FF);
        let physical = match self.mirroring {
            Mirroring::Vertical => match table {
                0 | 2 => 0,
                _ => 1,
            },
            Mirroring::Horizontal => match table {
                0 | 1 => 0,
                _ => 1,
            },
            Mirroring::FourScreen => table,
            Mirroring::SingleScreenLower => 0,
            Mirroring::SingleScreenUpper => 1,
        };
        physical * 0x400 + offset
    }

    pub(super) fn read_vram(&self, addr: u16) -> u8 {
        if addr >= 0x3F00 {
            self.read_palette(addr)
        } else if addr >= 0x2000 {
            self.vram[self.nametable_offset(addr)]
        } else {
            self.read_pattern(addr)
        }
    }

    pub(super) fn write_vram(&mut self, addr: u16, value: u8) {
        if addr >= 0x3F00 {
            self.write_palette(addr, value);
        } else if addr >= 0x2000 {
            let offset = self.nametable_offset(addr);
            self.vram[offset] = value;
        }
        // Pattern space is CHR ROM on CNROM; writes are ignored.
    }

    /// Read a byte from the CHR bank currently selected by the mapper.
    pub(super) fn read_pattern(&self, addr: u16) -> u8 {
        let offset = usize::from(self.chr_bank) * 0x2000 + usize::from(addr & 0x1FFF);
        self.chr.get(offset).copied().unwrap_or(0)
    }
}

fn palette_index(addr: u16) -> usize {
    let mut index = usize::from(addr & 0x1F);
    if index >= 0x10 && index & 0x03 == 0 {
        index -= 0x10;
    }
    index
}
