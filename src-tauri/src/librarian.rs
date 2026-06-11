//! 记忆官·提炼器(DESIGN §6.7):从近期 episode 提炼值得长期记住的项目事实,写进
//! 记忆库的事实层 —— 没有它,brief 的「当前事实」永远是 0,经理只有流水没有沉淀知识。
//!
//! 思考用 **claude**(§0「思考全程用 claude」;千问版已删)。烧钱规矩同经理大脑:
//! **只由人事部 `librarian` 角色的 `brain=='claude'` 显式打开**(seed 是休眠),绝不
//! 搭其他设置便车。触发:经理交付后(里程碑,§6.7),限频(距上次提炼 < 间隔则跳过)。
//! 产物按蓝图 §6.2 一律记**「员工汇报」档**(AI 的自然语言提炼,不可自授高档)。

use std::sync::Arc;

use quiver_core::event::AgentEventPayload;
use quiver_core::runner::claude::ClaudeRunner;
use quiver_core::runner::AgentRunner;
use quiver_memory::{MemoryStore, NewFact};
use quiver_store::Store;
use tauri::{AppHandle, Manager};

use crate::run::RunMode;

/// 两次提炼的最小间隔(限频,§6.7「不上关键路径、限频」)。
const MIN_INTERVAL_MS: i64 = 30 * 60 * 1000;
/// 一次提炼读的近期 episode 条数上限。
const EPISODE_BATCH: usize = 12;
/// 一次最多写入的事实条数(防 AI 灌库)。
const MAX_FACTS_PER_DISTILL: usize = 3;
/// 提炼产物的 kind 标签(也用于限频查询:最近一条该 kind 事实的 recorded_at)。
const DISTILL_KIND: &str = "提炼";

/// 经理交付后调用:若记忆官被显式打开且不在限频窗口内,起一个 claude 从近期
/// episode 提炼 1-3 条事实写入记忆。完全 best-effort——任何失败只是跳过,绝不影响编排。
/// 在独立 tokio task 里跑(调用方不等)。
pub fn distill_after_delivery(app: &AppHandle, store: &Arc<Store>, project: String) {
    let app = app.clone();
    let store = store.clone();
    tokio::spawn(async move {
        let _ = try_distill(&app, &store, &project).await;
    });
}

async fn try_distill(app: &AppHandle, store: &Arc<Store>, project: &str) -> anyhow::Result<()> {
    // 1. 显式开关(§14):librarian 角色 brain=='claude' 才干活;默认休眠。
    let Some(role) = store.get_role("librarian").ok().flatten() else {
        return Ok(());
    };
    if role.brain != "claude" {
        return Ok(());
    }
    let Some(state) = app.try_state::<crate::AppState>() else {
        return Ok(());
    };
    let Some(mem) = state.memory.get().cloned() else {
        return Ok(());
    };

    // 2. 限频:最近一条提炼事实距今 < 间隔 → 跳过。
    let now = crate::now_ms();
    if let Some(last) = last_distill_at(&mem, project) {
        if now - last < MIN_INTERVAL_MS {
            return Ok(());
        }
    }

    // 3. 取近期 episode 文本(工作记录 + 经理裁决留痕都在)。
    let episodes = mem.episodes_for_project(project, EPISODE_BATCH as i64)?;
    if episodes.len() < 3 {
        return Ok(()); // 记录太少,提不出有价值的事实
    }
    let log: String = episodes
        .iter()
        .map(|e| {
            let verdict = e.verify_result.as_deref().unwrap_or("-");
            let summary = e.summary.as_deref().unwrap_or("");
            format!("- [{verdict}] {summary}\n")
        })
        .collect();

    // 4. claude 提炼(角色配置的 model)。bin 跟随运行模式(与经理大脑一致):simulate → fake-claude
    // (免费预演整条提炼链路),real → 真 claude(真提炼)。守「simulate 全免费」红线 —— 记忆官
    // claude 不该在 simulate 下偷烧 real 额度。
    let settings = store.get_settings()?;
    let bin = crate::run::resolve_agent_bin(&settings, RunMode::from_label(&settings.default_mode))?;
    let prompt = format!(
        "你是项目记忆库的记忆官。下面是项目「{project}」最近的工作记录(含经理裁决)。从中提炼最多 \
         {MAX_FACTS_PER_DISTILL} 条**值得长期记住**的项目知识,不要复述单次流水。每条标一个种类:\
         「约定」(该遵守的规则)/「教训」(踩过的坑、下次避开)/「有效做法」(被验证管用的方法)。\
         只输出一个 JSON 数组,不要任何解释、不要修改文件:\n\
         [{{\"kind\":\"约定|教训|有效做法\",\"text\":\"事实陈述\",\"importance\":1到9}}]\n\
         没有值得记的就输出 []。\n\n工作记录:\n{log}"
    );
    let runner = ClaudeRunner::new("librarian").with_extra_args(["--model", role.model.as_str()]);
    let mut agent = runner.spawn(&prompt, std::path::Path::new(project), &bin).await?;
    let mut out = String::new();
    while let Some(ev) = agent.events.recv().await {
        if let AgentEventPayload::OutputChunk { text } = ev.payload {
            out.push_str(&text);
        }
    }

    // 5. 解析 + 写入(员工汇报档,§6.2;坏输出安全跳过)。
    let facts = parse_facts(&out);
    for f in facts.iter().take(MAX_FACTS_PER_DISTILL) {
        let _ = mem.insert_fact(&NewFact {
            project: project.to_string(),
            scope: None,
            kind: distill_kind(&f.kind),
            text: f.text.clone(),
            entities: None,
            entity: None,
            importance: Some(f.importance.clamp(1, 9)),
            valid_at: None,
            recorded_at: now,
            provenance: Some("librarian·claude".to_string()),
            trust: "员工汇报".to_string(),
            source_commit: None,
            source_episode_id: None,
        });
    }
    Ok(())
}

