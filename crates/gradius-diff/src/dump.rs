//! The shared state line and its parsers.
//!
//! The format is the one documented by [`gradius_core::Game::dump_state`]:
//!
//! ```text
//! frame=N state=S px=N pf=N py=N pyf=N speed=N scroll=LOW:HIGH:FRAC
//! score=N lives=N capsule=N meter=N objects=SLOT:KIND:X:Y:SUB;... [nmi=N] [mode=N]
//! ```
//!
//! The native side is parsed straight out of `dump_state()`, so the harness
//! stays honest about what the port actually exposes. The oracle side is built
//! from [`gradius_nes::PlayerState`], [`gradius_nes::ObjectState`] and the RAM
//! bytes listed in the crate docs.
//!
//! `nmi` (`$02`, the NMI frame counter) and `mode` (`$03`) are appended after
//! `objects`, as the format's append-only rule requires. The oracle always
//! writes them; a line without them (every fixture recorded before the fields
//! existed, and a native `dump_state()` line) parses with both as `None`.
//! They are measurement data: the comparator ignores them.

use gradius_nes::Emulator;

/// One live object from a dumped state line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotObject {
    pub slot: i64,
    /// `$030C`.
    pub kind: i64,
    /// X integer pixel.
    pub x: i64,
    /// Y integer pixel.
    pub y: i64,
    /// `$048C` on both sides.
    pub substate: i64,
}

/// A parsed `frame=... objects=...` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub frame: u64,
    pub state: String,
    pub px: i64,
    pub pf: i64,
    pub py: i64,
    pub pyf: i64,
    pub speed: i64,
    pub scroll_low: i64,
    pub scroll_high: i64,
    pub scroll_frac: i64,
    pub score: u64,
    pub lives: i64,
    pub capsule: i64,
    pub meter: i64,
    pub objects: Vec<SnapshotObject>,
    /// `$02`: the NMI frame counter, when the line carries it.
    pub nmi: Option<i64>,
    /// `$03`: the mode byte, when the line carries it.
    pub mode: Option<i64>,
}

impl Snapshot {
    /// Format the line exactly as [`gradius_core::Game::dump_state`] does.
    pub fn to_line(&self) -> String {
        let mut line = format!(
            "frame={} state={} px={} pf={} py={} pyf={} speed={} scroll={}:{}:{} score={} lives={} capsule={} meter={} objects=",
            self.frame,
            self.state,
            self.px,
            self.pf,
            self.py,
            self.pyf,
            self.speed,
            self.scroll_low,
            self.scroll_high,
            self.scroll_frac,
            self.score,
            self.lives,
            self.capsule,
            self.meter,
        );
        for (index, object) in self.objects.iter().enumerate() {
            if index > 0 {
                line.push(';');
            }
            line.push_str(&format!(
                "{}:{}:{}:{}:{}",
                object.slot, object.kind, object.x, object.y, object.substate
            ));
        }
        if let Some(nmi) = self.nmi {
            line.push_str(&format!(" nmi={nmi}"));
        }
        if let Some(mode) = self.mode {
            line.push_str(&format!(" mode={mode}"));
        }
        line
    }

    /// Parse one state line.
    pub fn parse(line: &str) -> Result<Self, String> {
        let mut frame = None;
        let mut state = None;
        let mut px = None;
        let mut pf = None;
        let mut py = None;
        let mut pyf = None;
        let mut speed = None;
        let mut scroll = None;
        let mut score = None;
        let mut lives = None;
        let mut capsule = None;
        let mut meter = None;
        let mut objects = None;
        let mut nmi = None;
        let mut mode = None;
        for field in line.split_whitespace() {
            let (key, value) = field
                .split_once('=')
                .ok_or_else(|| format!("field `{field}` is not KEY=VALUE"))?;
            match key {
                "frame" => frame = Some(parse_u64(key, value)?),
                "state" => state = Some(value.to_string()),
                "px" => px = Some(parse_i64(key, value)?),
                "pf" => pf = Some(parse_i64(key, value)?),
                "py" => py = Some(parse_i64(key, value)?),
                "pyf" => pyf = Some(parse_i64(key, value)?),
                "speed" => speed = Some(parse_i64(key, value)?),
                "scroll" => scroll = Some(parse_scroll(value)?),
                "score" => score = Some(parse_u64(key, value)?),
                "lives" => lives = Some(parse_i64(key, value)?),
                "capsule" => capsule = Some(parse_i64(key, value)?),
                "meter" => meter = Some(parse_i64(key, value)?),
                "objects" => objects = Some(parse_objects(value)?),
                "nmi" => nmi = Some(parse_i64(key, value)?),
                "mode" => mode = Some(parse_i64(key, value)?),
                other => return Err(format!("unknown field `{other}`")),
            }
        }
        let (scroll_low, scroll_high, scroll_frac) =
            scroll.ok_or_else(|| "missing field `scroll`".to_string())?;
        Ok(Self {
            frame: frame.ok_or_else(|| "missing field `frame`".to_string())?,
            state: state.ok_or_else(|| "missing field `state`".to_string())?,
            px: px.ok_or_else(|| "missing field `px`".to_string())?,
            pf: pf.ok_or_else(|| "missing field `pf`".to_string())?,
            py: py.ok_or_else(|| "missing field `py`".to_string())?,
            pyf: pyf.ok_or_else(|| "missing field `pyf`".to_string())?,
            speed: speed.ok_or_else(|| "missing field `speed`".to_string())?,
            scroll_low,
            scroll_high,
            scroll_frac,
            score: score.ok_or_else(|| "missing field `score`".to_string())?,
            lives: lives.ok_or_else(|| "missing field `lives`".to_string())?,
            capsule: capsule.ok_or_else(|| "missing field `capsule`".to_string())?,
            meter: meter.ok_or_else(|| "missing field `meter`".to_string())?,
            objects: objects.ok_or_else(|| "missing field `objects`".to_string())?,
            nmi,
            mode,
        })
    }

