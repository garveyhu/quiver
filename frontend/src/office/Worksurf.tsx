import type { PlacedWorker } from '@/office/workers';

interface WorksurfProps {
  worker: PlacedWorker | null;
  onClose: () => void;
}

function title(w: PlacedWorker): string {
  if (w.role === 'mgr') return '经理 · 领导区';
  if (w.role === 'aud') return '独立审计 · 质检台';
  const n = Number(w.id.replace('emp', '')) + 1;
  return `员工 #${n}`;
}

function state(w: PlacedWorker): string {
  if (w.role === 'mgr') return '在领导区盯着公司 —— 读记忆、决策、派活。';
  if (w.role === 'aud') return '在质检台待命 —— 成果交来就在干净环境重验、查测试有没有被改弱。';
  if (w.working) return `在独立工作区敲键:${w.label ?? '任务'}。再深钻一层看真实轨迹(轨迹/diff 接入中)。`;
  if (w.awaiting) return '等你拍板 —— 经理已复核,等下一步指令。';
  return '休息室待命 —— 没活时在这儿歇着。';
}

/**
 * 工作台 worksurf(移植原型 dive 落点):点小人 → 相机飞入 + 这张面板下钻到它的此刻状态。
 * 本刀展示身份/状态/当前活动;再深一层的真实轨迹(get_task_events 的事件流/diff)留后续刀。
 */
export function Worksurf({ worker, onClose }: WorksurfProps) {
  return (
    <div className={`worksurf${worker ? ' on' : ''}`}>
      {worker && (
        <>
          <div className="wbar">
            <span className="dot3">
              <i />
              <i />
              <i />
            </span>
            {title(worker)}
          </div>
          <div className="wmeta">{state(worker)}</div>
          <div className="wfoot">
            <button className="pbtn go" type="button" onClick={onClose}>
              收起 ▾
            </button>
          </div>
        </>
      )}
    </div>
  );
}
