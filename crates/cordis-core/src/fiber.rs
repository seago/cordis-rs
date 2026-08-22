//! Fiber（论文 Def 44/49）与生命周期状态。

use std::any::Any;
use std::cell::{Cell, Ref, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use crate::component::Component;
use crate::context::Context;
use crate::effect::{Disposer, EffectIter};
use crate::keyset::KeySet;
use crate::symbol::Symbol;

/// fiber 名（Def 44 的 `n: 𝔑`）：原子，只比较相等、从不检查结构
/// （Def 45 的名称纪律）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FiberId(u64);

impl FiberId {
    /// 分配一个全新的名字（绝不复用）。
    pub(crate) fn fresh(next: &mut u64) -> FiberId {
        let id = FiberId(*next);
        *next += 1;
        id
    }
}

/// 目标视图 `ω: d → 𝔑`（Def 46）：每个声明键 → 其提供者。
pub type View = BTreeMap<Symbol, FiberId>;

/// fiber 生命周期错误（Def 49 的 `ζ`）。
///
/// 生产者（M2-PR1 落地，L-Raise）：wasm 桥接层（guest trap / 越界 set /
/// 绑定冲突）与显式 raise 的组件迭代器经 [`FiberError::raise`] 抛出，
/// `reload` 捕获后记录为 fiber 的失败 outcome（§4.3.4 𝔈fail）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiberError(String);

impl FiberError {
    /// 构造错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// **L-Raise（§4.3.4，M2-PR1 落地）**：以本错误为 panic 载荷抛出——
    /// `reload` 的 `catch_unwind` 将其识别为"组件失败"（可恢复：记录
    /// outcome + 恢复已完成步骤），而非宿主 bug（后者 resume_unwind）。
    ///
    /// **依赖 `panic = unwind`（审查 nit5）**：`panic = abort` profile 下
    /// `catch_unwind` 永不捕获、raise 直接终止进程（本项目未设 abort）。
    ///
    /// 生产者：wasm 桥接层（guest trap / 越界 set / 绑定冲突）与显式
    /// raise 的组件迭代器。
    pub fn raise(self) -> ! {
        std::panic::panic_any(self)
    }
}

impl fmt::Display for FiberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FiberError {}

/// 生命周期状态 `ΘΓ`（Def 49；同步核心适配，见 THEORY-MAP 已知偏差）。
///
/// - `Reloading`/`Unloading` 携带 `ω`（转换承诺的视图）；`i`（剩余迭代器）
///   在同步核心中活在转换调用栈上，`g`（累加器）由 `ctx` 累加器承载
///   （Table 2：`fiber.dispose` = 累加器）；
/// - `ζ`（错误结果）：`Inactive(Some(ζ))` 为 L-Raise 失败终态（M2-PR1
///   落地）；同步核心中 `Unloading.outcome` 恒 `None`（ζ 直落 Inactive，
///   卸载结果中间形态随 async 化贯通，审查 nit3）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FiberState {
    /// `Inactive(ζ)`：未安装。
    Inactive(Option<FiberError>),
    /// `Reloading(i, g, ω)`：激活转换进行中。
    Reloading {
        /// 转换承诺的视图 `ω`。
        view: View,
    },
    /// `Active(g, ω)`：已安装，提供其绑定。
    Active {
        /// 激活时承诺的视图 `ω`。
        view: View,
    },
    /// `Unloading(g, ω, ζ)`：停用转换进行中（L-Leave 后停止提供、
    /// 逆执行前）。
    Unloading {
        /// 停用前承诺的视图 `ω`。
        view: View,
        /// 结果 `ζ`。
        outcome: Option<FiberError>,
    },
}

