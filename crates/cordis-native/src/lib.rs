//! Cordis 进程内组件后端（PLAN §4.3）。
//!
//! 受信组件直接以 Rust 代码内联到宿主进程，共享 trait 对象与 `Arc<dyn Any>`
//! 值语义（ADR-0004）。`#[component]` 宏糖由 `cordis-macro` 提供。
//!
//! 进度：PR #1 骨架。M0 后半（PR #7）填充。

#![deny(missing_docs)]
