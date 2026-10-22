//! Turning what the user typed into a *when*.
//!
//! Two grammars share the field and the command line: [humantime]'s
//! durations - `45s`, `30 min`, `1h30m` - and the wall clock - `23:45`,
//! `6:5:30` - which means the next time that clock time comes around.
//! They cannot collide: humantime never reads a colon. The extra rules
//! are sleepy's own: an empty field is not a when, and zero is not
//! either - a timer that fires before the user can lift their hands off
//! the keyboard is a prank, not a feature.
//!
//! [humantime]: https://docs.rs/humantime

use chrono::{DateTime, Local, MappedLocalTime, NaiveTime, TimeDelta};
use std::time::{Duration, SystemTime};

/// When the machine should sleep: after a while, or when the clock says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// This much time from now.
    After(Duration),
    /// The next time the wall clock reads this.
    At(NaiveTime),
}

impl When {
    /// How long from `now` until this comes around. A clock time means
    /// the next one to come: `23:45` typed at ten is tonight, typed at
    /// five to midnight tomorrow night.
    pub fn duration_from(&self, now: SystemTime) -> Duration {
        match self {
            When::After(duration) => *duration,
            When::At(time) => {
                let now = DateTime::<Local>::from(now);
                let delta = next_occurrence(*time, now) - now;
                // Whole seconds, rounded up: a countdown that sleeps at
                // 23:44:59.5 has fallen asleep before its time.
                match delta.num_nanoseconds() {
                    // The delta is positive and under two days; only a
                    // broken clock could make it anything else.
                    Some(ns) if ns > 0 => Duration::from_secs((ns as u64).div_ceil(1_000_000_000)),
                    _ => Duration::ZERO,
                }
            }
        }
    }
}

/// Parses a when from an input field: the clock's answer if the text
/// wears the clock's shape, humantime's otherwise.
pub fn parse_when(input: &str) -> Result<When, ParseError> {
    let text = input.trim();
    if text.contains(':') {
        return match parse_clock(text) {
            Some(time) => Ok(When::At(time)),
            None => Err(ParseError::Clock),
        };
    }
    parse_duration(input).map(When::After)
}

/// Parses a duration from an input field.
pub fn parse_duration(input: &str) -> Result<Duration, ParseError> {
    let text = input.trim();
    if text.is_empty() {
        return Err(ParseError::Empty);
    }
    match humantime::parse_duration(text) {
        Ok(duration) if duration.is_zero() => Err(ParseError::Zero),
        Ok(duration) => Ok(duration),
        Err(_) => Err(ParseError::Invalid),
    }
}

/// The clock's own grammar: `HH:MM`, or `HH:MM:SS` - one or two digits
/// per part, so `9:05` is as honest as `09:05`. Anything outside the
/// day's range is not a clock time, and `None` says so.
fn parse_clock(text: &str) -> Option<NaiveTime> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let number = |part: &str| -> Option<u32> {
        if part.is_empty() || part.len() > 2 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let hour = number(parts[0])?;
    let minute = number(parts[1])?;
    let second = match parts.get(2) {
        Some(part) => number(part)?,
        None => 0,
    };
    NaiveTime::from_hms_opt(hour, minute, second)
}

/// The next instant strictly after `now` at which the wall clock reads
/// `target`.
fn next_occurrence(target: NaiveTime, now: DateTime<Local>) -> DateTime<Local> {
    // Today if it has not gone by, tomorrow if it has; the third day
    // is for the spring-forward night that skips the time itself.
    for offset in 0..=2 {
        let day = now.date_naive() + TimeDelta::days(offset);
        let on_the_clock = match day.and_time(target).and_local_timezone(Local) {
            MappedLocalTime::Single(at) => Some(at),
            // Autumn turns one clock time into two: sleep at the first
            // passing, or at the second if the first has gone by.
            MappedLocalTime::Ambiguous(earliest, latest) => {
                Some(if earliest > now { earliest } else { latest })
            }
            // Spring takes a clock time off the day entirely.
            MappedLocalTime::None => None,
        };
        if let Some(at) = on_the_clock.filter(|at| *at > now) {
            return at;
        }
    }
    // Unreachable: tomorrow always offers the time, and no three days
    // in a row skip it. Should a clock ever disagree, sleep now.
    now
}

/// The wall clock's own answer for an instant - `23:45` - which is what
/// the frontends show beside a running countdown.
pub fn format_clock(at: SystemTime) -> String {
    DateTime::<Local>::from(at).format("%H:%M").to_string()
}

