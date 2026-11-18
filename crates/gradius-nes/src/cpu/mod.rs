//! A deterministic 6502 core.
//!
//! The CPU is only used by the analysis oracle (`gradius-nes`): it executes the
//! original ROM so that routines, constants and RAM behaviour can be observed.
//! It is not part of the shipped game.
//!
//! Cycle counts follow the documented 6502 timing, including page-crossing and
//! branch penalties. BCD arithmetic is not emulated (the NES leaves decimal
//! mode disabled); the flag still exists so code that saves/restores P behaves
//! identically.

mod address;
mod opcodes;
mod ops;
#[cfg(test)]
mod tests;

pub const FLAG_C: u8 = 0x01;
pub const FLAG_Z: u8 = 0x02;
pub const FLAG_I: u8 = 0x04;
pub const FLAG_D: u8 = 0x08;
pub const FLAG_B: u8 = 0x10;
pub const FLAG_U: u8 = 0x20;
pub const FLAG_V: u8 = 0x40;
pub const FLAG_N: u8 = 0x80;

/// Everything the CPU can reach. Implemented by the NES bus in this crate and
/// by a flat RAM stub in tests.
pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, value: u8);
    /// Called once per instruction fetch. The oracle uses it for coverage.
    fn on_exec(&mut self, _pc: u16, _opcode: u8) {}
}

#[derive(Clone, Debug)]
pub struct Cpu {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub p: u8,
    pub pc: u16,
    pub cycles: u64,
    /// Set when an unimplemented JAM/KIL opcode is executed.
    pub jammed: bool,
    /// Number of JAM/KIL opcodes executed.
    pub jam_count: u64,
    /// First opcode the core had no implementation for, if any.
    pub unknown_opcode: Option<u8>,
    /// Number of unknown opcodes executed.
    pub unknown_opcode_count: u64,
}

impl Cpu {
    pub fn new(reset_vector: u16) -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            p: FLAG_I | FLAG_U,
            pc: reset_vector,
            cycles: 0,
            jammed: false,
            jam_count: 0,
            unknown_opcode: None,
            unknown_opcode_count: 0,
        }
    }

    pub fn flag(&self, flag: u8) -> bool {
        self.p & flag != 0
    }

    fn set_flag(&mut self, flag: u8, on: bool) {
        if on {
            self.p |= flag;
        } else {
            self.p &= !flag;
        }
    }

    fn set_zn(&mut self, value: u8) {
        self.set_flag(FLAG_Z, value == 0);
        self.set_flag(FLAG_N, value & 0x80 != 0);
    }

    fn push(&mut self, bus: &mut impl Bus, value: u8) {
        bus.write(0x0100 | u16::from(self.sp), value);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pull(&mut self, bus: &mut impl Bus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 | u16::from(self.sp))
    }

    fn fetch(&mut self, bus: &mut impl Bus) -> u8 {
        let value = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    fn fetch16(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = u16::from(self.fetch(bus));
        let hi = u16::from(self.fetch(bus));
        lo | (hi << 8)
    }

    /// Trigger a non-maskable interrupt (the PPU calls this at scanline 241).
    pub fn nmi(&mut self, bus: &mut impl Bus) {
        let pc = self.pc;
        self.push(bus, (pc >> 8) as u8);
        self.push(bus, pc as u8);
        let p = (self.p | FLAG_U) & !FLAG_B;
        self.push(bus, p);
        self.set_flag(FLAG_I, true);
        self.pc = read_vector(bus, 0xFFFA);
        self.cycles += 7;
    }

    /// Trigger a maskable interrupt. Unused by the target game, kept for
    /// completeness of the core.
    pub fn irq(&mut self, bus: &mut impl Bus) {
        if self.flag(FLAG_I) {
            return;
        }
        let pc = self.pc;
        self.push(bus, (pc >> 8) as u8);
        self.push(bus, pc as u8);
        self.push(bus, (self.p | FLAG_U) & !FLAG_B);
        self.set_flag(FLAG_I, true);
        self.pc = read_vector(bus, 0xFFFE);
        self.cycles += 7;
    }

    /// Execute one instruction and return the cycles it consumed.
    pub fn step(&mut self, bus: &mut impl Bus) -> u32 {
        if self.jammed {
            // A halted core still burns cycles; otherwise a caller that waits
            // for a cycle budget would spin forever.
            self.cycles += 1;
            return 1;
        }
        let start_pc = self.pc;
        let opcode = self.fetch(bus);
        bus.on_exec(start_pc, opcode);
        let cycles = self.execute(opcode, bus);
        self.cycles += u64::from(cycles);
        cycles
    }
}

fn read_vector(bus: &mut impl Bus, addr: u16) -> u16 {
    let lo = u16::from(bus.read(addr));
    let hi = u16::from(bus.read(addr + 1));
    lo | (hi << 8)
}
