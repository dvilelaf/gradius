//! CPU bus, mapper 3 and controller shift registers.

use gradius_cart::Cartridge;

use crate::apu::Apu;
use crate::cpu::Bus as CpuBus;
use crate::ppu::Ppu;

/// Standard NES controller button bits, in the order the shift register emits
/// them: A, B, Select, Start, Up, Down, Left, Right.
pub const BUTTON_A: u8 = 0x01;
pub const BUTTON_B: u8 = 0x02;
pub const BUTTON_SELECT: u8 = 0x04;
pub const BUTTON_START: u8 = 0x08;
pub const BUTTON_UP: u8 = 0x10;
pub const BUTTON_DOWN: u8 = 0x20;
pub const BUTTON_LEFT: u8 = 0x40;
pub const BUTTON_RIGHT: u8 = 0x80;

#[derive(Clone, Debug, Default)]
pub struct Controller {
    pub buttons: u8,
    shift: u8,
    strobe: bool,
}

impl Controller {
    pub fn set_buttons(&mut self, buttons: u8) {
        self.buttons = buttons;
    }

    fn write(&mut self, value: u8) {
        self.strobe = value & 1 != 0;
        if self.strobe {
            self.shift = self.buttons;
        }
    }

    fn read(&mut self) -> u8 {
        if self.strobe {
            return self.buttons & 1;
        }
        let bit = self.shift & 1;
        self.shift = (self.shift >> 1) | 0x80;
        bit
    }
}

/// Execution coverage gathered while running the oracle.
#[derive(Clone, Debug)]
pub struct Trace {
    /// Opcode observed at each PC, or 0 if the address never executed.
    pub opcodes: Vec<u8>,
    /// Number of times each PC executed.
    pub hits: Vec<u32>,
    pub instructions: u64,
}

impl Trace {
    fn new() -> Self {
        Self {
            opcodes: vec![0; 0x10000],
            hits: vec![0; 0x10000],
            instructions: 0,
        }
    }

    pub fn was_executed(&self, pc: u16) -> bool {
        self.hits[usize::from(pc)] > 0
    }

    pub fn visited_addresses(&self) -> usize {
        self.hits.iter().filter(|hits| **hits > 0).count()
    }
}

/// Everything the CPU can address.
#[derive(Clone, Debug)]
pub struct Bus {
    pub ram: [u8; 2048],
    pub ppu: Ppu,
    pub apu: Apu,
    pub prg: Vec<u8>,
    /// Controller 1, read from `$4016`.
    pub controller: Controller,
    /// Controller 2, read from `$4017`. Both share the `$4016` strobe.
    pub controller2: Controller,
    pub trace: Trace,
    pub stall: u32,
}

impl Bus {
    pub fn new(cart: &Cartridge) -> Self {
        let prg = cart.prg.clone();
        Self {
            ram: [0; 2048],
            ppu: Ppu::new(cart),
            apu: Apu::new(),
            prg,
            controller: Controller::default(),
            controller2: Controller::default(),
            trace: Trace::new(),
            stall: 0,
        }
    }

    pub fn take_stall(&mut self) -> u32 {
        std::mem::take(&mut self.stall)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.prg[usize::from(addr - 0x8000) & 0x7FFF]
    }

    fn oam_dma(&mut self, page: u8) {
        let base = u16::from(page) << 8;
        for offset in 0..256u16 {
            self.ppu.oam[usize::from(offset as u8)] = self.read(base + offset);
        }
        // The real CPU is halted for 513 or 514 cycles; one extra cycle is
        // within the tolerance of this oracle.
        self.stall += 513;
    }
}

