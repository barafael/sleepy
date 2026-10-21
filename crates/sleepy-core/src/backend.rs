//! Running the action, or pretending to.
//!
//! The real work is one short command per platform. It is kept apart from
//! the decision to run it so the frontends can show what is coming, and so
//! a `DryRun` can walk the whole path — arm, count down, fire — without
//! ever touching the machine.

use crate::action::SleepAction;
use std::{error, fmt, io, process::Command};

/// Whether to touch the machine or only describe what would happen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    /// Run the platform's sleep command.
    #[default]
    Real,
    /// Do nothing but report the command. Good for trying the timer out.
    DryRun,
}

/// The program and arguments that would perform `action`, if this platform
/// has one.
///
/// Kept apart from running it so the frontends can promise what is coming
/// and the tests can check it without sleeping anyone.
pub fn command_for(action: SleepAction) -> Option<(String, Vec<String>)> {
    if cfg!(target_os = "linux") {
        // systemd takes the action's name as its verb, which is why
        // `SleepAction::verb` is spelled the way it is.
        Some(("systemctl".into(), vec![action.verb().into()]))
    } else if cfg!(target_os = "macos") {
        // `pmset sleepnow` is the only sleep macOS exposes this way.
        (action == SleepAction::Suspend).then(|| ("pmset".into(), vec!["sleepnow".into()]))
    } else if cfg!(target_os = "windows") {
        // SetSuspendState's first argument picks suspend (0) or hibernate
        // (1); the others force the action and forbid wake-up. Note the
        // known quirk: with hibernation enabled, 0 hibernates too, and
        // only `shutdown /h` is unambiguous.
        match action {
            SleepAction::Suspend => Some((
                "rundll32.exe".into(),
                vec!["powrprof.dll,SetSuspendState".into(), "0,1,0".into()],
            )),
            SleepAction::Hibernate => Some((
                "rundll32.exe".into(),
                vec!["powrprof.dll,SetSuspendState".into(), "1,1,0".into()],
            )),
            SleepAction::HybridSleep | SleepAction::SuspendThenHibernate => None,
        }
    } else {
        None
    }
}

/// The command as one line, for showing to the user; `None` if this
/// platform has no command for the action.
pub fn describe(action: SleepAction) -> Option<String> {
    let (program, args) = command_for(action)?;
    Some(
        format!("{program} {}", args.join(" "))
            .trim_end()
            .to_string(),
    )
}

/// Puts the machine to sleep now.
///
/// On success the process is frozen inside this call and wakes up on the
/// other side of the sleep, so callers should treat `Ok` as "leave now":
/// print nothing more, draw nothing more, exit.
pub fn execute(action: SleepAction, backend: Backend) -> Result<(), Error> {
    let Some((program, args)) = command_for(action) else {
        return Err(Error::Unsupported(action));
    };
    match backend {
        Backend::DryRun => Ok(()),
        Backend::Real => Command::new(&program)
            .args(&args)
            .status()
            .map_err(|source| Error::Spawn {
                program: program.clone(),
                source,
            })?
            // systemctl exits non-zero when logind refuses, for example
            // because another session is in the way.
            .success()
            .then_some(())
            .ok_or(Error::Refused { action, program }),
    }
}

/// Why the machine is still awake.
#[derive(Debug)]
pub enum Error {
    /// This platform has no command for this action.
    Unsupported(SleepAction),
    /// The command exists but could not be started.
    Spawn { program: String, source: io::Error },
    /// The command ran and said no.
    Refused {
        action: SleepAction,
        program: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(action) => {
                write!(f, "this machine has no command for {} ", action.label())
            }
            Self::Spawn { program, source } => write!(f, "cannot run {program}: {source}"),
            Self::Refused { action, program } => write!(
                f,
                "{program} refused to {} (does the session allow it?)",
                action.label()
            ),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Spawn { source, .. } => Some(source),
            _ => None,
        }
    }
}

// `io::Error` has no `PartialEq`, so compare the parts that matter: the
// action and the program. Tests compare outcomes, not error text.
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unsupported(a), Self::Unsupported(b)) => a == b,
            (Self::Spawn { program: a, .. }, Self::Spawn { program: b, .. }) => a == b,
            (Self::Refused { action: a, .. }, Self::Refused { action: b, .. }) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_never_touches_the_machine() {
        // Every action this platform knows, run through a dry fire: the
        // result must say nothing happened, and nothing must have happened.
        for action in SleepAction::ALL {
            let result = execute(action, Backend::DryRun);
            match command_for(action) {
                Some(_) => assert_eq!(result, Ok(()), "{action:?} should dry-fire"),
                None => assert!(matches!(result, Err(Error::Unsupported(_)))),
            }
        }
    }

    #[test]
    fn suspend_is_always_a_known_command() {
        // `detect` offers suspend everywhere; `execute` must be able to run
        // what it offered, or the two modules disagree.
        assert!(command_for(SleepAction::Suspend).is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_asks_systemctl_with_the_action_as_verb() {
        for action in SleepAction::ALL {
            let (program, args) = command_for(action).expect("systemd knows all four");
            assert_eq!(program, "systemctl");
            assert_eq!(args, [action.verb()]);
            assert_eq!(
                describe(action).as_deref(),
                Some(format!("systemctl {}", action.verb()).as_str())
            );
        }
    }
}
