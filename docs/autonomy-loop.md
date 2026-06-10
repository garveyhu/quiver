# 自治引擎重塑 · /loop 共享记忆

> 这是 `/loop`(每 20 分钟)的共享记忆。每轮 fire:① 读本文件 ② 做「路线图」最上面未勾的一项(一轮一个可验证切片) ③ `cargo test` + tsc + 桥接器真应用验证 ④ 勾掉并把发现写进「迭代日志」。**保持 app 始终可用、全程 simulate 免费验证(绝不用 real 烧额度)。**

## 用户的核心期望(产品意图,每轮对照)

> "我需要**绝对自治**的聪明的协作工作能力,支持**丰富的配置**和**强大的记忆决策**能力。初衷是让应用**一直跑下去**——有一个经理一直帮我去回其他 AI 的决策。而且每个人物(经理/员工)都该有**各种配置**,现在什么都没看见。"

拆成四根支柱(蓝图 docs/autonomous-org.md 对应章节):
- **A. 绝对自治** — 经理常驻、worker 完成后经理复核裁决(deliver/block/escalate),不只是派活(§5)
- **B. 丰富配置** — 人事部:经理/员工角色配置(模型/提示/预算/并发/重启策略),可见可改(§14)
- **C. 记忆决策** — 经理带着项目记忆(brief/facts/episodes)决策,决策也留痕回记忆(§6)
- **D. 体感不是玩具** — 决策可见有血肉、simulate 节奏可信、交互丝滑

## 已完成(本 session,自治引擎接通那一刀 + 第 1 轮)

- [x] 删走歪的 QwenBrain(千问只配做 embedding,经理思考必须 claude)
- [x] 经理控制循环 + Effect 执行层(`src-tauri/src/manager.rs`):tick→decide→apply→真 spawn worker→回流唤醒;`settings.autonomous` 开关(默认关,经理工作台里可开)
- [x] ClaudeBrain(`src-tauri/src/claude_brain.rs`):real 模式经理用 claude 想;simulate 用免费 RuleBrain
- [x] 前端经理工作台(`shell/ManagerDesk.tsx`,按 M):局面卡+自治开关+决策流
- [x] 〔轮1〕摄像头:点小人**不再强行缩放**,只开详情(Office.tsx)
- [x] 〔轮1〕弹窗提速 .35s→.16s + **点框外即收起**(ws-scrim,global.css/Office.tsx)
- [x] 〔轮1〕决策流加血肉:每拍带 taskPrompt(派的具体活)+ 决策依据(在途/上限/排队/预算)(manager.rs ManagerDecisionEvent + ManagerDesk situation/detail)
- [x] 〔轮1〕**经理记忆接通**:ManagerContext.brief 从 quiver-memory 真简报来(facts+episodes,manager.rs memory_brief)——"记忆→决策"的接缝通了
- [x] 〔轮1〕治玩具感:fake-claude happy 脚本重写成可信工作弧(读码→搜索→说明→双改→跑测→汇报,13 行+相位思考停顿,默认 ~5s,settings fakeDelayMs 等比放慢)

## 路线图(从上往下做,一轮一项,做完测了再勾)

- [x] **A1. 经理复核裁决**(轮2) — 完工 worker 进待复核队(PendingReview:node/task/终态,先入队再释放名额防漏裁);RuleBrain **先裁后派**(verified/done→Deliver,其余→Block 带原因);执行层裁决出队+带 taskPrompt 进决策流;循环退出条件加"复核队列空"。真合并列车接 Deliver 后面留 §5.7(审计就位后)。
- [x] **B1. 人事部 schema + IPC**(轮3) — `agent_role` 表(brain/model/system_prompt/budget_usd/max_turns/version,§20 裁剪)+seed 经理/员工(brain=rule 安全默认);store roles.rs(list/get/update,version+1);`list_roles`/`update_role` IPC(debug+release 两块都注册);前端 wire/commands 服务层就位。**经理大脑显式开关接通**:manager_brain() 唯一依据=role('manager').brain——'claude' 才构造 ClaudeBrain(用 role.model),ClaudeBrain dead_code 豁免摘除。
- [x] **B2. 人事部 UI**(轮4) — 叠层「人事部」(⌘K):经理卡(大脑 rule/claude 切换+烧钱警示)+员工卡(模型/单任务预算/轮数);useRoles hook;**配置不是摆设**:① 经理循环改为**每拍现场选脑**(删 per-project brain 缓存,改 brain 下一拍即生效,轮3 注意点解决);② real worker 真用员工角色配置(run.rs:model 用 role 的、budget_usd→--max-budget-usd、max_turns→--max-turns);③ resume_all 顺手戳醒等待中的循环。
- [x] **C1. 决策留痕回记忆**(轮5) — 真落地的派活/交付/拦下/升级写成 episode(record_decision_episode,best-effort 不挡编排;Noop/Refresh 不写防刷屏)。真 app 实测:memory.sqlite 有「经理·派活「…」(理由)」「经理·交付「…」」带 node-0;时间线穿插显示组织决策与 worker 交付;brief 取同表 → 经理记得自己之前的裁决,记忆↔决策闭环。
- [x] **A2. ClaudeBrain 拆活**(轮6-9) — ManagerContext 加 next_task(队首任务原文,peek 与 claim 同序)+ QUEUE_NEXT_PLACEHOLDER 常量(RuleBrain 占位=原样认领);ClaudeBrain 决策 prompt 嵌队首原文+改写指示(补背景/边界/验收标准)+待复核清单;执行层:经理给非占位 prompt → worker 跑经理改写后的任务描述。单测 build_prompt 嵌入;94 测试全过。
- [x] **C2. 工作台显示简报**(轮10) — 工作台「记忆」行+「经理在想什么 ▾(N 条)」折叠段:get_brief 前端封装(wire Brief/BriefFact+commands.getBrief+useManagerDesk),展开显示事实(可信度/重要度)与近期经历(含经理裁决留痕)。真 app 实测 10 条全显。注意:App.tsx 改动是 HMR 边界,验证前要整页 reload。
- [x] **A3. 打回重做·组织回流**(轮11,插队于 D1 前——失败任务是死胡同是"一直跑下去"的真缺口,§12) — `requeue_task_cmd`:已终结任务作为**新任务**入队(新 task_id 绕开 agent_event (task_id,seq) 主键冲突,老行留档;进行中的拒绝);决策流「拦下」行带「打回重做 ↻」按钮。真 app 实测完整回流:verify_failed→经理拦下→点打回→新任务→经理再派→再拦(打回是人工动作,不自动循环)。verify_command 测试后已还原空。
- [x] **A4. 自治模式的启动恢复**(轮12-14) — 发现:setup 的崩溃恢复 reconcile **只走旧 scheduler**,自治模式重启一次组织就"失忆降级"成无脑流水线。修:ManagerLoop::reconcile(requeue+sweep 孤儿+经理接管)+setup 按 autonomous 分流。端到端实测:造 running 残留→pkill 干净→起 app→reconcile requeue→「经理·派活「崩溃恢复测试任务」」→verified。测试数据已清。
- [x] **D1. 完工驻留**(轮15) — workers.ts 加 LingeringTask 驻留位(在途优先占工位,驻留用剩余,绝不挤新活);useWorkers diff 出"刚完工"任务驻留 4s,气泡报「✓ 验收通过/✗ 没过验收」再回休息室。真 app 实测抓到驻留瞬间:`✓ 验收通过|经理|审计`。escalate 高亮决策流已有(st-bad)。桥接器经验:eval 输出是多行 pretty JSON,Monitor 里要 `tr -d '\n'` 拍平再 grep,`tail -1` 只会拿到 `]`。

