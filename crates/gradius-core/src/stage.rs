//! Stage-1 state machine, scroll counter and the table-driven spawner.
//!
//! Every constant and branch here mirrors a routine measured in the original
//! `Gradius (E).nes`; the addresses are quoted next to the code. The data
//! itself lives in [`crate::rom::spawns`], extracted from the ROM.
//!
//! * Scroll (`$98E1`): `$3D` accumulates `$80` per frame, the carry is added to
//!   the 16-bit position at `$3E`/`$3F`. `$3F` bit 0 selects the nametable
//!   (`$9A70`).
//! * Spawner (`$A2BA`/`$A32F`): a section is eight 16-pixel column streams; the
//!   active column is `$3F & $0E` (one entry every 512 px of scroll).
//! * Section threshold (`$9A40`): when `$3F` reaches `L9A30[$19]` the stage
//!   state becomes `$81` (`L9A38`), which advances to the next section.
//!
//! ## Deliberate approximations
//!
//! `TRANSITION_FRAMES` is derived from `$9A28[$19]`: the four `$1B` states
//! `$81` (1 frame), `$82` (`$9A28` frames), `$83` and `$84` (1 frame each).
//! Stage 1 therefore spends six frames in the transition, which matches the six
//! `$0D` blank frames of the stage init. The VRAM uploads inside `$9C1E` have no
//! meaning in a software renderer.

use crate::rom::spawns::{Record, Spawns, COLUMN_SLOTS};
use crate::stage1_tables as tables;

/// Playable sections (`$19` never reaches 7; see the module docs of the
/// generated tables).
pub const SECTION_COUNT: usize = 7;

/// Frames spent on a section transition for stage 1 (`$19 = 0`).
///
/// The original runs the transition as four states: `$1B = $81` for one frame,
/// `$1B = $82` for `$9A28[$19]` frames (the `$4C/$4D` countdown at `$99DC`),
/// then `$1B = $83` and `$1B = $84` for one frame each. Stage 1's
/// `$9A28[0] = 3`, so the whole transition is six frames; the `$9C1E` background
/// passes and the `$0D = 5` blank happen inside it.
pub const TRANSITION_FRAMES: u16 = 6;

/// `$9A28[$19] + 3`: `$81` (1) + `$82` (`$9A28`) + `$83` (1) + `$84` (1).
pub fn transition_frames(section: u8) -> u16 {
    let index = usize::from(section).min(tables::CHECKPOINT_TIMER.len() - 1);
    u16::from(tables::CHECKPOINT_TIMER[index]) + 3
}

/// A 16.8 fixed-point scroll position (`$3D`, `$3E`, `$3F`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scroll {
    /// `$3D`, sub-pixel accumulator.
    pub fraction: u8,
    /// `$3E`, pixel low byte.
    pub low: u8,
    /// `$3F`, pixel high byte and nametable bit.
    pub high: u8,
}

impl Scroll {
    /// `$98E1`: `$3D += $80`, carry into `$3E`, carry into `$3F`.
    pub fn step(&mut self) {
        let sum = u16::from(self.fraction) + u16::from(tables::SCROLL_STEP);
        self.fraction = sum as u8;
        let carry = (sum >> 8) as u8;
        let sum = u16::from(self.low) + u16::from(carry);
        self.low = sum as u8;
        self.high = self.high.wrapping_add((sum >> 8) as u8);
    }

    /// `$3F:$3E` as one 16-bit position, used by the spawner comparison.
    pub fn position(self) -> u16 {
        (u16::from(self.high) << 8) | u16::from(self.low)
    }

    /// `$9A70`: PPUCTRL bit 0 is `$3F` bit 0.
    pub fn nametable(self) -> u8 {
        self.high & 1
    }
}

