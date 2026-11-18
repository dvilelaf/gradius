//! Loop timers for `scripts/bench.sh`.
//!
//! Built with `rustc` against the workspace's release rlibs, so it runs the
//! same optimized code as the shipping binaries without adding a crate to the
//! workspace. Two modes:
//!
//!   gradius-bench core PACK [FRAMES]    Game::step in a headless loop
//!   gradius-bench oracle ROM [FRAMES]   the gradius-nes reference emulator

use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: gradius-bench core|oracle PATH [FRAMES]");
        std::process::exit(2);
    }
    let frames: u64 = args
        .get(3)
        .map(|value| value.parse().expect("frames must be a number"))
        .unwrap_or(10_000);
    match args[1].as_str() {
        "core" => core(&args[2], frames),
        "oracle" => oracle(&args[2], frames),
        other => {
            eprintln!("gradius-bench: unknown mode {other}");
            std::process::exit(2);
        }
    }
}

fn core(path: &str, frames: u64) {
    let bytes = std::fs::read(path).expect("read pack");
    let pack = gradius_assets::Pack::parse(&bytes).expect("parse pack");
    let mut game = gradius_core::Game::new(pack.into_assets());

    let start = Instant::now();
    for _ in 0..frames {
        game.step(gradius_core::Buttons::NONE);
    }
    let elapsed = start.elapsed();
    println!(
        "core frames={} loop_ms={:.3} hash={:#018x}",
        game.frame,
        elapsed.as_secs_f64() * 1e3,
        game.state_hash(),
    );
}

fn oracle(path: &str, frames: u64) {
    let bytes = std::fs::read(path).expect("read ROM");
    let cart = gradius_cart::Cartridge::from_ines(&bytes).expect("parse iNES");
    let mut emulator = gradius_nes::Emulator::new(&cart).expect("build emulator");
    // Captured samples are not part of a run's cost; the APU still steps.
    emulator.set_audio_enabled(false);

    let start = Instant::now();
    for _ in 0..frames {
        emulator.run_frame();
    }
    let elapsed = start.elapsed();
    println!(
        "oracle frames={} loop_ms={:.3} hash={:#018x}",
        emulator.frames,
        elapsed.as_secs_f64() * 1e3,
        emulator.frame_hash(),
    );
}
