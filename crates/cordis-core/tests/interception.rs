//! M2-PR2：**interception 求值形态**（处置①，Def 30/31）直接测试。
//!
//! - `get(k, μ) = σ(k)(μ ⊕ₖ ι(k))` 的元数据侧：组件声明 `d(k)` 与上下文
//!   携带 `ι(k)` 右偏合并（`ι` 优先，§6.3"外层上下文约束组件"语义）；
//! - provider 函数 `σ(k)` 本实现为常量函数（返回绑定值）——求值结果 =
//!   绑定值，合并元数据经 [`Context::get_meta`] 暴露（provider 函数形态
//!   的价值核心随 typed world 评估，处置⑨）；
//! - [`Context::intercept_in_place`]：loader 的 `intercept` 字段分派
//!   （§5.2.1）——就地更新不触发 reload。

use cordis_core::effect::EffectIter;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{Component, Context, Disposer, FiberState, InterceptMeta, Key, Runtime, Step};
use std::collections::BTreeSet;
use std::rc::Rc;

// ── 测试元数据：路径 + 只读标志（⊕：路径取并、read_only 右偏）───────────

#[derive(Clone, Debug, PartialEq, Eq)]
struct PathMeta {
    paths: BTreeSet<String>,
    read_only: bool,
}

impl InterceptMeta for PathMeta {
    fn merge(existing: &Self, new: &Self) -> Self {
        let mut paths = existing.paths.clone();
        paths.extend(new.paths.iter().cloned());
        PathMeta {
            paths,
            read_only: new.read_only,
        }
    }
    fn clone_box(&self) -> Box<dyn InterceptMeta> {
        Box::new(self.clone())
    }
}

fn path_meta(paths: &[&str], read_only: bool) -> PathMeta {
    PathMeta {
        paths: paths.iter().map(|s| s.to_string()).collect(),
        read_only,
    }
}

// ── 键 ─────────────────────────────────────────────────────────────────

struct FsKey;
impl Key for FsKey {
    type Value = String;
    const SYMBOL: &'static str = "fs";
}

fn fs() -> Symbol {
    Symbol::intern("fs")
}

// ── 测试组件：声明 fs 的元数据（Def 30 的 𝔇inter）──────────────────────

struct MetaDeclaring {
    declared: Option<PathMeta>,
}

impl Component for MetaDeclaring {
    fn inject(&self) -> KeySet {
        [fs()].into_iter().collect()
    }
    fn provide(&self) -> KeySet {
        KeySet::new()
    }
    fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(once_finished())
    }
    fn declared_metadata(&self, key: Symbol) -> Option<Box<dyn InterceptMeta>> {
        if key == fs() {
            self.declared
                .clone()
                .map(|m| Box::new(m) as Box<dyn InterceptMeta>)
        } else {
            None
        }
    }
}

/// fs 提供者（供 MetaDeclaring 激活）。
struct FsProvider;

impl Component for FsProvider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        [fs()].into_iter().collect()
    }
    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(once_finished_binding(move || {
            ctx.set::<FsKey>("fs-value".into()).expect("绑定 fs")
        }))
    }
}

fn once_finished_binding(bind: impl FnOnce() -> Disposer + 'static) -> impl EffectIter {
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

fn once_finished() -> impl EffectIter {
    struct Done(bool);
    impl EffectIter for Done {
        fn next(&mut self) -> Step {
            if self.0 {
                self.0 = false;
                Step::Finished(Box::new(|| {}) as Disposer)
            } else {
                unreachable!("迭代器已完成")
            }
        }
    }
    Done(true)
}

fn use_at(
    root: &Rc<Context>,
    component: impl Component + 'static,
) -> Result<Rc<cordis_core::Fiber>, cordis_core::RegistryError> {
    root.use_component(Rc::new(component), Rc::new(()))
}

/// 读路径求值：`get(k, μ) = σ(k)(μ ⊕ ι(k))`——组件声明与上下文携带
/// 右偏合并（`ι` 优先：read_only 取 ι 的值、paths 取并）。
#[test]
fn get_meta_merges_declared_with_carried_right_biased() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let _p = use_at(&root, FsProvider).expect("provider 实例化");
    let fiber = use_at(
        &root,
        MetaDeclaring {
            declared: Some(path_meta(&["/a"], false)),
        },
    )
    .expect("实例化");

    // 无上下文携带：get_meta = 组件声明。
    assert_eq!(
        fiber.ctx().get_meta::<PathMeta>(fs()),
        Some(path_meta(&["/a"], false)),
        "无 ι：返回组件声明 μ"
    );

    // 上下文携带 ι（read_only=true、/b）→ 右偏合并：read_only 取 ι、paths 取并。
    fiber
        .ctx()
        .intercept_in_place(fs(), path_meta(&["/b"], true));
    assert_eq!(
        fiber.ctx().get_meta::<PathMeta>(fs()),
        Some(path_meta(&["/a", "/b"], true)),
        "μ ⊕ ι（ι 优先）"
    );
}

