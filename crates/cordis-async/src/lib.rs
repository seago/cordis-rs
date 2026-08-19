//! cordis-async 层（草案 v1.4，Phase 0）。
//!
//! 定位：sync `cordis-core` 零语义改动之上的一等 async 层——异步效应
//! 协议（AsyncEffectIter）、取消/错误通道、可 await 卸载编排（两阶段
//! 卸载 + settle + 代次）、Remote 桥。驱动引擎（`drive`/I-1/I-2）于
//! M0.2 实现；生命周期核心（AsyncRegistrar/AsyncFiberEntry/TailQueue/
//! settle，I-3 + drain 重入）于 M0.3 实现；失败通道（I-4 自退役 +
//! disabled 写回 + 复活、C-7 shutdown 双真）于 M0.4 实现；门面完善
//!（retire/update、测试 8/9/10）于 M0.5 实现；Remote 桥（Remote/
//! TokioRemote/spawn_remote）于 M0.6 实现。
//!
//! ## 门面纪律（契约 C-4，P1.2 H2 文档化）
//!
//! 生命周期变更（`retire` / `update` / `use_component`——即草案 C-4 的
//! `apply` 域对应物，语义等价注记 REVIEW-23b75fa nit-1）**必须走门面**
//! （[`AsyncRuntime`]）；绕过门面直接调 core 的 sync API（如
//! `Fiber::retire`）对 sync-only 组件是允许的，但**其 async 尾巴不会被
//! settle 记账**（`AsyncFiberHandle` 解引用出的 fiber 亦应经门面操作）。
//! 此分界是插件作者文档的明示项。
//!
//! ## 开放项决策状态（P1.2，H2 记录）
//!
//! - **O-2（settle 粒度）**：保持**显式 `settle()`**（框架层不提供自动
//!   settle 封装；「每次 retire/update 自动 settle」的模式由 app 层封装，
//!   草案 O-2 决议采纳）；
//! - **O-3（lifecycle observer hook）**：**不启用** core 既有
//!   `update_hook`/`retire_hook` 作门面 hook（草案默认；若 C-4 被频繁
//!   违反，按需启用逃生口——REVIEW-23b75fa nit-2 措辞对齐）；
//! - **O-4（Failed 载荷富化）**：保持 `String`（`AsyncFiberError` 不
//!   变）；结构化错误（错误码/可重试）等首个真实失败场景再定（草案 O-4
//!   决议采纳）。
//!
//! 依据：`docs/cordis-async-protocol-draft.md` v1.4（冻结）；
//! 执行计划 `docs/cordis-async-PHASE0-PLAN.md`（含里程碑间独立审查硬门禁）
//! 与 `docs/cordis-async-PHASE1-P2-PLAN.md`（P1.2 完善线）。

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
    Disposer, EffectIter, Fiber, FiberId, FiberState, KeySet, RegistryError, Runtime, StoreError,
    View,
};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::{Rc, Weak};
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

// ── M0.6：Remote 桥（草案 §2/§4）——spawn_remote ──────────────────────

/// 远端结果（类型擦除；join 侧 `downcast` 取回具体值）。
///
/// `Send`：跨线程从 worker 回灌组合线程（O-6 纪律：跨线程经 join/channel
/// 通信，不触碰组合线程资源）。
///
/// **API 冻结（P1.2 H3）**：本桥 API 面在 P1.3（Send-future 分池形态 /
/// WasmRemote 接入）扩展前冻结。
pub type RemoteValue = Box<dyn Any + Send>;

/// 远端 join（组合线程本地 future：await 即等待远端完成并回灌值）。
///
/// **API 冻结（P1.2 H3）**：`submit` 返回形态在 P1.3 不改变（REVIEW-aa346d2
/// minor-1 对齐计划 H3 标注）。
pub type RemoteJoin<T> = LocalBoxFuture<T>;

/// 远端请求载荷（P1.3 双形态）：Send 闭包（v1，`spawn_blocking` 运行）
/// 或 Send async future（P1.3 分池形态，multi_thread 池 `spawn` 运行）。
///
/// **P1.3 扩展点落地（REVIEW-aa346d2 / P1.2 H3 冻结保持）**：`submit` 签名
/// 不变；扩展在内部变体进行。
pub struct RemoteRequest(RemoteRequestInner);

