//! The stage-1 Big Core fight: per-frame state line plus boss HP.
//!
//! `record-boss` extends the native `Game::dump_state` line with one
//! append-only field, so the fight is readable frame by frame:
//!
//! ```text
//! frame=<n> ... objects=<slot>:<type>:<x>:<y>:<substate>;...
//! boss=<slot>:<x>:<y>:<damage>:<overheat>:<counter>
//! ```
//!
//! `boss=-` means no core is live. When it is:
//!
//! * `slot` is the object slot (9 in stage 1),
//! * `damage` is `$046C`, the armour counter; the core dies at 7,
//! * `overheat` is `$04CC`, incremented on every 256-frame wrap of `$04AC`;
//!   reaching 6 self-destructs the core,
//! * `counter` is the raw `$04AC` free-running counter.
//!
//! The score already in the state line is the score trajectory: every kill
//! adds a fixed BCD value (`$8468` = 10, the Big Core `$845A` = 1000).

use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use gradius_nes::Emulator;

use crate::state::StateLine;

/// The Big Core's raw `$030C` type and its fixed stage-1 slot (`$9993`).
pub const BOSS_KIND: u8 = 0x98;
pub const BOSS_SLOT: u8 = 9;

/// Boss armour hits needed to destroy the core (`$B92C`: `$046C >= 7`).
pub const BOSS_DAMAGE_LIMIT: u8 = 7;

/// Live Big Core counters read from the object table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BossStatus {
    pub slot: u8,
    /// `$036C`: X integer (240 on spawn, 164 when it stops).
    pub x: u8,
    /// `$032C`: Y integer, clamped to 24..=168.
    pub y: u8,
    /// `$046C`: damage counter, 0..=7.
    pub damage: u8,
    /// `$04CC`: overheat wraps.
    pub overheat: u8,
    /// `$04AC`: free-running frame counter.
    pub counter: u8,
}

impl BossStatus {
    /// Locate the core in the live object table. The core has no sprite frame
    /// (`$0120`) until its first animation, so `Emulator::objects()` skips it;
    /// read the fixed stage-1 slot directly.
    pub fn from_emulator(emulator: &Emulator) -> Option<Self> {
        let ram = &emulator.bus.ram;
        let slot = BOSS_SLOT;
        if ram[0x030C + usize::from(slot)] != BOSS_KIND {
            return None;
        }
        let slot = usize::from(slot);
        Some(Self {
            slot: BOSS_SLOT,
            x: ram[0x036C + slot],
            y: ram[0x032C + slot],
            damage: ram[0x046C + slot],
            overheat: ram[0x04CC + slot],
            counter: ram[0x04AC + slot],
        })
    }

    pub fn render(self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}",
            self.slot, self.x, self.y, self.damage, self.overheat, self.counter
        )
    }

    #[cfg(test)]
    pub fn parse(text: &str) -> Result<Self> {
        if text == "-" {
            bail!("`-` is the absent-boss marker, not a boss status");
        }
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 6 {
            bail!("expected slot:x:y:damage:overheat:counter, got `{text}`");
        }
        let component = |index: usize, name: &str| -> Result<u8> {
            parts[index]
                .parse()
                .with_context(|| format!("bad {name} in `{text}`"))
        };
        Ok(Self {
            slot: component(0, "slot")?,
            x: component(1, "x")?,
            y: component(2, "y")?,
            damage: component(3, "damage")?,
            overheat: component(4, "overheat")?,
            counter: component(5, "counter")?,
        })
    }
}

/// One recorded frame: the shared state line plus the boss field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BossLine {
    pub state: StateLine,
    pub boss: Option<BossStatus>,
}

impl BossLine {
    pub fn from_emulator(emulator: &Emulator) -> Self {
        Self {
            state: StateLine::from_emulator(emulator),
            boss: BossStatus::from_emulator(emulator),
        }
    }

    pub fn render(&self) -> String {
        let boss = self
            .boss
            .map(BossStatus::render)
            .unwrap_or_else(|| "-".to_string());
        format!("{} boss={boss}", self.state.render())
    }

    /// Parse a line produced by [`Self::render`].
    #[cfg(test)]
    pub fn parse(text: &str) -> Result<Self> {
        let (state, boss) = text
            .rsplit_once(" boss=")
            .ok_or_else(|| anyhow!("missing `boss=` field in `{text}`"))?;
        let boss = if boss == "-" {
            None
        } else {
            Some(BossStatus::parse(boss)?)
        };
        Ok(Self {
            state: StateLine::parse(state)?,
            boss,
        })
    }
}

