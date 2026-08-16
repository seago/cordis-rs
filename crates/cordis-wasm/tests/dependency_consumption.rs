//! PR #12：wasm 依赖者消费——provider（wasm）激活后，consumer（wasm）
//! 注入 db、经镜像读到核心 store 的值并提供 derived；退役级联停用。

use cordis_core::runtime::Runtime;
use cordis_core::symbol::Symbol;
use cordis_wasm::WasmComponent;
use std::rc::Rc;

/// 定位预编译 guest 组件（独立 crate 的 target 目录）。
fn guest_wasm(example: &str) -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest).join(format!(
        "../../examples/{example}/target/wasm32-wasip2/debug/{}.wasm",
        example.replace('-', "_")
    ))
}

fn load_guest(name: &str) -> Rc<WasmComponent> {
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm(name)).expect("先构建 guest（CI 步骤 build wasm guest）");
    WasmComponent::load(&engine, &bytes).expect("组件加载")
}

#[test]
fn wasm_consumer_reads_injected_provider_value() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    // provider 先激活（绑定 db = "wasm-pg"）。
    let provider = load_guest("wasm-plugin-rust");
    let p_fiber = root
        .use_component(provider, Rc::new(()))
        .expect("provider 实例化");
    assert!(
        matches!(&*p_fiber.state(), cordis_core::FiberState::Active { .. }),
        "provider 应激活"
    );
    assert!(
        runtime.store().contains(Symbol::intern("db")),
        "核心 store 有 db"
    );

    // consumer 注入 db：注入满足 → 激活 → step 读 db → 提供 derived。
    let consumer = load_guest("wasm-plugin-rust-consumer");
    let c_fiber = root
        .use_component(consumer, Rc::new(()))
        .expect("consumer 实例化");
    assert!(
        matches!(&*c_fiber.state(), cordis_core::FiberState::Active { .. }),
        "consumer 应激活（注入满足）"
    );

    // consumer 读到注入值并派生 derived。
    // 注意：`Ref` 是 Drop 类型，借用活到作用域末——必须块级隔离，
    // 否则退役（retire）时逆执行的 borrow_mut 会与它冲突。
    let derived = {
        let store = runtime.store();
        let derived = store
            .get_value(Symbol::intern("derived"))
            .expect("核心 store 有 derived");
        derived
            .downcast_ref::<cordis_wasm::wit::cordis::core::context::Value>()
            .expect("wasm 绑定值为 wit Value")
            .clone()
    };
    assert!(
        matches!(derived, cordis_wasm::wit::cordis::core::context::Value::Text(ref v) if v == "derived(wasm-pg)"),
        "consumer 读注入值并派生：{derived:?}"
    );

    // 退役 provider → consumer 级联停用 → 绑定全清。
    p_fiber.retire();
    assert!(
        matches!(&*c_fiber.state(), cordis_core::FiberState::Inactive(_)),
        "provider 退役 → consumer 级联停用"
    );
    assert!(
        runtime.store().symbols().next().is_none(),
        "退役后绑定全部恢复"
    );
    assert!(runtime.is_quiet(), "回到静止");
}
