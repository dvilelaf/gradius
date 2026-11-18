//! A small NES APU model used at runtime.
//!
//! Four channels (two pulses, triangle, noise) driven by `$4000..=$4013` and
//! `$4015`, producing mono samples. The model follows the same per-CPU-cycle
//! structure as the development oracle so a captured register timeline sounds
//! the same here as it does when the original cartridge is emulated.
//!
//! The DMC is not synthesised: Gradius never enables it. Its registers are
//! latched so a stray write cannot corrupt the mix.

/// NTSC CPU clock, in hertz.
pub const CPU_CLOCK_HZ: u32 = 1_789_773;
/// Native output rate of [`crate::Engine`].
pub const SAMPLE_RATE: u32 = 48_000;

const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

const DUTY: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];

const TRIANGLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];

/// NTSC noise timer periods, in CPU cycles.
const NOISE_PERIOD: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

/// One of the two pulse channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pulse {
    pub enabled: bool,
    pub duty: u8,
    pub duty_step: u8,
    pub timer_period: u16,
    pub timer_counter: u16,
    pub length: u8,
    pub length_halt: bool,
    pub constant_volume: bool,
    pub volume: u8,
    pub envelope_divider: u8,
    pub envelope_decay: u8,
    pub envelope_start: bool,
    pub sweep_enabled: bool,
    pub sweep_period: u8,
    pub sweep_negate: bool,
    pub sweep_shift: u8,
    pub sweep_divider: u8,
    pub sweep_reload: bool,
    pub is_pulse1: bool,
}

impl Pulse {
    pub fn new(is_pulse1: bool) -> Self {
        Self {
            enabled: false,
            duty: 0,
            duty_step: 0,
            timer_period: 0,
            timer_counter: 0,
            length: 0,
            length_halt: false,
            constant_volume: false,
            volume: 0,
            envelope_divider: 0,
            envelope_decay: 0,
            envelope_start: false,
            sweep_enabled: false,
            sweep_period: 0,
            sweep_negate: false,
            sweep_shift: 0,
            sweep_divider: 0,
            sweep_reload: false,
            is_pulse1,
        }
    }

    /// Note frequency implied by the current timer, in hertz.
    pub fn frequency_hz(&self) -> f64 {
        f64::from(CPU_CLOCK_HZ) / (16.0 * (f64::from(self.timer_period) + 1.0))
    }

    fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            self.duty_step = (self.duty_step + 1) & 7;
        } else {
            self.timer_counter -= 1;
        }
    }

    fn clock_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_start = false;
            self.envelope_decay = 15;
            self.envelope_divider = self.volume;
        } else if self.envelope_divider == 0 {
            self.envelope_divider = self.volume;
            if self.envelope_decay > 0 {
                self.envelope_decay -= 1;
            } else if self.length_halt {
                self.envelope_decay = 15;
            }
        } else {
            self.envelope_divider -= 1;
        }
    }

    fn sweep_target(&self) -> i32 {
        let change = i32::from(self.timer_period >> self.sweep_shift);
        if self.sweep_negate {
            i32::from(self.timer_period) - change - i32::from(self.is_pulse1)
        } else {
            i32::from(self.timer_period) + change
        }
    }

    fn clock_sweep(&mut self) {
        let target = self.sweep_target();
        if self.sweep_divider == 0
            && self.sweep_enabled
            && self.sweep_shift > 0
            && self.timer_period >= 8
            && (0..=0x7FF).contains(&target)
        {
            self.timer_period = target as u16;
        }
        if self.sweep_divider == 0 || self.sweep_reload {
            self.sweep_divider = self.sweep_period;
            self.sweep_reload = false;
        } else {
            self.sweep_divider -= 1;
        }
    }

    fn clock_length(&mut self) {
        if !self.length_halt && self.length > 0 {
            self.length -= 1;
        }
    }

    fn muted(&self) -> bool {
        let target = self.sweep_target();
        self.timer_period < 8 || target > 0x7FF
    }

    /// Current DAC level, 0..=15.
    pub fn output(&self) -> u8 {
        if !self.enabled || self.length == 0 || self.muted() {
            return 0;
        }
        let volume = if self.constant_volume {
            self.volume
        } else {
            self.envelope_decay
        };
        DUTY[usize::from(self.duty & 3)][usize::from(self.duty_step & 7)] * volume
    }

    /// `$4000..=$4003` for pulse 1, `$4004..=$4007` for pulse 2.
    pub fn write(&mut self, index: u8, value: u8) {
        match index & 3 {
            0 => {
                self.duty = value >> 6;
                self.length_halt = value & 0x20 != 0;
                self.constant_volume = value & 0x10 != 0;
                self.volume = value & 0x0F;
            }
            1 => {
                self.sweep_enabled = value & 0x80 != 0;
                self.sweep_period = (value >> 4) & 0x07;
                self.sweep_negate = value & 0x08 != 0;
                self.sweep_shift = value & 0x07;
                self.sweep_reload = true;
            }
            2 => {
                self.timer_period = (self.timer_period & 0x0700) | u16::from(value);
            }
            _ => {
                self.timer_period = (self.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
                if self.enabled {
                    self.length = LENGTH_TABLE[usize::from(value >> 3)];
                }
                self.envelope_start = true;
            }
        }
    }
}

