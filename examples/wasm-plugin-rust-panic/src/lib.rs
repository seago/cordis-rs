//! Cordis Rust guest 示例（Wasm 组件，M1 沙箱隔离测试）：**崩溃组件**。
//!
//! 提供 `boom` 键，但 `task.step()` 在第一步即 **panic**（trap）——
//! 验证沙箱隔离：guest 崩溃不伤宿主（M1 门禁 2/3）。

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
use exports::cordis::core::plugin::{EffectStep, Guest, GuestComponent, GuestTask, Task};

/// 崩溃组件（Def 43 的 (d, p, e) 跨边界形态）。
struct Boom;

impl Guest for Boom {
    type Component = Boom;
    type Task = BoomTask;
}

impl GuestComponent for Boom {
    fn new() -> Boom {
        Boom
    }
    fn inject(&self) -> Vec<String> {
        Vec::new()
    }
    fn provide(&self) -> Vec<String> {
        vec!["boom".into()]
    }
    fn start(&self) -> Task {
        Task::new(BoomTask { step: Cell::new(0) })
    }
}

/// 第一步即 panic（trap）——宿主应捕获而非崩溃。
struct BoomTask {
    step: Cell<u32>,
}

impl GuestTask for BoomTask {
    fn step(&self) -> Option<EffectStep> {
        if self.step.get() == 0 {
            self.step.set(1);
            panic!("boom: 组件崩溃（沙箱隔离测试）");
        } else {
            None
        }
    }
}

export!(Boom);
