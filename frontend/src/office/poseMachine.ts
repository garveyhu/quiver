import type { AgentEvent } from '@/types/agentEvent.types';
import type { Pose, WorkerView } from '@/office/types';

// Pure per-task pose state machine (§5.5, collapsed to the v1 pose set).
//
// v1 deliberately maps the design's typing/coffee/sweating/celebrating/sick set
// onto 5 poses we have art for: working / tense / celebrate / sick / idle. The
// pause-path coffee states fold into `tense` for now (the user will eyeball and
// we iterate). Latching matches the design: only terminal error / Finished
// drive the sticky celebrate|sick; a late event cannot bounce a terminal pose.

const SHELL_TOOLS = new Set(['Bash', 'bash', 'shell', 'sh', 'zsh', 'Shell']);

function isShellTool(tool: string): boolean {
  return SHELL_TOOLS.has(tool);
}

export function createWorker(taskId: string, slot: number): WorkerView {
  return { taskId, slot, pose: 'idle', bubble: '待命中…', sparkle: false, terminal: false };
}

function costText(cost: number | null): string {
  return cost === null ? '未统计' : `$${cost.toFixed(2)}`;
}

// Compute the next worker view from the current one + an incoming event.
// Returns a NEW object (immutable) so React/Phaser can diff cheaply.
export function reduceWorker(prev: WorkerView, event: AgentEvent): WorkerView {
  // Latched terminal poses are sticky — ignore everything except a fresh start.
  if (prev.terminal && event.kind !== 'worker_started') {
    return prev;
  }

  switch (event.kind) {
    case 'worker_started':
      return { ...prev, pose: 'working', bubble: '坐下开工…', sparkle: false, terminal: false };

    case 'output_chunk':
      return { ...prev, pose: 'working', bubble: '思考 / 输出中…' };

    case 'tool_use': {
      const pose: Pose = isShellTool(event.tool) ? 'tense' : 'working';
      const verb = isShellTool(event.tool) ? '跑命令' : '调用工具';
      return { ...prev, pose, bubble: `${verb}：${event.tool}` };
    }

    case 'result':
      // pre-gate: result came back, hold tension until the Finished verdict.
      return { ...prev, pose: 'tense', bubble: `等待校验… 花费 ${costText(event.costUsd)}` };

    case 'error': {
      // v1: treat all errors as terminal sick (pause-path nuance is Phase 6+).
      return { ...prev, pose: 'sick', bubble: `出错：${event.code}`, terminal: true };
    }

    case 'finished': {
      if (event.status === 'verified' || event.status === 'Verified') {
        return {
          ...prev,
          pose: 'celebrate',
          bubble: `完成 ✓ 花费 ${costText(event.costUsd)}`,
          sparkle: true,
          terminal: true,
        };
      }
      const label = event.status === 'verifyFailed' || event.status === 'VerifyFailed' ? '校验未过' : '失败';
      return { ...prev, pose: 'sick', bubble: `${label}：${event.status}`, sparkle: false, terminal: true };
    }
  }
}
