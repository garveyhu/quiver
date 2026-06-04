/**
 * DEV-ONLY Tauri IPC mock — screenshot/verification harness for Phase C.
 *
 * The real backend is a Tauri Rust core; a plain browser has no `invoke`/`listen`
 * bridge. This wires the official `@tauri-apps/api/mocks` `mockIPC` seam (with
 * `shouldMockEvents` so `listen`/`unlisten`/`emit` all work) — ONLY when the app
 * is opened with `?mock` in dev — so the bulletin board + 工坊 can be exercised
 * headless. It simulates the concurrent queue scheduler: pinned tasks go
 * queued → running (streaming believable `agent-event`s) → done, capped at a
 * `maxWorkers` of 3, with `task-updated` driving live board refreshes.
 *
 * Installed for side effects from `main.tsx` behind a dev + query-flag gate; the
 * packaged app never passes `?mock`, so the real Tauri internals stay in place.
 */

import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';

interface MockTask {
  id: string;
  project: string;
  prompt: string;
  mode: string;
  status: string;
  costUsd: number | null;
  branch: string | null;
  position: number;
  createdAt: number;
  updatedAt: number;
}

const MAX_WORKERS = 3;
const STEP_MS = 900;

const tasks: MockTask[] = [];
let running = 0;
let seq = 1000;

function now(): number {
  return Date.now();
}

function emitBoard(): void {
  void emit('task-updated', '');
}

function agentEvent(taskId: string, kind: string, extra: Record<string, unknown>): void {
  void emit('agent-event', {
    taskId,
    seq: seq++,
    tsMs: now(),
    runner: 'claude_cli',
    kind,
    ...extra,
  });
}

function wait(ms: number): Promise<void> {
  return new Promise(r => setTimeout(r, ms));
}

/** Drive one task through a believable streaming lifecycle, then free its slot. */
async function runTask(task: MockTask): Promise<void> {
  task.status = 'running';
  task.updatedAt = now();
  emitBoard();

  agentEvent(task.id, 'worker_started', { model: 'claude-sonnet-4-5', authMode: 'subscription' });
  await wait(STEP_MS);
  agentEvent(task.id, 'tool_use', { tool: 'Read', summary: 'README.md' });
  await wait(STEP_MS);
  agentEvent(task.id, 'tool_use', { tool: 'Bash', summary: 'cargo build' });
  await wait(STEP_MS);
  agentEvent(task.id, 'output_chunk', { text: `处理「${task.prompt}」中…` });
  await wait(STEP_MS);

  task.status = 'verifying';
  task.updatedAt = now();
  emitBoard();
  const cost = Math.round((0.01 + Math.random() * 0.05) * 100) / 100;
  agentEvent(task.id, 'result', { ok: true, costUsd: cost, numTurns: 2 });
  await wait(STEP_MS);

  task.status = 'verified';
  task.costUsd = cost;
  task.branch = task.mode === 'real' ? `quiver/${task.id}/attempt-1` : null;
  task.updatedAt = now();
  agentEvent(task.id, 'finished', { status: 'verified', costUsd: cost, branch: task.branch });
  emitBoard();

  running -= 1;
  pump();
}

/** Claim queued tasks while a slot is free — the maxWorkers concurrency cap. */
function pump(): void {
  for (const task of tasks) {
    if (running >= MAX_WORKERS) break;
    if (task.status === 'queued') {
      running += 1;
      void runTask(task);
    }
  }
}

/** The mocked command surface (mirrors src-tauri/src/lib.rs). */
function handleCommand(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case 'get_initial_state':
      return { lastProject: '/Users/demo/repos/quiver', recentProjects: [], history: [] };
    case 'get_settings':
      return {
        defaultMode: 'simulate',
        model: 'sonnet',
        maxWorkers: MAX_WORKERS,
        monthlyCreditCapUsd: null,
        nightlyBudgetUsd: null,
        agentBinOverride: null,
        fakeDelayMs: 700,
        theme: 'cozy',
        uiScale: 1,
      };
    case 'list_tasks':
      return [...tasks].sort((a, b) => a.position - b.position || a.createdAt - b.createdAt);
    case 'enqueue_task_cmd': {
      const id = `task-${now()}-${tasks.length}`;
      const task: MockTask = {
        id,
        project: '/Users/demo/repos/quiver',
        prompt: String(args.prompt ?? '任务'),
        mode: String(args.mode ?? 'simulate'),
        status: 'queued',
        costUsd: null,
        branch: null,
        position: tasks.length,
        createdAt: now(),
        updatedAt: now(),
      };
      tasks.push(task);
      emitBoard();
      pump();
      return task;
    }
    case 'reorder_task': {
      const t = tasks.find(x => x.id === args.id);
      if (t) t.position = Number(args.position);
      emitBoard();
      return null;
    }
    case 'cancel_task_cmd': {
      const i = tasks.findIndex(x => x.id === args.id);
      if (i >= 0 && tasks[i].status === 'queued') tasks.splice(i, 1);
      emitBoard();
      return null;
    }
    case 'update_settings':
      return handleCommand('get_settings', {});
    default:
      return null;
  }
}

/** Install the official IPC mock with event support. */
export function installDevMock(): void {
  mockIPC((cmd, args) => handleCommand(cmd, (args ?? {}) as Record<string, unknown>), {
    shouldMockEvents: true,
  });
  // eslint-disable-next-line no-console
  console.info('[quiver] dev Tauri mock installed (?mock) — concurrent queue simulated');
}
