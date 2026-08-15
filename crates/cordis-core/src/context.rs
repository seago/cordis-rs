//! 上下文（论文 Def 32 的 `Γ∞` 之 PR #3 最小版）。
//!
//! 承载：共效应表 `σ`（Def 22，PR #2 的 [`Store`]）与本层累加器 `ctx.dispose`
//! （Algorithm 1 第 17 行的 `ctx.dispose`；Def 6 的 `recover` 之宿主侧载体）。
//! PR #4 补充隔离/拦截投影与公开的 `set`/`get` 操作。
//!
//! 撤销顺序（审查 M-A 修复）：每步逆在**产出时**即推入累加器（应用序 LIFO，
//! 与论文 "prepending each new inverse therefore yields LIFO recovery" 一致），
//! 嵌套效应（外层迭代步骤中注册的内层效应）自然按应用逆序交错。

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

use crate::effect::{Disposer, EffectIter, Step, StepGuard, execute};
use crate::store::Store;

/// 记录迭代器：把内层迭代器的每步逆在产出时即推入本上下文累加器，
/// 并返回一个共享同一 armed 语义的等价闭包给 execute 折叠。
struct PushingIter {
    inner: Box<dyn EffectIter>,
    ctx: Rc<Context>,
}

impl EffectIter for PushingIter {
    fn next(&mut self) -> Step {
        match self.inner.next() {
            Step::Yielded(inv) => {
                let (exec, acc) = guarded_pair(inv);
                self.ctx.push_step(acc);
                Step::Yielded(exec)
            }
            Step::Finished(inv) => {
                let (exec, acc) = guarded_pair(inv);
                self.ctx.push_step(acc);
                Step::Finished(exec)
            }
        }
    }
}

/// 把一个步逆包装为两个等价闭包（共享同一 `StepGuard`）：
/// 一个交给 execute 折叠（手动撤销路径），一个推入上下文累加器（teardown 路径）。
fn guarded_pair(inv: Disposer) -> (Disposer, Disposer) {
    let guard = StepGuard::new(inv);
    (guard.disposer(), guard.disposer())
}

/// 上下文（PR #3 最小版）。
#[derive(Default)]
pub struct Context {
    store: RefCell<Store>,
    dispose: RefCell<Vec<Disposer>>,
}

impl Context {
    /// 空上下文。
    pub fn new() -> Self {
        Self::default()
    }

    /// 共效应表的可变访问（仅测试使用；PR #4 提供公开的 `set`/`get`）。
    #[cfg(test)]
    pub(crate) fn store_cell(&self) -> &RefCell<Store> {
        &self.store
    }

