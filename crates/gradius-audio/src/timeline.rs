//! Register timelines and their player.
//!
//! A [`Timeline`] is a sorted list of APU register writes expressed as offsets
//! in CPU cycles from the start of the track. The player applies each write to
//! an [`Apu`] at exactly the right cycle, so captured music and sound effects
//! keep their original articulation and pitch.
//!
//! Data comes from the pack's audio chunk, captured from the development oracle
//! by `xtask extract`. Nothing here depends on the cartridge or on
//! SDL.

use crate::apu::Apu;

/// One APU register write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// Offset from the start of the track, in CPU cycles.
    pub at: u32,
    /// APU register in `$4000..=$4017`, stored as a byte offset from `$4000`.
    pub addr: u8,
    /// Value written.
    pub value: u8,
}

impl Event {
    pub const fn new(at: u32, addr: u16, value: u8) -> Self {
        Self {
            at,
            addr: (addr & 0xFF) as u8,
            value,
        }
    }

    /// Absolute register address.
    pub const fn register(self) -> u16 {
        0x4000 | self.addr as u16
    }
}

/// A captured, replayable stream of APU writes.
#[derive(Clone, Copy, Debug)]
pub struct Timeline {
    /// Human-readable track name, used in diagnostics.
    pub name: &'static str,
    /// Writes sorted by `at`.
    pub events: &'static [Event],
    /// Track length in CPU cycles.
    pub total_cycles: u32,
    /// When set, the track loops back to this offset instead of stopping.
    pub loop_point: Option<u32>,
    /// Index of the first event at or after `loop_point`.
    pub loop_index: u16,
}

impl Timeline {
    /// True once the track has been played through (only meaningful for
    /// one-shot tracks).
    pub const fn is_looping(&self) -> bool {
        self.loop_point.is_some()
    }

    /// Length in seconds at the NTSC CPU clock.
    pub fn seconds(&self) -> f32 {
        self.total_cycles as f32 / crate::apu::CPU_CLOCK_HZ as f32
    }
}

/// An empty one-shot track. Frontends play it in place of a track the pack
/// does not carry, so a pack without audio is silent.
pub static SILENCE: Timeline = Timeline {
    name: "silence",
    events: &[],
    total_cycles: 29_780,
    loop_point: None,
    loop_index: 0,
};

/// Find the first frame at which a captured stream starts repeating itself.
///
/// Captured music usually opens with a lead-in (intro flourish, first phrase)
/// before the driver settles into its loop. `events` are the raw writes in CPU
/// cycles, `frame_cycles` is the NTSC frame length and `period_frames` the
/// musical period. The returned frame is the first offset whose next two
/// periods match register-for-register (one frame of tolerance, so a write may
/// straddle a frame boundary).
///
/// Used by the `capture-audio` tool to place `loop_point`; the runtime player
/// only needs the resulting offset.
pub fn find_loop_start(
    events: &[Event],
    frame_cycles: u32,
    period_frames: u32,
    max_start_frames: u32,
) -> Option<u32> {
    if period_frames == 0 || frame_cycles == 0 {
        return None;
    }
    let last_frame = events
        .last()
        .map(|event| event.at / frame_cycles)
        .unwrap_or(0);
    let span = 3 * period_frames;
    // A start needs three periods of data after it.
    let limit = last_frame.checked_sub(span - 1)?;
    let limit = max_start_frames.min(limit);
    let mut frames: Vec<Vec<(u8, u8)>> = vec![Vec::new(); (limit + span) as usize + 2];
    for event in events {
        let frame = event.at / frame_cycles;
        if frame < frames.len() as u32 {
            frames[frame as usize].push((event.addr, event.value));
        }
    }
    for frame in &mut frames {
        frame.sort_unstable();
    }
    let matches = |left: usize, right: usize| -> bool {
        let mut pool: Vec<(u8, u8)> = Vec::new();
        for shift in [-1i32, 0, 1] {
            let index = right as i32 + shift;
            if index >= 0 && (index as usize) < frames.len() {
                pool.extend_from_slice(&frames[index as usize]);
            }
        }
        // Every write in the first pass must appear in the next pass, possibly
        // one frame early or late. Extra writes in the next pass (an effect
        // that only happens once) do not disqualify the loop by themselves.
        frames[left].iter().all(|event| {
            pool.iter()
                .position(|candidate| candidate == event)
                .map(|index| {
                    pool.swap_remove(index);
                    true
                })
                .unwrap_or(false)
        })
    };
    for start in 0..=limit {
        let mut ok = true;
        'periods: for repeat in 0..2 {
            let first = start + repeat * period_frames;
            for offset in 0..period_frames {
                if !matches(
                    (first + offset) as usize,
                    (first + period_frames + offset) as usize,
                ) {
                    ok = false;
                    break 'periods;
                }
            }
        }
        if ok {
            return Some(start);
        }
    }
    None
}

/// Playback cursor for one [`Timeline`] feeding one [`Apu`].
#[derive(Clone, Debug)]
pub struct Player {
    timeline: &'static Timeline,
    clock: u32,
    index: usize,
    finished: bool,
    /// Supply bit 7 on `$4008` writes for captured tracks that never set it.
    reload_triangle: bool,
}

impl Player {
    pub fn new(timeline: &'static Timeline) -> Self {
        Self {
            timeline,
            clock: 0,
            index: 0,
            finished: false,
            reload_triangle: false,
        }
    }

