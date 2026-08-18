//! cordis-async 层（草案 v1.4，Phase 0）。
//!
//! 定位：sync `cordis-core` 零语义改动之上的一等 async 层——异步效应
//! 协议（AsyncEffectIter）、取消/错误通道、可 await 卸载编排（两阶段
//! 卸载 + settle + 代次）、Remote 桥。驱动引擎（`drive`/I-1/I-2）于
//! M0.2 实现；生命周期核心（AsyncRegistrar/AsyncFiberEntry/TailQueue/
//! settle，I-3 + drain 重入）于 M0.3 实现。
//!
//! 依据：`docs/cordis-async-protocol-draft.md` v1.4（冻结）；
//! 执行计划 `docs/cordis-async-PHASE0-PLAN.md`（含里程碑间独立审查硬门禁）。

#![deny(missing_docs)]

use std::future::Future;
use std::pin::Pin;

/// 组合线程本地 future（非 Send；仅在本线程 LocalSet 内 await）。
pub type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

/// 异步逆：撤销一步 async 效应（可 await；对应 core
/// [`cordis_core::effect::Disposer`] 的
/// `FnOnce()` 形态）。
pub type AsyncDisposer = Box<dyn FnOnce() -> LocalBoxFuture<()> + 'static>;

/// 异步效应迭代器：宿主驱动，每步 await 后产出逆或失败。
///
/// 与 core 同款纪律：迭代必须**有限步终止**（订阅型长驻行为用注册器
/// 模式，见草案 §6）。
pub trait AsyncEffectIter: 'static {
    /// 产下一步（可 await）；调用方保证步界 guard 检查。
    fn next(&mut self) -> LocalBoxFuture<AsyncStep>;
}

/// 单步结果（core [`cordis_core::effect::Step`] 的 async 等价物）。
pub enum AsyncStep {
    /// 产出逆并继续（core `Step::Yielded` 的 async 版）。
    Yielded(AsyncDisposer),
    /// 产出逆并终止（core `Step::Finished` 的 async 版）。
    Finished(AsyncDisposer),
    /// 组件运行时失败（core L-Raise 的 async 等价物；**不是** panic 通道）。
    Failed(AsyncFiberError),
}

/// 组件失败载荷（对应 core [`cordis_core::fiber::FiberError`]；async 世界以值传播，不经 panic）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncFiberError(String);

impl AsyncFiberError {
    /// 构造失败载荷。
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// 失败消息。
    pub fn message(&self) -> &str {
        &self.0
    }
}

/// 驱动引擎（草案 §1；Algorithm 1 的 async 转写）。
///
/// 逐步 await 迭代器：guard 在每个**步界**检查（§4.3.2 步界中断同语义）；
/// `Failed` → 先 LIFO await 恢复已完成步骤再报失败；正常终止 → 折叠
/// 复合逆（以应用逆序 await 各步逆，I-1）。
///
/// guard 为 `false` 时的退场：在途步完成、其逆照常入账并参与复合逆
///（I-2）——不中断在途的 `next()` 挂起。
pub async fn drive(
    mut iter: Box<dyn AsyncEffectIter>,
    guard: impl Fn() -> bool,
) -> Result<AsyncDisposer, AsyncFiberError> {
    let mut acc: Vec<AsyncDisposer> = Vec::new();
    loop {
        if !guard() {
            break;
        }
        match iter.next().await {
            AsyncStep::Yielded(d) => acc.push(d),
            AsyncStep::Finished(d) => {
                acc.push(d);
                break;
            }
            AsyncStep::Failed(e) => {
                // LIFO 恢复已完成步骤，再上报失败。
                for d in acc.into_iter().rev() {
                    d().await;
                }
                return Err(e);
            }
        }
    }
    Ok(Box::new(move || {
        Box::pin(async move {
            for d in acc.into_iter().rev() {
                d().await;
            }
        }) as LocalBoxFuture<()>
    }) as AsyncDisposer)
}

