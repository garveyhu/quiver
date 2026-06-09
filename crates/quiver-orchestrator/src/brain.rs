//! 真 LLM 经理大脑(§21):让千问看局面、按 [`Decision`] schema 输出一个决策。
//! behind `qwen` feature(经 quiver-llm 调 HTTP)。key 运行时读、绝不打印/提交。

use crate::{Decision, ManagerBrain, ManagerContext};

/// 千问经理大脑。`decide` 把局面喂给千问,要它输出一个 §21 决策 JSON,解析成 [`Decision`];
/// 解析失败 → [`Decision::Noop`](安全默认,拿不准就不动)。
pub struct QwenBrain {
    creds: quiver_llm::QwenCreds,
    model: String,
}

impl QwenBrain {
    /// `profile` = `"personal"`/`"company"`;`model` 如 `"qwen-plus"`。
    pub fn new(profile: &str, model: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self {
            creds: quiver_llm::load_qwen_creds(profile)?,
            model: model.into(),
        })
    }
}

impl ManagerBrain for QwenBrain {
    fn decide(&self, ctx: &ManagerContext) -> anyhow::Result<Decision> {
        let sys = "你是一个自治 AI 研发公司的经理。看当前局面,决定这一拍做什么。只输出一个 JSON 对象,不要任何解释。";
        let brief = if ctx.brief.is_empty() { "(无)" } else { ctx.brief.as_str() };
        let user = format!(
            "局面:在途 {}/{},排队 {} 个任务,预算剩余 ${:.2}。\n项目简报:\n{brief}\n\n\
             输出一个 JSON,action 取其一:\n\
             - {{\"action\":\"spawn\",\"prompt\":\"给新 worker 的任务描述\"}} —— 派新活(仅当在途未满且有预算)\n\
             - {{\"action\":\"continue\",\"node_id\":\"...\",\"prompt\":\"追加指令\"}} —— 让在途 worker 继续\n\
             - {{\"action\":\"deliver\",\"node_id\":\"...\"}} —— 交付某节点成果\n\
             - {{\"action\":\"block\",\"node_id\":\"...\",\"reason\":\"...\"}} —— 拦下某节点\n\
             - {{\"action\":\"escalate\",\"reason\":\"...\"}} —— 超出能力,升级给人\n\
             - {{\"action\":\"refresh_memory\"}} —— 先刷新记忆再决策\n\
             - {{\"action\":\"noop\"}} —— 这拍什么都不做",
            ctx.inflight, ctx.max_inflight, ctx.queued, ctx.budget_remaining_usd,
        );
        let reply = quiver_llm::qwen_chat(&self.creds, &self.model, sys, &user)?;
        let obj = quiver_llm::extract_json_object(&reply)
            .ok_or_else(|| anyhow::anyhow!("经理回复里没有 JSON 对象"))?;
        // 解析成 Decision;不认识的 action / 坏 JSON → Noop(安全默认,绝不乱动)。
        Ok(serde_json::from_str::<Decision>(obj).unwrap_or(Decision::Noop))
    }
}
