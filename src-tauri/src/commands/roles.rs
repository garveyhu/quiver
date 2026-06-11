//! 人事部 IPC(DESIGN §14 角色生命周期):列角色、雇/裁员工、增量改配置。
//! 改「经理」角色的 `brain` 即切换经理大脑(rule 免费 / claude 真想),下次循环启动生效。

use tauri::State;

use crate::{now_ms, AppState};

/// 人事部:全部角色配置(经理在前)。前端人事部 UI 的数据源(DESIGN §14)。
#[tauri::command]
pub fn list_roles(state: State<'_, AppState>) -> Result<Vec<quiver_store::AgentRole>, String> {
    let store = state.store()?;
    store.list_roles().map_err(|e| format!("{e:#}"))
}

/// 人事部:雇一个新员工(§14 角色生命周期)。生成唯一 id,默认免费规则配置,返回新角色行。
#[tauri::command]
pub fn hire_worker(state: State<'_, AppState>) -> Result<quiver_store::AgentRole, String> {
    let store = state.store()?;
    let now = now_ms();
    let role = quiver_store::AgentRole {
        id: format!("worker-{now}"),
        name: "新员工".to_string(),
        kind: "worker".to_string(),
        brain: "rule".to_string(),
        model: "sonnet".to_string(),
        system_prompt: String::new(),
        budget_usd: None,
        max_turns: None,
        specialty: "通用".to_string(),
        version: 1,
        updated_at: now,
    };
    store.create_role(&role).map_err(|e| format!("{e:#}"))?;
    Ok(role)
}

/// 人事部:裁掉一个员工(§14)。只能裁 kind=worker(经理/记忆官是单例,删不得)。
#[tauri::command]
pub fn fire_worker(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let store = state.store()?;
    if !store.delete_role(&id).map_err(|e| format!("{e:#}"))? {
        return Err("只能裁员工(经理/记忆官是单例岗位,删不得)".to_string());
    }
    Ok(())
}

/// 人事部:增量改一个角色(version+1),返回更新后的完整配置。改「经理」的 `brain` 字段
/// 即切换经理大脑(rule 免费 / claude 真想)——下次经理循环启动时生效。
#[tauri::command]
pub fn update_role(
    state: State<'_, AppState>,
    id: String,
    patch: quiver_store::RolePatch,
) -> Result<quiver_store::AgentRole, String> {
    let store = state.store()?;
    store
        .update_role(&id, &patch, now_ms())
        .map_err(|e| format!("{e:#}"))
}
