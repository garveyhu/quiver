import type { ButtonHTMLAttributes } from 'react';
import '../pixel.css';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'ghost' | 'danger';
}

/** 游戏风厚按钮:有下沉感,按下回弹。primary(琥珀)/ ghost(深)/ danger(红)。 */
export function Button({ variant = 'primary', className = '', ...rest }: ButtonProps) {
  const v = variant === 'primary' ? '' : variant;
  return <button className={`qv-btn ${v} ${className}`.trim()} {...rest} />;
}
