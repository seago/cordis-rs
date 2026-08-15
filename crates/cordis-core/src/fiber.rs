//! Fiber（论文 Def 44/49）与生命周期状态。

use std::cell::{Cell, Ref, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

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
/// 当前同步核心尚无生产者（效应失败路径随 L-Raise 接入 async 阶段）；
/// 保留状态形状以对齐 Def 49。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiberError(String);

impl FiberError {
    /// 构造错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
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
/// - `ζ`（错误结果）：当前恒 `None`（L-Raise 随 async 接入）。
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
    pub(crate) apply: Box<dyn Fn() -> Box<dyn EffectIter>>,
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
}

impl Fiber {
    /// fiber 名。
    pub fn id(&self) -> FiberId {
        self.id
    }

    /// 父 fiber（None = root）。
    pub fn parent(&self) -> Option<FiberId> {
        self.parent
    }

    /// 共效应规格 `d`。
    pub fn inject(&self) -> &KeySet {
        &self.inject
    }

    /// 供给 `p`。
    pub fn provide(&self) -> &KeySet {
        &self.provide
    }

    /// 生命周期状态 `θ`。
    pub fn state(&self) -> Ref<'_, FiberState> {
        self.state.borrow()
    }

    /// 是否已退役（`τ`）。
    pub fn retired(&self) -> bool {
        self.retired.get()
    }

    /// 退役（O-Retire，Def 47 的注册逆）：置 `τ` 并刷新目标（→ ⊥ → 卸载链）。
    ///
    /// 幂等：已退役时 refresh 无操作。调用方需持有 `Rc<Fiber>`。
    pub fn retire(self: &Rc<Self>) {
        self.retired.set(true);
        self.ctx.runtime().refresh(self);
    }
}