/// RemoteRequest 内部双变体。
enum RemoteRequestInner {
    /// 同步闭包（worker `spawn_blocking` 池执行）。
    Closure(Box<dyn FnOnce() -> RemoteValue + Send>),
    /// Send async future（worker multi_thread 池 `spawn` 执行）。
    Future(Pin<Box<dyn Future<Output = RemoteValue> + Send>>),
}

impl RemoteRequest {
    /// 构造闭包形态请求（结果自动装箱为 [`RemoteValue`]）。
    pub fn boxed<T: Any + Send>(f: impl FnOnce() -> T + Send + 'static) -> Self {
        Self(RemoteRequestInner::Closure(Box::new(move || {
            Box::new(f()) as RemoteValue
        })))
    }

    /// 构造 Send-future 形态请求（P1.3：异步计算提交 worker 池）。
    ///
    /// 分工（REVIEW-281c6ac nit-2）：future 形态适合**非阻塞异步**（IO /
    /// 拉取式流）；闭包形态（[`Self::boxed`]/[`From`]）适合**阻塞 / CPU 密集**
    /// （经 `spawn_blocking`）。两形态都遵守 O-6：worker 侧不触碰组合线程
    /// 资源。
    pub fn from_future(fut: impl Future<Output = RemoteValue> + Send + 'static) -> Self {
        Self(RemoteRequestInner::Future(Box::pin(fut)))
    }
}

impl<F> From<F> for RemoteRequest
where
    F: FnOnce() -> RemoteValue + Send + 'static,
{
    fn from(f: F) -> Self {
        Self(RemoteRequestInner::Closure(Box::new(f)))
    }
}

/// 远端执行器（草案 §2 的 pending-set 泛化；评审点 G）。
///
/// v1 唯一实现 [`TokioRemote`]（`spawn_blocking` 分池）；**WasmRemote**
/// 为 M1 宿主驱动协议（PR #11–13）的接入点——guest 无自发线程，
/// `submit` = 请求入队、宿主在 step 边界驱动并回填，语义不变。
/// Phase 0 不实现 WasmRemote，接入位置即本 trait。
///
/// **API 冻结（P1.2 H3）**：`submit(RemoteRequest) -> RemoteJoin<RemoteValue>`
/// 签名在 P1.3 扩展（Send-future 分池形态 / WasmRemote 接入）前冻结——
/// 是 P1.3 的稳定接入面；扩展以新增表述变体进行，不破坏既有签名。
pub trait Remote: 'static {
    /// 提交请求，返回可 await 的 join。
    fn submit(&self, req: RemoteRequest) -> RemoteJoin<RemoteValue>;
}

/// TokioRemote v1：把 Send 闭包提交到宿主提供的多线程 worker runtime 的
/// blocking pool（`Handle::spawn_blocking`），join 回灌组合线程。
///
/// **生命周期**：`worker`（[`tokio::runtime::Handle`]）须比本桥存活更久
/// （worker runtime 关闭后 submit 会 panic = 宿主配置错误 = bug）。
/// **O-6 纪律**：worker 侧不得触碰组合线程资源（core/LocalSet）——仅限
/// 纯外部 IO / CPU 密集计算，否则死锁。
///
/// **冻结声明（REVIEW-aa346d2 nit-1）**：本实现（M0.6）经复核，生命周期 /
/// O-6 语义完整；P1.3 扩展以新增形态进行，本类型不破坏性变更。
pub struct TokioRemote {
    worker: tokio::runtime::Handle,
}

impl TokioRemote {
    /// 以宿主 worker runtime 的句柄构造。
    pub fn new(worker: tokio::runtime::Handle) -> Self {
        Self { worker }
    }
}

impl Remote for TokioRemote {
    fn submit(&self, req: RemoteRequest) -> RemoteJoin<RemoteValue> {
        use self::RemoteRequestInner as I;
        // 双形态调度（P1.3 R1）：闭包 → blocking 池；Send-future → multi_thread
        // 池。submit 签名不变（冻结保持），两路 join 回灌组合线程。
        match req.0 {
            I::Closure(f) => {
                let handle = self.worker.spawn_blocking(f);
                Box::pin(await_remote_join(handle)) as LocalBoxFuture<RemoteValue>
            }
            I::Future(fut) => {
                let handle = self.worker.spawn(fut);
                Box::pin(await_remote_join(handle)) as LocalBoxFuture<RemoteValue>
            }
        }
    }
}

