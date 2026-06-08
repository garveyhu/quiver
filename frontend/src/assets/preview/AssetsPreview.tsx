import { useState } from 'react';
import type { CSSProperties, ReactNode } from 'react';
import {
  PALETTE,
  cssVar,
  StudioRoom,
  EmptyState,
  Worker,
  Desk,
  StudioWindow,
  CoffeeStation,
  Bookshelf,
  Plant,
  FloorLamp,
  WallClock,
  Rug,
  Cat,
  Armchair,
  Glow,
  Bubble,
  MenuBar,
  Panel,
  Button,
  SpeechBubble,
  StatusBubble,
  CountdownBubble,
  BudgetBar,
  XpBar,
  StatusTag,
  CostTag,
  Icon,
  Toast,
  Tooltip,
  Spinner,
  Dialog,
  TabBar,
  Toggle,
  Checkbox,
  TextField,
  Select,
  Slider,
  ProjectCard,
  EventRow,
  WorkerAvatar,
  NavRail,
  ProgressRing,
  ProgressBar,
  Achievement,
  XpFloat,
  SegmentedControl,
  Kbd,
  Divider,
  Banner,
  StatTile,
  NotificationCenter,
  ShortcutsPanel,
  Minimap,
  Cursor,
  SoundToggle,
  SettingsLedger,
} from '@/assets';
import type { WorkerState } from '@/assets';

const PAGE: CSSProperties = {
  minHeight: '100vh',
  background: '#0c0e14',
  color: '#e6e9f0',
  fontFamily: 'ui-monospace,Menlo,monospace',
  padding: '36px 28px 80px',
};
const SEC_T: CSSProperties = {
  fontSize: 13,
  letterSpacing: 2,
  color: PALETTE.amber,
  borderBottom: '2px solid #272d3a',
  padding: '6px 0',
  margin: '26px 0 14px',
};
const ROW: CSSProperties = { display: 'flex', flexWrap: 'wrap', gap: 14, alignItems: 'flex-end' };
const CAP: CSSProperties = { fontSize: 11, color: '#8b94a6', marginTop: 6, textAlign: 'center' };

function Stage({ children, w = 104, h = 104 }: { children: ReactNode; w?: number; h?: number }) {
  return (
    <div
      style={{
        width: w,
        height: h,
        position: 'relative',
        overflow: 'hidden',
        background: PALETTE.floor,
        border: '1px solid #0007',
      }}
    >
      {children}
    </div>
  );
}

function Tile({ children, cap }: { children: ReactNode; cap: string }) {
  return (
    <div style={{ background: '#13161f', border: '2px solid #272d3a', padding: 8 }}>
      {children}
      <div style={CAP}>{cap}</div>
    </div>
  );
}

const STATES: { s: WorkerState; t: string }[] = [
  { s: 'idle', t: '待命 idle' },
  { s: 'work', t: '干活 work' },
  { s: 'coffee', t: '卡住 coffee' },
  { s: 'sweat', t: '紧张 sweat' },
  { s: 'cel', t: '庆祝 celebrate' },
  { s: 'sick', t: '崩了 sick' },
];

const VARIANTS = [
  { body: cssVar('pink'), hair: '#5a3a4a', headphone: false, t: 'char0 粉·呆毛' },
  { body: cssVar('blue'), hair: '#3a3550', headphone: true, t: 'char1 蓝·耳机' },
  { body: cssVar('green'), hair: '#3a3550', headphone: true, t: 'char2 绿·耳机' },
  { body: cssVar('amber2'), hair: '#caa', headphone: false, t: 'char3 橙·呆毛' },
];

