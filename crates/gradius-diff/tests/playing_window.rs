//! End-to-end probe of the harness against the real cartridge.
//!
//! Skipped unless `GRADIUS_ROM` points at the cartridge *and* the workspace
//! asset pack exists, so CI never needs either. The ROM is only read; the
//! summary goes to a temporary file that is deleted again.

use std::path::PathBuf;
use std::process::Command;

/// Every field the report is expected to check during stage 1.
const FIELDS: [&str; 12] = [
    "lives",
    "meter",
    "obj.kind",
    "obj.slot",
    "obj.substate",
    "obj.x",
    "obj.y",
    "px",
    "py",
    "score",
    "scroll",
    "speed",
];

fn rom() -> Option<PathBuf> {
    std::env::var_os("GRADIUS_ROM").map(PathBuf::from)
}

fn pack() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak");
    path.is_file().then_some(path)
}

#[test]
fn six_hundred_frames_emit_every_reported_field() {
    let (Some(rom), Some(pack)) = (rom(), pack()) else {
        eprintln!("GRADIUS_ROM or assets/gradius.pak missing; skipping cartridge probe");
        return;
    };
    let json = std::env::temp_dir().join(format!("gradius-diff-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_gradius-diff"))
        .arg("--rom")
        .arg(&rom)
        .arg("--pack")
        .arg(&pack)
        .args(["--frames", "600", "--json"])
        .arg(&json)
        .output()
        .expect("failed to run gradius-diff");
    let status = output.status.code().expect("no exit code");
    assert!(
        matches!(status, 0 | 1),
        "harness failed with {status}: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(stdout.contains("600 frames run"), "{stdout}");
    assert!(stdout.contains("Playing frames"), "{stdout}");
    assert!(stdout.contains("first divergence:"), "{stdout}");
    assert!(stdout.contains("position error:"), "{stdout}");
    assert!(stdout.contains("objects: slot match"), "{stdout}");
    for field in FIELDS {
        assert!(
            stdout.contains(field),
            "stdout is missing {field}:\n{stdout}"
        );
    }

    let text = std::fs::read_to_string(&json).expect("cannot read the JSON summary");
    let _ = std::fs::remove_file(&json);
    assert!(json_well_formed(&text), "invalid JSON:\n{text}");
    for key in [
        "\"frames\"",
        "\"compared\"",
        "\"window\"",
        "\"fields\"",
        "\"first_divergence\"",
        "\"position\"",
        "\"objects\"",
    ] {
        assert!(text.contains(key), "JSON is missing {key}:\n{text}");
    }
    for field in FIELDS {
        assert!(
            text.contains(&format!("\"{field}\"")),
            "JSON is missing {field}:\n{text}"
        );
    }
    let compared: u64 = text
        .split("\"compared\": ")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .and_then(|value| value.trim().parse().ok())
        .expect("JSON compared count");
    assert!(compared > 0, "no Playing frames were compared:\n{text}");
    assert!(!text.contains("\"first\": null"), "{text}");
}

/// A small JSON well-formedness check; adding a parser dependency for one
/// assertion is not worth it.
fn json_well_formed(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    value(bytes, &mut index) && {
        skip_ws(bytes, &mut index);
        index == bytes.len()
    }
}

fn skip_ws(bytes: &[u8], index: &mut usize) {
    while matches!(bytes.get(*index), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        *index += 1;
    }
}

fn value(bytes: &[u8], index: &mut usize) -> bool {
    skip_ws(bytes, index);
    match bytes.get(*index) {
        Some(b'{') => object(bytes, index),
        Some(b'[') => array(bytes, index),
        Some(b'"') => string(bytes, index),
        Some(b't') => literal(bytes, index, b"true"),
        Some(b'f') => literal(bytes, index, b"false"),
        Some(b'n') => literal(bytes, index, b"null"),
        Some(_) => number(bytes, index),
        None => false,
    }
}

fn object(bytes: &[u8], index: &mut usize) -> bool {
    *index += 1;
    skip_ws(bytes, index);
    if bytes.get(*index) == Some(&b'}') {
        *index += 1;
        return true;
    }
    loop {
        skip_ws(bytes, index);
        if !string(bytes, index) {
            return false;
        }
        skip_ws(bytes, index);
        if bytes.get(*index) != Some(&b':') {
            return false;
        }
        *index += 1;
        if !value(bytes, index) {
            return false;
        }
        skip_ws(bytes, index);
        match bytes.get(*index) {
            Some(b',') => *index += 1,
            Some(b'}') => {
                *index += 1;
                return true;
            }
            _ => return false,
        }
    }
}

fn array(bytes: &[u8], index: &mut usize) -> bool {
    *index += 1;
    skip_ws(bytes, index);
    if bytes.get(*index) == Some(&b']') {
        *index += 1;
        return true;
    }
    loop {
        if !value(bytes, index) {
            return false;
        }
        skip_ws(bytes, index);
        match bytes.get(*index) {
            Some(b',') => *index += 1,
            Some(b']') => {
                *index += 1;
                return true;
            }
            _ => return false,
        }
    }
}

fn string(bytes: &[u8], index: &mut usize) -> bool {
    if bytes.get(*index) != Some(&b'"') {
        return false;
    }
    *index += 1;
    while let Some(&byte) = bytes.get(*index) {
        match byte {
            b'"' => {
                *index += 1;
                return true;
            }
            b'\\' => *index += 2,
            _ => *index += 1,
        }
    }
    false
}

fn number(bytes: &[u8], index: &mut usize) -> bool {
    let start = *index;
    while matches!(
        bytes.get(*index),
        Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
    ) {
        *index += 1;
    }
    std::str::from_utf8(&bytes[start..*index]).is_ok_and(|slice| slice.parse::<f64>().is_ok())
}

fn literal(bytes: &[u8], index: &mut usize, word: &[u8]) -> bool {
    if bytes[*index..].starts_with(word) {
        *index += word.len();
        true
    } else {
        false
    }
}
