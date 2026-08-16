//! M2-PR6（处置②）：**Algorithm 6 Proxy 中介访问**直接测试。
//!
//! `resolve` 沿 fiber 链向上解析：首个 committed 视图绑定 key 的 fiber
//! 授权（返回其承诺视图下解析的绑定值）；声明未提交 → `Inactive`；
//! 至 root 无声明 → `Undeclared`。读视图（committed）而非裸 store——
//! Thm 63 语义（teardown 中依赖仍可读）。

use cordis_core::effect::EffectIter;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{AccessError, Component, Context, Disposer, Key, Runtime, Step};
use std::rc::Rc;

struct DbKey;
impl Key for DbKey {
    type Value = String;
    const SYMBOL: &'static str = "db";
}

struct AppKey;
impl Key for AppKey {
    type Value = String;
    const SYMBOL: &'static str = "app";
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

/// db 提供者。
struct DbProvider;

impl Component for DbProvider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        spec(&["db"])
    }
    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(once_finished(move || {
            ctx.set::<DbKey>("pg".into()).expect("绑定 db")
        }))
    }
}

/// 注入 db 的消费者（无自身效应）。
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

/// 授权访问：消费者的 resolve 沿链读到提供者绑定（经自身 committed 视图）。
#[test]
fn resolve_authorizes_via_committed_view() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let _p = root
        .use_component(Rc::new(DbProvider), Rc::new(()))
        .expect("provider");
    let c = root
        .use_component(Rc::new(Consumer), Rc::new(()))
        .expect("consumer");
    assert!(
        matches!(&*c.state(), cordis_core::FiberState::Active { .. }),
        "consumer 激活"
    );

    let value = c.ctx().resolve::<DbKey>().expect("授权访问");
    assert_eq!(&*value, "pg", "resolve 返回绑定值");
}

/// INACTIVE_ACCESS：声明（inject）但未提交（提供者缺席 → 未加载）。
#[test]
fn resolve_raises_inactive_access() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let c = root
        .use_component(Rc::new(Consumer), Rc::new(()))
        .expect("consumer");
    assert!(
        matches!(&*c.state(), cordis_core::FiberState::Inactive(_)),
        "未激活"
    );

    assert_eq!(
        c.ctx().resolve::<DbKey>().unwrap_err(),
        AccessError::Inactive,
        "声明未提交 → INACTIVE_ACCESS"
    );
}

/// UNDECLARED_ACCESS：链上无任何声明（root 上下文直接访问）。
#[test]
fn resolve_raises_undeclared_access() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    assert_eq!(
        root.resolve::<AppKey>().unwrap_err(),
        AccessError::Undeclared,
        "root 无声明 → UNDECLARED_ACCESS"
    );
}

/// 链上行：访问 fiber 自身无该键声明（注入为空、committed 为空），沿
/// `fiber.parent` 上行到**祖先**（声明并已加载该键）授权（REVIEW-e8bd96e
/// major1 修正——原测试在子自身 committed 短路，未行使爬升）。
#[test]
fn resolve_climbs_fiber_chain() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    // 祖先链：root → DbProvider（提供 db）+ Parent（注入 db、已加载）；
    // Child（无任何注入）挂在 Parent 的 ctx 下。
    let _p = root
        .use_component(Rc::new(DbProvider), Rc::new(()))
        .expect("db 提供者");
    let parent = root
        .use_component(Rc::new(Consumer), Rc::new(()))
        .expect("父（注入 db、已加载）");
    assert!(
        matches!(&*parent.state(), cordis_core::FiberState::Active { .. }),
        "父激活"
    );
    let child = parent
        .ctx()
        .use_component(Rc::new(NoInject), Rc::new(()))
        .expect("子（无注入）");
    assert!(
        matches!(&*child.state(), cordis_core::FiberState::Active { .. }),
        "子激活"
    );

    // 子经 resolve 沿链：自身 committed 空（无注入）→ 无声明 → 爬升到
    // 父（committed 绑定 db）→ 授权。
    let value = child.ctx().resolve::<DbKey>().expect("沿链授权");
    assert_eq!(&*value, "pg", "子经父的承诺视图读到 db");
}

/// 无注入组件（链上行测试用：自身 committed 空、无声明）。
struct NoInject;

impl Component for NoInject {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        KeySet::new()
    }
    fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(once_finished(|| Box::new(|| {}) as Disposer))
    }
}
