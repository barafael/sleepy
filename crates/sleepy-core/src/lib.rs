//! sleepy-core: the sleep timer behind sleepy's frontends.

pub mod action;

pub use action::{Probe, SleepAction, detect, detect_with};
