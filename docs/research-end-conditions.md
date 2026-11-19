# Research: agent sessions as end-conditions

Sleepy counts down to a moment a human chose. The next moment worth waiting
for is one the machine chooses: *the agent session is over*. If sleepy can
watch a Claude Code or OpenCode session, it can put the machine to sleep when
the agent finishes, instead of guessing a duration up front.

This document is the research behind that idea: how each tool lets an outside
observer notice that a session has **ended** (hard, one-shot) or has **gone
idle** (soft — the agent finished a turn and nobody typed). Idle deserves a
grace countdown before firing; sleepy's existing arm → countdown → fire model
fits both as "arm on trigger, optionally with a grace period".

Verified against: Claude Code hooks reference (Sep 2026, v2.1.x era),
OpenCode V2 docs and `openapi.json`, live probes against a running
opencode v2.1.3 service, and `@opencode/client@2.1.4`.

## Claude Code

### 1. Hooks — the first-class, push-based surface

Hooks are user-defined handlers configured in `~/.claude/settings.json`
(global), `.claude/settings.json` (project, committable), or
`.claude/settings.local.json`. The full reference:
<https://code.claude.com/docs/en/hooks>. The events that matter here:

| Event | Fires | Use for sleepy |
|---|---|---|
| `SessionEnd` | Once, when the session terminates | hard end-condition |
| `Stop` | Every time Claude finishes responding (turn end) | idle signal — arm a grace countdown |
| `UserPromptSubmit` | When a new prompt is submitted | idle-reset — cancel the grace countdown |
| `Notification` (matcher `idle_prompt`) | ~60 s after Claude finished, only if the user hasn't typed and no background agent runs | built-in idle signal, but suppressed while the user appears at the terminal |

`SessionEnd` input (JSON on stdin for command hooks):

```json
{
  "session_id": "abc123",
  "transcript_path": "/Users/.../.claude/projects/.../<id>.jsonl",
  "cwd": "/Users/...",
  "hook_event_name": "SessionEnd",
  "reason": "other"
}
```

`reason` values: `clear` (`/clear`), `resume` (interactive `/resume`),
`logout`, `prompt_input_exit` (exited while prompt input was visible),
`other`. Hooks can't block termination and their JSON output is discarded —
they are pure side-effects, which is exactly what sleepy wants.

Design-shaping details:

- **`SessionEnd` hooks share a ~1.5-second budget.** Raisable with a
  per-hook `timeout` (up to 60 s) or `CLAUDE_CODE_SESSIONEND_HOOKS_TIMEOUT_MS`
  (milliseconds). So the hook must not run sleepy interactively — it should
  **signal a running sleepy watcher** (a `sleepy wake`-style subcommand over a
  Unix socket, or a file touch) or spawn a detached
  `setsid sleepy 5m --dry-run`-style countdown.
- **`Stop` input carries `background_tasks` and `session_crons`**, explicitly
  to distinguish "session is done" from "paused waiting for background work
  to wake it back up". Only treat Stop as idle when both arrays are empty.
  `Stop` also carries `last_assistant_message` and `stop_hook_active`.
- `Stop` does **not** fire on user interrupt; API errors fire `StopFailure`
  instead (with `error` types like `rate_limit`, `overloaded`). Decide
  whether a rate-limit-stalled session counts as idle — probably not.
- A `Stop` hook *can* block Claude from stopping (`decision: "block"`);
  sleepy must never do that — observe only.
- Handler types: `type: "command"` (event JSON on stdin), **`type: "http"`**
  (event JSON POSTed to a URL — sleepy could listen on localhost and skip
  shell scripts entirely), `mcp_tool`, `prompt`, `agent`. Command hooks
  support `async: true` for background runs.
- Hooks receive `session_id`, `transcript_path`, `cwd`, `permission_mode`,
  `hook_event_name` as common input fields; `$CLAUDE_PROJECT_DIR` is
  available for locating scripts.

Example — a SessionEnd hook that arms a five-minute sleepy countdown:

