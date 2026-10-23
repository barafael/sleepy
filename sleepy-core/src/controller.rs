//! What both frontends know and do, apart from how they look.
//!
//! The terminal and the window keep the same state and answer the same
//! commands - arm, pause, abort, toggle, nudge - so the answers live here
//! once, where they can be tested without a terminal or a window. Each
//! frontend wraps a `Controller`, adds its own presentation state, and
//! turns its own keys into calls here.

use crate::action::SleepAction;
use crate::awake::KeepAwake;
use crate::backend::{Backend, Error, execute};
use crate::cli::Options;
use crate::command::Command;
use crate::input::{ParseError, When, format_clock, parse_when};
use crate::timer::{Event as TimerEvent, Status, Timer};
use humantime::format_duration;
use std::time::{Duration, SystemTime};
use web_time::Instant;

/// The duration offered to a user who types nothing.
pub const DEFAULT_DURATION: &str = "30 min";

/// What one `+` or `-` is worth, in seconds.
pub const NUDGE: i64 = 60;

/// The wall clock's now. std's own panics on wasm, where there is no
/// clock to read; the browser's milliseconds since the epoch are a
/// `SystemTime` like any other, and arithmetic on one asks nothing of
/// the platform.
fn system_now() -> SystemTime {
    #[cfg(not(target_arch = "wasm32"))]
    {
        SystemTime::now()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let millis = js_sys::Date::now();
        SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(millis.max(0.0) as u64)
    }
}

/// The whole application state, apart from how it is drawn.
#[derive(Debug)]
pub struct Controller {
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
    /// `keep_screen_on`; dropping the controller lets it go.
    awake: KeepAwake,
    /// The action a finished countdown ran, for the frontend to report.
    pub fired: Option<SleepAction>,
    /// Why the machine is still awake after a fired countdown, if it is.
    pub failure: Option<Error>,
}

impl Controller {
    /// Starts from the command line's choices: `mode` highlights itself,
    /// `duration` fills the field - and, being an explicit request, arms
    /// the countdown at once, so the screen opens as a pause button rather
    /// than an ignition.
    pub fn new(options: Options) -> Self {
        let selected = options
            .mode
            .and_then(|want| options.modes.iter().position(|have| *have == want))
            .unwrap_or(0);
        let arm = options.when.is_some();
        let mut controller = Self {
            running: true,
            modes: options.modes,
            selected,
            input: options.when.unwrap_or_else(|| DEFAULT_DURATION.into()),
            timer: Timer::new(),
            backend: options.backend,
            keep_screen_on: options.keep_screen_on,
            awake: KeepAwake::new(),
            fired: None,
            failure: None,
        };
        if arm {
            controller.arm_selected();
        }
        controller
    }

    /// Whether the user has not yet asked to leave.
    pub fn running(&self) -> bool {
        self.running
    }

    /// The user asked to leave; the frontend closes on its way out.
    pub fn quit(&mut self) {
        self.running = false;
    }

    /// The highlighted mode, if the machine offered any.
    pub fn selected_action(&self) -> Option<SleepAction> {
        self.modes.get(self.selected).copied()
    }

    /// What the field means right now: a duration or a clock time, or
    /// the reason it is neither yet.
    ///
    /// # Errors
    ///
    /// Whatever [`crate::input::ParseError`] the field earns: empty, unreadable,
    /// zero, or a clock time out of range.
    pub fn when(&self) -> Result<When, ParseError> {
        parse_when(&self.input)
    }

    /// When the running countdown will end on the wall clock, which the
    /// headline shows beside the remaining time so `at 23:45` answers
    /// "when exactly?" without arithmetic. A paused countdown has no
    /// deadline - it would move with the pause - and an idle one has
    /// nothing to promise.
    pub(crate) fn deadline(&self) -> Option<SystemTime> {
        match self.timer.status() {
            Status::Armed => Some(system_now() + self.timer.remaining()),
            Status::Idle | Status::Paused | Status::Fired => None,
        }
    }

    /// The time left, in words and whole seconds: a countdown that
    /// jitters in microseconds is noise, not information.
    pub fn left(&self) -> String {
        format_duration(Duration::from_secs(self.timer.remaining().as_secs())).to_string()
    }

    /// The countdown's headline, in the words both frontends show: what
    /// is happening, how much is left, and - while a countdown runs -
    /// when exactly it ends. The terminal and the window render this and
    /// reword nothing, so neither can drift into promising something
    /// the other does not.
    pub fn headline(&self) -> String {
        match self.timer.status() {
            Status::Idle => self.selected_action().map_or_else(
                || "this machine offers nothing to arm".to_string(),
                |action| format!("press enter to arm the {} countdown", action.label()),
            ),
            Status::Armed | Status::Paused => {
                let left = self.left();
                if self.timer.status() == Status::Paused {
                    format!("paused, {left} left - press space to resume")
                } else {
                    // An armed countdown always carries its mode; the
                    // fallback is the grammar's own safety net, naming
                    // no mode the machine might not have.
                    let action = self
                        .timer
                        .action()
                        .map_or_else(|| String::from("sleep"), |mode| mode.label().to_lowercase());
                    match self.deadline() {
                        // The wall clock answers the question a countdown
                        // only asks in remainders: when, exactly.
                        Some(at) => format!("{action} in {left}, at {}", format_clock(at)),
                        None => format!("{action} in {left}"),
                    }
                }
            }
            Status::Fired => format!(
                "time! {}...",
                self.fired
                    .map_or_else(|| String::from("sleep the machine"), SleepAction::firing)
            ),
        }
    }

