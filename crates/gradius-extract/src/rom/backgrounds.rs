//! `backgrounds` family of `CHUNK_ROM`: the oracle's displayed backgrounds.

use gradius_cart::Cartridge;
use gradius_core::rom::Backgrounds;

pub(crate) fn extract(cart: &Cartridge) -> Result<Backgrounds, String> {
    crate::screens::capture_backgrounds(cart).map_err(|error| format!("backgrounds: {error:#}"))
}
