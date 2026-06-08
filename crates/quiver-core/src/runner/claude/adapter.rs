//! Normalize parsed Claude lines ([`RawLine`]) into wire [`AgentEvent`]s
//! (DESIGN §5.3). This is the one place backend-specific signals become the
//! runner-agnostic event shape the supervisor/UI consume (DESIGN §5.1).
//!
//! Phase-0 subset only: line kinds we have no variant for yet (`tool_result`,
//! permission prompts, rate-limit events) never reach here — the parser already
//! dropped them to `None`.

use serde_json::Value;

use crate::event::{AgentEvent, AgentEventPayload, AuthMode, RunnerKind};
use crate::runner::claude::parse::RawLine;

/// Max length of a `Bash` command summary before truncation (DESIGN §5.3:
/// "first ~40 chars of command").
const BASH_SUMMARY_MAX: usize = 40;

/// Per-task adapter: stamps `runner: ClaudeCli`, assigns a gap-free monotonic
/// `seq`, and maps each [`RawLine`] to one [`AgentEvent`].
///
/// One instance per task (= worktree = process = sprite), so `seq` is the
/// per-task ordering key for the §11 append-log replay.
pub struct ClaudeAdapter {
    task_id: String,
    next_seq: u64,
}

impl ClaudeAdapter {
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            next_seq: 0,
        }
    }

    /// Map one parsed line to a normalized event, advancing `seq`. `ts_ms` is the
    /// supervisor wall-clock at emit time (caller-supplied so the adapter stays
    /// clock-free and deterministic in tests).
    pub fn adapt(&mut self, raw: RawLine, ts_ms: i64) -> AgentEvent {
        let payload = match raw {
            RawLine::Init { session_id, model } => AgentEventPayload::WorkerStarted {
                session_id,
                model,
                auth_mode: AuthMode::Subscription,
            },
            RawLine::AssistantText { text } => AgentEventPayload::OutputChunk { text },
            RawLine::ToolUse {
                name,
                input_summary,
            } => {
                let summary = summarize(&name, &input_summary);
                AgentEventPayload::ToolUse {
                    tool: name,
                    summary,
                }
            }
            RawLine::ResultOk {
                total_cost_usd,
                num_turns,
                tokens,
                duration_ms,
            } => AgentEventPayload::Result {
                ok: true,
                cost_usd: total_cost_usd,
                num_turns,
                tokens,
                duration_ms,
            },
            RawLine::ResultErr { message } => AgentEventPayload::Error {
                code: "agent_error".to_string(),
                message,
            },
        };

        let seq = self.next_seq;
        self.next_seq += 1;
        AgentEvent {
            task_id: self.task_id.clone(),
            seq,
            ts_ms,
            runner: RunnerKind::ClaudeCli,
            payload,
        }
    }
}

/// Collapse a tool call to a one-line human summary (DESIGN §5.3):
/// `Edit → "edit " + basename(file_path)`, `Bash → first ~40 chars of command`,
/// anything else → the tool name. `input` is the raw serialized JSON the parser
/// captured; on any shape mismatch we fall back to the tool name.
pub fn summarize(tool: &str, input: &str) -> String {
    let value: Option<Value> = serde_json::from_str(input).ok();
    match tool {
        "Edit" => value
            .as_ref()
            .and_then(|v| v.get("file_path"))
            .and_then(Value::as_str)
            .map(|path| format!("edit {}", basename(path)))
            .unwrap_or_else(|| tool.to_string()),
        "Bash" => value
            .as_ref()
            .and_then(|v| v.get("command"))
            .and_then(Value::as_str)
            .map(truncate_command)
            .unwrap_or_else(|| tool.to_string()),
        other => other.to_string(),
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn truncate_command(cmd: &str) -> String {
    if cmd.chars().count() <= BASH_SUMMARY_MAX {
        cmd.to_string()
    } else {
        let truncated: String = cmd.chars().take(BASH_SUMMARY_MAX).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::claude::parse::parse_line;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/fake_claude_happy.jsonl");

    fn raw(idx: usize) -> RawLine {
        let line = FIXTURE.lines().nth(idx).expect("fixture line exists");
        parse_line(line).expect("fixture line parses")
    }

    #[test]
    fn init_maps_to_worker_started_subscription() {
        let mut ad = ClaudeAdapter::new("t1");
        let ev = ad.adapt(raw(0), 100);
        assert_eq!(ev.task_id, "t1");
        assert_eq!(ev.seq, 0);
        assert!(matches!(ev.runner, RunnerKind::ClaudeCli));
        match ev.payload {
            AgentEventPayload::WorkerStarted {
                session_id,
                model,
                auth_mode,
            } => {
                assert_eq!(session_id.as_deref(), Some("fake-session-0001"));
                assert_eq!(model.as_deref(), Some("claude-sonnet-4-5"));
                assert!(matches!(auth_mode, AuthMode::Subscription));
            }
            other => panic!("expected WorkerStarted, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_edit_summarizes_basename() {
        let mut ad = ClaudeAdapter::new("t1");
        let ev = ad.adapt(raw(1), 0);
        match ev.payload {
            AgentEventPayload::ToolUse { tool, summary } => {
                assert_eq!(tool, "Edit");
                assert_eq!(summary, "edit main.rs");
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn assistant_text_maps_to_output_chunk() {
        let mut ad = ClaudeAdapter::new("t1");
        let ev = ad.adapt(raw(3), 0);
        match ev.payload {
            AgentEventPayload::OutputChunk { text } => {
                assert_eq!(text, "Done. I edited src/main.rs as requested.");
            }
            other => panic!("expected OutputChunk, got {other:?}"),
        }
    }

    #[test]
    fn result_ok_maps_to_result() {
        let mut ad = ClaudeAdapter::new("t1");
        let ev = ad.adapt(raw(4), 0);
        match ev.payload {
            AgentEventPayload::Result {
                ok,
                cost_usd,
                num_turns,
                tokens,
                duration_ms,
            } => {
                assert!(ok);
                assert_eq!(cost_usd, Some(0.01));
                assert_eq!(num_turns, 2);
                assert_eq!(tokens, Some(165));
                assert_eq!(duration_ms, Some(1234));
            }
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn result_err_maps_to_error_agent_error() {
        let mut ad = ClaudeAdapter::new("t1");
        let ev = ad.adapt(
            RawLine::ResultErr {
                message: "boom".into(),
            },
            0,
        );
        match ev.payload {
            AgentEventPayload::Error { code, message } => {
                assert_eq!(code, "agent_error");
                assert_eq!(message, "boom");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn seq_is_per_adapter_monotonic() {
        let mut ad = ClaudeAdapter::new("t1");
        let seqs: Vec<u64> = (0..3).map(|_| ad.adapt(raw(3), 0).seq).collect();
        assert_eq!(seqs, vec![0, 1, 2]);
    }

    #[test]
    fn summarize_bash_truncates_long_command() {
        let long = r#"{"command":"echo aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa done"}"#;
        let s = summarize("Bash", long);
        // 40 chars + ellipsis
        assert_eq!(s.chars().count(), BASH_SUMMARY_MAX + 1);
        assert!(s.starts_with("echo aaaa"));
        assert!(s.ends_with('…'));
    }

    #[test]
    fn summarize_bash_keeps_short_command() {
        let short = r#"{"command":"cargo test"}"#;
        assert_eq!(summarize("Bash", short), "cargo test");
    }

    #[test]
    fn summarize_unknown_tool_is_tool_name() {
        assert_eq!(summarize("Read", r#"{"file_path":"a.rs"}"#), "Read");
    }
}
