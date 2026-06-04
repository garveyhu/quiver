import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Settings, SettingsPatch } from '@/types/persistence.types';

// IPC contract with src-tauri/src/lib.rs — keep names in sync.
const GET_SETTINGS = 'get_settings';
const UPDATE_SETTINGS = 'update_settings';

// How long to coalesce rapid edits (slider drags, typing) before persisting.
const SAVE_DEBOUNCE_MS = 350;

export type SaveStatus = 'idle' | 'saving' | 'saved' | 'error';

export interface SettingsState {
  settings: Settings | null;
  loading: boolean;
  saveStatus: SaveStatus;
  /** Optimistically merge a patch into local state and persist it (debounced). */
  patch: (patch: SettingsPatch) => void;
}

/**
 * The single seam between the UI and the durable §11 settings row. Loads the
 * full [`Settings`] on mount and exposes a debounced [`patch`] that updates local
 * state optimistically (instant UI) then persists via `update_settings`.
 *
 * Patches are merged so several edits within the debounce window collapse into
 * one round-trip; `update_settings` does partial writes server-side, so only the
 * touched fields hit SQLite. `saveStatus` drives the cozy "记录中… / 已记录" chip.
 */
export function useSettings(): SettingsState {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('idle');

  // Pending merged patch + its debounce timer, kept in refs so they survive
  // re-renders without re-triggering effects.
  const pendingRef = useRef<SettingsPatch>({});
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const savedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let active = true;
    invoke<Settings>(GET_SETTINGS)
      .then(s => {
        if (active) setSettings(s);
      })
      .catch(() => {
        if (active) setSaveStatus('error');
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
      if (timerRef.current) clearTimeout(timerRef.current);
      if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
    };
  }, []);

  const flush = useCallback(async () => {
    const merged = pendingRef.current;
    pendingRef.current = {};
    if (Object.keys(merged).length === 0) return;
    setSaveStatus('saving');
    try {
      const next = await invoke<Settings>(UPDATE_SETTINGS, { patch: merged });
      setSettings(next);
      setSaveStatus('saved');
      if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
      savedTimerRef.current = setTimeout(() => setSaveStatus('idle'), 1400);
    } catch {
      setSaveStatus('error');
    }
  }, []);

  const patch = useCallback(
    (next: SettingsPatch) => {
      // Optimistic local merge so controls feel instant.
      setSettings(prev => (prev ? { ...prev, ...stripUnchanged(next) } : prev));
      pendingRef.current = { ...pendingRef.current, ...next };
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => void flush(), SAVE_DEBOUNCE_MS);
    },
    [flush],
  );

  return { settings, loading, saveStatus, patch };
}

/**
 * Drop keys whose value is `undefined` from a patch before merging into local
 * state. (A `null` is meaningful — it clears a nullable cap — so it is kept.)
 */
function stripUnchanged(patch: SettingsPatch): Partial<Settings> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(patch)) {
    if (v !== undefined) out[k] = v;
  }
  return out as Partial<Settings>;
}