/// Triangle channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triangle {
    pub enabled: bool,
    pub control: bool,
    pub linear_reload_value: u8,
    pub linear_counter: u8,
    pub length: u8,
    pub timer_period: u16,
    pub timer_counter: u16,
    pub step: u8,
}

impl Triangle {
    pub fn new() -> Self {
        Self {
            enabled: false,
            control: false,
            linear_reload_value: 0,
            linear_counter: 0,
            length: 0,
            timer_period: 0,
            timer_counter: 0,
            step: 0,
        }
    }

    pub fn frequency_hz(&self) -> f64 {
        f64::from(CPU_CLOCK_HZ) / (32.0 * (f64::from(self.timer_period) + 1.0))
    }

    fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            self.step = (self.step + 1) & 31;
        } else {
            self.timer_counter -= 1;
        }
    }

    fn clock_linear(&mut self) {
        if self.control {
            self.linear_counter = self.linear_reload_value;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
    }

    fn clock_length(&mut self) {
        if !self.control && self.length > 0 {
            self.length -= 1;
        }
    }

    pub fn output(&self) -> u8 {
        if !self.enabled || self.length == 0 || self.linear_counter == 0 || self.timer_period < 2 {
            return 0;
        }
        TRIANGLE[usize::from(self.step & 31)]
    }

    /// `$4008`, `$400A` and `$400B`.
    pub fn write(&mut self, index: u8, value: u8) {
        match index & 3 {
            0 => {
                self.control = value & 0x80 != 0;
                self.linear_reload_value = value & 0x7F;
            }
            2 => {
                self.timer_period = (self.timer_period & 0x0700) | u16::from(value);
            }
            3 => {
                self.timer_period = (self.timer_period & 0x00FF) | (u16::from(value & 0x07) << 8);
                if self.enabled {
                    self.length = LENGTH_TABLE[usize::from(value >> 3)];
                }
            }
            _ => {}
        }
    }
}

impl Default for Triangle {
    fn default() -> Self {
        Self::new()
    }
}

/// Noise channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Noise {
    pub enabled: bool,
    pub mode: bool,
    pub timer_period: u16,
    pub timer_counter: u16,
    pub shift: u16,
    pub length: u8,
    pub length_halt: bool,
    pub constant_volume: bool,
    pub volume: u8,
    pub envelope_divider: u8,
    pub envelope_decay: u8,
    pub envelope_start: bool,
}

impl Noise {
    pub fn new() -> Self {
        Self {
            enabled: false,
            mode: false,
            timer_period: NOISE_PERIOD[0],
            timer_counter: 0,
            shift: 1,
            length: 0,
            length_halt: false,
            constant_volume: false,
            volume: 0,
            envelope_divider: 0,
            envelope_decay: 0,
            envelope_start: false,
        }
    }

    fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            let tap = if self.mode { 6 } else { 1 };
            let feedback = (self.shift & 1) ^ ((self.shift >> tap) & 1);
            self.shift = (self.shift >> 1) | (feedback << 14);
        } else {
            self.timer_counter -= 1;
        }
    }

    fn clock_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_start = false;
            self.envelope_decay = 15;
            self.envelope_divider = self.volume;
        } else if self.envelope_divider == 0 {
            self.envelope_divider = self.volume;
            if self.envelope_decay > 0 {
                self.envelope_decay -= 1;
            } else if self.length_halt {
                self.envelope_decay = 15;
            }
        } else {
            self.envelope_divider -= 1;
        }
    }

    fn clock_length(&mut self) {
        if !self.length_halt && self.length > 0 {
            self.length -= 1;
        }
    }

    pub fn output(&self) -> u8 {
        if !self.enabled || self.length == 0 || self.shift & 1 != 0 {
            return 0;
        }
        if self.constant_volume {
            self.volume
        } else {
            self.envelope_decay
        }
    }

    /// `$400C..=$400F`.
    pub fn write(&mut self, index: u8, value: u8) {
        match index & 3 {
            0 => {
                self.length_halt = value & 0x20 != 0;
                self.constant_volume = value & 0x10 != 0;
                self.volume = value & 0x0F;
            }
            2 => {
                self.mode = value & 0x80 != 0;
                self.timer_period = NOISE_PERIOD[usize::from(value & 0x0F)];
            }
            3 => {
                if self.enabled {
                    self.length = LENGTH_TABLE[usize::from(value >> 3)];
                }
                self.envelope_start = true;
            }
            _ => {}
        }
    }
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}

