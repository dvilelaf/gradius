//! NES APU model, mixer and register-write capture.
//!
//! The oracle only needs the APU to be *observable*: register writes must be
//! recorded exactly where they happen in the frame loop, the length counters and
//! envelopes must answer `$4015`/analysis queries plausibly, and the mixer must
//! produce samples that can be compared or extracted. This is therefore a
//! straightforward per-CPU-cycle model, not a transistor-level one.
//!
//! Channels: two pulses, triangle, noise. DMC registers are latched but the
//! sample playback itself is not emulated (Gradius never enables it), so it
//! contributes zero to the mix. The frame counter supports both the 4-step and
//! 5-step sequences and exposes the IRQ flag; wiring that flag into the CPU is
//! deliberately left to the caller.

use std::collections::VecDeque;

/// Output sample rate used by [`Apu::step`].
pub const SAMPLE_RATE: u32 = 48_000;
/// NTSC CPU clock, in hertz.
pub const CPU_CLOCK_HZ: f64 = 1_789_773.0;
/// CPU cycles per second, for the sample-rate accumulator.
const CPU_CLOCK_INT: u32 = 1_789_773;
/// Hard cap on the retained audio buffer (about eight seconds at 48 kHz).
pub const MAX_AUDIO_SAMPLES: usize = SAMPLE_RATE as usize * 8;
/// Hard cap on retained APU register writes.
pub const MAX_CAPTURED_WRITES: usize = 32_768;

