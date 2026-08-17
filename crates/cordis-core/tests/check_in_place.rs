//! G8 + G9（TS-REFERENCE-GAP）：`set_in_place` 就地改值 + `set_with_check`
//! 可用性谓词。
//!
//! - G8：论文 "overwriting its own binding in place is therefore not
//!   observed"——`set_in_place` 替换本 fiber 绑定值，**不 notify、不追踪**
//!   （idΓ 式；teardown 不恢复旧值）；非本 fiber 绑定 / 未绑定 → `Err`。
//! - G9：TS `provide(name, value, check)`——绑定携带可用性谓词，求值为假
//!   时依赖者解析不到（`provider_of` 每次求值，谓词须纯；变化即时生效，
//!   无需 notify——依赖者经 refresh 感知）。

use cordis_core::effect::EffectIter;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{Component, Context, Disposer, Key, Runtime, Step, StoreError};
use std::cell::Cell;
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

/// db 提供者（config = 初始值字符串）。
struct Provider;

impl Component for Provider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        spec(&["db"])
    }
    fn apply(&self, ctx: Rc<Context>, config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        let value = config.downcast_ref::<String>().expect("String").clone();
        Box::new(once_finished(move || {
            ctx.set::<DbKey>(value).expect("绑定 db")
        }))
    }
}

/// 注入 db 的消费者（无自身效应；激活 = 依赖可用性探针）。
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

// ── G8：set_in_place ──────────────────────────────────────────────────

/// 就地改值：绑定值替换、**不 notify**（消费者不重激活、fiber 不变）、
/// 不追踪（逆不存在——撤销不恢复旧值）。
#[test]
fn set_in_place_replaces_value_without_notify() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let provider = root
        .use_component(Rc::new(Provider), Rc::new("v0".to_string()))
        .expect("provider 注册");
    let consumer = root
        .use_component(Rc::new(Consumer), Rc::new(()))
        .expect("consumer 注册");
    let consumer_id = consumer.id();
    assert_eq!(db_value(&runtime), "v0", "初始绑定");

    // 提供者侧就地改值（同 fiber ctx）。
    provider
        .ctx()
        .set_in_place::<DbKey>("v1".to_string())
        .expect("就地改值");
    assert_eq!(db_value(&runtime), "v1", "值已替换");
    assert_eq!(consumer.id(), consumer_id, "不 notify：消费者 fiber 不变");
    assert!(
        matches!(&*consumer.state(), cordis_core::FiberState::Active { .. }),
        "消费者未重激活（原地不动）"
    );
    assert!(runtime.is_quiet(), "静止");

    // 非本 fiber 绑定 → Err（TS "cannot set property in multiple fibers"）：
    // root ctx（fiber=None，无纪律检查）上就地改值——绑定存在但安装者
    // ≠ root → 拒绝。
    assert!(
        root.use_component(Rc::new(Provider), Rc::new("x".to_string()))
            .is_err(),
        "第二个 db 提供者应 ProvisionClash"
    );
    let err = root.set_in_place::<DbKey>("v2".to_string());
    assert!(
        matches!(err, Err(StoreError::AlreadyBound(_))),
        "非安装者 ctx 就地改值被拒"
    );
    assert_eq!(db_value(&runtime), "v1", "值未被篡改");
    let _ = consumer;
}

/// 未绑定键就地改值 → Err（NotBound）。
#[test]
fn set_in_place_unbound_errors() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let provider = root
        .use_component(Rc::new(Provider), Rc::new("v0".to_string()))
        .expect("provider 注册");
    let _ = provider;
    let store = runtime.store();
    store.get_value(Symbol::intern("db")).expect("绑定存在");
    drop(store);
    // 撤销后未绑定 → NotBound。
    let runtime2 = Rc::new(Runtime::new());
    let root2 = runtime2.context();
    let p2 = root2
        .use_component(Rc::new(Provider), Rc::new("v0".to_string()))
        .expect("provider 注册");
    p2.retire();
    runtime2.remove_fiber(p2.id()).expect("移除");
    let err = root2.set_in_place::<DbKey>("v9".to_string());
    assert!(
        matches!(err, Err(StoreError::NotBound(_))),
        "未绑定 → NotBound"
    );
}

// ── G9：set_with_check ────────────────────────────────────────────────

