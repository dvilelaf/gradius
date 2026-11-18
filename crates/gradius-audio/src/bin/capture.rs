//! Development-only report of the audio capture.
//!
//! Runs the same capture `xtask extract` uses and prints each track's length;
//! it never writes anything. The timelines reach the game only through the
//! pack's audio chunk (`GRADIUS_ROM=<rom> scripts/extract-data`).
//!
//! ```text
//! cargo run -p gradius-audio --features capture --bin capture-audio -- "$GRADIUS_ROM"
//! ```

use gradius_audio::capture::capture_all;
use gradius_audio::{Music, Sfx};
use gradius_cart::Cartridge;

fn main() {
    if let Err(error) = run() {
        eprintln!("capture-audio: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let rom = std::env::args_os()
        .nth(1)
        .or_else(|| std::env::var_os("GRADIUS_ROM"))
        .ok_or("pass the cartridge path or set GRADIUS_ROM")?;
    let data =
        std::fs::read(&rom).map_err(|error| format!("{}: {error}", rom.to_string_lossy()))?;
    let cart = Cartridge::from_ines(&data).map_err(|error| error.to_string())?;
    let capture = capture_all(&cart)?;
    let tracks = Music::ALL
        .iter()
        .map(|music| music.name())
        .zip(&capture.music)
        .chain(Sfx::ALL.iter().map(|sfx| sfx.name()).zip(&capture.sfx));
    for (name, timeline) in tracks {
        println!(
            "{name:<16} {:>5} writes / {:>5.2}s{}",
            timeline.events.len(),
            timeline.seconds(),
            if timeline.is_looping() {
                " (loops)"
            } else {
                ""
            }
        );
    }
    println!("{} register writes in total", capture.events());
    Ok(())
}
