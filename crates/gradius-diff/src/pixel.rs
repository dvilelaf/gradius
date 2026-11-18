//! Framebuffer comparison and PNG writing.
//!
//! Both sides render 256x240 RGBA. A sampled frame is compared pixel by pixel,
//! split into the playfield (`y < 208`) and the HUD band (`y >= 208`). The
//! caller writes the two images and collects the per-sample statistics into a
//! [`PixelSummary`], which prints as a compact table and serialises as one JSON
//! object.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Framebuffer width, shared by the oracle and the port.
pub const WIDTH: usize = 256;
/// Framebuffer height, shared by the oracle and the port.
pub const HEIGHT: usize = 240;
/// First scanline of the HUD band. The playfield is everything above it.
pub const HUD_TOP: usize = 208;

/// Inclusive bounding box of the differing pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundingBox {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl BoundingBox {
    fn include(&mut self, x: usize, y: usize) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
    }

    /// `x0,y0-x1,y1`, the form used in the table and JSON.
    pub fn label(&self) -> String {
        format!("{},{}-{},{}", self.x0, self.y0, self.x1, self.y1)
    }
}

/// Differing and total pixel counts for one area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionDiff {
    pub differing: usize,
    pub total: usize,
}

impl RegionDiff {
    fn add(&mut self) {
        self.differing += 1;
    }

    /// Matching pixels in thousandths, rounded.
    pub fn match_thousandths(&self) -> i64 {
        rate_thousandths(self.total - self.differing, self.total)
    }

    /// Differing pixels in thousandths, rounded.
    pub fn diff_thousandths(&self) -> i64 {
        rate_thousandths(self.differing, self.total)
    }
}

/// One frame's pixel comparison.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PixelDiff {
    /// Whole framebuffer.
    pub total: RegionDiff,
    /// `y < 208`.
    pub playfield: RegionDiff,
    /// `y >= 208`.
    pub hud: RegionDiff,
    /// Set only when at least one pixel differs.
    pub bbox: Option<BoundingBox>,
}

impl PixelDiff {
    /// Compare two RGBA framebuffers of [`WIDTH`]x[`HEIGHT`].
    pub fn compare(oracle: &[u8], native: &[u8]) -> Self {
        assert_eq!(
            oracle.len(),
            native.len(),
            "framebuffer length mismatch: oracle {} native {}",
            oracle.len(),
            native.len()
        );
        assert_eq!(
            oracle.len(),
            WIDTH * HEIGHT * 4,
            "framebuffer is not {}x{} RGBA",
            WIDTH,
            HEIGHT
        );
        let mut diff = Self {
            total: RegionDiff {
                differing: 0,
                total: WIDTH * HEIGHT,
            },
            playfield: RegionDiff {
                differing: 0,
                total: WIDTH * HUD_TOP,
            },
            hud: RegionDiff {
                differing: 0,
                total: WIDTH * (HEIGHT - HUD_TOP),
            },
            bbox: None,
        };
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let offset = (y * WIDTH + x) * 4;
                if oracle[offset..offset + 4] == native[offset..offset + 4] {
                    continue;
                }
                diff.total.add();
                if y < HUD_TOP {
                    diff.playfield.add();
                } else {
                    diff.hud.add();
                }
                diff.bbox
                    .get_or_insert(BoundingBox {
                        x0: x,
                        y0: y,
                        x1: x,
                        y1: y,
                    })
                    .include(x, y);
            }
        }
        diff
    }

    /// One `diff match play-match hud-match bbox` table row.
    pub fn row(&self, frame: u64) -> String {
        format!(
            "{:<6} {:>6} {:>7} {:>9} {:>9}  {}",
            frame,
            format_rate(self.total.diff_thousandths()),
            format_rate(self.total.match_thousandths()),
            format_rate(self.playfield.match_thousandths()),
            format_rate(self.hud.match_thousandths()),
            self.bbox
                .map_or_else(|| "-".to_string(), |bbox| bbox.label())
        )
    }
}

/// An inclusive frame range for the pixel report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelWindow {
    pub from: u64,
    pub to: u64,
}

