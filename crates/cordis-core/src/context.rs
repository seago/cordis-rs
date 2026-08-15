//! 上下文（论文 Def 32 的 `Γ∞` 之 PR #3 最小版）。
//!
//! 承载：共效应表 `σ`（Def 22，PR #2 的 [`Store`]）与本层累加器 `ctx.dispose`
//! （Algorithm 1 第 17 行的 `ctx.dispose`；Def 6 的 `recover` 之宿主侧载体）。
//! PR #4 补充隔离/拦截投影与公开的 `set`/`get` 操作。

use std::cell::{Ref, RefCell};
use std::rc::Rc;

use crate::effect::{Disposer, EffectHandle, EffectIter, execute};
use crate::store::Store;

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
    pub fn store(&self) -> Ref<'_, Store> {
        self.store.borrow()
    }

    /// `ctx.effect(callback)`（Algorithm 1 第 9–18 行）。
    ///
    /// 以 `callback` 构造效应迭代器并立即执行至完成（同步核心）；返回的
    /// disposer 撤销该效应且**至多生效一次**（armed 幂等）。同一 disposer
    /// 同时被组合进本上下文的累加器（`ctx.dispose ← dispose ∘ ctx.dispose`），
    /// 使 [`Context::dispose_all`] 能恢复本上下文上的全部效应。
    ///
    /// 组合时机：论文伪代码置于 `dispose` 内部；本实现于注册时入栈，
    /// armed 幂等保证两者可观察等价（THEORY-MAP「已知偏差」）。
    pub fn effect(self: &Rc<Self>, callback: impl FnOnce() -> Box<dyn EffectIter>) -> Disposer {
        let handle = EffectHandle::new();
        let guard = {
            let handle = Rc::clone(&handle);
            move || handle.is_armed()
        };
        handle.install(execute(callback(), guard));

        let disposer = |handle: &Rc<EffectHandle>| -> Disposer {
            let handle = Rc::clone(handle);
            Box::new(move || handle.dispose())
        };
        let returned = disposer(&handle);
        self.dispose.borrow_mut().push(disposer(&handle));
        returned
    }

    /// 运行本上下文累加器（LIFO 恢复全部已注册效应；对应 Def 6 的 `recover`）。
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
        // 故意忽略返回的 disposer（累加器已持有同一句柄的副本）：
        // Box<dyn FnOnce> 自带 must_use，用 drop 表明意图。
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
        // guard 更强）；运行期幂等针对「返回副本与累加器副本共享同一句柄」的路径。
        let ctx = Rc::new(Context::new());
        let d = bind_effect::<KeyA>(&ctx, String::from("va"));
        d(); // 消耗返回的 disposer
        assert!(matches!(
            ctx.store().get::<KeyA>(),
            Err(StoreError::NotBound(_))
        ));
        // 累加器中的同一句柄副本因 armed 而 no-op：不得 panic、不得重复撤销。
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
}
