//! What the window knows, apart from how it looks.
//!
//! The same plain fields the terminal keeps - the modes on offer, the
//! typed duration, the timer - and egui widgets that edit them directly,
//! so there is no key handling to write at all. The countdown, however,
//! still needs a heartbeat: egui only repaints when asked, and a window
//! nobody is looking at must still fall asleep on time.

use humantime::format_duration;
use sleepy_core::{
    Backend, Error, SleepAction, Status, Timer, TimerEvent, describe, detect, execute,
    parse_duration,
};
use std::time::{Duration, Instant};

/// The duration offered to a user who types nothing.
const DEFAULT: &str = "30 min";

/// What one `-1 min` or `+1 min` is worth, in seconds.
const NUDGE: i64 = 60;

/// The heartbeat: a countdown that shows whole seconds needs four looks
/// a second, no more.
const TICK: Duration = Duration::from_millis(250);

/// The whole application state.
pub struct SleepyApp {
    /// What this machine can do, in display order.
    pub modes: Vec<SleepAction>,
    /// Index into `modes`.
    pub selected: usize,
    /// The duration field.
    pub input: String,
    /// The countdown itself.
    pub timer: Timer,
    /// Whether to really sleep or only say so.
    pub backend: Backend,
    /// The action a finished countdown ran, to be shown in the window.
    pub fired: Option<SleepAction>,
    /// Why the machine is still awake after a fired countdown, if it is.
    pub failure: Option<Error>,
}

impl SleepyApp {
    /// Starts from the command line's choices, exactly as the terminal
    /// does: a named duration arms at once, a named mode highlights.
    pub fn new(backend: Backend, duration: Option<String>, mode: Option<SleepAction>) -> Self {
        let modes = detect();
        let selected = mode
            .and_then(|want| modes.iter().position(|have| *have == want))
            .unwrap_or(0);
        let arm = duration.is_some();
        let mut app = Self {
            modes,
            selected,
            input: duration.unwrap_or_else(|| DEFAULT.into()),
            timer: Timer::new(),
            backend,
            fired: None,
            failure: None,
        };
        if arm {
            app.arm_selected();
        }
        app
    }

    /// The highlighted mode, if the machine offered any.
    pub fn selected_action(&self) -> Option<SleepAction> {
        self.modes.get(self.selected).copied()
    }

    /// What the duration field means right now: a duration, or the reason
    /// it is not one yet.
    pub fn duration(&self) -> Result<Duration, sleepy_core::ParseError> {
        parse_duration(&self.input)
    }

    /// Arms the highlighted mode with the typed duration; refuses - and
    /// lets the field show why - when either is missing.
    pub fn arm_selected(&mut self) -> bool {
        match (self.selected_action(), parse_duration(&self.input)) {
            (Some(action), Ok(duration)) => {
                self.fired = None;
                self.failure = None;
                self.timer.arm(action, duration)
            }
            _ => false,
        }
    }

    /// Does the thing the countdown was armed for. On a real sleep this
    /// call does not return until the machine wakes; the dry run answers
    /// at once, and the window is there to say so.
    fn perform(&mut self) {
        if let Some(action) = self.fired {
            self.failure = execute(action, self.backend).err();
        }
    }
}

impl eframe::App for SleepyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // egui only calls back when asked; a countdown must ask for its
        // own heartbeat. `logic` runs even while the window is hidden, so
        // a minimised sleepy still puts the machine to sleep on time.
        if matches!(self.timer.status(), Status::Armed | Status::Paused) {
            ctx.request_repaint_after(TICK);
        }
        if self.timer.tick(Instant::now()) == TimerEvent::Fired {
            self.fired = Some(
                self.timer
                    .action()
                    .expect("a fired timer knows what it is doing"),
            );
            self.perform();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The root ui has no margin or background; a panel supplies both.
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("sleepy");
                if self.backend == Backend::DryRun {
                    // Say it loudly: this window will talk about sleeping
                    // and do nothing else.
                    ui.colored_label(egui::Color32::ORANGE, "dry run");
                }
            });
            ui.add_space(8.0);
            draw_modes(ui, self);
            ui.add_space(8.0);
            draw_when(ui, self);
            ui.add_space(8.0);
            draw_buttons(ui, self);
            ui.add_space(12.0);
            draw_countdown(ui, self);
        });
    }
}

