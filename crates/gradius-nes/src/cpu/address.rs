//! Addressing modes and the helpers that read them.

use super::*;

impl Cpu {
    pub(super) fn imm(&mut self, bus: &mut impl Bus) -> u8 {
        self.fetch(bus)
    }

    pub(super) fn zp(&mut self, bus: &mut impl Bus) -> u16 {
        u16::from(self.fetch(bus))
    }

    pub(super) fn zpx(&mut self, bus: &mut impl Bus) -> u16 {
        u16::from(self.fetch(bus).wrapping_add(self.x))
    }

    pub(super) fn zpy(&mut self, bus: &mut impl Bus) -> u16 {
        u16::from(self.fetch(bus).wrapping_add(self.y))
    }

    pub(super) fn abs(&mut self, bus: &mut impl Bus) -> u16 {
        self.fetch16(bus)
    }

    pub(super) fn absx(&mut self, bus: &mut impl Bus) -> (u16, bool) {
        let base = self.abs(bus);
        let addr = base.wrapping_add(u16::from(self.x));
        (addr, page_crossed(base, addr))
    }

    pub(super) fn absy(&mut self, bus: &mut impl Bus) -> (u16, bool) {
        let base = self.abs(bus);
        let addr = base.wrapping_add(u16::from(self.y));
        (addr, page_crossed(base, addr))
    }

    pub(super) fn indx(&mut self, bus: &mut impl Bus) -> u16 {
        let pointer = self.fetch(bus).wrapping_add(self.x);
        let lo = u16::from(bus.read(u16::from(pointer)));
        let hi = u16::from(bus.read(u16::from(pointer.wrapping_add(1))));
        lo | (hi << 8)
    }

    pub(super) fn indy(&mut self, bus: &mut impl Bus) -> (u16, bool) {
        let pointer = self.fetch(bus);
        let lo = u16::from(bus.read(u16::from(pointer)));
        let hi = u16::from(bus.read(u16::from(pointer.wrapping_add(1))));
        let base = lo | (hi << 8);
        let addr = base.wrapping_add(u16::from(self.y));
        (addr, page_crossed(base, addr))
    }

    pub(super) fn branch(&mut self, bus: &mut impl Bus, condition: bool) -> u32 {
        // The offset byte is part of the instruction and is consumed even when
        // the branch is not taken.
        let offset = self.fetch(bus) as i8;
        if !condition {
            return 2;
        }
        let target = self.pc.wrapping_add_signed(i16::from(offset));
        let extra = page_crossed(self.pc, target);
        self.pc = target;
        3 + u32::from(extra)
    }

    pub(super) fn read_modify_write(
        &mut self,
        bus: &mut impl Bus,
        addr: u16,
        op: fn(&mut Self, u8) -> u8,
    ) {
        let value = bus.read(addr);
        let result = op(self, value);
        bus.write(addr, result);
    }
}

fn page_crossed(from: u16, to: u16) -> bool {
    from & 0xFF00 != to & 0xFF00
}