    /// The ship's X position as one 8.8 fixed-point number.
    pub fn x_fixed(&self) -> i64 {
        self.px * 256 + self.pf
    }

    /// The ship's Y position as one 8.8 fixed-point number.
    pub fn y_fixed(&self) -> i64 {
        self.py * 256 + self.pyf
    }
}

fn parse_u64(key: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("bad `{key}` value `{value}`"))
}

fn parse_i64(key: &str, value: &str) -> Result<i64, String> {
    value
        .parse()
        .map_err(|_| format!("bad `{key}` value `{value}`"))
}

fn parse_scroll(value: &str) -> Result<(i64, i64, i64), String> {
    let parts: Vec<&str> = value.split(':').collect();
    let [low, high, frac] = parts.as_slice() else {
        return Err(format!("bad scroll `{value}` (expected LOW:HIGH:FRAC)"));
    };
    Ok((
        parse_i64("scroll", low)?,
        parse_i64("scroll", high)?,
        parse_i64("scroll", frac)?,
    ))
}

fn parse_objects(value: &str) -> Result<Vec<SnapshotObject>, String> {
    let mut objects = Vec::new();
    if value.is_empty() {
        return Ok(objects);
    }
    for entry in value.split(';') {
        let parts: Vec<&str> = entry.split(':').collect();
        let [slot, kind, x, y, substate] = parts.as_slice() else {
            return Err(format!("bad object `{entry}` (expected SLOT:KIND:X:Y:SUB)"));
        };
        objects.push(SnapshotObject {
            slot: parse_i64("object slot", slot)?,
            kind: parse_i64("object kind", kind)?,
            x: parse_i64("object x", x)?,
            y: parse_i64("object y", y)?,
            substate: parse_i64("object substate", substate)?,
        });
    }
    Ok(objects)
}

/// Decode the oracle's game state from `$00` and the stage state machine `$1B`.
///
/// The port reports the documented `$1B` init values 1–4 as `Other(4)`, so the
/// oracle does the same, `$1B == 0` included: that is the frame the stage
/// handler is entered but has not run yet. Without this the window would open
/// before the ROM places the ship and the unset `$0360`/`$0320` would count as
/// a position divergence. `$80` (normal play), `$81` (transition) and `$C0`
/// (death countdown) all count as `Playing`, matching what the port reports.
fn oracle_state(ram: &[u8; 2048]) -> String {
    match ram[0x00] {
        0 => "Title".to_string(),
        5 if ram[0x1B] <= 4 => "Other(4)".to_string(),
        5 => "Playing".to_string(),
        other => format!("Other({other})"),
    }
}

/// Analysis only, matching `xtask record-boss`: restore the player-death bytes
/// so a scripted run can survive to the stage's core.
pub fn keep_alive_poke(emulator: &mut Emulator) {
    emulator.bus.ram[0x46] = 5;
    if emulator.bus.ram[0x0100] >= 2 {
        emulator.bus.ram[0x0100] = 1;
        emulator.bus.ram[0x4C] = 0;
    }
    if emulator.bus.ram[0x1B] == 0xA0 {
        emulator.bus.ram[0x1B] = 0x80;
    }
}

