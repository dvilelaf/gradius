//! Control flow, branches, flag and stack instructions.
//!
//! Part of the opcode table, split out of `execute` unchanged.

use super::super::*;

impl Cpu {
    pub(super) fn control(&mut self, opcode: u8, bus: &mut impl Bus) -> u32 {
        use super::super::FLAG_C as C;
        use super::super::FLAG_D as D;
        use super::super::FLAG_I as I;
        use super::super::FLAG_V as V;

        match opcode {
            // --- Control -------------------------------------------------
            0x00 => {
                // BRK
                let pc = self.pc.wrapping_add(1);
                self.push(bus, (pc >> 8) as u8);
                self.push(bus, pc as u8);
                self.push(bus, self.p | FLAG_B | FLAG_U);
                self.set_flag(I, true);
                self.pc = read_vector(bus, 0xFFFE);
                7
            }
            0x20 => {
                // JSR
                let target = self.fetch16(bus);
                let ret = self.pc.wrapping_sub(1);
                self.push(bus, (ret >> 8) as u8);
                self.push(bus, ret as u8);
                self.pc = target;
                6
            }
            0x60 => {
                // RTS
                let lo = u16::from(self.pull(bus));
                let hi = u16::from(self.pull(bus));
                self.pc = (lo | (hi << 8)).wrapping_add(1);
                6
            }
            0x40 => {
                // RTI
                self.p = (self.pull(bus) | FLAG_U) & !FLAG_B;
                let lo = u16::from(self.pull(bus));
                let hi = u16::from(self.pull(bus));
                self.pc = lo | (hi << 8);
                6
            }
            0x4C => {
                self.pc = self.fetch16(bus);
                3
            }
            0x6C => {
                // JMP (indirect), with the documented page-boundary bug.
                let pointer = self.fetch16(bus);
                let lo = u16::from(bus.read(pointer));
                let hi_addr = (pointer & 0xFF00) | u16::from((pointer as u8).wrapping_add(1));
                let hi = u16::from(bus.read(hi_addr));
                self.pc = lo | (hi << 8);
                5
            }

            // --- Branches ------------------------------------------------
            0x10 => self.branch(bus, !self.flag(FLAG_N)),
            0x30 => self.branch(bus, self.flag(FLAG_N)),
            0x50 => self.branch(bus, !self.flag(V)),
            0x70 => self.branch(bus, self.flag(V)),
            0x90 => self.branch(bus, !self.flag(C)),
            0xB0 => self.branch(bus, self.flag(C)),
            0xD0 => self.branch(bus, !self.flag(FLAG_Z)),
            0xF0 => self.branch(bus, self.flag(FLAG_Z)),

            // --- Flags / stack ------------------------------------------
            0x18 => {
                self.set_flag(C, false);
                2
            }
            0x38 => {
                self.set_flag(C, true);
                2
            }
            0x58 => {
                self.set_flag(I, false);
                2
            }
            0x78 => {
                self.set_flag(I, true);
                2
            }
            0xB8 => {
                self.set_flag(V, false);
                2
            }
            0xD8 => {
                self.set_flag(D, false);
                2
            }
            0xF8 => {
                self.set_flag(D, true);
                2
            }
            0x48 => {
                let a = self.a;
                self.push(bus, a);
                3
            }
            0x08 => {
                let p = self.p | FLAG_B | FLAG_U;
                self.push(bus, p);
                3
            }
            0x68 => {
                let v = self.pull(bus);
                self.lda(v);
                4
            }
            0x28 => {
                self.p = (self.pull(bus) | FLAG_U) & !FLAG_B;
                4
            }

            // --- NOP -----------------------------------------------------
            0xEA | 0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => 2,
            _ => self.record_unknown_opcode(opcode),
        }
    }
}
