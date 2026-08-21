//! Cordis Wasm 组件后端（PLAN §4.4，M1 交付）。
//!
//! 基于 wasmtime + 组件模型。wit 世界（真源）：
//! `crates/cordis-wasm/wit/cordis.wit`——通用 host 世界 `cordis`：
//!
//! - **import context**（宿主能力面）：`get`/`set` 与 `inverse` 资源
//!   （Def 8 的逆句柄化，PLAN §4.4：宿主 accumulator 持句柄，
//!   LIFO 调 `run()`；不依赖 destructor）；
//! - **export plugin**（guest 组件形态）：`component` 资源（Def 43 的
//!   (d, p, e) 跨边界）与 `task` 资源（Def 51 𝔈iter 跨边界）——
//!   **宿主驱动效应迭代协议**：`step()` 每步返回 `option<effect-step>`
//!   （含 inverse 句柄），guard 在宿主侧检查（§4.3.2 步界中断）。
//!
//! # 工具链（M1 决定）
//!
//! - guest 以 **`wasm32-wasip2`** target 编译（组件模型 preview2，
//!   rustc 直接产出**组件二进制**，无需 wasm-tools 组件化步骤）；
//! - guest 为 **no_std + alloc**：能力面 = 仅 `context` 接口（论文
//!   §6.3：import 面即能力面）；wasip2 标准库仍引用 WASI p2 接口
//!   （`wasi:io` 等），宿主经 `wasmtime_wasi::p2` 提供；
//! - 宿主 `Store<T>` 的 `T`（本 crate `Host`）实现 [`WasiView`]——
//!   因 `WasiView: Send` 约束，`Host` 内**不持有** `Rc`；核心逆
//!   （捕获 `Rc<Context>`，非 `Send`）存放在实例状态
//!   `InstanceState`（`Rc<RefCell>`，单线程安全）而非 `Host` 中。
//!
//! # 桥接（PR #11）
//!
//! [`WasmComponent`] 实现 [`cordis_core::Component`]：
//!
//! - `inject`/`provide` 经跨边界调用（`call_inject`/`call_provide`）；
//! - `apply` 返回 `WasmTaskIter`：宿主驱动 `task.step()`；guest 在
//!   step 内调用的 `context::set` 记录为 **pending**（宿主侧不直接
//!   绑定），迭代器在 step 后把每个 pending 转发到核心
//!   [`Context::set_dyn`]（ADR-0004 值语义：跨边界值装箱进核心
//!   store），逆以 rep 存入 `InstanceState::core_inverses`；
//! - 迭代器每步产出的逆 = 经 rep 执行核心 Disposer（unbind +
//!   notify）——进入核心累加器后与 `PushingIter` 共享 `StepGuard`
//!   幂等（双路径安全，见 core 文档）。
//!
//! # 值类型与依赖方向（PR #13 审查 m2，REVIEW-1df64a1）
//!
//! 双后端共存（PR #13）要求原生组件与 wasm 组件**值类型统一**：双方
//! 经 `Context::set_dyn` / `get_dyn` 使用 wit `Value` 装箱即可互通。
//! 但 `Value` 类型定义在本 crate 的 wit 绑定中——**原生组件要与 wasm
//! 组件互通须依赖本 crate（仅为一个值类型）**，依赖方向为
//! "原生 → wasm"，与"wasm 依赖 core、core 无关后端"的既有分层不一致。
//! 该边界已记录（THEORY-MAP PR #13 行）；生产化时把 `Value`（统一值
//! 类型）下沉到 `cordis-core` 或独立 value crate（M2 或正式双后端
//! 支持前处理）。
//!
//! # 进度
//!
//! - PR #10：wit 世界 v1 + 宿主加载/驱动原语 + Rust guest 示例端到端；
//! - PR #11：`WasmComponent` 接入 cordis-core（set 转发 + 逆衔接）；
//! - PR #12：wasm 依赖者消费（注入同步 + consumer guest）；
//! - PR #13：双后端共存（同一 loader 加载原生与 wasm，值类型统一）；
//! - PR #14+：沙箱隔离（guest 崩溃不伤宿主）、双语言 guest。

#![deny(missing_docs)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use cordis_async::{Remote, RemoteJoin, RemoteRequest, RemoteValue};
use cordis_core::component::Component;
use cordis_core::context::Context;
use cordis_core::effect::{EffectIter, Step};
use cordis_core::fiber::FiberError;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use wasmtime::component::{Component as WasmComponentType, HasSelf, Linker, Resource, ResourceAny};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

/// wit 世界 `cordis` 的宿主侧绑定（生成于 `wit/cordis.wit`）。
///
/// 生成代码（资源句柄/接口 trait）由 bindgen! 产出、无内联文档——
/// 语义见 wit 文件注释；此处整体放宽 `missing_docs`。
#[allow(missing_docs)]
pub mod wit {
    wasmtime::component::bindgen!({
        path: "wit/cordis.wit",
        world: "cordis",
    });
}

