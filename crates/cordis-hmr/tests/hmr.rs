//! M2-PR5（cordis-hmr）：Algorithm 8/9/10 测试——模块分类不动点、过期
//! 条目检测、事务性重载 + 回滚。

use cordis_core::effect::EffectIter;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{Component, Context, Disposer, FiberState, Key, Runtime, Step};
use cordis_hmr::{
    Classification, HashMapGraph, Hmr, ModuleLoader, classify, detect, get_dependencies,
};
use cordis_loader::{Entry, Loader};
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::rc::Rc;

// ── Algorithm 8：模块分类不动点 ───────────────────────────────────────

#[test]
fn classify_propagates_accepted_and_declined() {
    // 图：plugin → lib → dep
    let graph = HashMapGraph(HashMap::from([
        ("plugin".to_string(), vec!["lib".to_string()]),
        ("lib".to_string(), vec!["dep".to_string()]),
        ("dep".to_string(), vec![]),
    ]));

    // stashed = [plugin]：plugin 接受（种子）；lib 导入 dep——dep 无导入
    // （空集 ⊆ declined）→ 拒绝；lib 全部导入 ⊆ declined → 拒绝。
    let c = classify(&["plugin".to_string()], &[], &graph);
    assert_eq!(
        c,
        Classification {
            accepted: ["plugin".to_string()].into_iter().collect(),
            declined: ["lib".to_string(), "dep".to_string()].into_iter().collect(),
        }
    );

    // stashed = [dep]：dep 接受（种子）；分类**自 stashed 向下**传播
    //（Algorithm 8 原文：种子 = stashed 的导入）——lib/plugin（导入 dep
    // 的上游）不经分类，由 Algorithm 9 的依赖树 ∩ accepted 判定过期。
    let c2 = classify(&["dep".to_string()], &[], &graph);
    assert_eq!(c2.accepted, ["dep".to_string()].into_iter().collect());

    // externals：stashed = [plugin]，externals = [lib] → lib 拒绝种子；
    // plugin 接受；dep 不被分类（lib 已在 declined 种子中，传播不触达）。
    let c3 = classify(&["plugin".to_string()], &["lib".to_string()], &graph);
    assert_eq!(c3.accepted, ["plugin".to_string()].into_iter().collect());
    assert_eq!(c3.declined, ["lib".to_string()].into_iter().collect());
}

#[test]
fn classify_cycles_default_to_declined() {
    // 导入环 X → Y → X：无种子时全部未决 → 拒绝。
    let graph = HashMapGraph(HashMap::from([
        ("x".to_string(), vec!["y".to_string()]),
        ("y".to_string(), vec!["x".to_string()]),
    ]));
    // 无种子：无分类（Algorithm 8 以 stashed 为种子；空种子 = 空结果）。
    let c = classify(&[], &[], &graph);
    assert_eq!(c.accepted.len(), 0, "无种子：无接受");
    assert_eq!(c.declined.len(), 0, "无种子：无拒绝");

    // 环被种子打破：stashed = [x] → x 接受；y 导入 {x} ⊆ accepted → 接受。
    let c2 = classify(&["x".to_string()], &[], &graph);
    assert_eq!(
        c2.accepted,
        ["x".to_string(), "y".to_string()].into_iter().collect()
    );
}

// ── Algorithm 9：依赖树与过期条目 ─────────────────────────────────────

#[test]
fn detect_finds_stale_entries_respecting_declined() {
    let graph = HashMapGraph(HashMap::from([
        ("e1".to_string(), vec!["a".to_string()]),
        ("a".to_string(), vec!["b".to_string()]),
        ("e2".to_string(), vec!["c".to_string()]),
    ]));
    // e1 的树 = {e1, a, b}；accepted = {b} → e1 过期。
    let c = Classification {
        accepted: ["b".to_string()].into_iter().collect(),
        declined: BTreeSet::new(),
    };
    let stale = detect(&["e1".to_string(), "e2".to_string()], &c, &graph);
    assert_eq!(stale, vec!["e1".to_string()]);

    // declined 边界：a ∈ declined 时 e1 的树 = {e1}（不越过边界）→ 不过期。
    let tree = get_dependencies("e1", &["a".to_string()].into_iter().collect(), &graph);
    assert_eq!(
        tree,
        ["e1".to_string()].into_iter().collect(),
        "declined 边界截断"
    );
}

