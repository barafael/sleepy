//! Turning what the user typed into a duration.
//!
//! The format is [humantime]'s: `45s`, `30 min`, `1h30m`, `1.5h`, and the
//! rest of its grammar. The extra rules are sleepy's own: an empty field
//! is not a duration, and zero is not either — a timer that fires before
//! the user can lift their hands off the keyboard is a prank, not a
//! feature.
//!
//! [humantime]: https://docs.rs/humantime

use std::{error, fmt, time::Duration};

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

/// Why the input is not a duration yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Nothing was typed.
    Empty,
    /// Something was typed, but humantime does not know it.
    Invalid,
    /// A duration of zero; the machine would sleep immediately.
    Zero,
}

impl ParseError {
    /// The hint to show under the input field.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Empty => "type a duration, e.g. 30 min",
            Self::Invalid => "not a duration; try 45s, 30 min, 1h30m",
            Self::Zero => "that would sleep immediately; type more than 0",
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.hint())
    }
}

impl error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(input: &str) -> u64 {
        parse_duration(input).unwrap().as_secs()
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
    fn the_three_ways_to_be_wrong_are_told_apart() {
        assert_eq!(parse_duration(""), Err(ParseError::Empty));
        assert_eq!(parse_duration("   "), Err(ParseError::Empty));
        assert_eq!(parse_duration("soon"), Err(ParseError::Invalid));
        assert_eq!(parse_duration("0s"), Err(ParseError::Zero));
    }
}