/// Stage sub-state (`$1B`). Only the parts that change simulation behaviour
/// are modelled; the PPU uploads collapse into the frame counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// `$00 = 1`: title → demo transition. The original leaves title state 0
    /// at frame 133 and runs a 256-frame countdown before the demo.
    Attract(u16),
    /// `$00 = 2` first frame (`$8119` with `$01 == 0`): the ROM jumps to
    /// `$82BF`, which clears the demo's RAM and object pages on the next frame.
    DemoStart,
    /// `$00 = 3`: the continue countdown the demo enters on a Start press.
    /// It runs for 81 frames (`$07E4` digits stay zero) before the stage.
    Continue(u16),
    /// `$00 = 4`: the single enter-stage frame (`$815D`).
    StageInit,
    /// `$00 = 5` with `$1B` 0..4: the stage handler's own setup. The dump
    /// reports `Playing`; gameplay starts when the counter reaches zero.
    StageIntro(u16),
    /// `$9B38`: `$0D = 6`, the screen stays blanked for six frames.
    Blank(u8),
    /// `$1B` 1..4: HUD, palette and power-meter setup, one frame each.
    Init(u8),
    /// `$1B = $80`, normal play (`$9A40`).
    Normal,
    /// `$1B = $81`: the section threshold fired, the next section is prepared.
    Transition(u16),
    /// `$1B = $C0`: player death countdown (`$4C = 120`).
    Dying(u16),
    /// No original state: the Big Core died, the stage is over.
    Cleared(u16),
}

/// One object emitted by the spawner. The game allocates the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnEvent {
    /// `$030C`: enemy type or terrain type.
    pub kind: u8,
    /// `$010C`: subtype / tile base.
    pub subtype: u8,
    /// `$03BC`: sub-state.
    pub substate: u8,
    /// `$03EC`: formation parameter.
    pub flag: u8,
    /// `$036C`: X position in pixels.
    pub x: u8,
    /// `$032C`: Y position in pixels.
    pub y: u8,
    /// `$040C`/`$04EC` movement flags (`$A573`).
    pub motion: u8,
    /// `$03AC`: the movement parity from `$A573`, or the `$49` cannon index
    /// when `$A3E9` reads a class of four or more.
    pub phase: u8,
    /// True for a 3-byte `$A65C` terrain definition.
    pub terrain: bool,
}

/// One thing the spawner produced this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emission {
    Spawn(SpawnEvent),
    /// Five-byte command (`$A374`); bytes are `$63-$67` after `$64 -= $70`.
    Command([u8; 5]),
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    /// `$69`: attempts left, decremented before each allocation try.
    remaining: u8,
    /// `$A5B6` interval reloaded on a successful spawn.
    interval: u8,
    /// `$6E`: Y accumulator, advanced by `y_step` only when a member spawns.
    y: u8,
    y_step: u8,
    kind: u8,
    subtype: u8,
    substate: u8,
    x: u8,
    motion: u8,
    phase: u8,
    /// The current member was handed to the game and awaits the slot result.
    attempted: bool,
}

/// `$A2BA` spawner state.
#[derive(Clone, Copy, Debug)]
pub struct Spawner {
    spawns: &'static Spawns,
    /// `$19`: current section row of `spawns.sections`.
    section: u8,
    /// `$61`: base of the current 512-px column window.
    column_base: u8,
    /// Index into the current stream (`$6A/$6B` advanced by `$8407`).
    cursor: usize,
    stream: &'static [Record],
    pending: Option<Pending>,
    /// `$6C`: frames until the next member attempt. It is plain RAM: only a
    /// successful allocation reloads it, so a new formation that fails its
    /// first try still waits out the previous formation's interval.
    timer: u8,
    /// `$5D`: number of records consumed.
    pub spawn_count: u8,
    /// `$49`: alternating cannon index handed to `$03AC`.
    cannon: u8,
    /// `$0048`: live members per cannon class, set to the class size by
    /// `$A3F7` and decremented whenever `$BE9F` destroys a member.
    class_counts: [u8; 4],
    /// `$60 = 1`: the first `$A2BA` call only re-reads `$61`/`$6A`.
    init_pending: bool,
    /// `$60 = 0`: `$C1D0` disabled the spawner when a hit landed on the frame
    /// the `$9A30` threshold armed `$81`; `$A2C1` then returns at once and
    /// only a stage change re-enables it.
    disabled: bool,
    /// `$A32F` ran on this `step`: a record was read off the stream. Only
    /// this raises `$5D`; a waiting formation member (`$A325`) does not.
    pub record_read: bool,
}

