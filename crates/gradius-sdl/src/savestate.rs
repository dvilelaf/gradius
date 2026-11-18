//! F5/F9 savestates from the windowed frontend.
//!
//! One slot, `savestate.grs`, in the data folder (see `datadir`). The file
//! holds the compact `GRSS` image the core writes, not a ring slot.
//!
//! A save goes to a temporary sibling first and is renamed over the slot, so a
//! crash or a full disk leaves the previous state intact. A load takes a
//! backup snapshot before touching the game: a corrupt file is reported and
//! the running game is exactly as it was.

use std::fs;
use std::path::{Path, PathBuf};

use gradius_app::Game;
use sdl2::keyboard::Scancode;

/// The slot's file name inside the data folder.
pub const FILE_NAME: &str = "savestate.grs";

/// What F5 and F9 do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Save,
    Load,
}

/// The savestate hotkeys, or `None` for every other key.
pub fn hotkey(scancode: Scancode) -> Option<Action> {
    match scancode {
        Scancode::F5 => Some(Action::Save),
        Scancode::F9 => Some(Action::Load),
        _ => None,
    }
}

/// The short on-screen message for an attempt. Errors are spelled out so a
/// missing or corrupt slot is visible without the console.
pub fn notice(action: Action, result: &Result<(), String>) -> &'static str {
    match (action, result.is_ok()) {
        (Action::Save, true) => "SAVED",
        (Action::Save, false) => "SAVE ERROR",
        (Action::Load, true) => "LOADED",
        (Action::Load, false) => "LOAD ERROR",
    }
}

/// Snapshot `game` into `path` through the temporary file.
pub fn save(game: &Game, path: &Path) -> Result<(), String> {
    write_atomic(path, &game.snapshot())
}

/// Read `path` into `game`; on any error the game keeps its current state.
pub fn load(game: &mut Game, path: &Path) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    restore(game, &bytes)
}

/// Restore `bytes` with a rollback: `restore` may fail after touching fields,
/// so the image of the current state is re-applied before reporting.
pub fn restore(game: &mut Game, bytes: &[u8]) -> Result<(), String> {
    let backup = game.snapshot();
    match game.restore(bytes) {
        Ok(()) => Ok(()),
        Err(error) => {
            // `backup` came from this very game, so re-applying it can only
            // fail if the core broke its own format.
            if let Err(rollback) = game.restore(&backup) {
                eprintln!("warning: savestate rollback failed: {rollback:?}");
            }
            Err(format!("cannot restore {}: {error:?}", FILE_NAME))
        }
    }
}

/// Write `bytes` to `path` via a temporary sibling and a rename. The rename is
/// atomic on the same filesystem, which `path` and its sibling always share.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = temp_path(path);
    if let Err(error) = fs::write(&temp, bytes) {
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot write {}: {error}", temp.display()));
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot replace {}: {error}", path.display()));
    }
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    use gradius_core::Buttons;

    fn temp_dir(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("gradius-savestate-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("create temp directory");
        directory
    }

    fn game() -> Game {
        // The game needs the pack's ROM data (`scripts/extract-data`).
        let path = std::env::var_os("GRADIUS_PACK").map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/gradius.pak"),
            PathBuf::from,
        );
        let pack = gradius_assets::Pack::load(&path).unwrap_or_else(|_| {
            panic!(
                "no extracted pack at {}: run GRADIUS_ROM=<rom> scripts/extract-data",
                path.display()
            )
        });
        Game::new(pack.into_assets())
    }

    #[test]
    fn f5_saves_and_f9_loads() {
        assert_eq!(hotkey(Scancode::F5), Some(Action::Save));
        assert_eq!(hotkey(Scancode::F9), Some(Action::Load));
        assert_eq!(hotkey(Scancode::Escape), None);
        assert_eq!(hotkey(Scancode::F3), None);
    }

    #[test]
    fn notices_name_each_outcome() {
        assert_eq!(notice(Action::Save, &Ok(())), "SAVED");
        assert_eq!(notice(Action::Load, &Ok(())), "LOADED");
        assert_eq!(notice(Action::Save, &Err(String::new())), "SAVE ERROR");
        assert_eq!(notice(Action::Load, &Err(String::new())), "LOAD ERROR");
    }

    #[test]
    fn writes_then_reads_and_leaves_no_temporary_file() {
        let slot = temp_dir("roundtrip").join(FILE_NAME);
        write_atomic(&slot, b"first").expect("write slot");
        write_atomic(&slot, b"second").expect("overwrite slot");

        assert_eq!(fs::read(&slot).expect("read slot"), b"second");
        assert!(!temp_path(&slot).exists(), "temporary file was left behind");
    }

    #[test]
    fn game_state_survives_save_then_load() {
        let slot = temp_dir("game").join(FILE_NAME);
        let mut game = game();
        game.step(Buttons::NONE);
        let saved = game.snapshot();
        save(&game, &slot).expect("save slot");

        game.step(Buttons::NONE);
        game.step(Buttons::NONE);
        assert_ne!(game.snapshot(), saved, "the game moved on");

        load(&mut game, &slot).expect("load slot");
        assert_eq!(game.snapshot(), saved);
    }

    #[test]
    fn corrupt_slot_is_rejected_and_the_game_is_unchanged() {
        let slot = temp_dir("corrupt").join(FILE_NAME);
        fs::write(&slot, b"not a GRSS image").expect("write corrupt slot");

        let mut game = game();
        game.step(Buttons::NONE);
        let before = game.snapshot();
        assert!(load(&mut game, &slot).is_err());
        assert_eq!(
            game.snapshot(),
            before,
            "a failed load must not change state"
        );
    }

    #[test]
    fn truncated_snapshot_is_rejected_and_the_game_is_unchanged() {
        let mut game = game();
        game.step(Buttons::NONE);
        let before = game.snapshot();
        let truncated = &before[..before.len() / 2];

        assert!(restore(&mut game, truncated).is_err());
        assert_eq!(
            game.snapshot(),
            before,
            "a failed restore must not change state"
        );
    }
}
