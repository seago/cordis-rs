//! Cordis Rust guest 示例（Wasm 组件，M1 + B 计划 A2 + P-5 多轮 agent）。
//!
//! 提供 `db` 键（激活时绑定 `db = "wasm-pg"`）**并**演示 P-5 多轮 agent
//! 会话：每轮 `remote::submit("llm", [Count(round)])` → 宿主 `Step::Await`
//! 暂停（`Runtime::advance` 恢复，`poll_and_advance` 驱动）→ 下一拍
//! `handle.take()` 取回 worker 回复（guest 自己拿）→ 累积 → 下一轮；
//! 收尾把累积落盘 `probe`；任一轮 `Err`（worker 失败）→ `probe_err` 终止。
//! 等待步用 `EffectStep::Wait`（无逆）；收尾 `Done(inverse)`。
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

/// 会话轮数（P-5 多轮）。
const ROUNDS: u32 = 3;

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
            round: Cell::new(0),
            handle: RefCell::new(None),
            awaiting: Cell::new(false),
            acc: RefCell::new(String::new()),
            done: Cell::new(false),
        })
    }
}

/// 多轮 agent 迭代器：提交（Wait）→ 等回填（宿主 Await）→ take 累积 → 下一轮。
struct DbTask {
    round: Cell<u32>,
    handle: RefCell<Option<Handle>>,
    awaiting: Cell<bool>,
    acc: RefCell<String>,
    done: Cell<bool>,
}

impl GuestTask for DbTask {
    fn step(&self) -> Option<EffectStep> {
        if self.done.get() {
            return None;
        }
        if !self.awaiting.get() {
            // 提交轮：开始新一轮（或收尾）。
            if self.round.get() >= ROUNDS {
                self.done.set(true);
                // 收尾：累积回复落盘 probe。
                let acc = self.acc.borrow().clone();
                let inverse = context::set("probe", &Value::Text(acc)).ok()?;
                return Some(EffectStep::Done(inverse));
            }
            let h = cordis::core::remote::submit("llm", &vec![Value::Count(self.round.get() as u64)]);
            self.handle.replace(Some(h));
            self.awaiting.set(true);
            if self.round.get() == 0 {
                // 首轮顺带绑定 db（有逆步；后续轮纯等待 Wait）。
                let inverse = context::set("db", &Value::Text("wasm-pg".into())).ok()?;
                return Some(EffectStep::Step(inverse));
            }
            // 等待步（宿主 Await 暂停至回填后 advance）。
            return Some(EffectStep::Wait);
        }
        // take 轮：取回上一轮回复。
        let ready = self
            .handle
            .borrow()
            .as_ref()
            .and_then(|h| h.take());
        match ready {
            Some(Ok(v)) => {
                // 累积回复（LLM 模拟：reply:<round> 由宿主 op 返回）。
                if let Value::Text(t) = v {
                    if !self.acc.borrow().is_empty() {
                        self.acc.borrow_mut().push('|');
                    }
                    self.acc.borrow_mut().push_str(&t);
                }
                self.round.set(self.round.get() + 1);
                self.awaiting.set(false);
                // 下一轮（提交/收尾在下一拍）——当前步产 Wait 无意义，
                // 直接递归到提交分支：重新进入 step 语义——改：产 Wait 让
                // 宿主再 advance 一拍进入提交分支。
                Some(EffectStep::Wait)
            }
            Some(Err(e)) => {
                // 失败轮：落盘 probe_err 并终止。
                self.done.set(true);
                let inverse = context::set(
                    "probe_err",
                    &Value::Text(format!("err:{e}")),
                )
                .ok()?;
                Some(EffectStep::Done(inverse))
            }
            None => {
                // 未就绪：继续等待（宿主 Await）。
                Some(EffectStep::Wait)
            }
        }
    }
}

export!(DbProvider);