## 轮26:并发上限实时生效 + 批量派活(协作)
- [x] **max_workers 实时生效**(轮3 缓存坑彻底修) — InFlight::set_max + Orchestrator::set_max_inflight,manager_loop **每拍**把当前 settings.max_workers 同步进 orchestrator(像轮4 修经理大脑那样)。改并发不必重建经理循环。64 单测绿(orchestrator+app)。
- [x] **批量派活命令**「派一批活」— 一次下 3 个目标(enqueueBatch),经理按并发上限并行调度。体现 §3「协作」。
- ⚠️→✅ 轮26 撞出的两个真 bug 已修(轮27)。

## 轮27:修两个真 bug(重启瘫痪 + 假成功)
- [x] **bug1 重启项目自愈** — current_project(lib.rs)内存没当前项目时,从持久化 last_project 恢复并回填内存。根因:进程重启后内存 project_path 清空,getInitialState 要前端调才设,这之间任何 IPC 都瘫痪。真 app 实测:落 last_project→**重启不手动选项目→派一批 3 件全部入队+跑通**(自愈生效)。
- [x] **bug2 不再假报成功** — enqueueBatch 统计真实成败,caption 报"X/N 成功"+错误原因,绝不静默吞异常假报"已派"(违背质量的假成功)。
- 教训记档:**前端 catch 块绝不能空吞** —— 要么反馈用户要么至少 console;"假成功"比"明确失败"更伤(用户以为派了活,其实什么没发生)。tsc+app 34 测试绿。

## 轮28:并行协作画面闭合(补验轮26欠的诚实债)
- [x] **3 worker 真并行实测拿到** — 干净环境(项目自愈)+max=3+自治,派一批 → 后端 **Monitor 抓到同时 running=3**;决策流坐实经理**连续派 3 件不同活**(给看板加暗色模式/修登录态丢失/做转写导出),即 §3「协作」:经理并行调动多员工干不同任务。max_workers 实时生效(轮26)+批量派活(轮26)+项目自愈(轮27)三者合起来验证通过。配置已还原(autonomous=0/max=1/fake_delay=30)。
- 验证技巧记档:抓"瞬时并行态"用 Monitor until-loop 查 DB running 计数(前端 .worker.working 是实时的,eval 查询时机晚了任务已跑完);后端 DB 计数 + 决策流是更可靠的并行证据。

## 路线图·下一阶段(骨架后的深化,按价值排)

- [x] **C3. 图书管理员·claude 提炼器**(轮16) — 勘察修正:quiver-memory 的 librarian.rs 是**矛盾裁决器**(reconcile),"从 episode 提炼事实"的提炼器根本不存在 → 事实层永远 0 的根因。落地:① 删 QwenJudge+smoke example(千问思考违蓝图;parse_verdict 转 pub 留给将来 ClaudeJudge);② 新建 src-tauri/src/librarian.rs:claude 提炼器(读近 12 条 episode→提炼≤3 条事实→insert_fact 员工汇报档/kind=提炼/provenance=librarian·claude),30min 限频+<3 条 episode 不提+坏输出安全跳过,4 个 parse 单测;③ seed librarian 角色(brain=rule=休眠),人事部第三卡「图书管理员·档案室」(休眠·不提炼/claude·提炼事实+烧钱警示);④ 触发:经理 Deliver 后 best-effort 后台跑。真 app 实测:三卡渲染✓、seed 补进既有 DB✓、默认休眠交付后不起 claude✓。真提炼留用户显式开(烧钱)。测试 104 过 0 失败。
- [x] **A5. 带会话返工**(轮17) — 勘察发现 resume 链路其实已全通(worker_started 落 session_id、同任务再跑自动 --resume,reconcile 在用);真缺口是**打回重做开新 task_id 丢会话**。修:requeue_task_cmd 把老任务 session_id 复制给新任务 + prompt 加「(返工)上次成果未通过验收…」指引 → worker 在原会话续跑(记得自己改过什么),不是裸重跑。端到端实测:新任务 prompt 带返工前缀 + session_id 传承(fake-session-0001)。Decision::Continue 对**在途** worker 的注入受进程模型限制(claude -p 跑完即退,stdin 未接),engine 对已完工节点也会校验拒——保持现状,文档化。
- [x] **P0 硬化之 decision_log 落库**(轮18) — store/decisions.rs(§20 裁剪:每拍真实决策一行只追加,record+list 最新在前)+schema 建表带索引;emit_decision 同步落库(best-effort);get_decisions IPC;useManagerDesk 打开时回填历史(与 live 按 tsMs+seq 去重合并)。端到端实测:派活落 5 拍(spawn/noop/deliver)→**重启**→工作台打开即回填完整决策弧——决策历史跨重启存活。测试 70 过。
- [x] **P0 硬化·序号单调 + 急停核查 + 升级可感**(轮19) — ① 决策序号跨重启续编:SagaLedger::starting_at + Orchestrator::with_seq_start + store.max_decision_seq,经理循环建时从日志 max+1 接着编;实测重启后从 #5 续(不回卷)。② §8.4 急停核查:**已是 killpg**(lib.rs kill_pid 负 pid 杀整组 + ClaudeRunner process_group(0) 自成组),蓝图点名洞早已修,记档。③ escalate 可感:经理升级时状态条立刻喊人(「⚠ 经理升级等你拍板:…按 M 处理」),useManagerDesk onEscalate→App caption。测试 99 过。
- [ ] **P0 硬化·余项(降级为低优)** — saga 序号与日志同事务的原子分配:当前无"决策重放"机制(决策不从日志重放副作用),原子性暂无消费方;等做崩溃重放时一并上。栅栏三验(PID+启动时间+run-id):蓝图针对 detached 进程对账,当前 worker 是进程内 tokio task、fence 已足,等 worker 真 detached 化再做。

## 蓝图对照地图(autonomous-org.md,轮19 盘点;✓=已建 ◐=部分 ✗=未建)

