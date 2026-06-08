import { useEffect, useState } from 'react';

/**
 * 像 useState,但把值持久化到 localStorage(重启/重载后保留)。用于 UI 偏好
 * (声音、右栏折叠等)。读写都包了 try/catch,localStorage 不可用时退化为内存态。
 */
export function usePersistedState<T>(key: string, initial: T): [T, React.Dispatch<React.SetStateAction<T>>] {
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(key);
      return raw != null ? (JSON.parse(raw) as T) : initial;
    } catch {
      return initial;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      /* localStorage 不可用 → 退化为内存态 */
    }
  }, [key, value]);

  return [value, setValue];
}
