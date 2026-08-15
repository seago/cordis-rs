//! Cordis 宿主运行时核心（论文 §5.1）。
//!
//! 本 crate 是论文第 4 章演算的 Rust 转写，承载所有理论保证：
//! 可逆效应（§3.1）、反应式共效应（§3.2）、统一上下文（§3.3）、
//! fiber 生命周期状态机（§4）。**零依赖 wasmtime**（PLAN §3 原则 2）。
//!
//! 论文符号 ↔ 代码映射见 `docs/THEORY-MAP.md`。
//!
//! 进度：PR #1 骨架。PR #2 起填充 Symbol/Key/Spec/Store（Def 22–24）。

#![deny(missing_docs)]

/// 占位类型，PR #2 起移除。
pub fn placeholder() {}