/// Save-state view of [`Pending`], for callers outside this module.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PendingState {
    /// `$69`.
    pub remaining: u8,
    pub interval: u8,
    /// `$6E`.
    pub y: u8,
    pub y_step: u8,
    pub kind: u8,
    pub subtype: u8,
    pub substate: u8,
    pub x: u8,
    pub motion: u8,
    pub phase: u8,
    pub attempted: bool,
}

/// Save-state view of the spawner's private RAM.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SpawnerState {
    pub section: u8,
    pub column_base: u8,
    pub cursor: u32,
    pub timer: u8,
    pub spawn_count: u8,
    pub cannon: u8,
    pub class_counts: [u8; 4],
    pub init_pending: bool,
    pub record_read: bool,
    pub disabled: bool,
    pub pending: Option<PendingState>,
}

impl Spawner {
    /// Initialise for a section at the given `$3F` value (`$A2CB`).
    pub fn new(spawns: &'static Spawns, section: u8, scroll_high: u8) -> Self {
        let base = scroll_high & 0x0E;
        let column = usize::from(base >> 1).min(COLUMN_SLOTS - 1);
        let section = usize::from(section).min(SECTION_COUNT - 1);
        Self {
            spawns,
            section: section as u8,
            column_base: base,
            cursor: 0,
            stream: spawns.sections[section][column].as_slice(),
            pending: None,
            timer: 0,
            spawn_count: 0,
            cannon: 0,
            class_counts: [0; 4],
            init_pending: false,
            record_read: false,
            disabled: false,
        }
    }

    /// `$A2C6` initialisation: `$61 = $3F & $0E`, pointer from `$A7CA[$19]`.
    pub fn restart(&mut self, section: u8, scroll_high: u8) {
        let section = usize::from(section).min(SECTION_COUNT - 1);
        let base = scroll_high & 0x0E;
        let column = usize::from(base >> 1).min(COLUMN_SLOTS - 1);
        self.section = section as u8;
        self.column_base = base;
        self.cursor = 0;
        self.stream = self.spawns.sections[section][column].as_slice();
        self.pending = None;
        self.spawn_count = 0;
        self.init_pending = false;
        self.disabled = false;
    }

    /// `$60 = 1`: the next `$A2BA` call only re-initialises `$61`/`$6A`.
    pub fn defer_init(&mut self) {
        self.init_pending = true;
    }

    /// `$C1D0`: `$60 = 0`, so `$A2BA` returns at `$A2C5` for good.
    pub fn disable(&mut self) {
        self.disabled = true;
    }

    /// Copy the private RAM for a save state.
    pub(crate) fn state(&self) -> SpawnerState {
        SpawnerState {
            section: self.section,
            column_base: self.column_base,
            cursor: self.cursor as u32,
            timer: self.timer,
            spawn_count: self.spawn_count,
            cannon: self.cannon,
            class_counts: self.class_counts,
            init_pending: self.init_pending,
            record_read: self.record_read,
            disabled: self.disabled,
            pending: self.pending.map(|pending| PendingState {
                remaining: pending.remaining,
                interval: pending.interval,
                y: pending.y,
                y_step: pending.y_step,
                kind: pending.kind,
                subtype: pending.subtype,
                substate: pending.substate,
                x: pending.x,
                motion: pending.motion,
                phase: pending.phase,
                attempted: pending.attempted,
            }),
        }
    }

