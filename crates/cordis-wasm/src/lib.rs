//! Cordis Wasm 组件后端（PLAN §4.4，M1 交付）。
//!
//! 基于 wasmtime + 组件模型。wit 世界（真源）：
//! `crates/cordis-wasm/wit/cordis.wit`——通用 host 世界 `cordis`：
//!
//! - **import context**（宿主能力面）：`get`/`set` 与 `inverse` 资源
//!   （Def 8 的逆句柄化，PLAN §4.4：宿主 accumulator 持句柄，
//!   LIFO 调 `run()`；不依赖 destructor）；
//! - **export plugin**（guest 组件形态）：`component` 资源（Def 43 的
//!   (d, p, e) 跨边界）与 `task` 资源（Def 51 𝔈iter 跨边界）——
//!   **宿主驱动效应迭代协议**：`step()` 每步返回 `option<effect-step>`
//!   （含 inverse 句柄），guard 在宿主侧检查（§4.3.2 步界中断）。
//!
//! # 工具链（M1 决定）
//!
//! - guest 以 **`wasm32-wasip2`** target 编译（组件模型 preview2，
//!   rustc 直接产出**组件二进制**，无需 wasm-tools 组件化步骤）；
//! - guest 为 **no_std + alloc**：能力面 = 仅 `context` 接口（论文
//!   §6.3：import 面即能力面）；wasip2 标准库仍引用 WASI p2 接口
//!   （`wasi:io` 等），宿主经 `wasmtime_wasi::p2` 提供；
//! - 宿主 `Store<T>` 的 `T`（本 crate `Host`）实现 [`WasiView`]——
//!   因 `WasiView: Send` 约束，`Host` 内**不持有** `Rc`（cordis-core
//!   的 Rc 桥接在设计上位于宿主状态之外，PR #12 落地）。
//!
//! # 进度
//!
//! - PR #10：wit 世界 v1 + 宿主加载/驱动原语 + Rust guest 示例
//!   （`examples/wasm-plugin-rust`）端到端闭环；
//! - PR #11+：宿主驱动迭代协议的完整化（逆句柄表）、桥接 cordis-core
//!   `Component` trait、双后端共存、沙箱与双语言 guest。

#![deny(missing_docs)]

use std::collections::HashMap;

use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

/// wit 世界 `cordis` 的宿主侧绑定（生成于 `wit/cordis.wit`）。
///
/// 生成代码（资源句柄/接口 trait）由 bindgen! 产出、无内联文档——
/// 语义见 wit 文件注释；此处整体放宽 `missing_docs`。
#[allow(missing_docs)]
pub mod wit {
    wasmtime::component::bindgen!({
        path: "wit/cordis.wit",
        world: "cordis",
    });
}

pub use wit::cordis::core::context::{Host as ContextHost, HostInverse, Inverse, Value};

/// wasmtime 引擎（进程级共享）。
pub type Engine = wasmtime::Engine;

/// 逆句柄表条目：被撤销的键 + 一次撤销动作（宿主侧实现 Def 8 的逆）。
type InverseTask = (String, Box<dyn FnOnce() + Send + 'static>);

/// 组件宿主状态：绑定表（跨边界值）+ 逆句柄表 + WASI 上下文。
///
/// `Store<T>` 的 `T`。实现 `context` 接口（get/set）与 `inverse`
/// 资源（run/drop）。
pub struct Host {
    /// 跨边界绑定表（`σ` 的 wasm 侧镜像）。
    bindings: HashMap<String, Value>,
    /// 逆句柄表：`rep → (键, 撤销闭包)`（句柄化，不依赖 destructor）。
    inverses: Vec<Option<InverseTask>>,
    /// WASI preview2 上下文（wasip2 标准库依赖）。
    wasi: WasiCtx,
    /// WASI 资源表。
    table: wasmtime::component::ResourceTable,
}

impl Host {
    /// 空宿主。
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            inverses: Vec::new(),
            wasi: WasiCtxBuilder::new().build(),
            table: wasmtime::component::ResourceTable::new(),
        }
    }

    /// 当前绑定表（调试/断言用）。
    pub fn bindings(&self) -> &HashMap<String, Value> {
        &self.bindings
    }

    /// 执行一次逆（rep 表查找，撤销绑定 + 消费闭包）。
    ///
    /// 供宿主侧 LIFO 恢复调用（PLAN §4.4：不依赖 destructor）。
    pub fn run_inverse(&mut self, rep: u32) {
        if let Some((key, task)) = self
            .inverses
            .get_mut(rep as usize)
            .and_then(|slot| slot.take())
        {
            self.bindings.remove(&key);
            task();
        }
    }
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

impl WasiView for Host {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl HostInverse for Host {
    fn run(&mut self, self_: wasmtime::component::Resource<Inverse>) {
        self.run_inverse(self_.rep());
    }
    fn drop(&mut self, rep: wasmtime::component::Resource<Inverse>) -> wasmtime::Result<()> {
        self.inverses[rep.rep() as usize] = None;
        Ok(())
    }
}

impl ContextHost for Host {
    fn get(&mut self, key: String) -> Option<Value> {
        self.bindings.get(&key).cloned()
    }
    fn set(
        &mut self,
        key: String,
        value: Value,
    ) -> Result<wasmtime::component::Resource<Inverse>, String> {
        self.bindings.insert(key.clone(), value);
        let rep = self.inverses.len() as u32;
        self.inverses.push(Some((key, Box::new(|| {}))));
        Ok(wasmtime::component::Resource::new_own(rep))
    }
}
