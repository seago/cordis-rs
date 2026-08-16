//! 运行时：fiber registry `Fγ` + 生命周期状态机（§5.1.3，Algorithm 3/4/5）。
//!
//! - [`Runtime::register`]：Algorithm 4 的 `use`（组件实例化，Def 47）；
//! - [`Runtime::refresh`] / [`Runtime::reload`] / [`Runtime::unload`]：
//!   Algorithm 5 的惯性状态机（§4.3.3）；
//! - [`Runtime::notify_fibers`]：Algorithm 3 的通知传播（内置注册为
//!   运行时首个反应器）；
//! - [`Runtime::remove_fiber`]：O-Remove。
//!
//! **同步核心**：转换（reload/unload）在调用栈上同步跑完；嵌套操作（效应
//! 步骤中的 set → notify → 依赖者转换）形成同步级联。惯性（inertia）由
//! `FiberState` 的 `Reloading`/`Unloading` 标记实现——转换在途时 refresh
//! 推迟，转换完成时按目标自链（§4.3.3）。异步化（PR #6/async 阶段）时
//! 转换移入任务、`i` 移入状态（THEORY-MAP 已知偏差）。
//!
//! **级联深度边界（审查 m3）**：依赖链深度 N 的激活/停用级联产生 N 层
//! 嵌套调用栈（每层含 reload/unload 全流程）——超深链（数百层）有栈溢出
//! 风险，属同步核心的已知边界；async 化（转换入任务）后自然缓解。

use std::any::Any;
use std::cell::{Cell, Ref, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use crate::component::Component;
use crate::context::Context;
use crate::effect::{Disposer, EffectIter, execute, once};
use crate::fiber::{Fiber, FiberId, FiberState, View};
use crate::keyset::KeySet;
use crate::store::Store;
use crate::symbol::Symbol;

/// 通知反应器（Algorithm 3 的筛选/refresh 由反应器承担）。
///
/// 载荷语义（审查 M1/m1 统一）：`keys` 为**已解析的 realm 符号**（经通知方
/// 上下文的 `ρ` 解析，Def 28）——`set` 的绑定/撤销与 fiber 激活/停用
/// 均按 realm 广播；反应器须以 realm 语义筛选（fiber 反应器按
/// `f.ctx.resolve_realm(inject_key) == payload_realm` 匹配）。
pub type Reactor = Rc<dyn Fn(&Context, &[Symbol])>;

/// 注册表操作错误（对应 O-Insert / O-Remove 前提）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryError {
    /// O-Insert：`∃m. p ∩ p_m ≠ ∅`（单一来源纪律）。
    ProvisionClash,
    /// O-Remove：`n ∉ dom(Fγ)`。
    UnknownFiber,
    /// O-Remove：`τ_n ≠ ⊤`。
    NotRetired,
    /// O-Remove：`θ_n ≠ Inactive`。
    StillActive,
    /// O-Remove：`∃m. π_m = n`（须先移除子代）。
    HasChildren,
}

/// 运行时：共享共效应表 `σ`、fiber registry `Fγ` 与通知反应器。
pub struct Runtime {
    /// `σ`：按 realm 键控的依赖表（Def 28）。
    pub(crate) store: RefCell<Store>,
    /// 通知反应器（Algorithm 3 的 fiber 遍历已内置注册为首个反应器）。
    reactors: RefCell<Vec<Reactor>>,
    /// `Fγ`：fiber registry（Def 45）。
    pub(crate) fibers: RefCell<HashMap<FiberId, Rc<Fiber>>>,
    /// 名字计数器（Def 45：名称原子、绝不复用）。
    next: Cell<u64>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// 空运行时（内置注册 Algorithm 3 的 fiber 反应器）。
    pub fn new() -> Self {
        let runtime = Self {
            store: RefCell::new(Store::new()),
            reactors: RefCell::new(Vec::new()),
            fibers: RefCell::new(HashMap::new()),
            next: Cell::new(0),
        };
        runtime
            .reactors
            .borrow_mut()
            .push(Rc::new(|ctx: &Context, keys: &[Symbol]| {
                ctx.runtime().notify_fibers(keys)
            }));
        runtime
    }

