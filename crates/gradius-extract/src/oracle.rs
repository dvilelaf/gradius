//! Frame runner shared by the analysis commands.
//!
//! `Emulator::run_frame` is the normal path. `--assist` switches to a copy of
//! the same scanline loop that also injects the sprite-0 hit the game's HUD
//! split waits for. The current PPU model raises the real hit (see
//! `xtask verify-sprite0`), so the assist is a legacy workaround for
//! reproducing traces made before the deferred-hit fix; every assisted run
//! reports how many pulses it injected so the result is reproducible.

use gradius_nes::{Emulator, FRAMEBUFFER_LEN, SCREEN_HEIGHT};

/// Addresses of the two PPUSTATUS polling loops in the HUD split routine:
/// The HUD split routine polls PPUSTATUS twice: `LDA $2002 / AND #$40 / BNE`
/// at `$9A96` spins while the sprite-0 flag is set (wait for a stale hit to
/// clear) and `LDA $2002 / AND #$40 / BEQ` at `$9A9D` spins until a new hit
/// arrives. Reaching either one mid-instruction means the CPU is spinning.
pub const WAIT_CLEAR: std::ops::RangeInclusive<u16> = 0x9A96..=0x9A9C;
pub const WAIT_SET: std::ops::RangeInclusive<u16> = 0x9A9D..=0x9AA3;

/// A scripted input plan: buttons held for the whole run plus timed presses.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub held: u8,
    /// `(start_frame, buttons, duration_frames)`, sorted by start frame.
    pub schedule: Vec<(u64, u8, u64)>,
}

impl Plan {
    /// Buttons active on `frame`: the held set plus the most recent press that
    /// is still inside its duration window.
    pub fn buttons(&self, frame: u64) -> u8 {
        let scripted = self
            .schedule
            .iter()
            .rev()
            .find(|(start, _, duration)| frame >= *start && frame - start < *duration)
            .map(|(_, buttons, _)| *buttons)
            .unwrap_or(0);
        self.held | scripted
    }
}

/// One frame that ended with the player in the dying state (`$0100 >= 2`)
/// while `keep_alive` was on. `keep_alive` restores the state afterwards, so
/// this is the only trace of the hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LethalHit {
    pub frame: u64,
    pub scroll: u16,
    pub x: u8,
    pub y: u8,
}

/// Decode the hit state from CPU RAM. Returns `None` unless `$0100 >= 2`.
pub fn lethal_hit(ram: &[u8], frame: u64) -> Option<LethalHit> {
    if ram[0x0100] < 2 {
        return None;
    }
    Some(LethalHit {
        frame,
        scroll: u16::from(ram[0x3E]) | (u16::from(ram[0x3F]) << 8),
        x: ram[0x0360],
        y: ram[0x0320],
    })
}

/// Frame runner with an optional assist counter.
pub struct Runner {
    framebuffer: Vec<u8>,
    pub assist_pulses: u64,
    /// Hits that `keep_alive` masked, in frame order.
    pub lethal_hits: Vec<LethalHit>,
    /// Keep only the shield timer (`$46`) at 5 after every frame (analysis
    /// only). Enemy, bullet and option contact is survivable, but the
    /// `$C2B6` map probe bypasses `$46`, so true map collisions still kill.
    pub shield: bool,
    /// Keep the player alive after every frame (analysis only).
    ///
    /// `$C1B7`/`$C241` treat a non-zero `$46` as invulnerability and decrement
    /// it on contact, so setting it back to 5 survives enemy contact and
    /// bullets. A lethal hit that bypasses `$46` (the `$C1D0` path) still sets
    /// `$0100 = 2`, `$4C = 120` and `$1B = $A0`, so the flag also restores
    /// those three bytes. This only suppresses the player death state; it
    /// changes no enemy, spawner or boss code. `screens` does the `$46` poke by
    /// hand for the same reason.
    pub keep_alive: bool,
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

impl Runner {
    pub fn new() -> Self {
        Self {
            framebuffer: vec![0; FRAMEBUFFER_LEN],
            assist_pulses: 0,
            lethal_hits: Vec::new(),
            shield: false,
            keep_alive: false,
        }
    }

