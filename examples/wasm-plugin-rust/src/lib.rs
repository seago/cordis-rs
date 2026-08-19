//! Cordis Rust guest 示例（Wasm 组件，M1）。
//!
//! 提供 `db` 键（激活时绑定 `db = "wasm-pg"`）**并**演示 M1 wasm 桥远端
//! 提交：激活步经 `remote::submit` 提交宿主注册操作 `echo`（宿主侧
//! `remote_result` 轮询断言回填与 O-6 隔离——W2 端到端）。`handle.take()`
//! 契约面存在（wit/编译）；完整 take 轮询回填需两次驱动（M2 异步驱动解
//! 锁，见 `docs/cordis-wasm-WASMREMOTE-PROTOCOL.md` §时序边界）。
//!
//! 编译：`cargo build --target wasm32-wasip2`
//! wit 世界真源：`crates/cordis-wasm/wit/cordis.wit`（含 `remote` import）。

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

/// db 提供者组件（Def 43 的 (d, p, e) 跨边界形态）+ 远端提交探针（W2）。
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

/// 效应迭代器（Def 51 𝔈iter 跨边界形态）：一步 = db 绑定 + 远端提交。
struct DbTask {
    step: Cell<u32>,
}

impl GuestTask for DbTask {
    fn step(&self) -> Option<EffectStep> {
        if self.step.get() == 0 {
            self.step.set(1);
            // 激活效应：绑定 db = "wasm-pg"（逆由宿主提供）。
            let db_inverse = context::set("db", &Value::Text("wasm-pg".into())).ok()?;
            // W2 探针：提交宿主注册操作 echo（参数 7）。宿主经
            // `remote_result` 轮询断言回填（execute 同步一口气，take 时序
            // 边界见协议稿）。
            let _h = cordis::core::remote::submit("echo", &vec![Value::Count(7)]);
            Some(EffectStep {
                inverse: db_inverse,
                done: true,
            })
        } else {
            None
        }
    }
}

export!(DbProvider);
