//! Cordis 声明式组件加载器（论文 §5.2.1，PR #8 最小版）。
//!
//! 编排器把期望的系统组成描述为持久化配置（条目列表），加载器把配置的
//! 变化翻译为对应的 fiber 操作（§5.2.1："the loader translates changes to
//! this specification into the corresponding imperative fiber operations"）。
//!
//! **最小范围（PR #8）**：覆盖 Def 74 条目的 `id` / `config`（经 `revision`
//! 检测变更）/ `disabled` 三字段，以及组件名（`url` 的原生版，变更即重建）。
//! 协调是**增量的**（§5.2.1 reconciliation）：新增条目实例化、消失条目卸载、
//! `disabled` 切换卸载/重载、`config`/组件变更重建——已加载且未变化的
//! 条目不做任何操作（幂等）。
//!
//! **未覆盖（M2）**：`isolate` / `intercept` 注解、嵌套 group/include、
//! 托管 realm（Algorithm 7）、组件级 config diff（论文交由组件自决）。
//! 条目当前全部实例化在 root 上下文（根级，无子代——`remove_fiber` 的
//! `HasChildren` 前提不受影响）。

use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use cordis_core::{Component, Context, Fiber, Runtime};

/// 配置条目（Def 74 的 PR #8 最小版）：声明一个 fiber。
#[derive(Debug, Clone)]
pub struct Entry {
    /// 稳定标识——协调键（组内子列表变化时据此 diff）。
    pub id: String,
    /// 组件名（`url` 的原生版）：经 [`Loader::register_component`] 注册；
    /// 变更即重建条目（论文的 `id, url → rebuild`）。
    pub component: String,
    /// 配置（绑定进 `apply` 形成效应函数，Algorithm 4 第 9 行）。
    /// 以 [`Rc`] 持有以便重建时复用；**变更检测依赖 [`Entry::revision`]**
    /// （配置值本身不可比较）。
    pub config: Rc<dyn Any>,
    /// 配置修订号：调用方在 `config` 变更时递增（论文的组件级 diff 由
    /// 协调键承担，M2 再细化）。
    pub revision: u64,
    /// 是否被管理性关闭（`disabled`）。
    pub disabled: bool,
}

impl Entry {
    /// 构造条目。
    pub fn new(
        id: impl Into<String>,
        component: impl Into<String>,
        config: Rc<dyn Any>,
        revision: u64,
        disabled: bool,
    ) -> Self {
        Self {
            id: id.into(),
            component: component.into(),
            config,
            revision,
            disabled,
        }
    }
}

/// 已加载条目的运行时状态。
#[derive(Clone)]
struct LoadedEntry {
    component: String,
    config: Rc<dyn Any>,
    revision: u64,
    disabled: bool,
    /// 实例化的 fiber（`disabled` 或尚未满足依赖时为 `None`）。
    fiber: Option<Rc<Fiber>>,
}

/// 声明式加载器：维护 `id → fiber` 的映射，把配置变化增量协调到运行时。
pub struct Loader {
    runtime: Rc<Runtime>,
    root: Rc<Context>,
    components: RefCell<HashMap<String, Rc<dyn Component>>>,
    entries: RefCell<HashMap<String, LoadedEntry>>,
}

impl Loader {
    /// 在共享运行时上创建加载器（条目实例化于 root 上下文）。
    pub fn new(runtime: Rc<Runtime>) -> Self {
        let root = runtime.context();
        Self {
            runtime,
            root,
            components: RefCell::new(HashMap::new()),
            entries: RefCell::new(HashMap::new()),
        }
    }

    /// 注册组件（`Entry::component` 名的解析表，`url` 的原生版）。
    pub fn register_component(&self, name: impl Into<String>, component: Rc<dyn Component>) {
        self.components.borrow_mut().insert(name.into(), component);
    }

    /// 协调：以 `desired` 为权威记录，增量对齐（幂等）。
    ///
    /// - 新增条目 → 实例化（除非 `disabled`）；
    /// - 消失条目 → 退役、卸载并移除 fiber（释放供给名）；
    /// - `disabled` 切换 → 卸载 / 重载；
    /// - `component` 或 `revision` 变更 → 重建（退役旧 fiber、以新配置
    ///   重新实例化）。
    ///
    /// 组件名未注册或供给冲突（两个条目提供同一键）→ panic（配置错误，
    /// panic = bug）。
    pub fn apply(&self, desired: &[Entry]) {
        let desired_ids: BTreeSet<&str> = desired.iter().map(|e| e.id.as_str()).collect();
        for entry in desired {
            let loaded = self.entries.borrow().get(&entry.id).cloned();
            match loaded {
                None => self.load(entry),
                Some(loaded) => self.reconcile(entry, &loaded),
            }
        }
        let current: Vec<String> = self.entries.borrow().keys().cloned().collect();
        for id in current {
            if !desired_ids.contains(id.as_str()) {
                self.remove(&id);
            }
        }
    }

