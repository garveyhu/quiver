//! IPC 命令面(DESIGN §3),按业务域拆分。每个子模块只管一个领域的 `#[tauri::command]`
//! 处理器;这里扁平 re-export 它们,好让 [`crate::run`] 的 `invoke_handler` 仍用裸名注册。
//!
//! app 骨架(`AppState`、项目状态 helper `current_project`/`select_validated_project`、
//! Tauri builder)留在 crate 根 `lib.rs` —— 它是 `AppState` 的内核、被所有域共用。
//! 经理控制循环本身在 [`crate::manager`](运行时,非 IPC);这里 `manager_cmds` 只是预览命令。

mod manager_cmds;
mod memory;
mod observability;
mod project;
mod roles;
mod settings;
mod tasks;

pub use manager_cmds::*;
pub use memory::*;
pub use observability::*;
pub use project::*;
pub use roles::*;
pub use settings::*;
pub use tasks::*;