pub use wit::cordis::core::context::{Host as ContextHost, HostInverse, Inverse, Value};

/// 效应步（wit `effect-step` 的导出侧类型，A2b variant 形态）——外部
/// （测试/宿主集成）匹配用。
pub use wit::exports::cordis::core::plugin::EffectStep;

/// 核心逆（非 Send：捕获 `Rc<Context>`；仅单线程实例状态可执行）。
type CoreInverse = (String, Box<dyn FnOnce()>);

/// 待转发的绑定请求：guest 在 step 内调用 `context::set` 时记录，
/// 迭代器在 step 返回后转发到核心 store。
struct PendingSet {
    /// 逆句柄 rep（guest 持同一句柄，宿主撤销时按 rep 找核心逆）。
    rep: u32,
    /// 键（realm 解析由核心 `set_dyn` 承担，审查 m2）。
    key: String,
    /// 值（wit 跨边界值）。
    value: Value,
}

/// guest 提交的远端请求（submit 入队；step 后由迭代器 pump 到注入
/// Remote，W1b）。
struct RemotePending {
    rep: u32,
    name: String,
    params: Vec<Value>,
}

/// 宿主注册的远端操作（W-D2）：guest 提交名 + 参数 → wit `Value` 结果。
/// 以 `Arc` 传递（`RemoteOp: Send + Sync`——跨线程经 `RemoteRequest::boxed`
/// 交给 worker 执行；trait 对象自身 `Send+Sync`）。
pub type RemoteOp = dyn Fn(Vec<Value>) -> Value + Send + Sync;

/// 组件宿主状态：pending 队列 + 绑定镜像 + WASI 上下文。
///
/// `Store<T>` 的 `T`（`Send`：`WasiView` 约束）。实现 `context`
/// 接口（get/set）与 `inverse` 资源（run/drop）。
///
/// **rep 回收（P-1，产品验证线）**：`next_rep` 单调分配 + `inverse_free`
/// 复用池——`run_inverse`（逆执行）后 rep 入池，长驻组件生命周期内
/// 分配量有界（≈ 峰值并发逆数，非操作次数；REVIEW-2a7a686 m3 已知
/// 边界 → 已回收）。`drop` 保持 no-op（句柄销毁 ≠ 逆执行）。
pub struct Host {
    /// guest 在 step 期间累积的绑定请求（迭代器 step 后转发并清空）。
    pending: Vec<PendingSet>,
    /// guest 提交的远端请求（submit 入队；step 后 pump，W1b）。
    remote_pending: Vec<RemotePending>,
    /// 远端句柄 → 结果（None = 未就绪；Some(Ok/Err) = 就绪，take 取）。
    remote_results: std::collections::HashMap<u32, Option<Result<Value, String>>>,
    /// remote 句柄 rep 分配。
    next_remote_rep: u32,
    /// 绑定镜像（`get` 读取；由迭代器在转发后同步）。
    bindings: HashMap<String, Value>,
    /// 逆句柄计数器（跨步骤单调，保证 rep 唯一；回收见 inverse_free）。
    next_rep: u32,
    /// 可复用逆 rep 池（P-1：仅「逆已执行」入池——`run_inverse` 后
    /// 槽位空、句柄已撤销，复用安全；`drop` 不入池（句柄销毁 ≠ 逆执行，
    /// 绑定仍待撤销——REVIEW-2a7a686 m3 回收方案的语义边界））。
    inverse_free: Vec<u32>,
    /// WASI preview2 上下文（wasip2 标准库依赖）。
    wasi: WasiCtx,
    /// WASI 资源表。
    table: wasmtime::component::ResourceTable,
}

impl Host {
    /// 空宿主。
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            remote_pending: Vec::new(),
            remote_results: std::collections::HashMap::new(),
            next_remote_rep: 0,
            bindings: HashMap::new(),
            next_rep: 0,
            inverse_free: Vec::new(),
            wasi: WasiCtxBuilder::new().build(),
            table: wasmtime::component::ResourceTable::new(),
        }
    }

    /// 绑定镜像（调试/断言用）。
    pub fn bindings(&self) -> &HashMap<String, Value> {
        &self.bindings
    }
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

