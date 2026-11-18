//! Extraction of the `CHUNK_ROM` data from the cartridge.

use gradius_cart::Cartridge;
use gradius_core::rom::RomData;

use crate::Step;

mod backgrounds;
mod collision;
mod spawns;
mod sprites;
mod tables;

pub(crate) fn extract(
    cart: &Cartridge,
    progress: &mut dyn FnMut(Step, u32, u32),
) -> Result<RomData, String> {
    progress(Step::Collision, 0, 1);
    let collision = collision::extract(cart)?;
    progress(Step::Collision, 1, 1);
    progress(Step::Backgrounds, 0, 1);
    let backgrounds = backgrounds::extract(cart)?;
    progress(Step::Backgrounds, 1, 1);
    progress(Step::Sprites, 0, 1);
    let sprites = sprites::extract(cart)?;
    progress(Step::Sprites, 1, 1);
    progress(Step::Spawns, 0, 1);
    let spawns = spawns::extract(cart)?;
    progress(Step::Spawns, 1, 1);
    progress(Step::Tables, 0, 1);
    let tables = tables::extract(cart)?;
    progress(Step::Tables, 1, 1);
    Ok(RomData {
        collision,
        backgrounds,
        sprites,
        spawns,
        tables,
    })
}
