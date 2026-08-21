//! PR #13 双后端共存：同一 `Loader`/`Runtime` 同时加载**原生**与 **wasm**
//! 组件（M1 门禁：同一 loader 加载原生与 wasm 组件）。
//!
//! 值类型统一（PR #13 决策 → P-2 下沉）：原生组件经
//! `Context::set_dyn`/`get_dyn` 使用与 wasm 相同的统一类型
//! **`cordis_value::Value`**（P-2 产品验证线：值类型下沉独立 crate，
//! 依赖方向"原生→cordis-value"；跨类型值翻译边界已消除）——双方都走
//! 动态值 API 即可互通（双向测试直证）。

use cordis_core::effect::{EffectIter, once};
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{Component, FiberState, Runtime};
use cordis_loader::{Entry, Loader};
use cordis_wasm::Value;
use cordis_wasm::WasmComponent;
use std::rc::Rc;

fn load_guest(example: &str) -> Rc<WasmComponent> {
    let engine = wasmtime::Engine::default();
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest).join(format!(
        "../../examples/{example}/target/wasm32-wasip2/debug/{}.wasm",
        example.replace('-', "_")
    ));
    let bytes = std::fs::read(path).expect("先构建 guest（CI 步骤 build wasm guest）");
    WasmComponent::load(&engine, &bytes).expect("组件加载")
}

fn spec(names: &[&str]) -> KeySet {
    names.iter().map(|s| Symbol::intern(s)).collect()
}

/// 原生消费者：注入 `db`（经 `get_dyn` 读 wit Value），提供 `derived`。
struct NativeConsumer;

impl Component for NativeConsumer {
    fn inject(&self) -> KeySet {
        spec(&["db"])
    }
    fn provide(&self) -> KeySet {
        spec(&["derived"])
    }
    fn apply(
        &self,
        ctx: Rc<cordis_core::Context>,
        _config: &dyn std::any::Any,
    ) -> Box<dyn EffectIter> {
        Box::new(once(Box::new(move || {
            // 跨后端注入读取：wasm 提供者的值是 wit Value 装箱。
            // `get_dyn` 的 `Ref` 是 Drop 类型、借用活到作用域末——
            // 必须块级隔离（`set_dyn` 会 borrow store）。
            let db = {
                let db = ctx.get_dyn(Symbol::intern("db")).expect("注入的 db 可读");
                let db = db
                    .downcast_ref::<Value>()
                    .expect("wasm 提供者的值为 wit Value");
                db.clone()
            };
            let Value::Text(db) = db else {
                panic!("db 应为文本值");
            };
            ctx.set_dyn(
                Symbol::intern("derived"),
                Box::new(Value::Text(format!("native({db})"))),
            )
            .expect("绑定 derived")
        })))
    }
}

/// 原生提供者：提供 `db`（wit Value 装箱——与 wasm 组件值类型统一）。
struct NativeProvider;

impl Component for NativeProvider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        spec(&["db"])
    }
    fn apply(
        &self,
        ctx: Rc<cordis_core::Context>,
        _config: &dyn std::any::Any,
    ) -> Box<dyn EffectIter> {
        Box::new(once(Box::new(move || {
            ctx.set_dyn(
                Symbol::intern("db"),
                Box::new(Value::Text("native-pg".into())),
            )
            .expect("绑定 db")
        })))
    }
}

fn entry(id: &str, component: &str) -> Entry {
    Entry::new(id, component, Rc::new(()), 1, false)
}

#[test]
fn loader_loads_native_and_wasm_together() {
    let runtime = Rc::new(Runtime::new());
    let loader = Loader::new(Rc::clone(&runtime));
    loader.register_component("wasm-db", load_guest("wasm-plugin-rust"));
    loader.register_component(
        "native-consumer",
        Rc::new(NativeConsumer) as Rc<dyn Component>,
    );

    loader.apply(&[entry("db", "wasm-db"), entry("cons", "native-consumer")]);
    let db_fiber = loader.fiber("db").expect("wasm provider 已加载");
    let cons_fiber = loader.fiber("cons").expect("原生 consumer 已加载");
    assert!(
        matches!(&*db_fiber.state(), FiberState::Active { .. }),
        "wasm provider 激活"
    );
    assert!(
        matches!(&*cons_fiber.state(), FiberState::Active { .. }),
        "原生 consumer 激活（注入满足）"
    );

    // 值互通：wasm 提供 db（wit Value）→ 原生消费（get_dyn）→ derived。
    let derived = {
        let store = runtime.store();
        store
            .get_value(Symbol::intern("derived"))
            .expect("derived 已绑定")
            .downcast_ref::<Value>()
            .expect("cordis_value::Value")
            .clone()
    };
    assert!(
        matches!(derived, Value::Text(ref v) if v == "native(wasm-pg)"),
        "原生消费 wasm 值：{derived:?}"
    );

    // 移除 wasm provider 条目 → 原生 consumer 级联停用、绑定全清。
    loader.apply(&[entry("cons", "native-consumer")]);
    assert!(loader.fiber("db").is_none(), "wasm 条目已移除");
    assert!(
        matches!(&*cons_fiber.state(), FiberState::Inactive(_)),
        "依赖消失 → 原生 consumer 级联停用"
    );
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
    assert!(runtime.is_quiet(), "静止");
}

#[test]
fn wasm_consumer_reads_native_provider_value() {
    let runtime = Rc::new(Runtime::new());
    let loader = Loader::new(Rc::clone(&runtime));
    loader.register_component("native-db", Rc::new(NativeProvider) as Rc<dyn Component>);
    loader.register_component("wasm-consumer", load_guest("wasm-plugin-rust-consumer"));

    loader.apply(&[entry("db", "native-db"), entry("cons", "wasm-consumer")]);
    let db_fiber = loader.fiber("db").expect("原生 provider 已加载");
    let cons_fiber = loader.fiber("cons").expect("wasm consumer 已加载");
    assert!(
        matches!(&*db_fiber.state(), FiberState::Active { .. }),
        "原生 provider 激活"
    );
    assert!(
        matches!(&*cons_fiber.state(), FiberState::Active { .. }),
        "wasm consumer 激活（注入满足）"
    );

    // wasm consumer 读到原生提供的值并派生 derived(native-pg)。
    let derived = {
        let store = runtime.store();
        store
            .get_value(Symbol::intern("derived"))
            .expect("derived 已绑定")
            .downcast_ref::<Value>()
            .expect("cordis_value::Value")
            .clone()
    };
    assert!(
        matches!(derived, Value::Text(ref v) if v == "derived(native-pg)"),
        "wasm 消费原生值（值类型统一）：{derived:?}"
    );

    // 移除原生 provider → wasm consumer 级联停用。
    loader.apply(&[entry("cons", "wasm-consumer")]);
    assert!(
        matches!(&*cons_fiber.state(), FiberState::Inactive(_)),
        "依赖消失 → wasm consumer 级联停用"
    );
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
}
