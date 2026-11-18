//! Presentation geometry: where the fixed 256x240 image lands in a window.
//!
//! Pure arithmetic, so the fit rules are unit-tested without SDL. The core
//! framebuffer never changes size; only this rectangle does.

use gradius_app::{HEIGHT, WIDTH};

const SOURCE_WIDTH: u32 = WIDTH as u32;
const SOURCE_HEIGHT: u32 = HEIGHT as u32;

/// How the 256x240 image is mapped into a drawable of another size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// Integer scale, centred. Never a fractional pixel, never distorted.
    #[default]
    Integer,
    /// Largest undistorted size that fits, centred. Fractional scale.
    Aspect,
    /// Fill the whole drawable, distorting the image if the aspect differs.
    Stretch,
}

impl Fit {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "integer" => Ok(Self::Integer),
            "aspect" => Ok(Self::Aspect),
            "stretch" => Ok(Self::Stretch),
            other => Err(format!(
                "invalid fit `{other}`: expected integer, aspect or stretch"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Aspect => "aspect",
            Self::Stretch => "stretch",
        }
    }
}

/// Destination rectangle inside the window's drawable, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    /// Scale actually applied to the source image, in both axes.
    pub fn scale(self) -> f64 {
        f64::from(self.width) / f64::from(SOURCE_WIDTH)
    }

    /// Whether integer scaling produced this viewport (always true for
    /// [`Fit::Integer`] on a non-degenerate window).
    #[cfg(test)]
    pub fn is_integer(self) -> bool {
        self.width % SOURCE_WIDTH == 0 && self.height % SOURCE_HEIGHT == 0
    }
}

