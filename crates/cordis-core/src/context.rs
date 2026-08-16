//! 上下文（论文 Def 32 的 `Γ∞` 投影）与共效应操作（§5.1.2）。
//!
//! 每个上下文承载三个符号键控槽位（§5.1.2）：
//!
//! - `@@store`（[`crate::runtime::Runtime`] 共享的 [`Store`]）：`σ: (r: R) ⇀ 𝒱 r`
//!   （Def 28），按 realm 键控；
//! - `@@isolate`（本上下文的 `ρ`）：键 → realm 的重定向表（Def 28），
//!   未隔离的键解析到自身（`ρ(k) = k`）；
//! - `@@intercept`（本上下文的 `ι`）：键 → 拦截元数据的表（Def 30）。
//!
//! 操作（Algorithm 2、Def 29/31）：[`Context::get`] / [`Context::set`] /
//! [`Context::isolate`] / [`Context::intercept`]。`set` 是 [`Context::effect`]
//! 上的可逆效应（Def 23 注：`set` 类型即 `𝔈*_Σ`），绑定与撤销两侧都触发
//! [`Context::notify`]；绑定携带安装者（fiber 归属，Def 45）。
//!
//! 组件实例化经 [`Context::use_component`]（Algorithm 4，注册回调为父上下文
//! 的可逆效应——父卸载级联退役子，Def 47）。
//!
//! **撤销顺序（审查 M-A 修复）**：每步逆在**产出时**即推入累加器（应用序
//! LIFO，与论文 "prepending each new inverse therefore yields LIFO recovery"
//! 一致），嵌套效应自然按应用逆序交错。

use std::any::Any;
use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::component::Component;
use crate::effect::{Disposer, EffectIter, Step, StepGuard, execute, once};
use crate::fiber::{Fiber, FiberId};
use crate::key::Key;
use crate::keyset::KeySet;
use crate::runtime::{RegistryError, Runtime};
use crate::store::{Store, StoreError};
use crate::symbol::Symbol;

/// 拦截元数据（Def 30 的 `ℳ k`，带幺半群 `⊕k`）。
///
/// 实现须给出右偏合并：`merge(existing, new)` 中 `new`（后拦截的元数据）
/// 优先于 `existing`（§5.1.2："the new metadata ... takes priority"）。
///
/// 对象安全（dyn-clone 模式）：`merge` 返回 `Self`，以 `where Self: Sized`
/// 从 vtable 排除（不可经 `dyn` 调用，但 trait 仍可对象化）；`clone_box`
/// 提供深拷贝（派生上下文继承 `ι` 所需）。读取 API
/// （[`Context::intercept_of`]）在方法上以 `Clone` 约束具体类型。
///
/// 不要求 `Send + Sync`（审查 m4）：宿主为单线程 `Rc` 结构（ADR-0002），
/// 元数据只存活于进程内、不跨线程/不跨边界。
pub trait InterceptMeta: Any + 'static {
    /// 右偏合并（`new` 优先；语义由各键的 `⊕k` 决定，如标量覆写、集合取并）。
    fn merge(existing: &Self, new: &Self) -> Self
    where
        Self: Sized;

    /// 深拷贝（派生上下文继承 `ι` 所需）；通常实现为 `Box::new(self.clone())`。
    fn clone_box(&self) -> Box<dyn InterceptMeta>;
}

/// 上下文（`Γ∞` 投影：`ρ` + `ι` + 累加器 + fiber 归属；`σ` 共享自
/// [`Runtime`]）。
pub struct Context {
    runtime: Rc<Runtime>,
    /// `ρ`：键 → realm 的重定向表（Def 28）；未在表中的键解析到自身。
    realms: RefCell<HashMap<Symbol, Symbol>>,
    /// `ι`：键 → 拦截元数据（Def 30）。
    intercept: RefCell<HashMap<Symbol, Box<dyn InterceptMeta>>>,
    /// 本上下文累加器（Algorithm 1 第 17 行的 `ctx.dispose`；Def 6 的 `recover`）。
    dispose: RefCell<Vec<Disposer>>,
    /// 归属 fiber（None = root/编排器；fiber.ctx 即 Algorithm 4 第 8 行的
    /// 派生上下文）。绑定据此记录提供者（Def 45）。
    pub(crate) fiber: Option<FiberId>,
}

impl Context {
    /// 新建独立根上下文（自带运行时）。
    pub fn new() -> Rc<Self> {
        Rc::new(Self::new_with(&Rc::new(Runtime::new())))
    }

    pub(crate) fn new_with(runtime: &Rc<Runtime>) -> Self {
        Self {
            runtime: Rc::clone(runtime),
            realms: RefCell::new(HashMap::new()),
            intercept: RefCell::new(HashMap::new()),
            dispose: RefCell::new(Vec::new()),
            fiber: None,
        }
    }

