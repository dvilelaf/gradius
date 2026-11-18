// A release build on Windows is a GUI program: no console window when it is
// double-clicked. `attach_parent_console` gives --help and errors back to the
// terminal it was started from.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! SDL2 desktop frontend.
//!
//! The only crate that knows about a window, a gamepad or an audio device.
//! It loads the self-contained pack, feeds SDL's clock and input into the
//! platform-agnostic runner, uploads the RGBA framebuffer and drains semantic
//! sound events into the captured NES APU engine. A different platform needs
//! another backend with the same traits; the runner and the core do not change.

mod audio;
mod bmp;
mod controller;
mod datadir;
mod filechooser;
mod keybinds;
mod menu;
mod menu_input;
mod music;
mod options;
mod overlay;
mod savestate;
mod settings;
mod setup;
mod text;
mod viewport;
mod volume;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use audio::AudioBackend;
use controller::Controllers;
use gradius_app::{
    Buttons, FrameSink, Game, InputFrame, MusicCue, Runner, SoundSink, HEIGHT, STEP, WIDTH,
};
use gradius_audio::{AudioBank, Engine, SAMPLE_RATE, TICKS_PER_SECOND};
use gradius_core::audio::Sound;
use gradius_core::State;
use menu::Menu;
use options::Options;
use sdl2::controller::Button;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture};
use sdl2::video::{FullscreenType, Window};
use settings::Settings;
use viewport::Fit;
use volume::{Volume, STEP as VOLUME_STEP};

const TITLE: &str = "Gradius (E) native port (unofficial)";
const SCREENSHOT_DIR: &str = "screenshots";
/// How long a volume/mute change stays in the window title.
const VOLUME_NOTICE: Duration = Duration::from_millis(1_500);
/// How long a savestate result stays on screen.
const SAVESTATE_NOTICE: Duration = Duration::from_millis(1_500);
/// How long a power-press flash stays in the debug overlay.
const POWER_FLASH: Duration = Duration::from_millis(1_200);

/// Drawable sizes a `--print-viewport` headless run resolves: 16:9, 21:9,
/// 4:3 and a portrait monitor.
const VIEWPORT_SAMPLES: [(u32, u32); 4] = [(1920, 1080), (2560, 1080), (1024, 768), (1080, 1920)];

/// Reconnect stdout and stderr to the console that launched this GUI-subsystem
/// process, unless they are already redirected. Declared by hand: no crate.
#[cfg(all(windows, not(debug_assertions)))]
fn attach_parent_console() {
    use std::os::windows::io::IntoRawHandle;
    extern "system" {
        fn AttachConsole(process: u32) -> i32;
        fn GetStdHandle(which: u32) -> isize;
        fn SetStdHandle(which: u32, handle: isize) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    // SAFETY: plain kernel32 calls with valid arguments.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let current = GetStdHandle(which);
            if current != 0 && current != -1 {
                continue;
            }
            if let Ok(console) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
                SetStdHandle(which, console.into_raw_handle() as isize);
            }
        }
    }
}

