import type { CSSProperties, ReactNode } from 'react';
import '../pixel.css';
import { cssVar } from '@/assets/palette';

export interface MenuItem {
  label: string;
  active?: boolean;
  onClick?: () => void;
}

export interface MenuBarProps {
  brand?: string;
  items?: MenuItem[];
  /** 右侧区域(放预算条、设置图标等) */
  right?: ReactNode;
  className?: string;
  style?: CSSProperties;
}

/** 游戏风顶部菜单栏:品牌 + 菜单项 + 右侧 HUD。 */
export function MenuBar({ brand = 'QUIVER', items = [], right, className, style }: MenuBarProps) {
  return (
    <div className={`qv-menubar ${className ?? ''}`.trim()} style={style}>
      <span style={{ color: cssVar('amber'), fontWeight: 'bold', letterSpacing: 1, padding: '0 8px' }}>{brand}</span>
      {items.map((it, i) => (
        <span key={i} className={`qv-menuitem ${it.active ? 'active' : ''}`.trim()} onClick={it.onClick}>
          {it.label}
        </span>
      ))}
      {right !== undefined && <span style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 8 }}>{right}</span>}
    </div>
  );
}
