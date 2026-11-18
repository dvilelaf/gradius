//! Versioned audio chunk inside the self-contained asset pack.
//!
//! `xtask extract` captures the register timelines from the cartridge and
//! writes them into payload chunk [`gradius_pack::CHUNK_AUDIO`] with
//! [`encode_chunk`]; [`AudioBank::from_chunk`] rebuilds the [`Timeline`]s from
//! the bytes. Without a chunk there are no timelines and the game is silent.
//!
//! Layout of the chunk payload (all integers little-endian):
//!
//! ```text
//! magic      4  "GAUD"
//! version    u16  currently 2 (1 = timelines only)
//! tracks     u16  number of track records
//! per track:
//!   id       u8   Music or Sfx discriminant (see [`crate::data`])
//!   kind     u8   0 = music, 1 = sound effect
//!   flags    u16  bit 0 set when the track loops
//!   len      u32  bytes after this 8-byte header
//!   payload:
//!     name_len     u8
//!     name         name_len bytes, UTF-8
//!     total_cycles u32
//!     loop_cycles  u32  (0 when the track does not loop)
//!     loop_index   u32
//!     event_count  u32
//!     events       event_count * 6 bytes
//!       frame  u16  CPU frame offset from the start of the track
//!       cycle  u16  cycle within that frame
//!       addr   u8   APU register offset from `$4000`
//!       value  u8
//! driver_len u32  bytes in the driver section, 0 when absent (version 2)
//! driver:
//!   magic   4  "KDRV"
//!   version u16  currently 1
//!   base    u16  CPU address of the image, `$C000`
//!   request_addr u16, request_len u16, request bytes
//!   period_addr  u16, period_len  u16, period bytes
//!   channel_len u8 (17), channel_count u8 (4)
//!   per channel: base u16, APU register offset u8
//!   segments u16
//!   per segment: addr u16, len u16, bytes
//! ```
//!
//! `len` makes every record skippable: a reader that does not know a track id
//! or kind can jump over it, which keeps the chunk forward compatible. Music
//! ids 0-5 are the title, stage-1, Big Core, clear and game-over cues; 6-11
//! are the stage 2-7 second themes. Adding the latter did not change the
//! record layout, so the chunk version remains 2.
//!
//! Version 1 chunks (timelines only) still decode, and a version 2 chunk with
//! `driver_len = 0` decodes with no driver. The driver section contains only the request table, period
//! table and sequence bytes that the supported requests read, plus the four
//! channel-block addresses and register offsets. The channel blocks themselves
//! are mutable runtime state and are not cartridge data.

use std::fmt;

use crate::data::{Music, Sfx};
use crate::driver::{DriverBank, DriverSource, CHANNEL_BLOCK_LEN, REQUEST_TABLE_LEN};
use crate::timeline::{Event, Timeline};

/// Chunk magic, checked independently of the pack container.
pub const AUDIO_CHUNK_MAGIC: [u8; 4] = *b"GAUD";
/// Current chunk layout version.
pub const AUDIO_CHUNK_VERSION: u16 = 2;
const AUDIO_CHUNK_VERSION_TIMELINES: u16 = 1;
const DRIVER_MAGIC: [u8; 4] = *b"KDRV";
const DRIVER_VERSION: u16 = 1;
/// NTSC frame length used to split each event offset into frame and cycle.
pub const FRAME_CYCLES: u32 = 29_780;

const KIND_MUSIC: u8 = 0;
const KIND_SFX: u8 = 1;
const FLAG_LOOP: u16 = 0x0001;
const TRACK_HEADER_LEN: usize = 8;
const EVENT_BYTES: usize = 6;
const DRIVER_HEADER_LEN: usize = 8;

/// Why an audio chunk could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioChunkError {
    /// The payload ends before a field or record does.
    Truncated,
    /// The payload does not begin with [`AUDIO_CHUNK_MAGIC`].
    Magic,
    /// A future layout that this reader does not understand.
    Version(u16),
    /// A known track record is inconsistent.
    Malformed(&'static str),
}

