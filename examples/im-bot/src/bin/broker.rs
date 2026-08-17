//! 服务代理 broker 示例（论文 §6.2，处置⑦，M3-PR1b）。
//!
//! §6.2 的服务代理形态（服务复用——与 exclusive binding 对照，吸收其
//! 扰动）：
//!
//! > "Service broker: a central service that acts as the entrypoint for the
//! > interface **is injected by both** the backing providers and the
//! > consumers, so that multiple providers coexist and the broker dispatches
//! > each request among them. Compared to exclusive binding, the broker
//! > absorbs this perturbation: **updating a backing provider leaves the
//! > broker in place**, so consumers see no change to their dependency and
//! > no reload is triggered."
//!
//! > "…each provider **registers with the broker** through a revertible
//! > effect, so unloading it **reverts the registration and drops it from
//! > the broker's routing set automatically**."
//!
//! 依赖方向：**broker（中央服务）是后备提供者与消费者共同注入的依赖**——
//! broker 提供注册句柄（`reg`）与请求入口（`service`）两个键；后备提供者
//! 注入 `reg`、经**可逆效应**向 broker 路由集注册（`ctx.effect` 追踪逆：
//! 卸载自动撤销注册）；消费者注入 `service`，每次请求由 broker 按路由集
//! 分发（本示例用确定性策略：字典序最小者优先）。
//!
//! 演示：
//! 1. 两后备（a1/b1）+ broker + 消费者全激活；broker 路由集含两后备；
//! 2. **更新后备提供者不扰动消费者**（§6.2 "updating a backing provider
//!    leaves the broker in place … no reload is triggered"）：重建后备 b →
//!    broker **不重执行**（fiber 不变、注册键重绑）、消费者**效应不重执行**
//!    （全程 Active、fiber 不变）——只有 b 的注册先撤销再重挂；
//! 3. **卸载后备 = 可逆注册自动撤销**（§6.2 "unloading it reverts the
//!    registration and drops it from the broker's routing set
//!    automatically"）：移除 a1 → broker 与消费者**无感**（保持 Active、
//!    service 绑定不变），仅路由集失去 a1、分发转向 b1；
//! 4. **重注册自动恢复**：a 重新装载 → 注册恢复、路由集自动含回该后备。
//!
//! 运行：`cargo run -p im-bot --bin broker`（全部断言通过即成功）。

use cordis::{Context, EffectIter, FiberState, Key, Runtime, component};
use cordis_core::symbol::Symbol;
use cordis_loader::{Entry, Loader};
use std::any::Any;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// ── 效应重执行计数器（config 载体）────────────────────────────────────

/// 包在 config 里传给组件；`apply_impl` 每次执行 +1。用它**直证**：
/// 消费者/ broker 的效应在"更新/卸载后备"期间没有重执行（§6.2
/// "no reload is triggered"）。
#[derive(Clone)]
struct ExecCount(Rc<Cell<u64>>);

// ── 键 ────────────────────────────────────────────────────────────────

/// broker 提供的注册句柄（后备提供者注入；`Value` 需 `Send + Sync`，
/// 故共享路由集用 `Arc<Mutex<..>>`）。
struct RegKey;
impl Key for RegKey {
    type Value = RegHandle;
    const SYMBOL: &'static str = "reg";
}

/// broker 提供的请求入口（消费者注入）。
struct ServiceKey;
impl Key for ServiceKey {
    type Value = ServiceVal;
    const SYMBOL: &'static str = "service";
}

// ── broker 路由集与注册/分发句柄 ─────────────────────────────────────

/// broker 内部路由集（`BTreeSet`：字典序稳定，分发确定性）。
#[derive(Clone)]
struct RegHandle {
    route: Arc<Mutex<BTreeSet<String>>>,
}

impl RegHandle {
    /// 可逆注册（§6.2 "registers with the broker through a revertible
    /// effect"）：`id` 入路由集；返回的 disposer 撤销注册（集内移除）——
    /// 经 [`Context::effect`] 追踪后，后备卸载时自动执行。
    fn register(&self, id: String) -> cordis::Disposer {
        self.route.lock().expect("route").insert(id.clone());
        let route = Arc::clone(&self.route);
        Box::new(move || {
            route.lock().expect("route").remove(&id);
        })
    }
}

/// broker 的服务值：按路由集分发每次请求（确定性：字典序最小者优先，
/// 相当于 round-robin 之外的显式选择策略）。
#[derive(Clone)]
struct ServiceVal {
    route: Arc<Mutex<BTreeSet<String>>>,
}

impl ServiceVal {
    /// 分发一次请求：当前路由集里字典序最小的后备（无后备时 None——
    /// 此时消费者本就不会激活，其状态由场景断言观测）。
    fn dispatch(&self) -> Option<String> {
        self.route.lock().expect("route").first().cloned()
    }

