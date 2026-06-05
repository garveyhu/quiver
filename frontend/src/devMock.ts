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

// The mocked durable settings row. `update_settings` merges the patch into it and
// returns the merged whole — mirroring the real Rust partial-write contract — so
// the settings ledger's edits persist within the session and round-trip faithfully.
const settingsRow: Record<string, unknown> = {
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

const tasks: MockTask[] = [];
let running = 0;
let seq = 1000;

// Per-task event log so `get_task_events` can replay a finished run's full I/O in
// the Logbook (mirrors the durable §11 agent_event table).
interface MockStoredEvent {
  taskId: string;
  seq: number;
  tsMs: number;
  runner: string;
  kind: string;
  payloadJson: string;
}
const eventLog: Record<string, MockStoredEvent[]> = {};

function now(): number {
  return Date.now();
}

function emitBoard(): void {
  void emit('task-updated', '');
}

/** Record an event into the per-task log (so the Logbook can replay it later). */
function recordEvent(
  taskId: string,
  kind: string,
  extra: Record<string, unknown>,
  tsMs: number,
): void {
  const payload = { taskId, seq: seq, tsMs, runner: 'claude_cli', kind, ...extra };
  (eventLog[taskId] ??= []).push({
    taskId,
    seq,
    tsMs,
    runner: 'claude_cli',
    kind,
    payloadJson: JSON.stringify(payload),
  });
}

function agentEvent(taskId: string, kind: string, extra: Record<string, unknown>): void {
  const tsMs = now();
  recordEvent(taskId, kind, extra, tsMs);
  void emit('agent-event', {
    taskId,
    seq: seq++,
    tsMs,
    runner: 'claude_cli',
    kind,
    ...extra,
  });
}

/** Seed a finished historical run (task + full event log) for the 档案库 demo. */
function seedHistory(
  id: string,
  project: string,
  prompt: string,
  modeName: string,
  status: string,
  cost: number,
  branch: string | null,
  ageMs: number,
): void {
  const created = now() - ageMs;
  tasks.push({
    id,
    project,
    prompt,
    mode: modeName,
    status,
    costUsd: cost,
    branch,
    position: tasks.length,
    createdAt: created,
    updatedAt: created + 5000,
  });
  let t = created;
  const step = (kind: string, extra: Record<string, unknown>) => {
    recordEvent(id, kind, extra, t);
    seq++;
    t += 1100;
  };
  step('worker_started', { model: 'claude-sonnet-4-5', authMode: 'subscription' });
  step('tool_use', { tool: 'Read', summary: 'README.md' });
  step('tool_use', { tool: 'Bash', summary: 'cargo build --workspace' });
  step('output_chunk', { text: `分析「${prompt}」并实现改动…\n读取相关文件、定位修改点、应用补丁。` });
  step('tool_use', { tool: 'Edit', summary: 'src/lib.rs（+18 −4）' });
  step('output_chunk', { text: '改动完成，运行测试验证。' });
  const ok = status === 'verified';
  step('result', { ok, costUsd: cost, numTurns: 4 });
  if (!ok) {
    step('error', { code: 'verify_failed', message: '验证未通过：2 个测试失败' });
  }
  step('finished', { status, costUsd: cost, branch });
}

const DEMO_PROJECT = '/Users/demo/repos/quiver';
const DEMO_PROJECT_2 = '/Users/demo/repos/sage';
seedHistory('arc-1', DEMO_PROJECT, '为召回模块接入 bge-reranker 二阶段重排', 'real', 'verified', 0.42, 'quiver/arc-1/attempt-1', 86_400_000);
seedHistory('arc-2', DEMO_PROJECT, '把运行历史改成可搜索的档案库', 'simulate', 'verified', 0.07, null, 43_200_000);
seedHistory('arc-3', DEMO_PROJECT_2, '修复 token 过期后无法刷新的问题', 'real', 'failed', 0.19, null, 21_600_000);
seedHistory('arc-4', DEMO_PROJECT, '给公告板卡片加拖拽排序', 'simulate', 'verified', 0.05, null, 7_200_000);

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

// `?mock&noproject` exercises the cozy first-run (no repo picked) empty state.
const NO_PROJECT = location.search.includes('noproject');

/** The mocked command surface (mirrors src-tauri/src/lib.rs). */
function handleCommand(cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case 'get_initial_state':
      return {
        lastProject: NO_PROJECT ? null : '/Users/demo/repos/quiver',
        recentProjects: NO_PROJECT
          ? []
          : [
              { path: '/Users/demo/repos/quiver', lastUsedAt: now() - 3_600_000 },
              { path: '/Users/demo/repos/sage', lastUsedAt: now() - 86_400_000 },
            ],
      };
    case 'get_settings':
      return { ...settingsRow };
    case 'list_tasks': {
      // `list_tasks` filters by project/status; the archive passes both null to
      // get every run across projects (the bulletin board passes a project).
      const project = args.project == null ? null : String(args.project);
      const status = args.status == null ? null : String(args.status);
      return [...tasks]
        .filter(t => (project == null || t.project === project) && (status == null || t.status === status))
        .sort((a, b) => a.position - b.position || a.createdAt - b.createdAt);
    }
    case 'get_task_events': {
      const id = String(args.taskId ?? '');
      return [...(eventLog[id] ?? [])].sort((a, b) => a.seq - b.seq);
    }
    case 'check_environment':
      // A believable mixed verdict so the 工坊体检 page shows ✓/⚠/✗ at once.
      return [
        {
          id: 'claude_found',
          label: 'Claude 命令',
          status: 'ok',
          message: '已找到 claude：/opt/homebrew/bin/claude',
          detail: '/opt/homebrew/bin/claude',
        },
        {
          id: 'claude_version',
          label: 'Claude 版本',
          status: 'ok',
          message: 'claude 版本：1.0.0 (Claude Code)',
          detail: '1.0.0 (Claude Code)',
        },
        {
          id: 'claude_auth',
          label: 'Claude 登录态',
          status: 'warn',
          message: '未检测到 Claude 登录凭证——真实模式可能因未登录而失败。',
          remediation: '请在终端运行 `claude` 并完成登录（订阅 / OAuth），登录后重新检查。',
        },
        {
          id: 'subscription_env',
          label: '订阅路由',
          status: 'ok',
          message: '未检测到会绕过订阅的环境变量。',
        },
        {
          id: 'git_found',
          label: 'Git 命令',
          status: 'ok',
          message: '已找到 git：/usr/bin/git',
          detail: '/usr/bin/git',
        },
        {
          id: 'path_fixed',
          label: 'PATH 修复',
          status: 'ok',
          message: 'PATH 看起来已修复（包含常见的命令目录）。',
        },
        {
          id: 'worktree_temp',
          label: '工作树暂存',
          status: NO_PROJECT ? 'warn' : 'ok',
          message: NO_PROJECT
            ? '尚未选择项目——无法检测工作树暂存目录。'
            : '工作树暂存目录可写。',
          remediation: NO_PROJECT ? '请先在门口选择一个 git 项目，然后重新检查。' : undefined,
          detail: NO_PROJECT ? undefined : '/Users/demo/repos/quiver/.quiver/worktrees',
        },
      ];
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
    case 'update_settings': {
      // Partial write: merge only the keys present in the patch (omit = unchanged,
      // explicit null = clear a nullable cap) — exactly the Rust contract.
      const patch = (args.patch ?? {}) as Record<string, unknown>;
      for (const [k, v] of Object.entries(patch)) {
        if (v !== undefined) settingsRow[k] = v;
      }
      return { ...settingsRow };
    }
    default:
      return null;
  }
}

