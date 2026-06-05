#!/usr/bin/env bash
#
# agent-debug.sh — drive the running Quiver app for debugging (manual or AI-agent)
# via tauri-agent-tools and the debug-only dev bridge baked into src-tauri.
#
# The dev bridge (src-tauri/src/dev_bridge.rs, gated behind #[cfg(debug_assertions)])
# exposes a localhost HTTP server in debug builds. This script wraps the
# tauri-agent-tools CLI so you don't have to remember the live bridge PID, the
# Homebrew PATH for ImageMagick, or the dev-server port dance.
#
# Full guide: docs/agent-debug.md
#
# Usage:
#   scripts/agent-debug.sh up                 launch the app (handles the :1420 collision)
#   scripts/agent-debug.sh down               stop an app this script launched
#   scripts/agent-debug.sh probe              show the live bridge (pid, port, windows)
#   scripts/agent-debug.sh dom [depth]        print the DOM tree (default depth 3)
#   scripts/agent-debug.sh eval '<js>'        run JS in the webview, print the result
#   scripts/agent-debug.sh shot [sel] [out]   screenshot: full window, or a CSS-selected element
#   scripts/agent-debug.sh state              page URL / title / viewport
#   scripts/agent-debug.sh ipc [ms]           tail Tauri IPC calls (default 5000ms)
#   scripts/agent-debug.sh console [ms]       tail webview console (default 5000ms)
#   scripts/agent-debug.sh logs [ms]          tail Rust backend logs (default 5000ms)
#
set -euo pipefail

APP_NAME="Quiver"
DEV_PORT=1420
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="/tmp/quiver-agent-debug.log"
PIDFILE="/tmp/quiver-agent-debug.pid"
# Homebrew bin so `magick` (ImageMagick — needed to crop element screenshots) resolves
# even in a non-login shell.
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

have() { command -v "$1" >/dev/null 2>&1; }

# Echo the PID of the first LIVE Quiver dev bridge (from its /tmp token file), or
# return 1 if none is running.
bridge_pid() {
  local f p
  for f in /tmp/tauri-dev-bridge-*.token; do
    [ -e "$f" ] || continue
    p="${f##*tauri-dev-bridge-}"
    p="${p%.token}"
    if kill -0 "$p" 2>/dev/null; then
      echo "$p"
      return 0
    fi
  done
  return 1
}

# Remove only token files whose owning process is dead — never touch a live
# instance's token (the bridge cleans up its own token on a clean exit).
prune_stale_tokens() {
  local f p
  for f in /tmp/tauri-dev-bridge-*.token; do
    [ -e "$f" ] || continue
    p="${f##*tauri-dev-bridge-}"
    p="${p%.token}"
    kill -0 "$p" 2>/dev/null || rm -f "$f"
  done
}

# Echo the CGWindow id of the app's largest on-screen window, or nothing if it
# isn't running. Screen-Recording-gated. We deliberately read only the id (the
# kCGWindowBounds CFDictionary doesn't survive JXA's bridge on recent macOS, and
# `screencapture -l <id>` needs only the id anyway).
quiver_window_id() {
  osascript -l JavaScript -e '
    ObjC.import("CoreGraphics"); ObjC.import("Foundation");
    var ref = $.CGWindowListCopyWindowInfo(
      $.kCGWindowListOptionOnScreenOnly | $.kCGWindowListExcludeDesktopElements, 0);
    var arr = ObjC.castRefToObject(ref), n = arr.count, best = "";
    for (var i = 0; i < n; i++) {
      var w = arr.objectAtIndex(i);
      var o = w.objectForKey("kCGWindowOwnerName"); o = o ? ObjC.unwrap(o) : "";
      if (o.indexOf("'"$APP_NAME"'") < 0) continue;
      best = "" + ObjC.unwrap(w.objectForKey("kCGWindowNumber"));
      break; // first (front-most) match is the main window
    }
    best;
  ' 2>/dev/null
}

