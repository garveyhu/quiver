import { useManagerBrain } from '@/hooks/useManagerBrain';
import type { Settings, SettingsPatch } from '@/services/wire';

interface SettingsViewProps {
  open: boolean;
  settings: Settings | null;
  onPatch: (p: SettingsPatch) => void;
  /** 切经理大脑失败时的反馈(写操作绝不静默吞、假报成功)。 */
  onToast: (msg: string) => void;
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
export function SettingsView({ open, settings, onPatch, onToast, onClose }: SettingsViewProps) {
  const s = settings;
  const { brain, setBrain } = useManagerBrain();
  // 切经理大脑是写操作,失败如实反馈(绝不 void 吞掉、假报成功)。
  const switchBrain = (b: 'rule' | 'claude') =>
    void setBrain(b).catch(e => onToast(`切经理大脑失败:${e instanceof Error ? e.message : String(e)}`));
  // 「聪明的真实自治」三件套:自治开(经理接管) + 经理大脑 claude(聪明决策) + 员工真干。
  // 缺任一,经理就不会真正决策(尤其自治关时派活直接走流水线,经理工作台是空的)。
  const autonomousOn = !!s?.autonomous;
  const fullyReal = autonomousOn && s?.defaultMode === 'real' && brain === 'claude';
  // claude 经理 + 模拟 = 免费预演:经理走真决策路径(spawn 进程/parse 决策 JSON),桩决策预览
  // 完整流程,不烧钱(上轮接通)。一键设齐 simulate+claude+自治,用户不烧钱就能看 claude 经理工作。
  const previewMgr = brain === 'claude' && s?.defaultMode === 'simulate';
  const startPreview = () => {
    // 一键预演不只切大脑:还顺手给个示例自治目标(CEO 没设过的话)。否则经理 autonomous 了却没活、
    // 没目标,只会观望 —— CEO 点完预演盯着空办公室、看不到经理工作。有了目标,经理立刻主动 plan
    // 拆活,决策流就滚起来,这才看得到「给方向就自己干」。CEO 设过目标则尊重、不覆盖。
    onPatch({
      defaultMode: 'simulate',
      autonomous: true,
      autonomousGoal: s?.autonomousGoal?.trim() || '持续完善这个项目的测试与文档',
    });
    switchBrain('claude');
  };
  return (
    <div className={`panel${open ? ' on' : ''}`}>
      <h2>系统设置</h2>
      <div className="sub">全局配置,改动立即生效。「聪明的自治」=自治接管 + 经理 claude 决策 + 员工真干活,三个开关都在这。</div>
      <div className="body">
        {!s ? (
          <div className="rev">读取中…</div>
        ) : (
          <>
            {fullyReal ? (
              <div className="rev" style={{ marginBottom: 8 }}>
                <span className="st-ok">完整真实自治已就绪</span>
                <span className="st-meta">经理用 claude 决策、员工真 claude 干活 —— 会持续消耗订阅额度。</span>
              </div>
            ) : previewMgr && autonomousOn ? (
              <div className="rev" style={{ marginBottom: 8 }}>
                <span className="st-ok">claude 经理预演中 · 免费</span>
                <span className="st-meta">
                  经理走真决策路径(spawn 进程 / parse 决策 JSON),桩决策预览完整流程,不烧钱。要真智能,把「员工干活」切「真实」。
                </span>
              </div>
            ) : (
              <div className="rev" style={{ marginBottom: 8 }}>
                <span className="st-meta">没把握就先免费看 claude 经理怎么工作:</span>
                <button className="pbtn go" type="button" style={{ marginLeft: 8 }} onClick={startPreview}>
                  一键预演 claude 经理(免费)
                </button>
                {(s.defaultMode === 'real' || brain === 'claude') && !autonomousOn && (
                  <span className="st-bad" style={{ marginLeft: 8 }}>自治没开,经理不会决策</span>
                )}
              </div>
            )}
            <div className="set-sec">聪明自治 · 给个方向就自己干</div>
            <div className="kv">
              <b>自治运转</b>
              <span>
                <button
                  className={`pbtn${autonomousOn ? ' go' : ''}`}
                  type="button"
                  onClick={() => onPatch({ autonomous: true })}
                >
                  开 · 经理接管调度
                </button>
                <button
                  className={`pbtn${!autonomousOn ? ' go' : ''}`}
                  type="button"
                  style={{ marginLeft: 6 }}
                  onClick={() => onPatch({ autonomous: false })}
                >
                  关 · 直接流水线
                </button>
                <span className="st-meta" style={{ marginLeft: 8 }}>
                  开了经理才逐拍决策(派活/复核/交付),决策流才有内容
                </span>
              </span>
            </div>
            {autonomousOn && (
              <div className="kv">
                <b>自治目标</b>
                <span>
                  <input
                    className="field"
                    style={{ minWidth: 300 }}
                    placeholder="给经理一个高层方向,如「持续提升测试覆盖率」"
                    defaultValue={s.autonomousGoal ?? ''}
                    onBlur={e => onPatch({ autonomousGoal: e.target.value.trim() })}
                  />
                  <span className="st-meta" style={{ marginLeft: 8 }}>
                    设了它,经理派完手头的活就**主动规划推进目标**(真「绝对自治」);留空=被动等你派活
                  </span>
                </span>
              </div>
            )}
            <div className="kv">
              <b>员工干活</b>
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
                    会烧 headless 额度,产物留分支
                  </span>
                )}
              </span>
            </div>
            <div className="kv">
              <b>经理大脑</b>
              <span>
                <button
                  className={`pbtn${brain === 'rule' ? ' go' : ''}`}
                  type="button"
                  onClick={() => switchBrain('rule')}
                >
                  规则 · 免费
                </button>
                <button
                  className={`pbtn${brain === 'claude' ? ' go' : ''}`}
                  type="button"
                  style={{ marginLeft: 6 }}
                  onClick={() => switchBrain('claude')}
                >
                  claude · 真思考
                </button>
                {brain === 'claude' ? (
                  <span className="st-bad" style={{ marginLeft: 8 }}>
                    经理每拍决策烧 claude 额度
                  </span>
                ) : (
                  <span className="st-meta" style={{ marginLeft: 8 }}>
                    要「聪明」地拆活/按专长派人就切 claude;只看免费演示留规则
                  </span>
                )}
              </span>
            </div>
            <div className="set-sec">跑量与质量 · 花多少、跑多快、卡多严</div>
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