/// One APU register write, with the frame and absolute CPU cycle it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApuWrite {
    /// Video frame in which the write happened.
    pub frame: u64,
    /// Absolute CPU cycle at the start of the instruction that wrote it.
    pub cycle: u64,
    /// Register address in `$4000..=$4017`.
    pub addr: u16,
    /// Value written.
    pub value: u8,
}

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
    /// Length halt, which doubles as the envelope loop flag.
    pub length_halt: bool,
    pub constant_volume: bool,
    /// Envelope period or constant volume, depending on `constant_volume`.
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
    /// True for pulse 1, whose negate mode subtracts one extra.
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
        CPU_CLOCK_HZ / (16.0 * (f64::from(self.timer_period) + 1.0))
    }

    pub fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            self.duty_step = (self.duty_step + 1) & 7;
        } else {
            self.timer_counter -= 1;
        }
    }

    pub fn clock_envelope(&mut self) {
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

    pub fn sweep_target(&self) -> i32 {
        let change = i32::from(self.timer_period >> self.sweep_shift);
        if self.sweep_negate {
            i32::from(self.timer_period) - change - i32::from(self.is_pulse1)
        } else {
            i32::from(self.timer_period) + change
        }
    }

    pub fn clock_sweep(&mut self) {
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

    pub fn clock_length(&mut self) {
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Triangle {
    pub enabled: bool,
    /// Linear counter halt, which doubles as the reload flag.
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
        CPU_CLOCK_HZ / (32.0 * (f64::from(self.timer_period) + 1.0))
    }

    pub fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            self.step = (self.step + 1) & 31;
        } else {
            self.timer_counter -= 1;
        }
    }

    pub fn clock_linear(&mut self) {
        if self.control {
            self.linear_counter = self.linear_reload_value;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
    }

    pub fn clock_length(&mut self) {
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

/// Noise channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Noise {
    pub enabled: bool,
    pub mode: bool,
    pub timer_period: u16,
    pub timer_counter: u16,
    pub shift: u16,
    pub length: u8,
    /// Length halt, which doubles as the envelope loop flag.
    pub length_halt: bool,
    pub constant_volume: bool,
    pub volume: u8,
    pub envelope_divider: u8,
    pub envelope_decay: u8,
    pub envelope_start: bool,
}

impl Default for Noise {
    /// The same state as [`Noise::new`]: unlike [`Triangle`], the noise channel
    /// starts with a non-zero period and LFSR seed, so this cannot be derived.
    fn default() -> Self {
        Self::new()
    }
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

    pub fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            let tap = if self.mode { 6 } else { 1 };
            let feedback = (self.shift & 1) ^ ((self.shift >> tap) & 1);
            self.shift = (self.shift >> 1) | (feedback << 14);
        } else {
            self.timer_counter -= 1;
        }
    }

    pub fn clock_envelope(&mut self) {
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

    pub fn clock_length(&mut self) {
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

/// The APU: four channels, frame counter, capture and mixing.
#[derive(Clone, Debug)]
pub struct Apu {
    pub pulse1: Pulse,
    pub pulse2: Pulse,
    pub triangle: Triangle,
    pub noise: Noise,

    /// 4-step (false) or 5-step (true) frame sequence.
    pub mode5: bool,
    /// IRQ inhibit flag written to `$4017` bit 6.
    pub irq_inhibit: bool,
    /// Frame-counter IRQ flag, cleared by reading `$4015`.
    pub frame_irq: bool,
    /// Cycles into the current frame-counter sequence.
    pub frame_cycle: u32,
    /// APU-cycle phase: pulse and noise timers tick every other CPU cycle.
    apu_phase: bool,

    pub dmc_direct: u8,
    pub dmc_address: u8,
    pub dmc_length: u8,
    pub dmc_enabled: bool,

    /// Frame and absolute cycle attached to captured writes.
    pub frame: u64,
    pub cycle: u64,
    /// Opt in to register-write capture with [`Apu::start_capture`].
    pub capture: bool,
    pub writes: Vec<ApuWrite>,
    /// Writes evicted from the bounded capture buffer.
    pub writes_dropped: u64,

    /// When false, no samples are produced and the audio buffer stays empty.
    pub audio_enabled: bool,
    audio: VecDeque<f32>,
    sample_phase: u32,
    pub samples_produced: u64,
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
            apu_phase: false,
            dmc_direct: 0,
            dmc_address: 0,
            dmc_length: 0,
            dmc_enabled: false,
            frame: 0,
            cycle: 0,
            capture: false,
            writes: Vec::new(),
            writes_dropped: 0,
            audio_enabled: true,
            audio: VecDeque::new(),
            sample_phase: 0,
            samples_produced: 0,
        }
    }

    pub fn start_capture(&mut self) {
        self.capture = true;
    }

    pub fn stop_capture(&mut self) {
        self.capture = false;
    }

    pub fn clear_writes(&mut self) {
        self.writes.clear();
        self.writes_dropped = 0;
    }

    pub fn writes(&self) -> &[ApuWrite] {
        &self.writes
    }

    /// Read `$4015`: length-counter status plus the frame IRQ flag, which this
    /// read acknowledges.
    pub fn read_status(&mut self) -> u8 {
        let mut value = 0;
        if self.pulse1.length > 0 {
            value |= 0x01;
        }
        if self.pulse2.length > 0 {
            value |= 0x02;
        }
        if self.triangle.length > 0 {
            value |= 0x04;
        }
        if self.noise.length > 0 {
            value |= 0x08;
        }
        if self.frame_irq {
            value |= 0x40;
            self.frame_irq = false;
        }
        value
    }

    /// Write an APU register in `$4000..=$4017`.
    pub fn write(&mut self, addr: u16, value: u8) {
        if self.capture {
            if self.writes.len() >= MAX_CAPTURED_WRITES {
                let drop = MAX_CAPTURED_WRITES / 4;
                self.writes.drain(..drop);
                self.writes_dropped += drop as u64;
            }
            self.writes.push(ApuWrite {
                frame: self.frame,
                cycle: self.cycle,
                addr,
                value,
            });
        }
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
                // A write with bit 7 set clocks the quarter and half frames
                // immediately and starts the 5-step sequence.
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

    /// Advance the APU by `cycles` CPU cycles, producing samples as they fall
    /// due. `cycle` and `frame` should be set by the caller before stepping so
    /// register writes are timestamped.
    pub fn step(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.clock_once();
        }
    }

    fn clock_once(&mut self) {
        // Triangle advances once per CPU cycle.
        self.triangle.clock_timer();

        // Pulses and noise are clocked at the APU rate (every second CPU cycle).
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

        if self.audio_enabled {
            self.sample_phase += SAMPLE_RATE;
            if self.sample_phase >= CPU_CLOCK_INT {
                self.sample_phase -= CPU_CLOCK_INT;
                self.push_sample();
            }
        }
    }

    /// Current mixer output, in `-1.0..=1.0`.
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
        (pulse_out + tnd_out).clamp(-1.0, 1.0)
    }

    fn push_sample(&mut self) {
        let sample = self.mix();
        if self.audio.len() >= MAX_AUDIO_SAMPLES {
            self.audio.pop_front();
        }
        self.audio.push_back(sample);
        self.samples_produced += 1;
    }

    /// Append pending samples to `out` and clear the internal buffer.
    pub fn take_audio(&mut self, out: &mut Vec<f32>) {
        out.extend(self.audio.drain(..));
    }

    /// Move pending samples into a fresh vector and clear the internal buffer.
    pub fn drain_audio(&mut self) -> Vec<f32> {
        self.audio.drain(..).collect()
    }

    /// Number of samples currently buffered.
    pub fn audio_len(&self) -> usize {
        self.audio.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_frequency_matches_the_timer_period() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0xBF); // 50% duty, constant volume 15
        apu.write(0x4002, 0xFD); // timer low
        apu.write(0x4003, 0x08); // timer high 0, length enabled
        assert_eq!(apu.pulse1.timer_period, 0x00FD);
        let frequency = apu.pulse1.frequency_hz();
        assert!((frequency - 440.4).abs() < 1.0, "frequency was {frequency}");

        // One duty step lasts 2 * (timer + 1) CPU cycles.
        let step = apu.pulse1.duty_step;
        apu.step(2 * (u32::from(apu.pulse1.timer_period) + 1) * 2);
        assert!(apu.pulse1.duty_step != step);
    }

    #[test]
    fn pulse_envelope_decays_on_quarter_frames() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0x00); // envelope mode, period 0
        apu.write(0x4003, 0x08);
        assert_eq!(apu.pulse1.envelope_decay, 0);

        apu.quarter_frame(); // start: load 15
        assert_eq!(apu.pulse1.envelope_decay, 15);
        apu.quarter_frame();
        assert_eq!(apu.pulse1.envelope_decay, 14);
        apu.quarter_frame();
        assert_eq!(apu.pulse1.envelope_decay, 13);

        // The output falls with the envelope.
        apu.pulse1.timer_period = 100;
        apu.pulse1.duty = 2;
        apu.pulse1.duty_step = 1;
        assert_eq!(apu.pulse1.output(), 13);
    }

    #[test]
    fn triangle_is_silent_with_a_zero_linear_counter() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x04);
        apu.write(0x4008, 0x00); // linear reload 0, control clear
        apu.write(0x400A, 0x10); // timer low, keeps the channel out of the ultrasonic band
        apu.write(0x400B, 0x08); // length enabled
        apu.step(64);
        assert_eq!(apu.triangle.length, LENGTH_TABLE[1]);
        assert_eq!(apu.triangle.output(), 0);

        // Reloading the linear counter makes it sound.
        apu.write(0x4008, 0xFF);
        apu.quarter_frame();
        assert_eq!(apu.triangle.linear_counter, 0x7F);
        assert!(apu.triangle.output() > 0);
    }

    #[test]
    fn noise_lfsr_shifts_with_both_modes() {
        let mut noise = Noise::new();
        noise.timer_period = 0;
        noise.timer_counter = 0;
        noise.shift = 1;
        noise.clock_timer();
        // feedback = bit0 ^ bit1 = 1
        assert_eq!(noise.shift, 0x4000);

        let mut mode1 = Noise::new();
        mode1.mode = true;
        mode1.timer_period = 0;
        mode1.timer_counter = 0;
        mode1.shift = 0x41;
        mode1.clock_timer();
        // feedback = bit0 ^ bit6 = 1 ^ 1 = 0
        assert_eq!(mode1.shift & 0x4000, 0);

        // The enabled channel is audible around the LFSR output bit.
        let mut apu = Apu::new();
        apu.write(0x4015, 0x08);
        apu.write(0x400C, 0x1F); // constant volume 15, no halt
        apu.write(0x400E, 0x00);
        apu.write(0x400F, 0x08);
        apu.step(256);
        assert!(apu.noise.length > 0);
    }

    #[test]
    fn status_register_enables_and_clears_lengths() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x0F);
        apu.write(0x4003, 0x08);
        apu.write(0x4007, 0x08);
        apu.write(0x400B, 0x08);
        apu.write(0x400F, 0x08);
        assert_eq!(apu.read_status(), 0x0F);
        assert!(apu.pulse1.length > 0);
        assert!(apu.triangle.length > 0);

        apu.write(0x4015, 0x00);
        assert_eq!(apu.read_status(), 0x00);
        assert_eq!(apu.pulse1.length, 0);
        assert_eq!(apu.noise.length, 0);
    }

    #[test]
    fn frame_counter_raises_and_clears_the_irq_flag() {
        let mut apu = Apu::new();
        apu.write(0x4017, 0x00); // 4-step, IRQ enabled
        apu.step(14912);
        assert!(!apu.frame_irq);
        apu.step(2);
        assert!(apu.frame_irq);
        assert_eq!(apu.read_status() & 0x40, 0x40);
        assert!(!apu.frame_irq);

        // Inhibit suppresses it.
        apu.write(0x4017, 0x40);
        apu.step(29829 * 2);
        assert!(!apu.frame_irq);
    }

    #[test]
    fn dmc_registers_are_latched() {
        let mut apu = Apu::new();
        apu.write(0x4011, 0xFF);
        apu.write(0x4012, 0x12);
        apu.write(0x4013, 0x34);
        apu.write(0x4015, 0x10);
        assert_eq!(apu.dmc_direct, 0x7F);
        assert_eq!(apu.dmc_address, 0x12);
        assert_eq!(apu.dmc_length, 0x34);
        assert!(apu.dmc_enabled);
    }

    #[test]
    fn capture_records_writes_with_frame_and_cycle() {
        let mut apu = Apu::new();
        apu.frame = 7;
        apu.cycle = 1234;
        apu.write(0x4000, 0x11); // ignored while capture is off
        assert!(apu.writes().is_empty());

        apu.start_capture();
        apu.write(0x4000, 0xBF);
        apu.cycle += 4;
        apu.write(0x4015, 0x0F);
        assert_eq!(apu.writes().len(), 2);
        assert_eq!(
            apu.writes()[0],
            ApuWrite {
                frame: 7,
                cycle: 1234,
                addr: 0x4000,
                value: 0xBF
            }
        );
        assert_eq!(apu.writes()[1].addr, 0x4015);
    }

    #[test]
    fn audio_sampling_is_non_silent_and_bounded() {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x05);
        apu.write(0x4000, 0xBF); // pulse 1, 50% duty, volume 15
        apu.write(0x4002, 0x40);
        apu.write(0x4003, 0x08);
        apu.write(0x4008, 0xFF); // triangle
        apu.write(0x400A, 0x40);
        apu.write(0x400B, 0x08);
        apu.step(4_000);
        let mut samples = Vec::new();
        apu.take_audio(&mut samples);
        assert!(!samples.is_empty());
        assert!(samples.iter().any(|sample| sample.abs() > 1e-3));
        assert!(samples
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 1.0));

        // The retained buffer never grows beyond the cap.
        for _ in 0..MAX_AUDIO_SAMPLES * 2 {
            apu.push_sample();
        }
        assert!(apu.audio_len() <= MAX_AUDIO_SAMPLES);
    }

    #[test]
    fn audio_disabled_produces_nothing() {
        let mut apu = Apu::new();
        apu.audio_enabled = false;
        apu.write(0x4015, 0x01);
        apu.write(0x4000, 0xBF);
        apu.step(10_000);
        assert_eq!(apu.audio_len(), 0);
        assert!(apu.drain_audio().is_empty());
    }
}