**甲部·机房**
- §4 Agent 运行时:◐ ClaudeRunner(spawn/resume/process_group)+AgentRunner trait;✗ --json-schema 结构化输出、--mcp-config 自定义工具、权限回调、hooks
- §5 编排:✓ 经理循环/决策→Effect→执行/复核裁决/唤醒/熔断/序号单调;◐ 崩溃恢复(requeue+经理接管✓,saga 重放✗);✗ 子经理递归、§5.6 注入封顶、决策税完整版
- §5.7 合并列车:✗(merge_and_reverify 已建但休眠;**依赖沙箱**)
- §6 记忆:✓ episode 机械绑定/双时间事实表/简报/决策留痕/claude 提炼器(显式开)/矛盾裁决机制(FakeJudge);✗ 作废接运行路径(ClaudeJudge)、sqlite-vec 混合检索接线、实体词表、待审区流转、信任档自动升级
- §7 权能分权:✓ 思想上贯彻(worker 无合并权、AI 不设可信度);◐ 工具白/黑名单未接 --allowedTools
- §8 安全:✗ **沙箱(蓝图说"最优先,现在一行没写"——下一个大刀)**、独立审计变异式(有 clean_clone_verify 简版)、员工断网、记忆注入面;✓ 急停 killpg
- §9 成本:✓ 预算闸(滚动窗)/烧钱显式开关/熔断;✗ 配额感知串行、模型分工自动化、--max-budget-usd 进程级(worker 角色已接!)
**乙部·给人用的**
- §10 复盘:✓ decision_log+工作台回放/episode 时间线;✗ 因果树视图、注入快照逐回合、OTel 导出
- §11 测试:◐ fake-claude 桩+全套单测;✗ 铁律对抗测试、崩溃混沌测试、指标闸
- §12 验收台:◑ 晨报按结局三分类(轮24)+**晨报内一站式打回按钮(轮25:卡住/升级的任务直接打回,点后真产生新活被经理领走;onRequeue 抽成 App 共享回调,晨报+工作台复用)**、escalate 状态条喊人;✗ 验收"接受/合并"按钮(real 才有分支可合)、人机边界策略配置、叫醒模型
- §13 回滚:✗(撤单次合并外的粒度、记忆级联失效、命名还原点)
- §14 人事:✓ 三角色配置+版本化+真接线;✗ 模板库导入导出、产出统计闭环
- §15 自我升级 / §17 多仓库 / §16 开张引导:✗
**下一个大刀建议**:§8 沙箱(Seatbelt)——它是合并列车、真自治放开手的总前置;单独规划一刀做穿。

## §8 沙箱·进度(轮20 开工)

- [x] **沙箱片1:verify 收进沙箱**(轮20) — 勘察纠正:`sandbox.rs` 的 SandboxPolicy 策略+渲染**早已完整**(蓝图"一行没写"过时),真缺口是没人调 `.wrap()`。① **profile 策略实测定调**(§24):纯 `(deny default)` 白名单在新版 macOS/SIP 下连 dyld 都放不全(`/usr/bin/true` 都跑不起来)→ 翻转为 `(allow default)` + 黑名单收死「写」(deny 全盘 / allow 回 worktree)和「网」(deny network*);② **canonicalize 坑**:seatbelt 按内核真实路径匹配,`/tmp`→`/private/tmp` 不归一会让写被误拒,for_worktree 已 canonicalize;③ verify.run_capturing 在 macOS 包 sandbox-exec;④ **保住 §7 区分**:包沙箱前预检程序可执行性(绝对路径不存在→VerifyError 而非被 sandbox-exec 吞成 VerifyFailed);⑤ 顺手修 2 个 fake-claude 序列积压回归测试(轮1 改脚本后没跑到的集成测试,断言改成形状而非固定下标)。真 app 实测:verify 写 worktree 外**被沙箱拦住**、正常 verify 仍 verified。core+app 124 测试全绿。
- [x] **沙箱片2:worker(claude)进程收进沙箱**(轮21) — SandboxPolicy 扩展:加 deny_read_paths(§8.3 密钥黑名单:~/.ssh/.aws/.config/gh/.netrc/**.agents(千问 key)**/.config/gcloud/.kube)+ restrict_writes 开关 + for_worker()(allow default+留网+禁读密钥,**暂不收紧写**——claude 要写 ~/.claude 运行时,贸然禁写全盘会崩,写隔离留片2b 实测各路径后做)。ClaudeRunner 默认包沙箱(without_sandbox() 逃生舱);drive 包裹前预检 bin 可执行(保 SpawnFailed 语义,不被 sandbox-exec 吞成 NoResult)。真 app 实测:worker 经 sandbox-exec 包裹仍 verified(包裹不破坏)。125 测试全绿。**所有 claude 进程(经理/员工/图书管理员)现在都禁读密钥(§8.6 I7)**。
- [ ] **沙箱片2b:worker 写隔离** — 实测真 claude 跑一单,确认它写哪些路径(~/.claude/?/tmp/?),再把 for_worker 的 restrict_writes 开起来 + allow worktree + allow claude 必需的运行时写路径。需真 claude(烧钱),留用户在时做。
- [x] **沙箱片3:git「检出即执行」口子封堵**(轮22,§8.2 蓝图最狠一条) — `SAFE_GIT_FLAGS`(git/mod.rs 导出):`core.hooksPath=/dev/null`(钩子)+`core.fsmonitor=false`+`core.attributesFile=/dev/null`(filter/diff driver 触发点)+`core.symlinks=false`(符号链接逃逸)。注入两处会检出的命令:audit clean_clone(clone+checkout,另加 `--no-recurse-submodules`)、GitGuard.worktree_add(检出新分支也触发 smudge)。**坑(实测纠正)**:`protocol.file.allow=never` 会误杀可信本地主仓库 clone(file 源),且现代 git≥2.38 默认已禁子模块 file 协议(CVE-2022-39253)——故去掉它,子模块靠 `--no-recurse-submodules` 挡。新增对抗测试:恶意 .gitattributes smudge 过滤器在审计克隆时**不执行**(PWNED 文件不出现)。core 91+集成 14+app 34 全绿。

## §8 沙箱·三片完工小结(轮20-22)
verify 进沙箱(写隔离✓) → worker/经理/图书管理员进沙箱(禁读密钥+留网✓,写隔离待片2b实测) → git 检出即执行口子封堵(钩子/属性过滤器/子模块/符号链接✓)。**§8 安全本体从"一行没写"到三道防线就位**:进程被 seatbelt 罩 + 密钥读不到 + 恶意仓库检出不执行。剩:片2b worker 写隔离(需真 claude 实测)、独立审计变异式验证(有 clean_clone 简版)、员工真断网(本地转发器,大刀)。
- [x] **真合并列车接通(§5.7)**(轮23,沙箱三片完工后前置满足) — 经理 Deliver real 任务(有分支)→ `merge_and_reverify`(冲突探测→merge_no_ff→合并后重验→绿保留/红 reset main)合进 main,按结果改状态(merged / needs_rebase 留人工)。ProjectManager 加 MergeLock(交付全局串行)。**simulate 无分支 → 跳过合并不破坏**(实测 verified 仍 verified、决策弧含交付)。merge_and_reverify 自带"重验红→reset main"护栏守「main 永不坏」。merge 3+app 34 单测全绿+release。**真合并端到端需 real(有真实代码改动+分支)实测,留用户开 real 时验**(护栏在,坏不了 main)。**至此"自治公司推进项目"闭环理论完整:派活→干活→复核→交付→合进 main**。

