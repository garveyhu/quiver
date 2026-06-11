//! 经理 ctx 组装的只读 helper(§5/§6):从 store / 记忆库读出经理这一拍要看的局面片段 ——
//! 记忆简报、排队数、团队专长、队首任务、预算剩余。纯查询、无副作用,与编排状态机解耦,
//! 单拎一处方便各自演进(改预算口径、换简报选取策略都不动核心循环)。

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use quiver_store::{Settings, Store};

/// 没设夜预算时视为充裕(不因预算挡经理),与 `manager_preview` 同口径。
const BUDGET_UNCAPPED: f64 = 1_000_000.0;
const DAY_MS: i64 = 86_400_000;

/// 组装项目记忆简报文本(§6):前 8 条当前事实 + 近 6 条 episode。记忆是加性依赖 ——
/// MemoryStore 未装好/读错都返回空串,绝不挡经理决策(操作真值只来自 orchestration store)。
pub(super) fn memory_brief(app: &AppHandle, project_key: &str) -> String {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return String::new();
    };
    let Some(mem) = state.memory.get() else {
        return String::new();
    };
    mem.brief(project_key, 8, 6)
        .map(|b| b.to_text())
        .unwrap_or_default()
}

/// 某 project 当前排队任务数。
pub(super) fn count_queued(store: &Arc<Store>, project_key: &str) -> usize {
    store
        .list_tasks(Some(project_key), Some("queued"))
        .map(|v| v.len())
        .unwrap_or(0)
}

/// 团队成员清单(§14):员工 · 专长 · 战绩 · 当前负载 —— 让经理派活时知道谁擅长、谁靠谱、谁正忙,
/// 把活派给「对口 + 靠谱 + 此刻有空」的人(聪明协作的全局视角,而非闷头堆给一个人)。
pub(super) fn team_specialties(store: &Arc<Store>) -> Vec<String> {
    // 战绩:每个员工干过多少、成了多少 —— 让经理拆活/派活时不只知道谁擅长,还知道谁靠谱。
    let perf = crate::run::worker_perf(store);
    // 负载:每个员工此刻手上有几个在跑的活 —— 让经理别把新活堆给已经在忙的人。
    let load = worker_load(store);
    store
        .list_roles()
        .map(|roles| {
            roles
                .into_iter()
                .filter(|r| r.kind == "worker")
                .map(|r| {
                    let sp = if r.specialty.trim().is_empty() { "通用" } else { r.specialty.as_str() };
                    let record = match perf.get(&r.name) {
                        Some(&(total, ok)) if total > 0 => {
                            format!(" · 战绩 {ok}/{total}({}%)", ok * 100 / total)
                        }
                        _ => " · 暂无战绩".to_string(),
                    };
                    let busy = load.get(&r.name).copied().unwrap_or(0);
                    let load_str = if busy > 0 { format!(" · 在忙 {busy} 个") } else { " · 空闲".to_string() };
                    format!("{} · {sp}{record}{load_str}", r.name)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 每个员工此刻在跑的任务数(按 worker_role 聚合 running 任务)。读失败当全空,不挡决策。
fn worker_load(store: &Arc<Store>) -> std::collections::HashMap<String, usize> {
    let mut m: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    if let Ok(tasks) = store.list_tasks(None, Some("running")) {
        for t in tasks {
            if let Some(role) = t.worker_role {
                *m.entry(role).or_insert(0) += 1;
            }
        }
    }
    m
}

/// 队首任务原文(§5 A2):`list_tasks` 与 `claim_next_queued` 同序(position→时间),
/// 所以这里看到的第一条就是 Spawn 时会被认领的那条。
pub(super) fn peek_next_task(store: &Arc<Store>, project_key: &str) -> Option<String> {
    store
        .list_tasks(Some(project_key), Some("queued"))
        .ok()
        .and_then(|v| v.into_iter().next())
        .map(|t| t.prompt)
}

/// 经理可用预算剩余(美元):有夜预算 → 上限 − 近 24h 花费(夹 0);没设 → 视为充裕。
/// 与 `manager_preview` / scheduler 的 `over_budget` 同口径(§10)。
pub(super) fn budget_remaining(store: &Arc<Store>, settings: Option<&Settings>) -> f64 {
    let Some(settings) = settings else {
        return BUDGET_UNCAPPED;
    };
    match settings.nightly_budget_usd {
        Some(cap) if cap > 0.0 => {
            let spent = store.cost_since(crate::now_ms() - DAY_MS).unwrap_or(0.0);
            (cap - spent).max(0.0)
        }
        _ => BUDGET_UNCAPPED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_remaining_uncapped_without_nightly_budget() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        // 默认无夜预算 → 视为充裕(不因预算挡经理);settings 缺失同理。
        let settings = store.get_settings().unwrap();
        assert_eq!(budget_remaining(&store, Some(&settings)), BUDGET_UNCAPPED);
        assert_eq!(budget_remaining(&store, None), BUDGET_UNCAPPED);
    }

    #[test]
    fn budget_remaining_subtracts_spend_under_nightly_cap() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let mut settings = store.get_settings().unwrap();
        settings.nightly_budget_usd = Some(10.0);
        // 空库无花费 → 全额剩余;cap 为 0/负当未设(充裕)。
        assert_eq!(budget_remaining(&store, Some(&settings)), 10.0);
        settings.nightly_budget_usd = Some(0.0);
        assert_eq!(budget_remaining(&store, Some(&settings)), BUDGET_UNCAPPED);
    }
}
