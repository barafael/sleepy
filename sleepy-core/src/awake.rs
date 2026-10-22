//! Keeping the screen awake while a countdown runs.
//!
//! A user who says "sleep in thirty minutes" may well mean "and let me
//! watch something until then" - a screen that blanks at minute ten has
//! broken the promise. The fix is the platform's own idle inhibitor, held
//! for as long as the countdown is and released the moment it is not.
//!
//! Each platform gets the inhibition from a helper process, and the helper
//! is tied to sleepy's life so a killed sleepy never leaves the screen
//! pinned: on Linux `cat` exits when our pipe closes, on macOS `caffeinate`
//! waits on our pid, and on Windows a PowerShell watches it too.
//!
//! The browser has no helper process to spawn; there the same promise is
//! kept with a Wake Lock, held while the countdown runs and re-asked when
//! the page comes back to the light - see the `web` module.

#[cfg(not(target_arch = "wasm32"))]
use std::process::{Child, ChildStdin, Command, Stdio};

/// The screen-awake inhibition, held or not.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default)]
pub struct KeepAwake {
    child: Option<Child>,
    /// Held open; closing it tells the helper the countdown is over.
    pipe: Option<ChildStdin>,
}

#[cfg(not(target_arch = "wasm32"))]
impl KeepAwake {
    /// An inhibition nobody is holding.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts holding the screen awake, replacing any inhibition already
    /// held. A platform with no helper, or a helper that will not start,
    /// is a shrug, not a veto: the countdown goes on and the screen may
    /// still blank.
    pub fn start(&mut self) {
        self.stop();
        let Some(mut command) = helper() else {
            return;
        };
        if let Ok(mut child) = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            self.pipe = child.stdin.take();
            self.child = Some(child);
        }
    }

    /// Lets go: the helper exits and the screen may blank again.
    pub fn stop(&mut self) {
        drop(self.pipe.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Whether the screen is being held awake right now.
    pub fn active(&self) -> bool {
        self.child.is_some()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for KeepAwake {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The helper process that holds the inhibition, if this platform has one.
#[cfg(not(target_arch = "wasm32"))]
fn helper() -> Option<Command> {
    if cfg!(target_os = "linux") {
        // logind lifts the idle block when the inhibited command exits;
        // `cat` exits when our pipe closes, which happens even on a kill.
        let mut command = Command::new("systemd-inhibit");
        command.args([
            "--what=idle",
            "--who=sleepy",
            "--why=sleepy is counting down to sleep",
            "--mode=block",
            "cat",
        ]);
        Some(command)
    } else if cfg!(target_os = "macos") {
        // -d holds the display awake; -w ties the holding to our pid.
        let mut command = Command::new("caffeinate");
        command.args(["-d", "-w", &std::process::id().to_string()]);
        Some(command)
    } else if cfg!(target_os = "windows") {
        // SetThreadExecutionState lasts for the calling process, so a
        // little PowerShell sets the display requirement (ES_CONTINUOUS |
        // ES_DISPLAY_REQUIRED) and then waits for our pid to disappear -
        // and releasing it is simply PowerShell getting to leave.
        let script = format!(
            "Add-Type -MemberDefinition '[DllImport(\"kernel32.dll\")] public static extern uint SetThreadExecutionState(uint f);' -Name E -Namespace Sleepy; [Sleepy.E]::SetThreadExecutionState(0x80000002) | Out-Null; while (Get-Process -Id {} -ErrorAction SilentlyContinue) {{ Start-Sleep 1 }}",
            std::process::id()
        );
        let mut command = Command::new("powershell");
        command.args([
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ]);
        Some(command)
    } else {
        None
    }
}

/// The screen-awake wish, held or not.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default)]
pub struct KeepAwake {
    /// Whether the countdown wants the screen; the Wake Lock itself is
    /// the browser's to hand out, and `web` keeps it while it is wanted.
    want: bool,
}

#[cfg(target_arch = "wasm32")]
impl KeepAwake {
    /// An inhibition nobody is holding.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the browser to keep the screen awake, from now until `stop`.
    /// A browser that will not grant the lock is a shrug, not a veto:
    /// the countdown goes on and the screen may still blank.
    pub fn start(&mut self) {
        self.want = true;
        crate::web::hold_screen(true);
    }

    /// Lets go: the Wake Lock is released and the screen may blank again.
    pub fn stop(&mut self) {
        self.want = false;
        crate::web::hold_screen(false);
    }

    /// Whether the screen is being held awake right now - which on the
    /// web means "wanted": the browser may still blank a hidden page.
    pub fn active(&self) -> bool {
        self.want
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for KeepAwake {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
// A test that cannot get its answer is broken, not a machine that said no.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn letting_go_of_nothing_is_quiet() {
        let mut awake = KeepAwake::new();
        awake.stop();
        assert!(!awake.active());
    }

    #[test]
    fn starting_holds_or_shrugs_and_stopping_always_lets_go() {
        // Whether a helper exists is the machine's business; the contract
        // is only that stopping always returns to not holding, and that
        // starting again replaces rather than doubles up.
        let mut awake = KeepAwake::new();
        awake.start();
        awake.start();
        awake.stop();
        assert!(!awake.active());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_inhibits_the_idle_action_through_systemd() {
        let command = helper().expect("systemd-inhibit is the linux helper");
        assert_eq!(command.get_program(), "systemd-inhibit");
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--what=idle".to_string()));
        assert!(args.contains(&"--mode=block".to_string()));
        // `cat` is the leash: it lives while our pipe does.
        assert_eq!(args.last().map(String::as_str), Some("cat"));
    }
}