impl PixelWindow {
    /// Parse a `FROM:TO` range; both ends are inclusive.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let (from, to) = spec
            .split_once(':')
            .ok_or_else(|| format!("bad --pixel-window `{spec}`: expected FROM:TO"))?;
        let from: u64 = from
            .parse()
            .map_err(|_| format!("bad --pixel-window start `{from}`"))?;
        let to: u64 = to
            .parse()
            .map_err(|_| format!("bad --pixel-window end `{to}`"))?;
        if from > to {
            return Err(format!("--pixel-window {from}:{to} starts after it ends"));
        }
        Ok(Self { from, to })
    }

    pub fn contains(&self, frame: u64) -> bool {
        (self.from..=self.to).contains(&frame)
    }

    pub fn label(&self) -> String {
        format!("{}-{}", self.from, self.to)
    }
}

/// One sampled frame and the oracle state it was sampled in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixelSample {
    pub frame: u64,
    pub state: String,
    pub diff: PixelDiff,
}

/// One line of the per-state summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateSummary {
    pub state: String,
    pub samples: usize,
    /// Summed differing and total pixels over the state's samples.
    pub diff: PixelDiff,
    /// Union of the samples' differing-pixel boxes.
    pub bbox: Option<BoundingBox>,
    /// The three lowest-matching samples, worst first.
    pub worst: Vec<WorstFrame>,
}

/// One entry of [`StateSummary::worst`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorstFrame {
    pub frame: u64,
    pub match_thousandths: i64,
    pub bbox: Option<BoundingBox>,
}

/// Every sampled frame of a run.
#[derive(Clone, Debug, Default)]
pub struct PixelSummary {
    pub samples: Vec<PixelSample>,
    window: Option<PixelWindow>,
}

impl PixelSummary {
    /// Restrict the report to samples inside `window`.
    pub fn set_window(&mut self, window: PixelWindow) {
        self.window = Some(window);
    }

    pub fn push(&mut self, frame: u64, state: impl Into<String>, diff: PixelDiff) {
        if self.window.is_some_and(|window| !window.contains(frame)) {
            return;
        }
        self.samples.push(PixelSample {
            frame,
            state: state.into(),
            diff,
        });
    }

    /// First sample with any differing pixel.
    pub fn first_divergence(&self) -> Option<&PixelSample> {
        self.samples
            .iter()
            .find(|sample| sample.diff.total.differing > 0)
    }

    fn aggregate(&self) -> PixelDiff {
        aggregate(self.samples.iter())
    }

    /// Overall matching pixels in percent, or `None` without samples.
    pub fn match_percent(&self) -> Option<f64> {
        if self.samples.is_empty() {
            None
        } else {
            // `match_thousandths` is 0..=1000; percent is that / 10.
            Some(self.aggregate().total.match_thousandths() as f64 / 10.0)
        }
    }

    /// One summary per oracle state, in first-appearance order.
    pub fn states(&self) -> Vec<StateSummary> {
        let mut order: Vec<&str> = Vec::new();
        for sample in &self.samples {
            if !order.iter().any(|state| *state == sample.state) {
                order.push(&sample.state);
            }
        }
        order
            .into_iter()
            .map(|state| {
                let mut samples: Vec<&PixelSample> = self
                    .samples
                    .iter()
                    .filter(|sample| sample.state == state)
                    .collect();
                // Rank by raw differing pixels: the one-decimal rate rounds any
                // difference below 0.05% to 100.0%, which would put a clean
                // frame ahead of a real 2 px sample.
                samples.sort_by_key(|sample| core::cmp::Reverse(sample.diff.total.differing));
                let worst = samples
                    .iter()
                    .take(3)
                    .map(|sample| WorstFrame {
                        frame: sample.frame,
                        match_thousandths: sample.diff.total.match_thousandths(),
                        bbox: sample.diff.bbox,
                    })
                    .collect();
                StateSummary {
                    state: state.to_string(),
                    samples: samples.len(),
                    diff: aggregate(samples.iter().copied()),
                    bbox: union_bbox(samples.iter().copied()),
                    worst,
                }
            })
            .collect()
    }