/// First-seen bookkeeping for the summary printed by `record-boss`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BossTimeline {
    pub spawn_frame: Option<u64>,
    pub first_damage_frame: Option<u64>,
    pub max_damage: u8,
    pub death_frame: Option<u64>,
    pub spawn_score: Option<u32>,
    pub last_live_score: Option<u32>,
    pub death_score: Option<u32>,
    pub final_score: u32,
}

impl BossTimeline {
    /// Feed one frame.
    pub fn observe(&mut self, frame: u64, score: u32, boss: Option<BossStatus>) {
        self.final_score = score;
        match boss {
            Some(boss) => {
                if self.spawn_frame.is_none() {
                    self.spawn_frame = Some(frame);
                    self.spawn_score = Some(score);
                }
                self.last_live_score = Some(score);
                if boss.damage > self.max_damage {
                    self.max_damage = boss.damage;
                }
                if boss.damage > 0 && self.first_damage_frame.is_none() {
                    self.first_damage_frame = Some(frame);
                }
            }
            None if self.spawn_frame.is_some() && self.death_frame.is_none() => {
                self.death_frame = Some(frame);
                self.death_score = Some(score);
            }
            None => {}
        }
    }

    /// Human-readable proof line printed after the state file.
    ///
    /// `stage` is 1-based, `next_stage` is the frame `$19` moved off that
    /// stage. Stage 1 keeps the historical "stage 2 from" wording.
    pub fn summary(
        &self,
        frames: u64,
        scroll_stop: Option<u64>,
        next_stage: Option<u64>,
        stage: u8,
    ) -> String {
        let spawn = self
            .spawn_frame
            .map(|frame| frame.to_string())
            .unwrap_or_else(|| "never".to_string());
        let stop = scroll_stop
            .map(|frame| frame.to_string())
            .unwrap_or_else(|| "?".to_string());
        let damage = self
            .first_damage_frame
            .map(|frame| frame.to_string())
            .unwrap_or_else(|| "never".to_string());
        let death = self
            .death_frame
            .map(|frame| frame.to_string())
            .unwrap_or_else(|| "alive".to_string());
        let spawn_score = self
            .spawn_score
            .map(|score| score.to_string())
            .unwrap_or_else(|| "?".to_string());
        let reward = match (self.last_live_score, self.death_score) {
            (Some(before), Some(after)) => {
                format!(" (boss reward +{})", after.saturating_sub(before))
            }
            _ => String::new(),
        };
        let next = next_stage
            .map(|frame| frame.to_string())
            .unwrap_or_else(|| "?".to_string());
        format!(
            "boss: spawn {spawn} score {spawn_score}, scroll stop {stop}, \
             first damage {damage}, max damage {}/{}, end {death}{reward}, \
             {} from {next}, final score {}, {frames} frames recorded",
            self.max_damage,
            BOSS_DAMAGE_LIMIT,
            next_stage_label(stage),
            self.final_score
        )
    }

    /// Fail unless the core was reached and took at least one hit.
    pub fn check(&self, frames: u64) -> Result<()> {
        let spawn = self
            .spawn_frame
            .ok_or_else(|| anyhow!("the Big Core never spawned within {frames} frames"))?;
        if self.first_damage_frame.is_none() {
            bail!("the Big Core spawned at frame {spawn} but never took a hit");
        }
        Ok(())
    }

    /// Stage-aware proof. Stages 6 and 7 skip the `$9975` core spawn
    /// (`$99B3` sets `$1B = $86` directly), so their evidence is the stage
    /// ending, not a core fight.
    pub fn check_stage(&self, stage: u8, frames: u64, next_stage: Option<u64>) -> Result<()> {
        if stage >= 6 {
            if next_stage.is_none() {
                bail!("stage {stage} has no Big Core and did not end within {frames} frames");
            }
            return Ok(());
        }
        self.check(frames)
    }
}

/// `$9C3F`: rank = stage bit + weapon + options + shield, clamped to 7.
pub fn stage_rank(ram: &[u8]) -> u8 {
    (u8::from(ram[0x44] != 0) + ram[0x45] + u8::from(ram[0x46] != 0) + u8::from(ram[0x19] != 0))
        .min(7)
}

