import { STR } from '@/strings';

interface ProjectPickerProps {
  projectPath: string | null;
  disabled: boolean;
  onPick: () => void;
}

export function ProjectPicker({ projectPath, disabled, onPick }: ProjectPickerProps) {
  return (
    <div className="project-picker">
      <button type="button" className="project-pick-btn" disabled={disabled} onClick={onPick}>
        {STR.pickProject}
      </button>
      {projectPath ? (
        <span className="project-path" title={projectPath}>
          {STR.projectPathPrefix}
          {projectPath}
        </span>
      ) : (
        <span className="project-path project-path-empty">{STR.noProjectSelected}</span>
      )}
    </div>
  );
}
