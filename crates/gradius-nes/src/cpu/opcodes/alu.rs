//! Arithmetic, logic, compares, shifts, inc/dec and transfers.
//!
//! Part of the opcode table, split out of `execute` unchanged.

use super::super::*;

impl Cpu {
    pub(super) fn alu(&mut self, opcode: u8, bus: &mut impl Bus) -> u32 {
        match opcode {
            // --- ALU -----------------------------------------------------
            0x69 => {
                let v = self.imm(bus);
                self.adc(v);
                2
            }
            0x65 => {
                let a = self.zp(bus);
                let v = bus.read(a);
                self.adc(v);
                3
            }
            0x75 => {
                let a = self.zpx(bus);
                let v = bus.read(a);
                self.adc(v);
                4
            }
            0x6D => {
                let a = self.abs(bus);
                let v = bus.read(a);
                self.adc(v);
                4
            }
            0x7D => {
                let (a, p) = self.absx(bus);
                let v = bus.read(a);
                self.adc(v);
                4 + u32::from(p)
            }
            0x79 => {
                let (a, p) = self.absy(bus);
                let v = bus.read(a);
                self.adc(v);
                4 + u32::from(p)
            }
            0x61 => {
                let a = self.indx(bus);
                let v = bus.read(a);
                self.adc(v);
                6
            }
            0x71 => {
                let (a, p) = self.indy(bus);
                let v = bus.read(a);
                self.adc(v);
                5 + u32::from(p)
            }
            0xE9 | 0xEB => {
                let v = self.imm(bus);
                self.sbc(v);
                2
            }
            0xE5 => {
                let a = self.zp(bus);
                let v = bus.read(a);
                self.sbc(v);
                3
            }
            0xF5 => {
                let a = self.zpx(bus);
                let v = bus.read(a);
                self.sbc(v);
                4
            }
            0xED => {
                let a = self.abs(bus);
                let v = bus.read(a);
                self.sbc(v);
                4
            }
            0xFD => {
                let (a, p) = self.absx(bus);
                let v = bus.read(a);
                self.sbc(v);
                4 + u32::from(p)
            }
            0xF9 => {
                let (a, p) = self.absy(bus);
                let v = bus.read(a);
                self.sbc(v);
                4 + u32::from(p)
            }
            0xE1 => {
                let a = self.indx(bus);
                let v = bus.read(a);
                self.sbc(v);
                6
            }
            0xF1 => {
                let (a, p) = self.indy(bus);
                let v = bus.read(a);
                self.sbc(v);
                5 + u32::from(p)
            }
            0x29 => {
                let v = self.imm(bus);
                self.and(v);
                2
            }
            0x25 => {
                let a = self.zp(bus);
                let v = bus.read(a);
                self.and(v);
                3
            }
            0x35 => {
                let a = self.zpx(bus);
                let v = bus.read(a);
                self.and(v);
                4
            }
            0x2D => {
                let a = self.abs(bus);
                let v = bus.read(a);
                self.and(v);
                4
            }
            0x3D => {
                let (a, p) = self.absx(bus);
                let v = bus.read(a);
                self.and(v);
                4 + u32::from(p)
            }
            0x39 => {
                let (a, p) = self.absy(bus);
                let v = bus.read(a);
                self.and(v);
                4 + u32::from(p)
            }
            0x21 => {
                let a = self.indx(bus);
                let v = bus.read(a);
                self.and(v);
                6
            }
            0x31 => {
                let (a, p) = self.indy(bus);
                let v = bus.read(a);
                self.and(v);
                5 + u32::from(p)
            }
            0x09 => {
                let v = self.imm(bus);
                self.ora(v);
                2
            }
            0x05 => {
                let a = self.zp(bus);
                let v = bus.read(a);
                self.ora(v);
                3
            }
            0x15 => {
                let a = self.zpx(bus);
                let v = bus.read(a);
                self.ora(v);
                4
            }
            0x0D => {
                let a = self.abs(bus);
                let v = bus.read(a);
                self.ora(v);
                4
            }
            0x1D => {
                let (a, p) = self.absx(bus);
                let v = bus.read(a);
                self.ora(v);
                4 + u32::from(p)
            }
            0x19 => {
                let (a, p) = self.absy(bus);
                let v = bus.read(a);
                self.ora(v);
                4 + u32::from(p)
            }
            0x01 => {
                let a = self.indx(bus);
                let v = bus.read(a);
                self.ora(v);
                6
            }
            0x11 => {
                let (a, p) = self.indy(bus);
                let v = bus.read(a);
                self.ora(v);
                5 + u32::from(p)
            }
            0x49 => {
                let v = self.imm(bus);
                self.eor(v);
                2
            }
            0x45 => {
                let a = self.zp(bus);
                let v = bus.read(a);
                self.eor(v);
                3
            }
            0x55 => {
                let a = self.zpx(bus);
                let v = bus.read(a);
                self.eor(v);
                4
            }
            0x4D => {
                let a = self.abs(bus);
                let v = bus.read(a);
                self.eor(v);
                4
            }
            0x5D => {
                let (a, p) = self.absx(bus);
                let v = bus.read(a);
                self.eor(v);
                4 + u32::from(p)
            }
            0x59 => {
                let (a, p) = self.absy(bus);
                let v = bus.read(a);
                self.eor(v);
                4 + u32::from(p)
            }
            0x41 => {
                let a = self.indx(bus);
                let v = bus.read(a);
                self.eor(v);
                6
            }
            0x51 => {
                let (a, p) = self.indy(bus);
                let v = bus.read(a);
                self.eor(v);
                5 + u32::from(p)
            }

            // --- Compares ------------------------------------------------
            0xC9 => {
                let v = self.imm(bus);
                let a = self.a;
                self.compare(a, v);
                2
            }
            0xC5 => {
                let addr = self.zp(bus);
                let v = bus.read(addr);
                let a = self.a;
                self.compare(a, v);
                3
            }
            0xD5 => {
                let addr = self.zpx(bus);
                let v = bus.read(addr);
                let a = self.a;
                self.compare(a, v);
                4
            }
            0xCD => {
                let addr = self.abs(bus);
                let v = bus.read(addr);
                let a = self.a;
                self.compare(a, v);
                4
            }
            0xDD => {
                let (addr, p) = self.absx(bus);
                let v = bus.read(addr);
                let a = self.a;
                self.compare(a, v);
                4 + u32::from(p)
            }
            0xD9 => {
                let (addr, p) = self.absy(bus);
                let v = bus.read(addr);
                let a = self.a;
                self.compare(a, v);
                4 + u32::from(p)
            }
            0xC1 => {
                let addr = self.indx(bus);
                let v = bus.read(addr);
                let a = self.a;
                self.compare(a, v);
                6
            }
            0xD1 => {
                let (addr, p) = self.indy(bus);
                let v = bus.read(addr);
                let a = self.a;
                self.compare(a, v);
                5 + u32::from(p)
            }
            0xE0 => {
                let v = self.imm(bus);
                let x = self.x;
                self.compare(x, v);
                2
            }
            0xE4 => {
                let addr = self.zp(bus);
                let v = bus.read(addr);
                let x = self.x;
                self.compare(x, v);
                3
            }
            0xEC => {
                let addr = self.abs(bus);
                let v = bus.read(addr);
                let x = self.x;
                self.compare(x, v);
                4
            }
            0xC0 => {
                let v = self.imm(bus);
                let y = self.y;
                self.compare(y, v);
                2
            }
            0xC4 => {
                let addr = self.zp(bus);
                let v = bus.read(addr);
                let y = self.y;
                self.compare(y, v);
                3
            }
            0xCC => {
                let addr = self.abs(bus);
                let v = bus.read(addr);
                let y = self.y;
                self.compare(y, v);
                4
            }

            // --- Shifts / rotates ---------------------------------------
            0x0A => {
                let a = self.a;
                self.a = self.asl_value(a);
                2
            }
            0x06 => {
                let addr = self.zp(bus);
                self.read_modify_write(bus, addr, Self::asl_value);
                5
            }
            0x16 => {
                let addr = self.zpx(bus);
                self.read_modify_write(bus, addr, Self::asl_value);
                6
            }
            0x0E => {
                let addr = self.abs(bus);
                self.read_modify_write(bus, addr, Self::asl_value);
                6
            }
            0x1E => {
                let (addr, _) = self.absx(bus);
                self.read_modify_write(bus, addr, Self::asl_value);
                7
            }
            0x4A => {
                let a = self.a;
                self.a = self.lsr_value(a);
                2
            }
            0x46 => {
                let addr = self.zp(bus);
                self.read_modify_write(bus, addr, Self::lsr_value);
                5
            }
            0x56 => {
                let addr = self.zpx(bus);
                self.read_modify_write(bus, addr, Self::lsr_value);
                6
            }
            0x4E => {
                let addr = self.abs(bus);
                self.read_modify_write(bus, addr, Self::lsr_value);
                6
            }
            0x5E => {
                let (addr, _) = self.absx(bus);
                self.read_modify_write(bus, addr, Self::lsr_value);
                7
            }
            0x2A => {
                let a = self.a;
                self.a = self.rol_value(a);
                2
            }
            0x26 => {
                let addr = self.zp(bus);
                self.read_modify_write(bus, addr, Self::rol_value);
                5
            }
            0x36 => {
                let addr = self.zpx(bus);
                self.read_modify_write(bus, addr, Self::rol_value);
                6
            }
            0x2E => {
                let addr = self.abs(bus);
                self.read_modify_write(bus, addr, Self::rol_value);
                6
            }
            0x3E => {
                let (addr, _) = self.absx(bus);
                self.read_modify_write(bus, addr, Self::rol_value);
                7
            }
            0x6A => {
                let a = self.a;
                self.a = self.ror_value(a);
                2
            }
            0x66 => {
                let addr = self.zp(bus);
                self.read_modify_write(bus, addr, Self::ror_value);
                5
            }
            0x76 => {
                let addr = self.zpx(bus);
                self.read_modify_write(bus, addr, Self::ror_value);
                6
            }
            0x6E => {
                let addr = self.abs(bus);
                self.read_modify_write(bus, addr, Self::ror_value);
                6
            }
            0x7E => {
                let (addr, _) = self.absx(bus);
                self.read_modify_write(bus, addr, Self::ror_value);
                7
            }

            // --- Inc / dec ----------------------------------------------
            0xE6 => {
                let addr = self.zp(bus);
                let v = bus.read(addr).wrapping_add(1);
                bus.write(addr, v);
                self.set_zn(v);
                5
            }
            0xF6 => {
                let addr = self.zpx(bus);
                let v = bus.read(addr).wrapping_add(1);
                bus.write(addr, v);
                self.set_zn(v);
                6
            }
            0xEE => {
                let addr = self.abs(bus);
                let v = bus.read(addr).wrapping_add(1);
                bus.write(addr, v);
                self.set_zn(v);
                6
            }
            0xFE => {
                let (addr, _) = self.absx(bus);
                let v = bus.read(addr).wrapping_add(1);
                bus.write(addr, v);
                self.set_zn(v);
                7
            }
            0xC6 => {
                let addr = self.zp(bus);
                let v = bus.read(addr).wrapping_sub(1);
                bus.write(addr, v);
                self.set_zn(v);
                5
            }
            0xD6 => {
                let addr = self.zpx(bus);
                let v = bus.read(addr).wrapping_sub(1);
                bus.write(addr, v);
                self.set_zn(v);
                6
            }
            0xCE => {
                let addr = self.abs(bus);
                let v = bus.read(addr).wrapping_sub(1);
                bus.write(addr, v);
                self.set_zn(v);
                6
            }
            0xDE => {
                let (addr, _) = self.absx(bus);
                let v = bus.read(addr).wrapping_sub(1);
                bus.write(addr, v);
                self.set_zn(v);
                7
            }
            0xE8 => {
                self.x = self.x.wrapping_add(1);
                let x = self.x;
                self.set_zn(x);
                2
            }
            0xC8 => {
                self.y = self.y.wrapping_add(1);
                let y = self.y;
                self.set_zn(y);
                2
            }
            0xCA => {
                self.x = self.x.wrapping_sub(1);
                let x = self.x;
                self.set_zn(x);
                2
            }
            0x88 => {
                self.y = self.y.wrapping_sub(1);
                let y = self.y;
                self.set_zn(y);
                2
            }

            // --- Transfers ----------------------------------------------
            0xAA => {
                let a = self.a;
                self.ldx(a);
                2
            }
            0x8A => {
                let x = self.x;
                self.lda(x);
                2
            }
            0xA8 => {
                let a = self.a;
                self.ldy(a);
                2
            }
            0x98 => {
                let y = self.y;
                self.lda(y);
                2
            }
            0xBA => {
                let sp = self.sp;
                self.ldx(sp);
                2
            }
            0x9A => {
                self.sp = self.x;
                2
            }
            _ => self.record_unknown_opcode(opcode),
        }
    }
}