/// The APU: four channels, frame counter and mixer.
#[derive(Clone, Debug)]
pub struct Apu {
    pub pulse1: Pulse,
    pub pulse2: Pulse,
    pub triangle: Triangle,
    pub noise: Noise,

    pub mode5: bool,
    pub irq_inhibit: bool,
    pub frame_irq: bool,
    pub frame_cycle: u32,

    pub dmc_direct: u8,
    pub dmc_address: u8,
    pub dmc_length: u8,
    pub dmc_enabled: bool,

    /// Pulse and noise timers tick every other CPU cycle.
    apu_phase: bool,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub fn new() -> Self {
        Self {
            pulse1: Pulse::new(true),
            pulse2: Pulse::new(false),
            triangle: Triangle::new(),
            noise: Noise::new(),
            mode5: false,
            irq_inhibit: false,
            frame_irq: false,
            frame_cycle: 0,
            dmc_direct: 0,
            dmc_address: 0,
            dmc_length: 0,
            dmc_enabled: false,
            apu_phase: false,
        }
    }

    /// Return to the power-on state: all channels silent.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Write an APU register in `$4000..=$4017`.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x4000..=0x4003 => self.pulse1.write((addr & 3) as u8, value),
            0x4004..=0x4007 => self.pulse2.write((addr & 3) as u8, value),
            0x4008 => self.triangle.write(0, value),
            0x400A => self.triangle.write(2, value),
            0x400B => self.triangle.write(3, value),
            0x400C => self.noise.write(0, value),
            0x400E => self.noise.write(2, value),
            0x400F => self.noise.write(3, value),
            0x4011 => self.dmc_direct = value & 0x7F,
            0x4012 => self.dmc_address = value,
            0x4013 => self.dmc_length = value,
            0x4015 => {
                self.pulse1.enabled = value & 0x01 != 0;
                if !self.pulse1.enabled {
                    self.pulse1.length = 0;
                }
                self.pulse2.enabled = value & 0x02 != 0;
                if !self.pulse2.enabled {
                    self.pulse2.length = 0;
                }
                self.triangle.enabled = value & 0x04 != 0;
                if !self.triangle.enabled {
                    self.triangle.length = 0;
                }
                self.noise.enabled = value & 0x08 != 0;
                if !self.noise.enabled {
                    self.noise.length = 0;
                }
                self.dmc_enabled = value & 0x10 != 0;
            }
            0x4017 => {
                self.mode5 = value & 0x80 != 0;
                self.irq_inhibit = value & 0x40 != 0;
                if self.irq_inhibit {
                    self.frame_irq = false;
                }
                self.frame_cycle = 0;
                if self.mode5 {
                    self.quarter_frame();
                    self.half_frame();
                }
            }
            _ => {}
        }
    }

    fn quarter_frame(&mut self) {
        self.pulse1.clock_envelope();
        self.pulse2.clock_envelope();
        self.noise.clock_envelope();
        self.triangle.clock_linear();
    }

    fn half_frame(&mut self) {
        self.pulse1.clock_length();
        self.pulse2.clock_length();
        self.triangle.clock_length();
        self.noise.clock_length();
        self.pulse1.clock_sweep();
        self.pulse2.clock_sweep();
    }

    /// Advance by `cycles` CPU cycles.
    pub fn step(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.clock_once();
        }
    }

    fn clock_once(&mut self) {
        self.triangle.clock_timer();

        self.apu_phase = !self.apu_phase;
        if self.apu_phase {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }

        self.frame_cycle += 1;
        if self.mode5 {
            match self.frame_cycle {
                7457 => self.quarter_frame(),
                14913 => self.half_frame(),
                22371 => self.quarter_frame(),
                37281 => {
                    self.half_frame();
                    self.quarter_frame();
                    self.frame_cycle = 0;
                }
                _ => {}
            }
        } else {
            match self.frame_cycle {
                7457 => self.quarter_frame(),
                14913 => {
                    self.half_frame();
                    if !self.irq_inhibit {
                        self.frame_irq = true;
                    }
                }
                22371 => self.quarter_frame(),
                29829 => {
                    self.half_frame();
                    self.frame_cycle = 0;
                }
                _ => {}
            }
        }
    }

    /// Current mixer output, in `0.0..=1.0`.
    pub fn mix(&self) -> f32 {
        let pulse = f32::from(self.pulse1.output()) + f32::from(self.pulse2.output());
        let pulse_out = if pulse == 0.0 {
            0.0
        } else {
            95.88 / (8128.0 / pulse + 100.0)
        };
        let tnd =
            f32::from(self.triangle.output()) / 8227.0 + f32::from(self.noise.output()) / 12241.0;
        let tnd_out = if tnd == 0.0 {
            0.0
        } else {
            159.79 / (1.0 / tnd + 100.0)
        };
        (pulse_out + tnd_out).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_frequency_matches_the_timer() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0xBF);
        apu.write(0x4002, 0xFD);
        apu.write(0x4003, 0x08);
        assert_eq!(apu.pulse1.timer_period, 0x00FD);
        assert!((apu.pulse1.frequency_hz() - 440.4).abs() < 1.0);
    }

    #[test]
    fn pulse_envelope_decays() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0x00);
        apu.write(0x4003, 0x08);
        apu.quarter_frame();
        assert_eq!(apu.pulse1.envelope_decay, 15);
        apu.quarter_frame();
        assert_eq!(apu.pulse1.envelope_decay, 14);
        apu.pulse1.timer_period = 100;
        apu.pulse1.duty = 2;
        apu.pulse1.duty_step = 1;
        assert_eq!(apu.pulse1.output(), 14);
    }

    #[test]
    fn sweep_lowers_the_negated_period() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0xBF);
        apu.write(0x4001, 0x89); // enable, period 0, negate, shift 1
        apu.write(0x4002, 0x00);
        apu.write(0x4003, 0x02); // period 0x200
        let before = apu.pulse1.timer_period;
        apu.pulse1.clock_sweep();
        assert_eq!(apu.pulse1.timer_period, before - (before >> 1) - 1);
    }

    #[test]
    fn triangle_needs_a_linear_counter() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x04);
        apu.write(0x4008, 0x00);
        apu.write(0x400A, 0x10);
        apu.write(0x400B, 0x08);
        apu.step(64);
        assert_eq!(apu.triangle.output(), 0);
        apu.write(0x4008, 0xFF);
        apu.quarter_frame();
        assert_eq!(apu.triangle.linear_counter, 0x7F);
        assert!(apu.triangle.output() > 0);
    }

    #[test]
    fn noise_lfsr_advances_in_both_modes() {
        let mut noise = Noise::new();
        noise.timer_period = 0;
        noise.timer_counter = 0;
        noise.shift = 1;
        noise.clock_timer();
        assert_eq!(noise.shift, 0x4000);

        let mut mode1 = Noise::new();
        mode1.mode = true;
        mode1.timer_period = 0;
        mode1.timer_counter = 0;
        mode1.shift = 0x41;
        mode1.clock_timer();
        assert_eq!(mode1.shift & 0x4000, 0);
    }

    #[test]
    fn status_register_controls_lengths() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x0F);
        apu.write(0x4003, 0x08);
        apu.write(0x4007, 0x08);
        apu.write(0x400B, 0x08);
        apu.write(0x400F, 0x08);
        assert!(apu.pulse1.length > 0);
        assert!(apu.triangle.length > 0);
        assert!(apu.noise.length > 0);
        apu.write(0x4015, 0x00);
        assert_eq!(apu.pulse1.length, 0);
        assert_eq!(apu.noise.length, 0);
    }

    #[test]
    fn mix_is_bounded_and_audible() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x05);
        apu.write(0x4000, 0xBF);
        apu.write(0x4002, 0x40);
        apu.write(0x4003, 0x08);
        apu.write(0x4008, 0xFF);
        apu.write(0x400A, 0x40);
        apu.write(0x400B, 0x08);
        apu.step(2_000);
        let mut peak = 0.0f32;
        for _ in 0..1_000 {
            apu.step(1);
            let sample = apu.mix();
            assert!(sample.is_finite() && sample <= 1.0);
            peak = peak.max(sample);
        }
        assert!(peak > 0.01);
    }

    #[test]
    fn reset_silences_the_mixer() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x0F);
        apu.write(0x4000, 0xBF);
        apu.write(0x4002, 0x40);
        apu.write(0x4003, 0x08);
        apu.step(100);
        assert!(apu.mix() > 0.0);
        apu.reset();
        assert_eq!(apu.mix(), 0.0);
    }
}
