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
use sleepy_core::Options;

fn main() -> eframe::Result {
    let options = match Options::parse(std::env::args(), HELP, VERSION) {
        Ok(Some(options)) => options,
        // The help or the version was printed; there is nothing to run.
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
        Box::new(move |cc| Ok(Box::new(SleepyApp::new(cc, options).with_window(tracker)))),
    )
}

const HELP: &str = "\
sleepy-ui - arm a countdown to sleep, from a window

usage: sleepy-ui [DURATION] [MODE] [OPTIONS]

arguments:
  DURATION   when to sleep: humantime (45s, 30min, 1h30m) or a clock
             time (23:45) - the next one to come
             (default: 30 min; naming it arms the countdown at once)
  MODE       what to do when the time comes: suspend, hibernate,
             hybrid-sleep, suspend-then-hibernate, poweroff, or
             run-script - run ./action.sh (action.ps1 through
             PowerShell on Windows) instead of sleeping; offered
             only while that file sits in the working directory
             (default: the first this machine offers)

options:
      --dry-run         count down for real, but only say what would run
                        (the window has a checkbox for the same choice)
      --keep-screen-on  hold off screen blanking while the countdown runs
                        (also a checkbox)
  -h, --help            print this help
  -V, --version         print sleepy-ui's name and version

the countdown keeps running while the window is hidden; a minimised
sleepy still puts the machine to sleep on time.";

/// What `-V` prints: the crate's own name and version, kept true by
/// the build rather than by memory.
const VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));
