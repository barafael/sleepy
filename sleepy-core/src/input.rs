//! Turning what the user typed into a *when*.
//!
//! Two grammars share the field and the command line: [humantime]'s
//! durations - `45s`, `30 min`, `1h30m` - and the wall clock - `23:45`,
//! `6:5:30` - which means the next time that clock time comes around.
//! They cannot collide: humantime never reads a colon. The extra rules
//! are sleepy's own: an empty field is not a when, and zero is not
//! either - a timer that fires before the user can lift their hands off
//! the keyboard is a prank, not a feature - and the field only ever
//! holds what these grammars could still become, which
//! [`is_when_prefix`] is what the frontends ask.
//!
//! [humantime]: https://docs.rs/humantime

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

/// Whether `typed` could still become a when: a valid when is its own
/// prefix, and so is anything a few more characters could complete -
/// `23:`, `30 mi`, `1h3`.
///
/// The frontends ask before letting a character
/// into the field, so the field holds nothing no when could wear; the
/// keys that fall out - `k` among them - are commands at any moment.
pub fn is_when_prefix(typed: &str) -> bool {
    if typed.contains(':') {
        is_clock_prefix(typed)
    } else {
        is_duration_prefix(typed)
    }
}

/// The clock's shape, unfinished: one or two digits per part, at most
/// three parts, every finished part inside its range. `23:` may still
/// become `23:45`; `25:` becomes nothing, and neither does the empty
/// hour of `:30`.
fn is_clock_prefix(typed: &str) -> bool {
    let parts: Vec<&str> = typed.split(':').collect();
    if parts.len() > 3 {
        return false;
    }
    let fits = |part: &str, limit: u32| {
        part.len() <= 2
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && part.parse::<u32>().is_ok_and(|value| value <= limit)
    };
    parts.iter().enumerate().all(|(index, part)| match index {
        // The hour must be there to begin with, and must already
        // fit the day: no clock time starts `25`.
        0 => !part.is_empty() && fits(part, 23),
        // The minute and second may still be unwritten - a trailing
        // colon is a part on its way - but only the last part may be
        // empty, and one digit is as honest as two.
        _ => (part.is_empty() && index == parts.len() - 1) || fits(part, 59),
    })
}

/// humantime's unit words, exactly as humantime matches them - case
/// included: `M` is months, `m` is minutes, and `H` is nothing at all.
const UNIT_WORDS: &[&str] = &[
    "nanos", "nsec", "ns", //
    "usec", "us", "µs", //
    "millis", "msec", "ms", //
    "seconds", "second", "secs", "sec", "s", //
    "minutes", "minute", "min", "mins", "m", //
    "hours", "hour", "hr", "hrs", "h", //
    "days", "day", "d", //
    "weeks", "week", "wk", "wks", "w", //
    "months", "month", "M", //
    "years", "year", "yr", "yrs", "y", //
];

/// humantime's grammar, unfinished: number-unit pairs, and a tail that
/// may stop anywhere inside the next pair - mid-number, mid-word, or
/// after the space that has not reached a unit yet. It mirrors
/// humantime's own parser, quirks included: whitespace is skipped
/// inside numbers, so `3 0 m` really is thirty minutes.
fn is_duration_prefix(typed: &str) -> bool {
    // Where humantime's parser would be, having read the text so far.
    enum At {
        /// Before the first digit; whitespace is free.
        Start,
        /// Inside a number, or just past one, waiting for its unit.
        Number,
        /// Just past the point, before any fraction digit.
        Point,
        /// Inside the fraction's digits.
        Fraction,
        /// Inside a unit word, held in `unit`.
        Unit,
        /// Past a finished unit and the whitespace after it.
        Between,
    }
    // humantime's own letter class.
    let letter = |c: char| c.is_ascii_alphabetic() || c == 'µ';
    // A word no unit starts with completes to nothing.
    let starts_a_unit = |unit: &str| UNIT_WORDS.iter().any(|word| word.starts_with(unit));

    let mut at = At::Start;
    let mut unit = String::new();
    for c in typed.chars() {
        at = match (at, c) {
            (At::Start, c) if c.is_whitespace() => At::Start,
            (At::Start, c) if c.is_ascii_digit() => At::Number,
            (At::Number, c) if c.is_ascii_digit() || c.is_whitespace() => At::Number,
            (At::Number, '.') => At::Point,
            (At::Number | At::Fraction, c) if letter(c) => {
                unit.clear();
                unit.push(c);
                if !starts_a_unit(&unit) {
                    return false;
                }
                At::Unit
            }
            (At::Point, c) if c.is_whitespace() => At::Point,
            (At::Point, c) if c.is_ascii_digit() => At::Fraction,
            (At::Fraction, c) if c.is_ascii_digit() || c.is_whitespace() => At::Fraction,
            (At::Unit, c) if letter(c) => {
                unit.push(c);
                if !starts_a_unit(&unit) {
                    return false;
                }
                At::Unit
            }
            // Whitespace or a digit seals the word: it must be a whole
            // unit now, as humantime would insist.
            (At::Unit, c) if c.is_whitespace() || c.is_ascii_digit() => {
                if !UNIT_WORDS.contains(&unit.as_str()) {
                    return false;
                }
                unit.clear();
                if c.is_whitespace() {
                    At::Between
                } else {
                    At::Number
                }
            }
            (At::Between, c) if c.is_whitespace() => At::Between,
            (At::Between, c) if c.is_ascii_digit() => At::Number,
            _ => return false,
        };
    }
    // Whatever survives to the end can still be completed: a half-typed
    // word was kept a prefix of a real one, and every other state has a
    // character waiting that a when could wear.
    true
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
    Timestamp::try_from(at).map_or_else(|_| String::new(), |at| {
        at.to_zoned(TimeZone::system())
            .strftime("%H:%M")
            .to_string()
    })
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

    #[test]
    fn a_field_holds_what_a_when_could_still_become() {
        // The clock, half-written.
        for typed in [
            "2", "23", "23:", "23:4", "23:45", "23:45:", "23:45:3", "9:05", "0:00:00",
        ] {
            assert!(is_when_prefix(typed), "{typed:?}");
        }
        // The duration, half-written - humantime's grammar, quirks
        // included: whitespace inside numbers, a word mid-flight.
        for typed in [
            "1", "1h", "1h3", "1h30m", "30 ", "30 m", "30 mi", "1.", "1.5", "1.5h", "2 w", "2 wee",
            "5µ", "1h 30", "3 0 m",
            // `0s` parses as zero, but `0s5m` is a real duration.
            "0s",
        ] {
            assert!(is_when_prefix(typed), "{typed:?}");
        }
        // Empty is the emptiest prefix of all.
        assert!(is_when_prefix(""));
    }

    #[test]
    fn a_field_refuses_what_no_when_could_wear() {
        for typed in [
            "k",
            "z",
            "soon",
            "2x",
            "25:",
            "24:00",
            "12:60",
            "12:600",
            ":30",
            "1::",
            "1:2:3:4",
            // The space seals `mi`: no unit word it is.
            "30 mi ",
            "1.h",
            ".5",
            "1h x",
            "23:45:30:12",
        ] {
            assert!(!is_when_prefix(typed), "{typed:?}");
        }
        // `k` is a humantime letter after all - `2 week` is a real
        // prefix - but the frontends keep the key for the keep-awake
        // toggle, so it never reaches a field by typing.
        assert!(is_when_prefix("2 week"));
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
