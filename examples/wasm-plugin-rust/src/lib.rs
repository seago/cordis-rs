//! Cordis Rust guest 示例（Wasm 组件，M1 验收）。
//!
//! 提供 `db` 键：激活时经宿主 `context::set` 绑定 `db = "wasm-pg"`。
//!
//! 编译：`cargo build -p wasm-plugin-rust --target wasm32-wasip2`
//! （rustc 直接产出**组件二进制**，宿主侧加载见 `crates/cordis-wasm`；
//! wit 世界真源：`crates/cordis-wasm/wit/cordis.wit`）。
//!
//! **no_std + alloc**：能力面 = 仅 `context` 接口（论文 §6.3：
//! import 面即能力面；wasip2 标准库仍引用 WASI p2 接口，宿主提供）。

#![no_std]
extern crate alloc;

wit_bindgen::generate!({
    world: "cordis",
    path: "../../crates/cordis-wasm/wit/cordis.wit",
});

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;
use cordis::core::context::{self, Value};
use exports::cordis::core::plugin::{EffectStep, Guest, GuestComponent, GuestTask, Task};

/// db 提供者组件（Def 43 的 (d, p, e) 跨边界形态）。
struct DbProvider;

impl Guest for DbProvider {
    type Component = DbProvider;
    type Task = DbTask;
}

impl GuestComponent for DbProvider {
    fn new() -> DbProvider {
        DbProvider
    }
    fn inject(&self) -> Vec<String> {
        Vec::new()
    }
    fn provide(&self) -> Vec<String> {
        vec!["db".into()]
    }
    fn start(&self) -> Task {
        Task::new(DbTask { step: Cell::new(0) })
    }
}

/// 效应迭代器（Def 51 𝔈iter 跨边界形态）：一步绑定 db。
struct DbTask {
    step: Cell<u32>,
}

impl GuestTask for DbTask {
    fn step(&self) -> Option<EffectStep> {
        if self.step.get() == 0 {
            self.step.set(1);
            // 激活效应：绑定 db = "wasm-pg"（逆由宿主提供）。
            let inverse = context::set("db", &Value::Text("wasm-pg".into())).ok()?;
            Some(EffectStep {
                inverse,
                done: true,
            })
        } else {
            None
        }
    }
}

export!(DbProvider);
