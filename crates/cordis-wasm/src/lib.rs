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
//!   [`InstanceState`]（`Rc<RefCell>`，单线程安全）而非 `Host` 中。
//!
//! # 桥接（PR #11）
//!
//! [`WasmComponent`] 实现 [`cordis_core::Component`]：
//!
//! - `inject`/`provide` 经跨边界调用（`call_inject`/`call_provide`）；
//! - `apply` 返回 [`WasmTaskIter`]：宿主驱动 `task.step()`；guest 在
//!   step 内调用的 `context::set` 记录为 **pending**（宿主侧不直接
//!   绑定），迭代器在 step 后把每个 pending 转发到核心
//!   [`Context::set_dyn`]（ADR-0004 值语义：跨边界值装箱进核心
//!   store），逆以 rep 存入 [`InstanceState::core_inverses`]；
//! - 迭代器每步产出的逆 = 经 rep 执行核心 Disposer（unbind +
//!   notify）——进入核心累加器后与 `PushingIter` 共享 `StepGuard`
//!   幂等（双路径安全，见 core 文档）。
//!
//! # 值类型与依赖方向（PR #13 审查 m2，REVIEW-1df64a1）
//!
//! 双后端共存（PR #13）要求原生组件与 wasm 组件**值类型统一**：双方
//! 经 [`Context::set_dyn`]/[`get_dyn`] 使用 wit `Value` 装箱即可互通。
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

/// 组件宿主状态：pending 队列 + 绑定镜像 + WASI 上下文。
///
/// `Store<T>` 的 `T`（`Send`：`WasiView` 约束）。实现 `context`
/// 接口（get/set）与 `inverse` 资源（run/drop）。
///
/// **rep 空间单调（审查 m3，REVIEW-2a7a686）**：`next_rep` 与
/// [`InstanceState::core_inverses`] 槽位只增不减（drop 为 no-op）——
/// 已知边界（与组件生命周期内 set 次数同阶；M2 提供回收）。
pub struct Host {
    /// guest 在 step 期间累积的绑定请求（迭代器 step 后转发并清空）。
    pending: Vec<PendingSet>,
    /// 绑定镜像（`get` 读取；由迭代器在转发后同步）。
    bindings: HashMap<String, Value>,
    /// 逆句柄计数器（跨步骤单调，保证 rep 唯一）。
    next_rep: u32,
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
            bindings: HashMap::new(),
            next_rep: 0,
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
    /// [`InstanceState::core_inverses`]，由迭代器产出的逆闭包经 rep
    /// 执行（unbind + notify + 镜像清理）；撤销**只能由宿主驱动**
    /// （组件卸载路径），guest 调用 `inverse.run` 为协议违反——
    /// 以 panic 显式失败（panic = bug），不做静默 no-op。
    fn run(&mut self, _self_: Resource<Inverse>) {
        panic!(
            "inverse.run 由宿主驱动撤销（组件卸载路径）——guest 调用违反协议（Def 8 逆的撤销归宿主）"
        );
    }
    /// drop（m3，REVIEW-2a7a686）：防御性退化——核心逆在
    /// [`InstanceState::core_inverses`]（`Host` 拿不到），槽位与
    /// `next_rep` 空间**单调增长**属已知边界（每个组件的 rep 数量
    /// 与其生命周期内 set 次数同阶，M1 可接受；M2 提供回收）。
    fn drop(&mut self, _rep: Resource<Inverse>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl ContextHost for Host {
    fn get(&mut self, key: String) -> Option<Value> {
        self.bindings.get(&key).cloned()
    }
    fn set(&mut self, key: String, value: Value) -> Result<Resource<Inverse>, String> {
        let rep = self.next_rep;
        self.next_rep += 1;
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
            self.store.borrow_mut().data_mut().bindings.remove(&key);
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
            })),
        }))
    }

    /// 绑定镜像（调试/断言用）。
    pub fn bindings(&self) -> HashMap<String, Value> {
        self.state.borrow().store.borrow().data().bindings().clone()
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
            // M2-PR1（L-Raise）：guest 侧写入失败（绑定冲突 = AlreadyBound；
            // 越界写未声明键 = 核心 Def 43/48 纪律 panic）不再是宿主 panic
            // ——统一转为 FiberError raise（不可信输入 → 失败 outcome）。
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

impl EffectIter for WasmTaskIter {
    fn next(&mut self) -> Step {
        // 注入依赖同步（guest 的 get 读核心 store 的当前值）。
        self.sync_injected();
        // 驱动 guest 一步（同步；guard 在核心 execute 的步界检查）。
        // M2-PR1（L-Raise）：guest trap（wasmtime 错误）不再是宿主 panic
        // ——以 FiberError 载荷 raise，核心 reload 捕获后记录为 fiber 的
        // 失败 outcome（§4.3.4 𝔈fail）。
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
        match step {
            Some(effect_step) if !effect_step.done => Step::Yielded(step_inverse),
            Some(_) => Step::Finished(step_inverse),
            None => Step::Finished(Box::new(|| {}) as Box<dyn FnOnce()>),
        }
    }
}
