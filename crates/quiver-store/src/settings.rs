//! Single-row typed app-settings persistence (DESIGN §11 `app_config`, v1.0
//! spec module 1 — the "settings ledger").
//!
//! Backs the `settings` table: exactly one row (`id = 1`) holding the whole
//! typed config the UI's cozy settings ledger (Phase B) reads and writes. The
//! row is materialized with sensible defaults on first open so callers never
//! face an empty store — [`get_settings`](Store::get_settings) always returns a
//! complete [`Settings`].
//!
//! Updates are *partial*: [`update_settings`](Store::update_settings) takes a
//! [`SettingsPatch`] where every field is `Option`, and only the `Some` fields
//! are written. This lets the UI persist one toggle at a time (immediate-apply)
//! without round-tripping the whole struct.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::Store;

/// The default run mode (§4.2): the free, no-cost simulation by default so a
/// fresh install can never spend real credit by accident.
const DEFAULT_MODE: &str = "simulate";
/// The default model the real runner requests.
const DEFAULT_MODEL: &str = "sonnet";
/// Default parallel-worker ceiling (DESIGN §3 fan-out; v1 targets 2–3).
const DEFAULT_MAX_WORKERS: i64 = 3;
/// Default UI theme key for the cozy shell.
const DEFAULT_THEME: &str = "cozy";
/// Default UI scale (1.0 = 100%).
const DEFAULT_UI_SCALE: f64 = 1.0;
/// Default fake-claude pacing (ms between streamed lines) for simulate mode.
const DEFAULT_FAKE_DELAY_MS: i64 = 120;

/// The whole typed app-settings row (DESIGN §11, v1.0 spec module 1).
///
/// Serialized camelCase over Tauri so the React settings ledger consumes it
/// directly. `None` on the budget fields means "no cap configured" (distinct
/// from a `0` cap), matching the DESIGN §11 nullable budget columns and the
/// honest-economics stance (§1.3): an unset dollar ceiling is explicit, not a
/// silent zero.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// `"simulate"` (free, default) | `"real"` (official `claude`, §4.2).
    pub default_mode: String,
    /// The model the real runner requests (e.g. `"sonnet"`).
    pub model: String,
    /// Parallel-worker ceiling (DESIGN §3).
    pub max_workers: i64,
    /// Monthly dollar credit cap to govern against (§1.3, §10). `None` = unset.
    pub monthly_credit_cap_usd: Option<f64>,
    /// Per-night dollar budget to hard-pause at (§10). `None` = unset.
    pub nightly_budget_usd: Option<f64>,
    /// Absolute path override for the agent binary (§12). `None` = auto-resolve.
    pub agent_bin_override: Option<String>,
    /// Simulate-mode pacing: ms between streamed `fake-claude` lines.
    pub fake_delay_ms: i64,
    /// UI theme key for the cozy shell.
    pub theme: String,
    /// UI scale factor (1.0 = 100%).
    pub ui_scale: f64,
    /// The verify-gate command (DESIGN §7), run as `sh -c <cmd>` in each task's
    /// worktree before merge. Empty = no real gate (treated as always-pass, the
    /// original behavior) so existing setups are unchanged until a command is set.
    pub verify_command: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_mode: DEFAULT_MODE.to_string(),
            model: DEFAULT_MODEL.to_string(),
            max_workers: DEFAULT_MAX_WORKERS,
            monthly_credit_cap_usd: None,
            nightly_budget_usd: None,
            agent_bin_override: None,
            fake_delay_ms: DEFAULT_FAKE_DELAY_MS,
            theme: DEFAULT_THEME.to_string(),
            ui_scale: DEFAULT_UI_SCALE,
            verify_command: String::new(),
        }
    }
}

/// A partial update to [`Settings`]: every field is optional, and only the
/// `Some` fields are written (DESIGN §11 immediate-apply). The budget fields are
/// `Option<Option<f64>>` so the UI can distinguish "leave unchanged" (`None`)
/// from "explicitly clear the cap" (`Some(None)`) versus "set a cap"
/// (`Some(Some(x))`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub default_mode: Option<String>,
    pub model: Option<String>,
    pub max_workers: Option<i64>,
    #[serde(default, deserialize_with = "crate::settings::de_double_option")]
    pub monthly_credit_cap_usd: Option<Option<f64>>,
    #[serde(default, deserialize_with = "crate::settings::de_double_option")]
    pub nightly_budget_usd: Option<Option<f64>>,
    #[serde(default, deserialize_with = "crate::settings::de_double_option_str")]
    pub agent_bin_override: Option<Option<String>>,
    pub fake_delay_ms: Option<i64>,
    pub theme: Option<String>,
    pub ui_scale: Option<f64>,
    pub verify_command: Option<String>,
}

