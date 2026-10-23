//! sleepy-ui: arm a sleep timer from a window.
//!
//! The same core the terminal sleepy drives, dressed in egui: the window
//! shows the countdown and the buttons, and every decision - what this
//! machine can do, what the typed duration means, when the time has come -
//! stays sleepy-core's, where it is tested without a display server.
//!
//! The same window compiles to wasm and runs on a web page; there the
//! browser is the machine, and sleepy's one mode is the notification.

mod app;
mod command;
mod theme;
mod ui;

use app::SleepyApp;
use sleepy_core::Options;

/// The hourglass the window and the taskbar wear, drawn in `assets/`.
#[cfg(not(target_arch = "wasm32"))]
const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

#[cfg(not(target_arch = "wasm32"))]
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
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("sleepy")
        .with_app_id("sleepy-ui")
        .with_inner_size([560.0, 400.0])
        .with_min_inner_size([440.0, 320.0]);
    // The hourglass is embedded in the binary, so a failure to decode it
    // would be a build bug, not a runtime one; were it ever to happen,
    // the window simply goes iconless rather than refusing to open.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON_PNG) {
        viewport = viewport.with_icon(icon);
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "sleepy-ui",
        native_options,
        Box::new(move |cc| Ok(Box::new(SleepyApp::new(cc, options)))),
    )
}

/// The web's entry: the same window on a canvas, asking the browser's
/// leave to notify before anything else is drawn.
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    console_error_panic_hook::set_once();
    keep_ctrl_d();
    // The browser grants notifications once per site and remembers; ask
    // at the moment the page is freshest, where whoever came to be told
    // "time" in thirty minutes means to be told. Browsers that wait for
    // a gesture before they prompt are asked again at the first one.
    sleepy_core::web::ask_to_notify();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().and_then(|window| window.document());
        let canvas = document
            .as_ref()
            .and_then(|document| document.get_element_by_id("sleepy"))
            .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        let Some(canvas) = canvas else {
            // No canvas to draw on: the page itself says so below.
            report(&document, "this page has no canvas to draw sleepy on");
            return;
        };
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(SleepyApp::new(cc, web_options())))),
            )
            .await;
        match started {
            Ok(()) => {
                if let Some(document) = document.as_ref()
                    && let Some(loading) = document.get_element_by_id("loading")
                {
                    loading.remove();
                }
            }
            // A panic on the web has no stderr; the page is where the
            // news can still reach someone.
            Err(error) => {
                let message = error.as_string().unwrap_or_else(|| format!("{error:?}"));
                report(&document, &message);
            }
        }
    });
}

/// Says the bad news on the page, where the loading text already is.
#[cfg(target_arch = "wasm32")]
fn report(document: &Option<web_sys::Document>, message: &str) {
    if let Some(loading) = document
        .as_ref()
        .and_then(|document| document.get_element_by_id("loading"))
    {
        loading.set_text_content(Some(&format!("sleepy could not start: {message}")));
    }
}

/// Ctrl+D is sleepy's dry-run toggle and the browser's bookmark chord;
/// while the page has the keys, the page answers it. eframe leaves
/// this chord to the browser, so the page keeps its own counsel: the
/// bookmark dialog would open over a countdown the user was only
/// trying to mark dry.
#[cfg(target_arch = "wasm32")]
fn keep_ctrl_d() {
    use eframe::wasm_bindgen::JsCast as _;
    use eframe::wasm_bindgen::closure::Closure;
    let Some(window) = web_sys::window() else {
        return;
    };
    let keep = Closure::wrap(Box::new(|event: web_sys::KeyboardEvent| {
        if event.ctrl_key()
            && !(event.shift_key() || event.alt_key() || event.meta_key())
            && event.key().eq_ignore_ascii_case("d")
        {
            event.prevent_default();
        }
    }) as Box<dyn Fn(web_sys::KeyboardEvent)>);
    if window
        .add_event_listener_with_callback("keydown", keep.as_ref().unchecked_ref())
        .is_ok()
    {
        // The watcher lives as long as the page, which is as long as
        // the chord needs an answer.
        keep.forget();
    }
}

/// The choices the command line would have made: the browser offers one
/// mode, nothing is armed until asked, and a fired countdown really
/// sends the notification - the dry-run checkbox is there to unchoose.
#[cfg(target_arch = "wasm32")]
fn web_options() -> Options {
    Options {
        backend: sleepy_core::Backend::Real,
        keep_screen_on: false,
        when: None,
        mode: None,
        modes: sleepy_core::detect(),
    }
}

/// The help the command line prints.
#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
const VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));