use cordis_core::component::Component;
use cordis_core::context::Context;
use cordis_core::effect::once;
use cordis_core::key::Key;
use cordis_core::{
    Disposer, EffectIter, Fiber, FiberId, KeySet, RegistryError, Runtime, StoreError, View,
};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use tokio::task::JoinHandle;

/// 取消标志（草案 §2 取消通道的同步实现；单线程组合线程下 `Cell` 足够）：
/// 注册器逆置位（O(1)），drive 步界 guard 检查。不跨线程。
#[derive(Clone, Default)]
pub struct CancelFlag(Rc<Cell<bool>>);

impl CancelFlag {
    /// 置位取消（幂等）。
    pub fn cancel(&self) {
        self.0.set(true);
    }

    /// 是否已取消。
    pub fn cancelled(&self) -> bool {
        self.0.get()
    }

    /// 重置（M0.4 复活路径：新代重新激活）。
    #[allow(dead_code)] // M0.4 复活路径使用
    pub(crate) fn reset(&self) {
        self.0.set(false);
    }
}

/// async 段可用的上下文（草案 §2）。
///
/// 持有组合线程 `Rc<Context>`（非 Send，仅本线程 LocalSet 内使用）；
/// 约束：不跨 await 持有 store 借用（与 core m-A 借用纪律同款）。
#[derive(Clone)]
pub struct AsyncCx {
    pub(crate) ctx: Rc<Context>,
    pub(crate) fiber: FiberId,
    pub(crate) cancel: CancelFlag,
    pub(crate) generation: u64,
}

impl AsyncCx {
    /// 读取依赖（草案 §2 `get_cloned`）：Running 期读**活 store**——此时
    /// fiber Active ⟹ target == committed（裸 store 与提交视图等价），立即
    /// 克隆释放借用，返回值可跨 await。要求 `K::Value: Clone`（Arc 惯例）。
    ///
    /// **teardown 窗口**不走本方法（提供者绑定可能已撤销）——由「步创建
    /// 时捕获 Arc 克隆」承担（约定 C-1'）。
    pub fn get_cloned<K: Key>(&self) -> Option<K::Value>
    where
        K::Value: Clone,
    {
        self.ctx.get::<K>().ok().map(|r| r.clone())
    }

    /// 同步可逆绑定（core [`Context::set`] 语义透传）：绑定为 sync 元数据，
    /// 逆在 core 卸载时同步执行；**async 资源不放绑定里**（约定 C-2）。
    pub fn set<K: Key>(&self, value: K::Value) -> Result<Disposer, StoreError> {
        self.ctx.set::<K>(value)
    }

    /// 取消通道（卸载/目标变更时触发——注册器逆 cancel）。
    pub fn cancellation(&self) -> &CancelFlag {
        &self.cancel
    }

    /// 本 fiber 名。
    pub fn fiber(&self) -> FiberId {
        self.fiber
    }

    /// 当前激活代次（M0.3-b 起使用）。
    #[allow(dead_code)]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}

/// 组件侧注册点（草案 §5）：sync d/p 由挂载方给定，async 效应在此声明。
pub trait AsyncBehavior: 'static {
    /// 以 async 效应迭代器表达的行为段（有限步；长驻走注册器模式 §6）。
    fn apply_async(&self, cx: AsyncCx, config: &dyn Any) -> Box<dyn AsyncEffectIter>;
}

/// async 视图的 fiber 状态（草案 §5）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AsyncFiberState {
    /// sync 未激活或依赖未满足。
    Idle,
    /// 已激活（drive 完成或仍在途；disposer 待卸载时经 slot 收账）。
    Running {
        /// 激活代次（防串代；M0.3-b 起使用）。
        generation: u64,
    },
    /// 失败静止终态（I-4 视为静止）。
    Failed(AsyncFiberError),
}

// ── M0.3：两阶段卸载（草案 §3）——I-3 + drain 重入 ────────────────────