/** DEV 预览画廊:在真 React 应用里渲染全套像素组件。用 `?preview=assets` 打开。 */
export function AssetsPreview() {
  const [toggleOn, setToggleOn] = useState(true);
  const [checked, setChecked] = useState(false);
  const [tab, setTab] = useState(0);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [seg, setSeg] = useState(0);
  const [sound, setSound] = useState(true);
  return (
    <div style={PAGE}>
      <h1 style={{ fontSize: 22 }}>
        <span style={{ color: PALETTE.amber }}>Quiver</span> · 像素组件预览 <span style={{ color: '#8b94a6', fontSize: 12 }}>frontend/src/assets</span>
      </h1>

      <div style={SEC_T}>① 主大厅 StudioRoom</div>
      <div style={{ display: 'inline-block', border: '2px solid #272d3a' }}>
        <StudioRoom />
      </div>

      <div style={SEC_T}>② 角色 6 状态(以耳机小工为例)</div>
      <div style={ROW}>
        {STATES.map(({ s, t }) => (
          <Tile key={s} cap={t}>
            <Stage>
              <div style={{ position: 'absolute', left: 35, bottom: 22 }}>
                <Worker state={s} body={cssVar('blue')} hair="#3a3550" headphone />
              </div>
            </Stage>
          </Tile>
        ))}
      </div>

      <div style={SEC_T}>③ 4 工人变体</div>
      <div style={ROW}>
        {VARIANTS.map((v) => (
          <Tile key={v.t} cap={v.t}>
            <Stage>
              <div style={{ position: 'absolute', left: 35, bottom: 22 }}>
                <Worker state="idle" body={v.body} hair={v.hair} headphone={v.headphone} />
              </div>
            </Stage>
          </Tile>
        ))}
      </div>

      <div style={SEC_T}>④ 道具 props</div>
      <div style={ROW}>
        <Tile cap="工位 Desk"><Stage><Desk style={{ left: 0, bottom: 18 }} /></Stage></Tile>
        <Tile cap="咖啡角 CoffeeStation"><Stage w={120} h={120}><CoffeeStation style={{ left: 18, bottom: 12 }} /></Stage></Tile>
        <Tile cap="书堆 Bookshelf"><Stage><Bookshelf style={{ left: 8, top: 40 }} /></Stage></Tile>
        <Tile cap="绿植 Plant"><Stage><Plant style={{ left: 34, bottom: 18 }} /></Stage></Tile>
        <Tile cap="落地灯 FloorLamp"><Stage h={120}><FloorLamp style={{ left: 34, bottom: 10 }} /></Stage></Tile>
        <Tile cap="挂钟 WallClock"><Stage><WallClock style={{ left: 34, top: 34 }} /></Stage></Tile>
        <Tile cap="扶手椅+猫"><Stage><Armchair style={{ left: 22, bottom: 14 }} /><Cat style={{ left: 38, bottom: 46 }} /></Stage></Tile>
        <Tile cap="地毯 Rug"><Stage><Rug style={{ left: 8, bottom: 30, width: 88, height: 40 }} /></Stage></Tile>
      </div>

      <div style={SEC_T}>⑤ 窗 + 特效</div>
      <div style={ROW}>
        <Tile cap="夜窗 StudioWindow"><Stage w={250} h={200} ><StudioWindow style={{ left: 8, top: 8 }} /></Stage></Tile>
        <Tile cap="暖光晕 Glow"><Stage><Glow style={{ left: 30, bottom: 36, width: 44, height: 30 }} /></Stage></Tile>
        <Tile cap="!气泡 Bubble"><Stage><Bubble style={{ left: 34, top: 40 }} /></Stage></Tile>
      </div>

      <div style={SEC_T}>⑥ UI 套件(菜单 / 面板 / 按钮 / 气泡 / HUD / 图标)</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 20, alignItems: 'flex-start' }}>
        <div>
          <MenuBar
            items={[{ label: '看板', active: true }, { label: '档案' }, { label: '设置' }]}
            right={<BudgetBar spent={62} total={100} width={120} />}
          />
          <div style={{ height: 10 }} />
          <Panel title="工位详情 ▤" onClose={() => undefined} width={230}>
            <div style={{ fontSize: 11, color: '#8b94a6', display: 'flex', alignItems: 'center', gap: 6 }}>
              archer-1 <StatusTag kind="running" />
            </div>
            <div style={{ marginTop: 10, display: 'flex', gap: 6, alignItems: 'center' }}>
              <CostTag usd={0.42} />
              <Button>运行</Button>
              <Button variant="ghost">设置</Button>
            </div>
          </Panel>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div style={{ position: 'relative', width: 250, height: 120, background: PALETTE.floor, border: '1px solid #0007' }}>
            <div style={{ position: 'absolute', left: 14, top: 12 }}>
              <SpeechBubble>读 README,用一句话总结这个项目…</SpeechBubble>
            </div>
            <div style={{ position: 'absolute', left: 14, top: 66 }}>
              <SpeechBubble dark>思考中:先看目录结构</SpeechBubble>
            </div>
          </div>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <StatusBubble kind="await" />
            <StatusBubble kind="verified" />
            <StatusBubble kind="failed" />
            <StatusBubble kind="retry" />
            <StatusBubble kind="budget" />
            <CountdownBubble time="4:32" />
          </div>
          <XpBar xp={120} max={200} level={7} />
          <div style={{ display: 'flex', gap: 10, color: PALETTE.amber }}>
            {(['gear', 'play', 'pause', 'check', 'close', 'retry', 'search', 'star'] as const).map((n) => (
              <Icon key={n} name={n} size={16} />
            ))}
          </div>
        </div>
      </div>

      <div style={SEC_T}>⑦ 反馈 / 弹层 / 表单 / 卡片 / 空状态</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 20, alignItems: 'flex-start' }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <Toast kind="success">任务已验证并合并入 main</Toast>
          <Toast kind="error">验证失败:3 个测试未通过</Toast>
          <Toast kind="info">预算已用 62%,夜里会自动暂停</Toast>
          <div style={{ display: 'flex', gap: 14, alignItems: 'center', marginTop: 4 }}>
            <Tooltip label="悬浮提示在这里">
              <Button variant="ghost">悬停看 Tooltip</Button>
            </Tooltip>
            <span style={{ color: PALETTE.dim, fontSize: 11 }}>加载</span>
            <Spinner />
          </div>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <TabBar tabs={['看板', '档案', '设置']} active={tab} onChange={setTab} />
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, color: PALETTE.dim, fontSize: 11 }}>
            <span>模拟运行</span>
            <Toggle on={toggleOn} onChange={setToggleOn} />
            <Checkbox on={checked} onChange={setChecked} />
            <span>过夜跑</span>
          </div>
          <TextField placeholder="读 README,用一句话总结…" style={{ width: 220 }} />
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <Select options={['Z-Image', 'FLUX', 'SDXL']} />
            <Slider defaultValue={62} />
          </div>
          <Button onClick={() => setDialogOpen(true)}>打开弹窗</Button>
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <ProjectCard name="quiver" path="/Users/links/Coding/Archer/quiver" meta="最近运行 · 3 任务 · $1.24" />
          <div style={{ width: 230, background: PALETTE.night2, boxShadow: '0 0 0 2px ' + PALETTE.wall }}>
            <EventRow glyph="▶" text="WorkerStarted" time="22:01" />
            <EventRow glyph="✎" text="编辑 src/lib.rs" time="22:03" />
            <EventRow glyph="$" color={PALETTE.amber} text="本次花费" cost={0.42} time="22:05" />
            <EventRow glyph="✓" color={PALETTE.green} text="验证通过 → 合并" time="22:06" />
          </div>
        </div>

        <EmptyState />
      </div>

      <Dialog
        open={dialogOpen}
        title="确认运行 ▶"
        onClose={() => setDialogOpen(false)}
        actions={
          <>
            <Button variant="ghost" onClick={() => setDialogOpen(false)}>取消</Button>
            <Button onClick={() => setDialogOpen(false)}>开工</Button>
          </>
        }
      >
        <div style={{ fontSize: 11, color: PALETTE.dim }}>真实模式会消耗你的月度额度,产物只留在分支。</div>
      </Dialog>

      <div style={SEC_T}>⑧ 导航 / 进度 / 成就 / HUD2</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 20, alignItems: 'flex-start' }}>
        <NavRail
          items={[
            { icon: 'doc', active: true, title: '看板' },
            { icon: 'book', title: '档案' },
            { icon: 'folder', title: '项目' },
            { icon: 'gear', title: '设置' },
          ]}
        />
        <div style={{ display: 'flex', gap: 16, alignItems: 'center' }}>
          <ProgressRing pct={68} />
          <ProgressRing pct={42} size={56} color={PALETTE.green} label="✓" />
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            <ProgressBar pct={68} />
            <ProgressBar pct={88} color={PALETTE.red} />
          </div>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <Achievement title="升级! Lv 8" sub="完成 12 个任务" />
          <div style={{ display: 'flex', gap: 14, alignItems: 'center' }}>
            <XpFloat amount="+10 XP" />
            <XpFloat amount="+$0.42" color={PALETTE.green} />
          </div>
          <SegmentedControl options={['模拟运行', '真实 Claude']} value={seg} onChange={setSeg} />
          <div style={{ display: 'flex', gap: 6, alignItems: 'center', color: PALETTE.dim, fontSize: 11 }}>
            新建委托 <Kbd>⌘</Kbd> <Kbd>N</Kbd>
          </div>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <div style={{ display: 'flex', gap: 8 }}>
            <StatTile value="3" label="运行中" />
            <StatTile value="$1.24" label="今夜花费" color={PALETTE.green} />
            <StatTile value="Lv8" label="工坊等级" />
          </div>
          <div style={{ display: 'flex', gap: 10 }}>
            <WorkerAvatar body={PALETTE.pink} hair="#5a3a4a" />
            <WorkerAvatar state="cel" body={PALETTE.blue} headphone />
            <WorkerAvatar state="sick" body={PALETTE.green} headphone />
          </div>
        </div>
      </div>
      <div style={{ marginTop: 14 }}>
        <Banner kind="warn" action={<Button variant="ghost">提高上限</Button>}>
          预算到顶 — 队列已暂停,直到你提高上限 / 开启溢出 / 月度额度刷新
        </Banner>
      </div>
      <Divider />

      <div style={SEC_T}>⑨ 通知中心 / 快捷键 / 迷你地图 / 设置账本 / 光标</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 20, alignItems: 'flex-start' }}>
        <NotificationCenter
          items={[
            { kind: 'success', text: 'archer-1 已合并', time: '22:06' },
            { kind: 'error', text: 'archer-3 验证失败', time: '22:11' },
            { kind: 'info', text: '预算已用 62%', time: '22:14' },
          ]}
        />
        <ShortcutsPanel
          items={[
            { label: '新建委托', keys: ['⌘', 'N'] },
            { label: '打开档案', keys: ['⌘', 'L'] },
            { label: '设置账本', keys: ['⌘', ','] },
            { label: '暂停队列', keys: ['Space'] },
          ]}
        />
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          <div style={{ color: PALETTE.dim, fontSize: 11 }}>迷你地图(按状态着色)</div>
          <Minimap
            dots={[
              { x: 14, y: 60, state: 'work' },
              { x: 46, y: 55, state: 'coffee' },
              { x: 78, y: 62, state: 'sweat' },
              { x: 60, y: 30, state: 'cel' },
              { x: 30, y: 35, state: 'sick' },
            ]}
          />
          <div style={{ display: 'flex', gap: 12, alignItems: 'center', color: PALETTE.dim, fontSize: 11 }}>
            光标 <Cursor /> 音效 <SoundToggle on={sound} onChange={setSound} />
          </div>
        </div>
        <SettingsLedger />
      </div>

      <div style={SEC_T}>⑩ 调色板</div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 5 }}>
        {Object.entries(PALETTE).map(([k, v]) => (
          <div key={k} style={{ width: 60, textAlign: 'center' }}>
            <div style={{ height: 30, background: v, boxShadow: '1px 1px 0 #0006' }} />
            <div style={{ fontSize: 9, color: '#8b94a6' }}>{k}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