    /// 注册用户通知反应器（在 fiber 反应器之后按注册顺序调用）。
    ///
    /// **重入纪律（审查 M1）**：通知以**快照迭代**调用反应器——反应器内
    /// 注册新反应器安全（本轮不触发）；但反应器内**同步触发新的共效应
    /// 变更**会递归广播 notify（fiber 反应器经 refresh 惯性切断同步递归；
    /// 用户反应器应避免同步 set，或以守卫显式切断）。
    pub fn on_notify(&self, reactor: impl Fn(&Context, &[Symbol]) + 'static) {
        self.reactors.borrow_mut().push(Rc::new(reactor));
    }

    /// 在共享运行时上创建根上下文。
    pub fn context(self: &Rc<Self>) -> Rc<Context> {
        Rc::new(Context::new_with(self))
    }

    /// 按名读取 fiber（registry 查询）。
    pub fn fiber(&self, id: FiberId) -> Option<Rc<Fiber>> {
        self.fibers.borrow().get(&id).cloned()
    }

    /// registry 中 fiber 数。
    pub fn len(&self) -> usize {
        self.fibers.borrow().len()
    }

    /// registry 是否为空。
    pub fn is_empty(&self) -> bool {
        self.fibers.borrow().is_empty()
    }

    /// 活跃 fiber 集（Def 46 静止判定的安装集；供监控与 oracle 对比测试）。
    pub fn active_fibers(&self) -> BTreeSet<FiberId> {
        self.fibers
            .borrow()
            .values()
            .filter(|f| matches!(&*f.state.borrow(), FiberState::Active { .. }))
            .map(|f| f.id)
            .collect()
    }

    /// `σγ`（Def 45 式 (40)）：活跃提供者安装的绑定 realm 集合
    /// （仅 Active fiber 的绑定计入）。
    pub fn provided(&self) -> KeySet {
        let store = self.store.borrow();
        store
            .symbols()
            .filter(|r| {
                store
                    .binding(*r)
                    .and_then(|b| b.provider)
                    .is_some_and(|p| self.is_active(p))
            })
            .collect()
    }

