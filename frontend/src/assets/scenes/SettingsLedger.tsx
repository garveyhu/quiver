import { useState } from 'react';
import type { CSSProperties } from 'react';
import { Panel, TabBar, FieldRow, Toggle, Select, Slider, TextField, SegmentedControl, Button } from '@/assets/ui';

export interface SettingsLedgerProps {
  width?: number;
  className?: string;
  style?: CSSProperties;
}

/** 设置账本:整页拼装示例(面板 + 标签页 + 字段行 + 各表单控件)。 */
export function SettingsLedger({ width = 360, className, style }: SettingsLedgerProps) {
  const [tab, setTab] = useState(0);
  const [mode, setMode] = useState(0);
  const [overnight, setOvernight] = useState(true);
  const [sandbox, setSandbox] = useState(true);

  return (
    <Panel title="设置账本 ▤" width={width} className={className} style={style}>
      <TabBar tabs={['运行', '预算', '外观']} active={tab} onChange={setTab} />
      <div style={{ marginTop: 10 }}>
        <FieldRow label="运行模式" hint="模拟免费 · 真实消耗额度">
          <SegmentedControl options={['模拟', '真实']} value={mode} onChange={setMode} />
        </FieldRow>
        <FieldRow label="过夜运行" hint="跑完留分支,不并 main">
          <Toggle on={overnight} onChange={setOvernight} />
        </FieldRow>
        <FieldRow label="沙箱" hint="Seatbelt 隔离">
          <Toggle on={sandbox} onChange={setSandbox} />
        </FieldRow>
        <FieldRow label="并发工位">
          <Slider defaultValue={3} min={1} max={8} />
        </FieldRow>
        <FieldRow label="默认智能体">
          <Select options={['Claude CLI', 'Codex CLI']} />
        </FieldRow>
        <FieldRow label="预算上限 ($)">
          <TextField defaultValue="100" style={{ width: 80 }} />
        </FieldRow>
      </div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 12 }}>
        <Button variant="ghost">合上账本</Button>
        <Button>已记录</Button>
      </div>
    </Panel>
  );
}
