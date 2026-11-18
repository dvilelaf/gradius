//! Minimal PNG and hashing helpers for the development CLI.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::{Context, Result};

pub fn write_rgb(path: &Path, width: u32, height: u32, rgb: &[u8]) -> Result<()> {
    assert_eq!(
        rgb.len(),
        (width * height * 3) as usize,
        "framebuffer size mismatch"
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgb)?;
    Ok(())
}

pub fn sha256(data: &[u8]) -> String {
    gradius_extract::sha256_hex(data)
}
