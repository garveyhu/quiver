//! `fake-claude` — a scripted test double for the official `claude` CLI.
//!
//! It accepts arbitrary CLI args (the same flags the real binary takes, e.g.
//! `-p`, `--output-format stream-json`, `--verbose`, `--model`) and prints a
//! deterministic `stream-json` sequence to stdout: ONE JSON object per line
//! (NDJSON), matching the real Claude wire shape per DESIGN §5.3 as closely as
//! is reasonable. This lets the core loop be exercised for free and
//! deterministically in CI (DESIGN §13.1).
//!
//! A `--scenario <name>` flag selects the script. `happy` (default) and `crash`
//! are implemented; `verify_fail` / `credit_exhausted` are reserved for later
//! phases and currently fall back to `happy`.
//!
//! ## Live pacing
//!
//! To make `simulate` mode show a believable LIVE process (the office character
//! visibly progressing through poses) rather than an instant dump, each NDJSON
//! line is emitted with a small delay and an explicit stdout flush. The delay is
//! controlled by `QUIVER_FAKE_DELAY_MS` (default `DEFAULT_DELAY_MS`); set it to
//! `0` for no delay so tests stay fast.

use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

/// Default per-line pacing delay (ms) when `QUIVER_FAKE_DELAY_MS` is unset. Tuned
/// so a happy run takes a few seconds — long enough for the office to animate
/// through poses, short enough not to feel stuck.
const DEFAULT_DELAY_MS: u64 = 700;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let scenario = parse_scenario(args.iter().cloned());
    // Echo the received prompt (the `-p <prompt>` the runner passes) so the free
    // demo visibly reflects what the user typed instead of replaying a fixed
    // script that ignores input.
    let prompt = parse_prompt(args.iter().cloned());
    // When the runner passes `--resume <id>`, echo THAT session id throughout the
    // stream (a resumed run continues an existing session) so a test can assert the
    // handle was threaded through. Absent → the default fresh session.
    let session = parse_resume(args.iter().cloned()).unwrap_or_else(|| SESSION_ID.to_string());
    // Test-only: where the `spawn_child` scenario records its forked grandchild's
    // PID (passed as an extra arg by the killpg chaos test). Ignored otherwise.
    let child_pidfile = parse_flag(args.iter().cloned(), "--child-pidfile");
    let delay = resolve_delay(std::env::var("QUIVER_FAKE_DELAY_MS").ok());

    // 经理决策调用(ClaudeBrain build_prompt)→ 输出一个决策 JSON,不走工作弧。这让 simulate
    // 模式也能免费跑通整条 claude 经理链路(spawn 进程→收集→parse→执行),验证开真 claude 前
    // 管道无断点。决策逻辑同 RuleBrain「先裁后派」,只是走 ClaudeBrain 路径。
    if prompt.contains("自治 AI 研发公司的经理") || prompt.contains("只输出一个 JSON 对象") {
        emit_manager_decision(&session, &prompt, delay);
        return ExitCode::SUCCESS;
    }

    // 记忆官提炼调用(librarian distill)→ 输出一个 JSON 数组,不走工作弧。simulate 下记忆官也能
    // 免费跑通提炼链路(spawn→收集→parse→写入)。桩保守输出 []:绝不瞎造假事实污染记忆库 ——
    // 真模式提炼才由真 claude 从工作记录提真模式(无价值的流水它也会输出 [])。
    if prompt.contains("项目记忆库的记忆官") {
        emit_text(&session, "dis", "[]", delay);
        return ExitCode::SUCCESS;
    }

    // §5 双向协作验证(worker→经理):任务含「[请示验证]」且**首轮**(没收到 --resume)→ 跑一段
    // 后在产出末尾写 NEEDS_INPUT 模拟卡住请示;经理回答(continue)后 --resume 进来则正常完成。
    // 让 simulate 免费看到 worker→经理 的求助闭环。
    if prompt.contains("[请示验证]") && parse_resume(args.iter().cloned()).is_none() {
        // 请示在 result **之前** emit(worker 工作中卡住请示)—— result 之后的输出 runner 可能
        // 已不收(result 是终止信号)。真 claude 也是在产出过程里写 NEEDS_INPUT、然后才收尾。
        emit_init(&session, delay);
        // 文本含换行(NEEDS_INPUT 要单独成行)→ **必须 json_escape**,否则裸 \n 破坏 NDJSON、
        // runner 整条丢弃(emit_text 不自带转义,调用方负责)。这是 worker 请示信号丢失的真因。
        emit_text(
            &session,
            "ask",
            &json_escape("实现到一半遇到一个取舍拿不准。\nNEEDS_INPUT: 排序要稳定排序还是性能优先?"),
            delay,
        );
        emit_result_ok(&session, delay);
        return ExitCode::SUCCESS;
    }

    match scenario.as_str() {
        // Permanently-failed run (DESIGN §8.4): emit the init line so a worker
        // appears, then die non-zero with NO clean `result` line. Exercises the
        // deterministic crash-cleanup path in the supervisor.
        "crash" => {
            emit_init(&session, delay);
            return ExitCode::FAILURE;
        }
        // Test hook for the killpg chaos test (DESIGN §23 P3): fork a long-lived
        // grandchild that records its PID, emit init so a worker appears, then stay
        // alive so the run is genuinely in-flight when the test kills the process
        // GROUP. The grandchild inherits this process group, so a correct killpg
        // reaps it too — leaving no orphan.
        "spawn_child" => {
            spawn_orphan_probe(child_pidfile.as_deref());
            emit_init(&session, delay);
            std::thread::sleep(Duration::from_secs(30));
            return ExitCode::SUCCESS;
        }
        // Reserved for later phases (DESIGN §13.1). Fall back to happy for now.
        "verify_fail" | "credit_exhausted" => emit_happy(&session, &prompt, delay),
        // Default scripted success path.
        _ => emit_happy(&session, &prompt, delay),
    }

    ExitCode::SUCCESS
}