    /// Advance one video frame.
    pub fn step(&mut self, emulator: &mut Emulator, assist: bool) {
        if !assist {
            emulator.run_frame();
            return;
        }
        for line in 0..262usize {
            if line < SCREEN_HEIGHT {
                emulator.bus.ppu.render_line(line, &mut self.framebuffer);
            }
            if line == 241 {
                emulator.bus.ppu.start_vblank();
                if emulator.bus.ppu.nmi_enabled() {
                    emulator.cpu.nmi(&mut emulator.bus);
                }
            }
            if line == 261 {
                emulator.bus.ppu.end_vblank();
                if emulator.bus.ppu.mask & 0x18 != 0 {
                    emulator.bus.ppu.reload_scroll();
                }
            }

            let budget = 113 + u64::from(line % 3 == 2);
            let target = emulator.cpu.cycles + budget;
            while emulator.cpu.cycles < target {
                let cycles = emulator.cpu.step(&mut emulator.bus);
                let stall = emulator.bus.take_stall();
                emulator.cpu.cycles += u64::from(stall);
                if cycles == 0 {
                    break;
                }
            }

            // Replace the missing sprite-0 hit exactly while the split loop
            // polls it: clear any stale flag at the first loop, then raise it
            // at the second so the handler resumes and finishes the frame.
            if WAIT_CLEAR.contains(&emulator.cpu.pc) {
                emulator.bus.ppu.status &= !0x40;
            } else if WAIT_SET.contains(&emulator.cpu.pc) {
                emulator.bus.ppu.status |= 0x40;
                self.assist_pulses += 1;
            }

            if emulator.cpu.jammed {
                return;
            }
        }
        emulator.frames += 1;
    }

    /// Set this frame's buttons and advance one frame.
    pub fn advance(&mut self, emulator: &mut Emulator, plan: &Plan, assist: bool) {
        emulator.set_buttons(plan.buttons(emulator.frames));
        self.step(emulator, assist);
        if self.keep_alive || self.shield {
            emulator.bus.ram[0x46] = 5;
        }
        if self.keep_alive {
            if let Some(hit) = lethal_hit(&emulator.bus.ram, emulator.frames) {
                self.lethal_hits.push(hit);
            }
            if emulator.bus.ram[0x0100] >= 2 {
                emulator.bus.ram[0x0100] = 1;
                emulator.bus.ram[0x4C] = 0;
            }
            if emulator.bus.ram[0x1B] == 0xA0 {
                emulator.bus.ram[0x1B] = 0x80;
            }
        }
    }

