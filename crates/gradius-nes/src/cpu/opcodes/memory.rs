//! Loads, stores and BIT: the instructions that touch memory.
//!
//! Part of the opcode table, split out of `execute` unchanged.

use super::super::*;

impl Cpu {
    pub(super) fn memory(&mut self, opcode: u8, bus: &mut impl Bus) -> u32 {
        match opcode {
            // --- Loads ---------------------------------------------------
            0xA9 => {
                let v = self.imm(bus);
                self.lda(v);
                2
            }
            0xA5 => {
                let a = self.zp(bus);
                let v = bus.read(a);
                self.lda(v);
                3
            }
            0xB5 => {
                let a = self.zpx(bus);
                let v = bus.read(a);
                self.lda(v);
                4
            }
            0xAD => {
                let a = self.abs(bus);
                let v = bus.read(a);
                self.lda(v);
                4
            }
            0xBD => {
                let (a, p) = self.absx(bus);
                let v = bus.read(a);
                self.lda(v);
                4 + u32::from(p)
            }
            0xB9 => {
                let (a, p) = self.absy(bus);
                let v = bus.read(a);
                self.lda(v);
                4 + u32::from(p)
            }
            0xA1 => {
                let a = self.indx(bus);
                let v = bus.read(a);
                self.lda(v);
                6
            }
            0xB1 => {
                let (a, p) = self.indy(bus);
                let v = bus.read(a);
                self.lda(v);
                5 + u32::from(p)
            }
            0xA2 => {
                let v = self.imm(bus);
                self.ldx(v);
                2
            }
            0xA6 => {
                let a = self.zp(bus);
                let v = bus.read(a);
                self.ldx(v);
                3
            }
            0xB6 => {
                let a = self.zpy(bus);
                let v = bus.read(a);
                self.ldx(v);
                4
            }
            0xAE => {
                let a = self.abs(bus);
                let v = bus.read(a);
                self.ldx(v);
                4
            }
            0xBE => {
                let (a, p) = self.absy(bus);
                let v = bus.read(a);
                self.ldx(v);
                4 + u32::from(p)
            }
            0xA0 => {
                let v = self.imm(bus);
                self.ldy(v);
                2
            }
            0xA4 => {
                let a = self.zp(bus);
                let v = bus.read(a);
                self.ldy(v);
                3
            }
            0xB4 => {
                let a = self.zpx(bus);
                let v = bus.read(a);
                self.ldy(v);
                4
            }
            0xAC => {
                let a = self.abs(bus);
                let v = bus.read(a);
                self.ldy(v);
                4
            }
            0xBC => {
                let (a, p) = self.absx(bus);
                let v = bus.read(a);
                self.ldy(v);
                4 + u32::from(p)
            }

            // --- Stores --------------------------------------------------
            0x85 => {
                let a = self.zp(bus);
                bus.write(a, self.a);
                3
            }
            0x95 => {
                let a = self.zpx(bus);
                bus.write(a, self.a);
                4
            }
            0x8D => {
                let a = self.abs(bus);
                bus.write(a, self.a);
                4
            }
            0x9D => {
                let (a, _) = self.absx(bus);
                bus.write(a, self.a);
                5
            }
            0x99 => {
                let (a, _) = self.absy(bus);
                bus.write(a, self.a);
                5
            }
            0x81 => {
                let a = self.indx(bus);
                bus.write(a, self.a);
                6
            }
            0x91 => {
                let (a, _) = self.indy(bus);
                bus.write(a, self.a);
                6
            }
            0x86 => {
                let a = self.zp(bus);
                bus.write(a, self.x);
                3
            }
            0x96 => {
                let a = self.zpy(bus);
                bus.write(a, self.x);
                4
            }
            0x8E => {
                let a = self.abs(bus);
                bus.write(a, self.x);
                4
            }
            0x84 => {
                let a = self.zp(bus);
                bus.write(a, self.y);
                3
            }
            0x94 => {
                let a = self.zpx(bus);
                bus.write(a, self.y);
                4
            }
            0x8C => {
                let a = self.abs(bus);
                bus.write(a, self.y);
                4
            }

            // --- BIT -----------------------------------------------------
            0x24 => {
                let addr = self.zp(bus);
                let v = bus.read(addr);
                self.bit(v);
                3
            }
            0x2C => {
                let addr = self.abs(bus);
                let v = bus.read(addr);
                self.bit(v);
                4
            }
            _ => self.record_unknown_opcode(opcode),
        }
    }
}
