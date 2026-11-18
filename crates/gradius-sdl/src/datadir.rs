//! Where the player's data lives: `gradius.pak`, `savestate.grs`, `keybinds.ini`.
//!
//! The first rule that matches wins (the same ideas as LAS Recomp's
//! `runtime/paths.c`):
//! 1. `GRADIUS_DATA` names the folder.
//! 2. Development: the working directory has `assets/gradius.pak`. Everything
//!    stays where it has always been.
//! 3. Portable: the folder of the executable has `gradius.pak` or a file named
//!    `portable`.
//! 4. Windows: the folder of the executable (a zip, no installer).
//! 5. Linux and the rest: `$XDG_DATA_HOME/gradius`, or `~/.local/share/gradius`.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use crate::keybinds;
use crate::savestate;
use crate::settings;

/// The asset pack's file name outside development.
pub const PACK_FILE_NAME: &str = "gradius.pak";

const DEV_PACK: &str = "assets/gradius.pak";
const DEV_KEYBINDS: &str = "assets/keybinds.ini";
const DEV_SETTINGS: &str = "assets/settings.ini";
const PORTABLE_MARKER: &str = "portable";

/// The rule that picked the folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Env,
    Dev,
    Portable,
    User,
}

/// The files the frontend reads and writes, and the rule that chose them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataDir {
    pub root: PathBuf,
    pub pack: PathBuf,
    pub savestate: PathBuf,
    pub keybinds: PathBuf,
    pub settings: PathBuf,
    pub mode: Mode,
}

impl DataDir {
    /// Resolve the folder for the real process: its environment, working
    /// directory and executable. Creates the folder in the `Env` and `User`
    /// modes, which are the ones that do not already exist.
    pub fn from_process() -> io::Result<DataDir> {
        let cwd = std::env::current_dir()?;
        let exe = std::env::current_exe()?;
        let exe_dir = exe.parent().map_or_else(|| cwd.clone(), Path::to_path_buf);
        let data = resolve(&|key| std::env::var_os(key), &cwd, &exe_dir, cfg!(windows));
        data.create_root()?;
        Ok(data)
    }

    /// Create the folder for the `Env` and `User` modes. The others belong
    /// to the user already.
    pub fn create_root(&self) -> io::Result<()> {
        match self.mode {
            Mode::Env | Mode::User => std::fs::create_dir_all(&self.root),
            Mode::Dev | Mode::Portable => Ok(()),
        }
    }

    /// `--pack` replaces the pack and nothing else.
    pub fn with_pack(mut self, pack: PathBuf) -> Self {
        self.pack = pack;
        self
    }

    fn in_root(root: PathBuf, mode: Mode) -> Self {
        Self {
            pack: root.join(PACK_FILE_NAME),
            savestate: root.join(savestate::FILE_NAME),
            keybinds: root.join(keybinds::FILE_NAME),
            settings: root.join(settings::FILE_NAME),
            root,
            mode,
        }
    }
}