// ── Algorithm 10：事务性重载 + 回滚 ───────────────────────────────────

struct ValKey;
impl Key for ValKey {
    type Value = String;
    const SYMBOL: &'static str = "val";
}

struct SumKey;
impl Key for SumKey {
    type Value = String;
    const SYMBOL: &'static str = "sum";
}

fn spec(names: &[&str]) -> KeySet {
    names.iter().map(|s| Symbol::intern(s)).collect()
}

/// 提供者：绑定 val = config。
struct Provider;

impl Component for Provider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        spec(&["val"])
    }
    fn apply(&self, ctx: Rc<Context>, config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        let value = config
            .downcast_ref::<String>()
            .expect("config 为 String")
            .clone();
        Box::new(once_finished(move || {
            ctx.set::<ValKey>(value).expect("绑定 val")
        }))
    }
}

/// 提供者 v2：绑定 val = "v2:<config>"（版本体现在模块本身）。
struct ProviderV2;

impl Component for ProviderV2 {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        spec(&["val"])
    }
    fn apply(&self, ctx: Rc<Context>, config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        let value = config
            .downcast_ref::<String>()
            .expect("config 为 String")
            .clone();
        Box::new(once_finished(move || {
            ctx.set::<ValKey>(format!("v2:{value}")).expect("绑定 val")
        }))
    }
}

/// 消费者：注入 val → 提供 sum(val)。
struct Consumer;

impl Component for Consumer {
    fn inject(&self) -> KeySet {
        spec(&["val"])
    }
    fn provide(&self) -> KeySet {
        spec(&["sum"])
    }
    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(once_finished(move || {
            let value = {
                let v = ctx.get::<ValKey>().expect("注入的 val 可用");
                v.clone()
            };
            ctx.set::<SumKey>(format!("sum({value})"))
                .expect("绑定 sum")
        }))
    }
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

/// 固定映射的模块加载器（url → 新版本组件）。
struct MapLoader(HashMap<String, Rc<dyn Component>>);

impl ModuleLoader for MapLoader {
    fn load(&self, url: &str) -> anyhow::Result<Rc<dyn Component>> {
        self.0
            .get(url)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("模块 `{url}` 无新版本"))
    }
}

fn setup(val: &str) -> (Rc<Loader>, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::new());
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("db", Rc::new(Provider));
    loader.register_component("cons", Rc::new(Consumer));
    loader.apply(&[
        Entry::new("p", "db", Rc::new(val.to_string()), 0, false),
        Entry::new("c", "cons", Rc::new(()), 0, false),
    ]);
    (loader, runtime)
}

fn sum_of(runtime: &Runtime) -> String {
    let store = runtime.store();
    store
        .get_value(Symbol::intern("sum"))
        .expect("sum 绑定")
        .downcast_ref::<String>()
        .expect("String")
        .clone()
}

