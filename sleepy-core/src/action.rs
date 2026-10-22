//! The ways a machine can stop, and what this one can actually do.

use std::path::{Path, PathBuf};
// Only the linux probes run a command to ask a question.
#[cfg(target_os = "linux")]
use std::process::Command;
use strum::{Display, EnumIter, EnumString, IntoEnumIterator};

/// What the countdown does when it ends, listed from quickest to return
/// from to most final, ending with the two whose business is not the
/// machine's.
///
/// Which of the sleep modes exist is up to the firmware and
/// the operating system, so [`detect`] asks instead of assuming; the
/// script exists because the user left one in the working directory.
///
/// The words are systemd's - except `RunScript` and `Notify`, which
/// are sleepy's own - and they double as sleepy's names for the modes
/// on the command line; `strum` turns variant names into kebab-case,
/// and `Shutdown` spells
/// itself `poweroff` the way systemd does - while still answering to
/// `shutdown`, the word a human reaches for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumString, Display, EnumIter)]
#[strum(serialize_all = "kebab_case")]
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
    /// Power off: the most final. Nothing survives; "waking up" means
    /// booting again.
    #[strum(to_string = "poweroff", serialize = "shutdown")]
    Shutdown,
    /// Run the script the user left in the working directory instead of
    /// sleeping: `action.sh`, or `action.ps1` where the shell to honour
    /// is PowerShell's. What the script does is the script's business -
    /// sleepy only promises to run it when the countdown ends.
    RunScript,
    /// Send a notification instead of sleeping: the browser's one mode,
    /// where a page cannot stop its machine but can at least clear its
    /// throat when the time comes. The web build offers only this; no
    /// native probe ever does.
    Notify,
}

impl SleepAction {
    /// The name to show a human.
    ///
    /// A match rather than a derive: this is display copy - "suspend (to
    /// RAM)" explains, it does not name - and copy is not a serialization.
    /// The machine's own words come from `strum` above.
    pub fn label(self) -> &'static str {
        match self {
            Self::Suspend => "suspend (to RAM)",
            Self::Hibernate => "hibernate (to disk)",
            Self::HybridSleep => "hybrid sleep",
            Self::SuspendThenHibernate => "suspend, then hibernate",
            Self::Shutdown => "shutdown (power off)",
            Self::RunScript => "run-script",
            Self::Notify => "notify",
        }
    }

    /// The words a finished countdown shows after "time!": what is being
    /// done, to what.
    ///
    /// A match rather than `label` + " the machine" everywhere: running
    /// a script is not something done to the machine, and copy that does
    /// not read needs its own words.
    pub fn firing(self) -> String {
        match self {
            Self::RunScript => "run the script".into(),
            // Not something done to the machine - the machine is exactly
            // what stays awake to say it.
            Self::Notify => "send the notification".into(),
            action => format!("{} the machine", action.label().to_lowercase()),
        }
    }

    /// The logind D-Bus method that answers whether this can happen now,
    /// for the actions logind is asked about at all.
    ///
    /// systemd 262 removed the `systemctl can-...` verbs, pointing at these
    /// methods instead; logind has answered them all along. A match rather
    /// than `Can` + name, because the mapping is irregular - `Suspend`
    /// asks `CanSleep` and `Shutdown` asks `CanPowerOff` - and an
    /// irregular mapping is what a match is for.
    pub fn method(self) -> Option<&'static str> {
        match self {
            Self::Suspend => Some("CanSleep"),
            Self::Hibernate => Some("CanHibernate"),
            Self::HybridSleep => Some("CanHybridSleep"),
            Self::SuspendThenHibernate => Some("CanSuspendThenHibernate"),
            Self::Shutdown => Some("CanPowerOff"),
            // logind is not asked about the user's script, and a browser
            // has no logind to ask about a notification.
            Self::RunScript | Self::Notify => None,
        }
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
    SleepAction::iter()
        .filter(|action| probes.iter().find_map(|probe| probe.can(*action)) == Some(true))
        .collect()
}

/// The actions this machine can perform, in display order.
pub fn detect() -> Vec<SleepAction> {
    #[cfg(target_arch = "wasm32")]
    {
        // A browser tab cannot sleep its machine; the notification is
        // the web's only mode. It is offered whether or not this browser
        // will grant it - firing is where a refusal is read.
        vec![SleepAction::Notify]
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        #[cfg(target_os = "linux")]
        let probes: [&dyn Probe; 3] = [&Logind, &Systemctl, &Fallback];
        #[cfg(not(target_os = "linux"))]
        let probes: [&dyn Probe; 1] = [&Fallback];
        // A machine where nobody can answer (no logind, no systemctl) gets
        // the assumptions in `Fallback` rather than an empty list.
        let mut actions = detect_with(&probes);
        // The script is not a capability of the machine but a promise the
        // user left in the working directory, so it is not what the probes
        // are asked about - the file's presence is the whole answer. It
        // comes last, which is also where `iter` puts it.
        if script_path().is_some() {
            actions.push(SleepAction::RunScript);
        }
        actions
    }
}

