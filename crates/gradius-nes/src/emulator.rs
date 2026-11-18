//! Headless NES oracle.
//!
//! Runs the original cartridge deterministically, one scanline at a time, and
//! optionally records which code executed. This is the reference implementation
//! used to extract data and to check the native port.

use gradius_cart::Cartridge;

use crate::apu::ApuWrite;
use crate::bus::Bus;
use crate::cpu::{Bus as CpuBus, Cpu};
use crate::ppu::{fnv1a, PpuSnapshot, VramWrite, FRAMEBUFFER_LEN, HEIGHT, WIDTH};

#[derive(Debug, thiserror::Error)]
pub enum EmulatorError {
    #[error("cartridge is not a 32 KiB mapper-3 image: {0}")]
    UnsupportedCartridge(#[from] gradius_cart::CartError),
}

/// Read-only view of the player's object slot and the global counters that go
/// with it. Every field is decoded from CPU RAM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerState {
    /// X position of the ship (`$0360`).
    pub x_int: u8,
    /// X sub-pixel, 8.8 fixed point (`$0380`).
    pub x_frac: u8,
    /// Y position of the ship (`$0320`).
    pub y_int: u8,
    /// Y sub-pixel, 8.8 fixed point (`$0340`).
    pub y_frac: u8,
    /// Speed-up level (`$40`).
    pub speed_level: u8,
    /// Selected weapon (`$44`).
    pub weapon: u8,
    /// Number of options (`$45`).
    pub options: u8,
    /// Shield state (`$46`).
    pub shield: u8,
    /// Lives (`$20`).
    pub lives: u8,
    /// Six-digit BCD score, in RAM order `$07E4`, `$07E5`, `$07E6`
    /// (least significant pair first, most significant last).
    pub score_bcd: [u8; 3],
    /// Horizontal stage scroll, `$3E` low byte and `$3F` high byte. The high
    /// byte's bit 0 selects the nametable.
    pub scroll: u16,
}

impl PlayerState {
    /// Decode [`Self::score_bcd`] into a plain number, e.g. `0x07_02_00`
    /// becomes 70200.
    pub fn score(&self) -> u32 {
        let mut value = 0u32;
        for byte in self.score_bcd.iter().rev() {
            value = value * 100 + u32::from(byte >> 4) * 10 + u32::from(byte & 0x0F);
        }
        value
    }
}

/// One live entry of the 32-slot object table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectState {
    /// Slot index, 0-31. Slot 0 is the player.
    pub slot: u8,
    /// Sprite frame (`$0120`).
    pub frame: u8,
    /// Subtype / tile base (`$010C`).
    pub subtype: u8,
    /// Object type (`$030C`).
    pub kind: u8,
    /// Spawn/update sub-state (`$03BC`).
    pub substate: u8,
    /// Per-handler AI state / damage gate (`$048C`), the value the native core
    /// calls `ai_state` and reports in `Game::dump_state`.
    pub ai_state: u8,
    /// Y position (`$0320`) and sub-pixel (`$0340`).
    pub y_int: u8,
    pub y_frac: u8,
    /// X position (`$0360`) and sub-pixel (`$0380`).
    pub x_int: u8,
    pub x_frac: u8,
}

pub struct Emulator {
    pub cpu: Cpu,
    pub bus: Bus,
    framebuffer: Vec<u8>,
    pub frames: u64,
}

impl Emulator {
    pub fn new(cart: &Cartridge) -> Result<Self, EmulatorError> {
        let prg = cart.cpu_window()?;
        let reset = u16::from(prg[0x7FFC]) | (u16::from(prg[0x7FFD]) << 8);
        Ok(Self {
            cpu: Cpu::new(reset),
            bus: Bus::new(cart),
            framebuffer: vec![0; FRAMEBUFFER_LEN],
            frames: 0,
        })
    }

    /// Set controller 1's buttons. Kept as the single-pad entry point.
    pub fn set_buttons(&mut self, buttons: u8) {
        self.set_buttons_p1(buttons);
    }

    /// Set controller 1's buttons (`$4016`).
    pub fn set_buttons_p1(&mut self, buttons: u8) {
        self.bus.controller.set_buttons(buttons);
    }

