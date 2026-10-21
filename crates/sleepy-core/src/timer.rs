//! The countdown: arm it, pause it, abort it, or let it fire.
//!
//! A state machine with no threads and no timers of its own. The frontend
//! calls [`Timer::tick`] whenever it feels like (once a second is plenty)
//! with the current `Instant`, and the timer does the arithmetic. That
//! makes the whole machine testable: the tests hand it invented instants
//! and watch it count down, pause and fire without waiting for anything.

use crate::action::SleepAction;
use std::time::{Duration, Instant};

/// What the timer is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Sleeping peacefully; nothing is armed.
    #[default]
    Idle,
    /// Counting down towards the action.
    Armed,
    /// Counting down is suspended; the remaining time waits for a resume.
    Paused,
    /// The countdown reached zero and the action is being performed.
    Fired,
}

/// What a tick produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Just another tick.
    Nothing,
    /// The countdown ended; perform the action now, once.
    Fired,
}

/// A countdown to a [`SleepAction`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timer {
    /// What will happen when the countdown ends.
    action: Option<SleepAction>,
    /// The duration armed with, kept for the progress bar.
    total: Duration,
    /// Time left until the action.
    remaining: Duration,
    status: Status,
    /// When we last counted from; `None` until the first tick after
    /// arming or resuming, so time spent paused is never counted.
    last: Option<Instant>,
}

impl Timer {
    /// A timer that sleeps peacefully.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts (or restarts) the countdown to `action`.
    ///
    /// Arming from `Armed` or `Paused` replaces the old countdown; arming
    /// with a zero duration is refused, because that would fire before the
    /// user could blink.
    pub fn arm(&mut self, action: SleepAction, duration: Duration) -> bool {
        if duration.is_zero() {
            return false;
        }
        self.action = Some(action);
        self.total = duration;
        self.remaining = duration;
        self.status = Status::Armed;
        self.last = None;
        true
    }

    /// Stops counting; the remaining time is kept for [`Self::resume`].
    pub fn pause(&mut self) {
        if self.status == Status::Armed {
            self.status = Status::Paused;
            self.last = None;
        }
    }

    /// Counts again, from where pausing left off.
    pub fn resume(&mut self) {
        if self.status == Status::Paused {
            self.status = Status::Armed;
            self.last = None;
        }
    }

    /// Gives up: back to idle, nothing left over.
    pub fn abort(&mut self) {
        *self = Self::new();
    }

    /// Moves the deadline by `delta` (negative to come closer).
    ///
    /// The remaining time never leaves `0..=total`... except that a long
    /// enough `delta` may push it past zero, where the next tick fires:
    /// shortening the countdown to nothing is a way of saying "now".
    /// Idle and fired timers have no deadline to move.
    pub fn adjust(&mut self, delta: i64) {
        if !matches!(self.status, Status::Armed | Status::Paused) {
            return;
        }
        let seconds = self.remaining.as_secs_f64() + delta as f64;
        self.remaining = Duration::from_secs_f64(seconds.max(0.0));
    }

