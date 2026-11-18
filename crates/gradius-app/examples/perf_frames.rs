//! Per-frame cost of `Game::step` for `docs/PERFORMANCE.md`.
//!
//!   cargo run --release -p gradius-app --example perf_frames -- \
//!       assets/gradius.pak --frames 10000 --repeat 3
//!   cargo run --release -p gradius-app --example perf_frames -- \
//!       assets/gradius.pak --frames 10000 --stage 5 --warmup 600
//!
//! Each repeat builds a fresh game, optionally warms it up, and trades one
//! `Instant` pair per frame for the per-frame distribution the docs report
//! (median, p95). A counting global allocator records allocation calls made
//! inside the timed loop, so "no allocation per frame" is measured, not
//! assumed. `dump_state` is only used in a separate, untimed replay to count
//! frames with live drawable objects.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gradius_app::{Buttons, Game};
use gradius_assets::Pack;
use gradius_core::Assets;

struct Counting;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: forwards every call to the system allocator; the counters are the
// only added work.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

struct Args {
    pack: String,
    frames: usize,
    repeat: usize,
    stage: Option<u8>,
    warmup: usize,
}

fn args() -> Args {
    let mut args = Args {
        pack: String::new(),
        frames: 10_000,
        repeat: 3,
        stage: None,
        warmup: 0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--frames" => args.frames = it.next().expect("--frames value").parse().expect("frames"),
            "--repeat" => args.repeat = it.next().expect("--repeat value").parse().expect("repeat"),
            // 1-based like the rest of the tooling; `$19` is N - 1.
            "--stage" => {
                let stage: u8 = it.next().expect("--stage value").parse().expect("stage");
                args.stage = Some(stage - 1);
            }
            "--warmup" => args.warmup = it.next().expect("--warmup value").parse().expect("warmup"),
            other => args.pack = other.to_string(),
        }
    }
    assert!(!args.pack.is_empty(), "usage: perf_frames PACK [options]");
    assert!(
        args.frames > 0 && args.repeat > 0,
        "frames and repeat must be positive"
    );
    args
}

fn build(assets: &Assets, stage: Option<u8>) -> Game {
    match stage {
        Some(stage) => {
            let mut game = Game::new_at_stage(assets.clone(), stage);
            game.dev_keep_alive(true);
            game
        }
        None => Game::new(assets.clone()),
    }
}

fn step(game: &mut Game, frames: usize) {
    for _ in 0..frames {
        game.step(Buttons::NONE);
    }
}

fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    // Nearest rank: the smallest value with at least `percent`% at or below it.
    let rank = (sorted.len() * percent).div_ceil(100).max(1);
    sorted[rank - 1]
}

/// Untimed replay: how many measured frames had at least one drawable object.
fn object_frames(assets: &Assets, args: &Args) -> (usize, usize) {
    let mut game = build(assets, args.stage);
    step(&mut game, args.warmup);
    let mut frames = 0;
    let mut max = 0;
    for _ in 0..args.frames * args.repeat {
        game.step(Buttons::NONE);
        let state = game.dump_state();
        let rest = state.split("objects=").nth(1).unwrap_or("");
        let objects = if rest.is_empty() {
            0
        } else {
            rest.split(';').count()
        };
        if objects > 0 {
            frames += 1;
        }
        max = max.max(objects);
    }
    (frames, max)
}

fn main() {
    let args = args();
    let bytes = std::fs::read(&args.pack).expect("read pack");
    let assets = Pack::parse(&bytes).expect("parse pack").into_assets();

    let mut samples = Vec::with_capacity(args.frames * args.repeat);
    let mut allocations = 0u64;
    let mut allocated = 0u64;
    let mut alloc_frames = 0usize;

    for _ in 0..args.repeat {
        let mut game = build(&assets, args.stage);
        step(&mut game, args.warmup);
        let allocs_before = ALLOCS.load(Ordering::Relaxed);
        let bytes_before = BYTES.load(Ordering::Relaxed);
        for _ in 0..args.frames {
            let frame_allocs = ALLOCS.load(Ordering::Relaxed);
            let start = Instant::now();
            game.step(Buttons::NONE);
            samples.push(start.elapsed());
            if ALLOCS.load(Ordering::Relaxed) != frame_allocs {
                alloc_frames += 1;
            }
        }
        allocations += ALLOCS.load(Ordering::Relaxed) - allocs_before;
        allocated += BYTES.load(Ordering::Relaxed) - bytes_before;
    }
    let total: Duration = samples.iter().sum();

    samples.sort_unstable();
    let mean = total.as_secs_f64() * 1e6 / samples.len() as f64;
    let (with_objects, max_objects) = object_frames(&assets, &args);
    let label = match args.stage {
        Some(stage) => format!("stage{}", stage + 1),
        None => "initial".to_string(),
    };
    println!(
        "workload={label} frames={} warmup={} median_us={:.3} p95_us={:.3} mean_us={:.3} total_ms={:.3} allocs={} alloc_bytes={} alloc_frames={} frames_with_objects={}/{} max_objects={}",
        samples.len(),
        args.warmup,
        percentile(&samples, 50).as_secs_f64() * 1e6,
        percentile(&samples, 95).as_secs_f64() * 1e6,
        mean,
        total.as_secs_f64() * 1e3,
        allocations,
        allocated,
        alloc_frames,
        with_objects,
        args.frames * args.repeat,
        max_objects,
    );
}
