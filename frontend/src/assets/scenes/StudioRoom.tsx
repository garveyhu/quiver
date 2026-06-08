import type { CSSProperties } from 'react';
import { cssVar } from '@/assets/palette';
import { useElementSize } from '@/hooks/useElementSize';
import { Worker } from '@/assets/characters';
import type { WorkerState } from '@/assets/characters';
import {
  Desk,
  StudioWindow,
  CoffeeStation,
  Bookshelf,
  Rug,
  FloorLamp,
  WallClock,
  Plant,
  Cat,
  Armchair,
  BrickWall,
  Vignette,
  MoonBeam,
  LightPool,
  FairyLights,
  Kanban,
  NeonSign,
  HangingPlant,
} from '@/assets/props';
import { Dust } from '@/assets/fx';
import { SpeechBubble } from '@/assets/ui';

export interface RoomWorker {
  /** 旧字段,已不用于定位(StudioRoom 按工位槽自行分布);保留以兼容调用方。 */
  left?: number;
  state: WorkerState;
  body?: string;
  hair?: string;
  headphone?: boolean;
  /** 任务 id(用于点击选中) */
  taskId?: string;
  /** 头顶气泡(最新输出/状态) */
  bubble?: string;
  /** 工位灯色(随状态变,光=反馈母语) */
  light?: string;
}

export interface StudioRoomProps {
  /** 看板上的小工(默认演示三只)。真实接入时按任务数据传入。 */
  workers?: RoomWorker[];
  /** 点击某个工人(回传 taskId) */
  onWorkerClick?: (taskId: string) => void;
  /** 聚焦的任务:设置后其他工人压暗,聚焦工人高亮(点工人聚焦感) */
  focusedTaskId?: string;
  className?: string;
  style?: CSSProperties;
}

const FLOOR = 120; // 地板高度(自底)
const WALL_TOP = 14;

const DEMO_WORKERS: RoomWorker[] = [
  { state: 'work', body: cssVar('pink'), hair: '#5a3a4a' },
  { state: 'coffee', body: cssVar('green'), hair: '#3a3550', headphone: true },
  { state: 'sweat', body: cssVar('amber2'), hair: '#3a3550', headphone: true },
];

const GRID: CSSProperties = {
  position: 'absolute',
  inset: 0,
  pointerEvents: 'none',
  backgroundImage:
    'linear-gradient(rgba(0,0,0,.08) 2px,transparent 2px),linear-gradient(90deg,rgba(0,0,0,.06) 2px,transparent 2px)',
  backgroundSize: '8px 8px',
};

/**
 * 深夜工作室主大厅。**纯 CSS 流式自适应**:容器铺满父级,砖墙/地板/踢脚线全宽平铺
 * (repeating pattern,任意宽都锐利),装饰按左/中/右锚定,工位按百分比分布。
 * 不用 transform 缩放 → 永不模糊;精确铺满容器 → 永不裁切。全部 CSS,无需出图。
 */