## 验证方式(每轮)

- `QUIVER_FAKE_DELAY_MS=0 cargo test -p quiver-app -p quiver-core -p quiver-store -p quiver-orchestrator -p fake-claude` 全绿
- `yarn --cwd frontend tsc --noEmit` 干净
- 桥接器:`scripts/agent-debug.sh up` → eval 驱动(开自治→派活→读决策流 DOM)→ down。**模式必须 simulate**。
- ⚠️ 改了 Rust 必须重建二进制(`scripts/agent-debug.sh down && up` 会重建)才能在真 app 验证。
- ⚠️ **down 可能没真停**:它只认"本脚本启动的实例记录",记录丢了就 no-op(报「没有由本脚本启动的实例记录」)——此时 up 也不会再起新的,probe READY 探到的是**旧二进制的 bridge**!重启敏感的验证(如 reconcile)前先 `pkill -9 -f target/debug/Quiver` 杀干净再 up,并核对 app 启动时刻 vs 测试数据写入时刻的先后。

## 迭代日志

- 〔轮1 2026-06-10〕接通记忆简报→经理上下文;决策流加血肉;fake-claude 可信工作弧;摄像头/弹窗/点外关三处交互修复。全套测试绿(fake 10/core 70/app 29)+tsc 干净。
- 〔轮1·真 app 实测全过〕决策流:`派活 ▸「做转写的导出功能」· 有预算、在途未满、有排队 · 1/1在途 预算充裕`(血肉✓);worker 3.5s 仍在干活(fake_delay_ms=30 下全程 ~7.4s,玩具感✓);点小人 world transform 纹丝不动(摄像头✓);worksurf 秒开+点框外即收(交互✓)。
- 〔轮1·⚠️事故+修复〕**经理循环曾连环起真 claude 烧额度**:旧 manager_brain 按 `default_mode=="real"` 自动选 ClaudeBrain,用户 default_mode 恰是 real → simulate 任务被真 claude 经理想,且 Escalate 不等待立即下一拍 → 连环起进程(实烧 2 次短决策调用,已杀干净)。**修复两刀**:① manager_brain 恒 RuleBrain,ClaudeBrain 等人事部显式「经理大脑」配置才接(烧钱大脑绝不搭别的设置便车);② manager_loop 只有 Spawn/Continue/Deliver/Block(真改变局面)才立即下拍,Nothing/Escalate/Refresh 一律等待 + 每循环 200 拍硬熔断(§5.8)。教训已写进 lib.rs manager_brain 注释。
- 〔轮1·发现〕用户 DB:`default_mode=real`、`fake_delay_ms=30`(都是用户自己的设置,代码要适配它们,别动用户数据)。fake-claude 思考停顿已给 250ms 地板,与 fakeDelayMs 解耦。
- 〔轮2 2026-06-10〕**A1 经理复核裁决落地**。真 app 实测完整决策弧(5 拍干净):`#0 派活▸「做转写的导出功能」→ #1-2 按兵不动(等工) → #3 交付「做转写的导出功能」→ #4 收工`——经理真的在"看产物终态、替你回 AI 的决策"了。顺手修:TICK_IDLE_TIMEOUT 800ms→5s(兜底超时拍曾把决策流刷出 6 拍 noop,真大脑下每拍都是钱;唤醒主靠 notify,超时只是防丢保险)。测试 59 通过 0 失败(orchestrator+app),tsc 干净。
- 〔轮2·注意〕app 集成测试(run_task 套件)因 fake-claude 250ms think 地板变慢(~17s)——它走 settings 默认 fake_delay_ms=120 而非 env。能忍;若 CI 嫌慢,在测试 settings 里把 fake_delay_ms 设 0 即可(think 对 delay=0 整体跳过)。
- 〔轮5-10 2026-06-10〕**C1+A2+C2 连续落地**(轮6-9 被 cron 切片,跨轮接力完成)。C1:决策留痕 episode 实测落库+时间线穿插显示;A2:经理拆活(next_task 进 ctx、ClaudeBrain prompt 嵌任务原文+改写指示+待复核清单、执行层用经理改写的 prompt 派工,QUEUE_NEXT_PLACEHOLDER 区分占位),94 测试过,RuleBrain 回归决策弧不变;C2:简报可见化(经理在想什么 ▾),实测 10 条记忆全显。**四支柱现状:A 自治(派活+复核裁决+风暴熔断)✓ B 配置(人事部 UI+真接线+显式烧钱开关)✓ C 记忆(brief 注入+决策留痕+可见化)✓ D 体感(决策流血肉+工作弧+交互三修)✓——骨架全通,后续是深化(D1 工位停留、真合并列车、librarian 换 claude、崩溃恢复硬化)。真 app 实测:⌘K→人事部渲染经理/员工两卡;改员工预算 1.5→落库 `worker|1.5|v2`(version+1✓)→显式清空→`worker||v3`(双层 Option 清除✓)。桥接器经验:**React onBlur 要 dispatch `focusout`**(不是 `blur`,React 17+ 委托 focusout)。测试 64 过(app 30/store 34)+tsc。已还原用户配置。
- 〔轮3 2026-06-10〕**B1 人事部地基落地**。真 app 实测:agent_role seed 两行(`manager|经理|rule|sonnet|v1`/`worker|员工`)、派活无真 claude 进程(brain=rule 守住)、决策弧照常(派活→交付)。测试 93 通过 0 失败(store 34/app 30/orch 29)+tsc 干净+release check 过。**B2 注意**:UI 改经理 brain='claude' 后,经理循环是 per-project 缓存的(ProjectManager.brain 首次定)——改 brain 对已存在的 project 循环不生效,要么重启 app、要么 B2 顺手把 ManagerLoop 加"配置变了重建 brain"(update_role 后调,类似 resume_all 但换脑)。别忘这个,否则用户改了开关以为生效了。

## 轮 25-37 进度补记(共享记忆追平,2026-06-10)

> 25-36 轮口头汇报了但没记进文件,这里集中补。全程 simulate 免费验证,配置每轮还原。

