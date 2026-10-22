//! Pure UI state machine + view helpers. eframe-free so it can be tested headless.

use crate::config::MatchConfig;

/// Screen the app is currently showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Custom,
    Playing,
}

/// The state machine used by the eframe app. Transitions are explicit functions so tests can
/// drive them without a window.
#[derive(Clone, Debug)]
pub struct UiState {
    pub screen: Screen,
    pub cfg: MatchConfig,
    /// paused = sim does not advance but view is live.
    pub paused: bool,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            screen: Screen::Menu,
            cfg: MatchConfig::default(),
            paused: false,
        }
    }
}

impl UiState {
    pub fn open_custom(&mut self) {
        self.screen = Screen::Custom;
    }
    pub fn back_to_menu(&mut self) {
        self.screen = Screen::Menu;
        self.paused = false;
    }
    /// Validate + start a match. Returns false (and stays on Custom) if invalid.
    pub fn start_match(&mut self) -> bool {
        if !self.cfg.valid() {
            return false;
        }
        self.paused = false;
        self.screen = Screen::Playing;
        true
    }
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }
}
