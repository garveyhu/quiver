import { useEffect, useState } from 'react';

import { managerPreview } from '@/services/commands';
import type { Decision, ManagerPreview } from '@/services/wire';

interface BriefCardProps {
  open: boolean;
  defaultGoal: string;
  onClose: () => void;
  /** 用户点头 → 把(可能改过的)目标派给公司 */
  onConfirm: (goal: string) => void;
}

const ACTION_CN: Record<Decision['action'], string> = {
  spawn: '派员工开干 ▸',
  plan: '拆活分工 ⑃',
  continue: '续跑在途 worker',
  deliver: '推进交付',
  block: '拦下别交付',
  escalate: '升级给你拍板',
  refresh_memory: '先刷新记忆再定',
  noop: '此刻不新派(在途够多 / 预算紧)',
};

/**
 * 下目标 · Brief 卡(移植原型 openBrief 的握手):
 * step0 写一句目标 → step1 经理读此刻公司局面(接 manager_preview:在途/排队/预算/决策)→
 * "看着对 → 开始" 才入队花钱。体现「没看到这个之前,不花一分钱」。
 */
export function BriefCard({ open, defaultGoal, onClose, onConfirm }: BriefCardProps) {
  const [step, setStep] = useState(0);
  const [goal, setGoal] = useState(defaultGoal);
  const [preview, setPreview] = useState<ManagerPreview | null>(null);

  useEffect(() => {
    if (!open) return;
    setStep(0);
    setGoal(defaultGoal);
    setPreview(null);
  }, [open, defaultGoal]);

  const toManager = async () => {
    setStep(1);
    try {
      setPreview(await managerPreview());
    } catch {
      setPreview(null);
    }
  };

  return (
    <div className={`panel${open ? ' on' : ''}`}>
      {step === 0 ? (
        <>
          <h2>新任务 · 交给公司</h2>
          <div className="sub">写一个或多个目标（每行一个），经理会读一遍公司此刻的局面、报个能不能接，你点头才花钱。一次多行 = 公司持续消化一批活。</div>
          <div className="body">
            <textarea className="field" rows={3} value={goal} onChange={e => setGoal(e.target.value)} />
          </div>
          <div className="foot">
            <button className="pbtn" type="button" onClick={onClose}>
              取消
            </button>
            <button className="pbtn go" type="button" onClick={() => void toManager()} disabled={!goal.trim()}>
              交给经理
            </button>
          </div>
        </>
      ) : (
        <>
          <h2>经理读了读公司现状</h2>
          <div className="sub">没看到这个之前，不花一分钱、不派一个员工。</div>
          <div className="body">
            <div className="kv">
              <b>目标</b>
              <span>{goal}</span>
            </div>
            {preview ? (
              <>
                <div className="kv">
                  <b>在途</b>
                  <span>
                    {preview.inflight} / {preview.maxInflight} 个员工 · 排队 {preview.queued}
                  </span>
                </div>
                <div className="kv">
                  <b>预算</b>
                  <span>{preview.budgetRemainingUsd >= 1_000_000 ? '充裕（未设夜预算上限）' : `剩 $${preview.budgetRemainingUsd.toFixed(2)}`}</span>
                </div>
                <div className="kv">
                  <b>经理</b>
                  <span>{ACTION_CN[preview.decision.action] ?? preview.decision.action}</span>
                </div>
              </>
            ) : (
              <div className="kv">
                <b>经理</b>
                <span>读取中…</span>
              </div>
            )}
          </div>
          <div className="foot">
            <button className="pbtn" type="button" onClick={() => setStep(0)}>
              改任务
            </button>
            <button className="pbtn go" type="button" onClick={() => onConfirm(goal)}>
              看着对 → 开始
            </button>
          </div>
        </>
      )}
    </div>
  );
}
