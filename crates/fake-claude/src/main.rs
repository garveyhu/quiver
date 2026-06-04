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
    let delay = resolve_delay(std::env::var("QUIVER_FAKE_DELAY_MS").ok());

    match scenario.as_str() {
        // Permanently-failed run (DESIGN §8.4): emit the init line so a worker
        // appears, then die non-zero with NO clean `result` line. Exercises the
        // deterministic crash-cleanup path in the supervisor.
        "crash" => {
            emit_init(delay);
            return ExitCode::FAILURE;
        }
        // Reserved for later phases (DESIGN §13.1). Fall back to happy for now.
        "verify_fail" | "credit_exhausted" => emit_happy(&prompt, delay),
        // Default scripted success path.
        _ => emit_happy(&prompt, delay),
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

/// Print the `system / init` line — carries session_id + model. Shared by the
/// happy and crash scripts so both produce a `WorkerStarted` event.
fn emit_init(delay: Duration) {
    emit_line(
        &format!(
            r#"{{"type":"system","subtype":"init","session_id":"{SESSION_ID}","model":"{MODEL}","cwd":"/tmp/fake-worktree","tools":["Edit","Read","Bash"],"permissionMode":"acceptEdits","apiKeySource":"none"}}"#
        ),
        delay,
    );
}

/// Print the happy-path `stream-json` sequence:
/// init → tool_use → tool_result → assistant text → result/success.
///
/// `prompt` is the user's task text; it is echoed back in the assistant text and
/// the final `result` so the free demo visibly reflects what the user typed.
/// Each line is paced by `delay` so the office animates live.
fn emit_happy(prompt: &str, delay: Duration) {
    const TOOL_USE_ID: &str = "toolu_fake_0001";
    let echo = json_escape(prompt);
    // The text the agent "says" back — quotes the user's prompt verbatim.
    let reply = format!("Simulated run for your task: \\\"{echo}\\\". (No real agent ran.)");

    // 1. system / init line — carries session_id + model.
    emit_init(delay);

    // 2. assistant line containing a tool_use (Edit).
    emit_line(
        &format!(
            r#"{{"type":"assistant","session_id":"{SESSION_ID}","message":{{"id":"msg_fake_0001","role":"assistant","model":"{MODEL}","content":[{{"type":"tool_use","id":"{TOOL_USE_ID}","name":"Edit","input":{{"file_path":"src/main.rs","old_string":"foo","new_string":"bar"}}}}]}}}}"#
        ),
        delay,
    );

    // 3. user line containing the matching tool_result.
    emit_line(
        &format!(
            r#"{{"type":"user","session_id":"{SESSION_ID}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{TOOL_USE_ID}","is_error":false,"content":"The file src/main.rs has been edited."}}]}}}}"#
        ),
        delay,
    );

    // 4. assistant line with text — echoes the user's prompt.
    emit_line(
        &format!(
            r#"{{"type":"assistant","session_id":"{SESSION_ID}","message":{{"id":"msg_fake_0002","role":"assistant","model":"{MODEL}","content":[{{"type":"text","text":"{reply}"}}]}}}}"#
        ),
        delay,
    );

    // 5. result / success line — total_cost_usd, num_turns, usage. Its `result`
    // text also echoes the prompt.
    emit_line(
        &format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"session_id":"{SESSION_ID}","total_cost_usd":0.01,"num_turns":2,"duration_ms":1234,"duration_api_ms":1000,"result":"{reply}","usage":{{"input_tokens":120,"output_tokens":45,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}"#
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