    /// Arms the highlighted mode with the typed duration; refuses - and
    /// lets the screen show why - when either is missing. A finished
    /// countdown's report is cleared with the new arming.
    pub fn arm_selected(&mut self) -> bool {
        match (self.selected_action(), parse_when(&self.input)) {
            (Some(action), Ok(when)) => {
                self.fired = None;
                self.failure = None;
                // A clock time becomes the countdown that reaches it:
                // the timer itself stays monotonic, knowing only how
                // long is left.
                let armed = self.timer.arm(action, when.duration_from(system_now()));
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
    pub fn move_selection(&mut self, step: isize) {
        let count = isize::try_from(self.modes.len()).unwrap_or(0);
        if count == 0 {
            return;
        }
        let next = (isize::try_from(self.selected).unwrap_or(0) + step).rem_euclid(count);
        self.selected = usize::try_from(next).unwrap_or(0);
    }

    /// Runs one command. What a command *does* is decided here, once, so
    /// a key means the same thing in the terminal as in the window; what
    /// a key *is* stays each frontend's own business.
    pub fn apply(&mut self, command: Command) {
        let counting = matches!(self.timer.status(), Status::Armed | Status::Paused);
        match command {
            Command::PreviousMode => self.move_selection(-1),
            Command::NextMode => self.move_selection(1),
            Command::ToggleDryRun => self.toggle_backend(),
            Command::ToggleKeepAwake => self.toggle_keep_screen_on(),
            Command::Arm => {
                // A running countdown is never silently replaced: arming
                // waits for an idle (or finished) timer.
                if !counting {
                    self.arm_selected();
                }
            }
            Command::PauseResume => match self.timer.status() {
                Status::Armed => self.timer.pause(),
                Status::Paused => self.timer.resume(),
                Status::Idle | Status::Fired => {}
            },
            Command::Escape => {
                // Escape first saves the machine from sleep, and only a
                // second press - with nothing left to save - leaves.
                if counting {
                    self.abort();
                } else {
                    self.quit();
                }
            }
            Command::Longer => self.timer.adjust(NUDGE),
            Command::Shorter => self.timer.adjust(-NUDGE),
            Command::Quit => self.quit(),
        }
    }

    /// Gives up on the countdown; the machine stays awake and the report
    /// of a finished one goes with it.
    pub fn abort(&mut self) {
        self.timer.abort();
        self.awake.stop();
        self.fired = None;
        self.failure = None;
    }

    /// Counts the time that passed since the last tick. Returns whether
    /// the countdown just ended, in which case `fired` now names the
    /// action and the frontend should [`Self::perform`] it - after one
    /// last draw saying so, if it can.
    ///
    /// # Panics
    ///
    /// If a fired countdown carries no action, which is a bug in the
    /// timer's state machine rather than a machine answer.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.timer.tick(now) == TimerEvent::Fired {
            // A fired countdown was armed with an action; if that ever
            // stops being true it is a bug in the timer's state machine,
            // and a loud one beats a countdown that quietly sleeps
            // through its own alarm.
            #[allow(clippy::expect_used)]
            {
                self.fired = Some(
                    self.timer
                        .action()
                        .expect("a fired countdown knows what it is doing"),
                );
            }
            return true;
        }
        false
    }

    /// Does the thing the countdown was armed for. On a real sleep this
    /// call does not return until the machine wakes; the dry run answers
    /// at once, and the frontend is there to say so.
    pub fn perform(&mut self) {
        // Whatever happens next, the screen is on its own from here.
        self.awake.stop();
        if let Some(action) = self.fired {
            self.failure = execute(action, self.backend).err();
        }
    }
}

#[cfg(test)]
// A test that cannot get its answer is broken, not a machine that said no.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::input::format_clock;
    use std::time::Duration;

    /// A controller built by hand, so the tests never ask the machine what
    /// it can do (`detect` would spawn processes here).
    fn controller() -> Controller {
        Controller::new(Options {
            backend: Backend::Real,
            keep_screen_on: false,
            when: None,
            mode: None,
            modes: vec![SleepAction::Suspend],
        })
    }

    #[test]
    fn a_named_duration_arms_at_once() {
        let controller = Controller::new(Options {
            backend: Backend::Real,
            keep_screen_on: false,
            when: Some("45s".into()),
            mode: None,
            modes: vec![SleepAction::Suspend],
        });
        assert_eq!(controller.timer.status(), Status::Armed);
        assert_eq!(controller.timer.remaining(), Duration::from_secs(45));
    }

