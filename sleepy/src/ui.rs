//! Drawing: one function, one frame, from the app's state to the terminal.
//!
//! The screen is three blocks stacked - the modes the machine offers, the
//! duration field, the countdown - over a footer of key hints. Nothing here
//! decides anything; it only shows what `App` knows.

use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};
use sleepy_core::Status;

pub fn draw(frame: &mut Frame, app: &App) {
    let [modes, when, countdown, footer] = Layout::vertical([
        // Two borders, one line per mode, one line explaining the chosen one.
        Constraint::Length(mode_rows(app).saturating_add(3)),
        Constraint::Length(4),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_modes(frame, app, modes);
    draw_when(frame, app, when);
    draw_countdown(frame, app, countdown);
    draw_footer(frame, app, footer);
}

/// One line per mode the machine offers.
fn mode_rows(app: &App) -> u16 {
    u16::try_from(app.controller.modes.len()).unwrap_or(u16::MAX)
}

fn draw_modes(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" mode (up/down) ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [list_area, help] =
        Layout::vertical([Constraint::Length(mode_rows(app)), Constraint::Length(1)]).areas(inner);
    let list = List::new(
        app.controller
            .modes
            .iter()
            .map(|action| ListItem::from(action.label())),
    )
    .highlight_symbol("> ")
    .highlight_style(Style::new().bold().light_yellow());
    let mut state = ListState::default().with_selected(Some(app.controller.selected));
    frame.render_stateful_widget(list, list_area, &mut state);
    // The highlighted mode explains itself, so a name like "hybrid
    // sleep" is chosen knowing what it does.
    if let Some(action) = app.controller.selected_action() {
        // Indented past the marker column, so it reads as belonging to
        // the highlighted line above it.
        frame.render_widget(
            Paragraph::new(format!("  {}", action.help()))
                .dark_gray()
                .italic(),
            help,
        );
    }
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
        // in, in the words both frontends share.
        Ok(when) => frame.render_widget(Paragraph::new(when.echo().dark_gray()), hint),
        Err(error) => frame.render_widget(Paragraph::new(error.to_string().red()), hint),
    }
}

fn draw_countdown(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" countdown ");
    let inner = block.inner(area);
    // The margin is a courtesy: on a screen too short for it, the words
    // matter more than the breathing room.
    let [headline, bar] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
        .margin(u16::from(inner.height >= 4))
        .areas(inner);
    frame.render_widget(block, area);
    match app.controller.timer.status() {
        Status::Idle => frame.render_widget(
            Paragraph::new(app.controller.headline()).dark_gray(),
            headline,
        ),
        Status::Armed | Status::Paused => {
            frame.render_widget(Paragraph::new(app.controller.headline().bold()), headline);
            draw_progress(
                frame,
                bar,
                app.controller.timer.progress(),
                &app.controller.left(),
            );
        }
        Status::Fired => {
            frame.render_widget(Paragraph::new(app.controller.headline().bold()), headline)
        }
    }
}

/// The progress bar, drawn with background colors rather than block
/// glyphs: a font whose `█` leaves a hairline between cells turns a row
/// of them into a checkerboard, and a filled background is solid in
/// every font. The time left rides the middle of the bar - dark text on
/// the fill, bright text cut out of it - as the gauge used to place it.
fn draw_progress(frame: &mut Frame, area: Rect, ratio: f32, left: &str) {
    frame.render_widget(Paragraph::new(progress_line(area.width, ratio, left)), area);
}

/// The bar as one line, cell by cell: each cell takes the fill's colors
/// or the track's by where it sits, label or not, so the label never
/// punches a hole in the fill.
fn progress_line(width: u16, ratio: f32, left: &str) -> Line<'static> {
    let width = usize::from(width);
    let filled = (width as f64 * f64::from(ratio.clamp(0.0, 1.0))).round() as usize;
    let label: Vec<char> = left.chars().take(width).collect();
    let start = (width - label.len()) / 2;
    Line::from(
        (0..width)
            .map(|column| {
                let text = column
                    .checked_sub(start)
                    .and_then(|i| label.get(i))
                    .map_or(' ', |&c| c);
                let cell = Span::from(text.to_string()).bold();
                if column < filled {
                    cell.black().on_light_yellow()
                } else {
                    cell.light_yellow().on_black()
                }
            })
            .collect::<Vec<_>>(),
    )
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let keys = [
        ("up/down", "mode"),
        ("ctrl+d", "dry run"),
        ("ctrl+k", "keep awake"),
        ("enter", "arm"),
        ("space", "pause"),
        ("esc", "abort"),
        ("+/-", "minute"),
        ("q", "quit"),
    ];
    let mut spans = Vec::new();
    for (key, what) in keys {
        spans.push(Span::from(" ").dark_gray());
        // A one-letter key that starts its word is shown inside the
        // word - quit with the q picked out - because "qquit" reads
        // like a hiccup.
        if let Some(("", rest)) = what.split_once(key) {
            spans.push(Span::from(key).bold().underlined());
            spans.push(Span::from(rest).dark_gray());
        } else {
            spans.push(Span::from(key).bold());
            spans.push(Span::from(format!(" {what}")).dark_gray());
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
    use sleepy_core::{Backend, Command, Options, SleepAction};

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
            "ctrl+k keep awake",
            "enter arm",
            "space pause",
            "esc abort",
            // The one-letter key lives inside its word: the q of
            // "quit" is the underlined letter, not a separate column.
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
    fn the_highlighted_mode_explains_itself() {
        let mut app = app();
        app.controller.modes = vec![SleepAction::Suspend, SleepAction::Hibernate];
        let screen = screen_of(&app);
        assert!(
            screen.contains("sleeps now, wakes fast"),
            "suspend help:\n{screen}"
        );
        app.controller.selected = 1;
        let screen = screen_of(&app);
        assert!(
            screen.contains("memory to disk, power off"),
            "hibernate help:\n{screen}"
        );
    }

    #[test]
    fn the_progress_bar_is_solid_and_says_the_time_left() {
        let mut app = app();
        app.controller.input = "45s".into();
        assert!(app.controller.arm_selected());
        let screen = screen_of(&app);
        // Block glyphs leave a hairline between cells in many fonts, and
        // a row of them reads as a checkerboard; the bar is backgrounds now.
        assert!(!screen.contains('█'), "block glyphs:\n{screen}");
        // The bar sits under the headline and carries the time left.
        let headline = screen
            .lines()
            .position(|line| line.contains("suspend (to ram) in 45s"))
            .unwrap();
        assert!(
            screen
                .lines()
                .nth(headline + 1)
                .is_some_and(|line| line.contains("45s")),
            "bar label:\n{screen}"
        );
    }

    #[test]
    fn the_label_takes_the_fill_colors_where_the_fill_has_reached() {
        use ratatui::style::Color;
        // Most of the way through: the fill runs under the whole label.
        let line = progress_line(20, 0.8, "12s");
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text.trim(), "12s");
        let start = text.find("12s").unwrap();
        for (column, span) in line.spans.iter().enumerate() {
            let on_fill = column < 16;
            let expected = if on_fill {
                Color::LightYellow
            } else {
                Color::Black
            };
            assert_eq!(span.style.bg, Some(expected), "column {column}");
            if (start..start + 3).contains(&column) {
                // Dark on the fill: no black box punched into it.
                assert_eq!(span.style.fg, Some(Color::Black), "label {column}");
            }
        }
        // A label wider than the bar is cut, not overflowed.
        assert_eq!(progress_line(2, 0.5, "1h 2m").spans.len(), 2);
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
