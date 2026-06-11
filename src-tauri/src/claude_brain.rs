//! claude 引擎经理大脑(DESIGN §0/§4/§21):让订阅版 `claude` 看局面、按 §21 `Decision`
//! schema 输出一个决策 JSON,解析成 [`Decision`]。这是蓝图「思考全程用 claude」的经理侧
//! 落地 —— **取代被删的千问 `QwenBrain`**。
//!
//! 放在 app 层(不在 `quiver-orchestrator`)因为它要用 `quiver-core` 的 [`ClaudeRunner`],而
//! orchestrator 刻意保持纯逻辑、不依赖 runner。`manager_brain()`(lib.rs)选用它的**唯一**
//! 条件:人事部「经理」角色的 `brain == "claude"`(用户显式打开,§14)——它走 headless
//! 额度(§1.3);默认/其余一律免费 [`RuleBrain`](quiver_orchestrator::RuleBrain)。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use quiver_core::event::AgentEventPayload;
use quiver_core::runner::claude::ClaudeRunner;
use quiver_core::runner::AgentRunner;
use quiver_orchestrator::{Decision, ManagerBrain, ManagerContext};

/// claude 引擎经理大脑。每拍起一个轻量 `claude -p`(在 `cwd` 只读思考、只输出决策 JSON,
/// 不改代码),把输出解析成一个 [`Decision`]。
pub struct ClaudeBrain {
    bin: PathBuf,
    cwd: PathBuf,
    model: String,
    /// CEO 在人事部给经理写的工作指令 / 性格(§14 丰富配置):"预算紧优先复核"、"激进
    /// 并行派活"等。空则不注入。注入到决策 prompt 开头,塑造经理的判断风格。
    system_prompt: String,
    /// 思考流回调(可见性):claude 每吐一段思考文本就调一次 —— app 层注入它把思考 emit
    /// 到前端,让经理的决策过程**实时可见**(不再是黑箱)。None = 不外发(单测/纯逻辑路径)。
    on_think: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl ClaudeBrain {
    pub fn new(bin: PathBuf, cwd: PathBuf, model: impl Into<String>) -> Self {
        Self {
            bin,
            cwd,
            model: model.into(),
            system_prompt: String::new(),
            on_think: None,
        }
    }

    /// 注入 CEO 给经理的工作指令 / 性格(空则无效果)。
    pub fn with_system_prompt(mut self, sp: impl Into<String>) -> Self {
        self.system_prompt = sp.into();
        self
    }

    /// 注入思考流回调:claude 每产出一段思考文本就调一次,让经理思考实时可见(app 层注入 emit)。
    pub fn with_thinking_sink(mut self, sink: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        self.on_think = Some(sink);
        self
    }
}

#[async_trait]
impl ManagerBrain for ClaudeBrain {
    async fn decide(&self, ctx: &ManagerContext) -> anyhow::Result<Decision> {
        let prompt = build_prompt(ctx, &self.system_prompt);
        let runner = ClaudeRunner::new("manager").with_extra_args(["--model", self.model.as_str()]);
        let mut agent = runner.spawn(&prompt, &self.cwd, &self.bin).await?;
        // 收集 claude 的输出文本到流结束(EOF = sender drop);决策 JSON 在其中。
        let mut out = String::new();
        while let Some(ev) = agent.events.recv().await {
            if let AgentEventPayload::OutputChunk { text } = ev.payload {
                // 思考流实时外发(可见性):每段思考即时 emit,让前端看到经理在想什么。
                if let Some(sink) = &self.on_think {
                    sink(&text);
                }
                out.push_str(&text);
            }
        }
        Ok(parse_decision(&out))
    }
}

/// 把 claude 的输出解析成 [`Decision`]:先整体试,再抽第一个 JSON 对象试;都失败 → 升级
/// (§21「不认识/坏输出绝不崩,降级为升级」)。
fn parse_decision(text: &str) -> Decision {
    let trimmed = text.trim();
    if let Ok(d) = serde_json::from_str::<Decision>(trimmed) {
        return d;
    }
    if let Some(obj) = extract_json_object(trimmed) {
        if let Ok(d) = serde_json::from_str::<Decision>(obj) {
            return d;
        }
    }
    Decision::Escalate {
        reason: "经理(claude)输出无法解析为决策".to_string(),
    }
}

/// 从一段文本里抽出第一个平衡的 `{...}`(应对 claude 在 JSON 外带解释 / ```json 围栏)。
/// 简单括号配平,不处理字符串内部的花括号 —— 对干净的决策 JSON 够用。
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let mut depth = 0usize;
    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
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

/// 组装喂给 claude 的决策 prompt(§21):局面 + 待复核 + 队首任务全文(§5 A2) + 决策 schema,
/// 要求只输出一个 JSON 对象、不改文件。
fn build_prompt(ctx: &ManagerContext, system_prompt: &str) -> String {
    // 简报每条事实带可信度档(brief.to_text 标的)。点明怎么用 → 经理按可信度分级采信、别被低可信
    // 的「员工汇报/不可信」带偏(§6 强大记忆决策:可信度真正影响决策权重,不只是个标签)。
    let brief = if ctx.brief.is_empty() {
        "(无)".to_string()
    } else {
        format!(
            "{}\n(每条事实标了可信度:「权威 / 已验证·机械」是硬事实、可放心依赖;「已验证·印证」\
             较可靠;「员工汇报」是 AI 自述、参考即可;「不可信」需存疑核实。按可信度分级采信、\
             冲突时信高档的。)",
            ctx.brief.trim()
        )
    };
    // §14 CEO 给经理的工作指令/性格:塑造经理的判断风格,放在最前(最高优先)。
    let style = if system_prompt.trim().is_empty() {
        String::new()
    } else {
        format!("CEO 给你的工作准则(优先遵守):\n{}\n\n", system_prompt.trim())
    };
    // 待复核(先裁后派):有完工等裁的,列给经理看。
    let reviews = if ctx.pending_reviews.is_empty() {
        String::new()
    } else {
        let lines: String = ctx
            .pending_reviews
            .iter()
            .map(|r| {
                let round = if r.round > 0 {
                    format!(",已续跑第 {} 轮", r.round)
                } else {
                    String::new()
                };
                // §5 双向协作:worker 主动请示了 → 醒目列出问题,引导经理用 continue 回答它。
                let ask = match &r.question {
                    Some(q) => format!(" —— ⚠ worker 卡住请示:「{q}」(用 continue,prompt 写你的解答)"),
                    None => String::new(),
                };
                format!("- 节点 {} 完工,终态 {}(任务 {}{}){}\n", r.node_id, r.status, r.task_id, round, ask)
            })
            .collect();
        format!("\n完工待你复核裁决(deliver 交付 / continue 给指导再跑一轮改进 / block 拦下):\n{lines}")
    };
    // §14 团队专长:列出员工各擅长什么,经理派活时在任务描述里点明所需专长 → 派给对的人。
    let team = if ctx.team.is_empty() {
        String::new()
    } else {
        format!(
            "\n团队成员(专长 + 战绩 + 当前负载):派活、拆活时把活交给**对口、靠谱、此刻有空**的人 —— \
             难活/关键活优先给战绩好的;新人(暂无战绩)给机会练手;**别把新活堆给已经在忙的人,优先派给\
             空闲的**,让团队并行起来而不是一个人扛。\n{}\n",
            ctx.team.iter().map(|m| format!("- {m}\n")).collect::<String>()
        )
    };
    // 队首任务(A2 拆活):给原文,经理可原样派、也可结合简报改写/细化 spawn.prompt。
    let next = match &ctx.next_task {
        Some(t) => format!(
            "\n队列下一个任务(原文):「{t}」\n派活时 spawn.prompt 就是给 worker 的最终任务描述:\
             可照抄原文,也可以结合项目简报改写得更具体可执行(补背景/边界/验收标准);\
             **若该任务需要某专长(团队里有的),在描述里点明那个专长词,系统会自动派给对口的员工**。"
        ),
        None => String::new(),
    };
    // §5 主动自治:有 CEO 的高层目标 + 队列空时,经理不该 noop 退出,而是基于「目标 + 记忆
    // (已做了什么)」主动 plan 出推进目标的下一批任务 —— 这是「绝对自治」(主动驱动)的核心。
    let goal = if ctx.autonomous_goal.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\n\n【自治目标】CEO 要你持续推进:「{}」。**如果队列空了(排队 0)**,别 noop 收工 —— \
             看项目简报里已经做过什么,主动 plan 出推进这个目标的下一批具体子任务(2-4 个,别重复\
             已做的、按记忆里的经验来)。目标看着推进得差不多了、或拿不准下一步、或预算紧 → 才\
             escalate 给 CEO 或 noop。队列非空时照常先处理在手的活。",
            ctx.autonomous_goal.trim()
        )
    };
    format!(
        "{style}你是一个自治 AI 研发公司的经理。看当前局面,决定这一拍做什么。**先用一两句话讲你的\
         判断思路**(看了什么局面/简报里哪条、为什么这么定、为什么派给谁),让 CEO 看得到你在想什么;\
         然后在**最后一行**输出一个决策 JSON 对象。不要修改任何文件。\n\
         局面:在途 {}/{},排队 {} 个任务,预算剩余 ${:.2}。\n项目简报:\n{brief}{team}{reviews}{next}{goal}\n\n\
         决策 JSON,action 取其一:\n\
         - {{\"action\":\"spawn\",\"prompt\":\"给新 worker 的任务描述\"}} —— 派新活(仅当在途未满、\
           有预算、**且排队>0**;排队为 0 时没有新活可派,别 spawn,用 noop 或先处理待复核)\n\
         - {{\"action\":\"plan\",\"subtasks\":[\"子任务1\",\"子任务2\"]}} —— **拆活分工**:当队首任务\
           包含多件可独立完成的事时,把它拆成若干子任务,各自入队、分给多个 worker 并行协作;\
           每条 subtask 写成给 worker 的完整任务描述(同样可点明所需专长词)\n\
         - {{\"action\":\"continue\",\"node_id\":\"...\",\"prompt\":\"给 worker 的具体改进指导\"}} —— \
           **双向协作**:评审完一个完工节点、觉得产出方向对但还能更好(没到该 deliver、也不至于 block)时,\
           给它具体指导,让它 --resume 带着上一轮记忆 + 你的指导再跑一轮改进。反复救不动再 block\n\
         - {{\"action\":\"deliver\",\"node_id\":\"...\"}} —— 交付某节点成果\n\
         - {{\"action\":\"block\",\"node_id\":\"...\",\"reason\":\"...\"}} —— 拦下某节点\n\
         - {{\"action\":\"escalate\",\"reason\":\"...\"}} —— 超出能力,升级给人\n\
         - {{\"action\":\"refresh_memory\"}} —— 先刷新记忆再决策\n\
         - {{\"action\":\"noop\"}} —— 这拍什么都不做",
        ctx.inflight, ctx.max_inflight, ctx.queued, ctx.budget_remaining_usd,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ClaudeBrain::decide 的真 IO(起 claude -p)走 headless 额度,由用户手动验证;这里测它的
    // 核心纯逻辑 —— 从 claude(可能脏的)输出里解析出决策,含 §21 的坏输出降级。

    #[test]
    fn parse_clean_json() {
        assert_eq!(parse_decision(r#"{"action":"noop"}"#), Decision::Noop);
    }

    #[test]
    fn parse_json_with_surrounding_text() {
        // claude 常在 JSON 前后带话 —— 仍要抽得出。
        let d = parse_decision("好的,我的决策:\n{\"action\":\"spawn\",\"prompt\":\"读 README\"}\n以上。");
        assert!(matches!(d, Decision::Spawn { .. }));
    }

    #[test]
    fn parse_json_in_code_fence() {
        let d = parse_decision("```json\n{\"action\":\"escalate\",\"reason\":\"拿不准\"}\n```");
        assert!(matches!(d, Decision::Escalate { .. }));
    }

    #[test]
    fn parse_garbage_falls_back_to_escalate() {
        // §21:坏输出 / 空输出绝不崩,降级为升级给人。
        assert!(matches!(parse_decision("我不知道该干嘛"), Decision::Escalate { .. }));
        assert!(matches!(parse_decision(""), Decision::Escalate { .. }));
    }

    #[test]
    fn extract_balances_nested_braces() {
        assert_eq!(extract_json_object("x {\"a\":{\"b\":1}} y"), Some("{\"a\":{\"b\":1}}"));
        assert_eq!(extract_json_object("no json here"), None);
    }

    #[test]
    fn build_prompt_embeds_next_task_and_reviews() {
        use quiver_orchestrator::PendingReview;
        // A2 拆活:队首任务原文 + 改写指示要进决策 prompt;待复核也要列出。
        let ctx = ManagerContext {
            next_task: Some("做转写的导出功能".into()),
            pending_reviews: vec![PendingReview {
                node_id: "node-7".into(),
                task_id: "task-1".into(),
                status: "verified".into(), round: 0, question: None,
            }],
            team: vec!["员工 2 · 测试".into(), "员工 3 · 前端".into()],
            ..ManagerContext::default()
        };
        let p = build_prompt(&ctx, "预算紧时优先复核,别贸然派活");
        assert!(p.contains("「做转写的导出功能」"), "队首任务原文");
        assert!(p.contains("改写"), "改写指示");
        assert!(p.contains("node-7") && p.contains("verified"), "待复核行");
        assert!(p.contains("员工 2 · 测试") && p.contains("前端"), "团队专长清单进 prompt");
        assert!(p.contains("预算紧时优先复核") && p.contains("工作准则"), "CEO 给经理的指令进 prompt");
        // 都没有时不渲染对应段落。
        let empty = build_prompt(&ManagerContext::default(), "");
        assert!(!empty.contains("队列下一个任务") && !empty.contains("完工待你复核"));
        assert!(!empty.contains("工作准则"), "空 system_prompt 不渲染准则段");
    }
}