/// Render one natural-path core frame from the oracle's CPU RAM.
///
/// The slot-9 fields are the raw parallel arrays at `base + 0x0C + 9`.
pub fn trace_line(frame: u64, ram: &[u8]) -> String {
    const SLOT: usize = 9;
    let score = {
        let bcd = [ram[0x07E4], ram[0x07E5], ram[0x07E6]];
        let mut value = 0u32;
        for byte in bcd.iter().rev() {
            value = value * 100 + u32::from(byte >> 4) * 10 + u32::from(byte & 0x0F);
        }
        value
    };
    format!(
        "f{frame} stage={:02X} phase={:02X} scroll={:02X}:{:02X}:{:02X} score={score} \
         rank={}(w{} o{} s{} st{}) s17={:02X} \
         boss={:02X}:{}:{}:{}:{}:{}:{}:{}\n",
        ram[0x19],
        ram[0x1B],
        ram[0x3E],
        ram[0x3F],
        ram[0x3D],
        stage_rank(ram),
        u8::from(ram[0x44] != 0),
        ram[0x45],
        u8::from(ram[0x46] != 0),
        u8::from(ram[0x19] != 0),
        ram[0x17],
        ram[0x030C + SLOT],
        ram[0x036C + SLOT],
        ram[0x032C + SLOT],
        ram[0x048C + SLOT],
        ram[0x046C + SLOT],
        ram[0x04AC + SLOT],
        ram[0x04CC + SLOT],
        ram[0x042C + SLOT],
    )
}

/// `boss-trace`: follow the core's raw counters on the natural path.
pub fn trace(
    rom: &Path,
    script: &Path,
    from: u64,
    to: u64,
    keep_alive: bool,
    out: Option<&Path>,
) -> Result<()> {
    if from == 0 {
        bail!("--from must be at least 1");
    }
    if to <= from {
        bail!("--to must be greater than --from");
    }
    let cart = crate::load(rom)?;
    let source =
        fs::read_to_string(script).with_context(|| format!("reading {}", script.display()))?;
    let plan = crate::transition::parse_script(&source)?;
    let mut emulator = Emulator::new(&cart)?;
    let mut runner = crate::oracle::Runner::new();
    runner.keep_alive = keep_alive;
    runner.run_to(&mut emulator, from - 1, &plan, false);
    let mut text = format!(
        "; frames {from}..{to} (exclusive)\n\
         ; stage=$19 phase=$1B scroll=$3E:$3F:$3D rank=$9C3F \
         boss=kind:x:y:substate:$048C:damage:$046C:counter:$04AC:overheat:$04CC:period:$042C\n"
    );
    for _ in from..to {
        runner.advance(&mut emulator, &plan, false);
        text.push_str(&trace_line(emulator.frames, &emulator.bus.ram));
    }
    match out {
        Some(path) => {
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            println!(
                "wrote {} ({} frames)",
                path.display(),
                text.lines().count().saturating_sub(2)
            );
        }
        None => print!("{text}"),
    }
    Ok(())
}