impl CpuBus for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[usize::from(addr) & 0x07FF],
            0x2000..=0x3FFF => self.ppu.cpu_read(addr),
            0x4015 => self.apu.read_status(),
            0x4016 => self.controller.read(),
            0x4017 => self.controller2.read(),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => 0,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram[usize::from(addr) & 0x07FF] = value,
            0x2000..=0x3FFF => self.ppu.cpu_write(addr, value),
            0x4014 => self.oam_dma(value),
            0x4016 => {
                // A write to $4016 strobes both physical pads.
                self.controller.write(value);
                self.controller2.write(value);
            }
            0x4000..=0x4017 => self.apu.write(addr, value),
            // Mapper 3 (CNROM): any write selects one of the 8 KiB CHR banks.
            0x8000..=0xFFFF => self.ppu.set_chr_bank(value & 0x03),
            _ => {}
        }
    }

    fn on_exec(&mut self, pc: u16, opcode: u8) {
        let index = usize::from(pc);
        self.trace.opcodes[index] = opcode;
        self.trace.hits[index] = self.trace.hits[index].saturating_add(1);
        self.trace.instructions += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_cart::Header;

    fn cart() -> Cartridge {
        Cartridge {
            header: Header {
                prg_units: 2,
                chr_units: 4,
                mapper: 3,
                mirroring: gradius_cart::Mirroring::Vertical,
                has_trainer: false,
                has_battery: false,
            },
            prg: vec![0; 32 * 1024],
            chr: vec![0; 32 * 1024],
            trainer: None,
            trailing: Vec::new(),
        }
    }

    #[test]
    fn ram_is_mirrored_every_2k() {
        let mut bus = Bus::new(&cart());
        bus.write(0x0002, 0xAA);
        assert_eq!(bus.read(0x0802), 0xAA);
        assert_eq!(bus.read(0x1002), 0xAA);
    }

    #[test]
    fn cnrom_write_switches_chr_bank() {
        let mut bus = Bus::new(&cart());
        bus.write(0x8000, 0x02);
        assert_eq!(bus.ppu.chr_bank, 2);
        bus.write(0xFFFF, 0x07);
        assert_eq!(bus.ppu.chr_bank, 3);
    }

    #[test]
    fn apu_registers_are_routed_through_the_bus() {
        let mut bus = Bus::new(&cart());
        bus.apu.start_capture();
        bus.write(0x4015, 0x01);
        bus.write(0x4000, 0xBF);
        bus.write(0x4003, 0x08);
        assert_eq!(bus.read(0x4015) & 0x01, 0x01);
        assert_eq!(bus.apu.writes().len(), 3);
        assert_eq!(bus.apu.pulse1.volume, 15);
        assert_eq!(bus.apu.pulse1.length, 254);
        // $4017 selects the frame-counter mode and stays readable as 0.
        bus.write(0x4017, 0x40);
        assert!(bus.apu.irq_inhibit);
        assert_eq!(bus.read(0x4017), 0);
    }

    #[test]
    fn controller_shifts_msb_first() {
        let mut controller = Controller::default();
        controller.set_buttons(BUTTON_A | BUTTON_START);
        controller.write(1);
        controller.write(0);
        let bits: Vec<u8> = (0..8).map(|_| controller.read()).collect();
        assert_eq!(bits, vec![1, 0, 0, 1, 0, 0, 0, 0]);
    }

    #[test]
    fn controllers_shift_independently() {
        let mut bus = Bus::new(&cart());
        bus.controller.set_buttons(BUTTON_A);
        bus.controller2.set_buttons(BUTTON_RIGHT);
        bus.write(0x4016, 1);
        bus.write(0x4016, 0);

        let mut p1 = Vec::new();
        let mut p2 = Vec::new();
        for _ in 0..8 {
            p1.push(bus.read(0x4016));
            p2.push(bus.read(0x4017));
        }
        assert_eq!(p1, vec![1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(p2, vec![0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn strobe_latches_both_controllers() {
        let mut bus = Bus::new(&cart());
        bus.controller.set_buttons(BUTTON_A);
        bus.controller2.set_buttons(BUTTON_B);
        bus.write(0x4016, 1);
        bus.write(0x4016, 0);

        // Buttons pressed mid-shift must not disturb the latched registers.
        bus.controller.set_buttons(BUTTON_START);
        bus.controller2.set_buttons(BUTTON_SELECT);
        let p1: Vec<u8> = (0..8).map(|_| bus.read(0x4016)).collect();
        let p2: Vec<u8> = (0..8).map(|_| bus.read(0x4017)).collect();
        assert_eq!(p1, vec![1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(p2, vec![0, 1, 0, 0, 0, 0, 0, 0]);

        // The next strobe picks up the new buttons on both pads.
        bus.write(0x4016, 1);
        bus.write(0x4016, 0);
        let p1: Vec<u8> = (0..8).map(|_| bus.read(0x4016)).collect();
        let p2: Vec<u8> = (0..8).map(|_| bus.read(0x4017)).collect();
        assert_eq!(p1, vec![0, 0, 0, 1, 0, 0, 0, 0]);
        assert_eq!(p2, vec![0, 0, 1, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn button_bits_follow_the_documented_order() {
        let order = [
            (BUTTON_A, 0),
            (BUTTON_B, 1),
            (BUTTON_SELECT, 2),
            (BUTTON_START, 3),
            (BUTTON_UP, 4),
            (BUTTON_DOWN, 5),
            (BUTTON_LEFT, 6),
            (BUTTON_RIGHT, 7),
        ];
        for (button, index) in order {
            let mut bus = Bus::new(&cart());
            bus.controller2.set_buttons(button);
            bus.write(0x4016, 1);
            bus.write(0x4016, 0);
            let bits: Vec<u8> = (0..8).map(|_| bus.read(0x4017)).collect();
            let expected: Vec<u8> = (0..8).map(|i| u8::from(i == index)).collect();
            assert_eq!(bits, expected, "button {button:#04x} should be bit {index}");
        }
    }
}
