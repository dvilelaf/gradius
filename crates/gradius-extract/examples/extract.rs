//! `cargo run --release -p gradius-extract --example extract -- <rom> <out.pak>`
//!
//! Add `--no-default-features` for the sequential build; the pack is identical.

use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(rom), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: extract <rom> <out.pak>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&rom).expect("reading the ROM");
    let started = Instant::now();
    match gradius_extract::extract(&bytes, &mut |_, _, _| {}) {
        Ok(pack) => {
            std::fs::write(&out, &pack).expect("writing the pack");
            println!(
                "{} bytes in {:.1}s",
                pack.len(),
                started.elapsed().as_secs_f32()
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