    /// Compact table, a per-state summary and one aggregate line.
    pub fn table(&self) -> String {
        let mut out = String::from("pixel samples\n");
        out.push_str(&format!(
            "{:<6} {:>6} {:>7} {:>9} {:>9}  {}\n",
            "frame", "diff", "match", "play", "hud", "bbox"
        ));
        for sample in &self.samples {
            out.push_str(&sample.diff.row(sample.frame));
            out.push('\n');
        }
        if !self.samples.is_empty() {
            out.push_str("pixel states\n");
            out.push_str(&format!(
                "{:<10} {:>6} {:>6} {:>7} {:>9} {:>9}  {}\n",
                "state", "frames", "diff", "match", "play", "hud", "bbox"
            ));
            for state in self.states() {
                out.push_str(&format!(
                    "{:<10} {:>6} {:>6} {:>7} {:>9} {:>9}  {}\n",
                    state.state,
                    state.samples,
                    format_rate(state.diff.total.diff_thousandths()),
                    format_rate(state.diff.total.match_thousandths()),
                    format_rate(state.diff.playfield.match_thousandths()),
                    format_rate(state.diff.hud.match_thousandths()),
                    state
                        .bbox
                        .map_or_else(|| "-".to_string(), |bbox| bbox.label())
                ));
                let worst: Vec<String> = state
                    .worst
                    .iter()
                    .map(|sample| {
                        format!(
                            "{} {} at {}",
                            sample.frame,
                            format_rate(sample.match_thousandths),
                            sample
                                .bbox
                                .map_or_else(|| "-".to_string(), |bbox| bbox.label())
                        )
                    })
                    .collect();
                out.push_str(&format!("  worst {}\n", worst.join(", ")));
            }
        }
        let aggregate = self.aggregate();
        let window = match self.window {
            Some(window) => format!(" in window {}", window.label()),
            None => String::new(),
        };
        out.push_str(&format!(
            "pixel summary: {} samples{}, match {} (play {} hud {}), first divergence {}\n",
            self.samples.len(),
            window,
            format_rate(aggregate.total.match_thousandths()),
            format_rate(aggregate.playfield.match_thousandths()),
            format_rate(aggregate.hud.match_thousandths()),
            match self.first_divergence() {
                Some(sample) => format!(
                    "frame {} at {}",
                    sample.frame,
                    sample
                        .diff
                        .bbox
                        .map_or_else(|| "-".to_string(), |bbox| bbox.label())
                ),
                None => "none".to_string(),
            }
        ));
        out
    }

    /// The `"pixels": { ... }` member of the JSON report.
    pub fn to_json(&self) -> String {
        let samples: Vec<String> = self
            .samples
            .iter()
            .map(|sample| {
                let diff = &sample.diff;
                format!(
                    "    {{ \"frame\": {}, \"state\": \"{}\", \"differing\": {}, \"total\": {}, \"match\": {}, \"bbox\": {}, \"playfield\": {{ \"differing\": {}, \"total\": {}, \"match\": {} }}, \"hud\": {{ \"differing\": {}, \"total\": {}, \"match\": {} }} }}",
                    sample.frame,
                    sample.state,
                    diff.total.differing,
                    diff.total.total,
                    json_rate(diff.total.match_thousandths()),
                    json_bbox(diff.bbox),
                    diff.playfield.differing,
                    diff.playfield.total,
                    json_rate(diff.playfield.match_thousandths()),
                    diff.hud.differing,
                    diff.hud.total,
                    json_rate(diff.hud.match_thousandths())
                )
            })
            .collect();
        let states: Vec<String> = self
            .states()
            .iter()
            .map(|state| {
                let worst: Vec<String> = state
                    .worst
                    .iter()
                    .map(|sample| {
                        format!(
                            "{{ \"frame\": {}, \"match\": {}, \"bbox\": {} }}",
                            sample.frame,
                            json_rate(sample.match_thousandths),
                            json_bbox(sample.bbox)
                        )
                    })
                    .collect();
                format!(
                    "      {{ \"state\": \"{}\", \"samples\": {}, \"match\": {}, \"playfield\": {}, \"hud\": {}, \"bbox\": {}, \"worst\": [{}] }}",
                    state.state,
                    state.samples,
                    json_rate(state.diff.total.match_thousandths()),
                    json_rate(state.diff.playfield.match_thousandths()),
                    json_rate(state.diff.hud.match_thousandths()),
                    json_bbox(state.bbox),
                    worst.join(", ")
                )
            })
            .collect();
        let first = match self.first_divergence() {
            Some(sample) => format!(
                "{{ \"frame\": {}, \"bbox\": {} }}",
                sample.frame,
                json_bbox(sample.diff.bbox)
            ),
            None => "null".to_string(),
        };
        let aggregate = self.aggregate();
        let window = match self.window {
            Some(window) => format!("[ {}, {} ]", window.from, window.to),
            None => "null".to_string(),
        };
        format!(
            "  \"pixels\": {{\n    \"samples\": [\n{}\n    ],\n    \"summary\": {{ \"samples\": {}, \"window\": {}, \"match\": {}, \"playfield\": {}, \"hud\": {}, \"first_divergence\": {} }},\n    \"states\": [\n{}\n    ]\n  }}",
            samples.join(",\n"),
            self.samples.len(),
            window,
            json_rate(aggregate.total.match_thousandths()),
            json_rate(aggregate.playfield.match_thousandths()),
            json_rate(aggregate.hud.match_thousandths()),
            first,
            states.join(",\n")
        )
    }
}

