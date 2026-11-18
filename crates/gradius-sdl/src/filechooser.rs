//! The "choose your ROM" dialog, off the window's thread.
//!
//! A native dialog blocks until it closes, so it runs on its own thread and the
//! first-run screen keeps painting while it waits. It is a child of the game
//! window (`raw-window-handle`) so the desktop puts it on top. The preferred
//! way is the XDG desktop portal (through `rfd`, no GTK linked); when that
//! fails quickly, or `GRADIUS_FILE_DIALOG=zenity` forces it, the `zenity` or
//! `kdialog` program is run instead. With neither, the caller tells the player
//! to drag the file.

use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WindowHandle,
};

const TITLE: &str = "Choose your Gradius (E) ROM";
/// A portal that answers "nothing" sooner than a person can pick failed.
const PORTAL_FAILED_WITHIN: Duration = Duration::from_millis(1500);

#[derive(Debug, PartialEq, Eq)]
pub enum Picked {
    File(PathBuf),
    Cancelled,
    /// No dialog could be shown at all.
    Unavailable,
}

/// The game window's handles, movable to the dialog thread. The window outlives
/// the thread's use of them in practice (a closed window at worst makes the
/// portal ignore the parent).
struct Parent(RawWindowHandle, RawDisplayHandle);

unsafe impl Send for Parent {}

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(self.0) })
    }
}

impl HasDisplayHandle for Parent {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(self.1) })
    }
}

/// Open the dialog for `window`; the answer arrives on the returned channel.
pub fn pick<W: HasWindowHandle + HasDisplayHandle>(window: &W) -> Receiver<Picked> {
    let parent = match (window.window_handle(), window.display_handle()) {
        (Ok(window), Ok(display)) => Some(Parent(window.as_raw(), display.as_raw())),
        _ => None,
    };
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(run(parent));
    });
    receiver
}

fn run(parent: Option<Parent>) -> Picked {
    // GRADIUS_FILE_DIALOG=zenity skips the portal (diagnostics and desktops without one).
    if std::env::var("GRADIUS_FILE_DIALOG").as_deref() != Ok("zenity") {
        let started = Instant::now();
        let mut dialog = rfd::FileDialog::new()
            .set_title(TITLE)
            .add_filter("NES ROM", &["nes"]);
        if let Some(parent) = &parent {
            dialog = dialog.set_parent(parent);
        }
        match dialog.pick_file() {
            Some(file) => return Picked::File(file),
            None if started.elapsed() >= PORTAL_FAILED_WITHIN => return Picked::Cancelled,
            None => {}
        }
    }
    let attach = parent.as_ref().and_then(|parent| match parent.0 {
        RawWindowHandle::Xlib(handle) => Some(handle.window as _),
        RawWindowHandle::Xcb(handle) => Some(u64::from(handle.window.get())),
        _ => None,
    });
    program(attach)
}

/// `zenity`, else `kdialog`. Exit status 1 is the player cancelling.
fn program(attach: Option<u64>) -> Picked {
    let mut zenity = Command::new("zenity");
    zenity
        .arg("--file-selection")
        .arg(format!("--title={TITLE}"))
        .arg("--file-filter=NES ROM | *.nes");
    if let Some(window) = attach {
        zenity.arg(format!("--attach={window}"));
    }
    let mut kdialog = Command::new("kdialog");
    kdialog.args(["--title", TITLE, "--getopenfilename", ".", "*.nes"]);
    for mut command in [zenity, kdialog] {
        if let Ok(output) = command.output() {
            let chosen = String::from_utf8_lossy(&output.stdout);
            let chosen = chosen.trim_end_matches('\n');
            return if output.status.success() && !chosen.is_empty() {
                Picked::File(PathBuf::from(chosen))
            } else {
                Picked::Cancelled
            };
        }
    }
    Picked::Unavailable
}
