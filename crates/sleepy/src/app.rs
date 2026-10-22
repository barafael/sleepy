//! What the app knows, apart from how it looks.
//!
//! The state is small enough to keep in plain fields: what the machine can
//! do, which of that is highlighted, what has been typed, and the timer.
//! Decisions are delegated to sleepy-core; this only holds the answers the
//! screen will show and turns key presses into calls.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sleepy_core::{
    Backend, Error, KeepAwake, ParseError, SleepAction, Status, Timer, TimerEvent, detect, execute,
    parse_duration,
};
use std::time::{Duration, Instant};

/// The duration offered to a user who types nothing.
const DEFAULT: &str = "30 min";

/// What one `+` or `-` is worth, in seconds.
const NUDGE: i64 = 60;

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
    /// Whether to hold the screen awake while the countdown runs.
    pub keep_screen_on: bool,
    /// The inhibition itself, held while a countdown runs with
    /// `keep_screen_on`; dropping the app lets it go.
    awake: KeepAwake,
    /// The action a finished countdown ran, for the shell to report.
    pub fired: Option<SleepAction>,
    /// Why the machine is still awake after a fired countdown, if it is.
    pub failure: Option<Error>,
}

impl App {
    /// Starts from the command line's choices: `mode` highlights itself,
    /// `duration` fills the field - and, being an explicit request, arms
    /// the countdown at once, so the screen opens as a pause button rather
    /// than an ignition.
    pub fn new(
        backend: Backend,
        keep_screen_on: bool,
        duration: Option<String>,
        mode: Option<SleepAction>,
    ) -> Self {
        let modes = detect();
        let selected = mode
            .and_then(|want| modes.iter().position(|have| *have == want))
            .unwrap_or(0);
        let arm = duration.is_some();
        let mut app = Self {
            running: true,
            modes,
            selected,
            input: duration.unwrap_or_else(|| DEFAULT.into()),
            timer: Timer::new(),
            backend,
            keep_screen_on,
            awake: KeepAwake::new(),
            fired: None,
            failure: None,
        };
        if arm {
            app.arm_selected();
        }
        app
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

    /// Arms the highlighted mode with the typed duration; refuses - and
    /// lets the screen show why - when either is missing.
    pub fn arm_selected(&mut self) -> bool {
        match (self.selected_action(), parse_duration(&self.input)) {
            (Some(action), Ok(duration)) => {
                let armed = self.timer.arm(action, duration);
                if armed && self.keep_screen_on {
                    self.awake.start();
                }
                armed
            }
            _ => false,
        }
    }

    /// Switches between really sleeping and only saying so.
    pub fn toggle_backend(&mut self) {
        self.backend = match self.backend {
            Backend::Real => Backend::DryRun,
            Backend::DryRun => Backend::Real,
        };
    }

    /// Switches the screen inhibition, applying the change to a running
    /// countdown: flipping it on mid-count grabs the inhibition now, and
    /// flipping it off lets go.
    pub fn toggle_keep_screen_on(&mut self) {
        self.keep_screen_on = !self.keep_screen_on;
        if matches!(self.timer.status(), Status::Armed | Status::Paused) {
            if self.keep_screen_on {
                self.awake.start();
            } else {
                self.awake.stop();
            }
        }
    }

    /// Moves the highlight, wrapping at both ends.
    fn move_selection(&mut self, step: isize) {
        let count = isize::try_from(self.modes.len()).unwrap_or(0);
        if count == 0 {
            return;
        }
        let next = (isize::try_from(self.selected).unwrap_or(0) + step).rem_euclid(count);
        self.selected = usize::try_from(next).unwrap_or(0);
    }

    /// Handles a key press; returns whether the app should exit.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let idle = self.timer.status() == Status::Idle;
        match key.code {
            KeyCode::Char('c') if ctrl => self.running = false,
            KeyCode::Char('q') => self.running = false,
            // `d` is a humantime letter (days) and is sacrificed to the
            // cause anyway, like `q` before it: the toggle is a safety
            // valve that must work at any moment - flipping to a dry run
            // mid-countdown saves the machine without aborting the timer.
            KeyCode::Char('d') if !ctrl => self.toggle_backend(),
            // `k` keeps the screen awake; unlike `d` it is no humantime
            // letter, so typing loses nothing.
            KeyCode::Char('k') if !ctrl => self.toggle_keep_screen_on(),
            KeyCode::Esc => {
                // Escape first saves the machine from sleep, and only a
                // second press returns the shell.
                if idle {
                    self.running = false;
                } else {
                    self.timer.abort();
                    self.awake.stop();
                }
            }
            KeyCode::Up | KeyCode::BackTab => self.move_selection(-1),
            KeyCode::Down | KeyCode::Tab => self.move_selection(1),
            KeyCode::Enter if idle => {
                self.arm_selected();
            }
            KeyCode::Backspace if idle => {
                self.input.pop();
            }
            KeyCode::Char('u') if ctrl && idle => self.input.clear(),
            KeyCode::Char(' ') => match self.timer.status() {
                Status::Armed => self.timer.pause(),
                Status::Paused => self.timer.resume(),
                Status::Idle | Status::Fired => {}
            },
            KeyCode::Char('+') | KeyCode::Char('=') => self.timer.adjust(NUDGE),
            KeyCode::Char('-') => self.timer.adjust(-NUDGE),
            KeyCode::Char(c) if !ctrl && idle => self.input.push(c),
            _ => {}
        }
        !self.running
    }

