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

use std::process::ExitCode;

fn main() -> ExitCode {
    let scenario = parse_scenario(std::env::args().skip(1));

    match scenario.as_str() {
        // Permanently-failed run (DESIGN §8.4): emit the init line so a worker
        // appears, then die non-zero with NO clean `result` line. Exercises the
        // deterministic crash-cleanup path in the supervisor.
        "crash" => {
            emit_init();
            return ExitCode::FAILURE;
        }
        // Reserved for later phases (DESIGN §13.1). Fall back to happy for now.
        "verify_fail" | "credit_exhausted" => emit_happy(),
        // Default scripted success path.
        _ => emit_happy(),
    }

    ExitCode::SUCCESS
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

const SESSION_ID: &str = "fake-session-0001";
const MODEL: &str = "claude-sonnet-4-5";

/// Print the `system / init` line — carries session_id + model. Shared by the
/// happy and crash scripts so both produce a `WorkerStarted` event.
fn emit_init() {
    println!(
        r#"{{"type":"system","subtype":"init","session_id":"{SESSION_ID}","model":"{MODEL}","cwd":"/tmp/fake-worktree","tools":["Edit","Read","Bash"],"permissionMode":"acceptEdits","apiKeySource":"none"}}"#
    );
}

/// Print the happy-path `stream-json` sequence:
/// init → tool_use → tool_result → assistant text → result/success.
fn emit_happy() {
    const TOOL_USE_ID: &str = "toolu_fake_0001";

    // 1. system / init line — carries session_id + model.
    emit_init();

    // 2. assistant line containing a tool_use (Edit).
    println!(
        r#"{{"type":"assistant","session_id":"{SESSION_ID}","message":{{"id":"msg_fake_0001","role":"assistant","model":"{MODEL}","content":[{{"type":"tool_use","id":"{TOOL_USE_ID}","name":"Edit","input":{{"file_path":"src/main.rs","old_string":"foo","new_string":"bar"}}}}]}}}}"#
    );

    // 3. user line containing the matching tool_result.
    println!(
        r#"{{"type":"user","session_id":"{SESSION_ID}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{TOOL_USE_ID}","is_error":false,"content":"The file src/main.rs has been edited."}}]}}}}"#
    );

    // 4. assistant line with text.
    println!(
        r#"{{"type":"assistant","session_id":"{SESSION_ID}","message":{{"id":"msg_fake_0002","role":"assistant","model":"{MODEL}","content":[{{"type":"text","text":"Done. I edited src/main.rs as requested."}}]}}}}"#
    );

    // 5. result / success line — total_cost_usd, num_turns, usage.
    println!(
        r#"{{"type":"result","subtype":"success","is_error":false,"session_id":"{SESSION_ID}","total_cost_usd":0.01,"num_turns":2,"duration_ms":1234,"duration_api_ms":1000,"result":"Done. I edited src/main.rs as requested.","usage":{{"input_tokens":120,"output_tokens":45,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}"#
    );
}
