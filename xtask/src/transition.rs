//! Stage-transition ledger.
//!
//! One line per frame with the bytes that change at a stage handoff:
//!
//! ```text
//! f09601 stage=01 phase=80 music=93 scroll=01:00:00 ship=22,98 \
//!        player=01 s39=00 s3a=00 objects=09:85
//! ```
//!
//! `stage` is `$19`, `phase` `$1B`, `music` `$1C`, `scroll` is
//! `$3E:$3F:$3D`, `ship` is `$0360,$0320`, `player` is `$0100`, and
//! `objects` lists `slot:$030C` for the live object slots 1-9. See
//! `docs/analysis/stage_transition.md`.

use anyhow::{bail, Context, Result};

use crate::oracle;
use crate::parse_buttons;

/// Parse an xtask/diff input script into a frame plan.
///
/// Lines are `FRAME:buttons[:duration]` (six frames by default); `#` starts a
/// comment and names may be joined with `,` or `+`.
pub fn parse_script(text: &str) -> Result<oracle::Plan> {
    let mut schedule = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        let (at, spec, duration) = match parts.as_slice() {
            [at, spec] => (*at, *spec, 6),
            [at, spec, duration] => (
                *at,
                *spec,
                duration
                    .parse()
                    .with_context(|| format!("bad duration on line {} of the script", index + 1))?,
            ),
            _ => bail!(
                "expected FRAME:buttons[:frames] on line {} of the script",
                index + 1
            ),
        };
        let at: u64 = at
            .parse()
            .with_context(|| format!("bad frame on line {} of the script", index + 1))?;
        schedule.push((at, parse_buttons(spec)?, duration));
    }
    schedule.sort_by_key(|(at, _, _)| *at);
    Ok(oracle::Plan { held: 0, schedule })
}

/// Render one frame of the ledger, including its trailing newline.
pub fn line(frame: u64, ram: &[u8]) -> String {
    let mut objects = String::new();
    for slot in 1..=9usize {
        let kind = ram[0x030C + slot];
        if kind != 0 {
            objects.push_str(&format!("{slot:02}:{kind:02X}"));
            objects.push(' ');
        }
    }
    format!(
        "f{frame:05} stage={:02X} phase={:02X} music={:02X} scroll={:02X}:{:02X}:{:02X} ship={},{} player={:02X} s39={:02X} s3a={:02X} objects={objects}\n",
        ram[0x19],
        ram[0x1B],
        ram[0x1C],
        ram[0x3E],
        ram[0x3F],
        ram[0x3D],
        ram[0x360],
        ram[0x320],
        ram[0x100],
        ram[0x39],
        ram[0x3A],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradius_nes::{BUTTON_A, BUTTON_DOWN, BUTTON_START, BUTTON_UP};

    #[test]
    fn ledger_line_reports_the_handoff_bytes() {
        let mut ram = vec![0u8; 0x800];
        ram[0x19] = 1;
        ram[0x1B] = 0x80;
        ram[0x1C] = 0x93;
        ram[0x3E] = 1;
        ram[0x360] = 22;
        ram[0x320] = 98;
        ram[0x100] = 1;
        ram[0x030C + 9] = 0x85;
        assert_eq!(
            line(9602, &ram),
            "f09602 stage=01 phase=80 music=93 scroll=01:00:00 ship=22,98 \
             player=01 s39=00 s3a=00 objects=09:85 \n"
        );
    }

    #[test]
    fn scripts_accept_plus_and_default_to_six_frames() {
        let plan =
            parse_script("# boss script\n180:a+start+up:6\n0:a+down\n\n180:a+up:3\n").unwrap();
        assert_eq!(
            plan.schedule,
            vec![
                (0, BUTTON_A | BUTTON_DOWN, 6),
                (180, BUTTON_A | BUTTON_START | BUTTON_UP, 6),
                (180, BUTTON_A | BUTTON_UP, 3),
            ]
        );
    }
}
