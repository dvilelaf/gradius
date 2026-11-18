//! The extracted asset pack, for tests that build a [`gradius_core::Game`].
//!
//! A missing pack is a failure, not a skip: `scripts/extract-data` writes it.

use std::path::PathBuf;
use std::sync::OnceLock;

use gradius_assets::Pack;
use gradius_core::Assets;

/// `GRADIUS_PACK` when it is set, otherwise the workspace's `assets/gradius.pak`.
pub fn pack_path() -> PathBuf {
    std::env::var_os("GRADIUS_PACK").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak"),
        PathBuf::from,
    )
}

/// The assets from the extracted pack. The pack is decoded once per process.
///
/// # Panics
///
/// If the pack is missing or does not parse.
pub fn assets() -> Assets {
    static ASSETS: OnceLock<Assets> = OnceLock::new();
    ASSETS
        .get_or_init(|| {
            let path = pack_path();
            Pack::load(&path)
                .unwrap_or_else(|_| {
                    panic!(
                        "no extracted pack at {}: run GRADIUS_ROM=<rom> scripts/extract-data",
                        path.display()
                    )
                })
                .into_assets()
        })
        .clone()
}
