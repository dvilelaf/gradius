//! First-run screen: no pack yet, so ask for the player's own ROM, check it,
//! extract the pack with progress and save it in the data folder.
//!
//! The screen draws only text and a bar, in the interface font: nothing from
//! the game exists before the extraction. [`Setup`] is the pure state
//! machine; [`install`] is the work, run on a thread by the window loop and
//! inline by the headless path.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use gradius_extract::Step;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::pixels::Color as SdlColor;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::EventPump;

use crate::filechooser::{self, Picked};
use crate::options::Options;
use crate::text::{Color, Layer, Presenter};

const STEP_COUNT: u32 = 9;
const BACKGROUND: Color = [14, 16, 36, 255];
const WHITE: Color = [255, 255, 255, 255];
const GREY: Color = [175, 180, 205, 255];
const RED: Color = [255, 120, 110, 255];
/// The text column, centred, in layer units.
const COLUMN: f32 = 220.0;
const TEXT_SIZE: f32 = 10.5;
const LINE_HEIGHT: f32 = 14.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Waiting,
    Verifying,
    Extracting { step: Step, done: u32, total: u32 },
    Done,
}

/// What the screen is doing and the last thing that went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setup {
    pub phase: Phase,
    pub error: Option<String>,
    /// The file dialog is open on its own thread.
    pub choosing: bool,
}

impl Setup {
    pub fn new() -> Self {
        Self {
            phase: Phase::Waiting,
            error: None,
            choosing: false,
        }
    }

    /// A ROM was given. Only accepted while waiting for one.
    pub fn offer(&mut self) -> bool {
        if self.phase != Phase::Waiting {
            return false;
        }
        self.phase = Phase::Verifying;
        self.error = None;
        true
    }

    /// The ROM is Gradius (E); the extraction starts.
    pub fn verified(&mut self) {
        if self.phase == Phase::Verifying {
            self.phase = Phase::Extracting {
                step: Step::Tiles,
                done: 0,
                total: 1,
            };
        }
    }

    pub fn progress(&mut self, step: Step, done: u32, total: u32) {
        if matches!(self.phase, Phase::Extracting { .. }) {
            self.phase = Phase::Extracting { step, done, total };
        }
    }

    /// Anything went wrong: show why and ask again.
    pub fn failed(&mut self, message: String) {
        self.phase = Phase::Waiting;
        self.error = Some(message);
    }

    pub fn done(&mut self) {
        if matches!(self.phase, Phase::Extracting { .. }) {
            self.phase = Phase::Done;
        }
    }

    /// 0.0..=1.0 for the bar.
    pub fn fraction(&self) -> f32 {
        match self.phase {
            Phase::Extracting { step, done, total } => {
                let within = if total == 0 {
                    0.0
                } else {
                    done.min(total) as f32 / total as f32
                };
                (step as u32 as f32 + within) / STEP_COUNT as f32
            }
            Phase::Done => 1.0,
            _ => 0.0,
        }
    }
}

fn step_label(step: Step) -> &'static str {
    match step {
        Step::Tiles => "reading the graphics",
        Step::Audio => "capturing the music and sounds",
        Step::Backgrounds => "building the backgrounds",
        Step::Collision => "mapping the terrain",
        Step::Spawns => "placing the enemies",
        Step::Sprites => "assembling the sprites",
        Step::Tables => "reading the game tables",
        Step::Stage => "building the stage",
        Step::Pack => "saving the game data",
    }
}

/// Verify `rom`, extract it and write `pack` (through `gradius.pak.tmp`).
/// `verified` runs once the ROM is accepted; errors are plain sentences.
pub fn install(
    rom: &Path,
    pack: &Path,
    verified: &mut dyn FnMut(),
    progress: &mut dyn FnMut(Step, u32, u32),
) -> Result<(), String> {
    let bytes =
        std::fs::read(rom).map_err(|error| format!("cannot read {}: {error}", rom.display()))?;
    gradius_extract::identify(&bytes).map_err(|error| error.to_string())?;
    verified();
    let pak = gradius_extract::extract(&bytes, progress).map_err(|error| error.to_string())?;
    let tmp = pack.with_extension("pak.tmp");
    let write = || -> std::io::Result<()> {
        if let Some(dir) = pack.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&tmp, &pak)?;
        std::fs::rename(&tmp, pack)
    };
    write().map_err(|error| {
        let _ = std::fs::remove_file(&tmp);
        format!(
            "cannot save the game data to {}: {error}. Is the folder writable? Move the game to a folder you can write to",
            pack.display()
        )
    })
}