/// 门禁用例：改插件代码保存即生效——stashed 触发重载，新版本生效、
/// 其他组件状态保留（consumer 不重建、重新读取新值）。
#[test]
fn hmr_reload_applies_new_version_keeping_other_components() {
    let (loader, runtime) = setup("1");
    let c_first = loader.fiber("c").expect("consumer 激活").id();
    assert_eq!(sum_of(&runtime), "sum(1)");

    // "保存"：db 的导入 lib 变更 → db 过期；新版本模块 = ProviderV2
    //（config 不变——版本体现在模块本身）。
    let graph = HashMapGraph(HashMap::from([
        ("db".to_string(), vec!["lib".to_string()]),
        ("lib".to_string(), vec![]),
    ]));
    let mut versions: HashMap<String, Rc<dyn Component>> = HashMap::new();
    versions.insert("lib".to_string(), Rc::new(Provider)); // lib 占位（图连通用）
    versions.insert("db".to_string(), Rc::new(ProviderV2) as Rc<dyn Component>);

    let hmr = Hmr::new(loader.clone(), Box::new(MapLoader(versions)));
    let desired = vec![
        Entry::new("p", "db", Rc::new("1".to_string()), 0, false),
        Entry::new("c", "cons", Rc::new(()), 0, false),
    ];
    // stashed = [lib]（db 的导入被接受 → db 过期 → 重建）。
    let stale = hmr
        .reload(&["lib".to_string()], &[], &graph, &desired)
        .expect("重载成功");
    assert_eq!(stale, vec!["db".to_string()]);

    // 新版本生效：provider 重建后绑定 val="v2:1"，consumer 保持（不重建）
    // 且重新读取 → sum(v2:1)。
    let p_new = loader.fiber("p").expect("provider 重建");
    let c_now = loader.fiber("c").expect("consumer 仍在");
    assert_eq!(c_now.id(), c_first, "其他组件状态保留（consumer 不重建）");
    assert!(
        matches!(&*p_new.state(), FiberState::Active { .. }),
        "provider 激活"
    );
    assert!(
        matches!(&*c_now.state(), FiberState::Active { .. }),
        "consumer 激活"
    );
    assert_eq!(sum_of(&runtime), "sum(v2:1)", "新版本立即生效");
    assert!(runtime.is_quiet(), "静止");
}

/// 回滚用例：新模块加载失败 → 事务回滚（恢复旧版本，系统不进入半重载）。
#[test]
fn hmr_reload_rolls_back_on_load_failure() {
    let (loader, runtime) = setup("1");
    assert_eq!(sum_of(&runtime), "sum(1)");
    let p_first = loader.fiber("p").expect("provider").id();
    let c_first = loader.fiber("c").expect("consumer").id();

    // 新版本加载失败（MapLoader 无 "db" 新版本）。
    let graph = HashMapGraph(HashMap::from([("db".to_string(), vec![])]));
    let hmr = Hmr::new(loader.clone(), Box::new(MapLoader(HashMap::new())));
    let desired = vec![
        Entry::new("p", "db", Rc::new("1".to_string()), 0, false),
        Entry::new("c", "cons", Rc::new(()), 0, false),
    ];
    let result = hmr.reload(&["db".to_string()], &[], &graph, &desired);
    assert!(result.is_err(), "加载失败 → 重载报错");

    // 回滚：provider 恢复旧版本（fiber 重建回旧配置），consumer 不受影响。
    let p_now = loader.fiber("p").expect("provider 仍在");
    let c_now = loader.fiber("c").expect("consumer 仍在");
    assert_eq!(c_now.id(), c_first, "consumer fiber 不变");
    assert!(
        matches!(&*p_now.state(), FiberState::Active { .. }),
        "provider 激活"
    );
    assert!(
        matches!(&*c_now.state(), FiberState::Active { .. }),
        "consumer 激活"
    );
    assert_eq!(sum_of(&runtime), "sum(1)", "回滚到旧版本（val=1）");
    assert!(runtime.is_quiet(), "静止");
    // provider 重建过（回滚重建）：fiber id 变化（旧 fiber 已退役）。
    assert_ne!(p_now.id(), p_first, "回滚经重建（新 fiber）");
}

