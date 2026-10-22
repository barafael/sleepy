# sleepy

**Count down to sleep. The machine's, not yours.**

sleepy takes a when a human can read - a duration (`45s`, `30 min`, `1h30m`)
or a clock time (`23:45`, the next one to come) - counts down, and puts the
machine to sleep when the time comes. Pause, resume or abort while it runs;
`--dry-run` — or `Ctrl+D` in the terminal, or the checkbox in the window —
counts down for real and then only says what it would have run.
`--keep-screen-on` holds off screen blanking while the countdown runs, so you
can watch something until the moment comes. Two frontends share one core: a
terminal app and a window.

## Crates

| Crate | What it is |
|---|---|
| `sleepy-core` | The library: the command line both frontends share, the controller they drive - arm, pause, toggle, fire - sleep actions, capability detection, the timer, the screen guard, humantime and clock parsing. No terminal, no window - everything here is testable headless. |
| `sleepy` | The terminal app: ratatui and crossterm, no extra framework. |
| `sleepy-ui` | The window: eframe and egui, dressed in gruvbox - dark or light, as the desktop prefers. Operable by the terminal's keys or entirely by mouse; remembers its geometry. |

## Usage

```sh
sleepy                     # open the terminal and decide interactively
sleepy 30min               # arm thirty minutes of suspend, right away
sleepy 1h hibernate        # an hour, hibernating, if this machine can
sleepy 23:45               # sleep when the clock reads it next
sleepy 45s --dry-run       # count down, then report what would have run
sleepy 1h poweroff         # an hour, then off; `shutdown` names it too
sleepy 10m run-script      # when the time comes, run ./action.sh
sleepy 30min --keep-screen-on  # stay visible until the time comes
```

`sleepy-ui` takes the same command line, and the countdown keeps running
while its window is hidden - a minimised sleepy still puts the machine to
sleep on time.

A running countdown answers *when exactly?* on its headline: beside the
remaining time, `at 23:45` gives the clock's own answer - the same answer a
typed clock time promised.

Modes are `suspend`, `hibernate`, `hybrid-sleep`, `suspend-then-hibernate`
and `poweroff` (`shutdown` names it too); only the ones this machine offers
are ever on offer.

`run-script` is the odd one out: instead of sleeping, it runs a script you
left in the working directory - `action.sh` on Unix (run through its own
shebang), `action.ps1` through PowerShell on Windows. It is on offer only
while that file is there, and sleepy runs it with the same working directory
and standard streams it has itself; a non-zero exit is reported as the
script's own failure, not the machine's.

## Keys (terminal)

| Key | Action |
|---|---|
| typing | edit the when field: `30 min` or `23:45` |
| `Up` `Down` `Tab` | pick the mode |
| `Ctrl+D` | toggle the dry run: talk about sleeping, do nothing |
| `k` | toggle keep-awake: the screen stays on while it counts |
| `Enter` | arm the countdown |
| `Space` | pause, and pause again to resume |
| `+` `-` | stretch or shrink the deadline by a minute |
| `Backspace` / `Ctrl+U` | erase one character / clear the field |
| `Esc` | abort a running countdown, quit an idle one |
| `q` / `Ctrl+C` | quit |

The dry-run toggle can be flipped at any moment: switching to a dry run
mid-countdown saves the machine without aborting the timer. `Space` is
humantime's separator too, so it types while the field is being edited (idle)
and pauses once a countdown runs. The field takes only what a when could still
become - `23:`, `30 mi`, `1h3` - and refuses the rest; `k` is a command either
way, and the field never sees it.

## Keys (window)

The window answers to the same keys, and `Ctrl+D` toggles the dry run even
while the field has the keyboard, because a text field has no use for the
chord. While the field is focused it keeps only what a when can wear - space
types a space, `d` types a `d` - and what no when wears is a command either
way, as in the terminal: `k` toggles keep-awake, `q` quits, `+` `-` nudge the
deadline, and characters that complete no when the field simply refuses. `Esc`
leaves the field first, then aborts a countdown, then quits, one press at a
time.
Arming - `Enter`, the button, the hint - waits for an idle countdown: a
running one is never silently replaced. The status bar shows the keys as the
terminal's footer does, and each hint that stands for a single command can
be clicked instead of pressed.

## How sleepy knows what the machine can do

sleepy asks logind directly over D-Bus (`busctl call org.freedesktop.login1
/org/freedesktop/login1 org.freedesktop.login1.Manager CanSleep`), falling
back to `systemctl can-*` where logind has no answer. A `challenge` answer
counts as possible: polkit gets to ask for a password when the time comes,
not when the countdown starts.

## How sleepy keeps the screen on

Keeping the screen awake uses the platform's own inhibitor: logind's idle
block (`systemd-inhibit`) on Linux, `caffeinate` on macOS, and
`SetThreadExecutionState` on Windows. The helper process is tied to sleepy's
life - a pipe, a waited pid, a watched pid - so even a killed sleepy never
leaves the screen pinned, and letting go (abort, fire, quit) hands the
blanking back immediately.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

The MSRV is 1.95, which is what eframe asks for; the workspace says so
(`rust-version`) and the lints there say the house rules: no unsafe, no
unwraps outside tests.

The timer is handed its clock (`tick(now)`), so the whole state machine -
arm, pause, nudge, fire - is tested without waiting for anything. The
terminal app is exercised through a real pty (`sleepy/tests/pty.rs`): the
binary takes over a terminal it does not know is fake, and the test reads
the escape codes back - alternate screen on, deadline on the headline,
screen handed back clean. The window is exercised through its command line
and its key map; the pixels themselves are left to the human.

## License

sleepy is licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
