//! M2-PR1：**L-Raise 失败模型**（处置⑤ + ⑧ 落地）直接测试。
//!
//! §4.3.4 𝔈fail：组件迭代器以 [`FiberError::raise`] 抛出的失败载荷不再
//! 以宿主 panic 传播——`reload` 捕获后：目标置 ⊥ → 卸载路径恢复已完成
//! 步骤（已完成步骤的逆已在产出时入 ctx 累加器）→ 终态
//! `Inactive(Some(ζ))`；`is_quiet` 的 ζ 析取（Def 49 式 (45)）使失败
//! fiber 静止。

use cordis_core::effect::EffectIter;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{
    Component, Context, Disposer, Fiber, FiberError, FiberState, Key, Runtime, Step,
};
use std::cell::Cell;
use std::rc::Rc;

struct K0;
impl Key for K0 {
    type Value = u8;
    const SYMBOL: &'static str = "k0";
}

struct K1;
impl Key for K1 {
    type Value = u8;
    const SYMBOL: &'static str = "k1";
}

fn k0() -> Symbol {
    Symbol::intern("k0")
}

fn use_at(
    root: &Rc<Context>,
    component: impl Component + 'static,
) -> Result<Rc<Fiber>, cordis_core::RegistryError> {
    root.use_component(Rc::new(component), Rc::new(()))
}

/// 失败组件：第一步绑定 k0（完成），第二步 raise。
struct FailAfterFirst;

impl Component for FailAfterFirst {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        [k0()].into_iter().collect()
    }
    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(FailIter { ctx, step: 0 })
    }
}

struct FailIter {
    ctx: Rc<Context>,
    step: usize,
}

impl EffectIter for FailIter {
    fn next(&mut self) -> Step {
        match self.step {
            0 => {
                self.step = 1;
                let inverse = self.ctx.set::<K0>(1).expect("绑定 k0");
                Step::Yielded(inverse)
            }
            _ => FiberError::new("迭代器第二步失败（测试 raise）").raise(),
        }
    }
}

/// L-Raise：失败 → 错误 outcome（非宿主 panic）+ 已完成步骤恢复 + 静止。
#[test]
fn l_raise_records_error_outcome_and_recovers_completed_steps() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    // use_component 返回 Ok：失败以 fiber 状态（ζ）承载，不 panic。
    let fiber = use_at(&root, FailAfterFirst).expect("实例化成功（失败不 panic）");
    assert!(
        matches!(&*fiber.state(), FiberState::Inactive(Some(err)) if err.to_string().contains("第二步失败")),
        "L-Raise：fiber 以错误 outcome 终态：{:?}",
        *fiber.state()
    );
    // 已完成步骤已恢复：k0 绑定全清。
    assert!(
        runtime.store().symbols().next().is_none(),
        "已完成步骤（第一步绑定）已恢复"
    );
    // ζ 析取：失败 fiber 静止（Def 49 式 (45)）。
    assert!(runtime.is_quiet(), "失败 fiber 静止（ζ ≠ ⊥）");
    assert!(runtime.active_fibers().is_empty(), "无活跃 fiber");
}

/// **负向判别（审查 nit1，REVIEW-32a913d）**：非 `FiberError` 载荷的 panic
/// （宿主 bug）不被吞成失败 outcome——`reload` 的 `catch_unwind` 经
/// `downcast::<FiberError>()` 失败后 `resume_unwind` 原样重抛。
#[test]
#[should_panic(expected = "宿主 bug：非 FiberError 载荷")]
fn non_fiber_error_panic_propagates_as_host_bug() {
    struct HostBug;
    impl Component for HostBug {
        fn inject(&self) -> KeySet {
            KeySet::new()
        }
        fn provide(&self) -> KeySet {
            KeySet::new()
        }
        fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
            Box::new(HostBugIter)
        }
    }
    struct HostBugIter;
    impl EffectIter for HostBugIter {
        fn next(&mut self) -> Step {
            panic!("宿主 bug：非 FiberError 载荷")
        }
    }

    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    // should_panic：重抛（use_component 整体 panic）。
    let _ = use_at(&root, HostBug).expect("不应返回");
}

