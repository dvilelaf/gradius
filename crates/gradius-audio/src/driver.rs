//! Native reimplementation of the Konami sound driver in the cartridge.
//!
//! The original driver is a 6502 program entered at `$ECFC` once per NMI
//! (60.0988 Hz). It walks four channel blocks at `$B0`, `$C1`, `$D2`, `$E3`
//! and turns the request table at `$EFC7` into per-frame APU register writes.
//! [`Driver`] executes the same bytecode against a memory image and produces
//! the same writes, without a CPU or the cartridge at runtime.
//!
//! The image can come from the cartridge during development or from the
//! self-contained asset pack at runtime ([`DriverBank`]). [`extract_source`]
//! builds the pack section from a cartridge bank: it copies only the request
//! table, the period table and the sequence bytes that the twelve music requests
//! and eight sound effects actually read.
//!
//! This is a prototype: it covers the music command path, the sound-effect
//! command path, the control commands used by the captured tracks, the `$F0`
//! tempo-scaling counter and the out-of-range `$F0-$FC` note aliases (the
//! period lookup reads past the twelve-entry table, exactly like the 6502).

use std::cell::RefCell;
use std::collections::BTreeSet;

/// NTSC frame rate the driver is clocked at.
pub const FRAME_RATE_HZ: f64 = 60.098_8;

/// CPU address where the cartridge maps the sound bank and the pack images it.
pub const DRIVER_BASE: u16 = 0xC000;
/// Request table address and size: three bytes per entry, 64 entries.
pub const REQUEST_TABLE: u16 = 0xEFC7;
pub const REQUEST_TABLE_LEN: usize = 192;
/// Period table address and size: twelve big-endian semitone entries.
pub const PERIOD_TABLE: u16 = 0xEFB2;
pub const PERIOD_TABLE_LEN: usize = 24;
/// The four mutable 17-byte channel blocks (`$B0`, `$C1`, `$D2`, `$E3`).
pub const CHANNEL_BLOCKS: [u16; 4] = [0x00B0, 0x00C1, 0x00D2, 0x00E3];
pub const CHANNEL_REGS: [u8; 4] = [0x00, 0x04, 0x08, 0x0C];
pub const CHANNEL_BLOCK_LEN: u8 = 17;

/// One APU register write produced by a driver frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegWrite {
    pub addr: u16,
    pub value: u8,
}

impl RegWrite {
    const fn new(addr: u16, value: u8) -> Self {
        Self { addr, value }
    }
}

/// One channel start inside a sound request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
    /// Channel index: 0 = pulse 1, 1 = pulse 2, 2 = triangle, 3 = noise.
    pub channel: u8,
    /// CPU address of the first sequence byte.
    pub ptr: u16,
}

/// Period table at `$EFB2`, one 16-bit big-endian entry per semitone.
const PERIODS: [u16; 12] = [
    0x06AE, 0x064E, 0x05F4, 0x059E, 0x054E, 0x0501, 0x04B9, 0x0476, 0x0436, 0x03F9, 0x03C0, 0x038A,
];

/// APU register offset for each channel block.
const CHANNEL_REG: [u16; 4] = [0x0000, 0x0004, 0x0008, 0x000C];

const TRIANGLE: usize = 2;

#[derive(Clone, Copy, Default)]
struct Channel {
    /// `$00` frames left in the current note.
    duration: u8,
    /// `$01` default duration used by the sound-effect path.
    default_duration: u8,
    /// `$02` request id currently owning the channel, 0 when free.
    active_id: u8,
    /// `$03/$04` sequence pointer.
    ptr: u16,
    /// `$05` APU volume/duty configuration.
    config: u8,
    /// `$06` repeat counter for `$FE`.
    repeat: u8,
    /// `$07` last period high byte written.
    period_hi: u8,
    /// `$08` current `$4000` value, including the fade nibble.
    volume: u8,
    /// `$09` bit 7 marks an active `$FD` call, low bits select the parser.
    flags: u8,
    /// `$0A` tempo multiplier set by `$D0`.
    tempo: u8,
    /// `$0B` sweep value cache.
    sweep: u8,
    /// `$0C` period offset added to `$4002`.
    period_offset: u8,
    /// `$0D` fade countdown.
    fade_counter: u8,
    /// `$0E` fade start delay.
    fade_delay: u8,
    /// `$0F` fade step.
    fade_step: u8,
    /// `$10` octave, 0 highest.
    octave: u8,
}

