//! Development-only capture of the Konami sound driver's APU register stream.
//!
//! Runs the original cartridge in the `gradius-nes` oracle and returns the
//! music and sound-effect timelines that `xtask extract` writes into the
//! pack's audio chunk. The cartridge is only touched here; the shipped game
//! reads the chunk.
//!
//! Two capture paths are used:
//!
//! * the title fanfare is sliced straight out of a real run, because it is one
//!   short request and needs no isolation;
//! * music loops and effects are injected through `JSR $EC18` into an otherwise
//!   quiet driver and run once per frame, exactly as the NMI does. A real
//!   assisted game run proves every request (`$93`, `$96`, `$A5`, `$AC`,
//!   `$AF`) exists in the cartridge.
//!
//! Every write keeps the CPU-cycle offset at which the driver would have made
//! it, so the runtime player reproduces the original articulation.

use std::collections::BTreeMap;

use gradius_cart::Cartridge;
use gradius_nes::ppu::{FRAMEBUFFER_LEN, HEIGHT};
use gradius_nes::{ApuWrite, Emulator, BUTTON_A, BUTTON_B, BUTTON_START};

use crate::data::{Music, Sfx};
use crate::timeline::{Event, Timeline};

/// The captured timelines, in [`Music::ALL`] and [`Sfx::ALL`] order.
pub struct AudioCapture {
    pub music: Vec<Timeline>,
    pub sfx: Vec<Timeline>,
}

impl AudioCapture {
    /// Total register writes over every track.
    pub fn events(&self) -> usize {
        self.music
            .iter()
            .chain(&self.sfx)
            .map(|timeline| timeline.events.len())
            .sum()
    }
}

/// Capture every track from the cartridge.
///
/// The timelines are leaked (`Timeline` borrows its events for `'static`);
/// call it once per process.
pub fn capture_all(cart: &Cartridge) -> Result<AudioCapture, String> {
    let mut music = capture_music(cart)?;
    let mut effects = capture_effects(cart)?;
    let take = |map: &mut BTreeMap<&'static str, Captured>, name: &'static str| {
        map.remove(name)
            .map(|captured| captured.into_timeline(name))
            .ok_or_else(|| format!("missing {name}"))
    };
    Ok(AudioCapture {
        music: Music::ALL
            .iter()
            .map(|kind| take(&mut music, kind.name()))
            .collect::<Result<_, _>>()?,
        sfx: Sfx::ALL
            .iter()
            .map(|kind| take(&mut effects, kind.name()))
            .collect::<Result<_, _>>()?,
    })
}

const DRIVER: u16 = 0xECFC;
const TRIGGER: u16 = 0xEC18;
const FRAME_CYCLES: u32 = 29_780;

// Sound requests traced in the driver's `$EFC7` table and confirmed by a
// long assisted run (see `docs/AUDIO.md`).
const TITLE_ID: u8 = 0x90;
const STAGE1_ID: u8 = 0x93;
const STAGE1B_ID: u8 = 0x96;
const BOSS_ID: u8 = 0xA5;
const CLEAR_ID: u8 = 0xAC;
const GAMEOVER_ID: u8 = 0xAF;

// The six per-stage second themes (`$8344[$19]`). Stages 2-7 are captured by
// injecting the request into a quiet driver; the real-game windows for them
// arrive tens of thousands of frames into a run, beyond the stage-1 capture
// above. Stage 4's `$5D` is never requested in the assisted run because the
// stage clears inside one sample, so it is injection-only (marked in the docs).
// Format: (track key / timeline name, request id, loop period).
const STAGE_THEME_TRACKS: [(&str, u8, u32); 6] = [
    ("stage2", 0x59, 1_344),
    ("stage3", 0x5B, 1_228),
    ("stage4", 0x5D, 1_248),
    ("stage5", 0x5F, 1_080),
    ("stage6", 0x61, 2_040),
    ("stage7", 0x63, 1_423),
];

// Musical periods measured from the injected write stream. Each track keeps
// its lead-in, so the runtime plays the intro once and then loops the period.
const STAGE1_PERIOD: u32 = 1_024;
const STAGE1B_PERIOD: u32 = 1_248;
const BOSS_PERIOD: u32 = 384;
const MAX_LEADIN: u32 = 700;
struct Captured {
    events: Vec<(u32, u8, u8)>,
    total: u32,
    loop_point: Option<u32>,
}

