//! The desktop's light or dark preference on Linux.
//!
//! winit reports no system theme on Linux, so egui's "follow the system"
//! always picked dark there. This reads `color-scheme` from the XDG settings
//! portal and follows its changes on a background thread.

use futures_util::stream::StreamExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use zbus::{OwnedVariant, Variant};

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
        // zbus 6 has no blocking API: the thread that calls `block_on`
        // drives the runtime itself, which is exactly what a thread whose
        // only job is watching one signal can do.
        zbus::block_on(self.watch_portal(ctx))
    }

    async fn watch_portal(&self, ctx: &egui::Context) -> zbus::Result<()> {
        let connection = zbus::Connection::session().await?;
        let settings = zbus::Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )
        .await?;
        // Subscribe before reading, so a change in between is not lost.
        let mut changes = settings.receive_signal("SettingChanged").await?;
        // `ReadOne` is the current method; older portals only have `Read`.
        let value: OwnedVariant = match settings.call("ReadOne", &(NAMESPACE, KEY)).await {
            Ok(value) => value,
            Err(_) => settings.call("Read", &(NAMESPACE, KEY)).await?,
        };
        self.set(&value, ctx);
        while let Some(message) = changes.next().await {
            if let Ok((namespace, key, value)) = message
                .body()
                .deserialize::<(String, String, OwnedVariant)>()
                && namespace == NAMESPACE
                && key == KEY
            {
                self.set(&value, ctx);
            }
        }
        Ok(())
    }

    fn set(&self, value: &Variant<'_>, ctx: &egui::Context) {
        self.0.store(scheme(value), Ordering::Relaxed);
        ctx.request_repaint();
    }
}

/// The scheme number, unwrapping the extra variant layer `Read` adds.
fn scheme(value: &Variant<'_>) -> u8 {
    match value {
        Variant::U32(n @ 0..=2) => *n as u8,
        Variant::Variant(inner) => scheme(inner),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_values() {
        assert_eq!(scheme(&Variant::U32(2)), 2);
        assert_eq!(scheme(&Variant::Variant(Box::new(Variant::U32(1)))), 1);
        assert_eq!(scheme(&Variant::U32(7)), 0);
        assert_eq!(scheme(&Variant::from("dark")), 0);
    }
}
