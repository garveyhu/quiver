import '../pixel.css';

export interface TabBarProps {
  tabs: string[];
  active: number;
  onChange?: (index: number) => void;
}

/** 标签页条。 */
export function TabBar({ tabs, active, onChange }: TabBarProps) {
  return (
    <div className="qv-tabbar">
      {tabs.map((t, i) => (
        <div key={i} className={`qv-tab ${i === active ? 'active' : ''}`.trim()} onClick={() => onChange?.(i)}>
          {t}
        </div>
      ))}
    </div>
  );
}
