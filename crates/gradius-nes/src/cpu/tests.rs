//! 6502 core tests.

use super::*;

struct Ram(Vec<u8>);

impl Ram {
    fn new() -> Self {
        Self(vec![0; 0x10000])
    }

    fn load(&mut self, addr: u16, bytes: &[u8]) {
        self.0[usize::from(addr)..usize::from(addr) + bytes.len()].copy_from_slice(bytes);
    }
}

impl Bus for Ram {
    fn read(&mut self, addr: u16) -> u8 {
        self.0[usize::from(addr)]
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.0[usize::from(addr)] = value;
    }
}

fn cpu_with(program: &[u8]) -> (Cpu, Ram) {
    let mut ram = Ram::new();
    ram.load(0x0200, program);
    (Cpu::new(0x0200), ram)
}

#[test]
fn jam_and_unknown_opcode_counters() {
    let mut ram = Ram::new();
    ram.load(0x0200, &[0x02]); // JAM
    let mut cpu = Cpu::new(0x0200);
    cpu.step(&mut ram);
    assert!(cpu.jammed);
    assert_eq!(cpu.jam_count, 1);

    let mut ram = Ram::new();
    ram.load(0x0200, &[0x9E]); // undocumented SHX, not implemented
    let mut cpu = Cpu::new(0x0200);
    cpu.step(&mut ram);
    assert_eq!(cpu.unknown_opcode, Some(0x9E));
    assert_eq!(cpu.unknown_opcode_count, 1);
}

#[test]
fn adc_sets_carry_and_overflow() {
    let (mut cpu, mut ram) = cpu_with(&[0x69, 0x50]);
    cpu.a = 0x50;
    cpu.step(&mut ram);
    assert_eq!(cpu.a, 0xA0);
    assert!(cpu.flag(FLAG_V));
    assert!(!cpu.flag(FLAG_C));
}

#[test]
fn subtraction_uses_carry_as_borrow() {
    let (mut cpu, mut ram) = cpu_with(&[0xE9, 0x01]);
    cpu.a = 0x00;
    cpu.p |= FLAG_C;
    cpu.step(&mut ram);
    assert_eq!(cpu.a, 0xFF);
    assert!(!cpu.flag(FLAG_C));
    assert!(cpu.flag(FLAG_N));
}

#[test]
fn jsr_and_rts_round_trip() {
    let mut ram = Ram::new();
    ram.load(0x0200, &[0x20, 0x10, 0x02]); // JSR $0210
    ram.load(0x0210, &[0xA9, 0x42, 0x60]); // LDA #$42 ; RTS
    let mut cpu = Cpu::new(0x0200);
    cpu.step(&mut ram);
    assert_eq!(cpu.pc, 0x0210);
    cpu.step(&mut ram);
    assert_eq!(cpu.a, 0x42);
    cpu.step(&mut ram);
    assert_eq!(cpu.pc, 0x0203);
    assert_eq!(cpu.sp, 0xFD);
}

#[test]
fn branch_page_cross_costs_four_cycles() {
    let mut ram = Ram::new();
    ram.load(0x02FD, &[0xD0, 0x7F]); // BNE +127 -> $037E
    let mut cpu = Cpu::new(0x02FD);
    cpu.set_zn(1);
    assert_eq!(cpu.step(&mut ram), 4);
    assert_eq!(cpu.pc, 0x037E);
}

#[test]
fn zeropage_index_wraps() {
    let mut ram = Ram::new();
    ram.load(0x0200, &[0xB5, 0xFF]); // LDA $FF,X
    ram.0[0x0004] = 0x99; // X = 5 -> $04
    let mut cpu = Cpu::new(0x0200);
    cpu.x = 5;
    cpu.step(&mut ram);
    assert_eq!(cpu.a, 0x99);
}

#[test]
fn jmp_indirect_wraps_on_page_boundary() {
    let mut ram = Ram::new();
    ram.load(0x0300, &[0x6C, 0xFF, 0x04]);
    ram.0[0x04FF] = 0x34;
    ram.0[0x0400] = 0x12;
    ram.0[0x0500] = 0xFF; // must be ignored
    let mut cpu = Cpu::new(0x0300);
    cpu.step(&mut ram);
    assert_eq!(cpu.pc, 0x1234);
}

#[test]
fn nmi_pushes_state_and_vectors() {
    let mut ram = Ram::new();
    ram.0[0xFFFA] = 0x00;
    ram.0[0xFFFB] = 0x04;
    let mut cpu = Cpu::new(0x0200);
    cpu.nmi(&mut ram);
    assert_eq!(cpu.pc, 0x0400);
    assert_eq!(cpu.sp, 0xFA);
    assert!(cpu.flag(FLAG_I));
}