/// A native driver over an extracted PRG bank or a pack memory image.
pub struct Driver<'a> {
    mem: &'a [u8],
    base: u16,
    request_addr: u16,
    period_addr: u16,
    channel_reg: [u16; 4],
    channels: [Channel; 4],
    /// `$DD/$DE`: one-level `$FD` return address (the original is global).
    return_ptr: u16,
    /// `$F0`: the game's tempo scaling is enabled.
    tempo_scaling: bool,
    /// `$F1`: frames since the last tempo step.
    tempo_frames: u8,
    /// `$F2`: tempo step, incremented every 48 frames while scaling is on.
    tempo_step: u8,
    /// `$F3`: tempo limit from the last pulse-2 note; below 7 silences the
    /// triangle on the next 48-frame step.
    tempo_limit: u8,
    /// `$C3`: game mode; `$13` selects the longer fade step.
    mode: u8,
    /// Addresses read so far, when [`Driver::traced`] was used for extraction.
    reads: Option<RefCell<Vec<u16>>>,
}

impl<'a> Driver<'a> {
    /// `mem` is the region of the cartridge image that contains the driver's
    /// sequences; `base` is the CPU address of `mem[0]`. Tables are read from
    /// `mem` when they fall inside it and from the built-in copy otherwise.
    pub fn new(base: u16, mem: &'a [u8]) -> Self {
        Self::with_layout(base, mem, REQUEST_TABLE, PERIOD_TABLE, CHANNEL_REG)
    }

    /// A driver with explicit table addresses and channel register layout.
    pub fn with_layout(
        base: u16,
        mem: &'a [u8],
        request_addr: u16,
        period_addr: u16,
        channel_reg: [u16; 4],
    ) -> Self {
        Self {
            mem,
            base,
            request_addr,
            period_addr,
            channel_reg,
            channels: [Channel::default(); 4],
            return_ptr: 0,
            tempo_scaling: false,
            tempo_frames: 0,
            tempo_step: 0,
            tempo_limit: 0,
            mode: 0,
            reads: None,
        }
    }

    /// Enable or disable the game's `$F0` tempo scaling. `mode` is the `$C3`
    /// game byte; `$13` selects the longer pulse-2 fade step.
    pub fn set_tempo_scaling(&mut self, enabled: bool, mode: u8) {
        self.tempo_scaling = enabled;
        self.mode = mode;
    }

    /// Record every byte read. Used by [`extract_source`], not by playback.
    pub fn traced(mut self) -> Self {
        self.reads = Some(RefCell::new(Vec::new()));
        self
    }

    /// Forget the recorded reads, keeping tracing enabled.
    pub fn clear_reads(&self) {
        if let Some(reads) = &self.reads {
            reads.borrow_mut().clear();
        }
    }

    /// The addresses read since the last [`Driver::clear_reads`].
    pub fn reads(&self) -> Vec<u16> {
        self.reads
            .as_ref()
            .map(|reads| reads.borrow().clone())
            .unwrap_or_default()
    }

    fn read(&self, addr: u16) -> u8 {
        if let Some(reads) = &self.reads {
            reads.borrow_mut().push(addr);
        }
        self.mem
            .get(usize::from(addr.wrapping_sub(self.base)))
            .copied()
            .unwrap_or(0)
    }

    fn read16(&self, addr: u16) -> u16 {
        u16::from_le_bytes([self.read(addr), self.read(addr.wrapping_add(1))])
    }

    fn read16_be(&self, addr: u16) -> u16 {
        u16::from_be_bytes([self.read(addr), self.read(addr.wrapping_add(1))])
    }

    fn in_mem(&self, addr: u16) -> bool {
        usize::from(addr.wrapping_sub(self.base)) < self.mem.len()
    }