impl WasiView for Host {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl HostInverse for Host {
    /// 宿主侧逆执行入口——**协议上不被调用**（审查 m4，
    /// REVIEW-2a7a686）：核心逆（非 `Send`）存放在
    /// `InstanceState::core_inverses`，由迭代器产出的逆闭包经 rep
    /// 执行（unbind + notify + 镜像清理）；撤销**只能由宿主驱动**
    /// （组件卸载路径），guest 调用 `inverse.run` 为协议违反——
    /// 以 panic 显式失败（panic = bug），不做静默 no-op。
    fn run(&mut self, _self_: Resource<Inverse>) {
        panic!(
            "inverse.run 由宿主驱动撤销（组件卸载路径）——guest 调用违反协议（Def 8 逆的撤销归宿主）"
        );
    }
    /// drop（m3，REVIEW-2a7a686 + P-1 更新）：句柄销毁 ≠ 逆执行——绑定仍
    /// 待撤销，故本处保持 no-op 且**不入 free list**；rep 回收经
    /// `run_inverse`（逆执行后入池，REVIEW-P1-EXIT Minor-1 落地）。
    fn drop(&mut self, _rep: Resource<Inverse>) -> wasmtime::Result<()> {
        Ok(())
    }
}

/// M1 wasm 桥（W1a）：`remote` 接口 stub——W1b 填充宿主驱动（注册表 +
/// 注入 `Remote` 提交 + 句柄结果登记）。`todo!` 仅占位（W1a 不调用，
/// 编译通过即可）。
impl wit::cordis::core::remote::Host for Host {
    // bindgen 签名（无 Err 通道——错误恒延迟到 take 的 err，W-D3 异步契约）。
    fn submit(
        &mut self,
        name: String,
        params: Vec<wit::cordis::core::remote::Value>,
    ) -> Resource<wit::cordis::core::remote::Handle> {
        let rep = self.next_remote_rep;
        self.next_remote_rep += 1;
        self.remote_pending
            .push(RemotePending { rep, name, params });
        Resource::new_own(rep)
    }
}

impl wit::cordis::core::remote::HostHandle for Host {
    fn take(
        &mut self,
        handle: Resource<wit::cordis::core::remote::Handle>,
    ) -> Option<Result<wit::cordis::core::remote::Value, String>> {
        self.remote_results
            .get(&handle.rep())
            .and_then(|o| o.clone())
    }
    fn drop(
        &mut self,
        handle: Resource<wit::cordis::core::remote::Handle>,
    ) -> wasmtime::Result<()> {
        // guest 弃句柄 / 实例卸载：清理结果槽（worker 完成即弃则不残留）。
        self.remote_results.remove(&handle.rep());
        Ok(())
    }
}

impl ContextHost for Host {
    fn get(&mut self, key: String) -> Option<Value> {
        self.bindings.get(&key).cloned()
    }
    fn set(&mut self, key: String, value: Value) -> Result<Resource<Inverse>, String> {
        // P-1：优先复用已执行逆释放的 rep（free list），空则单调递增。
        let rep = self.inverse_free.pop().unwrap_or_else(|| {
            let r = self.next_rep;
            self.next_rep += 1;
            r
        });
        // 镜像先行：guest 的 get 在 step 内立即可读（核心绑定在迭代器
        // 转发后生效；逆执行时清理镜像）。
        self.bindings.insert(key.clone(), value.clone());
        self.pending.push(PendingSet { rep, key, value });
        Ok(Resource::new_own(rep))
    }
}

/// wasm 实例状态（每组件一个）：wasmtime store + 绑定实例 + 核心逆表。
///
/// **非 `Send`**（核心逆捕获 `Rc<Context>`）——单线程宿主以
/// `Rc<RefCell>` 持有（ADR-0002）。store 与核心逆表各自 `RefCell`：
/// 跨边界调用（借 `instance` 不可变 + `store` 可变）与逆执行
/// （`core_inverses` + 镜像）无借用冲突。
struct InstanceState {
    /// wasmtime store（宿主状态 `Host`：pending/镜像/WASI）。
    store: RefCell<Store<Host>>,
    /// bindgen 生成的绑定实例。
    instance: wit::Cordis,
    /// component 资源实例句柄。
    component_any: ResourceAny,
    /// 核心逆表：`rep → (键, 核心 Disposer)`（非 Send，仅此处可执行；
    /// 键用于逆执行后清理绑定镜像）。
    core_inverses: RefCell<Vec<Option<CoreInverse>>>,
    /// 注入的远端桥（W1b；复用 `cordis_async::Remote`，v1 = TokioRemote）。
    remote: RefCell<Option<Rc<dyn Remote>>>,
    /// 远端操作注册表（W-D2：名 → 可执行操作；非 Send 状态可捕获）。
    remote_ops: RefCell<HashMap<String, Arc<RemoteOp>>>,
    /// 在途远端 join（rep → LocalBoxFuture；step 边界 poll 回填，W1b）。
    remote_joins: RefCell<HashMap<u32, RemoteJoin<RemoteValue>>>,
}

impl InstanceState {
    /// 经 rep 执行核心逆（迭代器产出的逆闭包调用）：
    /// 核心 unbind + notify，并清理绑定镜像。
    fn run_inverse(&self, rep: u32) {
        // 两步：先 take（借用立即结束），再执行——if-let 临时借用
        // 存活到块尾会误伤后续借用（诊断结论）。
        let taken = self
            .core_inverses
            .borrow_mut()
            .get_mut(rep as usize)
            .and_then(|slot| slot.take());
        if let Some((key, task)) = taken {
            task();
            // n-2（REVIEW-f57faad）：task() panic 时 rep 不入池（保守留白——
            // 核心逆不应 panic，违反即宿主 bug）。
            let mut store = self.store.borrow_mut();
            let host = store.data_mut();
            host.bindings.remove(&key);
            // P-1：逆已执行（槽位空、句柄已撤销）→ rep 入 free list 复用。
            host.inverse_free.push(rep);
        }
    }
}

/// Wasm 组件（Def 43 的 (d, p, e) 跨边界形态）：接入 cordis-core。
///
/// 经 [`WasmComponent::load`] 从组件二进制创建（实例化 + constructor）。
///
/// **释放语义（§6.4 走查锚点）**：本类型无 `impl Drop`——`InstanceState`
/// （内含 wasmtime `Store`）由 `Rc` 归零时原生释放，与论文 §6.4 的
/// "released when a native embedder drops it (e.g., Wasmtime)" 逐字一致；
/// loader 移除条目 → 退役级联 → `Rc` 释放即模块撤回。
pub struct WasmComponent {
    state: Rc<RefCell<InstanceState>>,
}

impl WasmComponent {
    /// 加载组件二进制并实例化（Algorithm 4 的 use 前置：constructor）。
    pub fn load(engine: &Engine, component_bytes: &[u8]) -> anyhow::Result<Rc<Self>> {
        let mut linker = Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        let mut store = Store::new(engine, Host::new());
        wit::Cordis::add_to_linker::<_, HasSelf<_>>(&mut linker, |host| host)?;
        let component = WasmComponentType::from_binary(engine, component_bytes)?;
        let instance = wit::Cordis::instantiate(&mut store, &component, &linker)?;
        let comp = instance.cordis_core_plugin().component();
        let component_any = comp.call_constructor(&mut store)?;
        // load 无 ctx：占位（apply 时填充）。
        Ok(Rc::new(Self {
            state: Rc::new(RefCell::new(InstanceState {
                store: RefCell::new(store),
                instance,
                component_any,
                core_inverses: RefCell::new(Vec::new()),
                remote: RefCell::new(None),
                remote_ops: RefCell::new(HashMap::new()),
                remote_joins: RefCell::new(HashMap::new()),
            })),
        }))
    }