/// 无声明无携带 → None；有携带无声明 → 返回携带。
#[test]
fn get_meta_falls_back_to_carried_or_none() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let _p = use_at(&root, FsProvider).expect("provider 实例化");
    let fiber = use_at(&root, MetaDeclaring { declared: None }).expect("实例化");
    assert_eq!(
        fiber.ctx().get_meta::<PathMeta>(fs()),
        None,
        "无 μ 无 ι → None"
    );

    fiber
        .ctx()
        .intercept_in_place(fs(), path_meta(&["/c"], false));
    assert_eq!(
        fiber.ctx().get_meta::<PathMeta>(fs()),
        Some(path_meta(&["/c"], false)),
        "无 μ 有 ι → 返回 ι"
    );
}

/// 就地拦截（§5.2.1 intercept 字段分派）：不触发 reload——fiber 保持
/// Active、绑定不受扰动；读路径立即反映新元数据。
#[test]
fn intercept_in_place_updates_without_reload() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let _p = use_at(&root, FsProvider).expect("provider 实例化");
    let fiber = use_at(&root, MetaDeclaring { declared: None }).expect("实例化");
    assert!(matches!(&*fiber.state(), FiberState::Active { .. }), "激活");

    fiber
        .ctx()
        .intercept_in_place(fs(), path_meta(&["/d"], true));
    assert!(
        matches!(&*fiber.state(), FiberState::Active { .. }),
        "就地拦截不触发 reload（fiber 保持 Active）"
    );
    assert_eq!(
        fiber.ctx().get_meta::<PathMeta>(fs()),
        Some(path_meta(&["/d"], true)),
        "读路径立即反映"
    );
    assert!(runtime.is_quiet(), "无转换在途");
}

/// 派生 intercept（Def 31 精确形态）不触原上下文；就地拦截触原上下文。
#[test]
fn derived_intercept_is_isolated_in_place_is_shared() {
    let ctx = Context::new();
    let key = fs();

    // 派生：原上下文无元数据。
    let child = ctx.intercept(key, path_meta(&["/x"], false));
    assert_eq!(ctx.intercept_of::<PathMeta>(key), None, "派生不触原上下文");
    assert_eq!(
        child.intercept_of::<PathMeta>(key),
        Some(path_meta(&["/x"], false))
    );

    // 就地：原上下文直接更新。
    ctx.intercept_in_place(key, path_meta(&["/y"], true));
    assert_eq!(
        ctx.intercept_of::<PathMeta>(key),
        Some(path_meta(&["/y"], true)),
        "就地拦截更新原上下文"
    );
}

/// 类型纪律：组件声明与读取类型不一致 → panic。
#[test]
#[should_panic(expected = "拦截元数据类型冲突")]
fn declared_metadata_type_conflict_panics() {
    #[derive(Clone)]
    struct OtherMeta(String);
    impl InterceptMeta for OtherMeta {
        fn merge(_existing: &Self, new: &Self) -> Self {
            OtherMeta(new.0.clone())
        }
        fn clone_box(&self) -> Box<dyn InterceptMeta> {
            Box::new(self.clone())
        }
    }

    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let _p = use_at(&root, FsProvider).expect("provider 实例化");
    let fiber = use_at(
        &root,
        MetaDeclaring {
            declared: Some(path_meta(&["/a"], false)),
        },
    )
    .expect("实例化");
    let _ = fiber.ctx().get_meta::<OtherMeta>(fs());
}
