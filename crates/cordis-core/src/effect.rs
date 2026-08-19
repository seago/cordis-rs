//! 可逆效应引擎（论文 §3.1：Def 8 的效应函数、Def 51 的效应迭代器、Algorithm 1 的 execute）。
//!
//! 宿主侧同步核心：`EffectIter` 的每步完成一步效应并产出逆；`execute` 按
//! Algorithm 1 驱动迭代器（guard 在每次迭代边界检查），以 LIFO 顺序折叠各步逆
//! （论文前导句："prepending each new inverse therefore yields LIFO recovery"）。
//!
//! **协议约束（审查 M-B）**：当前同步核心要求迭代器在有限步内终止——论文模型
//! 中的效应序列是有限的（Def 51 的 `Maybe(ℑ)` 续体）；无限/订阅型效应不属于
//! 本阶段模型，违反者将导致 `execute` 永不返回、累加器无界增长。PR #5 接入
//! async 后另行支持。
//!
//! **panic 策略（审查 m-C）**：单线程宿主下 panic 即 bug（oracle 阶段策略）；
//! 单步逆 panic 会中止剩余撤销（无 unwind 保护），调用方须保证逆不 panic。
//!
//! PR #5 接入 tokio 后提供异步步骤（Algorithm 1 的 `await iter.next()`），
//! 本模块保持纯逻辑、零依赖（见 THEORY-MAP「已知偏差」）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// 逆（论文 Def 8 的 `g: Γ → Γ` 的宿主侧命令式载体）：一次性撤销其伴随的效应。
///
/// 单线程宿主（ADR-0002），不要求 `Send`。
pub type Disposer = Box<dyn FnOnce() + 'static>;

/// 效应迭代器 `𝔈iter_Γ`（Def 51）：宿主驱动，每步产出（逆，续体选项）。
///
/// **必须有限终止**：`next` 最终须产出 [`Step::Finished`]，否则 `execute` 永不返回
/// （审查 M-B，见模块文档）。
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
    /// 挂起等待外部就绪（B 计划 A1）：迭代器暂停在当前位置（已产逆保留
    /// 在挂起累加器），由 [`crate::runtime::Runtime::advance`] 恢复。
    /// 同步 [`execute`] 不接受本步（产之即 panic = 走错路径）；
    /// 可恢复路径经 [`try_execute_with`]。**添加性**：既有迭代器不产
    /// Await → 既有执行语义零变化。
    Await,
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
            Step::Await => {
                panic!(
                    "同步 execute 不接受 Await——迭代器应走 try_execute_with/advance 路径（调用方违反 B 计划 A1 约定）"
                )
            }
        }
    }
    Box::new(move || {
        for d in acc.into_iter().rev() {
            d();
        }
    })
}

/// 可恢复执行的迭代步进（B 计划 A1）：与 [`execute`] 相同驱动，但遇
/// [`Step::Await`] 返回挂起（迭代器 + 已累积逆），由调用方保存在
/// fiber 挂起态并在外部就绪后以同一初始 acc 恢复。
///
/// `Ok(disposer)`：迭代终止（含 guard 中断）——acc（含传入 `init_acc`）
/// 折叠为 LIFO disposer；`Err((iter, acc))`：遇 Await 挂起——`acc` 含
/// 本次及历史累积逆（可与 `init_acc` 连续）。
pub fn try_execute_with(
    mut iter: Box<dyn EffectIter>,
    guard: impl Fn() -> bool,
    mut acc: Vec<Disposer>,
) -> Result<Disposer, (Box<dyn EffectIter>, Vec<Disposer>)> {
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
            Step::Await => return Err((iter, acc)),
        }
    }
    let final_acc = acc;
    Ok(Box::new(move || {
        for d in final_acc.into_iter().rev() {
            d();
        }
    }))
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

/// 单步逆的幂等句柄（Algorithm 1 第 13–14 行的 armed 语义细化到每步）：
/// 同一步逆的多个等价闭包（execute 组合 + 上下文累加器）共享一个句柄，
/// 撤销至多生效一次，跨路径调用安全。
pub(crate) struct StepGuard {
    armed: Cell<bool>,
    task: RefCell<Option<Disposer>>,
}

impl StepGuard {
    /// 以一步逆新建句柄（armed = true）。
    pub(crate) fn new(inv: Disposer) -> Rc<Self> {
        Rc::new(Self {
            armed: Cell::new(true),
            task: RefCell::new(Some(inv)),
        })
    }

    /// 生成一个共享本句柄 armed 语义的 disposer（可多次调用生成多个等价闭包）。
    pub(crate) fn disposer(self: &Rc<Self>) -> Disposer {
        let guard = Rc::clone(self);
        Box::new(move || guard.run())
    }

    fn run(&self) {
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
            Step::Await => unreachable!("once 迭代器不产 Await"),
        }
    }

    /// B 计划 A1：挂起/恢复路径（try_execute_with）——Await 处停下保留
    /// 迭代器 + 已累积逆，恢复时以同 acc 继续，LIFO 折叠跨挂起保持。
    #[test]
    fn try_execute_with_suspends_on_await_and_resumes_lifo() {
        struct AwaitSteps {
            log: Rc<RefCell<Vec<String>>>,
            n: u32,
        }
        impl EffectIter for AwaitSteps {
            fn next(&mut self) -> Step {
                self.n += 1;
                let log = Rc::clone(&self.log);
                match self.n {
                    1 => Step::Yielded(Box::new(move || log.borrow_mut().push("a".into()))),
                    2 => Step::Await,
                    _ => Step::Finished(Box::new(move || log.borrow_mut().push("z".into()))),
                }
            }
        }
        let log = Rc::new(RefCell::new(Vec::new()));
        let iter: Box<dyn EffectIter> = Box::new(AwaitSteps {
            log: Rc::clone(&log),
            n: 0,
        });
        // 第一次：Yielded("a") 后遇 Await 挂起。
        let (iter, acc) = match try_execute_with(iter, || true, Vec::new()) {
            Err(suspended) => suspended,
            Ok(_) => panic!("应遇 Await 挂起"),
        };
        assert_eq!(acc.len(), 1, "已累积逆含第一步");

        // 恢复：Finished("z") → Ok 折叠。
        let disposer = match try_execute_with(iter, || true, acc) {
            Ok(d) => d,
            Err(_) => panic!("恢复应完成"),
        };
        disposer();
        assert_eq!(
            *log.borrow(),
            vec!["z".to_string(), "a".to_string()],
            "LIFO 跨挂起恢复（最后产生逆先执行）"
        );
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

    #[test]
    fn step_guard_is_idempotent_across_copies() {
        // 同一步逆的多个等价闭包（execute 组合 + 累加器）共享句柄，至多撤销一次。
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let log2 = Rc::clone(&log);
        let guard = StepGuard::new(Box::new(move || log2.borrow_mut().push("x".into())));
        let d1 = guard.disposer();
        let d2 = guard.disposer();
        d1();
        d2(); // no-op
        assert_eq!(*log.borrow(), vec!["x"]);
    }
}