    fn period(&self, pitch: u8) -> u16 {
        let at = self.period_addr.wrapping_add(u16::from(pitch) * 2);
        if self.in_mem(at) && self.in_mem(at.wrapping_add(1)) {
            self.read16_be(at)
        } else {
            PERIODS.get(usize::from(pitch)).copied().unwrap_or(0)
        }
    }

    /// Read the parts of a request from the table in memory.
    ///
    /// An id's entry index is `(id & $3F) * 3`; its top two bits are the part
    /// count minus one. Malformed or out-of-image entries end the list.
    pub fn request_parts(&self, id: u8) -> Vec<Part> {
        let count = usize::from(id >> 6) + 1;
        let base = self.request_addr.wrapping_add(u16::from(id & 0x3F) * 3);
        let mut parts = Vec::with_capacity(count);
        for entry in 0..count {
            let at = base.wrapping_add((entry as u16) * 3);
            if !self.in_mem(at) || !self.in_mem(at.wrapping_add(2)) {
                break;
            }
            let channel = self.read(at) >> 2;
            if channel > 3 {
                continue;
            }
            parts.push(Part {
                channel,
                ptr: self.read16(at.wrapping_add(1)),
            });
        }
        parts
    }

    /// Start a sound request by reading its parts from the request table.
    pub fn start_id(&mut self, id: u8) -> Vec<RegWrite> {
        let parts = self.request_parts(id);
        self.start(id, &parts)
    }

    /// Start a sound request. `id` is the value written to `$EC18`; its low
    /// six bits are the channel priority and the top two bits the part count
    /// minus one in the original request table.
    ///
    /// Returns the channel-start writes (the silence written by `$EC18`).
    pub fn start(&mut self, id: u8, parts: &[Part]) -> Vec<RegWrite> {
        let priority = id & 0x3F;
        let mut out = Vec::new();
        for part in parts {
            let idx = usize::from(part.channel);
            if idx >= 4 {
                continue;
            }
            if idx == 1 {
                // A part on pulse 2 restarts the tempo counter, even when the
                // priority check rejects it.
                self.tempo_scaling = false;
                self.tempo_frames = 0;
                self.tempo_step = 0;
            }
            if priority != 0 && priority < self.channels[idx].active_id {
                continue;
            }
            let mut ch = Channel {
                active_id: 0,
                ptr: part.ptr,
                duration: 1,
                ..Channel::default()
            };
            let first = self.read(part.ptr);
            ch.flags = if first & 0xF0 == 0x20 { 0 } else { 1 };
            let reg = self.channel_reg[idx];
            if idx == TRIANGLE {
                out.push(RegWrite::new(0x4008, 0));
                out.push(RegWrite::new(0x4009, 0));
            } else {
                out.push(RegWrite::new(0x4000 | reg, 0x30));
                out.push(RegWrite::new(0x4001 | reg, 0x30));
            }
            ch.active_id = priority;
            self.channels[idx] = ch;
        }
        out
    }

    /// Run one driver frame and append its APU writes.
    pub fn tick(&mut self, out: &mut Vec<RegWrite>) {
        for idx in 0..4 {
            if self.channels[idx].active_id != 0 {
                self.tick_channel(idx, out);
            }
        }
        if self.tempo_scaling {
            self.tempo_frames = self.tempo_frames.wrapping_add(1);
            if self.tempo_frames == 0x30 {
                self.tempo_frames = 0;
                self.tempo_step = self.tempo_step.wrapping_add(1);
                if self.tempo_limit < 7 {
                    let triangle = &mut self.channels[TRIANGLE];
                    triangle.active_id = 0;
                    out.push(RegWrite::new(0x4008, 0));
                    out.push(RegWrite::new(0x4009, 0));
                }
            }
        }
    }