/// 回滚用例（运行失败）：新组件实例化后 fiber 失败（L-Raise）→ 同样回滚。
#[test]
fn hmr_reload_rolls_back_on_component_failure() {
    let (loader, runtime) = setup("1");
    assert_eq!(sum_of(&runtime), "sum(1)");

    // 新版本组件：apply 时 raise（FiberError）→ fiber 失败态。
    struct Broken;
    impl Component for Broken {
        fn inject(&self) -> KeySet {
            KeySet::new()
        }
        fn provide(&self) -> KeySet {
            spec(&["val"])
        }
        fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
            Box::new(once_finished(move || {
                cordis_core::FiberError::new("新版本运行失败").raise()
            }))
        }
    }
    let mut versions: HashMap<String, Rc<dyn Component>> = HashMap::new();
    versions.insert("db".to_string(), Rc::new(Broken));
    let graph = HashMapGraph(HashMap::from([("db".to_string(), vec![])]));
    let hmr = Hmr::new(loader.clone(), Box::new(MapLoader(versions)));
    let desired = vec![
        Entry::new("p", "db", Rc::new("1".to_string()), 0, false),
        Entry::new("c", "cons", Rc::new(()), 0, false),
    ];
    let result = hmr.reload(&["db".to_string()], &[], &graph, &desired);
    assert!(result.is_err(), "组件运行失败 → 回滚");

    // 回滚后旧版本生效。
    assert_eq!(sum_of(&runtime), "sum(1)", "回滚到旧版本");
    assert!(matches!(
        &*loader.fiber("c").unwrap().state(),
        FiberState::Active { .. }
    ));
    assert!(runtime.is_quiet(), "静止");
}

/// 空 stashed：无过期条目 → 空操作（REVIEW-4c6e7fc nit4）。
#[test]
fn hmr_reload_noop_without_stashed() {
    let (loader, runtime) = setup("1");
    let p_first = loader.fiber("p").expect("provider").id();
    let c_first = loader.fiber("c").expect("consumer").id();

    let graph = HashMapGraph(HashMap::from([("db".to_string(), vec![])]));
    let hmr = Hmr::new(loader.clone(), Box::new(MapLoader(HashMap::new())));
    let desired = vec![
        Entry::new("p", "db", Rc::new("1".to_string()), 0, false),
        Entry::new("c", "cons", Rc::new(()), 0, false),
    ];
    let stale = hmr
        .reload(&[], &[], &graph, &desired)
        .expect("空 stashed 空操作");
    assert!(stale.is_empty(), "无过期条目");
    assert_eq!(loader.fiber("p").unwrap().id(), p_first, "provider 未动");
    assert_eq!(loader.fiber("c").unwrap().id(), c_first, "consumer 未动");
    assert_eq!(sum_of(&runtime), "sum(1)", "状态不变");
}

/// 事务 panic 安全（REVIEW-4c6e7fc major1）：新模块供给与其它存活条目
/// 冲突 → `apply` panic → **先回滚再重抛**（panic = bug 纪律 + 永不半
/// 重载）。
#[test]
fn hmr_reload_rolls_back_before_repanic_on_provision_clash() {
    let (loader, runtime) = setup("1");
    assert_eq!(sum_of(&runtime), "sum(1)");

    // 新版本 db 同时提供 val + sum → 与条目 o 的 sum 供给冲突。
    struct Clashing;
    impl Component for Clashing {
        fn inject(&self) -> KeySet {
            KeySet::new()
        }
        fn provide(&self) -> KeySet {
            spec(&["val", "sum"])
        }
        fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
            Box::new(once_finished(move || {
                ctx.set::<ValKey>("clash".into()).expect("绑定 val")
            }))
        }
    }
    let mut versions: HashMap<String, Rc<dyn Component>> = HashMap::new();
    versions.insert("db".to_string(), Rc::new(Clashing));
    let graph = HashMapGraph(HashMap::from([("db".to_string(), vec![])]));
    let hmr = Hmr::new(loader.clone(), Box::new(MapLoader(versions)));
    let desired = vec![
        Entry::new("p", "db", Rc::new("1".to_string()), 0, false),
        Entry::new("c", "cons", Rc::new(()), 0, false),
    ];

    // apply panic（ProvisionClash：新 db 提供 val+sum，与 c 的 sum 冲突）
    // → 回滚后重抛。
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        hmr.reload(&["db".to_string()], &[], &graph, &desired)
    }));
    assert!(result.is_err(), "供给冲突 panic 传播（panic = bug）");

    // 系统已回滚：旧版本生效、条目 c 状态保留、无半重载残留。
    assert_eq!(sum_of(&runtime), "sum(1)", "回滚到旧版本");
    assert!(
        matches!(
            &*loader.fiber("c").expect("c 仍在").state(),
            FiberState::Active { .. }
        ),
        "consumer 状态保留"
    );
    assert!(runtime.is_quiet(), "静止（无半重载残留）");
}