/// Build the oracle's state line from the emulator's public views.
///
/// See the crate docs for every field the oracle cannot supply.
pub fn oracle_snapshot(emulator: &Emulator) -> Snapshot {
    let player = emulator.player_state();
    let ram = emulator.ram_snapshot();
    let objects = emulator
        .objects()
        .into_iter()
        // Slot 0 is the ship in the original's parallel arrays; the port keeps
        // the ship outside its object table, so both sides compare it through
        // the top-level position fields. Slots above 9 are not allocated by
        // `$A3AB` (`LDX #$09`): beyond that the byte ranges overlap the
        // position arrays and `objects()` reads unrelated state.
        .filter(|object| (1..=9).contains(&object.slot) && object.kind != 0)
        .map(|object| SnapshotObject {
            slot: i64::from(object.slot),
            kind: i64::from(object.kind),
            x: i64::from(object.x_int),
            y: i64::from(object.y_int),
            // The native dump's fifth field is `$048C`; `ObjectState::substate`
            // is `$03BC`, so read the matching byte from RAM.
            substate: i64::from(ram[0x048C + usize::from(object.slot)]),
        })
        .collect();
    Snapshot {
        frame: emulator.frames,
        state: oracle_state(&ram),
        px: i64::from(player.x_int),
        pf: i64::from(player.x_frac),
        py: i64::from(player.y_int),
        pyf: i64::from(player.y_frac),
        speed: i64::from(player.speed_level),
        scroll_low: i64::from(player.scroll & 0xFF),
        scroll_high: i64::from(player.scroll >> 8),
        // `PlayerState` does not expose `$3D`, so read it from RAM.
        scroll_frac: i64::from(ram[0x3D]),
        score: u64::from(player.score()),
        lives: i64::from(player.lives),
        // The original keeps no capsule counter: `$42` is the six-slot meter
        // selection, which advances one step per pickup and resets on use.
        capsule: 0,
        meter: i64::from(ram[0x42]),
        objects,
        // `$02`/`$03` are zero-page bytes the port also models; the harness
        // records them for the `$02`-phase analysis (see
        // `docs/analysis/stage7_triage.md`).
        nmi: Some(i64::from(ram[0x02])),
        mode: Some(i64::from(ram[0x03])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATIVE: &str = "frame=42 state=Playing px=80 pf=0 py=96 pyf=0 speed=0 scroll=0:0:0 score=0 lives=3 capsule=0 meter=0 objects=9:133:64:80:3;8:5:80:80:0";

    #[test]
    fn parses_a_native_line() {
        let snapshot = Snapshot::parse(NATIVE).unwrap();
        assert_eq!(snapshot.frame, 42);
        assert_eq!(snapshot.state, "Playing");
        assert_eq!(snapshot.px, 80);
        assert_eq!(snapshot.py, 96);
        assert_eq!(snapshot.lives, 3);
        assert_eq!(snapshot.objects.len(), 2);
        assert_eq!(
            snapshot.objects[0],
            SnapshotObject {
                slot: 9,
                kind: 133,
                x: 64,
                y: 80,
                substate: 3
            }
        );
        assert_eq!(snapshot.objects[1].slot, 8);
    }

    #[test]
    fn empty_object_list_round_trips() {
        let line = "frame=0 state=Title px=80 pf=0 py=96 pyf=0 speed=0 scroll=0:0:0 score=0 lives=3 capsule=0 meter=0 objects=";
        let snapshot = Snapshot::parse(line).unwrap();
        assert!(snapshot.objects.is_empty());
        assert_eq!(snapshot.to_line(), line);
    }

    #[test]
    fn nmi_and_mode_are_trailing_and_optional() {
        let line = format!("{NATIVE} nmi=57 mode=1");
        let snapshot = Snapshot::parse(&line).unwrap();
        assert_eq!(snapshot.nmi, Some(57));
        assert_eq!(snapshot.mode, Some(1));
        assert_eq!(snapshot.objects.len(), 2);
        assert_eq!(snapshot.to_line(), line);
        // A fixture recorded before the fields existed loads with them unset
        // and round-trips byte for byte.
        let old = Snapshot::parse(NATIVE).unwrap();
        assert_eq!((old.nmi, old.mode), (None, None));
        assert_eq!(old.to_line(), NATIVE);
    }

    #[test]
    fn parsed_line_round_trips_through_the_formatter() {
        let snapshot = Snapshot::parse(NATIVE).unwrap();
        assert_eq!(snapshot.to_line(), NATIVE);
    }

    #[test]
    fn fixed_positions_combine_the_sub_pixel_byte() {
        let snapshot = Snapshot::parse(NATIVE).unwrap();
        assert_eq!(snapshot.x_fixed(), 80 * 256);
        assert_eq!(snapshot.y_fixed(), 96 * 256);
    }

    #[test]
    fn state_names_come_from_ram_zero_and_the_stage_state_machine() {
        let mut ram = [0u8; 2048];
        assert_eq!(oracle_state(&ram), "Title");
        ram[0x00] = 5;
        ram[0x1B] = 0;
        assert_eq!(oracle_state(&ram), "Other(4)");
        ram[0x1B] = 4;
        assert_eq!(oracle_state(&ram), "Other(4)");
        ram[0x1B] = 0x80;
        assert_eq!(oracle_state(&ram), "Playing");
        ram[0x1B] = 0xC0;
        assert_eq!(oracle_state(&ram), "Playing");
        ram[0x00] = 3;
        assert_eq!(oracle_state(&ram), "Other(3)");
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(Snapshot::parse("garbage")
            .unwrap_err()
            .contains("KEY=VALUE"));
        assert!(Snapshot::parse("frame=1").unwrap_err().contains("missing"));
        let line = NATIVE.replace("scroll=0:0:0", "scroll=0:0");
        assert!(Snapshot::parse(&line)
            .unwrap_err()
            .contains("expected LOW:HIGH:FRAC"));
        let line = NATIVE.replace("9:133:64:80:3", "9:133:64");
        assert!(Snapshot::parse(&line)
            .unwrap_err()
            .contains("expected SLOT"));
        let line = NATIVE.replace("frame=42", "frame=x");
        assert!(Snapshot::parse(&line).unwrap_err().contains("bad `frame`"));
    }
}
