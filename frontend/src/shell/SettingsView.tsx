import { Panel, FieldRow, SegmentedControl, Slider, Select, TextField, Button, Divider, PALETTE } from '@/assets';
import { useSettingsContext } from './SettingsContext';
import { useEnvironmentCheck } from '@/hooks/useEnvironmentCheck';
import { useVerifySuggestion } from '@/hooks/useVerifySuggestion';

const SAVE_LABEL: Record<string, string> = { saving: '记录中…', saved: '已记录', error: '记录失败', idle: '' };
const CHECK_COLOR: Record<string, string> = { ok: PALETTE.green, warn: PALETTE.amber, fail: PALETTE.red };
const CHECK_GLYPH: Record<string, string> = { ok: '✓', warn: '◔', fail: '✕' };

/** 设置账本(接 useSettings,乐观更新 + 防抖持久化)。 */
export function SettingsView() {
  const { settings, saveStatus, patch } = useSettingsContext();
  const health = useEnvironmentCheck();
  const verifySuggestion = useVerifySuggestion();

  if (!settings) {
    return (
      <Panel title="设置账本 ▤" width={440}>
        <div style={{ color: PALETTE.dim, fontSize: 11, padding: 8 }}>加载中…</div>
      </Panel>
    );
  }

  return (
    <Panel title={`设置账本 ▤   ${SAVE_LABEL[saveStatus] ?? ''}`} width={440}>
      <FieldRow label="默认模式" hint="模拟免费 · 真实消耗额度">
        <SegmentedControl
          options={['模拟', '真实']}
          value={settings.defaultMode === 'real' ? 1 : 0}
          onChange={(i) => patch({ defaultMode: i === 1 ? 'real' : 'simulate' })}
        />
      </FieldRow>
      <FieldRow label="并发工位">
        <Slider min={1} max={8} value={settings.maxWorkers} onChange={(e) => patch({ maxWorkers: Number(e.target.value) })} />
      </FieldRow>
      <FieldRow label="模型">
        <TextField value={settings.model} onChange={(e) => patch({ model: e.target.value })} style={{ width: 170 }} />
      </FieldRow>
      <FieldRow label="验证命令" hint="合并前在 worktree 跑(sh -c),不过则 VerifyFailed;空=不校验。例:cargo test · npm test · pytest · go test ./...">
        <TextField
          value={settings.verifyCommand}
          placeholder={verifySuggestion ? `${verifySuggestion}  (检测到,留空=不校验)` : '(空=不校验)'}
          onChange={(e) => patch({ verifyCommand: e.target.value })}
          style={{ width: 220 }}
        />
      </FieldRow>
      <FieldRow label="月度额度上限 $" hint="预算治理:到顶闭店">
        <TextField
          value={settings.monthlyCreditCapUsd ?? ''}
          onChange={(e) => patch({ monthlyCreditCapUsd: e.target.value ? Number(e.target.value) : null })}
          style={{ width: 90 }}
        />
      </FieldRow>
      <FieldRow label="过夜预算 $">
        <TextField
          value={settings.nightlyBudgetUsd ?? ''}
          onChange={(e) => patch({ nightlyBudgetUsd: e.target.value ? Number(e.target.value) : null })}
          style={{ width: 90 }}
        />
      </FieldRow>
      <FieldRow label="模拟延迟 ms">
        <TextField value={settings.fakeDelayMs} onChange={(e) => patch({ fakeDelayMs: Number(e.target.value) })} style={{ width: 90 }} />
      </FieldRow>
      <FieldRow label="主题">
        <Select options={['cozy', 'midnight']} value={settings.theme} onChange={(e) => patch({ theme: e.target.value })} />
      </FieldRow>

      <Divider />
      <div style={{ display: 'flex', alignItems: 'center', marginBottom: 6 }}>
        <span style={{ flex: 1, font: 'bold 11px ui-monospace,monospace', color: PALETTE.amber }}>工坊体检</span>
        <Button variant="ghost" onClick={() => void health.refresh()}>
          {health.loading ? '检查中…' : '重新检查'}
        </Button>
      </div>
      {health.checks.map((c) => (
        <div key={c.id} style={{ display: 'flex', alignItems: 'flex-start', gap: 8, padding: '4px 0' }}>
          <span style={{ color: CHECK_COLOR[c.status], width: 14, textAlign: 'center' }}>{CHECK_GLYPH[c.status]}</span>
          <span style={{ flex: 1, font: '11px ui-monospace,monospace', color: PALETTE.cream }}>
            {c.label}
            <span style={{ color: PALETTE.dim, marginLeft: 6 }}>{c.message}</span>
            {c.remediation && c.status !== 'ok' && (
              <div style={{ color: PALETTE.dim2, fontSize: 10, marginTop: 2 }}>{c.remediation}</div>
            )}
          </span>
        </div>
      ))}
    </Panel>
  );
}
