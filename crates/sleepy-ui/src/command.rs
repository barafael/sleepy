//! The commands the window answers to, from the keyboard or a clicked
//! key hint - the terminal's keys, transliterated.
//!
//! One table drives both: `take` turns key presses into commands, and
//! `Hint::all` draws the same keys in the status bar, where clicking one
//! stands in for pressing it.

/// Anything the user can ask the window to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Move the mode highlight up.
    PreviousMode,
    /// Move the mode highlight down.
    NextMode,
    /// Switch between really sleeping and only saying so.
    ToggleDryRun,
    /// Switch the screen inhibition.
    ToggleKeepAwake,
    /// Start the countdown with the highlighted mode and typed duration.
    Arm,
    /// Pause a running countdown, or resume a paused one.
    PauseResume,
    /// Abort a countdown; when there is none, close the window.
    Escape,
    /// Put a minute back on the countdown.
    Longer,
    /// Take a minute off the countdown.
    Shorter,
    /// Close the window.
    Quit,
}

/// A key hint in the status bar: the keycap, what it does, and the command
/// a click runs - `None` when the hint stands for more than one command,
/// because a click cannot choose between them.
pub struct Hint {
    /// The keycap text.
    pub keys: &'static str,
    /// What the key does.
    pub label: &'static str,
    /// What a click runs.
    pub command: Option<Command>,
}

impl Hint {
    /// The hints, in the order the terminal footer lists them. The escape
    /// hint reads "abort" while a countdown runs and "quit" once none does,
    /// because that is what the key will do.
    pub fn all(counting: bool) -> [Hint; 8] {
        let hint = |keys, label, command| Hint {
            keys,
            label,
            command,
        };
        [
            hint("up/down", "mode", None),
            hint("d", "dry run", Some(Command::ToggleDryRun)),
            hint("k", "keep awake", Some(Command::ToggleKeepAwake)),
            hint("enter", "arm", Some(Command::Arm)),
            hint("space", "pause", Some(Command::PauseResume)),
            hint(
                "esc",
                if counting { "abort" } else { "quit" },
                Some(Command::Escape),
            ),
            hint("+/-", "minute", None),
            hint("q", "quit", Some(Command::Quit)),
        ]
    }
}

/// Takes this frame's key presses, as commands.
///
/// While the duration field is being typed into, it keeps the letters:
/// `d`, `k` and space are humantime's characters too, and a window can
/// afford what the terminal had to sacrifice. What a text field has no use
/// for - the arrows, enter, escape - the window takes either way, and
/// consumes what it takes so the field never sees it.
pub fn take(ctx: &egui::Context, typing: bool) -> Vec<Command> {
    let mut commands = Vec::new();
    ctx.input_mut(|i| {
        let mut press = |key: egui::Key, command| {
            if i.consume_key(egui::Modifiers::NONE, key) {
                commands.push(command);
            }
        };
        press(egui::Key::ArrowUp, Command::PreviousMode);
        press(egui::Key::ArrowDown, Command::NextMode);
        press(egui::Key::Enter, Command::Arm);
        press(egui::Key::Escape, Command::Escape);
        if !typing {
            press(egui::Key::D, Command::ToggleDryRun);
            press(egui::Key::K, Command::ToggleKeepAwake);
            press(egui::Key::Space, Command::PauseResume);
            // The numpad's + and - arrive as Plus and Minus: egui folds
            // the keypad variants into the same logical keys.
            press(egui::Key::Plus, Command::Longer);
            press(egui::Key::Equals, Command::Longer);
            press(egui::Key::Minus, Command::Shorter);
            press(egui::Key::Q, Command::Quit);
        }
    });
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers};

    /// A context with one key press queued up.
    fn pressed(key: egui::Key) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.input_mut(|i| {
            i.events.push(Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        });
        ctx
    }

    #[test]
    fn keys_become_commands() {
        let cases = [
            (egui::Key::D, Command::ToggleDryRun),
            (egui::Key::K, Command::ToggleKeepAwake),
            (egui::Key::Q, Command::Quit),
            (egui::Key::Space, Command::PauseResume),
            (egui::Key::Enter, Command::Arm),
            (egui::Key::Escape, Command::Escape),
            (egui::Key::ArrowUp, Command::PreviousMode),
            (egui::Key::ArrowDown, Command::NextMode),
            (egui::Key::Minus, Command::Shorter),
            (egui::Key::Equals, Command::Longer),
        ];
        for (key, command) in cases {
            assert_eq!(take(&pressed(key), false), vec![command], "{key:?}");
        }
    }

    #[test]
    fn typing_keeps_its_letters_but_not_its_arrows() {
        // The field's characters stay out of the command queue...
        for key in [egui::Key::D, egui::Key::K, egui::Key::Q, egui::Key::Space] {
            assert_eq!(take(&pressed(key), true), Vec::new(), "{key:?}");
        }
        // ...and what the field cannot use the window still takes.
        for key in [egui::Key::ArrowUp, egui::Key::Enter, egui::Key::Escape] {
            assert_eq!(take(&pressed(key), true).len(), 1, "{key:?}");
        }
    }

    #[test]
    fn taken_keys_are_consumed() {
        // The field must not also type what the window took as a command.
        let ctx = pressed(egui::Key::Enter);
        assert_eq!(take(&ctx, true), vec![Command::Arm]);
        assert!(!ctx.input(|i| i.key_pressed(egui::Key::Enter)));
    }

    #[test]
    fn only_single_command_hints_are_clickable() {
        for counting in [false, true] {
            let hints = Hint::all(counting);
            assert_eq!(hints.len(), 8);
            // The terminal footer's words, kept in order.
            let keys: Vec<&str> = hints.iter().map(|h| h.keys).collect();
            assert_eq!(
                keys,
                ["up/down", "d", "k", "enter", "space", "esc", "+/-", "q"]
            );
            for hint in hints {
                assert_eq!(
                    hint.command.is_some(),
                    !hint.keys.contains('/'),
                    "{}",
                    hint.keys
                );
            }
        }
    }

    #[test]
    fn escape_says_what_it_will_do() {
        let esc = |counting| {
            Hint::all(counting)
                .into_iter()
                .find(|h| h.keys == "esc")
                .expect("the escape hint")
        };
        assert_eq!(esc(true).label, "abort");
        assert_eq!(esc(false).label, "quit");
    }
}
