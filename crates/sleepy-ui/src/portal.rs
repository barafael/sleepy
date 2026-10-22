//! Watch the desktop's light/dark setting over XDG Desktop Portal.
//!
//! The portal only tells us about *changes*, so we read the current value
//! once at startup through `org.freedesktop.appearance` and then subscribe to
//! the `SettingChanged` signal. The latest value is parked in a static that
//! the application polls.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

use zbus::connection::Builder;
use zbus::proxy;
use zbus::zvariant::{OwnedValue, Value};

/// Name of the portal settings namespace we care about.
const NAMESPACE: &str = "org.freedesktop.appearance";

/// Name of the setting we care about.
const PREFER_DARK: &str = "color-scheme";

/// Latest `color-scheme` value seen from the portal.
/// 0 = unknown, 1 = false, 2 = true.
static DARK: AtomicU8 = AtomicU8::new(0);

/// Returns the last value seen from the portal, if any.
pub fn prefer_dark() -> Option<bool> {
    match DARK.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// Starts the watcher thread. Returns as soon as the thread is running;
/// failures are logged, never fatal.
pub fn spawn() {
    std::thread::spawn(|| {
        if let Err(err) = watch() {
            log::debug!("portal watcher unavailable: {err}");
        }
    });
}

/// The `org.freedesktop.portal.Settings` interface.
#[proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait Settings {
    /// Emitted whenever a setting changes.
    #[zbus(signal)]
    fn setting_changed(&self, namespace: &str, key: &str, value: OwnedValue) -> zbus::Result<()>;

    /// Reads every setting in the given namespaces. The portal answers with a
    /// dict of dicts, which is why the return type is this wall of variants.
    fn read_all(&self, namespaces: &[&str]) -> zbus::Result<OwnedValue>;
}

/// Turns a portal value for `color-scheme` into a boolean.
///
/// The portal spec says `0` means no preference, `1` prefers dark and `2`
/// prefers light; anything else is ignored.
fn to_dark(value: &Value<'_>) -> Option<bool> {
    let value = value.try_clone().ok()?.into_owned();
    let value = u32::try_from(value).ok()?;
    match value {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// Records a value in the static, if it means anything to us.
fn record(value: &Value<'_>) {
    if let Some(dark) = to_dark(value) {
        DARK.store(u8::from(dark), Ordering::Relaxed);
    }
}

/// The whole watcher: connect, read the current value, then follow changes.
fn watch() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(watch_async())
}

async fn watch_async() -> Result<(), Box<dyn std::error::Error>> {
    // The session bus is where the portal lives.
    let conn = Builder::session()?.build().await?;
    let settings = SettingsProxy::new(&conn).await?;

    // The portal has no "get my theme" call, so read the whole namespace and
    // pick our key out of it.
    let all = settings.read_all(&[NAMESPACE]).await?;
    if let Some(value) = find_color_scheme(&all) {
        record(&value);
    }

    // Follow changes.
    let mut stream = settings.receive_setting_changed().await?;
    while let Some(signal) = stream.next().await {
        let args = signal.args()?;
        if args.namespace != NAMESPACE || args.key != PREFER_DARK {
            continue;
        }
        record(&args.value);
    }
    Ok(())
}

/// Digs `org.freedesktop.appearance / color-scheme` out of the nested dict
/// returned by `ReadAll`.
fn find_color_scheme(all: &OwnedValue) -> Option<Value<'static>> {
    let all = all.try_clone().ok()?.into_owned();
    let all: HashMap<String, OwnedValue> = all.try_into().ok()?;
    let namespace: HashMap<String, OwnedValue> = all.get(NAMESPACE)?.try_into().ok()?;
    let value = namespace.get(PREFER_DARK)?;
    Some(value.try_clone().ok()?.into_owned())
}
