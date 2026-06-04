import type { RecentProject } from '@/types/persistence.types';
import { STR } from '@/strings';

interface RecentProjectsProps {
  projects: RecentProject[];
  activePath: string | null;
  disabled: boolean;
  onSelect: (path: string) => void;
}

/**
 * The durable recent-projects list (DESIGN §11). Clicking an entry re-selects
 * that repo without the folder dialog. Survives an app restart.
 */
export function RecentProjects({ projects, activePath, disabled, onSelect }: RecentProjectsProps) {
  return (
    <div className="recent-projects">
      <h3 className="panel-subtitle">{STR.recentHeader}</h3>
      {projects.length === 0 ? (
        <p className="panel-empty">{STR.recentEmpty}</p>
      ) : (
        <ul className="recent-list">
          {projects.map(project => (
            <li key={project.path}>
              <button
                type="button"
                className={`recent-item${project.path === activePath ? ' recent-item-active' : ''}`}
                disabled={disabled}
                title={STR.recentReselectTitle}
                onClick={() => onSelect(project.path)}
              >
                {project.path}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