    /// Reload the triangle linear counter on every `$4008` write.
    ///
    /// The cartridge's APU keeps running across cue changes, so a track whose
    /// captured stream leaves the reload bit clear would still inherit a
    /// loaded linear counter. A fresh runtime APU does not: without the reload
    /// bit the triangle is silent. The reload is supplied for tracks where
    /// most `$4008` writes leave bit 7 clear; a track (like stage 1) that sets
    /// it on most notes keeps its exact articulation.
    pub fn with_triangle_reload(mut self) -> Self {
        let mut writes = 0usize;
        let mut reloads = 0usize;
        for event in self.timeline.events {
            if event.addr == 0x08 {
                writes += 1;
                if event.value & 0x80 != 0 {
                    reloads += 1;
                }
            }
        }
        self.reload_triangle = writes == 0 || reloads * 2 < writes;
        self
    }

    pub fn timeline(&self) -> &'static Timeline {
        self.timeline
    }

    /// Position within the track, in CPU cycles.
    pub fn position(&self) -> u32 {
        self.clock
    }

    /// True for a one-shot track that reached its end.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Advance `cycles` CPU cycles, writing due registers into `apu`.
    pub fn advance(&mut self, apu: &mut Apu, mut cycles: u32) {
        while cycles > 0 && !self.finished {
            if let Some(event) = self.timeline.events.get(self.index) {
                if event.at <= self.clock {
                    let value = if self.reload_triangle && event.addr == 0x08 {
                        event.value | 0x80
                    } else {
                        event.value
                    };
                    apu.write(event.register(), value);
                    self.index += 1;
                    continue;
                }
                let step = (event.at - self.clock).min(cycles);
                self.clock += step;
                cycles -= step;
            } else if self.clock < self.timeline.total_cycles {
                let step = (self.timeline.total_cycles - self.clock).min(cycles);
                self.clock += step;
                cycles -= step;
                if self.clock >= self.timeline.total_cycles {
                    self.wrap_or_finish();
                }
            } else {
                self.wrap_or_finish();
                if !self.finished {
                    continue;
                }
            }
        }
    }

    fn wrap_or_finish(&mut self) {
        match self.timeline.loop_point {
            Some(point) => {
                // Preserve the overshoot so a big render chunk cannot drift.
                let overshoot = self.clock.saturating_sub(self.timeline.total_cycles);
                self.clock = point.saturating_add(overshoot);
                self.index = usize::from(self.timeline.loop_index);
            }
            None => {
                self.clock = self.timeline.total_cycles;
                self.index = self.timeline.events.len();
                self.finished = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apu::Apu;

    const EVENTS: [Event; 3] = [
        Event::new(0, 0x4015, 0x01),
        Event::new(100, 0x4000, 0xBF),
        Event::new(200, 0x4002, 0x40),
    ];
    const LOOPING: Timeline = Timeline {
        name: "test",
        events: &EVENTS,
        total_cycles: 300,
        loop_point: Some(0),
        loop_index: 0,
    };
    const ONESHOT: Timeline = Timeline {
        name: "oneshot",
        events: &EVENTS,
        total_cycles: 300,
        loop_point: None,
        loop_index: 0,
    };

    #[test]
    fn writes_land_on_their_cycle() {
        let mut apu = Apu::new();
        let mut player = Player::new(&ONESHOT);
        player.advance(&mut apu, 99);
        assert!(apu.pulse1.enabled);
        assert!(!apu.pulse1.constant_volume);
        player.advance(&mut apu, 2);
        assert!(apu.pulse1.constant_volume);
        player.advance(&mut apu, 100);
        assert_eq!(apu.pulse1.timer_period, 0x40);
    }

    #[test]
    fn one_shot_finishes_and_stays_finished() {
        let mut apu = Apu::new();
        let mut player = Player::new(&ONESHOT);
        player.advance(&mut apu, 1_000);
        assert!(player.is_finished());
        assert_eq!(player.position(), 300);
        player.advance(&mut apu, 100);
        assert!(player.is_finished());
    }

    #[test]
    fn looping_track_wraps_at_its_length() {
        let mut apu = Apu::new();
        let mut player = Player::new(&LOOPING);
        player.advance(&mut apu, 299);
        assert!(!player.is_finished());
        assert_eq!(player.index, 3);
        player.advance(&mut apu, 1);
        assert!(!player.is_finished());
        assert_eq!(player.position(), 0);
        // The initial status write is due again on the new pass.
        player.advance(&mut apu, 1);
        assert_eq!(player.position(), 1);
        assert_eq!(player.index, 1);
    }

    #[test]
    fn loop_point_can_be_later_in_the_track() {
        const LATE: Timeline = Timeline {
            name: "late",
            events: &EVENTS,
            total_cycles: 300,
            loop_point: Some(100),
            loop_index: 1,
        };
        let mut apu = Apu::new();
        let mut player = Player::new(&LATE);
        player.advance(&mut apu, 300);
        assert_eq!(player.position(), 100);
        assert_eq!(player.index, 1);
    }

    #[test]
    fn event_register_uses_the_captured_offset() {
        assert_eq!(Event::new(0, 0x4008, 0).register(), 0x4008);
        assert_eq!(Event::new(0, 0x4015, 0).register(), 0x4015);
    }
}