fn main() {
    #[cfg(all(windows, not(debug_assertions)))]
    attach_parent_console();
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

const NO_ROM_DATA: &str = "the pack has no ROM data; run scripts/extract-data";

fn run() -> Result<(), String> {
    let options = Options::parse(std::env::args_os().skip(1))?;
    let data = datadir::DataDir::from_process()
        .map_err(|error| format!("cannot resolve the data folder: {error}"))?;
    let data = match options.pack.clone() {
        Some(pack) => data.with_pack(pack),
        None => data,
    };
    let missing_pack = options.pack.is_none() && !data.pack.exists();
    if options.dump_audio.is_some() && options.frames.is_none() {
        return Err("--dump-audio is a headless option; pass --frames N too".to_string());
    }
    // Windowed: the setup screen and the game share one window, so the pack
    // is loaded inside `run_windowed` once the window exists.
    let Some(frames) = options.frames else {
        return run_windowed(&options, &data, missing_pack);
    };
    // Headless: no window to ask in, so the ROM has to come with --rom.
    if let (true, Some(rom)) = (missing_pack, &options.rom) {
        setup::install(rom, &data.pack, &mut || {}, &mut |_, _, _| {})?;
    }
    let (game, bank) = load_game(&data.pack, &options)?;
    run_headless(game, frames, &options, bank)
}

fn load_game(pack: &Path, options: &Options) -> Result<(Game, AudioBank), String> {
    let pack = load_pack(pack)?;
    let bank = audio::load_audio_bank(pack.audio.as_deref());
    let assets = pack.into_assets();
    if assets.rom().is_err() {
        return Err(NO_ROM_DATA.to_string());
    }
    let mut game = match options.stage {
        Some(stage) => Game::new_at_stage(assets, stage - 1),
        None => Game::new(assets),
    };
    game.set_mods(options.mods());
    Ok((game, bank))
}

/// The CI/smoke path: exactly `frames` simulation ticks, then one line on
/// stdout. The hash is FNV-1a over the RGBA framebuffer, so two builds that
/// render the same pixels print the same value.
fn run_headless(game: Game, frames: u64, options: &Options, bank: AudioBank) -> Result<(), String> {
    let mut input = |frame: u64| {
        if options.autoplay {
            InputFrame::from(autoplay(frame))
        } else {
            InputFrame::NONE
        }
    };
    let mut soundtrack = options
        .dump_audio
        .as_ref()
        .map(|_| SoundtrackSink::new(bank));
    let mut muted = NullSink;
    let mut runner = Runner::new(game);
    let summary = match &mut soundtrack {
        Some(sink) => runner.run(frames, &mut input, sink),
        None => runner.run(frames, &mut input, &mut muted),
    };

    if let (Some(path), Some(sink)) = (&options.dump_audio, &soundtrack) {
        gradius_audio::wav::write_samples(path, SAMPLE_RATE, &sink.samples)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        let seconds = sink.samples.len() as f64 / f64::from(SAMPLE_RATE);
        println!(
            "audio: {} ({:.2}s, {} samples, 48 kHz mono)",
            path.display(),
            seconds,
            sink.samples.len()
        );
    }
    if let Some(path) = &options.screenshot {
        bmp::write_rgba(path, WIDTH, HEIGHT, runner.game().framebuffer().as_rgba())?;
        println!("screenshot: {}", path.display());
    }
    if options.print_viewport {
        for (width, height) in VIEWPORT_SAMPLES {
            print_viewport(options.fit, width, height);
        }
    }
    println!(
        "headless: frames={} hash={:#018x} score={} lives={} audio_events={}",
        summary.frames, summary.hash, summary.score, summary.lives, summary.audio_events,
    );
    Ok(())
}

/// Deterministic demo input: start the game, then fire and weave. Start is
/// pressed again every ten seconds so a long demo restarts after game over.
fn autoplay(frame: u64) -> Buttons {
    let phase = (frame / 40) % 4;
    let start = frame >= 120 && (frame - 120) % 600 < 6;
    Buttons {
        fire: frame >= 180,
        up: frame >= 180 && phase == 0,
        down: frame >= 180 && phase == 2,
        start,
        ..Buttons::NONE
    }
}

fn run_windowed(
    options: &Options,
    data: &datadir::DataDir,
    needs_setup: bool,
) -> Result<(), String> {
    // HIDAPI claims pads it often cannot read (root-only hidraw) and hides them
    // from evdev, leaving them open but silent; use evdev unless asked otherwise.
    #[cfg(target_os = "linux")]
    if std::env::var_os("SDL_JOYSTICK_HIDAPI").is_none() {
        let _ = sdl2::hint::set("SDL_JOYSTICK_HIDAPI", "0");
    }
    let sdl = sdl2::init().map_err(|error| format!("SDL2 init failed: {error}"))?;
    // Keep the 256x240 image crisp at every integer scale.
    let _ = sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "nearest");
    let video = sdl
        .video()
        .map_err(|error| format!("cannot open SDL video: {error}"))?;
    let (mut canvas, vsync) = build_canvas(&video, options.scale)?;
    canvas.set_draw_color(Color::BLACK);
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, WIDTH as u32, HEIGHT as u32)
        .map_err(|error| format!("cannot create texture: {error}"))?;
    let mut presenter = text::Presenter::new(&texture_creator);
    let mut event_pump = sdl
        .event_pump()
        .map_err(|error| format!("cannot create event pump: {error}"))?;
    if needs_setup
        && !setup::run(
            options,
            &data.pack,
            &mut canvas,
            &mut presenter,
            &mut event_pump,
        )?
    {
        return Ok(());
    }
    let (game, bank) = load_game(&data.pack, options)?;
    let keybinds = keybinds::load(&data.keybinds);
    let settings = Settings::startup(Settings::load(&data.settings), options);
    let mut controllers = Controllers::new(sdl.game_controller().ok(), sdl.joystick().ok());
    let mut audio = if options.audio {
        AudioBackend::open(
            &sdl,
            music::music_for(MusicCue::Title, &bank),
            Volume::new(settings.volume).gain(),
        )
    } else {
        AudioBackend::disabled()
    };

    let frame_time = Duration::from_secs_f64(STEP);
    let mut previous = Instant::now();
    let mut fullscreen = false;
    if settings.fullscreen {
        match canvas.window_mut().set_fullscreen(FullscreenType::Desktop) {
            Ok(()) => fullscreen = true,
            Err(error) => eprintln!("warning: fullscreen failed: {error}"),
        }
    }
    let mut shown_title: Option<String> = None;
    let mut notice: Option<(String, Instant)> = None;
    let mut save_notice: Option<(String, Instant)> = None;
    let mut volume = Volume::new(settings.volume);
    let mut menu = Menu::new(settings);
    let mut menu_open = false;
    let mut paused_before_menu = false;
    let mut debug_overlay = options.debug_overlay;
    let mut previous_power = false;
    let mut power_flash: Option<(String, Instant)> = None;
    let mut runner = Runner::new(game);
    runner.set_mods(settings.mods());

    if options.print_viewport {
        let (width, height) = canvas
            .output_size()
            .map_err(|error| format!("cannot read drawable size: {error}"))?;
        print_viewport(options.fit, width, height);
    }

    let mut stick = menu_input::MenuStick::default();
    'running: loop {
        let frame_start = Instant::now();
        let mut menu_input = None;
        let mut open_menu = false;
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'running,
                Event::KeyDown {
                    scancode: Some(scancode),
                    ..
                } if menu_open => {
                    if let Some(input) = menu_input::menu_key(scancode) {
                        menu_input = Some(input);
                    }
                }
                Event::ControllerButtonDown { button, .. } if menu_open => {
                    if let Some(input) = menu_input::menu_button(button) {
                        menu_input = Some(input);
                    }
                }
                Event::ControllerAxisMotion { axis, value, .. } if menu_open => {
                    if let Some(input) = stick.menu_input(axis, value) {
                        menu_input = Some(input);
                    }
                }
                Event::ControllerButtonDown {
                    button: Button::Guide,
                    ..
                } => open_menu = true,
                // Back is the NES Select: on the title it still picks 1P/2P,
                // anywhere else it opens the options menu (many pads have no
                // usable Guide button).
                Event::ControllerButtonDown {
                    button: Button::Back,
                    ..
                } if runner.game().state != State::Title => open_menu = true,
                Event::KeyDown {
                    scancode: Some(scancode),
                    ..
                } => match scancode {
                    Scancode::Escape => open_menu = true,
                    Scancode::F11 => {
                        fullscreen = !fullscreen;
                        let mode = if fullscreen {
                            FullscreenType::Desktop
                        } else {
                            FullscreenType::Off
                        };
                        if let Err(error) = canvas.window_mut().set_fullscreen(mode) {
                            eprintln!("warning: fullscreen toggle failed: {error}");
                            fullscreen = !fullscreen;
                        }
                        menu.settings.fullscreen = fullscreen;
                    }
                    Scancode::Pause => {
                        runner.toggle_pause();
                        previous = Instant::now();
                    }
                    Scancode::Minus => {
                        volume.adjust(-VOLUME_STEP);
                        audio.set_gain(volume.gain());
                        notice = Some((volume.notice(), Instant::now()));
                    }
                    Scancode::Equals => {
                        volume.adjust(VOLUME_STEP);
                        audio.set_gain(volume.gain());
                        notice = Some((volume.notice(), Instant::now()));
                    }
                    Scancode::M => {
                        volume.toggle_mute();
                        audio.set_gain(volume.gain());
                        notice = Some((volume.notice(), Instant::now()));
                    }
                    Scancode::F12 => {
                        match save_screenshot(runner.game().framebuffer(), runner.game().frame) {
                            Ok(path) => println!("screenshot: {}", path.display()),
                            Err(error) => eprintln!("warning: screenshot failed: {error}"),
                        }
                    }
                    Scancode::F3 => debug_overlay = !debug_overlay,
                    other => {
                        if let Some(action) = savestate::hotkey(other) {
                            let slot = &data.savestate;
                            let result = match action {
                                savestate::Action::Save => savestate::save(runner.game(), slot),
                                savestate::Action::Load => savestate::load(runner.game_mut(), slot),
                            };
                            if let Err(error) = &result {
                                eprintln!("warning: {error}");
                            }
                            save_notice = Some((
                                savestate::notice(action, &result).to_string(),
                                Instant::now(),
                            ));
                        }
                    }
                },
                Event::AudioDeviceRemoved {
                    iscapture: false, ..
                } => audio.reopen(&sdl),
                Event::JoyDeviceAdded { which, .. } => {
                    if let Some(controllers) = &mut controllers {
                        controllers.joy_added(which);
                    }
                }
                Event::ControllerDeviceAdded { which, .. } => {
                    if let Some(controllers) = &mut controllers {
                        controllers.added(which);
                    }
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    if let Some(controllers) = &mut controllers {
                        controllers.removed(which);
                    }
                }
                _ => {}
            }
        }

        if open_menu {
            menu.settings.volume = volume.percent();
            menu.settings.fullscreen = fullscreen;
            menu.open();
            menu_open = true;
            paused_before_menu = runner.is_paused();
            runner.set_paused(true);
        }
        if let Some(input) = menu_input {
            let before = menu.settings;
            let outcome = menu.handle(input);
            let after = menu.settings;
            if after != before {
                after.save(&data.settings);
                if after.volume != before.volume {
                    volume = Volume::new(after.volume);
                    audio.set_gain(volume.gain());
                }
                if after.fullscreen != fullscreen {
                    let mode = if after.fullscreen {
                        FullscreenType::Desktop
                    } else {
                        FullscreenType::Off
                    };
                    match canvas.window_mut().set_fullscreen(mode) {
                        Ok(()) => fullscreen = after.fullscreen,
                        Err(error) => eprintln!("warning: fullscreen toggle failed: {error}"),
                    }
                }
            }
            match outcome {
                menu::Outcome::Stay => {}
                menu::Outcome::Close => {
                    menu_open = false;
                    runner.set_paused(paused_before_menu);
                    previous = Instant::now();
                }
                menu::Outcome::Quit => break 'running,
            }
        }
        // The next game picks up the menu's choices; one in progress keeps
        // the ones it started with.
        if runner.game().state != State::Playing && runner.game().mods() != menu.settings.mods() {
            runner.set_mods(menu.settings.mods());
        }

        audio.retry_lost(&sdl);

        let paused = runner.is_paused() && !menu_open;
        let (window_width, window_height) = canvas
            .output_size()
            .map_err(|error| format!("cannot read drawable size: {error}"))?;
        let view = viewport::viewport(options.fit, window_width, window_height);
        let notice_text = match &notice {
            Some((text, at)) if at.elapsed() < VOLUME_NOTICE => Some(text.clone()),
            _ => {
                notice = None;
                None
            }
        };
        let title = window_title(paused, options.fit, view, notice_text.as_deref());
        if shown_title.as_deref() != Some(title.as_str()) {
            shown_title = Some(title.clone());
            canvas.window_mut().set_title(&title).ok();
        }

        let mut input = keyboard_frame(&event_pump, &keybinds);
        if let Some(controllers) = &controllers {
            input =
                controller::union_frame(input, controllers.buttons(runner.game().simultaneous()));
        }
        if menu_open {
            input = InputFrame::NONE;
        }

        let selected_before = runner.game().power().selected();
        let power_pressed = input.p1.power && !previous_power;
        previous_power = input.p1.power;

        let now = Instant::now();
        let delta = (now - previous).as_secs_f64();
        previous = now;
        let mut audio_sink = AudioSink {
            backend: &mut audio,
            bank: &bank,
        };
        runner.advance(delta, input, &mut audio_sink);

        if power_pressed && !paused {
            let applied = selected_before.is_some() && runner.game().power().selection() == 0;
            power_flash = Some((
                overlay::power_flash(selected_before, applied),
                Instant::now(),
            ));
        }

        let flash_text = match &power_flash {
            Some((text, at)) if at.elapsed() < POWER_FLASH => Some(text.clone()),
            _ => {
                power_flash = None;
                None
            }
        };
        let save_text = match &save_notice {
            Some((text, at)) if at.elapsed() < SAVESTATE_NOTICE => Some(text.clone()),
            _ => {
                save_notice = None;
                None
            }
        };
        let debug_lines = debug_overlay.then(|| overlay::debug_lines(runner.game().power()));

        runner.present(&mut TextureSink {
            texture: &mut texture,
        })?;
        canvas.clear();
        canvas
            .copy(
                &texture,
                None,
                Some(Rect::new(
                    view.x as i32,
                    view.y as i32,
                    view.width,
                    view.height,
                )),
            )
            .map_err(|error| format!("present failed: {error}"))?;
        // The frontend's own text goes on top at the window's resolution.
        let notices: Vec<&str> = [save_text.as_deref(), notice_text.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        if paused || debug_lines.is_some() || !notices.is_empty() || menu_open {
            let layer = presenter.begin(window_width, window_height);
            if paused {
                overlay::draw_paused(layer);
            }
            if let Some(lines) = &debug_lines {
                overlay::draw_debug(layer, lines, flash_text.as_deref());
            }
            if let Some(text) = notices.first() {
                overlay::draw_notice(layer, text);
            }
            if menu_open {
                menu.draw(layer, runner.game().state == State::Playing);
            }
            presenter.show(&mut canvas)?;
        }
        canvas.present();

        // Without a vsync renderer, pace the loop ourselves instead of
        // spinning a core.
        if !vsync {
            let elapsed = frame_start.elapsed();
            if elapsed < frame_time {
                std::thread::sleep(frame_time - elapsed);
            }
        }
    }
    Ok(())
}