    /// 宿主预注镜像（C 探针辅助，REVIEW-C1 minor-1）：向 guest 镜像写入
    /// 输入键——阶段 2 的 guest 经 `get` 读到回注值（无需注入依赖声明，
    /// 也不触碰核心依赖解析）。探针形态（评估后由正式通道替代，如 B 的
    /// Await 或 sync_injected 化）。
    pub fn preseed_mirror(&self, key: impl Into<String>, value: Value) {
        self.state
            .borrow()
            .store
            .borrow_mut()
            .data_mut()
            .bindings
            .insert(key.into(), value);
    }

    /// 绑定镜像（调试/断言用）。
    pub fn bindings(&self) -> HashMap<String, Value> {
        self.state.borrow().store.borrow().data().bindings().clone()
    }
}

impl WasmComponent {
    /// 配置远端桥（W1b；v1 注入 TokioRemote 供 guest 的 remote submit 执行；
    /// O-6：执行在 worker，不触碰组合线程资源）。
    pub fn configure_remote(&self, remote: Option<Rc<dyn Remote>>) {
        *self.state.borrow().remote.borrow_mut() = remote;
    }

    /// 注册远端操作（W-D2：guest 提交名 + 参数 → wit Value）。
    pub fn register_remote(&self, name: impl Into<String>, op: Arc<RemoteOp>) {
        self.state
            .borrow()
            .remote_ops
            .borrow_mut()
            .insert(name.into(), op);
    }

    /// 宿主侧轮询在途远端（W2）。**必要性（REVIEW-704a46c Minor-1）**：
    /// core `execute` 为同步一口气循环且单步组件一步 `done`，`apply` 后
    /// `WasmTaskIter::next` 不再运行——poll_remotes 是**单步激活场景回填的
    /// 唯一驱动面**；多步/长驻场景仍由迭代器 step 边界内部轮询。组合线程
    /// 在空闲/检查点调用，不阻塞（noop-waker 单次探测；O-6）。
    pub fn poll_remotes(&self) {
        let state = self.state.borrow();
        drive_poll_remote(
            &mut state.remote_joins.borrow_mut(),
            &mut state.store.borrow_mut().data_mut().remote_results,
        );
    }

    /// 远端句柄结果复制（调试/断言用）。
    pub fn remote_results_debug(
        &self,
    ) -> std::collections::HashMap<u32, Option<Result<Value, String>>> {
        self.state
            .borrow()
            .store
            .borrow()
            .data()
            .remote_results
            .clone()
    }

    /// 远端句柄结果复制（调试/断言用）。
    pub fn remote_result(&self, rep: u32) -> Option<Option<Result<Value, String>>> {
        self.state
            .borrow()
            .store
            .borrow()
            .data()
            .remote_results
            .get(&rep)
            .cloned()
    }
}

impl Component for WasmComponent {
    fn inject(&self) -> KeySet {
        let state = self.state.borrow();
        let component_any = state.component_any;
        let inject = state
            .instance
            .cordis_core_plugin()
            .component()
            .call_inject(&mut *state.store.borrow_mut(), component_any)
            .expect("跨边界 inject 调用");
        inject.iter().map(|s| Symbol::intern(s)).collect()
    }

