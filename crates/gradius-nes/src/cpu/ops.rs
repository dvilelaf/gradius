//! The operations the opcode arms call: loads, arithmetic and the
//! read-modify-write combinations behind the undocumented opcodes.

use super::*;

impl Cpu {
    pub(super) fn lda(&mut self, value: u8) {
        self.a = value;
        self.set_zn(value);
    }

    pub(super) fn ldx(&mut self, value: u8) {
        self.x = value;
        self.set_zn(value);
    }

    pub(super) fn ldy(&mut self, value: u8) {
        self.y = value;
        self.set_zn(value);
    }

    pub(super) fn adc(&mut self, value: u8) {
        let a = self.a;
        let carry = u16::from(self.flag(FLAG_C));
        let sum = u16::from(a) + u16::from(value) + carry;
        let result = sum as u8;
        self.set_flag(FLAG_C, sum > 0xFF);
        self.set_flag(FLAG_V, (!(a ^ value) & (a ^ result) & 0x80) != 0);
        self.a = result;
        self.set_zn(result);
    }

    pub(super) fn sbc(&mut self, value: u8) {
        self.adc(!value);
    }

    pub(super) fn compare(&mut self, register: u8, value: u8) {
        self.set_flag(FLAG_C, register >= value);
        self.set_zn(register.wrapping_sub(value));
    }

    pub(super) fn and(&mut self, value: u8) {
        self.a &= value;
        self.set_zn(self.a);
    }

    pub(super) fn ora(&mut self, value: u8) {
        self.a |= value;
        self.set_zn(self.a);
    }

    pub(super) fn eor(&mut self, value: u8) {
        self.a ^= value;
        self.set_zn(self.a);
    }

    pub(super) fn asl_value(&mut self, value: u8) -> u8 {
        self.set_flag(FLAG_C, value & 0x80 != 0);
        let result = value << 1;
        self.set_zn(result);
        result
    }

    pub(super) fn lsr_value(&mut self, value: u8) -> u8 {
        self.set_flag(FLAG_C, value & 0x01 != 0);
        let result = value >> 1;
        self.set_zn(result);
        result
    }

    pub(super) fn rol_value(&mut self, value: u8) -> u8 {
        let carry = u8::from(self.flag(FLAG_C));
        self.set_flag(FLAG_C, value & 0x80 != 0);
        let result = (value << 1) | carry;
        self.set_zn(result);
        result
    }

    pub(super) fn ror_value(&mut self, value: u8) -> u8 {
        let carry = if self.flag(FLAG_C) { 0x80 } else { 0 };
        self.set_flag(FLAG_C, value & 0x01 != 0);
        let result = (value >> 1) | carry;
        self.set_zn(result);
        result
    }

    pub(super) fn bit(&mut self, value: u8) {
        self.set_flag(FLAG_Z, self.a & value == 0);
        self.set_flag(FLAG_N, value & 0x80 != 0);
        self.set_flag(FLAG_V, value & 0x40 != 0);
    }

    pub(super) fn arr(&mut self, value: u8) {
        self.a &= value;
        self.a = self.ror_value(self.a);
        let a = self.a;
        self.set_flag(FLAG_C, a & 0x40 != 0);
        self.set_flag(FLAG_V, ((a >> 6) ^ (a >> 5)) & 1 != 0);
    }

    pub(super) fn dcp(&mut self, bus: &mut impl Bus, addr: u16) {
        let value = bus.read(addr).wrapping_sub(1);
        bus.write(addr, value);
        let a = self.a;
        self.compare(a, value);
    }

    pub(super) fn isc(&mut self, bus: &mut impl Bus, addr: u16) {
        let value = bus.read(addr).wrapping_add(1);
        bus.write(addr, value);
        self.sbc(value);
    }

    pub(super) fn slo(&mut self, bus: &mut impl Bus, addr: u16) {
        let value = bus.read(addr);
        let shifted = self.asl_value(value);
        bus.write(addr, shifted);
        self.ora(shifted);
    }

    pub(super) fn rla(&mut self, bus: &mut impl Bus, addr: u16) {
        let value = bus.read(addr);
        let rotated = self.rol_value(value);
        bus.write(addr, rotated);
        self.and(rotated);
    }

    pub(super) fn sre(&mut self, bus: &mut impl Bus, addr: u16) {
        let value = bus.read(addr);
        let shifted = self.lsr_value(value);
        bus.write(addr, shifted);
        self.eor(shifted);
    }

    pub(super) fn rra(&mut self, bus: &mut impl Bus, addr: u16) {
        let value = bus.read(addr);
        let rotated = self.ror_value(value);
        bus.write(addr, rotated);
        self.adc(rotated);
    }
}
