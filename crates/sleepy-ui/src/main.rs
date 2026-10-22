//! sleepy-ui: arm a sleep timer from a window.
//!
//! The same core the terminal sleepy drives, dressed in egui: the window
//! shows the countdown and the buttons, and every decision - what this
//! machine can do, what the typed duration means, when the time has come -
//! stays sleepy-core's, where it is tested without a display server.

mod app;

use app::SleepyApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("sleepy")
            .with_app_id("sleepy-ui")
            .with_inner_size([420.0, 380.0])
            .with_min_inner_size([340.0, 300.0]),
        // Until sleepy keeps its own geometry, eframe's persistence is
        // enough to remember the window between runs.
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        "sleepy-ui",
        options,
        Box::new(|_cc| Ok(Box::new(SleepyApp::new()))),
    )
}
