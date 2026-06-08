import type { CSSProperties } from 'react';

/** 所有像素道具的通用入参:由场景决定摆放位置(position:absolute + left/bottom)。 */
export interface PropProps {
  className?: string;
  style?: CSSProperties;
}