require_cli() {
  have tauri-agent-tools || {
    echo "✗ 缺 tauri-agent-tools。安装：npm install -g tauri-agent-tools" >&2
    exit 1
  }
}

require_bridge() {
  require_cli
  PID="$(bridge_pid || true)"
  [ -n "${PID:-}" ] || {
    echo "✗ 没有运行中的 Quiver bridge。先 \`scripts/agent-debug.sh up\`（或自行 cargo tauri dev）。" >&2
    exit 1
  }
}

# Probe whether the host terminal has macOS Screen Recording permission. Without
# it, every pixel-capture path fails with "could not create image from display".
require_screen_recording() {
  local probe="/tmp/.quiver-screencap-probe.png"
  if ! screencapture -x "$probe" 2>/dev/null; then
    echo "✗ 抓屏被系统拒绝（缺「屏幕录制」权限）。" >&2
    echo "  系统设置 → 隐私与安全性 → 屏幕录制 → 勾选你的终端 App，然后完全退出并重开终端。" >&2
    exit 1
  fi
  rm -f "$probe"
}

cmd_up() {
  require_cli
  if pid="$(bridge_pid || true)" && [ -n "${pid:-}" ]; then
    echo "✓ bridge 已在运行 pid=$pid（无需重启）"
    return 0
  fi
  prune_stale_tokens
  if lsof -nP -iTCP:"$DEV_PORT" -sTCP:LISTEN >/dev/null 2>&1; then
    # A dev server already owns :1420 (another `cargo tauri dev`, or `yarn dev`).
    # Starting a second one aborts on the port — so build and run the binary
    # directly, reusing the existing dev server.
    echo "• :$DEV_PORT 已被占用 → 编译并直接运行二进制，复用现有 dev server"
    ( cd "$REPO_ROOT" && cargo build -p quiver-app --bin "$APP_NAME" )
    nohup "$REPO_ROOT/target/debug/$APP_NAME" >"$LOG" 2>&1 &
    echo $! >"$PIDFILE"
  else
    echo "• 无 dev server → cargo tauri dev"
    ( cd "$REPO_ROOT" && nohup cargo tauri dev >"$LOG" 2>&1 & echo $! >"$PIDFILE" )
  fi
  echo "  启动中… 日志：$LOG"
  local i
  for i in $(seq 1 90); do
    if pid="$(bridge_pid || true)" && [ -n "${pid:-}" ]; then
      echo "✓ bridge 就绪 pid=$pid"
      return 0
    fi
    sleep 1
  done
  echo "✗ 等待 bridge 超时。查看日志：$LOG" >&2
  exit 1
}

cmd_down() {
  if [ -f "$PIDFILE" ]; then
    local p
    p="$(cat "$PIDFILE")"
    if kill -0 "$p" 2>/dev/null; then
      kill "$p" 2>/dev/null || true
      echo "✓ 已停止本脚本启动的实例 pid=$p"
    fi
    rm -f "$PIDFILE"
  else
    echo "• 没有由本脚本启动的实例记录（不会动你手动起的 cargo tauri dev）"
  fi
  # Only sweep tokens whose process is gone — leave live instances discoverable.
  prune_stale_tokens
}

cmd_probe() {
  require_cli
  tauri-agent-tools probe --json
}

cmd_dom() {
  require_bridge
  tauri-agent-tools dom --pid "$PID" --depth "${1:-3}"
}

cmd_eval() {
  require_bridge
  [ -n "${1:-}" ] || { echo "用法：agent-debug.sh eval '<js 表达式>'" >&2; exit 1; }
  tauri-agent-tools eval --pid "$PID" "$1"
}