    fn provide(&self) -> KeySet {
        let state = self.state.borrow();
        let component_any = state.component_any;
        let provide = state
            .instance
            .cordis_core_plugin()
            .component()
            .call_provide(&mut *state.store.borrow_mut(), component_any)
            .expect("跨边界 provide 调用");
        provide.iter().map(|s| Symbol::intern(s)).collect()
    }

    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        // 作用域块：`Ref` 是 Drop 类型，借用活到块末——须在
        // `ctx` 写入（borrow_mut）前结束。
        let task_any = {
            let state = self.state.borrow();
            let component_any = state.component_any;
            state
                .instance
                .cordis_core_plugin()
                .component()
                .call_start(&mut *state.store.borrow_mut(), component_any)
                .expect("跨边界 start 调用")
        };
        // 注入键（step 前同步其值进镜像，PR #12）。
        let inject = self.inject().iter().collect();
        Box::new(WasmTaskIter {
            state: Rc::clone(&self.state),
            task_any,
            ctx,
            inject,
        })
    }
}

/// 宿主驱动效应迭代器（Def 51 𝔈iter 跨边界形态，Algorithm 5 的 reload 驱动）。
///
/// 每步：驱动 guest `task.step()`；把 guest 在 step 内调用的
/// `context::set`（pending）转发到核心 [`Context::set_dyn`]；核心逆按
/// rep 存入实例的 `core_inverses`；本步逆 = 执行这些核心逆的闭包
/// （进入核心累加器，LIFO 撤销）。
///
/// **注入依赖同步（PR #12）**：step 前把核心 store 中本组件 `inject`
/// 键的值同步进镜像（[`Host::get`] 据此读取）——guest 消费依赖
/// （论文 §6.3 桥接透明性）。值为 wasm wit `Value` 装箱（另一 wasm
/// 组件提供）时可同步；原生组件提供的值（不同装箱类型）**不同步**
/// （镜像无此键 = `get` 返回 none，M1 边界：跨类型值翻译未支持）。
struct WasmTaskIter {
    state: Rc<RefCell<InstanceState>>,
    task_any: ResourceAny,
    ctx: Rc<Context>,
    /// 本组件注入键（符号；step 前同步其值进镜像）。
    inject: Vec<Symbol>,
}

impl WasmTaskIter {
    /// 同步注入依赖：核心 store → 镜像（guest 的 get 可读）。
    fn sync_injected(&self) {
        let state = self.state.borrow();
        let mut store = state.store.borrow_mut();
        let mirror = &mut store.data_mut().bindings;
        for key in &self.inject {
            // 审查 nit1（REVIEW-54a9b08）：单次 downcast 判型 + 取值。
            // 值为另一 wasm 组件的 wit `Value` 装箱时同步；原生组件
            // 提供的值（不同装箱类型）不同步——镜像无此键（get 返回
            // none），跨类型值翻译 M1 不支持。
            match self.ctx.get_dyn(*key) {
                Some(value) => match value.downcast_ref::<Value>() {
                    Some(v) => {
                        mirror.insert(key.as_str().to_string(), v.clone());
                    }
                    None => {
                        mirror.remove(key.as_str());
                    }
                },
                None => {
                    mirror.remove(key.as_str());
                }
            }
        }
    }

    /// 转发本步 pending 的 set 到核心 store（ADR-0004 值装箱），
    /// 返回 `(rep, 键, 核心 Disposer)` 列表（镜像已由 [`Host::set`]
    /// 先行插入，逆执行时清理）。
    fn forward_pending(&self) -> Vec<(u32, CoreInverse)> {
        let pending: Vec<PendingSet> = {
            let state = self.state.borrow();
            std::mem::take(&mut state.store.borrow_mut().data_mut().pending)
        };
        let mut inverses = Vec::new();
        for set in pending {
            // 审查 m2（REVIEW-2a7a686）：传**键**给 `set_dyn`——ρ 解析
            // （isolate 映射）由核心承担，与 typed `set` 对称。
            let key = Symbol::intern(&set.key);
            let value: Box<dyn std::any::Any + Send + Sync> = Box::new(set.value);
            // M2-PR1（L-Raise）+ 审查 nit4（REVIEW-32a913d）：越界写经
            // **供给纪律预检**直接 raise——不进入核心 set_dyn（避免
            // catch_unwind 捕获面覆盖 notify 级联中的宿主 bug panic）；
            // set_dyn 仅剩 AlreadyBound（Err）与防御性纪律捕获。
            let fid = self.ctx.fiber().expect("wasm 迭代器运行于 fiber ctx");
            let allowed = self
                .ctx
                .runtime()
                .fiber(fid)
                .is_some_and(|f| f.provide().contains(key));
            if !allowed {
                FiberError::new(format!(
                    "组件 {fid:?} 越界写入未声明的键 {key}（Def 43/48 纪律）"
                ))
                .raise();
            }
            // 注（审查 nit4）：catch_unwind 捕获面仍含 set_dyn 内部的
            // notify 级联（绑定后广播）——宿主 reactor panic 会被误判为
            // 组件失败（已知边界；同步核心 reactor 可信，现实触发概率低）。
            let disposer = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.ctx.set_dyn(key, value)
            })) {
                Ok(Ok(disposer)) => disposer,
                Ok(Err(err)) => {
                    FiberError::new(format!("wasm 组件绑定 {} 失败：{err:?}", set.key)).raise()
                }
                Err(payload) => {
                    // 核心纪律 panic（&'static str / String 载荷）。
                    let msg = payload
                        .downcast_ref::<&'static str>()
                        .map(|s| s.to_string())
                        .or_else(|| payload.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "wasm 组件违反核心纪律".into());
                    FiberError::new(msg).raise()
                }
            };
            inverses.push((set.rep, (set.key, disposer)));
        }
        inverses
    }
}

