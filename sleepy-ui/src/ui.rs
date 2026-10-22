//! Drawing: the terminal's screen, transliterated to widgets.
//!
//! The same three blocks the terminal stacks - the modes on offer, the
//! duration field, the countdown - in the same left-to-right order the
//! choices are made, over a status bar of key hints. The hints are the
//! terminal's footer: keycap and word, clickable, lighting up when their
//! command runs; and the blocks wear the accent bar that says where the
//! keyboard currently goes.

use crate::app::SleepyApp;
use crate::command;
use crate::theme;
use egui::{Align, Color32, CursorIcon, Label, Layout, Response, RichText, Sense, TextStyle, Ui};
use humantime::format_duration;
use sleepy_core::{Backend, Command, Status, When, describe, format_clock};
use std::time::Duration;

/// Horizontal gap between unrelated items.
const GAP: f32 = 10.0;

/// How long a key hint lights up after its command runs, in seconds.
const PULSE: f64 = 0.4;

/// The duration field's id: focus here means the keyboard is typing, not
/// giving commands.
pub fn field_id() -> egui::Id {
    egui::Id::unique("when")
}

/// Panel frame without padding (the default panel frames add 8 px).
fn flat_panel(ui: &Ui) -> egui::Frame {
    egui::Frame::NONE.fill(ui.visuals().panel_fill)
}

/// Renders the whole window; returns the command of a clicked key hint.
pub fn render(app: &mut SleepyApp, ui: &mut Ui) -> Option<Command> {
    let typing = ui.ctx().memory(|m| m.focused() == Some(field_id()));
    egui::Panel::top("header")
        .frame(flat_panel(ui))
        .show_separator_line(false)
        .show(ui, |ui| render_header(app, ui));
    let status = egui::Panel::bottom("status")
        .frame(flat_panel(ui))
        .show(ui, |ui| render_status(app, ui, typing));
    egui::CentralPanel::default()
        .frame(flat_panel(ui))
        .show(ui, |ui| render_body(app, ui, typing));
    status.inner
}

/// A full-width bar titling a block; filled with the accent when the
/// keyboard is busy with that block.
fn section_bar(ui: &mut Ui, title: &str, right: &str, focused: bool) {
    let p = theme::of(ui);
    let height = ui.text_style_height(&TextStyle::Heading) + 8.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
    let (bg, fg, sub) = if focused {
        (p.accent, p.on_accent, p.on_accent)
    } else {
        (p.surface, p.strong, p.weak)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, bg);
    painter.text(
        rect.left_center() + egui::vec2(GAP, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        TextStyle::Heading.resolve(ui.style()),
        fg,
    );
    painter.text(
        rect.right_center() - egui::vec2(GAP, 0.0),
        egui::Align2::RIGHT_CENTER,
        right,
        TextStyle::Body.resolve(ui.style()),
        sub,
    );
}

fn render_header(app: &SleepyApp, ui: &mut Ui) {
    let p = theme::of(ui);
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(GAP as i8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("sleepy").heading().strong().color(p.accent));
                ui.weak(env!("CARGO_PKG_VERSION"));
                ui.add_space(GAP);
                ui.label(
                    RichText::new("Put the machine to sleep after a while.")
                        .italics()
                        .color(p.weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (word, color) = match app.controller.timer.status() {
                        Status::Idle if app.controller.modes.is_empty() => ("stuck", p.red),
                        Status::Idle => ("idle", p.weak),
                        Status::Armed => ("counting", p.green),
                        Status::Paused => ("paused", p.yellow),
                        Status::Fired => ("fired", p.accent),
                    };
                    ui.label(RichText::new(word).monospace().color(color));
                });
            });
        });
}

/// The body: mode, when, and the toggles and buttons that follow them,
/// left to right in the order the choices are made; the countdown below,
/// full width, because it reports on all of it.
fn render_body(app: &mut SleepyApp, ui: &mut Ui, typing: bool) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(GAP as i8, 4))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    // Up and down drive the mode list whenever the keyboard
                    // is not typing, so that is when the bar lights up.
                    section_bar(ui, "mode", "", !typing);
                    draw_modes(app, ui);
                });
                ui.add_space(GAP);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    section_bar(ui, "when", "", typing);
                    draw_when(app, ui);
                    ui.add_space(4.0);
                    draw_toggles(app, ui);
                    ui.add_space(4.0);
                    draw_buttons(app, ui);
                });
            });
            ui.add_space(4.0);
            let counting = matches!(
                app.controller.timer.status(),
                Status::Armed | Status::Paused
            );
            section_bar(ui, "countdown", "", counting);
            draw_countdown(app, ui);
        });
}

