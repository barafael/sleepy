//! What the window knows, apart from how it looks.
//!
//! The state and the commands live in sleepy-core's `Controller`, shared
//! with the terminal so the two cannot drift apart; this adds only what is
//! the window's own: the clicked key hints and the field focus. The
//! countdown still needs a heartbeat: egui only repaints when asked, and a
//! window nobody is looking at must still fall asleep on time.

use crate::command;
use sleepy_core::{Command, Controller, Options, Status};
use std::time::Duration;
use web_time::Instant;

/// The heartbeat: a countdown that shows whole seconds needs four looks
/// a second, no more.
const TICK: Duration = Duration::from_millis(250);

/// The whole application state.
pub struct SleepyApp {
    /// Everything the app knows and does, shared with the terminal.
    pub controller: Controller,
    /// The last command run and when, so its key hint can light up.
    pub pressed: Option<(Command, f64)>,
    /// Until the duration field has been focused once: like the terminal,
    /// where the keystrokes go to the field from the first frame.
    pub focus_field: bool,
    /// For timing the key pulses.
    ctx: egui::Context,
}

impl SleepyApp {
    /// Starts from the command line's choices, exactly as the terminal
    /// does: a named duration arms at once, a named mode highlights.
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        crate::theme::install(&cc.egui_ctx);
        Self {
            controller: Controller::new(options),
            pressed: None,
            focus_field: true,
            ctx: cc.egui_ctx.clone(),
        }
    }

    /// Runs one command, from a key press or a clicked key hint, and notes
    /// the moment so the hint can light up. What the command does is
    /// core's answer - [`Controller::apply`] - the same one the terminal
    /// gets.
    fn run(&mut self, command: Command) {
        self.pressed = Some((command, self.ctx.input(|i| i.time)));
        // A tab cannot close itself: on the web, asking to leave means
        // giving up the countdown, and the page stays idle and armable.
        #[cfg(target_arch = "wasm32")]
        if command == Command::Quit {
            self.controller.abort();
            return;
        }
        self.controller.apply(command);
    }
}

impl eframe::App for SleepyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // egui only calls back when asked; a countdown must ask for its
        // own heartbeat. `logic` runs even while the window is hidden, so
        // a minimised sleepy still puts the machine to sleep on time.
        match self.controller.timer.status() {
            // On the web the bar falls smoothly: a visible page takes
            // every animation frame the display offers, and a hidden
            // one the browser throttles to about a look a second by
            // itself - smooth where it is seen, cheap where it is not.
            #[cfg(target_arch = "wasm32")]
            Status::Armed => ctx.request_repaint(),
            // The desktop needs four looks a second for a countdown
            // showing whole seconds, no more.
            #[cfg(not(target_arch = "wasm32"))]
            Status::Armed => ctx.request_repaint_after(TICK),
            // Paused shows nothing that moves; the heartbeat only
            // keeps the clock turning until the countdown resumes.
            Status::Paused => ctx.request_repaint_after(TICK),
            Status::Idle | Status::Fired => {}
        }
        if self.controller.tick(Instant::now()) {
            self.controller.perform();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let typing = ctx.memory(|m| m.focused() == Some(crate::ui::field_id()));
        for command in command::take(&ctx, typing) {
            if command == Command::Escape && typing {
                // Escape leaves the field first; the next one does what
                // the terminal's escape does.
                ctx.memory_mut(|m| m.surrender_focus(crate::ui::field_id()));
            } else {
                self.run(command);
            }
        }
        if !self.controller.running() {
            // A browser tab cannot close itself; the native window
            // leaves, and the web page simply stays, idle and armable.
            #[cfg(not(target_arch = "wasm32"))]
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if let Some(command) = crate::ui::render(self, ui) {
            self.run(command);
        }
    }
}
