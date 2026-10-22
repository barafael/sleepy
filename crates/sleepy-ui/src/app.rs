//! What the window knows, apart from how it looks.
//!
//! The same plain fields the terminal keeps - the modes on offer, the
//! typed duration, the timer - and egui widgets that edit them directly,
//! so there is no key handling to write at all.

use sleepy_core::{Backend, SleepAction, Timer, detect};

/// The duration offered to a user who types nothing.
const DEFAULT: &str = "30 min";

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
    /// Whether to really sleep or only say so. The buttons that can set
    /// this come in the next commit; until then the window only ever
    /// promises to talk about sleeping.
    pub backend: Backend,
}

impl SleepyApp {
    pub fn new() -> Self {
        Self {
            modes: detect(),
            selected: 0,
            input: DEFAULT.into(),
            timer: Timer::new(),
            backend: Backend::DryRun,
        }
    }
}

impl eframe::App for SleepyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The root ui has no margin or background; a panel supplies both.
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("sleepy");
            // The skeleton shows what it knows; the next commit turns
            // these facts into widgets the user can act on.
            let mode = self
                .modes
                .get(self.selected)
                .map(|mode| mode.label())
                .unwrap_or("nothing");
            ui.label(format!("selected mode: {mode}"));
            ui.label(format!("the field would say: {}", self.input));
            ui.label(format!("the timer is {:?}", self.timer.status()));
            ui.label(format!("backend: {:?}", self.backend));
        });
    }
}
