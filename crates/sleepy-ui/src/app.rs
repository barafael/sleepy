//! What the window knows, apart from how it looks.
//!
//! The same plain fields the terminal keeps - the modes on offer, the
//! typed duration, the timer - plus the commands the keyboard and the
//! clicked key hints produce. Decisions are delegated to sleepy-core; this
//! only holds the answers the screen will show and turns commands into
//! calls. The countdown still needs a heartbeat: egui only repaints when
//! asked, and a window nobody is looking at must still fall asleep on time.

use crate::command::{self, Command};
use sleepy_core::{
    Backend, Error, KeepAwake, SleepAction, Status, Timer, TimerEvent, detect, execute,
    parse_duration,
};
use std::time::{Duration, Instant};

/// The duration offered to a user who types nothing.
const DEFAULT: &str = "30 min";

/// What one `-1 min` or `+1 min` is worth, in seconds.
pub const NUDGE: i64 = 60;

/// The heartbeat: a countdown that shows whole seconds needs four looks
/// a second, no more.
const TICK: Duration = Duration::from_millis(250);

/// The whole application state.
pub struct SleepyApp {
    /// Cleared when the user asks to leave.
    running: bool,
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
    /// Whether to hold the screen awake while the countdown runs.
    pub keep_screen_on: bool,
    /// The inhibition itself, held while a countdown runs with
    /// `keep_screen_on`; closing the window lets it go.
    awake: KeepAwake,
    /// The action a finished countdown ran, to be shown in the window.
    pub fired: Option<SleepAction>,
    /// Why the machine is still awake after a fired countdown, if it is.
    pub failure: Option<Error>,
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
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        backend: Backend,
        keep_screen_on: bool,
        duration: Option<String>,
        mode: Option<SleepAction>,
    ) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let modes = detect();
        let selected = mode
            .and_then(|want| modes.iter().position(|have| *have == want))
            .unwrap_or(0);
        let arm = duration.is_some();
        let mut app = Self {
            running: true,
            modes,
            selected,
            input: duration.unwrap_or_else(|| DEFAULT.into()),
            timer: Timer::new(),
            backend,
            keep_screen_on,
            awake: KeepAwake::new(),
            fired: None,
            failure: None,
            pressed: None,
            focus_field: true,
            ctx: cc.egui_ctx.clone(),
            #[cfg(target_os = "linux")]
            system_theme: crate::portal::SystemTheme::watch(cc.egui_ctx.clone()),
            window: None,
        };
        if arm {
            app.arm_selected();
        }
        app
    }

    /// Remembers the window's geometry between runs.
    pub fn with_window(mut self, tracker: crate::geometry::Tracker) -> Self {
        self.window = Some(tracker);
        self
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
                let armed = self.timer.arm(action, duration);
                if armed && self.keep_screen_on {
                    self.awake.start();
                }
                armed
            }
            _ => false,
        }
    }

    /// Switches between really sleeping and only saying so.
    pub fn toggle_backend(&mut self) {
        self.backend = match self.backend {
            Backend::Real => Backend::DryRun,
            Backend::DryRun => Backend::Real,
        };
    }

    /// Switches the screen inhibition, applying the change to a running
    /// countdown: flipping it on mid-count grabs the inhibition now, and
    /// flipping it off lets go.
    pub fn toggle_keep_screen_on(&mut self) {
        self.keep_screen_on = !self.keep_screen_on;
        if matches!(self.timer.status(), Status::Armed | Status::Paused) {
            if self.keep_screen_on {
                self.awake.start();
            } else {
                self.awake.stop();
            }
        }
    }

    /// Moves the highlight, wrapping at both ends.
    fn move_selection(&mut self, step: isize) {
        let count = isize::try_from(self.modes.len()).unwrap_or(0);
        if count == 0 {
            return;
        }
        let next = (isize::try_from(self.selected).unwrap_or(0) + step).rem_euclid(count);
        self.selected = usize::try_from(next).unwrap_or(0);
    }

    /// Gives up on the countdown; the machine stays awake.
    pub fn abort(&mut self) {
        self.timer.abort();
        self.awake.stop();
        self.fired = None;
        self.failure = None;
    }

    /// Runs one command, from a key press or a clicked key hint, and notes
    /// the moment so the hint can light up.
    fn run(&mut self, command: Command) {
        self.pressed = Some((command, self.ctx.input(|i| i.time)));
        let counting = matches!(self.timer.status(), Status::Armed | Status::Paused);
        match command {
            Command::PreviousMode => self.move_selection(-1),
            Command::NextMode => self.move_selection(1),
            Command::ToggleDryRun => self.toggle_backend(),
            Command::ToggleKeepAwake => self.toggle_keep_screen_on(),
            Command::Arm => {
                self.arm_selected();
            }
            Command::PauseResume => match self.timer.status() {
                Status::Armed => self.timer.pause(),
                Status::Paused => self.timer.resume(),
                Status::Idle | Status::Fired => {}
            },
            Command::Escape => {
                // Escape first saves the machine from sleep, and only a
                // second press closes the window.
                if counting {
                    self.abort();
                } else {
                    self.running = false;
                }
            }
            Command::Longer => self.timer.adjust(NUDGE),
            Command::Shorter => self.timer.adjust(-NUDGE),
            Command::Quit => self.running = false,
        }
    }

    /// Does the thing the countdown was armed for. On a real sleep this
    /// call does not return until the machine wakes; the dry run answers
    /// at once, and the window is there to say so.
    fn perform(&mut self) {
        // Whatever happens next, the screen is on its own from here.
        self.awake.stop();
        if let Some(action) = self.fired {
            self.failure = execute(action, self.backend).err();
        }
    }
}

impl eframe::App for SleepyApp {
    /// Remembers the window's geometry. Not in eframe's storage: the
    /// geometry tracker writes its own file, where the terminal sleepy's
    /// settings already live and where the XWayland shift can be undone.
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
        if matches!(self.timer.status(), Status::Armed | Status::Paused) {
            ctx.request_repaint_after(TICK);
        }
        if self.timer.tick(Instant::now()) == TimerEvent::Fired {
            self.fired = Some(
                self.timer
                    .action()
                    .expect("a fired countdown knows what it is doing"),
            );
            self.perform();
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
        if !self.running {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if let Some(command) = crate::ui::render(self, ui) {
            self.run(command);
        }
    }
}