/// L-Raise 后可重试：失败 fiber 的目标在提供者出现时重新激活（L-Begin）。
#[test]
fn l_raise_failed_fiber_can_retry_activation() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    // 失败组件注入 k1（由外部提供者供给）——**首次**激活在 step 时失败，
    // 提供者退役再重装后 fiber 重新尝试并成功（一次性失败：跨激活共享
    // 尝试计数）。
    struct FailOnce(Rc<Cell<u32>>);
    impl Component for FailOnce {
        fn inject(&self) -> KeySet {
            [Symbol::intern("k1")].into_iter().collect()
        }
        fn provide(&self) -> KeySet {
            KeySet::new()
        }
        fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
            let attempt = self.0.get();
            self.0.set(attempt + 1);
            if attempt == 0 {
                Box::new(once_fail())
            } else {
                Box::new(once_finished(|| Box::new(|| {}) as Disposer))
            }
        }
    }
    fn once_fail() -> impl EffectIter {
        struct FailOnceIter(bool);
        impl EffectIter for FailOnceIter {
            fn next(&mut self) -> Step {
                if self.0 {
                    self.0 = false;
                    FiberError::new("首次激活失败").raise()
                } else {
                    unreachable!()
                }
            }
        }
        FailOnceIter(true)
    }

    struct Provider;
    impl Component for Provider {
        fn inject(&self) -> KeySet {
            KeySet::new()
        }
        fn provide(&self) -> KeySet {
            [Symbol::intern("k1")].into_iter().collect()
        }
        fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
            Box::new(once_finished(move || ctx.set::<K1>(1).expect("绑定 k1")))
        }
    }
    let p = use_at(&root, Provider).expect("provider 实例化");
    let f = use_at(&root, FailOnce(Rc::new(Cell::new(0)))).expect("fiber 实例化");
    assert!(
        matches!(&*f.state(), FiberState::Inactive(Some(_))),
        "首次激活失败：{:?}",
        *f.state()
    );
    assert!(runtime.is_quiet());

    // 提供者退役 → 重装：fiber 目标翻转 → 重新尝试。
    p.retire();
    runtime.remove_fiber(p.id()).expect("移除 provider");
    let p2 = use_at(&root, Provider).expect("provider 重装");
    assert!(
        matches!(&*f.state(), FiberState::Active { .. }),
        "重试后激活：{:?}",
        *f.state()
    );
    p2.retire();
    assert!(runtime.is_quiet());
}

/// **失败卸载路径（审查 nit9，REVIEW-32a913d）**：失败 fiber 的卸载
/// 恢复已完成步骤并通知依赖者（Thm 63 顺序）——提供者第二次激活失败时，
/// 消费方保持停用、绑定全清、静止。
///
/// 注：同步核心中失败激活的已完成步骤**立即**恢复（卸载路径），故
/// "失败瞬间有活跃依赖者"不可达（依赖者只能在激活完成后激活）；
/// 本测试断言失败卸载路径的可观测结果（错误 outcome、依赖者停用、
/// store 全清、is_quiet）。
#[test]
fn l_raise_failure_unload_recovers_and_notifies() {
    // 尝试计数：第 1 次激活成功（绑定 k1），第 2 次激活绑定 k1 后失败。
    let attempts = Rc::new(Cell::new(0u32));

    struct FailProvider(Rc<Cell<u32>>);
    impl Component for FailProvider {
        fn inject(&self) -> KeySet {
            KeySet::new()
        }
        fn provide(&self) -> KeySet {
            [Symbol::intern("k1")].into_iter().collect()
        }
        fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
            let attempt = self.0.get();
            self.0.set(attempt + 1);
            Box::new(FailBindIter {
                ctx,
                attempt,
                step: 0,
            })
        }
    }
    struct FailBindIter {
        ctx: Rc<Context>,
        attempt: u32,
        step: usize,
    }
    impl EffectIter for FailBindIter {
        fn next(&mut self) -> Step {
            match self.step {
                0 => {
                    self.step = 1;
                    let inverse = self.ctx.set::<K1>(1).expect("绑定 k1");
                    Step::Yielded(inverse)
                }
                _ if self.attempt == 0 => Step::Finished(Box::new(|| {}) as Disposer),
                _ => FiberError::new("第二次激活失败").raise(),
            }
        }
    }

    struct Consumer;
    impl Component for Consumer {
        fn inject(&self) -> KeySet {
            [Symbol::intern("k1")].into_iter().collect()
        }
        fn provide(&self) -> KeySet {
            KeySet::new()
        }
        fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
            Box::new(once_finished(|| Box::new(|| {}) as Disposer))
        }
    }

    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let provider = use_at(&root, FailProvider(Rc::clone(&attempts))).expect("provider 实例化");
    let consumer = use_at(&root, Consumer).expect("consumer 实例化");
    assert!(
        active_state(&consumer),
        "consumer 依赖提供者激活（第 1 次激活成功）"
    );
    assert!(runtime.store().contains(Symbol::intern("k1")), "k1 绑定");

    // 退役 → 重装（目标翻转驱动第 2 次激活）：绑定 k1 后失败。
    provider.retire();
    runtime.remove_fiber(provider.id()).expect("移除 provider");
    let provider2 = use_at(&root, FailProvider(attempts)).expect("provider2 实例化");
    assert!(
        matches!(&*provider2.state(), FiberState::Inactive(Some(err)) if err.to_string().contains("第二次激活失败")),
        "第 2 次激活失败 outcome：{:?}",
        *provider2.state()
    );
    // 失败卸载路径：依赖者保持停用、绑定全清、静止。
    assert!(
        matches!(&*consumer.state(), FiberState::Inactive(_)),
        "提供者失败卸载 → consumer 停用"
    );
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
    assert!(runtime.is_quiet(), "静止（失败亦静止）");
}

fn active_state(f: &Rc<Fiber>) -> bool {
    matches!(&*f.state(), FiberState::Active { .. })
}

fn once_finished(bind: impl FnOnce() -> Disposer + 'static) -> impl EffectIter {
    struct Done(Option<Box<dyn FnOnce() -> Disposer>>);
    impl EffectIter for Done {
        fn next(&mut self) -> Step {
            match self.0.take() {
                Some(bind) => Step::Finished(bind()),
                None => unreachable!("迭代器已完成"),
            }
        }
    }
    Done(Some(Box::new(bind)))
}
