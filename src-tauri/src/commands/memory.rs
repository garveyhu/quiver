//! 记忆库 IPC(DESIGN §6):CEO 注入权威事实、浏览 / 作废事实、读简报与近期 episode。
//! 写入走机械矛盾消解(§6.4 同主题低档作废);读全是只读快照。

use tauri::State;

use quiver_memory::{Brief, EpisodeRecord, FactRecord, NewFact};

use crate::{current_project, now_ms, AppState};

/// How many facts / episodes a brief carries (DESIGN §6 简报). Bounded so the
/// brief stays a compact context snapshot, not a memory dump.
const BRIEF_FACT_LIMIT: usize = 20;
const BRIEF_EPISODE_LIMIT: usize = 10;

/// CEO 给公司注入一条**权威事实**(§6.2 权威档):项目约束 / 已定方案 / 领域知识。存进
/// 记忆,注入经理简报 —— claude 经理决策时读得到(记忆 → 决策)。空文本拒绝。
#[tauri::command]
pub fn add_authoritative_fact(
    state: State<'_, AppState>,
    text: String,
    topic: Option<String>,
    importance: Option<i64>,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("事实内容不能为空".to_string());
    }
    // 主题(entity):同主题的旧低档事实会被这条权威事实机械作废(§6.4)。空 → 不框定主题、不作废。
    let entity = topic.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let project = current_project(&state)?.display().to_string();
    let mem = state.memory()?;
    let now = now_ms();
    let new_id = mem
        .insert_fact(&NewFact {
            project,
            scope: None,
            kind: "知识".to_string(),
            text,
            entities: None,
            entity: entity.clone(),
            importance: Some(importance.unwrap_or(9).clamp(1, 9)),
            valid_at: None,
            recorded_at: now,
            provenance: Some("CEO".to_string()),
            trust: "权威".to_string(),
            source_commit: None,
            source_episode_id: None,
        })
        .map_err(|e| format!("{e:#}"))?;
    // §6.4 机械矛盾消解:带主题时,作废同主题更低可信度的旧事实(如旧的员工汇报"用 RocksDB")。
    if entity.is_some() {
        let _ = mem.supersede_lower_same_entity(new_id, now);
    }
    Ok(())
}

/// 记忆库:当前项目所有**当前事实**(§6,未失效),供 CEO 浏览管理。
#[tauri::command]
pub fn get_memory_facts(state: State<'_, AppState>) -> Result<Vec<FactRecord>, String> {
    let project = current_project(&state)?.display().to_string();
    let mem = state.memory()?;
    mem.current_facts(&project).map_err(|e| format!("{e:#}"))
}

/// CEO 手动作废一条记错 / 过时的事实(§6)。失效不删,可追溯。
#[tauri::command]
pub fn retire_fact_cmd(state: State<'_, AppState>, fact_id: i64) -> Result<(), String> {
    let mem = state.memory()?;
    mem.invalidate_fact(fact_id, now_ms())
        .map_err(|e| format!("{e:#}"))?;
    Ok(())
}

/// The current project's memory brief (DESIGN §6): current-truth facts + recent
/// episodes, for the manager "简报书" panel. Empty (not an error) before any
/// memory is recorded — a fresh project simply has nothing yet.
#[tauri::command]
pub fn get_brief(state: State<'_, AppState>) -> Result<Brief, String> {
    let project = current_project(&state)?.display().to_string();
    let memory = state.memory()?;
    memory
        .brief(&project, BRIEF_FACT_LIMIT, BRIEF_EPISODE_LIMIT)
        .map_err(|e| format!("{e:#}"))
}

/// 当前项目近期 episode(DESIGN §6.2,最新在前,默认上限 50),供时间轴 / 档案 UI 回放
/// "发生过什么"。只读;记忆未初始化或项目未选时报错(由 UI 兜成空)。
#[tauri::command]
pub fn get_episodes(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<EpisodeRecord>, String> {
    let project = current_project(&state)?.display().to_string();
    let memory = state.memory()?;
    memory
        .episodes_for_project(&project, limit.unwrap_or(50))
        .map_err(|e| format!("{e:#}"))
}