impl Captured {
    fn into_timeline(self, name: &'static str) -> Timeline {
        let loop_index = self.loop_point.map_or(0, |point| {
            self.events
                .iter()
                .position(|event| event.0 >= point)
                .unwrap_or(0)
        });
        let events: Vec<Event> = to_events(&self.events);
        Timeline {
            name,
            events: Box::leak(events.into_boxed_slice()),
            total_cycles: self.total,
            loop_point: self.loop_point,
            loop_index: loop_index as u16,
        }
    }
}

/// Per-frame recording of an ordinary game run.
struct Run {
    writes: Vec<ApuWrite>,
    /// Sound requests, as `(frame, id, return address)`.
    triggers: Vec<(u64, u8, u16)>,
    cycles_at_frame: BTreeMap<u64, u64>,
}

fn run_game(cart: &Cartridge, frames: u64, keep_alive: bool) -> Result<Run, String> {
    let mut emu = Emulator::new(cart).map_err(|error| error.to_string())?;
    emu.set_capture_apu(true);
    let mut run = Run {
        writes: Vec::new(),
        triggers: Vec::new(),
        cycles_at_frame: BTreeMap::new(),
    };
    for frame in 0..frames {
        let mut buttons = 0u8;
        if (120..126).contains(&frame) || (180..186).contains(&frame) {
            buttons |= BUTTON_START;
        }
        if frame >= 180 {
            buttons |= BUTTON_A | BUTTON_B;
            buttons |= if (frame / 40) % 2 == 0 { 0x10 } else { 0x20 };
        }
        emu.set_buttons(buttons);
        run.cycles_at_frame.insert(frame, emu.cpu.cycles);
        step_frame(&mut emu, frame, Some(&mut run.triggers));
        if keep_alive {
            // Analysis-only invulnerability: let the run reach the Big Core
            // so the boss and stage-clear requests can be recorded.
            emu.bus.ram[0x46] = 5;
            if emu.bus.ram[0x0100] >= 2 {
                emu.bus.ram[0x0100] = 1;
                emu.bus.ram[0x4C] = 0;
            }
            if emu.bus.ram[0x1B] == 0xA0 {
                emu.bus.ram[0x1B] = 0x80;
            }
        }
        run.writes.extend(emu.apu_writes().iter().copied());
        emu.bus.apu.clear_writes();
    }
    Ok(run)
}

/// The oracle's frame loop, copied so the tool can see the sound request PC.
fn step_frame(emu: &mut Emulator, frame: u64, mut triggers: Option<&mut Vec<(u64, u8, u16)>>) {
    emu.bus.apu.frame = frame;
    emu.bus.ppu.frame = frame;
    let mut framebuffer = vec![0u8; FRAMEBUFFER_LEN];
    for line in 0..262usize {
        emu.bus.ppu.line = line;
        if line < HEIGHT {
            emu.bus.ppu.render_line(line, &mut framebuffer);
        }
        if line == 241 {
            emu.bus.ppu.start_vblank();
            if emu.bus.ppu.nmi_enabled() {
                emu.bus.apu.cycle = emu.cpu.cycles;
                emu.cpu.nmi(&mut emu.bus);
                emu.bus.apu.step(7);
            }
        }
        if line == 261 {
            emu.bus.ppu.end_vblank();
            if emu.bus.ppu.mask & 0x18 != 0 {
                emu.bus.ppu.reload_scroll();
            }
        }
        let budget = 113 + u64::from(line % 3 == 2);
        let target = emu.cpu.cycles + budget;
        while emu.cpu.cycles < target {
            let before = emu.cpu.cycles;
            let pc = emu.cpu.pc;
            if let Some(triggers) = triggers.as_deref_mut() {
                if pc == TRIGGER {
                    let sp = usize::from(emu.cpu.sp);
                    let lo = u16::from(emu.bus.ram[0x100 + ((sp + 1) & 0xFF)]);
                    let hi = u16::from(emu.bus.ram[0x100 + ((sp + 2) & 0xFF)]);
                    triggers.push((frame, emu.cpu.a, (lo | (hi << 8)).wrapping_sub(1)));
                }
            }
            emu.bus.apu.cycle = before;
            let cycles = emu.cpu.step(&mut emu.bus);
            let delta = (emu.cpu.cycles - before) as u32;
            emu.bus.apu.step(delta);
            let stall = emu.bus.take_stall();
            if stall > 0 {
                emu.bus.apu.step(stall);
            }
            emu.cpu.cycles += u64::from(stall);
            if cycles == 0 {
                break;
            }
        }
        if emu.cpu.jammed {
            break;
        }
    }
    emu.frames = frame + 1;
}