    /// Set controller 2's buttons (`$4017`).
    pub fn set_buttons_p2(&mut self, buttons: u8) {
        self.bus.controller2.set_buttons(buttons);
    }

    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Stable FNV-1a hash of the RGBA framebuffer, for frame comparisons.
    pub fn frame_hash(&self) -> u64 {
        fnv1a(&self.framebuffer)
    }

    /// Immutable PPU view for debug tooling.
    pub fn ppu_snapshot(&self) -> PpuSnapshot {
        self.bus.ppu.snapshot()
    }

    /// Copy of the 2 KiB of CPU RAM.
    pub fn ram_snapshot(&self) -> [u8; 2048] {
        self.bus.ram
    }

    /// Copy of the 256-byte OAM.
    pub fn oam_snapshot(&self) -> [u8; 256] {
        self.bus.ppu.oam
    }

    /// Player state decoded from CPU RAM.
    pub fn player_state(&self) -> PlayerState {
        let ram = &self.bus.ram;
        let score_bcd = [ram[0x07E4], ram[0x07E5], ram[0x07E6]];
        PlayerState {
            x_int: ram[0x0360],
            x_frac: ram[0x0380],
            y_int: ram[0x0320],
            y_frac: ram[0x0340],
            speed_level: ram[0x0040],
            weapon: ram[0x0044],
            options: ram[0x0045],
            shield: ram[0x0046],
            lives: ram[0x0020],
            score_bcd,
            scroll: u16::from(ram[0x3E]) | (u16::from(ram[0x3F]) << 8),
        }
    }

    /// Every active object slot.
    ///
    /// The game's parallel arrays are indexed at `base + 0x0C + slot`:
    /// `$A521` clears `$0100,X`..`$04E0,X` with `X = $A8 + 12`, and `$A3DE`
    /// writes frame `$012C`, Y `$032C`/`$034C`, X `$036C`/`$038C`, kind
    /// `$030C` and state `$03BC`/`$048C`. The player's own `$0320`/`$0360`
    /// pair is outside this table.
    pub fn objects(&self) -> Vec<ObjectState> {
        let ram = &self.bus.ram;
        (0..32u8)
            .filter(|slot| ram[0x012C + usize::from(*slot)] != 0)
            .map(|slot| {
                let i = usize::from(slot);
                ObjectState {
                    slot,
                    frame: ram[0x012C + i],
                    subtype: ram[0x010C + i],
                    kind: ram[0x030C + i],
                    substate: ram[0x03BC + i],
                    ai_state: ram[0x048C + i],
                    y_int: ram[0x032C + i],
                    y_frac: ram[0x034C + i],
                    x_int: ram[0x036C + i],
                    x_frac: ram[0x038C + i],
                }
            })
            .collect()
    }

    /// Enable or disable APU register-write capture.
    pub fn set_capture_apu(&mut self, enabled: bool) {
        if enabled {
            self.bus.apu.start_capture();
        } else {
            self.bus.apu.stop_capture();
        }
    }

    /// Enable or disable PPU VRAM (nametable and palette) write capture.
    pub fn set_capture_vram(&mut self, enabled: bool) {
        if enabled {
            self.bus.ppu.start_vram_capture();
        } else {
            self.bus.ppu.stop_vram_capture();
        }
    }

    /// APU register writes recorded so far, in execution order.
    pub fn apu_writes(&self) -> &[ApuWrite] {
        self.bus.apu.writes()
    }

    /// PPU VRAM writes recorded so far, in execution order.
    pub fn vram_writes(&self) -> &[VramWrite] {
        self.bus.ppu.vram_writes()
    }

    /// Enable or disable audio sample production (on by default).
    pub fn set_audio_enabled(&mut self, enabled: bool) {
        self.bus.apu.audio_enabled = enabled;
    }

    /// Append pending audio samples (48 kHz, `-1.0..=1.0`) to `out`.
    pub fn take_audio(&mut self, out: &mut Vec<f32>) {
        self.bus.apu.take_audio(out);
    }

    /// Move pending audio samples into a fresh vector.
    pub fn drain_audio(&mut self) -> Vec<f32> {
        self.bus.apu.drain_audio()
    }