impl WasmTaskIter {
    /// 本步提交的远端请求 → 注入 Remote（委托 [`drive_pump_remote`]）。
    fn pump_remotes(&self) {
        let state = self.state.borrow();
        let pending: Vec<RemotePending> =
            std::mem::take(&mut state.store.borrow_mut().data_mut().remote_pending);
        let remote = state.remote.borrow().clone();
        let ops = &*state.remote_ops.borrow();
        drive_pump_remote(
            pending,
            remote,
            ops,
            &mut state.remote_joins.borrow_mut(),
            &mut state.store.borrow_mut().data_mut().remote_results,
        );
    }

    /// step 边界 poll 在途远端 join（委托 [`drive_poll_remote`]）。
    fn poll_remotes(&self) {
        let state = self.state.borrow();
        drive_poll_remote(
            &mut state.remote_joins.borrow_mut(),
            &mut state.store.borrow_mut().data_mut().remote_results,
        );
    }
}

/// 提交驱动（纯函数，W1b 单测点）：pending 请求 → 注入 Remote（op 映射）。
fn drive_pump_remote(
    pending: Vec<RemotePending>,
    remote: Option<Rc<dyn Remote>>,
    ops: &std::collections::HashMap<String, Arc<RemoteOp>>,
    joins: &mut HashMap<u32, RemoteJoin<RemoteValue>>,
    results: &mut HashMap<u32, Option<Result<Value, String>>>,
) {
    for p in pending {
        let key = p.rep;
        match (ops.get(&p.name).cloned(), &remote) {
            (Some(op), Some(remote)) => {
                let params = p.params;
                // m-1（REVIEW-f883492）：op 在 worker 内执行——catch_unwind
                // 把 op panic 转 Err（经结果通道回填 err），避免 worker panic
                // 穿透到组合线程 step 边界的 await_remote_join expect。
                let req = RemoteRequest::boxed(move || -> Result<Value, String> {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| op(params)))
                        .map_err(panic_payload_to_string)
                });
                joins.insert(key, remote.submit(req));
            }
            (None, _) => {
                results.insert(
                    key,
                    Some(Err(format!(
                        "未知远端操作 \"{}\"（先 register_remote）",
                        p.name
                    ))),
                );
            }
            (_, None) => {
                results.insert(
                    key,
                    Some(Err("远端桥未配置（configure_remote 注入 Remote）".into())),
                );
            }
        }
    }
}

/// 回填驱动（纯函数，W1b 单测点）：noop-waker poll 在途 join，Ready 回填结果。
fn drive_poll_remote(
    joins: &mut HashMap<u32, RemoteJoin<RemoteValue>>,
    results: &mut HashMap<u32, Option<Result<Value, String>>>,
) {
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    let mut done = Vec::new();
    for (rep, join) in joins.iter_mut() {
        if let std::task::Poll::Ready(v) = join.as_mut().poll(&mut cx) {
            results.insert(*rep, Some(value_from_remote(v)));
            done.push(*rep);
        }
    }
    for rep in done {
        joins.remove(&rep);
    }
}

