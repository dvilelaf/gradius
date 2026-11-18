//! Frontend overlays drawn over the game, never by the core.
//!
//! They go into a [`Layer`] at the window's resolution (see `text`), so the
//! core framebuffer stays untouched and the text stays sharp.

use gradius_core::{PowerMeter, PowerUp};

use crate::text::{Color, Layer};

const WHITE: Color = [255, 255, 255, 255];
const YELLOW: Color = [255, 224, 64, 255];
const SHADE: Color = [0, 0, 0, 170];

/// Dim a horizontal band across the middle and write `PAUSED` on it.
pub fn draw_paused(layer: &mut Layer) {
    layer.band(96.0, 48.0, SHADE);
    layer.text_centered(108.0, "PAUSED", 20.0, true, WHITE);
}

/// Write the debug lines over the top of the frame; `flash` is yellow.
pub fn draw_debug(layer: &mut Layer, lines: &[String], flash: Option<&str>) {
    let count = lines.len() + usize::from(flash.is_some());
    if count == 0 {
        return;
    }
    layer.band(0.0, count as f32 * 10.0 + 4.0, SHADE);
    let mut y = 2.0;
    for line in lines {
        layer.text(4.0, y, line, 7.5, false, WHITE);
        y += 10.0;
    }
    if let Some(flash) = flash {
        layer.text(4.0, y, flash, 7.5, true, YELLOW);
    }
}

/// Write a one-line notice (savestate or volume feedback) over the bottom of
/// the frame, leaving the top of the screen and the pause band untouched.
pub fn draw_notice(layer: &mut Layer, text: &str) {
    layer.band(222.0, 18.0, SHADE);
    layer.text(6.0, 225.0, text, 8.5, false, WHITE);
}

/// The meter, weapon and flag lines the debug overlay shows.
pub fn debug_lines(power: &PowerMeter) -> Vec<String> {
    let selected = power.selected().map_or("NONE", PowerUp::label);
    let mut flags: Vec<&str> = Vec::new();
    if power.speed_level() > 0 {
        flags.push("SPD");
    }
    if power.has_missile() {
        flags.push("MSL");
    }
    match power.weapon_level() {
        1 => flags.push("LASER"),
        2 => flags.push("DOUBLE"),
        _ => {}
    }
    if power.options() > 0 {
        flags.push("OPT");
    }
    if power.shield_hits() > 0 {
        flags.push("SHD");
    }
    let flags = if flags.is_empty() {
        "NONE".to_string()
    } else {
        flags.join(" ")
    };
    vec![
        format!(
            "CAP {:03} SEL {} SPD {}",
            power.capsules(),
            power.selection(),
            power.speed_level()
        ),
        format!(
            "WPN {} OPT {} SHD {}",
            weapon_label(power.weapon_level()),
            power.options(),
            power.shield_hits()
        ),
        format!("SELECTED: {selected}"),
        format!("FLAGS {flags}"),
    ]
}

/// One-line feedback for a B/power press. `applied` is false for a held item
/// that the meter refused, which still proves the button reached the core.
pub fn power_flash(selected: Option<PowerUp>, applied: bool) -> String {
    match selected {
        None => "POWER B: NO SELECTION".to_string(),
        Some(power) if applied => format!("POWER B: {}", power.label()),
        Some(power) => format!("POWER B: {} NO CHANGE", power.label()),
    }
}

