//! sleepy-ui: arm a sleep timer from a window.
//!
//! The same core the terminal sleepy drives, dressed in egui: the window
//! shows the countdown and the buttons, and every decision - what this
//! machine can do, what the typed duration means, when the time has come -
//! stays sleepy-core's, where it is tested without a display server.

mod app;
mod command;
mod geometry;
#[cfg(target_os = "linux")]
mod portal;
mod theme;
mod ui;

use app::SleepyApp;
use sleepy_core::{Backend, SleepAction, detect, parse_duration};

fn main() -> eframe::Result {
    let options = match Options::parse(std::env::args()) {
        Ok(Some(options)) => options,
        // The help was printed; there is nothing to run.
        Ok(None) => return Ok(()),
        Err(message) => {
            eprintln!("sleepy-ui: {message}");
            std::process::exit(2);
        }
    };
    let saved = geometry::Geometry::load();
    let viewport = egui::ViewportBuilder::default()
        .with_title("sleepy")
        .with_app_id("sleepy-ui")
        .with_inner_size([560.0, 400.0])
        .with_min_inner_size([440.0, 320.0]);
    let native_options = eframe::NativeOptions {
        viewport: match saved {
            Some(geometry) => geometry.apply(viewport),
            None => viewport,
        },
        // sleepy restores the window itself; see `geometry`.
        persist_window: false,
        ..Default::default()
    };
    let tracker = geometry::Tracker::new(saved.and_then(|g| g.position));
    eframe::run_native(
        "sleepy-ui",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(
                SleepyApp::new(
                    cc,
                    options.backend,
                    options.keep_screen_on,
                    options.duration,
                    options.mode,
                )
                .with_window(tracker),
            ))
        }),
    )
}

/// What the command line decided before a single window appeared.
struct Options {
    backend: Backend,
    keep_screen_on: bool,
    duration: Option<String>,
    mode: Option<SleepAction>,
}

const HELP: &str = "\
sleepy-ui - arm a countdown to sleep, from a window

usage: sleepy-ui [DURATION] [MODE] [OPTIONS]

arguments:
  DURATION   when to sleep, in humantime: 45s, 30min, 1h30m
             (default: 30 min; naming it arms the countdown at once)
  MODE       which sleep to use: suspend, hibernate, hybrid-sleep,
             suspend-then-hibernate, poweroff
             (default: the first this machine offers)

options:
      --dry-run         count down for real, but only say what would run
                        (the window has a checkbox for the same choice)
      --keep-screen-on  hold off screen blanking while the countdown runs
                        (also a checkbox)
  -h, --help            print this help

the countdown keeps running while the window is hidden; a minimised
sleepy still puts the machine to sleep on time.";

impl Options {
    /// The terminal sleepy's parser, with the window's help text: two
    /// flags and two positionals do not earn a dependency.
    fn parse(args: impl Iterator<Item = String>) -> Result<Option<Self>, String> {
        let mut options = Self {
            backend: Backend::Real,
            keep_screen_on: false,
            duration: None,
            mode: None,
        };
        let mut positionals = 0;
        for arg in args.skip(1) {
            match arg.as_str() {
                "-h" | "--help" => {
                    println!("{HELP}");
                    return Ok(None);
                }
                "--dry-run" => options.backend = Backend::DryRun,
                "--keep-screen-on" => options.keep_screen_on = true,
                _ if arg.starts_with('-') => {
                    return Err(format!("unknown option {arg} (try --help)"));
                }
                _ if positionals == 0 => {
                    options.duration = Some(arg);
                    positionals += 1;
                }
                _ if positionals == 1 => {
                    options.mode = Some(
                        arg.parse()
                            .map_err(|_| format!("unknown mode {arg} (try --help)"))?,
                    );
                    positionals += 1;
                }
                _ => return Err(format!("unexpected argument {arg} (try --help)")),
            }
        }
        // Check the answers before the window opens, where errors go
        // unread and unheard.
        if let Some(text) = &options.duration {
            parse_duration(text).map_err(|error| format!("bad duration {text}: {error}"))?;
        }
        if let Some(mode) = options.mode {
            let offered = detect();
            if !offered.contains(&mode) {
                let names: Vec<String> = offered.iter().map(ToString::to_string).collect();
                return Err(format!(
                    "this machine does not offer {mode} (it offers: {})",
                    names.join(", ")
                ));
            }
        }
        Ok(Some(options))
    }
}