    /// Encodes the framebuffer as RGB (dropping alpha) for PNG output.
    pub fn rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(WIDTH * HEIGHT * 3);
        for pixel in self.framebuffer.chunks_exact(4) {
            out.extend_from_slice(&pixel[..3]);
        }
        out
    }

    /// Run exactly one video frame (262 scanlines).
    pub fn run_frame(&mut self) {
        self.bus.apu.frame = self.frames;
        self.bus.ppu.frame = self.frames;
        for line in 0..262usize {
            self.bus.ppu.line = line;
            let hit_before = self.bus.ppu.sprite_zero_hit;
            if line < HEIGHT {
                self.bus.ppu.render_line(line, &mut self.framebuffer);
            }
            // A sprite-0 overlap found while rendering this line becomes
            // visible to the CPU only when the PPU outputs the hit pixel, not
            // at the start of the line. Hold status bit 6 back until then so
            // `$2002` polling sees the same mid-line timing as hardware.
            let deferred_hit = match (hit_before, self.bus.ppu.sprite_zero_hit) {
                (None, Some(hit)) if hit.line == line => Some(hit),
                _ => None,
            };
            if deferred_hit.is_some() {
                self.bus.ppu.status &= !0x40;
            }
            if line == 241 {
                self.bus.ppu.start_vblank();
                if self.bus.ppu.nmi_enabled() {
                    self.bus.apu.cycle = self.cpu.cycles;
                    self.cpu.nmi(&mut self.bus);
                    self.bus.apu.step(7);
                }
            }
            if line == 261 {
                self.bus.ppu.end_vblank();
                // The pre-render line reloads the full scroll latch into `v`.
                // Rendering must be enabled for the real PPU to do this.
                if self.bus.ppu.mask & 0x18 != 0 {
                    self.bus.ppu.reload_scroll();
                }
            }

            // NTSC lines are 113.666 CPU cycles; approximate with a 3-line
            // repeating pattern (113, 114, 113).
            let budget = 113 + u64::from(line % 3 == 2);
            let target = self.cpu.cycles + budget;
            let mut hit_at = deferred_hit.map(|hit| self.cpu.cycles + u64::from(hit.dot) / 3);
            while self.cpu.cycles < target {
                if let Some(at) = hit_at {
                    if self.cpu.cycles >= at {
                        self.bus.ppu.status |= 0x40;
                        hit_at = None;
                    }
                }
                let before = self.cpu.cycles;
                self.bus.apu.cycle = before;
                let cycles = self.cpu.step(&mut self.bus);
                let delta = (self.cpu.cycles - before) as u32;
                self.bus.apu.step(delta);
                let stall = self.bus.take_stall();
                if stall > 0 {
                    self.bus.apu.step(stall);
                }
                self.cpu.cycles += u64::from(stall);
                if cycles == 0 {
                    break;
                }
            }
            // The hit pixel is always inside the visible line, so this only
            // triggers if a CPU step overshot the budget above.
            if hit_at.is_some() {
                self.bus.ppu.status |= 0x40;
            }
            if self.cpu.jammed {
                break;
            }
        }
        self.frames += 1;
    }

    pub fn run_frames(&mut self, frames: u64) {
        for _ in 0..frames {
            self.run_frame();
        }
    }

    /// Execute a single CPU instruction without advancing the PPU. Used by the
    /// trace log to debug boot code.
    pub fn step_instruction(&mut self) -> u32 {
        let before = self.cpu.cycles;
        self.bus.apu.cycle = before;
        let cycles = self.cpu.step(&mut self.bus);
        let delta = (self.cpu.cycles - before) as u32;
        self.bus.apu.step(delta);
        cycles
    }

    pub fn opcode_at(&mut self, addr: u16) -> u8 {
        self.bus.read(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_cart::{Header, Mirroring};

    fn cart() -> Cartridge {
        Cartridge {
            header: Header {
                prg_units: 2,
                chr_units: 4,
                mapper: 3,
                mirroring: Mirroring::Vertical,
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
    fn frame_hash_is_stable_and_covers_the_framebuffer() {
        let cart = cart();
        let mut first = Emulator::new(&cart).unwrap();
        first.run_frame();
        assert_eq!(first.frame_hash(), fnv1a(first.framebuffer()));

        let mut second = Emulator::new(&cart).unwrap();
        second.run_frame();
        assert_eq!(first.frame_hash(), second.frame_hash());

        // Changing the backdrop must change the hash.
        first.bus.ppu.palette[0] = 0x30;
        first.run_frame();
        assert_ne!(first.frame_hash(), second.frame_hash());
    }

    #[test]
    fn ppu_snapshot_reflects_ppu_state() {
        let cart = cart();
        let mut emulator = Emulator::new(&cart).unwrap();
        {
            let ppu = &mut emulator.bus.ppu;
            ppu.ctrl = 0xA8;
            ppu.mask = 0x1E;
            ppu.set_chr_bank(1);
            ppu.t = 0x4020;
            ppu.fine_x = 3;
            ppu.oam[0..4].copy_from_slice(&[9, 0x6D, 0x23, 0xF8]);
        }
        let snapshot = emulator.ppu_snapshot();
        assert_eq!(snapshot.ctrl, 0xA8);
        assert_eq!(snapshot.mask, 0x1E);
        assert_eq!(snapshot.chr_bank, 1);
        assert_eq!(snapshot.t, 0x4020);
        assert_eq!(snapshot.fine_x, 3);
        assert_eq!(&snapshot.oam[..4], &[9, 0x6D, 0x23, 0xF8]);
        assert_eq!(snapshot.nametable_checksum, fnv1a(&emulator.bus.ppu.vram));
    }

    #[test]
    fn ram_oam_and_audio_snapshots_are_exposed() {
        let cart = cart();
        let mut emulator = Emulator::new(&cart).unwrap();
        emulator.bus.apu.write(0x4015, 0x01);
        emulator.bus.apu.write(0x4000, 0xBF);
        emulator.bus.apu.write(0x4002, 0x40);
        emulator.bus.apu.write(0x4003, 0x08);
        emulator.bus.ppu.oam[3] = 0x6D;
        emulator.run_frames(2);

        let mut samples = Vec::new();
        emulator.take_audio(&mut samples);
        assert!(!samples.is_empty(), "the APU should have produced samples");
        assert!(samples.iter().any(|sample| sample.abs() > 1e-4));
        emulator.run_frame();
        assert!(!emulator.drain_audio().is_empty());

        let ram = emulator.ram_snapshot();
        emulator.bus.ram[0] = 0xAA;
        assert_eq!(ram[0], 0x00, "ram_snapshot must copy");
        let oam = emulator.oam_snapshot();
        assert_eq!(oam[3], 0x6D);
        emulator.bus.ppu.oam[3] = 0;
        assert_eq!(oam[3], 0x6D, "oam_snapshot must copy");
    }

    #[test]
    fn player_and_object_state_decode_cpu_ram() {
        let cart = cart();
        let mut emulator = Emulator::new(&cart).unwrap();
        {
            let ram = &mut emulator.bus.ram;
            ram[0x0360] = 80;
            ram[0x0380] = 0x80;
            ram[0x0320] = 96;
            ram[0x0340] = 0x40;
            ram[0x0040] = 2;
            ram[0x0044] = 1;
            ram[0x0045] = 3;
            ram[0x0046] = 1;
            ram[0x0020] = 3;
            ram[0x07E4] = 0x00;
            ram[0x07E5] = 0x08;
            ram[0x07E6] = 0x00;
            ram[0x3E] = 0x34;
            ram[0x3F] = 0x02;
            // Player at $0320/$0360 (outside the object table); slot 0 and
            // slot 5 objects at $012C/$032C/$036C. Slot 7 is dead.
            ram[0x012C] = 1;
            ram[0x012C + 5] = 4;
            ram[0x012C + 7] = 0;
            ram[0x010C] = 0x11;
            ram[0x030C] = 0x80;
            ram[0x032C] = 0x20;
            ram[0x034C] = 0x01;
            ram[0x036C] = 0x30;
            ram[0x038C] = 0x02;
            ram[0x010C + 5] = 0x22;
            ram[0x030C + 5] = 0x0A;
            ram[0x03BC + 5] = 0x03;
            ram[0x048C + 5] = 0x07;
            ram[0x032C + 5] = 0x40;
            ram[0x034C + 5] = 0x10;
            ram[0x036C + 5] = 0xC8;
            ram[0x038C + 5] = 0x20;
        }

        let player = emulator.player_state();
        assert_eq!(player.x_int, 80);
        assert_eq!(player.x_frac, 0x80);
        assert_eq!(player.y_int, 96);
        assert_eq!(player.y_frac, 0x40);
        assert_eq!(player.speed_level, 2);
        assert_eq!(player.weapon, 1);
        assert_eq!(player.options, 3);
        assert_eq!(player.shield, 1);
        assert_eq!(player.lives, 3);
        assert_eq!(player.score_bcd, [0x00, 0x08, 0x00]);
        assert_eq!(player.score(), 800);
        assert_eq!(player.scroll, 0x0234);

        let objects = emulator.objects();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].slot, 0);
        assert_eq!(objects[0].frame, 1);
        assert_eq!(objects[0].subtype, 0x11);
        assert_eq!(objects[0].kind, 0x80);
        assert_eq!(objects[0].y_int, 0x20);
        assert_eq!(objects[0].y_frac, 0x01);
        assert_eq!(objects[0].x_int, 0x30);
        assert_eq!(objects[0].x_frac, 0x02);
        assert_eq!(objects[1].slot, 5);
        assert_eq!(objects[1].frame, 4);
        assert_eq!(objects[1].subtype, 0x22);
        assert_eq!(objects[1].kind, 0x0A);
        assert_eq!(objects[1].substate, 0x03);
        assert_eq!(objects[1].ai_state, 0x07);
        assert_eq!(objects[1].y_int, 0x40);
        assert_eq!(objects[1].y_frac, 0x10);
        assert_eq!(objects[1].x_int, 0xC8);
        assert_eq!(objects[1].x_frac, 0x20);
    }

    /// A synthetic cartridge whose program fills the nametable with an opaque
    /// tile, puts sprite 0 over it and then polls `$2002` for the hit, exactly
    /// like the game's HUD split. The hit must arrive while the CPU polls and
    /// must be gone at pre-render.
    #[test]
    fn sprite_zero_hit_is_visible_to_cpu_polling() {
        let mut cart = cart();
        // Opaque pattern for tile 1 in both bit planes.
        cart.chr[0x10..0x20].fill(0xFF);

        #[rustfmt::skip]
        let program: [u8; 0x6D] = [
            0x78,                   // SEI
            0xA9, 0x00,             // LDA #$00
            0x8D, 0x00, 0x20,       // STA $2000  (NMI off, 8x8 sprites)
            0xA9, 0x3F,             // LDA #$3F
            0x8D, 0x06, 0x20,       // STA $2006
            0xA9, 0x00,             // LDA #$00
            0x8D, 0x06, 0x20,       // STA $2006
            0xA9, 0x0F,             // LDA #$0F
            0x8D, 0x07, 0x20,       // STA $2007  (backdrop)
            0xA9, 0x20,             // LDA #$20
            0x8D, 0x06, 0x20,       // STA $2006
            0xA9, 0x00,             // LDA #$00
            0x8D, 0x06, 0x20,       // STA $2006
            0xA0, 0x00,             // LDY #$00
            0xA2, 0x04,             // LDX #$04
            0xA9, 0x01,             // fill: LDA #$01
            0x8D, 0x07, 0x20,       // STA $2007
            0xC8,                   // INY
            0xD0, 0xF8,             // BNE fill
            0xCA,                   // DEX
            0xD0, 0xF5,             // BNE fill
            0xA9, 0x00,             // LDA #$00
            0x8D, 0x03, 0x20,       // STA $2003  (OAMADDR 0)
            0xA9, 0x13,             // LDA #$13  (Y = 19 -> top line 20)
            0x8D, 0x04, 0x20,       // STA $2004
            0xA9, 0x01,             // LDA #$01  (tile 1)
            0x8D, 0x04, 0x20,       // STA $2004
            0xA9, 0x00,             // LDA #$00  (attributes)
            0x8D, 0x04, 0x20,       // STA $2004
            0xA9, 0x10,             // LDA #$10  (X = 16)
            0x8D, 0x04, 0x20,       // STA $2004
            0xAD, 0x02, 0x20,       // vbl: LDA $2002
            0x29, 0x80,             // AND #$80
            0xF0, 0xF9,             // BEQ vbl
            0xA9, 0x00,             // LDA #$00
            0x8D, 0x05, 0x20,       // STA $2005
            0x8D, 0x05, 0x20,       // STA $2005
            0xA9, 0x1E,             // LDA #$1E
            0x8D, 0x01, 0x20,       // STA $2001  (bg + sprites on)
            0xAD, 0x02, 0x20,       // hit: LDA $2002
            0x29, 0x40,             // AND #$40
            0xF0, 0xF9,             // BEQ hit
            0xA9, 0xAA,             // LDA #$AA
            0x85, 0x00,             // STA $00
            0xA9, 0x55,             // LDA #$55
            0x85, 0x01,             // STA $01
            0x4C, 0x6A, 0x80,       // JMP $806A
        ];
        cart.prg[..program.len()].copy_from_slice(&program);
        cart.prg[0x7FFC] = 0x00;
        cart.prg[0x7FFD] = 0x80;

        let mut emulator = Emulator::new(&cart).unwrap();
        // Frame 0 only programs the PPU and waits for vblank.
        emulator.run_frame();
        assert_eq!(emulator.bus.ram[0], 0, "no hit before rendering starts");

        // Frame 1 renders line 20 and the polling loop must see the flag.
        emulator.run_frame();
        assert_eq!(emulator.bus.ram[0], 0xAA, "the CPU poll should see the hit");
        assert_eq!(emulator.bus.ram[1], 0x55, "and continue past the loop");
        assert_eq!(
            emulator.cpu.pc, 0x806A,
            "the program should be in its idle JMP"
        );
        // Pre-render already cleared it for the next frame, as on hardware.
        assert_eq!(emulator.bus.ppu.status & 0x40, 0);
        assert_eq!(emulator.bus.ppu.sprite_zero_hit, None);
        assert!(!emulator.cpu.jammed);
    }

    #[test]
    fn reset_vector_is_read_from_prg() {
        let mut cart = cart();
        cart.prg[0x7FFC] = 0x00;
        cart.prg[0x7FFD] = 0x80;
        let emulator = Emulator::new(&cart).unwrap();
        assert_eq!(emulator.cpu.pc, 0x8000);
    }

    #[test]
    fn synthetic_program_reads_both_controllers() {
        use crate::bus::{BUTTON_A, BUTTON_B, BUTTON_DOWN, BUTTON_LEFT, BUTTON_START};

        let mut cart = cart();
        // Strobe both pads once, then read $4016 eight times into $10-$17 and
        // $4017 eight times into $18-$1F, storing one bit per byte.
        #[rustfmt::skip]
        let program: [u8; 0x29] = [
            0xA9, 0x01,             // LDA #$01
            0x8D, 0x16, 0x40,       // STA $4016
            0xA9, 0x00,             // LDA #$00
            0x8D, 0x16, 0x40,       // STA $4016
            0xA2, 0x00,             // LDX #$00
            0xAD, 0x16, 0x40,       // LDA $4016   ; p1 loop
            0x29, 0x01,             // AND #$01
            0x95, 0x10,             // STA $10,X
            0xE8,                   // INX
            0xE0, 0x08,             // CPX #$08
            0xD0, 0xF4,             // BNE p1 loop
            0xA2, 0x00,             // LDX #$00
            0xAD, 0x17, 0x40,       // LDA $4017   ; p2 loop
            0x29, 0x01,             // AND #$01
            0x95, 0x18,             // STA $18,X
            0xE8,                   // INX
            0xE0, 0x08,             // CPX #$08
            0xD0, 0xF4,             // BNE p2 loop
            0x4C, 0x26, 0x80,       // JMP $8026
        ];
        cart.prg[..program.len()].copy_from_slice(&program);
        cart.prg[0x7FFC] = 0x00;
        cart.prg[0x7FFD] = 0x80;

        let mut emulator = Emulator::new(&cart).unwrap();
        emulator.set_buttons_p1(BUTTON_A | BUTTON_START);
        emulator.set_buttons_p2(BUTTON_B | BUTTON_DOWN | BUTTON_LEFT);
        emulator.run_frame();

        let ram = emulator.ram_snapshot();
        assert_eq!(
            &ram[0x10..0x18],
            &[1, 0, 0, 1, 0, 0, 0, 0],
            "p1 in A/B/Select/Start/Up/Down/Left/Right order"
        );
        assert_eq!(
            &ram[0x18..0x20],
            &[0, 1, 0, 0, 0, 1, 1, 0],
            "p2 in A/B/Select/Start/Up/Down/Left/Right order"
        );
        assert!(
            emulator.bus.trace.was_executed(0x8026),
            "the program should reach its idle loop"
        );
    }
}
