//! What the app knows, apart from how it looks.
//!
//! The state is small enough to keep in plain fields: what the machine can
//! do, which of that is highlighted, what has been typed, and the timer.
//! Decisions are delegated to sleepy-core; this only holds the answers the
//! screen will show.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sleepy_core::{Backend, ParseError, SleepAction, Timer, detect, parse_duration};
use std::time::Duration;

/// The duration offered to a user who types nothing.
const DEFAULT: &str = "30 min";

/// The whole application state.
#[derive(Debug)]
pub struct App {
    /// Cleared when the user asks to leave.
    running: bool,
    /// What this machine can do, in display order.
    pub modes: Vec<SleepAction>,
    /// Index into `modes`.
    pub selected: usize,
    /// The duration field.
    pub input: String,
    /// The countdown itself.
    pub timer: Timer,
    /// Whether to really sleep or only say so.
    pub backend: Backend,
}

impl App {
    pub fn new(backend: Backend) -> Self {
        Self {
            running: true,
            modes: detect(),
            selected: 0,
            input: DEFAULT.into(),
            timer: Timer::new(),
            backend,
        }
    }

    /// The highlighted mode, if the machine offered any.
    pub fn selected_action(&self) -> Option<SleepAction> {
        self.modes.get(self.selected).copied()
    }

    /// What the duration field means right now: a duration, or the reason
    /// it is not one yet.
    pub fn duration(&self) -> Result<Duration, ParseError> {
        parse_duration(&self.input)
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
