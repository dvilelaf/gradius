//! Interface text: Inter, rasterized in pure Rust at the window's resolution.
//!
//! The frontend's own screens (options menu, first-run setup, pause, notices)
//! draw into a [`Layer`], an RGBA buffer as big as the output with straight
//! alpha. The [`Presenter`] uploads it as a blended texture over the game, so
//! the text is sharp at any window size instead of scaled up from 256x240.
//! Coordinates are in "units": the output is 240 units tall, so a layout
//! holds its proportions from a small window to full screen.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use gradius_app::HEIGHT;
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use std::sync::OnceLock;

pub type Color = [u8; 4];

static REGULAR: &[u8] = include_bytes!("../fonts/Inter-Regular.otf");
static SEMIBOLD: &[u8] = include_bytes!("../fonts/Inter-SemiBold.otf");

fn font(bold: bool) -> &'static FontRef<'static> {
    static FONTS: OnceLock<[FontRef<'static>; 2]> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        [REGULAR, SEMIBOLD].map(|bytes| FontRef::try_from_slice(bytes).expect("embedded font"))
    });
    &fonts[usize::from(bold)]
}

/// A transparent RGBA canvas the size of the output.
pub struct Layer {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Layer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    pub fn clear(&mut self) {
        self.rgba.fill(0);
    }

    /// Pixels per unit.
    pub fn unit(&self) -> f32 {
        self.height.max(1) as f32 / HEIGHT as f32
    }

    /// Width in units.
    pub fn units_wide(&self) -> f32 {
        self.width as f32 / self.unit()
    }

    /// Blend `color` (its alpha is the coverage) over one pixel.
    fn blend(&mut self, x: i64, y: i64, color: Color, coverage: f32) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return;
        }
        let at = ((y as usize) * self.width as usize + x as usize) * 4;
        let pixel = &mut self.rgba[at..at + 4];
        let source = f32::from(color[3]) / 255.0 * coverage.clamp(0.0, 1.0);
        let below = f32::from(pixel[3]) / 255.0;
        let alpha = source + below * (1.0 - source);
        if alpha <= 0.0 {
            return;
        }
        for channel in 0..3 {
            let mixed = (f32::from(color[channel]) * source
                + f32::from(pixel[channel]) * below * (1.0 - source))
                / alpha;
            pixel[channel] = mixed.round() as u8;
        }
        pixel[3] = (alpha * 255.0).round() as u8;
    }

    /// Fill a rectangle given in units.
    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        let unit = self.unit();
        let (x0, y0) = ((x * unit).round() as i64, (y * unit).round() as i64);
        let (x1, y1) = (
            ((x + width) * unit).round() as i64,
            ((y + height) * unit).round() as i64,
        );
        for py in y0.max(0)..y1.min(i64::from(self.height)) {
            for px in x0.max(0)..x1.min(i64::from(self.width)) {
                self.blend(px, py, color, 1.0);
            }
        }
    }

    /// Fill every pixel row from `y` for `height` units, across the output.
    pub fn band(&mut self, y: f32, height: f32, color: Color) {
        self.rect(0.0, y, self.units_wide(), height, color);
    }

    /// Width of `text` in units at `size` units tall.
    pub fn measure(&self, text: &str, size: f32, bold: bool) -> f32 {
        let font = font(bold);
        let scaled = font.as_scaled(scale(font, size * self.unit()));
        let mut width = 0.0;
        let mut previous = None;
        for character in text.chars() {
            let id = scaled.glyph_id(character);
            if let Some(previous) = previous {
                width += scaled.kern(previous, id);
            }
            width += scaled.h_advance(id);
            previous = Some(id);
        }
        width / self.unit()
    }

    /// Draw `text` with its line box starting at (`x`, `y`) units; the box is
    /// `size * 1.3` tall.
    pub fn text(&mut self, x: f32, y: f32, text: &str, size: f32, bold: bool, color: Color) {
        let unit = self.unit();
        let font = font(bold);
        let px_scale = scale(font, size * unit);
        let scaled = font.as_scaled(px_scale);
        let baseline = y * unit + (size * 1.3 * unit - scaled.height()) / 2.0 + scaled.ascent();
        let mut caret = x * unit;
        let mut previous = None;
        for character in text.chars() {
            let id = scaled.glyph_id(character);
            if let Some(previous) = previous {
                caret += scaled.kern(previous, id);
            }
            let glyph = id.with_scale_and_position(px_scale, ab_glyph::point(caret, baseline));
            if let Some(outline) = font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    self.blend(
                        i64::from(bounds.min.x as i32) + i64::from(gx),
                        i64::from(bounds.min.y as i32) + i64::from(gy),
                        color,
                        coverage,
                    );
                });
            }
            caret += scaled.h_advance(id);
            previous = Some(id);
        }
    }

    /// [`Layer::text`] centered on the output.
    pub fn text_centered(&mut self, y: f32, text: &str, size: f32, bold: bool, color: Color) {
        let x = (self.units_wide() - self.measure(text, size, bold)) / 2.0;
        self.text(x, y, text, size, bold, color);
    }

    /// Greedy word wrap to `max_width` units; words wider than a line (the
    /// SHA-256) are cut.
    pub fn wrap(&self, text: &str, size: f32, bold: bool, max_width: f32) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        for word in text.split_whitespace() {
            let mut rest = word;
            while self.measure(rest, size, bold) > max_width && rest.chars().count() > 1 {
                let mut cut = rest.len();
                for (at, _) in rest.char_indices().skip(1) {
                    if self.measure(&rest[..at], size, bold) > max_width {
                        break;
                    }
                    cut = at;
                }
                if cut == rest.len() {
                    break;
                }
                lines.push(rest[..cut].to_string());
                rest = &rest[cut..];
            }
            let joined = lines.last().map(|line| format!("{line} {rest}"));
            match joined {
                Some(joined) if self.measure(&joined, size, bold) <= max_width => {
                    *lines.last_mut().unwrap() = joined;
                }
                _ => lines.push(rest.to_string()),
            }
        }
        lines
    }
}