/// The window title carries the current fit policy and resolved scale, plus a
/// short-lived volume/mute notice after a change.
fn window_title(paused: bool, fit: Fit, view: viewport::Viewport, notice: Option<&str>) -> String {
    let state = if paused { " - paused" } else { "" };
    let notice = notice.map(|text| format!(" - {text}")).unwrap_or_default();
    let scale = match fit {
        Fit::Integer => format!("{}x", view.width / WIDTH as u32),
        Fit::Aspect => format!("{:.2}x", view.scale()),
        Fit::Stretch => "stretch".to_string(),
    };
    format!("{TITLE}{state}{notice} [{fit} {scale}]", fit = fit.name())
}

/// One machine-readable line per drawable size; the headless path prints the
/// standard formats because it has no window to measure.
fn print_viewport(fit: Fit, width: u32, height: u32) {
    let view = viewport::viewport(fit, width, height);
    let scale = match fit {
        Fit::Stretch => "stretch".to_string(),
        _ => format!("{:.2}", view.scale()),
    };
    println!(
        "viewport: fit={} window={width}x{height} dest={},{} {}x{} scale={scale}",
        fit.name(),
        view.x,
        view.y,
        view.width,
        view.height,
    );
}

/// SDL's texture as the runner's RGBA frame sink: the core framebuffer goes
/// up untouched; the frontend's text is drawn over it afterwards.
struct TextureSink<'a, 'b> {
    texture: &'a mut Texture<'b>,
}