    /// 条目当前 fiber（未加载 / 已卸载 / 未满足依赖时为 `None`）。
    pub fn fiber(&self, id: &str) -> Option<Rc<Fiber>> {
        self.entries.borrow().get(id).and_then(|l| l.fiber.clone())
    }

    /// 已加载条目数。
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    // ── 内部：协调原语 ────────────────────────────────────────────────

    fn load(&self, entry: &Entry) {
        let fiber = if entry.disabled {
            None
        } else {
            Some(self.instantiate(entry))
        };
        self.entries.borrow_mut().insert(
            entry.id.clone(),
            LoadedEntry {
                component: entry.component.clone(),
                config: Rc::clone(&entry.config),
                revision: entry.revision,
                disabled: entry.disabled,
                fiber,
            },
        );
    }

    fn reconcile(&self, entry: &Entry, loaded: &LoadedEntry) {
        // 逐字段最小扰动（§5.2.1 的 per-field dispatch）。
        if loaded.disabled != entry.disabled {
            if entry.disabled {
                // disabled 置位：退役并移除 fiber（绑定随之恢复）。
                self.unload_fiber(&entry.id);
                self.update(&entry.id, entry, None);
            } else {
                // disabled 清除：以保留的配置重新实例化。
                let fiber = self.instantiate(entry);
                self.update(&entry.id, entry, Some(fiber));
            }
            return;
        }
        if entry.disabled {
            return; // 禁用且状态未变
        }
        if loaded.component != entry.component || loaded.revision != entry.revision {
            // 组件（url 原生版）/ 配置变更：重建条目。
            self.unload_fiber(&entry.id);
            let fiber = self.instantiate(entry);
            self.update(&entry.id, entry, Some(fiber));
        }
    }

    fn update(&self, id: &str, entry: &Entry, fiber: Option<Rc<Fiber>>) {
        if let Some(loaded) = self.entries.borrow_mut().get_mut(id) {
            loaded.component = entry.component.clone();
            loaded.config = Rc::clone(&entry.config);
            loaded.revision = entry.revision;
            loaded.disabled = entry.disabled;
            loaded.fiber = fiber;
        }
    }