enum Message {
    Verified,
    Progress(Step, u32, u32),
    Finished(Result<(), String>),
}

fn spawn(rom: PathBuf, pack: PathBuf) -> Receiver<Message> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let progress_sender = sender.clone();
        let verified_sender = sender.clone();
        let result = install(
            &rom,
            &pack,
            &mut || {
                let _ = verified_sender.send(Message::Verified);
            },
            &mut |step, done, total| {
                let _ = progress_sender.send(Message::Progress(step, done, total));
            },
        );
        let _ = sender.send(Message::Finished(result));
    });
    receiver
}

/// Ask for the ROM in the game's own window until the pack exists. `Ok(false)`
/// means the player closed the window.
pub fn run(
    options: &Options,
    pack: &Path,
    canvas: &mut Canvas<Window>,
    presenter: &mut Presenter,
    events: &mut EventPump,
) -> Result<bool, String> {
    canvas
        .window_mut()
        .set_title("Gradius (E) native port (unofficial) - first run")
        .ok();

    let mut setup = Setup::new();
    let mut worker: Option<Receiver<Message>> = None;
    let mut chooser: Option<Receiver<Picked>> = None;
    let mut offered = options.rom.clone();

    while setup.phase != Phase::Done {
        if let Some(rom) = offered.take() {
            if setup.offer() {
                worker = Some(spawn(rom, pack.to_path_buf()));
            }
        }
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => return Ok(false),
                // Esc only stops waiting for the dialog; closing the window
                // is the way out.
                Event::KeyDown {
                    scancode: Some(Scancode::Escape),
                    ..
                } => {
                    chooser = None;
                    setup.choosing = false;
                }
                Event::DropFile { filename, .. } => offered = Some(PathBuf::from(filename)),
                Event::KeyDown {
                    scancode: Some(Scancode::Return | Scancode::KpEnter),
                    ..
                }
                | Event::MouseButtonDown { .. }
                    if setup.phase == Phase::Waiting && chooser.is_none() =>
                {
                    chooser = Some(filechooser::pick(canvas.window()));
                    setup.choosing = true;
                    setup.error = None;
                }
                _ => {}
            }
        }
        if let Some(receiver) = &chooser {
            let answer = match receiver.try_recv() {
                Ok(picked) => Some(picked),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Picked::Cancelled),
            };
            if let Some(picked) = answer {
                match picked {
                    Picked::File(file) => offered = Some(file),
                    Picked::Cancelled => {}
                    Picked::Unavailable => {
                        setup.error = Some(
                            "no file chooser is available here: drag the .nes file onto this window"
                                .to_string(),
                        );
                    }
                }
                chooser = None;
                setup.choosing = false;
            }
        }
        if let Some(receiver) = &worker {
            loop {
                match receiver.try_recv() {
                    Ok(Message::Verified) => setup.verified(),
                    Ok(Message::Progress(step, done, total)) => setup.progress(step, done, total),
                    Ok(Message::Finished(Ok(()))) => setup.done(),
                    Ok(Message::Finished(Err(message))) => setup.failed(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if matches!(setup.phase, Phase::Verifying | Phase::Extracting { .. }) {
                            setup.failed("the extraction stopped unexpectedly".to_string());
                        }
                        break;
                    }
                }
            }
        }

        let (width, height) = canvas
            .output_size()
            .map_err(|error| format!("cannot read drawable size: {error}"))?;
        draw(presenter.begin(width, height), &setup);
        canvas.set_draw_color(SdlColor::BLACK);
        canvas.clear();
        presenter.show(canvas)?;
        canvas.present();
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(true)
}

/// Write a wrapped paragraph in the column starting at `left`.
fn paragraph(layer: &mut Layer, left: f32, y: &mut f32, text: &str, color: Color) {
    for line in layer.wrap(text, TEXT_SIZE, false, COLUMN) {
        layer.text(left, *y, &line, TEXT_SIZE, false, color);
        *y += LINE_HEIGHT;
    }
}

