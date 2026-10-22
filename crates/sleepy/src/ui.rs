//! Drawing: one function, one frame, from the app's state to the terminal.
//!
//! The screen is three blocks stacked - the modes the machine offers, the
//! duration field, the countdown - over a footer of key hints. Nothing here
//! decides anything; it only shows what `App` knows.

use crate::app::App;
use humantime::format_duration;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Gauge, List, ListItem, ListState, Paragraph},
};
use sleepy_core::{SleepAction, Status};
use std::time::Duration;

pub fn draw(frame: &mut Frame, app: &App) {
    let [modes, when, countdown, footer] = Layout::vertical([
        Constraint::Length(4 + u16::try_from(app.modes.len()).unwrap_or(4)),
        Constraint::Length(4),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_modes(frame, app, modes);
    draw_when(frame, app, when);
    draw_countdown(frame, app, countdown);
    draw_footer(frame, app, footer);
}

fn draw_modes(frame: &mut Frame, app: &App, area: Rect) {
    let list = List::new(
        app.modes
            .iter()
            .map(|action| ListItem::from(action.label())),
    )
    .highlight_symbol("> ")
    .highlight_style(Style::new().bold().light_yellow())
    .block(Block::bordered().title(" mode (up/down) "));
    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_when(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" when (humantime: 45s, 30 min, 1h30m) ");
    let [field, hint] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(block.inner(area));
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(app.input.as_str()), field);
    // The cursor sits after the text, so the field feels like a field.
    frame.set_cursor_position(Position::new(
        field.x + u16::try_from(app.input.chars().count()).unwrap_or(0),
        field.y,
    ));
    match app.duration() {
        Ok(duration) => frame.render_widget(
            Paragraph::new(format!("-> in {}", format_duration(duration)).dark_gray()),
            hint,
        ),
        Err(error) => frame.render_widget(Paragraph::new(error.hint().red()), hint),
    }
}

fn draw_countdown(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" countdown ");
    let [headline, bar] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
        .margin(1)
        .areas(block.inner(area));
    frame.render_widget(block, area);
    match app.timer.status() {
        Status::Idle => {
            let text = match app.selected_action() {
                Some(action) => format!("press enter to arm the {} countdown", action.label()),
                None => "this machine offers nothing to arm".to_string(),
            };
            frame.render_widget(Paragraph::new(text.dark_gray()), headline);
        }
        Status::Armed | Status::Paused => {
            // Whole seconds: a countdown that jitters in microseconds is
            // noise, not information.
            let left =
                format_duration(Duration::from_secs(app.timer.remaining().as_secs())).to_string();
            let action = app
                .timer
                .action()
                .unwrap_or(SleepAction::Suspend)
                .label()
                .to_lowercase();
            let text = if app.timer.status() == Status::Paused {
                format!("paused, {left} left - press space to resume")
            } else {
                format!("{action} in {left}")
            };
            frame.render_widget(Paragraph::new(text.bold()), headline);
            let gauge = Gauge::default()
                .ratio(f64::from(app.timer.progress()))
                .label(left)
                .gauge_style(Style::new().light_yellow());
            frame.render_widget(gauge, bar);
        }
        Status::Fired => {
            let action = app
                .timer
                .action()
                .unwrap_or(SleepAction::Suspend)
                .label()
                .to_lowercase();
            frame.render_widget(
                Paragraph::new(format!("time! {action} the machine...").bold()),
                headline,
            );
        }
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let keys = [
        ("up/down", "mode"),
        ("enter", "arm"),
        ("space", "pause"),
        ("esc", "abort"),
        ("+/-", "minute"),
        ("q", "quit"),
    ];
    let mut spans = keys
        .into_iter()
        .flat_map(|(key, what)| {
            [
                Span::from(" ").dark_gray(),
                Span::from(key).bold(),
                Span::from(format!("{what} ")).dark_gray(),
            ]
        })
        .collect::<Vec<_>>();
    if app.backend == sleepy_core::Backend::DryRun {
        spans.push(
            Span::from(" DRY RUN: the machine stays awake ")
                .yellow()
                .bold(),
        );
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