/// Emit one NDJSON line, flush stdout so the consumer sees it immediately, then
/// pace by `delay` so the live event stream is spread over wall-clock time. A
/// zero delay (set by tests) skips the sleep entirely.
fn emit_line(line: &str, delay: Duration) {
    let mut stdout = std::io::stdout();
    // A closed pipe (consumer dropped) is not an error worth aborting on — the
    // supervisor stops reading when it has what it needs.
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
}

/// Resolve the per-line pacing delay from an optional `QUIVER_FAKE_DELAY_MS`
/// value. Unset → [`DEFAULT_DELAY_MS`]; a parseable number → that many ms
/// (`0` = no delay); an unparseable value → the default (fail-soft).
fn resolve_delay(raw: Option<String>) -> Duration {
    let ms = raw
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_DELAY_MS);
    Duration::from_millis(ms)
}

/// Extract the value of `--scenario <name>` (or `--scenario=name`) from the
/// args, defaulting to `happy`. All other args are accepted and ignored.
fn parse_scenario(args: impl Iterator<Item = String>) -> String {
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--scenario=") {
            return value.to_string();
        }
        if arg == "--scenario" {
            if let Some(value) = args.next() {
                return value;
            }
        }
    }
    "happy".to_string()
}

/// Extract the value of `-p <prompt>` (or `--print <prompt>`) the runner passes —
/// this is the user's task text. Defaults to a placeholder when absent so the
/// scripted lines always render. All other args are accepted and ignored.
fn parse_prompt(args: impl Iterator<Item = String>) -> String {
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        if arg == "-p" || arg == "--print" {
            if let Some(value) = args.next() {
                return value;
            }
        }
    }
    "(no prompt provided)".to_string()
}

/// Extract the value of `--<name> <value>` (or `--<name>=<value>`) from the args.
/// Generic flag reader for the test-injection flags the runner forwards verbatim.
fn parse_flag(args: impl Iterator<Item = String>, name: &str) -> Option<String> {
    let eq = format!("{name}=");
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix(&eq) {
            return Some(value.to_string());
        }
        if arg == name {
            return args.next();
        }
    }
    None
}

/// Extract the value of `--resume <session_id>` (or `--resume=<id>`) the runner
/// passes to continue a prior session. `None` when absent (a fresh spawn). All
/// other args are accepted and ignored.
fn parse_resume(args: impl Iterator<Item = String>) -> Option<String> {
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--resume=") {
            return Some(value.to_string());
        }
        if arg == "--resume" {
            return args.next();
        }
    }
    None
}

