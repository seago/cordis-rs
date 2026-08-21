//! Cordis Rust guest 示例（Wasm 组件，M1 + B 计划 A2）。
//!
//! 提供 `db` 键（激活时绑定 `db = "wasm-pg"`）**并**经 M1 wasm 桥远端
//! `remote` 消费：step0 提交宿主操作 `echo`（参数 7）→ 宿主以 `Step::Await`
//! 暂停（core Runtime::advance 恢复）→ 后续步 `handle.take()` 轮询到
//! worker 回填结果（guest 自己取回）→ 结果 set 到 `probe`（Ok）/`probe_err`
//! （Err）。等待步用 `inverse: None`（effect-step option，wit 2026-08-20）。
//!
//! 编译：`cargo build --target wasm32-wasip2`
//! wit 世界真源：`crates/cordis-wasm/wit/cordis.wit`。

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
use core::cell::{Cell, RefCell};
use cordis::core::context::{self, Value};
use cordis::core::remote::Handle;
use exports::cordis::core::plugin::{EffectStep, Guest, GuestComponent, GuestTask, Task};

/// db 提供者组件 + 远端消费探针（A2）。
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
        vec!["db".into(), "probe".into(), "probe_err".into()]
    }
    fn start(&self) -> Task {
        Task::new(DbTask {
            step: Cell::new(0),
            handle: RefCell::new(None),
            done: Cell::new(false),
        })
    }
}

/// 效应迭代器（A2 多步）：提交 → 等待（Await）→ 取回 → 落盘。
struct DbTask {
    step: Cell<u32>,
    handle: RefCell<Option<Handle>>,
    done: Cell<bool>,
}

impl GuestTask for DbTask {
    fn step(&self) -> Option<EffectStep> {
        if self.done.get() {
            return None;
        }
        match self.step.get() {
            0 => {
                self.step.set(1);
                // 绑定 db + 提交 echo(7)；等待步（下一 host 步 Await）。
                let inverse = context::set("db", &Value::Text("wasm-pg".into())).ok()?;
                let h = cordis::core::remote::submit("echo", &vec![Value::Count(7)]);
                self.handle.replace(Some(h));
                Some(EffectStep::Step(inverse))
            }
            _ => {
                // 轮询 take：就绪 → probe/probe_err + done；未就绪 → 等待步
                //（inverse None，宿主 Await）。
                if let Some(h) = self.handle.borrow().as_ref()
                    && let Some(result) = h.take()
                {
                    self.done.set(true);
                    match result {
                        Ok(v) => {
                            let inverse = context::set("probe", &v).ok()?;
                            Some(EffectStep::Done(inverse))
                        }
                        Err(e) => {
                            let inverse = context::set(
                                "probe_err",
                                &Value::Text(format!("err:{e}")),
                            )
                            .ok()?;
                            Some(EffectStep::Done(inverse))
                        }
                    }
                } else {
                    // 等待步：无逆（none），宿主以 Await 暂停直至回填。
                    self.step.set(self.step.get() + 1);
                    Some(EffectStep::Wait)
                }
            }
        }
    }
}

export!(DbProvider);