    /// 路由集快照（供断言观测"哪个后备在册"）。
    fn routes(&self) -> Vec<String> {
        self.route.lock().expect("route").iter().cloned().collect()
    }
}

// ── broker（provide 注册句柄 + 服务入口）──────────────────────────────

/// 中央服务：不注入任何后备注册键（不因单一后备的增删而重载），只维护
/// 路由集并提供入口。
#[component(inject = [], provide = [RegKey, ServiceKey])]
struct Broker;

impl Broker {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let count = config
            .downcast_ref::<ExecCount>()
            .expect("ExecCount")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            count.0.set(count.0.get() + 1);
            // 共享路由集：注册句柄与服务分发读同一份状态。
            let route = Arc::new(Mutex::new(BTreeSet::new()));
            // 两绑定经 ctx.set 的可逆 disposer 均已入上下文累加器（armed
            // 至多生效一次）；首个句柄显式弃置（`set` 返回的重复句柄）。
            let _reg = ctx
                .set::<RegKey>(RegHandle {
                    route: Arc::clone(&route),
                })
                .expect("绑定 reg 注册句柄");
            ctx.set::<ServiceKey>(ServiceVal { route })
                .expect("绑定 service 入口")
        })))
    }
}

// ── 后备提供者（inject 注册句柄，经可逆效应注册）────────────────────

/// 后备实现（config = 实现标识）。**注入** broker 的注册句柄并注册自己；
/// 不提供任何键。
#[component(inject = [RegKey], provide = [])]
struct Backing;

impl Backing {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let id = config
            .downcast_ref::<String>()
            .expect("config 为 String")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            let handle = ctx.get::<RegKey>().expect("注册句柄可用").clone();
            // 可逆注册：`ctx.effect` 把注册的逆（撤销注册）推进上下文累加
            // 器——后备卸载/退役时自动执行，路由集自动失去该后备。
            // 注册的逆已由 `ctx.effect` 推入上下文累加器（卸载自动撤销）；
            // 返回的重复句柄显式弃置。
            let _reg_effect =
                ctx.effect(|| Box::new(cordis::once(Box::new(move || handle.register(id)))));
            Box::new(|| {}) as cordis::Disposer
        })))
    }
}

// ── 消费者（注入 service，探针 + 分发一次请求）──────────────────────

/// 消费者：不读不写任何值，仅凭 `inject = [ServiceKey]` 把自身激活态作为
/// "服务是否可用"的探针，供断言观测扰动；激活时向 broker 分发一次请求
/// （演示"broker dispatches each request among them"）。
#[component(inject = [ServiceKey], provide = [])]
struct Consumer;

impl Consumer {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let count = config
            .downcast_ref::<ExecCount>()
            .expect("ExecCount")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            count.0.set(count.0.get() + 1);
            let svc = ctx.get::<ServiceKey>().expect("服务可用").clone();
            let _ = svc.dispatch(); // 一次请求（路由结果仅作演示）
            Box::new(|| {}) as cordis::Disposer
        })))
    }
}

// ── 工具 ──────────────────────────────────────────────────────────────

fn entry(id: &str, component: &str, config: &str) -> Entry {
    Entry::new(id, component, Rc::new(config.to_string()), 0, false)
}

/// 读取当前 service 入口（断言路由集状态）。
fn service(runtime: &Runtime) -> ServiceVal {
    let store = runtime.store();
    store
        .get_value(Symbol::intern("service"))
        .expect("service 绑定")
        .downcast_ref::<ServiceVal>()
        .expect("ServiceVal")
        .clone()
}

fn assert_active(loader: &Loader, id: &str, what: &str) {
    assert!(
        matches!(
            &*loader.fiber(id).expect(what).state(),
            FiberState::Active { .. }
        ),
        "{what}：应保持 Active"
    );
}