fn weapon_label(level: u8) -> &'static str {
    match level {
        1 => "LASER",
        2 => "DOUBLE",
        _ => "NORMAL",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(layer: &Layer, from_row: usize, rows: usize) -> usize {
        let row = layer.width as usize * 4;
        layer.rgba[from_row * row..(from_row + rows) * row]
            .chunks(4)
            .filter(|pixel| pixel[3] > 0)
            .count()
    }

    #[test]
    fn paused_dims_a_band_across_the_middle_and_writes_on_it() {
        let mut layer = Layer::new(768, 720);
        draw_paused(&mut layer);
        assert_eq!(lit(&layer, 0, 100), 0, "above the band stays clear");
        assert_eq!(lit(&layer, 620, 100), 0, "below the band stays clear");
        let middle = (360 * 768 + 10) * 4;
        assert_eq!(layer.rgba[middle + 3], SHADE[3]);
        let white = layer
            .rgba
            .chunks(4)
            .filter(|p| p[..3] == [255; 3] && p[3] == 255)
            .count();
        assert!(white > 300, "only {white} letter pixels");
    }

    #[test]
    fn the_debug_band_sits_on_top_with_a_yellow_flash() {
        let mut layer = Layer::new(768, 720);
        let lines = vec!["CAP 003 SEL 2 SPD 1".to_string(), "FLAGS SPD".to_string()];
        draw_debug(&mut layer, &lines, Some("POWER B: MISSILE"));
        assert!(lit(&layer, 0, 100) > 0);
        assert_eq!(lit(&layer, 300, 100), 0);
        let yellow = layer
            .rgba
            .chunks(4)
            .filter(|p| p[..3] == [255, 224, 64] && p[3] == 255)
            .count();
        assert!(yellow > 50, "only {yellow} flash pixels");
    }

    #[test]
    fn the_notice_sits_at_the_bottom_and_leaves_the_top_alone() {
        let mut layer = Layer::new(768, 720);
        draw_notice(&mut layer, "Saved");
        assert_eq!(lit(&layer, 0, 600), 0);
        assert!(lit(&layer, 670, 50) > 100);
    }

    #[test]
    fn empty_input_draws_nothing() {
        let mut layer = Layer::new(768, 720);
        draw_debug(&mut layer, &[], None);
        assert_eq!(lit(&layer, 0, 720), 0);
        draw_paused(&mut Layer::new(0, 0));
    }

    #[test]
    fn status_lines_name_the_selected_slot_and_every_flag() {
        let mut meter = PowerMeter::default();
        meter.advance();
        meter.advance();
        meter.select(1);
        assert_eq!(meter.activate(false), Some(PowerUp::SpeedUp));
        meter.select(2);
        assert_eq!(meter.activate(false), Some(PowerUp::Missile));
        meter.select(3);
        assert_eq!(meter.activate(false), Some(PowerUp::Double));
        meter.select(5);
        assert_eq!(meter.activate(false), Some(PowerUp::Option));
        meter.select(6);
        assert_eq!(meter.activate(false), Some(PowerUp::Shield));
        meter.select(4);

        let lines = debug_lines(&meter);
        assert_eq!(lines[0], "CAP 002 SEL 4 SPD 1");
        assert_eq!(lines[1], "WPN DOUBLE OPT 1 SHD 5");
        assert_eq!(lines[2], "SELECTED: LASER");
        assert_eq!(lines[3], "FLAGS SPD MSL DOUBLE OPT SHD");
    }

    #[test]
    fn weapon_names_cover_normal_double_and_laser() {
        assert_eq!(weapon_label(0), "NORMAL");
        assert_eq!(weapon_label(1), "LASER");
        assert_eq!(weapon_label(2), "DOUBLE");
    }

    #[test]
    fn selection_zero_and_no_flags_are_spelled_out() {
        let meter = PowerMeter::default();
        let lines = debug_lines(&meter);
        assert_eq!(lines[0], "CAP 000 SEL 0 SPD 0");
        assert_eq!(lines[2], "SELECTED: NONE");
        assert_eq!(lines[3], "FLAGS NONE");
    }

    #[test]
    fn power_flash_uses_the_selected_item_name() {
        assert_eq!(
            power_flash(None, false),
            "POWER B: NO SELECTION".to_string()
        );
        assert_eq!(
            power_flash(Some(PowerUp::Double), true),
            "POWER B: DOUBLE".to_string()
        );
        assert_eq!(
            power_flash(Some(PowerUp::Double), false),
            "POWER B: DOUBLE NO CHANGE".to_string()
        );
    }
}