/// JSON-escape a string for safe interpolation into the hand-written `stream-json`
/// lines below (the prompt is user input — it may contain quotes, backslashes,
/// newlines, or control chars that would otherwise produce invalid NDJSON).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

const SESSION_ID: &str = "fake-session-0001";
const MODEL: &str = "claude-sonnet-4-5";

/// Test hook for the killpg chaos test: fork a long-lived grandchild that writes
/// its own PID to `pidfile`, then sleeps well past the test's lifetime. The
/// grandchild inherits this process's group, so the test can verify that killing
/// the GROUP (not just the agent pid) reaps it too — no orphan. No-op when no
/// `--child-pidfile` was passed (i.e. a normal run).
fn spawn_orphan_probe(pidfile: Option<&str>) {
    let Some(pidfile) = pidfile else {
        return;
    };
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("echo $$ > '{pidfile}'; sleep 30"))
        .spawn();
}

/// Print the `system / init` line — carries session_id + model. Shared by the
/// happy and crash scripts so both produce a `WorkerStarted` event.
fn emit_init(session: &str, delay: Duration) {
    emit_line(
        &format!(
            r#"{{"type":"system","subtype":"init","session_id":"{session}","model":"{MODEL}","cwd":"/tmp/fake-worktree","tools":["Edit","Read","Bash"],"permissionMode":"acceptEdits","apiKeySource":"none"}}"#
        ),
        delay,
    );
}

/// An extra "thinking" pause between work phases, so the simulated worker
/// visibly dwells on each phase (read → search → edit → test) instead of
/// strobing through. The pause unit has a floor (250ms) independent of the
/// per-line delay — a small `fakeDelayMs` setting compresses line pacing but a
/// run should still feel like real work (several seconds), not a blink. A zero
/// base delay (tests) skips entirely so CI stays fast.
fn think(delay: Duration, times: u32) {
    if delay.is_zero() {
        return;
    }
    let unit = delay.max(Duration::from_millis(250));
    std::thread::sleep(unit * times);
}

/// Emit one assistant tool_use line + its matching user tool_result line.
fn emit_tool(session: &str, id: &str, name: &str, input: &str, result: &str, delay: Duration) {
    emit_line(
        &format!(
            r#"{{"type":"assistant","session_id":"{session}","message":{{"id":"msg_{id}","role":"assistant","model":"{MODEL}","content":[{{"type":"tool_use","id":"toolu_{id}","name":"{name}","input":{input}}}]}}}}"#
        ),
        delay,
    );
    emit_line(
        &format!(
            r#"{{"type":"user","session_id":"{session}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"toolu_{id}","is_error":false,"content":"{result}"}}]}}}}"#
        ),
        delay,
    );
}

/// Emit one assistant text line.
fn emit_text(session: &str, id: &str, text: &str, delay: Duration) {
    emit_line(
        &format!(
            r#"{{"type":"assistant","session_id":"{session}","message":{{"id":"msg_{id}","role":"assistant","model":"{MODEL}","content":[{{"type":"text","text":"{text}"}}]}}}}"#
        ),
        delay,
    );
}

/// 经理决策桩:从决策 prompt 的局面里推出一个合法 [`Decision`] JSON,作为 assistant text 输出
/// (ClaudeBrain 收集 → parse)。逻辑同 RuleBrain「先裁后派」:有完工待复核 → 交付那个节点;
/// 否则有队首任务 → 派活;都没有 → 按兵不动。决策 JSON 经 [`json_escape`] 嵌进 text 值。
fn emit_manager_decision(session: &str, prompt: &str, delay: Duration) {
    let decision = decide_from_prompt(prompt);
    emit_text(session, "dec", &json_escape(&decision), delay);
}

