//! 可逆效应引擎（论文 §3.1：Def 8 的效应函数、Def 51 的效应迭代器、Algorithm 1 的 execute）。
//!
//! 宿主侧同步核心：`EffectIter` 的每步完成一步效应并产出逆；`execute` 按
//! Algorithm 1 驱动迭代器（guard 在每次迭代边界检查），以 LIFO 顺序折叠各步逆。
//! PR #5 接入 tokio 后提供异步步骤（Algorithm 1 的 `await iter.next()`），
//! 本模块保持纯逻辑、零依赖（见 THEORY-MAP「已知偏差」）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// 逆（论文 Def 8 的 `g: Γ → Γ` 的宿主侧命令式载体）：一次性撤销其伴随的效应。
///
/// 单线程宿主（ADR-0002），不要求 `Send`。
pub type Disposer = Box<dyn FnOnce() + 'static>;

/// 效应迭代器 `𝔈iter_Γ`（Def 51）：宿主驱动，每步产出（逆，续体选项）。
pub trait EffectIter: 'static {
    /// 执行一步效应并返回本步的逆。
    fn next(&mut self) -> Step;
}

/// 迭代步（Def 51 的 `Γ → Γ × (Γ → Γ) × Maybe(ℑ)` 的宿主侧投影）。
pub enum Step {
    /// 产出逆并继续迭代（`Just(ℑ)`）。
    Yielded(Disposer),
    /// 产出逆并终止迭代（`Nothing`）。
    Finished(Disposer),
}

/// Algorithm 1 的 execute 引擎。
///
/// 驱动 `iter` 直至 `guard` 失效或迭代终止，将各步逆以 LIFO 顺序折叠
/// （`inverse ← value ∘ inverse`，对应 Thm 16 的逆序撤销）。guard 在每次
/// 迭代边界检查（§4.3.2 步界中断）；guard 首次即失效时返回恒等逆。
pub fn execute(mut iter: Box<dyn EffectIter>, guard: impl Fn() -> bool) -> Disposer {
    let mut acc: Vec<Disposer> = Vec::new();
    loop {
        if !guard() {
            break;
        }
        match iter.next() {
            Step::Yielded(d) => acc.push(d),
            Step::Finished(d) => {
                acc.push(d);
                break;
            }
        }
    }
    Box::new(move || {
        for d in acc.into_iter().rev() {
            d();
        }
    })
}

/// 把单个效应包装为一步迭代器（Def 8 的 `𝔈Γ` 是 Def 51 的退化情形）。
pub fn once(callback: impl FnOnce() -> Disposer + 'static) -> impl EffectIter {
    Once {
        callback: Some(Box::new(callback)),
    }
}

struct Once {
    callback: Option<Box<dyn FnOnce() -> Disposer>>,
}

impl EffectIter for Once {
    fn next(&mut self) -> Step {
        let callback = self
            .callback
            .take()
            .expect("once 迭代器恰好产出一步（协议违反）");
        Step::Finished(callback())
    }
}

/// [`crate::context::Context::effect`] 的共享撤销句柄（Algorithm 1 第 10–17 行：
/// armed 标志 + 惰性任务）。
pub(crate) struct EffectHandle {
    armed: Cell<bool>,
    task: RefCell<Option<Disposer>>,
}

impl Default for EffectHandle {
    fn default() -> Self {
        Self {
            armed: Cell::new(true),
            task: RefCell::new(None),
        }
    }
}

impl EffectHandle {
    /// 新建句柄（armed = true，任务未就绪）。
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// 安装 execute 的组合逆。
    pub(crate) fn install(&self, task: Disposer) {
        *self.task.borrow_mut() = Some(task);
    }

    /// armed 标志（execute 的 guard 读取，Algorithm 1 第 11 行）。
    pub(crate) fn is_armed(&self) -> bool {
        self.armed.get()
    }

    /// 撤销（至多一次）：置 armed = false 并运行任务（Algorithm 1 第 12–16 行）。
    pub(crate) fn dispose(&self) {
        if !self.armed.get() {
            return;
        }
        self.armed.set(false);
        let task = self.task.borrow_mut().take();
        if let Some(task) = task {
            task();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Steps {
        log: Rc<RefCell<Vec<String>>>,
        stop: Rc<Cell<bool>>,
        n: usize,
        total: usize,
    }

    impl EffectIter for Steps {
        fn next(&mut self) -> Step {
            self.n += 1;
            let log = Rc::clone(&self.log);
            let name = format!("undo{}", self.n);
            if self.n == 2 {
                self.stop.set(true);
            }
            if self.n == self.total {
                Step::Finished(Box::new(move || log.borrow_mut().push(name)))
            } else {
                Step::Yielded(Box::new(move || log.borrow_mut().push(name)))
            }
        }
    }

    #[test]
    fn once_yields_single_finished_step() {
        let mut iter = once(|| Box::new(|| {}) as Disposer);
        match iter.next() {
            Step::Finished(_) => {}
            Step::Yielded(_) => panic!("once 应恰好一步并终止"),
        }
    }

    #[test]
    fn execute_runs_inverses_in_lifo() {
        // Thm 16(1)：逆序撤销——最后产出的逆最先运行。
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let iter = Steps {
            log: Rc::clone(&log),
            stop: Rc::new(Cell::new(false)),
            n: 0,
            total: 3,
        };
        let disposer = execute(Box::new(iter), || true);
        disposer();
        assert_eq!(*log.borrow(), vec!["undo3", "undo2", "undo1"]);
    }

    #[test]
    fn execute_interrupts_at_step_boundary() {
        // §4.3.2：guard 在迭代边界失效 → 仅已完成步骤的逆被折叠（部分回滚）。
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let stop = Rc::new(Cell::new(false));
        let iter = Steps {
            log: Rc::clone(&log),
            stop: Rc::clone(&stop),
            n: 0,
            total: 5,
        };
        let disposer = execute(Box::new(iter), {
            let stop = Rc::clone(&stop);
            move || !stop.get()
        });
        disposer();
        assert_eq!(
            *log.borrow(),
            vec!["undo2", "undo1"],
            "仅已完成的两步被恢复"
        );
    }

    #[test]
    fn execute_stops_after_finished() {
        let calls = Rc::new(Cell::new(0));
        let calls2 = Rc::clone(&calls);
        let iter = once(move || {
            calls2.set(calls2.get() + 1);
            Box::new(|| {}) as Disposer
        });
        drop(execute(Box::new(iter), || true));
        assert_eq!(calls.get(), 1, "Finished 后不得再驱动迭代器");
    }

    #[test]
    fn execute_with_dead_guard_is_noop() {
        let ran = Rc::new(Cell::new(false));
        let ran2 = Rc::clone(&ran);
        let iter = once(move || {
            ran2.set(true);
            Box::new(|| {}) as Disposer
        });
        let disposer = execute(Box::new(iter), || false);
        assert!(!ran.get(), "guard 首次即失效，迭代器不被驱动");
        disposer(); // 恒等逆
    }
}
