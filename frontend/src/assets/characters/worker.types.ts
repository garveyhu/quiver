/** 像素小工的 5(+idle)状态。映射 DESIGN §5.5 的 sprite 状态机。 */
export type WorkerState =
  | 'idle' // 待命(呼吸 + 眨眼)
  | 'work' // 干活 · 敲键盘
  | 'coffee' // 卡住 · 端咖啡(等你/限流/预算)
  | 'sweat' // 紧张 · 跑测试
  | 'cel' // 庆祝 · 验证通过已合并
  | 'sick'; // 崩了 · 失败

/** 同款不同色 + 配件 → 看板上区分不同任务的工人。 */
export interface WorkerSkin {
  /** 身体/手臂颜色(CSS color,建议用 cssVar('pink') 等 token) */
  body?: string;
  /** 头发颜色 */
  hair?: string;
  /** 戴耳机变体(替代呆毛) */
  headphone?: boolean;
}