fn main() {
    let runtime = Rc::new(Runtime::new());
    let loader = Loader::new(Rc::clone(&runtime));
    loader.register_component("a", Rc::new(Backing));
    loader.register_component("b", Rc::new(Backing));
    loader.register_component("broker", Rc::new(Broker));
    loader.register_component("consumer", Rc::new(Consumer));

    let broker_count = ExecCount(Rc::new(Cell::new(0)));
    let consumer_count = ExecCount(Rc::new(Cell::new(0)));
    let broker_entry = Entry::new("br", "broker", Rc::new(broker_count.clone()), 0, false);
    let consumer_entry = Entry::new("c", "consumer", Rc::new(consumer_count.clone()), 0, false);

    // ── 1. 装配：两后备 + broker + 消费者全激活 ─────────────────────
    loader.apply(&[
        entry("a1", "a", "impl-1"),
        entry("b1", "b", "impl-2"),
        broker_entry.clone(),
        consumer_entry.clone(),
    ]);
    let broker_first = loader.fiber("br").expect("broker 激活").id();
    let consumer_first = loader.fiber("c").expect("消费者激活").id();
    assert_eq!(
        service(&runtime).routes(),
        vec!["impl-1", "impl-2"],
        "装配：两后备均在路由集"
    );
    assert_eq!(
        service(&runtime).dispatch(),
        Some("impl-1".to_string()),
        "broker 分发（字典序最小者优先）"
    );
    assert_eq!(broker_count.0.get(), 1, "broker 效应恰执行 1 次");
    assert_eq!(consumer_count.0.get(), 1, "消费者效应恰执行 1 次");
    assert!(runtime.is_quiet(), "装配静止");

    // ── 2. 更新后备提供者不扰动消费者（§6.2）───────────────────────
    // 重建后备 b（revision 0→1 → 卸载→重装）：b 的注册先撤销再重挂
    //（路由集成员 impl-2 → impl-2'）；broker 与消费者**全程不受扰动**
    //（§6.2 "updating a backing provider leaves the broker in place, so
    // consumers see no change to their dependency and no reload is
    // triggered"）——效应不重执行（计数不变）、fiber 不变、service 绑定
    // 未变。
    loader.apply(&[
        entry("a1", "a", "impl-1"),
        Entry::new("b1", "b", Rc::new("impl-2'".to_string()), 1, false),
        broker_entry.clone(),
        consumer_entry.clone(),
    ]);
    assert_eq!(
        service(&runtime).routes(),
        vec!["impl-1", "impl-2'"],
        "b 更新：注册撤销并重挂（路由集成员替换）"
    );
    assert_eq!(
        service(&runtime).dispatch(),
        Some("impl-1".to_string()),
        "broker 分发不变（a1 仍在）"
    );
    assert_eq!(broker_count.0.get(), 1, "broker 未重执行（更新后备无感）");
    assert_eq!(
        consumer_count.0.get(),
        1,
        "消费者未重执行（无 reload——全程 Active）"
    );
    assert_eq!(
        loader.fiber("br").expect("broker").id(),
        broker_first,
        "broker fiber 不变"
    );
    assert_eq!(
        loader.fiber("c").expect("消费者").id(),
        consumer_first,
        "消费者 fiber 不变"
    );
    assert_active(&loader, "br", "broker");
    assert_active(&loader, "c", "消费者");
    assert!(runtime.is_quiet(), "更新后备后静止");

    // ── 3. 卸载后备 = 可逆注册自动撤销（§6.2）───────────────────────
    // 退役并移除 a1：后备的注册逆自动执行 → 路由集失去 impl-1 → 分发
    // 转向 impl-2'——broker 与消费者**无感**（保持 Active、service 绑定
    // 不变），无需任何手动注销。
    let a1 = loader.fiber("a1").expect("后备 a1").clone();
    a1.retire();
    loader.apply(&[
        Entry::new("b1", "b", Rc::new("impl-2'".to_string()), 1, false),
        broker_entry.clone(),
        consumer_entry.clone(),
    ]);
    assert_eq!(
        service(&runtime).routes(),
        vec!["impl-2'"],
        "卸载 a1 → 可逆注册自动撤销（路由集失去 impl-1）"
    );
    assert_eq!(
        service(&runtime).dispatch(),
        Some("impl-2'".to_string()),
        "分发转向剩余后备"
    );
    assert_eq!(broker_count.0.get(), 1, "broker 未重执行（卸载后备无感）");
    assert_eq!(consumer_count.0.get(), 1, "消费者未重执行、未离开 Active");
    assert_active(&loader, "br", "broker");
    assert_active(&loader, "c", "消费者");
    assert!(runtime.is_quiet(), "卸载后备后静止");

    // ── 4. 重注册自动恢复（路由集更新）──────────────────────────────
    // 后备 a 重新装载（新条目 a2）：注册恢复 → 路由集自动含回该后备。
    loader.apply(&[
        entry("a2", "a", "impl-3"),
        Entry::new("b1", "b", Rc::new("impl-2'".to_string()), 1, false),
        broker_entry.clone(),
        consumer_entry.clone(),
    ]);
    assert_eq!(
        service(&runtime).routes(),
        vec!["impl-2'", "impl-3"],
        "a 重注册 → 路由集自动恢复（含回 impl-3）"
    );
    assert_eq!(
        service(&runtime).dispatch(),
        Some("impl-2'".to_string()),
        "分发（字典序最小者：impl-2' < impl-3）"
    );
    assert_eq!(broker_count.0.get(), 1, "broker 仍未重执行");
    assert_eq!(consumer_count.0.get(), 1, "消费者仍未重执行");
    assert_active(&loader, "br", "broker");
    assert_active(&loader, "c", "消费者");
    assert!(runtime.is_quiet(), "重注册后静止");

    println!(
        "✓ im-bot broker：服务代理案例全部断言通过（更新/卸载后备不扰动 broker 与消费者 / 可逆注册自动撤销（仅路由集移除）/ 重注册自动恢复）"
    );
}
