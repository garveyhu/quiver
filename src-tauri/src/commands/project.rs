//! 项目面 IPC(DESIGN §11):选/记 git 仓库、最近列表、别名,以及 verify 命令嗅探。
//! 当前项目状态(`project_path`)与校验逻辑(`select_validated_project`/`current_project`)
//! 留在 crate 根 —— 它们是 `AppState` 的内核、多模块共用。

use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use quiver_store::InitialState;

use crate::{current_project, select_validated_project, AppState};

/// Open a folder dialog, validate the chosen directory is a git repo, store it
/// in app state, and return its absolute path. `Ok(None)` if the user cancels.
#[tauri::command]
pub async fn pick_project(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let chosen = app.dialog().file().blocking_pick_folder();
    let Some(folder) = chosen else {
        return Ok(None);
    };
    let path: PathBuf = folder
        .into_path()
        .map_err(|e| format!("could not resolve the chosen folder: {e}"))?;
    select_validated_project(&state, path).map(Some)
}

/// Re-select a project the user picked before (from the recent list) WITHOUT the
/// folder dialog.
#[tauri::command]
pub fn select_recent_project(state: State<'_, AppState>, path: String) -> Result<String, String> {
    select_validated_project(&state, PathBuf::from(path))
}

/// Load the durable startup bundle (DESIGN §11): last project + recents + history.
#[tauri::command]
pub fn get_initial_state(state: State<'_, AppState>) -> Result<InitialState, String> {
    let store = state.store()?;
    let initial = store.initial_state().map_err(|e| format!("{e:#}"))?;
    if let Some(last) = &initial.last_project {
        *state.project_path.lock().expect("project_path lock") = Some(PathBuf::from(last));
    }
    Ok(initial)
}

/// Forget a project from the recent list (durable). Does not touch the currently
/// picked project — only the MRU entry is dropped.
#[tauri::command]
pub fn remove_recent_project(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let store = state.store()?;
    store
        .remove_recent_project(&path)
        .map_err(|e| format!("{e:#}"))
}

/// Set (or clear, with `None`) a recent project's display alias.
#[tauri::command]
pub fn set_project_alias(
    state: State<'_, AppState>,
    path: String,
    alias: Option<String>,
) -> Result<(), String> {
    let store = state.store()?;
    store
        .set_recent_project_alias(&path, alias.as_deref())
        .map_err(|e| format!("{e:#}"))
}

/// Suggest a verify-gate command (DESIGN §7) by sniffing the current project for
/// well-known build/test markers. Read-only; returns "" when nothing is
/// recognized or no project is selected, so the UI just shows no hint. Purely a
/// convenience nudge to help the user enable the gate.
#[tauri::command]
pub fn suggest_verify_command(state: State<'_, AppState>) -> Result<String, String> {
    let project = match current_project(&state) {
        Ok(p) => p,
        Err(_) => return Ok(String::new()),
    };
    let has = |f: &str| project.join(f).exists();
    let cmd = if has("Cargo.toml") {
        "cargo test"
    } else if has("package.json") {
        "npm test"
    } else if has("pyproject.toml") || has("setup.py") {
        "pytest"
    } else if has("go.mod") {
        "go test ./..."
    } else if has("Makefile") || has("makefile") {
        "make test"
    } else {
        ""
    };
    Ok(cmd.to_string())
}
