# Debugging the running Quiver app (agent-driven)

How to inspect and drive the **live Quiver window** — read its DOM, run JS in the
webview, watch IPC/console/Rust logs, and take screenshots — from the command
line or an AI coding agent (Claude Code, Cursor, …).

The everyday entry point is `scripts/agent-debug.sh`. The rest of this doc
explains what it wraps and why.

---

## Why the usual browser tooling doesn't work

Quiver's UI runs in **WKWebView** (macOS' system webview), not Chrome:

- Browser-automation tools speak the Chrome DevTools Protocol. WKWebView is not
  Chrome and does not speak CDP — they cannot attach.
- Tauri's official WebDriver (`tauri-driver`) **does not support macOS** (only
  Windows/WebView2 and Linux/WebKitGTK). The Selenium/WebDriver route is closed
  on a Mac.

So instead we embed a tiny **debug-only HTTP bridge** inside the app. A CLI
(`tauri-agent-tools`) talks to it to eval JS / read the DOM, and pixel
screenshots are handled by the OS (`screencapture`) plus ImageMagick cropping.

## What's wired into the app

The bridge is vendored from `tauri-agent-tools` and integrated so it is **only
present in debug builds** — neither the module nor its localhost HTTP server ship
in a release binary:

- `src-tauri/src/dev_bridge.rs` — the bridge (vendored; do not hand-edit, re-copy
  from the npm package to upgrade).
- `src-tauri/src/lib.rs` — `#[cfg(debug_assertions)] mod dev_bridge;`, the
  `start_bridge(app.handle())` call inside `setup()`, and a `#[cfg]`-split
  `invoke_handler` that registers `dev_bridge::__dev_bridge_result` only in debug.
- `src-tauri/Cargo.toml` — the bridge's dependencies (`tiny_http`, `tracing*`,
  `uuid`, `rand`, `scopeguard`, `libc`).

On startup in debug, the bridge binds a random localhost port and writes
`/tmp/tauri-dev-bridge-<pid>.token`; the CLI auto-discovers it from that file.

## One-time setup

```bash
npm install -g tauri-agent-tools   # the CLI
brew install imagemagick           # `magick`, needed to crop element screenshots
```

**macOS Screen Recording permission** — pixel screenshots need it. Grant it to
the terminal app that hosts your shell (Terminal / iTerm / Ghostty / VS Code),
then **fully quit and reopen** that app (the permission only takes effect after a
restart). Without it, `screencapture` fails with
`could not create image from display` and every `shot` / screenshot command dies.

> Interaction commands (`click` / `type` / `scroll`) dispatch DOM events through
> eval, so they do **not** need Accessibility permission — Screen Recording (for
> screenshots) is the only OS gate.

## Using `scripts/agent-debug.sh`

```bash
scripts/agent-debug.sh up                 # launch the app (handles the :1420 collision)
scripts/agent-debug.sh probe              # show the live bridge (pid, port, windows)
scripts/agent-debug.sh state              # page URL / title / viewport
scripts/agent-debug.sh dom 3              # DOM tree to depth 3
scripts/agent-debug.sh eval 'document.title'
scripts/agent-debug.sh shot               # full-window screenshot → /tmp/quiver-shot.png
scripts/agent-debug.sh shot ".board" /tmp/board.png   # element screenshot
scripts/agent-debug.sh ipc 8000           # tail Tauri IPC calls for 8s
scripts/agent-debug.sh console 8000       # tail webview console for 8s
scripts/agent-debug.sh logs 8000          # tail Rust backend logs for 8s
scripts/agent-debug.sh down               # stop an app this script launched
```

The script handles the annoying parts for you: it auto-targets the live bridge
PID (so you never pass `--pid`), puts Homebrew on `PATH` so `magick` resolves,
and prints an actionable hint if Screen Recording permission is missing.

### How `shot` works (and its one limit)

`tauri-agent-tools screenshot` is **not** used — its JXA window-enumeration is
broken on recent macOS (Tahoe), failing with `list.map is not a function`
regardless of permissions. So `shot` captures natively instead:

- It finds the app's window id via CoreGraphics and runs `screencapture -l <id>`.
- For a `--selector`, it crops that window capture to the element's
  `getBoundingClientRect()` (fetched over the bridge), deriving the retina scale
  and title-bar offset from the captured PNG plus the webview viewport.

**Limit:** element screenshots only work for elements currently **visible in the
viewport** — a window capture can't reach content scrolled off-screen. Scroll the
element into view first (`agent-debug.sh eval 'document.querySelector(".x").scrollIntoView()'`).

### `up` and the `:1420` collision

`cargo tauri dev` starts the Vite dev server on **:1420**. If one is already
running (you, or another `cargo tauri dev`), a second launch aborts with
`Port 1420 already in use`. So `up` checks the port:

- **:1420 free** → runs `cargo tauri dev` normally.
- **:1420 already serving** → builds the binary and runs
  `target/debug/Quiver` **directly**, reusing the existing dev server. This is
  also the way to test a freshly-built bridge while another instance is running.

For day-to-day work you can keep your own `cargo tauri dev` running; the
inspection commands work against whatever debug instance is live.

## Raw CLI

The script is a thin wrapper — anything it can't do, call `tauri-agent-tools`
directly (it has ~25 commands). Target a specific instance with `--pid <n>` and a
specific window with `--window-label <label>`; add `--json` for machine-readable
output. Always pass `--duration <ms>` to the monitors (`ipc-monitor`,
`console-monitor`, `rust-logs`) or they run until interrupted.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `probe` says no bridge | App not in debug mode, or crashed and left a stale token. `rm -f /tmp/tauri-dev-bridge-*.token`, then `up`. |
| `could not create image from display` | Grant Screen Recording to the terminal app and **restart it**. |
| `magick` not found | `brew install imagemagick`. |
| `Port 1420 already in use` | Another dev server is up — use `up` (it runs the binary directly) or stop the other instance. |

## Release safety

Everything bridge-related is behind `#[cfg(debug_assertions)]`, so it is compiled
out of release builds. The bridge's crates are still *compiled* in release (just
unused); to drop them entirely, make them `optional = true` behind a `dev-bridge`
feature enabled only in dev.
