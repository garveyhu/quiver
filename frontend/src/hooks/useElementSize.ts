import { useCallback, useRef, useState } from 'react';

export interface Size {
  width: number;
  height: number;
}

/**
 * 测量一个元素的当前尺寸。返回一个 **回调 ref** + 实时尺寸——回调 ref 在节点挂载/
 * 卸载时触发,比 `useRef + useEffect([])` 更健壮(能正确处理条件渲染/重挂载,不会
 * 出现"effect 跑时 ref 还是 null 就再也不测"的问题)。挂载即用 clientWidth 立刻测一次,
 * 之后 ResizeObserver 跟进。用于把固定像素场景按容器大小铺满窗口。
 */
export function useElementSize<T extends HTMLElement>(): [(node: T | null) => void, Size] {
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  const roRef = useRef<ResizeObserver | null>(null);

  const refCb = useCallback((node: T | null) => {
    roRef.current?.disconnect();
    roRef.current = null;
    if (!node) return;
    const measure = () => setSize({ width: node.clientWidth, height: node.clientHeight });
    measure(); // 立刻测一次,避免首帧 0
    const ro = new ResizeObserver(measure);
    ro.observe(node);
    roRef.current = ro;
  }, []);

  return [refCb, size];
}