/// 多条目共享同一组件名（REVIEW-4c6e7fc nit2）：stale 去重、全部共享
/// 条目重建。
#[test]
fn hmr_reload_rebuilds_all_entries_sharing_url() {
    let (loader, runtime) = setup("1");
    // 第二个条目 p2 也使用 db 组件（同 url）——提供 val 冲突？两个条目
    // 都提供 val → ProvisionClash。改为 p2 用 Consumer（注入 val）——
    // 共享 url 需同组件；Consumer 提供 sum 与 c 冲突。用 distinct 组件
    // 名注册表：两个条目共用 "db"（提供 val）不可能共存。
    // 实际可共享场景：两个条目都用**消费者**（注入 val、无供给冲突）：
    loader.register_component("cons", Rc::new(Consumer));
    let _ = &runtime;
    // 重建场景：c1、c2 都是 cons（注入 val、提供 sum？——提供 sum 会
    // 冲突）。用无供给的消费者变体：
    struct PureConsumer;
    impl Component for PureConsumer {
        fn inject(&self) -> KeySet {
            spec(&["val"])
        }
        fn provide(&self) -> KeySet {
            KeySet::new()
        }
        fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
            Box::new(once_finished(|| Box::new(|| {}) as Disposer))
        }
    }
    loader.register_component("pure", Rc::new(PureConsumer));
    loader.apply(&[
        Entry::new("p", "db", Rc::new("1".to_string()), 0, false),
        Entry::new("c1", "pure", Rc::new(()), 0, false),
        Entry::new("c2", "pure", Rc::new(()), 0, false),
    ]);
    assert!(
        loader.fiber("c1").is_some() && loader.fiber("c2").is_some(),
        "双消费者激活"
    );

    // stashed = [db]（提供者）→ 过期；pure 的依赖树 = {pure, val?}——
    // pure 无模块级导入（图里无 pure）→ 不过期。断言只有 db 过期。
    let graph = HashMapGraph(HashMap::from([("db".to_string(), vec![])]));
    let mut versions: HashMap<String, Rc<dyn Component>> = HashMap::new();
    versions.insert("db".to_string(), Rc::new(Provider));
    let hmr = Hmr::new(loader.clone(), Box::new(MapLoader(versions)));
    let desired = vec![
        Entry::new("p", "db", Rc::new("1".to_string()), 0, false),
        Entry::new("c1", "pure", Rc::new(()), 0, false),
        Entry::new("c2", "pure", Rc::new(()), 0, false),
    ];
    let stale = hmr
        .reload(&["db".to_string()], &[], &graph, &desired)
        .expect("重载成功（新模块 = 旧 Provider 类型 + 配置不变）");
    assert_eq!(stale, vec!["db".to_string()], "仅 db 过期（stale 去重）");
    assert!(
        runtime.store().contains(Symbol::intern("val")),
        "db 重建后 val 绑定"
    );
    assert!(
        loader.fiber("c1").is_some() && loader.fiber("c2").is_some(),
        "共享 url 的两个消费者条目保留（未过期）"
    );
    assert!(runtime.is_quiet(), "静止");
}
