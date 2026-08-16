//! Cordis 进程内组件后端（PLAN §4.3）。
//!
//! 受信组件直接以 Rust 代码内联到宿主进程，共享 trait 对象与类型化键
//! （ADR-0004）。声明式 DX（`#[component]` 宏）由 `cordis-macro` 提供；
//! 本 crate 提供组合效应程序的辅助（[`with_ctx`]）。

#![deny(missing_docs)]

pub use cordis_core::{Component, Context, Disposer, EffectIter, Key, KeySet, Symbol};
use std::rc::Rc;

/// 把「以 ctx 为参数的步骤」包装为单步效应迭代器（Def 8 的 `𝔈Γ` 退化
/// 情形；等价于 `once(Box::new(move || step(&ctx)))`）。
///
/// 适用于 `Component::apply` 中绑定单个键等单步效应：
///
/// ```ignore
/// fn apply_impl(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
///     cordis_native::with_ctx(ctx, |ctx| {
///         ctx.set::<DbKey>(Database::connect(&self.url)).expect("绑定 db")
///     })
/// }
/// ```
pub fn with_ctx(
    ctx: Rc<Context>,
    step: impl FnOnce(&Rc<Context>) -> Disposer + 'static,
) -> impl EffectIter {
    cordis_core::once(Box::new(move || step(&ctx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cordis_core::{FiberState, Runtime};
    use std::any::Any;

    struct GreetKey;
    impl Key for GreetKey {
        type Value = String;
        const SYMBOL: &'static str = "greet";
    }

    struct Greeter;

    impl Component for Greeter {
        fn inject(&self) -> KeySet {
            KeySet::new()
        }
        fn provide(&self) -> KeySet {
            [Symbol::intern("greet")].into_iter().collect()
        }
        fn apply(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
            Box::new(with_ctx(ctx, |ctx| {
                ctx.set::<GreetKey>("hi".into()).expect("绑定 greet")
            }))
        }
    }

    #[test]
    fn with_ctx_component_activates_and_binds() {
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let fiber = root
            .use_component(Rc::new(Greeter), Rc::new(()))
            .expect("实例化");
        assert!(
            matches!(&*fiber.state(), FiberState::Active { .. }),
            "无依赖组件立即激活"
        );
        assert_eq!(root.get::<GreetKey>().unwrap().as_str(), "hi");
    }
}