    fn tick_channel(&mut self, idx: usize, out: &mut Vec<RegWrite>) {
        let mut ch = self.channels[idx];
        ch.duration = ch.duration.wrapping_sub(1);
        if ch.duration != 0 {
            self.fade(idx, &mut ch, out);
            self.channels[idx] = ch;
            return;
        }

        loop {
            let cmd = self.read(ch.ptr);
            if cmd >= 0xFD {
                match cmd {
                    0xFD => {
                        // One-level call. The return address is global.
                        ch.flags |= 0x80;
                        self.return_ptr = ch.ptr.wrapping_add(3);
                        ch.ptr = self.read16(ch.ptr.wrapping_add(1));
                        ch.duration = 1;
                    }
                    0xFE => {
                        // Repeat block; count is total passes, not extra. The
                        // 6502 tests bit 7 of `next - count` (`BMI`), so a
                        // count of `$FF` resets the counter and loops forever.
                        let next = ch.repeat.wrapping_add(1);
                        let count = self.read(ch.ptr.wrapping_add(1));
                        if next == count {
                            ch.repeat = 0;
                            ch.ptr = ch.ptr.wrapping_add(4);
                        } else {
                            ch.repeat = if next.wrapping_sub(count) & 0x80 != 0 {
                                next
                            } else {
                                next.wrapping_sub(1)
                            };
                            ch.ptr = self.read16(ch.ptr.wrapping_add(2));
                        }
                        ch.duration = 1;
                    }
                    _ => {
                        // $FF: return from a call, otherwise end the channel.
                        if ch.flags & 0x80 != 0 {
                            ch.flags &= 0x7F;
                            ch.ptr = self.return_ptr;
                            ch.duration = 1;
                        } else {
                            ch.active_id = 0;
                            let reg = self.channel_reg[idx];
                            let value = if idx == TRIANGLE { 0x00 } else { 0x30 };
                            out.push(RegWrite::new(0x4000 | reg, value));
                            self.channels[idx] = ch;
                            return;
                        }
                    }
                }
                continue;
            }

            if ch.flags & 0x7F != 0 {
                if !self.music_command(idx, &mut ch, out, cmd) {
                    break;
                }
            } else {
                self.sfx_command(idx, &mut ch, out);
                break;
            }
            // Only the triangle `$D0` path asks to parse the next byte in the
            // same frame.
            continue;
        }
        self.channels[idx] = ch;
    }

    /// Execute one byte of the music parser (`$EE7C`). Returns false when the
    /// command was fully handled.
    fn music_command(
        &mut self,
        idx: usize,
        ch: &mut Channel,
        out: &mut Vec<RegWrite>,
        cmd: u8,
    ) -> bool {
        if cmd & 0xF0 == 0xD0 {
            // Instrument: tempo, APU config, optional fade/note byte.
            ch.tempo = cmd & 0x0F;
            ch.config = self.read(ch.ptr.wrapping_add(1));
            if idx == TRIANGLE {
                // Triangle reads only the config byte, then parses the next
                // command immediately.
                ch.ptr = ch.ptr.wrapping_add(2);
                return true;
            }
            let param = self.read(ch.ptr.wrapping_add(2));
            ch.fade_delay = param >> 4;
            ch.fade_step = param & 0x0F;
            let mut offset = 3u16;
            if self.read(ch.ptr.wrapping_add(offset)) & 0xF0 == 0xE0 {
                ch.octave = self.read(ch.ptr.wrapping_add(offset)) & 0x0F;
                offset += 1;
            }
            let note = self.read(ch.ptr.wrapping_add(offset));
            ch.ptr = ch.ptr.wrapping_add(offset + 1);
            self.play_note(idx, ch, note, out);
        } else if cmd & 0xF0 == 0xE0 {
            ch.octave = cmd & 0x0F;
            let note = self.read(ch.ptr.wrapping_add(1));
            ch.ptr = ch.ptr.wrapping_add(2);
            self.play_note(idx, ch, note, out);
        } else {
            ch.ptr = ch.ptr.wrapping_add(1);
            self.play_note(idx, ch, cmd, out);
        }
        false
    }

