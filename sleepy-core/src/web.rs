//! The browser as sleepy's machine: notifications instead of sleep, and
//! a Wake Lock instead of an idle inhibitor.
//!
//! Neither API is kind to a caller who presumes: constructing a
//! `Notification` without permission throws, and a Wake Lock requested
//! by a hidden page is refused. So every call here asks first, and a
//! browser that says no gets the same answer sleepy gives a machine
//! without systemd - a shrug where it can be read, and a countdown that
//! goes on regardless.

use std::cell::RefCell;
use std::sync::Once;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    Notification, NotificationOptions, NotificationPermission, VisibilityState, WakeLock,
    WakeLockSentinel, WakeLockType,
};

use crate::action::SleepAction;
use crate::backend::{Backend, Error};

/// Ask the browser for leave to notify, right as the page loads.
///
/// Browsers that wait for a user gesture before they prompt (Chrome)
/// would swallow a request made at load, so the ask is repeated once at
/// the first pointer or key - whichever comes first.
pub fn ask_to_notify() {
    if !supported() {
        return;
    }
    request_permission();
    watch_first_gesture();
}

/// Whether this browser has the Notification API at all. (Private
/// windows disable it by not defining it, so `in` is the honest test.)
fn supported() -> bool {
    js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str("Notification")).unwrap_or(false)
}

/// Fires `Notification.requestPermission()` and swallows the answer:
/// the countdown reads the permission when it fires, so this call only
/// has to happen.
fn request_permission() {
    spawn_local(async {
        if let Ok(promise) = Notification::request_permission() {
            let _ = JsFuture::from(promise).await;
        }
    });
}

/// Browsers that defer the prompt until the user do something get asked
/// again at the first pointer or key; after that the watchers remove
/// themselves and never ask again.
fn watch_first_gesture() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let ask = Closure::wrap(Box::new(|| {
        let Some(window) = web_sys::window() else {
            return;
        };
        window.set_onpointerdown(None);
        window.set_onkeydown(None);
        if Notification::permission() == NotificationPermission::Default {
            request_permission();
        }
    }) as Box<dyn Fn()>);
    window.set_onpointerdown(Some(ask.as_ref().unchecked_ref()));
    window.set_onkeydown(Some(ask.as_ref().unchecked_ref()));
    // The watchers live as long as the page; they dismiss themselves
    // after the first gesture, and a page gets one page's worth.
    ask.forget();
}

/// Send the notification the countdown was armed for.
pub(crate) fn notify(backend: Backend) -> Result<(), Error> {
    if !supported() {
        // No Notification API: this machine has no command for notify.
        return Err(Error::Unsupported(SleepAction::Notify));
    }
    if backend == Backend::DryRun {
        return Ok(());
    }
    if Notification::permission() != NotificationPermission::Granted {
        // The user said no, or has not answered yet; either way it is
        // the browser that refuses, and the browser is named as such.
        return Err(Error::Refused {
            action: SleepAction::Notify,
            program: "the browser".into(),
        });
    }
    let options = NotificationOptions::new();
    options.set_body("time! the countdown ended.");
    Notification::new_with_options("sleepy", &options).map_err(|_| Error::Refused {
        action: SleepAction::Notify,
        program: "the browser".into(),
    })?;
    Ok(())
}

/// What sleepy wants of the screen, and what the browser has handed over.
/// One copy for the whole page: the browser hands out one lock per
/// document anyway, and the visibility watcher must find the wish
/// wherever it was left.
struct Screen {
    want: bool,
    held: Option<WakeLockSentinel>,
}