- **轮23 真合并列车接通(§5.7)** — 经理交付 real 任务(有分支)→ merge_and_reverify 合进 main(冲突→needs_rebase、重验红→reset main 护栏);simulate 无分支跳过不破坏。ProjectManager 加 MergeLock。
- **轮24-25 晨报验收台(§12)** — 晨报按结局三分类(合进 main/卡住等你/升级等你拍板,真数据)+ 一站式打回重做按钮(onRequeue 共享回调);escalate 状态条喊人。
- **轮26 并发实时生效 + 批量派活** — InFlight::set_max,manager_loop 每拍同步 settings.max_workers(轮3 缓存坑彻底修);「派一批活」命令。
- **轮27 两个真 bug 修复** — ①current_project 重启从 last_project 自愈(否则重启瘫痪);②enqueueBatch 不再静默吞异常假报成功。教训:前端 catch 绝不空吞。
- **轮28 并行协作实测** — max=3 派一批 → Monitor 抓到同时 running=3 + 决策流连续派 3 件不同活。
- **轮29 预算配额感知(§9)** — RuleBrain 预算不足支撑满并发时收敛(BUDGET_PER_SLOT_USD=0.5),极紧串行但空闲不饿死。
- **轮28(git) 8 批分批提交** — 28 轮工作按依赖分 8 个 commit(store→orchestrator→memory→core→fake-claude→app→frontend→docs)。**之后每轮都即时提交**。
- **轮30 每员工独立配置(§14)** — seed 三个独立员工(worker/worker-2/worker-3);run.rs 按 task_id 在员工间稳定分配,并行任务各用各自配置。
- **轮31 员工专长 + 智能派活** — agent_role 加 specialty;run.rs 先按专长匹配任务关键词派给对口员工。**并修迁移顺序 bug**:execute_batch 里 INSERT 引用新列必须先 add_column 再 seed,否则既有 DB 启动 panic。
- **轮32 CEO 雇人/裁员(§14)** — create_role/delete_role(只删 worker,经理/图书管理员单例保护);人事部「+雇个员工」+ 卡上「裁员✕」(至少留一人)。
- **轮33 claude 经理看团队专长指派** — ManagerContext 加 team(员工专长清单);ClaudeBrain prompt 嵌团队 + 指示经理改写任务突出专长 → run.rs 派给对的人。真指派端到端需 claude 经理。
- **轮34 CEO 注入权威事实(§6.2)** — add_authoritative_fact IPC,工作台「注入知识」框;权威档进简报、影响 claude 经理决策。
- **轮35 机械矛盾消解(§6.4)** — supersede_lower_same_entity:CEO 录入带主题的权威事实 → 作废同主题更低可信度的旧事实(按 trust_rank 五档,无 AI、可单测)。实测旧「RocksDB」被新「SQLite」作废。
- **轮36 控制条直达入口** — 经理/人事部从 ⌘K 提为常驻按钮(回应"配置看不到")。
- **轮37 整体回归** — 全套 **240 测试通过 0 失败**;系统整体健康。

### 对照地图更新(✗→✓ 的)
- §5.7 合并列车:✗→**✓ 接通**(真合并端到端需 real 验,护栏在)
- §8 安全:✗→**◐ 三道防线就位**(seatbelt 罩进程+禁读密钥+git 检出不执行;剩写隔离收紧/审计变异式/断网转发器)
- §9 成本:◐→**✓ 配额感知+烧钱显式开关+熔断**
- §10 复盘:◐→**◑ decision_log 落库+跨重启回放+晨报三分类**(剩因果树视图)
- §12 验收台:◐→**◑ 晨报三分类+一站式打回+escalate 喊人**(剩接受/合并按钮)
- §14 人事:✓→**✓✓ 每员工独立配置+专长+雇人裁员+智能派活**(剩模板库导入导出)
- §6 记忆:◐→**◑ 决策留痕+CEO 注入权威+机械矛盾消解+提炼器(claude)**(剩 ClaudeJudge 语义消解、向量检索接线)

### 仍需 real(烧钱,待用户在场)
claude 经理真思考/真拆活/按专长真指派、真合并列车端到端、worker 写隔离收紧、librarian 真提炼。机械层链路都建好了,开 claude 即生效。

## 轮 38-47 进度补记(2026-06-10,大量回应用户真用产品的实测反馈)

> 用户开始真用(切 real 跑任务),连续暴露体验缺陷。这十轮多在补"产品作为产品"的完整性 ——
> 教训:从用户实际使用视角检查,可观测性是命脉,别只建后端能力。

- **轮38 系统设置页 + 派活跟随模式(修真 bug)** — 用户发现:①没系统配置入口(shell 里压根没
  Settings 组件)②切 real 还跑 simulate(enqueueTask 硬编码'simulate')。建系统设置页(模式/模型/
  并发/预算/验证命令);派活跟随 settings.defaultMode。顺手:图书管理员→记忆官、CEO 多行下目标。
- **轮39 UI 塌陷修复 + CEO 接受合并** — 用户截图:①人事部卡片塌陷(误用决策流 .rev 横排样式)
  ②顶栏 HUD/控制条重叠(各自 fixed 相撞)。修:卡片独立纵向样式、顶栏 flex 容器。
  + merge_task_cmd:晨报点「合进 main」(§12 验收台接受闭环,走合并列车护栏)。
- **轮40 经理大脑入设置页 + 完整真实自治** — 用户困惑"切真实经理还是规则":运行模式/经理大脑/
  自治三开关割裂。设置页一处配齐,三个都 real 亮「完整真实自治已就绪」,缺自治时警示"经理不决策"。
- **轮41-44 可观测性大补(核心)** — 用户跑 real "什么都看不见、很迷茫"。根因:事件全留着但没 UI。
  建**追溯室**(所有任务+点开看 claude 每一步:思考/工具/结果/终态)→ v2 加搜索/模式/状态/员工
  筛选+思考折叠可读+渲染封顶 → 加经理决策线(一任务的经理+员工全程)→ 按员工追溯(task 加
  worker_role)。**点小人=员工工作台**(Worksurf 实时可读轨迹+跳追溯室)。
- **性能整机掉帧根治** — 用户"应用打开整机卡、鼠标掉帧"。量化:73 filter:blur+26 backdrop+18
  blend=99 个 GPU 模糊层。砍 blur(辉光→radial mask、阴影去 blur、关闭面板不渲染 backdrop)→
  99→17。**认知纠错**:opacity/transform 动画是合成器层面 GPU 不耗,误砍后恢复(LED 闪+浮尘),
  实测恢复 93 动画 fps 仍 102。画质:DPR=1(用户 2560x1440 非 retina 屏)→ 房间标签 counter-scale
  (净缩放 1:1)治糊;真正 GPU 杀手是 filter:blur 不是动画。
- **轮45 记忆库** — CEO 看见并管理公司记住的所有事实(按可信度档排),录入权威+作废记错/过时的
  (invalidate_fact 无替代纯作废)。"强大记忆决策"从能写到可见可管。
- **轮46 CEO 调教经理工作准则** — system_prompt 建好但 claude 没读/UI 没暴露。接通:人事部经理卡
  「工作准则」→ 注入 claude 经理决策最前。CEO 用一句话塑造经理风格。
- **轮47 整体回归** — 全套 **240 测试 0 失败**,系统健康。

