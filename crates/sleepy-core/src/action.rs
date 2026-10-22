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

    /// The logind D-Bus method that answers whether this can happen now.
    ///
    /// systemd 262 removed the `systemctl can-...` verbs, pointing at these
    /// methods instead; logind has answered them all along.
    pub fn method(self) -> &'static str {
        match self {
            Self::Suspend => "CanSleep",
            Self::Hibernate => "CanHibernate",
            Self::HybridSleep => "CanHybridSleep",
            Self::SuspendThenHibernate => "CanSuspendThenHibernate",
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
/// could bring its own answer. A probe that has no opinion returns `None`,
/// which lets [`detect_with`] ask the next one.
pub trait Probe {
    /// `Some(true)` can, `Some(false)` cannot, `None` no idea.
    fn can(&self, action: SleepAction) -> Option<bool>;
}

impl Probe for Vec<SleepAction> {
    fn can(&self, action: SleepAction) -> Option<bool> {
        Some(self.contains(&action))
    }
}

/// The actions the probes report, in display order.
///
/// For each action, the first probe with an opinion decides - so a cascade
/// goes from most specific to least, ending in a probe that always answers.
pub fn detect_with(probes: &[&dyn Probe]) -> Vec<SleepAction> {
    SleepAction::ALL
        .into_iter()
        .filter(|action| probes.iter().find_map(|probe| probe.can(*action)) == Some(true))
        .collect()
}

/// The actions this machine can perform, in display order.
pub fn detect() -> Vec<SleepAction> {
    #[cfg(target_os = "linux")]
    let probes: [&dyn Probe; 3] = [&Logind, &Systemctl, &Fallback];
    #[cfg(not(target_os = "linux"))]
    let probes: [&dyn Probe; 1] = [&Fallback];
    // A machine where nobody can answer (no logind, no systemctl) gets the
    // assumptions in `Fallback` rather than an empty list.
    detect_with(&probes)
}

/// Asks logind over D-Bus, the way systemd 262 wants you to.
///
/// `busctl call org.freedesktop.login1 ... CanSleep` answers `s "yes"`; if
/// busctl or logind is missing the call fails and we shrug.
#[cfg(target_os = "linux")]
pub struct Logind;

#[cfg(target_os = "linux")]
impl Probe for Logind {
    fn can(&self, action: SleepAction) -> Option<bool> {
        let reply = Command::new("busctl")
            .args([
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                action.method(),
            ])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())?;
        possible(reply_word(&reply))
    }
}

/// Asks systemd the old way, for the benefit of machines whose systemd
/// still has the `can-...` verbs (262 removed them, and answers the call
/// with an error we read as a shrug).
#[cfg(target_os = "linux")]
pub struct Systemctl;

#[cfg(target_os = "linux")]
impl Probe for Systemctl {
    fn can(&self, action: SleepAction) -> Option<bool> {
        let reply = Command::new("systemctl")
            .arg(format!("can-{}", action.verb()))
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())?;
        possible(reply_word(&reply))
    }
}

/// The answer inside a reply: the word inside the first pair of quotes
/// (busctl's `s "yes"`), or the whole reply when nothing is quoted
/// (systemctl's bare `yes`).
fn reply_word(reply: &str) -> &str {
    reply.split('"').nth(1).unwrap_or(reply.trim())
}

/// Whether an answer word means the action can happen.
///
/// systemd answers `yes` when it will just work, `challenge` when it will
/// ask for a polkit password first, and `platform` when the firmware does it
/// without systemd's help - all three can happen. `no`, `na` (not
/// applicable), `disabled` and `off` cannot. Anything else is a word we do
/// not know, which is a shrug, not a veto.
fn possible(word: &str) -> Option<bool> {
    match word.to_ascii_lowercase().as_str() {
        "yes" | "challenge" | "platform" => Some(true),
        "no" | "na" | "disabled" | "off" => Some(false),
        _ => None,
    }
}

/// What to assume where there is no systemd to ask.
///
/// macOS suspends with `pmset sleepnow` and Windows with powrprof, and
/// neither exposes per-action availability the way `systemctl can-...` does,
/// so we offer what we know how to run: suspend everywhere, and hibernate on
/// Windows, where it is a setting the user can turn on and we can ask for.
struct Fallback;

impl Probe for Fallback {
    fn can(&self, action: SleepAction) -> Option<bool> {
        let can = match action {
            SleepAction::Suspend => true,
            SleepAction::Hibernate => cfg!(target_os = "windows"),
            SleepAction::HybridSleep | SleepAction::SuspendThenHibernate => false,
        };
        Some(can)
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
    fn every_action_has_a_logind_method() {
        for action in SleepAction::ALL {
            assert!(
                action.method().starts_with("Can"),
                "{} has no Can* method",
                action.verb()
            );
        }
    }

    #[test]
    fn answer_words_are_read_as_systemd_means_them() {
        for word in ["yes", "challenge", "platform", "Yes", "CHALLENGE"] {
            assert_eq!(possible(word), Some(true), "{word} should be possible");
        }
        for word in ["no", "na", "disabled", "off"] {
            assert_eq!(possible(word), Some(false), "{word} should be impossible");
        }
        for word in ["", "maybe", "banana"] {
            assert_eq!(possible(word), None, "{word} is a shrug, not a veto");
        }
    }

    #[test]
    fn replies_are_unquoted_or_taken_whole() {
        // busctl quotes its reply; systemctl did not.
        assert_eq!(reply_word("s \"yes\"\n"), "yes");
        assert_eq!(reply_word("yes\n"), "yes");
    }

    #[test]
    fn detection_keeps_display_order() {
        let probe = vec![SleepAction::HybridSleep, SleepAction::Suspend];
        assert_eq!(
            detect_with(&[&probe]),
            [SleepAction::Suspend, SleepAction::HybridSleep]
        );
    }

    #[test]
    fn a_machine_that_answers_nothing_sleeps_not() {
        assert!(detect_with(&[&Vec::new()]).is_empty());
    }

    /// A probe with one fixed answer for every action, including a shrug.
    struct Scripted(Option<bool>);

    impl Probe for Scripted {
        fn can(&self, _action: SleepAction) -> Option<bool> {
            self.0
        }
    }

    #[test]
    fn the_first_probe_with_an_opinion_decides() {
        // The shrug steps aside for the probe that knows.
        let knows = vec![SleepAction::Suspend];
        assert_eq!(
            detect_with(&[&Scripted(None), &knows]),
            [SleepAction::Suspend]
        );
        // And the one that answers first is not overruled by the next.
        assert_eq!(detect_with(&[&Scripted(Some(true)), &Vec::new()]).len(), 4);
    }
}
