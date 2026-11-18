//! Backgrounds for `CHUNK_ROM`: the oracle capture of `xtask screens`, one run
//! per stage, folded into `gradius_core::rom::Backgrounds`.

use anyhow::Result;
use gradius_cart::Cartridge;
use gradius_core::rom::backgrounds::{Backgrounds, RomScreen};
use gradius_nes::BUTTON_START;

use crate::oracle::Plan;

use super::{capture, classify_solid, ScreenData};

/// `--frames` of the stage-1 run (`screens --frames 7800 --assist --press 180:start`).
const STAGE1_FRAMES: u64 = 7800;
/// `--frames` of the stage 2-7 runs (`--frames 20000 --assist --stage N --press 180:start`).
const STAGE_FRAMES: u64 = 20000;
const STAGES: u8 = 7;

fn rom_screen(screen: &ScreenData) -> RomScreen {
    RomScreen {
        width_tiles: screen.width_tiles,
        height_tiles: screen.height_tiles,
        tiles: screen.tiles.clone(),
        attributes: screen.attributes.clone(),
        palette: screen.palette,
        chr_bank: screen.chr_bank,
    }
}

/// Run the oracle for every stage (in parallel with the `parallel` feature:
/// each run owns its emulator).
pub fn capture_backgrounds(cart: &Cartridge) -> Result<Backgrounds> {
    let plan = Plan {
        held: 0,
        schedule: vec![(180, BUTTON_START, 6)],
    };
    let run_stage = |stage: u8| {
        let frames = if stage == 0 {
            STAGE1_FRAMES
        } else {
            STAGE_FRAMES
        };
        capture(cart, frames, &plan, true, true, stage, &[])
            .map(|(title, screen, classify, _)| (title, screen, classify))
    };
    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    let runs: Vec<Result<(ScreenData, ScreenData, ScreenData)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..STAGES)
            .map(|stage| {
                let run_stage = &run_stage;
                scope.spawn(move || run_stage(stage))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("background capture panicked")))
            })
            .collect()
    });
    #[cfg(not(all(feature = "parallel", not(target_arch = "wasm32"))))]
    let runs: Vec<Result<(ScreenData, ScreenData, ScreenData)>> =
        (0..STAGES).map(run_stage).collect();
    let mut runs = runs.into_iter();
    let (title, stage1, classify) = runs.next().expect("stage 1 run")?;
    let mut stages = vec![rom_screen(&stage1)];
    for run in runs {
        stages.push(rom_screen(&run?.1));
    }
    let mut solid = [0u8; 256];
    for (flag, solid_tile) in solid.iter_mut().zip(classify_solid(&classify)) {
        *flag = u8::from(solid_tile);
    }
    Ok(Backgrounds {
        title: rom_screen(&title),
        stages,
        solid,
    })
}
