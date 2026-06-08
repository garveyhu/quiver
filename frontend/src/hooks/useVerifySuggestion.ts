import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/**
 * 按当前项目类型建议一条 verify 命令(后端嗅探 Cargo.toml / package.json 等)。
 * 仅作设置页验证命令字段的占位提示;后端未就绪 / 无项目时返回 ''。
 */
export function useVerifySuggestion(): string {
  const [suggestion, setSuggestion] = useState('');
  useEffect(() => {
    let active = true;
    invoke<string>('suggest_verify_command')
      .then((s) => {
        if (active) setSuggestion(s);
      })
      .catch(() => {
        /* 命令缺失 / 无项目 → 无建议 */
      });
    return () => {
      active = false;
    };
  }, []);
  return suggestion;
}