impl FrameSink for TextureSink<'_, '_> {
    fn frame(&mut self, rgba: &[u8]) -> Result<(), String> {
        self.texture
            .update(None, rgba, WIDTH * 4)
            .map_err(|error| format!("texture upload failed: {error}"))
    }
}

/// The windowed audio sink: play effects and switch captured music cues.
struct AudioSink<'a, 'b> {
    backend: &'a mut AudioBackend,
    bank: &'b AudioBank,
}

impl SoundSink for AudioSink<'_, '_> {
    fn sound(&mut self, sound: Sound) {
        // The game-over theme is music, not a one-shot effect.
        if sound != Sound::GameOver {
            self.backend.play(audio::sfx_for(sound, self.bank));
        }
        self.backend.music_event(sound);
    }

    fn music(&mut self, cue: MusicCue) {
        self.backend.set_music(music::music_for(cue, self.bank));
    }
}

/// Drains events with no audio device or soundtrack render.
struct NullSink;

impl SoundSink for NullSink {
    fn sound(&mut self, _sound: Sound) {}
}

/// Renders the same state-driven cues as the windowed backend into a mono WAV
/// buffer, one exact slice per simulation tick.
struct SoundtrackSink {
    engine: Engine,
    samples: Vec<f32>,
    scratch: Vec<f32>,
    rendered: usize,
    bank: AudioBank,
}

