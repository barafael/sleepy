//! What the app knows, apart from how it looks.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The whole application state.
#[derive(Debug)]
pub struct App {
    /// Cleared when the user asks to leave.
    running: bool,
}

impl App {
    pub fn new() -> Self {
        Self { running: true }
    }

    /// Handles a key press; returns whether the app should exit.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        let ctrl_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) || ctrl_c {
            self.running = false;
        }
        !self.running
    }

    /// A heartbeat passed; the next commits give this a countdown to watch.
    pub fn on_tick(&mut self) {}
}