### 对照地图更新(✗/◑→✓)
- §10 可观测:◑→**✓ 追溯室(任务+经理+员工全程,搜索/筛选)+ 点小人实时轨迹**(剩因果树图)
- §12 验收台:◑→**✓ 三分类+打回+CEO 接受合并**(剩人机边界策略)
- §14 配置:✓✓→**✓✓✓ 系统设置页+经理工作准则+每员工+专长+雇人**(剩模板库导入导出)
- §6 记忆:◑→**✓ 录入+机械消解+记忆库浏览管理(作废)**(剩 ClaudeJudge 语义消解、向量检索)
- 性能/体验:**整机掉帧根治(GPU 99→17)、画质 DPR=1 适配、UI 塌陷/重叠修复**

### 教训(写给后续轮)
- **GPU 性能**:杀手是 filter:blur / backdrop-filter / mix-blend-mode(每帧重新模糊采样),
  不是 opacity/transform 动画(合成器层面便宜)。别误砍动画牺牲生命力。
- **用户视角**:基础可用性(设置入口/模式生效/看得到结果/不卡)和深层能力一样重要,且更先暴露。

## 轮 48-55 进度补记(2026-06-10)

> 主线:让 **claude 经理(真"聪明"的载体)从"建好待开、怕烧钱不敢验"变成"免费可跑可验证可
> 预演"** + 失败自愈/调度台(自治更可控) + 一批体验根因修复(尤其字糊)。

- **CEO 调教经理工作风格** — system_prompt 建好但 claude 没读/UI 没暴露。接通:人事部经理卡
  「工作准则」→ 注入 claude 经理决策最前。CEO 一句话塑造经理判断风格(谨慎/激进)。
- **失败自愈(绝对自治硬核)** — 验证失败的活,公司自己再试一轮(带返工会话续跑),到上限
  (MAX_AUTO_RETRY=2,首跑+重试1次)才停手等 CEO。task 加 attempt 列。实测 verify 红→拦下→
  自动重试1次(task-retry-2)→再失败停手。
- **调度台(自治可控)** — CEO 看排队/在跑的活,调优先级(经理按 position 认领)、取消/叫停。
  随 task-updated 实时刷新。"绝对自治"不等于 CEO 失控。
- **小人真实身份+专长** — 办公室小人/Worksurf 从"员工#N"接 worker_role+specialty → "员工2·
  测试"。CEO 一眼看到专长分工(聪明协作最直观)。
- **★ claude 经理 simulate 免费跑通(里程碑)** — 一直回避:claude 经理从没端到端验证,怕烧钱。
  补:fake-claude 识别决策 prompt→输出合法决策 JSON(先裁后派),走 ClaudeBrain 真路径;
  manager_brain simulate 用 fake bin 免费跑(spawn→parse→执行)。意义:验证开真 claude 前管道
  无断点 + 用户免费预演 claude 经理完整工作流。实测 spawn→deliver 跑通无 escalate。
- **一键预演 + 经理大脑可见** — 设置页一键「预演 claude 经理(免费)」(设齐 simulate+claude+
  自治);工作台显示大脑状态(规则/claude·预演/claude·真思考)。激活上述能力让用户可达可见。
- **CEO 接受合并(§12 验收闭环)** — 晨报 verified+有分支任务「合进 main」走合并列车护栏。

### 体验根因修复(用户真用产品逐个暴露)
- **字糊真凶(非 DPR)** — 用户"顶部入口清晰、其他模糊"线索定位:.world 用 transform:scale,
  fit 算 1.43 **放大**世界→整层栅格化成纹理再 GPU 上采样→文字糊(控制条不在 world 里所以清晰)。
  修:computeFit 上限=1 绝不放大;移除帮倒忙的 counter-scale(它让标签在纹理里更小再放大、更糊)。
- **非 retina 字糊** — body 的 -webkit-font-smoothing:antialiased 在 DPR=1 屏关掉了 subpixel
  渲染→糊。媒体查询 max-resolution:1.5dppx 下改回 subpixel-antialiased。
- 教训:**transform:scale 放大 = 纹理上采样必糊**(retina 也糊);别用 counter-scale 救(更糊)。
  清晰优先就别放大(1:1 或缩小)。性能上 GPU 杀手是 filter:blur 不是动画(轮38-47 已记)。

### 对照地图更新
- §5 编排:✓→**✓ + 失败自愈(经理自动重试)**;claude 经理 simulate 可跑(验证管道)
- §14 配置:✓✓✓→**✓✓✓ + 经理工作准则(CEO 调教风格)**
- 体验:**字糊根因(world 放大/antialiased)修复、调度台、小人真实身份**

### 仍需 real(烧钱,待用户在场)
真 claude 经理的**真智能**决策/拆活/指派(simulate 是 fake 桩,验证管道用);真合并端到端;
worker 写隔离收紧;librarian 真提炼。一键预演已让用户免费看到完整流程形态。

## 轮 56-63 进度补记(2026-06-10)

> 主线:把"聪明协作"从概念做成完整闭环(拆解→分工→并行→汇总),+ 房间标签字糊根治。

- **★ 任务分解/协作(里程碑)** — "协作工作能力"的质变:从"一个目标=一个员工"到"经理拆解
  大目标→多员工分工并行"。Decision::Plan{subtasks}+Effect::Plan:经理决策拆活→子任务入队
  (不占在途名额)→后续逐拍 spawn(按并发/专长)。fake-claude 经理:队首任务含顿号分隔多子目标
  →输出 plan 决策(真 claude 接入后智能拆)。实测下"做登录、做注册、做权限"→拆 3 子任务并行
  →全 verified。
- **协作树可视** — task 加 parent_goal;子任务标父目标文本。追溯室同 parent_goal 的子任务前插
  协作组头「⑃ 协作目标：X · M/N 子任务完成」+ 子任务缩进。CEO 一眼看出"这几个是同一目标
  拆出来分工的"。
- **父目标 planned 修复** — bug:execute Plan 认领父目标(→running)却没人干它→卡 running 僵尸。
  修:拆活后父目标标 planned(已拆解)。
- **协作汇总(闭环)** — maybe_complete_parent_goal:子任务全完成回流时,该目标所有子任务都到
  终态→父目标终结。全成功→done(协作完成);有失败(含 verify_failed)→needs_rebase(进晨报
  等你处理)。实测:全成功→done;verify 红全失败→needs_rebase(不卡 planned)。
- **★ 房间标签字糊根治** — 用户决定性线索"Mac 内屏(DPR=2)不糊、外接 2K 屏(DPR=1)糊"锁定:
  标签在 .world 里被相机 transform:scale 放大,DPR=1 纹理 1x 上采样必糊(物理限制,CSS 调不出)。
  根治:把标签**移出 world**,Office 屏幕空间浮层渲染,用相机 project(world→屏幕)定位→跟着
  办公室走但文字 1:1 永不缩放,DPR 无关地清晰。前几轮试 CSS(fit≤1/去 will-change)走了弯路。
