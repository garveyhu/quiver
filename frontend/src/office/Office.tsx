import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';

import type { CompletionFx } from '@/hooks/useCompletionFx';
import { useManagerThinking } from '@/hooks/useManagerThinking';
import { useRoles } from '@/hooks/useRoles';
import { useTaskEvents } from '@/hooks/useTaskEvents';
import { useWorkers } from '@/hooks/useWorkers';
import { getStats } from '@/services/commands';
import { subscribe } from '@/services/ipc';
import { buildScene } from '@/office/buildScene';
import type { SceneNode } from '@/office/primitives';
import { useCamera } from '@/office/useCamera';
import { Worker } from '@/office/Worker';
import { WorldFx } from '@/office/WorldFx';
import { Worksurf } from '@/office/Worksurf';
import type { PlacedWorker } from '@/office/workers';

/** 节点内部内容:复合结构(猫) > 可扩展"＋" > 标签文字 > 空。 */
function nodeChildren(node: SceneNode): ReactNode {
  if (node.composite === 'cat') {
    return (
      <>
        <div className="t" />
        <div className="b">
          <div className="e1" />
          <div className="e2" />
        </div>
      </>
    );
  }
  if (node.mark) return <b>＋</b>;
  return node.text;
}

/** 办公室建筑(房间)→ 点击打开的面板:把楼变成入口,点哪栋开哪个台 —— 减少对右上角图标的依赖,
 *  更沉浸。空 key(可扩展位无映射时)不可点。 */
const ROOM_PANEL: Record<string, string> = {
  ops: 'settings', // 运维·预算 → 系统设置(调预算/模式)
  dispatch: 'manager', // 领导区·经理 → 经理工作台(决策流/思考)
  work: 'board', // 工位区 → 调度台(队列/优先级)
  verify: 'trace', // 质检台 → 追溯室(每步档案)
  ship: 'report', // 发货口 → 晨报(交付成果)
  lounge: 'personnel', // 休息室 → 人事部(员工/专长)
  build: 'personnel', // 可扩展 → 人事部(雇人)
};

interface OfficeProps {
  /** 交付时刻特效(验收通过/打回),在世界内质检台/发货口播放 */
  completionFx: CompletionFx;
  /** 从员工工作台跳到追溯室看完整档案。 */
  onOpenTrace?: () => void;
  /** 点办公室建筑 → 打开对应面板(把楼变成功能入口)。 */
  onOpenPanel?: (panel: string) => void;
}

/** 等距像素办公室。静态场景 + 实时工人 + 滚轮缩放 + 点小人下钻 + 交付特效 + 点建筑开面板。 */
export function Office({ completionFx, onOpenTrace, onOpenPanel }: OfficeProps) {
  const scene = useMemo(() => buildScene(), []);
  const workers = useWorkers(scene.layout);
  const camera = useCamera(scene.layout.worldW, scene.layout.worldH);
  const [focused, setFocused] = useState<PlacedWorker | null>(null);
  const events = useTaskEvents(focused?.taskId ?? null);
  // 预算楼实时花费(今夜):订阅任务更新刷新 —— 让"运维·预算"楼带活数字,不只是个名字。
  const [spentDay, setSpentDay] = useState<number | null>(null);
  useEffect(() => {
    let alive = true;
    let un: (() => void) | undefined;
    const pull = () => void getStats().then(s => alive && setSpentDay(s.spentDay)).catch(() => {});
    pull();
    void subscribe('task-updated', pull).then(u => (alive ? (un = u) : u()));
    return () => {
      alive = false;
      un?.();
    };
  }, []);
  // 点开经理时订阅它的思考流(claude 决策的实时思路);点别人时不订阅,省开销。
  const managerThinking = useManagerThinking(focused?.role === 'mgr');
  // 角色配置:点小人时用 workerRole 关联专长(§14 专长分工可见)。常驻拉一次。
  const { roles } = useRoles(true);

  // 点小人只打开详情(Worksurf),**不强行缩放摄像头** —— 想看近景自己滚轮,别夺镜头。
  const dive = useCallback((w: PlacedWorker) => {
    setFocused(w);
  }, []);

  const closeDive = useCallback(() => {
    setFocused(null);
  }, []);

  // Esc 退出下钻(相机由 useCamera 自身的 Esc 复位,这里清掉 worksurf)。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setFocused(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <>
      <div className="scene-fit">
        <div className="world" style={camera.worldStyle}>
          {scene.nodes.map(node => (
            <div key={node.key} className={node.className} style={node.style}>
              {nodeChildren(node)}
            </div>
          ))}
          {workers.map(w => (
            <Worker key={w.id} worker={w} onDive={dive} />
          ))}
          <WorldFx layout={scene.layout} fx={completionFx} />
        </div>
      </div>
      {/* 房间标签:屏幕空间浮层(在 .world 外,不被 transform:scale 缩放)。用相机 project 跟着
          办公室走,但文字本身 1:1 渲染 → DPR=1 显示器也清晰,不再是纹理放大的糊字。 */}
      <div className="room-labels">
        {scene.roomLabels.map((l, i) => {
          const p = camera.project(l.wx, l.wy);
          const panel = ROOM_PANEL[l.key];
          // 预算楼带实时今夜花费,把"运维·预算"从死名字变成活状态。
          const text = l.key === 'ops' && spentDay != null ? `${l.text} · 今夜 $${spentDay.toFixed(2)}` : l.text;
          return (
            <div
              key={i}
              className={`roomlabel${panel ? ' clickable' : ''}`}
              style={{ left: p.x, top: p.y }}
              onClick={panel && onOpenPanel ? () => onOpenPanel(panel) : undefined}
            >
              {text}
            </div>
          );
        })}
      </div>
      <div className={`zoomhint${camera.zoomed || focused ? ' on' : ''}`}>滚轮缩放 · Esc / 双击 复位</div>
      {/* 详情弹出时:点框外任意处即收起,交互更顺手(不必非点「收起」按钮)。 */}
      {focused && <div className="ws-scrim" onClick={closeDive} />}
      <Worksurf worker={focused} events={events} roles={roles} managerThinking={managerThinking} onClose={closeDive} onOpenTrace={onOpenTrace} />
    </>
  );
}