fn scale(font: &FontRef<'_>, pixels: f32) -> PxScale {
    font.pt_to_px_scale(pixels).unwrap_or(PxScale::from(pixels))
}

/// Owns the [`Layer`] and its blended texture; both follow the output size.
pub struct Presenter<'a> {
    creator: &'a TextureCreator<WindowContext>,
    texture: Option<Texture<'a>>,
    layer: Layer,
}

impl<'a> Presenter<'a> {
    pub fn new(creator: &'a TextureCreator<WindowContext>) -> Self {
        Self {
            creator,
            texture: None,
            layer: Layer::new(0, 0),
        }
    }

    /// A cleared layer as big as the output.
    pub fn begin(&mut self, width: u32, height: u32) -> &mut Layer {
        if (self.layer.width, self.layer.height) == (width, height) {
            self.layer.clear();
        } else {
            self.layer = Layer::new(width, height);
            self.texture = None;
        }
        &mut self.layer
    }

    /// Composite the layer over what the canvas holds.
    pub fn show(&mut self, canvas: &mut Canvas<Window>) -> Result<(), String> {
        if self.layer.width == 0 || self.layer.height == 0 {
            return Ok(());
        }
        if self.texture.is_none() {
            let mut texture = self
                .creator
                .create_texture_streaming(
                    sdl2::pixels::PixelFormatEnum::RGBA32,
                    self.layer.width,
                    self.layer.height,
                )
                .map_err(|error| format!("cannot create overlay texture: {error}"))?;
            texture.set_blend_mode(BlendMode::Blend);
            self.texture = Some(texture);
        }
        let texture = self.texture.as_mut().expect("created above");
        texture
            .update(None, &self.layer.rgba, self.layer.width as usize * 4)
            .map_err(|error| format!("overlay upload failed: {error}"))?;
        canvas
            .copy(texture, None, None)
            .map_err(|error| format!("overlay present failed: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Color = [255, 255, 255, 255];

    fn lit(layer: &Layer) -> usize {
        layer.rgba.chunks(4).filter(|pixel| pixel[3] > 0).count()
    }

    #[test]
    fn text_is_antialiased_at_the_output_resolution() {
        let mut layer = Layer::new(1280, 960);
        layer.text(10.0, 10.0, "Options", 20.0, true, WHITE);
        assert!(lit(&layer) > 500, "only {} text pixels", lit(&layer));
        let partial = layer
            .rgba
            .chunks(4)
            .filter(|pixel| pixel[3] > 0 && pixel[3] < 255)
            .count();
        assert!(partial > 100, "no antialiasing: {partial} soft pixels");
        // Text stays on its own: nothing far from where it was drawn.
        assert_eq!(layer.rgba[(900 * 1280 * 4) as usize + 3], 0);
    }

    #[test]
    fn text_grows_with_the_window_height() {
        let small = Layer::new(256, 240);
        let big = Layer::new(1024, 960);
        assert!(
            (small.measure("Volume", 12.0, false) - big.measure("Volume", 12.0, false)).abs() < 1.0
        );
        assert!(small.measure("Volume", 12.0, true) > small.measure("Volume", 12.0, false));
    }

    #[test]
    fn blending_keeps_the_colour_of_a_dimmed_background() {
        let mut layer = Layer::new(64, 60);
        layer.band(0.0, 240.0, [0, 0, 0, 128]);
        layer.rect(0.0, 0.0, 10.0, 10.0, [255, 0, 0, 255]);
        assert_eq!(&layer.rgba[..4], &[255, 0, 0, 255]);
        let outside = (30 * 64 + 30) * 4;
        assert_eq!(&layer.rgba[outside..outside + 4], &[0, 0, 0, 128]);
    }

    #[test]
    fn long_messages_wrap_inside_the_width() {
        let layer = Layer::new(768, 720);
        let sha = "ab".repeat(32);
        let lines = layer.wrap(
            &format!("expected SHA-256 {sha} for the file"),
            10.0,
            false,
            200.0,
        );
        assert!(lines.len() > 1);
        assert!(lines
            .iter()
            .all(|line| layer.measure(line, 10.0, false) <= 200.0));
        assert!(lines.concat().replace(' ', "").contains(&sha));
    }

    #[test]
    fn drawing_off_the_edge_is_harmless() {
        let mut layer = Layer::new(32, 30);
        layer.text(-50.0, -5.0, "edge", 20.0, false, WHITE);
        layer.text(30.0, 28.0, "edge", 20.0, false, WHITE);
        layer.rect(-5.0, -5.0, 100.0, 100.0, WHITE);
    }
}