impl SoundtrackSink {
    fn new(bank: AudioBank) -> Self {
        let mut engine = Engine::new(SAMPLE_RATE);
        engine.play_music(music::music_for(MusicCue::Title, &bank));
        Self {
            engine,
            samples: Vec::new(),
            scratch: Vec::new(),
            rendered: 0,
            bank,
        }
    }
}

impl SoundSink for SoundtrackSink {
    fn sound(&mut self, sound: Sound) {
        if sound != Sound::GameOver {
            self.engine.play_sfx(audio::sfx_for(sound, &self.bank));
        }
        match sound {
            Sound::Pause => self.engine.pause_music(),
            Sound::Resume => self.engine.resume_music(),
            _ => {}
        }
    }

    fn music(&mut self, cue: MusicCue) {
        self.engine.play_music(music::music_for(cue, &self.bank));
    }

    fn tick(&mut self, frame: u64) {
        let target = ((frame as f64 * f64::from(SAMPLE_RATE)) / TICKS_PER_SECOND).round() as usize;
        if target > self.rendered {
            self.scratch.resize(target - self.rendered, 0.0);
            self.scratch.fill(0.0);
            self.engine.fill(&mut self.scratch);
            self.samples.extend_from_slice(&self.scratch);
            self.rendered = target;
        }
    }
}

