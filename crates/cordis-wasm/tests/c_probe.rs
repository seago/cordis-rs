//! C 探针（docs/cordis-wasm-C-PROBE-PLAN.md）：验证 "guest 以远端结果
//! 为输入" 需求强度，零 core 改动（两阶段 + 回注）。
//!
//! C1：宿主回注机制——worker 回填结果经 `ctx.set_dyn` 注入为下一阶段
//! 的输入绑定（注入同步 PR#12 让后续激活的 guest 经 `get` 读到）。

use cordis_async::TokioRemote;
use cordis_core::component::Component;
use cordis_core::runtime::Runtime;
use cordis_core::symbol::Symbol;
use cordis_wasm::Value;
use cordis_wasm::WasmComponent;
use std::rc::Rc;
use std::sync::Arc;

fn guest_wasm_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .join("../../examples/wasm-plugin-rust/target/wasm32-wasip2/debug/wasm_plugin_rust.wasm")
}

/// 轮询 comp 的 remote_result(rep) 直到就绪，返回 Ok(Value)/Err(String)。
/// （C 宿主侧回填驱动——与 remote_e2e 同构，语义不依赖 guest take。）
fn await_remote_value(comp: &Rc<WasmComponent>, rep: u32) -> Value {
    for _ in 0..4000 {
        comp.poll_remotes();
        if let Some(Some(Ok(v))) = comp.remote_result(rep) {
            return v;
        }
        if let Some(Some(Err(e))) = comp.remote_result(rep) {
            panic!("远端结果 err：{e}");
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("远端结果 4000 次轮询未就绪（C1 回填）");
}

/// C1：worker 回填 → 回注为核心 store 的注入键（阶段 2 输入）。
#[test]
fn c1_note_back_injects_result_into_store() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("worker runtime");
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm_path()).expect("先构建 guest");
    let comp = WasmComponent::load(&engine, &bytes).expect("组件加载");
    comp.configure_remote(Some(Rc::new(TokioRemote::new(worker.handle().clone()))));
    comp.register_remote(
        "echo",
        Arc::new(|params: Vec<Value>| {
            let n = match params.first() {
                Some(Value::Count(c)) => *c,
                _ => 0,
            };
            Value::Count(n * 2)
        }),
    );

    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let _fiber = root
        .use_component(Rc::clone(&comp) as Rc<dyn Component>, Rc::new(()))
        .expect("激活");

    // 回填（不断言 worker tid——此测试聚焦回注入，参数 7 → 14）。
    let result = await_remote_value(&comp, 0);
    assert!(matches!(result, Value::Count(14)), "echo(7)→14");

    // 回注到核心 store（注入键 probe_in）——C2 阶段 2 guest 经 get 读回。
    // Disposer 保留（保持回注绑定到测试作用域末——C2 阶段 2 读取期间不撤销）。
    let _keep = root
        .set_dyn(Symbol::intern("probe_in"), Box::new(result.clone()))
        .expect("回注绑定");
    assert!(
        runtime.store().contains(Symbol::intern("probe_in")),
        "回注键在核心 store"
    );

    let _ = worker;
}