fn draw_modes(app: &mut SleepyApp, ui: &mut Ui) {
    let p = theme::of(ui);
    if app.controller.modes.is_empty() {
        ui.label(
            RichText::new("this machine offers nothing to arm")
                .color(p.red)
                .strong(),
        );
        return;
    }
    // A list, not a row of chips: the terminal highlights one line at a
    // time, and up and down move that highlight.
    for (index, action) in app.controller.modes.iter().enumerate() {
        let selected = index == app.controller.selected;
        let fill = if selected {
            p.accent
        } else if index % 2 == 1 {
            p.stripe
        } else {
            Color32::TRANSPARENT
        };
        let marker = if selected { ">" } else { " " };
        let text = RichText::new(format!("{marker} {}", action.label()))
            .monospace()
            .color(if selected { p.on_accent } else { p.text });
        let text = if selected { text.strong() } else { text };
        let row = egui::Frame::NONE
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(Label::new(text).sense(Sense::CLICK).selectable(false))
            })
            .inner
            .on_hover_cursor(CursorIcon::PointingHand);
        if row.hovered() && !selected {
            ui.painter().hline(
                row.rect.x_range(),
                row.rect.bottom() - 1.0,
                egui::Stroke::new(1.0, p.text),
            );
        }
        if row.clicked() {
            app.controller.selected = index;
        }
    }
}

fn draw_when(app: &mut SleepyApp, ui: &mut Ui) {
    let p = theme::of(ui);
    let field = egui::TextEdit::singleline(&mut app.controller.input)
        .id(field_id())
        .hint_text("45s, 30 min, 1h30m, 23:45")
        .font(TextStyle::Monospace)
        .desired_width(f32::INFINITY);
    let response = ui.add(field);
    // Like the terminal, where the keystrokes go to the field from the
    // first frame; after that the user's focus is their own.
    if app.focus_field {
        response.request_focus();
        app.focus_field = false;
    }
    match app.controller.when() {
        // The field echoes its own meaning in the grammar it was typed
        // in, as it does in the terminal.
        Ok(When::After(duration)) => {
            ui.label(RichText::new(format!("-> in {}", format_duration(duration))).color(p.weak));
        }
        Ok(When::At(time)) => {
            ui.label(RichText::new(format!("-> at {}", time.format("%H:%M"))).color(p.weak));
        }
        Err(error) => {
            ui.label(RichText::new(error.to_string()).color(p.red));
        }
    }
}

/// A `[x]` toggle: monospace, accent when on, the info-bar style of the
/// reference windows.
fn toggle(ui: &mut Ui, name: &str, on: bool, tip: &str) -> Response {
    let p = theme::of(ui);
    let box_ = if on { "x" } else { " " };
    let color = if on { p.accent } else { p.weak };
    ui.add(
        Label::new(
            RichText::new(format!("[{box_}] {name}"))
                .monospace()
                .color(color),
        )
        .sense(Sense::CLICK)
        .selectable(false),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
    .on_hover_text(tip)
}

fn draw_toggles(app: &mut SleepyApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let dry = app.controller.backend == Backend::DryRun;
        if toggle(
            ui,
            "dry run",
            dry,
            "count down for real, but only say what would run",
        )
        .clicked()
        {
            app.controller.toggle_backend();
        }
        if toggle(
            ui,
            "keep screen on",
            app.controller.keep_screen_on,
            "hold off screen blanking while the countdown runs",
        )
        .clicked()
        {
            app.controller.toggle_keep_screen_on();
        }
    });
}

fn draw_buttons(app: &mut SleepyApp, ui: &mut Ui) {
    let armed = app.controller.timer.status() == Status::Armed;
    let paused = app.controller.timer.status() == Status::Paused;
    let counting = armed || paused;
    ui.horizontal(|ui| {
        // Arming waits for an idle (or finished) countdown: a running
        // one is never silently replaced.
        let can_arm = !counting
            && app.controller.selected_action().is_some()
            && app.controller.when().is_ok();
        if ui
            .add_enabled(can_arm, egui::Button::new("arm"))
            .on_hover_text("start the countdown (or press enter)")
            .clicked()
        {
            app.controller.arm_selected();
        }
        if ui.add_enabled(armed, egui::Button::new("pause")).clicked() {
            app.controller.timer.pause();
        }
        if ui
            .add_enabled(paused, egui::Button::new("resume"))
            .clicked()
        {
            app.controller.timer.resume();
        }
        if ui
            .add_enabled(counting, egui::Button::new("abort"))
            .on_hover_text("back to idle; the machine stays awake")
            .clicked()
        {
            app.controller.abort();
        }
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(counting, egui::Button::new("-1 min"))
            .clicked()
        {
            app.controller.timer.adjust(-sleepy_core::NUDGE);
        }
        if ui
            .add_enabled(counting, egui::Button::new("+1 min"))
            .clicked()
        {
            app.controller.timer.adjust(sleepy_core::NUDGE);
        }
    });
}

