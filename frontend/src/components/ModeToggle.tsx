import type { RunMode } from '@/types/run.types';

interface ModeToggleProps {
  mode: RunMode;
  disabled: boolean;
  onChange: (mode: RunMode) => void;
}

const REAL_WARNING =
  'Runs a real Claude agent with file + shell access on the selected repo. ' +
  'No sandbox yet — use a repo you’re OK with. Spends your monthly Agent credit. ' +
  'Work is left on a branch (not merged).';

export function ModeToggle({ mode, disabled, onChange }: ModeToggleProps) {
  return (
    <div className="mode-toggle">
      <div className="mode-options" role="radiogroup" aria-label="Run mode">
        <label className={mode === 'simulate' ? 'mode-option mode-active' : 'mode-option'}>
          <input
            type="radio"
            name="run-mode"
            value="simulate"
            checked={mode === 'simulate'}
            disabled={disabled}
            onChange={() => onChange('simulate')}
          />
          Simulate (free)
        </label>
        <label className={mode === 'real' ? 'mode-option mode-active' : 'mode-option'}>
          <input
            type="radio"
            name="run-mode"
            value="real"
            checked={mode === 'real'}
            disabled={disabled}
            onChange={() => onChange('real')}
          />
          Real Claude (spends credit)
        </label>
      </div>

      {mode === 'real' && (
        <p className="mode-warning" role="alert">
          {REAL_WARNING}
        </p>
      )}
    </div>
  );
}
