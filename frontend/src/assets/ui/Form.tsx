import type { CSSProperties, InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from 'react';
import '../pixel.css';

export interface FieldRowProps {
  label: ReactNode;
  hint?: ReactNode;
  children: ReactNode;
}

/** 设置账本里的一行:左侧标签(+ 说明),右侧控件。 */
export function FieldRow({ label, hint, children }: FieldRowProps) {
  return (
    <div className="qv-field">
      <div className="qv-field-label">
        {label}
        {hint && <div className="qv-field-hint">{hint}</div>}
      </div>
      {children}
    </div>
  );
}

export interface ToggleProps {
  on: boolean;
  onChange?: (value: boolean) => void;
  style?: CSSProperties;
}

/** 开关(设置账本用)。 */
export function Toggle({ on, onChange, style }: ToggleProps) {
  return (
    <div
      className={`qv-toggle ${on ? 'on' : ''}`.trim()}
      style={style}
      role="switch"
      aria-checked={on}
      onClick={() => onChange?.(!on)}
    >
      <div className="qv-knob" />
    </div>
  );
}

export interface CheckboxProps {
  on: boolean;
  onChange?: (value: boolean) => void;
}

/** 复选框。 */
export function Checkbox({ on, onChange }: CheckboxProps) {
  return (
    <div className={`qv-check ${on ? 'on' : ''}`.trim()} role="checkbox" aria-checked={on} onClick={() => onChange?.(!on)}>
      {on ? '✓' : ''}
    </div>
  );
}

/** 文本输入(IME 安全的真实 input,聚焦时琥珀描边)。 */
export function TextField(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input className="qv-input" {...props} />;
}

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  options: string[];
}

/** 下拉选择。 */
export function Select({ options, ...rest }: SelectProps) {
  return (
    <select className="qv-input" {...rest}>
      {options.map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
    </select>
  );
}

/** 滑块。 */
export function Slider(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input type="range" className="qv-range" {...props} />;
}
