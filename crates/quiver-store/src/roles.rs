//! 人事部:角色配置持久层(DESIGN §14 / 蓝图 autonomous-org §20 `agent_role` 裁剪版)。
//!
//! 每个"人物"(经理/员工)一行配置:用什么大脑/模型、附加什么人设提示、单任务预算与轮数
//! 上限。**改一次 `version`+1**(§14 版本化,审计"当时是什么配置"用)。首次打开 seed 内置的
//! 经理+员工两行,所以读永远不空。
//!
//! 安全要点(轮1 事故的制度化):经理用不用烧钱的 claude 大脑,**只由这里的 `brain` 字段
//! 决定**(用户在人事部 UI 显式改),不搭任何其他设置的便车。

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::Store;

/// 一个角色(人物)的完整配置行。camelCase 序列化,直接喂前端人事部 UI。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRole {
    /// 内置:`manager` / `worker`(将来自定义角色用新 id)。
    pub id: String,
    /// 显示名(「经理」「员工」)。
    pub name: String,
    /// `manager` | `worker` —— 决定它出现在人事部哪一栏、哪些字段生效。
    pub kind: String,
    /// 经理大脑:`rule`(免费规则) | `claude`(真 claude 想,走 headless 额度)。
    /// 仅 kind=manager 生效。**烧钱大脑只能在这里显式打开。**
    pub brain: String,
    /// claude 模型(经理思考 / 员工干活用),如 `sonnet`。
    pub model: String,
    /// 附加人设提示(拼在任务/决策 prompt 前)。空=无。
    pub system_prompt: String,
    /// 单任务美元预算上限(`--max-budget-usd`)。NULL=不限。
    pub budget_usd: Option<f64>,
    /// 单任务轮数上限(`--max-turns`)。NULL=不限。
    pub max_turns: Option<i64>,
    /// 配置版本:每次 update +1(§14)。
    pub version: i64,
    pub updated_at: i64,
}

/// 人事部的增量改动:都可选,只写给到的字段。`budget_usd`/`max_turns` 用双层 Option
/// 区分"不动"(缺键)与"显式清除"(null)。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RolePatch {
    pub name: Option<String>,
    pub brain: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub budget_usd: Option<Option<f64>>,
    #[serde(default, deserialize_with = "de_opt_i64")]
    pub max_turns: Option<Option<i64>>,
}

fn de_opt_f64<'de, D>(d: D) -> Result<Option<Option<f64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<f64>::deserialize(d)?))
}

fn de_opt_i64<'de, D>(d: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<i64>::deserialize(d)?))
}

const ROLE_COLUMNS: &str =
    "id, name, kind, brain, model, system_prompt, budget_usd, max_turns, version, updated_at";

fn row_to_role(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRole> {
    Ok(AgentRole {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        brain: row.get(3)?,
        model: row.get(4)?,
        system_prompt: row.get(5)?,
        budget_usd: row.get(6)?,
        max_turns: row.get(7)?,
        version: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

impl Store {
    /// 全部角色(经理在前,再按 id 稳定排序)。
    pub fn list_roles(&self) -> anyhow::Result<Vec<AgentRole>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(&format!(
            "SELECT {ROLE_COLUMNS} FROM agent_role
             ORDER BY CASE kind WHEN 'manager' THEN 0 ELSE 1 END, id"
        ))?;
        let rows = stmt.query_map([], row_to_role)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 单个角色;不存在返回 `None`(内置 id 被 seed 保证存在)。
    pub fn get_role(&self, id: &str) -> anyhow::Result<Option<AgentRole>> {
        let conn = self.conn.lock().expect("store lock");
        conn.query_row(
            &format!("SELECT {ROLE_COLUMNS} FROM agent_role WHERE id = ?1"),
            params![id],
            row_to_role,
        )
        .optional()
        .map_err(Into::into)
    }

    /// 增量更新一个角色,`version`+1(§14)。只写 patch 里给到的字段;角色不存在报错。
    /// 读-改-写在一把锁下,并发 patch 不丢字段。
    pub fn update_role(
        &self,
        id: &str,
        patch: &RolePatch,
        updated_at: i64,
    ) -> anyhow::Result<AgentRole> {
        let conn = self.conn.lock().expect("store lock");
        let mut role = conn
            .query_row(
                &format!("SELECT {ROLE_COLUMNS} FROM agent_role WHERE id = ?1"),
                params![id],
                row_to_role,
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("角色不存在:{id}"))?;

        if let Some(v) = &patch.name {
            role.name = v.clone();
        }
        if let Some(v) = &patch.brain {
            role.brain = v.clone();
        }
        if let Some(v) = &patch.model {
            role.model = v.clone();
        }
        if let Some(v) = &patch.system_prompt {
            role.system_prompt = v.clone();
        }
        if let Some(v) = patch.budget_usd {
            role.budget_usd = v;
        }
        if let Some(v) = patch.max_turns {
            role.max_turns = v;
        }
        role.version += 1;
        role.updated_at = updated_at;

        conn.execute(
            "UPDATE agent_role SET name=?2, brain=?3, model=?4, system_prompt=?5,
                    budget_usd=?6, max_turns=?7, version=?8, updated_at=?9
             WHERE id=?1",
            params![
                role.id,
                role.name,
                role.brain,
                role.model,
                role.system_prompt,
                role.budget_usd,
                role.max_turns,
                role.version,
                role.updated_at,
            ],
        )?;
        Ok(role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_store_seeds_builtin_roles() {
        let store = Store::open_in_memory().unwrap();
        let roles = store.list_roles().unwrap();
        let ids: Vec<&str> = roles.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["manager", "librarian", "worker"], "seed 经理在前,共三内置角色");
        // 安全默认:会烧钱的大脑(经理决策/图书管理员提炼)必须 seed 成免费 rule(休眠),
        // claude 只能在人事部显式打开。
        for id in ["manager", "librarian"] {
            let r = store.get_role(id).unwrap().unwrap();
            assert_eq!(r.brain, "rule", "{id} 必须 seed 为 rule");
            assert_eq!(r.version, 1);
        }
    }

    #[test]
    fn update_role_bumps_version_and_keeps_untouched_fields() {
        let store = Store::open_in_memory().unwrap();
        let patch = RolePatch {
            brain: Some("claude".into()),
            budget_usd: Some(Some(2.5)),
            ..RolePatch::default()
        };
        let updated = store.update_role("manager", &patch, 42).unwrap();
        assert_eq!(updated.brain, "claude");
        assert_eq!(updated.budget_usd, Some(2.5));
        assert_eq!(updated.version, 2, "改一次 version+1");
        assert_eq!(updated.updated_at, 42);
        // 没动的字段保持 seed 值。
        assert_eq!(updated.model, "sonnet");
        // 再清掉预算(显式 null)。
        let clear = RolePatch { budget_usd: Some(None), ..RolePatch::default() };
        let cleared = store.update_role("manager", &clear, 43).unwrap();
        assert_eq!(cleared.budget_usd, None);
        assert_eq!(cleared.version, 3);
    }

    #[test]
    fn update_unknown_role_errors() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.update_role("ghost", &RolePatch::default(), 1).is_err());
    }
}