fn draw_countdown(app: &mut SleepyApp, ui: &mut Ui) {
    let p = theme::of(ui);
    match app.controller.timer.status() {
        Status::Idle => {
            ui.weak(match app.controller.selected_action() {
                Some(mode) => format!("press enter to arm the {} countdown", mode.label()),
                None => "nothing to arm".to_string(),
            });
        }
        Status::Armed | Status::Paused => {
            // Whole seconds: a countdown that jitters in microseconds is
            // noise, not information.
            let left = format_duration(Duration::from_secs(
                app.controller.timer.remaining().as_secs(),
            ))
            .to_string();
            let action = app
                .controller
                .timer
                .action()
                .map_or(String::from("sleep"), |mode| mode.label().to_lowercase());
            let headline = if app.controller.timer.status() == Status::Paused {
                format!("paused, {left} left - press space to resume")
            } else {
                match app.controller.deadline() {
                    // The wall clock answers the question a countdown
                    // only asks in remainders: when, exactly.
                    Some(at) => format!("{action} in {left}, at {}", format_clock(at)),
                    None => format!("{action} in {left}"),
                }
            };
            ui.label(RichText::new(headline).strong().color(p.strong));
            ui.add(egui::ProgressBar::new(app.controller.timer.progress()).text(&left));
        }
        Status::Fired => {
            let action = app
                .controller
                .fired
                .map_or(String::from("sleep"), |mode| mode.label().to_lowercase());
            ui.label(RichText::new(format!("time! {action} the machine...")).size(24.0));
            if app.controller.backend == Backend::DryRun
                && let Some(command) = app.controller.fired.and_then(describe)
            {
                ui.label(RichText::new(format!("would have run: {command}")).color(p.weak));
            }
        }
    }
}

/// The status bar: the open typing, the standing warnings, and the key
/// hints. A hint lights up when its command runs and, if it stands for one
/// command, can be clicked to run it.
fn render_status(app: &SleepyApp, ui: &mut Ui, typing: bool) -> Option<Command> {
    let p = theme::of(ui);
    let now = ui.input(|i| i.time);
    let counting = matches!(
        app.controller.timer.status(),
        Status::Armed | Status::Paused
    );
    let mut clicked = None;
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(GAP as i8, 4))
        .show(ui, |ui| {
            if let Some(error) = &app.controller.failure {
                ui.label(RichText::new(error.to_string()).color(p.red).strong());
            }
            ui.horizontal_wrapped(|ui| {
                // The field, echoed while it has the keyboard, so the
                // window shows what is being typed rather than left to
                // the imagination.
                if typing {
                    ui.label(RichText::new("typing:").color(p.label));
                    ui.label(
                        RichText::new(format!(" {}▏ ", app.controller.input))
                            .monospace()
                            .color(p.on_accent)
                            .background_color(p.accent),
                    );
                    ui.add_space(GAP * 2.0);
                }
                if app.controller.backend == Backend::DryRun {
                    ui.label(
                        RichText::new(" DRY RUN: the machine stays awake ")
                            .strong()
                            .color(p.yellow),
                    );
                }
                if app.controller.keep_screen_on {
                    ui.label(
                        RichText::new(" keeping the screen awake ")
                            .strong()
                            .color(p.green),
                    );
                }
                for hint in command::Hint::all(counting) {
                    let pulse = app
                        .pressed
                        .filter(|(pressed, _)| Some(*pressed) == hint.command)
                        .map_or(0.0, |(_, at)| 1.0 - ((now - at) / PULSE).clamp(0.0, 1.0));
                    if pulse > 0.0 {
                        ui.ctx().request_repaint();
                    }
                    // Clicking stands in for the key, so not while typing:
                    // a stray click would steal the field's focus.
                    let clickable = hint.command.is_some() && !typing;
                    if key_hint(ui, hint.keys, hint.label, pulse as f32, clickable)
                        && let Some(command) = hint.command
                    {
                        clicked = Some(command);
                    }
                    ui.add_space(GAP);
                }
            });
        });
    clicked
}

/// A clickable key cap followed by its description.
fn key_hint(ui: &mut Ui, keys: &str, description: &str, pulse: f32, clickable: bool) -> bool {
    let p = theme::of(ui);
    let sense = if clickable {
        Sense::CLICK
    } else {
        Sense::hover()
    };
    let key = ui.add(
        Label::new(
            RichText::new(format!(" {keys} "))
                .monospace()
                .color(p.accent.lerp_to_gamma(p.on_accent, pulse))
                .background_color(p.hover.lerp_to_gamma(p.accent, pulse)),
        )
        .sense(sense),
    );
    let text = ui.add(Label::new(RichText::new(description).color(p.text)).sense(sense));
    if clickable {
        if key.hovered() || text.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            ui.painter().hline(
                text.rect.x_range(),
                text.rect.bottom() - 1.0,
                egui::Stroke::new(1.0, p.text),
            );
        }
        return key.clicked() || text.clicked();
    }
    false
}