    /// 归属 fiber（None = root）。
    pub fn fiber(&self) -> Option<FiberId> {
        self.fiber
    }

    /// 运行时引用（共享 `σ` 与 registry `Fγ`）。
    pub fn runtime(&self) -> &Rc<Runtime> {
        &self.runtime
    }

    /// `ρ(k)`：解析键的 realm；未隔离的键解析到自身（Def 28：`R ⊇ K`）。
    pub(crate) fn resolve_realm(&self, key: Symbol) -> Symbol {
        self.realms.borrow().get(&key).copied().unwrap_or(key)
    }

    /// 共效应表的可变访问（仅测试使用；公开读写经 [`Context::get`]/[`Context::set`]）。
    #[cfg(test)]
    pub(crate) fn store_cell(&self) -> &RefCell<Store> {
        &self.runtime.store
    }

    /// 共效应表的只读视图（`σ` 的当前快照）。
    ///
    /// **借用警告（审查 m-A）**：持有返回的 `Ref` 期间调用 [`Context::effect`]
    /// 会触发 `RefCell` 双重借用 panic（effect 内部需要 `borrow_mut`）；需要
    /// 在 effect 之前结束读取借用。
    pub fn store(&self) -> Ref<'_, Store> {
        self.runtime.store.borrow()
    }

    /// `get(k)`（Def 29 / Algorithm 2）：按 `ρ(k)` 解析并读取绑定。
    /// 前置条件 `ρ(k) ∈ dom(σ)`。
    ///
    /// 返回的 [`Ref`] 持有 store 的读取借用：借用期间调用 [`Context::set`]/
    /// [`Context::effect`] 会 `RefCell` panic（与 [`Context::store`] 同一纪律）。
    pub fn get<K: Key>(&self) -> Result<Ref<'_, K::Value>, StoreError> {
        let realm = self.resolve_realm(Symbol::intern(K::SYMBOL));
        let store = self.runtime.store.borrow();
        // 先经错误路径检查（存在性 + 类型），再经借用守卫映射出内部引用。
        store.get::<K>(realm)?;
        Ok(Ref::map(store, |s| {
            s.get::<K>(realm).expect("checked above")
        }))
    }

    /// `set(k, v)`（Def 29 / Algorithm 2）：绑定于 `ρ(k)` 的 realm，**可逆**
    /// （`set` 的类型即 `𝔈*_Σ`，Def 23 注；经 [`Context::effect`] 自动追踪）。
    ///
    /// 前置条件 `ρ(k) ∉ dom(σ)`（Def 23 沿 `ρ` 转译）：违反则返回 `Err`，
    /// **不产生状态变更**。绑定与撤销两侧均触发 [`Context::notify`]
    /// （Algorithm 2 第 8/11 行）。绑定携带安装者（`self.fiber`，Def 45）。
    ///
    /// **供给纪律（Def 43/48，执行期检查）**：组件（`self.fiber = Some(n)`）
    /// 只能写入其声明的供给 `p_n`——越界写入 panic（bug）。
    ///
    /// 返回的 disposer 撤销本绑定；同时已在累加器与 notify 中登记。
    ///
    /// **错误载体（审查 m2）**：本层报**用户键**（`AlreadyBound(key)`）；
    /// 表层 [`Store::bind`] 报 **realm**（`AlreadyBound(realm)`）——未隔离时
    /// 两者相同，隔离场景以各自语义为准（THEORY-MAP 已知偏差）。
    pub fn set<K: Key>(self: &Rc<Self>, value: K::Value) -> Result<Disposer, StoreError> {
        let key = Symbol::intern(K::SYMBOL);
        let realm = self.resolve_realm(key);
        // Def 43/48 纪律：组件只写自己声明的供给。
        if let Some(fid) = self.fiber {
            let allowed = self
                .runtime
                .fibers
                .borrow()
                .get(&fid)
                .is_some_and(|f| f.provide.contains(key));
            if !allowed {
                panic!("组件 {fid:?} 越界写入未声明的键 {key}（Def 43/48 纪律）");
            }
        }
        // 前置检查（快速失败）：单线程 + 无重入路径下，检查与绑定之间无
        // TOCTOU 窗口，expect 安全（审查 m3）。**PR #6 async 化必改项**：
        // await 可插入其他任务，绑定失败须改为可传播错误而非 panic。
        if self.runtime.store.borrow().contains(realm) {
            return Err(StoreError::AlreadyBound(key));
        }
        Ok(self.effect(|| -> Box<dyn EffectIter> {
            let ctx = Rc::clone(self);
            Box::new(once(Box::new(move || {
                ctx.runtime
                    .store
                    .borrow_mut()
                    .bind::<K>(realm, value, ctx.fiber)
                    .expect("前置条件已检查（ρ(k) ∉ dom(σ)）");
                // 载荷为 **realm**（已解析；审查 M1 统一——依赖者按 realm 匹配）。
                ctx.notify(&[realm]);
                let ctx = Rc::clone(&ctx);
                Box::new(move || {
                    ctx.runtime
                        .store
                        .borrow_mut()
                        .unbind::<K>(realm)
                        .expect("绑定由本效应安装");
                    ctx.notify(&[realm]);
                }) as Disposer
            })))
        }))
    }

    /// `isolate(k, r)`（Def 29）：**派生实现**（Def 27）——返回新上下文，
    /// 把 `k` 的 realm 覆写为 `r`，继承其余 `ρ`、`ι` 与 fiber 归属；
    /// 不写共享表、无需逆。同一键在不同 realm 下解析到独立绑定
    /// （多租户/沙箱/测试隔离）。
    pub fn isolate(self: &Rc<Self>, key: Symbol, realm: Symbol) -> Rc<Context> {
        let mut realms = self.realms.borrow().clone();
        realms.insert(key, realm);
        Rc::new(Context {
            runtime: Rc::clone(&self.runtime),
            realms: RefCell::new(realms),
            intercept: RefCell::new(clone_intercept(&self.intercept)),
            dispose: RefCell::new(Vec::new()),
            fiber: self.fiber,
        })
    }

    /// `intercept(k, ν)`（Def 31）：**派生实现**——返回新上下文，把 `k` 的
    /// 拦截元数据与 `ν` 右偏合并（`ι(k) ⊕k ν`，`ν` 优先），继承其余 `ι`、`ρ`
    /// 与 fiber 归属。
    ///
    /// 同一键的多次拦截必须使用同一元数据类型 `M`（类型冲突 panic，
    /// panic = bug 策略，见 [`crate::effect`] 模块文档）。
    ///
    /// **⚠ 半成品警示（M0 审查 REVIEW-M0）**：当前仅**累积元数据**——读路径
    /// （[`Context::get`]）不消费 `ι`，拦截对绑定值读取无任何效果。论文
    /// Def 30/31 的 provider 函数形态（`get(k,μ) = σ(k)(μ⊕ₖι(k))`）由 M1
    /// 落地（THEORY-MAP「里程碑走查记录」处置清单①）。在此前以"拦截已
    /// 生效"为前提使用本 API 将得到静默无效果行为。
    pub fn intercept<M: InterceptMeta>(self: &Rc<Self>, key: Symbol, meta: M) -> Rc<Context> {
        let mut table = clone_intercept(&self.intercept);
        let merged = match table.get(&key) {
            None => meta,
            Some(existing) => {
                let existing = (existing.as_ref() as &dyn Any)
                    .downcast_ref::<M>()
                    .expect("拦截元数据类型冲突：同一 key 的多次拦截必须使用同一类型");
                M::merge(existing, &meta)
            }
        };
        table.insert(key, Box::new(merged));
        Rc::new(Context {
            runtime: Rc::clone(&self.runtime),
            realms: RefCell::new(self.realms.borrow().clone()),
            intercept: RefCell::new(table),
            dispose: RefCell::new(Vec::new()),
            fiber: self.fiber,
        })
    }

    /// 派生一个归属给定 fiber 的上下文（Algorithm 4 第 8 行的 `fiber.ctx`）：
    /// 继承 `ρ`、`ι`，空累加器。
    pub(crate) fn derive_for_fiber(self: &Rc<Self>, id: FiberId) -> Rc<Context> {
        Rc::new(Context {
            runtime: Rc::clone(&self.runtime),
            realms: RefCell::new(self.realms.borrow().clone()),
            intercept: RefCell::new(clone_intercept(&self.intercept)),
            dispose: RefCell::new(Vec::new()),
            fiber: Some(id),
        })
    }

    /// 读取 `k` 处合并后的拦截元数据（未安装或类型不符返回 `None`）。
    ///
    /// **⚠ 半成品警示（M0 审查 REVIEW-M0）**：同 [`Context::intercept`]——
    /// 元数据可读回，但读路径（[`Context::get`]）不消费 `ι`；provider 函数
    /// 求值形态（Def 30/31）由 M1 落地。
    pub fn intercept_of<M: InterceptMeta + Clone>(&self, key: Symbol) -> Option<M> {
        self.intercept
            .borrow()
            .get(&key)
            .and_then(|boxed| (boxed.as_ref() as &dyn Any).downcast_ref::<M>())
            .cloned()
    }

    /// 满足谓词 `γ ⊧ d`（Def 24，经 `ρ` 解析）：`∀k ∈ d. ρ(k) ∈ dom(σ)`。
    pub fn satisfies(&self, spec: &KeySet) -> bool {
        let store = self.runtime.store.borrow();
        spec.iter().all(|k| store.contains(self.resolve_realm(k)))
    }

    /// `notify(ctx, keys)`（Algorithm 3）：把 binding 变更传播给反应器
    /// （fiber 反应器已内置——按 realm 语义筛选并驱动 refresh；用户反应器
    /// 按注册顺序随后调用）。
    ///
    /// **载荷语义（审查 M1/m1 统一）**：`keys` 为**已解析的 realm 符号**
    /// （经本上下文 `ρ` 解析）——`set` 与 fiber 激活/停用均按 realm 广播。
    ///
    /// **重入纪律（审查 M1）**：以**快照迭代**调用反应器——反应器内注册
    /// 新反应器安全（本轮不触发）；但反应器内**同步触发新的共效应变更**
    /// （如 [`Context::set`]）会递归广播 notify——fiber 反应器经 refresh
    /// 惯性（inertia）切断同步递归；用户反应器应避免同步 set，或以守卫
    /// 显式切断（见 `nested_notify_terminates_with_guarded_reactor` 测试）。
    pub fn notify(&self, keys: &[Symbol]) {
        self.runtime.notify(self, keys);
    }

    /// Algorithm 4 的 `use(ctx, component, config)`（Rust 关键字规避命名）：
    /// 在 `self` 下实例化组件（O-Insert 前提：供给不相交）。
    ///
    /// 注册回调为 `self` 上的可逆效应（Def 47）：应用 = 启动子生命周期；
    /// 逆 = O-Retire——`self` 上下文卸载（如父组件卸载）时级联退役子。
    ///
    /// `config` 以 [`Rc`] 持有（PR #8 起）：配置可共享/复用——loader 等
    /// 编排方在重建条目时需要保留配置（`Box` 不可克隆）。
    pub fn use_component(
        self: &Rc<Self>,
        component: Rc<dyn Component>,
        config: Rc<dyn Any>,
    ) -> Result<Rc<Fiber>, RegistryError> {
        self.runtime.register(self, component, config)
    }

    /// `ctx.effect(callback)`（Algorithm 1 第 9–18 行的同步核心）。
    ///
    /// 以 `callback` 构造效应迭代器并立即执行至完成；**每步逆在产出时即推入
    /// 本上下文累加器**（应用序 LIFO，嵌套效应正确交错，审查 M-A）。返回的
    /// disposer 撤销本效应的全部步骤（与累加器路径共享每步 armed 句柄，至多
    /// 生效一次）；armed 标志（Algorithm 1 第 10–11 行）当前仅作 guard 输入，
    /// PR #5 async 时代实现「dispose 中断在途迭代」。
    ///
    /// **调用方需持有 `Rc<Context>`**（`self: &Rc<Self>`）：回调需要把上下文
    /// 克隆进迭代器闭包（'static 约束）。
    ///
    /// **迭代器必须有限终止**（审查 M-B，见 [`crate::effect`] 模块文档）。
    pub fn effect(self: &Rc<Self>, callback: impl FnOnce() -> Box<dyn EffectIter>) -> Disposer {
        let armed = Rc::new(Cell::new(true));
        let guard = {
            let armed = Rc::clone(&armed);
            move || armed.get()
        };
        let composite = execute(
            Box::new(PushingIter {
                inner: callback(),
                ctx: Rc::clone(self),
            }),
            guard,
        );
        let armed = Rc::clone(&armed);
        Box::new(move || {
            armed.set(false);
            composite();
        })
    }

    /// 把一步逆推入本上下文累加器（产出时调用，应用序）。
    fn push_step(&self, disposer: Disposer) {
        self.dispose.borrow_mut().push(disposer);
    }

    /// 运行本上下文累加器（LIFO 恢复全部已注册步骤；对应 Def 6 的 `recover`）。
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

