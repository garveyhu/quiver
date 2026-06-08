//! Parse one Claude `stream-json` NDJSON line into a lightweight [`RawLine`].
//!
//! This is the line-level reader of the Claude adapter (DESIGN §5.3). It extracts
//! ONLY the fields Quiver consumes in the Phase-0 subset and drops everything else.
//! Lines that carry no signal we map yet (e.g. `user`/`tool_result`, permission
//! prompts, rate-limit events) parse to `None`. Normalization into the wire
//! `AgentEvent` happens one layer up, in `adapter` (Task 0.5).

use serde_json::Value;

/// The minimal projection of a Claude `stream-json` line.
///
/// Each variant keeps only the fields the adapter needs; the raw wire object is
/// otherwise discarded. `ToolUse.input_summary` is the *raw* serialized tool
/// input — the human one-liner (`summarize`) is the adapter's job, not the
/// parser's.
#[derive(Clone, Debug, PartialEq)]
pub enum RawLine {
    Init {
        session_id: Option<String>,
        model: Option<String>,
    },
    AssistantText {
        text: String,
    },
    ToolUse {
        name: String,
        input_summary: String,
    },
    ResultOk {
        total_cost_usd: Option<f64>,
        num_turns: u32,
        tokens: Option<u64>,
        duration_ms: Option<u64>,
    },
    ResultErr {
        message: String,
    },
}

/// Parse a single NDJSON line into a [`RawLine`], or `None` if the line is blank,
/// not valid JSON, or a kind we have no Phase-0 mapping for.
pub fn parse_line(line: &str) -> Option<RawLine> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;

    match v.get("type").and_then(Value::as_str)? {
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => Some(RawLine::Init {
            session_id: str_field(&v, "session_id"),
            model: str_field(&v, "model"),
        }),
        "assistant" => parse_assistant(&v),
        "result" => Some(parse_result(&v)),
        _ => None,
    }
}

/// An `assistant` line carries a `message.content` array; we take the first item
/// we recognize — a `text` block → `AssistantText`, a `tool_use` block →
/// `ToolUse`. Anything else → `None`.
fn parse_assistant(v: &Value) -> Option<RawLine> {
    let content = v.get("message")?.get("content")?.as_array()?;
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = block.get("text").and_then(Value::as_str)?.to_string();
                return Some(RawLine::AssistantText { text });
            }
            Some("tool_use") => {
                let name = block.get("name").and_then(Value::as_str)?.to_string();
                let input_summary = block
                    .get("input")
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                return Some(RawLine::ToolUse {
                    name,
                    input_summary,
                });
            }
            _ => continue,
        }
    }
    None
}

/// A `result` line is terminal accounting. `is_error:false` (or absent) → `ResultOk`
/// with `total_cost_usd` / `num_turns`; `is_error:true` → `ResultErr` with a message
/// drawn from `result`/`error`/`subtype` (best-effort).
fn parse_result(v: &Value) -> RawLine {
    let is_error = v.get("is_error").and_then(Value::as_bool).unwrap_or(false);
    if is_error {
        let message = str_field(v, "result")
            .or_else(|| str_field(v, "error"))
            .or_else(|| str_field(v, "subtype"))
            .unwrap_or_else(|| "agent error".to_string());
        RawLine::ResultErr { message }
    } else {
        let tokens = v.get("usage").and_then(|u| {
            let input = u.get("input_tokens").and_then(Value::as_u64);
            let output = u.get("output_tokens").and_then(Value::as_u64);
            match (input, output) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            }
        });
        RawLine::ResultOk {
            total_cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
            num_turns: v
                .get("num_turns")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
            tokens,
            duration_ms: v.get("duration_ms").and_then(Value::as_u64),
        }
    }
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The captured `fake-claude` happy-path fixture — the parser's test corpus.
    const FIXTURE: &str = include_str!("../../../tests/fixtures/fake_claude_happy.jsonl");

    fn fixture_line(idx: usize) -> &'static str {
        FIXTURE.lines().nth(idx).expect("fixture line exists")
    }

    #[test]
    fn parses_init_line() {
        let line = fixture_line(0);
        assert_eq!(
            parse_line(line),
            Some(RawLine::Init {
                session_id: Some("fake-session-0001".into()),
                model: Some("claude-sonnet-4-5".into()),
            })
        );
    }

    #[test]
    fn parses_tool_use_line() {
        let line = fixture_line(1);
        match parse_line(line) {
            Some(RawLine::ToolUse {
                name,
                input_summary,
            }) => {
                assert_eq!(name, "Edit");
                assert!(input_summary.contains("src/main.rs"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_user_line_maps_to_none() {
        let line = fixture_line(2);
        assert_eq!(parse_line(line), None);
    }

    #[test]
    fn parses_assistant_text_line() {
        let line = fixture_line(3);
        assert_eq!(
            parse_line(line),
            Some(RawLine::AssistantText {
                text: "Done. I edited src/main.rs as requested.".into(),
            })
        );
    }

    #[test]
    fn parses_result_success_line() {
        let line = fixture_line(4);
        assert_eq!(
            parse_line(line),
            Some(RawLine::ResultOk {
                total_cost_usd: Some(0.01),
                num_turns: 2,
                tokens: Some(165),
                duration_ms: Some(1234),
            })
        );
    }

    #[test]
    fn blank_and_garbage_lines_map_to_none() {
        assert_eq!(parse_line(""), None);
        assert_eq!(parse_line("   "), None);
        assert_eq!(parse_line("not json"), None);
        assert_eq!(parse_line(r#"{"type":"unknown_kind"}"#), None);
    }

    #[test]
    fn parses_result_error_line() {
        let line = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"boom"}"#;
        assert_eq!(
            parse_line(line),
            Some(RawLine::ResultErr {
                message: "boom".into(),
            })
        );
    }
}
