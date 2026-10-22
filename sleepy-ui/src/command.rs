//! How the window's keys become sleepy-core's commands, and how the same
//! keys are drawn as clickable hints in the status bar.
//!
//! One table drives both: `take` turns key presses into commands, and
//! `Hint::all` draws the same keys in the status bar, where clicking one
//! stands in for pressing it. What a command does is core's answer, not
//! this file's; this one only transliterates.

use sleepy_core::Command;

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
            hint("ctrl+d", "dry run", Some(Command::ToggleDryRun)),
            hint("k", "keep awake", Some(Command::ToggleKeepAwake)),
            // Arming waits for an idle countdown, so while one runs the
            // hint stands for nothing a click could do.
            hint("enter", "arm", (!counting).then_some(Command::Arm)),
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
/// `d` is one at all times, and the terminal affords the same. What a
/// text field has no use for - the arrows, enter, escape, Ctrl+D - the
/// window takes either way, and consumes what it takes so the field
/// never sees it.
pub fn take(ctx: &egui::Context, typing: bool) -> Vec<Command> {
    let mut commands = Vec::new();
    ctx.input_mut(|i| {
        let mut press = |modifiers: egui::Modifiers, key: egui::Key, command| {
            if i.consume_key(modifiers, key) {
                commands.push(command);
            }
        };
        let none = egui::Modifiers::NONE;
        press(none, egui::Key::ArrowUp, Command::PreviousMode);
        press(none, egui::Key::ArrowDown, Command::NextMode);
        press(none, egui::Key::Enter, Command::Arm);
        press(none, egui::Key::Escape, Command::Escape);
        // The dry-run toggle is a safety valve that must work even
        // mid-countdown, and no text field has a use for Ctrl+D, so it
        // is a command while typing too - leaving plain `d` to type.
        press(egui::Modifiers::CTRL, egui::Key::D, Command::ToggleDryRun);
        if !typing {
            press(none, egui::Key::K, Command::ToggleKeepAwake);
            press(none, egui::Key::Space, Command::PauseResume);
            // The numpad's + and - arrive as Plus and Minus: egui folds
            // the keypad variants into the same logical keys.
            press(none, egui::Key::Plus, Command::Longer);
            press(none, egui::Key::Equals, Command::Longer);
            press(none, egui::Key::Minus, Command::Shorter);
            press(none, egui::Key::Q, Command::Quit);
        }
    });
    commands
}

#[cfg(test)]
// A test that cannot get its answer is broken, not a machine that said no.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers};

    /// A context with one key press queued up.
    fn pressed_with(key: egui::Key, modifiers: Modifiers) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.input_mut(|i| {
            i.events.push(Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            });
        });
        ctx
    }

    fn pressed(key: egui::Key) -> egui::Context {
        pressed_with(key, Modifiers::NONE)
    }

    #[test]
    fn keys_become_commands() {
        let cases = [
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
        // The dry run rides on the chord, not the letter.
        assert_eq!(
            take(&pressed_with(egui::Key::D, Modifiers::CTRL), false),
            vec![Command::ToggleDryRun]
        );
    }

    #[test]
    fn typing_keeps_its_letters_but_not_its_arrows() {
        // The field's characters stay out of the command queue...
        for key in [egui::Key::D, egui::Key::K, egui::Key::Q, egui::Key::Space] {
            assert_eq!(take(&pressed(key), true), Vec::new(), "{key:?}");
        }
        // ...and `d` is a letter even when the field is not focused.
        assert_eq!(take(&pressed(egui::Key::D), false), Vec::new());
        // ...and what the field cannot use the window still takes.
        for key in [egui::Key::ArrowUp, egui::Key::Enter, egui::Key::Escape] {
            assert_eq!(take(&pressed(key), true).len(), 1, "{key:?}");
        }
        // Ctrl+D is a safety valve: taken while typing too.
        assert_eq!(
            take(&pressed_with(egui::Key::D, Modifiers::CTRL), true),
            vec![Command::ToggleDryRun]
        );
    }

    #[test]
    fn taken_keys_are_consumed() {
        // The field must not also type what the window took as a command.
        let ctx = pressed(egui::Key::Enter);
        assert_eq!(take(&ctx, true), vec![Command::Arm]);
        assert!(!ctx.input(|i| i.key_pressed(egui::Key::Enter)));
        let ctx = pressed_with(egui::Key::D, Modifiers::CTRL);
        assert_eq!(take(&ctx, true), vec![Command::ToggleDryRun]);
        assert!(
            ctx.input(|i| i.events.clone()).iter().all(|e| !matches!(
                e,
                Event::Key {
                    key: egui::Key::D,
                    ..
                }
            )),
            "the field must not see the chord either"
        );
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
                [
                    "up/down", "ctrl+d", "k", "enter", "space", "esc", "+/-", "q"
                ]
            );
            for hint in hints {
                // Multi-command hints never are; and arm is not while a
                // countdown runs, because arming waits for an idle one.
                let clickable = !hint.keys.contains('/') && !(counting && hint.keys == "enter");
                assert_eq!(hint.command.is_some(), clickable, "{}", hint.keys);
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