impl fmt::Display for AudioChunkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(formatter, "truncated audio chunk"),
            Self::Magic => write!(formatter, "not an audio chunk (bad magic)"),
            Self::Version(version) => {
                write!(formatter, "unsupported audio chunk version {version}")
            }
            Self::Malformed(reason) => write!(formatter, "malformed audio chunk: {reason}"),
        }
    }
}

impl std::error::Error for AudioChunkError {}

/// The music and effect timelines loaded from one pack chunk.
///
/// Tracks absent from the chunk stay `None`; the default bank has none.
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioBank {
    music: [Option<&'static Timeline>; Music::ALL.len()],
    sfx: [Option<&'static Timeline>; Sfx::ALL.len()],
    driver: Option<&'static DriverBank>,
}

impl AudioBank {
    /// Decode the payload of [`gradius_pack::CHUNK_AUDIO`].
    ///
    /// Unknown track ids and kinds are skipped; a malformed known track is an
    /// error. The decoded names and event arrays are leaked once so the
    /// existing [`Timeline`] and [`crate::Engine`] `&'static` interface keeps
    /// working; a pack is loaded once per process.
    pub fn from_chunk(bytes: &[u8]) -> Result<Self, AudioChunkError> {
        if bytes.len() < 8 {
            return Err(AudioChunkError::Truncated);
        }
        if bytes[..4] != AUDIO_CHUNK_MAGIC {
            return Err(AudioChunkError::Magic);
        }
        let version = read_u16(bytes, 4)?;
        if version != AUDIO_CHUNK_VERSION && version != AUDIO_CHUNK_VERSION_TIMELINES {
            return Err(AudioChunkError::Version(version));
        }
        let track_count = usize::from(read_u16(bytes, 6)?);
        let mut bank = Self::default();
        let mut offset = 8;
        for _ in 0..track_count {
            let header = bytes
                .get(offset..offset + TRACK_HEADER_LEN)
                .ok_or(AudioChunkError::Truncated)?;
            let id = header[0];
            let kind = header[1];
            let flags = u16::from_le_bytes([header[2], header[3]]);
            let length = usize::try_from(u32::from_le_bytes([
                header[4], header[5], header[6], header[7],
            ]))
            .map_err(|_| AudioChunkError::Malformed("track length"))?;
            let start = offset + TRACK_HEADER_LEN;
            let end = start
                .checked_add(length)
                .ok_or(AudioChunkError::Malformed("track length"))?;
            let body = bytes.get(start..end).ok_or(AudioChunkError::Truncated)?;
            if let Some(slot) = slot(&mut bank, kind, id) {
                *slot = Some(parse_track(body, flags)?);
            }
            offset = end;
        }
        if version == AUDIO_CHUNK_VERSION {
            let length = usize::try_from(read_u32(bytes, offset)?)
                .map_err(|_| AudioChunkError::Malformed("driver length"))?;
            offset += 4;
            if length > 0 {
                let end = offset
                    .checked_add(length)
                    .ok_or(AudioChunkError::Malformed("driver length"))?;
                let payload = bytes.get(offset..end).ok_or(AudioChunkError::Truncated)?;
                bank.driver = Some(parse_driver(payload)?);
                offset = end;
            }
        }
        if offset != bytes.len() {
            return Err(AudioChunkError::Malformed("trailing chunk bytes"));
        }
        Ok(bank)
    }

    /// The pack's music track, if the chunk carried it.
    pub fn music(&self, music: Music) -> Option<&'static Timeline> {
        self.music[music_index(music)]
    }

    /// The pack's sound effect, if the chunk carried it.
    pub fn sfx(&self, sfx: Sfx) -> Option<&'static Timeline> {
        self.sfx[sfx_index(sfx)]
    }

    /// The pack's driver image, when the chunk carried one.
    pub fn driver_bank(&self) -> Option<&'static DriverBank> {
        self.driver
    }

    /// A fresh native Konami driver over the pack's sequence image.
    pub fn driver(&self) -> Option<crate::driver::Driver<'static>> {
        self.driver.map(DriverBank::driver)
    }
}

