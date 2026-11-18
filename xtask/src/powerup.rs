//! Render the six power-ups from the asset pack for the visual proof.

use std::path::Path;

use anyhow::Result;
use gradius_assets::Pack;
use gradius_core::{font, Buttons, Framebuffer, Game, HEIGHT, WIDTH};

use crate::pngout;

/// Frames the proof holds fire after activating the power-up.
const FIRE_FRAMES: usize = 90;

struct Scenario {
    name: &'static str,
    slot: u8,
    steer_right: bool,
    steer_up: bool,
    /// Fire frames to hold before the capture; `FIRE_FRAMES` is the default.
    capture: usize,
}

const SCENARIOS: [Scenario; 6] = [
    Scenario {
        name: "speed",
        slot: 1,
        steer_right: true,
        steer_up: false,
        capture: 60,
    },
    Scenario {
        name: "missile",
        slot: 2,
        steer_right: false,
        steer_up: false,
        capture: 70,
    },
    Scenario {
        name: "double",
        slot: 3,
        steer_right: false,
        steer_up: false,
        capture: 12,
    },
    Scenario {
        name: "laser",
        slot: 4,
        steer_right: false,
        steer_up: false,
        capture: 70,
    },
    Scenario {
        name: "option",
        slot: 5,
        steer_right: false,
        steer_up: true,
        capture: 60,
    },
    Scenario {
        name: "shield",
        slot: 6,
        steer_right: false,
        steer_up: false,
        capture: FIRE_FRAMES,
    },
];

/// Assert the held-B activation actually applied the scenario's item.
fn assert_effect(game: &Game, slot: u8) {
    assert_eq!(game.power().selection(), 0, "slot {slot} was not consumed");
    match slot {
        1 => assert!(game.power().speed_level() > 0, "SPEED did not apply"),
        2 => assert!(game.power().has_missile(), "MISSILE did not apply"),
        3 => assert!(game.power().has_double(), "DOUBLE did not apply"),
        4 => assert!(game.power().has_laser(), "LASER did not apply"),
        5 => assert!(game.power().options() > 0, "OPTION did not apply"),
        6 => assert_eq!(
            game.power().shield_hits(),
            gradius_core::powerup::SHIELD_HITS,
            "SHIELD did not apply"
        ),
        _ => unreachable!("scenario slot {slot}"),
    }
}

pub fn run(pack_path: &Path, out: &Path) -> Result<()> {
    let pack = Pack::load(pack_path)?;
    std::fs::create_dir_all(out)?;

    let mut cells: Vec<(String, Vec<u8>)> = Vec::new();
    for scenario in SCENARIOS {
        let mut game = Game::new(pack.clone().into_assets());
        game.dev_start_run();
        game.dev_power_mut().select(scenario.slot);
        // `$8979` reads held B, not an edge: latch the button on one frame and
        // keep it down through activation and the fire window.
        game.step(Buttons {
            power: true,
            ..Buttons::NONE
        });
        let mut captured = None;
        for frame in 0..FIRE_FRAMES {
            game.step(Buttons {
                fire: true,
                power: true,
                right: scenario.steer_right,
                up: scenario.steer_up,
                ..Buttons::NONE
            });
            if frame + 1 == scenario.capture {
                captured = Some(game.framebuffer().as_rgba().to_vec());
            }
        }

        assert_effect(&game, scenario.slot);

        let rgba = captured.unwrap_or_else(|| game.framebuffer().as_rgba().to_vec());
        let rgb = to_rgb(&rgba);
        let file = out.join(format!("power-{}.png", scenario.name));
        pngout::write_rgb(&file, WIDTH as u32, HEIGHT as u32, &rgb)?;
        println!("wrote {}", file.display());

        let label = format!(
            "{} SPEED={} BULLETS={} OPTIONS={} SHIELD={}",
            scenario.name.to_uppercase(),
            game.power().speed_level(),
            game.dev_bullet_count(),
            game.power().options(),
            game.power().shield_hits(),
        );
        cells.push((label, rgba));
    }

    let sheet = contact_sheet(&cells);
    let file = out.join("power-contact-sheet.png");
    pngout::write_rgb(&file, sheet.0, sheet.1, &sheet.2)?;
    println!("wrote {}", file.display());
    Ok(())
}

fn to_rgb(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

const MARGIN: usize = 6;
const LABEL_H: usize = 12;
const COLS: usize = 3;
const ROWS: usize = 2;

/// Compose the six 256x240 frames into a labelled contact sheet.
fn contact_sheet(cells: &[(String, Vec<u8>)]) -> (u32, u32, Vec<u8>) {
    let sheet_w = MARGIN + COLS * (WIDTH + MARGIN);
    let sheet_h = MARGIN + ROWS * (HEIGHT + LABEL_H + MARGIN);
    let mut sheet = vec![0u8; sheet_w * sheet_h * 3];

    for (index, (label, rgba)) in cells.iter().enumerate() {
        let x0 = MARGIN + (index % COLS) * (WIDTH + MARGIN);
        let y0 = MARGIN + (index / COLS) * (HEIGHT + LABEL_H + MARGIN);

        let mut text = Framebuffer::new();
        text.clear([0, 0, 0]);
        font::draw_text(&mut text, 2, 3, label, [0xFF, 0xFF, 0xFF]);
        blit(&mut sheet, sheet_w, x0, y0, WIDTH, LABEL_H, text.as_rgba());
        blit(&mut sheet, sheet_w, x0, y0 + LABEL_H, WIDTH, HEIGHT, rgba);
    }
    (sheet_w as u32, sheet_h as u32, sheet)
}

fn blit(
    sheet: &mut [u8],
    sheet_w: usize,
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
    rgba: &[u8],
) {
    for y in 0..height {
        for x in 0..width {
            let source = (y * width + x) * 4;
            let target = ((y0 + y) * sheet_w + x0 + x) * 3;
            sheet[target..target + 3].copy_from_slice(&rgba[source..source + 3]);
        }
    }
}
