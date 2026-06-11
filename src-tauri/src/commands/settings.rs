//! 设置面 IPC(DESIGN §11):读 / 增量改 app 设置。改设置后重踢两条调度路径
//! (调高/清预算后让被 §10 预算闸停掉的队列复活)。

use tauri::{AppHandle, State};

use quiver_store::{Settings, SettingsPatch};

use crate::AppState;

/// Read the typed app settings (DESIGN §11, v1.0 module 1).
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let store = state.store()?;
    store.get_settings().map_err(|e| format!("{e:#}"))
}

/// Apply a partial settings update and return the resulting full [`Settings`].
///
/// After persisting, re-kick the scheduler: a settings change may raise the §10
/// budget cap, which should un-pause any queue the gate stopped — `resume_all`
/// re-checks the gate for each known project (no-op for queues already running).
#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let store = state.store()?;
    let settings = store.update_settings(&patch).map_err(|e| format!("{e:#}"))?;
    // 两条调度路径都重踢:旧 scheduler 总是;自治开着时经理循环也复活(调高/清预算后继续派活)。
    // 经理循环复用各 project 首次注册时存的大脑,故这里无需再传。
    state.scheduler.resume_all(app.clone(), store.clone()).await;
    if settings.autonomous {
        state.manager.resume_all(app, store.clone()).await;
    }
    Ok(settings)
}