fn slot(bank: &mut AudioBank, kind: u8, id: u8) -> Option<&mut Option<&'static Timeline>> {
    match kind {
        KIND_MUSIC if usize::from(id) < bank.music.len() => Some(&mut bank.music[usize::from(id)]),
        KIND_SFX if usize::from(id) < bank.sfx.len() => Some(&mut bank.sfx[usize::from(id)]),
        _ => None,
    }
}

fn parse_track(body: &[u8], flags: u16) -> Result<&'static Timeline, AudioChunkError> {
    let mut offset = 0;
    let name_len = usize::from(*body.first().ok_or(AudioChunkError::Truncated)?);
    offset += 1;
    let name_bytes = body
        .get(offset..offset + name_len)
        .ok_or(AudioChunkError::Truncated)?;
    let name = std::str::from_utf8(name_bytes)
        .map_err(|_| AudioChunkError::Malformed("track name is not UTF-8"))?;
    offset += name_len;
    let total_cycles = read_u32(body, offset)?;
    offset += 4;
    let loop_cycles = read_u32(body, offset)?;
    offset += 4;
    let loop_index = read_u32(body, offset)?;
    offset += 4;
    let event_count = usize::try_from(read_u32(body, offset)?)
        .map_err(|_| AudioChunkError::Malformed("event count"))?;
    offset += 4;
    let events_len = event_count
        .checked_mul(EVENT_BYTES)
        .ok_or(AudioChunkError::Malformed("event count"))?;
    let event_bytes = body
        .get(offset..offset + events_len)
        .ok_or(AudioChunkError::Truncated)?;
    if body.len() != offset + events_len {
        return Err(AudioChunkError::Malformed("track has trailing bytes"));
    }

    let mut events = Vec::with_capacity(event_count);
    let mut last = 0;
    for record in event_bytes.chunks_exact(EVENT_BYTES) {
        let frame = u32::from(u16::from_le_bytes([record[0], record[1]]));
        let cycle = u32::from(u16::from_le_bytes([record[2], record[3]]));
        if cycle >= FRAME_CYCLES {
            return Err(AudioChunkError::Malformed("event cycle is outside a frame"));
        }
        let at = frame
            .checked_mul(FRAME_CYCLES)
            .and_then(|base| base.checked_add(cycle))
            .ok_or(AudioChunkError::Malformed("event offset"))?;
        let addr = record[4];
        if addr > 0x17 {
            return Err(AudioChunkError::Malformed(
                "event register is outside the APU",
            ));
        }
        if at < last || at >= total_cycles {
            return Err(AudioChunkError::Malformed(
                "event is out of order or past the end",
            ));
        }
        last = at;
        events.push(Event {
            at,
            addr,
            value: record[5],
        });
    }

    let looping = flags & FLAG_LOOP != 0;
    let loop_point = looping.then_some(loop_cycles);
    if looping {
        if loop_cycles >= total_cycles {
            return Err(AudioChunkError::Malformed("loop point is past the end"));
        }
        if loop_index as usize >= events.len() {
            return Err(AudioChunkError::Malformed("loop index"));
        }
        if events[loop_index as usize].at < loop_cycles {
            return Err(AudioChunkError::Malformed(
                "loop index does not point at the loop",
            ));
        }
        if loop_index > 0 && events[loop_index as usize - 1].at >= loop_cycles {
            return Err(AudioChunkError::Malformed("loop index skips an event"));
        }
    } else if loop_index != 0 {
        return Err(AudioChunkError::Malformed(
            "one-shot track has a loop index",
        ));
    }

    Ok(Box::leak(Box::new(Timeline {
        name: Box::leak(name.to_owned().into_boxed_str()),
        events: Box::leak(events.into_boxed_slice()),
        total_cycles,
        loop_point,
        loop_index: u16::try_from(loop_index)
            .map_err(|_| AudioChunkError::Malformed("loop index"))?,
    })))
}