/// Sum differing and total pixels over a set of samples.
fn aggregate<'a>(samples: impl Iterator<Item = &'a PixelSample>) -> PixelDiff {
    let mut total = RegionDiff::default();
    let mut playfield = RegionDiff::default();
    let mut hud = RegionDiff::default();
    for sample in samples {
        total.differing += sample.diff.total.differing;
        total.total += sample.diff.total.total;
        playfield.differing += sample.diff.playfield.differing;
        playfield.total += sample.diff.playfield.total;
        hud.differing += sample.diff.hud.differing;
        hud.total += sample.diff.hud.total;
    }
    PixelDiff {
        total,
        playfield,
        hud,
        bbox: None,
    }
}

/// Smallest box containing every differing pixel of the samples.
fn union_bbox<'a>(samples: impl Iterator<Item = &'a PixelSample>) -> Option<BoundingBox> {
    samples
        .filter_map(|sample| sample.diff.bbox)
        .reduce(|mut boxed, other| {
            boxed.x0 = boxed.x0.min(other.x0);
            boxed.y0 = boxed.y0.min(other.y0);
            boxed.x1 = boxed.x1.max(other.x1);
            boxed.y1 = boxed.y1.max(other.y1);
            boxed
        })
}

/// Write one RGBA framebuffer as a PNG. Creates the parent directory.
pub fn write_png(path: &Path, rgba: &[u8]) -> Result<(), String> {
    assert_eq!(
        rgba.len(),
        WIDTH * HEIGHT * 4,
        "framebuffer is not {}x{} RGBA",
        WIDTH,
        HEIGHT
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let file =
        File::create(path).map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    writer
        .write_image_data(rgba)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn rate_thousandths(part: usize, total: usize) -> i64 {
    if total == 0 {
        0
    } else {
        ((part as u128 * 1000 + total as u128 / 2) / total as u128) as i64
    }
}

fn format_rate(rate: i64) -> String {
    format!("{}.{}%", rate / 10, rate % 10)
}

fn json_rate(rate: i64) -> String {
    format!("{}.{:03}", rate / 1000, rate % 1000)
}

fn json_bbox(bbox: Option<BoundingBox>) -> String {
    match bbox {
        Some(bbox) => format!("[{}, {}, {}, {}]", bbox.x0, bbox.y0, bbox.x1, bbox.y1),
        None => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> Vec<u8> {
        vec![0u8; WIDTH * HEIGHT * 4]
    }

    fn set(buffer: &mut [u8], x: usize, y: usize, rgba: [u8; 4]) {
        let offset = (y * WIDTH + x) * 4;
        buffer[offset..offset + 4].copy_from_slice(&rgba);
    }

    #[test]
    fn identical_buffers_match_completely() {
        let oracle = blank();
        let native = blank();
        let diff = PixelDiff::compare(&oracle, &native);
        assert_eq!(diff.total.differing, 0);
        assert_eq!(diff.total.match_thousandths(), 1000);
        assert_eq!(diff.playfield.differing, 0);
        assert_eq!(diff.playfield.match_thousandths(), 1000);
        assert_eq!(diff.hud.differing, 0);
        assert_eq!(diff.hud.match_thousandths(), 1000);
        assert_eq!(diff.bbox, None);
    }

    #[test]
    fn one_pixel_is_counted_and_boxed() {
        let oracle = blank();
        let mut native = blank();
        set(&mut native, 3, 4, [255, 0, 0, 255]);
        let diff = PixelDiff::compare(&oracle, &native);
        assert_eq!(diff.total.differing, 1);
        assert_eq!(diff.total.total, WIDTH * HEIGHT);
        assert_eq!(diff.playfield.differing, 1);
        assert_eq!(diff.hud.differing, 0);
        assert_eq!(
            diff.bbox,
            Some(BoundingBox {
                x0: 3,
                y0: 4,
                x1: 3,
                y1: 4
            })
        );
        // 1/61440 rounded to thousandths is zero, so the diff rate is 0.0%.
        assert_eq!(diff.total.diff_thousandths(), 0);
        assert!(diff.row(0).contains("3,4-3,4"));
    }

    #[test]
    fn the_hud_band_starts_at_row_208() {
        let oracle = blank();
        let mut native = blank();
        set(&mut native, 10, HUD_TOP - 1, [1, 2, 3, 255]);
        set(&mut native, 20, HUD_TOP, [1, 2, 3, 255]);
        set(&mut native, 30, HEIGHT - 1, [1, 2, 3, 255]);
        let diff = PixelDiff::compare(&oracle, &native);
        assert_eq!(diff.total.differing, 3);
        assert_eq!(diff.playfield.differing, 1);
        assert_eq!(diff.playfield.total, WIDTH * HUD_TOP);
        assert_eq!(diff.hud.differing, 2);
        assert_eq!(diff.hud.total, WIDTH * (HEIGHT - HUD_TOP));
        assert_eq!(
            diff.bbox,
            Some(BoundingBox {
                x0: 10,
                y0: HUD_TOP - 1,
                x1: 30,
                y1: HEIGHT - 1
            })
        );
    }

    #[test]
    fn summary_aggregates_regions_and_names_the_first_divergence() {
        let oracle = blank();
        let mut first = blank();
        set(&mut first, 0, 0, [9, 9, 9, 255]);
        let diff = PixelDiff::compare(&oracle, &first);
        let mut summary = PixelSummary::default();
        summary.push(0, "Title", diff);
        summary.push(60, "Title", PixelDiff::compare(&oracle, &oracle));
        assert_eq!(summary.first_divergence().unwrap().frame, 0);
        assert_eq!(summary.aggregate().total.differing, 1);
        let table = summary.table();
        assert!(table.contains("pixel summary: 2 samples"));
        assert!(table.contains("first divergence frame 0 at 0,0-0,0"));
        let json = summary.to_json();
        assert!(json.contains("\"pixels\""));
        assert!(json.contains("\"frame\": 0"));
        assert!(json.contains("\"state\": \"Title\""));
        assert!(json.contains("\"first_divergence\": { \"frame\": 0, \"bbox\": [0, 0, 0, 0] }"));
    }

    #[test]
    fn window_parses_and_filters_samples() {
        let window = PixelWindow::parse("10:20").expect("valid window");
        assert_eq!(window, PixelWindow { from: 10, to: 20 });
        assert!(!window.contains(9));
        assert!(window.contains(10));
        assert!(window.contains(20));
        assert!(!window.contains(21));
        assert!(PixelWindow::parse("10").is_err());
        assert!(PixelWindow::parse("a:20").is_err());
        assert!(PixelWindow::parse("10:b").is_err());
        assert!(PixelWindow::parse("20:10").is_err());

        let oracle = blank();
        let clean = PixelDiff::compare(&oracle, &oracle);
        let mut summary = PixelSummary::default();
        summary.set_window(window);
        summary.push(9, "Title", clean.clone());
        summary.push(10, "Attract", clean.clone());
        summary.push(20, "Playing", clean.clone());
        summary.push(21, "Playing", clean.clone());
        assert_eq!(summary.samples.len(), 2);
        assert_eq!(summary.samples[0].frame, 10);
        assert_eq!(summary.samples[1].state, "Playing");
        assert!(summary.table().contains("in window 10-20"));
        assert!(summary.to_json().contains("\"window\": [ 10, 20 ]"));
    }

    #[test]
    fn states_aggregate_in_first_appearance_order_with_worst_frames() {
        let oracle = blank();
        let mut one = blank();
        set(&mut one, 3, 4, [9, 9, 9, 255]);
        let mut many = blank();
        set(&mut many, 0, 0, [9, 9, 9, 255]);
        set(&mut many, 1, 1, [9, 9, 9, 255]);
        let clean = PixelDiff::compare(&oracle, &oracle);
        let mut summary = PixelSummary::default();
        summary.push(5, "Title", PixelDiff::compare(&oracle, &one));
        summary.push(6, "Title", clean.clone());
        summary.push(7, "Playing", PixelDiff::compare(&oracle, &many));
        summary.push(8, "Playing", PixelDiff::compare(&oracle, &one));

        let states = summary.states();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].state, "Title");
        assert_eq!(states[0].samples, 2);
        assert_eq!(states[0].diff.total.differing, 1);
        assert_eq!(states[0].diff.total.total, 2 * WIDTH * HEIGHT);
        assert_eq!(states[0].worst[0].frame, 5);
        assert_eq!(states[0].worst[0].bbox.unwrap().label(), "3,4-3,4");
        assert_eq!(states[1].state, "Playing");
        assert_eq!(states[1].diff.total.differing, 3);
        assert_eq!(states[1].bbox.unwrap().label(), "0,0-3,4");
        assert_eq!(states[1].worst[0].frame, 7);
        assert_eq!(states[1].worst[1].frame, 8);

        let table = summary.table();
        assert!(table.contains("pixel states"));
        assert!(table.contains("Title"));
        assert!(table.contains("Playing"));
        assert!(table.contains("worst 7"));
        assert!(summary.to_json().contains("\"states\""));
    }

    #[test]
    fn worst_frames_rank_by_differing_pixels_not_the_rounded_rate() {
        let oracle = blank();
        let mut one = blank();
        set(&mut one, 3, 4, [9, 9, 9, 255]);
        let mut two = blank();
        set(&mut two, 3, 4, [9, 9, 9, 255]);
        set(&mut two, 4, 4, [9, 9, 9, 255]);
        let mut summary = PixelSummary::default();
        summary.push(5, "Attract", PixelDiff::compare(&oracle, &one));
        summary.push(6, "Attract", PixelDiff::compare(&oracle, &two));

        let states = summary.states();
        // Both rates round to 100.0%, so the 2 px sample must still rank first.
        assert_eq!(states[0].worst[0].match_thousandths, 1000);
        assert_eq!(states[0].worst[0].frame, 6);
        assert_eq!(states[0].worst[1].frame, 5);
    }

    #[test]
    fn png_is_rgba_with_the_framebuffer_dimensions() {
        let mut pixels = blank();
        set(&mut pixels, 0, 0, [12, 34, 56, 255]);
        let path = std::env::temp_dir().join(format!(
            "gradius-diff-pixel-{}-{}.png",
            std::process::id(),
            line!()
        ));
        write_png(&path, &pixels).expect("write png");
        let data = std::fs::read(&path).expect("read png");
        let _ = std::fs::remove_file(&path);
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");

        let decoder = png::Decoder::new(std::io::Cursor::new(&data));
        let reader = decoder.read_info().expect("png header");
        let info = reader.info();
        assert_eq!(info.width, WIDTH as u32);
        assert_eq!(info.height, HEIGHT as u32);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
    }
}
