import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * 与 Tauri 后端通信的唯一封装层。展示组件/页面不直接 import @tauri-apps/api，
 * 只经由这里(或基于它的 hook)调命令、订事件。
 *
 * 注意:本应用未开 withGlobalTauri，window.__TAURI__ 不可用 —— 必须走 invoke/listen。
 */

/** 调一条后端 IPC 命令(返回类型由调用方按契约指定)。 */
export function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

/** 订阅一个后端事件通道；返回取消订阅句柄。 */
export function subscribe<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, e => handler(e.payload));
}
