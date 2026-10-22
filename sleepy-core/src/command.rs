//! The one thing the user can ask sleepy to do, from a key or a click.
//!
//! Each frontend translates its own keys into these - crossterm reports
//! characters, egui reports logical keys, and a window has a focused text
//! field where a terminal has an idle timer - but what a command *does*
//! lives once, in [`crate::Controller::apply`], so a key means the same
//! thing in the terminal as in the window.

/// Anything the user can ask sleepy to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Move the mode highlight up.
    PreviousMode,
    /// Move the mode highlight down.
    NextMode,
    /// Switch between really sleeping and only saying so.
    ToggleDryRun,
    /// Switch the screen inhibition.
    ToggleKeepAwake,
    /// Start the countdown with the highlighted mode and typed duration.
    Arm,
    /// Pause a running countdown, or resume a paused one.
    PauseResume,
    /// Abort a countdown; when there is none, leave.
    Escape,
    /// Put a minute back on the countdown.
    Longer,
    /// Take a minute off the countdown.
    Shorter,
    /// Leave.
    Quit,
}
