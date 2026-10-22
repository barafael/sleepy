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
use sleepy_core::{SleepAction, Status, When, format_clock};
use std::time::Duration;

pub fn draw(frame: &mut Frame, app: &App) {
    let [modes, when, countdown, footer] = Layout::vertical([
        Constraint::Length(4 + u16::try_from(app.controller.modes.len()).unwrap_or(4)),
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
        app.controller
            .modes
            .iter()
            .map(|action| ListItem::from(action.label())),
    )
    .highlight_symbol("> ")
    .highlight_style(Style::new().bold().light_yellow())
    .block(Block::bordered().title(" mode (up/down) "));
    let mut state = ListState::default().with_selected(Some(app.controller.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_when(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" when (45s, 30 min, 1h30m, 23:45) ");
    let [field, hint] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(block.inner(area));
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(app.controller.input.as_str()), field);
    // The cursor sits after the text, so the field feels like a field.
    frame.set_cursor_position(Position::new(
        field.x + u16::try_from(app.controller.input.chars().count()).unwrap_or(0),
        field.y,
    ));
    match app.controller.when() {
        // The field echoes its own meaning in the grammar it was typed
        // in: a duration counts off a stretch of time, a clock time
        // names the moment it walks up to.
        Ok(When::After(duration)) => frame.render_widget(
            Paragraph::new(format!("-> in {}", format_duration(duration)).dark_gray()),
            hint,
        ),
        Ok(When::At(time)) => frame.render_widget(
            Paragraph::new(format!("-> at {}", time.strftime("%H:%M")).dark_gray()),
            hint,
        ),
        Err(error) => frame.render_widget(Paragraph::new(error.to_string().red()), hint),
    }
}

fn draw_countdown(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" countdown ");
    let [headline, bar] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
        .margin(1)
        .areas(block.inner(area));
    frame.render_widget(block, area);
    match app.controller.timer.status() {
        Status::Idle => {
            let text = match app.controller.selected_action() {
                Some(action) => format!("press enter to arm the {} countdown", action.label()),
                None => "this machine offers nothing to arm".to_string(),
            };
            frame.render_widget(Paragraph::new(text.dark_gray()), headline);
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
                .unwrap_or(SleepAction::Suspend)
                .label()
                .to_lowercase();
            let text = if app.controller.timer.status() == Status::Paused {
                format!("paused, {left} left - press space to resume")
            } else {
                match app.controller.deadline() {
                    // The wall clock answers the question a countdown
                    // only asks in remainders: when, exactly.
                    Some(at) => format!("{action} in {left}, at {}", format_clock(at)),
                    None => format!("{action} in {left}"),
                }
            };
            frame.render_widget(Paragraph::new(text.bold()), headline);
            let gauge = Gauge::default()
                .ratio(f64::from(app.controller.timer.progress()))
                .label(left)
                .gauge_style(Style::new().light_yellow());
            frame.render_widget(gauge, bar);
        }
        Status::Fired => {
            let action = app
                .controller
                .timer
                .action()
                .unwrap_or(SleepAction::Suspend)
                .firing();
            frame.render_widget(
                Paragraph::new(format!("time! {action}...").bold()),
                headline,
            );
        }
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let keys = [
        ("up/down", "mode"),
        ("ctrl+d", "dry run"),
        ("k", "keep awake"),
        ("enter", "arm"),
        ("space", "pause"),
        ("esc", "abort"),
        ("+/-", "minute"),
        ("q", "quit"),
    ];
    let mut spans = Vec::new();
    for (key, what) in keys {
        spans.push(Span::from(" ").dark_gray());
        match what.split_once(key) {
            // A one-letter key that starts its word is shown inside the
            // word - quit with the q picked out - because "qquit" reads
            // like a hiccup.
            Some(("", rest)) => {
                spans.push(Span::from(key).bold().underlined());
                spans.push(Span::from(rest).dark_gray());
            }
            _ => {
                spans.push(Span::from(key).bold());
                spans.push(Span::from(format!(" {what}")).dark_gray());
            }
        }
    }
    if app.controller.backend == sleepy_core::Backend::DryRun {
        spans.push(
            Span::from(" DRY RUN: the machine stays awake ")
                .yellow()
                .bold(),
        );
    }
    if app.controller.keep_screen_on {
        spans.push(Span::from(" keeping the screen awake ").green().bold());
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
// A test that cannot get its answer is broken, not a machine that said no.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use sleepy_core::{Backend, Command, Options};

    /// An app built by hand, so the tests never ask the machine what it
    /// can do (`detect` would spawn processes here).
    fn app() -> App {
        App::new(Options {
            backend: Backend::Real,
            keep_screen_on: false,
            when: None,
            mode: None,
            modes: vec![SleepAction::Suspend],
        })
    }

    /// One frame, rendered to a screen of memory, as text. The pixels
    /// stay the human's; the words are ours to check. Wide on purpose:
    /// the footer carries every badge at once in one test, and a
    /// Paragraph clips what overflows.
    fn screen_of(app: &App) -> String {
        let mut terminal = ratatui::Terminal::new(TestBackend::new(160, 20)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area().height)
            .map(|y| {
                (0..buffer.area().width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }

    #[test]
    fn the_footer_promises_the_keys_it_keeps() {
        let screen = screen_of(&app());
        for promise in [
            "ctrl+d dry run",
            // The one-letter keys live inside their words: the k of
            // "keep awake" and the q of "quit" are the underlined
            // letters, not separate columns.
            "keep awake",
            "enter arm",
            "space pause",
            "esc abort",
            "quit",
        ] {
            assert!(screen.contains(promise), "{promise} missing:\n{screen}");
        }
        // The badges are earned, not assumed.
        assert!(!screen.contains("DRY RUN"));
        assert!(!screen.contains("keeping the screen awake"));
    }

    #[test]
    fn the_badges_appear_with_the_choices() {
        let mut app = app();
        app.controller.apply(Command::ToggleDryRun);
        app.controller.apply(Command::ToggleKeepAwake);
        let screen = screen_of(&app);
        assert!(
            screen.contains("DRY RUN: the machine stays awake"),
            "{screen}"
        );
        assert!(screen.contains("keeping the screen awake"), "{screen}");
    }

    #[test]
    fn the_field_echoes_both_grammars() {
        let mut app = app();
        app.controller.input.clear();
        type_text(&mut app, "30 min");
        let screen = screen_of(&app);
        assert!(screen.contains("-> in "), "duration echo:\n{screen}");
        app.controller.input.clear();
        type_text(&mut app, "23:45");
        let screen = screen_of(&app);
        assert!(screen.contains("-> at 23:45"), "clock echo:\n{screen}");
    }

    #[test]
    fn an_idle_screen_offers_the_arming_and_the_grammar() {
        let screen = screen_of(&app());
        assert!(
            screen.contains("press enter to arm the suspend (to RAM) countdown"),
            "{screen}"
        );
        // The default field says 30 min, and the echo says what it means.
        assert!(screen.contains("-> in "), "default echo:\n{screen}");
        // The field's title shows both grammars, clock included.
        assert!(screen.contains("23:45)"), "field title:\n{screen}");
    }

    #[test]
    fn an_armed_countdown_shows_the_clock_its_deadline() {
        let mut app = app();
        app.controller.input = "45s".into();
        assert!(app.controller.arm_selected());
        let screen = screen_of(&app);
        // "suspend (to ram) in 45s, at 23:45" - the remaining time and
        // the wall clock's answer, whatever time of day this test runs.
        let at = screen
            .split("suspend (to ram) in 45s, at ")
            .nth(1)
            .and_then(|rest| rest.get(..5))
            .unwrap_or_default();
        let digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
        assert!(
            at.len() == 5 && digits(&at[..2]) && at.as_bytes()[2] == b':' && digits(&at[3..]),
            "no clock time after the headline:\n{screen}"
        );
    }

    #[test]
    fn a_paused_countdown_promises_no_deadline() {
        let mut app = app();
        app.controller.input = "45s".into();
        assert!(app.controller.arm_selected());
        app.controller.apply(Command::PauseResume);
        let screen = screen_of(&app);
        assert!(screen.contains("paused, 45s left"), "headline:\n{screen}");
        // A paused countdown has no deadline: it would drift with the
        // pause, and the screen must not promise a sliding moment.
        assert!(!screen.contains(", at "), "paused headline:\n{screen}");
    }
}