/// 从决策 prompt 推一个决策 JSON 字符串(桩,验证 ClaudeBrain 链路用)。
fn decide_from_prompt(prompt: &str) -> String {
    // 先裁后派:完工待复核的优先裁决。§5 双向协作演示:首轮(prompt 里没"已续跑")给一轮具体
    // 指导让它再跑(continue);已续跑过 → 收尾交付(deliver)。真 claude 会按产出质量智能判断,
    // 这里桩固定"指导一轮再交付",好让 simulate 也能看到 spawn→continue→deliver 的双向闭环。
    if prompt.contains("完工待你复核") {
        if let Some(node) = extract_node_id(prompt) {
            if prompt.contains("已续跑") {
                return format!(r#"{{"action":"deliver","node_id":"{node}"}}"#);
            }
            return format!(
                r#"{{"action":"continue","node_id":"{node}","prompt":"产出方向对,请再完善边界处理和中文注释"}}"#
            );
        }
    }
    // 否则看队首任务:列举多个子目标(用「、」分隔)→ 拆活分工(§5 协作);单个 → 原样派活。
    if let Some(task) = extract_next_task(prompt) {
        // 把 CEO 录入的权威事实(记忆)注入派给员工的活 —— 单任务、拆出的子任务都带上公司约定
        // ("记忆 → 决策 → 执行",simulate 也看得见;真 claude 会智能选最相关事实,这里桩选第一条)。
        let memo = extract_authoritative_fact(prompt);
        let inject = |s: &str| match &memo {
            Some(m) => format!("{s}(公司约定:{m})"),
            None => s.to_string(),
        };
        // 用了记忆 → 决策理由也标出来,CEO 在决策流就能看到"经理在遵循公司约定"。
        let reason = memo
            .as_ref()
            .map(|m| format!(r#","reason":"遵循公司约定:{}""#, json_escape(m)))
            .unwrap_or_default();
        let parts: Vec<String> = task
            .split(['、', '，'])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if parts.len() > 1 {
            let arr = parts
                .iter()
                .map(|p| format!("\"{}\"", json_escape(&inject(p))))
                .collect::<Vec<_>>()
                .join(",");
            return format!(r#"{{"action":"plan","subtasks":[{arr}]{reason}}}"#);
        }
        return format!(r#"{{"action":"spawn","prompt":"{}"{reason}}}"#, json_escape(&inject(&task)));
    }
    r#"{"action":"noop"}"#.to_string()
}

/// 从决策 prompt 的记忆简报里抽第一条**权威**事实(CEO 录入的公司约定):brief 格式
/// "- [权威|重要度N] 文本"。没有则 None。
fn extract_authoritative_fact(prompt: &str) -> Option<String> {
    for line in prompt.lines() {
        if line.contains("[权威") {
            if let Some(idx) = line.find("] ") {
                let text = line[idx + "] ".len()..].trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
    }
    None
}

/// 从 "节点 {id} 完工" 抽出 node_id(到下一个空格/逗号止)。
fn extract_node_id(prompt: &str) -> Option<String> {
    let i = prompt.find("节点 ")? + "节点 ".len();
    let rest = &prompt[i..];
    let end = rest
        .char_indices()
        .find(|&(_, c)| c == ' ' || c == ',' || c == '，' || c == '完')
        .map(|(idx, _)| idx)
        .unwrap_or(rest.len());
    let id = rest[..end].trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// 从 "队列下一个任务(原文):「{t}」" 抽出任务原文。
fn extract_next_task(prompt: &str) -> Option<String> {
    let anchor = prompt.find("队列下一个任务")?;
    let after = &prompt[anchor..];
    let start = after.find('「')? + '「'.len_utf8();
    let rest = &after[start..];
    let end = rest.find('」')?;
    Some(rest[..end].to_string())
}

/// Print the happy-path `stream-json` sequence — a believable work arc rather
/// than an instant 5-line dump: init → 读相关代码(Read/Grep) → 中途说明 → 改代码
/// (Edit) → 跑测试(Bash) → 汇报 → result. Phase pauses (`think`) make the office
/// character visibly dwell on each step, so a simulate run feels like real work
/// (several seconds at the default pacing) instead of a toy that blinks.
///
/// `prompt` is the user's task text; it is echoed in the narration and the final
/// `result` so the free demo visibly reflects what the user typed.
fn emit_happy(session: &str, prompt: &str, delay: Duration) {
    let echo = json_escape(prompt);
    let reply = format!("Simulated run for your task: \\\"{echo}\\\". (No real agent ran.)");

    // 1. init — the worker clocks in.
    emit_init(session, delay);
    think(delay, 3);

    // 2. orient: read the entry point, then search for the relevant code.
    emit_tool(
        session, "fake_0001", "Read",
        r#"{"file_path":"src/main.rs"}"#,
        "src/main.rs: 212 lines.",
        delay,
    );
    think(delay, 4);
    emit_tool(
        session, "fake_0002", "Grep",
        r#"{"pattern":"handle_request","path":"src/"}"#,
        "3 matches in src/server.rs, src/routes.rs.",
        delay,
    );
    think(delay, 3);

    // 3. narrate the plan mid-flight — quotes the task so it's visibly yours.
    emit_text(
        session, "fake_0003",
        &format!("正在处理:\\\"{echo}\\\" — 已定位相关代码,开始修改。"),
        delay,
    );
    think(delay, 4);

    // 4. do the work: two edits, then run the test suite.
    emit_tool(
        session, "fake_0004", "Edit",
        r#"{"file_path":"src/server.rs","old_string":"foo","new_string":"bar"}"#,
        "The file src/server.rs has been edited.",
        delay,
    );
    think(delay, 3);
    emit_tool(
        session, "fake_0005", "Edit",
        r#"{"file_path":"src/routes.rs","old_string":"baz","new_string":"qux"}"#,
        "The file src/routes.rs has been edited.",
        delay,
    );
    think(delay, 4);
    emit_tool(
        session, "fake_0006", "Bash",
        r#"{"command":"cargo test"}"#,
        "test result: ok. 42 passed; 0 failed.",
        delay,
    );
    think(delay, 5);

    // 5. report + result/success (cost, turns, usage).
    emit_text(session, "fake_0007", &reply, delay);
    emit_result_ok(session, delay);
}

/// 发一条 success result 行(worker 收尾)。双向协作请示场景:worker 先 emit NEEDS_INPUT(在
/// result 之前,否则 runner 收不到),再用它收尾这一轮。
fn emit_result_ok(session: &str, delay: Duration) {
    let reply = "Simulated run. (No real agent ran.)";
    emit_line(
        &format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"session_id":"{session}","total_cost_usd":0.01,"num_turns":3,"duration_ms":3000,"duration_api_ms":2500,"result":"{reply}","usage":{{"input_tokens":300,"output_tokens":80,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}"#
        ),
        delay,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prompt_reads_dash_p_value() {
        let args = ["-p", "do the thing", "--verbose"].map(String::from);
        assert_eq!(parse_prompt(args.into_iter()), "do the thing");
    }

    #[test]
    fn parse_prompt_reads_long_print_flag() {
        let args = ["--print", "hello"].map(String::from);
        assert_eq!(parse_prompt(args.into_iter()), "hello");
    }

    #[test]
    fn parse_prompt_defaults_when_absent() {
        let args = ["--verbose"].map(String::from);
        assert_eq!(parse_prompt(args.into_iter()), "(no prompt provided)");
    }

    #[test]
    fn parse_resume_reads_value_and_eq_form() {
        let spaced = ["-p", "x", "--resume", "sess-9"].map(String::from);
        assert_eq!(parse_resume(spaced.into_iter()), Some("sess-9".to_string()));
        let eq = ["--resume=sess-eq", "--verbose"].map(String::from);
        assert_eq!(parse_resume(eq.into_iter()), Some("sess-eq".to_string()));
    }

    #[test]
    fn parse_resume_none_when_absent() {
        let args = ["-p", "x", "--verbose"].map(String::from);
        assert_eq!(parse_resume(args.into_iter()), None);
    }

    #[test]
    fn json_escape_handles_quotes_backslashes_and_controls() {
        assert_eq!(json_escape(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(json_escape("line1\nline2\t!"), "line1\\nline2\\t!");
        assert_eq!(json_escape("\u{0001}"), "\\u0001");
    }

    #[test]
    fn resolve_delay_defaults_when_unset() {
        assert_eq!(resolve_delay(None), Duration::from_millis(DEFAULT_DELAY_MS));
    }

    #[test]
    fn resolve_delay_zero_disables_pacing() {
        assert_eq!(resolve_delay(Some("0".to_string())), Duration::ZERO);
    }

    #[test]
    fn resolve_delay_parses_explicit_value() {
        assert_eq!(
            resolve_delay(Some("1200".to_string())),
            Duration::from_millis(1200)
        );
    }

    #[test]
    fn resolve_delay_falls_back_on_garbage() {
        assert_eq!(
            resolve_delay(Some("not-a-number".to_string())),
            Duration::from_millis(DEFAULT_DELAY_MS)
        );
    }
}