```json
{
  "hooks": {
    "SessionEnd": [
      {
        "matcher": "prompt_input_exit|logout|other",
        "hooks": [
          {
            "type": "command",
            "command": "setsid sleepy 5m >/dev/null 2>&1 &",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

Caveats:

- `/clear` and `/resume` fire `SessionEnd` while the process lives on —
  filter by `reason` if only real exits should count.
- Hooks run in-process: a SIGKILL'd `claude`, a destroyed terminal, or a
  crash likely never fires `SessionEnd`. Process watching is the fallback.

### 2. Process level — zero config

`claude -p "…"` (headless mode) exits when the run finishes, so a wrapper is
the whole integration:

```sh
claude -p "…" && sleepy 1m
```

For interactive sessions, watch the `claude` process — trivial if sleepy
spawns it (`pidfd`/`waitpid`), noisy otherwise (`pgrep` matches helper
processes too).

### 3. Transcript watching — heuristic fallback

Transcripts live at `~/.claude/projects/<project>/<session>.jsonl`; their
mtime is an idle heuristic. The docs warn the transcript is written
asynchronously and may lag the conversation, so this is a last resort, not a
trigger.

### Other surfaces (overkill here)

Claude Code also exports OpenTelemetry metrics/events, and the Claude Agent
SDK exposes session lifecycle programmatically. Both are heavier than hooks
and unnecessary for sleepy.

## OpenCode (V2)

OpenCode sessions are durable and server-owned: the TUI is just a client of
the background service, and closing it changes nothing. There is no "session
end" — **idle is the natural end-condition**. All four surfaces below were
verified against a live opencode v2.1.3 service.

### 1. `POST /api/experimental/session/{id}/wait` — the primitive

"Waits for a session agent loop to become idle." Returns `204 No Content`.
Verified: returns immediately for an already-idle session, blocks otherwise.

```sh
opencode api post /api/experimental/session/$SID/wait && sleepy 1m
```

Sleepy can spawn this per session-id and fire when it exits. Note the
`experimental` path segment — the API is marked experimental and may change.

### 2. `GET /api/session/active` — zero-config polling

Returns the map of currently-running sessions; an empty map means everything
is idle. Verified live:

```sh
$ opencode api get /api/session/active
{"data":{"ses_…":{"type":"running"}, …}}
```

This is the easiest integration: a read-only poll, no third-party config,
and shelling out to `opencode api` matches sleepy's existing
`busctl`/`systemd-inhibit` style. It also handles the server's auth (see
caveats).

### 3. `GET /api/event` — the SSE event stream

The push surface. The busy/idle model, confirmed from the generated client's
contract (the client itself maps these events to running/idle):

- `session.execution.started` → session is running;
- `session.execution.succeeded` / `session.execution.failed` /
  `session.execution.interrupted` → session is idle, with the outcome.

Each event carries `data.sessionID`. Special case: `interrupted` with
`reason: "shutdown"` is the server shutting down — not a "done, sleep now"
signal. An idle transition also appends a `Session.Message.Idle` message
(`outcome: succeeded | failed | interrupted`) to the session context.

### 4. A plugin — push without polling

A `@opencode/plugin` runs inside the server and can subscribe to the event
stream or wait on sessions, then poke sleepy:

```ts
import { Plugin } from "@opencode/plugin"

export default Plugin.define({
  id: "sleepy",
  async setup(ctx) {
    const controller = new AbortController()
    void (async () => {
      for await (const event of ctx.event.subscribe({ signal: controller.signal })) {
        if (event.type === "session.execution.succeeded") {
          // spawn sleepy, or write to its socket
        }
      }
    })()
    return () => controller.abort()
  },
})
```

Push-based and precise, but it asks the user to install something.
`ctx.session.wait({ sessionID })` is also available.

### Also useful

- `Session.Info.time.idle` — the persisted timestamp of when the session
  last went idle; `Session.Info.outcome` (`succeeded | failed | interrupted`)
  — its last outcome. A poller can read both from `GET /api/session/{id}`.
- `opencode run "…"` is non-interactive and exits when the assistant
  finishes — the same wrapper trick as `claude -p`:

  ```sh
  opencode run "…" && sleepy 1m
  ```

### Caveats

- **Auth**: the server rejects unauthenticated requests (`curl` gets a 401
  `UnauthorizedError`). `opencode api …` handles discovery and auth — sleepy
  should shell out to it rather than talk to the port directly.
- A session blocked on a **permission prompt** likely still counts as
  "running" (the agent loop is blocked, not idle), so sleepy would not sleep
  while a prompt waits for a human. Decide explicitly whether that is the
  desired behaviour.
- `/api/session/active` is server-wide; sleepy may want to scope to one
  session id or one directory (`GET /api/session?directory=…`).
- The `wait` endpoint 404s (`LocationNotFoundError`) for sessions whose
  location isn't loaded on the server — e.g. a fresh session created via the
  API that was never prompted or viewed.

## Mapping it into sleepy

- **OpenCode idle** — the easiest win: poll `opencode api get
  /api/session/active`, or `wait` on a known session id. Read-only, no
  config, fits sleepy's "ask the machine directly" architecture.
- **Claude end** — a `SessionEnd` hook that pokes a running sleepy watcher
  (Unix socket, or an `http` hook posting to a localhost listener sleepy
  owns), respecting the 1.5-second budget.
- **Idle, either tool** — arm a grace countdown on `Stop` /
  `session.execution.*`; cancel it on `UserPromptSubmit` / a new execution.
  The grace period matters: these events fire while the human is still
  reading the terminal.
- Reliability ranking: push (hooks / events) > blocking `wait` > polling >
  transcript mtime.

A sketch of the core API: a watcher trait beside the existing timer —
`ClaudeHooks::signal()`, `OpencodePoll::tick(now)`, `OpencodeWait::spawn(id)`
— each feeding the controller the same `Trigger::Ended | Trigger::Idled`
events the countdown already understands, so arm/pause/nudge/fire stay
untouched and stay testable headless.

## Sources

- Claude Code hooks reference — <https://code.claude.com/docs/en/hooks>
  (SessionEnd, Stop, StopFailure, Notification sections)
- Claude Code hooks guide — <https://code.claude.com/docs/en/hooks-guide>
- OpenCode V2 API reference — <https://opencode.ai/v2/docs/api> and the raw
  OpenAPI document at <https://opencode.ai/v2/openapi.json>
- OpenCode V2 plugins guide — <https://opencode.ai/v2/docs/build/plugins>
- OpenCode V2 CLI commands (`run`, `api`, `service`) —
  <https://opencode.ai/v2/docs/cli/commands/>
- Live probes (2026-10-24) against opencode v2.1.3: `GET /api/session/active`
  (running map), `POST /api/experimental/session/{id}/wait` (immediate 204
  when idle), direct-port requests without auth (401)
- `@opencode/client@2.1.4` contract source — the `session.execution.*` →
  running/idle mapping
