# sleepy

**Count down to sleep. The machine's, not yours.**

sleepy takes a duration a human can read (`45s`, `30 min`, `1h30m`), counts
down, and puts the machine to sleep when the time comes. Pause, resume or
abort while it runs; `--dry-run` — or `d` in the terminal, or the checkbox in
the window — counts down for real and then only says what it would have run.
`--keep-screen-on` holds off screen blanking while the countdown runs, so
you can watch something until the moment comes. Two frontends share one
core: a terminal app and a window.

## Crates

| Crate | What it is |
|---|---|
| `sleepy-core` | The library: sleep actions, capability detection, the timer, the screen guard, humantime parsing. No argv, no terminal, no window - everything here is testable headless. |
| `sleepy` | The terminal app: ratatui and crossterm, no extra framework. |
| `sleepy-ui` | The window: eframe and egui, dressed in gruvbox - dark or light, as the desktop prefers. Operable by the terminal's keys or entirely by mouse; remembers its geometry. |

## Usage

```sh
sleepy                     # open the terminal and decide interactively
sleepy 30min               # arm thirty minutes of suspend, right away
sleepy 1h hibernate        # an hour, hibernating, if this machine can
sleepy 45s --dry-run       # count down, then report what would have run
sleepy 1h poweroff         # an hour, then off; `shutdown` names it too
sleepy 30min --keep-screen-on  # stay visible until the time comes
```

`sleepy-ui` takes the same command line, and the countdown keeps running
while its window is hidden - a minimised sleepy still puts the machine to
sleep on time.

Modes are `suspend`, `hibernate`, `hybrid-sleep`, `suspend-then-hibernate`
and `poweroff` (`shutdown` names it too); only the ones this machine offers
are ever on offer.

## Keys (terminal)

| Key | Action |
|---|---|
| typing | edit the duration field |
| `Up` `Down` `Tab` | pick the mode |
| `d` | toggle the dry run: talk about sleeping, do nothing |
| `k` | toggle keep-awake: the screen stays on while it counts |
| `Enter` | arm the countdown |
| `Space` | pause, and pause again to resume |
| `+` `-` | stretch or shrink the deadline by a minute |
| `Backspace` / `Ctrl+U` | erase one character / clear the field |
| `Esc` | abort a running countdown, quit an idle one |
| `q` / `Ctrl+C` | quit |

## Keys (window)

The window answers to the same keys, with one difference the terminal could
not afford: while the duration field is focused it keeps its letters - `d`
types a day, space types a space - and only the keys a text field has no use
for (`Up` `Down` `Enter` `Esc`) are commands either way. `Esc` leaves the
field first, then aborts a countdown, then quits, one press at a time. The
status bar shows the keys as the terminal's footer does, and each hint that
stands for a single command can be clicked instead of pressed.

## How sleepy knows what the machine can do

systemd 262 removed `systemctl can-sleep`, so sleepy asks logind directly
over D-Bus (`busctl call org.freedesktop.login1 /org/freedesktop/login1
org.freedesktop.login1.Manager CanSleep`) and falls back to `systemctl
can-*` on older systems. A `challenge` answer counts as possible: polkit
gets to ask for a password when the time comes, not when the countdown
starts.

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

The timer is handed its clock (`tick(now)`), so the whole state machine -
arm, pause, nudge, fire - is tested without waiting for anything. The
terminal app is exercised through a real pty in a scratch harness, the
window through its command line and its key map; the pixels themselves are
left to the human.

## License

sleepy is licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