/// await 远端 JoinHandle 并解包（O-6：远端 panic = 宿主 bug）。
async fn await_remote_join(handle: tokio::task::JoinHandle<RemoteValue>) -> RemoteValue {
    handle
        .await
        .expect("远端任务 panic = 宿主 bug（O-6：远端不得触碰组合线程资源）")
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
    /// Remote 桥（M0.6；未安装时 `spawn_remote` panic = 宿主配置错误）。
    pub(crate) remote: Option<Rc<dyn Remote>>,
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

    /// 在 fiber 上下文注册 sync 效应（core [`Context::effect`] 透传）：
    /// 其逆进入 **fiber ctx 累加器**——fiber 卸载时自动执行。
    ///
    /// **S1 订阅模式（事件总线 spike）**：订阅经本方法注册 =
    /// `ctx.effect(once(订阅 + 逆=退订))`——随 fiber 卸载**自动退订**，
    /// 无需手工清理（草案 §8：订阅经 `ctx.effect` 注册 = 随 fiber 卸载
    /// 自动退订，TS `ctx.on` 语义）。
    pub fn effect(&self, callback: impl FnOnce() -> Box<dyn EffectIter> + 'static) -> Disposer {
        self.ctx.effect(callback)
    }

    /// 取消通道（卸载/目标变更时触发——注册器逆 cancel）。
    pub fn cancellation(&self) -> &CancelFlag {
        &self.cancel
    }

    /// pending-set 泛化（草案 §2）：把请求交给远端 worker，返回可 await
    /// 的 join（组合线程 await，跨线程回灌；O-6：worker 侧不触碰组合线程
    /// 资源）。v1 唯一实现 [`TokioRemote`]；WasmRemote 为 M1 接入点。
    ///
    /// **前置**：宿主须先 `AsyncRuntime::set_remote`——未安装时 panic
    /// （宿主配置错误 = bug，契约 C-3 同款诊断）。
    pub fn spawn_remote(&self, req: impl Into<RemoteRequest>) -> RemoteJoin<RemoteValue> {
        match &self.remote {
            Some(remote) => remote.submit(req.into()),
            None => panic!(
                "spawn_remote：未安装 Remote（宿主须先 AsyncRuntime::set_remote；配置错误 = bug）"
            ),
        }
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

    /// 无待收尾巴（async 视图静止判定 I-4 用）。
    fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
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

/// 一次激活的记账会话（M0.4 起）：drive 任务、注册器逆与 shutdown 兜底
/// 共享的三件套（取消标志/任务句柄/共享槽）。`handle` 不可克隆——单一
/// 持有者 = 条目；逆与兜底经 [`AsyncFiberEntry::take_session`] 取走
/// （O(1)、幂等、代次核对）。
struct ActiveSession {
    generation: u64,
    cancel: CancelFlag,
    handle: JoinHandle<()>,
    slot: Rc<RefCell<Option<AsyncDisposer>>>,
}

/// 条目登记表（fiber id → 条目弱引用；AsyncRuntime 持有，条目 apply 时
/// 自登记；Weak 值 = 惰性淘汰，无回边）。
type EntryRegistry = RefCell<HashMap<FiberId, Weak<AsyncFiberEntry>>>;

/// AsyncRuntime 注册表条目（评审点 B：**无回边到 AsyncRuntime**——否则
/// 形成 `AsyncRuntime → core → fiber → 注册器逆闭包 → 条目 → AsyncRuntime`
/// 引用环，关停泄漏。条目只持有尾部队列 Rc、fiber 弱引用与自身状态，
/// 环不存在）。
pub struct AsyncFiberEntry {
    state: RefCell<AsyncFiberState>,
    generation: Cell<u64>,
    queue: Rc<TailQueue>,
    /// 激活会话（当前代；卸载/兜底时取走）。
    session: RefCell<Option<ActiveSession>>,
    /// 所属 fiber（Weak 防环；apply 时经 runtime 反查 adopt）。
    fiber: RefCell<Option<Weak<Fiber>>>,
    /// AsyncRuntime 登记表句柄（apply 时自登记；值 Weak——无回边，惰性淘汰）。
    registry: RefCell<Option<Rc<EntryRegistry>>>,
}

impl AsyncFiberEntry {
    fn new(queue: Rc<TailQueue>) -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(AsyncFiberState::Idle),
            generation: Cell::new(0),
            queue,
            session: RefCell::new(None),
            fiber: RefCell::new(None),
            registry: RefCell::new(None),
        })
    }

    /// 绑定 AsyncRuntime 登记表（构造时；值 Weak 无回边）。
    fn attach_registry(&self, registry: Rc<EntryRegistry>) {
        *self.registry.borrow_mut() = Some(registry);
    }

    /// apply 时自登记（fiber id 此刻已知；use_component 与 wrap_component
    /// 统一走此路径）。
    fn self_register(self: &Rc<Self>, id: FiberId) {
        if let Some(registry) = self.registry.borrow().as_ref() {
            registry.borrow_mut().insert(id, Rc::downgrade(self));
        }
    }

    /// 激活：分配新代次并复位状态（幂等；重复调用仅再换代——复活路径
    /// 复用）。
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

    /// 关联所属 fiber（apply 时经 runtime 反查 adopt；Weak 防环）。
    fn adopt_fiber(&self, fiber: &Rc<Fiber>) {
        *self.fiber.borrow_mut() = Some(Rc::downgrade(fiber));
    }

    /// 所属 fiber（upgrade；未 adopt / 已释放时 None）。
    fn fiber_rc(&self) -> Option<Rc<Fiber>> {
        self.fiber.borrow().as_ref().and_then(|w| w.upgrade())
    }

    /// 登记激活会话（apply 步执行时；覆盖旧代——旧代会话已被其逆取走）。
    fn install_session(&self, session: ActiveSession) {
        *self.session.borrow_mut() = Some(session);
    }

    /// 取走激活会话（注册器逆 / shutdown 兜底；代次核对防串代；幂等——
    /// 先取者得，后取者得 None 即跳过）。
    fn take_session(&self, generation: u64) -> Option<ActiveSession> {
        let mut session = self.session.borrow_mut();
        if session.as_ref().is_some_and(|s| s.generation == generation) {
            session.take()
        } else {
            None
        }
    }

    /// 状态标记（评审点 H 决议：退化为纯状态标记；代次不匹配 = 旧代尾巴
    /// 迟到，跳过——不影响记账正确性，记账只有 settle 的 take 一条通道）。
    fn mark_running(&self, generation: u64) {
        if self.generation.get() == generation {
            *self.state.borrow_mut() = AsyncFiberState::Running { generation };
        }
    }

    /// 失败静止 + 自退役（草案 §3.3；I-4）：代次匹配 → `Failed(ζ)` 静止
    /// 终态；随后 `fiber.retire()`——loader retire hook 写回条目
    /// `disabled = true`（G1 通道）、core 级联卸载依赖者（sync 部分），
    /// 尾巴由 settle 收尾（失败路径 slot 留空——无 disposer，评审点 H）。
    /// 复活 = 编排方重启用（loader 重载 / 重建）→ 新代 `begin_activation`
    /// + 新 drive spawn。
    fn on_failed(&self, generation: u64, error: AsyncFiberError) {
        if self.generation.get() != generation {
            return;
        }
        *self.state.borrow_mut() = AsyncFiberState::Failed(error);
        if let Some(fiber) = self.fiber_rc() {
            fiber.retire();
        }
    }

    /// 当前状态（失败通道/复活路径测试与门面查询用）。
    pub fn state(&self) -> AsyncFiberState {
        self.state.borrow().clone()
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
    remote: Option<Rc<dyn Remote>>,
}

