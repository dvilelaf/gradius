//! The extracted pack's audio, shared by the integration tests.

use std::path::PathBuf;
use std::sync::OnceLock;

use gradius_audio::{AudioBank, Music, Sfx, Timeline};

/// `GRADIUS_PACK`, or `assets/gradius.pak` at the workspace root.
pub fn pack_path() -> PathBuf {
    std::env::var_os("GRADIUS_PACK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak")
        })
}

/// The pack's audio chunk bytes; panics when the pack has not been extracted.
pub fn chunk() -> &'static [u8] {
    static CHUNK: OnceLock<Vec<u8>> = OnceLock::new();
    CHUNK.get_or_init(|| {
        let bytes = std::fs::read(pack_path()).unwrap_or_else(|_| {
            panic!("no extracted pack: run GRADIUS_ROM=<rom> scripts/extract-data")
        });
        let pack = gradius_assets::Pack::parse(&bytes).expect("pack parses");
        pack.audio.expect("the pack has no audio chunk")
    })
}

/// The bank decoded from the pack (decoded once per process).
pub fn bank() -> &'static AudioBank {
    static BANK: OnceLock<AudioBank> = OnceLock::new();
    BANK.get_or_init(|| AudioBank::from_chunk(chunk()).expect("audio chunk decodes"))
}

pub fn music(music: Music) -> &'static Timeline {
    bank()
        .music(music)
        .unwrap_or_else(|| panic!("the pack is missing {music:?}"))
}

pub fn sfx(sfx: Sfx) -> &'static Timeline {
    bank()
        .sfx(sfx)
        .unwrap_or_else(|| panic!("the pack is missing {sfx:?}"))
}