    fn play_note(&mut self, idx: usize, ch: &mut Channel, note: u8, out: &mut Vec<RegWrite>) {
        let dur_nibble = note & 0x0F;
        let duration = if dur_nibble == 0 {
            ch.tempo
        } else {
            ch.tempo.wrapping_mul(dur_nibble + 1)
        };
        ch.duration = duration;
        let reg = self.channel_reg[idx];

        if idx == TRIANGLE {
            ch.volume = ch.config;
            out.push(RegWrite::new(0x4008, ch.config));
        } else {
            ch.volume = ch.config;
            if self.tempo_scaling && idx == 1 {
                // Pulse 2 tightens its envelope as the global counter runs.
                ch.fade_delay = 6;
                ch.fade_step = if self.mode == 0x13 { 0x0D } else { 0x05 };
                self.tempo_step = self.tempo_step.min(0x0B);
                self.tempo_limit = (ch.config & 0x0F).saturating_sub(self.tempo_step);
                let scaled = (ch.config & 0xF0) | self.tempo_limit;
                ch.volume = if scaled != 0 { scaled } else { ch.config };
            }
            ch.fade_counter = duration.wrapping_add(ch.fade_delay);
            out.push(RegWrite::new(0x4000 | reg, ch.volume));
        }

        let pitch = note >> 4;
        if pitch == 0x0C {
            let value = if idx == TRIANGLE { 0x00 } else { 0x30 };
            out.push(RegWrite::new(0x4000 | reg, value));
            return;
        }
        let base = self.period(pitch);
        let shift = 4u8.saturating_sub(ch.octave);
        let period = (base >> shift).wrapping_add(u16::from(ch.period_offset));
        self.write_period(idx, ch, (period >> 8) as u8, period as u8, out);
    }

    /// Sound-effect parser (`$EDB8`): `$2x` duration/config prefixes, sweep and
    /// alternate flags, then a raw period pair.
    fn sfx_command(&self, idx: usize, ch: &mut Channel, out: &mut Vec<RegWrite>) {
        let reg = self.channel_reg[idx];
        let mut offset = 0u16;
        loop {
            let mut cmd = self.read(ch.ptr.wrapping_add(offset));
            if cmd & 0xF0 == 0x20 {
                ch.default_duration = cmd & 0x0F;
                let volume = self.read(ch.ptr.wrapping_add(offset + 1));
                ch.volume = volume;
                out.push(RegWrite::new(0x4000 | reg, volume));
                offset += 2;
                cmd = self.read(ch.ptr.wrapping_add(offset));
            }
            ch.duration = ch.default_duration;

            if idx != TRIANGLE {
                if cmd == 0x11 {
                    offset += 1;
                    ch.period_offset = self.read(ch.ptr.wrapping_add(offset));
                    offset += 1;
                    continue;
                }
                if cmd == 0x10 {
                    offset += 1;
                    let value = self.read(ch.ptr.wrapping_add(offset));
                    ch.sweep = value;
                    out.push(RegWrite::new(0x4001 | reg, value));
                    offset += 1;
                }
                if ch.volume & 0x10 != 0 {
                    ch.volume &= 0xF0;
                    let mut value = self.read(ch.ptr.wrapping_add(offset));
                    if value == 0xF8 {
                        offset += 1;
                        value = self.read(ch.ptr.wrapping_add(offset));
                    }
                    ch.volume |= value >> 4;
                    out.push(RegWrite::new(0x4000 | reg, ch.volume));
                }
            }

            // The raw-period reader re-fetches the byte at the current
            // offset, so a preceding $10 only consumes its operand.
            let mut hi = self.read(ch.ptr.wrapping_add(offset)) & 0x07;
            offset += 1;
            let mut lo = self.read(ch.ptr.wrapping_add(offset));
            if idx < TRIANGLE && ch.period_offset != 0 {
                let sum = u16::from(lo) + u16::from(ch.period_offset);
                lo = sum as u8;
                hi = hi.wrapping_add((sum >> 8) as u8);
            }
            self.write_period(idx, ch, hi, lo, out);
            ch.ptr = ch.ptr.wrapping_add(offset + 1);
            return;
        }
    }