/// 最近一条提炼事实的 recorded_at(限频游标)。读失败当无(允许提炼,写入侧仍有保护)。
fn last_distill_at(mem: &MemoryStore, project: &str) -> Option<i64> {
    mem.current_facts(project)
        .ok()?
        .into_iter()
        .filter(|f| f.kind == DISTILL_KIND)
        .map(|f| f.recorded_at)
        .max()
}

/// 提炼出的一条事实(claude 输出的 JSON 数组元素)。
#[derive(Debug, serde::Deserialize)]
struct DistilledFact {
    /// 种类:约定/教训/有效做法。缺/不认识 → 退回笼统「提炼」(`distill_kind` 兜)。
    #[serde(default)]
    kind: String,
    text: String,
    #[serde(default = "default_importance")]
    importance: i64,
}

fn default_importance() -> i64 {
    5
}

/// 把 claude 提炼出的种类约束到记忆库认的几类(配合记忆库按种类分组);不认识的归笼统「提炼」。
fn distill_kind(raw: &str) -> String {
    match raw.trim() {
        "约定" | "教训" | "有效做法" | "状态" => raw.trim().to_string(),
        _ => DISTILL_KIND.to_string(),
    }
}

/// 从 claude 的(可能带围栏/解释的)输出里解析事实数组;解析不出 → 空(安全跳过)。
fn parse_facts(out: &str) -> Vec<DistilledFact> {
    let trimmed = out.trim();
    if let Ok(v) = serde_json::from_str::<Vec<DistilledFact>>(trimmed) {
        return v;
    }
    // 抽第一个平衡的 [...](应对围栏/前后缀文本)。
    if let Some(arr) = extract_json_array(trimmed) {
        if let Ok(v) = serde_json::from_str::<Vec<DistilledFact>>(arr) {
            return v;
        }
    }
    Vec::new()
}

/// 抽第一个平衡的 `[...]` 片段(简单配平,不处理字符串内方括号——对干净 JSON 数组够用)。
fn extract_json_array(s: &str) -> Option<&str> {
    let start = s.find('[')?;
    let mut depth = 0usize;
    for (i, c) in s[start..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + c.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // claude 真调用走订阅额度,由用户显式开 librarian 后验证;这里测纯逻辑:解析与安全默认。

    #[test]
    fn parse_clean_array() {
        let v = parse_facts(r#"[{"text":"verify 命令是质量闸门","importance":7}]"#);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].text, "verify 命令是质量闸门");
        assert_eq!(v[0].importance, 7);
    }

    #[test]
    fn parse_array_in_fence_and_default_importance() {
        let v = parse_facts("提炼结果:\n```json\n[{\"text\":\"任务多为导出功能\"}]\n```");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].importance, 5, "缺 importance 用默认");
    }

    #[test]
    fn parse_garbage_and_empty_are_safe() {
        assert!(parse_facts("我觉得没什么可记的").is_empty());
        assert!(parse_facts("[]").is_empty());
        assert!(parse_facts("").is_empty());
    }

    #[test]
    fn extract_balances_nested_brackets() {
        assert_eq!(extract_json_array("x [[1],[2]] y"), Some("[[1],[2]]"));
        assert_eq!(extract_json_array("no array"), None);
    }
}
