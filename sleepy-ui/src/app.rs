//! What the window knows, apart from how it looks.
//!
//! The state and the commands live in sleepy-core's `Controller`, shared
//! with the terminal so the two cannot drift apart; this adds only what is
//! the window's own: the clicked key hints, the field focus, the theme and
//! the geometry. The countdown still needs a heartbeat: egui only repaints
//! when asked, and a window nobody is looking at must still fall asleep on
//! time.

use crate::command;
use sleepy_core::{Command, Controller, Options, Status};
use std::time::{Duration, Instant};

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
    /// For saving settings, which happens outside a frame.
    ctx: egui::Context,
    /// The desktop's light/dark preference.
    #[cfg(target_os = "linux")]
    system_theme: crate::portal::SystemTheme,
    /// Saves the window's size and position.
    window: Option<crate::geometry::Tracker>,
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
            #[cfg(target_os = "linux")]
            system_theme: crate::portal::SystemTheme::watch(cc.egui_ctx.clone()),
            window: None,
        }
    }

    /// Remembers the window's geometry between runs.
    pub fn with_window(mut self, tracker: crate::geometry::Tracker) -> Self {
        self.window = Some(tracker);
        self
    }

    /// Runs one command, from a key press or a clicked key hint, and notes
    /// the moment so the hint can light up. What the command does is
    /// core's answer - [`Controller::apply`] - the same one the terminal
    /// gets.
    fn run(&mut self, command: Command) {
        self.pressed = Some((command, self.ctx.input(|i| i.time)));
        self.controller.apply(command);
    }
}

impl eframe::App for SleepyApp {
    /// Remembers the window's geometry. Not in eframe's storage: the
    /// geometry tracker writes its own file, where the terminal sleepy's
    /// settings already live and where the `XWayland` shift can be undone.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if let Some(window) = &self.window {
            window.save(&self.ctx);
        }
    }

    /// Supplies the system theme where winit does not know it.
    #[cfg(target_os = "linux")]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if raw_input.system_theme.is_none() {
            raw_input.system_theme = self.system_theme.get();
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // egui only calls back when asked; a countdown must ask for its
        // own heartbeat. `logic` runs even while the window is hidden, so
        // a minimised sleepy still puts the machine to sleep on time.
        if matches!(
            self.controller.timer.status(),
            Status::Armed | Status::Paused
        ) {
            ctx.request_repaint_after(TICK);
        }
        if self.controller.tick(Instant::now()) {
            self.controller.perform();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(window) = &mut self.window {
            window.observe(&ctx);
        }
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
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if let Some(command) = crate::ui::render(self, ui) {
            self.run(command);
        }
    }
}
