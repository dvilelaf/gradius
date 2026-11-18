//! `--native-keep-alive` option handling.
//!
//! These tests never open a cartridge: they only check that the flag is part of
//! the documented interface and that the parser accepts it, so they run on CI.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
}

fn garbage_rom(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("gradius-nka-{name}-{}.nes", std::process::id()));
    std::fs::write(&path, b"not an iNES file").expect("write scratch ROM");
    path
}

#[test]
fn help_documents_native_keep_alive() {
    let output = bin().arg("--help").output().expect("failed to run --help");
    assert!(output.status.success(), "--help did not succeed");
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.contains("--native-keep-alive"),
        "help is missing --native-keep-alive:\n{stdout}"
    );
}

#[test]
fn native_keep_alive_is_parsed_as_a_flag() {
    let rom = garbage_rom("flag");
    let output = bin()
        .arg("--rom")
        .arg(&rom)
        .args([
            "--pack",
            "missing.pak",
            "--frames",
            "1",
            "--native-keep-alive",
        ])
        .output()
        .expect("failed to run gradius-diff");
    let _ = std::fs::remove_file(&rom);
    assert_eq!(output.status.code(), Some(2), "expected a usage/IO error");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(
        stderr.contains("cannot parse ROM"),
        "the flag was not accepted before the ROM was read: {stderr}"
    );
    assert!(
        !stderr.contains("unknown argument"),
        "the flag reached the argument parser as unknown: {stderr}"
    );
}

#[test]
fn keep_alive_flags_are_independent() {
    let rom = garbage_rom("both");
    let output = bin()
        .arg("--rom")
        .arg(&rom)
        .args([
            "--pack",
            "missing.pak",
            "--frames",
            "1",
            "--keep-alive",
            "--native-keep-alive",
        ])
        .output()
        .expect("failed to run gradius-diff");
    let _ = std::fs::remove_file(&rom);
    assert_eq!(output.status.code(), Some(2), "expected a usage/IO error");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(
        stderr.contains("cannot parse ROM"),
        "both keep-alive flags together were not accepted: {stderr}"
    );
}
