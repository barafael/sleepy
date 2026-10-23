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
//! The field itself types whatever it is given; [`parse_when`] is the
//! one that judges it, and its [`ParseError`] - shown under the field -
//! is the refusal. There is no second grammar here that tries to guess,
//! mid-typing, what humantime would say.
//!
//! [humantime]: https://docs.rs/humantime

use humantime::format_duration;
use jiff::{
    Timestamp, ToSpan, Zoned,
    civil::{DateTime, Time},
    tz::{AmbiguousOffset, TimeZone},
};
use std::time::{Duration, SystemTime};

/// When the machine should sleep: after a while, or when the clock says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// This much time from now.
    After(Duration),
    /// The next time the wall clock reads this.
    At(Time),
}

impl When {
    /// How long from `now` until this comes around. A clock time means
    /// the next one to come: `23:45` typed at ten is tonight, typed at
    /// five to midnight tomorrow night.
    pub fn duration_from(&self, now: SystemTime) -> Duration {
        match self {
            When::After(duration) => *duration,
            When::At(time) => {
                // Only a clock outside the years jiff can name falls
                // short here; a machine that cannot read its own clock
                // has no time left to wait, which is what a deadline
                // that has gone by answers anyway.
                let Ok(now) = Timestamp::try_from(now) else {
                    return Duration::ZERO;
                };
                let now = now.to_zoned(TimeZone::system());
                let delta = now.duration_until(&next_occurrence(*time, &now));
                // Whole seconds, rounded up: a countdown that sleeps at
                // 23:44:59.5 has fallen asleep before its time.
                match delta.as_nanos() {
                    // The delta is positive and under two days; only a
                    // broken clock could make it anything else.
                    ns if ns > 0 => Duration::from_secs((ns as u64).div_ceil(1_000_000_000)),
                    _ => Duration::ZERO,
                }
            }
        }
    }

    /// What the field echoes beside itself: the promise this when would
    /// make if armed now, in the grammar it was typed in - a duration
    /// counts off a stretch of time, a clock time names the moment it
    /// walks up to. Both frontends show this and only this.
    pub fn echo(&self) -> String {
        match self {
            When::After(duration) => format!("-> in {}", format_duration(*duration)),
            When::At(time) => format!("-> at {}", time.strftime("%H:%M")),
        }
    }
}

/// Parses a when from an input field: the clock's answer if the text
/// wears the clock's shape, humantime's otherwise.
///
/// # Errors
///
/// [`ParseError::Clock`] when the text wears the clock's shape but not
/// its range, and whatever [`parse_duration`] says about the rest.
pub fn parse_when(input: &str) -> Result<When, ParseError> {
    let text = input.trim();
    if text.contains(':') {
        return parse_clock(text).map(When::At).ok_or(ParseError::Clock);
    }
    parse_duration(input).map(When::After)
}

/// Parses a duration from an input field.
///
/// # Errors
///
/// [`ParseError::Empty`] when nothing was typed, [`ParseError::Zero`]
/// when the answer is zero, and [`ParseError::Invalid`] when humantime
/// does not know the text.
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
fn parse_clock(text: &str) -> Option<Time> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let number = |part: &str| -> Option<i8> {
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
    Time::new(hour, minute, second, 0).ok()
}

/// The next instant strictly after `now` at which the wall clock reads
/// `target`.
fn next_occurrence(target: Time, now: &Zoned) -> Zoned {
    let zone = now.time_zone();
    // Today if it has not gone by, tomorrow if it has; the third day
    // is for the spring-forward night that skips the time itself.
    for offset in 0..=2 {
        let Ok(day) = now.date().checked_add(offset.days()) else {
            continue;
        };
        let ambiguous = zone.to_ambiguous_zoned(DateTime::from_parts(day, target));
        let on_the_clock = match ambiguous.offset() {
            AmbiguousOffset::Unambiguous { .. } => ambiguous.earlier().ok(),
            // Autumn turns one clock time into two: sleep at the first
            // passing, or at the second if the first has gone by.
            AmbiguousOffset::Fold { .. } => {
                match ambiguous
                    .clone()
                    .earlier()
                    .ok()
                    .filter(|first| *first > *now)
                {
                    Some(first) => Some(first),
                    None => ambiguous.later().ok(),
                }
            }
            // Spring takes a clock time off the day entirely.
            AmbiguousOffset::Gap { .. } => None,
        };
        if let Some(at) = on_the_clock.filter(|at| *at > *now) {
            return at;
        }
    }
    // Unreachable: tomorrow always offers the time, and no three days
    // in a row skip it. Should a clock ever disagree, sleep now.
    now.clone()
}

/// The wall clock's own answer for an instant - `23:45` - which is what
/// the frontends show beside a running countdown.
pub fn format_clock(at: SystemTime) -> String {
    // Only a clock outside the years jiff can name falls short here;
    // an instant it cannot place has no wall clock to read.
    Timestamp::try_from(at).map_or_else(
        |_| String::new(),
        |at| {
            at.to_zoned(TimeZone::system())
                .strftime("%H:%M")
                .to_string()
        },
    )
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
    use jiff::civil;

    fn secs(input: &str) -> u64 {
        parse_duration(input).unwrap().as_secs()
    }

    fn clock(hour: i8, minute: i8, second: i8) -> When {
        When::At(civil::time(hour, minute, second, 0))
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
            Ok(When::After(Duration::from_mins(90)))
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
    fn today_at(hour: i8, minute: i8) -> Zoned {
        let today = Zoned::now().date();
        // No zone skips a midday hour, so the instant is unambiguous
        // and `earlier` is it.
        TimeZone::system()
            .to_ambiguous_zoned(today.at(hour, minute, 0, 0))
            .earlier()
            .unwrap()
    }

    #[test]
    fn a_clock_time_means_the_next_one_to_come() {
        let now = today_at(8, 0);
        // This afternoon has not gone by: today it is.
        let afternoon = next_occurrence(civil::time(15, 30, 0, 0), &now);
        assert_eq!(afternoon.date(), now.date());
        assert_eq!(afternoon.time(), civil::time(15, 30, 0, 0));
        // This morning has: tomorrow, whatever the zone did to the day.
        let morning = next_occurrence(civil::time(7, 30, 0, 0), &now);
        assert_eq!(morning.date(), now.date().checked_add(1.days()).unwrap());
        assert_eq!(morning.time(), civil::time(7, 30, 0, 0));
        // A time exactly now has gone by: the next one is tomorrow.
        let same = next_occurrence(civil::time(8, 0, 0, 0), &now);
        assert_eq!(same.date(), now.date().checked_add(1.days()).unwrap());
        assert_eq!(same.time(), civil::time(8, 0, 0, 0));
    }

    #[test]
    fn a_clock_time_never_sleeps_before_its_time() {
        // Rounded up to the second, so the wall clock really reads the
        // target when the countdown ends - never a second before.
        let now = SystemTime::now();
        let target = Zoned::now().checked_add(3.hours()).unwrap();
        let time = civil::time(target.time().hour(), target.time().minute(), 0, 0);
        let duration = When::At(time).duration_from(now);
        let at = Timestamp::try_from(now + duration)
            .unwrap()
            .to_zoned(TimeZone::system());
        assert_eq!(
            at.strftime("%H:%M").to_string(),
            time.strftime("%H:%M").to_string()
        );
    }
}