    /// Restore a state written by [`state`](Self::state). `stream` is derived
    /// from `section`/`column_base`, exactly like [`new`](Self::new).
    pub(crate) fn set_state(&mut self, state: SpawnerState) {
        let section = usize::from(state.section).min(SECTION_COUNT - 1);
        let base = state.column_base & 0x0E;
        let column = usize::from(base >> 1).min(COLUMN_SLOTS - 1);
        self.section = section as u8;
        self.column_base = base;
        self.stream = self.spawns.sections[section][column].as_slice();
        self.cursor = state.cursor as usize;
        self.timer = state.timer;
        self.spawn_count = state.spawn_count;
        self.cannon = state.cannon;
        self.class_counts = state.class_counts;
        self.init_pending = state.init_pending;
        self.record_read = state.record_read;
        self.disabled = state.disabled;
        self.pending = state.pending.map(|pending| Pending {
            remaining: pending.remaining,
            interval: pending.interval,
            y: pending.y,
            y_step: pending.y_step,
            kind: pending.kind,
            subtype: pending.subtype,
            substate: pending.substate,
            x: pending.x,
            motion: pending.motion,
            phase: pending.phase,
            attempted: pending.attempted,
        });
    }

    /// One `$A2BA` call. Returns at most one emission, exactly like the
    /// original (it consumes one record or spawns one formation member).
    pub fn step(&mut self, scroll: Scroll) -> Option<Emission> {
        self.record_read = false;
        // `$A2C1`: `$60 = 0` returns before the stream is read.
        if self.disabled {
            return None;
        }
        // `$A2C1`-`$A2C9`: `$60 = 1` takes the initialisation path and only
        // re-reads `$61` and the `$6A/$6B` pointer for the current `$3F`.
        if self.init_pending {
            self.init_pending = false;
            self.restart_window(scroll.high);
            return None;
        }
        // `$A2F8`: `$69 != 0` keeps the pending formation running. Once `$69`
        // reaches 0 the same call falls through to the stream below.
        if self.pending.is_some() {
            let done = self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.remaining == 0);
            if !done {
                // `$A325`: `$6C` counts down; only a zero timer reaches `$A40B`.
                if self.timer != 0 {
                    self.timer -= 1;
                    return None;
                }
                return self.pending_step();
            }
            self.pending = None;
        }

        // $A2FC: entering the next 512-px window re-selects the column stream.
        if scroll.high == self.column_base.wrapping_add(2) {
            self.restart_window(scroll.high);
            return None;
        }

        // $A304: compare ($3F,$3E) with ($61 + 2*delay) split across two bytes.
        let Some(record) = self.stream.get(self.cursor).copied() else {
            // The pointer sits on the stream's `$FF` terminator. `$A32F`
            // raises `$5D` before the terminator check, so the exhausted
            // pointer keeps blocking the frame's `$BBB1` scan while its
            // target is reached; the terminator itself never spawns.
            let (target_high, target_low) = target(self.column_base, 0xFF);
            if scroll.high > target_high || (scroll.high == target_high && scroll.low >= target_low)
            {
                self.record_read = true;
            }
            return None;
        };
        let (target_high, target_low) = match record {
            Record::Spawn { delay, .. } => target(self.column_base, delay),
            // `$A304` reads the first byte of *every* record as the delay, so a
            // command fires on the same schedule; its second byte ($F0+) only
            // decides that it is a command when it is consumed at `$A341`.
            Record::Command(bytes) => target(self.column_base, bytes[0]),
        };
        let reached =
            scroll.high > target_high || (scroll.high == target_high && scroll.low >= target_low);
        if !reached {
            return None;
        }