    /// Per-frame volume fade (`$EE2F`).
    fn fade(&self, idx: usize, ch: &mut Channel, out: &mut Vec<RegWrite>) {
        if idx == TRIANGLE || ch.flags & 0x7F == 0 {
            return;
        }
        let previous = self.read(ch.ptr.wrapping_sub(1));
        if previous & 0xF0 == 0xC0 {
            return;
        }

        let mut a = ch.fade_counter.wrapping_sub(1);
        ch.fade_counter = a;
        if a == ch.duration {
            let step = a.wrapping_sub(ch.fade_step);
            let underflow = a < ch.fade_step;
            if !underflow && step != 0 {
                return;
            }
        } else {
            a = a.wrapping_sub(1);
            ch.fade_counter = a;
        }

        if ch.volume & 0x0F == 0 {
            return;
        }
        ch.volume = ch.volume.wrapping_sub(1);
        out.push(RegWrite::new(0x4000 | self.channel_reg[idx], ch.volume));
    }

    /// Write the period pair, mirroring the driver's `$07` high-byte cache.
    fn write_period(&self, idx: usize, ch: &mut Channel, hi: u8, lo: u8, out: &mut Vec<RegWrite>) {
        let reg = self.channel_reg[idx];
        let hi = hi | 0x08;
        if idx == TRIANGLE || hi != ch.period_hi || ch.volume & 0x10 == 0 || ch.sweep != 0 {
            ch.period_hi = hi;
            out.push(RegWrite::new(0x4003 | reg, hi));
        }
        out.push(RegWrite::new(0x4002 | reg, lo));
    }
}

/// A complete driver memory image loaded from an asset pack.
#[derive(Clone, Copy, Debug)]
pub struct DriverBank {
    base: u16,
    mem: &'static [u8],
    request_addr: u16,
    period_addr: u16,
    channel_reg: [u16; 4],
    channel_blocks: [(u16, u8); 4],
    channel_block_len: u8,
}

impl DriverBank {
    pub fn new(
        base: u16,
        mem: &'static [u8],
        request_addr: u16,
        period_addr: u16,
        channel_blocks: [(u16, u8); 4],
        channel_block_len: u8,
    ) -> Self {
        Self {
            base,
            mem,
            request_addr,
            period_addr,
            channel_reg: channel_blocks.map(|(_, reg)| u16::from(reg)),
            channel_blocks,
            channel_block_len,
        }
    }

    pub fn base(&self) -> u16 {
        self.base
    }

    pub fn mem(&self) -> &'static [u8] {
        self.mem
    }

    pub fn request_addr(&self) -> u16 {
        self.request_addr
    }

    pub fn period_addr(&self) -> u16 {
        self.period_addr
    }

    pub fn channel_reg(&self) -> [u16; 4] {
        self.channel_reg
    }

    /// Base address and APU register offset of the four channel blocks.
    pub fn channel_blocks(&self) -> [(u16, u8); 4] {
        self.channel_blocks
    }

    /// Size of one mutable channel block (`$11`).
    pub fn channel_block_len(&self) -> u8 {
        self.channel_block_len
    }

    /// A fresh driver over this image.
    pub fn driver(&self) -> Driver<'static> {
        Driver::with_layout(
            self.base,
            self.mem,
            self.request_addr,
            self.period_addr,
            self.channel_reg,
        )
    }
}

/// Music request ids covered by the pack's driver section, in the same order
/// as [`crate::Music::ALL`].
pub const MUSIC_REQUESTS: [u8; 12] = [
    0x90, 0x93, 0x96, 0xA5, 0xAC, 0xAF, 0x59, 0x5B, 0x5D, 0x5F, 0x61, 0x63,
];
/// Sound-effect request ids covered by the pack's driver section.
pub const SFX_REQUESTS: [u8; 12] = [
    0x01, 0x06, 0x05, 0x09, 0x0D, 0x0E, 0x36, 0x35, 0x03, 0x04, 0x3B, 0x0B,
];