impl AsyncRegistrar {
    fn new(
        inner: Rc<dyn Component>,
        behavior: Box<dyn AsyncBehavior>,
        entry: Rc<AsyncFiberEntry>,
        remote: Option<Rc<dyn Remote>>,
    ) -> Self {
        Self {
            inner,
            behavior,
            entry,
            remote,
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
        // 关联 fiber（自退役/复活路径需要；Weak 防环）并自登记（fiber id
        // 此刻已知——use_component / wrap_component 统一）。
        if let Some(fiber) = ctx.runtime().fiber(fiber_id) {
            self.entry.adopt_fiber(&fiber);
        }
        self.entry.self_register(fiber_id);
        let cancel = CancelFlag::default();
        let cx = AsyncCx {
            ctx: Rc::clone(&ctx),
            fiber: fiber_id,
            cancel: cancel.clone(),
            generation,
            remote: self.remote.clone(),
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
            // 记账会话入条目（逆与 shutdown 兜底共享；`handle` 不可克隆，
            // 单一持有者 = 条目——M0.4 起，替代逆直接捕获三件套）。
            entry.install_session(ActiveSession {
                generation,
                cancel: cancel.clone(),
                handle,
                slot: Rc::clone(&slot),
            });
            // 本步逆（契约 C-6 精神）：O(1) 取会话 → cancel + enqueue_tail；
            // 不 await、不 panic、不再借其他 RefCell。take 幂等——shutdown
            // 兜底先取走时本逆跳过（收账仍由 settle 完成）。
            let entry = Rc::clone(&entry);
            Box::new(move || {
                if let Some(s) = entry.take_session(generation) {
                    s.cancel.cancel();
                    entry.enqueue_tail(generation, s.handle, s.slot);
                }
            }) as Disposer
        })))
    }
}