/// Deserialize a present-but-maybe-null JSON field into `Some(Option<f64>)`,
/// while an absent field stays `None` (via `#[serde(default)]`). This is what
/// lets the wire distinguish "clear the cap" (`null`) from "don't touch it"
/// (key absent).
fn de_double_option<'de, D>(deserializer: D) -> Result<Option<Option<f64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<f64>::deserialize(deserializer)?))
}

/// Same as [`de_double_option`] but for an optional string field.
fn de_double_option_str<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

impl Store {
    /// The current settings, always complete (defaults are materialized on
    /// first open, so this never returns a partial/empty config).
    pub fn get_settings(&self) -> anyhow::Result<Settings> {
        let conn = self.conn.lock().expect("store lock");
        let row = conn
            .query_row(
                "SELECT default_mode, model, max_workers, monthly_credit_cap_usd,
                        nightly_budget_usd, agent_bin_override, fake_delay_ms, theme, ui_scale,
                        verify_command
                 FROM settings WHERE id = 1",
                [],
                |row| {
                    Ok(Settings {
                        default_mode: row.get(0)?,
                        model: row.get(1)?,
                        max_workers: row.get(2)?,
                        monthly_credit_cap_usd: row.get(3)?,
                        nightly_budget_usd: row.get(4)?,
                        agent_bin_override: row.get(5)?,
                        fake_delay_ms: row.get(6)?,
                        theme: row.get(7)?,
                        ui_scale: row.get(8)?,
                        verify_command: row.get(9)?,
                    })
                },
            )
            .optional()?;
        // The migration seeds row 1, so `None` only happens if a caller deleted
        // it — fall back to defaults rather than erroring.
        Ok(row.unwrap_or_default())
    }