/// 记录迭代器：把内层迭代器的每步逆在产出时即推入本上下文累加器，
/// 并返回一个共享同一 armed 语义的等价闭包给 execute 折叠。
struct PushingIter {
    inner: Box<dyn EffectIter>,
    ctx: Rc<Context>,
}

impl EffectIter for PushingIter {
    fn next(&mut self) -> Step {
        match self.inner.next() {
            Step::Yielded(inv) => {
                let (exec, acc) = guarded_pair(inv);
                self.ctx.push_step(acc);
                Step::Yielded(exec)
            }
            Step::Finished(inv) => {
                let (exec, acc) = guarded_pair(inv);
                self.ctx.push_step(acc);
                Step::Finished(exec)
            }
        }
    }
}

/// 把一个步逆包装为两个等价闭包（共享同一 `StepGuard`）：
/// 一个交给 execute 折叠（手动撤销路径），一个推入上下文累加器（teardown 路径）。
fn guarded_pair(inv: Disposer) -> (Disposer, Disposer) {
    let guard = StepGuard::new(inv);
    (guard.disposer(), guard.disposer())
}

/// 深拷贝拦截表（派生上下文继承 `ι`；`Box<dyn InterceptMeta>` 经 `clone_box`）。
fn clone_intercept(
    table: &RefCell<HashMap<Symbol, Box<dyn InterceptMeta>>>,
) -> HashMap<Symbol, Box<dyn InterceptMeta>> {
    table
        .borrow()
        .iter()
        .map(|(key, meta)| (*key, meta.clone_box()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::once;

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

    /// 绑定效应（Def 23 的 `set`：绑定 + 逆 = 撤销绑定）。
    fn bind_effect<K: Key>(ctx: &Rc<Context>, value: K::Value) -> Disposer {
        ctx.set::<K>(value).unwrap()
    }

    #[test]
    fn thm16_lifo_recovery_and_soundness_invariant() {
        // Thm 16(1)：逆序撤销时每一步恢复其应用时的状态；
        // Thm 16(2)：全部撤销后回到 γ₀（声音不变量 φ(γ) = γ₀）。
        let ctx = Context::new();
        let d1 = bind_effect::<KeyA>(&ctx, String::from("va"));
        let d2 = bind_effect::<KeyB>(&ctx, 7);

        // 两效应均已应用（execute 同步完成）。
        assert_eq!(&*ctx.get::<KeyA>().unwrap(), "va");
        assert_eq!(*ctx.get::<KeyB>().unwrap(), 7);

        // 逆序撤销：先 d2（恢复 e2 应用前的状态：e1 的绑定仍在）。
        d2();
        assert!(matches!(ctx.get::<KeyB>(), Err(StoreError::NotBound(_))));
        assert_eq!(&*ctx.get::<KeyA>().unwrap(), "va", "中间态：e1 不受影响");

        // 再 d1 → 回到 γ₀。
        d1();
        assert!(matches!(ctx.get::<KeyA>(), Err(StoreError::NotBound(_))));
        assert!(
            ctx.store().symbols().next().is_none(),
            "声音不变量：store 为空 = γ₀"
        );
    }

    /// 应用 4 个两两独立的 `set` 效应并按 `order` 排列撤销，断言回到 γ₀。
    ///
    /// 独立性（Def 19）：不同键的 `set` 效应——变换只动自己的键（可交换），
    /// 逆只撤自己的键（互不干扰）；每步撤销后断言剩余绑定数，检验
    /// Thm 20(1) 的中间态语义（`g_j` 在 δ_u 运行只撤回自己的贡献）。
    fn assert_permutation_reverts(order: &[usize; 4]) {
        let ctx = Context::new();
        let mut inverses: Vec<Option<Disposer>> = vec![
            Some(bind_effect::<KeyA>(&ctx, String::from("va"))),
            Some(bind_effect::<KeyB>(&ctx, 7)),
            Some(bind_effect::<KeyK1>(&ctx, 1)),
            Some(bind_effect::<KeyK2>(&ctx, 2)),
        ];
        assert_eq!(ctx.store().symbols().count(), 4, "δ₄：全部已应用");
        let mut undone = 0;
        for &i in order {
            let d = inverses[i - 1].take().expect("每个逆恰好调用一次");
            d();
            undone += 1;
            assert_eq!(
                ctx.store().symbols().count(),
                4 - undone,
                "排列 {order:?}：第 {undone} 步逆只撤自己的贡献（Thm 20(1)）"
            );
        }
        assert!(
            ctx.store().symbols().next().is_none(),
            "排列 {order:?} 撤销后回到 γ₀（Cor 21）"
        );
    }

    #[test]
    fn cor21_independent_effects_revert_in_any_permutation() {
        // Cor 21：设 e₁,…,eₙ 两两独立、自 γ₀ 依次应用，g₁,…,gₙ 为各步逆；
        // 在 δₙ 处以 {1..n} 的任意排列应用这 n 个逆都回到 γ₀。
        // LIFO 只是其中一种排列（Thm 16 无需独立性假设）；
        // 独立性（Def 19）买来的是**其余所有排列**。
        // 审查 nit 1 强化：穷举全部 4! = 24 种排列（而非代表选取）。
        let mut order = [1usize, 2, 3, 4];
        let mut seen = 0;
        loop {
            assert_permutation_reverts(&order);
            seen += 1;
            if !next_permutation(&mut order) {
                break;
            }
        }
        assert_eq!(seen, 24, "4! = 24 种排列全部验证");
    }

    /// 字典序下一个排列（就地改写）；已为最大排列时返回 `false`。
    fn next_permutation(order: &mut [usize; 4]) -> bool {
        // 从右向左找第一个升序对（a[i] < a[i+1]）。
        let mut i = order.len() - 1;
        while i > 0 && order[i - 1] >= order[i] {
            i -= 1;
        }
        if i == 0 {
            return false;
        }
        // 从右向左找第一个大于 order[i-1] 的元素并交换。
        let mut j = order.len() - 1;
        while order[j] <= order[i - 1] {
            j -= 1;
        }
        order.swap(i - 1, j);
        // 反转后缀 [i..] 使之为最小。
        order[i..].reverse();
        true
    }

    #[test]
    fn accumulator_reverts_all_effects_lifo() {
        // 累加器（dispose_all）按 LIFO 恢复全部注册效应（Thm 16(2)）。
        let ctx = Context::new();
        // 故意忽略返回的 disposer（累加器已持有同一步的等价闭包）：
        // 编译器视 Box<dyn FnOnce()> 为 must_use（unused boxed FnOnce trait
        // object 警告），用 drop 表明意图。
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
        // guard 更强）；运行期幂等针对「返回路径与累加器路径共享每步句柄」。
        let ctx = Context::new();
        let d = bind_effect::<KeyA>(&ctx, String::from("va"));
        d(); // 消耗返回的 disposer
        assert!(matches!(ctx.get::<KeyA>(), Err(StoreError::NotBound(_))));
        // 累加器中的等价闭包因共享句柄而 no-op：不得 panic、不得重复撤销。
        ctx.dispose_all();
        assert!(matches!(ctx.get::<KeyA>(), Err(StoreError::NotBound(_))));
    }

    #[test]
    fn effect_registers_into_context_accumulator() {
        // 不持有返回的 disposer，仅靠累加器即可恢复（fiber 卸载路径，Alg 5 第 26 行）。
        let ctx = Context::new();
        drop(ctx.set::<KeyA>(String::from("va")).unwrap());
        assert!(ctx.get::<KeyA>().is_ok());
        ctx.dispose_all();
        assert!(matches!(ctx.get::<KeyA>(), Err(StoreError::NotBound(_))));
    }

    struct KeyK1;
    impl Key for KeyK1 {
        type Value = usize;
        const SYMBOL: &'static str = "k1";
    }

    struct KeyK2;
    impl Key for KeyK2 {
        type Value = usize;
        const SYMBOL: &'static str = "k2";
    }

    struct KeyK3;
    impl Key for KeyK3 {
        type Value = usize;
        const SYMBOL: &'static str = "k3";
    }

    /// 审查 M-A 回归：内层效应注册于外层迭代步骤之间，
    /// `dispose_all` 必须按应用逆序交错撤销（Thm 16：E2 → E3 → E1）。
    #[test]
    fn nested_effect_reverts_in_application_order() {
        let ctx = Context::new();
        let log = Rc::new(RefCell::new(Vec::<String>::new()));

        struct Outer {
            ctx: Rc<Context>,
            log: Rc<RefCell<Vec<String>>>,
            step: usize,
        }
        impl EffectIter for Outer {
            fn next(&mut self) -> Step {
                self.step += 1;
                match self.step {
                    1 => {
                        // E1：绑定 k1（应用 t1）。
                        self.ctx
                            .store_cell()
                            .borrow_mut()
                            .bind::<KeyK1>(Symbol::intern("k1"), 1, None)
                            .unwrap();
                        let (ctx, log) = (Rc::clone(&self.ctx), Rc::clone(&self.log));
                        Step::Yielded(Box::new(move || {
                            log.borrow_mut().push("o1".into());
                            ctx.store_cell()
                                .borrow_mut()
                                .unbind::<KeyK1>(Symbol::intern("k1"))
                                .unwrap();
                        }) as Disposer)
                    }
                    2 => {
                        // E3：本步应用代码内注册内层效应（应用 t4，先于 E2）。
                        let inner_ctx = Rc::clone(&self.ctx);
                        let inner_log = Rc::clone(&self.log);
                        drop(self.ctx.effect(|| -> Box<dyn EffectIter> {
                            let ctx = Rc::clone(&inner_ctx);
                            let log = Rc::clone(&inner_log);
                            Box::new(once(Box::new(move || {
                                ctx.store_cell()
                                    .borrow_mut()
                                    .bind::<KeyK2>(Symbol::intern("k2"), 2, None)
                                    .unwrap();
                                let (ctx, log) = (Rc::clone(&ctx), Rc::clone(&log));
                                Box::new(move || {
                                    log.borrow_mut().push("i1".into());
                                    ctx.store_cell()
                                        .borrow_mut()
                                        .unbind::<KeyK2>(Symbol::intern("k2"))
                                        .unwrap();
                                }) as Disposer
                            })))
                        }));
                        // E2：绑定 k3（应用 t5）。
                        self.ctx
                            .store_cell()
                            .borrow_mut()
                            .bind::<KeyK3>(Symbol::intern("k3"), 3, None)
                            .unwrap();
                        let (ctx, log) = (Rc::clone(&self.ctx), Rc::clone(&self.log));
                        Step::Finished(Box::new(move || {
                            log.borrow_mut().push("o2".into());
                            ctx.store_cell()
                                .borrow_mut()
                                .unbind::<KeyK3>(Symbol::intern("k3"))
                                .unwrap();
                        }) as Disposer)
                    }
                    _ => panic!("迭代器必须终止（协议约束，审查 M-B）"),
                }
            }
        }

        drop(ctx.effect(|| -> Box<dyn EffectIter> {
            Box::new(Outer {
                ctx: Rc::clone(&ctx),
                log: Rc::clone(&log),
                step: 0,
            })
        }));

        assert_eq!(ctx.store().symbols().count(), 3, "k1、k2、k3 均已应用");
        ctx.dispose_all();
        assert_eq!(
            *log.borrow(),
            vec!["o2", "i1", "o1"],
            "撤销按应用逆序交错：E2 → E3 → E1（而非外层整组先撤）"
        );
        assert_eq!(ctx.store().symbols().count(), 0, "回到 γ₀");
    }

    // ── PR #4：共效应操作（Def 23/28/29/31，Algorithm 2/3）─────────────

    #[test]
    fn set_precondition_rejected_without_mutation() {
        // Def 23：set 前置条件 ρ(k) ∉ dom(σ)；违反报错且不产生状态变更。
        let ctx = Context::new();
        let d = ctx.set::<KeyA>(String::from("va")).unwrap();
        assert!(matches!(
            ctx.set::<KeyA>(String::from("vb")),
            Err(StoreError::AlreadyBound(_))
        ));
        assert_eq!(&*ctx.get::<KeyA>().unwrap(), "va", "原绑定保持");

        // 撤销后可重新绑定。
        d();
        drop(ctx.set::<KeyA>(String::from("vc")).unwrap());
        assert_eq!(&*ctx.get::<KeyA>().unwrap(), "vc");
    }

    #[test]
    fn get_precondition_rejected() {
        // Def 23：get 前置条件 ρ(k) ∈ dom(σ)。
        let ctx = Context::new();
        assert!(matches!(ctx.get::<KeyA>(), Err(StoreError::NotBound(_))));
    }

    #[test]
    fn set_notifies_on_bind_and_unbind() {
        // Algorithm 2：绑定与撤销两侧均触发 notify（Algorithm 3 骨架）。
        let runtime = Rc::new(Runtime::new());
        let ctx = runtime.context();
        let events = Rc::new(RefCell::new(Vec::<Vec<String>>::new()));
        let events2 = Rc::clone(&events);
        runtime.on_notify(move |_ctx, keys| {
            events2
                .borrow_mut()
                .push(keys.iter().map(|k| k.as_str().to_string()).collect());
        });

        let d = ctx.set::<KeyA>(String::from("va")).unwrap();
        assert_eq!(*events.borrow(), vec![vec!["a".to_string()]]);
        d();
        assert_eq!(
            *events.borrow(),
            vec![vec!["a".to_string()], vec!["a".to_string()]],
            "撤销侧同样通知"
        );
    }

    #[test]
    fn notify_reactor_can_register_new_reactor() {
        // 审查 M1：快照迭代——反应器内注册新反应器不 panic（borrow 已释放），
        // 且新反应器本轮不触发。
        let runtime = Rc::new(Runtime::new());
        let ctx = runtime.context();
        let calls = Rc::new(Cell::new(0));
        let calls2 = Rc::clone(&calls);
        runtime.on_notify(move |ctx, _keys| {
            calls2.set(calls2.get() + 1);
            // 反应器内注册新反应器：快照迭代下安全。
            ctx.runtime().on_notify(|_, _| {});
        });
        drop(ctx.set::<KeyA>(String::from("va")).unwrap());
        assert_eq!(calls.get(), 1, "新反应器本轮不触发");
    }

    #[test]
    fn nested_notify_terminates_with_guarded_reactor() {
        // 审查 M1：反应器内同步 set 会递归广播 notify——以守卫标志显式切断，
        // 验证终止与事件序列（PR #5 的 fiber 反应器以 refresh 惯性切断同步递归）。
        let ctx = Context::new();
        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let done = Rc::new(Cell::new(false));
        let events2 = Rc::clone(&events);
        let done2 = Rc::clone(&done);
        let ctx2 = Rc::clone(&ctx);
        ctx.runtime().on_notify(move |_ctx, keys| {
            events2.borrow_mut().push(keys[0].as_str().to_string());
            if !done2.get() {
                done2.set(true);
                // 反应器内同步 set → 嵌套 notify；守卫保证终止。
                drop(ctx2.set::<KeyB>(7).unwrap());
            }
        });
        drop(ctx.set::<KeyA>(String::from("va")).unwrap());
        assert_eq!(*events.borrow(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn satisfies_resolves_through_realms() {
        // γ ⊧ d（Def 24，经 ρ 解析）。
        let parent = Context::new();
        let key = Symbol::intern(KeyA::SYMBOL);
        let spec: KeySet = [key].into_iter().collect();

        assert!(!parent.satisfies(&spec));
        drop(parent.set::<KeyA>(String::from("va")).unwrap());
        assert!(parent.satisfies(&spec));

        // 隔离后父上下文的绑定不在子上下文（不同 realm）的满足中。
        let child = parent.isolate(key, Symbol::intern("other"));
        assert!(!child.satisfies(&spec), "子上下文解析到其他 realm");
    }

    #[test]
    fn isolation_binds_same_key_independently() {
        // Def 28/29：同一键在不同 realm 下独立绑定（多租户/沙箱隔离）。
        let parent = Context::new();
        let key = Symbol::intern(KeyA::SYMBOL);
        let realm_a = Symbol::intern("realm-a");
        let realm_b = Symbol::intern("realm-b");
        let a = parent.isolate(key, realm_a);
        let b = parent.isolate(key, realm_b);

        let da = a.set::<KeyA>(String::from("va")).unwrap();
        drop(b.set::<KeyA>(String::from("vb")).unwrap());

        assert_eq!(&*a.get::<KeyA>().unwrap(), "va");
        assert_eq!(&*b.get::<KeyA>().unwrap(), "vb");
        assert!(
            matches!(parent.get::<KeyA>(), Err(StoreError::NotBound(_))),
            "未隔离的父上下文解析到自身 realm，不受影响"
        );

        // 撤销 a 的绑定不影响 b。
        da();
        assert!(matches!(a.get::<KeyA>(), Err(StoreError::NotBound(_))));
        assert_eq!(&*b.get::<KeyA>().unwrap(), "vb");
    }

    /// 测试拦截元数据：路径集合取并 + 只读标志右偏覆写。
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    struct PathMeta {
        paths: std::collections::BTreeSet<String>,
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

    /// 与 `PathMeta` 类型不同的拦截元数据（类型冲突场景）。
    #[derive(Clone)]
    struct OtherMeta;

    impl InterceptMeta for OtherMeta {
        fn merge(existing: &Self, new: &Self) -> Self {
            let _ = (existing, new);
            OtherMeta
        }

        fn clone_box(&self) -> Box<dyn InterceptMeta> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn intercept_merges_right_biased_and_derives() {
        // Def 31：右偏合并（new 优先）；派生实现（Def 27）不影响原上下文。
        let ctx = Context::new();
        let key = Symbol::intern("fs");

        let c1 = ctx.intercept(
            key,
            PathMeta {
                paths: ["/a".into()].into_iter().collect(),
                read_only: false,
            },
        );
        let c2 = c1.intercept(
            key,
            PathMeta {
                paths: ["/b".into()].into_iter().collect(),
                read_only: true,
            },
        );

        // 右偏：路径取并、read_only 取 new 的值。
        assert_eq!(
            c2.intercept_of::<PathMeta>(key),
            Some(PathMeta {
                paths: ["/a".into(), "/b".into()].into_iter().collect(),
                read_only: true,
            })
        );
        // 原上下文不受影响（无元数据）；子上下文继承。
        assert_eq!(ctx.intercept_of::<PathMeta>(key), None);
        assert_eq!(
            c1.intercept_of::<PathMeta>(key),
            Some(PathMeta {
                paths: ["/a".into()].into_iter().collect(),
                read_only: false,
            })
        );
    }

    #[test]
    #[should_panic(expected = "拦截元数据类型冲突")]
    fn intercept_type_conflict_panics() {
        let ctx = Context::new();
        let key = Symbol::intern("fs");
        let c1 = ctx.intercept(key, PathMeta::default());
        c1.intercept::<OtherMeta>(key, OtherMeta);
    }
}