/// 门面句柄（草案 §5，P1.2 H1）：async 组件的弱引用身份 + 创建代次审计。
///
/// 由 [`AsyncRuntime::use_component`] 返回；[`AsyncRuntime::retire`] /
/// [`AsyncRuntime::update`] 经此操作（契约 C-4：生命周期变更走门面）。
///
/// - 内部持 `Weak<Fiber>`——**弱引用**，不延长 fiber 生命周期（防环，评审
///   点 B；句柄失效 = fiber 已释放 = 宿主使用失效句柄 = panic=bug 诊断）；
/// - `generation` 为**审计元数据**（use_component 时捕获的代次；注：换代
///   不使句柄失效——`retire`/`update` 操作 fiber 本体，防串代由条目内部
///   代次机制承担，实现注记 P1.2 H1）。
pub struct AsyncFiberHandle {
    fiber: Weak<Fiber>,
    generation: u64,
}

impl AsyncFiberHandle {
    fn new(fiber: &Rc<Fiber>, generation: u64) -> Self {
        Self {
            fiber: Rc::downgrade(fiber),
            generation,
        }
    }

    /// 解引用 fiber（已释放 = `None`）。返回临时强引（方法结束即释放，
    /// 不延长 fiber 生命周期——弱引封装不变；**警示（REVIEW-fa44fd6
    /// nit-2）**：调用方不得长期持有返回的强引克隆——那会延长 fiber
    /// 生命周期、破坏弱引语义，仅限读状态等瞬时使用）。
    pub fn fiber(&self) -> Option<Rc<Fiber>> {
        self.fiber.upgrade()
    }

    /// 创建代次（审计元数据；换代不失效）。
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// fiber id（存活期 `Some`；已释放 `None`——配合 [`AsyncRuntime::entry`]
    /// 查询条目状态）。
    pub fn id(&self) -> Option<FiberId> {
        self.fiber().map(|f| f.id())
    }
}

