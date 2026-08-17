//! 服务代理 broker 示例（论文 §6.2，处置⑦，M3-PR1b）。
//!
//! §6.2 的服务代理形态：中央服务作为接口入口，被后备提供者与消费者
//! 共同注入；多提供者共存，broker 分发请求。可逆效应注册——每个后备
//! 提供者经 `ctx.set` 注册（绑定 + 逆），卸载即撤销注册并自动移出
//! 路由集（"each provider registers with the broker through a revertible
//! effect, so unloading it reverts the registration and drops it from the
//! broker's routing set automatically"）。
//!
//! 演示：
//! 1. 两后备提供者（reg-a / reg-b 各自不同注册键）+ broker（注入两者、
//!    提供 service）+ 消费者（注入 service）——全部激活；
//! 2. **更新后备提供者不扰动消费者**（§6.2 "updating a backing provider
//!    leaves the broker in place, so consumers see no change to their
//!    dependency and no reload is triggered"）：替换 reg-b 的后备 →
//!    broker 选择不变（reg-a 仍在）→ 消费者 fiber 不变；
//! 3. **卸载后备 = 撤销注册**：退役 reg-a 的后备 → broker 重新选择
//!    reg-b → 消费者仍激活（service 键始终由 broker 提供）。
//!
//! 运行：`cargo run -p im-bot --bin broker`（全部断言通过即成功）。

use cordis::{Context, EffectIter, FiberState, Key, Runtime, component};
use cordis_core::symbol::Symbol;
use cordis_loader::{Entry, Loader};
use std::any::Any;
use std::rc::Rc;

// ── 键 ────────────────────────────────────────────────────────────────

/// 后备提供者的注册键（各自不同——供给不相交）。
struct RegAKey;
impl Key for RegAKey {
    type Value = String;
    const SYMBOL: &'static str = "reg-a";
}

struct RegBKey;
impl Key for RegBKey {
    type Value = String;
    const SYMBOL: &'static str = "reg-b";
}

/// broker 提供的服务键（消费者注入）。
struct ServiceKey;
impl Key for ServiceKey {
    type Value = String;
    const SYMBOL: &'static str = "service";
}

// ── 后备提供者（注册键提供者；config = 实现标识）──────────────────────

/// 注册 `reg-a` 的后备实现（config = 标识）。
#[component(inject = [], provide = [RegAKey])]
struct BackingA;

impl BackingA {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let id = config
            .downcast_ref::<String>()
            .expect("config 为 String")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            ctx.set::<RegAKey>(id).expect("注册 reg-a")
        })))
    }
}

/// 注册 `reg-b` 的后备实现（config = 标识）。
#[component(inject = [], provide = [RegBKey])]
struct BackingB;

impl BackingB {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let id = config
            .downcast_ref::<String>()
            .expect("config 为 String")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            ctx.set::<RegBKey>(id).expect("注册 reg-b")
        })))
    }
}

// ── broker（注入两注册键，提供 service）───────────────────────────────

/// 服务代理：选择第一个可用的后备实现（简单选择策略），绑定 service。
#[component(inject = [RegAKey, RegBKey], provide = [ServiceKey])]
struct Broker;

impl Broker {
    fn apply_impl(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(cordis::once(Box::new(move || {
            // 选择策略：reg-a 优先（若有），否则 reg-b。
            let selected = ctx
                .get::<RegAKey>()
                .map(|v| format!("via({v})"))
                .unwrap_or_else(|_| {
                    let b = ctx.get::<RegBKey>().expect("至少一个后备可用");
                    format!("via({b})")
                });
            ctx.set::<ServiceKey>(selected).expect("绑定 service")
        })))
    }
}

// ── 消费者（注入 service，读时访问）──────────────────────────────────

#[component(inject = [ServiceKey], provide = [])]
struct Consumer;

impl Consumer {
    fn apply_impl(&self, _ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(cordis::once(Box::new(|| {
            Box::new(|| {}) as cordis::Disposer
        })))
    }
}

fn entry(id: &str, component: &str, config: &str) -> Entry {
    Entry::new(id, component, Rc::new(config.to_string()), 0, false)
}

fn service_of(runtime: &Runtime) -> String {
    let store = runtime.store();
    store
        .get_value(Symbol::intern("service"))
        .expect("service 绑定")
        .downcast_ref::<String>()
        .expect("String")
        .clone()
}

