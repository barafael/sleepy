//! sleepy-core: the sleep timer behind sleepy's frontends.

pub mod action;
pub mod backend;

pub use action::{Probe, SleepAction, detect, detect_with};
pub use backend::{Backend, describe, execute};
