//! The ways a machine can sleep, and what this one can actually do.

use std::process::Command;

/// A way to put the machine to sleep, listed from quickest to enter to most
/// drastic. Which of these exist is up to the firmware and the operating
/// system, so [`detect`] asks instead of assuming.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SleepAction {
    /// Suspend to RAM: fast to enter and to leave, but the machine must keep
    /// battery power while it sleeps.
    Suspend,
    /// Suspend to disk: writes memory to swap and powers off completely.
    /// Slowest to leave, and survives any length of power loss.
    Hibernate,
    /// Hibernate and suspend at once: memory goes to both disk and RAM, so
    /// wake-up is fast while the battery holds and nothing is lost if it does
    /// not.
    HybridSleep,
    /// Suspend now and hibernate after a delay: fast wake-up for short
    /// naps, and the battery is safe for long ones.
    SuspendThenHibernate,
}

impl SleepAction {
    /// Every action, in display order.
    pub const ALL: [Self; 4] = [
        Self::Suspend,
        Self::Hibernate,
        Self::HybridSleep,
        Self::SuspendThenHibernate,
    ];

    /// The name to show a human.
    pub fn label(self) -> &'static str {
        match self {
            Self::Suspend => "suspend (to RAM)",
            Self::Hibernate => "hibernate (to disk)",
            Self::HybridSleep => "hybrid sleep",
            Self::SuspendThenHibernate => "suspend, then hibernate",
        }
    }

    /// The word systemd uses for it, which doubles as sleepy's own name for
    /// it on the command line.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Suspend => "suspend",
            Self::Hibernate => "hibernate",
            Self::HybridSleep => "hybrid-sleep",
            Self::SuspendThenHibernate => "suspend-then-hibernate",
        }
    }

    /// Finds an action by the name the user typed (`hibernate`,
    /// `hybrid-sleep`, ...).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.verb() == name)
    }
}

/// Answers "can this machine perform `action` right now?".
///
/// A trait so the listing logic can be tested with a fake, and so a frontend
/// could bring its own answer.
pub trait Probe {
    fn can(&self, action: SleepAction) -> bool;
}

impl Probe for Vec<SleepAction> {
    fn can(&self, action: SleepAction) -> bool {
        self.contains(&action)
    }
}

/// The actions a [`Probe`] reports, in display order.
pub fn detect_with(probe: &dyn Probe) -> Vec<SleepAction> {
    SleepAction::ALL
        .into_iter()
        .filter(|action| probe.can(*action))
        .collect()
}

/// The actions this machine can perform, in display order.
pub fn detect() -> Vec<SleepAction> {
    #[cfg(target_os = "linux")]
    if Systemctl::present() {
        return detect_with(&Systemctl);
    }
    // A machine without systemd to ask (or without a terminal program that
    // knows the answer) gets the assumptions in `Fallback` rather than an
    // empty list.
    detect_with(&Fallback)
}

/// Asks systemd, which owns the sleep machinery on most Linux machines.
#[cfg(target_os = "linux")]
pub struct Systemctl;

#[cfg(target_os = "linux")]
impl Systemctl {
    /// Whether `systemctl` is around to be asked at all.
    fn present() -> bool {
        Command::new("systemctl")
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
    }
}

#[cfg(target_os = "linux")]
impl Probe for Systemctl {
    fn can(&self, action: SleepAction) -> bool {
        let answer = Command::new("systemctl")
            .arg(format!("can-{}", action.verb()))
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned());
        answer.as_deref().is_some_and(possible)
    }
}

/// Whether a `systemctl can-...` answer means the action can happen.
///
/// systemd answers `yes` when it will just work, `challenge` when it will
/// ask for a polkit password first, and `platform` when the firmware does it
/// without systemd's help. All three can happen. `no`, `disabled`, `off`
/// and friends cannot, and neither can an empty answer.
fn possible(answer: &str) -> bool {
    matches!(answer.trim(), "yes" | "challenge" | "platform")
}

/// What to assume where there is no systemd to ask.
///
/// macOS suspends with `pmset sleepnow` and Windows with powrprof, and
/// neither exposes per-action availability the way `systemctl can-...` does,
/// so we offer what we know how to run: suspend everywhere, and hibernate on
/// Windows, where it is a setting the user can turn on and we can ask for.
struct Fallback;

impl Probe for Fallback {
    fn can(&self, action: SleepAction) -> bool {
        match action {
            SleepAction::Suspend => true,
            SleepAction::Hibernate => cfg!(target_os = "windows"),
            SleepAction::HybridSleep | SleepAction::SuspendThenHibernate => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for action in SleepAction::ALL {
            assert_eq!(SleepAction::from_name(action.verb()), Some(action));
        }
        assert_eq!(SleepAction::from_name("nap-time"), None);
    }

    #[test]
    fn systemd_answers_are_read_as_systemd_means_them() {
        for answer in ["yes", "challenge", "platform", "yes\n"] {
            assert!(possible(answer), "{answer} should be possible");
        }
        for answer in ["no", "disabled", "off", "", "no\n"] {
            assert!(!possible(answer), "{answer} should be impossible");
        }
    }

    #[test]
    fn detection_keeps_display_order() {
        let probe = vec![SleepAction::HybridSleep, SleepAction::Suspend];
        assert_eq!(
            detect_with(&probe),
            [SleepAction::Suspend, SleepAction::HybridSleep]
        );
    }

    #[test]
    fn a_machine_that_answers_nothing_sleeps_not() {
        assert!(detect_with(&Vec::new()).is_empty());
    }
}