    #[test]
    fn a_named_mode_highlights_itself() {
        let controller = Controller::new(Options {
            backend: Backend::Real,
            keep_screen_on: false,
            when: None,
            mode: Some(SleepAction::Hibernate),
            modes: vec![SleepAction::Suspend, SleepAction::Hibernate],
        });
        assert_eq!(controller.selected_action(), Some(SleepAction::Hibernate));
    }

    #[test]
    fn dry_run_can_be_flipped_while_counting() {
        // The safety valve: mid-countdown, a flip to dry run saves the
        // machine without aborting the timer.
        let mut controller = controller();
        assert!(controller.arm_selected());
        controller.toggle_backend();
        assert_eq!(controller.backend, Backend::DryRun);
        assert_eq!(controller.timer.status(), Status::Armed);
    }

    #[test]
    fn keep_awake_toggles_and_applies_to_a_running_countdown() {
        let mut controller = controller();
        controller.toggle_keep_screen_on();
        assert!(controller.keep_screen_on);
        // Arming with the flag on grabs the inhibition; flipping off
        // mid-count lets it go. Whether a helper exists is the machine's
        // business - the contract is that the flag and the holding agree.
        assert!(controller.arm_selected());
        controller.toggle_keep_screen_on();
        assert!(!controller.keep_screen_on);
    }

    #[test]
    fn aborting_forgets_the_countdown_and_the_report() {
        let mut controller = controller();
        assert!(controller.arm_selected());
        controller.abort();
        assert_eq!(controller.timer.status(), Status::Idle);
        assert!(controller.fired.is_none());
    }

    #[test]
    fn selection_wraps_at_both_ends() {
        let mut controller = Controller::new(Options {
            backend: Backend::Real,
            keep_screen_on: false,
            when: None,
            mode: None,
            modes: vec![SleepAction::Suspend, SleepAction::Hibernate],
        });
        controller.move_selection(-1);
        assert_eq!(controller.selected, 1);
        controller.move_selection(1);
        assert_eq!(controller.selected, 0);
    }

    #[test]
    fn ticking_reports_the_end_once_and_names_the_action() {
        let mut controller = controller();
        assert!(controller.arm_selected());
        controller.timer.adjust(-86400); // clamped to nothing: "now"
        controller.tick(Instant::now()); // the first tick only sets the clock
        assert!(controller.tick(Instant::now()));
        assert_eq!(controller.fired, Some(SleepAction::Suspend));
        // Fired is a one-time event; the frontend performs the action once.
        assert!(!controller.tick(Instant::now() + Duration::from_secs(1)));
    }

    #[test]
    fn escape_saves_the_machine_before_it_leaves() {
        let mut controller = controller();
        controller.apply(Command::Arm);
        assert_eq!(controller.timer.status(), Status::Armed);
        // A running countdown is never silently replaced: a second arm
        // waits, whatever the field says meanwhile.
        let before = controller.timer.remaining();
        controller.input = "1h".into();
        controller.apply(Command::Arm);
        assert_eq!(controller.timer.remaining(), before);
        // Escape first saves the machine; only the second press leaves.
        controller.apply(Command::Escape);
        assert_eq!(controller.timer.status(), Status::Idle);
        assert!(controller.running());
        controller.apply(Command::Escape);
        assert!(!controller.running());
    }

    #[test]
    fn pause_resume_and_nudge_answer_a_countdown() {
        let mut controller = controller();
        controller.apply(Command::Arm);
        controller.apply(Command::PauseResume);
        assert_eq!(controller.timer.status(), Status::Paused);
        controller.apply(Command::PauseResume);
        assert_eq!(controller.timer.status(), Status::Armed);
        let before = controller.timer.remaining();
        // A fresh countdown sits at its total, where `+` can only be
        // refused; a minute taken off can be put back.
        controller.apply(Command::Longer);
        assert_eq!(controller.timer.remaining(), before);
        controller.apply(Command::Shorter);
        assert_eq!(
            controller.timer.remaining(),
            before.checked_sub(Duration::from_secs(60)).unwrap()
        );
        controller.apply(Command::Longer);
        assert_eq!(controller.timer.remaining(), before);
        // Pausing an idle timer is nothing to do.
        controller.abort();
        controller.apply(Command::PauseResume);
        assert_eq!(controller.timer.status(), Status::Idle);
    }

    #[test]
    fn a_clock_time_arms_for_the_next_one_to_come() {
        let mut controller = controller();
        controller.input = "23:45".into();
        assert!(controller.arm_selected());
        // Whatever time this test runs, the deadline is the next 23:45,
        // and never more than a day away.
        let deadline = controller.deadline().unwrap();
        assert_eq!(format_clock(deadline), "23:45");
        let left = deadline.duration_since(SystemTime::now()).unwrap();
        assert!(left <= Duration::from_hours(24));
    }

    #[test]
    fn only_a_running_countdown_has_a_deadline() {
        let mut controller = controller();
        assert_eq!(controller.deadline(), None);
        controller.apply(Command::Arm);
        assert!(controller.deadline().is_some());
        // A paused countdown has no deadline: it would move with the
        // pause, and a promise that drifts is no promise.
        controller.apply(Command::PauseResume);
        assert_eq!(controller.deadline(), None);
    }
}