/// fiber（Def 44 的 `⟨d, p, e, π, σ, τ, θ⟩` 的生产实现）。
///
/// - `σ`（本 fiber 安装的表）：由 `ctx` 累加器与 store 中携带
///   `provider` 的绑定隐含表示（Def 45）；
/// - `e`（效应函数）：`apply` 闭包（Algorithm 4 第 9 行的 config 绑定）；
/// - `π`：`parent`；`τ`：`retired`；`θ`：`state`。
pub struct Fiber {
    /// `n`：fiber 名。
    pub(crate) id: FiberId,
    /// `π`：父 fiber（None = root）。
    pub(crate) parent: Option<FiberId>,
    /// `d`：共效应规格。
    pub(crate) inject: KeySet,
    /// `p`：供给。
    pub(crate) provide: KeySet,
    /// fiber 自己的上下文（Algorithm 4 第 8 行；效应经其注册，累加器即
    /// `fiber.dispose`，Table 2）。
    pub(crate) ctx: Rc<Context>,
    /// `e(config)`：config 绑定的效应函数（Algorithm 4 第 9 行）。
    /// 可换（`RefCell`）：§5.2.1 双向绑定的组件侧更新（[`Fiber::update`]）
    /// 就地替换本闭包后重跑，fiber 身份保留。
    pub(crate) apply: RefCell<Box<dyn Fn() -> Box<dyn EffectIter>>>,
    /// 组件实例（读取 `d(k)` 声明元数据，Def 30 的 `𝔇inter`；M2-PR2）。
    pub(crate) component: Rc<dyn Component>,
    /// `τ`：退役标志。
    pub(crate) retired: Cell<bool>,
    /// `θ`：生命周期状态。
    pub(crate) state: RefCell<FiberState>,
    /// `target_n(γ)` 的摘要（Def 46；Algorithm 5 的 `fiber.target`）。
    pub(crate) target: RefCell<Option<View>>,
    /// `ω`：承诺视图（Def 44/46；Algorithm 5 的 `fiber.committed`）。
    pub(crate) committed: RefCell<Option<View>>,
    /// 各次激活的恢复组合（Algorithm 5 第 16 行的 `fiber.dispose ←
    /// recover ∘ fiber.dispose`；LIFO 跨激活）。
    pub(crate) dispose: RefCell<Vec<Disposer>>,
    /// 挂起于 [`Step::Await`] 的可恢复上下文（B 计划 A1）：未完成迭代器 +
    /// 已累积逆 + 自报就绪判据（backlog ②）；`None` = 未挂起。恢复经
    /// [`Runtime::advance`]（外部）或 [`Runtime::poll_ready`]（判据评估）；
    /// 退役/卸载时残留逆补入 `dispose`（LIFO 保持）。
    pub(crate) resumable: RefCell<Option<Resumable>>,
}

/// 挂起可恢复上下文（B 计划 A1）：未完成迭代器 + 已累积逆（执行序；
/// 恢复以同 acc 继续，折叠后 LIFO）+ 就绪判据（backlog ② 判据 v2：
/// `Some(judge)` 由 [`Runtime::poll_ready`] 统一评估；`None` = 外部
/// 判据驱动。判据随本上下文存亡——advance 完成/unload 收账即释放）。
pub(crate) type Resumable = (
    Box<dyn EffectIter>,
    Vec<Disposer>,
    Option<Box<dyn Fn() -> bool>>,
);

impl Fiber {
    /// fiber 名。
    pub fn id(&self) -> FiberId {
        self.id
    }

    /// 父 fiber（None = root）。
    pub fn parent(&self) -> Option<FiberId> {
        self.parent
    }

    /// 是否挂起于 `Step::Await`（B 计划 A1）：`true` = 有可恢复上下文，
    /// 待外部就绪后 `Runtime::advance` 恢复。
    ///
    /// 生产消费：`Runtime::suspended_fibers` 派生自本访问器（`resumable`
    /// 为挂起语义单一事实来源，backlog ①）。
    pub fn is_suspended(&self) -> bool {
        self.resumable.borrow().is_some()
    }

    /// 共效应规格 `d`。
    pub fn inject(&self) -> &KeySet {
        &self.inject
    }

    /// 供给 `p`。
    pub fn provide(&self) -> &KeySet {
        &self.provide
    }