impl EffectIter for WasmTaskIter {
    fn next(&mut self) -> Step {
        // W1b：先 poll 在途远端（guest 本次 step 的 take 可读到回填）。
        self.poll_remotes();
        // 注入依赖同步（guest 的 get 读核心 store 的当前值）。
        self.sync_injected();
        // 驱动 guest 一步（同步；guard 在核心 execute 的步界检查）。
        // M2-PR1（L-Raise）：guest trap（wasmtime 错误）不再是宿主 panic
        // ——以 FiberError 载荷 raise，核心 reload 捕获后记录为 fiber 的
        // 失败 outcome（§4.3.4 𝔈fail）。
        // 注（审查 nit6，REVIEW-32a913d）：wasmtime 错误未区分 guest trap
        // 与宿主侧驱动错误（后者罕见）——一律转组件失败（粒度过度属已知
        // 边界，如需精确可在 Err 分支判别 Trap）。
        let step = {
            let state = self.state.borrow();
            state
                .instance
                .cordis_core_plugin()
                .task()
                .call_step(&mut *state.store.borrow_mut(), self.task_any)
                .unwrap_or_else(|err| {
                    FiberError::new(format!("wasm 组件 step 失败（trap）：{err}")).raise()
                })
        };
        // 转发本步 pending 的 set 到核心 store。
        let forwarded = self.forward_pending();
        // W1b：本步提交的远端请求 pump 到注入 Remote。
        self.pump_remotes();
        // 收集本步 rep（逆闭包按应用序执行）。
        let reps: Vec<u32> = forwarded.iter().map(|(rep, _)| *rep).collect();
        // 把核心逆挂到 rep 表（run_inverse 经 rep 查找、幂等 take）。
        {
            let state = self.state.borrow();
            let mut inverses = state.core_inverses.borrow_mut();
            for (rep, (key, disposer)) in forwarded {
                let idx = rep as usize;
                while inverses.len() <= idx {
                    inverses.push(None);
                }
                inverses[idx] = Some((key, disposer));
            }
        }
        // 本步逆：依次执行本步各 rep 的核心逆（应用序）。
        let state = Rc::clone(&self.state);
        let step_inverse = Box::new(move || {
            let state = state.borrow();
            for rep in reps {
                state.run_inverse(rep);
            }
        }) as Box<dyn FnOnce()>;
        // A2b（variant 形态，docs/cordis-wasm-A2B-PLAN.md）：`wait` 步 +
        // 在途远端 join → `Step::Await`（core 挂起，回填后 advance 恢复；
        // 等待步无逆、不累计）。`step` = 有逆步继续；`done` = 终止。
        use wit::exports::cordis::core::plugin::EffectStep;
        let awaiting = {
            let state = self.state.borrow();
            matches!(&step, Some(EffectStep::Wait)) && !state.remote_joins.borrow().is_empty()
        };
        match step {
            _ if awaiting => Step::Await,
            Some(EffectStep::Step(_)) => Step::Yielded(step_inverse),
            Some(EffectStep::Done(_)) => Step::Finished(step_inverse),
            Some(EffectStep::Wait) => Step::Yielded(step_inverse),
            None => Step::Finished(Box::new(|| {}) as Box<dyn FnOnce()>),
        }
    }
}

/// `RemoteValue` → wit `Value`（downcast；非 Value 类型 → Err）。
/// （发送方向不需要显式适配：`boxed(move || op(params))` 直接装箱为
/// `RemoteValue` 传输容器——W1b。）
fn value_from_remote(v: RemoteValue) -> Result<Value, String> {
    match v.downcast::<Result<Value, String>>() {
        Ok(b) => match *b {
            Ok(value) => Ok(value),
            Err(e) => Err(e),
        },
        Err(_) => {
            Err("远端结果非协议形态（Result<Value, String>）——注入 Remote 违反适配契约".into())
        }
    }
}

/// op panic 载荷 → err 文本（m-1；op 违反「不得 panic」义务时的兜底诊断）。
fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "远端操作 panic（op 违反「不得 panic」义务——宿主注册缺陷）".to_string()
    }
}

#[cfg(test)]
mod remote_tests {
    use super::*;
    use cordis_async::{Remote, TokioRemote};

    fn op_text(s: &'static str) -> Arc<RemoteOp> {
        Arc::new(move |_params: Vec<Value>| Value::Text(s.to_string()))
    }