    /// 共效应表的只读视图。
    ///
    /// **借用警告（审查 m-A）**：持有返回的 `Ref` 期间调用 [`Context::effect`]
    /// 会触发 `RefCell` 双重借用 panic（effect 内部需要 `borrow_mut`）；需要
    /// 在 effect 之前结束读取借用。
    pub fn store(&self) -> Ref<'_, Store> {
        self.store.borrow()
    }

    /// `ctx.effect(callback)`（Algorithm 1 第 9–18 行的同步核心）。
    ///
    /// 以 `callback` 构造效应迭代器并立即执行至完成；**每步逆在产出时即推入
    /// 本上下文累加器**（应用序 LIFO，嵌套效应正确交错，审查 M-A）。返回的
    /// disposer 撤销本效应的全部步骤（与累加器路径共享每步 armed 句柄，至多
    /// 生效一次）；armed 标志（Algorithm 1 第 10–11 行）当前仅作 guard 输入，
    /// PR #5 async 时代实现「dispose 中断在途迭代」。
    ///
    /// **调用方需持有 `Rc<Context>`**（`self: &Rc<Self>`）：回调需要把上下文
    /// 克隆进迭代器闭包（'static 约束）。
    ///
    /// **迭代器必须有限终止**（审查 M-B，见 [`crate::effect`] 模块文档）。
    pub fn effect(self: &Rc<Self>, callback: impl FnOnce() -> Box<dyn EffectIter>) -> Disposer {
        let armed = Rc::new(Cell::new(true));
        let guard = {
            let armed = Rc::clone(&armed);
            move || armed.get()
        };
        let composite = execute(
            Box::new(PushingIter {
                inner: callback(),
                ctx: Rc::clone(self),
            }),
            guard,
        );
        let armed = Rc::clone(&armed);
        Box::new(move || {
            armed.set(false);
            composite();
        })
    }

    /// 把一步逆推入本上下文累加器（产出时调用，应用序）。
    fn push_step(&self, disposer: Disposer) {
        self.dispose.borrow_mut().push(disposer);
    }

    /// 运行本上下文累加器（LIFO 恢复全部已注册步骤；对应 Def 6 的 `recover`）。
    ///
    /// 累加器先排空再运行，disposer 运行期间允许在本上下文注册新效应
    /// （新效应不会在这次 dispose_all 中恢复）。
    pub fn dispose_all(&self) {
        let disposers: Vec<Disposer> = self.dispose.borrow_mut().drain(..).rev().collect();
        for disposer in disposers {
            disposer();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::once;
    use crate::key::Key;
    use crate::store::StoreError;

    struct KeyA;
    impl Key for KeyA {
        type Value = String;
        const SYMBOL: &'static str = "a";
    }

    struct KeyB;
    impl Key for KeyB {
        type Value = u32;
        const SYMBOL: &'static str = "b";
    }

    /// 绑定效应（Def 23 的 `set(k, v)` 之最小形式：绑定 + 逆 = 撤销绑定）。
    fn bind_effect<K: Key>(ctx: &Rc<Context>, value: K::Value) -> Disposer {
        ctx.effect(|| -> Box<dyn EffectIter> {
            let ctx = Rc::clone(ctx);
            Box::new(once(Box::new(move || {
                ctx.store_cell().borrow_mut().bind::<K>(value).unwrap();
                let ctx = Rc::clone(&ctx);
                Box::new(move || {
                    ctx.store_cell().borrow_mut().unbind::<K>().unwrap();
                }) as Disposer
            })))
        })
    }

    #[test]
    fn thm16_lifo_recovery_and_soundness_invariant() {
        // Thm 16(1)：逆序撤销时每一步恢复其应用时的状态；
        // Thm 16(2)：全部撤销后回到 γ₀（声音不变量 φ(γ) = γ₀）。
        let ctx = Rc::new(Context::new());
        let d1 = bind_effect::<KeyA>(&ctx, String::from("va"));
        let d2 = bind_effect::<KeyB>(&ctx, 7);

        // 两效应均已应用（execute 同步完成）。
        assert_eq!(ctx.store().get::<KeyA>().unwrap(), "va");
        assert_eq!(ctx.store().get::<KeyB>().unwrap(), &7);

        // 逆序撤销：先 d2（恢复 e2 应用前的状态：e1 的绑定仍在）。
        d2();
        assert!(matches!(
            ctx.store().get::<KeyB>(),
            Err(StoreError::NotBound(_))
        ));
        assert_eq!(
            ctx.store().get::<KeyA>().unwrap(),
            "va",
            "中间态：e1 不受影响"
        );

        // 再 d1 → 回到 γ₀。
        d1();
        assert!(matches!(
            ctx.store().get::<KeyA>(),
            Err(StoreError::NotBound(_))
        ));
        assert!(
            ctx.store().symbols().next().is_none(),
            "声音不变量：store 为空 = γ₀"
        );
    }

    #[test]
    fn accumulator_reverts_all_effects_lifo() {
        // 累加器（dispose_all）按 LIFO 恢复全部注册效应（Thm 16(2)）。
        let ctx = Rc::new(Context::new());
        // 故意忽略返回的 disposer（累加器已持有同一步的等价闭包）：
        // 编译器视 Box<dyn FnOnce()> 为 must_use（unused boxed FnOnce trait
        // object 警告），用 drop 表明意图。
        drop(bind_effect::<KeyA>(&ctx, String::from("va")));
        drop(bind_effect::<KeyB>(&ctx, 7));
        assert_eq!(ctx.store().symbols().count(), 2);

        ctx.dispose_all();
        assert_eq!(ctx.store().symbols().count(), 0, "回到 γ₀");
    }

    #[test]
    fn disposer_is_idempotent() {
        // Algorithm 1 第 13–14 行：armed 使撤销至多生效一次。
        // 返回的 disposer 为 FnOnce：直接二次调用是编译期错误（比论文的运行时
        // guard 更强）；运行期幂等针对「返回路径与累加器路径共享每步句柄」。
        let ctx = Rc::new(Context::new());
        let d = bind_effect::<KeyA>(&ctx, String::from("va"));
        d(); // 消耗返回的 disposer
        assert!(matches!(
            ctx.store().get::<KeyA>(),
            Err(StoreError::NotBound(_))
        ));
        // 累加器中的等价闭包因共享句柄而 no-op：不得 panic、不得重复撤销。
        ctx.dispose_all();
        assert!(matches!(
            ctx.store().get::<KeyA>(),
            Err(StoreError::NotBound(_))
        ));
    }

    #[test]
    fn effect_registers_into_context_accumulator() {
        // 不持有返回的 disposer，仅靠累加器即可恢复（fiber 卸载路径，Alg 5 第 26 行）。
        let ctx = Rc::new(Context::new());
        drop(ctx.effect(|| -> Box<dyn EffectIter> {
            let ctx = Rc::clone(&ctx);
            Box::new(once(Box::new(move || {
                ctx.store_cell()
                    .borrow_mut()
                    .bind::<KeyA>(String::from("va"))
                    .unwrap();
                let ctx = Rc::clone(&ctx);
                Box::new(move || {
                    ctx.store_cell().borrow_mut().unbind::<KeyA>().unwrap();
                }) as Disposer
            })))
        }));
        assert!(ctx.store().get::<KeyA>().is_ok());
        ctx.dispose_all();
        assert!(matches!(
            ctx.store().get::<KeyA>(),
            Err(StoreError::NotBound(_))
        ));
    }

    struct KeyK1;
    impl Key for KeyK1 {
        type Value = usize;
        const SYMBOL: &'static str = "k1";
    }

    struct KeyK2;
    impl Key for KeyK2 {
        type Value = usize;
        const SYMBOL: &'static str = "k2";
    }

    struct KeyK3;
    impl Key for KeyK3 {
        type Value = usize;
        const SYMBOL: &'static str = "k3";
    }

    /// 审查 M-A 回归：内层效应注册于外层迭代步骤之间，
    /// `dispose_all` 必须按应用逆序交错撤销（Thm 16：E2 → E3 → E1）。
    #[test]
    fn nested_effect_reverts_in_application_order() {
        let ctx = Rc::new(Context::new());
        let log = Rc::new(RefCell::new(Vec::<String>::new()));

        struct Outer {
            ctx: Rc<Context>,
            log: Rc<RefCell<Vec<String>>>,
            step: usize,
        }
        impl EffectIter for Outer {
            fn next(&mut self) -> Step {
                self.step += 1;
                match self.step {
                    1 => {
                        // E1：绑定 k1（应用 t1）。
                        self.ctx.store_cell().borrow_mut().bind::<KeyK1>(1).unwrap();
                        let (ctx, log) = (Rc::clone(&self.ctx), Rc::clone(&self.log));
                        Step::Yielded(Box::new(move || {
                            log.borrow_mut().push("o1".into());
                            ctx.store_cell().borrow_mut().unbind::<KeyK1>().unwrap();
                        }) as Disposer)
                    }
                    2 => {
                        // E3：本步应用代码内注册内层效应（应用 t4，先于 E2）。
                        let inner_ctx = Rc::clone(&self.ctx);
                        let inner_log = Rc::clone(&self.log);
                        drop(self.ctx.effect(|| -> Box<dyn EffectIter> {
                            let ctx = Rc::clone(&inner_ctx);
                            let log = Rc::clone(&inner_log);
                            Box::new(once(Box::new(move || {
                                ctx.store_cell().borrow_mut().bind::<KeyK2>(2).unwrap();
                                let (ctx, log) = (Rc::clone(&ctx), Rc::clone(&log));
                                Box::new(move || {
                                    log.borrow_mut().push("i1".into());
                                    ctx.store_cell().borrow_mut().unbind::<KeyK2>().unwrap();
                                }) as Disposer
                            })))
                        }));
                        // E2：绑定 k3（应用 t5）。
                        self.ctx.store_cell().borrow_mut().bind::<KeyK3>(3).unwrap();
                        let (ctx, log) = (Rc::clone(&self.ctx), Rc::clone(&self.log));
                        Step::Finished(Box::new(move || {
                            log.borrow_mut().push("o2".into());
                            ctx.store_cell().borrow_mut().unbind::<KeyK3>().unwrap();
                        }) as Disposer)
                    }
                    _ => panic!("迭代器必须终止（协议约束，审查 M-B）"),
                }
            }
        }

        drop(ctx.effect(|| -> Box<dyn EffectIter> {
            Box::new(Outer {
                ctx: Rc::clone(&ctx),
                log: Rc::clone(&log),
                step: 0,
            })
        }));

        assert_eq!(ctx.store().symbols().count(), 3, "k1、k2、k3 均已应用");
        ctx.dispose_all();
        assert_eq!(
            *log.borrow(),
            vec!["o2", "i1", "o1"],
            "撤销按应用逆序交错：E2 → E3 → E1（而非外层整组先撤）"
        );
        assert_eq!(ctx.store().symbols().count(), 0, "回到 γ₀");
    }
}
