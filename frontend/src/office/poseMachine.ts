import type { AgentEvent } from '@/types/agentEvent.types';
import type { Pose, WorkerView, WorkerLogEntry } from '@/office/types';

// Pure per-task pose state machine (§5.5). Maps the live AgentEvent stream onto
// the archer's behavioural states. Latching matches the design: only terminal
// error / Finished drive the sticky celebrate|sick, and a late event cannot
// bounce a terminal pose.

const SHELL_TOOLS = new Set(['Bash', 'bash', 'shell', 'sh', 'zsh', 'Shell']);

function isShellTool(tool: string): boolean {
  return SHELL_TOOLS.has(tool);
}

export function createWorker(taskId: string, slot: number): WorkerView {
  return {
    taskId,
    slot,
    pose: 'idle',
    bubble: '待命中…',
    detail: '等待派发任务',
    costUsd: null,
    sparkle: false,
    terminal: false,
    log: [],
  };
}

function costText(cost: number | null): string {
  return cost === null ? '未统计' : `$${cost.toFixed(2)}`;
}

function pushLog(prev: WorkerView, event: AgentEvent, text: string): WorkerLogEntry[] {
  const entry: WorkerLogEntry = { seq: event.seq, kind: event.kind, text };
  // cap the in-memory trail so a chatty worker can't grow unbounded.
  const next = [...prev.log, entry];
  return next.length > 60 ? next.slice(next.length - 60) : next;
}

// Compute the next worker view from the current one + an incoming event.
// Returns a NEW object (immutable) so React can diff cheaply.
export function reduceWorker(prev: WorkerView, event: AgentEvent): WorkerView {
  // Latched terminal poses are sticky — ignore everything except a fresh start.
  if (prev.terminal && event.kind !== 'worker_started') {
    return { ...prev, log: pushLog(prev, event, terminalLogText(event)) };
  }

  switch (event.kind) {
    case 'worker_started': {
      const detail = event.model ? `就位 · 模型 ${event.model}` : '走向工位…';
      return {
        ...prev,
        pose: 'arriving',
        bubble: '走向工位…',
        detail,
        sparkle: false,
        terminal: false,
        log: pushLog(prev, event, detail),
      };
    }

    case 'output_chunk': {
      const text = event.text.trim().slice(0, 80) || '（输出）';
      return {
        ...prev,
        pose: 'working',
        bubble: text,
        detail: text,
        log: pushLog(prev, event, text),
      };
    }

    case 'tool_use': {
      const shell = isShellTool(event.tool);
      const pose: Pose = shell ? 'tense' : 'working';
      const verb = shell ? '跑命令' : '调用工具';
      const detail = `${verb}：${event.tool}${event.summary ? ` — ${event.summary}` : ''}`;
      return {
        ...prev,
        pose,
        bubble: `${verb}：${event.tool}`,
        detail,
        log: pushLog(prev, event, detail),
      };
    }

    case 'result': {
      const detail = `等待校验 · ${event.numTurns} 回合 · 花费 ${costText(event.costUsd)}`;
      return {
        ...prev,
        pose: 'tense',
        bubble: `等待校验… ${costText(event.costUsd)}`,
        detail,
        costUsd: event.costUsd ?? prev.costUsd,
        log: pushLog(prev, event, detail),
      };
    }

    case 'error': {
      const detail = `出错：${event.code} — ${event.message}`;
      return {
        ...prev,
        pose: 'sick',
        bubble: `出错：${event.code}`,
        detail,
        terminal: true,
        log: pushLog(prev, event, detail),
      };
    }

    case 'finished': {
      const verified = event.status === 'verified' || event.status === 'Verified';
      if (verified) {
        const detail = `完成并通过校验 · 花费 ${costText(event.costUsd)}`;
        return {
          ...prev,
          pose: 'celebrate',
          bubble: `完成 ✓ ${costText(event.costUsd)}`,
          detail,
          costUsd: event.costUsd ?? prev.costUsd,
          sparkle: true,
          terminal: true,
          log: pushLog(prev, event, detail),
        };
      }
      const label =
        event.status === 'verifyFailed' || event.status === 'VerifyFailed' ? '校验未过' : '失败';
      const detail = `${label}：${event.status} · 花费 ${costText(event.costUsd)}`;
      return {
        ...prev,
        pose: 'sick',
        bubble: `${label}`,
        detail,
        costUsd: event.costUsd ?? prev.costUsd,
        sparkle: false,
        terminal: true,
        log: pushLog(prev, event, detail),
      };
    }
  }
}

function terminalLogText(event: AgentEvent): string {
  switch (event.kind) {
    case 'output_chunk':
      return event.text.trim().slice(0, 80) || '（输出）';
    case 'tool_use':
      return `调用工具：${event.tool}`;
    case 'result':
      return `结果 · 花费 ${costText(event.costUsd)}`;
    default:
      return event.kind;
  }
}