# Screenshot the live app window natively. `tauri-agent-tools screenshot` is
# avoided on purpose: its JXA window-enumeration is broken on recent macOS. We
# capture the whole window by id via `screencapture -l`; with a selector we crop
# that capture to the element's rect (fetched over the bridge).
cmd_shot() {
  require_screen_recording
  local sel="${1:-}" out="${2:-/tmp/quiver-shot.png}"
  local id
  id="$(quiver_window_id)"
  [ -n "$id" ] || { echo "✗ 没找到 $APP_NAME 窗口（应用没在运行？先 \`up\`）。" >&2; exit 1; }

  if [ -z "$sel" ]; then
    screencapture -l"$id" -o "$out"
    echo "✓ 已保存整窗截图：$out"
    return 0
  fi

  # Element crop: capture the window, then crop to the element's rect. The crop
  # math derives the retina scale and title-bar height from the captured PNG plus
  # the webview viewport (over the bridge) — no fragile CGWindowBounds needed.
  #   scale     = png_width  / viewport_width      (no horizontal window chrome)
  #   titlebar  = png_height / scale - viewport_h  (window content height − viewport)
  require_bridge
  have magick || { echo "✗ 缺 ImageMagick（元素裁剪需要）：brew install imagemagick" >&2; exit 1; }
  # NOTE: `screencapture -l` refuses dot-prefixed (hidden) output paths.
  local winshot="/tmp/quiver-agent-winshot.png"
  screencapture -l"$id" -o "$winshot"

  local rect vp
  rect="$(tauri-agent-tools eval --pid "$PID" \
    "(function(){var e=document.querySelector('$sel');if(!e)return'';var r=e.getBoundingClientRect();return[Math.round(r.x),Math.round(r.y),Math.round(r.width),Math.round(r.height)].join(' ');})()" \
    2>/dev/null | tr -d '"')"
  [ -n "$rect" ] || { echo "✗ 选择器没匹配到元素：$sel" >&2; exit 1; }
  vp="$(tauri-agent-tools eval --pid "$PID" \
    "window.innerWidth+' '+window.innerHeight" 2>/dev/null | tr -d '"')"

  local png_w png_h rx ry rw rh vw vh
  png_w="$(magick identify -format '%w' "$winshot")"
  png_h="$(magick identify -format '%h' "$winshot")"
  read -r rx ry rw rh <<<"$rect"
  read -r vw vh <<<"$vp"
  local geom
  geom="$(awk -v pw="$png_w" -v ph="$png_h" -v vw="$vw" -v vh="$vh" \
              -v rx="$rx" -v ry="$ry" -v rw="$rw" -v rh="$rh" 'BEGIN{
        s = pw / vw; tb = ph / s - vh;
        printf "%dx%d+%d+%d", rw*s, rh*s, rx*s, (ry+tb)*s;
      }')"
  magick "$winshot" -crop "$geom" +repage "$out"
  echo "✓ 已保存元素截图（${sel}）：${out}"
}

cmd_state() {
  require_bridge
  tauri-agent-tools page-state --pid "$PID" --json
}

cmd_ipc() {
  require_bridge
  tauri-agent-tools ipc-monitor --pid "$PID" --duration "${1:-5000}"
}

cmd_console() {
  require_bridge
  tauri-agent-tools console-monitor --pid "$PID" --duration "${1:-5000}"
}

cmd_logs() {
  require_bridge
  tauri-agent-tools rust-logs --pid "$PID" --duration "${1:-5000}"
}

usage() {
  # Print the leading comment banner (every line until the first non-comment).
  awk 'NR>1 && /^#/ {sub(/^# ?/, ""); print; next} NR>1 {exit}' "${BASH_SOURCE[0]}"
}

main() {
  local sub="${1:-}"
  [ $# -gt 0 ] && shift || true
  case "$sub" in
    up)      cmd_up "$@" ;;
    down)    cmd_down "$@" ;;
    probe)   cmd_probe "$@" ;;
    dom)     cmd_dom "$@" ;;
    eval)    cmd_eval "$@" ;;
    shot)    cmd_shot "$@" ;;
    state)   cmd_state "$@" ;;
    ipc)     cmd_ipc "$@" ;;
    console) cmd_console "$@" ;;
    logs)    cmd_logs "$@" ;;
    ""|-h|--help|help) usage ;;
    *) echo "未知命令：$sub" >&2; usage; exit 1 ;;
  esac
}

main "$@"
