//! What the terminal app knows, apart from how it looks.
//!
//! The state and the commands live in sleepy-core's `Controller`, shared
//! with the window so the two cannot drift apart; this adds only what is
//! the terminal's own: which key means which command, and the field
//! editing that goes with it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sleepy_core::{Command, Controller, Options, Status, is_when_prefix};
use std::time::Instant;

/// Which key means which command, in the terminal. What is not here is
/// the field's own: while it is being edited (idle), the plain
/// characters a when could still wear - `d`, space, everything
/// humantime reads - type into it, and the rest are refused at the
/// door. The window transliterates its own keys to the same commands;
/// what a command does is core's, shared, so neither frontend can
/// drift.
fn command(key: KeyEvent, idle: bool) -> Option<Command> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Some(Command::Quit),
        KeyCode::Char('q') if !ctrl => Some(Command::Quit),
        // The dry-run toggle is a safety valve that must work at any
        // moment - flipping mid-countdown saves the machine without
        // aborting the timer - so it rides on a chord no text field
        // has a use for, leaving plain `d` to type: `2d` needs it.
        KeyCode::Char('d') if ctrl => Some(Command::ToggleDryRun),
        // `k` keeps the screen awake; it is a command at any moment,
        // for the field only takes what a when could still become,
        // and `k` never begins one.
        KeyCode::Char('k') if !ctrl => Some(Command::ToggleKeepAwake),
        KeyCode::Esc if !ctrl => Some(Command::Escape),
        KeyCode::Up | KeyCode::BackTab if !ctrl => Some(Command::PreviousMode),
        KeyCode::Down | KeyCode::Tab if !ctrl => Some(Command::NextMode),
        // Arming waits for an idle countdown - `apply` is the one that
        // says so, as it does in the window.
        KeyCode::Enter if !ctrl => Some(Command::Arm),
        // Space is humantime's separator, so while the field is being
        // edited (idle) it is not a command but a character; once a
        // countdown runs it pauses.
        KeyCode::Char(' ') if !ctrl && !idle => Some(Command::PauseResume),
        KeyCode::Char('+' | '=') if !ctrl => Some(Command::Longer),
        KeyCode::Char('-') if !ctrl => Some(Command::Shorter),
        _ => None,
    }
}

/// The whole application state.
#[derive(Debug)]
pub struct App {
    /// Everything the app knows and does, shared with the window.
    pub controller: Controller,
}

impl App {
    /// Starts from the command line's choices: `mode` highlights itself,
    /// `duration` fills the field - and, being an explicit request, arms
    /// the countdown at once, so the screen opens as a pause button rather
    /// than an ignition.
    pub fn new(options: Options) -> Self {
        Self {
            controller: Controller::new(options),
        }
    }

