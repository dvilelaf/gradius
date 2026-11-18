//! `collision` family of `CHUNK_ROM`: the packed `$0500`/`$0600` maps of the
//! seven stages (captured from the oracle) and the `$A7CA` terrain records.

use gradius_cart::Cartridge;
use gradius_core::rom::collision::{Terrain, MAP_LEN, STAGES};
use gradius_core::rom::Collision;
use gradius_nes::{Emulator, BUTTON_START};

use crate::oracle::{Plan, Runner};
use crate::sections;

/// Frames to run each stage: long enough for the boss-approach tail of the
/// slowest stage (stage 7 reloads its last page at frame 5928).
const FRAMES: u64 = 8000;
/// Start is pressed here, after the title is up, as `screens --press 180:start`.
const START_FRAME: u64 = 180;
/// Stage 5 paints no terrain: `$9B38` clears both pages at stage init and the
/// stage never writes them again, so what the pages hold while it scrolls is
/// recycled-window residue and its map is all zero.
const NO_TERRAIN_STAGE: usize = 4;
/// Stage 7 reaches the `$3F = $0C` clear trigger, so block 12 is captured. The
/// paired `$0600` page already holds block 13 at that moment; keeping only
/// block 12 left the last 512 px of the top wall non-solid, which the ship can
/// reach while the clear holds the scroll.
const STAGE7: usize = 6;
const STAGE7_LAST_BLOCK: usize = 12;
/// `$3F` blocks hold 32 columns; a block is 256 map bytes.
const BLOCK_LEN: usize = 256;
const BLOCKS: usize = MAP_LEN / BLOCK_LEN;

/// What one stage run saw: for every `$3F` block, each distinct content of its
/// `$0500`/`$0600` page with the first frame it was seen.
pub(crate) struct Capture {
    pages: Vec<Vec<(u64, [u8; BLOCK_LEN])>>,
}

/// Run the oracle through `stage` (zero-based `$19`), poking `$26` until it
/// plays and keeping the ship alive, and record the page of every block.
pub(crate) fn capture(cart: &Cartridge, stage: u8) -> Result<Capture, String> {
    let mut emulator = Emulator::new(cart).map_err(|error| error.to_string())?;
    let mut runner = Runner::new();
    runner.keep_alive = true;
    let plan = Plan {
        held: 0,
        schedule: vec![(START_FRAME, BUTTON_START, 6)],
    };
    let mut pages: Vec<Vec<(u64, [u8; BLOCK_LEN])>> = vec![Vec::new(); BLOCKS];
    while emulator.frames < FRAMES {
        let ram = &mut emulator.bus.ram;
        if stage > 0 && !(ram[0x00] == 5 && ram[0x19] == stage) {
            // Single-player: X = $18 is 0 at stage init.
            ram[0x26] = stage;
        }
        runner.advance(&mut emulator, &plan, false);
        let ram = &emulator.bus.ram;
        if ram[0x00] != 5 || ram[0x19] != stage || emulator.frames < 200 {
            continue;
        }
        let block = usize::from(ram[0x3F]);
        if block >= BLOCKS {
            continue;
        }
        let base = 0x0500 + 0x100 * (block & 1);
        let mut page = [0u8; BLOCK_LEN];
        page.copy_from_slice(&ram[base..base + BLOCK_LEN]);
        let seen = &mut pages[block];
        if seen.last().map(|(_, last)| last) != Some(&page) {
            seen.push((emulator.frames, page));
        }
        // Stage 7's last block never gets a `$3F` of its own: the clear holds
        // the scroll at `$0C`, but the paired page at that moment already
        // carries block 13's top wall.
        if stage as usize == STAGE7 && block == STAGE7_LAST_BLOCK {
            let other = 0x0500 + 0x100 * ((block + 1) & 1);
            let mut next = [0u8; BLOCK_LEN];
            next.copy_from_slice(&ram[other..other + BLOCK_LEN]);
            let seen = &mut pages[block + 1];
            if seen.last().map(|(_, last)| last) != Some(&next) {
                seen.push((emulator.frames, next));
            }
        }
    }
    Ok(Capture { pages })
}

pub(crate) fn extract(cart: &Cartridge) -> Result<Collision, String> {
    let terrain = sections::decode(&cart.prg).map_err(|error| error.to_string())?;
    let mut collision = Collision {
        threshold: terrain.thresholds,
        terrain: terrain.terrain.map(|records| {
            records
                .into_iter()
                .map(|record| Terrain {
                    column: record.column,
                    kind: record.kind,
                })
                .collect()
        }),
        maps: vec![vec![0; MAP_LEN]; STAGES],
    };
    for (stage, map) in collision.maps.iter_mut().enumerate() {
        let capture = capture(cart, stage as u8)?;
        if stage != NO_TERRAIN_STAGE {
            *map = first_value(&capture);
        }
    }
    Ok(collision)
}

/// First value seen for each byte of a block, the rule the stage-2..7 maps use.
/// A block's first capture is its reload: the page is read the first frame
/// `$3F` reaches the block (stage 7's block 11 at frame 5928, before the clear
/// path writes `$FF` at 5949).
fn first_value(capture: &Capture) -> Vec<u8> {
    let mut map = vec![0; MAP_LEN];
    for (block, seen) in capture.pages.iter().enumerate() {
        if let Some((_, page)) = seen.first() {
            map[block * BLOCK_LEN..][..BLOCK_LEN].copy_from_slice(page);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The capture follows the oracle only, so two runs give the same bytes.
    #[test]
    fn extraction_is_deterministic() {
        let Some(rom) = std::env::var_os("GRADIUS_ROM") else {
            return;
        };
        let cart = crate::load_cart(&std::fs::read(rom).unwrap()).unwrap();
        let first = capture(&cart, 6).unwrap();
        let second = capture(&cart, 6).unwrap();
        assert_eq!(first_value(&first), first_value(&second));
    }
}