- **轮63 回归** — 全套 240 测试 0 失败;浮层标签 z-index 无副作用(scrim 9500 盖标签 50)。

### 对照地图更新
- §5 协作:◑→**✓✓ 拆解→分工→并行→汇总完整闭环 + 协作树可视 + 失败收尾**
- 体验:**房间标签字糊根治(屏幕空间浮层,DPR 无关清晰)**

### 教训(写给后续轮)
- **DPR=1 屏 + transform:scale 放大 = 文字纹理上采样必糊**,CSS 调不出来。要清晰就别让文字
  在被 scale 的层里 —— 移出做屏幕空间浮层、坐标投影定位。retina 屏掩盖了这个问题(2x 纹理)。
- bridge shot 受屏幕录制权限挡、看不到视觉时,别反复试 CSS 参数,直接上架构正确的方案。

## 轮 64-70 进度补记(2026-06-10)

> 主线:把"强大的记忆决策"做厚(记忆贯穿决策与执行)+ 协作成果带到 CEO 全视线。

- **记忆注入派活** — fake 经理 spawn 时从决策简报抽 CEO 录入的权威事实,注入派给员工的活
  ("做导出"→"做导出(公司约定:用 SQLite)")。"记忆→决策→执行"simulate 也看得见(机制真实,
  fake 桩选第一条权威,真 claude 智能选最相关)。
- **协作子任务也注入记忆** — 统一注入:经理拆活时每个子任务都带公司约定→协作每个员工都遵循。
- **决策理由体现记忆** — 用了记忆的决策 reason 标「遵循公司约定:X」→ 决策流直接看到经理在
  基于记忆决策(不只藏在派出的任务里)。
- **晨报突出协作成果** — 晨报顶部单列「⑃ 协作成果」段(N/M 子任务完成+状态),协作父目标从
  普通分类排除不重复。useMorningReport 加 collabGoals;TaskStatus 补 planned。
- **气泡剥记忆尾巴** — 小人头顶气泡剥掉"(公司约定:…)",只留任务名(记忆约定在追溯室看完整)。

### 对照地图更新
- §6 记忆决策:✓→**✓✓ 记忆贯穿决策(reason)+执行(派活/子任务),simulate 全链路可见**
- §12 晨报:✓→**✓ + 协作成果段**

### 现状(诚实)
四大支柱在 simulate(免费)都有完整闭环、真 app 验证、且在 CEO 每条视线(办公室/工作台/追溯/
晨报)可见。机制全真实,只有"经理选哪条记忆/怎么拆/派给谁更优"是 fake 桩 —— 真 claude 接入
即从机械变智能。剩余真智能需用户开 real(烧 headless 额度),骨架/链路/可观测/配置/预演/安全
边界全铺好验证过。一键预演让用户免费看完整流程形态。

## 轮 71-74 进度补记(2026-06-10)

> 主线:从"功能可见"转向"敢托付" —— 给开 real 前的关键安全/正确护栏补自动测试守护。
> 认识:用户反复"达不到"的根本之一可能是缺信任基础(让 AI 真改代码/用密钥环境)。

- **协作组头参与员工 + 气泡剥记忆尾巴** — 追溯室协作组头显示"员工2、员工3 分工";小人气泡
  剥掉注入的"(公司约定:…)"只留任务名。
- **协作拆活单测** — Decision::Plan→Effect::Plan 固化:直通子任务、不占在途名额、空拆=Nothing。
- **端到端走查** — bridge 走"一键预演→CEO 记忆→复合目标→拆活+记忆注入→协作→汇总",链路
  端到端一致(决策体现记忆、每个子任务带公司约定、关联父目标、协作完成)。
- **★ merge 护栏测试(信任)** — 「main 永不坏」铁律首次有测试(真临时 git repo):绿合并→main
  推进文件落地;合并后重验红→main reset 回合并前(坏改动不留);冲突→main 一字不动。
- **★ 密钥全目录测试(信任)** — secret_dirs 每一个(.ssh/.aws/.config/gh/.netrc/.agents/
  .config/gcloud/.kube)都必须进 seatbelt 禁读,防误删让 real worker 读到用户凭据。
- **轮74 回归** — 全套 **246 测试 0 失败**。

### 对照地图更新
- §5 协作:✓✓(+单测固化);§8 安全:◐→**◐+(密钥全目录回归网);§5.7 合并护栏首次有测试**

### 现状与方向(诚实)
simulate 能做的核心全做透、可见性全覆盖、安全护栏有测试、免费预演能先看。"敢托付的自治
公司"骨架完成。真智能(经理理解语义拆活/择优选记忆派人)需用户开 real(烧 headless 额度)。
信任基础(main 永不坏/密钥不外泄/预算闸/失败自愈上限)都有测试守护 —— 开 real 的路铺平加护栏。
**下一步最高价值:用户给具体靶子,或开一次 real 看真智能天花板。**

## 轮 75-78:真跑 real,挖出并修复真路径 bug(2026-06-10)

> 用户授权"直接用真实 claude 测试跑真任务"。真跑挖出 simulate 永远发现不了的真 bug,
> 把"绝对自治/聪明协作/记忆决策"从"看着通"推进到"真的通"。成本每任务 $0.15-0.33。

- **★ 真交付断裂(修复)** — claude(acceptEdits)只改文件不 commit,supervisor 也没提交 worker
  改动 → 分支 HEAD 停在 base、合并空、reverify 红 → needs_rebase。real 交付一直是断的,
  simulate 不真改码、real 没端到端跑过而潜伏。修:GitGuard::commit_worktree。实测 merged。
- **★ 真经理不会拆活(修复)** — ClaudeBrain 决策 prompt 漏教 plan,真 claude 只会 spawn 不会
  拆解协作。修:prompt 补 plan。实测:复合目标→真 claude 自己拆 2 子任务→2 worker 真建文件
  (utils.py+CHANGELOG)→都 merged。
- **★ 真记忆决策(接线本就对,验证通过)** — CEO 录权威约定"函数名 q_ 前缀+中文 docstring"→
  真 claude 经理读 brief 主动把约定具体化进派的活(任务没提约定)→ worker 产出 q_factorial
  +中文 docstring。真 claude 运用记忆比 fake 机械附加聪明得多。
- **三块核心真路径全通** — 真交付/真协作拆活/真记忆决策。两个致命 bug 都是真跑才暴露的。

### 对照地图(真路径)
- §5 自治交付:✓✓ **真合并到 main(commit_worktree 修复后)**
- §5 聪明协作:✓✓ **真 claude 拆活+多员工真协作(plan 修复后)**
- §6 记忆决策:✓✓ **真 claude 读记忆主动运用进决策与执行**

### 教训
- **simulate 桩跑通 ≠ real 真路径通**。两个致命 bug(不提交/不拆活)都藏在"机制看着完整"
  之下,只有真跑才暴露。用户开 real 是对的 —— 真跑才发现真问题。

