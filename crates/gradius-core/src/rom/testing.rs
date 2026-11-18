//! The extracted ROM data for tests: read from the pack `xtask extract` wrote.

use std::path::PathBuf;
use std::sync::OnceLock;

use super::RomData;

pub fn rom() -> &'static RomData {
    static ROM: OnceLock<&'static RomData> = OnceLock::new();
    ROM.get_or_init(|| {
        let path = std::env::var_os("GRADIUS_PACK").map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak"),
            PathBuf::from,
        );
        let missing =
            || -> ! { panic!("no extracted pack: run GRADIUS_ROM=<rom> scripts/extract-data") };
        let payload = gradius_pack::read_chunk_from_file(&path, gradius_pack::CHUNK_ROM)
            .unwrap_or_else(|_| missing());
        RomData::decode(&payload)
            .unwrap_or_else(|error| panic!("{error}: rerun scripts/extract-data"))
            .leak()
    })
}