/// Pick the data folder by the rules in the module docs. `env` reads the
/// environment, `cwd` is the working directory, `exe_dir` the folder of the
/// executable and `windows` says whether the target is Windows.
pub fn resolve(
    env: &dyn Fn(&str) -> Option<OsString>,
    cwd: &Path,
    exe_dir: &Path,
    windows: bool,
) -> DataDir {
    if let Some(root) = env("GRADIUS_DATA").filter(|value| !value.is_empty()) {
        return DataDir::in_root(PathBuf::from(root), Mode::Env);
    }
    if cwd.join(DEV_PACK).is_file() {
        return DataDir {
            root: cwd.to_path_buf(),
            pack: cwd.join(DEV_PACK),
            savestate: cwd.join(savestate::FILE_NAME),
            keybinds: cwd.join(DEV_KEYBINDS),
            settings: cwd.join(DEV_SETTINGS),
            mode: Mode::Dev,
        };
    }
    if windows || exe_dir.join(PACK_FILE_NAME).is_file() || exe_dir.join(PORTABLE_MARKER).exists() {
        return DataDir::in_root(exe_dir.to_path_buf(), Mode::Portable);
    }
    let base = match env("XDG_DATA_HOME").filter(|value| Path::new(value).is_absolute()) {
        Some(xdg) => PathBuf::from(xdg),
        None => match env("HOME").filter(|value| !value.is_empty()) {
            Some(home) => PathBuf::from(home).join(".local/share"),
            None => PathBuf::from("/tmp/.local/share"),
        },
    };
    DataDir::in_root(base.join("gradius"), Mode::User)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the temp dir, unique per test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gradius-datadir-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp directory");
        dir
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        std::fs::write(path, b"x").expect("write file");
    }

    /// A simulated environment holding only `vars`.
    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key| {
            vars.iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn env_variable_wins_over_everything() {
        let base = temp_dir("env-wins");
        let cwd = base.join("cwd");
        let exe = base.join("exe");
        touch(&cwd.join("assets/gradius.pak"));
        touch(&exe.join("gradius.pak"));
        let target = base.join("chosen");

        let vars = [
            ("GRADIUS_DATA", target.to_str().expect("utf-8")),
            ("XDG_DATA_HOME", "/xdg"),
        ];
        let data = resolve(&env(&vars), &cwd, &exe, true);

        assert_eq!(data.mode, Mode::Env);
        assert_eq!(data.root, target);
        assert_eq!(data.pack, target.join("gradius.pak"));
        assert_eq!(data.savestate, target.join("savestate.grs"));
        assert_eq!(data.keybinds, target.join("keybinds.ini"));
    }

    #[test]
    fn empty_env_variable_is_ignored() {
        let base = temp_dir("env-empty");
        touch(&base.join("cwd/assets/gradius.pak"));
        let vars = [("GRADIUS_DATA", "")];
        let data = resolve(&env(&vars), &base.join("cwd"), &base.join("exe"), false);
        assert_eq!(data.mode, Mode::Dev);
    }

    #[test]
    fn development_uses_the_working_directory() {
        let base = temp_dir("dev");
        let cwd = base.join("checkout");
        touch(&cwd.join("assets/gradius.pak"));
        let vars = [("XDG_DATA_HOME", "/xdg"), ("HOME", "/home/x")];
        let data = resolve(&env(&vars), &cwd, &base.join("exe"), false);

        assert_eq!(data.mode, Mode::Dev);
        assert_eq!(data.root, cwd);
        assert_eq!(data.pack, cwd.join("assets/gradius.pak"));
        assert_eq!(data.savestate, cwd.join("savestate.grs"));
        assert_eq!(data.keybinds, cwd.join("assets/keybinds.ini"));
    }

    #[test]
    fn pack_next_to_the_executable_is_portable() {
        let base = temp_dir("portable-pack");
        let exe = base.join("app");
        touch(&exe.join("gradius.pak"));
        let vars = [("HOME", "/home/x")];
        let data = resolve(&env(&vars), &base.join("cwd"), &exe, false);

        assert_eq!(data.mode, Mode::Portable);
        assert_eq!(data.root, exe);
        assert_eq!(data.pack, exe.join("gradius.pak"));
    }

    #[test]
    fn portable_marker_file_is_portable() {
        let base = temp_dir("portable-marker");
        let exe = base.join("app");
        touch(&exe.join("portable"));
        let vars = [("HOME", "/home/x")];
        let data = resolve(&env(&vars), &base.join("cwd"), &exe, false);

        assert_eq!(data.mode, Mode::Portable);
        assert_eq!(data.root, exe);
    }

    #[test]
    fn windows_uses_the_executable_folder() {
        let base = temp_dir("windows");
        let exe = base.join("app");
        let vars = [("XDG_DATA_HOME", "/xdg"), ("HOME", "/home/x")];
        let data = resolve(&env(&vars), &base.join("cwd"), &exe, true);

        assert_eq!(data.mode, Mode::Portable);
        assert_eq!(data.root, exe);
        assert_eq!(data.pack, exe.join("gradius.pak"));
    }

    #[test]
    fn linux_uses_xdg_data_home() {
        let base = temp_dir("xdg");
        let vars = [("XDG_DATA_HOME", "/data/share"), ("HOME", "/home/x")];
        let data = resolve(&env(&vars), &base.join("cwd"), &base.join("exe"), false);

        assert_eq!(data.mode, Mode::User);
        assert_eq!(data.root, PathBuf::from("/data/share/gradius"));
        assert_eq!(data.pack, PathBuf::from("/data/share/gradius/gradius.pak"));
    }

    #[test]
    fn relative_xdg_data_home_is_ignored() {
        let base = temp_dir("xdg-relative");
        let vars = [("XDG_DATA_HOME", "relative"), ("HOME", "/home/x")];
        let data = resolve(&env(&vars), &base.join("cwd"), &base.join("exe"), false);

        assert_eq!(data.mode, Mode::User);
        assert_eq!(data.root, PathBuf::from("/home/x/.local/share/gradius"));
    }

    #[test]
    fn linux_without_xdg_uses_home() {
        let base = temp_dir("home");
        let vars = [("HOME", "/home/x")];
        let data = resolve(&env(&vars), &base.join("cwd"), &base.join("exe"), false);

        assert_eq!(data.mode, Mode::User);
        assert_eq!(data.root, PathBuf::from("/home/x/.local/share/gradius"));
        assert_eq!(
            data.savestate,
            PathBuf::from("/home/x/.local/share/gradius/savestate.grs")
        );
        assert_eq!(
            data.keybinds,
            PathBuf::from("/home/x/.local/share/gradius/keybinds.ini")
        );
    }

    #[test]
    fn user_folder_is_created_but_others_are_left_alone() {
        let base = temp_dir("create");
        let user = resolve(
            &env(&[("XDG_DATA_HOME", base.join("xdg").to_str().expect("utf-8"))]),
            &base.join("cwd"),
            &base.join("exe"),
            false,
        );
        user.create_root().expect("create user folder");
        assert!(user.root.is_dir());

        let portable = DataDir::in_root(base.join("app"), Mode::Portable);
        portable.create_root().expect("portable needs no folder");
        assert!(!base.join("app").exists());
    }

    #[test]
    fn with_pack_changes_only_the_pack() {
        let base = temp_dir("with-pack");
        let vars = [("GRADIUS_DATA", "/data")];
        let data = resolve(&env(&vars), &base.join("cwd"), &base.join("exe"), false);
        let overridden = data.clone().with_pack(PathBuf::from("other.pak"));

        assert_eq!(overridden.pack, PathBuf::from("other.pak"));
        assert_eq!(overridden.root, data.root);
        assert_eq!(overridden.savestate, data.savestate);
        assert_eq!(overridden.keybinds, data.keybinds);
        assert_eq!(overridden.mode, Mode::Env);
    }
}
