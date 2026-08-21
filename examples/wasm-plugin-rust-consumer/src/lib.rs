//! Cordis Rust guest 示例（Wasm 组件，M1 验收）：**db 消费者**。
//!
//! 注入 `db`（由 provider 组件提供），激活时经宿主 `context::get`
//! 读取注入值，再提供 `derived = "derived(<db>)"`。
//! 验证 wasm 组件消费注入依赖的链路（PR #12：镜像同步）。

#![no_std]
extern crate alloc;

wit_bindgen::generate!({
    world: "cordis",
    path: "../../crates/cordis-wasm/wit/cordis.wit",
});

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;
use cordis::core::context::{self, Value};
use exports::cordis::core::plugin::{EffectStep, Guest, GuestComponent, GuestTask, Task};

/// db 消费者组件（Def 43 的 (d, p, e) 跨边界形态）。
struct DbConsumer;

impl Guest for DbConsumer {
    type Component = DbConsumer;
    type Task = DbConsumerTask;
}

impl GuestComponent for DbConsumer {
    fn new() -> DbConsumer {
        DbConsumer
    }
    fn inject(&self) -> Vec<String> {
        vec!["db".into()]
    }
    fn provide(&self) -> Vec<String> {
        vec!["derived".into()]
    }
    fn start(&self) -> Task {
        Task::new(DbConsumerTask { step: Cell::new(0) })
    }
}

/// 效应迭代器（Def 51 𝔈iter 跨边界形态）：读注入 db → 提供 derived。
struct DbConsumerTask {
    step: Cell<u32>,
}

impl GuestTask for DbConsumerTask {
    fn step(&self) -> Option<EffectStep> {
        if self.step.get() == 0 {
            self.step.set(1);
            // 读注入的 db（宿主镜像已同步核心 store 的当前值）。
            let Value::Text(db) = context::get("db")? else {
                return None; // 依赖不可读：不提供（核心保持 Inactive 语义由宿主判定）
            };
            let derived = format!("derived({db})");
            let inverse = context::set("derived", &Value::Text(derived)).ok()?;
            Some(EffectStep::Done(inverse))
        } else {
            None
        }
    }
}

export!(DbConsumer);
