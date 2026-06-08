import type { TagKind } from '@/assets';

/** Rust task.status → 像素 StatusTag 种类。 */
export const TAG_OF: Record<string, TagKind> = {
  queued: 'paused',
  running: 'running',
  verifying: 'running',
  verified: 'verified',
  done: 'verified',
  failed: 'failed',
  verify_failed: 'failed',
  needs_rebase: 'paused',
};

/** Unix ms → 短时间:今天显示 HH:MM,否则 MM-DD。 */
export function shortTime(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  const pad = (n: number): string => String(n).padStart(2, '0');
  return d.toDateString() === now.toDateString()
    ? `${pad(d.getHours())}:${pad(d.getMinutes())}`
    : `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
