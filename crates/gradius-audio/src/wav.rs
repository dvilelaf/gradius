//! Minimal 16-bit PCM WAV writer (mono).
//!
//! Used by `gradius --dump-audio PATH` so a headless run can be checked with
//! any audio editor or `sox`, and by the tests to verify the container.

use std::io::{self, Write};

const HEADER_LEN: u32 = 44;
const BITS_PER_SAMPLE: u16 = 16;
const CHANNELS: u16 = 1;

/// Convert `-1.0..=1.0` samples to little-endian 16-bit PCM.
pub fn to_pcm16(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Encode a mono WAV file in memory.
pub fn encode(sample_rate: u32, samples: &[f32]) -> Vec<u8> {
    let data = to_pcm16(samples);
    let mut out = Vec::with_capacity(HEADER_LEN as usize + data.len());
    write_wav(&mut out, sample_rate, &data).expect("writing to a Vec cannot fail");
    out
}

/// Write a mono WAV file to `writer`. `data` must already be 16-bit PCM.
pub fn write_wav<W: Write>(writer: &mut W, sample_rate: u32, data: &[u8]) -> io::Result<()> {
    let byte_rate = sample_rate * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE) / 8;
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;
    let riff_len = HEADER_LEN - 8 + data.len() as u32;

    writer.write_all(b"RIFF")?;
    writer.write_all(&riff_len.to_le_bytes())?;
    writer.write_all(b"WAVE")?;
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?; // PCM
    writer.write_all(&CHANNELS.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&(data.len() as u32).to_le_bytes())?;
    writer.write_all(data)?;
    Ok(())
}

/// Convenience wrapper used by the frontend.
pub fn write_samples(path: &std::path::Path, sample_rate: u32, samples: &[f32]) -> io::Result<()> {
    let bytes = encode(sample_rate, samples);
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_describes_mono_16_bit_pcm() {
        let wav = encode(48_000, &[0.0, 0.5, -0.5]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1);
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1);
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            48_000
        );
        assert_eq!(
            u32::from_le_bytes([wav[28], wav[29], wav[30], wav[31]]),
            96_000
        );
        assert_eq!(u16::from_le_bytes([wav[32], wav[33]]), 2);
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 6);
        assert_eq!(wav.len(), 44 + 6);
    }

    #[test]
    fn samples_are_little_endian_and_clamped() {
        let data = to_pcm16(&[1.0, -1.0, 2.0, -2.0]);
        assert_eq!(i16::from_le_bytes([data[0], data[1]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([data[2], data[3]]), -i16::MAX);
        assert_eq!(i16::from_le_bytes([data[4], data[5]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([data[6], data[7]]), -i16::MAX);
    }

    #[test]
    fn different_rates_are_declared() {
        for rate in [44_100, 48_000] {
            let wav = encode(rate, &[0.0; 10]);
            assert_eq!(
                u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
                rate
            );
            assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 20);
        }
    }
}