/// Why the input is not a when yet.
///
/// The message is the whole story: the frontends show it under the input
/// field and the command line repeats it, so `thiserror` writes it once
/// and `Display` is the hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Nothing was typed.
    #[error("type a time, e.g. 30 min or 23:45")]
    Empty,
    /// Something was typed, but humantime does not know it.
    #[error("not a duration; try 45s, 30 min, 1h30m")]
    Invalid,
    /// A duration of zero; the machine would sleep immediately.
    #[error("that would sleep immediately; type more than 0")]
    Zero,
    /// The clock's shape, but not the clock's range: 25:00, 12:60.
    #[error("not a clock time; try 23:45, from 0:00 to 23:59:59")]
    Clock,
}

#[cfg(test)]
// A test that cannot get its answer is broken, not a machine that said no.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn secs(input: &str) -> u64 {
        parse_duration(input).unwrap().as_secs()
    }

    fn clock(hour: u32, minute: u32, second: u32) -> When {
        When::At(NaiveTime::from_hms_opt(hour, minute, second).unwrap())
    }

    #[test]
    fn humantime_grammar_is_accepted() {
        assert_eq!(secs("45s"), 45);
        assert_eq!(secs("30 min"), 1800);
        assert_eq!(secs("1h30m"), 5400);
        assert_eq!(secs("1.5h"), 5400);
        assert_eq!(secs("  2 hours "), 7200);
    }

    #[test]
    fn the_clock_grammar_is_accepted() {
        assert_eq!(parse_when("23:45"), Ok(clock(23, 45, 0)));
        assert_eq!(parse_when("06:05:30"), Ok(clock(6, 5, 30)));
        // One digit is as honest as two.
        assert_eq!(parse_when("9:5"), Ok(clock(9, 5, 0)));
        assert_eq!(parse_when("0:00"), Ok(clock(0, 0, 0)));
        // And the durations still answer, through the same door.
        assert_eq!(
            parse_when("1h30m"),
            Ok(When::After(Duration::from_secs(5400)))
        );
    }

    #[test]
    fn the_ways_to_be_wrong_are_told_apart() {
        assert_eq!(parse_when(""), Err(ParseError::Empty));
        assert_eq!(parse_when("   "), Err(ParseError::Empty));
        assert_eq!(parse_when("soon"), Err(ParseError::Invalid));
        assert_eq!(parse_when("0s"), Err(ParseError::Zero));
        // The clock's shape but out of the day's range is told apart
        // from a wrong duration, because it deserves its own correction.
        assert_eq!(parse_when("25:00"), Err(ParseError::Clock));
        assert_eq!(parse_when("12:60"), Err(ParseError::Clock));
        assert_eq!(parse_when("12:"), Err(ParseError::Clock));
        assert_eq!(parse_when("1:2:3:4"), Err(ParseError::Clock));
    }

    /// A `now` built by hand: today at `hour:minute`, in whatever zone
    /// the machine lives. The answers below hold in every zone on every
    /// day, because no zone skips a midday hour.
    fn today_at(hour: u32, minute: u32) -> DateTime<Local> {
        let naive = Local::now()
            .date_naive()
            .and_hms_opt(hour, minute, 0)
            .unwrap();
        naive.and_local_timezone(Local).earliest().unwrap()
    }

    #[test]
    fn a_clock_time_means_the_next_one_to_come() {
        let now = today_at(8, 0);
        // This afternoon has not gone by: today it is.
        let afternoon = next_occurrence(NaiveTime::from_hms_opt(15, 30, 0).unwrap(), now);
        assert_eq!(afternoon.date_naive(), now.date_naive());
        assert_eq!(
            afternoon.time(),
            NaiveTime::from_hms_opt(15, 30, 0).unwrap()
        );
        // This morning has: tomorrow, whatever the zone did to the day.
        let morning = next_occurrence(NaiveTime::from_hms_opt(7, 30, 0).unwrap(), now);
        assert_eq!(morning.date_naive(), now.date_naive() + TimeDelta::days(1));
        assert_eq!(morning.time(), NaiveTime::from_hms_opt(7, 30, 0).unwrap());
        // A time exactly now has gone by: the next one is tomorrow.
        let same = next_occurrence(NaiveTime::from_hms_opt(8, 0, 0).unwrap(), now);
        assert_eq!(same.date_naive(), now.date_naive() + TimeDelta::days(1));
        assert_eq!(same.time(), NaiveTime::from_hms_opt(8, 0, 0).unwrap());
    }

    #[test]
    fn a_clock_time_never_sleeps_before_its_time() {
        // Rounded up to the second, so the wall clock really reads the
        // target when the countdown ends - never a second before.
        let now = SystemTime::now();
        let target = Local::now() + TimeDelta::hours(3);
        let time = target
            .time()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();
        let duration = When::At(time).duration_from(now);
        let at = DateTime::<Local>::from(now + duration);
        assert_eq!(
            at.time().format("%H:%M").to_string(),
            time.format("%H:%M").to_string()
        );
    }
}