    /// Counts the time that passed since the last tick.
    ///
    /// Pass the current `Instant`; instants from the past or from before
    /// the last tick are ignored, so a frontend that ticks eagerly cannot
    /// make time run backwards.
    pub fn tick(&mut self, now: Instant) -> Event {
        if self.status != Status::Armed {
            return Event::Nothing;
        }
        let Some(last) = self.last else {
            self.last = Some(now);
            return Event::Nothing;
        };
        // A `now` from before the last tick (a resumed machine whose clock
        // jumped?) must not add time back.
        if let Some(passed) = now.checked_duration_since(last) {
            self.remaining = self.remaining.saturating_sub(passed);
        }
        self.last = Some(now);
        if self.remaining.is_zero() {
            self.status = Status::Fired;
            return Event::Fired;
        }
        Event::Nothing
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// The action being counted down to, if any.
    pub fn action(&self) -> Option<SleepAction> {
        self.action
    }

    /// Time left until the action.
    pub fn remaining(&self) -> Duration {
        self.remaining
    }

    /// The duration armed with.
    pub fn total(&self) -> Duration {
        self.total
    }

    /// How much of the countdown has passed, for a progress bar.
    pub fn progress(&self) -> f32 {
        if self.total.is_zero() {
            return 0.0;
        }
        let passed = self.total.saturating_sub(self.remaining);
        (passed.as_secs_f64() / self.total.as_secs_f64()) as f32
    }
}

/// A pretend timeline: instants measured from one fixed point, so tests
/// can hand the timer a believable past without waiting for a real one.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct Timeline(Instant);

#[cfg(test)]
impl Default for Timeline {
    fn default() -> Self {
        Self(Instant::now())
    }
}

#[cfg(test)]
impl Timeline {
    fn at(&self, seconds: f64) -> Instant {
        self.0 + Duration::from_secs_f64(seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SleepAction::Suspend;

    fn armed() -> Timer {
        let mut timer = Timer::new();
        assert!(timer.arm(Suspend, Duration::from_secs(60)));
        timer
    }

    #[test]
    fn an_idle_timer_ticks_quietly() {
        let clock = Timeline::default();
        let mut timer = Timer::new();
        assert_eq!(timer.status(), Status::Idle);
        assert_eq!(timer.tick(clock.at(0.0)), Event::Nothing);
        assert_eq!(timer.tick(clock.at(100.0)), Event::Nothing);
        assert_eq!(timer.remaining(), Duration::ZERO);
    }

    #[test]
    fn arming_starts_the_countdown() {
        let timer = armed();
        assert_eq!(timer.status(), Status::Armed);
        assert_eq!(timer.action(), Some(Suspend));
        assert_eq!(timer.remaining(), Duration::from_secs(60));
        assert_eq!(timer.total(), Duration::from_secs(60));
    }

    #[test]
    fn arming_with_no_time_is_refused() {
        let mut timer = Timer::new();
        assert!(!timer.arm(Suspend, Duration::ZERO));
        assert_eq!(timer.status(), Status::Idle);
    }

    #[test]
    fn it_counts_down_and_fires_once() {
        let clock = Timeline::default();
        let mut timer = armed();
        assert_eq!(timer.tick(clock.at(0.0)), Event::Nothing); // sets the clock
        assert_eq!(timer.tick(clock.at(10.0)), Event::Nothing);
        assert_eq!(timer.remaining(), Duration::from_secs(50));
        assert_eq!(timer.tick(clock.at(60.0)), Event::Fired);
        assert_eq!(timer.status(), Status::Fired);
        assert_eq!(timer.remaining(), Duration::ZERO);
        // Fired is a one-time event; the frontend performs the action once.
        assert_eq!(timer.tick(clock.at(61.0)), Event::Nothing);
    }

    #[test]
    fn pausing_freezes_the_countdown_and_resuming_continues_it() {
        let clock = Timeline::default();
        let mut timer = armed();
        timer.tick(clock.at(0.0));
        timer.tick(clock.at(40.0));
        timer.pause();
        assert_eq!(timer.status(), Status::Paused);
        // A whole hour passes while paused; none of it counts.
        assert_eq!(timer.tick(clock.at(3600.0)), Event::Nothing);
        assert_eq!(timer.remaining(), Duration::from_secs(20));
        timer.resume();
        // The first tick after resuming only sets the clock again.
        assert_eq!(timer.tick(clock.at(3605.0)), Event::Nothing);
        assert_eq!(timer.remaining(), Duration::from_secs(20));
        assert_eq!(timer.tick(clock.at(3620.0)), Event::Nothing);
        assert_eq!(timer.remaining(), Duration::from_secs(5));
        assert_eq!(timer.tick(clock.at(3625.0)), Event::Fired);
    }

    #[test]
    fn pause_and_resume_only_move_the_right_states() {
        let mut timer = Timer::new();
        timer.resume(); // idle stays idle
        assert_eq!(timer.status(), Status::Idle);
        timer.pause();
        assert_eq!(timer.status(), Status::Idle);
    }

    #[test]
    fn aborting_forgets_everything() {
        let clock = Timeline::default();
        let mut timer = armed();
        timer.tick(clock.at(0.0));
        timer.tick(clock.at(10.0));
        timer.abort();
        assert_eq!(timer, Timer::new());
    }

    #[test]
    fn time_never_runs_backwards() {
        let clock = Timeline::default();
        let mut timer = armed();
        timer.tick(clock.at(10.0)); // sets the clock
        timer.tick(clock.at(12.0));
        assert_eq!(timer.remaining(), Duration::from_secs(58));
        timer.tick(clock.at(5.0)); // a clock that jumped back
        assert_eq!(timer.remaining(), Duration::from_secs(58));
    }

    #[test]
    fn adjusting_moves_the_deadline() {
        let clock = Timeline::default();
        let mut timer = armed();
        timer.tick(clock.at(0.0));
        timer.tick(clock.at(10.0)); // 50 left
        timer.adjust(60);
        assert_eq!(timer.remaining(), Duration::from_secs(110));
        timer.adjust(-170);
        assert_eq!(timer.remaining(), Duration::ZERO);
        // A countdown shortened to nothing fires on the next tick.
        assert_eq!(timer.tick(clock.at(11.0)), Event::Fired);
    }

    #[test]
    fn adjusting_an_idle_timer_does_nothing() {
        let mut timer = Timer::new();
        timer.adjust(600);
        assert_eq!(timer.status(), Status::Idle);
        assert_eq!(timer.remaining(), Duration::ZERO);
    }

    #[test]
    fn progress_goes_from_zero_to_one() {
        let clock = Timeline::default();
        let mut timer = armed();
        assert_eq!(timer.progress(), 0.0);
        timer.tick(clock.at(0.0));
        timer.tick(clock.at(15.0));
        assert!((timer.progress() - 0.25).abs() < f32::EPSILON);
        timer.tick(clock.at(60.0));
        assert_eq!(timer.progress(), 1.0);
    }
}
