//! `--stage` option handling.
//!
//! These tests never open a cartridge: they only check that the flag is part
//! of the documented interface, that it is validated, and that it composes
//! with the pixel report, so they run on CI.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
}

fn garbage_rom(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("gradius-stage-{name}-{}.nes", std::process::id()));
    std::fs::write(&path, b"not an iNES file").expect("write scratch ROM");
    path
}

#[test]
fn help_documents_the_stage_option() {
    let output = bin().arg("--help").output().expect("failed to run --help");
    assert!(output.status.success(), "--help did not succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.contains("--stage N"),
        "help is missing --stage:\n{stdout}"
    );
}

#[test]
fn stage_is_accepted_before_the_rom_is_read() {
    let rom = garbage_rom("accepted");
    let output = bin()
        .arg("--rom")
        .arg(&rom)
        .args(["--pack", "missing.pak", "--frames", "1", "--stage", "2"])
        .output()
        .expect("failed to run gradius-diff");
    let _ = std::fs::remove_file(&rom);
    assert_eq!(output.status.code(), Some(2), "expected a usage/IO error");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(
        stderr.contains("cannot parse ROM"),
        "the flag was not accepted before the ROM was read: {stderr}"
    );
}

#[test]
fn stage_is_validated_to_one_through_seven() {
    for (value, expected) in [("0", "1-7"), ("8", "1-7"), ("x", "bad --stage")] {
        let rom = garbage_rom(value);
        let output = bin()
            .arg("--rom")
            .arg(&rom)
            .args(["--pack", "missing.pak", "--frames", "1", "--stage", value])
            .output()
            .expect("failed to run gradius-diff");
        let _ = std::fs::remove_file(&rom);
        assert_eq!(output.status.code(), Some(2), "--stage {value}");
        let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
        assert!(stderr.contains(expected), "--stage {value}: {stderr}");
    }
}

#[test]
fn stage_composes_with_the_pixel_window() {
    let rom = garbage_rom("pixels");
    let output = bin()
        .arg("--rom")
        .arg(&rom)
        .args([
            "--pack",
            "missing.pak",
            "--frames",
            "1",
            "--stage",
            "2",
            "--pixel-report",
            "--pixel-window",
            "9600:12000",
        ])
        .output()
        .expect("failed to run gradius-diff");
    let _ = std::fs::remove_file(&rom);
    assert_eq!(output.status.code(), Some(2), "expected a usage/IO error");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(
        stderr.contains("cannot parse ROM"),
        "--stage/--pixel-window not accepted together: {stderr}"
    );
}