/// 收尾队列：两阶段卸载的 async 阶段（阶段 2）FIFO（草案 §3.2）。
///
/// 入队序 = core sync 级联的卸载序（依赖者先撤，Thm 63）——settle 按
/// FIFO 排空即得 **I-3**（依赖者的 async 逆先 settle、提供者后 settle），
/// 顺序免费来自 sync 级联，本层只保证 FIFO。
#[derive(Default)]
pub struct TailQueue {
    inner: RefCell<VecDeque<Tail>>,
}

/// 一条尾巴：一次激活的 drive 任务收尾记账（评审点 H：disposer 只进
/// 共享槽，收账唯一通道 = settle 的 `take`，与 drive 完成时序无关）。
struct Tail {
    /// 记账代次（草案 §3.4：防串代元数据；主序由 FIFO 承担——每次卸载的
    /// settle 收该次卸载的账，M0.4 更新路径核对用）。
    #[allow(dead_code)]
    generation: u64,
    handle: JoinHandle<()>,
    slot: Rc<RefCell<Option<AsyncDisposer>>>,
}

/// drain 自再生守卫（§3.4）：settle 单轮排空（一代）后仍出现新尾巴，
/// 最多 `MAX_DRAIN_ROUNDS` 轮；超限 = 收尾逆持续注册新效应且不收敛
/// = 宿主 bug，panic 并诊断（死锁守卫）。
const MAX_DRAIN_ROUNDS: u32 = 64;

impl TailQueue {
    /// O(1) 入队（注册器逆 C-6 唯一允许的第二动作；不 await、不 panic）。
    pub(crate) fn enqueue_tail(
        &self,
        generation: u64,
        handle: JoinHandle<()>,
        slot: Rc<RefCell<Option<AsyncDisposer>>>,
    ) {
        self.inner.borrow_mut().push_back(Tail {
            generation,
            handle,
            slot,
        });
    }

    /// FIFO 排空（阶段 2）：逐个 await drive 任务收尾 → take 共享槽 →
    /// await 异步逆。逆可能注册**新的** async 效应（合法收尾逻辑）→ 入队
    /// → 下一轮排空；超过 `MAX_DRAIN_ROUNDS` 轮 = 尾巴自再生死循环，
    /// panic（死锁守卫）。
    ///
    /// drive 任务 panic（宿主 bug）经 JoinHandle 捕获、记录诊断，不进入
    /// 组件失败通道（§3.3）。
    pub async fn settle(&self) {
        let mut rounds: u32 = 0;
        loop {
            let batch: Vec<Tail> = self.inner.borrow_mut().drain(..).collect();
            if batch.is_empty() {
                return;
            }
            rounds += 1;
            assert!(
                rounds <= MAX_DRAIN_ROUNDS,
                "settle: drain 自再生死循环守卫（超过 {MAX_DRAIN_ROUNDS} 轮）——收尾逆持续注册新效应且不收敛，宿主 bug"
            );
            for tail in batch {
                if let Err(e) = tail.handle.await {
                    eprintln!("cordis-async: drive 任务 panic（宿主 bug）：{e}");
                }
                // 先取出再 await：RefMut 不跨 await（clippy
                // await-holding-refcell-ref；且逆可能重入 enqueue）。
                let disposer = tail.slot.borrow_mut().take();
                if let Some(d) = disposer {
                    d().await;
                }
            }
        }
    }
}

/// AsyncRuntime 注册表条目（评审点 B：**无回边到 AsyncRuntime**——否则
/// 形成 `AsyncRuntime → core → fiber → 注册器逆闭包 → 条目 → AsyncRuntime`
/// 引用环，关停泄漏。条目只持有尾部队列 Rc 与自身状态，环不存在）。
pub struct AsyncFiberEntry {
    state: RefCell<AsyncFiberState>,
    generation: Cell<u64>,
    queue: Rc<TailQueue>,
}

