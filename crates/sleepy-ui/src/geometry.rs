//! Remembers the window's size and position between runs.
//!
//! eframe can persist the window itself, but winit reports the client area's
//! position as the frame's under XWayland, so every restore placed the frame
//! where the client area had been: the window crept down by its title bar on
//! each launch. This measures where the window ends up relative to where it
//! was asked to go, and saves positions with that shift taken out, so they
//! round-trip whatever the window manager does. Wayland does not let
//! applications place windows; there only the size is restored.

use egui::{Pos2, Vec2, ViewportBuilder};
use std::path::PathBuf;

/// Seconds after startup during which the window manager may still move the window.
const SETTLE: f64 = 1.0;

/// Saved window size, position and state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Where to ask the window manager to put the window, if known.
    pub position: Option<Pos2>,
    /// Size of the client area, in points.
    pub size: Vec2,
    /// Whether the window was maximized.
    pub maximized: bool,
}

impl Geometry {
    fn path() -> Option<PathBuf> {
        eframe::storage_dir("sleepy-ui").map(|dir| dir.join("window"))
    }

    /// The geometry saved by the last run, if any.
    pub fn load() -> Option<Self> {
        Self::parse(&std::fs::read_to_string(Self::path()?).ok()?)
    }

    /// Format: `x y width height maximized`, with `-` for an unknown position.
    fn parse(text: &str) -> Option<Self> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        let [x, y, width, height, maximized] = fields[..] else {
            return None;
        };
        let position = match (x.parse(), y.parse()) {
            (Ok(x), Ok(y)) => Some(Pos2::new(x, y)),
            _ => None,
        };
        let size = Vec2::new(width.parse().ok()?, height.parse().ok()?);
        (size.x > 0.0 && size.y > 0.0).then_some(Self {
            position,
            size,
            maximized: maximized == "1",
        })
    }

    fn format(&self) -> String {
        let (x, y) = match self.position {
            Some(p) => (p.x.round().to_string(), p.y.round().to_string()),
            None => ("-".into(), "-".into()),
        };
        let (width, height) = (self.size.x.round(), self.size.y.round());
        format!("{x} {y} {width} {height} {}", u8::from(self.maximized))
    }

    /// Asks for this geometry in a new window.
    pub fn apply(&self, builder: ViewportBuilder) -> ViewportBuilder {
        let builder = builder
            .with_inner_size(self.size)
            .with_maximized(self.maximized);
        match self.position {
            Some(position) => builder.with_position(position),
            None => builder,
        }
    }
}

/// Follows the window to save where it is.
#[derive(Debug)]
pub struct Tracker {
    /// The position asked for at startup.
    requested: Option<Pos2>,
    /// Where the window ended up relative to `requested`.
    shift: Option<Vec2>,
}

impl Tracker {
    /// Tracks a window that was asked to open at `requested`.
    pub fn new(requested: Option<Pos2>) -> Self {
        Self {
            requested,
            shift: None,
        }
    }

    /// Measures the shift while the window settles. Called every frame.
    ///
    /// Window managers can move a new window more than once while mapping
    /// it, so the shift is taken from where the window is after a moment,
    /// not on the first frame.
    pub fn observe(&mut self, ctx: &egui::Context) {
        let (time, outer) = ctx.input(|i| (i.time, i.viewport().outer_rect));
        if let Some(outer) = outer
            && (self.shift.is_none() || time < SETTLE)
        {
            self.shift = Some(self.requested.map_or(Vec2::ZERO, |r| outer.min - r));
            if time < SETTLE {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(SETTLE - time));
            }
        }
    }

    /// Writes the window's current geometry for the next run.
    pub fn save(&self, ctx: &egui::Context) {
        let viewport = ctx.input(|i| i.viewport().clone());
        // Without a position (Wayland) egui has no inner rectangle either;
        // the content area has the same size.
        let size = viewport
            .inner_rect
            .map_or_else(|| ctx.content_rect().size(), |inner| inner.size());
        let geometry = Geometry {
            position: viewport
                .outer_rect
                .map(|outer| outer.min - self.shift.unwrap_or_default()),
            size,
            maximized: viewport.maximized.unwrap_or(false),
        };
        if let Some(path) = Geometry::path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, geometry.format());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let geometry = Geometry {
            position: Some(Pos2::new(300.0, 204.0)),
            size: Vec2::new(420.0, 380.0),
            maximized: false,
        };
        assert_eq!(geometry.format(), "300 204 420 380 0");
        assert_eq!(Geometry::parse(&geometry.format()), Some(geometry));
        let wayland = Geometry::parse("- - 420 380 1").unwrap();
        assert_eq!(wayland.position, None);
        assert!(wayland.maximized);
        assert_eq!(Geometry::parse("garbage"), None);
        assert_eq!(Geometry::parse("1 2 0 0 0"), None);
    }
}
