//! Dependency-free BMP writer.
//!
//! Used by the windowed F12 key and by `--screenshot` in headless mode. Writing
//! the file by hand keeps the headless path free of SDL video initialization,
//! and BMP is trivial enough that pulling an image crate would not pay for
//! itself here.

use std::path::Path;

/// Write a 24-bit bottom-up BMP from an RGBA buffer.
pub fn write_rgba(path: &Path, width: usize, height: usize, rgba: &[u8]) -> Result<(), String> {
    if rgba.len() < width * height * 4 {
        return Err("framebuffer is smaller than the image size".into());
    }
    let row_bytes = width * 3;
    let stride = (row_bytes + 3) & !3;
    let image_size = stride * height;
    let file_size = 54 + image_size;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
    }

    let mut bytes = Vec::with_capacity(file_size);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(file_size as u32).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&(width as i32).to_le_bytes());
    bytes.extend_from_slice(&(height as i32).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(image_size as u32).to_le_bytes());
    bytes.extend_from_slice(&2835i32.to_le_bytes());
    bytes.extend_from_slice(&2835i32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());

    let mut row = vec![0u8; stride];
    for y in (0..height).rev() {
        for x in 0..width {
            let offset = (y * width + x) * 4;
            row[x * 3] = rgba[offset + 2];
            row[x * 3 + 1] = rgba[offset + 1];
            row[x * 3 + 2] = rgba[offset];
        }
        row[row_bytes..].fill(0);
        bytes.extend_from_slice(&row);
    }
    std::fs::write(path, bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Timestamped name used by the F12 key.
pub fn screenshot_path(directory: &Path, frame: u64) -> std::path::PathBuf {
    directory.join(format!("gradius-{frame:06}.bmp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_well_formed_bmp() {
        let path = std::env::temp_dir().join(format!("gradius-bmp-{}.bmp", std::process::id()));
        let rgba = [
            255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        write_rgba(&path, 2, 2, &rgba).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(&bytes[..2], b"BM");
        // Top-left red pixel appears in the last row of a bottom-up image.
        // Bottom-up: the top-left red pixel is in the second row from the end.
        let second_row_from_end = 54 + 8;
        assert_eq!(
            &bytes[second_row_from_end..second_row_from_end + 3],
            &[0, 0, 255]
        );
    }
}
