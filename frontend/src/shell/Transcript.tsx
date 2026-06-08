import { useState } from 'react';
import type { CSSProperties } from 'react';
import { SpeechBubble, cssVar, PALETTE } from '@/assets';
import type { AgentEvent } from '@/types/agentEvent.types';

type Block =
  | { type: 'output'; text: string }
  | { type: 'tool'; tool: string; summary: string }
  | { type: 'status'; text: string; color: string }
  | { type: 'verify'; text: string };

/** 把事件流折叠成可读对话块:连续 output_chunk 拼成一段、tool_use 成卡片、其余成状态行。 */
function group(events: AgentEvent[]): Block[] {
  const blocks: Block[] = [];
  let buf = '';
  const flush = () => {
    if (buf.trim()) blocks.push({ type: 'output', text: buf });
    buf = '';
  };
  for (const e of events) {
    if (e.kind === 'output_chunk') {
      buf += e.text;
      continue;
    }
    flush();
    if (e.kind === 'tool_use') blocks.push({ type: 'tool', tool: e.tool, summary: e.summary });
    else if (e.kind === 'worker_started')
      blocks.push({ type: 'status', text: `工人就位${e.model ? ` · ${e.model}` : ''}`, color: PALETTE.amber });
    else if (e.kind === 'result') {
      const parts = [`结果 · ${e.numTurns} 回合`];
      if (e.costUsd != null) parts.push(`$${e.costUsd.toFixed(2)}`);
      if (e.tokens != null) parts.push(`${e.tokens} tok`);
      if (e.durationMs != null) parts.push(`${(e.durationMs / 1000).toFixed(1)}s`);
      blocks.push({ type: 'status', text: parts.join(' · '), color: PALETTE.amber });
    }
    else if (e.kind === 'error') blocks.push({ type: 'status', text: `✕ ${e.code} — ${e.message}`, color: PALETTE.red });
    else if (e.kind === 'finished') {
      const ok = e.status.toLowerCase() === 'verified';
      blocks.push({ type: 'status', text: `完成 · ${e.status}`, color: ok ? PALETTE.green : PALETTE.red });
      if (e.verifyOutput && e.verifyOutput.trim()) {
        blocks.push({ type: 'verify', text: e.verifyOutput });
      }
    }
  }
  flush();
  return blocks;
}

export interface TranscriptProps {
  events: AgentEvent[];
  prompt?: string;
  mode?: string;
  branch?: string | null;
  style?: CSSProperties;
}

/** 与 Claude 的对话/转写视图:你的委托在右,智能体的输出/工具/状态在左,像聊天。 */
export function Transcript({ events, prompt, mode, branch, style }: TranscriptProps) {
  const blocks = group(events);
  const outputText = blocks.reduce((s, b) => (b.type === 'output' ? (s ? s + '\n\n' : '') + b.text : s), '');
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard?.writeText(outputText);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8, padding: 10, ...style }}>
      {outputText && (
        <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
          <span
            role="button"
            title="复制智能体输出"
            onClick={copy}
            style={{ cursor: 'pointer', fontSize: 9, color: copied ? cssVar('green') : cssVar('dim'), boxShadow: 'inset 0 0 0 1px ' + (copied ? PALETTE.green : PALETTE.wall), padding: '2px 6px' }}
          >
            {copied ? '已复制 ✓' : '复制输出 ⧉'}
          </span>
        </div>
      )}
      {prompt && (
        <div style={{ alignSelf: 'flex-end', maxWidth: '88%' }}>
          <div style={{ font: '9px ui-monospace,monospace', color: cssVar('dim'), textAlign: 'right', marginBottom: 2 }}>你的委托</div>
          <div
            style={{
              background: cssVar('amber'),
              color: cssVar('ink'),
              font: '11px/1.4 ui-monospace,monospace',
              padding: '6px 9px',
              boxShadow: '2px 2px 0 #00000055',
            }}
          >
            {prompt}
          </div>
          {(mode || branch) && (
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6, marginTop: 3, font: '9px ui-monospace,monospace', color: cssVar('dim') }}>
              {mode && <span style={{ color: mode === 'real' ? cssVar('red') : cssVar('green') }}>{mode === 'real' ? '真实' : '模拟'}</span>}
              {branch && <span>· 分支 {branch}</span>}
            </div>
          )}
        </div>
      )}
      {blocks.length === 0 && <div style={{ color: cssVar('dim'), fontSize: 11 }}>智能体还没开口…</div>}
      {blocks.map((b, i) => {
        if (b.type === 'output') {
          return (
            <div key={i} style={{ alignSelf: 'flex-start', maxWidth: '92%' }}>
              <SpeechBubble dark>
                <span style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>{b.text.slice(0, 4000)}</span>
              </SpeechBubble>
            </div>
          );
        }
        if (b.type === 'tool') {
          return (
            <div
              key={i}
              style={{
                alignSelf: 'flex-start',
                display: 'flex',
                gap: 6,
                alignItems: 'baseline',
                background: cssVar('night'),
                boxShadow: 'inset 0 0 0 2px ' + PALETTE.wall,
                padding: '5px 8px',
                font: '10px ui-monospace,monospace',
                maxWidth: '92%',
              }}
            >
              <span style={{ color: cssVar('blue') }}>✎ {b.tool}</span>
              <span style={{ color: cssVar('dim'), overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {b.summary}
              </span>
            </div>
          );
        }
        if (b.type === 'verify') {
          return (
            <div
              key={i}
              style={{
                alignSelf: 'stretch',
                background: cssVar('night'),
                boxShadow: 'inset 0 0 0 2px ' + PALETTE.red,
                padding: '6px 8px',
              }}
            >
              <div style={{ font: 'bold 9px ui-monospace,monospace', color: PALETTE.red, marginBottom: 4 }}>
                验证未过 · 命令输出
              </div>
              <pre
                style={{
                  margin: 0,
                  font: '10px/1.4 ui-monospace,monospace',
                  color: cssVar('cream'),
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  maxHeight: 140,
                  overflow: 'auto',
                }}
              >
                {b.text}
              </pre>
            </div>
          );
        }
        return (
          <div key={i} style={{ alignSelf: 'center', font: '10px ui-monospace,monospace', color: b.color }}>
            — {b.text} —
          </div>
        );
      })}
    </div>
  );
}
