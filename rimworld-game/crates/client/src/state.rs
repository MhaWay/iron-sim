//! Pure UI state machine + helpers. eframe-free so headless tests can drive it.

use crate::game::Preset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    NewColony,
    Playing,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub seed: u64,
    pub preset: Preset,
    pub difficulty: u8, // 0 easy, 1 normal, 2 hard
    pub sandbox: bool,
    pub locale: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            seed: 12345,
            preset: Preset::Homestead,
            difficulty: 1,
            sandbox: true,
            locale: "en".to_string(),
        }
    }
}

impl Config {
    pub fn valid(&self) -> bool {
        self.seed != 0
            && matches!(
                self.locale.as_str(),
                "en" | "it" | "es" | "de" | "fr"
            )
            && self.difficulty < 3
    }
}

pub const SAVE_PATH: &str = "colony_save.bin";

#[derive(Clone, Debug)]
pub struct UiState {
    pub screen: Screen,
    pub cfg: Config,
    pub paused: bool,
    pub speed: u8,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            screen: Screen::Menu,
            cfg: Config::default(),
            paused: false,
            speed: 1,
        }
    }
}

impl UiState {
    pub fn open_new(&mut self) {
        self.screen = Screen::NewColony;
    }
    pub fn back_to_menu(&mut self) {
        self.screen = Screen::Menu;
        self.paused = false;
    }
    pub fn start(&mut self) -> bool {
        if !self.cfg.valid() {
            return false;
        }
        self.paused = false;
        self.speed = 1;
        self.screen = Screen::Playing;
        true
    }
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }
    pub fn set_speed(&mut self, s: u8) {
        self.speed = s.clamp(1, 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_machine_transitions() {
        let mut s = UiState::default();
        assert_eq!(s.screen, Screen::Menu);
        s.open_new();
        assert_eq!(s.screen, Screen::NewColony);
        assert!(s.start());
        assert_eq!(s.screen, Screen::Playing);
        s.toggle_pause();
        assert!(s.paused);
        s.set_speed(3);
        assert_eq!(s.speed, 3);
        s.back_to_menu();
        assert_eq!(s.screen, Screen::Menu);
        assert!(!s.paused);
    }
    #[test]
    fn invalid_config_blocks_start() {
        let mut s = UiState::default();
        s.cfg.seed = 0;
        assert!(!s.start());
        assert_eq!(s.screen, Screen::Menu);
    }
    #[test]
    fn all_five_locales_valid() {
        let mut s = UiState::default();
        for l in crate::i18n::LOCALES {
            s.cfg.locale = l.to_string();
            assert!(s.cfg.valid(), "locale {l} must be valid");
        }
    }
}
