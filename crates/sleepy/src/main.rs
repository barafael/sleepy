//! sleepy: arm a sleep timer from the terminal.

mod app;
mod event;
mod ui;

use anyhow::{Context, Result};
use crossterm::{
    event::KeyEventKind,
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io::stdout, time::Duration};

/// The heartbeat: four redraws a second are plenty for a countdown that
/// shows whole seconds, and keys still react as fast as the keyboard.
const TICK: Duration = Duration::from_millis(250);

fn main() {
    if let Err(e) = run() {
        eprintln!("sleepy: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // The guard owns the terminal's mood; dropping it gives the shell back.
    let _guard = Guard::enter().context("cannot take over the terminal")?;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(stdout())).context("cannot start the UI")?;
    let events = event::Events::new(TICK);
    let mut app = app::App::new();
    loop {
        terminal
            .draw(|frame| ui::draw(frame, &app))
            .context("cannot draw")?;
        match events.next().context("the event loop died")? {
            // Only presses: terminals that report releases would double
            // every keystroke.
            event::Event::Key(key) if key.kind == KeyEventKind::Press => {
                if app.on_key(key) {
                    break;
                }
            }
            event::Event::Key(_) => {}
            event::Event::Tick => app.on_tick(),
        }
    }
    Ok(())
}

/// The terminal in raw mode on the alternate screen, with a promise to give
/// the shell back exactly as it was found.
///
/// The promise is kept twice over: on the way out of `main` through `Drop`,
/// and on the way into a panic through a hook, so a bug never leaves the
/// user staring at a raw terminal that echoes nothing.
struct Guard;

impl Guard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Restore first: the panic message should be readable, and the
            // shell should be usable to act on it.
            let _ = Guard::leave();
            previous_hook(info);
        }));
        Ok(Self)
    }

    fn leave() -> Result<()> {
        disable_raw_mode()?;
        execute!(stdout(), LeaveAlternateScreen)?;
        Ok(())
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = Self::leave();
    }
}