    /// Handles a key press; returns whether the app should exit.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let idle = self.controller.timer.status() == Status::Idle;
        match command(key, idle) {
            Some(command) => self.controller.apply(command),
            // What is no command is the field's, while the field is
            // being edited: the terminal's mini line editor is its own
            // to drive, where the window leaves this to a real text
            // field.
            None if idle => match key.code {
                KeyCode::Backspace => {
                    self.controller.input.pop();
                }
                KeyCode::Char('u') if ctrl => self.controller.input.clear(),
                // The field holds only what a when could still become:
                // a character that completes no when is refused at the
                // door, so the error hint below the field stays a
                // correction rather than a surprise.
                KeyCode::Char(c) if !ctrl => {
                    let mut typed = self.controller.input.clone();
                    typed.push(c);
                    if is_when_prefix(&typed) {
                        self.controller.input.push(c);
                    }
                }
                _ => {}
            },
            None => {}
        }
        !self.controller.running()
    }

    /// A heartbeat passed: let the timer count. Returns whether the app
    /// should exit, like [`Self::on_key`] does - and it should the moment
    /// the countdown ends, so main can draw one last frame and then
    /// perform the action with the screen still saying so.
    pub fn on_tick(&mut self) -> bool {
        if self.controller.tick(Instant::now()) {
            self.controller.quit();
        }
        !self.controller.running()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sleepy_core::{Backend, SleepAction};

    /// An app built by hand, so the tests never ask the machine what it
    /// can do (`detect` would spawn processes here).
    fn app() -> App {
        App::new(Options {
            backend: Backend::Real,
            keep_screen_on: false,
            when: None,
            mode: None,
            modes: vec![SleepAction::Suspend],
        })
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.on_key(press(KeyCode::Char(c)));
        }
    }

    #[test]
    fn the_field_keeps_humantimes_letters_while_idle() {
        let mut app = app();
        app.controller.input.clear();
        type_text(&mut app, "30 min 2d");
        assert_eq!(app.controller.input, "30 min 2d");
        // `d` and space were typed, not spent: nothing was toggled and
        // nothing was armed.
        assert_eq!(app.controller.backend, Backend::Real);
        assert_eq!(app.controller.timer.status(), Status::Idle);
    }

    #[test]
    fn d_is_always_a_letter() {
        let mut app = app();
        app.controller.input.clear();
        type_text(&mut app, "2d");
        assert_eq!(app.controller.input, "2d");
        // Even a running countdown does not turn `d` into a command.
        assert!(app.controller.arm_selected());
        app.on_key(press(KeyCode::Char('d')));
        assert_eq!(app.controller.backend, Backend::Real);
        assert_eq!(app.controller.timer.status(), Status::Armed);
    }

    #[test]
    fn ctrl_d_toggles_dry_run_at_any_moment() {
        let mut app = app();
        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        app.on_key(ctrl_d);
        assert_eq!(app.controller.backend, Backend::DryRun);
        // The safety valve: mid-countdown too, flipping the backend
        // without aborting the timer - and the field never sees the
        // chord.
        let field = app.controller.input.clone();
        assert!(app.controller.arm_selected());
        app.on_key(ctrl_d);
        assert_eq!(app.controller.backend, Backend::Real);
        assert_eq!(app.controller.timer.status(), Status::Armed);
        assert_eq!(app.controller.input, field);
    }

    #[test]
    fn space_types_while_idle_and_pauses_while_counting() {
        let mut app = app();
        app.controller.input.clear();
        type_text(&mut app, "30 min");
        assert_eq!(app.controller.input, "30 min");
        assert!(app.controller.arm_selected());
        // Space typed a separator a moment ago; now it pauses.
        app.on_key(press(KeyCode::Char(' ')));
        assert_eq!(app.controller.timer.status(), Status::Paused);
        app.on_key(press(KeyCode::Char(' ')));
        assert_eq!(app.controller.timer.status(), Status::Armed);
    }

    #[test]
    fn k_is_a_command_even_while_typing() {
        let mut app = app();
        app.controller.input.clear();
        app.on_key(press(KeyCode::Char('k')));
        assert!(app.controller.keep_screen_on);
        // The field never saw the key: `k` is a command at any moment.
        assert_eq!(app.controller.input, "");
    }

    #[test]
    fn the_field_refuses_what_no_when_could_wear() {
        let mut app = app();
        app.controller.input.clear();
        type_text(&mut app, "23:4");
        assert_eq!(app.controller.input, "23:4");
        // A letter the clock does not wear is refused at the door: no
        // error to read, just a key that does nothing.
        type_text(&mut app, "x");
        assert_eq!(app.controller.input, "23:4");
        // An hour the day does not have never enters either.
        app.controller.input.clear();
        type_text(&mut app, "25:");
        assert_eq!(app.controller.input, "25");
        // But a prefix of a real when is never refused.
        app.controller.input.clear();
        type_text(&mut app, "1h30m");
        assert_eq!(app.controller.input, "1h30m");
    }

    #[test]
    fn escape_aborts_then_leaves() {
        let mut app = app();
        assert!(app.controller.arm_selected());
        // The first escape saves the machine; the shell waits.
        assert!(!app.on_key(press(KeyCode::Esc)));
        assert_eq!(app.controller.timer.status(), Status::Idle);
        // The second one, with nothing left to save, leaves.
        assert!(app.on_key(press(KeyCode::Esc)));
    }
}
