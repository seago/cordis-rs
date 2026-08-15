//! Cordis 过程宏 DX 层（PLAN §4.3）。
//!
//! `#[component]`：从 `#[inject]`/`#[provide]` 字段生成 `inject()`/`provide()`；
//! `ctx.inject::<K>()` 编译期类型安全。宏是包装，最后做（PLAN §8 反模式 3）。
//!
//! 进度：PR #1 骨架。

#![deny(missing_docs)]
