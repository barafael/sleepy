//! The desktop's light or dark preference on Linux.
//!
//! winit reports no system theme on Linux, so egui's "follow the system"
//! always picked dark there. This reads `color-scheme` from the XDG settings
//! portal and follows its changes on a background thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use zbus::zvariant::{OwnedValue, Value};

const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

/// The latest preference, in the portal's numbering: 0 none, 1 dark, 2 light.
#[derive(Clone, Default)]
pub struct SystemTheme(Arc<AtomicU8>);

impl SystemTheme {
    /// Starts following the portal; `ctx` is repainted when the preference changes.
    pub fn watch(ctx: egui::Context) -> Self {
        let theme = Self::default();
        let shared = theme.clone();
        // Without a session bus or portal there is nothing to follow, and the
        // fallback (dark) stays.
        let _ = std::thread::Builder::new()
            .name(String::from("color-scheme"))
            .spawn(move || shared.follow(&ctx));
        theme
    }

    /// The preferred theme, if the desktop states one.
    pub fn get(&self) -> Option<egui::Theme> {
        match self.0.load(Ordering::Relaxed) {
            1 => Some(egui::Theme::Dark),
            2 => Some(egui::Theme::Light),
            _ => None,
        }
    }

    fn follow(&self, ctx: &egui::Context) -> zbus::Result<()> {
        let connection = zbus::blocking::Connection::session()?;
        let settings = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )?;
        // Subscribe before reading, so a change in between is not lost.
        let changes = settings.receive_signal("SettingChanged")?;
        // `ReadOne` is the current method; older portals only have `Read`.
        let value: OwnedValue = settings
            .call("ReadOne", &(NAMESPACE, KEY))
            .or_else(|_| settings.call("Read", &(NAMESPACE, KEY)))?;
        self.set(&value, ctx);
        for message in changes {
            if let Ok((namespace, key, value)) =
                message.body().deserialize::<(String, String, OwnedValue)>()
                && namespace == NAMESPACE
                && key == KEY
            {
                self.set(&value, ctx);
            }
        }
        Ok(())
    }

    fn set(&self, value: &Value<'_>, ctx: &egui::Context) {
        self.0.store(scheme(value), Ordering::Relaxed);
        ctx.request_repaint();
    }
}

/// The scheme number, unwrapping the extra variant layer `Read` adds.
fn scheme(value: &Value<'_>) -> u8 {
    match value {
        Value::U32(n @ 0..=2) => *n as u8,
        Value::Value(inner) => scheme(inner),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_values() {
        assert_eq!(scheme(&Value::U32(2)), 2);
        assert_eq!(scheme(&Value::Value(Box::new(Value::U32(1)))), 1);
        assert_eq!(scheme(&Value::U32(7)), 0);
        assert_eq!(scheme(&Value::from("dark")), 0);
    }
}