impl AsyncFiberEntry {
    fn new(queue: Rc<TailQueue>) -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(AsyncFiberState::Idle),
            generation: Cell::new(0),
            queue,
        })
    }

    /// 激活：分配新代次并复位状态（幂等；重复调用仅再换代——复活路径
    /// 复用，M0.4）。
    fn begin_activation(&self) -> u64 {
        let g = self.generation.get() + 1;
        self.generation.set(g);
        *self.state.borrow_mut() = AsyncFiberState::Idle;
        g
    }

    /// 尾巴记账（O(1)；仅注册器逆调用，契约 C-6）。
    fn enqueue_tail(
        &self,
        generation: u64,
        handle: JoinHandle<()>,
        slot: Rc<RefCell<Option<AsyncDisposer>>>,
    ) {
        self.queue.enqueue_tail(generation, handle, slot);
    }

    /// 状态标记（评审点 H 决议：退化为纯状态标记；代次不匹配 = 旧代尾巴
    /// 迟到，跳过——不影响记账正确性，记账只有 settle 的 take 一条通道）。
    fn mark_running(&self, generation: u64) {
        if self.generation.get() == generation {
            *self.state.borrow_mut() = AsyncFiberState::Running { generation };
        }
    }

    /// 失败静止（I-4 前件；M0.4 扩展：自退役 + disabled 写回）。
    fn on_failed(&self, generation: u64, error: AsyncFiberError) {
        if self.generation.get() == generation {
            *self.state.borrow_mut() = AsyncFiberState::Failed(error);
        }
    }
}

/// sync 包装组件（草案 §3.1）：把 async 行为挂进 core 生命周期。
///
/// `d/p` 取自被包装的 sync 组件（依赖解析、级联、退役全部走 core）；
/// `apply` 的唯一步：spawn drive 任务 + 产出注册器逆（C-6）。
struct AsyncRegistrar {
    inner: Rc<dyn Component>,
    behavior: Box<dyn AsyncBehavior>,
    entry: Rc<AsyncFiberEntry>,
}

impl AsyncRegistrar {
    fn new(
        inner: Rc<dyn Component>,
        behavior: Box<dyn AsyncBehavior>,
        entry: Rc<AsyncFiberEntry>,
    ) -> Self {
        Self {
            inner,
            behavior,
            entry,
        }
    }
}

impl Component for AsyncRegistrar {
    fn inject(&self) -> KeySet {
        self.inner.inject()
    }

    fn provide(&self) -> KeySet {
        self.inner.provide()
    }

    fn apply(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let generation = self.entry.begin_activation();
        let fiber_id = ctx
            .fiber()
            .expect("AsyncRegistrar::apply 仅在 fiber 上下文执行");
        let cancel = CancelFlag::default();
        let cx = AsyncCx {
            ctx: Rc::clone(&ctx),
            fiber: fiber_id,
            cancel: cancel.clone(),
            generation,
        };
        let iter = self.behavior.apply_async(cx.clone(), config);
        let entry = Rc::clone(&self.entry);
        // guard 构成（评审点 A/E 双保险）：① 精确 target 比较（激活时视图）
        // + ② 取消标志。闭环：target 变化 → core refresh → 卸载 → 注册器逆
        // → cancel → drive 最近步界退场 → 尾巴入 settle。
        let view: Option<View> = ctx.runtime().fiber(fiber_id).and_then(|f| f.target_view());
        let guard = {
            let cancel = cancel.clone();
            let runtime = Rc::clone(ctx.runtime());
            let view = view.clone();
            move || {
                if cancel.cancelled() {
                    return false;
                }
                match view {
                    Some(ref v) => runtime
                        .fiber(fiber_id)
                        .is_some_and(|f| f.target_view().as_ref() == Some(v)),
                    None => false,
                }
            }
        };
        Box::new(once(Box::new(move || {
            // 步执行（apply 时，组合线程 LocalSet 上下文内）：spawn drive。
            let slot = Rc::new(RefCell::new(None::<AsyncDisposer>));
            let handle = tokio::task::spawn_local({
                let slot = Rc::clone(&slot);
                let entry = Rc::clone(&entry);
                async move {
                    match drive(iter, guard).await {
                        Ok(disposer) => {
                            // 评审点 H：无论 fiber 此刻 Active 还是已进入卸载，
                            // disposer 一律写共享槽——由 settle 统一取走、恰一次。
                            *slot.borrow_mut() = Some(disposer);
                            entry.mark_running(generation);
                        }
                        Err(e) => entry.on_failed(generation, e),
                    }
                }
            });
            // 本步逆（契约 C-6）：只做两件快事——cancel + enqueue_tail，
            // O(1)、不 await、不 panic、不再借其他 RefCell（core 卸载路径
            // 要求逆绝对干净，panic = 宿主 bug）。
            let entry = Rc::clone(&entry);
            let slot = Rc::clone(&slot);
            let cancel = cancel.clone();
            Box::new(move || {
                cancel.cancel();
                entry.enqueue_tail(generation, handle, slot);
            }) as Disposer
        })))
    }
}

