//! G1（TS-REFERENCE-GAP）：§5.2.1 双向绑定组件侧——`Fiber::update` 就地
//! 更新配置（TS `Fiber.update` 参照，fiber.ts:476）。
//!
//! 语义直证：换 config 闭包 → 逆转当前效应（依赖者级联停用，Thm 63 序）
//! → 以新配置重跑（fiber **身份保留**，非重建）→ 绑定重装（依赖者级联
//! 恢复）；写回观察者（`Runtime::set_update_hook`）在重跑前以新 config
//! 触发；非 Active fiber 调用 = 协议违反（panic = bug）。

use cordis_core::effect::EffectIter;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{Component, Context, Disposer, Fiber, Key, Runtime, Step};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct DbKey;
impl Key for DbKey {
    type Value = String;
    const SYMBOL: &'static str = "db";
}

fn spec(names: &[&str]) -> KeySet {
    names.iter().map(|s| Symbol::intern(s)).collect()
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

/// 效应重执行计数器（config 载体；直证"就地重跑恰好一次"）。
#[derive(Clone)]
struct ExecCount(Rc<Cell<u64>>);

/// db 提供者：config = 绑定值；每次执行计数 +1。
struct Provider;

impl Component for Provider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        spec(&["db"])
    }
    fn apply(&self, ctx: Rc<Context>, config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        let count = config
            .downcast_ref::<ExecCount>()
            .expect("ExecCount")
            .clone();
        Box::new(once_finished(move || {
            count.0.set(count.0.get() + 1);
            let value = ctx
                .get::<DbKey>()
                .map(|v| v.clone())
                .unwrap_or_else(|_| "v0".to_string());
            ctx.set::<DbKey>(value).expect("绑定 db")
        }))
    }
}

/// 注入 db 的消费者（无自身效应；激活 = 依赖可用性的探针）。
struct Consumer;

impl Component for Consumer {
    fn inject(&self) -> KeySet {
        spec(&["db"])
    }
    fn provide(&self) -> KeySet {
        KeySet::new()
    }
    fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(once_finished(|| Box::new(|| {}) as Disposer))
    }
}

fn db_value(runtime: &Runtime) -> String {
    let store = runtime.store();
    store
        .get_value(Symbol::intern("db"))
        .expect("db 绑定")
        .downcast_ref::<String>()
        .expect("String")
        .clone()
}

/// 装配：provider（提供 db）+ consumer（注入 db）全激活。
fn setup() -> (Rc<Runtime>, Rc<Fiber>, Rc<Fiber>, ExecCount) {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let count = ExecCount(Rc::new(Cell::new(0)));
    let provider = root
        .use_component(Rc::new(Provider), Rc::new(count.clone()))
        .expect("provider 注册");
    let consumer = root
        .use_component(Rc::new(Consumer), Rc::new(()))
        .expect("consumer 注册");
    assert!(matches!(
        &*provider.state(),
        cordis_core::FiberState::Active { .. }
    ));
    assert!(matches!(
        &*consumer.state(),
        cordis_core::FiberState::Active { .. }
    ));
    (runtime, provider, consumer, count)
}

/// `Fiber::update`：就地重跑、fiber 身份保留、依赖者级联恢复、绑定更新。
#[test]
fn fiber_update_reruns_in_place_keeping_identity() {
    let (runtime, provider, consumer, count) = setup();
    let provider_id = provider.id();
    assert_eq!(db_value(&runtime), "v0", "初始绑定（v0 为回退值）");
    let base = count.0.get();

    // 换配置重跑：提供者效应重执行恰 1 次，fiber 身份不变。
    provider.update(Rc::new(ExecCount(Rc::clone(&count.0))));
    assert_eq!(
        provider.id(),
        provider_id,
        "update 是就地重跑（fiber 身份保留，非重建）"
    );
    assert_eq!(count.0.get() - base, 1, "效应恰重跑 1 次");
    assert!(
        matches!(&*provider.state(), cordis_core::FiberState::Active { .. }),
        "重跑后 Active"
    );
    assert!(
        matches!(&*consumer.state(), cordis_core::FiberState::Active { .. }),
        "依赖者级联恢复（绑定撤销/重装后回到 Active）"
    );
    assert!(runtime.is_quiet(), "update 后静止");
}

/// 写回观察者：`set_update_hook` 在重跑前以新 config 触发（TS
/// `internal/update` 瀑布先写回后重启的序）。
#[test]
fn update_hook_fires_with_new_config_before_rerun() {
    let (runtime, provider, _consumer, _count) = setup();
    let seen: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let seen_hook = Rc::clone(&seen);
    runtime.set_update_hook(Some(Rc::new(
        move |_fiber: &Fiber, config: Rc<dyn std::any::Any>| {
            let value = config
                .downcast_ref::<ExecCount>()
                .expect("ExecCount")
                .0
                .get();
            *seen_hook.borrow_mut() = Some(format!("count={value}"));
        },
    )));
    let new_count = ExecCount(Rc::new(Cell::new(42)));
    provider.update(Rc::new(new_count));
    assert_eq!(
        seen.borrow().as_deref(),
        Some("count=42"),
        "观察者收到新 config（重跑前触发）"
    );
    assert!(runtime.is_quiet(), "静止");
}

/// 协议违反：非 Active fiber 上调用 update = panic（INACTIVE_EFFECT 同型）。
#[test]
#[should_panic(expected = "仅 Active fiber 可用")]
fn update_on_inactive_fiber_panics() {
    let (_runtime, provider, _consumer, _count) = setup();
    provider.retire();
    provider.update(Rc::new(ExecCount(Rc::new(Cell::new(1)))));
}