    /// fiber 自己的上下文（Algorithm 4 第 8 行的 `fiber.ctx`）：组件效应
    /// 经它注册，也用于在父 fiber 下实例化子组件（Def 47）。
    pub fn ctx(&self) -> &Rc<Context> {
        &self.ctx
    }

    /// 生命周期状态 `θ`。
    ///
    /// **借用警告（审查 m4）**：持有返回的 `Ref` 期间调用 [`Fiber::retire`]
    /// （内部 `refresh` → `borrow_mut(state)`）会 `RefCell` panic——读取借用
    /// 须在触发生命周期操作前释放。
    pub fn state(&self) -> Ref<'_, FiberState> {
        self.state.borrow()
    }

    /// 是否已退役（`τ`）。
    pub fn retired(&self) -> bool {
        self.retired.get()
    }

    /// 记录的目标摘要 `target_n(γ)`（Def 46 式 (41)）的只读视图
    ///（borrow 克隆；评审动作 3 / cordis-async 草案 O-1 落地形态）。
    ///
    /// **只读**：与 [`crate::runtime::Runtime::refresh`] 的重算路径解耦，调用方不得据此
    /// 直接驱动生命周期（目标仍由 `refresh` 权威重算）。与 reload 的
    /// `guard_target`（`fiber.target.borrow() == target0`）同款语义。
    pub fn target_view(&self) -> Option<View> {
        self.target.borrow().clone()
    }

    /// 组件实例（读取 `d(k)` 声明元数据；M2-PR2）。
    pub fn component(&self) -> &Rc<dyn Component> {
        &self.component
    }

    /// 退役（O-Retire，Def 47 的注册逆）：置 `τ` 并刷新目标（→ ⊥ → 卸载链）。
    ///
    /// 幂等：已退役时 refresh 无操作。调用方需持有 `Rc<Fiber>`。
    pub fn retire(self: &Rc<Self>) {
        self.retired.set(true);
        // 退役观察者（§5.2.1 双向绑定条目侧；TS `internal/plugin` 半段）：
        // 组件自退役 → loader 写回条目 `disabled`。任何 retire 均触发
        //（含 loader 驱动 teardown）——过滤在观察者内部（见
        // [`Runtime::set_retire_hook`]）。
        if let Some(hook) = &*self.ctx.runtime().retire_hook.borrow() {
            hook(self);
        }
        self.ctx.runtime().refresh(self);
    }

    /// 就地更新配置（§5.2.1 "the binding runs in both directions" 的组件侧；
    /// TS `Fiber.update` 参照，fiber.ts:476）。实现 = Algorithm 5 的
    /// reload/unload **强制实例**（换 config 闭包 → 逆转当前效应 →
    /// 链式重载；fiber 身份保留）。
    ///
    /// 语义：换 config 闭包 → 逆转当前全部效应（依赖者级联停用，Thm 63 序）
    /// → 以新配置重跑（fiber **身份保留**，非重建）→ 绑定重装（依赖者级联
    /// 恢复）。失败（L-Raise）→ `Inactive(ζ)`，与 `reload` 同路径。
    ///
    /// **Active** 或**失败态**（`Inactive(Some(ζ))`）可调用——与 TS
    /// `assertActive`（`uid !== null`）+ `_error = undefined` 的"失败 fiber
    /// 可经 update 复活"行为同型（REVIEW-97bb598 major-1 采纳）；退役/
    /// 未注册（`Inactive(None)`）调用 = 协议违反，panic = bug。写回观察者
    /// （[`crate::runtime::Runtime::set_update_hook`]）在重跑前以新 config 触发——loader
    /// 经此实现条目侧写回。
    pub fn update(self: &Rc<Self>, config: Rc<dyn Any>) {
        assert!(
            matches!(
                &*self.state.borrow(),
                FiberState::Active { .. } | FiberState::Inactive(Some(_))
            ),
            "Fiber::update 仅 Active/失败态（Inactive(ζ)）可用（§5.2.1 双向绑定；INACTIVE_EFFECT）"
        );
        self.ctx.runtime().update_fiber(self, config);
    }
}