fn load_pack(path: &Path) -> Result<gradius_assets::Pack, String> {
    gradius_assets::Pack::load(path).map_err(|error| {
        format!(
            "{error}\n  pack: {}\n  create it once from your own cartridge dump:\n      GRADIUS_ROM=<rom> scripts/extract-data\n  or let the game build it: gradius --rom <your Gradius (E) .nes file>",
            path.display()
        )
    })
}

/// Build the window and renderer, degrading from accelerated vsync to software
/// instead of refusing to start on machines with a weak driver.
fn build_canvas(
    video: &sdl2::VideoSubsystem,
    scale: u32,
) -> Result<(Canvas<Window>, bool), String> {
    let window = |video: &sdl2::VideoSubsystem| -> Result<Window, String> {
        video
            .window(TITLE, WIDTH as u32 * scale, HEIGHT as u32 * scale)
            .position_centered()
            .resizable()
            .build()
            .map_err(|error| format!("cannot create window: {error}"))
    };

    let (canvas, vsync) = match window(video)?
        .into_canvas()
        .present_vsync()
        .accelerated()
        .build()
    {
        Ok(canvas) => (canvas, true),
        Err(accelerated) => {
            eprintln!("warning: accelerated vsync renderer unavailable: {accelerated}");
            match window(video)?.into_canvas().present_vsync().build() {
                Ok(canvas) => (canvas, true),
                Err(vsync) => {
                    eprintln!("warning: vsync unavailable, pacing in software: {vsync}");
                    let canvas = window(video)?
                        .into_canvas()
                        .build()
                        .map_err(|error| format!("cannot create renderer: {error}"))?;
                    (canvas, false)
                }
            }
        }
    };

    // No logical size: run_windowed computes the destination rectangle itself
    // from the drawable size, so integer, aspect and stretch are all exact.
    Ok((canvas, vsync))
}

fn keyboard_frame(event_pump: &sdl2::EventPump, keybinds: &keybinds::Keybinds) -> InputFrame {
    let keys = event_pump.keyboard_state();
    keybinds.frame(&|code| keys.is_scancode_pressed(code))
}