fn main() {
    let runtime = Rc::new(Runtime::new());
    let loader = Loader::new(Rc::clone(&runtime));
    loader.register_component("a", Rc::new(BackingA));
    loader.register_component("b", Rc::new(BackingB));
    loader.register_component("broker", Rc::new(Broker));
    loader.register_component("consumer", Rc::new(Consumer));

    // ── 1. 装配：两后备 + broker + 消费者全激活 ─────────────────────
    loader.apply(&[
        entry("a1", "a", "impl-1"),
        entry("b1", "b", "impl-2"),
        entry("br", "broker", ""),
        entry("c", "consumer", ""),
    ]);
    let consumer_first = loader.fiber("c").expect("消费者激活").id();
    assert_eq!(
        service_of(&runtime),
        "via(impl-1)",
        "broker 选择 reg-a（优先）"
    );
    assert!(runtime.is_quiet(), "装配静止");

    // ── 2. 更新后备提供者不扰动消费者（§6.2）───────────────────────
    // 替换 reg-b 的后备（impl-2 → impl-2'）：broker 选择不变（reg-a 仍
    // 在）→ service 绑定值不变 → 消费者 fiber 不变（无 reload）。
    loader.apply(&[
        entry("a1", "a", "impl-1"),
        Entry::new("b1", "b", Rc::new("impl-2'".to_string()), 1, false),
        entry("br", "broker", ""),
        entry("c", "consumer", ""),
    ]);
    assert_eq!(
        service_of(&runtime),
        "via(impl-1)",
        "后备 b 更新 → broker 选择不变"
    );
    assert_eq!(
        loader.fiber("c").expect("消费者").id(),
        consumer_first,
        "更新后备提供者不扰动消费者（无 reload）"
    );
    assert!(runtime.is_quiet(), "更新后备后静止");

    // ── 3. 卸载后备 = 撤销注册（§6.2 可逆效应注册）─────────────────
    // 退役 reg-a 的后备并移除条目：broker 的注册依赖（reg-a）随可逆
    // 效应撤销 → broker 停用（路由集自动失去该后备）→ service 解除 →
    // 消费者级联停用——无需任何手动注销。
    let a1 = loader.fiber("a1").expect("后备 a1").clone();
    a1.retire();
    loader.apply(&[
        Entry::new("b1", "b", Rc::new("impl-2'".to_string()), 1, false),
        entry("br", "broker", ""),
        entry("c", "consumer", ""),
    ]);
    assert!(
        matches!(
            &*loader.fiber("br").expect("broker").state(),
            FiberState::Inactive(_)
        ),
        "后备 a 卸载 → 注册撤销 → broker 停用"
    );
    assert!(
        matches!(
            &*loader.fiber("c").expect("消费者").state(),
            FiberState::Inactive(_)
        ),
        "消费者级联停用（service 解除）"
    );
    let store = runtime.store();
    assert!(
        !store.contains(Symbol::intern("service")),
        "service 已解除（broker 停用）"
    );
    assert!(
        !store.contains(Symbol::intern("reg-a")),
        "reg-a 注册已撤销（可逆效应）"
    );
    assert!(
        store.contains(Symbol::intern("reg-b")),
        "reg-b 注册保留（后备 b1 仍在）"
    );
    drop(store);
    assert!(runtime.is_quiet(), "卸载后备后静止");

    // ── 4. 后备重新注册 → broker 自动恢复（路由集更新）──────────────
    loader.apply(&[
        Entry::new("a2", "a", Rc::new("impl-3".to_string()), 0, false),
        Entry::new("b1", "b", Rc::new("impl-2'".to_string()), 1, false),
        entry("br", "broker", ""),
        entry("c", "consumer", ""),
    ]);
    assert_eq!(
        service_of(&runtime),
        "via(impl-3)",
        "后备 a 重现 → broker 重新路由"
    );
    assert!(
        matches!(
            &*loader.fiber("c").expect("消费者").state(),
            FiberState::Active { .. }
        ),
        "消费者自动恢复"
    );
    assert!(runtime.is_quiet(), "后备重现后静止");

    println!(
        "✓ im-bot broker：服务代理案例全部断言通过（更新不扰动 / 卸载自动撤销注册 / 重注册自动恢复）"
    );
}
