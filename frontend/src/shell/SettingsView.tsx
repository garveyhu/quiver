import type { Settings, SettingsPatch } from '@/services/wire';

interface SettingsViewProps {
  open: boolean;
  settings: Settings | null;
  onPatch: (p: SettingsPatch) => void;
  onClose: () => void;
}

/** 数字输入解析:空=清除(null),非法=不动(undefined)。 */
function num(raw: string): number | null | undefined {
  const t = raw.trim();
  if (t === '') return null;
  const n = Number(t);
  return Number.isFinite(n) && n >= 0 ? n : undefined;
}

/**
 * 系统设置(§11):全局配置一处管 —— 运行模式、模型、并发、预算、验证命令、演示节奏。
 * 改动立即生效(immediate-apply)。**运行模式**是这里最关键的:simulate 免费、real 走
 * 订阅 headless 额度,CEO 下目标按这个模式跑。
 */
export function SettingsView({ open, settings, onPatch, onClose }: SettingsViewProps) {
  const s = settings;
  return (
    <div className={`panel${open ? ' on' : ''}`}>
      <h2>系统设置</h2>
      <div className="sub">全局配置,改动立即生效。运行模式决定 CEO 下目标是免费模拟还是真 claude 干活。</div>
      <div className="body">
        {!s ? (
          <div className="rev">读取中…</div>
        ) : (
          <>
            <div className="kv">
              <b>运行模式</b>
              <span>
                <button
                  className={`pbtn${s.defaultMode === 'simulate' ? ' go' : ''}`}
                  type="button"
                  onClick={() => onPatch({ defaultMode: 'simulate' })}
                >
                  模拟 · 免费
                </button>
                <button
                  className={`pbtn${s.defaultMode === 'real' ? ' go' : ''}`}
                  type="button"
                  style={{ marginLeft: 6 }}
                  onClick={() => onPatch({ defaultMode: 'real' })}
                >
                  真实 · claude 干活
                </button>
                {s.defaultMode === 'real' && (
                  <span className="st-bad" style={{ marginLeft: 8 }}>
                    会烧 headless 额度,产物留分支(无沙箱前不自动合 main)
                  </span>
                )}
              </span>
            </div>
            <div className="kv">
              <b>模型</b>
              <span>
                <input
                  className="field"
                  defaultValue={s.model}
                  onBlur={e => {
                    const v = e.target.value.trim();
                    if (v && v !== s.model) onPatch({ model: v });
                  }}
                />
                <span className="st-meta" style={{ marginLeft: 8 }}>
                  如 sonnet / opus(real 员工默认用它,可在人事部按员工覆盖)
                </span>
              </span>
            </div>
            <div className="kv">
              <b>并发上限</b>
              <span>
                <input
                  className="field role-num"
                  defaultValue={s.maxWorkers}
                  onBlur={e => {
                    const v = num(e.target.value);
                    if (typeof v === 'number' && v >= 1 && v !== s.maxWorkers) onPatch({ maxWorkers: v });
                  }}
                />
                <span className="st-meta" style={{ marginLeft: 8 }}>
                  同时几个员工并行(实时生效)
                </span>
              </span>
            </div>
            <div className="kv">
              <b>夜预算 $</b>
              <span>
                <input
                  className="field role-num"
                  defaultValue={s.nightlyBudgetUsd ?? ''}
                  placeholder="不限"
                  onBlur={e => {
                    const v = num(e.target.value);
                    if (v !== undefined && v !== s.nightlyBudgetUsd) onPatch({ nightlyBudgetUsd: v });
                  }}
                />
                <span className="st-meta" style={{ marginLeft: 8 }}>
                  近 24h 花到这就暂停队列(经理也会配额感知收敛)
                </span>
              </span>
            </div>
            <div className="kv">
              <b>验证命令</b>
              <span>
                <input
                  className="field"
                  defaultValue={s.verifyCommand}
                  placeholder="如 cargo test(空=总通过)"
                  onBlur={e => {
                    const v = e.target.value;
                    if (v !== s.verifyCommand) onPatch({ verifyCommand: v });
                  }}
                />
                <span className="st-meta" style={{ marginLeft: 8 }}>
                  合并前在沙箱里跑,不过则拦下(§7 质量闸)
                </span>
              </span>
            </div>
          </>
        )}
      </div>
      <div className="foot">
        <button className="pbtn go" type="button" onClick={onClose}>
          进办公室
        </button>
      </div>
    </div>
  );
}