/// The rectangle the 256x240 image occupies inside `window_width` x
/// `window_height`, centred with black bars on the unused axis.
pub fn viewport(fit: Fit, window_width: u32, window_height: u32) -> Viewport {
    if window_width == 0 || window_height == 0 {
        return Viewport {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }

    let (width, height) = match fit {
        Fit::Stretch => (window_width, window_height),
        Fit::Aspect => {
            let scale = (f64::from(window_width) / f64::from(SOURCE_WIDTH))
                .min(f64::from(window_height) / f64::from(SOURCE_HEIGHT));
            (
                (f64::from(SOURCE_WIDTH) * scale).round().max(1.0) as u32,
                (f64::from(SOURCE_HEIGHT) * scale).round().max(1.0) as u32,
            )
        }
        Fit::Integer => {
            // Never 0: a window smaller than the source shows it clipped
            // rather than nothing.
            let scale = (window_width / SOURCE_WIDTH)
                .min(window_height / SOURCE_HEIGHT)
                .max(1);
            (SOURCE_WIDTH * scale, SOURCE_HEIGHT * scale)
        }
    };

    Viewport {
        x: (window_width - width.min(window_width)) / 2,
        y: (window_height - height.min(window_height)) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_centred(view: Viewport, window_width: u32, window_height: u32) {
        let left = view.x;
        let right = window_width - (view.x + view.width.min(window_width));
        assert!(
            left.abs_diff(right) <= 1,
            "not centred horizontally: {view:?} in {window_width}x{window_height}"
        );
        let top = view.y;
        let bottom = window_height - (view.y + view.height.min(window_height));
        assert!(
            top.abs_diff(bottom) <= 1,
            "not centred vertically: {view:?} in {window_width}x{window_height}"
        );
    }

    #[test]
    fn integer_scale_selects_whole_source_multiples() {
        // 1080p: height is the limit, so 4x (1024x960) and not 7x.
        let view = viewport(Fit::Integer, 1920, 1080);
        assert_eq!(
            view,
            Viewport {
                x: 448,
                y: 60,
                width: 1024,
                height: 960
            }
        );
        assert_eq!(view.scale(), 4.0);

        // 2560x1080 ultrawide: still height-limited, pillarboxed.
        let view = viewport(Fit::Integer, 2560, 1080);
        assert_eq!(
            view,
            Viewport {
                x: 768,
                y: 60,
                width: 1024,
                height: 960
            }
        );

        // 4:3 1024x768: 3x, both bars.
        let view = viewport(Fit::Integer, 1024, 768);
        assert_eq!(
            view,
            Viewport {
                x: 128,
                y: 24,
                width: 768,
                height: 720
            }
        );

        // Portrait 1080x1920: width-limited, 4x.
        let view = viewport(Fit::Integer, 1080, 1920);
        assert_eq!(
            view,
            Viewport {
                x: 28,
                y: 480,
                width: 1024,
                height: 960
            }
        );
    }

    #[test]
    fn integer_mode_never_scales_fractionally() {
        for window_width in [320u32, 640, 800, 1024, 1366, 1920, 2560, 3440, 3840] {
            for window_height in [240u32, 480, 600, 720, 768, 1080, 1440, 1600, 2160] {
                let view = viewport(Fit::Integer, window_width, window_height);
                assert!(
                    view.is_integer(),
                    "fractional rect {view:?} in {window_width}x{window_height}"
                );
                assert_eq!(
                    view.width % SOURCE_WIDTH,
                    0,
                    "width not a multiple in {window_width}x{window_height}"
                );
                assert_eq!(
                    view.height % SOURCE_HEIGHT,
                    0,
                    "height not a multiple in {window_width}x{window_height}"
                );
                assert_centred(view, window_width, window_height);
            }
        }
    }

    #[test]
    fn aspect_mode_keeps_the_source_ratio_on_16_9_and_21_9() {
        // 1920x1080: 4.5x, bars left and right only.
        let view = viewport(Fit::Aspect, 1920, 1080);
        assert_eq!(
            view,
            Viewport {
                x: 384,
                y: 0,
                width: 1152,
                height: 1080
            }
        );
        assert!(view.scale() > 4.0 && view.scale() < 5.0);

        // 2560x1080 ultrawide: same 4.5x, wider pillarbox.
        let view = viewport(Fit::Aspect, 2560, 1080);
        assert_eq!(
            view,
            Viewport {
                x: 704,
                y: 0,
                width: 1152,
                height: 1080
            }
        );
    }

    #[test]
    fn aspect_mode_keeps_the_source_ratio_on_4_3_and_portrait() {
        // 1024x768: 3.2x, bars left and right.
        let view = viewport(Fit::Aspect, 1024, 768);
        assert_eq!(
            view,
            Viewport {
                x: 102,
                y: 0,
                width: 819,
                height: 768
            }
        );

        // 1080x1920 portrait: width-limited, letterboxed.
        let view = viewport(Fit::Aspect, 1080, 1920);
        assert_eq!(
            view,
            Viewport {
                x: 0,
                y: 453,
                width: 1080,
                height: 1013
            }
        );
    }

    #[test]
    fn aspect_mode_is_undistorted_in_every_common_format() {
        let ratio = f64::from(SOURCE_WIDTH) / f64::from(SOURCE_HEIGHT);
        for (window_width, window_height) in [
            (1920u32, 1080u32),
            (2560, 1080),
            (1024, 768),
            (1080, 1920),
            (3440, 1440),
        ] {
            let view = viewport(Fit::Aspect, window_width, window_height);
            let actual = f64::from(view.width) / f64::from(view.height);
            assert!(
                (actual - ratio).abs() < 0.002,
                "distorted {view:?} in {window_width}x{window_height}: {actual} vs {ratio}"
            );
            assert_centred(view, window_width, window_height);
        }
    }

    #[test]
    fn stretch_fills_the_whole_window() {
        assert_eq!(
            viewport(Fit::Stretch, 1920, 1080),
            Viewport {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            viewport(Fit::Stretch, 800, 600),
            Viewport {
                x: 0,
                y: 0,
                width: 800,
                height: 600
            }
        );
    }

    #[test]
    fn tiny_and_empty_windows_do_not_panic() {
        assert_eq!(
            viewport(Fit::Integer, 0, 0),
            Viewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0
            }
        );
        let view = viewport(Fit::Integer, 100, 100);
        assert_eq!(view.width, SOURCE_WIDTH);
        assert_eq!(view.height, SOURCE_HEIGHT);
        let view = viewport(Fit::Aspect, 100, 100);
        assert!(view.width > 0 && view.height > 0);
    }
}