    /// Advance one frame and sample a RAM byte at the end of the visible
    /// scanlines, before the vblank NMI reads the controller and overwrites
    /// `$07`.
    ///
    /// This mirrors [`Emulator::run_frame`] without the optional assist. The
    /// attract demo writes `$07` from its own table while the stage handler
    /// runs, so an end-of-frame sample would only ever show the real pad byte
    /// (zero with no input).
    pub fn advance_sampled(&mut self, emulator: &mut Emulator, plan: &Plan, address: u16) -> u8 {
        use gradius_nes::SCREEN_HEIGHT;

        emulator.set_buttons(plan.buttons(emulator.frames));
        let mut sampled = emulator.bus.ram[address as usize];
        emulator.bus.apu.frame = emulator.frames;
        emulator.bus.ppu.frame = emulator.frames;
        for line in 0..262usize {
            emulator.bus.ppu.line = line;
            let hit_before = emulator.bus.ppu.sprite_zero_hit;
            if line < SCREEN_HEIGHT {
                emulator.bus.ppu.render_line(line, &mut self.framebuffer);
            }
            let deferred_hit = match (hit_before, emulator.bus.ppu.sprite_zero_hit) {
                (None, Some(hit)) if hit.line == line => Some(hit),
                _ => None,
            };
            if deferred_hit.is_some() {
                emulator.bus.ppu.status &= !0x40;
            }
            if line == 241 {
                emulator.bus.ppu.start_vblank();
                if emulator.bus.ppu.nmi_enabled() {
                    emulator.bus.apu.cycle = emulator.cpu.cycles;
                    emulator.cpu.nmi(&mut emulator.bus);
                    emulator.bus.apu.step(7);
                }
            }
            if line == 261 {
                emulator.bus.ppu.end_vblank();
                if emulator.bus.ppu.mask & 0x18 != 0 {
                    emulator.bus.ppu.reload_scroll();
                }
            }

            let budget = 113 + u64::from(line % 3 == 2);
            let target = emulator.cpu.cycles + budget;
            let mut hit_at = deferred_hit.map(|hit| emulator.cpu.cycles + u64::from(hit.dot) / 3);
            while emulator.cpu.cycles < target {
                if let Some(at) = hit_at {
                    if emulator.cpu.cycles >= at {
                        emulator.bus.ppu.status |= 0x40;
                        hit_at = None;
                    }
                }
                let before = emulator.cpu.cycles;
                emulator.bus.apu.cycle = before;
                let cycles = emulator.cpu.step(&mut emulator.bus);
                let delta = (emulator.cpu.cycles - before) as u32;
                emulator.bus.apu.step(delta);
                let stall = emulator.bus.take_stall();
                if stall > 0 {
                    emulator.bus.apu.step(stall);
                }
                emulator.cpu.cycles += u64::from(stall);
                if cycles == 0 {
                    break;
                }
            }
            if hit_at.is_some() {
                emulator.bus.ppu.status |= 0x40;
            }
            if line == 240 {
                sampled = emulator.bus.ram[address as usize];
            }
            if emulator.cpu.jammed {
                break;
            }
        }
        emulator.frames += 1;
        sampled
    }

    /// Run until `emulator.frames == frame`.
    pub fn run_to(&mut self, emulator: &mut Emulator, frame: u64, plan: &Plan, assist: bool) {
        while emulator.frames < frame {
            self.advance(emulator, plan, assist);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_nes::{BUTTON_A, BUTTON_LEFT, BUTTON_START};

    #[test]
    fn lethal_hit_reads_dying_state_and_position() {
        let mut ram = vec![0u8; 0x800];
        assert_eq!(lethal_hit(&ram, 900), None);

        ram[0x0100] = 2;
        ram[0x003E] = 0x8C;
        ram[0x003F] = 0x02;
        ram[0x0360] = 80;
        ram[0x0320] = 64;
        assert_eq!(
            lethal_hit(&ram, 3000),
            Some(LethalHit {
                frame: 3000,
                scroll: 0x028C,
                x: 80,
                y: 64,
            })
        );

        // A dying value above 2 (explosion phase) still counts as a hit.
        ram[0x0100] = 3;
        assert_eq!(lethal_hit(&ram, 3001).unwrap().frame, 3001);
    }

    #[test]
    fn plan_holds_buttons_for_their_window() {
        let plan = Plan {
            held: BUTTON_LEFT,
            schedule: vec![(10, BUTTON_START, 6), (20, BUTTON_A, 2)],
        };
        assert_eq!(plan.buttons(9), BUTTON_LEFT);
        assert_eq!(plan.buttons(10), BUTTON_LEFT | BUTTON_START);
        assert_eq!(plan.buttons(15), BUTTON_LEFT | BUTTON_START);
        assert_eq!(plan.buttons(16), BUTTON_LEFT);
        assert_eq!(plan.buttons(20), BUTTON_LEFT | BUTTON_A);
        assert_eq!(plan.buttons(21), BUTTON_LEFT | BUTTON_A);
        assert_eq!(plan.buttons(22), BUTTON_LEFT);
    }

    #[test]
    fn overlapping_presses_use_the_latest() {
        let plan = Plan {
            held: 0,
            schedule: vec![(10, BUTTON_START, 10), (12, BUTTON_A, 2)],
        };
        assert_eq!(plan.buttons(11), BUTTON_START);
        assert_eq!(plan.buttons(12), BUTTON_A);
        assert_eq!(plan.buttons(14), BUTTON_START);
        assert_eq!(plan.buttons(20), 0);
    }
}
