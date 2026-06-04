//! Append-only normalized event log (DESIGN §11 `agent_event` — the SOURCE OF
//! TRUTH for replaying a run).
//!
//! Backs the `agent_event` table: one row per `AgentEvent`, keyed
//! `(task_id, seq)` so the full I/O of any run can be read back in order and
//! replayed through the same sprite state machine (Phase D's Logbook). The
//! supervisor is the single writer (§3 append-log + emitter): every event it
//! streams live to the UI is *also* appended here.
//!
//! This module is deliberately decoupled from `quiver-core`'s `AgentEvent`
//! type: it stores the envelope fields plus the event `kind` and its
//! already-serialized `payload_json`. The Tauri shell, which owns the
//! `quiver-core` dependency, extracts those from the live `AgentEvent` before
//! calling [`append_event`](Store::append_event). Replay returns the raw
//! payload JSON, which the UI parses with the exact same wire contract it uses
//! for live events (`agentEvent.types.ts`) — no second mapping to drift.

use rusqlite::params;
use serde::Serialize;

use crate::Store;

/// One event row to append (the envelope + kind + serialized payload). Mirrors
/// the `AgentEvent` wire envelope (DESIGN §5.2) without depending on the
/// `quiver-core` type.
#[derive(Clone, Debug)]
pub struct NewEvent<'a> {
    pub task_id: &'a str,
    pub seq: u64,
    pub ts_ms: i64,
    /// Runner provenance string (`claude_cli` | `codex_cli` | `anthropic_api`).
    pub runner: &'a str,
    /// The payload tag (`worker_started` | `tool_use` | `output_chunk` | …).
    pub kind: &'a str,
    /// The full event serialized to a flat camelCase JSON object — the same
    /// shape the UI parses live. Stored verbatim for byte-exact replay.
    pub payload_json: &'a str,
}

/// One stored event, read back for replay (DESIGN §11, ordered by `seq`).
///
/// `payload_json` is the verbatim flat camelCase `AgentEvent` JSON; the UI
/// parses it with the live `agentEvent.types.ts` contract. Serialized camelCase
/// so the `get_task_events` command hands the UI a ready-to-replay list.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvent {
    pub task_id: String,
    pub seq: u64,
    pub ts_ms: i64,
    pub runner: String,
    pub kind: String,
    pub payload_json: String,
}

impl Store {
    /// Append one event to the log. Idempotent on `(task_id, seq)`:
    /// re-appending the same key replaces the row (`INSERT OR REPLACE`), so a
    /// replayed/retried write can't error or duplicate.
    pub fn append_event(&self, event: &NewEvent<'_>) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT OR REPLACE INTO agent_event
                (task_id, seq, ts_ms, runner, kind, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.task_id,
                event.seq as i64,
                event.ts_ms,
                event.runner,
                event.kind,
                event.payload_json,
            ],
        )?;
        Ok(())
    }

    /// All events for one task, ordered by `seq` ascending (replay order).
    pub fn events_for_task(&self, task_id: &str) -> anyhow::Result<Vec<StoredEvent>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT task_id, seq, ts_ms, runner, kind, payload_json
             FROM agent_event WHERE task_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map(params![task_id], |row| {
                let seq: i64 = row.get(1)?;
                Ok(StoredEvent {
                    task_id: row.get(0)?,
                    seq: seq as u64,
                    ts_ms: row.get(2)?,
                    runner: row.get(3)?,
                    kind: row.get(4)?,
                    payload_json: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev<'a>(task_id: &'a str, seq: u64, kind: &'a str, payload: &'a str) -> NewEvent<'a> {
        NewEvent {
            task_id,
            seq,
            ts_ms: 1_000 + seq as i64,
            runner: "claude_cli",
            kind,
            payload_json: payload,
        }
    }

    #[test]
    fn append_and_replay_in_seq_order() {
        let store = Store::open_in_memory().unwrap();
        // Append out of order to prove the read sorts by seq.
        store
            .append_event(&ev("t1", 2, "output_chunk", r#"{"kind":"output_chunk","text":"b"}"#))
            .unwrap();
        store
            .append_event(&ev("t1", 0, "worker_started", r#"{"kind":"worker_started"}"#))
            .unwrap();
        store
            .append_event(&ev("t1", 1, "output_chunk", r#"{"kind":"output_chunk","text":"a"}"#))
            .unwrap();

        let replayed = store.events_for_task("t1").unwrap();
        let seqs: Vec<u64> = replayed.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2], "replay must be seq-ascending");
        assert_eq!(replayed[0].kind, "worker_started");
        assert_eq!(replayed[2].payload_json, r#"{"kind":"output_chunk","text":"b"}"#);
    }

    #[test]
    fn events_are_scoped_per_task() {
        let store = Store::open_in_memory().unwrap();
        store.append_event(&ev("a", 0, "worker_started", "{}")).unwrap();
        store.append_event(&ev("b", 0, "worker_started", "{}")).unwrap();
        store.append_event(&ev("a", 1, "result", "{}")).unwrap();

        assert_eq!(store.events_for_task("a").unwrap().len(), 2);
        assert_eq!(store.events_for_task("b").unwrap().len(), 1);
        assert!(store.events_for_task("missing").unwrap().is_empty());
    }

    #[test]
    fn reappending_same_key_replaces_not_duplicates() {
        let store = Store::open_in_memory().unwrap();
        store.append_event(&ev("t", 0, "output_chunk", "first")).unwrap();
        store.append_event(&ev("t", 0, "output_chunk", "second")).unwrap();
        let rows = store.events_for_task("t").unwrap();
        assert_eq!(rows.len(), 1, "PK (task_id, seq) must dedupe");
        assert_eq!(rows[0].payload_json, "second");
    }

    #[test]
    fn events_persist_across_reopen() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = Store::default_db_path(dir.path());
        {
            let store = Store::open(&path).unwrap();
            store
                .append_event(&ev("kept", 0, "worker_started", r#"{"kind":"worker_started"}"#))
                .unwrap();
            store
                .append_event(&ev("kept", 1, "result", r#"{"kind":"result","ok":true}"#))
                .unwrap();
        }
        let reopened = Store::open(&path).unwrap();
        let rows = reopened.events_for_task("kept").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].kind, "result");
    }
}