/// Save the unscaled core framebuffer as a BMP. SDL writes the file, so the
/// frontend needs no image codec.
fn save_screenshot(framebuffer: &gradius_core::Framebuffer, frame: u64) -> Result<PathBuf, String> {
    let directory = PathBuf::from(SCREENSHOT_DIR);
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    let path = bmp::screenshot_path(&directory, frame);
    bmp::write_rgba(&path, WIDTH, HEIGHT, framebuffer.as_rgba())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_carries_pause_and_a_volume_notice() {
        let view = viewport::Viewport {
            x: 0,
            y: 0,
            width: WIDTH as u32 * 3,
            height: HEIGHT as u32 * 3,
        };
        let title = window_title(true, Fit::Integer, view, Some("volume 40%"));
        assert!(title.contains("paused"), "{title}");
        assert!(title.contains("volume 40%"), "{title}");
        assert!(title.contains("[integer 3x]"), "{title}");

        let title = window_title(false, Fit::Integer, view, Some("muted"));
        assert!(title.contains("muted"), "{title}");
        assert!(!title.contains("paused"), "{title}");
    }

    #[test]
    fn autoplay_starts_the_game_and_holds_fire() {
        assert!(!autoplay(0).start);
        assert!(autoplay(120).start);
        assert!(!autoplay(100).fire);
        assert!(autoplay(300).fire);
        assert!(autoplay(180).fire);
        assert!(autoplay(720).start);
    }

    fn keyboard_frame_from(codes: &[Scancode]) -> InputFrame {
        keybinds::Keybinds::default().frame(&|code| codes.contains(&code))
    }

    #[test]
    fn keyboard_p1_bindings_are_unchanged() {
        let frame = keyboard_frame_from(&[
            Scancode::Up,
            Scancode::Down,
            Scancode::Left,
            Scancode::Right,
            Scancode::Z,
            Scancode::X,
            Scancode::Return,
            Scancode::Backspace,
        ]);
        assert_eq!(
            frame.p1,
            Buttons {
                up: true,
                down: true,
                left: true,
                right: true,
                fire: true,
                power: true,
                start: true,
                select: true,
            }
        );
        assert_eq!(frame.p2, Buttons::NONE);
    }

    #[test]
    fn keyboard_p2_numpad_aliases_still_work() {
        let frame = keyboard_frame_from(&[
            Scancode::Kp8,
            Scancode::Kp5,
            Scancode::Kp4,
            Scancode::Kp6,
            Scancode::Kp0,
            Scancode::KpPeriod,
            Scancode::KpPlus,
            Scancode::KpMinus,
        ]);
        assert_eq!(
            frame.p2,
            Buttons {
                up: true,
                down: true,
                left: true,
                right: true,
                fire: true,
                power: true,
                start: true,
                select: true,
            }
        );
    }

    #[test]
    fn keyboard_p2_fallback_keys_work_without_a_numpad() {
        let one = |code| keyboard_frame_from(&[code]).p2;
        assert_eq!(
            one(Scancode::T),
            Buttons {
                up: true,
                ..Buttons::NONE
            }
        );
        assert_eq!(
            one(Scancode::G),
            Buttons {
                down: true,
                ..Buttons::NONE
            }
        );
        assert_eq!(
            one(Scancode::F),
            Buttons {
                left: true,
                ..Buttons::NONE
            }
        );
        assert_eq!(
            one(Scancode::H),
            Buttons {
                right: true,
                ..Buttons::NONE
            }
        );
        assert_eq!(
            one(Scancode::R),
            Buttons {
                fire: true,
                ..Buttons::NONE
            }
        );
        assert_eq!(
            one(Scancode::Y),
            Buttons {
                power: true,
                ..Buttons::NONE
            }
        );
        assert_eq!(
            one(Scancode::Num1),
            Buttons {
                start: true,
                ..Buttons::NONE
            }
        );
        assert_eq!(
            one(Scancode::Num2),
            Buttons {
                select: true,
                ..Buttons::NONE
            }
        );
    }

    #[test]
    fn no_scancode_is_bound_to_both_players() {
        let bindings = keybinds::Keybinds::default();
        let p1: Vec<Scancode> = bindings.p1.scancodes().collect();
        for code in bindings.p2.scancodes() {
            assert!(!p1.contains(&code), "{code:?} is bound to both players");
        }
    }

    #[test]
    fn a_p2_only_press_leaves_p1_idle() {
        let codes: Vec<Scancode> = keybinds::Keybinds::default().p2.scancodes().collect();
        let frame = keyboard_frame_from(&codes);
        assert_eq!(frame.p1, Buttons::NONE);
        assert_eq!(
            frame.p2,
            Buttons {
                up: true,
                down: true,
                left: true,
                right: true,
                fire: true,
                power: true,
                start: true,
                select: true,
            }
        );
    }

    #[test]
    fn screenshot_writes_a_bmp() {
        let path = std::env::temp_dir().join(format!("gradius-shot-{}.bmp", std::process::id()));
        let mut framebuffer = gradius_core::Framebuffer::new();
        framebuffer.pixel(0, 0, [255, 0, 0]);
        bmp::write_rgba(&path, WIDTH, HEIGHT, framebuffer.as_rgba()).expect("write bmp");
        let bytes = std::fs::read(&path).expect("read bmp");
        let _ = std::fs::remove_file(&path);
        assert_eq!(&bytes[..2], b"BM");
        assert!(bytes.len() > 54);
    }
}
