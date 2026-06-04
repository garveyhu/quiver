import { STR } from '@/strings';

export type Scene = 'workshop' | 'board' | 'archive';

interface SceneTabsProps {
  scene: Scene;
  onChange: (scene: Scene) => void;
  /** Tabs with an unseen update — show the gently pulsing "有更新" badge. */
  alerts?: Set<Scene>;
}

const TABS: { id: Scene; label: string; tip: string }[] = [
  { id: 'workshop', label: STR.navWorkshop, tip: STR.navWorkshopTip },
  { id: 'board', label: STR.navBoard, tip: STR.navBoardTip },
  { id: 'archive', label: STR.navArchive, tip: STR.navArchiveTip },
];

/**
 * The top-level scene switcher — 工坊 / 公告板 / 档案 — styled as carved wooden
 * tabs. Pure presentation: it owns no scene state, just reflects `scene` +
 * `alerts` and reports clicks. The active panel mounts/unmounts in the parent so
 * live work keeps running regardless of which tab is shown. A tab in `alerts`
 * grows a pulsing badge so a run finishing on another tab is felt, not missed.
 */
export function SceneTabs({ scene, onChange, alerts }: SceneTabsProps) {
  return (
    <nav className="scene-tabs" role="tablist" aria-label={STR.navWorkshop}>
      {TABS.map(tab => {
        const alerted = alerts?.has(tab.id) ?? false;
        return (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={scene === tab.id}
            className={`scene-tab ${scene === tab.id ? 'scene-tab-active' : ''}`}
            data-tip={alerted ? STR.tabHasUpdate : tab.tip}
            onClick={() => onChange(tab.id)}
          >
            {tab.label}
            {alerted && <span className="scene-tab-badge" aria-label={STR.tabHasUpdate} />}
          </button>
        );
      })}
    </nav>
  );
}
