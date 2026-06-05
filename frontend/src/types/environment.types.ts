// 工坊体检（DESIGN §9 health check）的前端类型契约。
// 与 src-tauri/src/environment.rs 的 EnvironmentCheck / CheckStatus 一一对齐——
// Rust 端 #[serde(rename_all = "camelCase")]，故这里全部 camelCase。

export type CheckStatus = 'ok' | 'warn' | 'fail';

export interface EnvironmentCheck {
  /** 稳定探针 id（claude_found / git_found …）——UI 以此为 key。 */
  id: string;
  /** 状态字旁的中文标签。 */
  label: string;
  /** ok | warn | fail。 */
  status: CheckStatus;
  /** 人话结果（含发现的路径/版本）。 */
  message: string;
  /** 仅 warn/fail 才有的中文修复步骤。 */
  remediation?: string;
  /** 路径 / 版本号等原始细节，用于可展开行。 */
  detail?: string;
}