    fn instantiate(&self, entry: &Entry) -> Rc<Fiber> {
        let component = self
            .components
            .borrow()
            .get(&entry.component)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "未注册的组件 `{}`（先 register_component）",
                    entry.component
                )
            });
        self.root
            .use_component(component, Rc::clone(&entry.config))
            .unwrap_or_else(|err| panic!("条目 `{}` 实例化失败：{err:?}（配置错误）", entry.id))
    }

    fn unload_fiber(&self, id: &str) {
        let fiber = self
            .entries
            .borrow_mut()
            .get_mut(id)
            .and_then(|l| l.fiber.take());
        if let Some(fiber) = fiber {
            fiber.retire(); // 同步卸载（级联依赖者）
            self.runtime
                .remove_fiber(fiber.id())
                .unwrap_or_else(|err| panic!("条目 `{id}` 移除失败：{err:?}（根级条目应无子代）"));
        }
    }

    fn remove(&self, id: &str) {
        self.unload_fiber(id);
        self.entries.borrow_mut().remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cordis_core::{EffectIter, FiberState, Key, KeySet, Symbol, once};

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

    /// 测试组件：提供 `val`，值取自配置（`config.downcast_ref::<String>()`）。
    fn val_provider() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["val"]),
            effects: Box::new(|ctx, config| {
                let value = config
                    .downcast_ref::<String>()
                    .expect("val_provider 的 config 为 String")
                    .clone();
                Box::new(once(Box::new(move || {
                    ctx.set::<ValKey>(value).expect("绑定 val")
                })))
            }),
        })
    }

    /// 测试组件：注入 `val`，读取后绑定 `sum`。
    fn sum_consumer() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&["val"]),
            provide: spec(&["sum"]),
            effects: Box::new(|ctx, _config| {
                Box::new(once(Box::new(move || {
                    let value = {
                        let v = ctx.get::<ValKey>().expect("注入的 val 可用");
                        format!("sum({v})")
                    };
                    ctx.set::<SumKey>(value).expect("绑定 sum")
                })))
            }),
        })
    }

    /// 测试组件效应程序（ctx + config → 迭代器）。
    type Effects = Box<dyn Fn(Rc<Context>, &dyn Any) -> Box<dyn EffectIter>>;

    struct TestComponent {
        inject: KeySet,
        provide: KeySet,
        effects: Effects,
    }

    impl Component for TestComponent {
        fn inject(&self) -> KeySet {
            self.inject.clone()
        }
        fn provide(&self) -> KeySet {
            self.provide.clone()
        }
        fn apply(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
            (self.effects)(ctx, config)
        }
    }

    fn entry(id: &str, component: &str, config: &str, revision: u64, disabled: bool) -> Entry {
        Entry::new(
            id,
            component,
            Rc::new(config.to_string()),
            revision,
            disabled,
        )
    }

    fn loader() -> (Rc<Loader>, Rc<Runtime>) {
        let runtime = Rc::new(Runtime::new());
        let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
        loader.register_component("provider", val_provider());
        loader.register_component("consumer", sum_consumer());
        (loader, runtime)
    }

    #[test]
    fn loads_entries_and_activates_in_dependency_order() {
        let (loader, _runtime) = loader();
        loader.apply(&[
            entry("consumer", "consumer", "ignored", 1, false),
            entry("provider", "provider", "pg", 1, false),
        ]);
        // 消费者先声明：依赖未满足保持 Inactive；provider 出现后激活。
        let provider = loader.fiber("provider").expect("provider 已加载");
        let consumer = loader.fiber("consumer").expect("consumer 已加载");
        assert!(matches!(&*provider.state(), FiberState::Active { .. }));
        assert!(
            matches!(&*consumer.state(), FiberState::Active { .. }),
            "provider 激活后 consumer 自动激活"
        );
        assert_eq!(
            provider.ctx().get::<ValKey>().unwrap().as_str(),
            "pg",
            "provider 绑定配置值"
        );
        assert_eq!(
            consumer.ctx().get::<SumKey>().unwrap().as_str(),
            "sum(pg)",
            "consumer 读取到注入值"
        );
    }

    #[test]
    fn apply_is_idempotent() {
        let (loader, _runtime) = loader();
        let desired = [
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ];
        loader.apply(&desired);
        let id_before = loader.fiber("provider").unwrap().id();
        loader.apply(&desired);
        assert_eq!(
            loader.fiber("provider").unwrap().id(),
            id_before,
            "未变化条目不重建（协调幂等）"
        );
        assert_eq!(loader.len(), 2);
    }

    #[test]
    fn disabled_toggle_unloads_and_reloads() {
        let (loader, runtime) = loader();
        loader.apply(&[entry("provider", "provider", "pg", 1, false)]);
        let first_id = loader.fiber("provider").unwrap().id();

        // disabled 置位：退役 + 移除，绑定恢复。
        loader.apply(&[entry("provider", "provider", "pg", 1, true)]);
        assert!(loader.fiber("provider").is_none(), "禁用条目无 fiber");
        assert!(
            runtime.fiber(first_id).is_none(),
            "旧 fiber 已从 registry 移除"
        );
        assert!(runtime.store().symbols().next().is_none(), "绑定已恢复");

        // disabled 清除：以保留的配置重新实例化。
        loader.apply(&[entry("provider", "provider", "pg", 1, false)]);
        let second = loader.fiber("provider").expect("重载");
        assert!(matches!(&*second.state(), FiberState::Active { .. }));
        assert_ne!(second.id(), first_id, "重载产生新 fiber");
        assert_eq!(second.ctx().get::<ValKey>().unwrap().as_str(), "pg");
    }

    #[test]
    fn config_change_rebuilds_entry() {
        let (loader, _runtime) = loader();
        loader.apply(&[entry("provider", "provider", "pg", 1, false)]);
        let first_id = loader.fiber("provider").unwrap().id();
        assert_eq!(
            loader
                .fiber("provider")
                .unwrap()
                .ctx()
                .get::<ValKey>()
                .unwrap()
                .as_str(),
            "pg"
        );

        // revision 递增 + 新配置 → 重建（退役旧 fiber、新配置实例化）。
        loader.apply(&[entry("provider", "provider", "mysql", 2, false)]);
        let rebuilt = loader.fiber("provider").expect("重建");
        assert_ne!(rebuilt.id(), first_id, "config 变更重建条目");
        assert_eq!(rebuilt.ctx().get::<ValKey>().unwrap().as_str(), "mysql");

        // revision 不变、同配置 → 幂等。
        let id_before = rebuilt.id();
        loader.apply(&[entry("provider", "provider", "mysql", 2, false)]);
        assert_eq!(loader.fiber("provider").unwrap().id(), id_before);
    }

    #[test]
    fn removed_entry_unloads_and_cascades() {
        let (loader, runtime) = loader();
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        let provider_id = loader.fiber("provider").unwrap().id();
        let consumer = loader.fiber("consumer").unwrap();

        // 从配置中移除 provider → 退役卸载 + 移除；consumer 级联停用。
        loader.apply(&[entry("consumer", "consumer", "ignored", 1, false)]);
        assert!(loader.fiber("provider").is_none());
        assert!(runtime.fiber(provider_id).is_none(), "旧 fiber 已移除");
        assert!(
            matches!(&*consumer.state(), FiberState::Inactive(_)),
            "依赖消失 → consumer 级联停用"
        );
        assert!(runtime.store().symbols().next().is_none(), "绑定全部恢复");
    }

    #[test]
    #[should_panic(expected = "未注册的组件")]
    fn unknown_component_panics() {
        let (loader, _runtime) = loader();
        loader.apply(&[entry("x", "ghost", "cfg", 1, false)]);
    }
}