        self.cursor += 1;
        self.spawn_count = self.spawn_count.wrapping_add(1);
        // `$A32F`: reading a record raises `$5D`; the members a formation
        // spawns later do not.
        self.record_read = true;
        match record {
            Record::Command(bytes) => Some(Emission::Command(bytes)),
            Record::Spawn { kind, .. } => self.resolve(kind),
        }
    }

    fn restart_window(&mut self, scroll_high: u8) {
        self.column_base = scroll_high & 0x0E;
        let column = usize::from(self.column_base >> 1).min(COLUMN_SLOTS - 1);
        self.cursor = 0;
        self.stream = self.spawns.sections[usize::from(self.section)][column].as_slice();
        self.pending = None;
    }

    fn resolve(&mut self, kind: u8) -> Option<Emission> {
        if kind < 0x80 {
            // $A3AB: 3-byte terrain definition.
            let def = self.spawns.terrain_defs[usize::from(kind) & 0x7F];
            let (kind, x) = {
                let offset = def[0].wrapping_sub(0xA0);
                if offset < 0x30 {
                    (offset, 0xF0)
                } else {
                    (offset.wrapping_sub(0x30), 0x10)
                }
            };
            Some(Emission::Spawn(SpawnEvent {
                kind,
                subtype: 0,
                substate: 0,
                flag: 0,
                x,
                y: def[2],
                motion: def[1],
                phase: def[1] & 1,
                terrain: true,
            }))
        } else {
            // $A3DE: 4-byte definition and its formation descriptor.
            let def = self.spawns.object_defs[usize::from(kind & 0x3F)];
            let pair = self.spawns.formation_subtypes[usize::from(def[2]) & 0x3F];
            // `$A5B6` entries are `(delay, Y add, motion)`. The measured
            // oracle formation `$F4/$2A` spawns 11 frames apart (`$0A` timer plus the spawn frame)
            // and moves 2 px/frame, giving the 22-px X spacing at frame 414.
            let params = self.spawns.formation_params
                [usize::from(def[3]) % self.spawns.formation_params.len()];
            let count = pair[0] & 0x0F;
            let x = pair[0] & 0xF0;
            let delay = params[0];
            let y_step = params[1];
            let motion = params[2];
            // `$A3E9`/`$A452`: once per record, a class of four or more takes
            // the alternating `$49` cannon index for `$03AC` and resets the
            // `$0048` member count of that class.
            let phase = if count >= 4 && def[1] != 0x0B {
                self.cannon = (self.cannon.wrapping_add(1) & 1) | 0x02;
                self.class_counts[usize::from(self.cannon) & 0x03] = count;
                self.cannon
            } else {
                motion & 1
            };
            // `$A3DE` primes the formation: `$69 = count` and `$6E = pair[1]`,
            // leaving `$6C` alone, then falls into `$A40B` for the first try.
            self.pending = Some(Pending {
                remaining: count,
                interval: delay,
                y: pair[1],
                y_step,
                kind: def[1],
                subtype: def[0],
                substate: def[2],
                x,
                motion,
                phase,
                attempted: false,
            });
            self.pending_step()
        }
    }

    /// `$BE9F`: a destroyed cannon part decrements its `$0048` class count;
    /// the new `$03AC` is 0 while members remain and 1 for the last one, which
    /// is what makes its wreck keep drifting as a background piece.
    pub fn release_cannon(&mut self, phase: u8) -> u8 {
        match phase {
            0 | 1 => phase,
            phase => {
                let index = usize::from(phase) & 0x03;
                self.class_counts[index] = self.class_counts[index].wrapping_sub(1);
                if self.class_counts[index] == 0 {
                    1
                } else {
                    0
                }
            }
        }
    }

    /// `$A40B`: one formation member per `$69` countdown expiry.
    fn pending_step(&mut self) -> Option<Emission> {
        let pending = self.pending.as_mut()?;
        // The previous attempt is still awaiting its slot result.
        if pending.attempted {
            return None;
        }
        // `$A40B: DEC $69; BMI` — the countdown is consumed before the slot
        // search, so an allocation failure still spends one attempt.
        if pending.remaining == 0 {
            self.pending = None;
            return None;
        }
        pending.remaining -= 1;
        pending.attempted = true;
        Some(Emission::Spawn(SpawnEvent {
            kind: pending.kind,
            subtype: pending.subtype,
            substate: pending.substate,
            flag: 0,
            x: pending.x,
            // `$A42E` adds the Y step before storing the member; `$6E` only
            // keeps the new value when the slot allocation succeeds.
            y: pending.y.wrapping_add(pending.y_step),
            motion: pending.motion,
            phase: pending.phase,
            terrain: false,
        }))
    }

    /// Report whether the last [`Emission::Spawn`] found a free slot.
    ///
    /// The original charges `$69` before `$A411`'s search, so a full table
    /// loses that attempt but leaves `$6C`/`$6E` untouched; the next frame
    /// retries with the same Y (this is why a dropped first member still
    /// appears at the formation's first Y instead of one step down).
    pub fn commit(&mut self, spawned: bool) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        if !pending.attempted {
            return;
        }
        pending.attempted = false;
        if spawned {
            pending.y = pending.y.wrapping_add(pending.y_step);
            let (interval, remaining) = (pending.interval, pending.remaining);
            // `$A42C`: only a successful allocation reloads `$6C`.
            self.timer = interval;
            if remaining == 0 {
                self.pending = None;
            }
        }
    }
}