thread_local! {
    static SCREEN: RefCell<Screen> = const {
        RefCell::new(Screen {
            want: false,
            held: None,
        })
    };
    /// The one release notice, handed to every sentinel: it only says
    /// "the browser took it back", which is the same words each time.
    /// Leaked once, because every sentinel is asked to hold the same
    /// reference and the page outlives none of them.
    static ON_RELEASE: RefCell<Option<&'static Closure<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Hold (or let go of) the screen, for as long as sleepy wants it.
pub(crate) fn hold_screen(want: bool) {
    // A hidden page's request is refused, and a hiding page loses the
    // lock it holds; watching for the page coming back is what keeps the
    // wish and the lock in step, so watch from the first wish onward.
    watch_visibility();
    SCREEN.with(|screen| {
        let mut screen = screen.borrow_mut();
        screen.want = want;
        if want && screen.held.is_none() {
            hold();
        } else if !want {
            screen.release();
        }
    });
}

/// The browser's Wake Lock, if this browser hands one out at all. The
/// getter is web-sys's unconditional word that it does, so `in` is the
/// honest test, as it is for the Notification API.
fn wake_lock() -> Option<WakeLock> {
    let navigator = web_sys::window()?.navigator();
    if !js_sys::Reflect::has(&navigator, &JsValue::from_str("wakeLock")).unwrap_or(false) {
        return None;
    }
    Some(navigator.wake_lock())
}

/// Asks the browser for the Wake Lock. The answer arrives later; by
/// then sleepy may have let go, which the reply honours.
fn hold() {
    let Some(lock) = wake_lock() else {
        return;
    };
    let promise = lock.request(WakeLockType::Screen);
    spawn_local(async move {
        // A refused lock is a shrug, like a helper that will not start:
        // the countdown goes on and the screen may still blank.
        let Ok(sentinel) = JsFuture::from(promise).await else {
            return;
        };
        SCREEN.with(|screen| {
            let mut screen = screen.borrow_mut();
            if !screen.want {
                // Let go of while the browser was deciding.
                screen.let_go(sentinel);
                return;
            }
            sentinel.set_onrelease(Some(on_release()));
            screen.held = Some(sentinel);
        });
    });
}

impl Screen {
    /// Lets go of the held lock, if there is one.
    fn release(&mut self) {
        if let Some(sentinel) = self.held.take() {
            self.let_go(sentinel);
        }
    }

    /// Hands a sentinel back to the browser. The release promise has
    /// nothing left to say - the lock is the browser's again either
    /// way, and a lock the browser already took back says so quietly.
    fn let_go(&self, sentinel: WakeLockSentinel) {
        sentinel.set_onrelease(None);
        spawn_local(async move {
            let _ = JsFuture::from(sentinel.release()).await;
        });
    }
}

/// The notice every sentinel gets: the browser releases the lock on its
/// own when the page hides, and forgetting the sentinel is what lets a
/// later wish ask for a new one.
fn on_release() -> &'static js_sys::Function {
    // The reference is copied out of the cell: it is 'static on its
    // own, and does not live off the borrow.
    let closure: &'static Closure<dyn Fn()> = ON_RELEASE.with(|cell| {
        *cell.borrow_mut().get_or_insert_with(|| {
            Box::leak(Box::new(Closure::wrap(Box::new(|| {
                SCREEN.with(|screen| screen.borrow_mut().held = None);
            }) as Box<dyn Fn()>)))
        })
    });
    closure.as_ref().unchecked_ref()
}

/// Watches for the page coming back to the light, once per page: a page
/// that hid has lost its lock, and a page that returns re-asks for one
/// if the countdown still wants it.
fn watch_visibility() {
    static WATCHED: Once = Once::new();
    WATCHED.call_once(|| {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let again = Closure::wrap(Box::new(|| {
            let visible = web_sys::window()
                .and_then(|window| window.document())
                .is_some_and(|document| document.visibility_state() == VisibilityState::Visible);
            if visible {
                SCREEN.with(|screen| {
                    let mut screen = screen.borrow_mut();
                    if screen.want {
                        // Whatever handle the page left with is stale;
                        // the wish is what survives the dark.
                        screen.held = None;
                        hold();
                    }
                });
            }
        }) as Box<dyn Fn()>);
        document.set_onvisibilitychange(Some(again.as_ref().unchecked_ref()));
        again.forget();
    });
}
