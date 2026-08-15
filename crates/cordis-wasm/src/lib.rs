//! Cordis Wasm 组件后端（PLAN §4.4，M1 交付）。
//!
//! 基于 wasmtime + 组件模型：每 fiber 独立 Linker（import 集合 = 能力面）、
//! 宿主驱动的效应迭代协议（guest 导出 `task`/`inverse` 资源）、桥接层
//! 实现 resource handle ↔ `Arc<dyn Any>` 适配（论文 §6.3）。
//!
//! 进度：PR #1 骨架，wasmtime 依赖在 M1 引入。

#![deny(missing_docs)]