    /// Apply a partial update; only the `Some` fields of `patch` are written.
    /// Returns the resulting full [`Settings`].
    pub fn update_settings(&self, patch: &SettingsPatch) -> anyhow::Result<Settings> {
        // Read-modify-write under one lock so a concurrent partial update can't
        // interleave and lose a field. The local file + brief lock make this
        // cheap (no `.await` is held across it).
        let conn = self.conn.lock().expect("store lock");
        let mut current: Settings = conn
            .query_row(
                "SELECT default_mode, model, max_workers, monthly_credit_cap_usd,
                        nightly_budget_usd, agent_bin_override, fake_delay_ms, theme, ui_scale,
                        verify_command
                 FROM settings WHERE id = 1",
                [],
                |row| {
                    Ok(Settings {
                        default_mode: row.get(0)?,
                        model: row.get(1)?,
                        max_workers: row.get(2)?,
                        monthly_credit_cap_usd: row.get(3)?,
                        nightly_budget_usd: row.get(4)?,
                        agent_bin_override: row.get(5)?,
                        fake_delay_ms: row.get(6)?,
                        theme: row.get(7)?,
                        ui_scale: row.get(8)?,
                        verify_command: row.get(9)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default();

        if let Some(v) = &patch.default_mode {
            current.default_mode = v.clone();
        }
        if let Some(v) = &patch.model {
            current.model = v.clone();
        }
        if let Some(v) = patch.max_workers {
            current.max_workers = v;
        }
        if let Some(v) = patch.monthly_credit_cap_usd {
            current.monthly_credit_cap_usd = v;
        }
        if let Some(v) = patch.nightly_budget_usd {
            current.nightly_budget_usd = v;
        }
        if let Some(v) = &patch.agent_bin_override {
            current.agent_bin_override = v.clone();
        }
        if let Some(v) = patch.fake_delay_ms {
            current.fake_delay_ms = v;
        }
        if let Some(v) = &patch.theme {
            current.theme = v.clone();
        }
        if let Some(v) = patch.ui_scale {
            current.ui_scale = v;
        }
        if let Some(v) = &patch.verify_command {
            current.verify_command = v.clone();
        }

        conn.execute(
            "INSERT INTO settings
                (id, default_mode, model, max_workers, monthly_credit_cap_usd,
                 nightly_budget_usd, agent_bin_override, fake_delay_ms, theme, ui_scale,
                 verify_command)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                default_mode           = excluded.default_mode,
                model                  = excluded.model,
                max_workers            = excluded.max_workers,
                monthly_credit_cap_usd = excluded.monthly_credit_cap_usd,
                nightly_budget_usd     = excluded.nightly_budget_usd,
                agent_bin_override     = excluded.agent_bin_override,
                fake_delay_ms          = excluded.fake_delay_ms,
                theme                  = excluded.theme,
                ui_scale               = excluded.ui_scale,
                verify_command         = excluded.verify_command",
            params![
                current.default_mode,
                current.model,
                current.max_workers,
                current.monthly_credit_cap_usd,
                current.nightly_budget_usd,
                current.agent_bin_override,
                current.fake_delay_ms,
                current.theme,
                current.ui_scale,
                current.verify_command,
            ],
        )?;
        Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_store_returns_defaults() {
        let store = Store::open_in_memory().unwrap();
        let s = store.get_settings().unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.default_mode, "simulate", "default mode must be free");
        assert_eq!(s.monthly_credit_cap_usd, None, "no cap configured by default");
    }

    #[test]
    fn partial_update_only_writes_some_fields() {
        let store = Store::open_in_memory().unwrap();
        let patch = SettingsPatch {
            max_workers: Some(5),
            theme: Some("midnight".to_string()),
            ..SettingsPatch::default()
        };
        let updated = store.update_settings(&patch).unwrap();
        assert_eq!(updated.max_workers, 5);
        assert_eq!(updated.theme, "midnight");
        // Untouched fields keep their defaults.
        assert_eq!(updated.model, Settings::default().model);
        assert_eq!(updated.default_mode, "simulate");
    }

    #[test]
    fn budget_caps_round_trip_and_clear() {
        let store = Store::open_in_memory().unwrap();
        // Set a cap.
        store
            .update_settings(&SettingsPatch {
                monthly_credit_cap_usd: Some(Some(100.0)),
                ..SettingsPatch::default()
            })
            .unwrap();
        assert_eq!(store.get_settings().unwrap().monthly_credit_cap_usd, Some(100.0));
        // Explicitly clear it (Some(None) = clear).
        store
            .update_settings(&SettingsPatch {
                monthly_credit_cap_usd: Some(None),
                ..SettingsPatch::default()
            })
            .unwrap();
        assert_eq!(store.get_settings().unwrap().monthly_credit_cap_usd, None);
    }

    #[test]
    fn update_persists_across_reopen() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = Store::default_db_path(dir.path());
        {
            let store = Store::open(&path).unwrap();
            store
                .update_settings(&SettingsPatch {
                    model: Some("opus".to_string()),
                    nightly_budget_usd: Some(Some(8.0)),
                    agent_bin_override: Some(Some("/opt/claude".to_string())),
                    ..SettingsPatch::default()
                })
                .unwrap();
        }
        let reopened = Store::open(&path).unwrap();
        let s = reopened.get_settings().unwrap();
        assert_eq!(s.model, "opus");
        assert_eq!(s.nightly_budget_usd, Some(8.0));
        assert_eq!(s.agent_bin_override.as_deref(), Some("/opt/claude"));
    }

    #[test]
    fn patch_double_option_deserializes_absent_vs_null() {
        // Absent key → leave unchanged (None).
        let absent: SettingsPatch = serde_json::from_str("{}").unwrap();
        assert!(absent.monthly_credit_cap_usd.is_none());
        // Explicit null → clear (Some(None)).
        let null: SettingsPatch =
            serde_json::from_str(r#"{"monthlyCreditCapUsd": null}"#).unwrap();
        assert_eq!(null.monthly_credit_cap_usd, Some(None));
        // A value → set (Some(Some(x))).
        let set: SettingsPatch =
            serde_json::from_str(r#"{"monthlyCreditCapUsd": 42.5}"#).unwrap();
        assert_eq!(set.monthly_credit_cap_usd, Some(Some(42.5)));
    }
}