## 轮 79-80:异常路径 + 过夜多目标真跑(2026-06-10)

- **失败自愈真路径** — verify 必红 → 经理 block → 自动重试(attempt 1→2)→ 重试上限 → escalate
  升级给人。main 永不坏(坏代码反复失败始终没进 main)。
- **空转 spawn 修复** — 队列空时 claude 经理仍每拍 spawn,execute claim None 回滚却被判推进 →
  立即重 tick 空转烧钱。修:execute_effect 返回"是否真推进",claim None→false→经理等待。
  prompt 双保险:排队 0 别 spawn。实测 spawn 5→2,改 escalate。
- **★ 过夜多目标(产品意象验证)** — 连派 3 独立目标、并发 2:经理 spawn spawn→deliver deliver
  →spawn→deliver,并发上限遵守(worker 时间区间重叠证实)。3 个全 merged(power/reverse/gcd
  各自落到 main.py/stringutils.py/mathutils.py)。晨报准确汇总:"合进 main 3·花费 $0.59"。
  **一次跑通无 bug** —— 前 5 轮修复(commit/plan/空转)在完整流程全生效。

### 连续 6 轮真跑总结(轮 75-80)
真交付(commit_worktree)/真协作拆活(plan prompt)/真记忆决策(验证)/真自愈+main永不坏/
空转修复/过夜多目标。**正常+异常+工作流全部真 claude 端到端验证通过**。
方法论:每轮真跑都挖出 simulate 发现不了的真 bug —— "达不到"的根源是 simulate 假象盖住了
真路径断裂。真跑逐一逼出、修掉。

## 轮 81-82:真冲突 + 崩溃恢复真跑(2026-06-10)

- **真合并冲突护栏** — 2 并发任务改 main.py 同处:一个 merged、一个 needs_rebase(冲突挡住),
  main 干净无冲突标记、不自动解决。merge 护栏真路径与单测一致。
- **.quiver/ 污染用户项目(修)** — worktree 建在 <repo>/.quiver 却从不 gitignore → 用户真实
  项目 git status 多 ?? .quiver/(realtime-voice 残留 25 项)。修:创建 worktree 时写
  /.quiver/ 进 .git/info/exclude。已清 realtime-voice 残留。
- **崩溃恢复(核心 OK + 孤儿 worktree 泄漏修)** — 强杀 app(worker 跑一半):重启 reconcile
  requeue → 中断任务自动重试 → binary_search 合进 main(任务不丢,核心成立)。但孤儿 worktree
  泄漏:sweep 时孤儿进程还在写、remove 失败,退出后变 prunable 而 sweep 碰不到 → 累积。
  修:sweep 先 force remove prunable worktree。已知留:孤儿 worker **进程**跑到自然结束(成果
  不采纳),进程级清理需 PID 追踪。

### 连续 8 轮真跑(75-82)总结
真交付/真拆活协作/真记忆决策/真自愈+main永不坏/空转修复/过夜多目标/真冲突护栏/.quiver污染/
崩溃恢复+孤儿清理。**正常+异常+工作流+冲突+崩溃全部真 claude 端到端验证**。8 轮挖出 6 个
simulate 发现不了的真 bug(交付/拆活/空转/.quiver/孤儿worktree + 崩溃恢复粗糙)。

## 轮 83:崩溃恢复孤儿进程清理(2026-06-10)

- **孤儿 worker 进程 kill(修)** — 上轮留的最后一块:app 强杀时 claude 子进程成孤儿、续跑烧额度。
  PID 原只在内存(崩溃即丢)。修:worker_pid 持久化进 task 表;reconcile 读 running_task_pids、
  ps 验证确是 claude 再 kill(防 PID 复用误杀),再 requeue 清 PID。真跑实测:worker PID 52268
  写进 DB→强杀 app 孤儿续跑→重启 reconcile 清掉它+崩溃任务自动重试 merged。
  至此崩溃恢复卫生闭合:任务不丢 + 孤儿 worktree 清 + 孤儿进程 kill。

### 9 轮真跑(75-83)闭合
真交付/拆活协作/记忆决策/自愈+main永不坏/空转/过夜多目标/真冲突/.quiver污染/崩溃恢复(任务恢复
+孤儿worktree+孤儿进程)。**正常+异常+工作流+冲突+崩溃全链路真 claude 验证 + 卫生闭合**。

## 轮 84-90:预算硬闸 + 双向协作(用户点名核心)(2026-06-10)

> 用户打断、点明最缺「双向聪明协作」。主线:从单向传送带做成真团队(经理↔worker↔CEO 三向闭环)。

- **预算硬闸(real 真跑修)** — execute 派活前根本没硬闸、只靠 claude 自觉,且 ctx.budget 快照
  滞后一拍 → cap $0.3 烧到 $0.51。修:硬闸**实时重算**预算,耗尽绝不派、剩余留队列。
- **记忆官 simulate 一致性** — distill 固定 resolve_agent_bin(Real)→ simulate 下记忆官 claude
  偷烧 real。修:跟随 default_mode(simulate→fake 输出 []、不造假事实)。「simulate 全免费」无缺口。
- **专长派活** — 无专长任务按 task_id 散开会占测试/前端员工。修:优先通用员工,留专长员工接专长活。
- **★ 双向协作 经理→worker** — 用户点名核心。根因:Effect::Continue 是空壳、且要求 node 在途
  (完工即下线)→ 续跑永远被拒、协作单向死。修:Continue admit 新名额、worker --resume 带经理
  指导迭代改进;PendingReview 加 round 防无限。实测 spawn→continue(指导)→resume→deliver。
- **★ 双向协作 worker→经理** — worker 卡住主动请示(prompt 注入约定、写 NEEDS_INPUT→终态
  needs_input+question)。真因查很久:emit_text 不转义、多行 NEEDS_INPUT 的裸 \n 破坏 NDJSON、
  runner 整条丢弃。修:json_escape。实测 worker 请示「排序稳定还是性能?」被捕获。
- **双向协作可见** — 追溯室「⚠ worker 请示中」+ 展开看请示内容(TaskRecord.question 纵切上 wire)。
- **★ real 验证 + escalate 闭环** — 开 real:真 claude 经理两次都智能 escalate 缺信息任务(上游
  边界判断,比 worker 请示更聪明)。暴露真 bug:Effect::Escalate 空壳→任务卡 queued+反复升级。
  修:claim 队首标 escalated 挂起、原因存 question、晨报「等你处理」、追溯室「⚠ 经理升级·等你」。

### 对照地图:§5 协作 ✓✓✓ 三向闭环(经理→worker / worker→经理 / 经理→CEO)+ 全可见
### 连续 real 真跑(75-90):交付/拆活/记忆/自愈/main永不坏/空转/过夜/冲突/.quiver/崩溃恢复/
### 预算硬闸/经理智能 escalate —— 正常+异常+工作流+协作+边界 全真 claude 验证,11 个真 bug 修掉。