/// async 视图的运行时门面（草案 §5 的 M0.3–M0.4 部分：挂载 + settle +
/// 失败通道 + 关停）。
///
/// 持有 core [`Runtime`]（生命周期对齐）、全局收尾队列与注册表条目
/// （`Weak` 值：条目随 fiber/任务释放自然消亡，惰性淘汰——REVIEW-83c254a
/// nit-3 落地；条目在 apply 时经 registry 句柄自登记，use_component 与
/// wrap_component 统一）。M0.5 门面完备（update/代次）在此之上扩展。
pub struct AsyncRuntime {
    core: Rc<Runtime>,
    tails: Rc<TailQueue>,
    entries: Rc<EntryRegistry>,
    remote: RefCell<Option<Rc<dyn Remote>>>,
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
            entries: Rc::new(RefCell::new(HashMap::new())),
            remote: RefCell::new(None),
        })
    }

    /// 安装 Remote 桥（M0.6；v1：[`TokioRemote`]，WasmRemote 为 M1 接入
    /// 点）。须在 [`Self::use_component`] / [`Self::wrap_component`] 之前
    /// 调用（注册器**快照**捕获桥句柄）；覆盖幂等。
    ///
    /// **快照语义（REVIEW-4f1e555 nit-2）**：覆盖只影响随后
    /// `use_component`/`wrap_component` 快照的组合；**已挂载**注册器不
    /// 回溯（保持首次捕获，多为 `None`）——更换桥后需重挂载生效。
    pub fn set_remote(&self, remote: Rc<dyn Remote>) {
        *self.remote.borrow_mut() = Some(remote);
    }

    /// 挂载 async 组件（草案 §3.1）：sync 包装（`d/p` 取自 `comp`）+
    /// 注册表条目 + core 注册。依赖满足即激活并 spawn drive 任务；卸载时
    /// 注册器逆 cancel + 入队，由 [`Self::settle`] 收账。
    ///
    /// 返回 [`AsyncFiberHandle`]（弱引用句柄；P1.2 H1 收口，取代 M0.5 临时
    /// `Rc<Fiber>`）。条目登记由 apply 自登记完成。
    pub fn use_component(
        &self,
        ctx: &Rc<Context>,
        comp: Rc<dyn Component>,
        behavior: impl AsyncBehavior,
        config: Rc<dyn Any>,
    ) -> Result<AsyncFiberHandle, RegistryError> {
        let entry = AsyncFiberEntry::new(Rc::clone(&self.tails));
        entry.attach_registry(Rc::clone(&self.entries));
        let registrar = Rc::new(AsyncRegistrar::new(
            comp,
            Box::new(behavior),
            Rc::clone(&entry),
            self.remote.borrow().clone(),
        ));
        let fiber = ctx.use_component(registrar, config)?;
        // use_component 内已同步激活（apply 换代）——捕获审计代次。
        let generation = entry.generation.get();
        Ok(AsyncFiberHandle::new(&fiber, generation))
    }

    /// 构造「sync 组件 + async 行为」的注册器组件（loader 集成用：
    /// `loader.register_component(name, rt.wrap_component(comp, behavior))`
    /// 后经 `loader.apply` 挂载）。条目同样自登记（apply 时），参与
    /// shutdown 兜底枚举与 [`Self::is_quiet`]。
    pub fn wrap_component(
        &self,
        comp: Rc<dyn Component>,
        behavior: impl AsyncBehavior,
    ) -> Rc<dyn Component> {
        let entry = AsyncFiberEntry::new(Rc::clone(&self.tails));
        entry.attach_registry(Rc::clone(&self.entries));
        Rc::new(AsyncRegistrar::new(
            comp,
            Box::new(behavior),
            entry,
            self.remote.borrow().clone(),
        ))
    }

    /// 条目查询（失败通道/复活路径状态检查用；已淘汰条目返回 `None`）。
    pub fn entry(&self, id: FiberId) -> Option<Rc<AsyncFiberEntry>> {
        self.entries.borrow().get(&id).and_then(|w| w.upgrade())
    }

    /// 退役门面（契约 C-4：生命周期变更走门面）——转发 core
    /// [`Fiber::retire`]；async 尾巴由注册器逆 cancel + 入队、
    /// [`Self::settle`] 收账。
    ///
    /// 句柄失效（fiber 已释放）= 宿主使用失效句柄 = panic = bug。
    pub fn retire(&self, handle: &AsyncFiberHandle) {
        handle
            .fiber()
            .expect("AsyncFiberHandle：fiber 已释放（使用失效句柄 = 宿主 bug）")
            .retire();
    }

    /// 更新门面（契约 C-4；草案 §3.1 update 闭环）：换 config → core
    /// `update_fiber` 强制重跑——旧代 unload（注册器逆 cancel + 旧尾巴
    /// 入队）→ 链式 reload（新代 `begin_activation` + 新 drive spawn，
    /// fiber 身份保留）；编排方随后 [`Self::settle`] 排空旧代尾巴。
    ///
    /// 句柄失效 = 宿主 bug（同 [`Self::retire`]）。
    ///
    /// **时序注记（REVIEW-23383f3 nit-1）**：core 的 `update_fiber` 是
    /// unload+reload **原子同步**实例——新代 spawn 与旧尾巴 settle 之间
    /// 无同步边界，故观测序为「新代先激活（run:2）、旧尾巴后 settle
    /// （rev:1）」。草案 §9 测试 8「旧尾巴先 settle」的措辞是理论/祈愿态
    /// （core 冻结零改动下不可达）；实际语义是**新旧代尾巴独立收账、无
    /// 串代**（I-2），由 FIFO settle 保证。
    pub fn update(&self, handle: &AsyncFiberHandle, config: Rc<dyn Any>) {
        let fiber = handle
            .fiber()
            .expect("AsyncFiberHandle：fiber 已释放（使用失效句柄 = 宿主 bug）");
        self.core.update_fiber(&fiber, config);
    }

    /// 两阶段卸载阶段 2（草案 §3.2）：FIFO 排空收尾队列（I-3 序免费来自
    /// core sync 级联的入队序）。await 本方法直至全部尾巴 settle。
    pub async fn settle(&self) {
        self.tails.settle().await;
    }

    /// async 视图静止判定（I-4）：**无待收尾巴** 且 **无仍 Active 的
    /// async 组件**——`Failed` 视为静止（自退役后 fiber 即 Inactive）。
    ///
    /// 与 core [`Runtime::is_quiet`] 的差异（C-7 双真断言的一致性基础）：
    /// core 把 `Active` 视为静止（无在途转换）；async 视图要求关停后
    /// 不再有任何运行的 async 组件——编排方未退役即关停时本方法为假，
    /// 双真断言得以暴露违约。
    ///
    /// **合取限定（REVIEW-596125d nit-2）**：本方法不排除 core 在途转换
    /// （`Reloading`/`Unloading` 视为非 Active 即通过）——仅在
    /// `&& core.is_quiet()` 合取下才是整体静止判定（`shutdown` 双真断言
    /// 即此用法）；单独复用本方法作通用静止谓词时须自行合取 core 侧。
    pub fn is_quiet(&self) -> bool {
        self.tails.is_empty()
            && self.entries.borrow().values().all(|w| {
                w.upgrade().is_none_or(|entry| {
                    entry
                        .fiber_rc()
                        .is_none_or(|f| !matches!(*f.state(), FiberState::Active { .. }))
                })
            })
    }

    /// 关停（契约 C-7）：编排方**先行退役**（facade retire-all / loader
    /// teardown，hooks 按既有过滤语义处理、退役零配置污染）；本方法兜底
    /// ——对仍 `Active` 的 async fiber 执行注册器逆（cancel +
    /// enqueue_tail 收账，经条目激活会话），但**不代做 core 退役**；随后
    /// settle 到静止，并断言 `core.is_quiet() ∧ async.is_quiet()` **双真**
    /// （正式 assert——开放项 §4 决议）。编排方未退役即关停 = 调用方
    /// 违约，断言失败暴露。
    pub async fn shutdown(&self) {
        let actives: Vec<Rc<AsyncFiberEntry>> = self
            .entries
            .borrow()
            .values()
            .filter_map(|w| w.upgrade())
            .collect();
        for entry in actives {
            let active = entry
                .fiber_rc()
                .is_some_and(|f| matches!(*f.state(), FiberState::Active { .. }));
            if active && let Some(s) = entry.take_session(entry.generation.get()) {
                s.cancel.cancel();
                entry.enqueue_tail(entry.generation.get(), s.handle, s.slot);
            }
        }
        self.settle().await;
        assert!(
            self.core.is_quiet() && self.is_quiet(),
            "shutdown 双真断言（契约 C-7）：编排方应先退役再关停——core 或 async 视图仍有活动/在途尾巴"
        );
    }
}

/// WasmRemote（草案 §2/§4；P1.3 R2 接入点，实际宿主桥留 M1 wasm 专项）。
///
/// M1 host 驱动协议的接入点——**Wasm guest 无自发线程**：`submit` =
/// 请求**入队**，宿主在 **step 边界**驱动并 **回填**（join 语义与
/// [`TokioRemote`] 一致，但执行方为宿主驱动协议 PR #11–13 而非本地
/// worker 池）。
///
/// **范围（P1.3 决策 D-2）**：本类型为**接入点 + 协议接线说明**——
/// 实际宿主驱动桥（host 逐 step 边界驱动、跨 wasm 值传递/回填）在 M1
/// wasm 专项实现（`crates/cordis-wasm` 宿主驱动协议对接）。P1.3 不
/// 提供 [`Remote`] 实现（接入 host 协议前实现无意义）——M1 专项在
/// 接入后 `impl Remote for WasmRemote`（`submit` = 入队 + 宿主驱动）。
pub struct WasmRemote {
    // M1 专项：host 驱动协议句柄（guest 侧仅引用，无自发线程）。
    _private: (),
}
