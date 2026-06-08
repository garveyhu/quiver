use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub task_id: String,
    pub seq: u64,
    pub ts_ms: i64,
    pub runner: RunnerKind,
    #[serde(flatten)]
    pub payload: AgentEventPayload,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerKind {
    ClaudeCli,
    CodexCli,
    AnthropicApi,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMode {
    Subscription,
    ApiKey,
    OpenAiEndpoint,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum AgentEventPayload {
    WorkerStarted {
        /// Backend session handle (Claude `--resume` token). Threaded out so the
        /// manager can durably resume this worker's context (DESIGN §4/§6).
        session_id: Option<String>,
        model: Option<String>,
        auth_mode: AuthMode,
    },
    ToolUse { tool: String, summary: String },
    OutputChunk { text: String },
    Result {
        ok: bool,
        cost_usd: Option<f64>,
        num_turns: u32,
        tokens: Option<u64>,
        duration_ms: Option<u64>,
    },
    Error { code: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_started_serializes_flat_camelcase() {
        let ev = AgentEvent {
            task_id: "t1".into(),
            seq: 0,
            ts_ms: 1,
            runner: RunnerKind::ClaudeCli,
            payload: AgentEventPayload::WorkerStarted {
                session_id: Some("sess-1".into()),
                model: Some("sonnet".into()),
                auth_mode: AuthMode::Subscription,
            },
        };
        let v: serde_json::Value = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["kind"], "worker_started");
        assert_eq!(v["taskId"], "t1");
        assert_eq!(v["authMode"], "subscription");
        assert_eq!(v["sessionId"], "sess-1");
    }
}