fn capture_music(cart: &Cartridge) -> Result<BTreeMap<&'static str, Captured>, String> {
    // The assisted run reaches the Big Core and its destruction, so every
    // request the stage-1 music path uses appears in `triggers`.
    let run = run_game(cart, 12_000, true)?;
    for id in [TITLE_ID, STAGE1_ID, STAGE1B_ID, BOSS_ID, CLEAR_ID] {
        if first_trigger(&run, id).is_none() {
            return Err(format!("sound ${id:02X} never played in 12000 frames"));
        }
    }

    let mut music = BTreeMap::new();

    // Title/attract fanfare: the original plays it once and then stays quiet.
    music.insert("intro", slice_from_run(&run, TITLE_ID, 60)?);
    music.insert("stage1", capture_loop(cart, STAGE1_ID, STAGE1_PERIOD)?);
    music.insert("stage1_b", capture_loop(cart, STAGE1B_ID, STAGE1B_PERIOD)?);
    music.insert("boss", capture_loop(cart, BOSS_ID, BOSS_PERIOD)?);

    // One-shots: the fanfare after the Big Core and the game-over theme. The
    // game-over request only fires when the last life is lost, so a second,
    // ordinary run is needed.
    music.insert("stage_clear", capture_one_shot(cart, CLEAR_ID, 400)?);
    let ordinary = run_game(cart, 6_000, false)?;
    if first_trigger(&ordinary, GAMEOVER_ID).is_none() {
        return Err(format!(
            "sound ${GAMEOVER_ID:02X} never played before frame 6000"
        ));
    }
    music.insert("game_over_music", capture_one_shot(cart, GAMEOVER_ID, 600)?);

    // The six per-stage second themes. Every one is a looping BGM; the driver
    // clock aligns whole frames, so each period is an integer frame count
    // measured from a long quiet-driver injection.
    for &(key, id, period) in &STAGE_THEME_TRACKS {
        music.insert(key, capture_loop(cart, id, period)?);
    }
    Ok(music)
}

fn first_trigger(run: &Run, id: u8) -> Option<u64> {
    run.triggers
        .iter()
        .find(|(_, trigger, _)| *trigger == id)
        .map(|(frame, _, _)| *frame)
}

/// Slice one fixed-length window from a real run, starting at the request
/// frame. Used for the title fanfare, which is too short to need injection.
fn slice_from_run(run: &Run, id: u8, frames: u32) -> Result<Captured, String> {
    let frame = first_trigger(run, id).ok_or_else(|| format!("sound ${id:02X} never played"))?;
    let start = *run
        .cycles_at_frame
        .get(&frame)
        .ok_or("missing frame clock")?;
    let end = start + u64::from(frames * FRAME_CYCLES);
    let mut events = Vec::new();
    for write in &run.writes {
        if write.cycle < start || write.cycle >= end {
            continue;
        }
        if !(0x4000..=0x4017).contains(&write.addr) {
            continue;
        }
        events.push((
            (write.cycle - start) as u32,
            (write.addr & 0xFF) as u8,
            write.value,
        ));
    }
    events.sort_by_key(|event| event.0);
    let events = with_init(events);
    Ok(Captured {
        events,
        total: frames * FRAME_CYCLES,
        loop_point: None,
    })
}