/// G9 撤销：set_with_check 的 disposer 撤销绑定（unbind + notify）→
/// 依赖者恢复 Inactive（REVIEW-54814d0 nit-4）。
#[test]
fn check_binding_dispose_reverts_dependents() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let flag = Rc::new(Cell::new(true));
    let flag2 = Rc::clone(&flag);
    let provider = root
        .use_component(Rc::new(Provider), Rc::new("v0".to_string()))
        .expect("provider 注册");
    let consumer = root
        .use_component(Rc::new(Consumer), Rc::new(()))
        .expect("consumer 注册");
    provider.ctx().dispose_all();
    let disposer = provider
        .ctx()
        .set_with_check::<DbKey>("v1".to_string(), move || flag2.get())
        .expect("check 绑定");
    assert!(matches!(
        &*consumer.state(),
        cordis_core::FiberState::Active { .. }
    ));

    // 撤销绑定（同 set 的逆）→ notify → 依赖者 Inactive。
    disposer();
    assert!(
        matches!(&*consumer.state(), cordis_core::FiberState::Inactive(_)),
        "撤销 check 绑定 → 依赖者停用"
    );
    assert!(runtime.is_quiet(), "静止");
}

/// G8 + G9 交互：`set_in_place` 只替换值，**check 谓词保留**（REVIEW-
/// 54814d0 nit-4）——就地改值后谓词仍门控依赖者。
#[test]
fn set_in_place_keeps_check_predicate() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let flag = Rc::new(Cell::new(true));
    let flag2 = Rc::clone(&flag);
    let provider = root
        .use_component(Rc::new(Provider), Rc::new("v0".to_string()))
        .expect("provider 注册");
    let consumer = root
        .use_component(Rc::new(Consumer), Rc::new(()))
        .expect("consumer 注册");
    provider.ctx().dispose_all();
    let _d = provider
        .ctx()
        .set_with_check::<DbKey>("v1".to_string(), move || flag2.get())
        .expect("check 绑定");
    assert!(matches!(
        &*consumer.state(),
        cordis_core::FiberState::Active { .. }
    ));

    // 就地改值：值替换、check 保留。
    provider
        .ctx()
        .set_in_place::<DbKey>("v2".to_string())
        .expect("就地改值");
    assert_eq!(db_value(&runtime), "v2", "值已替换");
    assert!(
        matches!(&*consumer.state(), cordis_core::FiberState::Active { .. }),
        "check 保留：true 仍激活"
    );

    // check 翻转 → 依赖者仍被门控。
    flag.set(false);
    runtime.refresh(&consumer);
    assert!(
        matches!(&*consumer.state(), cordis_core::FiberState::Inactive(_)),
        "set_in_place 后 check 仍生效"
    );
}

/// check 为假 → 依赖者解析不到（Inactive）；为真 → 恢复。变化即时生效
///（谓词每次求值），依赖者经 refresh 感知。
#[test]
fn check_predicate_gates_dependents() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let flag = Rc::new(Cell::new(true));
    let flag2 = Rc::clone(&flag);
    // 带 check 的提供者：手动 use_component 后在 ctx 上 set_with_check。
    let provider = root
        .use_component(Rc::new(Provider), Rc::new("v0".to_string()))
        .expect("provider 注册");
    let consumer = root
        .use_component(Rc::new(Consumer), Rc::new(()))
        .expect("consumer 注册");
    // 换成 check 绑定：撤销原绑定（dispose_all），再 set_with_check。
    provider.ctx().dispose_all();
    let _check_disposer = provider
        .ctx()
        .set_with_check::<DbKey>("v1".to_string(), move || flag2.get())
        .expect("check 绑定");
    assert!(
        matches!(&*consumer.state(), cordis_core::FiberState::Active { .. }),
        "check=true：消费者激活"
    );

    // check 变假 → 依赖者经 refresh 停用（provider_of 求值谓词）。
    flag.set(false);
    runtime.refresh(&consumer);
    assert!(
        matches!(&*consumer.state(), cordis_core::FiberState::Inactive(_)),
        "check=false：消费者 Inactive（绑定视为未提供）"
    );

    // check 变真 → refresh 恢复。
    flag.set(true);
    runtime.refresh(&consumer);
    assert!(
        matches!(&*consumer.state(), cordis_core::FiberState::Active { .. }),
        "check=true：消费者恢复"
    );
    assert!(runtime.is_quiet(), "静止");
}
