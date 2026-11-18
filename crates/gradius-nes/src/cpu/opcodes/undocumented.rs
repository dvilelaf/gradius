//! Undocumented opcodes used by NES games, plus JAM/KIL.
//!
//! Part of the opcode table, split out of `execute` unchanged.

use super::super::*;

impl Cpu {
    pub(super) fn undocumented(&mut self, opcode: u8, bus: &mut impl Bus) -> u32 {
        use super::super::FLAG_C as C;

        match opcode {
            // --- Documented undocumented opcodes used by many NES games --
            0x04 | 0x44 | 0x64 => {
                let _ = self.zp(bus);
                3
            }
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => {
                let _ = self.zpx(bus);
                4
            }
            0x0C => {
                let _ = self.abs(bus);
                4
            }
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                let (_, p) = self.absx(bus);
                4 + u32::from(p)
            }
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {
                let _ = self.imm(bus);
                2
            }

            // LAX
            0xA7 => {
                let addr = self.zp(bus);
                let v = bus.read(addr);
                self.a = v;
                self.x = v;
                self.set_zn(v);
                3
            }
            0xB7 => {
                let addr = self.zpy(bus);
                let v = bus.read(addr);
                self.a = v;
                self.x = v;
                self.set_zn(v);
                4
            }
            0xAF => {
                let addr = self.abs(bus);
                let v = bus.read(addr);
                self.a = v;
                self.x = v;
                self.set_zn(v);
                4
            }
            0xBF => {
                let (addr, p) = self.absy(bus);
                let v = bus.read(addr);
                self.a = v;
                self.x = v;
                self.set_zn(v);
                4 + u32::from(p)
            }
            0xA3 => {
                let addr = self.indx(bus);
                let v = bus.read(addr);
                self.a = v;
                self.x = v;
                self.set_zn(v);
                6
            }
            0xB3 => {
                let (addr, p) = self.indy(bus);
                let v = bus.read(addr);
                self.a = v;
                self.x = v;
                self.set_zn(v);
                5 + u32::from(p)
            }
            0xAB => {
                let v = self.imm(bus);
                self.a = v;
                self.x = v;
                self.set_zn(v);
                2
            }
            // SAX
            0x87 => {
                let addr = self.zp(bus);
                bus.write(addr, self.a & self.x);
                3
            }
            0x97 => {
                let addr = self.zpy(bus);
                bus.write(addr, self.a & self.x);
                4
            }
            0x8F => {
                let addr = self.abs(bus);
                bus.write(addr, self.a & self.x);
                4
            }
            0x83 => {
                let addr = self.indx(bus);
                bus.write(addr, self.a & self.x);
                6
            }
            // DCP
            0xC7 => {
                let addr = self.zp(bus);
                self.dcp(bus, addr);
                5
            }
            0xD7 => {
                let addr = self.zpx(bus);
                self.dcp(bus, addr);
                6
            }
            0xCF => {
                let addr = self.abs(bus);
                self.dcp(bus, addr);
                6
            }
            0xDF => {
                let (addr, _) = self.absx(bus);
                self.dcp(bus, addr);
                7
            }
            0xDB => {
                let (addr, _) = self.absy(bus);
                self.dcp(bus, addr);
                7
            }
            0xC3 => {
                let addr = self.indx(bus);
                self.dcp(bus, addr);
                8
            }
            0xD3 => {
                let (addr, _) = self.indy(bus);
                self.dcp(bus, addr);
                8
            }
            // ISC
            0xE7 => {
                let addr = self.zp(bus);
                self.isc(bus, addr);
                5
            }
            0xF7 => {
                let addr = self.zpx(bus);
                self.isc(bus, addr);
                6
            }
            0xEF => {
                let addr = self.abs(bus);
                self.isc(bus, addr);
                6
            }
            0xFF => {
                let (addr, _) = self.absx(bus);
                self.isc(bus, addr);
                7
            }
            0xFB => {
                let (addr, _) = self.absy(bus);
                self.isc(bus, addr);
                7
            }
            0xE3 => {
                let addr = self.indx(bus);
                self.isc(bus, addr);
                8
            }
            0xF3 => {
                let (addr, _) = self.indy(bus);
                self.isc(bus, addr);
                8
            }
            // SLO
            0x07 => {
                let addr = self.zp(bus);
                self.slo(bus, addr);
                5
            }
            0x17 => {
                let addr = self.zpx(bus);
                self.slo(bus, addr);
                6
            }
            0x0F => {
                let addr = self.abs(bus);
                self.slo(bus, addr);
                6
            }
            0x1F => {
                let (addr, _) = self.absx(bus);
                self.slo(bus, addr);
                7
            }
            0x1B => {
                let (addr, _) = self.absy(bus);
                self.slo(bus, addr);
                7
            }
            0x03 => {
                let addr = self.indx(bus);
                self.slo(bus, addr);
                8
            }
            0x13 => {
                let (addr, _) = self.indy(bus);
                self.slo(bus, addr);
                8
            }
            // RLA
            0x27 => {
                let addr = self.zp(bus);
                self.rla(bus, addr);
                5
            }
            0x37 => {
                let addr = self.zpx(bus);
                self.rla(bus, addr);
                6
            }
            0x2F => {
                let addr = self.abs(bus);
                self.rla(bus, addr);
                6
            }
            0x3F => {
                let (addr, _) = self.absx(bus);
                self.rla(bus, addr);
                7
            }
            0x3B => {
                let (addr, _) = self.absy(bus);
                self.rla(bus, addr);
                7
            }
            0x23 => {
                let addr = self.indx(bus);
                self.rla(bus, addr);
                8
            }
            0x33 => {
                let (addr, _) = self.indy(bus);
                self.rla(bus, addr);
                8
            }
            // SRE
            0x47 => {
                let addr = self.zp(bus);
                self.sre(bus, addr);
                5
            }
            0x57 => {
                let addr = self.zpx(bus);
                self.sre(bus, addr);
                6
            }
            0x4F => {
                let addr = self.abs(bus);
                self.sre(bus, addr);
                6
            }
            0x5F => {
                let (addr, _) = self.absx(bus);
                self.sre(bus, addr);
                7
            }
            0x5B => {
                let (addr, _) = self.absy(bus);
                self.sre(bus, addr);
                7
            }
            0x43 => {
                let addr = self.indx(bus);
                self.sre(bus, addr);
                8
            }
            0x53 => {
                let (addr, _) = self.indy(bus);
                self.sre(bus, addr);
                8
            }
            // RRA
            0x67 => {
                let addr = self.zp(bus);
                self.rra(bus, addr);
                5
            }
            0x77 => {
                let addr = self.zpx(bus);
                self.rra(bus, addr);
                6
            }
            0x6F => {
                let addr = self.abs(bus);
                self.rra(bus, addr);
                6
            }
            0x7F => {
                let (addr, _) = self.absx(bus);
                self.rra(bus, addr);
                7
            }
            0x7B => {
                let (addr, _) = self.absy(bus);
                self.rra(bus, addr);
                7
            }
            0x63 => {
                let addr = self.indx(bus);
                self.rra(bus, addr);
                8
            }
            0x73 => {
                let (addr, _) = self.indy(bus);
                self.rra(bus, addr);
                8
            }
            // ANC / ALR / ARR / SBX
            0x0B | 0x2B => {
                let v = self.imm(bus);
                self.and(v);
                self.set_flag(C, self.a & 0x80 != 0);
                2
            }
            0x4B => {
                let v = self.imm(bus);
                self.and(v);
                let a = self.a;
                self.a = self.lsr_value(a);
                2
            }
            0x6B => {
                let v = self.imm(bus);
                self.arr(v);
                2
            }
            0xCB => {
                let v = self.imm(bus);
                let result = self.a.wrapping_sub(v) & self.x;
                self.x = result;
                self.set_zn(result);
                self.set_flag(C, self.a >= v);
                2
            }

            // --- JAM / KIL: halt the core, the oracle records it ---------
            0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xB2 | 0xD2 | 0xF2 => {
                self.jammed = true;
                self.jam_count = self.jam_count.saturating_add(1);
                2
            }
            _ => self.record_unknown_opcode(opcode),
        }
    }
}
