# Quiver — guidance for Claude

Quiver is an open-source macOS desktop app: a **supervisor for headless coding
agents** with a cozy pixel-office skin. One task = one git worktree = one detached
agent process = one pixel-worker sprite. It runs overnight, governs a dollar
budget, and gates every merge behind a verify step.

Full design: **`docs/DESIGN.md`** (large, §-numbered — cite sections as `DESIGN §N`).

## Stack & layout

- **Tauri v2** (Rust core) + **React/TypeScript** + **Phaser** (pixel UI) + **SQLite**.
- `src-tauri/` — the Tauri app crate (`quiver-app`).
  - `src/lib.rs` — the `tauri::Builder`, app state, and the IPC commands.
  - `src/run.rs` — per-task run plumbing (binary resolution, live streaming, persistence).
  - `src/scheduler.rs` — the concurrent queue scheduler.
  - `src/dev_bridge.rs` — **debug-only** inspection bridge (see below).
- `crates/quiver-core/` — pure-Rust supervisor: worktrees, verify-gate, merge.
- `crates/quiver-store/` — durable SQLite persistence (picked project, recents, history).
- `frontend/` — React + Phaser, built with Vite (dev server on **:1420**).

## Build & run

```bash
cargo tauri dev          # build + launch (runs `yarn --cwd frontend dev` first)
cargo build -p quiver-app --bin Quiver   # just the Rust binary
yarn --cwd frontend dev  # frontend dev server only (:1420)
cargo check              # typecheck the Rust side
```

## Conventions

- IPC commands live in `src-tauri/src/lib.rs`, annotated `#[tauri::command]`,
  registered in `run()`'s `invoke_handler`. They return `Result<T, String>`;
  errors are surfaced to the UI as the `String`.
- User-facing strings (errors, labels) are in **Chinese** — match that.
- Keep the pure supervisor logic in `quiver-core` (no Tauri types); the app crate
  is the only place that touches `tauri::*`.

## Debugging the running app

To confirm a UI/behavior change in the **real running window** (not just tests),
drive it with **`scripts/agent-debug.sh`** — it wraps `tauri-agent-tools` against
the debug-only dev bridge baked into the app. Full guide: `docs/agent-debug.md`.

```bash
scripts/agent-debug.sh up                 # launch (handles the :1420 collision)
scripts/agent-debug.sh probe              # find the live bridge
scripts/agent-debug.sh state              # URL / title / viewport
scripts/agent-debug.sh dom 3              # DOM tree
scripts/agent-debug.sh eval 'document.title'   # run JS in the webview
scripts/agent-debug.sh shot               # full-window screenshot → /tmp/quiver-shot.png
scripts/agent-debug.sh shot ".board"      # element screenshot (needs ImageMagick)
scripts/agent-debug.sh ipc 8000           # tail Tauri IPC / console / rust-logs
```

Gotchas worth remembering:

- **Pixel screenshots need macOS Screen Recording permission** on the host
  terminal (and a terminal restart). DOM/eval/IPC/console/logs do **not** —
  they go over the bridge's localhost HTTP. `click`/`type`/`scroll` are
  eval-based, so they don't need Accessibility either.
- The bridge is **debug-only** (`#[cfg(debug_assertions)]`) — absent in release.
- Don't hand-edit `src-tauri/src/dev_bridge.rs`; it's vendored from
  `tauri-agent-tools`. Re-copy from the npm package to upgrade.

Prefer this over claiming a change works from code-reading alone: launch it,
inspect it, then report what you actually observed.
