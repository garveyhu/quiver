import type { ReactNode } from 'react';

interface SettingFieldProps {
  label: string;
  /** Stardew-style hover tooltip explaining the control. */
  tip: string;
  /** Optional inline value/status shown to the right of the label. */
  hint?: ReactNode;
  children: ReactNode;
}

/**
 * One labeled setting row: a Chinese label with a "?" affordance that reveals a
 * cozy hover tooltip, plus the control on its own line. Presentation only — the
 * control (and its wiring) is passed as children.
 */
export function SettingField({ label, tip, hint, children }: SettingFieldProps) {
  return (
    <div className="setting-field">
      <div className="setting-field-head">
        <span className="setting-field-label">{label}</span>
        <span className="setting-tip" tabIndex={0} aria-label={tip}>
          <span className="setting-tip-mark" aria-hidden>
            ?
          </span>
          <span className="setting-tip-bubble" role="tooltip">
            {tip}
          </span>
        </span>
        {hint != null && <span className="setting-field-hint">{hint}</span>}
      </div>
      <div className="setting-field-control">{children}</div>
    </div>
  );
}