export function StudioRoom({ workers = DEMO_WORKERS, onWorkerClick, focusedTaskId, className, style }: StudioRoomProps) {
  // 工位数随容器宽自适应(宽屏多工位填充,不再固定 3 个间距过大)。
  const [ref, size] = useElementSize<HTMLDivElement>();
  const deskCount = size.width ? Math.min(Math.max(Math.round(size.width / 240), 2), 6) : 3;
  const pct = (i: number): string => `${((i + 1) / (deskCount + 1)) * 100}%`;
  const shown = workers.slice(-deskCount);
  const overflow = Math.max(0, workers.length - deskCount);
  return (
    <div
      ref={ref}
      className={className}
      style={{
        position: 'absolute',
        inset: 0,
        overflow: 'hidden',
        background: 'linear-gradient(180deg,#1b2440 0%,#202848 55%,#161d39 100%)',
        ...style,
      }}
    >
      {/* —— 后墙:砖(全宽平铺)+ 网格 —— */}
      <BrickWall style={{ left: 0, right: 0, top: WALL_TOP, bottom: FLOOR }} />
      <div style={GRID} />

      {/* —— 天花横梁(全宽)+ 顶部串灯(居中)—— */}
      <div style={{ position: 'absolute', left: 0, right: 0, top: 0, height: WALL_TOP, background: '#10162c' }} />
      <FairyLights width={700} count={13} style={{ left: '50%', transform: 'translateX(-50%)', top: 12 }} />

      {/* —— 暖光池(墙/地大范围氛围光)—— */}
      <LightPool style={{ left: 'calc(100% - 160px)', top: 150, width: 220, height: 220 }} />
      <LightPool style={{ left: -40, top: 210, width: 180, height: 200 }} />
      <LightPool style={{ left: 'calc(50% - 120px)', top: 60, width: 200, height: 150, opacity: 0.08 }} />

      {/* —— 夜窗 + 月光束(锚左)—— */}
      <StudioWindow style={{ left: 40, top: 50 }} />
      <MoonBeam style={{ left: 120, top: 70, width: 90, height: 300 }} />

      {/* —— 后墙装饰:看板(居中)/ 挂钟(中偏右)/ 书堆·霓虹·相框(锚右)/ 垂吊绿植 —— */}
      <Kanban style={{ left: '50%', transform: 'translateX(-50%)', top: 44 }} />
      <WallClock style={{ left: 'calc(50% + 70px)', top: 150 }} />
      <Bookshelf style={{ left: 'calc(100% - 270px)', top: 70 }} />
      <NeonSign style={{ left: 'calc(100% - 108px)', top: 46 }} />
      <div style={{ position: 'absolute', left: 'calc(100% - 116px)', top: 150, width: 44, height: 30, background: '#1a1f33', boxShadow: 'inset 0 0 0 2px var(--qv-wood-dk)' }} />
      <div style={{ position: 'absolute', left: 'calc(100% - 110px)', top: 156, width: 32, height: 18, background: cssVar('blue') }} />
      <HangingPlant style={{ left: 196, top: WALL_TOP }} />
      <HangingPlant style={{ left: 'calc(100% - 50px)', top: WALL_TOP }} />

      {/* —— 地板 + 踢脚线 + 地毯(全宽)—— */}
      <div className="qv-planks" style={{ height: FLOOR }} />
      <div style={{ position: 'absolute', left: 0, right: 0, bottom: FLOOR - 2, height: 5, background: cssVar('woodDk') }} />
      <Rug style={{ left: '50%', transform: 'translateX(-50%)', bottom: 8, width: 340, height: 84 }} />
      {/* 地面屏幕反光 */}
      <LightPool style={{ left: '12%', bottom: 40, width: 90, height: 40, background: cssVar('screen'), opacity: 0.1 }} />
      <LightPool style={{ left: '78%', bottom: 40, width: 90, height: 40, background: cssVar('red'), opacity: 0.1 }} />
      <Dust style={{ left: '30%', bottom: 200 }} />
      <Dust style={{ left: '55%', bottom: 230, animationDelay: '2s' }} />
      <Dust style={{ left: '80%', bottom: 180, animationDelay: '4s' }} />

      {/* —— 地面家具:落地灯(锚左)/ 咖啡角(锚右)/ 绿植 —— */}
      <FloorLamp style={{ left: 14, bottom: FLOOR - 2 }} />
      <CoffeeStation style={{ left: 'calc(100% - 92px)', bottom: FLOOR - 2 }} />
      <Plant style={{ left: 150, bottom: FLOOR - 2 }} />
      <Plant style={{ left: 'calc(100% - 228px)', bottom: FLOOR - 2 }} />

      {/* —— 工位小工(先画,桌子盖住下半身=坐姿;按百分比锚点分布)—— */}
      {shown.map((w, i) => {
        const dimmed = !!focusedTaskId && w.taskId !== focusedTaskId;
        return (
          <div
            key={w.taskId ?? i}
            style={{
              position: 'absolute',
              left: pct(i),
              bottom: 72,
              transform: 'translateX(-50%)',
              opacity: dimmed ? 0.35 : 1,
              transition: 'opacity 0.15s steps(2)',
            }}
          >
            {w.light && (
              <div
                style={{
                  position: 'absolute',
                  bottom: -14,
                  left: '50%',
                  transform: 'translateX(-50%)',
                  width: 66,
                  height: 22,
                  background: w.light,
                  opacity: 0.5,
                  borderRadius: '50%',
                  filter: 'blur(5px)',
                  zIndex: 0,
                }}
              />
            )}
            {w.bubble && (
              <SpeechBubble
                style={{ position: 'absolute', left: '50%', transform: 'translateX(-50%)', bottom: 54, width: 130, fontSize: 9, padding: '5px 7px', zIndex: 5 }}
              >
                {w.bubble}
              </SpeechBubble>
            )}
            <div
              onClick={() => w.taskId && onWorkerClick?.(w.taskId)}
              style={{ cursor: w.taskId && onWorkerClick ? 'pointer' : undefined }}
            >
              <Worker state={w.state} body={w.body} hair={w.hair} headphone={w.headphone} />
            </div>
          </div>
        );
      })}

      {/* —— 工位桌(在小工之后画 = 盖住下半身;数量随宽自适应)—— */}
      {Array.from({ length: deskCount }).map((_, i) => (
        <Desk
          key={i}
          screen={i % 3 === 2 ? 'red' : 'screen'}
          glowDelay={`${(i % 3) * 0.6}s`}
          style={{ left: pct(i), transform: 'translateX(-50%)', bottom: 64 }}
        />
      ))}

      {/* —— 工人多于工位:溢出提示 —— */}
      {overflow > 0 && (
        <div
          title="并发数超过工位,还有工人在别处忙"
          style={{ position: 'absolute', top: 10, right: 10, background: cssVar('night'), color: cssVar('amber'), padding: '3px 7px', fontSize: 10, boxShadow: 'inset 0 0 0 2px var(--qv-wall)' }}
        >
          +{overflow} 在别处忙
        </div>
      )}

      {/* —— 前景:扶手椅 + 睡猫(中偏左)—— */}
      <Armchair style={{ left: 'calc(50% - 130px)', bottom: 14 }} />
      <Cat style={{ left: 'calc(50% - 114px)', bottom: 46 }} />

      {/* —— 全局暖色暗角(最上层)—— */}
      <Vignette />
    </div>
  );
}