    /// 共效应表 `σ` 的只读快照（绑定 realm 集合等；供监控与测试）。
    ///
    /// **借用警告**：持有返回的 `Ref` 期间调用 [`Context::set`]（borrow_mut）
    /// 会 `RefCell` panic——读取借用须在变更前释放。
    pub fn store(&self) -> Ref<'_, Store> {
        self.store.borrow()
    }

    /// 静止判定 `quiet(γ)`（Def 46 式 (42)）：每个 fiber 都处于其目标
    /// （无转换在途）。
    pub fn is_quiet(&self) -> bool {
        self.fibers.borrow().values().all(|f| {
            let target = self.compute_target(f);
            match &*f.state.borrow() {
                FiberState::Inactive(_) => target.is_none(),
                FiberState::Active { view } => target.as_ref() == Some(view),
                _ => false, // 转换在途
            }
        })
    }

    // ── Algorithm 4：use（组件实例化）──────────────────────────────────

    /// Algorithm 4 的 `use(ctx, component, config)`：在 `ctx` 下实例化组件。
    ///
    /// O-Insert 前提（`∀m. p ∩ p_m = ∅`）不满足时返回
    /// [`RegistryError::ProvisionClash`]。注册回调为 `ctx` 上的可逆效应
    /// （Def 47）：应用 = 启动子生命周期（refresh）；逆 = O-Retire
    /// （[`Fiber::retire`]）——父上下文卸载时级联退役子。
    pub(crate) fn register(
        &self,
        ctx: &Rc<Context>,
        component: Rc<dyn Component>,
        config: Box<dyn Any>,
    ) -> Result<Rc<Fiber>, RegistryError> {
        let provide = component.provide();
        if self
            .fibers
            .borrow()
            .values()
            .any(|f| f.provide.intersects(&provide))
        {
            return Err(RegistryError::ProvisionClash);
        }
        let id = self.fresh_id();
        let inject = component.inject();
        let fiber_ctx = ctx.derive_for_fiber(id);
        let apply: Box<dyn Fn() -> Box<dyn EffectIter>> = {
            let component = Rc::clone(&component);
            let fiber_ctx = Rc::clone(&fiber_ctx);
            Box::new(move || component.apply(Rc::clone(&fiber_ctx), config.as_ref()))
        };
        let fiber = Rc::new(Fiber {
            id,
            parent: ctx.fiber,
            inject,
            provide,
            ctx: fiber_ctx,
            apply,
            retired: Cell::new(false),
            state: RefCell::new(FiberState::Inactive(None)),
            target: RefCell::new(None),
            committed: RefCell::new(None),
            dispose: RefCell::new(Vec::new()),
        });
        self.fibers.borrow_mut().insert(id, Rc::clone(&fiber));

        // 注册回调效应（Algorithm 4 第 2–6 行）：已组合进 ctx 累加器
        // （父卸载级联），返回的 disposer 故意忽略——drop 表明意图。
        drop(ctx.effect(|| -> Box<dyn EffectIter> {
            let runtime = Rc::clone(ctx.runtime());
            let fiber = Rc::clone(&fiber);
            Box::new(once(Box::new(move || {
                runtime.refresh(&fiber);
                let fiber = Rc::clone(&fiber);
                Box::new(move || fiber.retire()) as Disposer
            })))
        }));
        Ok(fiber)
    }

    /// O-Remove：`τ_n = ⊤ ∧ θ_n = Inactive ∧ ∀m. π_m ≠ n ⇒ γ ⇒ γ∖n`。
    ///
    /// **幽灵 fiber（审查 m2）**：仅从 registry 移除条目；fiber 对象仍被
    /// 父上下文的注册回调闭包持有，直至父 `dispose_all` 才释放。功能上
    /// 安全（`retire` 幂等、`refresh` 对已移除 fiber 无操作），但"移除后
    /// 对象仍存活"是预期语义——调用方不得在移除后依赖 `Runtime::fiber`
    /// 查询结果（返回 `None`）。
    pub fn remove_fiber(&self, id: FiberId) -> Result<(), RegistryError> {
        let fiber = self
            .fibers
            .borrow()
            .get(&id)
            .cloned()
            .ok_or(RegistryError::UnknownFiber)?;
        if !fiber.retired.get() {
            return Err(RegistryError::NotRetired);
        }
        if !matches!(*fiber.state.borrow(), FiberState::Inactive(_)) {
            return Err(RegistryError::StillActive);
        }
        if self.fibers.borrow().values().any(|m| m.parent == Some(id)) {
            return Err(RegistryError::HasChildren);
        }
        self.fibers.borrow_mut().remove(&id);
        Ok(())
    }

    // ── Algorithm 3：通知传播（内置 fiber 反应器）──────────────────────

    /// notify（Algorithm 3 骨架）：快照迭代反应器（审查 M1）。
    pub(crate) fn notify(&self, ctx: &Context, keys: &[Symbol]) {
        let reactors: Vec<Reactor> = self.reactors.borrow().iter().cloned().collect();
        for reactor in reactors {
            reactor(ctx, keys);
        }
    }

    /// Algorithm 3：binding 变更后重估受影响 fiber。
    ///
    /// 载荷为**已解析的 realm**（审查 M1 统一）；匹配按 realm 语义：
    /// fiber 的某个注入键 `ik` 经其自身 `ρ` 解析到载荷 realm 即受影响
    /// （等价于论文的 `key ∈ fiber.inject ∧ fiber.ctx[@@isolate][key] =
    /// ctx[@@isolate][key]`）。refresh 幂等：target 未变则无操作。
    pub(crate) fn notify_fibers(&self, keys: &[Symbol]) {
        let affected: Vec<Rc<Fiber>> = {
            let fibers = self.fibers.borrow();
            fibers
                .values()
                .filter(|f| {
                    keys.iter()
                        .any(|r| f.inject.iter().any(|ik| f.ctx.resolve_realm(ik) == *r))
                })
                .cloned()
                .collect()
        };
        for fiber in affected {
            self.refresh(&fiber);
        }
    }

    // ── Algorithm 5：refresh / reload / unload（惯性状态机）─────────────

    /// refresh（Algorithm 5 第 1–11 行）：重算 `target_n(γ)`；变化时启动
    /// 转换。转换在途（Reloading/Unloading）时推迟——惯性，转换完成时
    /// 按新目标自链（§4.3.3）。
    pub fn refresh(&self, fiber: &Rc<Fiber>) {
        let target = self.compute_target(fiber);
        {
            let mut recorded = fiber.target.borrow_mut();
            if *recorded == target {
                return;
            }
            *recorded = target;
        }
        // 注意：状态借用只活在语句级（match 分支体内不得持借——
        // reload/unload 内部会 borrow_mut state）。
        if matches!(
            &*fiber.state.borrow(),
            FiberState::Reloading { .. } | FiberState::Unloading { .. }
        ) {
            return; // 转换在途：惯性，完成时自链
        }
        if matches!(&*fiber.state.borrow(), FiberState::Active { .. }) {
            // L-Unload：目标已变（依赖消失/退役）→ 先停用，完成时自链。
            self.mark_unloading(fiber);
            self.unload(fiber);
            return;
        }
        if fiber.target.borrow().is_some() {
            self.reload(fiber);
        }
    }

    /// L-Leave（§4.3.1）：标记 Unloading——先停止提供（σγ 排除），
    /// 逆（dispose）在依赖者停用后执行。
    fn mark_unloading(&self, fiber: &Rc<Fiber>) {
        let view = fiber.committed.borrow().clone().unwrap_or_default();
        *fiber.state.borrow_mut() = FiberState::Unloading {
            view,
            outcome: None,
        };
    }

    /// reload（Algorithm 5 第 12–23 行）：执行组件效应并承诺视图。
    ///
    /// - `committed ← resolve(inject)`（当前各键提供者）；
    /// - 驱动效应迭代器，guard 在每个步骤边界检查 `fiber.target == target0`
    ///   （§4.3.2 步界中断：目标变化即停止，仅已完成步骤被恢复）；
    /// - 完成检查（惯性链）：目标未变 → `Active` + 通知依赖者；否则链式卸载。
    fn reload(&self, fiber: &Rc<Fiber>) {
        let target0 = fiber
            .target
            .borrow()
            .clone()
            .expect("reload 仅在 target = Some 时启动");
        let committed = self
            .resolve_view(fiber)
            .expect("target = Some ⟹ 全部声明键有活跃提供者");
        *fiber.committed.borrow_mut() = Some(committed.clone());
        *fiber.state.borrow_mut() = FiberState::Reloading {
            view: committed.clone(),
        };

        let guard_target = target0.clone();
        let guard = {
            let fiber = Rc::clone(fiber);
            move || fiber.target.borrow().as_ref() == Some(&guard_target)
        };
        let iter = (fiber.apply)();
        let recover = execute(iter, guard);
        fiber.dispose.borrow_mut().push(recover);

        if fiber.target.borrow().as_ref() == Some(&target0) {
            *fiber.state.borrow_mut() = FiberState::Active { view: committed };
            let provided = self.provided_of(fiber);
            fiber.ctx.notify(&provided);
        } else {
            self.mark_unloading(fiber);
            self.unload(fiber);
        }
    }

    /// unload（Algorithm 5 第 24–34 行）：
    ///
    /// 1. 通知依赖者（同步级联跑完——依赖者先于本 fiber 停用，且其 teardown
    ///    期间本 fiber 的绑定保持可读，Thm 63 顺序）；
    /// 2. 恢复：`fiber.dispose`（各次激活的恢复组合）+ `ctx` 累加器
    ///    （经 `set`/`effect` 注册的效应）——每步 armed 幂等，双路径安全；
    /// 3. 收尾：target ⊥ → `Inactive`；否则链式 reload（惯性）。
    fn unload(&self, fiber: &Rc<Fiber>) {
        self.mark_unloading(fiber);

        // 1. 依赖者先撤（Thm 63 的 ordering half）。
        let provided = self.provided_of(fiber);
        fiber.ctx.notify(&provided);

        // 2. 逆执行（LIFO）。
        let disposers: Vec<Disposer> = fiber.dispose.borrow_mut().drain(..).rev().collect();
        for disposer in disposers {
            disposer();
        }
        fiber.ctx.dispose_all();

        // 3. 收尾 + 惯性链。
        match fiber.target.borrow().clone() {
            None => {
                *fiber.committed.borrow_mut() = None;
                *fiber.state.borrow_mut() = FiberState::Inactive(None);
            }
            Some(_) => self.reload(fiber),
        }
    }

    // ── 派生量查询（Def 45/46）─────────────────────────────────────────

    /// `σγ` 视角的键提供者（Def 45 式 (40)）：仅 **Active** fiber 安装的
    /// 绑定计入；根绑定（provider = None）与不活跃提供者视为未提供。
    fn provider_of(&self, ctx: &Context, key: Symbol) -> Option<FiberId> {
        let realm = ctx.resolve_realm(key);
        let provider = self
            .store
            .borrow()
            .binding(realm)
            .and_then(|b| b.provider)?;
        self.is_active(provider).then_some(provider)
    }

    fn is_active(&self, id: FiberId) -> bool {
        let fibers = self.fibers.borrow();
        let Some(fiber) = fibers.get(&id) else {
            return false;
        };
        matches!(&*fiber.state.borrow(), FiberState::Active { .. })
    }

    /// fiber 的满足谓词 `γ ⊧ d`（Def 24，经 fiber.ctx 的 `ρ` 解析）。
    fn satisfied(&self, fiber: &Fiber) -> bool {
        fiber
            .inject
            .iter()
            .all(|k| self.provider_of(&fiber.ctx, k).is_some())
    }

    /// `target_n(γ)`（Def 46 式 (41)）：`⊥` 用 `None`。
    fn compute_target(&self, fiber: &Fiber) -> Option<View> {
        if fiber.retired.get() || !self.satisfied(fiber) {
            return None;
        }
        Some(
            fiber
                .inject
                .iter()
                .map(|k| {
                    (
                        k,
                        self.provider_of(&fiber.ctx, k)
                            .expect("satisfied ⟹ provider"),
                    )
                })
                .collect(),
        )
    }

    /// `resolve(inject)`（Algorithm 5 第 14 行）：各声明键的当前提供者。
    fn resolve_view(&self, fiber: &Fiber) -> Option<View> {
        let mut view = BTreeMap::new();
        for k in fiber.inject.iter() {
            let provider = self.provider_of(&fiber.ctx, k)?;
            view.insert(k, provider);
        }
        Some(view)
    }

    /// `provided(fiber)`（Algorithm 5 第 19/25 行）：本 fiber 安装的键。
    fn provided_of(&self, fiber: &Fiber) -> Vec<Symbol> {
        self.store.borrow().realms_with_provider(fiber.id)
    }

    fn fresh_id(&self) -> FiberId {
        let mut next = self.next.get();
        let id = FiberId::fresh(&mut next);
        self.next.set(next);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::effect::Step;
    use crate::key::Key;
    use crate::keyset::KeySet;

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

    struct ChildKey;
    impl Key for ChildKey {
        type Value = String;
        const SYMBOL: &'static str = "child";
    }

    struct K1Key;
    impl Key for K1Key {
        type Value = usize;
        const SYMBOL: &'static str = "k1";
    }

    struct K2Key;
    impl Key for K2Key {
        type Value = usize;
        const SYMBOL: &'static str = "k2";
    }

    fn sym(name: &str) -> Symbol {
        Symbol::intern(name)
    }

    fn spec(names: &[&str]) -> KeySet {
        names.iter().map(|s| sym(s)).collect()
    }

    /// 测试组件：d/p + 效应程序（闭包）。
    struct TestComponent {
        inject: KeySet,
        provide: KeySet,
        effects: Box<dyn Fn(Rc<Context>) -> Box<dyn EffectIter>>,
    }

    impl Component for TestComponent {
        fn inject(&self) -> KeySet {
            self.inject.clone()
        }
        fn provide(&self) -> KeySet {
            self.provide.clone()
        }
        fn apply(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
            (self.effects)(ctx)
        }
    }

    /// 提供者：无依赖，绑定 db。
    fn provider(db_value: &'static str) -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["db"]),
            effects: Box::new(move |ctx| {
                Box::new(once(Box::new(move || {
                    ctx.set::<DbKey>(db_value.to_string()).expect("绑定 db")
                })))
            }),
        })
    }

    /// 消费者：注入 db，读取后绑定 app。
    fn consumer() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&["db"]),
            provide: spec(&["app"]),
            effects: Box::new(|ctx| {
                Box::new(once(Box::new(move || {
                    // 读取借用须在 set 前释放（m-A 借用纪律）。
                    let value = {
                        let db = ctx.get::<DbKey>().expect("依赖可用");
                        format!("app({db})")
                    };
                    ctx.set::<AppKey>(value).expect("绑定 app")
                })))
            }),
        })
    }

    /// 消费者（两步）：第 1 步绑定 app；第 2 步产出「teardown 检查逆」——
    /// 卸载时逆序执行，先断言依赖 db 仍可读（Thm 63 的 teardown 可读性）。
    fn consumer_asserting_readable_teardown() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&["db"]),
            provide: spec(&["app"]),
            effects: Box::new(|ctx| Box::new(TwoStepTeardown { ctx, step: 0 })),
        })
    }

    struct TwoStepTeardown {
        ctx: Rc<Context>,
        step: usize,
    }

    impl EffectIter for TwoStepTeardown {
        fn next(&mut self) -> Step {
            self.step += 1;
            match self.step {
                1 => Step::Yielded({
                    // 读取借用须在 set 前释放（m-A 借用纪律）。
                    let value = {
                        let db = self.ctx.get::<DbKey>().expect("依赖可用");
                        format!("app({db})")
                    };
                    self.ctx.set::<AppKey>(value).expect("绑定 app")
                }),
                2 => Step::Finished(Box::new({
                    let ctx = Rc::clone(&self.ctx);
                    move || {
                        assert!(
                            ctx.get::<DbKey>().is_ok(),
                            "Thm 63：依赖者 teardown 期间依赖仍可读"
                        );
                    }
                }) as Disposer),
                _ => panic!("迭代器必须终止（协议约束）"),
            }
        }
    }

    #[test]
    fn use_activates_when_dependencies_satisfied() {
        // 无依赖组件：use 即激活（注册回调内 refresh → reload 同步完成）。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let a = root.use_component(provider("pg"), Box::new(())).unwrap();
        assert!(matches!(&*a.state(), FiberState::Active { .. }));
        // 绑定已安装且携带提供者（σγ 推导依据）。
        let realm = sym("db");
        assert_eq!(
            runtime
                .store
                .borrow()
                .binding(realm)
                .and_then(|b| b.provider),
            Some(a.id())
        );
        assert_eq!(root.get::<DbKey>().unwrap().as_str(), "pg");
    }

    #[test]
    fn dependency_ordering_both_orders() {
        // B 注入 db；无论 A/B 谁先 use，A 激活后 B 才激活（§3.2.2 前半）。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        // 顺序 1：先 B 后 A。
        let b = root.use_component(consumer(), Box::new(())).unwrap();
        assert!(
            matches!(&*b.state(), FiberState::Inactive(_)),
            "依赖未满足时保持 Inactive"
        );
        let a = root.use_component(provider("pg"), Box::new(())).unwrap();
        assert!(matches!(&*a.state(), FiberState::Active { .. }));
        assert!(
            matches!(&*b.state(), FiberState::Active { .. }),
            "A 激活的 notify 触发 B 激活"
        );
        assert_eq!(root.get::<AppKey>().unwrap().as_str(), "app(pg)");
    }

    #[test]
    fn withdrawal_cascade_disposes_dependents_first() {
        // Thm 63 顺序：退役提供者 → 依赖者先停用；依赖者 teardown 期间
        // 依赖仍可读（Teardown 检查逆在依赖者逆序恢复中先于提供者 dispose）。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let a = root.use_component(provider("pg"), Box::new(())).unwrap();
        let b = root
            .use_component(consumer_asserting_readable_teardown(), Box::new(()))
            .unwrap();
        assert!(matches!(&*b.state(), FiberState::Active { .. }));

        a.retire();
        assert!(matches!(&*a.state(), FiberState::Inactive(_)));
        assert!(
            matches!(&*b.state(), FiberState::Inactive(_)),
            "依赖者级联停用"
        );
        assert!(root.get::<DbKey>().is_err(), "绑定已恢复");
        assert!(root.get::<AppKey>().is_err());
    }

    #[test]
    fn parent_unload_cascades_to_children() {
        // Def 47 / Algorithm 4：子组件注册为父上下文的效应——
        // 父卸载时级联退役并卸载子。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let child_comp = Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["child"]),
            effects: Box::new(|ctx| {
                Box::new(once(Box::new(move || {
                    ctx.set::<ChildKey>("c".into()).expect("绑定 child")
                })))
            }),
        });
        let child_comp2 = Rc::clone(&child_comp);
        let parent_comp = Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["k1"]),
            effects: Box::new(move |ctx| {
                let _child = ctx
                    .use_component(Rc::clone(&child_comp2) as Rc<dyn Component>, Box::new(()))
                    .unwrap();
                Box::new(once(Box::new(move || {
                    ctx.set::<K1Key>(1).expect("绑定 parent")
                })))
            }),
        });
        let parent = root.use_component(parent_comp, Box::new(())).unwrap();
        let child = runtime
            .fibers
            .borrow()
            .values()
            .find(|f| f.provide.contains(sym("child")))
            .cloned()
            .expect("子已注册");
        assert!(matches!(&*child.state(), FiberState::Active { .. }));

        parent.retire();
        assert!(matches!(&*parent.state(), FiberState::Inactive(_)));
        assert!(
            matches!(&*child.state(), FiberState::Inactive(_)),
            "子随父级联卸载"
        );
        // 有子代（已退役未移除）不可移除父（O-Remove 前提）。
        assert_eq!(
            runtime.remove_fiber(parent.id()),
            Err(RegistryError::HasChildren)
        );
    }

    #[test]
    fn target_change_mid_reload_chains_unload() {
        // §4.3.3 惯性：转换中途目标变化（apply 内自退役）→ refresh 推迟、
        // 完成时链式卸载，已应用步骤全部恢复。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let comp = Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["k1", "k2"]),
            effects: Box::new(|ctx| Box::new(SelfRetiring { ctx, step: 0 })),
        });
        struct SelfRetiring {
            ctx: Rc<Context>,
            step: usize,
        }
        impl EffectIter for SelfRetiring {
            fn next(&mut self) -> Step {
                self.step += 1;
                match self.step {
                    1 => Step::Yielded(self.ctx.set::<K1Key>(1).expect("绑定 k1")),
                    2 => {
                        // 第 2 步：在 registry 中定位自己并退役
                        // （目标 → ⊥；转换在途 → refresh 惯性推迟）。
                        let me = self
                            .ctx
                            .runtime()
                            .fibers
                            .borrow()
                            .values()
                            .find(|f| Rc::ptr_eq(&f.ctx, &self.ctx))
                            .cloned();
                        if let Some(me) = me {
                            me.retire();
                        }
                        Step::Finished(self.ctx.set::<K2Key>(2).expect("绑定 k2"))
                    }
                    _ => panic!("迭代器必须终止（协议约束）"),
                }
            }
        }
        let fiber = root.use_component(comp, Box::new(())).unwrap();
        assert!(
            matches!(&*fiber.state(), FiberState::Inactive(_)),
            "中途退役 → 链式卸载 → Inactive"
        );
        assert!(fiber.retired());
        assert!(root.get::<K1Key>().is_err(), "已应用步骤被恢复");
        assert!(root.get::<K2Key>().is_err());
        assert!(runtime.is_quiet());
    }

    #[test]
    fn provision_clash_rejected() {
        // O-Insert 前提：供给不相交（单一来源纪律）。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        root.use_component(provider("pg"), Box::new(())).unwrap();
        assert!(matches!(
            root.use_component(provider("mysql"), Box::new(())),
            Err(RegistryError::ProvisionClash)
        ));
    }

    #[test]
    fn remove_preconditions() {
        // O-Remove 前提：退役 + Inactive + 无子代。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let a = root.use_component(provider("pg"), Box::new(())).unwrap();
        assert_eq!(runtime.remove_fiber(a.id()), Err(RegistryError::NotRetired));
        a.retire();
        // 同步卸载已完成：退役后即可移除。
        assert_eq!(runtime.remove_fiber(a.id()), Ok(()));
        assert!(runtime.fiber(a.id()).is_none());
        assert_eq!(
            runtime.remove_fiber(a.id()),
            Err(RegistryError::UnknownFiber)
        );
    }

    #[test]
    #[should_panic(expected = "越界写入")]
    fn set_outside_provision_panics() {
        // Def 43/48 纪律执行期检查：组件写入未声明的供给 → panic（bug）。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let comp = Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["db"]),
            effects: Box::new(|ctx| {
                Box::new(once(Box::new(move || {
                    ctx.set::<AppKey>("evil".into()).expect("绑定 app")
                })))
            }),
        });
        let _ = root.use_component(comp, Box::new(()));
    }

    #[test]
    fn quiet_after_activation_and_cascade() {
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        assert!(runtime.is_quiet(), "空 registry 恒静止");
        let a = root.use_component(provider("pg"), Box::new(())).unwrap();
        let b = root.use_component(consumer(), Box::new(())).unwrap();
        assert!(runtime.is_quiet());
        a.retire();
        assert!(runtime.is_quiet(), "级联后回到静止");
        assert!(matches!(&*b.state(), FiberState::Inactive(_)));
    }

    #[test]
    fn isolated_provider_notifies_dependents() {
        // 审查 M1 回归：隔离场景（isolate(db, realm)）下，激活/停用通知
        // 按 **realm** 语义匹配——依赖者必须收到提供者的级联通知。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let realm = Symbol::intern("realm-db");
        // 提供者与依赖者都从「db → realm」隔离的上下文中实例化
        // （fiber.ctx 继承 ρ，绑定落在 realm）。
        let ctx_a = root.isolate(sym("db"), realm);
        let ctx_b = root.isolate(sym("db"), realm);
        let a = ctx_a.use_component(provider("pg"), Box::new(())).unwrap();
        let b = ctx_b.use_component(consumer(), Box::new(())).unwrap();
        assert!(
            matches!(&*b.state(), FiberState::Active { .. }),
            "隔离提供者的激活必须通知到依赖者"
        );
        assert!(
            root.get::<DbKey>().is_err(),
            "根上下文解析到自身 realm，不可见"
        );

        // 停用级联同样按 realm 传播。
        a.retire();
        assert!(matches!(&*a.state(), FiberState::Inactive(_)));
        assert!(
            matches!(&*b.state(), FiberState::Inactive(_)),
            "隔离提供者的停用必须级联依赖者"
        );
        assert!(runtime.is_quiet());
    }

    #[test]
    fn cross_isolation_blocks_dependency() {
        // 审查 M1 负例：依赖者隔离到**不同** realm → 不满足、不被通知。
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        let ctx_a = root.isolate(sym("db"), Symbol::intern("realm-a"));
        let ctx_c = root.isolate(sym("db"), Symbol::intern("realm-b"));
        let a = ctx_a.use_component(provider("pg"), Box::new(())).unwrap();
        let c = ctx_c.use_component(consumer(), Box::new(())).unwrap();
        assert!(matches!(&*a.state(), FiberState::Active { .. }));
        assert!(
            matches!(&*c.state(), FiberState::Inactive(_)),
            "不同 realm 的绑定不可见（Def 28/29 隔离语义）"
        );
        a.retire();
        assert!(matches!(&*c.state(), FiberState::Inactive(_)));
        assert!(runtime.is_quiet());
    }
}