/// The logical driver assets copied out of one cartridge bank.
///
/// The four 17-byte channel blocks are mutable runtime state (all zero at
/// request time), so they have no ROM bytes; `channels` records their base
/// addresses and APU register offsets instead.
#[derive(Clone, Debug)]
pub struct DriverSource {
    pub base: u16,
    pub request_addr: u16,
    pub request: Vec<u8>,
    pub period_addr: u16,
    pub period: Vec<u8>,
    pub channels: [(u16, u8); 4],
    pub segments: Vec<(u16, Vec<u8>)>,
}

/// Copy the request table, period table and every sequence byte the twenty
/// supported requests actually read.
///
/// Each request is run through a traced driver until its read set has been
/// stable for 6,000 frames (100 s), which covers every lead-in and loop in the
/// captured soundtrack. Only bytes inside `mem` and outside the two tables are
/// kept, then adjacent addresses are merged into segments. This is what makes
/// the pack self-contained without copying unrelated cartridge data.
pub fn extract_source(base: u16, mem: &[u8]) -> DriverSource {
    let request = read_bytes(mem, base, REQUEST_TABLE, REQUEST_TABLE_LEN);
    let period = read_bytes(mem, base, PERIOD_TABLE, PERIOD_TABLE_LEN);
    let mut union = BTreeSet::new();

    for &id in MUSIC_REQUESTS.iter().chain(SFX_REQUESTS.iter()) {
        let mut driver = Driver::new(base, mem).traced();
        let parts = driver.request_parts(id);
        driver.clear_reads();
        let _ = driver.start(id, &parts);
        let mut seen: BTreeSet<u16> = driver
            .reads()
            .into_iter()
            .filter(|&addr| {
                usize::from(addr.wrapping_sub(base)) < mem.len()
                    && !in_range(addr, REQUEST_TABLE, REQUEST_TABLE_LEN)
                    && !in_range(addr, PERIOD_TABLE, PERIOD_TABLE_LEN)
            })
            .collect();
        let mut stable = 0u32;
        for _ in 0..200_000 {
            driver.clear_reads();
            let mut writes = Vec::new();
            driver.tick(&mut writes);
            let mut grew = false;
            for addr in driver.reads() {
                if usize::from(addr.wrapping_sub(base)) < mem.len()
                    && !in_range(addr, REQUEST_TABLE, REQUEST_TABLE_LEN)
                    && !in_range(addr, PERIOD_TABLE, PERIOD_TABLE_LEN)
                {
                    grew |= seen.insert(addr);
                }
            }
            if grew {
                stable = 0;
            } else {
                stable += 1;
                if stable >= 6_000 {
                    break;
                }
            }
        }
        union.extend(seen);
    }

    let mut segments: Vec<(u16, Vec<u8>)> = Vec::new();
    for addr in union {
        match segments.last_mut() {
            Some((start, bytes)) if addr == start.wrapping_add(bytes.len() as u16) => {
                bytes.push(read_mem(mem, base, addr));
            }
            _ => segments.push((addr, vec![read_mem(mem, base, addr)])),
        }
    }

    DriverSource {
        base,
        request_addr: REQUEST_TABLE,
        request,
        period_addr: PERIOD_TABLE,
        period,
        channels: [
            (CHANNEL_BLOCKS[0], CHANNEL_REGS[0]),
            (CHANNEL_BLOCKS[1], CHANNEL_REGS[1]),
            (CHANNEL_BLOCKS[2], CHANNEL_REGS[2]),
            (CHANNEL_BLOCKS[3], CHANNEL_REGS[3]),
        ],
        segments,
    }
}

fn in_range(addr: u16, start: u16, len: usize) -> bool {
    let offset = u32::from(addr).wrapping_sub(u32::from(start));
    offset < len as u32
}

fn read_mem(mem: &[u8], base: u16, addr: u16) -> u8 {
    mem.get(usize::from(addr.wrapping_sub(base)))
        .copied()
        .unwrap_or(0)
}

fn read_bytes(mem: &[u8], base: u16, addr: u16, len: usize) -> Vec<u8> {
    (0..len)
        .map(|offset| read_mem(mem, base, addr.wrapping_add(offset as u16)))
        .collect()
}