fn draw_modes(ui: &mut egui::Ui, app: &mut SleepyApp) {
    if app.modes.is_empty() {
        ui.colored_label(
            egui::Color32::DARK_RED,
            "this machine offers nothing to arm",
        );
        return;
    }
    ui.horizontal(|ui| {
        for (index, mode) in app.modes.iter().enumerate() {
            ui.selectable_value(&mut app.selected, index, mode.label());
        }
    });
}

fn draw_when(ui: &mut egui::Ui, app: &mut SleepyApp) {
    let field = egui::TextEdit::singleline(&mut app.input)
        .hint_text("45s, 30 min, 1h30m")
        .desired_width(f32::INFINITY);
    let response = ui.add(field);
    // Enter in the field means the same as the arm button.
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    if response.lost_focus() && enter && app.selected_action().is_some() && app.duration().is_ok() {
        app.arm_selected();
    }
    match app.duration() {
        Ok(duration) => {
            ui.colored_label(
                egui::Color32::DARK_GREEN,
                format!("-> in {}", format_duration(duration)),
            );
        }
        Err(error) => {
            ui.colored_label(egui::Color32::DARK_RED, error.hint());
        }
    }
}

fn draw_buttons(ui: &mut egui::Ui, app: &mut SleepyApp) {
    let armed = app.timer.status() == Status::Armed;
    let paused = app.timer.status() == Status::Paused;
    let counting = armed || paused;
    ui.horizontal(|ui| {
        let can_arm = app.selected_action().is_some() && app.duration().is_ok();
        if ui
            .add_enabled(can_arm, egui::Button::new("arm"))
            .on_hover_text("start the countdown (or press enter in the field)")
            .clicked()
        {
            app.arm_selected();
        }
        if ui.add_enabled(armed, egui::Button::new("pause")).clicked() {
            app.timer.pause();
        }
        if ui
            .add_enabled(paused, egui::Button::new("resume"))
            .clicked()
        {
            app.timer.resume();
        }
        if ui
            .add_enabled(counting, egui::Button::new("abort"))
            .on_hover_text("back to idle; the machine stays awake")
            .clicked()
        {
            app.timer.abort();
            app.fired = None;
            app.failure = None;
        }
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(counting, egui::Button::new("-1 min"))
            .clicked()
        {
            app.timer.adjust(-NUDGE);
        }
        if ui
            .add_enabled(counting, egui::Button::new("+1 min"))
            .clicked()
        {
            app.timer.adjust(NUDGE);
        }
    });
}

fn draw_countdown(ui: &mut egui::Ui, app: &mut SleepyApp) {
    match app.timer.status() {
        Status::Idle => {
            ui.weak(match app.selected_action() {
                Some(mode) => format!("press arm to start the {} countdown", mode.label()),
                None => "nothing to arm".to_string(),
            });
        }
        Status::Armed | Status::Paused => {
            // Whole seconds: a countdown that jitters in microseconds is
            // noise, not information.
            let left =
                format_duration(Duration::from_secs(app.timer.remaining().as_secs())).to_string();
            ui.label(egui::RichText::new(&left).monospace().size(32.0));
            ui.add(
                egui::ProgressBar::new(app.timer.progress())
                    .text(app.timer.action().map(|m| m.label()).unwrap_or("sleep")),
            );
            if app.timer.status() == Status::Paused {
                ui.weak("paused - the countdown waits for resume");
            }
        }
        Status::Fired => {
            let action = app
                .fired
                .map(|mode| mode.label().to_lowercase())
                .unwrap_or_else(|| "sleep".to_string());
            ui.label(egui::RichText::new(format!("time! {action} the machine...")).size(24.0));
            if app.backend == Backend::DryRun
                && let Some(command) = app.fired.and_then(describe)
            {
                ui.label(format!("would have run: {command}"));
            }
            if let Some(error) = &app.failure {
                ui.colored_label(egui::Color32::DARK_RED, error.to_string());
            }
        }
    }
}
