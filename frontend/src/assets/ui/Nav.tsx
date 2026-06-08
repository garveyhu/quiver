import type { CSSProperties } from 'react';
import '../pixel.css';
import { Icon } from './Icon';
import type { IconName } from './Icon';

export interface NavRailItem {
  icon: IconName;
  active?: boolean;
  title?: string;
  onClick?: () => void;
}

export interface NavRailProps {
  items: NavRailItem[];
  className?: string;
  style?: CSSProperties;
}

/** 侧边图标导航条(竖排)。 */
export function NavRail({ items, className, style }: NavRailProps) {
  return (
    <div className={`qv-navrail ${className ?? ''}`.trim()} style={style}>
      {items.map((it, i) => (
        <div
          key={i}
          className={`qv-navbtn ${it.active ? 'active' : ''}`.trim()}
          title={it.title}
          onClick={it.onClick}
        >
          <Icon name={it.icon} size={16} />
        </div>
      ))}
    </div>
  );
}
