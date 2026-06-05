import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { EnvironmentCheck } from '@/types/environment.types';

// IPC contract with src-tauri/src/environment.rs — keep the command name in sync.
const CHECK_ENVIRONMENT = 'check_environment';

export interface EnvironmentCheckState {
  /** One verdict per probe (claude / git / path / worktree …), in panel order. */
  checks: EnvironmentCheck[];
  /** True while a check_environment round-trip is in flight. */
  loading: boolean;
  /** A command-level failure (vs per-check warn/fail rows inside `checks`). */
  error: string | null;
  /** Re-run the whole probe battery (the 工坊体检 page's 重新检查 button). */
  refresh: () => Promise<void>;
}

/**
 * The seam between the 工坊体检 (workshop pre-flight) UI and the local-only
 * `check_environment` probe battery (DESIGN §9). On mount it runs the checks once;
 * `refresh` re-runs them on demand. Every probe is filesystem/env-only or a single
 * bounded subprocess — zero Anthropic API calls — so this is cheap to re-run.
 */
export function useEnvironmentCheck(): EnvironmentCheckState {
  const [checks, setChecks] = useState<EnvironmentCheck[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<EnvironmentCheck[]>(CHECK_ENVIRONMENT);
      setChecks(result);
    } catch (e) {
      setError(typeof e === 'string' ? e : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { checks, loading, error, refresh };
}
