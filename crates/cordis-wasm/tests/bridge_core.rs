//! PR #11：WasmComponent 接入 cordis-core——guest 的 set 转发进核心
//! store（ADR-0004 值装箱）、逆经 rep 撤销、retire 后绑定清除。

use cordis_core::Component;
use cordis_core::runtime::Runtime;
use cordis_core::symbol::Symbol;
use cordis_wasm::WasmComponent;
use std::rc::Rc;

/// 定位预编译 guest 组件（独立 crate 的 target 目录）。
fn guest_wasm_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .join("../../examples/wasm-plugin-rust/target/wasm32-wasip2/debug/wasm_plugin_rust.wasm")
}

fn load_component() -> Rc<WasmComponent> {
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm_path()).expect("先构建 guest：cargo build --manifest-path examples/wasm-plugin-rust/Cargo.toml --target wasm32-wasip2");
    WasmComponent::load(&engine, &bytes).expect("组件加载")
}

/// 断言核心 store 在 `realm` 有绑定（公开 `contains`）。
fn core_bound(runtime: &Rc<Runtime>, key: &str) -> bool {
    runtime.store().contains(Symbol::intern(key))
}

#[test]
fn wasm_component_activates_in_core_runtime() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let comp = load_component();

    // 注册（O-Insert + Def 47）：无依赖 → 立即激活。
    let fiber = root
        .use_component(
            Rc::clone(&comp) as Rc<dyn cordis_core::Component>,
            Rc::new(()),
        )
        .expect("实例化 wasm 组件");
    assert!(
        matches!(&*fiber.state(), cordis_core::FiberState::Active { .. }),
        "wasm 组件应激活"
    );

    // guest 的 set 已转发进核心 store（realm = "db"）。
    assert!(core_bound(&runtime, "db"), "核心 store 有 db 绑定");
    // 绑定镜像（Host 侧，guest 的 get 读取）同步可见。
    let mirror = comp.bindings();
    assert!(
        matches!(mirror.get("db"), Some(cordis_wasm::Value::Text(v)) if v == "wasm-pg"),
        "镜像值为 wasm-pg：{mirror:?}"
    );

    // σγ：Active fiber 提供的绑定计入。
    let provided = runtime.provided();
    assert!(
        provided.contains(Symbol::intern("db")),
        "σγ 含 db：{provided:?}"
    );

    // 退役 → 级联卸载 → 核心逆（unbind + notify）→ 绑定清除。
    fiber.retire();
    assert!(
        runtime.store().symbols().next().is_none(),
        "退役后绑定全部恢复"
    );
    assert!(runtime.is_quiet(), "回到静止");
}

#[test]
fn wasm_component_inject_provide_declared() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let comp = load_component();

    let inject = comp.inject();
    let provide = comp.provide();
    assert!(inject.is_empty(), "db 提供者无依赖");
    // A2 扩展提供键：db + 远端探针写端（probe/probe_err）——宽松断言
    //（含核心键即可，探针键演进不碎断言）。
    let names: Vec<_> = provide.iter().map(|s| s.as_str()).collect();
    for required in ["db", "probe", "probe_err"] {
        assert!(
            names.contains(&required),
            "供给声明跨边界缺 {required}: {names:?}"
        );
    }
    let _ = root;
}
