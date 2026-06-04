// Prevents an extra console window on Windows in release; harmless on macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // DESIGN §12, step 1: the VERY FIRST thing, before anything spawns a child.
    // A packaged .app launched from Finder inherits a sparse launchd PATH
    // (~`/usr/bin:/bin:/usr/sbin:/sbin`) with no Homebrew / ~/.claude/local, so
    // `git` and the agent binary would not be found. `fix_path_env::fix()`
    // repairs the process PATH to resemble a login shell's. Best-effort: a
    // failure here must not block startup, so we ignore the Result.
    let _ = fix_path_env::fix();

    quiver_app_lib::run();
}