/// The script the [`SleepAction::RunScript`] countdown runs, when the
/// user left one in the working directory: `action.sh`, or `action.ps1`
/// on Windows, where the shell to honour is PowerShell's.
pub fn script_path() -> Option<PathBuf> {
    script_in(Path::new("."))
}

/// [`script_path`] with the directory spelled out, so the tests can
/// look for a script without moving the whole process.
fn script_in(dir: &Path) -> Option<PathBuf> {
    let path = dir.join(script_name());
    path.is_file().then_some(path)
}

/// The script's conventional name, per platform.
pub(crate) fn script_name() -> &'static str {
    if cfg!(windows) {
        "action.ps1"
    } else {
        "action.sh"
    }
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
        let method = action.method()?;
        let reply = Command::new("busctl")
            .args([
                "call",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                method,
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
        // Only the sleep modes have a `can-...` verb; asking for
        // `can-run-script` would just spawn a refusal.
        action.method()?;
        let reply = Command::new("systemctl")
            .arg(format!("can-{action}"))
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
// Asking is native machinery; a browser has one mode and nothing to probe.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn reply_word(reply: &str) -> &str {
    reply.split('"').nth(1).unwrap_or_else(|| reply.trim())
}

/// Whether an answer word means the action can happen.
///
/// systemd answers `yes` when it will just work, `challenge` when it will
/// ask for a polkit password first, and `platform` when the firmware does it
/// without systemd's help - all three can happen. `no`, `na` (not
/// applicable), `disabled` and `off` cannot. Anything else is a word we do
/// not know, which is a shrug, not a veto.
// Asking is native machinery; a browser has one mode and nothing to probe.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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
/// so we offer what we know how to run: suspend and shut down everywhere,
/// and hibernate on Windows, where it is a setting the user can turn on and
/// we can ask for.
// Asking is native machinery; a browser has one mode and nothing to assume.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct Fallback;

impl Probe for Fallback {
    fn can(&self, action: SleepAction) -> Option<bool> {
        let can = match action {
            SleepAction::Suspend | SleepAction::Shutdown => true,
            SleepAction::Hibernate => cfg!(target_os = "windows"),
            // Hybrid sleep and suspend-then-hibernate are logind extras
            // the fallback never offers. And the machine can run any
            // script; whether sleepy offers to run *this* one is the
            // file's business, and `detect` asks the file itself. The
            // notification is the web's mode; a native sleepy has real
            // sleep to offer instead.
            SleepAction::HybridSleep
            | SleepAction::SuspendThenHibernate
            | SleepAction::RunScript
            | SleepAction::Notify => false,
        };
        Some(can)
    }
}

#[cfg(test)]
// A test that cannot get its answer is broken, not a machine that said no.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_words_are_systemds() {
        // Display is the command-line word, and `poweroff` is the one a
        // case conversion would get wrong, so pin the whole list. The
        // last two words are sleepy's own: no systemd ever had them.
        let words: Vec<String> = SleepAction::iter().map(|a| a.to_string()).collect();
        assert_eq!(
            words,
            [
                "suspend",
                "hibernate",
                "hybrid-sleep",
                "suspend-then-hibernate",
                "poweroff",
                "run-script",
                "notify"
            ]
        );
    }

    #[test]
    fn names_round_trip() {
        for action in SleepAction::iter() {
            assert_eq!(action.to_string().parse(), Ok(action));
        }
        assert!("nap-time".parse::<SleepAction>().is_err());
    }

    #[test]
    fn shutdown_answers_to_both_names() {
        // systemd's word and the human one.
        assert_eq!("poweroff".parse::<SleepAction>(), Ok(SleepAction::Shutdown));
        assert_eq!("shutdown".parse::<SleepAction>(), Ok(SleepAction::Shutdown));
    }

    #[test]
    fn every_sleep_action_has_a_logind_method() {
        for action in SleepAction::iter() {
            let method = action.method();
            if matches!(action, SleepAction::RunScript | SleepAction::Notify) {
                // logind is not asked about the user's script, and a
                // browser has no logind to ask about a notification.
                assert_eq!(method, None);
            } else {
                assert!(
                    method.is_some_and(|m| m.starts_with("Can")),
                    "{action} has no Can* method"
                );
            }
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
        assert_eq!(detect_with(&[&Vec::new()]), [] as [SleepAction; 0]);
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
        assert_eq!(detect_with(&[&Scripted(Some(true)), &Vec::new()]).len(), 7);
    }

    #[test]
    fn the_script_is_found_only_where_it_was_left() {
        // The file's presence in the directory is the whole detection;
        // a directory of one's own, so the test never asks the real
        // working directory about a script it does not own.
        let dir = std::env::temp_dir().join(format!("sleepy-script-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(script_in(&dir), None);
        std::fs::write(dir.join(script_name()), "#!/bin/sh\n").unwrap();
        assert_eq!(script_in(&dir), Some(dir.join(script_name())));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
