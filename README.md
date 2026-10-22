# sleepy

**Count down to sleep. The machine's, not yours.**

sleepy takes a duration a human can read (`45s`, `30 min`, `1h30m`), counts
down, and puts the machine to sleep when the time comes. Pause, resume or
abort while it runs; `--dry-run` counts down for real and then only says what
it would have run. Two frontends share one core: a terminal app and a window.

## Crates

| Crate | What it is |
|---|---|
| `sleepy-core` | The library: sleep actions, capability detection, the timer, humantime parsing. No argv, no terminal, no window - everything here is testable headless. |
| `sleepy` | The terminal app: ratatui and crossterm, no extra framework. |
| `sleepy-ui` | The window: eframe and egui. Remembers its geometry and follows the desktop's light/dark preference. |

## Usage

```sh
sleepy                     # open the terminal and decide interactively
sleepy 30min               # arm thirty minutes of suspend, right away
sleepy 1h hibernate        # an hour, hibernating, if this machine can
sleepy 45s --dry-run       # count down, then report what would have run
```

`sleepy-ui` takes the same command line, and the countdown keeps running
while its window is hidden - a minimised sleepy still puts the machine to
sleep on time.

Modes are `suspend`, `hibernate`, `hybrid-sleep` and `suspend-then-hibernate`;
only the ones this machine offers are ever on offer.

## Keys (terminal)

| Key | Action |
|---|---|
| typing | edit the duration field |
| `Up` `Down` `Tab` | pick the mode |
| `Enter` | arm the countdown |
| `Space` | pause, and pause again to resume |
| `+` `-` | stretch or shrink the deadline by a minute |
| `Backspace` / `Ctrl+U` | erase one character / clear the field |
| `Esc` | abort a running countdown, quit an idle one |
| `q` / `Ctrl+C` | quit |

## How sleepy knows what the machine can do

systemd 262 removed `systemctl can-sleep`, so sleepy asks logind directly
over D-Bus (`busctl call org.freedesktop.login1 /org/freedesktop/login1
org.freedesktop.login1.Manager CanSleep`) and falls back to `systemctl
can-*` on older systems. A `challenge` answer counts as possible: polkit
gets to ask for a password when the time comes, not when the countdown
starts.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

The timer is handed its clock (`tick(now)`), so the whole state machine -
arm, pause, nudge, fire - is tested without waiting for anything. The
terminal app is exercised through a real pty in a scratch harness, and the
window through its command line; the pixels themselves are left to the
human.

## License

sleepy is licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
