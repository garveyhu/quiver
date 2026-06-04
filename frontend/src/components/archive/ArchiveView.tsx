import { useMemo, useState } from 'react';
import { STR, TASK_STATUS_LABEL } from '@/strings';
import type { TaskRecord } from '@/types/persistence.types';
import { ArchiveEntry } from '@/components/archive/ArchiveEntry';

interface ArchiveViewProps {
  records: TaskRecord[];
  error: string | null;
  onOpen: (record: TaskRecord) => void;
}

const ALL = '__all__';

function projectName(project: string): string {
  const parts = project.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? project;
}

/**
 * The 档案库: a cozy browsable shelf of PAST runs. Free-text search on the
 * prompt + status filter + project filter, all client-side over the records the
 * hook already loaded. Newest first. Clicking an entry opens its Logbook.
 */
export function ArchiveView({ records, error, onOpen }: ArchiveViewProps) {
  const [query, setQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>(ALL);
  const [projectFilter, setProjectFilter] = useState<string>(ALL);

  // Distinct status / project values present in the data drive the dropdowns —
  // no point offering a filter for a status that never occurred.
  const statuses = useMemo(() => {
    const set = new Set<string>();
    for (const r of records) set.add(r.status);
    return [...set];
  }, [records]);

  const projects = useMemo(() => {
    const set = new Set<string>();
    for (const r of records) set.add(r.project);
    return [...set];
  }, [records]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return records.filter(r => {
      if (statusFilter !== ALL && r.status !== statusFilter) return false;
      if (projectFilter !== ALL && r.project !== projectFilter) return false;
      if (q && !r.prompt.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [records, query, statusFilter, projectFilter]);

  return (
    <section className="archive-view">
      <header className="archive-head">
        <h2 className="archive-title">{STR.archiveTitle}</h2>
        <p className="archive-subtitle">{STR.archiveSubtitle}</p>
      </header>

      <div className="archive-controls">
        <input
          type="search"
          className="archive-search"
          placeholder={STR.archiveSearchPlaceholder}
          aria-label={STR.archiveSearchAria}
          value={query}
          onChange={e => setQuery(e.target.value)}
        />
        <label className="archive-filter">
          <span className="archive-filter-label">{STR.archiveFilterStatus}</span>
          <select value={statusFilter} onChange={e => setStatusFilter(e.target.value)}>
            <option value={ALL}>{STR.archiveFilterAll}</option>
            {statuses.map(s => (
              <option key={s} value={s}>
                {TASK_STATUS_LABEL[s] ?? s}
              </option>
            ))}
          </select>
        </label>
        <label className="archive-filter">
          <span className="archive-filter-label">{STR.archiveFilterProject}</span>
          <select value={projectFilter} onChange={e => setProjectFilter(e.target.value)}>
            <option value={ALL}>{STR.archiveFilterAll}</option>
            {projects.map(p => (
              <option key={p} value={p} title={p}>
                {projectName(p)}
              </option>
            ))}
          </select>
        </label>
        <span className="archive-count">
          {STR.archiveCountPrefix}
          {filtered.length}
          {STR.archiveCountSuffix}
        </span>
      </div>

      {error && <p className="app-error">{error}</p>}

      {records.length === 0 ? (
        <p className="archive-empty">{STR.archiveEmpty}</p>
      ) : filtered.length === 0 ? (
        <p className="archive-empty">{STR.archiveNoMatch}</p>
      ) : (
        <ul className="archive-list">
          {filtered.map(record => (
            <ArchiveEntry key={record.id} record={record} onOpen={onOpen} />
          ))}
        </ul>
      )}
    </section>
  );
}
