//! sleepy-core: the sleep timer behind sleepy's frontends.
//!
//! Four questions, one module each:
//!
//! - [`action`]: what can this machine do? (suspend, hibernate, power off...)
//! - [`timer`]: when should we do it? (a countdown to arm, pause and abort)
//! - [`backend`]: how do we do it? (and a dry run that only says so)
//! - [`awake`]: how do we stay visible while we count? (and let go after)
//!
//! and, where the machine is a browser, one more:
//!
//! - `web`: the browser's own answers (a notification to fire, a
//!   Wake Lock to hold) for the parts the platform helpers cannot do;
//!
//! and three answers the frontends share, so the terminal and the window
//! cannot drift apart:
//!
//! - [`cli`]: what the command line decided, parsed once and detected once;
//! - [`command`]: what the user can ask for - one vocabulary;
//! - [`controller`]: the state and the answers - what every frontend does
//!   with a command, one grammar.
//!
//! The frontends (`sleepy`, `sleepy-ui`) only draw the answers and forward
//! the user's commands; every decision lives here, where it can be tested
//! without a terminal, a window, or a sleeping machine.

pub mod action;
pub mod awake;
pub mod backend;
pub mod cli;
pub mod command;
pub mod controller;
pub mod input;
pub mod timer;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use action::{SleepAction, detect};
pub use backend::{Backend, describe};
pub use cli::Options;
pub use command::Command;
pub use controller::Controller;
pub use timer::Status;