/// async 视图的运行时门面（草案 §5 的 M0.3 部分：挂载 + settle）。
///
/// 持有 core [`Runtime`]（生命周期对齐）、全局收尾队列与注册表条目。
/// M0.4 失败通道（retire/disabled/复活）、M0.5 门面完备
/// （update/shutdown/is_quiet/代次）在此之上扩展。
pub struct AsyncRuntime {
    #[allow(dead_code)] // M0.5 shutdown 双真断言使用
    core: Rc<Runtime>,
    tails: Rc<TailQueue>,
    #[allow(dead_code)] // M0.4 失败通道按 fiber 查条目使用
    entries: RefCell<HashMap<FiberId, Rc<AsyncFiberEntry>>>,
}

impl AsyncRuntime {
    /// 新建门面：与 `ctx` 所属 core runtime 对齐（契约 C-3：进程内唯一
    /// 组合线程，所有生命周期操作在其 LocalSet 上下文内进行）。
    ///
    /// **签名偏离（REVIEW-83c254a nit-2）**：草案 §5 的 `new() -> Self`
    /// 无渠道获取进程单例 [`Runtime`]——本实现取 `&Rc<Context>` 推导并
    /// 核对 LocalSet 对齐，更安全；`use_component` 返回 `Rc<Fiber>` 是
    /// M0.5 引入 `AsyncFiberHandle` 前的合理临时形态。
    pub fn new(ctx: &Rc<Context>) -> Rc<Self> {
        Rc::new(Self {
            core: Rc::clone(ctx.runtime()),
            tails: Rc::new(TailQueue::default()),
            entries: RefCell::new(HashMap::new()),
        })
    }

    /// 挂载 async 组件（草案 §3.1）：sync 包装（`d/p` 取自 `comp`）+
    /// 注册表条目 + core 注册。依赖满足即激活并 spawn drive 任务；卸载时
    /// 注册器逆 cancel + 入队，由 [`Self::settle`] 收账。
    pub fn use_component(
        &self,
        ctx: &Rc<Context>,
        comp: Rc<dyn Component>,
        behavior: impl AsyncBehavior,
        config: Rc<dyn Any>,
    ) -> Result<Rc<Fiber>, RegistryError> {
        let entry = AsyncFiberEntry::new(Rc::clone(&self.tails));
        let registrar = Rc::new(AsyncRegistrar::new(
            comp,
            Box::new(behavior),
            Rc::clone(&entry),
        ));
        let fiber = ctx.use_component(registrar, config)?;
        self.entries.borrow_mut().insert(fiber.id(), entry);
        Ok(fiber)
    }

    /// 两阶段卸载阶段 2（草案 §3.2）：FIFO 排空收尾队列（I-3 序免费来自
    /// core sync 级联的入队序）。await 本方法直至全部尾巴 settle。
    pub async fn settle(&self) {
        self.tails.settle().await;
    }
}
