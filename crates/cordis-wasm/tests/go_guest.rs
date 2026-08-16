//! PR #14：Go guest（标准 go + wasip1，经预览1 适配器组件化）与 Rust/native
//! 后端互操作——M1 门禁 3/3 的**双语言验收**：同一 `Loader`/`Runtime`
//! 可加载 Rust 与 Go 两种语言实现的组件，行为一致（db 消费者 → derived 提供者，
//! 与 examples/wasm-plugin-rust-consumer 语义相同）。
//!
//! Go guest 构建管线（CI 步骤 build wasm guest (Go)）：
//! ```text
//! cd examples/wasm-plugin-go
//! GOOS=wasip1 GOARCH=wasm go build -o guest-core.wasm .
//! cargo run -q -p componentize -- guest-core.wasm \
//!     ../../../third_party/wasi-preview1-adapter/wasi_snapshot_preview1.reactor.wasm \
//!     ../../../crates/cordis-wasm/wit cordis guest.wasm
//! ```

use cordis_core::effect::{EffectIter, once};
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{Component, FiberState, Runtime};
use cordis_loader::{Entry, Loader};
use cordis_wasm::WasmComponent;
use cordis_wasm::wit::cordis::core::context::Value;
use std::rc::Rc;

/// Go guest 是组件产物（core 模块已组件化），路径在示例根目录。
fn go_guest_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest).join("../../examples/wasm-plugin-go/guest.wasm")
}

fn load_rust_guest(example: &str) -> Rc<WasmComponent> {
    let engine = wasmtime::Engine::default();
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest).join(format!(
        "../../examples/{example}/target/wasm32-wasip2/debug/{}.wasm",
        example.replace('-', "_")
    ));
    let bytes = std::fs::read(path).expect("先构建 guest（CI 步骤 build wasm guest）");
    WasmComponent::load(&engine, &bytes).expect("组件加载")
}

fn load_go_guest() -> Rc<WasmComponent> {
    let engine = wasmtime::Engine::default();
    let bytes =
        std::fs::read(go_guest_path()).expect("先构建 Go guest（CI 步骤 build wasm guest (Go)）");
    WasmComponent::load(&engine, &bytes).expect("Go 组件加载")
}

fn spec(names: &[&str]) -> KeySet {
    names.iter().map(|s| Symbol::intern(s)).collect()
}

/// 原生提供者：提供 `db`（wit Value 装箱，与 wasm 组件值类型统一）。
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

fn derived_value(runtime: &Runtime) -> Value {
    let store = runtime.store();
    store
        .get_value(Symbol::intern("derived"))
        .expect("derived 已绑定")
        .downcast_ref::<Value>()
        .expect("wit Value")
        .clone()
}

#[test]
fn go_consumer_reads_rust_provider_value() {
    let runtime = Rc::new(Runtime::new());
    let loader = Loader::new(Rc::clone(&runtime));
    loader.register_component("rust-db", load_rust_guest("wasm-plugin-rust"));
    loader.register_component("go-consumer", load_go_guest());

    loader.apply(&[entry("db", "rust-db"), entry("cons", "go-consumer")]);
    let db_fiber = loader.fiber("db").expect("Rust provider 已加载");
    let cons_fiber = loader.fiber("cons").expect("Go consumer 已加载");
    assert!(
        matches!(&*db_fiber.state(), FiberState::Active { .. }),
        "Rust provider 激活"
    );
    assert!(
        matches!(&*cons_fiber.state(), FiberState::Active { .. }),
        "Go consumer 激活（注入满足）"
    );

    // Go guest 经 context::get 读到注入值并派生 derived(wasm-pg)。
    let derived = derived_value(&runtime);
    assert!(
        matches!(derived, Value::Text(ref v) if v == "derived(wasm-pg)"),
        "Go consumer 读 Rust 提供值：{derived:?}"
    );

    // 退役 provider → Go consumer 级联停用 → 绑定全清。
    db_fiber.retire();
    assert!(
        matches!(&*cons_fiber.state(), FiberState::Inactive(_)),
        "provider 退役 → Go consumer 级联停用"
    );
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
    assert!(runtime.is_quiet(), "回到静止");
}

#[test]
fn go_consumer_reads_native_provider_value() {
    let runtime = Rc::new(Runtime::new());
    let loader = Loader::new(Rc::clone(&runtime));
    loader.register_component("native-db", Rc::new(NativeProvider) as Rc<dyn Component>);
    loader.register_component("go-consumer", load_go_guest());

    loader.apply(&[entry("db", "native-db"), entry("cons", "go-consumer")]);
    let db_fiber = loader.fiber("db").expect("原生 provider 已加载");
    let cons_fiber = loader.fiber("cons").expect("Go consumer 已加载");
    assert!(
        matches!(&*db_fiber.state(), FiberState::Active { .. }),
        "原生 provider 激活"
    );
    assert!(
        matches!(&*cons_fiber.state(), FiberState::Active { .. }),
        "Go consumer 激活（注入满足）"
    );

    // Go guest 消费原生提供的值（值类型统一：两边都走 wit Value）。
    let derived = derived_value(&runtime);
    assert!(
        matches!(derived, Value::Text(ref v) if v == "derived(native-pg)"),
        "Go 消费原生值：{derived:?}"
    );

    // 移除原生 provider → Go consumer 级联停用。
    loader.apply(&[entry("cons", "go-consumer")]);
    assert!(
        matches!(&*cons_fiber.state(), FiberState::Inactive(_)),
        "依赖消失 → Go consumer 级联停用"
    );
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
    assert!(runtime.is_quiet(), "回到静止");
}
