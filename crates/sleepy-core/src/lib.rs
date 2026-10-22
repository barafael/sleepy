//! sleepy-core: the sleep timer behind sleepy's frontends.
//!
//! Four questions, one module each:
//!
//! - [`action`]: what can this machine do? (suspend, hibernate, power off...)
//! - [`timer`]: when should we do it? (a countdown to arm, pause and abort)
//! - [`backend`]: how do we do it? (and a dry run that only says so)
//! - [`awake`]: how do we stay visible while we count? (and let go after)
//!
//! and two answers the frontends share, so the terminal and the window
//! cannot drift apart:
//!
//! - [`cli`]: what the command line decided, parsed once and detected once;
//! - [`controller`]: the state and the commands - what every frontend does
//!   with the four answers above.
//!
//! The frontends (`sleepy`, `sleepy-ui`) only draw the answers and forward
//! the user's commands; every decision lives here, where it can be tested
//! without a terminal, a window, or a sleeping machine.

pub mod action;
pub mod awake;
pub mod backend;
pub mod cli;
pub mod controller;
pub mod input;
pub mod timer;

pub use action::{Probe, SleepAction, detect, detect_with};
pub use awake::KeepAwake;
pub use backend::{Backend, Error, describe, execute};
pub use cli::Options;
pub use controller::{Controller, NUDGE};
pub use input::{ParseError, parse_duration};
pub use timer::{Event as TimerEvent, Status, Timer};
