//! 智能派活(§14):给一个任务挑员工 —— 先按专长命中任务描述缩到对口的人,再在候选里按历史
//! 战绩档挑(高成功率优先、同档 task_id 稳定散开做负载均衡、新人给中档机会)。纯逻辑、可单测。

use quiver_store::Store;

/// 给一个任务挑员工:专长命中 → 对口的人;再按战绩在候选里挑(见 [`pick_worker_from`])。
/// 无员工角色 → `None`。`pub(super)` 给父 `run` 模块的 `run_streaming` 调。
pub(super) fn pick_worker_role(
    store: &Store,
    task_id: &str,
    prompt: &str,
) -> Option<quiver_store::AgentRole> {
    let workers: Vec<quiver_store::AgentRole> = store
        .list_roles()
        .ok()?
        .into_iter()
        .filter(|r| r.kind == "worker")
        .collect();
    // §14 战绩感知:每个员工干过多少、成了多少 —— 派活时优先"对口且靠谱"的人。
    let perf = worker_perf(store);
    pick_worker_from(workers, task_id, prompt, &perf)
}

/// 每个员工的历史战绩 (总数, 成功数),从全部任务按 worker_role 聚合。成功 = verified/merged/done。
pub(crate) fn worker_perf(store: &Store) -> std::collections::HashMap<String, (u32, u32)> {
    let mut m: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
    if let Ok(tasks) = store.list_tasks(None, None) {
        for t in tasks {
            if let Some(role) = t.worker_role {
                let e = m.entry(role).or_insert((0, 0));
                e.0 += 1;
                if matches!(t.status.as_str(), "verified" | "merged" | "done") {
                    e.1 += 1;
                }
            }
        }
    }
    m
}

/// 纯逻辑(可单测):从一组员工里挑一个。**先按专长命中任务描述**缩到对口的人,没命中则用通用
/// 员工;再在候选里**按战绩档挑**(高成功率优先、同档 task_id 稳定散开做负载均衡、没干过算中档给
/// 新人机会)。于是"对的人 + 靠谱的人"接活 —— 配置 + 战绩 + 调度接起来。
fn pick_worker_from(
    workers: Vec<quiver_store::AgentRole>,
    task_id: &str,
    prompt: &str,
    perf: &std::collections::HashMap<String, (u32, u32)>,
) -> Option<quiver_store::AgentRole> {
    if workers.is_empty() {
        return None;
    }
    // 专长命中的员工(可能多个);命中就在这组里按战绩挑。
    let hits: Vec<quiver_store::AgentRole> = workers
        .iter()
        .filter(|r| {
            let s = r.specialty.trim();
            !s.is_empty() && s != "通用" && prompt.contains(s)
        })
        .cloned()
        .collect();
    if !hits.is_empty() {
        return pick_best(hits, task_id, perf);
    }
    // 无专长命中:优先通用员工(把专长员工留给专长活);一个通用都没有 → 退回全体。
    let generic: Vec<quiver_store::AgentRole> = workers
        .iter()
        .filter(|r| {
            let s = r.specialty.trim();
            s.is_empty() || s == "通用"
        })
        .cloned()
        .collect();
    let pool = if generic.is_empty() { workers } else { generic };
    pick_best(pool, task_id, perf)
}

/// 从候选池按战绩档挑:成功率 ≥70% 高档、≥40% 中档、否则低档;没干过算中档(给新人机会)。优先
/// 最高档,同档内按 task_id 稳定散开 —— 同一任务总派同一人 + 多任务在靠谱的人之间均衡负载。
fn pick_best(
    pool: Vec<quiver_store::AgentRole>,
    task_id: &str,
    perf: &std::collections::HashMap<String, (u32, u32)>,
) -> Option<quiver_store::AgentRole> {
    let tier = |r: &quiver_store::AgentRole| -> u8 {
        match perf.get(&r.name).copied() {
            Some((total, ok)) if total > 0 => {
                let pct = ok * 100 / total;
                if pct >= 70 {
                    3
                } else if pct >= 40 {
                    2
                } else {
                    1
                }
            }
            _ => 2, // 没干过:中档,有机会但不如已证明靠谱的
        }
    };
    let best = pool.iter().map(&tier).max()?;
    let top: Vec<quiver_store::AgentRole> = pool.into_iter().filter(|r| tier(r) == best).collect();
    let idx = task_id.bytes().map(usize::from).sum::<usize>() % top.len();
    top.into_iter().nth(idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker(id: &str, specialty: &str) -> quiver_store::AgentRole {
        quiver_store::AgentRole {
            id: id.into(),
            name: id.into(),
            kind: "worker".into(),
            brain: "rule".into(),
            model: "sonnet".into(),
            system_prompt: String::new(),
            budget_usd: None,
            max_turns: None,
            specialty: specialty.into(),
            version: 1,
            updated_at: 0,
        }
    }

    #[test]
    fn pick_worker_prefers_specialty_match() {
        let none = std::collections::HashMap::new(); // 无战绩 → 都中档,退回专长 + task_id 散开
        let team = vec![worker("w1", "通用"), worker("w2", "测试"), worker("w3", "前端")];
        // 任务描述含"测试" → 派给测试专长的 w2(不管 task_id)。
        let r = pick_worker_from(team.clone(), "task-abc", "给登录补端到端测试", &none).unwrap();
        assert_eq!(r.id, "w2", "专长命中优先");
        // 含"前端" → w3。
        let r = pick_worker_from(team.clone(), "task-abc", "给看板加前端暗色模式", &none).unwrap();
        assert_eq!(r.id, "w3");
        // 无专长命中 → 优先派通用员工 w1(留专长员工接专长活)+ 按 task_id 稳定(同 id 同人)。
        let a = pick_worker_from(team.clone(), "task-xyz", "清理废弃依赖", &none).unwrap();
        let b = pick_worker_from(team.clone(), "task-xyz", "清理废弃依赖", &none).unwrap();
        assert_eq!(a.id, b.id, "同任务稳定到同一员工");
        assert_eq!(a.specialty, "通用", "无专长任务优先派通用员工,不占测试/前端员工");
        // 全员都有专长(没通用)→ 退回全体散开,不至于派不出去。
        let allspec = vec![worker("s1", "测试"), worker("s2", "前端")];
        assert!(pick_worker_from(allspec, "task-xyz", "清理废弃依赖", &none).is_some());
        // 空团队 → None。
        assert!(pick_worker_from(vec![], "t", "x", &none).is_none());
    }

    #[test]
    fn pick_worker_prefers_higher_track_record() {
        use std::collections::HashMap;
        // 两个通用员工:gA 战绩好(9/10=90%,高档)、gB 战绩差(3/10=30%,低档)。
        let team = || vec![worker("gA", "通用"), worker("gB", "通用")];
        let mut perf = HashMap::new();
        perf.insert("gA".to_string(), (10u32, 9u32));
        perf.insert("gB".to_string(), (10u32, 3u32));
        // 无专长任务 → 通用池里挑战绩最高档的 gA;不同任务也优先靠谱的。
        assert_eq!(pick_worker_from(team(), "task-1", "清理依赖", &perf).unwrap().id, "gA");
        assert_eq!(pick_worker_from(team(), "task-99", "整理文档", &perf).unwrap().id, "gA");
        // 战绩持平(都没干过=中档)→ 退回 task_id 稳定散开(同任务同人)。
        let none = HashMap::new();
        let r = pick_worker_from(team(), "task-1", "清理依赖", &none).unwrap();
        assert_eq!(pick_worker_from(team(), "task-1", "清理依赖", &none).unwrap().id, r.id);
    }
}