/**
 * `?mock&autostart` auto-pins a couple of live commissions right after install so
 * the headless screenshot harness sees archers at work without a manual enqueue.
 * Drives the same lifecycle path as a real pin (`enqueue_task_cmd` → pump → run),
 * so the whole bus→HallScene chain is exercised end to end.
 */
function autostartDemoRun(): void {
  const seed = [
    '修复登录超时：刷新 token 续期逻辑',
    '为公告板卡片加拖拽排序',
    '把运行历史改成可搜索的档案库',
  ];
  seed.forEach((prompt, i) => {
    setTimeout(() => {
      handleCommand('enqueue_task_cmd', { prompt, mode: i === 0 ? 'real' : 'simulate' });
    }, 400 + i * 500);
  });
}

/** Install the official IPC mock with event support. */
export function installDevMock(): void {
  mockIPC((cmd, args) => handleCommand(cmd, (args ?? {}) as Record<string, unknown>), {
    shouldMockEvents: true,
  });
  // `&noproject` exercises the cozy empty workshop, so it must NOT auto-pin any
  // commissions (there's no project to run them in) — only autostart with a project.
  if (location.search.includes('autostart') && !NO_PROJECT) {
    autostartDemoRun();
  }
  // eslint-disable-next-line no-console
  console.info('[quiver] dev Tauri mock installed (?mock) — concurrent queue simulated');
}
