//! The command line both frontends answer to.
//!
//! Two flags and two positionals do not earn a dependency, and `--help`
//! should read like sleepy rather than like a generator - so the parser
//! lives here once, and each frontend brings only its own help text. The
//! answers are checked before any screen is taken over, where errors go
//! unread and unheard, and the machine's offerings are detected once here
//! and carried to the app, which would otherwise detect them again.

use crate::action::SleepAction;
use crate::backend::Backend;
use crate::input::parse_duration;

/// What the command line decided before a single key was pressed.
#[derive(Debug)]
pub struct Options {
    /// Whether to really sleep or only say so.
    pub backend: Backend,
    /// Whether to hold the screen awake while the countdown runs.
    pub keep_screen_on: bool,
    /// The duration as typed, if one was; naming it arms the countdown.
    pub duration: Option<String>,
    /// The mode asked for, if any.
    pub mode: Option<SleepAction>,
    /// What this machine offers, detected once here for the app to keep.
    pub modes: Vec<SleepAction>,
}

impl Options {
    /// Parses the command line; `Ok(None)` means the help was printed.
    pub fn parse(args: impl Iterator<Item = String>, help: &str) -> Result<Option<Self>, String> {
        Self::parse_with(args, help, crate::detect)
    }

    /// A hand-written parser, with `detect` a parameter so the tests never
    /// ask the machine what it can do.
    pub fn parse_with(
        args: impl Iterator<Item = String>,
        help: &str,
        detected: impl FnOnce() -> Vec<SleepAction>,
    ) -> Result<Option<Self>, String> {
        let mut backend = Backend::Real;
        let mut keep_screen_on = false;
        let mut duration = None;
        let mut mode = None;
        let mut positionals = 0;
        for arg in args.skip(1) {
            match arg.as_str() {
                "-h" | "--help" => {
                    println!("{help}");
                    return Ok(None);
                }
                "--dry-run" => backend = Backend::DryRun,
                "--keep-screen-on" => keep_screen_on = true,
                _ if arg.starts_with('-') => {
                    return Err(format!("unknown option {arg} (try --help)"));
                }
                _ if positionals == 0 => {
                    duration = Some(arg);
                    positionals += 1;
                }
                _ if positionals == 1 => {
                    mode = Some(
                        arg.parse()
                            .map_err(|_| format!("unknown mode {arg} (try --help)"))?,
                    );
                    positionals += 1;
                }
                _ => return Err(format!("unexpected argument {arg} (try --help)")),
            }
        }
        // Check the answers before the screen opens, where errors go
        // unread and unheard.
        if let Some(text) = &duration {
            parse_duration(text).map_err(|error| format!("bad duration {text}: {error}"))?;
        }
        let modes = detected();
        if let Some(want) = mode
            && !modes.contains(&want)
        {
            let names: Vec<String> = modes.iter().map(ToString::to_string).collect();
            return Err(format!(
                "this machine does not offer {want} (it offers: {})",
                names.join(", ")
            ));
        }
        Ok(Some(Self {
            backend,
            keep_screen_on,
            duration,
            mode,
            modes,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine that offers suspend and nothing else.
    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        Options::parse_with(args.iter().map(|arg| arg.to_string()), "help", || {
            vec![SleepAction::Suspend]
        })
    }

    #[test]
    fn positionals_flags_and_defaults() {
        let options = parse(&["sleepy", "45s", "--dry-run", "--keep-screen-on"])
            .unwrap()
            .unwrap();
        assert_eq!(options.duration.as_deref(), Some("45s"));
        assert_eq!(options.backend, Backend::DryRun);
        assert!(options.keep_screen_on);
        let options = parse(&["sleepy"]).unwrap().unwrap();
        assert!(options.duration.is_none());
        assert_eq!(options.backend, Backend::Real);
        assert!(!options.keep_screen_on);
        // The offerings are detected once and carried along.
        assert_eq!(options.modes, [SleepAction::Suspend]);
    }

    #[test]
    fn the_mode_is_checked_against_the_offerings() {
        assert!(parse(&["sleepy", "45s", "suspend"]).unwrap().is_some());
        let error = parse(&["sleepy", "45s", "hibernate"]).unwrap_err();
        assert!(error.contains("does not offer hibernate"), "{error}");
    }

    #[test]
    fn bad_input_is_refused_before_any_screen() {
        assert!(
            parse(&["sleepy", "soon"])
                .unwrap_err()
                .contains("bad duration")
        );
        assert!(
            parse(&["sleepy", "45s", "nap"])
                .unwrap_err()
                .contains("unknown mode")
        );
        assert!(
            parse(&["sleepy", "--nope"])
                .unwrap_err()
                .contains("unknown option")
        );
        assert!(
            parse(&["sleepy", "1h", "suspend", "extra"])
                .unwrap_err()
                .contains("unexpected")
        );
    }
}