/// Capture a looping music request by injection and cut it at the first frame
/// whose write stream repeats.
fn capture_loop(cart: &Cartridge, id: u8, period: u32) -> Result<Captured, String> {
    let frames = MAX_LEADIN + 3 * period + 64;
    let first = inject(cart, id, frames)?;

    let start = crate::timeline::find_loop_start(
        &to_events(&first.events),
        FRAME_CYCLES,
        period,
        MAX_LEADIN,
    )
    .ok_or_else(|| format!("sound ${id:02X} does not loop every {period} frames"))?;

    let loop_cycles = (start + period) * FRAME_CYCLES;
    let mut events: Vec<_> = first
        .events
        .iter()
        .copied()
        .filter(|event| event.0 < loop_cycles)
        .collect();
    events.sort_by_key(|event| event.0);
    let (loop_bad, _) = frame_mismatches(&first.events, period, start, 2);
    if loop_bad > 0 {
        return Err(format!("sound ${id:02X} loop is not clean"));
    }

    Ok(Captured {
        events: with_init(events),
        total: loop_cycles,
        loop_point: Some(start * FRAME_CYCLES),
    })
}

/// Run one sound request in isolation and capture the driver's register stream
/// until it has been quiet for 20 frames.
fn capture_one_shot(cart: &Cartridge, id: u8, max_frames: u32) -> Result<Captured, String> {
    let mut emu = quiet_emulator(cart)?;
    call(&mut emu, TRIGGER, id)?;
    emu.bus.apu.clear_writes();

    let mut events = Vec::new();
    let mut frame_cycle = 0u32;
    let mut quiet_frames = 0;
    let mut wrote = false;
    for _ in 0..max_frames {
        let start = emu.cpu.cycles;
        call(&mut emu, DRIVER, 0)?;
        let end = emu.cpu.cycles;
        let before = emu.apu_writes().len();
        for write in emu.apu_writes() {
            if (0x4000..=0x4017).contains(&write.addr) {
                let at = frame_cycle + (write.cycle.saturating_sub(start)) as u32;
                events.push((at, (write.addr & 0xFF) as u8, write.value));
                wrote = true;
            }
        }
        emu.bus.apu.clear_writes();
        let spent = (end - start) as u32;
        if spent < FRAME_CYCLES {
            emu.bus.apu.step(FRAME_CYCLES - spent);
        }
        frame_cycle += FRAME_CYCLES;
        if before == 0 {
            if wrote {
                quiet_frames += 1;
                if quiet_frames >= 20 {
                    break;
                }
            }
        } else {
            quiet_frames = 0;
        }
    }
    if events.is_empty() {
        return Err(format!("sound ${id:02X} produced no APU writes"));
    }
    Ok(Captured {
        events: with_init(events),
        total: frame_cycle,
        loop_point: None,
    })
}

/// Run one request in isolation for a fixed number of frames without trimming.
fn inject(cart: &Cartridge, id: u8, frames: u32) -> Result<Captured, String> {
    let mut emu = quiet_emulator(cart)?;
    call(&mut emu, TRIGGER, id)?;
    emu.bus.apu.clear_writes();

    let mut events = Vec::new();
    let mut frame_cycle = 0u32;
    for _ in 0..frames {
        let start = emu.cpu.cycles;
        call(&mut emu, DRIVER, 0)?;
        let end = emu.cpu.cycles;
        for write in emu.apu_writes() {
            if (0x4000..=0x4017).contains(&write.addr) {
                let at = frame_cycle + (write.cycle.saturating_sub(start)) as u32;
                events.push((at, (write.addr & 0xFF) as u8, write.value));
            }
        }
        emu.bus.apu.clear_writes();
        let spent = (end - start) as u32;
        if spent < FRAME_CYCLES {
            emu.bus.apu.step(FRAME_CYCLES - spent);
        }
        frame_cycle += FRAME_CYCLES;
    }
    Ok(Captured {
        events,
        total: frame_cycle,
        loop_point: None,
    })
}

/// A clean driver: only the sound engine runs, no game code and no effects.
fn quiet_emulator(cart: &Cartridge) -> Result<Emulator, String> {
    let mut emu = Emulator::new(cart).map_err(|error| error.to_string())?;
    emu.run_frames(4);
    for base in [0xB0usize, 0xC1, 0xD2, 0xE3] {
        for offset in 0..17 {
            emu.bus.ram[base + offset] = 0;
        }
    }
    for addr in [
        0xF0usize, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF8, 0xF9, 0xFA, 0xFB, 0xDD, 0xDE,
    ] {
        emu.bus.ram[addr] = 0;
    }
    emu.bus.ram[0x15] = 0;
    emu.bus.ram[0xC3] = 0x10;
    emu.bus.apu = gradius_nes::Apu::new();
    emu.bus.apu.write(0x4015, 0x00);
    for addr in 0x4000..=0x400F {
        emu.bus.apu.write(addr, 0x30);
    }
    emu.bus.apu.write(0x4015, 0x0F);
    emu.bus.apu.write(0x4017, 0xC0);
    emu.set_capture_apu(true);
    emu.bus.apu.clear_writes();
    Ok(emu)
}

