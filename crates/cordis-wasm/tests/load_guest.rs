//! PR #10 端到端：加载 Rust guest 组件（examples/wasm-plugin-rust）→
//! constructor/inject/provide/start/step → 宿主绑定 + 逆撤销。

use cordis_wasm::EffectStep;
use cordis_wasm::Host;
use cordis_wasm::wit;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};
use wit::cordis::core::context::Value;

/// 定位预编译 guest 组件（独立 crate 的 target 目录）。
fn guest_wasm_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .join("../../examples/wasm-plugin-rust/target/wasm32-wasip2/debug/wasm_plugin_rust.wasm")
}

fn load() -> anyhow::Result<(Store<Host>, wit::Cordis)> {
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    let mut store = Store::new(&engine, Host::new());
    wit::Cordis::add_to_linker::<_, HasSelf<_>>(&mut linker, |host| host)?;
    let component = Component::from_file(&engine, guest_wasm_path())?;
    let instance = wit::Cordis::instantiate(&mut store, &component, &linker)?;
    Ok((store, instance))
}

#[test]
fn guest_activates_and_binds_via_host_context() -> anyhow::Result<()> {
    let (mut store, instance) = load()?;
    let comp = instance.cordis_core_plugin().component();

    // Algorithm 4 的 use：constructor → 组件实例。
    let component_any = comp.call_constructor(&mut store)?;

    // Def 43：声明核对（跨边界 (d, p)）。
    let inject = comp.call_inject(&mut store, component_any)?;
    let provide = comp.call_provide(&mut store, component_any)?;
    assert!(inject.is_empty(), "db 提供者无依赖：{inject:?}");
    // C2：提供键含远端探针写端（排序后断言）。
    // A2：提供键含远端探针写端——宽松断言（含核心键即可）。
    for required in ["db", "probe", "probe_err"] {
        assert!(
            provide.iter().any(|s| s == required),
            "供给声明缺 {required}: {provide:?}"
        );
    }

    // Algorithm 5 的 reload：start → task，宿主驱动 step。
    let task_any = comp.call_start(&mut store, component_any)?;
    let task = instance.cordis_core_plugin().task();
    let step = task.call_step(&mut store, task_any)?;

    // 激活效应：guest 经 context::set 绑定 db = "wasm-pg"（A2 多步：step0
    // 提交远端后 done:false——等待步由宿主 Await/advance 驱动，见 a2_e2e；
    // 此处断言 step0 副作用与逆（Option）形态）。
    let step = step.expect("第一步应有产出");
    assert!(
        matches!(step, EffectStep::Step(_)),
        "step0 为有逆步（db 绑定，A2b variant）"
    );
    let binding = store.data().bindings().get("db").cloned();
    assert!(
        matches!(binding, Some(Value::Text(ref v)) if v == "wasm-pg"),
        "宿主侧绑定：{binding:?}"
    );

    // 等待步（未完成）：再 step 产出 none 逆的 effect-step（等待远端，
    // A2 Await 语义；完整收敛见 a2_e2e 测试）。
    let next = task.call_step(&mut store, task_any)?;
    assert!(
        matches!(&next, Some(EffectStep::Wait)),
        "A2b 等待步（variant wait；完整收敛见 a2_e2e）"
    );

    // 逆撤销路径由 tests/bridge_core.rs 覆盖（WasmComponent 接入核心
    // Runtime：rep → 核心逆，retire 级联清除）。
    Ok(())
}