    /// A heartbeat passed: let the timer count. Returns whether the app
    /// should exit, like [`Self::on_key`] does - and it should the moment
    /// the countdown ends, so main can draw one last frame and then
    /// [`Self::perform`] the action with the screen still saying so.
    pub fn on_tick(&mut self) -> bool {
        if self.timer.tick(Instant::now()) == TimerEvent::Fired {
            self.fired = Some(
                self.timer
                    .action()
                    .expect("a fired timer knows what it is doing"),
            );
            self.running = false;
        }
        !self.running
    }

    /// Does the thing the countdown was armed for. Main calls this after
    /// the last draw: on a real sleep this call does not return until the
    /// machine wakes, and the dry run answers at once.
    pub fn perform(&mut self) {
        // Whatever happens next, the screen is on its own from here.
        self.awake.stop();
        if let Some(action) = self.fired {
            self.failure = execute(action, self.backend).err();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    /// An app built by hand, so the tests never ask the machine what it
    /// can do (`detect` would spawn processes here).
    fn app() -> App {
        App {
            running: true,
            modes: vec![SleepAction::Suspend],
            selected: 0,
            input: DEFAULT.into(),
            timer: Timer::new(),
            backend: Backend::Real,
            keep_screen_on: false,
            awake: KeepAwake::new(),
            fired: None,
            failure: None,
        }
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn d_toggles_dry_run_without_typing() {
        let mut app = app();
        assert_eq!(app.backend, Backend::Real);
        app.on_key(press(KeyCode::Char('d')));
        assert_eq!(app.backend, Backend::DryRun);
        app.on_key(press(KeyCode::Char('d')));
        assert_eq!(app.backend, Backend::Real);
        // The field never saw the key: `d` is a command, not a letter.
        assert_eq!(app.input, DEFAULT);
    }

    #[test]
    fn dry_run_can_be_flipped_while_counting() {
        // The safety valve: mid-countdown, a flip to dry run saves the
        // machine without aborting the timer.
        let mut app = app();
        assert!(app.arm_selected());
        app.on_key(press(KeyCode::Char('d')));
        assert_eq!(app.backend, Backend::DryRun);
        assert_eq!(app.timer.status(), Status::Armed);
    }

    #[test]
    fn k_toggles_keep_awake_without_typing() {
        let mut app = app();
        assert!(!app.keep_screen_on);
        app.on_key(press(KeyCode::Char('k')));
        assert!(app.keep_screen_on);
        app.on_key(press(KeyCode::Char('k')));
        assert!(!app.keep_screen_on);
        // The field never saw the key either.
        assert_eq!(app.input, DEFAULT);
    }
}
