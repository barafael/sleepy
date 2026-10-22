//! Persisted window geometry.
//!
//! The window remembers where it was. On startup we read the last size and
//! position back from egui's storage and hand them to eframe before the
//! window is created; on shutdown we write them out again.
//!
//! One detail is worth knowing about: egui's storage is only flushed when the
//! frame is *already* being rebuilt. A window that is merely hidden or
//! minimized is not being rebuilt, so a commit made from the close request
//! would be dropped. We therefore keep committing the geometry on every
//! rebuild until one is confirmed to have been stored, and only then allow
//! the window to close.

use std::error::Error;
use std::fmt;

use eframe::Storage;
use egui::{Context, Pos2, Vec2};

/// Application id used for storage.
pub const APP_ID: &str = "sleepy-ui";

/// Key the geometry is stored under.
const GEOMETRY_KEY: &str = "window.geometry";

/// The window geometry we persist across runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    /// Inner size of the window, in logical points.
    pub inner_size: Vec2,
    /// Position of the window's top-left corner, in logical points.
    pub position: Pos2,
}

impl WindowGeometry {
    /// Reads the persisted geometry, if any.
    pub fn load(storage: &dyn Storage) -> Option<Self> {
        let raw = storage.get_string(GEOMETRY_KEY)?;
        serde_json::from_str(&raw).ok()
    }

    /// Writes the geometry to storage.
    pub fn store(&self, storage: &mut dyn Storage) {
        match serde_json::to_string(self) {
            Ok(json) => storage.set_string(GEOMETRY_KEY, json),
            Err(err) => log::warn!("could not serialize window geometry: {err}"),
        }
    }
}

/// Error returned when the window is asked to close before the pending
/// geometry has been committed.
#[derive(Clone, Copy, Debug)]
pub struct PendingGeometry(());

impl fmt::Display for PendingGeometry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "window geometry not committed yet")
    }
}

impl Error for PendingGeometry {}

/// Tracks whether the latest geometry still needs to be committed.
#[derive(Clone, Copy, Debug, Default)]
pub struct GeometryTracker {
    /// Geometry that has not been stored yet.
    pending: Option<WindowGeometry>,
}

impl GeometryTracker {
    /// Creates a tracker with nothing pending.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the window's current geometry as pending, replacing any
    /// earlier value.
    pub fn observe(&mut self, ctx: &Context) {
        let input = ctx.input(|i| i.viewport().clone());

        // Either half may be unknown depending on the platform and the
        // window state, so both are optional until we have a pair.
        let inner_size = input
            .inner_size
            .or_else(|| ctx.memory(|m| m.screen_rect().size()))
            .or(input.raw_input.pixels_per_point.map(|_| Vec2::ZERO));
        let position = input.position;

        if let (Some(inner_size), Some(position)) = (inner_size, position) {
            if inner_size.x <= 0.0 || inner_size.y <= 0.0 {
                return;
            }
            self.pending = Some(WindowGeometry {
                inner_size,
                position,
            });
        }
    }

    /// Writes out the pending geometry. Returns `false` while something is
    /// still pending, so callers can postpone quitting until it is stored.
    pub fn commit(&mut self, ctx: &Context) -> Result<(), PendingGeometry> {
        let Some(pending) = self.pending else {
            return Ok(());
        };

        let stored = ctx
            .data(|data| data.get_persistent_storage::<()>(()))
            .and_then(|storage| {
                let mut storage = storage.write().unwrap();
                pending.store(&mut *storage);
                // The frame is being rebuilt, so egui will flush storage
                // when it is done.
                if ctx.has_requested_repaint() {
                    Some(Ok(()))
                } else {
                    // Nothing will flush it; try again on the next rebuild.
                    Some(Err(PendingGeometry(())))
                }
            })
            .unwrap_or_else(|| {
                // No storage available, so there is nothing to wait for.
                Ok(())
            });

        if stored.is_ok() {
            self.pending = None;
        }
        stored
    }
}
