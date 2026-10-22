//! sleepy: arm a sleep timer from the terminal.

mod app;
mod event;
mod ui;

use anyhow::{Context, Result};
use crossterm::{
    event::KeyEventKind,
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};
use sleepy_core::{Backend, Options, describe};
use std::{io::stdout, time::Duration};

/// The heartbeat: four redraws a second are plenty for a countdown that
/// shows whole seconds, and keys still react as fast as the keyboard.
const TICK: Duration = Duration::from_millis(250);

fn main() -> std::process::ExitCode {
    let options = match Options::parse(std::env::args(), HELP) {
        Ok(Some(options)) => options,
        // The help was printed; there is nothing to run.
        Ok(None) => return std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sleepy: {message}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match run(options) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sleepy: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

const HELP: &str = "\
sleepy - arm a countdown to sleep

usage: sleepy [DURATION] [MODE] [OPTIONS]

arguments:
  DURATION   when to sleep, in humantime: 45s, 30min, 1h30m
             (default: 30 min; naming it arms the countdown at once)
  MODE       which sleep to use: suspend, hibernate, hybrid-sleep,
             suspend-then-hibernate, poweroff
             (default: the first this machine offers)

options:
      --dry-run         count down for real, but only say what would run
                        (ctrl+d toggles it while sleepy runs)
      --keep-screen-on  hold off screen blanking while the countdown runs
                        (k toggles it)
  -h, --help            print this help

keys: ctrl+d dry run, k keep awake, enter arms, space pauses, esc aborts,
      +/- a minute, q quits
      d is always a letter (2d, 30 min); space types while the timer is idle";

fn run(options: Options) -> Result<()> {
    // The guard owns the terminal's mood; dropping it gives the shell back.
    let guard = Guard::enter().context("cannot take over the terminal")?;
    let app = {
        let mut terminal =
            Terminal::new(CrosstermBackend::new(stdout())).context("cannot start the UI")?;
        let events = event::Events::new(TICK);
        let mut app = app::App::new(options);
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
                event::Event::Tick => {
                    if app.on_tick() {
                        // One last frame saying "time!", then the action:
                        // a real sleep may take a moment to get going, and
                        // the screen should already be saying so.
                        terminal
                            .draw(|frame| ui::draw(frame, &app))
                            .context("cannot draw")?;
                        app.controller.perform();
                        break;
                    }
                }
            }
        }
        app
    };
    // Only now is it safe to talk: the terminal belongs to the shell again.
    drop(guard);
    if let Some(action) = app.controller.fired {
        if app.controller.backend == Backend::DryRun
            && let Some(command) = describe(action)
        {
            println!("would have run: {command}");
        }
        if let Some(error) = app.controller.failure {
            return Err(error.into());
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
        execute!(stdout(), EnterAlternateScreen, SetTitle("sleepy"))?;
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
        // The empty title is the terminal's own reset: sleepy's name goes
        // away with sleepy, whatever the shell puts back is its business.
        execute!(stdout(), LeaveAlternateScreen, SetTitle(""))?;
        Ok(())
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = Self::leave();
    }
}
