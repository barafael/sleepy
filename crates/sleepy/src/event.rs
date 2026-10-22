//! One place to wait on: keys from the keyboard, ticks from a clock.
//!
//! The countdown must keep redrawing even while nobody types, and keys must
//! still react instantly. So two producer threads post into one channel -
//! one blocks on `event::read`, the other sleeps and wakes as a tick - and
//! the main thread has a single `recv` to block on. No spinning, no async
//! runtime. When the receiver goes away the tick thread's send fails and
//! it finishes on its own; the key thread is parked inside `event::read`
//! and cannot notice until the next keypress, so it simply goes down with
//! the process - which exits right after the event loop does.

use crossterm::event::{self, Event as CEvent, KeyEvent};
use std::{
    sync::mpsc::{self, Receiver, RecvError},
    thread::{self, JoinHandle},
    time::Duration,
};

/// Something the app should react to.
#[derive(Debug)]
pub enum Event {
    /// The user pressed a key.
    Key(KeyEvent),
    /// The heartbeat; time may have passed.
    Tick,
}

/// The app's view of the outside world.
pub struct Events {
    rx: Receiver<Event>,
    // The tick thread stops when `rx` drops; the key thread is parked in
    // `event::read` and goes down with the process. The handles only
    // document that.
    _producers: (JoinHandle<()>, JoinHandle<()>),
}

impl Events {
    /// Keys as they arrive, plus a tick every `interval`.
    pub fn new(interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let keys = {
            let tx = tx.clone();
            thread::spawn(move || {
                while let Ok(CEvent::Key(key)) = event::read() {
                    if tx.send(Event::Key(key)).is_err() {
                        return;
                    }
                }
            })
        };
        let ticks = thread::spawn(move || {
            loop {
                thread::sleep(interval);
                if tx.send(Event::Tick).is_err() {
                    return;
                }
            }
        });
        Self {
            rx,
            _producers: (keys, ticks),
        }
    }

    /// The next event, waiting for one if nothing has happened yet.
    pub fn next(&self) -> Result<Event, RecvError> {
        self.rx.recv()
    }
}