/// Encode captured timelines and a driver section into a complete audio chunk.
///
/// `music` and `sfx` follow [`Music::ALL`] and [`Sfx::ALL`] order, so the same
/// capture always produces the same bytes. `driver` is the output of
/// [`encode_driver`], or empty for a timelines-only chunk.
pub fn encode_chunk(music: &[Timeline], sfx: &[Timeline], driver: &[u8]) -> Vec<u8> {
    let mut tracks = Vec::new();
    for (index, timeline) in music.iter().enumerate() {
        encode_track(&mut tracks, KIND_MUSIC, index as u8, timeline);
    }
    for (index, timeline) in sfx.iter().enumerate() {
        encode_track(&mut tracks, KIND_SFX, index as u8, timeline);
    }
    let mut out = Vec::with_capacity(12 + tracks.len() + driver.len());
    out.extend_from_slice(&AUDIO_CHUNK_MAGIC);
    out.extend_from_slice(&AUDIO_CHUNK_VERSION.to_le_bytes());
    out.extend_from_slice(&((music.len() + sfx.len()) as u16).to_le_bytes());
    out.extend_from_slice(&tracks);
    out.extend_from_slice(&(driver.len() as u32).to_le_bytes());
    out.extend_from_slice(driver);
    out
}

/// Serialize the extracted driver assets into the `KDRV` section.
pub fn encode_driver(source: &DriverSource) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&DRIVER_MAGIC);
    out.extend_from_slice(&DRIVER_VERSION.to_le_bytes());
    out.extend_from_slice(&source.base.to_le_bytes());
    out.extend_from_slice(&source.request_addr.to_le_bytes());
    out.extend_from_slice(&(source.request.len() as u16).to_le_bytes());
    out.extend_from_slice(&source.request);
    out.extend_from_slice(&source.period_addr.to_le_bytes());
    out.extend_from_slice(&(source.period.len() as u16).to_le_bytes());
    out.extend_from_slice(&source.period);
    out.push(CHANNEL_BLOCK_LEN);
    out.push(source.channels.len() as u8);
    for (base, reg) in source.channels {
        out.extend_from_slice(&base.to_le_bytes());
        out.push(reg);
    }
    out.extend_from_slice(&(source.segments.len() as u16).to_le_bytes());
    for (addr, bytes) in &source.segments {
        out.extend_from_slice(&addr.to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn parse_driver(payload: &[u8]) -> Result<&'static DriverBank, AudioChunkError> {
    if payload.len() < DRIVER_HEADER_LEN || payload[..4] != DRIVER_MAGIC {
        return Err(AudioChunkError::Malformed("driver magic"));
    }
    if read_u16(payload, 4)? != DRIVER_VERSION {
        return Err(AudioChunkError::Malformed("driver version"));
    }
    let base = read_u16(payload, 6)?;
    let mut offset = DRIVER_HEADER_LEN;

    let request_addr = read_u16(payload, offset)?;
    offset += 2;
    let request_len = usize::from(read_u16(payload, offset)?);
    offset += 2;
    if request_len != REQUEST_TABLE_LEN {
        return Err(AudioChunkError::Malformed("driver request table length"));
    }
    let request = payload
        .get(offset..offset + request_len)
        .ok_or(AudioChunkError::Truncated)?;
    offset += request_len;

    let period_addr = read_u16(payload, offset)?;
    offset += 2;
    let period_len = usize::from(read_u16(payload, offset)?);
    offset += 2;
    if period_len != crate::driver::PERIOD_TABLE_LEN {
        return Err(AudioChunkError::Malformed("driver period table length"));
    }
    let period = payload
        .get(offset..offset + period_len)
        .ok_or(AudioChunkError::Truncated)?;
    offset += period_len;

    let channel_len = *payload.get(offset).ok_or(AudioChunkError::Truncated)?;
    offset += 1;
    let channel_count = usize::from(*payload.get(offset).ok_or(AudioChunkError::Truncated)?);
    offset += 1;
    if channel_len != CHANNEL_BLOCK_LEN || channel_count != 4 {
        return Err(AudioChunkError::Malformed("driver channel layout"));
    }
    let mut channels = [(0u16, 0u8); 4];
    for channel in &mut channels {
        *channel = (
            read_u16(payload, offset)?,
            *payload.get(offset + 2).ok_or(AudioChunkError::Truncated)?,
        );
        offset += 3;
    }

    let segment_count = usize::from(read_u16(payload, offset)?);
    offset += 2;
    let mut image = vec![0u8; 0x1_0000usize - usize::from(base)];
    copy_to_image(&mut image, base, request_addr, request)?;
    copy_to_image(&mut image, base, period_addr, period)?;
    for _ in 0..segment_count {
        let addr = read_u16(payload, offset)?;
        offset += 2;
        let len = usize::from(read_u16(payload, offset)?);
        offset += 2;
        let bytes = payload
            .get(offset..offset + len)
            .ok_or(AudioChunkError::Truncated)?;
        offset += len;
        copy_to_image(&mut image, base, addr, bytes)?;
    }
    if offset != payload.len() {
        return Err(AudioChunkError::Malformed("driver trailing bytes"));
    }

    let mem: &'static [u8] = Box::leak(image.into_boxed_slice());
    Ok(Box::leak(Box::new(DriverBank::new(
        base,
        mem,
        request_addr,
        period_addr,
        channels,
        channel_len,
    ))))
}

fn copy_to_image(
    image: &mut [u8],
    base: u16,
    addr: u16,
    bytes: &[u8],
) -> Result<(), AudioChunkError> {
    let start = usize::from(
        addr.checked_sub(base)
            .ok_or(AudioChunkError::Malformed("driver address below bank"))?,
    );
    let end = start
        .checked_add(bytes.len())
        .ok_or(AudioChunkError::Malformed("driver segment length"))?;
    let slot = image
        .get_mut(start..end)
        .ok_or(AudioChunkError::Malformed("driver address outside bank"))?;
    slot.copy_from_slice(bytes);
    Ok(())
}

fn encode_track(out: &mut Vec<u8>, kind: u8, id: u8, timeline: &Timeline) {
    let mut body = Vec::new();
    let name = timeline.name.as_bytes();
    body.push(u8::try_from(name.len()).expect("track name is too long"));
    body.extend_from_slice(name);
    body.extend_from_slice(&timeline.total_cycles.to_le_bytes());
    body.extend_from_slice(&timeline.loop_point.unwrap_or(0).to_le_bytes());
    body.extend_from_slice(&u32::from(timeline.loop_index).to_le_bytes());
    body.extend_from_slice(&(timeline.events.len() as u32).to_le_bytes());
    for event in timeline.events {
        let frame = u16::try_from(event.at / FRAME_CYCLES).expect("event frame fits");
        let cycle = u16::try_from(event.at % FRAME_CYCLES).expect("event cycle fits");
        body.extend_from_slice(&frame.to_le_bytes());
        body.extend_from_slice(&cycle.to_le_bytes());
        body.push(event.addr);
        body.push(event.value);
    }

    out.push(id);
    out.push(kind);
    let flags = if timeline.loop_point.is_some() {
        FLAG_LOOP
    } else {
        0
    };
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
}

fn music_index(music: Music) -> usize {
    match music {
        Music::Title => 0,
        Music::Stage1 => 1,
        Music::Stage1B => 2,
        Music::Boss => 3,
        Music::StageClear => 4,
        Music::GameOver => 5,
        Music::Stage2 => 6,
        Music::Stage3 => 7,
        Music::Stage4 => 8,
        Music::Stage5 => 9,
        Music::Stage6 => 10,
        Music::Stage7 => 11,
    }
}

fn sfx_index(sfx: Sfx) -> usize {
    match sfx {
        Sfx::Shot => 0,
        Sfx::EnemyShot => 1,
        Sfx::EnemyHit => 2,
        Sfx::Explosion => 3,
        Sfx::PowerUp => 4,
        Sfx::Capsule => 5,
        Sfx::PlayerDeath => 6,
        Sfx::GameOver => 7,
        Sfx::BlockBreak => 8,
        Sfx::BlockBreakStage6 => 9,
        Sfx::Pause => 10,
        Sfx::ScreenClear => 11,
    }
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, AudioChunkError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or(AudioChunkError::Truncated)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, AudioChunkError> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or(AudioChunkError::Truncated)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    static LEAD_IN: [Event; 3] = [
        Event::new(0, 0x4015, 0x0F),
        Event::new(100, 0x4000, 0x7F),
        Event::new(FRAME_CYCLES + 7, 0x4002, 0x71),
    ];

    /// One looping and one one-shot synthetic timeline, named like the real ones.
    fn synthetic(name: &'static str, looping: bool) -> Timeline {
        Timeline {
            name,
            events: &LEAD_IN,
            total_cycles: 3 * FRAME_CYCLES,
            loop_point: looping.then_some(FRAME_CYCLES),
            loop_index: u16::from(looping) * 2,
        }
    }

    fn synthetic_tracks() -> (Vec<Timeline>, Vec<Timeline>) {
        (
            Music::ALL
                .iter()
                .map(|music| synthetic(music.name(), music.loops()))
                .collect(),
            Sfx::ALL
                .iter()
                .map(|sfx| synthetic(sfx.name(), false))
                .collect(),
        )
    }

    fn timelines_chunk() -> Vec<u8> {
        let (music, sfx) = synthetic_tracks();
        encode_chunk(&music, &sfx, &[])
    }

    #[test]
    fn chunk_round_trips_every_track() {
        let (music, sfx) = synthetic_tracks();
        let bank = AudioBank::from_chunk(&encode_chunk(&music, &sfx, &[])).unwrap();
        for (kind, expected) in Music::ALL.iter().zip(&music) {
            let loaded = bank.music(*kind).expect("music track");
            assert_eq!(loaded.name, expected.name);
            assert_eq!(loaded.events, expected.events);
            assert_eq!(loaded.total_cycles, expected.total_cycles);
            assert_eq!(loaded.loop_point, expected.loop_point);
            assert_eq!(loaded.loop_index, expected.loop_index);
        }
        for (kind, expected) in Sfx::ALL.iter().zip(&sfx) {
            let loaded = bank.sfx(*kind).expect("sfx track");
            assert_eq!(loaded.events, expected.events);
        }
    }

    #[test]
    fn encoding_is_deterministic() {
        assert_eq!(timelines_chunk(), timelines_chunk());
    }

    #[test]
    fn a_default_bank_has_no_tracks() {
        let bank = AudioBank::default();
        assert!(bank.music(Music::Title).is_none());
        assert!(bank.sfx(Sfx::Shot).is_none());
        assert!(bank.driver().is_none());
    }

    #[test]
    fn unknown_tracks_are_skippable() {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&AUDIO_CHUNK_MAGIC);
        chunk.extend_from_slice(&AUDIO_CHUNK_VERSION.to_le_bytes());
        chunk.extend_from_slice(&2u16.to_le_bytes());
        // A future music id with an opaque body.
        chunk.extend_from_slice(&[0x7F, KIND_MUSIC]);
        chunk.extend_from_slice(&0u16.to_le_bytes());
        chunk.extend_from_slice(&3u32.to_le_bytes());
        chunk.extend_from_slice(b"xyz");
        // A future track kind.
        chunk.extend_from_slice(&[0, 9]);
        chunk.extend_from_slice(&0u16.to_le_bytes());
        chunk.extend_from_slice(&5u32.to_le_bytes());
        chunk.extend_from_slice(b"abcd!");
        chunk.extend_from_slice(&0u32.to_le_bytes());
        let bank = AudioBank::from_chunk(&chunk).unwrap();
        assert!(bank.music(Music::Title).is_none());
        assert!(bank.sfx(Sfx::Shot).is_none());
    }

    #[test]
    fn rejects_foreign_and_future_chunks() {
        assert_eq!(
            AudioBank::from_chunk(b"nope").unwrap_err(),
            AudioChunkError::Truncated
        );
        assert_eq!(
            AudioBank::from_chunk(b"NOPE\x01\x00\x00\x00").unwrap_err(),
            AudioChunkError::Magic
        );
        let mut future = timelines_chunk();
        future[4..6].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(
            AudioBank::from_chunk(&future).unwrap_err(),
            AudioChunkError::Version(3)
        );
    }

    #[test]
    fn version_one_and_empty_driver_chunks_decode_without_a_driver() {
        // A version 1 payload has no trailing driver length at all.
        let mut old = timelines_chunk();
        old[4..6].copy_from_slice(&AUDIO_CHUNK_VERSION_TIMELINES.to_le_bytes());
        old.truncate(old.len() - 4);
        let bank = AudioBank::from_chunk(&old).expect("version 1 decodes");
        assert!(bank.driver_bank().is_none());
        assert_eq!(
            bank.music(Music::Title).map(|track| track.events),
            Some(&LEAD_IN[..])
        );

        // A version 2 chunk may carry an explicitly empty driver section.
        let bank = AudioBank::from_chunk(&timelines_chunk()).expect("current chunk decodes");
        assert!(bank.driver_bank().is_none());
        assert!(bank.driver().is_none());
        assert_eq!(
            bank.sfx(Sfx::Shot).map(|track| track.events),
            Some(&LEAD_IN[..])
        );
    }

    #[test]
    fn driver_section_round_trips_tables_and_sequences() {
        let source = DriverSource {
            base: 0xC000,
            request_addr: crate::driver::REQUEST_TABLE,
            request: vec![0xAB; REQUEST_TABLE_LEN],
            period_addr: crate::driver::PERIOD_TABLE,
            period: (0..crate::driver::PERIOD_TABLE_LEN as u8).collect(),
            channels: [
                (0x00B0, 0x00),
                (0x00C1, 0x04),
                (0x00D2, 0x08),
                (0x00E3, 0x0C),
            ],
            segments: vec![(0xC100, vec![0xD1, 0x7F, 0x00, 0x01, 0xFF])],
        };
        let (music, sfx) = synthetic_tracks();
        let chunk = encode_chunk(&music, &sfx, &encode_driver(&source));
        let bank = AudioBank::from_chunk(&chunk).expect("driver chunk decodes");
        let driver = bank.driver_bank().expect("driver section");
        assert_eq!(driver.base(), 0xC000);
        assert_eq!(driver.request_addr(), crate::driver::REQUEST_TABLE);
        assert_eq!(&driver.mem()[0x100..0x105], &[0xD1, 0x7F, 0x00, 0x01, 0xFF]);
        // Tables are present in the reconstructed image; the period table
        // overlaps the first three request bytes, exactly as in the cartridge.
        let request = usize::from(crate::driver::REQUEST_TABLE - 0xC000);
        assert_eq!(driver.mem()[request], 21);
        assert_eq!(driver.mem()[request + 3], 0xAB);
        assert_eq!(
            driver.mem()[usize::from(crate::driver::PERIOD_TABLE - 0xC000)],
            0
        );
        assert_eq!(driver.channel_reg(), [0, 4, 8, 12]);
        assert_eq!(
            driver.channel_blocks(),
            [(0xB0, 0), (0xC1, 4), (0xD2, 8), (0xE3, 12)]
        );
        assert_eq!(driver.channel_block_len(), 17);
    }

    #[test]
    fn malformed_driver_sections_are_rejected() {
        let mut chunk = timelines_chunk();
        let length_at = chunk.len() - 4;
        chunk[length_at..length_at + 4].copy_from_slice(&6u32.to_le_bytes());
        chunk.extend_from_slice(b"KDRV\x01\x00");
        assert_eq!(
            AudioBank::from_chunk(&chunk).unwrap_err(),
            AudioChunkError::Malformed("driver magic")
        );
    }
}
