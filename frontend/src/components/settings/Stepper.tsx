interface StepperProps {
  value: number;
  min: number;
  max: number;
  step?: number;
  /** Optional suffix unit shown after the value (e.g. "ms"). */
  unit?: string;
  ariaLabel: string;
  onChange: (value: number) => void;
}

/**
 * A cozy numeric stepper: carved − / + buttons flanking the current value,
 * clamped to [min, max]. Used for whole-number settings (worker count, pacing).
 */
export function Stepper({ value, min, max, step = 1, unit, ariaLabel, onChange }: StepperProps) {
  const clamp = (n: number) => Math.min(max, Math.max(min, n));
  return (
    <div className="stepper" role="group" aria-label={ariaLabel}>
      <button
        type="button"
        className="stepper-btn"
        aria-label="减少"
        disabled={value <= min}
        onClick={() => onChange(clamp(value - step))}
      >
        −
      </button>
      <span className="stepper-value">
        {value}
        {unit && <span className="stepper-unit">{unit}</span>}
      </span>
      <button
        type="button"
        className="stepper-btn"
        aria-label="增加"
        disabled={value >= max}
        onClick={() => onChange(clamp(value + step))}
      >
        +
      </button>
    </div>
  );
}