    /// 泵送 → 提交到注入 Remote（TokioRemote/worker）→ 轮询回填。
    #[test]
    fn pump_submits_and_poll_backfills() {
        let worker = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .expect("worker runtime");
        let remote: Rc<dyn Remote> = Rc::new(TokioRemote::new(worker.handle().clone()));

        let mut ops = std::collections::HashMap::new();
        ops.insert("greet".to_string(), op_text("hi"));
        let mut joins = HashMap::new();
        let mut results = HashMap::new();
        drive_pump_remote(
            vec![RemotePending {
                rep: 0,
                name: "greet".into(),
                params: vec![],
            }],
            Some(remote),
            &ops,
            &mut joins,
            &mut results,
        );
        assert!(joins.contains_key(&0), "请求已提交（join 在途）");

        // 轮询直到 worker 完成回填（noop-waker poll；worker 完成即 ready）。
        for _ in 0..2000 {
            drive_poll_remote(&mut joins, &mut results);
            if results.contains_key(&0) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        match results.get(&0) {
            Some(Some(Ok(Value::Text(s)))) => assert_eq!(s, "hi", "回填值"),
            other => panic!("期望回填 Ok(Text(hi))，实际 {other:?}"),
        }
        assert!(!joins.contains_key(&0), "完成的 join 已移除");
    }

    /// 未注册操作 → 立即 err（句柄 take 通道）。
    #[test]
    fn unknown_op_immediate_err() {
        let ops = std::collections::HashMap::new();
        let mut joins = HashMap::new();
        let mut results = HashMap::new();
        drive_pump_remote(
            vec![RemotePending {
                rep: 5,
                name: "nope".into(),
                params: vec![],
            }],
            None,
            &ops,
            &mut joins,
            &mut results,
        );
        assert!(joins.is_empty(), "未知名不提交");
        assert!(
            matches!(results.get(&5), Some(Some(Err(e))) if e.contains("未知远端操作")),
            "未知操作 → 句柄 err"
        );
    }

    /// 未配置桥 → err。
    #[test]
    fn unconfigured_remote_err() {
        let mut ops = std::collections::HashMap::new();
        ops.insert("x".to_string(), op_text("x"));
        let mut joins = HashMap::new();
        let mut results = HashMap::new();
        drive_pump_remote(
            vec![RemotePending {
                rep: 1,
                name: "x".into(),
                params: vec![],
            }],
            None,
            &ops,
            &mut joins,
            &mut results,
        );
        assert!(joins.is_empty());
        assert!(
            matches!(results.get(&1), Some(Some(Err(e))) if e.contains("未配置")),
            "未配置桥 → err"
        );
    }

    /// 适配器：非协议形态的 RemoteValue → err。
    #[test]
    fn adapter_rejects_non_protocol_value() {
        let err = value_from_remote(Box::new(42u32)).unwrap_err();
        assert!(err.contains("非协议形态"));
        let err2 = value_from_remote(Box::new(Value::Text("x".into()))).unwrap_err();
        assert!(err2.contains("非协议形态"));
    }

    /// m-1：op panic 被 worker 内捕获 → err 回填（不穿透组合线程）。
    #[test]
    fn op_panic_backfills_err() {
        let worker = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .expect("worker runtime");
        let remote: Rc<dyn Remote> = Rc::new(TokioRemote::new(worker.handle().clone()));
        let panic_op: Arc<RemoteOp> = Arc::new(|_| panic!("boom"));
        let mut ops = std::collections::HashMap::new();
        ops.insert("boom".to_string(), panic_op);
        let mut joins = HashMap::new();
        let mut results = HashMap::new();
        drive_pump_remote(
            vec![RemotePending {
                rep: 0,
                name: "boom".into(),
                params: vec![],
            }],
            Some(remote),
            &ops,
            &mut joins,
            &mut results,
        );
        for _ in 0..2000 {
            drive_poll_remote(&mut joins, &mut results);
            if results.contains_key(&0) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            matches!(results.get(&0), Some(Some(Err(e))) if e.contains("boom")),
            "op panic → 句柄 err 回填（组合线程不 panic）：{:?}",
            results
        );
    }

    /// W3 清理：drop 句柄 → 结果槽清除（guest 弃句柄/实例卸载不残留）。
    #[test]
    fn host_handle_drop_clears_result_slot() {
        let mut host = Host::new();
        let handle = wit::cordis::core::remote::Host::submit(&mut host, "echo".to_string(), vec![]);
        // 直接登记一个就绪结果，再 drop 句柄 → 槽清除。
        host.remote_results
            .insert(handle.rep(), Some(Ok(Value::Text("x".into()))));
        wit::cordis::core::remote::HostHandle::drop(&mut host, handle).expect("drop");
        assert!(
            !host.remote_results.contains_key(&0),
            "drop 句柄 → 结果槽清除"
        );
    }

    /// P-1 有界性验收：长驻循环 set→逆执行（rep 释放）→ set，rep 复用 +
    /// `next_rep` 有界恒定（REVIEW-2a7a686 m3 已知边界 → 已回收）。
    #[test]
    fn host_inverse_free_reuse_bounds_rep_allocation() {
        let mut host = Host::new();
        let set_once = |host: &mut Host| -> u32 {
            let handle = wit::cordis::core::remote::Host::submit(host, "nope".to_string(), vec![]);
            let _ = handle;
            // 用 context::set 分配逆 rep（submit 用 remote rep 空间）。
            let inv = ContextHost::set(host, "k".to_string(), Value::Count(1)).expect("set");
            inv.rep()
        };
        let first = set_once(&mut host);
        // 模拟逆执行释放（与 run_inverse 的入池等价）。
        host.inverse_free.push(first);
        for _ in 0..1000 {
            let r = set_once(&mut host);
            assert_eq!(r, first, "复用已执行逆释放的 rep");
            host.inverse_free.push(r);
        }
        assert_eq!(host.next_rep, 1, "next_rep 有界恒定（非操作次数）");
    }

    /// Host submit 入队 + rep 分配 + take 未就绪。
    #[test]
    fn host_submit_enqueues_and_take_pending() {
        let mut host = Host::new();
        let handle =
            wit::cordis::core::remote::Host::submit(&mut host, "greet".to_string(), vec![]);
        assert_eq!(handle.rep(), 0, "首个句柄 rep 0");
        assert_eq!(host.remote_pending.len(), 1, "已入队");
        let taken = wit::cordis::core::remote::HostHandle::take(&mut host, handle);
        assert!(
            taken.is_none(),
            "未就绪 → take 返回 None（整个 Option，非 Some(None)）"
        );
    }
}