/// `$A30A`: `($61 + 2*delay) : (2*delay & $FF)`.
fn target(column_base: u8, delay: u8) -> (u8, u8) {
    let doubled = u16::from(delay) << 1;
    (
        column_base.wrapping_add((doubled >> 8) as u8),
        doubled as u8,
    )
}

/// `$9B82`: ship Y and X from the per-section table, with the Y byte low in
/// `$3F` picking the row.
pub fn ship_start(rom: &crate::rom::RomData, section: u8, scroll_high: u8) -> (u8, u8) {
    let section = usize::from(section).min(tables::SHIP_Y_OFFSET.len() - 1);
    let index = usize::from(tables::SHIP_Y_OFFSET[section])
        .wrapping_add(usize::from(scroll_high >> 1))
        .min(rom.tables.ship_start.len() - 1);
    let byte = rom.tables.ship_start[index];
    // Y = byte & $F0, X = byte << 4 (stored in an 8-bit register, so the high
    // nibble is lost, exactly like $9B8F-$9BA9).
    (byte.wrapping_shl(4), byte & 0xF0)
}

/// `$9A40`: has the section reached its `$3F` threshold?
pub fn section_done(section: u8, scroll: Scroll) -> bool {
    let index = usize::from(section).min(tables::SECTION_THRESHOLD.len() - 1);
    scroll.high >= tables::SECTION_THRESHOLD[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::testing::rom;

    #[test]
    fn scroll_is_half_a_pixel_per_frame() {
        let mut scroll = Scroll::default();
        for _ in 0..4 {
            scroll.step();
        }
        assert_eq!((scroll.high, scroll.low, scroll.fraction), (0, 2, 0));
        scroll.step();
        assert_eq!((scroll.low, scroll.fraction), (2, 128));
    }

    #[test]
    fn nametable_flips_every_256_pixels() {
        let mut scroll = Scroll {
            fraction: 0,
            low: 0,
            high: 1,
        };
        assert_eq!(scroll.nametable(), 1);
        scroll.high = 2;
        assert_eq!(scroll.nametable(), 0);
        assert_eq!(scroll.position(), 0x0200);
    }

    #[test]
    fn spawn_target_matches_the_rom_split() {
        // delay $10 -> 2*delay = $20: high byte $61, low byte $20.
        assert_eq!(target(0x00, 0x10), (0x00, 0x20));
        assert_eq!(target(0x0C, 0x10), (0x0C, 0x20));
        // delay $E0 -> 2*delay = $1C0: carry into the high byte.
        assert_eq!(target(0x00, 0xE0), (0x01, 0xC0));
        assert_eq!(target(0x0C, 0x90), (0x0D, 0x20));
    }

    #[test]
    fn section_zero_column_zero_keeps_the_documented_order() {
        let s0 = rom().spawns.sections[0][0].as_slice();
        assert_eq!(s0.len(), 10);
        assert_eq!(
            s0[0],
            Record::Spawn {
                delay: 0x10,
                kind: 0x80
            }
        );
        assert_eq!(
            s0[9],
            Record::Spawn {
                delay: 0xE0,
                kind: 0x80
            }
        );
    }

    #[test]
    fn an_exhausted_stream_still_raises_the_five_d_flag() {
        // `$A32F` raises `$5D` before `$A33B` sees the `$FF` terminator, so
        // the exhausted pointer keeps `$BBB1` blocked while its target (the
        // terminator's $FF delay: high base+1, low $FE) is reached.
        let mut spawner = Spawner::new(&rom().spawns, 0, 0);
        spawner.cursor = spawner.stream.len();
        let mut scroll = Scroll {
            fraction: 0,
            low: 0xFD,
            high: 1,
        };
        assert!(spawner.step(scroll).is_none());
        assert!(!spawner.record_read);
        scroll.low = 0xFE;
        assert!(spawner.step(scroll).is_none());
        assert!(spawner.record_read);
    }

    #[test]
    fn every_stream_ends_without_a_dangling_command() {
        for section in &rom().spawns.sections {
            for stream in section {
                assert!(stream.len() < 128, "a stream must be finite");
            }
        }
    }

    #[test]
    fn ship_start_matches_the_measured_frame_900_position() {
        assert_eq!(ship_start(rom(), 0, 0), (0x50, 0x60));
        // Section 4 starts at row offset 0; $3F = $12 selects row 9.
        assert_eq!(ship_start(rom(), 4, 0x12), (0x60, 0x60));
        // Section 6 offset $14 plus $3F = 8 selects row 24 ($73).
        assert_eq!(ship_start(rom(), 6, 0x08), (0x30, 0x70));
    }

    #[test]
    fn a_terrain_record_uses_the_a3ab_field_mapping() {
        // `$A3AB`: 3-byte `$A65C` definition; type 1 is `A6 81 B7`, so the
        // X position is `$F0`, kind `$06`, Y `$B7` and motion `$81`.
        let mut spawner = Spawner::new(&rom().spawns, 0, 0);
        let event = match spawner.resolve(1) {
            Some(Emission::Spawn(event)) => event,
            other => panic!("expected a terrain spawn, got {other:?}"),
        };
        assert!(event.terrain);
        assert_eq!(event.kind, 6);
        assert_eq!(event.x, 0xF0);
        assert_eq!(event.y, 0xB7);
        assert_eq!(event.motion, 0x81);
        assert_eq!(event.subtype, 0);
        // Type 4 is `E3 80 10`: `def[0] - $D0` with X `$10`.
        let event = match spawner.resolve(4) {
            Some(Emission::Spawn(event)) => event,
            other => panic!("expected a terrain spawn, got {other:?}"),
        };
        assert_eq!(event.kind, 0x13);
        assert_eq!(event.x, 0x10);
        assert_eq!(event.y, 0x10);
    }

    #[test]
    fn a_failed_formation_allocation_retries_at_the_first_y() {
        // `$A40B` charges `$69` before `$A411`'s slot search. A full table
        // therefore skips the member without running `$A42E`, so `$6E` stays
        // at the formation's base Y and the retry spawns the *first* member
        // rather than one `y_step` lower (the stage-2 12161 `$83/$84` gap).
        let mut spawner = Spawner::new(&rom().spawns, 0, 0);
        // Kind `$88` -> def 8: substate 3 -> pair `($F2, $08)`, formation 7
        // -> interval 2, Y step `$40`. First member Y = `$08 + $40`.
        let first = match spawner.resolve(0x88) {
            Some(Emission::Spawn(event)) => event,
            other => panic!("expected a formation member, got {other:?}"),
        };
        assert_eq!(first.y, 0x48);
        // No free slot: the attempt is lost but `$6E`/`$6C` do not advance.
        spawner.commit(false);
        let pending = spawner.pending.expect("the retry is still queued");
        assert_eq!(pending.remaining, 1);
        assert_eq!(spawner.timer, 0, "the frame retries immediately");
        assert_eq!(pending.y, 0x08, "`$6E` kept the base Y");
        let retry = match spawner.pending_step() {
            Some(Emission::Spawn(event)) => event,
            other => panic!("expected a retry, got {other:?}"),
        };
        assert_eq!(retry.y, 0x48, "the retry is the first member, not +$40");
        spawner.commit(true);
        assert!(spawner.pending.is_none(), "the two-member formation ends");
    }

    #[test]
    fn a_formation_member_waits_the_interval_between_spawns() {
        let mut spawner = Spawner::new(&rom().spawns, 0, 0);
        let event = match spawner.resolve(0x88) {
            Some(Emission::Spawn(event)) => event,
            other => panic!("expected a formation member, got {other:?}"),
        };
        assert_eq!(event.y, 0x48);
        spawner.commit(true);
        // Interval 2: two timer decrements, then the second member.
        for _ in 0..2 {
            assert!(spawner.step(Scroll::default()).is_none());
        }
        let second = match spawner.step(Scroll::default()) {
            Some(Emission::Spawn(event)) => event,
            other => panic!("expected the second member, got {other:?}"),
        };
        assert_eq!(second.y, 0x88, "`$6E` advanced by Y step");
    }

    #[test]
    fn a_formation_that_misses_keeps_the_previous_interval() {
        // `$6C` is plain RAM and `$A3DE` does not reload it, so when the first
        // member of a new formation cannot be placed the retry still waits out
        // the interval left by the previous formation's last successful member.
        // Stage 5 needs this: the `$90` formation reads with every slot full
        // and only lands one `$8E` member once a slot frees.
        let mut spawner = Spawner::new(&rom().spawns, 4, 0);
        let mut scroll = Scroll::default();
        let mut missed = None;
        let mut retried = None;
        for frame in 0..1200u32 {
            if let Some(Emission::Spawn(event)) = spawner.step(scroll) {
                let is_wanted = event.kind == 0x0E;
                if is_wanted && missed.is_none() {
                    // No free slot: the attempt is lost.
                    spawner.commit(false);
                    missed = Some(frame);
                } else {
                    if is_wanted && retried.is_none() {
                        retried = Some(frame);
                    }
                    spawner.commit(true);
                }
            }
            scroll.step();
        }
        let missed = missed.expect("the stage-5 stream reaches a $0E formation");
        // The `$8F` record before it uses interval `$0A`: ten zero-timer frames
        // then the attempt, i.e. eleven frames after the failed one.
        assert_eq!(retried.expect("the member retries") - missed, 11);
    }

    #[test]
    fn an_object_record_uses_the_a5b6_formation_bytes() {
        // `$A5B6` entry 0 is `($0A, Y add $00, motion $C8)`; pair `$A58C`
        // entry 0 is `($F4, $2A)` so the first member sits at `$2A`.
        let mut spawner = Spawner::new(&rom().spawns, 0, 0);
        let event = match spawner.resolve(0x80) {
            Some(Emission::Spawn(event)) => event,
            other => panic!("expected a formation member, got {other:?}"),
        };
        assert!(!event.terrain);
        assert_eq!(event.kind, 5);
        assert_eq!(event.subtype, 1);
        assert_eq!(event.x, 0xF0);
        assert_eq!(event.y, 0x2A);
        assert_eq!(event.motion, 0xC8);
        // `$A40B` charged `$69` before the slot search; report the success so
        // the timer and `$6E` advance.
        spawner.commit(true);
        let pending = spawner.pending.expect("four-member formation");
        assert_eq!(pending.remaining, 3);
        assert_eq!(pending.y_step, 0x00);
        assert_eq!(pending.interval, 0x0A);
        // `$A40B` waits ten frames, then emits a member and reloads the timer.
        for _ in 0..10 {
            assert!(spawner.step(Scroll::default()).is_none());
        }
        let next = spawner.step(Scroll::default()).expect("member");
        match next {
            Emission::Spawn(member) => assert_eq!(member.y, 0x2A),
            Emission::Command(_) => panic!("formation member expected"),
        }
        spawner.commit(true);
        assert_eq!(spawner.pending.expect("three left").remaining, 2);
    }
}