/// Paint the whole screen into a layer the size of the window.
pub fn draw(layer: &mut Layer, setup: &Setup) {
    layer.band(0.0, 240.0, BACKGROUND);
    let left = (layer.units_wide() - COLUMN) / 2.0;
    let mut y = 36.0;
    layer.text(left, y, "Welcome to Gradius", 17.0, true, WHITE);
    y += 24.0;
    paragraph(layer, left, &mut y, "(E) native port (unofficial)", GREY);
    y += LINE_HEIGHT;
    match &setup.phase {
        Phase::Waiting if setup.choosing => {
            paragraph(
                layer,
                left,
                &mut y,
                "Waiting for the file chooser...",
                WHITE,
            );
            paragraph(layer, left, &mut y, "(Esc cancels)", GREY);
        }
        Phase::Waiting => {
            paragraph(
                layer,
                left,
                &mut y,
                "To play, this game needs your own copy of Gradius (E) for the NES: a file ending in .nes. It stays on your computer and is read only once.",
                GREY,
            );
            y += LINE_HEIGHT;
            paragraph(
                layer,
                left,
                &mut y,
                "Drag the file onto this window.",
                WHITE,
            );
            paragraph(
                layer,
                left,
                &mut y,
                "Or press Enter or click to browse for it.",
                WHITE,
            );
            if let Some(error) = &setup.error {
                y += LINE_HEIGHT;
                paragraph(layer, left, &mut y, error, RED);
            }
        }
        Phase::Verifying => paragraph(layer, left, &mut y, "Checking the file...", WHITE),
        Phase::Extracting { step, .. } => {
            paragraph(
                layer,
                left,
                &mut y,
                "Preparing your game. This takes a few seconds.",
                WHITE,
            );
            y += LINE_HEIGHT;
            let label = format!(
                "Step {} of {STEP_COUNT}: {}",
                *step as u32 + 1,
                step_label(*step)
            );
            paragraph(layer, left, &mut y, &label, GREY);
            y += 4.0;
            layer.rect(left, y, COLUMN, 8.0, GREY);
            layer.rect(left + 1.0, y + 1.0, COLUMN - 2.0, 6.0, BACKGROUND);
            layer.rect(
                left + 2.0,
                y + 2.0,
                (COLUMN - 4.0) * setup.fraction(),
                4.0,
                WHITE,
            );
        }
        Phase::Done => paragraph(layer, left, &mut y, "Ready!", WHITE),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn extracting() -> Setup {
        let mut setup = Setup::new();
        assert!(setup.offer());
        setup.verified();
        setup
    }

    #[test]
    fn a_rom_walks_the_screen_to_done() {
        let mut setup = Setup::new();
        assert_eq!(setup.phase, Phase::Waiting);
        assert!(setup.offer());
        assert_eq!(setup.phase, Phase::Verifying);
        setup.verified();
        assert!(matches!(
            setup.phase,
            Phase::Extracting {
                step: Step::Tiles,
                ..
            }
        ));
        setup.progress(Step::Stage, 1, 2);
        assert!(matches!(
            setup.phase,
            Phase::Extracting {
                step: Step::Stage,
                done: 1,
                total: 2
            }
        ));
        setup.done();
        assert_eq!(setup.phase, Phase::Done);
        assert_eq!(setup.fraction(), 1.0);
    }

    #[test]
    fn a_failure_shows_why_and_asks_again() {
        let mut setup = Setup::new();
        setup.offer();
        setup.failed("this is not Gradius (E)".to_string());
        assert_eq!(setup.phase, Phase::Waiting);
        assert_eq!(setup.error.as_deref(), Some("this is not Gradius (E)"));
        assert!(setup.offer());
        assert_eq!(setup.error, None);

        let mut setup = extracting();
        setup.failed("cannot save".to_string());
        assert_eq!(setup.phase, Phase::Waiting);
    }

    #[test]
    fn a_second_rom_is_ignored_while_working() {
        let mut setup = Setup::new();
        setup.offer();
        assert!(!setup.offer());
        setup.verified();
        assert!(!setup.offer());
        setup.done();
        assert!(!setup.offer());
    }

    #[test]
    fn late_messages_do_not_move_a_waiting_screen() {
        let mut setup = Setup::new();
        setup.verified();
        setup.progress(Step::Audio, 1, 1);
        setup.done();
        assert_eq!(setup, Setup::new());
    }

    #[test]
    fn the_bar_only_grows() {
        let mut setup = extracting();
        let mut last = setup.fraction();
        for (index, step) in [Step::Tiles, Step::Audio, Step::Stage, Step::Pack]
            .into_iter()
            .enumerate()
        {
            setup.progress(step, 0, 4);
            assert!(setup.fraction() >= last, "{index}");
            setup.progress(step, 4, 4);
            assert!(setup.fraction() >= last);
            last = setup.fraction();
        }
        assert!(last <= 1.0);
    }

    #[test]
    fn a_missing_rom_file_is_a_plain_error() {
        let dir = std::env::temp_dir().join(format!("gradius-setup-{}", std::process::id()));
        let error = install(
            &dir.join("nope.nes"),
            &dir.join("gradius.pak"),
            &mut || {},
            &mut |_, _, _| {},
        )
        .unwrap_err();
        assert!(error.contains("cannot read"), "{error}");
    }

    #[test]
    fn a_file_that_is_not_gradius_is_rejected_with_the_expected_sha() {
        let dir = std::env::temp_dir().join(format!("gradius-setup-fake-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut fake = b"NES\x1a\x02\x01\x30\x00".to_vec();
        fake.resize(16, 0);
        fake.resize(16 + 0x8000 + 0x2000, 0);
        let rom = dir.join("fake.nes");
        std::fs::write(&rom, &fake).unwrap();
        let pack = dir.join("gradius.pak");
        let mut verified = false;
        let error = install(&rom, &pack, &mut || verified = true, &mut |_, _, _| {}).unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!verified);
        assert!(error.contains("expected SHA-256"), "{error}");
        assert!(!pack.exists());
    }

    /// The whole windowed path on SDL's dummy driver: the ROM comes in, the bar
    /// is drawn on the shared canvas, the pack is written and the loop hands
    /// the same window back. Needs the player's ROM, so it skips without one.
    /// Ignored by default because a debug build takes ~6 minutes; the headless
    /// extraction test and the packaging smoke tests cover the extraction itself.
    /// Run it after changing the setup screen or the windowed loop.
    #[test]
    #[ignore = "full extraction in a debug build, ~6 min: cargo test -p gradius-sdl -- --ignored"]
    fn the_windowed_screen_extracts_and_returns_the_same_window() {
        let Some(rom) = std::env::var_os("GRADIUS_ROM") else {
            return;
        };
        let _ = sdl2::hint::set("SDL_VIDEODRIVER", "dummy");
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let options = Options::parse([OsString::from("--rom"), rom]).unwrap();
        let (mut canvas, _) = crate::build_canvas(&video, 1).unwrap();
        let creator = canvas.texture_creator();
        let mut presenter = Presenter::new(&creator);
        let mut events = sdl.event_pump().unwrap();
        let window_id = canvas.window().id();
        let dir = std::env::temp_dir().join(format!("gradius-setup-window-{}", std::process::id()));
        let pack = dir.join("gradius.pak");
        let finished = run(&options, &pack, &mut canvas, &mut presenter, &mut events).unwrap();
        let written = pack.exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(finished);
        assert!(written);
        assert_eq!(canvas.window().id(), window_id);
    }

    fn painted(setup: &Setup) -> Layer {
        let mut layer = Layer::new(768, 720);
        draw(&mut layer, setup);
        layer
    }

    #[test]
    fn the_screen_says_when_it_waits_for_the_file_chooser() {
        let idle = painted(&Setup::new());
        let waiting = painted(&Setup {
            choosing: true,
            ..Setup::new()
        });
        assert_ne!(idle.rgba, waiting.rgba);
    }

    #[test]
    fn the_screen_is_opaque_and_paints_light_text() {
        let layer = painted(&Setup::new());
        assert!(layer.rgba.chunks(4).all(|pixel| pixel[3] == 255));
        assert!(layer.rgba.chunks(4).any(|pixel| pixel[..3] == [255; 3]));
    }

    #[test]
    fn a_wrapped_error_stays_inside_the_window() {
        let setup = Setup {
            error: Some("x".repeat(300)),
            ..Setup::new()
        };
        painted(&setup);
    }
}
