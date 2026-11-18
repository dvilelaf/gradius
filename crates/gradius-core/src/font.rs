//! A tiny built-in 3x5 font for the HUD and the stage banner.
//!
//! The HUD does not need the cartridge font: keeping these glyphs in code
//! avoids depending on a specific tile bank and keeps scaling readable. Text
//! is drawn with one blank column between glyphs; [`draw_text_scaled`] doubles
//! or triples it for banners without changing the layout code.

use crate::video::{Framebuffer, WIDTH};

/// Glyphs are five rows of three bits, most significant bit on the left.
fn glyph(character: char) -> [u8; 5] {
    match character.to_ascii_uppercase() {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b101, 0b111, 0b111, 0b111, 0b101],
        'O' => [0b010, 0b101, 0b101, 0b101, 0b010],
        'P' => [0b110, 0b101, 0b110, 0b100, 0b100],
        'Q' => [0b010, 0b101, 0b101, 0b110, 0b011],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        ' ' => [0, 0, 0, 0, 0],
        '-' => [0, 0, 0b111, 0, 0],
        '.' => [0, 0, 0, 0, 0b010],
        ':' => [0, 0b010, 0, 0b010, 0],
        '!' => [0b010, 0b010, 0b010, 0, 0b010],
        '/' => [0b001, 0b001, 0b010, 0b100, 0b100],
        '>' => [0b100, 0b010, 0b001, 0b010, 0b100],
        '<' => [0b001, 0b010, 0b100, 0b010, 0b001],
        '+' => [0, 0b010, 0b111, 0b010, 0],
        '=' => [0, 0b111, 0, 0b111, 0],
        '?' => [0b111, 0b001, 0b011, 0, 0b010],
        '*' => [0b101, 0b010, 0b101, 0, 0],
        '(' => [0b001, 0b010, 0b010, 0b010, 0b001],
        ')' => [0b100, 0b010, 0b010, 0b010, 0b100],
        _ => [0b111, 0b101, 0b101, 0b101, 0b111],
    }
}

/// Width in pixels of a string at scale 1.
pub fn text_width(text: &str) -> i32 {
    match text.chars().count() {
        0 => 0,
        count => count as i32 * 4 - 1,
    }
}

/// Draw a short string with one pixel of spacing between characters.
pub fn draw_text(fb: &mut Framebuffer, x: i32, y: i32, text: &str, rgb: [u8; 3]) {
    draw_text_scaled(fb, x, y, text, 1, rgb);
}

/// Draw a string with square pixels of `scale` size.
pub fn draw_text_scaled(
    fb: &mut Framebuffer,
    mut x: i32,
    y: i32,
    text: &str,
    scale: i32,
    rgb: [u8; 3],
) {
    let scale = scale.max(1);
    for character in text.chars() {
        let rows = glyph(character);
        for (row_index, row) in rows.iter().enumerate() {
            for col in 0..3 {
                if row & (0b100 >> col) != 0 {
                    let px = x + col * scale;
                    let py = y + row_index as i32 * scale;
                    fb.rect(px, py, scale, scale, rgb);
                }
            }
        }
        x += 4 * scale;
    }
}

/// Draw a string horizontally centred on the 256-pixel screen.
pub fn draw_text_centered(fb: &mut Framebuffer, y: i32, text: &str, scale: i32, rgb: [u8; 3]) {
    let scale = scale.max(1);
    let width = text_width(text) * scale;
    draw_text_scaled(fb, (WIDTH as i32 - width) / 2, y, text, scale, rgb);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_characters_do_not_panic() {
        let mut fb = Framebuffer::new();
        draw_text(&mut fb, 0, 0, "¡?", [255, 255, 255]);
    }

    #[test]
    fn width_counts_the_spacing_between_glyphs() {
        assert_eq!(text_width(""), 0);
        assert_eq!(text_width("A"), 3);
        assert_eq!(text_width("AB"), 7);
        assert_eq!(text_width("STAGE 1"), 27);
    }

    #[test]
    fn scaled_text_is_wider_and_taller() {
        let mut fb = Framebuffer::new();
        draw_text_scaled(&mut fb, 0, 0, "I", 2, [1, 2, 3]);
        // The top row of `I` is three pixels wide at scale 2.
        assert_eq!(&fb.as_rgba()[..4], &[1, 2, 3, 0xFF]);
        assert_eq!(&fb.as_rgba()[4..8], &[1, 2, 3, 0xFF]);
        assert_eq!(&fb.as_rgba()[8..12], &[1, 2, 3, 0xFF]);
    }

    #[test]
    fn centered_text_stays_on_screen() {
        let mut fb = Framebuffer::new();
        draw_text_centered(&mut fb, 100, "GAME OVER", 2, [1, 2, 3]);
        assert!(fb.as_rgba().chunks_exact(4).any(|pixel| pixel[0] != 0));
    }
}
