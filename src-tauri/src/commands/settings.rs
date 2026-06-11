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
        state.manager.resume_all(app.clone(), store.clone()).await;
        // 「给个方向就自治」:CEO 刚开自治、却还没派过任何活时,current project 从没 ensure_running
        // 注册过经理循环 → resume_all 没它可重启,自治目标推不动(经理循环根本没起来)。这里主动
        // ensure_running 当前项目,让**纯目标驱动**的绝对自治也能启动(派过活的项目 ensure_running
        // 幂等 no-op,不会重复起循环)。
        if let Ok(project) = crate::current_project(&state) {
            let max_workers = (settings.max_workers as usize).max(1);
            state
                .manager
                .ensure_running(app, store.clone(), project, max_workers)
                .await;
        }
    }
    Ok(settings)
}
