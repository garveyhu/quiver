import { useCallback, useEffect, useRef, useState } from 'react';
import { EventBus, BUS, type ProjectsState, type RecentProject } from '@/game/EventBus';
import { projectLeaf } from '@/game/logbookState';
import { STR } from '@/strings';
import { play } from '@/utils/sound';

/**
 * The 项目管理 / project manager as a real React DOM overlay (the door hotspot no
 * longer drives pickProject directly — it opens this full CRUD panel over the
 * dimmed world). Same §6 two-layer rule + shared parchment/timber skin as the
 * Logbook / Archive overlays.
 *
 * 增改删查 (add / rename / remove / select / list), all over the project command
 * channels — the bus never touches Rust. The bridge satisfies each via the
 * existing hooks (useSupervisor.pickProject/selectRecentProject for add+select,
 * useProjects.remove/rename for the rest). This component only renders the pushed
 * {@link ProjectsState} and emits intents.
 */
export function ProjectManagerOverlay() {
  const [open, setOpen] = useState(false);
  const [state, setState] = useState<ProjectsState>({ recent: [], current: null, error: null });

  useEffect(() => {
    const onOpen = () => {
      EventBus.emit(BUS.sceneReady); // ask the bridge to flush the current snapshot
      setOpen(true);
    };
    const onClose = () => setOpen(false);
    const onState = (next: ProjectsState) => setState(next);
    EventBus.on(BUS.openProjects, onOpen);
    EventBus.on(BUS.closeProjects, onClose);
    EventBus.on(BUS.projectsState, onState);
    return () => {
      EventBus.off(BUS.openProjects, onOpen);
      EventBus.off(BUS.closeProjects, onClose);
      EventBus.off(BUS.projectsState, onState);
    };
  }, []);

  const close = useCallback(() => {
    play('close');
    EventBus.emit(BUS.closeProjects);
  }, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, close]);

  const add = useCallback(() => {
    play('open');
    EventBus.emit(BUS.projectAdd);
  }, []);

  const select = useCallback((path: string) => {
    play('open');
    EventBus.emit(BUS.projectSelect, { path });
  }, []);

  const remove = useCallback((path: string) => {
    if (!window.confirm(STR.projectsRemoveConfirm)) return;
    play('close');
    EventBus.emit(BUS.projectRemove, { path });
  }, []);

  const rename = useCallback((path: string, alias: string | null) => {
    EventBus.emit(BUS.projectAlias, { path, alias });
  }, []);

  if (!open) return null;

  return (
    <div className="projects-overlay" role="dialog" aria-modal="true" aria-label={STR.projectsTitle}>
      <div className="projects-scrim" onClick={close} role="presentation" />
      <div className="projects-panel" role="document">
        <header className="projects-header">
          <div className="projects-headline">
            <h2 className="projects-title">{STR.projectsTitle}</h2>
            <button
              type="button"
              className="projects-close"
              onClick={close}
              aria-label={STR.projectsCloseAria}
            >
              {STR.projectsClose}
            </button>
          </div>
          <p className="projects-subtitle">{STR.projectsSubtitle}</p>
          <button type="button" className="projects-add" onClick={add} title={STR.projectsAddTip}>
            {STR.projectsAdd}
          </button>
        </header>

        <div className="projects-list" tabIndex={0}>
          {state.error ? (
            <p className="projects-status projects-status-error">{state.error}</p>
          ) : state.recent.length === 0 ? (
            <p className="projects-status">{STR.projectsEmpty}</p>
          ) : (
            state.recent.map(project => (
              <ProjectRow
                key={project.path}
                project={project}
                current={project.path === state.current}
                onSelect={select}
                onRemove={remove}
                onRename={rename}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

interface RowProps {
  project: RecentProject;
  current: boolean;
  onSelect: (path: string) => void;
  onRemove: (path: string) => void;
  onRename: (path: string, alias: string | null) => void;
}

// One project row: display name (alias or path leaf) + full path, a 当前 badge for
// the active pick, and the 设为当前 / 改名 / 移除 actions. Rename opens an inline
// native input — a real DOM field, so the CJK IME + 拼音候选条 work for free; we
// commit on the 保存 button / blur, never on Enter-while-composing.
function ProjectRow({ project, current, onSelect, onRemove, onRename }: RowProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(project.alias ?? '');
  const composingRef = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const displayName = project.alias?.trim() ? project.alias : projectLeaf(project.path);

  const startEdit = useCallback(() => {
    setDraft(project.alias ?? '');
    setEditing(true);
  }, [project.alias]);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  const commit = useCallback(() => {
    const trimmed = draft.trim();
    // empty → clear the alias (null), so the row falls back to the path leaf.
    onRename(project.path, trimmed.length === 0 ? null : trimmed);
    setEditing(false);
  }, [draft, onRename, project.path]);

  const cancelEdit = useCallback(() => {
    setEditing(false);
    setDraft(project.alias ?? '');
  }, [project.alias]);

  return (
    <div className={`projects-row${current ? ' projects-row-current' : ''}`}>
      <div className="projects-row-main">
        <div className="projects-row-head">
          <span className="projects-name">{displayName}</span>
          {current ? <span className="projects-badge">{STR.projectsCurrentBadge}</span> : null}
        </div>
        <span className="projects-path" title={project.path}>
          {project.path}
        </span>

        {editing ? (
          <div className="projects-rename">
            <input
              ref={inputRef}
              type="text"
              className="projects-rename-input"
              value={draft}
              placeholder={STR.projectsAliasPlaceholder}
              aria-label={STR.projectsAliasPlaceholder}
              onChange={e => setDraft(e.target.value)}
              onCompositionStart={() => {
                composingRef.current = true;
              }}
              onCompositionEnd={() => {
                composingRef.current = false;
              }}
              onKeyDown={e => {
                if (e.key === 'Enter' && !e.nativeEvent.isComposing && !composingRef.current) {
                  e.preventDefault();
                  commit();
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  cancelEdit();
                }
              }}
            />
            <button type="button" className="projects-rename-save" onClick={commit}>
              {STR.projectsAliasSave}
            </button>
            <button type="button" className="projects-rename-cancel" onClick={cancelEdit}>
              {STR.projectsAliasCancel}
            </button>
          </div>
        ) : null}
      </div>

      {!editing ? (
        <div className="projects-row-actions">
          {!current ? (
            <button
              type="button"
              className="projects-action projects-action-select"
              onClick={() => onSelect(project.path)}
              title={STR.projectsSelectTip}
            >
              {STR.projectsSelect}
            </button>
          ) : null}
          <button
            type="button"
            className="projects-action"
            onClick={startEdit}
            title={STR.projectsRenameTip}
          >
            {STR.projectsRename}
          </button>
          <button
            type="button"
            className="projects-action projects-action-remove"
            onClick={() => onRemove(project.path)}
            title={STR.projectsRemoveTip}
          >
            {STR.projectsRemove}
          </button>
        </div>
      ) : null}
    </div>
  );
}
