//! The terminal app, run for real in a pty it does not know is fake.
//!
//! The unit tests hand `draw` a frame of memory; this hands the binary a
//! terminal - a real pty, real escape sequences, a real exit - and holds
//! sleepy to the oldest promise a terminal app makes: take the screen,
//! say its piece, and hand the shell back exactly as it was found.

// A test that cannot get its answer is broken, not a machine that said no.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Whatever the child has written, gathered by a reader thread: the
/// master's reader blocks, and the test must keep reading while it
/// decides what it has seen.
type Output = Arc<Mutex<Vec<u8>>>;

/// sleepy, in a terminal of its own, with its output being gathered.
fn sleepy(
    args: &[&str],
) -> (
    Box<dyn MasterPty + Send>,
    Box<dyn Child + Send + Sync>,
    Output,
) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sleepy"));
    command.args(args);
    // A pty has no terminal of its own to inherit a mood from.
    command.env("TERM", "xterm-256color");
    let child = pair.slave.spawn_command(command).unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let output: Output = Arc::new(Mutex::new(Vec::new()));
    let gathered = Arc::clone(&output);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(size) = reader.read(&mut chunk)
            && size > 0
        {
            gathered.lock().unwrap().extend_from_slice(&chunk[..size]);
        }
    });
    (pair.master, child, output)
}

fn seen(output: &Output, wanted: &[u8]) -> bool {
    let bytes = output.lock().unwrap();
    bytes.windows(wanted.len()).any(|window| window == wanted)
}

/// Everything said so far, once the screen has said `wanted` - polling,
/// so a fast machine finishes fast and a slow CI gets its patience.
fn until_said(output: &Output, wanted: &[u8], patience: Duration) -> Vec<u8> {
    let deadline = Instant::now() + patience;
    loop {
        let bytes = output.lock().unwrap().clone();
        if bytes.windows(wanted.len()).any(|w| w == wanted) || Instant::now() > deadline {
            return bytes;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn shown(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Wait for the terminal to say `wanted`, and fail naming what seeing it
/// proves.
fn assert_said(output: &Output, wanted: &[u8], promise: &str, patience: Duration) {
    let bytes = until_said(output, wanted, patience);
    assert!(seen(output, wanted), "{promise}:\n{}", shown(&bytes));
}

/// The exit, polled in slices: a sleepy that never leaves is a failed
/// test, not a hung one.
fn wait_for_exit(child: &mut Box<dyn Child + Send + Sync>) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "sleepy never left");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_dry_run_takes_the_screen_and_gives_it_back() {
    let (_master, mut child, output) = sleepy(&["2s", "--dry-run"]);
    // The alternate screen: the sequence every terminal knows, so the
    // shell's scrollback survives sleepy's art.
    assert_said(
        &output,
        b"[?1049h",
        "no alternate screen",
        Duration::from_secs(15),
    );
    // The armed headline: a countdown that names its deadline, which
    // is what a typed clock time promises and a duration earns.
    assert_said(
        &output,
        b", at ",
        "no deadline on the headline",
        Duration::from_secs(15),
    );
    // Two seconds and a tick later, the dry run fires without sleeping,
    // reports, and leaves.
    assert!(wait_for_exit(&mut child).success());
    assert_said(
        &output,
        b"would have run:",
        "no dry-run report",
        Duration::from_secs(5),
    );
    // And the shell's screen comes back: the leave-sequence, the last
    // promise the guard keeps.
    assert_said(
        &output,
        b"[?1049l",
        "screen never restored",
        Duration::from_secs(5),
    );
}

#[test]
fn a_typed_q_leaves_the_screen_clean() {
    let (master, mut child, output) = sleepy(&["--dry-run"]);
    // Idle, the footer offers the keys; the one this test takes is q.
    // The key names are bold and the words plain, so the styles split
    // "ctrl+d" from " dry run" with escape codes - the words are the
    // contiguous part to look for.
    assert_said(&output, b"dry run", "no footer", Duration::from_secs(15));
    let mut writer = master.take_writer().unwrap();
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();
    assert!(wait_for_exit(&mut child).success());
    assert_said(
        &output,
        b"[?1049l",
        "screen never restored",
        Duration::from_secs(5),
    );
}
