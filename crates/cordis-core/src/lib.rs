//! Cordis 宿主运行时核心（论文 §5.1）。
//!
//! 本 crate 是论文第 4 章演算的 Rust 转写，承载所有理论保证：
//! 可逆效应（§3.1）、反应式共效应（§3.2）、统一上下文（§3.3）、
//! fiber 生命周期状态机（§4）。**零依赖 wasmtime**（PLAN §3 原则 2）。
//!
//! - [`symbol`] / [`key`] / [`keyset`] / [`store`]：Def 22–25 的键与依赖表；
//! - [`effect`]：Def 8/51 的可逆效应与 Algorithm 1 的 execute 引擎（PR #3）；
//! - [`context`]：Def 32 的 `Γ∞` 最小版（store + 累加器，PR #3）；
//! - [`interp`]：§4.2 基础演算的参考解释器（oracle，PR #2）。
//!
//! 论文符号 ↔ 代码映射见 `docs/THEORY-MAP.md`。

#![deny(missing_docs)]

pub mod context;
pub mod effect;
pub mod fiber;
pub mod interp;
pub mod key;
pub mod keyset;
pub mod store;
pub mod symbol;

pub use context::Context;
pub use effect::{Disposer, EffectIter, Step, execute, once};
pub use fiber::FiberId;
pub use key::Key;
pub use keyset::KeySet;
pub use store::{Store, StoreError};
pub use symbol::Symbol;

// 说明：interp 的类型（Component/Action/Lifecycle 等）刻意不从根导出——
// 它们是 oracle 专用，且 `interp::Component` 与未来生产版 Component trait
// 名字冲突，按模块访问（`cordis_core::interp::…`）。
