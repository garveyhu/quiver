# Quiver 后端收尾 · loop 任务(按蓝图 §23 做到 P5)

把 Quiver 后端**按蓝图 docs/autonomous-org.md 做完**(P2 剩余 → P3 → P4 → P5)。
前端重写在这之后(见 loop.md)。你在一个反复唤醒的 /loop 里:每次唤醒 = 推进一刀、
验证、提交、记 BUILD-LOG、干净收尾。无人值守。

## 0. 每轮开头(状态在文件,不在记忆)
1. 读 docs/BUILD-LOG.md(跨轮真相:已完成/进行中/下一步/卡住)。
2. 读相关代码 + 蓝图对应段落确认现状,别凭记忆。
3. 从下面"路线图"挑下一个【最小、可验证、可提交】的切片开工。

## 1. 已完成(别重做)
- P0 地基 ✅(崩溃恢复/session_id 透出落库+--resume/reconcile/killpg/runner 薄层)。
- P1 记忆 ✅(quiver-memory:episode 绑 git、双时间事实、FTS5、recall §6.7、简报)。
- P2 机制 ✅(作废 supersede / 待审 staging+promote / 印证升档 corroborate / 状态唯一 + current_state)。

## 2. 路线图(顺序来,别跳;每刀 cargo 全绿才提交)
**P2 剩余**
- 向量召回:引 sqlite-vec(vec0 虚拟表 memory_fact_vec FLOAT[1536])+ 千问 v2 embedding;
  混合检索(FTS + 向量 + §6.7 recency/importance/trust 加权融合)。
- 图书管理员:作废/状态派生的 AI 判断(挑候选→一次 AI 调用判矛盾→supersede;§6.4)。
  AI 判断走可注入接口(测试用 stub,真跑用 LLM)。
**P3 安全**(沙箱在"多员工放开"前必须到位)
- 沙箱(克隆前就位,§7–8;macOS profile 见 §24,先脚手架+可配)、独立审计(变异式,
  干净克隆重跑+查测试被改弱)、权能分权(Rule of Two)、熔断、真急停(killpg ✅)。
**P4 监督+成本+递归**
- 重启/同级/升级阶梯(决策税 + 只减预算)、配额感知/缓存优先/价值闸、子经理(递归)。
**P5 乙部 + 自管**(§10–16)
- 观测/指标(OTel GenAI)、测试、验收台、回滚/还原点、人事、升级、开张;
  MCP 自管记忆(rmcp:memory.search/brief/write_fact…)、§5 编排器 MCP(org.inflight/
  episode.record/auditor.run)。
**§5 AI 经理编排**(贯穿)——经理决策(spawn/continue/deliver/block/escalate/refresh_memory,
  §21 --json-schema)、saga_step 去重钥匙=决策序号、栅栏令牌对账、有界在途状态(§5.6)。

## 3. 真 LLM / 千问 / 密钥(用户已授权花钱)
- **允许接真 LLM(claude 经理决策)+ 真千问 v2 embedding 并实跑验证**(会产生 API 费用,
  用户已同意)。§10 预算闸要接上兜底。
- **密钥只在运行时从 ~/.agents/resources.json 读**(千问 key 等);绝不硬编码、绝不写进
  仓库或提交。读取/解析做成模块,key 值绝不进日志/提交。
- 所有 LLM/embedding 调用走**可注入 trait**(spawn 出来的 runner 同款思路):测试用 fake/
  stub(免费确定性),真跑用实现。这样 cargo test 全绿不依赖网络。

## 4. §24 待你拍板(用合理默认先推,标进 BUILD-LOG 等用户改)
- 合并策略:P0 维持 keep_branch 手动合(run.rs 安全缝不动);自动合等用户定。
- embedding:千问 v2(1536 维)。沙箱 profile:先做可配脚手架,具体 profile 标给用户。
- "内容印证"提示词 / 评测集:先占位 + 标"需真实长会话调"。

## 5. 结构(允许重构,保持分层)
- crates/quiver-core(纯 supervisor)· quiver-store(操作库)· quiver-memory(记忆库)·
  src-tauri(app/IPC/调度)· 新增 crates/quiver-agent(可选,抽 runtime trait,注意 event 类型环)。
- src-tauri/src/lib.rs 的 IPC 命令多了就拆进 src-tauri/src/commands/ 按域分,别堆一个文件。
- 编排/saga/经理 这类纯逻辑放 quiver-core 或新 crate(quiver-orchestrator?),不碰 tauri 类型。

## 6. 每轮纪律 + 红线
- 一刀做完【必须验证】:`cargo check --workspace`,涉及逻辑 `cargo test --workspace`
  (改了 fake-claude 先 `cargo build -p fake-claude`)。过不了不提交、标 WIP 记 BUILD-LOG。
- 验证过就提交(分支 feat/game-first;Angular 规范;footer `Co-Authored-By: Claude Opus 4.8
  (1M context) <noreply@anthropic.com>`)。粒度小、信息清楚。
- 红线:绝不 push/开 PR/合 main(用户自己合);绝不硬编码密钥;不删非你建的文件;
  不 git reset --hard/force push;同一错误连续两次过不去就停手、记 BUILD-LOG"需你定"、
  转下一个独立任务,绝不空转。
- 每轮结尾更新 docs/BUILD-LOG.md:✅这轮(带 commit hash)、▶下一步、⚠卡住/待拍板。

现在开始:读 BUILD-LOG + 蓝图对应段落,从路线图挑下一刀,小步验证小步提交。
