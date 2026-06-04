import { STR } from '@/strings';
import type { RunMode } from '@/types/run.types';

interface ModeToggleProps {
  mode: RunMode;
  disabled: boolean;
  onChange: (mode: RunMode) => void;
}

export function ModeToggle({ mode, disabled, onChange }: ModeToggleProps) {
  return (
    <div className="mode-toggle">
      <div className="mode-options" role="radiogroup" aria-label={STR.modeAriaLabel}>
        <label className={mode === 'simulate' ? 'mode-option mode-active' : 'mode-option'}>
          <input
            type="radio"
            name="run-mode"
            value="simulate"
            checked={mode === 'simulate'}
            disabled={disabled}
            onChange={() => onChange('simulate')}
          />
          {STR.modeSimulate}
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
          {STR.modeReal}
        </label>
      </div>

      {mode === 'real' && (
        <p className="mode-warning" role="alert">
          {STR.realWarning}
        </p>
      )}
    </div>
  );
}
