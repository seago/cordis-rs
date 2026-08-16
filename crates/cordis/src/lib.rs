//! Cordis 门面 crate（PLAN §4.1）：统一 re-export 全部公开 API。
//!
//! `#[component]` 宏生成的代码引用 `::cordis::` 路径——使用宏的 crate
//! 依赖本门面即可。
//!
//! 审查 m1：core 以 glob 全量导出（`execute` 等逐一漏列问题不再复发）。

#![deny(missing_docs)]

pub use cordis_core::*;
pub use cordis_macro::component;