fn to_events(events: &[(u32, u8, u8)]) -> Vec<Event> {
    events
        .iter()
        .map(|(at, addr, value)| Event::new(*at, 0x4000u16 | u16::from(*addr), *value))
        .collect()
}

/// Count register-value mismatches between two consecutive periods, allowing a
/// write to straddle a frame boundary.
fn frame_mismatches(
    events: &[(u32, u8, u8)],
    period: u32,
    start: u32,
    repeats: u32,
) -> (usize, usize) {
    let window = (start + (repeats + 1) * period + 2) as usize;
    let mut frames: Vec<Vec<(u8, u8)>> = vec![Vec::new(); window];
    for (at, addr, value) in events {
        let frame = (at / FRAME_CYCLES) as usize;
        if frame < frames.len() {
            frames[frame].push((*addr, *value));
        }
    }
    let mut bad = 0;
    let mut total = 0;
    for repeat in 0..repeats {
        for offset in 0..period {
            let frame = (start + repeat * period + offset) as usize;
            let next = frame + period as usize;
            let left = &frames[frame];
            total += left.len();
            let mut right: Vec<(u8, u8)> = frames[next].clone();
            right.extend_from_slice(&frames[next - 1]);
            right.extend_from_slice(&frames[next + 1]);
            for event in left {
                if !right.contains(event) {
                    bad += 1;
                }
            }
        }
    }
    (bad, total)
}

fn capture_effects(cart: &Cartridge) -> Result<BTreeMap<&'static str, Captured>, String> {
    const EFFECTS: [(&str, u8); 12] = [
        ("shot", 0x01),
        ("enemy_shot", 0x06),
        ("enemy_hit", 0x05),
        ("explosion", 0x09),
        ("power_up", 0x0D),
        ("capsule", 0x0E),
        ("player_death", 0x36),
        ("game_over_sfx", 0x35),
        ("block_break", 0x03),
        ("block_break_stage6", 0x04),
        ("pause", 0x3B),
        ("screen_clear", 0x0B),
    ];
    let mut out = BTreeMap::new();
    for (name, id) in EFFECTS {
        out.insert(name, capture_one_shot(cart, id, 600)?);
    }
    Ok(out)
}

/// Inject a `JSR` to `addr` and run until it returns.
fn call(emu: &mut Emulator, addr: u16, a: u8) -> Result<(), String> {
    let ret = 0x0300u16;
    let sp = emu.cpu.sp as usize;
    emu.bus.ram[0x100 + sp] = ((ret - 1) >> 8) as u8;
    emu.bus.ram[0x100 + (sp.wrapping_sub(1) & 0xFF)] = ((ret - 1) & 0xFF) as u8;
    emu.cpu.sp = emu.cpu.sp.wrapping_sub(2);
    emu.cpu.pc = addr;
    emu.cpu.a = a;
    let mut steps = 0;
    while emu.cpu.pc != ret {
        let before = emu.cpu.cycles;
        emu.bus.apu.cycle = before;
        let cycles = emu.cpu.step(&mut emu.bus);
        let delta = (emu.cpu.cycles - before) as u32;
        emu.bus.apu.step(delta);
        steps += 1;
        if cycles == 0 || steps > 200_000 {
            return Err(format!("injected call to ${addr:04X} did not return"));
        }
    }
    Ok(())
}

/// The cartridge sets up the APU once at boot: enable all four channels and
/// select the 5-step frame sequence (`$4017 = $C0`). A runtime track starts
/// from a power-on APU, so it must repeat those two writes.
fn with_init(events: Vec<(u32, u8, u8)>) -> Vec<(u32, u8, u8)> {
    let mut out = Vec::with_capacity(events.len() + 2);
    out.push((0, 0x15, 0x0F));
    out.push((0, 0x17, 0xC0));
    out.extend(events);
    out
}