/// Label for the frame at which `$19` leaves `stage` (1-based).
fn next_stage_label(stage: u8) -> String {
    match stage {
        1 => "stage 2".to_string(),
        7 => "stage 1 (loop)".to_string(),
        other => format!("stage {}", other + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GameState, StateObject};

    fn state() -> StateLine {
        StateLine {
            frame: 7723,
            state: GameState::Playing,
            px: 80,
            pf: 0,
            py: 96,
            pyf: 0,
            speed: 0,
            scroll_low: 0,
            scroll_high: 0x0D,
            scroll_frac: 0,
            score: 700,
            lives: 3,
            capsule: 0,
            meter: 1,
            objects: vec![StateObject {
                slot: 9,
                kind: BOSS_KIND,
                x: 240,
                y: 128,
                substate: 0,
            }],
        }
    }

    #[test]
    fn boss_line_appends_the_boss_field() {
        let line = BossLine {
            state: state(),
            boss: Some(BossStatus {
                slot: 9,
                x: 240,
                y: 128,
                damage: 2,
                overheat: 1,
                counter: 0x5A,
            }),
        };
        assert_eq!(
            line.render(),
            "frame=7723 state=Playing px=80 pf=0 py=96 pyf=0 speed=0 \
             scroll=0:13:0 score=700 lives=3 capsule=0 meter=1 \
             objects=9:152:240:128:0 boss=9:240:128:2:1:90"
        );
    }

    #[test]
    fn boss_line_round_trips_with_and_without_a_core() {
        let mut line = BossLine {
            state: state(),
            boss: Some(BossStatus {
                slot: 9,
                x: 240,
                y: 128,
                damage: 0,
                overheat: 0,
                counter: 0,
            }),
        };
        assert_eq!(BossLine::parse(&line.render()).expect("parse"), line);
        line.boss = None;
        let text = line.render();
        assert!(text.ends_with(" boss=-"));
        assert_eq!(BossLine::parse(&text).expect("parse"), line);
    }

    #[test]
    fn malformed_boss_fields_are_rejected() {
        let text = BossLine {
            state: state(),
            boss: None,
        }
        .render();
        assert!(BossLine::parse(&text.replace("boss=-", "boss=9:1:2")).is_err());
        assert!(BossLine::parse(&text.replace("boss=-", "boss=9:x:2:3")).is_err());
        assert!(BossLine::parse(&text.replace(" boss=-", "")).is_err());
    }

    #[test]
    fn timeline_records_spawn_damage_and_death() {
        let mut timeline = BossTimeline::default();
        let boss = |damage| {
            Some(BossStatus {
                slot: 9,
                x: 164,
                y: 96,
                damage,
                overheat: 1,
                counter: 4,
            })
        };
        timeline.observe(10, 0, None);
        timeline.observe(11, 100, boss(0));
        timeline.observe(12, 120, boss(2));
        timeline.observe(13, 140, boss(7));
        timeline.observe(14, 1_000, None);
        assert_eq!(timeline.spawn_frame, Some(11));
        assert_eq!(timeline.first_damage_frame, Some(12));
        assert_eq!(timeline.max_damage, 7);
        assert_eq!(timeline.death_frame, Some(14));
        assert_eq!(timeline.spawn_score, Some(100));
        assert_eq!(timeline.last_live_score, Some(140));
        assert_eq!(timeline.death_score, Some(1_000));
        assert_eq!(timeline.final_score, 1_000);
        assert!(timeline.check(100).is_ok());
    }

    #[test]
    fn timeline_fails_when_the_core_is_never_reached() {
        let timeline = BossTimeline::default();
        let error = timeline.check(9000).expect_err("must fail");
        assert!(error.to_string().contains("never spawned"));
    }

    #[test]
    fn summary_names_the_next_stage() {
        assert_eq!(next_stage_label(1), "stage 2");
        assert_eq!(next_stage_label(2), "stage 3");
        assert_eq!(next_stage_label(6), "stage 7");
        assert_eq!(next_stage_label(7), "stage 1 (loop)");

        let timeline = BossTimeline {
            spawn_frame: Some(10),
            first_damage_frame: Some(20),
            max_damage: 6,
            death_frame: Some(30),
            final_score: 1_000,
            ..BossTimeline::default()
        };
        let text = timeline.summary(100, Some(9), Some(40), 1);
        assert!(text.contains("stage 2 from 40"), "{text}");
        let text = timeline.summary(100, Some(9), Some(40), 4);
        assert!(text.contains("stage 5 from 40"), "{text}");
    }

    #[test]
    fn stages_six_and_seven_only_need_the_stage_to_end() {
        let timeline = BossTimeline::default();
        assert!(timeline.check_stage(6, 100, None).is_err());
        assert!(timeline.check_stage(6, 100, Some(50)).is_ok());
        assert!(timeline.check_stage(7, 100, Some(50)).is_ok());
        assert!(timeline.check_stage(3, 100, Some(50)).is_err());
    }

    #[test]
    fn stage_rank_includes_the_stage_bit() {
        let mut ram = vec![0u8; 0x800];
        ram[0x44] = 1; // weapon
        ram[0x45] = 2; // options
        ram[0x46] = 5; // shield
        assert_eq!(stage_rank(&ram), 4, "stage 1: weapon+options+shield+base");
        ram[0x19] = 1; // stage 2
        assert_eq!(stage_rank(&ram), 5, "stage 2 adds one");
        ram[0x45] = 6;
        assert_eq!(stage_rank(&ram), 7, "clamped to 7");
    }

    #[test]
    fn trace_line_renders_the_core_counters() {
        let mut ram = vec![0u8; 0x800];
        ram[0x19] = 1;
        ram[0x1B] = 0x80;
        ram[0x3F] = 0x0D;
        ram[0x44] = 1;
        ram[0x45] = 2;
        ram[0x46] = 5;
        ram[0x17] = 5;
        ram[0x07E4] = 0x60;
        ram[0x07E5] = 0x27;
        ram[0x030C + 9] = 0x98;
        ram[0x036C + 9] = 163;
        ram[0x032C + 9] = 121;
        ram[0x048C + 9] = 0;
        ram[0x046C + 9] = 0;
        ram[0x04AC + 9] = 17;
        ram[0x04CC + 9] = 5;
        ram[0x042C + 9] = 52;
        assert_eq!(
            trace_line(18915, &ram),
            "f18915 stage=01 phase=80 scroll=00:0D:00 score=2760 \
             rank=5(w1 o2 s1 st1) s17=05 boss=98:163:121:0:0:17:5:52\n"
        );
    }
}
