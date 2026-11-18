//! The in-game options menu: state, input handling and drawing.
//!
//! Pure logic over [`Settings`]; the event loop feeds it [`Input`]s, applies
//! what changed and pauses the game while it is open.

use crate::options::MAX_HITS;
use crate::settings::Settings;
use crate::text::{Color, Layer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Close,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Resume,
    StartStage,
    Hits,
    TwoPlayers,
    Stage5Slowdown,
    Volume,
    Fullscreen,
    Quit,
}

const ITEMS: [Item; 8] = [
    Item::Resume,
    Item::StartStage,
    Item::Hits,
    Item::TwoPlayers,
    Item::Stage5Slowdown,
    Item::Volume,
    Item::Fullscreen,
    Item::Quit,
];

const WHITE: Color = [255, 255, 255, 255];
const YELLOW: Color = [255, 224, 64, 255];
const GREY: Color = [190, 194, 205, 255];
const SHADE: Color = [4, 6, 14, 225];
const HIGHLIGHT: Color = [255, 224, 64, 48];
/// The column the menu sits in, centred, in layer units.
const PANEL: f32 = 250.0;

#[derive(Clone, Debug)]
pub struct Menu {
    pub settings: Settings,
    selected: usize,
    confirm_quit: bool,
}

impl Menu {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            selected: 0,
            confirm_quit: false,
        }
    }

    /// Start from the top again, as when the menu is opened.
    pub fn open(&mut self) {
        self.selected = 0;
        self.confirm_quit = false;
    }

    pub fn handle(&mut self, input: Input) -> Outcome {
        if self.confirm_quit {
            match input {
                Input::Accept => return Outcome::Quit,
                Input::Back => self.confirm_quit = false,
                _ => {}
            }
            return Outcome::Stay;
        }
        let item = ITEMS[self.selected];
        match input {
            Input::Up => self.selected = (self.selected + ITEMS.len() - 1) % ITEMS.len(),
            Input::Down => self.selected = (self.selected + 1) % ITEMS.len(),
            Input::Back => return Outcome::Close,
            Input::Left => self.change(item, -1),
            Input::Right => self.change(item, 1),
            Input::Accept => match item {
                Item::Resume => return Outcome::Close,
                Item::Quit => self.confirm_quit = true,
                Item::TwoPlayers | Item::Stage5Slowdown | Item::Fullscreen => {
                    self.change(item, 1);
                }
                Item::StartStage | Item::Hits | Item::Volume => {}
            },
        }
        Outcome::Stay
    }

    fn change(&mut self, item: Item, direction: i32) {
        let settings = &mut self.settings;
        match item {
            Item::StartStage => {
                let next = i32::from(settings.start_stage) + direction;
                settings.start_stage = next.clamp(1, 7) as u8;
            }
            Item::Hits => {
                // 0 is Invincible, one step past 99.
                settings.hits = match (settings.hits, direction) {
                    (MAX_HITS, 1..) => 0,
                    (0, ..=-1) => MAX_HITS,
                    (0, _) => 0,
                    (hits, _) => (i32::from(hits) + direction).clamp(1, i32::from(MAX_HITS)) as u8,
                };
            }
            Item::TwoPlayers => settings.simultaneous_2p = !settings.simultaneous_2p,
            Item::Stage5Slowdown => settings.stage5_slowdown = !settings.stage5_slowdown,
            Item::Fullscreen => settings.fullscreen = !settings.fullscreen,
            Item::Volume => {
                // Steps of 10, snapping a +/- (5%) value onto the grid.
                let tens = i32::from(settings.volume);
                let next = if direction > 0 {
                    (tens / 10 + 1) * 10
                } else {
                    (tens + 9) / 10 * 10 - 10
                };
                settings.volume = next.clamp(0, 100) as u8;
            }
            Item::Resume | Item::Quit => {}
        }
    }

    fn label(&self, item: Item) -> String {
        let on = |flag: bool| if flag { "On" } else { "Off" };
        match item {
            Item::Resume => "Resume".to_string(),
            Item::StartStage => format!("Start stage: < {} >", self.settings.start_stage),
            Item::Hits if self.settings.hits == 0 => "Ship hits: < Invincible >".to_string(),
            Item::Hits => format!("Ship hits: < {} >", self.settings.hits),
            Item::TwoPlayers => format!("2 players at once: {}", on(self.settings.simultaneous_2p)),
            Item::Stage5Slowdown => {
                format!("Stage 5 slowdown: {}", on(self.settings.stage5_slowdown))
            }
            Item::Volume => format!("Volume: < {} >", self.settings.volume),
            Item::Fullscreen => format!("Fullscreen: {}", on(self.settings.fullscreen)),
            Item::Quit => "Quit".to_string(),
        }
    }

    /// The lines of the menu, for tests and drawing: `(text, highlighted)`.
    pub fn lines(&self) -> Vec<(String, bool)> {
        if self.confirm_quit {
            return vec![
                ("Quit?".to_string(), true),
                ("Enter = yes, Esc = no".to_string(), false),
            ];
        }
        let lines: Vec<(String, bool)> = ITEMS
            .iter()
            .enumerate()
            .map(|(index, item)| (self.label(*item), index == self.selected))
            .collect();
        lines
    }

    /// Dim the game and draw the menu over it.
    pub fn draw(&self, layer: &mut Layer, in_game: bool) {
        // Nearly opaque, so the title art behind it does not fight the text.
        layer.band(0.0, 240.0, SHADE);
        let left = (layer.units_wide() - PANEL) / 2.0;
        layer.text(left, 12.0, "Options", 18.0, true, WHITE);
        let mut y = 40.0;
        for (text, highlighted) in self.lines() {
            if highlighted {
                layer.rect(left - 6.0, y - 2.0, PANEL + 12.0, 18.0, HIGHLIGHT);
            }
            let color = if highlighted { YELLOW } else { WHITE };
            layer.text(left, y, &text, 12.5, highlighted, color);
            y += 19.0;
        }
        if self.confirm_quit {
            return;
        }
        if ITEMS[self.selected] == Item::TwoPlayers {
            layer.text(
                left,
                y + 2.0,
                "Choose 2 PLAYERS on the title screen",
                8.5,
                false,
                GREY,
            );
        }
        if in_game {
            let note = "Hits, stage, slowdown and 2P: next game";
            layer.text(left, y + 13.0, note, 8.5, false, GREY);
        }
        layer.text_centered(
            222.0,
            "Arrows move   Enter picks   Esc goes back",
            8.5,
            false,
            GREY,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Menu {
        Menu::new(Settings::default())
    }

    fn goto(menu: &mut Menu, steps: usize) {
        for _ in 0..steps {
            menu.handle(Input::Down);
        }
    }

    #[test]
    fn navigation_wraps_both_ways() {
        let mut menu = menu();
        menu.handle(Input::Up);
        assert_eq!(ITEMS[menu.selected], Item::Quit);
        menu.handle(Input::Down);
        assert_eq!(ITEMS[menu.selected], Item::Resume);
    }

    #[test]
    fn hits_stay_within_one_to_ninety_nine() {
        let mut menu = menu();
        goto(&mut menu, 2);
        menu.handle(Input::Left);
        assert_eq!(menu.settings.hits, 1);
        menu.settings.hits = 98;
        menu.handle(Input::Right);
        assert_eq!(menu.settings.hits, 99);
        menu.handle(Input::Left);
        assert_eq!(menu.settings.hits, 98);
    }

    #[test]
    fn invincible_follows_ninety_nine_and_returns_to_it() {
        let mut menu = menu();
        goto(&mut menu, 2);
        menu.settings.hits = 99;
        menu.handle(Input::Right);
        assert_eq!(menu.settings.hits, 0);
        menu.handle(Input::Right);
        assert_eq!(menu.settings.hits, 0);
        menu.handle(Input::Left);
        assert_eq!(menu.settings.hits, 99);
    }

    #[test]
    fn start_stage_cycles_one_to_seven_and_stops_at_the_limits() {
        let mut menu = menu();
        goto(&mut menu, 1);
        assert_eq!(ITEMS[menu.selected], Item::StartStage);
        menu.handle(Input::Left);
        assert_eq!(menu.settings.start_stage, 1);
        menu.settings.start_stage = 6;
        menu.handle(Input::Right);
        assert_eq!(menu.settings.start_stage, 7);
        menu.handle(Input::Right);
        assert_eq!(menu.settings.start_stage, 7);
        menu.handle(Input::Left);
        assert_eq!(menu.settings.start_stage, 6);
        assert!(menu.lines()[1].0.contains("Start stage: < 6 >"));
    }

    #[test]
    fn toggles_flip_with_accept_and_arrows() {
        let mut menu = menu();
        goto(&mut menu, 3);
        menu.handle(Input::Accept);
        assert!(menu.settings.simultaneous_2p);
        menu.handle(Input::Left);
        assert!(!menu.settings.simultaneous_2p);
        goto(&mut menu, 3);
        menu.handle(Input::Accept);
        assert!(menu.settings.fullscreen);
    }

    #[test]
    fn stage_five_slowdown_is_off_until_toggled() {
        let mut menu = menu();
        goto(&mut menu, 4);
        assert_eq!(ITEMS[menu.selected], Item::Stage5Slowdown);
        assert!(!menu.settings.stage5_slowdown);
        assert!(menu.lines()[4].0.contains("Stage 5 slowdown: Off"));
        menu.handle(Input::Accept);
        assert!(menu.settings.stage5_slowdown);
        assert!(menu.lines()[4].0.contains("Stage 5 slowdown: On"));
        menu.handle(Input::Right);
        assert!(!menu.settings.stage5_slowdown);
        menu.handle(Input::Left);
        assert!(menu.settings.stage5_slowdown);
    }

    #[test]
    fn volume_moves_in_tens_and_snaps_to_the_grid() {
        let mut menu = Menu::new(Settings {
            volume: 95,
            ..Settings::default()
        });
        goto(&mut menu, 5);
        menu.handle(Input::Right);
        assert_eq!(menu.settings.volume, 100);
        menu.handle(Input::Right);
        assert_eq!(menu.settings.volume, 100);
        menu.settings.volume = 95;
        menu.handle(Input::Left);
        assert_eq!(menu.settings.volume, 90);
        menu.handle(Input::Left);
        assert_eq!(menu.settings.volume, 80);
        menu.settings.volume = 5;
        menu.handle(Input::Left);
        assert_eq!(menu.settings.volume, 0);
        menu.handle(Input::Left);
        assert_eq!(menu.settings.volume, 0);
    }

    #[test]
    fn resume_and_back_close() {
        let mut menu = menu();
        assert_eq!(menu.handle(Input::Accept), Outcome::Close);
        assert_eq!(menu.handle(Input::Back), Outcome::Close);
    }

    #[test]
    fn quit_needs_confirmation() {
        let mut menu = menu();
        menu.handle(Input::Up);
        assert_eq!(menu.handle(Input::Accept), Outcome::Stay);
        assert!(menu.lines()[1].0.contains("Enter = yes"));
        assert_eq!(menu.handle(Input::Back), Outcome::Stay);
        assert_eq!(menu.lines().len(), ITEMS.len());
        menu.handle(Input::Accept);
        assert_eq!(menu.handle(Input::Accept), Outcome::Quit);
    }

    #[test]
    fn drawing_changes_the_frame() {
        let mut layer = Layer::new(768, 720);
        menu().draw(&mut layer, true);
        let alpha = |x: usize, y: usize| layer.rgba[(y * 768 + x) * 4 + 3];
        assert_eq!(alpha(5, 5), SHADE[3], "the whole frame is dimmed");
        let yellow = layer
            .rgba
            .chunks(4)
            .filter(|p| p[..3] == [255, 224, 64] && p[3] == 255)
            .count();
        assert!(yellow > 100, "the active line is not drawn: {yellow}");
        let white = layer
            .rgba
            .chunks(4)
            .filter(|p| p[..3] == [255; 3] && p[3] == 255)
            .count();
        assert!(white > 300, "the other lines are not drawn: {white}");
    }
}
