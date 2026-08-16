//! Cordis Rust guest 示例（Wasm 组件，M1 沙箱隔离测试）：**违规组件**。
//!
//! 不声明任何供给（`provide = []`），但 `task.step()` 第一步即经
//! `context::set` 写**未声明的键**——触发宿主 `set_dyn` 的
//! Def 43/48 纪律 panic（走查记录⑧）：恶意 guest 引发的**宿主侧**
//! panic 须与 guest 自身 trap 一样被宿主错误边界捕获（不伤宿主）。

#![no_std]
extern crate alloc;

wit_bindgen::generate!({
    world: "cordis",
    path: "../../crates/cordis-wasm/wit/cordis.wit",
});

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use cordis::core::context::{self, Value};
use exports::cordis::core::plugin::{EffectStep, Guest, GuestComponent, GuestTask, Task};

/// 违规组件：无注入、无供给，step 时越界写。
struct Misbehave;

impl Guest for Misbehave {
    type Component = Misbehave;
    type Task = MisbehaveTask;
}

impl GuestComponent for Misbehave {
    fn new() -> Misbehave {
        Misbehave
    }
    fn inject(&self) -> Vec<String> {
        Vec::new()
    }
    fn provide(&self) -> Vec<String> {
        Vec::new()
    }
    fn start(&self) -> Task {
        Task::new(MisbehaveTask { step: Cell::new(0) })
    }
}

struct MisbehaveTask {
    step: Cell<u32>,
}

impl GuestTask for MisbehaveTask {
    fn step(&self) -> Option<EffectStep> {
        if self.step.get() == 0 {
            self.step.set(1);
            // 写未声明供给键 "boom"：宿主 forward_pending → set_dyn →
            // Def 43/48 纪律 panic（宿主侧错误，非 wasm trap）。
            let _ = context::set("boom", &Value::Flag(true));
            None
        } else {
            None
        }
    }
}

export!(Misbehave);
